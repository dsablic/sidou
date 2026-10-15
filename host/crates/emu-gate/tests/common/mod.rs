// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared by the emulator tests: where the fm1 build is, and fm1-emu workarounds.

#![allow(dead_code)] // each test binary uses its own subset

use std::path::{Path, PathBuf};

use fm1_core::{Bus, Cpu};
use fm1_soc::Soc;
use sidou_tests::signal_metrics::estimate_f0;

/// The fm1 build directory, made absolute, or None to skip the emulator tests.
pub fn build_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("SIDOU_FM1_BUILD").filter(|d| !d.is_empty())?;
    Some(std::path::absolute(&dir).expect("resolve SIDOU_FM1_BUILD"))
}

pub fn skip_message(what: &str) {
    println!(
        "skipping {what}: set SIDOU_FM1_BUILD to the fm1 build directory \
         (e.g. $PWD/../build/fm1 from host/) after building the fm1 preset"
    );
}

pub fn read(path: &Path, what: &str) -> Vec<u8> {
    std::fs::read(path)
        .unwrap_or_else(|e| panic!("read {what} {}: {e}; build the fm1 preset", path.display()))
}

/// fm1-core at the pinned rev decodes the decrement-before block load `{rA, rB, ..} = [-rN]`
/// (`4x eb`, fm1-isa class `r_4_mov_r_4`) but has no semantics for it, and faults. clang
/// emitted it in `sid_cutoff_hz` to load a curve point until M1b Task 5 made that a table
/// lookup; nothing emits it now (the gates report emulated_block_loads). Run it here with
/// the convention fm1-core uses for the `[rN+]` block moves: base in bits 0-3, register mask in bits 16-31, lowest register
/// at the lowest address; `-rN` starts 4 bytes per register below the base and does not write
/// it back (`--rN` is a separate class). The emulator gate's host comparison checks the result.
/// Returns false, doing nothing, for any other instruction.
pub fn step_block_load_dec(cpu: &mut Cpu, soc: &mut Soc) -> bool {
    let raw = soc.read32(cpu.pc);
    if raw & 0x0804_fff0 != 0x0000_eb40 {
        return false;
    }
    assert!(
        cpu.rep_end == 0
            && cpu.pred_skips.is_empty()
            && cpu.pair_pending.is_none()
            && cpu.hold_irq == 0,
        "block load at {:#010x} inside a rep, predicated or paired construct",
        cpu.pc
    );
    let mask = raw >> 16;
    let mut addr = cpu.regs[(raw & 0xf) as usize].wrapping_sub(4 * mask.count_ones());
    for r in (0..16).filter(|r| mask >> r & 1 != 0) {
        cpu.regs[r] = soc.read32(addr);
        addr = addr.wrapping_add(4);
    }
    cpu.pc += 4;
    cpu.insn_count += 1;
    true
}

/// One instruction: the block load above, or fm1-core's step. Panics on a fault.
pub fn step(cpu: &mut Cpu, soc: &mut Soc) -> bool {
    let pc = cpu.pc;
    if step_block_load_dec(cpu, soc) {
        return true;
    }
    cpu.step(soc)
        .unwrap_or_else(|e| panic!("cpu fault at {pc:#010x}: {e:?}"));
    false
}

/// Where the SPL maps app.bin, and enters it.
pub const APP_BASE: u32 = 0x0200_0120;
/// The SPL's boot-parameter block (Felucca's crt0.S: inside the vector table at 0x01C7FE00).
const BOOT_PARAMS: u32 = 0x01C7_FE08;
/// Where fm1-emu's CLI builds the SPL's boot parameters for a package boot (RAM_BASE + 0x7fc00).
const PACKAGE_PARAMS: u32 = 0x01C7_FC00;
const RAM_SIZE: usize = 1 << 20;
const IRQ_ALNK0: u8 = 11;

/// The firmware image (`sidou_app.bin`) and its ELF, from the fm1 build.
pub struct Firmware {
    pub image: Vec<u8>,
    pub elf: Vec<u8>,
}

impl Firmware {
    pub fn load(build: &Path) -> Self {
        let dir = build.join("platform/fm1");
        Firmware {
            image: read(&dir.join("sidou_app.bin"), "firmware image"),
            elf: read(&dir.join("sidou_app.elf"), "firmware ELF"),
        }
    }

