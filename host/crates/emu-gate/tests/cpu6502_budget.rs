// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's 6502 on fm1-emu (M3 Task 2, docs/plans/2026-10-19-m3-sid-files.md): the device
//! build of core/cpu6502 (CPU6502_SMALL, -Os) in a bench image (bench/fm1/bench_6502_main.c),
//! stepped as pi32v2 code.
//!
//! * Dormann's functional test reaches its success trap: the decoder check. fm1-emu stops on an
//!   encoding it cannot decode (decision record 0008, Risks), so a run that finishes means the
//!   compiler's output for the dispatch decodes at the pinned emulator.
//! * Three GoatTracker example songs, packed by gt2-ref's `pack` with its default options, play
//!   for 10 s of 6502 time (init, then a play call every PAL frame): the SID writes the 6502
//!   makes equal the host 6502's, and the instructions a 6502 cycle costs, on average, stay
//!   within CPU6502_INSTR_PER_CYCLE (set from the worst song's average plus 5 %); the worst
//!   call is reported.
//! * Each of the 256 opcodes alone, the call's own cost taken off: instructions an opcode, by
//!   its cycle count and by opcode, and the I/O page's accesses through the hooks.
//!
//! * The voice shadow (M3 Task 5b, core/sid/sid_shadow.h) beside the same players: what a
//!   tune's writes and a frame's catch-up cost on average (SHADOW_FRAME_MEAN), and its dearest
//!   catch-up of a look-ahead slice (SHADOW_SLICE_WORST).
//!
//! Instructions are not device cycles (docs/hardware.md, "Open checks").

mod common;

use common::*;
use fm1_core::Bus;
use gt2_ref::trace::{run, Setup};
use gt2_ref::{pack, Options};

const PARAM_ADDR: u32 = 0x01CB_0000;
const MEM_ADDR: u32 = 0x01CC_0000;
const PAL_FRAME_CYCLES: u32 = 19_656;
/// 10 s of 6502 time at the PAL clock (985,248 Hz): 501 frames of 19,656 cycles.
const FRAMES: u32 = 501;
const DORMANN_SUCCESS: u32 = 0x3469;

/// The three example songs of the workload: the costliest per 6502 cycle of GoatTracker's 14
/// (the `every_example_song` survey, run with `--ignored`: 8.12 to 8.88 instructions a cycle).
const SONGS: [&str; 3] = ["wavecmdtest.sng", "dojo.sng", "funktest.sng"];

fn build() -> Option<std::path::PathBuf> {
    let b = build_dir();
    if b.is_none() {
        skip_message("the 6502 on fm1-emu");
    }
    b
}

fn dormann_image() -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../third_party/6502-tests/6502_functional_test.bin"
    );
    let bin = std::fs::read(path).expect("6502_functional_test.bin");
    assert_eq!(bin.len(), 0x10000);
    bin
}

fn load(soc: &mut fm1_soc::Soc, image: &[u8], params: &[u32]) {
    for (i, &b) in image.iter().enumerate() {
        if b != 0 {
            soc.write8(MEM_ADDR + i as u32, b);
        }
    }
    for (i, &v) in params.iter().enumerate() {
        soc.write32(PARAM_ADDR + 4 * i as u32, v);
    }
}

/// What one run of a player gave.
struct Workload {
    /// Instructions of each call (init first) and the 6502 cycles it took.
    calls: Vec<(u64, u32)>,
    writes: u32,
    sum: u32,
    over: u32,
    cycles: u32,
}

impl Workload {
    fn instructions(&self) -> u64 {
        self.calls.iter().map(|c| c.0).sum()
    }
    fn busy_cycles(&self) -> u64 {
        self.calls.iter().map(|c| u64::from(c.1)).sum()
    }
    fn per_cycle(&self) -> f64 {
        self.instructions() as f64 / self.busy_cycles() as f64
    }
    /// The call with the most instructions, and the one with the most per cycle.
    fn worst(&self) -> ((u64, u32), f64) {
        let most = *self.calls.iter().max_by_key(|c| c.0).unwrap();
        let ratio = self
            .calls
            .iter()
            .map(|c| c.0 as f64 / f64::from(c.1))
            .fold(0.0, f64::max);
        (most, ratio)
    }
}

