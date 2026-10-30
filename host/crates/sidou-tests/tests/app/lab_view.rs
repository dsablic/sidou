// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! LAB (app/lab_view.c) over the views (app/views.c): opened from the menu's LAB, its five pages
//! (SELECT), the four cards each page turns by its detents, ALGORITHM's choice on each page, the
//! card or choice lit in place of a popup, the header, the hint line, ENV, LFO and FX as pages,
//! HOME back to LIVE, the frame's budget and the pages' goldens.

use std::ffi::CStr;

use sidou_sys::*;

use crate::sid_view::golden;
use crate::store::{fresh, Rig as Store};
use crate::views::Ui;
use crate::{drain, Sent};

const K1: u32 = PANEL_ENC_K1;
/// app/panel.h PANEL_PAGE_LAB_FIRST: after the EXPERT pages.
const LAB_FIRST: u32 = PANEL_PAGE_EXPERT_FIRST + SID_EXPERT_COUNT.div_ceil(4);

fn step(ui: &mut Ui) -> Sent {
    ui.frame();
    drain(&mut ui.g)
}

/// Frames, each sent, until no transition runs, no popup shows and the page is whole.
pub(crate) fn settle(ui: &mut Ui) {
    for _ in 0..200 {
        step(ui);
        if unsafe { screen_animating(&ui.v.s) } == 0
            && unsafe { screen_popup_shows(&ui.v.s) } == 0
            && ui.v.lab.copying == 0
            && ui.v.lab.hl == 0
            && (ui.top() != "LAB" || ui.v.lab.art_done == 1)
        {
            step(ui);
            return;
        }
    }
    panic!("the screen kept changing");
}

/// LAB from the menu: HOME, SELECT to LAB, ALGORITHM clockwise; settled.
pub(crate) fn open_lab(ui: &mut Ui) {
    ui.press(PANEL_BTN_HOME);
    settle(ui);
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_LAB as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(ui);
    assert_eq!(ui.top(), "LAB");
}

pub(crate) fn lab_ui() -> Ui {
    let mut ui = Ui::without_cards();
    open_lab(&mut ui);
    ui
}

/// SELECT to page `page` from wherever LAB is.
pub(crate) fn to_page(ui: &mut Ui, page: u32) {
    ui.turn(PANEL_ENC_SELECT, -(LAB_PAGES as i32));
    ui.turn(PANEL_ENC_SELECT, page as i32);
    settle(ui);
    assert_eq!(u32::from(ui.v.lab.page), page);
}

fn param(ui: &Ui, p: u32) -> u32 {
    unsafe { live_param_get(&*ui.rig.live, p) }
}

fn set(ui: &mut Ui, p: u32, v: u32) {
    unsafe { live_param_set(&mut *ui.rig.live, p, v) };
}

/// The choice as the page shows it.
fn choice(ui: &Ui) -> String {
    let mut out = [0 as std::os::raw::c_char; LAB_VIEW_TEXT as usize];
    unsafe { lab_view_choice(&ui.v.lab, u32::from(ui.v.lab.page), out.as_mut_ptr()) };
    unsafe { CStr::from_ptr(out.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[test]
fn select_steps_the_six_pages_and_stops_at_the_ends() {
    let mut ui = lab_ui();
    let names = ["OSC", "ENVELOPE", "FILTER", "MOD", "FX", "MACRO"];
    assert_eq!(ui.v.lab.page, 0, "LAB opens at OSC");
    for (i, name) in names.iter().enumerate() {
        settle(&mut ui);
        assert_eq!(usize::from(ui.v.lab.page), i);
        assert_eq!(ui.big(0, 0, 2, name.len()), *name, "the page's name at 2x");
        assert_eq!(ui.big(0, 24, 2, 3), format!("{}/6", i + 1), "its place");
        ui.turn(PANEL_ENC_SELECT, 1);
    }
    assert_eq!(u32::from(ui.v.lab.page), LAB_PAGE_MACRO, "clamped at MACRO");
    ui.turn(PANEL_ENC_SELECT, -9);
    assert_eq!(ui.v.lab.page, 0, "clamped at OSC");
}

/// Each card, a detent each way: its parameter moves by the card's detent (from a value with room
/// both ways), and back.
#[test]
fn each_card_turns_its_parameter_by_its_detent() {
    // (page, knob, parameter, a start, the detent)
    let table: [(u32, u32, u32, u32, i32); 17] = [
        (LAB_PAGE_OSC, 1, LIVE_PARAM_PWM_DEPTH, 640, 32),
        (LAB_PAGE_OSC, 2, LIVE_PARAM_PWM_RATE, 10, 1),
        (LAB_PAGE_OSC, 3, LIVE_PARAM_TUNE, 50, 1),
        (LAB_PAGE_ENVELOPE, 0, LIVE_PARAM_ATTACK, 4, 1),
        (LAB_PAGE_ENVELOPE, 1, LIVE_PARAM_DECAY, 4, 1),
        (LAB_PAGE_ENVELOPE, 2, LIVE_PARAM_SUSTAIN, 4, 1),
        (LAB_PAGE_ENVELOPE, 3, LIVE_PARAM_RELEASE, 4, 1),
        (LAB_PAGE_FILTER, 1, LIVE_PARAM_RESONANCE, 6, 1),
        (LAB_PAGE_FILTER, 2, LIVE_PARAM_FILTER_VOICES, 3, 1),
        (LAB_PAGE_FILTER, 3, LIVE_PARAM_FILTER_LFO, 8, 1),
        (LAB_PAGE_MOD, 0, LIVE_PARAM_VIBRATO_RATE, 60, 1),
        (LAB_PAGE_MOD, 1, LIVE_PARAM_VIBRATO_DEPTH, 20, 1),
        (LAB_PAGE_MOD, 2, LIVE_PARAM_VIBRATO_DELAY, 20, 1),
        (LAB_PAGE_MOD, 3, LIVE_PARAM_ARP_RATE, 3, 1),
        (
            LAB_PAGE_FX,
            0,
            LIVE_PARAM_SLOT + LIVE_SLOT_AMOUNT,
            60,
            LIVE_SLOT_DETENT as i32,
        ),
        (
            LAB_PAGE_FX,
            1,
            LIVE_PARAM_SLOT + LIVE_SLOT_A,
            60,
            LIVE_SLOT_DETENT as i32,
        ),
        (
            LAB_PAGE_FX,
            3,
            LIVE_PARAM_SLOT + LIVE_SLOT_MIX,
            60,
            LIVE_SLOT_DETENT as i32,
        ),
    ];
    let mut ui = lab_ui();
    for (page, k, p, start, detent) in table {
        to_page(&mut ui, page);
        set(&mut ui, p, start);
        ui.turn(K1 + k, 1);
        assert_eq!(
            param(&ui, p) as i32,
            start as i32 + detent,
            "page {page} K{}: one detent",
            k + 1
        );
        ui.turn(K1 + k, -2);
        assert_eq!(param(&ui, p) as i32, start as i32 - detent, "and back two");
        // panel_knob_param names the card's parameter on LAB's panel page (learn's pick sets
        // it), EDIT held or not.
        let mut on_page = *ui.p;
        on_page.page = (LAB_FIRST + page) as u8;
        assert_eq!(unsafe { lab_view_panel_page(&ui.v.lab) }, LAB_FIRST + page);
        for held in [0, 1 << PANEL_BTN_EDIT] {
            assert_eq!(
                unsafe { panel_knob_param(&on_page, &*ui.rig.live, held, K1 + k) },
                p,
                "page {page} K{} names its parameter",
                k + 1
            );
        }
    }
    // EDIT held turns the card as without it: no hidden combination.
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    set(&mut ui, LIVE_PARAM_ATTACK, 4);
    ui.hold(PANEL_BTN_EDIT, true);
    ui.turn(K1, 1);
    ui.hold(PANEL_BTN_EDIT, false);
    assert_eq!(
        param(&ui, LIVE_PARAM_ATTACK),
        5,
        "EDIT + K1 on ENVELOPE: the attack"
    );
    // OSC's K1: LIVE's pulse walk (64 a detent); the cutoff and resonance as LIVE's K1 and K2.
    to_page(&mut ui, LAB_PAGE_OSC);
    set(&mut ui, LIVE_PARAM_PULSE_WIDTH, 1024);
    ui.turn(K1, 1);
    assert_eq!(param(&ui, LIVE_PARAM_PULSE_WIDTH), 1024 + 64);
    to_page(&mut ui, LAB_PAGE_FILTER);
    let before = param(&ui, LIVE_PARAM_CUTOFF);
    ui.turn(K1, 1);
    assert!(param(&ui, LIVE_PARAM_CUTOFF) > before, "the cutoff walk");
}

/// FILTER's K4, the filter LFO: its depth in K1's detents, the rate it shares with PWM HZ on its
/// name line; turned up from off on a sound whose LFO has no rate (SAW LEAD), the rate starts
/// at 1 Hz, so the first detent is heard.
#[test]
fn filter_k4_is_the_filter_lfo_with_its_rate_on_the_name_line() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER);
    assert_eq!(param(&ui, LIVE_PARAM_PWM_RATE), 0, "SAW LEAD: no LFO rate");
    let row = LAB_VIEW_LABEL_ROW as usize;
    assert_eq!(ui.small(row, 22, 8), "LFO 0HZ ");
    assert_eq!(ui.big(LAB_VIEW_VALUE_ROW as usize, 22, 3, 1), "0", "off");
    ui.turn(K1 + 3, 1);
    assert_eq!(param(&ui, LIVE_PARAM_FILTER_LFO), 1);
    assert_eq!(param(&ui, LIVE_PARAM_PWM_RATE), 20, "1 Hz to start");
    settle(&mut ui);
    assert_eq!(ui.small(row, 22, 8), "LFO1.0HZ");
    assert_eq!(ui.big(LAB_VIEW_VALUE_ROW as usize, 22, 3, 1), "1", "the depth");
    set(&mut ui, LIVE_PARAM_PWM_RATE, 25);
    settle(&mut ui);
    assert_eq!(ui.small(row, 22, 8), "LFO1.3HZ", "the shared rate, as PWM HZ says it");
    // turned back to off and up again, the rate the player set stays
    ui.turn(K1 + 3, -1);
    ui.turn(K1 + 3, 1);
    assert_eq!(param(&ui, LIVE_PARAM_PWM_RATE), 25);
    // the cards' text for the other pages' LFO: OSC's PWM HZ moves with it
    to_page(&mut ui, LAB_PAGE_OSC);
    assert_eq!(ui.small(row, 15, 6), "PWM HZ");
}

/// MOD's K4 is the arpeggio's RATE, as LIVE's ARP + K1 turns it: frames of 50 Hz (`3FR`), then
/// the clock's notes (`1/32`, `1/16`, `1/8`); dark grey while ARP is off, and turned all the same.
#[test]
fn mod_k4_is_the_arp_rate_dark_while_arp_is_off() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_MOD);
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    let (row, val) = (LAB_VIEW_LABEL_ROW as usize, LAB_VIEW_VALUE_ROW as usize + 1);
    assert_eq!(ui.small(row, 22, 4), "RATE");
    assert_eq!(ui.big(val, 22, 2, 3), "3FR", "SAW LEAD's default rate");
    assert_eq!(ui.g.attr[row * GRID_COLS as usize + 22] & 15, th.dim, "ARP off: dark");
    ui.turn(K1 + 3, 4);
    assert_eq!(u32::from(ui.live().arp_rate), LIVE_ARP_RATE_CLOCK, "settable while off");
    settle(&mut ui);
    assert_eq!(ui.big(val, 22, 2, 4), "1/32");
    ui.turn(K1 + 3, 2);
    settle(&mut ui);
    assert_eq!(ui.big(val, 22, 2, 3), "1/8");
    ui.turn(PANEL_ENC_ALGORITHM, 1); // ARP UP
    settle(&mut ui);
    assert_ne!(ui.g.attr[row * GRID_COLS as usize + 22] & 15, th.dim, "ARP on: lit");
    ui.turn(K1 + 3, -7);
    assert_eq!(ui.live().arp_rate, 1);
    settle(&mut ui);
    assert_eq!(ui.big(val, 22, 2, 3), "2FR");
}

/// MACRO (6/6): K1 RESO LNK and K2 PWM LNK, -8..+8 a detent at a time (signed: `+3`, `-8`), K3
/// and K4 dark and turning nothing; ALGORITHM has no choice (LINKED or NO LINKS). The drawing's
/// two graphs follow the links (a golden with both set).
#[test]
fn macro_turns_the_links_and_k3_k4_are_dark() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_MACRO);
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    let row = LAB_VIEW_LABEL_ROW as usize;
    assert_eq!(ui.small(row, 0, 7), "RES LNK");
    assert_eq!(ui.small(row, 8, 7), "PWM LNK");
    assert_eq!(choice(&ui), "NO LINKS");
    ui.turn(K1, 3);
    ui.turn(K1 + 1, -9);
    assert_eq!(param(&ui, LIVE_PARAM_RESO_LINK), LIVE_LINK_MAX as u32 + 3);
    assert_eq!(param(&ui, LIVE_PARAM_PWM_LINK), 0, "clamped at -8");
    settle(&mut ui);
    assert_eq!(ui.big(LAB_VIEW_VALUE_ROW as usize, 0, 3, 2), "+3");
    assert_eq!(ui.big(LAB_VIEW_VALUE_ROW as usize, 8, 3, 2), "-8");
    assert_eq!(choice(&ui), "LINKED");
    let snapshot = (0..LIVE_PARAM_COUNT).map(|p| param(&ui, p)).collect::<Vec<_>>();
    ui.turn(K1 + 2, 3);
    ui.turn(K1 + 3, 3);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(
        (0..LIVE_PARAM_COUNT).map(|p| param(&ui, p)).collect::<Vec<_>>(),
        snapshot,
        "K3, K4 and ALGORITHM turn nothing"
    );
    settle(&mut ui);
    let value = LAB_VIEW_VALUE_ROW as usize;
    for col in [15usize, 22] {
        assert_eq!(ui.g.attr[value * GRID_COLS as usize + col] & 15, th.dim, "col {col} dark");
    }
    ui.turn(K1 + 1, 12); // PWM LNK +4
    settle_hint(&mut ui);
    golden(&ui, "lab-macro-linked");
}

