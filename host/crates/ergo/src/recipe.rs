// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::config::Config;
use crate::journeys::{
    parse_table, run_journey, Act, Expect, Journey, Status, Step, BUTTONS, ENCODERS, FEATURES,
};

pub const SCHEMA: &str = "sidou-recipe/1";

pub const DEFAULT_RECIPES: &str = include_str!("../tables/recipes.txt");

fn button_name(id: u8) -> &'static str {
    BUTTONS
        .iter()
        .find(|(_, i)| *i == id)
        .map_or("?", |(n, _)| *n)
}

fn encoder_name(id: u8) -> &'static str {
    ENCODERS
        .iter()
        .find(|(_, i)| *i == id)
        .map_or("?", |(n, _)| *n)
}

pub fn note_name(n: u8) -> String {
    const N: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", N[usize::from(n % 12)], i32::from(n) / 12 - 1)
}

fn by_name(list: &[(&'static str, u8)], name: &str) -> Result<u8, String> {
    list.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, i)| *i)
        .ok_or_else(|| format!("unknown control {name}"))
}

pub fn act_json(a: &Act) -> Value {
    match a {
        Act::Boot => json!({"op": "boot"}),
        Act::Press(b) => json!({"op": "press", "button": button_name(*b)}),
        Act::Hold(b, ms) => json!({"op": "hold", "button": button_name(*b), "ms": ms}),
        Act::Down(k) => {
            json!({"op": "down", "note": note_name(*k - crate::device::panel::KEY0 + 53)})
        }
        Act::Up(k) => json!({"op": "up", "note": note_name(*k - crate::device::panel::KEY0 + 53)}),
        Act::Turn(e, n) => json!({"op": "turn", "encoder": encoder_name(*e), "detents": n}),
        Act::HoldTurn(b, e, n) => {
            json!({"op": "hold_turn", "button": button_name(*b), "encoder": encoder_name(*e), "detents": n})
        }
        Act::Hold2Turn(b, b2, e, n) => {
            json!({"op": "hold2_turn", "button": button_name(*b), "button2": button_name(*b2), "encoder": encoder_name(*e), "detents": n})
        }
        Act::HoldKey(b, n) => {
            json!({"op": "hold_key", "button": button_name(*b), "note": note_name(*n)})
        }
        Act::HoldPress(b, o) => {
            json!({"op": "hold_press", "button": button_name(*b), "press": button_name(*o)})
        }
        Act::Note(n, ms) => {
            json!({"op": "note", "notes": n.iter().map(|x| note_name(*x)).collect::<Vec<_>>(), "ms": ms})
        }
        Act::SelectItem(t) => json!({"op": "select_item", "text": t}),
        Act::Midi(b) => json!({"op": "midi", "bytes": b}),
        Act::Pot(v) => json!({"op": "pot", "value": v}),
        Act::Wait(ms) => json!({"op": "wait", "ms": ms}),
        Act::Clock(bpm, ms, start) => json!({"op": "clock", "bpm": bpm, "ms": ms, "start": start}),
        Act::PowerCycle => json!({"op": "power_cycle"}),
        Act::Play(bpm, events) => {
            json!({"op": "play", "bpm": bpm, "events": events.iter().map(|(n, len)| {
            json!({"notes": n.iter().map(|x| note_name(*x)).collect::<Vec<_>>(), "sixteenths": len})
        }).collect::<Vec<_>>()})
        }
        Act::Taps(n, ms) => {
            json!({"op": "taps", "notes": n.iter().map(|x| note_name(*x)).collect::<Vec<_>>(), "ms": ms})
        }
        Act::Sync => json!({"op": "sync"}),
        Act::Sweep(e, n, ms) => {
            json!({"op": "sweep", "encoder": encoder_name(*e), "detents": n, "ms": ms})
        }
    }
}

fn get_str<'a>(v: &'a Value, k: &str, at: &str) -> Result<&'a str, String> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{at}: `{k}` must be a string"))
}

fn get_i64(v: &Value, k: &str, at: &str) -> Result<i64, String> {
    v.get(k)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("{at}: `{k}` must be an integer"))
}

