// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::drain;
use crate::sid_view::golden;
use crate::song_view::{settle, Call, Song};
use crate::views::Ui;

const SONG_VIEWS: [&str; 5] = ["STEP", "TRACKER", "ORDER", "INSTR", "TABLE"];
const BEAT_AT_SPEED_6: u32 = 4 * 6;
const PAD_KEYS: u32 = PUNCH_VIEW_ALL;
const OTHER_KEYS: u32 = ((1 << PANEL_NOTE_KEYS) - 1) & !PAD_KEYS;

fn to_song_view(ui: &mut Ui, name: &str) {
    for _ in 0..=SONG_VIEWS.len() {
        if ui.top() == name {
            return;
        }
        ui.press(PANEL_BTN_SEQ);
        settle(ui);
    }
    panic!("no song view {name}");
}

fn punch_over(s: &mut Song, view: &str) {
    to_song_view(&mut s.ui, view);
    s.ui.press(PANEL_BTN_FX);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "PUNCH", "FX on {view}");
}

fn playing_on_punch() -> Song {
    let mut s = Song::new();
    punch_over(&mut s, "STEP");
    s.ui.press(PANEL_BTN_PLAY);
    settle(&mut s.ui);
    s.calls();
    s
}

fn hold(s: &mut Song, keys: u32) -> Vec<Call> {
    s.ui.note_keys(keys, keys);
    s.ui.frame();
    s.calls()
}

fn punches(calls: &[Call]) -> Vec<(u32, u32)> {
    calls
        .iter()
        .filter_map(|c| match *c {
            Call::Punch(p, b) => Some((p, b)),
            _ => None,
        })
        .collect()
}

fn leds(s: &mut Song) -> panel_leds {
    let mut i = panel_led_in::default();
    unsafe {
        views_leds(&mut *s.ui.v, s.ui.now, &mut i);
        panel_get_leds(&*s.ui.p, &*s.ui.rig.live, &i)
    }
}

#[test]
fn fx_on_every_song_view_opens_punch_and_home_goes_back_with_the_song_playing_on() {
    let mut s = Song::new();
    for view in SONG_VIEWS {
        punch_over(&mut s, view);
        s.ui.press(PANEL_BTN_HOME);
        settle(&mut s.ui);
        assert_eq!(s.ui.top(), view, "HOME from PUNCH over {view}");
        assert!(
            !s.calls().iter().any(|c| matches!(c, Call::Stop)),
            "{view}: the song is not stopped"
        );
    }
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "LIVE");
    s.ui.press(PANEL_BTN_FX);
    settle(&mut s.ui);
    assert_ne!(s.ui.top(), "PUNCH", "LIVE's FX opens LAB as before");
}

#[test]
fn held_keys_one_to_six_are_the_pads_several_at_once_and_each_lets_go_on_release() {
    let mut s = playing_on_punch();
    let both = 1 << 0 | 1 << 3;
    assert_eq!(punches(&hold(&mut s, both)), [(both, BEAT_AT_SPEED_6)]);
    let all = PAD_KEYS;
    assert_eq!(punches(&hold(&mut s, all)), [(all, BEAT_AT_SPEED_6)]);
    assert_eq!(punches(&hold(&mut s, all)), [], "nothing new while held");
    assert_eq!(punches(&hold(&mut s, 1 << 5)), [(1 << 5, BEAT_AT_SPEED_6)]);
    assert_eq!(punches(&hold(&mut s, 0)), [(0, BEAT_AT_SPEED_6)]);
}

#[test]
fn the_other_keys_play_nothing_and_send_nothing_on_punch() {
    let mut s = playing_on_punch();
    let calls = hold(&mut s, OTHER_KEYS);
    assert_eq!(calls, [], "no pad, no jam");
    assert!(
        s.ui.live().voice.iter().all(|v| v.gate == 0),
        "no key sounds on PUNCH"
    );
    assert_eq!(
        s.ui.big(PUNCH_VIEW_HINT_ROW as usize, 2, 2, 13),
        "KEYS ARE PADS"
    );
    hold(&mut s, 0);
    s.ui.down(PANEL_BTN_GLO);
    assert_eq!(punches(&hold(&mut s, 1 << 1)), [(1 << 1, BEAT_AT_SPEED_6)]);
    s.ui.up(PANEL_BTN_GLO);
    assert_eq!(s.ui.v.song.mute, 0, "GLO + a pad mutes no voice");
}

