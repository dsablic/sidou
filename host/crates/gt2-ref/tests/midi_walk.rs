// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The fixtures for the Songs -> MIDI page's note walk (`web/songmidi.js`): what GoatTracker 2.77's
//! packed player does with each voice's gate, call by call, for SIDOU's songs and GoatTracker's
//! examples. `web/songmidi.test.mjs` runs the JavaScript walk and compares every note's start,
//! end and pitch with these runs.
//!
//! A run is a stretch of calls with the voice's gate bit (SID register 4, bit 0) high:
//! `[first call, first call after, frequency a call after the first]` (the frequency register
//! at a note's first call is the previous note's; the wavetable's first step sets it on the
//! next). Call 0 is the first play call after init. Each voice also has its frequency's course
//! as the notes of GoatTracker's frequency table it sounds (`[call, note]` at each change of
//! the register's table note, -1 when the register holds no table frequency, as in a slide or
//! vibrato), which the node test compares with the pitch of every note the walk makes, wavetable
//! arpeggios included. The test checks the committed fixtures are
//! what the reference makes now; `SIDOU_MIDI_FIXTURE_UPDATE=1` rewrites them.
//!
//!   SIDOU_MIDI_FIXTURE_UPDATE=1 cargo test --release -p gt2-ref --test midi_walk

use gt2_ref::trace::trace;
use gt2_ref::*;
use std::path::PathBuf;

/// Calls traced of each song: its orderlists' one pass (the walk's `ends`, which the node test
/// checks) and a margin, so a note held across the loop point can be compared too.
const CALLS: &[(&str, usize)] = &[
    ("chrome-comet", 4200),
    ("iron-rite", 4500),
    ("neon-highway", 3900),
    ("paper-lanterns", 4300),
    ("sidou-loop", 600),
    ("2xtest", 4100),
    ("cabrinigreen", 8900),
    ("consultant", 9200),
    ("dojo", 8300),
    ("everlasting", 4900),
    ("funktest", 6400),
    ("ghosttrackers", 6800),
    ("hyperspace", 12900),
    ("sanction", 6500),
    ("sixpack", 4300),
    ("tempo2test", 400),
    ("transylvanian", 7300),
    ("unleash", 13700),
    ("wavecmdtest", 600),
];

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../web/songmidi-fixtures")
}

fn songs() -> Vec<(String, Vec<u8>)> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../assets/songs");
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(assets)
        .expect("assets/songs")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sng"))
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect();
    v.sort();
    v.extend(examples());
    v
}

/// One voice's runs of a high gate bit, and the frequency a call after each run starts.
fn runs(regs: &[[u8; 25]], voice: usize) -> Vec<[u32; 3]> {
    let (gate, lo) = (4 + 7 * voice, 7 * voice);
    let freq = |f: usize| regs[f][lo] as u32 | (regs[f][lo + 1] as u32) << 8;
    let mut out: Vec<[u32; 3]> = Vec::new();
    let mut start = None;
    for f in 0..=regs.len() {
        let on = f < regs.len() && regs[f][gate] & 1 == 1;
        match (start, on) {
            (None, true) => start = Some(f),
            (Some(s), false) => {
                let at = if s + 1 < f { s + 1 } else { s };
                out.push([s as u32, f as u32, freq(at)]);
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// GoatTracker's frequency table (gplay.c `freqtbllo`/`freqtblhi`: 128 entries, C-0 up).
fn freq_table() -> Vec<u16> {
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../third_party/goattracker2/src/gplay.c"),
    )
    .expect("gplay.c");
    let arr = |name: &str| -> Vec<u16> {
        let at = src.find(&format!("{name}[] = {{")).expect(name);
        let body = &src[src[at..].find('{').unwrap() + at + 1..];
        body[..body.find('}').unwrap()]
            .split(',')
            .filter_map(|x| {
                x.trim()
                    .strip_prefix("0x")
                    .and_then(|h| u16::from_str_radix(h, 16).ok())
            })
            .collect()
    };
    let (lo, hi) = (arr("freqtbllo"), arr("freqtblhi"));
    assert_eq!((lo.len(), hi.len()), (128, 128));
    lo.iter().zip(&hi).map(|(l, h)| l | h << 8).collect()
}

/// One voice's table note at each change, per call (-1: not a table frequency).
fn course(regs: &[[u8; 25]], voice: usize, table: &[u16]) -> Vec<[i32; 2]> {
    let lo = 7 * voice;
    let mut out: Vec<[i32; 2]> = Vec::new();
    for (f, r) in regs.iter().enumerate() {
        let freq = r[lo] as u16 | (r[lo + 1] as u16) << 8;
        let note = table
            .iter()
            .position(|&t| t == freq)
            .map_or(-1, |n| n as i32);
        if out.last().is_none_or(|l| l[1] != note) {
            out.push([f as i32, note]);
        }
    }
    out
}

fn fixture(sng: &[u8], calls: usize) -> String {
    let t = trace(sng, 0, calls, &Options::default()).expect("trace");
    let mut regs = [0u8; 25];
    let mut per_call = Vec::with_capacity(calls);
    for f in &t.frames {
        for call in &f.calls {
            for &(r, v) in call {
                regs[r as usize] = v;
            }
            per_call.push(regs);
        }
    }
    let table = freq_table();
    let courses: Vec<String> = (0..3)
        .map(|v| {
            let rows: Vec<String> = course(&per_call, v, &table)
                .iter()
                .map(|r| format!("[{},{}]", r[0], r[1]))
                .collect();
            format!("[{}]", rows.join(","))
        })
        .collect();
    let voices: Vec<String> = (0..3)
        .map(|v| {
            let rows: Vec<String> = runs(&per_call, v)
                .iter()
                .map(|r| format!("[{},{},{}]", r[0], r[1], r[2]))
                .collect();
            format!("[{}]", rows.join(","))
        })
        .collect();
    format!(
        "{{\"calls\":{},\"voices\":[\n{}\n],\"course\":[\n{}\n]}}\n",
        per_call.len(),
        voices.join(",\n"),
        courses.join(",\n")
    )
}

#[test]
fn the_fixtures_are_what_the_reference_player_makes() {
    let update = std::env::var_os("SIDOU_MIDI_FIXTURE_UPDATE").is_some();
    let dir = fixture_dir();
    let mut bad = Vec::new();
    for (name, sng) in songs() {
        if subtunes(&sng) != 1 {
            continue;
        }
        let stem = name.trim_end_matches(".sng");
        let calls = CALLS
            .iter()
            .find(|(n, _)| *n == stem)
            .unwrap_or_else(|| panic!("{stem}: add it to CALLS"))
            .1;
        let path = dir.join(format!("{stem}.json"));
        let now = fixture(&sng, calls);
        if update {
            std::fs::write(&path, &now).unwrap();
        } else if std::fs::read_to_string(&path).ok().as_deref() != Some(&now) {
            bad.push(name);
        }
    }
    assert!(
        bad.is_empty(),
        "stale or missing fixtures {bad:?}: SIDOU_MIDI_FIXTURE_UPDATE=1 cargo test --release -p gt2-ref --test midi_walk"
    );
}
