// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! REC while the song plays (app/song_view.c, M2d Task 6): a note key writes its note into the
//! cursor's voice at the row it was played in, quantised to the nearest row (or the row it falls
//! in), K1 and K2 become cutoff and resonance locks on the row they were turned in, in TRACKER
//! and in STEP, through the editor's cell writes, against the fake player (song_view.rs) whose
//! status says where each voice's row began and how far into the call the clock is.

use std::ffi::CStr;

use sidou_sys::*;
use sidou_tests::song::{SngSpec, Song as Model};

use crate::song_view::{open_tracker, settle, Song};

const R: [u8; 4] = [GT2_REST as u8, 0, 0, 0];
const C4: u8 = GT2_FIRSTNOTE as u8 + 48;
/// The note key 16 plays at the panel's default octave (tracker_view.rs).
const KEY16: u8 = GT2_FIRSTNOTE as u8 + 57;
/// Calls a row at tempo 6 (the fake's speed), in 256ths: a row's length in the status's clock.
const ROW: u32 = 6 * 256;

/// Voice 1 plays patterns 0 and 1 (16 rows each, a note and a command on pattern 0's row 5),
/// voice 2 pattern 2, voice 3 pattern 3; pattern 1 follows pattern 0, and the list restarts at 0.
fn song() -> Model {
    let mut p0 = vec![R; 16];
    p0[5] = [C4, 2, 4, 0x22];
    let mut s = SngSpec::new();
    s.orders = vec![vec![(vec![0, 1], 0), (vec![2], 0), (vec![3], 0)]];
    s.instr = vec![[0u8; 25], [0u8; 25]];
    s.patterns = vec![
        (p0, [0xff, 0, 0, 0]),
        (vec![R; 16], [0xff, 0, 0, 0]),
        (vec![R; 16], [0xff, 0, 0, 0]),
        (vec![R; 16], [0xff, 0, 0, 0]),
    ];
    let mut m = Model::host();
    m.read(&s.encode(), 0).unwrap();
    m
}

fn cell(s: &Song, pattern: u32, row: usize) -> [u8; 4] {
    let d = unsafe { gt2_pattern_data(s.ui.v.song.song, pattern) };
    unsafe { std::slice::from_raw_parts(d.add(row * 4), 4) }
        .try_into()
        .unwrap()
}

fn locks(s: &Song) -> Vec<(u8, u8, u8, u16)> {
    let mut out = vec![song_lock::default(); SONG_LOCKS_MAX as usize];
    let n = unsafe { song_ext_get_locks(s.ui.v.song.song, out.as_mut_ptr(), SONG_LOCKS_MAX) };
    out[..n as usize]
        .iter()
        .map(|l| (l.pattern, l.row, l.param, l.value))
        .collect()
}

/// The player at call 100 + `calls` of its play, voice 1 on row `row` since call 100 and the
/// clock `frac` 256ths on from the last call: a key now falls `calls * 256 + frac` into the row.
fn at(s: &mut Song, row: u8, elapsed: u32) {
    let st = &mut s.f.st;
    st.state = SONG_T_PLAYING as u8;
    st.order_pos = [0, 0, 0];
    st.row = [row, 0, 0];
    st.row_call = [100, 100, 100];
    st.now_call = 100 + (elapsed >> 8) as u16;
    st.now_frac = (elapsed & 255) as u8;
}

fn tracker() -> Song {
    let mut s = Song::playing(Some(song()));
    open_tracker(&mut s.ui);
    s
}

/// A key (key 16) pressed and let go, and the writes done.
fn tap(s: &mut Song, k: u32) {
    s.ui.note_keys(1 << k, 1 << k);
    s.ui.note_keys(0, 0);
    for _ in 0..6 {
        s.ui.frame();
    }
    settle(&mut s.ui);
}

/// The first play frame: the view reads the player's state.
fn playing(s: &mut Song, row: u8, elapsed: u32) {
    at(s, row, elapsed);
    s.ui.frame();
}

#[test]
fn quantising_is_the_nearest_row_or_the_row_the_key_falls_in() {
    let q = |mode, e| unsafe { song_view_quantise(mode, e, ROW) };
    let (near, off) = (SONG_VIEW_QUANT_NEAREST, SONG_VIEW_QUANT_OFF);
    // A key at each fraction of a row, in 1/8ths.
    for eighth in 0..8u32 {
        let e = eighth * ROW / 8;
        assert_eq!(q(near, e), u32::from(eighth >= 4), "{eighth}/8 nearest");
        assert_eq!(q(off, e), 0, "{eighth}/8 off");
    }
    assert_eq!(q(near, ROW / 2 - 1), 0);
    assert_eq!(q(near, ROW / 2), 1, "halfway rounds up");
    assert_eq!(q(near, ROW - 1), 1);
    assert_eq!(q(near, ROW), 1, "a row on: this row is the next");
    assert_eq!(q(near, ROW + ROW / 2 - 1), 1);
    assert_eq!(q(near, ROW + ROW / 2), 2);
    assert_eq!(q(off, ROW - 1), 0);
    assert_eq!(q(off, ROW), 1);
    assert_eq!(
        q(SONG_VIEW_QUANT_COUNT, ROW / 2),
        1,
        "the count-in quantises as nearest"
    );
    assert_eq!(
        unsafe { song_view_quantise(near, 100, 0) },
        0,
        "no tempo: no division"
    );
}

