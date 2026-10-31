// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeSet;
use std::ffi::CStr;

use sidou_sys::*;

use crate::views::Ui;
use crate::{c_text, drain, SPI_BUDGET_MS};

const FRAME_CELL_BUDGET: usize = 200;
const FRAMES_TO_DRAW_A_CARD: u32 = 12;

fn frames_within_the_cell_budget(ui: &mut Ui, n: u32, what: &str) {
    for _ in 0..n {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(
            sent.cells <= FRAME_CELL_BUDGET,
            "{what}: {} cells",
            sent.cells
        );
    }
}

fn hold_until_card(ui: &mut Ui) -> u32 {
    drain(&mut ui.g);
    ui.down(PANEL_BTN_HOME);
    for n in 0..25 {
        ui.frame_sent();
        if ui.top() == "HELP" {
            return n;
        }
    }
    panic!("no help card after a second");
}

fn hold_until_card_drawn(ui: &mut Ui) {
    hold_until_card(ui);
    ui.frames_sent(FRAMES_TO_DRAW_A_CARD);
}

fn large_text_at(ui: &Ui, row: usize, col: usize, n: usize) -> String {
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

fn title(ui: &Ui) -> (String, String) {
    let head = large_text_at(ui, 1, 1, 14);
    let (left, name) = head.split_at(4);
    let left = left.trim().to_string();
    assert!(
        ["HELP", "1/2", "2/2"].contains(&left.as_str()),
        "title bar: {head:?}"
    );
    (left, name.trim().to_string())
}

fn card_name(ui: &Ui) -> String {
    title(ui).1
}

fn line_in_state_words(h: &help_view, card: &help_card, l: usize) -> (String, String) {
    let p = unsafe { *card.lines.add(l) };
    let key = unsafe { CStr::from_ptr(p) };
    let mut buf = [0 as std::os::raw::c_char; HELP_TEXT_WORD as usize];
    let what = unsafe { help_view_what(h, p.add(key.to_bytes().len() + 1), buf.as_mut_ptr()) };
    (key.to_string_lossy().into_owned(), c_text(what))
}

fn line_as_coded(card: &help_card, l: usize) -> (String, String) {
    let p = unsafe { *card.lines.add(l) };
    let key = unsafe { CStr::from_ptr(p) };
    let what = unsafe { CStr::from_ptr(p.add(key.to_bytes().len() + 1)) };
    (
        key.to_string_lossy().into_owned(),
        what.to_string_lossy().into_owned(),
    )
}

fn lines_as_coded(card: &help_card) -> Vec<(String, String)> {
    (0..usize::from(card.count))
        .map(|l| line_as_coded(card, l))
        .collect()
}

fn lines_in_state_words(h: &help_view, card: &help_card) -> Vec<(String, String)> {
    (0..usize::from(card.count))
        .map(|l| line_in_state_words(h, card, l))
        .collect()
}

fn close_with_k1(ui: &mut Ui) {
    ui.up(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_K1, 1);
    ui.frame_sent();
    ui.settle();
    assert_ne!(ui.top(), "HELP");
}

#[test]
fn a_tap_goes_home_and_a_hold_shows_the_help_card() {
    let mut ui = Ui::without_cards();
    ui.settle();
    drain(&mut ui.g);
    ui.down(PANEL_BTN_HOME);
    ui.frames_sent(2);
    assert_eq!(ui.top(), "LIVE", "the press alone does not navigate");
    ui.up(PANEL_BTN_HOME);
    ui.frame_sent();
    assert_eq!(ui.top(), "MENU", "a tap goes to the menu");
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "LIVE");
    ui.settle();
    let fx = ui.v.fx_select;
    let held_frames = hold_until_card(&mut ui);
    assert!(
        (13..=15).contains(&held_frames),
        "after about 600 ms, not {held_frames} frames"
    );
    assert_eq!(card_name(&ui), "LIVE", "held: the card, never the menu");
    ui.frames_sent(FRAMES_TO_DRAW_A_CARD);
    ui.up(PANEL_BTN_HOME);
    ui.frame_sent();
    assert_eq!(ui.top(), "HELP", "it stays open after the release");
    ui.frames_sent(10);
    assert_eq!(ui.top(), "HELP");
    let cutoff = unsafe { live_param_get(&*ui.rig.live, LIVE_PARAM_CUTOFF) };
    ui.turn(PANEL_ENC_K1, 1);
    ui.frame_sent();
    ui.settle();
    assert_eq!(ui.top(), "LIVE");
    assert_eq!(ui.v.fx_select, fx, "the selection untouched");
    assert_eq!(
        unsafe { live_param_get(&*ui.rig.live, LIVE_PARAM_CUTOFF) },
        cutoff,
        "the cutoff untouched"
    );
}

#[test]
fn the_card_is_the_one_of_the_view_that_shows() {
    for (name, how) in [
        ("MENU", Open::Menu),
        ("CHIP", Open::Item(VIEWS_MENU_CHIP)),
        ("SCENE", Open::Item(VIEWS_MENU_SCENE)),
        ("STEP", Open::Seq(1)),
        ("STATUS", Open::Item(VIEWS_MENU_STATUS)),
        ("MIDI", Open::Item(VIEWS_MENU_MIDI)),
    ] {
        let mut ui = Ui::without_cards();
        ui.settle();
        open(&mut ui, &how);
        ui.settle();
        hold_until_card_drawn(&mut ui);
        assert_eq!(card_name(&ui), name);
        close_with_k1(&mut ui);
        assert_ne!(ui.top(), "LIVE", "{name}: still in the view");
    }
}

