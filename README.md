<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<p align="center"><img src="assets/logo/metal-chrome-lettering-2x.png" alt="SIDOU, in spiked metal letters with a red to white chrome bevel and a purple shadow" width="452" height="126"></p>

# SIDOU

[![CI](https://github.com/dsablic/sidou/actions/workflows/ci.yml/badge.svg)](https://github.com/dsablic/sidou/actions/workflows/ci.yml)

Custom firmware that turns the M-VAVE FM-1 into a C64 SID instrument. It has a
live-play mode, a step sequencer with a tracker underneath, and songs in
GoatTracker 2's format that the device plays. Planned: `.sid` playback and
importers for GoatTracker and SID-Wizard songs. The SID emulation adds an
amp-modeller-style "chip character" layer on top.

**Status:** M1 (the live instrument) and M2 (songs) are done: chip B, GoatTracker 2 songs
played by a C player on the device (PLAY, button 12), STEP and TRACKER and the song
pages, REC, parameter locks, power-loss-safe storage in flash, a first-run tour and help
cards. The latest version is **[v0.2.0](https://github.com/dsablic/sidou/releases/tag/v0.2.0)**
([CHANGELOG.md](CHANGELOG.md)), the first to run on a real FM-1 and to update over USB.
M3 (`.sid` files) is next; **v0.3.0** is the wider public release. See the
[design document](docs/plans/2026-10-14-sidou-design.md) for where it's going.

## Try it in your browser

**<https://dsablic.github.io/sidou/>** plays the latest tagged release
(deployed by `.github/workflows/pages.yml` on a `v*` tag): SIDOU running in
[fm1-emu](https://github.com/joakimeriksson/fm1-emu)'s emulated FM-1, played
from the computer keyboard, the mouse or touch, or a MIDI keyboard (Web
MIDI, through the emulated TRS jack).

To run your working tree's firmware the same way (Docker, Rust and Python 3
needed; [BUILDING.md](BUILDING.md), "Browser demo"):

```sh
tools/web-demo          # builds everything, then serves http://localhost:8642/
```

## Try it

**In the emulator** (no hardware needed). Build the firmware and the package
([BUILDING.md](BUILDING.md)), then, from `host/`:

```sh
SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate -- --nocapture --test-threads=1
```

The tests boot the firmware and the package in fm1-emu, play the keys,
knobs and MIDI, and check the sound and the screen
([TESTING.md](TESTING.md)). `build/fm1/sidou_package.wav` is what the
package plays with the A4 key held from power-on. fm1-emu's own command
line (`fm1-emu --fwsc build/fm1/sidou.fwsc`) stops early at the pinned
revision, at an instruction it does not execute yet; the tests work around
it ([docs/TODO.md](docs/TODO.md)).

**On hardware.** v0.2.0 runs on a real FM-1 (the [release](https://github.com/dsablic/sidou/releases/tag/v0.2.0)
has the package, `sidou.fwsc`). Installing custom firmware can leave an FM-1 unable to
boot, so this is entirely at your own risk: SIDOU comes with no warranty.

- **Install and update:** with Felucca's installer (`fm1_install.py sidou.fwsc`). The unit
  shows up on USB as "SIDOU FM-1", and later versions install over it the same way.
- **Back to M-VAVE's V15:** the same way, with M-VAVE's official `FM-1.fwsc`.
- **Recovery:** hold OCT- alone while switching on (about a second) to enter the chip's ROM
  boot mode, then restore a package with the unbricker (`tools/fm1-unbrick-mac`, or
  MvaveFM1Unbricker with jl-uboot-tool). This needs nothing of SIDOU but its first
  instructions.
- Read [docs/flash-day.md](docs/flash-day.md) first, and have the way back ready; the
  full [hardware checklist](docs/hardware-test.md) says what has passed on a unit and what
  has not (the song views, REC and the first-run tour are still to be checked there).

**What it plays:** on LIVE, one SID chip with three voices, four presets (SAW LEAD, PWM
BASS, SYNC LEAD, NOISE PERC) and DICE after them, which rolls a new sound each turn, three effect
slots (Distortion, Modulation, FX) after the chip in stereo, and the chip's profiles and
character (CHIP view). K1 is the cutoff, K2 the resonance, K3 the sound's own control
(pulse width, decay or sync ratio) and K4 the release; the big PRESETS encoder picks
the preset; SELECT and ALGORITHM page the knobs and the effects; MASTER is the volume.
PLAY (button 12) plays SIDOU's first song, SIDOU LOOP, and stops it; SEQ (11) opens the
song views (STEP, TRACKER, ORDER, INSTR, TABLE), and a song saves in GoatTracker's
format. MIDI notes play on every channel, with CCs for the main controls and program
change 0-3 for the first four presets. Every control and CC:
[docs/reference/controls.md](docs/reference/controls.md) and
[docs/reference/midi.md](docs/reference/midi.md).

## For contributors

- [BUILDING.md](BUILDING.md): building the host tools, the firmware and the
  package.
- [TESTING.md](TESTING.md): the test layers, and what a failure means.
- [docs/decisions/](docs/decisions/): decision records, starting with
  [0001](docs/decisions/0001-test-and-benchmark-harness.md) (C on the device,
  Rust for host tooling) and
  [0002](docs/decisions/0002-m0-sid-cost.md) (M0 SID cost).
- [docs/hardware.md](docs/hardware.md) and
  [docs/hardware-test.md](docs/hardware-test.md): the FM-1, and the checklist
  for a real unit.
- [CONTRIBUTING.md](CONTRIBUTING.md): how to report bugs and send changes.

SIDOU is an independent community project. It is not affiliated with
M-VAVE or with Commodore trademark holders.

## Licence

SIDOU is free software under the GNU General Public License v3.0 or later.
Third-party components keep their own licences. See [LICENSING.md](LICENSING.md)
and [CREDITS.md](CREDITS.md).
