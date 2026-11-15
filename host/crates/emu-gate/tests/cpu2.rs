// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::path::PathBuf;

use common::dual::{Core1Fate, Misreads};
use common::ring::{chip_b_use, note, push, settle, with_b, worst_on_both_chips};
use common::{run_past_boot, Firmware, Machine, SID_WORST_BLOCK_ONE_CHIP, TAP_PER_SAMPLE};
use fm1_core::Bus;

const CPU2_BUILD: &str = "SIDOU_FM1_CPU2_BUILD";
const CORE1_LOOP: &str = "fm1_cpu1_main";
const MAILBOX_REJECTED: u32 = 40;
const MAILBOX_JOBS: u32 = 48;
const BAR_BARRED: u32 = 4;
const SELF_TEST_BLOCKS: u32 = 100;

const SAMPLES_PER_MS: usize = 88;
const BLOCK_SAMPLES: usize = 256;
const FIRST_CUE: usize = 200_000;
const CUE_STEP_CYCLES: u64 = 500;
const MID_BLOCK_CYCLES: u64 = 348_000;
const BLOCK_FRAMES: f64 = 128.0;

const CHIP_B_PLAYING_JOBS: u32 = SELF_TEST_BLOCKS + 150;
const MAILBOX_MISREAD_RATE: f64 = 1e-5;
const SEED: u64 = 0;
const DEFAULT_SWEEP: &str = "jobs:1e-5:8";
const SAVE_SONG: u32 = 2;

const NO_FATE: Core1Fate = Core1Fate {
    flaky: None,
    misreads_mailbox_after_jobs: None,
    stalls_after: None,
    misreads: Misreads::Anywhere,
    flaky_after_jobs: None,
    stalls_after_jobs: None,
    faults_at_fpu: false,
};

struct Builds {
    one: Firmware,
    two: Firmware,
}

fn builds(what: &str) -> Option<Builds> {
    let two = std::env::var_os(CPU2_BUILD).filter(|d| !d.is_empty());
    let (Some(one), Some(two)) = (common::build_dir(), two) else {
        println!(
            "skipping {what}: set SIDOU_FM1_BUILD to the fm1 build and {CPU2_BUILD} to a build \
             with -DSIDOU_CPU2=ON (build/fm1-cpu2)"
        );
        return None;
    };
    let two = std::path::absolute(PathBuf::from(two)).expect("resolve the two-core build");
    Some(Builds {
        one: Firmware::load(&one),
        two: Firmware::load(&two),
    })
}

#[derive(Debug)]
struct Core1State {
    on: bool,
    barred: bool,
    jobs: u32,
    rejected: u32,
    faults: u32,
}

fn core1_state(m: &mut Machine, fw: &Firmware) -> Core1State {
    let mailbox = fw.symbol("fm1_cpu1_mb");
    Core1State {
        on: m.soc.read8(fw.symbol("fm1_cpu1_on")) != 0,
        barred: m.soc.read32(fw.symbol("fm1_cpu1_bar") + BAR_BARRED) == 1,
        jobs: m.soc.read32(mailbox + MAILBOX_JOBS),
        rejected: m.soc.read32(mailbox + MAILBOX_REJECTED),
        faults: m.soc.read32(fw.symbol("fm1_cpu1_fault")),
    }
}

fn boot(fw: &Firmware, fate: Core1Fate) -> Machine {
    let mut m = Machine::boot(&fw.image);
    if fw.has_symbol(CORE1_LOOP) {
        m.core1.mailbox_loop = fw.function_range(CORE1_LOOP);
    }
    m.core1.fate = fate;
    run_past_boot(&mut m, fw);
    m
}

fn cue(m: &mut Machine, ms: usize) {
    let to = FIRST_CUE + ms * SAMPLES_PER_MS;
    assert!(m.soc.periph.audio.len() <= to, "the cue at {ms} ms passed");
    while m.soc.periph.audio.len() < to {
        m.run_cycles(CUE_STEP_CYCLES);
    }
    m.run_cycles(MID_BLOCK_CYCLES);
}

fn write(m: &mut Machine, fw: &Firmware, writes: &[(u8, u8)]) {
    settle(m, fw);
    push(m, fw, writes);
}

fn tap_on_chip_b(m: &mut Machine, fw: &Firmware, on: bool) {
    m.soc.write8(fw.symbol("tap_chip"), u8::from(on));
}

