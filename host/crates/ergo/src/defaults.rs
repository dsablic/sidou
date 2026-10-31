// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::CStr;
use std::path::Path;

use serde_json::{json, Value};

use crate::config::Config;
use crate::device::{key_of_note, Dev};
use crate::findings::{metric, Finding, Metric, Sev};
use crate::journeys::{audio_stats, write_clip, FS};
use crate::metrics::rms_dbfs;

fn preset_stats(a: &[f32], sustain: u8, cfg: &Config) -> Value {
    let from = (FS * 0.02) as usize;
    let to = (FS * 0.42) as usize;
    let mut stats = audio_stats(&a[from.min(a.len())..to.min(a.len())], cfg);
    if sustain == 0 {
        let attack = &a[..a.len().min((FS * 0.3) as usize)];
        let w = (FS * 0.05) as usize;
        let loudest = if attack.len() < w {
            rms_dbfs(attack)
        } else {
            let mut energy: f64 = attack[..w].iter().map(|s| f64::from(*s).powi(2)).sum();
            let mut peak = energy;
            for i in w..attack.len() {
                energy += f64::from(attack[i]).powi(2) - f64::from(attack[i - w]).powi(2);
                peak = peak.max(energy);
            }
            crate::metrics::db((peak / w as f64).sqrt() / 32768.0)
        };
        stats["held_rms_dbfs"] = stats["rms_dbfs"].clone();
        stats["rms_dbfs"] = json!(loudest);
        stats["level_window"] = json!("loudest 50 ms in first 300 ms");
    }
    stats
}

pub fn preset_names() -> Vec<String> {
    (0..sidou_sys::LIVE_PRESET_COUNT as u8)
        .map_while(|i| {
            let p = unsafe { sidou_sys::live_preset_get(i) };
            (!p.is_null()).then(|| {
                unsafe { CStr::from_ptr((*p).name) }
                    .to_string_lossy()
                    .into_owned()
            })
        })
        .collect()
}

pub struct DefaultsResult {
    pub findings: Vec<Finding>,
    pub metrics: Vec<Metric>,
    pub table: Vec<Value>,
}

pub fn judge(label: &str, id: &str, stats: &Value, cfg: &Config) -> Vec<Finding> {
    let f = |k: &str| stats.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let mut out = Vec::new();
    let mk = |rule: &str, sev: Sev, title: String, rule_text: String| {
        Finding::new(format!("MUS.{rule}.{id}"), sev, "musical", title, rule_text)
            .numbers(stats.clone())
    };
    let rms = f("rms_dbfs");
    if rms <= cfg.silence_db {
        out.push(mk(
            "silent",
            Sev::High,
            format!("{label}: a held note is silent"),
            "A first sound is audible.".into(),
        ));
        return out;
    }
    if rms < cfg.first_rms_min_db {
        out.push(mk(
            "quiet",
            Sev::Medium,
            format!("{label}: quiet ({rms:.1} dBFS RMS)"),
            format!(
                "A held note's level is between {} and {} dBFS RMS.",
                cfg.first_rms_min_db, cfg.first_rms_max_db
            ),
        ));
    } else if rms > cfg.first_rms_max_db {
        out.push(mk(
            "hot",
            Sev::Medium,
            format!("{label}: hot ({rms:.1} dBFS RMS)"),
            format!(
                "A held note's level is between {} and {} dBFS RMS.",
                cfg.first_rms_min_db, cfg.first_rms_max_db
            ),
        ));
    }
    if f("clipped_samples") > 0.0 {
        out.push(mk(
            "clip",
            Sev::High,
            format!("{label}: clips ({} samples)", f("clipped_samples")),
            "No sample reaches full scale.".into(),
        ));
    }
    if f("dc").abs() > cfg.max_dc {
        out.push(mk(
            "dc",
            Sev::Low,
            format!("{label}: DC offset {:.2}% of full scale", 100.0 * f("dc")),
            format!("DC offset is under {}% of full scale.", 100.0 * cfg.max_dc),
        ));
    }
    let c = f("centroid_hz");
    let ceiling = if stats["sync_or_ring"].as_bool().unwrap_or(false) {
        cfg.centroid_max_hz_lead
    } else {
        cfg.centroid_max_hz
    };
    if c < cfg.centroid_min_hz || c > ceiling {
        out.push(mk(
            "bright",
            Sev::Low,
            format!("{label}: spectral centroid {c:.0} Hz"),
            format!(
                "Brightness (centroid) is between {} and {} Hz.",
                cfg.centroid_min_hz, ceiling
            ),
        ));
    }
    out
}

