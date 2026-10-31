<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Design notes

Why SIDOU's code is the way it is, by area. Code carries no explanatory comments; the decisions
behind it live here. Cross-cutting decisions also have records in [decisions/](decisions/).
The cleanup before 1.5's release moves the decisions now written as code comments here.

## Hardware

- The FM-1's CPU faults on any misaligned 16- or 32-bit RAM access; fm1-emu does not model it.
  Structures copied word by word, especially in the audio interrupt, are aligned to 4 bytes with
  a static assertion (1.1's ARP helper crash).

## Flash and build flags

- The app slot is 581,564 bytes; each release plan sets a ceiling (`flash_budget.rs`).
- Cold code builds at `-Os`/`-Oz`; files on instruction-budget paths keep `-O2`. `gt2_song.c`
  stays at `-Os`: at `-Oz` it moved four LCD goldens, cause not yet found.
