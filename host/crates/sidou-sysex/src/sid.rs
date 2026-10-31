// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;
use std::fmt;

use sidou_sys::{psid_header, psid_parse};

use crate::{Error, FILE_SID, MAX_FILE_LEN};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    NotSid,
    Rsid,
    Header,
    Mus,
    TooLong,
    Load,
    TooBig,
}

impl Refusal {
    fn of(code: u32) -> Self {
        match code {
            sidou_sys::PSID_E_RSID => Self::Rsid,
            sidou_sys::PSID_E_HEADER => Self::Header,
            sidou_sys::PSID_E_MUS => Self::Mus,
            sidou_sys::PSID_E_TOO_LONG => Self::TooLong,
            sidou_sys::PSID_E_LOAD => Self::Load,
            _ => Self::NotSid,
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotSid => "BROKEN .SID: not a PSID file",
            Self::Rsid => {
                "RSID: NOT YET: it needs a real C64's interrupts, which SIDOU does not have"
            }
            Self::Header => "BROKEN .SID: its header does not match the file",
            Self::Mus => "MUS: NOT SUPPORTED: Compute!'s Sidplayer data, not a tune",
            Self::TooLong => "TOO LONG: no data, or data past $FFFF",
            Self::Load => "BROKEN .SID: its data covers $0000-$0001",
            Self::TooBig => "TOO BIG: over the store's 65,536 bytes a file",
        })
    }
}

impl std::error::Error for Refusal {}

pub fn check(file: &[u8]) -> Result<psid_header, Refusal> {
    if file.len() > MAX_FILE_LEN {
        return Err(Refusal::TooBig);
    }
    let mut h = psid_header::default();
    // SAFETY: psid_parse reads file[..len] only and writes only h.
    let code = unsafe { psid_parse(file.as_ptr(), file.len() as u32, &mut h) };
    if code != sidou_sys::PSID_OK as i32 {
        return Err(Refusal::of(code as u32));
    }
    Ok(h)
}

fn text(field: &[core::ffi::c_char]) -> String {
    field
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect()
}

pub fn title(h: &psid_header) -> String {
    text(&h.name)
}

pub fn author(h: &psid_header) -> String {
    text(&h.author)
}

pub fn warnings(h: &psid_header) -> Vec<&'static str> {
    [
        (sidou_sys::PSID_W_PLAYSID, "PlaySID's samples are silent"),
        (
            sidou_sys::PSID_W_SID2,
            "an invalid second SID address: one chip",
        ),
        (sidou_sys::PSID_W_SID3, "a third SID is dropped"),
        (
            sidou_sys::PSID_W_MORE_SIDS,
            "the chips past the third are dropped",
        ),
        (
            sidou_sys::PSID_W_FM_OPL,
            "the FM OPL chip it asks for is silent",
        ),
    ]
    .into_iter()
    .filter(|(bit, _)| u32::from(h.warn) & bit != 0)
    .map(|(_, what)| what)
    .collect()
}

const STORE_NAME_MAX: usize = 16;
const RESERVED_STORE_NAMES: [&str; 2] = ["MIDI INSTRUMENT", "MIDI SONG"];

fn clean(raw: &str, max: usize) -> String {
    let upper: String = raw
        .chars()
        .map(|c| c.to_ascii_uppercase())
        .map(|c| if (' '..='_').contains(&c) { c } else { '?' })
        .collect();
    let cut: String = upper.trim().chars().take(max).collect();
    cut.trim_end().to_string()
}

pub fn store_name(title: &str, fallback: &str, taken: &mut HashSet<String>) -> String {
    let mut base = clean(title, STORE_NAME_MAX);
    if base.is_empty() || base == "<?>" {
        base = clean(fallback, STORE_NAME_MAX);
    }
    if base.is_empty() {
        base = "TUNE".to_string();
    }
    let free = |n: &String, taken: &HashSet<String>| {
        !taken.contains(n) && !RESERVED_STORE_NAMES.contains(&n.as_str())
    };
    let mut name = base.clone();
    let mut k = 2;
    while !free(&name, taken) {
        let suffix = format!("-{k}");
        name = clean(&base, STORE_NAME_MAX - suffix.len()) + &suffix;
        k += 1;
    }
    taken.insert(name.clone());
    name
}

pub fn syx(name: &str, file: &[u8]) -> Result<Vec<u8>, Error> {
    Ok(crate::put_frames(name, FILE_SID, file)?.concat())
}
