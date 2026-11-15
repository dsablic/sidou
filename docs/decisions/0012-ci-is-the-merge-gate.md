<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0012: CI on every push is the merge gate

**Status:** accepted, 2026-11-15 (the owner); in force once `check.yml` is on main and its
`check` job is the required check. Replaces the local `tools/merge-check` as the gate
(TESTING.md, "What runs when"; the local check stays).

## Context

Every merge passed `tools/merge-check` on the owner's Mac: both firmware builds, fmt and clippy,
the GCC check, the package, the ergonomics gate, the emulator gate, the browser demo, the host
tests, UBSan, REUSE and the README check. It took 30 to 45 minutes, one run at a time across
every worktree, and a second branch waited for the lock. GitHub's Linux runners are free for a
public repository, twenty at a time, four cores each, and fm1-emu's JIT runs on x86-64.

## Decision

1. **`.github/workflows/check.yml` runs on every push to any branch** (and by hand), split over
   the runners, in about 10 to 12 minutes; a superseded run of the same branch is cancelled.
2. **Its `check` job is the merge gate** once it is green and has matched the local results: the
   same tests per binary, all passing (proven on `ci-fast`, 2026-11-15).
3. **The scope is the merge check's** (`tools/change-scope`, shared by both): docs only, a
   release's version line, or everything. On CI a change to `.github/` is code.
4. **`tools/merge-check` stays** for offline work and for what CI cannot run: the two tests of
   the way back to V15 need M-VAVE's stock package, which is not SIDOU's to redistribute.
5. **CI runs what the local check runs, no more and no less.** A test that skips locally (the V15
   tests, the watchdog death test, ergo's firmware tests in the host run) skips on CI too and is
   listed; adding one to the gate is a decision of its own.

## Consequences

- A branch's result comes in about a quarter of the time and needs no lock or local Docker.
- The whole workspace is built and linked on Linux at every push, which the Mac never did: two
  test failures that only Linux shows were found and fixed (design notes, "CI").
- The build image must carry the tools the jobs need (rustfmt, clippy, cargo-nextest); a change
  to it is published from `build-image.yml` before a workflow can rely on it.
