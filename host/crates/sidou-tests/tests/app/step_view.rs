// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! STEP (app/step_view.c, M2d Task 5): the layout, the 16 keys as steps, paging, the lanes' gate,
//! SET and LOCK shown and kept apart, through the views with the fake player (song_view.rs), which
//! applies an edit as the player does between blocks.

use std::ffi::CStr;

use gt2_ref::trace::trace;
use gt2_ref::Options;
use sidou_sys::*;
use sidou_tests::player::{default_options, Player};
use sidou_tests::song::{SngSpec, Song as Model};
use sidou_tests::transport::Transport;

use crate::song_view::{attr, ch, Song};

/// The song view whole on the grid, the knob's popup gone from over it, and what the view
/// holds for the editor written (a frame runs one of its actions).
fn settle(ui: &mut crate::views::Ui) {
    for _ in 0..100 {
        let st = &ui.v.step;
        if st.qn == 0 && st.follow == 0 && ui.v.editor.pending == 0 {
            break;
        }
        ui.frame();
    }
    crate::song_view::settle(ui);
    ui.popup_gone();
    crate::song_view::settle(ui);
}

const R: [u8; 4] = [GT2_REST as u8, 0, 0, 0];
const OFF: [u8; 4] = [GT2_KEYOFF as u8, 0, 0, 0];
const C3: u8 = GT2_FIRSTNOTE as u8 + 36;

fn th() -> &'static theme {
    unsafe { &*theme_get(THEME_HIGH_CONTRAST) }
}

/// Three voices with 64-row patterns 0, 1 and 2 (voice 1's with a few notes and a command),
/// pattern 3 of 128 rows on voice 3's second entry; two instruments.
fn fixed_song() -> Model {
    let mut p0 = vec![R; 64];
    p0[0] = [C3 + 7, 1, 0, 0];
    p0[4] = [C3 + 12, 2, 0, 0];
    p0[5] = [GT2_REST as u8, 0, 4, 0x22];
    p0[9] = OFF;
    let mut s = SngSpec::new();
    s.orders = vec![vec![(vec![0], 0), (vec![1], 0), (vec![2, 3], 0)]];
    s.instr = vec![[0u8; 25], [0u8; 25]];
    s.patterns = vec![
        (p0, [0xff, 0, 0, 0]),
        (vec![R; 64], [0xff, 0, 0, 0]),
        (vec![R; 64], [0xff, 0, 0, 0]),
        (vec![R; 128], [0xff, 0, 0, 0]),
    ];
    let mut m = Model::host();
    m.read(&s.encode(), 0).unwrap();
    m
}

fn step() -> Song {
    let mut s = Song::playing(Some(fixed_song()));
    open_step(&mut s);
    s
}

fn open_step(s: &mut Song) {
    s.ui.press(PANEL_BTN_SEQ);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "STEP");
}

fn pattern(s: &Song, p: u32) -> Vec<[u8; 4]> {
    let song = s.ui.v.song.song;
    let n = unsafe { gt2_pattern_rows(song, p) } as usize - 1;
    let d = unsafe { gt2_pattern_data(song, p) };
    assert!(!d.is_null());
    unsafe { std::slice::from_raw_parts(d, n * 4) }
        .as_chunks::<4>()
        .0
        .to_vec()
}

fn locks(s: &Song) -> Vec<(u8, u8, u8, u16)> {
    let mut out = vec![song_lock::default(); SONG_LOCKS_MAX as usize];
    let n = unsafe { song_ext_get_locks(s.ui.v.song.song, out.as_mut_ptr(), SONG_LOCKS_MAX) };
    out[..n as usize]
        .iter()
        .map(|l| (l.pattern, l.row, l.param, l.value))
        .collect()
}

/// A step key pressed and let go.
fn tap(s: &mut Song, k: u32) {
    s.ui.note_keys(1 << k, 1 << k);
    s.ui.note_keys(0, 0);
    settle(&mut s.ui);
}

fn hold(s: &mut Song, k: u32) {
    s.ui.note_keys(1 << k, 1 << k);
}

fn release(s: &mut Song) {
    s.ui.note_keys(0, 0);
    settle(&mut s.ui);
}

/// Cell (col, row) of a lane's step i: the pip, and the notch under it.
fn pip(s: &Song, lane: usize, i: usize) -> (u8, u8) {
    let col = 5 + i + i / 4;
    let r = 4 + 3 * lane;
    (ch(&s.ui, col, r), attr(&s.ui, col, r))
}

fn notch(s: &Song, lane: usize, i: usize) -> (u8, u8) {
    let col = 5 + i + i / 4;
    let r = 5 + 3 * lane;
    (ch(&s.ui, col, r), attr(&s.ui, col, r))
}

#[test]
fn the_song_views_start_with_step() {
    let step_view: *const view = unsafe { &step_view_view };
    let tracker: *const view = unsafe { &tracker_view_view };
    unsafe {
        assert_eq!(song_view_next(std::ptr::null()), step_view);
        assert_eq!(song_view_next(&views_menu), step_view);
        assert_eq!(song_view_next(step_view), tracker);
        assert_eq!(song_view_is(step_view), 1);
    }
    let mut s = Song::playing(Some(fixed_song()));
    let mut seen = vec![];
    for _ in 0..6 {
        s.ui.press(PANEL_BTN_SEQ);
        settle(&mut s.ui);
        seen.push(s.ui.top());
    }
    assert_eq!(seen, ["STEP", "TRACKER", "ORDER", "INSTR", "TABLE", "STEP"]);
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "LIVE");
}

