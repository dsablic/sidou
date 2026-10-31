// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(dead_code, reason = "each test binary uses its own subset")]

use std::path::{Path, PathBuf};

mod budgets;
mod host_render;
pub mod panel;
pub mod usb_host;

#[allow(unused_imports, reason = "each test binary uses its own subset")]
pub use budgets::*;
#[allow(unused_imports, reason = "each test binary uses its own subset")]
pub use host_render::*;
use panel::*;

use fm1_core::{Bus, Cpu};
use fm1_soc::Soc;
use sidou_tests::signal_metrics::estimate_f0;

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

pub fn step(cpu: &mut Cpu, soc: &mut Soc) {
    let pc = cpu.pc;
    cpu.step(&mut AlignmentCheckedBus { soc, pc })
        .unwrap_or_else(|e| panic!("cpu fault at {pc:#010x}: {e:?}"));
}

struct AlignmentCheckedBus<'a> {
    soc: &'a mut Soc,
    pc: u32,
}

impl AlignmentCheckedBus<'_> {
    #[inline(always)]
    fn check(&self, addr: u32, bytes: u32) {
        if addr & (bytes - 1) != 0 {
            misaligned(self.pc, addr, bytes);
        }
    }
}

#[cold]
#[inline(never)]
fn misaligned(pc: u32, addr: u32, bytes: u32) {
    let ram = fm1_soc::RAM_BASE..fm1_soc::RAM_BASE + RAM_SIZE as u32;
    assert!(
        !ram.contains(&addr),
        "pc {pc:#010x}: a {bytes}-byte RAM access at {addr:#010x}, which the FM-1 faults on"
    );
}

impl Bus for AlignmentCheckedBus<'_> {
    fn read8(&mut self, addr: u32) -> u8 {
        self.soc.read8(addr)
    }
    fn read16(&mut self, addr: u32) -> u16 {
        self.check(addr, 2);
        self.soc.read16(addr)
    }
    fn read32(&mut self, addr: u32) -> u32 {
        self.check(addr, 4);
        self.soc.read32(addr)
    }
    fn fetch32(&mut self, addr: u32) -> u32 {
        self.soc.fetch32(addr)
    }
    fn write8(&mut self, addr: u32, value: u8) {
        self.soc.write8(addr, value);
    }
    fn write16(&mut self, addr: u32, value: u16) {
        self.check(addr, 2);
        self.soc.write16(addr, value);
    }
    fn write32(&mut self, addr: u32, value: u32) {
        self.check(addr, 4);
        self.soc.write32(addr, value);
    }
    fn tick(&mut self) -> bool {
        self.soc.tick()
    }
    fn irq_pending(&self) -> bool {
        self.soc.irq_pending()
    }
    fn jit_mem(&mut self) -> Option<fm1_core::JitMem> {
        self.soc.jit_mem()
    }
    fn tick_room(&self) -> u64 {
        self.soc.tick_room()
    }
    fn add_ticks(&mut self, n: u64) {
        self.soc.add_ticks(n);
    }
    fn hw_lock(&mut self, core: u32, set: bool) -> bool {
        self.soc.hw_lock(core, set)
    }
    fn next_tick_due(&self) -> bool {
        self.soc.next_tick_due()
    }
    fn idle(&mut self, core: u32) {
        self.soc.idle(core);
    }
    fn irq_window(&mut self) {
        self.soc.irq_window();
    }
    fn service(&mut self) {
        self.soc.service();
    }
    fn code_writes(&self) -> Option<u64> {
        self.soc.code_writes()
    }
    fn code_mark(&mut self, lo: u32, hi: u32) {
        self.soc.code_mark(lo, hi);
    }
}

pub const APP_BASE: u32 = 0x0200_0120;
const BOOT_PARAMS: u32 = 0x01C7_FE08;
const BOOT_PARAMS_PLACEHOLDER: u32 = BOOT_PARAMS + 0x40;
const PACKAGE_PARAMS: u32 = 0x01C7_FC00;
const RAM_SIZE: usize = 1 << 20;
const IRQ_ALNK0: u8 = 11;
const MASTER_POT_ADC_CHANNEL: u8 = 4;
const SPL_ENTERS_IN_USER_MODE: bool = true;

