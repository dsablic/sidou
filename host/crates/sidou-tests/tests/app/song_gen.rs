// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::song::Song as Model;

use crate::song_view::{ch, open_tracker, Song};
use crate::step_view::{
    dice_of, edit_press, fixed_song, hits, hold, is_note, loop_song, notes_col, open_step, pattern,
    reference_calls, release, set_dice_all, settle, sidou_calls, snapshot, step, C3, SCALES,
};
use crate::{c_utf8, PANEL_ENC_K3};

pub(crate) const DENSITY_BUSY: u32 = 3;
pub(crate) const DENSITY_FULL: u32 = 4;
pub(crate) const SCALE_MINOR_PENTATONIC: u32 = 0;
pub(crate) const SCALE_MAJOR: u32 = 3;
pub(crate) const RANGE_ONE_OCTAVE: u32 = 0;
pub(crate) const RANGE_BASS: u32 = 3;
const KEY_NOTE_OF_SONG_NOTE_0: u8 = 12;

fn tracker_on(song: Model) -> Song {
    let mut s = Song::playing(Some(song));
    open_tracker(&mut s.ui);
    s
}

pub(crate) fn roll_with_seed_at_a_fixed_time(s: &mut Song, seed: u32) -> Option<String> {
    s.ui.v.song.gen_.seed = seed;
    s.ui.v.s.now = 1000;
    s.ui.v.s.frame = 25;
    edit_press_toast(s, PANEL_BTN_SCL)
}

fn edit_press_toast(s: &mut Song, b: u32) -> Option<String> {
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.press(b);
    s.ui.hold(PANEL_BTN_EDIT, false);
    let toast = s.ui.toast();
    settle(&mut s.ui);
    toast
}

fn tracker_popup(s: &Song, enc: u32) -> Option<(String, String, u32, u32)> {
    let mut name = [0; SCREEN_POPUP_TEXT as usize];
    let mut value = [0; SCREEN_POPUP_TEXT as usize];
    let (mut fill, mut max) = (0, 0);
    let shown = unsafe {
        tracker_view_popup(
            &s.ui.v.song,
            enc,
            1,
            name.as_mut_ptr(),
            value.as_mut_ptr(),
            &mut fill,
            &mut max,
        )
    };
    (shown != 0).then(|| (c_utf8(name.as_ptr()), c_utf8(value.as_ptr()), fill, max))
}

fn settings_frame_lines(s: &Song, top: usize) -> Vec<(String, String, String)> {
    (0..5)
        .map(|i| {
            let r = top + 1 + 2 * i;
            (
                s.ui.small(r, 2, 2),
                s.ui.small(r + 1, 2, 6).trim_end().to_string(),
                s.ui.big(r, 11, 2, if i == 4 { 3 } else { 9 })
                    .trim_end()
                    .to_string(),
            )
        })
        .collect()
}

fn song_note_of_key(s: &Song, k: u32) -> u8 {
    let n = unsafe { live_key_note(s.ui.live(), (PANEL_BASE_NOTE + k) as u8) };
    n - KEY_NOTE_OF_SONG_NOTE_0
}

#[test]
fn trackers_dice_rolls_what_steps_does_for_the_same_seed_pattern_and_root() {
    for (dice, root) in [((2, 0, 0, 1), 36u8), ((3, 3, 4, 2), 41), ((1, 5, 1, 3), 50)] {
        let mut st = step();
        set_dice_all(&mut st, dice.0, dice.1, dice.2, dice.3);
        st.ui.v.song.gen_.root[0] = root;
        roll_with_seed_at_a_fixed_time(&mut st, 7);
        let mut tr = tracker_on(fixed_song());
        set_dice_all(&mut tr, dice.0, dice.1, dice.2, dice.3);
        tr.ui.v.song.gen_.root[0] = root;
        assert_eq!(tr.ui.v.song.instr, 1, "STEP's lane instrument");
        assert_eq!(
            roll_with_seed_at_a_fixed_time(&mut tr, 7).as_deref(),
            Some("NEW PATTERN")
        );
        assert_eq!(tr.ui.v.song.gen_.run, 0, "the run is over");
        let p = pattern(&tr, 0);
        assert_eq!(p, pattern(&st, 0), "{dice:?} root {root}");
        assert!(hits(&notes_col(&tr)).len() > 4);
        assert_eq!(p[5][2..], [4, 0x22], "the row's command stays");
    }
}