fn both_chips_played(m: &mut Machine, fw: &Firmware) {
    cue(m, 0);
    let mut first = note(0, 0x1cd6, 0x41);
    first.extend(note(1, 0x0e6b, 0x21));
    first.extend(with_b(&note(0, 0x1167, 0x11)));
    first.extend(with_b(&note(1, 0x2a00, 0x81)));
    first.extend(with_b(&[(0x15, 0x03), (0x16, 0x40), (0x17, 0xf3)]));
    write(m, fw, &first);
    chip_b_use(m, fw, true);
    cue(m, 300);
    tap_on_chip_b(m, fw, true);
    write(m, fw, &with_b(&note(2, 0x0800, 0x15)));
    cue(m, 500);
    write(m, fw, &[(0x04, 0x40)]);
    write(m, fw, &with_b(&[(0x16, 0x10), (0x18, 0x2f)]));
    cue(m, 800);
    tap_on_chip_b(m, fw, false);
    write(m, fw, &with_b(&[(0x04, 0x10), (0x0b, 0x80)]));
    cue(m, 900);
    chip_b_use(m, fw, false);
    cue(m, 1200);
}

struct Heard {
    audio: Vec<i16>,
    core1: Option<Core1State>,
}

fn heard(fw: &Firmware, fate: Core1Fate) -> Heard {
    let mut m = boot(fw, fate);
    both_chips_played(&mut m, fw);
    let core1 = fw.has_symbol(CORE1_LOOP).then(|| core1_state(&mut m, fw));
    Heard {
        audio: m.soc.periph.audio[FIRST_CUE..].to_vec(),
        core1,
    }
}

fn first_difference(one: &Heard, two: &Heard) -> Option<String> {
    let (a, b) = (&one.audio, &two.audio);
    (0..a.len().max(b.len()))
        .find(|&i| a.get(i) != b.get(i))
        .map(|i| {
            format!(
                "sample {i} ({:.1} ms after the first cue) of {}",
                i as f64 / SAMPLES_PER_MS as f64,
                a.len()
            )
        })
}

fn misreads(rate: f64, seed: u64, misreads: Misreads) -> Core1Fate {
    Core1Fate {
        flaky: Some((rate, seed)),
        misreads,
        ..NO_FATE
    }
}

fn after_jobs(jobs: u32, fate: Core1Fate) -> Core1Fate {
    Core1Fate {
        flaky_after_jobs: Some(jobs),
        ..fate
    }
}

#[test]
fn chip_b_on_core_1_sounds_sample_for_sample_as_one_core() {
    let Some(b) = builds("the two-core sound test") else {
        return;
    };
    let one = heard(&b.one, NO_FATE);
    let two = heard(&b.two, NO_FATE);
    let core1 = two.core1.as_ref().expect("the two-core build");
    let blocks = (two.audio.len() / BLOCK_SAMPLES) as u32;
    println!(
        "both chips played: {} samples, {} jobs on core 1 ({SELF_TEST_BLOCKS} of the self-test)",
        two.audio.len(),
        core1.jobs
    );
    assert!(core1.on && !core1.barred, "core 1 retired: {core1:?}");
    assert!(
        core1.jobs >= SELF_TEST_BLOCKS + blocks * 3 / 4,
        "chip B hardly went to core 1: {core1:?} in {blocks} blocks"
    );
    assert_eq!(first_difference(&one, &two), None, "two cores against one");
}

fn worst_blocks(fw: &Firmware) -> (f64, f64) {
    let mut m = boot(fw, NO_FATE);
    m.run_ms(500);
    settle(&mut m, fw);
    push(&mut m, fw, &worst_on_both_chips());
    chip_b_use(&mut m, fw, true);
    m.run_ms(300);
    let from = m.alnk_blocks.len();
    m.run_ms(1000);
    let worst = |v: &[u64]| v[from..].iter().copied().max().unwrap_or(0) as f64 / BLOCK_FRAMES;
    (worst(&m.alnk_blocks), worst(&m.core1_alnk_jobs))
}

