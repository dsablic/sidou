// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! MIDI learn on the screen and the panel (`app/learn_view.c`): SAVE opens the LEARN frame
//! over any view, a knob picks the parameter the panel would turn, the CC binds, HOME, the
//! timeout and the clear gestures, and the toasts that say how learn ended.

use std::ffi::CStr;

use sidou_sys::*;

use crate::views::Ui;

const SAVE: u32 = LEARN_VIEW_BUTTON;
const K1: u32 = PANEL_ENC_K1;
const ROW: usize = LEARN_VIEW_ROW as usize;
const ARROW_LEFT: char = '#'; // Ui::row shows glyphs outside ASCII as '#'

struct Learn {
    ui: Ui,
    ml: Box<midi_learn>,
}

impl Learn {
    fn new() -> Self {
        let mut ui = Ui::new();
        let mut ml = Box::<midi_learn>::default();
        unsafe {
            midi_learn_init(&mut *ml);
            views_learn(&mut *ui.v, &mut *ml);
        }
        ui.settle();
        Learn { ui, ml }
    }

    /// A CC from MIDI, as midi_map_cc routes it, then a frame.
    fn cc(&mut self, cc: u8, value: u8) {
        self.cc_between_frames(cc, value);
        self.ui.frame();
    }

    /// A CC from MIDI, as midi_map_cc routes it, before the next frame.
    fn cc_between_frames(&mut self, cc: u8, value: u8) {
        let e = unsafe { midi_learn_route(&mut *self.ml, cc) };
        if !e.is_null() {
            let e = unsafe { &*e };
            unsafe {
                live_param_set(
                    &mut *self.ui.rig.live,
                    u32::from(e.param),
                    midi_map_from_cc(e, value),
                )
            };
        }
    }

    /// The frame's two lines of text, checked to keep a blank cell inside either edge.
    fn lines(&self) -> (String, String) {
        let cut = |s: String| {
            assert_eq!((&s[3..4], &s[26..27]), (" ", " "), "{s}");
            s[4..26].trim_end().to_owned()
        };
        (cut(self.ui.row(ROW + 1)), cut(self.ui.row(ROW + 2)))
    }

    fn state(&self) -> u32 {
        u32::from(self.ml.state)
    }
}

