// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Random valid songs for the song player's register match (M2c Task 5b): a `.sng` that
//! gt2reloc packs, using every pattern command and special note, tables with jumps and
//! loops, the wavetable's commands, orderlists of up to 40 steps, patterns of up to 128
//! rows (cut to what packs into 256 bytes), up to 4 subtunes, now and then a gate-off timer
//! at or above the tempo, and random playroutine options within the device's caps
//! (multispeed 1-4).
//!
//! "Valid" is what the packer accepts (no table pointer onto a jump, no table running off
//! its end, no illegal wavetable command, patterns that pack into 256 bytes) and what plays
//! the same on any player: every note stays inside the frequency table (C-0 to B-7, with
//! transposes, wavetable notes and the note-independent speeds' next note), and every
//! orderlist ends on a pattern, so player.s never reads past its own orderlist.

use crate::Options;

/// A small xorshift generator: the same seed, the same song.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }

    pub fn word(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }

    /// 0 to n - 1.
    pub fn below(&mut self, n: u32) -> u32 {
        self.word() % n
    }

    /// lo to hi, both included.
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.below(hi - lo + 1)
    }

    pub fn byte(&mut self) -> u8 {
        self.word() as u8
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }

    pub fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[self.below(v.len() as u32) as usize]
    }
}

/// A generated song and the options to play it with.
#[derive(Clone, Debug)]
pub struct Generated {
    pub sng: Vec<u8>,
    pub options: Options,
}

/// What a speedtable entry is for.
#[derive(Clone, Copy, PartialEq)]
enum Speed {
    Vibrato,
    Porta,
    Calculated,
    Funk,
}

/// The tables as programs: each a run of rows ending in a jump (to a row of its own, or 0
/// to stop). `starts` are the programs' first rows (1-based).
#[derive(Default)]
struct Table {
    rows: Vec<(u8, u8)>,
    starts: Vec<u8>,
}

impl Table {
    /// A program of `rows` (not jumps) and its final jump: a loop to one of its rows, or a
    /// stop.
    fn program(&mut self, rng: &mut Rng, rows: Vec<(u8, u8)>) {
        let start = self.rows.len() as u8 + 1;
        let n = rows.len() as u8;
        self.rows.extend(rows);
        let target = if rng.chance(60) {
            start + rng.below(n as u32) as u8
        } else {
            0
        };
        self.rows.push((0xff, target));
        self.starts.push(start);
    }

    fn start_or_zero(&self, rng: &mut Rng, percent_zero: u32) -> u8 {
        if self.starts.is_empty() || rng.chance(percent_zero) {
            0
        } else {
            rng.pick(&self.starts)
        }
    }
}

const WAVEFORMS: [u8; 10] = [0x11, 0x21, 0x41, 0x81, 0x51, 0x15, 0x13, 0x40, 0x20, 0x10];
/// Pattern notes: C-2 to C-5 (note 24 to 60).
const NOTE_LO: u32 = 24;
const NOTE_HI: u32 = 60;
/// Transposes within +-7, wavetable notes within -12 to +24 and absolute notes up to 80:
/// every note stays within 5 to 91, its next note (for the note-independent speeds) too.
const TRANS: u32 = 7;

fn speed_entry(rng: &mut Rng, kind: Speed) -> (u8, u8) {
    match kind {
        Speed::Vibrato => (rng.range(1, 8) as u8, rng.range(1, 0x60) as u8),
        Speed::Porta => (rng.range(0, 1) as u8, rng.byte()),
        Speed::Calculated => (0x80 | rng.range(0, 6) as u8, rng.range(0, 5) as u8),
        Speed::Funk => (rng.range(3, 9) as u8, rng.range(3, 9) as u8),
    }
}

fn wave_right(rng: &mut Rng) -> u8 {
    match rng.below(5) {
        0 => rng.range(0, 24) as u8,           // relative up
        1 => rng.range(0x74, 0x7f) as u8,      // relative down, to -12
        2 => 0x80,                             // keep the frequency
        3 => rng.range(0x81, 0x81 + 80) as u8, // absolute, to note 80
        _ => 0,
    }
}

