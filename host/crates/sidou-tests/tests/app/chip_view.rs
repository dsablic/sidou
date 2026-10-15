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
const SAVE: u32 = LEARN_VIEW_BUTTON;
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
        if !card && unsafe { screen_animating(&ui.v.s) } == 0 && ui.v.chip.copying == 0 {
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

/// Cells [from, to) of a row, trimmed at the end.
fn text(ui: &Ui, row: u32, from: usize, to: usize) -> String {
    ui.row(row as usize)[from..to].trim_end().to_owned()
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

/// The character row r (0 age .. 3 bleed): its label and its value.
fn character_row(ui: &Ui, r: u32) -> (String, String) {
    let row = CHIP_VIEW_CHARACTER_ROW + 1 + r;
    (text(ui, row, 4, 9), text(ui, row, 23, 26).trim().to_owned())
}

/// A slot frame's title, the text between the corner's space and the next space-run.
fn slot_title(ui: &Ui, slot: u32) -> String {
    let row = CHIP_VIEW_SLOT_ROW + CHIP_VIEW_SLOT_ROWS * slot;
    let r = ui.row(row as usize);
    r[4..27].split("##").next().unwrap().trim().to_owned()
}

/// A slot's field `k` (0 Amount, 1 A, 2 B, 3 Mix): its label and value as the page shows them.
fn slot_field(ui: &Ui, slot: u32, k: u32) -> (String, String) {
    let row = CHIP_VIEW_SLOT_ROW + CHIP_VIEW_SLOT_ROWS * slot + 1 + k / 2;
    let at = 4 + 12 * (k as usize % 2);
    (
        text(ui, row, at, at + 5),
        text(ui, row, at + 6, at + 9).trim().to_owned(),
    )
}

#[test]
fn chip_opens_from_the_menu_behind_the_chip_card() {
    let mut ui = Ui::new();
    ui.settle();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    // The menu's first item opens the view: an arrow, no value.
    assert_eq!(&ui.row(5)[4..8], "CHIP");
    assert_eq!(ui.g.ch[5 * COLS + 26], FONT_ARROW_RIGHT as u8);
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
fn the_page_shows_the_profile_the_character_and_the_slots() {
    let ui = chip_ui();
    assert_eq!(text(&ui, 2, 2, 6), "CHIP");
    // The tabs: A in reverse video, B greyed with its milestone.
    assert_eq!(text(&ui, 2, 20, 28), " A  B M2");
    let at = |ui: &Ui, c: usize, r: usize| ui.g.attr[r * COLS + c];
    assert_eq!(at(&ui, 21, 2), ((GRID_LIGHT_BLUE << 4) | GRID_BLUE) as u8);
    assert_eq!(at(&ui, 24, 2) & 15, GRID_GREY as u8);
    // The frames and their titles.
    assert_eq!(text(&ui, CHIP_VIEW_CHIP_ROW, 3, 9), " CHIP");
    assert_eq!(text(&ui, CHIP_VIEW_CHARACTER_ROW, 3, 14), " CHARACTER");
    assert_eq!(
        text(&ui, CHIP_VIEW_CHIP_ROW + 1, 4, 26),
        format!("PROFILE{:>15}", "6581 R4AR")
    );
    assert_eq!(
        text(&ui, CHIP_VIEW_CHIP_ROW + 2, 4, 26),
        format!("MODEL{:>17}", "6581")
    );
    for (r, name) in ["AGE", "DAC", "THUMP", "BLEED"].iter().enumerate() {
        assert_eq!(character_row(&ui, r as u32), (name.to_string(), "0".into()));
    }
    for (slot, name) in ["DIST OFF", "MOD OFF", "FX OFF"].iter().enumerate() {
        assert_eq!(slot_title(&ui, slot as u32), *name);
        for (k, l) in ["AMT", "A", "B", "MIX"].iter().enumerate() {
            assert_eq!(
                slot_field(&ui, slot as u32, k as u32),
                (l.to_string(), "0".into())
            );
        }
    }
    assert_eq!(
        text(&ui, CHIP_VIEW_HELP_ROW, 2, 28),
        "SELECT MOVE ALGORITHM STEP"
    );
    // The focus on the chip: its two frames white, the play mark in the margin.
    assert_eq!(focus(&ui), CHIP_VIEW_CHIP);
    assert_eq!(
        ui.g.ch[CHIP_VIEW_CHIP_ROW as usize * COLS + 1],
        FONT_PLAY as u8
    );
    assert_eq!(
        at(&ui, 2, CHIP_VIEW_CHIP_ROW as usize) & 15,
        GRID_WHITE as u8
    );
    assert_eq!(
        at(&ui, 2, CHIP_VIEW_CHARACTER_ROW as usize) & 15,
        GRID_WHITE as u8
    );
    assert_eq!(
        at(&ui, 2, CHIP_VIEW_SLOT_ROW as usize) & 15,
        GRID_LIGHT_BLUE as u8
    );
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
    assert_eq!(focus(&ui), CHIP_VIEW_CHIP);
    // On the chip, the selected slot stays as it was.
    assert_eq!(ui.live().fx_select, 2);
    // SCL on a slot moves the focus with the slot; on the chip only the slot.
    ui.press(PANEL_BTN_SCL);
    assert_eq!((focus(&ui), ui.live().fx_select), (CHIP_VIEW_CHIP, 0));
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.press(PANEL_BTN_SCL);
    assert_eq!((focus(&ui), ui.live().fx_select), (2, 1));
    // The page shows the focus: the play mark and the white frame move.
    settle(&mut ui);
    let mark = |ui: &Ui, row: u32| ui.g.ch[row as usize * COLS + 1] == FONT_PLAY as u8;
    let mod_row = CHIP_VIEW_SLOT_ROW + CHIP_VIEW_SLOT_ROWS;
    assert!(mark(&ui, mod_row) && !mark(&ui, CHIP_VIEW_CHIP_ROW));
    assert_eq!(
        ui.g.attr[mod_row as usize * COLS + 2] & 15,
        GRID_WHITE as u8
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
    assert_eq!(
        text(&ui, CHIP_VIEW_CHIP_ROW + 1, 4, 26),
        format!("PROFILE{:>15}", "8580 R5")
    );
    assert_eq!(text(&ui, CHIP_VIEW_CHIP_ROW + 2, 22, 26), "8580");
    // Clamped at both ends; every profile in order on the way.
    ui.turn(PANEL_ENC_ALGORITHM, 9);
    assert_eq!(profile(&ui), SID_CHIPS - 1);
    for id in (0..SID_CHIPS).rev() {
        ui.rig.tick();
        settle(&mut ui);
        assert_eq!(profile(&ui), id);
        assert_eq!(
            text(&ui, CHIP_VIEW_CHIP_ROW + 1, 4, 26),
            format!("PROFILE{:>15}", profile_name(id))
        );
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
        let title = slot_title(&ui, slot);
        let names = ["DIST TAPE", "MOD CHORUS", "FX DELAY"];
        assert_eq!(title, names[slot as usize]);
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
    assert_eq!(slot_title(&ui, 0), "DIST OVERDRIVE");
    assert_eq!(slot_field(&ui, 0, 1).0, "TONE");
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
    assert_eq!(character_row(&ui, 0).1, "12");
    assert_eq!(character_row(&ui, 3).1, "24");
    // The bar: 12 cells for 0..127.
    let bar_row = (CHIP_VIEW_CHARACTER_ROW + 1) as usize;
    let bar: Vec<u8> = (10..22).map(|c| ui.g.ch[bar_row * COLS + c]).collect();
    let px = 12 * 12 * 8 / 127; // 12 of 127 over 96 pixels: 9
    assert_eq!(bar[0], FONT_BAR8 as u8);
    assert_eq!(bar[1], FONT_BAR0 as u8 + px as u8 - 8);
    assert_eq!(bar[2], FONT_BAR0 as u8);
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
    assert_eq!(slot_field(&ui, 2, 3), ("MIX".into(), "24".into()));
    assert_eq!(slot_field(&ui, 1, 0), ("AMT".into(), "8".into()));
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
    ui.press(SAVE);
    step(&mut ui);
    assert_eq!(ui.top(), "LEARN");
    assert_eq!(
        ui.p.page, PANEL_PAGE_CHARACTER as u8,
        "the page under learn"
    );
    ui.turn(K1, 1);
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
    assert_eq!(character_row(&ui, 0).1, "100");
    // On the FX slot, K4 is its Mix.
    ui.turn(PANEL_ENC_SELECT, 3);
    ui.press(SAVE);
    ui.turn(K1 + 3, 1);
    assert_eq!(
        u32::from(ml.param),
        LIVE_PARAM_SLOT + 2 * LIVE_SLOT_FIELDS + LIVE_SLOT_MIX
    );
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "CHIP", "HOME cancelled learn and stayed");
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
    let probe = 28 * COLS + 20;
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
        assert_eq!(character_row(&ui, row).1, want.to_string(), "CC {cc}");
    }
    ui.rig.cc(LIVE_CC_SLOT_FIRST + 5, 127); // MOD's algorithm: the last, WOW
    ui.rig.cc(LIVE_CC_SLOT_FIRST + 6, 50);
    step(&mut ui);
    assert_eq!(slot_title(&ui, 1), "MOD WOW");
    assert_eq!(slot_field(&ui, 1, 0), ("WOW".into(), "50".into()));
    ui.rig.cc(CC_PROFILE, 127); // the last: worn
    step(&mut ui);
    assert_eq!(
        text(&ui, CHIP_VIEW_CHIP_ROW + 1, 4, 26),
        format!("PROFILE{:>15}", "WORN 6581")
    );
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