fn name(param: u32) -> String {
    let mut buf = [0 as std::ffi::c_char; LEARN_VIEW_NAME as usize];
    unsafe { learn_view_param_name(param, buf.as_mut_ptr()) };
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[test]
fn save_opens_the_frame_a_knob_picks_and_a_cc_learns() {
    let mut t = Learn::new();
    let cutoff = t.ui.live().cutoff;
    t.ui.press(SAVE);
    assert_eq!(t.ui.top(), "LEARN");
    assert_eq!(t.state(), MIDI_LEARN_KNOB);
    t.ui.frame();
    assert!(t.ui.row(ROW).contains(" MIDI LEARN "), "{}", t.ui.row(ROW));
    assert_eq!(t.lines(), ("TURN A KNOB".into(), "SAVE: CLEAR ALL".into()));
    // LIVE still shows under the frame.
    assert_eq!(&t.ui.row(0)[1..5], "LIVE");
    t.ui.turn(K1, 3);
    assert_eq!(
        t.ui.live().cutoff,
        cutoff,
        "the knob picks, it does not turn"
    );
    assert_eq!(t.state(), MIDI_LEARN_CC);
    t.ui.frame();
    assert_eq!(
        t.lines(),
        (
            format!("CUTOFF {ARROW_LEFT} SEND A CC"),
            "HOME: CANCEL".into()
        )
    );
    t.cc(21, 127);
    assert_eq!(t.ui.top(), "LIVE", "learned: the frame closes");
    assert_eq!(
        t.ui.toast().as_deref(),
        Some("LEARNED CC 21 \u{fffd} CUTOFF")
    );
    assert_eq!(t.ui.live().cutoff, LIVE_CUTOFF_MAX as u16);
    assert_eq!(unsafe { midi_learn_cc_of(&*t.ml, LIVE_PARAM_CUTOFF) }, 21);
    // The panel is the panel's again.
    t.ui.turn(K1, -1);
    assert_eq!(
        t.ui.live().cutoff,
        LIVE_CUTOFF_MAX as u16 - LIVE_CUTOFF_DETENT as u16
    );
    // The capture went with the view: SAVE arms again, HOME then cancels.
    t.ui.settle();
    t.ui.press(PANEL_BTN_HOME);
    assert_eq!(t.ui.top(), "MENU");
}

#[test]
fn home_cancels_and_reaches_nothing_else() {
    let mut t = Learn::new();
    t.ui.press(SAVE);
    t.ui.turn(K1, 1);
    t.ui.press(PANEL_BTN_HOME);
    assert_eq!(
        t.ui.top(),
        "LIVE",
        "HOME closed learn, it did not open the menu"
    );
    assert_eq!(t.ui.toast().as_deref(), Some("LEARN CANCELLED"));
    t.cc(21, 0);
    assert_eq!(t.ml.count, 0);
}

#[test]
fn ten_seconds_without_a_step_time_out() {
    let mut t = Learn::new();
    t.ui.press(SAVE);
    let frames = MIDI_LEARN_TIMEOUT_MS / 40;
    for _ in 0..frames - 2 {
        t.ui.frame();
    }
    t.ui.turn(K1, 1); // a step restarts the time
    for _ in 0..frames - 2 {
        t.ui.frame();
    }
    assert_eq!(t.ui.top(), "LEARN");
    for _ in 0..3 {
        t.ui.frame();
    }
    assert_eq!(t.ui.top(), "LIVE");
    assert_eq!(t.ui.toast().as_deref(), Some("LEARN TIMED OUT"));
}

#[test]
fn edit_held_picks_the_selected_slots_controls() {
    let mut t = Learn::new();
    t.ui.press(PANEL_BTN_SCL); // the Modulation slot
    t.ui.press(SAVE);
    t.ui.hold(PANEL_BTN_EDIT, true);
    t.ui.turn(K1 + 3, 1);
    t.ui.hold(PANEL_BTN_EDIT, false);
    let mix = LIVE_PARAM_SLOT + LIVE_SLOT_FIELDS + LIVE_SLOT_MIX;
    assert_eq!(u32::from(t.ml.param), mix);
    assert_eq!(t.ui.live().slot[1][LIVE_SLOT_MIX as usize], 0, "not turned");
    t.ui.frame();
    assert_eq!(t.lines().0, format!("MOD MIX {ARROW_LEFT} SEND A CC"));
    t.cc(30, 127);
    assert_eq!(
        t.ui.toast().as_deref(),
        Some("LEARNED CC 30 \u{fffd} MOD MIX")
    );
    assert_eq!(t.ui.live().slot[1][LIVE_SLOT_MIX as usize], 127);
}

/// A CC ends learn, and before the next frame closes it (up to 40 ms) a knob turns: the
/// frame closes with its LEARNED toast and the knob turns what it always does.
#[test]
fn a_knob_turned_after_a_cc_learned_and_before_the_frame_turns() {
    let mut t = Learn::new();
    t.ui.press(SAVE);
    t.ui.turn(K1, 1);
    t.cc_between_frames(21, 64);
    let reso = t.ui.live().resonance;
    t.ui.turn(K1 + 1, 1);
    assert_eq!(t.ui.live().resonance, reso + 1, "K2 was swallowed");
    assert_eq!(t.ui.top(), "LIVE");
    assert_eq!(
        t.ui.toast().as_deref(),
        Some("LEARNED CC 21 \u{fffd} CUTOFF")
    );
    t.ui.frame();
    assert_eq!(t.ui.top(), "LIVE");
    assert_eq!(t.state(), MIDI_LEARN_IDLE);
}

/// The same with SAVE or HOME, meant for the frame: it closes with its LEARNED toast, and
/// neither arms learn again nor opens the menu.
#[test]
fn save_or_home_after_a_cc_learned_and_before_the_frame_keep_the_toast() {
    for button in [SAVE, PANEL_BTN_HOME] {
        let mut t = Learn::new();
        t.ui.press(SAVE);
        t.ui.turn(K1, 1);
        t.cc_between_frames(21, 64);
        t.ui.press(button);
        assert_eq!(t.ui.top(), "LIVE", "button {button}");
        assert_eq!(t.state(), MIDI_LEARN_IDLE, "button {button}");
        assert_eq!(
            t.ui.toast().as_deref(),
            Some("LEARNED CC 21 \u{fffd} CUTOFF"),
            "button {button}"
        );
        t.ui.frame();
        assert_eq!(t.ui.top(), "LIVE", "button {button}");
    }
}

#[test]
fn k4_is_the_preset_and_has_no_cc() {
    let mut t = Learn::new();
    t.ui.press(SAVE);
    t.ui.turn(PANEL_ENC_K4, 1);
    assert_eq!(t.ui.live().preset, 0, "K4 did not turn");
    assert_eq!(t.state(), MIDI_LEARN_KNOB);
    assert_eq!(t.ui.toast().as_deref(), Some("K4: THE PRESET, NO CC"));
    assert_eq!(t.ui.top(), "LEARN");
}

#[test]
fn the_keys_and_the_other_buttons_play_while_learn_waits() {
    let mut t = Learn::new();
    t.ui.press(SAVE);
    t.ui.press(PANEL_BTN_OCT_UP);
    assert_eq!(t.ui.live().octave, 1);
    t.ui.press(PANEL_BTN_SCL);
    assert_eq!(t.ui.live().fx_select, 1);
    assert_eq!(t.ui.top(), "LEARN");
}

#[test]
fn save_twice_asks_and_three_times_clears_every_mapping() {
    let mut t = Learn::new();
    unsafe { midi_learn_bind(&mut *t.ml, 21, LIVE_PARAM_CUTOFF) };
    t.ui.press(SAVE);
    t.ui.press(SAVE);
    t.ui.frame();
    assert_eq!(
        t.lines(),
        ("CLEAR ALL MAPPINGS?".into(), "SAVE: YES  HOME: NO".into())
    );
    t.ui.press(SAVE);
    assert_eq!(t.ui.top(), "LIVE");
    assert_eq!(t.ui.toast().as_deref(), Some("MIDI LEARN CLEARED"));
    assert_eq!(t.ml.count, 0);
}

#[test]
fn save_after_a_knob_forgets_that_knobs_cc() {
    let mut t = Learn::new();
    unsafe { midi_learn_bind(&mut *t.ml, 21, LIVE_PARAM_CUTOFF) };
    t.ui.press(SAVE);
    t.ui.turn(K1, 1);
    t.ui.frame();
    assert_eq!(t.lines().1, "SAVE: FORGET CC 21");
    t.ui.press(SAVE);
    assert_eq!(t.ui.toast().as_deref(), Some("CUTOFF: CC 21 FORGOTTEN"));
    t.ui.settle();
    t.ui.press(SAVE);
    t.ui.turn(K1, 1);
    t.ui.press(SAVE);
    assert_eq!(t.ui.toast().as_deref(), Some("CUTOFF: NO CC LEARNED"));
}

#[test]
fn learn_opens_over_the_menu_and_closes_back_to_it() {
    let mut t = Learn::new();
    t.ui.press(PANEL_BTN_HOME);
    t.ui.settle();
    t.ui.press(SAVE);
    assert_eq!(t.ui.top(), "LEARN");
    t.ui.frame();
    assert!(t.ui.row(4).contains("MENU"));
    t.ui.press(PANEL_BTN_HOME);
    assert_eq!(t.ui.top(), "MENU");
}

/// The frame's cells that carry an overlay (a pixel drawing over the glyph).
fn overlays_on_the_frame(ui: &Ui) -> usize {
    let cols = GRID_COLS as usize;
    let (col, w) = (LEARN_VIEW_COL as usize, LEARN_VIEW_W as usize);
    (ROW..ROW + LEARN_VIEW_H as usize)
        .flat_map(|r| (col..col + w).map(move |c| r * cols + c))
        .filter(|&i| ui.g.ovl[i] != 0)
        .count()
}

/// Over a view that draws overlays where the frame goes (About's scroller, the glyph sheet's
/// triangle wave), the frame is clear of them every frame, and they come back when it closes.
#[test]
fn the_frame_is_clear_of_the_overlays_of_the_view_under_it() {
    for (what, open) in [
        ("ABOUT", crate::about::open_about as fn(&mut Ui)),
        ("CHARSET", crate::views::open_charset),
    ] {
        let mut t = Learn::new();
        open(&mut t.ui);
        for _ in 0..10 {
            t.ui.frame();
        }
        assert_eq!(t.ui.top(), what);
        let under = overlays_on_the_frame(&t.ui);
        assert!(under > 0, "{what} draws no overlay where the frame goes");
        t.ui.press(SAVE);
        for f in 0..10 {
            t.ui.frame();
            assert_eq!(overlays_on_the_frame(&t.ui), 0, "{what}, frame {f}");
        }
        t.ui.press(PANEL_BTN_HOME);
        t.ui.frame();
        assert_eq!(t.ui.top(), what);
        assert!(
            overlays_on_the_frame(&t.ui) > 0,
            "{what}'s overlays stay off"
        );
    }
}

#[test]
fn without_learn_save_does_nothing() {
    let mut ui = Ui::new();
    ui.press(SAVE);
    assert_eq!(ui.top(), "LIVE");
}

#[test]
fn an_idle_frame_composes_nothing_and_a_step_composes() {
    let mut t = Learn::new();
    // Everything sent, as the device sends between frames: LIVE's scopes, held to a budget of
    // what the panel has yet to get, come back in the frame that opens learn.
    crate::drain(&mut t.ui.g);
    t.ui.press(SAVE);
    t.ui.frame();
    crate::drain(&mut t.ui.g);
    t.ui.frame();
    assert_eq!(crate::drain(&mut t.ui.g).cells, 0, "idle");
    t.ui.turn(K1, 1);
    t.ui.frame();
    let sent = crate::drain(&mut t.ui.g);
    assert!(
        sent.cells > 0 && sent.cells <= 2 * LEARN_VIEW_CHARS as usize,
        "{sent:?}: only the frame's text changes"
    );
}

#[test]
fn every_knob_parameter_has_a_name_that_fits() {
    let mut names = Vec::new();
    for p in (0..=LIVE_PARAM_PULSE_WIDTH).chain(LIVE_PARAM_SLOT..LIVE_PARAM_SLOT + 15) {
        let n = name(p);
        assert!(
            !n.is_empty() && n.len() < LEARN_VIEW_NAME as usize,
            "{p}: {n}"
        );
        // The longest toast still fits: "LEARNED CC 119 -> " and the name.
        assert!(18 + n.len() <= SCREEN_TOAST_CHARS as usize, "{n}");
        names.push(n);
    }
    assert_eq!(&names[..4], ["CUTOFF", "RESO", "PULSE", "DIST ALG"]);
    assert_eq!(names.last().map(String::as_str), Some("FX MIX"));
    assert_eq!(name(LIVE_PARAM_VOLUME), "PARAM 3");
}
