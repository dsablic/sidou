// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

pub const TAG_OPTS: u32 = tag(b"OPTS");
pub const TAG_CHPA: u32 = tag(b"CHPA");
pub const TAG_CHPB: u32 = tag(b"CHPB");
pub const TAG_SLOT: u32 = tag(b"SLOT");
pub const TAG_LOCK: u32 = tag(b"LOCK");
pub const TAG_MIDI: u32 = tag(b"MIDI");
pub const TAG_SCNE: u32 = tag(b"SCNE");
pub const TAG_KEYS: u32 = tag(b"KEYS");
pub const TAG_PAD: u32 = tag(b"PAD ");

pub const REST: [u8; 4] = [GT2_REST as u8, 0, 0, 0];
pub const KEY_OFF: u8 = GT2_KEYOFF as u8;
pub const KEY_ON: u8 = GT2_KEYON as u8;
pub const PATTERN_NOTE_C3: u8 = GT2_FIRSTNOTE as u8 + 36;

pub fn row(note: u8, instr: u8) -> [u8; 4] {
    [note, instr, 0, 0]
}

pub fn kit_instr(d: usize) -> [u8; 25] {
    // SAFETY: a kit drum is 25 bytes, an instrument's.
    unsafe { std::mem::transmute::<gt2_kit_drum, [u8; 25]>(gt2_kit_instr[d]) }
}

pub fn kit_tables() -> [Vec<(u8, u8)>; 4] {
    // SAFETY: plain constant data.
    let (wave, pulse) = unsafe {
        (
            (&raw const gt2_kit_wave).read(),
            (&raw const gt2_kit_pulse).read(),
        )
    };
    let table = |l: &[u8], r: &[u8]| l.iter().copied().zip(r.iter().copied()).collect();
    [
        table(&wave[0], &wave[1]),
        table(&pulse[0], &pulse[1]),
        vec![],
        vec![],
    ]
}

pub const fn tag(t: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*t)
}

pub fn span_off4(s: u32) -> u32 {
    s & 0xffff
}

pub fn span_len(s: u32) -> u32 {
    s >> 16
}
pub use sidou_sys::song::{ReadInfo, SngSpec, Song};

pub fn determinechannels(file: &[u8]) -> u32 {
    let at = |i: usize| file.get(i).copied().unwrap_or(0);
    let songs = at(100) as usize;
    let mut p = 101;
    for _ in 0..songs {
        for _ in 0..6 {
            let n = at(p) as usize;
            p += 1;
            let loadsize = n + 1;
            let buf: Vec<u8> = (0..loadsize).map(|i| at(p + i)).collect();
            p += loadsize;
            if loadsize < 2 || buf[loadsize - 2] != 0xff || buf[loadsize - 1] as usize >= loadsize {
                return 3;
            }
        }
    }
    6
}

pub struct Rng(pub u64);

impl Rng {
    pub fn word(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }

    pub fn below(&mut self, n: u32) -> u32 {
        self.word() % n
    }

    pub fn byte(&mut self) -> u8 {
        self.word() as u8
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }
}

pub fn random_order(rng: &mut Rng, steps: u32, patterns: u32) -> (Vec<u8>, u8) {
    let mut e = Vec::new();
    for _ in 0..steps {
        if rng.chance(25) {
            e.push(0xe0 + rng.below(0x1f) as u8);
        }
        if rng.chance(20) {
            e.push(0xd0 + rng.below(16) as u8);
        }
        e.push(rng.below(patterns) as u8);
    }
    let restart = rng.below(e.len() as u32) as u8;
    (e, restart)
}

const USED_INSTRUMENT_AD_BIT: u8 = 1;

