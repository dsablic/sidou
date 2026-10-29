// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The screen font: app/font.c and app/font.h are generated from assets/font/sidou-8x8.txt
//! (sidou_tests::font). SIDOU_FONT_UPDATE=1 rewrites them.

use sidou_sys::font_glyphs;
use sidou_tests::font::{self, Font};

fn sheet() -> Font {
    font::load()
}

#[test]
fn generated_files_are_up_to_date() {
    let update = std::env::var("SIDOU_FONT_UPDATE").as_deref() == Ok("1");
    let stale = font::check_or_write(&sheet(), update);
    assert!(
        stale.is_empty(),
        "{stale:?} are stale; rerun with SIDOU_FONT_UPDATE=1"
    );
    let f = sheet();
    for text in [font::c_header(&f), font::c_source(&f)] {
        for line in text.lines() {
            assert!(
                line.len() <= 100,
                "generated line over 100 columns: {line:?}"
            );
        }
    }
}

#[test]
fn the_compiled_table_is_the_sheet() {
    let f = sheet();
    let table = unsafe { font_glyphs };
    assert_eq!(table.len(), font::LAST + 1);
    for (code, got) in table.iter().enumerate() {
        assert_eq!(got, &f.glyphs[code], "glyph {code:#04x}");
    }
    for code in font::LAST + 1..font::GLYPHS {
        assert_eq!(f.glyphs[code], [0; 8], "{code:#04x} is not blank but is not in flash");
    }
}

#[test]
fn printable_ascii_is_complete_and_lower_case_is_upper_case() {
    let f = sheet();
    for c in 0x21u8..0x7f {
        assert!(
            f.glyphs[usize::from(c)].iter().any(|&r| r != 0),
            "{:?} has no glyph",
            char::from(c)
        );
    }
    assert_eq!(f.glyphs[0x20], [0; 8], "space is not blank");
    for c in b'a'..=b'z' {
        let e = f.entries.iter().find(|e| e.code == c).unwrap();
        assert_eq!(e.alias_of, Some(c - 0x20), "{:?}", char::from(c));
    }
}

#[test]
fn text_glyphs_leave_a_column_and_a_row_between_cells() {
    let f = sheet();
    for c in 0x21u8..0x7f {
        let g = f.glyphs[usize::from(c)];
        assert_eq!(g[7], 0, "{:?} uses the bottom row", char::from(c));
        assert!(
            g.iter().all(|r| r & 1 == 0),
            "{:?} uses the right column",
            char::from(c)
        );
    }
}

fn count(g: &[u8; 8]) -> u32 {
    g.iter().map(|r| r.count_ones()).sum()
}

#[test]
fn block_glyphs_fill_what_their_names_say() {
    let f = sheet();
    let g = |name: &str| f.glyphs[usize::from(f.code(name))];
    assert_eq!(g("FULL"), [0xff; 8]);
    for q in 0..16u8 {
        let glyph = g(&format!("Q{q}"));
        for (bit, (x, y)) in [(0, 0), (4, 0), (0, 4), (4, 4)].into_iter().enumerate() {
            let on = q >> bit & 1 != 0;
            for dy in 0..4 {
                for dx in 0..4 {
                    assert_eq!(f.pixel(f.code(&format!("Q{q}")), x + dx, y + dy), on);
                }
            }
        }
        assert_eq!(count(&glyph), 16 * q.count_ones());
    }
    for n in 0..=8u32 {
        assert_eq!(count(&g(&format!("V{n}"))), 8 * n, "V{n}");
        assert_eq!(count(&g(&format!("H{n}"))), 8 * n, "H{n}");
        assert_eq!(count(&g(&format!("BAR{n}"))), 6 * n, "BAR{n}");
    }
    // Lower eighths fill from the bottom, left eighths from the left.
    assert_eq!(g("V1"), [0, 0, 0, 0, 0, 0, 0, 0xff]);
    assert_eq!(g("H1"), [0x80; 8]);
    for n in 0..=16u32 {
        assert_eq!(count(&g(&format!("D{n}"))), 4 * n, "D{n}");
    }
    // Each dither level adds pixels to the one below it (ordered dithering).
    for n in 1..=16 {
        let (lo, hi) = (g(&format!("D{}", n - 1)), g(&format!("D{n}")));
        assert!(lo.iter().zip(&hi).all(|(l, h)| l & !h == 0), "D{n}");
    }
}