/// RESO LINK in LAB: FILTER's K1 turned, the resonance the link moves shows on the RESO card
/// (what the chip has) and the card lights with K1's; OSC's K1 and PWM LINK the same with the PWM
/// card.
#[test]
fn a_link_lights_the_card_it_moves() {
    let mut ui = lab_ui();
    set(&mut ui, LIVE_PARAM_RESO_LINK, LIVE_LINK_MAX as u32 + 8);
    let mid = unsafe { live_cutoff_steps[ui.live().model as usize][32] };
    set(&mut ui, LIVE_PARAM_CUTOFF, u32::from(mid)); // the walk's middle: the link adds nothing
    unsafe { live_tick(&mut *ui.rig.live) }; // the linked resonance follows at the tick
    to_page(&mut ui, LAB_PAGE_FILTER);
    let res = ui.live().res_out;
    assert_eq!(u32::from(res), param(&ui, LIVE_PARAM_RESONANCE));
    ui.turn(K1, -4);
    assert_ne!(ui.live().res_out, res, "the link moved it");
    assert_eq!(ui.v.lab.hl_link, 2, "RESO lights");
    for _ in 0..4 {
        ui.frame(); // the page built and copied, well within the light's second
    }
    let row = LAB_VIEW_LABEL_ROW as usize;
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    assert_eq!(ui.g.attr[row * GRID_COLS as usize + 8] >> 4, th.knob_text[1], "reversed");
    let shown: u32 = ui.big(LAB_VIEW_VALUE_ROW as usize, 8, 3, 2).trim().parse().unwrap();
    assert_eq!(shown, u32::from(ui.live().res_out), "the resonance the chip has");
    settle(&mut ui);
    assert_eq!(ui.v.lab.hl_link, 0, "and goes out with it");
    // K2 with no link moving anything lights itself alone
    set(&mut ui, LIVE_PARAM_RESO_LINK, LIVE_LINK_MAX as u32);
    ui.turn(K1, 1);
    assert_eq!(ui.v.lab.hl_link, 0);
    // OSC: PULSE moves PWM LINK's depth
    set(&mut ui, LIVE_PARAM_PWM_LINK, LIVE_LINK_MAX as u32 + 4);
    set(&mut ui, LIVE_PARAM_PWM_DEPTH, 64);
    to_page(&mut ui, LAB_PAGE_OSC);
    ui.turn(K1, -3);
    assert_eq!(ui.v.lab.hl_link, 2, "PWM lights");
}

/// On SYNC LEAD (a mono-sync saw) OSC's K1 reads RATIO and turns the sync ratio's semitones.
#[test]
fn osc_k1_is_the_ratio_on_a_mono_sync_sound_without_pulse() {
    let mut ui = Ui::without_cards();
    unsafe { live_set_preset(&mut *ui.rig.live, 2) };
    open_lab(&mut ui);
    assert_eq!(ui.small(LAB_VIEW_LABEL_ROW as usize, 0, 5), "RATIO");
    ui.turn(K1, 2);
    assert_eq!(ui.live().sync_semis, 2);
    settle(&mut ui);
    assert_eq!(ui.big(LAB_VIEW_VALUE_ROW as usize, 0, 3, 2), "+2");
}

#[test]
fn algorithm_steps_each_pages_choice() {
    let mut ui = lab_ui();
    // OSC: the waveforms, the combined ones, then ring and sync, clamped at both ends.
    let osc = [
        "TRIANGLE",
        "SAW",
        "PULSE",
        "NOISE",
        "TRI+SAW",
        "TRI+PULSE",
        "SAW+PULSE",
        "TRI+SAW+PUL",
        "TRI RING",
        "SAW SYNC",
        "PULSE SYNC",
    ];
    ui.turn(PANEL_ENC_ALGORITHM, -20);
    for (i, name) in osc.iter().enumerate() {
        assert_eq!(choice(&ui), *name, "OSC choice {i}");
        assert_eq!(
            ui.live().control & 0xF6,
            unsafe { lab_view_wave_control(i as u32) },
            "{name}'s bits"
        );
        ui.turn(PANEL_ENC_ALGORITHM, 1);
    }
    assert_eq!(choice(&ui), "PULSE SYNC", "clamped");
    // ENVELOPE: the instrument's envelope mode.
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(
        (choice(&ui), ui.live().envelope_mode),
        ("AUTHENTIC".into(), 1)
    );
    ui.turn(PANEL_ENC_ALGORITHM, -3);
    assert_eq!(choice(&ui), "INSTANT");
    // FILTER: LP, BP, HP, NOTCH; from a mode no choice has, the next one the turn's way.
    to_page(&mut ui, LAB_PAGE_FILTER);
    ui.turn(PANEL_ENC_ALGORITHM, -9);
    for name in ["LP", "BP", "HP", "NOTCH"] {
        assert_eq!(choice(&ui), name);
        ui.turn(PANEL_ENC_ALGORITHM, 1);
    }
    assert_eq!(choice(&ui), "NOTCH", "clamped");
    set(&mut ui, LIVE_PARAM_FILTER_MODE, 3); // LP+BP over MIDI
    assert_eq!(choice(&ui), "LP+BP");
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(choice(&ui), "HP");
    set(&mut ui, LIVE_PARAM_FILTER_MODE, 3);
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(choice(&ui), "BP");
    set(&mut ui, LIVE_PARAM_FILTER_MODE, 0);
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(choice(&ui), "OFF", "nothing below OFF");
    // MOD: ARP off, then its orders.
    to_page(&mut ui, LAB_PAGE_MOD);
    for (i, name) in ["ARP OFF", "ARP UP", "ARP DOWN", "ARP UP-DOWN", "ARP PLAYED"]
        .iter()
        .enumerate()
    {
        assert_eq!(choice(&ui), *name);
        assert_eq!(ui.live().arp_on, u8::from(i > 0));
        if i > 0 {
            assert_eq!(u32::from(ui.live().arp_order), i as u32 - 1);
        }
        ui.turn(PANEL_ENC_ALGORITHM, 1);
    }
    ui.turn(PANEL_ENC_ALGORITHM, -9);
    assert_eq!((choice(&ui), ui.live().arp_on), ("ARP OFF".into(), 0));
    // FX: the selected slot's algorithms, OFF included.
    to_page(&mut ui, LAB_PAGE_FX);
    for name in ["OFF", "TAPE", "OVERDRIVE", "FUZZ", "CRUSH", "LO-FI"] {
        assert_eq!(choice(&ui), name);
        ui.turn(PANEL_ENC_ALGORITHM, 1);
    }
    ui.press(PANEL_BTN_SCL); // the next slot, as everywhere
    assert_eq!(ui.live().fx_select, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(choice(&ui), "CHORUS");
}

#[test]
fn a_turn_lights_its_card_and_opens_no_popup() {
    // The menu's ALGORITHM that opens LAB is the menu's: LAB's choice is not lit by it.
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_LAB as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    step(&mut ui);
    step(&mut ui);
    assert_eq!((ui.top().as_str(), ui.v.lab.hl), ("LAB", 0));
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    let row = LAB_VIEW_LABEL_ROW as usize;
    for k in 0..4u32 {
        ui.turn(K1 + k, 1);
        step(&mut ui);
        step(&mut ui);
        assert_eq!(
            unsafe { screen_popup_shows(&ui.v.s) },
            0,
            "K{}: no popup",
            k + 1
        );
        assert_eq!(u32::from(ui.v.lab.hl), k + 1);
        let col = [0usize, 8, 15, 22][k as usize];
        let attr = ui.g.attr[row * GRID_COLS as usize + col];
        assert_eq!(
            attr,
            ((th.knob_text[k as usize] << 4) | th.bg) as u8,
            "K{}'s name reversed",
            k + 1
        );
        settle(&mut ui);
        assert_eq!(
            ui.g.attr[row * GRID_COLS as usize + col],
            ((th.bg << 4) | th.knob_text[k as usize]) as u8,
            "and back"
        );
    }
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    step(&mut ui);
    step(&mut ui);
    assert_eq!(
        unsafe { screen_popup_shows(&ui.v.s) },
        0,
        "ALGORITHM: no popup"
    );
    assert_eq!(u32::from(ui.v.lab.hl), LAB_VIEW_HL_CHOICE);
    let at = LAB_VIEW_CHOICE_ROW as usize * GRID_COLS as usize;
    assert_eq!(
        ui.g.attr[at],
        ((th.value << 4) | th.bg) as u8,
        "the choice reversed"
    );
    // PRESETS: the header names the sound, no popup.
    ui.turn(PANEL_ENC_PRESETS, 1);
    step(&mut ui);
    step(&mut ui);
    assert_eq!(
        unsafe { screen_popup_shows(&ui.v.s) },
        0,
        "PRESETS: no popup"
    );
    settle(&mut ui);
    assert_eq!(ui.row(LAB_VIEW_NAME_ROW as usize).trim_end(), "PWM BASS");
}

#[test]
fn the_header_names_the_page_the_sound_and_its_star() {
    let mut ui = lab_ui();
    assert_eq!(ui.big(0, 0, 2, 3), "OSC");
    let name = |ui: &Ui| ui.row(LAB_VIEW_NAME_ROW as usize).trim_end().to_string();
    assert_eq!(name(&ui), "SAW LEAD", "unedited: no star");
    ui.turn(K1 + 3, 1); // the fine tune
    settle(&mut ui);
    assert_eq!(name(&ui), "SAW LEAD*", "edited");
    ui.turn(K1 + 3, -1);
    settle(&mut ui);
    assert_eq!(name(&ui), "SAW LEAD", "back to the base");
}

#[test]
fn the_hint_line_fits_and_says_every_lab_gesture() {
    let phrases: Vec<String> = (0..LAB_VIEW_HINTS)
        .map(|n| {
            unsafe { CStr::from_ptr(lab_view_hint(n)) }
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    for p in &phrases {
        assert!(p.len() <= GRID_COLS as usize, "{p:?} is over a row");
    }
    let all = phrases.join(" ");
    for gesture in ["A/B", "UNDO", "VARY", "ROLL", "NAME", "KEEP", "PAGE"] {
        assert!(all.contains(gesture), "the hint line never says {gesture}");
    }
    // On the screen, a phrase at a time, LAB_VIEW_HINT_MS each, never truncated.
    let mut ui = lab_ui();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..(3 * LAB_VIEW_HINTS * LAB_VIEW_HINT_MS / 40) {
        step(&mut ui);
        let row = ui.row(LAB_VIEW_HINT_ROW as usize);
        seen.insert(row.trim_end().to_string());
    }
    for p in &phrases {
        assert!(seen.contains(p.as_str()), "{p:?} never showed: {seen:?}");
    }
}

#[test]
fn env_lfo_and_fx_open_their_page_in_lab() {
    let mut ui = lab_ui();
    let algorithm = ui.live().slot[0][LIVE_SLOT_ALGORITHM as usize];
    let model = ui.live().model_req;
    for (b, page) in [
        (PANEL_BTN_ENV, LAB_PAGE_ENVELOPE),
        (PANEL_BTN_LFO, LAB_PAGE_MOD),
        (PANEL_BTN_FX, LAB_PAGE_FX),
        (PANEL_BTN_ENV, LAB_PAGE_ENVELOPE),
    ] {
        ui.press(b);
        assert_eq!(u32::from(ui.v.lab.page), page);
    }
    // Neither the slot's algorithm nor the chip model moved (the panel has neither since LIVE's
    // change).
    assert_eq!(ui.live().slot[0][LIVE_SLOT_ALGORITHM as usize], algorithm);
    assert_eq!(ui.live().model_req, model);
}

/// LIVE's one change: ENV, LFO and FX open LAB over LIVE at ENVELOPE, MOD and FX, from TONE or a
/// slot's page; in the menu they do nothing (nor the model, nor the slot's algorithm).
#[test]
fn env_lfo_and_fx_on_live_open_lab_at_their_page() {
    let mut ui = Ui::without_cards();
    settle(&mut ui);
    let algorithm = ui.live().slot[0][LIVE_SLOT_ALGORITHM as usize];
    let model = ui.live().model_req;
    for (b, page, live_page) in [
        (PANEL_BTN_ENV, LAB_PAGE_ENVELOPE, 0),
        (PANEL_BTN_LFO, LAB_PAGE_MOD, 0),
        (PANEL_BTN_FX, LAB_PAGE_FX, 1),
    ] {
        ui.turn(PANEL_ENC_SELECT, live_page);
        settle(&mut ui);
        assert_eq!(ui.top(), "LIVE");
        ui.press(b);
        settle(&mut ui);
        assert_eq!(ui.top(), "LAB", "button {b}");
        assert_eq!(u32::from(ui.v.lab.page), page, "button {b}");
        assert_eq!(ui.v.s.depth, 2, "LAB over LIVE");
        ui.press(PANEL_BTN_HOME);
        settle(&mut ui);
        ui.turn(PANEL_ENC_SELECT, -live_page);
        settle(&mut ui);
    }
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "MENU");
    for b in [PANEL_BTN_ENV, PANEL_BTN_LFO, PANEL_BTN_FX] {
        ui.press(b);
        settle(&mut ui);
        assert_eq!(ui.top(), "MENU", "button {b} in the menu");
    }
    assert_eq!(ui.live().slot[0][LIVE_SLOT_ALGORITHM as usize], algorithm);
    assert_eq!(ui.live().model_req, model);
}

#[test]
fn home_goes_back_to_live_from_lab_and_from_the_menus_lab() {
    let mut ui = lab_ui();
    assert_eq!(ui.v.s.depth, 2, "LAB replaced the menu: LIVE, LAB");
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "LIVE");
    // SAVE as the menu's OK opens it too.
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_LAB as i32);
    ui.press(PANEL_BTN_SAVE);
    settle(&mut ui);
    assert_eq!((ui.top().as_str(), ui.v.s.depth), ("LAB", 2));
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "LIVE");
}

