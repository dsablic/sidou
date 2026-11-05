// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::lab_layers::{card, sound};
use crate::lab_view::{choice, lab_ui, settle, to_page};
use crate::views::Ui;
use crate::{PANEL_ENC_K2, PANEL_ENC_K3};

const PAGE: u32 = LAB_PAGE_LFO;

fn turn(ui: &mut Ui, enc: u32, d: i32) {
    ui.turn(enc, d);
    settle(ui);
}

fn values(ui: &Ui) -> [String; 4] {
    std::array::from_fn(|k| card(ui, PAGE, k as u32).1)
}

fn lfo_bytes(ui: &Ui, n: u32) -> [u8; 3] {
    let at = (LAB_SOUND_LFO + n * LAB_SOUND_LFO_BYTES) as usize;
    sound(ui)[at..at + 3].try_into().unwrap()
}

#[test]
fn lfo_goes_to_mod_and_pressed_again_to_the_lfo_page_and_back() {
    let mut ui = lab_ui();
    for want in [LAB_PAGE_MOD, LAB_PAGE_LFO, LAB_PAGE_MOD] {
        ui.press(PANEL_BTN_LFO);
        settle(&mut ui);
        assert_eq!(u32::from(ui.v.lab.page), want);
    }
}

#[test]
fn algorithm_picks_the_shape_and_a_first_turn_on_is_heard_at_once() {
    let mut ui = lab_ui();
    to_page(&mut ui, PAGE);
    assert_eq!(choice(&ui), "LFO OFF");
    let names = ["TRIANGLE", "SAW UP", "SAW DOWN", "SQUARE", "S&H", "S&H"];
    for (i, name) in names.iter().enumerate() {
        turn(&mut ui, PANEL_ENC_ALGORITHM, 1);
        assert_eq!(choice(&ui), *name);
        if i == 0 {
            let f = lfo_bytes(&ui, 0);
            assert!(f[2] != 0 && f[1] != 0, "a depth and a rate to hear: {f:?}");
        }
    }
    turn(&mut ui, PANEL_ENC_ALGORITHM, -9);
    assert_eq!(choice(&ui), "LFO OFF");
    assert_ne!(lfo_bytes(&ui, 0)[2], 0, "off keeps the depth");
}

#[test]
fn the_cards_set_the_edited_lfos_destination_rate_and_depth() {
    let mut ui = lab_ui();
    to_page(&mut ui, PAGE);
    turn(&mut ui, PANEL_ENC_K1, 1);
    assert_eq!(ui.big(0, 0, 2, 5), "LFO 2");
    turn(&mut ui, PANEL_ENC_ALGORITHM, 4);
    let mut dests = Vec::new();
    for _ in 0..4 {
        dests.push(values(&ui)[1].clone());
        turn(&mut ui, PANEL_ENC_K2, 1);
    }
    assert_eq!(dests, ["PCH", "PW", "CUT", "VOL"]);
    turn(&mut ui, PANEL_ENC_K3, 40);
    assert_eq!(values(&ui)[2], "13", "12.75 Hz at the top");
    turn(&mut ui, PANEL_ENC_K3, -40);
    assert_eq!(values(&ui)[2], ".05");
    turn(&mut ui, PANEL_ENC_K4, -60);
    turn(&mut ui, PANEL_ENC_K4, -60);
    assert_eq!(values(&ui)[3], "-63");
    let f = lfo_bytes(&ui, 1);
    assert_eq!(
        f,
        [
            (LAB_LFO_SQUARE | LAB_LFO_VOLUME << LAB_LFO_DEST_SHIFT) as u8,
            0,
            -63i8 as u8
        ]
    );
    assert_eq!(lfo_bytes(&ui, 0), [0; 3], "LFO 1 untouched");
    for v in values(&ui) {
        assert!(v.len() <= 3, "{v} fits a card at 2x");
    }
}
