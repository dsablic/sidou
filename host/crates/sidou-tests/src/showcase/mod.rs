// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's showcase songs: four original songs written for SIDOU's demo videos, tutorials
//! and manual, each as data in its own file here (the song's source), written out as a
//! GoatTracker 2 `.sng` with a SIDOU trailer (`assets/songs/<name>.sng`, which GoatTracker
//! opens; the trailer's `CHPA` names the chip the song is written for). They are not
//! compiled into the firmware: they are files for the store, the demos and the starter pack.
//! `tests/showcase.rs` keeps the files in step with this source (SIDOU_SONG_UPDATE=1
//! rewrites them), plays each on GoatTracker's own packed player against SIDOU's, and
//! renders each one (SIDOU_SHOWCASE_WAV=<dir> writes the renders as WAV files).
//!
//! Every sound is GoatTracker's own, so each song plays the same in GoatTracker: the
//! row-by-row filter moves are its `C` command and filter table rather than SIDOU's parameter
//! locks, and a song plays on chip A only (chip B's tracks are M5's). The trailer adds only
//! the chip each song is written for.
//!
//! The songs are written in a small text notation ([`Kit`]): a pattern is a list of rows
//! separated by spaces (bar lines `|` are ignored), each row one of
//! - `.` a rest, `.N` N rests; `^` key off, `+` key on;
//! - a note, `C-4`, `C#4` or `Eb4` (GoatTracker's C-0 to G#7);
//! - an alias the song defines (a drum, say), standing for a row;
//!
//! then optionally `/instrument` (by name) and `@X` with the command's digit and its data,
//! two hex digits (`@F05`) or `=` and a speedtable entry's name (`@3=slide`). An orderlist
//! is a list of pattern names, `+N` / `-N` (a transpose), `xN` (repeat the next pattern N
//! times) and `|` (the restart position).

use std::collections::HashMap;
use std::path::PathBuf;

use sidou_sys::*;

use crate::font::repo_root;
use crate::player::{default_options, Player};
use crate::song::{SngSpec, Song, TAG_CHPA};

pub mod barbarian;
pub mod chill;
pub mod game;
pub mod outrun;

pub const AUTHOR: &str = "SIDOU contributors";
pub const COPYRIGHT: &str = "2026 GPL-3.0-or-later";

/// A showcase song: its file name, title, the chip it is written for, and its source.
pub struct Showcase {
    /// The file's name in assets/songs, without `.sng`.
    pub name: &'static str,
    pub title: &'static str,
    pub model: sid_model,
    pub kit: fn() -> Kit,
}

pub fn all() -> [Showcase; 4] {
    [outrun::SONG, game::SONG, barbarian::SONG, chill::SONG]
}

/// The four tables' indexes in [`Kit::tables`].
pub const WAVE: usize = 0;
pub const PULSE: usize = 1;
pub const FILTER: usize = 2;
pub const SPEED: usize = 3;

/// A table program's jump (left `$FF`): `jump(k)` jumps to the program's own row k
/// (1-based), `jump(0)` stops. [`Kit::table`] makes it absolute.
pub const fn jump(k: u8) -> (u8, u8) {
    (0xff, k)
}

/// A wavetable absolute note (GoatTracker's numbering, C-0 is 0).
pub const fn abs(note: u8) -> u8 {
    0x80 + note
}

/// A wavetable relative note down by `k` semitones.
pub const fn down(k: u8) -> u8 {
    0x80 - k
}

/// A song being written: its instruments, tables and patterns by name.
#[derive(Default)]
pub struct Kit {
    pub title: String,
    instr: Vec<[u8; 25]>,
    instr_names: HashMap<String, u8>,
    pub tables: [Vec<(u8, u8)>; 4],
    table_names: [HashMap<String, u8>; 4],
    patterns: Vec<Vec<[u8; 4]>>,
    pattern_names: HashMap<String, u8>,
    aliases: HashMap<String, String>,
    orders: [(Vec<u8>, u8); 3],
}

/// A note's name to GoatTracker's number (C-0 is 0).
pub fn note(name: &str) -> Option<u8> {
    let b = name.as_bytes();
    if b.len() != 3 {
        return None;
    }
    let base = match b[0] {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return None,
    };
    let semi: i32 = base
        + match b[1] {
            b'-' => 0,
            b'#' => 1,
            b'b' => -1,
            _ => return None,
        };
    let octave = (b[2] as char).to_digit(10)? as i32;
    let n = octave * 12 + semi;
    (0..=0x5c).contains(&n).then_some(n as u8)
}

