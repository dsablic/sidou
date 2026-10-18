// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The CHIP view (app/chip_view.c) over the views (app/views.c): opened from the menu behind
//! THE CHIP's card, its page, the focus (SELECT), the profile picker and the slots' algorithms
//! (ALGORITHM), K1..K4 on the focused section through the view-aware panel_knob_param (which
//! MIDI learn and MIDI out follow), the EDIT shortcut kept in every view, MIDI's changes on
//! the page, and its budget.

use std::ffi::CStr;

use sidou_sys::*;

use crate::views::Ui;
use crate::{drain, Sent, SPI_BUDGET_MS};

const COLS: usize = GRID_COLS as usize;
const K1: u32 = PANEL_ENC_K1;
/// docs/reference/midi.md: the character's CCs (profile, age, DAC, thump, bleed).
const CC_PROFILE: u32 = 85;
const CC_AGE: u32 = 86;
const CC_DAC: u32 = 87;
const CC_THUMP: u32 = 89;
const CC_BLEED: u32 = 90;

/// A frame, and what it sent, as the panel takes it.
fn step(ui: &mut Ui) -> Sent {
    ui.frame();
    drain(&mut ui.g)
}

/// Frames, each sent, until no transition runs and the page is whole on the grid.
fn settle(ui: &mut Ui) {
    for _ in 0..200 {
        step(ui);
        let card = ui.top() == "TITLE";
        if !card
            && unsafe { screen_animating(&ui.v.s) } == 0
            && unsafe { screen_popup_shows(&ui.v.s) } == 0
            && ui.v.chip.copying == 0
        {
            step(ui);
            return;
        }
    }
    panic!("the screen kept changing");
}

/// The menu, then CHIP (the menu's first item, ALGORITHM clockwise), settled: behind THE CHIP's
/// card with the cards on.
fn open_chip(ui: &mut Ui) {
    ui.press(PANEL_BTN_HOME);
    settle(ui);
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CHIP as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(ui);
    assert_eq!(ui.top(), "CHIP");
}

fn chip_ui() -> Ui {
    let mut ui = Ui::without_cards();
    open_chip(&mut ui);
    ui
}

fn focus(ui: &Ui) -> u32 {
    unsafe { chip_view_focus(&ui.v.chip) }
}

fn param(ui: &Ui, p: u32) -> u32 {
    unsafe { live_param_get(&*ui.rig.live, p) }
}

/// The knobs' columns on the page: where each control's label, value and bar start, and its width.
const KNOB_COL: [usize; 4] = [0, 8, 15, 22];
const KNOB_W: [usize; 4] = [8, 7, 7, 8];

fn big(ui: &Ui, row: u32, col: usize, scale: usize, n: usize) -> String {
    ui.big(row as usize, col, scale, n)
}

fn small(ui: &Ui, row: u32, col: usize, n: usize) -> String {
    ui.small(row as usize, col, n)
}

/// The theme the page is drawn in.
fn theme_of(ui: &Ui) -> &'static theme {
    unsafe { &*theme_get(u32::from(ui.live().theme)) }
}

/// Control k's (0..4) label and value as the page shows them: the label small, the value at 3x
/// (rows 10-12) or, from 100, at 2x (rows 11-12).
fn control(ui: &Ui, k: usize) -> (String, String) {
    let col = KNOB_COL[k];
    let label = small(ui, CHIP_VIEW_LABEL_ROW, col, KNOB_W[k] - 1)
        .trim_end()
        .to_owned();
    let mut value = String::new();
    for i in 0..3 {
        let at = CHIP_VIEW_VALUE_ROW as usize * COLS + col + 3 * i;
        if u32::from(ui.g.ch[at]) == GRID_BIG_FIRST + 4 {
            value.push(ui.g.src[at] as char);
        }
    }
    if value.is_empty() {
        for i in 0..3 {
            let at = (CHIP_VIEW_VALUE_ROW as usize + 1) * COLS + col + 2 * i;
            if u32::from(ui.g.ch[at]) == GRID_BIG_FIRST {
                value.push(ui.g.src[at] as char);
            }
        }
    }
    (label, value)
}

/// The choice at 3x: its first word (the model, or the slot's algorithm) and the second (the
/// profile), blank for a slot.
fn choice(ui: &Ui, n: usize) -> String {
    big(ui, CHIP_VIEW_CHOICE_ROW, 0, 3, n)
}

fn profile_word(ui: &Ui, n: usize) -> String {
    big(ui, CHIP_VIEW_CHOICE_ROW, 13, 3, n)
}

/// The profile's short word as the page says it: the name without the model, "R4AR", "WORN".
fn word_of(id: u32) -> String {
    let name = profile_name(id);
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.split_once(' ').unwrap().1.to_owned()
    } else {
        name.split(' ').next().unwrap().to_owned()
    }
}

/// The 2x line of the other sections: the row of the `line`th of them, its name and its value.
fn summary(ui: &Ui, line: u32, name_len: usize, value_len: usize) -> (String, String) {
    let row = CHIP_VIEW_SUMMARY_ROW + 2 * line;
    (big(ui, row, 0, 2, name_len), big(ui, row, 9, 2, value_len))
}

/// Which tab of the row has what background: the focused one yellow, the others dark grey.
fn tab_bg(ui: &Ui, section: usize) -> u8 {
    let cols = [0usize, 8, 15, 22];
    ui.g.attr[CHIP_VIEW_TAB_ROW as usize * COLS + cols[section]] >> 4
}

/// A slot field's name for an algorithm, and whether the algorithm uses it.
fn label(slot: u32, alg: u32, field: u32) -> (String, bool) {
    let mut buf = [0 as std::ffi::c_char; CHIP_VIEW_LABEL as usize];
    let used = unsafe { chip_view_label(slot, alg, field, buf.as_mut_ptr()) } != 0;
    let s = unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    (s, used)
}

fn profile_name(id: u32) -> String {
    let p = unsafe { &*sid_chip_profile_get(id) };
    unsafe { CStr::from_ptr(p.name) }
        .to_string_lossy()
        .to_uppercase()
}

