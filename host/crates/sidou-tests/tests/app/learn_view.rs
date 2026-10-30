// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! MIDI learn on the screen and the panel (`app/learn_view.c`): a knob turned while SAVE is
//! held opens the LEARN frame over any view on the parameter the panel would turn, another knob
//! picks instead, the CC binds; SAVE forgets the picked CC, SAVE held + HOME asks to clear them
//! all; HOME and the timeout cancel; and the toasts that say how learn ended (M2a Task 5's
//! gesture; M1d's SAVE press armed it and a knob then picked).

use std::ffi::CStr;

use sidou_sys::*;

use crate::cutoff_after;
use crate::views::Ui;

const SAVE: u32 = SAVE_KEY_BUTTON;
const K1: u32 = PANEL_ENC_K1;
const ROW: usize = LEARN_VIEW_ROW as usize;

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
            unsafe { midi_map_cc_set(&mut *self.ui.rig.live, e, value) };
        }
    }

    /// The frame's two lines of text, checked to keep a blank cell inside either edge.
    fn lines(&self) -> (String, String, String) {
        let line = |row| self.ui.big(row, 1, 2, 14).trim_end().to_owned();
        (line(ROW), line(ROW + 2), line(ROW + 4))
    }

    fn state(&self) -> u32 {
        u32::from(self.ml.state)
    }

    /// SAVE held, knob enc turned, SAVE let go: learn opens on the knob's parameter.
    fn open(&mut self, enc: u32) {
        self.ui.down(SAVE);
        self.ui.turn(enc, 1);
        self.ui.up(SAVE);
    }

    /// SAVE held + HOME, SAVE let go.
    fn save_home(&mut self) {
        self.ui.down(SAVE);
        self.ui.press(PANEL_BTN_HOME);
        self.ui.up(SAVE);
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
fn learn_view_lines_fit_at_2x() {
    let params = [
        LIVE_PARAM_CUTOFF,
        LIVE_PARAM_RESONANCE,
        LIVE_PARAM_PULSE_WIDTH,
    ]
    .into_iter()
    .chain(LIVE_PARAM_PROFILE..=LIVE_PARAM_BLEED)
    .chain(LIVE_PARAM_SLOT..LIVE_PARAM_SLOT + LIVE_SLOTS * LIVE_SLOT_FIELDS)
    .chain(LIVE_PARAM_EXPERT..LIVE_PARAM_EXPERT + SID_EXPERT_COUNT);
    for param in params {
        if unsafe { midi_learn_param(param) } == 0 {
            continue;
        }
        for cc in [None, Some(21), Some(119)] {
            let mut t = Learn::new();
            t.open(K1);
            t.ml.param = param as u8;
            if let Some(cc) = cc {
                assert_ne!(unsafe { midi_learn_bind(&mut *t.ml, cc, param) }, 0);
            }
            t.ui.frame();
            assert_eq!(t.ui.g.src[20 * GRID_COLS as usize + 1], b'L');
            let lines = [20, 22, 24].map(|row| t.ui.big(row, 1, 2, 14).trim_end().to_owned());
            let expected = if param >= LIVE_PARAM_EXPERT {
                [
                    "81CURVE", "81RANGE", "80CURVE", "NONLIN", "BIAS", "BLEED", "KINK", "LEAK",
                    "COMB", "NOISE", "THUMP", "EXT LP", "EXT HP", "VOICEDC",
                ][(param - LIVE_PARAM_EXPERT) as usize]
                    .to_owned()
            } else {
                name(param)
            };
            assert_eq!(lines[0], format!("LEARN {expected}"));
            assert_eq!(lines[1], "SEND A CC");
            assert_eq!(
                lines[2],
                match cc {
                    None => "HOME CANCEL".into(),
                    Some(n) if n < 100 => format!("SAVE FORGET {n}"),
                    Some(n) => format!("SAVE FORGET{n}"),
                }
            );
            t.save_home();
            t.ui.frame();
            assert_eq!(t.ui.big(20, 1, 2, 14).trim_end(), "CLEAR ALL?");
            assert_eq!(t.ui.big(22, 1, 2, 14).trim_end(), "SAVE YES");
            assert_eq!(t.ui.big(24, 1, 2, 14).trim_end(), "HOME NO");
        }
    }
}

#[test]
fn save_held_and_a_knob_open_the_frame_on_its_parameter_and_a_cc_learns() {
    let mut t = Learn::new();
    let cutoff = t.ui.live().cutoff;
    t.ui.down(SAVE);
    assert_eq!(t.ui.top(), "LIVE", "never on SAVE's press");
    t.ui.turn(K1, 3);
    assert_eq!(t.ui.top(), "LEARN");
    assert_eq!(
        t.ui.live().cutoff,
        cutoff,
        "the knob picks, it does not turn"
    );
    assert_eq!(t.state(), MIDI_LEARN_CC);
    t.ui.up(SAVE);
    assert_eq!(t.state(), MIDI_LEARN_CC, "the release forgets nothing");
    t.ui.frame();
    assert_eq!(t.lines().0, "LEARN CUTOFF");
    // LIVE still shows under the frame.
    assert_eq!(&t.ui.row(0)[..8], "SAW LEAD");
    assert_eq!(
        t.lines(),
        (
            "LEARN CUTOFF".into(),
            "SEND A CC".into(),
            "HOME CANCEL".into()
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
        cutoff_after(t.ui.live().model, LIVE_CUTOFF_MAX as u16, -1)
    );
    // The capture went with the view: HOME opens the menu again.
    t.ui.settle();
    t.ui.press(PANEL_BTN_HOME);
    assert_eq!(t.ui.top(), "MENU");
}

#[test]
fn another_knob_with_save_held_or_not_picks_instead() {
    let mut t = Learn::new();
    t.open(K1);
    t.ui.turn(K1 + 1, 1);
    assert_eq!(u32::from(t.ml.param), LIVE_PARAM_RESONANCE);
    t.ui.down(SAVE);
    t.ui.turn(K1 + 2, -1);
    t.ui.up(SAVE);
    // K3 on SAW LEAD is the decay.
    assert_eq!(u32::from(t.ml.param), LIVE_PARAM_DECAY);
    assert_eq!(t.ui.top(), "LEARN");
    t.cc(21, 0);
    assert_eq!(unsafe { midi_learn_cc_of(&*t.ml, LIVE_PARAM_DECAY) }, 21);
}

#[test]
fn home_cancels_and_reaches_nothing_else() {
    let mut t = Learn::new();
    t.open(K1);
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
    t.open(K1 + 1);
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
    t.ui.hold(PANEL_BTN_EDIT, true);
    t.open(K1 + 3);
    t.ui.hold(PANEL_BTN_EDIT, false);
    let mix = LIVE_PARAM_SLOT + LIVE_SLOT_FIELDS + LIVE_SLOT_MIX;
    assert_eq!(u32::from(t.ml.param), mix);
    assert_eq!(t.ui.live().slot[1][LIVE_SLOT_MIX as usize], 0, "not turned");
    t.ui.frame();
    assert_eq!(t.lines().0, "LEARN MOD MIX");
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
    t.open(K1);
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
/// neither forgets the CC just learned nor opens the menu.
#[test]
fn save_or_home_after_a_cc_learned_and_before_the_frame_keep_the_toast() {
    for button in [SAVE, PANEL_BTN_HOME] {
        let mut t = Learn::new();
        t.open(K1);
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
        assert_eq!(
            unsafe { midi_learn_cc_of(&*t.ml, LIVE_PARAM_CUTOFF) },
            21,
            "button {button}"
        );
    }
}

#[test]
fn k3_ratio_has_no_cc() {
    let mut t = Learn::new();
    t.ui.rig.set_preset(2); // SYNC LEAD: K3 is the ratio
    t.open(PANEL_ENC_K1 + 2);
    assert_eq!(t.ui.live().sync_semis, 0, "K3 did not turn");
    assert_eq!(t.state(), MIDI_LEARN_IDLE);
    assert_eq!(t.ui.toast().as_deref(), Some("RATIO: NO CC"));
    assert_eq!(t.ui.top(), "LIVE", "the frame does not open on it");
    // In the frame, K3 says the same and the pick stays.
    t.open(K1);
    t.ui.settle();
    t.ui.turn(PANEL_ENC_K1 + 2, 1);
    assert_eq!(t.ui.toast().as_deref(), Some("RATIO: NO CC"));
    assert_eq!(
        (t.ui.top().as_str(), u32::from(t.ml.param)),
        ("LEARN", LIVE_PARAM_CUTOFF)
    );
    assert_eq!(t.ui.live().sync_semis, 0, "K3 did not turn");
    // K4 is the release: a parameter like any other.
    t.ui.turn(PANEL_ENC_K4, 1);
    assert_eq!(u32::from(t.ml.param), LIVE_PARAM_RELEASE);
}

#[test]
fn the_keys_and_the_other_buttons_play_while_learn_waits() {
    let mut t = Learn::new();
    t.open(K1);
    t.ui.press(PANEL_BTN_OCT_UP);
    assert_eq!(t.ui.live().octave, 1);
    t.ui.press(PANEL_BTN_SCL);
    assert_eq!(t.ui.live().fx_select, 1);
    assert_eq!(t.ui.top(), "LEARN");
}

#[test]
fn save_held_and_home_ask_and_save_clears_every_mapping() {
    let mut t = Learn::new();
    unsafe { midi_learn_bind(&mut *t.ml, 21, LIVE_PARAM_CUTOFF) };
    unsafe { midi_learn_bind(&mut *t.ml, 22, LIVE_PARAM_RESONANCE) };
    t.open(K1 + 2);
    t.save_home();
    assert_eq!(t.ui.top(), "LEARN", "HOME with SAVE held does not cancel");
    assert_eq!(t.state(), MIDI_LEARN_CONFIRM);
    assert_eq!(t.ml.count, 2, "asking clears nothing");
    t.ui.frame();
    assert_eq!(
        t.lines(),
        ("CLEAR ALL?".into(), "SAVE YES".into(), "HOME NO".into())
    );
    t.ui.press(SAVE);
    assert_eq!(t.ui.top(), "LIVE");
    assert_eq!(t.ui.toast().as_deref(), Some("MIDI LEARN CLEARED"));
    assert_eq!(t.ml.count, 0);
}

#[test]
fn home_or_ten_seconds_after_the_question_keep_the_mappings() {
    for wait in [false, true] {
        let mut t = Learn::new();
        unsafe { midi_learn_bind(&mut *t.ml, 21, LIVE_PARAM_CUTOFF) };
        t.open(K1);
        t.save_home();
        if wait {
            for _ in 0..MIDI_LEARN_TIMEOUT_MS / 40 + 1 {
                t.ui.frame();
            }
        } else {
            t.ui.press(PANEL_BTN_HOME);
        }
        assert_eq!(t.ui.top(), "LIVE", "wait {wait}");
        assert_eq!(t.ml.count, 1, "wait {wait}");
        let toast = if wait {
            "LEARN TIMED OUT"
        } else {
            "LEARN CANCELLED"
        };
        assert_eq!(t.ui.toast().as_deref(), Some(toast));
    }
}

#[test]
fn save_held_and_home_outside_the_frame_do_nothing() {
    let mut t = Learn::new();
    unsafe { midi_learn_bind(&mut *t.ml, 21, LIVE_PARAM_CUTOFF) };
    t.save_home();
    t.ui.settle();
    assert_eq!(t.ui.top(), "LIVE", "the menu does not open");
    assert_eq!((t.state(), t.ml.count), (MIDI_LEARN_IDLE, 1));
    // From the menu too: HOME does not go back.
    t.ui.press(PANEL_BTN_HOME);
    t.ui.settle();
    t.save_home();
    t.ui.settle();
    assert_eq!(t.ui.top(), "MENU");
}

#[test]
fn save_after_a_knob_forgets_that_knobs_cc() {
    let mut t = Learn::new();
    unsafe { midi_learn_bind(&mut *t.ml, 21, LIVE_PARAM_CUTOFF) };
    t.open(K1);
    t.ui.frame();
    assert_eq!(
        t.lines(),
        (
            "LEARN CUTOFF".into(),
            "SEND A CC".into(),
            "SAVE FORGET 21".into()
        )
    );
    t.ui.press(SAVE);
    assert_eq!(t.ui.top(), "LIVE");
    assert_eq!(t.ui.toast().as_deref(), Some("CUTOFF: CC 21 FORGOTTEN"));
    assert_eq!(t.ml.count, 0);
    // Nothing to forget: it says so and the frame stays, waiting for a CC.
    t.ui.settle();
    t.open(K1);
    t.ui.press(SAVE);
    assert_eq!(t.ui.toast().as_deref(), Some("CUTOFF: NO CC"));
    assert_eq!((t.ui.top().as_str(), t.state()), ("LEARN", MIDI_LEARN_CC));
    t.cc(40, 0);
    assert_eq!(unsafe { midi_learn_cc_of(&*t.ml, LIVE_PARAM_CUTOFF) }, 40);
}

#[test]
fn learn_opens_over_the_menu_and_closes_back_to_it() {
    let mut t = Learn::new();
    t.ui.press(PANEL_BTN_HOME);
    t.ui.settle();
    t.open(K1);
    assert_eq!(t.ui.top(), "LEARN");
    t.ui.frame();
    assert_eq!(t.ui.big(1, 1, 2, 4), "MENU");
    t.ui.press(PANEL_BTN_HOME);
    assert_eq!(t.ui.top(), "MENU");
}

/// Over the menu, SELECT and ALGORITHM reach nothing: the menu's ALGORITHM would open a view
/// over the frame (only the CHIP view takes them under it, `chip_view.rs`).
#[test]
fn select_and_algorithm_reach_nothing_over_the_menu() {
    let mut t = Learn::new();
    t.ui.press(PANEL_BTN_HOME);
    t.ui.settle();
    let cursor = t.ui.v.home.cursor;
    t.open(K1);
    t.ui.turn(PANEL_ENC_SELECT, 1);
    t.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(t.ui.top(), "LEARN");
    assert_eq!(t.ui.v.home.cursor, cursor);
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
        if what == "CHARSET" {
            assert_eq!(under, 0, "the charset wave is below the new band");
            assert_ne!(t.ui.g.ovl[26 * GRID_COLS as usize + 7], 0);
        } else {
            assert!(under > 0, "{what} draws no overlay where the frame goes");
        }
        t.open(K1);
        for f in 0..10 {
            t.ui.frame();
            assert_eq!(overlays_on_the_frame(&t.ui), 0, "{what}, frame {f}");
        }
        t.ui.press(PANEL_BTN_HOME);
        t.ui.frame();
        assert_eq!(t.ui.top(), what);
        assert!(
            overlays_on_the_frame(&t.ui) == under
                || what == "ABOUT" && overlays_on_the_frame(&t.ui) > 0,
            "{what}'s overlays stay off"
        );
    }
}

#[test]
fn without_learn_save_held_and_a_knob_turn_the_knob() {
    let mut ui = Ui::new();
    let cutoff = ui.live().cutoff;
    ui.down(SAVE);
    ui.turn(K1, 1);
    ui.up(SAVE);
    assert_eq!(ui.top(), "LIVE");
    assert_ne!(ui.live().cutoff, cutoff);
}

#[test]
fn an_idle_frame_composes_nothing_and_a_step_composes() {
    let mut t = Learn::new();
    // Everything sent, as the device sends between frames: LIVE's scopes, held to a budget of
    // what the panel has yet to get, come back in the frame that opens learn.
    crate::drain(&mut t.ui.g);
    t.open(K1);
    t.ui.frame();
    crate::drain(&mut t.ui.g);
    for _ in 0..8 {
        t.ui.frame();
        crate::drain(&mut t.ui.g);
    }
    t.ui.frame();
    assert_eq!(crate::drain(&mut t.ui.g).cells, 0, "idle");
    t.ui.turn(K1 + 1, 1);
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
