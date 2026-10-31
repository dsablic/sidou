// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use fm1_core::Bus;
use sidou_sys::ilp32;

use crate::common::{self, Firmware, Machine};

pub mod panel {
    pub const OCT_DOWN: u8 = 0;
    pub const OCT_UP: u8 = 1;
    pub const FX: u8 = 2;
    pub const SCL: u8 = 3;
    pub const ENV: u8 = 4;
    pub const LFO: u8 = 5;
    pub const EDIT: u8 = 6;
    pub const GLO: u8 = 7;
    pub const HOME: u8 = 8;
    pub const SAVE: u8 = 9;
    pub const ARP: u8 = 10;
    pub const SEQ: u8 = 11;
    pub const PLAY: u8 = 12;
    pub const REC: u8 = 13;
    pub const KEY0: u8 = 14;
    pub const SELECT: u8 = 0;
    pub const ALGORITHM: u8 = 1;
    pub const K1: u8 = 2;
    pub const K2: u8 = 3;
    pub const K3: u8 = 4;
    pub const K4: u8 = 5;
    pub const PRESETS: u8 = 6;
}

pub const FRAME_MS: u64 = 40;

pub fn key_of_note(n: u8) -> u8 {
    panel::KEY0 + n - 53
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Cell {
    pub ch: u8,
    pub attr: u8,
    pub src: u8,
    pub ovl: bool,
}

#[derive(Clone, Debug)]
pub struct Shot {
    pub transient: bool,
    pub fb: Vec<u16>,
    pub cells: Vec<Cell>,
    pub view: String,
    pub ms: u64,
}

impl Shot {
    pub fn cell(&self, col: usize, row: usize) -> Cell {
        self.cells[row * 30 + col]
    }
}

pub struct CapFrame {
    pub ms: u64,
    pub fb: Vec<u16>,
}

pub struct Recorder {
    pub frames: Vec<CapFrame>,
    pub next: u64,
    pub t0: u64,
    pub audio_from: usize,
    pub caption_marks: Vec<(u64, String)>,
}

pub struct Dev {
    pub m: Machine,
    pub fw: Firmware,
    pub rec: Option<Recorder>,
    names: Vec<(u32, u32, String)>,
}

fn symbols_with_sizes(fw: &Firmware) -> Vec<(u32, u32, String)> {
    use object::{Object, ObjectSymbol};
    let file = object::File::parse(&*fw.elf).expect("parse the firmware ELF");
    file.symbols()
        .filter(|s| s.size() > 0)
        .filter_map(|s| {
            Some((
                s.address() as u32,
                s.size() as u32,
                s.name().ok()?.to_string(),
            ))
        })
        .collect()
}

impl Dev {
    pub fn boot(build: &Path) -> Dev {
        let mut d = Dev::power_on(build);
        common::run_past_boot(&mut d.m, &d.fw);
        d.m.run_ms(300);
        d.mid_frame();
        d
    }

    pub fn power_on(build: &Path) -> Dev {
        let fw = Firmware::load(build);
        let m = Machine::boot(&fw.image);
        let names = symbols_with_sizes(&fw);
        Dev {
            m,
            fw,
            rec: None,
            names,
        }
    }

    pub fn power_on_with(build: &Path, songs: &[String], tunes: &[String]) -> Dev {
        let mut d = Dev::power_on(build);
        for name in songs {
            let bytes = showcase_song(name).expect("a showcase song");
            put_song(&mut d.m.soc.flash.mem, &store_name(name), &bytes);
        }
        let mut taken = std::collections::HashSet::new();
        for name in tunes {
            let bytes = starter_tune(name).expect("a starter tune");
            let h = sidou_sysex::sid::check(&bytes).expect("a starter tune checks");
            let title =
                sidou_sysex::sid::store_name(&sidou_sysex::sid::title(&h), name, &mut taken);
            put_file(
                &mut d.m.soc.flash.mem,
                &title,
                sidou_sys::STORE_FILE_SID as u8,
                &bytes,
            );
        }
        d
    }

    pub fn song(&mut self) -> sidou_tests::song::Song {
        let mut song = sidou_tests::song::Song::device();
        let base = self.fw.symbol("__song_region");
        let n = sidou_sys::SONG_REGION_BYTES as usize;
        let bytes: Vec<u8> = (0..n as u32).map(|i| self.m.soc.read8(base + i)).collect();
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), song.ptr_mut().cast::<u8>(), n);
        }
        song
    }

    pub fn pattern_notes(&mut self, v: u32, e: usize) -> Option<Vec<u8>> {
        let song = self.song();
        let (order, _) = song.order(0, v)?;
        let p = order
            .iter()
            .copied()
            .take_while(|x| u32::from(*x) != sidou_sys::GT2_LOOPSONG)
            .filter(|x| u32::from(*x) < sidou_sys::GT2_REPEAT)
            .nth(e)?;
        let rows = song.pattern(u32::from(p))?;
        Some(
            rows.iter()
                .map(|r| {
                    let n = u32::from(r[0]);
                    if (sidou_sys::GT2_FIRSTNOTE..=sidou_sys::GT2_LASTNOTE).contains(&n) {
                        r[0]
                    } else {
                        0
                    }
                })
                .collect(),
        )
    }

    pub fn song_row(&mut self) -> Option<u8> {
        let r = self.m.soc.read8(self.fw.symbol("song_seen") + 9);
        (r != 0xff).then_some(r)
    }

    pub fn sync_to_pattern(&mut self, limit: u64) -> bool {
        let end = self.ms() + limit;
        let mut last = self.song_row();
        while self.ms() < end {
            self.run(1);
            let now = self.song_row();
            if now == Some(0) && last != Some(0) {
                return true;
            }
            last = now;
        }
        false
    }

    pub fn wait_boot(&mut self) -> u64 {
        let t = common::run_past_boot(&mut self.m, &self.fw);
        self.m.run_ms(100);
        t
    }

    pub fn ms(&self) -> u64 {
        self.m.ms()
    }

    pub fn run(&mut self, ms: u64) {
        if self.rec.is_none() {
            self.m.run_ms(ms);
            return;
        }
        let end = self.m.ms() + ms;
        while self.m.ms() < end {
            let next = self.rec.as_ref().unwrap().next;
            let step = (end - self.m.ms()).min(next.saturating_sub(self.m.ms()).max(1));
            self.m.run_ms(step);
            let now = self.m.ms();
            let r = self.rec.as_mut().unwrap();
            if now >= r.next {
                r.frames.push(CapFrame {
                    ms: now,
                    fb: self.m.soc.lcd.fb.clone(),
                });
                r.next += FRAME_MS;
            }
        }
    }

    pub fn mid_frame(&mut self) {
        let (last, now) = (self.fw.symbol("last_compose"), self.fw.symbol("uptime_ms"));
        for _ in 0..80 {
            let age = self.m.soc.read32(now).wrapping_sub(self.m.soc.read32(last));
            if age == 20 {
                return;
            }
            self.run(1);
        }
    }

    pub fn start_capture(&mut self) {
        self.mid_frame();
        let t0 = self.m.ms();
        self.rec = Some(Recorder {
            frames: vec![CapFrame {
                ms: t0,
                fb: self.m.soc.lcd.fb.clone(),
            }],
            next: t0 + FRAME_MS,
            t0,
            audio_from: self.m.soc.periph.audio.len() / 2,
            caption_marks: Vec::new(),
        });
    }

    pub fn stop_capture(&mut self) -> Option<Recorder> {
        self.rec.take()
    }

    pub fn key_down(&mut self, id: u8) {
        self.m.key(id, true);
    }

    pub fn key_up(&mut self, id: u8) {
        self.m.key(id, false);
    }

    pub fn press(&mut self, id: u8) {
        self.m.key(id, true);
        self.run(30);
        self.m.key(id, false);
        self.run(30);
    }

    pub fn turn(&mut self, enc: u8, n: i32) {
        let mut left = n;
        while left != 0 {
            let c = left.clamp(-64, 64);
            self.m.turn(enc, c);
            self.run(10 * u64::from(c.unsigned_abs()));
            left -= c;
        }
    }

    pub fn note(&mut self, key: u8, ms: u64) {
        self.m.key(key, true);
        self.run(ms);
        self.m.key(key, false);
        self.run(30);
    }

    pub fn symbol_name(&self, addr: u32) -> String {
        self.names
            .iter()
            .find(|(a, s, _)| (*a..*a + *s).contains(&addr))
            .map(|(_, _, n)| n.clone())
            .unwrap_or_else(|| format!("view@{addr:#x}"))
    }

    pub fn view(&mut self) -> String {
        let ui = self.fw.symbol("ui");
        let depth = u32::from(self.m.soc.read8(ui + ilp32::screen::depth as u32));
        if depth == 0 {
            return "off".to_string();
        }
        let top = self
            .m
            .soc
            .read32(ui + ilp32::screen::stack as u32 + 8 * (depth - 1));
        let n = self.symbol_name(top);
        n.trim_end_matches("_view")
            .trim_start_matches("views_")
            .to_string()
    }

    pub fn depth(&mut self) -> u8 {
        self.m
            .soc
            .read8(self.fw.symbol("ui") + ilp32::screen::depth as u32)
    }

    pub fn animating(&mut self) -> bool {
        let ui = self.fw.symbol("ui");
        self.m.soc.read8(ui + ilp32::screen::wipe as u32) != 0
            || self.m.soc.read8(ui + ilp32::screen::raster_frames as u32) != 0
    }

    pub fn popup_rows(&mut self) -> u8 {
        let ui = self.fw.symbol("ui");
        let p = self.m.soc.read32(ui + ilp32::screen::popup as u32);
        if p == 0 {
            return 0;
        }
        self.m.soc.read8(p + ilp32::screen_popup_state::rows as u32)
    }

    pub fn toast_on(&mut self) -> bool {
        let ui = self.fw.symbol("ui");
        self.m.soc.read8(ui + ilp32::screen::toast_on as u32) != 0
    }

    pub fn settle(&mut self) {
        let mut guard = 0;
        self.run(FRAME_MS);
        while self.animating() && guard < 80 {
            self.run(FRAME_MS);
            guard += 1;
        }
        self.mid_frame();
    }

    fn grid_addr(&mut self) -> u32 {
        self.m.soc.read32(self.fw.symbol("ui"))
    }

    pub fn shot(&mut self) -> Shot {
        let g = self.grid_addr();
        if g == 0 {
            return Shot {
                transient: false,
                fb: self.m.soc.lcd.fb.clone(),
                cells: vec![Cell::default(); 900],
                view: "off".to_string(),
                ms: self.m.ms(),
            };
        }
        let n = 900u32;
        let mut cells = Vec::with_capacity(n as usize);
        for i in 0..n {
            cells.push(Cell {
                ch: self.m.soc.read8(g + ilp32::grid::shown_ch as u32 + i),
                attr: self.m.soc.read8(g + ilp32::grid::shown_attr as u32 + i),
                src: self.m.soc.read8(g + ilp32::grid::src as u32 + i),
                ovl: self.m.soc.read8(g + ilp32::grid::ovl as u32 + i) != 0,
            });
        }
        let view = self.view();
        Shot {
            transient: self.animating(),
            fb: self.m.soc.lcd.fb.clone(),
            cells,
            view,
            ms: self.m.ms(),
        }
    }

    pub fn live_addr(&mut self) -> u32 {
        self.m
            .soc
            .read32(self.fw.symbol("ui") + ilp32::views::l as u32)
    }

    fn menu_addr(&mut self) -> u32 {
        let field = if self.view() == "settings" {
            ilp32::views::setup
        } else {
            ilp32::views::home
        };
        self.fw.symbol("ui") + field as u32
    }

    pub fn menu_cursor(&mut self) -> (u8, u8) {
        let home = self.menu_addr();
        (
            self.m.soc.read8(home + ilp32::menu::cursor as u32),
            self.m.soc.read8(home + ilp32::menu::top as u32),
        )
    }

    pub fn menu_count(&mut self) -> u8 {
        let home = self.menu_addr();
        self.m.soc.read8(home + ilp32::menu::count as u32)
    }

    pub fn settings_bytes(&mut self) -> Vec<u8> {
        let ui = self.fw.symbol("ui");
        let route = self.m.soc.read32(ui + ilp32::views::route as u32);
        let mut out: Vec<u8> = (0..ilp32::midi_route_setup::SIZE as u32)
            .map(|i| {
                self.m
                    .soc
                    .read8(route + ilp32::midi_route::setup as u32 + i)
            })
            .collect();
        out.push(self.live_u8(ilp32::live::theme));
        out.push(self.live_u8(ilp32::live::keys_mode));
        out.push(self.m.soc.read8(ui + ilp32::views::cards as u32));
        out
    }

    pub fn live_u8(&mut self, off: usize) -> u8 {
        let l = self.live_addr();
        self.m.soc.read8(l + off as u32)
    }

    pub fn live_u16(&mut self, off: usize) -> u16 {
        let l = self.live_addr();
        let lo = self.m.soc.read8(l + off as u32);
        let hi = self.m.soc.read8(l + off as u32 + 1);
        u16::from(lo) | u16::from(hi) << 8
    }

    pub fn audio_since(&self, from: usize) -> Vec<f32> {
        let all = self.m.left();
        all[from.min(all.len())..]
            .iter()
            .map(|&s| f32::from(s))
            .collect()
    }

    pub fn audio_mark(&self) -> usize {
        self.m.soc.periph.audio.len() / 2
    }

    pub fn image_has(&self, needle: &[u8]) -> bool {
        self.fw.image.windows(needle.len()).any(|w| w == needle)
    }

    pub fn has_symbol(&self, name: &str) -> bool {
        self.names.iter().any(|(_, _, n)| n == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Params {
    pub cutoff: u16,
    pub resonance: u8,
    pub pulse_width: u16,
    pub master: u16,
    pub preset: u8,
    pub octave: i8,
    pub model: u8,
    pub profile: u8,
    pub fx_select: u8,
    pub decay: u8,
    pub release: u8,
    pub sync_semis: i8,
    pub character: [u8; 4],
    pub slot: [[u8; 5]; 3],
    pub arp: [u8; 4],
    pub lab: [i32; 12],
}

const LAB_FIELDS: [&str; 12] = [
    "attack",
    "sustain",
    "waveform",
    "filter_voices",
    "filter_mode",
    "pwm_depth",
    "pwm_rate",
    "tune",
    "velocity_target",
    "vibrato_rate",
    "vibrato_depth",
    "vibrato_delay",
];

impl Params {
    pub fn named(&self) -> Vec<(String, i64)> {
        let mut v = vec![
            ("cutoff".to_string(), i64::from(self.cutoff)),
            ("resonance".to_string(), i64::from(self.resonance)),
            ("pulse_width".to_string(), i64::from(self.pulse_width)),
            ("master".to_string(), i64::from(self.master)),
            ("preset".to_string(), i64::from(self.preset)),
            ("octave".to_string(), i64::from(self.octave)),
            ("model".to_string(), i64::from(self.model)),
            ("profile".to_string(), i64::from(self.profile)),
            ("fx_select".to_string(), i64::from(self.fx_select)),
            ("decay".to_string(), i64::from(self.decay)),
            ("release".to_string(), i64::from(self.release)),
            ("sync_semis".to_string(), i64::from(self.sync_semis)),
        ];
        for (i, c) in ["age", "dac", "thump", "bleed"].iter().enumerate() {
            v.push(((*c).to_string(), i64::from(self.character[i])));
        }
        for (s, name) in ["dist", "mod", "fx"].iter().enumerate() {
            for (f, field) in ["algorithm", "amount", "a", "b", "mix"].iter().enumerate() {
                v.push((format!("{name}.{field}"), i64::from(self.slot[s][f])));
            }
        }
        for (i, a) in ["arp_on", "arp_rate", "arp_order", "arp_span"]
            .iter()
            .enumerate()
        {
            v.push(((*a).to_string(), i64::from(self.arp[i])));
        }
        for (i, a) in LAB_FIELDS.iter().enumerate() {
            v.push(((*a).to_string(), i64::from(self.lab[i])));
        }
        v
    }

    pub fn diff(&self, other: &Params) -> Vec<String> {
        self.named()
            .iter()
            .zip(other.named())
            .filter(|(a, b)| a.1 != b.1)
            .map(|(a, _)| a.0.clone())
            .collect()
    }
}

impl Dev {
    pub fn params(&mut self) -> Params {
        let l = self.live_addr();
        if l == 0 {
            return Params::default();
        }
        let mut p = Params {
            cutoff: self.live_u16(ilp32::live::cutoff),
            resonance: self.live_u8(ilp32::live::resonance),
            pulse_width: self.live_u16(ilp32::live::pulse_width),
            master: self.live_u16(ilp32::live::master),
            preset: self.live_u8(ilp32::live::preset),
            octave: self.live_u8(ilp32::live::octave) as i8,
            model: self.live_u8(ilp32::live::model),
            profile: self.live_u8(ilp32::live::profile),
            fx_select: self.live_u8(ilp32::live::fx_select),
            decay: self.live_u8(ilp32::live::attack_decay) & 15,
            release: self.live_u8(ilp32::live::sustain_release) & 15,
            sync_semis: self.live_u8(ilp32::live::sync_semis) as i8,
            ..Params::default()
        };
        for i in 0..4 {
            p.character[i] = self.live_u8(ilp32::live::character + i);
        }
        for s in 0..3 {
            for f in 0..5 {
                p.slot[s][f] = self.live_u8(ilp32::live::slot + s * 5 + f);
            }
        }
        p.arp = [
            self.live_u8(ilp32::live::arp_on),
            self.live_u8(ilp32::live::arp_rate),
            self.live_u8(ilp32::live::arp_order),
            self.live_u8(ilp32::live::arp_span),
        ];
        let (ad, sr) = (
            self.live_u8(ilp32::live::attack_decay),
            self.live_u8(ilp32::live::sustain_release),
        );
        p.lab = [
            i32::from(ad >> 4),
            i32::from(sr >> 4),
            i32::from(self.live_u8(ilp32::live::control)),
            i32::from(self.live_u8(ilp32::live::filter_voices)),
            i32::from(self.live_u8(ilp32::live::filter_mode)),
            i32::from(self.live_u16(ilp32::live::pwm_depth)),
            (i32::from(self.live_u16(ilp32::live::pwm_rate)) * 1000 + 16384) / 32768,
            i32::from(self.live_u8(ilp32::live::tune) as i8),
            i32::from(self.live_u8(ilp32::live::velocity_target)),
            i32::from(self.live_u8(ilp32::live::vibrato_rate)),
            i32::from(self.live_u8(ilp32::live::vibrato_depth)),
            i32::from(self.live_u8(ilp32::live::vibrato_delay)),
        ];
        p
    }

    pub fn power_cycle(&mut self) {
        let flash = self.m.soc.flash.mem.clone();
        let mut m = Machine::boot_blank(&self.fw.image);
        m.soc.flash.mem.copy_from_slice(&flash);
        self.m = m;
        self.rec = None;
        common::run_past_boot(&mut self.m, &self.fw);
        self.m.run_ms(100);
        self.mid_frame();
    }
}

pub fn showcase_song(name: &str) -> Result<Vec<u8>, String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(format!(
            "song {name}: a file name of assets/songs, without .sng"
        ));
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../assets/songs")
        .join(format!("{name}.sng"));
    std::fs::read(&path).map_err(|e| format!("song {name}: {}: {e}", path.display()))
}

pub fn starter_tune(name: &str) -> Result<Vec<u8>, String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(format!(
            "tune {name}: a file name of assets/starter, without .sid"
        ));
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../assets/starter")
        .join(format!("{name}.sid"));
    std::fs::read(&path).map_err(|e| format!("tune {name}: {}: {e}", path.display()))
}

pub fn store_name(name: &str) -> String {
    name.replace('-', " ").to_uppercase()
}

unsafe extern "C" fn flash_map(ctx: *mut std::ffi::c_void, off: u32) -> *const u8 {
    unsafe { ctx.cast::<u8>().add(off as usize) }
}

unsafe extern "C" fn flash_prog(
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

unsafe extern "C" fn flash_erase(ctx: *mut std::ffi::c_void, off: u32) -> i32 {
    for i in 0..sidou_sys::STORE_SECTOR as usize {
        unsafe { *ctx.cast::<u8>().add(off as usize + i) = 0xff };
    }
    sidou_sys::STORE_OK
}

pub fn put_song(flash: &mut [u8], name: &str, bytes: &[u8]) {
    put_file(flash, name, sidou_sys::STORE_FILE_SONG as u8, bytes);
}

pub fn put_file(flash: &mut [u8], name: &str, file_type: u8, bytes: &[u8]) {
    use sidou_sys::*;
    let hooks = store_flash {
        map: Some(flash_map),
        prog: Some(flash_prog),
        erase: Some(flash_erase),
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
            store_file_begin(&mut *st, name.as_ptr(), file_type, bytes.len() as u32),
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
