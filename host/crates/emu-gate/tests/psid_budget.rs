// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;
mod flash_plan;

use common::*;
use flash_plan::{IMAGE_CEILING, PLAN};
use fm1_core::Bus;
use sidou_sys::*;
use sidou_tests::psid::{packed, songs};
use sidou_tests::psid_fixtures::{exact, Psid, INIT};

const PARAM_ADDR: u32 = 0x01CB_0000;
const FILE_ADDR: u32 = PARAM_ADDR + 0x100;
const MEM_ADDR: u32 = 0x01CC_0000;
const PASSES_BEFORE_A_BLOCK: u32 = 40;
const WRITTEN_REGISTERS: [(&str, u32); 3] = [
    ("voice 2's", 0x0f),
    ("the cutoff", 0x16),
    ("the volume", 0x18),
];
const SID_VOLUME: u32 = 0x18;
const M3_RAMTEXT: u64 = 1_536;

struct Blocks {
    blocks: Vec<(u64, u64)>,
    entries: Vec<u32>,
    writes: u32,
    late: u32,
    splits: u32,
}

fn chip_work(name: &str) -> bool {
    matches!(
        name,
        "render_6581"
            | "render_8580"
            | "render_6581_bleed"
            | "render_8580_bleed"
            | "clock_voices"
            | "fold_owed"
            | "grit"
    ) || ((name.starts_with("sid_osc_")
        || name.starts_with("sid_env_")
        || name.starts_with("sid_noise_"))
        && !name.contains("settle"))
}

fn bench_blocks(build: &std::path::Path, file: &[u8], blocks: usize, model: u32) -> Blocks {
    use fm1_core::Cpu;
    use object::{Object, ObjectSymbol, SymbolKind};
    assert!(FILE_ADDR as usize + file.len() <= MEM_ADDR as usize);
    let dir = build.join("bench/fm1");
    let elf = std::fs::read(dir.join("sid_bench_6502.elf")).expect("the 6502 bench's ELF");
    let bin = std::fs::read(dir.join("sid_bench_6502.bin")).expect("the 6502 bench's image");
    let obj = object::File::parse(&*elf).expect("parse the bench ELF");
    let mark = obj
        .symbols()
        .find(|s| s.name() == Ok("bench_mark"))
        .expect("bench_mark")
        .address() as u32;
    let base = u64::from(fm1_soc::XIP_BASE);
    let mut work = vec![false; bin.len()];
    for sym in obj
        .symbols()
        .filter(|s| s.kind() == SymbolKind::Text && s.size() > 0)
    {
        if sym.name().is_ok_and(chip_work) {
            let from = (sym.address() - base) as usize;
            let to = (from + sym.size() as usize).min(work.len());
            work[from..to].fill(true);
        }
    }
    let mut soc = fm1_soc::Soc::new(1 << 20, bin);
    for (i, &b) in file.iter().enumerate() {
        soc.write8(FILE_ADDR + i as u32, b);
    }
    for (i, v) in [
        6,
        file.len() as u32,
        blocks as u32,
        model,
        PASSES_BEFORE_A_BLOCK,
    ]
    .into_iter()
    .enumerate()
    {
        soc.write32(PARAM_ADDR + 4 * i as u32, v);
    }
    let mut cpu = Cpu::new(fm1_soc::XIP_BASE);
    let (mut marks, mut total, mut path, mut steps) = (0usize, 0u64, 0u64, 0u64);
    let mut out = Vec::with_capacity(blocks);
    const RESULT: u32 = 0x01C8_0000;
    while soc.read32(RESULT) != 0x5349_4442 {
        if cpu.pc == mark {
            marks += 1;
            if marks % 2 == 0 {
                out.push((total, path));
                total = 0;
                path = 0;
            }
        }
        if marks % 2 == 1 {
            total += 1;
            let at = cpu.pc.wrapping_sub(fm1_soc::XIP_BASE) as usize;
            path += u64::from(!work.get(at).copied().unwrap_or(false));
        }
        step(&mut cpu, &mut soc);
        steps += 1;
        assert!(steps < 4_000_000_000, "the bench did not finish");
    }
    assert_eq!(marks, 2 * blocks, "the bench's marks");
    assert_eq!(soc.read32(RESULT + 8), 1, "the tune did not start");
    let mut sample = |i: u32| u32::from(soc.read16(RESULT + 0x40 + 2 * i));
    let entries = (0..blocks as u32).map(|i| sample(2 + i)).collect();
    let writes = sample(0) | sample(1) << 16;
    Blocks {
        blocks: out,
        entries,
        writes,
        late: soc.read32(RESULT + 24),
        splits: soc.read32(RESULT + 28),
    }
}

