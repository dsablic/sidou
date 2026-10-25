<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU LOOP

| | |
|---|---|
| File | `sidou-loop.sid` |
| Title | SIDOU LOOP |
| Composer | SIDOU contributors |
| Licence | GPL-3.0-or-later |
| Source | `assets/songs/sidou-loop.sng`: the repository's song, packed by `gt2-ref` (GoatTracker 2.77's packer, default options, PAL) into PSID v2; `SIDOU_STARTER_UPDATE=1 cargo test -p sidou-tests --test starter` writes it |
| SHA-256 | `f8485cf54818d7f5a45491200148379247692feb936f6d8199765de101a88915` |
| Chip | 6581 |
| Clock | PAL, VBI (one play call a frame) |
| Songs | 1 |
| In the firmware | no (the store) |

The `.sid` is the song's own data with GoatTracker's playroutine, which carries its own permissive notice (`LicenseRef-GoatTracker-Playroutine`); the `.sng` it is built from is in the repository, as the GPL asks. [LICENSING.md](../../LICENSING.md#the-starter-pack) has the pack's tiers.
