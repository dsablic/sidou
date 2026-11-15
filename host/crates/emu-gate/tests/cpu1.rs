// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::path::Path;

use common::dual::{run_two_cores, write_input, Block, Core1Fate, DualRun};
use sidou_sys::{sid_model, SID_MODEL_6581, SID_MODEL_8580};

const BENCH: &str = "sid_bench_cpu1";
const WORST_BLOCK_SCENARIO: &str = "character_noise_tri_ring_0800";

const WORD_SCENARIO: usize = 2;
const WORD_MODEL: usize = 3;
const WORD_CORES: usize = 4;
const WORD_REBOOTS: usize = 5;
const WORD_STARTED: usize = 6;
const WORD_GAVE_UP: usize = 7;
const WORD_JOBS: usize = 8;
const WORD_BOOT: usize = 10;
const WORD_ON: usize = 11;

const STARTED: u32 = 0;
const HELD: u32 = u32::MAX;
const GAVE_UP_MISREAD: u32 = 1;
const GAVE_UP_LATE: u32 = 2;
const GAVE_UP_BARRED: u32 = 3;

const FLAKY_RATE: f64 = 1e-5;
const FLAKY_SEED: u64 = 7;
const STALLS_IDLE_AFTER: u64 = 2_000;
const JOBS_BEFORE_THE_MISREAD: u32 = 40;
const BLOCK: f64 = 128.0;

fn scenario() -> u32 {
    emu_gate::scenarios()
        .into_iter()
        .find(|(_, name)| name == WORST_BLOCK_SCENARIO)
        .expect("the worst block's scenario")
        .0
}

fn bench(build: &Path, model: sid_model, cores: u32, reboots: u32, fate: Core1Fate) -> DualRun {
    let boots = reboots as usize + 1;
    run_two_cores(build, BENCH, boots, fate, |soc| {
        write_input(soc, WORD_SCENARIO, scenario());
        write_input(soc, WORD_MODEL, u32::from(model == SID_MODEL_8580));
        write_input(soc, WORD_CORES, cores);
        write_input(soc, WORD_REBOOTS, reboots);
    })
}

fn one_core(build: &Path, model: sid_model) -> DualRun {
    bench(build, model, 1, 0, Core1Fate::default())
}

fn worst(blocks: &[Block], cost: impl Fn(&Block) -> u64) -> f64 {
    blocks.iter().map(cost).max().unwrap_or(0) as f64 / BLOCK
}

fn mean(blocks: &[Block], cost: impl Fn(&Block) -> u64) -> f64 {
    blocks.iter().map(cost).sum::<u64>() as f64 / blocks.len() as f64 / BLOCK
}

fn assert_one_core_after(run: &DualRun, why: u32, one: &DualRun) {
    let last = run.boots.last().unwrap();
    assert_eq!(last.words[WORD_GAVE_UP], why, "why core 1 is off");
    assert_eq!(last.words[WORD_ON], 0, "core 1 retired");
    assert_eq!(last.samples, one.boots[0].samples, "the fallback's samples");
}

#[test]
fn chip_b_on_core_1_renders_the_same_samples_and_frees_core_0() {
    let Some(build) = common::build_or_skip("the second core's bench") else {
        return;
    };
    for model in [SID_MODEL_6581, SID_MODEL_8580] {
        let one = one_core(&build, model);
        let two = bench(&build, model, 2, 0, Core1Fate::default());
        let (one_boot, two_boot) = (&one.boots[0], &two.boots[0]);
        let host = emu_gate::render_host_workload(2, model, scenario());
        assert_eq!(
            two_boot.words[WORD_STARTED], STARTED,
            "core 1 answered its start"
        );
        assert_eq!(
            two_boot.samples, one_boot.samples,
            "two cores against one, sample for sample"
        );
        assert!(
            common::max_diff(&one_boot.samples, &host) <= 1,
            "the target against the host"
        );
        assert_eq!(
            two_boot.words[WORD_JOBS] as usize,
            two.blocks.len(),
            "a job a block"
        );
        assert_eq!(two_boot.words[WORD_GAVE_UP], 0);

        let one_worst = worst(&one.blocks, |b| b.core0);
        let two_worst = worst(&two.blocks, |b| b.core0);
        let saved_mean = mean(&one.blocks, |b| b.core0) - mean(&two.blocks, |b| b.core0);
        let hand_over_fixed = worst(&two.blocks, |b| b.hand_over - b.spin);
        println!(
            "{}",
            serde_json::json!({
                "model": if model == SID_MODEL_8580 { "8580" } else { "6581" },
                "scenario": WORST_BLOCK_SCENARIO,
                "blocks": two.blocks.len(),
                "core0_worst_block_one_core": one_worst,
                "core0_worst_block_two_cores": two_worst,
                "core0_saved_worst": one_worst - two_worst,
                "core0_saved_mean": saved_mean,
                "core0_hand_over_worst_fixed": hand_over_fixed,
                "core0_hand_over_mean_fixed": mean(&two.blocks, |b| b.hand_over - b.spin),
                "core0_spin_worst": worst(&two.blocks, |b| b.spin),
                "core0_spin_mean": mean(&two.blocks, |b| b.spin),
                "core1_job_worst": worst(&two.blocks, |b| b.core1_job),
                "core1_mailbox_mean": mean(&two.blocks, |b| b.core1_mailbox),
            })
        );
        assert!(
            two_worst
                <= common::SID_WORST_BLOCK_ONE_CHIP
                    + hand_over_fixed
                    + worst(&two.blocks, |b| b.spin),
            "core 0's worst block with chip B away: {two_worst:.1} a sample"
        );
        assert!(
            two_worst < one_worst - common::SID_WORST_BLOCK_ONE_CHIP / 2.0,
            "chip B away saves core 0 less than half a chip: {one_worst:.1} -> {two_worst:.1}"
        );
    }
}

