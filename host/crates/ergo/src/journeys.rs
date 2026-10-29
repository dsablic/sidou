// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Journeys: scripted input sequences with checkpoints, run in fm1-emu. The table is
//! tables/journeys.txt (grammar in its header); each step has a screenshot, the optional
//! audio clip, and the metrics the ergonomics rules read: actions, hands, menu levels, frames
//! to completion, dead ends, "not yet possible" steps. The same table gives the common tasks
//! for the effort and consistency checks.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::config::Config;
use crate::device::{key_of_note, panel, Dev, Params, Shot, FRAME_MS};
use crate::metrics::{
    count_effort, db, dc_fraction, rms_dbfs, spectral_centroid, Effort, EffortCount,
};
use crate::readability::Captured;
use crate::screen::{text_runs, write_png};

pub const DEFAULT_TABLE: &str = include_str!("../tables/journeys.txt");

/// The audio rate the emulator plays at (24 MHz / 544).
pub const FS: f64 = 24e6 / 544.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Boot,
    Press(u8),
    Hold(u8, u64),
    Down(u8),
    Up(u8),
    Turn(u8, i32),
    /// A button held while an encoder turns.
    HoldTurn(u8, u8, i32),
    /// Two buttons held while an encoder turns (EDIT + SCL, then K1).
    Hold2Turn(u8, u8, u8, i32),
    /// A button held while a note key is pressed (GLO + key).
    HoldKey(u8, u8),
    /// A button held while another button is pressed (EDIT + SCL).
    HoldPress(u8, u8),
    /// Note keys (MIDI note numbers) held together for ms.
    Note(Vec<u8>, u64),
    /// Moves the menu's selection onto the item showing this text; one turn.
    SelectItem(String),
    Midi(Vec<u8>),
    Pot(u16),
    Wait(u64),
    /// MIDI clock at bpm for ms, with a start first.
    Clock(u32, u64, bool),
    PowerCycle,
    /// Note keys played in time: at `bpm`, each event a chord (empty: a rest) lasting `sixteenths`
    /// 16th notes, the keys held for most of it (`play`).
    Play(u32, Vec<(Vec<u8>, u32)>),
    /// Note keys tapped one after another, each held `ms` with a short gap (`taps`: STEP's keys).
    Taps(Vec<u8>, u64),
    /// An encoder turned `n` detents spread evenly over `ms` (`sweep`): a hand turning slowly.
    Sweep(u8, i32, u64),
    /// Waits for the playing song's voice 1 to reach the first row of a pattern (`sync`): what
    /// follows starts on the bar.
    Sync,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expect {
    View(String),
    Sound,
    Silent,
    Change,
    Popup,
    Toast,
    Text(String),
    /// This text is on the screen at 2x or larger: readable at arm's length (the play-time
    /// minimum of the readability check, docs/TESTING.md).
    BigText(String),
    /// The remembered parameters are what they were.
    Recalled,
    /// A parameter (a name of `Params::named`) within a range.
    Value(String, i64, i64),
    /// This text is not on the screen.
    Absent(String),
    /// The sound against the 400 ms before the step: brighter or darker (spectral centroid, 5%),
    /// louder or quieter (RMS, 1 dB). Needs a sound before the step.
    Brighter,
    Darker,
    Louder,
    Quieter,
    /// The RMS level of the last 400 ms, dBFS, within a range.
    Level(f64, f64),
    /// The song's pattern that voice `v` (1..3) plays at orderlist entry `e` (subtune 0) holds
    /// these notes (GoatTracker values), in row order, rests and key-offs skipped.
    Notes(u8, u8, Vec<u8>),
    /// That pattern holds a note on exactly these rows (0-based).
    Rows(u8, u8, Vec<u8>),
}

/// Where a step's video looks (docs/reference/recipe-schema.md, `focus`): the control used (a
/// button, encoder, MASTER, a note, `device` for the whole FM-1, `screen` for the screen alone,
/// `card` for a card instead of the device while the step runs),
/// a region of the screen in cells (col, row, cols, rows of the 30 x 30 grid) and a callout.
/// Every part is optional; the video derives what is missing from the step's inputs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Focus {
    pub control: Option<String>,
    pub screen: Option<[u8; 4]>,
    pub callout: Option<String>,
}

impl Focus {
    pub fn is_empty(&self) -> bool {
        *self == Focus::default()
    }
}

/// A focus control: a button, an encoder, MASTER, a note key (F3..G5), `device`, `screen` or
/// `card`.
pub fn check_focus_control(c: &str) -> Result<(), String> {
    if BUTTONS.iter().any(|(n, _)| *n == c)
        || ENCODERS.iter().any(|(n, _)| *n == c)
        || ["MASTER", "device", "screen", "card"].contains(&c)
        || note_number(c).is_ok()
    {
        Ok(())
    } else {
        Err(format!("unknown focus control {c}"))
    }
}

/// A screen region inside the 30 x 30 cells, at least one cell.
pub fn check_screen(r: [u8; 4]) -> Result<(), String> {
    let [c, row, w, h] = r.map(u32::from);
    if w == 0 || h == 0 || c + w > 30 || row + h > 30 {
        Err(format!(
            "screen region {c} {row} {w} {h} is outside the 30 x 30 cells"
        ))
    } else {
        Ok(())
    }
}

/// `C-3`, `D#4` (TRACKER's names) -> GoatTracker's note value.
pub fn gt_note(s: &str) -> Result<u8, String> {
    let b = s.as_bytes();
    if b.len() != 3 {
        return Err(format!("bad pattern note {s} (C-3, D#4)"));
    }
    let base = match b[0] {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return Err(format!("bad pattern note {s}")),
    };
    let sharp = match b[1] {
        b'-' => 0,
        b'#' => 1,
        _ => return Err(format!("bad pattern note {s}")),
    };
    let oct = match b[2] {
        o @ b'0'..=b'7' => o - b'0',
        _ => return Err(format!("bad pattern note {s}")),
    };
    Ok(sidou_sys::GT2_FIRSTNOTE as u8 + oct * 12 + base + sharp)
}

/// GoatTracker's note value -> `C-3`.
pub fn gt_note_name(v: u8) -> String {
    const N: [&str; 12] = [
        "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
    ];
    let n = v.wrapping_sub(sidou_sys::GT2_FIRSTNOTE as u8);
    format!("{}{}", N[usize::from(n % 12)], n / 12)
}

