// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::lab_sound::{valid, Sound};
use crate::Rig;

const NOTE: u8 = 60;
const TICKS_A_FRAME: u32 = 2;
const SAW: u8 = 0x20;
const PULSE: u8 = 0x40;
const NOISE: u8 = 0x80;
const TRIANGLE: u8 = 0x10;
const GATE: u8 = 0x01;
const UP: fn(u8) -> u8 = |n| n;
const DOWN: fn(u8) -> u8 = |n| (LAB_TABLE_NOTE_NONE as u8).wrapping_sub(n);
const ABSOLUTE: fn(u8) -> u8 = |midi| LAB_TABLE_NOTE_NONE as u8 + midi - 12;

pub(crate) struct Table {
    pub steps: Vec<(u8, u8)>,
    pub loop_from: u8,
    pub frames: u8,
    pub layer: u8,
}

impl Table {
    pub fn once(steps: &[(u8, u8)], frames: u8) -> Self {
        Table {
            steps: steps.to_vec(),
            loop_from: 0,
            frames,
            layer: 0,
        }
    }

    pub fn put(&self, s: &mut Sound) {
        s[LAB_SOUND_TABLE as usize] =
            (LAB_TABLE_ON | u32::from(self.layer) << LAB_TABLE_LAYER_SHIFT) as u8
                | self.steps.len() as u8;
        s[LAB_SOUND_TABLE_LOOP as usize] = self.loop_from;
        s[LAB_SOUND_TABLE_RATE as usize] = self.frames - 1;
        for (n, &(control, note)) in self.steps.iter().enumerate() {
            let at = LAB_SOUND_TABLE_STEPS as usize + 2 * n;
            s[at] = control;
            s[at + 1] = note;
        }
    }
}

fn rig_with(table: &Table, layers: u8) -> Rig {
    let mut rig = Rig::new(SID_MODEL_6581);
    let mut s = rig.sound();
    s[LAB_SOUND_CONTROL as usize] = TRIANGLE;
    s[LAB_SOUND_LAYERS as usize] = layers;
    s[LAB_SOUND_MONO_SYNC as usize] = 0;
    table.put(&mut s);
    assert!(valid(&s));
    unsafe { live_sound_apply(&mut *rig.live, s.as_ptr()) };
    rig
}

fn frames(rig: &mut Rig, n: u32) {
    for _ in 0..n * TICKS_A_FRAME {
        rig.tick();
    }
}

fn freq(note: u8) -> u16 {
    unsafe { live_freq_at(note, 0) }
}

fn heard(rig: &Rig, v: u8) -> (u8, u16) {
    (rig.control(v) & !GATE, rig.voice_freq(v))
}

#[test]
fn a_table_steps_the_waveform_and_pitch_at_its_rate_and_holds_its_last_step() {
    let table = Table::once(&[(SAW, UP(0)), (PULSE, UP(12)), (NOISE, DOWN(12))], 2);
    let mut rig = rig_with(&table, 0);
    rig.note_on(NOTE);
    assert_eq!(heard(&rig, 0), (SAW, freq(NOTE)), "step 1 at the note-on");
    assert!(rig.gated(0), "the gate is the key's");
    frames(&mut rig, 1);
    assert_eq!(heard(&rig, 0), (SAW, freq(NOTE)), "two frames a step");
    frames(&mut rig, 1);
    assert_eq!(heard(&rig, 0), (PULSE, freq(NOTE + 12)));
    frames(&mut rig, 2);
    assert_eq!(heard(&rig, 0), (NOISE, freq(NOTE - 12)));
    frames(&mut rig, 20);
    assert_eq!(
        heard(&rig, 0),
        (NOISE, freq(NOTE - 12)),
        "the last step holds"
    );
}

#[test]
fn a_loop_goes_back_to_its_step_and_runs_on_through_the_release() {
    let mut table = Table::once(&[(NOISE, UP(0)), (PULSE, UP(0)), (PULSE, UP(7))], 1);
    table.loop_from = 2;
    let mut rig = rig_with(&table, 0);
    rig.note_on(NOTE);
    let mut seen = Vec::new();
    for _ in 0..6 {
        seen.push(heard(&rig, 0));
        frames(&mut rig, 1);
    }
    let (n, p, p7) = (
        (NOISE, freq(NOTE)),
        (PULSE, freq(NOTE)),
        (PULSE, freq(NOTE + 7)),
    );
    assert_eq!(seen, [n, p, p7, p, p7, p]);
    rig.note_off(NOTE);
    assert!(!rig.gated(0));
    let before = heard(&rig, 0);
    frames(&mut rig, 1);
    assert_ne!(heard(&rig, 0), before, "the table runs on after the key");
}

#[test]
fn an_absolute_step_plays_its_own_note_whatever_the_key() {
    let table = Table::once(&[(NOISE, ABSOLUTE(84)), (TRIANGLE, UP(0))], 1);
    for key in [48, 60, 72] {
        let mut rig = rig_with(&table, 0);
        rig.note_on(key);
        assert_eq!(heard(&rig, 0), (NOISE, freq(84)), "key {key}");
        frames(&mut rig, 1);
        assert_eq!(heard(&rig, 0), (TRIANGLE, freq(key)));
    }
}

#[test]
fn each_note_of_a_chord_runs_its_own_table_from_its_note_on() {
    let table = Table::once(&[(SAW, UP(0)), (PULSE, UP(0))], 1);
    let mut rig = rig_with(&table, 0);
    rig.note_on(NOTE);
    frames(&mut rig, 1);
    rig.note_on(NOTE + 4);
    let late = (0..3u8)
        .find(|&v| rig.voice_freq(v) == freq(NOTE + 4))
        .unwrap();
    let early = (0..3u8).find(|&v| rig.voice_freq(v) == freq(NOTE)).unwrap();
    assert_eq!(heard(&rig, early).0, PULSE, "the first note is on step 2");
    assert_eq!(heard(&rig, late).0, SAW, "the second starts at step 1");
}

#[test]
fn on_a_layered_sound_the_table_plays_on_its_layer_alone() {
    let mut table = Table::once(&[(NOISE, UP(12)), (PULSE, UP(0))], 1);
    table.layer = 1;
    let mut rig = rig_with(&table, 2);
    rig.note_on(NOTE);
    assert_eq!(heard(&rig, 1), (NOISE, freq(NOTE + 12)), "layer 2 runs it");
    assert_eq!(heard(&rig, 0).0, TRIANGLE, "layer 1 keeps its own");
    frames(&mut rig, 1);
    assert_eq!(heard(&rig, 1), (PULSE, freq(NOTE)));
    assert_eq!(heard(&rig, 0), (TRIANGLE, freq(NOTE)));
    table.layer = 2;
    let mut rig = rig_with(&table, 1);
    rig.note_on(NOTE);
    assert_eq!(
        heard(&rig, 0).0,
        TRIANGLE,
        "a table on a layer the sound does not play is not heard"
    );
}

#[test]
fn a_sound_with_its_table_off_plays_as_before() {
    let table = Table::once(&[(NOISE, UP(12))], 1);
    let mut rig = rig_with(&table, 0);
    let mut s = rig.sound();
    s[LAB_SOUND_TABLE as usize] &= !(LAB_TABLE_ON as u8);
    unsafe { live_sound_apply(&mut *rig.live, s.as_ptr()) };
    rig.note_on(NOTE);
    frames(&mut rig, 3);
    assert_eq!(heard(&rig, 0), (TRIANGLE, freq(NOTE)));
}
