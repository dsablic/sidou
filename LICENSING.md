<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Licensing

SIDOU's own source code, tools and documentation are licensed under
**GPL-3.0-or-later** (see [LICENSE](LICENSE)). Every file carries an SPDX header
or is covered by [REUSE.toml](REUSE.toml). Full licence texts live in
[LICENSES/](LICENSES/). We follow the [REUSE](https://reuse.software/)
specification, and CI runs `reuse lint`.

Firmware images may include GPL-3.0-only code from other FM-1 projects. Those
images are distributed under GPL-3.0.

## Third-party components

This table is updated in the same pull request that adds or removes a
component. "Use" says whether a component ends up in the firmware image.

| Component | Origin | Licence | Use | Status |
|---|---|---|---|---|
| none yet | | | | |

Candidates under evaluation are listed in section 11 of the
[design document](docs/plans/2026-10-14-sidou-design.md).

## Music and content

No tunes ship yet. Each tune in the starter pack will have a metadata file
next to it, recording the composer, the licence or written permission, and
the source. SIDOU does not redistribute HVSC tunes; users load them from their
own copy.

## Trademarks

"Commodore", "C64", "SID" chip names and "M-VAVE" are used only to describe
what SIDOU is compatible with. No logos or trade dress are used.
