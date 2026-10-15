<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<p align="center"><img src="assets/logo/metal-chrome-lettering-2x.png" alt="SIDOU, in spiked metal letters with a red to white chrome bevel and a purple shadow" width="452" height="126"></p>

# SIDOU

[![CI](https://github.com/dsablic/sidou/actions/workflows/ci.yml/badge.svg)](https://github.com/dsablic/sidou/actions/workflows/ci.yml)

Custom firmware that turns the M-VAVE FM-1 into a C64 SID instrument. It has a
step sequencer with a tracker underneath, a live-play mode, `.sid` playback, and
importers for GoatTracker and SID-Wizard songs. The SID emulation adds an
amp-modeller-style "chip character" layer on top.

**Status:** M1a, the first firmware: a live SID instrument played from the
FM-1's keys and knobs and from MIDI (TRS and USB), with a status screen and
the community update protocol. It is tested in the fm1-emu emulator and on
the host only: **no FM-1 has run it yet**, and it is not ready to be
installed on hardware (see below). See the
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

## Try M1a

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

**On hardware: not yet.** Installing custom firmware can leave an FM-1
unable to boot, and SIDOU has not been tested on one. When it is time, it
installs like the other community firmwares, with Felucca's installer
(`fm1_install.py sidou.fwsc`), and its update loader is Felucca's own. Before
anything else, run the [hardware checklist](docs/hardware-test.md) with its
recovery tools at hand (FM-1-transporter, MvaveFM1Unbricker with
jl-uboot-tool) and M-VAVE's official `FM-1.fwsc` for the way back.
Entirely at your own risk: SIDOU comes with no warranty.

**What M1a plays:** one SID chip, three voices, four presets (SAW LEAD, PWM
BASS, SYNC LEAD, NOISE PERC); K1 cutoff, K2 resonance, K3 pulse width, K4
preset, MASTER volume, OCT-/OCT+, FX for the 6581/8580; MIDI notes on every
channel, CC 74 cutoff, CC 71 resonance, CC 1 pulse width, CC 7 volume,
program change 0-3 for the presets. Since M1c, three effect slots
(Distortion, Modulation, FX) after the chip, in stereo, and the chip
profiles and character (age, DAC, thump, bleed): on the panel in the CHIP
view (menu, CHIP: SELECT picks the chip or a slot, ALGORITHM its profile or
algorithm, K1-K4 its controls; EDIT + K1-K4 stays a shortcut to the selected
slot in every view), over MIDI on CC 85-87, 89-90 and 102-116: see
[docs/reference/controls.md](docs/reference/controls.md).

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