impl Kit {
    pub fn new(title: &str) -> Kit {
        Kit {
            title: title.to_string(),
            ..Kit::default()
        }
    }

    /// A table program; its jumps are its own rows (see [`jump`]). Its first row's index.
    pub fn table(&mut self, table: usize, name: &str, rows: &[(u8, u8)]) -> u8 {
        let start = self.tables[table].len() as u8 + 1;
        for &(l, r) in rows {
            let r = if l == 0xff && r != 0 && table != SPEED {
                start + r - 1
            } else {
                r
            };
            self.tables[table].push((l, r));
        }
        assert!(self.tables[table].len() <= 255, "table {table} full");
        self.table_names[table].insert(name.to_string(), start);
        start
    }

    pub fn wave(&mut self, name: &str, rows: &[(u8, u8)]) -> u8 {
        self.table(WAVE, name, rows)
    }

    pub fn pulse(&mut self, name: &str, rows: &[(u8, u8)]) -> u8 {
        self.table(PULSE, name, rows)
    }

    pub fn filter(&mut self, name: &str, rows: &[(u8, u8)]) -> u8 {
        self.table(FILTER, name, rows)
    }

    pub fn speed(&mut self, name: &str, left: u8, right: u8) -> u8 {
        self.table(SPEED, name, &[(left, right)])
    }

    fn table_at(&self, table: usize, name: &str) -> u8 {
        if name.is_empty() {
            return 0;
        }
        *self.table_names[table]
            .get(name)
            .unwrap_or_else(|| panic!("no table program {name:?}"))
    }

    /// An instrument: AD, SR, its wave, pulse and filter programs (by name, "" none), its
    /// vibrato (a speedtable name, "" none) and delay, the gate-off timer, the first-frame
    /// waveform.
    #[allow(clippy::too_many_arguments)]
    pub fn instrument(
        &mut self,
        name: &str,
        ad: u8,
        sr: u8,
        wave: &str,
        pulse: &str,
        filter: &str,
        vibrato: (&str, u8),
        gate: u8,
        first: u8,
    ) {
        let mut i = [0u8; 25];
        i[..9].copy_from_slice(&[
            ad,
            sr,
            self.table_at(WAVE, wave),
            self.table_at(PULSE, pulse),
            self.table_at(FILTER, filter),
            self.table_at(SPEED, vibrato.0),
            vibrato.1,
            gate,
            first,
        ]);
        let label = name.as_bytes();
        i[9..9 + label.len().min(16)].copy_from_slice(&label[..label.len().min(16)]);
        self.instr.push(i);
        assert!(self.instr.len() <= 63);
        self.instr_names
            .insert(name.to_string(), self.instr.len() as u8);
    }

    /// A shorthand row for patterns (a drum hit, say).
    pub fn alias(&mut self, name: &str, row: &str) {
        self.aliases.insert(name.to_string(), row.to_string());
    }

    fn row(&self, token: &str) -> Vec<[u8; 4]> {
        let (head, cmd) = match token.split_once('@') {
            Some((h, c)) => (h, Some(c)),
            None => (token, None),
        };
        let (head, instr) = match head.split_once('/') {
            Some((h, i)) => (h, Some(i)),
            None => (head, None),
        };
        if let Some(a) = self.aliases.get(head) {
            let mut r = self.row(a);
            assert_eq!(r.len(), 1, "alias {head} is one row");
            if let Some(i) = instr {
                r[0][1] = self.instr_number(i);
            }
            if let Some(c) = cmd {
                (r[0][2], r[0][3]) = self.command(c);
            }
            return r;
        }
        let mut count = 1;
        let n = if head == "." {
            0xbd
        } else if let Some(k) = head.strip_prefix('.') {
            count = k.parse().unwrap_or_else(|_| panic!("bad rest {token:?}"));
            0xbd
        } else if head == "^" {
            0xbe
        } else if head == "+" {
            0xbf
        } else {
            let v = note(head).unwrap_or_else(|| panic!("bad note {token:?}"));
            assert!(v <= 0x5c, "{token:?} above G#7");
            0x60 + v
        };
        let mut r = [n, 0, 0, 0];
        if let Some(i) = instr {
            r[1] = self.instr_number(i);
        }
        if let Some(c) = cmd {
            (r[2], r[3]) = self.command(c);
        }
        let mut out = vec![r];
        out.extend(std::iter::repeat_n([0xbd, 0, 0, 0], count - 1));
        out
    }

