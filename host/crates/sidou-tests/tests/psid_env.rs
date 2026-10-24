// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The PSID environment (M3 Task 6a; core/psid/psid_env.h, docs/reference/psid.md): loading,
//! the calls and their registers, the bank register, the period (frame, CIA latch, speed
//! bits), the clock, time and the scheduling of late calls, the limits, and slicing. Every
//! tune is a fixture built here (`sidou_tests::psid_fixtures`), and each test states the cycles
//! its fixture's instructions take.
use std::os::raw::c_void;

use sidou_sys::*;
use sidou_tests::psid_fixtures::*;
use sidou_tests::song::Rng;

const PAL: u32 = PSID_CLOCK_PAL;
const NTSC: u32 = PSID_CLOCK_NTSC;
const FRAME: u32 = PSID_FRAME_PAL;

/// A SID write: cycle, chip, register, value.
type Write = (u32, u32, u32, u32);

/// What a run reported, other than MORE and IDLE: the code, the cycle, and for a returned play
/// call its start and its interrupt's cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Event {
    code: i32,
    at: u32,
    start: u32,
    due: u32,
}

/// A loaded tune. Boxed, since the environment keeps pointers to the memory, the header and
/// the write log.
struct Tune {
    env: Box<psid_env>,
    mem: Box<[u8]>,
    header: Box<psid_header>,
    // Boxed so the sink's pointer to the Vec stays put when the Tune moves.
    #[allow(clippy::box_collection)]
    writes: Box<Vec<Write>>,
    // The chips whose voice shadows answer the tune's SID reads (`load_shadowed`), and the
    // shadows; the environment keeps pointers to them, so each is boxed.
    #[allow(clippy::vec_box)]
    chips: Vec<Box<sid>>,
    #[allow(clippy::vec_box)]
    shadows: Vec<Box<sid_shadow>>,
}

unsafe extern "C" fn sink(ctx: *mut c_void, cycle: u32, chip: u32, reg: u32, value: u32) {
    let w = unsafe { &mut *(ctx as *mut Vec<Write>) };
    w.push((cycle, chip, reg, value));
}

fn header(file: &[u8]) -> Box<psid_header> {
    let mut h = Box::<psid_header>::default();
    let r = unsafe { psid_parse(file.as_ptr(), file.len() as u32, &mut *h) };
    assert_eq!(r, PSID_OK as i32, "the fixture's header");
    h
}

impl Tune {
    fn load(p: &Psid) -> Tune {
        let file = p.bytes();
        let mut t = Tune {
            env: Box::default(),
            mem: vec![0xAAu8; 0x10000].into_boxed_slice(),
            header: header(&file),
            writes: Box::default(),
            chips: Vec::new(),
            shadows: Vec::new(),
        };
        unsafe {
            psid_env_load(&mut *t.env, t.mem.as_mut_ptr(), &*t.header, file.as_ptr());
            psid_env_set_sink(
                &mut *t.env,
                Some(sink),
                (&mut *t.writes as *mut Vec<Write>).cast(),
            );
        }
        t
    }

    /// A tune whose SID reads are answered by shadows of fresh chips of the given models
    /// (chip A's, and B's), started at cycle 0 as the player will.
    fn load_shadowed(p: &Psid, models: &[sid_model]) -> Tune {
        let mut t = Tune::load(p);
        for &m in models {
            let mut c = Box::<sid>::default();
            unsafe { sid_init(&mut *c, m, t.env.clock, 44_118) };
            let mut sh = Box::<sid_shadow>::default();
            unsafe { sid_shadow_init(&mut *sh, &*c, 0) };
            t.chips.push(c);
            t.shadows.push(sh);
        }
        let mut ptr = |i: usize| match t.shadows.get_mut(i) {
            Some(s) => &mut **s as *mut sid_shadow,
            None => std::ptr::null_mut(),
        };
        let (a, b) = (ptr(0), ptr(1));
        unsafe { psid_env_set_shadows(&mut *t.env, a, b) };
        t
    }

    /// The I/O page's hooks, as the 6502 calls them.
    fn rd(&mut self, addr: u32, cycle: u32) -> u8 {
        unsafe { (self.env.cpu.rd.unwrap())(self.env.cpu.user, addr, cycle) }
    }

    fn wr(&mut self, addr: u32, value: u32, cycle: u32) {
        unsafe { (self.env.cpu.wr.unwrap())(self.env.cpu.user, addr, value, cycle) }
    }

    fn start(&mut self, song: u32) -> i32 {
        unsafe { psid_env_start(&mut *self.env, song) }
    }

    fn run(&mut self, cycles: u32) -> i32 {
        unsafe { psid_env_run(&mut *self.env, cycles) }
    }

    fn now(&self) -> u32 {
        self.env.cpu.cycles
    }

    fn period(&self) -> u32 {
        unsafe { psid_env_period(&*self.env) }
    }

    /// Runs in slices of `slice` cycles until `until` or a stop: the events. MORE and IDLE are
    /// checked to be the only other results.
    fn play(&mut self, until: u32, slice: u32) -> Vec<Event> {
        let mut events = Vec::new();
        while (self.now().wrapping_sub(until) as i32) < 0 {
            let budget = slice.min(until.wrapping_sub(self.now()));
            let code = self.run(budget);
            match code as u32 {
                PSID_RUN_MORE | PSID_RUN_IDLE => {}
                PSID_RUN_INIT_DONE | PSID_RUN_CALL_DONE => events.push(Event {
                    code,
                    at: self.now(),
                    start: self.env.call_start,
                    due: self.env.call_due,
                }),
                _ => {
                    events.push(Event {
                        code,
                        at: self.now(),
                        start: self.env.call_start,
                        due: self.env.call_due,
                    });
                    break;
                }
            }
        }
        events
    }

    /// The play calls' starts, from the events.
    fn starts(events: &[Event]) -> Vec<u32> {
        events
            .iter()
            .filter(|e| e.code == PSID_RUN_CALL_DONE as i32)
            .map(|e| e.start)
            .collect()
    }