#[derive(Clone, Debug)]
pub struct Step {
    pub setup: bool,
    pub id: String,
    pub label: String,
    pub caption: Option<String>,
    pub acts: Vec<Act>,
    pub expects: Vec<Expect>,
    pub needs: Vec<String>,
    pub clip_ms: Option<u64>,
    pub probe: bool,
    pub task: Option<String>,
    pub milestone: Option<String>,
    pub destructive: bool,
    pub remember: bool,
    pub defaults: bool,
    /// The physical control in panel terms ("turn K1 clockwise about a quarter turn"); made from
    /// the actions when absent.
    pub control: Option<String>,
    /// Where the video looks during the step.
    pub focus: Focus,
    /// The step opens a chapter of a long video, with this title.
    pub chapter: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Journey {
    pub id: String,
    pub title: String,
    /// A tour only visits screens (for readability): no effort scoring.
    pub tour: bool,
    /// Starts at power-on (time 0, the boot running); else on LIVE, booted.
    pub poweron: bool,
    /// Recipe header: the one-line goal, what must be true to start, the skills taught and the
    /// related recipes (docs/reference/recipe-schema.md).
    pub goal: String,
    pub prereq: String,
    pub skills: Vec<String>,
    pub related: Vec<String>,
    /// Showcase songs (assets/songs/<name>.sng) put into the store before power-on.
    pub songs: Vec<String>,
    /// Starter-pack tunes (assets/starter/<name>.sid) put into the store before power-on, as a
    /// SysEx PUT of type 2 leaves them: the file's own bytes, under its store name.
    pub tunes: Vec<String>,
    /// Another recipe whose steps run first, unfilmed: this one starts where that one ends.
    pub after: Option<String>,
    /// The steps `after` resolves to (its own prelude first), run before step 1, and the songs
    /// that chain puts in the store.
    pub prelude: Vec<Step>,
    pub prelude_songs: Vec<String>,
    pub prelude_tunes: Vec<String>,
    pub steps: Vec<Step>,
}

/// Fills each journey's `prelude` from its `after` chain, and adds the chain's songs to its own.
pub fn resolve_after(js: &mut [Journey]) -> Result<(), String> {
    let all: Vec<Journey> = js.to_vec();
    for j in js.iter_mut() {
        let mut chain: Vec<&Journey> = Vec::new();
        let mut next = j.after.clone();
        while let Some(id) = next {
            if id == j.id || chain.iter().any(|c| c.id == id) {
                return Err(format!("{}: `after` goes round in a circle at {id}", j.id));
            }
            let p = all
                .iter()
                .find(|x| x.id == id)
                .ok_or_else(|| format!("{}: `after` names no recipe {id}", j.id))?;
            chain.push(p);
            next = p.after.clone();
        }
        j.prelude.clear();
        j.prelude_songs.clear();
        j.prelude_tunes.clear();
        for p in chain.iter().rev() {
            if p.poweron {
                return Err(format!("{}: `after` {} starts at power-on", j.id, p.id));
            }
            j.prelude.extend(p.steps.iter().cloned());
            j.prelude_songs.extend(p.songs.iter().cloned());
            j.prelude_tunes.extend(p.tunes.iter().cloned());
        }
        if j.poweron && j.after.is_some() {
            return Err(format!(
                "{}: a recipe with `after` cannot start at power-on",
                j.id
            ));
        }
    }
    Ok(())
}

pub const FEATURES: [&str; 8] = [
    "theme",
    "undo",
    "help",
    "order_view",
    "instr_view",
    "pattern_edit",
    "new_song",
    "song_save",
];

pub const BUTTONS: [(&str, u8); 14] = [
    ("OCT-", panel::OCT_DOWN),
    ("OCT+", panel::OCT_UP),
    ("FX", panel::FX),
    ("SCL", panel::SCL),
    ("ENV", panel::ENV),
    ("LFO", panel::LFO),
    ("EDIT", panel::EDIT),
    ("GLO", panel::GLO),
    ("HOME", panel::HOME),
    ("SAVE", panel::SAVE),
    ("ARP", panel::ARP),
    ("SEQ", panel::SEQ),
    ("PLAY", panel::PLAY),
    ("REC", panel::REC),
];

pub const ENCODERS: [(&str, u8); 7] = [
    ("SELECT", panel::SELECT),
    ("ALGORITHM", panel::ALGORITHM),
    ("K1", panel::K1),
    ("K2", panel::K2),
    ("K3", panel::K3),
    ("K4", panel::K4),
    ("PRESETS", panel::PRESETS),
];

fn button(s: &str) -> Result<u8, String> {
    BUTTONS
        .iter()
        .find(|(n, _)| *n == s)
        .map(|(_, i)| *i)
        .ok_or_else(|| format!("unknown button {s}"))
}

fn encoder(s: &str) -> Result<u8, String> {
    ENCODERS
        .iter()
        .find(|(n, _)| *n == s)
        .map(|(_, i)| *i)
        .ok_or_else(|| format!("unknown encoder {s}"))
}

/// `A4`, `C#3`: MIDI note (C4 = 60).
pub fn note_number(s: &str) -> Result<u8, String> {
    let b = s.as_bytes();
    let base = match b.first() {
        Some(b'C') => 0,
        Some(b'D') => 2,
        Some(b'E') => 4,
        Some(b'F') => 5,
        Some(b'G') => 7,
        Some(b'A') => 9,
        Some(b'B') => 11,
        _ => return Err(format!("bad note {s}")),
    };
    let (sharp, rest) = if b.get(1) == Some(&b'#') {
        (1, &s[2..])
    } else {
        (0, &s[1..])
    };
    let oct: i32 = rest.parse().map_err(|_| format!("bad note {s}"))?;
    let n = (oct + 1) * 12 + base + sharp;
    if !(53..=79).contains(&n) {
        return Err(format!("note {s} is off the keys (F3..G5)"));
    }
    Ok(n as u8)
}

fn num<T: std::str::FromStr>(s: &str) -> Result<T, String> {
    s.trim()
        .trim_start_matches('+')
        .parse()
        .map_err(|_| format!("bad number {s}"))
}

fn parse_act(text: &str) -> Result<Act, String> {
    let w: Vec<&str> = text.split_whitespace().collect();
    let need = |n: usize| {
        if w.len() < n {
            Err(format!("`{text}` needs {} arguments", n - 1))
        } else {
            Ok(())
        }
    };
    need(1)?;
    Ok(match w[0] {
        "boot" => Act::Boot,
        "powercycle" => Act::PowerCycle,
        "press" => {
            need(2)?;
            Act::Press(button(w[1])?)
        }
        "hold" => {
            need(3)?;
            Act::Hold(button(w[1])?, num(w[2])?)
        }
        "down" => {
            need(2)?;
            Act::Down(key_of_note(note_number(w[1])?))
        }
        "up" => {
            need(2)?;
            Act::Up(key_of_note(note_number(w[1])?))
        }
        "turn" => {
            need(3)?;
            Act::Turn(encoder(w[1])?, num(w[2])?)
        }
        "holdturn" => {
            need(4)?;
            Act::HoldTurn(button(w[1])?, encoder(w[2])?, num(w[3])?)
        }
        "hold2turn" => {
            need(5)?;
            Act::Hold2Turn(button(w[1])?, button(w[2])?, encoder(w[3])?, num(w[4])?)
        }
        "holdkey" => {
            need(3)?;
            Act::HoldKey(button(w[1])?, note_number(w[2])?)
        }
        "holdpress" => {
            need(3)?;
            Act::HoldPress(button(w[1])?, button(w[2])?)
        }
        "note" => {
            need(3)?;
            let ms = num(w[w.len() - 1])?;
            let notes = w[1..w.len() - 1]
                .iter()
                .map(|n| note_number(n))
                .collect::<Result<_, _>>()?;
            Act::Note(notes, ms)
        }
        "select-item" => {
            need(2)?;
            Act::SelectItem(w[1..].join(" ").to_uppercase())
        }
        "midi" => {
            need(2)?;
            Act::Midi(
                w[1..]
                    .iter()
                    .map(|h| u8::from_str_radix(h, 16).map_err(|_| format!("bad hex {h}")))
                    .collect::<Result<_, _>>()?,
            )
        }
        "cc" => {
            need(3)?;
            Act::Midi(vec![0xb0, num(w[1])?, num(w[2])?])
        }
        "pc" => {
            need(2)?;
            Act::Midi(vec![0xc0, num(w[1])?])
        }
        "pot" => {
            need(2)?;
            Act::Pot(num(w[1])?)
        }
        "wait" => {
            need(2)?;
            Act::Wait(num(w[1])?)
        }
        "clock" => {
            need(3)?;
            Act::Clock(num(w[1])?, num(w[2])?, w.get(3) == Some(&"start"))
        }
        "play" => {
            need(3)?;
            let bpm: u32 = num(w[1])?;
            if !(40..=300).contains(&bpm) {
                return Err(format!("play: {bpm} BPM is not 40..300"));
            }
            Act::Play(
                bpm,
                w[2..]
                    .iter()
                    .map(|t| play_event(t))
                    .collect::<Result<_, _>>()?,
            )
        }
        "taps" => {
            need(3)?;
            let ms = num(w[w.len() - 1])?;
            let notes = w[1..w.len() - 1]
                .iter()
                .map(|n| note_number(n))
                .collect::<Result<_, _>>()?;
            Act::Taps(notes, ms)
        }
        "sync" => Act::Sync,
        "sweep" => {
            need(4)?;
            let n: i32 = num(w[2])?;
            if n == 0 {
                return Err("sweep: turn at least one detent".into());
            }
            Act::Sweep(encoder(w[1])?, n, num(w[3])?)
        }
        a => return Err(format!("unknown action {a}")),
    })
}

/// One `play` event: `C4`, `C4+E4` (a chord) or `.` (a rest), then `:N` for N 16ths (default 1).
fn play_event(t: &str) -> Result<(Vec<u8>, u32), String> {
    let (notes, len) = match t.split_once(':') {
        Some((n, l)) => (n, num(l)?),
        None => (t, 1),
    };
    if !(1..=64).contains(&len) {
        return Err(format!("play: {t}: a length of 1..64 16ths"));
    }
    let notes = if notes == "." {
        Vec::new()
    } else {
        notes
            .split('+')
            .map(note_number)
            .collect::<Result<_, _>>()?
    };
    Ok((notes, len))
}

/// A `play` event's on and off times, ms from its start: a 16th at `bpm` is 15000 / bpm ms, and
/// the keys are let go a little before the next event (as a player would, so a repeated key
/// strikes again).
pub fn play_times(bpm: u32, events: &[(Vec<u8>, u32)]) -> Vec<(u64, u64)> {
    let sixteenth = 15_000.0 / f64::from(bpm);
    let mut t = 0.0;
    events
        .iter()
        .map(|(_, len)| {
            let on = t;
            t += sixteenth * f64::from(*len);
            let off = (t - (sixteenth * 0.25).min(40.0)).max(on + 10.0);
            (on.round() as u64, off.round() as u64)
        })
        .collect()
}

/// Parses the table (grammar: tables/journeys.txt's header).
pub fn parse_table(text: &str) -> Result<Vec<Journey>, String> {
    let mut out: Vec<Journey> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |e: String| format!("journeys line {}: {e}", n + 1);
        if let Some(rest) = line.strip_prefix("journey ") {
            let p: Vec<&str> = rest.split('|').map(str::trim).collect();
            if p.len() < 2 {
                return Err(at("journey ID | title [| tour] [| poweron]".into()));
            }
            out.push(Journey {
                id: p[0].to_string(),
                title: p[1].to_string(),
                tour: p[2..].contains(&"tour"),
                poweron: p[2..].contains(&"poweron"),
                goal: String::new(),
                prereq: String::new(),
                skills: Vec::new(),
                related: Vec::new(),
                songs: Vec::new(),
                tunes: Vec::new(),
                after: None,
                prelude: Vec::new(),
                prelude_songs: Vec::new(),
                prelude_tunes: Vec::new(),
                steps: Vec::new(),
            });
        } else if let Some((k, v)) = ["goal", "prereq", "skills", "related", "songs", "tunes", "after"]
            .iter()
            .find_map(|k| {
                line.strip_prefix(k)
                    .and_then(|r| r.strip_prefix(' '))
                    .map(|r| (*k, r.trim()))
            })
        {
            let j = out
                .last_mut()
                .ok_or_else(|| at("header line before any journey".into()))?;
            let list = |v: &str| {
                v.split(';')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            };
            match k {
                "goal" => j.goal = v.to_string(),
                "prereq" => j.prereq = v.to_string(),
                "skills" => j.skills = list(v),
                "after" => j.after = Some(v.to_string()),
                "songs" => {
                    j.songs = list(v);
                    for name in &j.songs {
                        crate::device::showcase_song(name).map_err(at)?;
                    }
                }
                "tunes" => {
                    j.tunes = list(v);
                    for name in &j.tunes {
                        crate::device::starter_tune(name).map_err(at)?;
                    }
                }
                _ => j.related = list(v),
            }
        } else if let Some(rest) = line.strip_prefix("step ") {
            let j = out
                .last_mut()
                .ok_or_else(|| at("step before any journey".into()))?;
            let p: Vec<&str> = rest.split('|').map(str::trim).collect();
            if p.len() < 4 {
                return Err(at("step ID | label | caption | actions [| flags]".into()));
            }
            let mut s = Step {
                setup: false,
                id: p[0].to_string(),
                label: p[1].to_string(),
                caption: (p[2] != "-").then(|| p[2].to_string()),
                acts: Vec::new(),
                expects: Vec::new(),
                needs: Vec::new(),
                clip_ms: None,
                probe: false,
                task: None,
                milestone: None,
                destructive: false,
                remember: false,
                defaults: false,
                control: None,
                focus: Focus::default(),
                chapter: None,
            };
            for a in p[3]
                .split(';')
                .map(str::trim)
                .filter(|a| !a.is_empty() && *a != "-")
            {
                s.acts.push(parse_act(a).map_err(at)?);
            }
            for f in p
                .get(4)
                .unwrap_or(&"")
                .split(';')
                .map(str::trim)
                .filter(|f| !f.is_empty() && *f != "-")
            {
                let (k, v) = f.split_once(' ').unwrap_or((f, ""));
                match k {
                    "view" => s.expects.push(Expect::View(v.to_string())),
                    "sound" => s.expects.push(Expect::Sound),
                    "silent" => s.expects.push(Expect::Silent),
                    "change" => s.expects.push(Expect::Change),
                    "popup" => s.expects.push(Expect::Popup),
                    "toast" => s.expects.push(Expect::Toast),
                    "text" => s.expects.push(Expect::Text(v.to_uppercase())),
                    "big" => s.expects.push(Expect::BigText(v.to_uppercase())),
                    "recalled" => s.expects.push(Expect::Recalled),
                    "absent" => s.expects.push(Expect::Absent(v.to_uppercase())),
                    "brighter" => s.expects.push(Expect::Brighter),
                    "darker" => s.expects.push(Expect::Darker),
                    "louder" => s.expects.push(Expect::Louder),
                    "quieter" => s.expects.push(Expect::Quieter),
                    "do" => s.control = Some(v.to_string()),
                    "focus" => {
                        check_focus_control(v).map_err(at)?;
                        s.focus.control = Some(v.to_string());
                    }
                    "screen" => {
                        let w: Vec<u8> = v
                            .split_whitespace()
                            .map(num)
                            .collect::<Result<_, _>>()
                            .map_err(at)?;
                        let r: [u8; 4] = w
                            .try_into()
                            .map_err(|_| at("screen COL ROW COLS ROWS".into()))?;
                        check_screen(r).map_err(at)?;
                        s.focus.screen = Some(r);
                    }
                    "callout" => s.focus.callout = Some(v.to_string()),
                    "chapter" => s.chapter = Some(v.to_string()),
                    "notes" | "rows" => {
                        let w: Vec<&str> = v.split_whitespace().collect();
                        if w.len() < 2 {
                            return Err(at(format!("{k} VOICE ENTRY ...")));
                        }
                        let voice: u8 = num(w[0]).map_err(at)?;
                        if !(1..=3).contains(&voice) {
                            return Err(at(format!("voice {voice} is not 1..3")));
                        }
                        let entry: u8 = num(w[1]).map_err(at)?;
                        if k == "notes" {
                            let n = w[2..]
                                .iter()
                                .map(|x| gt_note(x))
                                .collect::<Result<_, _>>()
                                .map_err(at)?;
                            s.expects.push(Expect::Notes(voice, entry, n));
                        } else {
                            let r = w[2..]
                                .iter()
                                .map(|x| num(x))
                                .collect::<Result<_, _>>()
                                .map_err(at)?;
                            s.expects.push(Expect::Rows(voice, entry, r));
                        }
                    }
                    "value" => {
                        let w: Vec<&str> = v.split_whitespace().collect();
                        if w.len() != 3 {
                            return Err(at("value NAME MIN MAX".into()));
                        }
                        s.expects.push(Expect::Value(
                            w[0].to_string(),
                            num(w[1]).map_err(at)?,
                            num(w[2]).map_err(at)?,
                        ));
                    }
                    "level" => {
                        let w: Vec<&str> = v.split_whitespace().collect();
                        if w.len() != 2 {
                            return Err(at("level MIN_DBFS MAX_DBFS".into()));
                        }
                        s.expects.push(Expect::Level(
                            num(w[0]).map_err(at)?,
                            num(w[1]).map_err(at)?,
                        ));
                    }
                    "clip" => s.clip_ms = Some(num(v).map_err(at)?),
                    "probe" => s.probe = true,
                    "task" => s.task = Some(v.to_string()),
                    "milestone" => s.milestone = Some(v.to_string()),
                    "destructive" => s.destructive = true,
                    "remember" => s.remember = true,
                    "defaults" => s.defaults = true,
                    "setup" => s.setup = true,
                    "needs" => {
                        if !FEATURES.contains(&v) {
                            return Err(at(format!("unknown feature {v} (known: {FEATURES:?})")));
                        }
                        s.needs.push(v.to_string());
                    }
                    k => return Err(at(format!("unknown flag {k}"))),
                }
            }
            j.steps.push(s);
        } else {
            return Err(at(format!("unrecognised line: {line}")));
        }
    }
    resolve_after(&mut out)?;
    Ok(out)
}

