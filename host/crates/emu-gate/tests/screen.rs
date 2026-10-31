// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::path::{Path, PathBuf};

use common::panel::{
    holding, press, select_menu_item, turn_time, ALGORITHM, ARP, EDIT, ENV, FX, GLO, HOME, K1, K4,
    KEY0, LFO, MENU_ITEMS, OCT_UP, PLAY, PRESETS, REC, SAVE, SCL, SELECT, SEQ,
};
use common::{
    booted, open_status, run_past_boot, screen_animating, screen_wiping, settings_item, tap_on,
    to_mid_frame, top_view, Firmware, Machine, POLL_INSTRUCTIONS, UI_FRAME_MS,
};
use fm1_core::Bus;

const W: usize = 240;
const CELL_PX: usize = 8;
const GRID_COLS: usize = W / CELL_PX;
const GRID_ROWS: usize = W / CELL_PX;
const SPI_HZ: f64 = 12e6;
const SPI_BUDGET_MS: f64 = 20.0;
const FRAME_CELLS_MOST: u32 = sidou_sys::LIVE_VIEW_FRAME_CELLS;
const K2: u8 = K1 + 1;
const K3: u8 = K1 + 2;
const LOWEST_KEY_NOTE: u8 = 53;

const fn note_key(midi_note: u8) -> u8 {
    KEY0 + midi_note - LOWEST_KEY_NOTE
}

const A4_KEY: u8 = note_key(69);
const MENU_CHIP: i32 = sidou_sys::VIEWS_MENU_CHIP as i32;
const MENU_SCENE: i32 = sidou_sys::VIEWS_MENU_SCENE as i32;
const MENU_CHARSET: i32 = sidou_sys::VIEWS_MENU_CHARSET as i32;
const MENU_ABOUT: i32 = sidou_sys::VIEWS_MENU_ABOUT as i32;
const MENU_MIDI: i32 = sidou_sys::VIEWS_MENU_MIDI as i32;
const MENU_SONG: i32 = sidou_sys::VIEWS_MENU_SONG as i32;
const TOAST_ROW: usize = sidou_sys::SCREEN_TOAST_ROW as usize;
const TOAST_SHOWN_MS: u64 = sidou_sys::SCREEN_TOAST_MS as u64;
const TOAST_GONE_MS: u64 = 1_600;
const POPUP_CLOSED_MS: u64 = 1_500;
const MODEL_8580_CC: [u8; 3] = [0xB0, 25, 127];
const NOISE_AT_FULL_SUSTAIN: [u8; 6] = [0xB0, 20, 64, 0xB0, 79, 127];
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
const BLACK: u8 = sidou_sys::GRID_BLACK as u8;
const WHITE: u8 = sidou_sys::GRID_WHITE as u8;
const BLUE: u8 = sidou_sys::GRID_BLUE as u8;
const YELLOW: u8 = sidou_sys::GRID_YELLOW as u8;

type CellRun = (usize, usize, usize);

fn rgb565(c: [u8; 3]) -> u16 {
    (u16::from(c[0] & 0xf8) << 8) | (u16::from(c[1] & 0xfc) << 3) | u16::from(c[2] >> 3)
}

fn tap_home(m: &mut Machine) {
    m.key(HOME, true);
    m.run_ms(2 * UI_FRAME_MS);
    m.key(HOME, false);
}

const TRANSITION_END_MS: u64 = 700;

fn settle(m: &mut Machine, fw: &Firmware) {
    m.run_ms(TRANSITION_END_MS);
    to_mid_frame(m, fw);
}

fn open_menu_settled(m: &mut Machine, fw: &Firmware) {
    press(m, HOME);
    settle(m, fw);
}

const MENU_ITEM_SETTLE_MS: u64 = 100;

fn enter_menu_item(m: &mut Machine, fw: &Firmware, item: i32) {
    open_menu_settled(m, fw);
    select_menu_item(m, item);
    m.run_ms(MENU_ITEM_SETTLE_MS);
    m.turn(ALGORITHM, 1);
}

fn open_charset(m: &mut Machine, fw: &Firmware) {
    enter_menu_item(m, fw, MENU_CHARSET);
    settle(m, fw);
}

fn open_midi_page(m: &mut Machine, fw: &Firmware) {
    open_menu_settled(m, fw);
    select_menu_item(m, MENU_MIDI);
    m.turn(ALGORITHM, 1);
    settle(m, fw);
}

fn menu_cursor_to(m: &mut Machine, fw: &Firmware, item: i32) {
    select_menu_item(m, item);
    m.run_ms(300);
    to_mid_frame(m, fw);
}

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

fn card_byte(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    ui_byte(m, fw, sidou_sys::ilp32::views::card + field)
}

fn live_ptr(m: &mut Machine, fw: &Firmware) -> u32 {
    m.soc
        .read32(fw.symbol("ui") + sidou_sys::ilp32::views::l as u32)
}

fn live_byte(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    let l = live_ptr(m, fw);
    m.soc.read8(l + field as u32)
}

fn octave_and_model(m: &mut Machine, fw: &Firmware) -> (u8, u8) {
    (
        live_byte(m, fw, sidou_sys::ilp32::live::octave),
        live_byte(m, fw, sidou_sys::ilp32::live::model),
    )
}

fn song_on_chip_a(m: &mut Machine, fw: &Firmware) {
    m.soc.write8(
        fw.symbol("keys_router") + sidou_sys::ilp32::keys_route::song as u32,
        1,
    );
}

fn showing_card(m: &mut Machine, fw: &Firmware) -> bool {
    top_view(m, fw) == fw.symbol("title_card_view")
}

const CARD_TIMEOUT_MS: u64 = 4000;

fn past_card(m: &mut Machine, fw: &Firmware) -> u64 {
    let from = m.ms();
    while showing_card(m, fw) || screen_animating(m, fw) {
        assert!(
            m.ms() < from + CARD_TIMEOUT_MS,
            "the title card did not end"
        );
        m.run_ms(UI_FRAME_MS);
    }
    m.ms() - from
}

const CARDS_OFF: u8 = 0;
const CARDS_ALWAYS: u8 = 2;

fn set_cards(m: &mut Machine, fw: &Firmware, cards: u8) {
    m.soc.write8(
        fw.symbol("ui") + sidou_sys::ilp32::views::cards as u32,
        cards,
    );
}

const SETTINGS_TO_MENU_MS: u64 = 100;

fn settings_back_to_live_with_the_menu_cursor_on_top(m: &mut Machine) {
    press(m, HOME);
    m.run_ms(SETTINGS_TO_MENU_MS);
    m.turn(SELECT, -MENU_ITEMS);
    m.run_ms(turn_time(MENU_ITEMS));
    press(m, HOME);
}

fn cards_off(m: &mut Machine, fw: &Firmware) {
    open_menu_settled(m, fw);
    settings_item(m, sidou_sys::VIEWS_SETTINGS_CARDS);
    m.turn(ALGORITHM, -1);
    m.run_ms(100);
    assert_eq!(ui_byte(m, fw, sidou_sys::ilp32::views::cards), CARDS_OFF);
    settings_back_to_live_with_the_menu_cursor_on_top(m);
    settle(m, fw);
    assert_eq!(top_view(m, fw), fw.symbol("live_view_view"));
}

fn cell_rows(fb: &[u16]) -> Vec<&[u16]> {
    fb.chunks(W * CELL_PX).collect()
}

fn cell_row_pixels(idx: &[u8], row: usize) -> &[u8] {
    &idx[row * CELL_PX * W..(row + 1) * CELL_PX * W]
}

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
    let tmp = path.with_extension(format!(
        "png.{}.{:?}.tmp",
        std::process::id(),
        std::thread::current().id()
    ));
    {
        let file =
            std::fs::File::create(&tmp).unwrap_or_else(|e| panic!("create {}: {e}", tmp.display()));
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W as u32, W as u32);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Four);
        enc.set_palette(SRGB.concat());
        let packed: Vec<u8> = idx.chunks(2).map(|p| p[0] << 4 | p[1]).collect();
        let mut w = enc.write_header().expect("PNG header");
        w.write_image_data(&packed).expect("PNG data");
        w.finish().expect("PNG end");
    }
    std::fs::rename(&tmp, path)
        .unwrap_or_else(|e| panic!("rename {} to {}: {e}", tmp.display(), path.display()));
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

fn paint_ignored_cells_blue(idx: &mut [u8], cells: &[CellRun]) {
    for &(col, row, len) in cells {
        for y in row * CELL_PX..(row + 1) * CELL_PX {
            idx[y * W + col * CELL_PX..y * W + (col + len) * CELL_PX].fill(BLUE);
        }
    }
}

