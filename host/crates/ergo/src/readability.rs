// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Readability: the scale, physical size and contrast of the text on each screen, judged
//! against the per-view table in readability.txt.

use std::collections::BTreeMap;

use serde_json::json;

use crate::config::Config;
use crate::device::Shot;
use crate::findings::{metric, Finding, Metric, Overlay, Sev, ShotRef};
use crate::metrics::{contrast_ratio, glyph_mm};
use crate::screen::{palette_rgb, text_runs, TextRun, COLOUR_NAMES};

pub const DEFAULT_RULES: &str = include_str!("../tables/readability.txt");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Play,
    Glance,
    Hint,
    Ignore,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Play => "play",
            Class::Glance => "glance",
            Class::Hint => "hint",
            Class::Ignore => "ignore",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Pat {
    Any,
    Note,
    Value,
    Words(Vec<String>),
    Contains(String),
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub view: String,
    pub class: Class,
    pub pat: Pat,
}

fn is_note(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() >= 3
        && (b'A'..=b'G').contains(&b[0])
        && (b[1] == b'-' || b[1] == b'#')
        && b[2].is_ascii_digit()
}

fn is_value(tok: &str) -> bool {
    tok.chars().any(|c| c.is_ascii_digit())
        && tok
            .chars()
            .all(|c| c.is_ascii_digit() || "+-.%/KkHhZzDdBbPpMm".contains(c))
}

impl Pat {
    fn matches(&self, text: &str) -> bool {
        match self {
            Pat::Any => true,
            Pat::Note => text.split_whitespace().any(is_note),
            Pat::Value => text.split_whitespace().any(is_value),
            Pat::Words(w) => {
                let t = text.to_uppercase();
                w.iter().any(|w| t.contains(w.as_str()))
            }
            Pat::Contains(s) => text.to_uppercase().contains(&s.to_uppercase()),
        }
    }
}

pub fn parse_rules(text: &str) -> Result<Vec<Rule>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let p: Vec<&str> = line.splitn(3, '|').map(str::trim).collect();
        if p.len() != 3 {
            return Err(format!(
                "readability rule line {}: want view | class | pattern",
                n + 1
            ));
        }
        let class = match p[1] {
            "play" => Class::Play,
            "glance" => Class::Glance,
            "hint" => Class::Hint,
            "ignore" => Class::Ignore,
            c => {
                return Err(format!(
                    "readability rule line {}: unknown class {c}",
                    n + 1
                ))
            }
        };
        let (kind, arg) = p[2].split_once(' ').unwrap_or((p[2], ""));
        let pat = match kind {
            "any" => Pat::Any,
            "note" => Pat::Note,
            "value" => Pat::Value,
            "words" => Pat::Words(arg.split(',').map(|s| s.trim().to_uppercase()).collect()),
            "contains" => Pat::Contains(arg.trim().to_string()),
            k => {
                return Err(format!(
                    "readability rule line {}: unknown pattern {k}",
                    n + 1
                ))
            }
        };
        out.push(Rule {
            view: p[0].to_string(),
            class,
            pat,
        });
    }
    Ok(out)
}

pub fn classify(rules: &[Rule], view: &str, text: &str) -> Class {
    rules
        .iter()
        .find(|r| (r.view == "*" || r.view == view) && r.pat.matches(text))
        .map_or(Class::Glance, |r| r.class)
}

/// A run with its judgement.
#[derive(Clone, Debug)]
pub struct RunReport {
    pub run: TextRun,
    pub class: Class,
    pub mm: f64,
    pub contrast: f64,
    pub size_ok: bool,
    pub contrast_ok: bool,
}

