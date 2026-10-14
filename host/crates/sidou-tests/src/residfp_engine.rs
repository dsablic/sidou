// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! reSIDfp set up as SIDOU's reference: PAL clock, chosen sampling, envelopes released.

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
        sid.reset();
        let mut engine = Self { sid };
        engine.release_power_on_envelopes();
        engine
    }

    /// reSIDfp, like the real chip, powers up each envelope counter at 0xaa and keeps
    /// it across reset; our core starts at 0. Release all three to 0 so both engines
    /// start the scripted writes from the same state (M1 decides whether our core
    /// should mimic 0xaa). With AD = SR = 0 the release from 0xaa takes 671 rate
    /// periods of 9 cycles (~7 ms measured); 50 ms is plenty. clockSilent() would only clock
    /// voice 3's envelope, so this clocks into a discarded buffer. Every register
    /// written here ends up 0, the same as after reset.
    fn release_power_on_envelopes(&mut self) {
        for v in 0..3u8 {
            self.sid.write(v * 7 + 4, 0x00);
            self.sid.write(v * 7 + 5, 0x00);
            self.sid.write(v * 7 + 6, 0x00);
        }
        let mut discard = Vec::new();
        self.render_cycles(SID_CLOCK_PAL / 20, &mut discard);
        // ENV3 is the only readable counter; voices 1 and 2 ran the same release.
        assert_eq!(
            self.sid.read(0x1c),
            0,
            "reSIDfp envelope did not release to 0"
        );
    }

    pub fn write(&mut self, reg: u8, value: u8) {
        self.sid.write(reg, value);
    }

    pub fn render_cycles(&mut self, cycles: u32, out: &mut Vec<i16>) {
        let mut buf = vec![0i16; self.sid.max_samples(cycles)];
        let n = self.sid.clock(cycles, &mut buf);
        out.extend_from_slice(&buf[..n]);
    }
}
