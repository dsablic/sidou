// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The title cards' art (app/title_card.h): the indexed PNGs `assets/logo/title-<name>.png`
//! (each card's held frame) and `assets/logo/title-glow.png` (their glow alone), drawn by
//! `assets/logo/metal-chrome.js`, and the C they generate, `app/title_card_bitmap.c`.
//! `tests/app/title_card_bitmap.rs` keeps the generated file in step with the PNGs.
//!
//! The glow is drawn a cell at a time: each cell is the font's ordered-dither glyph D0..D15
//! (a pixel set where the 4 x 4 Bayer matrix is below n) in two neighbouring stops of the
//! glow's colours, at a level L = 16 i + n (stop i, 0..47). The firmware draws it with glyphs
//! from a table of L, a quarter of the screen (the glow is symmetric about the screen's
//! centre), and fades it in through levels k = 1..4 of 4 (floor(L * k / 4)). A card's
//! bitmap holds the pixels of the cells that differ from the glow (the letters, their halos,
//! the rule, the glint, the stars), and the glow's background stop flat in the others, under
//! the glyph the firmware draws there; a bit a cell says which cells are which.

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::bitmap_codec;
use crate::font::repo_root;
use crate::logo::{self, Image};

/// A card: its PNG is `assets/logo/title-<name>.png`, its title THE `word`.
#[derive(Clone, Copy, Debug)]
pub struct Title {
    pub name: &'static str,
    pub word: &'static str,
}

/// The cards, in app/title_card.h's order (title_card_id).
pub const TITLES: [Title; 3] = [
    Title {
        name: "oscillator",
        word: "OSCILLATOR",
    },
    Title {
        name: "scene",
        word: "SCENE",
    },
    Title {
        name: "chip",
        word: "CHIP",
    },
];

/// The glow's colours, dark to bright: black, blue, purple, red (GRID_ colours).
pub const STOPS: [u8; 4] = [0, 6, 4, 2];
/// The highest level: under stop 2's last step (red never shows whole).
pub const MAX_LEVEL: u8 = 47;
/// The fade's levels: k = 1..LEVELS.
pub const LEVELS: u32 = 4;
/// The 4 x 4 Bayer matrix (the font's D glyphs and the art's dither).
pub const BAY: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
/// The screen's cells a side, and the glow table's (a quarter).
const CELLS: usize = 30;
pub const HALF: usize = 15;

/// The glow's levels, a cell each, row by row.
pub type Levels = [[u8; CELLS]; CELLS];

pub fn generated_path() -> PathBuf {
    repo_root().join("app/title_card_bitmap.c")
}

/// Pixel (x, y) of a cell (0..8) is set in dither glyph Dn.
pub fn dither_set(n: u8, x: usize, y: usize) -> bool {
    BAY[(y & 3) * 4 + (x & 3)] < n
}

/// Level `level` at fade step k of LEVELS: the stop (0..2) and the glyph's n (0..15).
pub fn glow_cell(level: u8, k: u32) -> (usize, u8) {
    let l = u32::from(level) * k / LEVELS;
    ((l >> 4) as usize, (l & 15) as u8)
}

fn cell_pixels(img: &Image, c: usize, r: usize) -> impl Iterator<Item = (usize, usize, u8)> + '_ {
    (0..64).map(move |p| {
        let (x, y) = (p % 8, p / 8);
        (x, y, img.px[(r * 8 + y) * img.w + c * 8 + x])
    })
}

