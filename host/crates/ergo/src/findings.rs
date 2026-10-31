// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sev {
    Info,
    Low,
    Medium,
    High,
}

impl Sev {
    pub fn name(self) -> &'static str {
        match self {
            Sev::Info => "info",
            Sev::Low => "low",
            Sev::Medium => "medium",
            Sev::High => "high",
        }
    }

    pub fn parse(s: &str) -> Sev {
        match s {
            "high" => Sev::High,
            "medium" => Sev::Medium,
            "low" => Sev::Low,
            _ => Sev::Info,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Overlay {
    pub col: usize,
    pub row: usize,
    pub w: usize,
    pub h: usize,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct ShotRef {
    pub path: String,
    pub caption: String,
    pub overlays: Vec<Overlay>,
}

#[derive(Clone, Debug)]
pub struct Series {
    pub name: String,
    pub points: Vec<(f64, f64)>,
}

#[derive(Clone, Debug)]
pub struct Chart {
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    pub series: Vec<Series>,
}

#[derive(Clone, Debug)]
pub struct ClipRef {
    pub label: String,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub id: String,
    pub severity: Sev,
    pub category: &'static str,
    pub title: String,
    pub rule: String,
    pub detail: String,
    pub numbers: Value,
    pub shots: Vec<ShotRef>,
    pub chart: Option<Chart>,
    pub clips: Vec<ClipRef>,
}

impl Finding {
    pub fn new(
        id: impl Into<String>,
        severity: Sev,
        category: &'static str,
        title: impl Into<String>,
        rule: impl Into<String>,
    ) -> Finding {
        Finding {
            id: id.into(),
            severity,
            category,
            title: title.into(),
            rule: rule.into(),
            detail: String::new(),
            numbers: json!({}),
            shots: Vec::new(),
            chart: None,
            clips: Vec::new(),
        }
    }

    pub fn detail(mut self, d: impl Into<String>) -> Finding {
        self.detail = d.into();
        self
    }

    pub fn numbers(mut self, n: Value) -> Finding {
        self.numbers = n;
        self
    }

    pub fn shot(mut self, s: ShotRef) -> Finding {
        self.shots.push(s);
        self
    }

    pub fn chart(mut self, c: Chart) -> Finding {
        self.chart = Some(c);
        self
    }

    pub fn clip(mut self, c: ClipRef) -> Finding {
        self.clips.push(c);
        self
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "severity": self.severity.name(),
            "category": self.category,
            "title": self.title,
            "rule": self.rule,
            "detail": self.detail,
            "numbers": self.numbers,
            "screenshots": self.shots.iter().map(|s| json!({
                "path": s.path, "caption": s.caption,
                "overlays": s.overlays.iter().map(|o| json!({
                    "col": o.col, "row": o.row, "w": o.w, "h": o.h, "label": o.label
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "clips": self.clips.iter().map(|c| json!({"label": c.label, "path": c.path})).collect::<Vec<_>>(),
            "chart": self.chart.as_ref().map(|c| json!({
                "title": c.title, "x": c.x_label, "y": c.y_label,
                "series": c.series.iter().map(|s| json!({"name": s.name,
                    "points": s.points.iter().map(|(x, y)| [*x, (y * 10.0).round() / 10.0]).collect::<Vec<_>>()})).collect::<Vec<_>>(),
            })),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Metric {
    pub key: String,
    pub value: f64,
    pub worse_when_higher: bool,
}

pub fn metric(key: impl Into<String>, value: f64, worse_when_higher: bool) -> Metric {
    Metric {
        key: key.into(),
        value,
        worse_when_higher,
    }
}

pub fn regressions(
    now: &[Metric],
    now_findings: &[Finding],
    baseline: &Value,
    tolerance: f64,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(m) = baseline.get("metrics").and_then(Value::as_object) {
        for x in now {
            if let Some(b) = m
                .get(&x.key)
                .and_then(|b| b.get("value"))
                .and_then(Value::as_f64)
            {
                let slack = tolerance * b.abs().max(1e-9);
                let worse = if x.worse_when_higher {
                    x.value > b + slack
                } else {
                    x.value < b - slack
                };
                if worse {
                    out.push(format!("{}: {:.3} -> {:.3}", x.key, b, x.value));
                }
            }
        }
    }
    if let Some(f) = baseline.get("findings").and_then(Value::as_object) {
        for x in now_findings {
            match f.get(&x.id).and_then(Value::as_str) {
                None if x.severity > Sev::Info => {
                    out.push(format!("new finding {} ({})", x.id, x.severity.name()))
                }
                Some(s) if Sev::parse(s) < x.severity => out.push(format!(
                    "finding {} rose from {} to {}",
                    x.id,
                    s,
                    x.severity.name()
                )),
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worse_metric_a_new_finding_and_a_risen_severity_are_flagged() {
        let base = json!({
            "metrics": {"a": {"value": 10.0}, "b": {"value": 10.0}, "c": {"value": 10.0}},
            "findings": {"F.1": "low", "F.2": "high"},
        });
        let now = [
            metric("a", 12.0, true),
            metric("b", 8.0, false),
            metric("c", 10.2, true),
        ];
        let fs = [
            Finding::new("F.1", Sev::High, "X", "t", "r"),
            Finding::new("F.2", Sev::Low, "X", "t", "r"),
            Finding::new("F.3", Sev::Medium, "X", "t", "r"),
            Finding::new("F.4", Sev::Info, "X", "t", "r"),
        ];
        let r = regressions(&now, &fs, &base, 0.05);
        assert_eq!(r.len(), 4, "{r:?}");
        assert!(r.iter().any(|s| s.starts_with("a:")));
        assert!(r.iter().any(|s| s.starts_with("b:")));
        assert!(r.iter().any(|s| s.contains("F.1 rose")));
        assert!(r.iter().any(|s| s.contains("new finding F.3")));
    }
}
