// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::process::ExitCode;

use sidou_sys::{SID_MODEL_6581, SID_MODEL_8580};
use sidou_tests::filter_sweeps::{sweep, FULL, MODES, SMALL};
use sidou_tests::signal_metrics::write_wav16;

const WAV_SAMPLE_RATE: u32 = 44_100;

fn main() -> ExitCode {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    println!(
        "{:<21} {:<8} envelope diff, max / RMS (dB)",
        "scenario", "model"
    );
    for (level, sustain) in [("", SMALL), ("_full", FULL)] {
        for (mode_name, mode) in MODES {
            let name = format!("{mode_name}_noise_sweep{level}");
            for m in [SID_MODEL_6581, SID_MODEL_8580] {
                let tag = if m == SID_MODEL_6581 { "6581" } else { "8580" };
                let d = sweep(m, mode, sustain);
                for (engine, x) in [("sidou", &d.ours), ("residfp", &d.reference)] {
                    let path = dir.join(format!("{name}_{tag}_{engine}.wav"));
                    if let Err(e) = write_wav16(&path, x, WAV_SAMPLE_RATE) {
                        eprintln!("cannot write {}: {e}", path.display());
                        return ExitCode::FAILURE;
                    }
                }
                println!("{name:<21} {tag:<8} {:.2} / {:.2}", d.max_db, d.rms_db);
            }
        }
    }
    ExitCode::SUCCESS
}