fn fill_the_stack_song_load_then_step(ui: &mut Ui) {
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.press(PANEL_BTN_SEQ);
    ui.settle();
    assert_eq!(ui.v.s.depth, SCREEN_DEPTH as u8, "the stack is full");
}

fn full_stack() -> Ui {
    let mut ui = Ui::without_cards();
    fill_the_stack_song_load_then_step(&mut ui);
    assert_eq!(ui.top(), "STEP");
    ui
}

#[test]
fn a_held_home_on_a_full_stack_shows_the_card_in_its_own_slot_and_a_tap_still_goes_home() {
    let mut ui = full_stack();
    hold_until_card_drawn(&mut ui);
    assert_eq!(card_name(&ui), "STEP");
    assert_eq!(ui.v.s.depth, SCREEN_SLOTS as u8);
    close_with_k1(&mut ui);
    assert_eq!(ui.top(), "STEP");
    assert_eq!(ui.v.s.depth, SCREEN_DEPTH as u8);
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_ne!(ui.top(), "STEP", "a tap goes home");
}

#[test]
fn midi_learn_on_a_full_stack_says_there_is_no_room() {
    let mut ui = full_stack();
    let _ml = ui.attach_learn();
    ui.learn_on(PANEL_ENC_K1);
    assert_ne!(ui.top(), "LEARN");
    assert_eq!(ui.toast().as_deref(), Some("NO ROOM FOR LEARN"));
}

enum Open {
    Seq(u32),
    Menu,
    Item(u32),
    SongAction {
        action: u32,
        select_turns: i32,
        algorithm_turns: i32,
    },
}

fn highlight_menu_item(ui: &mut Ui, item: u32) {
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.highlight_menu_item(item);
}

fn open(ui: &mut Ui, open: &Open) {
    match *open {
        Open::Seq(n) => {
            for _ in 0..n {
                ui.press(PANEL_BTN_SEQ);
                ui.settle();
            }
        }
        Open::Menu => {
            ui.press(PANEL_BTN_HOME);
            ui.settle();
        }
        Open::Item(item) => {
            highlight_menu_item(ui, item);
            ui.frame_sent();
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.settle();
        }
        Open::SongAction {
            action,
            select_turns,
            algorithm_turns,
        } => {
            highlight_menu_item(ui, VIEWS_MENU_SONG);
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.settle();
            ui.turn(PANEL_ENC_SELECT, action as i32);
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.turn(PANEL_ENC_SELECT, select_turns);
            ui.turn(PANEL_ENC_ALGORITHM, algorithm_turns);
            ui.settle();
        }
    }
}

#[test]
fn a_held_home_in_the_menu_keeps_the_menu() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    hold_until_card(&mut ui);
    ui.up(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.frame_sent();
    ui.settle();
    assert_eq!(ui.top(), "MENU");
}

#[test]
fn the_song_views_cards_say_undo_is_one_step() {
    for id in [HELP_CARD_STEP, HELP_CARD_TRACKER] {
        let card = unsafe { &help_cards[id as usize] };
        let lines = lines_as_coded(card);
        assert!(
            lines.contains(&("E+LFO".to_owned(), "ONE UNDO".to_owned())),
            "{}: {lines:?}",
            c_text(card.name)
        );
    }
}

#[test]
fn any_control_closes_the_card_and_does_nothing_else_but_a_note_plays() {
    for what in ["OCT+", "FX", "K1", "ALGORITHM", "SAVE"] {
        let mut ui = Ui::without_cards();
        ui.settle();
        hold_until_card_drawn(&mut ui);
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
        ui.frame_sent();
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
    hold_until_card_drawn(&mut ui);
    ui.rig.note_on(69);
    ui.frames_sent(4);
    assert!(
        ui.live().voice.iter().any(|v| v.gate != 0 && v.note == 69),
        "the note plays"
    );
    assert_eq!(ui.top(), "HELP", "and the card stays");
}

fn longest_word(code: u8) -> usize {
    let mut h = help_view::default();
    let what = [code as std::os::raw::c_char, 0];
    let mut most = 0;
    for arg in 0..=0x0fffu16 {
        h.arg = arg;
        let mut buf = [0 as std::os::raw::c_char; HELP_TEXT_WORD as usize];
        let w = c_text(unsafe { help_view_what(&h, what.as_ptr(), buf.as_mut_ptr()) });
        most = most.max(w.len());
    }
    most
}

#[test]
fn every_line_fits_14_characters_with_a_space_between() {
    let cards = unsafe { &help_cards };
    assert_eq!(cards.len(), HELP_CARDS as usize);
    for (c, card) in cards.iter().enumerate() {
        let n = card.count as u32;
        assert!(n <= HELP_LINES * HELP_PAGES && n > 0, "card {c}: {n} lines");
        let name = c_text(card.name);
        let name_room_after_help_or_page_and_a_space = if n > HELP_LINES { 10 } else { 9 };
        let room = name_room_after_help_or_page_and_a_space;
        assert!(
            !name.is_empty() && name.len() <= room,
            "card {c}: {name:?} in {room}"
        );
        for l in 0..card.count as usize {
            let (key, what) = line_as_coded(card, l);
            assert!(!key.is_empty() && !what.is_empty());
            let first = what.as_bytes()[0];
            let len = if first < b' ' {
                assert_eq!(what.len(), 1, "card {c} line {l}: a code is the whole text");
                longest_word(first)
            } else {
                what.len()
            };
            assert!(
                key.len() + 1 + len <= HELP_LINE_CHARS as usize,
                "card {c} line {l}: {key} {what:?} ({len})"
            );
        }
    }
}

fn assert_page_drawn(ui: &Ui, what: &str) {
    let h = &ui.v.help;
    let card = unsafe { &help_cards[h.card as usize] };
    let first = usize::from(h.page) * HELP_LINES as usize;
    for i in 0..HELP_LINES as usize {
        let got = large_text_at(ui, 4 + 3 * i, 1, 14);
        if first + i >= card.count as usize {
            assert_eq!(got.trim(), "", "{what}: line {i} past the card's");
            continue;
        }
        let (key, what_does) = line_in_state_words(h, card, first + i);
        assert!(
            got.starts_with(&key) && got.trim_end().ends_with(&what_does),
            "{what}: {got:?} for {key} {what_does}"
        );
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
            assert!(
                sent.cells <= FRAME_CELL_BUDGET,
                "open {open}: {} cells",
                sent.cells
            );
            assert!(sent.spi_ms() <= SPI_BUDGET_MS);
            shown |= ui.top() == "HELP";
        }
        assert!(shown);
        assert_page_drawn(&ui, &format!("open {open}"));
        for _ in 0..3 {
            ui.frame();
            assert_eq!(drain(&mut ui.g).cells, 0, "nothing left to send once drawn");
        }
        ui.up(PANEL_BTN_HOME);
        ui.turn(PANEL_ENC_K1, 1);
        frames_within_the_cell_budget(&mut ui, 30, &format!("closing, open {open}"));
        assert_ne!(ui.top(), "HELP");
    }
}

