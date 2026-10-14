<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU

[![CI](https://github.com/dsablic/sidou/actions/workflows/ci.yml/badge.svg)](https://github.com/dsablic/sidou/actions/workflows/ci.yml)

Custom firmware that turns the M-VAVE FM-1 into a C64 SID instrument. It has a
step sequencer with a tracker underneath, a live-play mode, `.sid` playback, and
importers for GoatTracker and SID-Wizard songs. The SID emulation adds an
amp-modeller-style "chip character" layer on top.

**Status:** M0 complete: SID core and benchmark harness. Nothing is
installable yet. One emulated SID costs 41 % of its CPU budget on the FM-1
([decision record 0002](docs/decisions/0002-m0-sid-cost.md)). See the
[design document](docs/plans/2026-10-14-sidou-design.md) for where it's going.

## For contributors

- [BUILDING.md](BUILDING.md): building the host tools and the firmware.
- [TESTING.md](TESTING.md): the test layers, and what a failure means.
- [docs/decisions/](docs/decisions/): decision records, starting with
  [0001](docs/decisions/0001-test-and-benchmark-harness.md) (C on the device,
  Rust for host tooling) and
  [0002](docs/decisions/0002-m0-sid-cost.md) (M0 SID cost).
- [CONTRIBUTING.md](CONTRIBUTING.md): how to report bugs and send changes.

SIDOU is an independent community project. It is not affiliated with
M-VAVE or with Commodore trademark holders.

## Licence

SIDOU is free software under the GNU General Public License v3.0 or later.
Third-party components keep their own licences. See [LICENSING.md](LICENSING.md)
and [CREDITS.md](CREDITS.md).
