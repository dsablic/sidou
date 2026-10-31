// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::config::Config;
use crate::findings::{metric, Finding, Metric, Sev, ShotRef};
use crate::journeys::{view_depth, JourneyResult, Status, StepResult};

#[derive(Clone, Debug)]
pub struct Task {
    pub name: String,
    pub journey: String,
    pub steps: Vec<String>,
    pub actions: u32,
    pub presses: u32,
    pub holds: u32,
    pub turns: u32,
    pub detents: u32,
    pub levels: u32,
    pub frames: f64,
    pub wait_ms: u64,
    pub possible: bool,
    pub ok: bool,
    pub why: String,
    pub two_handed: bool,
    pub shot: Option<String>,
}

pub fn collect(results: &[JourneyResult]) -> Vec<Task> {
    let mut out: Vec<Task> = Vec::new();
    for j in results.iter().filter(|j| !j.tour) {
        let mut i = 0;
        while i < j.steps.len() {
            let Some(name) = j.steps[i].task.clone() else {
                i += 1;
                continue;
            };
            let mut group: Vec<&StepResult> = Vec::new();
            while i < j.steps.len() && j.steps[i].task.as_deref() == Some(name.as_str()) {
                group.push(&j.steps[i]);
                i += 1;
            }
            out.push(task_of(&name, &j.id, &group));
        }
    }
    out
}

fn task_of(name: &str, journey: &str, g: &[&StepResult]) -> Task {
    let mut t = Task {
        name: name.to_string(),
        journey: journey.to_string(),
        steps: g.iter().map(|s| s.id.clone()).collect(),
        actions: 0,
        presses: 0,
        holds: 0,
        turns: 0,
        detents: 0,
        levels: 0,
        frames: 0.0,
        wait_ms: 0,
        possible: true,
        ok: true,
        why: String::new(),
        two_handed: false,
        shot: g.last().and_then(|s| s.shot.clone()),
    };
    let d0 = view_depth(&g[0].view_before, g[0].depth_before);
    let mut deepest = d0;
    for s in g {
        match &s.status {
            Status::NotPossible(w) => {
                t.possible = false;
                t.why = w.clone();
            }
            Status::DeadEnd(w) | Status::NavFailed(w) => {
                t.ok = false;
                t.why = w.clone();
            }
            Status::Ok => {}
        }
        t.actions += s.effort.actions;
        t.presses += s.effort.presses;
        t.holds += s.effort.holds;
        t.turns += s.effort.turns;
        t.detents += s.effort.detents;
        t.frames += s.frames_to_done;
        t.wait_ms += s.wait_ms;
        t.two_handed |= s.two_handed;
        deepest = deepest.max(view_depth(&s.view_after, s.depth_after));
    }
    t.levels = u32::from(deepest.saturating_sub(d0));
    t
}

