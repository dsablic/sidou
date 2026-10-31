// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::render::{device_sid, is_voice_control, render_then_advance_to};

const RATE: u32 = 44_118;
const SID_INIT_CYCLE_FRAC_Q16: u64 = 0;
const MODELS: [sid_model; 2] = [SID_MODEL_6581, SID_MODEL_8580];
const SYNC: u8 = 0x02;
const TEST: u8 = 0x08;
const VOLUME_FULL: u8 = 0x0f;
const MODE_VOLUME: u8 = 0x18;

struct Xorshift32(u32);

impl Xorshift32 {
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

fn render_zero_voice(s: &mut sid, n: usize) -> Vec<i16> {
    let mut out = vec![0i16; n];
    unsafe { sid_render_zero_voice(s, out.as_mut_ptr(), n as u32) };
    out
}

fn next_cycles(s: &sid) -> u32 {
    unsafe { sid_next_cycles(s) }
}

fn advance(s: &mut sid, k: u32) {
    unsafe { sid_advance(s, k) }
}

fn random_write(rng: &mut Xorshift32, sync: bool) -> (u8, u8) {
    let reg = rng.below(0x19) as u8;
    let mut value = rng.next() as u8;
    if is_voice_control(reg) {
        if !sync {
            value &= !SYNC;
        }
        if rng.below(4) != 0 {
            value &= !TEST;
        }
    }
    if reg == MODE_VOLUME {
        value |= VOLUME_FULL;
    }
    (reg, value)
}

fn observable_voices(s: &mut sid) -> Vec<u32> {
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

fn bursty_write_stream(rng: &mut Xorshift32, n: usize, sync: bool) -> Vec<(u64, u8, u8)> {
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

#[test]
fn a_write_placed_by_advance_lands_where_sid_clock_puts_it() {
    for model in MODELS {
        for seed in 1..=4u32 {
            let mut rng = Xorshift32(seed.wrapping_mul(0x9e37_79b9));
            let mut placed = device_sid(model);
            let mut reference = device_sid(model);
            let (mut c_placed, mut c_ref, mut samples) = (0u64, 0u64, 0u64);
            let step = u64::from(placed.cycles_per_sample_q16);
            for (i, &(x, reg, value)) in bursty_write_stream(&mut rng, 1_500, false)
                .iter()
                .enumerate()
            {
                samples += render_then_advance_to(&mut placed, &mut c_placed, x).len() as u64;
                assert_eq!(
                    samples,
                    (((x + 1) << 16) - 1) / step,
                    "write {i} at cycle {x}"
                );
                unsafe { sid_clock(&mut *reference, (x - c_ref) as u32) };
                c_ref = x;
                assert_eq!(
                    observable_voices(&mut placed),
                    observable_voices(&mut reference),
                    "model {model} seed {seed}: before write {i} (${reg:02x}) at cycle {x}"
                );
                unsafe {
                    sid_write(&mut *placed, reg, value);
                    sid_write(&mut *reference, reg, value);
                }
                assert_eq!(
                    observable_voices(&mut placed),
                    observable_voices(&mut reference),
                    "model {model} seed {seed}: after write {i} (${reg:02x}) at cycle {x}"
                );
            }
            let end = c_placed + 5_000;
            render_then_advance_to(&mut placed, &mut c_placed, end);
            unsafe { sid_clock(&mut *reference, (end - c_ref) as u32) };
            assert_eq!(
                observable_voices(&mut placed),
                observable_voices(&mut reference),
                "model {model} seed {seed}: at the end"
            );
        }
    }
}

#[test]
fn advances_without_writes_leave_the_samples_bit_for_bit() {
    for model in MODELS {
        let mut rng = Xorshift32(0x51d0_u32 + model);
        let mut plain = device_sid(model);
        let mut split = device_sid(model);
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
            assert_eq!(
                observable_voices(&mut plain),
                observable_voices(&mut split),
                "model {model}"
            );
        }
    }
}

#[test]
fn the_q16_accumulator_stays_right_across_a_million_advances() {
    let mut s = device_sid(SID_MODEL_8580);
    unsafe { sid_write(&mut *s, 0x00, 0x01) };
    let step = u64::from(s.cycles_per_sample_q16);
    let acc0 = s.osc[0].acc;
    let mut rng = Xorshift32(0x00c0_ffee);
    let (mut samples, mut advanced, mut advances) = (0u64, 0u64, 0u32);
    while advances < 1_000_000 {
        let due_q16 = SID_INIT_CYCLE_FRAC_Q16 + samples * step;
        let next_sample_cycles = ((due_q16 & 0xffff) + step) >> 16;
        let advanced_into_it = advanced - (due_q16 >> 16);
        assert_eq!(
            u64::from(next_cycles(&s)),
            next_sample_cycles - advanced_into_it,
            "after {advances} advances"
        );
        let left = (next_sample_cycles - advanced_into_it) as u32;
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
fn an_advance_of_zero_keeps_the_pulse_latch_and_osc3_held() {
    let mut s = device_sid(SID_MODEL_6581);
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

fn assert_mute_sounds_as_zero_voices(model: sid_model, seed: u32, bleed: f32) {
    let mut rng = Xorshift32(seed);
    let mut muted = device_sid(model);
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
        let b = render_zero_voice(&mut zero, 64);
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
            assert_mute_sounds_as_zero_voices(model, seed, 0.0);
            assert_mute_sounds_as_zero_voices(model, seed, 0.25);
        }
    }
}

#[test]
fn every_single_voice_mute_with_its_routing_and_3off() {
    for model in MODELS {
        for v in 0..3u8 {
            for routing in 0..16u8 {
                for off in [0u8, 0x80] {
                    let mut muted = device_sid(model);
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
                    let b = render_zero_voice(&mut zero, 2_000);
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
    for model in MODELS {
        let mut rng = Xorshift32(0xfeed_0000 + model);
        let mut chip = device_sid(model);
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
            let b = render_zero_voice(&mut zero, 64);
            assert_eq!(a, b, "model {model}");
        }
    }
}

#[test]
fn a_mute_leaves_the_voices_osc3_env3_sync_and_ring_alone() {
    for model in MODELS {
        let mut rng = Xorshift32(0x0dd5_eed0 + model);
        let mut muted = device_sid(model);
        let mut open = device_sid(model);
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
                observable_voices(&mut muted),
                observable_voices(&mut open),
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

const ROUTE_VOICES_1_3_AND_EXT_IN: u8 = 0x0d;
const THREE_OFF_FULL_VOLUME: u8 = 0x8f;
const MUTE_VOICE_2_WITH_BITS_3_TO_7_SET: u8 = 0xfa;

fn route_and_direct_masks(s: &mut sid) -> (u8, u8) {
    render(s, 0);
    (
        s.voice_path & (7 * SID_PATH_FILTER) as u8,
        (s.voice_path / SID_PATH_DIRECT as u8) & 7,
    )
}

#[test]
fn the_masks_follow_routing_3off_and_the_mute() {
    let mut s = device_sid(SID_MODEL_8580);
    assert_eq!(
        (s.mute, route_and_direct_masks(&mut s)),
        (0, (0, 7)),
        "sid_init's: every voice direct"
    );
    unsafe {
        sid_write(&mut *s, 0x17, ROUTE_VOICES_1_3_AND_EXT_IN);
        sid_write(&mut *s, 0x18, THREE_OFF_FULL_VOLUME);
    }
    assert_eq!(route_and_direct_masks(&mut s), (0x05, 0x02));
    unsafe { sid_set_mute(&mut *s, MUTE_VOICE_2_WITH_BITS_3_TO_7_SET) };
    assert_eq!(
        (s.mute, route_and_direct_masks(&mut s)),
        (0x02, (0x05, 0x00)),
        "bits 3-7 of the mute ignored"
    );
    unsafe { sid_set_mute(&mut *s, 0x05) };
    assert_eq!(route_and_direct_masks(&mut s), (0x00, 0x02));
    unsafe { sid_write(&mut *s, 0x17, 0x00) };
    assert_eq!(
        route_and_direct_masks(&mut s),
        (0x00, 0x02),
        "voice 3 still off (3OFF), voice 1 muted"
    );
    unsafe { sid_write(&mut *s, 0x18, 0x0f) };
    assert_eq!(route_and_direct_masks(&mut s), (0x00, 0x02));
    unsafe { sid_set_mute(&mut *s, 0) };
    assert_eq!(route_and_direct_masks(&mut s), (0x00, 0x07));
}