#[test]
fn tracker_rolls_the_shown_voices_pattern_and_mutates_it() {
    let mut s = tracker_on(fixed_song());
    s.ui.turn(PANEL_ENC_ALGORITHM, 1);
    settle(&mut s.ui);
    let v1 = pattern(&s, 0);
    edit_press(&mut s, PANEL_BTN_SCL);
    assert_eq!(pattern(&s, 0), v1, "V1 is left alone");
    let p1 = pattern(&s, 1);
    assert!(p1.iter().any(|c| is_note(c[0])), "V2's pattern rolled");
    assert_eq!(
        edit_press_toast(&mut s, PANEL_BTN_ARP).as_deref(),
        Some("MUTATED")
    );
    let after = pattern(&s, 1);
    let changed = (0..after.len()).filter(|&r| after[r] != p1[r]).count();
    assert!(
        (1..=4).contains(&changed),
        "MUTATE makes two changes at most, each a note and its key off: {changed} rows changed"
    );
    edit_press(&mut s, PANEL_BTN_LFO);
    assert_ne!(pattern(&s, 1), after);
}

#[test]
fn tracker_mutates_with_rec_on_and_rolls_nothing_while_recording() {
    let mut s = tracker_on(fixed_song());
    s.ui.press(PANEL_BTN_REC);
    settle(&mut s.ui);
    let before = pattern(&s, 0);
    assert_eq!(
        edit_press_toast(&mut s, PANEL_BTN_ARP).as_deref(),
        Some("MUTATED")
    );
    assert_ne!(pattern(&s, 0), before, "no key off at the cursor: MUTATE");
    let st = &mut s.f.st;
    st.state = SONG_T_PLAYING as u8;
    st.order_pos = [0, 0, 0];
    st.row = [2, 0, 0];
    s.ui.frame();
    assert!(unsafe { song_view_recording(&s.ui.v.song) } != 0);
    let before = pattern(&s, 0);
    edit_press(&mut s, PANEL_BTN_SCL);
    edit_press(&mut s, PANEL_BTN_ENV);
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.down(PANEL_BTN_SCL);
    assert_eq!(s.ui.v.song.gen_.scl_down, 0, "no chord while recording");
    s.ui.up(PANEL_BTN_SCL);
    s.ui.hold(PANEL_BTN_EDIT, false);
    settle(&mut s.ui);
    assert_eq!(pattern(&s, 0), before, "nothing rolled while recording");
}

#[test]
fn trackers_arp_landing_before_edit_is_the_same_mutate_and_no_key_off() {
    let rec_on = || {
        let mut s = tracker_on(fixed_song());
        s.ui.press(PANEL_BTN_REC);
        settle(&mut s.ui);
        s.ui.v.song.gen_.seed = 7;
        s.ui.v.s.now = 1000;
        s.ui.v.s.frame = 25;
        s
    };
    let mut twin = rec_on();
    assert_eq!(
        edit_press_toast(&mut twin, PANEL_BTN_ARP).as_deref(),
        Some("MUTATED")
    );
    let mut s = rec_on();
    let before = pattern(&s, 0);
    s.ui.down(PANEL_BTN_ARP);
    assert_eq!(pattern(&s, 0), before, "ARP waits for EDIT: no key off yet");
    s.ui.down(PANEL_BTN_EDIT);
    assert_eq!(s.ui.toast().as_deref(), Some("MUTATED"));
    s.ui.up(PANEL_BTN_ARP);
    s.ui.up(PANEL_BTN_EDIT);
    settle(&mut s.ui);
    assert_ne!(pattern(&s, 0), before, "mutated");
    assert_eq!(
        pattern(&s, 0),
        pattern(&twin, 0),
        "the same MUTATE as EDIT first"
    );
}

