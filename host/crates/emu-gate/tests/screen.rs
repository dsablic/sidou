// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The screen in fm1-emu: LCD snapshots of the views against golden images
//! (tests/golden/lcd/), reached through the panel as a player would (HOME, SELECT, ALGORITHM);
//! the boot sequence, short and full, and skipping it; LIVE's scopes following the notes; the
//! CHIP view's pages and what MIDI and the knobs change on them;
//! wipe and raster transitions frame by frame; and what drawing costs the main loop in
//! instructions and SPI time, idle, for small changes, per frame of each transition, and for
//! the animated boot, About and LIVE.
//!
//! The goldens are 4-bit indexed PNGs in the art palette (docs/art/README.md), so every pixel
//! must be one of the 16 colours. SIDOU_LCD_GOLDEN_UPDATE=1 rewrites them; a mismatch writes
//! the screen as `<name>.actual.png` into the fm1 build directory for a look.

mod common;

use std::path::{Path, PathBuf};

use common::{build_dir, Firmware, Machine};
use fm1_core::Bus;

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
/// app/views.h: the menu's title cards option and view items (STATUS is
/// common::open_status's), and its cursor cell on the first item.
const MENU_CHIP: i32 = sidou_sys::VIEWS_MENU_CHIP as i32;
const MENU_CARDS: i32 = sidou_sys::VIEWS_MENU_CARDS as i32;
const MENU_SCENE: i32 = sidou_sys::VIEWS_MENU_SCENE as i32;
const MENU_CHARSET: i32 = sidou_sys::VIEWS_MENU_CHARSET as i32;
const MENU_ABOUT: i32 = sidou_sys::VIEWS_MENU_ABOUT as i32;
const MENU_ITEMS: i32 = sidou_sys::VIEWS_MENU_ITEMS as i32;
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

/// Past the short boot (app/boot.h: the title card for 1.2 s from the first frame, then an
/// 8-frame wipe to LIVE): until the boot view is gone (common::run_past_boot).
fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    common::run_past_boot(&mut m, fw);
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

/// SETTLE_MS, then on to half way between two frames (common::to_mid_frame): the panel then
/// holds a whole frame, and what follows starts at the same phase every run.
fn settle(m: &mut Machine, fw: &Firmware) {
    m.run_ms(SETTLE_MS);
    common::to_mid_frame(m, fw);
}

/// The menu, settled.
fn open_menu(m: &mut Machine, fw: &Firmware) {
    press(m, HOME);
    settle(m, fw);
}

/// The menu's cursor on item `item`: to the top, then down. fm1-emu turns an encoder a
/// detent every 8 ms, so each turn is waited out before the next.
fn select(m: &mut Machine, item: i32) {
    m.turn(SELECT, -MENU_ITEMS);
    m.run_ms(10 * MENU_ITEMS as u64);
    m.turn(SELECT, item);
    m.run_ms(10 * item as u64);
}

/// The glyph sheet, from the menu.
fn open_charset(m: &mut Machine, fw: &Firmware) {
    open_menu(m, fw);
    select(m, MENU_CHARSET);
    m.run_ms(100);
    m.turn(ALGORITHM, 1);
    settle(m, fw);
}

/// A byte of the views (platform/fm1/ui.c `ui`) at `offset` in the device's layout
/// (sidou_sys::ilp32::views), the symbol's size checked against it.
fn ui_byte(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    use object::{Object, ObjectSymbol};
    let file = object::File::parse(&*fw.elf).expect("parse the firmware ELF");
    let sym = file
        .symbols()
        .find(|s| s.name() == Ok("ui"))
        .expect("no symbol ui in the firmware ELF");
    assert_eq!(
        sym.size(),
        sidou_sys::ilp32::views::SIZE as u64,
        "the views' layout moved"
    );
    m.soc.read8(sym.address() as u32 + offset as u32)
}

/// A field of the views' title card (app/title_card.h), e.g. ilp32::title_card::phase.
fn card_byte(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    ui_byte(m, fw, sidou_sys::ilp32::views::card + field)
}

/// The instrument's octave and chip model (app/live.h), through the views' pointer to it.
fn octave_and_model(m: &mut Machine, fw: &Firmware) -> (u8, u8) {
    let l = m
        .soc
        .read32(fw.symbol("ui") + sidou_sys::ilp32::views::l as u32);
    (
        m.soc.read8(l + sidou_sys::ilp32::live::octave as u32),
        m.soc.read8(l + sidou_sys::ilp32::live::model as u32),
    )
}

fn showing_card(m: &mut Machine, fw: &Firmware) -> bool {
    common::top_view(m, fw) == fw.symbol("title_card_view")
}

/// Longer than a title card from its start to the end of the cut under it (about 2.4 s).
const CARD_TIMEOUT_MS: u64 = 4000;

/// A frame at a time until the title card on top is gone and the cut under it has ended;
/// returns how long that took.
fn past_card(m: &mut Machine, fw: &Firmware) -> u64 {
    let from = m.ms();
    while showing_card(m, fw) || common::screen_animating(m, fw) {
        assert!(
            m.ms() < from + CARD_TIMEOUT_MS,
            "the title card did not end"
        );
        m.run_ms(UI_PERIOD_MS);
    }
    m.ms() - from
}

