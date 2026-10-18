<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Second flash: main a123730 on a real FM-1 (2026-10-18)

The second SIDOU build on hardware: main at `a123730`, with the USB-out fix (`dfa3415`), the
OCT- power-on gesture, MASTER as a digital gain (`af3cc6f`), the tail gate (`31bf4c6`) and
the readable screens (`64b6f97`).

## Unit and builds

| | |
|---|---|
| Unit | the same FM-1 as [the first flash](2026-10-17-first-flash.md) |
| Firmware before | **FM1 Open** (M-VAVE's open firmware, <https://github.com/m-vave/fm1_open>): USB 3654:7756 "FM1 Community", handshake identity `FM-1_8000013`, no INFO answer |
| SIDOU package | `build/fm1/sidou.fwsc` from `a123730`, SHA-256 `58f2eee4894c3f0adf4334a47ab8297d327a72fd78f0b419637ab8065ec451fa`; `tools/preflash` (branch `preflash-safety`): 13 PASS |
| Installer | ChoralRoot's `tools/fm1_install.py` |
| Firmware after | SIDOU (USB 1209:0001 "SIDOU") |

## The install

1. **Over FM1 Open, via V15.** ChoralRoot's installer only recognises the port with
   `--port "FM1 Community"`, and classifies `FM-1_8000013` as an unknown firmware it does not
   install over (its `docs/INSTALL-COMPAT.md`: installs over unknown firmware have left FM-1s
   that do not start). The way out is official V15, which the installer never refuses.
2. **V15's step 1 put the unit into FM1 Open's update mode.** It answered as
   `ota-FM1C_0000000000000000` (USB 4D4A:4155 "USB Composite Device"), and the installer
   stopped: "the device is in the update mode of ota-FM1C_…, the package is for FM-1_015:
   nothing was written". Its model check (`same_keyboard`) has no override.
3. **M-VAVE's own updater allows this case.** FM1 Open's `sdk/WebUpdate.js` (at `b178ac4`)
   uses the same two-step protocol (`F0 22 24 35 7F F7`, then the loader at 0xE0000000 and
   the image at 0xF0000000). For a stock package it accepts any `ota-` loader identity after
   step 1. A wrapper around the installer
   (`v15_over_open.py`: `same_keyboard` true only for package `FM-1_015` and identity
   `ota-FM1C_…`, everything else unchanged) wrote V15: "done: the FM-1 runs FM-1_015".
4. **SIDOU over V15:** the write reached 100 %, then the installer timed out waiting for the
   identity reply (as on the first flash). On USB the unit came back as "SIDOU": it was
   running.

## Results

| Check | Result |
|---|---|
| Boot, title card, LIVE, sound | **pass** ("looks normal") |
| The faint tail after notes (first flash, finding 2) | **fixed**: no faint sound |
| A playing song follows MASTER (finding 3) | **fixed** |
| Readability (big popups, 2x menu, toasts, the redrawn views) | **much better** (user) |
| USB-MIDI **output** (finding 1) | **still dead.** The identity query got no reply (installer `--info`, and 3 raw handshakes `F0 00 32 45 00 00 00 40 7F F7`), and a 120 s listen while K1 was turned and K4 pressed received **0** messages. `dfa3415` (no idle while configured, USB poll out of ALNK0, IN packets ≤16 bytes) did not fix it. |
| OCT- held at power-on → ROM boot | **pass**: about 7 s after SIDOU left USB, "WL80UBOOT1.00" (4C4A:8057) appeared. Recovery without the Flipper works. |

## Consequences

- **SIDOU still cannot be updated over USB-MIDI.** Updates and diagnostic builds go through
  the OCT- gesture and `tools/fm1-unbrick-mac` (`sudo … fm1_unbrick_mac.py restore PKG`). It
  writes the firmware region (0x4000-0x92FFF) of any recognised FM-1 package after a
  verified double backup, and reads it back.
- **The flash-day checklist needs an FM1 Open case:** install V15 first, through M-VAVE's
  rule above. Added to `docs/upstream-reports.md` for ChoralRoot's installer.
