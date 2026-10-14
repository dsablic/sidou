// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use sidou_sys::*;

fn setup() -> (sid_osc, sid_osc) {
    let mut target = sid_osc::default();
    let mut src = sid_osc::default();
    unsafe {
        sid_osc_reset(&mut target);
        sid_osc_reset(&mut src);
        sid_osc_write_control(&mut target, (SID_CTRL_SAW | SID_CTRL_SYNC) as u8);
        sid_osc_write_control(&mut src, SID_CTRL_SAW as u8);
    }
    target.freq = 0x1000;
    target.acc = 0x123456;
    src.freq = 0x2000;
    src.acc = 0x7ff000;
    (target, src)
}

#[test]
fn resets_target_when_source_msb_rises_on_the_last_cycle() {
    let (mut target, mut src) = setup();
    unsafe { sid_osc_advance(&mut src, 1, 0, 0) }; // 0x7ff000 -> 0x801000: rose this cycle
    assert_eq!(src.msb_rose, 1);
    unsafe { sid_osc_sync(&mut target, &src) };
    assert_eq!(target.acc, 0);
}

#[test]
fn target_keeps_cycles_that_elapsed_after_the_rise() {
    let (mut target, mut src) = setup();
    unsafe { sid_osc_advance(&mut src, 2, 0, 0) }; // rose one full cycle before the end of the step
    assert_eq!(src.msb_rose, 1);
    unsafe { sid_osc_sync(&mut target, &src) };
    assert_eq!(target.acc, 0x1000);
}

#[test]
fn no_rise_no_reset() {
    let (_target, mut src) = setup();
    src.acc = 0;
    unsafe { sid_osc_advance(&mut src, 2, 0, 0) };
    assert_eq!(src.msb_rose, 0);
}

/// A noise-combined sync source runs cycle by cycle; its overshoot must still count to
/// the end of the step, as sid_osc_sync expects.
#[test]
fn a_cycle_by_cycle_source_reports_overshoot_at_the_end_of_the_step() {
    for control in [0x91u8, 0xa1, 0xc1] {
        let mut src = sid_osc::default();
        unsafe {
            sid_osc_reset(&mut src);
            sid_osc_write_control(&mut src, control);
        }
        src.freq = 0x2000;
        src.acc = 0x7fe000; // rises in the first cycle of a 20-cycle step
        unsafe { sid_osc_advance(&mut src, 20, 0, 0) };
        if src.msb_rose == 0 {
            continue; // the 6581 pulled the MSB low (sawtooth combination)
        }
        assert_eq!(
            src.overshoot,
            (src.acc + 0x1000000 - 0x800000) & 0xffffff,
            "{control:#x}"
        );
        assert_eq!(src.overshoot / src.freq, 19, "{control:#x}");
    }
}
