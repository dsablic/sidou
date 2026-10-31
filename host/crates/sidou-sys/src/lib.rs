// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod bindings {
    #![allow(
        non_upper_case_globals,
        non_camel_case_types,
        non_snake_case,
        dead_code,
        unused_qualifications,
        clippy::all
    )]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;

extern "C" {
    pub fn cpu6502_run_hook_all(
        c: *mut cpu6502,
        limit: u32,
        stops: core::ffi::c_int,
    ) -> core::ffi::c_int;
    pub fn cpu6502_run_hook_inline(
        c: *mut cpu6502,
        limit: u32,
        stops: core::ffi::c_int,
    ) -> core::ffi::c_int;
}

extern "C" {
    pub fn sid_test_set_zero_voices(mask: u32);
    pub fn sid_init_zero_voice(s: *mut sid, model: sid_model, clock_hz: u32, sample_rate: u32);
    pub fn sid_write_zero_voice(s: *mut sid, reg: u8, value: u8);
    pub fn sid_render_zero_voice(s: *mut sid, out: *mut i16, frames: u32);
    pub fn sid_read_zero_voice(s: *mut sid, reg: u8) -> u8;
}

pub mod ilp32 {
    #![allow(non_snake_case, non_upper_case_globals, dead_code)]
    include!(concat!(env!("OUT_DIR"), "/ilp32.rs"));
}

pub mod workload;