pub fn judge(tasks: &[Task], cfg: &Config) -> (Vec<Finding>, Vec<Metric>) {
    let mut fs = Vec::new();
    let mut ms = Vec::new();
    for t in tasks {
        let id = format!("{}.{}", crate::slug(&t.name), t.journey);
        if !t.possible {
            fs.push(
                Finding::new(
                    format!("TASK.missing.{id}"),
                    Sev::Low,
                    "tasks",
                    format!("\"{}\": not possible on this firmware", t.name),
                    "The common tasks can be done.",
                )
                .detail(t.why.clone())
                .numbers(json!({"task": t.name, "journey": t.journey})),
            );
            continue;
        }
        ms.push(metric(
            format!("task.{id}.actions"),
            f64::from(t.actions),
            true,
        ));
        ms.push(metric(
            format!("task.{id}.levels"),
            f64::from(t.levels),
            true,
        ));
        ms.push(metric(format!("task.{id}.wait_ms"), t.wait_ms as f64, true));
        let over_a = t.actions > cfg.task_max_actions;
        let over_l = t.levels > cfg.task_max_menu_levels;
        let nums = json!({
            "task": t.name, "journey": t.journey, "steps": t.steps, "actions": t.actions,
            "presses": t.presses, "holds": t.holds, "turns": t.turns, "detents": t.detents,
            "menu_levels": t.levels, "frames_to_complete": t.frames, "forced_wait_ms": t.wait_ms,
            "two_handed": t.two_handed, "max_actions": cfg.task_max_actions,
            "max_menu_levels": cfg.task_max_menu_levels,
        });
        let add_shot = |f: Finding| match &t.shot {
            Some(p) => f.shot(ShotRef {
                path: p.clone(),
                caption: format!("where \"{}\" ends", t.name),
                overlays: vec![],
            }),
            None => f,
        };
        if over_a || over_l {
            let sev = if over_a && over_l {
                Sev::High
            } else {
                Sev::Medium
            };
            fs.push(add_shot(
                Finding::new(
                    format!("TASK.effort.{id}"),
                    sev,
                    "tasks",
                    format!(
                        "\"{}\" takes {} actions and {} menu level(s)",
                        t.name, t.actions, t.levels
                    ),
                    format!(
                        "No common task needs more than {} actions or more than {} menu level(s).",
                        cfg.task_max_actions, cfg.task_max_menu_levels
                    ),
                )
                .detail(format!(
                    "{} presses, {} holds, {} turns ({} detents); {:.0} frames, {} ms of forced waiting.",
                    t.presses, t.holds, t.turns, t.detents, t.frames, t.wait_ms
                ))
                .numbers(nums.clone()),
            ));
        }
        if t.wait_ms >= 1500 {
            fs.push(add_shot(
                Finding::new(
                    format!("TASK.wait.{id}"),
                    Sev::Low,
                    "tasks",
                    format!("\"{}\" makes the player wait {:.1} s", t.name, t.wait_ms as f64 / 1000.0),
                    "A common task does not hold the player up for seconds (a title card is 2.6 s).",
                )
                .numbers(nums.clone()),
            ));
        }
        if !t.ok {
            fs.push(add_shot(
                Finding::new(
                    format!("TASK.fail.{id}"),
                    Sev::High,
                    "tasks",
                    format!("\"{}\" did not complete", t.name),
                    "The scripted path reaches the state it is meant to.",
                )
                .detail(t.why.clone())
                .numbers(nums),
            ));
        }
    }
    (fs, ms)
}

const CORE: [&str; 6] = [
    "pattern_edit",
    "new_song",
    "song_save",
    "undo",
    "order_view",
    "instr_view",
];

