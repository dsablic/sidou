// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Host build of the SIDOU workload, for comparing against the pi32v2 build.

use std::sync::Mutex;

pub const SID_MODEL_6581: u32 = 0;

/// The workload keeps global state and is not thread-safe; tests run in parallel by default.
static LOCK: Mutex<()> = Mutex::new(());

extern "C" {
    fn workload_begin(chips: u32, model: u32);
    fn workload_render_samples(out: *mut i16, n: u32);
    fn workload_sample_count() -> u32;
}

pub fn render_host(chips: u32) -> Vec<i16> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let n = workload_sample_count();
        let mut out = vec![0i16; n as usize];
        workload_begin(chips, SID_MODEL_6581);
        workload_render_samples(out.as_mut_ptr(), n);
        out
    }
}
