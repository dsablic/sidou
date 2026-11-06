// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::{CStr, CString};

use sidou_sys::song::Song;
use sidou_sys::*;

use crate::lab::BYTES;

pub const FORMAT: &str = "sidou-patch/1";
const OUT_MAX: usize = 16 * 1024;

pub type Sound = [u8; BYTES];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    pub path: String,
    pub kind: String,
    pub names: Vec<String>,
    pub allowed: String,
}

fn text(bytes: &[u8]) -> String {
    CStr::from_bytes_until_nul(bytes)
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn defaults() -> Sound {
    let mut s = [0u8; BYTES];
    unsafe { patch_json_defaults(s.as_mut_ptr()) };
    s
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checked {
    pub sound: Sound,
    pub name: String,
    pub quiet: Option<String>,
}

pub fn check(json: &str) -> Result<Checked, String> {
    let mut p = patch_json::default();
    let r = unsafe { patch_json_read(&mut p, json.as_ptr().cast(), json.len() as u32) };
    let error = text(&p.error.iter().map(|&c| c as u8).collect::<Vec<_>>());
    if r < 0 {
        return Err(error);
    }
    Ok(Checked {
        sound: p.sound,
        name: text(&p.name.iter().map(|&c| c as u8).collect::<Vec<_>>()),
        quiet: (r == PATCH_JSON_QUIET as i32).then_some(error),
    })
}

pub fn read(json: &str) -> Result<(Sound, String), String> {
    check(json).map(|c| (c.sound, c.name))
}

pub fn write(sound: &Sound, name: &str) -> String {
    let mut out = vec![0u8; OUT_MAX];
    let c = CString::new(name).unwrap_or_default();
    let n = unsafe {
        patch_json_write(
            sound.as_ptr(),
            c.as_ptr(),
            out.as_mut_ptr().cast(),
            OUT_MAX as u32,
        )
    };
    String::from_utf8_lossy(&out[..n as usize]).into_owned()
}

pub fn fields() -> Vec<Field> {
    let mut out = vec![0u8; OUT_MAX];
    let n = unsafe { patch_json_fields(out.as_mut_ptr().cast(), OUT_MAX as u32) } as usize;
    String::from_utf8_lossy(&out[..n])
        .lines()
        .map(|line| {
            let mut col = line.split('\t');
            let mut next = || col.next().unwrap_or_default().to_string();
            let (path, kind, names, allowed) = (next(), next(), next(), next());
            Field {
                path,
                kind,
                names: names
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
                allowed,
            }
        })
        .collect()
}

pub fn song(sound: &Sound, name: &str, model: u32) -> Option<Vec<u8>> {
    let mut song = Song::device();
    let c = CString::new(name).ok()?;
    let r = unsafe { patch_json_song(song.ptr_mut(), sound.as_ptr(), c.as_ptr(), model) };
    (r == 0).then(|| song.write(true))
}

pub fn song_patch(song: &Song, instr: u32) -> Option<String> {
    let mut out = vec![0u8; OUT_MAX];
    let n = unsafe {
        patch_json_song_sound(song.ptr(), instr, out.as_mut_ptr().cast(), OUT_MAX as u32)
    } as usize;
    (n > 0).then(|| String::from_utf8_lossy(&out[..n]).into_owned())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bank {
    pub name: String,
    pub patches: Vec<(Sound, String)>,
    pub quiet: Option<String>,
}

fn c_text(chars: &[std::os::raw::c_char]) -> String {
    text(&chars.iter().map(|&c| c as u8).collect::<Vec<_>>())
}

fn bank_of(json: &str) -> Result<Box<patch_bank>, String> {
    let mut b = Box::<patch_bank>::default();
    let r = unsafe { patch_json_read_bank(&mut *b, json.as_ptr().cast(), json.len() as u32) };
    if r < 0 {
        return Err(c_text(&b.error));
    }
    Ok(b)
}

pub fn is_bank(json: &str) -> bool {
    unsafe { patch_json_is_bank(json.as_ptr().cast(), json.len() as u32) != 0 }
}

pub fn read_bank(json: &str) -> Result<Bank, String> {
    let b = bank_of(json)?;
    let quiet = c_text(&b.error);
    Ok(Bank {
        name: c_text(&b.name),
        patches: (0..b.count as usize)
            .map(|k| (b.sounds[k], c_text(&b.names[k])))
            .collect(),
        quiet: (!quiet.is_empty()).then_some(quiet),
    })
}

pub fn write_bank(name: &str, patches: &[(Sound, &str)]) -> String {
    let mut b = Box::<patch_bank>::default();
    for (d, c) in b.name.iter_mut().zip(name.bytes()) {
        *d = c as std::os::raw::c_char;
    }
    for (k, (sound, n)) in patches.iter().enumerate() {
        b.sounds[k] = *sound;
        for (d, c) in b.names[k].iter_mut().zip(n.bytes()) {
            *d = c as std::os::raw::c_char;
        }
    }
    b.count = patches.len() as u32;
    let mut out = vec![0u8; 16 * OUT_MAX];
    let n = unsafe { patch_json_write_bank(&*b, out.as_mut_ptr().cast(), out.len() as u32) };
    String::from_utf8_lossy(&out[..n as usize]).into_owned()
}

pub fn bank_song(json: &str, model: u32) -> Result<Vec<u8>, String> {
    let mut b = bank_of(json)?;
    let mut song = Song::device();
    match unsafe { patch_json_bank_song(song.ptr_mut(), &mut *b, model) } {
        0 => Ok(song.write(true)),
        r if r > 0 => Err(c_text(&b.error)),
        _ => Err("the bank's song could not be made".to_string()),
    }
}