pub fn run_all(build: &Path, cfg: &Config, out: &Path) -> DefaultsResult {
    let names = preset_names();
    let mut r = DefaultsResult {
        findings: Vec::new(),
        metrics: Vec::new(),
        table: Vec::new(),
    };
    let jobs: Vec<(usize, u8, &str)> = (0..names.len())
        .flat_map(|i| {
            [(53u8, "F3"), (69u8, "A4")]
                .into_iter()
                .map(move |(n, l)| (i, n, l))
        })
        .collect();
    let measured = crate::par_map(&jobs, |(i, note, _)| {
        let mut d = Dev::boot(build);
        d.m.midi(&[0xc0, *i as u8]);
        d.run(300);
        let m0 = d.audio_mark();
        d.key_down(key_of_note(*note));
        d.run(900);
        d.audio_since(m0)
    });
    for ((i, _, nname), a) in jobs.iter().zip(measured) {
        let name = &names[*i];
        {
            let preset = unsafe { &*sidou_sys::live_preset_get(*i as u8) };
            let mut stats = preset_stats(&a, preset.sustain_release >> 4, cfg);
            stats["sync_or_ring"] = json!(preset.mono_sync != 0 || preset.control & 0x06 != 0);
            let id = format!("{}.{}", crate::slug(name), nname.to_lowercase());
            let label = format!("preset {name}, {nname}");
            let clip = format!("defaults/{id}.wav");
            write_clip(&out.join(&clip), &a);
            let mut fs = judge(&label, &id, &stats, cfg);
            for f in &mut fs {
                f.clips.push(crate::findings::ClipRef {
                    label: label.clone(),
                    path: clip.clone(),
                });
            }
            r.findings.extend(fs);
            for k in ["rms_dbfs", "peak_dbfs", "centroid_hz"] {
                r.metrics.push(metric(
                    format!("musical.{id}.{k}"),
                    stats.get(k).and_then(Value::as_f64).unwrap_or(0.0),
                    k == "peak_dbfs",
                ));
            }
            r.table
                .push(json!({"preset": name, "note": nname, "stats": stats, "clip": clip}));
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_and_ring_leads_have_their_own_brightness_ceiling() {
        let cfg = Config::default();
        let mut stats = json!({"rms_dbfs": -20.0, "centroid_hz": 5200.0, "sync_or_ring": true});
        assert!(judge("lead", "lead", &stats, &cfg).is_empty());
        stats["sync_or_ring"] = json!(false);
        assert_eq!(judge("pad", "pad", &stats, &cfg)[0].id, "MUS.bright.pad");
        stats["sync_or_ring"] = json!(true);
        stats["centroid_hz"] = json!(6600.0);
        assert_eq!(judge("lead", "lead", &stats, &cfg)[0].id, "MUS.bright.lead");
    }

    #[test]
    fn a_percussive_preset_is_judged_by_its_attack() {
        let cfg = Config::default();
        let mut a = vec![0.0; (FS * 0.9) as usize];
        for (i, sample) in a.iter_mut().take((FS * 0.05) as usize).enumerate() {
            *sample = if i % 2 == 0 { 1400.0 } else { -1400.0 };
        }
        let stats = preset_stats(&a, 0, &cfg);
        assert!((stats["rms_dbfs"].as_f64().unwrap() + 27.39).abs() < 0.1);
        assert!(!judge("hit", "hit", &stats, &cfg)
            .iter()
            .any(|f| f.id == "MUS.quiet.hit"));
        assert!(preset_stats(&a, 8, &cfg)["rms_dbfs"].as_f64().unwrap() < cfg.first_rms_min_db);
    }

    #[test]
    fn silence_clipping_and_level_are_judged() {
        let cfg = Config::default();
        let good = json!({"rms_dbfs": -20.0, "peak_dbfs": -6.0, "clipped_samples": 0, "dc": 0.0, "centroid_hz": 900.0});
        assert!(judge("x", "x", &good, &cfg).is_empty());
        let bad = json!({"rms_dbfs": -40.0, "peak_dbfs": 0.0, "clipped_samples": 12, "dc": 0.02, "centroid_hz": 9000.0});
        let ids: Vec<String> = judge("x", "x", &bad, &cfg)
            .iter()
            .map(|f| f.id.clone())
            .collect();
        for k in ["MUS.quiet.x", "MUS.clip.x", "MUS.dc.x", "MUS.bright.x"] {
            assert!(ids.contains(&k.to_string()), "{ids:?}");
        }
        let silent = json!({"rms_dbfs": -90.0});
        assert_eq!(judge("x", "x", &silent, &cfg)[0].id, "MUS.silent.x");
    }
}
