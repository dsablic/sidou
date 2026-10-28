// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The value popup (app/screen.h, app/views.c popup_text) and the large toast: what a control's
//! change shows, how long it stays, what it covers and gives back, and what it costs a frame.

use std::ffi::CStr;

use sidou_sys::*;

use crate::views::{open_status, Ui};
use crate::{drain, SPI_BUDGET_MS};

const COLS: usize = GRID_COLS as usize;
const K2: u32 = PANEL_ENC_K1 + 1;
const K3: u32 = PANEL_ENC_K1 + 2;
const ROW: usize = SCREEN_POPUP_ROW as usize;
/// The box's rows: the name's from row 1 of it, the value's from row 4, the bar's row 7.
const NAME: usize = ROW + 1;
const VALUE: usize = ROW + 4;
const BAR: usize = ROW + 7;

/// A Ui over STATUS, which keeps its value popups (LIVE and CHIP highlight their knob instead).
fn status_ui() -> Ui {
    let mut ui = Ui::without_cards();
    ui.settle();
    open_status(&mut ui);
    ui.settle();
    ui
}

/// The MASTER pot moved to `raw`, which opens its popup over any view.
fn master(ui: &mut Ui, raw: i32) {
    unsafe {
        views_set_panel(&mut *ui.v, &mut *ui.p);
        panel_master(&mut *ui.p, &mut *ui.rig.live, raw);
    }
}

fn shown(ui: &Ui) -> bool {
    unsafe { screen_popup_shows(&ui.v.s) != 0 }
}

fn rows(ui: &Ui) -> usize {
    usize::from(ui.v.popup.rows)
}

/// The popup's name and value as it says them, from the first cell of its text.
fn says(ui: &Ui) -> (String, String) {
    let at = |r: usize| ui.row(r)[3..].split('#').next().unwrap().trim().to_owned();
    (at(NAME), at(VALUE))
}

/// Frames until the popup is whole (3 rows a frame, after any raster or wipe that runs).
fn open(ui: &mut Ui) {
    for _ in 0..40 {
        ui.frame();
        if rows(ui) == SCREEN_POPUP_ROWS as usize {
            return;
        }
    }
    panic!("the popup never opened: {} rows", rows(ui));
}

/// A row's text from column 2 on (the toast's first column).
fn line(ui: &Ui, row: usize) -> String {
    ui.row(row)[2..].to_string()
}

fn text_of(f: unsafe extern "C" fn(*const live, *mut std::ffi::c_char), l: &live) -> String {
    let mut buf = [0 as std::ffi::c_char; 16];
    unsafe { f(l, buf.as_mut_ptr()) };
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn cutoff_text(ui: &Ui) -> String {
    text_of(live_view_cutoff_text, ui.live())
}

#[test]
fn expert_formatter_uses_the_core_ranges_and_noise_tap_bands() {
    let percent = |value: f64| format!("{}%", (value * 100.0).round() as u32);
    for (index, raw, expected) in [
        (
            SID_EXPERT_FILTER_6581_CURVE,
            0,
            percent(SID_EXPERT_CURVE_6581_MIN),
        ),
        (
            SID_EXPERT_FILTER_6581_CURVE,
            127,
            percent(SID_EXPERT_CURVE_6581_MAX),
        ),
        (
            SID_EXPERT_FILTER_6581_RANGE,
            0,
            percent(SID_EXPERT_RANGE_6581_MIN),
        ),
        (SID_EXPERT_NOISE_WRITEBACK, 0, "OFF".to_owned()),
        (SID_EXPERT_NOISE_WRITEBACK, 1, "1 TAP".to_owned()),
        (SID_EXPERT_NOISE_WRITEBACK, 15, "1 TAP".to_owned()),
        (SID_EXPERT_NOISE_WRITEBACK, 16, "2 TAPS".to_owned()),
        (SID_EXPERT_NOISE_WRITEBACK, 127, "8 TAPS".to_owned()),
    ] {
        let mut text = [0; SCREEN_POPUP_TEXT as usize];
        unsafe { chip_view_expert_text(index, raw, text.as_mut_ptr()) };
        let actual = unsafe { CStr::from_ptr(text.as_ptr()) }.to_string_lossy();
        assert_eq!(actual, expected, "expert {index}, raw {raw}");
    }
}

#[test]
fn every_expert_knob_popup_has_its_name_and_physical_units() {
    let mut ui = Ui::without_cards();
    ui.settle();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CHIP as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, 4);
    ui.turn(PANEL_ENC_K1 + 2, 1);
    open(&mut ui);
    assert_eq!(says(&ui), ("8580 CURVE".to_owned(), "N/A".to_owned()));
    ui.popup_gone();
    let curve_max = format!("{}%", (SID_EXPERT_CURVE_6581_MAX * 100.0).round() as u32);
    for (index, (name, value)) in [
        ("6581 CURVE", curve_max.as_str()),
        ("6581 RANGE", "100%"),
        ("8580 CURVE", "100%"),
        ("NONLINEARITY", "200%"),
        ("FILTER BIAS", "2.31DC"),
        ("FILTER BLEED", "50%"),
        ("DAC KINK", "2.60R"),
        ("DAC LEAKAGE", "15.00%"),
        ("COMBINED", "STRONG"),
        ("NOISE WRITE", "8 TAPS"),
        ("OUTPUT THUMP", "1.00DC"),
        ("EXTERNAL LP", "20000HZ"),
        ("EXTERNAL HP", "100HZ"),
        ("VOICE DC", "0.60DC"),
    ]
    .into_iter()
    .enumerate()
    {
        if index == SID_EXPERT_FILTER_8580_CURVE as usize {
            unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_MODEL, 1) };
        } else if index == SID_EXPERT_NONLINEARITY as usize {
            unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_MODEL, 0) };
        }
        if index > 0 && index % 4 == 0 {
            ui.turn(PANEL_ENC_ALGORITHM, 1);
            ui.popup_gone();
        }
        let param = LIVE_PARAM_EXPERT + index as u32;
        unsafe { live_param_set(&mut *ui.rig.live, param, 126) };
        let knob = index % 4;
        ui.turn(PANEL_ENC_K1 + knob as u32, 1);
        open(&mut ui);
        assert_eq!(
            says(&ui),
            (name.to_owned(), value.to_owned()),
            "expert {index}"
        );
        let t = unsafe { &*theme_get(u32::from(ui.live().theme)) };
        assert_eq!(
            ui.v.popup.accent, t.knob[knob],
            "the physical knob's colour"
        );
        ui.popup_gone();
    }
    ui.turn(PANEL_ENC_K4, 1);
    ui.frame();
    assert!(
        !shown(&ui),
        "the unused final-page knob has no preset popup"
    );
}