    /// The writes to one register, as (cycle, value).
    fn reg(&self, reg: u32) -> Vec<(u32, u32)> {
        self.writes
            .iter()
            .filter(|w| w.2 == reg)
            .map(|w| (w.0, w.3))
            .collect()
    }
}

fn init_done(at: u32) -> Event {
    Event {
        code: PSID_RUN_INIT_DONE as i32,
        at,
        start: 0,
        due: 0,
    }
}

fn call_done(start: u32, due: u32, cycles: u32) -> Event {
    Event {
        code: PSID_RUN_CALL_DONE as i32,
        at: start + cycles,
        start,
        due,
    }
}

#[test]
fn the_bank_register_follows_the_calls_address() {
    for (addr, bank) in [
        (0x0000, 0x37),
        (0x9FFF, 0x37),
        (0xA000, 0x36),
        (0xCFFF, 0x36),
        (0xD000, 0x34),
        (0xDFFF, 0x34),
        (0xE000, 0x35),
        (0xFFFF, 0x35),
    ] {
        assert_eq!(unsafe { psid_env_bank(addr) }, bank, "${addr:04X}");
    }
}

#[test]
fn load_puts_the_data_at_its_address_over_zeroed_memory() {
    let p = vbi();
    let t = Tune::load(&p);
    let load = p.load as usize;
    assert_eq!(&t.mem[load..load + p.data.len()], &p.data[..]);
    assert_eq!(t.mem[0], 0x2F, "the port's direction at power-on");
    assert_eq!(t.mem[1], 0x37, "the port at power-on");
    let rest = t.mem[2..load].iter().chain(&t.mem[load + p.data.len()..]);
    assert!(rest.copied().all(|b| b == 0), "the rest is zeroed");
    assert_eq!(t.env.phase, PSID_PHASE_NONE as u8);
    assert_eq!(t.env.clock, PAL);
}

#[test]
fn load_in_pieces_equals_load_whole_with_a_load_address_prefix() {
    // The header's load address 0: the data's first two bytes are the load address.
    let p = vbi();
    let mut file = p.bytes();
    let off = 0x7C;
    file[8] = 0;
    file[9] = 0;
    file.splice(off..off, p.load.to_le_bytes());
    let h = header(&file);
    assert_eq!(h.load, p.load);
    let mut whole = Box::<psid_env>::default();
    let mut mem_whole = vec![0u8; 0x10000];
    unsafe { psid_env_load(&mut *whole, mem_whole.as_mut_ptr(), &*h, file.as_ptr()) };
    assert_eq!(
        &mem_whole[p.load as usize..p.load as usize + p.data.len()],
        &p.data[..]
    );
    let mut rng = Rng(0x6a);
    for _ in 0..50 {
        let mut env = Box::<psid_env>::default();
        let mut mem = vec![0x55u8; 0x10000];
        unsafe { psid_env_load(&mut *env, mem.as_mut_ptr(), &*h, std::ptr::null()) };
        let mut at = 0usize;
        while at < file.len() {
            let n = (1 + rng.below(40) as usize).min(file.len() - at);
            unsafe { psid_env_load_part(&mut *env, at as u32, file[at..].as_ptr(), n as u32) };
            at += n;
        }
        // Pieces past the file's end and empty ones change nothing.
        unsafe {
            psid_env_load_part(&mut *env, file.len() as u32, file.as_ptr(), 9);
            psid_env_load_part(&mut *env, 0, file.as_ptr(), 0);
        }
        assert!(mem == mem_whole);
    }
}

#[test]
fn load_over_leaves_the_memory_to_the_caller_but_the_port() {
    // Task 10's loading: the caller clears the 64 KiB in parts and brings the data in.
    let file = vbi().bytes();
    let h = header(&file);
    let mut whole = Box::<psid_env>::default();
    let mut mem_whole = vec![0x33u8; 0x10000];
    unsafe { psid_env_load(&mut *whole, mem_whole.as_mut_ptr(), &*h, file.as_ptr()) };
    let mut env = Box::<psid_env>::default();
    let mut mem = vec![0x55u8; 0x10000];
    unsafe { psid_env_load_over(&mut *env, mem.as_mut_ptr(), &*h) };
    assert_eq!(&mem[..2], &mem_whole[..2], "the port as at power-on");
    assert!(mem[2..].iter().all(|&b| b == 0x55), "the rest is the caller's");
    mem[2..].fill(0);
    unsafe { psid_env_load_part(&mut *env, 0, file.as_ptr(), file.len() as u32) };
    assert!(mem == mem_whole);
    assert_eq!(env.clock, whole.clock);
    assert_eq!(env.frame, whole.frame);
}

#[test]
fn a_vbi_tune_plays_every_frame_from_the_first_boundary_after_init() {
    let mut t = Tune::load(&vbi());
    assert_eq!(t.start(1), 0);
    let events = t.play(5 * FRAME + 100, 1000);
    // Init: INIT_RECORD's writes on 3, 7, 11; LDA #, STA $D418 (14-17); RTS: returns at 24.
    let mut expect = vec![init_done(24)];
    // Play: BANK_RECORD 12, COUNT 12, RTS 6.
    for k in 1..=5 {
        expect.push(call_done(k * FRAME, k * FRAME, 30));
    }
    assert_eq!(events, expect);
    assert_eq!(
        t.writes[..4],
        [
            (3, 0, 2, 0),
            (7, 0, 3, 0),
            (11, 0, 4, 0),
            (17, 0, 0x18, 0x0F)
        ]
    );
    let banks: Vec<(u32, u32)> = (1..=5).map(|k| (k * FRAME + 11, 0x37)).collect();
    assert_eq!(t.reg(1), banks, "$01 at each play call's start");
    let counts: Vec<(u32, u32)> = (1..=5).map(|k| (k * FRAME + 23, k)).collect();
    assert_eq!(t.reg(0), counts);
    assert_eq!(t.env.calls, 5);
    assert_eq!(t.env.overruns, 0);
    assert_eq!(t.period(), FRAME);
}

