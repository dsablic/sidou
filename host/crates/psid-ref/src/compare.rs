// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's side against the oracle, call by call (by index, so the two sides' different
//! periods do not shift the calls), and SIDOU's call times against the period rule.
//!
//! In each call the two sides' `$01` at its start, their I/O reads, their SID writes and their
//! lengths are compared in time order. Reads SIDOU made must have been made by the oracle at
//! the same cycle and address (m6502.h also makes the 6502's dummy reads, which SIDOU's core
//! does not send to the I/O page; those are not compared); the writes must be equal, cycle,
//! register and value, the chip aside (`psid.c` folds every SID into one); SIDOU's chip is
//! checked against the write's address separately. The first difference must be one of the
//! [`Rule`]s; it ends that tune's comparison, and the outcome names it. Anything else is an
//! error: SIDOU and `psid.c` disagree where no rule says they may.

use sidou_sys::psid_header;

use crate::{Call, End, IoWrite, Read, Run, Write};

/// The expected differences (TESTING.md, "The PSID oracle"; the M3 plan's Task 7 table). E8,
/// the timing between calls, is not one: calls are compared by index, and SIDOU's times are
/// checked against the period rule by [`check_timing`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rule {
    /// The bank register at a call's start: SIDOU sets it per call from the call's address
    /// (libsidplayfp's PSID driver, the format), `psid.c` sets `$37` once at load. With no ROMs
    /// on either side, a difference that maps the I/O page the same way (`$36`, `$35` or
    /// `$37`) shows only to a tune that reads `$01`: the comparison goes on, and from that call
    /// on E1 explains a difference no other rule does.
    E1Bank,
    /// A SID read (OSC3, ENV3, the POTs, a write-only register's bus value) with a different
    /// value: SIDOU's voice shadow (the C64, reSIDfp: Task 5b), `psid.c`'s random OSC3 and 0.
    E2SidRead,
    /// A raster read (`$D011`, `$D012`): SIDOU's from the absolute cycle in the tune's
    /// geometry (it jumps once at the 32-bit count's wrap), `psid.c`'s from the call's start
    /// in PAL's.
    E3Raster,
    /// A CIA 1 timer A read (`$DC04`, `$DC05`): SIDOU's counter, `psid.c`'s latch.
    E4CiaTimer,
    /// An interrupt flag read (`$D019`, `$DC0D`, `$DD0D`): SIDOU's flags, `psid.c`'s 0.
    E5IrqFlag,
    /// Any other I/O read: SIDOU's I/O shadows (the last write), `psid.c`'s 0.
    E6OtherRead,
    /// A call cut at `psid.c`'s cap (two frames for play, 5 PAL seconds for init) that SIDOU
    /// runs on (to its return, or its own limit of 1 s, or 5 s of the tune's clock), as a
    /// C64's handler runs to its RTI.
    E7Cut,
    /// A third SID's register (Task 6b): SIDOU drops its writes (counted) and reads 0;
    /// `psid.c` folds those inside `$D400-$D7FF` into its one SID (OSC3 random) and drops the
    /// rest. (The expansion areas outside a SID's window read 0 and drop writes on both.)
    E9ThirdSid,
    /// A JAM: SIDOU stops the tune at it (`6502 JAM AT`, as the real CPU halts); m6502.h's JAM
    /// keeps the CPU busy, so `psid.c` runs on to its cap.
    E10Jam,
    /// An interrupt call (play address 0; Task 6a): SIDOU's stack is what the IRQ (and, through
    /// `$0314`, the KERNAL's entry) leaves, S `$FC` (`$F9` and X = S through `$0314`), P pushed
    /// as `$20`; `psid.c`'s S is `$FC`, X 0, P pushed as the previous call left it. Only a
    /// handler that reads the stack or X unset sees it.
    E11IrqStack,
    /// A second SID outside `$D400-$D7FF` (`$DE00-$DFFF`, the format's `secondSIDAddress`):
    /// SIDOU plays it as chip B, `psid.c` drops its writes and reads 0.
    E12SecondSidOutside,
}