    /// The function symbol whose bytes hold `addr`, as `name+offset`.
    pub fn function_at(&self, addr: u32) -> String {
        use object::{Object, ObjectSymbol, SymbolKind};
        let file = object::File::parse(&*self.elf).expect("parse the firmware ELF");
        file.symbols()
            .filter(|s| s.kind() == SymbolKind::Text && s.size() > 0)
            .find(|s| (s.address()..s.address() + s.size()).contains(&u64::from(addr)))
            .map(|s| {
                format!(
                    "{}+{:#x}",
                    s.name().unwrap_or("?"),
                    u64::from(addr) - s.address()
                )
            })
            .unwrap_or_else(|| format!("{addr:#010x}"))
    }

    /// The address of a function in the ELF.
    pub fn symbol(&self, name: &str) -> u32 {
        use object::{Object, ObjectSymbol};
        let file = object::File::parse(&*self.elf).expect("parse the firmware ELF");
        let sym = file
            .symbols()
            .find(|s| s.name() == Ok(name))
            .unwrap_or_else(|| panic!("no symbol {name} in the firmware ELF"));
        sym.address() as u32
    }

    /// The firmware's `chips` (platform/fm1/audio.c), checked against the pi32v2 layout
    /// the tests read it with (sidou_sys::ilp32).
    pub fn chips(&self) -> u32 {
        use object::{Object, ObjectSymbol};
        let file = object::File::parse(&*self.elf).expect("parse the firmware ELF");
        let sym = file
            .symbols()
            .find(|s| s.name() == Ok("chips"))
            .expect("no symbol chips in the firmware ELF");
        assert_eq!(
            sym.size(),
            sidou_sys::ilp32::reg_ring_chips::SIZE as u64,
            "the firmware's reg_ring_chips is not laid out as sidou_sys::ilp32 says"
        );
        sym.address() as u32
    }
}

/// The audio DMA buffer after an ALNK0 interrupt (`Machine::snapshot_buffer`).
pub struct BufferSnapshot {
    /// The cycles at which the interrupt was taken and returned.
    pub entry: u64,
    pub exit: u64,
    pub words: Vec<i16>,
    /// The bytes at `Machine::snapshot_bytes`, in order.
    pub bytes: Vec<u8>,
}

/// A call being measured: where it returns to and the stack pointer at its entry.
struct OpenCall {
    rets: u32,
    sp: u32,
    start: u64,
}

/// The firmware booted in fm1-emu the way its CLI boots a bare app.bin
/// (`fm1-emu --bin <image padded to 0x02000120> --entry 0x02000120`): the SPL maps app.bin at
/// XIP 0x02000120 and enters its first byte in user mode with r0 pointing at a
/// boot-parameter block. No update package and no stock flash are involved.
pub struct Machine {
    pub soc: Soc,
    pub cpu: Cpu,
    pub steps: u64,
    /// Instructions run inside interrupts.
    pub isr_steps: u64,
    /// Instructions run by `step_block_load_dec` instead of fm1-core.
    pub emulated_block_loads: u64,
    /// The instructions of each ALNK0 interrupt (one audio block), nested ones included.
    pub alnk_blocks: Vec<u64>,
    alnk_open: Option<(usize, u64)>,
    /// Calls of the watched function outside interrupts: instructions each, its callees and
    /// nothing from interrupts included.
    pub watched_calls: Vec<u64>,
    /// The cycle at which each of those calls began.
    pub watched_starts: Vec<u64>,
    watch: Option<u32>,
    open_call: Option<OpenCall>,
    /// Instructions run outside interrupts (the main loop).
    pub main_steps: u64,
    /// With `watch_alnk_pcs`: instructions run at the ALNK0 interrupt's own level (not in
    /// interrupts nested in it) inside the watched range, and the distinct PCs outside it.
    pub alnk_pcs_inside: u64,
    pub alnk_pcs_outside: std::collections::BTreeSet<u32>,
    alnk_pc_range: Option<std::ops::Range<u32>>,
    /// The cycle at which each audio half went to the DAC's output (`soc.periph.audio`):
    /// half `h` (samples `h * HALF_FRAMES..`, per channel) finished playing at
    /// `half_played[h]`.
    pub half_played: Vec<u64>,
    /// With `snapshot_buffer`: after each ALNK0 interrupt, its entry and exit cycles and the
    /// DMA buffer's words (both halves, interleaved L/R) as fm1-emu plays them (>> 8 to i16).
    pub buffer_snapshots: Vec<BufferSnapshot>,
    snapshot: Option<(u32, u32)>,
    /// Bytes read into each buffer snapshot as well (e.g. the voices' gates).
    pub snapshot_bytes: Vec<u32>,
    alnk_entry: u64,
}