fn one_write_a_sample_tune(reg: u32) -> Vec<u8> {
    let mut init = String::new();
    for v in 0..3 {
        let r = 0xD400 + 7 * v;
        for (off, value) in [
            (0, 0xff),
            (1, 0xff),
            (2, 0),
            (3, 8),
            (5, 0),
            (6, 0xf0),
            (4, 0x91),
        ] {
            init += &format!("lda #${value:02x}\nsta ${:04x}\n", r + off);
        }
    }
    init += "lda #$0f\nsta $d418\nloop:\n";
    for k in 0..120u32 {
        let value = (k * 37 + 5) as u8;
        let value = if reg == SID_VOLUME {
            0x10 | (value & 0x0f)
        } else {
            value
        };
        init += &format!(
            "lda #${value:02x}\nsta ${:04x}\n{}",
            0xD400 + reg,
            exact(18)
        );
    }
    init += "jmp loop\n";
    Psid::new(&[(INIT, &init), (0x8000, "rts\n")]).bytes()
}

fn worst(b: &Blocks) -> (f64, usize) {
    b.blocks
        .iter()
        .enumerate()
        .map(|(i, &(_, path))| (path as f64 / HALF_FRAMES as f64, i))
        .fold((f64::MIN, 0), |a, x| if x.0 > a.0 { x } else { a })
}

#[test]
fn the_interrupts_worst_tune_block_holds_psid_isr_worst() {
    let Some(build) = build_or_skip("the .sid budgets") else {
        return;
    };
    let mut runs: Vec<(String, Vec<u8>, u32, usize)> = Vec::new();
    for (name, sng) in songs() {
        runs.push((name.clone(), packed(&sng, 0xD400), 0, 690));
    }
    for (name, sng) in songs().into_iter().take(3) {
        runs.push((format!("{name} (2SID)"), packed(&sng, 0xD420), 0, 690));
    }
    for (what, reg) in WRITTEN_REGISTERS {
        for model in [0u32, 1] {
            runs.push((
                format!(
                    "{}, a write to {what} every 4 cycles",
                    ["6581", "8580"][model as usize]
                ),
                worst_tune(false, 0x91, "sta", reg),
                model,
                300,
            ));
        }
        runs.push((
            format!("8580, 2SID, a write to {what} every 4 cycles"),
            worst_tune(true, 0x91, "sta", reg),
            1,
            300,
        ));
    }
    for (what, reg) in WRITTEN_REGISTERS {
        for model in [0u32, 1] {
            runs.push((
                format!(
                    "{}, a write to {what} every sample",
                    ["6581", "8580"][model as usize]
                ),
                one_write_a_sample_tune(reg),
                model,
                300,
            ));
        }
    }
    if let Some(only) = std::env::var_os("SIDOU_PSID_RUNS") {
        runs.retain(|r| r.0.contains(only.to_str().unwrap()));
    }
    let (mut dearest, mut which) = (0.0f64, String::new());
    for (name, file, model, blocks) in &runs {
        let b = bench_blocks(&build, file, *blocks, *model);
        let (w, at) = worst(&b);
        let mean = b.blocks.iter().map(|&(_, p)| p as f64).sum::<f64>()
            / b.blocks.len() as f64
            / HALF_FRAMES as f64;
        let most = b.entries.iter().copied().max().unwrap_or(0);
        println!(
            "{name}: the .sid path's worst block {w:.1} instructions a sample (block {at}: {} of \
             its {}), mean {mean:.1}; {} writes, at most {most} entries a block, {} late, at \
             most {} renders a chip's block",
            b.blocks[at].1, b.blocks[at].0, b.writes, b.late, b.splits
        );
        assert!(b.writes > 0, "{name}: no writes");
        if !name.contains("every 4 cycles") && !name.contains("every sample") {
            assert_eq!(b.late, 0, "{name}: writes late with the main loop ahead");
        }
        if w > dearest {
            dearest = w;
            which = name.clone();
        }
    }
    println!(
        "the .sid path's dearest block: {dearest:.1} instructions a sample ({which}); \
         PSID_ISR_WORST {PSID_ISR_WORST}"
    );
    assert!(dearest <= PSID_ISR_WORST, "{which}: {dearest:.1} a sample");
}