#[test]
fn the_layout_is_the_header_the_pages_the_lanes_the_held_frame_and_the_pads() {
    let s = step();
    let t = th();
    // The header, as TRACKER's: V1 at 2x in its colour, its pattern, the stop square, the tempo.
    assert_eq!(s.ui.big(0, 0, 2, 2), "V1");
    assert_eq!(attr(&s.ui, 0, 0) & 15, t.voice[0]);
    assert_eq!(s.ui.big(0, 5, 2, 6), "PAT 00");
    assert_eq!(ch(&s.ui, 22, 0), FONT_SQUARE as u8);
    assert_eq!(s.ui.small(0, 26, 4), " 125");
    // The page selector and the length; the ruler.
    assert_eq!(s.ui.small(2, 0, 4), "PAGE");
    assert_eq!(s.ui.small(2, 5, 7), "1 2 3 4");
    assert_eq!(
        attr(&s.ui, 5, 2),
        (u32::from(t.hot) << 4 | u32::from(t.bg)) as u8,
        "the shown page reversed"
    );
    assert_eq!(attr(&s.ui, 7, 2) & 15, t.label);
    assert_eq!(s.ui.small(2, 21, 9), "64 STEPS ");
    assert_eq!(s.ui.small(3, 5, 4), "1   ");
    assert_eq!(s.ui.small(3, 10, 1), "5");
    assert_eq!(s.ui.small(3, 15, 1), "9");
    assert_eq!(s.ui.small(3, 20, 2), "13");
    // The lanes: V1 with notes on steps 1, 5 (a note, a command, a key off after it on 10).
    assert_eq!(s.ui.big(4, 0, 2, 2), "V1");
    assert_eq!(s.ui.big(7, 0, 2, 2), "V2");
    assert_eq!(s.ui.big(10, 0, 2, 2), "V3");
    assert_eq!(s.ui.big(13, 0, 2, 1), "B");
    assert_eq!(attr(&s.ui, 0, 7) & 15, t.voice[1]);
    assert_eq!(ch(&s.ui, 4, 4), FONT_ARROW_RIGHT as u8, "the selected lane");
    assert_eq!(ch(&s.ui, 4, 7), b' ');
    assert_eq!(pip(&s, 0, 0), (FONT_BALL as u8, t.voice[0] | t.bg << 4));
    assert_eq!(pip(&s, 0, 1), (FONT_PIP as u8, t.dim | t.bg << 4));
    assert_eq!(pip(&s, 0, 4).0, FONT_BALL as u8);
    assert_eq!(pip(&s, 0, 9).0, FONT_PIP as u8, "a key off is not a note");
    assert_eq!(pip(&s, 1, 0).0, FONT_PIP as u8);
    // A group gap of one column between the fours.
    assert_eq!(ch(&s.ui, 9, 4), b' ');
    assert_eq!(ch(&s.ui, 14, 4), b' ');
    assert_eq!(s.ui.small(6, 5, 16), "C-3 I01 STACCATO");
    assert_eq!(attr(&s.ui, 5, 6) & 15, t.value, "the selected lane's line");
    assert_eq!(attr(&s.ui, 5, 9) & 15, t.hint);
    assert_eq!(s.ui.small(15, 5, 12), "CHIP B IN M5");
    assert_eq!(attr(&s.ui, 5, 15) & 15, t.dim, "B is greyed");
    // The held step's frame: nothing held.
    assert_eq!(s.ui.small(17, 2, 14), " HELD STEP -- ");
    assert_eq!(s.ui.small(18, 2, 9), "K1 CUTOFF");
    assert_eq!(s.ui.small(18, 17, 7), "K2 RESO");
    assert_eq!(s.ui.small(20, 2, 8), "K3 PULSE");
    assert_eq!(s.ui.small(20, 17, 7), "K4 WAVE");
    assert_eq!(
        attr(&s.ui, 5, 18) & 15,
        t.hint,
        "the legend in the hint role until a step is held (dim is 2.16:1)"
    );
    assert_eq!(attr(&s.ui, 20, 18) & 15, t.hint);
    assert!(
        !s.ui.small(22, 2, 27).contains("EDIT"),
        "the frame's foot names nothing: the help card says what EDIT does"
    );
    // The pads: two rows of eight in beats of four, numbered by beat at 2x (1 2 3 4  1 2 3 4);
    // the second and fourth beats shaded, a beat's first number white, the others light grey; a
    // note's pad in the lane's colour (steps 1 and 5 have notes).
    let numbers: String = (0..16)
        .map(|i| s.ui.big(pad_row(i), pad_col(i), 2, 1))
        .collect();
    assert_eq!(numbers, "1234123412341234");
    assert_eq!(
        pad_col(3) + 2 + 3,
        pad_col(4),
        "a beat two cells more apart"
    );
    let at = |i: usize| attr(&s.ui, pad_col(i), pad_row(i));
    let a = |fg: u8, bg: u8| (u32::from(bg) << 4 | u32::from(fg)) as u8;
    assert_eq!(at(0), a(t.bg, t.voice[0]), "a note's pad");
    assert_eq!(at(4), a(t.bg, t.voice[0]));
    assert_eq!(at(1), a(t.label, t.bg));
    assert_eq!(at(5), a(t.label, t.cursor), "the second beat shaded");
    assert_eq!(at(8), a(t.value, t.bg), "a beat's first step brighter");
    assert_eq!(at(12), a(t.value, t.cursor));
    assert_eq!(at(15), a(t.label, t.cursor));
    assert_eq!(
        s.ui.small(29, 0, 30).trim(),
        "",
        "no help line: the help card says what the controls do"
    );
}

/// Step i's pad: two rows of eight from STEP_VIEW_PADS_ROW, three rows apart; a pad two cells
/// and a cell apart, a beat two cells more.
fn pad_col(i: usize) -> usize {
    let j = i % 8;
    STEP_VIEW_PAD_COL as usize + 3 * j + j / 4 * 2
}