pub const USER_STACK: std::ops::Range<u32> = 0x01C7_8000..0x01C7_A000;
pub const SYSTEM_STACK: std::ops::Range<u32> = 0x01C7_A100..0x01C7_C000;
const STACK_PAINT: u32 = 0x5AC3_A53C;
const JIT_SLICE_INSTRUCTIONS: u64 = 4096;

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

    pub fn symbol(&self, name: &str) -> u32 {
        use object::{Object, ObjectSymbol};
        let file = object::File::parse(&*self.elf).expect("parse the firmware ELF");
        let sym = file
            .symbols()
            .find(|s| s.name() == Ok(name))
            .unwrap_or_else(|| panic!("no symbol {name} in the firmware ELF"));
        sym.address() as u32
    }

    pub fn function_range(&self, name: &str) -> std::ops::Range<u32> {
        use object::{Object, ObjectSymbol};
        let file = object::File::parse(&*self.elf).expect("parse the firmware ELF");
        let s = file
            .symbols()
            .find(|s| s.name() == Ok(name))
            .unwrap_or_else(|| panic!("no symbol {name}"));
        s.address() as u32..(s.address() + s.size()) as u32
    }

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

pub struct BufferSnapshot {
    pub entry: u64,
    pub exit: u64,
    pub words: Vec<i16>,
    pub bytes: Vec<u8>,
}

struct OpenCall {
    rets: u32,
    sp: u32,
    start: u64,
}

pub struct Machine {
    pub soc: Soc,
    pub cpu: Cpu,
    pub steps: u64,
    pub isr_steps: u64,
    pub alnk_blocks: Vec<u64>,
    alnk_open: Option<(usize, u64)>,
    pub watched_calls: Vec<u64>,
    pub watched_starts: Vec<u64>,
    pub watched_entry_bytes: Vec<Vec<u8>>,
    pub watched_exit_bytes: Vec<Vec<u8>>,
    watched_bytes_range: Option<(u32, u32)>,
    watched_entry: Option<u32>,
    open_call: Option<OpenCall>,
    pub main_steps: u64,
    pub passes: Vec<u64>,
    pass_watch: Option<(u32, u32)>,
    pass_start: Option<u64>,
    pub alnk_pcs_inside: u64,
    pub alnk_pcs_outside: std::collections::BTreeSet<u32>,
    alnk_pc_range: Option<std::ops::Range<u32>>,
    pub half_played: Vec<u64>,
    pub buffer_snapshots: Vec<BufferSnapshot>,
    snapshot: Option<(u32, u32)>,
    pub snapshot_bytes: Vec<u32>,
    alnk_entry: u64,
    pub usb: Option<usb_host::UsbHost>,
    pub check_stacks_on_drop: bool,
    pub profile: Option<std::collections::HashMap<u32, u64>>,
    pub trs_scheduled: std::collections::VecDeque<(u64, u8)>,
    pub masked_stalls: Vec<(u32, u64)>,
    pub masked_stalls_taken: u64,
    pub long_pass: Option<(u32, u64)>,
    long_pass_until: Option<u64>,
    pub idles: std::collections::BTreeMap<u32, u64>,
    jit_tick: bool,
    pub resets: Vec<(u64, u8)>,
    pub stop_at_reset: bool,
}

pub const RST_WDT: u8 = fm1_soc::p33::P33::RST_WDT;
pub const RST_SOFT: u8 = fm1_soc::p33::P33::RST_SOFT;