#[test]
fn init_gets_the_song_in_a_and_play_calls_start_from_the_same_registers() {
    let mut t = Tune::load(&forty_songs());
    for song in [1u32, 2, 33, 40] {
        t.writes.clear();
        assert_eq!(t.start(song), 0);
        t.play(10, 10);
        assert_eq!(
            t.writes[..3],
            [(3, 0, 2, song - 1), (7, 0, 3, 0), (11, 0, 4, 0)],
            "song {song}"
        );
    }
    // Song 0 is the header's start song; one past the last is refused, and changes nothing.
    assert_eq!(t.start(0), 0);
    assert_eq!(t.env.song, 1);
    assert_eq!(t.start(41), -1);
    assert_eq!(t.env.song, 1);
}

#[test]
fn a_cia_tune_takes_its_period_from_the_latch_init_sets() {
    let mut t = Tune::load(&cia(0x2000));
    t.start(1);
    // Init: INIT_RECORD 12, two loads and stores 12, RTS 6.
    let events = t.play(5 * 0x2001 + 100, 1500);
    assert_eq!(events[0], init_done(30));
    assert_eq!(Tune::starts(&events), [1, 2, 3, 4, 5].map(|k| k * 0x2001));
    assert_eq!(t.period(), 0x2001);
}

#[test]
fn the_latch_starts_at_the_kernals_value_for_the_clock() {
    for (clock, period) in [(CLOCK_PAL, 0x4026), (CLOCK_NTSC, 0x4296)] {
        let mut t = Tune::load(&cia_default().flags(clock));
        t.start(1);
        let events = t.play(3 * period + 40, 700);
        assert_eq!(Tune::starts(&events), [period, 2 * period, 3 * period]);
    }
}

#[test]
fn a_latch_changed_by_a_play_call_sets_the_next_period() {
    let mut t = Tune::load(&cia_changing());
    t.start(1);
    let events = t.play(40_000, 333);
    // Init sets $1000: the first call at 4,097. Call n sets n * $100 + $FF: the next call
    // (n + 1) * $100 later.
    let mut due = 0x1001;
    let mut expect = Vec::new();
    for n in 1..=8u32 {
        expect.push(due);
        due += (n + 1) * 0x100;
    }
    assert_eq!(Tune::starts(&events)[..8], expect[..]);
    let returned = Tune::starts(&events).len() as u32;
    assert_eq!(
        t.env.latch as u32,
        returned * 0x100 + 0xFF,
        "the last call's latch"
    );
}

#[test]
fn short_cia_periods_are_honoured() {
    // FM-1 SID replaces periods under 1,000 cycles with the frame; the C64 does not.
    for latch in [998u16, 99] {
        let period = latch as u32 + 1;
        let mut t = Tune::load(&cia(latch));
        t.start(1);
        let events = t.play(20 * period + 40, 64);
        let starts = Tune::starts(&events);
        assert_eq!(starts.len(), 20, "latch {latch}");
        for (k, s) in starts.iter().enumerate() {
            assert_eq!(*s, (k as u32 + 1) * period, "latch {latch}");
        }
        assert_eq!(t.env.overruns, 0);
    }
}

/// The period song `song` plays at, from its first two calls.
fn song_period(p: &Psid, song: u32) -> u32 {
    let mut t = Tune::load(p);
    assert_eq!(t.start(song), 0);
    let events = t.play(3 * FRAME, 5000);
    let s = Tune::starts(&events);
    assert_eq!(t.period(), s[1] - s[0], "song {song}");
    s[1] - s[0]
}

#[test]
fn the_speed_bits_pick_frame_or_cia_per_song_as_the_format_says() {
    const CIA: u32 = 10_000;
    // v2, PlaySID flag clear: songs above 32 take bit 31.
    let p = forty_songs();
    for (song, period) in [
        (1, CIA),
        (2, FRAME),
        (31, FRAME),
        (32, CIA),
        (33, CIA),
        (34, CIA),
        (40, CIA),
    ] {
        assert_eq!(song_period(&p, song), period, "v2, song {song}");
    }
    // The PlaySID flag, and v1: the bits repeat.
    let mut v1 = forty_songs();
    v1.version = 1;
    for p in [forty_songs().flags(CLOCK_PAL | FLAG_PLAYSID), v1] {
        for (song, period) in [(32, CIA), (33, CIA), (34, FRAME), (40, FRAME)] {
            assert_eq!(
                song_period(&p, song),
                period,
                "v{} flags {:x}, song {song}",
                p.version,
                p.flags
            );
        }
    }
}

#[test]
fn play_address_zero_goes_through_the_kernals_vector_with_its_stack() {
    for rti in [false, true] {
        let mut t = Tune::load(&irq_kernal(rti));
        t.start(1);
        let events = t.play(3 * FRAME + 100, 1000);
        // Init: INIT_RECORD 12, two loads and stores 12, RTS 6. The handler: TSX, STX 6,
        // IRQ_BANK_RECORD 7, COUNT 12, then JMP $EA31 (3) or the KERNAL exit's six (22).
        let cycles = if rti { 47 } else { 28 };
        let mut expect = vec![init_done(30)];
        for k in 1..=3 {
            expect.push(call_done(k * FRAME, k * FRAME, cycles));
        }
        assert_eq!(events, expect, "rti {rti}");
        // S after the sentinel, P, and the KERNAL entry's A, X and Y.
        let s: Vec<u32> = t.reg(6).iter().map(|w| w.1).collect();
        assert_eq!(s, [0xF9; 3]);
        // $01 as the tune left it (init's $37), not set by the call.
        let banks: Vec<(u32, u32)> = (1..=3).map(|k| (k * FRAME + 12, 0x37)).collect();
        assert_eq!(t.reg(1), banks);
    }
}