fn pad_row(i: usize) -> usize {
    STEP_VIEW_PADS_ROW as usize + i / 8 * 3
}

const PRESETS: u32 = 6;
const PANEL_ENC_K2: u32 = PANEL_ENC_K1 + 1;
const PANEL_ENC_K3: u32 = PANEL_ENC_K1 + 2;

#[test]
fn a_step_pressed_and_let_go_toggles_a_note_with_a_key_off_after_it() {
    let mut s = step();
    tap(&mut s, 6);
    let p = pattern(&s, 0);
    assert_eq!(p[6], [C3, 1, 0, 0], "the lane's note and instrument");
    assert_eq!(p[7], OFF, "staccato: a key off on the next empty row");
    assert_eq!(pip(&s, 0, 6).0, FONT_BALL as u8);
    assert_eq!(pip(&s, 0, 6).1 & 15, th().voice[0]);
    assert_eq!(
        attr(&s.ui, pad_col(6), pad_row(6)) >> 4,
        th().voice[0],
        "the pad of a note"
    );
    tap(&mut s, 6);
    let p = pattern(&s, 0);
    assert_eq!((p[6], p[7]), (R, R), "off: a rest, and its key off goes");
    assert_eq!(pip(&s, 0, 6).0, FONT_PIP as u8);
    // An existing note goes the same way, its instrument with it.
    tap(&mut s, 4);
    assert_eq!(pattern(&s, 0)[4], R);
    // The key off of a note before it stays where the step was: a staccato lane keeps it.
    tap(&mut s, 1);
    let p = pattern(&s, 0);
    assert_eq!((p[1], p[2]), ([C3, 1, 0, 0], OFF));
    tap(&mut s, 1);
    let p = pattern(&s, 0);
    assert_eq!(p[1], OFF, "row 0's note still wants its key off");
    assert_eq!(p[2], R);
    assert_eq!(s.f.index_swaps, 0);
    assert!(s.f.cell_writes > 0, "every change is a cell write");
}

#[test]
fn a_key_off_never_goes_over_a_note_and_a_command_row_counts_as_empty() {
    let mut s = step();
    tap(&mut s, 3);
    let p = pattern(&s, 0);
    assert_eq!(p[3], [C3, 1, 0, 0]);
    assert_eq!(p[4], [C3 + 12, 2, 0, 0], "row 4's note stays");
    // Off: the key off now stands where the note was, for the note before it (none: a rest).
    tap(&mut s, 4);
    tap(&mut s, 4);
    let p = pattern(&s, 0);
    assert_eq!(p[4], [C3, 1, 0, 0]);
    assert_eq!(
        p[5],
        [GT2_KEYOFF as u8, 0, 4, 0x22],
        "the command stays, with the key off"
    );
}

#[test]
fn a_note_that_ends_the_pattern_has_no_key_off_to_write() {
    let mut s = step();
    s.ui.turn(PANEL_ENC_ALGORITHM, 3);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(3, 5, 3), "49 ");
    tap(&mut s, 15);
    let p = pattern(&s, 0);
    assert_eq!(p[63], [C3, 1, 0, 0]);
    assert_eq!(s.f.cell_writes, 1);
}

#[test]
fn a_legato_lane_writes_no_key_off_and_taking_a_note_away_leaves_the_one_before() {
    let mut s = step();
    s.ui.press(PANEL_BTN_ARP);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(6, 5, 16), "C-3 I01 LEGATO  ");
    tap(&mut s, 6);
    let p = pattern(&s, 0);
    assert_eq!((p[6], p[7]), ([C3, 1, 0, 0], R));
    tap(&mut s, 1);
    tap(&mut s, 1);
    let p = pattern(&s, 0);
    assert_eq!(
        (p[1], p[2]),
        (R, R),
        "legato: a rest, whatever the row before"
    );
    s.ui.press(PANEL_BTN_ARP);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(6, 5, 16), "C-3 I01 STACCATO");
    // The gate belongs to the lane.
    s.ui.press(PANEL_BTN_ARP);
    s.ui.turn(PANEL_ENC_SELECT, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(6, 5, 16), "C-3 I01 LEGATO  ");
    assert_eq!(s.ui.small(9, 5, 16), "C-3 I01 STACCATO");
}

#[test]
fn select_picks_the_lane_and_the_keys_write_into_its_pattern() {
    let mut s = step();
    s.ui.turn(PANEL_ENC_SELECT, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.big(0, 0, 2, 2), "V2");
    assert_eq!(s.ui.big(0, 5, 2, 6), "PAT 01");
    assert_eq!(attr(&s.ui, 0, 0) & 15, th().voice[1]);
    assert_eq!(ch(&s.ui, 4, 7), FONT_ARROW_RIGHT as u8);
    assert_eq!(ch(&s.ui, 4, 4), b' ');
    tap(&mut s, 0);
    assert_eq!(pattern(&s, 1)[0], [C3, 1, 0, 0]);
    assert_eq!(
        pattern(&s, 0)[0][0],
        C3 + 7,
        "voice 1's own pattern is untouched"
    );
    assert_eq!(pip(&s, 1, 0).0, FONT_BALL as u8);
    // Three lanes, round: B is not a lane to pick.
    s.ui.turn(PANEL_ENC_SELECT, 2);
    assert_eq!(s.ui.v.song.voice, 0);
    s.ui.turn(PANEL_ENC_SELECT, -1);
    assert_eq!(s.ui.v.song.voice, 2);
}

