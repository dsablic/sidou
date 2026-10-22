// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The help card (app/help_view.c) and HOME acting on its release (app/views.c `views_flow`): a
//! tap goes home, a hold of 600 ms opens the card of the view that shows, any button or encoder
//! closes it and does nothing else, and no frame of it sends more than 200 cells; and the
//! first-run tour (app/views.c `tour_frame`) that follows the full boot an install plays.

use std::ffi::CStr;

use sidou_sys::*;

use crate::views::Ui;
use crate::{drain, SPI_BUDGET_MS};

/// A frame, and what it sent (the panel's drain): the card draws against the cells pending.
fn frame(ui: &mut Ui) {
    ui.frame();
    drain(&mut ui.g);
}

/// HOME down and the frames until the card shows (at most a second), HOME still down.
fn hold_until_card(ui: &mut Ui) -> u32 {
    drain(&mut ui.g);
    ui.down(PANEL_BTN_HOME);
    for n in 0..25 {
        frame(ui);
        if ui.top() == "HELP" {
            return n;
        }
    }
    panic!("no help card after a second");
}

/// n characters of large text from (row, col): a blank cell is a space (the grid keeps the
/// character a blank cell was last a piece of).
fn big(ui: &Ui, row: usize, col: usize, n: usize) -> String {
    (0..n)
        .map(|i| {
            let k = row * 30 + col + 2 * i;
            if ui.g.ch[k] == b' ' {
                ' '
            } else {
                ui.g.src[k] as char
            }
        })
        .collect()
}

fn card_name(ui: &Ui) -> String {
    let head = big(ui, 1, 1, 14);
    assert!(head.starts_with("HELP"), "title bar: {head:?}");
    head["HELP".len()..].trim().to_string()
}

fn text(p: *const std::os::raw::c_char) -> String {
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// Line `l` of a card: the control and what it does (one string with a NUL between).
fn line(card: &help_card, l: usize) -> (String, String) {
    let p = unsafe { *card.lines.add(l) };
    let key = unsafe { CStr::from_ptr(p) };
    let what = unsafe { CStr::from_ptr(p.add(key.to_bytes().len() + 1)) };
    (
        key.to_string_lossy().into_owned(),
        what.to_string_lossy().into_owned(),
    )
}

#[test]
fn a_tap_goes_home_and_a_hold_shows_the_help_card() {
    let mut ui = Ui::without_cards();
    ui.settle();
    drain(&mut ui.g);
    // A tap in 100 ms: the menu.
    ui.down(PANEL_BTN_HOME);
    frame(&mut ui);
    frame(&mut ui);
    assert_eq!(ui.top(), "LIVE", "the press alone does not navigate");
    ui.up(PANEL_BTN_HOME);
    frame(&mut ui);
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "LIVE");
    ui.settle();
    // Held: the card, never the menu.
    let fx = ui.v.fx_select;
    let frames = hold_until_card(&mut ui);
    assert!(
        (13..=15).contains(&frames),
        "after about 600 ms, not {frames} frames"
    );
    assert_eq!(card_name(&ui), "LIVE");
    for _ in 0..12 {
        frame(&mut ui);
    }
    ui.up(PANEL_BTN_HOME);
    frame(&mut ui);
    assert_eq!(ui.top(), "HELP", "it stays open after the release");
    for _ in 0..10 {
        frame(&mut ui);
    }
    assert_eq!(ui.top(), "HELP");
    ui.turn(PANEL_ENC_SELECT, 1);
    frame(&mut ui);
    ui.settle();
    assert_eq!(ui.top(), "LIVE");
    assert_eq!(ui.v.fx_select, fx, "the selection untouched");
}

