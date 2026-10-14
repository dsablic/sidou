// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! reSIDfp set up as SIDOU's reference: PAL clock and the chosen sampling.

use residfp_sys::{Model, Sampling, Sid};
use sidou_sys::{sid_model, SID_CLOCK_PAL, SID_MODEL_6581};

/// The true output rate of reSIDfp's DECIMATE mode. ZeroOrderResampler steps by
/// cyclesPerSample = int(clock / fs * 1024) in 1/1024 cycles, so the rate is
/// clock * 1024 / that (PAL at 44100 Hz: 22877 instead of 22877.41, ~44100.80 Hz).
pub fn residfp_decimate_rate(sample_rate: f64) -> f64 {
    let cycles_per_sample = (f64::from(SID_CLOCK_PAL) / sample_rate * 1024.0) as i32;
    f64::from(SID_CLOCK_PAL) * 1024.0 / f64::from(cycles_per_sample)
}

pub struct ResidfpEngine {
    sid: Sid,
}

impl ResidfpEngine {
    /// decimate: reSIDfp's DECIMATE mode, its ZeroOrderResampler (linear
    /// interpolation). Its rate is off only by integer rounding, ~0.0018% fast (see
    /// residfp_decimate_rate). The default RESAMPLE (two-pass sinc) sounds better but
    /// runs ~0.02% fast (measured: 44110 samples per 44100 expected).
    pub fn new(model: sid_model, sample_rate: f64, decimate: bool) -> Self {
        let mut sid = Sid::new();
        let model = if model == SID_MODEL_6581 {
            Model::Mos6581
        } else {
            Model::Csg8580
        };
        let sampling = if decimate {
            Sampling::Decimate
        } else {
            Sampling::Resample
        };
        sid.set_chip_model(model).expect("reSIDfp chip model");
        sid.set_sampling_parameters(f64::from(SID_CLOCK_PAL), sampling, sample_rate)
            .expect("reSIDfp sampling parameters");
        // reset() keeps each envelope counter at its power-on 0xaa and each accumulator
        // at 0x555555, as on the chip; SIDOU's sid_init powers up in the same state, so
        // both engines start the scripted writes alike.
        sid.reset();
        Self { sid }
    }

    pub fn write(&mut self, reg: u8, value: u8) {
        self.sid.write(reg, value);
    }

    /// Reads a register, e.g. OSC3 (`$1B`) or ENV3 (`$1C`).
    pub fn read(&mut self, reg: u8) -> u8 {
        self.sid.read(reg)
    }

    /// Advances `cycles` without producing samples; see [`Sid::clock_silent`].
    pub fn clock_silent(&mut self, cycles: u32) {
        self.sid.clock_silent(cycles);
    }

    pub fn render_cycles(&mut self, cycles: u32, out: &mut Vec<i16>) {
        let mut buf = vec![0i16; self.sid.max_samples(cycles)];
        let n = self.sid.clock(cycles, &mut buf);
        out.extend_from_slice(&buf[..n]);
    }
}
