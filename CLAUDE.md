<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Working on SIDOU

## Code

- **No code comments.** Make code self-documenting: clear names, small functions, named
  constants. Keep only what tools require (SPDX headers, a clippy-required `SAFETY` note).
- **Design decisions go in [docs/design-notes.md](docs/design-notes.md)**, a running document by
  area: why a budget is what it is, a hardware quirk (the FM-1 faults on any misaligned 16- or
  32-bit RAM access), why a file builds at `-Os`, a protocol choice. Cross-cutting decisions are
  also records in [docs/decisions/](docs/decisions/). History lives in git and the CHANGELOG.
- **Vendored upstream code stays byte for byte** with its flags (Felucca's `usb.c`,
  `third_party/`); SIDOU hooks around it, never forks it.
- **Ergonomics first:** the player sees what they tweak. An encoder that changes a parameter
  shown in small print brings up a popup with its name and value, large.

## Tests

- **Code and tests are DRY:** one helper, table or fixture instead of repeated blocks; shared
  test setup lives in the crate's common module.
- **Every test proves behaviour.** No low-value tests: a test that compares a constant against a
  literal, restates the code, or can't fail when the behaviour breaks is removed, not written.
- Test first for every behaviour change; firmware behaviour is proven in fm1-emu (emu-gate), the
  host C in sidou-tests, the site with node tests and `tools/web-demo --check`.
- Run targeted tests while working (`nice`, a few threads); the full merge check runs every stage
  before anything reaches `main`. Budgets (flash, RAMTEXT, instruction counts) are not raised to
  make a change fit without the owner's decision.

## Docs and releases

- **README.md names no release.** Version information lives in CHANGELOG.md: each release starts
  with a one-line summary. README, the demo page (`web/index.html`), the cheat sheet and
  `fmhub.json` (the Firmware Hub card) stay current with every feature. `tools/readme-check`
  enforces the rules it can.
- A release is tagged on its release branch, which merges into `main` right after the tag.
- `docs/reference/midi.md` is generated from its test's template (`SIDOU_MIDI_DOC_UPDATE=1`).

## The device

An FM-1 may be connected. Never flash it, send it SysEx or open its MIDI ports without the
owner's explicit yes for that step; test in the emulator first.
