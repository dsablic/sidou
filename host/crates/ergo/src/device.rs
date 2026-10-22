// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The firmware in fm1-emu as the harness drives it: the panel's inputs, the screen read from
//! the firmware's own grid (app/grid.h) and the live instrument's parameters, and an optional
//! recorder of every LCD frame. Built on emu-gate's `common` (boot, key, turn, run_ms).

use std::path::Path;

use fm1_core::Bus;
use sidou_sys::ilp32;

use crate::common::{self, Firmware, Machine};

/// app/panel.h: the buttons, the encoders (K1..K4 are 2..5).
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
    /// The id of note key 0 (F3, MIDI 53); a MIDI note n is id `KEY0 + n - 53`.
    pub const KEY0: u8 = 14;
    pub const SELECT: u8 = 0;
    pub const ALGORITHM: u8 = 1;
    pub const K1: u8 = 2;
    pub const K2: u8 = 3;
    pub const K3: u8 = 4;
    pub const K4: u8 = 5;
    pub const PRESETS: u8 = 6;
}

/// The frame period (platform/fm1/ui.c: 25 fps).
pub const FRAME_MS: u64 = 40;

/// The key id of MIDI note `n` (F3..G5 only).
pub fn key_of_note(n: u8) -> u8 {
    panel::KEY0 + n - 53
}

/// A cell of the shown frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Cell {
    pub ch: u8,
    pub attr: u8,
    /// The character a large-text piece is a piece of.
    pub src: u8,
    /// A pixel overlay is on the cell (a scope).
    pub ovl: bool,
}

/// The screen as the panel shows it.
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

/// One frame of a full capture.
pub struct CapFrame {
    pub ms: u64,
    pub fb: Vec<u16>,
}

