// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The front panel (`app/panel.c`): note keys, buttons, encoders, MASTER and LEDs into live.

use sidou_sys::*;

use crate::{freq_table, Rig};

struct Panel {
    rig: Rig,
    p: Box<panel>,
}

impl Panel {
    fn new() -> Self {
        let mut p = Box::<panel>::default();
        unsafe { panel_init(&mut *p) };
        let mut rig = Rig::new(SID_MODEL_6581);
        rig.clear();
        Panel { rig, p }
    }

    fn keys(&mut self, down: u32, pressed: u32) {
        unsafe { panel_note_keys(&mut *self.p, &mut *self.rig.live, down, pressed) }
    }

    fn buttons(&mut self, pressed: u32) {
        unsafe { panel_buttons(&mut *self.p, &mut *self.rig.live, pressed) }
    }

    fn enc(&mut self, enc: u32, steps: i32) {
        unsafe { panel_encoder(&mut *self.p, &mut *self.rig.live, enc, steps) }
    }

    fn master(&mut self, raw: i32) {
        unsafe { panel_master(&mut *self.p, &mut *self.rig.live, raw) }
    }

    fn leds(&self) -> panel_leds {
        unsafe { panel_get_leds(&*self.p, &*self.rig.live) }
    }

    /// Voices gated at the note's frequency.
    fn sounding(&self, note: u8) -> usize {
        let f = freq_table()[usize::from(note)];
        (0..3)
            .filter(|&v| self.rig.gated(v) && self.rig.voice_freq(v) == f)
            .count()
    }

    fn gated(&self) -> usize {
        (0..3).filter(|&v| self.rig.gated(v)).count()
    }
}

const fn key(k: u32) -> u32 {
    1 << k
}

#[test]
fn key_0_is_f3_and_key_16_is_a4() {
    let mut p = Panel::new();
    p.keys(key(0), key(0));
    assert_eq!(p.sounding(53), 1);
    assert_eq!(p.p.key_note[0], 53);
    p.keys(key(0) | key(16), key(16));
    assert_eq!(p.sounding(69), 1);
    assert_eq!(p.p.key_note[16], 69);
    assert_eq!(
        PANEL_BASE_NOTE + PANEL_NOTE_KEYS - 1,
        79,
        "the top key is G5"
    );
}

#[test]
fn a_held_key_writes_nothing_more() {
    let mut p = Panel::new();
    p.keys(key(5), key(5));
    p.rig.clear();
    p.keys(key(5), 0);
    p.keys(key(5), 0);
    assert!(p.rig.log().is_empty());
}

#[test]
fn key_up_releases_its_note() {
    let mut p = Panel::new();
    p.keys(key(3), key(3));
    assert_eq!(p.gated(), 1);
    p.keys(0, 0);
    assert_eq!(p.gated(), 0);
    assert_eq!(p.p.key_note[3], LIVE_NO_NOTE as u8);
}

#[test]
fn key_up_after_an_octave_change_releases_the_note_it_played() {
    let mut p = Panel::new();
    p.keys(key(7), key(7)); // C4 = 60
    p.buttons(1 << PANEL_BTN_OCT_UP);
    assert_eq!(p.rig.live.octave, 1);
    assert_eq!(p.sounding(60), 1, "the held note keeps sounding");
    p.keys(0, 0);
    assert_eq!(
        p.gated(),
        0,
        "a stuck note: key-up sent the new octave's note"
    );
    // The next press plays the new octave.
    p.keys(key(7), key(7));
    assert_eq!(p.sounding(72), 1);
}

#[test]
fn a_press_and_release_between_two_polls_plays_the_note_and_releases_it() {
    let mut p = Panel::new();
    p.keys(0, key(9));
    let ctrl: Vec<u8> = p
        .rig
        .log()
        .iter()
        .filter(|&&(r, _)| r == 4)
        .map(|&(_, v)| v & 1)
        .collect();
    assert_eq!(ctrl, [1, 0], "a gate on and then off");
    assert_eq!(p.gated(), 0);
}

#[test]
fn a_press_while_held_retriggers() {
    let mut p = Panel::new();
    p.keys(key(2), key(2));
    p.rig.clear();
    p.keys(key(2), key(2)); // let go and pressed again between the polls
    let gates: Vec<u8> = p
        .rig
        .log()
        .iter()
        .filter(|&&(r, _)| r % 7 == 4 && r < 21)
        .map(|&(_, v)| v & 1)
        .collect();
    assert_eq!(gates.first(), Some(&0), "the gate goes off first");
    assert_eq!(gates.last(), Some(&1), "and on again");
    assert_eq!(p.sounding(55), 1);
}

