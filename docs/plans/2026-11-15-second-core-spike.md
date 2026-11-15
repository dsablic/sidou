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

Measured 2026-11-15 on `cpu2-spike` (`8ce5a334` plan, `1d5ec9bc` bench and tests), fm1-emu
`c7e47c4f`. "Verified" means run in fm1-emu or read in a build artefact; "inferred" means reasoned
from those; "device" means only an FM-1 can tell. The emulator counts instructions (1 IPC at the
240 MHz the budgets assume) and models neither caches, `csync`'s cost, nor the two cores sharing
the bus.

### What was built

- **Bench image** `bench/fm1` `sid_bench_cpu1` (never the app): felucca-bsp `7e09895c`'s
  `chip/fm1_cpu1.{h,S}` and `fm1_guard.h`, byte for byte, with `FM1_CPU1=1`,
  `FM1_CPU1_FALLBACK=1`, `FM1_CPU1_TIMEOUT_US=1000u`; `fm1_vec.S` for the fault path. It holds core
  1 first thing, clears RAM, starts core 1 before the render (as at power-on), and renders the
  two-chip workload with chip B's `sid_render` handed to core 1 each block while core 0 renders
  chip A, then mixes (`workload_render_two_cores`, sharing the one-core render's block set-up and
  mix). One input word picks one core (the plain `workload_render_samples`, exactly what
  `sid_bench_2` runs) or two; another asks for a reset after the run. Sharing the set-up moved
  `sid_budget`'s two-chip worst block from 1,404.7 to 1,402.8 a sample (the bench's own code, not
  the chip's); every SID, effect and slot budget gate still passes.
- **Harness** `emu-gate/tests/common/dual.rs`: core 0 a step at a time (alignment-checked, as the
  other benches), core 1 a step after each of core 0's, so the two interleave instruction by
  instruction as the `fm1` binary does. fm1-emu does not hold core 1 on felucca-bsp's hold
  (`C1_CON` bit 1, upstream report 20), so the runner holds it while `C1_CON` says so. Options:
  fm1-emu's flaky-core-1 mode, one misread of the mailbox while core 0 hands a job over, a core 1
  that stops. The "SIDOU runs on core 0 only" assertion of the app's runner is unchanged.
