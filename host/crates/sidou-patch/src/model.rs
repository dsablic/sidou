// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub const OSCILLATORS: usize = 3;
pub const LFOS: usize = 4;
pub const CONTROLLERS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceMode {
    Single,
    Poly,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Controller {
    pub assignment: u8,
    pub limit_down: u8,
    pub limit_up: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Oscillator {
    pub restart_pulse_width: bool,
    pub gate_only: bool,
    pub track: u8,
    pub arp_speed: u8,
    pub transpose: i8,
    pub detune: i8,
    pub bend_range: u8,
    pub attack: u8,
    pub decay: u8,
    pub sustain: u8,
    pub release: u8,
    pub delay: u8,
    pub pulse_width_start: u8,
    pub pulse_width_add: u8,
    pub pulse_width_lfo: u8,
    pub pulse_width_lfo_depth: u8,
    pub control: u8,
    pub portamento: u8,
    pub vibrato_lfo: u8,
    pub vibrato_depth: u8,
    pub vibrato_wheel: u8,
    pub table_speed: u8,
}

impl Oscillator {
    pub fn waveform(&self) -> u8 {
        self.control >> 4
    }

    pub fn ring(&self) -> bool {
        self.control & 0x04 != 0
    }

    pub fn sync(&self) -> bool {
        self.control & 0x02 != 0
    }

    pub fn pulse_width(&self) -> u16 {
        u16::from(self.pulse_width_start) * 32
    }

    pub fn detune_cents(&self) -> f64 {
        f64::from(self.detune) * 100.0 / 64.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Filter {
    pub envelope_invert: bool,
    pub wrap: bool,
    pub routed: [bool; OSCILLATORS],
    pub resonance: u8,
    pub mode: u8,
    pub lfo: u8,
    pub cutoff: u8,
    pub envelope_depth: u8,
    pub attack: u8,
    pub decay: u8,
    pub sustain: u8,
    pub release: u8,
    pub lfo_depth: u8,
    pub lfo_wheel_depth: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Lfo {
    pub shape: u8,
    pub control_source: u8,
    pub sync_to_note_on: bool,
    pub invert: bool,
    pub positive_only: bool,
    pub sync_to_note_off: bool,
    pub control_destination: u8,
    pub speed: u8,
    pub sample_hold: u8,
    pub depth: u8,
    pub added_lfo: u8,
    pub lace_speed: u8,
    pub lace_with: u8,
    pub added_depth: u8,
    pub control_amount: u8,
    pub fade_in: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableNote {
    Absolute(u8),
    Up(u8),
    Down(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableStep {
    pub control: u8,
    pub note: TableNote,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Table {
    pub steps: Vec<TableStep>,
    pub loop_to: Option<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Patch {
    pub name: String,
    pub controllers: [Controller; CONTROLLERS],
    pub mode: VoiceMode,
    pub active: [bool; OSCILLATORS],
    pub legato: bool,
    pub sync_speed: u8,
    pub sync_hard_cut: u8,
    pub filter: Filter,
    pub oscillators: [Oscillator; OSCILLATORS],
    pub lfos: [Lfo; LFOS],
    pub tables: [Table; OSCILLATORS],
}
