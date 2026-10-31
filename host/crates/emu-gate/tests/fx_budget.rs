// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

const CORE_HZ: f64 = 240e6;
const SAMPLE_RATE: f64 = 44_100.0;
const FX_BUDGET: f64 = 500.0;
const TWO_CHIPS_PLUS_FX_BUDGET: f64 = common::TWO_CHIPS_PLUS_EFFECTS;
const TWO_CHIPS_AT_WORST: f64 = common::SID_WORST_BLOCK_TWO_CHIPS + common::TAP_PER_SAMPLE;

fn check(chips: u32, worst: bool) {
    let Some(build) = common::build_dir() else {
        common::skip_message("the effects gate");
        return;
    };
    let name = if worst { "fx_worst" } else { "fx" };
    let r = common::run(&build, &format!("sid_bench_{name}_{chips}"), 4);
    let host = emu_gate::render_host_fx(chips, worst);
    assert_eq!(r.samples.len(), host.len());
    let dry =
        emu_gate::render_host_workload(chips, sidou_sys::SID_MODEL_6581, sidou_sys::WORKLOAD_TRIAD);
    let changed = dry.iter().zip(&host).filter(|(a, b)| a != b).count();
    assert!(
        changed > host.len() / 2,
        "the effects change only {changed} samples; the scenario would be vacuous"
    );
    let diffs: Vec<i32> = r
        .samples
        .iter()
        .zip(&host)
        .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
        .collect();
    let worst = *diffs.iter().max().unwrap();

    let n = r.samples.len() as f64;
    let sid = r.phases[1] as f64 / n;
    let fx = r.phases[2] as f64 / n;
    let total = sid + fx;
    let report = serde_json::json!({
        "chips": chips,
        "scenario": name,
        "metric": "instructions (lower bound on cycles; assumes 1 IPC)",
        "sid_instructions_per_sample": sid,
        "fx_instructions_per_sample": fx,
        "total_instructions_per_sample": total,
        "fx_percent_of_240mhz_core": fx * SAMPLE_RATE / CORE_HZ * 100.0,
        "total_percent_of_240mhz_core": total * SAMPLE_RATE / CORE_HZ * 100.0,
        "two_chips_at_worst_plus_fx_instructions_per_sample": TWO_CHIPS_AT_WORST + fx,
        "setup_instructions": r.phases[0],
        "max_sample_diff_vs_host": worst,
    });
    println!("{report}");
    let out = build.join(format!("{name}_budget_{chips}.json"));
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));

    if let Some(i) = diffs.iter().position(|&d| d != 0) {
        panic!(
            "target output differs from host build by up to {worst} LSB; first at sample {i}: \
             target {} host {}",
            r.samples[i], host[i]
        );
    }
    assert!(
        fx <= FX_BUDGET,
        "the effects cost {fx:.0} instructions/sample, over the budget of {FX_BUDGET}"
    );
    assert!(
        TWO_CHIPS_AT_WORST + fx <= TWO_CHIPS_PLUS_FX_BUDGET,
        "two chips at the SID's worst block and the tap plus effects cost {:.0} instructions/sample, over \
         {TWO_CHIPS_PLUS_FX_BUDGET}",
        TWO_CHIPS_AT_WORST + fx
    );
}

#[test]
fn fx_one_chip_fits_budget_and_matches_host() {
    check(1, false);
}

#[test]
fn fx_two_chips_fit_budget_and_match_host() {
    check(2, false);
}

#[test]
fn fx_worst_case_one_chip_fits_budget_and_matches_host() {
    check(1, true);
}

#[test]
fn fx_worst_case_two_chips_fit_budget_and_match_host() {
    check(2, true);
}