pub fn journey_findings(
    j: &JourneyResult,
    metrics: &Value,
    cfg: &Config,
) -> (Vec<Finding>, Vec<Metric>) {
    let mut fs = Vec::new();
    let mut ms = Vec::new();
    let shot = |s: &StepResult, cap: String| {
        s.shot.clone().map(|p| ShotRef {
            path: p,
            caption: cap,
            overlays: vec![],
        })
    };
    if let Some((was, now)) = &j.settings_drift {
        fs.push(
            Finding::new(
                format!("HARN.settings.{}", j.id),
                Sev::High,
                "harness",
                format!("{} changed the settings it only meant to look at", j.title),
                "A tour only looks: MIDI setup, theme, KEYS and CARDS are what they were at its start.",
            )
            .detail(format!("The persisted settings bytes went from {was:?} to {now:?}."))
            .numbers(json!({"journey": j.id})),
        );
    }
    for (index, s) in j.steps.iter().enumerate() {
        let id = format!("{}.{}", j.id, s.id);
        match &s.status {
            Status::NotPossible(why) => {
                let core = why
                    .split_whitespace()
                    .nth(1)
                    .is_some_and(|f| CORE.contains(&f.trim_end_matches(':')));
                let mut f = Finding::new(
                    format!("JOUR.nyp.{id}"),
                    if core { Sev::Medium } else { Sev::Low },
                    "journeys",
                    format!("{}: \"{}\" is not yet possible", j.title, s.label),
                    "A journey can be done end to end on the firmware.",
                )
                .detail(why.clone())
                .numbers(json!({"journey": j.id, "step": s.id}));
                if let Some(sh) = shot(s, "the screen where the journey stops".into()) {
                    f = f.shot(sh);
                }
                fs.push(f);
            }
            Status::DeadEnd(why) => {
                let mut f = Finding::new(
                    format!("JOUR.dead.{id}"),
                    Sev::Medium,
                    "journeys",
                    format!("{}: dead end at \"{}\"", j.title, s.label),
                    "Every step of a journey leads to the state the player expects.",
                )
                .detail(format!("{why}; the screen shows {}.", s.view_after))
                .numbers(json!({"journey": j.id, "step": s.id, "view": s.view_after}));
                if let Some(sh) = shot(s, "where it ended".into()) {
                    f = f.shot(sh);
                }
                fs.push(f);
            }
            Status::NavFailed(why) => {
                let mut f = Finding::new(
                    format!("HARN.nav.{id}"),
                    Sev::High,
                    "harness",
                    format!("{}: navigation failed at \"{}\"", j.title, s.label),
                    "A journey step arrives at the view it means to measure; one that does not measures something else.",
                )
                .detail(format!(
                    "{why}; the screen shows {}. What this step and the ones after it report is not about the screen they name.",
                    s.view_after
                ))
                .numbers(json!({"journey": j.id, "step": s.id, "view": s.view_after}));
                if let Some(sh) = shot(s, "where it ended".into()) {
                    f = f.shot(sh);
                }
                fs.push(f);
            }
            Status::Ok => {}
        }
        if matches!(s.status, Status::Ok) {
            if s.note_blocked == Some(true) {
                fs.push(
                    Finding::new(
                        format!("JOUR.blocked.{id}"),
                        Sev::High,
                        "journeys",
                        format!("{}: after \"{}\" a key no longer sounds", j.title, s.label),
                        "A player can always play: no step leaves the keys silent.",
                    )
                    .numbers(json!({"journey": j.id, "step": s.id})),
                );
            } else if s.two_handed {
                let mut f = Finding::new(
                    format!("JOUR.hands.{id}"),
                    Sev::Medium,
                    "journeys",
                    format!("{}: \"{}\" takes both hands off the keys", j.title, s.label),
                    "A control a performer reaches for while playing needs one hand; a held button plus a key or knob needs two.",
                )
                .numbers(json!({"journey": j.id, "step": s.id, "holds": s.effort.holds}));
                if let Some(sh) = shot(s, "the result".into()) {
                    f = f.shot(sh);
                }
                fs.push(f);
            }
            let remembered = j.steps[..index].iter().any(|s| s.remember);
            let recovered = remembered
                && s.remembered_before
                && !s.remember
                && j.steps[index + 1..]
                    .iter()
                    .take_while(|s| !s.remember)
                    .any(|s| {
                        matches!(s.status, Status::Ok)
                            && s.expects
                                .iter()
                                .any(|(name, ok)| name == "recalled parameters" && *ok)
                    });
            if s.destructive && !s.changed_params.is_empty() && !s.confirm_seen && !recovered {
                let mut f = Finding::new(
                    format!("SAFE.destructive.{id}"),
                    Sev::Medium,
                    "safety",
                    format!("\"{}\" overwrites {} without asking", s.label, s.changed_params.join(", ")),
                    "A destructive action is confirmed or can be undone.",
                )
                .detail("The knob edits made before are replaced by the preset's values, and nothing on the screen asks first.")
                .numbers(json!({"journey": j.id, "step": s.id, "overwritten": s.changed_params}));
                if let Some(sh) = shot(s, "after the change".into()) {
                    f = f.shot(sh);
                }
                fs.push(f);
            }
        }
    }
    let n = |k: &str| metrics.get(k).and_then(Value::as_f64);
    for (k, hi) in [
        ("actions", true),
        ("mode_switches", true),
        ("menu_trips", true),
        ("dead_ends", true),
        ("not_yet_possible", true),
        ("stops_playing_steps", true),
        ("forced_wait_ms", true),
        ("time_to_first_sound_ms", true),
        ("actions_to_first_sound", true),
        ("time_to_first_loop_ms", true),
        ("actions_to_first_loop", true),
    ] {
        if let Some(v) = n(k) {
            ms.push(metric(format!("journey.{}.{k}", j.id), v, hi));
        }
    }
    let _ = cfg;
    (fs, ms)
}

