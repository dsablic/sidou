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
            "a table",
            three().tables(&[&[0x40, 0x1E, 0xFF]]),
            Loss::Table(1),
        ),
        (
            "a filter envelope",
            three().set(FILTER_ENV_DEPTH, 40),
            Loss::FilterEnvelope,
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
    assert_eq!(Loss::FilterEnvelope.to_string(), "FILTER ENVELOPE DROPPED");
}
