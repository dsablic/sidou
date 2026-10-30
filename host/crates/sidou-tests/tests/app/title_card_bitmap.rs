// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The title cards' art: app/title_card_bitmap.c is generated from assets/logo/title-glow.png and
//! assets/logo/title-<name>.png (sidou_tests::title_card). SIDOU_LOGO_UPDATE=1 rewrites it, with
//! app/logo_bitmap.c.

use sidou_sys::*;
use sidou_tests::logo::{self, Image};
use sidou_tests::title_card::{self as tc, TITLES};

const W: usize = 240;

#[test]
fn the_generated_file_is_up_to_date() {
    let update = std::env::var("SIDOU_LOGO_UPDATE").as_deref() == Ok("1");
    let stale = tc::check_or_write(update);
    assert!(
        stale.is_empty(),
        "{stale:?} is stale; rerun with SIDOU_LOGO_UPDATE=1"
    );
    for line in tc::c_source().lines() {
        assert!(
            line.len() <= 100,
            "generated line over 100 columns: {line:?}"
        );
    }
}

/// The font's dither glyphs D0..D16 are the 4 x 4 Bayer matrix the art dithers with: Dn sets
/// a pixel where the matrix is below n. So the glow drawn in glyphs is the art's.
#[test]
fn the_fonts_dither_glyphs_are_the_arts_bayer_order() {
    for n in 0..=16u8 {
        let g = &sidou_tests::font::compiled_glyph(FONT_D0 as u8 + n);
        for (y, row) in g.iter().enumerate().take(8) {
            for x in 0..8 {
                assert_eq!(
                    row >> (7 - x) & 1 == 1,
                    tc::dither_set(n, x, y),
                    "D{n} ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn the_glow_is_dither_glyphs_symmetric_about_the_centre() {
    let glow = logo::load("title-glow");
    let levels = tc::glow_levels(&glow).unwrap();
    let q = tc::quarter(&levels).unwrap();
    // Brightest at the centre, out to black at the corners, and never past stop 2 (purple,
    // red its dither): the firmware's colours are the stop and the next one.
    assert_eq!(q[14][14], *q.iter().flatten().max().unwrap());
    assert!(q[14][14] >= 32, "the centre reaches purple: {}", q[14][14]);
    assert_eq!(q[0][0], 0);
    assert!(levels.iter().flatten().all(|&l| l <= tc::MAX_LEVEL));
    // The glow alone is what the levels draw at full.
    assert_eq!(tc::glow_image(&levels, tc::LEVELS), glow);
}

/// Each card is the glow but for its exact cells, which hold its title, rule, glint and stars:
/// the flattened bitmap with the glyphs over it is the card's PNG pixel for pixel.
#[test]
fn each_card_is_its_glow_and_its_exact_cells() {
    let glow = logo::load("title-glow");
    let levels = tc::glow_levels(&glow).unwrap();
    let full = tc::glow_image(&levels, tc::LEVELS);
    for t in TITLES {
        let card = logo::load(&format!("title-{}", t.name));
        let cells = tc::exact_cells(&card, &glow);
        let n: u32 = cells.iter().map(|r| r.count_ones()).sum();
        println!("{}: {n} cells differ from the glow", t.word);
        assert!((100..400).contains(&n), "{}: {n} exact cells", t.word);
        let flat = tc::flattened(&card, &levels, &cells);
        let mut shown = flat.px.clone();
        for (r, bits) in cells.iter().enumerate() {
            for c in 0..30 {
                if bits >> c & 1 == 0 {
                    // The glyph over the flat stop: the glow's own pixels.
                    for y in 0..8 {
                        let at = (r * 8 + y) * W + c * 8;
                        shown[at..at + 8].copy_from_slice(&full.px[at..at + 8]);
                    }
                }
            }
        }
        assert!(shown == card.px, "{}", t.word);
        // THE and the title in the chrome's orange and yellow, which the glow never has.
        let warm = [GRID_ORANGE, GRID_YELLOW];
        let rows = (0..W)
            .filter(|&y| {
                card.px[y * W..(y + 1) * W]
                    .iter()
                    .any(|&c| warm.contains(&u32::from(c)))
            })
            .count();
        assert!(rows > 20, "{}: {rows} rows of chrome", t.word);
    }
}

#[test]
fn the_compiled_tables_are_the_pngs() {
    let cards = tc::load();
    let q = tc::quarter(&cards.glow).unwrap();
    assert_eq!(unsafe { title_card_glow }, q);
    let compiled = unsafe { title_card_cells };
    for (k, t) in TITLES.iter().enumerate() {
        assert_eq!(compiled[k], cards.cells[k], "{}", t.word);
        let b = unsafe { &title_card_bitmaps[k] };
        assert_eq!((b.w, b.h), (240, 240));
        let img = &cards.bitmaps[k];
        for y in 0..W {
            let mut row = vec![0xffu8; W];
            unsafe { grid_bitmap_row(b, 0, y as u32, W as u32, row.as_mut_ptr()) };
            assert_eq!(row, img.px[y * W..(y + 1) * W], "{} row {y}", t.word);
        }
    }
    let names: Vec<&str> = (0..TITLE_CARDS as usize)
        .map(|k| {
            unsafe { std::ffi::CStr::from_ptr(title_card_titles[k]) }
                .to_str()
                .unwrap()
        })
        .collect();
    assert_eq!(
        names,
        TITLES.iter().map(|t| t.word).collect::<Vec<_>>(),
        "the C table's titles in the generated order"
    );
}

#[test]
fn encoding_the_cards_together_round_trips() {
    let cards = tc::load();
    let (rows, data, bases) = logo::encode_shared(
        &cards
            .bitmaps
            .iter()
            .map(|img| (img, logo::FORMAT))
            .collect::<Vec<_>>(),
    );
    for ((img, rows), base) in cards.bitmaps.iter().zip(rows).zip(bases) {
        let enc = logo::Encoded {
            rows,
            data: data[base..].to_vec(),
        };
        assert_eq!(logo::decode(&enc, img.w, img.h), img.px);
    }
}

/// What the cards cost in flash, all three, their glow drawn in glyphs from 225 bytes of
/// levels, against the original whole-PNG byte-run control (57,190 bytes). The numeric
/// ceilings stay the same even though the new codec also shrinks that counterfactual.
#[test]
fn the_cards_are_small() {
    let cards = tc::load();
    let (runs, offs, bits, glow) = tc::size(&cards);
    let total = runs + offs + bits + glow;
    let whole: usize = TITLES
        .iter()
        .map(|t| logo::byte_run_size(&logo::load(&format!("title-{}", t.name))))
        .sum();
    println!(
        "title cards: {runs} bytes of runs + {offs} of row offsets + {bits} of cell bits + \
         {glow} of glow = {total}; the PNGs whole would be {whole}"
    );
    assert!(total <= 27_087, "{total} bytes");
    assert!(total * 2 < whole, "{total} against {whole}");
}

#[test]
fn a_glow_cell_that_is_not_a_dither_glyph_is_refused() {
    let glow = logo::load("title-glow");
    let mut bad = glow.clone();
    // A stray red pixel where cell (0, 0) is black.
    bad.px[3 * W + 5] = GRID_RED as u8;
    assert!(tc::glow_levels(&bad).unwrap_err().contains("cell (0, 0)"));
    // A cell in two stops that are not neighbours.
    let mut bad = glow.clone();
    for y in 0..8 {
        for x in 0..8 {
            bad.px[(14 * 8 + y) * W + 14 * 8 + x] = if (x + y) % 2 == 0 { 0 } else { 2 };
        }
    }
    assert!(tc::glow_levels(&bad).is_err());
    let small = Image {
        w: 8,
        h: 8,
        px: vec![0; 64],
    };
    assert!(tc::glow_levels(&small).is_err());
}
