// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::*;
use sidou_patch::lab::{lab_sound, Loss};
use sidou_patch::mbsid::{parse_bank, patch_dump, Bank, Engine, Error, Reason};
use sidou_patch::{format_of, Format, Patch, Unmapped, VoiceMode};
use sidou_sys::*;

const TRIANGLE_RING: u8 = 0x41;
const PULSE_SYNC: u8 = 0x24;
const VOICE_OFF: u8 = 0x10;
const OSC3_OFF: u8 = 0x80;
const BAND_PASS: u8 = 0x20;
const MESSAGE_BYTES: usize = 1036;

fn bank_of(patches: &[MbPatch]) -> Bank {
    parse_bank(&patches.iter().flat_map(MbPatch::dump).collect::<Vec<_>>()).unwrap()
}

fn one(patch: MbPatch) -> Patch {
    let mut bank = bank_of(&[patch]);
    assert!(bank.refused.is_empty(), "{:?}", bank.refused);
    bank.patches.remove(0)
}

#[test]
fn a_dump_is_the_header_the_low_nibble_first_and_a_checksum() {
    let patch = MbPatch::new("L");
    let message = patch_dump(&patch.0, 2, 4);
    assert_eq!(message.len(), MESSAGE_BYTES);
    assert_eq!(
        message[..12],
        [0xF0, 0x00, 0x00, 0x7E, 0x4B, 0x00, 0x02, 0x00, 2, 4, 0x0C, 0x04]
    );
    let sum: u32 = message[10..MESSAGE_BYTES - 1]
        .iter()
        .map(|&b| u32::from(b))
        .sum();
    assert_eq!(sum & 0x7F, 0, "the checksum makes the sum zero");
    assert_eq!(message[MESSAGE_BYTES - 1], 0xF7);
    assert_eq!(format_of(&message), Format::MidiboxSid);
    assert_eq!(format_of(&[0xF0, 0x00, 0x20, 0x3C]), Format::SidStation);
}

#[test]
fn a_lead_patch_layers_its_left_oscillators() {
    let patch = one(MbPatch::new("Lead Patch")
        .set(MB_FLAGS, 0x01)
        .voice(0, MB_V_WAVEFORM, TRIANGLE_RING)
        .voice(1, MB_V_WAVEFORM, PULSE_SYNC)
        .voice(1, MB_V_AD, 0x3C)
        .voice(1, MB_V_SR, 0xA5)
        .voice(1, MB_V_PW_L, 0x0F)
        .voice(1, MB_V_PW_H, 0xF8)
        .voice(2, MB_V_WAVEFORM, MB_SAW | VOICE_OFF));
    assert_eq!(patch.name, "Lead Patch");
    assert_eq!(patch.mode, VoiceMode::Single);
    assert!(patch.legato);
    assert_eq!(patch.active, [true, true, false]);
    let [first, second, _] = patch.oscillators;
    assert_eq!(
        (first.control, first.ring(), first.sync()),
        (0x14, true, false)
    );
    assert_eq!(
        (second.control, second.ring(), second.sync()),
        (0x42, false, true)
    );
    assert_eq!(
        (second.attack, second.decay, second.sustain, second.release),
        (3, 12, 10, 5)
    );
    assert_eq!(
        second.pulse_width(),
        0x800,
        "12 bits, high nibble's top bits ignored, to the nearest 32"
    );
    assert!(patch.unmapped.is_empty(), "{:?}", patch.unmapped);
}

#[test]
fn transpose_finetune_and_detune_fold_into_semitones_and_64ths() {
    let rows: [(usize, u8, u8, u8, u8, i8, i8); 8] = [
        (0, 0x4C, 0x80, 2, 0, 12, 0),
        (0, 0x40, 0x90, 2, 0, 0, 16),
        (0, 0x40, 0xC0, 2, 0, 1, 0),
        (0, 0x40, 0xC0, 0, 0, 0, 0),
        (0, 0x34, 0x70, 2, 0, -12, -16),
        (0, 0x40, 0x80, 2, 32, 0, 4),
        (1, 0x40, 0x80, 2, 32, 0, 16),
        (2, 0x40, 0x80, 2, 32, 0, -16),
    ];
    for (n, transpose, finetune, range, detune, semis, steps) in rows {
        let patch = one(MbPatch::new("Tune")
            .set(MB_DETUNE, detune)
            .voice(n, MB_V_TRANSPOSE, transpose)
            .voice(n, MB_V_FINETUNE, finetune)
            .voice(n, MB_V_PITCHRANGE, range));
        let osc = patch.oscillators[n];
        assert_eq!(
            (osc.transpose, osc.detune, osc.bend_range),
            (semis, steps, range),
            "osc {n}: transpose {transpose:#x}, finetune {finetune:#x}, range {range}, detune {detune}"
        );
    }
}