#[test]
fn play_address_zero_with_the_kernal_out_goes_through_ffff_and_rti() {
    let mut t = Tune::load(&irq_hardware());
    t.start(1);
    let events = t.play(3 * FRAME + 100, 1000);
    // Init: INIT_RECORD 12, LDA #, STA zp 5, two loads and stores 12, RTS 6.
    let mut expect = vec![init_done(35)];
    // The handler: TSX, STX 6, IRQ_BANK_RECORD 7, COUNT 12, RTI 6.
    for k in 1..=3 {
        expect.push(call_done(k * FRAME, k * FRAME, 31));
    }
    assert_eq!(events, expect);
    let s: Vec<u32> = t.reg(6).iter().map(|w| w.1).collect();
    assert_eq!(s, [0xFC; 3], "S after the sentinel and P");
    let banks: Vec<u32> = t.reg(1).iter().map(|w| w.1).collect();
    assert_eq!(banks, [0x35; 3]);
}

#[test]
fn every_call_starts_with_the_bank_register_for_its_address() {
    for (init, play, init_bank, play_bank) in [
        (0xA000u16, 0xE000u16, 0x36u32, 0x35u32),
        (0x1000, 0xD000, 0x37, 0x34),
        (0xC000, 0xDF00, 0x36, 0x34),
        (0x9F00, 0xFF00, 0x37, 0x35),
    ] {
        let mut t = Tune::load(&banks(init, play));
        t.start(1);
        t.play(3 * FRAME + 100, 2000);
        // Each call records $01 and leaves it at $35; the next call sets it again.
        let got: Vec<u32> = t.reg(1).iter().map(|w| w.1).collect();
        assert_eq!(
            got,
            [init_bank, play_bank, play_bank, play_bank],
            "init ${init:04X}, play ${play:04X}"
        );
    }
}

#[test]
fn a_two_second_init_runs_in_slices_and_play_starts_at_the_next_boundary() {
    let mut t = Tune::load(&slow_init());
    t.start(1);
    let events = t.play(103 * FRAME, 1500);
    // LDA #, STA zp 5; six rounds of 256 delays (1,286 cycles each, + 1), DEC zp 5 and BNE 3,
    // the last BNE not taken; LDA #, STA $D418 6; RTS 6.
    let end = 5 + 6 * (256 * 1286 + 1 + 8) - 1 + 6 + 6;
    assert_eq!(end, 1_975_366);
    assert_eq!(events[0], init_done(end));
    assert_eq!(t.writes[0], (end - 7, 0, 0x18, 0x0F));
    // The first play call at the first frame boundary after init's return.
    assert_eq!(Tune::starts(&events), [101 * FRAME, 102 * FRAME]);
}

#[test]
fn an_init_that_never_returns_stops_after_five_seconds() {
    for (flags, clock) in [(CLOCK_PAL, PAL), (CLOCK_NTSC, NTSC)] {
        let mut t = Tune::load(&init_forever().flags(flags));
        t.start(1);
        let events = t.play(6 * clock, 40_000);
        // JMP is 3 cycles, and 5 s of either clock is a multiple of 3.
        assert_eq!(
            events,
            [Event {
                code: PSID_RUN_INIT_TOO_LONG as i32,
                at: 5 * clock,
                start: 0,
                due: 0
            }]
        );
        assert_eq!(
            t.run(1000),
            PSID_RUN_INIT_TOO_LONG as i32,
            "it stays stopped"
        );
        assert_eq!(t.now(), 5 * clock);
        assert_eq!(t.env.calls, 0);
    }
}

#[test]
fn an_overrun_is_counted_and_the_next_call_starts_at_its_return() {
    let mut t = Tune::load(&play_overrun(23));
    t.start(1);
    let events = t.play(5 * FRAME - 1, 1500);
    // Init: RTS, 6. The first call: BANK_RECORD 12, LDA zp 3, BNE 2, 23 delays 29,579, COUNT 12,
    // RTS 6; it misses the interrupt at 2 frames, which is taken at its return. The rest:
    // BANK_RECORD 12, LDA zp 3, BNE 3, COUNT 12, RTS 6, each on the grid.
    let first = 12 + 3 + 2 + 23 * 1286 + 1 + 12 + 6;
    let ret = FRAME + first;
    assert_eq!(
        events,
        [
            init_done(6),
            call_done(FRAME, FRAME, first),
            call_done(ret, 2 * FRAME, 36),
            call_done(3 * FRAME, 3 * FRAME, 36),
            call_done(4 * FRAME, 4 * FRAME, 36),
        ]
    );
    assert_eq!(t.env.overruns, 1);
    assert_eq!(t.env.calls, 4);
}

#[test]
fn interrupts_missed_by_a_long_overrun_are_one_pending_interrupt() {
    // 61 delays: 78,447 cycles, nearly four frames; the interrupts at 2, 3 and 4 frames are
    // one, taken at the return, and the next comes at 5 frames.
    let mut t = Tune::load(&play_overrun(61));
    t.start(1);
    let events = t.play(6 * FRAME, 1500);
    let first = 12 + 3 + 2 + 61 * 1286 + 1 + 12 + 6;
    let ret = FRAME + first;
    assert!(ret > 4 * FRAME && ret < 5 * FRAME);
    assert_eq!(
        events,
        [
            init_done(6),
            call_done(FRAME, FRAME, first),
            call_done(ret, 4 * FRAME, 36),
            call_done(5 * FRAME, 5 * FRAME, 36),
        ]
    );
    assert_eq!(t.env.overruns, 1);
}

#[test]
fn a_play_call_that_never_returns_stops_after_a_second() {
    let mut t = Tune::load(&play_forever());
    t.start(1);
    let events = t.play(5 * PAL, 40_000);
    let last = *events.last().unwrap();
    assert_eq!(Tune::starts(&events), [FRAME, 2 * FRAME]);
    // The third call: BANK_RECORD 12, COUNT 12, CMP # 2, BNE 2, then JMP forever from cycle 28.
    // A second later is not on a JMP's boundary: the stop is at the first one past it.
    let start = 3 * FRAME;
    let stop = start + 28 + (PAL - 28).div_ceil(3) * 3;
    assert_eq!(
        last,
        Event {
            code: PSID_RUN_PLAY_HUNG as i32,
            at: stop,
            start,
            due: start
        }
    );
    assert_eq!(t.env.calls, 3);
    assert_eq!(t.run(1000), PSID_RUN_PLAY_HUNG as i32);
}

