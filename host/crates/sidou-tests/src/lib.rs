// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Reference harness: renders SIDOU and reSIDfp from the same register writes and measures
//! the difference. Used by the reference tests and by `sidou-compare`.

pub mod render;
pub mod residfp_engine;
pub mod signal_metrics;