#[test]
fn select_turns_a_card_of_two_pages_and_closes_one_of_one() {
    let mut ui = Ui::without_cards();
    ui.settle();
    hold_until_card_drawn(&mut ui);
    assert!(unsafe { help_cards[HELP_CARD_LIVE as usize].count } as u32 > HELP_LINES);
    assert_eq!(title(&ui), ("1/2".to_string(), "LIVE".to_string()));
    assert_page_drawn(&ui, "page 1");
    ui.up(PANEL_BTN_HOME);
    for (turn, page) in [(1, "2/2"), (1, "1/2"), (-1, "2/2")] {
        ui.turn(PANEL_ENC_SELECT, turn);
        frames_within_the_cell_budget(&mut ui, FRAMES_TO_DRAW_A_CARD, page);
        assert_eq!(ui.top(), "HELP", "SELECT closed the card");
        assert_eq!(title(&ui).0, page);
        assert_page_drawn(&ui, page);
    }
    assert_eq!(ui.v.live.page, 0, "SELECT paged LIVE under the card");
    close_with_k1(&mut ui);
    assert_eq!(ui.top(), "LIVE");
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    hold_until_card_drawn(&mut ui);
    assert_eq!(
        title(&ui),
        ("HELP".to_string(), "MENU".to_string()),
        "a card of one page says HELP"
    );
    ui.up(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.frame_sent();
    ui.settle();
    assert_eq!(ui.top(), "MENU");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ctl {
    Button(u32),
    Encoder(u32),
    Keys,
}

fn controls_of(key: &str) -> Option<Vec<(Ctl, bool)>> {
    const INFO: [&str; 8] = [
        "SETTINGS",
        "SETUP",
        "MACROS",
        "WIPES",
        "KEEPS",
        "QUANT",
        "HOLD HOME",
        "SPREAD",
    ];
    if INFO.contains(&key) {
        return None;
    }
    let (edit, rest) = match key.split_once('+') {
        Some(("E" | "EDIT", rest)) => (true, rest),
        Some(("STEP" | "GLO" | "SAVE" | "ARP" | "SCL", _)) => return Some(Vec::new()),
        _ => (false, key),
    };
    if rest.starts_with("KEY") {
        return Some(vec![(Ctl::Keys, edit)]);
    }
    let knobs = |range: std::ops::RangeInclusive<u32>| {
        range
            .map(|k| Ctl::Encoder(PANEL_ENC_K1 + k - 1))
            .collect::<Vec<_>>()
    };
    let mut out = Vec::new();
    let words: Vec<&str> = if rest == "OCT-/+" {
        vec!["OCT-", "OCT+"]
    } else {
        rest.split([' ', '/']).collect()
    };
    for w in words {
        let ctls = match w {
            "SELECT" => vec![Ctl::Encoder(PANEL_ENC_SELECT)],
            "ALG" => vec![Ctl::Encoder(PANEL_ENC_ALGORITHM)],
            "PRESETS" | "PRE" => vec![Ctl::Encoder(PANEL_ENC_PRESETS)],
            "K" | "K1-K4" => knobs(1..=4),
            "K2-K4" => knobs(2..=4),
            "K1" => knobs(1..=1),
            "K2" => knobs(2..=2),
            "K3" => knobs(3..=3),
            "K4" => knobs(4..=4),
            "OCT-" => vec![Ctl::Button(PANEL_BTN_OCT_DOWN)],
            "OCT+" => vec![Ctl::Button(PANEL_BTN_OCT_UP)],
            "FX" => vec![Ctl::Button(PANEL_BTN_FX)],
            "SCL" => vec![Ctl::Button(PANEL_BTN_SCL)],
            "ENV" => vec![Ctl::Button(PANEL_BTN_ENV)],
            "LFO" => vec![Ctl::Button(PANEL_BTN_LFO)],
            "EDIT" => vec![Ctl::Button(PANEL_BTN_EDIT)],
            "GLO" => vec![Ctl::Button(PANEL_BTN_GLO)],
            "HOME" => vec![Ctl::Button(PANEL_BTN_HOME)],
            "SAVE" => vec![Ctl::Button(PANEL_BTN_SAVE)],
            "ARP" => vec![Ctl::Button(PANEL_BTN_ARP)],
            "SEQ" => vec![Ctl::Button(PANEL_BTN_SEQ)],
            "PLAY" => vec![Ctl::Button(PANEL_BTN_PLAY)],
            "REC" => vec![Ctl::Button(PANEL_BTN_REC)],
            _ => panic!("{key:?}: no control is called {w:?}"),
        };
        out.extend(ctls.into_iter().map(|c| (c, edit)));
    }
    Some(out)
}

struct Ctx {
    s: crate::song_view::Song,
    _io: Box<song_io>,
    _tune: Option<(Box<crate::sid_view::Tune>, crate::sid_view::Bound)>,
}

const CONTEXTS: [&str; 43] = [
    "LIVE",
    "LAB OSC",
    "LAB ENV",
    "LAB FILTER",
    "LAB MOD",
    "LAB FX",
    "LAB MACRO",
    "LAB NAME",
    "LIVE SLOT",
    "MENU",
    "SETTINGS",
    "STORE",
    "RESET",
    "MIDI",
    "CHIP TABS",
    "CHIP",
    "CHIP SLOT",
    "EXPERT",
    "SCENE",
    "STEP",
    "HELD STEP",
    "STEP REC",
    "TRACKER",
    "TRACK EDIT",
    "TRACK REC",
    "ORDER",
    "INSTR",
    "TABLE",
    "ARP",
    "STATUS",
    "ABOUT",
    "CHARSET",
    "SONG",
    "FILES",
    "SAVE AS",
    "CONFIRM",
    "SIDS",
    "PLAYING",
    "UNSAVED",
    "LIFT",
    "STEP FULL",
    "CHIP MOD",
    "CHIP FX",
];

fn song_action(action: u32) -> Open {
    Open::SongAction {
        action,
        select_turns: 0,
        algorithm_turns: 0,
    }
}

fn menu_item(ui: &mut Ui, item: u32) {
    open(ui, &Open::Item(item));
}

fn open_ctx(name: &str) -> Ctx {
    use crate::song_view::{open_tracker, Song};
    let mut s = Song::playing(Some(sidou_tests::song::Song::device()));
    let mut io = Box::new(song_io {
        dirty: 1,
        ..Default::default()
    });
    io.hooks.builtin = Some(builtin_song_never_read);
    unsafe { views_song_io(&mut *s.ui.v, &mut *io) };
    let ui = &mut s.ui;
    ui.settle();
    let seq = |ui: &mut Ui, n: u32| open(ui, &Open::Seq(n));
    let mut tune = None;
    match name {
        "LIVE" => {}
        "LIVE SLOT" => {
            ui.turn(PANEL_ENC_SELECT, 1);
            ui.settle();
        }
        "MENU" => open(ui, &Open::Menu),
        "SETTINGS" => crate::views::open_setting(ui, VIEWS_SETTINGS_KEYS),
        "STORE" => crate::views::open_setting(ui, VIEWS_SETTINGS_STORE),
        "RESET" => crate::views::open_setting(ui, VIEWS_SETTINGS_RESET),
        "MIDI" => menu_item(ui, VIEWS_MENU_MIDI),
        "LAB OSC" | "LAB ENV" | "LAB FILTER" | "LAB MOD" | "LAB FX" | "LAB MACRO" | "LAB NAME" => {
            menu_item(ui, VIEWS_MENU_LAB);
            let page = [
                "LAB OSC",
                "LAB ENV",
                "LAB FILTER",
                "LAB MOD",
                "LAB FX",
                "LAB MACRO",
            ]
            .iter()
            .position(|n| *n == name);
            crate::lab_view::to_page(ui, page.unwrap_or(0) as u32);
            if page.is_none() {
                ui.down(PANEL_BTN_EDIT);
                ui.press(SAVE_KEY_BUTTON);
                ui.up(PANEL_BTN_EDIT);
                ui.settle();
                assert_ne!(ui.v.lab.mode, 0, "the name page");
            }
        }
        "SCENE" => menu_item(ui, VIEWS_MENU_SCENE),
        "STATUS" => menu_item(ui, VIEWS_MENU_STATUS),
        "ABOUT" => menu_item(ui, VIEWS_MENU_ABOUT),
        "CHARSET" => menu_item(ui, VIEWS_MENU_CHARSET),
        "SONG" => menu_item(ui, VIEWS_MENU_SONG),
        "FILES" => open(ui, &song_action(0)),
        "SAVE AS" => open(ui, &song_action(2)),
        "CONFIRM" => open(ui, &song_action(1)),
        "CHIP TABS" | "CHIP" | "CHIP SLOT" | "EXPERT" | "CHIP MOD" | "CHIP FX" => {
            menu_item(ui, VIEWS_MENU_CHIP);
            ui.turn(PANEL_ENC_SELECT, -8);
            let to = match name {
                "CHIP TABS" => 0,
                "CHIP" => 1,
                "CHIP SLOT" => 2,
                "CHIP MOD" => 3,
                "CHIP FX" => 4,
                _ => 5,
            };
            ui.turn(PANEL_ENC_SELECT, to);
            ui.settle();
        }
        "STEP" | "HELD STEP" | "STEP REC" => {
            seq(ui, 1);
            if name == "HELD STEP" {
                ui.note_keys(1 << 5, 1 << 5);
                ui.frame();
            } else if name == "STEP REC" {
                ui.press(PANEL_BTN_PLAY);
                ui.press(PANEL_BTN_REC);
                crate::song_view::settle(ui);
            }
        }
        "TRACKER" | "TRACK EDIT" | "TRACK REC" => {
            open_tracker(ui);
            if name == "TRACK REC" {
                ui.press(PANEL_BTN_PLAY);
            }
            if name != "TRACKER" {
                ui.press(PANEL_BTN_REC);
            }
            crate::song_view::settle(ui);
        }
        "ORDER" => seq(ui, 3),
        "INSTR" => seq(ui, 4),
        "TABLE" => seq(ui, 5),
        "ARP" => {
            seq(ui, 5);
            ui.press(PANEL_BTN_ARP);
            ui.settle();
        }
        "SIDS" | "PLAYING" | "UNSAVED" => {
            let bound = crate::sid_view::bind(ui, &["ALPHA", "BETA"]);
            crate::sid_view::open_sids(ui);
            let to_the_unsaved_question = name != "SIDS";
            let discarded_now_playing = name == "PLAYING";
            if to_the_unsaved_question {
                ui.turn(PANEL_ENC_ALGORITHM, 1);
            }
            if discarded_now_playing {
                ui.turn(PANEL_ENC_ALGORITHM, 1);
            }
            ui.settle();
            tune = Some(bound);
        }
        "LIFT" => {
            let mut bound = crate::sid_view::bind(ui, &["ALPHA"]);
            open(ui, &Open::Menu);
            crate::sid_view::offer(&mut bound.0, "ALPHA V1", 1);
            ui.settle();
            assert_eq!(ui.top(), "LIFTED SOUND");
            tune = Some(bound);
        }
        "STEP FULL" => fill_the_stack_song_load_then_step(ui),
        _ => panic!("no context {name}"),
    }
    Ctx {
        s,
        _io: io,
        _tune: tune,
    }
}

unsafe extern "C" fn builtin_song_never_read(
    _: *mut std::ffi::c_void,
    _: u32,
    _: *mut u8,
    _: u32,
) -> u32 {
    0
}

fn card_over(name: &str) -> (u32, u16) {
    let mut c = open_ctx(name);
    let ui = &mut c.s.ui;
    let under = (ui.top(), ui.v.s.depth);
    hold_until_card_drawn(ui);
    let id = (u32::from(ui.v.help.card), ui.v.help.arg);
    let want = match name {
        "STEP FULL" => "STEP",
        "CHIP MOD" | "CHIP FX" => "CHIP SLOT",
        _ => name,
    };
    assert_eq!(card_name(ui), want, "over {under:?}");
    assert_page_drawn(ui, name);
    close_with_k1(ui);
    assert_eq!(
        (ui.top(), ui.v.s.depth),
        under,
        "{name}: the view as it was"
    );
    id
}

#[test]
fn every_view_and_page_holds_home_to_its_own_card() {
    let mut seen = [false; HELP_CARDS as usize];
    for name in CONTEXTS {
        let (id, _) = card_over(name);
        seen[id as usize] = true;
    }
    assert!(seen.iter().all(|&s| s), "a card no view opens: {seen:?}");
}

#[test]
fn lab_help_cards_match_their_goldens() {
    for (name, golden_name, pages) in [
        ("LAB OSC", "help-lab-osc", 2),
        ("LAB ENV", "help-lab-envelope", 2),
        ("LAB FILTER", "help-lab-filter", 2),
        ("LAB MOD", "help-lab-mod", 2),
        ("LAB FX", "help-lab-fx", 2),
        ("LAB MACRO", "help-lab-macro", 2),
        ("LAB NAME", "help-lab-name", 1),
    ] {
        let mut c = open_ctx(name);
        let ui = &mut c.s.ui;
        if name == "LAB FX" {
            let to_overdrive = 2;
            ui.turn(PANEL_ENC_ALGORITHM, to_overdrive);
            ui.settle();
        }
        hold_until_card_drawn(ui);
        assert_eq!(card_name(ui), name);
        assert_page_drawn(ui, name);
        crate::sid_view::golden(ui, golden_name);
        if pages == 2 {
            ui.turn(PANEL_ENC_SELECT, 1);
            ui.frames_sent(FRAMES_TO_DRAW_A_CARD);
            assert_eq!(ui.top(), "HELP", "SELECT turned the card");
            assert_page_drawn(ui, name);
            crate::sid_view::golden(ui, &format!("{golden_name}-2"));
        }
        close_with_k1(ui);
    }
}

#[test]
fn a_slot_card_names_its_algorithms_controls_and_rec_its_quantise() {
    let words = |name: &str, setup: &dyn Fn(&mut Ui)| {
        let mut c = open_ctx(name);
        setup(&mut c.s.ui);
        let ui = &mut c.s.ui;
        hold_until_card_drawn(ui);
        let h = ui.v.help;
        let lines = lines_in_state_words(&h, unsafe { &help_cards[h.card as usize] });
        close_with_k1(ui);
        lines
    };
    let knob = |lines: &[(String, String)], k: &str| {
        lines
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, w)| w.clone())
            .unwrap()
    };
    for name in ["CHIP SLOT", "LIVE SLOT"] {
        let off = words(name, &|_| {});
        for k in ["K1", "K2", "K3", "K4"] {
            assert_eq!(knob(&off, k), "UNUSED", "{name}: OFF's {k}");
        }
        let tape_then_overdrive = words(name, &|ui| {
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.settle();
        });
        let got: Vec<String> = ["K1", "K2", "K3", "K4"]
            .iter()
            .map(|k| knob(&tape_then_overdrive, k))
            .collect();
        assert_eq!(got, ["DRIVE", "TONE", "BIAS", "MIX"], "{name}");
    }
    let mut c = open_ctx("TRACK REC");
    c.s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    let ui = &mut c.s.ui;
    hold_until_card_drawn(ui);
    let h = ui.v.help;
    let card = unsafe { &help_cards[h.card as usize] };
    assert!(lines_in_state_words(&h, card).contains(&("QUANT".to_string(), "COUNT-IN".to_string())));
}

