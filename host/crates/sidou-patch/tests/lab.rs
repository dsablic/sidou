// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::*;
use sidou_patch::lab::{lab_sound, Loss};
use sidou_patch::parse_bank;
use sidou_sys::*;

const SINGLE_ALL: u8 = 0b0000_0111;
const POLY_FIRST: u8 = 0b0000_1001;
const SAW: u8 = 0x20;
const PULSE: u8 = 0x40;
const TRIANGLE_RING: u8 = 0x14;

fn mapped(decoded: Decoded) -> (Vec<u8>, Vec<Loss>) {
    let patch = parse_bank(&dump(&PREFIX_LIBRARIAN, 0, &decoded))
        .unwrap()
        .remove(0);
    let (sound, losses) = lab_sound(&patch);
    assert!(unsafe { lab_sound_valid(sound.as_ptr()) } != 0, "{sound:?}");
    (sound.to_vec(), losses)
}

fn voiced(
    decoded: Decoded,
    n: usize,
    control: u8,
    adsr: [u8; 4],
    width: u8,
    semis: u8,
    fine: u8,
) -> Decoded {
    decoded
        .osc(n, OSC_WAVE, control)
        .osc(n, OSC_ATTACK, adsr[0])
        .osc(n, OSC_DECAY, adsr[1])
        .osc(n, OSC_SUSTAIN, adsr[2])
        .osc(n, OSC_RELEASE, adsr[3])
        .osc(n, OSC_PULSE_WIDTH, width)
        .osc(n, OSC_TRANSPOSE, semis)
        .osc(n, OSC_DETUNE, fine)
}

fn three() -> Decoded {
    let d = Decoded::new("Layered").set(PATCH_MODE, SINGLE_ALL);
    let d = voiced(d, 0, SAW, [1, 2, 10, 3], 64, 0, 0);
    let d = voiced(d, 1, PULSE, [4, 5, 8, 6], 32, 12, 0x10);
    voiced(d, 2, TRIANGLE_RING, [7, 8, 5, 9], 16, 0xF4, 0xF0)
}

fn layer_tuning(s: &[u8], n: usize) -> (i8, i8) {
    let at = (LAB_SOUND_LAYER_TUNING + n as u32 * LAB_SOUND_LAYER_TUNING_BYTES) as usize;
    (s[at] as i8, s[at + 1] as i8)
}

fn layer_voice(s: &[u8], n: usize) -> [u8; 5] {
    let at = (LAB_SOUND_LAYER_VOICE + (n as u32 - 1) * LAB_SOUND_LAYER_VOICE_BYTES) as usize;
    s[at..at + 5].try_into().unwrap()
}

#[test]
fn single_mode_plays_each_active_oscillator_as_a_layer() {
    let (s, losses) = mapped(three());
    assert_eq!(s[LAB_SOUND_LAYERS as usize], 2);
    assert_eq!(s[LAB_SOUND_MONO_SYNC as usize], 0);
    assert_eq!(s[LAB_SOUND_CONTROL as usize], SAW);
    assert_eq!(
        (s[LAB_SOUND_AD as usize], s[LAB_SOUND_SR as usize]),
        (0x12, 0xa3)
    );
    assert_eq!(u16::from_le_bytes([s[3], s[4]]), 0x800);
    assert_eq!(layer_voice(&s, 1), [PULSE, 0x45, 0x86, 0x00, 0x04]);
    assert_eq!(layer_voice(&s, 2), [TRIANGLE_RING, 0x78, 0x59, 0x00, 0x02]);
    assert_eq!(layer_tuning(&s, 0), (0, 0));
    assert_eq!(
        layer_tuning(&s, 1),
        (12, 25),
        "16 64ths of a semitone are 25 cents"
    );
    assert_eq!(layer_tuning(&s, 2), (-12, -25));
    assert!(losses.is_empty(), "{losses:?}");
}

