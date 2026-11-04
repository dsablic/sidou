// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::raw::c_char;

use sidou_sys::*;

use crate::lab_view::{choice, lab_ui, settle, to_page};
use crate::views::Ui;
use crate::{c_text, PANEL_ENC_K2, PANEL_ENC_K3};

fn card(ui: &Ui, page: u32, k: u32) -> (String, String) {
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

fn sound(ui: &Ui) -> [u8; LAB_SOUND_BYTES as usize] {
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
fn k2_k3_and_k4_tune_and_route_the_edited_layer() {
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

    let routed = |ui: &Ui| ui.live().filter_voices >> 1 & 1;
    let before = routed(&ui);
    ui.turn(PANEL_ENC_K4, if before == 1 { -1 } else { 1 });
    settle(&mut ui);
    assert_ne!(routed(&ui), before);
    assert_eq!(
        card(&ui, LAB_PAGE_LAYERS, 3).1,
        if before == 1 { "OFF" } else { "ON" }
    );
}

#[test]
fn the_page_shows_each_layer_and_marks_the_edited_one() {
    let mut ui = layers_page(2);
    ui.turn(PANEL_ENC_K1, 1);
    ui.turn(PANEL_ENC_K2, 7);
    settle(&mut ui);
    let art = (LAB_VIEW_ART_ROW..LAB_VIEW_ART_ROW + LAB_VIEW_ART_ROWS)
        .map(|r| ui.row(r as usize))
        .collect::<Vec<_>>()
        .join("\n");
    for text in ["L1", "L2", "L3", "+7"] {
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