pub fn act_from_json(v: &Value, at: &str) -> Result<Act, String> {
    let op = get_str(v, "op", at)?;
    let note = |k: &str| crate::journeys::note_number(get_str(v, k, at)?);
    Ok(match op {
        "boot" => Act::Boot,
        "power_cycle" => Act::PowerCycle,
        "press" => Act::Press(by_name(&BUTTONS, get_str(v, "button", at)?)?),
        "hold" => Act::Hold(
            by_name(&BUTTONS, get_str(v, "button", at)?)?,
            get_i64(v, "ms", at)? as u64,
        ),
        "down" => Act::Down(crate::device::key_of_note(note("note")?)),
        "up" => Act::Up(crate::device::key_of_note(note("note")?)),
        "turn" => Act::Turn(
            by_name(&ENCODERS, get_str(v, "encoder", at)?)?,
            get_i64(v, "detents", at)? as i32,
        ),
        "hold_turn" => Act::HoldTurn(
            by_name(&BUTTONS, get_str(v, "button", at)?)?,
            by_name(&ENCODERS, get_str(v, "encoder", at)?)?,
            get_i64(v, "detents", at)? as i32,
        ),
        "hold2_turn" => Act::Hold2Turn(
            by_name(&BUTTONS, get_str(v, "button", at)?)?,
            by_name(&BUTTONS, get_str(v, "button2", at)?)?,
            by_name(&ENCODERS, get_str(v, "encoder", at)?)?,
            get_i64(v, "detents", at)? as i32,
        ),
        "hold_key" => Act::HoldKey(by_name(&BUTTONS, get_str(v, "button", at)?)?, note("note")?),
        "hold_press" => Act::HoldPress(
            by_name(&BUTTONS, get_str(v, "button", at)?)?,
            by_name(&BUTTONS, get_str(v, "press", at)?)?,
        ),
        "note" => Act::Note(
            v.get("notes")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{at}: `notes` must be a list"))?
                .iter()
                .map(|n| crate::journeys::note_number(n.as_str().unwrap_or("")))
                .collect::<Result<_, _>>()?,
            get_i64(v, "ms", at)? as u64,
        ),
        "select_item" => Act::SelectItem(get_str(v, "text", at)?.to_uppercase()),
        "midi" => Act::Midi(
            v.get("bytes")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{at}: `bytes` must be a list"))?
                .iter()
                .map(|b| {
                    b.as_u64()
                        .filter(|b| *b < 256)
                        .map(|b| b as u8)
                        .ok_or_else(|| format!("{at}: bad byte"))
                })
                .collect::<Result<_, _>>()?,
        ),
        "pot" => Act::Pot(get_i64(v, "value", at)? as u16),
        "wait" => Act::Wait(get_i64(v, "ms", at)? as u64),
        "clock" => Act::Clock(
            get_i64(v, "bpm", at)? as u32,
            get_i64(v, "ms", at)? as u64,
            v.get("start").and_then(Value::as_bool).unwrap_or(false),
        ),
        "play" => {
            let bpm = get_i64(v, "bpm", at)?;
            if !(40..=300).contains(&bpm) {
                return Err(format!("{at}: `bpm` must be 40..300"));
            }
            let events = v
                .get("events")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{at}: `events` must be a list"))?
                .iter()
                .map(|e| {
                    let len = get_i64(e, "sixteenths", at)?;
                    if !(1..=64).contains(&len) {
                        return Err(format!("{at}: `sixteenths` must be 1..64"));
                    }
                    let notes = e
                        .get("notes")
                        .and_then(Value::as_array)
                        .ok_or_else(|| format!("{at}: an event's `notes` must be a list"))?
                        .iter()
                        .map(|n| crate::journeys::note_number(n.as_str().unwrap_or("")))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok((notes, len as u32))
                })
                .collect::<Result<_, String>>()?;
            Act::Play(bpm as u32, events)
        }
        "taps" => Act::Taps(
            v.get("notes")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{at}: `notes` must be a list"))?
                .iter()
                .map(|n| crate::journeys::note_number(n.as_str().unwrap_or("")))
                .collect::<Result<_, _>>()?,
            get_i64(v, "ms", at)? as u64,
        ),
        "sync" => Act::Sync,
        "sweep" => {
            let n = get_i64(v, "detents", at)? as i32;
            if n == 0 {
                return Err(format!("{at}: `detents` must not be 0"));
            }
            Act::Sweep(
                by_name(&ENCODERS, get_str(v, "encoder", at)?)?,
                n,
                get_i64(v, "ms", at)? as u64,
            )
        }
        o => return Err(format!("{at}: unknown op {o}")),
    })
}