#[test]
fn trackers_arp_alone_with_rec_on_writes_the_key_off() {
    let mut s = tracker_on(fixed_song());
    s.ui.press(PANEL_BTN_REC);
    settle(&mut s.ui);
    let at = |s: &Song, r: usize| pattern(s, 0)[r][0];
    assert_ne!(at(&s, 0), GT2_KEYOFF as u8);
    s.ui.down(PANEL_BTN_ARP);
    settle(&mut s.ui);
    assert_ne!(at(&s, 0), GT2_KEYOFF as u8, "not at its press");
    s.ui.up(PANEL_BTN_ARP);
    settle(&mut s.ui);
    assert_eq!(at(&s, 0), GT2_KEYOFF as u8, "at its release");
    s.ui.turn(PANEL_ENC_SELECT, 1);
    settle(&mut s.ui);
    let before = pattern(&s, 0);
    assert_ne!(before[1][0], GT2_KEYOFF as u8);
    s.ui.down(PANEL_BTN_ARP);
    s.ui.turn(PANEL_ENC_SELECT, 1);
    settle(&mut s.ui);
    assert_eq!(
        at(&s, 1),
        GT2_KEYOFF as u8,
        "the key off before the knob moved the cursor"
    );
    assert_eq!(s.ui.v.song.row, 2);
    s.ui.down(PANEL_BTN_EDIT);
    s.ui.up(PANEL_BTN_ARP);
    s.ui.up(PANEL_BTN_EDIT);
    settle(&mut s.ui);
    let after = pattern(&s, 0);
    assert_eq!(after[2..], before[2..], "no MUTATE");
    let st = &mut s.f.st;
    st.state = SONG_T_PLAYING as u8;
    st.order_pos = [0, 0, 0];
    st.row = [2, 0, 0];
    s.ui.frame();
    assert!(unsafe { song_view_recording(&s.ui.v.song) } != 0);
    let row = s.ui.v.song.row as usize;
    s.ui.down(PANEL_BTN_ARP);
    settle(&mut s.ui);
    assert_eq!(at(&s, row), GT2_KEYOFF as u8, "at once while recording");
    assert_eq!(s.ui.v.song.gen_.lone.waiting, SCREEN_LONE_NONE as u8);
    s.ui.up(PANEL_BTN_ARP);
}

#[test]
fn trackers_lfo_landing_before_edit_is_the_same_undo_and_no_rest() {
    let rec_on = || {
        let mut s = tracker_on(fixed_song());
        s.ui.press(PANEL_BTN_REC);
        settle(&mut s.ui);
        s
    };
    for lfo_first in [false, true] {
        let mut s = rec_on();
        let before = pattern(&s, 0);
        s.ui.press(PANEL_BTN_ARP);
        settle(&mut s.ui);
        assert_ne!(pattern(&s, 0), before);
        if lfo_first {
            s.ui.down(PANEL_BTN_LFO);
            assert_eq!(
                s.ui.v.song.gen_.lone.waiting, SCREEN_LONE_HELD_BACK as u8,
                "LFO waits for EDIT"
            );
            s.ui.down(PANEL_BTN_EDIT);
        } else {
            s.ui.down(PANEL_BTN_EDIT);
            s.ui.down(PANEL_BTN_LFO);
        }
        s.ui.up(PANEL_BTN_LFO);
        s.ui.up(PANEL_BTN_EDIT);
        settle(&mut s.ui);
        assert_eq!(pattern(&s, 0), before, "undone, lfo_first {lfo_first}");
        assert_eq!(s.ui.v.song.gen_.lone.waiting, SCREEN_LONE_NONE as u8);
    }
}