fn play(build: &std::path::Path, song: &[u8]) -> (Workload, Vec<(u8, u8)>) {
    let o = Options::default();
    let packed = pack(song, &o).expect("pack");
    let setup = Setup::of(&packed, &o);
    assert_eq!(setup.sid, 0xD400);
    let mut image = vec![0u8; 0x10000];
    image[packed.load as usize..packed.load as usize + packed.code.len()]
        .copy_from_slice(&packed.code);
    let r = run_with(build, "sid_bench_6502", 2 * (1 + FRAMES as usize), |soc| {
        load(
            soc,
            &image,
            &[
                1,
                u32::from(setup.init),
                u32::from(setup.play),
                0,
                FRAMES,
                PAL_FRAME_CYCLES,
            ],
        );
    });
    assert_eq!(
        r.words[2], 1,
        "a call did not return, at ${:04x}",
        r.words[7]
    );
    assert_eq!(r.samples.len(), 1 + FRAMES as usize);
    let calls = (0..r.samples.len())
        .map(|i| (r.phases[2 * i], r.samples[i] as u16 as u32))
        .collect();

    // The host 6502's writes: the same player, the same frames.
    let mut cpu = gt2_ref::trace::load(&packed, setup.sid).expect("place");
    let t = run(&mut cpu, &setup, 0, FRAMES as usize).expect("host trace");
    let mut expect: Vec<(u8, u8)> = t.init.clone();
    for f in &t.frames {
        for c in &f.calls {
            expect.extend(c.iter().copied());
        }
    }
    (
        Workload {
            calls,
            writes: r.words[5],
            sum: r.words[4],
            over: r.words[6],
            cycles: r.words[3],
        },
        expect,
    )
}

fn example(name: &str) -> Vec<u8> {
    gt2_ref::examples()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{name}"))
        .1
}

fn check_workload(build: &std::path::Path, name: &str) -> f64 {
    let (w, expect) = play(build, &example(name));
    let expect_sum = expect
        .iter()
        .fold(0u32, |s, &(_, v)| s.wrapping_add(u32::from(v)));
    assert_eq!(w.writes as usize, expect.len(), "{name}: writes");
    assert_eq!(w.sum, expect_sum, "{name}: the written values' sum");
    assert_eq!(w.over, 0, "{name}: a play ran past its frame");
    assert!(w.cycles >= FRAMES * PAL_FRAME_CYCLES);
    let (most, ratio) = w.worst();
    println!(
        "{name}: {:.3} instructions per 6502 cycle ({} instructions over {} busy cycles of {} \
         in 10 s, {} writes); the dearest call {} instructions in {} cycles; the dearest per \
         cycle {ratio:.3}",
        w.per_cycle(),
        w.instructions(),
        w.busy_cycles(),
        w.cycles,
        w.writes,
        most.0,
        most.1
    );
    w.per_cycle()
}

#[test]
fn dormanns_functional_test_reaches_its_success_trap_at_the_pinned_emulator() {
    let Some(build) = build() else { return };
    let image = dormann_image();
    let r = run_with(&build, "sid_bench_6502", 2, |soc| load(soc, &image, &[0]));
    assert_eq!(
        r.words[2], 1,
        "no success: the PC ended at ${:04x}",
        r.words[7]
    );
    assert_eq!(r.words[7], DORMANN_SUCCESS);
    println!(
        "Dormann: success at cycle {}, {} instructions, {:.3} per 6502 cycle",
        r.words[3],
        r.phases[0],
        r.phases[0] as f64 / f64::from(r.words[3])
    );
}

#[test]
fn the_workload_costs_at_most_the_constant_per_cycle() {
    let Some(build) = build() else { return };
    let worst = SONGS
        .iter()
        .map(|n| check_workload(&build, n))
        .fold(0.0, f64::max);
    println!("worst song {worst:.3}, CPU6502_INSTR_PER_CYCLE {CPU6502_INSTR_PER_CYCLE}");
    assert!(
        worst <= CPU6502_INSTR_PER_CYCLE,
        "{worst:.3} instructions per 6502 cycle, over CPU6502_INSTR_PER_CYCLE"
    );
}

