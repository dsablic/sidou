// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::path::PathBuf;

use common::panel::{press, ALGORITHM, HOME, SAVE, SELECT};
use common::radio::{held, power_on};
use common::{
    run_past_boot, settings_item, settled_flash, stored_value, toast_text, top_view, Firmware,
    Machine,
};
use fm1_core::Bus;
use sidou_sys::{ilp32, GRID_COLS, VIEWS_SETTINGS_BLUETOOTH};
use sidou_tests::ble_adverts::{ble_midi_packet, midi_advert, named_advert, scan_response};

const FAKE_BUILD: &str = "SIDOU_FM1_BLE_FAKE_BUILD";
const KORG: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
const KORG_NAME: &str = "microKEY Air";
const KORG_LINE: &str = "microKEY A";
const NAME_CHARS: usize = 10;
const PADS: [u8; 6] = [7, 7, 7, 7, 7, 0xC7];
const SPEAKER: [u8; 6] = [9, 9, 9, 9, 9, 9];
const PDU_STRIDE: u32 = 40;
const C4: u8 = 60;
const CUTOFF_CC: u8 = 74;
const RADIO_KEY: u32 = 0x10A;
const MODE_KEYBOARD: u8 = 2;
const ADDR_PUBLIC: u8 = 1;
const RECORD_KEYBOARD: std::ops::Range<usize> = 8..14;
const RECORD_TYPE: usize = 14;
const RECORD_NAME: usize = 15;
const RECORD_VERSION: u8 = 3;
const RECORD_BOND: std::ops::Range<usize> = 32..59;
const BOND_LTK: u8 = 1;
const BOND_AUTH: u8 = 4;
const FAKE_JUST_WORKS: u8 = 1;
const FAKE_PASSKEY: u8 = 2;
const NO_PASSKEY_TYPED_RIGHT: u32 = 999_999_999;
const ROLE_NONE: u8 = 0;
const ROLE_PERIPHERAL: u8 = 1;
const ROLE_CENTRAL: u8 = 2;
const LL_INITIATING: u8 = 3;
const LL_LINKED: u8 = 4;
const MENU_WIPE_MS: u64 = 700;
const TURN_MS: u64 = 100;
const POPUP_GONE_MS: u64 = 2000;
const RADIO_STARTED_MS: u64 = 3500;
const HEARD_MS: u64 = 1500;
const LINK_MS: u64 = 300;
const SAVED_MS: u64 = 2500;
const NOT_RETRIED_MS: u64 = 2000;
const MENU_ROW: u32 = 4;
const MENU_LINE_ROWS: u32 = 3;
const LINE_CHARS: u32 = 14;

fn fake_build(what: &str) -> Option<Firmware> {
    let Some(dir) = std::env::var_os(FAKE_BUILD).filter(|d| !d.is_empty()) else {
        println!(
            "skipping {what}: set {FAKE_BUILD} to a build with -DSIDOU_BLE_FAKE=ON \
             (build/fm1-blefake)"
        );
        return None;
    };
    let dir = std::path::absolute(PathBuf::from(dir)).expect("resolve the fake radio's build");
    Some(Firmware::load(&dir))
}

fn on_the_air(m: &mut Machine, fw: &Firmware, pdus: &[Vec<u8>]) {
    let (at, lens) = (fw.symbol("ble_fake_pdu"), fw.symbol("ble_fake_pdu_len"));
    for (i, p) in pdus.iter().enumerate() {
        let slot = at + PDU_STRIDE * i as u32;
        for (k, b) in p.iter().enumerate() {
            m.soc.write8(slot + k as u32, *b);
        }
        m.soc.write8(lens + i as u32, p.len() as u8);
    }
}

fn nearby(m: &mut Machine, fw: &Firmware) {
    on_the_air(
        m,
        fw,
        &[
            midi_advert(KORG, false),
            scan_response(KORG, false, KORG_NAME),
            named_advert(SPEAKER, false, "SPEAKER"),
            midi_advert(PADS, true),
            scan_response(PADS, true, "PADS"),
        ],
    );
}

fn set(m: &mut Machine, fw: &Firmware, name: &str, value: u8) {
    m.soc.write8(fw.symbol(name), value);
}

fn from_the_keyboard(m: &mut Machine, fw: &Firmware, messages: &[[u8; 3]]) {
    let p = ble_midi_packet(messages);
    let at = fw.symbol("ble_fake_midi");
    for (k, b) in p.iter().enumerate() {
        m.soc.write8(at + k as u32, *b);
    }
    set(m, fw, "ble_fake_midi_len", p.len() as u8);
    m.run_ms(50);
}

