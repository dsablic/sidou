// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Serialised access to the shared benchmark workload.
//!
//! The C workload keeps global state and is neither reentrant nor thread-safe, while Rust
//! runs tests in parallel. Every caller goes through [`Workload::lock`].

use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::{
    sid_model, workload_begin, workload_begin_scenario, workload_render_samples,
    workload_sample_count, workload_scenario_name, WORKLOAD_SCENARIOS,
};

static LOCK: Mutex<()> = Mutex::new(());

/// Exclusive access to the workload for as long as it lives.
pub struct Workload {
    _guard: MutexGuard<'static, ()>,
    /// Not `Sync`: `&Workload` must not reach a second thread.
    _not_sync: PhantomData<Cell<()>>,
}

impl Workload {
    pub fn lock() -> Self {
        Self {
            _guard: LOCK.lock().unwrap_or_else(PoisonError::into_inner),
            _not_sync: PhantomData,
        }
    }

    /// Samples in one run of the workload.
    pub fn sample_count(&self) -> usize {
        unsafe { workload_sample_count() as usize }
    }

    /// Start a run with `chips` chips (clamped to [1, WORKLOAD_MAX_CHIPS]).
    pub fn begin(&self, chips: u32, model: sid_model) {
        unsafe { workload_begin(chips, model) }
    }

    /// Start a run of one of the scenarios in `workload.h` (an unknown one plays the triad).
    pub fn begin_scenario(&self, chips: u32, model: sid_model, scenario: u32) {
        unsafe { workload_begin_scenario(chips, model, scenario) }
    }

    pub fn render_samples(&self, out: &mut [i16]) {
        let n = u32::try_from(out.len()).expect("block fits in u32");
        unsafe { workload_render_samples(out.as_mut_ptr(), n) }
    }
}

/// One whole run of the workload.
pub fn render(chips: u32, model: sid_model) -> Vec<i16> {
    let w = Workload::lock();
    let mut out = vec![0i16; w.sample_count()];
    w.begin(chips, model);
    w.render_samples(&mut out);
    out
}

/// One whole run of a scenario.
pub fn render_scenario(chips: u32, model: sid_model, scenario: u32) -> Vec<i16> {
    let w = Workload::lock();
    let mut out = vec![0i16; w.sample_count()];
    w.begin_scenario(chips, model, scenario);
    w.render_samples(&mut out);
    out
}

/// Every scenario's number and name, in order.
pub fn scenarios() -> Vec<(u32, String)> {
    (0..WORKLOAD_SCENARIOS)
        .map(|i| {
            let name = unsafe { std::ffi::CStr::from_ptr(workload_scenario_name(i)) };
            (i, name.to_string_lossy().into_owned())
        })
        .collect()
}
