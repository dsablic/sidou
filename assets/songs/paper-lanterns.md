<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# PAPER LANTERNS

| | |
|---|---|
| File | `paper-lanterns.sng` (GoatTracker 2, `GTS5`, with a SIDOU trailer; opens in GoatTracker 2.77) |
| Title | PAPER LANTERNS |
| Author | SIDOU contributors |
| Licence | GPL-3.0-or-later, as SIDOU |
| Source | `host/crates/sidou-tests/src/showcase/chill.rs`: the song as data, written for SIDOU's demos |
| Chip | 8580 (the trailer's `CHPA`) |
| In the firmware | no: a file for the store, the demo videos and the starter pack |

An original chill track in D major at a swung 94 BPM (GoatTracker's funktempo, 9 and 7
ticks): a triangle bass whose downbeats open with a pitch-dropped thump, multiplexed with
a rim click; a slowly pulse-width modulated melody, later a pulse-to-triangle flute; and
seventh-chord arpeggios two frames a note through a low-pass that opens, breathes and
closes. Intro, A, B, A with a higher second half, ending; 82 s, looping from A. Not derived
from any other tune.

`SIDOU_SONG_UPDATE=1 cargo test -p sidou-tests --test showcase` rewrites the `.sng` after an
edit of the source; the test checks that the file is in step with it, reads and writes it
within the device's caps, plays it against GoatTracker's own packed player (gt2-ref), and
renders it unclipped on both chip models.
