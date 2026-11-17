// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::song_view::Song;
use crate::step_view::{
    hold, is_note, locks, loop_song, notes_col, open_step, popup, release, settle, C3,
};
use crate::PANEL_ENC_K2;

fn bjorklund(hits: u32, steps: u32) -> String {
    let mut out = [0u8; SONG_GEN_EUCLID_ROWS as usize];
    unsafe { song_gen_bjorklund(hits, steps, out.as_mut_ptr()) };
    out[..steps as usize]
        .iter()
        .map(|&b| if b == 1 { 'x' } else { '.' })
        .collect()
}

#[test]
fn the_hits_are_bjorklunds_known_rhythms() {
    for (hits, steps, rhythm) in [
        (3, 8, "x..x..x."),
        (5, 8, "x.xx.xx."),
        (7, 16, "x..x.x.x..x.x.x."),
        (2, 5, "x.x.."),
        (3, 7, "x.x.x.."),
        (4, 7, "x.x.x.x"),
        (4, 9, "x.x.x.x.."),
        (5, 12, "x..x.x..x.x."),
        (5, 16, "x..x..x..x..x..."),
        (0, 8, "........"),
        (8, 8, "xxxxxxxx"),
        (1, 128, &format!("x{}", ".".repeat(127))),
    ] {
        assert_eq!(bjorklund(hits, steps), rhythm, "E({hits},{steps})");
    }
}

fn eight_row_lane() -> Song {
    let mut s = Song::playing(Some(loop_song()));
    open_step(&mut s);
    s
}

fn euclid(s: &mut Song, knob: u32, turn: i32) -> (String, String) {
    s.ui.down(PANEL_BTN_FX);
    s.ui.frame();
    s.ui.turn(knob, turn);
    settle(&mut s.ui);
    let shown = popup(s, knob, false).expect("the popup");
    s.ui.up(PANEL_BTN_FX);
    settle(&mut s.ui);
    (shown.0, shown.1)
}

fn hit_rows(s: &Song) -> Vec<usize> {
    notes_col(s)
        .iter()
        .enumerate()
        .filter(|(_, &n)| is_note(n))
        .map(|(r, _)| r)
        .collect()
}

#[test]
fn hits_and_rotate_write_the_rhythm_as_real_steps_keeping_notes_and_locks() {
    let mut s = eight_row_lane();
    assert_eq!(hit_rows(&s), [0, 4]);
    hold(&mut s, 3);
    s.ui.turn(PANEL_ENC_K1, 2);
    release(&mut s);
    let locked = locks(&s);
    assert_eq!(locked.len(), 1, "a cutoff lock on step 4");
    assert_eq!(
        euclid(&mut s, PANEL_ENC_K1, 3),
        ("HITS".into(), "3/8".into())
    );
    assert_eq!(hit_rows(&s), [0, 3, 6], "E(3,8)");
    assert_eq!(notes_col(&s)[0], C3, "a hit that stays keeps its note");
    assert_eq!(notes_col(&s)[3], C3, "a new hit is the lane's note");
    assert_eq!(locks(&s), locked, "the lock stays on its step");
    assert_eq!(s.ui.small(22, 2, 16), " EUCLID 3/8 ROT ");
    assert_eq!(
        euclid(&mut s, PANEL_ENC_K2, 1),
        ("ROTATE".into(), "1".into())
    );
    assert_eq!(hit_rows(&s), [1, 4, 7], "rotated a step later");
    assert_eq!(euclid(&mut s, PANEL_ENC_K2, -2).1, "7");
    assert_eq!(hit_rows(&s), [2, 5, 7], "rotated back past the first step");
    euclid(&mut s, PANEL_ENC_K1, 2);
    assert_eq!(hit_rows(&s), [1, 2, 4, 5, 7], "E(5,8) at the same rotation");
}

#[test]
fn euclid_off_keeps_the_steps_and_the_next_turn_starts_it_again() {
    let mut s = eight_row_lane();
    euclid(&mut s, PANEL_ENC_K1, 3);
    let steps = notes_col(&s);
    s.ui.down(PANEL_BTN_FX);
    s.ui.press(PANEL_BTN_LFO);
    assert_eq!(s.ui.toast().as_deref(), Some("EUCLID OFF"));
    s.ui.up(PANEL_BTN_FX);
    settle(&mut s.ui);
    assert_eq!(notes_col(&s), steps, "the steps stay");
    assert_ne!(s.ui.small(22, 2, 8), " EUCLID ");
    euclid(&mut s, PANEL_ENC_K1, 1);
    assert_eq!(hit_rows(&s), [0, 2, 4, 6], "E(4,8) from the kept hits");
}
