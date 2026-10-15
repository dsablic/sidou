// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The screen in fm1-emu: LCD snapshots of the views against golden images
//! (tests/golden/lcd/), reached through the panel as a player would (HOME, SELECT, ALGORITHM);
//! the wipe and raster transitions frame by frame; and what drawing costs the main loop in
//! instructions and SPI time, idle, for small changes and per frame of each transition.
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
const UI_PERIOD_MS: u64 = 40;
/// docs/art/README.md: at most about 20 ms of SPI per 40 ms frame at steady state.
const SPI_BUDGET_MS: f64 = 20.0;
/// A MIDI note's key id at octave 0 (note key 0 = F3 = MIDI 53, id 14).
const A4_KEY: u8 = 14 + 69 - 53;
/// app/panel.h: buttons and encoders.
const OCT_UP: u8 = 1;
const FX: u8 = 2;
const HOME: u8 = 8;
const SELECT: u8 = 0;
const ALGORITHM: u8 = 1;
/// app/views.c: the menu's items (CHARSET last), and its cursor cell on the first item.
const MENU_ITEMS: i32 = 4;
const MENU_CURSOR: (usize, usize, usize) = (3, 5, 1);
/// app/screen.h: the toast's row.
const TOAST_ROW: usize = 27;
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

/// A button pressed and let go, long enough for the key scan's debounce either way.
fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(30);
    m.key(id, false);
    m.run_ms(30);
}

/// Long enough for a wipe (8 frames) or a raster (14 frames) to end.
const SETTLE_MS: u64 = 700;

/// The menu, settled.
fn open_menu(m: &mut Machine) {
    press(m, HOME);
    m.run_ms(SETTLE_MS);
}

/// The glyph sheet, from the menu's last item.
fn open_charset(m: &mut Machine) {
    open_menu(m);
    m.turn(SELECT, MENU_ITEMS);
    m.run_ms(100);
    m.turn(ALGORITHM, 1);
    m.run_ms(SETTLE_MS);
}