/// Options within the device's caps: multispeed 1-4 and every other option at random.
fn options(rng: &mut Rng) -> Options {
    let mut o = Options {
        multiplier: rng.pick(&[1, 1, 1, 2, 3, 4]),
        optimize_pulse: rng.below(2),
        optimize_realtime: rng.below(2),
        ntsc: u32::from(rng.chance(20)),
        zeropage: 0xe0,
        ..Options::default()
    };
    o.player_version = rng.pick(&[
        0,
        0,
        crate::PLAYER_BUFFERED,
        crate::PLAYER_ZPGHOSTREGS | crate::PLAYER_BUFFERED,
        crate::PLAYER_FULLBUFFERED | crate::PLAYER_BUFFERED,
    ]);
    if rng.chance(10) {
        o.player_version |= crate::PLAYER_SOUNDEFFECTS | crate::PLAYER_BUFFERED;
    }
    if rng.chance(20) {
        o.player_version |= crate::PLAYER_AUTHORINFO;
    }
    if rng.chance(10) {
        o.player_version |= crate::PLAYER_NOOPTIMIZATION;
    }
    o.hr_adsr = match rng.below(10) {
        0 => 0xf000 | rng.below(0x1000),
        1 | 2 => rng.below(0xf000),
        _ => 0x0f00,
    };
    o
}

/// A pattern cut until it packs into gt2reloc's 256 bytes: an upper bound of packpattern's
/// size (an instrument byte, a command byte and its parameter whenever they change, the
/// note, the end mark; packed rests only make it smaller).
fn fit_packed(mut rows: Vec<[u8; 4]>) -> Vec<[u8; 4]> {
    loop {
        let mut size = 1;
        let mut cmd = None;
        for r in &rows {
            size += usize::from(r[1] != 0) + 1;
            if cmd != Some((r[2], r[3])) {
                cmd = Some((r[2], r[3]));
                size += 1 + usize::from(r[2] != 0);
            }
        }
        if size <= 255 {
            return rows;
        }
        rows.pop();
    }
}

