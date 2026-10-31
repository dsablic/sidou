// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::{press, ALGORITHM, HOME, K1, SAVE};
use common::{
    booted, firmware, put_value, run_past_boot, stored_value, top_view, Firmware, Machine,
};
use fm1_core::Bus;
use sidou_sys::ilp32;
use sidou_sys::*;

const MENU_WIPE_MS: u64 = 700;
const CC_FREE: u8 = 14;
const FOREIGN_BYTE: u8 = 0x5A;
const CC_ENVELOPE: u8 = 26;
const REC_CARDS: usize = 1;
const REC_ENVELOPE: usize = 6 + LIVE_PARAM_ENVELOPE_MODE as usize + 2;

fn power_on(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    run_past_boot(&mut m, fw);
    m
}

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

fn cc_free_row(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc
        .read8(fw.symbol("learned") + (ilp32::midi_learn::cc_row + CC_FREE as usize) as u32)
}

fn store_state(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc
        .read8(fw.symbol("store_fm1_store") + ilp32::store::state as u32)
}

fn setting_item(m: &mut Machine, item: u32) {
    press(m, HOME);
    m.run_ms(MENU_WIPE_MS);
    common::settings_item(m, item);
}

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
    let Some(fw) = firmware("the settings test") else {
        return;
    };
    let mut m = booted(&fw);
    assert!(
        !full_boot(&mut m, &fw),
        "a second power-on plays the short boot"
    );
    assert_eq!(
        (cards(&mut m, &fw), envelope(&mut m, &fw)),
        (1, LIVE_ENVELOPE_INSTANT as u8)
    );
    setting_item(&mut m, VIEWS_SETTINGS_CARDS);
    m.turn(ALGORITHM, -1);
    m.run_ms(100);
    press(&mut m, HOME);
    m.run_ms(300);
    press(&mut m, HOME);
    m.run_ms(700);
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

#[test]
fn the_full_boot_plays_once_after_an_install_then_the_short_one() {
    let Some(fw) = firmware("the settings test") else {
        return;
    };
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
    let mut old = rec[..SETTINGS_V1_BYTES as usize].to_vec();
    old[0] = 1;
    let v = SETTINGS_V1_BYTES as usize - SETTINGS_VERSION_CHARS as usize;
    old[v..].fill(0);
    old[v..v + 6].copy_from_slice(b"V0.0.1");
    old[REC_CARDS] = 0;
    put_value(&mut flash, SETTINGS_KEY, SETTINGS_TYPE as u8, &old);
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
    let Some(fw) = firmware("the settings test") else {
        return;
    };
    let mut m = booted(&fw);
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
fn a_foreign_store_formats_on_the_menus_save_confirm_and_then_keeps_the_settings() {
    let Some(fw) = firmware("the settings test") else {
        return;
    };
    let mut flash = Machine::boot_blank(&fw.image).soc.flash.mem.clone();
    flash[(STORE_LO + 7 * STORE_SECTOR) as usize + 123] = FOREIGN_BYTE;
    let mut m = power_on(&fw, &flash);
    assert_eq!(store_state(&mut m, &fw), STORE_STATE_FOREIGN as u8);
    setting_item(&mut m, VIEWS_SETTINGS_STORE);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    let region = (STORE_LO as usize)..(STORE_HI as usize);
    assert_eq!(
        m.soc.flash.mem[region.clone()],
        flash[region.clone()],
        "armed only"
    );
    m.turn(ALGORITHM, 6);
    m.turn(ALGORITHM, 1);
    m.run_ms(300);
    assert_eq!(
        m.soc.flash.mem[region.clone()],
        flash[region.clone()],
        "a fast spin formats nothing"
    );
    press(&mut m, SAVE);
    let start = m.ms();
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