#[test]
fn a_key_while_recording_writes_its_note_at_the_nearest_row_and_sounds() {
    // (the row the player is on, how far into it, the row the note lands on)
    let cases: [(u8, u32, usize); 7] = [
        (3, 0, 3),
        (3, ROW / 4, 3),
        (3, ROW / 2 - 1, 3),
        (3, ROW / 2, 4),
        (3, ROW * 3 / 4, 4),
        (3, ROW - 1, 4),
        (3, ROW + 20, 4), // a stale status: the row has gone by, the next one is the nearest
    ];
    for (row, elapsed, lands) in cases {
        let mut s = tracker();
        s.ui.press(PANEL_BTN_REC);
        s.calls();
        playing(&mut s, row, elapsed);
        tap(&mut s, 16);
        assert_eq!(cell(&s, 0, lands), [KEY16, 1, 0, 0], "row {row} +{elapsed}");
        for r in 0..16 {
            if r != lands && r != 5 {
                assert_eq!(cell(&s, 0, r), R, "row {r} untouched");
            }
        }
        assert_eq!((s.f.cell_writes, s.f.index_swaps), (1, 0), "one cell write");
        assert!(
            s.calls().contains(&crate::song_view::Call::Jam(
                0,
                u32::from(KEY16 - GT2_FIRSTNOTE as u8),
                1
            )),
            "the key sounds"
        );
        assert_eq!(s.ui.v.song.row, 0, "the cursor stays where it was");
    }
}

#[test]
fn off_records_the_row_the_key_falls_in() {
    let mut s = tracker();
    s.ui.v.song.quant = SONG_VIEW_QUANT_OFF as u8;
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 3, ROW - 1);
    tap(&mut s, 16);
    assert_eq!(cell(&s, 0, 3), [KEY16, 1, 0, 0]);
    assert_eq!(cell(&s, 0, 4), R);
}

#[test]
fn a_recorded_note_replaces_the_note_and_keeps_the_rows_command() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 5, 10);
    tap(&mut s, 16);
    assert_eq!(
        cell(&s, 0, 5),
        [KEY16, 1, 4, 0x22],
        "the note and instrument are replaced"
    );
}

#[test]
fn a_key_past_the_end_of_the_pattern_goes_to_the_one_that_plays_next_or_round() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 15, ROW * 3 / 4);
    tap(&mut s, 16);
    assert_eq!(
        cell(&s, 1, 0),
        [KEY16, 1, 0, 0],
        "row 16 is pattern 1's row 0"
    );
    // The last entry: the restart position's pattern.
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    at(&mut s, 15, ROW * 3 / 4);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(
        cell(&s, 0, 0),
        [KEY16, 1, 0, 0],
        "round to the restart's pattern 0"
    );
}

/// Ergonomics review E04 (M2 review 3.2): orderlist `D3 00 01` (pattern 0 repeated), a key in
/// the second half of pattern 0's last row on a pass the player repeats goes to pattern 0's
/// row 0, not to pattern 1's; on the last pass it goes to pattern 1.
#[test]
fn a_key_past_the_end_of_a_repeated_pattern_goes_to_its_next_pass() {
    let repeated = || {
        let mut m = song();
        let mut spec = SngSpec::new();
        spec.orders = vec![vec![
            (vec![GT2_REPEAT as u8 + 3, 0, 1], 0),
            (vec![2], 0),
            (vec![3], 0),
        ]];
        spec.instr = vec![[0u8; 25], [0u8; 25]];
        spec.patterns = (0..4).map(|_| (vec![R; 16], [0xff, 0, 0, 0])).collect();
        m.read(&spec.encode(), 0).unwrap();
        let mut s = Song::playing(Some(m));
        open_tracker(&mut s.ui);
        s.ui.press(PANEL_BTN_REC);
        at(&mut s, 15, ROW * 3 / 4);
        s.f.st.order_pos = [1, 0, 0];
        s
    };
    let mut s = repeated();
    s.f.st.again = 1; // voice 1's pattern plays again
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 0, 0), [KEY16, 1, 0, 0], "the repeat's next pass");
    assert_eq!(cell(&s, 1, 0), R);
    let mut s = repeated();
    s.f.st.again = 0b110; // the other voices' repeats are not voice 1's
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(
        cell(&s, 1, 0),
        [KEY16, 1, 0, 0],
        "the last pass: the next entry"
    );
    assert_eq!(cell(&s, 0, 0), R);
}

