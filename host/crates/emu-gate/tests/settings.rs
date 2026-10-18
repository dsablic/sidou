// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The settings on the device (app/settings.h, M2a Task 5), in fm1-emu with its flash kept
//! across power cycles: CARDS, the envelope mode and a learned CC survive one; the full boot
//! plays once after an install (no settings, or another version's), then the short one; SAVE's
//! press stores them at once; a foreign store reads foreign until the menu's confirm formats it.

mod common;

use std::ffi::c_void;

use common::{build_dir, run_past_boot, stored_value, top_view, Firmware, Machine};
use fm1_core::Bus;
use sidou_sys::ilp32;
use sidou_sys::*;

const SELECT: u8 = 0;
const ALGORITHM: u8 = 1;
const K1: u8 = 2;
const HOME: u8 = 8;
const SAVE: u8 = 9;
/// A CC the map leaves free (docs/reference/midi.md); the envelope mode's (app/midi_map.c).
const CC_FREE: u8 = 14;
const CC_ENVELOPE: u8 = 26;
/// The record's byte of the envelope mode: 6 header bytes, then parameters 0..35, the cutoff and
/// the pulse width two bytes each (app/settings.h).
const REC_ENVELOPE: usize = 6 + LIVE_PARAM_ENVELOPE_MODE as usize + 2;

fn fixture(what: &str) -> Option<Firmware> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    Some(Firmware::load(&build))
}

fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

/// Powered on with `flash`, past the boot.
fn power_on(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    run_past_boot(&mut m, fw);
    m
}

/// The boot the views played at this power-on (platform/fm1/ui.c `ui`, its `boot`): 1 full.
fn full_boot(m: &mut Machine, fw: &Firmware) -> bool {
    let a = fw.symbol("ui") + (ilp32::views::boot + ilp32::boot::full) as u32;
    m.soc.read8(a) != 0
}

fn cards(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc.read8(fw.symbol("ui") + ilp32::views::cards as u32)
}

fn envelope(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc
        .read8(fw.symbol("instrument") + ilp32::live::envelope_mode as u32)
}

/// CC 14's bound map row in platform/fm1/midi.c `learned` (MIDI_LEARN_NONE: none).
fn cc_free_row(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc
        .read8(fw.symbol("learned") + (ilp32::midi_learn::cc_row + CC_FREE as usize) as u32)
}

fn store_state(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc
        .read8(fw.symbol("store_fm1_store") + ilp32::store::state as u32)
}

/// The menu open, its cursor on `item` (fm1-emu turns an encoder a detent every 8 ms).
fn menu_item(m: &mut Machine, item: u32) {
    press(m, HOME);
    m.run_ms(700);
    m.turn(SELECT, -(VIEWS_MENU_ITEMS as i32));
    m.run_ms(10 * u64::from(VIEWS_MENU_ITEMS));
    m.turn(SELECT, item as i32);
    m.run_ms(10 * u64::from(item) + 50);
}

/// Until the settings stored differ from `before` (or `ms` passed): the time it took.
fn stored_within(m: &mut Machine, before: &Option<Vec<u8>>, ms: u64) -> Option<u64> {
    let start = m.ms();
    while m.ms() < start + ms {
        m.run_ms(50);
        if stored_value(&m.soc.flash.mem, SETTINGS_KEY) != *before {
            return Some(m.ms() - start);
        }
    }
    None
}

