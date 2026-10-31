// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::title_card as tc;

use crate::views::{Ui, CARDS_ALWAYS, CARDS_OFF, CARDS_ONCE};
use crate::{drain, petscii, pixels, Sent, SPI_BUDGET_MS};

const W: usize = 240;

fn settle(ui: &mut Ui) {
    ui.settle();
    ui.frame_sent();
}

fn open_from_menu(ui: &mut Ui, item: u32) {
    ui.press(PANEL_BTN_HOME);
    settle(ui);
    ui.highlight_menu_item(item);
    ui.frame_sent();
    ui.turn(PANEL_ENC_ALGORITHM, 1);
}

fn at_rest() -> Ui {
    let mut ui = Ui::new();
    ui.settle();
    drain(&mut ui.g);
    ui
}

fn card(ui: &Ui) -> &title_card {
    &ui.v.card
}

fn run_card(ui: &mut Ui) -> Vec<(u32, u8, u8, Sent)> {
    let mut out = Vec::new();
    for _ in 0..200 {
        let sent = ui.frame_sent();
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
    for (k, f) in frames.iter().enumerate() {
        assert!(
            f.3.cells <= TITLE_CARD_FRAME_CELLS as usize && f.3.spi_ms() <= SPI_BUDGET_MS,
            "frame {k}: {:?}, {:.1} ms",
            f.3,
            f.3.spi_ms()
        );
    }
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
    for f in frames.iter().filter(|f| f.0 == TITLE_CARD_WIPE) {
        assert!(f.3.cells <= 90, "{:?}", f.3);
    }
    for f in frames.iter().filter(|f| f.0 == TITLE_CARD_HOLD) {
        assert_eq!(f.3.cells, 0);
    }
    assert_eq!(ui.top(), "SCENE");
    assert_eq!(ui.v.s.wipe, SCREEN_BANDS as u8);
    let mut bands = 0;
    while unsafe { screen_animating(&ui.v.s) } != 0 {
        let sent = ui.frame_sent();
        assert!(sent.spi_ms() <= SPI_BUDGET_MS, "{sent:?}");
        bands += 1;
    }
    assert!(bands <= 5, "{bands} frames of bands");
    assert_eq!(u32::from(petscii(&ui.v.scene).piece), SCENE_FIRE);
    for _ in 0..60 {
        ui.frame_sent();
    }
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
        ui.frame_sent();
    }
    ui.frame_sent();
    assert_shows_png(&ui, "title-scene");
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

#[test]
fn the_glow_fades_in_through_its_dither_levels() {
    let levels = tc::glow_levels(&sidou_tests::logo::load("title-glow")).unwrap();
    let mut ui = at_rest();
    open_from_menu(&mut ui, VIEWS_MENU_SCENE);
    let mut seen = Vec::new();
    while card(&ui).phase == TITLE_CARD_GLOW as u8 {
        ui.frame_sent();
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
    ui.v.cards = CARDS_ALWAYS;
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "TITLE");
    assert_eq!(u32::from(card(&ui).id), TITLE_CARD_OSCILLATOR);
    while card(&ui).phase != TITLE_CARD_HOLD as u8 {
        ui.frame_sent();
    }
    ui.frame_sent();
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
    open_from_menu(&mut ui, VIEWS_MENU_STATUS);
    ui.settle();
    assert_eq!(ui.top(), "STATUS");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
}

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
        ui.frame_sent();
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
        ui.v.cards = CARDS_ALWAYS;
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

#[test]
fn a_note_plays_and_ends_the_card() {
    for short in [false, true] {
        let mut ui = at_rest();
        open_from_menu(&mut ui, VIEWS_MENU_SCENE);
        ui.frame_sent();
        ui.frame_sent();
        ui.rig.note_on(69);
        assert!(ui.live().voice.iter().any(|v| v.gate != 0 && v.note == 69));
        if short {
            ui.rig.note_off(69);
        }
        ui.frame();
        assert_eq!(ui.top(), "SCENE", "short {short}: the note did not end it");
    }
}

#[test]
fn a_note_held_from_before_does_not_end_the_card() {
    let mut ui = at_rest();
    ui.rig.note_on(60);
    open_from_menu(&mut ui, VIEWS_MENU_SCENE);
    ui.frames(10);
    assert_eq!(ui.top(), "TITLE");
}

#[test]
fn nothing_draws_over_the_card() {
    let mut ui = at_rest();
    open_from_menu(&mut ui, VIEWS_MENU_SCENE);
    ui.rig.button(LIVE_BUTTON_FX);
    ui.rig.button(LIVE_BUTTON_OCT_UP);
    for _ in 0..5 {
        ui.frame();
        assert!(ui.toast().is_none());
        assert_eq!(ui.v.s.raster_frames, 0);
    }
    run_card(&mut ui);
    ui.settle();
    assert!(
        matches!(ui.toast().as_deref(), None | Some("FIRE")),
        "what changed under the card was taken as seen (SCENE's own name is the only toast)"
    );
}

#[test]
fn the_cards_option_steps_off_once_always() {
    let mut ui = at_rest();
    assert_eq!(ui.v.cards, CARDS_ONCE, "ONCE at power-on");
    crate::views::open_setting(&mut ui, VIEWS_SETTINGS_CARDS);
    assert_eq!(
        ui.menu_line(VIEWS_SETTINGS_CARDS as usize),
        format!("CARDS{:>9}", "ONCE"),
        "SETTINGS shows CARDS ONCE"
    );
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(ui.v.cards, CARDS_OFF);
    crate::views::settings_settle(&mut ui);
    assert_eq!(
        ui.menu_line(VIEWS_SETTINGS_CARDS as usize),
        format!("CARDS{:>9}", "OFF"),
        "SETTINGS shows CARDS OFF"
    );
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(ui.v.cards, CARDS_OFF, "OFF is the end");
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.v.cards, CARDS_ONCE);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.v.cards, CARDS_ALWAYS);
    crate::views::settings_settle(&mut ui);
    assert_eq!(
        ui.menu_line(VIEWS_SETTINGS_CARDS as usize),
        format!("CARDS{:>9}", "ALWAYS"),
        "SETTINGS shows CARDS ALWAYS"
    );
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.v.cards, CARDS_ALWAYS, "ALWAYS is the end");
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(ui.v.cards, CARDS_ONCE);
}