#[test]
fn the_left_filter_maps_routing_mode_cutoff_and_resonance() {
    let patch = one(MbPatch::new("Filter")
        .filter(MB_F_CHANNELS, BAND_PASS | 0b011)
        .filter(MB_F_CUTOFF_L, 0x00)
        .filter(MB_F_CUTOFF_H, 0x84)
        .filter(MB_F_RESONANCE, 0xF7));
    let f = patch.filter;
    assert_eq!(f.routed, [true, true, false]);
    assert_eq!((f.mode, f.cutoff, f.resonance), (2, 32, 15));

    let silenced = one(MbPatch::new("3 Off").filter(MB_F_CHANNELS, OSC3_OFF | 0x10));
    assert_eq!(
        silenced.active,
        [true, true, false],
        "OSC3 off silences the third oscillator"
    );
    assert_eq!(silenced.filter.mode, 1);
}

#[test]
fn only_lead_engine_patches_import_and_the_rest_are_refused_with_the_reason() {
    let bank = bank_of(&[
        MbPatch::new("Lead"),
        MbPatch::new("Bass").set(MB_ENGINE, 1),
        MbPatch::new("Drums").set(MB_ENGINE, 2),
        MbPatch::new("Multi").set(MB_ENGINE, 3),
        MbPatch::new("Reserved").set(MB_ENGINE, 0xFC),
    ]);
    let kept: Vec<&str> = bank.patches.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(kept, ["Lead", "Reserved"]);
    let reasons: Vec<Reason> = bank.refused.iter().map(|r| r.reason).collect();
    assert_eq!(
        reasons,
        [
            Reason::Engine(Engine::Bassline),
            Reason::Engine(Engine::Drum),
            Reason::Engine(Engine::Multi)
        ]
    );

    let drums = patch_dump(&MbPatch::new("Drums").set(MB_ENGINE, 2).0, 2, 4);
    let refused = parse_bank(&drums).unwrap().refused.remove(0);
    assert_eq!(
        refused.to_string(),
        "C005 Drums: a Drum engine patch, only Lead patches import"
    );
}

#[test]
fn a_patch_without_a_name_is_named_after_its_slot() {
    let blank = patch_dump(&MbPatch::new("").set(0, 0x00).0, 1, 44);
    assert_eq!(parse_bank(&blank).unwrap().patches[0].name, "B045");
}

#[test]
fn a_patch_with_a_wrong_checksum_is_refused() {
    let mut message = MbPatch::new("Broken").dump();
    message[200] ^= 0x01;
    let bank = parse_bank(&message).unwrap();
    assert!(bank.patches.is_empty());
    assert_eq!(bank.refused[0].reason, Reason::Checksum);
    assert_eq!(
        bank.refused[0].to_string(),
        "A001 Broken: the checksum does not match, the patch is damaged"
    );
}

#[test]
fn other_messages_are_skipped_and_bad_input_is_refused_with_the_reason() {
    let request = [
        0xF0, 0x00, 0x00, 0x7E, 0x4B, 0x00, 0x01, 0x00, 0x00, 0x00, 0xF7,
    ];
    let mut ensemble = MbPatch::new("Ens").dump();
    ensemble[7] = 0x70;
    let mut bank = request.to_vec();
    bank.extend(ensemble);
    bank.extend(MbPatch::new("Kept").dump());
    assert_eq!(parse_bank(&bank).unwrap().patches[0].name, "Kept");

    let good = MbPatch::new("Good").dump();
    assert_eq!(parse_bank(&[]).err(), Some(Error::NoPatches));
    assert_eq!(parse_bank(&request).err(), Some(Error::NoPatches));
    assert_eq!(parse_bank(b"junk").err(), Some(Error::NotSysex { at: 0 }));
    assert_eq!(
        parse_bank(&good[..good.len() - 1]).err(),
        Some(Error::Unterminated { at: 0 })
    );
    let mut foreign = good.clone();
    foreign[3] = 0x7D;
    assert_eq!(
        parse_bank(&foreign).err(),
        Some(Error::ForeignMessage { at: 0 })
    );
    let mut short = good[..good.len() - 40].to_vec();
    short.push(0xF7);
    assert_eq!(parse_bank(&short).err(), Some(Error::Length { index: 0 }));
}

