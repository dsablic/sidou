// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Reference harness: renders SIDOU and reSIDfp from the same register writes and measures
//! the difference. Used by the reference tests and by `sidou-compare`. Also the generators of
//! the screen font (`font`), the About screen's credits (`credits`), the logos' bitmaps
//! (`logo`), the title cards' (`title_card`), SCENE's pixel art (`scene_art`), the song
//! model's Rust side (`song`) and the song player's (`player`).

pub mod credits;
pub mod filter_limits;
pub mod filter_sweeps;
pub mod font;
pub mod logo;
pub mod player;
pub mod register_probe;
pub mod render;
pub mod residfp_engine;
pub mod scene_art;
pub mod signal_metrics;
pub mod song;
pub mod title_card;
