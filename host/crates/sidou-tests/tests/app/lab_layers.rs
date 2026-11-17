// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::raw::c_char;

use sidou_sys::*;

use crate::lab_view::{choice, lab_ui, settle, to_page};
use crate::views::Ui;
use crate::{c_text, PANEL_ENC_K2, PANEL_ENC_K3};

pub(crate) fn card(ui: &Ui, page: u32, k: u32) -> (String, String) {
    let mut name = [0 as c_char; LAB_VIEW_TEXT as usize];
    let mut value = [0 as c_char; LAB_VIEW_TEXT as usize];
    let (mut fill, mut max) = (0u32, 0u32);
    unsafe {
        lab_view_card(
            &ui.v.lab,
            page,
            k,
            name.as_mut_ptr(),
            value.as_mut_ptr(),
            &mut fill,
            &mut max,
        )
    };
    (c_text(name.as_ptr()), c_text(value.as_ptr()))
}

pub(crate) fn sound(ui: &Ui) -> [u8; LAB_SOUND_BYTES as usize] {
    let mut s = [0u8; LAB_SOUND_BYTES as usize];
    unsafe { live_sound_get(ui.live(), s.as_mut_ptr()) };
    s
}

fn layers_page(extra: i32) -> Ui {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_LAYERS);
    ui.turn(PANEL_ENC_ALGORITHM, extra);
    settle(&mut ui);
    ui
}

#[test]
fn algorithm_sets_how_many_oscillators_a_note_plays() {
    let mut ui = layers_page(0);
    assert_eq!(choice(&ui), "ONE OSC");
    for (turn, name, layers) in [
        (1, "TWO OSC", 1),
        (1, "THREE OSC", 2),
        (1, "THREE OSC", 2),
        (-2, "ONE OSC", 0),
    ] {
        ui.turn(PANEL_ENC_ALGORITHM, turn);
        settle(&mut ui);
        assert_eq!(choice(&ui), name);
        assert_eq!(ui.live().layers, layers);
    }
}

#[test]
fn k1_picks_the_layer_osc_and_envelope_edit_and_their_header_says_which() {
    let mut ui = layers_page(1);
    assert_eq!(card(&ui, LAB_PAGE_LAYERS, 0), ("EDIT".into(), "L1".into()));
    ui.turn(PANEL_ENC_K1, 5);
    settle(&mut ui);
    assert_eq!(
        card(&ui, LAB_PAGE_LAYERS, 0).1,
        "L2",
        "no layer past the count"
    );
    assert_eq!(ui.live().layer_edit, 1);
    for (page, name) in [(LAB_PAGE_OSC, "OSC L2"), (LAB_PAGE_ENVELOPE, "ENVELOPE L2")] {
        to_page(&mut ui, page);
        assert_eq!(ui.big(0, 0, 2, name.len()), name);
    }
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    ui.turn(PANEL_ENC_K1, 3);
    settle(&mut ui);
    let s = sound(&ui);
    let layer2 = (LAB_SOUND_LAYER_VOICE + 1) as usize;
    assert_ne!(
        s[layer2] >> 4,
        s[LAB_SOUND_AD as usize] >> 4,
        "the attack turned on layer 2 only"
    );
}

#[test]
fn k2_and_k3_tune_the_edited_layer() {
    let mut ui = layers_page(1);
    ui.turn(PANEL_ENC_K1, 1);
    ui.turn(PANEL_ENC_K2, 12);
    ui.turn(PANEL_ENC_K3, -25);
    settle(&mut ui);
    assert_eq!(
        card(&ui, LAB_PAGE_LAYERS, 1),
        ("TRANS".into(), "+12".into())
    );
    assert_eq!(
        card(&ui, LAB_PAGE_LAYERS, 2),
        ("DETUNE".into(), "-25".into())
    );
    let s = sound(&ui);
    let tuning = (LAB_SOUND_LAYER_TUNING + LAB_SOUND_LAYER_TUNING_BYTES) as usize;
    assert_eq!((s[tuning] as i8, s[tuning + 1] as i8), (12, -25));
}

fn levels_sent(ui: &Ui) -> [u8; 3] {
    let mut sent = [0u8; 3];
    for &(reg, value) in ui.rig.log() {
        if let Some(v) = u32::from(reg).checked_sub(LIVE_REG_LEVEL_FIRST).filter(|&v| v < 3) {
            sent[v as usize] = value;
        }
    }
    sent
}

fn level_cut(ui: &Ui) -> [u8; 3] {
    let s = sound(ui);
    let at = LAB_SOUND_LEVEL_CUT as usize;
    [s[at], s[at + 1], s[at + 2]]
}

