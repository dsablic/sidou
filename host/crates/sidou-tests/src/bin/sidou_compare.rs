// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Renders filter sweeps on SIDOU and reSIDfp, writes each pair as WAV files into the
//! directory given as the first argument (default: the current directory) and prints the
//! largest envelope difference per scenario and model.

use std::path::PathBuf;
use std::process::ExitCode;

use sidou_sys::{SID_MODEL_6581, SID_MODEL_8580};
use sidou_tests::render::{render_residfp, render_sidou, RegWrite};
use sidou_tests::signal_metrics::{
    max_envelope_diff_db, normalized_to_max, rms_envelope_db, write_wav16,
};

const WINDOW: usize = 882; /* 20 ms; noise has no period to align to */
/* Windows 0-2 (the first 60 ms) are not compared; window 0 holds the click of
 * the 0 -> 15 volume step and is the loudest of all. Each curve is normalised
 * to its loudest *compared* window, so the baseline tracks the filtered noise,
 * not the click; the -40 dB floor stays relative to that peak. */
const FIRST_WINDOW: usize = 3;
const FLOOR_DB: f64 = -40.0;

fn envelope(x: &[f32]) -> Vec<f64> {
    let env = rms_envelope_db(x, WINDOW);
    let len = env.len();
    normalized_to_max(env, FIRST_WINDOW, len)
}

struct Scenario {
    name: &'static str,
    writes: Vec<RegWrite>,
    frames: u32,
}

fn noise_sweep(mode: u8) -> Vec<RegWrite> {
    let w = |frame, reg, value| RegWrite { frame, reg, value };
    let mut writes = vec![
        w(0, 0x18, mode | 0x0f),
        w(0, 0x17, 0x81),
        w(0, 0x00, 0x00),
        w(0, 0x01, 0x40),
        w(0, 0x05, 0x00),
        w(0, 0x06, 0xf0),
        w(0, 0x04, 0x81),
    ];
    writes.extend((0..64u32).map(|f| w(f, 0x16, (f * 4) as u8)));
    writes
}

fn main() -> ExitCode {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let scenarios = [
        Scenario {
            name: "lp_noise_sweep",
            writes: noise_sweep(0x10),
            frames: 64,
        },
        Scenario {
            name: "bp_noise_sweep",
            writes: noise_sweep(0x20),
            frames: 64,
        },
        Scenario {
            name: "hp_noise_sweep",
            writes: noise_sweep(0x40),
            frames: 64,
        },
    ];
    println!("{:<16} {:<8} max envelope diff (dB)", "scenario", "model");
    for s in &scenarios {
        for m in [SID_MODEL_6581, SID_MODEL_8580] {
            let tag = if m == SID_MODEL_6581 { "6581" } else { "8580" };
            let ours = render_sidou(m, &s.writes, s.frames);
            let reference = render_residfp(m, &s.writes, s.frames, false);
            for (engine, x) in [("sidou", &ours), ("residfp", &reference)] {
                let path = dir.join(format!("{}_{tag}_{engine}.wav", s.name));
                if let Err(e) = write_wav16(&path, x, 44100) {
                    eprintln!("cannot write {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            }
            let d = max_envelope_diff_db(
                &envelope(&ours),
                &envelope(&reference),
                FLOOR_DB,
                FIRST_WINDOW,
            );
            println!("{:<16} {:<8} {:.1}", s.name, tag, d);
        }
    }
    ExitCode::SUCCESS
}
