<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU roadmap

Where SIDOU is going, in the order we mean to do it. Each line says what a player gets; the
why lives in [decision records](decisions/) and the how in [plans](plans/). What shipped is
in [CHANGELOG.md](../CHANGELOG.md) and on the [releases page](https://github.com/dsablic/sidou/releases).
Sizes: S is under a day of work, M a few days, L more.

## 2.0

- **Chip B on the second core** ([decision 0010](decisions/0010-second-core-reopened.md)): on by
  default, built for 2.0 ([Unreleased](../CHANGELOG.md#unreleased)) after the device check
  (hardware-test §42): core 0's audio time halved on the owner's FM-1, falling back to one core
  with the same sound; STATUS's CORES page for every player. The default image is 559,796 bytes,
  19,720 under the 579,516 ceiling.
- **A layer's level in LAB**: built for 2.0 ([Unreleased](../CHANGELOG.md#unreleased)), LAYERS' K4
  LEVEL. It fits one core: the gain rides on the voice's output in both render loops, about 8
  instructions a sample per chip, inside every audio budget unchanged
  ([design notes](design-notes.md#a-layers-level-20)).
## 2.1: live

What the grooveboxes on the FM-1 have for playing live and SIDOU still lacks (the landscape check,
2026-11-18; the owner agreed):

- **The next pattern queued, starting on the bar:** pick the pattern or order position to come
  while the song plays; it starts at the bar's end, shown on the song views. Melodee, SLOOP and
  JIANT have it. S-M.
- **Copy and paste in STEP:** a lane's steps or a whole pattern, with their locks and ratchets.
  JIANT has it. S.
- **A small set of punch effects** (about six: mute, filter slam, stutter, freeze and the like),
  held on a key while it plays. SLOOP, Melodee and JIANT have theirs. M.
- **USB host for keyboards:** SETTINGS' USB, COMPUTER or KEYBOARD (every power-on starts as
  COMPUTER, so installs always work): a class-compliant USB-MIDI keyboard, controller or synth,
  alone or behind a hub, into the same MIDI queue as USB, TRS and Bluetooth. felucca-bsp's
  `usb_host.c` (Melodee's, already vendored, off) does the hosting; SIDOU adds the row, the main
  loop's task, its buffers in NOINIT. The FM-1 does not power the port (a powered hub or a
  powered OTG adapter); while a host, no computer over USB. Melodee and JIANT have it. About 3-5 KB
  of flash. S-M.
- **Sounds from a `.sid` tune into the Library:** the converter's instruments as LAB sounds to
  keep, edit and send. SID-1's page does the like. Site only. S.

Watched, not planned: C64 speech (SAM, as EVILGENIUS has it; its licence is unclear) and a microSD
card (felucca-bsp has one on the LCD's SPI; it would need a card fitted).

## Open from earlier releases

- **SIDOU's own USB ID** (0x51D0): waits for pid.codes request #1341. The update loader stays
  Felucca's, byte for byte, on 0x1209:0x0002.
- **The first-run tour on a real FM-1**: the one part of the device session not yet checked there.
- **The quality pass**: performance headroom (the thin spots in the audio-interrupt gates: LIVE
  noise, SELECT into RULE, STATUS idle, the MIDI floods, the USB drain), knob-curve follow-ups from
  the device sessions, and the two `-Oz` puzzles (`gt2_song.c`'s goldens, `fx_slots_init.c`'s
  timing). M-L.
- **Small fixes**: a pulled LAB name comes back after a power cycle (today the sound does, the
  name not); the Live Link smoke test's HOME hold no longer depends on wall-clock timing.

## 1.x

**Songs**

- **RSID tunes into songs on the site:** 1.21's converter takes PSID tunes; an RSID tune needs
  the C64's interrupts emulated around SIDOU's 6502. Site only. M.
- **Combined drum-and-bass instruments** from DICE: a drum's first frames, then the bass's wave
  program, in one instrument (1.5 did the time-shared form). M.
- **More than 48 keys in the store.** Only if players hit STORE FULL (the starter pack uses 11;
  LAB adds one). M-L; changes the store format.

**The site**

- **The editor's rest:** instrument delete and reorder, pattern renumbering, editing a Stereo
  song's voices 4-6, and the preview's output stage (thump, chip B, MASTER)
  ([editor.html](https://dsablic.github.io/sidou/editor.html)). M.
- **Live Link's scopes**, sent as state ([the protocol](reference/live-link.md)). M.

**Quality**

- **Suggested automations.** A one-line toast offers "record this knob move as locks?" after you
  keep doing it. L, idea; cheap in the main loop.
- **Faster test CI.** An optional sharded emu-gate workflow on GitHub's x86 runners (the gate's
  JIT works there), to take the emulator stage off the local machine. S-M.

- **48 kHz USB recording** (only if players ask; SIDOU is 44.1 kHz only): the resampler shares
  TIMER5's tick with the USB poll, so on the FM-1 it loses about 10 % of the frames (felucca-bsp
  #16's 30-minute test, 2026-11-18). Smaller resampler slices, the poll first, the poll timed by
  the clock. S-M.

## Parked

- **SIDOU as a DAW plugin** (parked by the owner, 2026-11-18, until players ask for it): a SIDOU
  edition of fm1-emu's FM-1 plugin (AU, VST3, CLAP) running the real firmware, with a large mirror
  of the screen, real knobs for LAB's pages and LIVE, tracker entry from the computer keyboard and
  side panels drawn from the firmware's state. The spike ([its plan](plans/2026-11-05-daw-plugin-spike.md),
  code in `spikes/daw-plugin/`) runs SIDOU's package at 17.6x real time; stable parameters a LAB
  card, GPL-3.0-or-later. A first release M, the whole edition L.
- **The 8580 mixer's grit** (measured, not kept): the spike (its branch `mixer-8580` no longer exists; the measurements are here) puts
  a quadratic on the 8580's output; its second harmonic lands within 0.8 dB of reSIDfp's, but the
  distance to the CSG8580R5 recordings does not move (0.04 dB at most on every tune), for 80 bytes
  of RAMTEXT and 11 instructions a sample. The reSIDfp op-amp port stays parked for good (the
  owner, 2026-11-04).
- **Wi-Fi and Ableton Link** ([decision 0011](decisions/0011-bluetooth-midi.md)): until Optimist
  runs Link on a real FM-1. Bluetooth audio is dropped.

## Later and ideas

- **A second engine on the FM-1's second core** (research): users want SID beside polyphonic
  synthesis without re-flashing. Flash is the limit (2,364 bytes left in the slot), so it needs
  room found first.
- **Faster firmware switching from the site** (research): the Library page installs another
  firmware package and brings SIDOU back, keeping the songs and sounds on the FM-1.
- **Layer mode.** Two sounds on one key. L; audio interrupt.
- **FM-1 Live Link extras:** a tray app, a share link, iOS.
- **RSID playback.** Not now (see below); revisit if there is demand.

## Not doing

- **RSID playback as built.** SIDOU refuses an RSID tune politely
  ([psid.md](reference/psid.md)): it needs a full C64 environment, not just the chips.
- **Combined-waveform rows in POOL.** POOL has no room for them
  ([flash-space plan](https://github.com/dsablic/sidou/blob/v1.6.0/docs/plans/2026-10-24-flash-space.md)).
- **Recording into a DAW with a plugin.** Superseded by USB audio out, which records over the
  cable.
- **2× oversampling and an idle LED glow.** Rejected: cost against a gain nobody asked for.
- **Our own BLE stack.** Not needed: Optimist's open stack has the radio's controller.
