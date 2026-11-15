// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

const ADV_IND: u8 = 0x00;
const SCAN_RSP: u8 = 0x04;
const TX_RANDOM: u8 = 0x40;
const AD_FLAGS: u8 = 0x01;
const AD_UUID128_COMPLETE: u8 = 0x07;
const AD_NAME_COMPLETE: u8 = 0x09;
const LE_GENERAL_NO_BR_EDR: u8 = 0x06;
pub const MIDI_SERVICE: [u8; 16] = [
    0x00, 0xC7, 0xC4, 0x4E, 0xE3, 0x6C, 0x51, 0xA7, 0x33, 0x4B, 0xE8, 0xED, 0x5A, 0x0E, 0xB8, 0x03,
];

fn ad(kind: u8, data: &[u8]) -> Vec<u8> {
    [&[data.len() as u8 + 1, kind][..], data].concat()
}

fn pdu(kind: u8, addr: [u8; 6], random: bool, ads: &[Vec<u8>]) -> Vec<u8> {
    let data = ads.concat();
    let header = kind | if random { TX_RANDOM } else { 0 };
    [&[header, 6 + data.len() as u8][..], &addr[..], &data[..]].concat()
}

pub fn midi_advert(addr: [u8; 6], random: bool) -> Vec<u8> {
    pdu(
        ADV_IND,
        addr,
        random,
        &[
            ad(AD_FLAGS, &[LE_GENERAL_NO_BR_EDR]),
            ad(AD_UUID128_COMPLETE, &MIDI_SERVICE),
        ],
    )
}

pub fn named_advert(addr: [u8; 6], random: bool, name: &str) -> Vec<u8> {
    pdu(
        ADV_IND,
        addr,
        random,
        &[
            ad(AD_FLAGS, &[LE_GENERAL_NO_BR_EDR]),
            ad(AD_NAME_COMPLETE, name.as_bytes()),
        ],
    )
}

pub fn scan_response(addr: [u8; 6], random: bool, name: &str) -> Vec<u8> {
    pdu(
        SCAN_RSP,
        addr,
        random,
        &[ad(AD_NAME_COMPLETE, name.as_bytes())],
    )
}

pub fn ble_midi_packet(messages: &[[u8; 3]]) -> Vec<u8> {
    const HEADER: u8 = 0x80;
    const TIMESTAMP: u8 = 0x80;
    let mut p = vec![HEADER];
    for m in messages {
        p.push(TIMESTAMP);
        p.extend_from_slice(m);
    }
    p
}
