// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub const SID_WORST_PER_CHIP: f64 = 550.0;

pub const SID_WORST_BLOCK_ONE_CHIP: f64 = 711.0;
pub const SID_WORST_BLOCK_TWO_CHIPS: f64 = 1426.0;

pub const TAP_PER_SAMPLE: f64 = 4.5;

pub const USB_AUDIO_PER_SAMPLE: f64 = 2.0;

pub const AUDIO_TARGET: f64 = 2700.0;
pub const ISR_OVERHEAD: f64 = 100.0;
pub const TWO_CHIPS_PLUS_EFFECTS: f64 = 2300.0;

pub const PLAYER_WORST_BLOCK: f64 = 60.0;
pub const CPU6502_INSTR_PER_CYCLE: f64 = 9.33;

pub const SHADOW_FRAME_MEAN: f64 = 49_250.0;
pub const SHADOW_SLICE_WORST: u64 = 42_690;
pub const PSID_SLICE_WORST: u64 = 40_000;
pub const PSID_ISR_WORST: f64 = 120.0;
pub const DIGI_PER_SAMPLE: f64 = 9.5;
pub const SLOTS_SWITCH_WORST: f64 = 964.0;
const M2_FREE_RAM: u64 = 10_000;
const M3_TUNE_RAM: u64 = 512;
const LAB_RAM: u64 = 512;
pub const FREE_RAM_MIN: u64 = M2_FREE_RAM - M3_TUNE_RAM - LAB_RAM;
pub const REGION_FREE_MIN: u64 = 5_120;

const _: () = assert!(
    TWO_CHIPS_PLUS_EFFECTS + TAP_PER_SAMPLE + ISR_OVERHEAD + PSID_ISR_WORST <= AUDIO_TARGET
);
const _: () = assert!(
    TWO_CHIPS_PLUS_EFFECTS
        + TAP_PER_SAMPLE
        + ISR_OVERHEAD
        + PLAYER_WORST_BLOCK
        + DIGI_PER_SAMPLE
        + USB_AUDIO_PER_SAMPLE
        <= AUDIO_TARGET
);

pub const CPU_HZ: u64 = 240_000_000;
pub const POLL_HZ: u64 = 2000;
pub const POLL_INSTRUCTIONS: u64 = CPU_HZ / POLL_HZ;
