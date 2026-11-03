// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::bank::bank_songs;
use sidou_patch::{format_of, mbsid, parse_bank, Format, Patch};

use crate::{put_frames, FILE_SONG};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Model {
    Mos6581,
    Csg8580,
}

impl Model {
    fn sid(self) -> u32 {
        match self {
            Self::Mos6581 => sidou_sys::SID_MODEL_6581,
            Self::Csg8580 => sidou_sys::SID_MODEL_8580,
        }
    }
}

pub struct SongFile {
    pub name: String,
    pub sng: Vec<u8>,
    pub syx: Vec<u8>,
}

pub struct BankFiles {
    pub songs: Vec<SongFile>,
    pub report: String,
}

pub fn default_name(bank: &[u8]) -> &'static str {
    match format_of(bank) {
        Format::SidStation => "SIDSTN",
        Format::MidiboxSid => "MBSID",
    }
}

fn read_bank(bank: &[u8]) -> Result<(Vec<Patch>, Vec<String>), String> {
    match format_of(bank) {
        Format::SidStation => parse_bank(bank)
            .map(|patches| (patches, Vec::new()))
            .map_err(|e| format!("not a SidStation bank: {e}")),
        Format::MidiboxSid => {
            let read =
                mbsid::parse_bank(bank).map_err(|e| format!("not a MIDIbox SID bank: {e}"))?;
            let refused: Vec<String> = read.refused.iter().map(ToString::to_string).collect();
            if read.patches.is_empty() {
                return Err(format!("no patch imports:\n{}", refused.join("\n")));
            }
            Ok((read.patches, refused))
        }
    }
}

pub fn bank_files(bank: &[u8], model: Model, base: &str) -> Result<BankFiles, String> {
    let (patches, refused) = read_bank(bank)?;
    let mut songs = Vec::new();
    let mut report = String::new();
    for line in &refused {
        report.push_str(&format!("refused {line}\n"));
    }
    for song in bank_songs(&patches, model.sid(), base) {
        let syx = put_frames(&song.name, FILE_SONG, &song.sng)
            .map_err(|e| format!("{}: {e}", song.name))?
            .concat();
        report.push_str(&format!("{} ({} patches)\n", song.name, song.patches));
        for line in &song.lines {
            report.push_str(&format!("  {line}\n"));
        }
        songs.push(SongFile {
            name: song.name,
            sng: song.sng,
            syx,
        });
    }
    if songs.is_empty() {
        return Err("no patch fits a song".to_string());
    }
    Ok(BankFiles { songs, report })
}
