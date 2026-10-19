// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The front panel (`app/panel.c`): note keys, buttons, encoders, MASTER and LEDs into live.

use sidou_sys::*;

use crate::{cutoff_after, freq_table, Rig};

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
        unsafe { panel_buttons(&mut *self.p, &mut *self.rig.live, pressed, 0) }
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
fn k1_to_k4_are_cutoff_resonance_the_sounds_own_control_and_release() {
    let mut p = Panel::new();
    let (c, r) = (p.rig.live.cutoff, p.rig.live.resonance);
    p.enc(PANEL_ENC_K1, -2);
    assert_eq!(p.rig.live.cutoff, cutoff_after(p.rig.live.model, c, -2));
    p.enc(PANEL_ENC_K1 + 1, 1);
    assert_eq!(p.rig.live.resonance, r + 1);
    // SAW LEAD: K3 is the decay (9 in the preset), K4 the release (6).
    p.enc(PANEL_ENC_K1 + 2, -3);
    assert_eq!(p.rig.live.attack_decay & 15, 6);
    p.enc(PANEL_ENC_K4, 2);
    assert_eq!(p.rig.live.sustain_release & 15, 8);
    assert_eq!(p.rig.live.preset, 0, "K4 is no longer the preset");
    p.rig.clear();
    for e in [0, 1] {
        p.enc(e, 5);
    }
    assert!(
        p.rig.log().is_empty(),
        "SELECT and ALGORITHM are the views'"
    );
}

#[test]
fn k4_turns_the_release() {
    let mut p = Panel::new();
    assert_eq!(
        unsafe { live_param_get(&*p.rig.live, LIVE_PARAM_RELEASE) },
        6
    );
    p.enc(PANEL_ENC_K4, 3);
    assert_eq!(
        unsafe { live_param_get(&*p.rig.live, LIVE_PARAM_RELEASE) },
        9
    );
    p.enc(PANEL_ENC_K4, -100);
    assert_eq!(
        unsafe { live_param_get(&*p.rig.live, LIVE_PARAM_RELEASE) },
        0
    );
    p.enc(PANEL_ENC_K4, 100);
    assert_eq!(
        unsafe { live_param_get(&*p.rig.live, LIVE_PARAM_RELEASE) },
        15
    );
    // The release is the SID's: the note's envelope ends on the register.
    assert_eq!(p.rig.live.sustain_release & 15, 15);
    assert_eq!(p.rig.reg(6) & 15, 15, "voice 1's sustain/release register");
    // MIDI learn and the popup find it as a parameter.
    assert_eq!(
        unsafe { panel_knob_param(&*p.p, &*p.rig.live, 0, PANEL_ENC_K4) },
        LIVE_PARAM_RELEASE
    );
}

#[test]
fn presets_steps_the_preset_in_every_view() {
    let mut p = Panel::new();
    p.enc(PANEL_ENC_PRESETS, 1);
    assert_eq!(p.rig.live.preset, 1);
    p.enc(PANEL_ENC_PRESETS, 100);
    assert_eq!(
        u32::from(p.rig.live.preset),
        LIVE_PRESET_SLOTS - 1,
        "clamped at the end, DICE"
    );
    p.enc(PANEL_ENC_PRESETS, -2);
    assert_eq!(u32::from(p.rig.live.preset), LIVE_PRESET_SLOTS - 3);
    p.enc(PANEL_ENC_PRESETS, -100);
    assert_eq!(p.rig.live.preset, 0, "and at the start");
    // The pages other than LIVE's turn it as well (the panel's own rule is the same everywhere).
    for page in [PANEL_PAGE_CHARACTER, PANEL_PAGE_SLOT, PANEL_PAGE_NONE] {
        p.p.page = page as u8;
        p.enc(PANEL_ENC_PRESETS, 1);
        assert_eq!(p.rig.live.preset, 1, "page {page}");
        p.enc(PANEL_ENC_PRESETS, -1);
    }
}

