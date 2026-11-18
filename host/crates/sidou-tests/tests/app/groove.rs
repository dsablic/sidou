// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::song_view::{attr, open_tracker, Song};
use crate::step_view::{
    groove, groove_turn, hold, leave_groove, locks, loop_song, notes_col, open_step, pattern,
    popup, release, settle, step, tap, th,
};
use crate::{PANEL_ENC_K2, PANEL_ENC_K3};

const TITLE_ROW: usize = STEP_VIEW_HELD_ROW as usize;
const LANE_ROW: usize = STEP_VIEW_LANES_ROW as usize;
const CMD_FUNKTEMPO: u8 = 0x0e;

fn looping() -> Song {
    let mut s = Song::playing(Some(loop_song()));
    open_step(&mut s);
    s
}

fn cell_value(s: &Song, k: usize) -> String {
    let (col, len) = if k % 2 == 1 { (26, 3) } else { (11, 4) };
    s.ui.small(TITLE_ROW + 2 + 2 * (k / 2), col, len)
        .trim_end()
        .to_owned()
}

fn page_popup(s: &Song) -> String {
    popup(s, PANEL_ENC_ALGORITHM, false)
        .expect("the page popup")
        .1
}

fn step_col(i: usize) -> usize {
    STEP_VIEW_STEP_COL as usize + i + i / 4
}

#[test]
fn algorithm_past_the_last_page_is_the_groove_page_and_back_is_the_picked_steps_page() {
    let mut s = step();
    s.ui.turn(PANEL_ENC_ALGORITHM, 3);
    settle(&mut s.ui);
    assert_eq!(page_popup(&s), "4/4");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(page_popup(&s), "GROOVE");
    assert_eq!(s.ui.small(2, 5, 9), "1 2 3 4 G");
    assert_eq!(attr(&s.ui, 13, 2) >> 4, th().hot, "G reversed");
    assert_eq!(attr(&s.ui, 11, 2) >> 4, th().bg);
    assert_eq!(s.ui.small(TITLE_ROW, 2, 8), " GROOVE ");
    assert_eq!(s.ui.small(TITLE_ROW, 12, 11), " V1 STEP 49");
    assert_eq!(
        s.ui.big(3, 5, 2, 3),
        "49 ",
        "the lanes stay on the page it came from"
    );
    for (k, name) in ["SWING", "HITS", "ROTATE", "RATCHET"].iter().enumerate() {
        let col = if k % 2 == 1 { 20 } else { 5 };
        assert_eq!(
            s.ui.small(TITLE_ROW + 1 + 2 * (k / 2), col, name.len()),
            *name
        );
    }
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(page_popup(&s), "GROOVE", "the last page there is");
    s.ui.turn(PANEL_ENC_PRESETS, -40);
    settle(&mut s.ui);
    let (name, value, ..) = popup(&s, PANEL_ENC_PRESETS, false).unwrap();
    assert_eq!((name.as_str(), value.as_str()), ("STEP", "9/64"));
    assert_eq!(
        s.ui.big(3, 5, 2, 3),
        "1  ",
        "the lanes follow the picked step"
    );
    leave_groove(&mut s);
    assert_eq!(page_popup(&s), "1/4", "back on the picked step's page");
}

#[test]
fn the_groove_page_shows_swing_hits_rotate_and_the_picked_steps_ratchet() {
    let mut s = looping();
    groove(&mut s);
    assert_eq!(
        (0..4).map(|k| cell_value(&s, k)).collect::<Vec<_>>(),
        ["STRT", "0", "0", "OFF"]
    );
    assert_eq!(
        groove_turn(&mut s, PANEL_ENC_K1, 2),
        ("SWING".into(), "SHUFFLE".into())
    );
    assert_eq!(
        groove_turn(&mut s, PANEL_ENC_K2, 3),
        ("HITS".into(), "3/8".into())
    );
    assert_eq!(
        groove_turn(&mut s, PANEL_ENC_K3, 1),
        ("ROTATE".into(), "1".into())
    );
    tap(&mut s, 1);
    assert_eq!(s.ui.small(TITLE_ROW, 12, 10), " V1 STEP 2");
    assert_eq!(
        groove_turn(&mut s, PANEL_ENC_K4, 1),
        ("RATCHET".into(), "x2".into())
    );
    assert_eq!(
        (0..4).map(|k| cell_value(&s, k)).collect::<Vec<_>>(),
        ["SHUF", "3", "1", "x2"]
    );
    tap(&mut s, 2);
    assert_eq!(cell_value(&s, 3), "---", "a rest has no ratchet");
    assert_eq!(popup(&s, PANEL_ENC_K4, false).unwrap().1, "NO NOTE");
}

