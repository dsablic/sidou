<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Licensing

SIDOU's own source code, tools and documentation are licensed under
**GPL-3.0-or-later** (see [LICENSE](LICENSE)). Every file carries an SPDX header
or is covered by [REUSE.toml](REUSE.toml). Full licence texts live in
[LICENSES/](LICENSES/). We follow the [REUSE](https://reuse.software/)
specification, and CI runs `reuse lint`.

The firmware image links Felucca's GPL-3.0-only HAL (`platform/fm1/hal/`),
so the firmware image as a whole is distributed under **GPL-3.0-only**. Our
own files keep their GPL-3.0-or-later licence.

## Third-party components

This table is updated in the same pull request that adds or removes a
component. "Use" says whether a component ends up in the firmware image.

| Component | Origin | Licence | Use | Status |
|---|---|---|---|---|
| reSID 0.16 filter cutoff points (`core/sid/sid_curves_resid016.h`) | reSID 0.16, © 2004 Dag Lem | GPL-2.0-or-later | data compiled into the firmware | M0 stand-in, replaced in M1 |
| Felucca HAL @ `a53ecf42` (`platform/fm1/hal/`, see [VENDORED.md](platform/fm1/VENDORED.md)) | Felucca, © 2026 Leo Kuroshita, Hügelton Instruments | GPL-3.0-only | linked into the firmware | in use |
| `platform/fm1/crt0.S`, `platform/fm1/app.ld`, `platform/fm1/lcd.c` | adapted from Felucca's `firmware/crt0.S`, `firmware/app.ld` and `firmware/src/lcd.c` @ `a53ecf42`, © 2026 Leo Kuroshita | GPL-3.0-only | linked into the firmware | in use |
| font8x8_basic @ `8e279d2d` (`platform/fm1/font/font8x8_basic.h`) | font8x8, Daniel Hepper, after Marcel Sondaar and IBM's VGA fonts | public domain | compiled into the firmware | placeholder until our own font |
| `bench/fm1/crt0.S`, `bench/fm1/bench.ld` | adapted from fm1-sid, © 2026 Joakim Eriksson | BSD-2-Clause | benchmark program only | in use |
| libresidfp @ `c1791383` (`third_party/libresidfp`) | libsidplayfp project | GPL-2.0-or-later | host tests only; git submodule, never linked into firmware | in use |
| fm1-emu crates `fm1-core`, `fm1-soc`, `fm1-isa` @ `e52f4640` | Joakim Eriksson | MIT | emulator gate only; fetched by cargo | in use |
| Rust crates `cc`, `criterion`, `object`, `serde_json` | crates.io | MIT OR Apache-2.0 | host build, tests and benchmark only; fetched by cargo | in use |
| Rust crate `bindgen` | crates.io | BSD-3-Clause | host build only; fetched by cargo | in use |
| JieLi pi32v2 toolchain (`jieli-linux-toolchains-20260730.1`) | Zhuhai JieLi Technology | proprietary, freely downloadable | build tool; downloaded into the build image, not redistributed | in use |

The Rust crates above are the direct dependencies of the `host/` workspace.
Their transitive dependencies are pinned in `host/Cargo.lock`; none of them
ends up in the firmware.

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