#[test]
fn poly_mode_and_inactive_oscillators_leave_layers_out() {
    let (s, _) = mapped(three().set(PATCH_MODE, POLY_FIRST));
    assert_eq!(
        s[LAB_SOUND_LAYERS as usize], 0,
        "poly: one oscillator, three notes"
    );

    let (s, _) = mapped(three().set(PATCH_MODE, 0b101).set(FILTER_ROUTING, 0b100));
    assert_eq!(s[LAB_SOUND_LAYERS as usize], 1);
    assert_eq!(
        layer_voice(&s, 1)[0],
        TRIANGLE_RING,
        "oscillator 3 is layer 2"
    );
    assert_eq!(
        s[LAB_SOUND_FILTER_VOICES as usize], 0b10,
        "its filter routing follows it"
    );
}

#[test]
fn the_filter_maps_its_type_cutoff_and_resonance() {
    let (s, _) = mapped(
        three()
            .set(FILTER_TYPE_LFO, 0x02)
            .set(FILTER_CUTOFF, 127)
            .set(FILTER_ROUTING, 0xF1),
    );
    assert_eq!(s[LAB_SOUND_FILTER_MODE as usize], 0x20, "band-pass");
    assert_eq!(u16::from_le_bytes([s[5], s[6]]), 127 * 16);
    assert_eq!(s[LAB_SOUND_RESONANCE as usize], 15);
    assert_eq!(s[LAB_SOUND_FILTER_VOICES as usize], 0b001);
}

#[test]
fn what_lab_cannot_hold_is_reported() {
    let cases: Vec<(&str, Decoded, Loss)> = vec![
        (
            "a second oscillator's table",
            three()
                .osc(0, OSC_TABLE_SPEED, 2)
                .osc(1, OSC_TABLE_SPEED, 2)
                .tables(&[&[0x40, 0x1E, 0xFF], &[0x20, 0x30, 0xFF]]),
            Loss::Table(2),
        ),
        (
            "a table of more than sixteen steps",
            three().osc(0, OSC_TABLE_SPEED, 2).tables(&[&long_table()]),
            Loss::TableCut(1),
        ),
        (
            "a fixed note",
            three().osc(1, OSC_TRACK, 40),
            Loss::FixedNote(2),
        ),
        (
            "a transpose past two octaves",
            three().osc(2, OSC_TRANSPOSE, 36),
            Loss::TransposeClamped(3),
        ),
        (
            "no active oscillator",
            three().set(PATCH_MODE, 0),
            Loss::NoOscillator,
        ),
    ];
    for (what, decoded, loss) in cases {
        let (_, losses) = mapped(decoded);
        assert!(losses.contains(&loss), "{what}: {losses:?}");
    }
}

#[test]
fn a_loss_reads_as_a_short_line() {
    assert_eq!(Loss::Table(2).to_string(), "OSC 2 TABLE: SONG INSTRUMENT");
    assert_eq!(Loss::TableCut(1).to_string(), "OSC 1 TABLE CUT TO 16");
    assert_eq!(
        Loss::FixedNote(3).to_string(),
        "OSC 3 FIXED NOTE: PLAYS THE KEY"
    );
}

#[test]
fn the_filter_envelope_maps_its_depth_direction_and_rates() {
    let envelope = |d: Decoded| {
        d.set(FILTER_ENV_DEPTH, 100)
            .set(FILTER_ENV_DEPTH + 1, 16)
            .set(FILTER_ENV_DEPTH + 2, 127)
            .set(FILTER_ENV_DEPTH + 3, 64)
            .set(FILTER_ENV_DEPTH + 4, 8)
    };
    let (s, _) = mapped(envelope(three()));
    assert_eq!(s[LAB_SOUND_FILTER_ENV as usize] as i8, 50);
    assert_eq!(s[LAB_SOUND_FILTER_ENV_AD as usize], 0x2f);
    assert_eq!(s[LAB_SOUND_FILTER_ENV_SR as usize], 0x81);
    let inverted = envelope(three()).set(PATCH_MODE, SINGLE_ALL | 0x80);
    let (s, _) = mapped(inverted);
    assert_eq!(
        s[LAB_SOUND_FILTER_ENV as usize] as i8, -50,
        "an inverted envelope sweeps down"
    );
}

fn long_table() -> Vec<u8> {
    let mut t = [0x41u8, 0xC0].repeat(18);
    t.push(0xFF);
    t
}

