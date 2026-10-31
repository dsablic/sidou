// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{screen_animating, top_view, Firmware, Machine};

pub const SELECT: u8 = sidou_sys::PANEL_ENC_SELECT as u8;
pub const ALGORITHM: u8 = sidou_sys::PANEL_ENC_ALGORITHM as u8;
pub const K1: u8 = sidou_sys::PANEL_ENC_K1 as u8;
pub const K4: u8 = sidou_sys::PANEL_ENC_K4 as u8;
pub const PRESETS: u8 = sidou_sys::PANEL_ENC_PRESETS as u8;

pub const OCT_DOWN: u8 = sidou_sys::PANEL_BTN_OCT_DOWN as u8;
pub const OCT_UP: u8 = sidou_sys::PANEL_BTN_OCT_UP as u8;
pub const FX: u8 = sidou_sys::PANEL_BTN_FX as u8;
pub const SCL: u8 = sidou_sys::PANEL_BTN_SCL as u8;
pub const ENV: u8 = sidou_sys::PANEL_BTN_ENV as u8;
pub const LFO: u8 = sidou_sys::PANEL_BTN_LFO as u8;
pub const EDIT: u8 = sidou_sys::PANEL_BTN_EDIT as u8;
pub const GLO: u8 = sidou_sys::PANEL_BTN_GLO as u8;
pub const HOME: u8 = sidou_sys::PANEL_BTN_HOME as u8;
pub const SAVE: u8 = sidou_sys::PANEL_BTN_SAVE as u8;
pub const ARP: u8 = sidou_sys::PANEL_BTN_ARP as u8;
pub const SEQ: u8 = sidou_sys::PANEL_BTN_SEQ as u8;
pub const PLAY: u8 = sidou_sys::PANEL_BTN_PLAY as u8;
pub const REC: u8 = sidou_sys::PANEL_BTN_REC as u8;
pub const KEY0: u8 = sidou_sys::PANEL_KEY_ID0 as u8;

pub const A4: u8 = 69;
pub const A4_KEY: u8 = note_key(A4);

pub const fn note_key(midi_note: u8) -> u8 {
    KEY0 + midi_note - sidou_sys::PANEL_BASE_NOTE as u8
}

pub const MENU_ITEMS: i32 = sidou_sys::VIEWS_MENU_ITEMS as i32;

pub const DEBOUNCED_MS: u64 = 30;
pub const MODIFIER_LEAD_MS: u64 = 10;
pub const DETENT_MS: u64 = 10;
pub const TURN_SETTLE_MS: u64 = 20;
pub const UI_WAIT_MS: u64 = 4_000;

pub fn press(m: &mut Machine, id: u8) {
    m.key(id, true);
    m.run_ms(DEBOUNCED_MS);
    m.key(id, false);
    m.run_ms(DEBOUNCED_MS);
}

pub fn holding(m: &mut Machine, modifier: u8, while_held: impl FnOnce(&mut Machine)) {
    m.key(modifier, true);
    m.run_ms(MODIFIER_LEAD_MS);
    while_held(m);
    m.key(modifier, false);
}

pub fn chord(m: &mut Machine, modifier: u8, id: u8) {
    holding(m, modifier, |m| press(m, id));
}

pub fn turn_time(clicks: i32) -> u64 {
    DETENT_MS * u64::from(clicks.unsigned_abs())
}

pub fn turn(m: &mut Machine, enc: u8, clicks: i32) {
    m.turn(enc, clicks);
    m.run_ms(turn_time(clicks) + TURN_SETTLE_MS);
}

pub fn wait_for_within(
    m: &mut Machine,
    within_ms: u64,
    what: &str,
    mut done: impl FnMut(&mut Machine) -> bool,
) {
    let start = m.ms();
    while !done(m) {
        assert!(m.ms() - start < within_ms, "timed out: {what}");
        m.run_ms(1);
    }
}

pub fn wait_for(m: &mut Machine, what: &str, done: impl FnMut(&mut Machine) -> bool) {
    wait_for_within(m, UI_WAIT_MS, what, done);
}

pub fn wait_for_view(m: &mut Machine, fw: &Firmware, view: &str) {
    let want = fw.symbol(view);
    wait_for(m, view, |m| {
        top_view(m, fw) == want && !screen_animating(m, fw)
    });
}

pub fn open_menu(m: &mut Machine, fw: &Firmware) {
    if top_view(m, fw) != fw.symbol("views_menu") {
        press(m, HOME);
        wait_for(m, "the menu", |m| {
            top_view(m, fw) == fw.symbol("views_menu")
        });
    }
}

pub fn select_menu_item(m: &mut Machine, item: i32) {
    m.turn(SELECT, -MENU_ITEMS);
    m.run_ms(turn_time(MENU_ITEMS));
    m.turn(SELECT, item);
    m.run_ms(turn_time(item));
}
