// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::lab::BYTES;

pub struct Sound {
    pub name: &'static str,
    pub about: &'static str,
    pub bytes: [u8; BYTES],
}

const TRI: u8 = 0x10;
const SAW: u8 = 0x20;
const PUL: u8 = 0x40;
const NOI: u8 = 0x80;
const RING: u8 = 0x04;
const SYNC: u8 = 0x02;
const LP: u8 = 0x10;
const DOWN: fn(u8) -> u8 = |n| LAB_TABLE_NOTE_NONE as u8 - n;

struct Shape([u8; BYTES]);

impl Shape {
    fn new(control: u8, ad: u8, sr: u8) -> Self {
        let mut s = [0u8; BYTES];
        s[LAB_SOUND_CONTROL as usize] = control;
        s[LAB_SOUND_AD as usize] = ad;
        s[LAB_SOUND_SR as usize] = sr;
        s[LAB_SOUND_BEND_RANGE as usize] = LIVE_BEND_RANGE_DEFAULT as u8;
        s[(LAB_SOUND_ARP + LIVE_ARP_RATE) as usize] = LIVE_ARP_RATE_DEFAULT as u8;
        s[(LAB_SOUND_ARP + LIVE_ARP_SPAN) as usize] = 1;
        Shape(s).pulse(2048).filter(LP, 2047, 0, 0)
    }

    fn put16(mut self, at: u32, v: u32) -> Self {
        self.0[at as usize..][..2].copy_from_slice(&(v as u16).to_le_bytes());
        self
    }

    fn pulse(self, width: u32) -> Self {
        self.put16(LAB_SOUND_PULSE, width)
    }

    fn filter(mut self, mode: u8, cutoff: u32, resonance: u8, voices: u8) -> Self {
        self.0[LAB_SOUND_FILTER_MODE as usize] = mode;
        self.0[LAB_SOUND_RESONANCE as usize] = resonance;
        self.0[LAB_SOUND_FILTER_VOICES as usize] = voices;
        self.put16(LAB_SOUND_CUTOFF, cutoff)
    }

    fn filter_env(mut self, depth: i8, ad: u8, sr: u8) -> Self {
        self.0[LAB_SOUND_FILTER_ENV as usize] = depth as u8;
        self.0[LAB_SOUND_FILTER_ENV_AD as usize] = ad;
        self.0[LAB_SOUND_FILTER_ENV_SR as usize] = sr;
        self
    }

    fn vibrato(mut self, rate: u8, depth: u8, delay: u8) -> Self {
        self.0[LAB_SOUND_VIBRATO_RATE as usize] = rate;
        self.0[LAB_SOUND_VIBRATO_DEPTH as usize] = depth;
        self.0[LAB_SOUND_VIBRATO_DELAY as usize] = delay;
        self
    }

    fn layer(mut self, n: u32, control: u8, ad: u8, sr: u8, semis: i8, cents: i8) -> Self {
        self.0[LAB_SOUND_LAYERS as usize] = self.0[LAB_SOUND_LAYERS as usize].max(n as u8);
        let v = (LAB_SOUND_LAYER_VOICE + (n - 1) * LAB_SOUND_LAYER_VOICE_BYTES) as usize;
        self.0[v..][..3].copy_from_slice(&[control, ad, sr]);
        self.0[v + 3..][..2].copy_from_slice(&2048u16.to_le_bytes());
        self.tune(n, semis, cents)
    }

    fn tune(mut self, n: u32, semis: i8, cents: i8) -> Self {
        let t = (LAB_SOUND_LAYER_TUNING + n * LAB_SOUND_LAYER_TUNING_BYTES) as usize;
        self.0[t] = semis as u8;
        self.0[t + 1] = cents as u8;
        self
    }

    fn table(mut self, steps: &[(u8, u8)], frames: u8, loop_from: u8) -> Self {
        self.0[LAB_SOUND_TABLE as usize] = LAB_TABLE_ON as u8 | steps.len() as u8;
        self.0[LAB_SOUND_TABLE_LOOP as usize] = loop_from;
        self.0[LAB_SOUND_TABLE_RATE as usize] = frames - 1;
        for (i, &(c, n)) in steps.iter().enumerate() {
            let at = LAB_SOUND_TABLE_STEPS as usize + 2 * i;
            self.0[at] = c;
            self.0[at + 1] = n;
        }
        self
    }

    fn lfo(mut self, n: u32, shape: u32, dest: u32, rate: u8, depth: i8) -> Self {
        let at = (LAB_SOUND_LFO + n * LAB_SOUND_LFO_BYTES) as usize;
        self.0[at] = (shape | dest << LAB_LFO_DEST_SHIFT) as u8;
        self.0[at + 1] = rate;
        self.0[at + 2] = depth as u8;
        self
    }

    fn arp(mut self, rate: u8, order: u8) -> Self {
        let a = LAB_SOUND_ARP as usize;
        self.0[a + LIVE_ARP_ON as usize] = 1;
        self.0[a + LIVE_ARP_RATE as usize] = rate;
        self.0[a + LIVE_ARP_ORDER as usize] = order;
        self
    }

