<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0001: C on the device, Rust for all host tooling

- **Status:** accepted
- **Date:** 2026-10-14

## Context

SIDOU's firmware runs on the FM-1's pi32v2 CPU. The only compiler for it is
JieLi's clang 4.0.1, a proprietary LLVM 4 fork. Rust needs a modern LLVM
backend for the target, and none exists. Zig's C backend produces bulky C
that runs DSP loops slower and is harder to debug. Device code therefore
has to be C (design document, section 15).

The host side needs unit tests, golden renders, a comparison with reSIDfp,
a micro-benchmark and an emulator gate that runs the real pi32v2 build in
fm1-emu. fm1-emu is a Rust library, so the gate needs Rust whatever the
rest uses.

M0 first built the host side as the plan described it: CMake presets for
the host, GoogleTest, Google Benchmark and CTest for the C core, a C++
reSIDfp harness, and Rust only for the emulator gate. That meant two test
harnesses, two build systems for host code, and C++ test code to maintain
next to the C core.

## Decision

- **Device code is C11** (`core/`, and later `app/` and `platform/fm1`),
  cross-built with CMake (preset `fm1`) through `tools/fm1-docker`. CMake is
  used for nothing else.
- **All host tooling is Rust,** in one cargo workspace (`host/`): tests,
  the benchmark, the reference comparison, the emulator gate, and later the
  table generators and packaging. One harness: `cargo test` and criterion.
- **The C core is tested through FFI.** `sidou-sys` compiles the same C
  files with the `cc` crate and generates bindings with bindgen, so the
  tests exercise the code that runs on the device.
- **reSIDfp is a test-only reference,** a git submodule compiled by `cc`.
  Our only C++ is a ~30-line `extern "C"` shim around its class API
  (`host/crates/residfp-sys/shim.cpp`). It is never linked into firmware.
- **No Python** in the build or the tests. (`reuse lint` is a third-party
  tool, run as-is.)

The C++ suite was replaced only after the Rust port was shown to be
equivalent: the same 55 tests, byte-identical golden hashes and comparison
WAV files, and identical reference measurements (recorded in commit
`c1ee4fe`).

## Consequences

- Contributors need Rust, a C/C++ compiler and libclang for the host build,
  and Docker for the firmware build. They no longer need CMake or Ninja on
  the host.
- The emulator gate compares the pi32v2 output with the host build of the
  same C code in-process, with no text output to parse.
- Host tools that M1 and later add (table generators, `.fwsc` packaging,
  `sidou-install`) go into the same workspace.
- The C code must stay plain enough for both clang 4.0.1 and current host
  compilers, and its float results must be identical on both
  (`-ffp-contract=off`, no `-ffast-math`).
- If a modern LLVM or GCC backend for pi32v2 appears, revisit the device
  language (design document, section 15).
