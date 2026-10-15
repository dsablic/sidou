// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Unit tests of the portable app logic (`app/`) through FFI.

mod bootguard;
mod expression;
mod font;
mod fx_live;
mod grid;
mod live_freq;
mod live_instrument;
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
