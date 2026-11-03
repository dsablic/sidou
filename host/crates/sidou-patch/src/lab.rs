// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt;

use sidou_sys::*;

use crate::model::{Oscillator, Patch, VoiceMode, OSCILLATORS};

pub const BYTES: usize = LAB_SOUND_BYTES as usize;
const CONTROL_KEPT: u8 = 0xF6;
const PULSE_UNIT: u16 = 32;
const CUTOFF_UNIT: u16 = 16;
const DETUNE_STEPS_PER_SEMITONE: i32 = 64;
const LAB_NAME: usize = LAB_SOUND_NAME as usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Loss {
    NoOscillator,
    Table(usize),
    FixedNote(usize),
    TransposeClamped(usize),
    DetuneClamped(usize),
    FilterEnvelope,
}

impl fmt::Display for Loss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOscillator => f.write_str("NO OSC ACTIVE: OSC 1 KEPT"),
            Self::Table(n) => write!(f, "OSC {n} TABLE: SONG INSTRUMENT"),
            Self::FixedNote(n) => write!(f, "OSC {n} FIXED NOTE: PLAYS THE KEY"),
            Self::TransposeClamped(n) => write!(f, "OSC {n} TRANSPOSE CLAMPED"),
            Self::DetuneClamped(n) => write!(f, "OSC {n} DETUNE CLAMPED"),
            Self::FilterEnvelope => f.write_str("FILTER ENVELOPE DROPPED"),
        }
    }
}

pub fn lab_name(name: &str) -> String {
    name.chars()
        .map(|c| c.to_ascii_uppercase())
        .map(|c| if (' '..='_').contains(&c) { c } else { ' ' })
        .take(LAB_NAME)
        .collect::<String>()
        .trim()
        .to_string()
}

fn sounding(patch: &Patch, losses: &mut Vec<Loss>) -> Vec<usize> {
    let active: Vec<usize> = (0..OSCILLATORS).filter(|&n| patch.active[n]).collect();
    if active.is_empty() {
        losses.push(Loss::NoOscillator);
        return vec![0];
    }
    match patch.mode {
        VoiceMode::Poly => vec![active[0]],
        VoiceMode::Single => active,
    }
}

fn clamped(value: i32, max: i32, loss: Loss, losses: &mut Vec<Loss>) -> i8 {
    if value.abs() > max {
        losses.push(loss);
    }
    value.clamp(-max, max) as i8
}

fn tuning(osc: &Oscillator, n: usize, losses: &mut Vec<Loss>) -> (i8, i8) {
    let semis = clamped(
        i32::from(osc.transpose),
        LIVE_TRANSPOSE_MAX as i32,
        Loss::TransposeClamped(n + 1),
        losses,
    );
    let cents = (i32::from(osc.detune) * 100
        + DETUNE_STEPS_PER_SEMITONE / 2 * i32::from(osc.detune).signum())
        / DETUNE_STEPS_PER_SEMITONE;
    let cents = clamped(
        cents,
        LIVE_DETUNE_CENTS as i32,
        Loss::DetuneClamped(n + 1),
        losses,
    );
    (semis, cents)
}

fn voice(osc: &Oscillator) -> [u8; 5] {
    let pulse = (u16::from(osc.pulse_width_start) * PULSE_UNIT).min(LIVE_PULSE_WIDTH_MAX as u16);
    let [lo, hi] = pulse.to_le_bytes();
    [
        osc.control & CONTROL_KEPT,
        osc.attack << 4 | osc.decay & 0x0F,
        osc.sustain << 4 | osc.release & 0x0F,
        lo,
        hi,
    ]
}

fn report(patch: &Patch, layers: &[usize], losses: &mut Vec<Loss>) {
    for &n in layers {
        let osc = &patch.oscillators[n];
        if osc.track != 0 {
            losses.push(Loss::FixedNote(n + 1));
        }
        if !patch.tables[n].steps.is_empty() {
            losses.push(Loss::Table(n + 1));
        }
    }
    if patch.filter.envelope_depth != 0 {
        losses.push(Loss::FilterEnvelope);
    }
}

pub fn lab_sound(patch: &Patch) -> ([u8; BYTES], Vec<Loss>) {
    let mut losses = Vec::new();
    let layers = sounding(patch, &mut losses);
    let mut s = [0u8; BYTES];
    for (layer, &n) in layers.iter().enumerate() {
        let osc = &patch.oscillators[n];
        let v = voice(osc);
        if layer == 0 {
            s[LAB_SOUND_CONTROL as usize..][..3].copy_from_slice(&v[..3]);
            s[LAB_SOUND_PULSE as usize..][..2].copy_from_slice(&v[3..]);
        } else {
            let at =
                (LAB_SOUND_LAYER_VOICE + (layer as u32 - 1) * LAB_SOUND_LAYER_VOICE_BYTES) as usize;
            s[at..][..5].copy_from_slice(&v);
        }
        let (semis, cents) = tuning(osc, n, &mut losses);
        let at = (LAB_SOUND_LAYER_TUNING + layer as u32 * LAB_SOUND_LAYER_TUNING_BYTES) as usize;
        s[at] = semis as u8;
        s[at + 1] = cents as u8;
        if patch.filter.routed[n] {
            s[LAB_SOUND_FILTER_VOICES as usize] |= 1 << layer;
        }
    }
    s[LAB_SOUND_LAYERS as usize] = (layers.len() - 1) as u8;
    let cutoff = u16::from(patch.filter.cutoff) * CUTOFF_UNIT;
    s[LAB_SOUND_CUTOFF as usize..][..2]
        .copy_from_slice(&cutoff.min(LIVE_CUTOFF_MAX as u16).to_le_bytes());
    s[LAB_SOUND_RESONANCE as usize] = patch.filter.resonance;
    s[LAB_SOUND_FILTER_MODE as usize] = patch.filter.mode << 4;
    s[LAB_SOUND_BEND_RANGE as usize] = patch.oscillators[layers[0]]
        .bend_range
        .min(LIVE_BEND_RANGE_MAX as u8);
    s[LAB_SOUND_ARP as usize + LIVE_ARP_SPAN as usize] = 1;
    report(patch, &layers, &mut losses);
    (s, losses)
}