#[test]
fn many_keys_steal_and_release_cleanly() {
    let mut p = Panel::new();
    let all = (1 << PANEL_NOTE_KEYS) - 1;
    p.keys(all, all);
    assert_eq!(p.gated(), 3);
    p.keys(0, 0);
    assert_eq!(p.gated(), 0);
    assert!(p.p.key_note.iter().all(|&n| n == LIVE_NO_NOTE as u8));
}

#[test]
fn buttons_turn_the_octave_and_toggle_the_model() {
    let mut p = Panel::new();
    p.buttons(1 << PANEL_BTN_OCT_DOWN);
    assert_eq!(p.rig.live.octave, -1);
    p.buttons(1 << PANEL_BTN_OCT_UP | 1 << PANEL_BTN_OCT_DOWN);
    assert_eq!(p.rig.live.octave, -1, "both at once cancel out");
    p.buttons(1 << PANEL_BTN_FX);
    assert_eq!(p.rig.live.model, SID_MODEL_8580);
    p.rig.clear();
    p.buttons(1 << 3 | 1 << 12 | 1 << 13);
    assert!(p.rig.log().is_empty(), "the other buttons do nothing yet");
}

#[test]
fn k1_to_k4_are_cutoff_resonance_pulse_width_and_preset() {
    let mut p = Panel::new();
    let (c, r, w) = (
        p.rig.live.cutoff,
        p.rig.live.resonance,
        p.rig.live.pulse_width,
    );
    p.enc(PANEL_ENC_K1, -2);
    assert_eq!(p.rig.live.cutoff, c - 2 * LIVE_CUTOFF_DETENT as u16);
    p.enc(PANEL_ENC_K1 + 1, 1);
    assert_eq!(p.rig.live.resonance, r + 1);
    p.enc(PANEL_ENC_K1 + 2, 3);
    assert_eq!(
        p.rig.live.pulse_width,
        w + 3 * LIVE_PULSE_WIDTH_DETENT as u16
    );
    p.enc(PANEL_ENC_K4, 1);
    assert_eq!(p.rig.live.preset, 1);
    p.rig.clear();
    for e in [0, 1, 6] {
        p.enc(e, 5);
    }
    assert!(
        p.rig.log().is_empty(),
        "SELECT, ALGORITHM and PRESETS do nothing yet"
    );
}

#[test]
fn master_follows_the_pot_with_hysteresis() {
    let mut p = Panel::new();
    p.master(900);
    assert_eq!(p.rig.live.master, 900);
    assert_eq!(p.rig.live.volume, 13);
    p.master(902);
    p.master(897);
    assert_eq!(p.rig.live.master, 900, "ADC noise is ignored");
    p.master(904);
    assert_eq!(p.rig.live.master, 904);
    p.master(-1);
    assert_eq!(p.rig.live.master, 904, "an ADC timeout is ignored");
    p.master(1022);
    p.master(1023);
    assert_eq!(p.rig.live.volume, 15, "the top end always lands");
    p.master(2);
    p.master(0);
    assert_eq!((p.rig.live.master, p.rig.live.volume), (0, 0));
    p.master(5000);
    assert_eq!(p.rig.live.master, 1023);
}

#[test]
fn leds_show_held_keys_the_octave_and_the_8580() {
    let mut p = Panel::new();
    let l = p.leds();
    assert_eq!((l.buttons, l.notes), (0, 0));
    p.keys(key(1) | key(26), key(1) | key(26));
    assert_eq!(p.leds().notes, key(1) | key(26));
    p.buttons(1 << PANEL_BTN_OCT_UP);
    assert_eq!(p.leds().buttons, 1 << PANEL_BTN_OCT_UP);
    p.buttons(1 << PANEL_BTN_OCT_DOWN);
    p.buttons(1 << PANEL_BTN_OCT_DOWN);
    assert_eq!(p.leds().buttons, 1 << PANEL_BTN_OCT_DOWN);
    p.buttons(1 << PANEL_BTN_FX);
    assert_eq!(
        p.leds().buttons,
        1 << PANEL_BTN_OCT_DOWN | 1 << PANEL_BTN_FX
    );
    p.keys(key(26), 0);
    assert_eq!(p.leds().notes, key(26));
}