pub fn expect_json(e: &Expect) -> Value {
    match e {
        Expect::View(v) => json!({"kind": "view", "value": v}),
        Expect::Text(t) => json!({"kind": "text", "value": t}),
        Expect::BigText(t) => json!({"kind": "text_large", "value": t}),
        Expect::Absent(t) => json!({"kind": "text_absent", "value": t}),
        Expect::Value(n, lo, hi) => json!({"kind": "param", "name": n, "min": lo, "max": hi}),
        Expect::Popup => json!({"kind": "popup"}),
        Expect::Toast => json!({"kind": "toast"}),
        Expect::Change => json!({"kind": "screen_changes"}),
        Expect::Recalled => json!({"kind": "recalled"}),
        Expect::Sound => json!({"kind": "audio", "assert": "sounding"}),
        Expect::Silent => json!({"kind": "audio", "assert": "silent"}),
        Expect::Brighter => json!({"kind": "audio", "assert": "brighter"}),
        Expect::Darker => json!({"kind": "audio", "assert": "darker"}),
        Expect::Louder => json!({"kind": "audio", "assert": "louder"}),
        Expect::Quieter => json!({"kind": "audio", "assert": "quieter"}),
        Expect::Level(lo, hi) => json!({"kind": "audio_level", "min_dbfs": lo, "max_dbfs": hi}),
        Expect::Notes(v, e, n) => json!({"kind": "pattern_notes", "voice": v, "entry": e,
            "notes": n.iter().map(|x| crate::journeys::gt_note_name(*x)).collect::<Vec<_>>()}),
        Expect::Rows(v, e, r) => json!({"kind": "pattern_rows", "voice": v, "entry": e, "rows": r}),
    }
}

pub fn expect_from_json(v: &Value, at: &str) -> Result<Expect, String> {
    Ok(match get_str(v, "kind", at)? {
        "view" => Expect::View(get_str(v, "value", at)?.to_string()),
        "text" => Expect::Text(get_str(v, "value", at)?.to_uppercase()),
        "text_large" => Expect::BigText(get_str(v, "value", at)?.to_uppercase()),
        "text_absent" => Expect::Absent(get_str(v, "value", at)?.to_uppercase()),
        "param" => Expect::Value(
            get_str(v, "name", at)?.to_string(),
            get_i64(v, "min", at)?,
            get_i64(v, "max", at)?,
        ),
        "popup" => Expect::Popup,
        "toast" => Expect::Toast,
        "screen_changes" => Expect::Change,
        "recalled" => Expect::Recalled,
        "audio" => match get_str(v, "assert", at)? {
            "sounding" => Expect::Sound,
            "silent" => Expect::Silent,
            "brighter" => Expect::Brighter,
            "darker" => Expect::Darker,
            "louder" => Expect::Louder,
            "quieter" => Expect::Quieter,
            a => return Err(format!("{at}: unknown audio assertion {a}")),
        },
        "audio_level" => Expect::Level(
            v.get("min_dbfs")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("{at}: `min_dbfs` must be a number"))?,
            v.get("max_dbfs")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("{at}: `max_dbfs` must be a number"))?,
        ),
        k @ ("pattern_notes" | "pattern_rows") => {
            let voice = get_i64(v, "voice", at)?;
            if !(1..=3).contains(&voice) {
                return Err(format!("{at}: `voice` must be 1..3"));
            }
            let entry = get_i64(v, "entry", at)?;
            if !(0..=254).contains(&entry) {
                return Err(format!("{at}: `entry` must be 0..254"));
            }
            let list = |key: &str| {
                v.get(key)
                    .and_then(Value::as_array)
                    .ok_or_else(|| format!("{at}: `{key}` must be a list"))
            };
            if k == "pattern_notes" {
                Expect::Notes(
                    voice as u8,
                    entry as u8,
                    list("notes")?
                        .iter()
                        .map(|n| crate::journeys::gt_note(n.as_str().unwrap_or("")))
                        .collect::<Result<_, _>>()
                        .map_err(|e| format!("{at}: {e}"))?,
                )
            } else {
                Expect::Rows(
                    voice as u8,
                    entry as u8,
                    list("rows")?
                        .iter()
                        .map(|r| {
                            r.as_u64()
                                .filter(|r| *r < 256)
                                .map(|r| r as u8)
                                .ok_or_else(|| format!("{at}: a row is 0..255"))
                        })
                        .collect::<Result<_, _>>()?,
                )
            }
        }
        k => return Err(format!("{at}: unknown expectation kind {k}")),
    })
}