#[test]
fn chip_opens_from_the_menu_behind_the_chip_card() {
    let mut ui = Ui::new();
    ui.settle();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    // The menu's first item opens the view: an arrow, no value.
    assert_eq!(&ui.menu_line(0)[..4], "CHIP");
    assert_eq!(ui.g.src[4 * COLS + 28], FONT_ARROW_RIGHT as u8);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.top(), "TITLE");
    assert_eq!(u32::from(ui.v.card.id), TITLE_CARD_CHIP);
    settle(&mut ui);
    assert_eq!(ui.top(), "CHIP");
    // HOME back to the menu, and again to LIVE.
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
    // Counter-clockwise on an action does nothing.
    ui.settle();
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(ui.top(), "MENU");
}

#[test]
fn the_page_shows_the_profile_the_character_and_the_other_sections() {
    let ui = chip_ui();
    let t = theme_of(&ui);
    // The title at 2x says which chip's page, in white while the tabs have no focus.
    assert_eq!(big(&ui, 0, 0, 2, 6), "CHIP A");
    assert_eq!(ui.g.attr[0] & 15, t.value);
    assert_eq!(u32::from(ui.g.attr[0] >> 4), GRID_BLACK);
    // The section tabs across the width: CHIP focused (yellow), the others dark grey.
    for (i, (name, col)) in [("CHIP", 2), ("DIST", 9), ("MOD", 17), ("FX", 25)]
        .into_iter()
        .enumerate()
    {
        assert_eq!(small(&ui, CHIP_VIEW_TAB_ROW, col, name.len()), name);
        assert_eq!(
            tab_bg(&ui, i),
            if i == 0 { t.hot } else { t.dim },
            "tab {name}"
        );
    }
    // The choice at 3x: the model and the profile; their names under it.
    assert_eq!(choice(&ui, 4), "6581");
    assert_eq!(profile_word(&ui, 4), "R4AR");
    assert_eq!(
        ui.g.attr[CHIP_VIEW_CHOICE_ROW as usize * COLS] & 15,
        t.value
    );
    assert_eq!(
        ui.g.attr[CHIP_VIEW_CHOICE_ROW as usize * COLS + 13] & 15,
        t.hot
    );
    assert_eq!(small(&ui, CHIP_VIEW_NAMES_ROW, 0, 5), "MODEL");
    assert_eq!(small(&ui, CHIP_VIEW_NAMES_ROW, 13, 7), "PROFILE");
    // The character in the knobs' places: label in the knob's colour, value, bar, name.
    for (k, name) in ["AGE", "DAC", "THUMP", "BLEED"].iter().enumerate() {
        assert_eq!(control(&ui, k), (name.to_string(), "0".into()));
        let col = KNOB_COL[k];
        assert_eq!(
            ui.g.attr[CHIP_VIEW_LABEL_ROW as usize * COLS + col] & 15,
            t.knob[k],
            "{name}'s colour"
        );
        assert_eq!(
            ui.g.attr[CHIP_VIEW_BAR_ROW as usize * COLS + col] & 15,
            t.knob[k]
        );
        assert_eq!(
            ui.g.attr[CHIP_VIEW_VALUE_ROW as usize * COLS + col] & 15,
            t.value
        );
        assert_eq!(small(&ui, CHIP_VIEW_TAG_ROW, col, 2), format!("K{}", k + 1));
    }
    // The other sections, one line each at 2x: DIST, MOD and FX.
    for (line, (name, value)) in [("DIST", "OFF"), ("MOD", "OFF"), ("FX", "OFF")]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            summary(&ui, line as u32, name.len(), value.len()),
            (name.to_string(), value.to_string())
        );
        assert_eq!(
            ui.g.attr[(CHIP_VIEW_SUMMARY_ROW as usize + 2 * line) * COLS + 9] & 15,
            t.dim,
            "OFF is dark grey"
        );
    }
    assert_eq!(
        small(&ui, CHIP_VIEW_HELP_ROW, 0, 26),
        "SELECT MOVE ALGORITHM STEP"
    );
    assert_eq!(focus(&ui), CHIP_VIEW_CHIP);
}

#[test]
fn every_algorithm_names_its_controls() {
    // core/fx/fx_slots.h: what Amount, A and B do per algorithm; Mix is always the mix.
    let want: [&[[&str; 3]]; 3] = [
        &[
            ["AMT", "A", "B"],
            ["DRIVE", "A", "B"],
            ["DRIVE", "TONE", "BIAS"],
            ["DRIVE", "OCT", "BIAS"],
            ["BITS", "RATE", "B"],
            ["TONE", "HUM", "NOISE"],
        ],
        &[
            ["AMT", "A", "B"],
            ["DEPTH", "RATE", "ENS"],
            ["DEPTH", "RATE", "FDBK"],
            ["DEPTH", "RATE", "FDBK"],
            ["DEPTH", "RATE", "SHAPE"],
            ["WIDTH", "RATE", "SHAPE"],
            ["WOW", "FLUT", "B"],
        ],
        &[
            ["AMT", "A", "B"],
            ["FDBK", "TIME", "DAMP"],
            ["FDBK", "TIME", "DAMP"],
            ["DECAY", "DAMP", "DIFF"],
            ["DECAY", "DAMP", "BAND"],
        ],
    ];
    for slot in 0..LIVE_SLOTS {
        let algs = unsafe { live_slot_algorithms(slot) };
        assert_eq!(algs as usize, want[slot as usize].len());
        for alg in 0..algs {
            for (k, &name) in want[slot as usize][alg as usize].iter().enumerate() {
                let field = LIVE_SLOT_AMOUNT + k as u32;
                // The generic names are the unused fields' (and an off slot's).
                let generic = ["AMT", "A", "B"][k] == name;
                assert_eq!(
                    label(slot, alg, field),
                    (name.to_string(), !generic),
                    "slot {slot} algorithm {alg} field {field}"
                );
            }
            assert_eq!(label(slot, alg, LIVE_SLOT_MIX), ("MIX".into(), alg != 0));
        }
    }
}

