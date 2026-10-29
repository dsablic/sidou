// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Unit tests of the portable app logic (`app/`) through FFI.

mod about;
mod boot;
mod bootguard;
mod chip_mix;
mod chip_spread;
mod chip_view;
mod credits;
mod demo_song;
mod expression;
mod font;
mod fx_live;
mod grid;
mod gt2_ins;
mod help_view;
mod keys_route;
mod lab_edit;
mod lab_sound;
mod lab_view;
mod learn_view;
mod live_arp;
mod live_cutoff;
mod live_dice;
mod live_freq;
mod live_instrument;
mod live_view;
mod logo;
mod logo_bitmap;
mod master_gain;
mod midi_clock;
mod midi_in;
mod midi_learn;
mod midi_map;
mod midi_out;
mod midi_ring;
mod midi_route;
mod midi_rx;
mod midi_sync;
mod midi_usb;
mod panel;
mod popup;
mod reg_ring;
mod save_key;
mod scene;
mod scene_art;
mod scene_px;
mod scene_rmr;
mod scene_rmr_drawing;
mod scene_song;
mod scope;
mod screen;
mod settings;
mod sid_io;
mod sid_view;
mod song_audition_shape;
mod song_edit;
mod song_gen;
mod song_io;
mod song_leds;
mod song_order_locks;
mod song_pages;
mod song_pattern_at;
mod song_rec;
mod song_view;
mod step_view;
mod store;
mod store_layout;
mod sysex;
mod tail_gate;
mod theme;
mod thump_out;
mod title_card;
mod title_card_bitmap;
mod tracker_view;
mod ui_sound;
mod user_profile;
mod user_sound;
mod views;
mod widget;

use std::ffi::c_void;

use sidou_sys::*;

/// SCENE's PETSCII piece: the view's union holds one unless RED MOON RITE runs.
pub fn petscii(sv: &scene_view) -> &sidou_sys::scene {
    unsafe { &sv.u.piece }
}

pub fn petscii_mut(sv: &mut scene_view) -> &mut sidou_sys::scene {
    unsafe { &mut sv.u.piece }
}

/// The part of a character a grid cell's piece code draws (app/grid.h GRID_BIG_FIRST): its
/// scale (2, 3, or 1 for a plain glyph) and the cell's place in it (y * scale + x).
pub(crate) fn piece_part(piece: usize) -> (usize, usize) {
    match piece {
        0..=3 => (2, piece),
        4..=12 => (3, piece - 4),
        _ => (1, 0),
    }
}

/// A register sink that records every write and mirrors the register file. The character's
/// tick (LIVE_REG_CHAR_TICK, sent every live_tick) is counted, not logged.
#[derive(Default)]
pub struct Recorder {
    pub log: Vec<(u8, u8)>,
    pub regs: [u8; 0x19],
    pub char_ticks: u32,
}

pub(crate) unsafe extern "C" fn record(ctx: *mut c_void, reg: u8, value: u8) {
    let r = unsafe { &mut *ctx.cast::<Recorder>() };
    if u32::from(reg) == LIVE_REG_CHAR_TICK {
        r.char_ticks += 1;
        return;
    }
    r.log.push((reg, value));
    if usize::from(reg) < r.regs.len() {
        r.regs[usize::from(reg)] = value;
    }
}

/// A `live` writing into a [`Recorder`]. Both are boxed: C keeps pointers to them.
pub struct Rig {
    pub live: Box<live>,
    pub rec: Box<Recorder>,
}

impl Rig {
    pub fn new(model: sid_model) -> Self {
        let mut rec = Box::<Recorder>::default();
        let mut l = Box::<live>::default();
        let ctx: *mut Recorder = &mut *rec;
        unsafe { live_init(&mut *l, Some(record), ctx.cast(), model) };
        Rig { live: l, rec }
    }

    /// Forgets what was written so far.
    pub fn clear(&mut self) {
        self.rec.log.clear();
    }

    pub fn log(&self) -> &[(u8, u8)] {
        &self.rec.log
    }

    pub fn reg(&self, reg: u8) -> u8 {
        self.rec.regs[usize::from(reg)]
    }