#[test]
fn a_jam_stops_the_tune_with_the_pc_on_the_opcode() {
    let mut t = Tune::load(&jam());
    t.start(1);
    let events = t.play(5 * FRAME, 1000);
    assert_eq!(events.last().unwrap().code, PSID_RUN_JAM as i32);
    assert_eq!(t.env.cpu.pc, PLAY + 20);
    assert_eq!(t.env.calls, 2);
    assert_eq!(t.run(1000), PSID_RUN_JAM as i32);
}

#[test]
fn ntsc_tunes_play_at_the_ntsc_clock_and_frame() {
    for (flags, clock, frame) in [
        (0, PAL, FRAME),
        (CLOCK_PAL, PAL, FRAME),
        (CLOCK_NTSC, NTSC, PSID_FRAME_NTSC),
        (CLOCK_PAL | CLOCK_NTSC, PAL, FRAME),
    ] {
        let mut t = Tune::load(&vbi().flags(flags));
        assert_eq!(t.env.clock, clock);
        assert_eq!(t.env.frame as u32, frame);
        t.start(1);
        let events = t.play(3 * frame + 50, 1000);
        assert_eq!(Tune::starts(&events), [frame, 2 * frame, 3 * frame]);
    }
    assert_eq!(PSID_FRAME_NTSC, 263 * 65);
    assert_eq!(PSID_FRAME_PAL, 312 * 63);
}

#[test]
fn idle_time_passes_up_to_the_due_call() {
    let mut t = Tune::load(&vbi());
    t.start(1);
    assert_eq!(t.run(1000), PSID_RUN_INIT_DONE as i32);
    assert_eq!(t.now(), 24);
    assert_eq!(unsafe { psid_env_due(&*t.env) }, FRAME);
    assert_eq!(t.run(1000), PSID_RUN_IDLE as i32);
    assert_eq!(t.now(), 1024);
    assert_eq!(t.run(FRAME - 1024 - 1), PSID_RUN_IDLE as i32);
    assert_eq!(t.now(), FRAME - 1);
    // A budget that ends on the due cycle starts the call and runs none of it.
    assert_eq!(t.run(1), PSID_RUN_MORE as i32);
    assert_eq!(t.now(), FRAME);
    assert_eq!(t.env.call_start, FRAME);
    assert_eq!(t.env.phase, PSID_PHASE_PLAY as u8);
    // A budget across the due cycle: the rest runs in the same run.
    assert_eq!(t.run(10), PSID_RUN_MORE as i32);
    assert_eq!(t.run(100), PSID_RUN_CALL_DONE as i32);
    assert_eq!(t.now(), FRAME + 30);
    assert_eq!(t.run(2 * FRAME), PSID_RUN_CALL_DONE as i32);
    assert_eq!(
        t.env.call_start,
        2 * FRAME,
        "the next call starts at its due"
    );
    assert_eq!(t.now(), 2 * FRAME + 30);
}

#[test]
fn nothing_runs_before_a_load_and_a_start() {
    let mut env = Box::<psid_env>::default();
    assert_eq!(
        unsafe { psid_env_run(&mut *env, 100) },
        PSID_RUN_NOT_STARTED as i32
    );
    assert_eq!(unsafe { psid_env_start(&mut *env, 1) }, -1);
    let mut t = Tune::load(&vbi());
    assert_eq!(t.run(100), PSID_RUN_NOT_STARTED as i32);
    assert_eq!(t.now(), 0);
}

#[test]
fn a_call_to_a_stop_address_returns_at_once() {
    // Play address 0 through a $0314 the tune never set: the call goes to $0000, the
    // sentinel, and ends there; nothing runs from zeroed memory.
    let mut t = Tune::load(&vbi().play(0));
    t.start(1);
    let events = t.play(3 * FRAME - 1, 1000);
    assert_eq!(
        Tune::starts(&events),
        [FRAME, 2 * FRAME],
        "two calls of 0 cycles"
    );
    assert!(events[1..].iter().all(|e| e.at == e.start));
}

#[test]
fn slicing_init_at_every_size_from_1_to_5000_changes_nothing() {
    let p = busy_init();
    let until = 120_000;
    let mut whole = Tune::load(&p);
    whole.start(1);
    let reference = whole.play(until, until);
    assert_eq!(whole.writes.len(), 3 + 256 + 2 * 5);
    assert_eq!(Tune::starts(&reference).len(), 5);
    for slice in 1..=5000 {
        // Loaded again each time: init and play keep state in the tune's memory.
        let mut t = Tune::load(&p);
        t.start(1);
        let events = t.play(until, slice);
        assert_eq!(events, reference, "slice {slice}");
        assert_eq!(t.writes.len(), whole.writes.len(), "slice {slice}");
        for (i, (a, b)) in t.writes.iter().zip(whole.writes.iter()).enumerate() {
            assert_eq!(a, b, "slice {slice}, write {i}");
        }
    }
}