pub fn describe(acts: &[Act]) -> String {
    let one = |a: &Act| match a {
        Act::Boot => "wait for the boot to end".to_string(),
        Act::Press(b) => format!("press {}", button_name(*b)),
        Act::Hold(b, ms) => format!("hold {} for {:.1} s", button_name(*b), *ms as f64 / 1000.0),
        Act::Down(k) => format!(
            "hold note key {}",
            note_name(*k - crate::device::panel::KEY0 + 53)
        ),
        Act::Up(k) => format!(
            "let go of note key {}",
            note_name(*k - crate::device::panel::KEY0 + 53)
        ),
        Act::Turn(e, n) => format!(
            "turn {} {} by {} detent{}",
            encoder_name(*e),
            if *n >= 0 {
                "clockwise"
            } else {
                "counter-clockwise"
            },
            n.abs(),
            if n.abs() == 1 { "" } else { "s" }
        ),
        Act::HoldTurn(b, e, n) => format!(
            "hold {} and turn {} {} by {} detents",
            button_name(*b),
            encoder_name(*e),
            if *n >= 0 {
                "clockwise"
            } else {
                "counter-clockwise"
            },
            n.abs()
        ),
        Act::Hold2Turn(b, b2, e, n) => format!(
            "hold {} and {} and turn {} {} by {} detents",
            button_name(*b),
            button_name(*b2),
            encoder_name(*e),
            if *n >= 0 {
                "clockwise"
            } else {
                "counter-clockwise"
            },
            n.abs()
        ),
        Act::HoldKey(b, n) => format!(
            "hold {} and press note key {}",
            button_name(*b),
            note_name(*n)
        ),
        Act::HoldPress(b, o) => format!("hold {} and press {}", button_name(*b), button_name(*o)),
        Act::Note(n, ms) => format!(
            "play {} for {:.1} s",
            n.iter()
                .map(|x| note_name(*x))
                .collect::<Vec<_>>()
                .join(" + "),
            *ms as f64 / 1000.0
        ),
        Act::SelectItem(t) => format!("turn SELECT until {t} is selected"),
        Act::Midi(_) => "send MIDI".to_string(),
        Act::Pot(v) => format!("set MASTER to {:.0}%", f64::from(*v) / 10.23),
        Act::Wait(ms) => format!("wait {:.1} s", *ms as f64 / 1000.0),
        Act::Clock(bpm, _, _) => format!("send MIDI clock at {bpm} BPM"),
        Act::PowerCycle => "switch off and on again".to_string(),
        Act::Play(bpm, ev) => format!(
            "play {} notes at {bpm} BPM",
            ev.iter().filter(|e| !e.0.is_empty()).count()
        ),
        Act::Taps(n, _) => format!(
            "tap {}",
            n.iter()
                .map(|x| note_name(*x))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Act::Sync => "wait for the song's next pattern".to_string(),
        Act::Sweep(e, n, ms) => format!(
            "turn {} slowly {} by {} detents over {:.1} s",
            encoder_name(*e),
            if *n >= 0 {
                "clockwise"
            } else {
                "counter-clockwise"
            },
            n.abs(),
            *ms as f64 / 1000.0
        ),
    };
    acts.iter().map(one).collect::<Vec<_>>().join(", then ")
}

pub fn step_json(s: &Step) -> Value {
    let mut value = json!({
        "id": s.id,
        "title": s.label,
        "caption": s.caption,
        "control": s.control.clone().unwrap_or_else(|| describe(&s.acts)),
        "inputs": s.acts.iter().map(act_json).collect::<Vec<_>>(),
        "expect": s.expects.iter().map(expect_json).collect::<Vec<_>>(),
        "needs": s.needs,
        "harness": {
            "clip_ms": s.clip_ms, "probe": s.probe, "task": s.task, "milestone": s.milestone,
            "destructive": s.destructive, "remember": s.remember, "defaults": s.defaults,
        },
    });
    if s.setup {
        value["harness"]["setup"] = json!(true);
    }
    if !s.focus.is_empty() {
        let mut f = Map::new();
        if let Some(c) = &s.focus.control {
            f.insert("control".into(), json!(c));
        }
        if let Some([col, row, cols, rows]) = s.focus.screen {
            f.insert(
                "screen".into(),
                json!({"col": col, "row": row, "cols": cols, "rows": rows}),
            );
        }
        if let Some(c) = &s.focus.callout {
            f.insert("callout".into(), json!(c));
        }
        value["focus"] = Value::Object(f);
    }
    if let Some(c) = &s.chapter {
        value["chapter"] = json!(c);
    }
    value
}

fn focus_from_json(s: &Value, at: &str) -> Result<crate::journeys::Focus, String> {
    let mut f = crate::journeys::Focus::default();
    let Some(v) = s.get("focus") else {
        return Ok(f);
    };
    let at = format!("{at}.focus");
    if !v.is_object() {
        return Err(format!("{at}: must be an object"));
    }
    if let Some(c) = v.get("control") {
        let c = c
            .as_str()
            .ok_or_else(|| format!("{at}: `control` must be a string"))?;
        crate::journeys::check_focus_control(c).map_err(|e| format!("{at}: {e}"))?;
        f.control = Some(c.to_string());
    }
    if let Some(r) = v.get("screen") {
        let n = |k: &str| -> Result<u8, String> {
            r.get(k)
                .and_then(Value::as_u64)
                .filter(|x| *x < 256)
                .map(|x| x as u8)
                .ok_or_else(|| format!("{at}.screen: `{k}` must be a cell count"))
        };
        let reg = [n("col")?, n("row")?, n("cols")?, n("rows")?];
        crate::journeys::check_screen(reg).map_err(|e| format!("{at}: {e}"))?;
        f.screen = Some(reg);
    }
    if let Some(c) = v.get("callout") {
        f.callout = Some(
            c.as_str()
                .ok_or_else(|| format!("{at}: `callout` must be a string"))?
                .to_string(),
        );
    }
    Ok(f)
}

pub fn recipe_json(j: &Journey) -> Value {
    json!({
        "schema": SCHEMA,
        "id": j.id,
        "title": j.title,
        "goal": j.goal,
        "prerequisites": prerequisites_json(j),
        "skills": j.skills,
        "related": j.related,
        "steps": j.steps.iter().map(step_json).collect::<Vec<_>>(),
    })
}

fn prerequisites_json(j: &Journey) -> Value {
    let mut p = json!({"start": if j.poweron { "power-on" } else { "live" }, "text": j.prereq});
    if !j.songs.is_empty() {
        p["songs"] = json!(j.songs);
    }
    if !j.tunes.is_empty() {
        p["tunes"] = json!(j.tunes);
    }
    if let Some(a) = &j.after {
        p["after"] = json!(a);
    }
    p
}

pub fn parse_recipe(v: &Value) -> Result<Journey, String> {
    let at = "recipe";
    if v.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        return Err(format!("{at}: `schema` must be \"{SCHEMA}\""));
    }
    let strs = |k: &str| -> Result<Vec<String>, String> {
        v.get(k)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{at}: `{k}` must be a list of strings"))?
            .iter()
            .map(|x| {
                x.as_str()
                    .map(String::from)
                    .ok_or_else(|| format!("{at}: `{k}` must hold strings"))
            })
            .collect()
    };
    let pre = v
        .get("prerequisites")
        .ok_or_else(|| format!("{at}: `prerequisites` is missing"))?;
    let start = get_str(pre, "start", "prerequisites")?;
    if !["power-on", "live"].contains(&start) {
        return Err(format!(
            "prerequisites: `start` must be power-on or live, not {start}"
        ));
    }
    let mut steps = Vec::new();
    for (i, s) in v
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{at}: `steps` must be a list"))?
        .iter()
        .enumerate()
    {
        let at = format!("steps[{i}]");
        let h = s.get("harness").cloned().unwrap_or(json!({}));
        let acts = s
            .get("inputs")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{at}: `inputs` must be a list"))?
            .iter()
            .enumerate()
            .map(|(k, a)| act_from_json(a, &format!("{at}.inputs[{k}]")))
            .collect::<Result<Vec<_>, _>>()?;
        let expects = s
            .get("expect")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{at}: `expect` must be a list"))?
            .iter()
            .enumerate()
            .map(|(k, e)| expect_from_json(e, &format!("{at}.expect[{k}]")))
            .collect::<Result<Vec<_>, _>>()?;
        let needs: Vec<String> = s
            .get("needs")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(n) = needs.iter().find(|n| !FEATURES.contains(&n.as_str())) {
            return Err(format!("{at}: unknown feature {n}"));
        }
        let caption = s.get("caption").and_then(Value::as_str).map(String::from);
        if caption.is_none() {
            return Err(format!("{at}: `caption` is required (the manual's text)"));
        }
        steps.push(Step {
            setup: h.get("setup").and_then(Value::as_bool).unwrap_or(false),
            id: get_str(s, "id", &at)?.to_string(),
            label: get_str(s, "title", &at)?.to_string(),
            caption,
            acts,
            expects,
            needs,
            clip_ms: h.get("clip_ms").and_then(Value::as_u64),
            probe: h.get("probe").and_then(Value::as_bool).unwrap_or(false),
            task: h.get("task").and_then(Value::as_str).map(String::from),
            milestone: h.get("milestone").and_then(Value::as_str).map(String::from),
            destructive: h
                .get("destructive")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            remember: h.get("remember").and_then(Value::as_bool).unwrap_or(false),
            defaults: h.get("defaults").and_then(Value::as_bool).unwrap_or(false),
            control: s.get("control").and_then(Value::as_str).map(String::from),
            focus: focus_from_json(s, &at)?,
            chapter: match s.get("chapter") {
                None => None,
                Some(c) => Some(
                    c.as_str()
                        .ok_or_else(|| format!("{at}: `chapter` must be a string"))?
                        .to_string(),
                ),
            },
        });
    }
    Ok(Journey {
        id: get_str(v, "id", at)?.to_string(),
        title: get_str(v, "title", at)?.to_string(),
        tour: false,
        poweron: start == "power-on",
        goal: get_str(v, "goal", at)?.to_string(),
        prereq: get_str(pre, "text", "prerequisites")?.to_string(),
        skills: strs("skills")?,
        related: strs("related")?,
        songs: match pre.get("songs") {
            None => Vec::new(),
            Some(list) => list
                .as_array()
                .ok_or("prerequisites: `songs` must be a list of strings")?
                .iter()
                .map(|x| {
                    let name = x
                        .as_str()
                        .ok_or("prerequisites: `songs` must hold strings")?;
                    crate::device::showcase_song(name)
                        .map_err(|e| format!("prerequisites: {e}"))?;
                    Ok(name.to_string())
                })
                .collect::<Result<_, String>>()?,
        },
        tunes: match pre.get("tunes") {
            None => Vec::new(),
            Some(list) => list
                .as_array()
                .ok_or("prerequisites: `tunes` must be a list of strings")?
                .iter()
                .map(|x| {
                    let name = x
                        .as_str()
                        .ok_or("prerequisites: `tunes` must hold strings")?;
                    crate::device::starter_tune(name).map_err(|e| format!("prerequisites: {e}"))?;
                    Ok(name.to_string())
                })
                .collect::<Result<_, String>>()?,
        },
        after: match pre.get("after") {
            None => None,
            Some(a) => Some(
                a.as_str()
                    .ok_or("prerequisites: `after` must be a recipe id")?
                    .to_string(),
            ),
        },
        prelude: Vec::new(),
        prelude_songs: Vec::new(),
        prelude_tunes: Vec::new(),
        steps,
    })
}