impl Machine {
    pub fn boot(image: &[u8]) -> Self {
        let mut xip = vec![0u8; (APP_BASE - fm1_soc::XIP_BASE) as usize];
        xip.extend_from_slice(image);
        let mut soc = Soc::new(RAM_SIZE, xip);
        // Placeholder contents (the CLI's): the firmware only saves r0, and fm1_irq_init then
        // overwrites the block with the vector table.
        soc.write32(BOOT_PARAMS, BOOT_PARAMS + 0x40);
        Self::start(soc, APP_BASE, BOOT_PARAMS)
    }

    /// An update package (.fwsc) booted as fm1-emu's `--fwsc` boots one
    /// (fm1_soc::stock_from_package and stock_boot): the XIP window shows the package's app
    /// area unscrambled, the flash chip holds its flash.bin, r0 points at the SPL's boot
    /// parameters built from its flash header.
    pub fn boot_package(package: &[u8]) -> Self {
        let stock = fm1_soc::stock_from_package(package, None).expect("an FM-1 package");
        let mut soc = Soc::new(RAM_SIZE, stock.xip.clone());
        let r0 = fm1_soc::stock_boot(&mut soc, &stock.flash, PACKAGE_PARAMS);
        Self::start(soc, stock.entry, r0)
    }

    fn start(mut soc: Soc, entry: u32, r0: u32) -> Self {
        // The firmware's main loop sleeps in `idle`; skip ahead to the next interrupt.
        soc.periph.skip_idle = true;
        let mut cpu = Cpu::new(entry);
        // The SPL hands cpu0 over in user mode (fm1-emu main.rs).
        cpu.user = true;
        cpu.regs[0] = r0;
        Machine {
            soc,
            cpu,
            steps: 0,
            isr_steps: 0,
            emulated_block_loads: 0,
            alnk_blocks: Vec::new(),
            alnk_open: None,
            watched_calls: Vec::new(),
            watched_starts: Vec::new(),
            watch: None,
            open_call: None,
            main_steps: 0,
            alnk_pcs_inside: 0,
            alnk_pcs_outside: std::collections::BTreeSet::new(),
            alnk_pc_range: None,
            half_played: Vec::new(),
            buffer_snapshots: Vec::new(),
            snapshot: None,
            snapshot_bytes: Vec::new(),
            alnk_entry: 0,
        }
    }

    /// Counts the instructions of every call of the function at `entry` from the main loop.
    pub fn watch_calls(&mut self, entry: u32) {
        self.watch = Some(entry);
    }

    /// Sorts every instruction the ALNK0 interrupt runs at its own level by whether its PC
    /// is in `range`.
    pub fn watch_alnk_pcs(&mut self, range: std::ops::Range<u32>) {
        self.alnk_pc_range = Some(range);
    }

    pub fn ms(&self) -> u64 {
        self.soc.periph.ms()
    }

    /// Snapshots `words` 32-bit words at `addr` (the audio DMA buffer) after every ALNK0
    /// interrupt into `buffer_snapshots`; None stops.
    pub fn snapshot_buffer(&mut self, buffer: Option<(u32, u32)>) {
        self.snapshot = buffer;
    }

    /// Runs `ms` milliseconds of emulated time.
    pub fn run_ms(&mut self, ms: u64) {
        let n = self.soc.periph.cycle_of_ms(ms);
        self.run_cycles(n);
    }

