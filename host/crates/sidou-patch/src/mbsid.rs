// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt;

use crate::model::{Filter, Oscillator, Patch, Unmapped, VoiceMode, OSCILLATORS};
use crate::sysex::{ascii_name, messages, Framing, END, START};

pub const PATCH_BYTES: usize = 512;
pub const HEADER: [u8; 5] = [START, 0x00, 0x00, 0x7E, 0x4B];
const DEVICE: u8 = 0x00;
const COMMAND_WRITE: u8 = 0x02;
const TYPE_PATCH_BANK: u8 = 0x00;
const TYPE_PATCH_LAST: u8 = 0x0F;
const ADDRESS_BYTES: usize = 5;
const NIBBLES: usize = 2 * PATCH_BYTES;
const BODY_BYTES: usize = HEADER.len() - 1 + ADDRESS_BYTES + NIBBLES + 1;
const CHECKSUM_MASK: u8 = 0x7F;

const NAME_BYTES: usize = 16;
const ENGINE: usize = 0x10;
const ENGINE_BITS: u8 = 0x03;
const FLAGS: usize = 0x50;
const LEGATO: u8 = 0x01;
const OSC_DETUNE: usize = 0x51;
const FILTER_LEFT: usize = 0x54;
const FILTER_RIGHT: usize = 0x5A;
const FILTER_BYTES: usize = 6;
const VOICES_LEFT: usize = 0x60;
const VOICES_RIGHT: usize = 0x90;
const VOICE_BYTES: usize = 16;
const MOD_PATHS: usize = 0x100;
const MOD_PATH_BYTES: usize = 8;
const MOD_PATH_COUNT: usize = 8;
const WAVETABLES: usize = 0x16C;
const WAVETABLE_BYTES: usize = 5;
const WAVETABLE_COUNT: usize = 4;

const V_WAVEFORM: usize = 1;
const V_AD: usize = 2;
const V_SR: usize = 3;
const V_PW_L: usize = 4;
const V_PW_H: usize = 5;
const V_TRANSPOSE: usize = 8;
const V_FINETUNE: usize = 9;
const V_PITCHRANGE: usize = 10;
const V_PORTAMENTO: usize = 11;
const V_ARP_MODE: usize = 12;

const WAVE_BITS: u8 = 0x0F;
const VOICE_OFF: u8 = 0x10;
const WAVE_SYNC: u8 = 0x20;
const WAVE_RING: u8 = 0x40;
const SID_SYNC: u8 = 0x02;
const SID_RING: u8 = 0x04;
const ARP_ON: u8 = 0x01;
const PULSE_HIGH_BITS: u8 = 0x0F;
const PULSE_UNIT_SHIFT: u32 = 5;

const F_CHANNELS: usize = 0;
const F_CUTOFF_L: usize = 1;
const F_CUTOFF_H: usize = 2;
const F_RESONANCE: usize = 3;
const F_KEYTRACK: usize = 4;
const ROUTING_BITS: u8 = 0x07;
const OSC3_OFF: u8 = 0x80;
const CUTOFF_HIGH_BITS: u8 = 0x0F;
const CUTOFF_TO_MODEL_SHIFT: u32 = 5;

const MOD_OPERATOR: usize = 2;
const MOD_DEPTH: usize = 3;
const MOD_TARGETS: std::ops::Range<usize> = 4..8;
const MOD_OPERATOR_BITS: u8 = 0x0F;
const WT_ASSIGN: usize = 1;
const WT_BEGIN: usize = 2;
const WT_END: usize = 3;
const WT_POSITION_BITS: u8 = 0x7F;
const WT_MEMORY: usize = 0x180;
const WT_RELATIVE_ZERO: u8 = 0x40;

const CENTRE: i32 = 0x80;
const TRANSPOSE_ZERO: i32 = 0x40;
const STEPS_PER_SEMITONE: i32 = 64;
const PITCH_SCALE: i32 = 256;
const FINETUNE_WEIGHT: i32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Engine {
    Lead,
    Bassline,
    Drum,
    Multi,
}

impl Engine {
    fn of(byte: u8) -> Self {
        match byte & ENGINE_BITS {
            0 => Self::Lead,
            1 => Self::Bassline,
            2 => Self::Drum,
            _ => Self::Multi,
        }
    }
}