pub fn export(table: &str, dir: &Path) -> Result<Vec<String>, String> {
    let js = parse_table(table)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    for j in js.iter().filter(|j| !j.tour) {
        let name = format!("{}.json", j.id);
        std::fs::write(
            dir.join(&name),
            serde_json::to_string_pretty(&recipe_json(j)).unwrap() + "\n",
        )
        .map_err(|e| e.to_string())?;
        names.push(name);
    }
    Ok(names)
}

#[derive(Debug)]
pub struct RecipeReport {
    pub id: String,
    pub file: PathBuf,
    pub problems: Vec<String>,
}

pub fn load_dir(dir: &Path) -> Result<Vec<(PathBuf, Value)>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let v = serde_json::from_str(
                &std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?,
            )
            .map_err(|e| format!("{}: {e}", p.display()))?;
            Ok((p, v))
        })
        .collect()
}

pub fn validate(
    dir: &Path,
    build: &Path,
    cfg: &Config,
    out: &Path,
) -> Result<Vec<RecipeReport>, String> {
    let recipes = load_dir(dir)?;
    let features = crate::journeys::detect_features(build);
    let mut parsed: Vec<Result<Journey, String>> =
        recipes.iter().map(|(_, v)| parse_recipe(v)).collect();
    let mut ok: Vec<Journey> = parsed
        .iter()
        .filter_map(|p| p.as_ref().ok().cloned())
        .collect();
    crate::journeys::resolve_after(&mut ok)?;
    for j in parsed.iter_mut().flatten() {
        if let Some(r) = ok.iter().find(|x| x.id == j.id) {
            *j = r.clone();
        }
    }
    let items: Vec<_> = recipes.iter().zip(parsed).collect();
    let reports = crate::par_map(&items, |((file, _), parsed)| {
        let mut rep = RecipeReport {
            id: file
                .file_stem()
                .map_or(String::new(), |s| s.to_string_lossy().into()),
            file: file.clone(),
            problems: Vec::new(),
        };
        match parsed {
            Err(e) => rep.problems.push(format!("schema: {e}")),
            Ok(j) => {
                if rep.id != j.id {
                    rep.problems
                        .push(format!("the file name says {} and the id {}", rep.id, j.id));
                }
                let (r, _) = run_journey(build, j, &features, cfg, &out.join("recipes"), false);
                for s in &r.steps {
                    match &s.status {
                        Status::Ok => {}
                        Status::DeadEnd(w) => rep.problems.push(format!("step {}: {w}", s.id)),
                        Status::NavFailed(w) => rep
                            .problems
                            .push(format!("step {}: navigation failed: {w}", s.id)),
                        Status::NotPossible(w) => {
                            rep.problems.push(format!("step {}: cannot run: {w}", s.id))
                        }
                    }
                }
            }
        }
        rep
    });
    Ok(reports)
}

