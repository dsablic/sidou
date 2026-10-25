// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! LAB on the device (docs/plans/2026-10-25-lab-plan.md), in fm1-emu with its flash kept across
//! a power cycle: a sound kept in a USER slot is the store's file and comes back at power-on as
//! the sound playing; the unsaved edit comes back with its edited mark.

mod common;

use fm1_core::Bus;

use common::{build_dir, run_past_boot, stored_file, stored_value, Firmware, Machine, SETTLE_MS};
use sidou_sys::*;

/// The store's test hook (platform/fm1/store_fm1.h store_fm1_test): its op, key, result, data.
const HOOK_KEY: u32 = 4;
const HOOK_RESULT: u32 = 12;
const HOOK_DATA: u32 = 16;
const KEEP: u32 = 7; /* STORE_FM1_KEEP */
const BYTES: usize = LAB_SOUND_BYTES as usize;
const STRIDE: usize = USER_SOUND_STRIDE as usize;

fn fixture() -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message("the LAB test");
        return None;
    };
    Some(Firmware::load(&build))
}

/// Powered on with `flash`, past the boot.
fn power_on(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    run_past_boot(&mut m, fw);
    m
}

fn live_at(fw: &Firmware, field: usize) -> u32 {
    fw.symbol("instrument") + field as u32
}

/// The current sound's base as the firmware's live has it (lab_sound.h's image).
fn base(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let mut s = vec![0u8; BYTES];
    let a = live_at(fw, ilp32::live::base);
    for (i, b) in s.iter_mut().enumerate() {
        *b = m.soc.read8(a + i as u32);
    }
    s
}

fn base_name(m: &mut Machine, fw: &Firmware) -> String {
    let a = live_at(fw, ilp32::live::base_name);
    (0..LAB_SOUND_NAME)
        .map(|i| m.soc.read8(a + i))
        .take_while(|&c| c != 0)
        .map(char::from)
        .collect()
}

/// The fine tune NRPN (0/7, LIVE_PARAM_TUNE) and the resonance CC, over TRS: an edit that is
/// the sound's alone (the tune) and one the settings record has too.
fn edit(m: &mut Machine, tune: u8, resonance: u8) {
    m.midi(&[0xB0, 99, 0, 98, 7, 6, 0, 38, tune]);
    m.midi(&[0xB0, LIVE_CC_RESONANCE as u8, resonance]);
    m.run_ms(100);
}

/// Waits until `done` holds of the flash, for up to `ms`.
fn until(m: &mut Machine, ms: u64, what: &str, done: impl Fn(&[u8]) -> bool) {
    let end = m.ms() + ms;
    while !done(&m.soc.flash.mem) {
        assert!(m.ms() < end, "{what} never reached the flash");
        m.run_ms(200);
    }
}

#[test]
fn a_kept_sound_survives_a_power_cycle() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    edit(&mut m, 70, 127);
    let tune = m.soc.read8(live_at(&fw, ilp32::live::tune)) as i8;
    assert_eq!(tune, 20, "the NRPN tuned it +20 cents");
    // Kept in USER 3 through the store's test hook (LAB's SAVE is Task 8's).
    let h = fw.symbol("store_fm1_hook");
    m.soc.write32(h + HOOK_KEY, 2);
    for (i, &b) in b"LAB 03\0".iter().enumerate() {
        m.soc.write8(h + HOOK_DATA + i as u32, b);
    }
    m.soc.write32(h, KEEP);
    let end = m.ms() + 5_000;
    while m.soc.read32(h) != 0 {
        assert!(m.ms() < end, "the keep never ran");
        m.run_ms(10);
    }
    assert_eq!(m.soc.read32(h + HOOK_RESULT) as i32, 0, "queued");
    let kept = base(&mut m, &fw); // a keep makes the sound playing the base
    assert_eq!(kept[LAB_SOUND_TUNE as usize] as i8, 20);
    assert_eq!(m.soc.read8(live_at(&fw, ilp32::live::user_sound)), 3);
    until(&mut m, 20_000, "the USER sounds' file", |f| {
        stored_file(f, "USER SOUNDS")
            .is_some_and(|(_, b)| b[USER_SOUND_HEADER as usize + 2 * STRIDE] == 1)
    });
    let (typ, file) = stored_file(&m.soc.flash.mem, "USER SOUNDS").unwrap();
    assert_eq!(u32::from(typ), STORE_FILE_SOUNDS);
    assert_eq!(file.len(), USER_SOUND_FILE_BYTES as usize);
    let at = USER_SOUND_HEADER as usize + 2 * STRIDE;
    assert_eq!(&file[at + 1..at + 7], b"LAB 03");
    assert_eq!(file[at + 13..at + STRIDE], kept);
    // the settings follow: their preset byte names USER 3
    until(&mut m, SETTLE_MS + 5_000, "the settings' USER 3", |f| {
        stored_value(f, SETTINGS_KEY).is_some_and(|r| r[4] == LIVE_PRESET_SLOTS as u8 + 2)
    });
    let flash = m.soc.flash.mem.clone();
    let mut m = power_on(&fw, &flash);
    assert_eq!(
        m.soc.read16(live_at(&fw, ilp32::live::sounds_valid)),
        1 << 2,
        "USER 3 holds a sound"
    );
    assert_eq!(
        m.soc.read8(live_at(&fw, ilp32::live::user_sound)),
        3,
        "and plays"
    );
    assert_eq!(base(&mut m, &fw), kept, "its base, as kept");
    assert_eq!(base_name(&mut m, &fw), "LAB 03");
    assert_eq!(m.soc.read8(live_at(&fw, ilp32::live::tune)) as i8, 20);
}

#[test]
fn the_edit_comes_back_at_power_on() {
    let Some(fw) = fixture() else { return };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    edit(&mut m, 30, 0);
    let tune = m.soc.read8(live_at(&fw, ilp32::live::tune)) as i8;
    assert_eq!(tune, -20);
    until(&mut m, SETTLE_MS + 5_000, "the unsaved edit", |f| {
        stored_value(f, USER_SOUND_EDIT_KEY)
            .is_some_and(|r| r[LAB_SOUND_TUNE as usize + 16] as i8 == -20)
    });
    let rec = stored_value(&m.soc.flash.mem, USER_SOUND_EDIT_KEY).unwrap();
    assert_eq!(rec.len(), USER_SOUND_EDIT_BYTES as usize);
    assert_eq!(rec[..3], [1, 0, 1], "version, SAW LEAD, edited");
    let flash = m.soc.flash.mem.clone();
    let mut m = power_on(&fw, &flash);
    assert_eq!(
        m.soc.read8(live_at(&fw, ilp32::live::tune)) as i8,
        -20,
        "the tune back"
    );
    assert_eq!(m.soc.read8(live_at(&fw, ilp32::live::resonance)), 0);
    // the base is SAW LEAD's, so the sound shows edited (its tune is not the base's)
    let base = base(&mut m, &fw);
    assert_eq!(base[LAB_SOUND_TUNE as usize], 0);
    assert_eq!(base_name(&mut m, &fw), "SAW LEAD");
}