fn compare_golden(build: &Path, name: &str, fb: &[u16], ignore: &[CellRun]) {
    let mut got = indexed(fb);
    paint_ignored_cells_blue(&mut got, ignore);
    let path = golden_dir().join(format!("{name}.png"));
    if std::env::var("SIDOU_LCD_GOLDEN_UPDATE").as_deref() == Ok("1") {
        std::fs::create_dir_all(golden_dir()).unwrap();
        write_png(&path, &got);
        println!("wrote {}", path.display());
        return;
    }
    let mut want = read_png(&path);
    paint_ignored_cells_blue(&mut want, ignore);
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

fn compare_golden_cells(build: &Path, name: &str, fb: &[u16], cells: &[CellRun]) {
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

const VOICE1_NOTE_CELLS: [CellRun; 2] = [(24, 3, 6), (24, 4, 6)];
const STATUS_VERSION_CELLS: CellRun = (12, 2, 9);

#[test]
fn the_status_pages_match_their_goldens_and_select_goes_round_them_either_way() {
    let Some((build, fw)) = common::build_and_firmware("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    assert!(m.soc.lcd.display_on, "DISPON was not sent");
    open_status(&mut m, &fw);
    compare_golden(&build, "status", &m.soc.lcd.fb, &[STATUS_VERSION_CELLS]);
    let turn_page = |m: &mut Machine, clicks: i32| {
        m.turn(SELECT, clicks);
        m.run_ms(600);
    };
    turn_page(&mut m, 1);
    compare_golden(
        &build,
        "status-setup",
        &m.soc.lcd.fb,
        &[STATUS_VERSION_CELLS],
    );
    m.key(A4_KEY, true);
    m.run_ms(200);
    compare_golden(&build, "status-a4", &m.soc.lcd.fb, &[STATUS_VERSION_CELLS]);
    m.key(A4_KEY, false);
    turn_page(&mut m, 1);
    compare_golden(
        &build,
        "status-effects",
        &m.soc.lcd.fb,
        &[STATUS_VERSION_CELLS],
    );
    turn_page(&mut m, 1);
    let first = indexed(&m.soc.lcd.fb);
    turn_page(&mut m, -1);
    compare_golden(
        &build,
        "status-effects",
        &m.soc.lcd.fb,
        &[STATUS_VERSION_CELLS],
    );
    turn_page(&mut m, 1);
    assert!(
        indexed(&m.soc.lcd.fb) == first,
        "SELECT round the three pages does not come back to the first"
    );
}

#[test]
fn a_button_change_shows_a_toast_that_goes_away() {
    let Some((build, fw)) = common::build_and_firmware("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    open_status(&mut m, &fw);
    let boot = indexed(&m.soc.lcd.fb);
    press(&mut m, OCT_UP);
    m.run_ms(200);
    compare_golden(
        &build,
        "status-toast",
        &m.soc.lcd.fb,
        &[STATUS_VERSION_CELLS],
    );
    m.run_ms(TOAST_SHOWN_MS);
    let now = indexed(&m.soc.lcd.fb);
    assert!(
        cell_row_pixels(&now, TOAST_ROW) == cell_row_pixels(&boot, TOAST_ROW),
        "the toast stayed"
    );
}

#[test]
fn a_fast_select_spin_leaves_one_selected_line() {
    let Some(fw) = common::firmware("fast menu spin") else {
        return;
    };
    for reverse in [false, true] {
        let mut m = booted(&fw);
        open_menu_settled(&mut m, &fw);
        let selected = unsafe { sidou_sys::grid_palette[sidou_sys::GRID_DARK_GREY as usize] };
        m.turn(SELECT, 8);
        for frame in 0..6 {
            m.run_ms(UI_FRAME_MS);
            to_mid_frame(&mut m, &fw);
            let bars = (0..6)
                .filter(|&k| m.soc.lcd.fb[(4 + 3 * k) * CELL_PX * W] == selected)
                .count();
            assert_eq!(bars, 1, "frame {frame}, reverse {reverse}");
            if reverse && frame == 0 {
                m.turn(SELECT, -1);
            }
        }
    }
}

#[test]
fn the_menu_matches_its_golden_images_at_the_top_and_scrolled_and_does_not_blink() {
    let Some((build, fw)) = common::build_and_firmware("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    open_menu_settled(&mut m, &fw);
    let top = m.soc.lcd.fb.clone();
    m.run_ms(16 * UI_FRAME_MS);
    assert!(m.soc.lcd.fb == top, "the idle menu changed");
    menu_cursor_to(&mut m, &fw, MENU_ITEMS - 1);
    compare_golden(&build, "menu-scrolled", &m.soc.lcd.fb, &[]);
    compare_golden(&build, "menu", &top, &[]);
}

const POPUP_GONE_MS: u64 = 2_000;

fn without_popups(m: &mut Machine, fw: &Firmware) {
    let at = fw.symbol("ui")
        + (sidou_sys::ilp32::views::popup + sidou_sys::ilp32::screen_popup_state::source) as u32;
    m.soc.write32(at, 0);
}

fn cell_colour(idx: &[u8], row: usize, col: usize, c: u8) -> usize {
    (row * CELL_PX..(row + 1) * CELL_PX)
        .map(|y| {
            idx[y * W + col * CELL_PX..y * W + (col + 1) * CELL_PX]
                .iter()
                .filter(|&&p| p == c)
                .count()
        })
        .sum()
}

fn corners_in_colour(idx: &[u8], row: usize, col: usize, c: u8) -> usize {
    let last = CELL_PX - 1;
    let top = row * CELL_PX * W + col * CELL_PX;
    let bottom = (row * CELL_PX + last) * W + col * CELL_PX;
    [top, top + last, bottom, bottom + last]
        .iter()
        .filter(|&&i| idx[i] == c)
        .count()
}

const POPUP_OPEN_ROWS: u8 = sidou_sys::SCREEN_POPUP_ROWS as u8;
const OFF_TO_OVERDRIVE: i32 = 2;
const HIGHLIGHT_GONE_MS: u64 = 1_300;
const LIVE_PAGE_TAG_ROW: usize = sidou_sys::LIVE_VIEW_PAGE_ROW as usize;
const LIVE_KNOB_LABEL_ROW: usize = sidou_sys::LIVE_VIEW_KNOB_ROW as usize;

fn hold_edit_on_overdrive(m: &mut Machine) {
    m.key(EDIT, true);
    m.run_ms(30);
    m.turn(ALGORITHM, OFF_TO_OVERDRIVE);
    m.run_ms(HIGHLIGHT_GONE_MS);
}

fn popup_rows(m: &mut Machine, fw: &Firmware) -> u8 {
    ui_byte(
        m,
        fw,
        sidou_sys::ilp32::views::popup + sidou_sys::ilp32::screen_popup_state::rows,
    )
}

#[test]
fn edit_and_a_knob_on_live_open_the_effects_popup_until_a_second_after_the_last_detent() {
    let Some((build, fw)) = common::build_and_firmware("the effect popup test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    hold_edit_on_overdrive(&mut m);
    m.turn(K1, 3);
    m.run_ms(300);
    to_mid_frame(&mut m, &fw);
    assert_eq!(
        popup_rows(&mut m, &fw),
        POPUP_OPEN_ROWS,
        "the popup is open"
    );
    compare_golden(&build, "live-edit-popup", &m.soc.lcd.fb, &[]);
    m.key(EDIT, false);
    m.run_ms(POPUP_CLOSED_MS);
    assert_eq!(
        popup_rows(&mut m, &fw),
        0,
        "gone a second after the last detent"
    );
}

#[test]
fn a_knob_on_live_highlights_its_column_for_a_second_and_opens_no_popup() {
    let Some((build, fw)) = common::build_and_firmware("the highlight test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    m.turn(K1, 1);
    m.run_ms(1_700);
    to_mid_frame(&mut m, &fw);
    let before = indexed(&m.soc.lcd.fb);
    let label = |idx: &[u8]| {
        (0..6)
            .map(|c| corners_in_colour(idx, LIVE_KNOB_LABEL_ROW, c, YELLOW))
            .collect::<Vec<_>>()
    };
    assert!(
        label(&before).iter().all(|&n| n <= 1),
        "{:?}",
        label(&before)
    );
    m.turn(K1, 5);
    m.run_ms(300);
    to_mid_frame(&mut m, &fw);
    assert_eq!(popup_rows(&mut m, &fw), 0, "no popup over LIVE");
    let now = indexed(&m.soc.lcd.fb);
    assert!(
        label(&now).iter().all(|&n| n >= 3),
        "CUTOFF is reversed in yellow: {:?}",
        label(&now)
    );
    assert!(
        (8..12).all(|c| corners_in_colour(&now, LIVE_KNOB_LABEL_ROW, c, YELLOW) <= 1),
        "RESO is not"
    );
    compare_golden(&build, "live-highlight", &m.soc.lcd.fb, &[]);
    m.run_ms(1_200);
    to_mid_frame(&mut m, &fw);
    let after = indexed(&m.soc.lcd.fb);
    assert!(label(&after).iter().all(|&n| n <= 1), "{:?}", label(&after));
    assert_eq!(popup_rows(&mut m, &fw), 0);
    m.turn(K1, -5);
    m.run_ms(1_700);
    to_mid_frame(&mut m, &fw);
    assert!(
        indexed(&m.soc.lcd.fb) == before,
        "LIVE differs after the knob"
    );
}

#[test]
fn a_knob_on_chip_highlights_and_opens_no_popup_but_edit_and_the_knob_open_the_slots() {
    let Some((build, fw)) = common::build_and_firmware("the highlight test") else {
        return;
    };
    let mut m = booted(&fw);
    open_chip(&mut m, &fw);
    let chip = indexed(&m.soc.lcd.fb);
    let label = |idx: &[u8]| {
        (0..3)
            .map(|c| corners_in_colour(idx, sidou_sys::CHIP_VIEW_LABEL_ROW as usize, c, YELLOW))
            .collect::<Vec<_>>()
    };
    assert!(label(&chip).iter().all(|&n| n <= 1), "{:?}", label(&chip));
    m.turn(K1, 3);
    m.run_ms(300);
    to_mid_frame(&mut m, &fw);
    assert_eq!(popup_rows(&mut m, &fw), 0, "no popup over CHIP");
    assert!(
        label(&indexed(&m.soc.lcd.fb)).iter().all(|&n| n >= 3),
        "AGE is reversed"
    );
    compare_golden(&build, "chip-highlight", &m.soc.lcd.fb, &[]);
    m.turn(K1, -3);
    m.run_ms(1_700);
    to_mid_frame(&mut m, &fw);
    assert!(
        indexed(&m.soc.lcd.fb) == chip,
        "CHIP differs after the knob"
    );
    holding(&mut m, EDIT, |m| {
        m.turn(K1, 1);
        m.run_ms(300);
    });
    assert_eq!(
        popup_rows(&mut m, &fw),
        POPUP_OPEN_ROWS,
        "EDIT + K1 keeps its popup"
    );
}

#[test]
fn edit_held_shows_the_selected_slots_page_tag_and_labels_on_lives_knob_row() {
    let Some(fw) = common::firmware("the EDIT layer test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    let tone = indexed(&m.soc.lcd.fb);
    m.key(EDIT, true);
    m.run_ms(200);
    to_mid_frame(&mut m, &fw);
    let held = indexed(&m.soc.lcd.fb);
    assert!(
        cell_row_pixels(&held, LIVE_PAGE_TAG_ROW) != cell_row_pixels(&tone, LIVE_PAGE_TAG_ROW),
        "the page tag says DIST, not TONE"
    );
    assert!(
        cell_row_pixels(&held, LIVE_KNOB_LABEL_ROW) != cell_row_pixels(&tone, LIVE_KNOB_LABEL_ROW),
        "the labels are the slot's"
    );
    m.key(EDIT, false);
    m.run_ms(200);
    to_mid_frame(&mut m, &fw);
    assert!(indexed(&m.soc.lcd.fb) == tone, "LIVE is back after EDIT");
}

#[test]
fn a_toast_closes_an_open_popup() {
    let Some(fw) = common::firmware("the toast test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    m.pot(300);
    m.run_ms(200);
    m.pot(700);
    m.run_ms(300);
    assert_eq!(
        popup_rows(&mut m, &fw),
        POPUP_OPEN_ROWS,
        "MASTER's popup over LIVE"
    );
    press(&mut m, OCT_UP);
    m.run_ms(90);
    assert_eq!(popup_rows(&mut m, &fw), 0, "the popup is gone");
    let toast = ui_byte(
        &mut m,
        &fw,
        sidou_sys::ilp32::views::s + sidou_sys::ilp32::screen::toast_on,
    );
    assert_ne!(toast, 0, "the toast shows");
}

#[test]
fn select_pages_and_algorithm_steps_the_profile_on_live() {
    let Some(fw) = common::firmware("the LIVE encoders test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    let page = |m: &mut Machine| {
        ui_byte(
            m,
            &fw,
            sidou_sys::ilp32::views::live + sidou_sys::ilp32::live_view::page,
        )
    };
    let tone = indexed(&m.soc.lcd.fb);
    assert_eq!(page(&mut m), 0);
    m.turn(SELECT, 1);
    m.run_ms(300);
    to_mid_frame(&mut m, &fw);
    assert_eq!(page(&mut m), 1, "the slot page");
    assert_eq!(live_byte(&mut m, &fw, sidou_sys::ilp32::live::fx_select), 0);
    assert_eq!(popup_rows(&mut m, &fw), 0, "no popup for SELECT");
    let dist = indexed(&m.soc.lcd.fb);
    assert!(
        cell_row_pixels(&dist, LIVE_PAGE_TAG_ROW) != cell_row_pixels(&tone, LIVE_PAGE_TAG_ROW),
        "the page tag says DIST, not TONE"
    );
    m.turn(SELECT, 1);
    m.run_ms(300);
    assert_eq!(live_byte(&mut m, &fw, sidou_sys::ilp32::live::fx_select), 1);
    m.turn(SELECT, -5);
    m.run_ms(300);
    assert_eq!(page(&mut m), 0, "clamped at TONE");
    to_mid_frame(&mut m, &fw);
    assert!(indexed(&m.soc.lcd.fb) == tone, "TONE is back as it was");
    let profile =
        |m: &mut Machine| u32::from(live_byte(m, &fw, sidou_sys::ilp32::live::profile_req));
    assert_eq!(profile(&mut m), sidou_sys::SID_CHIP_6581_R4AR);
    m.turn(ALGORITHM, 1);
    m.run_ms(300);
    to_mid_frame(&mut m, &fw);
    assert_eq!(profile(&mut m), sidou_sys::SID_CHIP_8580_R5);
    assert_eq!(popup_rows(&mut m, &fw), 0, "no popup over LIVE");
    const BLACK_ON_WHITE: u8 = 0x10;
    const WHITE_ON_BLACK: u8 = 0x01;
    const PROFILE_WORD: (u32, u32) = (11, 22);
    let attr = |m: &mut Machine, (col, row): (u32, u32)| -> u8 {
        let g = m.soc.read32(
            fw.symbol("ui") + (sidou_sys::ilp32::views::s + sidou_sys::ilp32::screen::g) as u32,
        );
        m.soc
            .read8(g + sidou_sys::ilp32::grid::attr as u32 + row * GRID_COLS as u32 + col)
    };
    assert_eq!(
        attr(&mut m, PROFILE_WORD),
        BLACK_ON_WHITE,
        "R5 is reversed while it shows the change"
    );
    let now = indexed(&m.soc.lcd.fb);
    assert_eq!(
        cell_colour(&now, PROFILE_WORD.1 as usize, 15, WHITE),
        0,
        "R4AR's third and fourth characters are gone"
    );
    m.run_ms(HIGHLIGHT_GONE_MS);
    assert_eq!(
        attr(&mut m, PROFILE_WORD),
        WHITE_ON_BLACK,
        "and plain again after a second"
    );
}

fn header_cells(idx: &[u8], cols: std::ops::Range<usize>) -> Vec<u8> {
    (0..2 * CELL_PX)
        .flat_map(|y| idx[y * W + cols.start * CELL_PX..y * W + cols.end * CELL_PX].to_vec())
        .collect()
}

#[test]
fn an_edited_preset_shows_a_star_keeps_its_edits_across_presets_and_edit_reverts_it() {
    let Some((build, fw)) = common::build_and_firmware("the preset memory test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    let factory = indexed(&m.soc.lcd.fb);
    m.turn(K1, -10);
    m.run_ms(1_700);
    to_mid_frame(&mut m, &fw);
    let edited = indexed(&m.soc.lcd.fb);
    compare_golden(&build, "live-edited", &m.soc.lcd.fb, &[]);
    let preset = |m: &mut Machine| live_byte(m, &fw, sidou_sys::ilp32::live::preset);
    const STAR_AFTER_SAW_LEAD: std::ops::Range<usize> = 16..18;
    assert!(
        header_cells(&edited, STAR_AFTER_SAW_LEAD) != header_cells(&factory, STAR_AFTER_SAW_LEAD),
        "a star after the name"
    );
    m.turn(PRESETS, 1);
    m.run_ms(500);
    assert_eq!(preset(&mut m), 1);
    m.turn(PRESETS, -1);
    m.run_ms(1_700);
    to_mid_frame(&mut m, &fw);
    assert_eq!(preset(&mut m), 0);
    assert!(
        indexed(&m.soc.lcd.fb) == edited,
        "SAW LEAD's edit is back, star and all"
    );
    holding(&mut m, EDIT, |m| {
        m.turn(PRESETS, 1);
        m.run_ms(300);
    });
    m.run_ms(1_700);
    to_mid_frame(&mut m, &fw);
    assert!(
        indexed(&m.soc.lcd.fb) == factory,
        "reverted to the factory's"
    );
}

#[test]
fn presets_past_noise_perc_reaches_dice_which_rolls_and_numbers_a_new_sound_each_detent() {
    let Some((build, fw)) = common::build_and_firmware("the DICE test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    let live = |m: &mut Machine, f: usize| live_byte(m, &fw, f);
    m.turn(PRESETS, 3);
    m.run_ms(500);
    assert_eq!(live(&mut m, sidou_sys::ilp32::live::preset), 3);
    m.turn(PRESETS, 1);
    m.run_ms(1_700);
    to_mid_frame(&mut m, &fw);
    assert_eq!(live(&mut m, sidou_sys::ilp32::live::preset), 4, "DICE");
    assert_eq!(live(&mut m, sidou_sys::ilp32::live::dice_rolls), 1);
    let first = indexed(&m.soc.lcd.fb);
    let digits = |idx: &[u8]| header_cells(idx, 10..14);
    let first_digits = digits(&first);
    compare_golden(&build, "live-dice", &m.soc.lcd.fb, &[]);
    m.turn(PRESETS, 1);
    m.run_ms(1_700);
    to_mid_frame(&mut m, &fw);
    assert_eq!(
        live(&mut m, sidou_sys::ilp32::live::preset),
        4,
        "still DICE"
    );
    assert_eq!(live(&mut m, sidou_sys::ilp32::live::dice_rolls), 2);
    assert!(indexed(&m.soc.lcd.fb) != first, "a new roll, a new screen");
    assert!(
        digits(&indexed(&m.soc.lcd.fb)) != first_digits,
        "the header's number moved to 02"
    );
    m.turn(PRESETS, -1);
    m.run_ms(500);
    assert_eq!(live(&mut m, sidou_sys::ilp32::live::preset), 3);
}

#[test]
fn live_shows_the_playing_song_and_glo_latched_by_two_taps_mutes_a_voice() {
    let Some((build, fw)) = common::build_and_firmware("the LIVE song test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    let lv = |m: &mut Machine, field: usize| ui_byte(m, &fw, sidou_sys::ilp32::views::live + field);
    assert_eq!(lv(&mut m, sidou_sys::ilp32::live_view::playing), 0);
    press(&mut m, PLAY);
    m.run_ms(2_000);
    to_mid_frame(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("live_view_view"));
    assert_eq!(
        lv(&mut m, sidou_sys::ilp32::live_view::playing),
        1,
        "the song plays"
    );
    assert!(
        lv(&mut m, sidou_sys::ilp32::live_view::bpm) != 0,
        "its tempo"
    );
    compare_golden(&build, "live-song", &m.soc.lcd.fb, &[]);
    const GLO_LATCHED: u8 = 3;
    press(&mut m, GLO);
    press(&mut m, GLO);
    assert_eq!(
        lv(&mut m, sidou_sys::ilp32::live_view::glo),
        GLO_LATCHED,
        "the layer is latched"
    );
    m.key(KEY0 + 1, true);
    m.run_ms(60);
    m.key(KEY0 + 1, false);
    m.run_ms(300);
    to_mid_frame(&mut m, &fw);
    assert_eq!(
        live_voices_gated(&mut m, &fw),
        0,
        "the layer's key sounds nothing"
    );
    compare_golden(&build, "live-glo", &m.soc.lcd.fb, &[]);
    press(&mut m, GLO);
    m.run_ms(100);
    assert_eq!(lv(&mut m, sidou_sys::ilp32::live_view::glo), 0);
    press(&mut m, PLAY);
    m.run_ms(1_000);
    assert_eq!(lv(&mut m, sidou_sys::ilp32::live_view::playing), 0);
}

#[test]
fn the_octave_shows_at_2x_when_not_0() {
    let Some(fw) = common::firmware("the octave test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    let ink = |m: &Machine| {
        let idx = indexed(&m.soc.lcd.fb);
        (0..2usize)
            .flat_map(|r| (26..30usize).map(move |c| (r, c)))
            .map(|(r, c)| (cell_colour(&idx, r, c, BLACK), r, c))
            .filter(|&(black, _, _)| black < 64)
            .count()
    };
    assert_eq!(ink(&m), 0, "nothing at octave 0");
    press(&mut m, OCT_UP);
    m.run_ms(300);
    to_mid_frame(&mut m, &fw);
    assert!(ink(&m) >= 4, "+1 at 2x: {} cells with ink", ink(&m));
}

const POPUP_OPEN_FRAMES: u64 = 70;
const POPUP_WORST_POLL: u64 = 42_000;
const POPUP_STATIC_EXTRA: f64 = 15_000.0;
const _: () = assert!(POPUP_WORST_POLL <= POLL_INSTRUCTIONS);

#[test]
fn the_popup_stays_within_its_budgets_over_every_view() {
    let Some(fw) = common::firmware("the popup budget test") else {
        return;
    };
    let mut report = serde_json::Map::new();
    let mut over = Vec::new();
    for view in ["live", "status", "menu", "chip", "scene", "midi"] {
        let mut m = Machine::boot(&fw.image);
        m.watch_calls(fw.symbol("ui_poll"));
        run_past_boot(&mut m, &fw);
        match view {
            "status" => open_status(&mut m, &fw),
            "menu" => open_menu_settled(&mut m, &fw),
            "chip" => open_chip(&mut m, &fw),
            "scene" => open_scene(&mut m, &fw, 2),
            "midi" => open_midi_page(&mut m, &fw),
            _ => settle(&mut m, &fw),
        }
        if view == "live" || view == "scene" {
            m.key(A4_KEY, true);
            m.run_ms(300);
        }
        let idle = measure(&mut m, &fw, 10, |_| {});
        let pot = std::cell::Cell::new(600i32);
        let nudge_a_control_with_a_popup = |m: &mut Machine, d: i32| {
            if matches!(view, "live" | "chip") {
                pot.set((pot.get() + d * 30).clamp(100, 1000));
                m.pot(pot.get() as u16);
            } else {
                m.turn(K1, d);
            }
        };
        let run = measure(&mut m, &fw, POPUP_OPEN_FRAMES, |m| {
            nudge_a_control_with_a_popup(m, 3)
        });
        let turning = measure_frames(&mut m, &fw, 30, |m, f| {
            if f % 2 == 0 {
                nudge_a_control_with_a_popup(m, if f % 4 == 0 { 3 } else { -3 });
            }
        });
        m.run_ms(200);
        let stay = measure(&mut m, &fw, 10, |_| {});
        let extra = stay.instructions_per_frame() - idle.instructions_per_frame();
        println!(
            "popup over {view}: {} cells a frame at most, {:.1} ms of SPI, ui_poll {} / {}, a static popup costs {extra:.0} a frame, sent {} windows",
            run.frames.iter().chain(&turning.frames).map(cells).max().unwrap(),
            run.worst_frame_ms().max(turning.worst_frame_ms()),
            run.worst_poll,
            turning.worst_poll,
            stay.total().windows
        );
        let view_stands_still = !matches!(view, "live" | "scene" | "midi");
        if extra >= POPUP_STATIC_EXTRA || (view_stands_still && stay.total().windows != 0) {
            over.push(format!(
                "{view}: a static popup costs {extra:.0} instructions a frame and sent {} windows",
                stay.total().windows
            ));
        }
        for (what, c) in [("run", &run), ("turning", &turning)] {
            let most = c.frames.iter().map(cells).max().unwrap();
            if most > FRAME_CELLS_MOST
                || c.worst_frame_ms() > SPI_BUDGET_MS
                || c.worst_poll >= POPUP_WORST_POLL
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

const POPUP_INPUT: u64 = 250;
const POPUP_PASS: u64 = 70_000;
const _: () = assert!(POPUP_INPUT + POPUP_PASS <= POLL_INSTRUCTIONS);

#[test]
fn a_popup_ask_keeps_the_input_and_the_passes_within_the_allowance() {
    let Some(fw) = common::firmware("the popup input test") else {
        return;
    };
    let mut over = Vec::new();
    for view in ["live", "live-edit", "status", "menu", "scene"] {
        let mut m = booted(&fw);
        match view {
            "status" => open_status(&mut m, &fw),
            "live-edit" => {
                settle(&mut m, &fw);
                hold_edit_on_overdrive(&mut m);
            }
            "menu" => {
                open_menu_settled(&mut m, &fw);
                select_menu_item(&mut m, sidou_sys::VIEWS_MENU_OCTAVE as i32);
            }
            "scene" => open_scene(&mut m, &fw, 0),
            _ => settle(&mut m, &fw),
        }
        m.watch_calls(fw.symbol("ui_encoder"));
        m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
        let mut turns = vec![("K1", K1, 1), ("K2", K2, 1)];
        if view == "menu" {
            turns.push(("ALGORITHM", ALGORITHM, 1));
            turns.push(("SELECT", SELECT, 1));
        }
        for (what, enc, steps) in turns {
            let cost = start_cost(&mut m, TOAST_GONE_MS, |m| m.turn(enc, steps));
            note_if_over(
                &mut over,
                &format!("popup over {view}, {what}"),
                cost,
                (POPUP_INPUT, POPUP_PASS),
            );
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}

#[test]
fn the_midi_page_popup_names_the_item_and_its_value() {
    let Some((build, fw)) = common::build_and_firmware("the MIDI page popup") else {
        return;
    };
    let mut m = booted(&fw);
    open_midi_page(&mut m, &fw);
    m.turn(SELECT, 1);
    m.run_ms(40);
    m.turn(ALGORITHM, 1);
    m.run_ms(200);
    to_mid_frame(&mut m, &fw);
    let mut text = |offset: usize| {
        let bytes: Vec<u8> = (0..sidou_sys::SCREEN_POPUP_TEXT as usize)
            .map(|i| ui_byte(&mut m, &fw, sidou_sys::ilp32::views::popup + offset + i))
            .take_while(|&b| b != 0)
            .collect();
        String::from_utf8(bytes).unwrap()
    };
    assert_eq!(text(sidou_sys::ilp32::screen_popup_state::name), "CHANNEL");
    assert_eq!(text(sidou_sys::ilp32::screen_popup_state::value), "2");
    compare_golden(&build, "midi-popup", &m.soc.lcd.fb, &[]);
}

#[test]
fn the_midi_page_at_2x_matches_its_golden_reaches_the_router_and_stays_within_its_budgets() {
    let Some((build, fw)) = common::build_and_firmware("the MIDI page test") else {
        return;
    };
    let mut m = booted(&fw);
    without_popups(&mut m, &fw);
    m.watch_calls(fw.symbol("ui_poll"));
    open_menu_settled(&mut m, &fw);
    select_menu_item(&mut m, MENU_MIDI);
    settle(&mut m, &fw);
    let open = measure(&mut m, &fw, 16, |m| m.turn(ALGORITHM, 1));
    settle(&mut m, &fw);
    compare_golden(&build, "midi", &m.soc.lcd.fb, &[]);
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
    let scroll: Vec<Cost> = (0..9)
        .map(|_| measure(&mut m, &fw, 4, |m| m.turn(SELECT, 1)))
        .collect();
    println!(
        "{}",
        serde_json::json!({
            "wipe_down_to_the_midi_page": open.json(),
            "midi_page_idle": idle.json(),
            "midi_page_mode_edit": edit.json(),
        })
    );
    for (what, c) in [("open", &open), ("idle", &idle), ("edit", &edit)]
        .into_iter()
        .chain(scroll.iter().map(|c| ("scroll", c)))
    {
        assert!(
            c.frames.iter().all(|f| cells(f) <= MIDI_PAGE_FRAME_CELLS),
            "MIDI {what}: over {MIDI_PAGE_FRAME_CELLS} cells"
        );
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
    assert_eq!(
        idle.total().windows,
        0,
        "the large MIDI list must not blink"
    );
}

const MIDI_PAGE_FRAME_CELLS: u32 = 190;
const MIDI_PAGE_WORST_POLL: u64 = 23_000;
const _: () = assert!(MIDI_PAGE_WORST_POLL <= POLL_INSTRUCTIONS);

#[test]
fn midi_rows_edits_update_the_song_header_rows() {
    let Some(fw) = common::firmware("the MIDI song-header rows test") else {
        return;
    };
    let mut m = booted(&fw);
    open_midi_page(&mut m, &fw);
    let header_rows = sidou_sys::ilp32::views::song + sidou_sys::ilp32::song_view::rows_beat;
    let route_rows = fw.symbol("routing")
        + (sidou_sys::ilp32::midi_route::setup + sidou_sys::ilp32::midi_route_setup::rows_per_beat)
            as u32;
    let was = m.soc.read8(route_rows);
    assert_eq!(ui_byte(&mut m, &fw, header_rows), was);
    const ROWS_PER_BEAT_ITEM: i32 = 7;
    m.turn(SELECT, ROWS_PER_BEAT_ITEM);
    m.run_ms(100);
    m.turn(ALGORITHM, 1);
    settle(&mut m, &fw);
    let now = m.soc.read8(route_rows);
    assert_ne!(now, was, "ROWS/BEAT must change the routing setup");
    assert_eq!(ui_byte(&mut m, &fw, header_rows), now);
}

#[test]
fn the_song_pages_match_their_goldens_and_stay_within_the_frame_budget() {
    song_pages(true);
}

#[test]
fn the_song_pages_stay_within_the_frame_budget_before_golden_review() {
    song_pages(false);
}

fn song_pages(goldens: bool) {
    let Some((build, fw)) = common::build_and_firmware("the SONG pages") else {
        return;
    };
    let snapshot = |name: &str, fb: &[u16]| {
        if goldens {
            compare_golden(&build, name, fb, &[]);
        } else {
            write_png(&build.join(format!("{name}.actual.png")), &indexed(fb));
        }
    };
    let mut m = booted(&fw);
    open_menu_settled(&mut m, &fw);
    select_menu_item(&mut m, MENU_SONG);
    m.watch_calls(fw.symbol("ui_poll"));
    let open = measure(&mut m, &fw, 24, |m| m.turn(ALGORITHM, 1));
    snapshot("song", &m.soc.lcd.fb);
    let directory = measure(&mut m, &fw, 24, |m| m.turn(ALGORITHM, 1));
    snapshot("song-load", &m.soc.lcd.fb);
    press(&mut m, HOME);
    settle(&mut m, &fw);
    m.turn(SELECT, 2);
    m.run_ms(30);
    let editor = measure(&mut m, &fw, 24, |m| m.turn(ALGORITHM, 1));
    snapshot("song-save-as", &m.soc.lcd.fb);
    press(&mut m, HOME);
    settle(&mut m, &fw);
    let io = fw.symbol("songs") + sidou_sys::ilp32::song_io::dirty as u32;
    m.soc.write8(io, 1);
    m.turn(SELECT, -1);
    m.run_ms(30);
    let confirm = measure(&mut m, &fw, 24, |m| m.turn(ALGORITHM, 1));
    snapshot("song-confirm", &m.soc.lcd.fb);
    for (name, cost) in [
        ("open", open),
        ("load", directory),
        ("save as", editor),
        ("confirm", confirm),
    ] {
        let most = cost.frames.iter().map(cells).max().unwrap_or(0);
        println!(
            "SONG {name}: worst poll {}, {most} cells, {:.2} ms SPI",
            cost.worst_poll,
            cost.worst_frame_ms()
        );
        assert!(cost.worst_poll <= POLL_INSTRUCTIONS);
        assert!(most <= sidou_sys::SONG_VIEW_FRAME_CELLS);
        assert!(cost.worst_frame_ms() <= SPI_BUDGET_MS);
    }
}

const ABOVE_THE_LEARN_FRAME: [CellRun; 20] = {
    let mut rows = [(0, 0, GRID_COLS); 20];
    let mut r = 0;
    while r < 20 {
        rows[r].1 = r;
        r += 1;
    }
    rows
};

#[test]
fn midi_learn_over_the_tracker_leaves_its_rows_as_they_were_when_it_closes() {
    let Some(fw) = common::firmware("learn restores TRACKER") else {
        return;
    };
    let mut m = booted(&fw);
    open_tracker(&mut m, &fw);
    m.run_ms(2000);
    to_mid_frame(&mut m, &fw);
    let band = 20 * CELL_PX * W..26 * CELL_PX * W;
    let before = m.soc.lcd.fb[band.clone()].to_vec();
    holding(&mut m, SAVE, |m| {
        m.turn(K1, 1);
        m.run_ms(200);
        assert_eq!(top_view(m, &fw), fw.symbol("learn_view_view"));
    });
    m.run_ms(30);
    press(&mut m, HOME);
    m.run_ms(2000);
    to_mid_frame(&mut m, &fw);
    assert_eq!(&m.soc.lcd.fb[band], before.as_slice());
}

#[test]
fn midi_learn_prompts_match_their_golden_images() {
    let Some((build, fw)) = common::build_and_firmware("the screen test") else {
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
    to_mid_frame(&mut m, &fw);
    compare_golden(
        &build,
        "learn-learned",
        &m.soc.lcd.fb,
        &ABOVE_THE_LEARN_FRAME,
    );
    assert_eq!(
        top_view(&mut m, &fw),
        fw.symbol("live_view_view"),
        "the frame did not close"
    );
}

#[test]
fn the_glyph_sheet_opens_from_the_menu_and_home_goes_back() {
    let Some((build, fw)) = common::build_and_firmware("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    settle(&mut m, &fw);
    let live = m.soc.lcd.fb.clone();
    open_charset(&mut m, &fw);
    compare_golden(&build, "charset", &m.soc.lcd.fb, &[]);
    press(&mut m, HOME);
    settle(&mut m, &fw);
    menu_cursor_to(&mut m, &fw, 0);
    compare_golden(&build, "menu", &m.soc.lcd.fb, &[]);
    press(&mut m, HOME);
    m.run_ms(2 * UI_FRAME_MS);
    assert!(!showing_card(&mut m, &fw), "a card back to LIVE");
    settle(&mut m, &fw);
    assert!(m.soc.lcd.fb == live, "LIVE differs after the glyph sheet");
}

const UNDER_THE_MENU: char = 'S';
const MENU: char = 'M';
const BOTH: char = 'B';
const WIPE_EDGE: char = 'E';
const NEITHER: char = '?';
const WHITE_565: u16 = 0xffff;

fn classify_rows(fb: &[u16], under: &[u16], menu: &[u16]) -> String {
    let (f, s, mm) = (cell_rows(fb), cell_rows(under), cell_rows(menu));
    (0..GRID_ROWS)
        .map(|r| match (f[r] == s[r], f[r] == mm[r]) {
            (true, true) => BOTH,
            (true, false) => UNDER_THE_MENU,
            (false, true) => MENU,
            _ if f[r].iter().all(|&p| p == WHITE_565) => WIPE_EDGE,
            _ => NEITHER,
        })
        .collect()
}

fn wipe_samples(m: &mut Machine, status: &[u16], menu: &[u16]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for _ in 0..20 {
        m.run_ms(UI_FRAME_MS);
        let rows = classify_rows(&m.soc.lcd.fb, status, menu);
        let shape = rows
            .trim_start_matches([MENU, BOTH])
            .trim_start_matches(WIPE_EDGE)
            .trim_start_matches([UNDER_THE_MENU, BOTH]);
        assert!(shape.is_empty(), "not a wipe: {rows}");
        if out.last() != Some(&rows) {
            out.push(rows);
        }
    }
    out
}

#[test]
fn a_wipe_opens_the_menu_from_the_top_and_closes_it_from_the_bottom() {
    let Some(fw) = common::firmware("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    cards_off(&mut m, &fw);
    let status = m.soc.lcd.fb.clone();
    open_menu_settled(&mut m, &fw);
    let menu = m.soc.lcd.fb.clone();
    press(&mut m, HOME);
    settle(&mut m, &fw);
    assert!(m.soc.lcd.fb == status);

    to_mid_frame(&mut m, &fw);
    tap_home(&mut m);
    let down = wipe_samples(&mut m, &status, &menu);
    let menu_rows = |r: &String| r.chars().filter(|&c| c == MENU).count();
    let edges = down.iter().filter(|r| r.contains(WIPE_EDGE)).count();
    assert!(edges >= 6, "{edges} frames with an edge: {down:#?}");
    assert!(
        down.windows(2)
            .all(|w| menu_rows(&w[1]) >= menu_rows(&w[0])),
        "the menu shrank on the way down: {down:#?}"
    );
    assert!(!down.last().unwrap().contains([UNDER_THE_MENU, WIPE_EDGE]));
    let edge_rows: Vec<usize> = down.iter().filter_map(|r| r.find(WIPE_EDGE)).collect();
    assert!(edge_rows.windows(2).all(|w| w[1] > w[0]), "{edge_rows:?}");

    settle(&mut m, &fw);
    to_mid_frame(&mut m, &fw);
    tap_home(&mut m);
    let up = wipe_samples(&mut m, &status, &menu);
    let edge_rows: Vec<usize> = up.iter().filter_map(|r| r.find(WIPE_EDGE)).collect();
    assert!(edge_rows.len() >= 6, "{up:#?}");
    assert!(edge_rows.windows(2).all(|w| w[1] < w[0]), "{edge_rows:?}");
    assert!(!up.last().unwrap().contains([MENU, WIPE_EDGE]));
}

#[test]
fn a_chip_change_sweeps_the_raster_over_the_border() {
    let Some(fw) = common::firmware("the screen test") else {
        return;
    };
    let mut m = booted(&fw);
    open_status(&mut m, &fw);
    let left_border_pixel = |r: usize| (r * CELL_PX + 4) * W + 4;
    let left_border =
        |idx: &[u8]| -> Vec<u8> { (0..GRID_ROWS).map(|r| idx[left_border_pixel(r)]).collect() };
    let boot = left_border(&indexed(&m.soc.lcd.fb));
    let ramp = [1u8, 7, 13, 3, 14, 15, 10];
    to_mid_frame(&mut m, &fw);
    m.midi(&MODEL_8580_CC);
    let mut frames = Vec::new();
    for _ in 0..24 {
        m.run_ms(UI_FRAME_MS);
        frames.push(left_border(&indexed(&m.soc.lcd.fb)));
    }
    let ring = unsafe { (*sidou_sys::theme_get(sidou_sys::THEME_HIGH_CONTRAST)).dim };
    let swept: Vec<&Vec<u8>> = frames
        .iter()
        .filter(|b| b.iter().any(|&c| c != ring))
        .collect();
    assert!(swept.len() >= 10, "{} frames of raster", swept.len());
    for b in &swept {
        for r in 1..GRID_ROWS {
            let k = ramp
                .iter()
                .position(|&c| c == b[r - 1])
                .expect("a ramp colour");
            assert_eq!(
                b[r],
                ramp[(k + 1) % ramp.len()],
                "bands in ramp order: {b:?}"
            );
        }
    }
    let distinct: std::collections::BTreeSet<&Vec<u8>> = swept.iter().copied().collect();
    assert!(distinct.len() >= 6, "the bands did not move");
    m.run_ms(2000);
    assert!(
        left_border(&indexed(&m.soc.lcd.fb)) == boot,
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

    fn since(self, before: Sent) -> Sent {
        Sent {
            windows: self.windows - before.windows,
            bytes: self.bytes - before.bytes,
        }
    }
}

const CELL_BYTES: u32 = 128;
const WINDOW_COMMAND_BYTES: u32 = 11;

fn sent(m: &mut Machine, fw: &Firmware) -> Sent {
    let base = fw.symbol("lcd_count");
    Sent {
        windows: m.soc.read32(base),
        bytes: m.soc.read32(base + 4),
    }
}

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

impl Cost {
    fn of(calls: &[u64], frames: Vec<Sent>) -> Cost {
        Cost {
            polls: calls.len(),
            instructions: calls.iter().sum(),
            worst_poll: calls.iter().copied().max().unwrap_or(0),
            frames,
        }
    }
}

const COMPOSE_POLL_STEPS_A_MS: u64 = 10;

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
            self.per.push(sent(m, fw).since(b));
        }
    }

    fn run(&mut self, m: &mut Machine, fw: &Firmware, ms: u64) {
        let step = m.soc.periph.cycle_of_ms(1) / COMPOSE_POLL_STEPS_A_MS;
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

    fn end(mut self, m: &mut Machine, fw: &Firmware) -> Vec<Sent> {
        self.close(m, fw);
        self.per
    }
}

fn measure(m: &mut Machine, fw: &Firmware, frames: u64, act: impl FnOnce(&mut Machine)) -> Cost {
    to_mid_frame(m, fw);
    let from = m.watched_calls.len();
    act(m);
    let mut composes = Composes::new(m, fw);
    composes.run(m, fw, frames * UI_FRAME_MS);
    let per = composes.end(m, fw);
    Cost::of(&m.watched_calls[from..], per)
}

#[test]
fn screen_changes_and_transitions_stay_within_the_budgets() {
    let Some(fw) = common::firmware("the screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    run_past_boot(&mut m, &fw);
    open_status(&mut m, &fw);
    let idle = measure(&mut m, &fw, 10, |_| {});
    m.turn(SELECT, 1);
    settle(&mut m, &fw);
    let note = measure(&mut m, &fw, 10, |m| m.key(A4_KEY, true));
    m.key(A4_KEY, false);
    m.run_ms(200);
    let menu = measure(&mut m, &fw, 16, tap_home);
    settle(&mut m, &fw);
    let menu_idle = measure(&mut m, &fw, 16, |_| {});
    let back = measure(&mut m, &fw, 16, |m| m.turn(ALGORITHM, 1));
    settle(&mut m, &fw);
    let raster = measure(&mut m, &fw, 24, |m| m.midi(&MODEL_8580_CC));
    let report = serde_json::json!({
        "frame_ms": UI_FRAME_MS,
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
    assert!(
        idle.instructions_per_frame() < IDLE_FRAME_INSTRUCTIONS,
        "{:.0} instructions per idle frame",
        idle.instructions_per_frame()
    );
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
        extra(&note) < ONE_NOTE_EXTRA_INSTRUCTIONS,
        "one note cost {} instructions",
        extra(&note)
    );
    assert!(
        menu_idle.total().windows <= 2,
        "{} windows idle in the menu",
        menu_idle.total().windows
    );
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
    }
    const ROW_BYTES: u32 = GRID_COLS as u32 * CELL_BYTES + WINDOW_COMMAND_BYTES;
    const WIPE_EDGE_ROWS: u32 = 8;
    for (what, c) in [("wipe up", &menu), ("wipe down", &back)] {
        let t = c.total();
        assert!(
            t.bytes <= (GRID_ROWS as u32 + WIPE_EDGE_ROWS) * ROW_BYTES,
            "{what}: {} bytes",
            t.bytes
        );
        assert!(
            extra(c) < WIPE_EXTRA_INSTRUCTIONS,
            "{what}: {} instructions",
            extra(c)
        );
    }
    let busy = raster.frames.iter().filter(|f| f.windows > 0).count();
    assert!(
        (7..=12).contains(&busy),
        "the raster sent in {busy} of 24 frames, not every other"
    );
    for (what, c) in [
        ("idle", &idle),
        ("note", &note),
        ("menu idle", &menu_idle),
        ("wipe up", &menu),
        ("wipe down", &back),
        ("raster", &raster),
    ] {
        assert!(
            c.worst_poll < SCREEN_WORST_POLL,
            "{what}: a ui_poll took {} instructions",
            c.worst_poll
        );
    }
}

const IDLE_FRAME_INSTRUCTIONS: f64 = 7_500.0;
const ONE_NOTE_EXTRA_INSTRUCTIONS: u64 = 30_000;
const WIPE_EXTRA_INSTRUCTIONS: u64 = 800_000;
const SCREEN_WORST_POLL: u64 = 100_000;

const BANNER_VERSION_CELLS: CellRun = (1, 3, 28);
const TITLE_VERSION_CELLS: CellRun = (1, sidou_sys::BOOT_SCENE_VERSION_ROW as usize, 28);

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

fn assert_title_card_is_the_logo_source_but_for_its_two_text_lines(fb: &[u16]) {
    let version = sidou_sys::BOOT_SCENE_VERSION_ROW as usize * CELL_PX;
    let key = sidou_sys::BOOT_SCENE_KEY_ROW as usize * CELL_PX;
    assert_shows_the_logo_source("the title card", fb, 0..version, 0);
    assert_shows_the_logo_source(
        "the title card",
        fb,
        version + CELL_PX..key,
        version + CELL_PX,
    );
    assert_shows_the_logo_source("the title card", fb, key + CELL_PX..W, key + CELL_PX);
}

const RUN_CURSOR: CellRun = (5, 13, 1);

#[test]
fn the_short_boot_shows_the_title_card_then_wipes_to_live() {
    let Some((build, fw)) = common::build_and_firmware("the boot screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.run_ms(700);
    assert!(m.soc.lcd.display_on, "DISPON was not sent");
    compare_golden(&build, "boot-title", &m.soc.lcd.fb, &[TITLE_VERSION_CELLS]);
    assert_title_card_is_the_logo_source_but_for_its_two_text_lines(&m.soc.lcd.fb);
    let gone = run_past_boot(&mut m, &fw);
    assert!(
        (1600..2200).contains(&gone),
        "the short boot ended at {gone} ms"
    );
    compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
}

const POWER_ON_HOLD_MS: u64 = 600;

fn hold_home_through_power_on(m: &mut Machine) {
    m.key(HOME, true);
    m.run_ms(POWER_ON_HOLD_MS);
    m.key(HOME, false);
}

const FULL_BOOT_LOADING_MS: u64 = 4_300;
const FULL_BOOT_READY_MS: u64 = 7_200;
const FULL_BOOT_TITLE_WIPE_MS: u64 = 6_800;
const FULL_BOOT_TITLE_MS: u64 = 8_000;
const FULL_BOOT_LIVE_MS: u64 = 11_200;

#[test]
fn home_held_at_power_on_plays_the_full_boot() {
    let Some((build, fw)) = common::build_and_firmware("the boot screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    hold_home_through_power_on(&mut m);
    m.run_ms(FULL_BOOT_LOADING_MS - POWER_ON_HOLD_MS);
    let idx = indexed(&m.soc.lcd.fb);
    let border: std::collections::BTreeSet<u8> = (0..W).map(|y| idx[y * W + 3]).collect();
    assert!(
        border.len() >= 4,
        "no loading stripes on the border: {border:?}"
    );
    compare_golden(
        &build,
        "boot-loading",
        &m.soc.lcd.fb,
        &[BANNER_VERSION_CELLS],
    );
    m.run_ms(FULL_BOOT_READY_MS - FULL_BOOT_LOADING_MS);
    let live = booted(&fw);
    assert!(
        cell_rows(&m.soc.lcd.fb)[7..14] != cell_rows(&live.soc.lcd.fb)[7..14],
        "LIVE, not the boot's prompt"
    );
    compare_golden(
        &build,
        "boot-ready",
        &m.soc.lcd.fb,
        &[BANNER_VERSION_CELLS, RUN_CURSOR],
    );
    m.run_ms(FULL_BOOT_TITLE_MS - FULL_BOOT_READY_MS);
    compare_golden(
        &build,
        "boot-title-full",
        &m.soc.lcd.fb,
        &[TITLE_VERSION_CELLS],
    );
    assert_title_card_is_the_logo_source_but_for_its_two_text_lines(&m.soc.lcd.fb);
    m.run_ms(FULL_BOOT_LIVE_MS - FULL_BOOT_TITLE_MS);
    compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
}

const TOUR_OFF: u8 = 0;
const TOUR_TURN_K1: u8 = 2;
const TOUR_AFTER_K1: u8 = 3;
const TOUR_PRESS_ENV: u8 = 4;
const TOUR_AFTER_ENV: u8 = 5;
const TOUR_HOLD_HOME: u8 = 6;

fn tour_step(m: &mut Machine, fw: &Firmware) -> u8 {
    ui_byte(m, fw, sidou_sys::ilp32::views::tour)
}

#[test]
fn the_tour_follows_the_first_boot_after_an_install() {
    let Some(fw) = common::firmware("the tour test") else {
        return;
    };
    let mut m = Machine::boot_blank(&fw.image);
    run_past_boot(&mut m, &fw);
    m.run_ms(200);
    assert_eq!(
        tour_step(&mut m, &fw),
        TOUR_TURN_K1,
        "TURN K1 after the boot"
    );
    m.turn(K1, 1);
    m.run_ms(200);
    assert_eq!(
        tour_step(&mut m, &fw),
        TOUR_AFTER_K1,
        "K1 ended the first toast"
    );
    m.run_ms(1500);
    assert_eq!(
        tour_step(&mut m, &fw),
        TOUR_PRESS_ENV,
        "PRESS ENV 1.6 s later"
    );
    press(&mut m, ENV);
    m.run_ms(200);
    assert_eq!(
        tour_step(&mut m, &fw),
        TOUR_AFTER_ENV,
        "ENV ended the second"
    );
    assert_eq!(
        top_view(&mut m, &fw),
        fw.symbol("lab_view_view"),
        "ENV opened LAB"
    );
    assert_eq!(
        ui_byte(
            &mut m,
            &fw,
            sidou_sys::ilp32::views::lab + sidou_sys::ilp32::lab_view::page
        ),
        sidou_sys::LAB_PAGE_ENVELOPE as u8,
        "at its ENVELOPE page"
    );
    m.run_ms(1700);
    assert_eq!(tour_step(&mut m, &fw), TOUR_HOLD_HOME, "HOLD HOME FOR HELP");
    hold_for_help(&mut m, &fw);
    assert_eq!(tour_step(&mut m, &fw), TOUR_OFF, "the card ended the tour");
    let_go_of_home_and_close_help_with_k1(&mut m, &fw);
}

#[test]
fn no_tour_when_home_asked_for_the_boot_or_on_a_normal_boot() {
    let Some(fw) = common::firmware("the tour test") else {
        return;
    };
    let mut m = booted(&fw);
    m.run_ms(300);
    assert_eq!(tour_step(&mut m, &fw), TOUR_OFF, "a normal boot");
    let mut m = Machine::boot_blank(&fw.image);
    hold_home_through_power_on(&mut m);
    run_past_boot(&mut m, &fw);
    m.run_ms(300);
    assert_eq!(
        tour_step(&mut m, &fw),
        TOUR_OFF,
        "HOME asked for the full boot"
    );
}

#[test]
fn a_button_skips_the_boot_and_does_nothing_else() {
    let Some((build, fw)) = common::build_and_firmware("the boot screen test") else {
        return;
    };
    for button in [FX, OCT_UP, HOME] {
        let mut m = Machine::boot(&fw.image);
        m.run_ms(900);
        press(&mut m, button);
        settle(&mut m, &fw);
        compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
    }
}

const AUDIBLE_PEAK: u16 = 1000;

fn left_peak_since(m: &Machine, from: usize) -> u16 {
    m.left()[from..]
        .iter()
        .map(|s| s.unsigned_abs())
        .max()
        .unwrap_or(0)
}

#[test]
fn a_note_skips_the_boot_and_plays() {
    let Some((build, fw)) = common::build_and_firmware("the boot screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.run_ms(600);
    let from = m.soc.periph.audio.len() / 2;
    m.key(A4_KEY, true);
    settle(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("live_view_view"));
    compare_golden_cells(&build, "live-a4", &m.soc.lcd.fb, &VOICE1_NOTE_CELLS);
    let peak = left_peak_since(&m, from);
    assert!(peak > AUDIBLE_PEAK, "the note did not play: peak {peak}");
}

#[test]
fn home_tapped_as_soon_as_the_card_shows_skips_the_short_boot() {
    let Some((build, fw)) = common::build_and_firmware("the boot screen test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    while !m.soc.lcd.display_on {
        assert!(m.ms() < 2000, "the display did not come on");
        m.run_ms(5);
    }
    let shown = m.ms();
    press(&mut m, HOME);
    let gone = run_past_boot(&mut m, &fw);
    assert!(
        gone < shown + 600,
        "the boot went on until {gone} ms (the display came on at {shown} ms): HOME did not skip"
    );
    compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
}

const ABOUT_GOLDEN_FRAME: u32 = 9;
const ABOUT_FIRST_PAGE_WHOLE_FRAME: u32 = 40;

fn open_about(m: &mut Machine, fw: &Firmware) {
    open_about_at(m, fw, ABOUT_GOLDEN_FRAME);
}

fn open_about_at(m: &mut Machine, fw: &Firmware, at_frame: u32) {
    open_menu_settled(m, fw);
    m.turn(SELECT, MENU_ABOUT);
    m.run_ms(turn_time(MENU_ABOUT));
    m.turn(ALGORITHM, 1);
    let at = sidou_sys::ilp32::views::about + sidou_sys::ilp32::about::frame;
    let _ = ui_byte(m, fw, at);
    let phase_at = fw.symbol("ui") + at as u32;
    for _ in 0..TRANSITION_END_MS + u64::from(at_frame) * UI_FRAME_MS {
        m.run_ms(1);
        let phase = m.soc.read32(phase_at);
        if phase >= at_frame {
            assert_eq!(phase, at_frame);
            break;
        }
    }
    to_mid_frame(m, fw);
    assert_eq!(m.soc.read32(phase_at), at_frame);
    assert_eq!(
        top_view(m, fw),
        fw.symbol("about_view"),
        "the menu did not open ABOUT"
    );
}

const ABOUT_VERSION_CELLS: [CellRun; 2] = [
    (1, sidou_sys::ABOUT_HEADING_ROW as usize, 28),
    (1, sidou_sys::ABOUT_HEADING_ROW as usize + 1, 28),
];

#[test]
fn about_opens_from_the_menu_shows_the_logo_moves_every_frame_and_home_goes_back() {
    let Some((build, fw)) = common::build_and_firmware("the About test") else {
        return;
    };
    let mut m = booted(&fw);
    open_about(&mut m, &fw);
    compare_golden(&build, "about", &m.soc.lcd.fb, &ABOUT_VERSION_CELLS);
    let band = sidou_sys::LOGO_METAL_BAND_ROW as usize * CELL_PX;
    let top = sidou_sys::ABOUT_LOGO_ROW as usize * CELL_PX;
    let rows = sidou_sys::ABOUT_LOGO_ROWS as usize * CELL_PX;
    assert_shows_the_logo_source("About", &m.soc.lcd.fb, top..top + rows, band);
    let before = m.soc.lcd.fb.clone();
    m.run_ms(UI_FRAME_MS);
    assert!(m.soc.lcd.fb != before, "About stood still");
    press(&mut m, HOME);
    settle(&mut m, &fw);
    press(&mut m, HOME);
    settle(&mut m, &fw);
    open_about_at(&mut m, &fw, ABOUT_FIRST_PAGE_WHOLE_FRAME);
    compare_golden(&build, "about-first", &m.soc.lcd.fb, &ABOUT_VERSION_CELLS);
    press(&mut m, HOME);
    settle(&mut m, &fw);
    menu_cursor_to(&mut m, &fw, 0);
    compare_golden(&build, "menu", &m.soc.lcd.fb, &[]);
}

fn cells(f: &Sent) -> u32 {
    (f.bytes - f.windows * WINDOW_COMMAND_BYTES) / CELL_BYTES
}

fn assert_frames_within(what: &str, c: &Cost, most_cells: u32, worst_poll: u64, per_frame: f64) {
    assert!(
        c.worst_frame_ms() <= SPI_BUDGET_MS,
        "{what}: a frame took {:.1} ms of SPI",
        c.worst_frame_ms()
    );
    let most = c.frames.iter().map(cells).max().unwrap_or(0);
    assert!(most <= most_cells, "{what}: a frame sent {most} cells");
    assert!(
        c.worst_poll < worst_poll,
        "{what}: a ui_poll took {} instructions",
        c.worst_poll
    );
    assert!(
        c.instructions_per_frame() < per_frame,
        "{what}: {:.0} instructions a frame",
        c.instructions_per_frame()
    );
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
    let Some(fw) = common::firmware("the boot and About budget test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    hold_home_through_power_on(&mut m);
    let boot = measure(&mut m, &fw, 260, |_| {});
    open_about(&mut m, &fw);
    let pages = u64::from(unsafe { sidou_sys::credits_page_count }) + 1;
    let about = measure(
        &mut m,
        &fw,
        pages * u64::from(sidou_sys::ABOUT_PAGE_FRAMES),
        |_| {},
    );
    let report = serde_json::json!({
        "frame_ms": UI_FRAME_MS,
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
    let n = about.frames.len() as f64;
    let avg = f64::from(about.frames.iter().map(cells).sum::<u32>()) / n;
    assert!(
        avg <= ABOUT_CELLS_PER_FRAME,
        "About sends {avg:.1} cells a frame"
    );
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

const WIPE_EDGE_ROW: u32 = 1;
const WIPE_CELLS: u32 = (sidou_sys::SCREEN_WIPE_ROWS + WIPE_EDGE_ROW) * GRID_COLS as u32;
const WIPE_WORST_POLL: u64 = 72_000;
const WIPE_PER_FRAME: f64 = 150_000.0;

fn wipe_windows(m: &mut Machine, fw: &Firmware, act: impl FnOnce(&mut Machine)) -> Vec<Sent> {
    to_mid_frame(m, fw);
    act(m);
    let mut out = Vec::new();
    let mut was = screen_wiping(m, fw);
    for _ in 0..30 {
        let before = sent(m, fw);
        m.run_ms(UI_FRAME_MS);
        let frame = sent(m, fw).since(before);
        let wiping = screen_wiping(m, fw);
        if was || wiping {
            out.push(frame);
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

#[test]
fn every_wipe_into_an_animated_view_at_any_phase_sends_only_its_rows() {
    let Some(fw) = common::firmware("the wipe budget test") else {
        return;
    };
    let mut m = booted(&fw);
    m.watch_calls(fw.symbol("ui_poll"));
    open_menu_settled(&mut m, &fw);
    m.turn(SELECT, MENU_ABOUT);
    settle(&mut m, &fw);
    let mut worst = Sent::default();
    let mut most = 0;
    let mut about_polls = Vec::new();
    for phase in (0..UI_FRAME_MS).step_by(5) {
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
    m.run_ms(FULL_BOOT_TITLE_WIPE_MS - POWER_ON_HOLD_MS);
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

fn wipe_instructions(calls: &[u64], frames: usize) -> (u64, f64) {
    let worst = calls.iter().copied().max().unwrap_or(0);
    (worst, calls.iter().sum::<u64>() as f64 / frames as f64)
}

const ABOUT_CELLS_PER_FRAME: f64 = 120.0;
const ABOUT_WORST_POLL: u64 = 80_000;
const ABOUT_PER_FRAME: f64 = 130_000.0;
const BOOT_WORST_POLL: u64 = 38_000;
const BOOT_PER_FRAME: f64 = 42_000.0;

const CHORD: [u8; 3] = [note_key(57), note_key(64), note_key(76)];
const CHORD_STAGGER_MS: u64 = 5;

fn chord_keys(m: &mut Machine, down: bool) {
    for k in CHORD {
        m.key(k, down);
    }
}

const SCOPE_LEFT: usize = sidou_sys::LIVE_VIEW_SCOPE_COL as usize * CELL_PX;
const SCOPE_W: usize = sidou_sys::SCOPE_W as usize;
const SCOPE_H: usize = sidou_sys::LIVE_VIEW_SCOPE_PX as usize;
const FLAT_TRACE_PIXELS: usize = 2 * SCOPE_W;

fn scope_top(voice: usize) -> usize {
    (sidou_sys::LIVE_VIEW_LANE_ROW as usize + sidou_sys::LIVE_VIEW_LANE_ROWS as usize * voice)
        * CELL_PX
}

fn scope_pixels(fb: &[u16]) -> Vec<u16> {
    let mut out = Vec::new();
    for v in 0..sidou_sys::SCOPE_VOICES as usize {
        for y in scope_top(v)..scope_top(v) + SCOPE_H {
            out.extend_from_slice(&fb[y * W + SCOPE_LEFT..y * W + SCOPE_LEFT + SCOPE_W]);
        }
    }
    out
}

fn trace_pixels(fb: &[u16]) -> [usize; 3] {
    let idx = indexed(fb);
    std::array::from_fn(|v| {
        (scope_top(v)..scope_top(v) + SCOPE_H)
            .flat_map(|y| (SCOPE_LEFT..SCOPE_LEFT + SCOPE_W).map(move |x| (x, y)))
            .filter(|&(x, y)| idx[y * W + x] != BLACK)
            .count()
    })
}

#[test]
fn live_shows_the_keys_on_chip_b_while_a_song_plays_chip_a() {
    let Some((build, fw)) = common::build_and_firmware("the LIVE keys test") else {
        return;
    };
    let mut m = booted(&fw);
    song_on_chip_a(&mut m, &fw);
    m.key(A4_KEY, true);
    m.run_ms(1500);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "live-keys-b", &m.soc.lcd.fb, &[]);
    let a4 = trace_pixels(&m.soc.lcd.fb);
    assert!(a4[0] > 200, "voice 1's trace with A4 on chip B: {a4:?}");
}

const BLOCK_CYCLES: u64 = 128 * 5_440;
const SCAN_FRAME_CYCLES: u64 = 11 * 24_000;
const SCAN_FRAMES_AT: u32 = 276;
const PRESS_TAKEN_AFTER_SCAN_FRAMES: u64 = 2;
const RELEASE_TAKEN_AFTER_SCAN_FRAMES: u64 = 8;
const NOTE_TO_SHOT_BLOCKS: u64 = 517;
const CHORD_STAGGER_BLOCKS: u64 = 2;
const SHOT_LEAD_MS: u64 = 1_700;

struct LiveShot {
    prepare_cycle: u64,
    prepare_block_start: u64,
}

fn block_phase(m: &Machine, c: u64) -> f64 {
    let at = *m.half_played.last().expect("audio running");
    ((c + BLOCK_CYCLES * 1_000 - at) % BLOCK_CYCLES) as f64 / BLOCK_CYCLES as f64
}

fn run_until(m: &mut Machine, cycle: u64) {
    let now = m.soc.periph.cycles;
    if cycle > now {
        m.run_cycles(cycle - now);
    }
}

fn live_shot(m: &mut Machine, fw: &Firmware, lead_ms: u64) -> LiveShot {
    m.watch_calls(fw.symbol("live_view_prepare"));
    let n = m.watched_starts.len();
    while m.watched_starts.len() == n {
        m.run_ms(1);
    }
    let last = *m.watched_starts.last().unwrap();
    let frame = m.soc.periph.cycle_of_ms(UI_FRAME_MS);
    let earliest = m.soc.periph.cycles + m.soc.periph.cycle_of_ms(lead_ms);
    let mut prepare = last;
    while prepare < earliest || !(0.4..0.6).contains(&block_phase(m, prepare)) {
        prepare += frame;
    }
    let block = prepare - (block_phase(m, prepare) * BLOCK_CYCLES as f64) as u64;
    LiveShot {
        prepare_cycle: prepare,
        prepare_block_start: block,
    }
}

fn key_into_block(m: &mut Machine, fw: &Firmware, key: u8, down: bool, block: u64) {
    let frames = fw.symbol("fm1_in") + SCAN_FRAMES_AT;
    let after = if down {
        PRESS_TAKEN_AFTER_SCAN_FRAMES
    } else {
        RELEASE_TAKEN_AFTER_SCAN_FRAMES
    } * SCAN_FRAME_CYCLES;
    let (lo, hi) = (block + BLOCK_CYCLES / 10, block + BLOCK_CYCLES / 2);
    run_until(m, lo.saturating_sub(after + 2 * SCAN_FRAME_CYCLES));
    loop {
        let f = m.soc.read32(frames);
        while m.soc.read32(frames) == f {
            m.run_cycles(2_400);
        }
        let taken = m.soc.periph.cycles + after;
        assert!(taken <= hi, "no scan frame puts key {key} into its block");
        if taken >= lo {
            m.key(key, down);
            return;
        }
    }
}

fn take_live_shot(m: &mut Machine, fw: &Firmware, shot: &LiveShot) {
    let frame = m.soc.periph.cycle_of_ms(UI_FRAME_MS);
    run_until(m, shot.prepare_cycle - frame / 4);
    let lv = fw.symbol("ui") + sidou_sys::ilp32::views::live as u32;
    let held = m.soc.read32(lv + sidou_sys::ilp32::live_view::held as u32);
    for i in 0..sidou_sys::SCOPE_VOICES * sidou_sys::SCOPE_W {
        m.soc
            .write8(lv + sidou_sys::ilp32::live_view::y as u32 + i, 0);
    }
    for i in 0..sidou_sys::LIVE_VIEW_METERS {
        m.soc
            .write8(lv + sidou_sys::ilp32::live_view::meter as u32 + i, 0);
    }
    m.watch_calls(fw.symbol("live_view_prepare"));
    let n = m.watched_starts.len();
    run_until(m, shot.prepare_cycle + frame / 2);
    let at = m.watched_starts[n];
    assert!(
        at.abs_diff(shot.prepare_cycle) < m.soc.periph.cycle_of_ms(1) / 2,
        "the frame's preparation came {} cycles from the one planned: the frames' grid slipped",
        at as i64 - shot.prepare_cycle as i64
    );
    to_mid_frame(m, fw);
    assert_eq!(
        m.soc.read32(lv + sidou_sys::ilp32::live_view::held as u32),
        held,
        "a trace was held back: the shot is not one preparation's"
    );
}

#[test]
fn live_matches_its_golden_images_and_its_scopes_follow_the_notes() {
    let Some((build, fw)) = common::build_and_firmware("the LIVE test") else {
        return;
    };
    let mut m = booted(&fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("live_view_view"));
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
    let idle = scope_pixels(&m.soc.lcd.fb);
    let flat = trace_pixels(&m.soc.lcd.fb);
    assert_eq!(
        flat, [FLAT_TRACE_PIXELS; 3],
        "the traces at rest: two pixels thick"
    );
    let shot = live_shot(&mut m, &fw, SHOT_LEAD_MS);
    key_into_block(
        &mut m,
        &fw,
        A4_KEY,
        true,
        shot.prepare_block_start - NOTE_TO_SHOT_BLOCKS * BLOCK_CYCLES,
    );
    take_live_shot(&mut m, &fw, &shot);
    compare_golden(&build, "live-a4", &m.soc.lcd.fb, &[]);
    let a4 = trace_pixels(&m.soc.lcd.fb);
    assert!(a4[0] > 200, "voice 1's trace with A4: {a4:?}");
    assert_eq!(
        &a4[1..],
        [FLAT_TRACE_PIXELS; 2],
        "voices 2 and 3 are silent"
    );
    let held = scope_pixels(&m.soc.lcd.fb);
    m.run_ms(UI_FRAME_MS);
    assert!(
        scope_pixels(&m.soc.lcd.fb) == held,
        "a sustained note's trace moved"
    );
    m.key(A4_KEY, false);
    m.run_ms(1500);
    to_mid_frame(&mut m, &fw);
    assert!(
        scope_pixels(&m.soc.lcd.fb) == idle,
        "the scopes are not flat after the release"
    );
}

#[test]
fn a_chord_on_live_takes_a_voice_a_note_and_matches_its_golden_image() {
    let Some((build, fw)) = common::build_and_firmware("the LIVE chord test") else {
        return;
    };
    let mut m = booted(&fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("live_view_view"));
    let shot = live_shot(&mut m, &fw, SHOT_LEAD_MS);
    for (i, k) in CHORD.into_iter().enumerate() {
        let lead_blocks = NOTE_TO_SHOT_BLOCKS - CHORD_STAGGER_BLOCKS * i as u64;
        let block = shot.prepare_block_start - lead_blocks * BLOCK_CYCLES;
        key_into_block(&mut m, &fw, k, true, block);
    }
    take_live_shot(&mut m, &fw, &shot);
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

const C64_GOLDEN_FRAME: u32 = 139;

fn c64_theme_and_back_to_live(m: &mut Machine, fw: &Firmware) {
    cards_off(m, fw);
    open_menu_settled(m, fw);
    settings_item(m, sidou_sys::VIEWS_SETTINGS_THEME);
    m.turn(ALGORITHM, 1);
    m.run_ms(20);
    settings_back_to_live_with_the_menu_cursor_on_top(m);
    let at = sidou_sys::ilp32::views::s + sidou_sys::ilp32::screen::frame;
    let _ = ui_byte(m, fw, at);
    let phase_at = fw.symbol("ui") + at as u32;
    for _ in 0..TRANSITION_END_MS {
        let phase = m.soc.read32(phase_at);
        assert!(phase <= C64_GOLDEN_FRAME, "frame {phase}");
        if phase == C64_GOLDEN_FRAME {
            break;
        }
        m.run_ms(1);
    }
    to_mid_frame(m, fw);
    assert_eq!(m.soc.read32(phase_at), C64_GOLDEN_FRAME);
    assert_eq!(top_view(m, fw), fw.symbol("live_view_view"));
}

#[test]
fn live_matches_its_golden_image_in_the_c64_theme_and_back() {
    let Some((build, fw)) = common::build_and_firmware("the LIVE theme test") else {
        return;
    };
    let mut m = booted(&fw);
    c64_theme_and_back_to_live(&mut m, &fw);
    m.key(A4_KEY, true);
    m.run_ms(1500);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "live-c64", &m.soc.lcd.fb, &[]);
    m.key(A4_KEY, false);
    m.run_ms(1500);
    open_menu_settled(&mut m, &fw);
    settings_item(&mut m, sidou_sys::VIEWS_SETTINGS_THEME);
    m.turn(ALGORITHM, -1);
    m.run_ms(100);
    settings_back_to_live_with_the_menu_cursor_on_top(&mut m);
    settle(&mut m, &fw);
    compare_golden(&build, "live", &m.soc.lcd.fb, &VOICE1_NOTE_CELLS);
}

#[test]
fn live_stays_within_its_budgets() {
    let Some(fw) = common::firmware("the LIVE budget test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    run_past_boot(&mut m, &fw);
    let idle = measure(&mut m, &fw, 25, |_| {});
    let chord = measure(&mut m, &fw, 75, |m| chord_keys(m, true));
    chord_keys(&mut m, false);
    m.run_ms(1500);
    m.midi(&NOISE_AT_FULL_SUSTAIN);
    m.run_ms(100);
    let noise = measure(&mut m, &fw, 75, |m| chord_keys(m, true));
    let report = serde_json::json!({
        "frame_ms": UI_FRAME_MS,
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
        assert_frames_within(
            what,
            c,
            sidou_sys::LIVE_VIEW_FRAME_CELLS,
            LIVE_WORST_POLL,
            per_frame,
        );
    }
    let n = noise.frames.len() as f64;
    let avg = f64::from(noise.frames.iter().map(cells).sum::<u32>()) / n;
    assert!(
        avg > 100.0,
        "noise sent {avg:.0} cells a frame: the scopes stood still"
    );
}

const LIVE_ARP_PER_FRAME: f64 = 65_000.0;

#[test]
fn arp_held_shows_its_page_let_go_turns_the_arpeggio_on_and_its_fastest_rate_stays_within_lives_budgets(
) {
    let Some((build, fw)) = common::build_and_firmware("the LIVE arpeggio test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    run_past_boot(&mut m, &fw);
    m.key(ARP, true);
    m.run_ms(200);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "live-arp-page", &m.soc.lcd.fb, &[]);
    m.key(ARP, false);
    m.run_ms(200);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "live-arp", &m.soc.lcd.fb, &[]);
    const ARP_RATE_3F_TO_1F: i32 = -2;
    holding(&mut m, ARP, |m| {
        m.turn(K1, ARP_RATE_3F_TO_1F);
        m.run_ms(60);
    });
    m.run_ms(1200);
    let arp = measure(&mut m, &fw, 75, |m| chord_keys(m, true));
    println!(
        "{}",
        serde_json::json!({ "live_arp_chord": animated_json(&arp) })
    );
    assert!(
        arp.worst_frame_ms() <= SPI_BUDGET_MS,
        "a frame took {:.1} ms of SPI",
        arp.worst_frame_ms()
    );
    let most = arp.frames.iter().map(cells).max().unwrap_or(0);
    assert!(
        most <= sidou_sys::LIVE_VIEW_FRAME_CELLS,
        "a frame sent {most} cells"
    );
    assert!(
        arp.worst_poll < LIVE_WORST_POLL,
        "a ui_poll took {} instructions",
        arp.worst_poll
    );
    assert!(
        arp.instructions_per_frame() < LIVE_ARP_PER_FRAME,
        "{:.0} instructions a frame",
        arp.instructions_per_frame()
    );
}

const LIVE_WORST_POLL: u64 = 68_000;
const LIVE_IDLE_PER_FRAME: f64 = 15_500.0;
const LIVE_CHORD_PER_FRAME: f64 = 53_000.0;
const LIVE_NOISE_PER_FRAME: f64 = 155_000.0;

const SCENE_PIECES: [&str; 6] = ["fire", "plasma", "rule30", "stars", "ant", "rmr"];
const _: () = assert!(SCENE_PIECES.len() == sidou_sys::SCENE_PIECES as usize);
const PIECE_NAME_TOAST_LEFT_MS: u64 = 1_000;

fn open_view_past_its_card(m: &mut Machine, fw: &Firmware, item: i32, view: &str) {
    enter_menu_item(m, fw, item);
    m.run_ms(100);
    assert!(showing_card(m, fw), "no card into {view}");
    past_card(m, fw);
    settle(m, fw);
    assert_eq!(
        top_view(m, fw),
        fw.symbol(view),
        "the menu's item did not open {view}"
    );
}

fn open_scene(m: &mut Machine, fw: &Firmware, piece: usize) {
    open_view_past_its_card(m, fw, MENU_SCENE, "scene_view_view");
    m.run_ms(PIECE_NAME_TOAST_LEFT_MS);
    for _ in 0..piece {
        m.turn(SELECT, 1);
        m.run_ms(TOAST_GONE_MS);
    }
    to_mid_frame(m, fw);
}

#[test]
fn fire_keeps_a_low_flame_in_silence() {
    let Some(fw) = common::firmware("the SCENE test") else {
        return;
    };
    let mut m = booted(&fw);
    open_scene(&mut m, &fw, 0);
    m.run_ms(3000);
    to_mid_frame(&mut m, &fw);
    let px = indexed(&m.soc.lcd.fb);
    const FLAME_ROWS: std::ops::RangeInclusive<usize> = 22..=28;
    let flame = [sidou_sys::GRID_RED as u8, sidou_sys::GRID_LIGHT_RED as u8];
    let lit = FLAME_ROWS
        .flat_map(|r| (0..GRID_COLS).map(move |c| (r, c)))
        .filter(|&(r, c)| flame.iter().any(|&red| cell_colour(&px, r, c, red) > 0))
        .count();
    println!("FIRE in silence: {lit} cells of rows 22-28 are red or light red");
    assert!(lit >= 20, "the idle flame: {lit} cells");
}

#[test]
fn entering_scene_names_the_piece_once_its_card_is_gone() {
    let Some(fw) = common::firmware("the SCENE test") else {
        return;
    };
    let mut m = booted(&fw);
    into_scene_card(&mut m, &fw);
    m.run_ms(100);
    assert!(showing_card(&mut m, &fw));
    assert_ne!(toast_text(&mut m, &fw), "FIRE", "named under the card");
    past_card(&mut m, &fw);
    m.run_ms(100);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("scene_view_view"));
    assert_eq!(toast_text(&mut m, &fw), "FIRE");
}

#[test]
fn algorithm_steps_scenes_piece_as_select_does() {
    let Some(fw) = common::firmware("the SCENE test") else {
        return;
    };
    let mut m = booted(&fw);
    open_scene(&mut m, &fw, 0);
    let which =
        |m: &mut Machine| usize::from(scene_byte(m, &fw, sidou_sys::ilp32::scene_view::which));
    assert_eq!(which(&mut m), 0);
    m.turn(ALGORITHM, 1);
    m.run_ms(100);
    assert_eq!(which(&mut m), 1);
    assert_eq!(toast_text(&mut m, &fw), "PLASMA");
    m.run_ms(TOAST_GONE_MS);
    m.turn(ALGORITHM, -1);
    m.run_ms(100);
    assert_eq!(which(&mut m), 0);
    assert_eq!(toast_text(&mut m, &fw), "FIRE");
}

fn assert_scene_piece_matches_its_golden_and_the_tap_runs_only_under_scene(piece: usize) {
    let Some((build, fw)) = common::build_and_firmware("the SCENE test") else {
        return;
    };
    let mut m = booted(&fw);
    open_scene(&mut m, &fw, 0);
    assert!(tap_on(&mut m, &fw), "the tap is off under SCENE");
    for (k, name) in SCENE_PIECES.iter().enumerate().take(piece + 1) {
        if k > 0 {
            m.turn(SELECT, 1);
            m.run_ms(TOAST_GONE_MS);
            to_mid_frame(&mut m, &fw);
        }
        chord_keys(&mut m, true);
        m.run_ms(1000);
        to_mid_frame(&mut m, &fw);
        if k == piece {
            compare_golden(&build, &format!("scene-{name}"), &m.soc.lcd.fb, &[]);
        }
        chord_keys(&mut m, false);
        m.run_ms(1500);
    }
    press(&mut m, HOME);
    settle(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("views_menu"));
    assert!(!tap_on(&mut m, &fw), "the tap runs under the menu");
}

#[test]
fn scene_fire_matches_its_golden_image() {
    assert_scene_piece_matches_its_golden_and_the_tap_runs_only_under_scene(0);
}

#[test]
fn scene_plasma_matches_its_golden_image() {
    assert_scene_piece_matches_its_golden_and_the_tap_runs_only_under_scene(1);
}

#[test]
fn scene_rule30_matches_its_golden_image() {
    assert_scene_piece_matches_its_golden_and_the_tap_runs_only_under_scene(2);
}

#[test]
fn scene_stars_matches_its_golden_image() {
    assert_scene_piece_matches_its_golden_and_the_tap_runs_only_under_scene(3);
}

#[test]
fn scene_ant_matches_its_golden_image() {
    assert_scene_piece_matches_its_golden_and_the_tap_runs_only_under_scene(4);
}

#[test]
fn scene_rmr_matches_its_golden_image() {
    assert_scene_piece_matches_its_golden_and_the_tap_runs_only_under_scene(SCENE_RMR);
}

fn cells_sent(c: &Cost) -> Vec<u32> {
    c.frames.iter().map(cells).collect()
}

#[test]
fn a_chords_note_ons_move_every_scene_piece_within_two_frames_and_silence_calms_it() {
    let Some(fw) = common::firmware("the SCENE test") else {
        return;
    };
    for (piece, name) in SCENE_PIECES.iter().enumerate() {
        let mut m = booted(&fw);
        open_scene(&mut m, &fw, piece);
        let quiet = cells_sent(&measure(&mut m, &fw, 25, |_| {}));
        let chord = cells_sent(&measure(&mut m, &fw, 25, |m| chord_keys(m, true)));
        chord_keys(&mut m, false);
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
        assert!(
            chord[..3].iter().any(|&n| n > 0),
            "{name}: nothing changed on the note-ons"
        );
        let (num, den) = if *name == "fire" { (3, 2) } else { (3, 1) };
        assert!(
            den * sum(&chord) > num * sum(&quiet).max(10)
                && den * sum(&chord) > num * sum(&after).max(10),
            "{name}: quiet {}, the chord {}, after {}",
            sum(&quiet),
            sum(&chord),
            sum(&after)
        );
    }
}

fn band(m: &mut Machine, f: u64) {
    const ARPEGGIO: [u8; 6] = [57, 60, 64, 69, 72, 76];
    const BASS: [u8; 2] = [53, 55];
    let arp = note_key(ARPEGGIO[(f / 4 % 6) as usize]);
    let bass = note_key(BASS[(f / 8 % 2) as usize]);
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

fn measure_band(m: &mut Machine, fw: &Firmware, frames: u64) -> Cost {
    measure_frames(m, fw, frames, band)
}

fn measure_frames(
    m: &mut Machine,
    fw: &Firmware,
    frames: u64,
    mut act: impl FnMut(&mut Machine, u64),
) -> Cost {
    to_mid_frame(m, fw);
    let from = m.watched_calls.len();
    let mut per = Vec::new();
    for f in 0..frames {
        act(m, f);
        let before = sent(m, fw);
        m.run_ms(UI_FRAME_MS);
        per.push(sent(m, fw).since(before));
    }
    Cost::of(&m.watched_calls[from..], per)
}

const FILTER_WIDE_OPEN: i32 = 64;

fn open_the_filter(m: &mut Machine) {
    m.turn(K1, FILTER_WIDE_OPEN);
    m.run_ms(POPUP_GONE_MS);
}

#[test]
fn scene_with_the_band_and_its_wipes_with_a_chord_stay_within_their_budgets() {
    let Some(fw) = common::firmware("the SCENE budget test") else {
        return;
    };
    let mut report = serde_json::Map::new();
    for (piece, name) in SCENE_PIECES.iter().enumerate() {
        let mut m = Machine::boot(&fw.image);
        m.watch_calls(fw.symbol("ui_poll"));
        run_past_boot(&mut m, &fw);
        open_scene(&mut m, &fw, piece);
        open_the_filter(&mut m);
        let c = measure_band(&mut m, &fw, 150);
        for k in 0..30 {
            band(&mut m, k);
        }
        chord_keys(&mut m, true);
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
        let SceneBound {
            cells_average: cells_bound,
            cells_most: most_bound,
            instructions_a_frame: per_frame_bound,
        } = SCENE_BOUNDS[piece];
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

#[test]
fn midi_learn_over_scene_and_live_stays_within_the_budget_from_opening_to_closing() {
    let Some(fw) = common::firmware("the learn budget test") else {
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
        open_the_filter(&mut m);
        let mut opened = 0;
        let c = measure_frames(&mut m, &fw, 120, |m, f| {
            band(m, f);
            match f {
                10 => m.key(SAVE, true),
                11 => m.turn(K1, 1),
                13 => m.key(SAVE, false),
                90 => {
                    opened = top_view(m, &fw);
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
            top_view(&mut m, &fw),
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

struct SceneBound {
    cells_average: f64,
    cells_most: u32,
    instructions_a_frame: f64,
}

const fn scene_bound(cells_average: f64, cells_most: u32, instructions_a_frame: f64) -> SceneBound {
    SceneBound {
        cells_average,
        cells_most,
        instructions_a_frame,
    }
}

const SCENE_BOUNDS: [SceneBound; 6] = [
    scene_bound(138.0, 175, 108_000.0),
    scene_bound(152.0, sidou_sys::SCENE_VIEW_FRAME_CELLS, 152_000.0),
    scene_bound(21.0, 84, 33_500.0),
    scene_bound(44.0, 75, 79_000.0),
    scene_bound(3.0, 10, 30_000.0),
    scene_bound(133.0, 136, 1_140_000.0),
];
const SCENE_WORST_POLL: u64 = 62_000;
const SCENE_INPUT: u64 = 230;
const SCENE_START_PASS: u64 = 67_000;
const RMR_INPUT: u64 = 1_050;
const RMR_START_PASS: u64 = 65_000;

fn start_cost(m: &mut Machine, ms: u64, act: impl FnOnce(&mut Machine)) -> (u64, u64) {
    let (calls, passes) = (m.watched_calls.len(), m.passes.len());
    act(m);
    m.run_ms(ms);
    let input = m.watched_calls[calls..].iter().copied().max().unwrap_or(0);
    let pass = m.passes[passes..].iter().copied().max().unwrap_or(0);
    (input, pass)
}

fn note_if_over(
    over: &mut Vec<String>,
    what: &str,
    (input, pass): (u64, u64),
    (input_bound, pass_bound): (u64, u64),
) {
    println!("{what}: the input {input}, the busiest pass {pass}");
    if input >= input_bound || pass >= pass_bound {
        over.push(format!(
            "{what}: the input {input}, the busiest pass {pass}"
        ));
    }
}

#[test]
fn starting_a_scene_piece_by_select_or_from_the_menu_holds_the_main_loop_within_the_poll_allowance()
{
    let Some(fw) = common::firmware("the SCENE input test") else {
        return;
    };
    let mut m = booted(&fw);
    set_cards(&mut m, &fw, CARDS_ALWAYS);
    open_scene(&mut m, &fw, 0);
    m.watch_calls(fw.symbol("ui_encoder"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut costs = Vec::new();
    for name in SCENE_PIECES.iter().cycle().skip(1).take(SCENE_PIECES.len()) {
        let c = start_cost(&mut m, TOAST_GONE_MS, |m| m.turn(SELECT, 1));
        costs.push((format!("SELECT to {name}"), c));
    }
    for (piece, name) in SCENE_PIECES.iter().enumerate() {
        if piece > 0 {
            m.turn(SELECT, 1);
            m.run_ms(TOAST_GONE_MS);
        }
        press(&mut m, HOME);
        settle(&mut m, &fw);
        assert_eq!(top_view(&mut m, &fw), fw.symbol("views_menu"));
        let calls = m.watched_calls.len();
        m.turn(ALGORITHM, 1);
        m.run_ms(100);
        let input = m.watched_calls[calls..].iter().copied().max().unwrap_or(0);
        to_held(&mut m, &fw);
        let (_, pass) = start_cost(&mut m, CARD_TIMEOUT_MS, |_| {});
        let c = (input, pass);
        assert_eq!(
            top_view(&mut m, &fw),
            fw.symbol("scene_view_view"),
            "{name}: the menu's SCENE did not open SCENE"
        );
        costs.push((format!("the menu into {name}"), c));
    }
    let mut over = Vec::new();
    for (what, cost) in &costs {
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
        note_if_over(&mut over, what, *cost, (input_bound, pass_bound));
    }
    assert!(over.is_empty(), "{over:#?}");
}

const CHARACTER_AND_SLOTS_OVER_MIDI: [u8; 19 * 3] = {
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

fn open_chip(m: &mut Machine, fw: &Firmware) {
    open_view_past_its_card(m, fw, MENU_CHIP, "chip_view_view");
}

fn chip_focus(m: &mut Machine, fw: &Firmware) -> u8 {
    let tabs = ui_byte(
        m,
        fw,
        sidou_sys::ilp32::views::chip + sidou_sys::ilp32::chip_view::tabs,
    );
    if tabs != 0 {
        return sidou_sys::CHIP_VIEW_TABS as u8;
    }
    if ui_byte(
        m,
        fw,
        sidou_sys::ilp32::views::chip + sidou_sys::ilp32::chip_view::expert,
    ) != 0
    {
        return sidou_sys::CHIP_VIEW_EXPERT as u8;
    }
    let on_chip = ui_byte(
        m,
        fw,
        sidou_sys::ilp32::views::chip + sidou_sys::ilp32::chip_view::chip,
    );
    let slot = live_byte(m, fw, sidou_sys::ilp32::live::fx_select);
    if on_chip != 0 {
        0
    } else {
        1 + slot
    }
}

fn character(m: &mut Machine, fw: &Firmware) -> [u8; 4] {
    std::array::from_fn(|i| live_byte(m, fw, sidou_sys::ilp32::live::character + i))
}

#[test]
fn chip_pages_and_tabs_match_their_golden_images() {
    let Some((build, fw)) = common::build_and_firmware("the CHIP test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&CHARACTER_AND_SLOTS_OVER_MIDI);
    m.run_ms(200);
    open_chip(&mut m, &fw);
    assert_eq!(chip_focus(&mut m, &fw), 0);
    compare_golden(&build, "chip", &m.soc.lcd.fb, &[]);
    for (k, name) in ["dist", "mod", "fx"].iter().enumerate() {
        m.turn(SELECT, 1);
        m.run_ms(TOAST_GONE_MS);
        settle(&mut m, &fw);
        assert_eq!(chip_focus(&mut m, &fw), k as u8 + 1);
        compare_golden(&build, &format!("chip-{name}"), &m.soc.lcd.fb, &[]);
    }
    m.turn(SELECT, -4);
    m.run_ms(100);
    assert_eq!(chip_focus(&mut m, &fw), sidou_sys::CHIP_VIEW_TABS as u8);
    m.turn(ALGORITHM, 1);
    m.run_ms(TOAST_GONE_MS);
    settle(&mut m, &fw);
    compare_golden(&build, "chip-b", &m.soc.lcd.fb, &[]);
    song_on_chip_a(&mut m, &fw);
    m.run_ms(300);
    settle(&mut m, &fw);
    compare_golden(&build, "chip-b-keys", &m.soc.lcd.fb, &[]);
    m.turn(ALGORITHM, -1);
    m.run_ms(300);
    settle(&mut m, &fw);
    compare_golden(&build, "chip-a-song", &m.soc.lcd.fb, &[]);
}

#[test]
fn a_profile_step_on_the_b_tab_during_a_song_reaches_chip_b_and_leaves_chip_a_bit_exact() {
    let Some(fw) = common::firmware("the CHIP B tab test") else {
        return;
    };
    let run = |step: bool| {
        let mut m = booted(&fw);
        song_on_chip_a(&mut m, &fw);
        m.run_ms(100);
        open_chip(&mut m, &fw);
        m.turn(SELECT, -1);
        m.run_ms(50);
        m.turn(ALGORITHM, 1);
        m.run_ms(50);
        m.turn(SELECT, 1);
        m.run_ms(50);
        if step {
            m.turn(ALGORITHM, 1);
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
    assert_eq!(
        (u32::from(b_still), u32::from(b_step)),
        (sidou_sys::SID_MODEL_6581, sidou_sys::SID_MODEL_8580),
        "the step did not reach chip B"
    );
    assert!(
        a_still == a_step,
        "chip A changed with the step on the B tab"
    );
}

#[test]
fn chip_follows_midi_and_the_knobs_there_and_back_and_select_stops_at_the_tabs() {
    let Some((build, fw)) = common::build_and_firmware("the CHIP test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&CHARACTER_AND_SLOTS_OVER_MIDI);
    m.run_ms(200);
    open_chip(&mut m, &fw);
    let golden = indexed(&m.soc.lcd.fb);
    m.midi(&[0xB0, 86, 0]);
    m.run_ms(200);
    to_mid_frame(&mut m, &fw);
    assert_eq!(character(&mut m, &fw), [0, 40, 64, 20]);
    let age_rows = sidou_sys::CHIP_VIEW_VALUE_ROW as usize..=sidou_sys::CHIP_VIEW_BAR_ROW as usize;
    let now = indexed(&m.soc.lcd.fb);
    assert!(
        age_rows
            .clone()
            .any(|r| cell_row_pixels(&now, r) != cell_row_pixels(&golden, r)),
        "the age's column did not change"
    );
    for r in (0..GRID_ROWS).filter(|r| !age_rows.contains(r)) {
        assert!(
            cell_row_pixels(&now, r) == cell_row_pixels(&golden, r),
            "row {r} changed"
        );
    }
    const AGE_0_TO_100_DETENTS: i32 = 25;
    m.turn(K1, AGE_0_TO_100_DETENTS);
    m.run_ms(turn_time(AGE_0_TO_100_DETENTS) + 200);
    assert_eq!(character(&mut m, &fw), [100, 40, 64, 20]);
    m.run_ms(POPUP_GONE_MS);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "chip", &m.soc.lcd.fb, &[]);
    m.turn(SELECT, -3);
    m.run_ms(100);
    assert_eq!(
        chip_focus(&mut m, &fw),
        sidou_sys::CHIP_VIEW_TABS as u8,
        "SELECT did not stop at the tabs, above the chip"
    );
}

const CHIP_IDLE_PER_FRAME: f64 = 7_000.0;
const CHIP_KNOB_PER_FRAME: f64 = 8_500.0;
const CHIP_BUSY_PER_FRAME: f64 = 41_000.0;
const CHIP_WORST_POLL: u64 = 38_000;
const CHIP_INPUT: u64 = 245;
const CHIP_PASS: u64 = 49_000;
const REG_RING_PREPARE_SLICE: u64 = sidou_sys::REG_RING_PREPARE_SLICE as u64;

#[test]
fn chip_idle_a_detent_a_tab_switch_and_a_burst_stay_within_its_budgets() {
    let Some(fw) = common::firmware("the CHIP budget test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    run_past_boot(&mut m, &fw);
    m.midi(&CHARACTER_AND_SLOTS_OVER_MIDI);
    m.run_ms(200);
    open_chip(&mut m, &fw);
    without_popups(&mut m, &fw);
    let idle = measure(&mut m, &fw, 25, |_| {});
    let knob = measure(&mut m, &fw, 25, |m| m.turn(K1, 1));
    m.turn(SELECT, -1);
    m.run_ms(200);
    let tab = measure(&mut m, &fw, 25, |m| m.turn(ALGORITHM, 1));
    m.turn(ALGORITHM, -1);
    m.turn(SELECT, 1);
    m.run_ms(200);
    let mut burst = CHARACTER_AND_SLOTS_OVER_MIDI;
    for v in burst.iter_mut().skip(2).step_by(3) {
        *v = 127 - *v;
    }
    let busy = measure(&mut m, &fw, 50, |m| {
        m.midi(&burst);
        m.turn(SELECT, 1);
        m.midi(&MODEL_8580_CC);
    });
    let report = serde_json::json!({
        "frame_ms": UI_FRAME_MS,
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
        assert_frames_within(
            what,
            c,
            sidou_sys::CHIP_VIEW_FRAME_CELLS,
            CHIP_WORST_POLL,
            per_frame,
        );
    }
    let knob_cells: u32 = knob.frames.iter().map(cells).sum();
    assert!(
        (1..=12).contains(&knob_cells),
        "a detent sent {knob_cells} cells"
    );
    let full = busy
        .frames
        .iter()
        .filter(|f| cells(f) >= sidou_sys::CHIP_VIEW_FRAME_CELLS - 30)
        .count();
    assert!(full >= 1, "the burst never filled a frame");
}

#[test]
fn chip_input_and_its_profile_steps_hold_the_main_loop_within_the_poll_allowance() {
    let Some(fw) = common::firmware("the CHIP input test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&CHARACTER_AND_SLOTS_OVER_MIDI);
    m.run_ms(200);
    open_chip(&mut m, &fw);
    m.watch_calls(fw.symbol("ui_encoder"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut over = Vec::new();
    for (what, enc, steps) in [
        ("SELECT down", SELECT, 1),
        ("ALGORITHM on DIST", ALGORITHM, 1),
        ("K1 on DIST", K1, 1),
        ("SELECT down", SELECT, 2),
        ("ALGORITHM on FX", ALGORITHM, -1),
        ("SELECT to the chip", SELECT, -3),
        ("K1 on the chip", K1, -1),
        ("SELECT to the tabs", SELECT, -1),
        ("ALGORITHM to the B tab", ALGORITHM, 1),
        ("K1 on the tabs", K1, 1),
        ("ALGORITHM to the A tab", ALGORITHM, -1),
        ("SELECT to the chip", SELECT, 1),
    ] {
        let cost = start_cost(&mut m, TOAST_GONE_MS, |m| m.turn(enc, steps));
        note_if_over(
            &mut over,
            &format!("CHIP {what}"),
            cost,
            (CHIP_INPUT, CHIP_PASS),
        );
    }
    for (what, steps) in [("ALGORITHM on the chip", 1), ("ALGORITHM back", -1)] {
        let cost = start_cost(&mut m, TOAST_GONE_MS, |m| m.turn(ALGORITHM, steps));
        note_if_over(
            &mut over,
            &format!("CHIP {what}"),
            cost,
            (CHIP_INPUT, CHIP_PASS + REG_RING_PREPARE_SLICE),
        );
    }
    assert!(over.is_empty(), "{over:#?}");
}

const CHIP_PICK_INPUT: u64 = 400;

#[test]
fn picking_an_algorithm_on_a_fresh_slot_holds_the_main_loop_within_the_poll_allowance() {
    let Some(fw) = common::firmware("the CHIP input test") else {
        return;
    };
    let mut m = booted(&fw);
    open_chip(&mut m, &fw);
    m.watch_calls(fw.symbol("ui_encoder"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut over = Vec::new();
    for (what, enc, steps) in [
        ("SELECT down", SELECT, 1),
        ("ALGORITHM OFF to TAPE", ALGORITHM, 1),
        ("ALGORITHM TAPE to OVERDRIVE", ALGORITHM, 1),
        ("SELECT down", SELECT, 2),
        ("ALGORITHM OFF to DELAY", ALGORITHM, 1),
    ] {
        let cost = start_cost(&mut m, TOAST_GONE_MS, |m| m.turn(enc, steps));
        let input_bound = if enc == ALGORITHM {
            CHIP_PICK_INPUT
        } else {
            CHIP_INPUT
        };
        note_if_over(
            &mut over,
            &format!("CHIP {what}"),
            cost,
            (input_bound, CHIP_PASS),
        );
    }
    assert!(over.is_empty(), "{over:#?}");
}

#[test]
fn expert_pages_are_readable_and_stay_inside_the_chip_frame_and_poll_budgets() {
    let Some((build, fw)) = common::build_and_firmware("the EXPERT page test") else {
        return;
    };
    let mut m = booted(&fw);
    open_chip(&mut m, &fw);
    without_popups(&mut m, &fw);
    m.turn(SELECT, 4);
    m.run_ms(TOAST_SHOWN_MS + 200);
    assert_eq!(chip_focus(&mut m, &fw), sidou_sys::CHIP_VIEW_EXPERT as u8);
    m.watch_calls(fw.symbol("ui_poll"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    for page in 0..4 {
        if page != 0 {
            m.turn(ALGORITHM, 1);
            m.run_ms(600);
        }
        to_mid_frame(&mut m, &fw);
        compare_golden(
            &build,
            &format!("chip-expert-{}", page + 1),
            &m.soc.lcd.fb,
            &[],
        );
        let idle = measure(&mut m, &fw, 25, |_| {});
        println!("EXPERT page {} idle: {}", page + 1, animated_json(&idle));
        assert_eq!(
            idle.total().windows,
            0,
            "an idle EXPERT page must compose nothing"
        );
        assert!(
            idle.instructions_per_frame() < CHIP_IDLE_PER_FRAME,
            "EXPERT idle {} instructions/frame",
            idle.instructions_per_frame()
        );
        let cost = measure(&mut m, &fw, 25, |m| m.turn(K1, 1));
        println!("EXPERT page {}: {}", page + 1, animated_json(&cost));
        assert!(cost.worst_poll < CHIP_WORST_POLL);
        assert!(cost.instructions_per_frame() < CHIP_KNOB_PER_FRAME);
        assert!(cost.worst_frame_ms() <= SPI_BUDGET_MS);
        assert!(
            cost.frames.iter().map(cells).max().unwrap_or(0) <= sidou_sys::CHIP_VIEW_FRAME_CELLS
        );
    }
    let worst = m.passes.iter().copied().max().unwrap_or(0);
    assert!(worst < POLL_INSTRUCTIONS, "EXPERT pass {worst}");
}

fn profile(m: &mut Machine, fw: &Firmware) -> u32 {
    u32::from(live_byte(m, fw, sidou_sys::ilp32::live::profile))
}

const FULL_CHARACTER: [u8; 12] = [0xB0, 86, 127, 0xB0, 87, 127, 0xB0, 89, 127, 0xB0, 90, 127];
const DAC_RAMP_DONE_MS: u64 = 2_500;

#[test]
fn every_chip_reset_at_full_character_is_prepared_in_counted_slices_within_the_poll_allowance() {
    use sidou_sys::{SID_CHIP_6581_R2, SID_CHIP_6581_R4AR, SID_CHIP_6581_WORN, SID_CHIP_8580_R5};
    let Some(fw) = common::firmware("the chip reset test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&FULL_CHARACTER);
    m.run_ms(DAC_RAMP_DONE_MS);
    open_chip(&mut m, &fw);
    assert_eq!(character(&mut m, &fw), [127; 4]);
    assert_eq!(profile(&mut m, &fw), SID_CHIP_6581_R4AR);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    m.watch_calls(fw.symbol("reg_ring_prepare_step"));
    let out = fw.symbol("out");
    let out_u32 = |m: &mut Machine, field: usize| m.soc.read32(out + field as u32);
    let mut costs: Vec<(String, u64, u64)> = Vec::new();
    let spent = fw.chips() + sidou_sys::ilp32::reg_ring_chips::default_8580_spent as u32;
    let mut run = |m: &mut Machine, what: String, want: u32, act: &dyn Fn(&mut Machine)| {
        let slices = out_u32(m, sidou_sys::ilp32::reg_ring_out::slices);
        let model = unsafe { (*sidou_sys::sid_chip_profile_get(want)).model };
        let built = u32::from(model) != sidou_sys::SID_MODEL_8580 || m.soc.read8(spent) != 0;
        let calls = m.watched_calls.len();
        let (_, pass) = start_cost(m, 500, act);
        assert_eq!(profile(m, &fw), want, "{what}");
        let slice = m.watched_calls[calls..].iter().copied().max().unwrap_or(0);
        let n = out_u32(m, sidou_sys::ilp32::reg_ring_out::slices) - slices;
        let c = sidou_sys::sid_character {
            profile: want as u8 + 1,
            age: 127,
            dac: 127,
            thump: 127,
            bleed: 127,
            ..Default::default()
        };
        let mut expect = unsafe { sidou_sys::sid_character_prep_slices(model.into(), &c) };
        if !built {
            expect += sidou_sys::SID_DAC_BUILD_STEPS;
        }
        println!("chip reset, {what}: the busiest pass {pass}, slice {slice}, {n} slices");
        assert_eq!(n, expect, "{what}: the slices");
        costs.push((what, pass, slice));
    };
    let r3 = sidou_sys::SID_CHIP_6581_R3;
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
    let profile_cc_value = |profile: u32| (profile * 128).div_ceil(13) as u8;
    for want in [
        r3,
        SID_CHIP_6581_R4AR,
        SID_CHIP_8580_R5,
        SID_CHIP_6581_R2,
        SID_CHIP_6581_WORN,
    ] {
        let v = profile_cc_value(want);
        run(&mut m, format!("CC 85 {v} to profile {want}"), want, &|m| {
            m.midi(&[0xB0, 85, v])
        });
    }
    for (v, want) in [(127u8, SID_CHIP_8580_R5), (0, SID_CHIP_6581_WORN)] {
        run(&mut m, format!("CC 25 {v} to profile {want}"), want, &|m| {
            m.midi(&[0xB0, 25, v])
        });
    }
    let worst = costs.iter().max_by_key(|c| c.1).unwrap();
    println!("chip reset, the worst pass: {} ({})", worst.1, worst.0);
    let worst = costs.iter().max_by_key(|c| c.2).unwrap();
    println!("chip reset, the worst slice: {} ({})", worst.2, worst.0);
    let over: Vec<_> = costs
        .iter()
        .filter(|c| c.1 >= POLL_INSTRUCTIONS || c.2 >= REG_RING_PREPARE_SLICE)
        .collect();
    assert!(over.is_empty(), "{over:#?}");
    assert_eq!(
        out_u32(&mut m, sidou_sys::ilp32::reg_ring_out::unprepared_resets),
        1,
        "a reset was prepared at once"
    );
}

#[test]
fn a_dac_sweep_during_a_profile_step_holds_every_pass_within_the_allowance() {
    let Some(fw) = common::firmware("the profile step and DAC sweep test") else {
        return;
    };
    let mut m = booted(&fw);
    m.midi(&NOISE_AT_FULL_SUSTAIN);
    chord_keys(&mut m, true);
    m.run_ms(300);
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut worst = 0;
    let mut dac = 0u8;
    const CC85_WORN: u8 = 43;
    const CC85_R4AR: u8 = 20;
    for (phase, profile) in [
        (0u64, CC85_WORN),
        (3, CC85_R4AR),
        (7, CC85_WORN),
        (11, CC85_R4AR),
        (13, CC85_WORN),
        (17, CC85_R4AR),
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
        worst <= POLL_INSTRUCTIONS,
        "a pass took {worst} instructions, over the poll allowance"
    );
}

fn into_card_of(m: &mut Machine, fw: &Firmware, item: i32) {
    open_menu_settled(m, fw);
    select_menu_item(m, item);
    m.run_ms(MENU_ITEM_SETTLE_MS);
    to_mid_frame(m, fw);
    m.turn(ALGORITHM, 1);
}

fn into_scene_card(m: &mut Machine, fw: &Firmware) {
    into_card_of(m, fw, MENU_SCENE);
}

fn into_chip_card(m: &mut Machine, fw: &Firmware) {
    into_card_of(m, fw, MENU_CHIP);
}

fn tap_to_oscillator_card(m: &mut Machine, fw: &Firmware) {
    set_cards(m, fw, CARDS_ALWAYS);
    open_menu_settled(m, fw);
    to_mid_frame(m, fw);
    m.key(HOME, true);
    m.run_ms(30);
    m.key(HOME, false);
}

fn into_oscillator_card(m: &mut Machine, fw: &Firmware) {
    tap_to_oscillator_card(m, fw);
    m.run_ms(2 * UI_FRAME_MS);
}

fn to_held(m: &mut Machine, fw: &Firmware) {
    let from = m.ms();
    while card_byte(m, fw, sidou_sys::ilp32::title_card::phase) != sidou_sys::TITLE_CARD_HOLD as u8
    {
        assert!(showing_card(m, fw), "the card ended before it held");
        assert!(
            m.ms() < from + CARD_TIMEOUT_MS,
            "the card did not reach its hold"
        );
        m.run_ms(UI_FRAME_MS);
    }
    m.run_ms(UI_FRAME_MS);
    to_mid_frame(m, fw);
    assert!(showing_card(m, fw));
}

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

#[test]
fn title_cards_match_their_goldens_and_their_sources_and_cut_to_their_views() {
    let Some((build, fw)) = common::build_and_firmware("the title card test") else {
        return;
    };
    let mut m = booted(&fw);
    set_cards(&mut m, &fw, CARDS_ALWAYS);
    into_chip_card(&mut m, &fw);
    m.run_ms(UI_FRAME_MS);
    assert!(showing_card(&mut m, &fw), "no card into CHIP");
    assert_eq!(
        card_byte(&mut m, &fw, sidou_sys::ilp32::title_card::id),
        sidou_sys::TITLE_CARD_CHIP as u8
    );
    to_held(&mut m, &fw);
    compare_golden(&build, "title-chip", &m.soc.lcd.fb, &[]);
    assert_shows_the_card_source("chip", &m.soc.lcd.fb);
    past_card(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("chip_view_view"));
    press(&mut m, HOME);
    settle(&mut m, &fw);
    press(&mut m, HOME);
    past_card(&mut m, &fw);
    settle(&mut m, &fw);
    into_scene_card(&mut m, &fw);
    m.run_ms(UI_FRAME_MS);
    assert!(showing_card(&mut m, &fw), "no card into SCENE");
    assert_eq!(
        card_byte(&mut m, &fw, sidou_sys::ilp32::title_card::id),
        sidou_sys::TITLE_CARD_SCENE as u8
    );
    to_held(&mut m, &fw);
    compare_golden(&build, "title-scene", &m.soc.lcd.fb, &[]);
    assert_shows_the_card_source("scene", &m.soc.lcd.fb);
    past_card(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("scene_view_view"));
    press(&mut m, HOME);
    settle(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("views_menu"));
    press(&mut m, HOME);
    m.run_ms(UI_FRAME_MS);
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
    assert_eq!(top_view(&mut m, &fw), fw.symbol("live_view_view"));
    compare_golden(&build, "live", &m.soc.lcd.fb, &[]);
}

#[test]
fn by_default_the_chips_card_plays_once_and_the_oscillators_never() {
    let Some(fw) = common::firmware("the title card test") else {
        return;
    };
    let mut m = booted(&fw);
    into_chip_card(&mut m, &fw);
    m.run_ms(UI_FRAME_MS);
    assert!(showing_card(&mut m, &fw), "no card the first time");
    past_card(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("chip_view_view"));
    press(&mut m, HOME);
    settle(&mut m, &fw);
    m.turn(ALGORITHM, 1);
    m.run_ms(2 * UI_FRAME_MS);
    assert!(!showing_card(&mut m, &fw), "a card the second time");
    settle(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("chip_view_view"));
    press(&mut m, HOME);
    settle(&mut m, &fw);
    press(&mut m, HOME);
    m.run_ms(2 * UI_FRAME_MS);
    assert!(!showing_card(&mut m, &fw), "THE OSCILLATOR under ONCE");
    settle(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("live_view_view"));
}

#[test]
fn cards_always_plays_every_card_every_time_and_off_plays_none() {
    let Some(fw) = common::firmware("the title card test") else {
        return;
    };
    let mut m = booted(&fw);
    set_cards(&mut m, &fw, CARDS_ALWAYS);
    for _ in 0..2 {
        into_chip_card(&mut m, &fw);
        m.run_ms(2 * UI_FRAME_MS);
        assert!(showing_card(&mut m, &fw), "ALWAYS: no THE CHIP");
        past_card(&mut m, &fw);
        press(&mut m, HOME);
        settle(&mut m, &fw);
        press(&mut m, HOME);
        m.run_ms(2 * UI_FRAME_MS);
        assert!(showing_card(&mut m, &fw), "ALWAYS: no THE OSCILLATOR");
        past_card(&mut m, &fw);
    }
    let mut m = booted(&fw);
    set_cards(&mut m, &fw, CARDS_OFF);
    into_scene_card(&mut m, &fw);
    m.run_ms(2 * UI_FRAME_MS);
    assert!(!showing_card(&mut m, &fw), "OFF: a card");
    settle(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("scene_view_view"));
}

fn showing_help(m: &mut Machine, fw: &Firmware) -> bool {
    top_view(m, fw) == fw.symbol("help_view_view")
}

const HELP_HOLD_MS: u64 = 560;
const HELP_TIMEOUT_MS: u64 = 1_500;
const HELP_DRAWN_MS: u64 = 400;

fn hold_for_help(m: &mut Machine, fw: &Firmware) {
    m.key(HOME, true);
    let from = m.ms();
    while !showing_help(m, fw) {
        assert!(
            m.ms() < from + HELP_TIMEOUT_MS,
            "no help card after {} ms",
            m.ms() - from
        );
        m.run_ms(UI_FRAME_MS);
    }
    assert!(
        m.ms() - from >= HELP_HOLD_MS,
        "the card came after {} ms",
        m.ms() - from
    );
    m.run_ms(HELP_DRAWN_MS);
    to_mid_frame(m, fw);
}

fn let_go_of_home_and_close_help_with_k1(m: &mut Machine, fw: &Firmware) {
    m.key(HOME, false);
    m.run_ms(200);
    assert!(showing_help(m, fw), "the release closed the card");
    m.turn(K1, 1);
    m.run_ms(TRANSITION_END_MS);
    assert!(!showing_help(m, fw), "K1 did not close the card");
}

fn fx_select(m: &mut Machine, fw: &Firmware) -> u8 {
    live_byte(m, fw, sidou_sys::ilp32::live::fx_select)
}

#[test]
fn holding_home_shows_the_help_card_and_a_tap_still_goes_home() {
    let Some((build, fw)) = common::build_and_firmware("the help card test") else {
        return;
    };
    let mut m = booted(&fw);
    press(&mut m, HOME);
    settle(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("views_menu"));
    press(&mut m, HOME);
    settle(&mut m, &fw);
    assert_eq!(
        top_view(&mut m, &fw),
        fw.symbol("live_view_view"),
        "no card back"
    );
    let fx = fx_select(&mut m, &fw);
    hold_for_help(&mut m, &fw);
    compare_golden(&build, "help-live", &m.soc.lcd.fb, &[]);
    let_go_of_home_and_close_help_with_k1(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("live_view_view"));
    assert_eq!(fx_select(&mut m, &fw), fx, "the selection untouched");
}

#[test]
fn help_cards_match_their_goldens() {
    let Some((build, fw)) = common::build_and_firmware("the help card test") else {
        return;
    };
    for name in [
        "menu", "settings", "chip", "scene", "step", "tracker", "order", "instr", "table",
        "status", "about", "charset", "midi", "song", "files", "saveas", "confirm",
    ] {
        let mut m = booted(&fw);
        match name {
            "menu" => open_menu_settled(&mut m, &fw),
            "settings" => {
                open_menu_settled(&mut m, &fw);
                settings_item(&mut m, sidou_sys::VIEWS_SETTINGS_KEYS);
            }
            "about" => enter_menu_item(&mut m, &fw, MENU_ABOUT),
            "charset" => enter_menu_item(&mut m, &fw, MENU_CHARSET),
            "files" | "saveas" | "confirm" => {
                if name == "confirm" {
                    press(&mut m, SEQ);
                    settle(&mut m, &fw);
                    press(&mut m, KEY0);
                    settle(&mut m, &fw);
                    press(&mut m, HOME);
                    settle(&mut m, &fw);
                }
                enter_menu_item(&mut m, &fw, MENU_SONG);
                settle(&mut m, &fw);
                const SONG_LOAD: i32 = 0;
                const SONG_NEW: i32 = 1;
                const SONG_SAVE_AS: i32 = 2;
                let action = match name {
                    "files" => SONG_LOAD,
                    "confirm" => SONG_NEW,
                    _ => SONG_SAVE_AS,
                };
                m.turn(SELECT, action);
                m.run_ms(turn_time(action) + 50);
                m.turn(ALGORITHM, 1);
            }
            "chip" | "scene" => {
                if name == "chip" {
                    into_chip_card(&mut m, &fw);
                } else {
                    into_scene_card(&mut m, &fw);
                }
                m.run_ms(2 * UI_FRAME_MS);
                assert!(showing_card(&mut m, &fw));
                past_card(&mut m, &fw);
            }
            "status" => open_status(&mut m, &fw),
            "midi" => enter_menu_item(&mut m, &fw, MENU_MIDI),
            "song" => enter_menu_item(&mut m, &fw, MENU_SONG),
            _ => {
                let seqs = ["step", "tracker", "order", "instr", "table"]
                    .iter()
                    .position(|n| *n == name)
                    .unwrap();
                for _ in 0..=seqs {
                    press(&mut m, SEQ);
                    settle(&mut m, &fw);
                }
            }
        }
        settle(&mut m, &fw);
        hold_for_help(&mut m, &fw);
        compare_golden(&build, &format!("help-{name}"), &m.soc.lcd.fb, &[]);
        if matches!(name, "step" | "tracker" | "order" | "instr" | "table") {
            m.turn(SELECT, 1);
            m.run_ms(400);
            to_mid_frame(&mut m, &fw);
            assert!(showing_help(&mut m, &fw), "SELECT closed {name}'s card");
            compare_golden(&build, &format!("help-{name}-2"), &m.soc.lcd.fb, &[]);
        }
        let_go_of_home_and_close_help_with_k1(&mut m, &fw);
    }
}

#[test]
fn the_states_help_cards_match_their_goldens() {
    let Some((build, fw)) = common::build_and_firmware("the help card test") else {
        return;
    };
    for name in [
        "live-2",
        "live-slot",
        "chip-tabs",
        "chip-slot",
        "expert",
        "step-held",
        "step-rec",
        "tracker-edit",
        "tracker-rec",
        "store",
    ] {
        let mut m = booted(&fw);
        match name {
            "live-2" => {}
            "live-slot" => {
                m.turn(SELECT, 1);
                m.run_ms(100);
            }
            "chip-tabs" | "chip-slot" | "expert" => {
                if name == "chip-slot" {
                    m.turn(SELECT, 1);
                    m.run_ms(100);
                    m.turn(ALGORITHM, OFF_TO_OVERDRIVE);
                    m.run_ms(100);
                    m.turn(SELECT, -1);
                    m.run_ms(100);
                }
                into_chip_card(&mut m, &fw);
                m.run_ms(2 * UI_FRAME_MS);
                past_card(&mut m, &fw);
                m.turn(SELECT, -8);
                m.run_ms(100);
                let to = match name {
                    "chip-tabs" => 0,
                    "chip-slot" => 2,
                    _ => 5,
                };
                m.turn(SELECT, to);
                m.run_ms(100);
            }
            "step-held" | "step-rec" => {
                press(&mut m, SEQ);
                settle(&mut m, &fw);
                if name == "step-held" {
                    m.key(KEY0 + 5, true);
                    m.run_ms(100);
                } else {
                    press(&mut m, PLAY);
                    press(&mut m, REC);
                }
            }
            "tracker-edit" | "tracker-rec" => {
                press(&mut m, SEQ);
                settle(&mut m, &fw);
                press(&mut m, SEQ);
                settle(&mut m, &fw);
                if name == "tracker-rec" {
                    press(&mut m, PLAY);
                }
                press(&mut m, REC);
            }
            _ => {
                open_menu_settled(&mut m, &fw);
                settings_item(&mut m, sidou_sys::VIEWS_SETTINGS_STORE);
            }
        }
        settle(&mut m, &fw);
        hold_for_help(&mut m, &fw);
        if name == "live-2" {
            m.turn(SELECT, 1);
            m.run_ms(400);
            to_mid_frame(&mut m, &fw);
        }
        compare_golden(&build, &format!("help-{name}"), &m.soc.lcd.fb, &[]);
        let_go_of_home_and_close_help_with_k1(&mut m, &fw);
        if name == "step-held" {
            m.key(KEY0 + 5, false);
        }
    }
}

#[test]
fn the_help_card_over_live_and_the_menu_stays_within_the_frame_budget_and_sends_nothing_once_drawn()
{
    let Some(fw) = common::firmware("the help card budget test") else {
        return;
    };
    for over_menu in [false, true] {
        let mut m = booted(&fw);
        if over_menu {
            open_menu_settled(&mut m, &fw);
        } else {
            m.midi(&NOISE_AT_FULL_SUSTAIN);
            chord_keys(&mut m, true);
        }
        settle(&mut m, &fw);
        let c = measure(&mut m, &fw, 40, |m| m.key(HOME, true));
        let per = cells_sent(&c);
        let most = per.iter().copied().max().unwrap_or(0);
        println!(
            "help card over {}: at most {most} cells, {per:?}",
            if over_menu { "the menu" } else { "LIVE" }
        );
        assert!(showing_help(&mut m, &fw));
        assert!(most <= FRAME_CELLS_MOST, "a frame sent {most} cells");
        assert!(c.worst_frame_ms() <= SPI_BUDGET_MS);
        assert_eq!(per.last().copied(), Some(0), "the drawn card still sends");
    }
}

#[test]
fn a_control_skips_a_title_card_and_does_nothing_else() {
    let Some(fw) = common::firmware("the title card test") else {
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
            top_view(&mut m, &fw),
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

#[test]
fn a_note_plays_through_a_title_card_and_ends_it() {
    let Some(fw) = common::firmware("the title card test") else {
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
        let peak = left_peak_since(&m, from);
        assert!(
            peak > AUDIBLE_PEAK,
            "midi {midi}: the note did not play: peak {peak}"
        );
        assert!(
            !showing_card(&mut m, &fw),
            "midi {midi}: the note did not end the card"
        );
        settle(&mut m, &fw);
        assert_eq!(top_view(&mut m, &fw), fw.symbol("live_view_view"));
    }
}

#[test]
fn title_cards_and_the_inputs_that_bring_them_stay_within_their_budgets() {
    let Some(fw) = common::firmware("the title card budget test") else {
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
            _ => tap_to_oscillator_card(m, &fw),
        };
        let mut m = booted(&fw);
        m.watch_calls(fw.symbol("ui_poll"));
        m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
        enter(&mut m);
        let from = (m.watched_calls.len(), m.passes.len());
        let mut composes = Composes::new(&mut m, &fw);
        let t0 = m.ms();
        loop {
            composes.run(&mut m, &fw, UI_FRAME_MS);
            if !showing_card(&mut m, &fw) && !screen_animating(&mut m, &fw) {
                break;
            }
            assert!(
                m.ms() < t0 + CARD_TIMEOUT_MS,
                "THE {name}: the card did not end"
            );
        }
        let frames = composes.end(&mut m, &fw);
        let c = Cost::of(&m.watched_calls[from.0..], frames);
        let pass = m.passes[from.1..].iter().copied().max().unwrap_or(0);
        let mut m = booted(&fw);
        m.watch_calls(fw.symbol(input));
        let bring_the_card_in = |m: &mut Machine| -> usize {
            if name == "oscillator" {
                set_cards(m, &fw, CARDS_ALWAYS);
                open_menu_settled(m, &fw);
                let n = m.watched_calls.len();
                press(m, HOME);
                n
            } else {
                open_menu_settled(m, &fw);
                select_menu_item(
                    m,
                    if name == "scene" {
                        MENU_SCENE
                    } else {
                        MENU_CHIP
                    },
                );
                m.run_ms(MENU_ITEM_SETTLE_MS);
                let n = m.watched_calls.len();
                m.turn(ALGORITHM, 1);
                n
            }
        };
        let n = bring_the_card_in(&mut m);
        m.run_ms(100);
        assert!(showing_card(&mut m, &fw));
        report.insert(
            format!("{name}_input"),
            serde_json::json!(m.watched_calls[n..].iter().max()),
        );
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
        assert!(
            (60..=72).contains(&per.len()),
            "THE {name}: {} frames",
            per.len()
        );
    }
    println!("{}", serde_json::Value::Object(report));
}

const TITLE_CARD_WORST_POLL: u64 = 79_000;
const TITLE_CARD_PER_FRAME: f64 = 62_000.0;
const TITLE_CARD_INPUT: u64 = 430;
const TITLE_CARD_PASS: u64 = 80_000;

fn song_byte(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    ui_byte(m, fw, sidou_sys::ilp32::views::song + field)
}

fn live_voices_gated(m: &mut Machine, fw: &Firmware) -> usize {
    (0..3)
        .filter(|v| {
            let at = sidou_sys::ilp32::live::voice
                + v * sidou_sys::ilp32::live_voice::SIZE
                + sidou_sys::ilp32::live_voice::gate;
            live_byte(m, fw, at) != 0
        })
        .count()
}

fn open_tracker(m: &mut Machine, fw: &Firmware) {
    press(m, SEQ);
    press(m, SEQ);
    settle(m, fw);
    assert_eq!(
        top_view(m, fw),
        fw.symbol("tracker_view_view"),
        "SEQ did not open the song views"
    );
}

#[test]
fn the_tracker_auditions_shows_voices_plays_records_and_mutes_as_its_goldens() {
    let Some((build, fw)) = common::build_and_firmware("the song views test") else {
        return;
    };
    let mut m = booted(&fw);
    open_tracker(&mut m, &fw);
    compare_golden(&build, "tracker", &m.soc.lcd.fb, &[]);
    m.key(A4_KEY, true);
    m.run_ms(120);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "tracker-audition", &m.soc.lcd.fb, &[]);
    m.key(A4_KEY, false);
    m.run_ms(200);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "tracker", &m.soc.lcd.fb, &[]);
    for name in ["tracker-v2", "tracker-v3"] {
        m.turn(ALGORITHM, 1);
        m.run_ms(200);
        to_mid_frame(&mut m, &fw);
        compare_golden(&build, name, &m.soc.lcd.fb, &[]);
    }
    m.turn(ALGORITHM, 1);
    m.run_ms(200);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "tracker", &m.soc.lcd.fb, &[]);
    press(&mut m, PLAY);
    m.run_ms(2_000);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "tracker-play", &m.soc.lcd.fb, &[]);
    press(&mut m, REC);
    assert_eq!(song_byte(&mut m, &fw, sidou_sys::ilp32::song_view::rec), 1);
    let calls = fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::calls as u32;
    let calls_at_rec = m.soc.read32(calls);
    const VOICE1_MUTED: u8 = 1;
    m.key(GLO, true);
    m.run_ms(30);
    to_mid_frame(&mut m, &fw);
    m.key(KEY0, true);
    m.run_ms(10);
    assert_eq!(
        song_byte(&mut m, &fw, sidou_sys::ilp32::song_view::mute),
        VOICE1_MUTED,
        "the mute within the key's pass"
    );
    m.run_ms(UI_FRAME_MS);
    assert_eq!(
        live_voices_gated(&mut m, &fw),
        0,
        "a key under GLO sounds nothing"
    );
    m.key(KEY0, false);
    m.key(GLO, false);
    m.run_ms(200);
    to_mid_frame(&mut m, &fw);
    assert_eq!(live_voices_gated(&mut m, &fw), 0);
    let played = m.soc.read32(calls) - calls_at_rec;
    assert!(
        played >= 10,
        "the song stood still under REC: {played} calls"
    );
    compare_golden(&build, "tracker-mute-rec", &m.soc.lcd.fb, &[]);
    press(&mut m, PLAY);
    press(&mut m, HOME);
    settle(&mut m, &fw);
    assert_eq!(top_view(&mut m, &fw), fw.symbol("live_view_view"));
    compare_golden(&build, "live-mutes", &m.soc.lcd.fb, &[]);
    m.key(KEY0, true);
    m.run_ms(30);
    assert_eq!(live_voices_gated(&mut m, &fw), 1);
    m.key(KEY0, false);
    m.run_ms(30);
}

const TRACKER_IDLE_PER_FRAME: f64 = 7_900.0;
const TRACKER_CHANGE_PER_FRAME: f64 = 10_100.0;
const TRACKER_PLAYING_PER_FRAME: f64 = 26_600.0;
const TRACKER_WORST_POLL: u64 = 33_000;
const TRACKER_INPUT: u64 = 4_600;
const TRACKER_PASS: u64 = 55_000;

#[test]
fn the_tracker_idle_rec_playing_and_muting_and_its_inputs_stay_within_their_budgets() {
    let Some(fw) = common::firmware("the song views budget test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    run_past_boot(&mut m, &fw);
    open_tracker(&mut m, &fw);
    let idle = measure(&mut m, &fw, 25, |_| {});
    let rec = measure(&mut m, &fw, 25, |m| m.key(REC, true));
    m.key(REC, false);
    m.run_ms(60);
    press(&mut m, REC);
    assert_eq!(song_byte(&mut m, &fw, sidou_sys::ilp32::song_view::rec), 0);
    let playing = measure(&mut m, &fw, 100, |m| m.key(PLAY, true));
    m.key(PLAY, false);
    m.run_ms(100);
    press(&mut m, PLAY);
    m.run_ms(470);
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
        assert_frames_within(
            what,
            c,
            sidou_sys::SONG_VIEW_FRAME_CELLS,
            TRACKER_WORST_POLL,
            per_frame,
        );
    }

    let mut m = booted(&fw);
    m.watch_calls(fw.symbol("ui_buttons"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut over = Vec::new();
    let mut costs = Vec::new();
    for (what, id) in [
        ("SEQ from LIVE", SEQ),
        ("SEQ in STEP", SEQ),
        ("REC", REC),
        ("PLAY", PLAY),
        ("REC again", REC),
    ] {
        let c = start_cost(&mut m, 1000, |m| press(m, id));
        costs.push((what, c));
    }
    m.key(GLO, true);
    m.run_ms(30);
    let c = start_cost(&mut m, 1000, |m| {
        m.key(KEY0, true);
        m.run_ms(30);
        m.key(KEY0, false);
    });
    costs.push(("GLO and a key", c));
    m.key(GLO, false);
    for (what, cost) in &costs {
        note_if_over(
            &mut over,
            &format!("TRACKER {what}"),
            *cost,
            (TRACKER_INPUT, TRACKER_PASS),
        );
    }
    assert!(over.is_empty(), "{over:#?}");
}

fn open_step(m: &mut Machine, fw: &Firmware) {
    press(m, SEQ);
    settle(m, fw);
    assert_eq!(
        top_view(m, fw),
        fw.symbol("step_view_view"),
        "SEQ did not open STEP"
    );
}

fn hold_step_and_lock(m: &mut Machine, step: u8, enc: u8, clicks: i32) {
    m.key(KEY0 + step, true);
    m.run_ms(60);
    m.turn(enc, clicks);
    m.run_ms(200);
}

fn release_step(m: &mut Machine, step: u8) {
    m.key(KEY0 + step, false);
    m.run_ms(200);
}

fn hold_edit_and_scl_for_dice(m: &mut Machine) {
    m.key(EDIT, true);
    m.run_ms(60);
    m.key(SCL, true);
    m.run_ms(60);
}

fn release_scl_and_edit(m: &mut Machine) {
    m.key(SCL, false);
    m.key(EDIT, false);
    m.run_ms(400);
}

#[test]
fn the_step_view_held_locked_dice_and_playing_match_their_goldens() {
    let Some((build, fw)) = common::build_and_firmware("the STEP golden test") else {
        return;
    };
    let mut m = booted(&fw);
    open_step(&mut m, &fw);
    compare_golden(&build, "step", &m.soc.lcd.fb, &[]);
    hold_step_and_lock(&mut m, 5, K1, 6);
    m.run_ms(POPUP_CLOSED_MS);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "step-held", &m.soc.lcd.fb, &[]);
    release_step(&mut m, 5);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "step-lock", &m.soc.lcd.fb, &[]);
    hold_edit_and_scl_for_dice(&mut m);
    m.turn(K1, 1);
    m.run_ms(100);
    m.turn(K3, 4);
    m.run_ms(POPUP_CLOSED_MS);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "step-dice", &m.soc.lcd.fb, &[]);
    release_scl_and_edit(&mut m);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "step-lock", &m.soc.lcd.fb, &[]);
    press(&mut m, PLAY);
    m.run_ms(2_000);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "step-play", &m.soc.lcd.fb, &[]);
}

#[test]
fn the_tracker_dice_frame_matches_its_golden_and_a_turn_without_a_roll_leaves_the_rows() {
    let Some((build, fw)) = common::build_and_firmware("the TRACKER DICE golden test") else {
        return;
    };
    let mut m = booted(&fw);
    open_tracker(&mut m, &fw);
    hold_edit_and_scl_for_dice(&mut m);
    const RHYTHM_TO_3_3_2: i32 = 2;
    m.turn(K4, RHYTHM_TO_3_3_2);
    m.run_ms(100);
    m.key(A4_KEY, true);
    m.run_ms(100);
    m.key(A4_KEY, false);
    m.run_ms(POPUP_CLOSED_MS);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "tracker-dice", &m.soc.lcd.fb, &[]);
    release_scl_and_edit(&mut m);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "tracker", &m.soc.lcd.fb, &[]);
}

fn load_song(m: &mut Machine, fw: &Firmware, bytes: &[u8]) {
    let base = fw.symbol("__song_region");
    for (i, &b) in bytes.iter().enumerate() {
        m.soc.write8(base + i as u32, b);
    }
}

fn song_view_playing(
    fw: &Firmware,
    song: Option<&sidou_tests::song::Song>,
    ms: u64,
    open: impl FnOnce(&mut Machine),
) -> (Cost, u32) {
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    run_past_boot(&mut m, fw);
    if let Some(s) = song {
        load_song(&mut m, fw, s.bytes());
    }
    open(&mut m);
    press(&mut m, PLAY);
    m.run_ms(500);
    let held_at = fw.symbol("ui")
        + (sidou_sys::ilp32::views::song + sidou_sys::ilp32::song_view::held) as u32;
    let held = m.soc.read32(held_at);
    let c = measure(&mut m, fw, ms / UI_FRAME_MS, |_| {});
    (c, m.soc.read32(held_at) - held)
}

fn step_run(fw: &Firmware, song: Option<&sidou_tests::song::Song>, ms: u64) -> (Cost, u32) {
    song_view_playing(fw, song, ms, |m| open_step(m, fw))
}

#[test]
fn the_step_view_idle_locks_taps_pages_playing_and_its_inputs_stay_within_their_budgets() {
    let Some(fw) = common::firmware("the STEP budget test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    run_past_boot(&mut m, &fw);
    open_step(&mut m, &fw);
    let idle = measure(&mut m, &fw, 25, |_| {});
    m.key(KEY0 + 5, true);
    m.run_ms(60);
    let lock = measure(&mut m, &fw, 25, |m| m.turn(K1, 6));
    m.key(KEY0 + 5, false);
    m.run_ms(300);
    let tap = measure(&mut m, &fw, 25, |m| {
        m.key(KEY0 + 9, true);
        m.run_ms(30);
        m.key(KEY0 + 9, false);
    });
    let page = measure(&mut m, &fw, 25, |m| m.turn(ALGORITHM, 1));
    let playing = measure(&mut m, &fw, 100, |m| m.key(PLAY, true));
    m.key(PLAY, false);
    let report = serde_json::json!({
        "step_idle": animated_json(&idle),
        "step_lock": animated_json(&lock),
        "step_tap": animated_json(&tap),
        "step_page": animated_json(&page),
        "step_playing": animated_json(&playing),
    });
    println!("{report}");
    assert_eq!(idle.total().windows, 0, "the idle STEP sent something");
    for (what, c, per_frame) in [
        ("idle", &idle, STEP_IDLE_PER_FRAME),
        ("lock", &lock, STEP_CHANGE_PER_FRAME),
        ("tap", &tap, STEP_CHANGE_PER_FRAME),
        ("page", &page, STEP_CHANGE_PER_FRAME),
        ("playing", &playing, STEP_PLAYING_PER_FRAME),
    ] {
        let most = c.frames.iter().map(cells).max().unwrap_or(0);
        println!(
            "STEP {what}: {:.0} instructions a frame, busiest ui_poll {}, most cells {most}, \
             worst SPI {:.2} ms",
            c.instructions_per_frame(),
            c.worst_poll,
            c.worst_frame_ms()
        );
        assert_frames_within(
            what,
            c,
            sidou_sys::SONG_VIEW_FRAME_CELLS,
            STEP_WORST_POLL,
            per_frame,
        );
    }

    let mut m = booted(&fw);
    m.watch_calls(fw.symbol("ui_buttons"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let mut costs = Vec::new();
    for (what, button, ms) in [
        ("SEQ from LIVE", SEQ, 1000),
        ("ARP (gate)", ARP, 300),
        ("PLAY", PLAY, 1000),
        ("a step key", KEY0 + 3, 1000),
    ] {
        costs.push((what, start_cost(&mut m, ms, |m| press(m, button))));
    }
    m.watch_calls(fw.symbol("ui_encoder"));
    m.key(KEY0 + 6, true);
    m.run_ms(60);
    for (what, enc, steps) in [("K1 lock", K1, 3), ("K2 lock", K2, 2), ("K3 lock", K3, 4)] {
        costs.push((what, start_cost(&mut m, 300, |m| m.turn(enc, steps))));
    }
    costs.push(("K4 set", start_cost(&mut m, 500, |m| m.turn(K4, 2))));
    costs.push((
        "PRESETS note",
        start_cost(&mut m, 500, |m| m.turn(PRESETS, 3)),
    ));
    m.key(KEY0 + 6, false);
    m.run_ms(300);
    costs.push((
        "ALGORITHM page",
        start_cost(&mut m, 300, |m| m.turn(ALGORITHM, 1)),
    ));
    let mut over = Vec::new();
    for (what, cost) in &costs {
        note_if_over(
            &mut over,
            &format!("STEP {what}"),
            *cost,
            (STEP_INPUT, STEP_PASS),
        );
    }
    assert!(over.is_empty(), "{over:#?}");
}

#[test]
fn the_step_view_stays_within_its_budgets_while_a_song_plays() {
    let Some(fw) = common::firmware("the STEP playing budget test") else {
        return;
    };
    let fast = tempo_3_song();
    let mut over = Vec::new();
    for (name, song) in [("loop", None), ("tempo3", Some(&fast))] {
        let (c, held) = step_run(&fw, song, 30_000);
        let per: Vec<u32> = c.frames.iter().map(cells).collect();
        let most = per.iter().copied().max().unwrap_or(0);
        let avg = f64::from(per.iter().sum::<u32>()) / per.len() as f64;
        println!(
            "STEP {name}: cells a frame {avg:.1} (most {most}), SPI {:.2} ms a frame (worst \
             {:.2}), {:.1}k instructions a frame, busiest ui_poll {:.1}k, {held} of {} frames cut \
             short",
            c.total().spi_ms() / per.len() as f64,
            c.worst_frame_ms(),
            c.instructions_per_frame() / 1000.0,
            c.worst_poll as f64 / 1000.0,
            per.len()
        );
        if most > sidou_sys::SONG_VIEW_FRAME_CELLS
            || c.worst_frame_ms() > SPI_BUDGET_MS
            || held != 0
            || c.worst_poll >= STEP_WORST_POLL
            || c.instructions_per_frame() >= STEP_PLAYING_PER_FRAME
        {
            over.push(format!(
                "{name}: {most} cells, {:.1} ms, {held} cut short, poll {}, {:.0} a frame",
                c.worst_frame_ms(),
                c.worst_poll,
                c.instructions_per_frame()
            ));
        }
    }
    assert!(over.is_empty(), "{over:#?}");
}

const STEP_IDLE_PER_FRAME: f64 = 7_900.0;
const STEP_CHANGE_PER_FRAME: f64 = 19_900.0;
const STEP_PLAYING_PER_FRAME: f64 = 24_500.0;
const STEP_WORST_POLL: u64 = 50_000;
const STEP_INPUT: u64 = 3_900;
const STEP_PASS: u64 = 61_000;

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

fn set_tracker_scroll(m: &mut Machine, fw: &Firmware, scroll: bool) {
    m.soc.write8(
        fw.symbol("ui")
            + (sidou_sys::ilp32::views::song + sidou_sys::ilp32::song_view::scroll) as u32,
        u8::from(scroll),
    );
}

fn tracker_run(
    fw: &Firmware,
    song: Option<&sidou_tests::song::Song>,
    scroll: bool,
    ms: u64,
) -> (Cost, u32) {
    song_view_playing(fw, song, ms, |m| {
        set_tracker_scroll(m, fw, scroll);
        open_tracker(m, fw);
    })
}

#[test]
fn the_tracker_scrolling_matches_its_golden_image() {
    let Some((build, fw)) = common::build_and_firmware("the TRACKER scroll test") else {
        return;
    };
    let mut m = booted(&fw);
    set_tracker_scroll(&mut m, &fw, true);
    open_tracker(&mut m, &fw);
    press(&mut m, PLAY);
    m.run_ms(2_000);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "tracker-scroll", &m.soc.lcd.fb, &[]);
}

#[test]
fn applied_song_edits_refresh_the_tracker_before_and_after_playback() {
    let Some(fw) = common::firmware("the TRACKER edit refresh test") else {
        return;
    };
    let song = tempo_3_song();
    for played in [false, true] {
        let mut m = booted(&fw);
        load_song(&mut m, &fw, song.bytes());
        open_tracker(&mut m, &fw);
        if played {
            press(&mut m, PLAY);
            m.run_ms(500);
            press(&mut m, PLAY);
            m.run_ms(500);
        }
        const HOOK_GO: u32 = 0;
        const HOOK_PATTERN: u32 = 4;
        const HOOK_ROW: u32 = 8;
        const HOOK_CELL: u32 = 12;
        const D4_INSTRUMENT_1_COMMAND_422: [u8; 4] = [0x60 + 4 * 12 + 2, 1, 4, 0x22];
        let hook = fw.symbol("song_test_hook");
        m.soc.write32(hook + HOOK_PATTERN, 0);
        m.soc.write32(hook + HOOK_ROW, 0);
        for (i, byte) in D4_INSTRUMENT_1_COMMAND_422.into_iter().enumerate() {
            m.soc.write8(hook + HOOK_CELL + i as u32, byte);
        }
        m.soc.write32(hook + HOOK_GO, 1);
        m.run_ms(200);
        assert_eq!(m.soc.read32(hook + HOOK_GO), 0, "the edit was taken");
        let grid = fw.symbol("cells");
        let ch = grid + sidou_sys::ilp32::grid::ch as u32;
        let src = grid + sidou_sys::ilp32::grid::src as u32;
        const ROW0_BIG_CELL_ROW: u32 = 11;
        const ROW0_COMMAND_CELL_ROW: u32 = 12;
        let cols = GRID_COLS as u32;
        let mut big = |col: u32, n: u32| -> Vec<u8> {
            (0..n)
                .map(|i| m.soc.read8(src + ROW0_BIG_CELL_ROW * cols + col + 2 * i))
                .collect()
        };
        let note = big(3, 3);
        let instrument = big(10, 2);
        let cmd: Vec<u8> = (0..3)
            .map(|i| m.soc.read8(ch + ROW0_COMMAND_CELL_ROW * cols + 15 + i))
            .collect();
        assert_eq!(note, b"D-4", "played first: {played}");
        assert_eq!(instrument, b"01", "played first: {played}");
        assert_eq!(cmd, b"422", "played first: {played}");
    }
}

fn assert_within_tracker_input(what: &str, (input, pass): (u64, u64)) {
    println!("{what}: input {input}, busiest pass {pass}");
    assert!(input < TRACKER_INPUT, "{what}: input {input}");
    assert!(pass < TRACKER_PASS, "{what}: pass {pass}");
}

#[test]
fn tracker_edits_from_the_panel_are_readable_and_bounded() {
    let Some((build, fw)) = common::build_and_firmware("the TRACKER panel editor test") else {
        return;
    };
    let mut m = booted(&fw);
    open_tracker(&mut m, &fw);
    press(&mut m, REC);
    m.run_ms(200);
    m.watch_calls(fw.symbol("ui_encoder"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    for (what, encoder, steps) in [
        ("row", SELECT, 1),
        ("instrument", K1, 1),
        ("command", K2, 4),
        ("data high", K3, 2),
        ("data low", K4, 3),
    ] {
        let cost = start_cost(&mut m, 200, |m| m.turn(encoder, steps));
        assert_within_tracker_input(&format!("TRACKER edit {what}"), cost);
    }
    m.run_ms(POPUP_CLOSED_MS);
    to_mid_frame(&mut m, &fw);
    let src = fw.symbol("cells") + sidou_sys::ilp32::grid::src as u32;
    const EDITED_ROW_BIG_CELL_ROW: u32 = 13;
    let command: Vec<_> = (0..3)
        .map(|i| {
            m.soc
                .read8(src + EDITED_ROW_BIG_CELL_ROW * GRID_COLS as u32 + 15 + 2 * i)
        })
        .collect();
    assert_eq!(command, b"423", "every encoder detent must reach the model");
    compare_golden(&build, "tracker-edited", &m.soc.lcd.fb, &[]);
    m.turn(K1, 1);
    m.run_ms(200);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "tracker-instrument-popup", &m.soc.lcd.fb, &[]);
    m.run_ms(POPUP_CLOSED_MS);
    holding(&mut m, EDIT, |m| {
        for (what, steps) in [("insert row", 1), ("delete row", -1)] {
            let cost = start_cost(m, 200, |m| m.turn(PRESETS, steps));
            assert_within_tracker_input(&format!("TRACKER edit {what}"), cost);
        }
        m.watch_calls(fw.symbol("ui_buttons"));
        for what in ["undo", "redo"] {
            let cost = start_cost(m, 200, |m| press(m, LFO));
            assert_within_tracker_input(&format!("TRACKER edit {what}"), cost);
        }
    });
    m.run_ms(30);
    for (what, button) in [("key off", ARP), ("rest", LFO), ("clear row", LFO)] {
        let cost = start_cost(&mut m, 200, |m| press(m, button));
        assert_within_tracker_input(&format!("TRACKER edit {what}"), cost);
    }
    press(&mut m, PLAY);
    m.run_ms(500);
    m.watch_calls(fw.symbol("ui_encoder"));
    for (what, encoder, steps) in [
        ("playing instrument", K1, 1),
        ("playing command", K2, 4),
        ("playing data", K3, 2),
    ] {
        let cost = start_cost(&mut m, 500, |m| m.turn(encoder, steps));
        assert_within_tracker_input(&format!("TRACKER edit {what}"), cost);
    }
    holding(&mut m, EDIT, |m| {
        let cost = start_cost(m, 500, |m| m.turn(PRESETS, 1));
        assert_within_tracker_input("TRACKER edit playing insert", cost);
    });
}

#[test]
fn tracker_compaction_is_bounded_and_shows_tidying() {
    use sidou_sys::*;
    let Some(fw) = common::firmware("the TRACKER compaction test") else {
        return;
    };
    let mut song = sidou_tests::song::Song::new(unsafe { gt2_region_min() });
    let mut edit = gt2_edit::default();
    unsafe {
        assert_eq!(
            gt2_pattern_resize(song.ptr_mut(), 0, GT2_MAX_ROWS, &mut edit),
            GT2_OK
        );
        gt2_edit_commit(song.ptr_mut(), &edit);
        loop {
            let mut pattern = 0;
            let result = gt2_pattern_add(song.ptr_mut(), 2, &mut pattern, &mut edit);
            if result != GT2_OK {
                assert_eq!(result, GT2_ERR_FULL);
                break;
            }
            gt2_edit_commit(song.ptr_mut(), &edit);
        }
        assert_eq!(
            gt2_pattern_resize(song.ptr_mut(), 1, GT2_DEFAULT_ROWS, &mut edit),
            GT2_OK
        );
        gt2_edit_commit(song.ptr_mut(), &edit);
        assert!(gt2_arena_largest_hole(song.ptr()) < GT2_PATTERN_MAX_BYTES);
    }
    let mut m = booted(&fw);
    load_song(&mut m, &fw, song.bytes());
    open_tracker(&mut m, &fw);
    press(&mut m, REC);
    m.run_ms(200);
    m.watch_calls(fw.symbol("ui_encoder"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    let pending = fw.symbol("ui") + (ilp32::views::song + ilp32::song_view::edit_pending) as u32;
    let blocks = GT2_MAX_PATTERNS + u32::from(song.raw().subtunes) * u32::from(song.raw().channels);
    const FRAMES_A_BLOCK: u32 = 4;
    let max_frames = FRAMES_A_BLOCK * blocks + 4;
    let mut frames = 0;
    let mut cost = (0, 0);
    holding(&mut m, EDIT, |m| {
        cost = start_cost(m, 0, |m| {
            m.turn(PRESETS, 1);
            m.run_ms(100);
            assert_eq!(toast_text(m, &fw), "TIDYING");
            while m.soc.read8(pending) != 0 && frames < max_frames {
                m.run_ms(UI_FRAME_MS);
                frames += 1;
            }
        });
    });
    let (input, pass) = cost;
    println!("TRACKER compaction: input {input}, busiest pass {pass}, {frames} completion frames");
    assert!(input < TRACKER_INPUT);
    assert!(pass < TRACKER_PASS, "compaction pass {pass}");
    assert_eq!(
        m.soc.read8(pending),
        0,
        "compaction and the user's swap complete"
    );
}

#[test]
fn tracker_refuses_growth_at_the_real_song_size_cap() {
    use sidou_sys::*;
    let Some(fw) = common::firmware("the TRACKER serialized-size refusal") else {
        return;
    };
    let mut spec = sidou_tests::song::SngSpec::new();
    spec.orders = vec![vec![(vec![207; 100], 0); 3]; 32];
    spec.patterns = (0..208)
        .map(|_| {
            (
                vec![[GT2_REST as u8, 0, 0, 0]; 64],
                [GT2_ENDPATT as u8, 0, 0, 0],
            )
        })
        .collect();
    spec.patterns[207].0[0] = [GT2_FIRSTNOTE as u8, 1, 0, 0];
    let mut model = common::device_song(&spec.encode(), None);
    let before = unsafe { gt2_saved_size(model.ptr()) };
    let trailer = unsafe { gt2_saved_trailer_size(model.raw().ext_used) };
    let padding = (1..SONG_EXT_BYTES - SONG_CHUNK_HEADER)
        .find(|&n| {
            before - trailer
                + unsafe { gt2_saved_trailer_size(model.raw().ext_used + SONG_CHUNK_HEADER + n) }
                == GT2_SAVED_MAX
        })
        .expect("fixture reaches the exact cap");
    assert_eq!(
        model.ext_set(sidou_tests::song::tag(b"CAPT"), &vec![1; padding as usize]),
        0
    );
    let expected = model.write(true);
    assert_eq!(expected.len(), GT2_SAVED_MAX as usize);
    let mut m = booted(&fw);
    load_song(&mut m, &fw, model.bytes());
    let region = fw.symbol("__song_region");
    open_tracker(&mut m, &fw);
    press(&mut m, REC);
    m.turn(K1, 1);
    m.run_ms(200);
    assert_eq!(toast_text(&mut m, &fw), "SONG FULL");
    let mut actual = sidou_tests::song::Song::device();
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(actual.ptr_mut().cast::<u8>(), SONG_REGION_BYTES as usize)
    };
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = m.soc.read8(region + i as u32);
    }
    assert_eq!(
        actual.write(true),
        expected,
        "a refused panel edit must not change the song"
    );
    assert_eq!(actual.raw().pending, 0);
}

const SONG_PAGES_PRESETS: u8 = sidou_sys::SONG_PAGES_ENC_PRESETS as u8;

fn assert_song_page_redraw_within_budget(what: &str, c: &Cost) {
    let most = c.frames.iter().map(cells).max().unwrap_or(0);
    println!(
        "{what}: poll {}, max cells {most}, SPI {} ms",
        c.worst_poll,
        c.worst_frame_ms()
    );
    assert!(
        most <= sidou_sys::SONG_VIEW_FRAME_CELLS,
        "{what}: {most} cells"
    );
    assert!(c.worst_frame_ms() <= SPI_BUDGET_MS, "{what}: SPI");
    assert!(
        c.worst_poll < TRACKER_WORST_POLL,
        "{what}: {}",
        c.worst_poll
    );
}

fn open_edit_page(m: &mut Machine, fw: &Firmware, steps: usize, symbol: &str) {
    open_tracker(m, fw);
    for _ in 0..steps {
        press(m, SEQ);
        settle(m, fw);
    }
    assert_eq!(top_view(m, fw), fw.symbol(symbol));
}

#[test]
fn the_readable_order_instrument_and_table_pages_match_their_goldens() {
    let Some((build, fw)) = common::build_and_firmware("the readable song edit pages") else {
        return;
    };
    for (steps, symbol, name) in [
        (1, "song_pages_order_view", "order"),
        (2, "song_pages_instr_view", "instr"),
        (3, "song_pages_table_view", "table-wave"),
    ] {
        let mut m = booted(&fw);
        open_edit_page(&mut m, &fw, steps, symbol);
        compare_golden(&build, name, &m.soc.lcd.fb, &[]);
        if name == "table-wave" {
            press(&mut m, ARP);
            settle(&mut m, &fw);
            m.turn(SONG_PAGES_PRESETS, 5);
            m.run_ms(POPUP_CLOSED_MS);
            settle(&mut m, &fw);
            compare_golden(&build, "table-arp", &m.soc.lcd.fb, &[]);
            press(&mut m, HOME);
            settle(&mut m, &fw);
            m.turn(ALGORITHM, 1);
            m.run_ms(POPUP_CLOSED_MS);
            settle(&mut m, &fw);
            compare_golden(&build, "table-pulse", &m.soc.lcd.fb, &[]);
        }
    }
}

#[test]
fn all_song_edit_pages_keep_the_existing_input_frame_and_spi_budgets() {
    let Some(fw) = common::firmware("the song edit pages budgets") else {
        return;
    };
    for (steps, symbol, name, turns) in [
        (
            1,
            "song_pages_order_view",
            "ORDER",
            vec![(SELECT, 1), (ALGORITHM, 1), (K1, 1), (K2, 1)],
        ),
        (
            2,
            "song_pages_instr_view",
            "INSTR",
            vec![(SELECT, 1), (K1, 1), (K2, 1), (SONG_PAGES_PRESETS, 1)],
        ),
        (
            3,
            "song_pages_table_view",
            "TABLE",
            vec![(SELECT, 1), (K1, -1), (K2, 1), (ALGORITHM, 1)],
        ),
    ] {
        let mut m = booted(&fw);
        open_edit_page(&mut m, &fw, steps, symbol);
        m.watch_calls(fw.symbol("ui_poll"));
        let idle = measure(&mut m, &fw, 25, |_| {});
        println!(
            "{name} idle: {} instructions/frame, poll {}, {} SPI windows",
            idle.instructions_per_frame(),
            idle.worst_poll,
            idle.total().windows
        );
        assert_eq!(idle.total().windows, 0);
        assert!(idle.instructions_per_frame() < TRACKER_IDLE_PER_FRAME);
        assert!(idle.worst_poll < TRACKER_WORST_POLL);
        m.watch_calls(fw.symbol("ui_encoder"));
        m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
        for (encoder, detents) in turns {
            let cost = start_cost(&mut m, 250, |m| m.turn(encoder, detents));
            assert_within_tracker_input(&format!("{name} encoder {encoder}"), cost);
            m.run_ms(POPUP_CLOSED_MS);
            m.watch_calls(fw.symbol("ui_poll"));
            let redraw = measure(&mut m, &fw, 12, |m| m.turn(encoder, detents));
            assert_song_page_redraw_within_budget(
                &format!("{name} encoder {encoder} redraw"),
                &redraw,
            );
            m.watch_calls(fw.symbol("ui_encoder"));
        }
        m.watch_calls(fw.symbol("ui_poll"));
        m.run_ms(POPUP_CLOSED_MS);
        let c = measure(&mut m, &fw, 30, |m| m.turn(SELECT, 8));
        println!(
            "{name} cursor: {} instructions/frame",
            c.instructions_per_frame()
        );
        assert_song_page_redraw_within_budget(&format!("{name} cursor"), &c);
    }
    let mut m = booted(&fw);
    open_edit_page(&mut m, &fw, 1, "song_pages_order_view");
    let order_length = fw.symbol("__song_region") + sidou_sys::ilp32::gt2_song::order as u32;
    let order_rows = |m: &mut Machine| (m.soc.read32(order_length) >> 16) as i32;
    holding(&mut m, EDIT, |m| {
        m.watch_calls(fw.symbol("ui_encoder"));
        m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
        for (name, steps) in [("insert", 1), ("delete", -1)] {
            let before = order_rows(m);
            let cost = start_cost(m, 250, |m| m.turn(SONG_PAGES_PRESETS, steps));
            assert_within_tracker_input(&format!("ORDER {name}"), cost);
            assert_eq!(order_rows(m), before + steps);
            m.run_ms(POPUP_CLOSED_MS);
            m.watch_calls(fw.symbol("ui_poll"));
            let redraw = measure(m, &fw, 12, |m| m.turn(SONG_PAGES_PRESETS, steps));
            assert_song_page_redraw_within_budget(&format!("ORDER {name} redraw"), &redraw);
            assert_eq!(order_rows(m), before + 2 * steps);
            m.watch_calls(fw.symbol("ui_encoder"));
        }
    });
}

const TRACKER_BAR_PER_FRAME: f64 = 55_000.0;
const TRACKER_SCROLL_PER_FRAME: f64 = 99_000.0;
const TRACKER_PLAY_WORST_POLL: u64 = 42_000;

#[test]
fn the_tracker_stays_within_its_budgets_while_a_song_plays() {
    let Some(fw) = common::firmware("the TRACKER playing budget test") else {
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

const _: () = assert!(LIVE_WORST_POLL <= POLL_INSTRUCTIONS);
const _: () = assert!(SCENE_WORST_POLL <= POLL_INSTRUCTIONS);
const _: () = assert!(SCENE_INPUT + SCENE_START_PASS <= POLL_INSTRUCTIONS);
const _: () = assert!(ABOUT_WORST_POLL <= POLL_INSTRUCTIONS);
const _: () = assert!(BOOT_WORST_POLL <= POLL_INSTRUCTIONS);
const _: () = assert!(TITLE_CARD_WORST_POLL <= POLL_INSTRUCTIONS);
const _: () = assert!(TITLE_CARD_INPUT + TITLE_CARD_PASS <= POLL_INSTRUCTIONS);
const _: () = assert!(CHIP_WORST_POLL <= POLL_INSTRUCTIONS);
const _: () = assert!(CHIP_INPUT + CHIP_PASS <= POLL_INSTRUCTIONS);
const _: () = assert!(TRACKER_WORST_POLL <= POLL_INSTRUCTIONS);
const _: () = assert!(TRACKER_PLAY_WORST_POLL <= POLL_INSTRUCTIONS);
const _: () = assert!(TRACKER_INPUT + TRACKER_PASS <= POLL_INSTRUCTIONS);
const _: () = assert!(STEP_WORST_POLL <= POLL_INSTRUCTIONS);
const _: () = assert!(STEP_INPUT + STEP_PASS <= POLL_INSTRUCTIONS);
const PREPARE_SLICES_PER_PASS: u64 = 1;
const _: () = assert!(
    LIVE_WORST_POLL + PREPARE_SLICES_PER_PASS * REG_RING_PREPARE_SLICE <= POLL_INSTRUCTIONS
);
const _: () = assert!(
    CHIP_INPUT + CHIP_PASS + PREPARE_SLICES_PER_PASS * REG_RING_PREPARE_SLICE <= POLL_INSTRUCTIONS
);
const _: () = assert!(
    TRACKER_INPUT + TRACKER_PASS + PREPARE_SLICES_PER_PASS * REG_RING_PREPARE_SLICE
        <= POLL_INSTRUCTIONS
);
const _: () = assert!(
    STEP_INPUT + STEP_PASS + PREPARE_SLICES_PER_PASS * REG_RING_PREPARE_SLICE <= POLL_INSTRUCTIONS
);

const AUDIO_B_UI: u8 = 2;
const UI_SOUND_RELEASED_MS: u64 = 800;
const BLIP_HZ: f64 = 1046.5;

fn ui_sounds_run(fw: &Firmware, sounds: bool, held: bool) -> (Vec<i16>, bool) {
    let mut m = booted(fw);
    if held {
        song_on_chip_a(&mut m, fw);
        m.run_ms(100);
        for k in CHORD {
            m.key(k, true);
            m.run_ms(CHORD_STAGGER_MS);
        }
    }
    open_menu_settled(&mut m, fw);
    settings_item(&mut m, sidou_sys::VIEWS_SETTINGS_SOUNDS);
    if sounds {
        m.turn(ALGORITHM, 1);
    }
    m.run_ms(UI_SOUND_RELEASED_MS);
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

#[test]
fn with_sounds_on_a_menu_move_blips_and_off_it_is_silent() {
    let Some(fw) = common::firmware("the UI sounds test") else {
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
    let magnitude_at_the_blips_pitch = |x: &[f32]| {
        let n = 4096.min(x.len());
        let w = 2.0 * std::f64::consts::PI * BLIP_HZ / common::FS;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &s) in x[..n].iter().enumerate() {
            re += f64::from(s) * (w * i as f64).cos();
            im -= f64::from(s) * (w * i as f64).sin();
        }
        (re * re + im * im).sqrt() / n as f64
    };
    let (e_on, e_off) = (
        magnitude_at_the_blips_pitch(&left(&on)),
        magnitude_at_the_blips_pitch(&left(&off)),
    );
    println!("UI sounds: the blip's band {e_on:.1} with SOUNDS on, {e_off:.1} off");
    assert!(
        e_on > 20.0 * e_off.max(0.5),
        "no blip: {e_on:.1} against {e_off:.1}"
    );
}

#[test]
fn with_three_notes_held_on_chip_b_a_menu_move_is_silent_and_bit_exact() {
    let Some(fw) = common::firmware("the UI sounds test") else {
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

const SONG_SLICE: u64 = sidou_sys::SONG_SLICE_INSTRUCTIONS as u64;
const _: () = assert!(LIVE_WORST_POLL + SONG_SLICE <= POLL_INSTRUCTIONS);
const _: () = assert!(CHIP_INPUT + CHIP_PASS + SONG_SLICE <= POLL_INSTRUCTIONS);
const _: () = assert!(SCENE_INPUT + SCENE_START_PASS + SONG_SLICE <= POLL_INSTRUCTIONS);
const _: () = assert!(RMR_INPUT + RMR_START_PASS + SONG_SLICE <= POLL_INSTRUCTIONS);
const _: () = assert!(TITLE_CARD_INPUT + TITLE_CARD_PASS + SONG_SLICE <= POLL_INSTRUCTIONS);
const _: () = assert!(WIPE_WORST_POLL + SONG_SLICE <= POLL_INSTRUCTIONS);
const _: () = assert!(ABOUT_WORST_POLL + SONG_SLICE <= POLL_INSTRUCTIONS);
const _: () = assert!(TRACKER_INPUT + TRACKER_PASS + SONG_SLICE <= POLL_INSTRUCTIONS);

const SCENE_RMR: usize = sidou_sys::SCENE_RMR as usize;

fn scene_byte(m: &mut Machine, fw: &Firmware, offset: usize) -> u8 {
    ui_byte(m, fw, sidou_sys::ilp32::views::scene + offset)
}

fn rmr_byte(m: &mut Machine, fw: &Firmware, field: usize) -> u8 {
    use sidou_sys::ilp32::{scene_view, scene_view_rite};
    scene_byte(m, fw, scene_view::u + scene_view_rite::rmr + field)
}

fn song_state(m: &mut Machine, fw: &Firmware) -> u32 {
    u32::from(
        m.soc
            .read8(fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::state as u32),
    )
}

fn toast_text(m: &mut Machine, fw: &Firmware) -> String {
    let at =
        fw.symbol("ui") + (sidou_sys::ilp32::views::s + sidou_sys::ilp32::screen::toast) as u32;
    (0..sidou_sys::SCREEN_TOAST_CHARS)
        .map(|i| m.soc.read8(at + i))
        .take_while(|&b| b != 0)
        .map(char::from)
        .collect()
}

fn rmr_with_the_song(m: &mut Machine, fw: &Firmware) {
    open_scene(m, fw, 0);
    m.turn(SELECT, -1);
    m.run_ms(100);
    assert_eq!(
        usize::from(scene_byte(m, fw, sidou_sys::ilp32::scene_view::which)),
        SCENE_RMR
    );
    assert_eq!(toast_text(m, fw), "RED MOON RITE", "SELECT's toast");
    m.run_ms(TOAST_GONE_MS);
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
    to_mid_frame(m, fw);
}

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
        m.run_ms(UI_FRAME_MS);
    }
    None
}

#[test]
fn red_moon_rite_plays_the_demo_songs_duel_disarm_and_stuck_sword_as_its_golden_images() {
    use sidou_sys::ilp32::scene_rmr as r;
    let Some((build, fw)) = common::build_and_firmware("the RED MOON RITE test") else {
        return;
    };
    let mut m = booted(&fw);
    rmr_with_the_song(&mut m, &fw);
    m.run_ms(1000);
    to_mid_frame(&mut m, &fw);
    compare_golden(&build, "scene-rmr-song", &m.soc.lcd.fb, &[]);
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
    m.run_ms(4 * UI_FRAME_MS);
    let unarmed = rmr_byte(&mut m, &fw, r::unarmed);
    assert!(
        rmr_byte(&mut m, &fw, r::sword) != 0 && unarmed < 2,
        "the stuck-sword golden must precede the sword's recovery"
    );
    compare_golden(&build, "scene-rmr-stuck", &m.soc.lcd.fb, &[]);
    println!("the disarm {landed:?} frames in, stuck {stuck:?} frames after");
}

#[test]
fn red_moon_rite_stays_within_its_budgets_over_a_minute_of_the_song() {
    let Some(fw) = common::firmware("the RED MOON RITE budget test") else {
        return;
    };
    let mut m = Machine::boot(&fw.image);
    m.watch_calls(fw.symbol("ui_poll"));
    m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
    run_past_boot(&mut m, &fw);
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
        run_past_boot(&mut m, &fw);
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

const RMR_CELLS_AVERAGE: f64 = 109.0;
const RMR_CELLS_MOST: u32 = 180;
const RMR_PER_FRAME: f64 = 1_000_000.0;
const RMR_WORST_POLL: u64 = 62_000;
const RMR_WORST_PASS: u64 = 63_000;
const RMR_WORST_SLICE: u64 = 62_000;
const RMR_WORST_RUN: u64 = 27_000;
const RMR_WORST_TILE: u64 = 6_500;
const _: () = assert!(RMR_WORST_POLL + SONG_SLICE <= POLL_INSTRUCTIONS);
const _: () = assert!(RMR_WORST_PASS + SONG_SLICE <= POLL_INSTRUCTIONS);
const _: () = assert!(2 * RMR_WORST_RUN <= RMR_WORST_POLL && RMR_WORST_SLICE <= RMR_WORST_POLL);

#[test]
fn a_muted_drum_voice_stops_red_moon_rites_chops_and_parries() {
    use sidou_sys::ilp32::scene_rmr as r;
    let Some(fw) = common::firmware("the RED MOON RITE mute test") else {
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
            m.run_ms(UI_FRAME_MS);
        }
        n
    };
    let open = attacks(&mut m, &fw, 150);
    const DRUM_VOICE_MUTED: u8 = 1 << 2;
    let mute = fw.symbol("song_transport") + sidou_sys::ilp32::song_ctl::mute as u32;
    m.soc.write8(mute, DRUM_VOICE_MUTED);
    m.run_ms(500);
    let muted = attacks(&mut m, &fw, 150);
    println!("frames in an attack or a parry over 6 s: {open} with the drums, {muted} muted");
    assert!(open > 20, "{open}: the drums did not move the duel");
    assert_eq!(muted, 0, "the muted drums still move the duel");
}

#[test]
fn red_moon_rite_sends_nothing_once_the_stopped_songs_releases_have_rung_out() {
    let Some(fw) = common::firmware("the RED MOON RITE calm test") else {
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
