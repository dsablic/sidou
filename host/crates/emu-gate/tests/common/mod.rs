// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared by the emulator tests: where the fm1 build is, and fm1-emu workarounds.

#![allow(dead_code)] // each test binary uses its own subset

use std::path::{Path, PathBuf};

pub mod usb_host;

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

/// One instruction through fm1-core's step. Panics on a fault.
pub fn step(cpu: &mut Cpu, soc: &mut Soc) {
    let pc = cpu.pc;
    cpu.step(soc)
        .unwrap_or_else(|e| panic!("cpu fault at {pc:#010x}: {e:?}"));
}

/// Where the SPL maps app.bin, and enters it.
pub const APP_BASE: u32 = 0x0200_0120;
/// The SPL's boot-parameter block (Felucca's crt0.S: inside the vector table at 0x01C7FE00).
const BOOT_PARAMS: u32 = 0x01C7_FE08;
/// Where fm1-emu's CLI builds the SPL's boot parameters for a package boot (RAM_BASE + 0x7fc00).
const PACKAGE_PARAMS: u32 = 0x01C7_FC00;
const RAM_SIZE: usize = 1 << 20;
const IRQ_ALNK0: u8 = 11;

/// The stacks (platform/fm1/app.ld `_ustack_lo`.. `_ustack_top`, `_sstack_lo`.. `_sstack_top`;
/// song.rs checks these against the ELF): the main loop's (user mode) and the interrupts'.
pub const USTACK: std::ops::Range<u32> = 0x01C7_8000..0x01C7_A000;
pub const SSTACK: std::ops::Range<u32> = 0x01C7_A100..0x01C7_C000;
/// What `Machine::start` fills both stacks with before the firmware's first instruction: the
/// deepest use is where the fill ends (`Machine::stack_use`).
const STACK_PAINT: u32 = 0x5AC3_A53C;

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

    /// A `Machine::profile` summed by function, the costliest first: (name, instructions).
    pub fn profile_by_function(
        &self,
        profile: &std::collections::HashMap<u32, u64>,
    ) -> Vec<(String, u64)> {
        use object::{Object, ObjectSymbol, SymbolKind};
        let file = object::File::parse(&*self.elf).expect("parse the firmware ELF");
        let mut syms: Vec<(u64, u64, String)> = file
            .symbols()
            .filter(|s| s.kind() == SymbolKind::Text && s.size() > 0)
            .map(|s| (s.address(), s.size(), s.name().unwrap_or("?").to_string()))
            .collect();
        syms.sort();
        let mut by: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for (&pc, &n) in profile {
            let pc = u64::from(pc);
            let i = syms.partition_point(|s| s.0 <= pc);
            let name = match i.checked_sub(1).map(|i| &syms[i]) {
                Some((a, size, name)) if pc < a + size => name.clone(),
                _ => format!("{pc:#010x}"),
            };
            *by.entry(name).or_insert(0) += n;
        }
        let mut v: Vec<_> = by.into_iter().collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.1));
        v
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
    /// The instructions of each ALNK0 interrupt (one audio block), nested ones included.
    pub alnk_blocks: Vec<u64>,
    alnk_open: Option<(usize, u64)>,
    /// Calls of the watched function outside interrupts: instructions each, its callees and
    /// nothing from interrupts included.
    pub watched_calls: Vec<u64>,
    /// The cycle at which each of those calls began.
    pub watched_starts: Vec<u64>,
    /// With `watch_bytes`: the watched range's bytes at each call's entry and at its return.
    pub watched_entry_bytes: Vec<Vec<u8>>,
    pub watched_exit_bytes: Vec<Vec<u8>>,
    watch_range: Option<(u32, u32)>,
    watch: Option<u32>,
    open_call: Option<OpenCall>,
    /// Instructions run outside interrupts (the main loop).
    pub main_steps: u64,
    /// With `watch_passes`: the main-loop instructions of each stretch from an entry of its
    /// first function to the next entry of its second, outside interrupts.
    pub passes: Vec<u64>,
    pass_watch: Option<(u32, u32)>,
    pass_start: Option<u64>,
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
    /// A USB host on the cable (common::usb_host), None: no host, as fm1-emu has none.
    pub usb: Option<usb_host::UsbHost>,
    /// Whether dropping the machine checks its stacks' deepest use (`Drop`): off for a run
    /// that leaves SIDOU (the stock firmware has its own stacks).
    pub stack_check: bool,
    /// With Some: every main-loop instruction inside a watched call (`watch_calls`; with no
    /// call watched, `watch_passes`' stretches), counted by its PC: where they go.
    pub profile: Option<std::collections::HashMap<u32, u64>>,
    /// TRS bytes to come at their cycles, in order: each goes into the jack (`midi`) once the
    /// clock reaches its cycle, also while a stall holds the CPU (a clock source that runs on
    /// whatever the firmware does).
    pub trs_at: std::collections::VecDeque<(u64, u8)>,
    /// (pc, cycles): each time the main loop reaches `pc`, the CPU stands still for `cycles`
    /// while the clock, the peripherals and `trs_at` run on, interrupts left pending, as a
    /// flash operation the firmware waits on with interrupts masked (fm1-emu's flash is
    /// instant). `stalled` counts them.
    pub stalls: Vec<(u32, u64)>,
    pub stalled: u64,
    /// (pc, cycles): once, when the main loop reaches `pc`, it stays there for `cycles` while
    /// the interrupts run as usual, as a main-loop pass that long would (a long pass).
    pub busy: Option<(u32, u64)>,
    busy_until: Option<u64>,
    /// Each main-loop instruction that slept (an `idle` fm1-emu skipped ahead on), by its PC,
    /// with how many times.
    pub idles: std::collections::BTreeMap<u32, u64>,
    /// run_cycles_jit's tick already spent on the next instruction (machine::run_blocks's
    /// `pending_tick`), carried from one run to the next.
    jit_tick: bool,
}

/// Every emulator test's journey checks the stacks when its machine goes: each stack's deepest
/// use stays within half of it (docs/plans/2026-10-19-m3-sid-files.md, Before M3 item 5: the
/// user stack is sized at twice its deepest use). With SIDOU_STACK_LOG set to a file, each
/// machine appends its test's name and both depths to it (the measurement the sizes come from).
impl Drop for Machine {
    fn drop(&mut self) {
        if !self.stack_check || std::thread::panicking() {
            return;
        }
        let (user, system) = self.stack_use();
        let name = std::thread::current().name().unwrap_or("?").to_string();
        if let Some(path) = std::env::var_os("SIDOU_STACK_LOG") {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("SIDOU_STACK_LOG");
            writeln!(f, "{name} {user} {system}").expect("SIDOU_STACK_LOG");
        }
        let (u, s) = (USTACK.end - USTACK.start, SSTACK.end - SSTACK.start);
        assert!(
            2 * user <= u && 2 * system <= s,
            "{name}: the stacks' deepest use, user {user} of {u}, system {system} of {s}: over half"
        );
    }
}

