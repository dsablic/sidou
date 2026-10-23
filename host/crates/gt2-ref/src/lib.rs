// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The reference for SIDOU's song player (M2c Task 3; decision record 0007): GoatTracker
//! 2.77's own packer (`gt2reloc`'s path, built from `third_party/goattracker2` with SIDOU's
//! shim) and its packed player run in a host-only 6502 ([`cpu`]), frame by frame
//! ([`trace`]); and a generator of valid random songs for the player's register match
//! ([`gen`]). Test-only: nothing here reaches the firmware.
//!
//! GoatTracker's code keeps its song in globals, so every call into it takes one lock.

use std::os::raw::{c_int, c_void};
use std::path::PathBuf;
use std::sync::Mutex;

pub mod cpu;
pub mod gen;
pub mod trace;

/// gt2reloc's options (readme 4.4), as GoatTracker's globals take them.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// `-A`: the hard restart ADSR, `$0F00`. `$F000` and above select `altplayer.s`.
    pub hr_adsr: u32,
    /// `-S`: the speed multiplier (0 is 25 Hz: one call every 40 ms, so a traced frame is
    /// then 40 ms), 1.
    pub multiplier: u32,
    /// `-N`: NTSC. In a packed `.prg` it changes nothing (only a `.sid`'s header and speed
    /// code); the player's frequency table is the same.
    pub ntsc: u32,
    /// `-F`: a custom SID clock (unused by the packer).
    pub custom_clock: u32,
    /// `-G`: the pitch of A-4 in Hz; 0 is the built-in frequency table.
    pub a4_hz: f32,
    /// `-V`: fine vibrato (only GTS2 conversion uses it, before the options are read).
    pub finevibrato: u32,
    /// `-O`: pulse optimisation (skipping), 1.
    pub optimize_pulse: u32,
    /// `-R`: realtime-effect optimisation (skipping), 1.
    pub optimize_realtime: u32,
    /// GoatTracker's `PLAYER_*` bits ([`PLAYER_BUFFERED`] and the others).
    pub player_version: u32,
    /// `-L`: the SID's address, `$D400`.
    pub sid_address: u32,
    /// `-W`: the player's address, `$1000`.
    pub player_address: u32,
    /// `-Z`: the zero page the player uses, `$FC`.
    pub zeropage: u32,
    /// 0 6581, 1 8580 (the `.sid` header only).
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

/// gt2ref.c's code for a song `loadsong` could not load.
const LOAD_FAILED: c_int = -2;
const LOAD_MESSAGE: &str = "loadsong: not a song it reads (an unknown ident, or a short file)";

fn take(p: *mut u8, len: usize) -> Vec<u8> {
    let v = unsafe { std::slice::from_raw_parts(p, len) }.to_vec();
    unsafe { gt2ref_free(p.cast()) };
    v
}

/// A packed song: the `.prg`'s bytes after its load address.
#[derive(Clone, Debug, PartialEq)]
pub struct Packed {
    pub load: u16,
    pub code: Vec<u8>,
}

impl Packed {
    /// `JSR load` with A the subtune: init (readme 5).
    pub fn init(&self) -> u16 {
        self.load
    }
    /// `JSR load+3`: one call of the play routine.
    pub fn play(&self) -> u16 {
        self.load + 3
    }
}

/// The song packed as `gt2reloc song.sng out.prg` with these options packs it.
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

/// GoatTracker's `loadsong` then `savesong`: the song as GoatTracker writes it back.
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

/// SIDOU's assembler (`shim/gt2ref_asm.c`) on a source of its own: the bytes from the first
/// statement on (no load address). Not strict, it is the dialect GoatTracker's player is
/// written in, both of its quirks included (`ldy abs,y` assembles as `$BC`, LDY abs,X, and
/// BVS as `$B0`). Strict, for the 6502 programs SIDOU's tests write (M3's PSID fixtures): the
/// quirks are off (`ldy abs,x` is `$BC`, `ldy abs,y` an error, BVS `$70`), and BIT, BRK, CLD,
/// CLV, RTI and SED are there too; illegal opcodes go in through `.BYTE`. An error is printed
/// to stderr with its line.
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

/// `third_party/goattracker2/examples`, the songs GoatTracker 2.77 ships.
pub fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../third_party/goattracker2/examples")
}

/// Every example song, by file name, in name order.
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

/// The subtune count a song's header gives.
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
