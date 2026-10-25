// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The SID core for M3's stamped writes (core/sid/sid.h): `sid_advance`, which clocks the
//! voices into the next sample so a write lands at its cycle, and the voice mute
//! (`sid_set_mute`), which takes a voice out of the mix and leaves everything else alone.
//!
//! The placement is checked against `sid_clock` with the write at the same cycle (the voices
//! reach the same state, outside hard sync, which `sid_clock` documents), the cycle accounting
//! against the Q16 arithmetic itself, and the mute against a build of the chip whose muted
//! voices output 0 (sidou-sys build.rs, `SID_TEST_ZERO_VOICE`).

use sidou_sys::*;

const RATE: u32 = 44_118;
const MODELS: [sid_model; 2] = [SID_MODEL_6581, SID_MODEL_8580];
const SYNC: u8 = 0x02;

/// xorshift32: the tests' streams, the same on every run.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

fn new_sid(model: sid_model) -> Box<sid> {
    let mut s = Box::<sid>::default();
    unsafe { sid_init(&mut *s, model, SID_CLOCK_PAL, RATE) };
    s
}

fn new_zero_voice(model: sid_model) -> Box<sid> {
    let mut s = Box::<sid>::default();
    unsafe { sid_init_zero_voice(&mut *s, model, SID_CLOCK_PAL, RATE) };
    s
}

fn render(s: &mut sid, n: usize) -> Vec<i16> {
    let mut out = vec![0i16; n];
    unsafe { sid_render(s, out.as_mut_ptr(), n as u32) };
    out
}

fn next_cycles(s: &sid) -> u32 {
    unsafe { sid_next_cycles(s) }
}

fn advance(s: &mut sid, k: u32) {
    unsafe { sid_advance(s, k) }
}

/// A random register write; `sync` allows hard sync in the control registers.
fn random_write(rng: &mut Rng, sync: bool) -> (u8, u8) {
    let reg = rng.below(0x19) as u8;
    let mut value = rng.next() as u8;
    if reg % 7 == 4 && reg < 0x15 {
        if !sync {
            value &= !SYNC;
        }
        // Mostly a waveform and the gate's both states, sometimes TEST.
        if rng.below(4) != 0 {
            value &= !0x08;
        }
    }
    if reg == 0x18 {
        value |= 0x0f; // loud enough that every voice is heard
    }
    (reg, value)
}

/// What the voices show of themselves: each one's accumulator, registers, waveform output and
/// envelope, and OSC3 and ENV3 as a read returns them. (Not the bookkeeping that depends on
/// how the cycles were stepped: the envelopes' idle counts, the noise's deferred shifts.)
fn voices(s: &mut sid) -> Vec<u32> {
    let mut v = Vec::new();
    for i in 0..3 {
        let o = &s.osc[i];
        let e = &s.env[i];
        v.extend([o.acc, o.freq, o.pw, u32::from(o.control), o.out]);
        v.extend([e.counter, e.env3, e.gate].map(u32::from));
        v.push(e.state);
    }
    v.push(u32::from(unsafe { sid_read(s, 0x1b) }));
    v.push(u32::from(unsafe { sid_read(s, 0x1c) }));
    v
}

/// A stream of writes at increasing chip cycles: runs of writes a few cycles apart (several in
/// one sample, some in the same cycle), and gaps up to a few frames between them.
fn stream(rng: &mut Rng, n: usize, sync: bool) -> Vec<(u64, u8, u8)> {
    let mut at = 0u64;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        at += match rng.below(8) {
            0 => u64::from(rng.below(20_000)),
            1 => 0,
            2..=4 => u64::from(rng.below(8)),
            _ => u64::from(rng.below(200)),
        };
        let (reg, value) = random_write(rng, sync);
        out.push((at, reg, value));
    }
    out
}

/// Renders `s` up to chip cycle `x` (`clocked` is its cycles so far) with whole samples, then
/// advances it the rest of the way into the next sample: as the audio interrupt will place a
/// write (docs/plans/2026-10-19-m3-sid-files.md, the stamped writes). Returns the samples.
fn render_to(s: &mut sid, clocked: &mut u64, x: u64) -> Vec<i16> {
    let mut out = Vec::new();
    loop {
        let next = u64::from(next_cycles(s));
        if *clocked + next > x {
            break;
        }
        out.extend(render(s, 1));
        *clocked += next;
    }
    let k = (x - *clocked) as u32;
    advance(s, k);
    *clocked += u64::from(k);
    out
}