#[test]
fn cards_off_plays_none() {
    let mut ui = at_rest();
    ui.v.cards = CARDS_OFF;
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "LIVE", "no card back to LIVE");
    ui.settle();
    open_from_menu(&mut ui, VIEWS_MENU_SCENE);
    assert_eq!(ui.top(), "SCENE", "no card into SCENE");
    ui.settle();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.choose_menu_item(VIEWS_MENU_CHIP);
    assert_eq!(ui.top(), "CHIP", "no card into CHIP");
}

#[test]
fn a_card_plays_once_then_not() {
    let mut ui = at_rest();
    for (item, id, name) in [
        (VIEWS_MENU_CHIP, TITLE_CARD_CHIP, "CHIP"),
        (VIEWS_MENU_SCENE, TITLE_CARD_SCENE, "SCENE"),
    ] {
        if ui.top() == "LIVE" {
            ui.press(PANEL_BTN_HOME);
            ui.settle();
        }
        ui.highlight_menu_item(item);
        ui.frame_sent();
        ui.turn(PANEL_ENC_ALGORITHM, 1);
        assert_eq!(ui.top(), "TITLE", "{name}: the card the first time");
        assert_eq!(u32::from(card(&ui).id), id);
        run_card(&mut ui);
        settle(&mut ui);
        assert_eq!(ui.top(), name);
        ui.press(PANEL_BTN_HOME);
        ui.settle();
        assert_eq!(ui.top(), "MENU");
        ui.turn(PANEL_ENC_ALGORITHM, 1);
        assert_ne!(ui.top(), "TITLE", "{name}: not the second time");
        ui.settle();
        assert_eq!(ui.top(), name);
        ui.press(PANEL_BTN_HOME);
        ui.settle();
    }
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "LIVE", "no THE OSCILLATOR");
}

#[test]
fn cards_always_plays_every_card() {
    let mut ui = at_rest();
    ui.v.cards = CARDS_ALWAYS;
    for _ in 0..2 {
        open_from_menu(&mut ui, VIEWS_MENU_CHIP);
        assert_eq!(ui.top(), "TITLE");
        assert_eq!(u32::from(card(&ui).id), TITLE_CARD_CHIP);
        run_card(&mut ui);
        settle(&mut ui);
        assert_eq!(ui.top(), "CHIP");
        ui.press(PANEL_BTN_HOME);
        ui.settle();
        ui.press(PANEL_BTN_HOME);
        assert_eq!(ui.top(), "TITLE", "the way back to LIVE too");
        assert_eq!(u32::from(card(&ui).id), TITLE_CARD_OSCILLATOR);
        run_card(&mut ui);
        settle(&mut ui);
        assert_eq!(ui.top(), "LIVE");
    }
}

#[test]
fn chip_from_the_menu_shows_its_card_then_cuts_to_chip() {
    let mut ui = at_rest();
    open_from_menu(&mut ui, VIEWS_MENU_CHIP);
    assert_eq!(ui.top(), "TITLE");
    assert_eq!(u32::from(card(&ui).id), TITLE_CARD_CHIP);
    while u32::from(card(&ui).phase) != TITLE_CARD_HOLD {
        ui.frame_sent();
    }
    ui.frame_sent();
    assert_shows_png(&ui, "title-chip");
    let frames = run_card(&mut ui);
    assert!(frames.len() >= TITLE_CARD_HOLD_FRAMES as usize - 2);
    settle(&mut ui);
    assert_eq!(ui.top(), "CHIP");
}
