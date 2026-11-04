<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0011: Bluetooth MIDI on the open stack

**Status:** accepted, 2026-11-04 (the owner), from
[the Bluetooth research](../plans/2026-11-05-bluetooth-research.md).

## Context

The FM-1's Bluetooth was usable only through M-VAVE's and JieLi's closed libraries, which a GPL
firmware cannot ship. Optimist's stack (Henri David, `GPL-3.0-only OR MPL-2.0`) runs Bluetooth
MIDI on felucca-bsp's radio drivers (MPL-2.0, the radio tables Apache-2.0) with nothing of
JieLi's linked; fm1-base carries it as `ble_stack = "own"`. Every part is compatible with SIDOU's
GPL-3.0-only image. Estimated for SIDOU: about 29-33 KB of flash, 6.5-8 KB of RAM (1.5 KB of it a
block the radio reads by DMA), 36-44 B of RAMTEXT; the radio's interrupts take about 0.02 % of the
core and sit below the audio interrupt.

## Decision

1. **Bluetooth MIDI comes first after 1.7 and the felucca-bsp move.** It starts as a build option,
   off by default, and goes into the main image once proven; a separate `sidou-bt` package is the
   fallback only if the flash cannot be found.
2. **Flash:** the remaining cold files at `-Oz` (about 6.8 KB) first, then what 1.7 leaves. The
   13,564 B above the ceiling stay untouched and no feature is dropped.
3. **RAM:** the stack's state that the radio's DMA does not read goes in NOINIT (15,616 B free; a
   placement rule, not a budget raise). The vendored files stay byte for byte.
4. **The power rails** that felucca-bsp raises at boot (`fm1_sys.h`, 1.60 V / 1.38 V, as M-VAVE's
   firmware sets them) are accepted with the felucca-bsp move, behind a device check with the
   owner's yes for that step.
5. **No pairing at first** (macOS and iOS connect without it; no AES in the interrupt). Pairing
   and the central role come later only if asked for.
6. **The stack is vendored from fm1-base's `bt/`** at the commit whose felucca-bsp pin SIDOU uses.
   Whether it could live in felucca-bsp itself goes in `docs/upstream-reports.md` for the next
   batch.
7. **Bluetooth audio (A2DP) is dropped** for SIDOU (about 37 KB of flash and 21 KB of RAM). **Wi-Fi
   and Ableton Link are parked** until Optimist has a Link session working on a real FM-1.

## Consequences

- The plan's steps 2 and 3 come first: a `SIDOU_BLE` build measures the real deltas, and SIDOU
  must boot in a radio-modelling emulator (it hangs in Optimist's today) or fm1-emu gains the
  radio model.
- Every device step (the boot with the raised rails, a Mac and an iPad connecting, an hour of two
  SIDs with Bluetooth MIDI) waits for the owner's yes for that step.
