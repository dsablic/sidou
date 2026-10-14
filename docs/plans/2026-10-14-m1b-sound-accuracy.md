# M1b: SID sound accuracy against reSIDfp, implementation plan

<!-- REUSE-IgnoreStart -->

> **For Claude:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> to implement this plan task by task.

**Goal:** close the gaps that M0 measured between our SID core and reSIDfp,
while staying inside the cycle budget. We take as much as possible from
reSIDfp as generated, checked-in tables. Every gain is turned into an asserted
test.

**Starting point** (M0, `docs/decisions/0002-m0-sid-cost.md`):

| Area | 6581 | 8580 |
|---|---|---|
| Pitch vs theory | 0.0014 cents | 0.0014 cents |
| Envelope shape | 2.04 dB (missing DAC) | 0.44 dB (release about 5.5% steep) |
| Low-pass noise sweep | 7.0 dB | 1.3 dB |
| Band-pass noise sweep | 5.2 dB | 1.5 dB |
| High-pass noise sweep | 2.0 dB | 2.8 dB |

Other gaps:
- combined waveforms are approximated with AND;
- no waveform or envelope DAC non-linearity;
- the envelope starts at 0 (reSIDfp powers up at 0xaa);
- `$1B` (OSC3) and `$1C` (ENV3) can't be read back.

Cost: 366 instructions per sample per chip, out of a budget of 900.

**Architecture:**
- **Table generator:** a new host crate, `host/crates/sidou-tablegen`, links
  the reSIDfp engine (`residfp-sys`, extended with shim calls into its table
  builders). It writes C headers into `core/sid/gen/`. They are checked in,
  carry the GPL-2.0-or-later header (derived from reSIDfp, © its authors),
  and record how they were made. A drift test regenerates them and fails CI
  if they differ.
- **Firmware runtime:** it only does table lookups, so the device cost stays
  bounded.
- **Measured fits:** filter behaviour that reSIDfp models as a circuit (not as
  a table) is measured from reSIDfp itself by the generator, which then fits
  our SVF parameters.

**Rules:**
- The same conventions as M0 and M1a: SPDX headers, the Co-Authored-By
  trailer, stage only your own files, and host commands run from `host/` with
  the Homebrew rustup PATH.
- Tolerances only ever get tighter. Each change re-records the golden hashes
  in the same commit, with a sentence on why the sound changed. Listen to
  `sidou-compare` WAVs before and after.
- Run the emulator gate after every task. Over 900 instructions per sample is
  a failure: report it and stop.
- Work in a separate git worktree on branch `m1b-accuracy`, which is
  parallel to M1a's `m1a-device`, and rebase onto `main` before merging. M1a
  doesn't touch `core/sid`.

---

### Task 1: Register reads, and power-on state that matches the chip

- `sid_read(sid*, reg)`:
  - `$19` and `$1A` (paddles) return 0xFF for now.
  - `$1B` is OSC3: the top 8 bits of voice 3's waveform output, matching the
    combined-waveform output once Task 3 lands.
  - `$1C` is ENV3: voice 3's envelope counter.
  - All other registers return the last written byte (data bus value). Bus
    value decay over time is not modelled; note that in a comment.
- Envelope power-on value: 0xaa. That's what reSIDfp does and what the real
  chip shows ("counter is not changed on reset"). After this, remove the
  reference harness's pre-roll workaround (the 50 ms release) so both engines
  start from the same state naturally. Update comments and recorded numbers.
- Reference tests:
  - **ENV3 sequence test:** for a set of ADSR scripts (all 16 attack, decay
    and release rates, gate toggles mid-phase, an ADSR-bug case), read
    `$1C` from both engines once per frame (and once per 100 cycles for one
    script) and require exact equality. This test is what will expose the
    8580 release slope (Task 2).
  - **OSC3 test:** for single waveforms (tri, saw, pulse, noise) at a few
    frequencies, `$1B` matches reSIDfp exactly once per frame.
- Commit: `feat(sid): register reads (OSC3/ENV3) and 0xaa envelope power-on state`.

### Task 2: Envelope timing equal to reSIDfp

- Use the ENV3 test to find every place where our envelope differs, starting
  with the 8580 release being about 5.5% steep.
- Look at reSIDfp's `EnvelopeGenerator` for the differences against
  reSID 0.16 that we copied: the pipeline delays on state changes and on
  exponential period changes, how `counter_enabled`/freeze works, and the
  rate-counter reset behaviour.
- Implement the differences in `sid_env.h`, keeping `sid_env_advance`'s jump
  ahead (extend the per-cycle reference model in the tests to match the new
  semantics first).
- **Target:** ENV3 sequences equal to reSIDfp for every script. The envelope
  shape test then tightens (8580 below 0.2 dB, and the 6581 bound is
  re-measured after Task 4).
- Commit: `fix(sid): envelope timing matches reSIDfp (ENV3 bit-exact)`.

### Task 3: Combined waveforms and noise write-back