#[test]
fn the_midi_page_names_each_edited_value_in_the_popup() {
    let mut ui = Ui::without_cards();
    let mut route = Box::<midi_route>::default();
    let mut sync = Box::<midi_sync>::default();
    unsafe {
        midi_route_init(
            &mut *route,
            &mut *ui.rig.live,
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        midi_sync_init(&mut *sync);
        views_route(&mut *ui.v, &mut *route);
        views_sync(&mut *ui.v, &mut *sync);
    }
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_MIDI as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    for (i, (name, value)) in [
        ("MODE", "OMNI"),
        ("CHANNEL", "2"),
        ("OPTION", "CHORD"),
        ("LEGATO", "ON"),
        ("CHORD 2", "+5"),
        ("CHORD 3", "+8"),
        ("ARP RATE", "4 FR"),
        ("ROWS/BEAT", "5"),
        ("CLOCK", "FOLLOW"),
        ("CLOCK OUT", "ON"),
    ]
    .into_iter()
    .enumerate()
    {
        if i != 0 {
            ui.turn(PANEL_ENC_SELECT, 1);
        }
        ui.turn(PANEL_ENC_ALGORITHM, 1);
        open(&mut ui);
        assert_eq!(says(&ui), (name.to_owned(), value.to_owned()), "item {i}");
        ui.popup_gone();
    }
}

fn grid_copy(ui: &Ui) -> (Vec<u8>, Vec<u8>) {
    (ui.g.ch.to_vec(), ui.g.attr.to_vec())
}

#[test]
fn a_knob_turn_opens_the_popup_three_rows_a_frame_then_closes_a_second_after_the_last_change() {
    let mut ui = status_ui();
    assert!(!shown(&ui));
    ui.turn(PANEL_ENC_K1, 3);
    let mut open_rows = Vec::new();
    for _ in 0..4 {
        ui.frame();
        open_rows.push(rows(&ui));
    }
    assert_eq!(
        open_rows,
        [0, 3, 6, 9],
        "the frame that asks opens nothing, then three rows a frame, top first"
    );
    let (name, value) = says(&ui);
    assert_eq!(name, "CUTOFF");
    assert_eq!(value, cutoff_text(&ui));
    // The bar, in the cutoff's share of 24 cells of 8 pixels.
    let px = u32::from(ui.live().cutoff) * 192 / LIVE_CUTOFF_MAX;
    let bar: Vec<u32> = (0..24)
        .map(|i| u32::from(ui.g.src[BAR * COLS + 3 + i]) - FONT_H0)
        .collect();
    assert_eq!(bar.iter().sum::<u32>(), px, "{bar:?}");
    // It stays while the control keeps changing: another turn every 15 frames for a minute.
    for n in 0..60 {
        if n % 15 == 0 {
            ui.turn(PANEL_ENC_K1, 1);
        }
        ui.frame();
        assert_eq!(rows(&ui), 9, "frame {n}");
    }
    let still_changing = ui.live().cutoff;
    assert_eq!(says(&ui).1, cutoff_text(&ui));
    // The last change, then SCREEN_POPUP_MS (25 frames) and the closing, 3 rows a frame.
    let mut shown_for = 0;
    let mut closing = Vec::new();
    for _ in 0..40 {
        ui.frame();
        if rows(&ui) == 9 {
            shown_for += 1;
        } else if rows(&ui) > 0 {
            closing.push(rows(&ui));
        }
    }
    assert!(ui.live().cutoff == still_changing);
    assert!(
        (24..=26).contains(&(shown_for + 14)),
        "{shown_for} more frames whole after the turn at 45"
    );
    assert_eq!(closing, [6, 3], "bottom first, three rows a frame");
    assert!(!shown(&ui));
    assert_eq!(ui.v.s.layers, 0, "no layer left to run");
}

#[test]
fn a_change_restores_the_view_beneath_cell_for_cell_once_the_popup_is_gone() {
    // The popup draws over rows 10..18; its closing hands them back to the view, which
    // composes them again. Each view then shows what it shows with no popup: the same grid
    // after a forced compose of its own.
    for view in ["LIVE", "STATUS", "MENU", "CHARSET", "CHIP"] {
        let mut ui = Ui::without_cards();
        ui.settle();
        match view {
            "STATUS" => open_status(&mut ui),
            "MENU" => {
                ui.press(PANEL_BTN_HOME);
                ui.settle();
            }
            "CHARSET" => {
                ui.press(PANEL_BTN_HOME);
                ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CHARSET as i32);
                ui.turn(PANEL_ENC_ALGORITHM, 1);
                ui.settle();
            }
            "CHIP" => {
                ui.press(PANEL_BTN_HOME);
                ui.turn(PANEL_ENC_ALGORITHM, 1);
                ui.settle();
            }
            _ => {}
        }
        drain(&mut ui.g);
        if view == "LIVE" || view == "CHIP" {
            // Their knobs highlight instead; the pot's popup stays.
            master(&mut ui, 300);
            ui.frame();
            master(&mut ui, 600);
        } else {
            ui.turn(K2, 2);
        }
        open(&mut ui);
        assert!(shown(&ui), "{view}");
        drain(&mut ui.g);
        ui.popup_gone();
        for _ in 0..4 {
            ui.frame();
            drain(&mut ui.g);
        }
        let after = grid_copy(&ui);
        unsafe { screen_redraw(&mut ui.v.s) };
        for _ in 0..4 {
            ui.frame();
            drain(&mut ui.g);
        }
        let again = grid_copy(&ui);
        assert!(
            after == again,
            "{view}: the view beneath was not given back whole"
        );
    }
}