#[test]
fn trackers_lfo_alone_with_rec_on_writes_the_rest() {
    let mut s = tracker_on(fixed_song());
    s.ui.press(PANEL_BTN_REC);
    settle(&mut s.ui);
    let at = |s: &Song, r: usize| pattern(s, 0)[r][0];
    assert_ne!(at(&s, 0), GT2_REST as u8);
    s.ui.down(PANEL_BTN_LFO);
    settle(&mut s.ui);
    assert_ne!(at(&s, 0), GT2_REST as u8, "not at its press");
    s.ui.up(PANEL_BTN_LFO);
    settle(&mut s.ui);
    assert_eq!(at(&s, 0), GT2_REST as u8, "at its release");
    s.ui.turn(PANEL_ENC_SELECT, 4);
    settle(&mut s.ui);
    assert_ne!(at(&s, 4), GT2_REST as u8);
    s.ui.down(PANEL_BTN_LFO);
    s.ui.turn(PANEL_ENC_SELECT, 1);
    settle(&mut s.ui);
    assert_eq!(at(&s, 4), GT2_REST as u8, "the rest before the knob moved");
    assert_eq!(s.ui.v.song.row, 5);
    s.ui.up(PANEL_BTN_LFO);
    s.ui.v.song.row = 9;
    let st = &mut s.f.st;
    st.state = SONG_T_PLAYING as u8;
    st.order_pos = [0, 0, 0];
    st.row = [2, 0, 0];
    s.ui.frame();
    assert!(unsafe { song_view_recording(&s.ui.v.song) } != 0);
    let row = s.ui.v.song.row as usize;
    assert_ne!(at(&s, row), GT2_REST as u8);
    s.ui.down(PANEL_BTN_LFO);
    settle(&mut s.ui);
    assert_eq!(at(&s, row), GT2_REST as u8, "at once while recording");
    assert_eq!(s.ui.v.song.gen_.lone.waiting, SCREEN_LONE_NONE as u8);
    s.ui.up(PANEL_BTN_LFO);
}

#[test]
fn trackers_frame_shows_the_settings_and_its_knobs_set_them_without_a_roll() {
    let mut s = tracker_on(fixed_song());
    let before = pattern(&s, 0);
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.down(PANEL_BTN_SCL);
    settle(&mut s.ui);
    let top = (TRACKER_VIEW_TOP + TRACKER_VIEW_ROW_CELLS * TRACKER_VIEW_DICE_ROW) as usize;
    assert_eq!(s.ui.small(top, 2, 23), " DICE - LET GO TO ROLL ");
    let line = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
    assert_eq!(
        settings_frame_lines(&s, top),
        [
            line("K1", "DENS", "MEDIUM"),
            line("K2", "RANGE", "2 OCT"),
            line("K3", "SCALE", "MIN PENT"),
            line("K4", "RHYTHM", "STRAIGHT"),
            line("  ", "ROOT", "C-3"),
        ]
    );
    s.ui.turn(PANEL_ENC_K1, 1);
    s.ui.turn(PANEL_ENC_K4, 3);
    settle(&mut s.ui);
    assert_eq!(settings_frame_lines(&s, top)[0].2, "BUSY");
    assert_eq!(settings_frame_lines(&s, top)[3].2, "FOUR");
    assert_eq!(
        tracker_popup(&s, PANEL_ENC_K4),
        Some(("RHYTHM".into(), "FOUR".into(), 3, 4))
    );
    let (d, _) = dice_of(&s);
    assert_eq!((d.density, d.rhythm), (3, 3));
    s.ui.up(PANEL_BTN_SCL);
    s.ui.hold(PANEL_BTN_EDIT, false);
    settle(&mut s.ui);
    assert_eq!(pattern(&s, 0), before, "a turn: no roll");
    assert_ne!(
        s.ui.small(top, 2, 23),
        " DICE - LET GO TO ROLL ",
        "the rows are back"
    );
    assert_eq!(
        tracker_popup(&s, PANEL_ENC_K4),
        None,
        "REC off: the knobs are LIVE's"
    );
}