    fn instr_number(&self, name: &str) -> u8 {
        *self
            .instr_names
            .get(name)
            .unwrap_or_else(|| panic!("no instrument {name:?}"))
    }

    fn command(&self, c: &str) -> (u8, u8) {
        let digit = u8::from_str_radix(&c[..1], 16).unwrap_or_else(|_| panic!("bad command {c:?}"));
        let rest = &c[1..];
        let data = if let Some(name) = rest.strip_prefix('=') {
            let table = match digit {
                8 => WAVE,
                9 => PULSE,
                0xa => FILTER,
                _ => SPEED,
            };
            self.table_at(table, name)
        } else {
            u8::from_str_radix(rest, 16).unwrap_or_else(|_| panic!("bad command {c:?}"))
        };
        (digit, data)
    }

    /// A pattern from rows in the notation above. It must be `rows` long.
    pub fn pattern(&mut self, name: &str, rows: usize, text: &str) {
        let p: Vec<[u8; 4]> = text
            .split_whitespace()
            .filter(|t| *t != "|")
            .flat_map(|t| self.row(t))
            .collect();
        assert_eq!(p.len(), rows, "pattern {name}: {} rows", p.len());
        assert!(rows <= 128);
        assert!(
            self.pattern_names
                .insert(name.to_string(), self.patterns.len() as u8)
                .is_none(),
            "pattern {name} twice"
        );
        self.patterns.push(p);
        assert!(self.patterns.len() <= 208);
    }

    /// A channel's orderlist in the notation above.
    pub fn order(&mut self, channel: usize, text: &str) {
        let mut e = Vec::new();
        let mut restart = None;
        for t in text.split_whitespace() {
            if t == "|" {
                restart = Some(e.len() as u8);
            } else if let Some(k) = t.strip_prefix('x') {
                let k: u8 = k.parse().unwrap();
                assert!((1..=16).contains(&k));
                e.push(0xd0 + (k - 1) % 16);
            } else if let Some(k) = t.strip_prefix('+') {
                let k: u8 = k.parse().unwrap();
                assert!(k <= 14);
                e.push(0xf0 + k);
            } else if let Some(k) = t.strip_prefix('-') {
                let k: u8 = k.parse().unwrap();
                assert!((1..=16).contains(&k));
                e.push(0xf0 - k);
            } else {
                e.push(
                    *self
                        .pattern_names
                        .get(t)
                        .unwrap_or_else(|| panic!("no pattern {t:?}")),
                );
            }
        }
        self.orders[channel] = (e, restart.unwrap_or(0));
    }

    /// The rows one pass of channel 1's orderlist plays, from the start to its end.
    pub fn rows(&self) -> u32 {
        let (e, _) = &self.orders[0];
        let mut rows = 0;
        let mut times = 1;
        for &b in e {
            match b {
                0xd0..=0xdf => times = (b - 0xd0) as u32 + 1,
                0xe0..=0xfe => {}
                p => {
                    rows += times * self.patterns[p as usize].len() as u32;
                    times = 1;
                }
            }
        }
        rows
    }

    /// The plain GoatTracker file, as written exactly from the kit.
    pub fn spec(&self) -> SngSpec {
        let string = |s: &str| {
            let mut b = [0u8; 32];
            b[..s.len()].copy_from_slice(s.as_bytes());
            b
        };
        let mut s = SngSpec::new();
        s.strings = [string(&self.title), string(AUTHOR), string(COPYRIGHT)];
        s.orders = vec![self.orders.to_vec()];
        s.instr = self.instr.clone();
        s.tables = self.tables.to_vec();
        s.patterns = self
            .patterns
            .iter()
            .map(|p| (p.clone(), [0xff, 0, 0, 0]))
            .collect();
        s
    }
}

impl Showcase {
    pub fn path(&self) -> PathBuf {
        repo_root().join(format!("assets/songs/{}.sng", self.name))
    }

