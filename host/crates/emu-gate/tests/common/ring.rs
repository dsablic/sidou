// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use fm1_core::Bus;
use sidou_sys::ilp32;

use super::{Firmware, Machine};

pub const CHIP_B: u8 = sidou_sys::REG_RING_CHIP_B as u8;

pub fn settle(m: &mut Machine, fw: &Firmware) {
    let r = fw.symbol("ring");
    loop {
        let head = m.soc.read32(r + ilp32::reg_ring::head as u32);
        let stage = m.soc.read32(r + ilp32::reg_ring::stage as u32);
        if head == stage && m.cpu.irq_levels.is_empty() {
            return;
        }
        m.run_cycles(37);
    }
}

pub fn push(m: &mut Machine, fw: &Firmware, entries: &[(u8, u8)]) {
    let r = fw.symbol("ring");
    let size = sidou_sys::REG_RING_SIZE;
    let stage = m.soc.read32(r + ilp32::reg_ring::stage as u32);
    let tail = m.soc.read32(r + ilp32::reg_ring::tail as u32);
    assert!(
        stage - tail + entries.len() as u32 <= size,
        "the ring is full"
    );
    for (i, &(reg, value)) in entries.iter().enumerate() {
        let at = r
            + ilp32::reg_ring::e as u32
            + ((stage + i as u32) % size) * ilp32::reg_ring_entry::SIZE as u32;
        m.soc.write8(at + ilp32::reg_ring_entry::reg as u32, reg);
        m.soc
            .write8(at + ilp32::reg_ring_entry::value as u32, value);
    }
    let end = stage + entries.len() as u32;
    m.soc.write32(r + ilp32::reg_ring::stage as u32, end);
    m.soc.write32(r + ilp32::reg_ring::head as u32, end);
}

pub fn chip_b_use(m: &mut Machine, fw: &Firmware, on: bool) {
    m.soc.write8(fw.symbol("b_want"), u8::from(on));
}

pub fn note(voice: u8, freq: u16, control: u8) -> Vec<(u8, u8)> {
    let v = voice * 7;
    vec![
        (0x18, 0x0f),
        (v, freq as u8),
        (v + 1, (freq >> 8) as u8),
        (v + 5, 0x09),
        (v + 6, 0xa6),
        (v + 4, control),
    ]
}

pub fn with_b(w: &[(u8, u8)]) -> Vec<(u8, u8)> {
    w.iter().map(|&(r, v)| (r | CHIP_B, v)).collect()
}

pub fn worst_on_both_chips() -> Vec<(u8, u8)> {
    let worst: Vec<(u8, u8)> = (0..3u8).flat_map(|v| note(v, 0x0800, 0x95)).collect();
    let mut both = worst.clone();
    both.extend(with_b(&worst));
    both
}