/// Opening LAB, every page, the cards and choices turned and a burst of MIDI: no frame sends more
/// than LAB_VIEW_FRAME_CELLS.
#[test]
fn the_frame_stays_within_200_cells() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_LAB as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    let mut most = 0;
    let mut frame = |ui: &mut Ui| {
        let s = step(ui);
        most = most.max(s.cells);
    };
    for _ in 0..30 {
        frame(&mut ui);
    }
    for page in 0..LAB_PAGES {
        ui.turn(PANEL_ENC_SELECT, if page == 0 { 0 } else { 1 });
        for _ in 0..3 {
            frame(&mut ui);
        }
        for k in 0..4 {
            ui.turn(K1 + k, 5);
            frame(&mut ui);
        }
        ui.turn(PANEL_ENC_ALGORITHM, 3);
        frame(&mut ui);
        ui.turn(PANEL_ENC_PRESETS, 1);
        for _ in 0..30 {
            frame(&mut ui);
        }
    }
    assert!(
        most <= LAB_VIEW_FRAME_CELLS as usize,
        "a frame sent {most} cells"
    );
}

/// The six pages on SAW LEAD as it loads (FX with OVERDRIVE on the first slot).
#[test]
fn the_pages_match_their_goldens() {
    let mut ui = lab_ui();
    for (page, name) in [
        (LAB_PAGE_OSC, "lab-osc"),
        (LAB_PAGE_ENVELOPE, "lab-envelope"),
        (LAB_PAGE_FILTER, "lab-filter"),
        (LAB_PAGE_MOD, "lab-mod"),
        (LAB_PAGE_FX, "lab-fx"),
        (LAB_PAGE_MACRO, "lab-macro"),
    ] {
        to_page(&mut ui, page);
        if page == LAB_PAGE_FX {
            ui.turn(PANEL_ENC_ALGORITHM, 2);
            settle(&mut ui);
        }
        settle_hint(&mut ui);
        golden(&ui, name);
    }
}

/// Settled, then on to the hint line's first phrase, drawn.
pub(crate) fn settle_hint(ui: &mut Ui) {
    settle(ui);
    for _ in 0..(LAB_VIEW_HINTS * LAB_VIEW_HINT_MS / 40 + 2) {
        if ui.v.s.now % (LAB_VIEW_HINTS * LAB_VIEW_HINT_MS) < 100 {
            break;
        }
        step(ui);
    }
    for _ in 0..4 {
        step(ui);
    }
    assert!(ui.row(LAB_VIEW_HINT_ROW as usize).starts_with("EDIT A/B"));
}

// ---- the drawings (Task 6 on) ----

const ROW0: usize = LAB_VIEW_ART_ROW as usize;
const COLS: usize = GRID_COLS as usize;

/// The overlay of grid cell (col, row): its mask and colour, or None.
fn overlay(g: &grid, col: usize, row: usize) -> Option<([u8; 8], u8)> {
    let slot = usize::from(g.ovl[row * COLS + col]);
    (slot != 0).then(|| (g.ovl_mask[slot - 1], g.ovl_color[slot - 1]))
}

/// The area's inked pixels as rows of 240 booleans, from the overlays.
fn ink(g: &grid) -> Vec<Vec<bool>> {
    let mut out = vec![vec![false; LAB_DRAW_W as usize]; LAB_DRAW_H as usize];
    for r in 0..LAB_DRAW_ROWS as usize {
        for c in 0..COLS {
            if let Some((m, _)) = overlay(g, c, ROW0 + r) {
                for (y, bits) in m.iter().enumerate() {
                    for j in 0..8 {
                        if bits >> (7 - j) & 1 != 0 {
                            out[r * 8 + y][c * 8 + j] = true;
                        }
                    }
                }
            }
        }
    }
    out
}

/// The cells of the area whose overlay is in colour `c`.
fn cells_in(g: &grid, c: u8) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for r in 0..LAB_DRAW_ROWS as usize {
        for col in 0..COLS {
            if overlay(g, col, ROW0 + r).is_some_and(|(_, k)| k == c) {
                out.push((col, r));
            }
        }
    }
    out
}

unsafe extern "C" fn synthetic(ctx: *const std::ffi::c_void, x: u32, out: *mut lab_draw_col) {
    let kind = unsafe { *(ctx as *const u32) };
    let y = synthetic_y(kind, x);
    let out = unsafe { &mut *out };
    out.lo = [y, LAB_DRAW_NONE as u8, LAB_DRAW_NONE as u8];
    out.hi = [y, 0, 0];
    out.lit = 0;
}

fn synthetic_y(kind: u32, x: u32) -> u8 {
    (match kind {
        0 => x * (LAB_DRAW_H - 2) / (LAB_DRAW_W - 1), // a diagonal
        1 => {
            if x < 120 {
                20
            } else {
                80
            }
        } // a step
        _ => 50,                                      // a flat line
    }) as u8
}

/// The kit's overlays against a reference rasteriser: each column's point joined to the one
/// before and a pixel thicker below, on the cells it crosses only.
#[test]
fn the_kit_draws_a_line_as_overlays_on_the_cells_it_crosses() {
    for kind in 0..3u32 {
        let mut g = Box::<grid>::default();
        let s = Box::<screen>::default();
        unsafe { grid_init(&mut *g, b' ', 0) };
        let d = lab_draw {
            fn_: Some(synthetic),
            ctx: (&kind as *const u32).cast(),
            color: [GRID_WHITE as u8, GRID_CYAN as u8, GRID_YELLOW as u8],
            lit_color: GRID_RED as u8,
        };
        let mut left = 1000;
        let whole = unsafe { lab_draw_apply(&d, &mut *g, &*s, ROW0 as u32, &mut left) };
        assert_eq!(whole, 1, "kind {kind}: drawn whole");
        // A sample every LAB_DRAW_STEP columns, as wide, joined to the one before.
        let mut want = vec![vec![false; LAB_DRAW_W as usize]; LAB_DRAW_H as usize];
        let mut prev: Option<u8> = None;
        for x in (0..LAB_DRAW_W).step_by(LAB_DRAW_STEP as usize) {
            let y = synthetic_y(kind, x);
            let (a, b) = match prev {
                Some(p) => (y.min(p), y.max(p)),
                None => (y, y),
            };
            prev = Some(y);
            let b = (u32::from(b) + 1).min(LAB_DRAW_H - 1) as u8;
            for row in a..=b {
                for dx in 0..LAB_DRAW_STEP {
                    want[usize::from(row)][(x + dx) as usize] = true;
                }
            }
        }
        assert_eq!(ink(&g), want, "kind {kind}: the pixels");
        let crossed: usize = (0..LAB_DRAW_ROWS as usize)
            .map(|r| {
                (0..COLS)
                    .filter(|&c| (0..8).any(|y| (0..8).any(|j| want[r * 8 + y][c * 8 + j])))
                    .count()
            })
            .sum();
        assert_eq!(
            cells_in(&g, GRID_WHITE as u8).len(),
            crossed,
            "kind {kind}: only the cells it crosses"
        );
        assert_eq!(1000 - left, crossed as u32, "each counted once");
        // Again: the same drawing changes nothing.
        let mut again = 1000;
        unsafe { lab_draw_apply(&d, &mut *g, &*s, ROW0 as u32, &mut again) };
        assert_eq!(again, 1000, "kind {kind}: nothing changed");
        // A budget of 5: five cells, then the rest at the next call.
        let mut g2 = Box::<grid>::default();
        unsafe { grid_init(&mut *g2, b' ', 0) };
        let mut five = 5;
        let first = unsafe { lab_draw_apply(&d, &mut *g2, &*s, ROW0 as u32, &mut five) };
        assert_eq!((first, five), (0, 0), "kind {kind}: cut short");
        let mut rest = 1000;
        assert_eq!(
            unsafe { lab_draw_apply(&d, &mut *g2, &*s, ROW0 as u32, &mut rest) },
            1
        );
        assert_eq!(ink(&g2), want, "kind {kind}: the rest, the next call");
    }
}

/// OSC's drawing for each waveform (SAW LEAD's other settings): goldens.
#[test]
fn osc_draws_each_waveform() {
    let mut ui = lab_ui();
    let choose = |ui: &mut Ui, i: i32| {
        ui.turn(PANEL_ENC_ALGORITHM, -20);
        ui.turn(PANEL_ENC_ALGORITHM, i);
    };
    for (i, name) in [
        (0, "lab-osc-tri"),
        (1, "lab-osc-saw"),
        (2, "lab-osc-pul25"),
        (3, "lab-osc-noise"),
        (9, "lab-osc-sync"),
        (8, "lab-osc-ring"),
    ] {
        choose(&mut ui, i);
        if i == 2 {
            set(&mut ui, LIVE_PARAM_PULSE_WIDTH, 1024);
        }
        settle_hint(&mut ui);
        golden(&ui, name);
    }
    // PWM BASS: the PWM's band behind the pulse's edge.
    let mut ui = Ui::without_cards();
    unsafe { live_set_preset(&mut *ui.rig.live, 1) };
    open_lab(&mut ui);
    settle_hint(&mut ui);
    golden(&ui, "lab-osc-pwm");
    let band = (ROW0..ROW0 + 13)
        .flat_map(|r| (0..COLS).map(move |c| (r, c)))
        .filter(|&(r, c)| ui.g.ch[r * COLS + c] == FONT_D2 as u8)
        .count();
    assert!(band > 0, "the band");
}

/// The envelope to scale: the attack's corner at its time on the axis (attack 0 and 15), the
/// plateau at the sustain's level (0 and 15), against the SID's rate table.
#[test]
fn envelope_draws_to_scale() {
    // The datasheet's times, within 20 %.
    let near = |got: u32, ms: u32| {
        let want = ms * 1000;
        got >= want * 8 / 10 && got <= want * 12 / 10
    };
    unsafe {
        assert!(near(lab_draw_attack_us(0), 2));
        assert!(near(lab_draw_attack_us(15), 8000));
        assert!(near(lab_draw_decay_us(0), 6));
        assert!(near(lab_draw_decay_us(15), 24000));
        for (ms, x) in [(10, 54), (100, 100), (1000, 148), (10_000, 196)] {
            let got = lab_draw_x_of_us(ms * 1000);
            assert!(got.abs_diff(x) <= 1, "{ms} ms at x {got}, not {x}");
        }
    }
    let top = |g: &grid, x: usize| -> Option<usize> {
        let ink = ink(g);
        (0..LAB_DRAW_H as usize).find(|&y| ink[y][x])
    };
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    for (attack, sustain) in [(0u32, 0u32), (0, 15), (15, 0), (15, 15)] {
        set(&mut ui, LIVE_PARAM_ATTACK, attack);
        set(&mut ui, LIVE_PARAM_SUSTAIN, sustain);
        set(&mut ui, LIVE_PARAM_DECAY, 2);
        settle(&mut ui);
        let (ad, sr) = (ui.live().attack_decay, ui.live().sustain_release);
        // The key goes at the middle, or once the sustain is reached and held half as long again.
        let gate = unsafe { lab_draw_gate_us(ad, sr) };
        let reached = unsafe { lab_draw_attack_us(attack) };
        assert!(gate >= unsafe { lab_draw_time_us(LAB_DRAW_GATE_X) });
        assert!(
            gate >= reached,
            "A{attack}: the attack is whole before the key goes"
        );
        let step = LAB_DRAW_STEP as usize;
        let y_at = |x: usize| {
            let t = unsafe { lab_draw_time_us(x as u32) };
            let mut level = unsafe { lab_draw_level(ad, sr, gate, t) };
            if t < reached && t < gate {
                // rising: the sample's top is where its time ends
                let end = unsafe { lab_draw_time_us((x + step) as u32) }
                    .min(reached)
                    .min(gate);
                level = level.max(unsafe { lab_draw_level(ad, sr, gate, end) });
            }
            (4 + (255 - level) * 96 / 255) as usize
        };
        for x in [10usize, 40, 80, 118, 130, 200] {
            // a sample is joined to the one before: its top is the higher of the two
            let y = y_at(x).min(y_at(x - step));
            let got = top(&ui.g, x).expect("a column of the curve");
            assert!(
                got.abs_diff(y) <= 1,
                "A{attack} S{sustain}: x {x} at row {got}, wants {y}"
            );
        }
        // The corner: the first column at the top is the attack's time on the axis.
        let corner = (0..LAB_DRAW_W as usize)
            .find(|&x| top(&ui.g, x) == Some(4))
            .unwrap();
        let want = unsafe { lab_draw_x_of_us(reached) } as usize / step * step;
        assert!(
            corner.abs_diff(want) <= step,
            "A{attack}: the corner at {corner}, not {want}"
        );
        if sustain == 15 && attack == 0 {
            assert_eq!(
                top(&ui.g, 110),
                Some(4),
                "no decay at S15: the plateau at the top"
            );
        }
        if sustain == 0 && attack == 0 {
            assert_eq!(
                top(&ui.g, 110),
                Some(100),
                "S0: down to nothing before the key goes"
            );
        }
    }
}

