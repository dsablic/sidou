// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::json;

use crate::import::{Model, SongFile};
use crate::{put_frames, FILE_SONG};

pub struct PatchFile {
    pub song: SongFile,
    pub quiet: Option<String>,
}

pub fn patch_file(text: &str, model: Model) -> Result<PatchFile, String> {
    let c = json::check(text)?;
    let sng = json::song(&c.sound, &c.name, model.sid()).ok_or("the song could not be made")?;
    let syx = put_frames(&c.name, FILE_SONG, &sng)
        .map_err(|e| format!("{}: {e}", c.name))?
        .concat();
    Ok(PatchFile {
        song: SongFile {
            name: c.name,
            sng,
            syx,
        },
        quiet: c.quiet,
    })
}

pub fn bank_file(text: &str, model: Model) -> Result<PatchFile, String> {
    let b = json::read_bank(text)?;
    let sng = json::bank_song(text, model.sid())?;
    let syx = put_frames(&b.name, FILE_SONG, &sng)
        .map_err(|e| format!("{}: {e}", b.name))?
        .concat();
    Ok(PatchFile {
        song: SongFile {
            name: b.name,
            sng,
            syx,
        },
        quiet: b.quiet,
    })
}