pub fn effort_of(a: &Act) -> Effort {
    match a {
        Act::Press(_) => Effort::Press,
        Act::Hold(..) => Effort::Hold,
        Act::Turn(_, n) | Act::Sweep(_, n, _) => Effort::Turn(n.unsigned_abs()),
        Act::HoldTurn(_, _, n) | Act::Hold2Turn(_, _, _, n) => Effort::HoldTurn(n.unsigned_abs()),
        Act::HoldKey(..) | Act::HoldPress(..) => Effort::HoldKey,
        Act::Note(..) | Act::Down(_) | Act::Play(..) | Act::Taps(..) => Effort::Note,
        Act::Pot(_) => Effort::Turn(1),
        Act::Midi(_) | Act::Clock(..) => Effort::Midi,
        Act::SelectItem(_) => Effort::Turn(0),
        Act::Up(_) | Act::Wait(_) | Act::Boot | Act::PowerCycle | Act::Sync => Effort::Wait,
    }
}

/// Which optional features this firmware has: `name -> reason` (empty reason: present).
pub fn detect_features(build: &Path) -> BTreeMap<String, (bool, String)> {
    let d = Dev::power_on(build);
    let mut f: BTreeMap<String, (bool, String)> = BTreeMap::new();
    let has_str = |s: &str| d.image_has(s.as_bytes());
    f.insert(
        "theme".into(),
        (has_str("THEME"), "no THEME item in the firmware".into()),
    );
    f.insert(
        "undo".into(),
        (has_str("UNDO"), "no UNDO in the firmware".into()),
    );
    f.insert(
        "help".into(),
        (has_str("HELP"), "no help card in the firmware".into()),
    );
    f.insert(
        "order_view".into(),
        (
            d.has_symbol("order_view_view"),
            "SEQ steps TRACKER only: no ORDER view".into(),
        ),
    );
    f.insert(
        "instr_view".into(),
        (d.has_symbol("instr_view_view"), "no INSTR view".into()),
    );
    f.insert(
        "new_song".into(),
        (
            has_str("NEW SONG") || has_str("CLEAR SONG"),
            "no way to start an empty song".into(),
        ),
    );
    f.insert(
        "song_save".into(),
        (
            has_str("SAVE SONG") || has_str("SONG SAVED"),
            "SAVE keeps settings only: songs are not saved".into(),
        ),
    );
    let edit = probe_pattern_edit(build);
    f.insert(
        "pattern_edit".into(),
        (
            edit,
            "REC and the keys leave the TRACKER's pattern unchanged".into(),
        ),
    );
    for k in FEATURES {
        f.entry(k.to_string()).or_insert((false, "unknown".into()));
    }
    f
}