#[test]
fn select_moves_the_focus_and_selects_the_slot() {
    let mut ui = chip_ui();
    assert_eq!(focus(&ui), CHIP_VIEW_CHIP);
    for want in 1..CHIP_VIEW_SECTIONS {
        ui.turn(PANEL_ENC_SELECT, 1);
        assert_eq!(focus(&ui), want);
        assert_eq!(u32::from(ui.live().fx_select), want - 1, "the slot follows");
    }
    // Clamped at both ends.
    ui.turn(PANEL_ENC_SELECT, 5);
    assert_eq!(focus(&ui), CHIP_VIEW_SECTIONS - 1);
    ui.turn(PANEL_ENC_SELECT, -9);
    assert_eq!(
        focus(&ui),
        CHIP_VIEW_TABS,
        "above the chip, the tabs (M2b Task 5)"
    );
    ui.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(focus(&ui), CHIP_VIEW_CHIP);
    // On the chip, the selected slot stays as it was.
    assert_eq!(ui.live().fx_select, 2);
    // SCL on a slot moves the focus with the slot; on the chip only the slot.
    ui.press(PANEL_BTN_SCL);
    assert_eq!((focus(&ui), ui.live().fx_select), (CHIP_VIEW_CHIP, 0));
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.press(PANEL_BTN_SCL);
    assert_eq!((focus(&ui), ui.live().fx_select), (2, 1));
    // The page shows the focus: the MOD tab is yellow and its section the page, the others
    // are the lines below.
    settle(&mut ui);
    let t = theme_of(&ui);
    assert_eq!(tab_bg(&ui, 2), t.hot);
    assert_eq!(tab_bg(&ui, 0), t.dim);
    assert_eq!(small(&ui, CHIP_VIEW_NAMES_ROW, 0, 9), "ALGORITHM");
    assert_eq!(
        summary(&ui, 0, 4, 9),
        ("CHIP".into(), "6581 R4AR".into()),
        "the chip's line among the others"
    );
    // The focus stays between visits.
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut ui);
    assert_eq!(focus(&ui), 2);
}

#[test]
fn algorithm_steps_the_profile_and_the_model_follows() {
    let mut ui = chip_ui();
    let profile = |ui: &Ui| param(ui, LIVE_PARAM_PROFILE);
    assert_eq!(profile(&ui), SID_CHIP_6581_R4AR);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(profile(&ui), SID_CHIP_8580_R5);
    // The chip changes at the next tick (a reset), and the page shows both.
    ui.rig.tick();
    assert_eq!(ui.live().model, SID_MODEL_8580);
    settle(&mut ui);
    assert_eq!(choice(&ui, 4), "8580");
    assert_eq!(profile_word(&ui, 2), "R5");
    // Clamped at both ends; every profile in order on the way.
    ui.turn(PANEL_ENC_ALGORITHM, 9);
    assert_eq!(profile(&ui), SID_CHIPS - 1);
    for id in (0..SID_CHIPS).rev() {
        ui.rig.tick();
        settle(&mut ui);
        assert_eq!(profile(&ui), id);
        let word = word_of(id);
        assert_eq!(profile_word(&ui, word.len()), word, "profile {id}");
        ui.turn(PANEL_ENC_ALGORITHM, -1);
    }
    assert_eq!(profile(&ui), 0);
    ui.turn(PANEL_ENC_ALGORITHM, -3);
    assert_eq!(profile(&ui), SID_CHIP_6581_R2);
}

#[test]
fn algorithm_steps_the_focused_slots_algorithm() {
    let mut ui = chip_ui();
    for slot in 0..LIVE_SLOTS {
        ui.turn(PANEL_ENC_SELECT, 1);
        let alg = |ui: &Ui| ui.live().slot[slot as usize][LIVE_SLOT_ALGORITHM as usize];
        let last = unsafe { live_slot_algorithms(slot) } as u8 - 1;
        ui.turn(PANEL_ENC_ALGORITHM, 1);
        assert_eq!(alg(&ui), 1);
        settle(&mut ui);
        let names = ["TAPE", "CHORUS", "DELAY"];
        let name = names[slot as usize];
        assert_eq!(choice(&ui, name.len()), name);
        ui.turn(PANEL_ENC_ALGORITHM, 99);
        assert_eq!(alg(&ui), last, "clamped at the last");
        ui.turn(PANEL_ENC_ALGORITHM, -99);
        assert_eq!(alg(&ui), 0, "clamped at OFF");
        ui.turn(PANEL_ENC_ALGORITHM, 2);
    }
    // The other slots kept theirs; nothing moved the profile.
    for slot in 0..LIVE_SLOTS as usize {
        assert_eq!(ui.live().slot[slot][LIVE_SLOT_ALGORITHM as usize], 2);
    }
    assert_eq!(param(&ui, LIVE_PARAM_PROFILE), SID_CHIP_6581_R4AR);
    settle(&mut ui);
    ui.turn(PANEL_ENC_SELECT, -2);
    settle(&mut ui);
    assert_eq!(choice(&ui, 9), "OVERDRIVE");
    assert_eq!(control(&ui, 1).0, "TONE");
}

