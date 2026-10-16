// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's first song (M2c Task 6), "SIDOU LOOP": an original eight-bar loop written for
//! SIDOU, as data in this file (the song's source), written out as a GoatTracker 2 `.sng`
//! (`assets/songs/sidou-loop.sng`, which GoatTracker opens) and compiled into the firmware
//! as `app/demo_song.c` (boot copies it into the song's region; PLAY plays it until M2d's
//! transport). `tests/app/demo_song.rs` keeps both in step with this file;
//! SIDOU_SONG_UPDATE=1 rewrites them.
//!
//! Three channels: a pulse lead with a pulse-width sweep, vibrato and a low-pass filter
//! sweep; a saw bass; drums (kick, snare and hat from wavetable programs). Am, F, C, G.

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::font::repo_root;
use crate::song::SngSpec;

pub const TITLE: &str = "SIDOU LOOP";
pub const AUTHOR: &str = "SIDOU contributors";

const R: [u8; 4] = [0xbd, 0, 0, 0];

/// A note: octave and semitone (C-0 is 0), GoatTracker's numbering.
fn n(octave: u8, semi: u8) -> u8 {
    0x60 + octave * 12 + semi
}

fn row(note: u8, instr: u8) -> [u8; 4] {
    [note, instr, 0, 0]
}

#[allow(clippy::too_many_arguments)]
fn ins(
    ad: u8,
    sr: u8,
    w: u8,
    p: u8,
    f: u8,
    vib: u8,
    vd: u8,
    gt: u8,
    fw: u8,
    name: &str,
) -> [u8; 25] {
    let mut i = [0u8; 25];
    i[..9].copy_from_slice(&[ad, sr, w, p, f, vib, vd, gt, fw]);
    i[9..9 + name.len()].copy_from_slice(name.as_bytes());
    i
}

fn string(s: &str) -> [u8; 32] {
    let mut b = [0u8; 32];
    b[..s.len()].copy_from_slice(s.as_bytes());
    b
}

/// A bass bar on a chord's root (semitone, octave) and its fifth.
fn bass(root: u8, octave: u8) -> Vec<[u8; 4]> {
    let lo = n(octave, root);
    let hi = lo + 12;
    let fifth = lo + 7;
    let mut p = vec![
        row(lo, 1),
        R,
        row(hi, 0),
        R,
        row(lo, 0),
        R,
        row(fifth, 0),
        R,
    ];
    p.extend([
        row(lo, 0),
        R,
        row(hi, 0),
        R,
        row(lo, 0),
        row(lo, 0),
        row(fifth, 0),
        R,
    ]);
    p
}

/// A lead bar: (row, note) pairs, the instrument on the first.
fn lead(notes: &[(usize, u8)]) -> Vec<[u8; 4]> {
    let mut p = vec![R; 16];
    for (k, &(r, note)) in notes.iter().enumerate() {
        p[r] = row(note, if k == 0 { 2 } else { 0 });
    }
    p
}

/// A drum bar: K kick, S snare, H hat, . rest.
fn drums(beat: &str) -> Vec<[u8; 4]> {
    beat.bytes()
        .map(|c| match c {
            b'K' => row(n(4, 0), 3),
            b'S' => row(n(4, 0), 4),
            b'H' => row(n(4, 0), 5),
            _ => R,
        })
        .collect()
}

