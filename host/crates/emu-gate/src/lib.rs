// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Host build of the SIDOU workload, for comparing against the pi32v2 build.

use sidou_sys::{workload, SID_MODEL_6581};

/// The workload keeps global state and is not thread-safe; tests run in parallel by default.
/// `workload::render` holds sidou-sys's workload Mutex for the whole run.
pub fn render_host(chips: u32) -> Vec<i16> {
    workload::render(chips, SID_MODEL_6581)
}
