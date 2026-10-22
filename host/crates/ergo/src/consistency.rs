// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Consistency: does each physical control mean the same kind of thing in every view? K1-K4
//! are a parameter row, SELECT moves a focus or a cursor, ALGORITHM steps a choice, HOME goes
//! back. Each control is tried in each view and what it did is measured; the exceptions that
//! docs/reference/controls.md documents are marked as such, anything else is a finding.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::Path;

use serde_json::{json, Value};

use crate::device::{panel, Dev};
use crate::findings::{Finding, Sev};
use crate::journeys::{run_script, view_depth};
use crate::screen::text_runs;

fn parameter_label_visible(param: &str, p: &crate::device::Params, text: &[String]) -> bool {
    use sidou_sys::*;
    let mut buf = [0; 32];
    if let Some((slot, field)) = param.split_once('.') {
        let Some(s) = ["dist", "mod", "fx"].iter().position(|n| *n == slot) else {
            return false;
        };
        let Some(f) = ["algorithm", "amount", "a", "b", "mix"]
            .iter()
            .position(|n| *n == field)
        else {
            return false;
        };
        if unsafe {
            chip_view_label(
                s as u32,
                u32::from(p.slot[s][0]),
                f as u32,
                buf.as_mut_ptr(),
            )
        } == 0
        {
            return false;
        }
    } else {
        let id = match param {
            "cutoff" => LIVE_PARAM_CUTOFF,
            "resonance" => LIVE_PARAM_RESONANCE,
            "pulse_width" => LIVE_PARAM_PULSE_WIDTH,
            "age" => LIVE_PARAM_AGE,
            "dac" => LIVE_PARAM_DAC,
            "thump" => LIVE_PARAM_THUMP,
            "bleed" => LIVE_PARAM_BLEED,
            _ => return false,
        };
        unsafe { learn_view_param_name(id, buf.as_mut_ptr()) };
    }
    let label = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy();
    !label.is_empty()
        && text
            .iter()
            .any(|t| format!(" {} ", t.to_uppercase()).contains(&format!(" {label} ")))
}

/// The views and how to get to each from LIVE (journeys.txt actions).
pub const VIEWS: &[(&str, &str)] = &[
    ("live", ""),
    ("menu", "press HOME; wait 900"),
    (
        "settings",
        "press HOME; select-item SETTINGS; press SAVE; wait 900",
    ),
    ("chip", "press HOME; turn ALGORITHM 1; wait 3000"),
    ("tracker", "press SEQ; press SEQ"),
    (
        "scene",
        "press HOME; select-item SCENE; turn ALGORITHM 1; wait 3000",
    ),
    (
        "status",
        "press HOME; select-item STATUS; turn ALGORITHM 1; wait 900",
    ),
    (
        "midi",
        "press HOME; select-item MIDI; turn ALGORITHM 1; wait 900",
    ),
    (
        "about",
        "press HOME; select-item ABOUT; turn ALGORITHM 1; wait 1500",
    ),
];

pub const CONTROLS: [(&str, u8); 8] = [
    ("K1", panel::K1),
    ("K2", panel::K2),
    ("K3", panel::K3),
    ("K4", panel::K4),
    ("PRESETS", panel::PRESETS),
    ("SELECT", panel::SELECT),
    ("ALGORITHM", panel::ALGORITHM),
    ("HOME", panel::HOME),
];

/// What docs/reference/controls.md says does nothing: (view, control). Everything else is
/// documented to act. Views missing from the doc's lists are marked `?` and not compared.
pub const DOCUMENTED_INERT: &[(&str, &str)] = &[
    ("tracker", "ALGORITHM"),
    ("status", "SELECT"),
    ("status", "ALGORITHM"),
    ("about", "SELECT"),
    ("about", "ALGORITHM"),
];

/// What one control did in one view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effect {
    pub labelled: bool,
    pub screen: bool,
    pub params: Vec<String>,
    pub view_changed: bool,
    pub depth_delta: i32,
    pub popup: bool,
}