#[test]
fn the_knobs_turn_the_character_on_the_chip() {
    let mut ui = chip_ui();
    let params = [
        LIVE_PARAM_AGE,
        LIVE_PARAM_DAC,
        LIVE_PARAM_THUMP,
        LIVE_PARAM_BLEED,
    ];
    let cutoff = ui.live().cutoff;
    let preset = ui.live().preset;
    for (k, &p) in params.iter().enumerate() {
        ui.turn(K1 + k as u32, 3 + k as i32);
        assert_eq!(param(&ui, p), 4 * (3 + k as u32), "K{}", k + 1);
    }
    assert_eq!(
        (ui.live().cutoff, ui.live().preset),
        (cutoff, preset),
        "K1..K4 also turned LIVE's knobs"
    );
    settle(&mut ui);
    assert_eq!(control(&ui, 0).1, "12");
    assert_eq!(control(&ui, 3).1, "24");
    // The bar: K1's column is 7 cells for 0..127, 12 of 127 over 56 pixels: 5.
    let bar: Vec<u8> = (0..7)
        .map(|c| ui.g.ch[CHIP_VIEW_BAR_ROW as usize * COLS + c])
        .collect();
    assert_eq!(bar[0], FONT_BAR0 as u8 + 5);
    assert_eq!(bar[1], FONT_BAR0 as u8);
    // Clamped at both ends.
    ui.turn(K1, 99);
    assert_eq!(param(&ui, LIVE_PARAM_AGE), 127);
    ui.turn(K1, -99);
    assert_eq!(param(&ui, LIVE_PARAM_AGE), 0);
    // The tick sends what changed to the chip (live.h LIVE_REG_CHAR).
    ui.turn(K1, 5);
    ui.rig.clear();
    ui.rig.tick();
    assert!(
        ui.rig.log().contains(&(LIVE_REG_CHAR_FIRST as u8 + 1, 20)),
        "{:?}",
        ui.rig.log()
    );
}

#[test]
fn the_knobs_turn_the_focused_slot() {
    let mut ui = chip_ui();
    let cutoff = ui.live().cutoff;
    for slot in 0..LIVE_SLOTS {
        ui.turn(PANEL_ENC_SELECT, 1);
        for k in 0..4 {
            ui.turn(K1 + k, 1 + k as i32 + slot as i32);
            assert_eq!(
                u32::from(ui.live().slot[slot as usize][(LIVE_SLOT_AMOUNT + k) as usize]),
                4 * (1 + k + slot),
                "slot {slot} K{}",
                k + 1
            );
        }
    }
    assert_eq!(ui.live().cutoff, cutoff);
    settle(&mut ui);
    assert_eq!(
        control(&ui, 3),
        ("MIX".into(), "24".into()),
        "FX is in focus"
    );
    ui.turn(PANEL_ENC_SELECT, -1);
    settle(&mut ui);
    assert_eq!(control(&ui, 0), ("AMT".into(), "8".into()), "MOD's Amount");
}

/// The EDIT layer stays a shortcut in every view: on the CHIP view, EDIT held turns the
/// selected slot from the chip's focus too, and in LIVE as before.
#[test]
fn the_edit_shortcut_turns_the_selected_slot_in_every_view() {
    let mut ui = chip_ui();
    ui.press(PANEL_BTN_SCL);
    assert_eq!(ui.live().fx_select, 1);
    ui.hold(PANEL_BTN_EDIT, true);
    ui.turn(K1, 2);
    ui.turn(K1 + 3, 1);
    ui.hold(PANEL_BTN_EDIT, false);
    assert_eq!(ui.live().slot[1][LIVE_SLOT_AMOUNT as usize], 8);
    assert_eq!(ui.live().slot[1][LIVE_SLOT_MIX as usize], 4);
    assert_eq!(param(&ui, LIVE_PARAM_AGE), 0, "EDIT + K1 turned the age");
    // ENV steps the selected slot's algorithm here as well.
    ui.press(PANEL_BTN_ENV);
    assert_eq!(ui.live().slot[1][LIVE_SLOT_ALGORITHM as usize], 1);
    // Back in LIVE: K1 is the cutoff again, EDIT + K1 the slot.
    ui.press(PANEL_BTN_HOME);
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "LIVE");
    assert_eq!(ui.p.page, PANEL_PAGE_LIVE as u8);
    let cutoff = ui.live().cutoff;
    ui.turn(K1, -1);
    assert_eq!(ui.live().cutoff, cutoff - LIVE_CUTOFF_DETENT as u16);
    ui.hold(PANEL_BTN_EDIT, true);
    ui.turn(K1, 1);
    ui.hold(PANEL_BTN_EDIT, false);
    assert_eq!(ui.live().slot[1][LIVE_SLOT_AMOUNT as usize], 12);
}

#[test]
fn the_panel_plays_on_the_chip_view() {
    let mut ui = chip_ui();
    let octave = ui.live().octave;
    ui.press(PANEL_BTN_OCT_UP);
    assert_eq!(ui.live().octave, octave + 1);
    ui.press(PANEL_BTN_OCT_DOWN);
    ui.press(PANEL_BTN_FX);
    assert_eq!(ui.live().model, SID_MODEL_8580, "FX");
    assert_eq!(param(&ui, LIVE_PARAM_PROFILE), SID_CHIP_8580_R5);
    ui.press(PANEL_BTN_FX);
    assert_eq!(param(&ui, LIVE_PARAM_PROFILE), SID_CHIP_6581_R4AR);
    // HOME goes back to the menu.
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
}

/// panel_knob_param on the CHIP view names what the panel turns, for every focus and knob, with
/// EDIT held or not: the page the views set is the one MIDI learn reads.
#[test]
fn the_knobs_parameters_follow_the_view() {
    let edit = 1 << PANEL_BTN_EDIT;
    for f in 0..CHIP_VIEW_SECTIONS {
        for held in [0, edit] {
            for enc in 0..PANEL_ENCODERS {
                if enc == PANEL_ENC_SELECT || enc == PANEL_ENC_ALGORITHM {
                    continue; // the view's own
                }
                let mut ui = chip_ui();
                ui.turn(PANEL_ENC_SELECT, f as i32);
                ui.hold(PANEL_BTN_EDIT, held != 0);
                let want = unsafe { panel_knob_param(&*ui.p, &*ui.rig.live, held, enc) };
                let before: Vec<u32> = (0..LIVE_PARAM_COUNT).map(|q| param(&ui, q)).collect();
                ui.turn(enc, 1);
                let changed: Vec<u32> = (0..LIVE_PARAM_COUNT)
                    .filter(|&q| param(&ui, q) != before[q as usize])
                    .collect();
                if want == LIVE_PARAM_COUNT {
                    assert!(changed.is_empty(), "focus {f} enc {enc}: {changed:?}");
                    continue;
                }
                assert_eq!(changed, [want], "focus {f} enc {enc} held {held}");
                let k = enc - K1;
                let slot = u32::from(ui.live().fx_select);
                let expect = if held != 0 || f != CHIP_VIEW_CHIP {
                    LIVE_PARAM_SLOT + slot * LIVE_SLOT_FIELDS + LIVE_SLOT_AMOUNT + k
                } else {
                    LIVE_PARAM_AGE + k
                };
                assert_eq!(want, expect, "focus {f} enc {enc} held {held}");
            }
        }
    }
}