fn probes() -> Vec<(screen_event, Ctl, bool)> {
    let mut out = Vec::new();
    for edit in [false, true] {
        let held = if edit { 1 << PANEL_BTN_EDIT } else { 0 };
        for b in 0..14u32 {
            if [
                PANEL_BTN_HOME,
                PANEL_BTN_SAVE,
                PANEL_BTN_SEQ,
                PANEL_BTN_PLAY,
                PANEL_BTN_REC,
            ]
            .contains(&b)
            {
                continue;
            }
            let e = screen_event {
                kind: SCREEN_PRESS as u8,
                id: b as u8,
                steps: 0,
                held: held | 1 << b,
            };
            out.push((e, Ctl::Button(b), edit));
        }
        for enc in 0..PANEL_ENCODERS {
            for steps in [1i16, -1] {
                let e = screen_event {
                    kind: SCREEN_TURN as u8,
                    id: enc as u8,
                    steps,
                    held,
                };
                out.push((e, Ctl::Encoder(enc), edit));
            }
        }
    }
    out
}

fn takes(ui: &mut Ui, e: &screen_event) -> bool {
    let top = ui.v.s.stack[usize::from(ui.v.s.depth) - 1];
    let v = unsafe { &*top.v };
    if let Some(input) = v.input {
        if unsafe { input(&mut ui.v.s, top.state, e) } != 0 {
            return true;
        }
    }
    if e.kind == SCREEN_TURN as u8 {
        if let Some(global) = ui.v.s.global {
            return unsafe { global(ui.v.s.global_ctx, e) } != 0;
        }
    }
    false
}

