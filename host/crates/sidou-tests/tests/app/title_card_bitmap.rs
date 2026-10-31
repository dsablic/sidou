// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
    assert_eq!(q[14][14], *q.iter().flatten().max().unwrap());
    assert!(q[14][14] >= 32, "the centre reaches purple: {}", q[14][14]);
    assert_eq!(q[0][0], 0);
    assert!(levels.iter().flatten().all(|&l| l <= tc::MAX_LEVEL));
    assert_eq!(tc::glow_image(&levels, tc::LEVELS), glow);
}

#[test]
fn each_card_is_its_glow_but_for_its_exact_cells_pixel_for_pixel() {
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
                    for y in 0..8 {
                        let at = (r * 8 + y) * W + c * 8;
                        shown[at..at + 8].copy_from_slice(&full.px[at..at + 8]);
                    }
                }
            }
        }
        assert!(shown == card.px, "{}", t.word);
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

const CARDS_FLASH_BYTES_MAX: usize = 27_087;

#[test]
fn the_cards_fit_their_flash_budget_in_under_half_the_pngs_byte_runs() {
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
    assert!(total <= CARDS_FLASH_BYTES_MAX, "{total} bytes");
    assert!(total * 2 < whole, "{total} against {whole}");
}

#[test]
fn a_glow_cell_that_is_not_a_dither_glyph_is_refused() {
    let glow = logo::load("title-glow");
    let mut stray_red_pixel = glow.clone();
    stray_red_pixel.px[3 * W + 5] = GRID_RED as u8;
    assert!(tc::glow_levels(&stray_red_pixel)
        .unwrap_err()
        .contains("cell (0, 0)"));
    let mut stops_not_neighbours = glow.clone();
    for y in 0..8 {
        for x in 0..8 {
            stops_not_neighbours.px[(14 * 8 + y) * W + 14 * 8 + x] =
                if (x + y) % 2 == 0 { 0 } else { 2 };
        }
    }
    assert!(tc::glow_levels(&stops_not_neighbours).is_err());
    let small = Image {
        w: 8,
        h: 8,
        px: vec![0; 64],
    };
    assert!(tc::glow_levels(&small).is_err());
}
