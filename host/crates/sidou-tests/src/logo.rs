// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The logos' bitmaps: the indexed PNGs `assets/logo/metal-chrome.png` and
//! `assets/logo/cracktro-chrome.png` (drawn by `assets/logo/<name>.js`) and the C they
//! generate, `app/logo_bitmap.c`, a `grid_bitmap` (app/grid.h) each for the renderer's pixel
//! region. `tests/app/logo_bitmap.rs` keeps the generated file in step with the PNGs.
//!
//! The encoding: each pixel row run-length coded, a byte a run of 1..16 pixels
//! (`(length - 1) << 4 | colour`), and a table of where each row's runs start, so that any
//! row can be decoded on its own. Rows alike share their runs.

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::font::repo_root;

/// The art palette (docs/art/README.md, app/grid.h's GRID_ colours) in sRGB.
pub const PALETTE: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00],
    [0xff, 0xff, 0xff],
    [0x68, 0x37, 0x2b],
    [0x70, 0xa4, 0xb2],
    [0x6f, 0x3d, 0x86],
    [0x58, 0x8d, 0x43],
    [0x35, 0x28, 0x79],
    [0xb8, 0xc7, 0x6f],
    [0x6f, 0x4f, 0x25],
    [0x43, 0x39, 0x00],
    [0x9a, 0x67, 0x59],
    [0x44, 0x44, 0x44],
    [0x6c, 0x6c, 0x6c],
    [0x9a, 0xd2, 0x84],
    [0x6c, 0x5e, 0xb5],
    [0x95, 0x95, 0x95],
];

/// An image as art palette indexes, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

/// A run-length coded image: row y's runs start at `data[rows[y]]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoded {
    pub rows: Vec<u16>,
    pub data: Vec<u8>,
}

/// The longest run a byte holds.
const RUN: usize = 16;

/// A logo's scene: `assets/logo/<name>.png`, compiled into `app/logo_bitmap.c` as `symbol`.
#[derive(Clone, Copy, Debug)]
pub struct Logo {
    pub name: &'static str,
    pub title: &'static str,
    pub symbol: &'static str,
}

/// The logos with a bitmap, the default first (app/logo.h).
pub const LOGOS: [Logo; 2] = [
    Logo {
        name: "metal-chrome",
        title: "Metal Chrome",
        symbol: "logo_metal_chrome_bitmap",
    },
    Logo {
        name: "cracktro-chrome",
        title: "Cracktro Chrome",
        symbol: "logo_cracktro_chrome_bitmap",
    },
];

/// The default logo's name (app/logo.h logo_default).
pub const DEFAULT: &str = "metal-chrome";

pub fn png_path(name: &str) -> PathBuf {
    repo_root().join(format!("assets/logo/{name}.png"))
}

pub fn generated_path() -> PathBuf {
    repo_root().join("app/logo_bitmap.c")
}

