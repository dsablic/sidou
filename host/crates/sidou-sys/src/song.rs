// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::raw::c_void;

use crate::*;

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
