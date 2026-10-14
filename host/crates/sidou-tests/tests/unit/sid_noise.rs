// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The noise register in reSIDfp's bit order (bit 22 is where the new bit enters).
use sidou_sys::*;

fn noise_osc(is6581: bool) -> sid_osc {
    let mut o = sid_osc::default();
    unsafe {
        sid_osc_set_model(&mut o, i32::from(is6581));
        sid_osc_reset(&mut o);
        sid_osc_write_control(&mut o, SID_CTRL_NOISE as u8);
    }
    o
}

fn advance(o: &mut sid_osc, cycles: u32) {
    unsafe { sid_osc_advance(o, cycles, 0, 0) }
}

/// One shift as reSIDfp does it outside TEST: up one, new bit = bit 0 ^ bit 5.
fn shift(sr: u32) -> u32 {
    (sr >> 1) | (((sr ^ (sr >> 5)) & 1) << 22)
}

#[test]
fn reset_sets_all_ones_then_shifts_in_a_zero() {
    let o = noise_osc(true);
    assert_eq!(o.sr, 0x3fffff);
    assert_eq!(o.noise, unsafe { sid_noise_output(0x3fffff) });
}

#[test]
fn output_uses_re_sid_fp_tap_mapping_and_write_back_inverts_it() {
    unsafe {
        assert_eq!(sid_noise_output(1 << 2), 0x800);
        assert_eq!(sid_noise_output(1 << 22), 0x010);
        assert_eq!(sid_noise_output(0x7fffff), 0xff0);
        assert_eq!(sid_noise_output(SID_NOISE_TAPS), 0xff0);
        for bit in 4..12 {
            let out = 1u32 << bit;
            assert_eq!(sid_noise_output(sid_noise_writeback(out)), out);
        }
    }
}

/// The register shifts two cycles after accumulator bit 19 rises, whatever the step
/// sizes; against a cycle-by-cycle model.
#[test]
fn shifts_two_cycles_after_bit_19_rises() {
    for freq in [0x0123u32, 0x7531, 0xffff] {
        let mut o = noise_osc(true);
        o.freq = freq;
        let (mut acc, mut sr, mut pipeline) = (o.acc, o.sr, 0u32);
        let mut x = 7u32;
        for step in 0..3000 {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            let cycles = 1 + (x >> 24) % 23;
            for _ in 0..cycles {
                let old = acc;
                acc = (acc + freq) & 0xffffff;
                if !old & acc & 0x80000 != 0 {
                    pipeline = 2;
                } else if pipeline > 0 {
                    pipeline -= 1;
                    if pipeline == 0 {
                        sr = shift(sr);
                    }
                }
            }
            advance(&mut o, cycles);
            assert_eq!(o.acc, acc, "freq {freq:#x}, step {step}");
            assert_eq!(o.sr, sr, "freq {freq:#x}, step {step}");
            assert_eq!(
                u32::from(o.shift_pipeline),
                pipeline,
                "freq {freq:#x}, step {step}"
            );
        }
    }
}

#[test]
fn test_keeps_the_register_then_fades_it_to_all_ones() {
    for (is6581, reset, fade) in [
        (true, SID_NOISE_RESET_6581, SID_NOISE_FADE_6581),
        (false, SID_NOISE_RESET_8580, SID_NOISE_FADE_8580),
    ] {
        let mut o = noise_osc(is6581);
        o.sr = 0x123456;
        unsafe { sid_osc_write_control(&mut o, (SID_CTRL_NOISE | SID_CTRL_TEST) as u8) };
        assert_eq!(o.acc, 0);
        advance(&mut o, reset - 1);
        assert_eq!(o.sr, 0x123456);
        advance(&mut o, 1);
        assert_eq!(o.sr, 0x123456 | 0x123456 >> 1 | 0x400000);
        let mut fades = 1;
        while o.sr != 0x7fffff {
            advance(&mut o, fade);
            fades += 1;
            assert!(fades < 30);
        }
        assert_eq!(o.sr_reset, 0);
    }
}

#[test]
fn releasing_test_shifts_with_the_new_bit_forced_to_one() {
    let mut o = noise_osc(true);
    o.sr = 0x000020; // bit 5 set: forced 1 ^ bit 5 = 0
    unsafe {
        sid_osc_write_control(&mut o, (SID_CTRL_NOISE | SID_CTRL_TEST) as u8);
    }
    advance(&mut o, 10);
    unsafe { sid_osc_write_control(&mut o, SID_CTRL_NOISE as u8) };
    assert_eq!(o.sr, 0x000010);
    o.sr = 0x000000;
    unsafe { sid_osc_write_control(&mut o, (SID_CTRL_NOISE | SID_CTRL_TEST) as u8) };
    advance(&mut o, 10);
    unsafe { sid_osc_write_control(&mut o, SID_CTRL_NOISE as u8) };
    assert_eq!(o.sr, 0x400000);
}

#[test]
fn register_is_maximal_length() {
    let mut sr = 0x7fffffu32;
    for _ in 0..0x7fffffu32 {
        sr = shift(sr);
    }
    assert_eq!(sr, 0x7fffff);
    // And sid_noise_shift_phase2 is that shift.
    let mut o = noise_osc(true);
    for _ in 0..1000 {
        let expect = shift(o.sr);
        o.latch = o.sr;
        o.test_or_reset = 0;
        unsafe { sid_noise_shift_phase2(&mut o, 8, 8) };
        assert_eq!(o.sr, expect);
    }
}