/// The card turned lights its part of the drawing: ENVELOPE's segments, OSC's pulse edge and band
/// and the tuned cycle's end.
#[test]
fn a_turned_card_lights_its_part() {
    let mut ui = lab_ui();
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    set(&mut ui, LIVE_PARAM_ATTACK, 6);
    set(&mut ui, LIVE_PARAM_DECAY, 6);
    set(&mut ui, LIVE_PARAM_SUSTAIN, 8);
    set(&mut ui, LIVE_PARAM_RELEASE, 6);
    settle(&mut ui);
    let gate_col = (LAB_DRAW_GATE_X / 8) as usize;
    let attack_end = unsafe { lab_draw_x_of_us(lab_draw_attack_us(6)) } as usize / 8;
    for k in 0..4u32 {
        ui.turn(K1 + k, 1);
        ui.turn(K1 + k, -1);
        step(&mut ui);
        step(&mut ui);
        let lit = cells_in(&ui.g, th.knob[k as usize]);
        assert!(!lit.is_empty(), "K{}: its segment lit", k + 1);
        let cols: Vec<usize> = lit.iter().map(|&(c, _)| c).collect();
        let (lo, hi) = (*cols.iter().min().unwrap(), *cols.iter().max().unwrap());
        match k {
            0 => assert!(hi <= attack_end + 1, "the attack: {lo}..{hi}"),
            1 | 2 => assert!(
                lo + 1 >= attack_end && hi <= gate_col,
                "decay/sustain: {lo}..{hi}"
            ),
            _ => assert!(lo >= gate_col, "the release: {lo}..{hi}"),
        }
        settle(&mut ui);
        assert!(cells_in(&ui.g, th.knob[k as usize]).is_empty(), "and back");
    }
    // OSC on a pulse: K1 the edge, K2 the band (glyphs in its colour), K4 the note's cycle end.
    let mut ui = Ui::without_cards();
    unsafe { live_set_preset(&mut *ui.rig.live, 1) };
    open_lab(&mut ui);
    ui.turn(K1, 1);
    step(&mut ui);
    step(&mut ui);
    assert!(
        !cells_in(&ui.g, th.knob[0]).is_empty(),
        "K1: the pulse's edge"
    );
    settle(&mut ui);
    ui.turn(K1 + 1, 1);
    step(&mut ui);
    step(&mut ui);
    let band = (ROW0..ROW0 + 13)
        .flat_map(|r| (0..COLS).map(move |c| r * COLS + c))
        .filter(|&i| ui.g.ch[i] == FONT_D2 as u8 && ui.g.attr[i] & 15 == th.knob[1])
        .count();
    assert!(band > 0, "K2: the band in its colour");
    settle(&mut ui);
    ui.turn(K1 + 3, 1);
    step(&mut ui);
    step(&mut ui);
    let end = cells_in(&ui.g, th.knob[3]);
    assert!(
        end.iter().all(|&(c, _)| c == 28),
        "K4: the note's cycle end {end:?}"
    );
    assert!(end.len() >= 10, "a line down the area");
}

/// While a key sounds the tap's envelope is a dot on ENVELOPE's curve at its level, and it moves
/// with it.
#[test]
fn the_envelope_dot_follows_the_tap() {
    let mut ui = lab_ui();
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    set(&mut ui, LIVE_PARAM_ATTACK, 10); // 500 ms
    settle(&mut ui);
    assert!(cells_in(&ui.g, th.value).is_empty(), "no dot at rest");
    let mut xs = Vec::new();
    for level in [60u8, 120, 180] {
        ui.sc.env[1] = level;
        ui.sc.state[1] = 0; // SID_ENV_ATTACK
        step(&mut ui);
        step(&mut ui);
        let dot = cells_in(&ui.g, th.value);
        assert!(!dot.is_empty(), "the dot at {level}");
        let x = dot.iter().map(|&(c, _)| c).min().unwrap();
        let attack = unsafe { lab_draw_attack_us(10) };
        let want = unsafe { lab_draw_x_of_us(attack * u32::from(level) / 255) } as usize / 8;
        assert!(
            x.abs_diff(want) <= 1,
            "level {level}: the dot at cell {x}, not {want}"
        );
        xs.push(x);
    }
    assert!(
        xs.windows(2).all(|w| w[0] < w[1]),
        "it moves up the attack: {xs:?}"
    );
    golden(&ui, "lab-envelope-dot");
    ui.sc.env[1] = 0;
    settle(&mut ui);
    assert!(
        cells_in(&ui.g, th.value).is_empty(),
        "gone when the voice is silent"
    );
}

/// FILTER's K4, the filter LFO, draws the band the cutoff sweeps: dotted cells behind the curve,
/// from the cutoff 8 detents down to 8 up (in K4's colour while it turns), with the LFO still
/// (the phase at its centre); a golden.
#[test]
fn the_filter_lfo_draws_its_band() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER);
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    let dots = |ui: &Ui| -> Vec<usize> {
        (0..GRID_COLS as usize)
            .filter(|&c| ui.g.ch[(ROW0 + 1) * GRID_COLS as usize + c] == FONT_D2 as u8)
            .collect()
    };
    assert!(dots(&ui).is_empty(), "no LFO: no band");
    ui.turn(K1, -200);
    ui.turn(K1, 32);
    set(&mut ui, LIVE_PARAM_FILTER_LFO, 8);
    settle(&mut ui);
    let band = dots(&ui);
    assert!(band.len() >= 4, "{band:?}");
    let cut = columns_in(&ui.g, th.label as u8);
    assert!(
        band.first().unwrap() < &cut[0] && band.last().unwrap() > &cut[0],
        "the band {band:?} about the cutoff's line {cut:?}"
    );
    set(&mut ui, LIVE_PARAM_FILTER_LFO, 16);
    settle(&mut ui);
    assert!(dots(&ui).len() > band.len(), "deeper, wider");
    page_golden(&mut ui, "lab-filter-lfo");
}

// ---- FILTER, MOD and FX (Task 7) ----

/// The columns of the cells of the area in colour `c`.
fn columns_in(g: &grid, c: u8) -> Vec<usize> {
    let mut cols: Vec<usize> = cells_in(g, c).iter().map(|&(col, _)| col).collect();
    cols.sort_unstable();
    cols.dedup();
    cols
}

/// A page, settled, with the hint line's first phrase: a golden of it.
fn page_golden(ui: &mut Ui, name: &str) {
    settle_hint(ui);
    golden(ui, name);
}

/// FILTER's response for each mode (LP, BP, HP, NOTCH) and at full resonance: goldens, with the
/// shape each mode has: the low pass is up at the left, the high pass at the right, the band
/// pass at neither end, the notch at both and down at the cutoff.
#[test]
fn filter_draws_each_mode() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER);
    set(&mut ui, LIVE_PARAM_RESONANCE, 4);
    set(&mut ui, LIVE_PARAM_CUTOFF, 400);
    unsafe { live_tick(&mut *ui.rig.live) };
    // the row of the curve (the lowest ink of the white trace) at pixel column x
    let top_at = |ui: &Ui, x: usize| -> Option<usize> {
        let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
        let mut best = None;
        for r in 0..LAB_DRAW_ROWS as usize {
            if let Some((m, k)) = overlay(&ui.g, x / 8, ROW0 + r) {
                if k == th.value as u8 {
                    for (y, bits) in m.iter().enumerate() {
                        if bits >> (7 - x % 8) & 1 != 0 {
                            best = best.or(Some(r * 8 + y));
                        }
                    }
                }
            }
        }
        best
    };
    for (i, name, left, right) in [
        (0, "lab-filter-lp", true, false),
        (1, "lab-filter-bp", false, false),
        (2, "lab-filter-hp", false, true),
        (3, "lab-filter-notch", true, true),
    ] {
        ui.turn(PANEL_ENC_ALGORITHM, -9);
        ui.turn(PANEL_ENC_ALGORITHM, i);
        settle(&mut ui);
        let (l, r) = (top_at(&ui, 40).unwrap(), top_at(&ui, 226).unwrap());
        assert_eq!(
            l < 20,
            left,
            "{name}: the passband at the left, curve at {l}"
        );
        assert_eq!(
            r < 20,
            right,
            "{name}: the passband at the right, curve at {r}"
        );
        page_golden(&mut ui, name);
    }
    // the resonance: a peak above the passband, and the reading lights where it is
    ui.turn(PANEL_ENC_ALGORITHM, -9);
    settle(&mut ui);
    let flat = top_at(&ui, 40).unwrap();
    set(&mut ui, LIVE_PARAM_RESONANCE, 15);
    settle(&mut ui);
    let cutoff_col = columns_in(
        &ui.g,
        unsafe { &*theme_get(u32::from(ui.live().theme)) }.label as u8,
    );
    assert_eq!(cutoff_col.len(), 1, "the cutoff is one line");
    let peak = (cutoff_col[0] * 8 - 16..cutoff_col[0] * 8 + 16)
        .filter_map(|x| top_at(&ui, x))
        .min()
        .unwrap();
    assert!(
        peak < flat,
        "the peak {peak} rises above the passband {flat}"
    );
    page_golden(&mut ui, "lab-filter-reso15");
}

/// The filter's drawing is the cutoff the chip has now: the wheel on CUTOFF moves the line, at
/// the column of the cutoff's Hz on the 30 Hz to 12 kHz axis.
#[test]
fn filter_follows_the_cutoff_the_chip_has() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FILTER);
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    set(&mut ui, LIVE_PARAM_MOD_TARGET, LIVE_MOD_CUTOFF);
    set(&mut ui, LIVE_PARAM_CUTOFF, 200);
    let mut cols = Vec::new();
    for wheel in [0u8, 40, 100] {
        unsafe {
            live_cc(&mut *ui.rig.live, LIVE_CC_MOD_WHEEL as u8, wheel);
            live_tick(&mut *ui.rig.live);
        }
        settle(&mut ui);
        let fc = u32::from(ui.live().cutoff_out);
        assert_eq!(fc, 200 + 8 * u32::from(wheel), "the chip's cutoff");
        let hz = unsafe { sid_cutoff_hz(ui.live().model, 0, fc) }.clamp(30.0, 12000.0);
        let want = 8.0 + 26.0 * (hz / 30.0).log2();
        let got = columns_in(&ui.g, th.label as u8);
        assert_eq!(got.len(), 1, "wheel {wheel}: one line");
        assert!(
            (got[0] as f32 * 8.0 - want).abs() < 12.0,
            "wheel {wheel}: the line at cell {}, {hz} Hz is column {want}",
            got[0]
        );
        cols.push(got[0]);
    }
    assert!(cols.windows(2).all(|w| w[0] < w[1]), "it opens: {cols:?}");
}

/// MOD: the vibrato over its delay (a flat start), the PWM below it, and ARP's staircase as the
/// order and the rate make it: goldens.
#[test]
fn mod_draws_the_vibrato_the_pwm_and_arp() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_MOD);
    set(&mut ui, LIVE_PARAM_VIBRATO_RATE, 40);
    set(&mut ui, LIVE_PARAM_VIBRATO_DEPTH, 100);
    set(&mut ui, LIVE_PARAM_VIBRATO_DELAY, 40);
    page_golden(&mut ui, "lab-mod-vibrato");
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    // the delay is a flat start: the first 40 ticks of 128 of the window's 204 columns
    let ys: Vec<usize> = (36..(32 + 204 * 40 / 128))
        .map(|x| {
            ink(&ui.g)
                .iter()
                .position(|row| row[x])
                .expect("the vibrato's ink")
        })
        .collect();
    assert!(
        ys.windows(2).all(|w| w[0].abs_diff(w[1]) <= 1),
        "flat over the delay: {ys:?}"
    );
    // ARP up-down over two octaves: a staircase in the marker's colour
    ui.turn(PANEL_ENC_ALGORITHM, 3);
    unsafe { live_arp_set(&mut *ui.rig.live, LIVE_ARP_SPAN, 2) };
    unsafe { live_arp_set(&mut *ui.rig.live, LIVE_ARP_RATE, 2) };
    settle(&mut ui);
    assert!(
        !cells_in(&ui.g, th.hot as u8).is_empty(),
        "ARP's staircase is drawn"
    );
    page_golden(&mut ui, "lab-mod-arp");
    ui.turn(PANEL_ENC_ALGORITHM, -9);
    settle(&mut ui);
    assert!(cells_in(&ui.g, th.hot as u8).is_empty(), "ARP off: none");
    // the lit part: K3 the delay, the flat start
    ui.turn(K1 + 2, 1);
    step(&mut ui);
    step(&mut ui);
    let lit = columns_in(&ui.g, th.knob[2] as u8);
    assert!(
        !lit.is_empty() && *lit.last().unwrap() <= 32 / 8 + 204 * 41 / 128 / 8 + 1,
        "K3 lights the delay: {lit:?}"
    );
}

/// FX: the chain (the slots, the selected one marked, one that is off dark) and the selected
/// slot's shape: a transfer curve that bends with the drive, the delay's taps, a slot without a
/// drawing named at 2x.
#[test]
fn fx_draws_the_chain_and_the_slots_shape() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_FX);
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    ui.turn(PANEL_ENC_ALGORITHM, 2);
    page_golden(&mut ui, "lab-fx-chain");
    assert_eq!(ui.big(ROW0, 7, 2, 9), "OVERDRIVE");
    // drive: a harder drive carries the curve to its rail sooner (a quarter in, its lowest ink is lower)
    let quarter = |ui: &Ui| {
        let ink = ink(&ui.g);
        (0..LAB_DRAW_H as usize)
            .rev()
            .find(|&y| ink[y][92 + 14])
            .expect("the curve is drawn")
    };
    set(&mut ui, LIVE_PARAM_SLOT + LIVE_SLOT_AMOUNT, 20);
    settle(&mut ui);
    let soft = quarter(&ui);
    set(&mut ui, LIVE_PARAM_SLOT + LIVE_SLOT_AMOUNT, 120);
    settle(&mut ui);
    page_golden(&mut ui, "lab-fx-drive");
    let hard = quarter(&ui);
    assert!(
        hard > soft,
        "the hard drive's curve is lower: {hard} against {soft}"
    );
    // the FX slot: DELAY's taps, in the layers' colours
    ui.press(PANEL_BTN_SCL);
    ui.press(PANEL_BTN_SCL);
    settle(&mut ui);
    assert_eq!(ui.live().fx_select, 2);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    set(
        &mut ui,
        LIVE_PARAM_SLOT + 2 * LIVE_SLOT_FIELDS + LIVE_SLOT_AMOUNT,
        90,
    );
    set(
        &mut ui,
        LIVE_PARAM_SLOT + 2 * LIVE_SLOT_FIELDS + LIVE_SLOT_A,
        40,
    );
    settle(&mut ui);
    assert_eq!(choice(&ui), "DELAY");
    assert!(
        !cells_in(&ui.g, th.value as u8).is_empty(),
        "the echoes are drawn"
    );
    page_golden(&mut ui, "lab-fx-delay");
    // LO-FI has no drawing: its name at 2x
    ui.press(PANEL_BTN_SCL);
    ui.turn(PANEL_ENC_ALGORITHM, 9);
    settle(&mut ui);
    assert_eq!(choice(&ui), "LO-FI");
    assert_eq!(ui.big(ROW0 + 9, 10, 2, 5), "LO-FI", "named, not drawn");
}

// ---- keeping (Task 8 on): LAB on a store ----

const SAVE: u32 = SAVE_KEY_BUTTON;

unsafe extern "C" fn no_force(_: i32) {}

