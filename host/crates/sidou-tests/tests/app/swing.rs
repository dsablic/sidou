// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::song::{SngSpec, Song as Model, REST as R};
use sidou_tests::song_embed;
use sidou_tests::transport::Transport;

use crate::song_view::{Call, Song};
use crate::step_view::{
    groove, groove_turn, notes_col, open_step, pattern, reference_calls, sidou_calls, snapshot, C3,
};

const CMD_CUTOFF: u8 = 0x0c;
const CMD_FUNKTEMPO: u8 = 0x0e;
const CMD_TEMPO: u8 = 0x0f;
const ROWS: usize = 16;
const LEVELS: [&str; 4] = ["STRAIGHT", "LIGHT", "SHUFFLE", "HARD"];

fn song_at(first_command: [u8; 2]) -> Model {
    song_with(&[(0, first_command)])
}

fn song_with(commands: &[(usize, [u8; 2])]) -> Model {
    let mut p0: Vec<[u8; 4]> = (0..ROWS)
        .map(|r| [if r % 2 == 0 { C3 } else { C3 + 12 }, 1, 0, 0])
        .collect();
    for &(row, command) in commands {
        p0[row][2..].copy_from_slice(&command);
    }
    let mut spec = SngSpec::new();
    let mut ins = [0u8; 25];
    ins[..9].copy_from_slice(&[0x09, 0xa0, 1, 1, 0, 0, 0, 2, 0x09]);
    spec.instr = vec![ins];
    spec.tables = vec![
        vec![(0x41, 0x00), (0xff, 0x00)],
        vec![(0x88, 0x00), (0xff, 0x00)],
        vec![],
        vec![(0x02, 0x40), (0x03, 0x05), (0x05, 0x03)],
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

fn speed_pair(s: &Song, cell: [u8; 4]) -> Option<(u8, u8)> {
    let song = unsafe { &*s.ui.v.song.song };
    let t = GT2_STBL as usize;
    match cell[2] {
        CMD_FUNKTEMPO => {
            let row = usize::from(cell[3]) - 1;
            Some((song.ltable[t][row], song.rtable[t][row]))
        }
        0 => Some((6, 6)),
        CMD_TEMPO => Some((cell[3], cell[3])),
        _ => None,
    }
}

fn funktempo(s: &Song) -> Option<(u8, u8)> {
    speed_pair(s, pattern(s, 0)[0])
}

fn stepping(m: Model) -> Song {
    let mut s = Song::playing(Some(m));
    open_step(&mut s);
    groove(&mut s);
    s
}

type Pair = Option<(u8, u8)>;

const PAIRS: [(u8, [Pair; 3]); 6] = [
    (3, [None, Some((4, 2)), None]),
    (4, [Some((5, 3)), None, Some((6, 2))]),
    (5, [Some((6, 4)), Some((7, 3)), None]),
    (6, [Some((7, 5)), Some((8, 4)), Some((9, 3))]),
    (7, [Some((8, 6)), Some((9, 5)), Some((10, 4))]),
    (8, [Some((9, 7)), Some((11, 5)), Some((12, 4))]),
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

fn onsets(calls: &[Vec<(u8, u8)>], voice: usize) -> Vec<usize> {
    let control = 4 + 7 * voice as u8;
    let mut gate = 0u8;
    let mut out = Vec::new();
    for (k, c) in calls.iter().enumerate() {
        for &(_, value) in c.iter().filter(|w| w.0 == control) {
            if value & 1 == 1 && gate & 1 == 0 {
                out.push(k);
            }
            gate = value;
        }
    }
    out
}

fn row_starts(rows: usize, length: impl Fn(usize) -> usize) -> Vec<usize> {
    (0..rows)
        .scan(0, |at, r| {
            let start = *at;
            *at += length(r);
            Some(start)
        })
        .collect()
}

fn eighths(short: usize, long: usize, row: usize) -> usize {
    if row % 4 < 2 {
        long
    } else {
        short
    }
}

fn sixteenths(short: usize, long: usize, row: usize) -> usize {
    if row % 2 == 0 {
        long
    } else {
        short
    }
}

type Lengths = fn(usize, usize, usize) -> usize;

fn reopened(s: &Song) -> (Vec<u8>, Model) {
    let sng = snapshot(s).write(false);
    let mut back = Model::host();
    back.read(&sng, 0).unwrap();
    (sng, back)
}

fn heard_from_pass(
    sng: &[u8],
    back: &Model,
    voice: usize,
    pass_notes: usize,
    notes: usize,
) -> Vec<usize> {
    let calls = 3 * pass_notes * ROWS * 6;
    let (ours, theirs) = (sidou_calls(back, calls), reference_calls(sng, calls));
    assert_eq!(ours, theirs, "GoatTracker plays it the same");
    let heard = &onsets(&ours, voice)[pass_notes..][..notes];
    heard.iter().map(|&c| c - heard[0]).collect()
}

#[test]
fn a_shuffled_song_plays_its_even_rows_long_as_goattracker_does_and_keeps_it_in_the_sng() {
    let mut s = stepping(at_speed(6));
    let bpm = s.ui.small(0, SONG_VIEW_TEMPO_COL as usize, 4);
    assert_eq!(s.ui.small(1, SONG_VIEW_TEMPO_COL as usize, 4), "BPM ");
    assert_eq!(swing(&mut s, 2).1, "SHUFFLE");
    assert_eq!(s.ui.small(0, SONG_VIEW_TEMPO_COL as usize, 4), bpm);
    assert_eq!(s.ui.small(1, SONG_VIEW_TEMPO_COL as usize, 4), "SHUF");
    let (sng, back) = reopened(&s);
    assert_eq!(
        heard_from_pass(&sng, &back, 0, ROWS, ROWS),
        row_starts(ROWS, |r| sixteenths(4, 8, r)),
        "each sixteenth after the beat 8 frames late"
    );
    let mut reopened = stepping(back);
    assert_eq!(funktempo(&reopened), Some((8, 4)));
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

const LOOP_PATTERNS: usize = 4;
const BASS_VOICE: u32 = 1;
const SPEED_6_PAIRS: [(usize, usize); 4] = [(6, 6), (5, 7), (4, 8), (3, 9)];

fn note_rows(s: &Song, p: u32) -> Vec<usize> {
    pattern(s, p)
        .iter()
        .enumerate()
        .filter(|(_, c)| (GT2_FIRSTNOTE..=GT2_LASTNOTE).contains(&u32::from(c[0])))
        .map(|(r, _)| r)
        .collect()
}

fn first_of_voice(s: &Song, voice: u32) -> u32 {
    u32::from(unsafe { *gt2_order_data(s.ui.v.song.song, 0, voice) })
}

fn sidou_loop() -> Model {
    Model::host_from(&song_embed::sng())
}

#[test]
fn sidou_loop_plays_eighths_so_each_level_delays_its_offbeat_eighths_as_goattracker_does() {
    let mut s = stepping(sidou_loop());
    let bass = first_of_voice(&s, BASS_VOICE);
    let notes = note_rows(&s, bass);
    for (k, (short, long)) in SPEED_6_PAIRS.into_iter().enumerate() {
        if k > 0 {
            assert_eq!(swing(&mut s, 1).1, LEVELS[k]);
        }
        let (sng, back) = reopened(&s);
        let starts = row_starts(ROWS, |r| eighths(short, long, r));
        assert_eq!(
            heard_from_pass(
                &sng,
                &back,
                BASS_VOICE as usize,
                LOOP_PATTERNS * notes.len(),
                notes.len()
            ),
            notes.iter().map(|&r| starts[r]).collect::<Vec<_>>(),
            "{}: the bass's offbeat eighths {} frames after the beat",
            LEVELS[k],
            2 * long
        );
    }
}

fn voice_1_row_starts(
    t: &mut Transport,
    mut each: impl FnMut(&mut Transport),
    done: impl Fn(&Transport, &[usize]) -> bool,
) -> Vec<usize> {
    let mut starts = Vec::new();
    for _ in 0..20_000 {
        each(t);
        t.pass();
        t.block_silent();
        starts.extend(
            t.events()
                .iter()
                .filter(|e| u32::from(e.flags) & GT2_EV_ROW != 0 && e.flags >> 5 == 0)
                .map(|e| usize::from(e.frame)),
        );
        if done(t, &starts) {
            return starts;
        }
    }
    panic!("the song did not play far enough: {starts:?}");
}

fn device(song: &Model) -> Model {
    let mut d = Model::device();
    d.read(&song.write(false), 0).unwrap();
    d
}

fn row_lengths_from(song: &Model, position: u32, rows: usize) -> Vec<usize> {
    let mut song = device(song);
    let mut t = Transport::new(&mut song);
    assert!(t.play(0, position));
    let starts = voice_1_row_starts(&mut t, |_| {}, |_, starts| starts.len() > rows);
    starts.windows(2).map(|w| w[1] - w[0]).take(rows).collect()
}

#[test]
fn a_swung_song_swings_from_wherever_it_starts_playing() {
    for (song, lengths, position) in [
        (song_at([0, 0]), sixteenths as Lengths, 0),
        (sidou_loop(), eighths as Lengths, 2),
    ] {
        let mut s = stepping(song);
        assert_eq!(swing(&mut s, 3).1, "HARD");
        let played = row_lengths_from(&snapshot(&s), position, ROWS);
        assert_eq!(
            played[1..],
            (1..ROWS).map(|r| lengths(3, 9, r)).collect::<Vec<_>>(),
            "from order position {position}, past the song's first row at speed 6"
        );
    }
}

#[test]
fn straight_gives_the_song_back_and_the_playing_song_its_even_rows_at_once() {
    let original = Song::playing(Some(sidou_loop()));
    let mut s = stepping(sidou_loop());
    s.f.st.state = SONG_T_PLAYING as u8;
    assert_eq!(swing(&mut s, 3).1, "HARD");
    s.calls();
    assert_eq!(swing(&mut s, -3).1, "STRAIGHT");
    for p in (0..GT2_MAX_PATTERNS)
        .filter(|&p| unsafe { !gt2_pattern_data(original.ui.v.song.song, p).is_null() })
    {
        assert_eq!(pattern(&s, p), pattern(&original, p), "pattern {p}");
    }
    let sent: Vec<Call> = s
        .calls()
        .into_iter()
        .filter(|c| matches!(c, Call::Funktempo(..)))
        .collect();
    assert!(
        !sent.is_empty() && sent.iter().all(|c| *c == Call::Funktempo(6, 6)),
        "the player's funktempo is even at once: {sent:?}"
    );
}

#[test]
fn an_even_row_with_another_command_or_its_own_funktempo_keeps_it() {
    const OWN_FUNKTEMPO: [u8; 2] = [CMD_FUNKTEMPO, 2];
    const CUTOFF: [u8; 2] = [CMD_CUTOFF, 0x40];
    let mut s = stepping(song_with(&[(4, CUTOFF), (8, OWN_FUNKTEMPO)]));
    assert_eq!(swing(&mut s, 2).1, "SHUFFLE");
    let rows = pattern(&s, 0);
    assert_eq!(rows[4][2..], CUTOFF);
    assert_eq!(rows[8][2..], OWN_FUNKTEMPO);
    assert_eq!(speed_pair(&s, rows[6]), Some((8, 4)));
    assert_eq!(swing(&mut s, -2).1, "STRAIGHT");
    let rows = pattern(&s, 0);
    assert_eq!(
        (
            rows[4][2..].to_vec(),
            rows[6][2..].to_vec(),
            rows[8][2..].to_vec()
        ),
        (CUTOFF.to_vec(), vec![0, 0], OWN_FUNKTEMPO.to_vec())
    );
}

#[test]
fn straight_sent_to_the_playing_song_with_its_funktempo_cleared_plays_even_rows_at_once() {
    const SENT_AFTER: u32 = 2 * ROWS as u32 * 6;
    let mut song = device(&song_at([CMD_FUNKTEMPO, 2]));
    let mut t = Transport::new(&mut song);
    assert!(t.play(0, 0));
    let mut sent = None;
    let starts = voice_1_row_starts(
        &mut t,
        |t| {
            if sent.is_none() && t.calls() > SENT_AFTER {
                let plain = [C3, 1, 0, 0];
                let mut e = gt2_edit::default();
                unsafe {
                    assert_eq!(
                        gt2_set_row(song.ptr_mut(), 0, 0, plain.as_ptr(), &mut e),
                        GT2_OK
                    );
                    gt2_edit_commit(song.ptr_mut(), &e);
                }
                assert!(t.funktempo(6, 6));
                sent = Some(t.calls() as usize);
            }
        },
        |t, _| t.calls() > 2 * SENT_AFTER,
    );
    let sent = sent.expect("the funktempo sent");
    let lengths = |keep: &dyn Fn(&[usize]) -> bool| -> Vec<usize> {
        starts
            .windows(2)
            .filter(|w| keep(w))
            .map(|w| w[1] - w[0])
            .collect()
    };
    let before = lengths(&|w| w[1] < sent);
    let after = lengths(&|w| w[0] > sent + 6);
    assert!(
        before.len() > ROWS
            && before[1..]
                .windows(2)
                .all(|w| w[0] + w[1] == 8 && w[0] != w[1]),
        "{before:?}"
    );
    assert!(
        after.len() > ROWS && after.iter().all(|&l| l == 6),
        "{after:?}"
    );
}

#[test]
fn a_first_row_with_another_command_refuses_the_swing() {
    let mut busy = stepping(song_at([CMD_CUTOFF, 0x40]));
    busy.ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(busy.ui.toast().as_deref(), Some("ROW HAS A COMMAND"));
    assert_eq!(pattern(&busy, 0)[0][2..], [CMD_CUTOFF, 0x40]);
}
