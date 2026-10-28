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
