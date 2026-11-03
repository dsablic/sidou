// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::bank::bank_songs;
use sidou_patch::parse_bank;

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

pub fn bank_files(bank: &[u8], model: Model, base: &str) -> Result<BankFiles, String> {
    let patches = parse_bank(bank).map_err(|e| format!("not a SidStation bank: {e}"))?;
    let mut songs = Vec::new();
    let mut report = String::new();
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