/// LAB over the views with the settings and the USER sounds on a store, the store's polls
/// between frames (as platform/fm1/ui.c wires them).
struct Stored {
    ui: Ui,
    st: Store,
    _set: Box<settings>,
    _ml: Box<midi_learn>,
}

impl Stored {
    fn new() -> Self {
        let mut ui = Ui::without_cards();
        let mut set = Box::<settings>::default();
        let mut ml = Box::<midi_learn>::default();
        let mut st = fresh(7, 256);
        unsafe {
            midi_learn_init(&mut *ml);
            views_learn(&mut *ui.v, &mut *ml);
            settings_init(
                &mut *set,
                &mut *ui.rig.live,
                &mut *ml,
                &mut ui.v.cards,
                &mut ui.v.song.scroll,
                std::ptr::null_mut(),
                &mut *st.st,
                c"V1.2.3".as_ptr(),
                Some(no_force),
            );
            settings_load(&mut *set, ui.now);
            views_settings(&mut *ui.v, &mut *set);
            views_sounds(&mut *ui.v, &mut *st.st);
        }
        let mut k = Stored {
            ui,
            st,
            _set: set,
            _ml: ml,
        };
        for _ in 0..140 {
            k.frame(); // past the first settings save
        }
        open_lab(&mut k.ui);
        k
    }

    fn frame(&mut self) {
        unsafe { store_quiet(&mut *self.st.st, 1) };
        for _ in 0..256 {
            if unsafe { store_poll(&mut *self.st.st) } == 0 {
                break;
            }
        }
        step(&mut self.ui);
    }

    fn frames(&mut self, n: u32) {
        for _ in 0..n {
            self.frame();
        }
    }

    /// Frames until a toast that starts with `prefix` shows (at most `ms`).
    fn toast_within(&mut self, ms: u32, prefix: &str) -> Option<String> {
        for _ in 0..ms / 40 {
            self.frame();
            if let Some(t) = self.ui.toast() {
                if t.starts_with(prefix) {
                    return Some(t);
                }
            }
        }
        None
    }

    /// Slot n as the store holds it: its name and sound.
    fn slot(&self, n: u32) -> Option<(String, Vec<u8>)> {
        let mut name = [0 as std::os::raw::c_char; LAB_SOUND_NAME as usize + 1];
        let mut s = [0u8; LAB_SOUND_BYTES as usize];
        (unsafe { user_sound_read(&self.ui.v.sounds, n, name.as_mut_ptr(), s.as_mut_ptr()) } != 0)
            .then(|| {
                let name = unsafe { CStr::from_ptr(name.as_ptr()) };
                (name.to_str().unwrap().to_owned(), s.to_vec())
            })
    }

    fn sound(&self) -> Vec<u8> {
        let mut s = [0u8; LAB_SOUND_BYTES as usize];
        unsafe { live_sound_get(&*self.ui.rig.live, s.as_mut_ptr()) };
        s.to_vec()
    }

    fn name_row(&self) -> String {
        self.ui
            .row(LAB_VIEW_NAME_ROW as usize)
            .trim_end()
            .to_string()
    }

    /// A turn of ENVELOPE's attack (a sound of its own), and SAVE.
    fn shape_and_keep(&mut self) {
        to_page(&mut self.ui, LAB_PAGE_ENVELOPE);
        self.ui.turn(K1, 1);
        self.ui.press(SAVE);
    }
}

#[test]
fn save_keeps_a_built_in_in_the_next_free_slot_with_its_automatic_name() {
    let mut k = Stored::new();
    assert_eq!(k.ui.v.sounds.result, 0);
    k.shape_and_keep();
    assert_eq!(
        k.ui.toast().as_deref(),
        None,
        "the toast waits for the commit"
    );
    assert_eq!(
        k.toast_within(2000, "KEPT").as_deref(),
        Some("KEPT AS U01 LAB 01")
    );
    for _ in 0..3 {
        step(&mut k.ui); // the toast drawn whole
    }
    golden(&k.ui, "lab-kept");
    let (name, sound) = k.slot(0).expect("USER 1 holds the sound");
    assert_eq!(name, "LAB 01");
    assert_eq!(sound, k.sound());
    assert_eq!(sound[LAB_SOUND_AD as usize] >> 4, 1, "the attack turned");
    assert_eq!(k.ui.live().user_sound, 1);
    assert_eq!(k.ui.live().sounds_valid, 1);
    k.frames(3);
    assert_eq!(k.name_row(), "LAB 01", "kept: the star goes");
    assert!(k.slot(1).is_none());
    // a second keep of another built-in takes the next slot
    k.frames(60);
    unsafe { live_recall_preset(&mut *k.ui.rig.live, 1) };
    k.shape_and_keep();
    assert_eq!(
        k.toast_within(2000, "KEPT").as_deref(),
        Some("KEPT AS U02 LAB 02")
    );
    assert_eq!(k.slot(1).unwrap().0, "LAB 02");
}

#[test]
fn save_updates_a_user_sound_in_place() {
    let mut k = Stored::new();
    k.shape_and_keep();
    k.toast_within(2000, "KEPT").unwrap();
    k.frames(60);
    k.ui.turn(K1 + 1, 2); // DECAY
    k.frames(3);
    assert_eq!(k.name_row(), "LAB 01*", "edited again");
    k.ui.press(SAVE);
    assert_eq!(
        k.toast_within(2000, "KEPT").as_deref(),
        Some("KEPT U01 LAB 01")
    );
    let (name, sound) = k.slot(0).unwrap();
    assert_eq!(name, "LAB 01", "its name kept");
    assert_eq!(sound, k.sound());
    assert_eq!(
        sound[LAB_SOUND_AD as usize] & 15,
        11,
        "the decay (9 + 2) is in the slot"
    );
    assert!(k.slot(1).is_none(), "no second slot");
    k.frames(3);
    assert_eq!(k.name_row(), "LAB 01");
}

#[test]
fn a_sixteenth_keep_and_then_a_full_set() {
    let mut k = Stored::new();
    for n in 0..16u32 {
        unsafe { live_recall_preset(&mut *k.ui.rig.live, 1) };
        k.shape_and_keep();
        let want = format!("KEPT AS U{:02} LAB {:02}", n + 1, n + 1);
        assert_eq!(k.toast_within(4000, "KEPT").as_deref(), Some(want.as_str()));
        k.frames(60);
    }
    assert_eq!(k.ui.live().sounds_valid, 0xFFFF);
    // a seventeenth, from a sound that is not a USER one: a toast, no write, nothing changed
    unsafe { live_recall_preset(&mut *k.ui.rig.live, 2) };
    let before = k.sound();
    k.ui.press(SAVE);
    assert_eq!(k.ui.toast().as_deref(), Some("USER FULL: EDIT+SAVE"));
    assert_eq!(k.ui.v.sounds.state, 0, "no write");
    k.frames(60);
    for n in 0..16u32 {
        assert_eq!(k.slot(n).unwrap().0, format!("LAB {:02}", n + 1));
    }
    assert_eq!(k.sound(), before);
    assert_eq!(k.ui.live().user_sound, 0);
    // a USER sound of the full set still keeps in place
    unsafe { live_sound_apply(&mut *k.ui.rig.live, k.slot(4).unwrap().1.as_ptr()) };
    let n = std::ffi::CString::new("LAB 05").unwrap();
    unsafe {
        live_load_user(
            &mut *k.ui.rig.live,
            4,
            k.slot(4).unwrap().1.as_ptr(),
            n.as_ptr(),
        )
    };
    k.ui.turn(K1 + 1, 1);
    k.ui.press(SAVE);
    assert_eq!(
        k.toast_within(2000, "KEPT").as_deref(),
        Some("KEPT U05 LAB 05")
    );
}

#[test]
fn save_and_a_knob_is_still_learn() {
    let mut k = Stored::new();
    k.ui.learn_on(K1);
    k.frames(3);
    assert!(
        unsafe { learn_view_is_open(&k.ui.v.learn) } != 0,
        "learn opened"
    );
    assert_eq!(k.ui.live().sounds_valid, 0, "no keep");
    assert_eq!(k.ui.v.sounds.state, 0);
    // SAVE + HOME is learn's clear: no keep either
    k.ui.down(SAVE);
    k.ui.press(PANEL_BTN_HOME);
    k.ui.up(SAVE);
    k.frames(3);
    assert_eq!(k.ui.live().sounds_valid, 0);
    assert_eq!(k.ui.v.sounds.state, 0);
}

#[test]
fn the_toast_waits_for_the_commit() {
    let mut k = Stored::new();
    k.shape_and_keep();
    assert_ne!(k.ui.v.sounds.state, 0, "queued, not written");
    assert_eq!(k.ui.toast().as_deref(), None);
    let mut seen = 0;
    for _ in 0..50 {
        k.frame();
        if k.ui.toast().is_some_and(|t| t.starts_with("KEPT")) {
            seen += 1;
            assert_eq!(k.ui.v.sounds.state, 0, "the write is over when it says so");
            assert!(k.slot(0).is_some(), "and the slot reads back");
            break;
        }
    }
    assert_eq!(seen, 1);
}

#[test]
fn a_second_keep_while_one_writes_is_refused_with_a_toast() {
    let mut k = Stored::new();
    k.shape_and_keep();
    assert_ne!(k.ui.v.sounds.state, 0);
    k.ui.turn(K1 + 1, 1);
    k.ui.press(SAVE);
    assert_eq!(k.ui.toast().as_deref(), Some("SAVING: WAIT"));
    k.toast_within(2000, "KEPT AS").unwrap();
    k.frames(60);
    assert_eq!(k.ui.live().sounds_valid, 1, "one slot, one write");
    // the refused keep wrote nothing: the slot holds the first sound
    assert_ne!(k.slot(0).unwrap().1, k.sound());
    // and now it keeps
    k.ui.press(SAVE);
    assert_eq!(
        k.toast_within(2000, "KEPT").as_deref(),
        Some("KEPT U01 LAB 01")
    );
    assert_eq!(k.slot(0).unwrap().1, k.sound());
}

// ---- the name page (Task 9): EDIT + SAVE ----

const EDIT: u32 = PANEL_BTN_EDIT;

impl Stored {
    /// EDIT held, SAVE pressed and let go (EDIT let go after): the name page.
    fn edit_save(&mut self) {
        self.ui.down(EDIT);
        self.ui.press(SAVE);
        self.ui.up(EDIT);
        self.frames(3);
    }

    fn lab(&self) -> &lab_view {
        &self.ui.v.lab
    }

    fn page_name(&self) -> String {
        let n: Vec<u8> = self
            .lab()
            .name
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8(n).unwrap().trim_end().to_string()
    }

    /// The keys k (bit k of the key mask) pressed and let go.
    fn type_keys(&mut self, keys: &[u32]) {
        for &k in keys {
            self.ui.note_keys(1 << k, 1 << k);
            self.ui.note_keys(0, 0);
        }
        self.frames(3);
    }

    fn sounding(&self) -> bool {
        self.ui.live().voice.iter().any(|v| v.gate != 0)
    }
}

#[test]
fn edit_save_opens_the_name_page_on_the_current_slot() {
    let mut k = Stored::new();
    k.edit_save();
    assert_eq!(u32::from(k.lab().mode), LAB_VIEW_MODE_NAME);
    assert_eq!(k.lab().slot, 0, "the lowest free slot of a built-in");
    assert_eq!(k.page_name(), "LAB 01");
    assert_eq!(k.ui.v.sounds.state, 0, "nothing written by opening it");
    assert_eq!(k.ui.live().sounds_valid, 0);
    // K1 turns the slot, and the automatic name follows it
    k.ui.turn(K1, 4);
    k.frames(3);
    assert_eq!(k.lab().slot, 4);
    assert_eq!(k.page_name(), "LAB 05");
    k.ui.turn(K1, 40);
    assert_eq!(k.lab().slot, 15, "clamped");
    k.ui.turn(K1, -40);
    assert_eq!(k.lab().slot, 0);
    // a USER sound opens on its own slot with its own name, which K1 leaves alone
    k.ui.press(PANEL_BTN_HOME);
    k.frames(3);
    assert_eq!(u32::from(k.lab().mode), LAB_VIEW_MODE_PAGES);
    k.ui.turn(K1 + 1, 1);
    k.ui.press(SAVE);
    k.toast_within(2000, "KEPT").unwrap();
    k.frames(60);
    k.edit_save();
    assert_eq!(k.lab().slot, 0);
    assert_eq!(k.page_name(), "LAB 01");
    k.ui.turn(K1, 2);
    assert_eq!(k.page_name(), "LAB 01", "a USER sound's name stays");
}

#[test]
fn keys_type_the_name_and_select_moves() {
    let mut k = Stored::new();
    k.edit_save();
    k.type_keys(&[7, 8]); // H I
    assert_eq!(
        k.page_name(),
        "HI",
        "the first key starts a name of its own"
    );
    assert_eq!(k.lab().cursor_at, 2);
    k.type_keys(&[26, 0]); // a space, A
    assert_eq!(k.page_name(), "HI A");
    assert_eq!(k.lab().cursor_at, 4);
    // SELECT moves the cursor, clamped
    k.ui.turn(PANEL_ENC_SELECT, -2);
    assert_eq!(k.lab().cursor_at, 2);
    k.ui.turn(PANEL_ENC_SELECT, -9);
    assert_eq!(k.lab().cursor_at, 0);
    k.ui.turn(PANEL_ENC_SELECT, 99);
    assert_eq!(u32::from(k.lab().cursor_at), LAB_SOUND_NAME - 1);
    // ALGORITHM changes the character under it: the SAVE AS alphabet, wrapping
    k.ui.turn(PANEL_ENC_SELECT, -99);
    k.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(k.page_name(), "II A", "H to I");
    k.ui.turn(PANEL_ENC_ALGORITHM, -2);
    assert_eq!(&k.page_name()[..1], "G");
    k.ui.turn(PANEL_ENC_ALGORITHM, 64);
    assert_eq!(&k.page_name()[..1], "G", "a lap of the alphabet");
    // the last character: a key types it and the cursor stays
    k.ui.turn(PANEL_ENC_SELECT, 99);
    k.type_keys(&[25]);
    assert_eq!(k.page_name().chars().last(), Some('Z'));
    assert_eq!(u32::from(k.lab().cursor_at), LAB_SOUND_NAME - 1);
    // the name written is the typed one, trimmed
    k.ui.turn(PANEL_ENC_SELECT, -99);
    k.ui.press(SAVE);
    assert_eq!(
        k.toast_within(2000, "KEPT").as_deref(),
        Some("KEPT AS U01 GI A       Z")
    );
}

