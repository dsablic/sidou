// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The built-in digi drums (decision record 0009): four short samples, synthesized here from a
//! fixed-seed noise generator, sines and exponential envelopes, no recorded material, and
//! written as 4-bit codes, the values a C64 player writes to `$D418`'s low nibble.
//!
//! The recipe, at the digi's rate (one code every 5 output samples, 24 MHz / 544 / 5, about
//! 8,824 Hz), each sample then scaled to its peak and rounded to a code (rest at 8):
//!
//! - KICK, 220 ms: a sine whose pitch falls from 188 Hz to 48 Hz (48 + 140 e^(-t/25 ms)), at
//!   an envelope that rises in 1 ms and decays with 160 ms, and a click of noise over its
//!   first milliseconds (e^(-t/2 ms), at 0.4).
//! - SNARE, 180 ms: two tones, 190 Hz (0.55, e^(-t/35 ms)) and 330 Hz (0.25, e^(-t/20 ms)),
//!   and noise through a first difference (brighter), at 0.8 with e^(-t/55 ms).
//! - CLAP, 200 ms: noise through a two-pole resonator at 1.1 kHz, Q about 3, in three bursts
//!   10 ms apart (each e^(-t/4 ms)) and a tail from 30 ms (0.7, e^(-t/60 ms)).
//! - HAT, 60 ms: noise through two first differences, e^(-t/12 ms).
//!
//! Each sample ends at its last code away from the rest, then at the rest (8). The codes are
//! packed two a byte (code i in byte i / 2, the low nibble first), after one pad code, so that
//! the interrupt's index 0 can mean "none" (song_ctl.digi_at).

use std::f64::consts::PI;

use crate::Generated;

/// The digi's rate: the output's (24 MHz / 544) over 5.
pub const RATE: f64 = 24e6 / 544.0 / 5.0;

struct Noise(u32);

impl Noise {
    /// White noise in -1..1 (xorshift32).
    fn next(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        f64::from(x) / f64::from(u32::MAX) * 2.0 - 1.0
    }
}

fn len(ms: f64) -> usize {
    (ms / 1000.0 * RATE).round() as usize
}

fn kick() -> Vec<f64> {
    let mut noise = Noise(0x6b69_636b);
    let mut phase = 0.0f64;
    (0..len(220.0))
        .map(|i| {
            let t = i as f64 / RATE;
            let f = 48.0 + 140.0 * (-t / 0.025).exp();
            phase += 2.0 * PI * f / RATE;
            let env = (1.0 - (-t / 0.001).exp()) * (-t / 0.16).exp();
            phase.sin() * env + 0.4 * noise.next() * (-t / 0.002).exp()
        })
        .collect()
}

fn snare() -> Vec<f64> {
    let mut noise = Noise(0x736e_6172);
    let mut last = 0.0;
    (0..len(180.0))
        .map(|i| {
            let t = i as f64 / RATE;
            let n = noise.next();
            let bright = (n - last) * 0.5;
            last = n;
            0.55 * (2.0 * PI * 190.0 * t).sin() * (-t / 0.035).exp()
                + 0.25 * (2.0 * PI * 330.0 * t).sin() * (-t / 0.02).exp()
                + 0.8 * bright * (-t / 0.055).exp()
        })
        .collect()
}

fn clap() -> Vec<f64> {
    let mut noise = Noise(0x636c_6170);
    // A two-pole resonator (a band-pass) at f0 with pole radius r.
    let (f0, r) = (1100.0, 0.88);
    let (a1, a2) = (2.0 * r * (2.0 * PI * f0 / RATE).cos(), -r * r);
    let (mut y1, mut y2) = (0.0, 0.0);
    (0..len(200.0))
        .map(|i| {
            let t = i as f64 / RATE;
            let y = (1.0 - r) * noise.next() + a1 * y1 + a2 * y2;
            y2 = y1;
            y1 = y;
            let burst = |at: f64| {
                if t >= at {
                    (-(t - at) / 0.004).exp()
                } else {
                    0.0
                }
            };
            let tail = if t >= 0.03 {
                0.7 * (-(t - 0.03) / 0.06).exp()
            } else {
                0.0
            };
            y * (burst(0.0) + burst(0.01) + burst(0.02) + tail)
        })
        .collect()
}

fn hat() -> Vec<f64> {
    let mut noise = Noise(0x6861_7421);
    let (mut l1, mut l2) = (0.0, 0.0);
    (0..len(60.0))
        .map(|i| {
            let t = i as f64 / RATE;
            let n = noise.next();
            let d1 = n - l1;
            l1 = n;
            let d2 = d1 - l2;
            l2 = d1;
            d2 * (-t / 0.012).exp()
        })
        .collect()
}