/// The song.
pub fn sng() -> Vec<u8> {
    let mut s = SngSpec::new();
    s.strings = [
        string(TITLE),
        string(AUTHOR),
        string("2026 GPL-3.0-or-later"),
    ];
    // Patterns 0-3 the bass (Am F C G), 4-7 the lead, 8 and 9 the drums.
    let mut patterns = vec![bass(9, 1), bass(5, 1), bass(0, 2), bass(7, 1)];
    patterns.push(lead(&[
        (0, n(4, 4)),
        (4, n(4, 9)),
        (6, n(5, 0)),
        (8, n(4, 11)),
        (10, n(4, 9)),
        (12, n(4, 7)),
        (14, n(4, 4)),
    ]));
    patterns.push(lead(&[
        (0, n(4, 5)),
        (4, n(4, 9)),
        (6, n(5, 0)),
        (8, n(5, 2)),
        (10, n(5, 0)),
        (12, n(4, 9)),
        (14, n(4, 5)),
    ]));
    patterns.push(lead(&[
        (0, n(4, 7)),
        (4, n(5, 0)),
        (6, n(5, 4)),
        (8, n(5, 2)),
        (10, n(5, 0)),
        (12, n(4, 7)),
        (14, n(4, 4)),
    ]));
    patterns.push(lead(&[
        (0, n(4, 2)),
        (4, n(4, 7)),
        (6, n(4, 11)),
        (8, n(5, 2)),
        (12, n(5, 0)),
        (14, n(4, 11)),
    ]));
    patterns.push(drums("K.H.S.H.K.K.S.HH"));
    patterns.push(drums("K.H.S.H.K.S.SSSS"));
    s.patterns = patterns.into_iter().map(|p| (p, [0xff, 0, 0, 0])).collect();
    s.orders = vec![vec![
        (vec![4, 5, 6, 7], 0),
        (vec![0, 1, 2, 3], 0),
        (vec![8, 8, 8, 9], 0),
    ]];
    s.instr = vec![
        ins(0x0a, 0x49, 1, 0, 0, 0, 0, 2, 0x09, "bass"),
        ins(0x26, 0x88, 3, 1, 1, 1, 0x10, 2, 0x09, "lead"),
        ins(0x08, 0x00, 5, 0, 0, 0, 0, 2, 0x09, "kick"),
        ins(0x08, 0x00, 11, 0, 0, 0, 0, 2, 0x09, "snare"),
        ins(0x01, 0x00, 16, 0, 0, 0, 0, 2, 0x09, "hat"),
    ];
    s.tables = vec![
        // the wavetable: bass saw (1), lead pulse (3), kick (5), snare (11), hat (16)
        vec![
            (0x21, 0x00),
            (0xff, 0x00),
            (0x41, 0x00),
            (0xff, 0x00),
            (0x81, 0xd4),
            (0x11, 0xa4),
            (0x11, 0x9c),
            (0x11, 0x94),
            (0x10, 0x90),
            (0xff, 0x00),
            (0x81, 0xdc),
            (0x41, 0xac),
            (0x81, 0xd0),
            (0x80, 0xc8),
            (0xff, 0x00),
            (0x81, 0xdf),
            (0x80, 0xdf),
            (0xff, 0x00),
        ],
        // the pulse table: the lead's width, then a sweep up and down
        vec![(0x88, 0x00), (0x20, 0x10), (0x20, 0xf0), (0xff, 0x02)],
        // the filter table: low-pass on the lead, its cutoff swept
        vec![
            (0x90, 0x61),
            (0x00, 0x30),
            (0x40, 0x01),
            (0x40, 0xff),
            (0xff, 0x03),
        ],
        // the speed table: the lead's vibrato
        vec![(0x03, 0x20)],
    ];
    s.encode()
}

pub fn sng_path() -> PathBuf {
    repo_root().join("assets/songs/sidou-loop.sng")
}

pub fn generated_path() -> PathBuf {
    repo_root().join("app/demo_song.c")
}

/// app/demo_song.c for a song's bytes.
pub fn c_source(sng: &[u8]) -> String {
    let mut out = String::new();
    // REUSE-IgnoreStart
    out += "// SPDX-FileCopyrightText: 2026 SIDOU contributors\n";
    out += "// SPDX-License-Identifier: GPL-3.0-or-later\n";
    // REUSE-IgnoreEnd
    out +=
        "/* Generated by host/crates/sidou-tests/src/song_embed.rs from its song, written out as\n";
    out += " * assets/songs/sidou-loop.sng: do not edit (SIDOU_SONG_UPDATE=1 cargo test rewrites it).\n";
    let _ = writeln!(
        out,
        " * {TITLE}, by {AUTHOR}, GPL-3.0-or-later (assets/songs/sidou-loop.md). */"
    );
    out += "#include \"demo_song.h\"\n\n";
    let _ = writeln!(out, "const uint8_t demo_song_sng[{}] = {{", sng.len());
    for chunk in sng.chunks(16) {
        let line: Vec<String> = chunk.iter().map(|b| format!("0x{b:02x}")).collect();
        let _ = writeln!(out, "    {},", line.join(", "));
    }
    out += "};\n";
    let _ = writeln!(out, "const uint32_t demo_song_sng_bytes = {}u;", sng.len());
    out
}

/// The files that differ from what this file makes; with `update`, rewritten.
pub fn check_or_write(update: bool) -> Vec<PathBuf> {
    let sng = sng();
    let files = [
        (sng_path(), sng.clone()),
        (generated_path(), c_source(&sng).into_bytes()),
    ];
    let mut stale = Vec::new();
    for (path, want) in files {
        if std::fs::read(&path).ok().as_deref() != Some(&want[..]) {
            if update {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, &want).unwrap();
            } else {
                stale.push(path);
            }
        }
    }
    stale
}