#[test]
fn k4_sets_the_edited_layers_level_on_its_voice_alone() {
    let mut ui = layers_page(2);
    assert_eq!(card(&ui, LAB_PAGE_LAYERS, 3), ("LEVEL".into(), "100".into()));
    ui.turn(PANEL_ENC_K1, 1);
    ui.turn(PANEL_ENC_K4, -60);
    settle(&mut ui);
    assert_eq!(card(&ui, LAB_PAGE_LAYERS, 3), ("LEVEL".into(), "40".into()));
    assert_eq!(levels_sent(&ui), [100, 40, 100]);
    assert_eq!(level_cut(&ui), [0, 60, 0], "kept in the sound as 100 less it");
    ui.turn(PANEL_ENC_K4, -64);
    ui.turn(PANEL_ENC_K4, -64);
    settle(&mut ui);
    assert_eq!(levels_sent(&ui), [100, 0, 100], "down to silent");
    ui.turn(PANEL_ENC_K4, 64);
    ui.turn(PANEL_ENC_K4, 64);
    settle(&mut ui);
    assert_eq!(levels_sent(&ui), [100, 100, 100], "and no further than full");
}

#[test]
fn one_osc_plays_its_chord_at_the_first_layers_level_and_a_new_layer_starts_there() {
    let mut ui = layers_page(0);
    ui.turn(PANEL_ENC_K4, -30);
    settle(&mut ui);
    assert_eq!(levels_sent(&ui), [70, 70, 70], "every voice of the chord");
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut ui);
    assert_eq!(levels_sent(&ui), [70, 70, 100], "layer 2 a copy of the first, voice 3 unused");
    ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(card(&ui, LAB_PAGE_LAYERS, 3).1, "70");
}

#[test]
fn the_level_comes_back_with_the_sound_and_a_preset_plays_at_full() {
    let mut ui = layers_page(2);
    ui.turn(PANEL_ENC_K1, 2);
    ui.turn(PANEL_ENC_K4, -45);
    settle(&mut ui);
    let kept = sound(&ui);
    unsafe { live_set_preset(&mut *ui.rig.live, 1) };
    settle(&mut ui);
    assert_eq!(levels_sent(&ui), [100; 3]);
    unsafe { live_sound_apply(&mut *ui.rig.live, kept.as_ptr()) };
    settle(&mut ui);
    assert_eq!(levels_sent(&ui), [100, 100, 55]);
    assert_eq!(sound(&ui), kept);
}

#[test]
fn vary_on_layers_changes_the_layers_not_their_levels() {
    let mut ui = layers_page(0);
    ui.turn(PANEL_ENC_K4, -20);
    ui.turn(PANEL_ENC_ALGORITHM, 2);
    settle(&mut ui);
    let before = sound(&ui);
    for _ in 0..40 {
        assert_eq!(
            crate::lab_edit::edit_with(&mut ui, PANEL_BTN_ARP).as_deref(),
            Some("VARIED")
        );
        assert_eq!(level_cut(&ui), [20; 3]);
    }
    assert_ne!(sound(&ui), before);
}

#[test]
fn the_page_shows_each_layer_and_marks_the_edited_one() {
    let mut ui = layers_page(2);
    ui.turn(PANEL_ENC_K1, 1);
    ui.turn(PANEL_ENC_K2, 7);
    ui.turn(PANEL_ENC_K4, -35);
    settle(&mut ui);
    let art = (LAB_VIEW_ART_ROW..LAB_VIEW_ART_ROW + LAB_VIEW_ART_ROWS)
        .map(|r| ui.row(r as usize))
        .collect::<Vec<_>>()
        .join("\n");
    for text in ["L1", "L2", "L3", "+7", "65", "100"] {
        assert!(art.contains(text), "{text} in\n{art}");
    }
}

#[test]
fn a_one_voice_sync_sound_keeps_one_oscillator_and_says_why() {
    let mut ui = lab_ui();
    unsafe { live_set_preset(&mut *ui.rig.live, 2) };
    settle(&mut ui);
    assert_eq!(ui.live().mono_sync, 1, "SYNC LEAD");
    to_page(&mut ui, LAB_PAGE_LAYERS);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut ui);
    assert_eq!(ui.live().layers, 0);
    assert_eq!(choice(&ui), "ONE OSC: SYNC");
}

#[test]
fn filter_env_shapes_the_cutoff_sweep_with_its_depth_on_algorithm() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER_ENV);
    assert_eq!(choice(&ui), "ENV OFF");
    ui.turn(PANEL_ENC_ALGORITHM, 20);
    ui.turn(PANEL_ENC_K1, 3);
    ui.turn(PANEL_ENC_K3, -15);
    settle(&mut ui);
    assert_eq!(choice(&ui), "DEPTH +20");
    assert_eq!(
        card(&ui, LAB_PAGE_FILTER_ENV, 0),
        ("ATTACK".into(), "3".into())
    );
    assert_eq!(
        card(&ui, LAB_PAGE_FILTER_ENV, 2),
        ("SUS".into(), "0".into())
    );
    let s = sound(&ui);
    assert_eq!(s[LAB_SOUND_FILTER_ENV as usize] as i8, 20);
    assert_eq!(s[LAB_SOUND_FILTER_ENV_AD as usize] >> 4, 3);
    assert_eq!(s[LAB_SOUND_FILTER_ENV_SR as usize] >> 4, 0);
}