    /// Runs `n` CPU cycles of emulated time (fm1-emu: one instruction a cycle).
    pub fn run_cycles(&mut self, n: u64) {
        let end = self.soc.periph.cycles + n;
        // fm1_soc::machine::run_blocks, one instruction at a time so `step` can stand in for
        // the instruction fm1-core lacks.
        while self.soc.periph.cycles < end {
            let soc = &mut self.soc;
            let cpu = &mut self.cpu;
            if soc.periph.tick() {
                let halves = soc.periph.audio_halves;
                soc.advance(0);
                if soc.periph.audio_halves != halves {
                    self.half_played.push(soc.periph.cycles);
                }
            }
            soc.periph.in_isr = !cpu.irq_levels.is_empty();
            if soc.periph.any_pending() {
                if let Some((irq, prio)) = soc.pending_irq() {
                    if cpu.irq_ready(prio) {
                        let handler = soc.read32(fm1_soc::VEC_BASE + 4 * u32::from(irq));
                        if irq == IRQ_ALNK0 && self.alnk_open.is_none() {
                            self.alnk_open = Some((cpu.irq_levels.len(), self.steps));
                            self.alnk_entry = soc.periph.cycles;
                        }
                        cpu.interrupt(handler, prio);
                    }
                }
            }
            let in_isr = !cpu.irq_levels.is_empty();
            if let (Some(range), Some((depth, _))) = (&self.alnk_pc_range, self.alnk_open) {
                if cpu.irq_levels.len() == depth + 1 {
                    if range.contains(&cpu.pc) {
                        self.alnk_pcs_inside += 1;
                    } else {
                        self.alnk_pcs_outside.insert(cpu.pc);
                    }
                }
            }
            if in_isr {
                self.isr_steps += 1;
            } else {
                if let Some(entry) = self.watch {
                    if cpu.pc == entry && self.open_call.is_none() {
                        self.watched_starts.push(soc.periph.cycles);
                        self.open_call = Some(OpenCall {
                            rets: cpu.rets,
                            sp: cpu.sp,
                            start: self.main_steps,
                        });
                    }
                }
                if let Some(c) = &self.open_call {
                    if cpu.pc == c.rets && cpu.sp >= c.sp {
                        self.watched_calls.push(self.main_steps - c.start);
                        self.open_call = None;
                    }
                }
                self.main_steps += 1;
            }
            if step(cpu, soc) {
                self.emulated_block_loads += 1;
            }
            self.steps += 1;
            if let Some((depth, start)) = self.alnk_open {
                if self.cpu.irq_levels.len() <= depth {
                    self.alnk_blocks.push(self.steps - start);
                    self.alnk_open = None;
                    if let Some((addr, words)) = self.snapshot {
                        let w = (0..words)
                            .map(|i| (self.soc.read32(addr + 4 * i) as i32 >> 8) as i16)
                            .collect();
                        self.buffer_snapshots.push(BufferSnapshot {
                            entry: self.alnk_entry,
                            exit: self.soc.periph.cycles,
                            words: w,
                            bytes: self
                                .snapshot_bytes
                                .iter()
                                .map(|&a| self.soc.read8(a))
                                .collect(),
                        });
                    }
                }
            }
        }
    }

    /// Key or button `id` (Felucca's ids: buttons 0..13, note keys 14..40) down or up.
    pub fn key(&mut self, id: u8, down: bool) {
        self.soc.periph.apply(&fm1_soc::Action::Key { id, down });
    }

    /// Turns encoder `enc` (K1..K4 = 2..5) by `clicks` detents, + = clockwise.
    pub fn turn(&mut self, enc: u8, clicks: i32) {
        self.soc.periph.apply(&fm1_soc::Action::Enc { enc, clicks });
    }

    /// The MASTER pot, 0..1023.
    pub fn pot(&mut self, value: u16) {
        self.soc
            .periph
            .apply(&fm1_soc::Action::Adc { ch: 4, value });
    }

    /// Bytes into the TRS MIDI IN jack; fm1-emu delivers one per 31250-baud byte time.
    pub fn midi(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.soc.periph.apply(&fm1_soc::Action::Midi { b });
        }
    }

    /// The left channel of everything played so far (mono unless an effect slot makes
    /// stereo).
    pub fn left(&self) -> Vec<i16> {
        self.soc.periph.audio.iter().step_by(2).copied().collect()
    }

    /// The right channel of everything played so far.
    pub fn right(&self) -> Vec<i16> {
        self.soc
            .periph
            .audio
            .iter()
            .skip(1)
            .step_by(2)
            .copied()
            .collect()
    }
}

