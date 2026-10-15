// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Reference harness: renders SIDOU and reSIDfp from the same register writes and measures
//! the difference. Used by the reference tests and by `sidou-compare`. Also the generators of
//! the screen font (`font`), the About screen's credits (`credits`) and the logo's bitmap
//! (`logo`).

pub mod credits;
pub mod filter_sweeps;
pub mod font;
pub mod logo;
pub mod register_probe;
pub mod render;
pub mod residfp_engine;
pub mod signal_metrics;
