// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The button LEDs' inputs from the views (`views_leds`, app/views.c): the song's beat, REC, the
//! voices GLO's layer shows, the question that waits for SAVE, and STEP's page. The lights those
//! inputs make are panel.rs's.

use sidou_sys::*;

use crate::song_view::Song;
use crate::step_view::{open_step, step};

fn inputs(s: &mut Song, ms: u32) -> panel_led_in {
    let mut i = panel_led_in::default();
    unsafe { views_leds(&mut *s.ui.v, ms, &mut i) };
    i
}

fn leds(s: &mut Song, ms: u32) -> panel_leds {
    let i = inputs(s, ms);
    unsafe { panel_get_leds(&*s.ui.p, &*s.ui.rig.live, &i) }
}

const fn bit(b: u32) -> u32 {
    1 << b
}

#[test]
fn the_beat_comes_from_the_players_clock_in_any_view() {
    let mut s = Song::new();
    s.ui.frame();
    assert_eq!(s.ui.top(), "LIVE", "not a song view");
    s.f.st.state = SONG_T_PLAYING as u8;
    s.ui.press(PANEL_BTN_OCT_UP); // an event: the views know the panel
    s.ui.p.playing = 1;
    // 4 rows a beat of speed 6: 24 calls; the first quarter is 6 calls.
    for (calls, lit) in [
        (0, true),
        (5, true),
        (6, false),
        (23, false),
        (24, true),
        (30, false),
    ] {
        s.f.clock_calls = calls;
        let l = leds(&mut s, 0);
        assert_eq!(l.buttons >> PANEL_BTN_PLAY & 1, lit as u32, "call {calls}");
        assert_eq!(
            l.buttons_dim >> PANEL_BTN_PLAY & 1,
            !lit as u32,
            "call {calls}"
        );
    }
    // Funktempo 5/7: a beat is 4 rows of 6 on average, still 24 calls.
    s.f.st.speed = [5, 7];
    s.f.clock_calls = 24;
    assert_eq!(inputs(&mut s, 0).beat, 0);
    // The rows a beat are the views' (the song's setup).
    s.ui.v.song.rows_beat = 8;
    s.f.clock_calls = 24;
    assert_eq!(inputs(&mut s, 0).beat, 128);
    // Stopped: no beat, no light.
    s.ui.p.playing = 0;
    s.f.st.state = SONG_T_STOPPED as u8;
    let l = leds(&mut s, 0);
    assert_eq!((l.buttons | l.buttons_dim) & bit(PANEL_BTN_PLAY), 0);
}

#[test]
fn rec_dim_when_armed_blinking_in_the_count_in_lit_recording() {
    let mut s = step();
    s.ui.press(PANEL_BTN_HOME);
    s.ui.settle();
    assert_ne!(s.ui.top(), "STEP");
    let rec = bit(PANEL_BTN_REC);
    assert_eq!(inputs(&mut s, 0).rec, PANEL_REC_OFF as u8);
    s.ui.press(PANEL_BTN_REC);
    s.ui.frame();
    assert_eq!(s.ui.v.song.rec, 1);
    assert_eq!(
        inputs(&mut s, 0).rec,
        PANEL_REC_ARMED as u8,
        "outside the song views"
    );
    open_step(&mut s);
    assert_eq!(
        inputs(&mut s, 0).rec,
        PANEL_REC_ARMED as u8,
        "the song is stopped"
    );
    s.ui.v.song.st.state = SONG_T_PLAYING as u8;
    s.ui.v.song.counting = 0;
    assert_eq!(inputs(&mut s, 0).rec, PANEL_REC_ON as u8);
    assert_eq!(leds(&mut s, 0).buttons & rec, rec);
    s.ui.v.song.counting = 1;
    s.ui.v.song.st.now_call = 10;
    s.ui.v.song.count_end = 50;
    assert_eq!(inputs(&mut s, 0).rec, PANEL_REC_COUNT as u8);
    s.ui.v.song.st.now_call = 50;
    assert_eq!(
        inputs(&mut s, 0).rec,
        PANEL_REC_ON as u8,
        "the count-in ran out"
    );
}

#[test]
fn glo_inputs_are_the_mutes_and_solos() {
    let mut s = Song::new();
    s.ui.v.song.mute = 0b001;
    let i = inputs(&mut s, 0);
    assert_eq!((i.silent, i.solo), (0b001, 0));
    s.ui.v.song.solo = 0b100;
    let i = inputs(&mut s, 0);
    assert_eq!(
        (i.silent, i.solo),
        (0b011, 0b100),
        "a solo silences the others"
    );
}

#[test]
fn a_question_waits_for_save_in_settings_store() {
    let mut s = Song::new();
    assert_eq!(inputs(&mut s, 0).ask, 0);
    crate::views::open_setting(&mut s.ui, VIEWS_SETTINGS_STORE);
    s.ui.frame();
    assert_eq!(inputs(&mut s, 0).ask, 0, "not armed");
    s.ui.v.format_arm = 1;
    assert_eq!(inputs(&mut s, 0).ask, 1, "FORMAT? waits for SAVE");
    s.ui.v.format_arm = 0;
    assert_eq!(inputs(&mut s, 0).ask, 0);
}

#[test]
fn step_page_and_playhead_follow_the_selected_lane() {
    let mut s = step();
    let i = inputs(&mut s, 1000);
    assert_eq!(i.step_on, 1);
    assert_eq!(i.step_rows, 0xffff);
    assert_eq!(i.step_set, 1 | 1 << 4, "voice 1's notes at steps 1 and 5");
    assert_eq!(i.step_head, PANEL_NO_STEP as u8, "stopped");
    let l = leds(&mut s, 1000);
    assert_eq!(l.notes, 1 | 1 << 4);
    assert_eq!(l.notes_dim, 0xffff & !(1 | 1 << 4));
    // The song plays row 4 of voice 1: its key is the playhead, inverted when it moves there.
    s.f.st.state = SONG_T_PLAYING as u8;
    s.f.st.order_pos = [0, 0, 0];
    s.f.st.row = [4, 0, 0];
    s.ui.frame();
    let i = inputs(&mut s, 2000);
    assert_eq!(i.step_head, 4);
    assert_eq!(i.head_ms, 2000);
    let l = leds(&mut s, 2000 + 10);
    assert_eq!(l.notes & (1 << 4), 0, "a set step flicks dark");
    let l = leds(&mut s, 2000 + PANEL_FLICK_MS + 5);
    assert_eq!(l.notes & (1 << 4), 1 << 4, "and back");
    // It moves on to an empty step: that one lights, the old one is as it was.
    s.f.st.row = [5, 0, 0];
    s.ui.frame();
    let i = inputs(&mut s, 3000);
    assert_eq!((i.step_head, i.head_ms), (5, 3000));
    let l = leds(&mut s, 3010);
    assert_eq!(l.notes & (1 << 5), 1 << 5);
    // Another voice's lane: its pattern's notes.
    s.ui.v.song.voice = 1;
    let i = inputs(&mut s, 4000);
    assert_eq!(i.step_set, 0, "voice 2's pattern is empty");
    // Out of the view the page is gone.
    s.ui.press(PANEL_BTN_HOME);
    s.ui.settle();
    assert_ne!(s.ui.top(), "STEP");
    assert_eq!(inputs(&mut s, 5000).step_on, 0);
}
