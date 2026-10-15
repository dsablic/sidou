// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Host build of the SIDOU workload, for comparing against the pi32v2 build.

use sidou_sys::{workload, SID_MODEL_6581};

/// The workload keeps global state and is not thread-safe; tests run in parallel by default.
/// `workload::render` holds sidou-sys's workload Mutex for the whole run.
pub fn render_host(chips: u32) -> Vec<i16> {
    workload::render(chips, SID_MODEL_6581)
}

/// Host build of the effects scenario: the workload's output through the post-chip chain
/// with every effect at full (or the worst case of `workload_fx.h`), as `bench_fx_main.c`
/// runs it on the target.
pub fn render_host_fx(chips: u32, worst: bool) -> Vec<i16> {
    let w = workload::Workload::lock();
    let mut out = vec![0i16; w.sample_count()];
    w.begin(chips, SID_MODEL_6581);
    unsafe {
        if worst {
            sidou_sys::workload_fx_begin_worst();
        } else {
            sidou_sys::workload_fx_begin();
        }
    }
    w.render_samples(&mut out);
    let n = u32::try_from(out.len()).expect("run fits in u32");
    unsafe { sidou_sys::workload_fx_process(out.as_mut_ptr(), n) };
    out
}

/// Host build of the effect-slot scenarios (`bench_slots_main.c`): the workload's output
/// through each scenario of `workload_slots.h`, their stereo outputs one after another.
pub fn render_host_slots(chips: u32) -> Vec<i16> {
    let w = workload::Workload::lock();
    let mut sid = vec![0i16; w.sample_count()];
    w.begin(chips, SID_MODEL_6581);
    w.render_samples(&mut sid);
    let frames = sidou_sys::WORKLOAD_SLOTS_FRAMES as usize;
    let count = unsafe { sidou_sys::workload_slots_count() } as usize;
    let mut out = vec![0i16; 2 * frames * count];
    for (s, chunk) in out.chunks_mut(2 * frames).enumerate() {
        unsafe {
            sidou_sys::workload_slots_begin(s as u32, sid.as_ptr(), frames as u32);
            sidou_sys::workload_slots_process(
                sid[frames..].as_ptr(),
                chunk.as_mut_ptr(),
                frames as u32,
            );
        }
    }
    let block = sidou_sys::WORKLOAD_SLOTS_SWITCH_BLOCK as usize;
    let blocks = sidou_sys::WORKLOAD_SLOTS_SWITCH_BLOCKS as usize;
    let mut sw = vec![0i16; 2 * block * blocks];
    unsafe { sidou_sys::workload_slots_switch_begin(sid.as_ptr(), frames as u32) };
    for (b, chunk) in sw.chunks_mut(2 * block).enumerate() {
        unsafe {
            sidou_sys::workload_slots_switch_block(
                sid[frames + (block * b) % frames..].as_ptr(),
                chunk.as_mut_ptr(),
                b as u32,
            )
        };
    }
    out.extend(sw);
    out
}