#[test]
fn a_used_slot_asks_once_more() {
    let mut k = Stored::new();
    k.shape_and_keep();
    k.toast_within(2000, "KEPT").unwrap();
    k.frames(60);
    let kept = k.slot(0).unwrap();
    unsafe { live_recall_preset(&mut *k.ui.rig.live, 1) };
    k.ui.turn(K1 + 1, 3);
    k.edit_save();
    assert_eq!(k.lab().slot, 1, "the lowest free slot");
    k.ui.turn(K1, -1);
    assert_eq!(k.lab().slot, 0, "a used one");
    k.type_keys(&[1, 2]); // BC: a name of the player's
    k.ui.press(SAVE);
    k.frames(3);
    assert_eq!(u32::from(k.lab().mode), LAB_VIEW_MODE_ASK);
    assert_eq!(k.ui.v.sounds.state, 0, "asked, not written");
    assert_eq!(k.slot(0).unwrap(), kept);
    // another control cancels the question
    k.ui.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(u32::from(k.lab().mode), LAB_VIEW_MODE_NAME);
    k.ui.press(SAVE);
    assert_eq!(u32::from(k.lab().mode), LAB_VIEW_MODE_ASK);
    assert_eq!(k.slot(0).unwrap(), kept, "still asking");
    k.ui.press(SAVE);
    assert_eq!(
        k.toast_within(2000, "KEPT").as_deref(),
        Some("KEPT AS U01 BC")
    );
    assert_eq!(
        u32::from(k.lab().mode),
        LAB_VIEW_MODE_PAGES,
        "written: back to the pages"
    );
    let (name, sound) = k.slot(0).unwrap();
    assert_eq!(name, "BC");
    assert_eq!(sound, k.sound());
    assert_ne!(sound, kept.1);
}

#[test]
fn home_leaves_the_page_without_writing() {
    let mut k = Stored::new();
    k.edit_save();
    k.type_keys(&[0, 1]);
    k.ui.press(PANEL_BTN_HOME);
    k.frames(3);
    assert_eq!(k.ui.top(), "LAB", "HOME leaves the page, not LAB");
    assert_eq!(u32::from(k.lab().mode), LAB_VIEW_MODE_PAGES);
    assert_eq!(k.ui.v.sounds.state, 0);
    assert_eq!(k.ui.live().sounds_valid, 0);
    k.frames(60);
    assert!(k.slot(0).is_none());
    // and the next HOME is LAB's: back to LIVE
    k.ui.press(PANEL_BTN_HOME);
    k.frames(30);
    assert_eq!(k.ui.top(), "LIVE");
}

#[test]
fn the_name_page_takes_the_keys_and_lets_them_go() {
    let mut k = Stored::new();
    k.ui.note_keys(1 << 12, 1 << 12);
    k.frames(3);
    assert!(k.sounding(), "a key plays on the pages");
    k.ui.note_keys(0, 0);
    k.frames(3);
    k.edit_save();
    assert!(!k.sounding(), "the sound stopped with the page");
    k.type_keys(&[3]);
    assert!(!k.sounding(), "a typing key makes no sound");
    assert_eq!(&k.page_name()[..1], "D");
    k.ui.press(PANEL_BTN_HOME);
    k.frames(3);
    k.ui.note_keys(1 << 12, 1 << 12);
    k.frames(3);
    assert!(k.sounding(), "a key plays again once the page is left");
}

#[test]
fn the_name_page_matches_its_goldens() {
    let mut k = Stored::new();
    k.edit_save();
    k.type_keys(&[2, 0, 18, 18]); // CASS
    settle(&mut k.ui);
    k.frames(10);
    golden(&k.ui, "lab-name");
    // a used slot, asked
    k.ui.press(SAVE);
    k.toast_within(3000, "KEPT").unwrap();
    k.frames(80);
    unsafe { live_recall_preset(&mut *k.ui.rig.live, 1) };
    k.edit_save();
    k.ui.turn(K1, -1);
    k.ui.press(SAVE);
    settle(&mut k.ui);
    k.frames(10);
    assert_eq!(u32::from(k.lab().mode), LAB_VIEW_MODE_ASK);
    golden(&k.ui, "lab-name-overwrite");
}

// ---- PRESETS into the USER sounds (Task 10) ----

const PRESETS: u32 = PANEL_ENC_PRESETS;

impl Stored {
    /// USER sound n kept under `name` with the attack `attack` and the first slot's algorithm
    /// `alg`, as the sound playing; its write finished. The built-in it was made from is back.
    fn put(&mut self, n: u32, name: &str, attack: u32, alg: u32) {
        let live = &mut *self.ui.rig.live;
        unsafe {
            live_recall_preset(live, 1);
            live_param_set(live, LIVE_PARAM_ATTACK, attack);
            live_slot_pick(live, 0, alg);
        }
        let mut s = self.sound();
        let c = std::ffi::CString::new(name).unwrap();
        let r = unsafe {
            user_sound_save(
                &mut self.ui.v.sounds,
                n,
                c.as_ptr(),
                s.as_mut_ptr(),
                self.ui.v.s.now,
            )
        };
        assert_eq!(r, 0, "queued");
        for _ in 0..400 {
            self.frame();
            if self.ui.v.sounds.state == 0 && self.ui.v.sounds.result == 0 {
                break;
            }
        }
        assert_eq!(self.ui.v.sounds.state, 0, "written");
        self.frames(60); // the keep's toast gone
        unsafe { live_recall_preset(&mut *self.ui.rig.live, 0) };
    }

    fn preset(&self) -> String {
        let mut buf = [0 as std::os::raw::c_char; LIVE_PRESET_NAME_CHARS as usize];
        unsafe { live_preset_name(&*self.ui.rig.live, buf.as_mut_ptr()) };
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    fn presets(&mut self, steps: i32) {
        self.ui.turn(PRESETS, steps);
    }

    fn attack(&self) -> u32 {
        unsafe { live_param_get(&*self.ui.rig.live, LIVE_PARAM_ATTACK) }
    }

    fn slot_alg(&self) -> u8 {
        self.ui.live().slot[0][LIVE_SLOT_ALGORITHM as usize]
    }
}

#[test]
fn presets_steps_from_dice_into_the_user_sounds_and_back() {
    let mut k = Stored::new();
    k.put(1, "AAA", 2, 3);
    k.put(4, "BBB", 5, 3);
    assert_eq!(k.ui.live().sounds_valid, 1 << 1 | 1 << 4);
    k.presets(4);
    assert_eq!(k.ui.live().preset, LIVE_PRESET_DICE as u8, "DICE");
    assert_eq!(k.ui.live().user_sound, 0);
    k.presets(1);
    assert_eq!((k.ui.live().user_sound, k.preset().as_str()), (2, "AAA"));
    assert_eq!(k.attack(), 2, "its sound");
    k.presets(1);
    assert_eq!((k.ui.live().user_sound, k.preset().as_str()), (5, "BBB"));
    assert_eq!(k.attack(), 5);
    k.presets(1);
    assert_eq!(k.ui.live().user_sound, 5, "the end clamps");
    k.presets(-1);
    assert_eq!((k.ui.live().user_sound, k.preset().as_str()), (2, "AAA"));
    k.presets(-1);
    assert_eq!(k.ui.live().preset, LIVE_PRESET_DICE as u8);
    assert_eq!(k.ui.live().user_sound, 0);
    assert!(k.preset().starts_with("DICE"));
    k.presets(-1);
    assert_eq!(k.ui.live().preset, LIVE_PRESET_DICE as u8 - 1, "NOISE PERC");
    // a run of detents at once goes the same way
    k.presets(9);
    assert_eq!(k.ui.live().user_sound, 5, "clamped at BBB");
    k.presets(-9);
    assert_eq!(k.ui.live().preset, 0, "and back to SAW LEAD");
    // DICE with no USER sound after it still rolls again at the end
    let mut k = Stored::new();
    k.presets(4);
    let roll = k.ui.live().dice_rolls;
    k.presets(1);
    assert_eq!(k.ui.live().preset, LIVE_PRESET_DICE as u8);
    assert_ne!(k.ui.live().dice_rolls, roll, "a new roll");
}

#[test]
fn empty_slots_are_skipped() {
    let mut k = Stored::new();
    k.put(15, "LAST", 4, 0);
    k.presets(5);
    assert_eq!(
        (k.ui.live().user_sound, k.preset().as_str()),
        (16, "LAST"),
        "USER 16 is the first USER sound: the 15 empty slots before it are skipped"
    );
    k.presets(-1);
    assert_eq!(k.ui.live().preset, LIVE_PRESET_DICE as u8);
}

#[test]
fn a_user_sound_sets_the_slots_and_a_built_in_leaves_them() {
    let mut k = Stored::new();
    k.put(0, "SLOTTED", 3, 3);
    // a built-in with the first slot off
    unsafe { live_slot_pick(&mut *k.ui.rig.live, 0, 0) };
    assert_eq!(k.slot_alg(), 0);
    k.presets(5);
    assert_eq!(k.ui.live().user_sound, 1);
    assert_eq!(k.slot_alg(), 3, "the sound's own slot");
    // a built-in leaves the slots as they are
    unsafe { live_slot_pick(&mut *k.ui.rig.live, 0, 3) };
    k.presets(-5);
    assert_eq!(k.ui.live().preset, 0);
    assert_eq!(k.ui.live().user_sound, 0);
    assert_eq!(k.slot_alg(), 3, "the slots as they were");
}

#[test]
fn edit_presets_reverts_a_user_sound() {
    let mut k = Stored::new();
    k.put(2, "KEPT", 4, 0);
    k.presets(5);
    k.frames(3);
    assert_eq!((k.ui.live().user_sound, k.attack()), (3, 4));
    k.ui.turn(K1, 3); // LAB's page is OSC: K1 is the pulse width; the attack on ENVELOPE
    to_page(&mut k.ui, LAB_PAGE_ENVELOPE);
    k.ui.turn(K1, 3);
    k.frames(3);
    assert_eq!(k.attack(), 7);
    assert_eq!(k.preset(), "KEPT*");
    k.ui.hold(EDIT, true);
    k.ui.turn(PRESETS, 1);
    k.ui.hold(EDIT, false);
    k.frames(3);
    assert_eq!(k.attack(), 4, "back to the sound as kept");
    assert_eq!(k.preset(), "KEPT");
    assert_eq!(k.ui.live().user_sound, 3, "still that sound");
}

#[test]
fn the_user_sound_left_keeps_its_edit() {
    let mut k = Stored::new();
    k.put(0, "ONE", 1, 0);
    k.put(1, "TWO", 2, 0);
    k.presets(5);
    assert_eq!(k.ui.live().user_sound, 1);
    unsafe { live_param_set(&mut *k.ui.rig.live, LIVE_PARAM_ATTACK, 9) };
    assert_eq!(k.preset(), "ONE*");
    // away to DICE and back: the edit comes back, the sound still differs from its base
    k.presets(-1);
    assert_eq!(k.ui.live().user_sound, 0);
    assert_eq!(
        k.ui.live().preset,
        LIVE_PRESET_DICE as u8,
        "DICE, not SAW LEAD's edits"
    );
    k.presets(1);
    assert_eq!((k.preset().as_str(), k.attack()), ("ONE*", 9));
    // to the next USER sound: ONE's edit is kept, TWO is as kept
    k.presets(1);
    assert_eq!((k.preset().as_str(), k.attack()), ("TWO", 2));
    k.presets(-1);
    assert_eq!((k.preset().as_str(), k.attack()), ("ONE*", 9));
    // SAW LEAD's own edits were never touched by it
    k.presets(-5);
    assert_eq!(k.ui.live().preset, 0);
    assert_eq!(k.preset(), "SAW LEAD", "unedited");
    // an edit of the second one takes the shared place from the first
    k.presets(6);
    assert_eq!(k.ui.live().user_sound, 2);
    unsafe { live_param_set(&mut *k.ui.rig.live, LIVE_PARAM_ATTACK, 11) };
    k.presets(-1);
    assert_eq!(
        (k.preset().as_str(), k.attack()),
        ("ONE", 1),
        "ONE's edit was replaced"
    );
    k.presets(1);
    assert_eq!((k.preset().as_str(), k.attack()), ("TWO*", 11));
}

#[test]
fn the_menus_pre_names_a_user_sound() {
    let mut k = Stored::new();
    k.put(6, "MENU ONE", 1, 0);
    k.ui.press(PANEL_BTN_HOME); // LAB -> LIVE
    settle(&mut k.ui);
    k.ui.press(PANEL_BTN_HOME); // the menu
    settle(&mut k.ui);
    k.ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    k.ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_PRESET as i32);
    k.ui.turn(PANEL_ENC_ALGORITHM, 5); // the menu's PRE steps on past DICE
    for _ in 0..12 {
        k.frame();
    }
    assert_eq!(k.ui.live().user_sound, 7);
    let line = k.ui.menu_line(VIEWS_MENU_PRESET as usize);
    assert!(line.contains("MENU ONE"), "{line:?}");
}

#[test]
fn lab_presets_loads_and_live_shows_it() {
    let mut k = Stored::new();
    k.put(2, "U THREE", 6, 3);
    // in LAB: PRESETS loads the USER sound, the header names it
    k.presets(5);
    settle(&mut k.ui);
    assert_eq!(k.ui.live().user_sound, 3);
    assert_eq!(k.name_row(), "U THREE");
    assert_eq!(k.ui.toast(), None, "PRESETS opens no popup in LAB");
    to_page(&mut k.ui, LAB_PAGE_ENVELOPE);
    settle_hint(&mut k.ui);
    golden(&k.ui, "lab-user");
    // on LIVE the header names it too
    k.ui.press(PANEL_BTN_HOME);
    settle(&mut k.ui);
    assert_eq!(k.ui.top(), "LIVE");
    for _ in 0..10 {
        k.frame();
    }
    assert_eq!(k.ui.big(0, 0, 2, 7), "U THREE");
    golden(&k.ui, "live-user");
}

