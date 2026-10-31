// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use serde_json::json;

use crate::device::{Dev, Recorder, FRAME_MS};
use crate::journeys::{Journey, FS};
use crate::screen::png_bytes;

pub struct StepTimes {
    pub start: u64,
    pub end: u64,
    pub acts: Vec<(u64, u64)>,
}

pub struct Capture {
    pub journey: String,
    pub title: String,
    pub steps: Vec<serde_json::Value>,
    pub t0: u64,
    pub frames: Vec<(u64, Vec<u16>)>,
    pub audio: Vec<i16>,
    pub audio_offset_ms: f64,
    pub captions: Vec<(u64, String)>,
}

fn sample_time_ms(dev: &Dev, i: usize) -> f64 {
    let half = crate::common::HALF_FRAMES;
    let h = i / half;
    match dev.m.half_played.get(h) {
        Some(end) => {
            let end_ms = *end as f64 / 240_000.0;
            end_ms - ((half - i % half) as f64) / FS * 1000.0
        }
        None => i as f64 / FS * 1000.0,
    }
}

impl Capture {
    pub fn from(
        rec: Recorder,
        dev: &Dev,
        marks: Vec<(u64, String)>,
        times: Vec<StepTimes>,
        j: &Journey,
    ) -> Capture {
        let all = &dev.m.soc.periph.audio;
        let from = (rec.audio_from * 2).min(all.len());
        let t0 = rec.t0;
        let steps = j
            .steps
            .iter()
            .zip(&times)
            .map(|(s, t)| {
                let mut v = crate::recipe::step_json(s);
                let o = v.as_object_mut().unwrap();
                o.remove("expect");
                o.remove("harness");
                o.remove("needs");
                o.insert("ms".into(), json!(t.start - t0));
                o.insert("end_ms".into(), json!(t.end - t0));
                if let Some(inputs) = o.get_mut("inputs").and_then(|i| i.as_array_mut()) {
                    for (input, (a, b)) in inputs.iter_mut().zip(&t.acts) {
                        input["ms"] = json!(a - t0);
                        input["end_ms"] = json!(b - t0);
                    }
                }
                v
            })
            .collect();
        Capture {
            journey: j.id.clone(),
            title: j.title.clone(),
            steps,
            t0: rec.t0,
            frames: rec.frames.into_iter().map(|f| (f.ms, f.fb)).collect(),
            audio: all[from..].to_vec(),
            audio_offset_ms: sample_time_ms(dev, rec.audio_from) - rec.t0 as f64,
            captions: marks,
        }
    }

    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir.join("frames"))?;
        for (i, (_, fb)) in self.frames.iter().enumerate() {
            std::fs::write(dir.join(format!("frames/{:05}.png", i + 1)), png_bytes(fb))?;
        }
        std::fs::write(
            dir.join("audio.wav"),
            wav_stereo16(&self.audio, FS.round() as u32),
        )?;
        let frames: Vec<_> = self
            .frames
            .iter()
            .enumerate()
            .map(|(i, (ms, _))| json!({"file": format!("frames/{:05}.png", i + 1), "ms": ms - self.t0}))
            .collect();
        let captions: Vec<_> = self
            .captions
            .iter()
            .map(|(ms, c)| json!({"ms": ms - self.t0, "text": c}))
            .collect();
        let side = json!({
            "journey": self.journey,
            "title": self.title,
            "fps": 1000.0 / FRAME_MS as f64,
            "frame_ms": FRAME_MS,
            "width": 240, "height": 240,
            "frames": frames,
            "audio": {"file": "audio.wav", "rate": FS.round() as u32, "channels": 2,
                      "offset_ms": (self.audio_offset_ms * 1000.0).round() / 1000.0,
                      "samples": self.audio.len() / 2},
            "captions": captions,
            "steps": self.steps,
        });
        std::fs::write(
            dir.join("frames.json"),
            serde_json::to_string_pretty(&side).unwrap() + "\n",
        )?;
        Ok(())
    }
}

pub fn wav_stereo16(x: &[i16], rate: u32) -> Vec<u8> {
    let data = (x.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + x.len() * 2);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * 4).to_le_bytes());
    v.extend_from_slice(&4u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data.to_le_bytes());
    for s in x {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wav_has_its_header_and_data() {
        let w = wav_stereo16(&[1, -1, 2, -2], 44118);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(w.len(), 44 + 8);
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 44118);
        assert_eq!(i16::from_le_bytes(w[44..46].try_into().unwrap()), 1);
    }
}