fn toasted(m: &mut Machine, fw: &Firmware) -> String {
    let said = toast_text(m, fw);
    let at = fw.symbol("ui") + (ilp32::views::s + ilp32::screen::toast) as u32;
    m.soc.write8(at, 0);
    said
}

fn big_line(m: &mut Machine, fw: &Firmware, row: u32) -> Vec<u8> {
    let src = fw.symbol("cells") + ilp32::grid::src as u32;
    (0..LINE_CHARS)
        .map(|i| m.soc.read8(src + row * GRID_COLS + 1 + 2 * i))
        .collect()
}

fn text(line: &[u8]) -> String {
    String::from_utf8_lossy(line).trim_end().to_owned()
}

fn bluetooth_row(m: &mut Machine, fw: &Firmware) -> String {
    let top = m
        .soc
        .read8(fw.symbol("ui") + (ilp32::views::setup + ilp32::menu::top) as u32);
    let row = MENU_ROW + MENU_LINE_ROWS * (VIEWS_SETTINGS_BLUETOOTH - u32::from(top));
    text(&big_line(m, fw, row))
}

fn device_line(m: &mut Machine, fw: &Firmware, i: u32) -> Vec<u8> {
    big_line(m, fw, MENU_ROW + MENU_LINE_ROWS * i)
}

fn turn(m: &mut Machine, steps: i32) {
    m.turn(ALGORITHM, steps);
    m.run_ms(TURN_MS);
}

fn booted_in_settings(fw: &Firmware) -> Machine {
    let mut m = power_on(fw, &settled_flash(&fw.image));
    run_past_boot(&mut m, fw);
    press(&mut m, HOME);
    m.run_ms(MENU_WIPE_MS);
    settings_item(&mut m, VIEWS_SETTINGS_BLUETOOTH);
    m
}

fn keyboard_mode(fw: &Firmware) -> Machine {
    let mut m = booted_in_settings(fw);
    turn(&mut m, 1);
    turn(&mut m, 1);
    m.run_ms(RADIO_STARTED_MS.saturating_sub(m.ms()) + POPUP_GONE_MS);
    m
}

fn open_devices(m: &mut Machine, fw: &Firmware) {
    press(m, SAVE);
    m.run_ms(HEARD_MS);
    assert_eq!(
        top_view(m, fw),
        fw.symbol("views_devices_view"),
        "SAVE on KEYBOARD opens DEVICES"
    );
}

fn listed(m: &mut Machine, fw: &Firmware) -> Vec<String> {
    (0..3)
        .map(|i| text(&device_line(m, fw, i)[..NAME_CHARS]))
        .collect()
}

fn pick_the_korg(m: &mut Machine, fw: &Firmware) {
    let at = listed(m, fw)
        .iter()
        .position(|n| n == KORG_LINE)
        .expect("the Korg is listed");
    m.turn(SELECT, at as i32);
    m.run_ms(TURN_MS);
    press(m, SAVE);
    m.run_ms(LINK_MS);
}

fn connected(fw: &Firmware) -> Machine {
    let mut m = keyboard_mode(fw);
    nearby(&mut m, fw);
    open_devices(&mut m, fw);
    pick_the_korg(&mut m, fw);
    m
}

fn role(m: &mut Machine, fw: &Firmware) -> u8 {
    m.soc.read8(fw.symbol("ble_radio_role"))
}

fn cutoff(m: &mut Machine, fw: &Firmware) -> u16 {
    m.soc
        .read16(fw.symbol("instrument") + ilp32::live::cutoff as u32)
}

