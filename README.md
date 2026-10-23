<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<p align="center"><img src="assets/logo/metal-chrome-lettering-2x.png" alt="SIDOU, in spiked metal letters with a red to white chrome bevel and a purple shadow" width="452" height="126"></p>

# SIDOU

[![CI](https://github.com/dsablic/sidou/actions/workflows/ci.yml/badge.svg)](https://github.com/dsablic/sidou/actions/workflows/ci.yml)

SIDOU is free firmware that turns the M-VAVE FM-1 into a C64 SID music machine. It has two
SID chips, each with its own character: age, DAC sloppiness, thump and filter bleed. It
plays and saves GoatTracker 2 songs, so a song made on the panel opens in GoatTracker and a
GoatTracker song plays on the device. You write songs on the panel with STEP and a tracker,
and you can play the chip live from the keys or over MIDI.

<p align="center"><img src="docs/media/sidou-screens.png" alt="Eight screens of SIDOU on the FM-1's LCD: the boot title, LIVE playing a song with the keys on chip B, the TRACKER, STEP, the CHIP view, the RED MOON RITE scene, an ORDER list and an instrument page" width="1002"></p>

*The boot title, LIVE, TRACKER, STEP, CHIP, the RED MOON RITE scene, ORDER and INSTR, from the
screenshots the tests check.*


[![SIDOU in 77 seconds](docs/media/videos/sidou-intro.webp)](https://dsablic.github.io/sidou/videos.html#sidou-intro)

**Watch:** [SIDOU in 77 seconds](https://dsablic.github.io/sidou/videos.html#sidou-intro) · [What's new in 0.3.0](https://dsablic.github.io/sidou/videos.html#whats-new) · [all the how-to videos](https://dsablic.github.io/sidou/videos.html)
## What you can do

- **Play the SID live.** Four presets (SAW LEAD, PWM BASS, SYNC LEAD, NOISE PERC) and DICE,
  which rolls a new sound at each turn. Cutoff, resonance, the sound's own control and the
  release are on K1 to K4.
- **Shape the chip.** The CHIP view picks the model (6581 or 8580) and a profile, and sets
  its character: age, DAC, thump and bleed. Three effect slots (distortion, modulation, FX)
  follow the chip in stereo.
- **Use two SIDs.** Songs play on one chip while the keys play the other, on top of the song.
- **Make songs on the panel.** STEP is a drum-machine view of a pattern. TRACKER is
  GoatTracker's layout, one voice at a time. ORDER, INSTR and TABLE edit the rest. REC records
  the keys and the filter knobs while the song plays, and parameter locks are on STEP.
- **Keep GoatTracker 2 songs.** SAVE writes a `.sng` to the device's flash. Power loss does not
  corrupt it.
- **Take a song into a DAW.** The site's [Songs to MIDI](https://dsablic.github.io/sidou/midi.html)
  page turns any GoatTracker 2 `.sng` into a `.mid`, a MIDI channel a SID voice, with the notes where
  GoatTracker's player puts them.
- **Play from MIDI.** Notes on every channel, CCs for the main controls, program change for
  presets and songs ([midi.md](docs/reference/midi.md)).
- **Look at it.** SCENE has generative pieces, and RED MOON RITE is the first illustrated piece.
- **Learn it from the device.** A first-run tour, and a help card on every screen.

Coming next: `.sid` file playback (M3).

## Try it in your browser

**<https://dsablic.github.io/sidou/>** runs the newest build in an emulated FM-1 (it follows the main branch, so it can be ahead of the latest release)
([fm1-emu](https://github.com/joakimeriksson/fm1-emu)). Play it from the computer keyboard,
the mouse, touch or a MIDI keyboard (Web MIDI). The emulated FM-1 keeps its flash in the
browser, so your settings and songs survive a reload. The page can export and import that
flash.

## Install

Installing is at your own risk, and SIDOU comes with no warranty; the official firmware is one
install away, and holding OCT- at power-on is a rescue ([docs/flash-day.md](docs/flash-day.md)).

1. Download the `.fwsc` file (for example `sidou-0.3.1.fwsc`) from the [latest release](https://github.com/dsablic/sidou/releases/latest).
2. Install it with Felucca's installer (`fm1_install.py sidou-0.3.1.fwsc`). The unit then shows
   up on USB as "SIDOU FM-1", and later versions install over it the same way.
3. **Back to stock:** install M-VAVE's official `FM-1.fwsc` (V15) the same way.
4. **Recovery:** if the unit does not boot, hold OCT- alone while switching on, for about a
   second. That enters the chip's ROM boot mode, and the unbricker (`tools/fm1-unbrick-mac`, or
   MvaveFM1Unbricker) can restore a package. It needs nothing of SIDOU.

The installer has no backup, so save your songs first (see below). The
[hardware checklist](docs/hardware-test.md) says what has passed on a real unit and what has
not.

## Controls

On LIVE:

| Control | Does |
|---|---|
| Note keys | play the chip (F3 to G5 at the current octave) |
| OCT- / OCT+ | octave down and up |
| K1, K2 | cutoff, resonance |
| K3 | the sound's own control: pulse width, decay or sync ratio |
| K4 | release |
| PRESETS | the preset (DICE after the fourth) |
| SELECT / ALGORITHM | the knob page (TONE, DIST, MOD, FX) / the chip profile or effect algorithm |
| EDIT held + K1 to K4 | the selected effect slot's amount, A, B and mix |
| SCL / ENV | the next effect slot / its next algorithm |
| MASTER | volume |
| SEQ | the song views, back to the one you used last; again: STEP, TRACKER, ORDER, INSTR, TABLE |
| PLAY / REC | play and stop the song / record mode |
| SAVE | in the menu: opens the highlighted item; in a song view: saves the song |
| HOME | the menu from LIVE, back one view elsewhere (settings are under SETTINGS in the menu) |

**Hold HOME on any screen for its help card**: it says what each control does there, for what the screen is doing (SELECT turns a two-page card). Every control, every screen and every MIDI
message is in [docs/reference/controls.md](docs/reference/controls.md) and
[docs/reference/midi.md](docs/reference/midi.md).

## Songs and GoatTracker

SIDOU's songs are GoatTracker 2 `.sng` files ([format notes](docs/reference/song-format.md)).

- **On the device:** SAVE in a song view stores the song in the FM-1's flash. The menu's SONG
  item loads, creates, saves under a name and deletes songs.
- **Over USB:** SysEx GET reads a song from the device and PUT writes one to it
  ([protocol](docs/reference/sysex.md)).
- **The showcase songs:** a fresh install lists only the built-in SIDOU LOOP. The
  [Songs page](https://dsablic.github.io/sidou/songs.html) of the browser demo sends
  CHROME COMET, IRON RITE, NEON HIGHWAY and PAPER LANTERNS ([`assets/songs`](assets/songs/)) to a
  connected FM-1 in one click (Chrome or Edge: Web MIDI with SysEx; Safari has none). Run locally,
  `tools/web-demo` serves it at `http://localhost:8642/songs.html`.
- **On your computer:** the `.sng` opens in GoatTracker 2, and a GoatTracker 2 song put on the
  device plays there if it fits the device's limits. The device refuses one it cannot play, with
  a message.

## For developers

- [BUILDING.md](BUILDING.md): building the host tools, the firmware and the package.
- [TESTING.md](TESTING.md): the test layers, and what a failure means.
- [docs/videos.md](docs/videos.md): the how-to videos, made from the recipes in the emulator
  (`tools/make-videos`).
- [docs/decisions/](docs/decisions/): decision records, starting with
  [0001](docs/decisions/0001-test-and-benchmark-harness.md) (C on the device, Rust for host
  tooling) and [0002](docs/decisions/0002-m0-sid-cost.md) (M0 SID cost).
- [docs/hardware.md](docs/hardware.md) and [docs/hardware-test.md](docs/hardware-test.md):
  the FM-1, and the checklist for a real unit.
- [CONTRIBUTING.md](CONTRIBUTING.md): how to report bugs and send changes.

**In the emulator** (no hardware needed). Build the firmware and the package
([BUILDING.md](BUILDING.md)), then, from `host/`:

```sh
SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo nextest run --release -p emu-gate
```

(`cargo install --locked cargo-nextest` once; plain `cargo test --release -p emu-gate` also works, more slowly.)

The tests boot the firmware and the package in fm1-emu, play the keys,
knobs and MIDI, and check the sound and the screen
([TESTING.md](TESTING.md)). `build/fm1/sidou_package.wav` is what the
package plays with the A4 key held from power-on. fm1-emu's own command
line runs the package too (`fm1-emu --fwsc build/fm1/sidou.fwsc --ms 10000`).

To run your working tree's firmware in the browser (Docker, Rust and Python 3 needed;
[BUILDING.md](BUILDING.md), "Browser demo"):

```sh
tools/web-demo          # builds everything, then serves http://localhost:8642/
```

## Status and roadmap

- **v0.3.1** is released (2026-10-23, [release notes](docs/release-notes/v0.3.1.md)): an
  install over Felucca boots straight into SIDOU (it detoured through UBOOT for 45 s).
- **v0.3.0** (2026-10-23, [release notes](docs/release-notes/v0.3.0.md),
  [CHANGELOG.md](CHANGELOG.md)) was the first wider public release: GoatTracker 2 songs written
  and played on the FM-1, two SIDs (6581 and 8580) with the chip's character dialled in, STEP
  with parameter locks, REC over a loop, CHIP SPREAD, chord arpeggios, MIDI and SysEx over USB,
  help cards on the device. v0.2.0 (2026-10-22) was the first version to run on a real FM-1.
- **Next:** SETTINGS → RESET, sounds and settings back to factory with the songs kept.
- **M3** follows: `.sid` playback.

Installs, updates, the way back to stock, the OCT- recovery, MIDI and SysEx over USB, the song views and REC have run on a real unit; the first-run tour is still to be checked there. See the
[roadmap](docs/plans/2026-10-18-roadmap-after-m2.md) and the
[design document](docs/plans/2026-10-14-sidou-design.md).

## Credits and licence

SIDOU stands on [Felucca](https://github.com/hugelton/Felucca) (the FM-1 platform code and
installer), reSID and reSIDfp (the SID models), [GoatTracker 2](https://sourceforge.net/projects/goattracker2/)
(the song format), and [fm1-emu](https://github.com/joakimeriksson/fm1-emu) (the emulator the
tests and the browser demo run on). The full list is in [CREDITS.md](CREDITS.md).

SIDOU is an independent community project. It is not affiliated with
M-VAVE or with Commodore trademark holders.

SIDOU is free software under the GNU General Public License v3.0 or later.
Third-party components keep their own licences. See [LICENSING.md](LICENSING.md)
and [CREDITS.md](CREDITS.md).