/// The 30 cell rows of a screen, each as its 8 pixel rows.
fn cell_rows(fb: &[u16]) -> Vec<&[u16]> {
    fb.chunks(W * 8).collect()
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
fn a_button_change_shows_a_toast_that_goes_away() {
    let Some((build, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    let boot = indexed(&m.soc.lcd.fb);
    press(&mut m, OCT_UP);
    m.run_ms(200);
    compare_golden(&build, "status-toast", &m.soc.lcd.fb, &[VERSION_CELLS]);
    // app/screen.h SCREEN_TOAST_MS: 1.5 s, then the row is the view's again.
    m.run_ms(1500);
    let now = indexed(&m.soc.lcd.fb);
    let row = |idx: &[u8]| idx[TOAST_ROW * 8 * W..(TOAST_ROW + 1) * 8 * W].to_vec();
    assert!(row(&now) == row(&boot), "the toast stayed");
}

#[test]
fn the_menu_matches_its_golden_image_and_its_cursor_blinks() {
    let Some((build, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    open_menu(&mut m);
    // The cursor's phase depends on the frame count, so the golden leaves its cell out and the
    // blink is checked here: a light blue block and the blue background, 8 frames each.
    compare_golden(&build, "menu", &m.soc.lcd.fb, &[VERSION_CELLS, MENU_CURSOR]);
    let (cc, cr, _) = MENU_CURSOR;
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..16 {
        m.run_ms(UI_PERIOD_MS);
        seen.insert(indexed(&m.soc.lcd.fb)[(cr * 8 + 4) * W + cc * 8 + 4]);
    }
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        [6, 14],
        "blue and light blue"
    );
}

#[test]
fn the_glyph_sheet_opens_from_the_menu_and_home_goes_back() {
    let Some((build, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    let status = m.soc.lcd.fb.clone();
    open_charset(&mut m);
    compare_golden(&build, "charset", &m.soc.lcd.fb, &[]);
    // Back twice: the overlays are gone and the panel shows the status screen as at boot.
    press(&mut m, HOME);
    m.run_ms(SETTLE_MS);
    compare_golden(&build, "menu", &m.soc.lcd.fb, &[VERSION_CELLS, MENU_CURSOR]);
    press(&mut m, HOME);
    m.run_ms(SETTLE_MS);
    assert!(
        m.soc.lcd.fb == status,
        "the status screen differs after the glyph sheet"
    );
}

/// Each cell row of the panel as S (the status screen's), M (the menu's), B (both: the rows
/// they share), E (the wipe's white edge) or ? (none of them).
fn classify(fb: &[u16], status: &[u16], menu: &[u16]) -> String {
    let (cc, cr, _) = MENU_CURSOR;
    let same = |row: usize, a: &[u16], b: &[u16]| {
        (0..8 * W).all(|i| {
            let x = i % W;
            (row == cr && x / 8 == cc) || a[i] == b[i]
        })
    };
    let (f, s, mm) = (cell_rows(fb), cell_rows(status), cell_rows(menu));
    (0..30)
        .map(|r| match (same(r, f[r], s[r]), same(r, f[r], mm[r])) {
            (true, true) => 'B',
            (true, false) => 'S',
            (false, true) => 'M',
            _ if f[r].iter().all(|&p| p == 0xffff) => 'E',
            _ => '?',
        })
        .collect()
}

/// Samples the panel every frame until it holds still; checks every sample is the menu over
/// the status screen with at most an edge between them.
fn wipe_samples(m: &mut Machine, status: &[u16], menu: &[u16]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for _ in 0..20 {
        m.run_ms(UI_PERIOD_MS);
        let rows = classify(&m.soc.lcd.fb, status, menu);
        let shape = rows
            .trim_start_matches(['M', 'B'])
            .trim_start_matches('E')
            .trim_start_matches(['S', 'B']);
        assert!(shape.is_empty(), "not a wipe: {rows}");
        if out.last() != Some(&rows) {
            out.push(rows);
        }
    }
    out
}

#[test]
fn a_wipe_opens_the_menu_from_the_top_and_closes_it_from_the_bottom() {
    let Some((_, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    let status = m.soc.lcd.fb.clone();
    open_menu(&mut m);
    let menu = m.soc.lcd.fb.clone();
    press(&mut m, HOME);
    m.run_ms(SETTLE_MS);
    assert!(m.soc.lcd.fb == status);

    m.key(HOME, true);
    let down = wipe_samples(&mut m, &status, &menu);
    m.key(HOME, false);
    let menu_rows = |r: &String| r.chars().filter(|&c| c == 'M').count();
    let edges = down.iter().filter(|r| r.contains('E')).count();
    assert!(edges >= 6, "{edges} frames with an edge: {down:#?}");
    assert!(
        down.windows(2)
            .all(|w| menu_rows(&w[1]) >= menu_rows(&w[0])),
        "the menu shrank on the way down: {down:#?}"
    );
    assert!(!down.last().unwrap().contains(['S', 'E']));
    // The edge moves down: its row rises frame by frame.
    let edge_rows: Vec<usize> = down.iter().filter_map(|r| r.find('E')).collect();
    assert!(edge_rows.windows(2).all(|w| w[1] > w[0]), "{edge_rows:?}");

    m.run_ms(SETTLE_MS);
    m.key(HOME, true);
    let up = wipe_samples(&mut m, &status, &menu);
    m.key(HOME, false);
    let edge_rows: Vec<usize> = up.iter().filter_map(|r| r.find('E')).collect();
    assert!(edge_rows.len() >= 6, "{up:#?}");
    assert!(edge_rows.windows(2).all(|w| w[1] < w[0]), "{edge_rows:?}");
    assert!(!up.last().unwrap().contains(['M', 'E']));
}

#[test]
fn a_chip_change_sweeps_the_raster_over_the_border() {
    let Some((_, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    let boot = indexed(&m.soc.lcd.fb);
    // The left border column's colour per cell row, every frame.
    let border = |m: &Machine| -> Vec<u8> {
        let idx = indexed(&m.soc.lcd.fb);
        (0..30).map(|r| idx[(r * 8 + 4) * W + 4]).collect()
    };
    let ramp = [1u8, 7, 13, 3, 14, 4, 10];
    m.key(FX, true);
    let mut frames = Vec::new();
    for _ in 0..24 {
        m.run_ms(UI_PERIOD_MS);
        frames.push(border(&m));
    }
    m.key(FX, false);
    let swept: Vec<&Vec<u8>> = frames
        .iter()
        .filter(|b| b.iter().any(|&c| c != 14))
        .collect();
    assert!(swept.len() >= 10, "{} frames of raster", swept.len());
    for b in &swept {
        for r in 1..30 {
            let k = ramp
                .iter()
                .position(|&c| c == b[r - 1])
                .expect("a ramp colour");
            assert_eq!(b[r], ramp[(k + 1) % 7], "bands in ramp order: {b:?}");
        }
    }
    let distinct: std::collections::BTreeSet<&Vec<u8>> = swept.iter().copied().collect();
    assert!(distinct.len() >= 6, "the bands did not move");
    m.run_ms(2000);
    let now = indexed(&m.soc.lcd.fb);
    assert!(
        (0..30).all(|r| now[(r * 8 + 4) * W + 4] == boot[(r * 8 + 4) * W + 4]),
        "the border did not come back"
    );
}

#[derive(Clone, Copy, Debug, Default)]
struct Sent {
    windows: u32,
    bytes: u32,
}

impl Sent {
    fn spi_ms(self) -> f64 {
        f64::from(self.bytes) * 8.0 / SPI_HZ * 1e3
    }
}

/// platform/fm1/lcd.h lcd_count.
fn sent(m: &mut Machine, fw: &Firmware) -> Sent {
    let base = fw.symbol("lcd_count");
    Sent {
        windows: m.soc.read32(base),
        bytes: m.soc.read32(base + 4),
    }
}

/// What a stretch of emulated time cost: ui_poll's instructions (its callees included), and
/// the windows and SPI bytes sent in each UI_PERIOD_MS frame of it.
struct Cost {
    polls: usize,
    instructions: u64,
    worst_poll: u64,
    frames: Vec<Sent>,
}

impl Cost {
    fn total(&self) -> Sent {
        self.frames.iter().fold(Sent::default(), |a, f| Sent {
            windows: a.windows + f.windows,
            bytes: a.bytes + f.bytes,
        })
    }

    fn worst_frame_ms(&self) -> f64 {
        self.frames.iter().map(|f| f.spi_ms()).fold(0.0, f64::max)
    }

    fn instructions_per_frame(&self) -> f64 {
        self.instructions as f64 / self.frames.len() as f64
    }

    fn json(&self) -> serde_json::Value {
        let t = self.total();
        serde_json::json!({
            "ui_poll_calls": self.polls,
            "instructions": self.instructions,
            "instructions_per_frame": self.instructions_per_frame(),
            "worst_ui_poll": self.worst_poll,
            "windows": t.windows,
            "spi_bytes": t.bytes,
            "spi_ms": t.spi_ms(),
            "worst_frame_spi_ms": self.worst_frame_ms(),
            "per_frame": self.frames.iter().map(|f| [f.windows, f.bytes]).collect::<Vec<_>>(),
        })
    }
}

/// `frames` frames, after `act` (a key going down, say) at the start of the first.
fn measure(m: &mut Machine, fw: &Firmware, frames: u64, act: impl FnOnce(&mut Machine)) -> Cost {
    let from = m.watched_calls.len();
    act(m);
    let mut per = Vec::new();
    for _ in 0..frames {
        let before = sent(m, fw);
        m.run_ms(UI_PERIOD_MS);
        let after = sent(m, fw);
        per.push(Sent {
            windows: after.windows - before.windows,
            bytes: after.bytes - before.bytes,
        });
    }
    let calls = &m.watched_calls[from..];
    Cost {
        polls: calls.len(),
        instructions: calls.iter().sum(),
        worst_poll: calls.iter().copied().max().unwrap_or(0),
        frames: per,
    }
}

#[test]
fn screen_changes_and_transitions_stay_within_the_budgets() {
    let Some((_, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    m.run_ms(BOOT_MS);
    let idle = measure(&mut m, &fw, 10, |_| {});
    let note = measure(&mut m, &fw, 10, |m| m.key(A4_KEY, true));
    m.key(A4_KEY, false);
    m.run_ms(200);
    let menu = measure(&mut m, &fw, 16, |m| m.key(HOME, true));
    m.key(HOME, false);
    m.run_ms(SETTLE_MS);
    let menu_idle = measure(&mut m, &fw, 16, |_| {});
    let back = measure(&mut m, &fw, 16, |m| m.key(HOME, true));
    m.key(HOME, false);
    m.run_ms(SETTLE_MS);
    let raster = measure(&mut m, &fw, 24, |m| m.key(FX, true));
    m.key(FX, false);
    let report = serde_json::json!({
        "frame_ms": UI_PERIOD_MS,
        "spi_hz": SPI_HZ,
        "idle": idle.json(),
        "one_note": note.json(),
        "wipe_down_to_the_menu": menu.json(),
        "menu_idle_cursor_blinking": menu_idle.json(),
        "wipe_up_to_the_status_screen": back.json(),
        "chip_change_raster_and_toast": raster.json(),
    });
    println!("{report}");

    let extra = |c: &Cost| {
        c.instructions
            .saturating_sub(idle.instructions_per_frame() as u64 * c.frames.len() as u64)
    };
    assert_eq!(idle.total().windows, 0, "an unchanged screen was sent");
    // Composing an unchanged status screen (about 30k) and the empty polls between frames
    // (about 390, one per wake-up) find nothing to send: 37k at M1e Task 2.
    assert!(
        idle.instructions_per_frame() < 40_000.0,
        "{:.0} instructions per idle frame",
        idle.instructions_per_frame()
    );
    // One note: one run of two or three cells.
    assert!(
        (1..=2).contains(&note.total().windows),
        "{} windows for one note",
        note.total().windows
    );
    assert!(
        note.total().spi_ms() < 1.0,
        "one note took {:.2} ms of SPI",
        note.total().spi_ms()
    );
    assert!(
        extra(&note) < 10_000,
        "one note cost {} instructions",
        extra(&note)
    );
    // The menu blinks its cursor: one cell every 8 frames.
    assert!(
        menu_idle.total().windows <= 2,
        "{} windows idle in the menu",
        menu_idle.total().windows
    );
    // The transitions: every frame inside the SPI budget. A wipe sends each row that changes
    // once and its edge row once more, a raster step every other frame the 116 border cells.
    for (what, c) in [
        ("wipe down", &menu),
        ("wipe up", &back),
        ("raster", &raster),
    ] {
        assert!(
            c.worst_frame_ms() <= SPI_BUDGET_MS,
            "{what}: a frame took {:.1} ms of SPI",
            c.worst_frame_ms()
        );
        assert!(
            c.worst_poll < 100_000,
            "{what}: a ui_poll took {} instructions",
            c.worst_poll
        );
    }
    let full_screen = 30 * (240 * 8 * 2 + 11);
    for (what, c) in [("wipe down", &menu), ("wipe up", &back)] {
        let t = c.total();
        assert!(
            t.bytes <= full_screen + 8 * (240 * 8 * 2 + 11),
            "{what}: {} bytes",
            t.bytes
        );
        // A full screen's rendering (about 580 instructions a cell) and the edge rows.
        assert!(extra(c) < 800_000, "{what}: {} instructions", extra(c));
    }
    let busy = raster.frames.iter().filter(|f| f.windows > 0).count();
    assert!(
        (7..=12).contains(&busy),
        "the raster sent in {busy} of 24 frames, not every other"
    );
    for (what, c) in [("idle", &idle), ("note", &note), ("menu idle", &menu_idle)] {
        assert!(
            c.worst_poll < 100_000,
            "{what}: a ui_poll took {} instructions",
            c.worst_poll
        );
    }
}