#[test]
fn a_step_key_on_the_groove_page_picks_the_step_and_changes_no_note() {
    let mut s = looping();
    groove(&mut s);
    let notes = notes_col(&s);
    tap(&mut s, 4);
    tap(&mut s, 5);
    assert_eq!(notes_col(&s), notes, "no step toggled");
    assert_eq!(s.ui.small(TITLE_ROW, 12, 10), " V1 STEP 6");
    assert_eq!(
        attr(&s.ui, step_col(5), LANE_ROW) >> 4,
        th().hot,
        "the picked step lit"
    );
    assert_eq!(attr(&s.ui, step_col(4), LANE_ROW) >> 4, th().bg);
    hold(&mut s, 0);
    s.ui.turn(PANEL_ENC_K1, 1);
    release(&mut s);
    assert!(
        locks(&s).is_empty(),
        "a key on GROOVE holds no step for locks"
    );
    tap(&mut s, 12);
    assert_eq!(
        s.ui.small(TITLE_ROW, 12, 10),
        " V1 STEP 1",
        "past the lane's 8 steps"
    );
}

#[test]
fn while_the_song_plays_algorithm_opens_the_groove_page_on_the_playing_page() {
    let mut s = step();
    s.f.st.state = SONG_T_PLAYING as u8;
    s.f.st.order_pos = [0, 0, 0];
    s.f.st.row = [37, 37, 37];
    settle(&mut s.ui);
    assert_eq!(s.ui.big(3, 5, 2, 3), "33 ");
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    assert_eq!(page_popup(&s), "GROOVE");
    assert_eq!(s.ui.small(TITLE_ROW, 12, 11), " V1 STEP 33");
    leave_groove(&mut s);
    s.f.st.row = [53, 53, 53];
    settle(&mut s.ui);
    assert_eq!(
        s.ui.big(3, 5, 2, 3),
        "49 ",
        "the page follows the song again"
    );
}

#[test]
fn the_old_chords_set_no_swing_no_euclid_and_no_ratchet() {
    let mut s = looping();
    let notes = notes_col(&s);
    s.ui.down(PANEL_BTN_FX);
    s.ui.turn(PANEL_ENC_K1, 3);
    s.ui.press(PANEL_BTN_LFO);
    assert_ne!(s.ui.toast().as_deref(), Some("EUCLID OFF"));
    s.ui.up(PANEL_BTN_FX);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "PUNCH", "FX opens PUNCH, no chord");
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(notes_col(&s), notes, "FX + K1 writes no rhythm");
    hold(&mut s, 0);
    s.ui.down(PANEL_BTN_EDIT);
    s.ui.turn(PANEL_ENC_ALGORITHM, -1);
    assert_eq!(popup(&s, PANEL_ENC_ALGORITHM, true).unwrap().0, "CHANCE");
    s.ui.up(PANEL_BTN_EDIT);
    release(&mut s);
    assert_eq!(pattern(&s, 0)[0][1], 1, "the step keeps its instrument");
    assert_eq!(locks(&s), [(0, 0, SONG_LOCK_CHANCE as u8, 95)]);
    for tracker in [false, true] {
        let mut s = looping();
        if tracker {
            open_tracker(&mut s.ui);
        }
        s.ui.down(PANEL_BTN_EDIT);
        s.ui.down(PANEL_BTN_SCL);
        s.ui.turn(PANEL_ENC_ALGORITHM, 2);
        s.ui.up(PANEL_BTN_SCL);
        s.ui.up(PANEL_BTN_EDIT);
        settle(&mut s.ui);
        assert_ne!(pattern(&s, 0)[0][2], CMD_FUNKTEMPO, "tracker {tracker}");
        assert_eq!(
            s.ui.small(1, SONG_VIEW_TEMPO_COL as usize, 4),
            "BPM ",
            "tracker {tracker}"
        );
    }
}