#[test]
fn presets_sets_the_lanes_instrument_or_with_a_step_held_the_steps_note() {
    let mut s = step();
    s.ui.turn(PRESETS, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(6, 5, 16), "C-3 I02 STACCATO");
    assert_eq!(
        s.ui.v.song.instr, 2,
        "the keys audition the lane's instrument"
    );
    tap(&mut s, 2);
    assert_eq!(pattern(&s, 0)[2], [C3, 2, 0, 0]);
    // A step held: its note, and the lane's note from then on.
    hold(&mut s, 8);
    s.ui.turn(PRESETS, 5);
    release(&mut s);
    assert_eq!(
        pattern(&s, 0)[8],
        [C3 + 5, 2, 0, 0],
        "the knob touched it: no toggle at its release"
    );
    assert_eq!(s.ui.small(6, 5, 7), "F-3 I02");
    tap(&mut s, 10);
    assert_eq!(pattern(&s, 0)[10][0], C3 + 5);
    // The note of a step with one moves from its own.
    hold(&mut s, 4);
    s.ui.turn(PRESETS, -2);
    release(&mut s);
    assert_eq!(pattern(&s, 0)[4][0], C3 + 12 - 2);
    assert_eq!(pattern(&s, 0)[4][1], 2, "its own instrument stays");
    // The instrument stays within 1..63.
    s.ui.turn(PRESETS, -100);
    assert_eq!(s.ui.v.step.instr[0], 1);
    s.ui.turn(PRESETS, 100);
    assert_eq!(s.ui.v.step.instr[0], 63);
}

#[test]
fn a_held_steps_note_stops_at_g_sharp_7_the_patterns_last_note() {
    // Above G#7 (92) a pattern byte is REST, KEYOFF or KEYON, not a note.
    let last = GT2_LASTNOTE as u8;
    let mut s = step();
    hold(&mut s, 8);
    s.ui.turn(PRESETS, 100);
    release(&mut s);
    assert_eq!(pattern(&s, 0)[8][0], last, "the knob stops at G#7");
    assert_eq!(s.ui.v.step.note[0], last - GT2_FIRSTNOTE as u8);
    hold(&mut s, 8);
    s.ui.turn(PRESETS, 3);
    release(&mut s);
    assert_eq!(pattern(&s, 0)[8][0], last, "and stays there");
    tap(&mut s, 10);
    assert_eq!(pattern(&s, 0)[10][0], last, "the lane's note is G#7 too");
}

#[test]
fn algorithm_pages_and_the_page_shows_in_the_selector_and_the_ruler() {
    let mut s = step();
    s.ui.v.song.order_pos = 1;
    s.ui.turn(PANEL_ENC_SELECT, 2);
    settle(&mut s.ui);
    assert_eq!(s.ui.big(0, 5, 2, 6), "PAT 03");
    assert_eq!(s.ui.small(2, 5, 15), "1 2 3 4 5 6 7 8");
    assert_eq!(s.ui.small(2, 21, 9), "128 STEPS");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(3, 5, 2), "17");
    assert_eq!(s.ui.small(3, 20, 2), "29");
    assert_eq!(attr(&s.ui, 7, 2) >> 4, th().hot, "page 2 reversed");
    assert_eq!(attr(&s.ui, 5, 2) >> 4, th().bg);
    tap(&mut s, 0);
    assert_eq!(pattern(&s, 3)[16], [C3, 1, 0, 0]);
    assert_eq!(pip(&s, 2, 0).0, FONT_BALL as u8);
    s.ui.turn(PANEL_ENC_ALGORITHM, 20);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(3, 5, 3), "113", "the last page: 8 of 8");
    s.ui.turn(PANEL_ENC_ALGORITHM, -20);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(3, 5, 2), "1 ", "page 1");
    // A pattern of another length shows as many pages as it needs: 64 rows, 4 pages.
    s.ui.v.song.order_pos = 0;
    settle(&mut s.ui);
    assert_eq!(s.ui.small(2, 5, 15).trim_end(), "1 2 3 4");
    // The popup names the page.
    let (name, value, fill, max) = popup(&s, PANEL_ENC_ALGORITHM, false).unwrap();
    assert_eq!(
        (name.as_str(), value.as_str(), fill, max),
        ("PAGE", "1/4", 0, 3)
    );
}

#[test]
fn while_the_song_plays_the_page_and_the_white_step_follow_it() {
    let mut s = step();
    s.ui.v.song.order_pos = 1;
    s.ui.turn(PANEL_ENC_SELECT, 2);
    settle(&mut s.ui);
    s.f.st.state = SONG_T_PLAYING as u8;
    s.f.st.order_pos = [0, 0, 1];
    s.f.st.row = [3, 3, 70];
    settle(&mut s.ui);
    assert_eq!(s.ui.small(3, 5, 3), "65 ", "row 70 is on page 5");
    assert_eq!(attr(&s.ui, 13, 2) >> 4, th().hot);
    assert_eq!(pip(&s, 2, 6), (FONT_PIP as u8, th().value | th().bg << 4));
    assert_eq!(
        pip(&s, 0, 3).1 & 15,
        th().value,
        "voice 1's playing step is white too"
    );
    assert_eq!(
        attr(&s.ui, pad_col(6), pad_row(6)) >> 4,
        th().cursor,
        "the pads are the keys, not the playhead"
    );
    // ALGORITHM does not page against the playhead.
    s.ui.turn(PANEL_ENC_ALGORITHM, -3);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(3, 5, 3), "65 ");
    // Stopped, the page stays where the playhead left it.
    s.f.st.state = SONG_T_STOPPED as u8;
    settle(&mut s.ui);
    assert_eq!(s.ui.small(3, 5, 3), "65 ");
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(3, 5, 3), "49 ");
}