// ---- A/B on EDIT's tap (Task 11) ----

fn attack(ui: &Ui) -> u32 {
    param(ui, LIVE_PARAM_ATTACK)
}

fn base_attack(ui: &Ui) -> u32 {
    u32::from(ui.live().base[LAB_SOUND_AD as usize] >> 4)
}

/// LAB on ENVELOPE with the attack turned 3 detents: an edited sound, settled.
fn edited_lab() -> Ui {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    ui.turn(K1, 3);
    settle(&mut ui);
    assert_eq!(attack(&ui), base_attack(&ui) + 3);
    ui
}

/// The letter the header shows while comparing ("" when not).
fn ab_letter(ui: &Ui) -> String {
    let k = LAB_VIEW_NAME_ROW as usize * COLS + LAB_VIEW_AB_COL as usize;
    if u32::from(ui.g.ch[k]) == GRID_BIG_FIRST {
        (ui.g.src[k] as char).to_string()
    } else {
        String::new()
    }
}

/// The sound's name as the header shows it, without the letter.
fn name_row(ui: &Ui) -> String {
    let row = ui.row(LAB_VIEW_NAME_ROW as usize);
    let row = row.trim_end();
    let letter = ab_letter(ui);
    row.strip_suffix(letter.as_str())
        .unwrap_or(row)
        .trim_end()
        .to_string()
}

#[test]
fn a_tap_of_edit_compares_and_another_returns() {
    let mut ui = edited_lab();
    let a = base_attack(&ui);
    assert_eq!(name_row(&ui), "SAW LEAD*");
    ui.press(EDIT);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_A);
    assert_eq!(attack(&ui), a, "A: the sound as loaded");
    settle(&mut ui);
    assert_eq!(ab_letter(&ui), "A");
    assert_eq!(name_row(&ui), "SAW LEAD", "A is the base: no star");
    ui.press(EDIT);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_B);
    assert_eq!(attack(&ui), a + 3, "B: the edit again");
    settle(&mut ui);
    assert_eq!(ab_letter(&ui), "B");
    assert_eq!(name_row(&ui), "SAW LEAD*");
    ui.press(EDIT);
    assert_eq!(
        (u32::from(ui.v.lab.ab), attack(&ui)),
        (LAB_VIEW_AB_A, a),
        "and A again"
    );
    // A turn of B's is still B's: the next tap puts it aside with the rest of the edit
    ui.press(EDIT);
    ui.turn(K1 + 1, 1); // the decay
    let decay = param(&ui, LIVE_PARAM_DECAY);
    ui.press(EDIT);
    assert_eq!(attack(&ui), a);
    assert_eq!(
        u32::from(ui.live().base[LAB_SOUND_AD as usize] & 15),
        param(&ui, LIVE_PARAM_DECAY)
    );
    ui.press(EDIT);
    assert_eq!((attack(&ui), param(&ui, LIVE_PARAM_DECAY)), (a + 3, decay));
}

/// A control used while EDIT is held, and its name.
type Combo = (&'static str, fn(&mut Ui));

#[test]
fn edit_as_a_modifier_never_compares() {
    let combos: [Combo; 9] = [
        ("EDIT + K2", |ui| ui.turn(K1 + 1, 1)),
        ("EDIT + ALGORITHM", |ui| ui.turn(PANEL_ENC_ALGORITHM, 1)),
        ("EDIT + SELECT", |ui| ui.turn(PANEL_ENC_SELECT, 1)),
        ("EDIT + LFO", |ui| ui.press(PANEL_BTN_LFO)),
        ("EDIT + ARP", |ui| ui.press(PANEL_BTN_ARP)),
        ("EDIT + SCL", |ui| ui.press(PANEL_BTN_SCL)),
        ("EDIT + PRESETS", |ui| ui.turn(PANEL_ENC_PRESETS, 1)),
        ("EDIT + GLO", |ui| ui.press(PANEL_BTN_GLO)),
        ("EDIT + SAVE", |ui| {
            ui.press(SAVE);
            assert_eq!(
                u32::from(ui.v.lab.mode),
                LAB_VIEW_MODE_NAME,
                "the name page"
            );
        }),
    ];
    for (what, combo) in combos {
        let mut ui = edited_lab();
        ui.down(EDIT);
        combo(&mut ui);
        ui.up(EDIT);
        step(&mut ui);
        assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF, "{what} compared");
        assert_ne!(
            ui.toast().as_deref(),
            Some("NOTHING TO COMPARE"),
            "{what} tapped"
        );
    }
    // SAVE down first, then EDIT: a combination too
    let mut ui = edited_lab();
    ui.down(SAVE);
    ui.down(EDIT);
    ui.up(EDIT);
    ui.up(SAVE);
    assert_eq!(
        u32::from(ui.v.lab.ab),
        LAB_VIEW_AB_OFF,
        "SAVE + EDIT compared"
    );
    // EDIT's layer elsewhere is as it was: on LIVE, EDIT + K2 turns the slot's A, and a tap
    // there compares nothing
    let mut ui = edited_lab();
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "LIVE");
    let slot_a = ui.live().slot[0][LIVE_SLOT_A as usize];
    ui.down(EDIT);
    ui.turn(K1 + 1, 1);
    ui.up(EDIT);
    assert_ne!(
        ui.live().slot[0][LIVE_SLOT_A as usize],
        slot_a,
        "EDIT + K2 on LIVE"
    );
    let sound = attack(&ui);
    ui.press(EDIT);
    step(&mut ui);
    assert_eq!(
        (u32::from(ui.v.lab.ab), attack(&ui), ui.toast()),
        (LAB_VIEW_AB_OFF, sound, None),
        "a tap on LIVE"
    );
}

/// The header's A/B letter: (the letter, 1 when it is reversed: comparing; 0 outlined: a peek).
fn ab_mark(ui: &Ui) -> (String, bool) {
    let th = unsafe { &*theme_get(u32::from(ui.live().theme)) };
    let k = LAB_VIEW_NAME_ROW as usize * COLS + LAB_VIEW_AB_COL as usize;
    (ab_letter(ui), ui.g.attr[k] >> 4 == th.hot)
}

/// Hold-to-peek: a note key while EDIT is down plays A (the sound as kept or loaded) for as long
/// as EDIT is held, the header's A outlined; letting EDIT go gives B back, and no tap compares.
#[test]
fn a_note_while_edit_is_down_peeks_at_a() {
    let mut ui = edited_lab();
    let a = base_attack(&ui);
    let steps = ui.v.lab.undo.steps;
    ui.down(EDIT);
    assert_eq!(attack(&ui), a + 3, "EDIT alone: still B");
    ui.note_keys(1 << 12, 1 << 12);
    assert_eq!(attack(&ui), a, "the note plays A");
    settle(&mut ui);
    assert_eq!(ab_mark(&ui), ("A".into(), false), "A, outlined");
    assert_eq!(name_row(&ui), "SAW LEAD", "A is the base: no star");
    ui.note_keys(0, 0);
    ui.note_keys(1 << 14, 1 << 14);
    assert_eq!(attack(&ui), a, "A for as long as EDIT is held");
    ui.note_keys(0, 0);
    ui.up(EDIT);
    assert_eq!(attack(&ui), a + 3, "B back");
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF, "no tap");
    settle(&mut ui);
    assert_eq!(ab_letter(&ui), "");
    assert_eq!(ui.v.lab.undo.steps, steps, "a peek is no step");
    assert_eq!(ui.toast(), None);
    // the tap still compares
    ui.press(EDIT);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_A);
    // a peek while A plays: A stays, and EDIT's release toggles nothing
    ui.down(EDIT);
    ui.note_keys(1 << 12, 1 << 12);
    ui.note_keys(0, 0);
    ui.up(EDIT);
    assert_eq!((u32::from(ui.v.lab.ab), attack(&ui)), (LAB_VIEW_AB_A, a));
    // a peek from B: A while held, then B again, still comparing
    ui.press(EDIT);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_B);
    ui.down(EDIT);
    ui.note_keys(1 << 12, 1 << 12);
    assert_eq!(attack(&ui), a);
    ui.note_keys(0, 0);
    ui.up(EDIT);
    assert_eq!((u32::from(ui.v.lab.ab), attack(&ui)), (LAB_VIEW_AB_B, a + 3));
    // on an unedited sound a peek plays the sound as it is, and says nothing
    let mut ui = lab_ui();
    ui.down(EDIT);
    ui.note_keys(1 << 12, 1 << 12);
    ui.note_keys(0, 0);
    ui.up(EDIT);
    step(&mut ui);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF);
    assert_eq!(ui.toast(), None, "no NOTHING TO COMPARE: it was a peek");
}

/// Any other control during a peek puts B back first, then acts on B.
#[test]
fn a_control_during_a_peek_acts_on_b() {
    let mut ui = edited_lab();
    let a = base_attack(&ui);
    ui.down(EDIT);
    ui.note_keys(1 << 12, 1 << 12);
    assert_eq!(attack(&ui), a);
    let decay = param(&ui, LIVE_PARAM_DECAY);
    ui.turn(K1 + 1, 1); // the decay, with EDIT held: on B
    assert_eq!(
        (attack(&ui), param(&ui, LIVE_PARAM_DECAY)),
        (a + 3, decay + 1),
        "B back, and the turn on it"
    );
    ui.note_keys(0, 0);
    ui.up(EDIT);
    assert_eq!(
        (attack(&ui), param(&ui, LIVE_PARAM_DECAY)),
        (a + 3, decay + 1),
        "nothing more at the release"
    );
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF);
    // a button too: EDIT + LFO undoes B's last step, not A
    ui.down(EDIT);
    ui.note_keys(1 << 12, 1 << 12);
    ui.press(PANEL_BTN_LFO);
    ui.note_keys(0, 0);
    ui.up(EDIT);
    assert_eq!(
        (attack(&ui), param(&ui, LIVE_PARAM_DECAY)),
        (a + 3, decay),
        "B's decay undone"
    );
}

#[test]
fn a_turn_on_a_makes_a_the_edit() {
    let mut ui = edited_lab();
    let a = base_attack(&ui);
    ui.press(EDIT);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_A);
    let decay = param(&ui, LIVE_PARAM_DECAY);
    ui.turn(K1 + 1, 1);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF, "comparing ends");
    assert_eq!(
        (attack(&ui), param(&ui, LIVE_PARAM_DECAY)),
        (a, decay + 1),
        "A with the turn is the edit"
    );
    settle(&mut ui);
    assert_eq!(ab_letter(&ui), "");
    // the next tap compares the new edit with the base
    ui.press(EDIT);
    assert_eq!(param(&ui, LIVE_PARAM_DECAY), decay);
    ui.press(EDIT);
    assert_eq!((attack(&ui), param(&ui, LIVE_PARAM_DECAY)), (a, decay + 1));
}

#[test]
fn nothing_to_compare_on_an_unedited_sound() {
    let mut ui = lab_ui();
    ui.press(EDIT);
    step(&mut ui);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF);
    assert_eq!(ui.toast().as_deref(), Some("NOTHING TO COMPARE"));
    // nor on B made the base by a revert
    let mut ui = edited_lab();
    ui.press(EDIT);
    ui.press(EDIT);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_B);
    ui.hold(EDIT, true);
    ui.turn(PANEL_ENC_PRESETS, 1); // EDIT + PRESETS: back to the base
    ui.hold(EDIT, false);
    ui.press(EDIT);
    step(&mut ui);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF);
    assert_eq!(ui.toast().as_deref(), Some("NOTHING TO COMPARE"));
}

#[test]
fn a_load_ends_comparing() {
    let mut ui = edited_lab();
    ui.press(EDIT);
    settle(&mut ui);
    assert_eq!(ab_letter(&ui), "A");
    ui.turn(PANEL_ENC_PRESETS, 1); // PWM BASS
    settle(&mut ui);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF);
    assert_eq!(ab_letter(&ui), "");
    assert_eq!(name_row(&ui), "PWM BASS");
    ui.press(EDIT);
    step(&mut ui);
    assert_eq!(
        ui.toast().as_deref(),
        Some("NOTHING TO COMPARE"),
        "the other sound's edit is not B"
    );
}

#[test]
fn a_and_b_match_their_goldens() {
    let mut ui = edited_lab();
    ui.press(EDIT);
    settle_hint(&mut ui);
    golden(&ui, "lab-a");
    ui.press(EDIT);
    settle_hint(&mut ui);
    golden(&ui, "lab-b");
}

// ---- undo on EDIT + LFO (Task 12) ----

/// EDIT held, LFO pressed, EDIT let go; the toast it made.
fn undo(ui: &mut Ui) -> Option<String> {
    ui.down(EDIT);
    ui.press(PANEL_BTN_LFO);
    ui.up(EDIT);
    step(ui);
    ui.toast()
}

/// The sound and the envelope mode, as an undo step sees them.
fn image(ui: &Ui) -> Vec<u8> {
    let mut s = [0u8; LAB_EDIT_BYTES as usize];
    unsafe { lab_edit_get(&*ui.rig.live, s.as_mut_ptr()) };
    s.to_vec()
}

fn steps(ui: &Ui) -> u32 {
    u32::from(ui.v.lab.undo.steps)
}

#[test]
fn each_card_turn_undoes_to_the_value_before_the_run() {
    let mut ui = lab_ui();
    for page in 0..LAB_PAGES {
        to_page(&mut ui, page);
        for k in 0..4 {
            for dir in [1, -1] {
                let before = image(&ui);
                let n = steps(&ui);
                ui.turn(K1 + k, 2 * dir);
                ui.turn(K1 + k, dir);
                if image(&ui) == before {
                    assert_eq!(
                        steps(&ui),
                        n,
                        "page {page} K{}: nothing changed, no step",
                        k + 1
                    );
                    continue;
                }
                assert_eq!(steps(&ui), n + 1, "page {page} K{}: one step", k + 1);
                undo(&mut ui);
                assert_eq!(image(&ui), before, "page {page} K{} {dir:+}", k + 1);
            }
        }
    }
}

