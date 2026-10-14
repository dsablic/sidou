<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Testing SIDOU

All tests, benchmarks and tools live in the cargo workspace in `host/`. They
call the C core through FFI (`sidou-sys`), so they test the same code that
runs on the device. Commands below run from `host/`. [BUILDING.md](BUILDING.md)
lists the prerequisites.

## Quick reference

| Layer | Command | Where |
|---|---|---|
| Everything except the emulator gate | `cargo test --workspace --release` | all crates |
| Unit tests | `cargo test --release -p sidou-tests --test unit` | `crates/sidou-tests/tests/unit/` |
| App logic (live instrument, MIDI parser) | `cargo test --release -p sidou-tests --test app` | `crates/sidou-tests/tests/app/`; `SIDOU_FREQ_TABLE_UPDATE=1` regenerates `app/live_freq.c` |
| Golden renders | `cargo test --release -p sidou-tests --test golden` | `crates/sidou-tests/tests/golden.rs`, `tests/golden/hashes.txt` |
| reSIDfp reference | `cargo test --release -p sidou-tests --test reference` | `crates/sidou-tests/tests/reference.rs` |
| Filter comparison | `cargo run --release -p sidou-tests --bin sidou-compare -- <dir>` | `crates/sidou-tests/src/bin/sidou_compare.rs` |
| Host benchmark | `cargo bench -p sidou-bench` | `crates/sidou-bench/benches/workload.rs` |
| Emulator tests (gate and boot) | `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate -- --nocapture --test-threads=1` | `crates/emu-gate/tests/` |

Use `--release` for the tests. The C code is optimised either way (the test
profile sets `opt-level = 3`), but the reference tests do a lot of Rust-side
signal analysis.

## Test layers

### Unit tests

One module per part of the core: oscillator (`sid_osc`), noise (`sid_noise`),
hard sync (`sid_sync`), envelope (`sid_env`), filter (`sid_filter`), the chip
(`sid`) and constants (`sid_defs`). Most pin a documented SID behaviour, such
as the ADSR delay bug, the noise LFSR taps or hard-sync timing.

**A failure means** the core no longer behaves like the chip in that
respect. Fix the code, unless the test itself pinned the wrong behaviour; in
that case, say in the commit which source (reSID, a measurement, a datasheet)
shows the new expectation is right.

### Golden renders

`renders_match_recorded_hashes` renders the shared benchmark workload
(`bench/workload.c`) for a 6581 and an 8580 with one chip, and a 6581 with
two chips. It compares the FNV-1a 64 hash of each render with
`tests/golden/hashes.txt`. `workload_never_clips_and_has_no_dc_offset` checks
the same renders for clipping and DC offset.

**A failure means** the output changed, by as little as one bit. If you did
not mean to change the sound, it's a regression. If you did, rewrite the
file:

```sh
SIDOU_GOLDEN_UPDATE=1 cargo test --release -p sidou-tests --test golden
```

Before you commit the new hashes, listen to the change and say in the commit
message why the sound changed. A hash change on its own proves nothing.

Golden hashes depend on exact float results. Compiler flags that relax float
semantics (`-ffast-math`, `-ffp-contract=fast`, passed through `CFLAGS`)
change them; see [BUILDING.md](BUILDING.md).

### reSIDfp reference tests

These render the same register writes on SIDOU and on reSIDfp (libresidfp,
test-only) and compare them:
- **pitch**: an unfiltered sawtooth at A4 must be within 0.1 cents of the
  theoretical frequency and of reSIDfp, for both models (a reSIDfp result off
  from theory points at the harness, not the core);
- **envelope shape**: the RMS envelope of a note through attack, decay,
  sustain and release, normalised to the sustain level and compared down to
  -20 dB, must be within 0.75 dB of reSIDfp on the 8580, and 2.5 dB on the
  6581.

The tests print the measured values. The M0 values are in
[decision record 0002](docs/decisions/0002-m0-sid-cost.md).

**A failure means** SIDOU moved away from reSIDfp on something both
should agree on. The 6581 tolerance is looser because M0 does not model the
6581's non-linear envelope DAC; M1 does and tightens it.

**Tolerances only get tighter.** Never loosen one to make a failure pass,
unless a decision record in `docs/decisions/` explains why.

### Filter comparison (`sidou-compare`)

`sidou-compare` renders low-, band- and high-pass noise sweeps on both
engines, writes each pair as 16-bit WAV files into `<dir>`, which must exist (default: the
current directory) and prints the largest envelope difference per scenario
and model. It has no pass/fail threshold: it is for listening and for
tracking the filter's distance from reSIDfp between milestones. The
`sidou_compare_smoke` test only checks that it runs and writes all twelve
files.

### Host benchmark

A criterion benchmark of the shared workload for one and two chips, on both
models. Throughput is reported per chip (samples × chips), and
`workload_begin` is not timed. Reports go to `host/target/criterion/`.