- In `residfp-sys/shim.cpp`, add `residfp_pulldown_table(model, cws, out)`,
  which copies `WaveformCalculator::buildPulldownTable(model, AVERAGE)` (or
  the variant our default profile uses) into a flat buffer. Also expose
  `getWaveTable()` if needed.
- `sidou-tablegen` writes `core/sid/gen/wave_6581.h` and `wave_8580.h`:
  4096-entry tables for each combination (ST, P_T, PS_, PST), with `uint16_t`
  or packed 12-bit storage. Report the size; target ≤ 64 KB of flash for all
  of them.
- `sid_osc_output` uses the tables for combinations. Single waveforms stay
  computed. Add noise write-back when noise is combined with other waveforms
  (reSIDfp's LFSR write-back behaviour), with tests.
- Tests:
  - OSC3 equality with reSIDfp for every combination at several
    frequencies;
  - the drift test (tablegen output equals the checked-in headers).
- Commit: `feat(sid): combined waveforms and noise write-back from reSIDfp tables`.

### Task 4: DACs (waveform and envelope)

- Shim: `residfp_dac_table(model, bits, out)` using `Dac::kinkedDac(model)`,
  for the 12-bit waveform DAC and the 8-bit envelope DAC.
- `sidou-tablegen` writes `gen/dac_6581.h` and `gen/dac_8580.h` as float
  tables, or Q15 if the pi32v2 benchmark prefers it.
- The voice output becomes `dac_wave[w] * dac_env[e]`, plus the model's voice
  DC, replacing `(w - wave_zero) * e + voice_dc`. Keep the scaling so peak
  levels stay similar, and check against reSIDfp's mixer output level.
- Tests:
  - DAC tables against reSIDfp's;
  - the 6581 envelope shape test tightens (target ≤ 0.5 dB);
  - the golden hashes are re-recorded.
- Commit: `feat(sid): kinked waveform and envelope DACs from reSIDfp`.

### Task 5: Filter cutoff and resonance fitted to reSIDfp

- **Measure:** `sidou-tablegen fit-filter` drives reSIDfp per model with:
  - small-signal sine sweeps or white noise through LP, BP and HP;
  - FC on a grid (every 32 steps plus the 6581's 1023/1024 discontinuity);
  - resonance 0, 8 and 15.

  It estimates the effective cutoff (−3 dB point, or the BP peak) and the Q.
- **Fit:**
  - a 2048-entry cutoff table in Hz per model, interpolated between
    measured points and monotonic where reSIDfp is;
  - a 16-entry resonance-to-k table.

  These replace `sid_curves_resid016.h`, which then goes along with its
  attribution entry.
- **Assert:** turn `sidou-compare`'s scenarios into tests with tolerances,
  using small-signal levels where our linear SVF should match:
  - 8580: target ≤ 1.0 dB on LP, BP and HP;
  - 6581: record whatever the linear fit gives; Task 6 handles the level
    dependence.
- Commit: `feat(sid): filter cutoff and resonance tables fitted to reSIDfp`.

### Task 6: 6581 filter non-linearity

- **Measure in reSIDfp:** how the 6581's effective cutoff and gain change
  with input level and DC bias (sweeps at several levels and voice counts).
- **Extend our 6581 stage** within budget. Options, cheapest first:
  1. calibrate the soft-clip drive and where it applies;
  2. add a level-dependent cutoff shift, `g_eff = g × (1 + α·|state|)`,
     using a precomputed `a1(g)` approximation rather than a per-sample
     divide;
  3. a small 2D table `g_eff[fc][level]`.

  Pick the cheapest that meets the target, and report the trade-offs with
  numbers.
- **Target:**
  - 6581 LP and BP noise sweeps ≤ 2.0 dB at the M0 workload levels;
  - a new loud-signal test (three voices into the filter) ≤ 3 dB;
  - the emulator gate stays ≤ 900 instructions per sample per chip.
- Commit: `feat(sid): calibrated 6581 filter non-linearity`.

### Task 7: Accuracy decision record

- Write `docs/decisions/0003-m1b-accuracy.md`. It records:
  - the before/after table (all metrics above, plus OSC3 and ENV3
    exactness);
  - flash cost of the tables;
  - instructions per sample before and after;
  - what remains off and why;
  - which reSIDfp defaults we matched (combined-waveform strength, 6581 filter
    range/curve). These become the default "6581 R4AR"/"8580 R5" profiles
    for M1c.
- Update `LICENSING.md` (generated tables are GPL-2.0-or-later, derived from
  reSIDfp; reSID 0.16 points removed), `CREDITS.md`, `CHANGELOG.md` and
  `TESTING.md` (the new tests and the drift check).
- Commit: `docs: M1b accuracy results`.

## Done when
- OSC3 and ENV3 are bit-exact against reSIDfp for the test scripts on both
  models.
- The asserted tolerances meet the targets above (or the decision record
  explains each miss).
- The tables are generated, checked in and drift-checked in CI.
- The emulator gate is ≤ 900 instructions per sample per chip and still
  bit-exact between host and target.
- All CI jobs are green.

<!-- REUSE-IgnoreEnd -->
