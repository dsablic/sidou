// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod bank;
pub mod lab;
pub mod mbsid;
pub mod model;
pub mod sidstation;
pub mod sounds;
mod sysex;

pub use model::{
    Controller, Filter, Lfo, Oscillator, Patch, Table, TableNote, TableStep, Unmapped, VoiceMode,
};
pub use sidstation::{parse_bank, patch_dump, Error};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    SidStation,
    MidiboxSid,
}

pub fn format_of(bytes: &[u8]) -> Format {
    if bytes.starts_with(&mbsid::HEADER) {
        Format::MidiboxSid
    } else {
        Format::SidStation
    }
}
