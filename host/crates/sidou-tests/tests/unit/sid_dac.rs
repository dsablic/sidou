// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The voice DACs (sid_dac.c) against reSIDfp's.

use residfp_sys::{dac_table, Model};
use sidou_sys::*;

/* reSIDfp's constants (SID.cpp, FilterModelConfig6581). */
const ZERO_6581: usize = 0x380;
const ZERO_8580: usize = 0x800;
const VOICE_DC_6581: f64 = 0.2143 / 1.5;

fn built(is6581: bool) -> &'static sid_dac {
    unsafe { &*sid_dac_default(i32::from(is6581)) }
}

#[test]
fn the_dacs_are_residfps() {
    for (model, is6581) in [(Model::Mos6581, true), (Model::Csg8580, false)] {
        let d = built(is6581);
        // reSIDfp's envDAC, as floats.
        let env = dac_table(model, 8, 1.0, false);
        for (e, r) in env.iter().enumerate() {
            let ours = f64::from(d.env[e]);
            assert!((ours - r).abs() < 1e-6, "{model:?} env {e}: {ours} vs {r}");
        }
        // reSIDfp's oscDAC (the DAC minus its value at the zero), at twice its scale,
        // with the 6581's voice DC added.
        let wave = dac_table(model, 12, 1.0, is6581);
        let (zero, dc) = if is6581 {
            (wave[ZERO_6581], VOICE_DC_6581)
        } else {
            (wave[ZERO_8580], 0.0)
        };
        for (w, r) in wave.iter().enumerate() {
            let ours = f64::from(d.wave[w]);
            let want = 2.0 * (r - zero + dc);
            assert!(
                (ours - want).abs() < 1e-5,
                "{model:?} wave {w:#x}: {ours} vs {want}"
            );
        }
    }
}

#[test]
fn silence_leaks() {
    // An off bit still conducts: at envelope 0 a voice is quieter, not silent.
    let d = built(true);
    assert!(d.env[0] > 0.007 && d.env[0] < 0.008, "{}", d.env[0]);
    let d = built(false);
    assert!(d.env[0] > 0.003 && d.env[0] < 0.004, "{}", d.env[0]);
}

#[test]
fn the_shared_tables_are_the_defaults_built() {
    let mut d = Box::<sid_dac>::default();
    unsafe {
        sid_dac_build(
            &mut *d,
            1,
            SID_DAC_KINK_6581 as f32,
            SID_DAC_LEAKAGE_6581 as f32,
        )
    };
    assert_eq!(d.wave[..], built(true).wave[..]);
    assert_eq!(d.env[..], built(true).env[..]);
    // Every chip of a model shares one.
    assert!(std::ptr::eq(built(false), built(false)));
}

#[test]
fn the_kink_bends_the_6581_and_a_linear_ladder_stays_linear() {
    let build = |is6581: i32, kink: f32| {
        let mut d = Box::<sid_dac>::default();
        unsafe { sid_dac_build(&mut *d, is6581, kink, 0.0) };
        d
    };
    // The 8580's terminated ladder at 2R/R = 2: equal steps.
    let d = build(0, 2.0);
    let step = d.wave[1] - d.wave[0];
    for w in 1..4096 {
        assert!(
            (d.wave[w] - d.wave[w - 1] - step).abs() < 1e-5,
            "step at {w}"
        );
    }
    // The kink moves the 6581's midscale step.
    let (a, b) = (build(1, 2.2), build(1, 2.3));
    assert!((a.wave[0x800] - a.wave[0x7ff] - (b.wave[0x800] - b.wave[0x7ff])).abs() > 1e-4);
}

#[test]
fn sid_set_dac_swaps_the_tables() {
    let mut s = Box::<sid>::default();
    unsafe { sid_init(&mut *s, SID_MODEL_8580, SID_CLOCK_PAL, 44100) };
    assert!(std::ptr::eq(s.dac, built(false)));
    let mut d = Box::<sid_dac>::default();
    unsafe {
        sid_dac_build(&mut *d, 0, 2.3, 0.01);
        sid_set_dac(&mut *s, &*d);
    }
    assert!(std::ptr::eq(s.dac, &*d));
}

/// CRC-32 (IEEE 802.3, as zlib's crc32) of a table's floats, each as its IEEE-754 bit
/// pattern in little-endian bytes, `wave` then `env`: how the device holds sid_dac in
/// memory, so a device-side CRC over the struct gives the same value.
fn table_crc(d: &sid_dac) -> u32 {
    let mut crc = !0u32;
    for v in d.wave.iter().chain(d.env.iter()) {
        for b in v.to_bits().to_le_bytes() {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
            }
        }
    }
    !crc
}

/* The default tables' CRCs, for comparing with the device's (docs/hardware-test.md):
 * sid_dac_build runs in float on both, but the device's float division need not round
 * as the host's or fm1-emu's soft FPU do. Pinned, so a change shows here first. */
const CRC_8580: u32 = 0xfde1_a3ba;
const CRC_6581: u32 = 0x748f_e138;

#[test]
fn the_default_tables_crc() {
    let crc8580 = table_crc(built(false));
    let crc6581 = table_crc(built(true));
    println!("sid_dac_default(0) (8580): CRC-32 {crc8580:#010x}");
    println!("sid_dac_default(1) (6581): CRC-32 {crc6581:#010x}");
    assert_eq!(crc8580, CRC_8580, "8580 table changed: {crc8580:#010x}");
    assert_eq!(crc6581, CRC_6581, "6581 table changed: {crc6581:#010x}");
}

/// sid_dac_job builds sid_dac_build's table in SID_DAC_BUILD_STEPS steps, for a main loop that
/// must not stall (M1c Task 3's ramp): done only after the last, and the same table bit for
/// bit, whatever was in it before (the steps write every entry). A step after the last does
/// nothing.
#[test]
fn a_dac_job_builds_the_same_table_in_steps() {
    for (is6581, kink, leakage) in [(1, 2.5f32, 0.045f32), (0, 2.1, 0.0035), (1, 2.2, 0.0075)] {
        let mut want = Box::<sid_dac>::default();
        unsafe { sid_dac_build(&mut *want, is6581, kink, leakage) };
        let mut d = Box::<sid_dac>::default();
        d.wave.fill(f32::NAN);
        d.env.fill(f32::NAN);
        let mut job = sid_dac_job::default();
        unsafe { sid_dac_job_start(&mut job, &mut *d, is6581, kink, leakage) };
        let done: Vec<i32> = (0..SID_DAC_BUILD_STEPS + 1)
            .map(|_| unsafe { sid_dac_job_step(&mut job) })
            .collect();
        let mut expect = vec![0; SID_DAC_BUILD_STEPS as usize - 1];
        expect.extend([1, 1]);
        assert_eq!(done, expect);
        assert!(d
            .wave
            .iter()
            .zip(&want.wave)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        assert!(d
            .env
            .iter()
            .zip(&want.env)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
    }
}
