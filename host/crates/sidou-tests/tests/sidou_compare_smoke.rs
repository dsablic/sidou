// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use std::process::Command;

#[test]
fn sidou_compare_smoke() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sidou-compare-smoke");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_sidou-compare"))
        .arg(&dir)
        .output()
        .expect("run sidou-compare");
    assert!(
        out.status.success(),
        "sidou-compare failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let table = String::from_utf8_lossy(&out.stdout);
    println!("{table}");
    assert_eq!(
        table.lines().count(),
        1 + 2 * 3 * 2,
        "one row per level, scenario and model"
    );
    for scenario in ["lp", "bp", "hp"] {
        for level in ["", "_full"] {
            for model in ["6581", "8580"] {
                for engine in ["sidou", "residfp"] {
                    let wav = dir.join(format!(
                        "{scenario}_noise_sweep{level}_{model}_{engine}.wav"
                    ));
                    let len = std::fs::metadata(&wav)
                        .unwrap_or_else(|e| panic!("{}: {e}", wav.display()))
                        .len();
                    assert!(len > 44, "{} holds no samples", wav.display());
                }
            }
        }
    }
}