/// The TRS backlog `Machine::midi_paced` keeps: under a third of fm1-emu's limit, about 0.3 s
/// of bytes.
pub const MIDI_PACED_BACKLOG: usize = 900;

/// `Machine::trs_at`'s bytes whose cycle has come, into the jack.
fn feed_trs(trs_at: &mut std::collections::VecDeque<(u64, u8)>, soc: &mut Soc) {
    while let Some(&(at, b)) = trs_at.front() {
        if at > soc.periph.cycles {
            break;
        }
        soc.periph.apply(&fm1_soc::Action::Midi { b });
        trs_at.pop_front();
    }
}

// ---- the store core on the host, over a flash dump (read only) ----

unsafe extern "C" fn dump_map(ctx: *mut std::ffi::c_void, off: u32) -> *const u8 {
    unsafe { ctx.cast::<u8>().add(off as usize) }
}

unsafe extern "C" fn dump_refuse(
    _: *mut std::ffi::c_void,
    _: u32,
    _: *const std::ffi::c_void,
    _: u32,
) -> i32 {
    sidou_sys::STORE_ERROR
}

unsafe extern "C" fn dump_no_erase(_: *mut std::ffi::c_void, _: u32) -> i32 {
    sidou_sys::STORE_ERROR
}

/// The store mounted from a flash dump (the whole chip) on the host: the same C core as the
/// firmware's, which may not write. Whatever it finds.
fn mount_any(flash: &mut [u8]) -> Box<sidou_sys::store> {
    use sidou_sys::*;
    let f = store_flash {
        map: Some(dump_map),
        prog: Some(dump_refuse),
        erase: Some(dump_no_erase),
        ctx: flash.as_mut_ptr().cast(),
    };
    let mut st: Box<store> = Box::default();
    unsafe {
        store_init(&mut *st, &f, 256);
        store_mount(&mut *st);
        while store_busy(&*st) != 0 {
            store_poll(&mut *st);
        }
    }
    st
}

/// As mount_any, which must find SIDOU's store, uncorrupted.
pub fn mount_dump(flash: &mut [u8]) -> Box<sidou_sys::store> {
    let st = mount_any(flash);
    assert_eq!(st.state, sidou_sys::STORE_STATE_SIDOU as u8);
    assert_eq!(st.corrupt, 0, "the mount saw corruption");
    st
}

/// `key`'s value in a flash dump's store, if it is SIDOU's and holds it.
pub fn stored_value(flash: &[u8], key: u32) -> Option<Vec<u8>> {
    let mut flash = flash.to_vec();
    let st = mount_any(&mut flash);
    if st.state != sidou_sys::STORE_STATE_SIDOU as u8 {
        return None;
    }
    let mut buf = vec![0u8; sidou_sys::STORE_VALUE_MAX as usize];
    let n = unsafe {
        sidou_sys::store_get(
            &*st,
            key,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
            std::ptr::null_mut(),
        )
    };
    (n >= 0).then(|| {
        buf.truncate(n as usize);
        buf
    })
}

/// Emulated ms the first boot on a blank flash runs before its settings are stored: the debounce
/// (app/settings.h, 5 s from the load at the screen's start) and the held programs' wait for
/// quiet, with room.
pub const SETTLE_MS: u64 = 7_000;

/// The flash after a first boot of `image` on a blank chip: its settings stored (app/settings.h:
/// the defaults and the firmware's version), as a device holds them from its second power-on.
/// Made once per image and test process.
pub fn settled_flash(image: &[u8]) -> Vec<u8> {
    use std::sync::{Mutex, OnceLock};
    type FlashCache = Vec<(Vec<u8>, Vec<u8>)>;
    static MADE: OnceLock<Mutex<FlashCache>> = OnceLock::new();
    let made = MADE.get_or_init(|| Mutex::new(Vec::new()));
    let mut made = made.lock().unwrap();
    if let Some((_, f)) = made.iter().find(|(i, _)| i == image) {
        return f.clone();
    }
    let mut m = Machine::boot_blank(image);
    m.run_ms_jit(SETTLE_MS);
    let flash = m.soc.flash.mem.clone();
    assert!(
        stored_value(&flash, sidou_sys::SETTINGS_KEY).is_some(),
        "the first boot stored no settings in {SETTLE_MS} ms"
    );
    made.push((image.to_vec(), flash.clone()));
    flash
}

impl Machine {
    /// As the device boots from its second power-on: the settings of a first boot stored, so
    /// the boot is the short one (app/boot.h). boot_blank is the first power-on after an install.
    pub fn boot(image: &[u8]) -> Self {
        let flash = settled_flash(image);
        let mut m = Self::boot_blank(image);
        m.soc.flash.mem.copy_from_slice(&flash);
        m
    }

    /// On a blank flash chip: the first power-on after an install (the full boot, app/boot.h).
    pub fn boot_blank(image: &[u8]) -> Self {
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
        let r0 = fm1_soc::stock_boot(&mut soc, &stock.flash, PACKAGE_PARAMS, fm1_soc::Board::Fm1);
        Self::start(soc, stock.entry, r0)
    }

    fn start(mut soc: Soc, entry: u32, r0: u32) -> Self {
        // The firmware's main loop sleeps in `idle`; skip ahead to the next interrupt.
        soc.periph.skip_idle = true;
        for a in USTACK.step_by(4).chain(SSTACK.step_by(4)) {
            soc.write32(a, STACK_PAINT);
        }
        let mut cpu = Cpu::new(entry);
        // The SPL hands cpu0 over in user mode (fm1-emu main.rs).
        cpu.user = true;
        cpu.regs[0] = r0;
        Machine {
            soc,
            cpu,
            steps: 0,
            isr_steps: 0,
            alnk_blocks: Vec::new(),
            alnk_open: None,
            watched_calls: Vec::new(),
            watched_starts: Vec::new(),
            watched_entry_bytes: Vec::new(),
            watched_exit_bytes: Vec::new(),
            watch_range: None,
            watch: None,
            open_call: None,
            main_steps: 0,
            passes: Vec::new(),
            pass_watch: None,
            pass_start: None,
            alnk_pcs_inside: 0,
            alnk_pcs_outside: std::collections::BTreeSet::new(),
            alnk_pc_range: None,
            half_played: Vec::new(),
            buffer_snapshots: Vec::new(),
            snapshot: None,
            snapshot_bytes: Vec::new(),
            alnk_entry: 0,
            usb: None,
            stack_check: true,
            profile: None,
            trs_at: std::collections::VecDeque::new(),
            stalls: Vec::new(),
            stalled: 0,
            busy: None,
            busy_until: None,
            idles: std::collections::BTreeMap::new(),
            jit_tick: false,
        }
    }

    /// The deepest use of each stack so far, in bytes (user, system): from the top down to the
    /// lowest word that no longer holds `start`'s fill.
    pub fn stack_use(&mut self) -> (u32, u32) {
        let mut deepest = |r: std::ops::Range<u32>| {
            let top = r.end;
            let low = r
                .step_by(4)
                .find(|&a| self.soc.read32(a) != STACK_PAINT)
                .unwrap_or(top);
            top - low
        };
        (deepest(USTACK), deepest(SSTACK))
    }