pub fn random_canonical(rng: &mut Rng, stereo: bool) -> SngSpec {
    let channels = if stereo { 6 } else { 3 };
    let npat = 1 + rng.below(12);
    let ninstr = rng.below(11) as usize;
    let mut s = SngSpec::new();
    for k in 0..3 {
        for b in s.strings[k].iter_mut().take(rng.below(20) as usize) {
            *b = b'A' + rng.below(26) as u8;
        }
    }
    s.orders = (0..1 + rng.below(3))
        .map(|_| {
            (0..channels)
                .map(|_| {
                    let steps = 1 + rng.below(8);
                    random_order(rng, steps, npat)
                })
                .collect()
        })
        .collect();
    s.orders[0][0].0.push(npat as u8 - 1);
    s.instr = (0..ninstr)
        .map(|_| {
            let mut i = [0u8; 25];
            for b in i.iter_mut() {
                *b = rng.byte();
            }
            i
        })
        .collect();
    if let Some(last) = s.instr.last_mut() {
        last[0] |= USED_INSTRUMENT_AD_BIT;
    }
    s.tables = (0..4)
        .map(|_| {
            let n = rng.below(21) as usize;
            let mut t: Vec<(u8, u8)> = (0..n).map(|_| (rng.byte(), rng.byte())).collect();
            if let Some(last) = t.last_mut() {
                last.0 |= 1;
            }
            t
        })
        .collect();
    s.patterns = (0..npat)
        .map(|_| {
            let rows = 1 + rng.below(64);
            let r = (0..rows)
                .map(|_| {
                    [
                        0x60 + rng.below(0x60) as u8,
                        rng.below(ninstr as u32 + 1) as u8,
                        rng.below(16) as u8,
                        rng.byte(),
                    ]
                })
                .collect();
            (r, [0xff, 0, 0, 0])
        })
        .collect();
    s
}

pub fn set_mono_from_spec(s: &mut Song, spec: &SngSpec) {
    let p = s.ptr_mut();
    let commit = |e: &gt2_edit| unsafe { gt2_edit_commit(p, e) };
    let mut e = gt2_edit::default();
    unsafe {
        for (n, (rows, _)) in spec.patterns.iter().enumerate() {
            if n >= 3 {
                let mut got = 0;
                assert_eq!(
                    gt2_pattern_add(p, rows.len() as u32, &mut got, &mut e),
                    GT2_OK
                );
                assert_eq!(got, n as u32);
            } else {
                assert_eq!(
                    gt2_pattern_resize(p, n as u32, rows.len() as u32, &mut e),
                    GT2_OK
                );
            }
            commit(&e);
            for (r, cell) in rows.iter().enumerate() {
                assert_eq!(
                    gt2_set_row(p, n as u32, r as u32, cell.as_ptr(), &mut e),
                    GT2_OK
                );
                commit(&e);
            }
        }
        for (c, (entries, restart)) in spec.orders[0].iter().enumerate() {
            let c = c as u32;
            for (k, v) in entries.iter().enumerate() {
                assert_eq!(gt2_order_insert(p, 0, c, k as u32, *v, &mut e), GT2_OK);
                commit(&e);
            }
            assert_eq!(
                gt2_order_delete(p, 0, c, entries.len() as u32, &mut e),
                GT2_OK
            );
            commit(&e);
            assert_eq!(
                gt2_set_order(p, 0, c, entries.len() as u32, *restart, &mut e),
                GT2_OK
            );
            commit(&e);
        }
        for (k, v) in spec.instr[0].iter().enumerate() {
            assert_eq!(gt2_set_instr(p, 1, k as u32, *v, &mut e), GT2_OK);
            commit(&e);
        }
        for (t, rows) in spec.tables.iter().enumerate() {
            for (r, (l, rr)) in rows.iter().enumerate() {
                assert_eq!(gt2_set_table(p, t as u32, r as u32, 0, *l, &mut e), GT2_OK);
                commit(&e);
                assert_eq!(gt2_set_table(p, t as u32, r as u32, 1, *rr, &mut e), GT2_OK);
                commit(&e);
            }
        }
    }
    for k in 0..3 {
        s.set_string(k, &spec.strings[k]);
    }
}