/// MIDI learn over the CHIP view: K1 on the chip picks the age, the CC binds to it and moves
/// it, the page shows the value; on a slot K4 picks its Mix.
#[test]
fn midi_learn_binds_a_chip_view_knob() {
    let mut ui = chip_ui();
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *ui.v, &mut *ml);
    }
    ui.learn_on(K1);
    step(&mut ui);
    assert_eq!(ui.top(), "LEARN");
    assert_eq!(
        ui.p.page, PANEL_PAGE_CHARACTER as u8,
        "the page under learn"
    );
    assert_eq!(u32::from(ml.state), MIDI_LEARN_CC);
    assert_eq!(u32::from(ml.param), LIVE_PARAM_AGE);
    assert_eq!(param(&ui, LIVE_PARAM_AGE), 0, "the knob picked and turned");
    step(&mut ui);
    let row = ui.row(LEARN_VIEW_ROW as usize + 1);
    assert!(row[4..].starts_with("AGE # SEND A CC"), "{row}");
    // A CC from MIDI binds, closes the frame, and from then on moves the age.
    let cc = 21u8;
    let route = |ui: &mut Ui, ml: &mut midi_learn, value: u8| {
        let e = unsafe { midi_learn_route(ml, cc) };
        assert!(!e.is_null());
        let e = unsafe { &*e };
        unsafe {
            live_param_set(
                &mut *ui.rig.live,
                u32::from(e.param),
                midi_map_from_cc(e, value),
            )
        };
    };
    route(&mut ui, &mut ml, 100);
    assert_eq!(unsafe { midi_learn_cc_of(&*ml, LIVE_PARAM_AGE) }, cc);
    settle(&mut ui);
    assert_eq!(ui.top(), "CHIP");
    assert_eq!(param(&ui, LIVE_PARAM_AGE), 100);
    assert_eq!(control(&ui, 0).1, "100", "three digits: 2x");
    // On the FX slot, K4 is its Mix.
    ui.turn(PANEL_ENC_SELECT, 3);
    ui.learn_on(K1 + 3);
    assert_eq!(
        u32::from(ml.param),
        LIVE_PARAM_SLOT + 2 * LIVE_SLOT_FIELDS + LIVE_SLOT_MIX
    );
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "CHIP", "HOME cancelled learn and stayed");
}

/// Under MIDI learn's frame the CHIP view keeps SELECT and ALGORITHM: SELECT moves the focus
/// off the chip to DIST, so K4 then learns DIST's Mix, not the bleed; ALGORITHM steps DIST's
/// algorithm. The knobs stay learn's: they pick and do not turn.
#[test]
fn select_and_algorithm_work_under_the_learn_frame() {
    let mut ui = chip_ui();
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *ui.v, &mut *ml);
    }
    assert_eq!(focus(&ui), CHIP_VIEW_CHIP);
    assert_eq!(ui.live().fx_select, 0);
    ui.learn_on(K1);
    step(&mut ui);
    assert_eq!(ui.top(), "LEARN");
    ui.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(focus(&ui), 1, "SELECT moved the focus under the frame");
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(
        ui.live().slot[0][LIVE_SLOT_ALGORITHM as usize],
        1,
        "ALGORITHM stepped DIST's"
    );
    assert_eq!(ui.top(), "LEARN", "the frame stays");
    let mix = ui.live().slot[0][LIVE_SLOT_MIX as usize];
    ui.turn(K1 + 3, 1);
    assert_eq!(u32::from(ml.state), MIDI_LEARN_CC);
    assert_eq!(
        u32::from(ml.param),
        LIVE_PARAM_SLOT + LIVE_SLOT_MIX,
        "K4 learns DIST's Mix, not the bleed"
    );
    assert_eq!(
        ui.live().slot[0][LIVE_SLOT_MIX as usize],
        mix,
        "the knob picked, not turned"
    );
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "CHIP");
}

/// The page's names for the learn frame and its toasts: the character's controls.
#[test]
fn the_character_parameters_have_names() {
    let name = |p: u32| {
        let mut buf = [0 as std::ffi::c_char; LEARN_VIEW_NAME as usize];
        unsafe { learn_view_param_name(p, buf.as_mut_ptr()) };
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    };
    assert_eq!(name(LIVE_PARAM_PROFILE), "PROFILE");
    assert_eq!(name(LIVE_PARAM_AGE), "AGE");
    assert_eq!(name(LIVE_PARAM_DAC), "DAC");
    assert_eq!(name(LIVE_PARAM_THUMP), "THUMP");
    assert_eq!(name(LIVE_PARAM_BLEED), "BLEED");
}