#[test]
fn combined_with_pulse_the_output_locks_the_register_at_zero() {
    let mut o = noise_osc(true);
    o.freq = 0x7531;
    o.pw = 0x100;
    unsafe { sid_osc_write_control(&mut o, (SID_CTRL_NOISE | SID_CTRL_PULSE) as u8) };
    for _ in 0..2000 {
        advance(&mut o, 22);
    }
    assert_eq!(o.sr, 0);
    assert_eq!(o.noise, 0);
}

/// sid_osc_pull_msb relies on this: on the 6581, in every profile, a sawtooth
/// combination's output has its top bit low in the cycle the accumulator enters its
/// upper half (top 12 bits 0x800 to 0x80f), with the pulse high or low.
#[test]
fn saw_combinations_output_a_low_top_bit_entering_the_upper_half() {
    let mut o = sid_osc::default();
    unsafe { sid_osc_set_model(&mut o, 1) };
    for profile in 0..SID_PROFILES {
        unsafe { sid_osc_set_profile(&mut o, profile) };
        for control in [0x30u8, 0x60, 0x70] {
            o.control = control;
            unsafe { sid_osc_update_mode(&mut o) };
            for ix in 0x800..=0x80f {
                for pulse in [0, 0xfff] {
                    let out = unsafe { sid_osc_combine(&o, ix, pulse, 0xfff) };
                    assert_eq!(
                        out & 0x800,
                        0,
                        "profile {profile}, control {control:#04x}, ix {ix:#x}"
                    );
                }
            }
        }
    }
}

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        self.0 >> 8
    }
}

fn assert_same_osc(a: &sid_osc, b: &sid_osc, at: &str) {
    assert_eq!(a.acc, b.acc, "acc, {at}");
    assert_eq!(a.sr, b.sr, "sr, {at}");
    assert_eq!(a.noise, b.noise, "noise, {at}");
    assert_eq!(a.out, b.out, "out, {at}");
    if a.cycle_mode == 1 {
        // Kept only while running cycle by cycle (locked up, OSC3 is computed instead).
        assert_eq!(a.osc3, b.osc3, "osc3, {at}");
        assert_eq!(a.pulse, b.pulse, "pulse, {at}");
        if a.is6581 == 0 {
            // The 8580's OSC3 latch; the 6581 does not use it.
            assert_eq!(a.tri_saw, b.tri_saw, "tri_saw, {at}");
        }
    }
    assert_eq!(a.shift_pipeline, b.shift_pipeline, "pipeline, {at}");
    assert_eq!(a.test_or_reset, b.test_or_reset, "test_or_reset, {at}");
    assert_eq!(a.sr_reset, b.sr_reset, "sr_reset, {at}");
    assert_eq!(a.msb_rose, b.msb_rose, "msb_rose, {at}");
    assert_eq!(a.pulled_last, b.pulled_last, "pulled_last, {at}");
    if a.shift_pipeline == 1 {
        assert_eq!(a.latch, b.latch, "latch, {at}");
    }
}

/// Noise combined with other waveforms, with TEST, ring modulation and pulse width
/// changes: sid_osc_advance's bulk steps against cycle by cycle.
#[test]
fn combined_noise_bulk_steps_match_cycle_by_cycle() {
    for (is6581, seed) in [(1, 1u32), (0, 2), (1, 3), (0, 4)] {
        let mut rng = Rng(seed);
        let mut fast = noise_osc(is6581 == 1);
        let mut slow = fast;
        let mut src_acc = 0x123456u32;
        for step in 0..150_000 {
            let op = rng.next() % 64;
            if op < 4 {
                let wf = [0x9u8, 0xa, 0xb, 0xc, 0xd, 0xe, 0xf, 0x8][(rng.next() % 8) as usize];
                let control = (wf << 4)
                    | if rng.next().is_multiple_of(4) {
                        SID_CTRL_TEST as u8
                    } else {
                        0
                    }
                    | if rng.next().is_multiple_of(4) {
                        SID_CTRL_RING as u8
                    } else {
                        0
                    };
                unsafe {
                    sid_osc_latch_ramp(&mut fast, src_acc);
                    sid_osc_latch_ramp(&mut slow, src_acc);
                    sid_osc_write_control(&mut fast, control);
                    sid_osc_write_control(&mut slow, control);
                }
            } else if op < 6 {
                let pw = rng.next() % 0x1000;
                unsafe { sid_osc_write_pw(&mut fast, pw) };
                unsafe { sid_osc_write_pw(&mut slow, pw) };
            } else if op < 7 {
                let freq = [0x0123u32, 0x1167, 0x4000, 0xfedc][(rng.next() % 4) as usize];
                fast.freq = freq;
                fast.quiet = 0; // as sid_write does
                slow.freq = freq;
                slow.quiet = 0; // as sid_write does
            } else {
                let cycles = if rng.next().is_multiple_of(4) {
                    1 + rng.next() % 3
                } else {
                    1 + rng.next() % 23
                };
                let src_freq = 0x2b89;
                unsafe {
                    if fast.control >> 4 > 8 {
                        sid_osc_advance(&mut fast, cycles, src_acc, src_freq);
                        sid_osc_advance_cycles(&mut slow, cycles, src_acc, src_freq);
                    } else {
                        sid_osc_advance(&mut fast, cycles, src_acc, src_freq);
                        sid_osc_advance(&mut slow, cycles, src_acc, src_freq);
                    }
                }
                src_acc = (src_acc + cycles * src_freq) & 0xffffff;
                if fast.control >> 4 > 8 {
                    assert_same_osc(&fast, &slow, &format!("model {is6581}, step {step}"));
                }
            }
        }
    }
}
