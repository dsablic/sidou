<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Bluetooth connecting out: design

1.11's headline: SIDOU connects to a Bluetooth MIDI keyboard (the central role of Optimist's
stack), the owner's Korg microKEY Air as the test. Agreed with the owner 2026-11-15.
[Decision 0011](../decisions/0011-bluetooth-midi.md) holds the why; this is the what.

## Decided

- **No pairing** (the owner, 2026-11-15): `SIDOU_BLE_CENTRAL` level 3. Without pairing the stack
  costs 7,408 bytes; with it 12,920, over the ceiling before a page (the flash find,
  design-notes "Flash budget and ceiling"). A keyboard that asks for pairing is shown as such, and
  that is what would make the case for pairing and SCENE's art.
- **One link at a time** (the stack's limit, as Melodee and fm1-base): while SIDOU is connected to a
  keyboard a computer cannot connect to it.
- **The mode on the existing row** (option A): SETTINGS, BLUETOOTH is **OFF / ON / KEYBOARD**.
- **Feedback by toasts and the row** (option A): no new mark on the playing screens.

## What the player sees

- **OFF** and **ON** are as in 1.9 and 1.10 (ON: a Mac or an iPad connects to SIDOU).
- **KEYBOARD**: SIDOU connects out to the remembered keyboard and is not visible to computers. The
  row reads `KEYBOARD NONE` (none picked yet), `KEYBOARD LOOKING` (picked, not connected) or
  `KEYBOARD <name>` (connected, the name cut to fit), or `NEEDS PAIRING` when the keyboard asked for
  security.
- **DEVICES**: SAVE on the row while it reads KEYBOARD opens it. It scans for about 8 seconds and
  lists up to 8 nearby devices that advertise BLE-MIDI (the stack lists only those), each with
  signal bars. SELECT or the encoder moves, SAVE connects and remembers, HOME backs out unchanged.
  An empty list reads `NO KEYBOARDS FOUND`, SAVE scans again.
- **Playing**: the keyboard is one more MIDI input. Its notes, CCs and clock reach the same parser
  and routing as USB and TRS (channels, MIDI LEARN, the clock as today).
- **Toasts**: `KEYBOARD <name>` when it connects, `KEYBOARD LOST` when it goes (held notes released,
  SIDOU looks for it again), `NEEDS PAIRING` when it refuses an unpaired link.
- The mode and the remembered keyboard are kept across power cycles; with KEYBOARD, SIDOU looks for
  it after power-on until it connects.

## How it is built

- **Radio side** in `ble_radio.c` (the peripheral's unit, `-Oz` since the flash find): the scan's
  reports into the stack's scan table, starting and cancelling a connection, and
  `ble_central_state` / `ble_central_fail` turned into events for the views (connected, lost,
  needs pairing). While the mode is KEYBOARD and no link is up it scans for the remembered address
  and connects when it hears it, for as long as it takes.
- **The setting**: store record `0x10A` gains the mode (0, 1, 2) and the remembered keyboard
  (address, its type, a name of up to 16 characters). A 1.9 or 1.10 record reads as OFF or ON with
  no keyboard.
- **The page**: `views_devices.c`, built `-Oz` like the other cold UI files; the row's third value
  in `views_settings.c`.
- **Flash**: the stack and the page in about 9.8 KB of room. `flash_budget.rs` holds the default
  image under 579,516, aiming at a few hundred bytes to spare; if the page does not fit, its signal
  bars go first. RAMTEXT is not touched.

## How it is tested

- **The emulator**: fm1-emu has no radio, so the radio side sits behind a small interface a test
  build fakes. The fake plays a keyboard that appears in a scan, connects, sends notes, goes, and
  asks for pairing. emu-gate checks the row and the page on screen and every toast, the fake's
  notes playing through the path USB takes, held notes released on LOST, looking again after a
  power cycle, and older records read right.
- **Host tests**: the scan list's order, cut names, the record's format.
- **The device (hardware-test §38)**, every step with the owner's yes: the microKEY Air found and
  connected; notes, knobs and clock; the keyboard switched off and on (LOST, then connected again);
  a power cycle reconnects; back to ON, the Mac connects; the peripheral path again, as the radio
  driver is now `-Oz`.
