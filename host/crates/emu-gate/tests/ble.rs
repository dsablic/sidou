// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::panel::{press, ALGORITHM, HOME};
use common::{
    firmware, put_value, settled_flash, stored_value, Firmware, Machine, REGION_FREE_MIN,
};
use fm1_core::Bus;
use sidou_sys::{ilp32, LIVE_VOICES, VIEWS_SETTINGS_BLUETOOTH};

const C4: u8 = 60;
const NOINIT: std::ops::Range<u32> = 0x01C7_C000..0x01C7_FD50;
const RAM_START: u32 = 0x01C0_E000;
const POOL_START: u32 = 0x01C1_E000;
const BOOT_MS: u64 = 500;
const ROLE_PERIPHERAL: u8 = 1;
const ROLE_NONE: u8 = 0;
const RADIO_KEY: u32 = 0x10A;
const RADIO_TYPE: u8 = 1;
const ADDRESS: [u8; 6] = [1, 2, 3, 4, 5, 0xC6];
const START_AFTER_MS: u64 = 3000;
const RADIO_READY: u8 = 1;
const START_HOLD_MS_MAX: f64 = 600.0;
const CPU_HZ: f64 = 240e6;
const MENU_WIPE_MS: u64 = 700;
const P3_ANA_CON0: u16 = 0x00;
const P3_ANA_CON6: u16 = 0x06;
const P3_ANA_CON7: u16 = 0x07;
const P3_ANA_CON9: u16 = 0x09;
const BOOT_LOADER_VDC14: u8 = 3;
const BOOT_LOADER_SYSVDD: u8 = 11;
const DCDC13_EN: u8 = 0x08;
const STOCK_DCDC_TUNE: u8 = 0xA5;
const START_MARK: u32 = 0x4254_5354;

fn section(fw: &Firmware, name: &str) -> (u32, u64) {
    use object::{Object, ObjectSection};
    let file = object::File::parse(&*fw.elf).expect("the firmware ELF");
    file.section_by_name(name)
        .map_or((0, 0), |s| (s.address() as u32, s.size()))
}

fn held(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let voices = fw.symbol("instrument") + ilp32::live::voice as u32;
    let mut notes = Vec::new();
    for i in 0..LIVE_VOICES {
        let v = voices + i * ilp32::live_voice::SIZE as u32;
        if m.soc.read8(v + ilp32::live_voice::gate as u32) != 0 {
            notes.push(m.soc.read8(v + ilp32::live_voice::note as u32));
        }
    }
    notes
}

fn from_the_air(m: &mut Machine, fw: &Firmware, pkt: u32) {
    let (ring, w) = (fw.symbol("ble_radio_in"), fw.symbol("ble_radio_in_w"));
    let n = m.soc.read32(w);
    let slots = fw.function_range("ble_radio_in").len() as u32 / 4;
    m.soc.write32(ring + 4 * (n % slots), pkt);
    m.soc.write32(w, n + 1);
}

fn note_on(note: u8) -> u32 {
    u32::from_le_bytes([0x09, 0x90, note, 100])
}

#[test]
fn the_stack_lives_in_noinit_and_the_ram_region_keeps_its_floor() {
    let Some(fw) = firmware("the Bluetooth test") else {
        return;
    };
    let (lo, hi) = (fw.symbol("_ble_noinit_start"), fw.symbol("_ble_noinit_end"));
    assert!(NOINIT.contains(&lo) && hi <= NOINIT.end && hi > lo);
    for ring in ["ble_radio_in", "ble_radio_out", "in_ring"] {
        let at = fw.symbol(ring);
        assert!(NOINIT.contains(&at), "{ring} at {at:#x}, outside NOINIT");
    }
    let used = section(&fw, ".data").1 + section(&fw, ".bss").1 + section(&fw, ".song_ram").1;
    let free = u64::from(POOL_START - RAM_START) - used;
    println!(
        "the RAM region: {free} bytes free with Bluetooth, NOINIT {} bytes",
        section(&fw, ".noinit").1
    );
    assert!(
        free >= REGION_FREE_MIN,
        "the RAM region has {free} bytes free"
    );
}