It has no threshold. Use it to compare before and after a change on the
same machine; it says little about the FM-1, where the emulator gate is
the measure.

### Emulator gate

The M0 decision gate. It runs the pi32v2 benchmark programs
(`sid_bench_1` and `sid_bench_2`) in fm1-emu, used as a library, and:
- counts the instructions per output sample, and fails if one chip needs
  more than 900, or two chips more than 1,800 (design document, section 8);
- renders the same workload on the host build of the same C code, and fails
  if any sample differs by more than 1 LSB (at M0 they are identical);
- writes `sid_budget_1.json` and `sid_budget_2.json` into the fm1 build
  directory.

Build the `fm1` preset first (see [BUILDING.md](BUILDING.md)). Without
`SIDOU_FM1_BUILD`, the gate prints a message and passes without running,
which is why the CI job checks that the JSON files exist.

Instruction counts are a lower bound on cycles: the pi32v2 mostly issues one
instruction per cycle, but code fetched from flash (XIP) and divides take
longer.

fm1-emu at the pinned revision cannot execute one instruction that clang
emits (`{R*4} = [-R]`). The emulator tests step it themselves
(`crates/emu-gate/tests/common/mod.rs`); see [docs/TODO.md](docs/TODO.md).

**A failure means** one of three things: the core got too slow for the
budget, the target and host builds no longer compute the same thing (look
for float flags or undefined behaviour), or the gate could not run the
program (the message names the file or the faulting address).

### Boot test

`crates/emu-gate/tests/boot.rs` boots the firmware image
(`platform/fm1/sidou_app.bin`) in fm1-emu the way the FM-1's SPL starts it:
mapped at `0x02000120`, entered in user mode with `r0` pointing at a
boot-parameter block. The A4 key is held from power-on. It runs 900 ms of
emulated time and checks that:
- the audio interrupt delivers the frames of at least the 500 ms after the
  LCD's start-up delays;
- left and right are equal;
- the output is sample for sample what the host build of the same code renders
  for the same instrument (`live_init`, the MASTER pot, then the A4 note-on,
  which the test finds in one of the first 16 blocks);
- the note is not silent and its pitch is 440 Hz within 0.5 %.

It writes `sidou_boot.json` (frames, peak, measured pitch, the note-on's
block, instructions in and out of interrupts, the largest audio block) into the
fm1 build directory. Like the gate, it skips without `SIDOU_FM1_BUILD`.

### Live instrument test

`crates/emu-gate/tests/live.rs` plays the firmware through fm1-emu's front
panel (keys, encoders, the MASTER pot) and checks:
- a key plays its pitch (A4 and E4, within 0.5 %), and nothing plays before;
- OCT+ doubles the pitch and lights its LED; a held key lights its LED;
- turning K1 down by 40 detents cuts the share of the power above 2 kHz at
  least fourfold;
- MASTER turned down lowers the peak;
- the LCD after boot: the C64 blue at a fixed background pixel, the light blue
  border, text inside, no other colours; nothing is sent while nothing
  changes, and a held note redraws one strip;
- the 6581/8580 toggle: `sid_init` runs in the main loop only (the test counts
  its instructions per call), no audio block gets near the block's
  instruction budget, no block is silent, and the FX LED lights; the device's
  `chips` state, read from emulated RAM through its ELF symbol, shows the
  active chip flipping on each FX press with the right spare model and no lost
  reset;
- the main loop stays under 5M instructions/s (about 2 % of the core) idle and
  with a key held.

## Sanitizers

The C and C++ code can be built with sanitizers through `SIDOU_SANITIZE`:

```sh
# UBSan in trap mode: no runtime needed, works on stable Rust and macOS.
SIDOU_SANITIZE=undefined cargo test --workspace

# ASan: needs nightly Rust, which provides the ASan runtime. CI runs it on Linux.
SIDOU_SANITIZE=address RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly test --workspace -Zbuild-std --target x86_64-unknown-linux-gnu
```

Under UBSan, undefined behaviour stops the test with a trap (SIGILL or
SIGTRAP) rather than a report. Re-run the failing test in a debugger to find the
line.

## Continuous integration

`.github/workflows/ci.yml` runs on every push to `main` and every pull
request:

| Job | What it runs |
|---|---|
| `host` (Ubuntu and macOS) | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --release`, and the tests again with `SIDOU_SANITIZE=undefined` |
| `asan` (Ubuntu, nightly Rust) | the tests with `SIDOU_SANITIZE=address` |
| `msrv` (Ubuntu, Rust 1.87) | `cargo check` |
| `reuse` | `reuse lint` |
| `fm1` (Ubuntu) | the `fm1` build in Docker, the emulator gate, a check that both JSON reports exist; the reports are uploaded as the `sid-budget` artifact |

Before opening a pull request, run at least:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --release
```

and `reuse lint` (or `python3 -m reuse lint`) from the repository root.
