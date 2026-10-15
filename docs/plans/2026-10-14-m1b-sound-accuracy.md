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

**Design requirement from M1c (added 2026-10-14):** the analogue model's
parameters must later be controllable live (M1c Task 3: filter curves and
non-linearity, DAC kink and leakage, combined-waveform strength, voice DC,
external filter). So the tables and fits M1b generates must be
**parameterised**: a function of these parameters, generated over a grid,
with the device interpolating between grid points. They must not be frozen
per chip model. At minimum, keep the generator and the C data layout ready
for that, with the reSIDfp default point as one grid point. M1b's tests run
at the default point.

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
    script) and require exact equality.
  - **OSC3 test:** for single waveforms (tri, saw, pulse, noise) at a few
    frequencies, `$1B` matches reSIDfp exactly once per frame.
- Commit: `feat(sid): register reads (OSC3/ENV3) and 0xaa envelope power-on state`.

### Task 2: Envelope timing equal to reSIDfp

- Use the ENV3 test to find every place where our envelope differs. Task 1
  found that all 32 mismatches are a constant one-step phase offset:
  reSIDfp takes its first step a few cycles after a reset or state change,
  where ours waits a full rate period. Attack and decay rates already match.
  The 8580's 5.5% steeper release is *not* envelope timing (the counters fall
  at the same rate). It comes from the output path, and Task 4 owns it.
- Look at reSIDfp's `EnvelopeGenerator` for the differences against
  reSID 0.16 that we copied: the pipeline delays on state changes and on
  exponential period changes, how `counter_enabled`/freeze works, and the
  rate-counter reset behaviour.
- Implement the differences in `sid_env.h`, keeping `sid_env_advance`'s jump
  ahead (extend the per-cycle reference model in the tests to match the new
  semantics first).
- **Target:** ENV3 sequences equal to reSIDfp for every script, with all 32
  ignored cases un-ignored. Envelope shape tolerances are re-measured, but
  their targets move to Task 4.
- **Task 1 review follow-ups** (in the same task):
  - say in `sid_env.h`/`sid_init` docs that the power-on rate-counter phase
    was approximate until this fix;
  - document that OSC3 with no waveform selected returns 0, while reSIDfp
    fades the last value (keep 0 or match; decide and test);
  - list all OSC3 reconstruction edge cases in the comment (freq write in
    the last 1–2 cycles, 8580 TEST/control write in the last cycle, hard
    sync, TEST release);
  - justify `CLOCK_STEP`;
  - fix the `sid.h` "advances the same way" note for hard sync;
  - fix the stale "cycles is at most ~23" comment in `sid_osc.h`;
  - add a dense OSC3 pulse/noise script (a read every 7 cycles over a
    frame).
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

