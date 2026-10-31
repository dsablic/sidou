// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::{sid, sid_render, sid_write};
use sidou_tablegen::filter_fit::{
    register_writes, spectra, transfer, Mode, Response, CAPTURE, CLOCK_HZ, FS, MODES, SETTLE_CYCLES,
};

pub const TOP_HZ: f64 = 7000.0;

pub fn settled_output(s: &mut sid, fc: u32, res: u8, mode: Option<Mode>) -> Vec<f64> {
    let settle = (f64::from(SETTLE_CYCLES) * FS / CLOCK_HZ) as usize;
    let mut out = vec![0i16; settle + CAPTURE];
    unsafe {
        for (reg, value) in register_writes(fc, res, mode) {
            sid_write(s, reg, value);
        }
        sid_render(s, out.as_mut_ptr(), out.len() as u32);
    }
    out[settle..].iter().map(|&v| f64::from(v)).collect()
}

pub fn responses(output: impl Fn(Option<Mode>) -> Vec<f64>) -> Vec<(Mode, Response)> {
    let direct = spectra(&output(None));
    MODES
        .iter()
        .map(|&m| (m, transfer(&spectra(&output(Some(m))), &direct)))
        .collect()
}

pub fn passband_offset(
    reference: &Response,
    ours: &Response,
    lo_hz: f64,
    hi_hz: f64,
) -> Option<f64> {
    let d: Vec<f64> = reference
        .hz
        .iter()
        .zip(&reference.db)
        .filter(|&(&h, _)| h >= lo_hz && h <= hi_hz)
        .map(|(&h, &db)| {
            let j = ours
                .hz
                .iter()
                .position(|&x| x == h)
                .expect("the same bands");
            ours.db[j] - db
        })
        .collect();
    (!d.is_empty()).then(|| d.iter().sum::<f64>() / d.len() as f64)
}

pub fn passband(mode: Mode, f0: f64) -> (f64, f64) {
    match mode {
        Mode::Lp => (f0 / 8.0, f0 / 3.0),
        Mode::Bp => (f0 / 1.2, f0 * 1.2),
        Mode::Hp => (3.0 * f0, (9.0 * f0).min(TOP_HZ)),
    }
}
