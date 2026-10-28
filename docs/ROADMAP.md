<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU roadmap

Where SIDOU is going, in the order we mean to do it. Each line says what a player gets; the
why lives in [decision records](decisions/) and the how in [plans](plans/). What shipped is
in [CHANGELOG.md](../CHANGELOG.md). Sizes: S is under a day of work, M a few days, L more.

## Released

- **[0.5.0](https://github.com/dsablic/sidou/releases/tag/v0.5.0)** (2026-10-25): `.sid`
  playback on SIDOU's own 6502, SIDS and NOW PLAYING, tunes over SysEx.
- **[0.4.0](https://github.com/dsablic/sidou/releases/tag/v0.4.0)** (2026-10-25): SID drum kit
  and digi drums, generative STEP, recording over the USB cable, an ARP helper, songs to MIDI.
- **[0.3.1](https://github.com/dsablic/sidou/releases/tag/v0.3.1)** (2026-10-23): an install
  over Felucca boots straight into SIDOU.
- **[0.3.0](https://github.com/dsablic/sidou/releases/tag/v0.3.0)** (2026-10-23): the first
  wider release: help cards, the first-run tour, the output limiter, CHIP SPREAD.
- **[0.2.0](https://github.com/dsablic/sidou/releases/tag/v0.2.0)** (2026-10-22): the first
  release to run on a real FM-1, with USB-MIDI out.

## 1.0 (in progress)

**LAB: shape one sound and keep it.** A view for experimenting on the go: five pages with
drawings of the oscillator, envelope and more, one press to keep a sound, 16 USER sounds,
A/B, undo and variation rolls. ENV, LFO and FX on LIVE open it. Design:
[plans/2026-10-25-lab-design.md](plans/2026-10-25-lab-design.md). L, in progress.

**Smaller things that make 1.0 feel finished**

- **ARP helper: audition and waveform choice.** Hear the draft before SAVE and pick its
  waveform in TABLE's helper. S.
- **Readable small text.** TABLE's and STEP's small text at 2× where there is room. S-M.
- **Fewer late writes for dense `.sid` players.** Done: 24 render parts a block (from 16), 80
  writes a block's window (from 100, to stay in the interrupt's budget), proved by a
  one-write-a-sample bench and synthetic player bursts; Sound Test stays a known limit.
- **Minimal DICE settings.** Density and scale, saved per song. M.
- **Store health on STATUS.** A retired store sector is shown, and a background settings save
  that failed is detected and retried. S.
- **More test coverage.** Done: a gate that decodes every instruction of the image, and the
  ergo harness walks LOAD and the SONG pages, checking SAVE and HOME on every list.
- **A SIDS how-to video and What's new in 1.0.** S-M.
- **The GitHub look.** Avatar, social preview, the Firmware Hub card checked. S, by hand.

**Hygiene before the tag**

- **Its own USB ID.** A pid.codes product ID for the app, applied for by the user. 1.0 is not
  blocked by it: the ID lands in a 1.0.x. The update loader stays Felucca's, vendored byte for
  byte, on 0x1209:0x0002.
- **The ergo baseline refreshed** after LAB, with the accepted findings reviewed.
- **A device session gates 1.0.** USB audio recording, the first-run tour, unplugging the
  cable during playback, REC timing, LAB's readability and keep flow, a 2SID tune, and a long 48 kHz recording (the CPU erratum in Felucca's resampler, [felucca-bsp#16](https://github.com/hugelton/felucca-bsp/issues/16); if it fails, 48 kHz goes off until a fix).

## 1.x

**Sound and play**

- **The rest of DICE's settings:** range and rhythm. M.
- **A held step's audition plays its locks.** Hold a step and hear it with its locks. M; needs
  room in the audio interrupt, so a device check follows.
- **LAB's song bridge:** send a sound to a song as an instrument, pull one back. M-L. Its own
  design first (the LAB design calls it the second step).
- **Instruments from a `.sid`.** Lift an instrument out of a tune you play. M-L; pairs with
  the song bridge.
- **LAB: hold-to-peek and macros.** S-M each.

**Songs**

- **Generative play in TRACKER:** the same chance, MUTATE and DICE as STEP, C64 idioms
  (generated arpeggio chords, multiplexed drums, filter sweeps), a scale per lane. M each.
- **MIDI export on the device,** not only on the site; wavetable arpeggios and TRKB subtunes
  in the export. M.
- **More than 48 keys in the store.** Only if players hit STORE FULL (the starter pack uses
  11; LAB adds one). M-L; changes the store format.

**Connectivity**

- **A browser editor and librarian.** Edit instruments, tables and patterns, list the
  device's songs, export plain GoatTracker files; today the site sends songs and files over
  SysEx. L; no firmware change.
- **Screen mirror (FM-1 Live Link).** The screen, scopes and remote controls in a browser
  over the USB cable, for the other FM-1 firmwares too. L; the design text is in the
  [old roadmap](https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-18-roadmap-after-m2.md).

**Platform**

- **Move to felucca-bsp (Felucca 1.7.0's board support).** Felucca 1.7.0 moved its chip and board
  code (USB, flash, the second core) into [felucca-bsp](https://github.com/hugelton/felucca-bsp),
  under MPL-2.0. SIDOU takes it as it takes Felucca's code today, byte for byte with its flags:
  - its `chip/usb.c` in place of 1.6.1's, with its USB-name build settings instead of SIDOU's
    hook, once [felucca-bsp#14](https://github.com/hugelton/felucca-bsp/issues/14) (the SIE
    timing at `-O2`) and [#15](https://github.com/hugelton/felucca-bsp/issues/15) (a raw MIDI-in
    hook) are settled;
  - the update loader kept compatible: installs and updates from 0.x and from Felucca still work;
  - its notes on the CPU (64-bit multiplies beside 16-bit loads) checked against SIDOU's audio
    path;
  - what MPL-2.0 changes: the second core's code becomes usable, and the BLE variant's licence
    question (below) is worth asking again;
  - fm1-emu bumped past its watchdog model, so the tests check that a hang recovers.

  L; a device session (installs, updates, USB-MIDI, USB audio).

**Quality**

- **Knob curves for every parameter.** K1's cutoff, MASTER, K3 and K4 are done; the rest get
  a useful range, an even-in-perception curve and a test for dead zones and jumps. M.
- **Suggested automations.** A one-line toast offers "record this knob move as locks?" after
  you keep doing it. L, idea; cheap in the main loop.
- **SysEx during a chord.** The 100 ms gap before a store write is an ear call, not yet made.
  S.
- **Performance headroom.** A pass over the thin spots in the audio-interrupt gates (LIVE
  noise, SELECT into RULE, STATUS idle, MIDI floods, USB drain). M-L.
- **Faster test CI.** Split the 13-minute test and use nextest in CI. S-M.
- **REC from the keys, compensated.** The key-to-row timing is uncompensated (about 3 ms);
  decide by feel on the device. S.
- **sidou-flash retries on NAK BUSY** when a transfer meets a store write. S.

## Later and ideas

- **BLE MIDI as a user-built variant.** Source only (`SIDOU_WIRELESS=ble`), never published or
  built by CI: M-VAVE's BLE library is closed, and linking it into a distributed image would
  need every copyleft owner in the image to agree; a private build is allowed by the GPL. L.
  [Why](https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-18-roadmap-after-m2.md). To ask again once SIDOU is on felucca-bsp (MPL-2.0).
- **Layer mode.** Two sounds on one key. L; audio interrupt.
- **FM-1 Live Link extras:** a tray app, a share link, iOS.
- **RSID playback.** Not now (see below); revisit if there is demand.

## Not doing

- **RSID playback as built.** SIDOU refuses an RSID tune politely
  ([psid.md](reference/psid.md)): it needs a full C64 environment, not just the chips.
- **Combined-waveform rows in POOL.** POOL has no room for them
  ([flash-space plan](plans/2026-10-24-flash-space.md)).
- **Recording into a DAW with a plugin.** Superseded by USB audio out, which records over the
  cable.
- **2× oversampling and an idle LED glow.** Rejected: cost against a gain nobody asked for.
- **Our own BLE stack.** The radio's controller exists only in the closed library.
