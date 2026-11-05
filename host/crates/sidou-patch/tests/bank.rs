// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::*;
use sidou_patch::bank::{bank_songs, wave_rows, PATCHES_PER_SONG};
use sidou_patch::{parse_bank, Patch, Table, TableNote, TableStep};
use sidou_sys::song::Song;
use sidou_sys::*;

const SAW: u8 = 0x20;
const PULSE: u8 = 0x40;
const SINGLE_ALL: u8 = 0b111;
const KEY_C4: u8 = GT2_FIRSTNOTE as u8 + 48;

fn patch(name: &str, tables: &[&[u8]]) -> Patch {
    let d = Decoded::new(name)
        .set(PATCH_MODE, SINGLE_ALL)
        .osc(0, OSC_WAVE, SAW)
        .osc(1, OSC_WAVE, PULSE)
        .osc(1, OSC_PULSE_WIDTH, 64)
        .osc(2, OSC_WAVE, SAW)
        .osc(0, OSC_SUSTAIN, 15)
        .osc(1, OSC_SUSTAIN, 15)
        .osc(2, OSC_SUSTAIN, 15)
        .osc(0, OSC_TABLE_SPEED, 2)
        .osc(2, OSC_TABLE_SPEED, 2)
        .set(SYNC_SPEED, 100)
        .tables(tables);
    parse_bank(&dump(&PREFIX_LIBRARIAN, 0, &d))
        .unwrap()
        .remove(0)
}

fn step(control: u8, note: TableNote) -> TableStep {
    TableStep { control, note }
}

#[test]
fn a_table_becomes_wave_rows_with_its_notes_delays_and_loop() {
    let table = Table {
        steps: vec![
            step(0x40, TableNote::Absolute(60)),
            step(0x80, TableNote::Up(3)),
            step(0x20, TableNote::Down(12)),
            step(0x00, TableNote::Up(0)),
        ],
        loop_to: Some(1),
    };
    let frames_per_step = 3;
    let rows = wave_rows(&table, 5, frames_per_step, 10);
    assert_eq!(
        rows,
        vec![
            (0x41, 0x80 + 48),
            (0x01, 0x80),
            (0x81, 8),
            (0x01, 0x80),
            (0x21, 0x80 - 7),
            (0x01, 0x80),
            (0xE0, 5),
            (0x01, 0x80),
            (0xFF, 10 + 2),
        ],
        "C-4 absolute; +3 and -12 relative with the transpose of +5 folded in; a silent step; a jump back to step 2; \
         each step its row's frame and a delay row of 1, which GoatTracker holds for 2"
    );
    let slow = Table {
        steps: vec![step(0x20, TableNote::Up(0))],
        loop_to: None,
    };
    assert_eq!(
        wave_rows(&slow, 0, 20, 1),
        vec![(0x21, 0), (0x0F, 0x80), (0x02, 0x80), (0xFF, 0)],
        "20 frames: the row, then delays of 16 and 3 frames"
    );
    let ends = Table {
        steps: vec![step(0x20, TableNote::Up(0))],
        loop_to: None,
    };
    assert_eq!(wave_rows(&ends, 0, 1, 1), vec![(0x21, 0), (0xFF, 0)]);
}

#[test]
fn a_bank_fills_songs_of_sixteen_patches_each_pulled_back_exactly() {
    let patches: Vec<Patch> = (0..20).map(|n| patch(&format!("P{n}"), &[])).collect();
    let songs = bank_songs(&patches, SID_MODEL_6581, "SIDSTN");
    assert_eq!(songs.len(), 2);
    assert_eq!(
        (songs[0].name.as_str(), songs[1].name.as_str()),
        ("SIDSTN 1", "SIDSTN 2")
    );
    assert_eq!(songs[0].patches, PATCHES_PER_SONG);
    assert_eq!(songs[1].patches, 4);
    let mut song = Song::device();
    song.read(&songs[1].sng, 0).unwrap();
    for (i, p) in patches[16..].iter().enumerate() {
        let (want, _) = sidou_patch::lab::lab_sound(p);
        let mut rows = [0u8; 4];
        let mut got = [0u8; LAB_SOUND_BYTES as usize];
        let kept = unsafe {
            lab_song_kept(
                song.ptr(),
                i as u32 + 1,
                rows.as_mut_ptr(),
                got.as_mut_ptr(),
            )
        };
        assert_ne!(kept, 0, "instrument {} keeps its LAB sound", i + 1);
        assert_eq!(got, want, "{}", p.name);
    }
}

#[test]
fn a_table_patch_gets_table_instruments_and_a_pattern_that_plays_them_together() {
    let patches = vec![
        patch("Plain", &[]),
        patch(
            "Tabled",
            &[
                &[0x40, 0x3C, 0x20, 0xC7, 0xFF],
                &[0xFF],
                &[0x80, 0x48, 0xFE, 0x00],
            ],
        ),
    ];
    let songs = bank_songs(&patches, SID_MODEL_6581, "SIDSTN");
    assert_eq!(songs.len(), 1);
    let mut song = Song::device();
    song.read(&songs[0].sng, 0).unwrap();
    let raw = song.raw();
    let wave_start = |i: usize| {
        let wave = raw.instr[i - 1][GT2_INS_WAVE as usize] as usize;
        assert_ne!(wave, 0, "instrument {i} has a wave program");
        raw.ltable[GT2_WTBL as usize][wave - 1]
    };
    assert_eq!(wave_start(3), 0x41, "oscillator 1's table: pulse first");
    assert_eq!(wave_start(5), 0x81, "oscillator 3's table: noise first");
    let firsts: Vec<[u8; 4]> = (0..3)
        .map(|c| {
            let (orders, _) = song.order(0, c).unwrap();
            song.pattern(u32::from(orders[0])).unwrap()[0]
        })
        .collect();
    assert_eq!(
        firsts,
        [[KEY_C4, 3, 0, 0], [KEY_C4, 4, 0, 0], [KEY_C4, 5, 0, 0]],
        "each oscillator on its own channel"
    );
    assert!(
        songs[0].lines.iter().any(|l| l.contains("TABLED")),
        "{:?}",
        songs[0].lines
    );
}