#[test]
fn devices_lists_the_ble_midi_keyboards_heard_and_save_connects_and_keeps_the_chosen_one() {
    let Some(fw) = fake_build("the DEVICES page") else {
        return;
    };
    let mut m = keyboard_mode(&fw);
    assert_eq!(bluetooth_row(&mut m, &fw), "KEYBOARD NONE");
    nearby(&mut m, &fw);
    open_devices(&mut m, &fw);
    let mut names = listed(&mut m, &fw);
    names.sort();
    assert_eq!(
        names,
        ["", "PADS", KORG_LINE],
        "the BLE-MIDI devices, names cut to fit; the speaker is not one"
    );
    let first = device_line(&mut m, &fw, 0);
    assert!(
        first[NAME_CHARS + 1..]
            .iter()
            .all(|&b| b >= sidou_sys::FONT_V1 as u8),
        "signal bars: {first:02x?}"
    );
    toasted(&mut m, &fw);
    pick_the_korg(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("views_settings_view"));
    assert_eq!(toasted(&mut m, &fw), format!("KEYBOARD {KORG_NAME}"));
    assert_eq!(role(&mut m, &fw), ROLE_CENTRAL);
    m.run_ms(POPUP_GONE_MS);
    assert_eq!(bluetooth_row(&mut m, &fw), KORG_NAME);
    m.run_ms(SAVED_MS);
    let rec = stored_value(&m.soc.flash.mem, RADIO_KEY).expect("the radio's record");
    assert_eq!(rec[1], MODE_KEYBOARD);
    assert_eq!(rec[RECORD_KEYBOARD], KORG);
    assert_eq!(rec[RECORD_TYPE], ADDR_PUBLIC);
    assert_eq!(
        &rec[RECORD_NAME..RECORD_NAME + KORG_NAME.len()],
        KORG_NAME.as_bytes()
    );
}

#[test]
fn home_leaves_devices_with_nothing_picked_and_the_row_unchanged() {
    let Some(fw) = fake_build("DEVICES' HOME") else {
        return;
    };
    let mut m = keyboard_mode(&fw);
    nearby(&mut m, &fw);
    open_devices(&mut m, &fw);
    press(&mut m, HOME);
    m.run_ms(MENU_WIPE_MS + LINK_MS);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("views_settings_view"));
    assert_eq!(bluetooth_row(&mut m, &fw), "KEYBOARD NONE");
    assert_eq!(role(&mut m, &fw), ROLE_NONE, "nothing connected");
}

#[test]
fn the_keyboards_notes_and_ccs_play_as_any_midi_input() {
    let Some(fw) = fake_build("the keyboard's notes") else {
        return;
    };
    let mut m = connected(&fw);
    from_the_keyboard(&mut m, &fw, &[[0x90, C4, 100]]);
    assert_eq!(held(&mut m, &fw), [C4], "the note from the keyboard");
    from_the_keyboard(&mut m, &fw, &[[0x80, C4, 0]]);
    assert!(held(&mut m, &fw).is_empty(), "its note off");
    from_the_keyboard(&mut m, &fw, &[[0xB0, CUTOFF_CC, 0]]);
    let closed = cutoff(&mut m, &fw);
    from_the_keyboard(&mut m, &fw, &[[0xB0, CUTOFF_CC, 127]]);
    assert!(
        cutoff(&mut m, &fw) > closed,
        "CC 74 opens the cutoff as over USB"
    );
}

#[test]
fn a_keyboard_gone_releases_its_notes_says_lost_and_reconnects_by_itself() {
    let Some(fw) = fake_build("the keyboard lost") else {
        return;
    };
    let mut m = connected(&fw);
    from_the_keyboard(&mut m, &fw, &[[0x90, C4, 100]]);
    assert_eq!(held(&mut m, &fw), [C4]);
    toasted(&mut m, &fw);
    set(&mut m, &fw, "ble_fake_away", 1);
    m.run_ms(LINK_MS);
    assert!(held(&mut m, &fw).is_empty(), "held notes released");
    assert_eq!(toasted(&mut m, &fw), "KEYBOARD LOST");
    assert_eq!(bluetooth_row(&mut m, &fw), "LOOKING");
    set(&mut m, &fw, "ble_fake_away", 0);
    m.run_ms(LINK_MS);
    assert_eq!(toasted(&mut m, &fw), format!("KEYBOARD {KORG_NAME}"));
    assert_eq!(role(&mut m, &fw), ROLE_CENTRAL);
}

fn read32(m: &mut Machine, fw: &Firmware, name: &str) -> u32 {
    m.soc.read32(fw.symbol(name))
}

fn kept_bond(m: &mut Machine) -> Vec<u8> {
    m.run_ms(SAVED_MS);
    let rec = stored_value(&m.soc.flash.mem, RADIO_KEY).expect("the radio's record");
    assert_eq!(rec[0], RECORD_VERSION, "a bonded record");
    rec[RECORD_BOND].to_vec()
}

fn the_fakes_ltk(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let at = fw.symbol("ble_fake_ltk");
    (0..16).map(|i| m.soc.read8(at + i)).collect()
}

