<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU LOOP

| | |
|---|---|
| File | `sidou-loop.sng` (GoatTracker 2, `GTS5`; opens in GoatTracker 2.77) |
| Title | SIDOU LOOP |
| Author | SIDOU contributors |
| Licence | GPL-3.0-or-later, as SIDOU |
| Source | `host/crates/sidou-tests/src/song_embed.rs`: the song as data, written for SIDOU (M2c Task 6) |
| In the firmware | `app/demo_song.c`, generated from the same source; PLAY (button 12) plays it |

An original eight-bar loop in A minor (Am, F, C, G) for SIDOU's tests and demos: a pulse
lead with a pulse-width sweep, vibrato and a low-pass filter sweep, a saw bass, and drums
from wavetable programs. Not derived from any other tune.

`SIDOU_SONG_UPDATE=1 cargo test -p sidou-tests --test app demo_song` rewrites the `.sng` and
`app/demo_song.c` after an edit of the source; the tests check that both are in step with it.
