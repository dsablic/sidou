// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's named chip profiles (M1c Task 3): each is a set of reSIDfp settings, a filter
//! curve setting (a row of gen/filter.h), a combined-waveform strength and the voice DACs'
//! ladder, plus the 6581 filter's non-linearity amount (sid_set_filter_nonlinearity), and
//! the curve setting the age/heat control drifts towards. `profiles_header` writes them to
//! core/sid/gen/profiles.h.
//!
//! What reSIDfp exposes, and what each profile takes from it (libresidfp c1791383):
//! - `setFilter6581Curve` / `setFilter8580Curve` (0..1, 0.5 by default; Filter6581.h and
//!   Filter8580.h say 0 is dark and 1 bright, which the 6581 does, while the 8580's cutoff
//!   rises as its curve falls, measured: FC 1024 is 7.1 kHz at 0.5 and 10.4 kHz at 0.25, as
//!   its integrators' gate voltage, 1.8 - 0.6 curve, rises) and `setFilter6581Range` (0..1: the VCRs' uCox, 1 + 39 r uA/V^2; the
//!   default, never set, is 20 uA/V^2, r = 19/39). The curve shifts the 6581's cutoff DAC
//!   zero (FilterModelConfig6581::getDAC), the range scales its VCR current.
//! - `setCombinedWaveforms`: three sampled chips (WaveformCalculator.cpp): AVERAGE from a
//!   6581 R3 0486S (sampled by Trurl) and an 8580 R5 1088 (reFX-Mike), WEAK from a 6581 R2
//!   4383 (ltx128) and an 8580 R5 4887, STRONG from a 6581 R2 0384 (Trurl) and an 8580 R5
//!   1489.
//! - `setDacLeakage` (0..1, a scale on the MOSFET leakage, 1 by default; it cannot raise
//!   it) and the R-2R ladder's 2R/R ratio, which reSIDfp fixes per model (2.20 on the 6581,
//!   2.00 on the 8580; Dac.cpp).
//! - The op-amp transfer function of reSIDfp's 6581 filter is measured on a 6581R4AR 0687
//!   (FilterModelConfig6581.cpp), its 8580's on an 8580R5 1690.
//!
//! What is not documented anywhere we found: a filter curve or range per 6581 revision.
//! The revisions R2, R3 and R4AR share their filter design (the R3 changed the input pins'
//! protection, the R4AR the silicon grade; Wikipedia "MOS Technology 6581", "Revisions",
//! uncited there), while 6581 filters vary widely from chip to chip (reSIDfp's Filter6581.h:
//! its resistances are "only approximate values for one particular SID chip"). So the R2 and
//! R3 rows below are representative points in that spread, chosen by SIDOU: R2 brighter,
//! R3 darker than reSIDfp's default (the R4AR's), each with the combined waveforms sampled
//! from that revision. They are labelled as SIDOU's choices in the header and the docs.
//! "Worn 6581" is an artistic profile, not a measured chip: darker and narrower (curve and
//! range), the strongest combined waveforms, a leakier and more kinked DAC than reSIDfp
//! allows, and more non-linearity.

use std::fmt::Write;

use residfp_sys::{CombinedWaveforms, Model};

use crate::filter_fit::CurveSetting;

/// The 6581's filter range for reSIDfp's default uCox (20e-6 = (1 + 39 r) 1e-6).
pub const DEFAULT_RANGE_6581: f64 = 19.0 / 39.0;

/// The filter curve settings fitted for gen/filter.h, in table order per model (index 0 is
/// the one sid_init uses, reSIDfp's default). The profiles below index into them.
pub const FILTER_CURVES: [CurveSetting; 7] = [
    // 6581: reSIDfp's default (the R4AR), R2, R3, worn, and age/heat's target.
    CurveSetting::default_for(Model::Mos6581),
    CurveSetting {
        model: Model::Mos6581,
        curve: 0.58,
        range6581: None,
    },
    CurveSetting {
        model: Model::Mos6581,
        curve: 0.42,
        range6581: None,
    },
    CurveSetting {
        model: Model::Mos6581,
        curve: 0.35,
        range6581: Some(0.4),
    },
    CurveSetting {
        model: Model::Mos6581,
        curve: 0.25,
        range6581: Some(0.3),
    },
    // 8580: reSIDfp's default (the R5), and age/heat's target.
    CurveSetting::default_for(Model::Csg8580),
    CurveSetting {
        model: Model::Csg8580,
        curve: 0.75,
        range6581: None,
    },
];