pub fn lint(j: &Journey, ids: &[String]) -> Vec<String> {
    let mut p = Vec::new();
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &j.steps {
        *seen.entry(&s.id).or_default() += 1;
        if s.expects.is_empty() {
            p.push(format!("step {} has no expected outcome", s.id));
        }
        if s.caption.is_none() {
            p.push(format!("step {} has no caption", s.id));
        }
    }
    for (id, n) in seen {
        if n > 1 {
            p.push(format!("step id {id} is used {n} times"));
        }
    }
    for r in &j.related {
        if !ids.contains(r) {
            p.push(format!("related recipe {r} does not exist"));
        }
    }
    if j.goal.is_empty() {
        p.push("no goal".into());
    }
    if j.skills.is_empty() {
        p.push("no skills".into());
    }
    p
}

pub fn schema_keys() -> Map<String, Value> {
    recipe_json(&Journey {
        id: String::new(),
        title: String::new(),
        tour: false,
        poweron: false,
        goal: String::new(),
        prereq: String::new(),
        skills: vec![],
        related: vec![],
        songs: vec![],
        tunes: vec![],
        after: None,
        prelude: vec![],
        prelude_songs: vec![],
        prelude_tunes: vec![],
        steps: vec![],
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_survives_recipe_export_and_import() {
        let j = parse_table(concat!(
            "journey t | T\n goal G\n prereq P\n skills S\n related t\n",
            "step a | Learn | Hold SAVE | holdturn SAVE K1 1 | setup"
        ))
        .unwrap()
        .remove(0);
        let mut value = recipe_json(&j);
        assert!(parse_recipe(&value).unwrap().steps[0].setup);
        value["steps"][0]["harness"]
            .as_object_mut()
            .unwrap()
            .remove("setup");
        assert!(!parse_recipe(&value).unwrap().steps[0].setup);
    }

    const T: &str = "journey r | Recipe\n goal Make a sound\n prereq Power on\n skills first sound; keys\n related other\n\
        step a | Play | Play a note | note A4 500; turn K1 -3; holdkey GLO F#3; midi 90 45 7f | sound; brighter; value cutoff 0 100; text M2; absent XYZ; level -40 -10; do Press a key\n\
        step b | Wait | - | wait 100 | silent; popup; toast; change; view live\n";

    #[test]
    fn a_table_round_trips_through_the_machine_form() {
        let j = parse_table(T).unwrap().remove(0);
        let v = recipe_json(&j);
        assert_eq!(v["schema"], SCHEMA);
        assert_eq!(v["steps"][0]["control"], "Press a key");
        assert_eq!(v["steps"][1]["control"], "wait 0.1 s");
        let back = parse_recipe(&v).unwrap_err();
        assert!(back.contains("caption"), "{back}");
        let mut v2 = v.clone();
        v2["steps"][1]["caption"] = json!("Wait");
        let back = parse_recipe(&v2).unwrap();
        assert_eq!(back.steps[0].acts, j.steps[0].acts);
        assert_eq!(back.steps[0].expects, j.steps[0].expects);
        assert_eq!(back.steps[1].expects, j.steps[1].expects);
        assert_eq!(
            (
                back.goal.as_str(),
                back.skills.len(),
                back.related[0].as_str()
            ),
            ("Make a sound", 2, "other")
        );
        assert!(!back.poweron);
    }

    #[test]
    fn the_schema_refuses_what_it_does_not_know() {
        let j = parse_table(T).unwrap().remove(0);
        let mut v = recipe_json(&j);
        v["steps"][0]["caption"] = json!("x");
        v["steps"][1]["caption"] = json!("x");
        assert!(parse_recipe(&v).is_ok());
        let mut bad = v.clone();
        bad["schema"] = json!("other/9");
        assert!(parse_recipe(&bad).unwrap_err().contains("schema"));
        let mut bad = v.clone();
        bad["steps"][0]["inputs"][0]["op"] = json!("kick");
        assert!(parse_recipe(&bad)
            .unwrap_err()
            .contains("steps[0].inputs[0]"));
        let mut bad = v.clone();
        bad["steps"][0]["expect"][0] = json!({"kind": "audio", "assert": "purple"});
        assert!(parse_recipe(&bad).unwrap_err().contains("purple"));
        let mut bad = v;
        bad["prerequisites"]["start"] = json!("a song");
        assert!(parse_recipe(&bad).is_err());
    }

    #[test]
    fn lint_finds_missing_outcomes_and_dangling_related_recipes() {
        let j = parse_table("journey r | R\n skills a\n goal g\n related nope\n step a | A | c | wait 1\n step a | B | c | wait 1 | popup\n").unwrap().remove(0);
        let p = lint(&j, &["r".into()]);
        assert!(p.iter().any(|x| x.contains("no expected outcome")));
        assert!(p.iter().any(|x| x.contains("used 2 times")));
        assert!(p.iter().any(|x| x.contains("nope")));
    }

    #[test]
    fn the_video_fields_and_new_inputs_round_trip() {
        let j = parse_table(concat!(
            "journey a | A\n goal G\n skills S\n songs neon-highway\n",
            " step a | A | c | play 125 E4:2 .:2 G4+B4; taps F3 G3 200; sweep K2 8 400; sync | sound; ",
            "focus K2; screen 1 2 3 4; callout TURN K2; chapter Intro; notes 1 0 E-4; rows 2 0 0 2\n",
            "journey b | B\n goal G\n skills S\n after a\n step b | B | c | wait 1 | sound; focus card\n",
        ))
        .unwrap();
        for j in &j {
            let v = recipe_json(j);
            let back = parse_recipe(&v).unwrap();
            assert_eq!(back.steps[0].acts, j.steps[0].acts);
            assert_eq!(back.steps[0].expects, j.steps[0].expects);
            assert_eq!(back.steps[0].focus, j.steps[0].focus);
            assert_eq!(back.steps[0].chapter, j.steps[0].chapter);
            assert_eq!(back.songs, j.songs);
            assert_eq!(back.after, j.after);
        }
        let v = recipe_json(&j[0]);
        assert_eq!(v["prerequisites"]["songs"][0], "neon-highway");
        assert_eq!(v["steps"][0]["focus"]["screen"]["cols"], 3);
        assert_eq!(recipe_json(&j[1])["prerequisites"]["after"], "a");
        let plain = parse_table("journey c | C\n step c | C | c | wait 1 | sound\n").unwrap();
        let v = recipe_json(&plain[0]);
        assert!(v["steps"][0].get("focus").is_none());
        assert!(v["prerequisites"].get("songs").is_none());
        let mut bad = recipe_json(&j[0]);
        bad["steps"][0]["focus"]["control"] = json!("K9");
        assert!(parse_recipe(&bad).unwrap_err().contains("focus"));
        let mut bad = recipe_json(&j[0]);
        bad["prerequisites"]["songs"] = json!(["../etc"]);
        assert!(parse_recipe(&bad).is_err());
    }

    #[test]
    fn starter_tunes_and_two_held_buttons_round_trip() {
        let j = parse_table(concat!(
            "journey t | T\n goal G\n skills S\n tunes dojo; chrome-comet\n",
            " step a | A | c | hold2turn EDIT SCL K1 3 | text X\n",
        ))
        .unwrap()
        .remove(0);
        assert_eq!(j.tunes, ["dojo", "chrome-comet"]);
        let v = recipe_json(&j);
        assert_eq!(v["prerequisites"]["tunes"][1], "chrome-comet");
        assert_eq!(v["steps"][0]["inputs"][0]["op"], "hold2_turn");
        let back = parse_recipe(&v).unwrap();
        assert_eq!(back.tunes, j.tunes);
        assert_eq!(back.steps[0].acts, j.steps[0].acts);
        assert!(parse_table("journey t | T\n tunes no-such-tune\n").is_err());
        let mut bad = v;
        bad["prerequisites"]["tunes"] = json!(["../etc"]);
        assert!(parse_recipe(&bad).is_err());
    }

    #[test]
    fn note_names_invert_note_numbers() {
        for n in 53..=79 {
            assert_eq!(crate::journeys::note_number(&note_name(n)), Ok(n));
        }
    }
}