/// The menu's CARDS off (ALGORITHM counter-clockwise on it), and back to LIVE with no card.
fn cards_off(m: &mut Machine, fw: &Firmware) {
    open_menu(m, fw);
    select(m, MENU_CARDS);
    m.turn(ALGORITHM, -1);
    m.run_ms(100);
    assert_eq!(ui_byte(m, fw, sidou_sys::ilp32::views::cards), 0);
    m.turn(SELECT, -MENU_ITEMS); // the cursor back on the first item
    m.run_ms(10 * MENU_ITEMS as u64);
    press(m, HOME);
    settle(m, fw);
    assert_eq!(common::top_view(m, fw), fw.symbol("live_view_view"));
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

/// Only the cells (col, row, len) of the panel against those of a golden image.
fn compare_golden_cells(build: &Path, name: &str, fb: &[u16], cells: &[(usize, usize, usize)]) {
    let got = indexed(fb);
    let want = read_png(&golden_dir().join(format!("{name}.png")));
    for &(col, row, len) in cells {
        for y in row * 8..row * 8 + 8 {
            let span = y * W + col * 8..y * W + (col + len) * 8;
            if got[span.clone()] != want[span] {
                let actual = build.join(format!("{name}.cells.actual.png"));
                write_png(&actual, &got);
                panic!(
                    "{name}: cells ({col}, {row}) x {len} differ from the golden; the screen is \
                     in {}",
                    actual.display()
                );
            }
        }
    }
}

/// LIVE's first voice: its note's cells (app/live_view.c, the info column).
const VOICE1_NOTE: (usize, usize, usize) = (25, 2, 4);

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
    common::open_status(&mut m, &fw);
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
    common::open_status(&mut m, &fw);
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
    open_menu(&mut m, &fw);
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

/// app/panel.h PANEL_BTN_SAVE and K1.
const SAVE: u8 = 9;
const K1: u8 = 2;

/// Rows 0..22, every cell: what the views under MIDI learn's frame show, theirs to test.
/// The frame is rows 23..26 (app/learn_view.h LEARN_VIEW_ROW), the toast row 27.
const ABOVE_THE_LEARN_FRAME: [(usize, usize, usize); 23] = {
    let mut rows = [(0, 0, W / 8); 23];
    let mut r = 0;
    while r < 23 {
        rows[r].1 = r;
        r += 1;
    }
    rows
};

/// MIDI learn's frame (app/learn_view.h) over the status screen as it asks for a knob, then
/// for a CC, and the toast once CC 21 came over TRS. Only rows 23..27 count, the frame's and
/// the toast's (and the border): the view under the frame is not this test's.
#[test]
fn midi_learn_prompts_match_their_golden_images() {
    let Some((build, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    m.key(SAVE, true);
    settle(&mut m, &fw);
    compare_golden(&build, "learn-knob", &m.soc.lcd.fb, &ABOVE_THE_LEARN_FRAME);
    m.turn(K1, 1);
    settle(&mut m, &fw);
    compare_golden(&build, "learn-cc", &m.soc.lcd.fb, &ABOVE_THE_LEARN_FRAME);
    m.key(SAVE, false);
    m.midi(&[0xB0, 21, 64]);
    m.run_ms(200);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(
        &build,
        "learn-learned",
        &m.soc.lcd.fb,
        &ABOVE_THE_LEARN_FRAME,
    );
    assert_eq!(
        common::top_view(&mut m, &fw),
        fw.symbol("live_view_view"),
        "the frame did not close"
    );
}

#[test]
fn the_glyph_sheet_opens_from_the_menu_and_home_goes_back() {
    let Some((build, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    // LIVE whole: the boot can end with its last rows still to go out (a frame sends at
    // most LIVE_VIEW_FRAME_CELLS), so the reference waits for them like the views after it.
    settle(&mut m, &fw);
    let live = m.soc.lcd.fb.clone();
    open_charset(&mut m, &fw);
    compare_golden(&build, "charset", &m.soc.lcd.fb, &[]);
    // Back twice: the sheet's overlays are gone and the panel shows LIVE as at boot.
    // The cursor is on CHARSET now, blinking in its own phase.
    press(&mut m, HOME);
    settle(&mut m, &fw);
    let (cc, cr, cl) = MENU_CURSOR;
    let on_charset = (cc, cr + MENU_CHARSET as usize, cl);
    compare_golden(
        &build,
        "menu",
        &m.soc.lcd.fb,
        &[VERSION_CELLS, MENU_CURSOR, on_charset],
    );
    // Back to LIVE behind THE OSCILLATOR's card.
    press(&mut m, HOME);
    assert!(showing_card(&mut m, &fw), "no card back to LIVE");
    past_card(&mut m, &fw);
    settle(&mut m, &fw);
    assert!(m.soc.lcd.fb == live, "LIVE differs after the glyph sheet");
}

/// Each cell row of the panel as S (the view under the menu's: LIVE's), M (the menu's), B
/// (both: the rows they share), E (the wipe's white edge) or ? (none of them).
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

/// Samples the panel every frame until it holds still (from half way between two frames,
/// common::to_mid_frame, so that each sample sees a frame whole); checks every sample is the
/// menu over LIVE with at most an edge between them.
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
    // Back from the menu to LIVE is a wipe up with the title cards off (with them on, THE
    // OSCILLATOR's card cuts in: title_cards_match_their_goldens_and_their_sources).
    cards_off(&mut m, &fw);
    let status = m.soc.lcd.fb.clone();
    open_menu(&mut m, &fw);
    let menu = m.soc.lcd.fb.clone();
    press(&mut m, HOME);
    settle(&mut m, &fw);
    assert!(m.soc.lcd.fb == status);

    common::to_mid_frame(&mut m, &fw);
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

    settle(&mut m, &fw);
    common::to_mid_frame(&mut m, &fw);
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
    common::open_status(&mut m, &fw);
    let boot = indexed(&m.soc.lcd.fb);
    // The left border column's colour per cell row, every frame.
    let border = |m: &Machine| -> Vec<u8> {
        let idx = indexed(&m.soc.lcd.fb);
        (0..30).map(|r| idx[(r * 8 + 4) * W + 4]).collect()
    };
    let ramp = [1u8, 7, 13, 3, 14, 4, 10];
    common::to_mid_frame(&mut m, &fw);
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
    common::to_mid_frame(m, fw);
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
    common::run_past_boot(&mut m, &fw);
    // The status screen, from the menu: LIVE, the root, moves every frame and has its own
    // budget (live_stays_within_its_budgets).
    common::open_status(&mut m, &fw);
    let idle = measure(&mut m, &fw, 10, |_| {});
    let note = measure(&mut m, &fw, 10, |m| m.key(A4_KEY, true));
    m.key(A4_KEY, false);
    m.run_ms(200);
    // HOME goes back from the status screen to the menu; ALGORITHM on STATUS opens it again.
    let menu = measure(&mut m, &fw, 16, |m| m.key(HOME, true));
    m.key(HOME, false);
    settle(&mut m, &fw);
    let menu_idle = measure(&mut m, &fw, 16, |_| {});
    let back = measure(&mut m, &fw, 16, |m| m.turn(ALGORITHM, 1));
    settle(&mut m, &fw);
    let raster = measure(&mut m, &fw, 24, |m| m.key(FX, true));
    m.key(FX, false);
    let report = serde_json::json!({
        "frame_ms": UI_PERIOD_MS,
        "spi_hz": SPI_HZ,
        "idle": idle.json(),
        "one_note": note.json(),
        "wipe_up_to_the_menu": menu.json(),
        "menu_idle_cursor_blinking": menu_idle.json(),
        "wipe_down_to_the_status_screen": back.json(),
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
        ("wipe up", &menu),
        ("wipe down", &back),
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
    for (what, c) in [("wipe up", &menu), ("wipe down", &back)] {
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
/// the title card's version line (app/boot.h BOOT_SCENE_VERSION_ROW), inside the border.
const BANNER: (usize, usize, usize) = (1, 3, 28);
const TITLE_VERSION: (usize, usize, usize) = (1, sidou_sys::BOOT_SCENE_VERSION_ROW as usize, 28);

/// The panel's pixel rows `rows` against the logo's source, assets/logo/metal-chrome.png,
/// from its pixel row `from`: the firmware shows the art pixel for pixel.
fn assert_shows_the_logo_source(what: &str, fb: &[u16], rows: std::ops::Range<usize>, from: usize) {
    let src = sidou_tests::logo::load(sidou_tests::logo::DEFAULT);
    let got = indexed(fb);
    for (k, y) in rows.clone().enumerate() {
        let want = &src.px[(from + k) * W..(from + k + 1) * W];
        if let Some(x) = (0..W).find(|&x| got[y * W + x] != want[x]) {
            panic!(
                "{what}: pixel ({x}, {y}) is colour {} where the logo's source has {}",
                got[y * W + x],
                want[x]
            );
        }
    }
}

/// The title card is Metal Chrome's scene but for the card's two lines of text over it.
fn assert_title_card_is_the_logo_source(fb: &[u16]) {
    let version = sidou_sys::BOOT_SCENE_VERSION_ROW as usize * 8;
    let key = sidou_sys::BOOT_SCENE_KEY_ROW as usize * 8;
    assert_shows_the_logo_source("the title card", fb, 0..version, 0);
    assert_shows_the_logo_source("the title card", fb, version + 8..key, version + 8);
    assert_shows_the_logo_source("the title card", fb, key + 8..W, key + 8);
}
/// app/boot.c: the prompt's cursor after RUN.
const RUN_CURSOR: (usize, usize, usize) = (5, 13, 1);

#[test]
fn the_short_boot_shows_the_title_card_then_live() {
    let Some((build, fw)) = fixture("the boot screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.run_ms(700);
    assert!(m.soc.lcd.display_on, "DISPON was not sent");
    compare_golden(&build, "boot-title", &m.soc.lcd.fb, &[TITLE_VERSION]);
    assert_title_card_is_the_logo_source(&m.soc.lcd.fb);
    let gone = common::run_past_boot(&mut m, &fw);
    // The title card shows 1.2 s from about 0.4 s in, then the 8-frame wipe.
    assert!(
        (1600..2200).contains(&gone),
        "the short boot ended at {gone} ms"
    );
    compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
}

/// HOME held at power-on, until the key scan has seen it (it starts with the interrupts, after
/// the LCD's init, about 400 ms in, and platform/fm1/main.c reads it 3 ms later).
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
    // app/boot.h, from the full boot's first frame (about 400 ms in): LOADING from 3.0 s to
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
    // and RUN, where LIVE has its second voice.
    let live = booted(&fw);
    assert!(
        cell_rows(&m.soc.lcd.fb)[7..14] != cell_rows(&live.soc.lcd.fb)[7..14],
        "LIVE, not the boot's prompt"
    );
    compare_golden(&build, "boot-ready", &m.soc.lcd.fb, &[BANNER, RUN_CURSOR]);
    m.run_ms(8000 - 7200);
    compare_golden(&build, "boot-title-full", &m.soc.lcd.fb, &[TITLE_VERSION]);
    assert_title_card_is_the_logo_source(&m.soc.lcd.fb);
    m.run_ms(11_200 - 8000);
    compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
}

#[test]
fn a_button_skips_the_boot_and_does_nothing_else() {
    let Some((build, fw)) = fixture("the boot screen test") else {
        return;
    };
    // FX would toggle the chip and OCT+ step the octave; either shows on the status screen and
    // as a toast.
    // HOME too: not held at power-on, its press is a skip like any other (app/boot.h).
    for button in [FX, OCT_UP, HOME] {
        let mut m = Machine::boot(&fw.image);
        m.run_ms(900);
        press(&mut m, button);
        settle(&mut m, &fw);
        compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
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
    settle(&mut m, &fw);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("live_view_view"));
    // LIVE names the note on voice 1, as when the note plays after the boot.
    compare_golden_cells(&build, "live-a4", &m.soc.lcd.fb, &[VOICE1_NOTE]);
    let peak = m.left()[from..]
        .iter()
        .map(|s| s.unsigned_abs())
        .max()
        .unwrap_or(0);
    assert!(peak > 1000, "the note did not play: peak {peak}");
}

/// HOME tapped as soon as the title card shows, HOME not held at power-on: it skips the short
/// boot (it is not taken for a hold, app/boot.h).
#[test]
fn home_tapped_as_soon_as_the_card_shows_skips_it() {
    let Some((build, fw)) = fixture("the boot screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    while !m.soc.lcd.display_on {
        assert!(m.ms() < 2000, "the display did not come on");
        m.run_ms(5);
    }
    let shown = m.ms();
    press(&mut m, HOME);
    let gone = common::run_past_boot(&mut m, &fw);
    assert!(
        gone < shown + 600,
        "the boot went on until {gone} ms (the display came on at {shown} ms): HOME did not skip"
    );
    compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
}

/// About, from the menu's last item.
fn open_about(m: &mut Machine, fw: &Firmware) {
    open_menu(m, fw);
    m.turn(SELECT, MENU_ABOUT); // from the first item: the menu's first opening
    m.run_ms(100);
    m.turn(ALGORITHM, 1);
    settle(m, fw);
}

/// The first credits page names SIDOU and its version (app/about.h ABOUT_HEADING_ROW).
const ABOUT_VERSION: (usize, usize, usize) = (1, sidou_sys::ABOUT_HEADING_ROW as usize, 28);

#[test]
fn about_opens_from_the_menu_and_home_goes_back() {
    let Some((build, fw)) = fixture("the About test") else {
        return;
    };
    let mut m = booted(&fw);
    open_about(&mut m, &fw);
    compare_golden(&build, "about", &m.soc.lcd.fb, &[ABOUT_VERSION]);
    // The logo at the top: the scene's lettering band (app/logo.h LOGO_METAL_BAND_ROW).
    let band = sidou_sys::LOGO_METAL_BAND_ROW as usize * 8;
    let top = sidou_sys::ABOUT_LOGO_ROW as usize * 8;
    let rows = sidou_sys::ABOUT_LOGO_ROWS as usize * 8;
    assert_shows_the_logo_source("About", &m.soc.lcd.fb, top..top + rows, band);
    // The cracktro moves every frame.
    let before = m.soc.lcd.fb.clone();
    m.run_ms(UI_PERIOD_MS);
    assert!(m.soc.lcd.fb != before, "About stood still");
    press(&mut m, HOME);
    settle(&mut m, &fw);
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
    // About: a full round of its pages (the first, SIDOU's, and the credits), 4.8 s each.
    open_about(&mut m, &fw);
    let pages = u64::from(unsafe { sidou_sys::credits_page_count }) + 1;
    let about = measure(
        &mut m,
        &fw,
        pages * u64::from(sidou_sys::ABOUT_PAGE_FRAMES),
        |_| {},
    );
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
/// What a wipe into an animated view costs the main loop: its busiest ui_poll, and its
/// instructions per 40 ms frame. Measured with Cracktro Chrome: About 65k and 118k, the title
/// card (4 rows of its scene and the edge a frame) 30k and 134k.
const WIPE_WORST_POLL: u64 = 72_000;
const WIPE_PER_FRAME: f64 = 150_000.0;

/// 40 ms windows from now until a wipe has run and ended, as `measure` counts them: those
/// during which a wipe ran (read from the screen's state at each window's start and end).
fn wipe_windows(m: &mut Machine, fw: &Firmware, act: impl FnOnce(&mut Machine)) -> Vec<Sent> {
    common::to_mid_frame(m, fw);
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
    m.watch_calls(fw.symbol("ui_poll"));
    open_menu(&mut m, &fw);
    m.turn(SELECT, MENU_ABOUT);
    settle(&mut m, &fw);
    let mut worst = Sent::default();
    let mut most = 0;
    let mut about_polls = Vec::new();
    for phase in (0..UI_PERIOD_MS).step_by(5) {
        m.run_ms(phase + 1);
        let from = m.watched_calls.len();
        let frames = wipe_windows(&mut m, &fw, |m| m.turn(ALGORITHM, 1));
        about_polls.push(wipe_instructions(&m.watched_calls[from..], frames.len()));
        assert_wipe_holds_still(&format!("About at phase {phase} ms"), &frames);
        for f in &frames {
            most = most.max(cells(f));
            if f.bytes > worst.bytes {
                worst = *f;
            }
        }
        press(&mut m, HOME);
        settle(&mut m, &fw);
    }
    let about = about_polls
        .iter()
        .fold((0, 0.0), |a, w| (a.0.max(w.0), f64::max(a.1, w.1)));
    println!(
        "wipe into About: at most {most} cells, worst 40 ms {:.2} ms of SPI; busiest ui_poll \
         {}, {:.0} instructions a frame at most",
        worst.spi_ms(),
        about.0,
        about.1
    );

    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    hold_home_through_power_on(&mut m);
    // The title card wipes in 7.0 s after the full boot's first frame.
    m.run_ms(6800 - 600);
    let from = m.watched_calls.len();
    let frames = wipe_windows(&mut m, &fw, |_| {});
    let title_polls = wipe_instructions(&m.watched_calls[from..], frames.len());
    assert_wipe_holds_still("the title card", &frames);
    let title = frames.iter().map(|f| f.spi_ms()).fold(0.0, f64::max);
    println!(
        "wipe into the title card: at most {} cells, worst 40 ms {title:.2} ms of SPI; busiest \
         ui_poll {}, {:.0} instructions a frame",
        frames.iter().map(cells).max().unwrap_or(0),
        title_polls.0,
        title_polls.1
    );
    // Revealing a bitmap (Cracktro Chrome's scene, all of the title card; its lettering band,
    // the top of About) a few rows a frame costs no more than revealing cells.
    for (what, (poll, per_frame)) in [("About", about), ("the title card", title_polls)] {
        assert!(
            poll < WIPE_WORST_POLL,
            "the wipe into {what}: a ui_poll took {poll} instructions"
        );
        assert!(
            per_frame < WIPE_PER_FRAME,
            "the wipe into {what}: {per_frame:.0} instructions a frame"
        );
    }
}

/// A wipe's busiest ui_poll and its instructions per 40 ms frame.
fn wipe_instructions(calls: &[u64], frames: usize) -> (u64, f64) {
    let worst = calls.iter().copied().max().unwrap_or(0);
    (worst, calls.iter().sum::<u64>() as f64 / frames as f64)
}

/// Main-loop instructions in ui_poll (fm1-emu). About with Cracktro Chrome (a still bitmap
/// at the top, the bars behind the scroller): its busiest ui_poll (a compose and a run) 72k,
/// 118k a frame with the rendering of its 87 cells, 3.0M a second (1.2 % of 240 MHz); with
/// Pulse Train (M1e Task 4) 91k and 144k, at Task 3 105k and 161k. The full boot: 33k and 37k
/// (Pulse Train 37k and 46k; Task 3 64k and 50k).
const ABOUT_WORST_POLL: u64 = 80_000;
const ABOUT_PER_FRAME: f64 = 130_000.0;
const BOOT_WORST_POLL: u64 = 38_000;
const BOOT_PER_FRAME: f64 = 42_000.0;

/// Note keys (key id = 14 + note - 53) of a chord over three octaves, one per voice: A3, E4
/// and E5, so the scopes show 2, 3 and 4 periods (app/live_view.c).
const CHORD: [u8; 3] = [14 + 57 - 53, 14 + 64 - 53, 14 + 76 - 53];
/// Between the chord's presses: more than a key scan frame and its debounce.
const CHORD_STAGGER_MS: u64 = 5;

/// LIVE's three scopes (app/live_view.h): 160 x 32 pixels from (8, 16), (8, 64), (8, 112).
fn scope_pixels(fb: &[u16]) -> Vec<u16> {
    let mut out = Vec::new();
    for v in 0..3 {
        let top = (2 + 6 * v) * 8;
        for y in top..top + 32 {
            out.extend_from_slice(&fb[y * W + 8..y * W + 168]);
        }
    }
    out
}

/// The scope pixels in the trace's colours (any but the background, the centre line's and
/// black), per voice.
fn trace_pixels(fb: &[u16]) -> [usize; 3] {
    let idx = indexed(fb);
    std::array::from_fn(|v| {
        let top = (2 + 6 * v) * 8;
        (top..top + 32)
            .flat_map(|y| (8..168).map(move |x| (x, y)))
            .filter(|&(x, y)| ![BLUE, 14, 0].contains(&idx[y * W + x]))
            .count()
    })
}

#[test]
fn live_matches_its_golden_images_and_its_scopes_follow_the_notes() {
    let Some((build, fw)) = fixture("the LIVE test") else {
        return;
    };
    let mut m = booted(&fw);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("live_view_view"));
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
    let idle = scope_pixels(&m.soc.lcd.fb);
    let flat = trace_pixels(&m.soc.lcd.fb);
    // At rest each trace is a flat line across the scope.
    assert_eq!(flat, [160; 3], "the traces at rest");

    // A4, held to its sustain level (SAW LEAD: decay 9).
    m.key(A4_KEY, true);
    m.run_ms(1500);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "live-a4", &m.soc.lcd.fb, &[]);
    let a4 = trace_pixels(&m.soc.lcd.fb);
    assert!(a4[0] > 200, "voice 1's trace with A4: {a4:?}");
    assert_eq!(&a4[1..], [160, 160], "voices 2 and 3 are silent");
    // A held note stands still: the trace is folded by the oscillator's phase.
    let held = scope_pixels(&m.soc.lcd.fb);
    m.run_ms(UI_PERIOD_MS);
    assert!(
        scope_pixels(&m.soc.lcd.fb) == held,
        "a sustained note's trace moved"
    );

    // Released: flat again, as at rest.
    m.key(A4_KEY, false);
    m.run_ms(1500);
    common::to_mid_frame(&mut m, &fw);
    assert!(
        scope_pixels(&m.soc.lcd.fb) == idle,
        "the scopes are not flat after the release"
    );

    // A chord: a voice each. The presses are CHORD_STAGGER_MS apart, more than a key scan
    // frame (~1.1 ms) and its debounce, so each is its own note-on, in order, and live's
    // voice allocation decides the voices (E5, A3, E4 on voices 1, 2, 3, as M1b recorded
    // them), not where the main loop's passes fall against the scan.
    for k in CHORD {
        m.key(k, true);
        m.run_ms(CHORD_STAGGER_MS);
    }
    m.run_ms(1500);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "live-chord", &m.soc.lcd.fb, &[]);
    let chord = trace_pixels(&m.soc.lcd.fb);
    assert!(
        chord.iter().all(|&n| n > 200),
        "the chord's traces: {chord:?}"
    );
    for k in CHORD {
        m.key(k, false);
    }
}

#[test]
fn live_stays_within_its_budgets() {
    let Some((_, fw)) = fixture("the LIVE budget test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    common::run_past_boot(&mut m, &fw);
    // At rest: composed every frame, nothing sent.
    let idle = measure(&mut m, &fw, 25, |_| {});
    // A chord of saws from its note-ons through the attack, decay and sustain.
    let chord = measure(&mut m, &fw, 75, |m| {
        for k in CHORD {
            m.key(k, true);
        }
    });
    for k in CHORD {
        m.key(k, false);
    }
    m.run_ms(1500);
    // The busiest: three voices of noise held at full sustain (the waveform and sustain CCs
    // over TRS MIDI). Noise moves every scope cell its trace crosses, every frame.
    m.midi(&[0xB0, 20, 64, 0xB0, 79, 127]);
    m.run_ms(100);
    let noise = measure(&mut m, &fw, 75, |m| {
        for k in CHORD {
            m.key(k, true);
        }
    });
    let report = serde_json::json!({
        "frame_ms": UI_PERIOD_MS,
        "spi_hz": SPI_HZ,
        "live_idle": animated_json(&idle),
        "live_saw_chord": animated_json(&chord),
        "live_noise_chord": animated_json(&noise),
    });
    println!("{report}");
    assert_eq!(idle.total().windows, 0, "LIVE at rest sent something");
    for (what, c, per_frame) in [
        ("idle", &idle, LIVE_IDLE_PER_FRAME),
        ("saw chord", &chord, LIVE_CHORD_PER_FRAME),
        ("noise chord", &noise, LIVE_NOISE_PER_FRAME),
    ] {
        assert!(
            c.worst_frame_ms() <= SPI_BUDGET_MS,
            "{what}: a frame took {:.1} ms of SPI",
            c.worst_frame_ms()
        );
        // app/live_view.h LIVE_VIEW_FRAME_CELLS.
        let most = c.frames.iter().map(cells).max().unwrap_or(0);
        assert!(most <= 200, "{what}: a frame sent {most} cells");
        assert!(
            c.worst_poll < LIVE_WORST_POLL,
            "{what}: a ui_poll took {} instructions",
            c.worst_poll
        );
        assert!(
            c.instructions_per_frame() < per_frame,
            "{what}: {:.0} instructions a frame",
            c.instructions_per_frame()
        );
    }
    // The voices take turns at the budget: noise moves on every scope.
    let n = noise.frames.len() as f64;
    let avg = f64::from(noise.frames.iter().map(cells).sum::<u32>()) / n;
    assert!(
        avg > 100.0,
        "noise sent {avg:.0} cells a frame: the scopes stood still"
    );
}

/// Main-loop instructions in ui_poll (fm1-emu) on LIVE, at M1e Task 4 (docs/art/README.md,
/// "LIVE"), each part of the canvas drawn again only when it changes: at rest 13.7k a frame
/// (it composes, sends nothing); a chord of saws 47.5k (5 cells a frame once it holds still);
/// three voices of noise 141.2k (146 cells a frame). The busiest ui_poll, 60.5k, is a frame of
/// noise's compose; the tap is read half way between frames.
const LIVE_WORST_POLL: u64 = 68_000;
const LIVE_IDLE_PER_FRAME: f64 = 15_500.0;
const LIVE_CHORD_PER_FRAME: f64 = 53_000.0;
const LIVE_NOISE_PER_FRAME: f64 = 155_000.0;

/// SCENE's pieces (app/scene.h), in SELECT's order, as their goldens are named.
const SCENE_PIECES: [&str; 5] = ["fire", "plasma", "rule30", "stars", "ant"];
/// app/panel.h K1: the cutoff.
const K1_CUTOFF: u8 = 2;

/// SCENE, from the menu and past THE SCENE's card, on piece `piece` (SELECT from FIRE, the
/// first), each piece's toast gone (1.5 s), and on to half way between two frames.
fn open_scene(m: &mut Machine, fw: &Firmware, piece: usize) {
    open_menu(m, fw);
    select(m, MENU_SCENE);
    m.run_ms(100);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    assert!(showing_card(m, fw), "no card into SCENE");
    past_card(m, fw);
    settle(m, fw);
    assert_eq!(
        common::top_view(m, fw),
        fw.symbol("scene_view_view"),
        "the menu's SCENE did not open SCENE"
    );
    for _ in 0..piece {
        m.turn(SELECT, 1);
        m.run_ms(1600);
    }
    common::to_mid_frame(m, fw);
}

/// Each piece after a chord held for a second (its note-ons, then its envelopes), from the
/// piece's start: a fixed frame with a known audio state.
#[test]
fn scene_matches_its_golden_images() {
    let Some((build, fw)) = fixture("the SCENE test") else {
        return;
    };
    let mut m = booted(&fw);
    open_scene(&mut m, &fw, 0);
    assert!(common::tap_on(&mut m, &fw), "the tap is off under SCENE");
    for (k, name) in SCENE_PIECES.iter().enumerate() {
        if k > 0 {
            m.turn(SELECT, 1);
            m.run_ms(1600);
            common::to_mid_frame(&mut m, &fw);
        }
        for key in CHORD {
            m.key(key, true);
        }
        m.run_ms(1000);
        common::to_mid_frame(&mut m, &fw);
        compare_golden(&build, &format!("scene-{name}"), &m.soc.lcd.fb, &[]);
        for key in CHORD {
            m.key(key, false);
        }
        m.run_ms(1500);
    }
    // HOME goes back to the menu, and the tap goes off.
    press(&mut m, HOME);
    settle(&mut m, &fw);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("views_menu"));
    assert!(!common::tap_on(&mut m, &fw), "the tap runs under the menu");
}

/// Cells each 40 ms of `frames` sent.
fn cells_sent(c: &Cost) -> Vec<u32> {
    c.frames.iter().map(cells).collect()
}

/// A chord's note-ons move every piece at once, and once it has died away the piece calms:
/// what silence sends before and after, against the chord's frames.
#[test]
fn a_note_moves_every_piece_and_silence_calms_it() {
    let Some((_, fw)) = fixture("the SCENE test") else {
        return;
    };
    for (piece, name) in SCENE_PIECES.iter().enumerate() {
        let mut m = booted(&fw);
        open_scene(&mut m, &fw, piece);
        let quiet = cells_sent(&measure(&mut m, &fw, 25, |_| {}));
        let chord = cells_sent(&measure(&mut m, &fw, 25, |m| {
            for key in CHORD {
                m.key(key, true);
            }
        }));
        for key in CHORD {
            m.key(key, false);
        }
        m.run_ms(3000);
        let after = cells_sent(&measure(&mut m, &fw, 25, |_| {}));
        let sum = |v: &[u32]| v.iter().sum::<u32>();
        println!(
            "SCENE {name}: cells in 25 frames: quiet {}, the chord {} (its first frames {:?}), \
             after {}",
            sum(&quiet),
            sum(&chord),
            &chord[..4],
            sum(&after)
        );
        // The note-ons show within two frames (the key scan's debounce, then a frame).
        assert!(
            chord[..3].iter().any(|&n| n > 0),
            "{name}: nothing changed on the note-ons"
        );
        assert!(
            sum(&chord) > 3 * sum(&quiet).max(10) && sum(&chord) > 3 * sum(&after).max(10),
            "{name}: quiet {}, the chord {}, after {}",
            sum(&quiet),
            sum(&chord),
            sum(&after)
        );
    }
}

/// A band on the keys, frame by frame: an arpeggio note every 4 frames, let go after 3, and a
/// bass note every 8, let go after 6.
fn band(m: &mut Machine, f: u64) {
    const ARP: [u8; 6] = [57, 60, 64, 69, 72, 76];
    const BASS: [u8; 2] = [53, 55];
    let key = |note: u8| 14 + note - 53;
    let arp = key(ARP[(f / 4 % 6) as usize]);
    let bass = key(BASS[(f / 8 % 2) as usize]);
    match f % 4 {
        0 => m.key(arp, true),
        3 => m.key(arp, false),
        _ => {}
    }
    match f % 8 {
        0 => m.key(bass, true),
        6 => m.key(bass, false),
        _ => {}
    }
}

/// `frames` frames of the band, as `measure` counts them.
fn measure_band(m: &mut Machine, fw: &Firmware, frames: u64) -> Cost {
    measure_frames(m, fw, frames, band)
}

/// `frames` frames, as `measure` counts them, `act(m, f)` at the start of frame `f`.
fn measure_frames(
    m: &mut Machine,
    fw: &Firmware,
    frames: u64,
    mut act: impl FnMut(&mut Machine, u64),
) -> Cost {
    common::to_mid_frame(m, fw);
    let from = m.watched_calls.len();
    let mut per = Vec::new();
    for f in 0..frames {
        act(m, f);
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

/// Each piece with the band playing and the filter wide open (the busiest plasma, the hottest
/// fire), and the wipe from it to the next piece with a chord held: every frame inside the
/// SPI budget and SCENE's cells (app/scene_view.h SCENE_VIEW_FRAME_CELLS), and the main loop's
/// cost a frame and its busiest ui_poll within the bounds below.
#[test]
fn scene_stays_within_its_budgets() {
    let Some((_, fw)) = fixture("the SCENE budget test") else {
        return;
    };
    let mut report = serde_json::Map::new();
    for (piece, name) in SCENE_PIECES.iter().enumerate() {
        let mut m = Machine::boot(&fw.image);
        m.watch_calls(fw.symbol("ui_poll"));
        common::run_past_boot(&mut m, &fw);
        open_scene(&mut m, &fw, piece);
        m.turn(K1_CUTOFF, 64);
        m.run_ms(1000);
        let c = measure_band(&mut m, &fw, 150);
        for k in 0..30 {
            band(&mut m, k); // into the wipe with notes playing
        }
        for key in CHORD {
            m.key(key, true);
        }
        m.run_ms(200);
        let from = m.watched_calls.len();
        let wipe = wipe_windows(&mut m, &fw, |m| m.turn(SELECT, 1));
        let (wipe_poll, _) = wipe_instructions(&m.watched_calls[from..], wipe.len());
        assert_wipe_holds_still(&format!("the wipe from {name}"), &wipe);
        let per = cells_sent(&c);
        let avg = f64::from(per.iter().sum::<u32>()) / per.len() as f64;
        let most = per.iter().copied().max().unwrap_or(0);
        report.insert(name.to_string(), animated_json(&c));
        println!(
            "SCENE {name}: {avg:.1} cells a frame, at most {most}; SPI {:.2} ms a frame, worst \
             {:.2} ms; {:.0} instructions a frame, busiest ui_poll {}; the wipe to the next \
             piece: busiest ui_poll {wipe_poll}",
            c.total().spi_ms() / per.len() as f64,
            c.worst_frame_ms(),
            c.instructions_per_frame(),
            c.worst_poll
        );
        let (cells_bound, most_bound, per_frame_bound) = SCENE_BOUNDS[piece];
        assert!(
            c.worst_frame_ms() <= SPI_BUDGET_MS,
            "{name}: a frame took {:.1} ms of SPI",
            c.worst_frame_ms()
        );
        assert!(
            most <= sidou_sys::SCENE_VIEW_FRAME_CELLS,
            "{name}: a frame sent {most} cells"
        );
        assert!(avg <= cells_bound, "{name}: {avg:.1} cells a frame");
        assert!(most <= most_bound, "{name}: {most} cells in a frame");
        assert!(
            c.worst_poll < SCENE_WORST_POLL && wipe_poll < SCENE_WORST_POLL,
            "{name}: a ui_poll took {} instructions ({wipe_poll} in the wipe)",
            c.worst_poll
        );
        assert!(
            c.instructions_per_frame() < per_frame_bound,
            "{name}: {:.0} instructions a frame",
            c.instructions_per_frame()
        );
    }
    println!("{}", serde_json::Value::Object(report));
}

/// MIDI learn's frame (SAVE) over each SCENE piece and over LIVE, the band playing and the
/// filter wide open, until HOME closes it: the frame comes in at once over a view that composes
/// every frame within its budget, so the view leaves it the cells it covers (app/screen.h
/// screen_cover) and its overlays stay (SCREEN_OVER), and no frame, the frame's first and the
/// one after it closes included, sends more than SCENE_VIEW_FRAME_CELLS (LIVE_VIEW_FRAME_CELLS)
/// cells or the SPI budget.
#[test]
fn midi_learn_over_scene_and_live_stays_within_the_budget() {
    let Some((_, fw)) = fixture("the learn budget test") else {
        return;
    };
    let mut over = Vec::new();
    let shown = SCENE_PIECES.iter().map(|&n| Some(n)).chain([None]);
    for (piece, name) in shown.enumerate() {
        let mut m = booted(&fw);
        if name.is_some() {
            open_scene(&mut m, &fw, piece);
        }
        let name = name.unwrap_or("LIVE");
        m.turn(K1_CUTOFF, 64);
        m.run_ms(1000);
        let mut opened = 0;
        let c = measure_frames(&mut m, &fw, 120, |m, f| {
            band(m, f);
            match f {
                10 => m.key(SAVE, true),
                13 => m.key(SAVE, false),
                90 => {
                    opened = common::top_view(m, &fw);
                    m.key(HOME, true);
                }
                93 => m.key(HOME, false),
                _ => {}
            }
        });
        assert_eq!(
            opened,
            fw.symbol("learn_view_view"),
            "{name}: SAVE did not open learn"
        );
        assert_ne!(
            common::top_view(&mut m, &fw),
            fw.symbol("learn_view_view"),
            "{name}: HOME did not close learn"
        );
        let per = cells_sent(&c);
        let most = per.iter().copied().max().unwrap_or(0);
        println!(
            "learn over {name}: at most {most} cells, worst {:.2} ms of SPI; the frames before \
             SAVE {:?}, from it {:?}, from HOME {:?}",
            c.worst_frame_ms(),
            &per[4..10],
            &per[10..18],
            &per[90..96]
        );
        if most > sidou_sys::SCENE_VIEW_FRAME_CELLS || most > sidou_sys::LIVE_VIEW_FRAME_CELLS {
            over.push(format!("learn over {name}: a frame sent {most} cells"));
        }
        if c.worst_frame_ms() > SPI_BUDGET_MS {
            over.push(format!(
                "learn over {name}: a frame took {:.2} ms of SPI",
                c.worst_frame_ms()
            ));
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}

/// SCENE with the band playing and the filter wide open, 150 frames a piece (fm1-emu, M1e Task
/// 5; docs/art/README.md, "SCENE"), bounded about 10 % above what was measured (never looser
/// than at first): each piece's cells a frame on average and at most, and its main-loop
/// instructions a frame (ui_poll), and the busiest ui_poll of any piece (FIRE's, 58.9k: its two
/// burns, half way between frames). Measured after the review (FIRE's new look, the covered
/// cells left out of the copy, M1b Task 6's tap):
///
/// | Piece | Cells a frame (average, most) | SPI a frame (average, worst) | Instructions a frame | Busiest ui_poll |
/// |---|---|---|---|---|
/// | FIRE | 125.0, 159 | 10.93 ms, 13.85 ms | 101.6k | 58.9k |
/// | PLASMA | 138.4, 200 | 12.29 ms, 17.64 ms | 141.4k | 53.0k |
/// | RULE 30 | 18.9, 76 | 1.65 ms, 6.60 ms | 32.7k | 21.3k |
/// | STARS | 39.5, 68 | 3.59 ms, 6.16 ms | 74.1k | 21.1k |
/// | ANT | 2.7, 9 | 0.25 ms, 0.80 ms | 29.3k | 18.9k |
const SCENE_BOUNDS: [(f64, u32, f64); 5] = [
    (138.0, 175, 108_000.0), // fire
    (152.0, 200, 152_000.0), // plasma: the most is SCENE_VIEW_FRAME_CELLS
    (21.0, 84, 33_500.0),    // rule 30
    (44.0, 75, 79_000.0),    // stars
    (3.0, 10, 30_000.0),     // ant
];
const SCENE_WORST_POLL: u64 = 62_000;
/// A piece starts in the compose of the first frame of the wipe that brings it in (SELECT, or
/// the menu's ALGORITHM on SCENE), inside ui_poll, not in the input's call (ui_encoder). The
/// input and a frame can fall in one main-loop pass, between two drains of MIDI: the busiest
/// input (SCENE_INPUT) and the busiest pass after it (SCENE_START_PASS, from midi_drain to
/// midi_service: the drain, play() and ui_poll's frame with the start) stay within the latency
/// gate's poll allowance together. Measured (fm1-emu, M1e Task 5), the busiest of every piece,
/// bounded about 10 % above:
///
/// | | Input (ui_encoder) | Busiest pass after it |
/// |---|---|---|
/// | SELECT | 203 (to RULE 30) | 60.7k (to FIRE: a frame of its burns) |
/// | The menu's ALGORITHM | 170 | 60.7k (into FIRE) |
///
/// With the title cards (on by default) the menu's ALGORITHM pushes THE SCENE's card over
/// SCENE as well: 350, within TITLE_CARD_INPUT; the piece starts in the cut after the card.
///
/// The start of the costliest piece, PLASMA, was 48.8k in the input's call when it drew its
/// whole canvas there; the busiest pass after SELECT to PLASMA is now 32.7k.
const SCENE_INPUT: u64 = 230;
const SCENE_START_PASS: u64 = 67_000;

/// What `act` costs over the next `ms`: the busiest ui_encoder call and the busiest main-loop
/// pass (`Machine::passes`).
fn start_cost(m: &mut Machine, ms: u64, act: impl FnOnce(&mut Machine)) -> (u64, u64) {
    let (calls, passes) = (m.watched_calls.len(), m.passes.len());
    act(m);
    m.run_ms(ms);
    let input = m.watched_calls[calls..].iter().copied().max().unwrap_or(0);
    let pass = m.passes[passes..].iter().copied().max().unwrap_or(0);
    (input, pass)
}

/// Stepping round the pieces with SELECT, and into each piece from the menu's ALGORITHM (THE
/// SCENE's card, then the piece's start in the cut after it, SCREEN_BANDS): the input's call
/// within SCENE_INPUT, every pass after it within SCENE_START_PASS.
#[test]
fn starting_a_piece_holds_the_main_loop_within_the_poll_allowance() {
    let Some((_, fw)) = fixture("the SCENE input test") else {
        return;
    };
    let mut m = booted(&fw);
    open_scene(&mut m, &fw, 0);
    m.watch_calls(fw.symbol("ui_encoder"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut costs = Vec::new();
    let mut over_input = Vec::new();
    for name in SCENE_PIECES.iter().cycle().skip(1).take(SCENE_PIECES.len()) {
        let c = start_cost(&mut m, 1600, |m| m.turn(SELECT, 1));
        costs.push((format!("SELECT to {name}"), c));
    }
    for (piece, name) in SCENE_PIECES.iter().enumerate() {
        if piece > 0 {
            m.turn(SELECT, 1);
            m.run_ms(1600);
        }
        press(&mut m, HOME);
        settle(&mut m, &fw);
        assert_eq!(common::top_view(&mut m, &fw), fw.symbol("views_menu"));
        // Behind THE SCENE's card, the piece starts in the cut after it (app/title_card.h):
        // the input is the turn, the passes from the card's hold (its own passes are
        // title_cards_stay_within_their_budgets').
        let calls = m.watched_calls.len();
        m.turn(ALGORITHM, 1);
        m.run_ms(100);
        let input = m.watched_calls[calls..].iter().copied().max().unwrap_or(0);
        to_held(&mut m, &fw);
        let (_, pass) = start_cost(&mut m, CARD_TIMEOUT_MS, |_| {});
        let c = (input, pass);
        assert_eq!(
            common::top_view(&mut m, &fw),
            fw.symbol("scene_view_view"),
            "{name}: the menu's SCENE did not open SCENE"
        );
        costs.push((format!("the menu into {name}"), c));
        // The menu's ALGORITHM on SCENE pushes the card over it too: its own bound.
        if input >= TITLE_CARD_INPUT {
            over_input.push(format!("the menu into {name}: the input {input}"));
        }
    }
    let mut over = over_input;
    for (what, (input, pass)) in &costs {
        println!(
            "{what}: the input {input}, the busiest pass {pass}, together {} instructions",
            input + pass
        );
        let input_bound = if what.starts_with("SELECT") {
            SCENE_INPUT
        } else {
            TITLE_CARD_INPUT
        };
        if *input >= input_bound || *pass >= SCENE_START_PASS {
            over.push(format!(
                "{what}: the input {input}, the busiest pass {pass}"
            ));
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}

// ---- the CHIP view (app/chip_view.h) ----

/// app/panel.h K1, the CHIP view's first knob (the age on the chip, Amount on a slot).
const K1_CHIP: u8 = 2;

/// The character and every slot over TRS MIDI (docs/reference/midi.md): age 100, DAC 40,
/// thump 64, bleed 20; DIST OVERDRIVE, MOD CHORUS, FX REVERB (the algorithm CCs divide 0..127
/// among the slot's algorithms), each slot's Amount, A, B and Mix.
const CHIP_STATE: [u8; 19 * 3] = {
    const CCS: [(u8, u8); 19] = [
        (86, 100),
        (87, 40),
        (89, 64),
        (90, 20),
        (102, 50),
        (103, 90),
        (104, 30),
        (105, 70),
        (106, 127),
        (107, 20),
        (108, 60),
        (109, 0),
        (110, 10),
        (111, 80),
        (112, 80),
        (113, 100),
        (114, 50),
        (115, 64),
        (116, 40),
    ];
    let mut out = [0u8; 19 * 3];
    let mut i = 0;
    while i < CCS.len() {
        out[3 * i] = 0xB0;
        out[3 * i + 1] = CCS[i].0;
        out[3 * i + 2] = CCS[i].1;
        i += 1;
    }
    out
};

/// CHIP from the menu, past THE CHIP's card, the slot's toast gone, settled.
fn open_chip(m: &mut Machine, fw: &Firmware) {
    open_menu(m, fw);
    select(m, MENU_CHIP);
    m.run_ms(100);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    assert!(showing_card(m, fw), "no card into CHIP");
    past_card(m, fw);
    settle(m, fw);
    assert_eq!(
        common::top_view(m, fw),
        fw.symbol("chip_view_view"),
        "the menu's CHIP did not open CHIP"
    );
}

/// The CHIP view's focus (app/chip_view.h chip_view_focus): 0 the chip, 1 + the selected slot.
fn chip_focus(m: &mut Machine, fw: &Firmware) -> u8 {
    let on_chip = ui_byte(
        m,
        fw,
        sidou_sys::ilp32::views::chip + sidou_sys::ilp32::chip_view::chip,
    );
    let l = m
        .soc
        .read32(fw.symbol("ui") + sidou_sys::ilp32::views::l as u32);
    let slot = m.soc.read8(l + sidou_sys::ilp32::live::fx_select as u32);
    if on_chip != 0 {
        0
    } else {
        1 + slot
    }
}

/// The instrument's character controls (app/live.h live.character: age, DAC, thump, bleed).
fn character(m: &mut Machine, fw: &Firmware) -> [u8; 4] {
    let l = m
        .soc
        .read32(fw.symbol("ui") + sidou_sys::ilp32::views::l as u32);
    std::array::from_fn(|i| {
        m.soc
            .read8(l + (sidou_sys::ilp32::live::character + i) as u32)
    })
}

/// Each CHIP page, its focus moved down by SELECT: the chip (the profile and the character),
/// DIST, MOD and FX, with the character and the slots set over MIDI.
#[test]
fn chip_matches_its_golden_images() {
    let Some((build, fw)) = fixture("the CHIP test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&CHIP_STATE);
    m.run_ms(200);
    open_chip(&mut m, &fw);
    assert_eq!(chip_focus(&mut m, &fw), 0);
    compare_golden(&build, "chip", &m.soc.lcd.fb, &[]);
    for (k, name) in ["dist", "mod", "fx"].iter().enumerate() {
        m.turn(SELECT, 1);
        m.run_ms(1600); // the slot's toast
        settle(&mut m, &fw);
        assert_eq!(chip_focus(&mut m, &fw), k as u8 + 1);
        compare_golden(&build, &format!("chip-{name}"), &m.soc.lcd.fb, &[]);
    }
}

/// The page follows MIDI and the knobs, there and back: a CC takes the age to 0 and the page
/// shows it; K1 turned 25 detents on the chip takes it back to 100 (4 a detent), in the
/// instrument and on the page, which is its golden again; and SELECT past the end stays.
#[test]
fn chip_follows_midi_and_the_knobs_there_and_back() {
    let Some((build, fw)) = fixture("the CHIP test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&CHIP_STATE);
    m.run_ms(200);
    open_chip(&mut m, &fw);
    let golden = indexed(&m.soc.lcd.fb);
    m.midi(&[0xB0, 86, 0]);
    m.run_ms(200);
    common::to_mid_frame(&mut m, &fw);
    assert_eq!(character(&mut m, &fw), [0, 40, 64, 20]);
    let row = |idx: &[u8], r: usize| idx[r * 8 * W..(r + 1) * 8 * W].to_vec();
    let age_row = sidou_sys::CHIP_VIEW_CHARACTER_ROW as usize + 1;
    let now = indexed(&m.soc.lcd.fb);
    assert!(
        row(&now, age_row) != row(&golden, age_row),
        "the age's row did not change"
    );
    for r in (0..30).filter(|&r| r != age_row) {
        assert!(row(&now, r) == row(&golden, r), "row {r} changed");
    }
    m.turn(K1_CHIP, 25);
    m.run_ms(25 * 10 + 200);
    common::to_mid_frame(&mut m, &fw);
    assert_eq!(character(&mut m, &fw), [100, 40, 64, 20]);
    compare_golden(&build, "chip", &m.soc.lcd.fb, &[]);
    m.turn(SELECT, -3);
    m.run_ms(100);
    assert_eq!(chip_focus(&mut m, &fw), 0, "SELECT went past the chip");
}

/// Main-loop instructions in ui_poll (fm1-emu) on the CHIP view, bounded about 10 % above what
/// was measured (docs/art/README.md, "CHIP"):
///
/// | CHIP | Cells a frame (average, most) | Worst SPI | Instructions a frame | Busiest ui_poll |
/// |---|---|---|---|---|
/// | Idle | 0, 0 | 0 | 6.3k | 0.3k |
/// | A knob's detent (25 frames) | 0.08, 2 | 0.19 ms | 7.7k | 34.2k |
/// | The busiest (50 frames) | 26.9, 200 | 17.62 ms | 37.4k | 28.6k |
///
/// Idle it composes nothing. A detent builds the page in its canvas and copies the two cells
/// that changed (the busiest ui_poll: the build and the scan of 900 cells). The busiest is a
/// burst of every control over MIDI with a focus move and a chip change (its raster and toast):
/// its first frame fills the budget and the rest goes out in the frames after.
const CHIP_IDLE_PER_FRAME: f64 = 7_000.0;
const CHIP_KNOB_PER_FRAME: f64 = 8_500.0;
const CHIP_BUSY_PER_FRAME: f64 = 41_000.0;
const CHIP_WORST_POLL: u64 = 38_000;
/// SELECT, ALGORITHM and a knob on the CHIP view: the input's call (ui_encoder) and the busiest
/// main-loop pass after it (from midi_drain to midi_service), within the latency gate's poll
/// allowance together. Measured: the input 198 at most (K1 on DIST), the busiest pass 44.5k
/// (SELECT from FX back to the chip: three frames change colour).
const CHIP_INPUT: u64 = 220;
const CHIP_PASS: u64 = 49_000;
/// ALGORITHM on the chip steps the profile: its pass is the chip reset (the live tick prepares
/// the spare chip, decision record 0006, "Cost"), as the FX button's and CC 85's are. A reset
/// is the one input over the poll allowance, by design: it is a deliberate step the player
/// takes, at most one a 10 ms tick (requests in between coalesce), MIDI waits in its buffer
/// rather than being dropped, and the FX button and CC 85 have always cost the same. A note
/// arriving during the pass waits for it (about 2 ms) and the drain after it: about 5 ms,
/// against decision record 0005's 3.4 ms, only while the profile or model steps.
/// docs/TODO.md: split the spare chip's preparation over ticks (M2).
///
/// CHIP_PROFILE_PASS: R4AR to the 8580 and back at the page's character (CHIP_STATE), measured
/// 362.6k and 313.5k. CHIP_RESET_PASS: every reset at full character (age, DAC, thump and
/// bleed 127), measured onto WORN 471.5k by ALGORITHM, 471.4k by CC 85, 470.7k by FX (R2 and
/// R3 471.2k, R4AR 313.5k, the 8580 362.6k): the worst case. Both about 10 % above.
const CHIP_PROFILE_PASS: u64 = 400_000;
const CHIP_RESET_PASS: u64 = 520_000;

/// The CHIP view idle, a knob's detent, and the busiest frames: every frame within
/// CHIP_VIEW_FRAME_CELLS and the SPI budget, the main loop's cost a frame and its busiest
/// ui_poll within the bounds above.
#[test]
fn chip_stays_within_its_budgets() {
    let Some((_, fw)) = fixture("the CHIP budget test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    common::run_past_boot(&mut m, &fw);
    m.midi(&CHIP_STATE);
    m.run_ms(200);
    open_chip(&mut m, &fw);
    let idle = measure(&mut m, &fw, 25, |_| {});
    let knob = measure(&mut m, &fw, 25, |m| m.turn(K1_CHIP, 1));
    // Every control changed at once, the focus moved to DIST, and the chip changed: the toast,
    // the raster, every bar and value, three frames' colours.
    let mut burst = CHIP_STATE;
    for v in burst.iter_mut().skip(2).step_by(3) {
        *v = 127 - *v;
    }
    let busy = measure(&mut m, &fw, 50, |m| {
        m.midi(&burst);
        m.turn(SELECT, 1);
        m.key(FX, true);
    });
    m.key(FX, false);
    let report = serde_json::json!({
        "frame_ms": UI_PERIOD_MS,
        "spi_hz": SPI_HZ,
        "chip_idle": animated_json(&idle),
        "chip_knob": animated_json(&knob),
        "chip_busiest": animated_json(&busy),
    });
    println!("{report}");
    assert_eq!(idle.total().windows, 0, "the idle CHIP view sent something");
    for (what, c, per_frame) in [
        ("idle", &idle, CHIP_IDLE_PER_FRAME),
        ("a knob", &knob, CHIP_KNOB_PER_FRAME),
        ("the busiest", &busy, CHIP_BUSY_PER_FRAME),
    ] {
        assert!(
            c.worst_frame_ms() <= SPI_BUDGET_MS,
            "{what}: a frame took {:.1} ms of SPI",
            c.worst_frame_ms()
        );
        let most = c.frames.iter().map(cells).max().unwrap_or(0);
        assert!(
            most <= sidou_sys::CHIP_VIEW_FRAME_CELLS,
            "{what}: a frame sent {most} cells"
        );
        assert!(
            c.worst_poll < CHIP_WORST_POLL,
            "{what}: a ui_poll took {} instructions",
            c.worst_poll
        );
        assert!(
            c.instructions_per_frame() < per_frame,
            "{what}: {:.0} instructions a frame",
            c.instructions_per_frame()
        );
    }
    // A detent changes its value's cells and its bar's, a run or two.
    let knob_cells: u32 = knob.frames.iter().map(cells).sum();
    assert!(
        (1..=12).contains(&knob_cells),
        "a detent sent {knob_cells} cells"
    );
    // The burst changes more than a frame's budget: it spreads over frames.
    let full = busy
        .frames
        .iter()
        .filter(|f| cells(f) >= sidou_sys::CHIP_VIEW_FRAME_CELLS - 30)
        .count();
    assert!(full >= 1, "the burst never filled a frame");
}

/// SELECT, ALGORITHM on a slot and a knob on the CHIP view: the input within CHIP_INPUT, every
/// pass after it within CHIP_PASS; ALGORITHM on the chip, the profile, its input within
/// CHIP_INPUT and its pass, the chip reset's, within CHIP_PROFILE_PASS.
#[test]
fn chip_input_holds_the_main_loop_within_the_poll_allowance() {
    let Some((_, fw)) = fixture("the CHIP input test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&CHIP_STATE);
    m.run_ms(200);
    open_chip(&mut m, &fw);
    m.watch_calls(fw.symbol("ui_encoder"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut over = Vec::new();
    for (what, enc, steps) in [
        ("SELECT down", SELECT, 1),
        ("ALGORITHM on DIST", ALGORITHM, 1),
        ("K1 on DIST", K1_CHIP, 1),
        ("SELECT down", SELECT, 2),
        ("ALGORITHM on FX", ALGORITHM, -1),
        ("SELECT to the chip", SELECT, -3),
        ("K1 on the chip", K1_CHIP, -1),
    ] {
        let (input, pass) = start_cost(&mut m, 1600, |m| m.turn(enc, steps));
        println!("CHIP {what}: the input {input}, the busiest pass {pass}");
        if input >= CHIP_INPUT || pass >= CHIP_PASS {
            over.push(format!(
                "{what}: the input {input}, the busiest pass {pass}"
            ));
        }
    }
    // The profile: the input as the others', the pass the chip reset's (CHIP_PROFILE_PASS).
    for (what, steps) in [("ALGORITHM on the chip", 1), ("ALGORITHM back", -1)] {
        let (input, pass) = start_cost(&mut m, 1600, |m| m.turn(ALGORITHM, steps));
        println!("CHIP {what}: the input {input}, the busiest pass {pass}");
        if input >= CHIP_INPUT || pass >= CHIP_PROFILE_PASS {
            over.push(format!(
                "{what}: the input {input}, the busiest pass {pass}"
            ));
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}

/// The profile live has (live.profile, a SID_CHIP_*).
fn profile(m: &mut Machine, fw: &Firmware) -> u8 {
    let l = m
        .soc
        .read32(fw.symbol("ui") + sidou_sys::ilp32::views::l as u32);
    m.soc.read8(l + sidou_sys::ilp32::live::profile as u32)
}

/// Every way to reset the chip, at full character (age, DAC, thump and bleed at 127, settled):
/// ALGORITHM on CHIP onto every profile, the FX button both ways, and CC 85 onto every profile;
/// the busiest main-loop pass after each within CHIP_RESET_PASS.
#[test]
fn chip_resets_at_full_character_stay_within_their_bound() {
    use sidou_sys::{SID_CHIP_6581_R2, SID_CHIP_6581_R4AR, SID_CHIP_6581_WORN, SID_CHIP_8580_R5};
    let Some((_, fw)) = fixture("the chip reset test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&[0xB0, 86, 127, 0xB0, 87, 127, 0xB0, 89, 127, 0xB0, 90, 127]);
    m.run_ms(2500); // the DACs' ramp, the longest, takes 1.72 s
    open_chip(&mut m, &fw);
    assert_eq!(character(&mut m, &fw), [127; 4]);
    assert_eq!(u32::from(profile(&mut m, &fw)), SID_CHIP_6581_R4AR);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut costs: Vec<(String, u64)> = Vec::new();
    let mut run = |m: &mut Machine, what: String, want: u32, act: &dyn Fn(&mut Machine)| {
        let (_, pass) = start_cost(m, 500, act);
        assert_eq!(u32::from(profile(m, &fw)), want, "{what}");
        println!("chip reset, {what}: the busiest pass {pass}");
        costs.push((what, pass));
    };
    let r3 = SID_CHIP_6581_R4AR - 1;
    // ALGORITHM: R4AR up to WORN, then down to R2.
    for (steps, want) in [
        (1, SID_CHIP_8580_R5),
        (1, SID_CHIP_6581_WORN),
        (-1, SID_CHIP_8580_R5),
        (-1, SID_CHIP_6581_R4AR),
        (-1, r3),
        (-1, SID_CHIP_6581_R2),
    ] {
        run(
            &mut m,
            format!("ALGORITHM {steps:+} to profile {want}"),
            want,
            &|m| m.turn(ALGORITHM, steps),
        );
    }
    // CC 85 (0..4 over 0..127): onto each profile, ending on WORN.
    for want in [
        r3,
        SID_CHIP_6581_R4AR,
        SID_CHIP_8580_R5,
        SID_CHIP_6581_R2,
        SID_CHIP_6581_WORN,
    ] {
        let v = (want * 127).div_ceil(4) as u8;
        run(&mut m, format!("CC 85 {v} to profile {want}"), want, &|m| {
            m.midi(&[0xB0, 85, v])
        });
    }
    // FX: from WORN to the 8580, and back to the last 6581 profile, WORN.
    for want in [SID_CHIP_8580_R5, SID_CHIP_6581_WORN] {
        run(&mut m, format!("FX to profile {want}"), want, &|m| {
            press(m, FX)
        });
    }
    let worst = costs.iter().max_by_key(|c| c.1).unwrap();
    println!("chip reset, the worst: {} ({})", worst.1, worst.0);
    let over: Vec<_> = costs.iter().filter(|c| c.1 >= CHIP_RESET_PASS).collect();
    assert!(over.is_empty(), "{over:#?}");
}

// ---- the title cards (app/title_card.h) ----

/// The menu's ALGORITHM on SCENE, from LIVE: THE SCENE's card.
fn into_scene_card(m: &mut Machine, fw: &Firmware) {
    open_menu(m, fw);
    select(m, MENU_SCENE);
    m.run_ms(100);
    common::to_mid_frame(m, fw);
    m.turn(ALGORITHM, 1);
}

/// The menu's ALGORITHM on CHIP, from LIVE: THE CHIP's card.
fn into_chip_card(m: &mut Machine, fw: &Firmware) {
    open_menu(m, fw);
    select(m, MENU_CHIP);
    m.run_ms(100);
    common::to_mid_frame(m, fw);
    m.turn(ALGORITHM, 1);
}

/// HOME from the menu, from LIVE: THE OSCILLATOR's card.
fn into_oscillator_card(m: &mut Machine, fw: &Firmware) {
    open_menu(m, fw);
    common::to_mid_frame(m, fw);
    press(m, HOME);
}

/// A frame at a time until the card holds (TITLE_CARD_HOLD), then a frame more, and on to half
/// way between two frames: the held frame whole on the panel.
fn to_held(m: &mut Machine, fw: &Firmware) {
    let from = m.ms();
    while card_byte(m, fw, sidou_sys::ilp32::title_card::phase) != sidou_sys::TITLE_CARD_HOLD as u8
    {
        assert!(showing_card(m, fw), "the card ended before it held");
        assert!(
            m.ms() < from + CARD_TIMEOUT_MS,
            "the card did not reach its hold"
        );
        m.run_ms(UI_PERIOD_MS);
    }
    m.run_ms(UI_PERIOD_MS);
    common::to_mid_frame(m, fw);
    assert!(showing_card(m, fw));
}

/// The panel against a card's source, assets/logo/title-<name>.png, pixel for pixel.
fn assert_shows_the_card_source(name: &str, fb: &[u16]) {
    let src = sidou_tests::logo::load(&format!("title-{name}"));
    let got = indexed(fb);
    if let Some(i) = (0..W * W).find(|&i| got[i] != src.px[i]) {
        panic!(
            "THE {}: pixel ({}, {}) is colour {} where title-{name}.png has {}",
            name.to_uppercase(),
            i % W,
            i / W,
            got[i],
            src.px[i]
        );
    }
}

/// THE CHIP into CHIP and THE SCENE into SCENE from the menu, THE OSCILLATOR back to LIVE from
/// the menu: each held frame against its golden and against its source PNG, and the view under
/// it after.
#[test]
fn title_cards_match_their_goldens_and_their_sources() {
    let Some((build, fw)) = fixture("the title card test") else {
        return;
    };
    let mut m = booted(&fw);
    into_chip_card(&mut m, &fw);
    m.run_ms(UI_PERIOD_MS);
    assert!(showing_card(&mut m, &fw), "no card into CHIP");
    assert_eq!(
        card_byte(&mut m, &fw, sidou_sys::ilp32::title_card::id),
        sidou_sys::TITLE_CARD_CHIP as u8
    );
    to_held(&mut m, &fw);
    compare_golden(&build, "title-chip", &m.soc.lcd.fb, &[]);
    assert_shows_the_card_source("chip", &m.soc.lcd.fb);
    past_card(&mut m, &fw);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("chip_view_view"));
    press(&mut m, HOME);
    settle(&mut m, &fw);
    press(&mut m, HOME);
    past_card(&mut m, &fw);
    settle(&mut m, &fw);
    into_scene_card(&mut m, &fw);
    m.run_ms(UI_PERIOD_MS);
    assert!(showing_card(&mut m, &fw), "no card into SCENE");
    assert_eq!(
        card_byte(&mut m, &fw, sidou_sys::ilp32::title_card::id),
        sidou_sys::TITLE_CARD_SCENE as u8
    );
    to_held(&mut m, &fw);
    compare_golden(&build, "title-scene", &m.soc.lcd.fb, &[]);
    assert_shows_the_card_source("scene", &m.soc.lcd.fb);
    past_card(&mut m, &fw);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("scene_view_view"));
    // SCENE back to the menu: no card; the menu back to LIVE: THE OSCILLATOR.
    press(&mut m, HOME);
    settle(&mut m, &fw);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("views_menu"));
    press(&mut m, HOME);
    m.run_ms(UI_PERIOD_MS);
    assert!(showing_card(&mut m, &fw), "no card back to LIVE");
    assert_eq!(
        card_byte(&mut m, &fw, sidou_sys::ilp32::title_card::id),
        sidou_sys::TITLE_CARD_OSCILLATOR as u8
    );
    to_held(&mut m, &fw);
    compare_golden(&build, "title-oscillator", &m.soc.lcd.fb, &[]);
    assert_shows_the_card_source("oscillator", &m.soc.lcd.fb);
    past_card(&mut m, &fw);
    settle(&mut m, &fw);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("live_view_view"));
    compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
}

/// Any button or knob on a card ends it at the next frame and does nothing else: FX would
/// toggle the chip, OCT+ step the octave, SELECT pick a piece; the view under it cuts in.
#[test]
fn a_control_skips_a_title_card_and_does_nothing_else() {
    let Some((_, fw)) = fixture("the title card test") else {
        return;
    };
    for what in ["FX", "OCT+", "SELECT", "HOME"] {
        let mut m = booted(&fw);
        into_scene_card(&mut m, &fw);
        m.run_ms(300);
        assert!(showing_card(&mut m, &fw), "{what}: no card");
        let which = sidou_sys::ilp32::views::scene + sidou_sys::ilp32::scene_view::which;
        let before = (octave_and_model(&mut m, &fw), ui_byte(&mut m, &fw, which));
        match what {
            "FX" => press(&mut m, FX),
            "OCT+" => press(&mut m, OCT_UP),
            "SELECT" => {
                m.turn(SELECT, 1);
                m.run_ms(60);
            }
            _ => press(&mut m, HOME),
        }
        assert!(!showing_card(&mut m, &fw), "{what} did not skip the card");
        settle(&mut m, &fw);
        assert_eq!(
            common::top_view(&mut m, &fw),
            fw.symbol("scene_view_view"),
            "{what}: not SCENE after the card"
        );
        assert_eq!(
            (octave_and_model(&mut m, &fw), ui_byte(&mut m, &fw, which)),
            before,
            "{what} did something"
        );
    }
}

/// A note on a card plays at once, from the keys or MIDI, and ends the card.
#[test]
fn a_note_plays_through_a_title_card_and_ends_it() {
    let Some((_, fw)) = fixture("the title card test") else {
        return;
    };
    for midi in [false, true] {
        let mut m = booted(&fw);
        into_oscillator_card(&mut m, &fw);
        m.run_ms(200);
        assert!(showing_card(&mut m, &fw), "no card");
        let from = m.soc.periph.audio.len() / 2;
        if midi {
            m.midi(&[0x90, 69, 100]);
        } else {
            m.key(A4_KEY, true);
        }
        m.run_ms(60);
        let peak = m.left()[from..]
            .iter()
            .map(|s| s.unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(
            peak > 1000,
            "midi {midi}: the note did not play: peak {peak}"
        );
        assert!(
            !showing_card(&mut m, &fw),
            "midi {midi}: the note did not end the card"
        );
        settle(&mut m, &fw);
        assert_eq!(common::top_view(&mut m, &fw), fw.symbol("live_view_view"));
    }
}

/// Each card from its start to the end of the cut under it, nothing played: every frame within
/// TITLE_CARD_FRAME_CELLS (and the cut's 180) and the SPI budget, the main loop's cost a frame
/// and its busiest ui_poll within the bounds below; and the input that brings it in (ui_encoder
/// for CHIP and SCENE, ui_buttons for HOME) with the busiest main-loop pass of the card within
/// the poll allowance together.
#[test]
fn title_cards_stay_within_their_budgets() {
    let Some((_, fw)) = fixture("the title card budget test") else {
        return;
    };
    let mut report = serde_json::Map::new();
    for (name, input) in [
        ("chip", "ui_encoder"),
        ("scene", "ui_encoder"),
        ("oscillator", "ui_buttons"),
    ] {
        let enter = |m: &mut Machine| match name {
            "chip" => into_chip_card(m, &fw),
            "scene" => into_scene_card(m, &fw),
            _ => into_oscillator_card(m, &fw),
        };
        // The frames and ui_poll, with the passes.
        let mut m = booted(&fw);
        m.watch_calls(fw.symbol("ui_poll"));
        m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
        enter(&mut m);
        let from = (m.watched_calls.len(), m.passes.len());
        let mut frames = Vec::new();
        let t0 = m.ms();
        loop {
            let before = sent(&mut m, &fw);
            m.run_ms(UI_PERIOD_MS);
            let after = sent(&mut m, &fw);
            frames.push(Sent {
                windows: after.windows - before.windows,
                bytes: after.bytes - before.bytes,
            });
            if !showing_card(&mut m, &fw) && !common::screen_animating(&mut m, &fw) {
                break;
            }
            assert!(
                m.ms() < t0 + CARD_TIMEOUT_MS,
                "THE {name}: the card did not end"
            );
        }
        let calls = &m.watched_calls[from.0..];
        let c = Cost {
            polls: calls.len(),
            instructions: calls.iter().sum(),
            worst_poll: calls.iter().copied().max().unwrap_or(0),
            frames,
        };
        let pass = m.passes[from.1..].iter().copied().max().unwrap_or(0);
        // The input, alone.
        let mut m = booted(&fw);
        m.watch_calls(fw.symbol(input));
        if name != "oscillator" {
            open_menu(&mut m, &fw);
            select(
                &mut m,
                if name == "scene" {
                    MENU_SCENE
                } else {
                    MENU_CHIP
                },
            );
            m.run_ms(100);
            let n = m.watched_calls.len();
            m.turn(ALGORITHM, 1);
            m.run_ms(100);
            assert!(showing_card(&mut m, &fw));
            report.insert(
                format!("{name}_input"),
                serde_json::json!(m.watched_calls[n..].iter().max()),
            );
        } else {
            open_menu(&mut m, &fw);
            let n = m.watched_calls.len();
            press(&mut m, HOME);
            assert!(showing_card(&mut m, &fw));
            report.insert(
                format!("{name}_input"),
                serde_json::json!(m.watched_calls[n..].iter().max()),
            );
        }
        let input_cost = report[&format!("{name}_input")].as_u64().unwrap_or(0);
        let per = cells_sent(&c);
        let most = per.iter().copied().max().unwrap_or(0);
        println!(
            "THE {}: {} frames, {:.1} cells a frame, at most {most}; worst {:.2} ms of SPI; \
             {:.0} instructions a frame, busiest ui_poll {}; the input {input_cost}, the busiest \
             pass {pass}, together {}",
            name.to_uppercase(),
            per.len(),
            f64::from(per.iter().sum::<u32>()) / per.len() as f64,
            c.worst_frame_ms(),
            c.instructions_per_frame(),
            c.worst_poll,
            input_cost + pass
        );
        report.insert(name.to_string(), animated_json(&c));
        assert!(
            c.worst_frame_ms() <= SPI_BUDGET_MS,
            "THE {name}: a frame took {:.1} ms of SPI",
            c.worst_frame_ms()
        );
        assert!(
            most <= sidou_sys::TITLE_CARD_FRAME_CELLS,
            "THE {name}: a frame sent {most} cells"
        );
        assert!(
            c.worst_poll < TITLE_CARD_WORST_POLL,
            "THE {name}: a ui_poll took {} instructions",
            c.worst_poll
        );
        assert!(
            c.instructions_per_frame() < TITLE_CARD_PER_FRAME,
            "THE {name}: {:.0} instructions a frame",
            c.instructions_per_frame()
        );
        assert!(
            input_cost < TITLE_CARD_INPUT && pass < TITLE_CARD_PASS,
            "THE {name}: the input {input_cost}, the busiest pass {pass}"
        );
        // From the input to the view under it cut in: the glow, the wipe, the 1 s hold, the cut.
        assert!(
            (60..=72).contains(&per.len()),
            "THE {name}: {} frames",
            per.len()
        );
    }
    println!("{}", serde_json::Value::Object(report));
}

/// The title cards in fm1-emu (title_cards_stay_within_their_budgets), bounded about 10 %
/// above what was measured (M1e, the cards' first version):
///
/// | Card | Frames | Cells a frame (average, most) | Worst SPI | Instructions a frame | Busiest ui_poll | Input | Busiest pass |
/// |---|---|---|---|---|---|---|---|
/// | THE SCENE | 66 | 72.4, 200 | 17.14 ms | 56.0k | 71.6k | 350 (ui_encoder) | 73.1k |
/// | THE OSCILLATOR | 64 | 68.7, 200 | 17.14 ms | 55.1k | 71.3k | 387 (ui_buttons) | 72.8k |
/// | THE CHIP (M1c Task 4) | 66 | 72.4, 200 | 17.14 ms | 55.6k | 71.2k | 364 (ui_encoder) | 72.9k |
///
/// The busiest frames are the glow's first level (200 cells, the most a frame) and its scan
/// of the screen's 900 cells for those that differ.
const TITLE_CARD_WORST_POLL: u64 = 79_000;
const TITLE_CARD_PER_FRAME: f64 = 62_000.0;
const TITLE_CARD_INPUT: u64 = 430;
const TITLE_CARD_PASS: u64 = 80_000;

// A ui_poll holds MIDI's drain for as long as it runs: every view's worst stays within the
// latency gate's poll allowance (common::POLL_INSTRUCTIONS, 120k), so a compose that grows
// past it fails here, not as a late note somewhere else.
const _: () = assert!(LIVE_WORST_POLL <= common::POLL_INSTRUCTIONS);
const _: () = assert!(SCENE_WORST_POLL <= common::POLL_INSTRUCTIONS);
const _: () = assert!(SCENE_INPUT + SCENE_START_PASS <= common::POLL_INSTRUCTIONS);
const _: () = assert!(ABOUT_WORST_POLL <= common::POLL_INSTRUCTIONS);
const _: () = assert!(BOOT_WORST_POLL <= common::POLL_INSTRUCTIONS);
const _: () = assert!(TITLE_CARD_WORST_POLL <= common::POLL_INSTRUCTIONS);
const _: () = assert!(TITLE_CARD_INPUT + TITLE_CARD_PASS <= common::POLL_INSTRUCTIONS);
const _: () = assert!(CHIP_WORST_POLL <= common::POLL_INSTRUCTIONS);
const _: () = assert!(CHIP_INPUT + CHIP_PASS <= common::POLL_INSTRUCTIONS);
