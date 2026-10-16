<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Implementer brief (shared by every M2 task)

You are implementing one task of a SIDOU plan. SIDOU is custom firmware that turns the
M-VAVE FM-1 (JieLi AC7911B8 pi32v2, 240×240 RGB565 SPI LCD) into a C64 SID instrument.
Device code is C11 with no allocation and no libm; host tooling and tests are Rust in
`host/`; fm1-emu runs the firmware for end-to-end tests.

## Before you start
- Read your task's section of the plan file in full, the plan's header (goal, constraints,
  rules), and `docs/plans/2026-10-15-m2-overview.md`.
- Read the code and docs the task names. Follow the existing code's style and comment density.
- Use TDD: write the failing test first, see it fail, then implement.

## Rules
- Work only in your worktree and branch. Commit there; **never push, never tag** — the
  controller merges and pushes. Stage only your own files.
- Commit messages end with a blank line then `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- Run host commands from `host/` with `PATH="/opt/homebrew/opt/rustup/bin:$PATH"`.
- Judge every check by its **exit code**, never by grepping coloured output.
- **Tolerances and bounds only tighten.** A bound may be set from a new measurement for new
  code; an existing bound is never loosened. If one can't be met, stop and report.
- Host and target stay **bit-exact** for audio (`-ffp-contract=off`).
- Audio-interrupt code lives in `.ram_text`; `ram_audio.rs` rejects flash calls and flash
  data loads from it. UI code stays out of `.ram_text`.
- Every view's worst `ui_poll` and every input + frame pass is tied at compile time to
  `POLL_INSTRUCTIONS` (120k); every frame ≤ 20 ms of SPI.
- GCC check on every changed C file under `app/` and `core/`:
  `docker run --rm -v "$PWD":/src -w /src gcc:13 gcc -O3 -Werror -Wall -Wextra -std=c11 -ffp-contract=off -Wshadow -DSIDOU_ALL_COMBINED_PROFILES -DSIDOU_MIDI_MAP_DOC -Icore -Ibench -Iapp -Ihost/crates/sidou-sys -c <file> -o /dev/null`
  (`app/midi_usb.c`'s stringop-overflow there is a known false positive; `platform/fm1`
  files are covered by the fm1 build with `-Wall -Wextra`).
- `noinline` helpers are `__attribute__((noinline, unused)) static`, never `static inline`.
- SPDX headers on every new file; REUSE stays compliant.
- Temporary files go in `build/scratch/` in your worktree.
- Never put the user's email address or any personal data in requests to outside services.
- **A real FM-1 may be plugged into this computer. Never open, enumerate, send to, or flash
  any USB or MIDI device**: no CoreMIDI/Web MIDI/libusb code against real ports, no SysEx,
  no installer runs. Everything runs in fm1-emu or host tests. Hardware steps go into
  `docs/hardware-test.md` for the user to run.

## Checks before you report (each must exit 0)
- `cargo fmt --all --check`
- `cargo clippy --locked --workspace --all-targets -- -D warnings`
- `cargo test --locked --workspace --release`
- `SIDOU_SANITIZE=undefined cargo test --locked --workspace`
- the GCC check
- fm1 build from the repo root: `tools/fm1-docker cmake --preset fm1 && tools/fm1-docker cmake --build --preset fm1`
- `sidou-pack` as in `.github/workflows/ci.yml` (so the package test is fresh)
- `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --locked --release -p emu-gate -- --nocapture --test-threads=1`
- `uvx --from 'reuse[charset-normalizer]' reuse lint`
- `tools/web-demo --check` if the task changes anything the firmware shows or plays

## Report
Commits; what you built; design decisions and deviations from the plan (with reasons);
measured costs against every gate (instructions/sample, `ui_poll`, SPI, flash, RAM,
RAMTEXT); tests added; check results; anything left open. Stop after your task.
