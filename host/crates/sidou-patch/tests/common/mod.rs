// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(dead_code, reason = "each test binary uses its own subset")]

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

pub fn dump(prefix: &[u8], declared_size: u16, decoded: &Decoded) -> Vec<u8> {
    let mut m = sidou_patch::patch_dump(&decoded.0, declared_size);
    if prefix != PREFIX_LIBRARIAN {
        m.splice(1..1 + PREFIX_LIBRARIAN.len(), prefix.iter().copied());
    }
    m
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

pub const MB_ENGINE: usize = 0x10;
pub const MB_FLAGS: usize = 0x50;
pub const MB_DETUNE: usize = 0x51;
pub const MB_FILTER_LEFT: usize = 0x54;
pub const MB_FILTER_RIGHT: usize = 0x5A;
pub const MB_VOICES_LEFT: usize = 0x60;
pub const MB_VOICES_RIGHT: usize = 0x90;
pub const MB_VOICE_BYTES: usize = 16;
pub const MB_MOD_PATHS: usize = 0x100;
pub const MB_MOD_PATH_BYTES: usize = 8;
pub const MB_WAVETABLES: usize = 0x16C;
pub const MB_WAVETABLE_BYTES: usize = 5;

pub const MB_V_WAVEFORM: usize = 1;
pub const MB_V_AD: usize = 2;
pub const MB_V_SR: usize = 3;
pub const MB_V_PW_L: usize = 4;
pub const MB_V_PW_H: usize = 5;
pub const MB_V_TRANSPOSE: usize = 8;
pub const MB_V_FINETUNE: usize = 9;
pub const MB_V_PITCHRANGE: usize = 10;
pub const MB_V_PORTAMENTO: usize = 11;
pub const MB_V_ARP_MODE: usize = 12;

pub const MB_F_CHANNELS: usize = 0;
pub const MB_F_CUTOFF_L: usize = 1;
pub const MB_F_CUTOFF_H: usize = 2;
pub const MB_F_RESONANCE: usize = 3;
pub const MB_F_KEYTRACK: usize = 4;

pub const MB_MOD_OPERATOR: usize = 2;
pub const MB_MOD_DEPTH: usize = 3;
pub const MB_MOD_TARGET_LEFT: usize = 4;
pub const MB_WT_ASSIGN: usize = 1;
pub const MB_WT_BEGIN: usize = 2;
pub const MB_WT_END: usize = 3;
pub const MB_WT_MEMORY: usize = 0x180;
pub const MB_WT_RELATIVE_ZERO: u8 = 0x40;

pub const MB_CENTRE: u8 = 0x80;
pub const MB_TRANSPOSE_ZERO: u8 = 0x40;
pub const MB_PITCHRANGE: u8 = 2;
pub const MB_SAW: u8 = 0x02;
pub const MB_PULSE: u8 = 0x04;

pub struct MbPatch(pub [u8; sidou_patch::mbsid::PATCH_BYTES]);

impl MbPatch {
    pub fn new(name: &str) -> Self {
        let mut bytes = [0u8; sidou_patch::mbsid::PATCH_BYTES];
        bytes[..16].fill(b' ');
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        bytes[MB_WT_MEMORY..].fill(MB_WT_RELATIVE_ZERO);
        let patch = (0..3).fold(Self(bytes), |p, n| {
            p.voice(n, MB_V_WAVEFORM, MB_SAW)
                .voice(n, MB_V_TRANSPOSE, MB_TRANSPOSE_ZERO)
                .voice(n, MB_V_FINETUNE, MB_CENTRE)
                .voice(n, MB_V_PITCHRANGE, MB_PITCHRANGE)
        });
        (0..8).fold(patch, |p, n| p.mod_path(n, MB_MOD_DEPTH, MB_CENTRE))
    }

    pub fn set(mut self, at: usize, value: u8) -> Self {
        self.0[at] = value;
        self
    }

    pub fn voice(self, n: usize, field: usize, value: u8) -> Self {
        let at = n * MB_VOICE_BYTES + field;
        self.set(MB_VOICES_LEFT + at, value)
            .set(MB_VOICES_RIGHT + at, value)
    }

    pub fn filter(self, field: usize, value: u8) -> Self {
        self.set(MB_FILTER_LEFT + field, value)
            .set(MB_FILTER_RIGHT + field, value)
    }

    pub fn mod_path(self, n: usize, field: usize, value: u8) -> Self {
        self.set(MB_MOD_PATHS + n * MB_MOD_PATH_BYTES + field, value)
    }

    pub fn wavetable(self, n: usize, field: usize, value: u8) -> Self {
        self.set(MB_WAVETABLES + n * MB_WAVETABLE_BYTES + field, value)
    }

    pub fn dump(&self) -> Vec<u8> {
        sidou_patch::mbsid::patch_dump(&self.0, 0, 0)
    }
}

pub const ACID: &str = r#"{"format": "sidou-step/1", "name": "acid line", "role": "acid", "pages": 1,
  "instrument": {"name": "303", "wave": "saw", "envelope": {"attack": 0, "decay": 8, "sustain": 8, "release": 8},
    "filter": {"cutoff_hz": 2077, "resonance": 12, "voices": [1], "envelope": {"depth": 40, "decay": 6}}, "vibrato": {"rate_hz": 0}},
  "steps": [{"step": 1, "note": "C2", "accent": true}, {"step": 3, "note": "C2", "slide": true},
    {"step": 4, "note": "D#2"}, {"step": 7, "note": "C3", "accent": true}, {"step": 9, "note": "G2", "chance": 75},
    {"step": 11, "note": "A#2", "slide": true}, {"step": 12, "note": "C3"}, {"step": 15, "note": "F2"}]}"#;
pub const BEAT: &str = r#"{"format": "sidou-step/1", "name": "BEAT", "voice": 3, "role": "drums", "pages": 2, "instrument": "kit",
  "steps": [{"step": 1, "drum": "kick"}, {"step": 5, "drum": "snare"}, {"step": 7, "drum": "closed_hat", "chance": 50},
    {"step": 9, "drum": "kick"}, {"step": 13, "drum": "snare"}, {"step": 17, "drum": "kick"}, {"step": 21, "drum": "clap"},
    {"step": 25, "drum": "kick"}, {"step": 29, "drum": "digi_snare"}, {"step": 32, "drum": "open_hat"}]}"#;
pub const TUNE: &str = r#"{"format": "sidou-step/1", "voice": 2, "gate": "legato", "instrument": "pulse",
  "steps": [{"step": 1, "note": "E4"}, {"step": 5, "note": "G4"}, {"step": 9, "note": "A4"}, {"step": 13, "note": "B4"}]}"#;

pub const STEPS: [(&str, &str); 3] = [("acid-line", ACID), ("beat", BEAT), ("tune", TUNE)];
