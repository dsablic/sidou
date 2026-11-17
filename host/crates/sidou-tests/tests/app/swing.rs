// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::song::{SngSpec, Song as Model, REST as R};

use crate::song_view::Song;
use crate::step_view::{
    groove, groove_turn, notes_col, open_step, pattern, reference_calls, sidou_calls, snapshot, C3,
};

const CMD_CUTOFF: u8 = 0x0c;
const CMD_FUNKTEMPO: u8 = 0x0e;
const CMD_TEMPO: u8 = 0x0f;
const ROWS: usize = 16;
const LEVELS: [&str; 4] = ["STRAIGHT", "LIGHT", "SHUFFLE", "HARD"];

fn song_at(first_command: [u8; 2]) -> Model {
    let mut p0: Vec<[u8; 4]> = (0..ROWS)
        .map(|r| [if r % 2 == 0 { C3 } else { C3 + 12 }, 1, 0, 0])
        .collect();
    p0[0][2..].copy_from_slice(&first_command);
    let mut spec = SngSpec::new();
    let mut ins = [0u8; 25];
    ins[..9].copy_from_slice(&[0x09, 0xa0, 1, 1, 0, 0, 0, 2, 0x09]);
    spec.instr = vec![ins];
    spec.tables = vec![
        vec![(0x41, 0x00), (0xff, 0x00)],
        vec![(0x88, 0x00), (0xff, 0x00)],
        vec![],
        vec![(0x02, 0x40)],
    ];
    spec.patterns = vec![p0, vec![R; ROWS], vec![R; ROWS]]
        .into_iter()
        .map(|p| (p, [0xff, 0, 0, 0]))
        .collect();
    let mut m = Model::host();
    m.read(&spec.encode(), 0).unwrap();
    m
}

fn at_speed(speed: u8) -> Model {
    song_at([CMD_TEMPO, speed])
}

fn swing(s: &mut Song, steps: i32) -> (String, String) {
    groove_turn(s, PANEL_ENC_K1, steps)
}

fn funktempo(s: &Song) -> Option<(u8, u8)> {
    let first = pattern(s, 0)[0];
    let song = unsafe { &*s.ui.v.song.song };
    let t = GT2_STBL as usize;
    match first[2] {
        CMD_FUNKTEMPO => {
            let row = usize::from(first[3]) - 1;
            Some((song.ltable[t][row], song.rtable[t][row]))
        }
        0 => Some((6, 6)),
        CMD_TEMPO => Some((first[3], first[3])),
        _ => None,
    }
}

fn stepping(m: Model) -> Song {
    let mut s = Song::playing(Some(m));
    open_step(&mut s);
    groove(&mut s);
    s
}

type Pair = Option<(u8, u8)>;

const PAIRS: [(u8, [Pair; 3]); 6] = [
    (3, [None, Some((2, 4)), None]),
    (4, [Some((3, 5)), None, Some((2, 6))]),
    (5, [Some((4, 6)), Some((3, 7)), None]),
    (6, [Some((5, 7)), Some((4, 8)), Some((3, 9))]),
    (7, [Some((6, 8)), Some((5, 9)), Some((4, 10))]),
    (8, [Some((7, 9)), Some((5, 11)), Some((4, 12))]),
];

#[test]
fn every_swing_level_at_speeds_3_to_8_writes_its_funktempo_or_is_refused_in_the_popup() {
    for (speed, pairs) in PAIRS {
        let mut s = stepping(at_speed(speed));
        let notes = notes_col(&s);
        let mut kept = (speed, speed);
        for (k, given) in pairs.iter().enumerate() {
            let level = LEVELS[k + 1];
            let (name, value) = swing(&mut s, 1);
            assert_eq!(name, "SWING");
            match given {
                Some(pair) => {
                    assert_eq!(value, level, "speed {speed}");
                    kept = *pair;
                }
                None => assert_eq!(value, format!("NO {level}"), "speed {speed}"),
            }
            let (a, b) = funktempo(&s).expect("a funktempo");
            assert_eq!(
                (a, b),
                kept,
                "speed {speed} {value}: the last level given plays"
            );
            assert_eq!(u32::from(a + b), 2 * u32::from(speed), "the average kept");
        }
        assert_eq!(notes_col(&s), notes, "the notes stay");
    }
}

fn row_lengths(calls: &[Vec<(u8, u8)>]) -> Vec<usize> {
    let mut pitch = None;
    let mut starts = Vec::new();
    for (k, c) in calls.iter().enumerate() {
        for &(reg, value) in c {
            if reg == 1 && pitch != Some(value) {
                pitch = Some(value);
                starts.push(k);
            }
        }
    }
    starts.windows(2).map(|w| w[1] - w[0]).collect()
}

#[test]
fn a_shuffled_song_plays_its_even_rows_long_as_goattracker_does_and_keeps_it_in_the_sng() {
    let mut s = stepping(at_speed(6));
    let bpm = s.ui.small(0, SONG_VIEW_TEMPO_COL as usize, 4);
    assert_eq!(s.ui.small(1, SONG_VIEW_TEMPO_COL as usize, 4), "BPM ");
    assert_eq!(swing(&mut s, 2).1, "SHUFFLE");
    assert_eq!(s.ui.small(0, SONG_VIEW_TEMPO_COL as usize, 4), bpm);
    assert_eq!(s.ui.small(1, SONG_VIEW_TEMPO_COL as usize, 4), "SHUF");
    let sng = snapshot(&s).write(false);
    let mut back = Model::host();
    back.read(&sng, 0).unwrap();
    let calls = 3 * ROWS * 6;
    let (ours, theirs) = (sidou_calls(&back, calls), reference_calls(&sng, calls));
    assert_eq!(ours, theirs, "GoatTracker plays it the same");
    let rows = row_lengths(&ours);
    assert_eq!(
        &rows[ROWS..2 * ROWS],
        [8, 4].repeat(ROWS / 2),
        "rows {rows:?}"
    );
    let mut reopened = stepping(back);
    assert_eq!(funktempo(&reopened), Some((4, 8)));
    assert_eq!(
        reopened.ui.small(1, SONG_VIEW_TEMPO_COL as usize, 4),
        "SHUF"
    );
    assert_eq!(swing(&mut reopened, -2).1, "STRAIGHT");
    assert_eq!(
        pattern(&reopened, 0)[0],
        [C3, 1, 0, 0],
        "straight at 6 is no command"
    );
}

#[test]
fn a_first_row_with_another_command_refuses_the_swing() {
    let mut busy = stepping(song_at([CMD_CUTOFF, 0x40]));
    busy.ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(busy.ui.toast().as_deref(), Some("ROW HAS A COMMAND"));
    assert_eq!(pattern(&busy, 0)[0][2..], [CMD_CUTOFF, 0x40]);
}