impl Rule {
    pub const ALL: [Rule; 11] = [
        Rule::E1Bank,
        Rule::E2SidRead,
        Rule::E3Raster,
        Rule::E4CiaTimer,
        Rule::E5IrqFlag,
        Rule::E6OtherRead,
        Rule::E7Cut,
        Rule::E9ThirdSid,
        Rule::E10Jam,
        Rule::E11IrqStack,
        Rule::E12SecondSidOutside,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Rule::E1Bank => "E1 bank register",
            Rule::E2SidRead => "E2 SID read",
            Rule::E3Raster => "E3 raster read",
            Rule::E4CiaTimer => "E4 CIA timer read",
            Rule::E5IrqFlag => "E5 interrupt flag read",
            Rule::E6OtherRead => "E6 other I/O read",
            Rule::E7Cut => "E7 call cut by psid.c",
            Rule::E9ThirdSid => "E9 third SID",
            Rule::E10Jam => "E10 JAM",
            Rule::E11IrqStack => "E11 interrupt call's stack",
            Rule::E12SecondSidOutside => "E12 second SID outside $D400-$D7FF",
        }
    }
}

/// Where the comparison of a tune ended, with the rule that explains it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    pub rule: Rule,
    /// The call's index (0 is init) and the cycle in it.
    pub call: usize,
    pub cycle: u32,
    pub detail: String,
}

/// A tune's comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Calls equal on both sides, before the first difference or the end of either run.
    pub calls: usize,
    /// Writes in those calls.
    pub writes: usize,
    pub first: Option<Difference>,
}

/// A tune's SID windows, as SIDOU's environment and the format place them.
struct Windows {
    sid2: u32,
    sid3: u32,
}

const SID_WINDOW: u32 = 0x20;

impl Windows {
    fn of(h: &psid_header) -> Windows {
        Windows {
            sid2: u32::from(h.sid2),
            sid3: u32::from(h.sid3),
        }
    }

    fn in_sid2(&self, a: u32) -> bool {
        self.sid2 != 0 && a.wrapping_sub(self.sid2) < SID_WINDOW
    }

    fn in_sid3(&self, a: u32) -> bool {
        self.sid3 != 0 && a.wrapping_sub(self.sid3) < SID_WINDOW
    }

    /// The rule a read of `a` with different values falls under.
    fn read_rule(&self, a: u32) -> Rule {
        if self.in_sid3(a) && !self.in_sid2(a) {
            return Rule::E9ThirdSid;
        }
        if self.in_sid2(a) || (0xD400..0xD800).contains(&a) {
            return Rule::E2SidRead;
        }
        match a {
            0xD000..=0xD3FF => match a & 0x3F {
                0x11 | 0x12 => Rule::E3Raster,
                0x19 => Rule::E5IrqFlag,
                _ => Rule::E6OtherRead,
            },
            0xDC00..=0xDDFF => match (a >> 8 & 1, a & 0x0F) {
                (0, 0x04 | 0x05) => Rule::E4CiaTimer,
                (_, 0x0D) => Rule::E5IrqFlag,
                _ => Rule::E6OtherRead,
            },
            _ => Rule::E6OtherRead,
        }
    }

    /// The chip SIDOU must have sent a write of `a` to, or None if `a` is no SID's.
    fn chip(&self, a: u32) -> Option<u8> {
        if self.in_sid2(a) {
            Some(1)
        } else if self.in_sid3(a) {
            None
        } else if (0xD400..0xD800).contains(&a) {
            Some(0)
        } else {
            None
        }
    }
}

/// The two sides' first disagreement in one call, before it is explained.
enum Event<'a> {
    Bank,
    Read(&'a Read, Option<&'a Read>),
    Write(usize, Option<&'a Write>, Option<&'a Write>),
    Length,
}