fn tracker_text(d: &mut Dev) -> String {
    let s = d.shot();
    text_runs(&s.cells)
        .iter()
        .map(|r| r.text.clone())
        .collect::<Vec<_>>()
        .join("|")
}

/// Does REC plus a key change what TRACKER shows? (Behaviour, not a symbol: it follows main.)
fn probe_pattern_edit(build: &Path) -> bool {
    let mut d = Dev::boot(build);
    d.press(panel::SEQ); // STEP, then TRACKER
    d.press(panel::SEQ);
    d.settle();
    if d.view() != "tracker" {
        return false;
    }
    let before = tracker_text(&mut d);
    d.press(panel::REC);
    d.note(key_of_note(69), 300);
    d.press(panel::REC);
    d.settle();
    // The REC mark itself changes the header; compare only the pattern rows.
    let rows = |t: &str| {
        t.split('|')
            .filter(|r| {
                r.len() > 4
                    && r.as_bytes()[2] == b' '
                    && r.as_bytes()[..2].iter().all(u8::is_ascii_hexdigit)
            })
            .collect::<Vec<_>>()
            .join("|")
    };
    let after = tracker_text(&mut d);
    if std::env::var_os("ERGO_DEBUG").is_some() {
        eprintln!("before {}\nafter {}", rows(&before), rows(&after));
    }
    rows(&before) != rows(&after)
}

/// The first grid row of the menu's items and the rows each takes (app/views.c MENU_ROW, and
/// the 2x lines every 3 rows).
const MENU_FIRST_ROW: usize = 4;
const MENU_LINE_ROWS: usize = 3;

/// The text of the menu's selected line. Which line is the firmware's own cursor and first
/// visible item (`Dev::menu_cursor`), never the colours: a theme can colour a value like the bar.
pub fn menu_selected(shot: &Shot, cursor: u8, top: u8) -> Option<String> {
    let row = MENU_FIRST_ROW + MENU_LINE_ROWS * usize::from(cursor.checked_sub(top)?);
    let mut runs: Vec<_> = text_runs(&shot.cells)
        .into_iter()
        .filter(|r| r.scale >= 2 && r.row == row)
        .collect();
    runs.sort_by_key(|r| r.col);
    let text = runs
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then_some(text)
}

/// The menu pages `select-item` works on: the main menu and its SETTINGS page.
fn is_menu(view: &str) -> bool {
    view == "menu" || view == "settings"
}

/// Whether the menu's selected line, as drawn, starts with `text`; the screen may trail the
/// cursor by a frame or two, so it gets a few before a no.
fn menu_shows(d: &mut Dev, text: &str) -> bool {
    for _ in 0..4 {
        let (cursor, top) = d.menu_cursor();
        let s = d.shot();
        if is_menu(&s.view) && menu_selected(&s, cursor, top).is_some_and(|t| t.starts_with(text)) {
            return true;
        }
        d.run(FRAME_MS);
    }
    false
}

/// Turns SELECT until the menu's selected line starts with `text`: forward, then back from the
/// end of the menu (it does not wrap). False if no line ever does.
fn select_menu_item(d: &mut Dev, text: &str, detents: &mut i32) -> bool {
    let mut dir = 1;
    let mut flips = 0;
    for _ in 0..=2 * u32::from(d.menu_count()) + 2 {
        if menu_shows(d, text) {
            return true;
        }
        if !is_menu(&d.view()) {
            return false;
        }
        let was = d.menu_cursor();
        d.turn(panel::SELECT, dir);
        d.run(FRAME_MS * 2);
        *detents += 1;
        if d.menu_cursor() == was {
            flips += 1;
            if flips > 1 {
                return false;
            }
            dir = -dir;
        }
    }
    false
}

/// What a step measured.
#[derive(Clone, Debug)]
pub enum Status {
    Ok,
    /// Ran, and an expectation did not hold.
    DeadEnd(String),
    /// The step did not get to the view it meant to: what followed measured another screen.
    NavFailed(String),
    NotPossible(String),
}

#[derive(Clone, Debug)]
pub struct StepResult {
    pub remember: bool,
    pub remembered_before: bool,
    pub id: String,
    pub label: String,
    pub caption: Option<String>,
    pub status: Status,
    pub effort: EffortCount,
    pub start_ms: u64,
    pub end_ms: u64,
    pub wait_ms: u64,
    pub frames_to_done: f64,
    pub view_before: String,
    pub view_after: String,
    pub depth_before: u8,
    pub depth_after: u8,
    pub max_depth: u8,
    pub two_handed: bool,
    pub note_blocked: Option<bool>,
    pub shot: Option<String>,
    pub clip: Option<String>,
    pub rms_db: f64,
    pub task: Option<String>,
    pub milestone: Option<String>,
    pub first_sound_ms: Option<u64>,
    pub changed_params: Vec<String>,
    pub confirm_seen: bool,
    pub destructive: bool,
    pub defaults: Option<Value>,
    pub expects: Vec<(String, bool)>,
    /// Each input's start and end, ms (emulated time).
    pub act_ms: Vec<(u64, u64)>,
}

pub struct JourneyResult {
    pub id: String,
    pub title: String,
    pub tour: bool,
    pub steps: Vec<StepResult>,
    pub captured: Vec<Captured>,
    /// A tour only looks: the persisted settings before and after, when they differ.
    pub settings_drift: Option<(Vec<u8>, Vec<u8>)>,
}

