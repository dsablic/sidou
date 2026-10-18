<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Roadmap after M2

**Status:** agreed with the user on 2026-10-18, after SIDOU's first run on a real FM-1
(`docs/hardware-results/2026-10-17-first-flash.md`) and a look at the FM-1 community's work
(FM-1 SID, the second core, Felucca 1.6.1, the FM-1 Firmware Hub). It replaces the order in
the design document's section 14 from M2 on; M3's scope is unchanged, its approach is set by
the 6502 study below.

## Order

1. **Close M2.** The remaining tasks: M2c Tasks 8-9, M2d Tasks 3-7, M2e Tasks 5-6, M2b
   Tasks 6 and 8, then the end-of-M2 review by area (storage most closely, and the tests that
   were re-aimed during M2).
2. **Win back performance headroom.** Several gates pass by very small margins: LIVE noise
   (0.6k of 155k), SELECT into RULE 30 (3 of 230), STATUS idle (about 30 of 7,500), the MIDI
   floods and the USB drain (under 1 %). A pass that measures each hot path against a base
   build and its disassembly, and makes the code cheaper, so later work has room. Bounds only
   tighten.
3. **Pre-flash safety.**
   - The vendored update loader goes to Felucca 1.6.1's, which refuses a package of the wrong
     size before anything is erased.
   - A pre-flash check script: package checksum, identity and sizes; the recovery route ready
     (the OCT- power-on gesture into ROM boot, `tools/fm1-unbrick-mac`, a verified V15 copy).
   - A one-page flash-day checklist.
4. **Flash day.** Install over V15 with the normal installer, then, in order: the OCT-
   gesture, the USB update gate (`docs/hardware-test.md` 8.1), the readability and audio
   checks, then the ergonomics recipes on the device. This becomes the routine after every
   large merge, so hardware problems surface early.
5. **The ergonomics pass.** The user reviews the harness's findings and the heuristic review
   (`host/crates/ergo`, `build/ergo/`); the accepted ones become the fix list. Knob ranges and
   curves, gestures, consistency, readability, workflow.
6. **v0.1**, led by what sets SIDOU apart: writing C64 music on the FM-1, GoatTracker 2
   compatible both ways, two authentic SIDs with the chip character. With:
   - a small group of testers who already have a recovery route (also to measure the power
     cost of the main loop's spin while USB is attached);
   - a submission to the FM-1 Firmware Hub (thefirmwarehub.com) for the release, not before.
7. **M3, leaner.** `.sid` playback as [decision record 0008](../decisions/0008-m3-6502-and-fm1-sid.md)
   sets it: SIDOU's own 6502 (about ten times fewer instructions than FM-1 SID's m6502.h in
   the study), run a frame ahead with cycle-stamped SID writes, on one core; FM-1 SID's
   BSD-2-Clause PSID environment design with its bounds bug and five other gaps fixed;
   m6502.h and FM-1 SID's player as a host-side oracle only.
8. **The resource-heavy UX:** help cards, first-run guidance, suggested automations.

## Alongside

- **FM-1 SID:** report its PSID loader's bounds bug (decision record 0008), shown to the
  user before it is posted.
- **fm1-emu:** bump the pinned revision (decoder fixes, speed); offer the USB host model built
  for SIDOU's tests and the decoder gap SIDOU found (`docs/TODO.md`) back upstream. Anything
  posted to another project is shown to the user first.
- **The GoatTracker round trip:** M2e Task 5's SysEx song dumps, then a small browser page
  (Web MIDI) to send a song between the FM-1 and a computer, as a recipe and a video.
- **Recording into a DAW now:** check SIDOU in fm1-emu's AU / VST3 / CLAP plugin and document
  it, ahead of USB audio out on the device.
- **Recipes and videos:** the core recipe set, docs pages and `llms.txt` after the ergonomics
  pass; videos from the journeys' full captures with the showcase songs.