    pub fn voice_freq(&self, v: u8) -> u16 {
        u16::from(self.reg(v * 7)) | u16::from(self.reg(v * 7 + 1)) << 8
    }

    pub fn voice_pw(&self, v: u8) -> u16 {
        u16::from(self.reg(v * 7 + 2)) | u16::from(self.reg(v * 7 + 3) & 0x0f) << 8
    }

    pub fn control(&self, v: u8) -> u8 {
        self.reg(v * 7 + 4)
    }

    pub fn gated(&self, v: u8) -> bool {
        self.control(v) & 1 != 0
    }

    pub fn note_on(&mut self, note: u8) {
        unsafe { live_note_on(&mut *self.live, note, 100) }
    }

    pub fn note_off(&mut self, note: u8) {
        unsafe { live_note_off(&mut *self.live, note) }
    }

    /// The preset stepped, as PRESETS and the menu's PRE do.
    pub fn preset_step(&mut self, steps: i32) {
        unsafe { live_preset_step(&mut *self.live, steps, 0) };
    }

    /// The pulse width moved by `detents` of 32 (what K3 turned before it became the sound's own
    /// control): a test helper, through the parameter.
    pub fn pulse(&mut self, detents: i32) {
        let pw = (i64::from(self.live.pulse_width) + i64::from(detents) * 32).clamp(0, 4095);
        unsafe { live_param_set(&mut *self.live, LIVE_PARAM_PULSE_WIDTH, pw as u32) };
    }

    pub fn knob(&mut self, knob: live_knob_id, delta: i32) {
        unsafe { live_knob(&mut *self.live, knob, delta) }
    }

    pub fn button(&mut self, button: live_button_id) {
        unsafe { live_button(&mut *self.live, button) }
    }

    pub fn cc(&mut self, cc: u32, value: u8) {
        unsafe { live_cc(&mut *self.live, cc as u8, value) }
    }

    pub fn set_master(&mut self, value: u16) {
        unsafe { live_set_master(&mut *self.live, value) }
    }

    pub fn set_preset(&mut self, index: u8) {
        unsafe { live_set_preset(&mut *self.live, index) }
    }

    pub fn tick(&mut self) {
        unsafe { live_tick(&mut *self.live) }
    }

    pub fn key_note(&self, base: u8) -> u8 {
        unsafe { live_key_note(&*self.live, base) }
    }
}

pub fn preset(index: u8) -> &'static live_preset {
    let p = unsafe { live_preset_get(index) };
    assert!(!p.is_null(), "preset {index} missing");
    unsafe { &*p }
}

pub fn preset_name(p: &live_preset) -> String {
    unsafe { std::ffi::CStr::from_ptr(p.name) }
        .to_str()
        .unwrap()
        .to_owned()
}

/// Where K1 leaves a cutoff after `steps` detents (K1's walk, live.h): from the table's entry that
/// holds it, `steps` entries on; from any other register, the first entry past it in the turn's
/// direction (by the buckets of live_cutoff_up and _down) and the rest of the steps from there.
pub fn cutoff_after(model: u32, cutoff: u16, steps: i32) -> u16 {
    let t = unsafe { live_cutoff_steps }[model as usize];
    let top = t.len() as i32 - 1;
    let pos = match t.iter().position(|&e| e == cutoff) {
        Some(i) => i as i32,
        None if steps > 0 => {
            i32::from(unsafe { live_cutoff_up }[model as usize][usize::from(cutoff >> 5)]) - 1
        }
        None => {
            i32::from(unsafe { live_cutoff_down }[model as usize][usize::from(cutoff >> 5)]) + 1
        }
    };
    let e = t[(pos + steps).clamp(0, top) as usize];
    if (steps > 0 && e > cutoff) || (steps < 0 && e < cutoff) {
        e
    } else {
        cutoff
    }
}

pub fn freq_table() -> [u16; 128] {
    unsafe { live_freq_table }
}