/// The SID's most expensive scenario per chip, on average, in instructions per sample: the
/// SID gate's max_instructions_per_sample (sid_budget.rs fails above it). M1b measures 526.8
/// (two chips, gate_every_block on the 6581) and 522.8 (one chip). The effect gates no longer
/// pair it with the slots (they take the worst block, below); it stays as the SID's guard.
pub const SID_WORST_PER_CHIP: f64 = 530.0;

/// The SID's most expensive block (128 samples) in instructions per sample, for one chip
/// and for two together, as sid_budget.rs measures it (it fails above these). The audio
/// ISR has a hard per-block deadline, so the effect gates pair the slots with this, not
/// with a scenario's average. Both are the first block of a combined-noise scenario from
/// power-on (noise_tri_ring_0800 on the 8580: 690.83 and 1,409.20 at M1b), while the
/// register runs through its shifts until it locks up (decision record 0004, "Worst
/// blocks").
pub const SID_WORST_BLOCK_ONE_CHIP: f64 = 691.0;
pub const SID_WORST_BLOCK_TWO_CHIPS: f64 = 1410.0;

/// The audio target, instructions per sample for everything the audio interrupt does (design
/// doc section 8): slots_budget.rs splits it, latency.rs holds the render allowance to it.
pub const AUDIO_TARGET: f64 = 2700.0;
/// The audio ISR's own overhead around the chips and the slots (0004's estimate).
pub const ISR_OVERHEAD: f64 = 100.0;
/// Two chips plus the effects (M1c plan Task 1's cap).
pub const TWO_CHIPS_PLUS_EFFECTS: f64 = 2300.0;

/// 24 MHz / 544, the I2S frame rate.
pub const FS: f64 = 24e6 / 544.0;

/// platform/fm1/audio.h AUDIO_SAMPLE_RATE and AUDIO_HALF_FRAMES.
pub const SAMPLE_RATE: u32 = 44_118;
pub const HALF_FRAMES: usize = 128;
/// fm1-emu's MASTER pot reading at power-on.
pub const POT: u16 = 900;
/// platform/fm1/audio.c SLOTS_SEED.
pub const SLOTS_SEED: u32 = 0x51D0;

/// The firmware's instrument at boot (platform/fm1/main.c: live_init, then the MASTER pot),
/// rendered by the host build in blocks as audio_fill does (the chip, then the effect
/// slots). Before block `note_block` the note `note` goes on; each `(block, cc, value)` of
/// `ccs` goes to live before its block (as the device's drain applies a pass's entries at a
/// block start), in order. Interleaved L, R.
pub fn render_host(
    frames: usize,
    note: u8,
    note_block: usize,
    ccs: &[(usize, u8, u8)],
) -> Vec<i16> {
    use sidou_sys::*;
    let mut chip = Box::<sid>::default();
    let mut pool = Box::<sid_slot_pool>::default();
    let mut slots = Box::<sid_slots>::default();
    let mut fx = Box::<fx_live>::default();
    let mut target = Box::new(fx_live_target {
        sid: live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: SAMPLE_RATE,
        },
        fx: &mut *fx,
    });
    let mut l = Box::<live>::default();
    let t: *mut fx_live_target = &mut *target;
    let mut out = vec![0i16; 2 * frames];
    let mut mono = vec![0i16; HALF_FRAMES];
    // SAFETY: everything C keeps a pointer to is boxed and outlives the calls; the buffers
    // hold the samples asked for.
    unsafe {
        sid_slots_init(&mut *slots, SAMPLE_RATE, SLOTS_SEED, &mut *pool);
        fx_live_init(&mut *fx, &mut *slots);
        live_init(
            &mut *l,
            Some(fx_live_sid_sink),
            t.cast::<std::ffi::c_void>(),
            SID_MODEL_6581,
        );
        live_set_master(&mut *l, POT);
        for (i, half) in out.chunks_mut(2 * HALF_FRAMES).enumerate() {
            for &(_, cc, value) in ccs.iter().filter(|c| c.0 == i) {
                live_cc(&mut *l, cc, value);
            }
            if i == note_block {
                live_note_on(&mut *l, note, 100);
            }
            let n = half.len() / 2;
            sid_render(&mut *chip, mono.as_mut_ptr(), n as u32);
            sid_slots_process(&mut *slots, mono.as_ptr(), half.as_mut_ptr(), n as u32);
        }
    }
    out
}