#[test]
#[ignore = "the survey that picked SONGS"]
fn every_example_song() {
    let Some(build) = build() else { return };
    for (name, _) in gt2_ref::examples() {
        check_workload(&build, &name);
    }
}

#[test]
fn each_opcode_alone_and_the_io_page() {
    let Some(build) = build() else { return };
    let r = run_with(&build, "sid_bench_6502", 4 * 259, |soc| {
        load(soc, &[], &[2])
    });
    assert_eq!(r.words[2], 1);
    // Per opcode: the run that executes nothing (the call's own cost, with the dispatch's
    // loop test), the run that executes one instruction.
    let call = (0..259).map(|k| r.phases[4 * k]).collect::<Vec<_>>();
    let one = (0..259)
        .map(|k| (r.phases[4 * k + 2] - call[k], r.samples[k] as u32))
        .collect::<Vec<_>>();
    let overhead = call[0xEA];
    assert!(call.iter().all(|&c| c == overhead), "the empty call varies");
    let nop = one[0xEA];
    println!(
        "a call that runs nothing: {overhead} instructions; NOP ($EA, {} cycles): {} instructions \
         more, which is the dispatch's fetch, table lookup, jump and the loop test",
        nop.1, nop.0
    );
    const JAM: [usize; 12] = [
        0x02, 0x12, 0x22, 0x32, 0x42, 0x52, 0x62, 0x72, 0x92, 0xB2, 0xD2, 0xF2,
    ];
    let mut by_cycles: std::collections::BTreeMap<u32, Vec<(f64, u64, usize)>> = Default::default();
    for (op, &(i, c)) in one.iter().enumerate().take(256) {
        if JAM.contains(&op) {
            continue;
        }
        assert!(
            c >= 2 && i > 0,
            "opcode {op:02x}: {i} instructions, {c} cycles"
        );
        by_cycles
            .entry(c)
            .or_default()
            .push((i as f64 / f64::from(c), i, op));
    }
    for (c, v) in &by_cycles {
        let mean = v.iter().map(|x| x.0).sum::<f64>() / v.len() as f64;
        let lo = v.iter().min_by_key(|x| x.1).unwrap();
        let hi = v.iter().max_by_key(|x| x.1).unwrap();
        println!(
            "{c}-cycle opcodes ({} of them): {mean:.2} instructions per cycle on average; fewest \
             {} (${:02X}), most {} (${:02X})",
            v.len(),
            lo.1,
            lo.2,
            hi.1,
            hi.2
        );
    }
    let name = ["LDA $D400", "STA $D400", "INC $D400"];
    for k in 0..3 {
        let (i, c) = one[256 + k];
        println!(
            "{} through the hooks: {i} instructions in {c} cycles ({:.2} per cycle)",
            name[k],
            i as f64 / f64::from(c)
        );
    }
    let ram = |op: usize| one[op].0;
    // The out-of-line hook costs more than the inline memory access of the same opcode.
    assert!(one[256].0 > ram(0xAD) && one[257].0 > ram(0x8D) && one[258].0 > ram(0xEE));
}

