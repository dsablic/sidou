// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use fm1_core::Bus;
use sidou_sys::{ilp32, LIVE_VOICES};

use super::{Firmware, Machine};

pub fn held(m: &mut Machine, fw: &Firmware) -> Vec<u8> {
    let voices = fw.symbol("instrument") + ilp32::live::voice as u32;
    let mut notes = Vec::new();
    for i in 0..LIVE_VOICES {
        let v = voices + i * ilp32::live_voice::SIZE as u32;
        if m.soc.read8(v + ilp32::live_voice::gate as u32) != 0 {
            notes.push(m.soc.read8(v + ilp32::live_voice::note as u32));
        }
    }
    notes
}

pub fn power_on(fw: &Firmware, flash: &[u8]) -> Machine {
    let mut m = Machine::boot_blank(&fw.image);
    m.soc.flash.mem.copy_from_slice(flash);
    m
}