/// Ergonomics review E05 (M2 review 3.3): REC writes the pitch that was heard. Voice 1's
/// orderlist is `F2 00 E0 01`: pattern 0 transposed +2, pattern 1 -16; a key records its note
/// less its entry's transpose, so the pattern plays it back as it sounded, and a note the
/// transpose takes out of C-0..G#7 is refused with NOTE OUT OF RANGE (the key still sounds).
#[test]
fn rec_subtracts_the_entrys_transpose_and_refuses_what_leaves_the_range() {
    let transposed = || {
        let mut m = song();
        let mut spec = SngSpec::new();
        spec.orders = vec![vec![
            (vec![GT2_TRANSUP as u8 + 2, 0, GT2_TRANSDOWN as u8, 1], 0),
            (vec![2], 0),
            (vec![3], 0),
        ]];
        spec.instr = vec![[0u8; 25], [0u8; 25]];
        spec.patterns = (0..4).map(|_| (vec![R; 16], [0xff, 0, 0, 0])).collect();
        m.read(&spec.encode(), 0).unwrap();
        let mut s = Song::playing(Some(m));
        open_tracker(&mut s.ui);
        s.ui.press(PANEL_BTN_REC);
        s
    };
    // Pattern 0 (+2): A-4 heard is G-4 in the pattern.
    let mut s = transposed();
    at(&mut s, 3, 0);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 0, 3), [KEY16 - 2, 1, 0, 0], "less the +2");
    // Past pattern 0's end: pattern 1's transpose (-16), not pattern 0's.
    at(&mut s, 15, ROW * 3 / 4);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 1, 0), [KEY16 + 16, 1, 0, 0], "less the -16");
    // Octave +3 on pattern 1 (-16): key 0 is F-7 (77), 93 in the pattern: past G#7.
    let mut s = transposed();
    s.ui.rig.live.octave = 3;
    at(&mut s, 4, 0);
    s.f.st.order_pos = [3, 0, 0];
    s.ui.frame();
    s.calls();
    s.ui.note_keys(1, 1);
    assert_eq!(s.ui.toast().as_deref(), Some("NOTE OUT OF RANGE"));
    s.ui.note_keys(0, 0);
    settle(&mut s.ui);
    assert_eq!(cell(&s, 1, 4), R, "nothing written");
    assert_eq!(s.f.cell_writes, 0);
    assert!(
        s.calls()
            .iter()
            .any(|c| matches!(c, crate::song_view::Call::Jam(0, 77, _))),
        "the key still sounds"
    );
}

/// Ergonomics review E03 (M2 review 5.4): while REC records, TRACKER shows the recorded voice's
/// playing pattern and follows its row, so the notes are seen landing; when recording stops
/// (the song stopped, REC on) the edit cursor is back where it was.
#[test]
fn tracker_follows_the_playing_pattern_while_recording_and_the_cursor_comes_back() {
    let mut s = tracker();
    s.ui.turn(PANEL_ENC_SELECT, 2);
    settle(&mut s.ui);
    s.ui.press(PANEL_BTN_REC);
    settle(&mut s.ui);
    assert_eq!(
        s.ui.small(29, 17, 13),
        "ORD 00 ROW 02",
        "REC on, stopped: the cursor"
    );
    // Playing pattern 1 (entry 1), row 6.
    at(&mut s, 6, 0);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    settle(&mut s.ui);
    assert_eq!(s.ui.big(0, 5, 2, 6), "PAT 01", "the playing pattern");
    assert_eq!(s.ui.small(29, 17, 13), "ORD 01 ROW 06");
    tap(&mut s, 16);
    assert_eq!(cell(&s, 1, 6), [KEY16, 1, 0, 0]);
    let j = (6 - unsafe { tracker_view_top(6, 0) }) as usize;
    let r = TRACKER_VIEW_TOP as usize + TRACKER_VIEW_ROW_CELLS as usize * j;
    assert_eq!(
        s.ui.big(r, 3, 2, 3),
        "A-4",
        "the note landed where it shows"
    );
    // The row moves on: the bar follows it.
    at(&mut s, 9, 0);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    settle(&mut s.ui);
    assert_eq!(s.ui.small(29, 17, 13), "ORD 01 ROW 09");
    assert_eq!(s.ui.v.song.row, 2, "the cursor kept");
    assert_eq!(s.ui.v.song.order_pos, 0);
    // The song stops: the edit cursor where it was.
    s.f.st.state = SONG_T_STOPPED as u8;
    s.ui.frame();
    settle(&mut s.ui);
    assert_eq!(s.ui.big(0, 5, 2, 6), "PAT 00");
    assert_eq!(s.ui.small(29, 17, 13), "ORD 00 ROW 02");
}

