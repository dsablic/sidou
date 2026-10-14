// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Reads a register from SIDOU and reSIDfp at the same chip cycles while both run the
//! same register writes, for exact comparisons of OSC3 (`$1B`) and ENV3 (`$1C`).
//!
//! Neither engine renders audio here. SIDOU advances with `sid_clock`, which runs the
//! same voice-clocking code as `sid_render` for an exact number of cycles; reSIDfp
//! advances with `clockSilent`. Both put writes and reads at exact cycles, which audio
//! rendering at 44.1 kHz cannot do: a PAL frame (19705 cycles) or 100 cycles is not a
//! whole number of samples at any supported rate.

use sidou_sys::{sid, sid_clock, sid_init, sid_model, sid_read, sid_write, SID_CLOCK_PAL};

use crate::render::REF_SAMPLE_RATE;
use crate::residfp_engine::ResidfpEngine;

/// Chip cycles in one PAL frame (985248 / 50, rounded), as in the audio renders.
pub const CYCLES_PER_FRAME: u64 = 19705;

#[derive(Clone, Copy, Debug)]
pub struct TimedWrite {
    pub cycle: u64,
    pub reg: u8,
    pub value: u8,
}

/// What [`probe`] needs from an engine.
pub trait Chip {
    fn write(&mut self, reg: u8, value: u8);
    fn read(&mut self, reg: u8) -> u8;
    fn clock(&mut self, cycles: u32);
}

pub struct Sidou(Box<sid>);

impl Sidou {
    pub fn new(model: sid_model) -> Self {
        let mut s = Box::<sid>::default();
        unsafe { sid_init(&mut *s, model, SID_CLOCK_PAL, REF_SAMPLE_RATE as u32) };
        Self(s)
    }
}

impl Chip for Sidou {
    fn write(&mut self, reg: u8, value: u8) {
        unsafe { sid_write(&mut *self.0, reg, value) }
    }

    fn read(&mut self, reg: u8) -> u8 {
        unsafe { sid_read(&mut *self.0, reg) }
    }

    fn clock(&mut self, cycles: u32) {
        unsafe { sid_clock(&mut *self.0, cycles) }
    }
}

pub struct Residfp(ResidfpEngine);

impl Residfp {
    pub fn new(model: sid_model) -> Self {
        Self(ResidfpEngine::new(model, REF_SAMPLE_RATE, true))
    }
}

impl Chip for Residfp {
    fn write(&mut self, reg: u8, value: u8) {
        self.0.write(reg, value);
    }

    fn read(&mut self, reg: u8) -> u8 {
        self.0.read(reg)
    }

    fn clock(&mut self, cycles: u32) {
        self.0.clock_silent(cycles);
    }
}

/// Runs `writes` (sorted by cycle) for `total` cycles and reads `reg` every `interval`
/// cycles, at cycles `interval`, `2 * interval`, ... A write at cycle `c` happens after
/// `c` cycles have run, and before a read at the same cycle.
pub fn probe(
    chip: &mut impl Chip,
    writes: &[TimedWrite],
    reg: u8,
    interval: u64,
    total: u64,
) -> Vec<u8> {
    assert!(interval > 0);
    assert!(
        writes.windows(2).all(|w| w[0].cycle <= w[1].cycle),
        "writes must be sorted by cycle"
    );
    let mut reads = Vec::with_capacity((total / interval) as usize);
    let mut next = 0;
    let mut now = 0u64;
    let mut next_read = interval;
    while next_read <= total {
        while next < writes.len() && writes[next].cycle <= now {
            chip.write(writes[next].reg, writes[next].value);
            next += 1;
        }
        if now == next_read {
            reads.push(chip.read(reg));
            next_read += interval;
            continue;
        }
        let stop = writes
            .get(next)
            .map_or(next_read, |w| w.cycle.min(next_read));
        chip.clock(u32::try_from(stop - now).expect("step fits in u32"));
        now = stop;
    }
    reads
}

/// The first reads where the two sequences differ.
#[derive(Debug)]
pub struct Mismatch {
    /// Index of the first differing read; it was taken at cycle `(index + 1) * interval`.
    pub index: usize,
    pub ours: u8,
    pub reference: u8,
    /// How many reads differ in total.
    pub count: usize,
}

pub fn first_mismatch(ours: &[u8], reference: &[u8]) -> Option<Mismatch> {
    assert_eq!(ours.len(), reference.len());
    let mut diffs = ours
        .iter()
        .zip(reference)
        .enumerate()
        .filter(|(_, (a, b))| a != b);
    let (index, (&o, &r)) = diffs.next()?;
    Some(Mismatch {
        index,
        ours: o,
        reference: r,
        count: 1 + diffs.count(),
    })
}