#[test]
fn cards_the_envelope_mode_and_a_learned_cc_survive_a_power_cycle() {
    let Some(fw) = fixture("the settings test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    assert!(
        !full_boot(&mut m, &fw),
        "a second power-on plays the short boot"
    );
    assert_eq!(
        (cards(&mut m, &fw), envelope(&mut m, &fw)),
        (1, LIVE_ENVELOPE_INSTANT as u8)
    );
    // CARDS off in the menu (ALGORITHM counter-clockwise), back to LIVE.
    menu_item(&mut m, VIEWS_MENU_CARDS);
    m.turn(ALGORITHM, -1);
    m.run_ms(100);
    press(&mut m, HOME);
    m.run_ms(700);
    // The envelope mode over MIDI; CC 14 learned for K1's cutoff.
    m.midi(&[0xB0, CC_ENVELOPE, 127]);
    m.run_ms(50);
    m.key(SAVE, true);
    m.run_ms(30);
    m.turn(K1, 1);
    m.run_ms(30);
    m.key(SAVE, false);
    m.run_ms(30);
    m.midi(&[0xB0, CC_FREE, 64]);
    m.run_ms(200);
    let want = (0, LIVE_ENVELOPE_AUTHENTIC as u8, cc_free_row(&mut m, &fw));
    assert_eq!(
        (cards(&mut m, &fw), envelope(&mut m, &fw)),
        (want.0, want.1)
    );
    assert_ne!(want.2, MIDI_LEARN_NONE as u8, "CC 14 not learned");
    // The debounce passes (no toast, no SAVE), and the power goes.
    let before = stored_value(&m.soc.flash.mem, SETTINGS_KEY);
    let took = stored_within(&mut m, &before, 8_000).expect("never stored");
    println!("the background save landed {took} ms after the last change");
    assert!(took >= 4_000, "stored {took} ms in: before the debounce");
    let flash = m.soc.flash.mem.clone();
    let mut m = power_on(&fw, &flash);
    assert!(!full_boot(&mut m, &fw), "the short boot");
    assert_eq!(
        (
            cards(&mut m, &fw),
            envelope(&mut m, &fw),
            cc_free_row(&mut m, &fw)
        ),
        want,
        "CARDS, the envelope mode and CC 14's binding are back"
    );
}

// ---- a host store over a flash copy, writable ----

unsafe extern "C" fn copy_map(ctx: *mut c_void, off: u32) -> *const u8 {
    unsafe { ctx.cast::<u8>().add(off as usize) }
}

unsafe extern "C" fn copy_prog(ctx: *mut c_void, off: u32, p: *const c_void, n: u32) -> i32 {
    for i in 0..n as usize {
        unsafe { *ctx.cast::<u8>().add(off as usize + i) &= *p.cast::<u8>().add(i) };
    }
    0
}

unsafe extern "C" fn copy_erase(ctx: *mut c_void, off: u32) -> i32 {
    for i in 0..STORE_SECTOR as usize {
        unsafe { *ctx.cast::<u8>().add(off as usize + i) = 0xFF };
    }
    0
}

/// `key`'s value replaced by `data` in a flash copy, as an older firmware would have stored it.
fn put(flash: &mut [u8], key: u32, data: &[u8]) {
    let f = store_flash {
        map: Some(copy_map),
        prog: Some(copy_prog),
        erase: Some(copy_erase),
        ctx: flash.as_mut_ptr().cast(),
    };
    let mut st: Box<store> = Box::default();
    unsafe {
        store_init(&mut *st, &f, 256);
        store_mount(&mut *st);
        store_quiet(&mut *st, 1);
        while store_poll(&mut *st) != 0 {}
        assert_eq!(
            store_put(
                &mut *st,
                key,
                SETTINGS_TYPE as u8,
                data.as_ptr().cast(),
                data.len() as u32
            ),
            STORE_OK
        );
        while store_poll(&mut *st) != 0 {}
    }
}

