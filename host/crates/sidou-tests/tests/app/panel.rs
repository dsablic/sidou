// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::{cutoff_after, freq_table, Rig, PANEL_ENC_K2, PANEL_ENC_K3};

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
        unsafe { panel_get_leds(&*self.p, &*self.rig.live, std::ptr::null()) }
    }

    fn leds_in(&self, i: &panel_led_in) -> panel_leds {
        unsafe { panel_get_leds(&*self.p, &*self.rig.live, i) }
    }

    fn voices_gated_at(&self, note: u8) -> usize {
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

const fn btn(b: u32) -> u32 {
    1 << b
}

const C4: u8 = 60;
const C4_KEY: u32 = 7;
const SAW_LEAD_DECAY: u8 = 9;
const SAW_LEAD_RELEASE: u8 = 6;

#[test]
fn key_0_is_f3_and_key_16_is_a4() {
    let mut p = Panel::new();
    p.keys(key(0), key(0));
    assert_eq!(p.voices_gated_at(53), 1);
    assert_eq!(p.p.key_note[0], 53);
    p.keys(key(0) | key(16), key(16));
    assert_eq!(p.voices_gated_at(69), 1);
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
    p.keys(key(C4_KEY), key(C4_KEY));
    p.buttons(btn(PANEL_BTN_OCT_UP));
    assert_eq!(p.rig.live.octave, 1);
    assert_eq!(p.voices_gated_at(C4), 1, "the held note keeps sounding");
    p.keys(0, 0);
    assert_eq!(
        p.gated(),
        0,
        "a stuck note: key-up sent the new octave's note"
    );
    p.keys(key(C4_KEY), key(C4_KEY));
    assert_eq!(
        p.voices_gated_at(C4 + 12),
        1,
        "the next press plays the new octave"
    );
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
    let let_go_and_pressed_again_between_the_polls = key(2);
    p.keys(key(2), let_go_and_pressed_again_between_the_polls);
    let gates: Vec<u8> = p
        .rig
        .log()
        .iter()
        .filter(|&&(r, _)| r % 7 == 4 && r < 21)
        .map(|&(_, v)| v & 1)
        .collect();
    assert_eq!(gates.first(), Some(&0), "the gate goes off first");
    assert_eq!(gates.last(), Some(&1), "and on again");
    assert_eq!(p.voices_gated_at(55), 1);
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
fn buttons_turn_the_octave_and_fx_and_env_do_nothing_here() {
    let mut p = Panel::new();
    p.buttons(btn(PANEL_BTN_OCT_DOWN));
    assert_eq!(p.rig.live.octave, -1);
    p.buttons(btn(PANEL_BTN_OCT_UP) | btn(PANEL_BTN_OCT_DOWN));
    assert_eq!(p.rig.live.octave, -1, "both at once cancel out");
    p.rig.clear();
    p.buttons(btn(PANEL_BTN_FX) | btn(PANEL_BTN_ENV));
    assert_eq!(
        p.rig.live.model, SID_MODEL_6581,
        "FX opens LAB from LIVE (app/views.c)"
    );
    assert_eq!(
        p.rig.live.slot[0][LIVE_SLOT_ALGORITHM as usize], 0,
        "ENV too"
    );
    assert!(p.rig.log().is_empty(), "FX and ENV do nothing in the panel");
    p.buttons(1 << 3 | 1 << 12 | 1 << 13);
    assert!(p.rig.log().is_empty(), "the other buttons do nothing yet");
}

#[test]
fn k1_to_k4_are_cutoff_resonance_the_sounds_own_control_and_release() {
    let mut p = Panel::new();
    let (c, r) = (p.rig.live.cutoff, p.rig.live.resonance);
    p.enc(PANEL_ENC_K1, -2);
    assert_eq!(p.rig.live.cutoff, cutoff_after(p.rig.live.model, c, -2));
    p.enc(PANEL_ENC_K2, 1);
    assert_eq!(p.rig.live.resonance, r + 1);
    p.enc(PANEL_ENC_K3, -3);
    assert_eq!(p.rig.live.attack_decay & 15, SAW_LEAD_DECAY - 3);
    p.enc(PANEL_ENC_K4, 2);
    assert_eq!(p.rig.live.sustain_release & 15, SAW_LEAD_RELEASE + 2);
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
        u32::from(SAW_LEAD_RELEASE)
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
    assert_eq!(p.rig.live.sustain_release & 15, 15);
    assert_eq!(p.rig.reg(6) & 15, 15, "voice 1's sustain/release register");
    assert_eq!(
        unsafe { panel_knob_param(&*p.p, &*p.rig.live, 0, PANEL_ENC_K4) },
        LIVE_PARAM_RELEASE,
        "MIDI learn and the popup find it as a parameter"
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

unsafe extern "C" fn count_hook(ctx: *mut std::ffi::c_void) {
    unsafe { *ctx.cast::<u32>() += 1 };
}

fn panel_counting_master_asks(asks: &mut u32) -> Panel {
    let mut p = Panel::new();
    p.p.master_hook = Some(count_hook);
    p.p.master_ctx = (asks as *mut u32).cast();
    p
}

fn noisy_pot_reading(seed: &mut u32, at: i32, n: i32) -> i32 {
    *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
    (at + (*seed >> 16) as i32 % (2 * n + 1) - n).clamp(0, 1023)
}

#[test]
fn a_noisy_pot_at_rest_never_asks_for_the_popup() {
    for at in [0, 3, 512, 1020, 1023] {
        for n in [1, 2, 4, 6, 7] {
            let mut asks = 0u32;
            let mut p = panel_counting_master_asks(&mut asks);
            p.master(at);
            let first = asks;
            let mut seed = (at * 31 + n) as u32;
            for _ in 0..20_000 {
                p.master(noisy_pot_reading(&mut seed, at, n));
            }
            assert_eq!(asks, first, "pot at {at}, noise +-{n}");
            let g = i32::from(p.rig.live.master);
            assert!(
                (g - at).abs() <= n + PANEL_MASTER_END as i32,
                "gain {g} at {at}"
            );
            if at <= 3 || at >= 1020 {
                assert!(g == 0 || g == 1023, "an end holds through the noise: {g}");
            }
        }
    }
}

#[test]
fn a_real_turn_asks_and_its_gain_reaches_the_target() {
    for (counts_a_tick, path) in [(1, [512, 0, 1023, 600]), (40, [512, 1023, 0, 300])] {
        let mut asks = 0u32;
        let mut p = panel_counting_master_asks(&mut asks);
        let mut seed = 7u32;
        let mut at = path[0];
        p.master(at);
        for &to in &path[1..] {
            let before = asks;
            while at != to {
                at += (to - at).clamp(-counts_a_tick, counts_a_tick);
                p.master(noisy_pot_reading(&mut seed, at, 2));
            }
            for _ in 0..5 {
                p.master(at);
            }
            assert!(
                asks > before,
                "a turn to {to} ({counts_a_tick} a tick) asks for the popup"
            );
            let g = i32::from(p.rig.live.master);
            if to == 0 || to == 1023 {
                assert_eq!(g, to, "an end lands exactly");
            } else {
                assert!(
                    (g - to).abs() < PANEL_MASTER_HYSTERESIS as i32,
                    "gain {g} for {to}"
                );
            }
            assert!(p.p.master_follow > 0, "the popup follows the turn to {to}");
        }
        for _ in 0..PANEL_MASTER_FOLLOW {
            p.master(at);
        }
        let rest = asks;
        for _ in 0..5_000 {
            p.master(noisy_pot_reading(&mut seed, at, 6));
        }
        assert_eq!(asks, rest, "at rest after the turn: no popup");
    }
}

#[test]
fn leds_show_held_keys_the_octave_and_the_8580() {
    let mut p = Panel::new();
    let l = p.leds();
    assert_eq!((l.buttons, l.notes), (0, 0));
    p.keys(key(1) | key(26), key(1) | key(26));
    assert_eq!(p.leds().notes, key(1) | key(26));
    p.buttons(btn(PANEL_BTN_OCT_UP));
    assert_eq!(p.leds().buttons, btn(PANEL_BTN_OCT_UP));
    p.buttons(btn(PANEL_BTN_OCT_DOWN));
    p.buttons(btn(PANEL_BTN_OCT_DOWN));
    assert_eq!(p.leds().buttons, btn(PANEL_BTN_OCT_DOWN));
    p.rig.button(LIVE_BUTTON_FX);
    assert_eq!(
        p.leds().buttons,
        btn(PANEL_BTN_OCT_DOWN) | btn(PANEL_BTN_FX),
        "the 8580 lights FX"
    );
    p.keys(key(26), 0);
    assert_eq!(p.leds().notes, key(26));
}

#[test]
fn panel_keys_play_velocity_127_which_sounds_as_velocity_off_on_every_target() {
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

#[test]
fn glo_held_keeps_the_six_leftmost_keys_presses_for_its_layer_and_lets_the_other_keys_play() {
    let mut p = Panel::new();
    unsafe { panel_buttons(&mut *p.p, &mut *p.rig.live, 0, btn(PANEL_BTN_GLO)) };
    p.keys(key(0) | key(5) | key(6), key(0) | key(5) | key(6));
    assert_eq!(p.gated(), 1, "key 6 plays, keys 0 and 5 are the layer's");
    assert_eq!(p.voices_gated_at(53 + 6), 1);
    assert_eq!(
        unsafe { panel_take_glo(&mut *p.p) },
        0b100001,
        "key 6 is no toggle"
    );
    assert_eq!(unsafe { panel_take_glo(&mut *p.p) }, 0, "taken");
    p.keys(key(0) | key(5) | key(6), key(0));
    assert_eq!(
        unsafe { panel_take_glo(&mut *p.p) },
        1,
        "pressed again under GLO: another toggle"
    );
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

#[test]
fn a_latched_glo_is_the_layer_without_holding_it_and_lights_glo() {
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

#[test]
fn quiet_keys_release_their_note_and_sound_nothing_until_they_go_up() {
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

fn led_in() -> panel_led_in {
    panel_led_in {
        beat: PANEL_BEAT_NONE as u8,
        step_head: PANEL_NO_STEP as u8,
        ..Default::default()
    }
}

#[test]
fn no_input_shows_the_panels_own_lights_and_lit_hides_dim() {
    let mut p = Panel::new();
    p.rig.live.arp_on = 1;
    p.p.playing = 1;
    let l = p.leds();
    assert_eq!(l.buttons, 0, "no input: no beat, no step");
    assert_eq!(l.buttons_dim, btn(PANEL_BTN_ARP) | btn(PANEL_BTN_PLAY));
    let mut i = led_in();
    i.beat = 0;
    let l = p.leds_in(&i);
    assert_eq!(l.buttons, btn(PANEL_BTN_PLAY));
    assert_eq!(l.buttons_dim & l.buttons, 0);
}

#[test]
fn arp_is_dim_while_on_and_lit_in_the_first_half_of_each_step() {
    let mut p = Panel::new();
    let l = &mut p.rig.live;
    l.arp_voice = 0;
    let three_frames_a_step = 2;
    l.arp_rate = three_frames_a_step;
    assert_eq!(p.leds().buttons_dim, 0, "off: dark");
    p.rig.live.arp_on = 1;
    for (wait, lit) in [(6, true), (5, true), (4, true), (3, false), (1, false)] {
        p.rig.live.arp_wait = wait;
        let l = p.leds();
        assert_eq!(l.buttons >> PANEL_BTN_ARP & 1, lit as u32, "wait {wait}");
        assert_eq!(
            (l.buttons | l.buttons_dim) >> PANEL_BTN_ARP & 1,
            1,
            "on is always at least dim"
        );
    }
    p.rig.live.arp_voice = LIVE_ARP_NO_VOICE as u8;
    p.rig.live.arp_wait = 6;
    let l = p.leds();
    assert_eq!(
        (l.buttons, l.buttons_dim),
        (0, btn(PANEL_BTN_ARP)),
        "on with nothing held: dim, never lit"
    );
}

#[test]
fn arp_at_one_or_two_frames_a_step_lights_once_a_cycle() {
    for rate in [0u8, 1] {
        let mut p = Panel::new();
        let l = &mut p.rig.live;
        l.arp_on = 1;
        l.arp_voice = 0;
        l.arp_rate = rate;
        let a_step_just_began = (rate + 1) * 2;
        l.arp_wait = a_step_just_began;
        for pos in 0..4 {
            p.rig.live.arp_pos = pos;
            let lit = p.leds().buttons >> PANEL_BTN_ARP & 1;
            assert_eq!(lit, u32::from(pos == 0), "rate {rate} pos {pos}");
        }
    }
}

#[test]
fn arp_on_a_midi_clock_counts_clocks() {
    let mut p = Panel::new();
    let l = &mut p.rig.live;
    l.arp_on = 1;
    l.arp_voice = 0;
    let sixteenths_of_six_clocks = LIVE_ARP_RATE_CLOCK as u8 + 1;
    l.arp_rate = sixteenths_of_six_clocks;
    l.arp_clock_seen = 1;
    l.arp_wait = 1;
    for (clocks, lit) in [(6, 1), (4, 1), (3, 0), (1, 0)] {
        p.rig.live.arp_clocks = clocks;
        assert_eq!(
            p.leds().buttons >> PANEL_BTN_ARP & 1,
            lit,
            "clocks {clocks}"
        );
    }
}

#[test]
fn play_is_lit_on_each_beats_first_quarter_dim_between_and_dark_stopped() {
    let mut p = Panel::new();
    let mut i = led_in();
    assert_eq!(
        p.leds_in(&i).buttons | p.leds_in(&i).buttons_dim,
        0,
        "stopped"
    );
    p.p.playing = 1;
    for (beat, lit) in [(0, true), (63, true), (64, false), (200, false)] {
        i.beat = beat;
        let l = p.leds_in(&i);
        assert_eq!(l.buttons >> PANEL_BTN_PLAY & 1, lit as u32, "beat {beat}");
        assert_eq!(
            l.buttons_dim >> PANEL_BTN_PLAY & 1,
            !lit as u32,
            "beat {beat}"
        );
    }
    i.beat = PANEL_BEAT_NONE as u8;
    let l = p.leds_in(&i);
    assert_eq!(
        (l.buttons, l.buttons_dim),
        (0, btn(PANEL_BTN_PLAY)),
        "playing but no beat: dim"
    );
}

#[test]
fn rec_is_dim_armed_blinks_in_the_count_in_and_lit_recording() {
    let p = Panel::new();
    let mut i = led_in();
    let rec = btn(PANEL_BTN_REC);
    let l = p.leds_in(&i);
    assert_eq!((l.buttons, l.buttons_dim), (0, 0));
    i.rec = PANEL_REC_ARMED as u8;
    let l = p.leds_in(&i);
    assert_eq!((l.buttons, l.buttons_dim), (0, rec));
    i.rec = PANEL_REC_ON as u8;
    assert_eq!(p.leds_in(&i).buttons, rec);
    i.rec = PANEL_REC_COUNT as u8;
    let mut seen = [false; 2];
    for ms in (0..2000).step_by(10) {
        i.ms = ms;
        let l = p.leds_in(&i);
        assert_eq!(l.buttons_dim, 0, "a blink is lit or dark");
        seen[usize::from(l.buttons == rec)] = true;
        let again = {
            i.ms = ms + 2 * PANEL_BLINK_HALF_MS;
            p.leds_in(&i).buttons
        };
        assert_eq!(again, l.buttons, "one period on");
    }
    assert_eq!(seen, [true, true], "it blinks");
}

#[test]
fn save_blinks_while_a_question_waits() {
    let p = Panel::new();
    let mut i = led_in();
    let save = btn(PANEL_BTN_SAVE);
    for ms in (0..1000).step_by(50) {
        i.ms = ms;
        assert_eq!(p.leds_in(&i).buttons, 0, "no question");
    }
    i.ask = 1;
    let on = (0..1000)
        .step_by(10)
        .filter(|&ms| {
            i.ms = ms;
            p.leds_in(&i).buttons == save
        })
        .count();
    assert!(
        (40..=60).contains(&on),
        "lit about half of 100 samples: {on}"
    );
}

#[test]
fn glo_layer_shows_the_voices_while_held_or_latched() {
    let mut p = Panel::new();
    let mut i = led_in();
    let (voice_2, voice_3) = (0b010, 0b100);
    i.silent = voice_2;
    i.solo = voice_3;
    p.keys(key(7), key(7));
    assert_eq!(
        p.leds_in(&i).notes,
        key(7),
        "GLO not held: the keys are the player's"
    );
    p.keys(0, 0);
    for latch in [false, true] {
        p.p.held = if latch { 0 } else { btn(PANEL_BTN_GLO) };
        p.p.glo_latch = u8::from(latch);
        let mut on = [false; 2];
        for ms in (0..1000).step_by(10) {
            i.ms = ms;
            let l = p.leds_in(&i);
            let layer = l.notes & 0x3f;
            assert_eq!(
                layer & 0b101,
                0b101,
                "voices 1 and 3 sound: keys 1 and 3 lit"
            );
            assert_eq!(layer & 0b010, 0, "voice 2 muted: key 2 dark");
            assert_eq!(
                l.notes_dim & 0b111000,
                0b011000,
                "solo keys: voices 1 and 2 not soloed, dim"
            );
            on[usize::from(layer & 0b100000 != 0)] = true;
        }
        assert_eq!(on, [true, true], "the solo blinks");
    }
    p.p.held = btn(PANEL_BTN_GLO);
    p.keys(key(9) | key(1), key(9) | key(1));
    let l = p.leds_in(&i);
    assert_eq!(l.notes & key(9), key(9));
    assert_eq!(
        l.notes & key(1),
        0,
        "a GLO key sounds nothing and shows the voice"
    );
}

#[test]
fn step_page_shows_set_steps_lit_empty_ones_dim_and_the_playhead_flicks() {
    let mut p = Panel::new();
    let mut i = led_in();
    i.step_on = 1;
    let a_short_last_page_of_8_steps = 0x00ff;
    i.step_rows = a_short_last_page_of_8_steps;
    i.step_set = 0b0001_0001;
    i.ms = 1000;
    i.head_ms = 0;
    p.keys(key(2) | key(20), key(2) | key(20));
    let l = p.leds_in(&i);
    assert_eq!(
        l.notes,
        0b0001_0001 | key(20),
        "held keys light only above the 16"
    );
    assert_eq!(l.notes_dim, 0x00ff & !0b0001_0001);
    i.step_head = 4;
    i.head_ms = 1000 - 10;
    let l = p.leds_in(&i);
    assert_eq!(l.notes & key(4), 0, "a set step goes dark");
    assert_eq!(l.notes_dim & key(4), 0);
    i.head_ms = 1000 - PANEL_FLICK_MS - 1;
    assert_eq!(
        p.leds_in(&i).notes & key(4),
        key(4),
        "lit again after PANEL_FLICK_MS"
    );
    i.step_head = 5;
    i.head_ms = 1000 - 10;
    let l = p.leds_in(&i);
    assert_eq!(
        (l.notes & key(5), l.notes_dim & key(5)),
        (key(5), 0),
        "the playhead lights an empty step"
    );
    i.head_ms = 0;
    let l = p.leds_in(&i);
    assert_eq!(
        (l.notes & key(5), l.notes_dim & key(5)),
        (0, key(5)),
        "then it is dim again"
    );
    i.step_head = 12;
    i.head_ms = 1000;
    assert_eq!(
        p.leds_in(&i).notes & key(12),
        0,
        "no playhead on a step that does not exist"
    );
    p.p.glo_latch = 1;
    i.silent = 0b111;
    i.head_ms = 0;
    let l = p.leds_in(&i);
    assert_eq!(l.notes & 0x3f, 0, "all muted");
    assert_eq!(l.notes_dim & 0x3f, 0b111000, "solo keys available");
    assert_eq!(
        l.notes_dim & 0xc0,
        0xc0 & 0x00ff,
        "steps 7 and 8 still the page's"
    );
}
