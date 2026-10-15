// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Unit tests of the portable app logic (`app/`) through FFI.

mod about;
mod boot;
mod bootguard;
mod credits;
mod expression;
mod font;
mod fx_live;
mod grid;
mod live_freq;
mod live_instrument;
mod logo;
mod midi_in;
mod midi_map;
mod midi_out;
mod midi_ring;
mod midi_rx;
mod midi_usb;
mod panel;
mod reg_ring;
mod screen;
mod views;
mod widget;

use std::ffi::c_void;

use sidou_sys::*;

/// A register sink that records every write and mirrors the register file.
#[derive(Default)]
pub struct Recorder {
    pub log: Vec<(u8, u8)>,
    pub regs: [u8; 0x19],
}

unsafe extern "C" fn record(ctx: *mut c_void, reg: u8, value: u8) {
    let r = unsafe { &mut *ctx.cast::<Recorder>() };
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

pub fn freq_table() -> [u16; 128] {
    unsafe { live_freq_table }
}

/// The panel as palette indexes, 240 x 240, from the composed grid: each cell's glyph in its
/// colours and its overlay over it, as grid_render_run draws it.
pub fn pixels(g: &sidou_sys::grid) -> Vec<u8> {
    let cols = GRID_COLS as usize;
    let w = cols * 8;
    let mut out = vec![0u8; w * w];
    for i in 0..cols * cols {
        let (cx, cy) = (i % cols, i / cols);
        let glyph = unsafe { font_glyphs }[usize::from(g.ch[i])];
        let (fg, bg) = (g.attr[i] & 15, g.attr[i] >> 4);
        let ovl = usize::from(g.ovl[i]);
        for y in 0..8 {
            for x in 0..8 {
                let bit = 0x80 >> x;
                let c = if ovl != 0 && g.ovl_mask[ovl - 1][y] & bit != 0 {
                    g.ovl_color[ovl - 1]
                } else if glyph[y] & bit != 0 {
                    fg
                } else {
                    bg
                };
                out[(cy * 8 + y) * w + cx * 8 + x] = c;
            }
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
    while unsafe { grid_next_run(g, &mut run) } != 0 {
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