/// The voice shadow (M3 Task 5b, core/sid/sid_shadow.h) on the device build: the instructions a
/// tune's SID writes and a frame's catch-up cost the main loop. Each SONGS player runs 10 s
/// twice, without the shadow and with it (bench mode 3: every write also into a 6581's shadow at
/// its cycle, and the shadow clocked to each frame's end as the look-ahead's idle slices will);
/// the difference over the calls, plus the catch-ups, is the shadow's. Reported per frame and as
/// a share of the 240 MHz core at 50 frames a second.
fn shadow_cost(build: &std::path::Path, name: &str) -> (f64, u64) {
    let o = Options::default();
    let packed = pack(&example(name), &o).expect("pack");
    let setup = Setup::of(&packed, &o);
    let mut image = vec![0u8; 0x10000];
    image[packed.load as usize..packed.load as usize + packed.code.len()]
        .copy_from_slice(&packed.code);
    let params = |mode| {
        [
            mode,
            u32::from(setup.init),
            u32::from(setup.play),
            0,
            FRAMES,
            PAL_FRAME_CYCLES,
        ]
    };
    let plain = run_with(build, "sid_bench_6502", 2 * (1 + FRAMES as usize), |soc| {
        load(soc, &image, &params(1))
    });
    let shadowed = run_with(build, "sid_bench_6502", 2 + 4 * FRAMES as usize, |soc| {
        load(soc, &image, &params(3))
    });
    assert_eq!(plain.words[2], 1);
    assert_eq!(shadowed.words[2], 1);
    assert_eq!(plain.words[5], shadowed.words[5], "{name}: the same writes");
    let mut per_frame = Vec::with_capacity(FRAMES as usize);
    for f in 0..FRAMES as usize {
        let call = shadowed.phases[2 + 4 * f] - plain.phases[2 * (f + 1)];
        let catch_up = shadowed.phases[4 + 4 * f];
        per_frame.push(call + catch_up);
    }
    let writes = (shadowed.phases[0] - plain.phases[0]) as f64;
    let mean = per_frame.iter().sum::<u64>() as f64 / f64::from(FRAMES);
    let worst = *per_frame.iter().max().unwrap();
    println!(
        "{name}: the shadow costs {mean:.0} instructions a frame on average ({:.2} % of the core \
         at 50 frames a second), {worst} in the dearest frame; init's {writes:.0} more",
        mean * 50.0 / CPU_HZ as f64 * 100.0,
    );
    (mean, worst)
}

/// The shadow's dearest catch-up: every voice on a combined waveform with noise (the voices run
/// cycle by cycle while the register shifts towards its lock-up: the chip's worst block,
/// decision record 0004), clocked in slices of the look-ahead's PSID_SLICE_CYCLES.
fn shadow_slices(build: &std::path::Path, model: u32, control: u32, others: u32, freq: u32) -> u64 {
    const SLICES: usize = 40;
    let r = run_with(build, "sid_bench_6502", 2 * SLICES, |soc| {
        load(
            soc,
            &[],
            &[
                4,
                model,
                control,
                freq,
                others,
                SLICES as u32,
                PSID_SLICE_CYCLES,
            ],
        )
    });
    assert_eq!(r.words[2], 1);
    let slices: Vec<u64> = (0..SLICES).map(|k| r.phases[2 * k]).collect();
    let worst = *slices.iter().max().unwrap();
    println!(
        "model {model}, voice 3 {control:#04x}, voices 1-2 {others:#04x}, freq {freq:#06x}: the \
         dearest {PSID_SLICE_CYCLES}-cycle slice {worst} instructions ({:.2} a cycle), the \
         first {}, the last {}",
        worst as f64 / f64::from(PSID_SLICE_CYCLES),
        slices[0],
        slices[SLICES - 1]
    );
    worst
}

/// The look-ahead's slice (docs/plans/2026-10-19-m3-sid-files.md, Task 8 confirms it).
const PSID_SLICE_CYCLES: u32 = 1_500;

#[test]
fn the_voice_shadow_costs_the_main_loop_little() {
    let Some(build) = build() else { return };
    let mut worst_mean = 0f64;
    for name in SONGS {
        let (mean, _) = shadow_cost(&build, name);
        worst_mean = worst_mean.max(mean);
    }
    let mut worst_slice = 0;
    for (model, control, others, freq) in [
        (0, 0x91, 0x91, 0xffff),
        (0, 0xb1, 0x91, 0xffff),
        (0, 0xd1, 0xd1, 0x0800),
        (1, 0x91, 0x91, 0xffff),
        (0, 0x21, 0x21, 0xffff),
    ] {
        worst_slice = worst_slice.max(shadow_slices(&build, model, control, others, freq));
    }
    println!(
        "the shadow: {worst_mean:.0} instructions a frame at most on average \
         (SHADOW_FRAME_MEAN {SHADOW_FRAME_MEAN}), the dearest slice {worst_slice} \
         (SHADOW_SLICE_WORST {SHADOW_SLICE_WORST})"
    );
    assert!(worst_mean <= SHADOW_FRAME_MEAN);
    assert!(worst_slice <= SHADOW_SLICE_WORST);
}
