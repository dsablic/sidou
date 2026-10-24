<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MW Title Remix, 2x-speed

| | |
|---|---|
| File | `2xtest.sid` |
| Title | MW Title Remix, 2x-speed |
| Composer | Cadaver (Lasse Öörni) |
| Licence | GPL-2.0-or-later |
| Source | `third_party/goattracker2/examples/2xtest.sng`: GoatTracker 2.77's example song, packed by `gt2-ref` (GoatTracker 2.77's packer, default options, PAL) into PSID v2; `SIDOU_STARTER_UPDATE=1 cargo test -p sidou-tests --test starter` writes it |
| SHA-256 | `0e3ed68afb4891abd6a778413c77c5fb119e7540c6fa592666a2352826849e42` |
| Chip | 6581 |
| Clock | PAL, CIA 1 timer A, 2x speed (a stub before the player sets the timer, as GoatTracker's own `.sid` writer does) |
| Songs | 1 |
| In the firmware | no (the store) |

The `.sid` is the song's own data with GoatTracker's playroutine, which carries its own permissive notice (`LicenseRef-GoatTracker-Playroutine`); the `.sng` it is built from is in the repository, as the GPL asks. [LICENSING.md](../../LICENSING.md#the-starter-pack) has the pack's tiers.