    /// The song read into SIDOU's model, its chip's setup added (`CHPA`).
    pub fn song(&self) -> Song {
        let plain = (self.kit)().spec().encode();
        let mut s = Song::host();
        s.read(&plain, 0)
            .unwrap_or_else(|e| panic!("{}: {}", self.name, e.msg));
        let c = song_chip_setup {
            model: (self.model == SID_MODEL_8580) as u8,
            envelope: 1, // AUTHENTIC, a song's default (decision record 0005)
            ..Default::default()
        };
        assert_eq!(unsafe { song_ext_set_chip(s.ptr_mut(), TAG_CHPA, &c) }, 0);
        s
    }

    /// The play routine's calls in one pass: up to the call that starts channel 1's first
    /// row after its orderlist's end.
    pub fn frames(&self) -> u32 {
        let rows = (self.kit)().rows();
        let song = self.song();
        let mut p = Player::new(&song, 0, &default_options()).expect("the player takes it");
        let mut seen = 0;
        for f in 0.. {
            p.call();
            if p.raw().ev[0].flags as u32 & GT2_EV_ROW != 0 {
                if seen == rows {
                    return f;
                }
                seen += 1;
            }
        }
        unreachable!()
    }

    /// The file: SIDOU's writer's output, with its trailer.
    pub fn sng(&self) -> Vec<u8> {
        self.song().write(true)
    }

    /// The song without its trailer, as GoatTracker saves it.
    pub fn plain(&self) -> Vec<u8> {
        self.song().write(false)
    }
}

/// The files that differ from what the sources make; with `update`, rewritten.
pub fn check_or_write(update: bool) -> Vec<PathBuf> {
    let mut stale = Vec::new();
    for s in all() {
        let want = s.sng();
        let path = s.path();
        if std::fs::read(&path).ok().as_deref() != Some(&want[..]) {
            if update {
                std::fs::write(&path, &want).unwrap();
            } else {
                stale.push(path);
            }
        }
    }
    stale
}

/// The render's sample rate.
pub const RATE: u32 = 44_100;
/// A PAL frame in C64 cycles, and the clock (decision record 0007, "The player's place").
const FRAME_CYCLES: u64 = 19_656;
const PAL_HZ: u64 = 985_248;
/// The release after the last frame, in frames.
pub const TAIL_FRAMES: u32 = 100;

/// One pass of the song on SIDOU's player and SID core, at 44.1 kHz on the PAL frame clock;
/// then every gate off and a short tail for the releases.
pub fn render(song: &Song, model: sid_model, frames: u32) -> Vec<i16> {
    render_voices(song, model, frames, 7)
}

/// [`render`] with only the voices in `voices` (bit 0 voice 1) sounding: the others' voice
/// registers are not written (a stem; a voice synced or ring-modulated by a silent one
/// loses that).
pub fn render_voices(song: &Song, model: sid_model, frames: u32, voices: u8) -> Vec<i16> {
    let mut chip = Box::<sid>::default();
    unsafe { sid_init(&mut *chip, model, SID_CLOCK_PAL, RATE) };
    let mut p = Player::new(song, 0, &default_options()).expect("the player takes the song");
    let mut out = Vec::new();
    let mut owed: u64 = 0;
    let mut waves = [0u8; 3];
    for f in 0..frames + TAIL_FRAMES {
        if f < frames {
            for (r, v) in p.call() {
                if r < 21 && voices & (1 << (r / 7)) == 0 {
                    continue;
                }
                if r < 21 && r % 7 == 4 {
                    waves[(r / 7) as usize] = v;
                }
                unsafe { sid_write(&mut *chip, r, v) };
            }
        } else if f == frames {
            for (k, w) in waves.iter().enumerate() {
                unsafe { sid_write(&mut *chip, (k * 7 + 4) as u8, w & 0xfe) };
            }
        }
        owed += FRAME_CYCLES * RATE as u64;
        let n = (owed / PAL_HZ) as usize;
        owed %= PAL_HZ;
        let at = out.len();
        out.resize(at + n, 0);
        unsafe { sid_render(&mut *chip, out[at..].as_mut_ptr(), n as u32) };
    }
    out
}

/// A 16-bit mono WAV file.
pub fn wav(samples: &[i16], rate: u32) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}
