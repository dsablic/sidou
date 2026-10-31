// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::fmt::Write as _;

pub const DEVICE_SAMPLE_RATE: u32 = 44_118;
pub const PAL_CYCLES_PER_FRAME: u32 = 19_705;

pub mod bitmap_codec;
pub mod credits;
pub mod filter_limits;
pub mod filter_passband;
pub mod filter_sweeps;
pub mod font;
pub mod harte;
pub mod logo;
pub mod player;
pub mod psid;
pub mod psid_fixtures;
pub mod register_probe;
pub mod render;
pub mod residfp_engine;
pub mod scene_art;
pub mod showcase;
pub mod signal_metrics;
pub mod song;
pub mod song_embed;
pub mod starter;
pub mod title_card;
pub mod transport;

pub fn below_and_not_nan(x: f64, limit: f64) -> bool {
    x.partial_cmp(&limit) == Some(Ordering::Less)
}

pub fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}
