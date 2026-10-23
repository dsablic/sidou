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
1b. **Faster tests, so iterations are faster** (the user, 2026-10-21), alongside M2's close
   and before the headroom pass. The emulator gate is the long pole: it runs every test one
   at a time (`--test-threads=1`, a default from M0 that no test is known to need; fm1-emu
   counts instructions, so parallel runs should not change a number), and `screen.rs` alone
   takes about 10 minutes on a quiet machine. In order: time every test file; show that the
   gate's numbers are the same in parallel (each test in its own process, e.g. nextest), then
   run it in parallel locally and in CI; run the merge check's independent stages side by
   side; the fm1-emu bump's JIT (item 2). A test that truly needs to be alone says why.
   **Done 2026-10-22** (branch `fasttests`; the machine was loaded by other work, so read the
   times as ratios). Serial emu-gate, per file in seconds: screen 1966, song 1053, latency 432,
   store 137, sysex 107, character 113, song_store 110, midi 79, song_bank 82, midi_transport 66,
   settings 56, user_profile 53, the other 14 files under 31 each; 4480 in all. Run with
   cargo-nextest (each test its own process, ten at once) it takes 845 to 1251 s over four
   runs, and 1035 s inside the merge check beside the other stages: the wall time is now the
   one test `the_players_worst_block_fits_its_budget` (760 to 845 s alone; `host/.config/nextest.toml`
   starts it first). Every JSON report (22 files) was identical to the serial run's in three
   parallel runs, and in the fourth (the merge check) all but the spelling of one path in
   `sidou_package.json`, which follows how SIDOU_FM1_BUILD is written; all 235 tests passed each
   time and no field is a wall-clock one. Shared
   state found: none that mattered. Every report has its own name, the emulator has no globals
   (only trace environment variables nobody sets), the one in-process cache is behind a
   mutex; the one shared file name, `<name>.actual.png` of the two song-page tests, is now
   written whole and renamed. The merge check (`merge-check.new.sh`) runs the two firmware
   builds, fmt/clippy, the release tests, UBSan (its own target directory), pack and REUSE side by side:
   19 minutes for the whole check. Left: splitting that one 13-minute test (its closing
   bounds combine maxima across all songs, so a split must keep them together), and CI.
