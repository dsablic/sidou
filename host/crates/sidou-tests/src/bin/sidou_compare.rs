// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Renders the filter sweeps (sidou_tests::filter_sweeps) on SIDOU and reSIDfp, at the
//! small-signal level the reference tests assert and at full scale, writes each pair as
//! WAV files into the directory given as the first argument (default: the current
//! directory) and prints the largest envelope difference per scenario and model.

use std::path::PathBuf;
use std::process::ExitCode;

use sidou_sys::{SID_MODEL_6581, SID_MODEL_8580};
use sidou_tests::filter_sweeps::{sweep, FULL, MODES, SMALL};
use sidou_tests::signal_metrics::write_wav16;

fn main() -> ExitCode {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    println!("{:<21} {:<8} max envelope diff (dB)", "scenario", "model");
    for (level, sustain) in [("", SMALL), ("_full", FULL)] {
        for (mode_name, mode) in MODES {
            let name = format!("{mode_name}_noise_sweep{level}");
            for m in [SID_MODEL_6581, SID_MODEL_8580] {
                let tag = if m == SID_MODEL_6581 { "6581" } else { "8580" };
                let (ours, reference, d) = sweep(m, mode, sustain);
                for (engine, x) in [("sidou", &ours), ("residfp", &reference)] {
                    let path = dir.join(format!("{name}_{tag}_{engine}.wav"));
                    if let Err(e) = write_wav16(&path, x, 44100) {
                        eprintln!("cannot write {}: {e}", path.display());
                        return ExitCode::FAILURE;
                    }
                }
                println!("{name:<21} {tag:<8} {d:.2}");
            }
        }
    }
    ExitCode::SUCCESS
}