#[test]
fn core_0s_worst_block_drops_by_chip_bs_render_on_core_1() {
    let Some(b) = builds("the two-core budget test") else {
        return;
    };
    let (one, _) = worst_blocks(&b.one);
    let (two, core1) = worst_blocks(&b.two);
    println!(
        "{}",
        serde_json::json!({
            "core0_worst_block_one_core": one,
            "core0_worst_block_two_cores": two,
            "core0_saved": one - two,
            "core1_worst_job": core1,
        })
    );
    assert!(
        two < one - SID_WORST_BLOCK_ONE_CHIP / 2.0,
        "chip B on core 1 saves core 0 less than half a chip: {one:.1} -> {two:.1} a sample"
    );
    assert!(
        core1 <= SID_WORST_BLOCK_ONE_CHIP + TAP_PER_SAMPLE,
        "core 1's job, chip B's render, costs {core1:.1} a sample"
    );
}

#[test]
fn every_fallback_to_one_core_sounds_the_same_samples() {
    let Some(b) = builds("the two-core fallback test") else {
        return;
    };
    let one = heard(&b.one, NO_FATE);
    let fates = [
        (
            "misreads from power-on",
            misreads(MAILBOX_MISREAD_RATE, SEED, Misreads::Anywhere),
        ),
        (
            "misreads its mailbox as chip B plays",
            after_jobs(
                CHIP_B_PLAYING_JOBS,
                misreads(MAILBOX_MISREAD_RATE, SEED, Misreads::InTheLoop),
            ),
        ),
        (
            "stops answering",
            Core1Fate {
                stalls_after_jobs: Some(CHIP_B_PLAYING_JOBS),
                ..NO_FATE
            },
        ),
        (
            "faults at its first FPU instruction",
            Core1Fate {
                faults_at_fpu: true,
                ..NO_FATE
            },
        ),
    ];
    for (what, fate) in fates {
        let two = heard(&b.two, fate);
        let core1 = two.core1.as_ref().expect("the two-core build");
        println!("a core 1 that {what}: {core1:?}");
        assert!(
            !core1.on && core1.barred,
            "a core 1 that {what} still runs: {core1:?}"
        );
        assert_eq!(
            first_difference(&one, &two),
            None,
            "a core 1 that {what}: one core's samples"
        );
        let (by, retired) = match what {
            "misreads from power-on" => ("the mailbox check", core1.rejected > 0),
            "misreads its mailbox as chip B plays" => (
                "the mailbox check",
                core1.rejected > 0 && core1.jobs > SELF_TEST_BLOCKS,
            ),
            "stops answering" => (
                "the timeout",
                core1.jobs > SELF_TEST_BLOCKS && core1.rejected == 0 && core1.faults == 0,
            ),
            _ => ("the fault path", core1.faults > 0 && core1.jobs == 0),
        };
        assert!(retired, "a core 1 that {what}, not by {by}: {core1:?}");
    }
}

#[derive(Default, Debug, Clone, Copy)]
struct Outcomes {
    caught_clean: u32,
    caught_late: u32,
    harmless: u32,
    unseen: u32,
}

fn outcomes(one: &Heard, runs: &[Heard]) -> Outcomes {
    let mut o = Outcomes::default();
    for two in runs {
        let on = two.core1.as_ref().expect("the two-core build").on;
        let same = first_difference(one, two).is_none();
        match (on, same) {
            (false, true) => o.caught_clean += 1,
            (false, false) => o.caught_late += 1,
            (true, true) => o.harmless += 1,
            (true, false) => o.unseen += 1,
        }
    }
    o
}

impl Outcomes {
    fn damaged(&self) -> u32 {
        self.caught_late + self.unseen
    }
}

struct Sweep {
    misreads: Misreads,
    rate: f64,
    units: u64,
}

fn sweep() -> Sweep {
    let spec = std::env::var("SIDOU_CPU2_SWEEP").unwrap_or_else(|_| DEFAULT_SWEEP.into());
    let parts: Vec<&str> = spec.split(':').collect();
    let [place, rate, units] = parts[..] else {
        panic!("SIDOU_CPU2_SWEEP is anywhere|jobs:<rate>:<units>, not {spec}");
    };
    Sweep {
        misreads: if place == "jobs" {
            Misreads::InJobs
        } else {
            Misreads::Anywhere
        },
        rate: rate.parse().expect("the sweep's rate"),
        units: units.parse().expect("the sweep's units"),
    }
}

