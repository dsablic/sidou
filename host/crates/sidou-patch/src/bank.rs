// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::CString;

use sidou_sys::song::{SngSpec, Song};
use sidou_sys::*;

use crate::lab::{lab_name, lab_sound, layer_oscillators, Loss, BYTES};
use crate::model::{Patch, Table, TableNote, TableStep, VoiceMode, OSCILLATORS};

pub const PATCHES_PER_SONG: usize = LAB_SONG_KEPT_MAX as usize;
const INSTRUMENTS_MAX: usize = GT2_MAX_INSTR as usize;
const TABLE_ROWS_MAX: usize = GT2_TABLE_ROWS as usize;
const TABLES: usize = GT2_TABLES as usize;
const PATTERN_ROWS: usize = 32;
const KEY_OFF_ROW: usize = 24;
const KEY_C4: u8 = GT2_FIRSTNOTE as u8 + 48;
const FRAME_HZ: u32 = 50;
const DEFAULT_UPDATE_HZ: u32 = 50;
const MIDI_C0: i32 = 12;
const GATE: u8 = 0x01;
const WAVE_BITS: u8 = 0xF0;
const SILENT_WAVE: u8 = 0xE0;
const DELAY_MAX: u32 = 0x0F;
const KEEP_PITCH: u8 = 0x80;
const RELATIVE_UP_MAX: i32 = 0x5F;
const RELATIVE_DOWN_MAX: i32 = 32;
const JUMP: u8 = 0xFF;
const ABSOLUTE_LOWEST: i32 = 1;
const ABSOLUTE_HIGHEST: i32 = 0x5F;

pub struct BankSong {
    pub name: String,
    pub sng: Vec<u8>,
    pub patches: usize,
    pub lines: Vec<String>,
}

fn right_of(note: TableNote, transpose: i32) -> u8 {
    match note {
        TableNote::Absolute(n) => {
            let idx = (i32::from(n) - MIDI_C0).clamp(ABSOLUTE_LOWEST, ABSOLUTE_HIGHEST);
            KEEP_PITCH + idx as u8
        }
        TableNote::Up(k) | TableNote::Down(k) => {
            let k = i32::from(k);
            let v = transpose
                + if matches!(note, TableNote::Up(_)) {
                    k
                } else {
                    -k
                };
            let v = v.clamp(-RELATIVE_DOWN_MAX, RELATIVE_UP_MAX);
            if v >= 0 {
                v as u8
            } else {
                (i32::from(KEEP_PITCH) + v) as u8
            }
        }
    }
}

fn left_of(control: u8) -> u8 {
    if control & WAVE_BITS == 0 {
        SILENT_WAVE | control & 0x0F
    } else {
        control | GATE
    }
}

pub fn wave_rows(
    table: &Table,
    transpose: i32,
    frames_per_step: u32,
    first_row: usize,
) -> Vec<(u8, u8)> {
    let mut rows = Vec::new();
    let mut starts = Vec::new();
    for s in &table.steps {
        starts.push(rows.len());
        rows.push((left_of(s.control), right_of(s.note, transpose)));
        let mut wait = frames_per_step.saturating_sub(1);
        while wait > 0 {
            let d = wait.min(DELAY_MAX);
            rows.push((d as u8, KEEP_PITCH));
            wait -= d;
        }
    }
    let target = table
        .loop_to
        .and_then(|l| starts.get(usize::from(l)))
        .map_or(0, |&r| (first_row + r) as u8);
    rows.push((JUMP, target));
    rows
}

fn frames_per_step(patch: &Patch, n: usize) -> u32 {
    let hz = if patch.sync_speed == 0 {
        DEFAULT_UPDATE_HZ
    } else {
        u32::from(patch.sync_speed)
    };
    let speed = u32::from(patch.oscillators[n].table_speed);
    ((FRAME_HZ * speed + hz / 2) / hz).max(1)
}

fn has_table(patch: &Patch, n: usize) -> bool {
    !patch.tables[n].steps.is_empty() && patch.oscillators[n].table_speed != 0
}

fn one_oscillator(patch: &Patch, n: usize) -> Patch {
    let mut p = patch.clone();
    p.mode = VoiceMode::Poly;
    p.active = [false; OSCILLATORS];
    p.active[n] = true;
    p
}

#[derive(Clone, Default)]
struct Builder {
    instr: Vec<[u8; 25]>,
    tables: Vec<Vec<(u8, u8)>>,
    kept: Vec<(u32, lab_song_instr, [u8; BYTES])>,
    orders: [Vec<u8>; 3],
    patterns: Vec<Vec<[u8; 4]>>,
    patches: usize,
    lines: Vec<String>,
}

fn encode(sound: &[u8; BYTES], name: &str, model: u32, first: [u8; TABLES]) -> lab_song_instr {
    let c = CString::new(name).unwrap_or_default();
    let mut out = lab_song_instr::default();
    unsafe {
        lab_song_encode(
            sound.as_ptr(),
            c.as_ptr(),
            0,
            model,
            first.as_ptr(),
            &mut out,
        )
    };
    out
}