#[test]
fn select_never_asks_and_nor_does_a_turn_with_nothing_to_show() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, 2);
    assert_eq!(ui.v.s.popup_ask, 0);
    for _ in 0..4 {
        ui.frame();
    }
    assert!(!shown(&ui));
    // ALGORITHM on an item that opens a view opens it, with no popup; PRESETS (encoder 6) steps
    // the preset and opens its popup, over any view but LIVE.
    ui.turn(PANEL_ENC_SELECT, -2);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert_eq!(ui.top(), "CHIP");
    ui.turn(PANEL_ENC_PRESETS, 1);
    open(&mut ui);
    assert_eq!(says(&ui).0, "PRESET");
    assert_eq!(ui.live().preset, 1);
}

#[test]
fn midi_changes_never_open_the_popup() {
    let mut ui = status_ui();
    for n in 0..60u32 {
        // What MIDI does to the instrument: controllers, program changes and the volume.
        ui.rig.cc(74, (n * 2 % 128) as u8);
        ui.rig.cc(71, (n % 128) as u8);
        unsafe {
            live_param_set(&mut *ui.rig.live, LIVE_PARAM_PULSE_WIDTH, n * 50);
            live_param_set(&mut *ui.rig.live, LIVE_PARAM_AGE, n);
        }
        ui.rig.set_preset((n % 4) as u8);
        ui.rig.set_master((n * 9 % 1024) as u16);
        ui.frame();
        assert!(!shown(&ui), "frame {n}");
        assert_eq!(ui.v.s.popup_ask, 0);
    }
    // The panel's own turn does open it: the one switch is where the asks come from.
    ui.turn(PANEL_ENC_K1, 1);
    ui.frame();
    assert!(shown(&ui));
}

#[test]
fn the_midi_switch_is_one_call_and_off_by_default() {
    assert_eq!(VIEWS_POPUP_MIDI, 0, "the default");
    let mut ui = Ui::without_cards();
    ui.settle();
    // On: a parameter MIDI moved opens the popup with its name and value.
    unsafe { views_popup_midi(&mut *ui.v, 1) };
    ui.frame();
    assert!(!shown(&ui), "nothing moved yet");
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_RESONANCE, 9) };
    open(&mut ui);
    assert_eq!(says(&ui), ("RESO".to_owned(), "9".to_owned()));
    // A panel knob in the same frame names itself, not the watcher's.
    ui.popup_gone();
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_CUTOFF, 100) };
    ui.turn(PANEL_ENC_K1, 1);
    open(&mut ui);
    assert_eq!(says(&ui).0, "CUTOFF");
    // Off again: MIDI is silent as before, and the screen has no layer to run.
    ui.popup_gone();
    unsafe { views_popup_midi(&mut *ui.v, 0) };
    unsafe { live_param_set(&mut *ui.rig.live, LIVE_PARAM_PULSE_WIDTH, 777) };
    for _ in 0..6 {
        ui.frame();
    }
    assert!(!shown(&ui));
    assert_eq!(ui.v.s.layers, 0);
}

#[test]
fn the_master_pot_asks_for_the_volume_when_it_moves() {
    let mut ui = Ui::without_cards();
    ui.settle();
    unsafe {
        panel_master(&mut *ui.p, &mut *ui.rig.live, 500);
        views_set_panel(&mut *ui.v, &mut *ui.p);
    }
    for _ in 0..4 {
        ui.frame();
    }
    assert!(!shown(&ui), "a pot at rest asks nothing");
    unsafe { panel_master(&mut *ui.p, &mut *ui.rig.live, 502) };
    ui.frame();
    assert!(!shown(&ui), "inside the hysteresis");
    unsafe { panel_master(&mut *ui.p, &mut *ui.rig.live, 700) };
    open(&mut ui);
    let want = u32::from(ui.live().volume);
    assert_eq!(want, 68);
    assert_eq!(says(&ui), ("VOLUME".to_owned(), format!("{want}%")));
    let px = u32::from(ui.live().master) * 192 / LIVE_MASTER_MAX;
    let bar: u32 = (0..24)
        .map(|i| u32::from(ui.g.src[BAR * COLS + 3 + i]) - FONT_H0)
        .sum();
    assert_eq!(bar, px);
    // the pot at rest at 0 for a few ticks: the next ask waits PANEL_MASTER_ASK_GAP ticks
    for _ in 0..PANEL_MASTER_ASK_GAP {
        unsafe { panel_master(&mut *ui.p, &mut *ui.rig.live, 0) };
    }
    ui.frame();
    assert_eq!(says(&ui), ("VOLUME".to_owned(), "0%".to_owned()));
}

