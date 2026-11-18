// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::path::PathBuf;

use common::dual::{Core1Fate, Misreads, NO_FATE};
use common::panel::{press, turn, ALGORITHM, HOME, SAVE, SELECT};
use common::ring::{chip_b_use, note, push, settle, with_b, worst_on_both_chips};
use common::{
    boot_with_core1, firmware, open_status, run_past_boot, top_view, Firmware, Machine,
    CORE1_FAULTS, CORE1_LOOP, SID_WORST_BLOCK_ONE_CHIP, TAP_PER_SAMPLE,
};
use fm1_core::Bus;
use sidou_sys::{ilp32, GRID_BIG_FIRST, GRID_BIG_PIECES, GRID_COLS};

const ONE_CORE_BUILD: &str = "SIDOU_FM1_ONE_CORE_BUILD";
const MAILBOX_REJECTED: u32 = 40;
const MAILBOX_JOBS: u32 = 48;
const BAR_BARRED: u32 = 4;
const FAULT_PC: u32 = 20;
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

struct Builds {
    one: Firmware,
    two: Firmware,
}

fn builds(what: &str) -> Option<Builds> {
    let one = std::env::var_os(ONE_CORE_BUILD).filter(|d| !d.is_empty());
    let (Some(two), Some(one)) = (common::build_dir(), one) else {
        println!(
            "skipping {what}: set SIDOU_FM1_BUILD to the fm1 build and {ONE_CORE_BUILD} to a \
             build with -DSIDOU_CPU2=OFF (build/fm1-cpu1)"
        );
        return None;
    };
    let one = std::path::absolute(PathBuf::from(one)).expect("resolve the one-core build");
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
    boot_after(fw, fate, &[])
}

