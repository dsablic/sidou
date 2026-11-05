<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Slide and accent as STEP locks: design

**Status:** design for 1.7, 2026-11-05.

## What the player does

In STEP, with a step held: **FX** turns its **SLIDE** on or off, **ENV** its **ACCENT** (a toast
says which, the step shows the yellow lock notch). SLIDE holds the note into the next one with no
retrigger and glides to its pitch, as a 303's slide; ACCENT lifts the step's cutoff (+384),
resonance (+4) and the voice's sustain (full). Both are LOCKs: LFO on the held step clears them
with the others. The site's sounds have a 303 BASS to play them with.

## Format and engine

LOCK parameters 10 (slide) and 11 (accent), value 1. `song_lock_apply` (flash since 1.7's RAMTEXT
step) applies them after the player's frame: no change to the GoatTracker player. Older firmware
drops a LOCK chunk with an unknown parameter (song-format.md says so).

## Budgets

Flash about 1 KB; RAM 20 bytes of player state at the end of `song_ctl`; `SONG_ROW_LOCKS` stays
14 (a row with every sound lock, its chance, a slide and an accent at a pattern boundary can lose
one); RAMTEXT +48 (the play command's reset). Measured: the player's worst block (`song.rs`) went
from 6,278 to 6,488 instructions of its 6,500 (6 calls of a fast-forward slice): a first version
cost 6,533, over; the slides are now found inside the row's existing copy loop and the slide code
runs only on a channel that slides. The margin left is 12 instructions.
