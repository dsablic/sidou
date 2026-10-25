// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! LAB (app/lab_view.c) over the views (app/views.c): opened from the menu's LAB, its five pages
//! (SELECT), the four cards each page turns by its detents, ALGORITHM's choice on each page, the
//! card or choice lit in place of a popup, the header, the hint line, ENV, LFO and FX as pages,
//! HOME back to LIVE, the frame's budget and the pages' goldens.

use std::ffi::CStr;

use sidou_sys::*;

use crate::sid_view::golden;
use crate::views::Ui;
use crate::{drain, Sent};

const K1: u32 = PANEL_ENC_K1;
/// app/panel.h PANEL_PAGE_LAB_FIRST: after the EXPERT pages.
const LAB_FIRST: u32 = PANEL_PAGE_EXPERT_FIRST + (SID_EXPERT_COUNT + 3) / 4;

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
fn select_steps_the_five_pages_and_stops_at_the_ends() {
    let mut ui = lab_ui();
    let names = ["OSC", "ENVELOPE", "FILTER", "MOD", "FX"];
    assert_eq!(ui.v.lab.page, 0, "LAB opens at OSC");
    for (i, name) in names.iter().enumerate() {
        settle(&mut ui);
        assert_eq!(usize::from(ui.v.lab.page), i);
        assert_eq!(ui.big(0, 0, 2, name.len()), *name, "the page's name at 2x");
        assert_eq!(ui.big(0, 24, 2, 3), format!("{}/5", i + 1), "its place");
        ui.turn(PANEL_ENC_SELECT, 1);
    }
    assert_eq!(ui.v.lab.page, 4, "clamped at FX");
    ui.turn(PANEL_ENC_SELECT, -9);
    assert_eq!(ui.v.lab.page, 0, "clamped at OSC");
}

/// Each card, a detent each way: its parameter moves by the card's detent (from a value with room
/// both ways), and back.
#[test]
fn each_card_turns_its_parameter_by_its_detent() {
    // (page, knob, parameter, a start, the detent)
    let table: [(u32, u32, u32, u32, i32); 16] = [
        (LAB_PAGE_OSC, 1, LIVE_PARAM_PWM_DEPTH, 640, 32),
        (LAB_PAGE_OSC, 2, LIVE_PARAM_PWM_RATE, 10, 1),
        (LAB_PAGE_OSC, 3, LIVE_PARAM_TUNE, 50, 1),
        (LAB_PAGE_ENVELOPE, 0, LIVE_PARAM_ATTACK, 4, 1),
        (LAB_PAGE_ENVELOPE, 1, LIVE_PARAM_DECAY, 4, 1),
        (LAB_PAGE_ENVELOPE, 2, LIVE_PARAM_SUSTAIN, 4, 1),
        (LAB_PAGE_ENVELOPE, 3, LIVE_PARAM_RELEASE, 4, 1),
        (LAB_PAGE_FILTER, 1, LIVE_PARAM_RESONANCE, 6, 1),
        (LAB_PAGE_FILTER, 2, LIVE_PARAM_FILTER_VOICES, 3, 1),
        (LAB_PAGE_MOD, 0, LIVE_PARAM_VIBRATO_RATE, 60, 1),
        (LAB_PAGE_MOD, 1, LIVE_PARAM_VIBRATO_DEPTH, 20, 1),
        (LAB_PAGE_MOD, 2, LIVE_PARAM_VIBRATO_DELAY, 20, 1),
        (LAB_PAGE_MOD, 3, LIVE_PARAM_VELOCITY_TARGET, 1, 1),
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
        let mut on_page = (*ui.p).clone();
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
    // FILTER's K4 is free: nothing turns, nothing is named.
    let snapshot = (0..LIVE_PARAM_COUNT)
        .map(|p| param(&ui, p))
        .collect::<Vec<_>>();
    ui.turn(K1 + 3, 3);
    assert_eq!(
        (0..LIVE_PARAM_COUNT)
            .map(|p| param(&ui, p))
            .collect::<Vec<_>>(),
        snapshot,
        "FILTER's K4 turns nothing"
    );
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
    // Consumed: the panel never saw them (ENV's next algorithm, FX's chip toggle).
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

/// The five pages on SAW LEAD as it loads (FX with OVERDRIVE on the first slot).
#[test]
fn the_pages_match_their_goldens() {
    let mut ui = lab_ui();
    for (page, name) in [
        (LAB_PAGE_OSC, "lab-osc"),
        (LAB_PAGE_ENVELOPE, "lab-envelope"),
        (LAB_PAGE_FILTER, "lab-filter"),
        (LAB_PAGE_MOD, "lab-mod"),
        (LAB_PAGE_FX, "lab-fx"),
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

/// On to the hint line's first phrase, settled.
pub(crate) fn settle_hint(ui: &mut Ui) {
    for _ in 0..(LAB_VIEW_HINTS * LAB_VIEW_HINT_MS / 40 + 2) {
        if ui.v.s.now % (LAB_VIEW_HINTS * LAB_VIEW_HINT_MS) < LAB_VIEW_HINT_MS - 400 {
            break;
        }
        step(ui);
    }
    settle(ui);
}