#[test]
fn master_follows_the_pot_with_hysteresis() {
    let mut p = Panel::new();
    p.master(900);
    assert_eq!(p.rig.live.master, 900);
    assert_eq!(p.rig.live.volume, 88);
    p.master(902);
    p.master(897);
    assert_eq!(p.rig.live.master, 900, "ADC noise is ignored");
    p.master(904);
    assert_eq!(p.rig.live.master, 904);
    p.master(-1);
    assert_eq!(p.rig.live.master, 904, "an ADC timeout is ignored");
    p.master(1022);
    p.master(1023);
    assert_eq!(p.rig.live.volume, 100, "the top end always lands");
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

/// The panel has no velocity, so its keys play 127, which sounds as velocity off whatever
/// the target: the full sustain level, pulse width and cutoff.
#[test]
fn panel_notes_sound_as_velocity_off_on_every_target() {
    let off = {
        let mut p = Panel::new();
        p.keys(key(0), key(0));
        p.rig.tick();
        (p.rig.reg(6), p.rig.voice_pw(0), p.rig.live.cutoff_out)
    };
    for target in [
        LIVE_VELOCITY_SUSTAIN,
        LIVE_VELOCITY_CUTOFF,
        LIVE_VELOCITY_PULSE_WIDTH,
    ] {
        let mut p = Panel::new();
        unsafe { live_param_set(&mut *p.rig.live, LIVE_PARAM_VELOCITY_TARGET, target) };
        p.keys(key(0), key(0));
        p.rig.tick();
        let got = (p.rig.reg(6), p.rig.voice_pw(0), p.rig.live.cutoff_out);
        assert_eq!(got, off, "velocity target {target}");
    }
}

/// app/panel.h: the ids M2d leans on, Felucca's (unconfirmed on a SIDOU unit:
/// docs/hardware-test.md).
#[test]
fn the_song_buttons_have_feluccas_ids() {
    assert_eq!(
        [
            PANEL_BTN_LFO,
            PANEL_BTN_GLO,
            PANEL_BTN_ARP,
            PANEL_BTN_SEQ,
            PANEL_BTN_PLAY,
            PANEL_BTN_REC
        ],
        [5, 7, 10, 11, 12, 13]
    );
}

/// GLO's layer: with GLO held one of the six leftmost keys sounds nothing until it goes up and
/// its press is kept for panel_take_glo; the other keys play.
#[test]
fn glo_layer_lets_the_other_keys_play() {
    let mut p = Panel::new();
    unsafe { panel_buttons(&mut *p.p, &mut *p.rig.live, 0, 1 << PANEL_BTN_GLO) };
    p.keys(key(0) | key(5) | key(6), key(0) | key(5) | key(6));
    assert_eq!(p.gated(), 1, "key 6 plays, keys 0 and 5 are the layer's");
    assert_eq!(p.sounding(53 + 6), 1);
    assert_eq!(
        unsafe { panel_take_glo(&mut *p.p) },
        0b100001,
        "key 6 is no toggle"
    );
    assert_eq!(unsafe { panel_take_glo(&mut *p.p) }, 0, "taken");
    // Pressed again under GLO: another toggle; let go of GLO: still nothing until they go up.
    p.keys(key(0) | key(5) | key(6), key(0));
    assert_eq!(unsafe { panel_take_glo(&mut *p.p) }, 1);
    unsafe { panel_buttons(&mut *p.p, &mut *p.rig.live, 0, 0) };
    p.keys(key(0) | key(5) | key(6), 0);
    assert_eq!(p.gated(), 1, "key 6 still sounds");
    p.keys(key(5), 0);
    assert_eq!(p.gated(), 0);
    p.keys(key(0) | key(5), key(0));
    assert_eq!(
        p.gated(),
        1,
        "key 0 up and down again: it plays; key 5 still quiet"
    );
    assert_eq!(unsafe { panel_take_glo(&mut *p.p) }, 0);
}

/// GLO latched is the same layer: the leftmost keys are the layer's, the others play, and GLO's
/// LED is lit.
#[test]
fn a_latched_glo_is_the_layer_without_holding_it() {
    let mut p = Panel::new();
    assert_eq!(p.leds().buttons >> PANEL_BTN_GLO & 1, 0);
    p.p.glo_latch = 1;
    assert_eq!(p.leds().buttons >> PANEL_BTN_GLO & 1, 1, "GLO's LED");
    p.keys(key(1) | key(7), key(1) | key(7));
    assert_eq!(p.gated(), 1, "key 7 plays");
    assert_eq!(unsafe { panel_take_glo(&mut *p.p) }, 0b10);
    p.p.glo_latch = 0;
    p.keys(key(1) | key(7), 0);
    assert_eq!(p.gated(), 1, "key 1 stays quiet until it goes up");
    p.keys(0, 0);
    p.keys(key(1), key(1));
    assert_eq!(p.gated(), 1, "and plays again once pressed");
}

/// panel_quiet_keys: a view takes keys (the song views' jam); a note they sound is released.
#[test]
fn quiet_keys_sound_nothing_until_they_go_up() {
    let mut p = Panel::new();
    p.keys(key(3), key(3));
    assert_eq!(p.gated(), 1);
    unsafe { panel_quiet_keys(&mut *p.p, &mut *p.rig.live, key(3) | key(4)) };
    assert_eq!(p.gated(), 0, "released");
    p.keys(key(3) | key(4), key(4));
    assert_eq!(p.gated(), 0);
    p.keys(0, 0);
    p.keys(key(4), key(4));
    assert_eq!(p.gated(), 1);
}
