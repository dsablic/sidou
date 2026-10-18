// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The run: every check in turn, the findings and metrics gathered, the files written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::config::Config;
use crate::findings::{regressions, Finding, Metric, Sev};
use crate::journeys::{self, Journey, JourneyResult, Status};
use crate::readability::{self, Captured};
use crate::report::{self, esc, table, Assets};
use crate::{consistency, defaults, feedback, knobs, tasks};

pub struct Opts {
    pub out: PathBuf,
    pub build: PathBuf,
    pub baseline: Option<PathBuf>,
    pub journeys_file: Option<PathBuf>,
    pub readability_file: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
    /// Run only these checks (journeys, readability, knobs, feedback, consistency, defaults).
    pub only: Option<Vec<String>>,
    /// Capture this journey in full (every frame and the audio) and nothing else.
    pub capture: Option<String>,
    pub fail_on_regression: bool,
    pub knob_filter: Option<String>,
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, x| {
        (h ^ u64::from(*x)).wrapping_mul(0x100000001b3)
    })
}

fn wanted(o: &Opts, k: &str) -> bool {
    o.only.as_ref().is_none_or(|v| v.iter().any(|x| x == k))
}

pub fn load_journeys(o: &Opts) -> Result<Vec<Journey>, String> {
    let text = match &o.journeys_file {
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        None => journeys::DEFAULT_TABLE.to_string(),
    };
    journeys::parse_table(&text)
}

fn step_json(s: &journeys::StepResult) -> Value {
    let (status, reason) = match &s.status {
        Status::Ok => ("ok", String::new()),
        Status::DeadEnd(w) => ("dead_end", w.clone()),
        Status::NotPossible(w) => ("not_yet_possible", w.clone()),
    };
    json!({
        "id": s.id, "label": s.label, "caption": s.caption, "status": status, "reason": reason,
        "actions": {"presses": s.effort.presses, "holds": s.effort.holds, "turns": s.effort.turns,
                    "detents": s.effort.detents, "notes": s.effort.notes, "counted": s.effort.actions},
        "two_handed": s.two_handed, "note_blocked": s.note_blocked,
        "forced_wait_ms": s.wait_ms, "frames_to_done": s.frames_to_done,
        "view_before": s.view_before, "view_after": s.view_after,
        "stack_depth_after": s.depth_after, "screenshot": s.shot, "clip": s.clip,
        "rms_dbfs": s.rms_db, "task": s.task, "milestone": s.milestone,
        "changed_params": s.changed_params, "confirm_seen": s.confirm_seen,
        "defaults": s.defaults,
        "expectations": s.expects.iter().map(|(n, ok)| json!({"name": n, "ok": ok})).collect::<Vec<_>>(),
    })
}