#[test]
fn notes_from_the_air_play_and_a_dropped_link_releases_them() {
    let Some(fw) = firmware("the Bluetooth test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.run_ms(BOOT_MS);
    let role = fw.symbol("ble_radio_role");
    m.soc.write8(role, ROLE_PERIPHERAL);
    m.soc.write8(fw.symbol("ble_radio_awake"), 1);
    from_the_air(&mut m, &fw, note_on(C4));
    m.run_ms(50);
    assert_eq!(held(&mut m, &fw), [C4], "the note from Bluetooth");
    m.soc.write8(role, ROLE_NONE);
    m.run_ms(50);
    assert!(
        held(&mut m, &fw).is_empty(),
        "the link dropped with the note held"
    );
}

fn power_on(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    m
}

#[test]
fn bluetooth_on_is_kept_and_the_radio_starts_after_the_boot_holding_the_loop_briefly() {
    let Some(fw) = firmware("the Bluetooth test") else {
        return;
    };
    let mut flash = settled_flash(&fw.image);
    put_value(
        &mut flash,
        RADIO_KEY,
        RADIO_TYPE,
        &[1, 1, 1, 2, 3, 4, 5, 0xC6],
    );
    let mut m = power_on(&fw, &flash);
    let state = fw.symbol("ble_radio_state");
    m.watch_calls(fw.symbol("ble_radio_poll"));
    m.run_ms(START_AFTER_MS - 500);
    assert_eq!(
        m.soc.read8(state),
        0,
        "the radio started before the boot's waits"
    );
    m.run_ms(1500);
    assert_eq!(
        m.soc.read8(state),
        RADIO_READY,
        "BLUETOOTH ON did not start the radio"
    );
    let addr: Vec<u8> = (0..6)
        .map(|i| m.soc.read8(fw.symbol("ble_radio_addr") + i))
        .collect();
    assert_eq!(addr, ADDRESS, "the kept address");
    let hold = *m.watched_calls.iter().max().expect("the poll ran") as f64 / CPU_HZ * 1e3;
    println!("the radio's start held the main loop {hold:.1} ms in fm1-emu (no radio: its waits time out)");
    assert!(
        hold <= START_HOLD_MS_MAX,
        "the start held the main loop {hold:.1} ms"
    );
}

#[test]
fn bluetooth_set_on_is_saved_with_an_address_and_off_stops_advertising() {
    let Some(fw) = firmware("the Bluetooth test") else {
        return;
    };
    let mut m = power_on(&fw, &settled_flash(&fw.image));
    common::run_past_boot(&mut m, &fw);
    press(&mut m, HOME);
    m.run_ms(MENU_WIPE_MS);
    common::settings_item(&mut m, VIEWS_SETTINGS_BLUETOOTH);
    m.turn(ALGORITHM, 1);
    m.run_ms(START_AFTER_MS + 2500);
    let rec = stored_value(&m.soc.flash.mem, RADIO_KEY).expect("the radio's record");
    assert_eq!(rec[..2], [1, 1], "version 1, ON");
    assert_eq!(rec[7] & 0xC0, 0xC0, "a random static address: {rec:02x?}");
    m.turn(ALGORITHM, -1);
    m.run_ms(200);
    assert_ne!(
        m.soc.read8(fw.symbol("ble_radio_state")),
        RADIO_READY,
        "OFF left it advertising"
    );
}

fn p33_set(m: &mut Machine, reg: u16, value: u8) {
    use fm1_soc::p33::P33;
    const SELECT_AND_SHIFT: u32 = 0x11;
    for b in [((reg >> 8) & 3) as u8, reg as u8, value] {
        m.soc.p33.write(P33::DAT, u32::from(b));
        m.soc.p33.write(P33::CON, SELECT_AND_SHIFT);
    }
    m.soc.p33.write(P33::CON, 0);
}

fn as_the_boot_loader_leaves_the_rails(m: &mut Machine) {
    p33_set(m, P3_ANA_CON6, BOOT_LOADER_VDC14);
    p33_set(m, P3_ANA_CON9, BOOT_LOADER_SYSVDD);
    p33_set(m, P3_ANA_CON0, 0x04);
}

fn rails(m: &Machine) -> (u8, u8, u8, u8) {
    let r = |a| m.soc.p33.reg(a, false);
    (
        r(P3_ANA_CON6) & 7,
        r(P3_ANA_CON9) & 15,
        r(P3_ANA_CON0) & DCDC13_EN,
        r(P3_ANA_CON7),
    )
}

#[test]
fn the_boot_raises_the_rails_and_sets_the_power_up_before_the_radio_can_start() {
    let Some(fw) = firmware("the Bluetooth test") else {
        return;
    };
    let mut m = power_on(&fw, &settled_flash(&fw.image));
    as_the_boot_loader_leaves_the_rails(&mut m);
    m.run_ms(100);
    assert_eq!(
        m.soc.read8(fw.symbol("ble_radio_state")),
        0,
        "the radio is not started yet"
    );
    assert_eq!(
        rails(&m),
        (7, 15, DCDC13_EN, STOCK_DCDC_TUNE),
        "VDC14 7, SYSVDD 15, the DC-DC on and tuned"
    );
    let report: Vec<u8> = (0..4)
        .map(|i| m.soc.read8(fw.symbol("ble_power_report") + i))
        .collect();
    assert_eq!(
        report,
        [BOOT_LOADER_VDC14, BOOT_LOADER_SYSVDD, 7, 15],
        "the rails read at boot, before and after"
    );
}

#[test]
fn with_bluetooth_off_the_main_loop_leaves_the_radio_alone() {
    let Some(fw) = firmware("the Bluetooth test") else {
        return;
    };
    let mut m = power_on(&fw, &settled_flash(&fw.image));
    common::run_past_boot(&mut m, &fw);
    m.run_ms(START_AFTER_MS);
    m.watch_calls(fw.symbol("ble_radio_poll"));
    m.run_ms(START_AFTER_MS);
    assert!(
        m.watched_calls.is_empty(),
        "the main loop polled the radio {} times with BLUETOOTH OFF",
        m.watched_calls.len()
    );
    assert_eq!(
        m.soc.read8(fw.symbol("ble_radio_state")),
        0,
        "the radio started"
    );
}

#[test]
fn bluetooth_on_is_saved_only_after_the_radio_started() {
    let Some(fw) = firmware("the Bluetooth test") else {
        return;
    };
    let mut m = power_on(&fw, &settled_flash(&fw.image));
    common::run_past_boot(&mut m, &fw);
    m.run_ms(START_AFTER_MS);
    press(&mut m, HOME);
    m.run_ms(MENU_WIPE_MS);
    common::settings_item(&mut m, VIEWS_SETTINGS_BLUETOOTH);
    let state = fw.symbol("ble_radio_state");
    m.turn(ALGORITHM, 1);
    let mut started_at = None;
    for _ in 0..4000 {
        m.run_ms(1);
        let saved = stored_value(&m.soc.flash.mem, RADIO_KEY).is_some_and(|r| r[1] == 1);
        if started_at.is_none() && m.soc.read8(state) == RADIO_READY {
            started_at = Some(m.ms());
        }
        if saved {
            assert!(
                started_at.is_some(),
                "ON was saved before the radio had started"
            );
            return;
        }
    }
    panic!("ON was never saved");
}

#[test]
fn a_reset_during_the_radio_start_boots_with_bluetooth_off() {
    let Some(fw) = firmware("the Bluetooth test") else {
        return;
    };
    let mut flash = settled_flash(&fw.image);
    put_value(
        &mut flash,
        RADIO_KEY,
        RADIO_TYPE,
        &[1, 1, 1, 2, 3, 4, 5, 0xC6],
    );
    let mut m = power_on(&fw, &flash);
    m.soc.write32(fw.symbol("ble_radio_start_mark"), START_MARK);
    m.run_ms(START_AFTER_MS + 3000);
    assert_ne!(
        m.soc.read8(fw.symbol("ble_radio_state")),
        RADIO_READY,
        "the radio started again"
    );
    let rec = stored_value(&m.soc.flash.mem, RADIO_KEY).expect("the radio's record");
    assert_eq!(
        rec[..2],
        [1, 0],
        "BLUETOOTH saved OFF after the start that reset"
    );
    assert_eq!(rec[2..], ADDRESS, "the address kept");
}