/// The largest difference between two interleaved renders, in LSB.
pub fn max_diff(a: &[i16], b: &[i16]) -> i32 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| (i32::from(x) - i32::from(y)).abs())
        .max()
        .unwrap_or(0)
}

/// The left channel played during the next `ms`, as floats.
pub fn record(m: &mut Machine, ms: u64) -> Vec<f32> {
    let from = m.soc.periph.audio.len() / 2;
    m.run_ms(ms);
    m.left()[from..].iter().map(|&s| f32::from(s)).collect()
}

pub fn pitch(x: &[f32]) -> f64 {
    estimate_f0(x, FS, 60.0, 4000.0)
}

/// The share of the signal's power above `hz` (a DFT of the first 4096 samples, mean removed).
pub fn power_above(x: &[f32], hz: f64) -> f64 {
    let n = 4096.min(x.len());
    let mean = x[..n].iter().map(|&v| f64::from(v)).sum::<f64>() / n as f64;
    let v: Vec<f64> = x[..n].iter().map(|&s| f64::from(s) - mean).collect();
    let (mut above, mut total) = (0.0, 0.0);
    for k in 1..n / 2 {
        let w = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &s) in v.iter().enumerate() {
            let a = w * i as f64;
            re += s * a.cos();
            im -= s * a.sin();
        }
        let p = re * re + im * im;
        total += p;
        if k as f64 * FS / n as f64 > hz {
            above += p;
        }
    }
    above / total
}

/// The largest absolute sample.
pub fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, &s| m.max(s.abs()))
}

// ---- benchmark images (bench/fm1): the effects gates ----

const BENCH_RESULT_ADDR: u32 = 0x01C8_0000;
const BENCH_RESULT_MAGIC: u32 = 0x5349_4442;
const BENCH_MAX_STEPS: u64 = 2_000_000_000;

/// A benchmark image run to completion.
pub struct Run {
    /// Instructions between consecutive `bench_mark` calls.
    pub phases: Vec<u64>,
    pub samples: Vec<i16>,
    /// Instructions run by `step_block_load_dec` instead of fm1-core.
    pub emulated_block_loads: u64,
}

fn bench_u32(soc: &Soc, addr: u32) -> u32 {
    let o = addr - fm1_soc::RAM_BASE;
    u32::from_le_bytes([
        soc.ram.read8(o),
        soc.ram.read8(o + 1),
        soc.ram.read8(o + 2),
        soc.ram.read8(o + 3),
    ])
}

/// Runs `bench/fm1/<name>.bin` to completion; it must call `bench_mark` `marks` times. The
/// samples it leaves at the result address come back with the instruction counts.
pub fn run(build: &Path, name: &str, marks: usize) -> Run {
    use object::{Object, ObjectSymbol};
    let dir = build.join("bench/fm1");
    let elf_path = dir.join(format!("{name}.elf"));
    let elf = read(&elf_path, "bench ELF");
    let bin = read(&dir.join(format!("{name}.bin")), "bench image");
    let file = object::File::parse(&*elf)
        .unwrap_or_else(|e| panic!("parse ELF {}: {e}", elf_path.display()));
    let mark = file
        .symbols()
        .find(|s| s.name() == Ok("bench_mark"))
        .expect("bench_mark symbol")
        .address() as u32;

    let mut soc = Soc::new(1 << 20, bin);
    let mut cpu = Cpu::new(fm1_soc::XIP_BASE);
    let mut seen = Vec::new();
    let mut steps: u64 = 0;
    let mut emulated_block_loads: u64 = 0;
    while bench_u32(&soc, BENCH_RESULT_ADDR) != BENCH_RESULT_MAGIC {
        if cpu.pc == mark {
            seen.push(steps);
        }
        if step(&mut cpu, &mut soc) {
            emulated_block_loads += 1;
        }
        steps += 1;
        assert!(
            steps < BENCH_MAX_STEPS,
            "benchmark did not finish (pc {:#010x})",
            cpu.pc
        );
    }
    assert_eq!(
        seen.len(),
        marks,
        "expected {marks} bench_mark calls, saw {}",
        seen.len()
    );

    let n = bench_u32(&soc, BENCH_RESULT_ADDR + 4) as usize;
    let base = BENCH_RESULT_ADDR + 0x40 - fm1_soc::RAM_BASE;
    let samples = (0..n)
        .map(|i| {
            let o = base + 2 * i as u32;
            i16::from_le_bytes([soc.ram.read8(o), soc.ram.read8(o + 1)])
        })
        .collect();
    Run {
        phases: seen.windows(2).map(|w| w[1] - w[0]).collect(),
        samples,
        emulated_block_loads,
    }
}

