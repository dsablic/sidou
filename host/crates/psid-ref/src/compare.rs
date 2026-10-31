// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::psid_header;

use crate::{Call, End, IoWrite, Read, Run, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rule {
    E1Bank,
    E2SidRead,
    E3Raster,
    E4CiaTimer,
    E5IrqFlag,
    E6OtherRead,
    E7Cut,
    E9ThirdSid,
    E10Jam,
    E11IrqStack,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    pub rule: Rule,
    pub call: usize,
    pub cycle: u32,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub calls: usize,
    pub writes: usize,
    pub first: Option<Difference>,
}

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

enum Event<'a> {
    Bank,
    Read(&'a Read, Option<&'a Read>),
    Write(usize, Option<&'a Write>, Option<&'a Write>),
    Length,
}

fn io_on(bank: u8) -> bool {
    bank & 3 != 0 && bank & 4 != 0
}

fn write_eq(a: &Write, b: &Write) -> bool {
    (a.cycle, a.reg, a.value) == (b.cycle, b.reg, b.value)
}

fn first_event<'a>(s: &'a Call, o: &'a Call) -> Option<(u32, Event<'a>)> {
    if io_on(s.bank) != io_on(o.bank) {
        return Some((0, Event::Bank));
    }
    let mut events: Vec<(u32, u8, Event<'a>)> = Vec::new();
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

struct Ctx {
    play: u16,
    win: Windows,
    latent: Option<(usize, u8, u8)>,
}

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

pub fn compare(h: &psid_header, sidou: &Run, oracle: &Run) -> Result<Outcome, String> {
    let mut x = Ctx {
        play: h.play,
        win: Windows::of(h),
        latent: None,
    };
    let win = &x.win;
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

fn speed_bit(h: &psid_header, n: u32) -> bool {
    let bit = if h.version == 1 || h.flags & 2 != 0 {
        (n - 1) % 32
    } else {
        n.min(32) - 1
    };
    h.speed >> bit & 1 != 0
}

pub fn check_timing(h: &psid_header, run: &Run, song: u32, seconds: u32) -> Result<usize, String> {
    let ntsc = h.clock == 2;
    let (clock, frame, mut latch) = if ntsc {
        (1_022_727u32, 17_095u32, 0x4295u32)
    } else {
        (985_248, 19_656, 0x4025)
    };
    let cia = speed_bit(h, song);
    let total = seconds * clock;
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
