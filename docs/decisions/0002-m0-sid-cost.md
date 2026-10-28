<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0002: M0 SID cost: go

- **Status:** accepted
- **Date:** 2026-10-14

## Context

M0 asks one question: does one SID, including the 6581 filter, cost no more
than 900 pi32v2 instructions per output sample on the FM-1? That is the
per-chip budget in section 8 of the design document. At 240 MHz and
44.1 kHz, one core has about 5,442 cycles per sample.

The M0 core (`core/sid/`) has the oscillators, noise, hard sync and ring
modulation, the envelope generator with the ADSR delay bug, a
zero-delay-feedback state-variable filter with reSID 0.16 cutoff curves, the
mixer and a DC blocker. Out of M0's scope, and so not in these numbers:
combined-waveform tables (M0 uses AND, which costs the same), the 6581
envelope DAC, chip profiles and the character layer.

**How it was measured:**
- **Target:** the shared workload (`bench/workload.c`: a C major triad
  through a resonant low-pass, with a cutoff change, a switch to noise and
  gate-off; 11,025 samples at 44.1 kHz) on a 6581, built for pi32v2 with
  the pinned JieLi clang 4.0.1 at `-O2 -mcpu=r3 -mfprev1`. It runs in
  fm1-emu at `e52f4640`, which counts the instructions between
  `bench_mark()` calls (`host/crates/emu-gate`, see
  [TESTING.md](../../TESTING.md)).
- **Host:** criterion, on an Apple M1 Max; `cc` builds the C code at
  `-O3` for the bench profile.
- **Reference:** `cargo test --release` in `host/`, which runs the reSIDfp
  tests and `sidou-compare`.

The numbers below come from `build/fm1/sid_budget_{1,2}.json` and test
output at commit `4a0cf71`.

## Results

### Cost on the pi32v2

| | 1 chip | 2 chips |
|---|---|---|
| Instructions per sample | 366.2 | 740.5 |
| Share of the per-chip budget (900 per chip) | 41 % | 41 % |
| Share of one 240 MHz core (at 1 instruction per cycle) | 6.73 % | 13.61 % |
| Init (`sid_init` for every chip) | 194,295 instructions | 388,572 instructions |
| Largest sample difference, target vs host build | 0 | 0 |

- **Instructions are a lower bound on cycles.** The pi32v2 mostly issues
  one instruction per cycle, but code fetched from flash (XIP) and divides
  take longer. Cycles still have to be measured on the device.
- **The FPU is used in hardware.** The toolchain file passes `-mcpu=r3
  -mfprev1`. Without `-mcpu=r3`, clang turns every float operation into a
  soft-float library call; with it, the image calls no float helpers at all.
- **Target and host agree bit for bit,** so host tests are valid for the
  device build. Both compile with `-ffp-contract=off`.
- Init takes under 1 ms per chip at 240 MHz, most of it in building the cutoff
  table.

### Host benchmark

Samples per second for each chip, from `cargo bench -p sidou-bench`:

| | 6581 | 8580 |
|---|---|---|
| 1 chip | 56.4 M | 59.5 M |
| 2 chips | 54.1 M | 58.9 M |

Runs vary by a few percent; an earlier run gave 54.6 M and 57.2 M for one
chip. That is over 1,000 times real time, so host tests and tools are not
limited by the core.

### Distance from reSIDfp

| Test | 6581 | 8580 | Tolerance |
|---|---|---|---|
| Sawtooth pitch, SIDOU vs theory (cents) | 0.0014 | 0.0014 | 0.1 |
| Sawtooth pitch, SIDOU vs reSIDfp (cents) | 0.0014 | 0.0014 | 0.1 |
| Envelope shape, largest difference down to -20 dB | 2.04 dB | 0.44 dB | 2.5 dB (6581), 0.75 dB (8580) |

`sidou-compare`, the largest envelope difference on filtered noise sweeps
(informational, no threshold):

| Sweep | 6581 | 8580 |
|---|---|---|
| Low-pass | 7.0 dB | 1.3 dB |
| Band-pass | 5.2 dB | 1.5 dB |
| High-pass | 2.0 dB | 2.8 dB |

Known differences, all for M1:
- **6581 envelope:** reSIDfp models the 6581's non-linear envelope DAC; M0
  does not, so the release drifts about 2 dB apart by -20 dB.
- **8580 release:** ours falls about 5.5 % faster than reSIDfp's (55.9 vs
  53.0 dB/s). It is 1.8 dB apart by -30 dB, below the test's floor.
- **Power-on state:** reSIDfp, like the real chip, starts each envelope
  counter at 0xaa; SIDOU starts at 0. The test harness releases reSIDfp's
  envelopes before measuring.
- **6581 filter:** the low- and band-pass sweeps are furthest apart. M0
  uses reSID 0.16 cutoff points and a saturation stand-in, not a model of
  the 6581's filter.

### Emulator gap

fm1-emu at `e52f4640` decodes but cannot execute `{R*4} = [-R]`, which clang
emits when loading cutoff curve points. The gate steps that instruction
itself (2,048 times per chip, all in `sid_init`), and the bit-exact host
match confirms the semantics. It is to be reported upstream
([https://github.com/dsablic/sidou/blob/v0.5.0/docs/TODO.md](https://github.com/dsablic/sidou/blob/v0.5.0/docs/TODO.md)).

## Decision

**Go.** One chip costs 366 instructions per sample, 41 % of its budget, and
two chips scale linearly. M1 continues with this core, with the hardware
FPU, and without any of the cuts in section 8 of the design document.

## Consequences

- The headroom (about 530 instructions per chip) is for what M0 left out:
  combined-waveform tables, the 6581 DAC and filter model, chip profiles
  and the character layer. The emulator gate keeps the 900-instruction
  limit, so overruns show up in CI.
- Instruction counts are not cycles. Before M1's budget is final, measure
  cycles on hardware or with a cycle-accurate fm1-emu, especially for code
  run from flash.
- The reference tolerances stay where they are; M1 tightens them as it
  closes the gaps above (6581 DAC, 8580 release timing).
- If the emulator learns `{R*4} = [-R]`, bump the pinned fm1-emu revision and
  drop the workaround.