    /// Counts the instructions of every call of the function at `entry` from the main loop.
    pub fn watch_calls(&mut self, entry: u32) {
        self.watch = Some(entry);
    }

    /// With `watch_calls`: snapshots `len` bytes at `addr` at each watched call's entry and
    /// return (`watched_entry_bytes`, `watched_exit_bytes`), e.g. the state a call works on.
    pub fn watch_bytes(&mut self, addr: u32, len: u32) {
        self.watch_range = Some((addr, len));
    }

    /// Counts the main loop's instructions from each entry of the function at `from` to the
    /// next entry of the one at `to` (`passes`): with midi_drain and midi_service, what a
    /// main-loop pass runs between draining MIDI and its other services (play(), ui_poll).
    pub fn watch_passes(&mut self, from: u32, to: u32) {
        self.pass_watch = Some((from, to));
        self.pass_start = None;
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

    /// Runs `ms` milliseconds through fm1-soc's own loop (`machine::run_blocks`) with fm1-core's
    /// JIT, several times faster than `run_ms`, for a run whose only result is the machine it
    /// leaves (the settled flash). It keeps none of this Machine's counts (steps, the ALNK0
    /// blocks, the watches), has no USB host, and it stops within a
    /// block of the end rather than on it. The JIT runs the same guest instructions as the
    /// interpreter (fm1-emu stops both at the same one), so the flash it leaves is the stepped
    /// run's, byte for byte: `boot.rs` checks that.
    pub fn run_ms_jit(&mut self, ms: u64) {
        let n = self.soc.periph.cycle_of_ms(ms);
        self.run_cycles_jit(n);
    }

    /// `run_ms_jit` for `n` cycles.
    pub fn run_cycles_jit(&mut self, n: u64) {
        assert!(self.usb.is_none(), "run_ms_jit has no USB host");
        let end = self.soc.periph.cycles + n;
        self.cpu.jit_on = true;
        let mut pending_tick = std::mem::take(&mut self.jit_tick);
        while self.soc.periph.cycles < end {
            // (the budget is in instructions, while `skip_idle` jumps the clock: a short one, so
            // the run stops near `end` and not that many instructions later)
            let n = (end - self.soc.periph.cycles).min(4096) as u32;
            fm1_soc::machine::run_blocks(&mut self.cpu, &mut self.soc, n, &mut pending_tick)
                .unwrap_or_else(|e| panic!("cpu fault: {e}"));
            assert!(!self.soc.cpu1_release, "SIDOU runs on core 0 only");
        }
        self.jit_tick = pending_tick;
        self.cpu.jit_on = false;
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
                if let Some((from, to)) = self.pass_watch {
                    if cpu.pc == from {
                        self.pass_start = Some(self.main_steps);
                    } else if cpu.pc == to {
                        if let Some(start) = self.pass_start.take() {
                            self.passes.push(self.main_steps - start);
                        }
                    }
                }
                if let Some(entry) = self.watch {
                    if cpu.pc == entry && self.open_call.is_none() {
                        self.watched_starts.push(soc.periph.cycles);
                        if let Some((addr, len)) = self.watch_range {
                            let b = (0..len).map(|i| soc.read8(addr + i)).collect();
                            self.watched_entry_bytes.push(b);
                        }
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
                        if let Some((addr, len)) = self.watch_range {
                            let b = (0..len).map(|i| soc.read8(addr + i)).collect();
                            self.watched_exit_bytes.push(b);
                        }
                    }
                }
                if let Some(p) = &mut self.profile {
                    if self.open_call.is_some()
                        || (self.watch.is_none() && self.pass_start.is_some())
                    {
                        *p.entry(cpu.pc).or_insert(0) += 1;
                    }
                }
                self.main_steps += 1;
            }
            if !in_isr {
                if let Some((pc, n)) = self.busy {
                    if cpu.pc == pc {
                        self.busy = None;
                        self.busy_until = Some(soc.periph.cycles + n);
                    }
                }
                if let Some(until) = self.busy_until {
                    if soc.periph.cycles < until {
                        feed_trs(&mut self.trs_at, soc);
                        self.steps += 1;
                        continue; // the pass goes on: the next cycle, interrupts taken
                    }
                    self.busy_until = None;
                }
            }
            if !in_isr && !self.stalls.is_empty() {
                if let Some(&(_, n)) = self.stalls.iter().find(|s| s.0 == cpu.pc) {
                    self.stalled += 1;
                    for _ in 0..n {
                        if soc.periph.tick() {
                            let halves = soc.periph.audio_halves;
                            soc.advance(0);
                            if soc.periph.audio_halves != halves {
                                self.half_played.push(soc.periph.cycles);
                            }
                        }
                        feed_trs(&mut self.trs_at, soc);
                    }
                }
            }
            feed_trs(&mut self.trs_at, soc);
            let (pc, skipped) = (cpu.pc, soc.periph.skipped);
            step(cpu, soc);
            if !in_isr && soc.periph.skipped != skipped {
                *self.idles.entry(pc).or_insert(0) += 1;
            }
            if let Some(usb) = &mut self.usb {
                usb.after_step(soc, self.alnk_open.is_some());
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

    /// Runs `ms` milliseconds a 1 ms USB frame at a time (common::usb_host): SOF, and the
    /// IN packets the device readied go to the host.
    pub fn usb_frames(&mut self, ms: u64) {
        for _ in 0..ms {
            if let Some(usb) = &mut self.usb {
                usb.frame(&mut self.soc);
            }
            self.run_ms(1);
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

    /// Runs `ms` milliseconds with the TRS jack at its full speed: `message(i)` gives message
    /// `i` (whole messages, so the last one fed is the last one played once the jack
    /// drains), fed as the jack takes them. fm1-emu drops what is behind a second's backlog
    /// (`MIDI_QUEUE_MAX`, 3125 bytes), so a stream poured in at once plays half of it and
    /// leaves the jack idle for the rest: here the backlog is topped up to
    /// `MIDI_PACED_BACKLOG` bytes every millisecond, so it never runs dry (a byte takes 320
    /// us) and never overflows. Returns how many messages it fed. The caller checks
    /// `midi_dropped() == 0` and `midi_queued() > 0` at the end: the jack was busy throughout.
    pub fn midi_paced(&mut self, ms: u64, mut message: impl FnMut(u32) -> Vec<u8>) -> u32 {
        let mut fed = 0u32;
        let mut top_up = |m: &mut Machine| {
            while m.soc.periph.midi_queued() < MIDI_PACED_BACKLOG {
                m.midi(&message(fed));
                fed += 1;
            }
        };
        for _ in 0..ms {
            top_up(self);
            self.run_ms(1);
        }
        fed
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
/// SID gate's max_instructions_per_sample (sid_budget.rs fails above it). M1b measures 556.3
/// (two chips, gate_every_block on the 6581, since Task 6's non-linear 6581 filter; 526.8
/// before) and 552.2 (one chip); M1c Task 3 549.37 and 545.30 (the 6581 render's clamps as
/// fmax and fmin, and its struct's layout: decision record 0006, Cost). The effect gates no
/// longer pair it with the slots (they take the worst block, below); it stays as the SID's
/// guard.
pub const SID_WORST_PER_CHIP: f64 = 550.0;

/// The SID's most expensive block (128 samples) in instructions per sample, for one chip
/// and for two together, as sid_budget.rs measures it (it fails above these). The audio
/// ISR has a hard per-block deadline, so the effect gates pair the slots with this, not
/// with a scenario's average. Both are the first block of a combined-noise scenario from
/// power-on while the register runs through its shifts until it locks up (decision record
/// 0004, "Worst blocks"), on the 6581 since M1b Task 6's non-linear filter: noise_tri_ffff
/// for one chip (717.49 in M1b) and noise_tri_ring_0800 for two (1,439.67). The 8580's are
/// 691.16 and 1,409.87 (690.85 and 1,409.24 before Task 6, when they were the worst).
/// M1c Task 3 added the chip's character (character_noise_*: the worn 6581 profile and the
/// filter bleed at 127, the one control with a per-sample cost), which first measured 724.21
/// and 1,453.96; with sid_nl_tick's clamps as the FPU's fmax and fmin the worst are again
/// the default chip's: 710.48 and 1,425.66 (the character's 709.94 and 1,424.80).
pub const SID_WORST_BLOCK_ONE_CHIP: f64 = 711.0;
pub const SID_WORST_BLOCK_TWO_CHIPS: f64 = 1426.0;

/// Instructions a sample the LIVE view's tap (app/scope.h) adds to the audio interrupt
/// (ram_audio.rs fails above it). LIVE is the view at power-on, so the tap runs by default
/// and the effect gates count it with the chips. docs/plans/2026-10-15-m1e-look-and-feel.md
/// asks for under about 10. M1e Task 4 measured 4.8, with sid_render called once per
/// 16-sample point. The points now come from inside one render a block
/// (sid_render_tap), and M1b Task 6's 6581 measures 4.44 (406.71 against 402.27, a held
/// chord).
pub const TAP_PER_SAMPLE: f64 = 4.5;

/// Instructions a sample recording over USB audio adds to the audio interrupt (usb.rs fails
/// above it): the audio interrupt itself does nothing for it (platform/fm1/usb_audio.h), but
/// TIMER5's work (a packet, a quarter of the producer) nests in a render now and then. Measured
/// 0.87 at 44.1 kHz and 1.11 at 48 (a chord held, three turns each). The rest runs in TIMER5
/// outside renders: every interrupt together gains 41.83 a sample recording at 44.1 kHz and
/// 335.31 at 48, where it resamples.
pub const USB_AUDIO_PER_SAMPLE: f64 = 2.0;

/// The audio target, instructions per sample for everything the audio interrupt does (design
/// doc section 8): slots_budget.rs splits it, latency.rs holds the render allowance to it.
pub const AUDIO_TARGET: f64 = 2700.0;
/// The audio ISR's own overhead around the chips and the slots (0004's estimate).
pub const ISR_OVERHEAD: f64 = 100.0;
/// Two chips plus the effects (M1c plan Task 1's cap).
pub const TWO_CHIPS_PLUS_EFFECTS: f64 = 2300.0;

/// The song player's worst 128-sample block in instructions a sample (song.rs fails above
/// it): its call and SONG_EDITS_PER_BLOCK of the costliest edit command, with the block's
/// command handling (M2c Task 6; the plan's budget, docs/plans/2026-10-15-m2c-song-engine.md).
pub const PLAYER_WORST_BLOCK: f64 = 60.0;
/// The digi drums' cost in the audio interrupt (core/player/song_ctl.c song_ctl_block_end: its
/// trigger and its mix; decision record 0009), instructions a sample while a digi drum plays
/// without a break (song.rs: a hit every row, each 220 ms long); measured 10.42, then 9.47 with
/// the codes two a byte and each code's level held to a pointer (no count a sample;
/// docs/plans/2026-10-24-flash-space.md). A bound set from this measurement (new code); with no
/// digi playing the function tests one word a block, and no example song has a DIGI chunk, so it
/// is outside the player's worst block and added here.
pub const DIGI_PER_SAMPLE: f64 = 9.5;
/// The effect slots' worst switching block, instructions a sample (slots_budget.rs measures
/// 964, M1c Task 3): what the audio target holds besides the chips, the tap and the ISR when
/// a preset change switches the slots.
pub const SLOTS_SWITCH_WORST: f64 = 964.0;
/// Free RAM (the RAM region and POOL): the last row of the overview's gate table
/// (docs/plans/2026-10-15-m2-overview.md, "RAM"), M2f's: 10,000. The table counts each plan's
/// budget once, and M2a's 2,000 is already spent inside M2c's 22,000, so the plans on top of
/// it are M2d (6,000), M2e (2,000) and M2f (5,000): 22,000 - 13,000 = 9,000, and the table's
/// slack makes 10,000. (It was 7,000 until the end-of-M2 review, which counted M2a twice.)
/// M3's Task 9 lowers it by the 512 bytes M3 takes.
pub const FREE_RAM_MIN: u64 = 10_000;
/// Each region (the RAM region, POOL) keeps this free: M3's start target in
/// docs/plans/2026-10-19-m3-sid-files.md, "Before M3": the 4 KiB floor plus 1 KiB, for M3's 512
/// bytes plus a margin.
pub const REGION_FREE_MIN: u64 = 5_120;
// A song plays with two chips at their worst, the effects at their cap, the ISR, the tap, the
// player and a digi drum, while a computer records over USB: within the audio target.
const _: () = assert!(
    TWO_CHIPS_PLUS_EFFECTS
        + TAP_PER_SAMPLE
        + ISR_OVERHEAD
        + PLAYER_WORST_BLOCK
        + DIGI_PER_SAMPLE
        + USB_AUDIO_PER_SAMPLE
        <= AUDIO_TARGET
);

/// fm1-emu's CPU clock, one instruction a cycle (latency.rs checks it).
pub const CPU_HZ: u64 = 240_000_000;
/// The TRS UART poll's rate (platform/fm1/main.c: every 5th 10 kHz tick).
pub const POLL_HZ: u64 = 2000;
/// The latency gate's poll allowance in main-loop instructions (latency.rs: a note-on is in
/// the block that starts within a block and a poll of its arrival, 3.40 ms): 0.5 ms at
/// CPU_HZ. The main loop drains MIDI between ui_poll calls, so a longer ui_poll would eat
/// into it: every view's worst ui_poll bound stays within it (screen.rs).
pub const POLL_INSTRUCTIONS: u64 = CPU_HZ / POLL_HZ;

/// 24 MHz / 544, the I2S frame rate.
pub const FS: f64 = 24e6 / 544.0;

/// platform/fm1/audio.h AUDIO_SAMPLE_RATE and AUDIO_HALF_FRAMES.
pub const SAMPLE_RATE: u32 = 44_118;
pub const HALF_FRAMES: usize = 128;
/// fm1-emu's MASTER pot reading at power-on.
pub const POT: u16 = 900;
/// The MASTER gain at the pot's power-on reading, as a share of the chips' output (the audio taper,
/// app/master_gain.h): a peak the chips alone would give is this much of it at the speaker. The
/// tests' "a note sounds" levels are set for the chips' own output and scale by it.
pub fn boot_gain() -> f32 {
    // SAFETY: a pure function of its arguments.
    let g = unsafe { sidou_sys::master_gain_of(u32::from(POT), sidou_sys::LIVE_MASTER_MAX) };
    g as f32 / sidou_sys::MASTER_UNITY as f32
}

/// The MASTER gain stage audio_fill ends with (platform/fm1/audio.c, app/master_gain.h): the
/// block's interleaved L, R at the gain of `master` (the pot, 0..LIVE_MASTER_MAX), held since
/// the boot set it: no ramp.
pub fn master_stage(half: &mut [i16], master: u16) {
    let mut block = [0i16; 2 * HALF_FRAMES];
    let n = half.len();
    assert!(n <= block.len());
    block[..n].copy_from_slice(half);
    // SAFETY: block holds the 128 frames master_gain_apply scales.
    unsafe {
        let g = sidou_sys::master_gain_of(u32::from(master), sidou_sys::LIVE_MASTER_MAX);
        sidou_sys::master_gain_apply(block.as_mut_ptr(), sidou_sys::MASTER_BLOCK_LOG2, g, g);
    }
    half.copy_from_slice(&block[..n]);
}
/// The tail gate audio_fill runs on the mixed mono block before the slots (platform/fm1/audio.c,
/// app/tail_gate.h): chip A's, and chip B's if it rendered. A partial block (the last of a render
/// that is not whole blocks) is not run: the device renders whole blocks only.
pub fn tail_stage(
    gate: &mut sidou_sys::tail_gate,
    a: &sidou_sys::sid,
    b: Option<&sidou_sys::sid>,
    mono: &mut [i16],
    n: usize,
) {
    if n == HALF_FRAMES {
        // SAFETY: mono holds the block's 128 samples; the chips are alive and not rendering.
        unsafe {
            sidou_sys::tail_gate_run(
                gate,
                a,
                b.map_or(std::ptr::null(), |b| b as *const _),
                mono.as_mut_ptr(),
            )
        };
    }
}
/// The chips' thump audio_fill plays after the slots (platform/fm1/audio.c, app/thump_out.h):
/// chip A's steps and chip B's if it rendered the block (`b`; one out of use drops its own,
/// as the device does), on the block's interleaved L, R.
pub fn thump_stage(
    t: &mut sidou_sys::thump_out,
    a: &mut sidou_sys::sid,
    b: Option<&mut sidou_sys::sid>,
    half: &mut [i16],
    n: usize,
) {
    // SAFETY: the chips are alive and not rendering; half holds n frames.
    unsafe {
        sidou_sys::thump_out_take(t, 0, a);
        if let Some(b) = b {
            sidou_sys::thump_out_take(t, 1, b);
        }
        sidou_sys::thump_out_apply(t, half.as_mut_ptr(), n as u32);
    }
}

/// platform/fm1/audio.c SLOTS_SEED.
pub const SLOTS_SEED: u32 = 0x51D0;

/// The firmware's instrument at boot (platform/fm1/main.c: live_init, its preparer, then the
/// MASTER pot), rendered by the host build in blocks as audio_fill does (the chip, then the
/// effect slots); a profile step prepares the spare a slice a tick as the device does
/// (live_sid_prepare), so its reset lands at the device's tick. Before block `note_block` the note `note` goes on; each `(block, cc, value)` of
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
    let mut spare = Box::<sid>::default();
    let mut dacs = Box::<[sid_dac; 2]>::default();
    let mut pool = Box::<sid_slot_pool>::default();
    let mut slots = Box::<sid_slots>::default();
    let mut fx = Box::<fx_live>::default();
    let mut target = Box::new(fx_live_target {
        sid: live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: SAMPLE_RATE,
            dacs: dacs.as_mut_ptr(),
            spare: &mut *spare,
            ..Default::default()
        },
        fx: &mut *fx,
    });
    let mut l = Box::<live>::default();
    let t: *mut fx_live_target = &mut *target;
    let mut out = vec![0i16; 2 * frames];
    let mut mono = vec![0i16; HALF_FRAMES];
    let mut gate = tail_gate::default();
    let mut thump = thump_out::default();
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
        // The device's preparer (platform/fm1/main.c): a profile step's reset at its tick.
        let sid_target: *mut live_sid_target = &mut (*t).sid;
        live_set_preparer(&mut *l, Some(live_sid_prepare), sid_target.cast());
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
            tail_stage(&mut gate, &chip, None, &mut mono, n);
            sid_slots_process(&mut *slots, mono.as_ptr(), half.as_mut_ptr(), n as u32);
            thump_stage(&mut thump, &mut chip, None, half, n);
            master_stage(half, POT);
        }
    }
    out
}

/// Chip B's events for render_host_chips, before a block as the device's drain applies them:
/// register writes straight to chip A or chip B (sid_write, as the drain does with entries
/// pushed into the ring) and chip B going in or out of use.
pub struct ChipEvent {
    pub block: usize,
    pub a: Vec<(u8, u8)>,
    pub b: Vec<(u8, u8)>,
    pub b_use: Option<bool>,
}

/// What render_host_chips renders: the output (interleaved L, R), and each chip's own samples
/// before the mix (chip B's 0 while out of use).
pub struct ChipsRender {
    pub out: Vec<i16>,
    pub a: Vec<i16>,
    pub b: Vec<i16>,
}

/// The firmware's two chips at boot rendered by the host build as audio_fill does: chip A the
/// instrument (render_host, idle), chip B made as the device's boot makes it (the keys'
/// setup: the 6581 R4AR at character 0, the envelope instant; platform/fm1/audio.c
/// app/keys_route.h: chip B follows the keys' setup from the first ticks) and rendered only
/// while in use (with a user, or until its envelopes are at 0), faded in or out over the block it goes
/// in or out of use (app/chip_mix.h), mixed before the effect slots.
pub fn render_host_chips(frames: usize, events: &[ChipEvent]) -> ChipsRender {
    use sidou_sys::*;
    let mut chip = Box::<sid>::default();
    let mut spare = Box::<sid>::default();
    let mut dacs = Box::<[sid_dac; 2]>::default();
    let mut chip_b = Box::<sid>::default();
    let mut dacs_b = Box::<[sid_dac; 2]>::default();
    let mut pool = Box::<sid_slot_pool>::default();
    let mut slots = Box::<sid_slots>::default();
    let mut fx = Box::<fx_live>::default();
    let mut target = Box::new(fx_live_target {
        sid: live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: SAMPLE_RATE,
            dacs: dacs.as_mut_ptr(),
            spare: &mut *spare,
            ..Default::default()
        },
        fx: &mut *fx,
    });
    let mut target_b = Box::new(live_sid_target {
        chip: &mut *chip_b,
        clock_hz: SID_CLOCK_PAL,
        sample_rate: SAMPLE_RATE,
        dacs: dacs_b.as_mut_ptr(),
        ..Default::default()
    });
    let mut l = Box::<live>::default();
    let t: *mut fx_live_target = &mut *target;
    let tb: *mut live_sid_target = &mut *target_b;
    let mut r = ChipsRender {
        out: vec![0i16; 2 * frames],
        a: vec![0i16; frames],
        b: vec![0i16; frames],
    };
    let mut mono = vec![0i16; HALF_FRAMES];
    let mut gate = tail_gate::default();
    let mut thump = thump_out::default();
    let mut mono_b = vec![0i16; HALF_FRAMES];
    let mut over = vec![0i16; HALF_FRAMES];
    let mut b_on = false;
    let mut b_want = false;
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
        let sid_target: *mut live_sid_target = &mut (*t).sid;
        live_set_preparer(&mut *l, Some(live_sid_prepare), sid_target.cast());
        live_set_master(&mut *l, POT);
        let sink_b = |reg: u32, value: u32| live_sid_sink(tb.cast(), reg as u8, value as u8);
        sink_b(LIVE_REG_CHAR_FIRST, SID_CHIP_6581_R4AR);
        for f in 1..LIVE_CHAR_FIELDS {
            sink_b(LIVE_REG_CHAR_FIRST + f, 0);
        }
        sink_b(LIVE_REG_RESET, SID_MODEL_6581);
        sink_b(LIVE_REG_GATE_RESET, 1);
        for (i, half) in r.out.chunks_mut(2 * HALF_FRAMES).enumerate() {
            for e in events.iter().filter(|e| e.block == i) {
                for &(reg, v) in &e.a {
                    sid_write(&mut *chip, reg, v);
                }
                for &(reg, v) in &e.b {
                    sid_write(&mut *chip_b, reg, v);
                }
                if let Some(u) = e.b_use {
                    b_want = u;
                }
            }
            let n = half.len() / 2;
            sid_render(&mut *chip, mono.as_mut_ptr(), n as u32);
            r.a[i * HALF_FRAMES..i * HALF_FRAMES + n].copy_from_slice(&mono[..n]);
            // In use with a user, or until its envelopes are at 0 after the last let go
            // (platform/fm1/audio.c audio_fill).
            let tails = b_on && chip_b.env.iter().any(|e| e.counter != 0);
            let b_in = b_want || tails;
            let b_rendered = b_in || b_on;
            if b_in || b_on {
                sid_render(&mut *chip_b, mono_b.as_mut_ptr(), n as u32);
                r.b[i * HALF_FRAMES..i * HALF_FRAMES + n].copy_from_slice(&mono_b[..n]);
                let fade = if b_in == b_on {
                    CHIP_MIX_FULL
                } else if b_in {
                    CHIP_MIX_IN
                } else {
                    CHIP_MIX_OUT
                };
                // The excess where some sum is past int16, else none (audio_fill's no_over).
                if chip_mix(
                    mono.as_mut_ptr(),
                    mono_b.as_ptr(),
                    over.as_mut_ptr(),
                    n as u32,
                    fade,
                ) == 0
                {
                    over.fill(0);
                }
                b_on = b_in;
            }
            tail_stage(
                &mut gate,
                &chip,
                b_rendered.then_some(&*chip_b),
                &mut mono,
                n,
            );
            // The two chips' whole sum past int16 (app/chip_mix.h), as audio_fill passes it.
            sid_slots_process_wide(
                &mut *slots,
                mono.as_ptr(),
                if b_rendered {
                    over.as_ptr()
                } else {
                    std::ptr::null()
                },
                half.as_mut_ptr(),
                n as u32,
            );
            if !b_rendered {
                thump_out_drop(&mut *chip_b);
            }
            thump_stage(
                &mut thump,
                &mut chip,
                b_rendered.then_some(&mut *chip_b),
                half,
                n,
            );
            master_stage(half, POT);
        }
    }
    r
}

