<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Bluetooth connecting out: the plan

**Status:** plan, 2026-11-15, branch `connect-out` from `main` `45a79563`. It builds
[the design](2026-11-15-bluetooth-connect-out-design.md) agreed with the owner (OFF / ON /
KEYBOARD, DEVICES, toasts, no pairing). Every step is test first; the device steps wait for the
owner's yes ([hardware-test §38](../hardware-test.md)).

## Ground rules

- Optimist's stack (`third_party/fm1-base-ble`) stays byte for byte; SIDOU hooks around it.
- The default image stays under 579,516 (`flash_budget.rs`); room before: 9,772 bytes
  (569,744). The central at level 3 measured 7,408. RAMTEXT does not move; new state that the
  radio's DMA does not read goes in NOINIT.
- With BLUETOOTH OFF the main loop still never polls the radio (`ble.rs`).

## Tasks

1. **The level in the default build.** `SIDOU_BLE_CENTRAL` stops being a cache option: the
   default and only build is level 3 (no pairing; level 1 does not fit). sidou-pack's
   `ABSENT_WHEN_OFF` loses it; its test names another option. Measure the image.
2. **The record (host test first).** `platform/fm1/ble_keyboard.c`, plain C: store record
   `0x10A` version 2 (version, mode 0/1/2, SIDOU's address, the keyboard's address, its type, a
   name of up to 16 characters) written and read; a version 1 record (1.9, 1.10) reads as OFF or
   ON with no keyboard; anything else reads as unknown. sidou-sys builds it and the vendored
   `ble_scan.c` (its own flags, no warnings as errors). Host tests: round trip, the 1.9 record,
   a short or newer record.
3. **The scan list (host test first).** `ble_keyboard_rows`: the stack's table to the DEVICES
   rows (name, signal bars, the address it connects to), in the order the stack lists them; a
   device with no name shows its address. Host tests feed real advertising PDUs through
   `ble_scan_add`: only BLE-MIDI advertisers listed, first heard first, a scan response's name
   over none, a long name cut at 16, a nameless one by address.
4. **The fake radio (test build).** `-DSIDOU_BLE_FAKE=ON` (never released, recorded in the
   option record when on, warns) compiles `ble_radio.c` with `ble_radio_fake.c` in place of the
   driver and the link layer: the stack's calls the glue makes (`ble_init`, `ble_enable`,
   `ble_ll_scan`, `ble_ll_scan_take`, `ble_central_connect` / `_cancel` / `_state` / `_fail`,
   `ble_connected`, `ble_midi_ready`) answered from symbols emu-gate writes, and the glue's
   callbacks (`ble_app_state`, `ble_app_midi_in`) called as the stack calls them; notes go
   through the vendored BLE-MIDI decoder (`ble_midi_dec`) as `ble_att.c` does. The scan's
   reports go through the real `ble_scan_add`. CI and `tools/merge-check` build it as
   `build/fm1-blefake` (`SIDOU_FM1_BLE_FAKE_BUILD`).
5. **The radio side** in `ble_radio.c`: the mode (OFF / ON / KEYBOARD) followed with one
   sequence (the central left, the scan wish set, the radio off and on: a computer's link drops
   when KEYBOARD comes); KEYBOARD never advertises (the scan wish is always on); looking for the
   remembered keyboard is initiating to its address, which waits for its advertisement;
   `ble_central_state` / `_fail` watched for connected, lost (only after a link was ready, and
   not when SIDOU left it) and needs pairing (no looking again until the player picks or changes
   the mode); the DEVICES list is fed while the page reads it (browsing ends when it stops
   reading), the central left while browsing.
6. **The views.** `views_radio` gains mode, keyboard state, the list and the pick. The
   BLUETOOTH row: OFF / ON / KEYBOARD, its states, the popup. `views_devices.c` at `-Oz`: the
   list with signal bars, SELECT or ALGORITHM moves, SAVE connects and remembers, HOME backs
   out, `SCANNING` then `NO KEYBOARDS FOUND` (SAVE scans again). The toasts come from the radio
   through `ui.c`.
7. **emu-gate, on the fake build** (`ble_keyboard.rs`): the row's states and the popup; DEVICES
   lists the fake's keyboards with bars, HOME leaves it unchanged, SAVE connects (toast and row);
   the keyboard's notes play and a CC moves its parameter; the keyboard gone releases held notes
   (`KEYBOARD LOST`) and it comes back by itself; a keyboard asking for security shows
   `NEEDS PAIRING` and is not tried again; KEYBOARD drops a computer's link and never
   advertises; a power cycle looks for the kept keyboard; a 1.10 record reads as ON; the empty
   list. On the default build: the image under the ceiling, NOINIT placement, OFF still leaves
   the main loop alone.
8. **Docs.** controls.md (the row, DEVICES, the toasts), the controllers manual, store-format's
   record, design notes (Bluetooth), BUILDING (the level, the fake build), TESTING, CHANGELOG
   [Unreleased], hardware-test §38. README, the demo page, the cheat sheet and fmhub.json wait
   for §38 to pass, as the player docs only name what passed on the device.
9. **Gate.** Targeted tests locally; push; CI's `check` green on the last commit.