#[test]
fn the_card_is_the_one_of_the_view_that_shows() {
    for (name, open) in [
        ("MENU", 0),
        ("CHIP", 1),
        ("SCENE", 2),
        ("STEP", 3),
        ("ALL", 4),
        ("MIDI", 5),
    ] {
        let mut ui = Ui::without_cards();
        ui.settle();
        match open {
            0 => ui.press(PANEL_BTN_HOME),
            1 | 2 | 4 | 5 => {
                ui.press(PANEL_BTN_HOME);
                ui.settle();
                ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
                let item = [
                    0,
                    VIEWS_MENU_CHIP,
                    VIEWS_MENU_SCENE,
                    0,
                    VIEWS_MENU_STATUS,
                    VIEWS_MENU_MIDI,
                ][open];
                ui.turn(PANEL_ENC_SELECT, item as i32);
                frame(&mut ui);
                ui.turn(PANEL_ENC_ALGORITHM, 1);
            }
            _ => ui.press(PANEL_BTN_SEQ),
        }
        ui.settle();
        hold_until_card(&mut ui);
        for _ in 0..12 {
            frame(&mut ui);
        }
        assert_eq!(card_name(&ui), name, "open {open}");
        ui.up(PANEL_BTN_HOME);
        ui.turn(PANEL_ENC_K1, 1);
        frame(&mut ui);
        ui.settle();
        assert_ne!(ui.top(), "HELP", "open {open}");
        assert_ne!(ui.top(), "LIVE", "open {open}: still in the view");
    }
}

/// HOME -> SONG -> LOAD's page -> SEQ: STEP on a full stack (SCREEN_DEPTH).
fn full_stack() -> Ui {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.press(PANEL_BTN_SEQ);
    ui.settle();
    assert_eq!(ui.top(), "STEP");
    assert_eq!(ui.v.s.depth, SCREEN_DEPTH as u8, "the stack is full");
    ui
}

/// With no room for the card, HOME held does what a tap does: no silent hold that eats the
/// release.
#[test]
fn a_held_home_on_a_full_stack_goes_home_as_a_tap_does() {
    let mut ui = full_stack();
    ui.down(PANEL_BTN_HOME);
    for _ in 0..25 {
        frame(&mut ui);
    }
    ui.up(PANEL_BTN_HOME);
    for _ in 0..3 {
        frame(&mut ui);
    }
    ui.settle();
    assert_ne!(ui.top(), "STEP", "HOME did not go home");
    assert_ne!(ui.top(), "HELP");
}

/// MIDI learn that cannot open on a full stack says so.
#[test]
fn midi_learn_on_a_full_stack_says_there_is_no_room() {
    let mut ui = full_stack();
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *ui.v, &mut *ml);
    }
    ui.learn_on(PANEL_ENC_K1);
    assert_ne!(ui.top(), "LEARN");
    assert_eq!(ui.toast().as_deref(), Some("NO ROOM FOR LEARN"));
}

/// Every card is the card of a view, and every view with a card of its own opens it: the view
/// to open (SEQ n times from LIVE, or the menu's item with ALGORITHM turned) and the card's name.
#[test]
fn every_card_opens_over_its_own_view() {
    enum Open {
        Live,
        Seq(u32),
        Menu,
        Item(u32),
    }
    let all = [
        ("LIVE", Open::Live),
        ("MENU", Open::Menu),
        ("CHIP", Open::Item(VIEWS_MENU_CHIP)),
        ("SCENE", Open::Item(VIEWS_MENU_SCENE)),
        ("STEP", Open::Seq(1)),
        ("TRACKER", Open::Seq(2)),
        ("ORDER", Open::Seq(3)),
        ("INSTR", Open::Seq(4)),
        ("TABLE", Open::Seq(5)),
        ("ALL", Open::Item(VIEWS_MENU_STATUS)),
        ("MIDI", Open::Item(VIEWS_MENU_MIDI)),
        ("SONG", Open::Item(VIEWS_MENU_SONG)),
    ];
    assert_eq!(all.len(), HELP_CARDS as usize, "a card without a view here");
    let mut seen = [false; HELP_CARDS as usize];
    for (name, open) in all {
        let mut ui = Ui::without_cards();
        ui.settle();
        match open {
            Open::Live => {}
            Open::Seq(n) => {
                for _ in 0..n {
                    ui.press(PANEL_BTN_SEQ);
                    ui.settle();
                }
                assert_eq!(ui.top(), name, "SEQ x{n}");
            }
            Open::Menu => {
                ui.press(PANEL_BTN_HOME);
                ui.settle();
            }
            Open::Item(item) => {
                ui.press(PANEL_BTN_HOME);
                ui.settle();
                ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
                ui.turn(PANEL_ENC_SELECT, item as i32);
                frame(&mut ui);
                ui.turn(PANEL_ENC_ALGORITHM, 1);
                ui.settle();
            }
        }
        hold_until_card(&mut ui);
        for _ in 0..12 {
            frame(&mut ui);
        }
        assert_eq!(card_name(&ui), name);
        seen[ui.v.help.card as usize] = true;
        ui.up(PANEL_BTN_HOME);
    }
    assert!(seen.iter().all(|&s| s), "{seen:?}");
}

