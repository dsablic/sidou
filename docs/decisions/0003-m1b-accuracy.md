<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0003: M1b SID accuracy against reSIDfp: results

- **Status:** accepted
- **Date:** 2026-10-15

## Context

M1b ([plan](../plans/2026-10-14-m1b-sound-accuracy.md)) closes the gaps that
M0 measured between SIDOU's SID core and reSIDfp
([decision record 0002](0002-m0-sid-cost.md)). It must stay inside the
900 instructions per sample per chip budget. What reSIDfp builds as tables
(combined waveforms, filter cutoff and damping) comes from generated,
checked-in headers in `core/sid/gen/`. reSIDfp's R-2R DAC ladders are
ported (`core/sid/sid_dac.c`). What reSIDfp models as a circuit (the 6581
filter's non-linearity) is measured from reSIDfp and fitted.

This record gives the results, what they cost, what is still off and why,
and which reSIDfp defaults SIDOU matches.

**How it was measured:** on `m1b-accuracy` at `02c4882` (equal to `main`),
on an Apple M1 Max, from `host/`:
- `cargo test --release -p sidou-tests --test reference`, `--test filter`
  and `--test register_reads`, with `-- --nocapture`;
- the emulator gate (`cargo test --release -p emu-gate --test sid_budget
  --test fx_budget --test slots_budget --test ram_audio`) with
  `SIDOU_FM1_BUILD` set to a fresh `fm1` build. It runs in fm1-emu at
  `e52f4640` with the pinned JieLi clang 4.0.1 (`-O2 -mcpu=r3 -mfprev1`),
  and its numbers come from `build/fm1/*_budget_*.json`;
- table sizes from `nm -S` on `build/fm1/platform/fm1/sidou_app.elf`.

"M0" is M0's model, as 0002 measured it. Some metrics did not exist at M0.
For those, the "before" column gives the first measurement and names the
task that made it.

## Results

### Accuracy, before and after

The sweep figures are the largest envelope difference over the sweep's
windows, LP / BP / HP, with the RMS over the windows in brackets. The
small-signal level is one voice of noise at envelope 0x44, the level the
filter tables were fitted at. Full scale is sustain 15. "Loud" is three
voices at full sustain into the 6581's filter.

| Metric | Model | Before | After M1b | Target | Status |
|---|---|---|---|---|---|
| ENV3 (`$1C`) against reSIDfp | both | not readable; counters started at 0, not 0xaa | bit-exact in all 108 ENV3 tests (both models): every attack, decay and release rate, gate changes mid-phase, the ADSR delay bug, sustain changes, the power-on release; read once a frame and every 100 cycles | exact | met |
| OSC3 (`$1B`) against reSIDfp | both | not readable | bit-exact in 97 OSC3 tests (both models; every single and combined waveform at four frequencies, noise through TEST, lock-up, ring modulation; read once a frame and every 7 cycles) and in 1,260 adversarial deferral scripts; two cases differ, see "What remains off" | exact | met, two gaps pinned |
| Combined waveforms | both | the AND of the selected waveforms | reSIDfp's pulldown tables (AVERAGE), with the noise register's write-back and lock-up; OSC3 exact for every combination | reSIDfp's | met |
| Sawtooth pitch, against theory (cents) | both | 0.0014 | 0.0002 (0.0002 against reSIDfp) | 0.1 | met |
| Envelope shape down to -20 dB, against reSIDfp | 6581 | 2.04 dB (no DAC) | 0.311 dB | 0.5 dB | met |
| | 8580 | 0.44 dB (release 55.9 against 53.0 dB/s) | 0.030 dB (release 53.2 against 52.8 dB/s) | 0.2 dB | met |
| DACs | both | linear: `(w - wave_zero) * e + voice_dc` | reSIDfp's R-2R ladders (`Dac::kinkedDac`): the 6581's kinked, with leakage; the voice DC follows the envelope DAC | reSIDfp's | met |
| Level of one unfiltered voice against reSIDfp (saw, triangle, pulse, at A4 and A2) | 6581 | not matched (Task 4 raised the output 3.37 dB) | -0.122 to -0.012 dB | 0.2 dB | met |
| | 8580 | not matched (raised 1.05 dB) | -0.096 to +0.091 dB | 0.2 dB | met |
| Small-signal filter sweeps | 6581 | 4.88 / 5.22 / 5.29 (M0's filter, measured before Task 5) | 4.83 / 2.45 / 1.01 (1.07 / 0.67 / 0.42) | record (Task 5) | recorded; LP explained below |
| | 8580 | 2.23 / 1.72 / 3.27 (M0's filter, measured before Task 5) | 1.62 / 2.02 / 1.70 (0.53 / 0.79 / 0.87) | 1.0 dB | **missed**, explained below |
| Full-scale filter sweeps | 6581 | 7.0 / 5.2 / 2.0 | 1.88 / 1.88 / 1.49 (0.52 / 0.57 / 0.84) | LP, BP 2.0; HP 2.1 | met |
| | 8580 | 1.3 / 1.5 / 2.8 | 0.83 / 2.04 / 1.26 (0.38 / 0.69 / 0.48) | record only | the BP regressed, explained below |
| Loud filter sweeps, three noise voices | 6581 | 9.38 / 7.80 / 14.93 (Task 5's model) | 2.44 / 1.91 / 2.81 (0.95 / 0.74 / 1.13) | 3 dB | met |
| Loud filter sweeps, a chord | 6581 | 16.47 / 14.85 / 23.33 (Task 5's model) | 2.32 / **4.37** / 2.83 (0.72 / 3.48 / 1.59) | 3 dB | BP **missed**, explained below |
| Passband gain, SIDOU minus reSIDfp (LP f0/8 to f0/3, BP within 1.2x of f0, HP 3 f0 to 9 f0) | 6581 | +1.66 to +4.99 / -1.61 to +3.04 / +5.63 to +6.47 dB (Task 5) | -1.74 to +1.78 / -2.01 to +2.05 / -0.39 to +0.51 dB | reSIDfp's level | met within about 2 dB |
| | 8580 | -0.11 to 0.00 / -0.17 to -0.05 / -0.05 to +0.03 dB (Task 5) | -0.11 to 0.00 / -0.17 to -0.05 / -0.05 to +0.03 dB | 0.2 dB | met |
| Near-cutoff transfer function, largest error (FC 64 to 512, resonance 0, 8 and 15, half to twice the cutoff, up to 7 kHz) | 6581 | 3.34 / 3.69 / 3.45 dB (Task 5) | 3.31 / 3.12 / 2.88 dB | record | recorded |
| | 8580 | 0.97 / 0.49 / 0.22 dB (Task 5) | 0.97 / 0.49 / 0.22 dB | 1.0 / 0.55 / 0.3 dB | met |

Notes on the table:
- **Full-scale M0 figures.** They are 0002's `sidou-compare` figures. Just
  before Task 5, after Tasks 1-4 had changed the voices, the same sweeps
  gave 7.1 / 5.1 / 2.1 (6581) and 1.3 / 1.7 / 2.8 (8580).
- **The 8580's filter at Task 6.** Task 6 changed only the 6581. The
  8580's render is bit for bit what Task 5 left (golden hash unchanged).
- **Filter fit errors.** `sidou-tablegen fit-filter` reports how well the
  fit matches reSIDfp's analogue response around the cutoff (RMS / largest).
  The 8580 is within 0.14 / 0.87 dB for every mode and resonance. The
  6581 is 0.37 to 1.45 / 1.45 to 6.6 dB, worst on the high-pass in its
  threshold region (FC up to about 600), where its damping drifts with FC.
- **Every ceiling is the measured value,** rounded up to 0.01 dB
  (`tests/filter.rs`, `tests/reference.rs`). Tolerances only get tighter.

### Costs

**Flash** (the firmware image, `.rodata`):

| Table | Bytes | Read by |
|---|---|---|
| Combined-waveform pulldown, `gen/wave_6581.h` + `wave_8580.h` (AVERAGE; 5 rows of 4,096 packed 12-bit entries per model; since M2, `gen/pulldown.h`'s shared blocks: see below) | 61,440 (184,320 with `SIDOU_ALL_COMBINED_PROFILES`) | the oscillators, in the audio interrupt, through `sid_osc.pulldown` |
| Filter cutoff and damping, `gen/filter.h` (`SID_CUTOFF_HZ_*` 2 x 8,192, `SID_RES_K_*` 2 x 64; one curve row per model) | 16,512 | `sid_init` and `sid_set_filter_curve` only |
| **Total** | **77,952** | 64 KB was the pulldown tables' target |

M0's reSID 0.16 cutoff points (`sid_curves_resid016.h`) are gone.

**Since M2: the pulldown rows share 16-entry blocks** (`gen/pulldown.h`, `SID_PD`). Most of
a row repeats elsewhere: the NP row is the same in all three 6581 profiles, and long runs
of zeros and identities recur across rows, profiles and models. sidou-tablegen splits
every compiled row into 256 blocks of 16 entries, stores each distinct block once (24
bytes, packed as before), and gives each row an index of 256 u16 offsets from the row to
its blocks. It is lossless: sidou-tablegen's `shared_pulldown_blocks_read_back_as_residfps_tables`
reads every row of every profile and model back against reSIDfp's tables, and emu-gate's
`character.rs` does the same on the firmware's own `SID_PD`. The device's set (the 6581's
three profiles and the 8580's AVERAGE, 122,880 bytes as packed rows) takes 39,256 bytes:
20 rows of index and 1,209 blocks. The image shrank from 493,628 to 410,260 bytes.

The lookup (`sid_pulldown`) reads the row's index entry first, a second flash load and
three more instructions: +12.0 instructions a sample per chip with three voices on
pulse+saw, +8.0 in the combined scenario. The worst blocks, combined noise, call it
rarely and did not rise. `.ram_text` grew 160 bytes, as the lookup is inlined at five
places.

**RAM:**

| What | Bytes | Where |
|---|---|---|
| Voice DAC tables, one per model, shared by every chip (`sid_dac_default`) | 34,816 (2 x 17,408) | POOL, built at the first `sid_init` per model |
| `SID_NOISE_JUMP` (`gen/noise_jump.h`), `SID_NOISE_WB`, `SID_ENV_RATE_POS`, `SID_ENV_THRESHOLDS` | 2,116 + 128 + 32 + 32 = 2,308 | `.data`: the audio path reads them, so they are not in flash |
| Per chip: `res_k[16]` | 64 | in `sid`, so `sid_write` reads no flash |

Each chip keeps its 2,048-entry `cutoff_g` table (8,192 bytes), as at M0.
It is now filled from `gen/filter.h` instead of computed.

**Instructions per sample** (pi32v2, fm1-emu; lower bounds on cycles):

| | M0 | After M1b |
|---|---|---|
| Triad, 6581, one chip (average) | 366.2 | 400.50 |
| Triad, 6581, two chips | 740.5 | 808.88 (404.44 a chip) |
| Triad, 8580, one / two chips | not measured | 347.37 / 702.67 |
| Most expensive scenario, one chip (average) | the triad was the only scenario | 552.23 (6581, `gate_every_block`); 8580 508.88 |
| Most expensive scenario, two chips | | 1,112.61 (556.30 a chip; bound `SID_WORST_PER_CHIP` 557); 8580 1,025.88 |
| Worst 128-sample block, one chip | not measured (blocks were first counted at Task 2) | 717.49 (6581, `noise_tri_ffff`; bound 718); 8580 691.16 |
| Worst 128-sample block, two chips | | 1,439.67 (6581, `noise_tri_ring_0800`; bound 1,440); 8580 1,409.87 |
| `sid_init`, 6581, one / two chips (instructions) | 194,295 / 388,572 | 241,053 / 388,454 (the first per model builds the shared DAC table; the cutoff table is no longer computed) |
| Largest sample difference, target against host | 0 | 0, in every scenario on both models |

The worst blocks are power-on transients of noise combined with another
waveform: the noise register shifts until it locks up, in a scenario's
first block. Since Task 6 they are the 6581's, whose non-linear filter
costs about 38 instructions per sample per chip. The 6581's per-render
setup (`sid_nl_coefs`) and the tap's single render a block keep that
figure down.

**The gates with the LIVE view's tap.** LIVE is the view at power-on. Its
tap adds 4.44 instructions a sample (`ram_audio.rs`, a chord held: 406.71
with the tap, 402.27 without), against a bound of 4.5 (`TAP_PER_SAMPLE`).
- Effects: 1,440 + 4.5 + 455.19 (the worst effects case) = 1,899.69 <=
  2,300.
- Slots: 1,440 + 4.5 + 838.97 (every slot's heaviest) = 2,283.47 <= 2,300,
  with 16.53 to spare.
- Audio target: 1,440 + 4.5 + 100 (ISR) + 964.28 (the worst switching
  block) = 2,508.78 <= 2,700.

**The 6502's headroom is 191 instructions per sample.** It was 227 before
Task 6's 6581 filter and without the tap; the old budget reserved 400.
[docs/TODO.md](../TODO.md) lists the ways back if M3's 6502 does not fit.

## What remains off, and why

**OSC3 read gaps.** Both are read-only and inaudible. `#[ignore]`d tests
in `register_reads.rs` pin them, and both still fail as described:
- *OSC3 right after a hard sync* (`hard_sync_while_deferring_reads_osc3_exactly`).
  `read_osc3` rebuilds the previous cycle from the current state, and the
  restarted accumulator has lost it. One read per sync differs on the 6581,
  where reSIDfp still reads the last value before the sync, and two on the
  8580: 289 and 578 of a script's 98,525 reads.
- *OSC3 a cycle after a pulse-width write* (`pulse_width_write_reads_osc3_exactly`).
  With pulse at PW 0 and a write of PW 0x800 at cycle 100, reSIDfp reads
  0xff at cycle 101 and SIDOU 0x00, on both models. `read_osc3` ignores
  `pulse_held`, the comparison `sid_write` latched.

**Deliberate differences.** Tests pin each one:
- with no waveform selected, OSC3 reads 0 where reSIDfp holds the last
  value and fades it;
- the paddles (`$19`, `$1A`) read 0xff;
- the data bus value read back from write-only registers does not decay;
- the gate reset (`sid_set_gate_reset`, M1d) departs from the chip on
  purpose. It is off in the core by default, which is bit-exact with
  reSIDfp. The live instrument turns it on (ENVELOPE: INSTANT).

**The 8580's small-signal sweeps miss 1.0 dB** (1.62 / 2.02 / 1.70). The
near-cutoff transfer functions are within 1 dB, so the cutoff and damping
tables are not the cause:
- the low-pass's largest window is a normalisation artefact at low FC;
- the band-pass's comes from frequency warping in SIDOU's trapezoidal
  filter near Nyquist;
- the high-pass lacks reSIDfp's output low-pass.

An output one-pole at 15.92 kHz was measured. It helps the 8580's LP and
HP (1.32 / 0.85) but worsens its BP (2.35) and the 6581, so it was not
added. Fitting the warped response instead trades BP for LP and HP.

**The 8580's full-scale band-pass regressed:** 1.5 at M0 and 1.7 just
before Task 5, now 2.04. This is the same band-pass warping, with the
small-signal fit at full scale. It is record-only, with its ceiling at
the measured value.

**The 6581's loud chord band-pass is 4.37 dB** (target 3). With the
cutoff above the chord, reSIDfp's band-pass stays louder than SIDOU's.
reSIDfp's op-amps saturate, and SIDOU's lower bound on the load term
(`SID_NL_M_MIN`, 0.35) stands in for that. It is what keeps three voices
at full resonance under full scale (the headroom test, -0.53 dB). 0.3
clipped. Damping that also follows the load gave three loud misses instead
of one, so it was not taken.

**The 6581 volume register's DC step and its mixer's compression are
deferred together.** In reSIDfp, dropping the volume from 15 to 0 with
gated sawtooths steps the output 2.65x as far as SIDOU's with one voice,
and 1.39x with three. In reSIDfp, three voices come out 0.4 to 2.0 dB
quieter than one voice predicts. SIDOU's voices add linearly. Per-input
mixer DC matched reSIDfp's steps within 10%, but without the compression
three loud voices then clipped (the headroom test). Neither goes in
without the other.

**Output high-pass: 1.6 Hz in reSIDfp, 16 Hz in SIDOU.** SIDOU's DC
blocker is at 16 Hz, and the DC tests rely on it. reSIDfp's output still
carries the volume write's DC step in the sweep's early windows. That is
why the 6581's small-signal low-pass sweep stays at 4.83 (its fourth
window).

**Two emulator checks were restated, not loosened.** Task 6 put the
6581's filter at reSIDfp's level, 3.3 dB lower than before, and loosened
two checks. After review, both were restated (`fd37bec`):
- *Overdrive* (`fx_live.rs`): the crest factor is compared with the slot on
  at no drive, with the same tone and mix, against the original 0.8. It is
  played on a chord (A4, C#5, E5), about as loud as one note was before:
  3.14 -> 1.32.
- *Latency* (`latency.rs`): "the note is silent" is now half the held
  note's peak on the host core (SAW LEAD, the 6581, C4 at velocity 100:
  2,505), not an absolute number.

**The RMS over the windows** is reported and bounded next to every largest
window (the table above). A single bad window does not hide a sweep that is
off throughout, and the reverse.

**Flash reads in the audio interrupt.** The audio code names no flash data:
`ram_audio.rs` checks that no address relocation in `.ram_text` points into
XIP (17 address loads, none into flash). The combined waveforms' pulldown
rows (61,440 bytes; since M2 `gen/pulldown.h`'s shared blocks, 39,256 bytes on the device)
stay in flash by design and are read through a pointer,
which the check cannot see. What they cost on the device is a hardware open
check ([docs/hardware.md](../hardware.md), "Combined waveforms from flash").
If they cost too much, the default profile's rows move to POOL.

## reSIDfp defaults SIDOU matches

These defaults are M1c's default profiles, **"6581 R4AR"** and **"8580
R5"**:

| Parameter | 6581 | 8580 | SIDOU's control |
|---|---|---|---|
| Combined-waveform strength | AVERAGE | AVERAGE | `sid_set_combined_profile` (WEAK and STRONG behind `SIDOU_ALL_COMBINED_PROFILES`); a discrete choice, not a scale |
| DAC 2R/R ratio and leakage | 2.20, 0.0075 | 2.00, 0.0035 | `sid_set_dac` with a table from `sid_dac_build` |
| Filter curve | `setFilter6581Curve(0.5)`, range at reSIDfp's default (uCox 20e-6) | `setFilter8580Curve(0.5)` | `sid_set_filter_curve` selects a row of `gen/filter.h`; row 0, the default, is the only row today |
| 6581 filter non-linearity | reSIDfp's (amount 1) | none (ignored) | `sid_set_filter_nonlinearity` sets the amount, 0 (linear) to 2 |
| Voice DC | follows the envelope DAC, 0.2143 V per unit | constant; the DC blocker removes it | none yet (M1c) |

M1c Task 3 adds the other profiles (6581 R2, R3, "worn"). It does this
with more rows in `gen/filter.h` (`FILTER_CURVES` in `sidou-tablegen`),
more DAC tables and other non-linearity amounts. reSIDfp's default stays
one grid point among them.

## The plan's "Done when"

| Item | Status |
|---|---|
| OSC3 and ENV3 bit-exact against reSIDfp for the test scripts, on both models | **Met.** 108 ENV3 and 97 OSC3 tests, and 1,260 deferral scripts, all exact. Two OSC3 cases are `#[ignore]`d, explained above: a read right after a hard sync and a cycle after a pulse-width write. |
| The asserted tolerances meet the targets, or this record explains each miss | **Met.** Missed: the 8580's small-signal sweeps (1.0 dB) and the 6581's loud chord band-pass (3 dB); regressed: the 8580's full-scale band-pass (record only). Each is explained above. Everything else meets its target. |
| The tables are generated, checked in and drift-checked in CI | **Met.** `core/sid/gen/` is generated by `sidou-tablegen` and checked in. `tests/drift.rs` checks the computed tables in every host job. The ignored filter-table check (`checked_in_filter_tables_match_the_measurement`) re-measures reSIDfp in the Linux host job; it passes locally. |
| The emulator gate is <= 900 instructions per sample per chip and bit-exact between host and target | **Met.** Worst average 556.30 a chip, worst block 717.49 (one chip) and 1,439.67 (two). No sample differs from the host build in any scenario. |
| All CI jobs are green | **Pending a push.** CI is paused at the user's request until the work is done. `cargo test --locked --workspace --release`, the emulator gate with a fresh `fm1` build, the filter-table drift check and `reuse lint` all pass locally. CI runs them when the branch is pushed. |

## Decision

M1b is done. SIDOU's core matches reSIDfp:
- bit for bit on the register reads, except two pinned OSC3 cases;
- within 0.31 dB on envelopes and 0.12 dB on the level;
- within about 2 dB on the filter almost everywhere.

The worst average is 556 instructions per sample per chip, against a
budget of 900. The misses above stay open with their reasons, and the
tests hold their ceilings at the measured values.

## Consequences

- M1c builds its chip profiles on these controls. The default profiles
  are reSIDfp's defaults, so the reference tests keep running at them.
- The volume DC step and the mixer's compression go in together, or not
  at all. The 8580's band-pass warping and the output low-pass belong
  with them, as one output-path change measured against every sweep.
- The 6502 (M3) has 191 instructions per sample of audio headroom, not
  400. [docs/TODO.md](../TODO.md) has the plan if it does not fit.
- Hardware must confirm what the emulator cannot measure: cycles against
  instructions, and the cost of the pulldown rows' flash reads.