fn expect_name(e: &Expect) -> String {
    match e {
        Expect::View(v) => format!("view {v}"),
        Expect::Sound => "sound".into(),
        Expect::Silent => "silent".into(),
        Expect::Change => "screen change".into(),
        Expect::Popup => "popup".into(),
        Expect::Toast => "toast".into(),
        Expect::Text(t) => format!("text {t}"),
        Expect::BigText(t) => format!("text {t} at 2x or larger"),
        Expect::Recalled => "recalled parameters".into(),
        Expect::Value(n, lo, hi) => format!("{n} in {lo}..{hi}"),
        Expect::Absent(t) => format!("no text {t}"),
        Expect::Brighter => "brighter".into(),
        Expect::Darker => "darker".into(),
        Expect::Louder => "louder".into(),
        Expect::Quieter => "quieter".into(),
        Expect::Level(lo, hi) => format!("level {lo}..{hi} dBFS"),
        Expect::Notes(v, e, n) => format!(
            "voice {v} entry {e} notes {}",
            n.iter()
                .map(|x| gt_note_name(*x))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Expect::Rows(v, e, r) => format!(
            "voice {v} entry {e} notes on rows {}",
            r.iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

/// Decimates by two (averaging) for the small clips: 22 kHz keeps what the filter does.
pub fn decimate2(x: &[f32]) -> Vec<f32> {
    x.chunks(2)
        .map(|c| c.iter().sum::<f32>() / c.len() as f32)
        .collect()
}

pub fn write_clip(path: &Path, x: &[f32]) {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).expect("create the clip directory");
    }
    sidou_tests::signal_metrics::write_wav16(path, &decimate2(x), (FS / 2.0).round() as u32)
        .expect("write a clip");
}

/// Run state across steps.
#[derive(Default)]
pub struct Ctx {
    pub remembered: Option<Params>,
    /// The last view a step said it would reach.
    pub intended: Option<String>,
    /// A step did not arrive and none has since: what follows is on the wrong screen.
    pub off_course: bool,
}

/// The first 5 ms window of `x` whose RMS is above `silence_db`: its start, in ms.
pub fn first_sound_ms(x: &[f32], silence_db: f64) -> Option<f64> {
    let w = (FS * 0.005) as usize;
    x.chunks(w)
        .position(|c| c.len() == w && rms_dbfs(c) > silence_db)
        .map(|i| i as f64 * 5.0)
}

/// Runs one action; false when it was a navigation that did not arrive (a menu item not found).
fn run_act(d: &mut Dev, act: &Act, waited: &mut u64, n_detents: &mut i32) -> bool {
    match act {
        Act::Boot => {
            let t = d.wait_boot();
            *waited += t;
        }
        Act::Press(b) => d.press(*b),
        Act::Hold(b, ms) => {
            d.key_down(*b);
            d.run(*ms);
            d.key_up(*b);
            d.run(30);
        }
        Act::Down(k) => {
            d.key_down(*k);
            d.run(30);
        }
        Act::Up(k) => {
            d.key_up(*k);
            d.run(30);
        }
        Act::Turn(e, n) => d.turn(*e, *n),
        Act::HoldTurn(b, e, n) => {
            d.key_down(*b);
            d.run(30);
            d.turn(*e, *n);
            d.key_up(*b);
            d.run(30);
        }
        Act::Hold2Turn(b, b2, e, n) => {
            d.key_down(*b);
            d.run(60);
            d.key_down(*b2);
            d.run(150);
            d.turn(*e, *n);
            d.run(60);
            d.key_up(*b2);
            d.key_up(*b);
            d.run(30);
        }
        Act::HoldKey(b, note) => {
            d.key_down(*b);
            d.run(30);
            d.note(key_of_note(*note), 150);
            d.key_up(*b);
            d.run(30);
        }
        Act::HoldPress(b, other) => {
            d.key_down(*b);
            d.run(30);
            d.press(*other);
            d.key_up(*b);
            d.run(30);
        }
        Act::Note(notes, ms) => {
            for n in notes {
                d.key_down(key_of_note(*n));
            }
            d.run(*ms);
            *waited += *ms;
            for n in notes {
                d.key_up(key_of_note(*n));
            }
            d.run(30);
        }
        Act::SelectItem(text) => {
            *n_detents = 0;
            return select_menu_item(d, text, n_detents);
        }
        Act::Midi(b) => {
            d.m.midi(b);
            d.run(5 + b.len() as u64);
        }
        Act::Pot(v) => {
            d.m.pot(*v);
            d.run(60);
        }
        Act::Wait(ms) => {
            d.run(*ms);
            *waited += *ms;
        }
        Act::Clock(bpm, ms, start) => {
            if *start {
                d.m.midi(&[0xfa]);
            }
            *waited += *ms;
            let period = 60_000.0 / (f64::from(*bpm) * 24.0);
            let mut t = 0.0;
            let mut sent = 0.0;
            while t < *ms as f64 {
                d.m.midi(&[0xf8]);
                sent += period;
                let step = sent.floor() as u64;
                sent -= step as f64;
                d.run(step.max(1));
                t += period;
            }
        }
        Act::PowerCycle => {
            d.power_cycle();
            *waited += 0;
        }
        Act::Play(bpm, events) => {
            let t0 = d.ms();
            for ((notes, _), (on, off)) in events.iter().zip(play_times(*bpm, events)) {
                d.run((t0 + on).saturating_sub(d.ms()));
                for n in notes {
                    d.key_down(key_of_note(*n));
                }
                d.run((t0 + off).saturating_sub(d.ms()));
                for n in notes {
                    d.key_up(key_of_note(*n));
                }
            }
            let (_, end) = play_times(*bpm, events).last().copied().unwrap_or((0, 0));
            let sixteenth = (15_000 / *bpm) as u64;
            d.run((t0 + end + sixteenth.min(40)).saturating_sub(d.ms()));
        }
        Act::Taps(notes, ms) => {
            for n in notes {
                d.key_down(key_of_note(*n));
                d.run(*ms);
                d.key_up(key_of_note(*n));
                d.run(60);
            }
        }
        Act::Sweep(e, n, ms) => {
            let t0 = d.ms();
            let k = n.unsigned_abs() as u64;
            for i in 0..k {
                d.turn(*e, n.signum());
                let next = t0 + ms * (i + 1) / k;
                d.run(next.saturating_sub(d.ms()));
            }
        }
        Act::Sync => {
            let t0 = d.ms();
            if !d.sync_to_pattern(20_000) {
                return false;
            }
            *waited += d.ms() - t0;
        }
    }
    true
}

/// Runs `;`-separated actions with no measuring (a setup).
pub fn run_script(d: &mut Dev, text: &str) {
    let (mut waited, mut det) = (0, 0);
    for a in text.split(';').map(str::trim).filter(|a| !a.is_empty()) {
        let act = parse_act(a).unwrap_or_else(|e| panic!("setup script: {e}"));
        assert!(
            run_act(d, &act, &mut waited, &mut det),
            "setup script: {a} did not arrive"
        );
    }
}

/// The depth of the view stack not counting a title card on top (it is a transient).
pub fn view_depth(view: &str, depth: u8) -> u8 {
    if view == "title_card" {
        depth.saturating_sub(1)
    } else {
        depth
    }
}

/// Spectral centroid of the last 2048 samples, and the RMS of the last 400 ms.
fn tail_stats(x: &[f32]) -> (f64, f64) {
    let n = (FS * 0.4) as usize;
    let t = &x[x.len().saturating_sub(n)..];
    let c = &x[x.len().saturating_sub(2048)..];
    (spectral_centroid(c, FS), rms_dbfs(t))
}

/// What a check sees: the screen before, where the step's audio starts, the sound before it.
pub struct CheckCtx<'a> {
    pub before: &'a Shot,
    pub audio_from: usize,
    pub pre: &'a [f32],
    pub remembered: &'a Option<Params>,
}

fn check(d: &mut Dev, e: &Expect, cx: &CheckCtx, cfg: &Config) -> bool {
    let (before, audio_from, remembered) = (cx.before, cx.audio_from, cx.remembered);
    match e {
        Expect::Value(n, lo, hi) => d
            .params()
            .named()
            .iter()
            .find(|(k, _)| k == n)
            .is_some_and(|(_, v)| (*lo..=*hi).contains(v)),
        Expect::Absent(t) => {
            let s = d.shot();
            !text_runs(&s.cells)
                .iter()
                .any(|r| r.text.to_uppercase().contains(t.as_str()))
        }
        Expect::Brighter | Expect::Darker | Expect::Louder | Expect::Quieter => {
            let (c0, r0) = tail_stats(cx.pre);
            if r0 <= cfg.silence_db {
                return false;
            }
            let a = d.audio_since(audio_from.saturating_sub((FS * 0.4) as usize));
            let (c1, r1) = tail_stats(&a);
            match e {
                Expect::Brighter => c1 > c0 * (1.0 + cfg.jnd_centroid_pct / 100.0),
                Expect::Darker => c1 < c0 / (1.0 + cfg.jnd_centroid_pct / 100.0),
                Expect::Louder => r1 > r0 + cfg.jnd_level_db,
                _ => r1 < r0 - cfg.jnd_level_db,
            }
        }
        Expect::Level(lo, hi) => {
            let a = d.audio_since(audio_from.saturating_sub((FS * 0.4) as usize));
            let (_, r) = tail_stats(&a);
            (*lo..=*hi).contains(&r)
        }
        Expect::View(v) => d.view() == *v,
        Expect::Sound | Expect::Silent => {
            let a = d.audio_since(audio_from);
            let tail = &a[a.len().saturating_sub((FS * 0.3) as usize)..];
            let loud = rms_dbfs(tail) > cfg.silence_db;
            if matches!(e, Expect::Sound) {
                loud || rms_dbfs(&a) > cfg.silence_db
            } else {
                !loud
            }
        }
        Expect::Change => d.m.soc.lcd.fb != before.fb,
        Expect::Popup => d.popup_rows() > 0,
        Expect::Toast => d.toast_on(),
        Expect::Text(t) => {
            let s = d.shot();
            text_runs(&s.cells)
                .iter()
                .any(|r| r.text.to_uppercase().contains(t.as_str()))
        }
        Expect::BigText(t) => {
            let s = d.shot();
            text_runs(&s.cells)
                .iter()
                .any(|r| r.scale >= 2 && r.text.to_uppercase().contains(t.as_str()))
        }
        Expect::Recalled => remembered.as_ref().is_some_and(|p| *p == d.params()),
        Expect::Notes(v, e, want) => d
            .pattern_notes(u32::from(*v) - 1, usize::from(*e))
            .is_some_and(|rows| rows.iter().filter(|n| **n != 0).eq(want.iter())),
        Expect::Rows(v, e, want) => d
            .pattern_notes(u32::from(*v) - 1, usize::from(*e))
            .is_some_and(|rows| {
                rows.iter()
                    .enumerate()
                    .filter(|(_, n)| **n != 0)
                    .map(|(i, _)| i)
                    .eq(want.iter().map(|r| usize::from(*r)))
            }),
    }
}

/// Runs one step.
#[allow(clippy::too_many_arguments)]
pub fn run_step(
    d: &mut Dev,
    s: &Step,
    features: &BTreeMap<String, (bool, String)>,
    cfg: &Config,
    ctx: &mut Ctx,
    out: &Path,
    prefix: &str,
    index: usize,
) -> (StepResult, Option<Captured>) {
    let view_before = d.view();
    let depth_before = d.depth();
    let start_ms = d.ms();
    let mut r = StepResult {
        remember: false,
        remembered_before: false,
        id: s.id.clone(),
        label: s.label.clone(),
        caption: s.caption.clone(),
        status: Status::Ok,
        effort: EffortCount::default(),
        start_ms,
        end_ms: start_ms,
        wait_ms: 0,
        frames_to_done: 0.0,
        view_before: view_before.clone(),
        view_after: view_before.clone(),
        depth_before,
        depth_after: depth_before,
        max_depth: depth_before,
        two_handed: false,
        note_blocked: None,
        shot: None,
        clip: None,
        rms_db: -120.0,
        task: s.task.clone(),
        milestone: s.milestone.clone(),
        first_sound_ms: None,
        changed_params: Vec::new(),
        confirm_seen: false,
        destructive: s.destructive,
        defaults: None,
        expects: Vec::new(),
        act_ms: Vec::new(),
    };
    for need in &s.needs {
        if let Some((false, why)) = features.get(need) {
            r.status = Status::NotPossible(format!("needs {need}: {why}"));
            return (r, None);
        }
    }
    d.mid_frame();
    let before = d.shot();
    let params_before = d.params();
    r.remembered_before = ctx.remembered.as_ref() == Some(&params_before);
    let audio_from = d.audio_mark();
    let pre = d.audio_since(audio_from.saturating_sub((FS * 0.4) as usize));
    let mut waited = 0u64;
    let mut efforts = Vec::new();
    let mut detents = 0i32;
    let mut nav_failed: Option<String> = None;
    for a in &s.acts {
        detents = 0;
        let act_start = d.ms();
        let arrived = run_act(d, a, &mut waited, &mut detents);
        r.act_ms.push((act_start, d.ms()));
        if !arrived {
            nav_failed = Some(match a {
                Act::SelectItem(text) => {
                    format!("no menu line {text} was reached, on {}", d.view())
                }
                Act::Sync => "no song played a pattern's first row within 20 s".to_string(),
                _ => unreachable!("only a menu selection or a sync can fail to arrive"),
            });
            efforts.push(Effort::Turn(detents.unsigned_abs()));
            break;
        }
        let mut e = effort_of(a);
        if matches!(a, Act::SelectItem(_)) {
            e = Effort::Turn(detents.unsigned_abs());
        }
        efforts.push(e);
        r.max_depth = r.max_depth.max(d.depth());
    }
    let _ = detents;
    if s.remember {
        ctx.remembered = Some(d.params());
        r.remember = true;
    }
    let inputs_end = d.ms();
    // Wait for the expectations (a toast or popup shows within a few frames; a wipe or a card
    // takes up to about 3 s), polling every frame.
    let cx = CheckCtx {
        before: &before,
        audio_from,
        pre: &pre,
        remembered: &ctx.remembered,
    };
    let mut met = s.expects.is_empty();
    let mut confirm_seen = false;
    let mut polls = 0;
    let mut settled_at = d.ms();
    while !met && polls < 90 {
        met = s.expects.iter().all(|e| check(d, e, &cx, cfg));
        if met {
            break;
        }
        d.run(FRAME_MS);
        polls += 1;
        r.max_depth = r.max_depth.max(d.depth());
        let sh = d.shot();
        confirm_seen |= text_runs(&sh.cells).iter().any(|t| t.text.contains('?'));
        settled_at = d.ms();
    }
    let _ = inputs_end;
    for e in &s.expects {
        let ok = check(d, e, &cx, cfg);
        r.expects.push((expect_name(e), ok));
    }
    let intended = s.expects.iter().find_map(|e| match e {
        Expect::View(v) => Some(v.clone()),
        _ => None,
    });
    if r.expects.iter().any(|(_, ok)| !ok) {
        let bad: Vec<_> = r
            .expects
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(n, _)| n.clone())
            .collect();
        r.status = if intended.is_some() && bad.iter().any(|n| n.starts_with("view ")) {
            Status::NavFailed(format!("expected {}, on {}", bad.join(", "), d.view()))
        } else {
            Status::DeadEnd(format!("expected {}", bad.join(", ")))
        };
    }
    if let Some(why) = nav_failed {
        r.status = Status::NavFailed(why);
    }
    if let Some(v) = &intended {
        ctx.intended = Some(v.clone());
        ctx.off_course = matches!(r.status, Status::NavFailed(_));
    } else if matches!(r.status, Status::NavFailed(_)) {
        ctx.off_course = true;
    } else if ctx.off_course && matches!(r.status, Status::Ok) {
        let now = d.view();
        if let Some(want) = ctx.intended.as_ref().filter(|w| **w != now) {
            r.status = Status::NavFailed(format!(
                "off course after an earlier step: on {now}, meant {want}"
            ));
        }
    }
    r.frames_to_done =
        (settled_at.saturating_sub(start_ms).saturating_sub(waited)) as f64 / FRAME_MS as f64;
    // Screenshot: with a popup or toast, as soon as it is up; else after the wipes.
    if s.expects.is_empty() || !met {
        d.run(FRAME_MS);
        d.settle();
    } else if !s
        .expects
        .iter()
        .any(|e| matches!(e, Expect::Popup | Expect::Toast))
    {
        d.settle();
    } else {
        d.mid_frame();
    }
    let shot = d.shot();
    confirm_seen |= text_runs(&shot.cells).iter().any(|t| t.text.contains('?'));
    r.confirm_seen = confirm_seen;
    r.end_ms = d.ms();
    r.view_after = shot.view.clone();
    r.depth_after = d.depth();
    r.max_depth = r.max_depth.max(r.depth_after);
    r.effort = count_effort(&efforts);
    r.wait_ms = waited;
    r.two_handed = performance_hands(s, &r.effort);
    r.changed_params = params_before.diff(&d.params());

    let all_audio = d.audio_since(audio_from);
    r.rms_db = rms_dbfs(&all_audio);
    r.first_sound_ms = first_sound_ms(&all_audio, cfg.silence_db).map(|ms| start_ms + ms as u64);
    if let Some(ms) = s.clip_ms {
        d.run(ms);
        let a = d.audio_since(audio_from);
        let from = a.len().saturating_sub((FS * 1.5) as usize);
        let name = format!("{prefix}/{index:02}-{}.wav", s.id);
        write_clip(&out.join(&name), &a[from..]);
        r.clip = Some(name);
    }
    if s.defaults {
        let a = d.audio_since(audio_from);
        let from = a.len().saturating_sub((FS * 0.5) as usize);
        r.defaults = Some(audio_stats(&a[from..], cfg));
    }
    if s.probe {
        let m0 = d.audio_mark();
        d.note(key_of_note(69), 200);
        let a = d.audio_since(m0);
        r.note_blocked = Some(rms_dbfs(&a) <= cfg.silence_db);
    }
    let path = format!("{prefix}/{index:02}-{}.png", s.id);
    write_png(&out.join(&path), &shot.fb);
    r.shot = Some(path.clone());
    let cap = Captured {
        name: format!("{prefix}/{} - {}", s.id, s.label),
        path,
        shot,
    };
    (r, Some(cap))
}