pub fn run(o: &Opts) -> Result<i32, String> {
    let cfg = match &o.config_file {
        Some(p) => Config::from_text(
            &std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        )?,
        None => Config::default(),
    };
    let journeys = load_journeys(o)?;
    std::fs::create_dir_all(&o.out).map_err(|e| e.to_string())?;
    let features = journeys::detect_features(&o.build);

    if let Some(id) = &o.capture {
        let j = journeys
            .iter()
            .find(|j| &j.id == id)
            .ok_or_else(|| format!("no journey {id}"))?;
        let (_, cap) = journeys::run_journey(&o.build, j, &features, &cfg, &o.out, true);
        let dir = o.out.join("capture").join(id);
        cap.ok_or("no capture")?
            .write(&dir)
            .map_err(|e| e.to_string())?;
        println!("captured {id} to {}", dir.display());
        return Ok(0);
    }

    let rules = readability::parse_rules(&match &o.readability_file {
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        None => readability::DEFAULT_RULES.to_string(),
    })?;

    let mut findings: Vec<Finding> = Vec::new();
    let mut metrics: Vec<Metric> = Vec::new();
    let mut results: Vec<JourneyResult> = Vec::new();
    let mut captured: Vec<Captured> = Vec::new();

    if wanted(o, "journeys") || wanted(o, "readability") {
        let done = crate::par_map(&journeys, |j| {
            eprintln!("journey {}", j.id);
            journeys::run_journey(&o.build, j, &features, &cfg, &o.out, false).0
        });
        for mut r in done {
            captured.append(&mut r.captured);
            results.push(r);
        }
    }
    let mut journey_json = Vec::new();
    for r in results.iter().filter(|r| !r.tour) {
        let m = journeys::journey_metrics(r, &cfg);
        let (f, mm) = tasks::journey_findings(r, &m, &cfg);
        findings.extend(f);
        metrics.extend(mm);
        journey_json.push(json!({"id": r.id, "title": r.title, "metrics": m,
            "steps": r.steps.iter().map(step_json).collect::<Vec<_>>()}));
    }
    for r in results.iter().filter(|r| r.tour) {
        journey_json.push(json!({"id": r.id, "title": r.title, "tour": true,
            "steps": r.steps.iter().map(step_json).collect::<Vec<_>>()}));
    }
    let task_list = tasks::collect(&results);
    let (tf, tm) = tasks::judge(&task_list, &cfg);
    findings.extend(tf);
    metrics.extend(tm);
    if wanted(o, "journeys") {
        findings.extend(tasks::safety(&features));
    }

    let mut read = readability::ReadResult::default();
    if wanted(o, "readability") {
        read = readability::analyse(&captured, &rules, &cfg);
        findings.append(&mut read.findings);
        metrics.append(&mut read.metrics);
    }
    let mut knob_table = Vec::new();
    if wanted(o, "knobs") {
        eprintln!("knobs");
        let k = knobs::run_all(&o.build, &cfg, &o.out, o.knob_filter.as_deref());
        findings.extend(k.findings);
        metrics.extend(k.metrics);
        knob_table = k.table;
    }
    let mut feed_table = Vec::new();
    if wanted(o, "feedback") {
        eprintln!("feedback");
        let f = feedback::run_all(&o.build, &cfg);
        findings.extend(f.findings);
        metrics.extend(f.metrics);
        feed_table = f.table;
    }
    let mut cons_matrix = Vec::new();
    if wanted(o, "consistency") {
        eprintln!("consistency");
        let c = consistency::run_all(&o.build);
        findings.extend(c.findings);
        cons_matrix = c.matrix;
    }
    let mut def_table = Vec::new();
    if wanted(o, "defaults") {
        eprintln!("defaults");
        let d = defaults::run_all(&o.build, &cfg, &o.out);
        findings.extend(d.findings);
        metrics.extend(d.metrics);
        def_table = d.table;
    }

    findings.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.id.cmp(&b.id)));
    let fw = std::fs::read(o.build.join("platform/fm1/sidou_app.bin")).unwrap_or_default();
    let firmware = json!({"image_bytes": fw.len(), "image_fnv1a": format!("{:016x}", fnv(&fw))});

    let regs = match &o.baseline {
        Some(p) => {
            let b: Value = serde_json::from_str(
                &std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
            )
            .map_err(|e| format!("{}: {e}", p.display()))?;
            Some(regressions(&metrics, &findings, &b, 0.05))
        }
        None => None,
    };

    // Files.
    let counts = |s: Sev| findings.iter().filter(|f| f.severity == s).count();
    let mut by_cat: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &findings {
        *by_cat.entry(f.category).or_default() += 1;
    }
    let fj = json!({
        "schema": 1, "firmware": firmware,
        "summary": {"high": counts(Sev::High), "medium": counts(Sev::Medium), "low": counts(Sev::Low),
                    "info": counts(Sev::Info), "by_category": by_cat},
        "findings": findings.iter().map(Finding::to_json).collect::<Vec<_>>(),
        "regressions": regs,
    });
    let write = |name: &str, v: &Value| {
        std::fs::write(
            o.out.join(name),
            serde_json::to_string_pretty(v).unwrap() + "\n",
        )
        .map_err(|e| e.to_string())
    };
    write("findings.json", &fj)?;
    write(
        "journeys.json",
        &json!({"schema": 1, "firmware": firmware,
            "features": features.iter().map(|(k, (ok, why))| (k.clone(), json!({"available": ok, "reason": if *ok { "" } else { why }}))).collect::<serde_json::Map<_, _>>(),
            "journeys": journey_json,
            "tasks": task_list.iter().map(|t| json!({"name": t.name, "journey": t.journey, "steps": t.steps,
                "possible": t.possible, "ok": t.ok, "actions": t.actions, "menu_levels": t.levels,
                "frames": t.frames, "forced_wait_ms": t.wait_ms, "two_handed": t.two_handed})).collect::<Vec<_>>()}),
    )?;
    write(
        "metrics.json",
        &json!({"schema": 1, "firmware": firmware,
            "metrics": metrics.iter().map(|m| (m.key.clone(), json!({"value": m.value, "worse_when_higher": m.worse_when_higher}))).collect::<serde_json::Map<_, _>>(),
            "findings": findings.iter().map(|f| (f.id.clone(), json!(f.severity.name()))).collect::<serde_json::Map<_, _>>()}),
    )?;
    let html = build_report(
        o,
        &cfg,
        &findings,
        &results,
        &task_list,
        &read.summary,
        &knob_table,
        &feed_table,
        &cons_matrix,
        &def_table,
        &regs,
        &firmware,
        &features,
    );
    std::fs::write(o.out.join("report.html"), html).map_err(|e| e.to_string())?;
    println!(
        "{} findings: {} high, {} medium, {} low, {} info. Report: {}",
        findings.len(),
        counts(Sev::High),
        counts(Sev::Medium),
        counts(Sev::Low),
        counts(Sev::Info),
        o.out.join("report.html").display()
    );
    if let Some(r) = &regs {
        println!("{} regression(s) against the baseline", r.len());
        for x in r {
            println!("  {x}");
        }
        if o.fail_on_regression && !r.is_empty() {
            return Ok(1);
        }
    }
    Ok(0)
}