#[test]
fn every_knob_and_algorithm_shows_its_name_and_value_as_the_views_say_them() {
    let mut ui = status_ui();
    ui.turn(K2, 4);
    open(&mut ui);
    assert_eq!(
        says(&ui),
        ("RESO".to_owned(), ui.live().resonance.to_string())
    );
    // K3 is the sound's own control: the decay on SAW LEAD, the pulse width on PWM BASS, the sync
    // ratio on SYNC LEAD; K4 the release.
    ui.turn(K3, 2);
    ui.frame();
    assert_eq!(says(&ui), ("DECAY".to_owned(), "11".to_owned()));
    ui.rig.set_preset(1);
    ui.turn(K3, -2);
    ui.frame();
    let pc = (u32::from(ui.live().pulse_width) * 99 + 2047) / 4095;
    assert_eq!(says(&ui), ("PULSE".to_owned(), format!("{pc}%")));
    ui.rig.set_preset(2);
    ui.turn(K3, 3);
    ui.frame();
    assert_eq!(says(&ui), ("RATIO".to_owned(), "+3".to_owned()));
    ui.turn(PANEL_ENC_K4, 2);
    ui.frame();
    assert_eq!(says(&ui), ("RELEASE".to_owned(), "10".to_owned()));
    ui.rig.set_preset(0);
    // PRESETS: the preset, by name, with no bar.
    ui.turn(PANEL_ENC_PRESETS, 1);
    ui.frame();
    let p = unsafe { &*live_preset_get(ui.live().preset) };
    let name = unsafe { CStr::from_ptr(p.name) }
        .to_string_lossy()
        .into_owned();
    assert_eq!(says(&ui), ("PRESET".to_owned(), name));
    assert!(
        (0..24).all(|i| ui.g.src[BAR * COLS + 3 + i] == b' '),
        "no bar for a choice"
    );
    // EDIT held: the selected slot's Amount, in the algorithm's own word.
    unsafe { live_slot_set(&mut *ui.rig.live, 0, LIVE_SLOT_ALGORITHM, 2) };
    ui.down(PANEL_BTN_EDIT);
    ui.turn(PANEL_ENC_K1, 3);
    ui.frame();
    let mut buf = [0 as std::ffi::c_char; CHIP_VIEW_LABEL as usize];
    unsafe { chip_view_label(0, 2, LIVE_SLOT_AMOUNT, buf.as_mut_ptr()) };
    let label = unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    let amount = ui.live().slot[0][LIVE_SLOT_AMOUNT as usize];
    assert_eq!(says(&ui), (format!("DIST {label}"), amount.to_string()));
    assert_eq!(amount, 12, "a detent is 4");
    ui.up(PANEL_BTN_EDIT);
    // ENV (a button) toasts; the popup is for the knobs.
    ui.popup_gone();
    ui.press(PANEL_BTN_ENV);
    ui.frame();
    assert!(!shown(&ui) && ui.toast().is_some());
}

#[test]
fn the_menus_algorithm_shows_the_item_in_full_and_no_toast_says_it_again() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_PRESET as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    open(&mut ui);
    assert_eq!(
        says(&ui).0,
        "PRESET",
        "PRE in the menu, the whole word here"
    );
    assert_eq!(ui.live().preset, 1);
    ui.popup_gone();
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    open(&mut ui);
    assert_eq!(says(&ui), ("OCTAVE".to_owned(), "+1".to_owned()));
    assert_eq!(ui.toast(), None, "the popup says it: no toast over it");
    ui.popup_gone();
    crate::views::open_setting(&mut ui, VIEWS_SETTINGS_ROWS);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    open(&mut ui);
    assert_eq!(says(&ui), ("TRACKER ROWS".to_owned(), "SCROLL".to_owned()));
    // The two-chip item names fit the largest text or the next size down: 12 characters at 2x.
    assert_eq!(
        ui.g.ch[NAME * COLS + 3],
        GRID_BIG_FIRST as u8,
        "2 times the font"
    );
}