#[test]
fn the_full_boot_plays_once_after_an_install_then_the_short_one() {
    let Some(fw) = fixture("the settings test") else {
        return;
    };
    // A blank flash: the first power-on.
    let mut m = Machine::boot_blank(&fw.image);
    run_past_boot(&mut m, &fw);
    assert!(
        full_boot(&mut m, &fw),
        "the first boot after an install is the full one"
    );
    m.run_ms(common::SETTLE_MS);
    let mut flash = m.soc.flash.mem.clone();
    let rec = stored_value(&flash, SETTINGS_KEY).expect("the version stored");
    let mut m = power_on(&fw, &flash);
    assert!(!full_boot(&mut m, &fw), "then the short one");
    // An older firmware's record: the full boot once, its settings kept, then the short one.
    let mut old = rec[..SETTINGS_V1_BYTES as usize].to_vec();
    old[0] = 1;
    let v = SETTINGS_V1_BYTES as usize - SETTINGS_VERSION_CHARS as usize;
    old[v..].fill(0);
    old[v..v + 6].copy_from_slice(b"V0.0.1");
    old[1] = 0; // CARDS off
    put(&mut flash, SETTINGS_KEY, &old);
    let mut m = power_on(&fw, &flash);
    assert!(full_boot(&mut m, &fw), "a new version: the full boot");
    assert_eq!(cards(&mut m, &fw), 0, "its settings still apply");
    m.run_ms(common::SETTLE_MS);
    let flash = m.soc.flash.mem.clone();
    let mut m = power_on(&fw, &flash);
    assert!(!full_boot(&mut m, &fw), "once");
    assert_eq!(cards(&mut m, &fw), 0);
}

#[test]
fn saves_press_on_live_stores_the_settings_within_a_second() {
    let Some(fw) = fixture("the settings test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("live_view_view"));
    m.midi(&[0xB0, CC_ENVELOPE, 127]);
    m.run_ms(50);
    let before = stored_value(&m.soc.flash.mem, SETTINGS_KEY);
    assert_eq!(before.as_ref().unwrap()[REC_ENVELOPE], 0);
    press(&mut m, SAVE);
    let took = stored_within(&mut m, &before, 1_000).expect("not stored within a second");
    println!("SAVE stored the settings in {took} ms");
    let after = stored_value(&m.soc.flash.mem, SETTINGS_KEY).unwrap();
    assert_eq!(after[REC_ENVELOPE], LIVE_ENVELOPE_AUTHENTIC as u8);
}

#[test]
fn a_foreign_store_formats_on_the_menus_confirm_and_then_keeps_the_settings() {
    let Some(fw) = fixture("the settings test") else {
        return;
    };
    let mut flash = Machine::boot_blank(&fw.image).soc.flash.mem.clone();
    flash[(STORE_LO + 7 * STORE_SECTOR) as usize + 123] = 0x5A; // another firmware's byte
    let mut m = power_on(&fw, &flash);
    assert_eq!(store_state(&mut m, &fw), STORE_STATE_FOREIGN as u8);
    menu_item(&mut m, VIEWS_MENU_STORE);
    // One turn arms, nothing written; the second formats: every sector erased, the audio held
    // through each.
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    let region = (STORE_LO as usize)..(STORE_HI as usize);
    assert_eq!(
        m.soc.flash.mem[region.clone()],
        flash[region.clone()],
        "armed only"
    );
    m.turn(ALGORITHM, 1);
    let start = m.ms();
    // Every sector erased (forced: the user asked), then the settings stored again.
    while stored_value(&m.soc.flash.mem, SETTINGS_KEY).is_none() {
        m.run_ms(100);
        assert!(m.ms() < start + 30_000, "the format never ended");
    }
    println!(
        "the format and the settings' save took {} ms",
        m.ms() - start
    );
    for sector in m.soc.flash.mem[region].chunks(STORE_SECTOR as usize) {
        assert!(
            sector.iter().all(|&b| b == 0xFF) || sector[..8] == *b"SIDOUSTR",
            "a sector neither erased nor SIDOU's"
        );
    }
    let flash = m.soc.flash.mem.clone();
    let mut m = power_on(&fw, &flash);
    assert_eq!(store_state(&mut m, &fw), STORE_STATE_SIDOU as u8);
    assert!(!full_boot(&mut m, &fw));
}