#[test]
fn a_core_1_that_misreads_falls_back_to_one_core_with_the_same_samples() {
    let Some(build) = common::build_or_skip("the second core's bench") else {
        return;
    };
    let fate = Core1Fate {
        flaky: Some((FLAKY_RATE, FLAKY_SEED)),
        ..Core1Fate::default()
    };
    let flaky = bench(&build, SID_MODEL_6581, 2, 0, fate);
    assert_eq!(flaky.boots[0].words[WORD_STARTED], STARTED);
    assert!(flaky.core1_misreads > 0);
    assert_eq!(
        flaky.boots[0].words[WORD_JOBS], 0,
        "retired at its first look, before a job"
    );
    assert_one_core_after(&flaky, GAVE_UP_MISREAD, &one_core(&build, SID_MODEL_6581));
}

#[test]
fn one_misread_in_core_1s_mailbox_mid_song_falls_back_with_the_same_samples() {
    let Some(build) = common::build_or_skip("the second core's bench") else {
        return;
    };
    let fate = Core1Fate {
        misreads_mailbox_after_jobs: Some(JOBS_BEFORE_THE_MISREAD),
        ..Core1Fate::default()
    };
    let run = bench(&build, SID_MODEL_6581, 2, 0, fate);
    assert_eq!(run.core1_misreads, 1);
    assert_eq!(
        run.boots[0].words[WORD_JOBS], JOBS_BEFORE_THE_MISREAD,
        "no job after the misread"
    );
    assert_one_core_after(&run, GAVE_UP_MISREAD, &one_core(&build, SID_MODEL_6581));
}

#[test]
fn a_reset_holds_a_running_core_1_and_the_next_boot_starts_it_again() {
    let Some(build) = common::build_or_skip("the second core's bench") else {
        return;
    };
    let run = bench(&build, SID_MODEL_6581, 2, 1, Core1Fate::default());
    assert_eq!(
        run.boots[0].words[WORD_ON], 1,
        "core 1 running when the reset is asked for"
    );
    assert_eq!(
        run.core1_held_at_resets,
        [true],
        "core 1 held when the reset lands"
    );
    let again = &run.boots[1];
    assert_eq!(again.words[WORD_BOOT], 2);
    assert_eq!(again.words[WORD_STARTED], STARTED);
    assert_eq!(again.words[WORD_JOBS] as usize, run.blocks.len());
    assert_eq!(
        again.samples,
        one_core(&build, SID_MODEL_6581).boots[0].samples
    );
}

#[test]
fn after_a_fallback_the_next_boot_stays_on_one_core() {
    let Some(build) = common::build_or_skip("the second core's bench") else {
        return;
    };
    let fate = Core1Fate {
        flaky: Some((FLAKY_RATE, FLAKY_SEED)),
        ..Core1Fate::default()
    };
    let run = bench(&build, SID_MODEL_6581, 2, 1, fate);
    let again = &run.boots[1];
    assert_eq!(again.words[WORD_BOOT], 2);
    assert_eq!(
        again.words[WORD_STARTED], HELD,
        "the second boot keeps one core"
    );
    assert_eq!(again.words[WORD_JOBS], 0);
    assert_one_core_after(&run, GAVE_UP_BARRED, &one_core(&build, SID_MODEL_6581));
}

#[test]
fn a_core_1_that_stops_answering_times_out_to_one_core() {
    let Some(build) = common::build_or_skip("the second core's bench") else {
        return;
    };
    let fate = Core1Fate {
        stalls_after: Some(STALLS_IDLE_AFTER),
        ..Core1Fate::default()
    };
    let stalled = bench(&build, SID_MODEL_6581, 2, 0, fate);
    assert_eq!(stalled.boots[0].words[WORD_STARTED], STARTED);
    assert_one_core_after(&stalled, GAVE_UP_LATE, &one_core(&build, SID_MODEL_6581));
}
