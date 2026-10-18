<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# CHROME COMET

| | |
|---|---|
| File | `chrome-comet.sng` (GoatTracker 2, `GTS5`, with a SIDOU trailer; opens in GoatTracker 2.77) |
| Title | CHROME COMET |
| Author | SIDOU contributors |
| Licence | GPL-3.0-or-later, as SIDOU |
| Source | `host/crates/sidou-tests/src/showcase/game.rs`: the song as data, written for SIDOU's demos |
| Chip | 6581 (the trailer's `CHPA`) |
| In the firmware | no: a file for the store, the demo videos and the starter pack |

An original track in the spirit of mid-eighties C64 game music, D minor at 150 BPM: a
staccato pulse bass bouncing octaves, a lead with a chirp attack, delayed vibrato and
slides, sixteenth arpeggio runs, laser zaps hard-synced to the bass, and one voice of
drums (a pitch-dropped pulse kick, a noise snare, a ring-modulated hat, triangle toms)
multiplexed with one-frame chord arpeggios. Intro, theme, theme climbing, second theme,
bass break under a resonant sweep, solo, theme, ending; 80 s, looping from the theme. Not
derived from any other tune: no other tune's melody, chords or riffs.

`SIDOU_SONG_UPDATE=1 cargo test -p sidou-tests --test showcase` rewrites the `.sng` after an
edit of the source; the test checks that the file is in step with it, reads and writes it
within the device's caps, plays it against GoatTracker's own packed player (gt2-ref), and
renders it unclipped on both chip models.