/// Level, peak, clipping, DC and brightness of a note. The level is the loudest 100 ms RMS
/// (a percussive preset has decayed by the end of the window, which is its design).
fn performance_hands(s: &Step, effort: &EffortCount) -> bool {
    !s.setup && effort.two_handed > 0
}

pub fn audio_stats(x: &[f32], cfg: &Config) -> Value {
    let clipped = x
        .iter()
        .filter(|s| f64::from(s.abs()) >= cfg.clip_level)
        .count();
    let peak = x.iter().fold(0f32, |m, s| m.max(s.abs()));
    let w = (FS * 0.1) as usize;
    let loudest = if x.len() <= w {
        rms_dbfs(x)
    } else {
        (0..=x.len() - w)
            .step_by(w / 2)
            .map(|i| rms_dbfs(&x[i..i + w]))
            .fold(-120.0, f64::max)
    };
    json!({
        "rms_dbfs": loudest,
        "peak_dbfs": db(f64::from(peak) / 32768.0),
        "clipped_samples": clipped,
        "dc": dc_fraction(x),
        "centroid_hz": spectral_centroid(x, FS),
    })
}

pub fn run_journey(
    build: &Path,
    j: &Journey,
    features: &BTreeMap<String, (bool, String)>,
    cfg: &Config,
    out: &Path,
    capture_full: bool,
) -> (JourneyResult, Option<crate::capture::Capture>) {
    // A journey starts at power-on, unless its first step boots: then the boot is the step.
    let mut songs = j.prelude_songs.clone();
    for s in &j.songs {
        if !songs.contains(s) {
            songs.push(s.clone());
        }
    }
    let mut tunes = j.prelude_tunes.clone();
    for s in &j.tunes {
        if !tunes.contains(s) {
            tunes.push(s.clone());
        }
    }
    let mut d = if songs.is_empty() && tunes.is_empty() {
        if j.poweron {
            Dev::power_on(build)
        } else {
            Dev::boot(build)
        }
    } else {
        let mut d = Dev::power_on_with(build, &songs, &tunes);
        if !j.poweron {
            crate::common::run_past_boot(&mut d.m, &d.fw);
            d.m.run_ms(300);
            d.mid_frame();
        }
        d
    };
    let mut ctx = Ctx::default();
    let mut res = JourneyResult {
        id: j.id.clone(),
        title: j.title.clone(),
        tour: j.tour,
        steps: Vec::new(),
        captured: Vec::new(),
        settings_drift: None,
    };
    // The `after` recipe's steps: where this one starts. A failure there is this one's.
    let mut prelude_ctx = Ctx::default();
    for (i, s) in j.prelude.iter().enumerate() {
        let (r, _) = run_step(
            &mut d,
            s,
            features,
            cfg,
            &mut prelude_ctx,
            out,
            &format!("journeys/{}/prelude", j.id),
            i + 1,
        );
        if !matches!(r.status, Status::Ok) {
            let mut r = r;
            r.id = format!("(after) {}", r.id);
            res.steps.push(r);
            return (res, None);
        }
    }
    let settings_start = d.settings_bytes();
    if capture_full {
        d.start_capture();
    }
    let mut marks: Vec<(u64, String)> = Vec::new();
    let mut timeline: Vec<crate::capture::StepTimes> = Vec::new();
    for (i, s) in j.steps.iter().enumerate() {
        if let Some(c) = &s.caption {
            marks.push((d.ms(), c.clone()));
        }
        let (r, cap) = run_step(
            &mut d,
            s,
            features,
            cfg,
            &mut ctx,
            out,
            &format!("journeys/{}", j.id),
            i + 1,
        );
        if let Some(c) = cap {
            res.captured.push(c);
        }
        timeline.push(crate::capture::StepTimes {
            start: r.start_ms,
            end: r.end_ms,
            acts: r.act_ms.clone(),
        });
        res.steps.push(r);
    }
    let settings_end = d.settings_bytes();
    if j.tour && settings_end != settings_start {
        res.settings_drift = Some((settings_start, settings_end));
    }
    let cap = d
        .stop_capture()
        .map(|rec| crate::capture::Capture::from(rec, &d, marks, timeline, j));
    (res, cap)
}

