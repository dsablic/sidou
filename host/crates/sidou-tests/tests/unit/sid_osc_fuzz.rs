// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Adversarial fuzz (from the Task 3 reviews): noise combined with other waveforms,
//! sid_osc_advance's bulk steps against strictly cycle by cycle, every field after every
//! step: long TEST holds through fade events, rich register refills, every waveform,
//! the output updated as clock_voices does, random hard syncs and ring sources that
//! leave their linear course. FUZZ_OPS and FUZZ_SEEDS scale it up.
use sidou_sys::*;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

fn same(a: &sid_osc, b: &sid_osc, at: &str) {
    let fields = [
        ("acc", a.acc, b.acc),
        ("sr", a.sr, b.sr),
        ("noise", a.noise, b.noise),
        ("out", a.out, b.out),
        ("osc3", a.osc3, b.osc3),
        ("pipeline", a.shift_pipeline as u32, b.shift_pipeline as u32),
        ("tor", a.test_or_reset as u32, b.test_or_reset as u32),
        ("sr_reset", a.sr_reset, b.sr_reset),
        ("msb_rose", a.msb_rose as u32, b.msb_rose as u32),
        ("pulled_last", a.pulled_last as u32, b.pulled_last as u32),
    ];
    for (n, x, y) in fields {
        assert_eq!(x, y, "{n} differs, {at}: fast {a:?}\nslow {b:?}");
    }
    if a.cycle_mode == 1 {
        // Kept current only cycle by cycle; locked up, the inline path recomputes them
        // when needed (the pulse on re-entry, the latch on a control write).
        assert_eq!(a.pulse, b.pulse, "pulse, {at}");
        if a.is6581 == 0 {
            assert_eq!(a.tri_saw, b.tri_saw, "tri_saw, {at}");
        }
    }
    if a.shift_pipeline == 1 {
        assert_eq!(a.latch, b.latch, "latch, {at}");
    }
    if a.msb_rose != 0 {
        assert_eq!(a.overshoot, b.overshoot, "overshoot, {at}");
    }
}

fn run(seed: u64, is6581: i32, profile: u32, ops: u32) -> (u64, u64) {
    let mut r = Rng(seed);
    let mut o = sid_osc::default();
    unsafe {
        sid_osc_set_model(&mut o, is6581);
        sid_osc_set_profile(&mut o, profile);
        sid_osc_reset(&mut o);
        sid_osc_write_control(&mut o, 0x80);
    }
    let (mut fast, mut slow) = (o, o);
    let mut src_acc = r.below(1 << 24);
    let mut src_freq = r.below(0x10000);
    let mut combined_cycles = 0u64;
    let mut held = 0u32;
    for step in 0..ops {
        let op = r.below(100);
        if op < 6 && fast.pulse_held == 0 {
            let wf = r.below(16) as u8;
            let mut c = (wf << 4) | (r.below(16) as u8 & 0x0e);
            if r.below(3) != 0 {
                c &= !(SID_CTRL_TEST as u8);
            }
            if r.below(40) == 0 {
                c |= SID_CTRL_TEST as u8;
                held = 2000;
            }
            unsafe {
                // As sid_write: latch the 8580's triangle/saw, then write.
                sid_osc_latch_ramp(&mut fast, src_acc);
                sid_osc_latch_ramp(&mut slow, src_acc);
                sid_osc_write_control(&mut fast, c);
                sid_osc_write_control(&mut slow, c);
            }
        } else if op < 8 && fast.pulse_held == 0 {
            let pw = [0u32, 0xfff, 0x800, r.below(0x1000)][r.below(4) as usize];
            unsafe { sid_osc_write_pw(&mut fast, pw) };
            unsafe { sid_osc_write_pw(&mut slow, pw) };
        } else if op < 11 {
            let f = [
                0u32,
                0xffff,
                0x8000,
                0x7fff,
                0x10,
                0x1,
                r.below(0x10000),
                r.below(0x10000),
            ][r.below(8) as usize];
            fast.freq = f;
            fast.quiet = 0; // as sid_write does
            slow.freq = f;
            slow.quiet = 0; // as sid_write does
        } else if op < 12 && r.below(2) == 0 {
            let mut src = sid_osc::default();
            src.freq = 1 + r.below(0xffff);
            src.overshoot = r.below(src.freq * 23);
            src.msb_rose = 1;
            if fast.control & SID_CTRL_TEST as u8 == 0 && fast.shift_pipeline == 0 {
                unsafe {
                    sid_osc_sync(&mut fast, &src);
                    sid_osc_sync(&mut slow, &src);
                }
            }
        } else if op < 12 && r.below(2) == 0 {
            // the source deviates slightly from its linear course (as a pulled MSB or sync would)
            src_acc = (src_acc ^ 0x800000) & 0xffffff;
        } else if op < 12 {
            src_freq = [0u32, 0xffff, r.below(0x10000)][r.below(3) as usize];
            src_acc = r.below(1 << 24);
        } else if op < 13
            && fast.control >> 4 > 8
            && fast.shift_pipeline == 0
            && slow.shift_pipeline == 0
        {
            // refill the register with a rich, consistent state
            let sr = r.below(1 << 23);
            for x in [&mut fast, &mut slow] {
                x.sr = sr;
                x.latch = sr;
                x.noise = unsafe { sid_noise_output(sr) };
                x.test_or_reset = 0;
                // Poking the state behind the core's back: recompute what it derives
                // from it, as a write would (the register can only refill through one).
                unsafe {
                    let control = x.control;
                    sid_osc_latch_ramp(x, src_acc);
                    sid_osc_write_control(x, control);
                }
            }
        } else {
            let mut cycles = match r.below(4) {
                0 => 1 + r.below(3),
                1 => 1 + r.below(256),
                _ => 1 + r.below(23),
            };
            let reps = if held > 0 {
                held -= 1;
                20
            } else {
                1
            };
            for _ in 0..reps {
                if let Some(max) = 0xffffffu32.checked_div(fast.freq) {
                    cycles = cycles.min(max).max(1);
                }
                unsafe {
                    combined_cycles += u64::from(cycles);
                    sid_osc_advance(&mut fast, cycles, src_acc, src_freq);
                    sid_osc_advance_cycles(&mut slow, cycles, src_acc, src_freq);
                }
                src_acc = (src_acc + cycles * src_freq) & 0xffffff;
                // As clock_voices does after each step.
                fast.pulse_held = 0;
                slow.pulse_held = 0;
                if fast.control >> 4 != 0 {
                    // As clock_voices does with the envelope nonzero.
                    fast.out = unsafe { sid_osc_output(&fast, src_acc) };
                }
                if fast.control >> 4 > 8 {
                    same(
                        &fast,
                        &slow,
                        &format!("seed {seed} model {is6581} profile {profile} step {step}"),
                    );
                }
            }
        }
    }
    (combined_cycles, 0)
}

#[test]
fn reviewer_fuzz_bulk_vs_strict() {
    let ops: u32 = std::env::var("FUZZ_OPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200_000);
    let seeds: u64 = std::env::var("FUZZ_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let mut total = 0;
    for seed in 1..=seeds {
        for is6581 in [1, 0] {
            for profile in 0..SID_PROFILES {
                total += run(seed * 7919 + profile as u64, is6581, profile, ops).0;
            }
        }
    }
    assert!(total > 0);
}
