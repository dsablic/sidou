// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#[derive(Clone, Debug)]
pub struct Config {
    pub screen_mm: f64,
    pub play_min_mm: f64,
    pub contrast_small: f64,
    pub contrast_large: f64,

    pub jnd_pitch_cents: f64,
    pub jnd_cutoff_cents: f64,
    pub jnd_level_db: f64,
    pub jnd_centroid_pct: f64,
    pub dead_zone_centroid_pct: f64,
    pub jnd_spectral_db: f64,
    pub min_audible_spectral_db: f64,
    pub jnd_percent: f64,
    pub max_step_cutoff_cents: f64,
    pub max_step_db: f64,
    pub max_step_percent: f64,
    pub max_step_cv: f64,
    pub dead_zone_detents: usize,
    pub min_cutoff_octaves: f64,
    pub min_audible_cents: f64,
    pub min_audible_db: f64,
    pub max_detents: usize,
    pub min_detents: usize,

    pub task_max_actions: u32,
    pub task_max_menu_levels: u32,
    pub max_feedback_ms: u64,
    pub max_popup_ms: u64,

    pub first_rms_min_db: f64,
    pub first_rms_max_db: f64,
    pub clip_level: f64,
    pub max_dc: f64,
    pub centroid_min_hz: f64,
    pub centroid_max_hz: f64,
    pub centroid_max_hz_lead: f64,
    pub silence_db: f64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            screen_mm: 23.4,
            play_min_mm: 1.5,
            contrast_small: 4.5,
            contrast_large: 3.0,
            jnd_pitch_cents: 5.0,
            jnd_cutoff_cents: 50.0,
            jnd_level_db: 1.0,
            jnd_centroid_pct: 5.0,
            dead_zone_centroid_pct: 8.0,
            jnd_spectral_db: 1.5,
            min_audible_spectral_db: 4.0,
            jnd_percent: 1.0,
            max_step_cutoff_cents: 400.0,
            max_step_db: 6.0,
            max_step_percent: 12.0,
            max_step_cv: 0.5,
            dead_zone_detents: 4,
            min_cutoff_octaves: 3.0,
            min_audible_cents: 600.0,
            min_audible_db: 3.0,
            max_detents: 64,
            min_detents: 8,
            task_max_actions: 3,
            task_max_menu_levels: 1,
            max_feedback_ms: 100,
            max_popup_ms: 200,
            first_rms_min_db: -30.0,
            first_rms_max_db: -8.0,
            clip_level: 32767.0,
            max_dc: 0.005,
            centroid_min_hz: 150.0,
            centroid_max_hz: 5000.0,
            centroid_max_hz_lead: 6500.0,
            silence_db: -60.0,
        }
    }
}

impl Config {
    pub fn set(&mut self, key: &str, val: &str) -> bool {
        let Ok(f) = val.trim().parse::<f64>() else {
            return false;
        };
        match key.trim() {
            "screen_mm" => self.screen_mm = f,
            "play_min_mm" => self.play_min_mm = f,
            "contrast_small" => self.contrast_small = f,
            "contrast_large" => self.contrast_large = f,
            "jnd_pitch_cents" => self.jnd_pitch_cents = f,
            "jnd_cutoff_cents" => self.jnd_cutoff_cents = f,
            "jnd_level_db" => self.jnd_level_db = f,
            "jnd_centroid_pct" => self.jnd_centroid_pct = f,
            "dead_zone_centroid_pct" => self.dead_zone_centroid_pct = f,
            "jnd_percent" => self.jnd_percent = f,
            "jnd_spectral_db" => self.jnd_spectral_db = f,
            "min_audible_spectral_db" => self.min_audible_spectral_db = f,
            "max_step_cutoff_cents" => self.max_step_cutoff_cents = f,
            "max_step_db" => self.max_step_db = f,
            "max_step_percent" => self.max_step_percent = f,
            "max_step_cv" => self.max_step_cv = f,
            "dead_zone_detents" => self.dead_zone_detents = f as usize,
            "min_cutoff_octaves" => self.min_cutoff_octaves = f,
            "min_audible_cents" => self.min_audible_cents = f,
            "min_audible_db" => self.min_audible_db = f,
            "max_detents" => self.max_detents = f as usize,
            "min_detents" => self.min_detents = f as usize,
            "task_max_actions" => self.task_max_actions = f as u32,
            "task_max_menu_levels" => self.task_max_menu_levels = f as u32,
            "max_feedback_ms" => self.max_feedback_ms = f as u64,
            "max_popup_ms" => self.max_popup_ms = f as u64,
            "first_rms_min_db" => self.first_rms_min_db = f,
            "first_rms_max_db" => self.first_rms_max_db = f,
            "clip_level" => self.clip_level = f,
            "max_dc" => self.max_dc = f,
            "centroid_min_hz" => self.centroid_min_hz = f,
            "centroid_max_hz" => self.centroid_max_hz = f,
            "centroid_max_hz_lead" => self.centroid_max_hz_lead = f,
            "silence_db" => self.silence_db = f,
            _ => return false,
        }
        true
    }

    pub fn from_text(text: &str) -> Result<Config, String> {
        let mut c = Config::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("config line {}: no `=`", n + 1))?;
            if !c.set(k, v) {
                return Err(format!(
                    "config line {}: unknown key or bad value: {line}",
                    n + 1
                ));
            }
        }
        Ok(c)
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "screen_mm": self.screen_mm, "play_min_mm": self.play_min_mm,
            "contrast_small": self.contrast_small, "contrast_large": self.contrast_large,
            "jnd_pitch_cents": self.jnd_pitch_cents, "jnd_cutoff_cents": self.jnd_cutoff_cents,
            "jnd_level_db": self.jnd_level_db, "jnd_centroid_pct": self.jnd_centroid_pct,
            "dead_zone_centroid_pct": self.dead_zone_centroid_pct,
            "jnd_percent": self.jnd_percent, "jnd_spectral_db": self.jnd_spectral_db,
            "min_audible_spectral_db": self.min_audible_spectral_db, "max_step_cutoff_cents": self.max_step_cutoff_cents,
            "max_step_db": self.max_step_db, "max_step_percent": self.max_step_percent,
            "max_step_cv": self.max_step_cv, "dead_zone_detents": self.dead_zone_detents,
            "min_cutoff_octaves": self.min_cutoff_octaves,
            "min_audible_cents": self.min_audible_cents, "min_audible_db": self.min_audible_db,
            "max_detents": self.max_detents, "min_detents": self.min_detents,
            "task_max_actions": self.task_max_actions,
            "task_max_menu_levels": self.task_max_menu_levels,
            "max_feedback_ms": self.max_feedback_ms, "max_popup_ms": self.max_popup_ms,
            "first_rms_min_db": self.first_rms_min_db, "first_rms_max_db": self.first_rms_max_db,
            "clip_level": self.clip_level, "max_dc": self.max_dc,
            "centroid_min_hz": self.centroid_min_hz, "centroid_max_hz": self.centroid_max_hz,
            "centroid_max_hz_lead": self.centroid_max_hz_lead,
            "silence_db": self.silence_db,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_apply_and_unknown_keys_are_refused() {
        let c = Config::from_text("# x\ntask_max_actions = 2\nplay_min_mm=2.0\n").unwrap();
        assert_eq!(c.task_max_actions, 2);
        assert!((c.play_min_mm - 2.0).abs() < 1e-12);
        assert!(Config::from_text("nope = 1").is_err());
        assert!(Config::from_text("max_dc = x").is_err());
    }
}
