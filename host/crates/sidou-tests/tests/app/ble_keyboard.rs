// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::ble_adverts::{midi_advert, named_advert, scan_response};

const KORG: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
const NAMELESS: [u8; 6] = [0x2A, 0x3F, 0, 0, 0, 0xC1];
const LONG: [u8; 6] = [1, 2, 3, 4, 5, 0xC2];
const SPEAKER: [u8; 6] = [9, 9, 9, 9, 9, 9];
const OWN: [u8; 6] = [1, 2, 3, 4, 5, 0xC6];
const NAME_MAX: usize = BLE_KEYBOARD_NAME as usize;
const RECORD: usize = 1 + size_of::<ble_setting_kept>();

fn listed(heard: &[(Vec<u8>, u32)]) -> Vec<(String, [u8; 6], u8, u8)> {
    let mut t: Box<ble_scan_tab> = Box::default();
    let mut out: [ble_keyboard; 8] = Default::default();
    let mut bars = [0u8; 8];
    unsafe {
        ble_scan_clear(&mut *t);
        for (p, times) in heard {
            for i in 0..*times {
                ble_scan_add(&mut *t, p.as_ptr(), p.len() as u32, 0, 100 + i);
            }
        }
        let n = ble_keyboard_list(&*t, out.as_mut_ptr(), bars.as_mut_ptr()) as usize;
        out[..n]
            .iter()
            .zip(bars)
            .map(|(k, b)| (text(&k.name), k.addr, k.type_, b))
            .collect()
    }
}

fn text(name: &[std::os::raw::c_char]) -> String {
    name.iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect()
}

fn name_bytes(s: &str) -> [std::os::raw::c_char; NAME_MAX + 1] {
    let mut n = [0; NAME_MAX + 1];
    for (d, b) in n.iter_mut().zip(s.bytes()) {
        *d = b as std::os::raw::c_char;
    }
    n
}

fn read(rec: &[u8]) -> Option<ble_setting_kept> {
    let mut kept = ble_setting_kept::default();
    (unsafe { ble_record_read(&mut kept, rec.as_ptr(), rec.len() as u32) } != 0).then_some(kept)
}

#[test]
fn the_list_holds_only_ble_midi_devices_first_heard_first_with_names_cut_at_16_and_bars() {
    let speaker = named_advert(SPEAKER, false, "SPEAKER");
    let list = listed(&[
        (speaker, 9),
        (midi_advert(KORG, false), 8),
        (scan_response(KORG, false, "microKEY Air"), 1),
        (midi_advert(NAMELESS, true), 4),
        (midi_advert(LONG, true), 1),
        (scan_response(LONG, true, "A VERY LONG KEYBOARD NAME"), 1),
    ]);
    let pub_ = BLE_ADDR_PUBLIC as u8;
    let rnd = BLE_ADDR_RANDOM as u8;
    assert_eq!(
        list,
        [
            ("microKEY Air".to_owned(), KORG, pub_, 3),
            ("MIDI 3F2A".to_owned(), NAMELESS, rnd, 2),
            ("A VERY LONG KEYB".to_owned(), LONG, rnd, 1),
        ],
        "the speaker is not listed; the strongest has three bars; a nameless one shows its address"
    );
}

fn kept_with(bond: ble_bond) -> ble_setting_kept {
    ble_setting_kept {
        mode: BLE_MODE_KEYBOARD as u8,
        own: OWN,
        keyboard: ble_keyboard {
            addr: KORG,
            type_: BLE_ADDR_RANDOM as u8,
            name: name_bytes("SIXTEEN CHARS OK"),
        },
        bond,
    }
}

const BOND: ble_bond = ble_bond {
    has: (BLE_BOND_LTK | BLE_BOND_AUTH) as u8,
    ltk: [0xA5; 16],
    rand: [1, 2, 3, 4, 5, 6, 7, 8],
    ediv: [0x34, 0x12],
};

fn written(kept: &ble_setting_kept) -> [u8; RECORD] {
    let mut rec = [0u8; RECORD];
    unsafe { ble_record_write(kept, rec.as_mut_ptr()) };
    rec
}

#[test]
fn a_kept_keyboard_and_its_bond_read_back_as_written() {
    let kept = kept_with(BOND);
    let rec = written(&kept);
    assert_eq!(
        rec[..2],
        [BLE_RECORD_VERSION as u8, BLE_MODE_KEYBOARD as u8]
    );
    let back = read(&rec).expect("a version 3 record");
    assert_eq!(
        (back.mode, back.own, back.keyboard.addr, back.keyboard.type_),
        (kept.mode, OWN, KORG, BLE_ADDR_RANDOM as u8)
    );
    assert_eq!(text(&back.keyboard.name), "SIXTEEN CHARS OK");
    assert_eq!(
        (back.bond.has, back.bond.ltk, back.bond.rand, back.bond.ediv),
        (BOND.has, BOND.ltk, BOND.rand, BOND.ediv)
    );
}

#[test]
fn a_record_from_1_11_reads_its_keyboard_with_no_bond() {
    let rec = written(&kept_with(BOND));
    let unbonded = RECORD - size_of::<ble_bond>();
    let old = [&[BLE_RECORD_VERSION_UNBONDED as u8][..], &rec[1..unbonded]].concat();
    let mut back = kept_with(BOND);
    assert_ne!(
        unsafe { ble_record_read(&mut back, old.as_ptr(), old.len() as u32) },
        0,
        "the 1.11 record"
    );
    assert_eq!(
        (back.mode, back.keyboard.addr, text(&back.keyboard.name)),
        (BLE_MODE_KEYBOARD as u8, KORG, "SIXTEEN CHARS OK".to_owned())
    );
    assert_eq!(back.bond.has, 0, "no bond");
}

#[test]
fn a_record_from_1_9_or_1_10_reads_as_off_or_on_with_no_keyboard() {
    for (on, mode) in [(0u8, BLE_MODE_OFF), (1, BLE_MODE_ON)] {
        let old = [&[BLE_RECORD_VERSION_ON_OFF as u8, on][..], &OWN[..]].concat();
        let kept = read(&old).expect("the 1.9 record");
        assert_eq!(
            (kept.mode, kept.own, kept.keyboard.type_),
            (mode as u8, OWN, BLE_ADDR_NONE as u8)
        );
    }
}

#[test]
fn a_record_of_another_length_or_version_is_not_read() {
    let mut rec = [0u8; RECORD];
    rec[0] = BLE_RECORD_VERSION as u8;
    assert!(read(&rec).is_some());
    assert!(read(&rec[..RECORD - 1]).is_none(), "cut short");
    rec[0] = BLE_RECORD_VERSION_UNBONDED as u8;
    assert!(
        read(&rec).is_none(),
        "a 1.11 record as long as a bonded one"
    );
    rec[0] = BLE_RECORD_VERSION as u8 + 1;
    assert!(read(&rec).is_none(), "a newer version");
    assert!(read(&[]).is_none(), "empty");
}