impl Builder {
    fn new() -> Self {
        Builder {
            tables: vec![Vec::new(); TABLES],
            ..Default::default()
        }
    }

    fn place(&mut self, sound: &[u8; BYTES], name: &str, model: u32) -> lab_song_instr {
        let first: [u8; TABLES] = std::array::from_fn(|t| (self.tables[t].len() + 1) as u8);
        let out = encode(sound, name, model, first);
        for t in 0..TABLES {
            let n = usize::from(out.rows[t]);
            self.tables[t].extend((0..n).map(|r| (out.left[t][r], out.right[t][r])));
        }
        self.instr.push(out.instr);
        out
    }

    fn add(&mut self, patch: &Patch, model: u32) {
        let name = lab_name(&patch.name);
        let (sound, losses) = lab_sound(patch);
        let kept = self.place(&sound, &name, model);
        let number = self.instr.len() as u32;
        self.kept.push((number, kept, sound));
        let layers = layer_oscillators(patch);
        let tabled = layers.iter().any(|&n| has_table(patch, n));
        let mut channel_instr = [0u8; 3];
        if tabled {
            for (channel, &n) in layers.iter().enumerate() {
                let (single, _) = lab_sound(&one_oscillator(patch, n));
                let _ = self.place(&single, &name, model);
                let first_row = self.tables[GT2_WTBL as usize].len() + 1;
                let osc = &patch.oscillators[n];
                let table = if has_table(patch, n) {
                    patch.tables[n].clone()
                } else {
                    Table {
                        steps: vec![TableStep {
                            control: osc.control,
                            note: TableNote::Up(0),
                        }],
                        loop_to: None,
                    }
                };
                let rows = wave_rows(
                    &table,
                    i32::from(osc.transpose),
                    frames_per_step(patch, n),
                    first_row,
                );
                self.tables[GT2_WTBL as usize].extend(rows);
                let ins = self.instr.last_mut().unwrap();
                ins[GT2_INS_WAVE as usize] = first_row as u8;
                channel_instr[channel] = self.instr.len() as u8;
            }
            for (c, &i) in channel_instr.iter().enumerate() {
                let mut rows = vec![[GT2_REST as u8, 0, 0, 0]; PATTERN_ROWS];
                if i != 0 {
                    rows[0] = [KEY_C4, i, 0, 0];
                    rows[KEY_OFF_ROW] = [GT2_KEYOFF as u8, 0, 0, 0];
                }
                self.orders[c].push(self.patterns.len() as u8);
                self.patterns.push(rows);
            }
        }
        let shown: Vec<String> = losses.iter().map(Loss::to_string).collect();
        let tail = if shown.is_empty() {
            String::new()
        } else {
            format!(": {}", shown.join(", "))
        };
        self.lines.push(format!("I{number:02} {name}{tail}"));
        self.patches += 1;
    }

    fn fits(&self) -> bool {
        self.patches <= PATCHES_PER_SONG
            && self.instr.len() <= INSTRUMENTS_MAX
            && self.tables.iter().all(|t| t.len() <= TABLE_ROWS_MAX)
            && self.patterns.len() <= GT2_MAX_PATTERNS as usize
    }

    fn finish(self, name: &str) -> Option<BankSong> {
        let mut spec = SngSpec::new();
        for (d, b) in spec.strings[0].iter_mut().zip(name.bytes()) {
            *d = b;
        }
        spec.instr = self.instr.clone();
        spec.tables = self.tables.clone();
        if !self.patterns.is_empty() {
            spec.orders = vec![self.orders.iter().map(|o| (o.clone(), 0)).collect()];
            spec.patterns = self
                .patterns
                .iter()
                .map(|p| (p.clone(), [0xff, 0, 0, 0]))
                .collect();
        }
        let mut song = Song::device();
        song.read(&spec.encode(), 0).ok()?;
        for (i, kept, sound) in &self.kept {
            if unsafe { lab_song_keep(song.ptr_mut(), *i, kept, sound.as_ptr()) } != 0 {
                return None;
            }
        }
        Some(BankSong {
            name: name.to_string(),
            sng: song.write(true),
            patches: self.patches,
            lines: self.lines,
        })
    }
}

pub fn bank_songs(patches: &[Patch], model: u32, base: &str) -> Vec<BankSong> {
    let mut songs = Vec::new();
    let mut b = Builder::new();
    for patch in patches {
        let before = b.clone();
        b.add(patch, model);
        if !b.fits() {
            let name = format!("{base} {}", songs.len() + 1);
            songs.extend(before.finish(&name));
            b = Builder::new();
            b.add(patch, model);
        }
    }
    if b.patches > 0 {
        let name = format!("{base} {}", songs.len() + 1);
        songs.extend(b.finish(&name));
    }
    songs
}