fn slot(bank: u8, number: u8) -> String {
    let letter = char::from(b'A'.wrapping_add(bank));
    format!("{letter}{:03}", u32::from(number) + 1)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    Engine(Engine),
    Checksum,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Refused {
    pub bank: u8,
    pub number: u8,
    pub name: String,
    pub reason: Reason,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let slot = slot(self.bank, self.number);
        if self.name == slot {
            write!(f, "{slot}: ")?;
        } else {
            write!(f, "{slot} {}: ", self.name)?;
        }
        match self.reason {
            Reason::Engine(engine) => {
                write!(f, "a {engine:?} engine patch, only Lead patches import")
            }
            Reason::Checksum => f.write_str("the checksum does not match, the patch is damaged"),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Bank {
    pub patches: Vec<Patch>,
    pub refused: Vec<Refused>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    NoPatches,
    NotSysex { at: usize },
    Unterminated { at: usize },
    ForeignMessage { at: usize },
    Length { index: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPatches => f.write_str("no patch dump in the file"),
            Self::NotSysex { at } => write!(f, "byte {at} is not the start of a SysEx message"),
            Self::Unterminated { at } => write!(f, "the message at byte {at} has no end"),
            Self::ForeignMessage { at } => {
                write!(f, "the message at byte {at} is not a MIDIbox SID message")
            }
            Self::Length { index } => {
                write!(f, "patch dump {index} is not {PATCH_BYTES} bytes long")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<Framing> for Error {
    fn from(framing: Framing) -> Self {
        match framing {
            Framing::NotSysex { at } => Self::NotSysex { at },
            Framing::Unterminated { at } => Self::Unterminated { at },
        }
    }
}

fn checksum(nibbles: impl Iterator<Item = u8>) -> u8 {
    nibbles
        .fold(0u8, |sum, n| sum.wrapping_add(n))
        .wrapping_neg()
        & CHECKSUM_MASK
}

pub fn patch_dump(patch: &[u8; PATCH_BYTES], bank: u8, number: u8) -> Vec<u8> {
    let nibbles: Vec<u8> = patch.iter().flat_map(|&b| [b & 0x0F, b >> 4]).collect();
    let mut m = HEADER.to_vec();
    m.extend([DEVICE, COMMAND_WRITE, TYPE_PATCH_BANK, bank, number]);
    m.extend_from_slice(&nibbles);
    m.push(checksum(nibbles.iter().copied()));
    m.push(END);
    m
}

pub fn parse_bank(bytes: &[u8]) -> Result<Bank, Error> {
    let mut bank = Bank::default();
    let mut dumps = 0;
    for message in messages(bytes) {
        let (at, body) = message?;
        let address = body
            .strip_prefix(&HEADER[1..])
            .ok_or(Error::ForeignMessage { at })?;
        let &[_device, command, kind, bank_number, number, ..] = address else {
            continue;
        };
        if command != COMMAND_WRITE || kind > TYPE_PATCH_LAST {
            continue;
        }
        let index = dumps;
        dumps += 1;
        if body.len() != BODY_BYTES {
            return Err(Error::Length { index });
        }
        let nibbles = &address[ADDRESS_BYTES..ADDRESS_BYTES + NIBBLES];
        let data: [u8; PATCH_BYTES] =
            std::array::from_fn(|i| nibbles[2 * i] & 0x0F | (nibbles[2 * i + 1] & 0x0F) << 4);
        let name = Some(ascii_name(&data[..NAME_BYTES]))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| slot(bank_number, number));
        let refuse = |reason| Refused {
            bank: bank_number,
            number,
            name: name.clone(),
            reason,
        };
        if checksum(nibbles.iter().copied()) != address[address.len() - 1] {
            bank.refused.push(refuse(Reason::Checksum));
            continue;
        }
        match Engine::of(data[ENGINE]) {
            Engine::Lead => bank.patches.push(lead_patch(&data, name)),
            engine => bank.refused.push(refuse(Reason::Engine(engine))),
        }
    }
    if dumps == 0 {
        return Err(Error::NoPatches);
    }
    Ok(bank)
}

fn voice(data: &[u8; PATCH_BYTES], n: usize) -> &[u8] {
    &data[VOICES_LEFT + n * VOICE_BYTES..][..VOICE_BYTES]
}

fn sounding(data: &[u8; PATCH_BYTES], n: usize) -> bool {
    let wave = voice(data, n)[V_WAVEFORM];
    let silenced = n == OSCILLATORS - 1 && data[FILTER_LEFT + F_CHANNELS] & OSC3_OFF != 0;
    wave & VOICE_OFF == 0 && wave & WAVE_BITS != 0 && !silenced
}

fn div_round(value: i32, by: i32) -> i32 {
    (value + by / 2).div_euclid(by)
}

fn detune_share(n: usize, detune: i32) -> i32 {
    match n {
        0 => detune >> 2,
        1 => detune,
        _ => -detune,
    }
}

fn tuning(v: &[u8], n: usize, detune: u8) -> (i8, i8) {
    let range = i32::from(v[V_PITCHRANGE]);
    let offset =
        FINETUNE_WEIGHT * (i32::from(v[V_FINETUNE]) - CENTRE) + detune_share(n, i32::from(detune));
    let steps = div_round(range * offset * STEPS_PER_SEMITONE, PITCH_SCALE);
    let total = (i32::from(v[V_TRANSPOSE]) - TRANSPOSE_ZERO) * STEPS_PER_SEMITONE + steps;
    let semis = div_round(total, STEPS_PER_SEMITONE);
    let rest = total - semis * STEPS_PER_SEMITONE;
    (
        semis.clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8,
        rest as i8,
    )
}

fn oscillator(data: &[u8; PATCH_BYTES], n: usize) -> Oscillator {
    let v = voice(data, n);
    let wave = v[V_WAVEFORM];
    let pulse = u16::from(v[V_PW_L]) | u16::from(v[V_PW_H] & PULSE_HIGH_BITS) << 8;
    let rounded = (pulse + (1 << (PULSE_UNIT_SHIFT - 1))) >> PULSE_UNIT_SHIFT;
    let (transpose, detune) = tuning(v, n, data[OSC_DETUNE]);
    Oscillator {
        control: (wave & WAVE_BITS) << 4
            | if wave & WAVE_RING != 0 { SID_RING } else { 0 }
            | if wave & WAVE_SYNC != 0 { SID_SYNC } else { 0 },
        attack: v[V_AD] >> 4,
        decay: v[V_AD] & 0x0F,
        sustain: v[V_SR] >> 4,
        release: v[V_SR] & 0x0F,
        pulse_width_start: rounded as u8,
        transpose,
        detune,
        bend_range: v[V_PITCHRANGE],
        ..Default::default()
    }
}

fn filter(data: &[u8; PATCH_BYTES]) -> Filter {
    let f = &data[FILTER_LEFT..][..FILTER_BYTES];
    let cutoff = u16::from(f[F_CUTOFF_L]) | u16::from(f[F_CUTOFF_H] & CUTOFF_HIGH_BITS) << 8;
    Filter {
        routed: std::array::from_fn(|n| f[F_CHANNELS] & ROUTING_BITS & (1 << n) != 0),
        mode: f[F_CHANNELS] >> 4 & 0x07,
        cutoff: (cutoff >> CUTOFF_TO_MODEL_SHIFT) as u8,
        resonance: f[F_RESONANCE] >> 4,
        ..Default::default()
    }
}

fn mod_path_active(path: &[u8]) -> bool {
    path[MOD_OPERATOR] & MOD_OPERATOR_BITS != 0
        && i32::from(path[MOD_DEPTH]) != CENTRE
        && path[MOD_TARGETS].iter().any(|&t| t != 0)
}

fn wavetable_active(data: &[u8; PATCH_BYTES], table: &[u8]) -> bool {
    let [begin, end] = [WT_BEGIN, WT_END].map(|f| usize::from(table[f] & WT_POSITION_BITS));
    let steps = &data[WT_MEMORY..][begin.min(end)..=begin.max(end)];
    table[WT_ASSIGN] != 0 && steps.iter().any(|&step| step != WT_RELATIVE_ZERO)
}

fn unmapped(data: &[u8; PATCH_BYTES], active: &[bool; OSCILLATORS]) -> Vec<Unmapped> {
    let mut out = Vec::new();
    for n in (0..OSCILLATORS).filter(|&n| active[n]) {
        let v = voice(data, n);
        if v[V_ARP_MODE] & ARP_ON != 0 {
            out.push(Unmapped::Arpeggio(n + 1));
        }
        if v[V_PORTAMENTO] != 0 {
            out.push(Unmapped::Portamento(n + 1));
        }
    }
    let routed = data[FILTER_LEFT + F_CHANNELS] & ROUTING_BITS != 0;
    if routed && data[FILTER_LEFT + F_KEYTRACK] != 0 {
        out.push(Unmapped::FilterKeyTracking);
    }
    let paths = data[MOD_PATHS..]
        .chunks(MOD_PATH_BYTES)
        .take(MOD_PATH_COUNT);
    out.extend(
        paths
            .enumerate()
            .filter(|(_, path)| mod_path_active(path))
            .map(|(n, _)| Unmapped::ModulationPath(n + 1)),
    );
    let tables = data[WAVETABLES..]
        .chunks(WAVETABLE_BYTES)
        .take(WAVETABLE_COUNT);
    out.extend(
        tables
            .enumerate()
            .filter(|(_, table)| wavetable_active(data, table))
            .map(|(n, _)| Unmapped::Wavetable(n + 1)),
    );
    let voices = OSCILLATORS * VOICE_BYTES;
    let right_differs = data[VOICES_LEFT..][..voices] != data[VOICES_RIGHT..][..voices]
        || data[FILTER_LEFT..][..FILTER_BYTES] != data[FILTER_RIGHT..][..FILTER_BYTES];
    if right_differs {
        out.push(Unmapped::RightSid);
    }
    out
}

fn lead_patch(data: &[u8; PATCH_BYTES], name: String) -> Patch {
    let active = std::array::from_fn(|n| sounding(data, n));
    Patch {
        name,
        controllers: Default::default(),
        mode: VoiceMode::Single,
        active,
        legato: data[FLAGS] & LEGATO != 0,
        sync_speed: 0,
        sync_hard_cut: 0,
        filter: filter(data),
        oscillators: std::array::from_fn(|n| oscillator(data, n)),
        lfos: Default::default(),
        tables: Default::default(),
        unmapped: unmapped(data, &active),
    }
}
