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
    fn residfp_reset(s: *mut RawSid);
    fn residfp_write(s: *mut RawSid, reg: u8, value: u8);
    fn residfp_read(s: *mut RawSid, reg: u8) -> u8;
    fn residfp_clock(s: *mut RawSid, cycles: u32, buf: *mut i16) -> i32;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Mos6581,
    Csg8580,
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
        unsafe { residfp_reset(self.raw.as_ptr()) }
    }

    pub fn write(&mut self, reg: u8, value: u8) {
        unsafe { residfp_write(self.raw.as_ptr(), reg, value) }
    }

    pub fn read(&mut self, reg: u8) -> u8 {
        unsafe { residfp_read(self.raw.as_ptr(), reg) }
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
        let n = usize::try_from(n).expect("reSIDfp returned a negative count");
        assert!(
            n <= buf.len(),
            "reSIDfp overran a {}-sample buffer",
            buf.len()
        );
        n
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
