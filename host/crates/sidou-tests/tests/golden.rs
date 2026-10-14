// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Golden renders of the shared workload, recorded as FNV-1a 64 hashes in
//! tests/golden/hashes.txt. SIDOU_GOLDEN_UPDATE=1 rewrites the file.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sidou_sys::{sid_model, workload, SID_MODEL_6581, SID_MODEL_8580};

struct Case {
    name: &'static str,
    chips: u32,
    model: sid_model,
}

const CASES: [Case; 3] = [
    Case {
        name: "workload_6581_1chip",
        chips: 1,
        model: SID_MODEL_6581,
    },
    Case {
        name: "workload_8580_1chip",
        chips: 1,
        model: SID_MODEL_8580,
    },
    Case {
        name: "workload_6581_2chip",
        chips: 2,
        model: SID_MODEL_6581,
    },
];

fn golden_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/golden/hashes.txt")
}

fn render(c: &Case) -> Vec<i16> {
    workload::render(c.chips, c.model)
}

fn fnv1a(samples: &[i16]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &s in samples {
        for byte in (s as u16).to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

fn load(path: &PathBuf) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut words = text.split_whitespace();
    let mut hashes = BTreeMap::new();
    while let (Some(name), Some(hash)) = (words.next(), words.next()) {
        hashes.insert(name.to_owned(), hash.to_owned());
    }
    hashes
}

fn hex(v: u64) -> String {
    format!("{v:016x}")
}

fn updating() -> bool {
    std::env::var("SIDOU_GOLDEN_UPDATE").as_deref() == Ok("1")
}

#[test]
fn renders_match_recorded_hashes() {
    let path = golden_file();
    if updating() {
        let text: String = CASES
            .iter()
            .map(|c| format!("{} {}\n", c.name, hex(fnv1a(&render(c)))))
            .collect();
        if let Err(e) = std::fs::write(&path, text) {
            panic!("could not write {}: {e}", path.display());
        }
        println!("rewrote {}", path.display());
        return;
    }
    let expected = load(&path);
    let mut failures = Vec::new();
    for c in &CASES {
        let Some(want) = expected.get(c.name) else {
            panic!(
                "{} missing; run with SIDOU_GOLDEN_UPDATE=1 and review the diff",
                c.name
            );
        };
        let got = hex(fnv1a(&render(c)));
        println!("{} {got}", c.name);
        if &got != want {
            failures.push(format!(
                "{} changed ({got}, recorded {want}). If intended, rerun with SIDOU_GOLDEN_UPDATE=1",
                c.name
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn workload_never_clips_and_has_no_dc_offset() {
    // The 6581's volume write at frame 0 is a real DC step; the 16 Hz blocker needs
    // ~50 ms to absorb it, so the mean is taken after that.
    const SETTLE: usize = 2205;
    let mut failures = Vec::new();
    for c in &CASES {
        let out = render(c);
        let mut sum = 0.0;
        for (i, &s) in out.iter().enumerate() {
            assert!(s > -32768, "{}", c.name);
            assert!(s < 32767, "{}", c.name);
            if i >= SETTLE {
                sum += f64::from(s);
            }
        }
        let mean = sum / (out.len() - SETTLE) as f64;
        if mean.abs().partial_cmp(&200.0) != Some(std::cmp::Ordering::Less) {
            failures.push(format!("{}: mean {mean}", c.name));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