fn popup(s: &Song, enc: u32, edit: bool) -> Option<(String, String, u32, u32)> {
    let mut name = [0; SCREEN_POPUP_TEXT as usize];
    let mut value = [0; SCREEN_POPUP_TEXT as usize];
    let (mut fill, mut max) = (0, 0);
    let shown = unsafe {
        step_view_popup(
            &s.ui.v.step,
            enc,
            i32::from(edit),
            name.as_mut_ptr(),
            value.as_mut_ptr(),
            &mut fill,
            &mut max,
        )
    };
    let text = |b: &[std::ffi::c_char]| {
        unsafe { CStr::from_ptr(b.as_ptr()) }
            .to_str()
            .unwrap()
            .to_owned()
    };
    (shown != 0).then(|| (text(&name), text(&value), fill, max))
}

#[test]
fn a_held_step_and_a_knob_is_a_lock_that_the_step_shows_and_keeps_for_its_row_only() {
    let mut s = step();
    let t = th();
    hold(&mut s, 3);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(17, 2, 13), " HELD STEP 4 ");
    assert_eq!(attr(&s.ui, 2, 17) & 15, t.hot);
    assert_eq!(
        pip(&s, 0, 3).1 >> 4,
        t.hot,
        "the held step is a yellow block"
    );
    assert_eq!(attr(&s.ui, pad_col(3), pad_row(3)) >> 4, t.hot);
    assert_eq!(
        attr(&s.ui, 5, 18) & 15,
        t.knob_text[0],
        "the frame comes alive"
    );
    s.ui.turn(PANEL_ENC_K1, 1);
    settle(&mut s.ui);
    assert_eq!(locks(&s), [(0, 3, SONG_LOCK_CUTOFF as u8, 1056)]);
    assert_eq!(s.ui.small(18, 12, 4), "LOCK");
    assert_eq!(attr(&s.ui, 12, 18) & 15, t.hot, "LOCK in yellow");
    assert_eq!(s.ui.small(19, 11, 3), "52 ");
    assert_eq!(s.ui.small(19, 26, 3), "---", "the others have none");
    s.ui.turn(PANEL_ENC_K1, 2);
    s.ui.turn(PANEL_ENC_K2, 1);
    s.ui.turn(PANEL_ENC_K3, -1);
    settle(&mut s.ui);
    assert_eq!(
        locks(&s),
        [
            (0, 3, SONG_LOCK_CUTOFF as u8, 1120),
            (0, 3, SONG_LOCK_RESONANCE as u8, 9),
            (0, 3, SONG_LOCK_PULSE as u8, 1984),
        ]
    );
    // The knob's popup says what it holds, with the bar's fill.
    assert_eq!(
        popup(&s, PANEL_ENC_K1, false),
        Some(("CUTOFF".into(), "1120".into(), 1120, 2047))
    );
    assert_eq!(
        popup(&s, PANEL_ENC_K3, false),
        Some(("PULSE WIDTH".into(), "1984".into(), 1984, 4095))
    );
    release(&mut s);
    // A lock is no toggle, and the notch stays after the key.
    assert_eq!(pattern(&s, 0)[3], R);
    assert_eq!(s.f.cell_writes, 0);
    assert_eq!(notch(&s, 0, 3), (FONT_V4 as u8, t.hot | t.bg << 4));
    assert_eq!(notch(&s, 0, 2).0, b' ');
    // A knob without a held step is LIVE's: no lock.
    s.ui.turn(PANEL_ENC_K1, 1);
    assert_eq!(locks(&s).len(), 3);
    // EDIT held: the character.
    hold(&mut s, 3);
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.turn(PANEL_ENC_K1, 1);
    s.ui.turn(PANEL_ENC_K4, -1);
    s.ui.hold(PANEL_BTN_EDIT, false);
    release(&mut s);
    let l = locks(&s);
    assert!(l.contains(&(0, 3, SONG_LOCK_AGE as u8, 66)));
    assert!(l.contains(&(0, 3, SONG_LOCK_BLEED as u8, 62)));
    // Values stay in range.
    hold(&mut s, 3);
    s.ui.turn(PANEL_ENC_K2, 100);
    s.ui.turn(PANEL_ENC_K1, -1000);
    release(&mut s);
    let l = locks(&s);
    assert!(l.contains(&(0, 3, SONG_LOCK_RESONANCE as u8, 15)));
    assert!(l.contains(&(0, 3, SONG_LOCK_CUTOFF as u8, 0)));
}

#[test]
fn a_held_step_and_k4_is_a_set_the_players_own_waveform_command_shown_apart_from_a_lock() {
    let mut s = step();
    let t = th();
    hold(&mut s, 3);
    s.ui.turn(PANEL_ENC_K4, 1);
    settle(&mut s.ui);
    assert_eq!(
        pattern(&s, 0)[3],
        [GT2_REST as u8, 0, 7, 0x11],
        "7XY, triangle"
    );
    assert_eq!(s.ui.small(20, 25, 3), "SET");
    assert_eq!(
        attr(&s.ui, 25, 20) & 15,
        t.knob[1],
        "SET in its own colour, not LOCK's"
    );
    assert_ne!(t.knob[1], t.hot);
    assert_eq!(s.ui.small(21, 26, 3), "TRI");
    s.ui.turn(PANEL_ENC_K4, 2);
    assert_eq!(pattern(&s, 0)[3][3], 0x41, "pulse");
    s.ui.turn(PANEL_ENC_K4, 5);
    assert_eq!(pattern(&s, 0)[3][3], 0x81, "noise, the last");
    assert_eq!(
        popup(&s, PANEL_ENC_K4, false),
        Some(("WAVEFORM".into(), "NOISE".into(), 4, 4))
    );
    release(&mut s);
    assert_eq!(pattern(&s, 0)[3][2], 7);
    assert_eq!(pattern(&s, 0)[3][0], GT2_REST as u8, "no note was written");
    assert_eq!(
        notch(&s, 0, 3),
        (FONT_V2 as u8, t.knob[1] | t.bg << 4),
        "a SET is a cyan notch"
    );
    assert!(locks(&s).is_empty(), "a SET is no lock");
    // Both on one step: the lock's yellow notch, and the frame says both.
    hold(&mut s, 3);
    s.ui.turn(PANEL_ENC_K1, 1);
    settle(&mut s.ui);
    assert_eq!(s.ui.small(18, 12, 4), "LOCK");
    assert_eq!(s.ui.small(20, 25, 3), "SET");
    release(&mut s);
    assert_eq!(notch(&s, 0, 3).0, FONT_V4 as u8);
    // Down to none: the command goes.
    hold(&mut s, 3);
    s.ui.turn(PANEL_ENC_K4, -10);
    release(&mut s);
    assert_eq!(pattern(&s, 0)[3], R);
}