#[test]
fn the_chip_view_highlights_its_knobs_and_shows_its_choice_with_no_popup() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert_eq!(ui.top(), "CHIP");
    // ALGORITHM: the profile (or the slot's algorithm) shows at 3x on the page: no popup, no toast.
    let profile = ui.live().profile_req;
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    for _ in 0..4 {
        ui.frame();
    }
    assert_ne!(ui.live().profile_req, profile);
    assert!(!shown(&ui) && ui.toast().is_none());
    ui.turn(PANEL_ENC_SELECT, 1); // DIST
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    for _ in 0..4 {
        ui.frame();
    }
    assert_eq!(ui.live().slot[0][LIVE_SLOT_ALGORITHM as usize], 1);
    assert!(!shown(&ui) && ui.toast().is_none());
    // K1 on the chip page: the character's first control, its label highlighted.
    ui.turn(PANEL_ENC_SELECT, -1);
    ui.turn(PANEL_ENC_K1, 3);
    for _ in 0..4 {
        ui.frame();
    }
    assert!(!shown(&ui));
    assert_eq!(ui.v.chip.hl, 1);
    assert_eq!(unsafe { live_param_get(&*ui.rig.live, LIVE_PARAM_AGE) }, 12);
    // EDIT + K1 with the focus on the chip turns a slot that is not on the page: its popup.
    ui.down(PANEL_BTN_EDIT);
    ui.turn(PANEL_ENC_K1, 1);
    open(&mut ui);
    assert!(says(&ui).0.starts_with("DIST"), "{:?}", says(&ui));
    ui.up(PANEL_BTN_EDIT);
}

#[test]
fn nothing_opens_under_the_boot_a_title_card_or_midi_learn() {
    let mut ui = Ui::new();
    unsafe { views_boot(&mut *ui.v, 1, 0) };
    ui.frame();
    assert_eq!(ui.top(), "BOOT");
    ui.turn(PANEL_ENC_K1, 1);
    for _ in 0..6 {
        ui.frame();
    }
    assert!(!shown(&ui), "the knob that ended the boot did nothing else");
    assert_eq!(ui.v.s.popup_ask, 0);
    // A title card ends on any control and the control does nothing else.
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.top(), "TITLE");
    ui.turn(PANEL_ENC_K1, 1);
    for _ in 0..8 {
        ui.frame();
    }
    assert!(!shown(&ui));
    // MIDI learn: SAVE held and a knob learns it; its frame replaces a popup at once.
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *ui.v, &mut *ml);
    }
    ui.v.cards = 0;
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_K1, 1);
    open(&mut ui);
    ui.learn_on(PANEL_ENC_K1);
    assert_eq!(ui.top(), "LEARN");
    ui.frame();
    assert!(!shown(&ui), "learn's frame took the popup's place");
    ui.turn(K2, 1);
    for _ in 0..4 {
        ui.frame();
    }
    assert!(!shown(&ui), "a knob under learn picks, it does not show");
}

#[test]
fn a_view_switch_takes_the_popup_away_in_the_next_frame() {
    let mut ui = Ui::without_cards();
    ui.settle();
    master(&mut ui, 300);
    ui.frame();
    master(&mut ui, 600);
    open(&mut ui);
    ui.press(PANEL_BTN_HOME); // the menu, wiped in
    ui.frame(); // the view on top is not the popup's: it goes in the first frame of the wipe
    assert!(!shown(&ui));
    assert_eq!(ui.v.s.layers, 0);
    ui.settle();
    let heading: String = ui
        .big(1, 1, 2, 14)
        .chars()
        .map(|c| if c.is_ascii() { c } else { '#' })
        .collect();
    assert_eq!(heading, format!("MENU   #  1/{VIEWS_MENU_ITEMS}"));
}

#[test]
fn the_popup_covers_its_rows_and_guards_them_from_the_views_large_text() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    let covered = |ui: &Ui, c: u32, r: u32| unsafe { screen_covered(&ui.v.s, c, r) } != 0;
    assert!(!covered(&ui, 5, 12));
    ui.turn(PANEL_ENC_K1, 1);
    ui.frame(); // the ask: the text, no rows yet
    ui.frame(); // three rows, covered
    let covered_now = |ui: &Ui, c: u32, r: u32| unsafe { screen_covered(&ui.v.s, c, r) } != 0;
    assert_eq!(ui.v.popup.rows, 3);
    assert!(
        !covered_now(&ui, 5, 14),
        "only the rows it draws are covered"
    );
    ui.frame();
    assert_eq!(ui.v.popup.rows, 6);
    ui.turn(PANEL_ENC_K1, 1);
    ui.frame();
    let g = &ui.v.s.g;
    assert!(!g.is_null());
    assert_eq!(unsafe { (*ui.v.s.g).big_guard }, ((1u32 << 6) - 1) << ROW);
    ui.frame();
    assert_eq!(unsafe { (*ui.v.s.g).big_guard }, ((1u32 << 9) - 1) << ROW);
    // The menu's own large text under the box was not redrawn over it (the guard): the box's
    // cells hold the popup's pieces, whatever the menu composed there.
    assert_eq!(says(&ui).0, "CUTOFF");
    ui.popup_gone();
    assert_eq!(unsafe { (*ui.v.s.g).big_guard }, 0);
    for _ in 0..4 {
        ui.frame();
    }
    // The menu is whole again under it: its lines read as before (SONG's and LAB's were under
    // the box).
    assert_eq!(
        ui.menu_line(VIEWS_MENU_SONG as usize),
        format!("{:<13}#", "SONG")
    );
    assert_eq!(
        ui.menu_line(VIEWS_MENU_LAB as usize),
        format!("{:<13}#", "LAB")
    );
}