/// The device's song region (platform/fm1/audio.c `song_region`, 64 KiB) holding `sng` as
/// the device reads it (its multispeed cap), with `o` as the song's options (an OPTS chunk)
/// if given: the bytes a test writes into the device's region, and the host's copy.
pub fn device_song(sng: &[u8], o: Option<&sidou_sys::gt2_options>) -> sidou_tests::song::Song {
    let mut song = sidou_tests::song::Song::device();
    song.read(sng, 0).expect("the song reads");
    if let Some(o) = o {
        assert_eq!(
            unsafe { sidou_sys::song_ext_set_options(song.ptr_mut(), o) },
            0
        );
    }
    song
}

/// A primitive of the song model making one edit (gt2_song.h).
pub type SongPrim = Box<dyn Fn(*mut sidou_sys::gt2_song, &mut sidou_sys::gt2_edit) -> u32>;

/// What the device did with a song, for the host to do the same at the same blocks.
pub struct SongPlay<'a> {
    /// The song (a copy plays).
    pub song: &'a sidou_tests::song::Song,
    /// The block whose start applied PLAY (0: the first block the interrupt rendered).
    pub play_block: usize,
    /// (block, cc, value): to live before the block, as render_host's.
    pub ccs: &'a [(usize, u8, u8)],
    /// live_tick calls before each block (the device's, from the drain's tick count).
    pub ticks: &'a [u32],
    /// (block, primitive): edits sent before the block (applied at its start).
    pub edits: &'a [(usize, SongPrim)],
    /// The block whose start applies STOP.
    pub stop_block: Option<usize>,
    /// The keys' move to chip B at PLAY (app/keys_route.h): the block chip B goes in use, and
    /// the block whose drain applies the keys' registers on it (live_rewrite).
    pub b_use_block: Option<usize>,
    pub b_rewrite_block: Option<usize>,
}