#[test]
fn the_row_comes_from_the_players_entry_not_the_cursors() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    at(&mut s, 2, 0);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 1, 2), [KEY16, 1, 0, 0]);
    assert_eq!(cell(&s, 0, 2), R);
}

#[test]
fn the_key_goes_to_the_cursors_voice() {
    let mut s = tracker();
    s.ui.turn(PANEL_ENC_ALGORITHM, 1); // voice 2
    s.ui.press(PANEL_BTN_REC);
    at(&mut s, 7, 0);
    s.f.st.row = [0, 7, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 2, 7), [KEY16, 1, 0, 0]);
}

#[test]
fn with_rec_off_or_the_song_stopped_nothing_is_recorded_here() {
    let mut s = tracker();
    playing(&mut s, 3, 0);
    tap(&mut s, 16);
    assert_eq!(s.f.cell_writes, 0, "REC off");
    // Stopped, REC on: the cursor's entry of Task 3.
    s.ui.press(PANEL_BTN_REC);
    s.f.st.state = SONG_T_STOPPED as u8;
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 0, 0), [KEY16, 1, 0, 0]);
    assert_eq!(s.ui.v.song.row, 1, "and the cursor advances");
}

#[test]
fn notes_in_one_pass_all_get_written_in_order() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 3, 0);
    let keys = (1u32 << 16) | (1 << 18);
    s.ui.note_keys(keys, keys);
    s.ui.note_keys(0, 0);
    for _ in 0..8 {
        s.ui.frame();
    }
    settle(&mut s.ui);
    // Both keys went to voice 1's row 3: the later key's note stays.
    assert_eq!(s.f.cell_writes, 2);
    assert_eq!(cell(&s, 0, 3)[0], KEY16 + 2);
    assert_eq!(s.ui.v.song.rec_n, 0);
}

#[test]
fn song_full_and_saving_refuse_a_recorded_note_with_a_toast() {
    for (guard, message) in [
        (SONG_EDIT_GUARD_SAVING, "SAVING"),
        (SONG_EDIT_GUARD_FULL, "SONG FULL"),
    ] {
        let mut s = tracker();
        s.f.edit_guard = guard as i32;
        s.ui.press(PANEL_BTN_REC);
        playing(&mut s, 3, 0);
        tap(&mut s, 16);
        assert_eq!(s.f.cell_writes, 0);
        assert_eq!(cell(&s, 0, 3), R);
        assert_eq!(s.ui.toast().as_deref(), Some(message));
    }
}

/// GoatTracker's last pattern note, G#7: 0xBD..0xBF are REST, KEYOFF and KEYON.
const LAST: u8 = GT2_LASTNOTE as u8;

#[test]
fn rec_writes_up_to_g_sharp_7_and_refuses_the_three_notes_above_it() {
    // Octave +3: key 15 is G#7 (92), keys 16-18 are 93-95, which would be a rest, a key off
    // and a key on in the pattern.
    let mut s = tracker();
    s.ui.rig.live.octave = 3;
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 3, 0);
    tap(&mut s, 15);
    assert_eq!(
        cell(&s, 0, 3),
        [LAST, 1, 0, 0],
        "G#7 is the last note written"
    );
    for k in 16..19 {
        let writes = s.f.cell_writes;
        playing(&mut s, 7, 0);
        s.ui.note_keys(1 << k, 1 << k);
        assert_eq!(
            s.ui.toast().as_deref(),
            Some("NOTE OUT OF RANGE"),
            "key {k}"
        );
        s.ui.note_keys(0, 0);
        settle(&mut s.ui);
        assert_eq!(s.f.cell_writes, writes, "key {k}: nothing written");
        assert_eq!(cell(&s, 0, 7), R, "key {k}");
    }
    assert_eq!(s.ui.v.song.rec_n, 0);
}

#[test]
fn step_records_its_keys_above_the_steps_up_to_g_sharp_7_only() {
    let mut s = step();
    s.ui.rig.live.octave = 3;
    s.ui.press(PANEL_BTN_REC);
    for k in 16..19 {
        playing(&mut s, 4, 0);
        s.ui.note_keys(1 << k, 1 << k);
        assert_eq!(
            s.ui.toast().as_deref(),
            Some("NOTE OUT OF RANGE"),
            "key {k}"
        );
        s.ui.note_keys(0, 0);
        settle(&mut s.ui);
        assert_eq!(cell(&s, 0, 4), R, "key {k}");
    }
    assert_eq!(s.f.cell_writes, 0);
}

