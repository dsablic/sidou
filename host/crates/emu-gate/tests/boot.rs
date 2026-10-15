// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: boots the SIDOU firmware image (`sidou_app.bin`) in fm1-emu with the A4 key
//! held, and checks that the audio interrupt plays the live instrument sample for sample as
//! the host build of the same code does.
//!
//! The image is booted as fm1-emu's CLI boots a bare `app.bin` (common::Machine).

mod common;

use common::{build_dir, max_diff, render_host, Firmware, Machine, HALF_FRAMES};
use sidou_tests::signal_metrics::estimate_f0;

const RUN_MS: u64 = 900;
/// The LCD's reset and wake-up delays come before the audio starts.
const BOOT_MS_MAX: u64 = 400;
/// 24 MHz / 544, the I2S frame rate.
const FS: f64 = 24e6 / 544.0;
/// The A4 key (note key 16 = MIDI 69, Felucca id 30) and its pitch.
const A4_KEY: u8 = 30;
const A4: u8 = 69;
const TONE_HZ: f64 = 440.0;
/// The DMA plays the zeroed buffer before the first half the interrupt filled.
const MAX_SILENT_HALVES: usize = 4;
/// The note-on lands in one of the first blocks (the key debounce is ~2 ms).
const MAX_NOTE_BLOCK: usize = 16;
/// The tone is checked after this much output, once the attack and DC blocker settled.
const SETTLE_MS: f64 = 100.0;

#[test]
fn boots_and_plays_a_held_key_sample_exact() {
    let Some(build) = build_dir() else {
        common::skip_message("the boot test");
        return;
    };
    let fw = Firmware::load(&build);
    let mut m = Machine::boot(&fw.image);
    m.key(A4_KEY, true);
    m.run_ms(RUN_MS);

    let audio = &m.soc.periph.audio;
    let frames = audio.len() / 2;
    let expected_frames = (RUN_MS - BOOT_MS_MAX) as f64 / 1000.0 * FS;
    assert!(
        frames as f64 > expected_frames,
        "{frames} frames of audio in {RUN_MS} ms, expected over {expected_frames:.0}: \
         the audio interrupt is not running"
    );
    let (left, right): (Vec<i16>, Vec<i16>) = audio.chunks_exact(2).map(|f| (f[0], f[1])).unzip();
    assert_eq!(
        left, right,
        "every effect slot is off at boot: L and R must match"
    );

    // Sample-exact against the host, both channels: the emulator turns each int32 word back
    // into the int16 the firmware rendered (>> 8). The block the note-on reached the chip in
    // depends on the key scan, so find it.
    let mut best: Option<(usize, usize)> = None;
    let mut min_diff = i32::MAX;
    'search: for silent in 0..=MAX_SILENT_HALVES {
        if audio[..2 * silent * HALF_FRAMES].iter().any(|&s| s != 0) {
            break; // the DMA's zeros only come before the first rendered half
        }
        let played = &audio[2 * silent * HALF_FRAMES..];
        for k in 0..MAX_NOTE_BLOCK {
            let host = render_host(played.len() / 2, A4, k, &[]);
            let d = max_diff(played, &host);
            min_diff = min_diff.min(d);
            if d == 0 {
                best = Some((silent, k));
                break 'search;
            }
        }
    }
    let tail = &left[(SETTLE_MS / 1000.0 * FS) as usize..];
    let peak = tail.iter().map(|&s| i32::from(s).abs()).max().unwrap_or(0);
    let x: Vec<f32> = tail.iter().map(|&s| f32::from(s)).collect();
    let f0 = estimate_f0(&x, FS, 100.0, 2000.0);
    let report = serde_json::json!({
        "run_ms": RUN_MS,
        "frames": frames,
        "audio_halves": m.soc.periph.audio_halves,
        "instructions": m.steps,
        "isr_instructions": m.isr_steps,
        "max_block_instructions": m.alnk_blocks.iter().max(),
        "emulated_block_loads": m.emulated_block_loads,
        "silent_halves": best.map(|b| b.0),
        "note_block": best.map(|b| b.1),
        "min_max_sample_diff_vs_host": min_diff,
        "peak": peak,
        "f0_hz": f0,
        "expected_hz": TONE_HZ,
    });
    println!("{report}");
    let out = build.join("sidou_boot.json");
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));

    assert!(
        best.is_some(),
        "the firmware's audio matches no host render with the note-on in blocks \
         0..{MAX_NOTE_BLOCK}: closest max difference {min_diff} LSB"
    );
    assert!(peak > 2000, "the output is silent (peak {peak})");
    assert!(
        (f0 / TONE_HZ - 1.0).abs() < 0.005,
        "the tone is at {f0:.2} Hz, expected {TONE_HZ} Hz"
    );
}

/// The audio starts before the screen's first frame (platform/fm1/main.c), and the main loop
/// plays a key held at power-on while that frame goes out (platform/fm1/ui.h ui_init): the
/// boot holds no sound up. Also the time to first audio, for docs/art/README.md "Boot".
#[test]
fn the_audio_and_the_keys_start_before_the_display() {
    let Some(build) = build_dir() else {
        common::skip_message("the boot order test");
        return;
    };
    let fw = Firmware::load(&build);
    let mut m = Machine::boot(&fw.image);
    m.key(A4_KEY, true);
    let (mut audio_at, mut note_at, mut display_at) = (None, None, None);
    let mut seen = 0;
    while m.ms() < RUN_MS && display_at.is_none() {
        m.run_ms(1);
        let audio = &m.soc.periph.audio;
        if audio_at.is_none() && !audio.is_empty() {
            audio_at = Some(m.ms());
        }
        if note_at.is_none() && audio[seen..].iter().any(|&s| s != 0) {
            note_at = Some(m.ms());
        }
        seen = audio.len();
        if m.soc.lcd.display_on {
            display_at = Some(m.ms());
        }
    }
    println!(
        "first audio at {audio_at:?} ms, the held key heard at {note_at:?} ms, the display on \
         at {display_at:?} ms from power-on"
    );
    let display_at = display_at.expect("the display came on");
    let audio_at = audio_at.expect("the audio started");
    let note_at = note_at.expect("the key played");
    // The display comes on once the first frame is out. On the device that frame is a full
    // screen of SPI, about 80 ms; fm1-emu sends SPI without its transfer time, so here only the
    // order shows: the sound and the key come before it.
    assert!(
        audio_at < display_at,
        "the audio started at {audio_at} ms, after the display at {display_at} ms"
    );
    assert!(
        note_at < display_at,
        "the key played at {note_at} ms, after the display came on at {display_at} ms"
    );
    // The LCD's reset and wake-up delays (lcd_init) come first.
    assert!(audio_at < BOOT_MS_MAX, "the audio started at {audio_at} ms");
}