#[test]
fn a_write_placed_by_advance_lands_where_sid_clock_puts_it() {
    for model in MODELS {
        for seed in 1..=4u32 {
            let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9));
            let mut placed = new_sid(model);
            let mut reference = new_sid(model);
            let (mut c_placed, mut c_ref, mut samples) = (0u64, 0u64, 0u64);
            let step = u64::from(placed.cycles_per_sample_q16);
            for (i, &(x, reg, value)) in stream(&mut rng, 1_500, false).iter().enumerate() {
                samples += render_to(&mut placed, &mut c_placed, x).len() as u64;
                // The samples stay where the Q16 step puts them: every one that ends by x.
                // Sample n ends at cycle n step >> 16 (sid_init's accumulator starts at 0).
                assert_eq!(
                    samples,
                    (((x + 1) << 16) - 1) / step,
                    "write {i} at cycle {x}"
                );
                unsafe { sid_clock(&mut *reference, (x - c_ref) as u32) };
                c_ref = x;
                assert_eq!(
                    voices(&mut placed),
                    voices(&mut reference),
                    "model {model} seed {seed}: before write {i} (${reg:02x}) at cycle {x}"
                );
                unsafe {
                    sid_write(&mut *placed, reg, value);
                    sid_write(&mut *reference, reg, value);
                }
                assert_eq!(
                    voices(&mut placed),
                    voices(&mut reference),
                    "model {model} seed {seed}: after write {i} (${reg:02x}) at cycle {x}"
                );
            }
            let end = c_placed + 5_000;
            render_to(&mut placed, &mut c_placed, end);
            unsafe { sid_clock(&mut *reference, (end - c_ref) as u32) };
            assert_eq!(
                voices(&mut placed),
                voices(&mut reference),
                "model {model} seed {seed}: at the end"
            );
        }
    }
}

#[test]
fn advances_without_writes_leave_the_samples_bit_for_bit() {
    // The voices clocked in pieces reach each sample's end as one step reaches it, so a block
    // with advances in it sounds as one without: the render itself is the same code.
    for model in MODELS {
        let mut rng = Rng(0x51d0_u32 + model);
        let mut plain = new_sid(model);
        let mut split = new_sid(model);
        for _ in 0..200 {
            for _ in 0..rng.below(6) {
                let (reg, value) = random_write(&mut rng, false);
                unsafe {
                    sid_write(&mut *plain, reg, value);
                    sid_write(&mut *split, reg, value);
                }
            }
            let block = render(&mut plain, 128);
            let mut got = Vec::with_capacity(128);
            while got.len() < 128 {
                let mut left = next_cycles(&split);
                while rng.below(3) == 0 && left > 1 {
                    let k = rng.below(left);
                    advance(&mut split, k);
                    left -= k;
                    assert_eq!(next_cycles(&split), left);
                }
                got.extend(render(&mut split, 1));
            }
            assert_eq!(block, got, "model {model}");
            assert_eq!(plain.cycle_frac_q16, split.cycle_frac_q16, "model {model}");
            assert_eq!(voices(&mut plain), voices(&mut split), "model {model}");
        }
    }
}

#[test]
fn the_q16_accumulator_stays_right_across_a_million_advances() {
    // Voice 1 at frequency 1 with no waveform counts the cycles the voices have run (its
    // accumulator, mod 2^24); the Q16 arithmetic says how many the samples should have run.
    let mut s = new_sid(SID_MODEL_8580);
    unsafe { sid_write(&mut *s, 0x00, 0x01) };
    let step = u64::from(s.cycles_per_sample_q16);
    let acc0 = s.osc[0].acc;
    let mut rng = Rng(0x00c0_ffee);
    let (mut samples, mut advanced, mut advances) = (0u64, 0u64, 0u32);
    while advances < 1_000_000 {
        let frac0 = 0u64; // sid_init's
        let due = frac0 + samples * step; // the Q16 time at the last sample's end
        let next = ((due & 0xffff) + step) >> 16; // the next sample's cycles, before advances
        let into = advanced - (due >> 16); // cycles already advanced into it
        assert_eq!(
            u64::from(next_cycles(&s)),
            next - into,
            "after {advances} advances"
        );
        let left = (next - into) as u32;
        if left > 1 && rng.below(4) != 0 {
            let k = rng.below(left);
            advance(&mut s, k);
            advanced += u64::from(k);
            advances += 1;
        } else {
            let n = 1 + rng.below(3);
            render(&mut s, n as usize);
            samples += u64::from(n);
            advanced = (samples * step) >> 16;
        }
        let clocked = (s.osc[0].acc.wrapping_sub(acc0)) & 0xff_ffff;
        assert_eq!(
            u64::from(clocked),
            advanced & 0xff_ffff,
            "after {advances} advances"
        );
    }
    assert_eq!(
        u64::from(s.cycle_frac_q16) & 0xffff,
        (samples * step) & 0xffff
    );
}