#[test]
fn a_set_is_refused_where_the_rows_command_column_is_taken() {
    let mut s = step();
    hold(&mut s, 5);
    s.ui.turn(PANEL_ENC_K4, 1);
    assert_eq!(s.ui.toast().as_deref(), Some("ROW HAS A COMMAND"));
    assert_eq!(pattern(&s, 0)[5], [GT2_REST as u8, 0, 4, 0x22]);
    assert_eq!(s.ui.v.s.popup_ask, 0, "no popup over the refusal");
    release(&mut s);
    // A lock needs no command column.
    hold(&mut s, 5);
    s.ui.turn(PANEL_ENC_K1, 1);
    release(&mut s);
    assert_eq!(locks(&s).len(), 1);
}

#[test]
fn the_notches_follow_the_locks_of_each_lane_and_page() {
    let mut model = fixed_song();
    for (pattern, row, param) in [
        (0u32, 3u32, SONG_LOCK_CUTOFF),
        (0, 3, SONG_LOCK_PULSE),
        (0, 20, SONG_LOCK_RESONANCE),
        (0, 63, SONG_LOCK_AGE),
        (1, 5, SONG_LOCK_CUTOFF),
        (2, 0, SONG_LOCK_BLEED),
        (3, 20, SONG_LOCK_CUTOFF),
    ] {
        assert_eq!(
            unsafe { song_lock_put(model.ptr_mut(), pattern, row, param, 1) },
            0
        );
    }
    let mut s = Song::playing(Some(model));
    open_step(&mut s);
    let marked = |s: &Song, lane: usize| -> Vec<usize> {
        (0..16)
            .filter(|&i| notch(s, lane, i).0 == FONT_V4 as u8)
            .collect()
    };
    assert_eq!(marked(&s, 0), [3]);
    assert_eq!(marked(&s, 1), [5]);
    assert_eq!(marked(&s, 2), [0]);
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(marked(&s, 0), [4], "page 2 is rows 16-31: row 20");
    assert!(marked(&s, 1).is_empty() && marked(&s, 2).is_empty());
    s.ui.turn(PANEL_ENC_ALGORITHM, 2);
    settle(&mut s.ui);
    assert_eq!(marked(&s, 0), [15], "page 4's last row, 63");
}

#[test]
fn lfo_on_a_held_step_clears_its_locks_and_its_set() {
    let mut s = step();
    hold(&mut s, 3);
    s.ui.turn(PANEL_ENC_K1, 1);
    s.ui.turn(PANEL_ENC_K4, 1);
    s.ui.press(PANEL_BTN_LFO);
    release(&mut s);
    assert!(locks(&s).is_empty());
    assert_eq!(pattern(&s, 0)[3], R);
    assert_eq!(pattern(&s, 0)[0][0], C3 + 7, "other steps stay");
    hold(&mut s, 0);
    s.ui.turn(PANEL_ENC_K1, 1);
    release(&mut s);
    hold(&mut s, 1);
    s.ui.press(PANEL_BTN_LFO);
    release(&mut s);
    assert_eq!(locks(&s).len(), 1, "another step's lock stays");
}

#[test]
fn the_keys_above_the_sixteen_steps_audition_and_a_step_key_auditions_its_note() {
    let mut s = step();
    s.calls();
    s.ui.note_keys(1 << 16, 1 << 16);
    s.ui.note_keys(0, 0);
    assert_eq!(pattern(&s, 0).iter().filter(|r| r[0] == C3).count(), 0);
    let jam: Vec<_> = s.calls();
    assert!(
        jam.iter()
            .any(|c| matches!(c, crate::song_view::Call::Jam(0, _, 1))),
        "{jam:?}"
    );
    s.calls();
    hold(&mut s, 0);
    let jam = s.calls();
    assert_eq!(
        jam,
        [crate::song_view::Call::Jam(0, 43, 1)],
        "the step's own note, C-3 plus a fifth, at its own instrument"
    );
    release(&mut s);
    s.ui.frame();
    assert!(
        s.calls().contains(&crate::song_view::Call::Jam(0, 0xff, 1)),
        "the release"
    );
}

#[test]
fn glo_and_a_step_key_mutes_and_toggles_no_step() {
    let mut s = step();
    s.ui.hold(PANEL_BTN_GLO, true);
    s.ui.note_keys(1, 1);
    s.ui.note_keys(0, 0);
    s.ui.hold(PANEL_BTN_GLO, false);
    settle(&mut s.ui);
    assert_eq!(s.ui.v.song.mute, 1);
    assert_eq!(pattern(&s, 0)[0][0], C3 + 7);
    assert_eq!(s.f.cell_writes, 0);
    assert_eq!(s.ui.small(0, 18, 3), "MUT");
    assert_eq!(attr(&s.ui, 0, 0) & 15, th().dim, "V1 grey");
}