struct HostSink {
    target: *mut sidou_sys::fx_live_target,
    reset: bool,
}

unsafe extern "C" fn host_song_sink(ctx: *mut std::ffi::c_void, reg: u8, value: u8) {
    let h = unsafe { &mut *(ctx as *mut HostSink) };
    if reg == sidou_sys::LIVE_REG_RESET as u8 {
        h.reset = true;
    }
    unsafe { sidou_sys::fx_live_sid_sink(h.target.cast(), reg, value) };
}

unsafe extern "C" fn host_ready(
    _ctx: *mut std::ffi::c_void,
    _setup: *const sidou_sys::song_chip_setup,
) -> i32 {
    1
}

/// The firmware playing a song (platform/fm1/audio.c and main.c), rendered by the host build:
/// the instrument at boot as render_host, chip B as render_host_chips makes it, song_ctl on
/// chip A, its commands sent where the device sent them, chip A's envelopes AUTHENTIC from the
/// play (the song's setup), the keys moved to chip B at the device's blocks, each block's
/// chip A split at the call as audio_fill splits it, chip B mixed while in use, then the
/// effect slots. Interleaved L, R.
pub fn render_host_song(frames: usize, p: &SongPlay) -> Vec<i16> {
    render_host_song_master(frames, p, POT)
}