/// The song for a seed.
pub fn generate(seed: u64) -> Generated {
    let rng = &mut Rng::new(seed);
    let options = options(rng);
    let noopt = options.player_version & crate::PLAYER_NOOPTIMIZATION != 0;

    // The speedtable: each entry one kind, so each command points at one that suits it.
    let kinds: Vec<Speed> = (0..rng.range(2, 9))
        .map(|_| rng.pick(&[Speed::Vibrato, Speed::Porta, Speed::Calculated, Speed::Funk]))
        .collect();
    let speeds: Vec<(u8, u8)> = kinds.iter().map(|&k| speed_entry(rng, k)).collect();
    let speed_of = |rng: &mut Rng, want: &[Speed]| -> u8 {
        let idx: Vec<u8> = (0..kinds.len())
            .filter(|&i| want.contains(&kinds[i]))
            .map(|i| i as u8 + 1)
            .collect();
        if idx.is_empty() {
            0
        } else {
            rng.pick(&idx)
        }
    };
    let motion = [Speed::Vibrato, Speed::Porta, Speed::Calculated];

    let simple_pulse = rng.chance(40);
    let mut pulse = Table::default();
    for _ in 0..rng.range(0, 3) {
        let rows = (0..rng.range(1, 4))
            .map(|k| {
                let mut r = if k == 0 || rng.chance(30) {
                    (0x80 | rng.below(16) as u8, rng.byte())
                } else {
                    (rng.range(1, 0x40) as u8, rng.byte())
                };
                if simple_pulse {
                    r.1 &= 0xf0;
                }
                r
            })
            .collect();
        pulse.program(rng, rows);
    }
    let mut filter = Table::default();
    for _ in 0..rng.range(0, 3) {
        let rows = (0..rng.range(1, 5))
            .map(|k| match if k == 0 { 0 } else { rng.below(3) } {
                0 => (
                    rng.pick(&[0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0, 0xf0]),
                    rng.byte(),
                ),
                1 => (0x00, rng.byte()),
                _ => (rng.range(1, 0x30) as u8, rng.byte()),
            })
            .collect();
        filter.program(rng, rows);
    }
    let filter_used = !filter.starts.is_empty();

    let mut wave = Table::default();
    let delays = rng.chance(60);
    let commands = rng.chance(50);
    for _ in 0..rng.range(1, 5) {
        let rows = (0..rng.range(1, 6))
            .map(|k| {
                if k == 0 {
                    // a note first, so the channel's last note (the note-independent
                    // speeds' base) is always one the frequency table holds
                    let note = loop {
                        let r = wave_right(rng);
                        if r != 0x80 && !(0x74..=0x7f).contains(&r) {
                            break r;
                        }
                    };
                    return (rng.pick(&WAVEFORMS) | 1, note);
                }
                match rng.below(if commands { 7 } else { 6 }) {
                    0 | 1 => {
                        // $D0-$DF run as commands when the table has delays and
                        // commands, and may then reach a handler that takes a table
                        // index (the packer renumbers the tables, and a handler it
                        // leaves out runs the next one's code): plain waveforms only
                        // without that, -I0 (which turns both on) included
                        // (gt2_match.rs's quirk fixture covers the quirk)
                        let w = if !(delays && commands) && !noopt && rng.chance(10) {
                            rng.range(0xd0, 0xdf) as u8
                        } else {
                            rng.pick(&WAVEFORMS) | rng.below(2) as u8
                        };
                        (w, wave_right(rng))
                    }
                    2 => (rng.range(0xe0, 0xef) as u8, wave_right(rng)),
                    3 if delays => (rng.range(1, 0x0f) as u8, wave_right(rng)),
                    4 => (0x00, wave_right(rng)),
                    6 => {
                        let mut cmds = vec![1u8, 2, 3, 4, 5, 6, 7, 13];
                        if !pulse.starts.is_empty() {
                            cmds.push(9);
                        }
                        if filter_used {
                            cmds.extend([10, 11, 12]);
                        }
                        let c = rng.pick(&cmds);
                        let d = match c {
                            1 | 2 | 4 => speed_of(rng, &motion),
                            3 if rng.chance(30) => 0,
                            3 => speed_of(rng, &motion),
                            7 => rng.pick(&WAVEFORMS),
                            9 => rng.pick(&pulse.starts),
                            10 => rng.pick(&filter.starts),
                            13 => rng.below(16) as u8,
                            _ => rng.byte(),
                        };
                        (0xf0 + c, d)
                    }
                    _ => (rng.pick(&WAVEFORMS) | 1, wave_right(rng)),
                }
            })
            .collect();
        wave.program(rng, rows);
    }

    // The instruments. Instrument 1 holds the filter's program, so that the wavetable's
    // filter commands always have the filter's code to run in.
    let ninstr = rng.range(1, 8) as usize;
    let mut instr: Vec<[u8; 25]> = (0..ninstr)
        .map(|k| {
            let mut i = [0u8; 25];
            let vib = if rng.chance(40) {
                speed_of(rng, &[Speed::Vibrato, Speed::Calculated])
            } else {
                0
            };
            let mut gate = rng.range(0, 3) as u8;
            if gate == 0 && rng.chance(70) {
                gate = 2;
            }
            if rng.chance(3) {
                // at or above the tempo: the editor stops; the packed player never fetches
                // the channel's next row again
                gate = rng.range(6, 0x3f) as u8;
            }
            if rng.chance(15) {
                gate |= 0x80;
            }
            if rng.chance(15) {
                gate |= 0x40;
            }
            i[0] = rng.byte();
            i[1] = rng.byte();
            i[2] = rng.pick(&wave.starts);
            i[3] = pulse.start_or_zero(rng, 40);
            i[4] = if k == 0 && filter_used {
                rng.pick(&filter.starts)
            } else {
                filter.start_or_zero(rng, 70)
            };
            i[5] = vib;
            i[6] = if vib != 0 { rng.range(0, 6) as u8 } else { 0 };
            i[7] = gate;
            i[8] = rng.pick(&[0x09, 0x09, 0x41, 0x81, 0x11, 0x00, 0xfe, 0xff]);
            i
        })
        .collect();
    if rng.chance(15) {
        // instrument 63's AD sets the default tempo
        instr.resize(63, [0; 25]);
        instr[62][0] = rng.range(4, 9) as u8;
    }

    // The patterns.
    let npat = rng.range(2, 8) as usize;
    // Each pattern opens on a note and an instrument (not a toneportamento), so a channel
    // has a note before any effect reads it.
    let patterns: Vec<Vec<[u8; 4]>> = (0..npat)
        .map(|_| {
            let rows = if rng.chance(15) {
                rng.range(64, 128)
            } else {
                rng.range(4, 40)
            };
            (0..rows)
                .map(|r| {
                    let note = match if r == 0 { 9 } else { rng.below(20) } {
                        0..=8 => 0xbd,
                        9..=15 => 0x60 + rng.range(NOTE_LO, NOTE_HI) as u8,
                        16 | 17 => 0xbe,
                        _ => 0xbf,
                    };
                    let ins = if r == 0 || rng.chance(30) {
                        rng.range(1, ninstr as u32) as u8
                    } else {
                        0
                    };
                    let (c, d) = if rng.chance(45) {
                        (0, 0)
                    } else {
                        let c = rng.below(16) as u8;
                        let d = match c {
                            0 => 0,
                            1 | 2 | 4 => speed_of(rng, &motion),
                            3 if rng.chance(30) => 0,
                            3 => speed_of(rng, &motion),
                            7 => rng.pick(&WAVEFORMS) | rng.below(2) as u8,
                            8 => wave.start_or_zero(rng, 20),
                            9 => pulse.start_or_zero(rng, 20),
                            10 => filter.start_or_zero(rng, 20),
                            11 => {
                                if rng.chance(20) {
                                    0
                                } else {
                                    rng.byte()
                                }
                            }
                            13 => {
                                if rng.chance(25) {
                                    rng.range(0x10, 0xff) as u8
                                } else {
                                    rng.below(16) as u8
                                }
                            }
                            14 => speed_of(rng, &[Speed::Funk]),
                            15 => match rng.below(10) {
                                0 => rng.range(0, 2) as u8,
                                1 => rng.range(0x80, 0x82) as u8,
                                2..=4 => rng.range(0x84, 0x8a) as u8,
                                _ => rng.range(4, 10) as u8,
                            },
                            _ => rng.byte(),
                        };
                        // funktempo needs a funk entry
                        if (c == 14 && d == 0) || (c == 3 && r == 0) {
                            (0, 0)
                        } else {
                            (c, d)
                        }
                    };
                    [note, ins, c, d]
                })
                .collect::<Vec<[u8; 4]>>()
        })
        .map(fit_packed)
        .collect();

    // The orderlists: [transpose] [repeat] pattern steps, sometimes an R0 or a transpose
    // after a repeat (readme 3.1's rules broken, as GoatTracker packs them), always a
    // pattern last.
    let subtunes = rng.pick(&[1, 1, 2, 3, 4]) as usize;
    let orders: Vec<Vec<(Vec<u8>, u8)>> = (0..subtunes)
        .map(|_| {
            (0..3)
                .map(|_| {
                    let mut e = Vec::new();
                    let steps = if rng.chance(15) {
                        rng.range(7, 40)
                    } else {
                        rng.range(1, 6)
                    };
                    for _ in 0..steps {
                        let trans = |rng: &mut Rng| {
                            let t = rng.range(0, 2 * TRANS) as i32 - TRANS as i32;
                            (0xf0i32 + t) as u8
                        };
                        let broken = rng.chance(3);
                        if rng.chance(25) && !broken {
                            e.push(trans(rng));
                        }
                        if rng.chance(20) {
                            e.push(if rng.chance(15) {
                                0xd0
                            } else {
                                rng.range(0xd1, 0xd3) as u8
                            });
                        }
                        if broken {
                            e.push(trans(rng));
                        }
                        e.push(rng.below(npat as u32) as u8);
                    }
                    // a restart among the entries the packer keeps (it drops R0)
                    let kept = e.iter().filter(|&&b| b != 0xd0).count() as u32;
                    let restart = rng.below(kept) as u8;
                    (e, restart)
                })
                .collect()
        })
        .collect();

    let mut sng = Vec::new();
    sng.extend_from_slice(b"GTS5");
    for name in [b"GENERATED SONG".as_slice(), b"SIDOU", b"2026"] {
        let mut s = [0u8; 32];
        s[..name.len()].copy_from_slice(name);
        sng.extend_from_slice(&s);
    }
    sng.push(orders.len() as u8);
    for sub in &orders {
        for (entries, restart) in sub {
            sng.push(entries.len() as u8 + 1);
            sng.extend_from_slice(entries);
            sng.push(0xff);
            sng.push(*restart);
        }
    }
    sng.push(instr.len() as u8);
    for i in &instr {
        sng.extend_from_slice(i);
    }
    for t in [&wave.rows, &pulse.rows, &filter.rows, &speeds] {
        sng.push(t.len() as u8);
        sng.extend(t.iter().map(|r| r.0));
        sng.extend(t.iter().map(|r| r.1));
    }
    sng.push(patterns.len() as u8);
    for p in &patterns {
        sng.push(p.len() as u8 + 1);
        for r in p {
            sng.extend_from_slice(r);
        }
        sng.extend_from_slice(&[0xff, 0, 0, 0]);
    }
    Generated { sng, options }
}
