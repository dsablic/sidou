<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# FM-1 SID's PSID player, vendored for the PSID oracle (test-only)

**Test-only.** `host/crates/psid-ref` builds a derived copy of `psid.c` (below) with
`third_party/chips/m6502.h` into the host oracle SIDOU's PSID environment is compared against
(M3 Task 7; `TESTING.md`, "The PSID oracle"). Nothing else compiles these files: no CMake
target builds `third_party/`, and they never reach the firmware. SIDOU's own PSID code
(`core/psid`) follows the player's design and carries its notice (`LICENSING.md`); it takes
nothing from FM-1 SID's `fm1-sid/firmware/` (GPL-3.0-only), which is not vendored.

- **Source:** <https://github.com/joakimeriksson/fm1-emu>, `fm1-sid/player/`, at commit
  `7ca5b96c9e828dea5176fc60f0b945abbf442895` (the M3 plan's pin; the fm1-emu checkout cargo
  keeps for that rev).
- **Licence:** BSD-2-Clause (`LICENSES/BSD-2-Clause.txt`), as each file's
  `SPDX-License-Identifier` says; the copyright line is fm1-emu's `LICENSE`'s: "Copyright (c)
  2026 Joakim Eriksson" (`REUSE.toml` adds it, since the files carry none). fm1-emu's root
  `LICENSE` is MIT; `fm1-sid/README.md` calls FM-1 SID BSD, and the files' own identifiers
  are taken as their licence.
- **Local changes:** none.

| File | SHA-256 | What it is |
|---|---|---|
| `psid.c` | `ca816da41a354a850da432449c2c988f11d4e33a3a83f9aa4b09d5b525850f0d` | the PSID environment around m6502.h: memory, `$01`, the calls, the I/O page's reads and writes |
| `psid.h` | `531d09cb20e1435e29b4ba98e640a0ac601ce5996c3597bada257f6d0c0ad898` | its interface |
| `psidlog.c` | `2c944a23293f1f2542d136d7f81eb38fd9d65c988be387c2b05be8e24204350d` | its host tool, a tune's writes as a sidlog; kept for the scheduling it shows (the first play call at the first frame boundary after init, then a period each), which `psid_ref::run` repeats; not built |

**The derived copy:** `host/crates/psid-ref/c/psid_oracle.c` is `psid.c` with lines added
between `/* psid-ref: begin */` and `/* psid-ref: end */` markers only: its copyright lines and
the BSD notice, a log of the I/O page's reads (a callback around `bus_read`, with the cycle,
the address and the value each read returned) and a byte of memory read back (for `$01` at a
call's start). `host/crates/psid-ref/tests/oracle.rs` checks this file's SHA-256 and that the
copy without those blocks is this file byte for byte.

*Pocket Sync* (FM-1 SID's own tune, `fm1-sid/tools/housetune.py`) is not vendored yet: the
plan leaves it to the controller, who runs the script once (`tunes/`, here).