impl Drop for Machine {
    fn drop(&mut self) {
        if !self.check_stacks_on_drop || std::thread::panicking() {
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
        let (u, s) = (
            USER_STACK.end - USER_STACK.start,
            SYSTEM_STACK.end - SYSTEM_STACK.start,
        );
        assert!(
            2 * user <= u && 2 * system <= s,
            "{name}: the stacks' deepest use, user {user} of {u}, system {system} of {s}: over half"
        );
    }
}

pub const MIDI_PACED_BACKLOG: usize = 900;

fn tick_and_note_halves(soc: &mut Soc, half_played: &mut Vec<u64>) -> bool {
    let ticked = soc.periph.tick();
    if ticked {
        let halves = soc.periph.audio_halves;
        soc.advance(0);
        if soc.periph.audio_halves != halves {
            half_played.push(soc.periph.cycles);
        }
    }
    ticked
}

fn read_bytes(soc: &mut Soc, (addr, len): (u32, u32)) -> Vec<u8> {
    (0..len).map(|i| soc.read8(addr + i)).collect()
}

fn feed_trs(trs_scheduled: &mut std::collections::VecDeque<(u64, u8)>, soc: &mut Soc) {
    while let Some(&(at, b)) = trs_scheduled.front() {
        if at > soc.periph.cycles {
            break;
        }
        soc.periph.apply(&fm1_soc::Action::Midi { b });
        trs_scheduled.pop_front();
    }
}

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

pub fn mount_dump(flash: &mut [u8]) -> Box<sidou_sys::store> {
    let st = mount_any(flash);
    assert_eq!(st.state, sidou_sys::STORE_STATE_SIDOU as u8);
    assert_eq!(st.corrupt, 0, "the mount saw corruption");
    st
}

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

pub fn stored_file(flash: &[u8], name: &str) -> Option<(u8, Vec<u8>)> {
    use sidou_sys::*;
    let mut flash = flash.to_vec();
    let st = mount_any(&mut flash);
    if st.state != STORE_STATE_SIDOU as u8 {
        return None;
    }
    let name = std::ffi::CString::new(name).unwrap();
    let mut f = store_file::default();
    if unsafe { store_file_open(&*st, name.as_ptr(), &mut f) } != STORE_OK {
        return None;
    }
    let mut buf = vec![0u8; f.length as usize];
    let n = unsafe { store_file_read(&*st, &f, 0, buf.as_mut_ptr().cast(), f.length) };
    (n == f.length as i32).then_some((f.type_, buf))
}

unsafe extern "C" fn copy_map(ctx: *mut std::ffi::c_void, off: u32) -> *const u8 {
    unsafe { ctx.cast::<u8>().add(off as usize) }
}

unsafe extern "C" fn copy_prog(
    ctx: *mut std::ffi::c_void,
    off: u32,
    data: *const std::ffi::c_void,
    n: u32,
) -> i32 {
    for i in 0..n as usize {
        unsafe { *ctx.cast::<u8>().add(off as usize + i) &= *data.cast::<u8>().add(i) };
    }
    sidou_sys::STORE_OK
}

unsafe extern "C" fn copy_erase(ctx: *mut std::ffi::c_void, off: u32) -> i32 {
    for i in 0..sidou_sys::STORE_SECTOR as usize {
        unsafe { *ctx.cast::<u8>().add(off as usize + i) = 0xff };
    }
    sidou_sys::STORE_OK
}

pub fn put_file(flash: &mut [u8], name: &str, file_type: u32, bytes: &[u8]) {
    use sidou_sys::*;
    let hooks = store_flash {
        map: Some(copy_map),
        prog: Some(copy_prog),
        erase: Some(copy_erase),
        ctx: flash.as_mut_ptr().cast(),
    };
    let mut st = Box::<store>::default();
    let name = std::ffi::CString::new(name).unwrap();
    unsafe {
        store_init(&mut *st, &hooks, STORE_WINDOW_MAX);
        store_mount(&mut *st);
        store_quiet(&mut *st, 1);
        while store_poll(&mut *st) != 0 {}
        assert_eq!(
            store_file_begin(&mut *st, name.as_ptr(), file_type as u8, bytes.len() as u32),
            STORE_OK
        );
        let mut offset = 0;
        while offset < bytes.len() {
            offset += store_file_write(
                &mut *st,
                bytes.as_ptr().add(offset).cast(),
                (bytes.len() - offset) as u32,
            ) as usize;
            store_poll(&mut *st);
        }
        assert_eq!(store_file_end(&mut *st), STORE_OK);
        while store_file_result(&*st) == STORE_BUSY {
            store_poll(&mut *st);
        }
        assert_eq!(store_file_result(&*st), STORE_OK);
    }
}

pub const SETTLE_MS: u64 = 7_000;

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
    pub fn boot(image: &[u8]) -> Self {
        let flash = settled_flash(image);
        let mut m = Self::boot_blank(image);
        m.soc.flash.mem.copy_from_slice(&flash);
        m
    }

    pub fn boot_blank(image: &[u8]) -> Self {
        let mut xip = vec![0u8; (APP_BASE - fm1_soc::XIP_BASE) as usize];
        xip.extend_from_slice(image);
        let mut soc = Soc::new(RAM_SIZE, xip);
        soc.write32(BOOT_PARAMS, BOOT_PARAMS_PLACEHOLDER);
        Self::start(soc, APP_BASE, BOOT_PARAMS)
    }

