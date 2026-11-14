// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::json::Sound;
use crate::lab::BYTES;

pub const NAME: &str = "USER SOUNDS";
pub const SLOTS: usize = USER_SOUNDS as usize;
const NAME_BYTES: usize = LAB_SOUND_NAME as usize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserSound {
    pub name: String,
    pub sound: Sound,
}

pub fn read_slot(file: &[u8], slot: usize) -> Option<UserSound> {
    let mut name = [0u8; NAME_BYTES + 1];
    let mut sound = [0u8; BYTES];
    let used = unsafe {
        user_sounds_file_read(
            file.as_ptr(),
            file.len() as u32,
            slot as u32,
            name.as_mut_ptr().cast(),
            sound.as_mut_ptr(),
        )
    };
    (used != 0).then(|| UserSound {
        name: String::from_utf8_lossy(
            &name[..name.iter().position(|&c| c == 0).unwrap_or(NAME_BYTES)],
        )
        .into_owned(),
        sound,
    })
}

pub fn read(file: &[u8]) -> Vec<Option<UserSound>> {
    (0..SLOTS).map(|slot| read_slot(file, slot)).collect()
}
