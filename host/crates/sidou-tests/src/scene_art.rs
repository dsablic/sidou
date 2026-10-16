// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! RED MOON RITE's art in flash: the indexed PNGs in `assets/scene/` (drawn by
//! `assets/scene/red-moon-rite.js`) and the C they generate, `app/scene_rmr_art.c`, a
//! `scene_image` (app/scene_art.h) each. `tests/app/scene_art.rs` keeps the generated file in
//! step with the PNGs.
//!
//! The encoding is the logo bitmaps' (`logo.rs`): each pixel row run-length coded, a byte a
//! run of 1..16 pixels (`(length - 1) << 4 | colour`), a table of where each row's runs start,
//! rows alike sharing their runs across every image. Transparency is a colour no image of the
//! set uses (`clear`). Each image is cropped to the rectangle its pixels cover; an unarmed pose
//! is a patch over its armed pose (the rectangle where they differ) when that is smaller; a sky
//! is a gradient computed on the device and the pixels the gradient does not give (the moon,
//! wisps and hills), which is smaller than the whole sky.

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::font::repo_root;
use crate::logo::{check_or_write_file, table, PALETTE, SPDX};

/// A transparent pixel.
pub const CLEAR: u8 = 255;

/// The longest run a byte holds.
const RUN: usize = 16;

/// app/scene_art.h SCENE_ART_SEEK: a row wider than this has a run start, and a checkpoint,
/// every SEEK pixels.
pub const SEEK: usize = 32;

/// The flash ceiling for RED MOON RITE's art (docs/plans/2026-10-15-m2f-scene.md).
pub const CEILING: usize = 48 * 1024;

/// An image as palette indexes, row by row, `CLEAR` where transparent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Picture {
    pub fn at(&self, x: usize, y: usize) -> u8 {
        self.px[y * self.w + x]
    }
}

/// An indexed PNG whose pixels are art palette colours or fully transparent.
pub fn parse_png(bytes: &[u8]) -> Result<Picture, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .map_err(|e| format!("not a PNG: {e}"))?;
    let info = dec.info();
    let (w, h) = (info.width as usize, info.height as usize);
    if info.color_type != png::ColorType::Indexed {
        return Err(format!("{:?}, not an indexed PNG", info.color_type));
    }
    if w == 0 || h == 0 || w > 240 || h > 240 {
        return Err(format!("{w} x {h}: not within 240 x 240"));
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
            match trns.get(entry) {
                Some(0) => {
                    px.push(CLEAR);
                    continue;
                }
                Some(255) | None => {}
                Some(a) => return Err(format!("({x}, {y}): entry {entry} has alpha {a}")),
            }
            let rgb = plte
                .get(entry * 3..entry * 3 + 3)
                .ok_or(format!("({x}, {y}): entry {entry} is past the palette"))?;
            let c = PALETTE.iter().position(|p| p == rgb).ok_or(format!(
                "({x}, {y}): {rgb:02x?} is not an art palette colour"
            ))?;
            px.push(c as u8);
        }
    }
    Ok(Picture { w, h, px })
}

pub fn png_path(name: &str) -> PathBuf {
    repo_root().join(format!("assets/scene/{name}.png"))
}

