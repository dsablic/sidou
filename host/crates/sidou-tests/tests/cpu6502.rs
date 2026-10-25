// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's 6502 (core/cpu6502): Tom Harte's SingleStepTests (the committed subset, and the
//! full set with `--ignored` and SIDOU_HARTE_DIR), Klaus Dormann's functional test, slicing,
//! the 32-bit count's wrap, the stops, the I/O page's hooks and their cycle stamps.
//!
//! Three builds of the same file (sidou-sys's build.rs): the device's (CPU6502_SMALL), and the
//! tests' CPU6502_HOOK_ALL as the device builds it and with the I/O accesses inline, where
//! every operand access goes through the hooks so each one's cycle can be checked.

use std::collections::BTreeSet;
use std::ffi::{c_int, c_void};
use std::path::{Path, PathBuf};

use sidou_sys::{
    cpu6502, cpu6502_reset_regs, cpu6502_run, cpu6502_run_hook_all, cpu6502_run_hook_inline,
    CPU6502_JAM, CPU6502_RET, CPU6502_RUN,
};
use sidou_tests::harte::{self, Test};

type RunFn = unsafe extern "C" fn(*mut cpu6502, u32, c_int) -> c_int;

#[derive(Clone, Copy)]
struct Variant {
    name: &'static str,
    run: RunFn,
    /// Every operand access goes through the hooks (CPU6502_HOOK_ALL).
    hook_all: bool,
}

const DEVICE: Variant = Variant {
    name: "device",
    run: cpu6502_run,
    hook_all: false,
};
const HOOK_ALL: Variant = Variant {
    name: "hook_all",
    run: cpu6502_run_hook_all,
    hook_all: true,
};
const HOOK_INLINE: Variant = Variant {
    name: "hook_inline",
    run: cpu6502_run_hook_inline,
    hook_all: true,
};

/// One access through a hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Access {
    addr: u16,
    value: u8,
    cycle: u32,
    read: bool,
}

/// The memory behind the hooks: they read and write the same 64 KiB the core does, and log.
struct Bus {
    mem: Vec<u8>,
    log: Vec<Access>,
}

unsafe extern "C" fn hook_rd(user: *mut c_void, addr: u32, cycle: u32) -> u8 {
    // SAFETY: `user` is the Cpu's boxed Bus, alive for the run.
    let bus = unsafe { &mut *(user as *mut Bus) };
    let value = bus.mem[addr as usize];
    bus.log.push(Access {
        addr: addr as u16,
        value,
        cycle,
        read: true,
    });
    value
}

unsafe extern "C" fn hook_wr(user: *mut c_void, addr: u32, value: u32, cycle: u32) {
    // SAFETY: as hook_rd.
    let bus = unsafe { &mut *(user as *mut Bus) };
    assert!(value <= 0xff, "a hook's value is a byte: {value:#x}");
    bus.mem[addr as usize] = value as u8;
    bus.log.push(Access {
        addr: addr as u16,
        value: value as u8,
        cycle,
        read: false,
    });
}

/// PC, A, X, Y, S, P and the count.
type Regs = (u16, u8, u8, u8, u8, u8, u32);

struct Cpu {
    bus: Box<Bus>,
    c: cpu6502,
    run: RunFn,
}

impl Cpu {
    fn new(v: Variant) -> Self {
        let mut bus = Box::new(Bus {
            mem: vec![0; 0x10000],
            log: Vec::new(),
        });
        let c = cpu6502 {
            mem: bus.mem.as_mut_ptr(),
            rd: Some(hook_rd),
            wr: Some(hook_wr),
            user: &mut *bus as *mut Bus as *mut c_void,
            ..Default::default()
        };
        Cpu { bus, c, run: v.run }
    }

    fn run(&mut self, limit: u32, stops: bool) -> u32 {
        // SAFETY: `c.mem` is the boxed bus's 64 KiB and `c.user` the bus, both alive.
        unsafe { (self.run)(&mut self.c, limit, c_int::from(stops)) as u32 }
    }

    fn mem(&self) -> &[u8] {
        &self.bus.mem
    }

    fn mem_mut(&mut self) -> &mut [u8] {
        &mut self.bus.mem
    }