#[test]
fn what_the_model_cannot_hold_is_reported_as_a_loss() {
    let cases: [(&str, MbPatch, Option<Unmapped>, &str); 10] = [
        (
            "a modulation path to the filter",
            MbPatch::new("Mod")
                .mod_path(1, MB_MOD_OPERATOR, 1)
                .mod_path(1, MB_MOD_DEPTH, 0xC0)
                .mod_path(1, MB_MOD_TARGET_LEFT, 0x40),
            Some(Unmapped::ModulationPath(2)),
            "MOD PATH 2: DROPPED",
        ),
        (
            "a modulation path with zero depth",
            MbPatch::new("Mod")
                .mod_path(1, MB_MOD_OPERATOR, 1)
                .mod_path(1, MB_MOD_TARGET_LEFT, 0x40),
            None,
            "",
        ),
        (
            "a wavetable sequencer",
            MbPatch::new("Wt")
                .wavetable(2, MB_WT_ASSIGN, 5)
                .wavetable(2, MB_WT_BEGIN, 0x10)
                .wavetable(2, MB_WT_END, 0x9F)
                .set(MB_WT_MEMORY + 0x1F, 0x41),
            Some(Unmapped::Wavetable(3)),
            "WAVETABLE 3: DROPPED",
        ),
        (
            "a wavetable whose steps change nothing",
            MbPatch::new("Wt")
                .wavetable(2, MB_WT_ASSIGN, 5)
                .wavetable(2, MB_WT_END, 0x1F),
            None,
            "",
        ),
        (
            "an arpeggio",
            MbPatch::new("Arp").voice(1, MB_V_ARP_MODE, 0x01),
            Some(Unmapped::Arpeggio(2)),
            "OSC 2 ARP: DROPPED",
        ),
        (
            "an arpeggio on a silent oscillator",
            MbPatch::new("Arp")
                .voice(1, MB_V_ARP_MODE, 0x01)
                .voice(1, MB_V_WAVEFORM, VOICE_OFF),
            None,
            "",
        ),
        (
            "a portamento",
            MbPatch::new("Glide").voice(0, MB_V_PORTAMENTO, 40),
            Some(Unmapped::Portamento(1)),
            "OSC 1 PORTAMENTO: DROPPED",
        ),
        (
            "filter key tracking",
            MbPatch::new("Track")
                .filter(MB_F_CHANNELS, 0x11)
                .filter(MB_F_KEYTRACK, 0x80),
            Some(Unmapped::FilterKeyTracking),
            "FILTER KEYTRACK: DROPPED",
        ),
        (
            "key tracking on an unused filter",
            MbPatch::new("Track").filter(MB_F_KEYTRACK, 0x80),
            None,
            "",
        ),
        (
            "a right SID unlike the left",
            MbPatch::new("Stereo").set(MB_VOICES_RIGHT + MB_V_AD, 0x55),
            Some(Unmapped::RightSid),
            "RIGHT SID: LEFT KEPT",
        ),
    ];
    for (what, source, unmapped, line) in cases {
        let patch = one(source);
        let (_, losses) = lab_sound(&patch);
        match unmapped {
            Some(u) => {
                assert_eq!(patch.unmapped, [u], "{what}");
                assert!(losses.contains(&Loss::Unmapped(u)), "{what}: {losses:?}");
                assert_eq!(Loss::Unmapped(u).to_string(), line, "{what}");
            }
            None => assert!(patch.unmapped.is_empty(), "{what}: {:?}", patch.unmapped),
        }
    }
}

#[test]
fn a_three_oscillator_lead_patch_is_a_valid_layered_lab_sound() {
    let patch = one(MbPatch::new("Layers")
        .voice(1, MB_V_WAVEFORM, MB_PULSE)
        .voice(1, MB_V_PW_H, 0x08)
        .filter(MB_F_CHANNELS, 0x10 | 0b101));
    let (sound, losses) = lab_sound(&patch);
    assert!(unsafe { lab_sound_valid(sound.as_ptr()) } != 0);
    assert_eq!(sound[LAB_SOUND_LAYERS as usize], 2);
    assert_eq!(sound[LAB_SOUND_FILTER_VOICES as usize], 0b101);
    assert!(losses.is_empty(), "{losses:?}");
}