#[test]
fn a_playing_tune_holds_the_main_loops_budgets() {
    let Some(build) = build_or_skip("the .sid budgets") else {
        return;
    };
    let fw = Firmware::load(&build);
    let example = |n: &str| {
        gt2_ref::examples()
            .into_iter()
            .find(|(k, _)| k == n)
            .unwrap()
            .1
    };
    let runs: Vec<(&str, Vec<u8>)> = vec![
        ("wavecmdtest", packed(&example("wavecmdtest.sng"), 0xD400)),
        ("dojo, 2SID", packed(&example("dojo.sng"), 0xD420)),
        (
            "8580, noise and triangle, a write every 4 cycles",
            worst_tune(false, 0x91, "sta", 0x0f),
        ),
        (
            "2SID, noise and triangle, an OSC3 read every 4 cycles",
            worst_tune(true, 0x91, "lda", 0x1b),
        ),
    ];
    let (mut slice_worst, mut pass_worst) = (0u64, 0u64);
    for (name, file) in &runs {
        let mut flash = settled_flash(&fw.image);
        put_file(&mut flash, "TUNE", STORE_FILE_SID, file);
        let mut m = Machine::boot_blank(&fw.image);
        m.soc.flash.mem.copy_from_slice(&flash);
        run_past_boot(&mut m, &fw);
        m.watch_calls(fw.symbol("psid_ctl_poll"));
        m.watch_passes(fw.symbol("midi_drain"), fw.symbol("midi_service"));
        ui_play_tune(&mut m, &fw, 0);
        ui_back_to(&mut m, &fw, "live_view_view");
        assert!(tap_on(&mut m, &fw), "LIVE is not showing");
        m.run_ms(4000);
        let writes = m
            .soc
            .read32(fw.symbol("player_isr") + ilp32::psid_ctl::writes as u32);
        let slices = m
            .soc
            .read32(fw.symbol("player_isr") + ilp32::psid_ctl::slices as u32);
        let slice = m.watched_calls.iter().copied().max().unwrap_or(0);
        let pass = m.passes.iter().copied().max().unwrap_or(0);
        let block = m.alnk_blocks.iter().copied().max().unwrap_or(0) as f64 / HALF_FRAMES as f64;
        println!(
            "{name}: {slices} slices, {writes} writes; the dearest psid_ctl_poll {slice}, the \
             busiest pass {pass} ({} passes); the dearest audio block {block:.1} a sample",
            m.passes.len()
        );
        assert!(
            block <= AUDIO_TARGET,
            "{name}: an audio block of {block:.1} a sample"
        );
        assert!(slices > 100 && writes > 0, "{name}: the tune did not play");
        slice_worst = slice_worst.max(slice);
        pass_worst = pass_worst.max(pass);
    }
    println!(
        "a tune playing: the dearest slice {slice_worst} (PSID_SLICE_WORST {PSID_SLICE_WORST}), \
         the busiest pass {pass_worst} (POLL_INSTRUCTIONS {POLL_INSTRUCTIONS})"
    );
    assert!(slice_worst <= PSID_SLICE_WORST);
    assert!(pass_worst <= POLL_INSTRUCTIONS);
}

#[test]
fn a_tunes_memory_fits_m3s_budgets() {
    use object::{Object, ObjectSection, ObjectSymbol};
    let Some(build) = build_or_skip("the .sid budgets") else {
        return;
    };
    let fw = Firmware::load(&build);
    let file = object::File::parse(&*fw.elf).unwrap();
    let size = |name: &str| file.section_by_name(name).map_or(0, |s| s.size());
    let sym = |name: &str| {
        file.symbols()
            .find(|s| s.name() == Ok(name))
            .unwrap_or_else(|| panic!("no {name}"))
    };
    let union = sym("player_isr").size();
    assert_eq!(
        union,
        (ilp32::song_ctl::SIZE + 2 * ilp32::gt2_shape::SIZE) as u64,
        "the tune's state is larger than the song's"
    );
    assert!(
        (ilp32::psid_ctl::SIZE + ilp32::psid_work::SIZE + ilp32::psid_header::SIZE) as u64 <= union
    );
    let region = fw.symbol("__song_region_end") - fw.symbol("__song_region");
    assert_eq!(
        region, 0x10000,
        "the 6502's memory is the song's 64 KiB region"
    );
    const RAM_START: u32 = 0x01C0_E000;
    const POOL_START: u32 = 0x01C1_E000;
    const POOL_BYTES: u64 = 0x59F00;
    let ram_free =
        u64::from(POOL_START - RAM_START) - size(".data") - size(".bss") - size(".song_ram");
    let pool_free = POOL_BYTES - size(".pool");
    let m3: Vec<(String, u64)> = file
        .symbols()
        .filter(|s| {
            s.address() >= u64::from(fw.symbol("_rt_start"))
                && s.address() < u64::from(fw.symbol("_rt_end"))
                && s.name().is_ok_and(|n| {
                    n.starts_with("psid_")
                        || n == "sid_advance"
                        || n == "play_a"
                        || n == "render_part"
                })
        })
        .map(|s| (s.name().unwrap().to_string(), s.size()))
        .collect();
    let m3_text: u64 = m3.iter().map(|x| x.1).sum();
    let image = fw.image.len();
    println!(
        "free RAM {} (RAM region {ram_free}, POOL {pool_free}; FREE_RAM_MIN {FREE_RAM_MIN}); \
         the tune's state {} bytes in the song's {union}; RAMTEXT {} bytes, M3's {m3_text} \
         ({m3:?}, of {M3_RAMTEXT}); the image {image} of {PLAN}'s ceiling {IMAGE_CEILING}",
        ram_free + pool_free,
        ilp32::psid_ctl::SIZE + ilp32::psid_work::SIZE + ilp32::psid_header::SIZE,
        size(".ram_text")
    );
    assert!(ram_free >= REGION_FREE_MIN && pool_free >= REGION_FREE_MIN);
    assert!(ram_free + pool_free >= FREE_RAM_MIN);
    assert!(m3_text <= M3_RAMTEXT, "M3's RAMTEXT {m3_text}");
    assert!(image <= IMAGE_CEILING);
}