    pub fn boot_package(package: &[u8]) -> Self {
        let stock = fm1_soc::stock_from_package(package, None).expect("an FM-1 package");
        let mut soc = Soc::new(RAM_SIZE, stock.xip.clone());
        let r0 = fm1_soc::stock_boot(&mut soc, &stock.flash, PACKAGE_PARAMS, fm1_soc::Board::Fm1);
        Self::start(soc, stock.entry, r0)
    }

    fn start(mut soc: Soc, entry: u32, r0: u32) -> Self {
        soc.periph.skip_idle = true;
        for a in USER_STACK.step_by(4).chain(SYSTEM_STACK.step_by(4)) {
            soc.write32(a, STACK_PAINT);
        }
        let mut cpu = Cpu::new(entry);
        cpu.user = SPL_ENTERS_IN_USER_MODE;
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
            watched_bytes_range: None,
            watched_entry: None,
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
            check_stacks_on_drop: true,
            profile: None,
            trs_scheduled: std::collections::VecDeque::new(),
            masked_stalls: Vec::new(),
            masked_stalls_taken: 0,
            long_pass: None,
            long_pass_until: None,
            idles: std::collections::BTreeMap::new(),
            jit_tick: false,
            resets: Vec::new(),
            stop_at_reset: false,
        }
    }

    pub fn plug_usb(&mut self) {
        let mut host = usb_host::UsbHost::plug(&mut self.soc);
        host.start();
        self.usb = Some(host);
    }

    pub fn reset_due(&self) -> bool {
        self.soc.reset_due()
    }

    pub fn reset(&mut self) {
        if !self.soc.reset_due() {
            return;
        }
        let at = self.ms();
        self.soc.chip_reset(&mut self.cpu);
        let cause = self.soc.p33.reg(fm1_soc::p33::P33::RST_SRC, false);
        self.resets.push((at, cause));
        self.alnk_open = None;
        self.open_call = None;
        self.pass_start = None;
        self.long_pass_until = None;
    }

    pub fn stack_use(&mut self) -> (u32, u32) {
        let mut deepest = |r: std::ops::Range<u32>| {
            let top = r.end;
            let low = r
                .step_by(4)
                .find(|&a| self.soc.read32(a) != STACK_PAINT)
                .unwrap_or(top);
            top - low
        };
        (deepest(USER_STACK), deepest(SYSTEM_STACK))
    }

    pub fn watch_calls(&mut self, entry: u32) {
        self.watched_entry = Some(entry);
    }

    pub fn watch_bytes(&mut self, addr: u32, len: u32) {
        self.watched_bytes_range = Some((addr, len));
    }

    pub fn watch_passes(&mut self, from: u32, to: u32) {
        self.pass_watch = Some((from, to));
        self.pass_start = None;
    }

    pub fn watch_alnk_pcs(&mut self, range: std::ops::Range<u32>) {
        self.alnk_pc_range = Some(range);
    }

    pub fn ms(&self) -> u64 {
        self.soc.periph.ms()
    }

    pub fn snapshot_buffer(&mut self, buffer: Option<(u32, u32)>) {
        self.snapshot = buffer;
    }

    pub fn run_ms(&mut self, ms: u64) {
        let n = self.soc.periph.cycle_of_ms(ms);
        self.run_cycles(n);
    }

    pub fn run_ms_jit(&mut self, ms: u64) {
        let n = self.soc.periph.cycle_of_ms(ms);
        self.run_cycles_jit(n);
    }

    pub fn run_cycles_jit(&mut self, n: u64) {
        assert!(self.usb.is_none(), "run_ms_jit has no USB host");
        let end = self.soc.periph.cycles + n;
        self.cpu.jit_on = true;
        let mut pending_tick = std::mem::take(&mut self.jit_tick);
        while self.soc.periph.cycles < end {
            let n = (end - self.soc.periph.cycles).min(JIT_SLICE_INSTRUCTIONS) as u32;
            let resets = self.soc.resets;
            fm1_soc::machine::run_blocks(&mut self.cpu, &mut self.soc, n, &mut pending_tick)
                .unwrap_or_else(|e| panic!("cpu fault: {e}"));
            if self.soc.resets != resets {
                let cause = self.soc.p33.reg(fm1_soc::p33::P33::RST_SRC, false);
                self.resets.push((self.ms(), cause));
                break;
            }
            assert!(!self.soc.cpu1_release, "SIDOU runs on core 0 only");
        }
        self.jit_tick = pending_tick;
        self.cpu.jit_on = false;
    }

    pub fn run_cycles(&mut self, n: u64) {
        let end = self.soc.periph.cycles + n;
        self.soc.take_boot(&self.cpu);
        while self.soc.periph.cycles < end {
            if self.soc.reset_due() {
                if self.stop_at_reset {
                    return;
                }
                self.reset();
            }
            let soc = &mut self.soc;
            let cpu = &mut self.cpu;
            if tick_and_note_halves(soc, &mut self.half_played) && soc.reset_due() {
                continue;
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
                if let Some(entry) = self.watched_entry {
                    if cpu.pc == entry && self.open_call.is_none() {
                        self.watched_starts.push(soc.periph.cycles);
                        if let Some(range) = self.watched_bytes_range {
                            self.watched_entry_bytes.push(read_bytes(soc, range));
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
                        if let Some(range) = self.watched_bytes_range {
                            self.watched_exit_bytes.push(read_bytes(soc, range));
                        }
                    }
                }
                if let Some(p) = &mut self.profile {
                    if self.open_call.is_some()
                        || (self.watched_entry.is_none() && self.pass_start.is_some())
                    {
                        *p.entry(cpu.pc).or_insert(0) += 1;
                    }
                }
                self.main_steps += 1;
            }
            if !in_isr {
                if let Some((pc, n)) = self.long_pass {
                    if cpu.pc == pc {
                        self.long_pass = None;
                        self.long_pass_until = Some(soc.periph.cycles + n);
                    }
                }
                if let Some(until) = self.long_pass_until {
                    if soc.periph.cycles < until {
                        feed_trs(&mut self.trs_scheduled, soc);
                        self.steps += 1;
                        continue;
                    }
                    self.long_pass_until = None;
                }
            }
            if !in_isr && !self.masked_stalls.is_empty() {
                if let Some(&(_, n)) = self.masked_stalls.iter().find(|s| s.0 == cpu.pc) {
                    self.masked_stalls_taken += 1;
                    for _ in 0..n {
                        tick_and_note_halves(soc, &mut self.half_played);
                        feed_trs(&mut self.trs_scheduled, soc);
                    }
                }
            }
            feed_trs(&mut self.trs_scheduled, soc);
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

    pub fn usb_frames(&mut self, ms: u64) {
        for _ in 0..ms {
            if let Some(usb) = &mut self.usb {
                usb.frame(&mut self.soc);
            }
            self.run_ms(1);
        }
    }

    pub fn key(&mut self, id: u8, down: bool) {
        self.soc.periph.apply(&fm1_soc::Action::Key { id, down });
    }

    pub fn turn(&mut self, enc: u8, clicks: i32) {
        self.soc.periph.apply(&fm1_soc::Action::Enc { enc, clicks });
    }

    pub fn pot(&mut self, value: u16) {
        self.soc.periph.apply(&fm1_soc::Action::Adc {
            ch: MASTER_POT_ADC_CHANNEL,
            value,
        });
    }

    pub fn midi(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.soc.periph.apply(&fm1_soc::Action::Midi { b });
        }
    }

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

    pub fn left(&self) -> Vec<i16> {
        self.soc.periph.audio.iter().step_by(2).copied().collect()
    }

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

pub const FS: f64 = 24e6 / 544.0;

pub fn max_diff(a: &[i16], b: &[i16]) -> i32 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| (i32::from(x) - i32::from(y)).abs())
        .max()
        .unwrap_or(0)
}