/// Whether `$01` maps the I/O page in. Neither side has ROMs, so this is all a bank
/// register changes in what the 6502 sees; another difference (`$36` against `$37`, say) only
/// shows to a tune that reads `$01` itself.
fn io_on(bank: u8) -> bool {
    bank & 3 != 0 && bank & 4 != 0
}

fn write_eq(a: &Write, b: &Write) -> bool {
    (a.cycle, a.reg, a.value) == (b.cycle, b.reg, b.value)
}

/// SIDOU's call `s` against the oracle's `o`: the earliest difference, with its cycle.
fn first_event<'a>(s: &'a Call, o: &'a Call) -> Option<(u32, Event<'a>)> {
    if io_on(s.bank) != io_on(o.bank) {
        return Some((0, Event::Bank));
    }
    let mut events: Vec<(u32, u8, Event<'a>)> = Vec::new();
    // Reads: every one of SIDOU's at the oracle's same cycle and address, with its value.
    let mut j = 0;
    for r in &s.reads {
        while j < o.reads.len() && o.reads[j].cycle < r.cycle {
            j += 1;
        }
        let m = o.reads[j..]
            .iter()
            .take_while(|q| q.cycle == r.cycle)
            .find(|q| q.addr == r.addr);
        if m.is_none_or(|q| q.value != r.value) {
            events.push((r.cycle, 0, Event::Read(r, m)));
            break;
        }
    }
    // Writes, in order.
    let n = s.writes.len().max(o.writes.len());
    for k in 0..n {
        let (a, b) = (s.writes.get(k), o.writes.get(k));
        let same = matches!((a, b), (Some(a), Some(b)) if write_eq(a, b));
        if !same {
            let c = a
                .map_or(u32::MAX, |w| w.cycle)
                .min(b.map_or(u32::MAX, |w| w.cycle));
            events.push((c, 1, Event::Write(k, a, b)));
            break;
        }
    }
    let ends_equal = s.cycles == o.cycles && s.end == o.end
        // Both stopped at a 5 s init limit, each where its 6502 got there: psid.c mid-
        // instruction, SIDOU after the instruction that crossed it.
        || (o.end == End::Cut
            && matches!(s.end, End::Stopped(_))
            && s.cycles >= o.cycles
            && s.cycles - o.cycles < 8);
    if !ends_equal {
        events.push((s.cycles.min(o.cycles), 2, Event::Length));
    }
    events
        .into_iter()
        .min_by_key(|e| (e.0, e.1))
        .map(|e| (e.0, e.2))
}

fn hex_write(w: Option<&Write>) -> String {
    match w {
        Some(w) => format!(
            "cycle {} reg ${:02X} = ${:02X} (chip {}, ${:04X})",
            w.cycle, w.reg, w.value, w.chip, w.addr
        ),
        None => "none".into(),
    }
}

fn dropped_at(io: &[IoWrite], win: &Windows, cycle: u32) -> bool {
    io.iter()
        .any(|w| w.cycle == cycle && win.in_sid3(u32::from(w.addr)))
}

/// What [`explain`] knows of the tune: its play address, its SID windows, and the first call
/// whose bank register differed without changing the I/O page's mapping, with both values.
struct Ctx {
    play: u16,
    win: Windows,
    latent: Option<(usize, u8, u8)>,
}

/// Explains SIDOU's call `i` differing from the oracle's at `event`, or says why nothing does.
fn explain(
    x: &Ctx,
    i: usize,
    s: &Call,
    o: &Call,
    cycle: u32,
    event: Event,
) -> Result<Difference, String> {
    let (win, irq) = (&x.win, i > 0 && x.play == 0);
    let diff = |rule, detail: String| {
        Ok(Difference {
            rule,
            call: i,
            cycle,
            detail,
        })
    };
    let what = match event {
        Event::Bank => {
            return diff(
                Rule::E1Bank,
                format!("$01 ${:02X} against psid.c's ${:02X}", s.bank, o.bank),
            )
        }
        Event::Read(r, Some(q)) => {
            let rule = win.read_rule(u32::from(r.addr));
            return diff(
                rule,
                format!(
                    "${:04X} read ${:02X} against psid.c's ${:02X}",
                    r.addr, r.value, q.value
                ),
            );
        }
        Event::Read(r, None) => format!(
            "SIDOU read ${:04X} (${:02X}) at cycle {}, where psid.c made no read of it",
            r.addr, r.value, r.cycle
        ),
        Event::Write(k, a, b) => {
            if let Some(w) = a {
                if !(0xD400..0xD800).contains(&u32::from(w.addr))
                    && b.is_none_or(|b| b.cycle >= w.cycle)
                {
                    return diff(
                        Rule::E12SecondSidOutside,
                        format!("write {k}: SIDOU's {}", hex_write(a)),
                    );
                }
            }
            if let Some(b) = b {
                if a.is_none_or(|a| a.cycle >= b.cycle) && dropped_at(&s.io_writes, win, b.cycle) {
                    return diff(
                        Rule::E9ThirdSid,
                        format!(
                            "write {k}: psid.c's {}, SIDOU dropped it",
                            hex_write(Some(b))
                        ),
                    );
                }
            }
            format!(
                "write {k}: SIDOU's {}, psid.c's {}",
                hex_write(a),
                hex_write(b)
            )
        }
        Event::Length => {
            if o.end == End::Cut && s.cycles > o.cycles {
                return diff(
                    Rule::E7Cut,
                    format!(
                        "psid.c cut it at {} cycles, SIDOU ran {} ({:?})",
                        o.cycles, s.cycles, s.end
                    ),
                );
            }
            if s.end == End::Stopped(sidou_sys::PSID_RUN_JAM) && o.end == End::Cut {
                return diff(
                    Rule::E10Jam,
                    format!(
                        "SIDOU stopped at a JAM after {} cycles, psid.c ran to {}",
                        s.cycles, o.cycles
                    ),
                );
            }
            format!(
                "SIDOU's call took {} cycles ({:?}), psid.c's {} ({:?})",
                s.cycles, s.end, o.cycles, o.end
            )
        }
    };
    if irq {
        return diff(Rule::E11IrqStack, what);
    }
    if let Some((k, sb, ob)) = x.latent {
        return diff(
            Rule::E1Bank,
            format!("{what}; $01 differs since call {k} (${sb:02X} against psid.c's ${ob:02X})"),
        );
    }
    Err(format!("call {i} (start {}): {what}", s.start))
}

/// Compares SIDOU's run of a tune with the oracle's (module docs). `Err` says where they
/// disagree and no rule explains it, or where SIDOU sent a write to the wrong chip.
pub fn compare(h: &psid_header, sidou: &Run, oracle: &Run) -> Result<Outcome, String> {
    let mut x = Ctx {
        play: h.play,
        win: Windows::of(h),
        latent: None,
    };
    let win = &x.win;
    // SIDOU's chips, against the writes' addresses: every write, compared or not.
    for (i, c) in sidou.calls.iter().enumerate() {
        for w in &c.writes {
            let want = win.chip(u32::from(w.addr));
            if want != Some(w.chip) || w.reg != (w.addr & 0x1F) as u8 {
                return Err(format!(
                    "call {i}: a write of ${:04X} went to chip {} register ${:02X} (want {want:?})",
                    w.addr, w.chip, w.reg
                ));
            }
        }
    }
    let mut out = Outcome {
        calls: 0,
        writes: 0,
        first: None,
    };
    // A bank register that differs without changing the I/O page's mapping (E1, latent): the
    // comparison goes on, and E1 explains what nothing else does from there on.
    for (i, (s, o)) in sidou.calls.iter().zip(&oracle.calls).enumerate() {
        if x.latent.is_none() && s.bank != o.bank {
            x.latent = Some((i, s.bank, o.bank));
        }
        if let Some((cycle, event)) = first_event(s, o) {
            out.first = Some(explain(&x, i, s, o, cycle, event)?);
            return Ok(out);
        }
        out.calls += 1;
        out.writes += s.writes.len();
        if matches!(s.end, End::Stopped(_)) {
            break;
        }
    }
    Ok(out)
}

/// The speed bit of song `n` (1-based), as the format gives it: bit `min(n, 32) - 1`, but for
/// a v1 file or one with the PlaySID flag the bits repeat, `(n - 1) mod 32`.
fn speed_bit(h: &psid_header, n: u32) -> bool {
    let bit = if h.version == 1 || h.flags & 2 != 0 {
        (n - 1) % 32
    } else {
        n.min(32) - 1
    };
    h.speed >> bit & 1 != 0
}

/// E8: SIDOU's call times against the period rule (`docs/reference/psid.md`, "Time", "The
/// period", "The clock"), restated here from the format and the C64: a VBI song's period is
/// the frame of its clock (PAL 19,656 cycles, NTSC 17,095), a CIA song's CIA 1 timer A's
/// latch plus 1 as the tune last wrote it before the previous call returned (from the KERNAL's `$4025`
/// PAL, `$4295` NTSC), with no floor. The first play call is at the first period boundary
/// after init's return; each next one's interrupt a period after the last one's; a call that
/// returns after it (not on it) is an overrun, the next call starts at its return, and the
/// interrupts it missed are one, so the grid stays. Also that every call due within `seconds`
/// ran, and no later one. Returns the calls checked.
pub fn check_timing(h: &psid_header, run: &Run, song: u32, seconds: u32) -> Result<usize, String> {
    let ntsc = h.clock == 2;
    let (clock, frame, mut latch) = if ntsc {
        (1_022_727u32, 17_095u32, 0x4295u32)
    } else {
        (985_248, 19_656, 0x4025)
    };
    let cia = speed_bit(h, song);
    let total = seconds * clock;
    // The latch's writes, on the running count, in order.
    let mut writes: Vec<(u32, u32, u32)> = Vec::new();
    for c in &run.calls {
        for w in &c.io_writes {
            let a = u32::from(w.addr);
            if (0xDC00..0xDD00).contains(&a) && matches!(a & 0x0F, 4 | 5) {
                writes.push((c.start.wrapping_add(w.cycle), a & 0x0F, u32::from(w.value)));
            }
        }
    }
    let mut wi = 0;
    let mut period_at = |t: u32| -> u32 {
        while wi < writes.len() && writes[wi].0 < t {
            let (_, reg, v) = writes[wi];
            latch = if reg == 4 {
                (latch & 0xFF00) | v
            } else {
                (latch & 0x00FF) | v << 8
            };
            wi += 1;
        }
        if cia {
            latch + 1
        } else {
            frame
        }
    };
    let calls = &run.calls;
    let first = calls.first().ok_or("no init call")?;
    if first.start != 0 {
        return Err(format!("init started at {}", first.start));
    }
    let mut ret = first.cycles;
    let p = period_at(ret);
    let mut grid = (ret / p + 1) * p;
    let mut start = grid;
    for (i, c) in calls.iter().enumerate().skip(1) {
        if c.start != start {
            return Err(format!(
                "call {i} started at {}, the period rule says {start} (the last return {ret})",
                c.start
            ));
        }
        ret = c.start + c.cycles;
        let p = period_at(ret);
        let next = grid + p;
        if ret > next {
            start = ret;
            grid = next + (ret - next) / p * p;
        } else {
            start = next;
            grid = next;
        }
    }
    let last = calls.last().unwrap();
    if !matches!(last.end, End::Stopped(_)) && start < total {
        return Err(format!(
            "the run ended with a call due at {start}, before the end at {total}"
        ));
    }
    Ok(calls.len())
}