/// MIDI out sends what the CHIP view's knobs turn as their CCs (docs/reference/midi.md).
#[test]
fn midi_out_sends_the_chip_views_knobs() {
    let mut ui = chip_ui();
    let mut out = Box::<midi_out>::default();
    unsafe { midi_out_init(&mut *out, &*ui.rig.live, 0, 0) };
    let tick = |ui: &Ui, out: &mut midi_out| {
        let mut buf = [0u32; MIDI_OUT_TICK_PACKETS as usize];
        let n = unsafe { midi_out_tick(out, &*ui.rig.live, 64, buf.as_mut_ptr()) };
        let mut got: Vec<[u8; 4]> = buf[..n as usize].iter().map(|p| p.to_le_bytes()).collect();
        got.sort();
        got
    };
    let cc = |p: u32, value: u32| {
        let e = unsafe { &*midi_map_for_param(p) };
        [0x0B, 0xB0, e.cc, unsafe { midi_map_to_cc(e, value) }]
    };
    ui.turn(K1, 2);
    ui.turn(K1 + 2, 1);
    assert_eq!(cc(LIVE_PARAM_AGE, 8), [0x0B, 0xB0, CC_AGE as u8, 8]);
    assert_eq!(
        tick(&ui, &mut out),
        [cc(LIVE_PARAM_AGE, 8), cc(LIVE_PARAM_THUMP, 4)]
    );
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(K1 + 1, 3);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    let dist = LIVE_PARAM_SLOT;
    assert_eq!(cc(dist + LIVE_SLOT_A, 12)[2], LIVE_CC_SLOT_FIRST as u8 + 2);
    assert_eq!(
        tick(&ui, &mut out),
        [
            cc(dist + LIVE_SLOT_ALGORITHM, 1),
            cc(dist + LIVE_SLOT_A, 12)
        ]
    );
}

/// What MIDI changes while the page shows is on it at the next frame; nothing new, nothing
/// composed.
#[test]
fn midi_changes_show_on_the_page() {
    let mut ui = chip_ui();
    // An idle page: nothing sent, no compose (the probe cell stays).
    let probe = 24 * COLS + 20;
    ui.g.ch[probe] = b'@';
    for _ in 0..30 {
        assert_eq!(step(&mut ui), Sent::default());
    }
    assert_eq!(ui.g.ch[probe], b'@', "the idle page was composed");
    ui.g.ch[probe] = b' ';
    drain(&mut ui.g);
    for (cc, value, row) in [
        (CC_AGE, 64, 0u32),
        (CC_DAC, 1, 1),
        (CC_THUMP, 127, 2),
        (CC_BLEED, 99, 3),
    ] {
        ui.rig.cc(cc, value);
        step(&mut ui);
        let want = unsafe { live_param_get(&*ui.rig.live, LIVE_PARAM_AGE + row) };
        assert_eq!(control(&ui, row as usize).1, want.to_string(), "CC {cc}");
    }
    ui.rig.cc(LIVE_CC_SLOT_FIRST + 5, 127); // MOD's algorithm: the last, WOW
    ui.rig.cc(LIVE_CC_SLOT_FIRST + 6, 50);
    step(&mut ui);
    assert_eq!(summary(&ui, 1, 3, 3), ("MOD".into(), "WOW".into()));
    ui.turn(PANEL_ENC_SELECT, 2);
    settle(&mut ui);
    assert_eq!(choice(&ui, 3), "WOW");
    assert_eq!(control(&ui, 0), ("WOW".into(), "50".into()));
    ui.turn(PANEL_ENC_SELECT, -2);
    settle(&mut ui);
    ui.rig.cc(CC_PROFILE, 40); // 4, worn: CC 85 spans the five built-in profiles, then USER 1..8
    step(&mut ui);
    assert_eq!(profile_word(&ui, 4), "WORN");
}

/// Everything changing at once from MIDI, with a toast and the chip's raster: no frame sends
/// more than CHIP_VIEW_FRAME_CELLS or the SPI budget, and the page ends whole.
#[test]
fn a_burst_of_changes_stays_within_the_budget() {
    let mut ui = chip_ui();
    let mut most = Sent::default();
    for round in 0..3u8 {
        ui.press(PANEL_BTN_OCT_UP); // a toast
        for cc in LIVE_CC_SLOT_FIRST..=LIVE_CC_SLOT_LAST {
            ui.rig.cc(cc, 127 - round * 60);
        }
        for cc in [CC_AGE, CC_DAC, CC_THUMP, CC_BLEED] {
            ui.rig.cc(cc, 127 - round * 60);
        }
        ui.turn(PANEL_ENC_SELECT, if round == 1 { -3 } else { 3 });
        ui.press(PANEL_BTN_FX); // a chip change: the raster
        let mut frames = 0;
        loop {
            let s = step(&mut ui);
            assert!(
                s.cells <= CHIP_VIEW_FRAME_CELLS as usize && s.spi_ms() <= SPI_BUDGET_MS,
                "round {round} frame {frames}: {s:?}, {:.1} ms",
                s.spi_ms()
            );
            if s.cells > most.cells {
                most = s;
            }
            frames += 1;
            let still = unsafe { screen_animating(&ui.v.s) } == 0 && ui.toast().is_none();
            if still && ui.v.chip.copying == 0 && frames > 60 {
                break;
            }
            assert!(frames < 200, "the page did not settle");
        }
    }
    assert!(ui.v.chip.held > 0, "the budget never held a frame back");
    println!("CHIP busiest frame {most:?}, {:.1} ms", most.spi_ms());
    // The page is whole: what a fresh compose would draw.
    let now: Vec<u8> = ui.g.ch.to_vec();
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut ui);
    assert_eq!(ui.g.ch.to_vec(), now);
}

