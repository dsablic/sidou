// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::mbsid::{parse_bank, Bank, Reason};

const BANK_ENV: &str = "MBSID_PRESETS";
const BANK_PATCHES: usize = 128;

fn preset_bank() -> Bank {
    let path = std::env::var(BANK_ENV).unwrap_or_else(|_| panic!("set {BANK_ENV} to the bank"));
    parse_bank(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
#[ignore = "needs a MIDIbox SID V2 preset bank (.syx) in MBSID_PRESETS"]
fn a_preset_bank_decodes_every_patch_to_valid_ranges() {
    let bank = preset_bank();
    assert_eq!(bank.patches.len() + bank.refused.len(), BANK_PATCHES);
    for refused in &bank.refused {
        assert!(
            matches!(refused.reason, Reason::Engine(_)),
            "{refused}: every preset's checksum matches"
        );
        println!("{refused}");
    }
    for patch in &bank.patches {
        assert!(!patch.name.is_empty(), "{patch:?}");
        assert!(patch.filter.mode <= 7 && patch.filter.cutoff <= 127);
        for osc in &patch.oscillators {
            assert!(osc.detune.abs() <= 32, "{}", patch.name);
            assert!(osc.pulse_width() <= 4096, "{}", patch.name);
        }
    }
}

#[test]
#[ignore = "needs a MIDIbox SID V2 preset bank (.syx) in MBSID_PRESETS"]
fn every_lead_preset_maps_to_a_valid_lab_sound_and_packs_into_songs() {
    let bank = preset_bank();
    for patch in &bank.patches {
        let (sound, losses) = sidou_patch::lab::lab_sound(patch);
        assert!(
            unsafe { sidou_sys::lab_sound_valid(sound.as_ptr()) } != 0,
            "{}",
            patch.name
        );
        let lines: Vec<String> = losses.iter().map(ToString::to_string).collect();
        println!("{:<16} {}", patch.name, lines.join(", "));
    }
    let songs = sidou_patch::bank::bank_songs(&bank.patches, sidou_sys::SID_MODEL_6581, "MBSID");
    assert_eq!(
        songs.iter().map(|s| s.patches).sum::<usize>(),
        bank.patches.len()
    );
    for s in &songs {
        let mut song = sidou_sys::song::Song::device();
        song.read(&s.sng, 0).unwrap();
    }
}
