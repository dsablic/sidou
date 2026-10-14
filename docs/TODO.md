<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# TODO

Follow-ups that aren't tied to a milestone task yet.

## Report fm1-emu: decrementing register block load is not executed

- **Where:** fm1-emu (github.com/joakimeriksson/fm1-emu) at rev
  `e52f4640157f490f8c3af778dd044e9d33e989e8`.
- **Problem:** fm1-isa decodes `r_4_mov_r_4` (`{R*4} = [-R]`, mask
  `0x0804fff0`, match `0x0000eb40`). fm1-core executes only the `[R+]` and
  `[R++]` block moves, so stepping it fails with
  `Unsupported { name: "r_4_mov_r_4" }`.
- **Repro:** JieLi clang 4.0.1 `-mcpu=r3 -O2` emits `41 eb 0a 00`
  (`{r1, r3} = [-r1]`) when loading the previous element of a two-float
  array, as in `interpolate()` in `core/sid/sid_filter.c`. Running our bench
  image (`build/fm1/bench/fm1/sid_bench_1.bin`, entry `0x02000000`) faults at
  PC `0x02000e50`.
- **Expected semantics** (what our workaround implements, and what the
  bit-exact host match confirms): the registers load in ascending order from
  `base - 4 * count`, and the base is not written back.
- **Our workaround:** `step_block_load_dec` in
  `host/crates/emu-gate/tests/common/mod.rs`, shared by the emulator gate
  and the boot test. Remove it once upstream supports the instruction.
- **Action:** open an issue (and possibly a PR) upstream with the above, then
  bump the pinned rev.

## fm1-emu does not model P33 or the watchdog

- **Where:** fm1-emu at rev `e52f4640157f490f8c3af778dd044e9d33e989e8`.
- **Problem:** the P33 interface (`P33_CON` 0x13E08, `P33_DAT` 0x13E0C)
  is not modelled: reads return 0, so `fm1_wdt_arm`, `fm1_wdt_feed` and
  `fm1_reboot` (Felucca's `fm1_sys.h`) run without effect. The boot test
  cannot tell whether the firmware feeds the watchdog, or whether a crash
  would reset the chip.
- **Action:** ask upstream for a P33 model with a watchdog that resets the
  emulated chip when it is not fed; until then, check watchdog behaviour on
  the device.