/// A latched GLO (a double tap) is the layer too: the key mutes and toggles no step, as held.
#[test]
fn a_latched_glo_and_a_step_key_mutes_and_toggles_no_step() {
    let mut s = step();
    s.ui.p.glo_latch = 1;
    s.ui.note_keys(1, 1);
    s.ui.note_keys(0, 0);
    settle(&mut s.ui);
    assert_eq!(s.ui.v.song.mute, 1);
    assert_eq!(pattern(&s, 0)[0][0], C3 + 7, "no step toggled");
    assert_eq!(s.f.cell_writes, 0);
}

#[test]
fn an_edit_that_waits_is_retried_and_the_second_row_follows_the_first() {
    let mut s = step();
    s.f.refuse = true;
    s.ui.note_keys(1 << 6, 1 << 6);
    s.ui.note_keys(0, 0);
    s.ui.frame();
    assert_eq!(pattern(&s, 0)[6], R, "the player has not taken it yet");
    s.f.refuse = false;
    settle(&mut s.ui);
    let p = pattern(&s, 0);
    assert_eq!((p[6], p[7]), ([C3, 1, 0, 0], OFF));
}

#[test]
fn knob_turns_and_taps_that_arrive_while_the_editor_is_busy_wait_their_turn_in_order() {
    let mut s = step();
    // The player takes nothing for now (the audition starting holds edits for a while).
    s.f.refuse = true;
    hold(&mut s, 5 + 3);
    s.ui.turn(PANEL_ENC_K1 + 3, 1);
    s.ui.turn(PANEL_ENC_K1 + 3, 1);
    s.ui.turn(PRESETS, 2);
    s.ui.turn(PRESETS, 3);
    s.ui.frame();
    assert_eq!(pattern(&s, 0)[8], R, "nothing applied yet");
    s.f.refuse = false;
    release(&mut s);
    let p = pattern(&s, 0);
    assert_eq!(
        p[8],
        [C3 + 5, 1, 7, 0x21],
        "both SET detents (sawtooth) and both note turns (+5) arrived: {:?}",
        p[8]
    );
    // Taps in a row: all of them, in order, however busy the editor.
    s.f.refuse = true;
    for k in [1, 3, 5] {
        s.ui.note_keys(1 << k, 1 << k);
        s.ui.note_keys(0, 0);
    }
    s.ui.frame();
    assert_eq!(pattern(&s, 0)[1], R);
    s.f.refuse = false;
    settle(&mut s.ui);
    let p = pattern(&s, 0);
    for k in [1usize, 3, 5] {
        assert_eq!(
            p[k][0],
            C3 + 5,
            "step {}: the lane's note is the one set",
            k + 1
        );
    }
    // The queue holds four besides the one in flight: the sixth action is refused with a
    // toast, nothing else lost.
    s.f.refuse = true;
    for k in [10, 11, 12, 13, 14, 15] {
        s.ui.note_keys(1 << k, 1 << k);
        s.ui.note_keys(0, 0);
    }
    assert_eq!(s.ui.toast().as_deref(), Some("WAITING"));
    s.f.refuse = false;
    settle(&mut s.ui);
    let p = pattern(&s, 0);
    assert_eq!(p[10][0], C3 + 5);
    assert_eq!(p[14][0], C3 + 5);
    assert_eq!(
        p[15][0], GT2_KEYOFF as u8,
        "the sixth was refused: only the fifth's key off"
    );
}

#[test]
fn no_song_no_edit_and_a_toast_says_so() {
    let mut s = Song::playing(None);
    s.ui.press(PANEL_BTN_SEQ);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "STEP");
    assert_eq!(s.ui.small(2, 21, 9).trim(), "0 STEPS");
    tap(&mut s, 0);
    s.ui.turn(PANEL_ENC_K1, 1);
    assert!(s.ui.toast().is_some());
}

/// A song of three 8-row patterns (6 calls a row): voice 1 plays notes on rows 0 and 4 with a
/// pulse instrument whose wavetable and pulse table give the voice something to hold.
fn loop_song() -> Model {
    let row = |note: u8, instr: u8| [note, instr, 0, 0];
    let p0 = vec![row(C3, 1), R, R, R, row(C3 + 4, 0), R, R, R];
    let p1 = vec![row(C3 - 12, 1), R, R, R, R, R, R, R];
    let p2 = vec![row(C3 - 17, 1), R, R, R, R, R, R, R];
    let mut spec = SngSpec::new();
    spec.orders = vec![vec![(vec![0], 0), (vec![1], 0), (vec![2], 0)]];
    let mut ins = [0u8; 25];
    ins[..9].copy_from_slice(&[0x09, 0xa0, 1, 1, 0, 0, 0, 2, 0x09]);
    spec.instr = vec![ins];
    spec.tables = vec![
        vec![(0x41, 0x00), (0xff, 0x00)],
        vec![(0x88, 0x00), (0xff, 0x00)],
        vec![],
        vec![],
    ];
    spec.patterns = vec![p0, p1, p2]
        .into_iter()
        .map(|p| (p, [0xff, 0, 0, 0]))
        .collect();
    let mut m = Model::host();
    m.read(&spec.encode(), 0).unwrap();
    m
}

/// The song the view edits, as a model of its own (the region copied).
fn snapshot(s: &Song) -> Model {
    let mut m = Model::host();
    let n = m.bytes().len();
    unsafe {
        std::ptr::copy_nonoverlapping(s.ui.v.song.song.cast::<u8>(), m.ptr_mut().cast::<u8>(), n);
    }
    m
}

fn loop_step() -> Song {
    let mut s = Song::playing(Some(loop_song()));
    open_step(&mut s);
    s
}