pub fn record(m: &mut Machine, ms: u64) -> Vec<f32> {
    let from = m.soc.periph.audio.len() / 2;
    m.run_ms(ms);
    m.left()[from..].iter().map(|&s| f32::from(s)).collect()
}

pub fn pitch(x: &[f32]) -> f64 {
    estimate_f0(x, FS, 60.0, 4000.0)
}

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

pub fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, &s| m.max(s.abs()))
}

pub fn worst_tune(two: bool, control: u8, op: &str, reg: u32) -> Vec<u8> {
    use sidou_tests::psid_fixtures::{Psid, INIT};
    let mut init = String::new();
    for chip in 0..if two { 2 } else { 1 } {
        for v in 0..3 {
            let r = 0xD400 + 0x20 * chip + 7 * v;
            for (off, value) in [
                (0, 0xff),
                (1, 0xff),
                (2, 0x00),
                (3, 0x08),
                (5, 0x00),
                (6, 0xf0),
            ] {
                init += &format!("lda #${value:02x}\nsta ${:04x}\n", r + off);
            }
            init += &format!("lda #${control:02x}\nsta ${:04x}\n", r + 4);
        }
    }
    init += "lda #$ff\nloop:\n";
    if !op.is_empty() {
        for k in 0..240 {
            let chip = if two && k % 2 == 1 { 0x20 } else { 0 };
            init += &format!("{op} ${:04x}\n", 0xD400 + chip + reg);
        }
    }
    init += "jmp loop\n";
    let psid = Psid::new(&[(INIT, &init), (0x8000, "rts\n")]);
    if two { psid.sid2(0x42) } else { psid }.bytes()
}

