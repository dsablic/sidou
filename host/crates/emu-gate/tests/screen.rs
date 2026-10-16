// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The screen in fm1-emu: LCD snapshots of the views against golden images
//! (tests/golden/lcd/), reached through the panel as a player would (HOME, SELECT, ALGORITHM);
//! the boot sequence, short and full, and skipping it; LIVE's scopes following the notes; the
//! CHIP view's pages and what MIDI and the knobs change on them; the song views' frame (SEQ,
//! REC, GLO's mutes);
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
/// app/panel.h: the song views' buttons (Felucca's ids, unconfirmed on a SIDOU unit) and the
/// first note key's id.
const GLO: u8 = 7;
const SEQ: u8 = 11;
const PLAY: u8 = 12;
const REC: u8 = 13;
const KEY0: u8 = 14;
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
const MENU_MIDI: i32 = sidou_sys::VIEWS_MENU_MIDI as i32;
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
fn the_menu_matches_its_golden_images_at_the_top_and_scrolled_and_does_not_blink() {
    let Some((build, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    open_menu(&mut m, &fw);
    // 2 times the font, 6 items of 13: the first six, the selected line in reverse video.
    compare_golden(&build, "menu", &m.soc.lcd.fb, &[]);
    // The selection is a line in reverse video that does not blink (a 2 x 2 cell block would
    // send 2 windows each phase, over screen_changes_and_transitions_stay_within_the_budgets'
    // bound for the menu idle): the panel stands still.
    let still = m.soc.lcd.fb.clone();
    m.run_ms(16 * UI_PERIOD_MS);
    assert!(m.soc.lcd.fb == still, "the idle menu changed");
    // Scrolled to the last item: the last six, an arrow up in place of the one down.
    select(&mut m, MENU_ITEMS - 1);
    m.run_ms(300);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "menu-scrolled", &m.soc.lcd.fb, &[]);
}

/// A knob's popup (app/screen.h) is gone this long after its last detent: a second, and the three
/// frames it takes to close, with room.
const POPUP_GONE_MS: u64 = 2_000;

/// The firmware without its popup (views.popup.source NULL): for the gates of what a view itself
/// costs when a control changes, which a popup's hundreds of cells would drown.
fn no_popup(m: &mut Machine, fw: &Firmware) {
    let at = fw.symbol("ui")
        + (sidou_sys::ilp32::views::popup + sidou_sys::ilp32::screen_popup_state::source) as u32;
    m.soc.write32(at, 0);
}

/// The value popup (app/screen.h) in the device's firmware: over LIVE and over CHIP, K1's
/// parameter in the largest text with its bar; a second after the last turn it goes and LIVE is
/// back as it was.
#[test]
fn a_knob_popup_matches_its_goldens_over_live_and_chip_and_gives_the_view_back() {
    let Some((build, fw)) = fixture("the popup test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    let live = m.soc.lcd.fb.clone();
    m.turn(K1, 5);
    m.run_ms(300);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "popup-live", &m.soc.lcd.fb, &[]);
    // The cutoff back where it was; a second after that turn the popup closes (3 rows a
    // frame) and the panel shows LIVE as before the knob.
    m.turn(K1, -5);
    m.run_ms(1700);
    common::to_mid_frame(&mut m, &fw);
    assert!(m.soc.lcd.fb == live, "LIVE differs after the popup");
    open_chip(&mut m, &fw);
    let chip = m.soc.lcd.fb.clone();
    m.turn(K1, 3);
    m.run_ms(300);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "popup-chip", &m.soc.lcd.fb, &[]);
    m.turn(K1, -3);
    m.run_ms(1700);
    common::to_mid_frame(&mut m, &fw);
    assert!(m.soc.lcd.fb == chip, "CHIP differs after the popup");
}

/// The value popup's costs over each view (app/screen.h popup_step, popup_draw, app/views.c
/// popup_text): K1 turned and the popup open, held a second and closed (3 rows, 78 cells, a frame
/// each way), with a note held under LIVE and SCENE; turned every other frame for a second; and
/// held static, which draws the box again every frame (each cell written once, so nothing is
/// sent) and forces nothing on the view under it. Every frame within the SPI budget and 200
/// cells, every ui_poll and a static popup's extra instructions a frame within the bounds below
/// (about 15 % over what was measured, M2f large text). A static popup draws again only when
/// the view composed (LIVE and SCENE compose every frame), and sends nothing:
///
/// | View | Cells a frame (most) | Worst SPI | Busiest ui_poll | Static popup, extra a frame |
/// |---|---|---|---|---|
/// | LIVE (a note) | 132 | 11.3 ms | 30.7k | 7.3k |
/// | STATUS | 132 | 11.3 ms | 33.7k | 0.15k |
/// | MENU | 132 | 11.3 ms | 36.5k | 0.15k |
/// | CHIP | 132 | 11.3 ms | 35.4k | 0.1k |
/// | SCENE, RULE 30 (a note) | 158 | 13.6 ms | 31.3k | 12.5k |
const POPUP_OPEN_FRAMES: u64 = 70;
const POPUP_WORST_POLL: u64 = 42_000;
const POPUP_STATIC_EXTRA: f64 = 15_000.0;
const _: () = assert!(POPUP_WORST_POLL <= common::POLL_INSTRUCTIONS);

#[test]
fn the_popup_stays_within_its_budgets_over_every_view() {
    let Some((_, fw)) = fixture("the popup budget test") else {
        return;
    };
    let mut report = serde_json::Map::new();
    let mut over = Vec::new();
    for view in ["live", "status", "menu", "chip", "scene"] {
        let mut m = Machine::boot(&fw.image);
        m.watch_calls(fw.symbol("ui_poll"));
        common::run_past_boot(&mut m, &fw);
        match view {
            "status" => common::open_status(&mut m, &fw),
            "menu" => open_menu(&mut m, &fw),
            "chip" => open_chip(&mut m, &fw),
            "scene" => open_scene(&mut m, &fw, 2),
            _ => settle(&mut m, &fw),
        }
        if view == "live" || view == "scene" {
            m.key(A4_KEY, true);
            m.run_ms(300);
        }
        let idle = measure(&mut m, &fw, 10, |_| {});
        // K1 once: it opens, stays a second and closes within the 70 frames.
        let run = measure(&mut m, &fw, POPUP_OPEN_FRAMES, |m| m.turn(K1, 3));
        // Held open: a turn every other frame for a second, then the popup static.
        let turning = measure_frames(&mut m, &fw, 30, |m, f| {
            if f % 2 == 0 {
                m.turn(K1, if f % 4 == 0 { 3 } else { -3 });
            }
        });
        m.run_ms(200); // the last detents' values go out
        let stay = measure(&mut m, &fw, 10, |_| {});
        for (what, c) in [("run", &run), ("turning", &turning)] {
            let most = c.frames.iter().map(cells).max().unwrap();
            assert!(most <= 200, "{view} {what}: a frame sent {most} cells");
            assert!(
                c.worst_frame_ms() <= SPI_BUDGET_MS,
                "{view} {what}: {:.1} ms of SPI",
                c.worst_frame_ms()
            );
            assert!(
                c.worst_poll < POPUP_WORST_POLL,
                "{view} {what}: a ui_poll took {}",
                c.worst_poll
            );
        }
        let extra = stay.instructions_per_frame() - idle.instructions_per_frame();
        println!(
            "popup over {view}: {} cells a frame at most, {:.1} ms of SPI, ui_poll {} / {}, a static popup costs {extra:.0} a frame, sent {} windows",
            run.frames.iter().chain(&turning.frames).map(cells).max().unwrap(),
            run.worst_frame_ms().max(turning.worst_frame_ms()),
            run.worst_poll,
            turning.worst_poll,
            stay.total().windows
        );
        // (LIVE and SCENE move on their own: they send what they send)
        let still = !matches!(view, "live" | "scene");
        if extra >= POPUP_STATIC_EXTRA || (still && stay.total().windows != 0) {
            over.push(format!(
                "{view}: a static popup costs {extra:.0} instructions a frame and sent {} windows",
                stay.total().windows
            ));
        }
        for (what, c) in [("run", &run), ("turning", &turning)] {
            let most = c.frames.iter().map(cells).max().unwrap();
            if most > 200 || c.worst_frame_ms() > SPI_BUDGET_MS || c.worst_poll >= POPUP_WORST_POLL
            {
                over.push(format!(
                    "{view} {what}: {most} cells, {:.1} ms of SPI, a ui_poll of {}",
                    c.worst_frame_ms(),
                    c.worst_poll
                ));
            }
        }
        report.insert(
            view.to_string(),
            serde_json::json!({
                "idle_per_frame": idle.instructions_per_frame(),
                "static_per_frame": stay.instructions_per_frame(),
                "worst_poll_run": run.worst_poll,
                "worst_poll_turning": turning.worst_poll,
                "most_cells_run": run.frames.iter().map(cells).max(),
                "most_cells_turning": turning.frames.iter().map(cells).max(),
                "worst_spi_ms": run.worst_frame_ms().max(turning.worst_frame_ms()),
            }),
        );
    }
    println!("{}", serde_json::Value::Object(report));
    assert!(over.is_empty(), "{over:#?}");
}