/// A song's SID writes call by call from SIDOU's player, as one list a call.
fn sidou_calls(song: &Model, calls: usize) -> Vec<Vec<(u8, u8)>> {
    let mut p = Player::new(song, 0, &default_options()).expect("the player takes the song");
    (0..calls).map(|_| p.call()).collect()
}

/// GoatTracker's own player's writes for the same file, a list a call.
fn reference_calls(sng: &[u8], calls: usize) -> Vec<Vec<(u8, u8)>> {
    let o = default_options();
    let opts = Options {
        hr_adsr: u32::from(o.hr_adsr),
        multiplier: u32::from(o.multispeed),
        ntsc: u32::from(o.ntsc),
        finevibrato: u32::from(o.finevibrato),
        optimize_pulse: u32::from(o.optimize_pulse),
        optimize_realtime: u32::from(o.optimize_realtime),
        player_version: 0,
        sid_address: u32::from(o.sid_address),
        player_address: u32::from(o.player_page) << 8,
        zeropage: u32::from(o.zeropage),
        sid_model: u32::from(o.sid_model),
        ..Options::default()
    };
    let t = trace(sng, 0, calls, &opts).expect("the reference plays it");
    t.frames
        .iter()
        .flat_map(|f| f.calls.clone())
        .take(calls)
        .collect()
}

/// Voice 1's control register after each call: the last write to register 4 so far.
fn control(calls: &[Vec<(u8, u8)>]) -> Vec<u8> {
    let mut at = 0u8;
    calls
        .iter()
        .map(|c| {
            for &(reg, v) in c {
                if reg == 4 {
                    at = v;
                }
            }
            at
        })
        .collect()
}

#[test]
fn a_step_made_song_plays_as_goattrackers_player_does_with_its_set_and_without_the_chunk_locks() {
    let mut s = loop_step();
    // Steps 2 and 3 of voice 1 (the lane's note and instrument, a key off after each), and a SET
    // (sawtooth) on row 5, after the note on row 4.
    tap(&mut s, 1);
    tap(&mut s, 2);
    hold(&mut s, 5);
    s.ui.turn(PANEL_ENC_K1 + 3, 2);
    release(&mut s);
    let song = snapshot(&s);
    let p = pattern(&s, 0);
    assert_eq!(p[1], [C3, 1, 0, 0]);
    assert_eq!(p[2], [C3, 1, 0, 0]);
    assert_eq!(p[3], OFF, "staccato");
    assert_eq!(p[5], [GT2_REST as u8, 0, 7, 0x21]);
    assert!(
        song.ext_find(sidou_tests::song::TAG_LOCK).is_none(),
        "no chunk locks"
    );
    // SIDOU's player against GoatTracker's own, call for call, over three patterns of rows.
    let sng = song.write(false);
    let (ours, theirs) = (sidou_calls(&song, 400), reference_calls(&sng, 400));
    assert_eq!(ours.len(), theirs.len());
    for (k, (a, b)) in ours.iter().zip(&theirs).enumerate() {
        assert_eq!(a, b, "call {k}");
    }
    assert!(ours.iter().any(|c| !c.is_empty()));
    // The SET persists over the following rows: from row 5 (call 37: a row is 6 calls, the first
    // starting at call 7) voice 1's waveform is the sawtooth's with the gate as the notes left
    // it, through rows 6 and 7, until the next bar's note sets its own.
    let ctrl = control(&ours);
    assert_eq!(ctrl[36] & 0xf0, 0x40, "the pulse before it");
    for (call, c) in ctrl.iter().enumerate().take(55).skip(37) {
        assert_eq!(c & 0xf0, 0x20, "call {call}: control {c:#x}");
    }
    assert_eq!(ctrl[57] & 0xf0, 0x40, "the next bar's note");
}

/// Cutoff and voice 1's pulse width after each call, and the row voice 1 plays.
fn play(s: &mut Model, calls: u32) -> (Transport, Vec<(u32, u32, u8, u8)>) {
    let mut t = Transport::new(s);
    assert!(t.play(0, 0));
    let mut out = Vec::new();
    while t.calls() < calls {
        t.pass();
        let c = t.calls();
        t.block_silent();
        if t.calls() != c {
            out.push((
                t.chip.fc,
                t.chip.osc[0].pw,
                t.sc.row_pat[0],
                t.sc.row_row[0],
            ));
        }
    }
    (t, out)
}

#[test]
fn a_lock_made_on_a_held_step_holds_for_its_row_only_in_the_players_output() {
    let mut s = loop_step();
    hold(&mut s, 3);
    s.ui.turn(PANEL_ENC_K1, 4);
    s.ui.turn(PANEL_ENC_K3, 2);
    release(&mut s);
    let mut locked = snapshot(&s);
    assert_eq!(
        locks(&s),
        [
            (0, 3, SONG_LOCK_CUTOFF as u8, 1152),
            (0, 3, SONG_LOCK_PULSE as u8, 2176)
        ]
    );
    let mut plain = loop_song();
    let (t0, base) = play(&mut plain, 300);
    let (t, got) = play(&mut locked, 300);
    let mut held = 0;
    for (k, (&(fc, pw, pat, r), &(fc0, pw0, _, _))) in got.iter().zip(&base).enumerate() {
        if (pat, r) == (0, 3) {
            assert_eq!((fc, pw), (1152, 2176), "call {k}: row 3 locked");
            held += 1;
        } else {
            assert_eq!(
                (fc, pw),
                (fc0, pw0),
                "call {k}: row {r}, the song's own again"
            );
        }
    }
    assert!(held > 20, "{held} locked calls");
    assert!(
        sidou_tests::transport::player_state(&t.sc.player)
            == sidou_tests::transport::player_state(&t0.sc.player),
        "the player itself plays as without the lock"
    );
}
