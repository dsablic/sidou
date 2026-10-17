// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The title cards (app/title_card.c) over the views (app/views.c): when they show, their
//! sequence frame by frame (the glow's levels, the title's wipe, the hold, the banded cut),
//! the budget, skipping by any control without its action, a note that plays and ends it,
//! nothing drawn over it, and the menu's CARDS option.

use sidou_sys::*;
use sidou_tests::title_card as tc;

use crate::views::Ui;
use crate::{drain, petscii, pixels, Sent, SPI_BUDGET_MS};

const W: usize = 240;

/// Frames, each sent whole as the panel would, until no transition runs.
fn settle(ui: &mut Ui) {
    ui.settle();
    step(ui);
}

/// From LIVE: HOME, SELECT to `item`, ALGORITHM clockwise; what the menu drew sent.
fn open_from_menu(ui: &mut Ui, item: u32) {
    ui.press(PANEL_BTN_HOME);
    settle(ui);
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, item as i32);
    step(ui);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
}

/// LIVE with nothing left to send.
fn at_rest() -> Ui {
    let mut ui = Ui::new();
    ui.settle();
    drain(&mut ui.g);
    ui
}

fn card(ui: &Ui) -> &title_card {
    &ui.v.card
}

/// A frame, and what it sent.
fn step(ui: &mut Ui) -> Sent {
    ui.frame();
    drain(&mut ui.g)
}

/// What the card did each frame until it ended: (phase, level, columns, cells sent).
fn run_card(ui: &mut Ui) -> Vec<(u32, u8, u8, Sent)> {
    let mut out = Vec::new();
    for _ in 0..200 {
        let sent = step(ui);
        if ui.top() != "TITLE" {
            return out;
        }
        let c = card(ui);
        out.push((u32::from(c.phase), c.level, c.cols, sent));
    }
    panic!("the card did not end");
}

fn assert_shows_png(ui: &Ui, name: &str) {
    let img = sidou_tests::logo::load(name);
    let px = pixels(&ui.g);
    if let Some(i) = (0..W * W).find(|&i| px[i] != img.px[i]) {
        panic!(
            "{name}: pixel ({}, {}) is {} where the PNG has {}",
            i % W,
            i / W,
            px[i],
            img.px[i]
        );
    }
}

#[test]
fn scene_from_the_menu_shows_its_card_then_cuts_to_scene() {
    let mut ui = at_rest();
    open_from_menu(&mut ui, VIEWS_MENU_SCENE);
    assert_eq!(ui.top(), "TITLE");
    assert_eq!(u32::from(card(&ui).id), TITLE_CARD_SCENE);
    let frames = run_card(&mut ui);
    // Every frame inside the budget.
    for (k, f) in frames.iter().enumerate() {
        assert!(
            f.3.cells <= TITLE_CARD_FRAME_CELLS as usize && f.3.spi_ms() <= SPI_BUDGET_MS,
            "frame {k}: {:?}, {:.1} ms",
            f.3,
            f.3.spi_ms()
        );
    }
    // The glow's levels in order, then the wipe 3 columns a frame, then the hold.
    let phases: Vec<u32> = frames.iter().map(|f| f.0).collect();
    let glow = phases.iter().filter(|&&p| p == TITLE_CARD_GLOW).count();
    let wipe = phases.iter().filter(|&&p| p == TITLE_CARD_WIPE).count();
    let hold = phases.iter().filter(|&&p| p == TITLE_CARD_HOLD).count();
    assert!(phases.windows(2).all(|w| w[0] <= w[1]), "{phases:?}");
    let levels: Vec<u8> = frames
        .iter()
        .filter(|f| f.0 == TITLE_CARD_GLOW)
        .map(|f| f.1)
        .collect();
    assert!(levels.windows(2).all(|w| w[1] == w[0] || w[1] == w[0] + 1));
    assert_eq!((levels[0], *levels.last().unwrap()), (1, 4));
    let cols: Vec<u8> = frames
        .iter()
        .filter(|f| f.0 == TITLE_CARD_WIPE)
        .map(|f| f.2)
        .collect();
    assert_eq!(cols, (1..=10).map(|k| 3 * k).collect::<Vec<u8>>());
    assert_eq!(wipe, 10);
    assert_eq!(hold, TITLE_CARD_HOLD_FRAMES as usize);
    // The glow: each level's cells in frames of at most 200, then 2 frames still; the first
    // level changes the whole menu (900 cells, 5 frames).
    println!(
        "THE SCENE: {glow} frames of glow, {wipe} of wipe, {hold} of hold, {} in all; {} cells",
        frames.len(),
        frames.iter().map(|f| f.3.cells).sum::<usize>()
    );
    assert!((16..=25).contains(&glow), "{glow} frames of glow");
    assert!(
        frames[..4].iter().all(|f| f.3.cells == 200),
        "the first level in bands"
    );
    // The wipe sends the columns it adds: 90 cells a frame.
    for f in frames.iter().filter(|f| f.0 == TITLE_CARD_WIPE) {
        assert!(f.3.cells <= 90, "{:?}", f.3);
    }
    // The hold sends nothing.
    for f in frames.iter().filter(|f| f.0 == TITLE_CARD_HOLD) {
        assert_eq!(f.3.cells, 0);
    }
    // Then the cut to SCENE, in bands.
    assert_eq!(ui.top(), "SCENE");
    assert_eq!(ui.v.s.wipe, SCREEN_BANDS as u8);
    let mut bands = 0;
    while unsafe { screen_animating(&ui.v.s) } != 0 {
        let sent = step(&mut ui);
        assert!(sent.spi_ms() <= SPI_BUDGET_MS, "{sent:?}");
        bands += 1;
    }
    assert!(bands <= 5, "{bands} frames of bands");
    // SCENE started its piece in the cut, and the cut showed all of it.
    assert_eq!(u32::from(petscii(&ui.v.scene).piece), SCENE_FIRE);
    step(&mut ui);
    let cv = &petscii(&ui.v.scene).cv;
    assert!(
        (0..30 * 30).all(|i| ui.g.ch[i] == cv.ch[i / 30][i % 30]
            && ui.g.attr[i] == cv.attr[i / 30][i % 30]),
        "the grid is not the fire's canvas after the cut"
    );
}