impl Effect {
    pub fn acts(&self) -> bool {
        self.screen || !self.params.is_empty() || self.view_changed
    }
}

/// What the panel showed over the 1.2 s after `setup` and a control used (`dir`: None leaves it
/// alone): the distinct frames (a hash each), and what the control did.
fn after(build: &Path, setup: &str, control: u8, dir: Option<i32>) -> (BTreeSet<u64>, Effect) {
    let frame = |d: &Dev| {
        let mut h = DefaultHasher::new();
        d.m.soc.lcd.fb.hash(&mut h);
        h.finish()
    };
    let mut d = Dev::boot(build);
    run_script(&mut d, setup);
    d.settle();
    let view0 = d.view();
    let depth0 = i32::from(view_depth(&view0, d.depth()));
    let fb0 = d.m.soc.lcd.fb.clone();
    let p0 = d.params();
    let mut popup = false;
    match dir {
        Some(n) if control == panel::HOME => {
            let _ = n;
            d.key_down(control);
            d.run(30);
            d.key_up(control);
        }
        Some(n) => d.m.turn(control, n),
        None => {}
    }
    let mut seen = BTreeSet::new();
    let mut text = Vec::new();
    for _ in 0..30 {
        d.run(40);
        popup |= d.popup_rows() > 0;
        seen.insert(frame(&d));
        text.extend(text_runs(&d.shot().cells).into_iter().map(|r| r.text));
    }
    let view1 = d.view();
    let p1 = d.params();
    let params = p0.diff(&p1);
    let e = Effect {
        labelled: params
            .iter()
            .all(|p| parameter_label_visible(p, &p1, &text)),
        screen: d.m.soc.lcd.fb != fb0,
        params,
        view_changed: view1 != view0,
        depth_delta: i32::from(view_depth(&view1, d.depth())) - depth0,
        popup,
    };
    (seen, e)
}

/// What a control does in the view `setup` reaches. A view that changes by itself (About, or a
/// blinking cursor) is compared with the same run left alone: the control did something on
/// screen when it made a frame the idle run never showed. A control at the end of its range is
/// tried the other way as well.
pub fn try_control(build: &Path, setup: &str, control: u8) -> Effect {
    let (idle, _) = after(build, setup, control, None);
    let mut best = Effect::default();
    for dir in [1, -1] {
        let (seen, mut e) = after(build, setup, control, Some(dir));
        e.screen = !seen.is_subset(&idle);
        if e.acts() {
            return e;
        }
        best = e;
        if control == panel::HOME {
            break;
        }
    }
    best
}

/// The kind of thing a control is expected to do.
pub fn kind_ok(control: &str, e: &Effect) -> Result<(), String> {
    match control {
        "K1" | "K2" | "K3" | "K4" if !e.params.is_empty() => Ok(()),
        "K1" | "K2" | "K3" | "K4" => Err("changed no parameter".into()),
        "SELECT" if !e.params.is_empty() => {
            Err(format!("changed a parameter ({})", e.params.join(", ")))
        }
        "SELECT" => Ok(()),
        "ALGORITHM" => Ok(()),
        "HOME" if e.depth_delta < 0 || e.view_changed => Ok(()),
        "HOME" => Err("did not go back or open the menu".into()),
        _ => Ok(()),
    }
}

pub struct ConsistencyResult {
    pub findings: Vec<Finding>,
    pub matrix: Vec<Value>,
}

