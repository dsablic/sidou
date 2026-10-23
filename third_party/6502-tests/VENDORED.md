<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 6502 test data: Klaus Dormann's functional test, a subset of Tom Harte's SingleStepTests

Test data only, never built into the firmware.

## Klaus Dormann's 6502 functional test

**Test-only:** `host/crates/gt2-ref/tests/cpu.rs` runs it on the host 6502 the song player's
reference harness uses, and `host/crates/sidou-tests/tests/cpu6502.rs` on SIDOU's own 6502
(`core/cpu6502`), each of its builds.

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
below.

## Tom Harte's SingleStepTests, 6502 v1: the first 100 tests of each opcode

**Test-only:** `host/crates/sidou-tests/tests/cpu6502.rs` runs them on SIDOU's 6502
(`core/cpu6502`): the final registers and memory for all 256 opcodes, the cycle count for the
244 that do not JAM, and each hooked access's cycle stamp.

- **Source:** <https://github.com/SingleStepTests/65x02>, `6502/v1/`: 256 JSON files,
  `00.json` to `ff.json`, 10,000 tests each.
- **Upstream commit:** not recorded. The controller's copy
  (`/Users/denis/sidou-flash/harte/harte/`, 1.0 GB, fetched 2026-10-18 for the M3 6502
  study) came without its commit, and SIDOU's implementers do not fetch. The copy is
  identified by its files instead: `shasum -a 256 *.json | shasum -a 256` in it gives
  `441a66d321df079d9c039d1e90f4e85d9006c31d5d13502411e6b62b202fa821` (`00.json` alone:
  `cd255389bacb969ff028c67375e7d0fed4065bb9405a8432de4a7521e69d1024`). Whoever next fetches
  the set records the commit here, after checking these hashes against it.
- **Licence:** MIT, © Tom Harte and SingleStepTests' contributors (the repository's
  `LICENSE`; `LICENSES/MIT.txt`).
- **Local changes:** a subset, in another encoding: `harte/<op>.bin` holds the first 100
  tests of `<op>.json`, every field of each (the initial and the final state, the cycles),
  in the binary `host/crates/sidou-tests/src/harte.rs` describes (the M3 study's
  `harte2bin.py` layout: byte for byte the study's files' first 100 tests). 256 files,
  1,702,462 bytes; `shasum -a 256 *.bin | shasum -a 256` in `harte/` gives
  `d135f883fb3b1ad8f53c3dd9b7e259caf2d5eae30e63af4d3a94774bf4bee2eb`.
- **Made with** (from `host/`):
  `cargo run --release -p sidou-tests --bin harte-subset -- <the 6502/v1 directory> ../third_party/6502-tests/harte 100`
- **The full set** runs with
  `SIDOU_HARTE_DIR=<the 6502/v1 directory> cargo test --release -p sidou-tests --test cpu6502 -- --ignored harte_full_set`.
