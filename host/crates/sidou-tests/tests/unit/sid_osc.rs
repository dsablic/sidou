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
    unsafe { sid_osc_advance(&mut o, 3, 0, 0) };
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
        unsafe { sid_osc_advance(&mut o, CYCLES, 0, 0) };
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
    unsafe { sid_osc_advance(&mut o, 3, 0, 0) };
    assert_eq!(output(&o, &o), 3);
}

#[test]
fn triangle_folds_on_the_msb() {
    // reSIDfp's table: the top 12 bits folded, shifted up one, so bit 0 is always 0.
    let mut o = make_osc(SID_CTRL_TRI, 0);
    o.acc = 0x400000;
    assert_eq!(output(&o, &o), 0x800);
    o.acc = 0x7fffff;
    assert_eq!(output(&o, &o), 0xffe);
    o.acc = 0xc00000;
    assert_eq!(output(&o, &o), 0x7fe);
}

#[test]
fn ring_mod_inverts_the_msb_while_the_source_msb_is_low() {
    // reSIDfp: index = acc ^ (~source & 0x800000).
    let mut o = make_osc(SID_CTRL_TRI | SID_CTRL_RING, 0);
    let mut src = make_osc(SID_CTRL_SAW, 0);
    o.acc = 0x400000;
    src.acc = 0x800000;
    assert_eq!(output(&o, &src), 0x800);
    src.acc = 0x000000;
    assert_eq!(output(&o, &src), 0x7fe);
    // With sawtooth selected too, ring modulation does not touch the index.
    o.control = (SID_CTRL_TRI | SID_CTRL_SAW | SID_CTRL_RING) as u8;
    unsafe { sid_osc_update_mode(&mut o) };
    let plain = output(&o, &src);
    o.control = (SID_CTRL_TRI | SID_CTRL_SAW) as u8;
    unsafe { sid_osc_update_mode(&mut o) };
    assert_eq!(output(&o, &src), plain);
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
    unsafe { sid_osc_advance(&mut o, 100, 0, 0) };
    assert_eq!(output(&o, &o), 0);
}

#[test]
fn test_bit_resets_and_holds_the_accumulator() {
    let mut o = make_osc(SID_CTRL_SAW, 0x1000);
    unsafe {
        sid_osc_advance(&mut o, 10, 0, 0);
        sid_osc_write_control(&mut o, (SID_CTRL_SAW | SID_CTRL_TEST) as u8);
        sid_osc_advance(&mut o, 10, 0, 0);
    }
    assert_eq!(o.acc, 0);
}

/// Every combination of waveforms, in every profile and on both models, goes
/// through reSIDfp's pulldown table: the AND of the waveforms, then the table.
#[test]
fn combined_waveforms_match_re_sid_fp_tables() {
    use residfp_sys::{pulldown_table, CombinedWaveforms, Model};
    let grid = [
        CombinedWaveforms::Weak,
        CombinedWaveforms::Average,
        CombinedWaveforms::Strong,
    ];
    assert_eq!(grid.len() as u32, SID_PROFILES);
    for (is6581, model) in [(1, Model::Mos6581), (0, Model::Csg8580)] {
        for (profile, cws) in grid.into_iter().enumerate() {
            let table = pulldown_table(model, cws);
            for row in &table {
                // A locked-up noise register relies on this: an output of 0 stays 0.
                assert_eq!(row[0], 0);
            }
            let mut o = make_osc(0, 0);
            unsafe {
                sid_osc_set_model(&mut o, is6581);
                sid_osc_set_profile(&mut o, profile as u32);
            }
            for (control, row) in [(0x30u8, 0usize), (0x50, 1), (0x60, 2), (0x70, 3)] {
                o.control = control;
                unsafe { sid_osc_update_mode(&mut o) };
                o.pw = 0x100;
                for ix in 0..0x1000u32 {
                    o.acc = ix << 12;
                    let saw = ix;
                    let tri = (if ix & 0x800 != 0 { ix ^ 0xfff } else { ix } << 1) & 0xfff;
                    let ramp = match control {
                        0x30 | 0x70 => saw & (saw << 1),
                        0x50 => tri,
                        _ => saw,
                    };
                    let pulse = if control & 0x40 == 0 || ix >= 0x100 {
                        0xfff
                    } else {
                        0
                    };
                    let want = u32::from(table[row][(ramp & pulse) as usize]);
                    assert_eq!(
                        output(&o, &o),
                        want,
                        "{model:?} {cws:?} {control:#04x} {ix:#x}"
                    );
                }
            }
        }
    }
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

/// sid_wave_ramp is reSIDfp's waveform table, all 4 x 4096 entries (so the triangle's
/// bit 0 is always 0).
#[test]
fn wave_ramp_is_re_sid_fp_wave_table() {
    let table = residfp_sys::wave_table();
    for (kind, row) in table.iter().enumerate() {
        for (ix, &want) in row.iter().enumerate() {
            let got = unsafe { sid_wave_ramp(kind as u32, ix as u32) };
            assert_eq!(got, u32::from(want), "kind {kind}, ix {ix:#x}");
        }
    }
}
