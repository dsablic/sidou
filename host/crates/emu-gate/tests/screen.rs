// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The screen in fm1-emu: LCD snapshots against golden images (tests/golden/lcd/), and what a
//! screen change costs the main loop in instructions and SPI time.
//!
//! The goldens are 4-bit indexed PNGs in the art palette (docs/art/README.md), so every pixel
//! must be one of the 16 colours. SIDOU_LCD_GOLDEN_UPDATE=1 rewrites them; a mismatch writes
//! the screen as `<name>.actual.png` into the fm1 build directory for a look.

mod common;

use std::path::{Path, PathBuf};

use common::{build_dir, Firmware, Machine};
use fm1_core::Bus;

const BOOT_MS: u64 = 500;
const W: usize = 240;
/// platform/fm1/lcd.c: SPI1 at 60 MHz / (4 + 1).
const SPI_HZ: f64 = 12e6;
/// platform/fm1/ui.h
const UI_STATUS: u8 = 0;
const UI_CHARSET: u8 = 1;
const UI_PERIOD_MS: u64 = 40;
/// A MIDI note's key id at octave 0 (note key 0 = F3 = MIDI 53, id 14).
const A4_KEY: u8 = 14 + 69 - 53;

/// The art palette (docs/art/README.md) in sRGB; the panel gets its RGB565 truncation.
const SRGB: [[u8; 3]; 16] = [
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
const BLUE: u8 = 6;

fn rgb565(c: [u8; 3]) -> u16 {
    (u16::from(c[0] & 0xf8) << 8) | (u16::from(c[1] & 0xfc) << 3) | u16::from(c[2] >> 3)
}

fn fixture(what: &str) -> Option<(PathBuf, Firmware)> {
    let Some(build) = build_dir() else {
        common::skip_message(what);
        return None;
    };
    let fw = Firmware::load(&build);
    Some((build, fw))
}

fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    m.run_ms(BOOT_MS);
    m
}

fn show(m: &mut Machine, fw: &Firmware, screen: u8) {
    m.soc.write8(fw.symbol("ui_screen"), screen);
}

/// The panel as palette indexes; panics on a colour outside the palette.
fn indexed(fb: &[u16]) -> Vec<u8> {
    let pal: Vec<u16> = SRGB.iter().map(|&c| rgb565(c)).collect();
    fb.iter()
        .enumerate()
        .map(|(i, &p)| {
            pal.iter().position(|&c| c == p).unwrap_or_else(|| {
                panic!(
                    "pixel ({}, {}) is {p:#06x}, not a palette colour",
                    i % W,
                    i / W
                )
            }) as u8
        })
        .collect()
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/golden/lcd")
}

