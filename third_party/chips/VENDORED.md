<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# floooh/chips `m6502.h`, vendored for the PSID oracle (test-only)

**Test-only.** `host/crates/psid-ref` builds it, with FM-1 SID's player
(`third_party/fm1-sid-player`), into the host oracle SIDOU's PSID environment is compared
against (M3 Task 7; `TESTING.md`, "The PSID oracle"). Nothing else compiles it: no CMake
target builds `third_party/`, and it never reaches the firmware. SIDOU's own 6502
(`core/cpu6502`) takes no code from it.

- **Source:** <https://github.com/floooh/chips>, `chips/m6502.h`, at commit
  `ee88c35ad6427341aa6999c3b07233e1f8bd2396` (2026-09-25, "m6502.h: fix nmi instruction"),
  the last commit that changed the file; `master` at `9e88298c` (2026-09-26) has the same
  file (git blob `20d2d35d961b979771b3b11a735f0b29e5512fb3`).
- **How it came:** FM-1 SID's `tools/fetch_ext.sh` fetches `m6502.h` from `master`
  unpinned; the M3 6502 study's copy (`/Users/denis/sidou-flash/harte/ext/chips/m6502.h`,
  fetched 2026-10-18) is this file. Checked 2026-10-24 against the repository's history: its
  git blob is the one above, so the commit is recorded here.
- **SHA-256:** `b3fc32a01f1f7cdc62f919ccba06cc20c2a333e7ca30c38189e31aa65868d46e`
  (144,215 bytes).
- **Licence:** zlib (`LICENSES/Zlib.txt`), © 2018 Andre Weissflog (the notice in the file's
  header comment, "zlib/libpng license"; the repository's `LICENSE` is the same).
- **Local changes:** none.
