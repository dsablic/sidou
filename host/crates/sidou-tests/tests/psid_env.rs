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
    // overlay with the I/O page's shadows (Task 6b); Task 6a's part, measured as the pi32v2
    // lays it out (ILP32), 92 bytes.
    assert_eq!(ilp32::psid_env::SIZE, 92);
    assert_eq!(ilp32::cpu6502::SIZE, 32);
}
