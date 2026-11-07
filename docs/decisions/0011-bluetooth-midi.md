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
GPL-3.0-only image. Estimated for SIDOU, as a peripheral only: about 29-33 KB of flash, 6.5-8 KB of RAM (1.5 KB of it a
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
5. **The first Bluetooth release connects both ways** (the owner, 2026-11-04): a computer or a
   tablet connects to SIDOU, and SIDOU connects out to a Bluetooth MIDI keyboard or controller
   (the central role, Optimist's `BLE_CENTRAL`, about 12-20 KB of flash and 1.5-2.2 KB of RAM more,
   and a devices page). **No pairing at first** (macOS and iOS connect without it; no AES in the
   interrupt); a keyboard that insists on pairing is the first reason to add it (about 3.3-3.8 KB).
6. **The stack is vendored from fm1-base's `bt/`** at the commit whose felucca-bsp pin SIDOU uses.
   Whether it could live in felucca-bsp itself goes in `docs/upstream-reports.md` for the next
   batch.
7. **Bluetooth audio (A2DP) is dropped** for SIDOU (about 37 KB of flash and 21 KB of RAM). **Wi-Fi
   and Ableton Link are parked** until Optimist has a Link session working on a real FM-1.

## The owner's decisions on the measured plan (2026-11-05)

From [the 1.8 Bluetooth plan](../plans/2026-11-05-bluetooth-1.8-plan.md), which measured both ways
at 39,312 bytes of flash against 15,616 free. These revise points 2 and 3 above where they differ.

1. **Flash: the reserve is released.** The 13,564 bytes between the 568,000-byte image ceiling
   and the app slot (581,564, fixed by the loader) go to the felucca-bsp move and Bluetooth, but
   at least 2,048 bytes stay under the slot. The new ceiling is set from what the move and
   Bluetooth need ([the design notes](../design-notes.md#flash-budget-and-ceiling)).
2. **USB audio at 44.1 kHz only** (`FELUCCA_UAC_48K 0`, a setting of the vendored `usb.c`): about
   8,256 bytes. Every doc and page that offers 48 kHz says 44.1 kHz.
3. **Both ways in 1.8:** SIDOU as a peripheral, and connecting out (central) to BLE MIDI
   keyboards, the owner's Korg microKEY Air as the test. The last 1.2-2.7 KB come from the plan's
   options: the radio driver at plain `-Os` (an upstream change; its pull request text prepared in
   [upstream-reports.md](../upstream-reports.md), nothing filed), a hot/cold file split, or
   `-Os`/`-Oz` on cold code. No pairing at first.
4. **RAM:** `midi.c`'s MIDI input ring (3,088 bytes, the CPU's only) moves to NOINIT, so the RAM
   region keeps its 5,120-byte floor (`REGION_FREE_MIN`, unchanged).
5. **The update loader keeps Felucca 1.6.1's HAL**, so its pinned SHA-256 is unchanged; the app
   moves to felucca-bsp.
   Done in 1.8's step 2: the ceiling is 579,516 (the slot less 2,048), 44.1 kHz only saved
   8,256 bytes, six cold files at `-Os`/`-Oz` 1,440 (four more failed their gates), and the ring left the RAM region for NOINIT
   (the region 10,528 free).
6. **The radio is tested on the device.** fm1-emu is asked for a radio model in
   [upstream-reports.md](../upstream-reports.md); nothing is filed.

## The owner's decision on the measured fit (2026-11-06)

Measured on the felucca-bsp move with step 2's room: both ways came to 585,232 bytes before its
views, 5,716 over the 579,516 ceiling (the move cost 4,128 bytes, not the 784 the plan assumed,
and four of the ten cold-file changes failed their gates), so about 8-10 KB short with the
DEVICES page. The owner chose **the peripheral first: 1.8 ships Bluetooth MIDI as a peripheral
only** (SIDOU seen by a Mac, an iPad or a phone), and point 5's connecting out moves to the next
release, which finds its room with the radio driver at plain `-Os` (felucca-bsp#31) and hot/cold
splits of the gate-bound files. The central role's code is not built into 1.8's image
(`SIDOU_BLE_CENTRAL` 0), so nothing of it is half-shipped. 1.8's device test is SIDOU in a Mac's
Audio MIDI Setup, notes both ways, no audio glitch while connected, and a firmware install still
working.

## The owner's decision on the test build (2026-11-07)

The first device test reset the FM-1 at BLUETOOTH ON; the fix (the stock power setup before the
radio, as fm1-base's boot does it) put the Bluetooth image at 580,016 bytes, 500 over 1.8's
579,516. The owner allowed **the `SIDOU_BLE` test build up to 500 bytes of the slot's 2 KB
reserve, for the device test only**: its ceiling is 580,016. The default build's stays 579,516, and
the Bluetooth build must fit 579,516 again before Bluetooth ships in a release.

## Consequences

- The plan's steps 2 and 3 come first: a `SIDOU_BLE` build measures the real deltas, and SIDOU
  must boot in a radio-modelling emulator (it hangs in Optimist's today) or fm1-emu gains the
  radio model.
- Every device step (the boot with the raised rails, a Mac and an iPad connecting, SIDOU
  connecting out to the owner's Korg microKEY Air, which also shows whether a keyboard insists on
  pairing, an hour of two SIDs with Bluetooth MIDI) waits for the owner's yes for that step.
