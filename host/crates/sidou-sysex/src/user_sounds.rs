// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::json;
use sidou_patch::user_sounds::{self, NAME};

use crate::send::{Client, GetRefusal, Port, FILE_USER_SOUNDS, NAK_IO, NAK_MALFORMED};

pub struct PatchText {
    pub slot: String,
    pub name: String,
    pub file_name: String,
    pub json: String,
}

pub fn fetch<P: Port>(client: &mut Client<P>) -> Result<Option<Vec<u8>>, String> {
    match client.get_refusing(NAME).map_err(refusal_words)? {
        None => Ok(None),
        Some((FILE_USER_SOUNDS, bytes)) => Ok(Some(bytes)),
        Some(_) => Err(format!("{NAME} on the FM-1 is not a USER sounds file")),
    }
}

fn refusal_words((reason, words): GetRefusal) -> String {
    match reason {
        Some(NAK_MALFORMED) => {
            "this FM-1's SIDOU is too old to read USER sounds: update SIDOU on the FM-1".to_string()
        }
        Some(NAK_IO) => "the FM-1 was saving a sound: try again".to_string(),
        _ => words,
    }
}

fn file_stem(name: &str) -> String {
    let mut stem = String::new();
    for c in name.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if c != '-' || !stem.ends_with('-') {
            stem.push(c);
        }
    }
    stem.trim_matches('-').to_string()
}

pub fn patch_files(file: &[u8]) -> Vec<PatchText> {
    user_sounds::read(file)
        .into_iter()
        .enumerate()
        .filter_map(|(n, held)| held.map(|h| (n, h)))
        .map(|(n, held)| {
            let slot = format!("U{:02}", n + 1);
            let stem = file_stem(&held.name);
            let file_name = if stem.is_empty() {
                format!("{slot}.json")
            } else {
                format!("{slot}-{stem}.json")
            };
            PatchText {
                json: json::write(&held.sound, &held.name),
                slot,
                name: held.name,
                file_name,
            }
        })
        .collect()
}
