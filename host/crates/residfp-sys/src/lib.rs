// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
    fn residfp_set_combined_waveforms(s: *mut RawSid, cws: i32) -> i32;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombinedWaveforms {
    Average = 0,
    Weak = 1,
    Strong = 2,
}

pub const PULLDOWN_ROWS: [&str; 5] = ["ST", "PT", "PS", "PST", "NP"];

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

pub fn wave_table() -> Vec<[u16; 4096]> {
    let mut flat = vec![0i16; 4 * 4096];
    let status = unsafe { residfp_wave_table(flat.as_mut_ptr()) };
    assert_eq!(status, 0, "reSIDfp getWaveTable threw");
    rows(&flat)
}

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
    Decimate,
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

pub struct Sid {
    raw: NonNull<RawSid>,
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

    pub fn set_filter_curve(&mut self, model: Model, curve: f64) {
        let csg8580 = i32::from(model == Model::Csg8580);
        let status = unsafe { residfp_set_filter_curve(self.raw.as_ptr(), csg8580, curve) };
        assert_eq!(status, 0, "reSIDfp setFilterCurve threw");
    }

    pub fn set_filter6581_range(&mut self, range: f64) {
        let status = unsafe { residfp_set_filter6581_range(self.raw.as_ptr(), range) };
        assert_eq!(status, 0, "reSIDfp setFilter6581Range threw");
    }

    pub fn set_combined_waveforms(&mut self, cws: CombinedWaveforms) {
        let status = unsafe { residfp_set_combined_waveforms(self.raw.as_ptr(), cws as i32) };
        assert_eq!(status, 0, "reSIDfp setCombinedWaveforms threw");
    }

    pub fn max_samples(&self, cycles: u32) -> usize {
        let ratio = self
            .samples_per_cycle
            .expect("set the sampling parameters first");
        (f64::from(cycles) * ratio) as usize + 64
    }

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
        assert!(dac[1024] < dac[1023]);
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