pub fn safety(features: &BTreeMap<String, (bool, String)>) -> Vec<Finding> {
    let mut fs = Vec::new();
    if let Some((false, why)) = features.get("undo") {
        fs.push(
            Finding::new(
                "SAFE.undo",
                Sev::Medium,
                "safety",
                "There is no undo",
                "Work can be recovered from a mistake: an undo, or a confirmation before anything is lost.",
            )
            .detail(format!(
                "{why}. A preset change (K4) replaces the cutoff, resonance and pulse width edits; a song edit cannot be taken back."
            )),
        );
    }
    fs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::EffortCount;

    #[test]
    fn a_later_passing_recall_recovers_a_destructive_step() {
        let mut remembered = step("shape", None, 1, 1, "live");
        remembered.remember = true;
        let mut destructive = step("next", None, 1, 1, "live");
        destructive.destructive = true;
        destructive.remembered_before = true;
        destructive.changed_params = vec!["cutoff".into()];
        let mut recalled = step("back", None, 1, 1, "live");
        recalled.expects = vec![("recalled parameters".into(), true)];
        let mut j = JourneyResult {
            id: "jam".into(),
            title: "Jam".into(),
            tour: false,
            steps: vec![remembered, destructive, recalled],
            captured: vec![],
            settings_drift: None,
        };
        let safe = |j: &JourneyResult| {
            !journey_findings(j, &json!({}), &Config::default())
                .0
                .iter()
                .any(|f| f.id.starts_with("SAFE.destructive"))
        };
        assert!(safe(&j));
        j.steps[2].expects[0].1 = false;
        assert!(!safe(&j));
        j.steps[2].expects[0].1 = true;
        j.steps[0].remember = false;
        assert!(!safe(&j));
        j.steps[0].remember = true;
        j.steps[2].remember = true;
        assert!(!safe(&j));
        j.steps[2].remember = false;
        j.steps[1].remembered_before = false;
        assert!(!safe(&j));
    }

    fn step(id: &str, task: Option<&str>, actions: u32, depth: u8, view: &str) -> StepResult {
        StepResult {
            remember: false,
            remembered_before: false,
            id: id.into(),
            label: id.into(),
            caption: None,
            status: Status::Ok,
            effort: EffortCount {
                actions,
                presses: actions,
                ..EffortCount::default()
            },
            start_ms: 0,
            end_ms: 0,
            wait_ms: 0,
            frames_to_done: 1.0,
            view_before: "live".into(),
            view_after: view.into(),
            depth_before: 1,
            depth_after: depth,
            max_depth: depth,
            two_handed: false,
            note_blocked: None,
            shot: None,
            clip: None,
            rms_db: -120.0,
            task: task.map(String::from),
            milestone: None,
            first_sound_ms: None,
            changed_params: vec![],
            confirm_seen: false,
            destructive: false,
            defaults: None,
            expects: vec![],
            act_ms: vec![],
        }
    }

    #[test]
    fn consecutive_steps_with_one_task_name_are_one_task_and_a_title_card_is_not_a_level() {
        let j = JourneyResult {
            id: "j".into(),
            title: "J".into(),
            tour: false,
            captured: vec![],
            settings_drift: None,
            steps: vec![
                step("a", Some("open CHIP"), 2, 4, "title_card"),
                step("b", Some("open CHIP"), 1, 3, "chip"),
                step("c", None, 5, 3, "chip"),
                step("d", Some("other"), 1, 1, "live"),
            ],
        };
        let t = collect(&[j]);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].actions, t[0].levels), (3, 2));
        assert_eq!((t[1].actions, t[1].levels), (1, 0));
    }

    #[test]
    fn a_task_over_either_limit_is_a_finding_and_a_missing_one_is_noted() {
        let cfg = Config::default();
        let mut a = task_of("long", "j", &[&step("a", Some("long"), 4, 3, "chip")]);
        a.possible = true;
        let mut m = task_of("short", "j", &[&step("a", Some("short"), 1, 2, "tracker")]);
        m.possible = true;
        let mut x = task_of("gone", "j", &[&step("a", Some("gone"), 0, 1, "live")]);
        x.possible = false;
        let (f, _) = judge(&[a, m, x], &cfg);
        let ids: Vec<&str> = f.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"TASK.effort.long.j"), "{ids:?}");
        assert!(!ids.contains(&"TASK.effort.short.j"));
        assert!(ids.contains(&"TASK.missing.gone.j"));
    }
}