#[test]
fn a_held_home_in_the_menu_keeps_the_menu() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    hold_until_card(&mut ui);
    ui.up(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, 1);
    frame(&mut ui);
    ui.settle();
    assert_eq!(ui.top(), "MENU");
}

#[test]
fn any_control_closes_the_card_and_does_nothing_else_but_a_note_plays() {
    for what in ["OCT+", "FX", "K1", "ALGORITHM", "SAVE"] {
        let mut ui = Ui::without_cards();
        ui.settle();
        hold_until_card(&mut ui);
        for _ in 0..12 {
            frame(&mut ui);
        }
        ui.up(PANEL_BTN_HOME);
        let cutoff = |ui: &Ui| unsafe { live_param_get(&*ui.rig.live, LIVE_PARAM_CUTOFF) };
        let (octave, model, c, preset) = (
            ui.live().octave,
            ui.live().model,
            cutoff(&ui),
            ui.live().preset,
        );
        match what {
            "OCT+" => ui.press(PANEL_BTN_OCT_UP),
            "FX" => ui.press(PANEL_BTN_FX),
            "K1" => ui.turn(PANEL_ENC_K1, 5),
            "ALGORITHM" => ui.turn(PANEL_ENC_ALGORITHM, 1),
            _ => ui.press(SAVE_KEY_BUTTON),
        }
        frame(&mut ui);
        ui.settle();
        assert_eq!(ui.top(), "LIVE", "{what}: the card closes");
        assert_eq!(
            (
                ui.live().octave,
                ui.live().model,
                cutoff(&ui),
                ui.live().preset
            ),
            (octave, model, c, preset),
            "{what} did something"
        );
        assert!(ui.toast().is_none(), "{what}: a toast");
    }
    let mut ui = Ui::without_cards();
    ui.settle();
    hold_until_card(&mut ui);
    for _ in 0..12 {
        frame(&mut ui);
    }
    ui.rig.note_on(69);
    for _ in 0..4 {
        frame(&mut ui);
    }
    assert!(
        ui.live().voice.iter().any(|v| v.gate != 0 && v.note == 69),
        "the note plays"
    );
    assert_eq!(ui.top(), "HELP", "and the card stays");
}

#[test]
fn every_line_fits_14_characters_with_a_space_between() {
    let cards = unsafe { &help_cards };
    assert_eq!(cards.len(), HELP_CARDS as usize);
    for (c, card) in cards.iter().enumerate() {
        assert!(card.count as u32 <= HELP_LINES && card.count > 0);
        assert!(!text(card.name).is_empty() && text(card.name).len() <= 10);
        for l in 0..card.count as usize {
            let (key, what) = line(card, l);
            assert!(!key.is_empty() && !what.is_empty());
            assert!(
                key.len() + 1 + what.len() <= HELP_LINE_CHARS as usize,
                "card {c} line {l}: {key} {what}"
            );
        }
    }
}

