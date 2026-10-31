// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::raw::{c_int, c_void};
use std::path::PathBuf;
use std::sync::Mutex;

pub mod cpu;
pub mod gen;
pub mod trace;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub hr_adsr: u32,
    pub multiplier: u32,
    pub ntsc: u32,
    pub custom_clock: u32,
    pub a4_hz: f32,
    pub finevibrato: u32,
    pub optimize_pulse: u32,
    pub optimize_realtime: u32,
    pub player_version: u32,
    pub sid_address: u32,
    pub player_address: u32,
    pub zeropage: u32,
    pub sid_model: u32,
}

pub const PLAYER_BUFFERED: u32 = 8;
pub const PLAYER_SOUNDEFFECTS: u32 = 16;
pub const PLAYER_VOLUME: u32 = 32;
pub const PLAYER_AUTHORINFO: u32 = 64;
pub const PLAYER_ZPGHOSTREGS: u32 = 128;
pub const PLAYER_NOOPTIMIZATION: u32 = 256;
pub const PLAYER_FULLBUFFERED: u32 = 512;

unsafe extern "C" {
    fn gt2ref_default_options(o: *mut Options);
    fn gt2ref_pack(
        sng: *const u8,
        len: usize,
        o: *const Options,
        prg: *mut *mut u8,
        prg_len: *mut usize,
    ) -> c_int;
    fn gt2ref_canonical(
        sng: *const u8,
        len: usize,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> c_int;
    fn gt2ref_free(p: *mut c_void);
    fn gt2ref_assemble(
        src: *const u8,
        len: usize,
        strict: c_int,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> c_int;
}

impl Default for Options {
    fn default() -> Options {
        let mut o = Options {
            hr_adsr: 0,
            multiplier: 0,
            ntsc: 0,
            custom_clock: 0,
            a4_hz: 0.0,
            finevibrato: 0,
            optimize_pulse: 0,
            optimize_realtime: 0,
            player_version: 0,
            sid_address: 0,
            player_address: 0,
            zeropage: 0,
            sid_model: 0,
        };
        unsafe { gt2ref_default_options(&mut o) };
        o
    }
}

static GOATTRACKER: Mutex<()> = Mutex::new(());

const LOAD_FAILED: c_int = -2;
const LOAD_MESSAGE: &str = "loadsong: not a song it reads (an unknown ident, or a short file)";

fn take(p: *mut u8, len: usize) -> Vec<u8> {
    let v = unsafe { std::slice::from_raw_parts(p, len) }.to_vec();
    unsafe { gt2ref_free(p.cast()) };
    v
}

#[derive(Clone, Debug, PartialEq)]
pub struct Packed {
    pub load: u16,
    pub code: Vec<u8>,
}

impl Packed {
    pub fn init(&self) -> u16 {
        self.load
    }
    pub fn play(&self) -> u16 {
        self.load + 3
    }
}

pub fn pack(sng: &[u8], o: &Options) -> Result<Packed, String> {
    let _g = GOATTRACKER.lock().unwrap_or_else(|e| e.into_inner());
    let mut p = std::ptr::null_mut();
    let mut len = 0usize;
    let r = unsafe { gt2ref_pack(sng.as_ptr(), sng.len(), o, &mut p, &mut len) };
    if r == LOAD_FAILED {
        return Err(LOAD_MESSAGE.into());
    }
    if r != 0 || p.is_null() {
        return Err("gt2reloc wrote no file".into());
    }
    let prg = take(p, len);
    if prg.len() < 3 {
        return Err("gt2reloc wrote an empty file".into());
    }
    Ok(Packed {
        load: u16::from_le_bytes([prg[0], prg[1]]),
        code: prg[2..].to_vec(),
    })
}

pub fn canonical(sng: &[u8]) -> Result<Vec<u8>, String> {
    let _g = GOATTRACKER.lock().unwrap_or_else(|e| e.into_inner());
    let mut p = std::ptr::null_mut();
    let mut len = 0usize;
    let r = unsafe { gt2ref_canonical(sng.as_ptr(), sng.len(), &mut p, &mut len) };
    if r == LOAD_FAILED {
        return Err(LOAD_MESSAGE.into());
    }
    if r != 0 || p.is_null() {
        return Err("savesong wrote no file".into());
    }
    Ok(take(p, len))
}

pub fn assemble(src: &str, strict: bool) -> Result<Vec<u8>, String> {
    let mut p = std::ptr::null_mut();
    let mut len = 0usize;
    let r = unsafe {
        gt2ref_assemble(
            src.as_ptr(),
            src.len(),
            c_int::from(strict),
            &mut p,
            &mut len,
        )
    };
    if r != 0 {
        if !p.is_null() {
            unsafe { gt2ref_free(p.cast()) };
        }
        return Err("assembly failed (the error is on stderr)".into());
    }
    if p.is_null() {
        return Ok(Vec::new());
    }
    Ok(take(p, len))
}

pub fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../third_party/goattracker2/examples")
}

pub fn examples() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(examples_dir())
        .expect("third_party/goattracker2/examples")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sng"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&p).unwrap())
        })
        .collect();
    v.sort();
    v
}

pub fn subtunes(sng: &[u8]) -> u32 {
    sng.get(100).copied().unwrap_or(0) as u32
}

#[cfg(test)]
mod tests {
    use super::assemble;

    #[test]
    fn the_dialect_keeps_goattrackers_two_quirks() {
        let src = ".ORG ($1000)\nldy $2000,y\nbvs l\nl: rts\n";
        assert_eq!(
            assemble(src, false).unwrap(),
            [0xbc, 0x00, 0x20, 0xb0, 0x00, 0x60]
        );
        assert!(assemble(".ORG ($1000)\nldy $2000,x\n", false).is_err());
        assert!(assemble(".ORG ($1000)\nrti\n", false).is_err());
    }

    #[test]
    fn strict_mode_assembles_what_is_written() {
        let src = ".ORG ($1000)\nldy $2000,x\nbvs l\nl: rti\nbit <$fb\nbit $d011\n\
                   brk\ncld\nclv\nsed\n.BYTE ($02)\n";
        assert_eq!(
            assemble(src, true).unwrap(),
            [
                0xbc, 0x00, 0x20, 0x70, 0x00, 0x40, 0x24, 0xfb, 0x2c, 0x11, 0xd0, 0x00, 0xd8, 0xb8,
                0xf8, 0x02
            ]
        );
        assert!(assemble(".ORG ($1000)\nldy $2000,y\n", true).is_err());
        assert!(assemble(".ORG ($1000)\nlda undefined\n", true).is_err());
    }
}