fn saves(name: &str) -> bool {
    matches!(
        name,
        "CHIP"
            | "LAB OSC"
            | "LAB ENV"
            | "LAB FILTER"
            | "LAB MOD"
            | "LAB FX"
            | "LAB MACRO"
            | "LAB NAME"
            | "EXPERT"
            | "MENU"
            | "SONG"
            | "FILES"
            | "CONFIRM"
            | "SAVE AS"
            | "STORE"
            | "RESET"
            | "STEP"
            | "HELD STEP"
            | "STEP REC"
            | "TRACKER"
            | "TRACK EDIT"
            | "TRACK REC"
            | "ORDER"
            | "INSTR"
            | "TABLE"
            | "ARP"
            | "STEP FULL"
            | "SIDS"
            | "UNSAVED"
            | "LIFT"
    )
}

fn scl_waits(lone: &screen_lone) -> bool {
    lone.waiting == SCREEN_LONE_HELD_BACK as u8 && lone.button == PANEL_BTN_SCL as u8
}

fn required(name: &str) -> BTreeSet<(Ctl, bool)> {
    let mut need = BTreeSet::new();
    need.insert((Ctl::Button(PANEL_BTN_HOME), false));
    if saves(name) {
        need.insert((Ctl::Button(PANEL_BTN_SAVE), false));
    }
    for (e, ctl, edit) in probes() {
        let mut c = open_ctx(name);
        if takes(&mut c.s.ui, &e) {
            if scl_waits(&c.s.ui.v.song.gen_.lone) || scl_waits(&c.s.ui.v.lab.lone) {
                continue;
            }
            need.insert((ctl, edit));
        }
    }
    let mut c = open_ctx(name);
    let ui = &mut c.s.ui;
    let top = unsafe { screen_top(&ui.v.s) };
    if unsafe { song_view_is(top) } != 0 {
        need.insert((Ctl::Keys, false));
        need.insert((Ctl::Button(PANEL_BTN_SEQ), false));
        let e = screen_event {
            kind: SCREEN_PRESS as u8,
            id: PANEL_BTN_SEQ as u8,
            steps: 0,
            held: 1 << PANEL_BTN_SEQ | 1 << PANEL_BTN_EDIT,
        };
        if unsafe { song_pages_jump(&mut ui.v.pages, &mut ui.v.s, &e) } != 0 {
            need.insert((Ctl::Button(PANEL_BTN_SEQ), true));
        }
    }
    if name.starts_with("LAB ") && name != "LAB NAME" {
        need.insert((Ctl::Button(PANEL_BTN_REC), false));
        if name != "LAB FX" {
            need.insert((Ctl::Button(PANEL_BTN_REC), true));
        }
    }
    if name == "INSTR" {
        for b in [PANEL_BTN_ENV, PANEL_BTN_LFO, PANEL_BTN_FX] {
            need.insert((Ctl::Button(b), false));
        }
    }
    if ui.top() == "CHIP" && unsafe { chip_view_page(&ui.v.chip) } != PANEL_PAGE_LIVE {
        for k in PANEL_ENC_K1..=PANEL_ENC_K4 {
            need.insert((Ctl::Encoder(k), false));
        }
    }
    need
}

