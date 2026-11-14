// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::json;

use crate::import::{Model, SongFile};
use crate::{put_frames, FILE_SONG};

pub struct PatchFile {
    pub song: SongFile,
    pub quiet: Option<String>,
    pub tempo: Option<String>,
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
        tempo: None,
    })
}

pub fn bank_file(text: &str, model: Model) -> Result<PatchFile, String> {
    let (b, sng) = json::check_bank(text, model.sid())?;
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
        tempo: None,
    })
}

fn named_song(
    sng: Vec<u8>,
    quiet: Option<String>,
    tempo: Option<String>,
) -> Result<PatchFile, String> {
    let name = String::from_utf8_lossy(&sng[4..36])
        .trim_end_matches('\0')
        .to_string();
    let syx = put_frames(&name, FILE_SONG, &sng)
        .map_err(|e| format!("{name}: {e}"))?
        .concat();
    Ok(PatchFile {
        song: SongFile { name, sng, syx },
        quiet,
        tempo,
    })
}

pub fn step_file(text: &str, model: Model) -> Result<PatchFile, String> {
    let (sng, quiet) = json::check_step(text, model.sid())?;
    named_song(sng, quiet, None)
}

pub fn song_file(text: &str, model: Model) -> Result<PatchFile, String> {
    let made = json::check_song(text, model.sid())?;
    let tempo = format!(
        "asked {}, plays {}.{}",
        made.bpm,
        made.bpm_played10 / 10,
        made.bpm_played10 % 10
    );
    named_song(made.sng, None, Some(tempo))
}