#[test]
fn a_note_key_with_edit_and_scl_held_sets_the_lanes_root_and_the_roll_still_comes() {
    for tracker in [false, true] {
        let mut s = if tracker {
            tracker_on(fixed_song())
        } else {
            step()
        };
        set_dice_all(
            &mut s,
            DENSITY_FULL,
            SCALE_MINOR_PENTATONIC,
            0,
            RANGE_ONE_OCTAVE,
        );
        let top = if tracker {
            (TRACKER_VIEW_TOP + TRACKER_VIEW_ROW_CELLS * TRACKER_VIEW_DICE_ROW) as usize
        } else {
            STEP_VIEW_HELD_ROW as usize
        };
        let k = if tracker { 9 } else { 3 };
        let want = song_note_of_key(&s, k);
        s.ui.hold(PANEL_BTN_EDIT, true);
        s.ui.down(PANEL_BTN_SCL);
        s.ui.note_keys(1 << k, 1 << k);
        s.ui.note_keys(0, 0);
        settle(&mut s.ui);
        assert_eq!(s.ui.v.song.gen_.root[0], want, "tracker {tracker}");
        let name = s.ui.big(top + 9, 11, 2, 3);
        let names = [
            "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
        ];
        assert_eq!(
            name,
            format!("{}{}", names[usize::from(want % 12)], want / 12),
            "the frame's ROOT"
        );
        s.ui.up(PANEL_BTN_SCL);
        s.ui.hold(PANEL_BTN_EDIT, false);
        settle(&mut s.ui);
        let p = pattern(&s, 0);
        assert!(
            (0..p.len()).all(|r| is_note(p[r][0])),
            "tracker {tracker}: rolled, a note on every row"
        );
        for c in &p {
            let n = c[0] - GT2_FIRSTNOTE as u8;
            assert!(
                n >= want && n < want + 12,
                "tracker {tracker}: {n} is off the root {want}"
            );
        }
        if !tracker {
            assert_eq!(s.ui.v.step.held, STEP_VIEW_NONE as u8, "no step held");
        }
    }
}

#[test]
fn k3_sets_the_selected_lanes_scale_and_each_lane_rolls_in_its_own() {
    let mut s = step();
    s.ui.turn(PANEL_ENC_SELECT, 1);
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.down(PANEL_BTN_SCL);
    s.ui.turn(PANEL_ENC_K3, 3);
    settle(&mut s.ui);
    assert_eq!(
        settings_frame_lines(&s, STEP_VIEW_HELD_ROW as usize)[2].2,
        "MAJOR"
    );
    s.ui.up(PANEL_BTN_SCL);
    s.ui.hold(PANEL_BTN_EDIT, false);
    settle(&mut s.ui);
    let (d, r) = dice_of(&s);
    assert_eq!((d.scale, r), ([0, 3, 0], 1));
    let chunk = snapshot(&s)
        .ext_find(sidou_tests::song::tag(b"DICE"))
        .unwrap();
    assert_eq!(chunk[2], 0, "byte 2 is lane 1's");
    for (lane, scale) in [(1usize, 3usize), (0, 0)] {
        if lane == 0 {
            s.ui.turn(PANEL_ENC_SELECT, -1);
        }
        edit_press(&mut s, PANEL_BTN_SCL);
        let p = pattern(&s, lane as u32);
        let notes: Vec<u8> = p.iter().map(|c| c[0]).filter(|&n| is_note(n)).collect();
        assert!(notes.len() > 8, "lane {lane}");
        for n in notes {
            assert!(
                SCALES[scale].contains(&((n - C3) % 12)),
                "lane {lane}: {n} is out of scale {scale}"
            );
        }
    }
    s.ui.hold(PANEL_BTN_EDIT, true);
    s.ui.down(PANEL_BTN_SCL);
    settle(&mut s.ui);
    assert_eq!(
        settings_frame_lines(&s, STEP_VIEW_HELD_ROW as usize)[2].2,
        "MIN PENT"
    );
    s.ui.turn(PANEL_ENC_SELECT, 1);
    settle(&mut s.ui);
    s.ui.up(PANEL_BTN_SCL);
    s.ui.hold(PANEL_BTN_EDIT, false);
    settle(&mut s.ui);
}