#[test]
fn a_run_of_detents_is_one_step() {
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    let before = image(&ui);
    for _ in 0..5 {
        ui.turn(K1, 1);
    }
    ui.turn(K1, -2);
    assert_eq!(steps(&ui), 1, "one card's run");
    // another control ends the run: the same card again is a step of its own
    ui.press(PANEL_BTN_ENV);
    ui.turn(K1, 1);
    assert_eq!(steps(&ui), 2);
    ui.turn(K1 + 1, 1);
    ui.turn(K1, 1);
    assert_eq!(
        steps(&ui),
        4,
        "K2 between: K1's next turn is a step of its own"
    );
    for left in (0..4).rev() {
        assert_eq!(undo(&mut ui), Some(format!("UNDO ({left})")));
    }
    assert_eq!(image(&ui), before, "all of it undone");
    assert_eq!(undo(&mut ui).as_deref(), Some("NOTHING TO UNDO"));
}

#[test]
fn sixteen_steps_then_the_oldest_falls_off() {
    assert_eq!(LAB_EDIT_STEPS, 16);
    let mut ui = lab_ui();
    to_page(&mut ui, LAB_PAGE_OSC);
    let mut images = vec![image(&ui)];
    for i in 0..18 {
        ui.turn(K1 + 1 + 2 * (i % 2), 1); // the PWM depth and the tune, by turns: a step each
        images.push(image(&ui));
    }
    assert_eq!(steps(&ui), LAB_EDIT_STEPS);
    for n in (0..16).rev() {
        undo(&mut ui);
        assert_eq!(image(&ui), images[n + 2], "{n} steps left");
    }
    assert_eq!(
        undo(&mut ui).as_deref(),
        Some("NOTHING TO UNDO"),
        "the two oldest fell off"
    );
    assert_eq!(image(&ui), images[2]);
}

#[test]
fn a_big_step_drops_the_oldest_steps_it_needs_room_for() {
    let mut e = Box::<lab_edit>::default();
    let n = LAB_EDIT_BYTES as usize;
    let zero = vec![0u8; n];
    // ten steps of two bytes each
    for i in 0..10u8 {
        let mut b = zero.clone();
        b[0] = i + 1;
        b[1] = i + 1;
        let mut before = zero.clone();
        before[0] = i;
        before[1] = i;
        assert_eq!(
            unsafe { lab_edit_push(&mut *e, before.as_ptr(), b.as_ptr(), 0) },
            1
        );
    }
    assert_eq!((e.steps, e.n), (10, 20));
    // a step of every byte: room for it is made by dropping the oldest whole steps
    let all = vec![0xAAu8; n];
    unsafe { lab_edit_push(&mut *e, zero.as_ptr(), all.as_ptr(), 0) };
    assert_eq!(
        (u32::from(e.steps), u32::from(e.n)),
        (2, 2 + LAB_EDIT_BYTES),
        "the newest small step and the big one"
    );
    // a merge adds only what the newest step does not hold, keeping its old values
    let mut more = all.clone();
    more[3] = 0x55;
    assert_eq!(
        unsafe { lab_edit_push(&mut *e, all.as_ptr(), more.as_ptr(), 1) },
        1
    );
    assert_eq!(
        u32::from(e.n),
        2 + LAB_EDIT_BYTES,
        "nothing new: byte 3 is in the step"
    );
    // nothing changed and no merge: no step
    assert_eq!(
        unsafe { lab_edit_push(&mut *e, all.as_ptr(), all.as_ptr(), 0) },
        0
    );
    assert_eq!(e.steps, 2);
    // every entry's place is a byte of the image, the steps' first marked
    let firsts = (0..e.n as usize)
        .filter(|k| {
            let i = (e.first as usize + k) % LAB_EDIT_ENTRIES as usize;
            e.at[i] & LAB_EDIT_FIRST as u8 != 0
        })
        .count();
    assert_eq!(firsts, 2);
}

#[test]
fn an_algorithm_run_undoes_as_one_step() {
    let mut ui = lab_ui();
    let before = image(&ui);
    for _ in 0..3 {
        ui.turn(PANEL_ENC_ALGORITHM, 1);
    }
    assert_ne!(choice(&ui), "SAW");
    assert_eq!(steps(&ui), 1);
    undo(&mut ui);
    assert_eq!(image(&ui), before);
    assert_eq!(choice(&ui), "SAW");
    // ENVELOPE's choice is the envelope mode, the instrument's: undone too
    to_page(&mut ui, LAB_PAGE_ENVELOPE);
    let mode = ui.live().envelope_mode;
    ui.turn(PANEL_ENC_ALGORITHM, if mode == 0 { 1 } else { -1 });
    assert_ne!(ui.live().envelope_mode, mode);
    undo(&mut ui);
    assert_eq!(ui.live().envelope_mode, mode);
}

#[test]
fn a_revert_and_an_ab_edit_undo_as_one_step() {
    let mut ui = edited_lab();
    let edited = image(&ui);
    ui.hold(EDIT, true);
    ui.turn(PANEL_ENC_PRESETS, 1); // the revert
    ui.hold(EDIT, false);
    assert_eq!(attack(&ui), base_attack(&ui), "reverted");
    assert_eq!(steps(&ui), 2, "the turn and the revert");
    undo(&mut ui);
    assert_eq!(image(&ui), edited, "the revert undone");
    // A becomes the edit: B's difference is a step, and the turn on A another
    ui.press(EDIT);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_A);
    let a = image(&ui);
    ui.turn(K1 + 1, 1);
    assert_eq!(steps(&ui), 3, "the first turn, B's step, the turn on A");
    undo(&mut ui);
    assert_eq!(image(&ui), a, "the turn on A undone");
    undo(&mut ui);
    assert_eq!(image(&ui), edited, "and B back");
    // undo while A plays gives B back at once
    ui.press(EDIT);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_A);
    undo(&mut ui);
    assert_eq!(u32::from(ui.v.lab.ab), LAB_VIEW_AB_OFF);
    assert_eq!(image(&ui), edited);
}

#[test]
fn undo_after_a_load_says_nothing_to_undo() {
    let mut ui = edited_lab();
    assert_eq!(steps(&ui), 1);
    ui.turn(PANEL_ENC_PRESETS, 1); // PWM BASS
    step(&mut ui);
    let loaded = image(&ui);
    assert_eq!(undo(&mut ui).as_deref(), Some("NOTHING TO UNDO"));
    assert_eq!(image(&ui), loaded);
    // a program change (MIDI) is a load too
    ui.turn(K1, 1);
    assert_eq!(steps(&ui), 1);
    unsafe { live_set_preset(&mut *ui.rig.live, 0) };
    assert_eq!(undo(&mut ui).as_deref(), Some("NOTHING TO UNDO"));
}

#[test]
fn undo_survives_home_and_back() {
    let mut ui = edited_lab();
    let before = {
        let mut b = image(&ui);
        b[LAB_SOUND_AD as usize] = ui.live().base[LAB_SOUND_AD as usize];
        b
    };
    ui.press(PANEL_BTN_HOME);
    settle(&mut ui);
    assert_eq!(ui.top(), "LIVE");
    open_lab(&mut ui);
    assert_eq!(undo(&mut ui).as_deref(), Some("UNDO (0)"));
    assert_eq!(image(&ui), before);
}

#[test]
fn lab_presets_loads_and_resets_undo() {
    let mut k = Stored::new();
    k.put(2, "U THREE", 6, 3);
    to_page(&mut k.ui, LAB_PAGE_ENVELOPE);
    k.ui.turn(K1, 2);
    k.ui.turn(K1 + 1, 2);
    assert_eq!(steps(&k.ui), 2);
    k.presets(5); // in LAB: the USER sound loads
    k.frame();
    assert_eq!(k.ui.live().user_sound, 3);
    assert_eq!(k.attack(), 6);
    assert_eq!(undo(&mut k.ui).as_deref(), Some("NOTHING TO UNDO"));
    assert_eq!(k.attack(), 6, "the loaded sound stays");
    // its own edits undo again
    k.ui.turn(K1, 1);
    assert_eq!(undo(&mut k.ui).as_deref(), Some("UNDO (0)"));
    assert_eq!(k.attack(), 6);
}

/// LAB over the views with MIDI learn wired (the frame composes LAB under it).
fn learn_ui(preset: Option<u8>) -> (Ui, Box<midi_learn>) {
    let mut ui = Ui::without_cards();
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *ui.v, &mut *ml);
        if let Some(p) = preset {
            live_set_preset(&mut *ui.rig.live, p);
        }
    }
    open_lab(&mut ui);
    (ui, ml)
}

/// SAVE held + each card on each page: learn opens on the card's parameter (FX: the selected
/// slot's control), over LAB; the two cards with none say so and open nothing.
#[test]
fn save_and_each_card_learns_its_parameter() {
    let slot = |s: u32, f: u32| LIVE_PARAM_SLOT + s * LIVE_SLOT_FIELDS + f;
    let none = LIVE_PARAM_COUNT;
    let table: [(u32, [u32; 4]); 5] = [
        (
            LAB_PAGE_OSC,
            [
                LIVE_PARAM_PULSE_WIDTH,
                LIVE_PARAM_PWM_DEPTH,
                LIVE_PARAM_PWM_RATE,
                LIVE_PARAM_TUNE,
            ],
        ),
        (
            LAB_PAGE_ENVELOPE,
            [
                LIVE_PARAM_ATTACK,
                LIVE_PARAM_DECAY,
                LIVE_PARAM_SUSTAIN,
                LIVE_PARAM_RELEASE,
            ],
        ),
        (
            LAB_PAGE_FILTER,
            [
                LIVE_PARAM_CUTOFF,
                LIVE_PARAM_RESONANCE,
                LIVE_PARAM_FILTER_VOICES,
                LIVE_PARAM_FILTER_LFO,
            ],
        ),
        (
            LAB_PAGE_MOD,
            [
                LIVE_PARAM_VIBRATO_RATE,
                LIVE_PARAM_VIBRATO_DEPTH,
                LIVE_PARAM_VIBRATO_DELAY,
                LIVE_PARAM_ARP_RATE,
            ],
        ),
        (
            LAB_PAGE_FX,
            [
                slot(1, LIVE_SLOT_AMOUNT),
                slot(1, LIVE_SLOT_A),
                slot(1, LIVE_SLOT_B),
                slot(1, LIVE_SLOT_MIX),
            ],
        ),
    ];
    for (page, params) in table {
        for (k, want) in params.into_iter().enumerate() {
            let (mut ui, ml) = learn_ui(Some(1));
            ui.rig.live.fx_select = 1;
            to_page(&mut ui, page);
            ui.learn_on(K1 + k as u32);
            step(&mut ui);
            if want == none {
                assert_eq!(ui.top(), "LAB", "page {page} K{}: nothing to learn", k + 1);
                assert_eq!(ml.state as u32, MIDI_LEARN_IDLE);
                assert_eq!(ui.toast().as_deref(), Some("K4: NOTHING"));
                continue;
            }
            assert_eq!(ui.top(), "LEARN", "page {page} K{}", k + 1);
            assert_eq!(u32::from(ml.param), want, "page {page} K{}", k + 1);
            assert_eq!(
                ui.big(LEARN_VIEW_ROW as usize, 1, 2, 3),
                "LEA",
                "the frame, over LAB"
            );
            ui.press(PANEL_BTN_HOME);
            assert_eq!(ui.top(), "LAB", "HOME cancels and stays");
        }
    }
}

/// Learn's frame and toast name each LAB parameter, the frame within its 14 characters.
#[test]
fn learn_names_every_lab_card() {
    let names = [
        (LIVE_PARAM_PWM_DEPTH, "PWM DEPTH"),
        (LIVE_PARAM_PWM_RATE, "PWM RATE"),
        (LIVE_PARAM_TUNE, "TUNE"),
        (LIVE_PARAM_FILTER_VOICES, "VOICES"),
        (LIVE_PARAM_VELOCITY_TARGET, "VELOCITY"),
        (LIVE_PARAM_VIBRATO_RATE, "VIB RATE"),
        (LIVE_PARAM_VIBRATO_DEPTH, "VIB AMT"),
        (LIVE_PARAM_VIBRATO_DELAY, "VIB DLY"),
        (LIVE_PARAM_FILTER_LFO, "FILT LFO"),
        (LIVE_PARAM_ARP_RATE, "ARP RATE"),
    ];
    for (p, name) in names {
        let mut buf = [0 as std::os::raw::c_char; LEARN_VIEW_NAME as usize];
        unsafe { learn_view_param_name(p, buf.as_mut_ptr()) };
        let got = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy();
        assert_eq!(got, name);
    }
}

/// A CC learned on PWM depth moves the card and OSC's band (the drawing above the cards).
#[test]
fn a_learned_cc_turns_the_card_and_the_drawing() {
    let (mut ui, mut ml) = learn_ui(Some(1));
    ui.learn_on(K1 + 1);
    step(&mut ui);
    assert_eq!(u32::from(ml.param), LIVE_PARAM_PWM_DEPTH);
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
            );
        }
    };
    set(&mut ui, LIVE_PARAM_PWM_DEPTH, 0);
    route(&mut ui, &mut ml, 0);
    settle(&mut ui);
    assert_eq!(ui.top(), "LAB", "the frame closed");
    assert_eq!(unsafe { midi_learn_cc_of(&*ml, LIVE_PARAM_PWM_DEPTH) }, cc);
    let cols = GRID_COLS as usize;
    let area = |ui: &Ui| ui.g.ch[3 * cols..20 * cols].to_vec(); // the band: the canvas's glyphs
    let value = |ui: &Ui| {
        ui.g.src[LAB_VIEW_VALUE_ROW as usize * cols..(LAB_VIEW_VALUE_ROW as usize + 2) * cols]
            .to_vec()
    };
    let (drawing, card) = (area(&ui), value(&ui));
    route(&mut ui, &mut ml, 100);
    settle(&mut ui);
    assert!(
        param(&ui, LIVE_PARAM_PWM_DEPTH) > 0,
        "the CC moved the parameter"
    );
    assert_ne!(value(&ui), card, "the card moved");
    assert_ne!(area(&ui), drawing, "and the band");
}