fn write_png(path: &Path, idx: &[u8]) {
    let file =
        std::fs::File::create(path).unwrap_or_else(|e| panic!("create {}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W as u32, W as u32);
    enc.set_color(png::ColorType::Indexed);
    enc.set_depth(png::BitDepth::Four);
    enc.set_palette(SRGB.concat());
    let packed: Vec<u8> = idx.chunks(2).map(|p| p[0] << 4 | p[1]).collect();
    let mut w = enc.write_header().expect("PNG header");
    w.write_image_data(&packed).expect("PNG data");
}

fn read_png(path: &Path) -> Vec<u8> {
    let file = std::fs::File::open(path).unwrap_or_else(|e| {
        panic!(
            "open {}: {e}; create it with SIDOU_LCD_GOLDEN_UPDATE=1",
            path.display()
        )
    });
    let mut dec = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .expect("PNG header");
    let info = dec.info();
    assert_eq!((info.width, info.height), (W as u32, W as u32));
    assert_eq!(info.color_type, png::ColorType::Indexed);
    assert_eq!(info.bit_depth, png::BitDepth::Four);
    let plte = info.palette.as_ref().expect("a PLTE chunk").to_vec();
    assert_eq!(
        plte,
        SRGB.concat(),
        "the golden's palette is not the art palette"
    );
    let mut buf = vec![0; dec.output_buffer_size().expect("PNG size")];
    dec.next_frame(&mut buf).expect("PNG data");
    buf.iter().flat_map(|&b| [b >> 4, b & 15]).collect()
}

/// Cells (col, row, len) left out of the comparison, painted blue in both images.
fn mask(idx: &mut [u8], cells: &[(usize, usize, usize)]) {
    for &(col, row, len) in cells {
        for y in row * 8..row * 8 + 8 {
            idx[y * W + col * 8..y * W + (col + len) * 8].fill(BLUE);
        }
    }
}

fn compare_golden(build: &Path, name: &str, fb: &[u16], ignore: &[(usize, usize, usize)]) {
    let mut got = indexed(fb);
    mask(&mut got, ignore);
    let path = golden_dir().join(format!("{name}.png"));
    if std::env::var("SIDOU_LCD_GOLDEN_UPDATE").as_deref() == Ok("1") {
        std::fs::create_dir_all(golden_dir()).unwrap();
        write_png(&path, &got);
        println!("wrote {}", path.display());
        return;
    }
    let mut want = read_png(&path);
    mask(&mut want, ignore);
    let diff: Vec<usize> = (0..got.len()).filter(|&i| got[i] != want[i]).collect();
    if !diff.is_empty() {
        let actual = build.join(format!("{name}.actual.png"));
        write_png(&actual, &got);
        let i = diff[0];
        panic!(
            "{name}: {} pixels differ from {}, the first at ({}, {}) (colour {} instead of {}); \
             the screen is in {}",
            diff.len(),
            path.display(),
            i % W,
            i / W,
            got[i],
            want[i],
            actual.display()
        );
    }
}

/// The status screen's version (row 2, right-aligned before column 28): it changes with
/// every release, so the comparison leaves it out. platform/fm1/ui.c asserts at compile time
/// that "V" and the version fit these cells.
const VERSION_CELLS: (usize, usize, usize) = (14, 2, 14);

#[test]
fn the_status_screen_matches_its_golden_image() {
    let Some((build, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    assert!(m.soc.lcd.display_on, "DISPON was not sent");
    compare_golden(&build, "status", &m.soc.lcd.fb, &[VERSION_CELLS]);
    // A held note shows on the notes line.
    m.key(A4_KEY, true);
    m.run_ms(200);
    compare_golden(&build, "status-a4", &m.soc.lcd.fb, &[VERSION_CELLS]);
}

#[test]
fn the_glyph_sheet_matches_its_golden_image_and_back() {
    let Some((build, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    let status = m.soc.lcd.fb.clone();
    show(&mut m, &fw, UI_CHARSET);
    m.run_ms(200);
    compare_golden(&build, "charset", &m.soc.lcd.fb, &[]);
    // Back: the overlays are gone and the panel shows the status screen as at boot.
    show(&mut m, &fw, UI_STATUS);
    m.run_ms(200);
    assert!(
        m.soc.lcd.fb == status,
        "the status screen differs after the glyph sheet"
    );
}

#[derive(Clone, Copy, Debug)]
struct Sent {
    windows: u32,
    bytes: u32,
}

/// platform/fm1/lcd.h lcd_count.
fn sent(m: &mut Machine, fw: &Firmware) -> Sent {
    let base = fw.symbol("lcd_count");
    Sent {
        windows: m.soc.read32(base),
        bytes: m.soc.read32(base + 4),
    }
}

/// What `ms` of emulated time cost: ui_poll's instructions (its callees included) and the
/// windows and SPI bytes sent.
struct Cost {
    polls: usize,
    instructions: u64,
    worst_poll: u64,
    windows: u32,
    bytes: u32,
}

impl Cost {
    fn spi_ms(&self) -> f64 {
        f64::from(self.bytes) * 8.0 / SPI_HZ * 1e3
    }
}

fn measure(m: &mut Machine, fw: &Firmware, ms: u64) -> Cost {
    let from = m.watched_calls.len();
    let before = sent(m, fw);
    m.run_ms(ms);
    let after = sent(m, fw);
    let calls = &m.watched_calls[from..];
    Cost {
        polls: calls.len(),
        instructions: calls.iter().sum(),
        worst_poll: calls.iter().copied().max().unwrap_or(0),
        windows: after.windows - before.windows,
        bytes: after.bytes - before.bytes,
    }
}

#[test]
fn a_screen_change_costs_the_main_loop_little() {
    let Some((_, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    m.run_ms(BOOT_MS);
    // 10 frames each; the change happens at the start of the window.
    let window = 10 * UI_PERIOD_MS;
    let idle = measure(&mut m, &fw, window);
    m.key(A4_KEY, true);
    let note = measure(&mut m, &fw, window);
    show(&mut m, &fw, UI_CHARSET);
    let full = measure(&mut m, &fw, window);
    let frames = (window / UI_PERIOD_MS) as f64;
    let per_frame = |c: &Cost| c.instructions as f64 / frames;
    let report = serde_json::json!({
        "frame_ms": UI_PERIOD_MS,
        "spi_hz": SPI_HZ,
        "idle": {
            "ui_poll_calls": idle.polls,
            "instructions_per_frame": per_frame(&idle),
            "worst_ui_poll": idle.worst_poll,
            "windows": idle.windows,
        },
        "small_change_one_note": {
            "extra_instructions": note.instructions.saturating_sub(idle.instructions),
            "worst_ui_poll": note.worst_poll,
            "windows": note.windows,
            "spi_bytes": note.bytes,
            "spi_ms": note.spi_ms(),
        },
        "full_screen_change": {
            "extra_instructions": full.instructions.saturating_sub(idle.instructions),
            "worst_ui_poll": full.worst_poll,
            "windows": full.windows,
            "spi_bytes": full.bytes,
            "spi_ms": full.spi_ms(),
        },
    });
    println!("{report}");

    assert_eq!(idle.windows, 0, "an unchanged screen was sent");
    // Composing an unchanged status screen (about 20k) and the empty polls between frames
    // (about 100, one per wake-up) find nothing to send: 30k at M1e.
    assert!(
        per_frame(&idle) < 40_000.0,
        "{:.0} instructions per idle frame",
        per_frame(&idle)
    );
    // One note: one run of two or three cells.
    assert!(
        (1..=2).contains(&note.windows),
        "{} windows for one note",
        note.windows
    );
    assert!(
        note.spi_ms() < 1.0,
        "one note took {:.2} ms of SPI",
        note.spi_ms()
    );
    // 1.8k at M1e.
    assert!(
        note.instructions - idle.instructions < 10_000,
        "one note cost {} instructions",
        note.instructions - idle.instructions
    );
    // Every cell, as 30 full-row runs: the 77 ms of SPI a full redraw takes, once.
    assert_eq!(
        full.windows, 30,
        "a full screen in {} windows",
        full.windows
    );
    assert_eq!(full.bytes, 30 * (240 * 8 * 2 + 11));
    // 518k at M1e: about 580 instructions a cell, 2.2 ms of CPU at 240 MHz.
    assert!(
        full.instructions - idle.instructions < 800_000,
        "a full screen cost {} instructions",
        full.instructions - idle.instructions
    );
    // No single pass holds the main loop long: a pass composes at most one frame and renders
    // at most two runs (the one it sends and the next). 61k at M1e, 0.26 ms at 240 MHz.
    for (what, c) in [("idle", &idle), ("note", &note), ("full", &full)] {
        assert!(
            c.worst_poll < 100_000,
            "{what}: a ui_poll took {} instructions",
            c.worst_poll
        );
    }
}