/// A turn that asks for the popup stays within an input bound whatever view takes it (the CHIP
/// view's own: CHIP_INPUT): the input's call (ui_encoder, which stores the ask) and the busiest
/// pass after it, in which the frame asks the views for the text and draws the popup's first
/// rows. Measured (M2f): K1 218 on LIVE and STATUS (234 over the menu, 230 on SCENE), K2 170 to
/// 186, ALGORITHM 166, SELECT 129, the busiest pass 37.2k on the views that compose little and
/// 59.8k on SCENE (its own piece). K4 (the preset) is no popup's cost:
/// it loads a preset, 1,185 here and 88,997 in the busiest pass over LIVE, as before.
const POPUP_INPUT: u64 = 250;
const POPUP_PASS: u64 = 70_000;
const _: () = assert!(POPUP_INPUT + POPUP_PASS <= common::POLL_INSTRUCTIONS);

#[test]
fn a_popup_ask_keeps_the_input_and_the_passes_within_the_allowance() {
    let Some((_, fw)) = fixture("the popup input test") else {
        return;
    };
    let mut over = Vec::new();
    for view in ["live", "status", "menu", "scene"] {
        let mut m = booted(&fw);
        match view {
            "status" => common::open_status(&mut m, &fw),
            "menu" => {
                open_menu(&mut m, &fw);
                select(&mut m, 3); // OCTAVE
            }
            "scene" => open_scene(&mut m, &fw, 0),
            _ => settle(&mut m, &fw),
        }
        m.watch_calls(fw.symbol("ui_encoder"));
        m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
        let mut turns = vec![("K1", K1, 1), ("K2", K1 + 1, 1)];
        if view == "menu" {
            turns.push(("ALGORITHM", ALGORITHM, 1));
            turns.push(("SELECT", SELECT, 1));
        }
        for (what, enc, steps) in turns {
            let (input, pass) = start_cost(&mut m, 1600, |m| m.turn(enc, steps));
            println!("popup over {view}, {what}: the input {input}, the busiest pass {pass}");
            if input >= POPUP_INPUT || pass >= POPUP_PASS {
                over.push(format!("{view} {what}: the input {input}, the pass {pass}"));
            }
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}

/// The MIDI page (M2e Task 3, app/views.c): its golden image, the wipe that opens it and an
/// edit of MODE within the views' budgets, and the edit reaching MIDI's mode.
#[test]
fn the_midi_page_matches_its_golden_image_and_stays_within_the_budgets() {
    let Some((build, fw)) = fixture("the MIDI page test") else {
        return;
    };
    let mut m = booted(&fw);
    m.watch_calls(fw.symbol("ui_poll"));
    open_menu(&mut m, &fw);
    select(&mut m, MENU_MIDI);
    settle(&mut m, &fw);
    let open = measure(&mut m, &fw, 16, |m| m.turn(ALGORITHM, 1));
    settle(&mut m, &fw);
    compare_golden(&build, "midi", &m.soc.lcd.fb, &[VERSION_CELLS, MENU_CURSOR]);
    let idle = measure(&mut m, &fw, 16, |_| {});
    let edit = measure(&mut m, &fw, 8, |m| m.turn(ALGORITHM, -3));
    let mode = m.soc.read8(
        fw.symbol("routing")
            + (sidou_sys::ilp32::midi_route::setup + sidou_sys::ilp32::midi_route_setup::mode)
                as u32,
    );
    assert_eq!(
        mode,
        sidou_sys::MIDI_MODE_MULTI as u8,
        "MODE did not reach the router"
    );
    println!(
        "{}",
        serde_json::json!({
            "wipe_down_to_the_midi_page": open.json(),
            "midi_page_idle": idle.json(),
            "midi_page_mode_edit": edit.json(),
        })
    );
    for (what, c) in [("open", &open), ("idle", &idle), ("edit", &edit)] {
        assert!(
            c.worst_frame_ms() <= SPI_BUDGET_MS,
            "the MIDI page's {what}: a frame took {:.1} ms of SPI",
            c.worst_frame_ms()
        );
        assert!(
            c.worst_poll < MIDI_PAGE_WORST_POLL,
            "the MIDI page's {what}: a ui_poll took {} instructions",
            c.worst_poll
        );
    }
    assert!(
        idle.total().windows <= 2,
        "{} windows idle on the MIDI page",
        idle.total().windows
    );
}

/// The MIDI page's busiest ui_poll, about 10 % above what was measured (M2e Task 3: 20.6k,
/// the wipe that opens it; an edit of MODE 20.3k).
const MIDI_PAGE_WORST_POLL: u64 = 23_000;
const _: () = assert!(MIDI_PAGE_WORST_POLL <= common::POLL_INSTRUCTIONS);

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

/// MIDI learn's frame (app/learn_view.h) over LIVE as SAVE held + K1 open it on the cutoff and
/// it asks for a CC, and the toast once CC 21 came over TRS. Only rows 23..27 count, the
/// frame's and the toast's (and the border): the view under the frame is not this test's.
#[test]
fn midi_learn_prompts_match_their_golden_images() {
    let Some((build, fw)) = fixture("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    m.key(SAVE, true);
    m.run_ms(30);
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
    // The cursor is on CHARSET now (the menu scrolled to it); back on the first item the menu
    // is its golden again.
    press(&mut m, HOME);
    settle(&mut m, &fw);
    select(&mut m, 0);
    m.run_ms(300);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "menu", &m.soc.lcd.fb, &[]);
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
    let same = |a: &[u16], b: &[u16]| (0..8 * W).all(|i| a[i] == b[i]);
    let (f, s, mm) = (cell_rows(fb), cell_rows(status), cell_rows(menu));
    (0..30)
        .map(|r| match (same(f[r], s[r]), same(f[r], mm[r])) {
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
/// the windows and SPI bytes each frame (each compose) in it sent.
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

/// What each compose sent: from its compose (ui.c's `last_compose`, stamped as the compose
/// starts; its cells go out in later ui_poll calls) to the next one's, the last to `end`. Run
/// 100 us at a time: a compose's own call is longer, so its cells are never counted before it.
struct Composes {
    last: u32,
    stamp: u32,
    start: Option<Sent>,
    per: Vec<Sent>,
}

impl Composes {
    fn new(m: &mut Machine, fw: &Firmware) -> Self {
        let last = fw.symbol("last_compose");
        Composes {
            last,
            stamp: m.soc.read32(last),
            start: None,
            per: Vec::new(),
        }
    }

    fn close(&mut self, m: &mut Machine, fw: &Firmware) {
        if let Some(b) = self.start {
            let now = sent(m, fw);
            self.per.push(Sent {
                windows: now.windows - b.windows,
                bytes: now.bytes - b.bytes,
            });
        }
    }

    /// Runs `ms`, closing a frame at each compose.
    fn run(&mut self, m: &mut Machine, fw: &Firmware, ms: u64) {
        let step = m.soc.periph.cycle_of_ms(1) / 10;
        let end = m.soc.periph.cycles + m.soc.periph.cycle_of_ms(ms);
        while m.soc.periph.cycles < end {
            m.run_cycles(step);
            let s = m.soc.read32(self.last);
            if s != self.stamp {
                self.stamp = s;
                self.close(m, fw);
                self.start = Some(sent(m, fw));
            }
        }
    }

    /// The frames, the last closed now.
    fn end(mut self, m: &mut Machine, fw: &Firmware) -> Vec<Sent> {
        self.close(m, fw);
        self.per
    }
}

/// `frames` frames' time, after `act` (a key going down, say) at the start of the first: what
/// each compose in it sent, from its compose (ui.c's `last_compose`, stamped as the compose
/// starts; its cells go out in later ui_poll calls) to the next one's, the last to the end. Per
/// compose, not per UI_PERIOD_MS of time: a window of time can hold the tail of one frame and
/// the whole of the next, which no frame sends.
fn measure(m: &mut Machine, fw: &Firmware, frames: u64, act: impl FnOnce(&mut Machine)) -> Cost {
    common::to_mid_frame(m, fw);
    let from = m.watched_calls.len();
    act(m);
    let mut composes = Composes::new(m, fw);
    composes.run(m, fw, frames * UI_PERIOD_MS);
    let per = composes.end(m, fw);
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
    // The cursor is on ABOUT now (the menu scrolled to it); back on the first item the menu is
    // its golden again.
    select(&mut m, 0);
    m.run_ms(300);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "menu", &m.soc.lcd.fb, &[]);
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

/// LIVE while the keys play chip B (M2b Task 4: a song on chip A, here its hook,
/// `keys_router.song`): the header says KEYS:B, and A4 shows on the scope read from chip B.
#[test]
fn live_shows_the_keys_on_chip_b() {
    let Some((build, fw)) = fixture("the LIVE keys test") else {
        return;
    };
    let mut m = booted(&fw);
    m.soc.write8(
        fw.symbol("keys_router") + sidou_sys::ilp32::keys_route::song as u32,
        1,
    );
    m.key(A4_KEY, true);
    m.run_ms(1500);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "live-keys-b", &m.soc.lcd.fb, &[]);
    let a4 = trace_pixels(&m.soc.lcd.fb);
    assert!(a4[0] > 200, "voice 1's trace with A4 on chip B: {a4:?}");
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
/// SELECT's order (app/scene.h): the five PETSCII pieces, then RED MOON RITE (app/scene_rmr.h).
const SCENE_PIECES: [&str; 6] = ["fire", "plasma", "rule30", "stars", "ant", "rmr"];
const _: () = assert!(SCENE_PIECES.len() == sidou_sys::SCENE_PIECES as usize);
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
fn scene_matches_its_golden_image(piece: usize) {
    let Some((build, fw)) = fixture("the SCENE test") else {
        return;
    };
    let mut m = booted(&fw);
    open_scene(&mut m, &fw, 0);
    assert!(common::tap_on(&mut m, &fw), "the tap is off under SCENE");
    for (k, name) in SCENE_PIECES.iter().enumerate().take(piece + 1) {
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
        if k == piece {
            compare_golden(&build, &format!("scene-{name}"), &m.soc.lcd.fb, &[]);
        }
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

#[test]
fn scene_fire_matches_its_golden_image() {
    scene_matches_its_golden_image(0);
}

#[test]
fn scene_plasma_matches_its_golden_image() {
    scene_matches_its_golden_image(1);
}

#[test]
fn scene_rule30_matches_its_golden_image() {
    scene_matches_its_golden_image(2);
}

#[test]
fn scene_stars_matches_its_golden_image() {
    scene_matches_its_golden_image(3);
}

#[test]
fn scene_ant_matches_its_golden_image() {
    scene_matches_its_golden_image(4);
}

#[test]
fn scene_rmr_matches_its_golden_image() {
    scene_matches_its_golden_image(SCENE_RMR);
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
        m.run_ms(POPUP_GONE_MS); // the popup it opened (app/screen.h) has closed
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
        // RED MOON RITE's ui_polls render the compositor's runs (two of 8 painted cells,
        // bounded by scene_budget.rs), its own bound; so is the wipe into it, from ANT.
        let poll_bound = |k: usize| {
            if k == SCENE_RMR {
                RMR_WORST_POLL
            } else {
                SCENE_WORST_POLL
            }
        };
        let next = (piece + 1) % SCENE_PIECES.len();
        assert!(
            c.worst_poll < poll_bound(piece) && wipe_poll < poll_bound(piece).max(poll_bound(next)),
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
        m.run_ms(POPUP_GONE_MS);
        let mut opened = 0;
        let c = measure_frames(&mut m, &fw, 120, |m, f| {
            band(m, f);
            match f {
                10 => m.key(SAVE, true),
                11 => m.turn(K1_CUTOFF, 1),
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
const SCENE_BOUNDS: [(f64, u32, f64); 6] = [
    (138.0, 175, 108_000.0),   // fire
    (152.0, 200, 152_000.0),   // plasma: the most is SCENE_VIEW_FRAME_CELLS
    (21.0, 84, 33_500.0),      // rule 30
    (44.0, 75, 79_000.0),      // stars
    (3.0, 10, 30_000.0),       // ant
    (133.0, 136, 1_140_000.0), // red moon rite: the pair drifting a pixel a frame
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
/// RED MOON RITE (M2f Task 4): SELECT from it to FIRE clears its painter region, which marks
/// every cell (measured below, bounded about 10 % over), and the busiest pass after SELECT to
/// it or the menu into it is one of its ui_polls (its start, or two runs of the compositor).
const RMR_INPUT: u64 = 1_050;
const RMR_START_PASS: u64 = 65_000;

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
        // SELECT from RED MOON RITE clears its painter region (every cell marked), and its
        // start and its ui_polls are the compositor's: their own bounds.
        let input_bound = if what == "SELECT to fire" || what == "SELECT to rmr" {
            RMR_INPUT
        } else if what.starts_with("SELECT") {
            SCENE_INPUT
        } else {
            TITLE_CARD_INPUT
        };
        let pass_bound = if what.ends_with("rmr") {
            RMR_START_PASS
        } else {
            SCENE_START_PASS
        };
        if *input >= input_bound || *pass >= pass_bound {
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

/// The CHIP view's focus (app/chip_view.h chip_view_focus): 0xff the tabs, 0 the chip, 1 + the
/// selected slot.
fn chip_focus(m: &mut Machine, fw: &Firmware) -> u8 {
    let tabs = ui_byte(
        m,
        fw,
        sidou_sys::ilp32::views::chip + sidou_sys::ilp32::chip_view::tabs,
    );
    if tabs != 0 {
        return sidou_sys::CHIP_VIEW_TABS as u8;
    }
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
    // The B tab (M2b Task 5), from the tabs' focus above CHIP: with no song, the keys' setup
    // (chip B follows it), the slots shared.
    m.turn(SELECT, -4);
    m.run_ms(100);
    assert_eq!(chip_focus(&mut m, &fw), sidou_sys::CHIP_VIEW_TABS as u8);
    m.turn(ALGORITHM, 1);
    m.run_ms(1600); // the toast of the slots SELECT went through
    settle(&mut m, &fw);
    compare_golden(&build, "chip-b", &m.soc.lcd.fb, &[]);
    // A song on chip A (its hook, keys_router.song): the keys on chip B, KEYS on its tab.
    m.soc.write8(
        fw.symbol("keys_router") + sidou_sys::ilp32::keys_route::song as u32,
        1,
    );
    m.run_ms(300);
    settle(&mut m, &fw);
    compare_golden(&build, "chip-b-keys", &m.soc.lcd.fb, &[]);
    // The A tab during the song: the song's chip, its setup (as the song found chip A).
    m.turn(ALGORITHM, -1);
    m.run_ms(300);
    settle(&mut m, &fw);
    compare_golden(&build, "chip-a-song", &m.soc.lcd.fb, &[]);
}

/// On the B tab during a song (the keys on chip B), a profile step reaches chip B, the keys',
/// and leaves chip A, the song's, bit for bit: its sid in emulated RAM is byte for byte what it
/// is at the same moment in a run without the step (so its audio is too).
#[test]
fn a_profile_step_on_the_b_tab_leaves_chip_a_bit_exact() {
    let Some((_, fw)) = fixture("the CHIP B tab test") else {
        return;
    };
    let run = |step: bool| {
        let mut m = booted(&fw);
        m.soc.write8(
            fw.symbol("keys_router") + sidou_sys::ilp32::keys_route::song as u32,
            1,
        );
        m.run_ms(100);
        open_chip(&mut m, &fw);
        m.turn(SELECT, -1);
        m.run_ms(50);
        m.turn(ALGORITHM, 1); // the B tab
        m.run_ms(50);
        m.turn(SELECT, 1);
        m.run_ms(50);
        if step {
            m.turn(ALGORITHM, 1); // R4AR to the 8580 R5, the keys' setup on chip B
        }
        m.run_ms(400);
        let sid = |m: &mut Machine, x: usize| {
            let base = fw.chips();
            let active = m
                .soc
                .read8(base + (sidou_sys::ilp32::reg_ring_chips::active + x) as u32);
            let s = base
                + (sidou_sys::ilp32::reg_ring_chips::chip
                    + usize::from(active) * sidou_sys::ilp32::sid::SIZE) as u32;
            (0..sidou_sys::ilp32::sid::SIZE as u32)
                .map(|i| m.soc.read8(s + i))
                .collect::<Vec<u8>>()
        };
        let model = |m: &mut Machine, x: usize| {
            let v = sid(m, x);
            v[sidou_sys::ilp32::sid::model]
        };
        (sid(&mut m, 0), model(&mut m, 1))
    };
    let (a_still, b_still) = run(false);
    let (a_step, b_step) = run(true);
    assert_eq!((b_still, b_step), (0, 1), "the step did not reach chip B");
    assert!(
        a_still == a_step,
        "chip A changed with the step on the B tab"
    );
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
    assert_eq!(character(&mut m, &fw), [100, 40, 64, 20]);
    m.run_ms(POPUP_GONE_MS); // the knob's popup has closed
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "chip", &m.soc.lcd.fb, &[]);
    m.turn(SELECT, -3);
    m.run_ms(100);
    assert_eq!(
        chip_focus(&mut m, &fw),
        sidou_sys::CHIP_VIEW_TABS as u8,
        "SELECT did not stop at the tabs, above the chip"
    );
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
/// allowance together. Measured: the input 210 at most (K1 on DIST, which stores the popup's ask;
/// 217 before M2f's large text, whose input path saved 25 instructions elsewhere), the busiest pass 44.5k
/// (SELECT from FX back to the chip: three frames change colour).
const CHIP_INPUT: u64 = 215;
const CHIP_PASS: u64 = 49_000;
/// ALGORITHM on the chip steps the profile, as the FX button and CC 85 do: the live tick
/// prepares the spare chip a slice a tick (app/reg_ring.h, reg_ring_prepare_step; decision
/// record 0006, "Cost") and swaps it in once it is whole, so the step's passes are the
/// others' plus a slice, within the poll allowance like every pass (M2b Task 1 retired M1's
/// exception, a reset in one pass of up to 471.5k).
///
/// REG_RING_PREPARE_SLICE: the most one slice may take (app/reg_ring.h), measured at full
/// character onto every profile (chip_resets_at_full_character_stay_within_the_poll_allowance).
/// With the busiest pass of LIVE or CHIP it stays within the allowance (asserted below).
const REG_RING_PREPARE_SLICE: u64 = 40_000;
const _: () = assert!(REG_RING_PREPARE_SLICE == sidou_sys::REG_RING_PREPARE_SLICE as u64);

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
    // The view's own cost: the popup a knob opens has its own gate (the_popup_stays_within_...).
    no_popup(&mut m, &fw);
    let idle = measure(&mut m, &fw, 25, |_| {});
    let knob = measure(&mut m, &fw, 25, |m| m.turn(K1_CHIP, 1));
    // A tab switch (M2b Task 5), from the tabs' focus: the tabs' row and the help line.
    m.turn(SELECT, -1);
    m.run_ms(200);
    let tab = measure(&mut m, &fw, 25, |m| m.turn(ALGORITHM, 1));
    m.turn(ALGORITHM, -1);
    m.turn(SELECT, 1);
    m.run_ms(200);
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
        "chip_tab": animated_json(&tab),
        "chip_busiest": animated_json(&busy),
    });
    println!("{report}");
    assert_eq!(idle.total().windows, 0, "the idle CHIP view sent something");
    for (what, c, per_frame) in [
        ("idle", &idle, CHIP_IDLE_PER_FRAME),
        ("a knob", &knob, CHIP_KNOB_PER_FRAME),
        ("a tab switch", &tab, CHIP_KNOB_PER_FRAME),
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
/// CHIP_INPUT and every pass while the spare is prepared and swapped in within CHIP_PASS and
/// a slice (REG_RING_PREPARE_SLICE), so within the poll allowance.
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
        ("SELECT to the tabs", SELECT, -1),
        ("ALGORITHM to the B tab", ALGORITHM, 1),
        ("K1 on the tabs", K1_CHIP, 1),
        ("ALGORITHM to the A tab", ALGORITHM, -1),
        ("SELECT to the chip", SELECT, 1),
    ] {
        let (input, pass) = start_cost(&mut m, 1600, |m| m.turn(enc, steps));
        println!("CHIP {what}: the input {input}, the busiest pass {pass}");
        if input >= CHIP_INPUT || pass >= CHIP_PASS {
            over.push(format!(
                "{what}: the input {input}, the busiest pass {pass}"
            ));
        }
    }
    // The profile: the input as the others', the passes with a slice of the preparation.
    for (what, steps) in [("ALGORITHM on the chip", 1), ("ALGORITHM back", -1)] {
        let (input, pass) = start_cost(&mut m, 1600, |m| m.turn(ALGORITHM, steps));
        println!("CHIP {what}: the input {input}, the busiest pass {pass}");
        if input >= CHIP_INPUT || pass >= CHIP_PASS + REG_RING_PREPARE_SLICE {
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
/// the busiest main-loop pass after each within the poll allowance, every slice of the spare's
/// preparation within REG_RING_PREPARE_SLICE, as many slices as the host's preparation counts
/// (sid_character_prep_slices; the first onto the 8580 4 more, its default DACs), and none
/// prepared at once (unprepared_resets: live_init's only).
#[test]
fn chip_resets_at_full_character_stay_within_the_poll_allowance() {
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
    m.watch_calls(fw.symbol("reg_ring_prepare_step"));
    let out = fw.symbol("out");
    let out_u32 = |m: &mut Machine, field: usize| m.soc.read32(out + field as u32);
    let mut costs: Vec<(String, u64, u64)> = Vec::new();
    // The register ring's default_8580_spent: the first 8580 preparation spends
    // SID_DAC_BUILD_STEPS slices on its default DACs (sid_dac_default_step).
    let spent = fw.chips() + sidou_sys::ilp32::reg_ring_chips::default_8580_spent as u32;
    let mut run = |m: &mut Machine, what: String, want: u32, act: &dyn Fn(&mut Machine)| {
        let slices = out_u32(m, sidou_sys::ilp32::reg_ring_out::slices);
        let model = unsafe { (*sidou_sys::sid_chip_profile_get(want)).model };
        let built = u32::from(model) != sidou_sys::SID_MODEL_8580 || m.soc.read8(spent) != 0;
        let calls = m.watched_calls.len();
        let (_, pass) = start_cost(m, 500, act);
        assert_eq!(u32::from(profile(m, &fw)), want, "{what}");
        let slice = m.watched_calls[calls..].iter().copied().max().unwrap_or(0);
        let n = out_u32(m, sidou_sys::ilp32::reg_ring_out::slices) - slices;
        let c = sidou_sys::sid_character {
            profile: want as u8 + 1,
            age: 127,
            dac: 127,
            thump: 127,
            bleed: 127,
        };
        let mut expect = unsafe { sidou_sys::sid_character_prep_slices(model.into(), &c) };
        if !built {
            expect += sidou_sys::SID_DAC_BUILD_STEPS;
        }
        println!("chip reset, {what}: the busiest pass {pass}, slice {slice}, {n} slices");
        assert_eq!(n, expect, "{what}: the slices");
        costs.push((what, pass, slice));
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
    // CC 85 (0..12 over 0..127, the profiles then USER 1..8): onto each profile, ending on WORN.
    for want in [
        r3,
        SID_CHIP_6581_R4AR,
        SID_CHIP_8580_R5,
        SID_CHIP_6581_R2,
        SID_CHIP_6581_WORN,
    ] {
        let v = (want * 128).div_ceil(13) as u8;
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
    println!("chip reset, the worst pass: {} ({})", worst.1, worst.0);
    let worst = costs.iter().max_by_key(|c| c.2).unwrap();
    println!("chip reset, the worst slice: {} ({})", worst.2, worst.0);
    let over: Vec<_> = costs
        .iter()
        .filter(|c| c.1 >= common::POLL_INSTRUCTIONS || c.2 >= REG_RING_PREPARE_SLICE)
        .collect();
    assert!(over.is_empty(), "{over:#?}");
    assert_eq!(
        out_u32(&mut m, sidou_sys::ilp32::reg_ring_out::unprepared_resets),
        1,
        "a reset was prepared at once"
    );
}

/// The DACs moved during a profile step (review I1): CC 87 sweeping every 10 ms while CC 85
/// steps R4AR to WORN and back on LIVE with a chord of noise held (its busiest view), each
/// step's swap pass among them, at several phases of the sweep against the step; every
/// main-loop pass within the poll allowance, so no pass runs a slice and a ramp table step
/// together.
#[test]
fn a_dac_sweep_during_a_profile_step_holds_every_pass_within_the_allowance() {
    let Some((_, fw)) = fixture("the profile step and DAC sweep test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&[0xB0, 20, 64, 0xB0, 79, 127]); // noise, full sustain: LIVE's busiest
    for k in CHORD {
        m.key(k, true);
    }
    m.run_ms(300);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut worst = 0;
    let mut dac = 0u8;
    for (phase, profile) in [
        (0u64, 43u8), // WORN
        (3, 20),      // R4AR
        (7, 43),
        (11, 20),
        (13, 43),
        (17, 20),
    ] {
        let from = m.passes.len();
        m.run_ms(phase);
        m.midi(&[0xB0, 85, profile]);
        for _ in 0..30 {
            dac = if dac >= 120 { 0 } else { dac + 24 };
            m.midi(&[0xB0, 87, dac]);
            m.run_ms(10);
        }
        let pass = m.passes[from..].iter().copied().max().unwrap_or(0);
        println!("profile step CC 85 {profile} at phase {phase} ms: the busiest pass {pass}");
        worst = worst.max(pass);
    }
    println!("profile step with the DACs sweeping: the busiest pass {worst}");
    assert!(
        worst <= common::POLL_INSTRUCTIONS,
        "a pass took {worst} instructions, over the poll allowance"
    );
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
        let mut composes = Composes::new(&mut m, &fw);
        let t0 = m.ms();
        loop {
            composes.run(&mut m, &fw, UI_PERIOD_MS);
            if !showing_card(&mut m, &fw) && !common::screen_animating(&mut m, &fw) {
                break;
            }
            assert!(
                m.ms() < t0 + CARD_TIMEOUT_MS,
                "THE {name}: the card did not end"
            );
        }
        let frames = composes.end(&mut m, &fw);
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

/// The song views' state (app/song_view.h) inside the views.
fn song_byte(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    ui_byte(m, fw, sidou_sys::ilp32::views::song + field)
}

/// The instrument's voices gated (app/live.h live.voice).
fn live_gated(m: &mut Machine, fw: &Firmware) -> usize {
    let l = m
        .soc
        .read32(fw.symbol("ui") + sidou_sys::ilp32::views::l as u32);
    (0..3u32)
        .filter(|v| {
            let at = sidou_sys::ilp32::live::voice as u32
                + v * sidou_sys::ilp32::live_voice::SIZE as u32
                + sidou_sys::ilp32::live_voice::gate as u32;
            m.soc.read8(l + at) != 0
        })
        .count()
}

/// TRACKER from SEQ, settled.
fn open_tracker(m: &mut Machine, fw: &Firmware) {
    press(m, SEQ);
    settle(m, fw);
    assert_eq!(
        common::top_view(m, fw),
        fw.symbol("tracker_view_view"),
        "SEQ did not open the song views"
    );
}

/// SEQ opens TRACKER: stopped, SIDOU LOOP's patterns and tempo; PLAY plays it (the play mark,
/// the position, the waveforms and meters), REC in the header; GLO and the leftmost key mute
/// voice 1 within the key's pass and sound nothing, and LIVE's header says so.
#[test]
fn the_song_views_header_and_heads_match_their_goldens() {
    let Some((build, fw)) = fixture("the song views test") else {
        return;
    };
    let mut m = booted(&fw);
    open_tracker(&mut m, &fw);
    compare_golden(&build, "tracker", &m.soc.lcd.fb, &[]);
    press(&mut m, PLAY);
    m.run_ms(2_000);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "tracker-play", &m.soc.lcd.fb, &[]);
    press(&mut m, REC);
    assert_eq!(song_byte(&mut m, &fw, sidou_sys::ilp32::song_view::rec), 1);
    // GLO and the leftmost key: voice 1 muted at once, shown at the next frame, no sound.
    m.key(GLO, true);
    m.run_ms(30);
    common::to_mid_frame(&mut m, &fw);
    m.key(KEY0, true);
    m.run_ms(10);
    assert_eq!(
        song_byte(&mut m, &fw, sidou_sys::ilp32::song_view::mute),
        1,
        "the mute within the key's pass"
    );
    m.run_ms(UI_PERIOD_MS);
    assert_eq!(live_gated(&mut m, &fw), 0, "a key under GLO sounds nothing");
    m.key(KEY0, false);
    m.key(GLO, false);
    m.run_ms(200);
    common::to_mid_frame(&mut m, &fw);
    assert_eq!(live_gated(&mut m, &fw), 0);
    compare_golden(&build, "tracker-mute-rec", &m.soc.lcd.fb, &[]);
    // Stopped, back in LIVE: its header, in the preset's place.
    press(&mut m, PLAY);
    press(&mut m, HOME);
    settle(&mut m, &fw);
    assert_eq!(common::top_view(&mut m, &fw), fw.symbol("live_view_view"));
    compare_golden(&build, "live-mutes", &m.soc.lcd.fb, &[]);
    // The keys play the instrument again once GLO is let go.
    m.key(KEY0, true);
    m.run_ms(30);
    assert_eq!(live_gated(&mut m, &fw), 1);
    m.key(KEY0, false);
    m.run_ms(30);
}

/// TRACKER's main-loop costs (M2d Task 2), with SIDOU LOOP bound, bounded about 10 % above
/// what was measured:
///
/// | TRACKER | Cells a frame (average, most) | Worst SPI | Instructions a frame | Busiest ui_poll |
/// |---|---|---|---|---|
/// | Idle (25 frames) | 0, 0 | 0 | 8.1k | 1.3k |
/// | REC on (25 frames) | 0.12, 3 | 0.26 ms | 8.3k | 3.9k |
/// | Playing, BAR (100 frames) | 21.9, 86 | 7.56 ms | 25.7k | 29.8k |
/// | GLO and a key: MUT (25 frames) | 1.24, 31 | 2.71 ms | 9.8k | 27.4k |
///
/// Idle it composes nothing. The header and the heads are built again when they change (REC,
/// the meters), the rows when the window, the bar, a pattern or a mute does (a row step: the
/// build reads 25 rows of three patterns), and only the rows built are compared and copied.
/// The inputs (ui_buttons for SEQ, REC and PLAY; GLO's key goes through the note keys' pass)
/// and the busiest pass after each, against TRACKER_INPUT and TRACKER_PASS. Measured: SEQ from
/// LIVE the input 4.2k (the push: the wipe's set-up over LIVE's overlays) and the pass 48.0k
/// (the wipe's first frame: the whole canvas built and its rows copied whole); PLAY the input
/// 1.9k (song_ctl_play) and the pass 29.6k; SEQ in TRACKER and REC the input 0.2k and the pass
/// 5.0k to 30.4k; GLO's key 29.7k. The playing costs over 30 s, both modes and two tempos, are
/// the_tracker_stays_within_its_budgets_while_a_song_plays's.
const TRACKER_IDLE_PER_FRAME: f64 = 8_900.0;
const TRACKER_CHANGE_PER_FRAME: f64 = 10_800.0;
const TRACKER_PLAYING_PER_FRAME: f64 = 28_000.0;
const TRACKER_WORST_POLL: u64 = 33_000;
const TRACKER_INPUT: u64 = 4_600;
const TRACKER_PASS: u64 = 55_000;

#[test]
fn the_song_views_stay_within_their_budgets() {
    let Some((_, fw)) = fixture("the song views budget test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    common::run_past_boot(&mut m, &fw);
    open_tracker(&mut m, &fw);
    let idle = measure(&mut m, &fw, 25, |_| {});
    let rec = measure(&mut m, &fw, 25, |m| m.key(REC, true));
    m.key(REC, false);
    let playing = measure(&mut m, &fw, 100, |m| m.key(PLAY, true));
    m.key(PLAY, false);
    m.run_ms(100);
    m.key(PLAY, true);
    m.run_ms(30);
    m.key(PLAY, false);
    m.run_ms(500);
    m.key(GLO, true);
    m.run_ms(100);
    let glo = measure(&mut m, &fw, 25, |m| m.key(KEY0, true));
    m.key(KEY0, false);
    m.key(GLO, false);
    let report = serde_json::json!({
        "tracker_idle": animated_json(&idle),
        "tracker_rec": animated_json(&rec),
        "tracker_playing": animated_json(&playing),
        "tracker_glo": animated_json(&glo),
    });
    println!("{report}");
    assert_eq!(idle.total().windows, 0, "the idle TRACKER sent something");
    for (what, c, per_frame) in [
        ("idle", &idle, TRACKER_IDLE_PER_FRAME),
        ("REC", &rec, TRACKER_CHANGE_PER_FRAME),
        ("playing", &playing, TRACKER_PLAYING_PER_FRAME),
        ("GLO and a key", &glo, TRACKER_CHANGE_PER_FRAME),
    ] {
        assert!(
            c.worst_frame_ms() <= SPI_BUDGET_MS,
            "{what}: a frame took {:.1} ms of SPI",
            c.worst_frame_ms()
        );
        let most = c.frames.iter().map(cells).max().unwrap_or(0);
        assert!(
            most <= sidou_sys::SONG_VIEW_FRAME_CELLS,
            "{what}: a frame sent {most} cells"
        );
        assert!(
            c.worst_poll < TRACKER_WORST_POLL,
            "{what}: a ui_poll took {} instructions",
            c.worst_poll
        );
        assert!(
            c.instructions_per_frame() < per_frame,
            "{what}: {:.0} instructions a frame",
            c.instructions_per_frame()
        );
    }

    // The inputs and the busiest pass after each.
    let mut m = booted(&fw);
    m.watch_calls(fw.symbol("ui_buttons"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut over = Vec::new();
    let mut costs = Vec::new();
    for (what, id) in [
        ("SEQ from LIVE", SEQ),
        ("SEQ in TRACKER", SEQ),
        ("REC", REC),
        ("PLAY", PLAY),
        ("REC again", REC),
    ] {
        let c = start_cost(&mut m, 1000, |m| press(m, id));
        costs.push((what, c));
    }
    // GLO's key: the input is the note keys' pass (ui_note_keys is inlined into input_poll's
    // caller's pass), so the pass alone.
    m.key(GLO, true);
    m.run_ms(30);
    let c = start_cost(&mut m, 1000, |m| {
        m.key(KEY0, true);
        m.run_ms(30);
        m.key(KEY0, false);
    });
    costs.push(("GLO and a key", c));
    m.key(GLO, false);
    for (what, (input, pass)) in &costs {
        println!("TRACKER {what}: the input {input}, the busiest pass {pass}");
        if *input >= TRACKER_INPUT || *pass >= TRACKER_PASS {
            over.push(format!(
                "{what}: the input {input}, the busiest pass {pass}"
            ));
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}

/// A song at tempo 3 (a row every 3 calls, 251 BPM): every voice a note on every row of a
/// 64-row pattern, voice 1's first row setting the tempo.
fn tempo_3_song() -> sidou_tests::song::Song {
    let mut s = sidou_tests::song::SngSpec::new();
    let mut ins = [0u8; 25];
    ins[..9].copy_from_slice(&[0x09, 0x00, 1, 0, 0, 0, 0, 2, 0x09]);
    s.instr = vec![ins];
    s.tables = vec![vec![(0x21, 0x00), (0xff, 0x00)], vec![], vec![], vec![]];
    s.patterns = (0..3u8)
        .map(|v| {
            let rows = (0..64u8)
                .map(|r| {
                    let mut c = [0x60 + 24 + v * 12 + (r * 5) % 12, 1, 0, 0];
                    if v == 0 && r == 0 {
                        c[2] = 0xf;
                        c[3] = 0x03;
                    }
                    c
                })
                .collect();
            (rows, [0xff, 0, 0, 0])
        })
        .collect();
    common::device_song(&s.encode(), None)
}

/// TRACKER playing for `ms`: the song (None: SIDOU LOOP) injected, BAR or SCROLL (the views'
/// TRACKER ROWS, set in RAM: the menu's path is the host tests'), SEQ, PLAY; the frames'
/// costs and the frames whose copy the budget cut short (song_view.held).
fn tracker_run(
    fw: &Firmware,
    song: Option<&sidou_tests::song::Song>,
    scroll: bool,
    ms: u64,
) -> (Cost, u32) {
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    common::run_past_boot(&mut m, fw);
    if let Some(s) = song {
        let base = fw.symbol("__song_region");
        for (i, &b) in s.bytes().iter().enumerate() {
            m.soc.write8(base + i as u32, b);
        }
    }
    let views = fw.symbol("ui");
    m.soc.write8(
        views + (sidou_sys::ilp32::views::song + sidou_sys::ilp32::song_view::scroll) as u32,
        u8::from(scroll),
    );
    open_tracker(&mut m, fw);
    press(&mut m, PLAY);
    m.run_ms(500);
    let held_at =
        views + (sidou_sys::ilp32::views::song + sidou_sys::ilp32::song_view::held) as u32;
    let held = m.soc.read32(held_at);
    let c = measure(&mut m, fw, ms / UI_PERIOD_MS, |_| {});
    (c, m.soc.read32(held_at) - held)
}

/// TRACKER in SCROLL (the views' TRACKER ROWS set in RAM), SIDOU LOOP two seconds in: the
/// playing row on screen row 14, the rows above and below it.
#[test]
fn the_tracker_scrolling_matches_its_golden_image() {
    let Some((build, fw)) = fixture("the TRACKER scroll test") else {
        return;
    };
    let mut m = booted(&fw);
    m.soc.write8(
        fw.symbol("ui")
            + (sidou_sys::ilp32::views::song + sidou_sys::ilp32::song_view::scroll) as u32,
        1,
    );
    open_tracker(&mut m, &fw);
    press(&mut m, PLAY);
    m.run_ms(2_000);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "tracker-scroll", &m.soc.lcd.fb, &[]);
}

#[test]
fn applied_song_edits_refresh_the_tracker_before_and_after_playback() {
    let Some((_, fw)) = fixture("the TRACKER edit refresh test") else {
        return;
    };
    let song = tempo_3_song();
    for played in [false, true] {
        let mut m = booted(&fw);
        let base = fw.symbol("__song_region");
        for (i, &byte) in song.bytes().iter().enumerate() {
            m.soc.write8(base + i as u32, byte);
        }
        open_tracker(&mut m, &fw);
        if played {
            press(&mut m, PLAY);
            m.run_ms(500);
            press(&mut m, PLAY);
            m.run_ms(500);
        }
        let hook = fw.symbol("song_test_hook");
        m.soc.write32(hook + 4, 0); // pattern 0
        m.soc.write32(hook + 8, 0); // row 0
        for (i, byte) in [0x60 + 4 * 12 + 2, 1, 4, 0x22].into_iter().enumerate() {
            m.soc.write8(hook + 12 + i as u32, byte);
        }
        m.soc.write32(hook, 1);
        m.run_ms(200);
        assert_eq!(m.soc.read32(hook), 0, "the edit was taken");
        let cells = fw.symbol("cells") + sidou_sys::ilp32::grid::ch as u32;
        let note: Vec<u8> = (0..8)
            .map(|i| m.soc.read8(cells + 6 * 30 + 3 + i))
            .collect();
        assert_eq!(note, b"D-401422", "played first: {played}");
    }
}

/// TRACKER's costs while a song plays, 30 s each (docs/art/README.md "TRACKER"):
///
/// | 30 s playing | Cells a frame (average, most) | SPI a frame (average, worst) | Instructions a frame | Busiest ui_poll | Frames at the cap |
/// |---|---|---|---|---|---|
/// | SIDOU LOOP (tempo 6), BAR | 25.6, 129 | 2.22, 11.35 ms | 27.6k | 31.4k | 0 of 750 |
/// | SIDOU LOOP, SCROLL | 83.4, 200 | 7.30, 17.56 ms | 51.0k | 34.0k | 256 |
/// | tempo_3_song, BAR | 61.6, 200 | 5.35, 17.79 ms | 49.8k | 38.4k | 38 (page turns) |
/// | tempo_3_song, SCROLL | 100.4, 200 | 9.08, 17.58 ms | 89.9k | 36.8k | 24 |
///
/// The bar keeps every frame within SONG_VIEW_FRAME_CELLS and the SPI budget; SCROLL holds
/// to the cells as well, and what does not fit waits (the frames cut short: its lag).
const TRACKER_BAR_PER_FRAME: f64 = 55_000.0;
const TRACKER_SCROLL_PER_FRAME: f64 = 99_000.0;
const TRACKER_PLAY_WORST_POLL: u64 = 42_000;

#[test]
fn the_tracker_stays_within_its_budgets_while_a_song_plays() {
    let Some((_, fw)) = fixture("the TRACKER playing budget test") else {
        return;
    };
    let fast = tempo_3_song();
    let mut report = serde_json::Map::new();
    let mut over = Vec::new();
    for (name, song, scroll) in [
        ("loop_bar", None, false),
        ("loop_scroll", None, true),
        ("tempo3_bar", Some(&fast), false),
        ("tempo3_scroll", Some(&fast), true),
    ] {
        let (c, held) = tracker_run(&fw, song, scroll, 30_000);
        let per: Vec<u32> = c.frames.iter().map(cells).collect();
        let most = per.iter().copied().max().unwrap_or(0);
        let avg = f64::from(per.iter().sum::<u32>()) / per.len() as f64;
        let full = per
            .iter()
            .filter(|&&n| n >= sidou_sys::SONG_VIEW_FRAME_CELLS - 10)
            .count();
        println!(
            "TRACKER {name}: cells a frame {avg:.1} (most {most}), SPI {:.2} ms a frame (worst \
             {:.2}), {:.1}k instructions a frame, busiest ui_poll {:.1}k, {held} of {} frames cut \
             short, {full} at the cap",
            c.total().spi_ms() / per.len() as f64,
            c.worst_frame_ms(),
            c.instructions_per_frame() / 1000.0,
            c.worst_poll as f64 / 1000.0,
            per.len()
        );
        let mut j = animated_json(&c);
        j["frames_cut_short"] = serde_json::json!(held);
        j["frames_at_the_cap"] = serde_json::json!(full);
        j.as_object_mut().unwrap().remove("per_frame");
        report.insert(name.into(), j);
        if most > sidou_sys::SONG_VIEW_FRAME_CELLS || c.worst_frame_ms() > SPI_BUDGET_MS {
            over.push(format!(
                "{name}: {most} cells, {:.1} ms",
                c.worst_frame_ms()
            ));
        }
        let per_frame = if scroll {
            TRACKER_SCROLL_PER_FRAME
        } else {
            TRACKER_BAR_PER_FRAME
        };
        if c.instructions_per_frame() >= per_frame || c.worst_poll >= TRACKER_PLAY_WORST_POLL {
            over.push(format!(
                "{name}: {:.0} instructions a frame, a ui_poll {}",
                c.instructions_per_frame(),
                c.worst_poll
            ));
        }
    }
    println!("{}", serde_json::Value::Object(report));
    assert!(over.is_empty(), "{over:#?}");
}

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
const _: () = assert!(TRACKER_WORST_POLL <= common::POLL_INSTRUCTIONS);
const _: () = assert!(TRACKER_PLAY_WORST_POLL <= common::POLL_INSTRUCTIONS);
const _: () = assert!(TRACKER_INPUT + TRACKER_PASS <= common::POLL_INSTRUCTIONS);
// A pass that runs a slice of the spare's preparation (reg_ring_prepare_step) and the busiest
// view's work still fits.
// One slice a pass, however many ticks a pass catches up (reg_ring_pass_end, platform/fm1/
// main.c TICK_CATCH_UP), and no ramp table step in it.
const PREPARE_SLICES_PER_PASS: u64 = 1;
const _: () = assert!(
    LIVE_WORST_POLL + PREPARE_SLICES_PER_PASS * REG_RING_PREPARE_SLICE <= common::POLL_INSTRUCTIONS
);
const _: () = assert!(
    CHIP_INPUT + CHIP_PASS + PREPARE_SLICES_PER_PASS * REG_RING_PREPARE_SLICE
        <= common::POLL_INSTRUCTIONS
);
const _: () = assert!(
    TRACKER_INPUT + TRACKER_PASS + PREPARE_SLICES_PER_PASS * REG_RING_PREPARE_SLICE
        <= common::POLL_INSTRUCTIONS
);

// ---- the UI sounds (M2b Task 7, app/ui_sound.h) ----

/// platform/fm1/audio.h AUDIO_B_UI: chip B's in-use bit for the UI sounds.
const AUDIO_B_UI: u8 = 2;

/// From LIVE: the menu, SOUNDS turned on or left off, then the cursor moved down one item and
/// back (two blips with SOUNDS on); with `held`, three notes held on chip B first (a song's
/// hook on: the keys play chip B). The audio from just before the moves, and whether chip B
/// was in use for a sound (its b_want's UI bit) at any 5 ms along them.
fn ui_sounds_run(fw: &Firmware, sounds: bool, held: bool) -> (Vec<i16>, bool) {
    let mut m = booted(fw);
    if held {
        m.soc.write8(
            fw.symbol("keys_router") + sidou_sys::ilp32::keys_route::song as u32,
            1,
        );
        m.run_ms(100);
        for k in CHORD {
            m.key(k, true);
            m.run_ms(CHORD_STAGGER_MS);
        }
    }
    open_menu(&mut m, fw);
    select(&mut m, sidou_sys::VIEWS_MENU_SOUNDS as i32);
    if sounds {
        m.turn(ALGORITHM, 1);
    }
    m.run_ms(800); // the confirm's sound and its release
    let from = m.soc.periph.audio.len();
    let mut ui_use = false;
    let b_want = fw.symbol("b_want");
    for step in [1, -1] {
        m.turn(SELECT, step);
        for _ in 0..60 {
            m.run_ms(5);
            ui_use |= m.soc.read8(b_want) & AUDIO_B_UI != 0;
        }
    }
    (m.soc.periph.audio[from..].to_vec(), ui_use)
}

/// With SOUNDS on and nothing held, a menu move sounds on chip B: chip B is in use for it, and
/// the output has the blip's energy (C-6, 1,047 Hz, on a triangle) where the same run with
/// SOUNDS off has none; with SOUNDS off chip B is never in use for a sound.
#[test]
fn with_sounds_on_a_menu_move_blips_and_off_it_is_silent() {
    let Some((_, fw)) = fixture("the UI sounds test") else {
        return;
    };
    let (on, on_use) = ui_sounds_run(&fw, true, false);
    let (off, off_use) = ui_sounds_run(&fw, false, false);
    assert!(on_use, "chip B never in use for the blips");
    assert!(!off_use, "chip B in use for a sound with SOUNDS off");
    let left = |v: &[i16]| {
        v.iter()
            .step_by(2)
            .map(|&s| f32::from(s))
            .collect::<Vec<f32>>()
    };
    let band = |x: &[f32]| {
        // The DFT's power at the blip's pitch over the first 4096 samples of the move.
        let n = 4096.min(x.len());
        let w = 2.0 * std::f64::consts::PI * 1046.5 / common::FS;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &s) in x[..n].iter().enumerate() {
            re += f64::from(s) * (w * i as f64).cos();
            im -= f64::from(s) * (w * i as f64).sin();
        }
        (re * re + im * im).sqrt() / n as f64
    };
    let (e_on, e_off) = (band(&left(&on)), band(&left(&off)));
    println!("UI sounds: the blip's band {e_on:.1} with SOUNDS on, {e_off:.1} off");
    assert!(
        e_on > 20.0 * e_off.max(0.5),
        "no blip: {e_on:.1} against {e_off:.1}"
    );
}

/// With three notes held on chip B (the keys there, a song's hook on), a menu move with SOUNDS
/// on finds no free voice: it does not sound, and the output is the same run's with SOUNDS
/// off, sample for sample (the notes untouched).
#[test]
fn with_three_notes_held_on_chip_b_a_menu_move_is_silent_and_bit_exact() {
    let Some((_, fw)) = fixture("the UI sounds test") else {
        return;
    };
    let (on, on_use) = ui_sounds_run(&fw, true, true);
    let (off, _) = ui_sounds_run(&fw, false, true);
    assert!(!on_use, "chip B in use for a sound with every voice held");
    assert!(
        on.iter().any(|&v| v.abs() > 500),
        "the chord does not sound"
    );
    assert!(on == off, "the held notes changed with SOUNDS on");
}

// A pass that runs a slice of the song's analysis or fast-forward (song_ctl_poll, M2c Task 6:
// never in a pass that ran a chip preparation's slice, platform/fm1/audio.c audio_song_poll)
// and the busiest view's work still fits; song.rs measures the slices against
// SONG_SLICE_INSTRUCTIONS.
const SONG_SLICE: u64 = sidou_sys::SONG_SLICE_INSTRUCTIONS as u64;
const _: () = assert!(LIVE_WORST_POLL + SONG_SLICE <= common::POLL_INSTRUCTIONS);
const _: () = assert!(CHIP_INPUT + CHIP_PASS + SONG_SLICE <= common::POLL_INSTRUCTIONS);
const _: () = assert!(SCENE_INPUT + SCENE_START_PASS + SONG_SLICE <= common::POLL_INSTRUCTIONS);
const _: () = assert!(RMR_INPUT + RMR_START_PASS + SONG_SLICE <= common::POLL_INSTRUCTIONS);
const _: () = assert!(TITLE_CARD_INPUT + TITLE_CARD_PASS + SONG_SLICE <= common::POLL_INSTRUCTIONS);
const _: () = assert!(WIPE_WORST_POLL + SONG_SLICE <= common::POLL_INSTRUCTIONS);
const _: () = assert!(ABOUT_WORST_POLL + SONG_SLICE <= common::POLL_INSTRUCTIONS);
const _: () = assert!(TRACKER_INPUT + TRACKER_PASS + SONG_SLICE <= common::POLL_INSTRUCTIONS);

// ---- RED MOON RITE (app/scene_rmr.h) playing the demo song ----

const SCENE_RMR: usize = sidou_sys::SCENE_RMR as usize;

/// A byte of the views' SCENE (app/scene_view.h) at `offset` in the device's layout.
fn scene_byte(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    ui_byte(m, fw, sidou_sys::ilp32::views::scene + offset)
}

/// A byte of RED MOON RITE's state (app/scene_rmr.h scene_rmr) while it runs.
fn rmr_byte(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    use sidou_sys::ilp32::{scene_view, scene_view_rite};
    scene_byte(m, fw, scene_view::u + scene_view_rite::rmr + field)
}

/// The song transport's state (core/player/song_ctl.h SONG_*).
fn song_state(m: &mut Machine, fw: &Firmware) -> u32 {
    u32::from(
        m.soc
            .read8(fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::state as u32),
    )
}

/// The screen's toast text (app/screen.h).
fn toast_text(m: &mut Machine, fw: &Firmware) -> String {
    let at =
        fw.symbol("ui") + (sidou_sys::ilp32::views::s + sidou_sys::ilp32::screen::toast) as u32;
    (0..sidou_sys::SCREEN_TOAST_CHARS)
        .map(|i| m.soc.read8(at + i))
        .take_while(|&b| b != 0)
        .map(char::from)
        .collect()
}

/// SCENE on RED MOON RITE (SELECT back from FIRE), its toast gone, then PLAY until the song
/// plays; on to half way between two frames.
fn rmr_with_the_song(m: &mut Machine, fw: &Firmware) {
    open_scene(m, fw, 0);
    m.turn(SELECT, -1);
    m.run_ms(100);
    assert_eq!(
        usize::from(scene_byte(m, fw, sidou_sys::ilp32::scene_view::which)),
        SCENE_RMR
    );
    assert_eq!(toast_text(m, fw), "RED MOON RITE", "SELECT's toast");
    m.run_ms(1600);
    press(m, PLAY);
    for _ in 0..100 {
        if song_state(m, fw) == sidou_sys::SONG_PLAYING {
            break;
        }
        m.run_ms(10);
    }
    assert_eq!(
        song_state(m, fw),
        sidou_sys::SONG_PLAYING,
        "PLAY did not play the song"
    );
    common::to_mid_frame(m, fw);
}

/// Frames until `ok`, at most `frames`: the frame count it took.
fn frames_until(
    m: &mut Machine,
    fw: &Firmware,
    frames: u32,
    mut ok: impl FnMut(&mut Machine, &Firmware) -> bool,
) -> Option<u32> {
    for f in 0..frames {
        if ok(m, fw) {
            return Some(f);
        }
        m.run_ms(UI_PERIOD_MS);
    }
    None
}

/// The demo song plays the duel: a second in, the fill's spinning attack landing (the sword
/// knocked from his hand) and the sword stuck in the floor, each as its golden image.
#[test]
fn red_moon_rite_plays_the_demo_song_as_its_golden_images() {
    use sidou_sys::ilp32::scene_rmr as r;
    let Some((build, fw)) = fixture("the RED MOON RITE test") else {
        return;
    };
    let mut m = booted(&fw);
    rmr_with_the_song(&mut m, &fw);
    m.run_ms(1000);
    common::to_mid_frame(&mut m, &fw);
    compare_golden(&build, "scene-rmr-song", &m.soc.lcd.fb, &[]);
    // The demo song's fourth bar ends in the fill: about 8 s at 6 frames a row.
    let landed = frames_until(&mut m, &fw, 600, |m, fw| {
        rmr_byte(m, fw, r::sword) != 0 && rmr_byte(m, fw, r::sword_n) <= 2
    });
    assert!(landed.is_some(), "no disarm in 24 s of the song");
    compare_golden(&build, "scene-rmr-disarm", &m.soc.lcd.fb, &[]);
    let stuck = frames_until(&mut m, &fw, 100, |m, fw| {
        let u = rmr_byte(m, fw, r::unarmed);
        rmr_byte(m, fw, r::sword_n) >= 20
            && u < 2
            && u32::from(rmr_byte(m, fw, r::pose + usize::from(u))) != sidou_sys::SCENE_RMR_REACH
    });
    assert!(stuck.is_some(), "the sword never stuck");
    m.run_ms(4 * UI_PERIOD_MS);
    let unarmed = rmr_byte(&mut m, &fw, r::unarmed);
    assert!(
        rmr_byte(&mut m, &fw, r::sword) != 0 && unarmed < 2,
        "the stuck-sword golden must precede the sword's recovery"
    );
    compare_golden(&build, "scene-rmr-stuck", &m.soc.lcd.fb, &[]);
    println!("the disarm {landed:?} frames in, stuck {stuck:?} frames after");
}

/// What the duel costs over 60 s of the demo song: the tiles (cells) a frame against the
/// prototype's 117 on average and 176 at most, SPI a frame, the main loop's instructions a
/// frame, the busiest ui_poll and main-loop pass; then, on runs of 20 s each (fm1-emu is
/// deterministic), the busiest call of each kind a ui_poll makes for it: a scan slice
/// (scene_view_idle) and a run rendered (grid_render_run, two a ui_poll at most), and the
/// costliest tile (scene_px_tile) of the vignette's own frames.
#[test]
fn red_moon_rite_stays_within_its_budgets_over_a_minute_of_the_song() {
    let Some((_, fw)) = fixture("the RED MOON RITE budget test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    common::run_past_boot(&mut m, &fw);
    rmr_with_the_song(&mut m, &fw);
    let passes = m.passes.len();
    let c = measure_frames(&mut m, &fw, 1500, |_, _| {});
    let pass = m.passes[passes..].iter().copied().max().unwrap_or(0);
    let per = cells_sent(&c);
    let avg = f64::from(per.iter().sum::<u32>()) / per.len() as f64;
    let most = per.iter().copied().max().unwrap_or(0);
    let busiest = |f: &str| {
        let mut m = Machine::boot(&fw.image);
        m.watch_calls(fw.symbol(f));
        common::run_past_boot(&mut m, &fw);
        rmr_with_the_song(&mut m, &fw);
        let from = m.watched_calls.len();
        m.run_ms(20_000);
        let calls = &m.watched_calls[from..];
        let mean = calls.iter().sum::<u64>() as f64 / calls.len().max(1) as f64;
        (calls.iter().copied().max().unwrap_or(0), mean, calls.len())
    };
    let (slice, slice_mean, slices) = busiest("scene_view_idle");
    let (run, run_mean, runs) = busiest("grid_render_run");
    let (tile, tile_mean, tiles) = busiest("scene_px_tile");
    println!(
        "{}",
        serde_json::json!({
            "frames": per.len(),
            "cells_a_frame": avg,
            "cells_most": most,
            "spi_ms_a_frame": c.total().spi_ms() / per.len() as f64,
            "spi_ms_worst": c.worst_frame_ms(),
            "instructions_a_frame": c.instructions_per_frame(),
            "busiest_ui_poll": c.worst_poll,
            "busiest_pass": pass,
            "scan_slices_in_20_s": slices, "scan_slice_mean": slice_mean,
            "busiest_scan_slice": slice,
            "runs_in_20_s": runs, "run_mean": run_mean, "busiest_run": run,
            "tiles_in_20_s": tiles, "tile_mean": tile_mean, "busiest_tile": tile,
        })
    );
    assert!(
        c.worst_frame_ms() <= SPI_BUDGET_MS,
        "a frame took {:.1} ms of SPI",
        c.worst_frame_ms()
    );
    assert!(
        most <= sidou_sys::SCENE_VIEW_FRAME_CELLS,
        "a frame sent {most} cells"
    );
    assert!(avg <= RMR_CELLS_AVERAGE, "{avg:.1} cells a frame");
    assert!(most <= RMR_CELLS_MOST, "{most} cells in a frame");
    assert!(
        c.instructions_per_frame() <= RMR_PER_FRAME,
        "{:.0} instructions a frame",
        c.instructions_per_frame()
    );
    assert!(
        c.worst_poll <= RMR_WORST_POLL,
        "a ui_poll took {}",
        c.worst_poll
    );
    assert!(pass <= RMR_WORST_PASS, "a main-loop pass took {pass}");
    assert!(slice <= RMR_WORST_SLICE, "a scan slice took {slice}");
    assert!(run <= RMR_WORST_RUN, "a run took {run}");
    assert!(tile <= RMR_WORST_TILE, "a tile took {tile}");
}

/// The duel's bounds, about 10 % over what M2f Task 4 measured over a minute of the demo song
/// (red_moon_rite_stays_within_its_budgets_over_a_minute_of_the_song), the plan's 180 cells at
/// most: measured, 98.8 cells a frame on average and 170 at most (the prototype's 117 and 176),
/// 8.7 ms of SPI a frame and 15.0 at worst, 904k instructions a frame, the busiest ui_poll
/// 56.4k, pass 57.5k, scan slice 56.3k and run 24.6k, the costliest tile 5,930. That tile is
/// over the bench's (scene_budget.rs TILE_WORST 5,300: two fighters, the spark and the arc over
/// the arena), so the run and slice caps came down to 4 and 8 (app/scene_px.h) for two runs, or
/// a slice, and a song's slice to fit a pass.
const RMR_CELLS_AVERAGE: f64 = 109.0;
const RMR_CELLS_MOST: u32 = 180;
const RMR_PER_FRAME: f64 = 1_000_000.0;
const RMR_WORST_POLL: u64 = 62_000;
const RMR_WORST_PASS: u64 = 63_000;
const RMR_WORST_SLICE: u64 = 62_000;
const RMR_WORST_RUN: u64 = 27_000;
const RMR_WORST_TILE: u64 = 6_500;
// A pass that runs RED MOON RITE's busiest ui_poll may run a song's analysis slice too.
const _: () = assert!(RMR_WORST_POLL + SONG_SLICE <= common::POLL_INSTRUCTIONS);
const _: () = assert!(RMR_WORST_PASS + SONG_SLICE <= common::POLL_INSTRUCTIONS);
// A ui_poll renders two runs, or scans a slice: either within its bound.
const _: () = assert!(2 * RMR_WORST_RUN <= RMR_WORST_POLL && RMR_WORST_SLICE <= RMR_WORST_POLL);

/// A muted drum voice stops the chops and parries; the bass and the arp go on moving it.
#[test]
fn a_muted_drum_voice_stops_the_chops_and_parries() {
    use sidou_sys::ilp32::scene_rmr as r;
    let Some((_, fw)) = fixture("the RED MOON RITE mute test") else {
        return;
    };
    let mut m = booted(&fw);
    rmr_with_the_song(&mut m, &fw);
    let attacks = |m: &mut Machine, fw: &Firmware, frames: u32| {
        let mut n = 0;
        for _ in 0..frames {
            for k in 0..2 {
                let p = u32::from(rmr_byte(m, fw, r::pose + k));
                n += u32::from(
                    p == sidou_sys::SCENE_RMR_RAISE
                        || p == sidou_sys::SCENE_RMR_CHOP
                        || p == sidou_sys::SCENE_RMR_BLOCK,
                );
            }
            m.run_ms(UI_PERIOD_MS);
        }
        n
    };
    let open = attacks(&mut m, &fw, 150);
    // Voice 3, the drums: the interrupt's mute mask (song_ctl.mute), as a mute command sets it.
    let mute = fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::mute as u32;
    m.soc.write8(mute, 1 << 2);
    m.run_ms(500);
    let muted = attacks(&mut m, &fw, 150);
    println!("frames in an attack or a parry over 6 s: {open} with the drums, {muted} muted");
    assert!(open > 20, "{open}: the drums did not move the duel");
    assert_eq!(muted, 0, "the muted drums still move the duel");
}

/// Once the song stops and its releases have rung out, the duel calms: nothing is sent.
#[test]
fn red_moon_rite_calms_when_the_song_stops() {
    let Some((_, fw)) = fixture("the RED MOON RITE calm test") else {
        return;
    };
    let mut m = booted(&fw);
    rmr_with_the_song(&mut m, &fw);
    let playing = cells_sent(&measure(&mut m, &fw, 50, |_| {}));
    press(&mut m, PLAY);
    m.run_ms(3000);
    let after = cells_sent(&measure(&mut m, &fw, 50, |_| {}));
    let sum = |v: &[u32]| v.iter().sum::<u32>();
    println!(
        "cells in 2 s: playing {}, after the stop {}",
        sum(&playing),
        sum(&after)
    );
    assert!(sum(&playing) > 1000);
    assert_eq!(sum(&after), 0, "{after:?}");
}
