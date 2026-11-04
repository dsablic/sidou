// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::lab_edit::edit_with;
use crate::lab_layers::{card, sound};
use crate::lab_view::{choice, lab_ui, settle, to_page};
use crate::views::Ui;
use crate::{PANEL_ENC_K2, PANEL_ENC_K3};

const PAGE: u32 = LAB_PAGE_TABLE;

fn table_page() -> Ui {
    let mut ui = lab_ui();
    to_page(&mut ui, PAGE);
    ui
}

fn turn(ui: &mut Ui, enc: u32, d: i32) {
    ui.turn(enc, d);
    settle(ui);
}

fn values(ui: &Ui) -> [String; 4] {
    std::array::from_fn(|k| card(ui, PAGE, k as u32).1)
}

fn step(ui: &Ui, n: usize) -> (u8, u8) {
    let s = sound(ui);
    let at = LAB_SOUND_TABLE_STEPS as usize + 2 * n;
    (s[at], s[at + 1])
}

fn steps(ui: &Ui) -> u32 {
    u32::from(sound(ui)[LAB_SOUND_TABLE as usize]) & LAB_TABLE_COUNT_MASK
}

#[test]
fn algorithm_turns_the_table_on_with_the_sounds_own_waveform_and_picks_how_it_ends() {
    let mut ui = table_page();
    assert_eq!(choice(&ui), "TABLE OFF");
    let control = sound(&ui)[LAB_SOUND_CONTROL as usize];
    turn(&mut ui, PANEL_ENC_ALGORITHM, 1);
    assert_eq!(choice(&ui), "PLAY ONCE");
    assert_eq!(step(&ui, 0), (control, 0), "step 1 is the sound as it was");
    let shown = values(&ui);
    assert_eq!([&shown[0], &shown[2], &shown[3]], ["1", "0", "1"]);
    turn(&mut ui, PANEL_ENC_ALGORITHM, 1);
    assert_eq!(choice(&ui), "LOOP FROM 1");
    turn(&mut ui, PANEL_ENC_ALGORITHM, 5);
    assert_eq!(choice(&ui), "LOOP FROM 1", "no loop past the steps");
    turn(&mut ui, PANEL_ENC_ALGORITHM, -9);
    assert_eq!(choice(&ui), "TABLE OFF");
    assert_eq!(steps(&ui), 1, "off keeps the steps for the next time");
}

#[test]
fn a_turn_on_new_adds_a_step_and_wave_end_cuts_the_table_there() {
    let mut ui = table_page();
    turn(&mut ui, PANEL_ENC_ALGORITHM, 1);
    turn(&mut ui, PANEL_ENC_K1, 5);
    assert_eq!(values(&ui)[0], "NEW", "one past the end");
    turn(&mut ui, PANEL_ENC_K3, 12);
    assert_eq!(steps(&ui), 2);
    let shown = values(&ui);
    assert_eq!([&shown[0], &shown[2]], ["2", "+12"]);
    turn(&mut ui, PANEL_ENC_K1, 1);
    turn(&mut ui, PANEL_ENC_K2, 1);
    assert_eq!(steps(&ui), 3, "a copy of step 2, its waveform turned");
    assert_eq!(step(&ui, 2).1, 12);
    turn(&mut ui, PANEL_ENC_K2, -20);
    assert_eq!(values(&ui)[1], "OFF", "a fast turn stops at OFF");
    assert_eq!(steps(&ui), 3);
    turn(&mut ui, PANEL_ENC_K2, -1);
    assert_eq!(
        steps(&ui),
        2,
        "END from OFF cuts the table before this step"
    );
    assert_eq!(values(&ui)[0], "NEW");
}

#[test]
fn the_cards_name_each_wave_and_note_in_three_characters_and_set_the_rate() {
    let mut ui = table_page();
    turn(&mut ui, PANEL_ENC_ALGORITHM, 1);
    let mut seen = Vec::new();
    turn(&mut ui, PANEL_ENC_K2, -20);
    for _ in 0..LAB_TABLE_WAVE_CODES {
        seen.push(values(&ui)[1].clone());
        turn(&mut ui, PANEL_ENC_K2, 1);
    }
    assert_eq!(
        seen,
        [
            "OFF", "TRI", "SAW", "PUL", "NOI", "T+S", "T+P", "S+P", "TSP", "TRR", "SAS", "PUS",
            "PUS"
        ]
    );
    turn(&mut ui, PANEL_ENC_K3, -30);
    assert_eq!(values(&ui)[2], "-24");
    assert_eq!(step(&ui, 0).1, LAB_TABLE_NOTE_NONE as u8 - 24);
    turn(&mut ui, PANEL_ENC_K4, 30);
    assert_eq!(values(&ui)[3], "16");
    assert_eq!(
        sound(&ui)[LAB_SOUND_TABLE_RATE as usize],
        LAB_TABLE_RATE_MAX as u8 - 1
    );
    for v in values(&ui) {
        assert!(v.len() <= 3, "{v} fits a card at 2x");
    }
}

#[test]
fn an_imported_absolute_note_shows_its_name_and_turns_by_a_semitone() {
    let mut ui = table_page();
    let mut s = sound(&ui);
    crate::live_table::Table::once(&[(0x88, LAB_TABLE_NOTE_NONE as u8 + 48)], 1).put(&mut s);
    unsafe { live_sound_apply(&mut *ui.rig.live, s.as_ptr()) };
    settle(&mut ui);
    assert_eq!(
        values(&ui)[1],
        "88",
        "a control outside the list as its hex"
    );
    assert_eq!(values(&ui)[2], "C-4");
    turn(&mut ui, PANEL_ENC_K3, 1);
    assert_eq!(values(&ui)[2], "C#4");
}

#[test]
fn the_table_is_turned_on_for_the_layer_being_edited_and_the_header_names_it() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_LAYERS);
    turn(&mut ui, PANEL_ENC_ALGORITHM, 1);
    turn(&mut ui, PANEL_ENC_K1, 1);
    to_page(&mut ui, PAGE);
    turn(&mut ui, PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.big(0, 0, 2, 8), "TABLE L2");
    let head = u32::from(sound(&ui)[LAB_SOUND_TABLE as usize]);
    assert_eq!((head & LAB_TABLE_LAYER_MASK) >> LAB_TABLE_LAYER_SHIFT, 1);
}

#[test]
fn a_table_edit_is_one_undo_step() {
    let mut ui = table_page();
    let before = sound(&ui);
    turn(&mut ui, PANEL_ENC_ALGORITHM, 1);
    turn(&mut ui, PANEL_ENC_K3, 7);
    let edited = sound(&ui);
    let _ = edit_with(&mut ui, PANEL_BTN_LFO);
    assert_ne!(sound(&ui), edited, "the note's run undone");
    assert_eq!(step(&ui, 0).1, 0);
    let _ = edit_with(&mut ui, PANEL_BTN_LFO);
    assert_eq!(sound(&ui), before, "then the table's turn on");
}
