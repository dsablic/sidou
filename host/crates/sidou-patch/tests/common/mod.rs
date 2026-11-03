// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub const NAME_BYTES: usize = 10;
pub const PATCH_MODE: usize = 22;
pub const FILTER_ROUTING: usize = 24;
pub const FILTER_TYPE_LFO: usize = 25;
pub const FILTER_CUTOFF: usize = 26;
pub const FILTER_ENV_DEPTH: usize = 27;
pub const OSCILLATOR_BASE: [usize; 3] = [36, 57, 78];
pub const LFO_BASE: [usize; 4] = [99, 110, 121, 132];
pub const TABLES_AT: usize = 143;

pub const OSC_FLAGS: usize = 0;
pub const OSC_TRACK: usize = 1;
pub const OSC_TRANSPOSE: usize = 3;
pub const OSC_DETUNE: usize = 4;
pub const OSC_ATTACK: usize = 6;
pub const OSC_DECAY: usize = 7;
pub const OSC_SUSTAIN: usize = 8;
pub const OSC_RELEASE: usize = 9;
pub const OSC_PULSE_WIDTH: usize = 11;
pub const OSC_WAVE: usize = 15;
pub const OSC_TABLE_SPEED: usize = 20;
pub const SYNC_SPEED: usize = 34;

pub const LFO_TYPE: usize = 0;
pub const LFO_OPTIONS: usize = 1;
pub const LFO_SPEED: usize = 2;

pub const PREFIX_LIBRARIAN: [u8; 4] = [0x00, 0x45, 0x01, 0x00];
pub const PREFIX_HARDWARE: [u8; 5] = [0x00, 0x20, 0x3C, 0x01, 0x00];

pub struct Decoded(pub Vec<u8>);

impl Decoded {
    pub fn new(name: &str) -> Self {
        let mut bytes = vec![b' '; NAME_BYTES];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        bytes.resize(TABLES_AT, 0);
        bytes.extend([0xFF; 3]);
        Self(bytes)
    }

    pub fn set(mut self, at: usize, value: u8) -> Self {
        self.0[at] = value;
        self
    }

    pub fn osc(self, n: usize, field: usize, value: u8) -> Self {
        self.set(OSCILLATOR_BASE[n] + field, value)
    }

    pub fn lfo(self, n: usize, field: usize, value: u8) -> Self {
        self.set(LFO_BASE[n] + field, value)
    }

    pub fn tables(mut self, tables: &[&[u8]]) -> Self {
        self.0.truncate(TABLES_AT);
        for table in tables {
            self.0.extend_from_slice(table);
        }
        self
    }
}

fn swapped(byte: u8) -> u8 {
    byte << 4 | byte >> 4
}

pub fn dump(prefix: &[u8], declared_size: u16, decoded: &Decoded) -> Vec<u8> {
    let mut message = vec![0xF0];
    message.extend_from_slice(prefix);
    message.extend([
        0x02,
        0x00,
        (declared_size >> 7) as u8,
        (declared_size & 0x7F) as u8,
    ]);
    message.extend([0x2D; 24]);
    message.push(0x45);
    message.extend_from_slice(&decoded.0[..NAME_BYTES]);
    for &byte in &decoded.0[NAME_BYTES..] {
        let stored = swapped(byte);
        message.extend([stored >> 4, stored & 0x0F]);
    }
    message.push(0xF7);
    message
}

pub fn all_clear(prefix: &[u8]) -> Vec<u8> {
    let mut message = vec![0xF0];
    message.extend_from_slice(prefix);
    message.extend([0x01, 0x45]);
    message.extend([0x2D; 14]);
    message.push(0xF7);
    message
}

pub fn control_message(prefix: &[u8], kind: u8) -> Vec<u8> {
    let mut message = vec![0xF0];
    message.extend_from_slice(prefix);
    message.extend([kind, 0x00, 0x00]);
    message.push(0xF7);
    message
}