#[test]
fn the_held_frame_is_the_cards_png_pixel_for_pixel() {
    let mut ui = at_rest();
    open_from_menu(&mut ui, VIEWS_MENU_SCENE);
    while card(&ui).phase != TITLE_CARD_HOLD as u8 {
        step(&mut ui);
    }
    step(&mut ui);
    assert_shows_png(&ui, "title-scene");
    // The glow cells keep a dither glyph, the cells that differ from it are blank over the
    // bitmap.
    let cells = unsafe { &title_card_cells[TITLE_CARD_SCENE as usize] };
    for (r, bits) in cells.iter().enumerate() {
        for c in 0..30 {
            let ch = ui.g.ch[r * 30 + c];
            if bits >> c & 1 == 1 {
                assert_eq!(ch, b' ', "({c}, {r})");
            } else {
                assert!((FONT_D0 as u8..FONT_D16 as u8).contains(&ch), "({c}, {r})");
            }
        }
    }
}

/// Each level of the glow, once its cells are out, is the art's glow at that level.
#[test]
fn the_glow_fades_in_through_its_dither_levels() {
    let levels = tc::glow_levels(&sidou_tests::logo::load("title-glow")).unwrap();
    let mut ui = at_rest();
    open_from_menu(&mut ui, VIEWS_MENU_SCENE);
    let mut seen = Vec::new();
    while card(&ui).phase == TITLE_CARD_GLOW as u8 {
        step(&mut ui);
        let c = card(&ui);
        if c.settled != 0 && c.phase == TITLE_CARD_GLOW as u8 && !seen.contains(&c.level) {
            seen.push(c.level);
            let want = tc::glow_image(&levels, u32::from(c.level));
            assert!(pixels(&ui.g) == want.px, "level {}", c.level);
        }
    }
    assert_eq!(seen, vec![1, 2, 3, 4]);
}

#[test]
fn home_from_the_menu_to_live_shows_the_oscillator_card() {
    let mut ui = at_rest();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "TITLE");
    assert_eq!(u32::from(card(&ui).id), TITLE_CARD_OSCILLATOR);
    while card(&ui).phase != TITLE_CARD_HOLD as u8 {
        step(&mut ui);
    }
    step(&mut ui);
    assert_shows_png(&ui, "title-oscillator");
    run_card(&mut ui);
    assert_eq!(ui.top(), "LIVE");
}

#[test]
fn power_on_the_boot_and_back_from_a_view_to_the_menu_show_no_card() {
    let mut ui = Ui::new();
    assert_eq!(ui.top(), "LIVE");
    unsafe { views_boot(&mut *ui.v, 0, 0) };
    for _ in 0..80 {
        ui.frame();
        assert_ne!(ui.top(), "TITLE");
    }
    assert_eq!(ui.top(), "LIVE");
    // STATUS back to the menu: no card.
    open_from_menu(&mut ui, VIEWS_MENU_STATUS);
    ui.settle();
    assert_eq!(ui.top(), "STATUS");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
}