#[test]
fn k1_and_k2_record_cutoff_and_resonance_locks_on_the_row_they_were_turned_in() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    let cut = unsafe { live_param_get(&*s.ui.rig.live, LIVE_PARAM_CUTOFF) };
    let res = unsafe { live_param_get(&*s.ui.rig.live, LIVE_PARAM_RESONANCE) };
    playing(&mut s, 3, ROW - 10); // nearly the next row: a lock is the row it happened in
    s.ui.turn(PANEL_ENC_K1, 2);
    assert_eq!(locks(&s), vec![(0, 3, 0, (cut + 64) as u16)]);
    s.ui.turn(PANEL_ENC_K1 + 1, -1);
    at(&mut s, 4, 30);
    s.ui.frame();
    s.ui.turn(PANEL_ENC_K1, 1);
    s.ui.turn(PANEL_ENC_K1 + 1, 3);
    assert_eq!(
        locks(&s),
        vec![
            (0, 3, 0, (cut + 64) as u16),
            (0, 3, 1, (res - 1) as u16),
            (0, 4, 0, (cut + 96) as u16),
            (0, 4, 1, (res + 2) as u16),
        ]
    );
    assert_eq!(s.f.cell_writes, 0, "locks are not cell writes");
    assert_eq!(
        unsafe { CStr::from_ptr(s.ui.v.popup.name.as_ptr()) }.to_str(),
        Ok("RESONANCE")
    );
}

#[test]
fn the_knob_values_clamp_and_a_stopped_song_keeps_k1_as_it_was() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 3, 0);
    s.ui.turn(PANEL_ENC_K1, 1000);
    s.ui.turn(PANEL_ENC_K1 + 1, 1000);
    assert_eq!(locks(&s), vec![(0, 3, 0, 2047), (0, 3, 1, 15)]);
    s.ui.turn(PANEL_ENC_K1, -1000);
    assert_eq!(locks(&s)[0], (0, 3, 0, 0));
    // Stopped: K1 is the row's instrument again (Task 3) and no lock appears.
    s.f.st.state = SONG_T_STOPPED as u8;
    s.ui.frame();
    s.ui.turn(PANEL_ENC_K1, 2);
    settle(&mut s.ui);
    assert_eq!(cell(&s, 0, 0)[1], 2);
    assert_eq!(locks(&s).len(), 2);
}

#[test]
fn a_frozen_song_refuses_a_lock_with_saving() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 3, 0);
    unsafe { (*s.ui.v.song.song.cast_mut()).frozen = 1 };
    s.ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(locks(&s), vec![]);
    assert_eq!(s.ui.toast().as_deref(), Some("SAVING"));
}

#[test]
fn a_count_in_of_one_bar_runs_before_recording_begins() {
    let mut s = tracker();
    s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    s.ui.press(PANEL_BTN_REC);
    // PLAY with REC on: the song starts, and a bar (16 rows of 6 calls) is counted in.
    s.ui.press(PANEL_BTN_PLAY);
    at(&mut s, 0, 0);
    s.f.st.now_call = 0;
    s.f.st.row_call = [0; 3];
    s.ui.frame();
    assert_eq!(s.ui.toast().as_deref(), Some("COUNT-IN"));
    assert_eq!(s.ui.v.song.count_end, 96);
    s.f.st.now_call = 90;
    s.f.st.row = [15, 0, 0];
    s.f.st.row_call = [90, 0, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(
        s.f.cell_writes, 0,
        "nothing is recorded during the count-in"
    );
    s.ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(locks(&s), vec![], "nor the knobs");
    s.f.st.now_call = 96;
    s.f.st.row = [0, 0, 0];
    s.f.st.row_call = [96, 0, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(
        cell(&s, 0, 0),
        [KEY16, 1, 0, 0],
        "and from the bar's end it records"
    );
}

#[test]
fn the_menus_rec_item_chooses_the_quantising() {
    let mut s = Song::playing(Some(song()));
    crate::views::open_setting(&mut s.ui, VIEWS_SETTINGS_REC);
    assert_eq!(
        s.ui.v.song.quant, SONG_VIEW_QUANT_NEAREST as u8,
        "the default"
    );
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(s.ui.v.song.quant, SONG_VIEW_QUANT_OFF as u8);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(s.ui.v.song.quant, SONG_VIEW_QUANT_COUNT as u8);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(
        s.ui.v.song.quant, SONG_VIEW_QUANT_COUNT as u8,
        "it stops at the end"
    );
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(s.ui.v.song.quant, SONG_VIEW_QUANT_NEAREST as u8);
}

// ---- STEP ----

fn step() -> Song {
    let mut s = Song::playing(Some(song()));
    s.ui.press(PANEL_BTN_SEQ);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "STEP");
    s
}

const C3: u8 = GT2_FIRSTNOTE as u8 + 36;

#[test]
fn a_step_key_while_recording_writes_the_lanes_note_and_its_key_off() {
    let mut s = step();
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 3, ROW * 3 / 4);
    tap(&mut s, 9); // any of the 16 pads
    assert_eq!(
        cell(&s, 0, 4),
        [C3, 1, 0, 0],
        "the lane's note at the nearest row"
    );
    assert_eq!(
        cell(&s, 0, 5),
        [C4, 2, 4, 0x22],
        "row 5 holds a note: no key off over it"
    );
    assert_eq!(cell(&s, 0, 3), R);
    assert_eq!(s.ui.v.step.held, STEP_VIEW_NONE as u8);
    // The pad's own step (9) was not toggled by its release.
    assert_eq!(cell(&s, 0, 8), R);
    let mut s = step();
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 8, 0);
    tap(&mut s, 0);
    assert_eq!(cell(&s, 0, 8), [C3, 1, 0, 0]);
    assert_eq!(
        cell(&s, 0, 9),
        [GT2_KEYOFF as u8, 0, 0, 0],
        "staccato: a key off after"
    );
    assert_eq!((s.f.cell_writes, s.f.index_swaps), (2, 0));
}