/// The tabs (M2b Task 5): SELECT moves up from CHIP into them (clamped), ALGORITHM there
/// switches A and B (clamped); the title says which chip's page, in yellow with the tabs' focus,
/// KEYS:A or KEYS:B on the keys' chip; with no song the B tab shows the keys' setup, FOLLOWS KEYS
/// and the slots SHARED in its note, and K1..K4 turn the keys' setup on either tab.
#[test]
fn the_tabs_switch_mark_the_keys_and_show_the_keys_setup() {
    let mut ui = chip_ui();
    ui.rig.live.two_chips = 1;
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_AGE, 40) };
    settle(&mut ui);
    let t = theme_of(&ui);
    assert_eq!(big(&ui, 0, 0, 2, 6), "CHIP A");
    assert_eq!(small(&ui, 0, 24, 6), "KEYS:A", "the keys play chip A");
    ui.turn(PANEL_ENC_SELECT, -1);
    assert_eq!(focus(&ui), CHIP_VIEW_TABS);
    ui.turn(PANEL_ENC_SELECT, -3);
    assert_eq!(focus(&ui), CHIP_VIEW_TABS, "clamped above the tabs");
    settle(&mut ui);
    assert_eq!(
        ui.g.attr[0] & 15,
        t.hot,
        "the tabs' focus: the title yellow"
    );
    assert_eq!(
        tab_bg(&ui, 0),
        t.label,
        "CHIP, whose controls K1..K4 turn, marked grey"
    );
    let rows = |ui: &Ui| -> Vec<String> {
        (2..CHIP_VIEW_NOTE_ROW)
            .map(|r| ui.row(r as usize))
            .collect()
    };
    let a_page = rows(&ui);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.v.chip.tab, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 3);
    assert_eq!(ui.v.chip.tab, 1, "clamped at B");
    settle(&mut ui);
    assert_eq!(big(&ui, 0, 0, 2, 6), "CHIP B");
    assert_eq!(
        small(&ui, 0, 24, 6),
        "      ",
        "the keys play chip A, not B"
    );
    assert_eq!(
        rows(&ui),
        a_page,
        "with no song the B tab shows the keys' setup, the slots shared"
    );
    assert_eq!(
        small(&ui, CHIP_VIEW_NOTE_ROW, 0, 26).trim(),
        "FOLLOWS KEYS  SLOTS SHARED"
    );
    assert_eq!(
        small(&ui, CHIP_VIEW_HELP_ROW, 0, 26),
        "SELECT MOVE ALGORITHM STEP",
        "the help line keeps the moves"
    );
    // K1 on the tabs and on the chip turns the keys' age, on the B tab too.
    assert_eq!(unsafe { chip_view_page(&ui.v.chip) }, PANEL_PAGE_CHARACTER);
    ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(param(&ui, LIVE_PARAM_AGE), 44);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(param(&ui, LIVE_PARAM_AGE), 48);
    // The keys on chip B: KEYS moves, and the B tab's note says the slots are A's too.
    ui.rig.live.keys_chip = 1;
    settle(&mut ui);
    assert_eq!(small(&ui, 0, 24, 6), "KEYS:B");
    assert_eq!(
        small(&ui, CHIP_VIEW_NOTE_ROW, 0, 26).trim(),
        "SLOTS SHARED WITH CHIP A"
    );
    assert_eq!(
        small(&ui, CHIP_VIEW_HELP_ROW, 0, 26),
        "SELECT MOVE ALGORITHM STEP",
        "the help line keeps the moves"
    );
}

