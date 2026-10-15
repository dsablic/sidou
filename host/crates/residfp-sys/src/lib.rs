// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal FFI to reSIDfp, through the extern "C" shim in `shim.cpp`. Test-only: reSIDfp
//! (GPL-2.0-or-later) is the reference SIDOU is compared against, never part of firmware.

use std::ptr::NonNull;

#[repr(C)]
struct RawSid {
    _opaque: [u8; 0],
}

extern "C" {
    fn residfp_new() -> *mut RawSid;
    fn residfp_free(s: *mut RawSid);
    fn residfp_set_model(s: *mut RawSid, csg8580: i32) -> i32;
    fn residfp_set_sampling(s: *mut RawSid, clock_hz: f64, decimate: i32, sample_rate: f64) -> i32;
    fn residfp_reset(s: *mut RawSid) -> i32;
    fn residfp_write(s: *mut RawSid, reg: u8, value: u8) -> i32;
    fn residfp_read(s: *mut RawSid, reg: u8) -> i32;
    fn residfp_clock(s: *mut RawSid, cycles: u32, buf: *mut i16) -> i32;
    fn residfp_clock_silent(s: *mut RawSid, cycles: u32) -> i32;
    fn residfp_set_filter_curve(s: *mut RawSid, csg8580: i32, curve: f64) -> i32;
    fn residfp_set_filter6581_range(s: *mut RawSid, range: f64) -> i32;
    fn residfp_filter6581_dac(curve: f64, out: *mut u16) -> i32;
    fn residfp_pulldown_table(csg8580: i32, cws: i32, out: *mut i16) -> i32;
    fn residfp_wave_table(out: *mut i16) -> i32;
    fn residfp_dac_table(
        csg8580: i32,
        bits: u32,
        leak_level: f64,
        saturate: i32,
        out: *mut f64,
    ) -> i32;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Mos6581,
    Csg8580,
}

/// reSIDfp's combined-waveform strength profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombinedWaveforms {
    Average = 0,
    Weak = 1,
    Strong = 2,
}

/// Rows of reSIDfp's pulldown table, in its order.
pub const PULLDOWN_ROWS: [&str; 5] = ["ST", "PT", "PS", "PST", "NP"];

/// reSIDfp's combined-waveform pulldown tables, `PULLDOWN_ROWS` by 4096 inputs (12-bit
/// waveform outputs before pulldown). Each entry is the 12-bit output after it.
pub fn pulldown_table(model: Model, cws: CombinedWaveforms) -> Vec<[u16; 4096]> {
    let mut flat = vec![0i16; 5 * 4096];
    let status = unsafe {
        residfp_pulldown_table(
            i32::from(model == Model::Csg8580),
            cws as i32,
            flat.as_mut_ptr(),
        )
    };
    assert_eq!(status, 0, "reSIDfp buildPulldownTable threw");
    rows(&flat)
}

/// reSIDfp's waveform table: rows none (0xfff), triangle, sawtooth and ST (saw & saw << 1),
/// each indexed by the accumulator's top 12 bits.
pub fn wave_table() -> Vec<[u16; 4096]> {
    let mut flat = vec![0i16; 4 * 4096];
    let status = unsafe { residfp_wave_table(flat.as_mut_ptr()) };
    assert_eq!(status, 0, "reSIDfp getWaveTable threw");
    rows(&flat)
}

/// reSIDfp's DAC output for every input below `2^bits`, as `SID::setChipModel` builds
/// it: `Dac(bits).kinkedDac(model, leak_level)`, then `getOutput(i, saturate)`. reSIDfp
/// uses 12 bits for the waveform DAC (saturating on the 6581) and 8 for the envelope's,
/// with `leak_level` 1 by default.
pub fn dac_table(model: Model, bits: u32, leak_level: f64, saturate: bool) -> Vec<f64> {
    assert!((1..=12).contains(&bits));
    let mut out = vec![0f64; 1 << bits];
    let status = unsafe {
        residfp_dac_table(
            i32::from(model == Model::Csg8580),
            bits,
            leak_level,
            i32::from(saturate),
            out.as_mut_ptr(),
        )
    };
    assert_eq!(status, 0, "reSIDfp kinkedDac threw");
    out
}

/// The 6581 filter's cutoff DAC for a curve setting (0..1, reSIDfp's default 0.5):
/// `FilterModelConfig6581::getDAC(curve)`, the normalised gate voltage Vw of the filter's
/// VCRs for each FC value. The cutoff rises with Vw.
pub fn filter6581_dac(curve: f64) -> Vec<u16> {
    let mut out = vec![0u16; 2048];
    let status = unsafe { residfp_filter6581_dac(curve, out.as_mut_ptr()) };
    assert_eq!(status, 0, "reSIDfp getDAC threw");
    out
}

