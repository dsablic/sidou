// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
mod knob_curves;
mod lab_edit;
mod lab_song;
mod lab_sound;
mod lab_view;
mod learn_view;
mod link;
mod live_arp;
mod live_cutoff;
mod live_dice;
mod live_freq;
mod live_instrument;
mod live_macro;
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
mod song_gen_roles;
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

use std::ffi::{c_char, c_void, CStr};

use sidou_sys::*;

pub(crate) const SAW_LEAD: u8 = 0;
pub(crate) const PWM_BASS: u8 = 1;
pub(crate) const SYNC_LEAD: u8 = 2;
pub(crate) const NOISE_PERC: u8 = 3;
pub(crate) const PANEL_ENC_K2: u32 = PANEL_ENC_K1 + 1;
pub(crate) const PANEL_ENC_K3: u32 = PANEL_ENC_K1 + 2;
pub(crate) const STATUS_BAR_H: usize = 16;
pub(crate) const CHARACTER: usize = LIVE_CHAR_FIELDS as usize;
pub(crate) const RELEASE_TO_ZERO_MS: [u32; 16] = [
    6, 24, 48, 72, 114, 168, 204, 240, 300, 750, 1500, 2400, 3000, 9000, 15000, 24000,
];

pub(crate) fn c_text(p: *const c_char) -> String {
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

pub(crate) fn c_utf8(p: *const c_char) -> String {
    assert!(!p.is_null());
    unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned()
}

pub(crate) fn shown_preset_name(l: &live) -> String {
    let mut buf = [0 as c_char; LIVE_PRESET_NAME_CHARS as usize];
    unsafe { live_preset_name(l, buf.as_mut_ptr()) };
    c_text(buf.as_ptr())
}

pub(crate) fn map_entry(param: u32) -> &'static midi_map_entry {
    let e = unsafe { midi_map_for_param(param) };
    assert!(!e.is_null());
    unsafe { &*e }
}

pub(crate) fn cc_packet(channel: u8, cc: u8, value: u8) -> u32 {
    u32::from_le_bytes([
        0x0B | (MIDI_OUT_CABLE as u8) << 4,
        0xB0 | channel,
        cc,
        value,
    ])
}

pub(crate) fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
}

pub(crate) fn hex_bytes(text: &str) -> Vec<u8> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .flat_map(str::split_whitespace)
        .map(|v| u8::from_str_radix(v, 16).unwrap())
        .collect()
}

pub fn petscii(sv: &scene_view) -> &sidou_sys::scene {
    unsafe { &sv.u.piece }
}

pub fn petscii_mut(sv: &mut scene_view) -> &mut sidou_sys::scene {
    unsafe { &mut sv.u.piece }
}

pub(crate) fn piece_part(piece: usize) -> (usize, usize) {
    match piece {
        0..=3 => (2, piece),
        4..=12 => (3, piece - 4),
        _ => (1, 0),
    }
}

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

    pub fn preset_step(&mut self, steps: i32) {
        unsafe { live_preset_step(&mut *self.live, steps, 0) };
    }

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

    pub fn param(&self, p: u32) -> u32 {
        unsafe { live_param_get(&*self.live, p) }
    }

    pub fn set_param(&mut self, p: u32, v: u32) {
        unsafe { live_param_set(&mut *self.live, p, v) }
    }

    pub fn sound(&self) -> [u8; LAB_SOUND_BYTES as usize] {
        let mut s = [0u8; LAB_SOUND_BYTES as usize];
        unsafe { live_sound_get(&*self.live, s.as_mut_ptr()) };
        s
    }
}

pub(crate) struct LiveChip {
    pub(crate) chip: Box<sid>,
    pub(crate) target: Box<live_sid_target>,
    _dacs: Box<[sid_dac; 2]>,
    pub(crate) live: Box<live>,
}

impl LiveChip {
    pub(crate) fn new(model: sid_model, rate: u32) -> Self {
        let mut chip = Box::<sid>::default();
        unsafe { sid_init(&mut *chip, model, SID_CLOCK_PAL, rate) };
        let mut dacs = Box::<[sid_dac; 2]>::default();
        let mut target = Box::new(live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: rate,
            dacs: dacs.as_mut_ptr(),
            ..Default::default()
        });
        let mut l = Box::<live>::default();
        let ctx: *mut live_sid_target = &mut *target;
        unsafe { live_init(&mut *l, Some(live_sid_sink), ctx.cast::<c_void>(), model) };
        LiveChip {
            chip,
            target,
            _dacs: dacs,
            live: l,
        }
    }

    pub(crate) fn render(&mut self, n: usize) -> Vec<i16> {
        let mut out = vec![0i16; n];
        unsafe { sid_render(&mut *self.chip, out.as_mut_ptr(), n as u32) };
        out
    }
}

pub fn preset(index: u8) -> &'static live_preset {
    let p = unsafe { live_preset_get(index) };
    assert!(!p.is_null(), "preset {index} missing");
    unsafe { &*p }
}

pub fn preset_name(p: &live_preset) -> String {
    c_utf8(p.name)
}

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

fn run_pixel_buffer() -> Vec<u32> {
    vec![0u32; GRID_RUN_PIXELS as usize / 2]
}

pub fn pixels(g: &sidou_sys::grid) -> Vec<u8> {
    let cols = GRID_COLS as usize;
    let w = cols * 8;
    let pal: [u16; 16] = std::array::from_fn(|c| c as u16);
    let mut out = vec![0u8; w * w];
    let mut buf = run_pixel_buffer();
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

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Sent {
    pub windows: usize,
    pub cells: usize,
}

pub const SPI_HZ: f64 = 12e6;
pub const WINDOW_BYTES: usize = 11;
pub const SPI_BUDGET_MS: f64 = 20.0;
const CELL_BYTES: usize = 8 * 8 * 2;

impl Sent {
    pub fn bytes(self) -> usize {
        self.cells * CELL_BYTES + self.windows * WINDOW_BYTES
    }

    pub fn spi_ms(self) -> f64 {
        self.bytes() as f64 * 8.0 / SPI_HZ * 1e3
    }
}

pub fn drain(g: &mut sidou_sys::grid) -> Sent {
    let mut sent = Sent::default();
    let mut run = grid_run::default();
    let pal = [0u16; 16];
    let mut px = run_pixel_buffer();
    while unsafe { grid_next_run(g, &mut run) } != 0 {
        unsafe { grid_render_run(g, &run, pal.as_ptr(), px.as_mut_ptr().cast()) };
        sent.windows += 1;
        sent.cells += usize::from(run.len);
    }
    sent
}

pub fn grid_row(g: &sidou_sys::grid, row: usize) -> String {
    let cols = GRID_COLS as usize;
    (0..cols).map(|c| printable(g.ch[row * cols + c])).collect()
}

pub(crate) fn printable(code: u8) -> char {
    if (0x20..0x7f).contains(&code) {
        code as char
    } else {
        '#'
    }
}

const WIPE_FRAMES_MAX: usize = 40;

pub fn wipe_frames(frame: &mut dyn FnMut() -> (Sent, bool)) -> Vec<Sent> {
    let mut out = Vec::new();
    let mut wiping_before = false;
    for _ in 0..WIPE_FRAMES_MAX {
        let (sent, wiping) = frame();
        if wiping || wiping_before {
            out.push(sent);
        }
        if wiping_before && !wiping {
            return out;
        }
        wiping_before = wiping;
    }
    panic!("no wipe ran and ended in {WIPE_FRAMES_MAX} frames: {out:?}");
}

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