fn paired_with(fw: &Firmware, how: u8) -> Machine {
    let mut m = keyboard_mode(fw);
    nearby(&mut m, fw);
    set(&mut m, fw, "ble_fake_secure", how);
    open_devices(&mut m, fw);
    toasted(&mut m, fw);
    pick_the_korg(&mut m, fw);
    m
}

fn assert_not_tried_again(m: &mut Machine, fw: &Firmware) {
    for _ in 0..NOT_RETRIED_MS / 10 {
        m.run_ms(10);
        let ll = m.soc.read8(fw.symbol("ble_fake_ll"));
        assert!(
            ll != LL_INITIATING && ll != LL_LINKED,
            "tried again at {} ms",
            m.ms()
        );
    }
}

fn away_and_back(m: &mut Machine, fw: &Firmware) {
    set(m, fw, "ble_fake_away", 1);
    m.run_ms(LINK_MS);
    set(m, fw, "ble_fake_away", 0);
    m.run_ms(LINK_MS);
}

#[test]
fn a_keyboard_that_asks_for_pairing_pairs_connects_and_keeps_the_bond() {
    let Some(fw) = fake_build("a Just Works keyboard") else {
        return;
    };
    let mut m = paired_with(&fw, FAKE_JUST_WORKS);
    assert_eq!(toasted(&mut m, &fw), format!("KEYBOARD {KORG_NAME}"));
    assert_eq!(role(&mut m, &fw), ROLE_CENTRAL);
    assert_eq!(read32(&mut m, &fw, "ble_fake_pairings"), 1);
    let bond = kept_bond(&mut m);
    assert_eq!(bond[0], BOND_LTK, "a Just Works bond, not authenticated");
    assert_eq!(
        bond[1..17],
        the_fakes_ltk(&mut m, &fw),
        "the keyboard's LTK"
    );
    away_and_back(&mut m, &fw);
    assert_eq!(role(&mut m, &fw), ROLE_CENTRAL, "back after LOST");
    assert_eq!(
        (
            read32(&mut m, &fw, "ble_fake_pairings"),
            read32(&mut m, &fw, "ble_fake_encryptions")
        ),
        (1, 1),
        "encrypted with the bond, not paired again"
    );
    let flash = m.soc.flash.mem.clone();
    let mut m = power_on(&fw, &flash);
    run_past_boot(&mut m, &fw);
    nearby(&mut m, &fw);
    set(&mut m, &fw, "ble_fake_secure", FAKE_JUST_WORKS);
    set(&mut m, &fw, "ble_fake_bonded", 1);
    let at = fw.symbol("ble_fake_ltk");
    for (i, b) in bond[1..17].iter().enumerate() {
        m.soc.write8(at + i as u32, *b);
    }
    m.run_ms(RADIO_STARTED_MS.saturating_sub(m.ms()) + LINK_MS);
    assert_eq!(role(&mut m, &fw), ROLE_CENTRAL, "after a power cycle");
    assert_eq!(
        (
            read32(&mut m, &fw, "ble_fake_pairings"),
            read32(&mut m, &fw, "ble_fake_encryptions")
        ),
        (0, 1),
        "the kept bond encrypts the link after the power cycle"
    );
}

#[test]
fn a_keyboard_that_lost_the_bond_is_paired_again_once_then_needs_pairing() {
    let Some(fw) = fake_build("a stale bond") else {
        return;
    };
    let mut m = paired_with(&fw, FAKE_JUST_WORKS);
    let first = kept_bond(&mut m);
    toasted(&mut m, &fw);
    set(&mut m, &fw, "ble_fake_forgot", 1);
    away_and_back(&mut m, &fw);
    m.run_ms(LINK_MS);
    assert_eq!(role(&mut m, &fw), ROLE_CENTRAL, "paired again");
    assert_eq!(read32(&mut m, &fw, "ble_fake_pairings"), 2);
    let second = kept_bond(&mut m);
    assert_ne!(first[1..17], second[1..17], "the new bond kept");
    toasted(&mut m, &fw);
    set(&mut m, &fw, "ble_fake_forgot", 1);
    set(&mut m, &fw, "ble_fake_secure", FAKE_PASSKEY);
    m.soc
        .write32(fw.symbol("ble_fake_typed"), NO_PASSKEY_TYPED_RIGHT);
    away_and_back(&mut m, &fw);
    m.run_ms(3 * LINK_MS);
    assert_eq!(read32(&mut m, &fw, "ble_fake_pairings"), 2, "no third bond");
    assert_eq!(toasted(&mut m, &fw), "NEEDS PAIRING");
    assert_eq!(bluetooth_row(&mut m, &fw), "NEEDS PAIRING");
    assert_not_tried_again(&mut m, &fw);
}

