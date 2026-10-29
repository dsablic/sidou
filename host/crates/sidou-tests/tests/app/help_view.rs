// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The help card (app/help_view.c) and HOME acting on its release (app/views.c `views_flow`): a
//! tap goes home, a hold of 600 ms opens the card of the view that shows in the state it is in,
//! SELECT turns a card of two pages, any other button or encoder closes it and does nothing else,
//! no frame of it sends more than 200 cells, and every control a view's input takes is on its
//! card; and the first-run tour (app/views.c `tour_frame`) that follows the full boot an install
//! plays.

use std::collections::BTreeSet;
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

/// The title bar: HELP, or the page of a card of two (`1/2`), and the card's name.
fn title(ui: &Ui) -> (String, String) {
    let head = big(ui, 1, 1, 14);
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

fn text(p: *const std::os::raw::c_char) -> String {
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// Line `l` of a card: the control and what it does (one string with a NUL between), the state's
/// words (HELP_TEXT_*) as `h` gives them.
fn line_of(h: &help_view, card: &help_card, l: usize) -> (String, String) {
    let p = unsafe { *card.lines.add(l) };
    let key = unsafe { CStr::from_ptr(p) };
    let mut buf = [0 as std::os::raw::c_char; HELP_TEXT_WORD as usize];
    let what = unsafe { help_view_what(h, p.add(key.to_bytes().len() + 1), buf.as_mut_ptr()) };
    (key.to_string_lossy().into_owned(), text(what))
}

/// Line `l` of a card, its own words (a HELP_TEXT_* code as a control character).
fn line(card: &help_card, l: usize) -> (String, String) {
    let p = unsafe { *card.lines.add(l) };
    let key = unsafe { CStr::from_ptr(p) };
    let what = unsafe { CStr::from_ptr(p.add(key.to_bytes().len() + 1)) };
    (
        key.to_string_lossy().into_owned(),
        what.to_string_lossy().into_owned(),
    )
}

/// HOME let go, then K1 (a card of two pages takes SELECT): the card closes.
fn close(ui: &mut Ui) {
    ui.up(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_K1, 1);
    frame(ui);
    ui.settle();
    assert_ne!(ui.top(), "HELP");
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
    let cutoff = unsafe { live_param_get(&*ui.rig.live, LIVE_PARAM_CUTOFF) };
    ui.turn(PANEL_ENC_K1, 1);
    frame(&mut ui);
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
    for (name, open) in [
        ("MENU", 0),
        ("CHIP", 1),
        ("SCENE", 2),
        ("STEP", 3),
        ("STATUS", 4),
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

/// On a full stack the card has its own slot (screen_push_over): a held HOME shows STEP's card,
/// a control closes it and STEP is back as it was; a tap still goes home.
#[test]
fn a_held_home_on_a_full_stack_shows_the_card_too() {
    let mut ui = full_stack();
    hold_until_card(&mut ui);
    for _ in 0..12 {
        frame(&mut ui);
    }
    assert_eq!(card_name(&ui), "STEP");
    assert_eq!(ui.v.s.depth, SCREEN_SLOTS as u8);
    close(&mut ui);
    assert_eq!(ui.top(), "STEP");
    assert_eq!(ui.v.s.depth, SCREEN_DEPTH as u8);
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_ne!(ui.top(), "STEP", "a tap goes home");
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

/// How a view (or a page) is reached from LIVE for the walk below.
enum Open {
    /// SEQ n times.
    Seq(u32),
    Menu,
    /// The menu's item, ALGORITHM clockwise.
    Item(u32),
    /// The menu's SONG, then its action (ALGORITHM clockwise on it), then `then` turns of
    /// SELECT and of ALGORITHM (a confirmation's YES and its OK).
    Song(u32, i32, i32),
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
            ui.press(PANEL_BTN_HOME);
            ui.settle();
            ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
            ui.turn(PANEL_ENC_SELECT, item as i32);
            frame(ui);
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.settle();
        }
        Open::Song(action, select, alg) => {
            ui.press(PANEL_BTN_HOME);
            ui.settle();
            ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
            ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.settle();
            ui.turn(PANEL_ENC_SELECT, action as i32);
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.turn(PANEL_ENC_SELECT, select);
            ui.turn(PANEL_ENC_ALGORITHM, alg);
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
    frame(&mut ui);
    ui.settle();
    assert_eq!(ui.top(), "MENU");
}

/// Ergonomics review E32: undo keeps one step, and the song views that undo (STEP and TRACKER)
/// say so on their cards.
#[test]
fn the_song_views_cards_say_undo_is_one_step() {
    for id in [HELP_CARD_STEP, HELP_CARD_TRACKER] {
        let card = unsafe { &help_cards[id as usize] };
        let lines: Vec<(String, String)> = (0..usize::from(card.count))
            .map(|l| line(card, l))
            .collect();
        assert!(
            lines.contains(&("E+LFO".to_owned(), "ONE UNDO".to_owned())),
            "{}: {lines:?}",
            text(card.name)
        );
    }
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

/// The longest word a HELP_TEXT_* code gives: a slot's control name, UNUSED, a quantise.
fn longest_word(code: u8) -> usize {
    let mut h = help_view::default();
    let what = [code as std::os::raw::c_char, 0];
    let mut most = 0;
    for arg in 0..=0x0fffu16 {
        h.arg = arg;
        let mut buf = [0 as std::os::raw::c_char; HELP_TEXT_WORD as usize];
        let w = text(unsafe { help_view_what(&h, what.as_ptr(), buf.as_mut_ptr()) });
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
        // HELP (four) or the page (three) and a space, then the name, in 14 characters.
        let name = text(card.name);
        let room = if n > HELP_LINES { 10 } else { 9 };
        assert!(
            !name.is_empty() && name.len() <= room,
            "card {c}: {name:?} in {room}"
        );
        for l in 0..card.count as usize {
            let (key, what) = line(card, l);
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

/// The page shown: its lines on the grid, a line every three rows from row 4.
fn assert_page_drawn(ui: &Ui, what: &str) {
    let h = &ui.v.help;
    let card = unsafe { &help_cards[h.card as usize] };
    let first = usize::from(h.page) * HELP_LINES as usize;
    for i in 0..HELP_LINES as usize {
        let got = big(ui, 4 + 3 * i, 1, 14);
        if first + i >= card.count as usize {
            assert_eq!(got.trim(), "", "{what}: line {i} past the card's");
            continue;
        }
        let (key, what_does) = line_of(h, card, first + i);
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
            assert!(sent.cells <= 200, "open {open}: {} cells", sent.cells);
            assert!(sent.spi_ms() <= SPI_BUDGET_MS);
            shown |= ui.top() == "HELP";
        }
        assert!(shown);
        assert_page_drawn(&ui, &format!("open {open}"));
        // Nothing left to send once it is drawn.
        for _ in 0..3 {
            ui.frame();
            assert_eq!(drain(&mut ui.g).cells, 0);
        }
        ui.up(PANEL_BTN_HOME);
        ui.turn(PANEL_ENC_K1, 1);
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

/// A card of two pages (LIVE's) says 1/2; SELECT turns it to 2/2 and back, either way, each
/// frame of a turn within 200 cells and the page whole after it, and does nothing else (the knob
/// page stays TONE); K1 closes it. A card of one page (the menu's) says HELP and SELECT closes it.
#[test]
fn select_turns_a_card_of_two_pages_and_closes_one_of_one() {
    let mut ui = Ui::without_cards();
    ui.settle();
    hold_until_card(&mut ui);
    for _ in 0..12 {
        frame(&mut ui);
    }
    assert!(unsafe { help_cards[HELP_CARD_LIVE as usize].count } as u32 > HELP_LINES);
    assert_eq!(title(&ui), ("1/2".to_string(), "LIVE".to_string()));
    assert_page_drawn(&ui, "page 1");
    ui.up(PANEL_BTN_HOME);
    for (turn, page) in [(1, "2/2"), (1, "1/2"), (-1, "2/2")] {
        ui.turn(PANEL_ENC_SELECT, turn);
        for _ in 0..12 {
            ui.frame();
            let sent = drain(&mut ui.g);
            assert!(sent.cells <= 200, "{} cells", sent.cells);
        }
        assert_eq!(ui.top(), "HELP", "SELECT closed the card");
        assert_eq!(title(&ui).0, page);
        assert_page_drawn(&ui, page);
    }
    assert_eq!(ui.v.live.page, 0, "SELECT paged LIVE under the card");
    close(&mut ui);
    assert_eq!(ui.top(), "LIVE");
    // One page: HELP, and SELECT closes it.
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    hold_until_card(&mut ui);
    for _ in 0..12 {
        frame(&mut ui);
    }
    assert_eq!(title(&ui), ("HELP".to_string(), "MENU".to_string()));
    ui.up(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, 1);
    frame(&mut ui);
    ui.settle();
    assert_eq!(ui.top(), "MENU");
}

// ---- every control on the card: the views' input handlers fed every button and encoder ----

/// A control of the panel as the cards' lines name it: a button, an encoder or the note keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ctl {
    Button(u32),
    Encoder(u32),
    Keys,
}

/// The controls (and whether EDIT is held) a card line's control words name: SELECT, ALG,
/// K1..K4 (K1-K4, K1 K2, K3/K4; K the four), PRESETS (PRE), KEYS..., the buttons; E+ or EDIT+
/// with EDIT held. Another modifier (STEP+, GLO+, SAVE+, ARP+) names a gesture no probe makes:
/// nothing.
/// None for a line that names no control but tells something (SETTINGS KEPT, MACROS RESET,
/// SPREAD A/B L/R).
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

/// A view in a state, opened from power-on with a song bound (the device's), and the card its
/// held HOME opens.
struct Ctx {
    s: crate::song_view::Song,
    _io: Box<song_io>,
    _tune: Option<(Box<crate::sid_view::Tune>, crate::sid_view::Bound)>,
}

/// The views and states with a card of their own (and the walk to each).
const CONTEXTS: [&str; 41] = [
    "LIVE",
    "LAB OSC",
    "LAB ENV",
    "LAB FILTER",
    "LAB MOD",
    "LAB FX",
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
    // The same cards reached another way: STEP on a full stack, CHIP's slots MOD and FX.
    "STEP FULL",
    "CHIP MOD",
    "CHIP FX",
];

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
    // The built-in song the firmware has (SIDOU LOOP): LOAD lists it first.
    io.hooks.builtin = Some(no_bytes);
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
        "LAB OSC" | "LAB ENV" | "LAB FILTER" | "LAB MOD" | "LAB FX" | "LAB NAME" => {
            menu_item(ui, VIEWS_MENU_LAB);
            let page = ["LAB OSC", "LAB ENV", "LAB FILTER", "LAB MOD", "LAB FX"]
                .iter()
                .position(|n| *n == name);
            crate::lab_view::to_page(ui, page.unwrap_or(0) as u32);
            if page.is_none() {
                ui.down(PANEL_BTN_EDIT); // EDIT + SAVE: the name page
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
        "FILES" => open(ui, &Open::Song(0, 0, 0)),
        "SAVE AS" => open(ui, &Open::Song(2, 0, 0)),
        "CONFIRM" => open(ui, &Open::Song(1, 0, 0)),
        "CHIP TABS" | "CHIP" | "CHIP SLOT" | "EXPERT" | "CHIP MOD" | "CHIP FX" => {
            menu_item(ui, VIEWS_MENU_CHIP);
            // From the tabs: the chip, the slots, EXPERT.
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
            // The SID views over the fake player, a store of two tunes; the song unsaved.
            let bound = crate::sid_view::bind(ui, &["ALPHA", "BETA"]);
            crate::sid_view::open_sids(ui);
            if name != "SIDS" {
                ui.turn(PANEL_ENC_ALGORITHM, 1); // the question
            }
            if name == "PLAYING" {
                ui.turn(PANEL_ENC_ALGORITHM, 1); // discarded: NOW PLAYING
            }
            ui.settle();
            tune = Some(bound);
        }
        "STEP FULL" => {
            ui.press(PANEL_BTN_HOME);
            ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SONG as i32);
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.press(PANEL_BTN_SEQ);
            ui.settle();
            assert_eq!(ui.v.s.depth, SCREEN_DEPTH as u8, "the stack is full");
        }
        _ => panic!("no context {name}"),
    }
    Ctx {
        s,
        _io: io,
        _tune: tune,
    }
}

/// A built-in song of no bytes: the walk never reads it (the song is unsaved, so LOAD asks first).
unsafe extern "C" fn no_bytes(_: *mut std::ffi::c_void, _: u32, _: *mut u8, _: u32) -> u32 {
    0
}

/// The card a held HOME opens over the context (the context left as it was): its id and its
/// state's words.
fn card_over(name: &str) -> (u32, u16) {
    let mut c = open_ctx(name);
    let ui = &mut c.s.ui;
    let under = (ui.top(), ui.v.s.depth);
    hold_until_card(ui);
    for _ in 0..12 {
        frame(ui);
    }
    let id = (u32::from(ui.v.help.card), ui.v.help.arg);
    let want = match name {
        "STEP FULL" => "STEP",
        "CHIP MOD" | "CHIP FX" => "CHIP SLOT",
        _ => name,
    };
    assert_eq!(card_name(ui), want, "over {under:?}");
    assert_page_drawn(ui, name);
    close(ui);
    assert_eq!(
        (ui.top(), ui.v.s.depth),
        under,
        "{name}: the view as it was"
    );
    id
}

/// Every view and page the panel reaches, in every state with a card of its own, holds HOME to
/// that card: the walk opens each (a fresh power-on each time), holds HOME and reads the card's
/// name and lines; every card is some state's.
#[test]
fn every_view_and_page_holds_home_to_its_own_card() {
    let mut seen = [false; HELP_CARDS as usize];
    for name in CONTEXTS {
        let (id, _) = card_over(name);
        seen[id as usize] = true;
    }
    assert!(seen.iter().all(|&s| s), "a card no view opens: {seen:?}");
}

/// LAB's cards, each a golden: the pages (two pages each) and the name page. A page's card names
/// the control of its sound as the page has it.
#[test]
fn lab_help_cards_match_their_goldens() {
    for (name, golden_name, pages) in [
        ("LAB OSC", "help-lab-osc", 2),
        ("LAB ENV", "help-lab-envelope", 2),
        ("LAB FILTER", "help-lab-filter", 2),
        ("LAB MOD", "help-lab-mod", 2),
        ("LAB FX", "help-lab-fx", 2),
        ("LAB NAME", "help-lab-name", 1),
    ] {
        let mut c = open_ctx(name);
        let ui = &mut c.s.ui;
        if name == "LAB FX" {
            ui.turn(PANEL_ENC_ALGORITHM, 2); // OVERDRIVE: its own names
            ui.settle();
        }
        hold_until_card(ui);
        for _ in 0..12 {
            frame(ui);
        }
        assert_eq!(card_name(ui), name);
        assert_page_drawn(ui, name);
        crate::sid_view::golden(ui, golden_name);
        if pages == 2 {
            ui.turn(PANEL_ENC_SELECT, 1);
            for _ in 0..12 {
                frame(ui);
            }
            assert_eq!(ui.top(), "HELP", "SELECT turned the card");
            assert_page_drawn(ui, name);
            crate::sid_view::golden(ui, &format!("{golden_name}-2"));
        }
        close(ui);
    }
}

/// The state's words: CHIP's slot card names its algorithm's controls (OVERDRIVE: DRIVE, TONE,
/// BIAS, MIX; OFF: all four UNUSED), LIVE's slot page the same, and TRACKER's recording card the
/// quantise SETTINGS chose.
#[test]
fn a_slot_card_names_its_algorithms_controls_and_rec_its_quantise() {
    let words = |name: &str, setup: &dyn Fn(&mut Ui)| {
        let mut c = open_ctx(name);
        setup(&mut c.s.ui);
        let ui = &mut c.s.ui;
        hold_until_card(ui);
        for _ in 0..12 {
            frame(ui);
        }
        let h = ui.v.help;
        let card = unsafe { &help_cards[h.card as usize] };
        let lines: Vec<(String, String)> = (0..usize::from(card.count))
            .map(|l| line_of(&h, card, l))
            .collect();
        close(ui);
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
        // ALGORITHM steps DIST to TAPE, then OVERDRIVE.
        let od = words(name, &|ui| {
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.settle();
        });
        let got: Vec<String> = ["K1", "K2", "K3", "K4"]
            .iter()
            .map(|k| knob(&od, k))
            .collect();
        assert_eq!(got, ["DRIVE", "TONE", "BIAS", "MIX"], "{name}");
    }
    let mut c = open_ctx("TRACK REC");
    c.s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    let ui = &mut c.s.ui;
    hold_until_card(ui);
    for _ in 0..12 {
        frame(ui);
    }
    let h = ui.v.help;
    let card = unsafe { &help_cards[h.card as usize] };
    assert!((0..usize::from(card.count))
        .map(|l| line_of(&h, card, l))
        .any(|l| l == ("QUANT".to_string(), "COUNT-IN".to_string())));
}

/// The buttons and encoders a probe feeds a view: every button but those the global handler
/// takes first (HOME, SEQ, PLAY, REC; SAVE is save_key's), every encoder both ways; each alone
/// and with EDIT held.
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

/// Whether the view on top, in its state, takes the event: its own input, or for a turn the
/// screen's global handler (SONG's pages take their encoders there; the views' global handler
/// takes no turn).
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

/// What SAVE does on its own in a state (app/views.c save(), mirrored): a USER slot on CHIP's chip
/// and EXPERT, the keep in LAB, the song in the song views, the menu's and SONG's pages' OK, SAVE AS's name,
/// STORE's format and RESET.
fn saves(name: &str) -> bool {
    matches!(
        name,
        "CHIP"
            | "LAB OSC"
            | "LAB ENV"
            | "LAB FILTER"
            | "LAB MOD"
            | "LAB FX"
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
    )
}

/// The controls a state acts on, from its handlers: each probe on a fresh power-on in that state
/// (an event changes the state), the view's input (STEP's SCL alone aside: it waits there for
/// EDIT, and is SCL's next slot as everywhere); then what acts outside it: HOME everywhere,
/// SAVE where save() has a meaning of its own, the keys and SEQ (the next view; EDIT + SEQ where
/// song_pages_jump takes it) in a song view, K1..K4 where the view gives the panel a page of its
/// own (chip_view_page: the chip's character, a slot's controls).
fn required(name: &str) -> BTreeSet<(Ctl, bool)> {
    let mut need = BTreeSet::new();
    need.insert((Ctl::Button(PANEL_BTN_HOME), false));
    if saves(name) {
        need.insert((Ctl::Button(PANEL_BTN_SAVE), false));
    }
    for (e, ctl, edit) in probes() {
        let mut c = open_ctx(name);
        if takes(&mut c.s.ui, &e) {
            // STEP holds SCL alone only to wait for EDIT (EDIT + SCL, whichever lands first):
            // it is still SCL's next slot, everywhere's, at its release.
            if c.s.ui.v.step.scl_down == STEP_VIEW_SCL_ALONE as u8 {
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
    if ui.top() == "CHIP" && unsafe { chip_view_page(&ui.v.chip) } != PANEL_PAGE_LIVE {
        for k in PANEL_ENC_K1..=PANEL_ENC_K4 {
            need.insert((Ctl::Encoder(k), false));
        }
    }
    need
}

/// Every control a view's input handler acts on, in each state with a card of its own, has a
/// line on that card: a control the view takes without its line fails here. A line with EDIT
/// (E+X) names EDIT + X; X alone covers EDIT + X too where the view takes X as well.
#[test]
fn every_control_a_view_takes_is_on_its_card() {
    let mut missing = Vec::new();
    for name in CONTEXTS {
        let (id, _) = card_over(name);
        let card = unsafe { &help_cards[id as usize] };
        let mut has = BTreeSet::new();
        for l in 0..usize::from(card.count) {
            let (key, _) = line(card, l);
            has.extend(controls_of(&key).unwrap_or_default());
        }
        let need = required(name);
        for &(ctl, edit) in &need {
            let ok = has.contains(&(ctl, edit))
                || (edit && need.contains(&(ctl, false)) && has.contains(&(ctl, false)));
            if !ok {
                missing.push(format!(
                    "{name} ({}): {}{ctl:?}",
                    text(card.name),
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

/// The probe sees what it should: a control a view takes and its card left out fails. STEP takes
/// ARP (the lane's gate): a card without ARP's line misses it.
#[test]
fn the_completeness_check_finds_a_missing_line() {
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
    let without: BTreeSet<(Ctl, bool)> = (0..usize::from(card.count))
        .map(|l| line(card, l).0)
        .filter(|k| k != "ARP")
        .flat_map(|k| controls_of(&k).unwrap_or_default())
        .collect();
    assert!(!without.contains(&(Ctl::Button(PANEL_BTN_ARP), false)));
}

/// Where the panel is: the view on top, SONG's page, the stack's depth and what a file action
/// would act on (the page's action and name, the song file's state and target).
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
    // SAVE AS's name is a value ALGORITHM edits, not an item it opens.
    let chosen = if page == "SAVE AS" {
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

/// The panel's OK on every list screen: wherever ALGORITHM clockwise opens or chooses the
/// highlighted item (the menu's views, SONG's actions, LOAD's and DELETE's files, a confirmation's
/// YES), a clean SAVE does the same; on a confirmation's NO, SAVE chooses NO (back a page, as
/// HOME). And HOME goes back from each of them. The walk is every state with a card (CONTEXTS):
/// a new list screen whose ALGORITHM opens something is checked without being named here.
#[test]
fn save_opens_the_highlighted_item_on_every_list_screen_and_home_goes_back() {
    let mut lists = Vec::new();
    let mut wrong = Vec::new();
    let names: Vec<&str> = CONTEXTS.iter().copied().chain(["CONFIRM YES"]).collect();
    for name in names {
        if name == "UNSAVED" {
            // The question's own answers (https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-19-m3-sid-files.md, "Memory"): SAVE
            // saves the song first, ALGORITHM again discards it; sid_view.rs checks both.
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
        if by_alg == before {
            continue; // ALGORITHM changes a value here (or nothing): no item to open
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
    // A confirmation's NO: SAVE goes back a page, as HOME does, and does nothing else.
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

/// Lines dropped for room before two pages: STEP's SEQ NEXT VIEW and TRACKER's HOME BACK.
#[test]
fn the_song_views_cards_keep_seq_and_home() {
    for id in [HELP_CARD_STEP, HELP_CARD_TRACKER] {
        let card = unsafe { &help_cards[id as usize] };
        let lines: Vec<(String, String)> = (0..usize::from(card.count))
            .map(|l| line(card, l))
            .collect();
        for want in [("SEQ", "NEXT VIEW"), ("HOME", "BACK")] {
            assert!(
                lines.contains(&(want.0.to_owned(), want.1.to_owned())),
                "{}: {lines:?}",
                text(card.name)
            );
        }
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
    // ENV opens LAB at its ENVELOPE page: the toast goes, then the last step 1.6 s later (the
    // help card it asks for is LAB's).
    ui.press(PANEL_BTN_ENV);
    let start = ui.now;
    frame(&mut ui);
    assert_eq!(ui.top(), "LAB");
    assert_eq!(ui.v.lab.page, LAB_PAGE_ENVELOPE as u8);
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
    ui.turn(PANEL_ENC_K1, 1);
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

/// Ergonomics review E30 (M2 review 5.13): OCT+ pressed in the menu while the tour's first step
/// shows: the tour ends its own toast only, and OCT+'s (the octave) shows.
#[test]
fn an_action_during_the_tour_keeps_its_own_toast() {
    let mut ui = Ui::without_cards();
    after_the_install_boot(&mut ui, 0);
    until_toast(&mut ui, "TURN K1", 400);
    ui.press(PANEL_BTN_HOME);
    frame(&mut ui);
    assert_eq!(ui.top(), "MENU");
    ui.press(PANEL_BTN_OCT_UP);
    for _ in 0..3 {
        frame(&mut ui);
    }
    let t = ui.toast().unwrap_or_default();
    assert!(
        !t.is_empty() && t != "TURN K1",
        "OCT+'s toast is gone: {t:?}"
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
    ui.turn(PANEL_ENC_K1, 1);
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

/// HOME in TABLE's ARP helper closes the helper (the table stays); SAVE writes the rows, not the
/// song, and closes it; held HOME shows the helper's card.
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
