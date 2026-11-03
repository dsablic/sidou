// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::lab_sound::{put16, valid, Sound};
use crate::Rig;

const SAW: u8 = 0x20;
const PULSE: u8 = 0x40;
const TRIANGLE: u8 = 0x10;
const NOTE: u8 = 60;

struct Layer {
    control: u8,
    ad: u8,
    sr: u8,
    pulse: u16,
    transpose: i8,
    detune: i8,
}

const LAYERS: [Layer; 3] = [
    Layer { control: SAW, ad: 0x12, sr: 0xa3, pulse: 0x800, transpose: 0, detune: 0 },
    Layer { control: PULSE, ad: 0x45, sr: 0x86, pulse: 0x400, transpose: 12, detune: 0 },
    Layer { control: TRIANGLE, ad: 0x78, sr: 0x59, pulse: 0x200, transpose: -12, detune: 25 },
];

fn tuning_at(layer: usize) -> usize {
    (LAB_SOUND_LAYER_TUNING + layer as u32 * LAB_SOUND_LAYER_TUNING_BYTES) as usize
}

fn voice_at(layer: usize) -> usize {
    (LAB_SOUND_LAYER_VOICE + (layer as u32 - 1) * LAB_SOUND_LAYER_VOICE_BYTES) as usize
}

fn layered(count: usize) -> Sound {
    let mut s = Rig::new(SID_MODEL_6581).sound();
    s[LAB_SOUND_MONO_SYNC as usize] = 0;
    s[LAB_SOUND_LAYERS as usize] = (count - 1) as u8;
    for (n, layer) in LAYERS.iter().enumerate().take(count) {
        s[tuning_at(n)] = layer.transpose as u8;
        s[tuning_at(n) + 1] = layer.detune as u8;
        if n == 0 {
            s[LAB_SOUND_CONTROL as usize] = layer.control;
            s[LAB_SOUND_AD as usize] = layer.ad;
            s[LAB_SOUND_SR as usize] = layer.sr;
            put16(&mut s, LAB_SOUND_PULSE, u32::from(layer.pulse));
        } else {
            let at = voice_at(n);
            s[at] = layer.control;
            s[at + 1] = layer.ad;
            s[at + 2] = layer.sr;
            s[at + 3] = layer.pulse as u8;
            s[at + 4] = (layer.pulse >> 8) as u8;
        }
    }
    s[LAB_SOUND_VELOCITY as usize] = LIVE_VELOCITY_OFF as u8;
    s[LAB_SOUND_PRESSURE as usize] = 0;
    put16(&mut s, LAB_SOUND_PWM_DEPTH, 0);
    s[LAB_SOUND_VIBRATO_DEPTH as usize] = 0;
    s[LAB_SOUND_ARP as usize + LIVE_ARP_ON as usize] = 0;
    assert!(valid(&s), "{s:?}");
    s
}

fn rig_with(s: &Sound) -> Rig {
    let mut rig = Rig::new(SID_MODEL_6581);
    unsafe { live_sound_apply(&mut *rig.live, s.as_ptr()) };
    rig
}

fn ratio(a: u16, b: u16) -> f64 {
    f64::from(a) / f64::from(b)
}

#[test]
fn a_layered_note_plays_each_layer_on_its_own_voice() {
    let mut rig = rig_with(&layered(3));
    rig.note_on(NOTE);
    for (v, layer) in LAYERS.iter().enumerate() {
        let v = v as u8;
        assert_eq!(rig.control(v), layer.control | 1, "voice {v}");
        assert_eq!(rig.reg(v * 7 + 5), layer.ad, "voice {v}");
        assert_eq!(rig.reg(v * 7 + 6), layer.sr, "voice {v}");
        assert_eq!(rig.voice_pw(v), layer.pulse, "voice {v}");
    }
    assert!((ratio(rig.voice_freq(1), rig.voice_freq(0)) - 2.0).abs() < 0.002);
    let down_and_detuned = 0.5 * 2f64.powf(25.0 / 1200.0);
    assert!((ratio(rig.voice_freq(2), rig.voice_freq(0)) - down_and_detuned).abs() < 0.002);
    rig.note_off(NOTE);
    assert!((0..3).all(|v| !rig.gated(v)));
}

#[test]
fn a_layered_sound_plays_one_note_at_a_time() {
    let mut rig = rig_with(&layered(2));
    rig.note_on(NOTE);
    let first = rig.voice_freq(0);
    rig.note_on(NOTE + 7);
    assert!(rig.voice_freq(0) > first);
    assert!((ratio(rig.voice_freq(1), rig.voice_freq(0)) - 2.0).abs() < 0.002);
    assert!(rig.gated(0) && rig.gated(1));
    assert_eq!(rig.control(2), 0, "the layer past the count is silent");
}

#[test]
fn fewer_layers_leave_the_other_voices_silent_and_one_layer_is_poly_again() {
    let mut rig = rig_with(&layered(2));
    rig.note_on(NOTE);
    assert!(rig.gated(0) && rig.gated(1) && !rig.gated(2));

    let mut poly = layered(1);
    poly[tuning_at(0)] = 12;
    let mut rig = rig_with(&poly);
    rig.note_on(NOTE);
    rig.note_on(NOTE + 4);
    rig.note_on(NOTE + 7);
    assert!((0..3).all(|v| rig.gated(v)), "three notes, three voices");
    let mut plain = poly;
    plain[tuning_at(0)] = 0;
    let mut reference = rig_with(&plain);
    reference.note_on(NOTE);
    assert!((ratio(rig.voice_freq(0), reference.voice_freq(0)) - 2.0).abs() < 0.002);
}

#[test]
fn a_layered_sound_round_trips_through_live() {
    for count in 1..=3 {
        let s = layered(count);
        assert_eq!(rig_with(&s).sound(), s, "{count} layers");
    }
}

#[test]
fn layers_out_of_range_or_with_one_voice_sync_are_no_sound() {
    let good = layered(3);
    let mut broken = Vec::new();
    let mut s = good;
    s[LAB_SOUND_LAYERS as usize] = 3;
    broken.push(("four layers", s));
    let mut s = good;
    s[LAB_SOUND_MONO_SYNC as usize] = 1;
    broken.push(("layers with one-voice sync", s));
    let mut s = good;
    s[tuning_at(1)] = (LIVE_TRANSPOSE_MAX as i8 + 1) as u8;
    broken.push(("transpose", s));
    let mut s = good;
    s[tuning_at(2) + 1] = (-(LIVE_DETUNE_CENTS as i32) - 1) as i8 as u8;
    broken.push(("detune", s));
    let mut s = good;
    s[voice_at(2)] |= 1;
    broken.push(("a gate in a layer", s));
    let mut s = good;
    s[voice_at(1) + 4] = 0x10;
    broken.push(("a layer's pulse", s));
    for (what, s) in broken {
        assert!(!valid(&s), "{what}");
    }
}
