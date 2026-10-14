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
    assert_eq!(
        o.cycle_mode, 0,
        "locked up, it leaves the cycle-by-cycle path"
    );
}

/// sid_osc_pull_msb relies on this: on the 6581, at every strength, a sawtooth
/// combination's output has its top bit low in the cycle the accumulator enters its
/// upper half (top 12 bits 0x800 to 0x80f), with the pulse high or low.
#[test]
fn saw_combinations_output_a_low_top_bit_entering_the_upper_half() {
    let mut o = sid_osc::default();
    unsafe { sid_osc_set_model(&mut o, 1) };
    for point in 0..SID_STRENGTH_POINTS {
        unsafe { sid_osc_set_strength(&mut o, point) };
        for control in [0x30u8, 0x60, 0x70] {
            o.control = control;
            unsafe { sid_osc_update_mode(&mut o) };
            for ix in 0x800..=0x80f {
                for pulse in [0, 0xfff] {
                    let out = unsafe { sid_osc_combine(&o, ix, pulse, 0xfff) };
                    assert_eq!(
                        out & 0x800,
                        0,
                        "strength point {point}, control {control:#04x}, ix {ix:#x}"
                    );
                }
            }
        }
    }
}
