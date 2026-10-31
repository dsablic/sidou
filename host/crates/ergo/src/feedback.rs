// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use serde_json::{json, Value};

use crate::config::Config;
use crate::device::{panel, Dev};
use crate::findings::{metric, Finding, Metric, Sev};
use crate::journeys::run_script;

struct Case {
    name: &'static str,
    setup: &'static str,
    input: Input,
}

enum Input {
    Turn(u8),
    Press(u8),
    Pot,
    HoldTurn(u8, u8),
}

const CASES: &[Case] = &[
    Case {
        name: "K1",
        setup: "",
        input: Input::Turn(panel::K1),
    },
    Case {
        name: "K2",
        setup: "",
        input: Input::Turn(panel::K2),
    },
    Case {
        name: "K3",
        setup: "",
        input: Input::Turn(panel::K3),
    },
    Case {
        name: "K4",
        setup: "",
        input: Input::Turn(panel::K4),
    },
    Case {
        name: "MASTER",
        setup: "",
        input: Input::Pot,
    },
    Case {
        name: "EDIT+K1",
        setup: "turn SELECT 1; turn ALGORITHM 1; turn SELECT -1",
        input: Input::HoldTurn(panel::EDIT, panel::K1),
    },
    Case {
        name: "OCT+",
        setup: "",
        input: Input::Press(panel::OCT_UP),
    },
    Case {
        name: "FX",
        setup: "",
        input: Input::Press(panel::FX),
    },
    Case {
        name: "HOME (open the menu)",
        setup: "",
        input: Input::Press(panel::HOME),
    },
    Case {
        name: "SEQ (open the song views)",
        setup: "",
        input: Input::Press(panel::SEQ),
    },
    Case {
        name: "SELECT (menu)",
        setup: "press HOME; wait 900",
        input: Input::Turn(panel::SELECT),
    },
    Case {
        name: "ALGORITHM (menu, OCTAVE)",
        setup: "press HOME; wait 900; turn SELECT 3",
        input: Input::Turn(panel::ALGORITHM),
    },
    Case {
        name: "K1 (CHIP)",
        setup: "press HOME; turn ALGORITHM 1; wait 3000",
        input: Input::Turn(panel::K1),
    },
    Case {
        name: "ALGORITHM (CHIP profile)",
        setup: "press HOME; turn ALGORITHM 1; wait 3000",
        input: Input::Turn(panel::ALGORITHM),
    },
    Case {
        name: "K1 (TRACKER)",
        setup: "press SEQ; press SEQ",
        input: Input::Turn(panel::K1),
    },
];

const PHASES: [u64; 4] = [0, 10, 20, 30];
const LIMIT_MS: u64 = 600;

fn measure(build: &Path, case: &Case, phase: u64) -> (Option<u64>, Option<u64>) {
    let mut d = Dev::boot(build);
    run_script(&mut d, case.setup);
    d.settle();
    d.run(phase);
    let base = d.m.soc.lcd.fb.clone();
    let popup0 = d.popup_rows();
    let t0 = d.ms();
    match case.input {
        Input::Turn(e) => d.m.turn(e, 1),
        Input::Press(b) => d.key_down(b),
        Input::Pot => d.m.pot(512),
        Input::HoldTurn(b, e) => {
            d.key_down(b);
            d.run(30);
            d.m.turn(e, 1);
        }
    }
    let t_in = if matches!(case.input, Input::HoldTurn(..)) {
        d.ms()
    } else {
        t0
    };
    let (mut first, mut popup) = (None, None);
    let mut released = false;
    while d.ms() < t_in + LIMIT_MS && (first.is_none() || popup.is_none()) {
        d.run(1);
        if matches!(case.input, Input::Press(_)) && !released && d.ms() >= t_in + 30 {
            if let Input::Press(b) = case.input {
                d.key_up(b);
            }
            released = true;
        }
        if first.is_none() && d.m.soc.lcd.fb != base {
            first = Some(d.ms() - t_in);
        }
        if popup.is_none() && popup0 == 0 && d.popup_rows() > 0 {
            popup = Some(d.ms() - t_in);
        }
    }
    (first, popup)
}

pub struct FeedbackResult {
    pub findings: Vec<Finding>,
    pub metrics: Vec<Metric>,
    pub table: Vec<Value>,
}

pub fn run_all(build: &Path, cfg: &Config) -> FeedbackResult {
    let mut r = FeedbackResult {
        findings: Vec::new(),
        metrics: Vec::new(),
        table: Vec::new(),
    };
    let jobs: Vec<(usize, u64)> = (0..CASES.len())
        .flat_map(|i| PHASES.iter().map(move |p| (i, *p)))
        .collect();
    let done = crate::par_map(&jobs, |(i, p)| {
        eprintln!("  feedback {} +{p} ms", CASES[*i].name);
        measure(build, &CASES[*i], *p)
    });
    for (ci, c) in CASES.iter().enumerate() {
        let m: Vec<_> = done[ci * PHASES.len()..(ci + 1) * PHASES.len()].to_vec();
        let firsts: Vec<Option<u64>> = m.iter().map(|x| x.0).collect();
        let popups: Vec<Option<u64>> = m.iter().map(|x| x.1).collect();
        let worst = |v: &[Option<u64>]| {
            if v.iter().any(Option::is_none) {
                None
            } else {
                v.iter().flatten().max().copied()
            }
        };
        let best = |v: &[Option<u64>]| v.iter().flatten().min().copied();
        let (wf, wp) = (worst(&firsts), worst(&popups));
        let id = crate::slug(c.name);
        r.table.push(json!({
            "control": c.name, "first_change_ms": firsts, "popup_ms": popups,
            "worst_first_change_ms": wf, "best_first_change_ms": best(&firsts),
            "worst_popup_ms": wp,
        }));
        r.metrics.push(metric(
            format!("feedback.{}.first_change_ms", c.name),
            wf.map_or(f64::from(LIMIT_MS as u32) + 1.0, |x| x as f64),
            true,
        ));
        let rule = format!(
            "A control changes the screen within {} ms (100 ms feels immediate; the UI runs at 25 fps, 40 ms a frame).",
            cfg.max_feedback_ms
        );
        match wf {
            None => r.findings.push(
                finding(
                    format!("FEED.none.{id}"),
                    Sev::High,
                    format!(
                        "{}: nothing changes on the screen within {LIMIT_MS} ms",
                        c.name
                    ),
                    rule,
                )
                .numbers(json!({"first_change_ms": firsts})),
            ),
            Some(ms) if ms > cfg.max_feedback_ms => r.findings.push(
                finding(
                    format!("FEED.slow.{id}"),
                    if ms > 2 * cfg.max_feedback_ms {
                        Sev::Medium
                    } else {
                        Sev::Low
                    },
                    format!("{}: the screen answers in up to {ms} ms", c.name),
                    rule,
                )
                .numbers(json!({"first_change_ms": firsts, "limit_ms": cfg.max_feedback_ms})),
            ),
            _ => {}
        }
        if let Some(p) = wp {
            if p > cfg.max_popup_ms {
                r.findings.push(
                    finding(
                        format!("FEED.popup.{id}"),
                        Sev::Low,
                        format!("{}: the value popup opens after up to {p} ms", c.name),
                        format!("The value popup opens within {} ms.", cfg.max_popup_ms),
                    )
                    .numbers(json!({"popup_ms": popups})),
                );
            }
        }
    }
    r
}

fn finding(
    id: impl Into<String>,
    sev: Sev,
    title: impl Into<String>,
    rule: impl Into<String>,
) -> Finding {
    Finding::new(id, sev, "feedback", title, rule)
}