#[test]
fn an_advance_of_zero_changes_nothing() {
    // Not even the end of a step: a write's pulse latch and OSC3's last until the next cycle.
    let mut s = new_sid(SID_MODEL_6581);
    render(&mut s, 10);
    unsafe { sid_write(&mut *s, 0x12, 0x41) };
    let before = (
        s.vf.osc3_held,
        s.vf.written,
        s.osc[2].pulse_held,
        s.cycle_frac_q16,
    );
    advance(&mut s, 0);
    assert_eq!(
        before,
        (
            s.vf.osc3_held,
            s.vf.written,
            s.osc[2].pulse_held,
            s.cycle_frac_q16
        )
    );
}

/// Plays the same random writes on a chip muted with `mask` and on the zero-voice build with
/// the same voices at 0, block by block, the mute changing at block starts; every sample must
/// match.
fn mute_against_zero_voices(model: sid_model, seed: u32, bleed: f32) {
    let mut rng = Rng(seed);
    let mut muted = new_sid(model);
    let mut zero = new_zero_voice(model);
    muted.bleed = bleed;
    zero.bleed = bleed;
    for block in 0..400 {
        if block % 25 == 0 {
            let mask = rng.below(8);
            unsafe {
                sid_set_mute(&mut *muted, mask as u8);
                sid_test_set_zero_voices(mask);
            }
        }
        for _ in 0..rng.below(6) {
            let (reg, value) = random_write(&mut rng, true);
            unsafe {
                sid_write(&mut *muted, reg, value);
                sid_write_zero_voice(&mut *zero, reg, value);
            }
        }
        let a = render(&mut muted, 64);
        let mut b = vec![0i16; 64];
        unsafe { sid_render_zero_voice(&mut *zero, b.as_mut_ptr(), 64) };
        assert_eq!(
            a, b,
            "model {model} seed {seed:#x} bleed {bleed} block {block}"
        );
    }
    unsafe { sid_test_set_zero_voices(0) };
}

#[test]
fn a_muted_voice_sounds_as_a_voice_that_outputs_nothing() {
    for model in MODELS {
        for seed in [0x1234_5678, 0x0bad_cafe, 0x7357_0001] {
            mute_against_zero_voices(model, seed, 0.0);
            mute_against_zero_voices(model, seed, 0.25); // the bleed's render
        }
    }
}

#[test]
fn every_single_voice_mute_with_its_routing_and_3off() {
    // Each voice muted alone, routed into the filter or not, voice 3 with 3OFF on and off, on
    // a sound with all three voices playing.
    for model in MODELS {
        for v in 0..3u8 {
            for routing in 0..16u8 {
                for off in [0u8, 0x80] {
                    let mut muted = new_sid(model);
                    let mut zero = new_zero_voice(model);
                    let setup: &[(u8, u8)] = &[
                        (0x01, 0x11),
                        (0x05, 0x09),
                        (0x06, 0xf0),
                        (0x04, 0x21),
                        (0x08, 0x1d),
                        (0x0c, 0x09),
                        (0x0d, 0xf0),
                        (0x0b, 0x41),
                        (0x0a, 0x08),
                        (0x0f, 0x2a),
                        (0x13, 0x09),
                        (0x14, 0xf0),
                        (0x12, 0x11),
                        (0x16, 0x40),
                        (0x17, 0xf0 | routing),
                        (0x18, 0x1f | off),
                    ];
                    unsafe {
                        sid_set_mute(&mut *muted, 1 << v);
                        sid_test_set_zero_voices(1 << v);
                        for &(reg, value) in setup {
                            sid_write(&mut *muted, reg, value);
                            sid_write_zero_voice(&mut *zero, reg, value);
                        }
                    }
                    let a = render(&mut muted, 2_000);
                    let mut b = vec![0i16; 2_000];
                    unsafe { sid_render_zero_voice(&mut *zero, b.as_mut_ptr(), 2_000) };
                    assert_eq!(
                        a, b,
                        "model {model} voice {v} routing {routing:#x} 3off {off:#x}"
                    );
                }
            }
        }
    }
    unsafe { sid_test_set_zero_voices(0) };
}

