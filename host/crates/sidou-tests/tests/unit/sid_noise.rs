// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;

#[test]
fn clocks_on_every_rising_edge_of_bit_19() {
    const FREQ: u32 = 0xffff;
    const CYCLES: u32 = 23;
    let mut o = sid_osc::default();
    unsafe {
        sid_osc_reset(&mut o);
        sid_osc_write_control(&mut o, SID_CTRL_NOISE as u8);
    }
    o.freq = FREQ;
    o.acc = 0xf00000; // the first step crosses the 24-bit wrap

    let mut reference = o;
    let mut clocks = 0;
    for step in 0..200 {
        for _ in 0..CYCLES {
            let prev = reference.acc;
            reference.acc = (prev + FREQ) & 0xffffff;
            if prev & 0x80000 == 0 && reference.acc & 0x80000 != 0 {
                unsafe { sid_lfsr_clock(&mut reference) };
                clocks += 1;
            }
        }
        unsafe { sid_osc_advance(&mut o, CYCLES) };
        assert_eq!(o.acc, reference.acc, "step {step}");
        assert_eq!(o.lfsr, reference.lfsr, "step {step}");
    }
    assert!(clocks > 200);
}

#[test]
fn test_bit_clears_lfsr_so_noise_is_silent() {
    let mut o = sid_osc::default();
    unsafe {
        sid_osc_reset(&mut o);
        sid_osc_write_control(&mut o, SID_CTRL_NOISE as u8);
        o.freq = 0xffff;
        sid_osc_advance(&mut o, 23);
        sid_osc_write_control(&mut o, (SID_CTRL_NOISE | SID_CTRL_TEST) as u8);
        sid_osc_advance(&mut o, 23);
    }
    assert_eq!(o.lfsr, 0);
    assert_eq!(unsafe { sid_osc_output(&o, &o) }, 0);
}

#[test]
fn releasing_test_bit_reloads_lfsr() {
    let mut o = sid_osc::default();
    unsafe {
        sid_osc_reset(&mut o);
        sid_osc_write_control(&mut o, (SID_CTRL_NOISE | SID_CTRL_TEST) as u8);
        sid_osc_write_control(&mut o, SID_CTRL_NOISE as u8);
    }
    assert_eq!(o.lfsr, SID_LFSR_RESET);
}

#[test]
fn first_clock_from_reset_shifts_in_zero() {
    let mut o = sid_osc::default();
    unsafe {
        sid_osc_reset(&mut o);
        sid_lfsr_clock(&mut o);
    }
    assert_eq!(o.lfsr, 0x7ffff0);
}

#[test]
fn output_uses_re_sid_tap_mapping() {
    unsafe {
        assert_eq!(sid_noise_output(SID_LFSR_RESET), 0xfe0);
        assert_eq!(sid_noise_output(0x400000), 0x800);
        assert_eq!(sid_noise_output(0x000004), 0x010);
    }
}

#[test]
fn lfsr_is_maximal_length() {
    let mut o = sid_osc::default();
    unsafe {
        sid_osc_reset(&mut o);
        for _ in 0..0x7fffffu32 {
            sid_lfsr_clock(&mut o);
        }
    }
    assert_eq!(o.lfsr, SID_LFSR_RESET);
}