fn table_of(s: &[u8]) -> &[u8] {
    &s[LAB_SOUND_TABLE as usize..][..LAB_SOUND_TABLE_BYTES as usize]
}

#[test]
fn the_first_tabled_oscillator_keeps_its_table_on_the_lab_sound() {
    let (s, losses) = mapped(
        three()
            .osc(1, OSC_TABLE_SPEED, 4)
            .set(SYNC_SPEED, 100)
            .tables(&[&[0xFF], &[0x81, 0x3C, 0x40, 0xC7, 0x20, 0x83, 0xFE, 0x01]]),
    );
    let t = table_of(&s);
    assert_eq!(
        t[0],
        (LAB_TABLE_ON | 1 << LAB_TABLE_LAYER_SHIFT | 3) as u8,
        "on, on layer 2 (oscillator 2), three steps"
    );
    assert_eq!(t[1], 2, "loops back to the second step");
    assert_eq!(t[2], 1, "speed 4 at 100 Hz is two frames of 50 Hz");
    assert_eq!(
        &t[3..9],
        &[0x80, LAB_TABLE_NOTE_NONE as u8 + 60 - 12, 0x40, 7, 0x20, 0x80 - 3],
        "the gate left to the key; an absolute C-4, +7, -3 (the layer's own transpose stays on the layer)"
    );
    assert!(
        !losses.iter().any(|l| matches!(l, Loss::Table(_))),
        "{losses:?}"
    );
}

#[test]
fn a_table_that_does_not_play_is_neither_kept_nor_reported() {
    let (s, losses) = mapped(three().tables(&[&[0x40, 0x1E, 0xFF]]));
    assert_eq!(
        table_of(&s)[0],
        0,
        "speed 0: the SidStation does not run it"
    );
    assert!(!losses.iter().any(|l| matches!(l, Loss::Table(_))));
}

fn lfo_of(s: &[u8], n: usize) -> [u8; 3] {
    let at = (LAB_SOUND_LFO + n as u32 * LAB_SOUND_LFO_BYTES) as usize;
    s[at..at + 3].try_into().unwrap()
}

#[test]
fn the_first_oscillators_vibrato_pwm_and_filter_lfos_take_labs_two() {
    const VIBRATO_LFO: usize = 17;
    const VIBRATO_DEPTH: usize = 18;
    const PWM_LFO: usize = 13;
    const PWM_DEPTH: usize = 14;
    const LFO_DEPTH_FIELD: usize = 4;
    let d = three()
        .osc(0, VIBRATO_LFO, 1)
        .osc(0, VIBRATO_DEPTH, 127)
        .osc(0, PWM_LFO, 3)
        .osc(0, PWM_DEPTH, 64)
        .set(FILTER_TYPE_LFO, 0x01 | 2 << 4)
        .set(FILTER_CUTOFF + 6, 40)
        .set(LFO_BASE[1] + LFO_TYPE, 4)
        .set(LFO_BASE[1] + LFO_SPEED, 127)
        .set(LFO_BASE[1] + LFO_DEPTH_FIELD, 127)
        .set(LFO_BASE[3] + LFO_TYPE, 3)
        .set(LFO_BASE[3] + LFO_OPTIONS, 0x02)
        .set(LFO_BASE[3] + LFO_SPEED, 64);
    let (s, losses) = mapped(d);
    assert_eq!(
        lfo_of(&s, 0),
        [
            (LAB_LFO_SAMPLE_HOLD | LAB_LFO_PITCH << LAB_LFO_DEST_SHIFT) as u8,
            31,
            5
        ],
        "LFO 2's random shape, full speed, a full vibrato as about a semitone"
    );
    assert_eq!(
        lfo_of(&s, 1),
        [
            (LAB_LFO_SQUARE | LAB_LFO_PULSE << LAB_LFO_DEST_SHIFT) as u8,
            15,
            (-31i8) as u8
        ],
        "LFO 4 inverted on the width, half speed, half depth"
    );
    assert!(losses.contains(&Loss::LfoDropped("FILTER")), "{losses:?}");
    assert_eq!(
        Loss::LfoDropped("FILTER").to_string(),
        "FILTER LFO: DROPPED (TWO LFOS)"
    );
}