#[test]
fn time_wraps_past_two_to_the_32() {
    // The same tunes with every time shifted to just before the count wraps, after init: the
    // same calls, shifted.
    for p in [vbi(), play_overrun(23), cia(998)] {
        let mut a = Tune::load(&p);
        let mut b = Tune::load(&p);
        a.start(1);
        b.start(1);
        assert_eq!(a.run(1 << 20), PSID_RUN_INIT_DONE as i32);
        assert_eq!(b.run(1 << 20), PSID_RUN_INIT_DONE as i32);
        let shift = 0u32.wrapping_sub(3 * FRAME + 77);
        b.env.cpu.cycles = b.env.cpu.cycles.wrapping_add(shift);
        b.env.next_due = b.env.next_due.wrapping_add(shift);
        b.env.next_grid = b.env.next_grid.wrapping_add(shift);
        b.env.call_start = b.env.call_start.wrapping_add(shift);
        let ea = a.play(8 * FRAME, 1500);
        let eb = b.play((8 * FRAME).wrapping_add(shift), 1500);
        let back: Vec<Event> = eb
            .iter()
            .map(|e| Event {
                code: e.code,
                at: e.at.wrapping_sub(shift),
                start: e.start.wrapping_sub(shift),
                due: e.due.wrapping_sub(shift),
            })
            .collect();
        assert_eq!(back, ea);
        assert!(ea.len() > 4);
        assert!(b.now() < 8 * FRAME, "b's count wrapped");
        assert_eq!(a.env.overruns, b.env.overruns);
    }
}

#[test]
fn the_environment_fits_its_share_of_the_overlay() {
    // The plan's "Memory and flash": the environment is about 400 bytes of the main loop's
    // overlay with the I/O page's shadows. As the pi32v2 lays it out (ILP32): Task 6a's 92
    // bytes, the two shadow pointers, the VIC's 64 bytes, the CIAs' 32 and two flags; Task 8's
    // count of SID accesses.
    assert_eq!(ilp32::psid_env::SIZE, 200);
    assert_eq!(ilp32::cpu6502::SIZE, 32);
}

// --- The I/O page (Task 6b) -----------------------------------------------------------------

const M6581: sid_model = SID_MODEL_6581;
const M8580: sid_model = SID_MODEL_8580;

/// Runs init (to its return or stop) in one go, and returns what it reported.
fn run_init(t: &mut Tune) -> i32 {
    assert_eq!(t.start(1), 0);
    t.run(6_000_000)
}

/// (chip, register, value) of every SID write, in order.
fn sid_writes(t: &Tune) -> Vec<(u32, u32, u32)> {
    t.writes.iter().map(|w| (w.1, w.2, w.3)).collect()
}

#[test]
fn exact_delays_take_exactly_their_cycles() {
    for n in (0..40).chain([
        100, 1285, 1286, 1287, 1288, 1293, 1294, 1299, 1300, 1301, 1302, 1303, 1294, 2573, 2574,
        2575, 2580, 3000, 20_000, 100_000, 329_000,
    ]) {
        if n == 1 {
            continue;
        }
        let p = Psid::new(&[(INIT, &format!("{}rts\n", exact(n)))]);
        let mut t = Tune::load(&p);
        assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32, "n = {n}");
        assert_eq!(t.now(), n + 6, "n = {n}");
    }
}

#[test]
fn the_sid_windows_are_chip_a_with_its_mirrors() {
    // $D400-$D7FF, every 32 bytes; the VIC, colour RAM and CIA 1 are not the SID.
    let p = stores(&[
        0xD400, 0xD41F, 0xD420, 0xD480, 0xD7E0, 0xD7FF, 0xD3FF, 0xD800, 0xDC00, 0xDE00,
    ]);
    let mut t = Tune::load(&p);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    assert_eq!(
        sid_writes(&t),
        [
            (0, 0, 1),
            (0, 31, 2),
            (0, 0, 3),
            (0, 0, 4),
            (0, 0, 5),
            (0, 31, 6)
        ]
    );
}

#[test]
fn a_second_sid_window_is_chip_b_and_takes_priority() {
    // At $D420: chip A's mirrors resume at $D440.
    let p = stores(&[0xD400, 0xD41F, 0xD420, 0xD43F, 0xD440, 0xD7E0]).sid2(0x42);
    let mut t = Tune::load(&p);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    assert_eq!(
        sid_writes(&t),
        [
            (0, 0, 1),
            (0, 31, 2),
            (1, 0, 3),
            (1, 31, 4),
            (0, 0, 5),
            (0, 0, 6)
        ]
    );
    // At $DE00, outside the SID range, and the last window in it, $D7E0.
    let p = stores(&[0xD400, 0xDE00, 0xDE1F, 0xDE20, 0xDF00]).sid2(0xE0);
    let mut t = Tune::load(&p);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    assert_eq!(sid_writes(&t), [(0, 0, 1), (1, 0, 2), (1, 31, 3)]);
    let p = stores(&[0xD7C0, 0xD7DF, 0xD7E0, 0xD7FF]).sid2(0x7E);
    let mut t = Tune::load(&p);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    assert_eq!(
        sid_writes(&t),
        [(0, 0, 1), (0, 31, 2), (1, 0, 3), (1, 31, 4)]
    );
}

#[test]
fn a_third_sids_writes_are_dropped_and_counted_and_its_reads_are_zero() {
    let p = stores(&[0xD400, 0xD420, 0xDE00, 0xDE01, 0xD440])
        .sid2(0x42)
        .sid3(0xE0);
    let mut t = Tune::load_shadowed(&p, &[M6581, M6581]);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    assert_eq!(sid_writes(&t), [(0, 0, 1), (1, 0, 2), (0, 0, 5)]);
    assert_eq!(t.env.sid3_dropped, 2);
    // Its reads: nothing there. A restart clears the count.
    assert_eq!(t.rd(0xDE05, 100), 0);
    assert_eq!(t.start(1), 0);
    assert_eq!(t.env.sid3_dropped, 0);
    // A third SID inside the SID range, with the second elsewhere.
    let p = stores(&[0xD440, 0xD460]).sid2(0x44).sid3(0x46);
    let mut t = Tune::load(&p);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    assert!(t.writes.is_empty() || sid_writes(&t).iter().all(|w| w.0 != 2));
    assert_eq!(t.env.sid3_dropped, 1);
    assert_eq!(sid_writes(&t), [(1, 0, 1)]);
}