/// A sample as codes 0-15 (rest 8), scaled to its peak, ending at the rest.
pub fn codes(x: &[f64]) -> Vec<u8> {
    let peak = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let mut c: Vec<u8> = x
        .iter()
        .map(|v| ((v / peak * 7.49).round() + 8.0).clamp(0.0, 15.0) as u8)
        .collect();
    while c.last() == Some(&8) {
        c.pop();
    }
    c.push(8);
    c
}

/// The four samples in the DIGI chunk's order (1 KICK, 2 SNARE, 3 CLAP, 4 HAT), as codes.
pub fn samples() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("KICK", codes(&kick())),
        ("SNARE", codes(&snare())),
        ("CLAP", codes(&clap())),
        ("HAT", codes(&hat())),
    ]
}

/// The codes as gen/digi.h holds them: a pad code (0), then the samples' codes one after
/// another, two a byte, the low nibble first. Returns the bytes and where each sample's codes
/// start, with the end after the last.
pub fn packed() -> (Vec<u8>, Vec<usize>) {
    let mut codes = vec![0u8];
    let mut starts = vec![1usize];
    for (_, c) in samples() {
        codes.extend_from_slice(&c);
        starts.push(codes.len());
    }
    let bytes = codes
        .chunks(2)
        .map(|p| p[0] | p.get(1).map_or(0, |h| h << 4))
        .collect();
    (bytes, starts)
}

pub fn digi_header() -> Generated {
    let samples = samples();
    let (bytes, starts) = packed();
    let mut s = String::new();
    // REUSE-IgnoreStart
    s.push_str(
        "// SPDX-FileCopyrightText: 2026 SIDOU contributors\n\
         // SPDX-License-Identifier: GPL-3.0-or-later\n",
    );
    // REUSE-IgnoreEnd
    s.push_str(
        "/* Generated by sidou-tablegen; do not edit. To regenerate, from host/:\n\
         \x20*   cargo run --release -p sidou-tablegen digi\n\
         \x20* The built-in digi drums (decision record 0009), synthesized by\n\
         \x20* host/crates/sidou-tablegen/src/digi.rs (the recipe is there): 4-bit codes, the\n\
         \x20* values a C64 player writes to $D418's low nibble, rest at 8, one every\n\
         \x20* SONG_DIGI_SPAN output samples, two a byte: code i is byte i / 2's low nibble for\n\
         \x20* an even i, its high one for an odd i. Code 0 is a pad (index 0 is the interrupt's\n\
         \x20* \"none\"). Sample k (1 KICK, 2 SNARE, 3 CLAP, 4 HAT) is the codes SONG_DIGI_START[k - 1]\n\
         \x20* up to SONG_DIGI_START[k]. The audio path reads the codes: RAM on the device, not\n\
         \x20* flash (emu-gate tests/ram_audio.rs); the starts are the main loop's. */\n\
         #ifndef SIDOU_GEN_DIGI_H\n#define SIDOU_GEN_DIGI_H\n\n#include <stdint.h>\n\n",
    );
    s.push_str(&format!(
        "#define SONG_DIGI_SAMPLES {}u\n#define SONG_DIGI_BYTES {}u\n\n",
        samples.len(),
        bytes.len()
    ));
    let list: Vec<String> = starts.iter().map(|v| v.to_string()).collect();
    s.push_str(&format!(
        "static const uint16_t SONG_DIGI_START[SONG_DIGI_SAMPLES + 1u] = {{{}}};\n\n",
        list.join(", ")
    ));
    s.push_str(
        "#ifdef __PI32V2__\n__attribute__((section(\".data.song_digi_data\")))\n#endif\n\
         static const uint8_t SONG_DIGI_DATA[SONG_DIGI_BYTES] = {\n",
    );
    for (n, (name, c)) in samples.iter().enumerate() {
        s.push_str(&format!(
            "    /* {name}: {} codes from code {} (byte {}) */\n",
            c.len(),
            starts[n],
            starts[n] / 2
        ));
    }
    for chunk in bytes.chunks(16) {
        let line: Vec<String> = chunk.iter().map(|b| format!("0x{b:02x}")).collect();
        s.push_str("    ");
        s.push_str(&line.join(", "));
        s.push_str(",\n");
    }
    s.push_str("};\n\n#endif\n");
    Generated {
        path: "core/player/gen/digi.h",
        contents: s,
    }
}
