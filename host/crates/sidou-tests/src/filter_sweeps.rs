// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::sid_model;

use crate::render::{render_residfp, render_sidou, RegWrite};
use crate::signal_metrics::{
    max_envelope_diff_db, normalized_to_max, rms_envelope_db, rms_envelope_diff_db,
};

pub const WINDOW: usize = 882;
pub const FIRST_WINDOW: usize = 3;
pub const FLOOR_DB: f64 = -40.0;
pub const FRAMES: u32 = 64;

pub const SMALL: u8 = 0x4;
pub const FULL: u8 = 0xf;

pub const MODES: [(&str, u8); 3] = [("lp", 0x10), ("bp", 0x20), ("hp", 0x40)];

pub fn writes(mode: u8, sustain: u8) -> Vec<RegWrite> {
    let w = |frame, reg, value| RegWrite { frame, reg, value };
    let mut writes = vec![
        w(0, 0x18, mode | 0x0f),
        w(0, 0x17, 0x81),
        w(0, 0x00, 0x00),
        w(0, 0x01, 0x40),
        w(0, 0x05, 0x00),
        w(0, 0x06, sustain << 4),
        w(0, 0x04, 0x81),
    ];
    writes.extend((0..FRAMES).map(|f| w(f, 0x16, (f * 4) as u8)));
    writes
}

fn envelope(x: &[f32]) -> Vec<f64> {
    let env = rms_envelope_db(x, WINDOW);
    let len = env.len();
    normalized_to_max(env, FIRST_WINDOW, len)
}

pub const LOUD: [(&str, bool); 2] = [("noise3", false), ("chord", true)];

pub fn loud_writes(mode: u8, chord: bool) -> Vec<RegWrite> {
    let w = |frame, reg, value| RegWrite { frame, reg, value };
    let mut writes = vec![w(0, 0x18, mode | 0x0f), w(0, 0x17, 0x87)];
    let voices: [(u16, u8, u16); 3] = if chord {
        [(0x1167, 0x21, 0), (0x15ed, 0x41, 0x800), (0x1a13, 0x11, 0)]
    } else {
        [(0x4000, 0x81, 0), (0x3a00, 0x81, 0), (0x3300, 0x81, 0)]
    };
    for (v, (freq, control, pw)) in voices.into_iter().enumerate() {
        let b = (v * 7) as u8;
        writes.extend([
            w(0, b, freq as u8),
            w(0, b + 1, (freq >> 8) as u8),
            w(0, b + 2, pw as u8),
            w(0, b + 3, (pw >> 8) as u8),
            w(0, b + 5, 0x00),
            w(0, b + 6, FULL << 4),
            w(0, b + 4, control),
        ]);
    }
    writes.extend((0..FRAMES).map(|f| w(f, 0x16, (f * 4) as u8)));
    writes
}

pub struct Sweep {
    pub ours: Vec<f32>,
    pub reference: Vec<f32>,
    pub max_db: f64,
    pub rms_db: f64,
}

pub fn sweep(model: sid_model, mode: u8, sustain: u8) -> Sweep {
    compare(model, &writes(mode, sustain))
}

pub fn sweep_loud(model: sid_model, mode: u8, chord: bool) -> Sweep {
    compare(model, &loud_writes(mode, chord))
}

fn compare(model: sid_model, w: &[RegWrite]) -> Sweep {
    let ours = render_sidou(model, w, FRAMES);
    let reference = render_residfp(model, w, FRAMES, false);
    let (a, b) = (envelope(&ours), envelope(&reference));
    Sweep {
        max_db: max_envelope_diff_db(&a, &b, FLOOR_DB, FIRST_WINDOW),
        rms_db: rms_envelope_diff_db(&a, &b, FLOOR_DB, FIRST_WINDOW),
        ours,
        reference,
    }
}