- **Tests** `emu-gate/tests/cpu1.rs` (six, 4 s in all), on the two-chip worst block's scenario
  (`character_noise_tri_ring_0800`, `sid_budget`'s worst at 1,402.8 a sample on the 6581).

### (a) The same samples: verified

Two cores render the same 11,025 samples as one core, bit for bit, on the 6581 and the 8580, and
one core stays within 1 LSB of the host build (as `sid_budget` checks). 88 blocks, 88 jobs.

### (b) Core 0's time: verified in the emulator, the hand-over's real cost is device-only

| Instructions a sample, core 0 | 6581 | 8580 |
|---|---|---|
| Worst block, one core | 1,402.8 | 1,395.0 |
| Worst block, chip B on core 1 | 710.8 | 711.3 |
| Saved on the worst block | **692.0** (88,578 a block) | **683.8** |
| Saved on average over the scenario | 366.1 | 334.3 |
| Hand-over on core 0 (`fm1_cpu1_run`, the wait's checks), mean / worst | 0.42 / 0.48 (54 / 61 a block) | 0.42 / 0.48 |
| Core 0 waiting for core 1 (chip B's block longer than chip A's), mean / worst | 0.36 / 12.3 | 0.52 / 20.1 |
| Core 1's job, worst | 703.8 | 704.3 |

- The worst block on core 0 drops to chip A's alone (`SID_WORST_BLOCK_ONE_CHIP` is 711): 692 of
  the 715 chip B costs come back, **12.7 % of a 240 MHz core** in the worst block (8.5 % at
  360 MHz), 6-7 % on average.
- The hand-over costs core 0 about 60 instructions a block in the emulator, where `csync` is free
  and memory is one view. Felucca's split charges 40 a sample (`C1_HAND`, 5,120 a block) for the
  same hand-over on the device; even at that the saving stays above 650 a sample. **Device:** the
  `csync`s, the shared cache's line traffic and two cores rendering from SRAM at once.
- Core 1 between jobs polls its mailbox: 3 instructions and one load a round.

### (c) The fallback: verified, with one gap found

- **fm1-emu's flaky mode** (`1e-5`, seed 7, the research's parameters): core 1 misreads its
  mailbox while core 0 is still setting up, core 0 retires it at its first hand-over, and every
  block renders on core 0: the same samples as one core, `gave_up` = MISREAD, core 1 held.
- **One misread mid-song** (the mailbox's `request`, misread after 40 jobs, as core 0 hands the
  41st over): core 1 refuses the job, core 0 renders chip B itself in that block and every later
  one, the same samples; no job runs after the misread.
- **A core 1 that stops answering** (idle, after its start): the hand-over times out after
  1,000 us, core 0 renders the block itself, `gave_up` = LATE, the same samples. A core 1 that
  stops in the middle of a job would leave chip B's state part-advanced, and core 0 renders the
  block from there: one wrong block, then on (inferred, not tested; the job is never run twice).
  1,000 us is 2.6 times chip B's worst job at 240 MHz and leaves the block room to finish.
- **The gap: the check word guards the mailbox, not the job's data.** A misread inside chip B's
  render reaches the output and nothing notices it (a seed sweep, 40 seeds each, in the bench):

  | Misread rate a load | Fell back, same samples | Output differed, never fell back |
  |---|---|---|
  | 1e-5 | 38 (all at the start, before a job) | 2 |
  | 3e-6 | 30 | 10 |
  | 1e-6 | 35 (24 never misread where it mattered) | 5 |

  The bench keeps core 1 busy most of the time, so it overstates this. In the app core 1 would
  poll about 85-90 % of each 2.9 ms block (a load every 3 instructions) and render chip B in the
  rest (a load every ~6.7): about 4-6 % of a misreading unit's misreads would land in the render
  (inferred, from the emulator's uniform rate). A unit's first misread then most likely retires
  core 1 within microseconds, but up to 1 in ~20 bad units would first play a wrong block or carry a
  wrong value in chip B's state (a phase, an envelope count, a filter state) on to the next note.
  Jangada saw misreads "more often under load", which makes the render's share larger
  (**device**). The worker's own `jobs++` is a read-modify-write that a misread writes back wrong
  (seen in the emulator: 88 jobs counted as 2,097,240); it is a statistic only.
- **Mitigation, not built:** a start-up self-test, after `fm1_cpu1_start`: core 1 renders a fixed
  patch for ~100 blocks, core 0 renders the same and compares, and any difference retires core 1
  (about 1.3 M core-1 loads in ~40 ms: a unit misreading at 1e-6 a load misreads in it 3 times
  in 4, at 1e-5 always; not every misread changes the output, so it catches somewhat fewer). Together with the mailbox check it leaves only units that misread rarely.

### (d) Resets: partly verified

- **Verified:** a reset asked for while core 1 runs (`fm1_reboot`: P33 `PR_PWR`, the path
  `fm1_enter_uboot` also takes) finds core 1 held when it lands, and the next boot starts it
  again and renders the same samples on two cores. After a fallback, the bar (`.noinit`) keeps
  the next boot on one core (`fm1_cpu1_start` returns -1, `gave_up` = BARRED, no jobs), with the
  same samples.
- **Not run:** the update path (`fm1_enter_update`) holds core 1 with the same `fm1_cpu1_stop`
  before it writes the update record (read in `fm1_sys.h`), but resets through `PWR_CON`, which
  fm1-emu does not model (upstream report 11, item 5).
- **Device:** whether the chip's watchdog reset resets core 1 too. fm1-emu drops core 1 on every
  reset, so it cannot tell. SIDOU's boot holds core 1 as its first call (`fm1_cstart`), before it
  clears RAM, so a core 1 that survived would be held before it could matter (inferred).

### (e) Flash, RAM and RAMTEXT: measured on a throwaway app build

A sizing build of the app (`-DSIDOU_CPU2=ON`, not committed: `main.c` defines `FM1_CPU1`, starts
core 1 after the RAM copy in `fm1_cstart`, and puts the hand-over in RAMTEXT; `audio_fill` hands
chip B over in the plain one-tune case; `app.ld` puts `.c1_text` and `fm1_cpu1_run`, `_wait`,
`_retire` in `.ram_text` and core 1's stacks in NOINIT):

| | Default build | With core 1 | Cost |
|---|---|---|---|
| Image (ceiling 579,516) | 577,152 (2,364 under) | 578,608 (**908 under**) | **+1,456 B** flash |
| of which `.text` (start, bar, fault path, entry) | | | +928 |
| of which RAMTEXT (the loop 146, retire 146, run 108, wait 82, the wrappers 30, alignment) | 44,864 of 49,152 | 45,392 | **+528**, 3,760 left |
| RAM region (`REGION_FREE_MIN` 5,120) | 7,348 free | 7,204 free | +144 (mailbox, fault record, job) |
| NOINIT | 9,480 free | 6,408 free | +3,072 (core 1's stacks) |

- **RAMTEXT is not the blocker any more.** The research found 16 bytes free; 1.7 moved the song
  player to flash (49,136 to 42,704), and today 4,288 are free. Nothing has to be found.
- **Core 1's stacks must not go in `.bss`:** there the RAM region keeps 4,132 bytes, under
  `REGION_FREE_MIN`. NOINIT has room and the stacks need no clearing (the start marks them).
- **The flash gate:** felucca-bsp keeps the flash driver's "core 1 is idle" word static and wants
  `fm1_cpu1.h` in the flash driver's unit. SIDOU's driver is in `midi.c` (`ble_setting.c` and the
  flash spike's build have their own copies of the gate), the fault handler in `main.c`. In the sizing build the
  gate in `midi.c` is built without `FM1_CPU1`, so it checks core 0 and its interrupts only. That
  is safe by SIDOU's design (inferred): a job runs only inside the audio interrupt, core 0 waits
  for it before leaving, and every flash write runs with interrupts masked, so core 1 is always in
  its RAM loop then. It needs a design note and a test, or the units rearranged.
- **Against Bluetooth connect-out:** the room under the ceiling is 2,364 bytes; core 1 takes
  1,456, connect-out about 9-10 KB. Connect-out does not fit even alone, so it needs a flash find
  (the SCENE reserve, about 57 KB, is the owner's) whichever comes first; core 1 makes that find
  1.5 KB larger. Behind a build option that is off, core 1 costs the default image nothing.

### (f) Core 1's FPU: device-only

- SIDOU's render needs it: 7.5 % of the instructions a two-chip bench runs are FPU operations
  (1,497,573 of 20 M, counted by their `3f e5`/`3f f5` prefix in fm1-emu).
- fm1-emu runs one ISA on both cores, so the bench's equality says nothing about the chip.
- The stock firmware, which renders its synth on core 1 (fm1-emu's docs), would settle it if its
  core 1 ran FPU operations; in a probe here its core 1 started at 1 ms and polled its RAM job loop
  (`0x01C024xx`) for 6 s without making a sound, and ran none. Felucca's core-1 mix is integer and
  its build has no `-mcpu=r3` (soft float). So nothing here settles it. **Inferred:** the JieLi SDK runs
  FreeRTOS SMP on both cores (Bubba Machine), which suggests symmetric cores.
- If core 1 has no FPU and traps on one, the fault path retires it (one core, same sound); if it
  executes them wrongly, the self-test above would catch it at boot. The first device run tells.

## What only a device can tell

1. Whether this unit's core 1 misreads (felucca-bsp's `hwtest-exp`, or Felucca with 2ND CORE ON),
   and how often under SIDOU's load.
2. Whether core 1 has an FPU (the first SIDOU two-core run: the same sound, or a fallback).
3. The hand-over's real cost and the slowdown of two cores rendering from SRAM at once
   (`corex2`'s counters, or TIMER4 around the render).
4. Whether the watchdog reset resets core 1.

## Recommendation

Chip B on core 1 works as designed in fm1-emu: the same samples, core 0's worst block halved
(692 of 715 instructions a sample back, 12.7 % of a 240 MHz core), every fallback path rendering
the same samples, for 1,456 bytes of flash, 528 of RAMTEXT (which now has room) and 3 KB of
NOINIT. The open risks are the device's: a misreading unit, which the mailbox check does not fully
cover (a start-up self-test closes most of it), and the FPU.

- **Do next, cheaply:** turn the sizing build into the app's `SIDOU_CPU2` option, off by default
  (the default image unchanged), with the self-test, and prove it in emu-gate with the option on.
  It costs the default image nothing and makes the device check one flash away.
- **Ship it to players only after** the device check (owner's yes for each step) and only if the
  main loop's headroom is wanted more than 1.5 KB of flash: as a runtime setting it costs the
  flash above plus its menu entry. If Bluetooth connect-out is the next flash-hungry feature, find
  its room first and count core 1's 1.5 KB in the same find.

### Estimate of the work left (behind an off-by-default option)

| Step | Days |
|---|---|
| `SIDOU_CPU2` in the app from the sizing build: start, hand-over, `app.ld` (`.c1_text`, the RAMTEXT functions, stacks in NOINIT), the bar's build id | 0.5 |
| The flash gate: a design note and a test for the interrupt-only rule, or the units rearranged | 0.5 |
| emu-gate with the option on: the app's runner stepping core 1 (`run_dual`), budgets counted per core, the RAM, RAMTEXT, image and SRAM-branch gates, sample-for-sample against the default build | 1.5-2 |
| The start-up self-test, and core 1's fault path through `fm1_vec.S` (fm1-emu's `cpu1_survives`) | 1 |
| Docs: design notes, decision 0010's follow-up, Felucca #267 and report 20 for the batch | 0.5 |
| **Total** | **4-4.5**, then a device day |