/// The panel as palette indexes, 240 x 240, from the composed grid: each row of cells as
/// grid_render_run draws it (glyphs, overlays and the pixel region), with the palette mapping
/// each colour to itself.
pub fn pixels(g: &sidou_sys::grid) -> Vec<u8> {
    let cols = GRID_COLS as usize;
    let w = cols * 8;
    let pal: [u16; 16] = std::array::from_fn(|c| c as u16);
    let mut out = vec![0u8; w * w];
    // 4-byte aligned, as grid_render_run takes it.
    let mut buf = vec![0u32; GRID_RUN_PIXELS as usize / 2];
    for row in 0..cols {
        let run = grid_run {
            row: row as u8,
            col: 0,
            len: cols as u8,
        };
        let px = buf.as_mut_ptr().cast::<u16>();
        unsafe { grid_render_run(g, &run, pal.as_ptr(), px) };
        let px = unsafe { std::slice::from_raw_parts(px, GRID_RUN_PIXELS as usize) };
        for (o, &p) in out[row * 8 * w..(row + 1) * 8 * w].iter_mut().zip(px) {
            *o = p as u8;
        }
    }
    out
}

/// What one drain of a grid's changed runs sent.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Sent {
    pub windows: usize,
    pub cells: usize,
}

/// platform/fm1/lcd.c: SPI1 at 60 MHz / (4 + 1); lcd.h: 11 command bytes a window.
pub const SPI_HZ: f64 = 12e6;
pub const WINDOW_BYTES: usize = 11;
/// docs/art/README.md: at most about 20 ms of SPI per 40 ms frame.
pub const SPI_BUDGET_MS: f64 = 20.0;

impl Sent {
    pub fn bytes(self) -> usize {
        self.cells * 128 + self.windows * WINDOW_BYTES
    }

    pub fn spi_ms(self) -> f64 {
        self.bytes() as f64 * 8.0 / SPI_HZ * 1e3
    }
}

/// Every changed run of g, counted as shown, as platform/fm1/ui.c sends them.
pub fn drain(g: &mut sidou_sys::grid) -> Sent {
    let mut sent = Sent::default();
    let mut run = grid_run::default();
    let pal = [0u16; 16];
    let mut px = vec![0u32; GRID_RUN_PIXELS as usize / 2]; // 4-byte aligned
    while unsafe { grid_next_run(g, &mut run) } != 0 {
        // Rendered, as platform/fm1/ui.c renders every run it is handed: a painter region
        // (RED MOON RITE's) takes each tile's hash as it is painted (app/scene_px.h).
        unsafe { grid_render_run(g, &run, pal.as_ptr(), px.as_mut_ptr().cast()) };
        sent.windows += 1;
        sent.cells += usize::from(run.len);
    }
    sent
}

/// Cell row `row` of a grid as text, '#' for codes outside printable ASCII.
pub fn grid_row(g: &sidou_sys::grid, row: usize) -> String {
    let cols = GRID_COLS as usize;
    (0..cols)
        .map(|c| g.ch[row * cols + c])
        .map(|b| {
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '#'
            }
        })
        .collect()
}

/// A wipe's frames: each sends its SCREEN_WIPE_ROWS new rows and its edge row at most
/// (app/screen.h), so the view under it must hold still. Runs frames with `frame` until a wipe
/// has started and ended (at most 40 frames), and returns what each frame of it sent.
pub fn wipe_frames(frame: &mut dyn FnMut() -> (Sent, bool)) -> Vec<Sent> {
    let mut out = Vec::new();
    let mut was = false;
    for _ in 0..40 {
        let (sent, wiping) = frame();
        if wiping || was {
            out.push(sent);
        }
        if was && !wiping {
            return out;
        }
        was = wiping;
    }
    panic!("no wipe ran and ended in 40 frames: {out:?}");
}

/// Asserts every frame of a wipe sends its rows and nothing else, inside the SPI budget.
pub fn assert_wipe_holds_still(what: &str, frames: &[Sent]) {
    let most = (SCREEN_WIPE_ROWS as usize + 1) * GRID_COLS as usize;
    assert!(frames.len() >= 6, "{what}: {} wipe frames", frames.len());
    for (k, s) in frames.iter().enumerate() {
        assert!(
            s.cells <= most && s.spi_ms() <= SPI_BUDGET_MS,
            "{what}: wipe frame {k} sent {} cells, {:.1} ms ({s:?}); at most {most}",
            s.cells,
            s.spi_ms()
        );
    }
}
