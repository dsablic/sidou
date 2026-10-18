// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The harness is deterministic: one short journey captured twice gives the same frames, audio
//! and sidecar, byte for byte (what lets docs and videos regenerate only when the UI changes).
//! Needs the fm1 build (SIDOU_FM1_BUILD, as emu-gate's tests); skips without it.

use ergo::config::Config;
use ergo::journeys::{detect_features, parse_table, run_journey};

const SHORT: &str = "journey short | Short\n\
  step turn | Turn K1 | Turn K1 to open the filter | turn K1 -4 | popup\n\
  step note | Play | Play a note | note A4 200 | sound\n";

#[test]
fn a_short_journey_captured_twice_is_byte_identical() {
    let Some(build) = ergo::common::build_dir() else {
        ergo::common::skip_message("the ergo determinism test");
        return;
    };
    let j = parse_table(SHORT).unwrap().remove(0);
    let features = detect_features(&build);
    let cfg = Config::default();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("ergo-det-{}", std::process::id()));
    let mut caps = Vec::new();
    for i in 0..2 {
        let out = dir.join(format!("run{i}"));
        let (r, cap) = run_journey(&build, &j, &features, &cfg, &out, true);
        assert_eq!(r.steps.len(), 2);
        let cap = cap.expect("a full capture");
        assert!(cap.frames.len() > 10, "{} frames", cap.frames.len());
        cap.write(&out.join("cap")).unwrap();
        caps.push(out);
    }
    let list = |d: &std::path::Path| {
        let mut v: Vec<_> = walk(d);
        v.sort();
        v
    };
    let (a, b) = (list(&caps[0]), list(&caps[1]));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(
            x.strip_prefix(&caps[0]).unwrap(),
            y.strip_prefix(&caps[1]).unwrap()
        );
        assert!(
            std::fs::read(x).unwrap() == std::fs::read(y).unwrap(),
            "{} differs",
            x.display()
        );
    }
    assert!(a.iter().any(|p| p.ends_with("cap/audio.wav")));
    assert!(a.iter().any(|p| p.ends_with("cap/frames.json")));
    let side: serde_json::Value =
        serde_json::from_slice(&std::fs::read(caps[0].join("cap/frames.json")).unwrap()).unwrap();
    assert_eq!(side["captions"][0]["text"], "Turn K1 to open the filter");
    assert_eq!(side["fps"], 25.0);
    std::fs::remove_dir_all(&dir).ok();
}

fn walk(d: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(d).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}
