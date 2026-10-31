// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::raw::c_void;

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

pub struct Song {
    region: Vec<u32>,
}

unsafe extern "C" fn read_slice(ctx: *mut c_void, offset: u32, buf: *mut u8, len: u32) -> u32 {
    let data = unsafe { &*(ctx as *const Vec<u8>) };
    let offset = offset as usize;
    if offset >= data.len() {
        return 0;
    }
    let n = (len as usize).min(data.len() - offset);
    unsafe { std::ptr::copy_nonoverlapping(data.as_ptr().add(offset), buf, n) };
    n as u32
}

unsafe extern "C" fn write_vec(ctx: *mut c_void, buf: *const u8, len: u32) -> i32 {
    let out = unsafe { &mut *(ctx as *mut Vec<u8>) };
    out.extend_from_slice(unsafe { std::slice::from_raw_parts(buf, len as usize) });
    0
}

#[derive(Debug, Clone)]
pub struct ReadInfo {
    pub msg: String,
    pub version: u8,
    pub channels: u8,
    pub trailer: u32,
    pub gt2_bytes: u32,
    pub file_bytes: u32,
    pub need_bytes: u32,
}

impl Song {
    pub fn new(bytes: u32) -> Song {
        let mut region = vec![0u32; bytes.div_ceil(4) as usize];
        let s = unsafe { gt2_song_init(region.as_mut_ptr().cast(), bytes) };
        assert!(!s.is_null(), "gt2_song_init({bytes})");
        Song { region }
    }

    pub fn device() -> Song {
        Song::new(SONG_REGION_BYTES)
    }

    pub fn host_from(file: &[u8]) -> Song {
        let mut song = Song::host();
        song.read(file, 0).unwrap();
        song
    }

    pub fn host() -> Song {
        Song::new(GT2_REGION_MAX)
    }

    pub fn ptr(&self) -> *const gt2_song {
        self.region.as_ptr().cast()
    }

    pub fn ptr_mut(&mut self) -> *mut gt2_song {
        self.region.as_mut_ptr().cast()
    }

    pub fn raw(&self) -> &gt2_song {
        unsafe { &*self.ptr() }
    }

    pub fn copy_from(&mut self, other: &Song) {
        assert_eq!(self.region.len(), other.region.len());
        self.region.copy_from_slice(&other.region);
    }