    fn regs(&self) -> Regs {
        let c = &self.c;
        (c.pc, c.a, c.x, c.y, c.s, c.p, c.cycles)
    }
}

// --- Tom Harte's tests ---------------------------------------------------------------------

const JAMS: [u8; 12] = [
    0x02, 0x12, 0x22, 0x32, 0x42, 0x52, 0x62, 0x72, 0x92, 0xb2, 0xd2, 0xf2,
];

#[derive(Default, Debug)]
struct Tally {
    tests: usize,
    state: usize,
    cycles: usize,
    jams: usize,
    stamped: usize,
}

/// Runs one test on a variant; the first failure is returned as text. `io` sets the device
/// variant's I/O page, so its hooked accesses (where $01 maps I/O in) are checked too.
fn harte_one(v: Variant, t: &Test, io: bool, base: u32, tally: &mut Tally) -> Result<(), String> {
    let mut cpu = Cpu::new(v);
    // $01 maps I/O in unless the test sets it, so the device variant hooks the I/O page.
    cpu.mem_mut()[1] = 0x37;
    for &(addr, value) in &t.initial.ram {
        cpu.mem_mut()[addr as usize] = value;
    }
    let i = &t.initial;
    cpu.c.pc = i.pc;
    cpu.c.s = i.s;
    cpu.c.a = i.a;
    cpu.c.x = i.x;
    cpu.c.y = i.y;
    cpu.c.p = i.p;
    cpu.c.io = u8::from(io);
    cpu.c.cycles = base;
    let op = cpu.mem()[i.pc as usize];
    let jam = JAMS.contains(&op);
    let state = cpu.run(base.wrapping_add(1), false);
    tally.tests += 1;

    let f = &t.fin;
    // A JAM leaves the PC on the opcode; Harte's final PC is the byte after it.
    let want_pc = if jam { f.pc.wrapping_sub(1) } else { f.pc };
    let c = &cpu.c;
    if (c.pc, c.s, c.a, c.x, c.y) != (want_pc, f.s, f.a, f.x, f.y) || (c.p ^ f.p) & 0xcf != 0 {
        return Err(format!(
            "registers: pc {:04x} s {:02x} a {:02x} x {:02x} y {:02x} p {:02x}, want {want_pc:04x} {:02x} {:02x} {:02x} {:02x} {:02x}",
            c.pc, c.s, c.a, c.x, c.y, c.p, f.s, f.a, f.x, f.y, f.p
        ));
    }
    if let Some(&(addr, want)) = f.ram.iter().find(|&&(a, w)| cpu.mem()[a as usize] != w) {
        return Err(format!(
            "memory ${addr:04x} = {:02x}, want {want:02x}",
            cpu.mem()[addr as usize]
        ));
    }
    tally.state += 1;
    if jam {
        if state != CPU6502_JAM || c.state as u32 != CPU6502_JAM {
            return Err(format!("JAM opcode: state {state}"));
        }
        tally.jams += 1;
        return Ok(());
    }
    if state != CPU6502_RUN {
        return Err(format!("state {state}"));
    }
    let took = c.cycles.wrapping_sub(base) as usize;
    if took != t.cycles.len() {
        return Err(format!("{took} cycles, want {}", t.cycles.len()));
    }
    tally.cycles += 1;

    // Each hooked access is the test's cycle at its stamp; with HOOK_ALL every write is one.
    let mut writes = BTreeSet::new();
    for a in &cpu.bus.log {
        let k = a.cycle.wrapping_sub(base) as usize;
        let want = t
            .cycles
            .get(k)
            .ok_or(format!("{a:?}: past the last cycle"))?;
        if (want.addr, want.value, want.read) != (a.addr, a.value, a.read) {
            return Err(format!("{a:?} at cycle {k}, the test has {want:?}"));
        }
        if !a.read && !writes.insert(k) {
            return Err(format!("two writes stamped at cycle {k}"));
        }
    }
    if v.hook_all {
        let want: BTreeSet<usize> = (0..t.cycles.len()).filter(|&k| !t.cycles[k].read).collect();
        if writes != want {
            return Err(format!(
                "writes at cycles {writes:?}, the test's at {want:?}"
            ));
        }
    }
    if !cpu.bus.log.is_empty() {
        tally.stamped += 1;
    }
    Ok(())
}

