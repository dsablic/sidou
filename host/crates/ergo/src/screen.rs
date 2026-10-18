// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading a screen: the 16-colour palette, text found in the grid's cells (scale from the
//! cell codes, colours from the attribute bytes), and PNG output.

use std::path::Path;

use crate::device::{Cell, Shot};

/// app/grid.h: large text's pieces are codes GRID_BIG_FIRST.., 2x then 3x, then the plain
/// piece.
pub(crate) const BIG_FIRST: u8 = 0x01;
const PLAIN_PIECE: u8 = BIG_FIRST + 13;

/// The art palette as the panel shows it: the firmware's RGB565 table expanded to 8 bits.
pub fn palette565() -> [u16; 16] {
    unsafe { sidou_sys::grid_palette }
}

pub fn palette_rgb() -> [[u8; 3]; 16] {
    let p = palette565();
    std::array::from_fn(|i| {
        let c = p[i];
        let (r, g, b) = ((c >> 11) as u8 & 31, (c >> 5) as u8 & 63, c as u8 & 31);
        [r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2]
    })
}

pub const COLOUR_NAMES: [&str; 16] = [
    "black",
    "white",
    "red",
    "cyan",
    "purple",
    "green",
    "blue",
    "yellow",
    "orange",
    "brown",
    "light red",
    "dark grey",
    "grey",
    "light green",
    "light blue",
    "light grey",
];

/// A piece of text found on the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun {
    pub row: usize,
    pub col: usize,
    /// Cells wide.
    pub cells: usize,
    /// 1, 2 or 3 times the font.
    pub scale: u32,
    pub fg: u8,
    pub bg: u8,
    pub text: String,
}

fn glyph(c: &Cell) -> Option<(char, u32)> {
    if c.ovl {
        return None;
    }
    let printable = |ch: u8| (0x20..=0x7e).contains(&ch);
    match c.ch {
        0x20..=0x7e => Some((c.ch as char, 1)),
        BIG_FIRST if printable(c.src) => Some((c.src as char, 2)),
        5 if printable(c.src) => Some((c.src as char, 3)),
        PLAIN_PIECE if printable(c.src) => Some((c.src as char, 1)),
        _ => None,
    }
}

/// The text on a screen. Plain text runs over adjacent cells that share their attribute (a
/// single space may join words); a large character is read at its top-left piece, `scale`
/// cells apart.
pub fn text_runs(cells: &[Cell]) -> Vec<TextRun> {
    let mut out: Vec<TextRun> = Vec::new();
    for row in 0..30usize {
        let mut cur: Option<TextRun> = None;
        let mut col = 0usize;
        let flush = |cur: &mut Option<TextRun>, out: &mut Vec<TextRun>| {
            if let Some(mut r) = cur.take() {
                let t = r.text.trim_end().to_string();
                if !t.trim().is_empty() {
                    r.cells = r.cells.min(t.chars().count() * r.scale as usize);
                    r.text = t;
                    out.push(r);
                }
            }
        };
        while col < 30 {
            let c = cells[row * 30 + col];
            match glyph(&c) {
                Some((ch, scale))
                    if ch != ' ' || cur.as_ref().is_some_and(|r| r.scale == scale) =>
                {
                    let (fg, bg) = (c.attr & 15, c.attr >> 4);
                    let joins = cur.as_ref().is_some_and(|r| {
                        r.scale == scale && (ch == ' ' || (r.fg == fg && r.bg == bg))
                    });
                    if !joins {
                        flush(&mut cur, &mut out);
                        cur = Some(TextRun {
                            row,
                            col,
                            cells: 0,
                            scale,
                            fg,
                            bg,
                            text: String::new(),
                        });
                    }
                    let r = cur.as_mut().unwrap();
                    r.text.push(ch);
                    r.cells += scale as usize;
                    col += scale as usize;
                }
                _ => {
                    flush(&mut cur, &mut out);
                    col += 1;
                }
            }
        }
        flush(&mut cur, &mut out);
    }
    out
}

/// The panel's pixels as palette indexes.
pub fn indexed(fb: &[u16]) -> Vec<u8> {
    let pal = palette565();
    fb.iter()
        .map(|p| pal.iter().position(|c| c == p).unwrap_or(0) as u8)
        .collect()
}

/// A screen as a 4-bit indexed PNG in the art palette.
pub fn png_bytes(fb: &[u16]) -> Vec<u8> {
    let idx = indexed(fb);
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 240, 240);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Four);
        enc.set_palette(palette_rgb().concat());
        let packed: Vec<u8> = idx.chunks(2).map(|p| p[0] << 4 | p[1]).collect();
        let mut w = enc.write_header().expect("PNG header");
        w.write_image_data(&packed).expect("PNG data");
    }
    out
}

pub fn write_png(path: &Path, fb: &[u16]) {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).expect("create the output directory");
    }
    std::fs::write(path, png_bytes(fb)).expect("write a PNG");
}

/// The screen as text, one line a cell row, for the log and the probe.
pub fn ascii(shot: &Shot) -> String {
    let mut s = String::new();
    for row in 0..30 {
        for col in 0..30 {
            let c = shot.cell(col, row);
            s.push(match glyph(&c) {
                Some((ch, _)) => ch,
                None if c.ch == 0x20 => ' ',
                None if (1..=13).contains(&c.ch) => '~',
                None => '.',
            });
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(cells: &mut [Cell], col: usize, row: usize, s: &str, attr: u8) {
        for (i, b) in s.bytes().enumerate() {
            cells[row * 30 + col + i] = Cell {
                ch: b,
                attr,
                ..Cell::default()
            };
        }
    }

    #[test]
    fn plain_text_is_one_run_per_attribute() {
        let mut cells = vec![Cell::default(); 900];
        put(&mut cells, 2, 3, "SAW LEAD", 0x61);
        put(&mut cells, 10, 3, "A-4", 0x6e);
        let r = text_runs(&cells);
        assert_eq!(r.len(), 2);
        assert_eq!(
            (r[0].text.as_str(), r[0].scale, r[0].fg, r[0].bg),
            ("SAW LEAD", 1, 1, 6)
        );
        assert_eq!(r[1].text, "A-4");
    }

    #[test]
    fn large_text_is_found_by_its_pieces_and_scale() {
        let mut cells = vec![Cell::default(); 900];
        // "OK" at 3x from column 4: top-left pieces (code 5) at columns 4 and 7.
        for (i, ch) in ['O', 'K'].into_iter().enumerate() {
            for y in 0..3 {
                for x in 0..3 {
                    cells[(5 + y) * 30 + 4 + i * 3 + x] = Cell {
                        ch: 5 + (y * 3 + x) as u8,
                        attr: 0x01,
                        src: ch as u8,
                        ovl: false,
                    };
                }
            }
        }
        let r = text_runs(&cells);
        assert_eq!(r.len(), 1);
        assert_eq!(
            (
                r[0].text.as_str(),
                r[0].scale,
                r[0].cells,
                r[0].row,
                r[0].col
            ),
            ("OK", 3, 6, 5, 4)
        );
    }

    #[test]
    fn a_two_times_text_is_scale_2() {
        let mut cells = vec![Cell::default(); 900];
        for y in 0..2 {
            for x in 0..2 {
                cells[(27 + y) * 30 + 2 + x] = Cell {
                    ch: 1 + (y * 2 + x) as u8,
                    attr: 0x01,
                    src: b'X',
                    ovl: false,
                };
            }
        }
        let r = text_runs(&cells);
        assert_eq!((r[0].scale, r[0].text.as_str()), (2, "X"));
    }
}