#[test]
fn a_keyboard_that_needs_a_passkey_shows_it_large_and_pairs_once_it_is_typed() {
    let Some(fw) = fake_build("a passkey keyboard") else {
        return;
    };
    let mut m = paired_with(&fw, FAKE_PASSKEY);
    m.run_ms(LINK_MS);
    let shown = read32(&mut m, &fw, "ble_fake_shown");
    assert!(shown < 1_000_000, "a passkey waits: {shown:#x}");
    assert_eq!(toasted(&mut m, &fw), format!("PASSKEY {shown:06}"));
    assert_eq!(
        role(&mut m, &fw),
        ROLE_CENTRAL,
        "the link waits for the passkey"
    );
    m.run_ms(NOT_RETRIED_MS);
    m.soc.write32(fw.symbol("ble_fake_typed"), shown);
    m.run_ms(LINK_MS);
    assert_eq!(toasted(&mut m, &fw), format!("KEYBOARD {KORG_NAME}"));
    let bond = kept_bond(&mut m);
    assert_eq!(bond[0], BOND_LTK | BOND_AUTH, "an authenticated bond");
}

#[test]
fn a_passkey_typed_wrong_says_needs_pairing_and_is_not_tried_again() {
    let Some(fw) = fake_build("a passkey typed wrong") else {
        return;
    };
    let mut m = paired_with(&fw, FAKE_PASSKEY);
    m.run_ms(LINK_MS);
    let shown = read32(&mut m, &fw, "ble_fake_shown");
    toasted(&mut m, &fw);
    m.soc
        .write32(fw.symbol("ble_fake_typed"), (shown + 1) % 1_000_000);
    m.run_ms(LINK_MS);
    assert_eq!(toasted(&mut m, &fw), "NEEDS PAIRING");
    assert_not_tried_again(&mut m, &fw);
}

#[test]
fn keyboard_drops_a_computers_link_never_advertises_and_on_lets_it_back() {
    let Some(fw) = fake_build("KEYBOARD and a computer") else {
        return;
    };
    let mut m = booted_in_settings(&fw);
    turn(&mut m, 1);
    m.run_ms(RADIO_STARTED_MS.saturating_sub(m.ms()));
    set(&mut m, &fw, "ble_fake_computer", 1);
    m.run_ms(LINK_MS);
    assert_eq!(role(&mut m, &fw), ROLE_PERIPHERAL, "the Mac connected");
    toasted(&mut m, &fw);
    turn(&mut m, 1);
    m.run_ms(LINK_MS);
    assert_eq!(role(&mut m, &fw), ROLE_NONE, "KEYBOARD dropped the Mac");
    let adverts = m.soc.read32(fw.symbol("ble_fake_advertised"));
    m.run_ms(NOT_RETRIED_MS);
    assert_eq!(
        m.soc.read32(fw.symbol("ble_fake_advertised")),
        adverts,
        "KEYBOARD advertised"
    );
    assert_eq!(role(&mut m, &fw), ROLE_NONE);
    turn(&mut m, -1);
    m.run_ms(LINK_MS);
    assert_eq!(
        role(&mut m, &fw),
        ROLE_PERIPHERAL,
        "back to ON, the Mac connects"
    );
    assert_eq!(toasted(&mut m, &fw), "", "no keyboard toast for the Mac");
}

#[test]
fn the_kept_keyboard_is_looked_for_and_connected_after_a_power_cycle() {
    let Some(fw) = fake_build("the keyboard after a power cycle") else {
        return;
    };
    let mut m = connected(&fw);
    m.run_ms(SAVED_MS);
    let flash = m.soc.flash.mem.clone();
    let mut m = power_on(&fw, &flash);
    run_past_boot(&mut m, &fw);
    nearby(&mut m, &fw);
    toasted(&mut m, &fw);
    m.run_ms(RADIO_STARTED_MS.saturating_sub(m.ms()) + LINK_MS);
    assert_eq!(
        role(&mut m, &fw),
        ROLE_CENTRAL,
        "connected to the kept keyboard"
    );
    assert_eq!(toasted(&mut m, &fw), format!("KEYBOARD {KORG_NAME}"));
}