**Decisions after the Task 3 review** (2026-10-14, made by the controller
while the user was away; revisit if needed):
- **Combined-waveform "strength" is a discrete three-way profile choice**
  (reSIDfp's WEAK, AVERAGE and STRONG). These are separately sampled chips,
  not points on a scale, so they are not exposed as a continuous knob. M1c's
  analogue-parameter page lists it as a selector.
- **Flash:** only AVERAGE is compiled by default (61,440 B, within the
  64 KB target). WEAK and STRONG sit behind a build option, with the same
  `[POINTS][5][6144]` layout. If M1c needs live switching between all
  three, move to 16-entry block dedup (about 56 KB for everything) at that
  point.
- **Determinism:** the checked-in generated headers are the source of
  truth, and the generator is a documented tool. A drift-test failure on a
  new platform means a libm difference (the default 8580 table has a
  threshold gap of exactly 0), not stale tables.

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
  - the envelope shape test tightens: 6581 ≤ 0.5 dB and 8580 ≤ 0.2 dB. The
    8580 release slope is an output-path difference, investigated here
    (envelope DAC, voice DC, mixer/external filter);
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

**Results and decisions after Task 5** (2026-10-15):
- **Measurement:** noise (0xffff) at envelope 0x44 through reSIDfp's filter
  against the same noise unfiltered gives each transfer function as a cross
  spectrum (`sidou-tablegen`'s `filter_fit.rs`). The fit is the analogue
  prototype of SIDOU's filter, LP, BP and HP together, within a factor of
  three of the cutoff: reSIDfp's 6581 filter is not a state-variable filter
  further out (its band-pass and high-pass level off about 15 and 28 dB down).
- **Tables (`gen/filter.h`):** `SID_CUTOFF_HZ_<model>[curve][2048]` (float Hz)
  and `SID_RES_K_<model>[curve][16]` (k = 1/Q), one row per curve setting
  (`FILTER_CURVES` in `sidou-tablegen`; reSIDfp's defaults now, M1c Task 3
  adds rows). Between the measured FC values the cutoff follows what the
  chip's cutoff DAC outputs (the 6581's kinked DAC from reSIDfp, the 8580's
  W/L), so it is monotonic where reSIDfp is. Out of the measurement's reach
  (8580 FC 0, 6581 above about 15 kHz) it is extrapolated; the device clamps
  to 30 Hz and 0.45 fs anyway.
- **Placement:** 16,512 bytes in flash, read by `sid_init` (and `sid_set_filter_curve`) only. Each chip
  keeps its 16 damping values in its own struct (RAM), so `sid_write` reads no
  flash. Instructions per sample unchanged (worst block 690.83 for one chip,
  1,409.20 for two; 690.85 and 1,409.24 once merged with M1d's gate reset);
  `sid_init` is 54k instructions cheaper.
- **Fit errors** (dB around the cutoff, RMS / largest over the FC values in
  reach): 8580 within 0.14 / 0.87 for every mode and resonance; 6581 0.37 to
  1.45 / 1.45 to 6.6, the largest on the high-pass in its threshold region
  (FC up to about 600), where its damping drifts with FC.
- **Asserted (`tests/filter.rs`):** the sweep target is missed (8580 1.62 /
  2.02 / 1.70 dB, LP / BP / HP, largest window); a new near-cutoff transfer
  metric is within 1 dB (FC 64-512, up to 7 kHz, level-normalised per mode).
  The 6581's sweeps are 5.16 / 6.22 / 1.04. Each sweep also reports the RMS
  over its windows (8580 0.53 / 0.79 / 0.87, 6581 1.19 / 2.92 / 0.45), and
  the ceilings sit at the measured values, rounded up to 0.01 dB.
- **Why the 8580's sweeps miss 1.0 dB** (from the review; not the cutoff or
  damping tables):
  - the LP's largest window is a normalisation artefact at low FC;
  - the BP's comes from frequency warping in SIDOU's trapezoidal SVF near
    Nyquist;
  - the HP lacks reSIDfp's output low-pass;
  - the output one-pole was measured (below, Task 6's gaps): it helps LP
    and HP but worsens BP and the 6581, so it is not the fix;
  - fitting the warped response instead trades BP for LP and HP;
  - the fit errors above are against the analogue prototype, not the
    digital filter SIDOU runs.
- **Full scale (sustain 15), largest window, LP / BP / HP**, record only
  (`full_scale_filter_sweeps_*`, ceilings at the measured values):

  | Full scale | Before Task 5 | After Task 5 |
  |---|---|---|
  | 6581 | 7.1 / 5.1 / 2.1 | 4.93 / 4.64 / **6.10** |
  | 8580 | 1.3 / 1.7 / 2.8 | 0.83 / **2.04** / 1.26 |

  Two regressed: the 6581's HP, level-dependent, at FC ≥ 1,760 around the
  0.45 fs cutoff clamp (a Task 6 target), and the 8580's BP. RMS over the
  windows: 6581 1.39 / 2.35 / 2.99, 8580 0.38 / 0.69 / 0.48.
- **Passband gain** (the same transfer functions, SIDOU minus reSIDfp,
  level-matched; LP f0/8 to f0/3, BP within 1.2x of f0, HP 3 f0 to 9 f0):
  the 8580 within 0.2 dB; the 6581's filter 1.7 to 5.0 dB louder on the LP,
  -1.6 to +3.0 dB on the BP and 5.6 to 6.5 dB on the HP, more than
  `VF_TR_RATIO`'s 0.59 dB explains. Held at those ranges; a gap for Task 6.

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
  - the 6581's full-scale HP sweep back to ≤ 2.1 dB (Task 5 took it from 2.1
    to 6.10): it is level-dependent, at FC ≥ 1,760, around SIDOU's 0.45 fs
    cutoff clamp;
  - the 6581's passband gain against reSIDfp (1.7 to 6.5 dB louder after
    Task 5; `tests/filter.rs`);
  - a new loud-signal test (three voices into the filter) ≤ 3 dB;
  - the emulator gate stays ≤ 900 instructions per sample per chip.