/// The row of `setting` among its model's rows of gen/filter.h.
pub fn curve_row(setting: &CurveSetting) -> usize {
    FILTER_CURVES
        .iter()
        .filter(|s| s.model == setting.model)
        .position(|s| s == setting)
        .expect("a fitted curve setting")
}

/// One named profile.
#[derive(Clone, Copy, Debug)]
pub struct ChipProfile {
    /// As the CHIP view and the MIDI reference show it.
    pub name: &'static str,
    /// The C enumerator's suffix: SID_CHIP_<ident>.
    pub ident: &'static str,
    pub model: Model,
    /// Its filter's reSIDfp setting.
    pub filter: CurveSetting,
    /// The setting age/heat drifts its filter towards (at age 127).
    pub hot: CurveSetting,
    pub combined: CombinedWaveforms,
    /// The voice DACs' 2R/R ratio and leakage (sid_dac_build); None for reSIDfp's.
    pub dac: Option<(f64, f64)>,
    /// The 6581 filter's non-linearity amount (1: reSIDfp's).
    pub nonlinearity: f64,
    /// Where it comes from, for the generated header.
    pub source: &'static str,
}

/// The profiles, in the order of the MIDI map's Profile row (0..4).
pub const PROFILES: [ChipProfile; 5] = [
    ChipProfile {
        name: "6581 R2",
        ident: "6581_R2",
        model: Model::Mos6581,
        filter: FILTER_CURVES[1],
        hot: FILTER_CURVES[4],
        combined: CombinedWaveforms::Weak,
        dac: None,
        nonlinearity: 1.0,
        source: "combined waveforms sampled from a 6581 R2 4383 (reSIDfp WEAK, ltx128); \
                 filter curve 0.58, SIDOU's choice (brighter than the R4AR's; no documented \
                 per-revision value)",
    },
    ChipProfile {
        name: "6581 R3",
        ident: "6581_R3",
        model: Model::Mos6581,
        filter: FILTER_CURVES[2],
        hot: FILTER_CURVES[4],
        combined: CombinedWaveforms::Average,
        dac: None,
        nonlinearity: 1.0,
        source: "combined waveforms sampled from a 6581 R3 0486S (reSIDfp AVERAGE, Trurl); \
                 filter curve 0.42, SIDOU's choice (darker than the R4AR's; no documented \
                 per-revision value)",
    },
    ChipProfile {
        name: "6581 R4AR",
        ident: "6581_R4AR",
        model: Model::Mos6581,
        filter: FILTER_CURVES[0],
        hot: FILTER_CURVES[4],
        combined: CombinedWaveforms::Average,
        dac: None,
        nonlinearity: 1.0,
        source: "reSIDfp's defaults: its 6581 op-amp measured on a 6581R4AR 0687, filter \
                 curve 0.5 and range default, AVERAGE combined waveforms",
    },
    ChipProfile {
        name: "8580 R5",
        ident: "8580_R5",
        model: Model::Csg8580,
        filter: FILTER_CURVES[5],
        hot: FILTER_CURVES[6],
        combined: CombinedWaveforms::Average,
        dac: None,
        nonlinearity: 1.0,
        source: "reSIDfp's defaults: its 8580 op-amp measured on an 8580R5 1690, filter \
                 curve 0.5, AVERAGE combined waveforms (an 8580 R5 1088)",
    },
    ChipProfile {
        name: "worn 6581",
        ident: "6581_WORN",
        model: Model::Mos6581,
        filter: FILTER_CURVES[3],
        hot: FILTER_CURVES[4],
        combined: CombinedWaveforms::Strong,
        dac: Some((2.30, 0.0225)),
        nonlinearity: 1.5,
        source: "ARTISTIC, not a measured chip: filter curve 0.35 and range 0.4 (darker and \
                 narrower), STRONG combined waveforms (a 6581 R2 0384), the DAC's 2R/R 2.30 \
                 and three times reSIDfp's leakage (beyond setDacLeakage's reach), \
                 non-linearity 1.5",
    },
];

fn model_c(model: Model) -> &'static str {
    match model {
        Model::Mos6581 => "SID_MODEL_6581",
        Model::Csg8580 => "SID_MODEL_8580",
    }
}

fn combined_c(cws: CombinedWaveforms) -> &'static str {
    match cws {
        CombinedWaveforms::Weak => "SID_PROFILE_WEAK",
        CombinedWaveforms::Average => "SID_PROFILE_AVERAGE",
        CombinedWaveforms::Strong => "SID_PROFILE_STRONG",
    }
}

