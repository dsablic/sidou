// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt;

use crate::model::{
    Controller, Filter, Lfo, Oscillator, Patch, Table, TableNote, TableStep, VoiceMode, OSCILLATORS,
};
use crate::sysex::{ascii_name, messages, Framing, END, START};

const PREFIX_LIBRARIAN: [u8; 4] = [0x00, 0x45, 0x01, 0x00];
const PREFIX_HARDWARE: [u8; 5] = [0x00, 0x20, 0x3C, 0x01, 0x00];
const TYPE_PATCH_DUMP: u8 = 0x02;
const HEADER_AFTER_TYPE: usize = 3 + HEADER_PADS + 1;
const NAME_BYTES: usize = 10;

const CONTROLLERS_AT: usize = 10;
const CONTROLLER_BYTES: usize = 3;
const PATCH_MODE: usize = 22;
const FILTER_ROUTING: usize = 24;
const FILTER_TYPE_LFO: usize = 25;
const FILTER_BLOCK: usize = 26;
const SYNC_SPEED: usize = 34;
const SYNC_HARD_CUT: usize = 35;
const OSCILLATOR_AT: usize = 36;
const OSCILLATOR_BYTES: usize = 21;
const LFO_AT: usize = 99;
const LFO_BYTES: usize = 11;
const TABLES_AT: usize = 143;
const TABLE_END: u8 = 0xFF;
const TABLE_LOOP: u8 = 0xFE;
const TABLE_STEPS_MAX: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    NoPatches,
    NotSysex { at: usize },
    Unterminated { at: usize },
    ForeignMessage { at: usize },
    Truncated { index: usize },
    TableOverrun { index: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPatches => f.write_str("no patch dump in the file"),
            Self::NotSysex { at } => write!(f, "byte {at} is not the start of a SysEx message"),
            Self::Unterminated { at } => write!(f, "the message at byte {at} has no end"),
            Self::ForeignMessage { at } => {
                write!(f, "the message at byte {at} is not a SidStation message")
            }
            Self::Truncated { index } => write!(f, "patch {index} is cut short"),
            Self::TableOverrun { index } => {
                write!(f, "patch {index}'s tables run past its data")
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

const HEADER_PAD: u8 = 0x2D;
const HEADER_PADS: usize = 24;

pub fn patch_dump(decoded: &[u8], declared_size: u16) -> Vec<u8> {
    let mut m = vec![START];
    m.extend_from_slice(&PREFIX_LIBRARIAN);
    m.extend([
        TYPE_PATCH_DUMP,
        0x00,
        (declared_size >> 7) as u8,
        (declared_size & 0x7F) as u8,
    ]);
    m.extend([HEADER_PAD; HEADER_PADS]);
    m.push(0x45);
    let name = decoded.len().min(NAME_BYTES);
    m.extend_from_slice(&decoded[..name]);
    for &byte in &decoded[name..] {
        let stored = byte.rotate_right(4);
        m.extend([stored >> 4, stored & 0x0F]);
    }
    m.push(END);
    m
}

pub fn parse_bank(bytes: &[u8]) -> Result<Vec<Patch>, Error> {
    let mut patches = Vec::new();
    for message in messages(bytes) {
        let (at, body) = message?;
        let after_prefix = strip_prefix(body).ok_or(Error::ForeignMessage { at })?;
        if after_prefix.first() == Some(&TYPE_PATCH_DUMP) {
            patches.push(parse_patch(&after_prefix[1..], patches.len())?);
        }
    }
    if patches.is_empty() {
        return Err(Error::NoPatches);
    }
    Ok(patches)
}

fn strip_prefix(body: &[u8]) -> Option<&[u8]> {
    body.strip_prefix(&PREFIX_LIBRARIAN[..])
        .or_else(|| body.strip_prefix(&PREFIX_HARDWARE[..]))
}

fn parse_patch(after_type: &[u8], index: usize) -> Result<Patch, Error> {
    let payload = after_type
        .get(HEADER_AFTER_TYPE..)
        .ok_or(Error::Truncated { index })?;
    let data = decode_payload(payload);
    if data.len() < TABLES_AT {
        return Err(Error::Truncated { index });
    }
    let tables = parse_tables(&data[TABLES_AT..]).ok_or(Error::TableOverrun { index })?;
    Ok(Patch {
        name: ascii_name(&data[..NAME_BYTES]),
        controllers: std::array::from_fn(|n| controller_at(&data, n)),
        mode: if data[PATCH_MODE] & 0x08 != 0 {
            VoiceMode::Poly
        } else {
            VoiceMode::Single
        },
        active: std::array::from_fn(|n| data[PATCH_MODE] & (1 << n) != 0),
        legato: data[PATCH_MODE] & 0x20 != 0,
        sync_speed: data[SYNC_SPEED],
        sync_hard_cut: data[SYNC_HARD_CUT],
        filter: filter_of(&data),
        oscillators: std::array::from_fn(|n| {
            oscillator_at(&data[OSCILLATOR_AT + n * OSCILLATOR_BYTES..])
        }),
        lfos: std::array::from_fn(|n| lfo_at(&data[LFO_AT + n * LFO_BYTES..])),
        tables,
        unmapped: Vec::new(),
    })
}

fn decode_payload(payload: &[u8]) -> Vec<u8> {
    let name = payload.len().min(NAME_BYTES);
    let mut data = payload[..name].to_vec();
    for pair in payload[name..].as_chunks::<2>().0 {
        let stored = pair[0] << 4 | pair[1] & 0x0F;
        data.push(stored.rotate_right(4));
    }
    data
}

fn controller_at(data: &[u8], n: usize) -> Controller {
    let at = CONTROLLERS_AT + n * CONTROLLER_BYTES;
    Controller {
        assignment: data[at],
        limit_down: data[at + 1],
        limit_up: data[at + 2],
    }
}

fn filter_of(data: &[u8]) -> Filter {
    let routing = data[FILTER_ROUTING];
    let kind = data[FILTER_TYPE_LFO];
    let mode = data[PATCH_MODE];
    let block = &data[FILTER_BLOCK..];
    Filter {
        envelope_invert: mode & 0x80 != 0,
        wrap: mode & 0x40 != 0,
        routed: std::array::from_fn(|n| routing & (1 << n) != 0),
        resonance: routing >> 4,
        mode: kind & 0x07,
        lfo: kind >> 4 & 0x03,
        cutoff: block[0],
        envelope_depth: block[1],
        attack: block[2],
        decay: block[3],
        sustain: block[4],
        release: block[5],
        lfo_depth: block[6],
        lfo_wheel_depth: block[7],
    }
}

fn oscillator_at(b: &[u8]) -> Oscillator {
    Oscillator {
        restart_pulse_width: b[0] & 0x01 != 0,
        gate_only: b[0] & 0x02 != 0,
        track: b[1],
        arp_speed: b[2],
        transpose: b[3] as i8,
        detune: b[4] as i8,
        bend_range: b[5],
        attack: b[6],
        decay: b[7],
        sustain: b[8],
        release: b[9],
        delay: b[10],
        pulse_width_start: b[11],
        pulse_width_add: b[12],
        pulse_width_lfo: b[13],
        pulse_width_lfo_depth: b[14],
        control: b[15],
        portamento: b[16],
        vibrato_lfo: b[17],
        vibrato_depth: b[18],
        vibrato_wheel: b[19],
        table_speed: b[20],
    }
}

fn lfo_at(b: &[u8]) -> Lfo {
    Lfo {
        shape: b[0] & 0x07,
        control_source: b[0] >> 4,
        sync_to_note_on: b[1] & 0x01 != 0,
        invert: b[1] & 0x02 != 0,
        positive_only: b[1] & 0x04 != 0,
        sync_to_note_off: b[1] & 0x08 != 0,
        control_destination: b[1] >> 4 & 0x07,
        speed: b[2],
        sample_hold: b[3],
        depth: b[4],
        added_lfo: b[5],
        lace_speed: b[6],
        lace_with: b[7],
        added_depth: b[8],
        control_amount: b[9],
        fade_in: b[10],
    }
}

fn parse_tables(mut rest: &[u8]) -> Option<[Table; OSCILLATORS]> {
    let mut tables: [Table; OSCILLATORS] = Default::default();
    for table in &mut tables {
        if rest.is_empty() {
            break;
        }
        let mut steps = Vec::new();
        while let Some((&control, tail)) = rest.split_first() {
            match control {
                TABLE_END => {
                    rest = tail;
                    break;
                }
                TABLE_LOOP => {
                    let (&to, tail) = tail.split_first()?;
                    table.loop_to = Some(to);
                    rest = tail;
                    break;
                }
                _ => {
                    let (&note, tail) = tail.split_first()?;
                    if steps.len() == TABLE_STEPS_MAX {
                        return None;
                    }
                    steps.push(TableStep {
                        control,
                        note: note_of(note),
                    });
                    rest = tail;
                }
            }
        }
        table.steps = steps;
    }
    Some(tables)
}

fn note_of(byte: u8) -> TableNote {
    match byte {
        0x00..=0x7F => TableNote::Absolute(byte),
        0x80..=0xBF => TableNote::Down(byte & 0x3F),
        _ => TableNote::Up(byte & 0x3F),
    }
}