/// Any button or encoder ends the card at the next frame and does nothing else: no octave
/// step, no chip toggle, no menu, no piece picked.
#[test]
fn a_button_or_a_knob_skips_and_does_nothing_else() {
    let buttons = [
        PANEL_BTN_HOME,
        PANEL_BTN_OCT_DOWN,
        PANEL_BTN_OCT_UP,
        PANEL_BTN_FX,
        PANEL_BTN_SCL,
        PANEL_BTN_ENV,
        PANEL_BTN_EDIT,
        PANEL_BTN_SAVE,
    ];
    for b in buttons {
        let mut ui = at_rest();
        open_from_menu(&mut ui, VIEWS_MENU_SCENE);
        step(&mut ui);
        let (octave, model, which) = (ui.live().octave, ui.live().model, ui.v.scene.which);
        ui.press(b);
        assert_eq!(ui.top(), "TITLE", "button {b}: at once");
        ui.frame();
        assert_eq!(ui.top(), "SCENE", "button {b} did not skip");
        assert_eq!(ui.v.s.wipe, SCREEN_BANDS as u8, "button {b}: the cut");
        assert_eq!(
            (ui.live().octave, ui.live().model, ui.v.scene.which),
            (octave, model, which),
            "button {b} did something"
        );
    }
    for enc in [
        PANEL_ENC_SELECT,
        PANEL_ENC_ALGORITHM,
        PANEL_ENC_K1,
        PANEL_ENC_K4,
    ] {
        let mut ui = at_rest();
        ui.press(PANEL_BTN_HOME);
        ui.settle();
        ui.press(PANEL_BTN_HOME);
        assert_eq!(ui.top(), "TITLE");
        let cursor = ui.v.home.cursor;
        ui.turn(enc, 1);
        ui.frame();
        assert_eq!(ui.top(), "LIVE", "encoder {enc} did not skip");
        assert_eq!(ui.v.home.cursor, cursor, "encoder {enc} moved the menu");
        assert_eq!(ui.v.s.depth, 1);
    }
}

/// A note plays, and ends the card: from the keys or MIDI, however short.
#[test]
fn a_note_plays_and_ends_the_card() {
    for short in [false, true] {
        let mut ui = at_rest();
        open_from_menu(&mut ui, VIEWS_MENU_SCENE);
        step(&mut ui);
        step(&mut ui);
        ui.rig.note_on(69);
        assert!(ui.live().voice.iter().any(|v| v.gate != 0 && v.note == 69));
        if short {
            ui.rig.note_off(69);
        }
        ui.frame();
        assert_eq!(ui.top(), "SCENE", "short {short}: the note did not end it");
    }
}

/// A held note does not keep the card away: only a note played while it shows ends it.
#[test]
fn a_note_held_from_before_does_not_end_the_card() {
    let mut ui = at_rest();
    ui.rig.note_on(60);
    open_from_menu(&mut ui, VIEWS_MENU_SCENE);
    for _ in 0..10 {
        ui.frame();
    }
    assert_eq!(ui.top(), "TITLE");
}

#[test]
fn nothing_draws_over_the_card() {
    let mut ui = at_rest();
    open_from_menu(&mut ui, VIEWS_MENU_SCENE);
    ui.rig.button(LIVE_BUTTON_FX); // MIDI or the panel's chip toggle, as it were
    ui.rig.button(LIVE_BUTTON_OCT_UP);
    for _ in 0..5 {
        ui.frame();
        assert!(ui.toast().is_none());
        assert_eq!(ui.v.s.raster_frames, 0);
    }
    run_card(&mut ui);
    ui.settle();
    assert!(
        ui.toast().is_none(),
        "what changed under the card was taken as seen"
    );
}

/// The menu's CARDS item: on at power-on; counter-clockwise off, and then neither SCENE nor
/// LIVE shows a card; clockwise on again.
#[test]
fn the_cards_option_turns_them_off_and_on() {
    let mut ui = at_rest();
    assert_eq!(ui.v.cards, 1, "on at power-on");
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CARDS as i32);
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(ui.v.cards, 0);
    ui.frame();
    assert!(
        (0..30).any(|r| ui.row(r).contains("CARDS") && ui.row(r).contains("OFF")),
        "the menu shows CARDS OFF"
    );
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "LIVE", "no card back to LIVE");
    ui.settle();
    open_from_menu(&mut ui, VIEWS_MENU_SCENE);
    assert_eq!(ui.top(), "SCENE", "no card into SCENE");
    ui.settle();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CARDS as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.v.cards, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.v.cards, 1, "on stays on");
}

/// THE CHIP before the CHIP view, from the menu's first item: its held frame is its PNG, and
/// the CHIP view cuts in under it.
#[test]
fn chip_from_the_menu_shows_its_card_then_cuts_to_chip() {
    let mut ui = at_rest();
    open_from_menu(&mut ui, VIEWS_MENU_CHIP);
    assert_eq!(ui.top(), "TITLE");
    assert_eq!(u32::from(card(&ui).id), TITLE_CARD_CHIP);
    while u32::from(card(&ui).phase) != TITLE_CARD_HOLD {
        step(&mut ui);
    }
    step(&mut ui);
    assert_shows_png(&ui, "title-chip");
    let frames = run_card(&mut ui);
    assert!(frames.len() >= TITLE_CARD_HOLD_FRAMES as usize - 2);
    settle(&mut ui);
    assert_eq!(ui.top(), "CHIP");
}