#[test]
fn the_card_draws_its_lines_and_the_help_card_stays_within_the_frame_budget() {
    for open in 0..4 {
        let mut ui = Ui::without_cards();
        ui.settle();
        match open {
            1 => {
                ui.press(PANEL_BTN_HOME);
                ui.settle();
            }
            2 => {
                ui.press(PANEL_BTN_SEQ);
                ui.settle();
            }
            3 => {
                ui.rig.note_on(60);
                ui.settle();
            }
            _ => {}
        }
        drain(&mut ui.g);
        ui.down(PANEL_BTN_HOME);
        let mut shown = false;
        for _ in 0..40 {
            ui.frame();
            let sent = drain(&mut ui.g);
            assert!(sent.cells <= 200, "open {open}: {} cells", sent.cells);
            assert!(sent.spi_ms() <= SPI_BUDGET_MS);
            shown |= ui.top() == "HELP";
        }
        assert!(shown);
        let card = unsafe { &help_cards[ui.v.help.card as usize] };
        for l in 0..card.count as usize {
            let (key, what) = line(card, l);
            let want = format!("{:<14}", format!("{key} {what}"));
            let got = big(&ui, 4 + 3 * l, 1, 14);
            assert!(
                got.starts_with(&key) && got.trim_end().ends_with(&what),
                "{got:?} for {want:?}"
            );
        }
        // Nothing left to send once it is drawn.
        for _ in 0..3 {
            ui.frame();
            assert_eq!(drain(&mut ui.g).cells, 0);
        }
        ui.up(PANEL_BTN_HOME);
        ui.turn(PANEL_ENC_SELECT, 1);
        for _ in 0..30 {
            ui.frame();
            let sent = drain(&mut ui.g);
            assert!(
                sent.cells <= 200,
                "closing, open {open}: {} cells",
                sent.cells
            );
        }
        assert_ne!(ui.top(), "HELP");
    }
}

// ---- the first-run tour (app/views.c tour_frame) ----

/// The full boot an install plays (HOME not held), frames until it is over, the toast of the
/// last frame.
fn after_the_install_boot(ui: &mut Ui, home_held: i32) {
    drain(&mut ui.g);
    unsafe { views_boot(&mut *ui.v, 1, home_held) };
    for _ in 0..400 {
        frame(ui);
        if ui.top() == "LIVE" && ui.v.s.wipe == SCREEN_CUT as u8 {
            return;
        }
    }
    panic!("the boot did not end");
}

/// Frames until the toast says `text`, at most `ms` of them; its first frame.
fn until_toast(ui: &mut Ui, text: &str, ms: u32) {
    let end = ui.now + ms;
    while ui.now < end {
        if ui.toast().as_deref() == Some(text) {
            return;
        }
        frame(ui);
    }
    panic!(
        "no toast {text:?} in {ms} ms; the toast is {:?}",
        ui.toast()
    );
}

#[test]
fn the_tour_follows_the_first_boot_after_an_install() {
    let mut ui = Ui::without_cards();
    after_the_install_boot(&mut ui, 0);
    until_toast(&mut ui, "TURN K1", 400);
    // K1: the toast goes (the popup is the answer), and the next comes 1.6 s later.
    let start = ui.now;
    ui.turn(PANEL_ENC_K1, 3);
    frame(&mut ui);
    assert_ne!(
        ui.toast().as_deref(),
        Some("TURN K1"),
        "the step's toast stays"
    );
    until_toast(&mut ui, "PRESS ENV", 3000);
    assert!(
        ui.now - start >= 1600,
        "PRESS ENV came after {} ms",
        ui.now - start
    );
    // ENV steps an effect: its own toast, then the last step 1.6 s later.
    ui.press(PANEL_BTN_ENV);
    let start = ui.now;
    frame(&mut ui);
    assert_ne!(ui.toast().as_deref(), Some("PRESS ENV"));
    until_toast(&mut ui, "HOLD HOME FOR HELP", 3000);
    assert!(ui.now - start >= 1600);
    // HOME held: the card, and no more toasts.
    ui.down(PANEL_BTN_HOME);
    for _ in 0..20 {
        frame(&mut ui);
    }
    assert_eq!(ui.top(), "HELP");
    assert!(ui.toast().is_none(), "a toast over the card");
    ui.up(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, 1);
    for _ in 0..600 {
        frame(&mut ui);
        assert!(
            ui.toast().is_none(),
            "a toast after the tour: {:?}",
            ui.toast()
        );
    }
}

