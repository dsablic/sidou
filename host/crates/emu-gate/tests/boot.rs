// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: boots the SIDOU firmware image (`sidou_app.bin`) in fm1-emu and checks that
//! the audio interrupt plays the SID test tone.
//!
//! The image is booted the way fm1-emu's own CLI boots a bare `app.bin`
//! (`fm1-emu --bin <image padded to 0x02000120> --entry 0x02000120`): the SPL maps app.bin at
//! XIP 0x02000120 and enters its first byte in user mode with r0 pointing at a boot-parameter
//! block. No update package and no stock flash are involved.

mod common;

use common::{build_dir, read, step};
use fm1_core::{Bus, Cpu};
use fm1_soc::{Soc, VEC_BASE, XIP_BASE};
use sidou_tests::signal_metrics::estimate_f0;

/// Where the SPL maps app.bin, and enters it.
const APP_BASE: u32 = 0x0200_0120;
/// The SPL's boot-parameter block (Felucca's crt0.S: inside the vector table at 0x01C7FE00).
const BOOT_PARAMS: u32 = 0x01C7_FE08;
const RAM_SIZE: usize = 1 << 20;
const RUN_MS: u64 = 500;
/// 24 MHz / 544, the I2S frame rate.
const FS: f64 = 24e6 / 544.0;
/// The firmware's test tone (platform/fm1/audio.c): voice 1 sawtooth, A4.
const TONE_HZ: f64 = 440.0;
/// The tone is checked after this much output, once the attack and DC blocker settled.
const SETTLE_MS: f64 = 100.0;

struct Boot {
    /// Interleaved L/R, as the I2S DMA played it.
    audio: Vec<i16>,
    halves: u64,
    steps: u64,
    isr_steps: u64,
    emulated_block_loads: u64,
}

fn boot(image: &[u8], ms: u64) -> Boot {
    let mut xip = vec![0u8; (APP_BASE - XIP_BASE) as usize];
    xip.extend_from_slice(image);
    let mut soc = Soc::new(RAM_SIZE, xip);
    // The firmware's main loop sleeps in `idle`; skip ahead to the next interrupt.
    soc.periph.skip_idle = true;
    let mut cpu = Cpu::new(APP_BASE);
    // The SPL hands cpu0 over in user mode (fm1-emu main.rs).
    cpu.user = true;
    soc.write32(BOOT_PARAMS, BOOT_PARAMS + 0x40);
    cpu.regs[0] = BOOT_PARAMS;

    let end = soc.periph.cycle_of_ms(ms);
    let (mut steps, mut isr_steps, mut emulated_block_loads) = (0u64, 0u64, 0u64);
    // fm1_soc::machine::run_blocks, one instruction at a time so `step` can stand in for
    // the instruction fm1-core lacks.
    while soc.periph.cycles < end {
        if soc.periph.tick() {
            soc.advance(0);
        }
        soc.periph.in_isr = !cpu.irq_levels.is_empty();
        if soc.periph.any_pending() {
            if let Some((irq, prio)) = soc.pending_irq() {
                if cpu.irq_ready(prio) {
                    let handler = soc.read32(VEC_BASE + 4 * u32::from(irq));
                    cpu.interrupt(handler, prio);
                }
            }
        }
        if !cpu.irq_levels.is_empty() {
            isr_steps += 1;
        }
        if step(&mut cpu, &mut soc) {
            emulated_block_loads += 1;
        }
        steps += 1;
    }
    Boot {
        audio: std::mem::take(&mut soc.periph.audio),
        halves: soc.periph.audio_halves,
        steps,
        isr_steps,
        emulated_block_loads,
    }
}

#[test]
fn boots_and_plays_the_test_tone() {
    let Some(build) = build_dir() else {
        common::skip_message("the boot test");
        return;
    };
    let image = read(&build.join("platform/fm1/sidou_app.bin"), "firmware image");
    let r = boot(&image, RUN_MS);

    let frames = r.audio.len() / 2;
    let expected_frames = RUN_MS as f64 / 1000.0 * FS;
    assert!(
        frames as f64 > expected_frames * 0.9,
        "{frames} frames of audio in {RUN_MS} ms, expected about {expected_frames:.0}: \
         the audio interrupt is not running"
    );
    let (left, right): (Vec<i16>, Vec<i16>) = r.audio.chunks_exact(2).map(|f| (f[0], f[1])).unzip();
    assert_eq!(left, right, "the test tone is mono: L and R must match");

    let tail = &left[(SETTLE_MS / 1000.0 * FS) as usize..];
    let peak = tail.iter().map(|&s| i32::from(s).abs()).max().unwrap_or(0);
    let x: Vec<f32> = tail.iter().map(|&s| f32::from(s)).collect();
    let f0 = estimate_f0(&x, FS, 100.0, 2000.0);
    let report = serde_json::json!({
        "run_ms": RUN_MS,
        "frames": frames,
        "audio_halves": r.halves,
        "instructions": r.steps,
        "isr_instructions": r.isr_steps,
        "emulated_block_loads": r.emulated_block_loads,
        "peak": peak,
        "f0_hz": f0,
        "expected_hz": TONE_HZ,
    });
    println!("{report}");
    let out = build.join("sidou_boot.json");
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));

    assert!(peak > 2000, "the output is silent (peak {peak})");
    assert!(
        (f0 / TONE_HZ - 1.0).abs() < 0.005,
        "the tone is at {f0:.2} Hz, expected {TONE_HZ} Hz"
    );
}