pub fn judge(shot: &Shot, rules: &[Rule], cfg: &Config) -> Vec<RunReport> {
    let pal = palette_rgb();
    text_runs(&shot.cells)
        .into_iter()
        .map(|run| {
            // Dots and dashes alone (an empty tracker row) are not text to read.
            let mut class = if run.text.chars().any(char::is_alphanumeric) {
                classify(rules, &shot.view, &run.text)
            } else {
                Class::Ignore
            };
            let mut inactive = false;
            for id in 0..2 {
                let th = unsafe { &*sidou_sys::theme_get(id) };
                #[cfg(theme_hint)]
                let hint = th.hint;
                #[cfg(not(theme_hint))]
                let hint = th.label;
                if run.bg == th.bg {
                    inactive |= run.fg == th.dim;
                    if class == Class::Play && [th.label, hint, th.dim].contains(&run.fg) {
                        class = Class::Glance;
                    }
                }
            }
            let mm = glyph_mm(run.scale, cfg.screen_mm);
            let contrast = contrast_ratio(pal[run.fg as usize], pal[run.bg as usize]);
            let need = if run.scale >= 2 {
                cfg.contrast_large
            } else {
                cfg.contrast_small
            };
            RunReport {
                size_ok: class != Class::Play || mm + 1e-9 >= cfg.play_min_mm,
                contrast_ok: class == Class::Ignore
                    || (inactive && class != Class::Hint)
                    || contrast + 1e-9 >= need,
                run,
                class,
                mm,
                contrast,
            }
        })
        .collect()
}

/// A screen the harness saved, with where.
pub struct Captured {
    pub name: String,
    pub path: String,
    pub shot: Shot,
}

#[derive(Default)]
pub struct ReadResult {
    pub findings: Vec<Finding>,
    pub metrics: Vec<Metric>,
    /// Per view: the text scales seen, play-text counts, and fg/bg pairs in use.
    pub summary: serde_json::Value,
}