pub fn analyse(effects: &BTreeMap<(String, String), Effect>) -> ConsistencyResult {
    let mut findings: Vec<Finding> = Vec::new();
    let mut matrix = Vec::new();
    let mut inert: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for ((view, control), e) in effects {
        let documented_inert = DOCUMENTED_INERT.contains(&(view.as_str(), control.as_str()));
        matrix.push(json!({
            "view": view, "control": control, "acts": e.acts(), "screen": e.screen,
            "params": e.params, "view_changed": e.view_changed, "popup": e.popup,
            "documented_inert": documented_inert,
            "labelled": e.labelled,
        }));
        if !e.acts() {
            if documented_inert && !["status", "about", "charset"].contains(&view.as_str()) {
                inert
                    .entry(control_static(control))
                    .or_default()
                    .push(view_static(view));
            } else if !documented_inert {
                findings.push(
                    finding(
                        format!("CONS.drift.{view}.{}", crate::slug(control)),
                        Sev::Medium,
                        format!("{control} does nothing in {view}, and controls.md says it acts"),
                        "A control does what docs/reference/controls.md says in every view.",
                    )
                    .numbers(json!({"view": view, "control": control})),
                );
            }
        } else if documented_inert {
            findings.push(
                finding(
                    format!("CONS.drift.{view}.{}", crate::slug(control)),
                    Sev::Low,
                    format!("{control} acts in {view}, and controls.md says it does nothing"),
                    "A control does what docs/reference/controls.md says in every view.",
                )
                .numbers(json!({"view": view, "control": control, "params": e.params})),
            );
        }
        if e.acts() {
            if let Err(why) = kind_ok(control, e) {
                findings.push(
                    finding(
                        format!("CONS.kind.{view}.{}", crate::slug(control)),
                        Sev::Medium,
                        format!("{control} in {view} {why}"),
                        "K1-K4 are a parameter row; SELECT moves a focus or cursor; ALGORITHM steps a choice; HOME goes back.",
                    )
                    .numbers(json!({"view": view, "control": control, "params": e.params, "depth_delta": e.depth_delta})),
                );
            }
        }
    }
    for (control, views) in &inert {
        findings.push(
            finding(
                format!("CONS.inert.{}", crate::slug(control)),
                Sev::Low,
                format!("{control} does nothing in {} of {} views ({})", views.len(), VIEWS.len(), views.join(", ")),
                "The same physical control means the same kind of thing in every view: it acts, or the screen says it is idle. (controls.md documents these.)",
            )
            .detail("Documented in controls.md; the harness checks that the document and the firmware agree and lists the exception here.")
            .numbers(json!({"views": views})),
        );
    }
    // K1-K4's meaning across views: the parameter families they move.
    let mut fam: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for ((view, control), e) in effects {
        if control == "K1" && !e.params.is_empty() && !e.labelled {
            fam.entry(e.params.join("+"))
                .or_default()
                .push(view.clone());
        }
    }
    if !fam.is_empty() {
        findings.push(
            finding(
                "CONS.k-meaning",
                Sev::Low,
                format!("K1 moves {} parameter families without a visible label", fam.len()),
                "K1-K4 are one parameter row; where the row changes, the screen says what each knob does now.",
            )
            .detail(
                fam.iter()
                    .map(|(p, v)| format!("{p}: {}", v.join(", ")))
                    .collect::<Vec<_>>()
                    .join("; "),
            )
            .numbers(json!(fam)),
        );
    }
    ConsistencyResult { findings, matrix }
}

fn control_static(s: &str) -> &'static str {
    CONTROLS
        .iter()
        .find(|(n, _)| *n == s)
        .map_or("?", |(n, _)| *n)
}

fn view_static(s: &str) -> &'static str {
    VIEWS.iter().find(|(n, _)| *n == s).map_or("?", |(n, _)| *n)
}

pub fn run_all(build: &Path) -> ConsistencyResult {
    let jobs: Vec<(&str, &str, &str, u8)> = VIEWS
        .iter()
        .flat_map(|(v, s)| CONTROLS.iter().map(move |(n, id)| (*v, *s, *n, *id)))
        .collect();
    let done = crate::par_map(&jobs, |(v, setup, n, id)| {
        eprintln!("  consistency {v} {n}");
        try_control(build, setup, *id)
    });
    let effects: BTreeMap<(String, String), Effect> = jobs
        .iter()
        .zip(done)
        .map(|((v, _, n, _), e)| ((v.to_string(), n.to_string()), e))
        .collect();
    analyse(&effects)
}