/// Runs a set of opcode files on a variant and returns its tally; panics with the failures.
fn harte_run(v: Variant, io: bool, files: &[(u8, Vec<Test>)]) -> Tally {
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    for (op, tests) in files {
        for (n, t) in tests.iter().enumerate() {
            // Half the tests start just before the count wraps: the stamps and the limit wrap.
            let base = if n % 2 == 0 { 0 } else { 0xffff_fffc };
            if let Err(e) = harte_one(v, t, io, base, &mut tally) {
                if failures.len() < 20 {
                    failures.push(format!(
                        "{}: opcode {op:02x} test {n} {}: {e}",
                        v.name, t.name
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures, the first:\n{}",
        failures.len(),
        failures.join("\n")
    );
    tally
}

fn subset() -> Vec<(u8, Vec<Test>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../third_party/6502-tests/harte");
    (0..=255u8)
        .map(|op| {
            let path = dir.join(format!("{op:02x}.bin"));
            let bin = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            (op, harte::from_bin(&bin).unwrap())
        })
        .collect()
}

#[test]
fn harte_subset_every_opcode() {
    let files = subset();
    assert!(files.iter().all(|(_, t)| t.len() == 100));
    for (v, io) in [
        (DEVICE, false),
        (DEVICE, true),
        (HOOK_ALL, true),
        (HOOK_INLINE, true),
    ] {
        let t = harte_run(v, io, &files);
        eprintln!("Harte subset, {} (io {io}): {t:?}", v.name);
        assert_eq!(t.tests, 25_600);
        assert_eq!(t.state, 25_600);
        assert_eq!(t.jams, 1_200);
        assert_eq!(t.cycles, 24_400);
        if v.hook_all {
            // Each of the 169 opcodes with a memory operand or a push, in every test.
            assert_eq!(t.stamped, 16_900, "{t:?}");
        } else if io {
            assert!(
                t.stamped > 0,
                "the I/O page was hooked in some tests: {t:?}"
            );
        }
    }
}

#[test]
#[ignore = "the full set: SIDOU_HARTE_DIR names a directory of Tom Harte's 6502 v1 JSON files"]
fn harte_full_set() {
    let dir = PathBuf::from(std::env::var("SIDOU_HARTE_DIR").expect("SIDOU_HARTE_DIR"));
    let mut totals: Vec<(Variant, bool, Tally)> = [
        (DEVICE, false),
        (DEVICE, true),
        (HOOK_ALL, true),
        (HOOK_INLINE, true),
    ]
    .into_iter()
    .map(|(v, io)| (v, io, Tally::default()))
    .collect();
    for op in 0..=255u8 {
        let path = dir.join(format!("{op:02x}.json"));
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let files = vec![(op, harte::parse_json(&text, usize::MAX).unwrap())];
        for (v, io, total) in &mut totals {
            let t = harte_run(*v, *io, &files);
            total.tests += t.tests;
            total.state += t.state;
            total.cycles += t.cycles;
            total.jams += t.jams;
            total.stamped += t.stamped;
        }
    }
    for (v, io, t) in &totals {
        eprintln!("Harte full set, {} (io {io}): {t:?}", v.name);
        assert_eq!(t.state, t.tests);
        assert_eq!(t.cycles + t.jams, t.tests);
    }
}

// --- Klaus Dormann's functional test --------------------------------------------------------

/// The test's "jmp *" on success (6502_functional_test.lst).
const SUCCESS: u16 = 0x3469;

fn dormann(v: Variant) -> Cpu {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../third_party/6502-tests/6502_functional_test.bin"
    );
    let bin = std::fs::read(path).expect("6502_functional_test.bin");
    assert_eq!(bin.len(), 0x10000);
    let mut cpu = Cpu::new(v);
    cpu.mem_mut().copy_from_slice(&bin);
    cpu_reset(&mut cpu);
    cpu.c.pc = 0x0400;
    cpu
}

fn cpu_reset(cpu: &mut Cpu) {
    // SAFETY: a plain struct update.
    unsafe { cpu6502_reset_regs(&mut cpu.c) };
}

/// A trap: the next instruction leaves the PC where it is (a jump or a taken branch to
/// itself). Stepping one instruction more changes nothing a slice would not.
fn trapped(cpu: &mut Cpu) -> bool {
    let pc = cpu.c.pc;
    cpu.run(cpu.c.cycles.wrapping_add(1), false);
    cpu.c.pc == pc
}

#[test]
fn dormanns_functional_test_passes_on_every_variant() {
    let mut done = Vec::new();
    for v in [DEVICE, HOOK_ALL, HOOK_INLINE] {
        let mut cpu = dormann(v);
        let mut chunks = 0;
        loop {
            let state = cpu.run(cpu.c.cycles.wrapping_add(100_000), false);
            assert_eq!(state, CPU6502_RUN, "{}: pc {:04x}", v.name, cpu.c.pc);
            if cpu.c.pc == SUCCESS {
                break;
            }
            assert!(
                !trapped(&mut cpu),
                "{}: trapped at ${:04x} (see the listing)",
                v.name,
                cpu.c.pc
            );
            chunks += 1;
            assert!(chunks < 2_000, "{}: no end", v.name);
        }
        // The success trap is reached within the last chunk; the count at the chunk's end.
        eprintln!(
            "Dormann's functional test, {}: success by cycle {}",
            v.name, cpu.c.cycles
        );
        done.push(cpu.c.cycles);
    }
    assert!(done.windows(2).all(|w| w[0] == w[1]), "{done:?}");
}

// --- Slicing and the wrap --------------------------------------------------------------------

const SLICED: u32 = 100_000;

/// Dormann's first SLICED cycles from `base`, in slices of `step` cycles (the whole at once
/// for 0): the registers, the count from base, a hash of the memory and the hooks' log.
fn sliced(v: Variant, base: u32, step: u32) -> (Regs, u64, Vec<Access>) {
    let mut cpu = dormann(v);
    cpu.c.cycles = base;
    let mut done = 0;
    while done < SLICED {
        let to = if step == 0 {
            SLICED
        } else {
            (done + step).min(SLICED)
        };
        assert_eq!(cpu.run(base.wrapping_add(to), false), CPU6502_RUN);
        done = cpu.c.cycles.wrapping_sub(base);
    }
    let mut r = cpu.regs();
    r.6 = r.6.wrapping_sub(base);
    let hash = cpu.mem().iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    });
    let log = cpu
        .bus
        .log
        .iter()
        .map(|a| Access {
            cycle: a.cycle.wrapping_sub(base),
            ..*a
        })
        .collect();
    (r, hash, log)
}

#[test]
fn slices_of_every_length_from_1_to_300_cycles_equal_one_run() {
    for v in [DEVICE, HOOK_ALL] {
        let whole = sliced(v, 0, 0);
        if v.hook_all {
            assert!(whole.2.len() > 1_000, "the stream is hooked");
        }
        for step in 1..=300 {
            assert!(sliced(v, 0, step) == whole, "{}: slices of {step}", v.name);
        }
    }
}

#[test]
fn a_run_across_the_counts_wrap_equals_one_from_zero() {
    for v in [DEVICE, HOOK_ALL] {
        let whole = sliced(v, 0, 0);
        // The wrap falls in the middle of the run, once at once and once in slices.
        let base = 0u32.wrapping_sub(SLICED / 2);
        assert!(sliced(v, base, 0) == whole, "{}: across the wrap", v.name);
        assert!(
            sliced(v, base, 97) == whole,
            "{}: across the wrap, sliced",
            v.name
        );
        // The count has wrapped: it is past zero now.
        let mut cpu = dormann(v);
        cpu.c.cycles = base;
        cpu.run(base.wrapping_add(SLICED), false);
        assert!(cpu.c.cycles >= SLICED / 2 && cpu.c.cycles < SLICED / 2 + 8);
    }
}

// --- The stops, the registers, JAM ----------------------------------------------------------

/// A CPU over memory full of NOPs, with `code` at $1000 and the PC on it.
fn nops(v: Variant, code: &[u8]) -> Cpu {
    let mut cpu = Cpu::new(v);
    cpu.mem_mut().fill(0xea);
    cpu.mem_mut()[0x1000..0x1000 + code.len()].copy_from_slice(code);
    cpu_reset(&mut cpu);
    cpu.c.pc = 0x1000;
    cpu
}

#[test]
fn the_sentinel_and_the_kernal_exits_stop_a_call_only_with_stops() {
    type Setup = fn(&mut Cpu);
    let cases: [(&str, &[u8], Setup, u16); 6] = [
        // RTS to the sentinel a call pushes ($0000, so $0001)
        (
            "RTS to the sentinel",
            &[0x60],
            |c| {
                c.c.s = 0xfd;
                c.mem_mut()[0x1fe..0x200].copy_from_slice(&[0x00, 0x00]);
            },
            0x0001,
        ),
        ("JMP $0000", &[0x4c, 0x00, 0x00], |_| {}, 0x0000),
        ("JMP $EA31", &[0x4c, 0x31, 0xea], |_| {}, 0xea31),
        (
            "JMP ($0314) to $EA81",
            &[0x6c, 0x14, 0x03],
            |c| c.mem_mut()[0x314..0x316].copy_from_slice(&[0x81, 0xea]),
            0xea81,
        ),
        (
            "RTI to $FEBC",
            &[0x40],
            |c| {
                c.c.s = 0xfc;
                c.mem_mut()[0x1fd..0x200].copy_from_slice(&[0x20, 0xbc, 0xfe]);
            },
            0xfebc,
        ),
        (
            "RTS to $EA31",
            &[0x60],
            |c| {
                c.c.s = 0xfd;
                c.mem_mut()[0x1fe..0x200].copy_from_slice(&[0x30, 0xea]);
            },
            0xea31,
        ),
    ];
    for v in [DEVICE, HOOK_ALL] {
        for (name, code, setup, to) in cases {
            let mut cpu = nops(v, code);
            setup(&mut cpu);
            assert_eq!(cpu.run(1_000, true), CPU6502_RET, "{name}");
            assert_eq!(cpu.c.pc, to, "{name}");
            assert_eq!(cpu.c.state as u32, CPU6502_RET, "{name}");

            let mut cpu = nops(v, code);
            setup(&mut cpu);
            assert_eq!(cpu.run(1_000, false), CPU6502_RUN, "{name}, no stops");
            assert!(cpu.c.cycles >= 1_000, "{name}: ran on through the NOPs");
        }
        // An ordinary return does not stop.
        let mut cpu = nops(v, &[0x4c, 0x00, 0x20]);
        assert_eq!(cpu.run(100, true), CPU6502_RUN);
    }
}

#[test]
fn reset_regs_sets_the_registers_a_call_starts_from() {
    let mut cpu = Cpu::new(DEVICE);
    cpu.c = cpu6502 {
        pc: 0x1234,
        a: 1,
        x: 2,
        y: 3,
        s: 4,
        p: 0xff,
        io: 1,
        state: CPU6502_JAM as u8,
        cycles: 777,
        ..cpu.c
    };
    cpu_reset(&mut cpu);
    let c = &cpu.c;
    assert_eq!((c.a, c.x, c.y, c.s, c.p), (0, 0, 0, 0xff, 0x24));
    assert_eq!(c.state as u32, CPU6502_RUN);
    assert_eq!((c.pc, c.io, c.cycles), (0x1234, 1, 777));
}

#[test]
fn a_jam_stops_on_its_opcode_and_stops_again() {
    for v in [DEVICE, HOOK_ALL] {
        for op in JAMS {
            let mut cpu = nops(v, &[0xea, op]);
            assert_eq!(cpu.run(1_000, false), CPU6502_JAM);
            assert_eq!((cpu.c.pc, cpu.c.cycles), (0x1001, 2));
            assert_eq!(cpu.run(1_000, false), CPU6502_JAM, "again");
            assert_eq!((cpu.c.pc, cpu.c.cycles), (0x1001, 2));
        }
    }
}

// --- The I/O page ---------------------------------------------------------------------------

#[test]
fn the_io_page_goes_to_the_hooks_only_while_io_is_set_and_01_maps_it_in() {
    // STA $D400; INC $D418 (a read-modify-write); LDA $D41B; then a JAM to stop.
    let code = [0x8d, 0x00, 0xd4, 0xee, 0x18, 0xd4, 0xad, 0x1b, 0xd4, 0x02];
    for (io, port, hooked) in [
        (1, 0x35, true),
        (1, 0x37, true),
        (1, 0x36, true),
        (1, 0x34, false), // all RAM
        (1, 0x31, false), // bit 2 clear: the character ROM, not I/O
        (0, 0x35, false),
    ] {
        let mut cpu = nops(DEVICE, &code);
        cpu.c.io = io;
        cpu.c.a = 0x5a;
        cpu.c.cycles = 1_000;
        cpu.mem_mut()[1] = port;
        cpu.mem_mut()[0xd418] = 0x0f;
        cpu.mem_mut()[0xd41b] = 0x77;
        assert_eq!(cpu.run(u32::MAX / 2, false), CPU6502_JAM);
        assert_eq!(cpu.c.cycles, 1_000 + 4 + 6 + 4);
        assert_eq!(cpu.c.a, 0x77);
        assert_eq!(cpu.mem()[0xd400], 0x5a);
        assert_eq!(cpu.mem()[0xd418], 0x10);
        let want: &[Access] = if hooked {
            &[
                Access {
                    addr: 0xd400,
                    value: 0x5a,
                    cycle: 1_003,
                    read: false,
                },
                // INC: the read on its third last cycle, the old value, then the new.
                Access {
                    addr: 0xd418,
                    value: 0x0f,
                    cycle: 1_007,
                    read: true,
                },
                Access {
                    addr: 0xd418,
                    value: 0x0f,
                    cycle: 1_008,
                    read: false,
                },
                Access {
                    addr: 0xd418,
                    value: 0x10,
                    cycle: 1_009,
                    read: false,
                },
                Access {
                    addr: 0xd41b,
                    value: 0x77,
                    cycle: 1_013,
                    read: true,
                },
            ]
        } else {
            &[]
        };
        assert_eq!(cpu.bus.log, want, "io {io}, $01 = {port:02x}");
    }
}

#[test]
fn the_converter_keeps_every_field_of_a_test() {
    let json = r#"[
      {"name": "20 c2 8f", "initial": {"pc": 21129, "s": 62, "a": 194, "x": 45, "y": 62, "p": 110,
        "ram": [[21129, 32], [21130, 194], [21131, 143], [318, 155], [36802, 200]]},
       "final": {"pc": 36802, "s": 60, "a": 194, "x": 45, "y": 62, "p": 110,
        "ram": [[317, 139], [318, 82], [21129, 32], [21130, 194], [21131, 143], [36802, 200]]},
       "cycles": [[21129, 32, "read"], [21130, 194, "read"], [318, 155, "read"],
        [318, 82, "write"], [317, 139, "write"], [21131, 143, "read"]]},
      {"name": "second", "initial": {"pc": 0, "s": 0, "a": 0, "x": 0, "y": 0, "p": 0, "ram": []},
       "final": {"pc": 1, "s": 0, "a": 0, "x": 0, "y": 0, "p": 0, "ram": []}, "cycles": []}
    ]"#;
    let tests = harte::parse_json(json, usize::MAX).unwrap();
    assert_eq!(tests.len(), 2);
    let t = &tests[0];
    assert_eq!(t.name, "20 c2 8f");
    assert_eq!((t.initial.pc, t.fin.pc, t.fin.s), (21129, 36802, 60));
    assert_eq!(t.fin.ram[1], (318, 82));
    assert_eq!(
        t.cycles[3],
        harte::Cycle {
            addr: 318,
            value: 82,
            read: false
        }
    );
    assert_eq!(harte::parse_json(json, 1).unwrap().len(), 1);
    // The binary keeps everything but the names.
    let back = harte::from_bin(&harte::to_bin(&tests)).unwrap();
    let unnamed: Vec<Test> = tests
        .into_iter()
        .map(|t| Test {
            name: String::new(),
            ..t
        })
        .collect();
    assert_eq!(back, unnamed);
    // JSON the converter does not take: an escape, a fraction, a 9-bit byte.
    assert!(harte::parse_json(r#"[{"name": "a\"b"}]"#, 1).is_err());
    assert!(harte::parse_json("[1.5]", 1).is_err());
    let wide = json.replacen("\"a\": 194", "\"a\": 256", 1);
    assert!(harte::parse_json(&wide, 1).is_err());
}
