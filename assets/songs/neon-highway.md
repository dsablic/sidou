<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# NEON HIGHWAY

| | |
|---|---|
| File | `neon-highway.sng` (GoatTracker 2, `GTS5`, with a SIDOU trailer; opens in GoatTracker 2.77) |
| Title | NEON HIGHWAY |
| Author | SIDOU contributors |
| Licence | GPL-3.0-or-later, as SIDOU |
| Source | `host/crates/sidou-tests/src/showcase/outrun.rs`: the song as data, written for SIDOU's demos |
| Chip | 8580 (the trailer's `CHPA`) |
| In the firmware | no: a file for the store, the demo videos and the starter pack |

An original synthwave track in E minor at 125 BPM (Em C G D, Em C Am B): a resonant
low-pass saw bass bouncing octaves under a four-bar filter sweep and a two-bar wah, a
driving sixteenth arpeggio riff, a pulse-width modulated lead with vibrato and slides, and
drums multiplexed with one-frame chord arpeggios on one voice. Intro, riff, hook,
breakdown, return, tag and ending; 73 s, looping from the riff. Not derived from any
other tune.

`SIDOU_SONG_UPDATE=1 cargo test -p sidou-tests --test showcase` rewrites the `.sng` after an
edit of the source; the test checks that the file is in step with it, reads and writes it
within the device's caps, plays it against GoatTracker's own packed player (gt2-ref), and
renders it unclipped on both chip models.