/// The glow's level in each cell, from the glow alone at full level: each cell must be Dn
/// over a stop in the next stop up.
pub fn glow_levels(glow: &Image) -> Result<Levels, String> {
    if (glow.w, glow.h) != (240, 240) {
        return Err(format!("{} x {}: not the screen", glow.w, glow.h));
    }
    let mut out = [[0u8; CELLS]; CELLS];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, level) in row.iter_mut().enumerate() {
            // Pixel (0, 3) of the cell is Bayer's 15: never set below D16, the background.
            let bg = glow.px[(r * 8 + 3) * glow.w + c * 8];
            let i = STOPS[..3].iter().position(|&s| s == bg).ok_or(format!(
                "cell ({c}, {r}): {bg} is not a glow stop below the top"
            ))?;
            let n = cell_pixels(glow, c, r).filter(|&(_, _, p)| p != bg).count() / 4;
            let ok = n < 16
                && cell_pixels(glow, c, r).all(|(x, y, p)| {
                    p == if dither_set(n as u8, x, y) {
                        STOPS[i + 1]
                    } else {
                        bg
                    }
                });
            if !ok {
                return Err(format!("cell ({c}, {r}): not a dither glyph of the glow"));
            }
            *level = (16 * i + n) as u8;
            if *level > MAX_LEVEL {
                return Err(format!("cell ({c}, {r}): level {level} over {MAX_LEVEL}"));
            }
        }
    }
    Ok(out)
}

/// The top-left quarter of a glow symmetric about the screen's centre.
pub fn quarter(levels: &Levels) -> Result<[[u8; HALF]; HALF], String> {
    let mut q = [[0u8; HALF]; HALF];
    for r in 0..CELLS {
        for c in 0..CELLS {
            let (qr, qc) = (r.min(CELLS - 1 - r), c.min(CELLS - 1 - c));
            if r < HALF && c < HALF {
                q[r][c] = levels[r][c];
            } else if levels[r][c] != levels[qr][qc] {
                return Err(format!("cell ({c}, {r}): the glow is not symmetric"));
            }
        }
    }
    Ok(q)
}

/// The glow at fade step k, as the firmware draws it with glyphs.
pub fn glow_image(levels: &Levels, k: u32) -> Image {
    let mut px = vec![0u8; 240 * 240];
    for (r, row) in levels.iter().enumerate() {
        for (c, &level) in row.iter().enumerate() {
            let (i, n) = glow_cell(level, k);
            for y in 0..8 {
                for x in 0..8 {
                    px[(r * 8 + y) * 240 + c * 8 + x] = if dither_set(n, x, y) {
                        STOPS[i + 1]
                    } else {
                        STOPS[i]
                    };
                }
            }
        }
    }
    Image { w: 240, h: 240, px }
}

/// The cells of a card that differ from its glow, a bit each (bit c of row r).
pub fn exact_cells(card: &Image, glow: &Image) -> [u32; CELLS] {
    let mut out = [0u32; CELLS];
    for (r, bits) in out.iter_mut().enumerate() {
        for c in 0..CELLS {
            if cell_pixels(card, c, r)
                .zip(cell_pixels(glow, c, r))
                .any(|(a, b)| a.2 != b.2)
            {
                *bits |= 1 << c;
            }
        }
    }
    out
}

/// A card's bitmap: its pixels in its exact cells, and the glow's background stop flat in
/// the others (the firmware draws the glyph over it).
pub fn flattened(card: &Image, levels: &Levels, cells: &[u32; CELLS]) -> Image {
    let mut px = card.px.clone();
    for r in 0..CELLS {
        for c in 0..CELLS {
            if cells[r] >> c & 1 == 0 {
                let (i, _) = glow_cell(levels[r][c], LEVELS);
                for y in 0..8 {
                    for x in 0..8 {
                        px[(r * 8 + y) * 240 + c * 8 + x] = STOPS[i];
                    }
                }
            }
        }
    }
    Image { w: 240, h: 240, px }
}

/// Everything the generated file holds, from the PNGs.
pub struct Cards {
    pub glow: Levels,
    pub cells: Vec<[u32; CELLS]>,
    pub bitmaps: Vec<Image>,
}

pub fn load() -> Cards {
    let glow_img = logo::load("title-glow");
    let glow = glow_levels(&glow_img).unwrap_or_else(|e| panic!("title-glow.png: {e}"));
    let mut cells = Vec::new();
    let mut bitmaps = Vec::new();
    for t in TITLES {
        let card = logo::load(&format!("title-{}", t.name));
        let c = exact_cells(&card, &glow_img);
        bitmaps.push(flattened(&card, &glow, &c));
        cells.push(c);
    }
    Cards {
        glow,
        cells,
        bitmaps,
    }
}