/// render_host_song with the MASTER pot at `master` for the whole run (the firmware's boot
/// reads the pot before the audio interrupt starts, so the gain is in force from the first
/// block; the song's registers are the same at every MASTER).
pub fn render_host_song_master(frames: usize, p: &SongPlay, master: u16) -> Vec<i16> {
    use sidou_sys::*;
    let mut chip = Box::<sid>::default();
    let mut spare = Box::<sid>::default();
    let mut dacs = Box::<[sid_dac; 2]>::default();
    let mut chip_b = Box::<sid>::default();
    let mut dacs_b = Box::<[sid_dac; 2]>::default();
    let mut pool = Box::<sid_slot_pool>::default();
    let mut slots = Box::<sid_slots>::default();
    let mut fx = Box::<fx_live>::default();
    let mut target = Box::new(fx_live_target {
        sid: live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: SAMPLE_RATE,
            dacs: dacs.as_mut_ptr(),
            spare: &mut *spare,
            ..Default::default()
        },
        fx: &mut *fx,
    });
    let mut target_b = Box::new(live_sid_target {
        chip: &mut *chip_b,
        clock_hz: SID_CLOCK_PAL,
        sample_rate: SAMPLE_RATE,
        dacs: dacs_b.as_mut_ptr(),
        ..Default::default()
    });
    let tb: *mut live_sid_target = &mut *target_b;
    let mut mono_b = vec![0i16; HALF_FRAMES];
    let mut over = vec![0i16; HALF_FRAMES];
    let (mut b_on, mut b_want) = (false, false);
    let mut sink = Box::new(HostSink {
        target: &mut *target,
        reset: false,
    });
    let mut l = Box::<live>::default();
    let mut song = sidou_tests::song::Song::device();
    song.copy_from(p.song);
    let mut sc = Box::<song_ctl>::default();
    let mut shapes = Box::<[gt2_shape; 2]>::default();
    let mut work = Box::<song_work>::default();
    let mut out = vec![0i16; 2 * frames];
    let mut mono = vec![0i16; HALF_FRAMES];
    let mut gate = tail_gate::default();
    let mut thump = thump_out::default();
    // SAFETY: everything C keeps a pointer to is boxed and outlives the calls; the buffers
    // hold the samples asked for.
    unsafe {
        sid_slots_init(&mut *slots, SAMPLE_RATE, SLOTS_SEED, &mut *pool);
        fx_live_init(&mut *fx, &mut *slots);
        let sink_ctx: *mut HostSink = &mut *sink;
        live_init(
            &mut *l,
            Some(host_song_sink),
            sink_ctx.cast(),
            SID_MODEL_6581,
        );
        let sid_target: *mut live_sid_target = &mut (*sink.target).sid;
        live_set_preparer(&mut *l, Some(live_sid_prepare), sid_target.cast());
        let (a, b) = shapes.split_at_mut(1);
        song_ctl_init(&mut *sc, song.ptr_mut(), &mut a[0], &mut b[0], &mut *work);
        let lp: *mut live = &mut *l;
        // the device's hooks bring chip A to the song's setup (the gate reset, above) and hand
        // it back to keys_route at the stop: no register writes of song control's own
        let _ = lp;
        let hooks = song_chip_hooks {
            chip_a_song: Some(host_ready),
            chip_a_keys: None,
            chip_a_character: None,
            ctx: std::ptr::null_mut(),
        };
        song_ctl_set_hooks(&mut *sc, &hooks);
        live_set_master(&mut *l, POT);
        // chip B, made with the keys' setup (render_host_chips)
        let sink_b = |reg: u32, value: u32| live_sid_sink(tb.cast(), reg as u8, value as u8);
        sink_b(LIVE_REG_CHAR_FIRST, SID_CHIP_6581_R4AR);
        for f in 1..LIVE_CHAR_FIELDS {
            sink_b(LIVE_REG_CHAR_FIRST + f, 0);
        }
        sink_b(LIVE_REG_RESET, SID_MODEL_6581);
        sink_b(LIVE_REG_GATE_RESET, 1);
        sink.reset = false;
        for (i, half) in out.chunks_mut(2 * HALF_FRAMES).enumerate() {
            for &(_, cc, value) in p.ccs.iter().filter(|c| c.0 == i) {
                live_cc(&mut *l, cc, value);
            }
            for _ in 0..p.ticks.get(i).copied().unwrap_or(0) {
                live_tick(&mut *l);
            }
            if p.b_rewrite_block == Some(i) {
                // the keys' registers on chip B (keys_route's move: live_rewrite)
                let (s0, c0) = (l.sink, l.sink_ctx);
                l.sink = Some(live_sid_sink);
                l.sink_ctx = tb.cast();
                live_rewrite(&mut *l);
                l.sink = s0;
                l.sink_ctx = c0;
            }
            if p.b_use_block == Some(i) {
                b_want = true;
            }
            if i == p.play_block {
                // the song's setup reached chip A before the play: AUTHENTIC envelopes
                sid_set_gate_reset(&mut *chip, 0);
                assert_eq!(song_ctl_play(&mut *sc, 0, 0), 1);
                while song_ctl_state(&*sc) == SONG_LOADING {
                    song_ctl_poll(&mut *sc, SONG_POLL_SLICE | SONG_POLL_RING);
                }
            }
            if p.edits.iter().any(|e| e.0 == i) {
                // The device releases the preceding block's acknowledged swaps between
                // blocks, before preparing the next recorded edit.
                song_ctl_poll(&mut *sc, SONG_POLL_RING);
            }
            for (_, prim) in p.edits.iter().filter(|e| e.0 == i) {
                let mut e = gt2_edit::default();
                assert_eq!(prim(song.ptr_mut(), &mut e), GT2_OK, "the host's edit");
                assert_eq!(song_ctl_edit(&mut *sc, &e), 1, "the host's edit went");
                // The recorded block is application, after the device's main-loop shape
                // preparation. Instrument/table edits can require that preparation even
                // when their resulting shape is compatible with the running player.
                while sc.deriving != 0 {
                    song_ctl_poll(&mut *sc, SONG_POLL_SLICE | SONG_POLL_RING);
                }
            }
            if p.stop_block == Some(i) {
                assert_eq!(song_ctl_stop(&mut *sc), 1);
            }
            song_ctl_poll(&mut *sc, SONG_POLL_RING);
            song_ctl_commit(&mut *sc);
            // the drain's swap hook: chip A was swapped for a prepared one
            if std::mem::take(&mut sink.reset) {
                song_ctl_chip_swapped((&mut *sc as *mut song_ctl).cast(), 0, &mut *chip);
            }
            let n = half.len() / 2;
            song_ctl_block(&mut *sc, &mut *chip);
            song_ctl_render(&mut *sc, &mut *chip, mono.as_mut_ptr(), n as u32);
            let tails = b_on && chip_b.env.iter().any(|e| e.counter != 0);
            let b_in = b_want || tails;
            let b_rendered = b_in || b_on;
            if b_in || b_on {
                sid_render(&mut *chip_b, mono_b.as_mut_ptr(), n as u32);
                let fade = if b_in == b_on {
                    CHIP_MIX_FULL
                } else if b_in {
                    CHIP_MIX_IN
                } else {
                    CHIP_MIX_OUT
                };
                // The excess where some sum is past int16, else none (audio_fill's no_over).
                if chip_mix(
                    mono.as_mut_ptr(),
                    mono_b.as_ptr(),
                    over.as_mut_ptr(),
                    n as u32,
                    fade,
                ) == 0
                {
                    over.fill(0);
                }
                b_on = b_in;
            }
            tail_stage(
                &mut gate,
                &chip,
                b_rendered.then_some(&*chip_b),
                &mut mono,
                n,
            );
            song_ctl_block_end(&mut *sc, mono.as_mut_ptr(), n as u32);
            // The two chips' whole sum past int16 (app/chip_mix.h), as audio_fill passes it.
            sid_slots_process_wide(
                &mut *slots,
                mono.as_ptr(),
                if b_rendered {
                    over.as_ptr()
                } else {
                    std::ptr::null()
                },
                half.as_mut_ptr(),
                n as u32,
            );
            if !b_rendered {
                thump_out_drop(&mut *chip_b);
            }
            thump_stage(
                &mut thump,
                &mut chip,
                b_rendered.then_some(&mut *chip_b),
                half,
                n,
            );
            master_stage(half, master);
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
    /// The result area's first words (0 the magic, 1 the samples, then the image's own).
    pub words: [u32; 8],
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
    run_with(build, name, marks, |_| {})
}

/// `run`, with `setup` writing the emulated RAM before the image starts (its inputs).
pub fn run_with(build: &Path, name: &str, marks: usize, setup: impl FnOnce(&mut Soc)) -> Run {
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
    setup(&mut soc);
    let mut cpu = Cpu::new(fm1_soc::XIP_BASE);
    let mut seen = Vec::new();
    let mut steps: u64 = 0;
    while bench_u32(&soc, BENCH_RESULT_ADDR) != BENCH_RESULT_MAGIC {
        if cpu.pc == mark {
            seen.push(steps);
        }
        step(&mut cpu, &mut soc);
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
    let words = std::array::from_fn(|i| bench_u32(&soc, BENCH_RESULT_ADDR + 4 * i as u32));
    Run {
        phases: seen.windows(2).map(|w| w[1] - w[0]).collect(),
        samples,
        words,
    }
}

// ---- the screen's state in emulated RAM ----

/// platform/fm1/ui.c `ui`, an app/views.h `views` whose first field is its `screen`
/// (app/screen.h), at the device's 32-bit layout: `g` (a pointer), `stack` (SCREEN_SLOTS entries
/// of two pointers: SCREEN_DEPTH views and the help card's slot), then `depth`, `wipe`,
/// `wipe_rows` and `raster_frames`, a byte each.
const SCREEN_STACK: u32 = sidou_sys::ilp32::screen::stack as u32;
const SCREEN_DEPTH: u32 = sidou_sys::ilp32::screen::depth as u32;
const SCREEN_WIPE: u32 = sidou_sys::ilp32::screen::wipe as u32;
const SCREEN_RASTER: u32 = sidou_sys::ilp32::screen::raster_frames as u32;
/// app/screen.h SCREEN_SLOTS: the views and the help card's slot.
const SCREEN_MAX_DEPTH: u8 = sidou_sys::SCREEN_SLOTS as u8;

/// The view on top of the screen's stack (the address of its `view` table), checked against
/// the layout above: the stack's depth in range and LIVE, the root, at its bottom.
pub fn top_view(m: &mut Machine, fw: &Firmware) -> u32 {
    let ui = fw.symbol("ui");
    let depth = m.soc.read8(ui + SCREEN_DEPTH);
    assert!(
        (1..=SCREEN_MAX_DEPTH).contains(&depth),
        "the screen's depth reads {depth}: platform/fm1/ui.c ui's layout moved"
    );
    assert_eq!(
        m.soc.read32(ui + SCREEN_STACK),
        fw.symbol("live_view_view"),
        "the stack's bottom is not LIVE: platform/fm1/ui.c ui's layout moved"
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

/// app/panel.h HOME and the menu's encoders; app/views.h the STATUS item and the items.
const HOME: u8 = 8;
const SELECT: u8 = 0;
const ALGORITHM: u8 = 1;
const MENU_STATUS: i32 = sidou_sys::VIEWS_MENU_STATUS as i32;
const MENU_ITEMS: i32 = sidou_sys::VIEWS_MENU_ITEMS as i32;

/// With the menu showing: SETTINGS opened from it (SELECT to it, ALGORITHM clockwise) and its
/// cursor on `item` (app/views.h VIEWS_SETTINGS_*). HOME twice goes back to LIVE.
pub fn settings_item(m: &mut Machine, item: u32) {
    m.turn(SELECT, -MENU_ITEMS);
    m.run_ms(10 * MENU_ITEMS as u64);
    m.turn(SELECT, sidou_sys::VIEWS_MENU_SETTINGS as i32);
    m.run_ms(10 * u64::from(sidou_sys::VIEWS_MENU_SETTINGS));
    m.turn(ALGORITHM, 1);
    m.run_ms(100); // (a cut: no wipe)
    m.turn(SELECT, -(sidou_sys::VIEWS_SETTINGS_ITEMS as i32));
    m.run_ms(10 * u64::from(sidou_sys::VIEWS_SETTINGS_ITEMS));
    m.turn(SELECT, item as i32);
    m.run_ms(10 * u64::from(item) + 50);
}

/// The status screen from LIVE, through the menu (HOME, SELECT to STATUS, ALGORITHM
/// clockwise), its wipe over, and on to half way between two frames (to_mid_frame).
pub fn open_status(m: &mut Machine, fw: &Firmware) {
    m.key(HOME, true);
    m.run_ms(30);
    m.key(HOME, false);
    m.run_ms(30);
    // fm1-emu turns an encoder a detent every 8 ms: each turn is waited out.
    m.turn(SELECT, -MENU_ITEMS);
    m.run_ms(10 * MENU_ITEMS as u64);
    m.turn(SELECT, MENU_STATUS);
    m.run_ms(10 * MENU_STATUS as u64);
    m.turn(ALGORITHM, 1);
    m.run_ms(700);
    assert_eq!(
        top_view(m, fw),
        fw.symbol("views_status"),
        "the menu's STATUS did not open the status screen"
    );
    to_mid_frame(m, fw);
}

/// The tap's `on` (platform/fm1/audio.c `tap`), checked against the pi32v2 layout.
pub fn tap_on(m: &mut Machine, fw: &Firmware) -> bool {
    use object::{Object, ObjectSymbol};
    let file = object::File::parse(&*fw.elf).expect("parse the firmware ELF");
    let sym = file
        .symbols()
        .find(|s| s.name() == Ok("tap"))
        .expect("no symbol tap in the firmware ELF");
    assert_eq!(
        sym.size(),
        sidou_sys::ilp32::scope::SIZE as u64,
        "the tap's layout moved"
    );
    m.soc
        .read8(sym.address() as u32 + sidou_sys::ilp32::scope::on as u32)
        != 0
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
