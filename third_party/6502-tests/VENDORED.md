<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Klaus Dormann's 6502 functional test (test data only)

**Test-only:** `host/crates/gt2-ref/tests/cpu.rs` runs it on the host 6502 the song player's
reference harness uses; never built into the firmware.

- **Source:** <https://github.com/Klaus2m5/6502_65C02_functional_tests>, commit
  `7954e2dbb49c469ea286070bf46cdd71aeb29e4b` (2020-01-05), `bin_files/`.
- **Licence:** GPL-3.0-or-later, © 2012-2015 Klaus Dormann (the header of
  `6502_functional_test.a65`, repeated in the listing).
- **Local changes:** none.

| File | SHA-256 | What it is |
|---|---|---|
| `6502_functional_test.bin` | `fa12bfc761e6f9057e4cc01a665a7b800ff01ae91f598af1e39a1201d01953fd` | the assembled test, a 64 KiB image loaded at 0, entered at `$0400` |
| `6502_functional_test.lst` | `a85bf71692a7087182f3a574830e74d420d166032f37d385e34940e7bb8d49e3` | its listing: the build's settings (`disable_decimal = 0`: decimal mode is tested) and the success trap, `jmp *` at `$3469` |

It tests the documented NMOS opcodes only. M3's own 6502 needs the undocumented ones for
`.sid` files: those are tested with Tom Harte's per-opcode tests (design doc section 9),
not with this.