pub fn load(name: &str) -> Picture {
    let path = png_path(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    parse_png(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn generated_path() -> PathBuf {
    repo_root().join("app/scene_rmr_art.c")
}

/// A raster-split sky (app/scene_art.h's scene_gradient).
#[derive(Clone, Copy, Debug)]
pub struct Gradient {
    pub y0: u8,
    pub y1: u8,
    pub seam: u8,
    pub stops: &'static [(u8, u8)],
}

const BAYER: [u32; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

impl Gradient {
    /// The prototype's gradient(): the colour at screen pixel (x, y).
    pub fn at(&self, x: usize, y: usize) -> u8 {
        let s = self.stops;
        let mut i = 0;
        while i + 1 < s.len() && y >= usize::from(s[i + 1].0) {
            i += 1;
        }
        if i + 1 < s.len() && y + usize::from(self.seam) >= usize::from(s[i + 1].0) {
            let d = (y + usize::from(self.seam) - usize::from(s[i + 1].0)) as u32;
            let level = BAYER[(y & 3) * 4 + ((x >> 1) & 3)];
            if 2 * level * u32::from(self.seam) < (2 * d + 1) * 16 {
                return s[i + 1].1;
            }
        }
        s[i].1
    }
}

/// The sky's rows on the screen: sky-<n>.png holds rows 16..140.
pub const SKY_Y: usize = 16;

/// The three sky palettes, as red-moon-rite.js's PALS (BLACK 0, BLUE 6, PURPLE 4, RED 2,
/// LIGHT_RED 10, ORANGE 8), from row 16 to 140 with seams of 6 rows. The round-trip test
/// checks that gradient and overlay give the PNGs back.
pub const SKIES: [Gradient; 3] = [
    Gradient {
        y0: 16,
        y1: 140,
        seam: 6,
        stops: &[(16, 0), (30, 6), (58, 4), (100, 2), (124, 10)],
    },
    Gradient {
        y0: 16,
        y1: 140,
        seam: 6,
        stops: &[(16, 6), (28, 4), (56, 2), (92, 10), (124, 2)],
    },
    Gradient {
        y0: 16,
        y1: 140,
        seam: 6,
        stops: &[(16, 4), (32, 2), (62, 10), (96, 10), (118, 8)],
    },
];

/// The armed poses, in app/scene_art.h's order, and the unarmed ones.
pub const POSES: [&str; 8] = [
    "guard", "guard2", "raise", "chop", "block", "spinA", "spinB", "hit",
];
pub const UNARMED: [&str; 4] = ["guard", "guard2", "hit", "reach"];

/// What an image is drawn from, before encoding.
#[derive(Clone, Debug)]
pub struct Source {
    /// The C initialiser's place: `scene_rmr_barb[0][SCENE_RMR_GUARD]`.
    pub symbol: String,
    pub png: String,
    /// The frame to code, in stored pixels, `CLEAR` where transparent.
    pub frame: Picture,
    pub gx: u8,
    pub anchor: (u8, u8),
    /// For an unarmed pose: the armed pose's index in the list, a patch candidate.
    pub base: Option<usize>,
}

/// Every image of the set, in the generated file's order.
pub fn sources() -> Vec<Source> {
    let mut out = Vec::new();
    let enum_name = |p: &str| match p {
        "spinA" => "SPIN_A".to_string(),
        "spinB" => "SPIN_B".to_string(),
        p => p.to_uppercase(),
    };
    // The armed poses come first: fighter who's pose k is source who * 8 + k.
    for who in 0..2 {
        for pose in POSES {
            let png = format!("barb-{who}-{pose}");
            out.push(Source {
                symbol: format!("scene_rmr_barb[{who}][SCENE_RMR_{}]", enum_name(pose)),
                frame: load(&png),
                png,
                gx: 2,
                anchor: (58, 112),
                base: None,
            });
        }
    }
    for who in 0..2 {
        for pose in UNARMED {
            let png = format!("barb-{who}-{pose}-unarmed");
            out.push(Source {
                symbol: format!(
                    "scene_rmr_barb_unarmed[{who}][SCENE_RMR_UNARMED_{}]",
                    enum_name(pose)
                ),
                frame: load(&png),
                png,
                gx: 2,
                anchor: (58, 112),
                base: POSES
                    .iter()
                    .position(|p| *p == pose)
                    .map(|k| who * POSES.len() + k),
            });
        }
    }
    for k in 0..8 {
        let png = format!("sword-{k}");
        out.push(Source {
            symbol: format!("scene_rmr_sword[{k}]"),
            frame: load(&png),
            png,
            gx: 2,
            anchor: (42, 42),
            base: None,
        });
    }
    out.push(Source {
        symbol: "scene_rmr_sword_stuck".into(),
        frame: load("sword-stuck"),
        png: "sword-stuck".into(),
        gx: 2,
        anchor: (16, 10),
        base: None,
    });
    for (i, g) in SKIES.iter().enumerate() {
        let png = format!("sky-{i}");
        let sky = load(&png);
        let px = (0..sky.h)
            .flat_map(|y| (0..sky.w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let p = sky.at(x, y);
                if p == g.at(x, y + SKY_Y) {
                    CLEAR
                } else {
                    p
                }
            })
            .collect();
        out.push(Source {
            symbol: format!("scene_rmr_sky[{i}]"),
            frame: Picture {
                w: sky.w,
                h: sky.h,
                px,
            },
            png,
            gx: 1,
            anchor: (0, 0),
            base: None,
        });
    }
    out.push(Source {
        symbol: "scene_rmr_arena".into(),
        frame: load("arena"),
        png: "arena".into(),
        gx: 1,
        anchor: (0, 0),
        base: None,
    });
    out
}

/// The coded rectangle of an image: (x, y, w, h).
pub type Rect = (usize, usize, usize, usize);

/// The smallest rectangle holding every pixel `keep` picks.
fn bbox(p: &Picture, keep: impl Fn(usize, usize) -> bool) -> Option<Rect> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..p.h {
        for x in 0..p.w {
            if keep(x, y) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x0 != usize::MAX).then_some((x0, y0, x1 - x0, y1 - y0))
}

/// One row's runs, `clear` coding transparency; a row wider than SEEK has a run start every
/// SEEK pixels.
fn row_runs(row: &[u8], clear: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut x = 0;
    let split = row.len() > SEEK;
    while x < row.len() {
        let c = row[x];
        let most = if split { RUN.min(SEEK - x % SEEK) } else { RUN };
        let n = row[x..].iter().take(most).take_while(|&&p| p == c).count();
        out.push(((n - 1) << 4) as u8 | if c == CLEAR { clear } else { c });
        x += n;
    }
    out
}

fn rect_rows(p: &Picture, r: Rect, clear: u8) -> Vec<Vec<u8>> {
    let (x, y, w, h) = r;
    (y..y + h)
        .map(|yy| row_runs(&p.px[yy * p.w + x..yy * p.w + x + w], clear))
        .collect()
}

/// What rows cost alone: their runs (rows alike shared) and offsets.
fn rows_cost(rows: &[Vec<u8>]) -> usize {
    let mut seen = std::collections::HashSet::new();
    let runs: usize = rows
        .iter()
        .filter(|r| seen.insert((*r).clone()))
        .map(|r| r.len())
        .sum();
    runs + 2 * rows.len()
}

/// An image as it is coded.
#[derive(Clone, Debug)]
pub struct Coded {
    pub rect: Rect,
    pub rows: Vec<Vec<u8>>,
    /// The base image's index, when coded as a patch.
    pub base: Option<usize>,
    /// What it costs alone (runs and offsets), and what the whole frame would have.
    pub cost: usize,
    pub whole_cost: usize,
}

/// The colour no image uses: the set's `clear`.
pub fn clear_colour(srcs: &[Source]) -> u8 {
    (0..16u8)
        .find(|c| srcs.iter().all(|s| !s.frame.px.contains(c)))
        .expect("every colour is used: no colour is left to code transparency")
}

/// Codes each image: cropped, or as a patch over its base when that is smaller.
pub fn code(srcs: &[Source], clear: u8) -> Vec<Coded> {
    srcs.iter()
        .map(|s| {
            let f = &s.frame;
            let rect = bbox(f, |x, y| f.at(x, y) != CLEAR).expect("an empty image");
            let rows = rect_rows(f, rect, clear);
            let whole_cost = rows_cost(&rows);
            let whole = Coded {
                rect,
                rows,
                base: None,
                cost: whole_cost,
                whole_cost,
            };
            let Some(b) = s.base else {
                return whole;
            };
            let base = &srcs[b].frame;
            assert_eq!(
                (base.w, base.h),
                (f.w, f.h),
                "{}: not its base's size",
                s.png
            );
            let Some(prect) = bbox(f, |x, y| f.at(x, y) != base.at(x, y)) else {
                // The same picture: a patch of one pixel.
                let prect = (0, 0, 1, 1);
                let rows = rect_rows(f, prect, clear);
                let cost = rows_cost(&rows);
                return Coded {
                    rect: prect,
                    rows,
                    base: Some(b),
                    cost,
                    whole_cost,
                };
            };
            let prows = rect_rows(f, prect, clear);
            let cost = rows_cost(&prows);
            if cost < whole_cost {
                Coded {
                    rect: prect,
                    rows: prows,
                    base: Some(b),
                    cost,
                    whole_cost,
                }
            } else {
                whole
            }
        })
        .collect()
}

/// The checkpoints of a coded image's rows (app/scene_art.h `seek`), None when it is no wider
/// than SEEK: for each row, the offset of the run that starts at each multiple of SEEK.
pub fn seek_table(c: &Coded) -> Option<Vec<u8>> {
    let w = c.rect.2;
    let k = (w - 1) / SEEK;
    if k == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(k * c.rows.len());
    for (r, runs) in c.rows.iter().enumerate() {
        let mut at = 0;
        let mut next = SEEK;
        for (off, &b) in runs.iter().enumerate() {
            if at == next && next < w {
                out.push(u8::try_from(off).expect("a row's runs within 256 bytes"));
                next += SEEK;
            }
            at += usize::from(b >> 4) + 1;
        }
        assert_eq!(
            out.len(),
            (r + 1) * k,
            "row {r}: a run starts at every checkpoint"
        );
    }
    Some(out)
}

/// The set's rows in one array of runs, rows alike shared across every image: each image's
/// row offsets, and the runs.
pub fn share(coded: &[Coded]) -> (Vec<Vec<u16>>, Vec<u8>) {
    let mut data: Vec<u8> = Vec::new();
    let mut seen: std::collections::HashMap<Vec<u8>, u16> = std::collections::HashMap::new();
    let rows = coded
        .iter()
        .map(|c| {
            c.rows
                .iter()
                .map(|runs| {
                    *seen.entry(runs.clone()).or_insert_with(|| {
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

/// Decodes a coded image back to its frame (the model the C decoder is checked against).
pub fn decode(coded: &[Coded], i: usize, srcs: &[Source], clear: u8) -> Picture {
    let s = &srcs[i];
    let c = &coded[i];
    let mut px = match c.base {
        Some(b) => decode(coded, b, srcs, clear).px,
        None => vec![CLEAR; s.frame.w * s.frame.h],
    };
    let (x0, y0, w, _) = c.rect;
    for (r, runs) in c.rows.iter().enumerate() {
        let mut x = x0;
        for &b in runs {
            let v = if b & 15 == clear { CLEAR } else { b & 15 };
            for _ in 0..=(b >> 4) {
                px[(y0 + r) * s.frame.w + x] = v;
                x += 1;
            }
        }
        assert_eq!(x, x0 + w, "{}: row {r}'s runs add up to its width", s.png);
    }
    Picture {
        w: s.frame.w,
        h: s.frame.h,
        px,
    }
}

/// The torches' flame areas, {x, y, w, h}, from flame-mask.png: one each side of the middle.
pub fn flames() -> [[u8; 4]; 2] {
    let m = load("flame-mask");
    [0usize, 1].map(|side| {
        let (x, y, w, h) = bbox(&m, |x, y| {
            m.at(x, y) != CLEAR && (x >= m.w / 2) == (side == 1)
        })
        .expect("a flame each side");
        [x, y, w, h].map(|v| u8::try_from(v).unwrap())
    })
}

/// The skies coded whole instead (the other choice the plan names): runs (rows alike shared
/// across the three) and offsets, against the gradients and overlays the set uses.
pub fn skies_both_ways() -> (usize, usize) {
    let srcs = sources();
    let clear = clear_colour(&srcs);
    let whole: Vec<Vec<u8>> = (0..SKIES.len())
        .flat_map(|i| {
            let sky = load(&format!("sky-{i}"));
            rect_rows(&sky, (0, 0, sky.w, sky.h), clear)
        })
        .collect();
    let coded = code(&srcs, clear);
    let overlays: Vec<Vec<u8>> = srcs
        .iter()
        .zip(&coded)
        .filter(|(s, _)| s.png.starts_with("sky-"))
        .flat_map(|(_, c)| c.rows.clone())
        .collect();
    let gradients: usize = SKIES
        .iter()
        .map(|g| GRADIENT_STRUCT + STOP * g.stops.len())
        .sum();
    (rows_cost(&whole), rows_cost(&overlays) + gradients)
}

/// sizeof(scene_image) on the pi32v2 (ILP32): ten bytes, two of padding, four pointers.
pub const IMAGE_STRUCT: usize = 28;
/// sizeof(scene_gradient): four bytes and a pointer; and a stop.
pub const GRADIENT_STRUCT: usize = 8;
pub const STOP: usize = 2;

/// What the set takes in flash: runs, row offsets and checkpoints, the images' and gradients'
/// structs, the stops and the flame table.
pub fn flash() -> (usize, usize, usize, usize) {
    let srcs = sources();
    let coded = code(&srcs, clear_colour(&srcs));
    let (rows, data) = share(&coded);
    let offs: usize = rows.iter().map(|r| 2 * r.len()).sum::<usize>()
        + coded
            .iter()
            .filter_map(seek_table)
            .map(|t| t.len())
            .sum::<usize>();
    let structs = IMAGE_STRUCT * srcs.len()
        + SKIES
            .iter()
            .map(|g| GRADIENT_STRUCT + STOP * g.stops.len())
            .sum::<usize>()
        + 8;
    (data.len(), offs, structs, data.len() + offs + structs)
}

/// `app/scene_rmr_art.c`, from the PNGs.
pub fn c_source() -> String {
    let srcs = sources();
    let clear = clear_colour(&srcs);
    let coded = code(&srcs, clear);
    let (rows, data) = share(&coded);
    let (runs, offs, structs, total) = flash();
    let mut s = String::from(SPDX);
    let _ = write!(
        s,
        "/* Generated by host/crates/sidou-tests/src/scene_art.rs from the PNGs in\n \
         * assets/scene/ (drawn by assets/scene/red-moon-rite.js); do not edit.\n \
         * RED MOON RITE's art (app/scene_art.h): each image cropped to the pixels it covers,\n \
         * a pixel row's runs a byte each, (length - 1) << 4 | colour, colour {clear} coding\n \
         * transparency (no image uses it), and a table of where each row's runs start; rows\n \
         * alike share their runs across every image. The unarmed poses are patches over their\n \
         * armed poses where that is smaller; the skies are a gradient computed on the device\n \
         * and what it does not give (the moon, wisps and hills). A row wider than {SEEK}\n \
         * pixels has a run start every {SEEK}, and a checkpoint table says where they are.\n \
         * In flash: {runs} bytes of runs, {offs} of row offsets and checkpoints and {structs}\n \
         * of tables, {total} in all.\n"
    );
    for (src, c) in srcs.iter().zip(&coded) {
        let (x, y, w, h) = c.rect;
        let how = match c.base {
            Some(_) => format!("a patch, {} bytes (whole, {})", c.cost, c.whole_cost),
            None => format!("{} bytes", c.cost),
        };
        let _ = writeln!(
            s,
            " * {}: {w} x {h} from ({x}, {y}) of {} x {}, {how}.",
            src.png, src.frame.w, src.frame.h
        );
    }
    let _ = write!(
        s,
        " */\n#include \"scene_art.h\"\n\n#include <stddef.h>\n\n\
         static const uint8_t RUNS[{}] = {{\n{}}};\n",
        data.len(),
        table(&data, 16, |v| format!("0x{v:02x}")),
    );
    for (i, r) in rows.iter().enumerate() {
        let _ = write!(
            s,
            "\nstatic const uint16_t ROWS_{i}[{}] = {{\n{}}};\n",
            r.len(),
            table(r, 12, |v| format!("{v:5}")),
        );
        if let Some(t) = seek_table(&coded[i]) {
            let _ = write!(
                s,
                "\nstatic const uint8_t SEEK_{i}[{}] = {{\n{}}};\n",
                t.len(),
                table(&t, 16, |v| format!("{v:3}")),
            );
        }
    }
    let init = |i: usize| {
        let src = &srcs[i];
        let c = &coded[i];
        let (x, y, w, h) = c.rect;
        // A patch's base on a line of its own, within 100 columns.
        let base = c
            .base
            .map(|b| {
                // The decoders draw a patch's base in the patch's frame.
                let b_src = &srcs[b];
                assert_eq!(
                    (b_src.frame.w, b_src.frame.h, b_src.gx),
                    (src.frame.w, src.frame.h, src.gx),
                    "{}: a patch's frame is its base's",
                    src.png
                );
                format!("\n            &{}", b_src.symbol)
            })
            .unwrap_or_else(|| " NULL".into());
        assert!(matches!(src.gx, 1 | 2), "{}: gx is 1 or 2", src.png);
        let seek = if seek_table(c).is_some() {
            format!("SEEK_{i}")
        } else {
            "NULL".into()
        };
        format!(
            "{{{}, {}, {x}, {y}, {w}, {h}, {}, {clear}, {}, {}, ROWS_{i}, RUNS, {seek},{base}}}",
            src.frame.w, src.frame.h, src.gx, src.anchor.0, src.anchor.1
        )
    };
    let group = |prefix: &str| -> Vec<usize> {
        (0..srcs.len())
            .filter(|&i| srcs[i].symbol.starts_with(prefix))
            .collect()
    };
    let barbs = |prefix: &str, decl: &str| {
        let mut out = format!("\nconst scene_image {decl} = {{\n");
        for who in 0..2 {
            out += "    {\n";
            for i in group(&format!("{prefix}[{who}]")) {
                let _ = writeln!(out, "        {},", init(i));
            }
            out += "    },\n";
        }
        out += "};\n";
        out
    };
    s += &barbs("scene_rmr_barb", "scene_rmr_barb[2][SCENE_RMR_POSES]");
    s += &barbs(
        "scene_rmr_barb_unarmed",
        "scene_rmr_barb_unarmed[2][SCENE_RMR_UNARMED_POSES]",
    );
    let list = |prefix: &str, decl: &str| {
        let mut out = format!("\nconst scene_image {decl} = {{\n");
        for i in group(prefix) {
            let _ = writeln!(out, "    {},", init(i));
        }
        out += "};\n";
        out
    };
    s += &list(
        "scene_rmr_sword[",
        "scene_rmr_sword[SCENE_RMR_SWORD_FRAMES]",
    );
    for name in ["scene_rmr_sword_stuck", "scene_rmr_arena"] {
        let i = group(name)[0];
        let _ = write!(s, "\nconst scene_image {name} =\n    {};\n", init(i));
    }
    s += &list("scene_rmr_sky[", "scene_rmr_sky[SCENE_RMR_SKIES]");
    for (i, g) in SKIES.iter().enumerate() {
        let stops: Vec<String> = g
            .stops
            .iter()
            .map(|(y, c)| format!("{{{y}, {c}}}"))
            .collect();
        let _ = write!(
            s,
            "\nstatic const scene_stop STOPS_{i}[{}] = {{{}}};\n",
            stops.len(),
            stops.join(", ")
        );
    }
    s += "\nconst scene_gradient scene_rmr_sky_gradient[SCENE_RMR_SKIES] = {\n";
    for (i, g) in SKIES.iter().enumerate() {
        let _ = writeln!(
            s,
            "    {{{}, {}, {}, {}, STOPS_{i}}},",
            g.y0,
            g.y1,
            g.seam,
            g.stops.len()
        );
    }
    s += "};\n";
    let f = flames();
    let _ = write!(
        s,
        "\nconst uint8_t scene_rmr_flames[2][4] = {{{{{}}}, {{{}}}}};\n",
        f[0].map(|v| v.to_string()).join(", "),
        f[1].map(|v| v.to_string()).join(", ")
    );
    s
}

/// Writes `app/scene_rmr_art.c` if `update`, else returns it if it is stale.
pub fn check_or_write(update: bool) -> Vec<PathBuf> {
    check_or_write_file(&generated_path(), &c_source(), update)
}
