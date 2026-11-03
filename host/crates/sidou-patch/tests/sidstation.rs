// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::*;
use sidou_patch::{parse_bank, Error, TableNote, VoiceMode};

fn one(decoded: &Decoded) -> sidou_patch::Patch {
    let mut patches = parse_bank(&dump(&PREFIX_LIBRARIAN, 0, decoded)).unwrap();
    assert_eq!(patches.len(), 1);
    patches.remove(0)
}

#[test]
fn the_name_and_patch_mode_decode() {
    let single = one(&Decoded::new("Anpanman").set(PATCH_MODE, 0b0010_0111));
    assert_eq!(single.name, "Anpanman");
    assert_eq!(single.mode, VoiceMode::Single);
    assert_eq!(single.active, [true, true, true]);
    assert!(single.legato);

    let poly = one(&Decoded::new("Keys").set(PATCH_MODE, 0b0000_1001));
    assert_eq!(poly.mode, VoiceMode::Poly);
    assert_eq!(poly.active, [true, false, false]);
}

#[test]
fn an_oscillators_stored_nibbles_are_swapped_back() {
    let patch = one(&Decoded::new("Osc")
        .osc(1, OSC_WAVE, 0x44)
        .osc(1, OSC_ATTACK, 3)
        .osc(1, OSC_DECAY, 9)
        .osc(1, OSC_SUSTAIN, 12)
        .osc(1, OSC_RELEASE, 10)
        .osc(1, OSC_PULSE_WIDTH, 64)
        .osc(1, OSC_TRANSPOSE, 0xF4)
        .osc(1, OSC_DETUNE, 0x10)
        .osc(1, OSC_FLAGS, 0b01)
        .osc(1, OSC_TABLE_SPEED, 5));
    let osc = patch.oscillators[1];
    assert_eq!((osc.waveform(), osc.ring(), osc.sync()), (4, true, false));
    assert_eq!(
        (osc.attack, osc.decay, osc.sustain, osc.release),
        (3, 9, 12, 10)
    );
    assert_eq!(osc.pulse_width(), 0x800);
    assert_eq!(osc.transpose, -12);
    assert_eq!(osc.detune, 16);
    assert!((osc.detune_cents() - 25.0).abs() < 1e-9);
    assert!(osc.restart_pulse_width && !osc.gate_only);
    assert_eq!(osc.table_speed, 5);
    assert_eq!(patch.oscillators[0], Default::default());
}

#[test]
fn the_filter_and_an_lfo_decode() {
    let patch = one(&Decoded::new("Filt")
        .set(FILTER_ROUTING, 0xA5)
        .set(FILTER_TYPE_LFO, 0x21)
        .set(FILTER_CUTOFF, 77)
        .lfo(2, LFO_TYPE, 0x73)
        .lfo(2, LFO_OPTIONS, 0b0010_1101)
        .lfo(2, LFO_SPEED, 40));
    let filter = patch.filter;
    assert_eq!(filter.routed, [true, false, true]);
    assert_eq!(filter.resonance, 10);
    assert_eq!((filter.mode, filter.lfo, filter.cutoff), (1, 2, 77));
    let lfo = patch.lfos[2];
    assert_eq!((lfo.shape, lfo.control_source), (3, 7));
    assert_eq!(lfo.control_destination, 2);
    assert!(lfo.sync_to_note_on && lfo.positive_only && !lfo.invert && lfo.sync_to_note_off);
    assert_eq!(lfo.speed, 40);
}

#[test]
fn tables_decode_steps_notes_loops_and_ends() {
    let patch = one(&Decoded::new("Tables").tables(&[
        &[0x40, 0x1E, 0x80, 0x50, 0x40, 0x8C, 0x40, 0xC3, 0xFE, 0x01],
        &[0x20, 0x00, 0xFF],
        &[0xFF],
    ]));
    let first = &patch.tables[0];
    assert_eq!(first.steps.len(), 4);
    assert_eq!(first.steps[0].control, 0x40);
    assert_eq!(first.steps[0].note, TableNote::Absolute(0x1E));
    assert_eq!(first.steps[2].note, TableNote::Down(12));
    assert_eq!(first.steps[3].note, TableNote::Up(3));
    assert_eq!(first.loop_to, Some(1));
    assert_eq!(patch.tables[1].steps.len(), 1);
    assert_eq!(patch.tables[1].loop_to, None);
    assert!(patch.tables[2].steps.is_empty());
}

#[test]
fn tables_after_the_last_one_with_content_may_be_absent() {
    let patch = one(&Decoded::new("Body").tables(&[&[0x40, 0xC0, 0x90, 0xFF, 0xFE, 0x00]]));
    assert_eq!(patch.tables[0].steps.len(), 2);
    assert_eq!(patch.tables[0].steps[1].note, TableNote::Up(63));
    assert_eq!(patch.tables[0].loop_to, Some(0));
    assert!(patch.tables[1].steps.is_empty() && patch.tables[2].steps.is_empty());
}

#[test]
fn a_table_the_data_ends_in_has_no_terminator() {
    let patch = one(&Decoded::new("Fatburger").tables(&[&[0x90, 0xFF]]));
    assert_eq!(patch.tables[0].steps.len(), 1);
    assert_eq!(patch.tables[0].loop_to, None);
}

#[test]
fn both_prefixes_parse_and_a_wrong_size_field_is_ignored() {
    let decoded = Decoded::new("Both");
    for prefix in [&PREFIX_LIBRARIAN[..], &PREFIX_HARDWARE[..]] {
        for declared in [0, 307, 16383] {
            let patches = parse_bank(&dump(prefix, declared, &decoded)).unwrap();
            assert_eq!(patches[0].name, "Both");
        }
    }
}

#[test]
fn a_bank_yields_its_patches_and_skips_the_other_messages() {
    let mut bank = all_clear(&PREFIX_LIBRARIAN);
    bank.extend(dump(&PREFIX_LIBRARIAN, 0, &Decoded::new("One")));
    bank.extend(control_message(&PREFIX_LIBRARIAN, 0x03));
    bank.extend(dump(&PREFIX_LIBRARIAN, 0, &Decoded::new("Two")));
    bank.extend(control_message(&PREFIX_LIBRARIAN, 0x04));
    let names: Vec<_> = parse_bank(&bank)
        .unwrap()
        .into_iter()
        .map(|patch| patch.name)
        .collect();
    assert_eq!(names, ["One", "Two"]);
}

#[test]
fn bad_input_is_refused_with_the_reason() {
    let good = dump(&PREFIX_LIBRARIAN, 0, &Decoded::new("Good"));
    assert_eq!(parse_bank(&[]), Err(Error::NoPatches));
    assert_eq!(parse_bank(b"not sysex"), Err(Error::NotSysex { at: 0 }));

    let unterminated = &good[..good.len() - 1];
    assert_eq!(parse_bank(unterminated), Err(Error::Unterminated { at: 0 }));

    let mut foreign = good.clone();
    foreign[1] = 0x7D;
    assert_eq!(parse_bank(&foreign), Err(Error::ForeignMessage { at: 0 }));

    let mut short = good.clone();
    short.truncate(short.len() - 40);
    short.push(0xF7);
    assert_eq!(parse_bank(&short), Err(Error::Truncated { index: 0 }));

    let cut_mid_step = Decoded::new("Run").tables(&[&[0x40]]);
    assert_eq!(
        parse_bank(&dump(&PREFIX_LIBRARIAN, 0, &cut_mid_step)),
        Err(Error::TableOverrun { index: 0 })
    );
}