#[test]
fn the_self_test_keeps_a_misreading_core_1s_render_out_of_the_sound() {
    let Some(b) = builds("the two-core misread sweep") else {
        return;
    };
    let sweep = sweep();
    let one = heard(&b.one, NO_FATE);
    let seeds: Vec<u64> = (0..sweep.units).collect();
    let run = |from_power_on: bool| {
        let runs = common::par_map(&seeds, |&seed| {
            let unit = misreads(sweep.rate, seed, sweep.misreads);
            let fate = if from_power_on {
                unit
            } else {
                after_jobs(SELF_TEST_BLOCKS, unit)
            };
            heard(&b.two, fate)
        });
        outcomes(&one, &runs)
    };
    let (without, with) = (run(false), run(true));
    println!(
        "{}",
        serde_json::json!({
            "misreads_in_jobs_only": sweep.misreads == Misreads::InJobs,
            "rate_a_load": sweep.rate,
            "units": sweep.units,
            "without_the_self_test": format!("{without:?}"),
            "with_the_self_test": format!("{with:?}"),
        })
    );
    assert!(
        with.damaged() <= without.damaged() && with.caught_clean >= without.caught_clean,
        "the self-test made it worse: {with:?} against {without:?}"
    );
    if sweep.misreads == Misreads::InJobs {
        assert!(
            without.unseen > 0,
            "without the self-test no misread in the render reached the sound: {without:?}"
        );
        assert_eq!(
            with.caught_clean as u64, sweep.units,
            "the self-test let a misreading render through: {with:?}"
        );
    }
}

#[test]
fn a_flash_write_never_runs_while_core_1_has_a_job() {
    let Some(b) = builds("the two-core flash gate test") else {
        return;
    };
    let fw = &b.two;
    let mut m = boot(fw, NO_FATE);
    cue(&mut m, 0);
    write(&mut m, fw, &worst_on_both_chips());
    chip_b_use(&mut m, fw, true);
    cue(&mut m, 100);
    let jobs = core1_state(&mut m, fw).jobs;
    common::song_io(&mut m, fw, SAVE_SONG, "CORES");
    let core1 = core1_state(&mut m, fw);
    let watch = &m.flash_watch;
    println!(
        "a song saved while chip B plays on core 1: {} instructions with the flash busy, {} of \
         them with a job on core 1; {} jobs meanwhile",
        watch.busy_steps,
        watch.busy_core1_in_job,
        core1.jobs - jobs
    );
    assert!(watch.busy_steps > 0, "nothing written to the flash");
    assert!(core1.on && core1.jobs > jobs, "core 1 stopped: {core1:?}");
    assert_eq!(watch.busy_core1_in_job, 0, "a flash write beside a job");
    assert!(common::stored_file(&m.soc.flash.mem, "CORES").is_some());
}

const NOINIT: std::ops::Range<u32> = 0x01C7_C000..0x01C7_FD50;
const IN_SRAM: [&str; 7] = [
    CORE1_LOOP,
    "fm1_cpu1_retire",
    "cpu2_core1_run",
    "cpu2_core1_wait",
    "cpu2_chip_b_away",
    "cpu2_chip_b_back",
    "cpu2_render_job",
];

#[test]
fn only_the_two_core_build_has_core_1_its_loop_in_sram_and_its_stacks_in_noinit() {
    let Some(b) = builds("the two-core layout test") else {
        return;
    };
    for name in IN_SRAM {
        assert!(!b.one.has_symbol(name), "the default build has {name}");
    }
    let fw = &b.two;
    let sram = fw.symbol("_rt_start")..fw.symbol("_rt_end");
    for name in IN_SRAM {
        let at = fw.function_range(name);
        assert!(
            sram.contains(&at.start) && at.end <= sram.end,
            "{name} at {at:x?}, outside .ram_text"
        );
    }
    let stacks = fw.symbol("fm1_cpu1_ustack")..fw.symbol("fm1_cpu1_sstack_top");
    println!(
        "core 1's stacks {stacks:x?} ({} bytes); .ram_text {} bytes against {}; image {} bytes \
         against {}",
        stacks.len(),
        sram.len(),
        b.one.symbol("_rt_end") - b.one.symbol("_rt_start"),
        fw.image.len(),
        b.one.image.len()
    );
    assert!(NOINIT.contains(&stacks.start) && stacks.end <= NOINIT.end);
}
