// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Both renderers apply writes at PAL frame (50 Hz) boundaries and return
//! float samples in [-1, 1].

use sidou_sys::{sid, sid_init, sid_model, sid_render, sid_write, SID_CLOCK_PAL};

use crate::residfp_engine::ResidfpEngine;

#[derive(Clone, Copy, Debug)]
pub struct RegWrite {
    pub frame: u32,
    pub reg: u8,
    pub value: u8,
}

pub const REF_SAMPLE_RATE: f64 = 44100.0;

const SAMPLES_PER_FRAME: u32 = 882;
const CYCLES_PER_FRAME: u32 = 19705; // 985248 / 50, rounded

fn apply(writes: &[RegWrite], mut next: usize, frame: u32, mut write: impl FnMut(u8, u8)) -> usize {
    while next < writes.len() && writes[next].frame <= frame {
        write(writes[next].reg, writes[next].value);
        next += 1;
    }
    next
}

fn to_float(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| f32::from(s) / 32768.0).collect()
}

pub fn render_sidou(model: sid_model, writes: &[RegWrite], frames: u32) -> Vec<f32> {
    let mut s = Box::<sid>::default();
    unsafe { sid_init(&mut *s, model, SID_CLOCK_PAL, REF_SAMPLE_RATE as u32) };
    let mut out = vec![0i16; (frames * SAMPLES_PER_FRAME) as usize];
    let mut next = 0;
    for f in 0..frames {
        next = apply(writes, next, f, |r, v| unsafe { sid_write(&mut *s, r, v) });
        let start = (f * SAMPLES_PER_FRAME) as usize;
        let block = &mut out[start..start + SAMPLES_PER_FRAME as usize];
        unsafe { sid_render(&mut *s, block.as_mut_ptr(), SAMPLES_PER_FRAME) };
    }
    to_float(&out)
}

/// decimate: see [`ResidfpEngine::new`]. Use it when timing or pitch is measured; the
/// true output rate is then `residfp_decimate_rate(REF_SAMPLE_RATE)`.
pub fn render_residfp(
    model: sid_model,
    writes: &[RegWrite],
    frames: u32,
    decimate: bool,
) -> Vec<f32> {
    let mut e = ResidfpEngine::new(model, REF_SAMPLE_RATE, decimate);
    let mut out = Vec::new();
    let mut next = 0;
    for f in 0..frames {
        next = apply(writes, next, f, |r, v| e.write(r, v));
        e.render_cycles(CYCLES_PER_FRAME, &mut out);
    }
    to_float(&out)
}