    pub fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.region.as_ptr().cast(), self.region.len() * 4) }
    }

    pub fn read(&mut self, file: &[u8], max_multispeed: u8) -> Result<ReadInfo, ReadInfo> {
        let data = file.to_vec();
        let caps = gt2_caps { max_multispeed };
        let mut info = gt2_read_info::default();
        let r = unsafe {
            gt2_sng_read(
                self.ptr_mut(),
                Some(read_slice),
                (&data as *const Vec<u8>).cast_mut().cast(),
                &caps,
                &mut info,
            )
        };
        let msg: Vec<u8> = info
            .msg
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        let out = ReadInfo {
            msg: String::from_utf8(msg).unwrap(),
            version: info.version,
            channels: info.channels,
            trailer: info.trailer as u32,
            gt2_bytes: info.gt2_bytes,
            file_bytes: info.file_bytes,
            need_bytes: info.need_bytes,
        };
        if r == 0 {
            Ok(out)
        } else {
            Err(out)
        }
    }

    pub fn write(&self, with_trailer: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let r = unsafe {
            gt2_sng_write(
                self.ptr(),
                with_trailer as i32,
                Some(write_vec),
                (&mut out as *mut Vec<u8>).cast(),
            )
        };
        assert_eq!(r, 0);
        out
    }

    pub fn check_plain(&self, file: &[u8]) -> (i32, String) {
        let data = file.to_vec();
        let mut msg = [0 as std::os::raw::c_char; 32];
        let r = unsafe {
            gt2_sng_check_plain(
                self.ptr(),
                Some(read_slice),
                (&data as *const Vec<u8>).cast_mut().cast(),
                msg.as_mut_ptr(),
            )
        };
        let m: Vec<u8> = msg
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        (r, String::from_utf8(m).unwrap())
    }

    pub fn order(&self, subtune: u32, channel: u32) -> Option<(Vec<u8>, u8)> {
        let p = unsafe { gt2_order_data(self.ptr(), subtune, channel) };
        if p.is_null() {
            return None;
        }
        let n = unsafe { gt2_order_entries(self.ptr(), subtune, channel) } as usize;
        let d = unsafe { std::slice::from_raw_parts(p, n + 2) };
        assert_eq!(d[n], 0xff);
        Some((d[..n].to_vec(), d[n + 1]))
    }

    pub fn pattern(&self, p: u32) -> Option<Vec<[u8; 4]>> {
        let d = unsafe { gt2_pattern_data(self.ptr(), p) };
        if d.is_null() {
            return None;
        }
        let rows = unsafe { gt2_pattern_rows(self.ptr(), p) } as usize;
        let d = unsafe { std::slice::from_raw_parts(d, rows * 4) };
        Some(d.chunks(4).map(|r| [r[0], r[1], r[2], r[3]]).collect())
    }

    pub fn set_string(&mut self, which: usize, v: &[u8; 32]) {
        let s = unsafe { &mut *self.ptr_mut() };
        let field = match which {
            0 => &mut s.name,
            1 => &mut s.author,
            _ => &mut s.copyright,
        };
        for (d, &b) in field.iter_mut().zip(v.iter()) {
            *d = b as _;
        }
    }

    pub fn ext_set(&mut self, tag: u32, data: &[u8]) -> i32 {
        unsafe { song_ext_set(self.ptr_mut(), tag, data.as_ptr(), data.len() as u32) }
    }

    pub fn ext_find(&self, tag: u32) -> Option<Vec<u8>> {
        let mut len = 0u32;
        let p = unsafe { song_ext_find(self.ptr(), tag, &mut len) };
        if p.is_null() {
            None
        } else {
            Some(unsafe { std::slice::from_raw_parts(p, len as usize) }.to_vec())
        }
    }

    pub fn ext_chunks(&self) -> Vec<(u32, Vec<u8>)> {
        let s = self.raw();
        let ext = &s.ext[..s.ext_used as usize];
        let mut out = Vec::new();
        let mut at = 0;
        while at < ext.len() {
            let t = u32::from_le_bytes(ext[at..at + 4].try_into().unwrap());
            let n = u32::from_le_bytes(ext[at + 4..at + 8].try_into().unwrap()) as usize;
            out.push((t, ext[at + 8..at + 8 + n].to_vec()));
            at += 8 + n;
        }
        out
    }

    pub fn bitmap(&self) -> Vec<bool> {
        let s = self.raw();
        let b = self.bytes();
        let off = s.bitmap_offset as usize;
        (0..s.arena_units as usize)
            .map(|u| b[off + u / 8] >> (u % 8) & 1 == 1)
            .collect()
    }
}

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

#[derive(Clone, Debug)]
pub struct SngSpec {
    pub ident: [u8; 4],
    pub strings: [[u8; 32]; 3],
    pub orders: Vec<Vec<(Vec<u8>, u8)>>,
    pub instr: Vec<[u8; 25]>,
    pub tables: Vec<Vec<(u8, u8)>>,
    pub patterns: Vec<(Vec<[u8; 4]>, [u8; 4])>,
}

impl SngSpec {
    pub fn new() -> SngSpec {
        SngSpec {
            ident: *b"GTS5",
            strings: [[0; 32]; 3],
            orders: vec![vec![(vec![0], 0), (vec![1], 0), (vec![2], 0)]],
            instr: Vec::new(),
            tables: vec![Vec::new(); 4],
            patterns: (0..3)
                .map(|_| (vec![[0xbd, 0, 0, 0]; 64], [0xff, 0, 0, 0]))
                .collect(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&self.ident);
        for s in &self.strings {
            f.extend_from_slice(s);
        }
        f.push(self.orders.len() as u8);
        for sub in &self.orders {
            for (entries, restart) in sub {
                f.push(entries.len() as u8 + 1);
                f.extend_from_slice(entries);
                f.push(0xff);
                f.push(*restart);
            }
        }
        f.push(self.instr.len() as u8);
        for i in &self.instr {
            f.extend_from_slice(i);
        }
        for t in &self.tables {
            f.push(t.len() as u8);
            f.extend(t.iter().map(|r| r.0));
            f.extend(t.iter().map(|r| r.1));
        }
        f.push(self.patterns.len() as u8);
        for (rows, end) in &self.patterns {
            f.push(rows.len() as u8 + 1);
            for r in rows {
                f.extend_from_slice(r);
            }
            f.extend_from_slice(end);
        }
        f
    }
}

impl Default for SngSpec {
    fn default() -> Self {
        SngSpec::new()
    }
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