fn boot_after(fw: &Firmware, fate: Core1Fate, bar: &[u8]) -> Machine {
    let mut m = boot_with_core1(fw, fate);
    for (i, b) in bar.iter().enumerate() {
        m.soc.write8(fw.symbol("fm1_cpu1_bar") + i as u32, *b);
    }
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
        ("faults at its first FPU instruction", CORE1_FAULTS),
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
    let Some(fw) = firmware("the two-core flash gate test") else {
        return;
    };
    let fw = &fw;
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

const CORES_PAGE_FROM_THE_FIRST: i32 = -1;
const CORES_FIRST_ROW: u32 = 5;
const CORES_LINE_ROWS: u32 = 3;
const CORES_DETAIL_ROW: u32 = 22;
const CORES_DETAILS: u32 = 3;
const CORES_ASK_ROW: u32 = 25;
const BIG_FIRST_COL: u32 = 2;
const BIG_LAST_COL: u32 = 26;
const METER_WINDOW_MS: u64 = 1_200;
const BAR_BYTES: u32 = 12;
const PERCENT: f64 = 100.0;
const CPU_HZ: f64 = 240e6;
const SAMPLE_RATE: f64 = 44_118.0;
const METER_TOLERANCE: f64 = 0.1;

#[derive(Debug)]
struct CoresPage {
    core1: String,
    why: String,
    jobs: u32,
    chip_b: String,
    avg: f64,
    worst: f64,
    details: Vec<String>,
    asks: String,
}

fn grid_cell(m: &mut Machine, fw: &Firmware, field: usize, row: u32, col: u32) -> u8 {
    m.soc
        .read8(fw.symbol("cells") + field as u32 + row * GRID_COLS + col)
}

fn big_row(m: &mut Machine, fw: &Firmware, row: u32) -> String {
    (BIG_FIRST_COL..=BIG_LAST_COL)
        .step_by(2)
        .map(|col| {
            let piece = grid_cell(m, fw, ilp32::grid::ch, row, col);
            let big = piece.wrapping_sub(GRID_BIG_FIRST as u8) < GRID_BIG_PIECES as u8;
            if big {
                char::from(grid_cell(m, fw, ilp32::grid::src, row, col))
            } else {
                ' '
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn small_row(m: &mut Machine, fw: &Firmware, row: u32) -> String {
    (1..GRID_COLS - 1)
        .map(|col| char::from(grid_cell(m, fw, ilp32::grid::ch, row, col)))
        .collect::<String>()
        .trim()
        .to_string()
}

fn cores_line(m: &mut Machine, fw: &Firmware, line: u32, label: &str) -> String {
    let text = big_row(m, fw, CORES_FIRST_ROW + line * CORES_LINE_ROWS);
    text.strip_prefix(label)
        .unwrap_or_else(|| panic!("the CORES page's line {line} is {text:?}, not {label}"))
        .trim()
        .to_string()
}

fn percent(text: &str) -> f64 {
    text.trim_end_matches('%')
        .parse()
        .unwrap_or_else(|_| panic!("{text:?} is not a percentage"))
}

fn cores_page(m: &mut Machine, fw: &Firmware) -> CoresPage {
    CoresPage {
        core1: cores_line(m, fw, 0, "CORE 1"),
        why: cores_line(m, fw, 1, "WHY"),
        jobs: cores_line(m, fw, 2, "JOBS")
            .parse()
            .expect("JOBS is a number"),
        chip_b: cores_line(m, fw, 3, "CHIP B"),
        avg: percent(&cores_line(m, fw, 4, "AVG")),
        worst: percent(&cores_line(m, fw, 5, "WORST")),
        details: (0..CORES_DETAILS)
            .map(|i| small_row(m, fw, CORES_DETAIL_ROW + i))
            .collect(),
        asks: big_row(m, fw, CORES_ASK_ROW),
    }
}

fn to_cores_page(m: &mut Machine, fw: &Firmware) -> CoresPage {
    turn(m, SELECT, CORES_PAGE_FROM_THE_FIRST);
    m.run_ms(METER_WINDOW_MS);
    cores_page(m, fw)
}

fn open_cores(m: &mut Machine, fw: &Firmware) -> CoresPage {
    open_status(m, fw);
    to_cores_page(m, fw)
}

fn both_chips_at_their_worst(m: &mut Machine, fw: &Firmware) {
    settle(m, fw);
    push(m, fw, &worst_on_both_chips());
    chip_b_use(m, fw, true);
}

fn audio_time_in_the_emulator(m: &Machine, from: usize) -> f64 {
    let blocks = &m.alnk_blocks[from..];
    let mean = blocks.iter().sum::<u64>() as f64 / blocks.len() as f64;
    mean * SAMPLE_RATE / (CPU_HZ * BLOCK_FRAMES) * PERCENT
}

#[test]
fn the_cores_page_shows_core_1_running_and_core_0s_audio_time_with_chip_b_on_either_core() {
    let Some(fw) = firmware("the CORES page test") else {
        return;
    };
    let fw = &fw;
    let mut m = boot(fw, NO_FATE);
    both_chips_at_their_worst(&mut m, fw);
    let two = open_cores(&mut m, fw);
    let from = m.alnk_blocks.len();
    m.run_ms(METER_WINDOW_MS);
    let measured = audio_time_in_the_emulator(&m, from);
    let two = CoresPage {
        avg: cores_page(&mut m, fw).avg,
        ..two
    };
    turn(&mut m, ALGORITHM, 1);
    m.run_ms(METER_WINDOW_MS);
    let one = cores_page(&mut m, fw);
    m.run_ms(METER_WINDOW_MS);
    let one_later = cores_page(&mut m, fw);
    turn(&mut m, ALGORITHM, 1);
    m.run_ms(METER_WINDOW_MS);
    let back = cores_page(&mut m, fw);
    println!(
        "two cores {two:?}\none core {one:?}\nback {back:?}\nthe emulator's own count: {measured:.1} %"
    );
    assert_eq!(
        (two.core1.as_str(), two.why.as_str(), two.chip_b.as_str()),
        ("RUNNING", "--", "CORE 1")
    );
    assert!(two.jobs > SELF_TEST_BLOCKS, "no jobs counted: {two:?}");
    assert!(
        (two.avg - measured).abs() <= measured * METER_TOLERANCE,
        "the page's AVG {} % against the emulator's {measured:.1} %",
        two.avg
    );
    assert!(two.avg <= two.worst, "{two:?}");
    assert_eq!(
        (one.core1.as_str(), one.chip_b.as_str()),
        ("RUNNING", "CORE 0")
    );
    assert_eq!(one_later.jobs, one.jobs, "core 1 still gets jobs on CORE 0");
    assert!(
        one.avg > two.avg + 1.0,
        "chip B on core 0 costs the audio no more: {} % against {} %",
        one.avg,
        two.avg
    );
    assert_eq!(back.chip_b, "CORE 1");
    assert!(
        back.jobs > one_later.jobs,
        "core 1 got no jobs back: {back:?}"
    );
}

fn fell_back(fw: &Firmware, fate: Core1Fate) -> Machine {
    let mut m = boot(fw, fate);
    both_chips_at_their_worst(&mut m, fw);
    m.run_ms(METER_WINDOW_MS);
    m
}

fn bar_of(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let at = fw.symbol("fm1_cpu1_bar");
    (0..BAR_BYTES).map(|i| m.soc.read8(at + i)).collect()
}

#[test]
fn the_cores_page_says_why_core_1_fell_back() {
    let Some(fw) = firmware("the CORES page fallback test") else {
        return;
    };
    let fw = &fw;
    let fates = [
        (
            "MISREAD",
            misreads(MAILBOX_MISREAD_RATE, SEED, Misreads::Anywhere),
        ),
        (
            "SELF-TEST",
            misreads(MAILBOX_MISREAD_RATE, SEED, Misreads::InJobs),
        ),
        (
            "LATE",
            Core1Fate {
                stalls_after_jobs: Some(CHIP_B_PLAYING_JOBS),
                ..NO_FATE
            },
        ),
        ("FAULT", CORE1_FAULTS),
    ];
    for (why, fate) in fates {
        let mut m = fell_back(fw, fate);
        let page = open_cores(&mut m, fw);
        let fault_pc = m.soc.read32(fw.symbol("fm1_cpu1_fault") + FAULT_PC);
        println!("{why}: {page:?}");
        assert_eq!(
            (page.core1.as_str(), page.why.as_str(), page.chip_b.as_str()),
            ("OFF", why, "CORE 0")
        );
        let pc_line = format!("PC {fault_pc:08X}");
        assert_eq!(
            page.details[1].starts_with(&pc_line),
            why == "FAULT",
            "the fault's line: {:?}",
            page.details
        );
        let bar = bar_of(&mut m, fw);
        let mut next = boot_after(fw, NO_FATE, &bar);
        let barred = open_cores(&mut next, fw);
        assert_eq!(
            (barred.core1.as_str(), barred.why.as_str()),
            ("OFF", "BARRED"),
            "the boot after {why}"
        );
    }
}

#[test]
fn save_then_save_again_on_the_cores_page_lifts_the_bar_and_restarts_on_two_cores() {
    let Some(fw) = firmware("the CORES page restart test") else {
        return;
    };
    let fw = &fw;
    let mut fallen = fell_back(fw, CORE1_FAULTS);
    let bar = bar_of(&mut fallen, fw);
    let mut m = boot_after(fw, NO_FATE, &bar);
    assert_eq!(open_cores(&mut m, fw).why, "BARRED");
    press(&mut m, SAVE);
    m.run_ms(METER_WINDOW_MS);
    assert_eq!(cores_page(&mut m, fw).asks, "SAVE AGAIN");
    press(&mut m, HOME);
    m.run_ms(METER_WINDOW_MS);
    assert!(m.resets.is_empty(), "HOME restarted it");
    assert_eq!(
        top_view(&mut m, fw),
        fw.symbol("views_menu"),
        "HOME left STATUS"
    );
    turn(&mut m, ALGORITHM, 1);
    m.run_ms(METER_WINDOW_MS);
    let page = to_cores_page(&mut m, fw);
    assert_eq!((page.why.as_str(), page.asks.as_str()), ("BARRED", ""));
    press(&mut m, SAVE);
    press(&mut m, SAVE);
    assert_eq!(m.resets.len(), 1, "SAVE, SAVE AGAIN did not restart");
    run_past_boot(&mut m, fw);
    let page = open_cores(&mut m, fw);
    assert_eq!((page.core1.as_str(), page.why.as_str()), ("RUNNING", "--"));
}