#[test]
fn a_legato_lane_records_the_note_alone() {
    let mut s = step();
    s.ui.press(PANEL_BTN_ARP); // legato
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 8, 0);
    tap(&mut s, 0);
    assert_eq!(cell(&s, 0, 8), [C3, 1, 0, 0]);
    assert_eq!(cell(&s, 0, 9), R);
    assert_eq!(s.f.cell_writes, 1);
}

#[test]
fn the_keys_above_the_steps_record_their_own_pitch_on_the_selected_lane() {
    let mut s = step();
    s.ui.turn(PANEL_ENC_SELECT, 1); // lane 2
    s.ui.press(PANEL_BTN_REC);
    at(&mut s, 4, 0);
    s.f.st.row = [0, 4, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 2, 4), [KEY16, 1, 0, 0]);
}

#[test]
fn step_with_the_song_stopped_toggles_as_before_and_needs_no_rec() {
    let mut s = step();
    s.ui.press(PANEL_BTN_REC);
    tap(&mut s, 2);
    assert_eq!(cell(&s, 0, 2), [C3, 1, 0, 0]);
}

#[test]
fn k1_and_k2_in_step_record_locks_and_a_held_step_keeps_its_own_lock() {
    let mut s = step();
    s.ui.press(PANEL_BTN_REC);
    let cut = unsafe { live_param_get(&*s.ui.rig.live, LIVE_PARAM_CUTOFF) };
    playing(&mut s, 6, 20);
    s.ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(locks(&s), vec![(0, 6, 0, (cut + 32) as u16)]);
    s.ui.frame();
    assert_eq!(
        unsafe { CStr::from_ptr(s.ui.v.popup.name.as_ptr()) }.to_str(),
        Ok("CUTOFF")
    );
}

/// Ergonomics review E23 (M2 review 5.7): REC outside the song views says what it did with a
/// toast (no REC mark shows there), and starts no count-in there, where nothing records.
#[test]
fn rec_on_live_toasts_its_state_and_counts_nothing_in() {
    let mut s = Song::playing(Some(song()));
    s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    at(&mut s, 3, 0);
    assert_eq!(s.ui.top(), "LIVE");
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.ui.toast().as_deref(), Some("REC ON"));
    assert_eq!(s.ui.v.song.counting, 0, "no count-in on LIVE");
    s.ui.frame();
    assert_eq!(s.ui.toast().as_deref(), Some("REC ON"));
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.ui.toast().as_deref(), Some("REC OFF"));
    // In a song view the header's REC says it: no toast.
    open_tracker(&mut s.ui);
    for _ in 0..60 {
        s.ui.frame();
    }
    s.ui.press(PANEL_BTN_REC);
    assert_ne!(s.ui.toast().as_deref(), Some("REC ON"));
}

/// Ergonomics review E24 (M2 review 5.8): a song view opened while the song already plays
/// (with REC on and COUNT-IN) starts no count-in: was_playing is read on entering.
#[test]
fn opening_a_song_view_mid_song_starts_no_count_in() {
    let mut s = Song::playing(Some(song()));
    s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    s.ui.press(PANEL_BTN_REC); // on LIVE, the song stopped
    for _ in 0..60 {
        s.ui.frame();
    }
    at(&mut s, 3, 0); // the song plays (started from LIVE)
    s.ui.frame();
    open_tracker(&mut s.ui);
    assert_eq!(s.ui.v.song.counting, 0, "no count-in on entering");
    assert_ne!(s.ui.toast().as_deref(), Some("COUNT-IN"));
    tap(&mut s, 16);
    assert_eq!(cell(&s, 0, 3), [KEY16, 1, 0, 0], "it records at once");
    // Stopped and played again in the view: that play counts in.
    s.f.st.state = SONG_T_STOPPED as u8;
    s.ui.frame();
    at(&mut s, 0, 0);
    s.ui.frame();
    assert_eq!(s.ui.v.song.counting, 1);
}

