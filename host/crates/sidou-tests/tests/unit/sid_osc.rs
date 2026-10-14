// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;

fn make_osc(control: u32, freq: u32) -> sid_osc {
    let mut o = sid_osc::default();
    unsafe {
        sid_osc_reset(&mut o);
        sid_osc_write_control(&mut o, control as u8);
    }
    o.freq = freq;
    o
}

fn output(o: &sid_osc, src: &sid_osc) -> u32 {
    unsafe { sid_osc_output(o, src) }
}

#[test]
fn accumulator_adds_frequency_every_cycle() {
    let mut o = make_osc(SID_CTRL_SAW, 0x1000);
    unsafe { sid_osc_advance(&mut o, 3) };
    assert_eq!(o.acc, 0x3000);
}

#[test]
fn accumulator_wraps_at_24_bits_and_reports_msb_rise() {
    const FREQ: u32 = 0xffff;
    const CYCLES: u32 = 23;
    let mut o = make_osc(SID_CTRL_SAW, FREQ);
    o.acc = 0xf00000; // the first step crosses the 24-bit wrap
    let mut wraps = 0;
    let mut rises = 0;
    for step in 0..200 {
        let mut acc = o.acc;
        let mut rose = 0u8;
        let mut overshoot = 0u32;
        for _ in 0..CYCLES {
            let prev = acc;
            acc = (prev + FREQ) & 0xffffff;
            overshoot += FREQ;
            if prev & 0x800000 == 0 && acc & 0x800000 != 0 {
                rose = 1;
                overshoot = acc - 0x800000;
            }
            if acc < prev {
                wraps += 1;
            }
        }
        unsafe { sid_osc_advance(&mut o, CYCLES) };
        assert_eq!(o.acc, acc, "step {step}");
        assert_eq!(o.msb_rose, rose, "step {step}");
        if rose != 0 {
            assert_eq!(o.overshoot, overshoot, "step {step}");
            rises += 1;
        }
    }
    assert!(wraps > 0);
    assert!(rises > 0);
}

#[test]
fn saw_is_the_top_12_bits() {
    let mut o = make_osc(SID_CTRL_SAW, 0x1000);
    unsafe { sid_osc_advance(&mut o, 3) };
    assert_eq!(output(&o, &o), 3);
}

#[test]
fn triangle_folds_on_the_msb() {
    let mut o = make_osc(SID_CTRL_TRI, 0);
    o.acc = 0x400000;
    assert_eq!(output(&o, &o), 0x800);
    o.acc = 0x7fffff;
    assert_eq!(output(&o, &o), 0xfff);
    o.acc = 0xc00000;
    assert_eq!(output(&o, &o), 0x7ff);
}

#[test]
fn ring_mod_xors_the_source_msb_into_the_triangle() {
    let mut o = make_osc(SID_CTRL_TRI | SID_CTRL_RING, 0);
    let mut src = make_osc(SID_CTRL_SAW, 0);
    o.acc = 0x400000;
    src.acc = 0x800000;
    assert_eq!(output(&o, &src), 0x7ff);
}

#[test]
fn pulse_compares_top_12_bits_with_pulse_width() {
    let mut o = make_osc(SID_CTRL_PULSE, 0);
    o.pw = 0x800;
    o.acc = 0x7ff000;
    assert_eq!(output(&o, &o), 0x000);
    o.acc = 0x800000;
    assert_eq!(output(&o, &o), 0xfff);
}

#[test]
fn no_waveform_selected_outputs_zero() {
    let mut o = make_osc(0, 0x1000);
    unsafe { sid_osc_advance(&mut o, 100) };
    assert_eq!(output(&o, &o), 0);
}

#[test]
fn test_bit_resets_and_holds_the_accumulator() {
    let mut o = make_osc(SID_CTRL_SAW, 0x1000);
    unsafe {
        sid_osc_advance(&mut o, 10);
        sid_osc_write_control(&mut o, (SID_CTRL_SAW | SID_CTRL_TEST) as u8);
        sid_osc_advance(&mut o, 10);
    }
    assert_eq!(o.acc, 0);
}

#[test]
fn combined_waveforms_are_anded_for_now() {
    let mut o = make_osc(SID_CTRL_SAW | SID_CTRL_PULSE, 0);
    o.pw = 0x100;
    o.acc = 0x5a5000;
    assert_eq!(output(&o, &o), 0x5a5 & 0xfff);
}

#[test]
fn reset_keeps_the_accumulator() {
    // reSIDfp: "accumulator is not changed on reset".
    let mut o = sid_osc {
        acc: 0x555555,
        freq: 0x1234,
        ..Default::default()
    };
    unsafe { sid_osc_reset(&mut o) };
    assert_eq!(o.acc, 0x555555);
    assert_eq!(o.freq, 0);
}
