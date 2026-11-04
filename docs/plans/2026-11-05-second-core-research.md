<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The FM-1's second core: research

**Status:** research, 2026-11-05. No firmware changes; nothing here reaches `rel160` or `main`
without the owner's decision. Each finding is marked **verified** (read in source, a build
artefact or an issue) or **inferred** (a reading not yet checked on SIDOU's code or the device).

## Short answers

- **Can SIDOU use core 2?** Yes, as a polled worker that runs one job at a time for core 0
  inside the audio interrupt, the model every FM-1 firmware that uses it follows. **Chip B's
  render is the best first job**: it is already in SRAM, reads no flash on its main path, is
  independent of chip A within a block, and is about a quarter of the audio interrupt's budget.
- **Memory:** both cores see the same SRAM through one shared cache controller; `csync` around
  each hand-over, and each core's writes on cache lines of their own.
- **fm1-emu runs core 2** (released by `0x10008` bit 3, entry word at `0x01C7FFF8`), including
  a "flaky core 1" mode for the units that misread RAM. SIDOU's emu-gate harness does not drive
  it yet: it asserts "SIDOU runs on core 0 only". The first experiment is provable in fm1-emu.
- **Licence:** felucca-bsp's second-core files are MPL-2.0 without the "Incompatible With
  Secondary Licenses" notice, so SIDOU can link them. SIDOU's image is **already GPL-3.0-only**
  (Felucca's HAL and `usb.c`), so Felucca's GPL-3.0-only copy would not change the image's
  licence either; the BSP's copy is the one to take because it is the maintained one.
- **Costs:** about 3.3 KB of RAM (stacks and mailbox), a few hundred bytes of flash, and about
  200 bytes of RAMTEXT, **which has 16 bytes free**. RAMTEXT is the blocker, not flash.
- **Device risk:** no new bricking path; the risks are a unit whose core 1 misreads RAM, a
  boot loop if a failed start restarts, and the update path's hold. All have known defences.

## The AC79's second core

| Fact | Source | Status |
|---|---|---|
| Two pi32v2 cores, single-precision FPU, rated DC-320 MHz; custom firmwares run at 360 MHz (the SPL's `SYS_CLK`) | `docs/hardware.md` "CPU", "Clock" | verified (docs) |
| **Start:** core 1 held (`fm1_cpu1_stop`), its interrupt bank `0x1EEF300..` zeroed, its entry address written to `0x01C7FFF8` (the write-protected top of RAM, unlocked for it), `SYS_DIV` `0x10008` bit 3 set while it starts, then `C1_CON` `0x1EEE004` bit 3 set and bit 1 cleared | felucca-bsp `chip/fm1_cpu1.h` `fm1_cpu1_start` | verified (source) |
| **Reset vector:** core 1 boots through the chip's ROM, which jumps to the word at `0x01C7FFF8`; the PC limits must be open while it does (DBG bit 10, `c1_pc_limit_err`), and closed again once it reports in | same; felucca-bsp#1 | verified (source, issue) |
| **Must start at power-on**, before the audio and the timers, interrupts off; started later it never reaches its entry | felucca-bsp#1 (X0X, Melodee); Felucca `firmware/src/main.c` | verified (reports), not by SIDOU |
| **Entry code:** `cli`, `icfg = 0`, its own `usp` (2,816 B) and `ssp` (256 B), `rti` into the RAM loop | felucca-bsp `chip/fm1_cpu1.S` | verified |
| **Interrupts:** none on core 1; it runs polled. Its faults raise the exception on core 0 (debug unit errors) or on itself; `fm1_cpu1_fault_c` in `fm1_irq.h` (`FM1_CPU1`) handles both | `fm1_cpu1.h` | verified (source) |
| **Caches:** one cache controller for both cores; the way split is CPU against peripherals, not core against core; `csync` around each hand-over is enough | felucca-bsp#1 (charlesvestal) | verified as a report; not measured by SIDOU |
| **Shared SRAM:** the whole of RAM, same addresses on both cores | fm1-emu `crates/fm1-soc/src/machine.rs`, `Core1Bus`; Felucca's mailbox | verified (emulator, source) |
| **Mailbox:** a 32-byte-aligned struct: core 0's `request`, `fn`, `arg`, `check`, then on a cache line of its own core 1's `ready`, `complete`, `rejected`, `misread`, `jobs`; core 1 runs a job only if `check == fn ^ arg ^ request * 0x9E3779B1` | `fm1_cpu1.h` `fm1_cpu1_mb` | verified |
| **Inter-core lock:** `lockset`/`lockclr`, `testset`, soft IRQs exist; the mailbox does not use them | `docs/hardware.md` table; fm1-emu `crates/fm1-core/src/lib.rs` (`hw_lock`) | verified |
| **Hold:** `C1_CON` bit 1 set, then bit 3 cleared, with the bus traps off: holding cuts core 1's outstanding read and raises a delayed AXI-invalid flag on core 0 (1 in 6 Felucca updates crashed before this) | felucca-bsp `chip/fm1_cpu1_stop.h`; SIDOU `platform/fm1/hal/fm1_sys.h` (Felucca 1.6.1, the same fix) | verified |
| **Units that cannot run it:** on some FM-1s core 1 reads `0x00200000` where RAM holds 0, more often under load (audio, screen, USB together); X0X on such a unit clicked and its USB-MIDI updates stopped mid-session 4 times of 5 | charlesvestal/fm1-x0x#10 (Jangada's measurements); keremimo/melodee#17 | verified (reports); cause unknown (guess: bus or supply at 360 MHz) |
| Whether core 1 has its own FPU | `docs/hardware.md` says "a single-precision FPU" | **not verified**: SIDOU's render uses floats (6581 clamps) |

## Who uses it, and for what

| Firmware | Use | Source | Status |
|---|---|---|---|
| M-VAVE stock | synth rendering on cpu1 (its RTOS on both cores) | fm1-emu `docs/stock-firmware.md` | verified (emulator docs) |
| Melodee | origin of the worker, the check word, the bar, the fault path; `FM1_CPU1_FALLBACK 0` (restart on failure) | felucca-bsp `chip/fm1_cpu1.h` header, `LICENSING.md` | verified |
| X0X | part of its drums on core 1 since 1.0: worst case 115 % of one core down to 82 % | felucca-bsp#1; fm1-x0x `ca2d377` `firmware/src/app/plat_fm1.c` | verified |
| NuSaw, SPH-16, LOOM-8 | half the voices (NuSaw); the same API | felucca-bsp#1, `docs/EXPERIMENTAL.md` | verified (reports) |
| Felucca 1.6.1 to 1.8.x | parts of the mix (`fx.c` `mix_block`, `mix_split` by cost), MENU > SYSTEM > 2ND CORE, OFF by default, BETA; `FM1_CPU1_FALLBACK 1`; bit-exact with one core | Felucca `a4f007c` `firmware/src/felucca.c`, `fx.c`, `BUILDING.md` | verified |
| Jangada | measurements and hardening (hold before `.bss` clear, check word, cache-line layout); the split's cost estimates in Felucca | fm1-x0x#10; Felucca `LICENSING.md` | verified |
| FM-106, Bubba Machine (FreeRTOS on both cores), COSMOS (core 1 faults on the chip), FM-1 Dock, ChoralRoot | voices on both cores; an RTOS | fm1-emu `README.md`, `docs/releases/v0.4.0.md`, `docs/stock-firmware.md` | verified (emulator docs) |
| FM1 Open (m-vave/fm1_open `4c7496d`) | none found | grep for `cpu1`, "second core" | verified (grep) |

Felucca 1.7.0 moved its second core to felucca-bsp (Felucca commit `987b205`, "Hardware layer
from felucca-bsp, step 4"); 1.8.x takes the BSP as a submodule. felucca-bsp's own status is
**beta**: "runs in Melodee; this version not yet on a device" (`docs/EXPERIMENTAL.md`).

**Felucca #267** (<https://github.com/hugelton/Felucca/issues/267>, Jan Kampling, closed): a
host-proven ordering gap in 1.6.1's `fm1_cpu1.h`: core 0 synchronised before reading "done"
instead of after. Closed because the file moved to felucca-bsp; the maintainer suggested
raising it there. **Inferred:** felucca-bsp's `fm1_cpu1_wait` (at `9670b5d`, file last changed
`62e3451`) still has its `csync` before the `complete == request` read and none after it. With
one shared cache this is probably harmless, but it is the same question; SIDOU should not rely on
it without an answer (see the decisions).

## SIDOU's platform layer today

Measured from the rel160 working build of 2026-11-04 17:29 (`build/fm1/platform/fm1/sidou_app.elf` in the rel160
worktree, HEAD `d5e39c98`, docs-only commits after it; section headers read
directly):

| Region (`platform/fm1/app.ld`) | Size | Used | Free |
|---|---|---|---|
| App image (slot `0x8DFBC`, 581,564 B) | `IMAGE_CEILING` 568,000 | 542,928 | **25,072 under the ceiling** (38,636 in the slot) |
| RAMTEXT `0x01C00000`, 48 KiB (ends at the debug area `0x01C0C000`) | 49,152 | 49,136 (`_rt_end` `0x01C0BFF0`) | **16** |
| RAM `0x01C0E000`, 64 KiB (`.data` 3,296, `.bss` 45,808, `.song_ram` 8,532) | 65,536 | 57,636 | 7,900 |
| POOL `0x01C1E000` (song region 64 KiB, DAC tables, USB ring) | 368,384 | 363,056 | 5,328 |
| NOINIT `0x01C7C000` | 15,696 | 80 | 15,616 |

The roadmap's "about 25 KB left in the slot" is the room under `IMAGE_CEILING`: **verified**
(25,072 B). The 1.5.x roadmap's "RAMTEXT 80 bytes free" is now 16.

What SIDOU does with core 2 today (**verified**): `main.c` holds it first thing at every boot and
before every reset (`fm1_cpu1_halt`, vendored Felucca 1.6.1 `hal/fm1_sys.h`, `FM1_CPU1_USED`
set to 1 for that header and 0 for `fm1_irq.h`); `fm1_cpu1.{h,S}` are not vendored
(`platform/fm1/VENDORED.md`); decision 0008 says "No second core" (misreads, GPL-3.0-only);
`docs/hardware-test.md` lists core 1 as a later milestone.

### The audio budget (emu-gate `common::budgets`, `docs/design-notes.md` "Budgets")

A block is 128 frames at 44,117.6 Hz, 2.90 ms: 696,000 instructions at the 240 MHz the budgets
assume (the chip runs at 360 MHz; `docs/hardware.md`). `AUDIO_TARGET` is 2,700 instructions a
sample (345,600 a block, half of a 240 MHz core).

| Work | Instructions a sample | Where it runs |
|---|---|---|
| Chip A, worst block (`SID_WORST_BLOCK_ONE_CHIP`) | 711 | audio interrupt, RAMTEXT |
| Chip B, worst block (`SID_WORST_BLOCK_TWO_CHIPS` 1,426 less 711) | **715** | audio interrupt, RAMTEXT |
| Effects, all on (`fx_budget`) | 500 | audio interrupt |
| Slots' worst switching block | 964 a sample over the block | audio interrupt |
| Song player's worst block | 60 | audio interrupt |
| ISR, tap, USB audio | 100, 4.5, 2.0 | audio interrupt |
| The 6502 (`CPU6502_INSTR_PER_CYCLE` 9.33) | main loop; on the device 10-12 % of the CPU at worst (2026-10-25 flash day) | main loop, flash (`cpu6502_run`, 11 KB) |
| UI poll | `POLL_INSTRUCTIONS` 120,000 a pass | main loop |

### What to move first

1. **Chip B's render.** In `audio_fill` (`platform/fm1/audio.c`) the case with chip B on and no
   two-chip tune is `play_a(player, a, ...); sid_render(b, sb, 128);`: two independent renders
   after `reg_ring_drain`. Core 0 hands `sid_render(b, sb, 128)` to core 1, renders chip A, waits,
   then mixes. Core 0 saves up to **about 711 of 715 instructions a sample** (26 % of
   `AUDIO_TARGET`; about 13 % of a 240 MHz core, 31.5 M instructions a second) less the
   hand-over: a few hundred instructions a block plus the cache's cost (Felucca's split
   charges 40 a sample, `C1_HAND`: **inferred**, not measured for SIDOU). The time comes back to
   the main loop, which is where SIDOU is thinnest (MIDI floods at 98 %, K1's input 246 of 250).
   Why it is the easy one (**verified** from `docs/design-notes.md` "SRAM audio path"): the
   render is in RAMTEXT already, its tables are in `.data` and POOL, so core 1 never needs the
   flash on its main path, and a job that only runs inside the audio interrupt is never under a
   store write (core 0 writes the flash with interrupts masked, after a block). Exceptions: the
   combined waveforms' pulldown rows are in flash by design (a read, inside the same window, so
   safe). `sb` lives on core 0's interrupt stack; it should become a static, cache-line-aligned
   buffer.
2. **Two-chip `.sid` tunes** (`psid_ctl_block` renders both chips in parts at stamped writes):
   the same gain, more work (chip B's writes split from chip A's). Second.
3. **The 6502** frees 10-12 % of the main loop but runs from flash (11 KB, too large for RAMTEXT)
   all the time, so core 1 would have to be parked around every store write, and it talks to
   the voice shadow and the write ring every slice. More risk, less gain. Not first.
4. **The UI** needs the LCD's DMA, TIMER5 and the store: interrupts core 1 does not take. No.
5. **A second engine** (roadmap "Later and ideas"): the plumbing of step 1 is its base. An engine
   would run from flash on core 1 (as Felucca's jobs do), inside the audio interrupt's window.
   Its flash comes from the 25 KB under the ceiling (a raise is the owner's decision). Felucca's
   engines are MIT (hugelton/Felucca-Engines), a possible source. Sizes not measured.

### Costs

| Item | Cost | Status |
|---|---|---|
| Core 1's stacks (`fm1_cpu1.S`) | 3,072 B of `.bss` (could go to POOL) | verified size |
| Mailbox, fault record, bar | about 100 B; the bar 12 B of `.noinit` | verified (source) |
| The waiting loop (`.c1_text`, RAMTEXT) | about 50 instructions (Felucca #267 counts 50-52), about 200 B | inferred size; **RAMTEXT has 16 B** |
| Start, wait, check, fault code (flash) | under 2 KB | inferred |
| RAM after it | RAM 7,900 less 3,300 if in `.bss`; `FREE_RAM_MIN` is 8,976 counted over every area | inferred: check against emu-gate's RAM gate |

RAMTEXT room, from `docs/hardware.md` "Code in SRAM": `-Os` for `core/fx` (about 3.3 KB) or
RAMTEXT moved above the debug area, taking from POOL (5,328 B free there).

## Moving to felucca-bsp

`docs/ROADMAP.md` Platform: SIDOU takes felucca-bsp byte for byte with its flags once
felucca-bsp#15 (raw MIDI-in hook; PR #21 is SIDOU's patch) is settled; #14 and #16 are fixed in
`14610bd`. What changes for the second core (**verified** from felucca-bsp `LICENSING.md`):

- The BSP's `chip/fm1_cpu1.{h,S}`, `fm1_cpu1_stop.h` and the `FM1_CPU1` paths of `fm1_irq.h`
  are MPL-2.0 (keremimo's agreement, felucca-bsp#5; charlesvestal's API; Leo Kuroshita's start),
  combinable with GPL code under MPL-2.0 section 3.3.
- SIDOU is GPL-3.0-or-later, but its image is GPL-3.0-only today (`LICENSING.md`: Felucca's HAL,
  `usb.c`, `app.ld`, `crt0.S`, `lcd.c`). The image could become GPL-3.0-or-later only when every
  GPL-3.0-only file is replaced by the BSP's MPL-2.0 copy or SIDOU's own code (felucca-bsp
  `LICENSING.md`: copies taken from Felucca's tree stay GPL-3.0-only). So decision 0008's
  licence reason no longer stands on its own; the misread reason does.
- The BSP's default is `FM1_CPU1_FALLBACK 0` (restart); Felucca builds with 1 (one core until
  power-off). SIDOU wants 1: a restart on a bad unit costs the player sound, and a boot loop.
- `FM1_CPU1_TIMEOUT_US` is 8,000; SIDOU's block is 2,900 µs, so a late job drops a block before
  the timeout. SIDOU sets it below the block (a build flag, no fork).

## fm1-emu

**Verified** at v0.4.0 (`9de1ef0`) and at SIDOU's
pin `93cb7ee3` (2026-10-29): `fm1_soc::machine::{released_cpu1, run_cpu1, run_dual}` exist in
both.

- Core 1 starts when core 0 sets `0x10008` bit 3, at the word in `0x01C7FFF8`, in supervisor
  mode. It runs as many instructions as core 0's cycles passed, after each 1,024-instruction
  slice (the `fm1` binary interleaves instruction by instruction).
- Its own interrupt bank (`0x1EEF300`), `idle` sleeping until one is pending, `lockset`/`lockclr`
  as a real lock, `cnum` giving the core's number (since `5cbe8f5`, after the pin: `idle`).
- `--flaky-core1 RATE --seed N`: each RAM load on core 1 has bit 21 flipped at that rate,
  deterministic by seed. Felucca 1.6.1 with 2ND CORE ON falls back to one core under it.
- `--cpu1-survives` (since `b468ead`, after the pin): a fault on core 1 stops it alone.
- Not modelled: cache coherency (memory is one view; `csync` is a no-op), bus contention between
  the cores, the clock above 240 MHz, a hold's delayed AXI flag (**inferred** from reading
  `machine.rs` and `lib.rs`; not exhaustively checked).
- **SIDOU's harness:** `host/crates/emu-gate/tests/common/mod.rs` steps core 0 only, and
  `run_cycles_jit` asserts `!soc.cpu1_release` ("SIDOU runs on core 0 only").

So nothing is needed upstream to start; the harness needs a dual-core runner. Per-core
instruction counts exist in fm1-emu's `FM1_TPROF` (commit `f76fea7`, **inferred** from its
message); emu-gate's budgets count core 0's.

## Risks to the device

- **Bricking:** none new. Core 1 writes no flash in this design, the update loader and the flash
  head are untouched, and every reset already holds core 1 (`fm1_cpu1_halt`).
- **A failed start or a misreading unit:** with `FALLBACK 1` the FM-1 goes on with one core until
  power-off. With `FALLBACK 0` it restarts, and SIDOU's boot guard sends the third boot to UBOOT
  after two early deaths (`docs/design-notes.md` "Boot").
- **Recovery paths, all proven on this unit** (`docs/hardware.md` "Updates and recovery"): the
  OCT- power-on gesture; UBOOT; Flipper Zero ROM boot plus `tools/fm1-unbrick-mac`
  (2026-10-17); M-VAVE's updater or "Return to official V15".
- **Updates with core 1 on:** X0X's updates failed 4 times of 5 on a misreading unit
  (fm1-x0x#10). The update path holds core 1 before step 1 (Felucca 1.6.1's fix); a test in
  fm1-emu proves it.
- **Both cores at 360 MHz:** the misreads may come from the bus or supply at that clock (Jangada's
  guess). Whether the owner's unit misreads is unknown: no test has been run on it.

## The first experiment (fm1-emu, no device)

1. **Harness:** an emu-gate runner on `fm1_soc::machine::run_dual` (or `released_cpu1` plus
   `run_cpu1` after each slice); the "core 0 only" assertion kept for the default build. Bump the
   pin to v0.4.0 for `idle` and `--cpu1-survives`.
2. **A bench image** in `bench/fm1`, not the app: felucca-bsp's `chip/fm1_cpu1.{h,S}` and
   `fm1_cpu1_stop.h` vendored byte for byte with `FM1_CPU1=1`, `FM1_CPU1_FALLBACK=1`; core 1
   renders `sid_render(b, sb, 128)` while core 0 renders chip A, the workload of
   `SID_WORST_BLOCK_TWO_CHIPS`.
3. **Proofs:** the output sample for sample against the host build (as `sid_budget` does); core 0's
   instructions a block against the one-core run (expected: about 711 a sample less); with
   `--flaky-core1 1e-5 --seed 7` the fallback renders the same samples on core 0 and stays on
   one core; a held core 1 at an update's reset and at a watchdog reset.
4. **Then** the app behind a build option (`SIDOU_CPU2`, off by default), the RAMTEXT room found
   first, emu-gate's budgets counting core 0 and core 1 apart.
5. **Device, only with the owner's yes for each step:** felucca-bsp's `hwtest-exp` or Felucca with
   2ND CORE ON tells whether this unit misreads, then SIDOU's build; the `corex2` performance
   counters measure the real gain.

## Sources

- felucca-bsp `9670b5d` (<https://github.com/hugelton/felucca-bsp>): `chip/fm1_cpu1.h`,
  `chip/fm1_cpu1.S`, `chip/fm1_cpu1_stop.h` (last changed `62e3451`, 2026-10-24),
  `docs/EXPERIMENTAL.md`, `LICENSING.md`; issues
  [#1](https://github.com/hugelton/felucca-bsp/issues/1),
  [#5](https://github.com/hugelton/felucca-bsp/issues/5),
  [#14](https://github.com/hugelton/felucca-bsp/issues/14),
  [#15](https://github.com/hugelton/felucca-bsp/issues/15),
  [#16](https://github.com/hugelton/felucca-bsp/issues/16).
- Felucca `a4f007c` (v1.8.1 is `f4df296`; <https://github.com/hugelton/Felucca>):
  `firmware/src/felucca.c`, `main.c`, `fx.c`, `firmware/app.ld`, `BUILDING.md`, `LICENSING.md`;
  [#267](https://github.com/hugelton/Felucca/issues/267).
- fm1-x0x `ca2d377`, [fm1-x0x#10](https://github.com/charlesvestal/fm1-x0x/issues/10).
- fm1-emu v0.4.0 `9de1ef0` and pin `93cb7ee3`: `crates/fm1-soc/src/machine.rs`, `flaky.rs`,
  `crates/fm1-core/src/lib.rs`, `docs/stock-firmware.md`, `docs/releases/v0.4.0.md`.
- m-vave/fm1_open `4c7496d`.
- SIDOU: `platform/fm1/app.ld`, `audio.c`, `main.c`, `hal/fm1_sys.h`, `VENDORED.md`,
  `LICENSING.md`, `docs/design-notes.md`, `docs/hardware.md`, `docs/ROADMAP.md`,
  `docs/decisions/0008-m3-6502-and-fm1-sid.md`, `host/crates/emu-gate/tests/common/mod.rs`.

## Not verified

- Whether core 1 has its own FPU (SIDOU's render needs one).
- The hand-over's real cost on SIDOU, and the cache's behaviour under two renders.
- Whether the owner's unit misreads on core 1.
- `docs/hardware.md` says "the stock firmware is single-core"; fm1-emu's docs say it renders its
  synth on cpu1. The second looks right; that line needs checking and correcting.

## Decisions for the owner

1. **Revisit decision 0008's "No second core"?** Its licence reason no longer holds on its own
   (the image is GPL-3.0-only already, and the BSP's copy is MPL-2.0); the misread reason stands,
   with a fallback for it.
2. **Take core 2 from felucca-bsp** (MPL-2.0, byte for byte) rather than Felucca 1.6.1's copy, and
   only as part of the felucca-bsp move or ahead of it?
3. **Chip B's render as the first job**, behind `SIDOU_CPU2`, off by default?
4. **Where the RAMTEXT room comes from:** `-Os` for `core/fx`, or RAMTEXT above the debug area
   taking from POOL.
5. **`FM1_CPU1_FALLBACK 1`** (one core until power-off) and a timeout below one block, both as
   build flags.
6. **Raise the #267 ordering question on felucca-bsp** (into `docs/upstream-reports.md` first, per
   the batch rule)?
7. **Bump fm1-emu's pin to v0.4.0** and add the dual-core runner to emu-gate.
8. **A device check of this unit's core 1** (another firmware or `hwtest-exp` flashed): only with
   your yes for that step, after the fm1-emu experiment.
