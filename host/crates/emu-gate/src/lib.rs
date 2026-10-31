// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::{sid_model, workload, SID_MODEL_6581};

pub use sidou_sys::workload::scenarios;

pub fn render_host_workload(chips: u32, model: sid_model, scenario: u32) -> Vec<i16> {
    workload::render_scenario(chips, model, scenario)
}

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

pub fn render_host_scene() -> Vec<i16> {
    unsafe extern "C" fn no_mark() {}
    let mut w = Box::<sidou_sys::workload_scene>::default();
    let mut log = vec![0i16; sidou_sys::WORKLOAD_SCENE_LOG as usize];
    let n = unsafe { sidou_sys::workload_scene_run(&mut *w, Some(no_mark), log.as_mut_ptr()) };
    log.truncate(n as usize);
    log
}
