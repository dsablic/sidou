// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::CStr;

use sidou_sys::*;
use sidou_tests::song::{SngSpec, Song as Model, REST as R};

use crate::song_view::{open_tracker, settle, Song};
use crate::step_view::{locks, C3};
use crate::PANEL_ENC_K2;

const C4: u8 = GT2_FIRSTNOTE as u8 + 48;
const KEY16: u8 = GT2_FIRSTNOTE as u8 + 57;
const CLOCK_PER_CALL: u32 = 256;
const FAKE_TEMPO: u32 = 6;
const ROW: u32 = FAKE_TEMPO * CLOCK_PER_CALL;
const VOICE_1: u8 = 0b001;
const VOICES_2_AND_3: u8 = 0b110;

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

fn blank_song_with_voice_1_order(voice_1: Vec<u8>) -> Model {
    let mut m = song();
    let mut spec = SngSpec::new();
    spec.orders = vec![vec![(voice_1, 0), (vec![2], 0), (vec![3], 0)]];
    spec.instr = vec![[0u8; 25], [0u8; 25]];
    spec.patterns = (0..4).map(|_| (vec![R; 16], [0xff, 0, 0, 0])).collect();
    m.read(&spec.encode(), 0).unwrap();
    m
}

fn cell(s: &Song, pattern: u32, row: usize) -> [u8; 4] {
    let d = unsafe { gt2_pattern_data(s.ui.v.song.song, pattern) };
    unsafe { std::slice::from_raw_parts(d.add(row * 4), 4) }
        .try_into()
        .unwrap()
}

fn player_at(s: &mut Song, row: u8, elapsed: u32) {
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

fn tap(s: &mut Song, k: u32) {
    s.ui.note_keys(1 << k, 1 << k);
    s.ui.note_keys(0, 0);
    s.ui.frames(6);
    settle(&mut s.ui);
}

fn playing(s: &mut Song, row: u8, elapsed: u32) {
    player_at(s, row, elapsed);
    s.ui.frame();
}

#[test]
fn quantising_is_the_nearest_row_or_the_row_the_key_falls_in() {
    let q = |mode, e| unsafe { song_view_quantise(mode, e, ROW) };
    let (near, off) = (SONG_VIEW_QUANT_NEAREST, SONG_VIEW_QUANT_OFF);
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
    let cases: [(u8, u32, usize); 7] = [
        (3, 0, 3),
        (3, ROW / 4, 3),
        (3, ROW / 2 - 1, 3),
        (3, ROW / 2, 4),
        (3, ROW * 3 / 4, 4),
        (3, ROW - 1, 4),
        (3, ROW + 20, 4),
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
    let mut s = tracker();
    s.ui.press(PANEL_BTN_REC);
    player_at(&mut s, 15, ROW * 3 / 4);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(
        cell(&s, 0, 0),
        [KEY16, 1, 0, 0],
        "round to the restart's pattern 0"
    );
}

#[test]
fn a_key_past_the_end_of_a_repeated_pattern_goes_to_its_next_pass() {
    let repeated = || {
        let m = blank_song_with_voice_1_order(vec![GT2_REPEAT as u8 + 3, 0, 1]);
        let mut s = Song::playing(Some(m));
        open_tracker(&mut s.ui);
        s.ui.press(PANEL_BTN_REC);
        player_at(&mut s, 15, ROW * 3 / 4);
        s.f.st.order_pos = [1, 0, 0];
        s
    };
    let mut s = repeated();
    s.f.st.again = VOICE_1;
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 0, 0), [KEY16, 1, 0, 0], "the repeat's next pass");
    assert_eq!(cell(&s, 1, 0), R);
    let mut s = repeated();
    s.f.st.again = VOICES_2_AND_3;
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(
        cell(&s, 1, 0),
        [KEY16, 1, 0, 0],
        "the last pass: the next entry"
    );
    assert_eq!(cell(&s, 0, 0), R);
}

#[test]
fn rec_subtracts_the_entrys_transpose_and_refuses_what_leaves_the_range() {
    let transposed = || {
        let m =
            blank_song_with_voice_1_order(vec![GT2_TRANSUP as u8 + 2, 0, GT2_TRANSDOWN as u8, 1]);
        let mut s = Song::playing(Some(m));
        open_tracker(&mut s.ui);
        s.ui.press(PANEL_BTN_REC);
        s
    };
    let mut s = transposed();
    player_at(&mut s, 3, 0);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 0, 3), [KEY16 - 2, 1, 0, 0], "less the +2");
    player_at(&mut s, 15, ROW * 3 / 4);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 1, 0), [KEY16 + 16, 1, 0, 0], "less the -16");
    let mut s = transposed();
    s.ui.rig.live.octave = 3;
    player_at(&mut s, 4, 0);
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
    player_at(&mut s, 6, 0);
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
    player_at(&mut s, 9, 0);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    settle(&mut s.ui);
    assert_eq!(s.ui.small(29, 17, 13), "ORD 01 ROW 09");
    assert_eq!(s.ui.v.song.row, 2, "the cursor kept");
    assert_eq!(s.ui.v.song.order_pos, 0);
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
    player_at(&mut s, 2, 0);
    s.f.st.order_pos = [1, 0, 0];
    s.ui.frame();
    tap(&mut s, 16);
    assert_eq!(cell(&s, 1, 2), [KEY16, 1, 0, 0]);
    assert_eq!(cell(&s, 0, 2), R);
}

