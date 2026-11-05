// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt;

use sidou_sys::*;

use crate::bank::{frames_per_step, has_table, right_of};
use crate::model::{Oscillator, Patch, Unmapped, VoiceMode, OSCILLATORS};

pub const BYTES: usize = LAB_SOUND_BYTES as usize;
const CONTROL_KEPT: u8 = 0xF6;
const PULSE_UNIT: u16 = 32;
const CUTOFF_UNIT: u16 = 16;
const DETUNE_STEPS_PER_SEMITONE: i32 = 64;
const LAB_NAME: usize = LAB_SOUND_NAME as usize;
const SIDSTATION_TO_NIBBLE: u32 = 3;
const SIDSTATION_DEPTH_TO_LAB: i32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Loss {
    NoOscillator,
    Table(usize),
    TableCut(usize),
    TableRate(usize),
    LfoDropped(&'static str),
    FixedNote(usize),
    TransposeClamped(usize),
    DetuneClamped(usize),
    Unmapped(Unmapped),
}

impl fmt::Display for Loss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOscillator => f.write_str("NO OSC ACTIVE: OSC 1 KEPT"),
            Self::Table(n) => write!(f, "OSC {n} TABLE: SONG INSTRUMENT"),
            Self::TableCut(n) => write!(f, "OSC {n} TABLE CUT TO {LAB_TABLE_STEPS_MAX}"),
            Self::TableRate(n) => write!(f, "OSC {n} TABLE SPEED CLAMPED"),
            Self::LfoDropped(what) => write!(f, "{what} LFO: DROPPED (TWO LFOS)"),
            Self::FixedNote(n) => write!(f, "OSC {n} FIXED NOTE: PLAYS THE KEY"),
            Self::TransposeClamped(n) => write!(f, "OSC {n} TRANSPOSE CLAMPED"),
            Self::DetuneClamped(n) => write!(f, "OSC {n} DETUNE CLAMPED"),
            Self::Unmapped(u) => match u {
                Unmapped::ModulationPath(n) => write!(f, "MOD PATH {n}: DROPPED"),
                Unmapped::Wavetable(n) => write!(f, "WAVETABLE {n}: DROPPED"),
                Unmapped::Arpeggio(n) => write!(f, "OSC {n} ARP: DROPPED"),
                Unmapped::Portamento(n) => write!(f, "OSC {n} PORTAMENTO: DROPPED"),
                Unmapped::FilterKeyTracking => f.write_str("FILTER KEYTRACK: DROPPED"),
                Unmapped::RightSid => f.write_str("RIGHT SID: LEFT KEPT"),
            },
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

pub fn layer_oscillators(patch: &Patch) -> Vec<usize> {
    sounding(patch, &mut Vec::new())
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

fn report(patch: &Patch, layers: &[usize], carried: Option<usize>, losses: &mut Vec<Loss>) {
    for &n in layers {
        let osc = &patch.oscillators[n];
        if osc.track != 0 {
            losses.push(Loss::FixedNote(n + 1));
        }
        if has_table(patch, n) && carried != Some(n) {
            losses.push(Loss::Table(n + 1));
        }
    }
}

const STEPS_MAX: usize = LAB_TABLE_STEPS_MAX as usize;
const GATE: u8 = 0x01;

fn table(
    patch: &Patch,
    layers: &[usize],
    s: &mut [u8; BYTES],
    losses: &mut Vec<Loss>,
) -> Option<usize> {
    let (layer, &n) = layers
        .iter()
        .enumerate()
        .find(|&(_, &n)| has_table(patch, n))?;
    let t = &patch.tables[n];
    if t.steps.len() > STEPS_MAX {
        losses.push(Loss::TableCut(n + 1));
    }
    let steps = t.steps.len().min(STEPS_MAX);
    let frames = frames_per_step(patch, n);
    if frames > LAB_TABLE_RATE_MAX {
        losses.push(Loss::TableRate(n + 1));
    }
    let at = LAB_SOUND_TABLE as usize;
    s[at] = (LAB_TABLE_ON | (layer as u32) << LAB_TABLE_LAYER_SHIFT) as u8 | steps as u8;
    s[LAB_SOUND_TABLE_LOOP as usize] =
        t.loop_to
            .map_or(0, |l| if usize::from(l) < steps { l + 1 } else { 0 });
    s[LAB_SOUND_TABLE_RATE as usize] = (frames.min(LAB_TABLE_RATE_MAX) - 1) as u8;
    for (i, step) in t.steps.iter().take(steps).enumerate() {
        let at = LAB_SOUND_TABLE_STEPS as usize + 2 * i;
        s[at] = step.control & !GATE;
        s[at + 1] = right_of(step.note, 0);
    }
    Some(n)
}

fn nibble(value: u8) -> u8 {
    (u32::from(value) >> SIDSTATION_TO_NIBBLE).min(15) as u8
}

fn filter_envelope(patch: &Patch, s: &mut [u8; BYTES]) {
    let f = &patch.filter;
    let depth =
        (i32::from(f.envelope_depth) / SIDSTATION_DEPTH_TO_LAB).min(LIVE_FENV_DEPTH_MAX as i32);
    let signed = if f.envelope_invert { -depth } else { depth };
    s[LAB_SOUND_FILTER_ENV as usize] = signed as i8 as u8;
    s[LAB_SOUND_FILTER_ENV_AD as usize] = nibble(f.attack) << 4 | nibble(f.decay);
    s[LAB_SOUND_FILTER_ENV_SR as usize] = nibble(f.sustain) << 4 | nibble(f.release);
}

const SIDSTATION_LFO_FLAT: u8 = 5;
const SIDSTATION_SPEED_MAX: u32 = 127;
const SIDSTATION_DEPTH_MAX: i32 = 127;
const PITCH_DEPTH_A_SEMITONE: i32 = LAB_LFO_DEPTH_MAX as i32 / 12;

fn lfos(patch: &Patch, first: usize, s: &mut [u8; BYTES], losses: &mut Vec<Loss>) {
    let osc = &patch.oscillators[first];
    let wanted = [
        ("VIBRATO", osc.vibrato_lfo, osc.vibrato_depth, LAB_LFO_PITCH),
        (
            "PWM",
            osc.pulse_width_lfo,
            osc.pulse_width_lfo_depth,
            LAB_LFO_PULSE,
        ),
        (
            "FILTER",
            patch.filter.lfo,
            patch.filter.lfo_depth,
            LAB_LFO_CUTOFF,
        ),
    ];
    let mut n = 0;
    for (what, select, depth, dest) in wanted {
        let lfo = &patch.lfos[usize::from(select) % patch.lfos.len()];
        if depth == 0 || lfo.shape == SIDSTATION_LFO_FLAT {
            continue;
        }
        if n == LAB_SOUND_LFOS as usize {
            losses.push(Loss::LfoDropped(what));
            continue;
        }
        let depth = i32::from(depth);
        let scaled = if dest == LAB_LFO_PITCH {
            (depth * PITCH_DEPTH_A_SEMITONE + SIDSTATION_DEPTH_MAX / 2) / SIDSTATION_DEPTH_MAX
        } else {
            depth * LAB_LFO_DEPTH_MAX as i32 / SIDSTATION_DEPTH_MAX
        }
        .clamp(1, LAB_LFO_DEPTH_MAX as i32);
        let signed = if lfo.invert { -scaled } else { scaled };
        let at = (LAB_SOUND_LFO + n as u32 * LAB_SOUND_LFO_BYTES) as usize;
        s[at] = ((u32::from(lfo.shape) + LAB_LFO_TRIANGLE) | (dest << LAB_LFO_DEST_SHIFT)) as u8;
        s[at + 1] = (u32::from(lfo.speed).min(SIDSTATION_SPEED_MAX) * (LAB_LFO_RATES - 1)
            / SIDSTATION_SPEED_MAX) as u8;
        s[at + 2] = signed as i8 as u8;
        n += 1;
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
    filter_envelope(patch, &mut s);
    let carried = table(patch, &layers, &mut s, &mut losses);
    lfos(patch, layers[0], &mut s, &mut losses);
    report(patch, &layers, carried, &mut losses);
    losses.extend(patch.unmapped.iter().copied().map(Loss::Unmapped));
    (s, losses)
}
