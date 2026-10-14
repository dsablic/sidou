// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! USB-MIDI event packets to the MIDI byte stream (`app/midi_usb.c`).

use sidou_sys::*;

/// A packet as the endpoint holds it: `[cable << 4 | cin, m0, m1, m2]`, read little-endian.
fn packet(cable: u8, cin: u8, m: [u8; 3]) -> u32 {
    u32::from_le_bytes([cable << 4 | cin, m[0], m[1], m[2]])
}

fn bytes(p: u32) -> Vec<u8> {
    let mut out = [0xEEu8; 3];
    let n = unsafe { midi_usb_bytes(p, out.as_mut_ptr()) } as usize;
    assert!(n <= 3, "{n} bytes from one packet");
    assert!(
        out[n..].iter().all(|&b| b == 0xEE),
        "wrote past its count: {out:02x?} (count {n})"
    );
    out[..n].to_vec()
}

#[test]
fn every_cin_gives_its_byte_count() {
    let cases: [(u8, [u8; 3], &[u8]); 20] = [
        (0x0, [0x90, 0x3C, 0x64], &[]),
        (0x1, [0x90, 0x3C, 0x64], &[]),
        (0x2, [0xF1, 0x12, 0x00], &[0xF1, 0x12]),
        (0x2, [0xF3, 0x05, 0x00], &[0xF3, 0x05]),
        (0x3, [0xF2, 0x01, 0x02], &[0xF2, 0x01, 0x02]),
        (0x4, [0xF0, 0x7E, 0x7F], &[0xF0, 0x7E, 0x7F]),
        (0x4, [0x01, 0x02, 0x03], &[0x01, 0x02, 0x03]),
        (0x5, [0xF7, 0x00, 0x00], &[0xF7]),
        (0x5, [0xF6, 0x00, 0x00], &[0xF6]),
        (0x6, [0x01, 0xF7, 0x00], &[0x01, 0xF7]),
        (0x6, [0xF0, 0xF7, 0x00], &[0xF0, 0xF7]),
        (0x7, [0x01, 0x02, 0xF7], &[0x01, 0x02, 0xF7]),
        (0x8, [0x80, 0x3C, 0x40], &[0x80, 0x3C, 0x40]),
        (0x9, [0x9F, 0x3C, 0x64], &[0x9F, 0x3C, 0x64]),
        (0xA, [0xA2, 0x3C, 0x10], &[0xA2, 0x3C, 0x10]),
        (0xB, [0xB0, 0x4A, 0x7F], &[0xB0, 0x4A, 0x7F]),
        (0xC, [0xC5, 0x07, 0x00], &[0xC5, 0x07]),
        (0xD, [0xD0, 0x40, 0x00], &[0xD0, 0x40]),
        (0xE, [0xE3, 0x00, 0x40], &[0xE3, 0x00, 0x40]),
        (0xF, [0xF8, 0x00, 0x00], &[0xF8]),
    ];
    for (cin, m, want) in cases {
        assert_eq!(bytes(packet(0, cin, m)), want, "CIN {cin:#x} with {m:02x?}");
    }
}

#[test]
fn every_real_time_byte_passes_as_a_single_byte() {
    for rt in 0xF8..=0xFFu8 {
        assert_eq!(bytes(packet(0, 0xF, [rt, 0, 0])), [rt]);
    }
    // CIN 0xF also carries unparsed single bytes, a SysEx data byte among them.
    assert_eq!(bytes(packet(0, 0xF, [0x42, 0, 0])), [0x42]);
}

#[test]
fn the_cable_number_is_ignored() {
    for cable in 0..16 {
        assert_eq!(
            bytes(packet(cable, 0x9, [0x90, 0x3C, 0x64])),
            [0x90, 0x3C, 0x64],
            "cable {cable}"
        );
    }
}

#[test]
fn a_channel_message_whose_status_does_not_match_its_cin_is_dropped() {
    for cin in 0x8..=0xEu8 {
        for status in (0x80..=0xF0u8).step_by(0x10) {
            let p = packet(0, cin, [status | 3, 0x10, 0x20]);
            let got = bytes(p);
            if status >> 4 == cin {
                assert!(!got.is_empty(), "CIN {cin:#x} status {status:#x} dropped");
            } else {
                assert!(
                    got.is_empty(),
                    "CIN {cin:#x} with status {status:#x} gave {got:02x?}"
                );
            }
        }
    }
}

/// A SysEx message as a USB host sends it: CIN 4 for each full 3 bytes but the last, then
/// CIN 5, 6 or 7 for the 1, 2 or 3 bytes that end it.
fn sysex_packets(msg: &[u8]) -> Vec<u32> {
    let chunks: Vec<&[u8]> = msg.chunks(3).collect();
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let cin = if i + 1 < chunks.len() {
                0x4
            } else {
                0x4 + c.len() as u8
            };
            let mut m = [0u8; 3];
            m[..c.len()].copy_from_slice(c);
            packet(0, cin, m)
        })
        .collect()
}

#[test]
fn sysex_of_every_length_comes_back_whole() {
    for payload in 0..=12u8 {
        let mut msg = vec![0xF0];
        msg.extend((0..payload).map(|i| i * 7 % 128));
        msg.push(0xF7);
        let got: Vec<u8> = sysex_packets(&msg).into_iter().flat_map(bytes).collect();
        assert_eq!(got, msg, "a SysEx with {payload} data bytes");
    }
}

#[test]
fn packets_through_the_parser_give_the_messages() {
    use std::ffi::c_void;
    unsafe extern "C" fn on_msg(ctx: *mut c_void, m: *const midi_msg) {
        unsafe { &mut *ctx.cast::<Vec<midi_msg>>() }.push(unsafe { *m });
    }
    let mut got: Box<Vec<midi_msg>> = Box::default();
    let mut p = Box::<midi_in>::default();
    let ctx: *mut Vec<midi_msg> = &mut *got;
    unsafe { midi_in_init(&mut *p, Some(on_msg), None, ctx.cast()) };
    let mut stream = vec![
        packet(0, 0x9, [0x91, 60, 100]),
        packet(0, 0xF, [0xF8, 0, 0]),
    ];
    stream.extend(sysex_packets(&[0xF0, 0x7D, 1, 2, 3, 4, 0xF7]));
    stream.extend([packet(0, 0xC, [0xC0, 2, 0]), packet(0, 0x8, [0x81, 60, 0])]);
    for pk in stream {
        let b = bytes(pk);
        unsafe { midi_in_feed(&mut *p, b.as_ptr(), b.len()) };
    }
    let kinds: Vec<(midi_kind, u8, u8)> =
        got.iter().map(|m| (m.kind, m.channel, m.data1)).collect();
    assert_eq!(
        kinds,
        [
            (MIDI_NOTE_ON, 1, 60),
            (MIDI_REALTIME, 0, 0xF8),
            (MIDI_PROGRAM, 0, 2),
            (MIDI_NOTE_OFF, 1, 60),
        ]
    );
}