#[test]
fn an_unanswered_tour_runs_on_its_own_clock_and_ends() {
    let mut ui = Ui::without_cards();
    after_the_install_boot(&mut ui, 0);
    until_toast(&mut ui, "TURN K1", 400);
    until_toast(&mut ui, "PRESS ENV", 12_000);
    let t0 = ui.now;
    until_toast(&mut ui, "HOLD HOME FOR HELP", 12_000);
    let gap = ui.now - t0;
    assert!(
        (9_600..=9_680).contains(&gap),
        "8 s and 1.6 s, not {gap} ms"
    );
    for _ in 0..400 {
        frame(&mut ui);
    }
    assert!(ui.toast().is_none());
    assert_eq!(ui.v.tour, 0, "the tour is over");
}

#[test]
fn no_tour_when_home_asked_for_the_boot() {
    let mut ui = Ui::without_cards();
    drain(&mut ui.g);
    unsafe { views_boot(&mut *ui.v, 1, 1) };
    ui.hold(PANEL_BTN_HOME, true);
    ui.press(PANEL_BTN_HOME);
    ui.hold(PANEL_BTN_HOME, false);
    for _ in 0..1500 {
        frame(&mut ui);
        assert!(ui.toast().is_none(), "a toast at {} ms", ui.now);
    }
    assert_eq!(ui.v.tour, 0);
}

#[test]
fn no_tour_on_a_normal_boot() {
    let mut ui = Ui::without_cards();
    drain(&mut ui.g);
    unsafe { views_boot(&mut *ui.v, 0, 0) };
    for _ in 0..1500 {
        frame(&mut ui);
        assert!(ui.toast().is_none(), "a toast at {} ms", ui.now);
    }
    assert_eq!(ui.v.tour, 0);
}

/// Ergonomics review E30 (M2 review 5.13): ENV pressed in the menu while the tour's first step
/// shows: the tour ends its own toast only, and ENV's (its effect's name) shows.
#[test]
fn an_action_during_the_tour_keeps_its_own_toast() {
    let mut ui = Ui::without_cards();
    after_the_install_boot(&mut ui, 0);
    until_toast(&mut ui, "TURN K1", 400);
    ui.press(PANEL_BTN_HOME);
    frame(&mut ui);
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_ENV);
    for _ in 0..3 {
        frame(&mut ui);
    }
    let t = ui.toast().unwrap_or_default();
    assert!(
        !t.is_empty() && t != "TURN K1",
        "ENV's toast is gone: {t:?}"
    );
}

/// Ergonomics review E29 (M2 review 5.12): a toast raised while a help card shows waits under
/// it (nothing draws over the card, whose frames stay within 200 cells), then shows.
#[test]
fn a_toast_waits_under_the_help_card_and_shows_after_it() {
    let mut ui = Ui::without_cards();
    ui.settle();
    hold_until_card(&mut ui);
    unsafe { screen_toast(&mut ui.v.s, c"SONG SAVED".as_ptr(), SCREEN_TOAST_MS) };
    for _ in 0..30 {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(sent.cells <= 200, "{} cells", sent.cells);
        assert!(ui.toast().is_none(), "a toast over the card");
    }
    ui.up(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, 1);
    let mut seen = false;
    for _ in 0..30 {
        frame(&mut ui);
        seen |= ui.toast().as_deref() == Some("SONG SAVED");
    }
    assert_ne!(ui.top(), "HELP");
    assert!(seen, "the toast after the card");
}

/// Ergonomics review E28 (M2 review 5.14): the keys under a help card over TRACKER play the
/// song view's jam (its instrument, its voice), not LIVE's instrument.
#[test]
fn keys_under_a_help_card_play_the_view_under_it() {
    use crate::song_view::{open_tracker, Call, Song};
    let mut s = Song::playing(Some(sidou_tests::song::Song::device()));
    open_tracker(&mut s.ui);
    hold_until_card(&mut s.ui);
    s.ui.up(PANEL_BTN_HOME);
    s.calls();
    s.ui.note_keys(1 << 16, 1 << 16);
    s.ui.note_keys(0, 0);
    s.ui.frame();
    assert_eq!(s.ui.top(), "HELP", "the card stays");
    assert!(
        s.calls().iter().any(|c| matches!(c, Call::Jam(..))),
        "the jam plays the key"
    );
    assert!(
        s.ui.live().voice.iter().all(|v| v.gate == 0),
        "LIVE's instrument stays silent"
    );
}