#[test]
fn the_zero_voice_build_with_none_at_zero_is_the_chip() {
    // The reference is the chip: with no voice at 0 it plays what the chip plays.
    for model in MODELS {
        let mut rng = Rng(0xfeed_0000 + model);
        let mut chip = new_sid(model);
        let mut zero = new_zero_voice(model);
        unsafe { sid_test_set_zero_voices(0) };
        for _ in 0..200 {
            for _ in 0..rng.below(6) {
                let (reg, value) = random_write(&mut rng, true);
                unsafe {
                    sid_write(&mut *chip, reg, value);
                    sid_write_zero_voice(&mut *zero, reg, value);
                }
            }
            let a = render(&mut chip, 64);
            let mut b = vec![0i16; 64];
            unsafe { sid_render_zero_voice(&mut *zero, b.as_mut_ptr(), 64) };
            assert_eq!(a, b, "model {model}");
        }
    }
}

#[test]
fn a_mute_leaves_the_voices_osc3_env3_sync_and_ring_alone() {
    // Mutes act on the output only: a muted chip's voices, hard sync and ring modulation
    // included, stay those of the same chip unmuted, write by write, and so do its reads. The
    // 6581's filter load follows the routing register, not the mute.
    for model in MODELS {
        let mut rng = Rng(0x0dd5_eed0 + model);
        let mut muted = new_sid(model);
        let mut open = new_sid(model);
        for i in 0..4_000 {
            if i % 100 == 0 {
                unsafe { sid_set_mute(&mut *muted, rng.below(8) as u8) };
            }
            let (reg, value) = random_write(&mut rng, true);
            unsafe {
                sid_write(&mut *muted, reg, value);
                sid_write(&mut *open, reg, value);
            }
            let n = rng.below(4) as usize;
            render(&mut muted, n);
            render(&mut open, n);
            assert_eq!(
                voices(&mut muted),
                voices(&mut open),
                "model {model} write {i}"
            );
            for v in 0..3 {
                let (a, b) = (&muted.osc[v], &open.osc[v]);
                assert_eq!(
                    (a.sr, a.noise, a.msb_rose, a.overshoot),
                    (b.sr, b.noise, b.msb_rose, b.overshoot),
                    "model {model} write {i} voice {v}"
                );
            }
            assert_eq!(
                muted.dc_in.to_bits(),
                open.dc_in.to_bits(),
                "model {model} write {i}"
            );
        }
    }
}

/// The render's two masks (sid.h, voice_path), as a render (of no samples) takes them as it
/// starts: (route_on, direct_on).
fn masks(s: &mut sid) -> (u8, u8) {
    render(s, 0);
    (
        s.voice_path & (7 * SID_PATH_FILTER) as u8,
        (s.voice_path / SID_PATH_DIRECT as u8) & 7,
    )
}

#[test]
fn the_masks_follow_routing_3off_and_the_mute() {
    let mut s = new_sid(SID_MODEL_8580);
    assert_eq!(
        (s.mute, masks(&mut s)),
        (0, (0, 7)),
        "sid_init's: every voice direct"
    );
    unsafe {
        sid_write(&mut *s, 0x17, 0x0d); // voices 1 and 3 and EXT IN into the filter
        sid_write(&mut *s, 0x18, 0x8f); // 3OFF
    }
    assert_eq!(masks(&mut s), (0x05, 0x02));
    unsafe { sid_set_mute(&mut *s, 0xfa) }; // bits 3-7 ignored: voice 2
    assert_eq!((s.mute, masks(&mut s)), (0x02, (0x05, 0x00)));
    unsafe { sid_set_mute(&mut *s, 0x05) };
    assert_eq!(masks(&mut s), (0x00, 0x02));
    unsafe { sid_write(&mut *s, 0x17, 0x00) };
    assert_eq!(
        masks(&mut s),
        (0x00, 0x02),
        "voice 3 still off (3OFF), voice 1 muted"
    );
    unsafe { sid_write(&mut *s, 0x18, 0x0f) };
    assert_eq!(masks(&mut s), (0x00, 0x02));
    unsafe { sid_set_mute(&mut *s, 0) };
    assert_eq!(masks(&mut s), (0x00, 0x07));
}
