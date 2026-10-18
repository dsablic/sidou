// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Every threshold the harness judges by, in one place, with where it comes from. Override one
//! with `--config FILE` (lines of `key = value`, `#` comments); `--print-config` shows them.
//!
//! The sources are of three kinds, and each field says which: **psychophysics** (textbook
//! just-noticeable differences, rounded), **standard** (WCAG, ISO) and **judgement** (this
//! project's own rule of thumb, to be tuned by the user as the review goes).

#[derive(Clone, Debug)]
pub struct Config {
    // ---- readability ----
    /// The FM-1's lit screen width, mm (the user's measurement; assets/prototypes/
    /// screen-readability.html uses the same). 240 px across it: 0.0975 mm a pixel.
    pub screen_mm: f64,
    /// Smallest glyph height, mm, for text a player reads while playing: the 2x font on this
    /// screen (16 px, 1.56 mm). Judgement; ISO 9241-303 / common guidance for handheld
    /// displays asks for 3-5 mm at arm's length, so 1.56 mm is already a compromise the
    /// 240 px screen forces; the rule keeps play-time text from being the 0.78 mm 1x font.
    pub play_min_mm: f64,
    /// WCAG 2.x AA contrast for normal-size text (1x here).
    pub contrast_small: f64,
    /// WCAG 2.x AA contrast for large text (2x and 3x here).
    pub contrast_large: f64,

    // ---- knobs and encoders ----
    /// Pitch JND, cents. Psychophysics: a trained listener resolves 3-6 cents on a steady
    /// mid-range tone (Moore, An Introduction to the Psychology of Hearing). Applies to
    /// anything that moves a frequency a player hears as pitch (none of the panel's knobs
    /// today; the cutoff is a timbre control, below).
    pub jnd_pitch_cents: f64,
    /// Smallest cutoff step worth a detent, cents of the cutoff frequency. Judgement: a filter
    /// edge moving by under about a quarter tone is not heard as a change on a sustained saw.
    pub jnd_cutoff_cents: f64,
    /// Level JND, dB. Psychophysics: about 1 dB for broadband sounds (Jesteadt et al.).
    pub jnd_level_db: f64,
    /// Brightness (spectral centroid) JND, percent of the centroid. Judgement: published
    /// timbre-discrimination thresholds are a few percent; 5% is used (about 85 cents).
    pub jnd_centroid_pct: f64,
    /// Brightness noise floor for dead zones (fixed-note captures jitter about 6%).
    pub dead_zone_centroid_pct: f64,
    /// Timbre JND: the RMS difference of two log spectra (metrics::spectral_distance_db), dB.
    /// Judgement: centroid and level miss changes of tone (pulse width, a distortion's tone
    /// control) that the spectrum shows; 1.5 dB over the bins that carry the sound is above what
    /// the same note differs by from one window to the next.
    pub jnd_spectral_db: f64,
    /// Smallest timbre change over a full sweep that counts as audible, same measure.
    pub min_audible_spectral_db: f64,
    /// Smallest step of an unspecified parameter, percent of its range. Judgement: a knob
    /// needs 20-100 distinguishable positions; below 1% a detent changes nothing.
    pub jnd_percent: f64,
    /// Largest step before it sounds like a jump: cutoff, cents. Judgement: a third of an
    /// octave a detent is still a smooth sweep; more is stepped.
    pub max_step_cutoff_cents: f64,
    /// Largest level step, dB (a halving is 6 dB).
    pub max_step_db: f64,
    /// Largest step of an unspecified parameter, percent of range.
    pub max_step_percent: f64,
    /// Unevenness: the spread (std/mean) of step sizes, for a curve meant to be log or linear.
    /// Judgement: 0.5 and over means some steps are 2x or more of others.
    pub max_step_cv: f64,
    /// Consecutive detents whose parameter changed but whose sound did not (centroid and level
    /// both below their JND) that make a dead zone.
    pub dead_zone_detents: usize,
    /// Fewest octaves of cutoff a K1 sweep should cover. Judgement: a filter sweep that
    /// covers under three octaves cannot open and close a sound.
    pub min_cutoff_octaves: f64,
    /// Least change of the rendered sound over a full sweep that counts as audible: the
    /// centroid moves by this many cents, or the level by `min_audible_db`, or (resonance,
    /// pulse width) the centroid range by this fraction. Judgement.
    pub min_audible_cents: f64,
    pub min_audible_db: f64,
    /// Most detents for a full sweep of one knob. Judgement: a 24-detent encoder turns once
    /// in 24; 64 detents is under three turns, a sweep a hand can make in a second.
    pub max_detents: usize,
    /// Fewest detents: under this the steps are coarse jumps.
    pub min_detents: usize,

    // ---- tasks and feedback ----
    /// Most actions (presses, holds, turns) a common task may need (docs/TODO.md: the UX
    /// pass's "no path longer than two"; the user's brief allows three).
    pub task_max_actions: u32,
    /// Most menu levels (views pushed on the screen's stack) a common task may need.
    pub task_max_menu_levels: u32,
    /// Latency from an input to the first changed pixel, ms. Standard: about 100 ms still feels
    /// immediate (Nielsen, after Miller); the UI runs at 25 fps (40 ms).
    pub max_feedback_ms: u64,
    /// Latency to the value popup, ms. Judgement: it opens 3 rows a frame, so a few frames.
    pub max_popup_ms: u64,

    // ---- musical defaults ----
    /// A first sound's level window, dBFS RMS of a held note: not buried, not hot.
    pub first_rms_min_db: f64,
    pub first_rms_max_db: f64,
    /// A sample at or past this magnitude is a clipped one (of 32768).
    pub clip_level: f64,
    /// Largest DC offset, fraction of full scale.
    pub max_dc: f64,
    /// A reasonable spectral centroid of a first sound, Hz.
    pub centroid_min_hz: f64,
    pub centroid_max_hz: f64,
    /// Below this RMS (dBFS) a window counts as silent.
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
            silence_db: -60.0,
        }
    }
}

impl Config {
    /// Sets `key` from text; false if the key is unknown or the value does not parse.
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
            "silence_db" => self.silence_db = f,
            _ => return false,
        }
        true
    }

    /// Reads `key = value` lines over the defaults.
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