#[test]
fn the_key_goes_to_the_cursors_voice() {
    let mut s = tracker();
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    s.ui.press(PANEL_BTN_REC);
    player_at(&mut s, 7, 0);
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
    s.ui.frames(8);
    settle(&mut s.ui);
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

const LAST: u8 = GT2_LASTNOTE as u8;

#[test]
fn rec_writes_up_to_g_sharp_7_and_refuses_the_three_notes_above_it() {
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
    playing(&mut s, 3, ROW - 10);
    s.ui.turn(PANEL_ENC_K1, 2);
    assert_eq!(locks(&s), vec![(0, 3, 0, (cut + 64) as u16)]);
    s.ui.turn(PANEL_ENC_K2, -1);
    player_at(&mut s, 4, 30);
    s.ui.frame();
    s.ui.turn(PANEL_ENC_K1, 1);
    s.ui.turn(PANEL_ENC_K2, 3);
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
    s.ui.turn(PANEL_ENC_K2, 1000);
    assert_eq!(locks(&s), vec![(0, 3, 0, 2047), (0, 3, 1, 15)]);
    s.ui.turn(PANEL_ENC_K1, -1000);
    assert_eq!(locks(&s)[0], (0, 3, 0, 0));
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
    s.ui.press(PANEL_BTN_PLAY);
    player_at(&mut s, 0, 0);
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

fn step() -> Song {
    let mut s = Song::playing(Some(song()));
    s.ui.press(PANEL_BTN_SEQ);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "STEP");
    s
}

#[test]
fn a_step_key_while_recording_writes_the_lanes_note_and_its_key_off() {
    let mut s = step();
    s.ui.press(PANEL_BTN_REC);
    playing(&mut s, 3, ROW * 3 / 4);
    tap(&mut s, 9);
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
    s.ui.press(PANEL_BTN_ARP);
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
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.press(PANEL_BTN_REC);
    player_at(&mut s, 4, 0);
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

#[test]
fn rec_on_live_toasts_its_state_and_counts_nothing_in() {
    let mut s = Song::playing(Some(song()));
    s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    player_at(&mut s, 3, 0);
    assert_eq!(s.ui.top(), "LIVE");
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.ui.toast().as_deref(), Some("REC ON"));
    assert_eq!(s.ui.v.song.counting, 0, "no count-in on LIVE");
    s.ui.frame();
    assert_eq!(s.ui.toast().as_deref(), Some("REC ON"));
    s.ui.press(PANEL_BTN_REC);
    assert_eq!(s.ui.toast().as_deref(), Some("REC OFF"));
    open_tracker(&mut s.ui);
    s.ui.frames(60);
    s.ui.press(PANEL_BTN_REC);
    assert_ne!(s.ui.toast().as_deref(), Some("REC ON"));
}

#[test]
fn opening_a_song_view_mid_song_starts_no_count_in() {
    let mut s = Song::playing(Some(song()));
    s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    s.ui.press(PANEL_BTN_REC);
    s.ui.frames(60);
    player_at(&mut s, 3, 0);
    s.ui.frame();
    open_tracker(&mut s.ui);
    assert_eq!(s.ui.v.song.counting, 0, "no count-in on entering");
    assert_ne!(s.ui.toast().as_deref(), Some("COUNT-IN"));
    tap(&mut s, 16);
    assert_eq!(cell(&s, 0, 3), [KEY16, 1, 0, 0], "it records at once");
    s.f.st.state = SONG_T_STOPPED as u8;
    s.ui.frame();
    player_at(&mut s, 0, 0);
    s.ui.frame();
    assert_eq!(s.ui.v.song.counting, 1);
}

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
    let mut s = tracker();
    s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    s.ui.press(PANEL_BTN_REC);
    s.ui.press(PANEL_BTN_PLAY);
    player_at(&mut s, 0, 0);
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
    let mut s = step();
    s.ui.v.song.quant = SONG_VIEW_QUANT_COUNT as u8;
    s.ui.press(PANEL_BTN_REC);
    s.ui.press(PANEL_BTN_PLAY);
    player_at(&mut s, 0, 0);
    s.f.st.now_call = 0;
    s.f.st.row_call = [0; 3];
    s.ui.frame();
    assert_eq!(s.ui.v.song.counting, 1);
    let cutoff = s.ui.live().cutoff;
    s.ui.turn(PANEL_ENC_K1, 1);
    assert_ne!(s.ui.live().cutoff, cutoff, "K1 moved LIVE's cutoff");
    assert_eq!(locks(&s), vec![]);
}

const REC_LAG_SAMPLES: u32 = 265;
const PAL_SAMPLES_PER_CALL: u32 = 880;
const LAG: u32 = REC_LAG_SAMPLES * CLOCK_PER_CALL / PAL_SAMPLES_PER_CALL;

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
    player_at(&mut s, row, elapsed);
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

#[test]
fn rec_places_a_key_where_it_was_played_the_lag_before_now() {
    let (off, near) = (SONG_VIEW_QUANT_OFF, SONG_VIEW_QUANT_NEAREST);
    for tempo in [6u8, 3] {
        let row = u32::from(tempo) * CLOCK_PER_CALL;
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

#[test]
fn before_row_0_the_lag_goes_back_to_the_entry_that_played_before_where_that_is_certain() {
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
    let m = blank_song_with_voice_1_order(vec![0, GT2_REPEAT as u8 + 3, 1]);
    assert_eq!(
        lands_in(m, off, 6, 2, 0, 10, false),
        (1, 0),
        "a repeat before the entry: row 0"
    );
    let m = blank_song_with_voice_1_order(vec![0, GT2_TRANSUP as u8 + 2, 1]);
    assert_eq!(
        lands_in(m, off, 6, 2, 0, 10, false),
        (0, 15),
        "over a transpose"
    );
}
