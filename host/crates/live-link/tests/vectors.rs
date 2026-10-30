// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The test vectors of docs/reference/live-link.md, byte for byte, and the reference decoder
//! on them.

use fm1_live_link::*;

fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}

#[test]
fn the_host_frames_are_the_specs_vectors() {
    assert_eq!(start(), hex("F0 7D 46 4D 31 4C 01 00 01 7E F7"));
    assert_eq!(heartbeat(), hex("F0 7D 46 4D 31 4C 02 7E F7"));
    assert_eq!(stop(), hex("F0 7D 46 4D 31 4C 03 7D F7"));
    assert_eq!(keyframe(), hex("F0 7D 46 4D 31 4C 04 7C F7"));
    assert_eq!(font(), hex("F0 7D 46 4D 31 4C 05 7B F7"));
    assert_eq!(palette(), hex("F0 7D 46 4D 31 4C 06 7A F7"));
    // HOME (button 8) held and pressed, K1 (encoder 2) a detent clockwise
    assert_eq!(
        panel(1 << 8, 1 << 8, &[0, 0, 1]),
        hex("F0 7D 46 4D 31 4C 07 00 00 01 00 01 00 00 01 76 F7")
    );
}

#[test]
fn packing_round_trips_and_refuses_what_is_not_canonical() {
    for n in 0..=PAYLOAD_MAX {
        let p: Vec<u8> = (0..n).map(|i| (i * 37 + 200) as u8).collect();
        let packed = pack(&p);
        assert_eq!(packed.len(), n + n.div_ceil(7));
        assert_eq!(unpack(&packed).as_deref(), Some(&p[..]));
        let f = frame(CELLS, &p);
        assert_eq!(parse(&f), Some((CELLS, p.clone())));
        if n > 0 {
            let mut bad = f.clone();
            bad[7] ^= 1; // a mask bit flipped: the checksum fails
            assert_eq!(parse(&bad), None);
        }
    }
    assert_eq!(unpack(&[0x01]), None, "a mask with no byte");
    assert_eq!(unpack(&[0x02, 0x10]), None, "a mask bit with no byte");
}

fn mirror() -> Mirror {
    let mut m = Mirror::default();
    let mut hello = vec![1, 7, 30, 30, 8, 8, 16, 0, 0xdc, 1, 14, 30, 14, 7];
    hello.extend([0; 8]);
    hello.extend(b"TEST");
    assert!(m.take(&frame(HELLO, &hello)));
    m
}

#[test]
fn cells_frames_decode_into_the_mirror() {
    let mut m = mirror();
    assert_eq!(m.hello.as_ref().unwrap().name, "TEST");
    assert!(m.take(&hex("F0 7D 46 4D 31 4C 46 00 00 00 41 0E 00 6B F7")));
    assert_eq!(m.cells[0].ch, b'A');
    assert_eq!(m.cells[0].attr, 0x0e);
    // an overlay cell (colour 3) and a piece (the top left of an 'A' at twice the size)
    assert!(m.take(&hex(
        "F0 7D 46 4D 31 4C 46 20 05 03 20 0E 13 00 40 00 00 00 00 00 00 01 01 00 7E 41 50 F7"
    )));
    let c = m.cells[5 * 30 + 3];
    assert_eq!((c.ch, c.x, c.mask[0], c.mask[7]), (0x20, 0x13, 0x80, 0x01));
    let p = m.cells[5 * 30 + 4];
    assert_eq!((p.ch, p.attr, p.x), (0x01, 0x7e, b'A'));
    assert_eq!(m.rejected, 0);
}

#[test]
fn a_piece_draws_its_part_of_the_character_scaled() {
    let mut m = mirror();
    m.font[usize::from(b'A')] = [0x80, 0, 0, 0, 0, 0, 0, 0x01];
    for i in 0..16u16 {
        m.palette[usize::from(i)] = i;
    }
    // the four pieces of a 2x character, fg 1 on bg 0
    for (k, piece) in (1u8..=4).enumerate() {
        assert!(m.take(&frame(CELLS, &[0, k as u8, piece, 0x01, b'A'])));
    }
    let fb = m.render();
    let w = 240;
    // the glyph's top-left pixel is a 2 x 2 block in piece 1 (cell 0)
    assert_eq!(
        [fb[0], fb[1], fb[w], fb[w + 1], fb[2]],
        [1, 1, 1, 1, 0],
        "piece 1's top left"
    );
    // its bottom-right pixel the bottom-right 2 x 2 of piece 4 (cell 3, the lower right part)
    assert_eq!([fb[7 * w + 3 * 8 + 7], fb[6 * w + 3 * 8 + 6]], [1, 1]);
}