/// Replays a tune's SID writes on a chip, clocking it to each write's cycle; `at_write` is
/// called before each, with the chip clocked to the cycle `back` before it (a read's).
fn replay(chip: &mut sid, writes: &[Write], back: u32, mut before: impl FnMut(&mut sid, usize)) {
    let mut now = 0u32;
    for (i, w) in writes.iter().enumerate() {
        let at = w.0.wrapping_sub(back);
        if (at.wrapping_sub(now) as i32) > 0 {
            unsafe { sid_clock(chip, at - now) };
            now = at;
        }
        before(chip, i);
        let c = w.0;
        if (c.wrapping_sub(now) as i32) > 0 {
            unsafe { sid_clock(chip, c - now) };
            now = c;
        }
        if w.1 == 0 {
            unsafe { sid_write(chip, w.2 as u8, w.3 as u8) };
        }
    }
}

#[test]
fn osc3_reads_are_the_chips_at_the_reads_cycle() {
    for model in [M6581, M8580] {
        let p = osc3_noise();
        let mut t = Tune::load_shadowed(&p, &[model]);
        assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
        // The eight reads are the writes to register 0, each 4 cycles after its read.
        let reads: Vec<(u32, u32)> = t.reg(0);
        assert_eq!(reads.len(), 8);
        let values: std::collections::BTreeSet<u32> = reads.iter().map(|r| r.1).collect();
        assert!(values.len() >= 6, "noise: {reads:?}");
        let mut chip = Box::<sid>::default();
        unsafe { sid_init(&mut *chip, model, PAL, 44_118) };
        let writes = t.writes.clone();
        replay(&mut chip, &writes, 4, |c, i| {
            let w = writes[i];
            if w.2 == 0 {
                let theirs = unsafe { sid_read(c, 0x1b) };
                assert_eq!(u32::from(theirs), w.3, "write {i} at {}", w.0);
            }
        });
    }
}

#[test]
fn an_envelope_wait_ends_when_the_chips_envelope_gets_there() {
    // Without real reads the loop never ends (ENV3 reads 0, so the first wait stays).
    let p = env3_wait();
    let mut t = Tune::load(&p);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_TOO_LONG as i32);
    for model in [M6581, M8580] {
        let mut t = Tune::load_shadowed(&p, &[model]);
        assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32, "model {model}");
        let (marker, gate) = (*t.writes.last().unwrap(), t.writes[t.writes.len() - 2]);
        assert_eq!((marker.2, gate.2), (0, 0x12));
        // The reference chip, given the same writes at the same cycles and stepped a cycle at
        // a time: ENV3 rises, then first reads 0 again at cycle t2.
        let mut chip = Box::<sid>::default();
        unsafe { sid_init(&mut *chip, model, PAL, 44_118) };
        let mut now = 0u32;
        for w in &t.writes[..t.writes.len() - 1] {
            unsafe { sid_clock(&mut *chip, w.0 - now) };
            now = w.0;
            unsafe { sid_write(&mut *chip, w.2 as u8, w.3 as u8) };
        }
        let (mut rose, mut t2) = (false, 0);
        for _ in 0..200_000 {
            unsafe { sid_clock(&mut *chip, 1) };
            now += 1;
            let e = unsafe { sid_read(&mut *chip, 0x1c) };
            if e != 0 {
                rose = true;
            } else if rose {
                t2 = now;
                break;
            }
        }
        assert!(t2 > 0, "the chip's envelope never fell, model {model}");
        // The loop reads every 7 cycles; the last read, which saw 0, is 6 before the write.
        let r = marker.0 - 6;
        assert!(
            r - 7 < t2 && t2 <= r,
            "read at {r}, chip's ENV3 at 0 from {t2}"
        );
    }
}

#[test]
fn the_raster_line_comes_from_the_absolute_cycle() {
    // The first read of a line is in its first loop's 9 cycles; the write is 8 after it.
    for (ntsc, line, per) in [(false, 0x80u32, 63u32), (true, 0x60, 65)] {
        let mut p = raster_wait(line as u8);
        if ntsc {
            p = p.flags(CLOCK_NTSC);
        }
        let mut t = Tune::load(&p);
        assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
        let r = t.writes[0].0 - 8;
        assert!(
            r >= line * per && r < line * per + 9,
            "read at {r}, ntsc {ntsc}"
        );
    }
    // $D011's bit 7, line 256 and on.
    let p = raster_high_wait();
    let mut t = Tune::load(&p);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    let r = t.writes[0].0 - 6;
    assert!((256 * 63..256 * 63 + 7).contains(&r), "read at {r}");
}

#[test]
fn the_raster_of_every_call_is_the_absolute_one() {
    // A CIA song of 10,000 cycles: the calls start at different places in the frame, where
    // FM-1 SID's raster, from each call's start, reads 0.
    let p = raster_in_play(9_999);
    let mut t = Tune::load(&p);
    assert_eq!(t.start(1), 0);
    t.play(8 * 10_000, 977);
    let w: Vec<(u32, u32)> = t.reg(0);
    assert!(w.len() >= 6);
    for &(at, v) in &w {
        assert_eq!(v, (at - 4) % FRAME / 63, "read at {}", at - 4);
    }
    assert!(
        w.iter()
            .map(|x| x.1)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 4
    );
}

#[test]
fn cia_timer_a_counts_down_from_the_latch_from_its_underflow() {
    let p = cia_counter();
    let mut t = Tune::load(&p);
    assert_eq!(t.start(1), 0);
    let events = t.play(8 * 2_305, 311);
    // Init: the latch $0900 is set by cycle 12, and the counter (a period of 2,305 from cycle
    // 0) read at 15 and 23 (a load's last cycle): 2,304 - 15 and 2,304 - 23.
    assert_eq!(t.reg(0)[0], (19, 0xF1));
    assert_eq!(t.reg(1)[0], (27, 0x08));
    // Every play call starts on an underflow, so its reads at +3 and +11 see 2,301 and 2,293
    // (FM-1 SID would return the latch, $0900, itself).
    let calls = Tune::starts(&events).len();
    assert!(calls >= 5);
    assert_eq!(t.reg(2).len(), calls);
    for &(_, v) in &t.reg(2) {
        assert_eq!(v, 0x08);
    }
    for &(_, v) in &t.reg(3) {
        assert_eq!(v, 0xF5);
    }
}