/// Reads and checks `assets/logo/<name>.png`.
pub fn load(name: &str) -> Image {
    let path = png_path(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    parse_png(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// An indexed PNG whose pixels are art palette colours (its palette in any order), at most
/// the screen and in whole 8 x 8 cells.
pub fn parse_png(bytes: &[u8]) -> Result<Image, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .map_err(|e| format!("not a PNG: {e}"))?;
    let info = dec.info();
    let (w, h) = (info.width as usize, info.height as usize);
    if info.color_type != png::ColorType::Indexed {
        return Err(format!("{:?}, not an indexed PNG", info.color_type));
    }
    if w == 0 || h == 0 || w % 8 != 0 || h % 8 != 0 || w > 240 || h > 240 {
        return Err(format!(
            "{w} x {h}: not whole cells of 8 x 8 pixels within 240 x 240"
        ));
    }
    let depth = info.bit_depth as usize;
    let plte = info.palette.as_ref().ok_or("no PLTE chunk")?.to_vec();
    let trns = info.trns.as_ref().map(|t| t.to_vec()).unwrap_or_default();
    let mut buf = vec![0; dec.output_buffer_size().ok_or("PNG size")?];
    let frame = dec
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG data: {e}"))?;
    let stride = frame.line_size;
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let line = &buf[y * stride..(y + 1) * stride];
        for x in 0..w {
            let bit = x * depth;
            let entry = usize::from(line[bit / 8] >> (8 - depth - bit % 8)) & ((1 << depth) - 1);
            let rgb = plte
                .get(entry * 3..entry * 3 + 3)
                .ok_or(format!("({x}, {y}): entry {entry} is past the palette"))?;
            if trns.get(entry).is_some_and(|&a| a != 255) {
                return Err(format!("({x}, {y}): entry {entry} is transparent"));
            }
            let c = PALETTE.iter().position(|p| p == rgb).ok_or(format!(
                "({x}, {y}): {rgb:02x?} is not an art palette colour"
            ))?;
            px.push(c as u8);
        }
    }
    Ok(Image { w, h, px })
}

/// One row's runs.
fn row_runs(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut x = 0;
    while x < row.len() {
        let c = row[x];
        let n = row[x..].iter().take(RUN).take_while(|&&p| p == c).count();
        out.push(((n - 1) << 4) as u8 | c);
        x += n;
    }
    out
}

pub fn encode(img: &Image) -> Encoded {
    let (mut rows, data) = encode_shared(&[img]);
    Encoded {
        rows: rows.remove(0),
        data,
    }
}

/// Several images into one array of runs: each image's row offsets, and the runs, which rows
/// alike share within an image and across them.
pub fn encode_shared(imgs: &[&Image]) -> (Vec<Vec<u16>>, Vec<u8>) {
    let mut data: Vec<u8> = Vec::new();
    let mut seen: std::collections::HashMap<Vec<u8>, u16> = std::collections::HashMap::new();
    let rows = imgs
        .iter()
        .map(|img| {
            img.px
                .chunks(img.w)
                .map(|row| {
                    let runs = row_runs(row);
                    *seen.entry(runs).or_insert_with_key(|runs| {
                        let at = u16::try_from(data.len()).expect("runs past 64 KB");
                        data.extend_from_slice(runs);
                        at
                    })
                })
                .collect()
        })
        .collect();
    (rows, data)
}

pub fn decode(enc: &Encoded, w: usize, h: usize) -> Vec<u8> {
    let mut px = Vec::with_capacity(w * h);
    for &at in &enc.rows[..h] {
        let mut n = 0;
        for &b in &enc.data[usize::from(at)..] {
            if n == w {
                break;
            }
            let len = usize::from(b >> 4) + 1;
            px.extend(std::iter::repeat_n(b & 15, len));
            n += len;
        }
        assert_eq!(n, w, "a row's runs add up to its width");
    }
    px
}

// The generated file's own licence header; reuse must not read it as this file's.
// REUSE-IgnoreStart
pub(crate) const SPDX: &str = "// SPDX-FileCopyrightText: 2026 SIDOU contributors\n\
                               // SPDX-License-Identifier: GPL-3.0-or-later\n";
// REUSE-IgnoreEnd

/// Numbers as C initialisers, `per` to a line.
pub(crate) fn table<T: std::fmt::Display>(
    items: &[T],
    per: usize,
    fmt: impl Fn(&T) -> String,
) -> String {
    let mut s = String::new();
    for chunk in items.chunks(per) {
        let line: Vec<String> = chunk.iter().map(&fmt).collect();
        let _ = writeln!(s, "    {},", line.join(", "));
    }
    s
}

/// What an image costs alone: its runs and row offsets, in bytes.
pub fn size(img: &Image) -> (usize, usize) {
    let enc = encode(img);
    (enc.data.len(), 2 * enc.rows.len())
}

/// `app/logo_bitmap.c`, from the logos' PNGs.
pub fn c_source() -> String {
    let imgs: Vec<Image> = LOGOS.iter().map(|l| load(l.name)).collect();
    let (rows, data) = encode_shared(&imgs.iter().collect::<Vec<_>>());
    let mut s = String::from(SPDX);
    let files: Vec<String> = LOGOS
        .iter()
        .map(|l| format!("assets/logo/{}.png", l.name))
        .collect();
    let files = files.join(" and\n * ");
    let _ = write!(
        s,
        "/* Generated by host/crates/sidou-tests/src/logo.rs from {};\n \
         * do not edit.\n \
         * The logos' scenes (app/logo.h), each a grid_bitmap (app/grid.h): a pixel row's runs, a\n \
         * byte each, (length - 1) << 4 | colour, and a table of where each row's runs start.\n \
         * Rows alike share their runs, within a scene and across them.\n",
        files
    );
    for (l, img) in LOGOS.iter().zip(&imgs) {
        let (runs, offs) = size(img);
        let _ = writeln!(
            s,
            " * {}: {} x {} pixels, {runs} bytes of runs and {offs} of row offsets alone,\n \
             *   {} in all; 4 bits a pixel unpacked would be {}.",
            l.title,
            img.w,
            img.h,
            runs + offs,
            img.w * img.h / 2
        );
    }
    let offs: usize = rows.iter().map(|r| 2 * r.len()).sum();
    let _ = write!(
        s,
        " * Together: {} bytes of runs and {offs} of row offsets, {} in all. */\n\
         #include \"logo.h\"\n\n\
         static const uint8_t RUNS[{}] = {{\n{}}};\n",
        data.len(),
        data.len() + offs,
        data.len(),
        table(&data, 16, |v| format!("0x{v:02x}")),
    );
    for ((l, img), r) in LOGOS.iter().zip(&imgs).zip(&rows) {
        let rows_name = l.name.replace('-', "_").to_uppercase();
        let _ = write!(
            s,
            "\nstatic const uint16_t {rows_name}[{}] = {{\n{}}};\n\n\
             const grid_bitmap {} = {{{}, {}, {rows_name}, RUNS}};\n",
            img.h,
            table(r, 12, |v| format!("{v:5}")),
            l.symbol,
            img.w,
            img.h,
        );
    }
    s
}

/// Writes `app/logo_bitmap.c` if `update`, else returns it if it is stale.
pub fn check_or_write(update: bool) -> Vec<PathBuf> {
    check_or_write_file(&generated_path(), &c_source(), update)
}

/// Writes `want` to `path` if `update`, else returns `path` if it holds something else.
pub(crate) fn check_or_write_file(
    path: &std::path::Path,
    want: &str,
    update: bool,
) -> Vec<PathBuf> {
    if update {
        std::fs::write(path, want).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        return Vec::new();
    }
    if std::fs::read_to_string(path).ok().as_deref() == Some(want) {
        Vec::new()
    } else {
        vec![path.to_path_buf()]
    }
}