#[test]
fn a_chance_below_one_hundred_is_marked_in_tracker() {
    let mut s = step();
    hold(&mut s, 3);
    s.ui.turn(PANEL_ENC_ALGORITHM, -2);
    release(&mut s);
    s.ui.press(PANEL_BTN_SEQ);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "TRACKER");
    let row_of = |u: usize| 3 + 2 * (u + 4) + 1;
    let col = TRACKER_VIEW_CHANCE_COL as usize;
    assert_eq!(ch(&s.ui, col, row_of(3)), b'%');
    let th = unsafe { &*theme_get(u32::from(s.ui.live().theme)) };
    assert_eq!(
        crate::song_view::attr(&s.ui, col, row_of(3)) & 15,
        th.hot,
        "yellow"
    );
    for u in [0usize, 1, 2, 4, 5, 6, 7] {
        assert_ne!(ch(&s.ui, col, row_of(u)), b'%', "row {u}");
    }
}

fn sweep_song() -> Model {
    let m = loop_song();
    let d = unsafe { gt2_pattern_data(m.ptr(), 0) }.cast_mut();
    unsafe { *d.add(2 * 4 + 2) = 4 };
    unsafe { *d.add(2 * 4 + 3) = 0x22 };
    let file = m.write(false);
    let mut back = Model::host();
    back.read(&file, 0).unwrap();
    back
}

#[test]
fn edit_and_env_writes_a_filter_sweep_into_the_empty_command_cells_every_two_rows() {
    for tracker in [false, true] {
        let mut s = Song::playing(Some(sweep_song()));
        if tracker {
            open_tracker(&mut s.ui);
        } else {
            open_step(&mut s);
        }
        let before = pattern(&s, 0);
        let toast = edit_press_toast(&mut s, PANEL_BTN_ENV).unwrap();
        let p = pattern(&s, 0);
        let up = toast == "SWEEP UP";
        assert!(up || toast == "SWEEP DOWN", "{toast}");
        let values = [0x10u8, 0x5a, 0xa5, 0xf0];
        for (r, c) in p.iter().enumerate() {
            assert_eq!(c[..2], before[r][..2], "row {r}: the note column stays");
            if r == 2 {
                assert_eq!(c[2..], [4, 0x22], "row 2's command stays");
            } else if r % 2 == 0 {
                let v = if up { values[r / 2] } else { values[3 - r / 2] };
                assert_eq!(c[2..], [0x0c, v], "tracker {tracker} row {r}");
            } else {
                assert_eq!(c[2..], before[r][2..], "row {r}: odd rows stay");
            }
        }
        edit_press(&mut s, PANEL_BTN_LFO);
        let undone = pattern(&s, 0);
        assert_eq!(
            (0..8).filter(|&r| undone[r] != p[r]).count(),
            1,
            "one cell back"
        );
    }
}

#[test]
fn a_swept_song_plays_in_goattrackers_player_as_written() {
    let mut s = Song::playing(Some(sweep_song()));
    open_step(&mut s);
    let up = edit_press_toast(&mut s, PANEL_BTN_ENV).as_deref() == Some("SWEEP UP");
    let song = snapshot(&s);
    let sng = song.write(false);
    let (ours, theirs) = (sidou_calls(&song, 200), reference_calls(&sng, 200));
    assert_eq!(ours, theirs);
    let mut cutoffs: Vec<u8> = Vec::new();
    for c in &theirs {
        for &(reg, v) in c {
            if reg == 0x16 && cutoffs.last() != Some(&v) {
                cutoffs.push(v);
            }
        }
    }
    let want: Vec<u8> = if up {
        vec![0x10, 0xa5, 0xf0]
    } else {
        vec![0xf0, 0x5a, 0x10]
    };
    assert!(
        cutoffs.windows(3).any(|w| w == want.as_slice()),
        "cutoffs {cutoffs:02x?}, want {want:02x?} in turn"
    );
}
