<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Roadmap after M2

**Status:** agreed with the user on 2026-10-18, after SIDOU's first run on a real FM-1
(`docs/hardware-results/2026-10-17-first-flash.md`) and a look at the FM-1 community's work
(FM-1 SID, the second core, Felucca 1.6.1, the FM-1 Firmware Hub). It replaces the order in
the design document's section 14 from M2 on; M3's scope is unchanged, its approach is set by
the 6502 study below.

## Order

0. **USB-MIDI out on the device, first** (top priority since the second flash,
   `docs/hardware-results/2026-10-18-second-flash.md`). Nobody else installs SIDOU until, on
   our unit, SIDOU answers the installer's identity query, updates itself over USB, and
   returns to V15 over USB. Also an escape hatch that doesn't depend on SIDOU's own USB
   stack: a power-on gesture into the Felucca update loader, which an ordinary installer
   can then write through.
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
6. **v0.1**, which ships with **no known bug**: everything spotted by then is fixed first,
   from the ergonomics review (`docs/plans/2026-10-18-ergonomics-pass.md`, including its
   deferred TRACKER and CHIP items), flash day, the testers and `docs/TODO.md`'s bug entries.
   The harness is re-run on the release candidate, and every finding it reports is fixed or
   shown to be a false positive by a harness fix. It is led by what sets SIDOU apart: writing C64 music on the FM-1, GoatTracker 2
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
9. **Generative play (after v0.1).** Patterns that change while they play, as the newest FM-1
   firmwares do (checked 2026-10-18: JIANT FM's generate / mutate / dice, Felucca's RANDOM,
   COOK and per-step chance, Felucca_seq_per_track's generative arps), aimed at C64 music:
   - **STEP and TRACKER:** per-step chance, MUTATE (changes the pattern a little on each
     press), and random notes in a scale on a fixed rhythm;
   - **C64 idioms, generated:** arpeggio chords, drums multiplexed into a voice, filter
     sweeps (the tricks of the showcase songs, made playable);
   - **pattern DICE,** next to the ergonomics pass's patch DICE.
   Everything stays plain GoatTracker 2 data, so a generated song opens in GoatTracker. Its
   own plan when we get there; first candidate: per-step chance plus MUTATE in STEP.

## Alongside

- **Reports for other projects** are collected in `docs/upstream-reports.md` (FM-1 SID's
  PSID loader bug, fm1-emu's decoder gaps) and filed later, after the user reviews them.
- **fm1-emu:** bump the pinned revision (decoder fixes, speed); offer the USB host model built
  for SIDOU's tests and the decoder gap SIDOU found (`docs/TODO.md`) back upstream. Anything
  posted to another project is shown to the user first.
- **FM-1 Live Link, a companion for the whole FM-1 community** (after USB-MIDI out works on
  the device, before v0.1). A browser page (Web MIDI: Chrome and Edge on desktop, Chrome on
  Android; iOS needs a native app later) that shows the device on a large screen and
  controls it, built as a module any FM-1 firmware can adopt:
  - **Protocol:** SysEx over the normal USB-MIDI port, no special USB mode, with an open
    spec. Two display modes: **cells** (changed character + colour cells, plus a one-time
    font and palette upload, a few hundred bytes a frame; SIDOU's 30x30 grid) and
    **pixels** (changed rectangles, run-length coded RGB565, for firmwares that draw pixels).
    Plus **state** streams a firmware may offer: SID or synth register writes (SIDOU: about
    25 bytes a frame per chip), transport and the current view. Opt-in on the device, with a
    rate cap. Its cost is measured against the main-loop and USB gates and stays within them.
  - **Device side:** a small C module, MIT-licensed so GPL-3.0-only firmwares (FM1 Open,
    Felucca's family) can take it as easily as SIDOU (GPL-3.0-or-later). It sends what the
    firmware already draws, changed cells or rectangles; nothing new is rendered for it.
  - **Viewer:** the mirror drawn with the device's own font and palette at any size; for
    SIDOU, live per-voice scopes and the filter curve from the streamed register writes run
    through SIDOU's own SID core in WASM (`tools/web-demo`), and all three TRACKER voices
    side by side. Remote control both ways through the MIDI the panel already sends.
  - **One companion page:** the mirror, the scopes, remote control and the GoatTracker song
    transfer below. A PWA, so it installs and works offline once loaded.
  - **Works with most devices, nothing to install:**
    - **Host** (the device the FM-1 is plugged into): Chrome or Edge on Windows, macOS or
      Linux; Firefox after its Web MIDI permission prompt; Chrome on an Android phone or
      tablet over a USB-C cable.
    - **Watchers on any screen:** a Share button on the host page shows a QR code and a
      link; any browser that opens it (iPhone Safari, tablets, smart-TV browsers,
      projectors) gets the live mirror and scopes, relayed peer-to-peer from the host over
      WebRTC. It needs only a small signalling service to connect the browsers (a free
      serverless function or an existing public one; the stream itself never passes
      through it). View only by default; remote control only when the host allows it.
    - **The big TV:** cast the host page with what the OS already has (AirPlay from a Mac,
      Chrome's "Cast tab", Miracast from Windows), or open the share link in the TV's
      browser. AirPlay from the FM-1 itself is out: it has no network, AirPlay needs Apple's
      certification, and the chip can't encode video.
    - **The tray app** (macOS menu bar, Windows tray, Linux), the optional "just works"
      host: Tauri (Rust, like SIDOU's host tools) around the same viewer code as the page.
      It notices the FM-1 when it is plugged in and starts Live Link, serves the viewer on
      the local network (QR link; no signalling service, works offline at a gig), and
      offers the mirror to OBS and video calls. Later the companion for song transfer,
      backups and firmware updates (behind the same safety checks and an explicit confirm).
      Installed on macOS from our own Homebrew tap as a **formula** (`brew install
      dsablic/sidou/fm1-live-link`, built from source or a prebuilt binary, started at
      login with `brew services`): formulae aren't quarantined, so it runs unsigned without
      Gatekeeper warnings. Not a cask: Homebrew 5 ended support for casks that fail
      Gatekeeper on 2026-09-01. A signed download or App Store build only if there is
      demand (an Apple developer account, the user's call).
    - **Later: an iPhone and iPad host app** (a web view with a CoreMIDI bridge), since
      Safari has no Web MIDI. Needs an Apple developer account; the user's call when we get
      there.
  - **Prior art:** the Dirtywave M8's headless mode with m8c (a screen stream over USB
    serial); nothing like it exists for the FM-1 (checked 2026-10-18).
  - **Offered to the other FM-1 firmwares** once SIDOU ships it, through
    `docs/upstream-reports.md`'s offers, shown to the user first.
- **The GoatTracker round trip:** M2e Task 5's SysEx song dumps, then a small browser page
  (Web MIDI) to send a song between the FM-1 and a computer, as a recipe and a video.
- **Recording into a DAW now:** check SIDOU in fm1-emu's AU / VST3 / CLAP plugin and document
  it, ahead of USB audio out on the device.
- **Recipes and videos:** the core recipe set, docs pages and `llms.txt` after the ergonomics
  pass; videos from the journeys' full captures with the showcase songs.