// ---- the screen's state in emulated RAM ----

/// platform/fm1/ui.c `ui`, an app/views.h `views` whose first field is its `screen`
/// (app/screen.h), at the device's 32-bit layout: `g` (a pointer), `stack` (4 entries of two
/// pointers), then `depth`, `wipe`, `wipe_rows` and `raster_frames`, a byte each.
const SCREEN_STACK: u32 = 4;
const SCREEN_DEPTH: u32 = 36;
const SCREEN_WIPE: u32 = 37;
const SCREEN_RASTER: u32 = 39;
/// app/screen.h SCREEN_DEPTH.
const SCREEN_MAX_DEPTH: u8 = 4;

/// The view on top of the screen's stack (the address of its `view` table), checked against
/// the layout above: the stack's depth in range and the status view at its bottom.
pub fn top_view(m: &mut Machine, fw: &Firmware) -> u32 {
    let ui = fw.symbol("ui");
    let depth = m.soc.read8(ui + SCREEN_DEPTH);
    assert!(
        (1..=SCREEN_MAX_DEPTH).contains(&depth),
        "the screen's depth reads {depth}: platform/fm1/ui.c ui's layout moved"
    );
    assert_eq!(
        m.soc.read32(ui + SCREEN_STACK),
        fw.symbol("views_status"),
        "the stack's bottom is not the status view: platform/fm1/ui.c ui's layout moved"
    );
    m.soc.read32(ui + SCREEN_STACK + 8 * u32::from(depth - 1))
}

/// 1 while a wipe or a raster runs (app/screen.h screen_animating).
pub fn screen_animating(m: &mut Machine, fw: &Firmware) -> bool {
    let ui = fw.symbol("ui");
    m.soc.read8(ui + SCREEN_WIPE) != 0 || m.soc.read8(ui + SCREEN_RASTER) != 0
}

/// 1 while a wipe runs.
pub fn screen_wiping(m: &mut Machine, fw: &Firmware) -> bool {
    let ui = fw.symbol("ui");
    m.soc.read8(ui + SCREEN_WIPE) != 0
}

/// Longer than the full boot (app/boot.h: the title card ends 10 s in) and its wipe.
pub const BOOT_TIMEOUT_MS: u64 = 12_000;

/// Runs a frame (40 ms) at a time until the boot view (app/boot.h) is gone and its wipe has
/// ended; panics after `BOOT_TIMEOUT_MS` from power-on. Returns the time, ms from power-on.
pub fn run_past_boot(m: &mut Machine, fw: &Firmware) -> u64 {
    let boot = fw.symbol("boot_view");
    while m.ms() < BOOT_TIMEOUT_MS {
        // The screen exists once ui_init has run; before, `ui` is zero.
        let ui = fw.symbol("ui");
        let started = m.soc.read8(ui + SCREEN_DEPTH) != 0;
        if started && top_view(m, fw) != boot && !screen_animating(m, fw) {
            return m.ms();
        }
        m.run_ms(40);
    }
    panic!("the boot was still on the screen {BOOT_TIMEOUT_MS} ms after power-on");
}

/// Runs a millisecond at a time until the firmware's last frame (platform/fm1/ui.c
/// `last_compose`, on the main loop's `uptime_ms`) is 20 ms old: half way between two frames,
/// when the last one's cells have gone out and the next one's have not. Then 40 ms steps each
/// hold one frame whole, and a look at the panel sees a frame, not part of one.
pub fn to_mid_frame(m: &mut Machine, fw: &Firmware) {
    let (last, now) = (fw.symbol("last_compose"), fw.symbol("uptime_ms"));
    for _ in 0..80 {
        let age = m.soc.read32(now).wrapping_sub(m.soc.read32(last));
        if age == 20 {
            return;
        }
        m.run_ms(1);
    }
    panic!("the UI's frames are not 40 ms apart");
}