/// During a song the A tab shows the song's setup (keys_route.song_want) and edits it: the
/// profile on ALGORITHM, the controls on K1..K4 (the view's own: the panel's page names
/// nothing there, so MIDI learn binds nothing), and the keys' setup is left alone; the B tab
/// shows and edits the keys'.
#[test]
fn during_a_song_the_a_tab_edits_the_songs_setup() {
    let mut ui = chip_ui();
    ui.rig.live.two_chips = 1;
    ui.rig.live.keys_chip = 1;
    let mut kr = Box::<keys_route>::default();
    kr.l = &mut *ui.rig.live;
    kr.song = 1;
    kr.song_want = chip_setup {
        profile: SID_CHIP_6581_R2 as u8,
        controls: [10, 20, 30, 40],
        gate_reset: 1,
    };
    unsafe { chip_view_set_keys(&mut ui.v.chip, &mut *kr) };
    settle(&mut ui);
    assert_eq!(unsafe { chip_view_song_tab(&ui.v.chip) }, 1);
    assert_eq!(profile_word(&ui, 2), "R2");
    assert_eq!(control(&ui, 0).1, "10");
    assert_eq!(
        small(&ui, CHIP_VIEW_NOTE_ROW, 0, 26).trim(),
        "SONG SETUP  SLOTS SHARED"
    );
    assert_eq!(
        small(&ui, CHIP_VIEW_HELP_ROW, 0, 26),
        "SELECT MOVE ALGORITHM STEP",
        "the help line keeps the moves"
    );
    assert_eq!(unsafe { chip_view_page(&ui.v.chip) }, PANEL_PAGE_NONE);
    let (age, profile) = (param(&ui, LIVE_PARAM_AGE), param(&ui, LIVE_PARAM_PROFILE));
    ui.turn(PANEL_ENC_K1, 2);
    // The panel's page as the event left it (views set it before and after each event).
    let p: *const panel = &*ui.p;
    let learn = unsafe { panel_knob_param(p, &*ui.rig.live, 0, PANEL_ENC_K1) };
    assert_eq!(
        learn, LIVE_PARAM_COUNT,
        "MIDI learn would bind a keys' control"
    );
    ui.turn(PANEL_ENC_K4, -1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(kr.song_want.controls, [18, 20, 30, 36]);
    assert_eq!(u32::from(kr.song_want.profile), SID_CHIP_6581_R2 + 1);
    assert_eq!(
        (param(&ui, LIVE_PARAM_AGE), param(&ui, LIVE_PARAM_PROFILE)),
        (age, profile)
    );
    settle(&mut ui);
    assert_eq!(control(&ui, 0).1, "18");
    // The B tab: the keys' setup, the knobs on it.
    ui.turn(PANEL_ENC_SELECT, -1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(unsafe { chip_view_page(&ui.v.chip) }, PANEL_PAGE_CHARACTER);
    ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(param(&ui, LIVE_PARAM_AGE), age + 4);
    assert_eq!(kr.song_want.controls[0], 18, "the song's age moved");
    unsafe { chip_view_set_keys(&mut ui.v.chip, std::ptr::null_mut()) };
}

#[test]
fn only_explicit_song_chip_edits_notify_the_edit_callback() {
    unsafe extern "C" fn edited(
        ctx: *mut std::ffi::c_void,
        want: *const chip_setup,
        mask: u32,
    ) -> i32 {
        let edits = unsafe { &mut *ctx.cast::<Vec<(chip_setup, u32)>>() };
        edits.push((unsafe { *want }, mask));
        1
    }

    let mut ui = chip_ui();
    let mut edits = Vec::<(chip_setup, u32)>::new();
    let mut kr = Box::<keys_route>::default();
    kr.l = &mut *ui.rig.live;
    kr.song = 1;
    kr.song_want.controls = [10, 20, 30, 40];
    kr.song_edit = Some(edited);
    kr.song_edit_ctx = (&mut edits as *mut Vec<(chip_setup, u32)>).cast();
    unsafe { chip_view_set_keys(&mut ui.v.chip, &mut *kr) };

    let locked = chip_setup {
        controls: [90, 20, 30, 40],
        ..kr.song_want
    };
    unsafe { keys_route_song_wants(&mut *kr, &locked) };
    assert!(edits.is_empty(), "runtime locks must not count as edits");
    ui.turn(PANEL_ENC_K1, 1);
    ui.turn(PANEL_ENC_K4, -1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(edits.len(), 3);
    assert_eq!(edits[0].0.controls, [94, 20, 30, 40]);
    assert_eq!(edits[1].0.controls, [90, 20, 30, 36]);
    assert_eq!(edits[2].0.profile, locked.profile + 1);
    assert_eq!(
        edits.iter().map(|e| e.1).collect::<Vec<_>>(),
        [1, 8, KEYS_ROUTE_EDIT_PROFILE]
    );
    assert_eq!(
        kr.song_want.controls, locked.controls,
        "callback owns acceptance"
    );
    assert_eq!(kr.song_want.profile, locked.profile);
    ui.rig.live.user_valid = 1;
    ui.rig.live.user_base[0] = locked.profile;
    ui.rig.live.user_ctl[0] = locked.controls;
    ui.turn(PANEL_ENC_ALGORITHM, SID_CHIPS as i32);
    assert_eq!(edits[3].0.controls, locked.controls);
    assert_eq!(
        edits[3].1,
        KEYS_ROUTE_EDIT_PROFILE | KEYS_ROUTE_EDIT_CONTROLS,
        "USER selection explicitly supplies every control"
    );

    ui.hold(PANEL_BTN_EDIT, true);
    ui.turn(PANEL_ENC_K1, 1);
    ui.hold(PANEL_BTN_EDIT, false);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.turn(PANEL_ENC_K1, 1);
    ui.turn(PANEL_ENC_SELECT, -2);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(
        edits.len(),
        4,
        "slots and keys must not edit the song setup"
    );
    unsafe { chip_view_set_keys(&mut ui.v.chip, std::ptr::null_mut()) };
}

#[test]
fn refused_song_chip_edits_keep_the_setup_user_identity_and_popup_value() {
    unsafe extern "C" fn refuse(ctx: *mut std::ffi::c_void, _: *const chip_setup, _: u32) -> i32 {
        unsafe { *ctx.cast::<u32>() += 1 };
        0
    }

    let mut ui = chip_ui();
    let mut calls = 0u32;
    let mut kr = Box::<keys_route>::default();
    kr.l = &mut *ui.rig.live;
    kr.song = 1;
    kr.song_want.profile = SID_CHIP_6581_R3 as u8;
    kr.song_want.controls = [10, 20, 30, 40];
    kr.song_user = 1;
    kr.song_edit = Some(refuse);
    kr.song_edit_ctx = (&mut calls as *mut u32).cast();
    ui.rig.live.user_valid = 3;
    ui.rig.live.user_base[0] = kr.song_want.profile;
    ui.rig.live.user_base[1] = kr.song_want.profile;
    ui.rig.live.user_ctl[1] = [50, 60, 70, 80];
    unsafe { chip_view_set_keys(&mut ui.v.chip, &mut *kr) };
    let before = kr.song_want;

    for (encoder, steps) in [
        (PANEL_ENC_ALGORITHM, 1),
        (PANEL_ENC_ALGORITHM, -1),
        (PANEL_ENC_K1, 1),
    ] {
        ui.turn(encoder, steps);
        assert_eq!(kr.song_want.profile, before.profile);
        assert_eq!(kr.song_want.controls, before.controls);
        assert_eq!(kr.song_user, 1);
        assert_eq!(unsafe { chip_view_user(&ui.v.chip) }, 1);
        ui.frame();
        let value = unsafe { CStr::from_ptr(ui.v.popup.value.as_ptr()) }
            .to_str()
            .unwrap();
        assert_eq!(
            value,
            if encoder == PANEL_ENC_ALGORITHM {
                "USER 1"
            } else {
                "10"
            }
        );
    }
    assert_eq!(calls, 3);
    unsafe { chip_view_set_keys(&mut ui.v.chip, std::ptr::null_mut()) };
}

/// MIDI learn over the song's A tab (PANEL_PAGE_NONE): K1..K4 pick nothing, and the toast says
/// why (SONG SETUP: NO CC), not K4's preset message.
#[test]
fn learn_over_the_songs_tab_says_its_knobs_have_no_cc() {
    let mut ui = chip_ui();
    ui.rig.live.two_chips = 1;
    ui.rig.live.keys_chip = 1;
    let mut kr = Box::<keys_route>::default();
    kr.l = &mut *ui.rig.live;
    kr.song = 1;
    unsafe { chip_view_set_keys(&mut ui.v.chip, &mut *kr) };
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *ui.v, &mut *ml);
    }
    ui.learn_on(K1);
    step(&mut ui);
    assert_eq!(ui.p.page, PANEL_PAGE_NONE as u8);
    assert_eq!(ui.top(), "CHIP", "no frame on the song's tab");
    assert_eq!(
        u32::from(ml.state),
        MIDI_LEARN_IDLE,
        "a knob picked on the song's tab"
    );
    assert_eq!(ui.toast().as_deref(), Some("SONG SETUP: NO CC"));
    unsafe { chip_view_set_keys(&mut ui.v.chip, std::ptr::null_mut()) };
}