/// gen/profiles.h: the profiles' enumerators and their rows (sid_chip_profile, in
/// sid_character.h).
pub fn profiles_header() -> crate::Generated {
    let mut s = String::new();
    // REUSE-IgnoreStart
    s.push_str(
        "// SPDX-FileCopyrightText: 2026 SIDOU contributors\n\
         // SPDX-License-Identifier: GPL-3.0-or-later\n",
    );
    // REUSE-IgnoreEnd
    s.push_str(
        "/* Generated by sidou-tablegen (profiles.rs); do not edit. To regenerate, from host/:\n\
         \x20*   cargo run --release -p sidou-tablegen\n\
         \x20* SIDOU's chip profiles (M1c Task 3), as reSIDfp settings (libresidfp c1791383):\n\
         \x20* the filter curve setting (a row of gen/filter.h, and the row age/heat drifts\n\
         \x20* towards), the combined-waveform strength (gen/grid.h), the voice DACs' 2R/R ratio\n\
         \x20* and leakage (sid_dac.h) and the 6581 filter's non-linearity amount. reSIDfp\n\
         \x20* documents no filter curve or range per 6581 revision (the revisions share their\n\
         \x20* filter design; 6581 filters vary widely from chip to chip), so the R2 and R3\n\
         \x20* curves are SIDOU's representative choices; the combined waveforms are sampled\n\
         \x20* from those revisions. \"worn 6581\" is artistic. Sources, per profile:\n",
    );
    for (i, p) in PROFILES.iter().enumerate() {
        writeln!(s, " *   {i} {}: {}", p.name, p.source).unwrap();
    }
    s.push_str(
        " * docs/decisions/0006-chip-profiles.md has the reasoning.\n */\n\
         #ifndef SIDOU_GEN_PROFILES_H\n#define SIDOU_GEN_PROFILES_H\n\n",
    );
    let idents: Vec<String> = PROFILES
        .iter()
        .map(|p| format!("SID_CHIP_{}", p.ident))
        .collect();
    writeln!(s, "enum {{ {}, SID_CHIPS }};\n", idents.join(", ")).unwrap();
    s.push_str(
        "/* {name, model, filter curve row, age/heat's row, combined waveforms, DAC 2R/R,\n\
         \x20* DAC leakage, non-linearity} */\n\
         #define SID_CHIP_PROFILE_ROWS \\\n    { \\\n",
    );
    for p in &PROFILES {
        assert_eq!(p.filter.model, p.model);
        assert_eq!(p.hot.model, p.model);
        let (kink, leak) = match (p.dac, p.model) {
            (Some((k, l)), _) => (format!("{k:.2}f"), format!("{l:.4}f")),
            (None, Model::Mos6581) => ("SID_DAC_KINK_6581".into(), "SID_DAC_LEAKAGE_6581".into()),
            (None, Model::Csg8580) => ("SID_DAC_KINK_8580".into(), "SID_DAC_LEAKAGE_8580".into()),
        };
        writeln!(
            s,
            "        {{\"{}\", {}, {}u, {}u, {}, {kink}, {leak}, {:.2}f}}, \\",
            p.name,
            model_c(p.model),
            curve_row(&p.filter),
            curve_row(&p.hot),
            combined_c(p.combined),
            p.nonlinearity
        )
        .unwrap();
    }
    s.push_str("    }\n\n#endif\n");
    crate::Generated {
        path: "core/sid/gen/profiles.h",
        contents: s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_points_at_rows_of_its_model() {
        for p in &PROFILES {
            assert_eq!(p.filter.model, p.model, "{}", p.name);
            assert_eq!(p.hot.model, p.model, "{}", p.name);
            curve_row(&p.filter);
            curve_row(&p.hot);
        }
        // Row 0 of each model is reSIDfp's default, which sid_init uses.
        assert_eq!(curve_row(&CurveSetting::default_for(Model::Mos6581)), 0);
        assert_eq!(curve_row(&CurveSetting::default_for(Model::Csg8580)), 0);
    }

    #[test]
    fn the_defaults_are_residfps() {
        for p in PROFILES
            .iter()
            .filter(|p| p.name == "6581 R4AR" || p.name == "8580 R5")
        {
            assert_eq!(p.filter, CurveSetting::default_for(p.model));
            assert_eq!(p.combined, CombinedWaveforms::Average);
            assert_eq!(p.dac, None);
            assert_eq!(p.nonlinearity, 1.0);
        }
    }
}