/// Ergonomics review E25 (M2 review 5.11): K1 while recording says the cutoff in Hz, as every
/// other view; during a count-in TRACKER's K1 does nothing and opens no popup, and STEP's acts as
/// in LIVE (its knobs do with no step held).
#[test]
fn k1_recording_shows_hz_and_a_count_in_takes_nothing() {
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 3, 0);
    s.ui.turn(PANEL_ENC_K1, 1);
    let (mut name, mut value) = (
        [0; SCREEN_POPUP_TEXT as usize],
        [0; SCREEN_POPUP_TEXT as usize],
    );
    let (mut fill, mut max) = (0, 0);
    let text = |b: &[std::ffi::c_char]| {
        unsafe { CStr::from_ptr(b.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    };
    assert_eq!(
        unsafe {
            tracker_view_popup(
                &s.ui.v.song,
                PANEL_ENC_K1,
                0,
                name.as_mut_ptr(),
                value.as_mut_ptr(),
                &mut fill,
                &mut max,
            )
        },
        1
    );
    let mut hz = [0; 9];
    unsafe {
        live_view_fc_text(
            &*s.ui.rig.live,
            u32::from(s.ui.v.song.rec_cut),
            hz.as_mut_ptr(),
        )
    };
    assert_eq!(text(&name), "CUTOFF");
    assert_eq!(text(&value), text(&hz), "in Hz");
    assert!(!text(&value).starts_with(&s.ui.v.song.rec_cut.to_string()));
    // A count-in in TRACKER: K1 records nothing, edits nothing and has no popup.
    let mut s = tracker();
    s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    s.ui.press(PANEL_BTN_REC);
    s.ui.press(PANEL_BTN_PLAY);
    at(&mut s, 0, 0);
    s.f.st.now_call = 0;
    s.f.st.row_call = [0; 3];
    s.ui.frame();
    assert_eq!(s.ui.v.song.counting, 1);
    let cutoff = s.ui.live().cutoff;
    let writes = s.f.cell_writes;
    s.ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(s.ui.live().cutoff, cutoff);
    assert_eq!((locks(&s), s.f.cell_writes), (vec![], writes));
    assert_eq!(
        unsafe {
            tracker_view_popup(
                &s.ui.v.song,
                PANEL_ENC_K1,
                0,
                name.as_mut_ptr(),
                value.as_mut_ptr(),
                &mut fill,
                &mut max,
            )
        },
        0,
        "no popup during the count-in"
    );
    // In STEP: LIVE's knob.
    let mut s = step();
    s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    s.ui.press(PANEL_BTN_REC);
    s.ui.press(PANEL_BTN_PLAY);
    at(&mut s, 0, 0);
    s.f.st.now_call = 0;
    s.f.st.row_call = [0; 3];
    s.ui.frame();
    assert_eq!(s.ui.v.song.counting, 1);
    let cutoff = s.ui.live().cutoff;
    s.ui.turn(PANEL_ENC_K1, 1);
    assert_ne!(s.ui.live().cutoff, cutoff, "K1 moved LIVE's cutoff");
    assert_eq!(locks(&s), vec![]);
}

// ---- 1.1 item 14: REC from the keys, compensated ----

/// What the device's status says a key lags (platform/fm1/main.c REC_LAG_SAMPLES, 265 samples,
/// over PAL's 880 samples a call), in 256ths of a call: 6.0 ms.
const LAG: u32 = 265 * 256 / 880;

/// The player on voice 1's `row` of orderlist entry `entry`, `elapsed` into it, at `tempo`, the
/// status with the device's lag; a key (key 16) pressed: the pattern and row it lands on.
fn lands(
    quant: u32,
    tempo: u8,
    entry: u8,
    row: u8,
    elapsed: u32,
    started_here: bool,
) -> (u32, usize) {
    lands_in(song(), quant, tempo, entry, row, elapsed, started_here)
}

fn lands_in(
    model: Model,
    quant: u32,
    tempo: u8,
    entry: u8,
    row: u8,
    elapsed: u32,
    started_here: bool,
) -> (u32, usize) {
    let mut s = Song::playing(Some(model));
    open_tracker(&mut s.ui);
    s.ui.v.song.quant = quant as u8;
    s.ui.press(PANEL_BTN_REC);
    at(&mut s, row, elapsed);
    let st = &mut s.f.st;
    st.speed = [tempo, tempo];
    st.rec_lag = LAG as u16;
    st.order_pos = [entry, 0, 0];
    if started_here {
        st.row_call = [0; 3];
        st.now_call = (elapsed >> 8) as u16;
    }
    s.ui.frame();
    tap(&mut s, 16);
    let mut found = Vec::new();
    for p in 0..4 {
        for r in 0..16 {
            if cell(&s, p, r)[0] == KEY16 {
                found.push((p, r));
            }
        }
    }
    assert_eq!(found.len(), 1, "one note recorded: {found:?}");
    found[0]
}

/// The key is placed where it was played, LAG before the status's now: in OFF, a key less than
/// LAG into a row was played in the row before, LAG or more in this one; in NEAREST the half-way
/// point moves on by LAG. At tempo 6 (a row 6 calls, 120 ms) and at tempo 3 (60 ms).
#[test]
fn rec_places_a_key_where_it_was_played_the_lag_before_now() {
    let (off, near) = (SONG_VIEW_QUANT_OFF, SONG_VIEW_QUANT_NEAREST);
    for tempo in [6u8, 3] {
        let row = u32::from(tempo) * 256;
        let cases = [
            (off, 0, 2, "OFF, the row's start: the row before"),
            (off, LAG - 1, 2, "OFF, just under the lag: the row before"),
            (off, LAG, 3, "OFF, the lag: this row"),
            (off, row - 1, 3, "OFF, the row's end: this row"),
            (off, row + LAG - 1, 3, "OFF, a stale status: this row still"),
            (near, 0, 3, "NEAREST, the row's start: this row"),
            (
                near,
                row / 2 + LAG - 1,
                3,
                "NEAREST, under half a row past the lag",
            ),
            (
                near,
                row / 2 + LAG,
                4,
                "NEAREST, half a row past the lag: the next",
            ),
            (near, row - 1, 4, "NEAREST, the row's end: the next"),
        ];
        for (quant, elapsed, want, what) in cases {
            assert_eq!(
                lands(quant, tempo, 0, 3, elapsed, false),
                (0, want),
                "tempo {tempo}: {what}"
            );
        }
    }
}

/// A key the lag puts before a pattern's row 0 goes to the last row of the entry that played
/// before it (pattern 0's row 15, its 16 rows), not into this pattern; NEAREST rounds it to row
/// 0 as before. Where what played before is not certain it stays on row 0: the first entry (the
/// play's start, or the list come round), the play started on this row, a repeat in between.
#[test]
fn before_row_0_the_lag_goes_back_to_the_entry_that_played_before() {
    let (off, near) = (SONG_VIEW_QUANT_OFF, SONG_VIEW_QUANT_NEAREST);
    for tempo in [6u8, 3] {
        assert_eq!(
            lands(off, tempo, 1, 0, LAG - 1, false),
            (0, 15),
            "tempo {tempo}"
        );
        assert_eq!(lands(off, tempo, 1, 0, LAG, false), (1, 0));
        assert_eq!(lands(near, tempo, 1, 0, 0, false), (1, 0), "NEAREST: row 0");
        assert_eq!(
            lands(off, tempo, 1, 0, 10, true),
            (1, 0),
            "the play began here"
        );
        assert_eq!(
            lands(off, tempo, 0, 0, 10, false),
            (0, 0),
            "the first entry"
        );
    }
    // Pattern 0, a repeat command, pattern 1: pattern 1's later passes follow pattern 1.
    let mut m = song();
    let mut spec = SngSpec::new();
    spec.orders = vec![vec![
        (vec![0, GT2_REPEAT as u8 + 3, 1], 0),
        (vec![2], 0),
        (vec![3], 0),
    ]];
    spec.instr = vec![[0u8; 25], [0u8; 25]];
    spec.patterns = (0..4).map(|_| (vec![R; 16], [0xff, 0, 0, 0])).collect();
    m.read(&spec.encode(), 0).unwrap();
    assert_eq!(
        lands_in(m, off, 6, 2, 0, 10, false),
        (1, 0),
        "a repeat before the entry: row 0"
    );
    // A transpose between the two is no repeat: back to pattern 0, at its own transpose.
    let mut m = song();
    let mut spec = SngSpec::new();
    spec.orders = vec![vec![
        (vec![0, GT2_TRANSUP as u8 + 2, 1], 0),
        (vec![2], 0),
        (vec![3], 0),
    ]];
    spec.instr = vec![[0u8; 25], [0u8; 25]];
    spec.patterns = (0..4).map(|_| (vec![R; 16], [0xff, 0, 0, 0])).collect();
    m.read(&spec.encode(), 0).unwrap();
    assert_eq!(
        lands_in(m, off, 6, 2, 0, 10, false),
        (0, 15),
        "over a transpose"
    );
}