pub fn analyse(shots: &[Captured], rules: &[Rule], cfg: &Config) -> ReadResult {
    let mut by_view: BTreeMap<String, Vec<(&Captured, Vec<RunReport>)>> = BTreeMap::new();
    for c in shots {
        by_view
            .entry(c.shot.view.clone())
            .or_default()
            .push((c, judge(&c.shot, rules, cfg)));
    }
    let mut r = ReadResult::default();
    let mut summary = serde_json::Map::new();
    for (view, list) in &by_view {
        if list
            .iter()
            .all(|(_, rs)| rs.iter().all(|x| x.class == Class::Ignore))
        {
            continue;
        }
        // Distinct play texts, and those under the size rule.
        let mut play: BTreeMap<String, (u32, bool)> = BTreeMap::new();
        let mut scales: BTreeMap<u32, u32> = BTreeMap::new();
        let mut pairs: BTreeMap<(u8, u8, u32), (f64, u32)> = BTreeMap::new();
        for (_, rs) in list {
            for x in rs {
                if x.class == Class::Ignore {
                    continue;
                }
                *scales.entry(x.run.scale).or_default() += 1;
                let e = pairs
                    .entry((x.run.fg, x.run.bg, x.run.scale))
                    .or_insert((x.contrast, 0));
                e.1 += 1;
                if x.class == Class::Play {
                    play.insert(x.run.text.clone(), (x.run.scale, x.size_ok));
                }
            }
        }
        let small: Vec<(&String, u32)> = play
            .iter()
            .filter(|(_, (_, ok))| !ok)
            .map(|(t, (s, _))| (t, *s))
            .collect();
        let pair_json: Vec<_> = pairs
            .iter()
            .map(|((fg, bg, sc), (ratio, n))| {
                json!({"fg": COLOUR_NAMES[*fg as usize], "bg": COLOUR_NAMES[*bg as usize],
                       "scale": sc, "ratio": (ratio * 100.0).round() / 100.0, "runs": n})
            })
            .collect();
        summary.insert(
            view.clone(),
            json!({
                "scales": scales.iter().map(|(k, v)| (format!("{k}x"), json!(v))).collect::<serde_json::Map<_, _>>(),
                "play_texts": play.len(),
                "play_below_min": small.len(),
                "pairs": pair_json,
            }),
        );
        r.metrics.push(metric(
            format!("read.{view}.play_below_min"),
            small.len() as f64,
            true,
        ));
        r.metrics.push(metric(
            format!("read.{view}.play_texts"),
            play.len() as f64,
            false,
        ));

        // Size finding: the shot with the most violating play runs carries the overlay.
        if !small.is_empty() {
            let worst = list
                .iter()
                .max_by_key(|(_, rs)| rs.iter().filter(|x| !x.size_ok).count())
                .unwrap();
            let overlays: Vec<Overlay> = worst
                .1
                .iter()
                .filter(|x| !x.size_ok)
                .map(|x| Overlay {
                    col: x.run.col,
                    row: x.run.row,
                    w: x.run.cells.max(1),
                    h: x.run.scale as usize,
                    label: format!("{}x {:.2} mm", x.run.scale, x.mm),
                })
                .collect();
            let share = small.len() as f64 / play.len().max(1) as f64;
            let sev = if share >= 0.5 {
                Sev::High
            } else if small.len() > 2 {
                Sev::Medium
            } else {
                Sev::Low
            };
            let samples: Vec<String> = small
                .iter()
                .take(14)
                .map(|(t, _)| format!("\"{t}\""))
                .collect();
            r.findings.push(
                Finding::new(
                    format!("READ.size.{view}"),
                    sev,
                    "readability",
                    format!(
                        "{view}: {} of {} play-time texts are under {:.2} mm",
                        small.len(),
                        play.len(),
                        cfg.play_min_mm
                    ),
                    format!(
                        "Text read while playing (values, notes, names) is at least {:.2} mm tall (2x the font on a {} mm screen).",
                        cfg.play_min_mm, cfg.screen_mm
                    ),
                )
                .detail(format!("Under the minimum: {}.", samples.join(", ")))
                .numbers(json!({
                    "play_texts": play.len(), "below_min": small.len(),
                    "glyph_mm_1x": glyph_mm(1, cfg.screen_mm), "min_mm": cfg.play_min_mm,
                }))
                .shot(ShotRef {
                    path: worst.0.path.clone(),
                    caption: format!("{} (marked: play-time text under the minimum)", worst.0.name),
                    overlays,
                }),
            );
        }

        // Contrast finding.
        let mut bad: BTreeMap<(u8, u8, u32), (f64, Vec<String>)> = BTreeMap::new();
        let mut worst_shot: Option<(usize, &Captured, Vec<Overlay>)> = None;
        let mut play_fail = false;
        for (c, rs) in list {
            let mut ov = Vec::new();
            for x in rs.iter().filter(|x| !x.contrast_ok) {
                let e = bad
                    .entry((x.run.fg, x.run.bg, x.run.scale))
                    .or_insert((x.contrast, Vec::new()));
                if e.1.len() < 6 && !e.1.contains(&x.run.text) {
                    e.1.push(x.run.text.clone());
                }
                play_fail |= x.class == Class::Play;
                ov.push(Overlay {
                    col: x.run.col,
                    row: x.run.row,
                    w: x.run.cells.max(1),
                    h: x.run.scale as usize,
                    label: format!("{:.2}:1", x.contrast),
                });
            }
            if !ov.is_empty() && worst_shot.as_ref().is_none_or(|(n, _, _)| ov.len() > *n) {
                worst_shot = Some((ov.len(), c, ov));
            }
        }
        let min_ratio = bad.values().map(|(r, _)| *r).fold(f64::MAX, f64::min);
        r.metrics.push(metric(
            format!("read.{view}.contrast_failing_pairs"),
            bad.len() as f64,
            true,
        ));
        if let Some((_, c, ov)) = worst_shot {
            let lines: Vec<String> = bad
                .iter()
                .map(|((fg, bg, sc), (ratio, ex))| {
                    format!(
                        "{} on {} at {}x: {:.2}:1 (needs {}) e.g. {}",
                        COLOUR_NAMES[*fg as usize],
                        COLOUR_NAMES[*bg as usize],
                        sc,
                        ratio,
                        if *sc >= 2 {
                            cfg.contrast_large
                        } else {
                            cfg.contrast_small
                        },
                        ex.iter()
                            .map(|t| format!("\"{t}\""))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                })
                .collect();
            let sev = if play_fail && min_ratio < cfg.contrast_large {
                Sev::High
            } else if play_fail {
                Sev::Medium
            } else {
                Sev::Low
            };
            r.findings.push(
                Finding::new(
                    format!("READ.contrast.{view}"),
                    sev,
                    "readability",
                    format!(
                        "{view}: {} colour pair(s) below the contrast minimum",
                        bad.len()
                    ),
                    format!(
                        "WCAG contrast at least {}:1 for 1x text and {}:1 for 2x and larger.",
                        cfg.contrast_small, cfg.contrast_large
                    ),
                )
                .detail(lines.join("; "))
                .numbers(json!({"failing_pairs": bad.len(), "lowest_ratio": min_ratio}))
                .shot(ShotRef {
                    path: c.path.clone(),
                    caption: format!("{} (marked: low-contrast text)", c.name),
                    overlays: ov,
                }),
            );
        }
    }
    r.summary = serde_json::Value::Object(summary);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Cell;

    #[test]
    fn label_and_hint_coloured_text_is_not_play_text() {
        for id in 0..2 {
            let th = unsafe { &*sidou_sys::theme_get(id) };
            #[cfg(theme_hint)]
            let roles = [th.label, th.hint, th.dim];
            #[cfg(not(theme_hint))]
            let roles = [th.label, th.dim];
            for role in roles {
                let s = shot_with("live", "125", role | th.bg << 4);
                let r = judge(&s, &parse_rules(DEFAULT_RULES).unwrap(), &Config::default());
                assert_eq!(r[0].class, Class::Glance);
                assert!(r[0].size_ok);
            }
        }
    }

    #[test]
    fn dim_text_is_exempt_unless_it_is_a_hint() {
        let th = unsafe { &*sidou_sys::theme_get(0) };
        let s = shot_with("live", "OFF", th.dim | th.bg << 4);
        let r = judge(&s, &parse_rules(DEFAULT_RULES).unwrap(), &Config::default());
        assert!(r[0].contrast_ok);
        let r = judge(&s, &parse_rules("* | hint | any").unwrap(), &Config::default());
        assert!(!r[0].contrast_ok);
    }

    fn shot_with(view: &str, scale1_text: &str, attr: u8) -> Shot {
        let mut cells = vec![Cell::default(); 900];
        for (i, b) in scale1_text.bytes().enumerate() {
            cells[30 + 1 + i] = Cell {
                ch: b,
                attr,
                ..Cell::default()
            };
        }
        Shot {
            fb: vec![],
            cells,
            view: view.to_string(),
            ms: 0,
        }
    }

    #[test]
    fn the_default_table_parses_and_classifies_by_content() {
        let rules = parse_rules(DEFAULT_RULES).unwrap();
        assert_eq!(classify(&rules, "live", "A-4"), Class::Play);
        assert_eq!(classify(&rules, "live", "FC 11.4K RES 6"), Class::Play);
        assert_eq!(classify(&rules, "live", "SAW LEAD"), Class::Play);
        assert_eq!(classify(&rules, "live", "FILTER LP"), Class::Glance);
        assert_eq!(classify(&rules, "about", "V0.2.0 2026"), Class::Ignore);
        assert_eq!(classify(&rules, "menu", "SELECT MOVE"), Class::Play);
    }

    #[test]
    fn a_play_value_at_1x_breaks_the_size_rule_and_a_ratio_is_computed() {
        let rules = parse_rules(DEFAULT_RULES).unwrap();
        let cfg = Config::default();
        // light blue on blue (14 on 6), the LIVE colours.
        let th = unsafe { &*sidou_sys::theme_get(1) };
        let s = shot_with("live", "FC 11.4K", th.value | th.bg << 4);
        let r = judge(&s, &rules, &cfg);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].class, Class::Play);
        assert!(!r[0].size_ok);
        assert!((r[0].mm - 0.78).abs() < 0.01);
        assert!(r[0].contrast > 1.0);
    }

    #[test]
    fn a_run_of_dots_is_not_text() {
        let rules = parse_rules(DEFAULT_RULES).unwrap();
        let s = shot_with("tracker", "---.....", 0x6e);
        assert_eq!(
            judge(&s, &rules, &Config::default())[0].class,
            Class::Ignore
        );
    }

    #[test]
    fn bad_rules_are_refused() {
        assert!(parse_rules("live | loud | any").is_err());
        assert!(parse_rules("live | play").is_err());
        assert!(parse_rules("live | play | sometimes").is_err());
    }
}