#[test]
fn stopped_the_pads_do_nothing_and_the_page_says_play_to_punch() {
    let mut s = Song::new();
    punch_over(&mut s, "TRACKER");
    assert_eq!(punches(&hold(&mut s, 1 << 2)), [], "stopped");
    settle(&mut s.ui);
    assert_eq!(
        s.ui.big(PUNCH_VIEW_HINT_ROW as usize, 2, 2, 13),
        "PLAY TO PUNCH"
    );
    golden(&s.ui, "punch-stopped");
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.frame();
    assert_eq!(
        punches(&s.calls()),
        [(1 << 2, BEAT_AT_SPEED_6)],
        "the held pad punches once the song plays"
    );
    s.ui.press(PANEL_BTN_PLAY);
    s.ui.frame();
    assert_eq!(
        punches(&s.calls()),
        [(0, BEAT_AT_SPEED_6)],
        "stopping lets it go"
    );
}

#[test]
fn leaving_punch_with_pads_held_lets_them_go() {
    for leave in [PANEL_BTN_HOME, PANEL_BTN_SEQ] {
        let mut s = playing_on_punch();
        hold(&mut s, 1 << 4 | 1 << 1);
        s.ui.press(leave);
        settle(&mut s.ui);
        assert_ne!(s.ui.top(), "PUNCH");
        assert!(
            punches(&s.calls()).last() == Some(&(0, BEAT_AT_SPEED_6)),
            "button {leave}: the pads let go"
        );
    }
}

#[test]
fn seq_on_punch_opens_the_song_view_used_last_over_it_as_from_any_view() {
    let mut s = Song::new();
    punch_over(&mut s, "ORDER");
    s.ui.press(PANEL_BTN_SEQ);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "ORDER");
    s.ui.press(PANEL_BTN_HOME);
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "PUNCH", "HOME comes back");
}

#[test]
fn a_held_step_and_fx_still_set_its_slide_and_open_no_punch() {
    let mut s = crate::step_view::step();
    crate::step_view::hold(&mut s, 2);
    s.ui.press(PANEL_BTN_FX);
    assert_eq!(s.ui.toast().as_deref(), Some("SLIDE ON"));
    settle(&mut s.ui);
    assert_eq!(s.ui.top(), "STEP");
}

#[test]
fn the_held_pads_light_their_keys_and_the_other_pads_glow_dim() {
    let mut s = playing_on_punch();
    hold(&mut s, 1 << 2);
    let l = leds(&mut s);
    assert_eq!(l.notes & PAD_KEYS, 1 << 2, "the held pad lit");
    assert_eq!(
        l.notes_dim & PAD_KEYS,
        PAD_KEYS & !(1 << 2),
        "the others dim"
    );
    assert_eq!((l.notes | l.notes_dim) & OTHER_KEYS, 0, "no other key lit");
}

#[test]
fn the_page_shows_each_pad_big_and_lights_the_held_ones() {
    let mut s = playing_on_punch();
    settle(&mut s.ui);
    for pad in 0..SONG_T_PUNCH_PADS {
        let name = unsafe { std::ffi::CStr::from_ptr(punch_view_pad_name(pad)) }
            .to_string_lossy()
            .into_owned();
        let row = (PUNCH_VIEW_PAD_ROW + PUNCH_VIEW_PAD_STEP * pad) as usize;
        assert_eq!(
            s.ui.big(row, 2, 2, 2 + name.len()),
            format!("{} {name}", pad + 1)
        );
    }
    golden(&s.ui, "punch");
    hold(&mut s, 1 << 1 | 1 << 3);
    settle(&mut s.ui);
    golden(&s.ui, "punch-held");
}

#[test]
fn every_change_of_the_pads_and_the_transport_fits_the_frame_budget() {
    let mut s = playing_on_punch();
    settle(&mut s.ui);
    let steps: [&dyn Fn(&mut Song); 4] = [
        &|s| s.ui.note_keys(PAD_KEYS, PAD_KEYS),
        &|s| s.ui.note_keys(0, 0),
        &|s| s.ui.press(PANEL_BTN_PLAY),
        &|s| s.ui.press(PANEL_BTN_PLAY),
    ];
    for (k, step) in steps.iter().enumerate() {
        step(&mut s);
        let mut frames = 0;
        loop {
            let sent = s.ui.frame_sent();
            assert!(
                sent.cells <= PUNCH_VIEW_FRAME_CELLS as usize,
                "step {k}, frame {frames}: {sent:?}"
            );
            frames += 1;
            if sent.cells == 0 {
                break;
            }
            assert!(frames < 10, "step {k} kept drawing");
        }
    }
    drain(&mut s.ui.g);
}
