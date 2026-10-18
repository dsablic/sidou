<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# IRON RITE

| | |
|---|---|
| File | `iron-rite.sng` (GoatTracker 2, `GTS5`, with a SIDOU trailer; opens in GoatTracker 2.77) |
| Title | IRON RITE |
| Author | SIDOU contributors |
| Licence | GPL-3.0-or-later, as SIDOU |
| Source | `host/crates/sidou-tests/src/showcase/barbarian.rs`: the song as data, written for SIDOU's demos |
| Chip | 6581 (the trailer's `CHPA`) |
| In the firmware | no: a file for the store, the demo videos and the starter pack |

An original heavy track in E Phrygian at 107 BPM, written for SCENE's RED MOON RITE: a
palm-muted pulse riff whose resonant low-pass snaps shut on every note, a hard-synced
growl riding the riff's pitch, a ring-modulated bell, a lead theme, sync screams answered
by anvils (a triangle ring-modulated by the lead's voice) in a duel where the cutoff is set
row by row, and war toms. Intro, riff, theme, duel, return, ending; 85 s, looping from the
riff. Not derived from any other tune.

`SIDOU_SONG_UPDATE=1 cargo test -p sidou-tests --test showcase` rewrites the `.sng` after an
edit of the source; the test checks that the file is in step with it, reads and writes it
within the device's caps, plays it against GoatTracker's own packed player (gt2-ref), and
renders it unclipped on both chip models.