#[test]
fn a_vbi_calls_start_sets_the_vic_flag_and_a_cia_calls_the_cias() {
    for cia in [false, true] {
        let p = irq_flags(cia);
        let mut t = Tune::load(&p);
        assert_eq!(t.start(1), 0);
        let events = t.play(5 * FRAME, 997);
        let calls = Tune::starts(&events).len();
        assert!(calls >= 3);
        for r in 0..3 {
            assert_eq!(t.reg(r)[0].1, 0, "init reads, register {r}");
        }
        // $D019 / $DC0D by the kind of call; the write of $D019 and the read of $DC0D
        // acknowledge, the other kind of flag is not there, and $DD0D is 0.
        let (first, then) = if cia { (0, 0x81) } else { (0x81, 0) };
        for i in 0..calls {
            let at = |r: u32| t.reg(r)[i].1;
            assert_eq!(
                [at(3), at(4), at(5), at(6), at(7)],
                [first, 0, then, 0, 0],
                "call {i}, cia {cia}"
            );
        }
    }
}

#[test]
fn a_write_only_register_reads_the_bus_until_its_lifetime_is_up() {
    // (model, cycles from the write to the read, the value read), at the lifetime's edges:
    // 0x1d00 on a 6581 and 0xa2000 on an 8580. The read can come 4 cycles after the write
    // at the soonest (STA, LDA).
    for (model, d, v) in [
        (M6581, 4, 0xA5),
        (M6581, 0x1cff, 0xA5),
        (M6581, 0x1d00, 0),
        (M8580, 0x1d00, 0xA5),
        (M8580, 0xa1fff, 0xA5),
        (M8580, 0xa2000, 0),
    ] {
        let p = bus_read(d);
        let mut t = Tune::load_shadowed(&p, &[model]);
        assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
        let (w, r) = (t.reg(5)[0].0, t.reg(0)[0]);
        assert_eq!(r.0 - 4 - w, d, "the fixture's distance");
        assert_eq!(r.1, v, "model {model}, {d} cycles");
    }
}

#[test]
fn each_chip_has_its_own_bus() {
    let p = two_sid_buses();
    let mut t = Tune::load_shadowed(&p, &[M6581, M6581]);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    assert_eq!(
        sid_writes(&t),
        [(0, 5, 0x11), (1, 5, 0x22), (0, 0, 0x11), (0, 1, 0x22)]
    );
    // Without a shadow for B its reads are 0.
    let mut t = Tune::load_shadowed(&p, &[M6581]);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    assert_eq!(sid_writes(&t)[2..], [(0, 0, 0x11), (0, 1, 0)]);
}

#[test]
fn the_vic_and_cias_read_back_what_was_written() {
    let p = io_readback();
    let mut t = Tune::load(&p);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    let got: Vec<u32> = (0..8).map(|r| t.reg(r)[0].1).collect();
    assert_eq!(got, [0x22, 0, 0, 0x33, 0x44, 0x55, 0, 0x1B]);
    // A restart clears them.
    assert_eq!(t.start(1), 0);
    assert_eq!(t.rd(0xD021, 5), 0);
    assert_eq!(t.rd(0xDC02, 5), 0);
    assert_eq!(t.rd(0xDD02, 5), 0);
}

#[test]
fn with_ram_banked_in_the_io_page_is_ram() {
    let p = io_ram();
    let mut t = Tune::load_shadowed(&p, &[M6581]);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    assert!(t.writes.is_empty(), "{:?}", t.writes);
    assert_eq!(t.mem[0xF0..0xF5], [0x77, 0x77, 0x77, 0x77, 0]);
    for a in [0xD021, 0xD400, 0xDC02, 0xD800] {
        assert_eq!(t.mem[a], 0x77, "${a:04X}");
    }
    assert_eq!(t.shadows[0].bus, 0);
}

#[test]
fn a_read_modify_write_of_a_sid_register_is_two_writes() {
    let p = rmw();
    let mut t = Tune::load_shadowed(&p, &[M6581]);
    assert_eq!(run_init(&mut t), PSID_RUN_INIT_DONE as i32);
    let w = t.reg(5);
    assert_eq!(w.len(), 3);
    assert_eq!(w[1].1, 0x05); // the old value, then the new, in consecutive cycles
    assert_eq!(w[2], (w[1].0 + 1, 0x0A));
    assert_eq!(t.shadows[0].bus, 0x0A);
}

#[test]
fn the_interrupt_flags_are_acknowledged_as_stated() {
    let p = vbi();
    let mut t = Tune::load(&p);
    assert_eq!(t.start(1), 0);
    t.env.vic_irq = 0x81;
    t.env.cia1_icr = 0x81;
    assert_eq!(t.rd(0xD019, 1), 0x81);
    assert_eq!(
        t.rd(0xD019, 2),
        0x81,
        "a read of $D019 does not acknowledge"
    );
    t.wr(0xD019, 0, 3);
    assert_eq!(t.rd(0xD019, 4), 0, "a write does");
    assert_eq!(t.rd(0xDC0D, 5), 0x81);
    assert_eq!(t.rd(0xDC0D, 6), 0, "a read of $DC0D does");
    assert_eq!(t.rd(0xDD0D, 7), 0);
    // The registers' mirrors: the VIC every 64 bytes, a CIA every 16.
    t.wr(0xD3E5, 0x42, 8);
    assert_eq!(t.rd(0xD025, 9), 0x42);
    t.wr(0xDCF3, 0x43, 8);
    assert_eq!(t.rd(0xDC03, 9), 0x43);
    t.wr(0xDDF3, 0x44, 8);
    assert_eq!(t.rd(0xDD03, 9), 0x44);
    assert_eq!(t.rd(0xDC13, 9), 0x43);
}