#[test]
fn the_popup_costs_no_frame_more_than_the_budget_over_every_view() {
    for view in ["LIVE", "STATUS", "MENU", "CHARSET", "CHIP", "SCENE"] {
        let mut ui = Ui::without_cards();
        ui.settle();
        match view {
            "STATUS" => open_status(&mut ui),
            "MENU" => {
                ui.press(PANEL_BTN_HOME);
                ui.settle();
            }
            "CHARSET" => {
                ui.press(PANEL_BTN_HOME);
                ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_CHARSET as i32);
                ui.turn(PANEL_ENC_ALGORITHM, 1);
                ui.settle();
            }
            "CHIP" => {
                ui.press(PANEL_BTN_HOME);
                ui.turn(PANEL_ENC_ALGORITHM, 1);
                ui.settle();
            }
            "SCENE" => {
                ui.press(PANEL_BTN_HOME);
                ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SCENE as i32);
                ui.turn(PANEL_ENC_ALGORITHM, 1);
                ui.settle();
            }
            _ => {}
        }
        for _ in 0..80 {
            ui.frame();
            drain(&mut ui.g);
        }
        // A note sounds while the knob turns, a detent every frame for a second, a toast in
        // the middle of it (OCT+), then the popup closes.
        ui.rig.note_on(57);
        let mut worst = (0usize, 0.0f64);
        let mut cells = Vec::new();
        for n in 0..90 {
            if n < 30 {
                ui.turn(PANEL_ENC_K1, if n % 2 == 0 { 7 } else { -5 });
            }
            if n == 12 {
                ui.press(PANEL_BTN_OCT_UP);
            }
            ui.frame();
            let sent = drain(&mut ui.g);
            cells.push(sent.cells);
            worst = (worst.0.max(sent.cells), worst.1.max(sent.spi_ms()));
        }
        println!(
            "popup over {view}: at most {} cells a frame, {:.2} ms of SPI",
            worst.0, worst.1
        );
        assert!(
            worst.0 <= 200,
            "{view}: a frame sent {} cells: {cells:?}",
            worst.0
        );
        assert!(worst.1 <= SPI_BUDGET_MS, "{view}: {:.2} ms of SPI", worst.1);
        ui.popup_gone();
    }
}

#[test]
fn the_toast_is_large_text_on_two_rows_or_four_when_it_breaks_at_a_space() {
    let mut ui = Ui::without_cards();
    ui.settle();
    let row = SCREEN_TOAST_ROW as usize;
    let th = unsafe { &*theme_get(THEME_HIGH_CONTRAST) };
    let attr = (th.toast_bg << 4) | th.toast_fg;
    let toast = |ui: &mut Ui, t: &str| {
        let c = std::ffi::CString::new(t).unwrap();
        unsafe { screen_toast(&mut ui.v.s, c.as_ptr(), 100_000) };
        ui.frame();
    };
    // One line: 13 characters at most, 2 x 2 cells each, rows 27 and 28, in reverse video.
    toast(&mut ui, "SAVED (GAPS)");
    assert!(line(&ui, row).starts_with("SAVED (GAPS)"));
    assert!(line(&ui, row + 1).starts_with("SAVED (GAPS)"));
    assert_eq!(ui.g.attr[row * COLS + 2], attr);
    assert_eq!(ui.g.ch[row * COLS + 2], GRID_BIG_FIRST as u8);
    assert_eq!(ui.g.ch[(row + 1) * COLS + 3], (GRID_BIG_FIRST + 3) as u8);
    // Longer: two lines broken at the last space that fits, rows 25 to 28.
    toast(&mut ui, "NOT SAVED: FOREIGN STORE");
    // Row 25 is LIVE's slot algorithm at 2x: its first letter shows left of the toast.
    assert!(ui.row(row - 2).contains("NOT SAVED:"));
    assert!(line(&ui, row).starts_with("FOREIGN STORE"));
    assert_eq!(ui.v.s.toast_h, 4);
    // Back to one line: the rows above go back to the view's.
    toast(&mut ui, "OCTAVE +1");
    ui.frame();
    assert!(line(&ui, row).starts_with("OCTAVE +1"));
    assert!(!line(&ui, row - 2).contains("NOT SAVED"));
    // Words that cannot break into two lines of 13: ordinary text on one row, as before.
    toast(&mut ui, "SNG: A-4 PITCH NOT PLAYED");
    assert!(line(&ui, row).starts_with("SNG: A-4 PITCH NOT PLAYED"));
    assert_eq!(ui.v.s.toast_h, 1);
    assert_eq!(ui.g.ch[row * COLS + 2], b'S');
}

#[test]
fn a_large_toast_ends_and_gives_its_rows_back() {
    let mut ui = Ui::without_cards();
    open_status(&mut ui);
    let before = grid_copy(&ui);
    unsafe { screen_toast(&mut ui.v.s, c"NOT SAVED: FOREIGN STORE".as_ptr(), 1000) };
    for _ in 0..40 {
        ui.frame();
    }
    assert_eq!(ui.v.s.toast_on, 0);
    assert!(
        before == grid_copy(&ui),
        "the status screen was not given back"
    );
}

#[test]
fn a_delayed_menu_popup_keeps_the_item_that_algorithm_changed() {
    let octave = VIEWS_MENU_PRESET + 1;
    for target in [VIEWS_MENU_SETTINGS, VIEWS_MENU_CHIP] {
        let mut ui = Ui::without_cards();
        ui.press(PANEL_BTN_HOME);
        ui.settle();
        drain(&mut ui.g);
        ui.turn(PANEL_ENC_SELECT, octave as i32);
        ui.turn(PANEL_ENC_ALGORITHM, 1);
        ui.frame();
        drain(&mut ui.g);
        ui.turn(PANEL_ENC_SELECT, target as i32 - octave as i32);
        ui.frame();
        drain(&mut ui.g);
        for _ in 0..4 {
            ui.frame();
            assert!(drain(&mut ui.g).cells <= 176);
        }
        assert_eq!(rows(&ui), SCREEN_POPUP_ROWS as usize);
        assert_eq!(says(&ui), ("OCTAVE".to_owned(), "+1".to_owned()));
    }
}