fn num(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::Number(n)) => n.as_f64().map_or("-".into(), |x| {
            if x.fract() == 0.0 {
                format!("{x:.0}")
            } else {
                format!("{x:.1}")
            }
        }),
        _ => "-".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_report(
    o: &Opts,
    cfg: &Config,
    findings: &[Finding],
    results: &[JourneyResult],
    task_list: &[tasks::Task],
    read_summary: &Value,
    knob_table: &[Value],
    feed_table: &[Value],
    cons_matrix: &[Value],
    def_table: &[Value],
    regs: &Option<Vec<String>>,
    firmware: &Value,
    features: &BTreeMap<String, (bool, String)>,
) -> String {
    let mut a = Assets::new(&o.out);
    let mut body = String::new();
    let count = |s: Sev| findings.iter().filter(|f| f.severity == s).count();
    body += "<header><h1>SIDOU ergonomics report</h1><p class=\"muted\">The firmware driven in fm1-emu, scored against explicit rules. Every finding has a stable id; thresholds and their sources are in the configuration at the end.</p></header>";
    body += "<div class=\"cal\"><b>Calibrate</b><span class=\"muted\">Hold a bank card (85.6 mm) against the bar, drag until they match, then enter the FM-1's lit screen width.</span><div class=\"card-bar\" id=\"cardbar\"></div><div><label>pixels per mm <input type=\"range\" id=\"ppm\" min=\"2.5\" max=\"8\" step=\"0.01\" value=\"3.78\"></label><label>screen width, mm <input type=\"number\" id=\"mmw\" min=\"15\" max=\"40\" step=\"0.1\" value=\"23.4\"></label><span class=\"mono\" id=\"readout\"></span></div></div>";
    body += "<section id=\"summary\"><h2>Summary</h2><div class=\"summary\">";
    for (s, n) in [
        ("high", count(Sev::High)),
        ("medium", count(Sev::Medium)),
        ("low", count(Sev::Low)),
        ("info", count(Sev::Info)),
    ] {
        body += &format!("<div class=\"stat\"><b>{n}</b><span class=\"sev {s}\">{s}</span></div>");
    }
    body += "</div>";
    body += &format!(
        "<p class=\"muted\">Firmware image {} bytes, fnv1a {}.</p>",
        firmware["image_bytes"], firmware["image_fnv1a"]
    );
    if let Some(r) = regs {
        if r.is_empty() {
            body += "<p><b>Regressions against the baseline: none.</b></p>";
        } else {
            body += &format!(
                "<p class=\"bad\">{} regression(s) against the baseline:</p><ul>",
                r.len()
            );
            for x in r {
                body += &format!("<li class=\"mono\">{}</li>", esc(x));
            }
            body += "</ul>";
        }
    }
    let mut cats: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for f in findings {
        cats.entry(f.category).or_default().push(f);
    }
    let rows: Vec<Vec<String>> = cats
        .iter()
        .map(|(c, fs)| {
            let n = |s: Sev| fs.iter().filter(|f| f.severity == s).count().to_string();
            vec![
                esc(c),
                n(Sev::High),
                n(Sev::Medium),
                n(Sev::Low),
                n(Sev::Info),
            ]
        })
        .collect();
    body += &table(&["category", "high", "medium", "low", "info"], &rows);
    body += "<h3>The ten most serious</h3><ol>";
    for f in findings.iter().take(10) {
        body += &format!(
            "<li><span class=\"sev {}\">{}</span> <a href=\"#{}\">{}</a></li>",
            f.severity.name(),
            f.severity.name(),
            esc(&f.id),
            esc(&f.title)
        );
    }
    body += "</ol></section>";

    for (cat, fs) in &cats {
        body += &format!("<section id=\"cat-{cat}\"><h2>Findings: {cat}</h2>");
        for f in fs {
            body += &report::finding_card(&mut a, f);
        }
        body += "</section>";
    }

    // Journeys.
    body += "<section id=\"journeys\"><h2>Journeys</h2><p class=\"muted\">Each journey is a scripted input sequence; a screenshot per step, a clip at the checkpoints. Dashed cards are steps not yet possible on this firmware.</p>";
    let feat: Vec<Vec<String>> = features
        .iter()
        .map(|(k, (ok, why))| {
            vec![
                esc(k),
                if *ok {
                    "yes".into()
                } else {
                    format!("<span class=\"bad\">no</span> {}", esc(why))
                },
            ]
        })
        .collect();
    body += &table(&["feature", "on this firmware"], &feat);
    for r in results {
        body += &format!(
            "<h3>{} <span class=\"muted mono\">{}{}</span></h3>",
            esc(&r.title),
            esc(&r.id),
            if r.tour {
                " (screens for readability)"
            } else {
                ""
            }
        );
        if !r.tour {
            let m = journeys::journey_metrics(r, cfg);
            body += &format!(
                "<p class=\"mono\">actions {} | first sound {} ms / {} action(s) | first loop {} ms / {} action(s) | mode switches {} | menu trips {} | dead ends {} | not yet possible {} | steps that stop playing {} | forced waiting {} ms</p>",
                num(&m, "actions"),
                num(&m, "time_to_first_sound_ms"),
                num(&m, "actions_to_first_sound"),
                num(&m, "time_to_first_loop_ms"),
                num(&m, "actions_to_first_loop"),
                num(&m, "mode_switches"),
                num(&m, "menu_trips"),
                num(&m, "dead_ends"),
                num(&m, "not_yet_possible"),
                num(&m, "stops_playing_steps"),
                num(&m, "forced_wait_ms")
            );
        }
        body += "<div class=\"strip\">";
        for s in &r.steps {
            let (cls, st) = match &s.status {
                Status::Ok => ("", "ok".to_string()),
                Status::DeadEnd(w) => ("dead", format!("dead end: {w}")),
                Status::NotPossible(w) => ("nyp", format!("not yet possible: {w}")),
            };
            let shot = match &s.shot {
                Some(p) if a.imgs.contains_key(p) || std::fs::metadata(o.out.join(p)).is_ok() => {
                    a.shot(p, &format!("{} -> {}", s.view_before, s.view_after), &[])
                }
                _ => String::new(),
            };
            let shot = shot.replace("class=\"bezel real\"", "class=\"bezel one\"");
            body += &format!(
                "<div class=\"step {cls}\"><b>{}</b>{}<div class=\"st\">{}</div>{}{}</div>",
                esc(&s.label),
                s.caption.as_ref().map_or(String::new(), |c| format!(
                    "<i class=\"muted\">{}</i>",
                    esc(c)
                )),
                esc(&st),
                shot,
                s.clip
                    .as_ref()
                    .map_or(String::new(), |c| a.audio(c, "clip")),
            );
        }
        body += "</div>";
    }
    body += "</section>";

    // Tasks.
    body += "<section id=\"tasks\"><h2>Effort per task</h2>";
    let rows: Vec<Vec<String>> = task_list
        .iter()
        .map(|t| {
            let bad_a = t.actions > cfg.task_max_actions;
            let bad_l = t.levels > cfg.task_max_menu_levels;
            if !t.possible {
                return vec![
                    esc(&t.name),
                    esc(&t.journey),
                    "-".into(),
                    "-".into(),
                    "-".into(),
                    "-".into(),
                    format!("<span class=\"bad\">not possible</span> {}", esc(&t.why)),
                ];
            }
            vec![
                esc(&t.name),
                esc(&t.journey),
                format!(
                    "<span class=\"{}\">{}</span> ({}p {}h {}t/{}d)",
                    if bad_a { "bad" } else { "ok" },
                    t.actions,
                    t.presses,
                    t.holds,
                    t.turns,
                    t.detents
                ),
                format!(
                    "<span class=\"{}\">{}</span>",
                    if bad_l { "bad" } else { "ok" },
                    t.levels
                ),
                format!("{:.0}", t.frames),
                format!("{}", t.wait_ms),
                if t.ok {
                    "ok".into()
                } else {
                    format!("<span class=\"bad\">{}</span>", esc(&t.why))
                },
            ]
        })
        .collect();
    body += &table(
        &[
            "task",
            "journey",
            "actions (presses, holds, turns/detents)",
            "menu levels",
            "frames",
            "forced wait ms",
            "result",
        ],
        &rows,
    );
    body += "</section>";

    // Knobs.
    body += "<section id=\"knobs\"><h2>Knob and encoder feel</h2>";
    let rows: Vec<Vec<String>> = knob_table
        .iter()
        .map(|k| {
            let s = &k["stats"];
            vec![
                esc(k["control"].as_str().unwrap_or("")),
                esc(k["view"].as_str().unwrap_or("")),
                esc(k["param"].as_str().unwrap_or("")),
                num(s, "detents"),
                format!("{} {}", num(s, "span"), s["unit"].as_str().unwrap_or("")),
                num(s, "median_step"),
                num(s, "max_step"),
                num(s, "audible_span_cents"),
                num(s, "audible_span_db"),
                num(s, "dead_zone_detents"),
            ]
        })
        .collect();
    body += &table(
        &[
            "control",
            "view",
            "parameter",
            "detents",
            "span",
            "median step",
            "max step",
            "brightness span (cents)",
            "level span (dB)",
            "dead zone (detents)",
        ],
        &rows,
    );
    body += "</section>";

    // Readability.
    body += "<section id=\"readability\"><h2>Readability</h2><p class=\"muted\">Text scale found in the firmware's grid, per view; colour pairs in use with their WCAG contrast.</p>";
    if let Value::Object(m) = read_summary {
        let rows: Vec<Vec<String>> = m
            .iter()
            .map(|(v, s)| {
                let scales: Vec<String> = s["scales"]
                    .as_object()
                    .map(|o| o.iter().map(|(k, n)| format!("{k}: {n}")).collect())
                    .unwrap_or_default();
                let pairs: Vec<String> = s["pairs"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|p| {
                                format!(
                                    "{} on {} {}x {}:1",
                                    p["fg"].as_str().unwrap_or(""),
                                    p["bg"].as_str().unwrap_or(""),
                                    p["scale"],
                                    p["ratio"]
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                vec![
                    esc(v),
                    esc(&scales.join(", ")),
                    num(s, "play_texts"),
                    num(s, "play_below_min"),
                    format!("<small>{}</small>", esc(&pairs.join("; "))),
                ]
            })
            .collect();
        body += &table(
            &[
                "view",
                "text runs by scale",
                "play-time texts",
                "under the minimum",
                "colour pairs (contrast)",
            ],
            &rows,
        );
    }
    body += "</section>";

    // Consistency.
    body += "<section id=\"consistency\"><h2>Consistency</h2>";
    let mut views: Vec<String> = Vec::new();
    for c in cons_matrix {
        let v = c["view"].as_str().unwrap_or("").to_string();
        if !views.contains(&v) {
            views.push(v);
        }
    }
    let rows: Vec<Vec<String>> = consistency::CONTROLS
        .iter()
        .map(|(name, _)| {
            let mut r = vec![esc(name)];
            for v in &views {
                let c = cons_matrix
                    .iter()
                    .find(|c| c["view"] == *v && c["control"] == *name);
                r.push(match c {
                    Some(c) if c["acts"] == true => format!(
                        "acts{}",
                        if c["params"].as_array().is_some_and(|p| !p.is_empty()) {
                            format!(
                                " <small>{}</small>",
                                esc(&c["params"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .take(3)
                                    .collect::<Vec<_>>()
                                    .join(","))
                            )
                        } else {
                            String::new()
                        }
                    ),
                    Some(c) if c["documented_inert"] == true => {
                        "<span class=\"ok\">idle (documented)</span>".into()
                    }
                    Some(_) => "<span class=\"bad\">idle</span>".into(),
                    None => "-".into(),
                });
            }
            r
        })
        .collect();
    let mut head = vec!["control"];
    head.extend(views.iter().map(String::as_str));
    body += &table(&head, &rows);
    body += "</section>";

    // Feedback.
    body += "<section id=\"feedback\"><h2>Feedback speed</h2><p class=\"muted\">Milliseconds from the input to the first changed pixel, and to the value popup; four phases of the 40 ms frame.</p>";
    let rows: Vec<Vec<String>> = feed_table
        .iter()
        .map(|f| {
            let fmt = |k: &str| {
                f[k].as_array()
                    .map(|a| {
                        a.iter()
                            .map(|x| x.as_u64().map_or("-".to_string(), |v| v.to_string()))
                            .collect::<Vec<_>>()
                            .join(" / ")
                    })
                    .unwrap_or_default()
            };
            vec![
                esc(f["control"].as_str().unwrap_or("")),
                fmt("first_change_ms"),
                fmt("popup_ms"),
            ]
        })
        .collect();
    body += &table(
        &[
            "control",
            "first changed pixel (ms, four phases)",
            "popup (ms, four phases)",
        ],
        &rows,
    );
    body += "</section>";

    // Defaults.
    body += "<section id=\"musical\"><h2>Musical defaults</h2>";
    let rows: Vec<Vec<String>> = def_table
        .iter()
        .map(|d| {
            let s = &d["stats"];
            vec![
                esc(d["preset"].as_str().unwrap_or("")),
                esc(d["note"].as_str().unwrap_or("")),
                format!("{:.1}", s["rms_dbfs"].as_f64().unwrap_or(0.0)),
                format!("{:.1}", s["peak_dbfs"].as_f64().unwrap_or(0.0)),
                num(s, "clipped_samples"),
                format!("{:.2}%", 100.0 * s["dc"].as_f64().unwrap_or(0.0)),
                format!("{:.0}", s["centroid_hz"].as_f64().unwrap_or(0.0)),
                d["clip"].as_str().map_or(String::new(), |c| a.audio(c, "")),
            ]
        })
        .collect();
    body += &table(
        &[
            "preset",
            "note",
            "RMS dBFS",
            "peak dBFS",
            "clipped",
            "DC",
            "centroid Hz",
            "clip",
        ],
        &rows,
    );
    body += "</section>";

    // Config.
    body += "<section id=\"config\"><h2>Thresholds</h2><p class=\"muted\">Sources and reasoning are documented on each field in <code>host/crates/ergo/src/config.rs</code>; override with <code>--config</code>.</p><pre class=\"mono\">";
    body += &esc(&serde_json::to_string_pretty(&cfg.to_json()).unwrap());
    body += "</pre></section>";

    let imgs = serde_json::to_string(&a.imgs).unwrap();
    let auds = serde_json::to_string(&a.auds).unwrap();
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>SIDOU ergonomics report</title><style>{}</style></head><body><main>{}</main><script>const IMG={};const AUD={};</script><script>{}</script></body></html>",
        report::CSS,
        body,
        imgs.replace("</", "<\\/"),
        auds.replace("</", "<\\/"),
        report::JS
    )
}

pub fn default_build_dir() -> PathBuf {
    for c in ["../build/fm1", "build/fm1"] {
        if Path::new(c).join("platform/fm1/sidou_app.bin").exists() {
            return PathBuf::from(c);
        }
    }
    PathBuf::from("../build/fm1")
}
