<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Flash day: the checklist

One page for installing a SIDOU build on a real FM-1, in order. The full checklist is
[hardware-test.md](hardware-test.md); the first run and its recovery are in
[hardware-results/2026-10-17-first-flash.md](hardware-results/2026-10-17-first-flash.md).
Stop at the first step that fails and go to "If a step fails" below. Do not skip ahead.

## Before the install

- [ ] **Build the package** from the commit under test (BUILDING.md) or take the release's.
- [ ] **`tools/preflash build/fm1/sidou.fwsc --unbricker <MvaveFM1Unbricker folder>
  --installer <fm1_install.py>`** prints only PASS (a SKIP for something you will not use is
  fine). It reads files only and never touches a device. It checks the package's SHA-256,
  identity, app and loader sizes, the `FELUCCA-LOADER-1` marker and CRCs, that the OCT- gesture
  is documented, `tools/fm1-unbrick-mac`, jl-uboot-tool at the pinned commit with a valid wl82
  loader, the V15 `FM-1.fwsc` (SHA-256 `db1642b2...edb8a`) and the installer. It ends with the
  exact commands to run. Write the package SHA-256 into hardware-test.md's "Unit and build".
- [ ] **The way back is ready**: V15 `FM-1.fwsc` and the unbricker on this computer, a USB
  cable that carries data, nothing else MIDI plugged in.
- [ ] **The build has the OCT- gesture** (it is not in v0.2.0-m2-preview). Without it, have a
  Flipper Zero with flipper-fm1-recovery or the FM-1 Transporter at hand
  ([hardware-test.md, section 0](hardware-test.md#0-before-you-start-recovery)).
- [ ] `fm1_install.py --info` (Felucca 1.6.1's own, `tools/fm1_install.py`) shows the firmware
  now on the unit (note it), then `fm1_install.py --backup .` saves its user data.
- [ ] **If the unit runs M-VAVE's FM1 Open** (handshake identity `FM-1_8000013`, USB port "FM1
  Community"; the installer needs `--port "FM1 Community"` and treats it as unknown firmware):
  go to V15 first, never straight to SIDOU. After step 1, FM1 Open's loader answers
  `ota-FM1C_...`, and the installer's model check refuses it for V15, though M-VAVE's own
  updater accepts any `ota-` loader for a stock package. Use
  `tools/fm1_install_v15_over_open.py <Felucca tools dir> FM-1.fwsc --port "FM1 Community"`
  (the installer's own checks, plus that one rule for the V15 package only), then continue.
- [ ] After a firmware switch on a Mac: delete the old USB-MIDI device in Audio MIDI Setup.

## The install

SIDOU's USB-MIDI out is still dead, so a SIDOU unit cannot be updated through the installer
yet ([second flash](hardware-results/2026-10-18-second-flash.md)). Two routes:

- **From V15 or a Felucca-family firmware** (the first SIDOU on a unit): run the commands
  `tools/preflash` printed, one at a time. Step 1 reports no error, the unit restarts as
  "Felucca Update", step 2 reaches 100 %. The final `--info` times out, and that is the
  known state; what counts is the unit coming back as USB 1209:0001 "SIDOU" and running.
- **From a SIDOU build that has the OCT- gesture** (every later build): put the unit into ROM
  boot with OCT- (below), then `sudo <venv>/bin/python fm1_unbrick_mac.py restore sidou.fwsc`
  from `tools/fm1-unbrick-mac`. It writes the firmware region (0x4000-0x92FFF) after a
  verified double backup and reads it back; a power cycle starts the new build.

## Then, in order

1. **The OCT- gesture** ([section 1](hardware-test.md#1-install), "Right after the first
   install"; it passed on hardware on 2026-10-18). Switch off, hold OCT- alone, switch on, keep holding about a second: black
   screen, `WL82 UBOOT1.00` (USB 4C4A:8057) on the computer. Stays five minutes; a power cycle
   without the key boots SIDOU. A brief OCT- tap, OCT- with OCT+, and OCT- with a note key boot
   normally. **This must pass before anything else**, because it is what makes every later
   failure recoverable.
2. **The USB update gate** ([section 8.1](hardware-test.md#81-before-any-release-usb-midi-out-must-pass)):
   `--info` answers, a reinstall over SIDOU completes, K1 sends CC 74 and K4 program changes,
   again after deleting the device in the OS's MIDI setup, each cold and after a replug.
3. **Readability**: the screens at arm's length: LIVE, TRACKER, CHIP, menus and popups, in
   both themes ([section 6](hardware-test.md#6-lcd)). Note what cannot be read.
4. **The audio checks**: the note keys and their LEDs, MASTER (a playing song must follow
   it), the knob ranges, the quiet floor after a release, no clicks while the screen redraws
   ([sections 4 to 6](hardware-test.md#4-note-keys-and-their-leds)).
5. **The recipes**: `docs/recipes/*.json` on the device, as the ergonomics harness records them.
   Note every place where the device and the harness disagree.

Record each result in a copy of hardware-test.md, and the findings in a new page under
`docs/hardware-results/`.

## If a step fails

- **Step 1 of the install errors, or the install stops before the unit restarts**: nothing is
  erased yet. Unplug, plug, run `--info`, retry once. A refused package ("FAILED -15", or
  -14 in the loader) means the wrong package: run `tools/preflash` on it.
- **The unit sits in "Felucca Update"** (power loss or a cable pull during step 2): plug it
  back and run the same install again; it resumes.
- **The OCT- gesture fails**: stop. Recover with the fallback in section 0 (a Flipper Zero or
  the Transporter into ROM boot), then `tools/fm1-unbrick-mac` (Mac) or MvaveFM1Unbricker
  restores V15. Record what you saw, and do not continue.
- **The unit is in `WL82 UBOOT1.00`** (any reason): `sudo <venv>/bin/python fm1_unbrick_mac.py
  info` reads only; `... restore FM-1.fwsc` writes V15 after two verified backups. If a write
  or verify fails, do not power off: run `restore` again
  ([tools/fm1-unbrick-mac/README.md](../tools/fm1-unbrick-mac/README.md)).
- **The USB gate, readability, audio or a recipe fails**: SIDOU is running and recoverable.
  Record it in the results page, and either stay on the build to look closer or go back with
  `fm1_install.py FM-1.fwsc` (V15, [section 9](hardware-test.md#9-reinstall-back-to-stock-v15-and-sidou-again))
  or the OCT- gesture. A failure in step 2 blocks any release.
- **Nothing responds at all**: it is the ROM's job, not SIDOU's. Use the Flipper or Transporter
  route to ROM boot; the chip's flash head is never written by SIDOU's path.