#[test]
fn box_lines_meet_at_the_cell_edges() {
    let f = sheet();
    // The 2-pixel line runs through rows 3-4 and columns 3-4, so frames join up.
    let edges = |name: &str| {
        let c = f.code(name);
        (
            f.pixel(c, 3, 0) && f.pixel(c, 4, 0),
            f.pixel(c, 3, 7) && f.pixel(c, 4, 7),
            f.pixel(c, 0, 3) && f.pixel(c, 0, 4),
            f.pixel(c, 7, 3) && f.pixel(c, 7, 4),
        )
    };
    // (up, down, left, right)
    for (name, want) in [
        ("BOX_H", (false, false, true, true)),
        ("BOX_V", (true, true, false, false)),
        ("BOX_TL", (false, true, false, true)),
        ("BOX_TR", (false, true, true, false)),
        ("BOX_BL", (true, false, false, true)),
        ("BOX_BR", (true, false, true, false)),
        ("ROUND_TL", (false, true, false, true)),
        ("ROUND_TR", (false, true, true, false)),
        ("ROUND_BL", (true, false, false, true)),
        ("ROUND_BR", (true, false, true, false)),
        ("TEE_DOWN", (false, true, true, true)),
        ("TEE_UP", (true, false, true, true)),
        ("TEE_RIGHT", (true, true, false, true)),
        ("TEE_LEFT", (true, true, true, false)),
        ("CROSS", (true, true, true, true)),
    ] {
        assert_eq!(edges(name), want, "{name}: (up, down, left, right)");
    }
}

#[test]
fn the_sheet_rejects_malformed_glyphs() {
    for (bad, why) in [
        ("0x41 'A'\n########\n", "fewer than 8 rows"),
        ("0x41 'A'\n#######\n", "8 of"),
        ("0x42 'B'\n........\n........\n........\n........\n........\n........\n........\n........\n0x41 'A' = 0x42\n", "out of order"),
        ("0x61 'a' = 0x41\n", "not a drawn glyph"),
        ("0x41 bad\n", "neither"),
    ] {
        let e = font::parse(bad).expect_err(why);
        assert!(e.contains(why), "{e:?} does not say {why:?}");
    }
}

/// font8x8_basic (public domain, test data only): `[code][row]`, bit 0 the leftmost pixel.
fn font8x8_basic() -> Vec<[u8; 8]> {
    include_str!("../data/font8x8_basic.h")
        .lines()
        .filter_map(|l| l.trim().strip_prefix('{')?.split_once('}'))
        .map(|(bytes, _)| {
            let mut g = [0u8; 8];
            for (row, b) in g.iter_mut().zip(bytes.split(',')) {
                let b = b.trim().trim_start_matches("0x");
                // Mirror into the sheet's order, bit 7 leftmost.
                *row = u8::from_str_radix(b, 16).expect("hex byte").reverse_bits();
            }
            g
        })
        .collect()
}

/// Pixels that differ between `a` and `b` moved right by `dx` and down by `dy`.
fn distance(a: &[u8; 8], b: &[u8; 8], dx: i32, dy: i32) -> u32 {
    (0..8i32)
        .map(|y| {
            let row = usize::try_from(y - dy)
                .ok()
                .and_then(|r| b.get(r))
                .copied()
                .unwrap_or(0);
            let row = if dx >= 0 { row >> dx } else { row << -dx };
            (a[y as usize] ^ row).count_ones()
        })
        .sum()
}

/// Glyphs whose shape is forced at 8x8 with 2-pixel strokes: a dot, a tick, a bar, or a
/// sans-serif stem whose only difference from font8x8 is font8x8's serifs. Each is
/// justified in docs/art/README.md ("Provenance and licence").
const FORCED: &str = ".,:;'`_|IT";

/// Closer than this to font8x8_basic, a glyph would be a copy (docs/art/README.md).
const MIN_DISTANCE: u32 = 5;

#[test]
fn glyphs_are_not_copied_from_font8x8() {
    let f = sheet();
    let reference = font8x8_basic();
    assert_eq!(reference.len(), 128);
    for c in 0x21u8..0x7f {
        let ch = char::from(c);
        let near = (-1..=1)
            .flat_map(|dx| (-1..=1).map(move |dy| (dx, dy)))
            .map(|(dx, dy)| {
                distance(
                    &f.glyphs[usize::from(c)],
                    &reference[usize::from(c)],
                    dx,
                    dy,
                )
            })
            .min()
            .unwrap();
        if FORCED.contains(ch) {
            assert!(
                near < MIN_DISTANCE,
                "{ch:?} is {near} pixels away; drop it from FORCED"
            );
            continue;
        }
        assert!(
            near >= MIN_DISTANCE,
            "{ch:?} is {near} pixels from font8x8_basic (shifted up to one pixel); redraw it"
        );
    }
}
