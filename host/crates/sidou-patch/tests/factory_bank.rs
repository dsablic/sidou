// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_patch::{parse_bank, Patch};

const BANK_ENV: &str = "SIDSTATION_PRESETS";
const FACTORY_PATCHES: usize = 90;

fn factory_bank() -> Vec<Patch> {
    let path = std::env::var(BANK_ENV).unwrap_or_else(|_| panic!("set {BANK_ENV} to the bank"));
    parse_bank(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
#[ignore = "needs Elektron's SidStation_Presets_r1.syx in SIDSTATION_PRESETS"]
fn the_factory_bank_decodes_to_valid_ranges() {
    let patches = factory_bank();
    assert_eq!(patches.len(), FACTORY_PATCHES);
    for patch in &patches {
        assert!(!patch.name.is_empty(), "{patch:?}");
        assert!((1..=7).contains(&patch.filter.mode), "{}", patch.name);
        assert!(patch.filter.resonance <= 15);
        for lfo in &patch.lfos {
            assert!(lfo.shape <= 5, "{}", patch.name);
        }
        for osc in &patch.oscillators {
            assert!(osc.attack <= 15 && osc.decay <= 15 && osc.sustain <= 15 && osc.release <= 15);
        }
    }
}

#[test]
#[ignore = "needs Elektron's SidStation_Presets_r1.syx in SIDSTATION_PRESETS"]
fn every_factory_patch_maps_to_a_valid_lab_sound() {
    for patch in factory_bank() {
        let (sound, losses) = sidou_patch::lab::lab_sound(&patch);
        assert!(
            unsafe { sidou_sys::lab_sound_valid(sound.as_ptr()) } != 0,
            "{}",
            patch.name
        );
        let lines: Vec<String> = losses.iter().map(ToString::to_string).collect();
        println!("{:<10} {}", patch.name, lines.join(", "));
    }
}

#[test]
#[ignore = "needs Elektron's SidStation_Presets_r1.syx in SIDSTATION_PRESETS"]
fn the_factory_bank_packs_into_songs_the_device_reads() {
    let patches = factory_bank();
    let songs = sidou_patch::bank::bank_songs(&patches, sidou_sys::SID_MODEL_6581, "SIDSTN");
    assert_eq!(
        songs.iter().map(|s| s.patches).sum::<usize>(),
        FACTORY_PATCHES
    );
    for s in &songs {
        let mut song = sidou_sys::song::Song::device();
        song.read(&s.sng, 0).unwrap();
        println!("{}: {} patches, {} bytes", s.name, s.patches, s.sng.len());
    }
}
