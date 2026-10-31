// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use common::*;
use fm1_core::Bus;
use gt2_ref::trace::{run, Setup};
use gt2_ref::{pack, Options};

const PARAM_ADDR: u32 = 0x01CB_0000;
const MEM_ADDR: u32 = 0x01CC_0000;
const PAL_FRAME_CYCLES: u32 = 19_656;
const FRAMES: u32 = 501;
const DORMANN_SUCCESS: u32 = 0x3469;
const NOP: usize = 0xEA;
const LDA_ABS: usize = 0xAD;
const STA_ABS: usize = 0x8D;
const INC_ABS: usize = 0xEE;
const HOOK_ACCESSES: [&str; 3] = ["LDA $D400", "STA $D400", "INC $D400"];
const OPCODE_RUNS: usize = 256 + HOOK_ACCESSES.len();
const PHASES_PER_RUN: usize = 4;

const SONGS: [&str; 3] = ["wavecmdtest.sng", "dojo.sng", "funktest.sng"];

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

struct Workload {
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
    let Some(build) = build_or_skip("the 6502 on fm1-emu") else {
        return;
    };
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
    let Some(build) = build_or_skip("the 6502 on fm1-emu") else {
        return;
    };
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
    let Some(build) = build_or_skip("the 6502 on fm1-emu") else {
        return;
    };
    for (name, _) in gt2_ref::examples() {
        check_workload(&build, &name);
    }
}

#[test]
fn each_opcode_alone_and_the_io_page() {
    let Some(build) = build_or_skip("the 6502 on fm1-emu") else {
        return;
    };
    let r = run_with(
        &build,
        "sid_bench_6502",
        PHASES_PER_RUN * OPCODE_RUNS,
        |soc| load(soc, &[], &[2]),
    );
    assert_eq!(r.words[2], 1);
    let empty_call = (0..OPCODE_RUNS)
        .map(|k| r.phases[PHASES_PER_RUN * k])
        .collect::<Vec<_>>();
    let one = (0..OPCODE_RUNS)
        .map(|k| {
            (
                r.phases[PHASES_PER_RUN * k + 2] - empty_call[k],
                r.samples[k] as u32,
            )
        })
        .collect::<Vec<_>>();
    let overhead = empty_call[NOP];
    assert!(
        empty_call.iter().all(|&c| c == overhead),
        "the empty call varies"
    );
    let nop = one[NOP];
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
    for (k, name) in HOOK_ACCESSES.iter().enumerate() {
        let (i, c) = one[256 + k];
        println!(
            "{name} through the hooks: {i} instructions in {c} cycles ({:.2} per cycle)",
            i as f64 / f64::from(c)
        );
    }
    let ram = |op: usize| one[op].0;
    let hooked = |k: usize| one[256 + k].0;
    assert!(
        hooked(0) > ram(LDA_ABS) && hooked(1) > ram(STA_ABS) && hooked(2) > ram(INC_ABS),
        "an access through the out-of-line hooks costs no more than the inline memory access"
    );
}

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

const PSID_SLICE_CYCLES: u32 = 1_500;

#[test]
fn the_voice_shadow_costs_the_main_loop_little() {
    let Some(build) = build_or_skip("the 6502 on fm1-emu") else {
        return;
    };
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

fn look_ahead(build: &std::path::Path, file: &[u8], passes: usize, model: u32) -> Vec<(u64, u32)> {
    const FILE_ADDR: u32 = PARAM_ADDR + 0x100;
    assert!(FILE_ADDR as usize + file.len() <= MEM_ADDR as usize);
    let r = run_with(build, "sid_bench_6502", 2 * passes, |soc| {
        for (i, &b) in file.iter().enumerate() {
            soc.write8(FILE_ADDR + i as u32, b);
        }
        load(soc, &[], &[5, file.len() as u32, passes as u32, model]);
    });
    assert_eq!(r.words[2], 1, "the tune did not start");
    (0..passes)
        .map(|k| (r.phases[2 * k], r.samples[k] as u16 as u32))
        .collect()
}

#[test]
fn a_look_ahead_slice_holds_its_budget() {
    let Some(build) = build_or_skip("the 6502 on fm1-emu") else {
        return;
    };
    let song = sidou_tests::psid::packed(&example("wavecmdtest.sng"), 0xD400);
    let runs: Vec<(&str, Vec<u8>, u32)> = vec![
        (
            "8580, noise and triangle, a write every 4 cycles",
            worst_tune(false, 0x91, "sta", 0x0f),
            1,
        ),
        (
            "6581, noise and triangle, a write every 4 cycles",
            worst_tune(false, 0x91, "sta", 0x0f),
            0,
        ),
        (
            "8580, noise and triangle, a volume write every 4 cycles",
            worst_tune(false, 0x91, "sta", 0x18),
            1,
        ),
        (
            "8580, noise and triangle, an OSC3 read every 4 cycles",
            worst_tune(false, 0x91, "lda", 0x1b),
            1,
        ),
        (
            "8580, saw, a write every 4 cycles",
            worst_tune(false, 0x21, "sta", 0x0f),
            1,
        ),
        (
            "8580, noise and triangle, no accesses",
            worst_tune(false, 0x91, "", 0),
            1,
        ),
        (
            "8580, 2SID, noise and triangle, a write every 4 cycles",
            worst_tune(true, 0x91, "sta", 0x0f),
            1,
        ),
        (
            "8580, 2SID, noise and triangle, an OSC3 read every 4 cycles",
            worst_tune(true, 0x91, "lda", 0x1b),
            1,
        ),
        (
            "8580, 2SID, noise and triangle, no accesses",
            worst_tune(true, 0x91, "", 0),
            1,
        ),
        ("6581, wavecmdtest", song, 0),
    ];
    let mut worst = 0;
    for (name, file, model) in &runs {
        let passes = look_ahead(&build, file, 300, *model);
        assert!(
            passes.iter().all(|p| p.1 != 0xffff),
            "{name}: a pass ran no slice"
        );
        let most = *passes.iter().max_by_key(|p| p.0).unwrap();
        let cycles = passes.iter().map(|p| u64::from(p.1)).sum::<u64>();
        let total = passes.iter().map(|p| p.0).sum::<u64>();
        println!(
            "{name}: the dearest pass {} instructions ({} cycles, pass {}); {:.1} a 6502 cycle \
             on average; the first passes {:?}",
            most.0,
            most.1,
            passes.iter().position(|p| *p == most).unwrap(),
            total as f64 / cycles as f64,
            &passes[..6]
        );
        worst = worst.max(most.0);
    }
    println!(
        "the dearest look-ahead pass {worst} instructions (PSID_SLICE_WORST {PSID_SLICE_WORST})"
    );
    assert!(worst <= PSID_SLICE_WORST);
}