fn rows(flat: &[i16]) -> Vec<[u16; 4096]> {
    flat.chunks(4096)
        .map(|c| {
            let mut row = [0u16; 4096];
            for (r, &v) in row.iter_mut().zip(c) {
                *r = u16::try_from(v).expect("12-bit table entry");
            }
            row
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sampling {
    /// DECIMATE: the ZeroOrderResampler (linear interpolation).
    Decimate,
    /// RESAMPLE: the two-pass sinc resampler.
    Resample,
}

#[derive(Debug)]
pub struct Error(&'static str);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "reSIDfp rejected {}", self.0)
    }
}

impl std::error::Error for Error {}

/// One reSIDfp chip.
pub struct Sid {
    raw: NonNull<RawSid>,
    /// Output samples per chip cycle, once the sampling parameters are set.
    samples_per_cycle: Option<f64>,
}

impl Sid {
    pub fn new() -> Self {
        Self {
            raw: NonNull::new(unsafe { residfp_new() }).expect("allocate reSIDfp::SID"),
            samples_per_cycle: None,
        }
    }

    pub fn set_chip_model(&mut self, model: Model) -> Result<(), Error> {
        let csg8580 = i32::from(model == Model::Csg8580);
        match unsafe { residfp_set_model(self.raw.as_ptr(), csg8580) } {
            0 => Ok(()),
            _ => Err(Error("the chip model")),
        }
    }

    pub fn set_sampling_parameters(
        &mut self,
        clock_hz: f64,
        sampling: Sampling,
        sample_rate: f64,
    ) -> Result<(), Error> {
        let decimate = i32::from(sampling == Sampling::Decimate);
        match unsafe { residfp_set_sampling(self.raw.as_ptr(), clock_hz, decimate, sample_rate) } {
            0 => {
                self.samples_per_cycle = Some(sample_rate / clock_hz);
                Ok(())
            }
            _ => Err(Error("the sampling parameters")),
        }
    }

    pub fn reset(&mut self) {
        let status = unsafe { residfp_reset(self.raw.as_ptr()) };
        assert_eq!(status, 0, "reSIDfp reset threw");
    }

    pub fn write(&mut self, reg: u8, value: u8) {
        let status = unsafe { residfp_write(self.raw.as_ptr(), reg, value) };
        assert_eq!(status, 0, "reSIDfp write threw");
    }

    pub fn read(&mut self, reg: u8) -> u8 {
        let value = unsafe { residfp_read(self.raw.as_ptr(), reg) };
        u8::try_from(value).expect("reSIDfp read threw")
    }

    /// The filter's curve setting for `model` (0..1; reSIDfp's default 0.5). Each chip
    /// keeps one per model.
    pub fn set_filter_curve(&mut self, model: Model, curve: f64) {
        let csg8580 = i32::from(model == Model::Csg8580);
        let status = unsafe { residfp_set_filter_curve(self.raw.as_ptr(), csg8580, curve) };
        assert_eq!(status, 0, "reSIDfp setFilterCurve threw");
    }

    /// The 6581 filter's range setting (0..1). reSIDfp keeps it once per process, so it
    /// changes every 6581 chip, existing ones included; its default is no call at all
    /// (uCox 20e-6, about 0.487 on this scale).
    pub fn set_filter6581_range(&mut self, range: f64) {
        let status = unsafe { residfp_set_filter6581_range(self.raw.as_ptr(), range) };
        assert_eq!(status, 0, "reSIDfp setFilter6581Range threw");
    }

    /// A buffer size that holds every sample `cycles` chip cycles can produce.
    pub fn max_samples(&self, cycles: u32) -> usize {
        let ratio = self
            .samples_per_cycle
            .expect("set the sampling parameters first");
        (f64::from(cycles) * ratio) as usize + 64
    }

    /// Runs `cycles` chip cycles into `buf` and returns how many samples were produced.
    /// `buf` must hold at least [`Sid::max_samples`]`(cycles)` samples, because reSIDfp
    /// writes without a bound.
    pub fn clock(&mut self, cycles: u32, buf: &mut [i16]) -> usize {
        assert!(
            buf.len() >= self.max_samples(cycles),
            "a {}-sample buffer is too small for {cycles} cycles",
            buf.len()
        );
        let n = unsafe { residfp_clock(self.raw.as_ptr(), cycles, buf.as_mut_ptr()) };
        let n = usize::try_from(n).expect("reSIDfp clock threw");
        assert!(
            n <= buf.len(),
            "reSIDfp overran a {}-sample buffer",
            buf.len()
        );
        n
    }

    /// Runs `cycles` chip cycles without producing output: reSIDfp's `clockSilent`. It
    /// clocks the three waveform generators and voice 3's envelope only, which is all
    /// that OSC3 (`$1B`) and ENV3 (`$1C`) read. Voices 1 and 2's envelopes stand still.
    pub fn clock_silent(&mut self, cycles: u32) {
        let status = unsafe { residfp_clock_silent(self.raw.as_ptr(), cycles) };
        assert_eq!(status, 0, "reSIDfp clockSilent threw");
    }
}

impl Default for Sid {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Sid {
    fn drop(&mut self) {
        unsafe { residfp_free(self.raw.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_6581_cutoff_dac_rises_with_fc_except_at_its_kinks() {
        let dac = filter6581_dac(0.5);
        assert!(dac[2047] > dac[0]);
        // The kinked ladder steps down where a high bit turns on (1023 -> 1024).
        assert!(dac[1024] < dac[1023]);
        // A darker curve lowers every value.
        let dark = filter6581_dac(0.0);
        assert!(dark.iter().zip(&dac).all(|(d, m)| d <= m));
        assert!(dark != dac);
    }

    #[test]
    fn the_filter_curve_changes_the_filtered_output() {
        let render = |curve: f64| {
            let mut s = Sid::new();
            s.set_chip_model(Model::Csg8580).unwrap();
            s.set_sampling_parameters(985_248.0, Sampling::Decimate, 44_100.0)
                .unwrap();
            s.reset();
            s.set_filter_curve(Model::Csg8580, curve);
            for (r, v) in [
                (0x18, 0x1f),
                (0x17, 0x01),
                (0x16, 0x40),
                (0x01, 0x40),
                (0x06, 0xf0),
                (0x04, 0x81),
            ] {
                s.write(r, v);
            }
            let mut buf = vec![0i16; s.max_samples(100_000)];
            let n = s.clock(100_000, &mut buf);
            buf.truncate(n);
            buf
        };
        assert_ne!(render(0.2), render(0.8));
    }
}
