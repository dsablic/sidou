<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]
### Added
- Design document, licensing, credits and contribution guidelines.
- SID core in C11 (`core/sid/`): oscillators, noise, hard sync and ring
  modulation, envelope generator with the ADSR delay bug, a
  zero-delay-feedback state-variable filter with reSID 0.16 cutoff curves,
  mixer and DC blocker, for the 6581 and the 8580.
- Rust host workspace (`host/`): FFI to the C core, unit and golden-render
  tests, and a criterion benchmark.
- reSIDfp reference harness: pitch and envelope tests, and the
  `sidou-compare` filter comparison tool.
- Pinned JieLi pi32v2 toolchain in Docker, and a bare-metal pi32v2 benchmark
  program.
- Emulator benchmark gate: runs the pi32v2 build in fm1-emu, enforces 900
  instructions per sample per chip, and checks the output against the host
  build.
- CI: host tests on Linux and macOS with UBSan; ASan on Linux; MSRV check, REUSE lint,
  and the firmware build with the emulator gate.
- Building and testing guides, and decision records for the host tooling
  and the M0 SID cost.
- FM-1 platform (`platform/fm1/`): Felucca's HAL (GPL-3.0-only), startup
  code and memory map, and a firmware image that boots, runs the audio
  interrupt and plays a SID test tone; an fm1-emu boot test checks it.
- The firmware plays the live instrument from the FM-1's keys, K1-K4, MASTER,
  OCT-/OCT+ and FX (6581/8580), lights the held keys and the octave, and shows
  a status screen on the LCD. Register writes reach the audio interrupt
  through a ring that never drops; a model change swaps to a chip prepared in
  the main loop. fm1-emu tests play it end to end.
- MIDI in over the TRS jack and USB (Felucca's USB stack, adapted: USB-MIDI
  only): notes on every channel, CC 74/71/1/7, program change; a lock-free
  ring from the 2 kHz polls to the main loop, one parser per source, exact
  recovery from lost input.
- The audio path (the ALNK0 interrupt, the render, the register drain) runs
  from SRAM; tests check that the interrupt runs nowhere else and that no
  call crosses between flash and SRAM code.
- The community SysEx update protocol, step 1 (Felucca's `ota.c`), and
  Felucca's update loader, unmodified and byte-identical to Felucca's own
  build; a boot-loop guard that sends a third early death in a row to UBOOT.
- `sidou-pack`: the `.fwsc` packager (a port of Felucca's), byte-identical to
  Felucca's packer on the same inputs; CI builds `sidou.fwsc`, boots it in
  fm1-emu and uploads it.
- The M1a hardware checklist (`docs/hardware-test.md`) and checklist builds
  that die early on purpose (`SIDOU_TEST_DEATH`).