- **Gaps carried over from Task 4** (measured against reSIDfp once the output
  level matched, single unfiltered voices within 0.12 dB):
  - *6581 volume-register DC step:* dropping the volume from 15 to 0 with
    gated sawtooths steps reSIDfp's output 2.65× as far as ours with one voice,
    1.39× with three. Our 6581 voice DC follows the envelope DAC only;
    reSIDfp's mixer adds an operating-point shift.
  - *`VF_TR_RATIO`:* reSIDfp's 6581 mixer takes unfiltered voices at 1.07×
    (+0.59 dB) the level the filter gets; we feed both paths alike.
  - *6581 mixer compression:* three voices come out 0.4 to 2.0 dB quieter in
    reSIDfp than one voice predicts (a 25% pulse most); ours add linearly.
  - *Output-stage roll-off:* reSIDfp's external filter takes up to 0.25 dB
    off bright waves near 1 kHz; we have no low-pass there. Measured after
    Task 5: a 15.92 kHz impulse-invariant one-pole after the DC blocker
    (largest window, LP / BP / HP, without -> with) moves the small-signal
    sweeps to 8580 1.62 / 2.02 / 1.70 -> 1.32 / 2.35 / 0.85 and 6581 5.15 /
    6.22 / 1.04 -> 4.91 / 7.18 / 2.19, and the full-scale ones to 8580 0.83 /
    2.04 / 1.26 -> 0.53 / 2.37 / 0.79 and 6581 4.93 / 4.64 / 6.10 -> 4.72 /
    5.63 / 7.54. It helps the 8580's LP and HP but worsens its BP and the
    6581, so it is not added; it belongs with the 6581's filter level and the
    BP's warping, not on its own.
- Commit: `feat(sid): calibrated 6581 filter non-linearity`.

### Task 7: Accuracy decision record

- Write `docs/decisions/0003-m1b-accuracy.md`. It records:
  - the before/after table (all metrics above, plus OSC3 and ENV3
    exactness);
  - flash cost of the tables;
  - instructions per sample before and after;
  - what remains off and why, including two OSC3 read gaps (read-only and
    inaudible; `#[ignore]`d tests in `register_reads.rs` pin them):
    - *OSC3 right after a hard sync:* `read_osc3` rebuilds the previous
      cycle from the current state, and the restarted accumulator has lost
      it. One read per sync differs on the 6581 (reSIDfp still reads the last
      value before the sync), two on the 8580;
    - *OSC3 a cycle after a pulse-width write:* with pulse at PW 0 and a
      write of PW 0x800 at cycle 100, reSIDfp reads 255 at cycle 101 and
      SIDOU 0, on both models: `read_osc3` ignores `pulse_held`, the
      comparison `sid_write` latched;
  - the filter misses and gaps after Task 5 (and what Task 6 changed):
    - the 8580's sweep target missed (1.62 / 2.02 / 1.70 dB), with the
      causes above; the near-cutoff transfer metric within 1 dB;
    - the full-scale regressions: the 6581's HP (2.1 -> 6.10) and the
      8580's BP (1.7 -> 2.04), record-only ceilings;
    - the 6581's passband gain, 1.7 to 6.5 dB above reSIDfp's;
    - the output one-pole, measured and not added (Task 6's gaps);
    - the RMS over windows next to each largest window;
  - the audio interrupt's flash reads: none named by the audio code
    (`ram_audio.rs` checks the relocations), but the combined waveforms'
    pulldown rows, read through a pointer, stay in flash by design (a
    hardware open check);
  - which reSIDfp defaults we matched (combined-waveform strength, 6581 filter
    range/curve). These become the default "6581 R4AR"/"8580 R5" profiles
    for M1c (`sid_set_filter_curve` selects a row of `gen/filter.h`).
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