/// A full capture in progress: every frame boundary's LCD, and where the audio starts.
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
    /// Booted as from the second power-on, past the boot and its wipe, settled.
    pub fn boot(build: &Path) -> Dev {
        let mut d = Dev::power_on(build);
        common::run_past_boot(&mut d.m, &d.fw);
        d.m.run_ms(300);
        d.mid_frame();
        d
    }

    /// Powered on, as from the second power-on (the settings stored), at time 0.
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

    /// Powered on as `power_on`, with these showcase songs (assets/songs/<name>.sng) in the
    /// store, each under its title (`chrome-comet` is CHROME COMET).
    pub fn power_on_with(build: &Path, songs: &[String]) -> Dev {
        let mut d = Dev::power_on(build);
        for name in songs {
            let bytes = showcase_song(name).expect("a showcase song");
            put_song(&mut d.m.soc.flash.mem, &store_name(name), &bytes);
        }
        d
    }

    /// The song as the device holds it now (its model region).
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

    /// The rows (note value each, 0 for none) of the pattern voice `v` (0..2) plays at its
    /// orderlist's pattern entry `e` (subtune 0; repeats and transposes are not entries).
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

    /// The row voice 1 of the playing song is on (main.c's `song_seen`), or None before one.
    pub fn song_row(&mut self) -> Option<u8> {
        // song_status: state, multispeed, ntsc, mute, speed[2], order_pos[3], row[3].
        let r = self.m.soc.read8(self.fw.symbol("song_seen") + 9);
        (r != 0xff).then_some(r)
    }

    /// Waits until voice 1 of the playing song starts a pattern (its row goes back to 0),
    /// at most `limit` ms; true when it did.
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

    /// Waits for the boot to end by itself; returns the milliseconds it took.
    pub fn wait_boot(&mut self) -> u64 {
        let t = common::run_past_boot(&mut self.m, &self.fw);
        self.m.run_ms(100);
        t
    }

    pub fn ms(&self) -> u64 {
        self.m.ms()
    }

    /// Runs `ms` emulated milliseconds, saving frames while a full capture runs.
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

    /// To half way between two UI frames (a whole frame is on the panel).
    pub fn mid_frame(&mut self) {
        // An idle screen composes nothing, so its last frame never gets 20 ms old: after 80 ms
        // of looking, take the phase as it is.
        let (last, now) = (self.fw.symbol("last_compose"), self.fw.symbol("uptime_ms"));
        for _ in 0..80 {
            let age = self.m.soc.read32(now).wrapping_sub(self.m.soc.read32(last));
            if age == 20 {
                return;
            }
            self.run(1);
        }
    }

    /// Starts a full capture: frames every `FRAME_MS` from now (half way between frames).
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

    /// A button pressed and let go, long enough for the key scan's debounce.
    pub fn press(&mut self, id: u8) {
        self.m.key(id, true);
        self.run(30);
        self.m.key(id, false);
        self.run(30);
    }

    /// An encoder turned by `n` detents (fm1-emu takes 8 ms a detent), waited out in steps of
    /// at most 64 detents.
    pub fn turn(&mut self, enc: u8, n: i32) {
        let mut left = n;
        while left != 0 {
            let c = left.clamp(-64, 64);
            self.m.turn(enc, c);
            self.run(10 * u64::from(c.unsigned_abs()));
            left -= c;
        }
    }

    /// A note key held for `ms`, then let go.
    pub fn note(&mut self, key: u8, ms: u64) {
        self.m.key(key, true);
        self.run(ms);
        self.m.key(key, false);
        self.run(30);
    }

    /// The name of the symbol holding `addr`.
    pub fn symbol_name(&self, addr: u32) -> String {
        self.names
            .iter()
            .find(|(a, s, _)| (*a..*a + *s).contains(&addr))
            .map(|(_, _, n)| n.clone())
            .unwrap_or_else(|| format!("view@{addr:#x}"))
    }

    /// The view on top of the screen's stack, named after its table's symbol (`live_view_view`
    /// is `live`, `views_menu` is `menu`): the names follow the firmware, not a list here.
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

    /// The depth of the screen's view stack.
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

    /// Rows of the value popup that are drawn now (0: closed).
    pub fn popup_rows(&mut self) -> u8 {
        let ui = self.fw.symbol("ui");
        let p = self.m.soc.read32(ui + ilp32::screen::popup as u32);
        if p == 0 {
            return 0;
        }
        self.m.soc.read8(p + ilp32::screen_popup_state::rows as u32)
    }

    /// Whether a toast is on the screen.
    pub fn toast_on(&mut self) -> bool {
        let ui = self.fw.symbol("ui");
        self.m.soc.read8(ui + ilp32::screen::toast_on as u32) != 0
    }

    /// Runs until any wipe or raster is over, then to half way between frames.
    pub fn settle(&mut self) {
        let mut guard = 0;
        self.run(FRAME_MS);
        while self.animating() && guard < 80 {
            self.run(FRAME_MS);
            guard += 1;
        }
        self.mid_frame();
    }

    /// The grid struct's address (the screen's first field points to it).
    fn grid_addr(&mut self) -> u32 {
        self.m.soc.read32(self.fw.symbol("ui"))
    }

    /// The screen as shown: the panel's pixels, and the grid's cells.
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

    /// The menu whose page shows: SETTINGS' (views.setup) while it shows, else the main one's
    /// (views.home).
    fn menu_addr(&mut self) -> u32 {
        let field = if self.view() == "settings" {
            ilp32::views::setup
        } else {
            ilp32::views::home
        };
        self.fw.symbol("ui") + field as u32
    }

    /// The menu's own cursor and first visible item, read from the firmware's RAM (views.home,
    /// or views.setup on SETTINGS).
    pub fn menu_cursor(&mut self) -> (u8, u8) {
        let home = self.menu_addr();
        (
            self.m.soc.read8(home + ilp32::menu::cursor as u32),
            self.m.soc.read8(home + ilp32::menu::top as u32),
        )
    }

    /// How many items the menu (or SETTINGS) has.
    pub fn menu_count(&mut self) -> u8 {
        let home = self.menu_addr();
        self.m.soc.read8(home + ilp32::menu::count as u32)
    }

    /// What the menu persists, as bytes: the MIDI setup, the theme, KEYS and CARDS. A journey that
    /// is meant to leave the settings alone compares this before and after.
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

    /// The left channel played since sample `from`.
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

    /// Whether the firmware image holds `needle` (a string, to tell if a feature is on main).
    pub fn image_has(&self, needle: &[u8]) -> bool {
        self.fw.image.windows(needle.len()).any(|w| w == needle)
    }

    pub fn has_symbol(&self, name: &str) -> bool {
        self.names.iter().any(|(_, _, n)| n == name)
    }
}

/// The live instrument's parameters as the firmware holds them (app/live.h `live`).
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
    /// K3's and K4's envelope nibbles (attack/decay's decay, sustain/release's release) and the
    /// sync ratio's semitones.
    pub decay: u8,
    pub release: u8,
    pub sync_semis: i8,
    pub character: [u8; 4],
    /// Each slot: algorithm, amount, A, B, mix.
    pub slot: [[u8; 5]; 3],
}

impl Params {
    /// Name and value of every field, for diffing.
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
        v
    }

    /// The names of the fields that differ.
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
        p
    }

    /// Powers off and on again over the same flash: what a save left is what loads.
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

/// A showcase song's file, assets/songs/<name>.sng.
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

/// The store name a showcase song goes under: `chrome-comet` is CHROME COMET.
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

/// Writes a song file into a flash image's store, as the device's SAVE would (emu-gate's
/// song_store.rs does the same for its fixtures).
pub fn put_song(flash: &mut [u8], name: &str, bytes: &[u8]) {
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
            store_file_begin(
                &mut *st,
                name.as_ptr(),
                STORE_FILE_SONG as u8,
                bytes.len() as u32
            ),
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