    fn sound(self, name: &'static str, about: &'static str) -> Sound {
        Sound {
            name,
            about,
            bytes: self.0,
        }
    }
}

pub fn patch_file(name: &str) -> String {
    let stem: Vec<String> = name
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    format!("patches/{}.json", stem.join("-"))
}

pub fn sidou_sounds() -> Vec<Sound> {
    vec![
        Shape::new(PUL, 0x08, 0xa8)
            .lfo(0, LAB_LFO_TRIANGLE, LAB_LFO_PULSE, 12, 28)
            .filter(LP, 1300, 4, 1)
            .sound(
                "PWM STRINGS",
                "a slow pulse-width sweep through a soft low-pass",
            ),
        Shape::new(SAW, 0x08, 0x88)
            .filter(LP, 300, 12, 1)
            .filter_env(40, 0x06, 0x00)
            .sound(
                "303 BASS",
                "a resonant saw bass for STEP's SLIDE and ACCENT locks",
            ),
        Shape::new(PUL, 0x00, 0xf9)
            .pulse(1536)
            .lfo(0, LAB_LFO_TRIANGLE, LAB_LFO_CUTOFF, 16, 40)
            .filter(LP, 1000, 10, 1)
            .sound(
                "WOBBLE BASS",
                "a pulse bass with the cutoff wobbling just under 1 Hz",
            ),
        Shape::new(TRI, 0x00, 0xa0)
            .table(&[(NOI, 40), (TRI, 0), (TRI, DOWN(5))], 1, 0)
            .sound(
                "NOISE KICK",
                "a noise click into a triangle that drops in pitch",
            ),
        Shape::new(PUL, 0x00, 0xc4)
            .pulse(1024)
            .table(&[(PUL, 0), (PUL, 4), (PUL, 7), (PUL, 12)], 2, 1)
            .sound("CHIP CHORD", "a major arpeggio on one key, the C64 way"),
        Shape::new(SAW, 0x00, 0xe6)
            .layer(1, SAW, 0x00, 0xe6, 0, 12)
            .layer(2, SAW, 0x00, 0xe6, -12, -8)
            .filter(LP, 1500, 6, 7)
            .sound(
                "FAT SAW",
                "three saws, detuned and an octave down, through the filter",
            ),
        Shape::new(TRI | SYNC, 0x00, 0xb8)
            .layer(1, SAW | SYNC, 0x00, 0xb8, 7, 0)
            .layer(2, TRI, 0x00, 0xb8, 0, 0)
            .lfo(0, LAB_LFO_TRIANGLE, LAB_LFO_PITCH, 4, 2)
            .sound(
                "SYNC SCREAM",
                "a saw hard-synced to a fifth below, with a slow pitch drift",
            ),
        Shape::new(TRI | RING, 0x0a, 0x08)
            .layer(1, TRI, 0x0a, 0x08, 0, 0)
            .layer(2, TRI, 0x0a, 0x08, 19, 0)
            .sound(
                "RING BELL",
                "a ring-modulated triangle that rings out like a bell",
            ),
        Shape::new(PUL, 0x00, 0xf0)
            .pulse(2048)
            .lfo(0, LAB_LFO_SAMPLE_HOLD, LAB_LFO_CUTOFF, 20, 30)
            .filter(LP, 1100, 12, 1)
            .sound(
                "S&H BLIPS",
                "random resonant low-pass steps, a new level each cycle",
            ),
        Shape::new(TRI, 0x00, 0xf2)
            .layer(1, PUL, 0x00, 0xf2, 12, 0)
            .lfo(0, LAB_LFO_TRIANGLE, LAB_LFO_VOLUME, 18, 24)
            .sound(
                "TREM ORGAN",
                "a triangle and a pulse an octave up, with a tremolo",
            ),
        Shape::new(NOI, 0x00, 0x90)
            .table(&[(NOI, 0), (TRI, 12), (NOI, 0)], 2, 0)
            .sound(
                "SID SNARE",
                "a noise burst, a pitched triangle hit, then noise again",
            ),
        Shape::new(NOI, 0x00, 0x62).sound("NOISE HAT", "a short burst of noise"),
        Shape::new(SAW, 0x8a, 0xa9)
            .filter(LP, 200, 8, 1)
            .filter_env(30, 0xaa, 0x6a)
            .vibrato(40, 12, 40)
            .sound(
                "SWEEP PAD",
                "a saw pad that opens slowly, with a late vibrato",
            ),
        Shape::new(TRI, 0x40, 0xa6).vibrato(55, 18, 25).sound(
            "FLUTE",
            "a soft triangle with a vibrato that comes in after the attack",
        ),
        Shape::new(PUL, 0x00, 0x80)
            .pulse(512)
            .table(
                &[
                    (PUL, 24),
                    (PUL, 19),
                    (PUL, 14),
                    (PUL, 9),
                    (PUL, 4),
                    (PUL, 0),
                ],
                1,
                0,
            )
            .sound(
                "LASER ZAP",
                "a thin pulse that falls two octaves in a few frames",
            ),
        Shape::new(PUL, 0x00, 0xa4).pulse(1536).arp(1, 2).sound(
            "ARP LEAD",
            "a pulse that arpeggiates the keys held, up and down",
        ),
    ]
}
