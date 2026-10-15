// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The screen in fm1-emu: LCD snapshots of the views against golden images
//! (tests/golden/lcd/), reached through the panel as a player would (HOME, SELECT, ALGORITHM);
//! the boot sequence, short and full, and skipping it; the wipe and raster transitions frame
//! by frame; and what drawing costs the main loop in instructions and SPI time, idle, for
//! small changes, per frame of each transition, and for the animated boot and About.
//!
//! The goldens are 4-bit indexed PNGs in the art palette (docs/art/README.md), so every pixel
//! must be one of the 16 colours. SIDOU_LCD_GOLDEN_UPDATE=1 rewrites them; a mismatch writes
//! the screen as `<name>.actual.png` into the fm1 build directory for a look.

mod common;

use std::path::{Path, PathBuf};

use common::{build_dir, Firmware, Machine};
use fm1_core::Bus;

/// Past the short boot (app/boot.h: the title card for 1.2 s from the first frame, then an
/// 8-frame wipe to the status screen). 48 frames after M1e Task 2's 500 ms, a whole number of
/// cursor blinks (16 frames), so that the measurements' frames and the menu cursor's phase
/// fall as they did.
const BOOT_MS: u64 = 2420;
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
/// app/views.h: the menu's view items, and its cursor cell on the first item.
const MENU_CHARSET: i32 = 3;
const MENU_ABOUT: i32 = 4;
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
    m.turn(SELECT, MENU_CHARSET);
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
    // An idle frame composes nothing (app/screen.h): the views compare what they show with
    // their last compose, and the polls between frames (about 390, one per wake-up) return at
    // once with nothing to send. 6.6k at M1e Task 2, from 37k when every frame composed.
    assert!(
        idle.instructions_per_frame() < 7_500.0,
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
    // Above idle: one compose of the status screen (about 26k) and the run.
    assert!(
        extra(&note) < 30_000,
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

/// The boot's version text, left out as the status screen's: the prompt's banner (row 3) and
/// the title card's version line (row 18), inside the border.
const BANNER: (usize, usize, usize) = (1, 3, 28);
const TITLE_VERSION: (usize, usize, usize) = (1, 18, 28);
/// app/boot.c: the prompt's cursor after RUN.
const RUN_CURSOR: (usize, usize, usize) = (5, 13, 1);

#[test]
fn the_short_boot_shows_the_title_card_then_the_status_screen() {
    let Some((build, fw)) = fixture("the boot screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.run_ms(700);
    assert!(m.soc.lcd.display_on, "DISPON was not sent");
    compare_golden(&build, "boot-title", &m.soc.lcd.fb, &[TITLE_VERSION]);
    m.run_ms(BOOT_MS - 700);
    compare_golden(&build, "status", &m.soc.lcd.fb, &[VERSION_CELLS]);
}

/// HOME held at power-on, until the key scan has seen it (it starts with the interrupts, after
/// the LCD's init and the boot's first frame, about 380 ms in).
fn hold_home_through_power_on(m: &mut Machine) {
    m.key(HOME, true);
    m.run_ms(600);
    m.key(HOME, false);
}

#[test]
fn home_held_at_power_on_plays_the_full_boot() {
    let Some((build, fw)) = fixture("the boot screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    hold_home_through_power_on(&mut m);
    // app/boot.h, from the full boot's first frame (about 420 ms in): LOADING from 3.0 s to
    // 5.6 s, RUN typed by 6.5 s, the title card from 7.0 s to 10.0 s, then the wipe.
    m.run_ms(4300 - 600);
    // The loading stripes: colours on the border other than the status screen's.
    let idx = indexed(&m.soc.lcd.fb);
    let border: std::collections::BTreeSet<u8> = (0..W).map(|y| idx[y * W + 3]).collect();
    assert!(
        border.len() >= 4,
        "no loading stripes on the border: {border:?}"
    );
    compare_golden(&build, "boot-loading", &m.soc.lcd.fb, &[BANNER]);
    m.run_ms(7200 - 4300);
    // Still the prompt: its rows 7-13 hold READY., LOAD..., SEARCHING..., LOADING, READY.
    // and RUN, where the status screen has its sound parameters.
    let status = booted(&fw);
    assert!(
        cell_rows(&m.soc.lcd.fb)[7..14] != cell_rows(&status.soc.lcd.fb)[7..14],
        "the status screen, not the boot's prompt"
    );
    compare_golden(&build, "boot-ready", &m.soc.lcd.fb, &[BANNER, RUN_CURSOR]);
    m.run_ms(8000 - 7200);
    compare_golden(&build, "boot-title-full", &m.soc.lcd.fb, &[TITLE_VERSION]);
    m.run_ms(11_200 - 8000);
    compare_golden(&build, "status", &m.soc.lcd.fb, &[VERSION_CELLS]);
}

#[test]
fn a_button_skips_the_boot_and_does_nothing_else() {
    let Some((build, fw)) = fixture("the boot screen test") else {
        return;
    };
    // FX would toggle the chip and OCT+ step the octave; either shows on the status screen and
    // as a toast.
    // Pressed after app/boot.h BOOT_HOLD_MS, so that HOME does not mean "the full boot".
    for button in [FX, OCT_UP, HOME] {
        let mut m = Machine::boot(&fw.image);
        m.run_ms(900);
        press(&mut m, button);
        m.run_ms(SETTLE_MS);
        compare_golden(&build, "status", &m.soc.lcd.fb, &[VERSION_CELLS]);
    }
}

#[test]
fn a_note_skips_the_boot_and_plays() {
    let Some((build, fw)) = fixture("the boot screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.run_ms(600);
    let from = m.soc.periph.audio.len() / 2;
    m.key(A4_KEY, true);
    m.run_ms(SETTLE_MS);
    compare_golden(&build, "status-a4", &m.soc.lcd.fb, &[VERSION_CELLS]);
    let peak = m.left()[from..]
        .iter()
        .map(|s| s.unsigned_abs())
        .max()
        .unwrap_or(0);
    assert!(peak > 1000, "the note did not play: peak {peak}");
}

/// About, from the menu's last item.
fn open_about(m: &mut Machine) {
    open_menu(m);
    m.turn(SELECT, MENU_ABOUT);
    m.run_ms(100);
    m.turn(ALGORITHM, 1);
    m.run_ms(SETTLE_MS);
}

/// The first credits page names SIDOU and its version (row 12).
const ABOUT_VERSION: (usize, usize, usize) = (1, 12, 28);

#[test]
fn about_opens_from_the_menu_and_home_goes_back() {
    let Some((build, fw)) = fixture("the About test") else {
        return;
    };
    let mut m = booted(&fw);
    open_about(&mut m);
    compare_golden(&build, "about", &m.soc.lcd.fb, &[ABOUT_VERSION]);
    // The cracktro moves every frame.
    let before = m.soc.lcd.fb.clone();
    m.run_ms(UI_PERIOD_MS);
    assert!(m.soc.lcd.fb != before, "About stood still");
    press(&mut m, HOME);
    m.run_ms(SETTLE_MS);
    // The cursor is on ABOUT now, blinking in its own phase.
    let (cc, cr, cl) = MENU_CURSOR;
    let on_about = (cc, cr + MENU_ABOUT as usize, cl);
    compare_golden(
        &build,
        "menu",
        &m.soc.lcd.fb,
        &[VERSION_CELLS, MENU_CURSOR, on_about],
    );
}

/// Cells a frame sent: each a 128-byte window body, each window 11 command bytes.
fn cells(f: &Sent) -> u32 {
    (f.bytes - f.windows * 11) / 128
}

fn animated_json(c: &Cost) -> serde_json::Value {
    let n = c.frames.len() as f64;
    let per: Vec<u32> = c.frames.iter().map(cells).collect();
    let t = c.total();
    serde_json::json!({
        "frames": c.frames.len(),
        "cells_per_frame": f64::from(per.iter().sum::<u32>()) / n,
        "max_cells": per.iter().max(),
        "windows_per_frame": f64::from(t.windows) / n,
        "spi_bytes_per_frame": f64::from(t.bytes) / n,
        "spi_ms_per_frame": t.spi_ms() / n,
        "worst_frame_spi_ms": c.worst_frame_ms(),
        "instructions_per_frame": c.instructions_per_frame(),
        "worst_ui_poll": c.worst_poll,
        "per_frame": c.frames.iter().map(|f| [f.windows, f.bytes]).collect::<Vec<_>>(),
    })
}

#[test]
fn the_boot_and_about_stay_within_their_budgets() {
    let Some((_, fw)) = fixture("the boot and About budget test") else {
        return;
    };
    // The full boot, HOME held at power-on: every frame after the first (drawn whole before
    // the display is on) inside the SPI budget.
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    hold_home_through_power_on(&mut m);
    let boot = measure(&mut m, &fw, 260, |_| {});
    // About: 12 s, past two pages and their fades.
    open_about(&mut m);
    let about = measure(&mut m, &fw, 300, |_| {});
    let report = serde_json::json!({
        "frame_ms": UI_PERIOD_MS,
        "spi_hz": SPI_HZ,
        "full_boot": animated_json(&boot),
        "about": animated_json(&about),
    });
    println!("{report}");
    for (what, c) in [("boot", &boot), ("about", &about)] {
        assert!(
            c.worst_frame_ms() <= SPI_BUDGET_MS,
            "{what}: a frame took {:.1} ms of SPI",
            c.worst_frame_ms()
        );
    }
    // docs/plans/2026-10-15-m1e-look-and-feel.md: About trimmed from the prototype's 240
    // cells a frame to about 120.
    let n = about.frames.len() as f64;
    let avg = f64::from(about.frames.iter().map(cells).sum::<u32>()) / n;
    assert!(avg <= 120.0, "About sends {avg:.1} cells a frame");
    // About composes every frame: its busiest frame is one compose and a run (ui_poll),
    // and every frame costs about the same.
    assert!(
        about.worst_poll < ABOUT_WORST_POLL,
        "About: a ui_poll took {} instructions",
        about.worst_poll
    );
    assert!(
        about.instructions_per_frame() < ABOUT_PER_FRAME,
        "About: {:.0} instructions a frame",
        about.instructions_per_frame()
    );
    assert!(
        boot.worst_poll < BOOT_WORST_POLL,
        "boot: a ui_poll took {} instructions",
        boot.worst_poll
    );
    assert!(
        boot.instructions_per_frame() < BOOT_PER_FRAME,
        "boot: {:.0} instructions a frame",
        boot.instructions_per_frame()
    );
}

/// app/screen.h SCREEN_WIPE_ROWS and the edge row: what a wipe frame sends at most.
const WIPE_CELLS: u32 = (4 + 1) * 30;

/// 40 ms windows from now until a wipe has run and ended, as `measure` counts them: those
/// during which a wipe ran (read from the screen's state at each window's start and end).
fn wipe_windows(m: &mut Machine, fw: &Firmware, act: impl FnOnce(&mut Machine)) -> Vec<Sent> {
    act(m);
    let mut out = Vec::new();
    let mut was = common::screen_wiping(m, fw);
    for _ in 0..30 {
        let before = sent(m, fw);
        m.run_ms(UI_PERIOD_MS);
        let after = sent(m, fw);
        let wiping = common::screen_wiping(m, fw);
        if was || wiping {
            out.push(Sent {
                windows: after.windows - before.windows,
                bytes: after.bytes - before.bytes,
            });
        }
        if was && !wiping {
            return out;
        }
        was = wiping;
    }
    panic!("no wipe ran and ended in 30 frames");
}

fn assert_wipe_holds_still(what: &str, frames: &[Sent]) {
    assert!(frames.len() >= 6, "{what}: {} wipe frames", frames.len());
    for (k, f) in frames.iter().enumerate() {
        assert!(
            cells(f) <= WIPE_CELLS && f.spi_ms() <= SPI_BUDGET_MS,
            "{what}: 40 ms {k} of the wipe sent {} cells, {:.1} ms of SPI ({f:?})",
            cells(f),
            f.spi_ms()
        );
    }
}

/// Every wipe into an animated view sends its rows and nothing more, inside the SPI budget:
/// the wipe into About, started at every phase against the firmware's frames, and the full
/// boot's wipe into the title card (the view under a wipe holds still, app/screen.h).
#[test]
fn every_wipe_into_an_animated_view_sends_only_its_rows() {
    let Some((_, fw)) = fixture("the wipe budget test") else {
        return;
    };
    let mut m = booted(&fw);
    open_menu(&mut m);
    m.turn(SELECT, MENU_ABOUT);
    m.run_ms(SETTLE_MS);
    let mut worst = Sent::default();
    let mut most = 0;
    for phase in (0..UI_PERIOD_MS).step_by(5) {
        m.run_ms(phase + 1);
        let frames = wipe_windows(&mut m, &fw, |m| m.turn(ALGORITHM, 1));
        assert_wipe_holds_still(&format!("About at phase {phase} ms"), &frames);
        for f in &frames {
            most = most.max(cells(f));
            if f.bytes > worst.bytes {
                worst = *f;
            }
        }
        press(&mut m, HOME);
        m.run_ms(SETTLE_MS);
    }
    println!(
        "wipe into About: at most {most} cells, worst 40 ms {:.2} ms of SPI",
        worst.spi_ms()
    );

    let mut m = Machine::boot(&fw.image);
    hold_home_through_power_on(&mut m);
    // The title card wipes in 7.0 s after the full boot's first frame.
    m.run_ms(6800 - 600);
    let frames = wipe_windows(&mut m, &fw, |_| {});
    assert_wipe_holds_still("the title card", &frames);
    let title = frames.iter().map(|f| f.spi_ms()).fold(0.0, f64::max);
    println!(
        "wipe into the title card: at most {} cells, worst 40 ms {title:.2} ms of SPI",
        frames.iter().map(cells).max().unwrap_or(0)
    );
}

/// Main-loop instructions in ui_poll (fm1-emu). About at M1e Task 3: its busiest ui_poll (a
/// compose and a run) 105k, 161k a frame with the rendering of its 93 cells, 4.0M a second
/// (1.7 % of 240 MHz). The full boot: 64k and 50k.
const ABOUT_WORST_POLL: u64 = 120_000;
const ABOUT_PER_FRAME: f64 = 180_000.0;
const BOOT_WORST_POLL: u64 = 75_000;
const BOOT_PER_FRAME: f64 = 60_000.0;