#[test]
fn a_popup_open_waits_for_a_jump_between_uncovered_menu_bars() {
    let octave = VIEWS_MENU_PRESET + 1;
    let mut ui = Ui::without_cards();
    ui.rig.live.theme = THEME_C64 as u8;
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    ui.turn(PANEL_ENC_SELECT, octave as i32);
    ui.frame();
    drain(&mut ui.g);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.frame();
    drain(&mut ui.g);
    assert_eq!(rows(&ui), 0);
    ui.turn(PANEL_ENC_SELECT, -(octave as i32));
    for frame in 0..4 {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(sent.cells <= 176, "frame {frame}: {sent:?}");
        assert_eq!(
            (0..6)
                .filter(|&k| ui.g.attr[(4 + 3 * k) * COLS] == ui.v.home.sel_attr)
                .count(),
            1
        );
    }
    assert_eq!(
        rows(&ui),
        SCREEN_POPUP_ROWS as usize,
        "fully drawn within four frames"
    );
}

#[test]
fn a_fresh_popup_waits_for_a_menu_bar_move() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    drain(&mut ui.g);
    ui.turn(PANEL_ENC_K1, 1);
    for _ in 0..3 {
        ui.frame();
        drain(&mut ui.g);
    }
    assert_eq!(rows(&ui), 6);
    ui.turn(K2, 1);
    ui.turn(PANEL_ENC_SELECT, 1);
    for frame in 0..4 {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(sent.cells <= 176, "frame {frame}: {sent:?}");
    }
    assert_eq!(rows(&ui), SCREEN_POPUP_ROWS as usize);
    assert_eq!(says(&ui).0, "RESO");
}

#[test]
fn a_popup_delayed_by_continuous_menu_moves_does_not_expire_unseen() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    drain(&mut ui.g);
    ui.turn(PANEL_ENC_K1, 1);
    ui.frame();
    drain(&mut ui.g);
    for n in 0..40 {
        ui.turn(PANEL_ENC_SELECT, if n % 2 == 0 { 1 } else { -1 });
        ui.frame();
        assert!(drain(&mut ui.g).cells <= 176);
    }
    for _ in 0..4 {
        ui.frame();
        assert!(drain(&mut ui.g).cells <= 176);
    }
    assert_eq!(rows(&ui), SCREEN_POPUP_ROWS as usize);
}

#[test]
fn the_menu_stays_within_the_cell_and_spi_budgets_as_it_scrolls_and_under_a_popup() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    drain(&mut ui.g);
    let mut worst = (0usize, 0.0f64);
    let mut frames = Vec::new();
    // A step a frame down and up the whole list, jumps from end to end, a popup over the middle
    // (ALGORITHM on a value item) and OCT+'s toast in the course of it.
    let turns: Vec<i32> = (0..VIEWS_MENU_ITEMS)
        .map(|_| 1)
        .chain((0..VIEWS_MENU_ITEMS).map(|_| -1))
        .chain([99, -99, 99, -99])
        .collect();
    for (n, steps) in turns.iter().enumerate() {
        ui.turn(PANEL_ENC_SELECT, *steps);
        if n == VIEWS_MENU_PRESET as usize - 1 {
            ui.turn(PANEL_ENC_ALGORITHM, 1); // PRE: a value item's popup
        }
        if n == 20 {
            ui.press(PANEL_BTN_OCT_UP);
        }
        ui.frame();
        let sent = drain(&mut ui.g);
        frames.push(sent.cells);
        worst = (worst.0.max(sent.cells), worst.1.max(sent.spi_ms()));
    }
    for _ in 0..80 {
        ui.frame();
        let sent = drain(&mut ui.g);
        frames.push(sent.cells);
        worst = (worst.0.max(sent.cells), worst.1.max(sent.spi_ms()));
    }
    println!(
        "menu: at most {} cells a frame, {:.2} ms of SPI: {frames:?}",
        worst.0, worst.1
    );
    assert!(worst.0 <= 176, "a frame sent {} cells: {frames:?}", worst.0);
    assert!(worst.1 <= 15.10, "{:.2} ms of SPI", worst.1);
    // Every line is whole once it settles: each item reads as its own.
    ui.popup_gone();
    for _ in 0..4 {
        ui.frame();
    }
    let top = usize::from(ui.v.home.top);
    for k in top..top + VIEWS_MENU_SHOWN as usize {
        assert!(!ui.menu_line(k).trim().is_empty(), "item {k}");
    }
}

