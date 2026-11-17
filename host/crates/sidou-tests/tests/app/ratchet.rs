// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::song::{SngSpec, Song as Model, REST as R};

use crate::song_view::{ch, Song};
use crate::step_view::{
    groove, groove_turn, hold, leave_groove, loop_song, open_step, pattern, reference_calls,
    release, row_start, sidou_calls, snapshot, tap, voice_1_control_after_each_call, C3,
};

const LEAD: u8 = 1;
const GATE: u8 = 1;
const PULSE_GATED: u8 = 0x41;
const NO_CHANGE: u8 = 0x80;
const JUMP: u8 = 0xff;
const STEP_COL: usize = STEP_VIEW_STEP_COL as usize;
const NOTCH_ROW: usize = STEP_VIEW_LANES_ROW as usize + 1;

fn stepping(m: Model) -> Song {
    let mut s = Song::playing(Some(m));
    open_step(&mut s);
    s
}

fn ratchet(s: &mut Song, step: u32, turn: i32) -> String {
    groove(s);
    tap(s, step);
    let (name, value) = groove_turn(s, PANEL_ENC_K4, turn);
    assert_eq!(name, "RATCHET");
    leave_groove(s);
    value
}

fn instrument(s: &Song, i: u8) -> [u8; GT2_INSTR_BYTES as usize] {
    unsafe { (*s.ui.v.song.song).instr[usize::from(i) - 1] }
}

fn wave_rows(s: &Song, from: u8, n: usize) -> Vec<(u8, u8)> {
    let song = unsafe { &*s.ui.v.song.song };
    let t = GT2_WTBL as usize;
    (0..n)
        .map(|k| {
            let r = usize::from(from) - 1 + k;
            (song.ltable[t][r], song.rtable[t][r])
        })
        .collect()
}

fn gate_onsets(control: &[u8], from: usize, to: usize) -> usize {
    (from..to)
        .filter(|&k| control[k] & GATE == 1 && control[k - 1] & GATE == 0)
        .count()
}

#[test]
fn a_ratcheted_step_plays_a_shared_copy_that_gates_the_note_again_as_goattracker_does() {
    let mut s = stepping(loop_song());
    assert_eq!(ratchet(&mut s, 0, 1), "x2");
    let copy = pattern(&s, 0)[0][1];
    assert_ne!(copy, LEAD, "the step plays a copy");
    let (lead, made) = (instrument(&s, LEAD), instrument(&s, copy));
    let sound = |i: &[u8; 25]| {
        let mut b = *i;
        b[GT2_INS_WAVE as usize] = 0;
        b[..GT2_INS_GATETIMER as usize].to_vec()
    };
    assert_eq!(sound(&made), sound(&lead), "the copy is the same sound");
    assert_eq!(
        made[GT2_INS_GATETIMER as usize..GT2_INS_NAME as usize],
        [1, PULSE_GATED],
        "its first frame is the first hit, its gate cleared one frame before the next note"
    );
    assert_eq!(
        wave_rows(&s, made[GT2_INS_WAVE as usize], 4),
        [
            (0, NO_CHANGE),
            (PULSE_GATED & !GATE, 0),
            (PULSE_GATED, 0),
            (JUMP, lead[GT2_INS_WAVE as usize])
        ]
    );
    assert_eq!(
        ch(&s.ui, STEP_COL, NOTCH_ROW),
        FONT_TICK2 as u8,
        "two ticks on the step"
    );
    tap(&mut s, 2);
    assert_eq!(ratchet(&mut s, 2, 2), "x3");
    assert_eq!(ch(&s.ui, STEP_COL + 2, NOTCH_ROW), FONT_TICK3 as u8);
    let song = snapshot(&s);
    let sng = song.write(false);
    let calls = row_start(8) + 1;
    let ours = sidou_calls(&song, calls);
    assert_eq!(
        ours,
        reference_calls(&sng, calls),
        "GoatTracker plays it the same"
    );
    let control = voice_1_control_after_each_call(&ours);
    assert_eq!(
        gate_onsets(&control, row_start(0), row_start(1)),
        2,
        "{control:?}"
    );
    assert_eq!(
        gate_onsets(&control, row_start(2), row_start(3)),
        3,
        "{control:?}"
    );
    assert_eq!(ratchet(&mut s, 2, -1), "x2");
    assert_eq!(
        pattern(&s, 0)[2][1],
        copy,
        "one copy for each instrument and count"
    );
}

#[test]
fn x4_is_refused_where_a_row_is_too_short_and_off_and_clear_give_the_step_back_its_instrument() {
    let mut s = stepping(loop_song());
    assert_eq!(ratchet(&mut s, 0, 3), "NO x4");
    assert_eq!(
        ch(&s.ui, STEP_COL, NOTCH_ROW),
        FONT_TICK3 as u8,
        "the last count given plays"
    );
    assert_eq!(ratchet(&mut s, 0, -2), "OFF");
    assert_eq!(
        pattern(&s, 0)[0][1],
        LEAD,
        "OFF is the lane's own instrument"
    );
    ratchet(&mut s, 0, 1);
    hold(&mut s, 0);
    s.ui.press(PANEL_BTN_LFO);
    release(&mut s);
    assert_eq!(
        pattern(&s, 0)[0][1],
        LEAD,
        "the clear takes the ratchet too"
    );
}

fn every_slot_used() -> Model {
    let mut spec = SngSpec::new();
    let mut ins = [0u8; 25];
    ins[..9].copy_from_slice(&[0x09, 0xa0, 1, 0, 0, 0, 0, 2, 0x09]);
    spec.instr = vec![ins; GT2_MAX_INSTR as usize];
    spec.tables = vec![vec![(PULSE_GATED, 0), (JUMP, 0)], vec![], vec![], vec![]];
    let mut p0 = vec![R; 8];
    p0[0] = [C3, LEAD, 0, 0];
    spec.patterns = vec![p0, vec![R; 8], vec![R; 8]]
        .into_iter()
        .map(|p| (p, [0xff, 0, 0, 0]))
        .collect();
    let mut m = Model::host();
    m.read(&spec.encode(), 0).unwrap();
    m
}

#[test]
fn with_every_instrument_slot_used_a_ratchet_is_refused_with_a_toast() {
    let mut s = stepping(every_slot_used());
    groove(&mut s);
    tap(&mut s, 0);
    s.ui.turn(PANEL_ENC_K4, 1);
    assert_eq!(s.ui.toast().as_deref(), Some("INSTRUMENTS FULL"));
    assert_eq!(pattern(&s, 0)[0][1], LEAD);
}
