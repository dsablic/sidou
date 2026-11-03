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