/// What the cards cost in flash: runs, row offsets, cell bits and the glow table, in bytes.
pub fn size(cards: &Cards) -> (usize, usize, usize, usize) {
    let (rows, data, _) = logo::encode_shared(
        &cards
            .bitmaps
            .iter()
            .map(|img| (img, logo::FORMAT))
            .collect::<Vec<_>>(),
    );
    let offs = rows.iter().map(|r| 2 * r.len()).sum();
    (data.len(), offs, 4 * CELLS * cards.cells.len(), HALF * HALF)
}

/// `app/title_card_bitmap.c`.
pub fn c_source() -> String {
    let cards = load();
    let q = quarter(&cards.glow).unwrap_or_else(|e| panic!("title-glow.png: {e}"));
    let (rows, data, bases) = logo::encode_shared(
        &cards
            .bitmaps
            .iter()
            .map(|img| (img, logo::FORMAT))
            .collect::<Vec<_>>(),
    );
    let (runs, offs, bits, glow) = size(&cards);
    let mut s = String::from(logo::SPDX);
    let _ = write!(
        s,
        "/* Generated by host/crates/sidou-tests/src/title_card.rs from\n \
         * assets/logo/title-glow.png and assets/logo/title-<name>.png; do not edit.\n \
         * The title cards (app/title_card.h): the glow's level in each cell of the screen's\n \
         * top-left quarter, and each card's bitmap (its pixels in the cells that differ from\n \
         * the glow, the glow's background stop flat in the others) with a bit a cell for the\n \
         * cells that differ. Run dictionaries and row packets (docs/art/bitmap-codec.md):\n \
         * {runs} bytes of coded data, {offs} of row offsets,\n \
         * {bits} of cell bits and {glow} of glow levels: {} in all. */\n\
         #include \"title_card.h\"\n\n\
         const uint8_t title_card_glow[TITLE_CARD_GLOW_HALF][TITLE_CARD_GLOW_HALF] = {{\n",
        runs + offs + bits + glow
    );
    for row in &q {
        let line: Vec<String> = row.iter().map(|v| format!("{v:2}")).collect();
        let _ = writeln!(s, "    {{{}}},", line.join(", "));
    }
    let _ = write!(
        s,
        "}};\n\nconst uint32_t title_card_cells[TITLE_CARDS][GRID_ROWS] = {{\n"
    );
    for (t, c) in TITLES.iter().zip(&cards.cells) {
        let _ = write!(
            s,
            "    {{ /* {} */\n{}    }},\n",
            t.word,
            logo::table(c, 6, |v| format!("0x{v:08x}"))
        );
    }
    let _ = write!(
        s,
        "}};\n\nstatic const uint8_t RUNS[{}] = {{\n{}}};\n",
        data.len(),
        logo::table(&data, 16, |v| format!("0x{v:02x}"))
    );
    for (t, r) in TITLES.iter().zip(&rows) {
        let _ = write!(
            s,
            "\nstatic const uint16_t {}[240] = {{\n{}}};\n",
            t.word,
            logo::table(r, 12, |v| format!("{v:5}"))
        );
    }
    let _ = write!(
        s,
        "\nconst grid_bitmap title_card_bitmaps[TITLE_CARDS] = {{\n{}}};\n",
        TITLES
            .iter()
            .zip(bases)
            .map(|(t, base)| format!(
                "    {{240, 240, {}, {}}},\n",
                t.word,
                bitmap_codec::data_ref(base)
            ))
            .collect::<String>()
    );
    s
}

/// Writes `app/title_card_bitmap.c` if `update`, else returns it if it is stale.
pub fn check_or_write(update: bool) -> Vec<PathBuf> {
    logo::check_or_write_file(&generated_path(), &c_source(), update)
}
