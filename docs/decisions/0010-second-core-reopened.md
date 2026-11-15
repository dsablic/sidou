<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0010: The second core, reopened

**Status:** accepted, 2026-11-04 (the owner). Revisits decision 0008's "No second core".

## Context

Decision 0008 gave two reasons for one core: some FM-1 units misread memory on core 1, and the
community's dual-core code was GPL-3.0-only. The research of 2026-11-04
([second-core-research](../plans/2026-11-05-second-core-research.md)) found that:

- felucca-bsp's second-core code (`chip/fm1_cpu1.{h,S}`, `fm1_cpu1_stop.h`) is MPL-2.0, and
  SIDOU's image is already GPL-3.0-only (Felucca's HAL and `usb.c`), so the licence reason no
  longer stands. The misread reason does.
- fm1-emu (v0.4.0) runs core 1, including a mode that simulates the misreading units, so the
  work is provable in the emulator first.
- Chip B's SID render is the first candidate: it runs from SRAM, is independent of chip A, and
  its worst block is about 26 % of the audio budget (715 of the 1,426 instructions-a-sample of
  two chips' worst blocks).

## Decision

- **The second core may be used**, starting with chip B's render, behind a build option that is
  off by default until it has passed fm1-emu (sample for sample against one core, and under the
  misread mode) and a device check that has the owner's yes for that step.
- **The code comes from felucca-bsp, byte for byte with its flags** (the vendoring rule), when
  SIDOU moves to it; nothing is taken from Felucca's own tree.
- **A failed start falls back to one core until power-off** (`FM1_CPU1_FALLBACK 1`), never a
  restart; a late job's timeout is set below SIDOU's block.
- **The SRAM comes first.** Core 1's waiting loop needs about 200 bytes of RAMTEXT, which has 16
  free. The room is found in 1.7's performance pass by real simplification (the research names
  `-Os` for `core/fx` or RAMTEXT above the debug area); no budget is raised without the owner.

## Consequences

- emu-gate gains a dual-core runner; its "SIDOU runs on core 0 only" assertion goes when the
  option is on.
- The fm1-emu pin moves to v0.4.0 with this work.
- Felucca #267's ordering question goes to `docs/upstream-reports.md`.
- Unverified until measured: whether core 1 has its own FPU, the hand-over cost, and whether the
  owner's unit misreads.

## Follow-up (2026-11-15)

- **The spike measured it** ([the plan](../plans/2026-11-15-second-core-spike.md)): chip B on core 1
  plays the same samples, gives core 0 back nearly all of chip B's render, and falls back to one
  core with the same samples. The SRAM concern is gone: 1.7 moved the song player to flash and
  RAMTEXT had 4,288 bytes free.
- **Built behind `-DSIDOU_CPU2=ON`, off by default** ([design notes](../design-notes.md#second-core-sidou_cpu2)):
  felucca-bsp's `fm1_cpu1.{h,S}` byte for byte with `FM1_CPU1_FALLBACK 1`, core 1 started after
  the RAM copy, chip B's render handed over in `audio_fill`, the bar per build. The default image
  is byte for byte unchanged; the two-core image is 579,424 bytes, under the ceiling.
- **The flash gate** keeps SIDOU's units: the driver's gate is built without `FM1_CPU1` and the
  interrupt-only rule (a job only inside the audio interrupt or the boot's self-test, waited for
  before either returns; flash writes only from the main loop, interrupts masked) keeps every
  flash transaction beside an idle core 1, proven in fm1-emu.
- **A start-up self-test** closes the gap the spike found (a misread inside the render, which the
  mailbox's check word does not cover): core 1 and core 0 render the same fixed patch for 100
  blocks and any difference retires core 1.
- emu-gate builds and tests the two-core image beside the default one (`build/fm1-cpu2`, CI's
  `cpu2` shard and `tests/cpu2.rs`); the "core 0 only" assertion holds for every other image.
- **Next:** the device check, with the owner's yes for each step; shipping it to players is a
  separate decision (1,888 bytes of flash against Bluetooth connect-out's flash find).

