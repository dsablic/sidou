<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The second core: the measuring spike

**Status:** spike, 2026-11-15, branch `cpu2-spike` from `main` `806571c4`. Nothing ships: a bench
image and emu-gate tests only; the app's default build is unchanged. It executes "The first
experiment" of the [research](2026-11-05-second-core-research.md) under
[decision 0010](../decisions/0010-second-core-reopened.md), so the owner can decide whether and
when chip B's render goes to core 1, and order it against Bluetooth connect-out (about 9-10 KB of
flash; the image has about 2.36 KB under its 579,516-byte ceiling).

## What changed since the research

- SIDOU builds from felucca-bsp `7e09895c`; `fm1_cpu1_stop.h` is vendored, `fm1_cpu1.{h,S}` and
  `fm1_guard.h` (which `fm1_cpu1.h` includes) are not.
- emu-gate's fm1-emu pin is `c7e47c4f` (v0.5.1): core 1 (`run_dual`, `released_cpu1`,
  `run_cpu1`), the flaky-core-1 mode and `cpu1_survives`. The harness still asserts "SIDOU runs on
  core 0 only".

## Steps

1. **Harness.** A dual-core bench runner in emu-gate's `common`: core 0 a step at a time as
   `run_with` does, core 1 (once released) a step after each of core 0's, both counted apart; the
   flaky mode and `cpu1_survives` as options. The default build's "core 0 only" assertion stays.
2. **Bench image** `sid_bench_cpu1` in `bench/fm1`: felucca-bsp `7e09895c`'s
   `chip/fm1_cpu1.{h,S}` (and `fm1_guard.h`) byte for byte, `FM1_CPU1=1`,
   `FM1_CPU1_FALLBACK=1`, `FM1_CPU1_TIMEOUT_US` below the 2,900 us block. Core 1 renders chip B
   (`sid_render(b, sb, 128)`) while core 0 renders chip A, then core 0 mixes, the two-chip
   workload `sid_budget` measures, on its worst-block scenario.
3. **Measure and prove in fm1-emu:**
   1. the output sample for sample equal to the one-core render (the host's two-chip render);
   2. core 0's instructions a block against one core: what is saved, and the hand-over's cost
      (`fm1_cpu1_run`, the wait's checks, the mailbox on core 1);
   3. with the flaky mode, the fallback renders the same samples on core 0 and stays on one core;
   4. core 1 held at an update's reset and at a watchdog reset, where the emulator can show it;
   5. the flash the core-1 code would add to the app (start, wait, fault path), its RAM and
      RAMTEXT, what RAMTEXT has free today, and where about 200 B could come from;
   6. whether core 1 has its own FPU, if the emulator or the docs settle it (else device-only).
4. **Results** below: numbers, verified against inferred, what only a device can tell, a
   recommendation and an estimate of the work left to ship it behind an off-by-default option.

## Results

(filled in by the spike)