#[test]
fn every_control_a_view_takes_is_on_its_card() {
    let mut missing = Vec::new();
    for name in CONTEXTS {
        let (id, _) = card_over(name);
        let card = unsafe { &help_cards[id as usize] };
        let mut has = BTreeSet::new();
        for l in 0..usize::from(card.count) {
            let (key, _) = line_as_coded(card, l);
            has.extend(controls_of(&key).unwrap_or_default());
        }
        let need = required(name);
        for &(ctl, edit) in &need {
            let ok = has.contains(&(ctl, edit))
                || (edit && need.contains(&(ctl, false)) && has.contains(&(ctl, false)));
            if !ok {
                missing.push(format!(
                    "{name} ({}): {}{ctl:?}",
                    c_text(card.name),
                    if edit { "EDIT + " } else { "" }
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "controls without a line:\n{}",
        missing.join("\n")
    );
}

#[test]
fn the_completeness_check_finds_steps_arp_line_missing_from_its_card() {
    let need = required("STEP");
    assert!(
        need.contains(&(Ctl::Button(PANEL_BTN_ARP), false)),
        "{need:?}"
    );
    assert!(
        need.contains(&(Ctl::Button(PANEL_BTN_LFO), true)),
        "{need:?}"
    );
    assert!(
        !need.contains(&(Ctl::Button(PANEL_BTN_FX), false)),
        "{need:?}"
    );
    let card = unsafe { &help_cards[HELP_CARD_STEP as usize] };
    let without: BTreeSet<(Ctl, bool)> = lines_as_coded(card)
        .into_iter()
        .map(|(key, _)| key)
        .filter(|k| k != "ARP")
        .flat_map(|k| controls_of(&k).unwrap_or_default())
        .collect();
    assert!(!without.contains(&(Ctl::Button(PANEL_BTN_ARP), false)));
}

fn place(ui: &Ui) -> String {
    let m = &ui.v.songs;
    let io = unsafe { &*ui.v.song.io };
    let name = |b: &[std::os::raw::c_char]| {
        b.iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8 as char)
            .collect::<String>()
    };
    let page = if ui.top() == "SONG" {
        ["ROOT", "FILES", "SAVE AS", "CONFIRM"][usize::from(m.page)]
    } else {
        "-"
    };
    let name_is_a_value_algorithm_edits = page == "SAVE AS";
    let chosen = if name_is_a_value_algorithm_edits {
        String::new()
    } else {
        name(&m.name)
    };
    format!(
        "{} {page} depth {} action {} name {chosen:?} io {} {:?}",
        ui.top(),
        ui.v.s.depth,
        m.action,
        io.state,
        name(&io.target)
    )
}

#[test]
fn save_opens_the_highlighted_item_on_every_list_screen_and_home_goes_back() {
    let mut lists = Vec::new();
    let mut wrong = Vec::new();
    let names: Vec<&str> = CONTEXTS.iter().copied().chain(["CONFIRM YES"]).collect();
    for name in names {
        let has_its_own_answers_checked_in_sid_view = name == "UNSAVED";
        if has_its_own_answers_checked_in_sid_view {
            continue;
        }
        let ctx = |n: &str| {
            let mut c = open_ctx(if n == "CONFIRM YES" { "CONFIRM" } else { n });
            if n == "CONFIRM YES" {
                c.s.ui.turn(PANEL_ENC_SELECT, 1);
                c.s.ui.settle();
            }
            c
        };
        let mut c = ctx(name);
        let before = place(&c.s.ui);
        c.s.ui.turn(PANEL_ENC_ALGORITHM, 1);
        c.s.ui.settle();
        let by_alg = place(&c.s.ui);
        let algorithm_opens_no_item = by_alg == before;
        if algorithm_opens_no_item {
            continue;
        }
        lists.push(name);
        let mut c = ctx(name);
        c.s.ui.press(SAVE_KEY_BUTTON);
        c.s.ui.settle();
        let by_save = place(&c.s.ui);
        if by_save != by_alg {
            wrong.push(format!("{name}: ALGORITHM gives {by_alg}, SAVE {by_save}"));
        }
        let mut c = ctx(name);
        c.s.ui.press(PANEL_BTN_HOME);
        c.s.ui.settle();
        let back = place(&c.s.ui);
        let shallower = c.s.ui.v.s.depth < ctx(name).s.ui.v.s.depth
            || (c.s.ui.top() == "SONG" && back != before);
        if !shallower {
            wrong.push(format!("{name}: HOME stays ({back})"));
        }
    }
    let mut c = open_ctx("CONFIRM");
    c.s.ui.press(SAVE_KEY_BUTTON);
    c.s.ui.settle();
    let by_save = place(&c.s.ui);
    let mut c = open_ctx("CONFIRM");
    c.s.ui.press(PANEL_BTN_HOME);
    c.s.ui.settle();
    assert_eq!(by_save, place(&c.s.ui), "CONFIRM: SAVE on NO is HOME");
    for want in ["MENU", "SONG", "FILES", "CONFIRM YES"] {
        assert!(lists.contains(&want), "{want} is a list screen: {lists:?}");
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_song_views_cards_keep_seq_and_home() {
    for id in [HELP_CARD_STEP, HELP_CARD_TRACKER] {
        let card = unsafe { &help_cards[id as usize] };
        let lines = lines_as_coded(card);
        for want in [("SEQ", "NEXT VIEW"), ("HOME", "BACK")] {
            assert!(
                lines.contains(&(want.0.to_owned(), want.1.to_owned())),
                "{}: {lines:?}",
                c_text(card.name)
            );
        }
    }
}

fn after_the_install_boot_without_home(ui: &mut Ui) {
    drain(&mut ui.g);
    unsafe { views_boot(&mut *ui.v, 1, 0) };
    for _ in 0..400 {
        ui.frame_sent();
        if ui.top() == "LIVE" && ui.v.s.wipe == SCREEN_CUT as u8 {
            return;
        }
    }
    panic!("the boot did not end");
}

fn until_toast(ui: &mut Ui, text: &str, ms: u32) {
    let end = ui.now + ms;
    while ui.now < end {
        if ui.toast().as_deref() == Some(text) {
            return;
        }
        ui.frame_sent();
    }
    panic!(
        "no toast {text:?} in {ms} ms; the toast is {:?}",
        ui.toast()
    );
}

#[test]
fn the_tour_follows_the_first_boot_after_an_install() {
    let mut ui = Ui::without_cards();
    after_the_install_boot_without_home(&mut ui);
    until_toast(&mut ui, "TURN K1", 400);
    let start = ui.now;
    ui.turn(PANEL_ENC_K1, 3);
    ui.frame_sent();
    assert_ne!(
        ui.toast().as_deref(),
        Some("TURN K1"),
        "K1 answers the step: its toast goes"
    );
    until_toast(&mut ui, "PRESS ENV", 3000);
    assert!(
        ui.now - start >= 1600,
        "PRESS ENV came after {} ms",
        ui.now - start
    );
    ui.press(PANEL_BTN_ENV);
    let start = ui.now;
    ui.frame_sent();
    assert_eq!(ui.top(), "LAB");
    assert_eq!(
        ui.v.lab.page, LAB_PAGE_ENVELOPE as u8,
        "ENV opens LAB at its ENVELOPE page"
    );
    assert_ne!(ui.toast().as_deref(), Some("PRESS ENV"));
    until_toast(&mut ui, "HOLD HOME FOR HELP", 3000);
    assert!(ui.now - start >= 1600);
    ui.down(PANEL_BTN_HOME);
    ui.frames_sent(20);
    assert_eq!(ui.top(), "HELP");
    assert!(ui.toast().is_none(), "a toast over the card");
    ui.up(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_K1, 1);
    for _ in 0..600 {
        ui.frame_sent();
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
    after_the_install_boot_without_home(&mut ui);
    until_toast(&mut ui, "TURN K1", 400);
    until_toast(&mut ui, "PRESS ENV", 12_000);
    let t0 = ui.now;
    until_toast(&mut ui, "HOLD HOME FOR HELP", 12_000);
    let gap = ui.now - t0;
    assert!(
        (9_600..=9_680).contains(&gap),
        "8 s and 1.6 s, not {gap} ms"
    );
    ui.frames_sent(400);
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
        ui.frame_sent();
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
        ui.frame_sent();
        assert!(ui.toast().is_none(), "a toast at {} ms", ui.now);
    }
    assert_eq!(ui.v.tour, 0);
}

#[test]
fn an_action_during_the_tour_keeps_its_own_toast() {
    let mut ui = Ui::without_cards();
    after_the_install_boot_without_home(&mut ui);
    until_toast(&mut ui, "TURN K1", 400);
    ui.press(PANEL_BTN_HOME);
    ui.frame_sent();
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_OCT_UP);
    ui.frames_sent(3);
    let t = ui.toast().unwrap_or_default();
    assert!(
        !t.is_empty() && t != "TURN K1",
        "OCT+'s toast is gone: {t:?}"
    );
}

#[test]
fn a_toast_waits_under_the_help_card_and_shows_after_it() {
    let mut ui = Ui::without_cards();
    ui.settle();
    hold_until_card(&mut ui);
    unsafe { screen_toast(&mut ui.v.s, c"SONG SAVED".as_ptr(), SCREEN_TOAST_MS) };
    for _ in 0..30 {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(sent.cells <= FRAME_CELL_BUDGET, "{} cells", sent.cells);
        assert!(ui.toast().is_none(), "a toast over the card");
    }
    ui.up(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_K1, 1);
    let mut seen = false;
    for _ in 0..30 {
        ui.frame_sent();
        seen |= ui.toast().as_deref() == Some("SONG SAVED");
    }
    assert_ne!(ui.top(), "HELP");
    assert!(seen, "the toast after the card");
}

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

#[test]
fn home_and_save_in_the_arp_helper_close_it_and_never_leave_the_table() {
    let mut c = open_ctx("ARP");
    let ui = &mut c.s.ui;
    assert_eq!(ui.v.pages.arp, 1);
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_eq!((ui.top().as_str(), ui.v.pages.arp), ("TABLE", 0));
    ui.press(PANEL_BTN_ARP);
    assert_eq!(ui.v.pages.arp, 1);
    ui.press(PANEL_BTN_SAVE);
    assert_eq!((ui.top().as_str(), ui.v.pages.arp), ("TABLE", 0));
    assert_eq!(ui.toast().as_deref(), Some("ARP SAVED"));
}