2. **Win back performance headroom.** It starts with the **fm1-emu bump** (added
   2026-10-21): the pass measures in the emulator, so on the revision SIDOU keeps. Upstream
   has since fixed the and-not decoding and modelled P33 (`docs/upstream-reports.md`, items
   3 and 5), fuzzed its decoder against the vendor objdump, and runs its JIT on x86-64
   Linux, which speeds up CI's emulator tests. Goldens it moves are viewed, then re-blessed
   once. Several gates pass by very small margins: LIVE noise
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
6. **v0.3.0, the first public release** (numbered after v0.2.0, the M2 release; the user, 2026-10-21), which ships with **no known bug**: everything spotted by then is fixed first,
   from the ergonomics review (`docs/plans/2026-10-18-ergonomics-pass.md`, including its
   deferred TRACKER and CHIP items), flash day, the testers and `docs/TODO.md`'s bug entries.
   The harness is re-run on the release candidate, and every finding it reports is fixed or
   shown to be a false positive by a harness fix. Added 2026-10-21, from the community's
   work:
   - **A long-run clock-drift test:** at least ten minutes against a steady external MIDI
     clock in fm1-emu, the song still on the beat at the end (Felucca's issue #269 is this
     bug in another sequencer; SIDOU's tests cover tempo changes, not accumulated drift).
     **Done 2026-10-22:** emu-gate's `midi_transport.rs` follows a steady TRS clock for ten
     minutes at 120 and at 133 BPM (fm1-core's JIT, about 2 to 4 minutes of wall time each):
     every row stays within 1 ms of where rows sat in the first minute (the last minute at
     most 0.54 and 0.60 ms off), and the song's row matches the clock's.
   - **Positioning:** SID voices are appearing as one engine among many (Melodee's SID
     engine, Mini-Tone's), so the release notes and the Hub listing lead with what only SIDOU
     does: GoatTracker 2 compatibility both ways and two SIDs with the chip character. It is led by what sets SIDOU apart: writing C64 music on the FM-1, GoatTracker 2
   compatible both ways, two authentic SIDs with the chip character. With:
   - no separate tester round (the user, 2026-10-23): the OCT- recovery route, the USB update
     path and the way back to V15 are proven on hardware, so the release goes out for people to
     experiment with; the power cost of the main loop's spin while USB is attached is measured
     when someone can;
   - SIDOU's own identity **`FM-1_964`** (a nod to the C64): there is no registry; of the 80
     packages in the Firmware Hub's catalogue on 2026-10-23, 17 ship `FM-1_900` and the rest
     picked a number (several collide: 950, 910, 955, 985, 700); none uses 964;
   - a submission to the FM-1 Firmware Hub (thefirmwarehub.com) for the release, not before.
6a. **v0.3.2: SETTINGS → RESET and the button LEDs** (the user, 2026-10-23). v0.3.1 shipped the
   first-boot fix alone. RESET is in [TODO.md](../TODO.md). The LEDs show what is latched and what
   moves in time, using the hardware's dim glow as well as full light: dim = available or set,
   full = active now. Today they show held keys, the octave (OCT-/OCT+), FX for the 8580, EDIT
   for an effect slot in use, PLAY while playing and GLO while latched.
   - **ARP:** lit while ARP is on, flashing on each arpeggio step; at the fastest rates (1-2
     frames) once per cycle, as faster reads as flicker.
   - **PLAY:** full on each beat, dim between, while the song plays.
   - **REC:** lit while recording, blinking through the count-in.
   - **GLO:** lit while latched; with GLO held or latched, note keys 1-6 show the voices (lit
     sounding, dark muted, blinking solo).
   - **STEP:** the 16 keys show the page (lit a set step, dim an empty one) and the playhead
     moving along them, in place of the held-key lights in that view.
   - **SAVE:** blinks while a question waits for SAVE to confirm.
   - Host tests for every state (`panel_get_leds` grows the inputs it needs); the flash hold's
     dark LEDs stay as they are; controls.md says what each LED means.
7. **M3, leaner.** `.sid` playback as [decision record 0008](../decisions/0008-m3-6502-and-fm1-sid.md)
   sets it: SIDOU's own 6502 (about ten times fewer instructions than FM-1 SID's m6502.h in
   the study), run a frame ahead with cycle-stamped SID writes, on one core; FM-1 SID's
   BSD-2-Clause PSID environment design with its bounds bug and five other gaps fixed;
   m6502.h and FM-1 SID's player as a host-side oracle only.
8. **The resource-heavy UX:** help cards, first-run guidance, suggested automations.
9. **Generative play (after v0.3.0).** Patterns that change while they play, as the newest FM-1
   firmwares do (checked 2026-10-18: JIANT FM's generate / mutate / dice, Felucca's RANDOM,
   COOK and per-step chance, Felucca_seq_per_track's generative arps), aimed at C64 music:
   - **STEP and TRACKER:** per-step chance, MUTATE (changes the pattern a little on each
     press), and random notes in a scale on a fixed rhythm;
   - **C64 idioms, generated:** arpeggio chords, drums multiplexed into a voice, filter
     sweeps (the tricks of the showcase songs, made playable);
   - **pattern DICE,** next to the ergonomics pass's patch DICE.
   Everything stays plain GoatTracker 2 data, so a generated song opens in GoatTracker. Its
   own plan when we get there; first candidate: per-step chance plus MUTATE in STEP.
10. **BLE MIDI, a user-built variant (after v0.3.0).** The user's decision (2026-10-18), after the
   licence audit of the app image: published SIDOU links only code whose owners allow it,
   and about a third of the image is others' copyleft (Felucca's HAL, USB and update code,
   GPL-3.0-only; the reSIDfp-derived filter and waveform tables and DAC, GPL-2.0-or-later;
   the GoatTracker-derived player and format code). Linking M-VAVE's closed BLE library
   (`libfm1_engine.a`, from FM1 Open) into a *published* image would need all of them to
   agree. So, as design doc section 17 planned:
   - **Source only:** SIDOU publishes the BLE integration and an opt-in build setting
     (`SIDOU_WIRELESS=ble`). The user's build fetches M-VAVE's SDK library on their own
     machine and links it there; GPL allows a private combination that isn't distributed.
   - **Never published:** CI never builds or ships the variant; no release artefact holds
     the vendor library.
   - **Our own BLE stack is not an option:** the radio's controller is undocumented and
     exists only inside the closed library, so an open host stack would still link it.
   - **Engineering:** the spike as section 17 describes, with FM1 Open's BLE MIDI path as
     the working reference: RAM, flash, the SDK's runtime next to SIDOU's bare-metal loop,
     radio interrupts against the audio deadline. BLE MIDI feeds the same timestamped MIDI
     path as USB and TRS. Wi-Fi stays out (never certified).
   - **Later, if wanted:** the owners' permissions could still make it a published feature;
     not pursued now.
## From the other SID firmwares (2026-10-23, candidates)

Checked against SID-1, Melodee, Mini-Tone and FM-1 SID's own READMEs; each is SIDOU's own design,
not a copy:

- **Chord arpeggios on LIVE** (Melodee's held-chord chip arpeggios): hold a chord, one voice steps
  its notes at frame rate, speed and order on the panel. The C64's signature sound, playable.
  **In v0.3.0** (the user, 2026-10-23).
- **A SID drum kit and digi drums** (Melodee's SID KIT and SID DIGI, SID-1's sample kit): drums on
  the chip's own voices for STEP's lanes, and short samples played through the 6581's volume
  register as C64 tunes do. v0.3.x or with M3.
- **A master soft limiter** (Mini-Tone): several effects at full feedback can still clip the
  output; a limiter after MASTER makes "no setting clips" true everywhere. **In v0.3.0** (the user,
  2026-10-23), with CHIP SPREAD in the same output-stage track. **Done** (2026-10-23): where the
  output clamped, before MASTER ([decision record 0004](../decisions/0004-effect-algorithms.md),
  "The output's soft limiter").
- **USB audio out** (Melodee, Felucca's `FELUCCA_UAC`; TODO.md): moved up, right after v0.3.0.
- **Instruments from a `.sid` file** (SID-1's web page): with M3's 6502, run a tune and pull out its
  instruments as GoatTracker instruments. With or after M3.
- **A song as MIDI** (SID-1): a `.sng` exported as a `.mid`, a channel a voice. **Done** as the
  site's Songs to MIDI page (`web/midi.html`, `web/songmidi.js`; CHANGELOG, Unreleased): the notes
  checked call by call against GoatTracker's packed player. Still to do: the same export on the
  device.
- **A web editor** for instruments and settings (SID-1, Mini-Tone): Live Link's second page.
- **Layer mode** (SID-1): patches stacked on one note, beside the live, multitimbral, whole-chip and
  omni modes. Later.
- Not taken: 2x oversampling (no CPU to spare), an idle LED glow (cosmetic; the ergonomics list).

## Alongside

- **Reports for other projects** are collected in `docs/upstream-reports.md` (FM-1 SID's
  PSID loader bug, fm1-emu's decoder gaps) and filed later, after the user reviews them.
- **fm1-emu:** the pin bump moved to item 2 (2026-10-21); offer the USB host model built
  for SIDOU's tests and the decoder gap SIDOU found (`docs/TODO.md`) back upstream. Anything
  posted to another project is shown to the user first.
- **FM-1 Live Link, a companion for the whole FM-1 community** (after USB-MIDI out works on
  the device, before v0.3.0). A browser page (Web MIDI: Chrome and Edge on desktop, Chrome on
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