fn finding(
    id: impl Into<String>,
    sev: Sev,
    title: impl Into<String>,
    rule: impl Into<String>,
) -> Finding {
    Finding::new(id, sev, "consistency", title, rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_age_on_screen_or_in_a_popup_is_consistent() {
        let mut e = fx(true, &["age"], false, 0);
        e.labelled =
            parameter_label_visible("age", &crate::device::Params::default(), &["AGE 32".into()]);
        let mut m = BTreeMap::from([(("chip".into(), "K1".into()), e)]);
        assert!(!analyse(&m)
            .findings
            .iter()
            .any(|f| f.id == "CONS.k-meaning"));
        m.get_mut(&("chip".into(), "K1".into())).unwrap().labelled = false;
        assert!(analyse(&m)
            .findings
            .iter()
            .any(|f| f.id == "CONS.k-meaning"));
        assert!(!parameter_label_visible(
            "age",
            &crate::device::Params::default(),
            &["MESSAGE".into()]
        ));
        let mut p = crate::device::Params::default();
        p.slot[0][0] = 2;
        assert!(parameter_label_visible(
            "dist.amount",
            &p,
            &["DIST DRIVE".into()]
        ));
    }

    #[test]
    fn tracker_select_is_documented_and_read_only_views_do_not_count_as_inert() {
        let mut m = BTreeMap::new();
        m.insert(("tracker".into(), "SELECT".into()), fx(true, &[], false, 0));
        for view in ["status", "about"] {
            m.insert((view.into(), "SELECT".into()), fx(false, &[], false, 0));
            m.insert((view.into(), "ALGORITHM".into()), fx(false, &[], false, 0));
        }
        m.insert(("menu".into(), "ALGORITHM".into()), fx(true, &[], false, 0));
        assert!(analyse(&m).findings.is_empty());
        m.insert(
            ("tracker".into(), "ALGORITHM".into()),
            fx(false, &[], false, 0),
        );
        let f = analyse(&m).findings;
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].id, "CONS.inert.algorithm");
        assert_eq!(f[0].numbers["views"], json!(["tracker"]));
    }

    fn fx(screen: bool, params: &[&str], view_changed: bool, depth: i32) -> Effect {
        Effect {
            labelled: false,
            screen,
            params: params.iter().map(|s| s.to_string()).collect(),
            view_changed,
            depth_delta: depth,
            popup: false,
        }
    }

    #[test]
    fn kinds_are_checked_per_control() {
        assert!(kind_ok("K1", &fx(true, &["cutoff"], false, 0)).is_ok());
        assert!(kind_ok("K1", &fx(true, &[], false, 0)).is_err());
        assert!(kind_ok("SELECT", &fx(true, &[], false, 0)).is_ok());
        assert!(kind_ok("SELECT", &fx(true, &["cutoff"], false, 0)).is_err());
        assert!(kind_ok("HOME", &fx(true, &[], true, 1)).is_ok());
        assert!(kind_ok("HOME", &fx(true, &[], false, 0)).is_err());
    }

    #[test]
    fn an_undocumented_dead_control_is_drift_and_a_documented_one_is_an_exception() {
        let mut m = BTreeMap::new();
        m.insert(
            ("tracker".to_string(), "ALGORITHM".to_string()),
            fx(false, &[], false, 0),
        );
        m.insert(
            ("menu".to_string(), "ALGORITHM".to_string()),
            fx(true, &[], false, 0),
        );
        m.insert(
            ("menu".to_string(), "K1".to_string()),
            fx(false, &[], false, 0),
        );
        m.insert(
            ("live".to_string(), "SELECT".to_string()),
            fx(false, &[], false, 0),
        );
        let r = analyse(&m);
        let ids: Vec<&str> = r.findings.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"CONS.inert.algorithm"), "{ids:?}");
        assert!(ids.contains(&"CONS.drift.menu.k1"), "{ids:?}");
        assert!(!ids.contains(&"CONS.drift.tracker.algorithm"));
        // LIVE's SELECT pages the knob row now: dead there is drift.
        assert!(ids.contains(&"CONS.drift.live.select"), "{ids:?}");
    }
}
