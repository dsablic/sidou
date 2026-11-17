// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use ergo::capture::{AFTER_ACTION_MS, LEAD_IN_MS};
use ergo::config::Config;
use ergo::journeys::{detect_features, parse_table, run_journey};
use serde_json::Value;

const SHORT: &str = "journey short | Short\n\
  step turn | Turn K1 | Turn K1 to open the filter | turn K1 -4 | popup\n\
  step note | Play | Play a note | note A4 200 | sound\n";

const POWER_ON: &str = "journey boot | Boot | poweron\n\
  step boot | Boot | HOME held from power-on | hold HOME 2500; wait 3500 | change\n";

const MENUS: &str = "journey menus | Menus\n\
  step keyboard | Keyboard | SETTINGS, BLUETOOTH, DEVICES | press HOME; wait 500; select-item SETTINGS; press SAVE; wait 500; select-item BLUETOOTH; turn ALGORITHM 2; wait 1500; press SAVE; wait 2000 | view devices\n";

fn captured(table: &str, dir: &std::path::Path) -> Option<Value> {
    let Some(build) = ergo::common::build_dir() else {
        ergo::common::skip_message("the ergo capture tests");
        return None;
    };
    let j = parse_table(table).unwrap().remove(0);
    let (_, cap) = run_journey(
        &build,
        &j,
        &detect_features(&build),
        &Config::default(),
        dir,
        true,
    );
    cap.expect("a full capture").write(dir).unwrap();
    let side = serde_json::from_slice(&std::fs::read(dir.join("frames.json")).unwrap()).unwrap();
    std::fs::remove_dir_all(dir).ok();
    Some(side)
}

fn scratch(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("ergo-{name}-{}", std::process::id()))
}

#[test]
fn a_captured_step_shows_its_caption_before_the_action_and_holds_for_reading_and_the_result() {
    let Some(side) = captured(SHORT, &scratch("pace")) else {
        return;
    };
    for s in side["steps"].as_array().unwrap() {
        let at = |k: &str| s[k].as_u64().unwrap();
        let inputs = s["inputs"].as_array().unwrap();
        let first = inputs[0]["ms"].as_u64().unwrap();
        let acted = inputs
            .iter()
            .filter(|i| i["op"] != "wait")
            .map(|i| i["end_ms"].as_u64().unwrap())
            .max()
            .unwrap();
        assert!(
            first >= at("ms") + LEAD_IN_MS,
            "{}: acts {} ms in",
            s["id"],
            first - at("ms")
        );
        assert!(
            at("end_ms") - at("ms") >= at("read_ms"),
            "{}: shown {} ms",
            s["id"],
            at("end_ms") - at("ms")
        );
        assert!(
            at("end_ms") >= acted + AFTER_ACTION_MS,
            "{}: result held {} ms",
            s["id"],
            at("end_ms") - acted
        );
    }
}

#[test]
fn menus_drawn_over_several_frames_are_captured_only_when_fully_drawn() {
    let Some(side) = captured(MENUS, &scratch("menus")) else {
        return;
    };
    let lcd = &side["lcd"];
    assert!(lcd["torn_ticks_held_back"].as_u64().unwrap() > 0, "{lcd}");
    assert!(lcd["longest_hold_ms"].as_u64().unwrap() < 1000, "{lcd}");
}

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
    let side: Value =
        serde_json::from_slice(&std::fs::read(caps[0].join("cap/frames.json")).unwrap()).unwrap();
    assert_eq!(side["captions"][0]["text"], "Turn K1 to open the filter");
    assert_eq!(side["fps"], 25.0);
    let steps = side["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["id"], "turn");
    assert_eq!(steps[0]["inputs"][0]["op"], "turn");
    assert_eq!(steps[0]["inputs"][0]["encoder"], "K1");
    let (a, b) = (
        steps[0]["inputs"][0]["ms"].as_u64().unwrap(),
        steps[0]["inputs"][0]["end_ms"].as_u64().unwrap(),
    );
    assert!(a < b && b <= steps[1]["ms"].as_u64().unwrap());
    assert!(steps[1]["end_ms"].as_u64().unwrap() > steps[1]["ms"].as_u64().unwrap());
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

#[test]
fn a_power_on_journey_acts_at_power_on_with_no_lead_in() {
    let Some(side) = captured(POWER_ON, &scratch("power-on")) else {
        return;
    };
    let step = &side["steps"][0];
    let waited = step["inputs"][0]["ms"].as_u64().unwrap() - step["ms"].as_u64().unwrap();
    assert!(waited < LEAD_IN_MS, "{step}");
}