const BENCH_RESULT_ADDR: u32 = 0x01C8_0000;
const BENCH_RESULT_MAGIC: u32 = 0x5349_4442;
const BENCH_MAX_STEPS: u64 = 2_000_000_000;

pub struct Run {
    pub phases: Vec<u64>,
    pub samples: Vec<i16>,
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

pub fn run(build: &Path, name: &str, marks: usize) -> Run {
    run_with(build, name, marks, |_| {})
}

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

const SCREEN_STACK: u32 = sidou_sys::ilp32::screen::stack as u32;
const SCREEN_DEPTH: u32 = sidou_sys::ilp32::screen::depth as u32;
const SCREEN_WIPE: u32 = sidou_sys::ilp32::screen::wipe as u32;
const SCREEN_RASTER: u32 = sidou_sys::ilp32::screen::raster_frames as u32;
const SCREEN_MAX_DEPTH: u8 = sidou_sys::SCREEN_SLOTS as u8;

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

pub fn screen_animating(m: &mut Machine, fw: &Firmware) -> bool {
    let ui = fw.symbol("ui");
    m.soc.read8(ui + SCREEN_WIPE) != 0 || m.soc.read8(ui + SCREEN_RASTER) != 0
}

pub fn screen_wiping(m: &mut Machine, fw: &Firmware) -> bool {
    let ui = fw.symbol("ui");
    m.soc.read8(ui + SCREEN_WIPE) != 0
}

const MENU_STATUS: i32 = sidou_sys::VIEWS_MENU_STATUS as i32;
const MENU_SETTINGS: i32 = sidou_sys::VIEWS_MENU_SETTINGS as i32;
const MENU_SIDS: i32 = sidou_sys::VIEWS_MENU_SIDS as i32;
const SETTINGS_ITEMS: i32 = sidou_sys::VIEWS_SETTINGS_ITEMS as i32;
const MENU_CUT_MS: u64 = 100;
const MENU_WIPE_MS: u64 = 700;
const SETTINGS_CURSOR_SETTLE_MS: u64 = 50;
const BROWSER_SETTLE_MS: u64 = 80;

pub fn settings_item(m: &mut Machine, item: u32) {
    select_menu_item(m, MENU_SETTINGS);
    m.turn(ALGORITHM, 1);
    m.run_ms(MENU_CUT_MS);
    m.turn(SELECT, -SETTINGS_ITEMS);
    m.run_ms(turn_time(SETTINGS_ITEMS));
    m.turn(SELECT, item as i32);
    m.run_ms(turn_time(item as i32) + SETTINGS_CURSOR_SETTLE_MS);
}

pub fn open_status(m: &mut Machine, fw: &Firmware) {
    press(m, HOME);
    select_menu_item(m, MENU_STATUS);
    m.turn(ALGORITHM, 1);
    m.run_ms(MENU_WIPE_MS);
    assert_eq!(
        top_view(m, fw),
        fw.symbol("views_status"),
        "the menu's STATUS did not open the status screen"
    );
    to_mid_frame(m, fw);
}

pub fn open_sids(m: &mut Machine, fw: &Firmware) {
    open_menu(m, fw);
    select_menu_item(m, MENU_SIDS);
    m.run_ms(DETENT_MS);
    m.turn(ALGORITHM, 1);
    let scanning = fw.symbol("ui")
        + (sidou_sys::ilp32::views::sids + sidou_sys::ilp32::sid_view::scanning) as u32;
    wait_for(m, "the SIDS browser and its page", |m| {
        top_view(m, fw) == fw.symbol("sid_view_browser")
            && !screen_animating(m, fw)
            && m.soc.read8(scanning) == 0
    });
}

pub fn ui_play_tune(m: &mut Machine, fw: &Firmware, index: u32) {
    open_sids(m, fw);
    m.turn(SELECT, -(sidou_sys::SID_VIEW_FILES as i32));
    m.run_ms(BROWSER_SETTLE_MS);
    m.turn(SELECT, index as i32);
    m.run_ms(turn_time(index as i32) + DETENT_MS);
    m.turn(ALGORITHM, 1);
    wait_for(m, "NOW PLAYING", |m| {
        top_view(m, fw) == fw.symbol("sid_view_playing")
    });
}

pub fn ui_back_to(m: &mut Machine, fw: &Firmware, name: &str) {
    let want = fw.symbol(name);
    for _ in 0..4 {
        if top_view(m, fw) == want {
            return;
        }
        let from = top_view(m, fw);
        press(m, HOME);
        wait_for(m, "HOME back a view", |m| {
            top_view(m, fw) != from && !screen_animating(m, fw)
        });
    }
    assert_eq!(top_view(m, fw), want, "not back on {name}");
}

pub fn ui_end_tune(m: &mut Machine, fw: &Firmware) {
    let state = fw.symbol("player_isr") + sidou_sys::ilp32::psid_ctl::state as u32;
    if top_view(m, fw) == fw.symbol("live_view_view") {
        open_sids(m, fw);
    }
    let st = u32::from(m.soc.read8(state));
    if [
        sidou_sys::PSID_INIT,
        sidou_sys::PSID_PLAYING,
        sidou_sys::PSID_PAUSED,
    ]
    .contains(&st)
    {
        press(m, PLAY);
    }
    ui_back_to(m, fw, "live_view_view");
}

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

pub fn build_or_skip(what: &str) -> Option<PathBuf> {
    let build = build_dir();
    if build.is_none() {
        skip_message(what);
    }
    build
}

pub fn build_and_firmware(what: &str) -> Option<(PathBuf, Firmware)> {
    let build = build_or_skip(what)?;
    let fw = Firmware::load(&build);
    Some((build, fw))
}

pub fn firmware(what: &str) -> Option<Firmware> {
    build_and_firmware(what).map(|(_, fw)| fw)
}

pub fn booted(fw: &Firmware) -> Machine {
    let mut m = Machine::boot(&fw.image);
    run_past_boot(&mut m, fw);
    m
}

pub fn assert_within_budget<T: PartialOrd + std::fmt::Display>(what: &str, cost: T, budget: T) {
    assert!(
        cost <= budget,
        "{what} costs {cost}, over its budget of {budget}"
    );
}

pub const BOOT_TIMEOUT_MS: u64 = 12_000;
pub const UI_FRAME_MS: u64 = 40;
const MID_FRAME_AGE_MS: u32 = 20;

pub fn run_past_boot(m: &mut Machine, fw: &Firmware) -> u64 {
    let boot = fw.symbol("boot_view");
    while m.ms() < BOOT_TIMEOUT_MS {
        let ui = fw.symbol("ui");
        let started = m.soc.read8(ui + SCREEN_DEPTH) != 0;
        if started && top_view(m, fw) != boot && !screen_animating(m, fw) {
            return m.ms();
        }
        m.run_ms(UI_FRAME_MS);
    }
    panic!("the boot was still on the screen {BOOT_TIMEOUT_MS} ms after power-on");
}

pub fn to_mid_frame(m: &mut Machine, fw: &Firmware) {
    let (last, now) = (fw.symbol("last_compose"), fw.symbol("uptime_ms"));
    for _ in 0..2 * UI_FRAME_MS {
        let age = m.soc.read32(now).wrapping_sub(m.soc.read32(last));
        if age == MID_FRAME_AGE_MS {
            return;
        }
        m.run_ms(1);
    }
    panic!("the UI's frames are not 40 ms apart");
}