/// The journey-level numbers.
pub fn journey_metrics(j: &JourneyResult, cfg: &Config) -> Value {
    let mut actions = 0u32;
    let mut first_sound: Option<(u64, u32)> = None;
    let mut first_loop: Option<(u64, u32)> = None;
    let (mut switches, mut menu_trips, mut dead, mut np, mut stops, mut wait) =
        (0, 0, 0, 0, 0, 0u64);
    let mut stop_steps = Vec::new();
    for s in &j.steps {
        match &s.status {
            Status::NotPossible(_) => {
                np += 1;
                continue;
            }
            Status::DeadEnd(_) | Status::NavFailed(_) => dead += 1,
            Status::Ok => {}
        }
        actions += s.effort.actions;
        wait += s.wait_ms;
        if let (None, Some(t)) = (first_sound, s.first_sound_ms) {
            first_sound = Some((t, actions.max(1)));
        }
        if s.milestone.as_deref() == Some("first_loop")
            && first_loop.is_none()
            && matches!(s.status, Status::Ok)
        {
            first_loop = Some((s.end_ms, actions));
        }
        if s.view_before != s.view_after {
            switches += 1;
            if s.view_after == "menu" {
                menu_trips += 1;
            }
        }
        if s.two_handed || s.note_blocked == Some(true) {
            stops += 1;
            stop_steps.push(s.id.clone());
        }
    }
    let t0 = j.steps.first().map_or(0, |s| s.start_ms);
    json!({
        "steps": j.steps.len(),
        "actions": actions,
        "time_to_first_sound_ms": first_sound.map(|(t, _)| t.saturating_sub(t0)),
        "actions_to_first_sound": first_sound.map(|(_, a)| a),
        "time_to_first_loop_ms": first_loop.map(|(t, _)| t.saturating_sub(t0)),
        "actions_to_first_loop": first_loop.map(|(_, a)| a),
        "mode_switches": switches,
        "menu_trips": menu_trips,
        "dead_ends": dead,
        "not_yet_possible": np,
        "stops_playing_steps": stops,
        "stops_playing": stop_steps,
        "forced_wait_ms": wait,
        "silence_db": cfg.silence_db,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_taps_sweep_and_sync_parse() {
        let j = parse_table(
            "journey t | T\nstep a | A | - | play 120 A3:2 C4+E4 .:4; taps F3 A3 200; sweep K1 -10 500; sync | sound",
        )
        .unwrap();
        let a = &j[0].steps[0].acts;
        assert_eq!(
            a[0],
            Act::Play(120, vec![(vec![57], 2), (vec![60, 64], 1), (vec![], 4)])
        );
        assert_eq!(a[1], Act::Taps(vec![53, 57], 200));
        assert_eq!(a[2], Act::Sweep(panel::K1, -10, 500));
        assert_eq!(a[3], Act::Sync);
        assert!(parse_table("journey t | T\nstep a | A | - | play 20 A3 | sound").is_err());
        assert!(parse_table("journey t | T\nstep a | A | - | play 120 E3 | sound").is_err());
        assert!(parse_table("journey t | T\nstep a | A | - | sweep K1 0 100 | sound").is_err());
    }

    #[test]
    fn play_times_follow_the_tempo_and_let_go_before_the_next_note() {
        // At 120 BPM a 16th is 125 ms; a key is let go 31 ms before the next.
        let t = play_times(120, &[(vec![57], 1), (vec![], 2), (vec![60], 1)]);
        assert_eq!(t, vec![(0, 94), (125, 344), (375, 469)]);
    }

    #[test]
    fn focus_chapter_and_pattern_outcomes_parse() {
        let j = parse_table(concat!(
            "journey t | T\n",
            "step a | A | c | press SEQ | view step; focus SEQ; screen 0 2 30 13; callout SEQ > STEP; chapter One\n",
            "step b | B | c | wait 1 | notes 2 0 A-2 C#3; rows 3 0 0 4 8 12; rows 3 1\n",
        ))
        .unwrap();
        let s = &j[0].steps;
        assert_eq!(s[0].focus.control.as_deref(), Some("SEQ"));
        assert_eq!(s[0].focus.screen, Some([0, 2, 30, 13]));
        assert_eq!(s[0].focus.callout.as_deref(), Some("SEQ > STEP"));
        assert_eq!(s[0].chapter.as_deref(), Some("One"));
        let first = sidou_sys::GT2_FIRSTNOTE as u8;
        assert_eq!(
            s[1].expects,
            vec![
                Expect::Notes(2, 0, vec![first + 33, first + 37]),
                Expect::Rows(3, 0, vec![0, 4, 8, 12]),
                Expect::Rows(3, 1, vec![]),
            ]
        );
        for bad in [
            "focus KNOB9",
            "screen 0 0 31 1",
            "screen 0 0 0 1",
            "notes 4 0 C-3",
            "notes 1 0 H-3",
        ] {
            let t = format!("journey t | T\nstep a | A | c | wait 1 | {bad}");
            assert!(parse_table(&t).is_err(), "{bad}");
        }
        assert_eq!(gt_note_name(gt_note("D#4").unwrap()), "D#4");
    }

    #[test]
    fn after_runs_a_chain_first_and_refuses_a_circle() {
        let t = concat!(
            "journey a | A\nsongs chrome-comet\nstep a1 | A1 | c | wait 1 | sound\n",
            "journey b | B\nafter a\nstep b1 | B1 | c | wait 1 | sound\n",
            "journey c | C\nafter b\nstep c1 | C1 | c | wait 1 | sound\n",
        );
        let j = parse_table(t).unwrap();
        let c = j.iter().find(|j| j.id == "c").unwrap();
        let ids: Vec<_> = c.prelude.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a1", "b1"]);
        assert_eq!(c.prelude_songs, ["chrome-comet"]);
        assert!(c.songs.is_empty());
        let circle = "journey a | A\nafter b\nstep a1 | A | c | wait 1 | sound\njourney b | B\nafter a\nstep b1 | B | c | wait 1 | sound\n";
        assert!(parse_table(circle).unwrap_err().contains("circle"));
        assert!(
            parse_table("journey a | A\nafter nope\nstep a1 | A | c | wait 1 | sound\n").is_err()
        );
        assert!(parse_table("journey a | A\nsongs no-such-song\n").is_err());
    }

    #[test]
    fn setup_allows_two_hands_but_performance_does_not() {
        let j = parse_table("journey t | T\nstep a | A | - | holdturn SAVE K1 1 | setup").unwrap();
        let s = &j[0].steps[0];
        assert!(!performance_hands(s, &count_effort(&[Effort::HoldTurn(1)])));
        let j = parse_table("journey t | T\nstep a | A | - | holdturn EDIT K1 1").unwrap();
        assert!(performance_hands(
            &j[0].steps[0],
            &count_effort(&[Effort::HoldTurn(1)])
        ));
    }

    #[test]
    fn the_default_table_parses() {
        let j = parse_table(DEFAULT_TABLE).unwrap();
        assert!(j.len() >= 7);
        assert!(j.iter().any(|j| j.id == "jam"));
        assert!(j.iter().all(|j| !j.steps.is_empty()));
    }

    #[test]
    fn a_step_parses_actions_flags_and_captions() {
        let j = parse_table(
            "journey t | Test\n step a | Label | Turn K1 to open the filter | turn K1 +8; note A4 300 | popup; clip 400; task cutoff; needs undo\n",
        )
        .unwrap();
        let s = &j[0].steps[0];
        assert_eq!(s.caption.as_deref(), Some("Turn K1 to open the filter"));
        assert_eq!(s.acts[0], Act::Turn(panel::K1, 8));
        assert_eq!(s.acts[1], Act::Note(vec![69], 300));
        assert_eq!(s.expects, vec![Expect::Popup]);
        assert_eq!(s.clip_ms, Some(400));
        assert_eq!(s.task.as_deref(), Some("cutoff"));
        assert_eq!(s.needs, vec!["undo"]);
    }

    #[test]
    fn mistakes_in_the_table_are_refused_with_the_line() {
        for bad in [
            "step a | b | - | press HOME",
            "journey t | T\nstep a | b | - | press NOPE",
            "journey t | T\nstep a | b | - | press HOME | needs magic",
            "journey t | T\nstep a | b | - | note H9 300",
            "journey t | T\nstep a | b",
            "nonsense",
        ] {
            assert!(parse_table(bad).is_err(), "{bad}");
        }
        assert!(parse_table("journey t | T\nstep a | b | - | zap")
            .unwrap_err()
            .contains("line 2"));
    }

    /// A menu screen of 2x lines every 3 rows from row 4, as app/views.c draws it: each line
    /// `(label, value)` with its colours; the label and the value are separate runs.
    fn menu_screen(lines: &[(&str, &str, u8, u8)]) -> Shot {
        use crate::device::Cell;
        let mut cells = vec![
            Cell {
                ch: b' ',
                attr: 0x00,
                src: 0,
                ovl: false,
            };
            900
        ];
        for (k, (label, value, label_attr, value_attr)) in lines.iter().enumerate() {
            let row = 4 + 3 * k;
            let mut put = |col: usize, text: &str, attr: u8| {
                for (i, ch) in text.bytes().enumerate() {
                    cells[row * 30 + col + 2 * i] = Cell {
                        ch: crate::screen::BIG_FIRST,
                        attr,
                        src: ch,
                        ovl: false,
                    };
                }
            };
            put(1, label, *label_attr);
            put(30 - 1 - 2 * value.len(), value, *value_attr);
        }
        Shot {
            transient: false,
            fb: Vec::new(),
            cells,
            view: "menu".into(),
            ms: 0,
        }
    }

    #[test]
    fn the_selected_menu_line_is_the_cursors_whatever_the_colours() {
        // HIGH theme: a cyan value (3) above the bar (black on white, 0x10 | 0) which is not the
        // first line whose colours differ from the others'.
        let high = menu_screen(&[
            ("CHIP", "", 0x61, 0x61),
            ("TRACKER", "", 0x61, 0x61),
            ("PRE", "SAW LEAD", 0x61, 0x63),
            ("OCTAVE", "0", 0x00, 0x00),
            ("THEME", "HIGH", 0x61, 0x63),
            ("STATUS", "", 0x61, 0x61),
        ]);
        let text = |s: &Shot, cursor, top| menu_selected(s, cursor, top);
        assert_eq!(text(&high, 3, 0).as_deref(), Some("OCTAVE 0"));
        assert_eq!(text(&high, 4, 0).as_deref(), Some("THEME HIGH"));
        assert_eq!(text(&high, 5, 0).as_deref(), Some("STATUS"));
        // The menu scrolled by two: item 4 is the third line.
        assert_eq!(text(&high, 4, 2).as_deref(), Some("PRE SAW LEAD"));
        // C64 theme: light blue on blue everywhere, the bar the reverse.
        let c64 = menu_screen(&[
            ("CHIP", "", 0x6e, 0x6e),
            ("TRACKER", "", 0xe6, 0xe6),
            ("PRE", "PWM BASS", 0x6e, 0x6e),
        ]);
        assert_eq!(text(&c64, 1, 0).as_deref(), Some("TRACKER"));
        assert_eq!(text(&c64, 0, 0).as_deref(), Some("CHIP"));
        // A cursor above the first visible line is nothing.
        assert_eq!(text(&c64, 0, 1), None);
    }

    #[test]
    fn notes_are_midi_numbers_on_the_keys() {
        assert_eq!(note_number("A4"), Ok(69));
        assert_eq!(note_number("F3"), Ok(53));
        assert_eq!(note_number("G5"), Ok(79));
        assert_eq!(note_number("C#4"), Ok(61));
        assert!(note_number("C2").is_err());
    }

    #[test]
    fn effort_of_an_act_list_counts_what_the_player_does() {
        let acts = [
            Act::Press(8),
            Act::SelectItem("CHIP".into()),
            Act::Turn(1, 1),
            Act::Wait(2000),
        ];
        let e: Vec<Effort> = acts.iter().map(effort_of).collect();
        let c = count_effort(&e);
        assert_eq!(c.actions, 3);
    }
}