#[test]
fn the_chip_views_user_profiles_and_save_target_are_named() {
    let mut ui = Ui::without_cards();
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert_eq!(ui.top(), "CHIP");
    // USER 1 holds a profile (its base the 8580 R5), USER 2 is empty.
    ui.rig.live.user_valid = 0b01;
    ui.rig.live.user_base[0] = SID_CHIP_8580_R5 as u8;
    // EDIT + ALGORITHM picks the save target, the chip unchanged: an empty slot ends in a dash.
    ui.down(PANEL_BTN_EDIT);
    ui.turn(PANEL_ENC_ALGORITHM, 2);
    open(&mut ui);
    assert_eq!(says(&ui), ("SAVE TO".to_owned(), "USER 2 -".to_owned()));
    ui.popup_gone();
    ui.turn(PANEL_ENC_ALGORITHM, -1);
    open(&mut ui);
    assert_eq!(says(&ui), ("SAVE TO".to_owned(), "USER 1".to_owned()));
    ui.popup_gone();
    ui.turn(PANEL_ENC_ALGORITHM, -9);
    open(&mut ui);
    assert_eq!(says(&ui), ("SAVE TO".to_owned(), "OFF".to_owned()));
    ui.up(PANEL_BTN_EDIT);
    ui.popup_gone();
    // The picker runs past the built-in profiles to the filled USER slots.
    ui.turn(PANEL_ENC_ALGORITHM, 9);
    for _ in 0..4 {
        ui.frame();
    }
    assert!(!shown(&ui), "the choice shows at 3x on the page");
    assert_eq!(ui.live().user_applied, 1);
}

#[test]
fn a_knob_on_live_highlights_its_label_and_opens_no_popup() {
    let mut ui = Ui::without_cards();
    ui.settle();
    assert_eq!(ui.top(), "LIVE");
    for k in 0..4u32 {
        ui.turn(PANEL_ENC_K1 + k, 1);
        assert_ne!(ui.v.s.popup_ask, 0, "the input asks as for any view");
        for _ in 0..4 {
            ui.frame();
        }
        assert!(!shown(&ui), "K{}", k + 1);
        assert_eq!(ui.v.s.popup_ask, 0);
        assert_eq!(u32::from(ui.v.live.hl), k + 1);
    }
    // MASTER keeps its popup over LIVE; PRESETS and ALGORITHM have none to show.
    master(&mut ui, 200);
    ui.frame();
    master(&mut ui, 700);
    open(&mut ui);
    assert_eq!(says(&ui).0, "VOLUME");
    // EDIT held is the slot's page on the knob row.
    ui.popup_gone();
    assert_eq!(ui.v.live.edit, 0);
    ui.hold(PANEL_BTN_EDIT, true);
    ui.frame();
    assert_eq!(ui.v.live.edit, 1);
    ui.hold(PANEL_BTN_EDIT, false);
    ui.frame();
    assert_eq!(ui.v.live.edit, 0);
}

#[test]
fn a_toast_closes_an_open_popup() {
    let mut ui = Ui::without_cards();
    ui.settle();
    master(&mut ui, 200);
    ui.frame();
    master(&mut ui, 700);
    open(&mut ui);
    assert!(shown(&ui));
    ui.press(PANEL_BTN_OCT_UP);
    ui.frame();
    assert!(
        !shown(&ui),
        "the popup is gone in the frame the toast shows"
    );
    assert_eq!(rows(&ui), 0);
    assert_eq!(ui.toast().as_deref(), Some("OCTAVE +1"));
    // The popup's rows are the view's again.
    for _ in 0..4 {
        ui.frame();
    }
    assert!(ui.row(ROW + 4).trim_end().chars().all(|c| c != '%'));
}

/// The device's boot and a turn, a tick at a time (LIVE_TICK_HZ, four ticks a 40 ms frame): the
/// pot read at power-on before the views hook it (the first reading odd or not), resting with
/// noise, then a slow turn mid-travel whose landed changes come every frame. Each frame asked
/// for the popup and the screen opens it only in a frame without an ask (screen.c popup_step):
/// on the FM-1 the popup stayed shut until an end, where the reading stops changing.
#[test]
fn a_turn_after_boot_opens_the_master_popup_mid_travel() {
    for (boot, noise) in [(500, 0), (0, 3), (1023, 3), (500, 10), (500, 20)] {
        let mut ui = Ui::without_cards();
        let mut seed = 1u32;
        let mut rnd = |n: i32| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 16) as i32 % (2 * n + 1) - n
        };
        let mut tick = |ui: &mut Ui, at: i32, n: i32| unsafe {
            panel_master(&mut *ui.p, &mut *ui.rig.live, (at + rnd(n)).clamp(0, 1023));
        };
        tick(&mut ui, boot, 0); // main.c's reading at power-on: no hook yet
        ui.settle();
        unsafe { views_set_panel(&mut *ui.v, &mut *ui.p) };
        for _ in 0..300 {
            tick(&mut ui, 500, noise);
            ui.frame();
        }
        ui.popup_gone();
        let mut at = 500;
        let mut opened = None;
        for f in 0..100 {
            for _ in 0..4 {
                at -= 1;
                tick(&mut ui, at, noise);
            }
            ui.frame();
            if opened.is_none() && rows(&ui) == SCREEN_POPUP_ROWS as usize {
                opened = Some(f);
            }
        }
        let f = opened.unwrap_or_else(|| panic!("boot {boot}, noise {noise}: no popup mid-travel"));
        assert!(
            f < 15,
            "boot {boot}, noise {noise}: the popup opened only at frame {f}"
        );
        for _ in 0..PANEL_MASTER_ASK_GAP {
            tick(&mut ui, at, 0);
        }
        ui.frame();
        assert_eq!(
            says(&ui).1,
            format!("{}%", ui.live().volume),
            "it ends on the value"
        );
    }
}
