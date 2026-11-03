// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod bank;
pub mod lab;
pub mod model;
pub mod sidstation;

pub use model::{
    Controller, Filter, Lfo, Oscillator, Patch, Table, TableNote, TableStep, VoiceMode,
};
pub use sidstation::{parse_bank, Error};
