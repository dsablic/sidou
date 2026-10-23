<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0006: Chip profiles and in-chip character (M1c Task 3)

**Status:** accepted, 2026-10-15.

## Context

Design doc section 4 puts the SID's analogue model under the player's control: five named
chip profiles (6581 R2, 6581 R3, 6581 R4AR, 8580 R5 and a "worn 6581") and four live
controls (age/heat, DAC sloppiness, output thump, filter bleed). Every change blends
precomputed tables or parameters in the main loop; nothing is rebuilt on the audio thread.
At zero character each profile is to match reSIDfp at that chip's settings.

## What reSIDfp offers

reSIDfp (libresidfp c1791383, the `third_party/libresidfp` submodule) exposes:

| Setting | Range | What it is |
|---|---|---|
| `setFilter6581Curve` | 0..1, 0.5 default | shifts the 6581 cutoff DAC's zero (`FilterModelConfig6581::getDAC`); 0 dark, 1 bright |
| `setFilter6581Range` | 0..1, never set by default (uCox 20 uA/V², r = 19/39) | the VCRs' uCox, (1 + 39 r) uA/V²; **once per process**: every 6581 follows it |
| `setFilter8580Curve` | 0..1, 0.5 default | the integrators' gate voltage, 1.8 - 0.6 curve. Its header says 0 is dark; measured, the 8580's cutoff **rises** as the curve falls (FC 1024: 7.1 kHz at 0.5, 10.4 kHz at 0.25) |
| `setCombinedWaveforms` | AVERAGE, WEAK, STRONG | three sampled chips per model (`WaveformCalculator.cpp`): AVERAGE a 6581 R3 0486S (Trurl) and an 8580 R5 1088 (reFX-Mike); WEAK a 6581 R2 4383 (ltx128) and an 8580 R5 4887; STRONG a 6581 R2 0384 (Trurl) and an 8580 R5 1489 |
| `setDacLeakage` | 0..1, 1 default | scales the DAC's MOSFET leakage down only; the R-2R ladder's 2R/R is fixed per model (2.20 on the 6581, 2.00 on the 8580; `Dac.cpp`) |

Its 6581 filter's op-amp transfer function was measured on a 6581R4AR 0687, its 8580's on
an 8580R5 1690 (`FilterModelConfig6581.cpp`, `FilterModelConfig8580.cpp`).

## Uncertainty: no per-revision filter settings exist

We found no documented filter curve or range per 6581 revision, in reSIDfp, sidplayfp or
the community. The R2, R3 and R4AR share their filter design (Wikipedia, "MOS Technology
6581", Revisions: the R3 changed the input pins' protection, the R4AR the silicon grade;
uncited there; Stone Oakvalley's SID collection, "The R2 is not R2", repeats that the R3
left the filter alone), while 6581 filters vary widely from chip to chip (reSIDfp's
`Filter6581.h`: its resistances are "only approximate values for one particular SID
chip"). So the R2 and R3 filter rows are **SIDOU's representative choices** within that
spread, not measurements: the R2 brighter, the R3 darker than reSIDfp's default. What is
revision-specific and sampled is their combined waveforms.

## The profiles

`sidou-tablegen` (`profiles.rs`) holds them and generates `core/sid/gen/profiles.h` and the
filter rows of `core/sid/gen/filter.h` (`FILTER_CURVES`); the drift tests keep both.

| # | Profile | Filter (reSIDfp setting) | Combined waveforms | DACs | Non-linearity | Source |
|---|---|---|---|---|---|---|
| 0 | 6581 R2 | curve 0.58, range default | WEAK (R2 4383) | reSIDfp's | 1 | SIDOU's choice; waveforms sampled |
| 1 | 6581 R3 | curve 0.42, range default | AVERAGE (R3 0486S) | reSIDfp's | 1 | SIDOU's choice; waveforms sampled |
| 2 | 6581 R4AR | curve 0.5, range default | AVERAGE | reSIDfp's | 1 | reSIDfp's defaults (op-amp from an R4AR) |
| 3 | 8580 R5 | curve 0.5 | AVERAGE (R5 1088) | reSIDfp's | n/a | reSIDfp's defaults |
| 4 | worn 6581 | curve 0.35, range 0.4 | STRONG (R2 0384) | 2R/R 2.30, leakage 0.0225 | 1.5 | **artistic** |

The R3 and the R4AR share their combined waveforms: reSIDfp's AVERAGE, sampled from a 6581
R3 0486S. reSIDfp has no R4AR sample, so the R4AR profile plays the R3's; what tells the two
apart is the filter curve.

"Worn 6581" is defined as an aged, abused chip, not a measured one: a darker and narrower
filter (lower curve and range), the strongest combined waveforms, a ladder further from
2R = 2R/R and three times reSIDfp's leakage (beyond what `setDacLeakage` can do), and half
again the 6581 non-linearity.

Two more rows are what age/heat drifts towards: 6581 curve 0.25 at range 0.3, and 8580
curve 0.75. Cutoffs at FC 256 / 1024 / 2047: R4AR 302 / 8,612 / 27,003 Hz; R2 566 / 10,484
/ 29,912; R3 239 / 6,732 / 23,194; worn 191 / 4,182 / 15,428; 6581 hot 143 / 1,741 /
10,422; 8580 R5 1,772 / 7,134 / 14,299; 8580 hot 966 / 3,877 / 7,780.

The 6581 non-linearity's lowest cutoff (`sid_nonlin.floor`) now follows the cutoff table at
FC 0 against the default row's, and an amount above 1 no longer raises it: reSIDfp's
lowest cutoff moves with its curve and range, and scaling it by the amount put worn's
filter at 361 Hz where reSIDfp's is 191 Hz (13.6 dB off; 5.9 after). At the default row
and amounts up to 1 it is M1b's, bit for bit.

## The controls

Each is 0..127 (MIDI CC 86, 87, 89, 90; NRPN 2/1..4), recorded and sent at the next 10 ms
tick, and ramped there (below):
- **Age/heat:** the cutoff and damping move linearly in Hz from the profile's row towards
  its hot row (127: the hot row), and the 6581 non-linearity rises to twice the profile's
  (clamped to 2). The chip holds both rows (`cutoff_g`, the profile's as the SVF's g, and
  `cutoff_x_to`, the hot row's as angles, pi f / fs) and blends them at each `$15`-`$17`
  write and at a commit (`sid_update_filter`: the angle's blend, then its tangent, a Pade
  approximant, and the first row's angle an arctangent one, about 40 instructions; at age
  0 the first row's entry exactly, so the reSIDfp comparisons and the golden hashes hold
  bit for bit). The blended cutoff is the Hz blend of the two rows' entries to 1.5e-6
  (`unit/sid_filter.rs`). So an age change is a number in the stage, no table: the first
  build of this task rebuilt 2,048 tangents for it (0.20 M instructions) and blended in Hz
  first; a blend of g instead would have been up to 49 % off the Hz blend near the top
  rows.
- **DAC sloppiness:** the voice DACs' 2R/R ratio rises by 0.3 and their leakage to six
  times the profile's at 127; a new table is built, over four ticks (`sid_dac_job`: the
  ladders and raw outputs, then a third of the waveform DAC each; `sid_dac_build` is the
  same steps, so the table is bit for bit).
- **Output thump:** a DC the volume register scales, up to 1.0 voice-DAC unit at volume
  15, so a volume write steps the output. The step goes straight into the DC blocker's
  state at the `$18` write (the blocker's own response to a DC step), so it costs nothing
  per sample. Headroom: the output gain falls so that full scale moves out by the thump,
  the largest step a volume write can add (6581 at 127: -1.20 dB), so a signal that reached
  full scale plus the step still does: the clipping M1b saw, matching the step alone,
  cannot happen. (Amended 2026-10-23, the second amendment below: an output stage after the
  effect slots plays the steps, fitted into the room the output leaves; the chip keeps no
  headroom, its gain is sid_init's at any thump.) What it is not:
  - **Not M1b's volume step, closed.** M1b left the 6581's volume-register DC step as a gap:
    reSIDfp's step is 2.65 times SIDOU's with one voice, 1.39 times with three; the extra is
    0.33 to 0.47 units, so thump 42 to 60 is about reSIDfp's step, but only about: the
    thump's step is the same for any voice count, where reSIDfp's grows with the voices'
    DC, and its headroom puts the 6581 0.41 to 0.59 dB below reSIDfp's level there (M1b's
    single-voice level tolerance is 0.2 dB). At the default, 0, the gap stays open, as
    M1b left it.
  - **(Superseded after the first hardware test: MASTER is now a digital gain after the
    chips, `app/master_gain.h`, and no longer writes the volume register, so it no longer
    thumps; a song's own `$18` writes still do, as they should.)**
    **The MASTER pot clicks with it.** MASTER writes the volume register, so with thump
    on each of its 15 steps steps the output: about 280 LSB a step at 127 on the 6581
    (4,191 for 0 to 15, `unit/sid_character.rs`), as reSIDfp's chip does at every volume
    write. Scaling MASTER digitally while thump is on (the output gain, at no per-sample
    cost) was considered and not done: the live instrument's only volume writes are
    MASTER's, so thump would go silent until M3's player writes `$18`. Thump 0, the
    default, keeps MASTER clean. `docs/reference/controls.md` says so.
- **Filter bleed:** up to half the unfiltered voices also reach the filter's input.
  (Amended 2026-10-23, below: the routed voices leak past the filter instead, and DAC and
  thump gained bit errors and a note's thump.)

### Ramped, and never in the way of the main loop

A jump of a control is not applied at once: every tick (live sends `LIVE_REG_CHAR_TICK`
after the settings that changed) the sink's ramp (`sid_character_ramp_tick`) moves age and
thump up to 4 towards the target (0 to 127 in 32 ticks, 0.32 s), the bleed up to 1 (127
ticks, 1.27 s) and the DACs up to 3 a table (43 tables, 172 ticks, 1.72 s). The register
ring's sink and the host's (`live_sid_sink`) run the same ramp at the same ticks. The first
build moved the bleed 4 a tick and the DACs 32 a table (four tables, 0.16 s); the click
test (below) heard both steps.

The first build of this task waited, at each tick with a change, for the register ring to
empty (up to a block, 2.9 ms), then rebuilt in place (age 0.20 M instructions, a DAC table
0.09 M, about 1.1 ms at full character): turning age while playing held key and MIDI
handling back by up to about 4 ms a tick. Now nothing waits:
- the ramp's stage goes into whichever of two stages is not queued, and the sink pushes
  `REG_RING_CHARACTER` with its index; the drain copies it into the rendered chip at the
  block boundary (`sid_character_commit`, which also sets the filter's coefficients for the
  age) and clears its pending flag, after a compiler fence. The producer writes a stage
  only while its flag is clear: on one core the interrupt runs to its end before the
  producer resumes, so neither sees the other's half-written stage. With both queued (two
  ticks in one pass, then a third before a block) the tick waits its turn
  (`character_defers`), which a main loop on time never meets;
- a DAC table goes into whichever of the two tables neither the chip the queued entries
  end on, nor the rendered one while a swap is queued, nor a queued stage points at; with
  none free the DACs wait a tick. The choice reads the queued stages before the chips,
  against the drain's order (a commit sets the chip's table, then clears the stage's flag),
  so a commit between the reads cannot make the table it hands the chip look free; the
  first build read the chips first, and an app test that plays the interrupt there caught
  the fourth table of a 0 to 127 sweep being built over the one the chip played;
- the drain counts every tick's entry (`ticks`), so the emulator test replays the device's
  ticks block for block on the host (`character.rs`, sample-exact).

Measured in fm1-emu: a tick's character work is at most 28,334 instructions (the DAC
table's first step; the others about 18k, a stage 373), from up to 0.20 M and the wait. A
note-on while CC 86 and CC 87 sweep all the while (`latency.rs`, 24 trials, the DACs
building a step a tick): taken by the audio interrupt within 3.26 ms of its arrival
(bound 3.40, a block and a poll), its first sample in the buffer within 3.50 ms (bound
3.65 with the render), the note-off within 3.12 ms, as the busy screen's case without the
sweep (3.26 and 3.50); the longest main-loop pass (midi_drain to midi_service) 47,348
instructions, against the poll allowance of 120,000.

A profile is a reset and is not ramped: the sink waits for the ring to empty (as for any
model change) and gives the spare chip the whole character before the swap
(`prepare_spare`).

## Results

**Each profile at zero character against reSIDfp at its settings** (`tests/profiles.rs`,
the transfer functions of `tests/filter.rs`, FC 64..512 at resonance 0, 8 and 15; largest
shape error LP / BP / HP in dB, passband gain range):

| Profile | Shape | Passband gain | Against M1b's tolerances |
|---|---|---|---|
| 6581 R4AR | 3.31 / 3.12 / 2.88 | -1.74..1.78, -2.01..2.05, -0.39..0.51 | within (the default, unchanged) |
| 8580 R5 | 0.97 / 0.49 / 0.22 | -0.11..0.00, -0.17..-0.05, -0.05..0.03 | within |
| 6581 R2 | 2.62 / 2.38 / 2.81 | -1.52..1.80, -2.24..1.18, -0.29..0.49 | LP gain +0.01, BP gain -0.22 dB over |
| 6581 R3 | 3.32 / 3.13 / 2.89 | -1.64..2.26, -2.60..2.18, -0.39..0.54 | shape at the limit; gain up to 0.58 dB over |
| worn 6581 | 5.92 / 5.68 / 5.16 | -1.67..2.00, -1.66..2.21, -0.42..0.10 | over: artistic |

SIDOU's 6581 output mix and non-linearity were fitted at reSIDfp's default curve (M1b Task
6), so away from it the passband spreads a little; the R3's shape errors are the R4AR's
(FC 64, resonance 15, the threshold region both share). Worn at a non-linearity of 1 is
4.2 / 4.0 / 3.7 dB: the model, fitted at the default range, follows range 0.4 less
closely, and its 1.5 adds the rest by design. The test holds each profile to its own
ceilings (M1b's where met, else the measured value rounded up to 0.01 dB).

**Combined waveforms:** OSC3 equals reSIDfp's at each profile's strength, read for read,
for ST, PT, PS and PST at two pulse widths, once a frame and every 7 cycles, and the reads
tell the profile's strength from the other two.

**Controls** (`tests/unit/sid_character.rs`): each moves its measure monotonically at every
one of its 128 values (age: the cutoff at FC 1024 falls and the non-linearity rises, and
the chord's brightness falls at five; DAC: a released voice's leakage, 6581 R4AR 31 to 177
RMS; thump: the step at a volume write, its signed area, the peak 23 to 4,191 LSB on the
6581; bleed: the filter's added output, 294 to 894 RMS), and clamps (past 127 is 127, a
bad profile is the model's default).

**No clicks:** the effect slots' measure (TESTING.md): a jump of each control from 0 to 127
through the host's sink and its ramp, against the same move made smoothly, must keep the
2nd difference of the two runs' difference within 30 LSB, which a 100 LSB step exceeds.
On a 110 Hz triangle (the filter as the control needs it: age through the low-pass at
resonance 15 and FC 1536, the bleed into the high-pass), at ten phases of the triangle.
The smooth run takes the ramp's own path at its speed (the line through the middle of each
of its steps), the control interpolated between its settings' stages at every sample (the
DAC tables every 16 samples). A reference ramped over a fixed 0.2 s, as the slots' tests
use, measures the DACs' path instead of their steps: two runs at different DAC settings at
the same moment differ by the tables' kinks wherever the triangle crosses them, so the
first build's DAC ramp read 126 LSB on the R4AR against it, and slower ramps read more
(213 at 8 a table, 263 at 4). Against the ramp's own path the first build's ramps measured
(6581 R4AR, 8580 R5, worn): age 5, 3, 3 (at FC 384), thump 19, 12, 19, but bleed 55, 61,
53 and the DACs 125, 52, 269. Now: age 11, 4, 5; DACs 14, 9, 27; thump 19, 12, 19; bleed
15, 17, 15; each ramp at least 0.2 s. At once: age 226, 65, 170 (at FC 384 only 13 to 20,
which is why the test sits at 1536), DACs 465, 335, 990, thump 529, 313, 517, bleed
1,672, 1,859, 1,626. The first click test bounded the largest sample step by 1.25 times
the triangle's own slope, which age and the DACs at once passed.

**Age's rows against reSIDfp** (`tests/profiles.rs`, recorded, not bounded: they are
artistic targets): the 6581's (curve 0.25 at range 0.3) 5.04 / 4.81 / 4.40 dB in shape,
passband gain -1.77..-0.29, -1.03..0.98, -0.40..0.05 dB, as far from SIDOU's fit as worn's
row (the model was fitted at the default range); the 8580's (curve 0.75) 0.23 / 0.16 /
0.21 dB, -0.09..-0.01, -0.09..-0.03, -0.05..0.02 dB, within M1b's tolerances.

## Cost

- **Flash:** 351,404 bytes of image on main with M1e's logo and title cards (main alone
  241,512; the first build of this task 410,076), 60.4 % of the app slot's 581,564: the
  6581's WEAK and STRONG combined waveforms (`SIDOU_ALL_COMBINED_PROFILES`, on by default,
  +61,440 bytes; R2 and worn need them), five 6581 filter rows and two 8580 rows (8,256
  bytes a row: 57,792 bytes, from M1b's 16,512), and the code. The 8580's WEAK and STRONG
  stay out (`SIDOU_ALL_COMBINED_PROFILES_8580`, off): its one profile, the R5, plays
  AVERAGE, and the first build carried them, 61,440 bytes no profile read. emu-gate's
  `character.rs` checks the tables the image holds. If M2 or M3 need the flash, the active
  profile's rows can be generated into POOL instead (docs/TODO.md). Since M2 the rows
  share 16-entry blocks (`gen/pulldown.h`): the four profiles' 122,880 bytes take 39,256
  (decision record 0003, Costs).
- **RAM:** `chips` (POOL) grew from 17,880 to 69,368 bytes: two 17,408-byte DAC tables,
  each chip's second filter row (8,256 bytes), the ramp and the two stages. POOL keeps
  99,248 bytes free. `.ram_text` grew from 33,924 to 36,376 bytes (74.0 % of 48 KB): the
  renders' copies with the bleed, and the age's blend in `sid_render_tap` and the drain.
- **Audio interrupt, filter writes:** the age's blend is an atan (up to 3 divides) and a
  tan, about 70 instructions. The first build ran it at every `$15`-`$17` write in the
  drain, so dense automation (16 USB CCs a millisecond, emu-gate's `midi.rs`) added 58
  instructions a sample to the interrupt for the cutoff at age 0, 108 at age 127 (over the
  ISR's 100), and 44 and 70 for the resonance. A write now only marks the filter
  (`filter_dirty`), and `sid_render_tap` recomputes it once before the block, from the
  registers as the block's writes left them: what the per-write updates played, bit for bit
  (a render reads the coefficients only at its start). The same floods measure 34.1 and
  34.6 (cutoff, age 0 and 127), 31.9 and 32.5 (resonance): the entries' own pops and
  writes; the gate holds them to the ISR's 100 and age 127 within 2 of age 0.
- **Per sample:** nothing for profile, age, DAC or thump (tables and parameters made in the
  main loop; thump at the `$18` write). The bleed runs in a copy of each model's render
  used only while it is not 0; its floor is its multiply-add, about 2 instructions a
  sample a chip. The first build measured it at 5.55 and 11.91 (one and two chips) in the
  worst blocks: the rest was `sid_nl_tick`'s clamps, data-dependent branches meeting the
  bled voices. They are now fmax and fmin, which the pi32v2's FPU has as instructions
  (fm1-isa's `fpu.rs`; clang lowers `__builtin_fmaxf` to one): the same values but for a
  tie of +0 and -0, which no bound here makes. In the emulator gate that makes the
  bleed's copy 6.0 a sample cheaper, the worst blocks 5.7 (one chip) and 12.2 (two)
  cheaper, and the default render 1.0 dearer on average (register allocation); a `fabs`
  for the swing made the render 17 dearer and is not used. With the clamps, every 6581
  scenario measures 7.0 a sample under main's, from fewer spills in the register
  allocation (`render_6581`'s frame went from main's 136 to 128 bytes, its stack accesses
  from 107 to 86), the 8580's 0.1 over (the render's choice of copy), the output bit for
  bit the same. Worst blocks now: 710.52 and 1,425.73 (one and two chips: the default
  chip's noise_tri_ffff and noise_tri_ring_0800, 0.04 and 0.07 over the 710.48 and
  1,425.66 before the filter's update moved into `sid_render_tap`; the costliest
  character's 709.94 and 1,424.80), from the first build's 724.21 and 1,453.96 and M1b's
  717.49 and 1,439.67. `SID_WORST_BLOCK_*` tighten to 711 and 1,426 (from 725 and 1,454), `SID_WORST_PER_CHIP` to 550 (549.37 measured, from 557).
- **Gates:** 1,426 + 4.5 (tap) + 838.97 = 2,269.47 of 2,300 (30.53 to spare, from 2.53 in
  the first build and 16.53 in M1b); the 2,700 target leaves the 6502 205 (from 177; M1b
  191).
- **Main loop:** MIDI out's look and live's snapshot cover 42 parameters instead of 37
  (MIDI out idle 906 instructions a tick, deduplicated 1,914: 21.6 and 45.6 a parameter,
  its emulator bounds now 22.0 and 46.0). The LIVE view's chord golden
  (`tests/golden/lcd/live-chord.png`) depended on where the chord's three presses fell
  against the key scan, which the extra main-loop work moved; the test now presses the
  keys 5 ms apart, more than a scan frame, so the voices follow the keys whatever the
  main loop costs.
- **Main loop, the character:** a control tick at most 28,334 instructions (above). A
  profile change is a reset: 0.50 M instructions for worn at full character (`sid_init`
  0.16 M, the filter's two rows 0.23 M, the DAC table 0.08 M), from 0.70 M. The main-loop
  pass that runs it (midi_drain to midi_service, emu-gate's
  `chip_resets_at_full_character_stay_within_their_bound`) measured 471.5k onto worn by the
  CHIP view's ALGORITHM, 471.4k by CC 85 and 470.7k by the FX button (R2 and R3 471.2k, R4AR
  313.5k, the 8580 362.6k), bounded at 520k. It is the one pass over the latency gate's poll
  allowance (120k), an exception kept on purpose: a profile or model step is a deliberate
  act of the player, applied at most once a 10 ms tick (requests in between coalesce), MIDI
  waits in its buffer meanwhile rather than being dropped, and the FX button and CC 85 have
  always cost the same. A note arriving during the pass waits for it (about 2 ms) and for
  the drain after it: about 5 ms against decision record 0005's 3.4 ms, only when the
  player steps the profile or model. M2b Task 1 prepares the spare over ticks instead and
  retires the exception (amendment below). `sid_init`
  itself, 147,391 instructions in M1b on the 6581 (204,473 on the 8580), grew to 217,834
  in the first build (this record said 0.31 M: that was the SID gate's whole set-up, the
  default DAC tables included, 241,053 in M1b and 312k then) and is now 160,057 (225,505):
  one pass builds both rows for one curve setting, and the tables, which it writes whole,
  are no longer zeroed first (the device's `memset` goes a byte at a time). The gate's
  set-up is 241,591 again.

## Amendment: the spare chip a slice a tick (M2b Task 1, 2026-10-16)

The exception in "Cost" is gone: no main-loop pass is over the poll allowance any more.

- **The request, not the reset, changes hands.** A profile or model step (the CHIP view's
  ALGORITHM, the FX button, CC 25, CC 85) is a request; `live_tick` asks its preparer once a
  tick (`reg_ring_prepare_step`, app/reg_ring.h) for one slice of the spare chip, and sends
  the reset, the gate reset and every register, as before, in the tick the spare is whole.
  The old chip plays on meanwhile, every register and setting kept. A request for another
  profile restarts the preparation (the newest wins); for the same one it goes on, taking the
  controls as they move (the DACs' setting until the filter rows are done, the rest until the
  last slice), and the ramp takes the chip the rest of the way after the swap. A request back
  to the chip's own profile drops it; requests faster than a preparation (every 70 ms or
  less) restart it each time, so the chip swaps only once they stop. While a preparation
  runs the ramp builds no DAC table (one it was building is dropped), so the spare always has
  a free one; and a pass runs one build's work at most: one slice a pass however many ticks
  it catches up (`reg_ring_pass_end`), and no ramp table started in a pass that ran a slice,
  the swap's last one included. The screens show the
  model asked for at once (as PROFILE always showed the profile asked for), so the toast and
  the raster cover the preparation.
- **The slices** (`sid_character_prep`, core/sid/sid_character.h): `sid_init` without its
  tables, then the profile's first filter row 410 FC values a slice (tangents, 5 slices),
  its hot row 1,024 a slice (angles, 2), the DAC table a `sid_dac_job` step a slice (4) when
  the character's DACs are custom, the stage committed in the last. 7 slices with the
  model's own DACs (R2, R3, R4AR and the R5 with DAC sloppiness at 0), 11 otherwise (worn
  always, every profile at full character): 70 or 110 ms. The chip is `sid_init` and
  `sid_character_apply`'s byte for byte (host test, every profile on both models at four
  settings). The 8580's default DAC tables, built on first use (`sid_dac_default`, 94,995
  instructions in one go), are built a `sid_dac_job` step a slice by a preparer's first 8580
  preparation (`sid_dac_default_step`): it spends 4 more slices on them whether or not they
  are built yet (`default_8580_spent`, on the host as on the device), so the count is the
  same everywhere: about 150 ms, once after power-on. (Building them
  at boot instead shifted the boot's timing, and with it three goldens and a LIVE ui_poll.)
- **Measured** (emu-gate `screen.rs`, every way to step at full character onto every
  profile): the worst slice 35,378 instructions (the 8580 R5's tangent row), against
  `REG_RING_PREPARE_SLICE` = 40,000; the worst pass with a slice 74,749 (from 471.5k), the
  CHIP view's ALGORITHM 71,762 with the page's character; with LIVE's busiest screen (a noise
  chord) and CC 87 sweeping through a step, swaps included, 91,643. `CHIP_RESET_PASS` and
  `CHIP_PROFILE_PASS` (520k and 400k) are gone: the passes are held to the poll allowance,
  and at compile time the busiest LIVE pass (68k) and CHIP's (49k plus its input) fit with a
  slice. The latency gate's new profile-step case (a preparation in flight all through each
  trial, at least 18 slices each) keeps the normal bounds over 24 trials: a note-on taken by
  the audio interrupt within 3.06 ms (mean 1.69), its first sample in the buffer within
  3.30 ms, a note-off taken within 2.81 ms, against the 3.40 ms block and poll; the longest
  pass 60,125 instructions, MIDI's longest wait in its ring 1.0 ms. That wait is not the
  preparation's: Task 1's first build measured 201 us in this case, and the review's fixes
  1.0 ms, but the latency gate's Idle case (no profile step) and its USB retry case measure
  the same 1.0 ms in the same builds. It is `usb_retry`'s attach (docs/hardware.md, 300k
  instructions once a second), which a CC of the trials now happens to meet: in fm1-emu a
  stream of CC 85 steps every 40 ms keeps the ring's wait at 203 us or less for 3 s of steps,
  and one CC put on the attach waits 1.2 ms. Benign, and the note bounds hold.
- **Determinism:** the slices a preparation takes depend only on the request
  (`sid_character_prep_slices`), and the host's `live_sid_prepare` runs the same ones at the
  same ticks, so the emulator gate's host renders swap in the device's block, bit for bit
  (`character.rs`). The device lags the host only in a catch-up pass of two ticks: there the
  second tick runs no slice and starts no DAC table (the host's ticks each stand for a
  pass), a tick later.
- **The store:** a pass that runs a slice sets `reg_ring_out.sliced`, for M2a Task 4's store
  to skip its work in that pass.

## Amendment: independent expert controls (M2b Task 6)

This amendment replaces M1c's deferral of the expert layer. The inventory comes from the
character resolver, DAC builder, filter model and oscillator, not just the four macros.
It separates fourteen editable settings from the fixed calibration constants below.
The earlier measurements in this record describe the profile/macro path; they are not
measurements of the new expert range.

### Inventory and encoding

Indices are the zero-based `SID_EXPERT_*` order. Each parameter has NRPN MSB 2 and
LSB `5 + index`, with raw edited values 0..127. Except for combined strength and noise
write-back, the raw range maps linearly to the physical range shown here. `255` is the
stored/API sentinel for following the profile/macros, **not** an NRPN data value.
The override mask distinguishes legacy zero-initialized settings from explicit raw zero.

“Rebuild” means a large filter/DAC table built in main context, not a coefficient update
or selection of an existing table. Model-specific settings remain addressable on both
models, but only affect the model named in their description.

| Index | NRPN | Parameter | Edited range | Unit | Rebuild? |
|---|---|---|---|---|---|
| 0 | 2/5 | 6581 filter curve | 0..1 | reSIDfp coordinate | Yes: filter rows |
| 1 | 2/6 | 6581 filter range | 0..1 | reSIDfp coordinate | Yes: filter rows |
| 2 | 2/7 | 8580 filter curve | 0..1 | reSIDfp coordinate | Yes: filter rows |
| 3 | 2/8 | 6581 non-linearity | 0..2 | multiplier | No |
| 4 | 2/9 | Filter-input DC bias (6581) | 0..2.3125 | voice-DAC units per routed input | No |
| 5 | 2/10 | Filter bleed | 0..0.5 | fraction of the routed voices past the filter | No |
| 6 | 2/11 | DAC ladder kink | 1.8..2.6 | 2R/R ratio | Yes: voice/envelope DACs |
| 7 | 2/12 | DAC leakage | 0..0.15 | fraction of an off bit's weight | Yes: voice/envelope DACs |
| 8 | 2/13 | Combined-waveform strength | WEAK / AVERAGE / STRONG | sampled-table choice | No |
| 9 | 2/14 | Noise write-back | 0..1 | tap-selection strength | No: small stage masks |
| 10 | 2/15 | Output thump | 0..1 | voice-DAC units at volume 15 (a volume write's step; a note's is twice it) | No |
| 11 | 2/16 | External low-pass corner (6581) | 1000..20000 | Hz | No |
| 12 | 2/17 | External high-pass corner | 0..100 | Hz | No |
| 13 | 2/18 | Envelope-following voice DC | 0..0.6 | voice-DAC units before envelope scaling | Yes: DACs |

The curve/range coordinates are not Hz. The 6581 range corresponds to reSIDfp's
`uCox = 1 + 39 × range` in µA/V²; increasing the 8580 curve darkens its response.
Combined strength selects WEAK for raw 0..31, AVERAGE for 32..95, STRONG for 96..127;
the panel calls AVERAGE “MED”. Noise write-back selects `ceil(raw × 8 / 127)` writable
taps, so it has nine effective tap counts, not 128 continuously different strengths.
It retains the model's noise timing and existing register state rather than reseeding it.

The three DC controls are distinct: filter bias changes the 6581 non-linearity's load
per routed voice/EXT IN; thump changes the transient caused by a volume-register write;
voice DC is embedded in each waveform DAC entry and follows that voice's envelope.
The last is specified after the DAC builder's factor of two, not in volts. External HP
is the output DC blocker on both models; zero removes its DC rejection. The current
external LP is the fitted 6581 output pole: the 8580 render does not consume that pole.

### Defaults for every profile

These are resolved physical defaults with all four macros at zero and no overrides,
not rounded raw values. `r = 19/39` and `d = 2 × (0.2143/1.5)` retain the resolver's
unquantized expressions (`d` is approximately 0.28573333 voice-DAC units). Parenthesized
values are retained/resolved defaults for settings inactive on that profile's model.

| Index / parameter | 6581 R2 | 6581 R3 | 6581 R4AR | 8580 R5 | Worn 6581 |
|---|---|---|---|---|---|
| 0 / 6581 curve | 0.58 | 0.42 | 0.5 | (0.5) | 0.35 |
| 1 / 6581 range | r | r | r | (r) | 0.4 |
| 2 / 8580 curve | (0.5) | (0.5) | (0.5) | 0.5 | (0.5) |
| 3 / non-linearity | 1 | 1 | 1 | (1) | 1.5 |
| 4 / filter bias | 1.15625 | 1.15625 | 1.15625 | (1.15625) | 1.15625 |
| 5 / bleed | 0 | 0 | 0 | 0 | 0 |
| 6 / DAC kink | 2.2 | 2.2 | 2.2 | 2.0 | 2.3 |
| 7 / DAC leakage | 0.0075 | 0.0075 | 0.0075 | 0.0035 | 0.0225 |
| 8 / combined | WEAK | AVERAGE | AVERAGE | AVERAGE | STRONG |
| 9 / noise write-back | 1 | 1 | 1 | 1 | 1 |
| 10 / thump | 0 | 0 | 0 | 0 | 0 |
| 11 / external LP | 13154 | 13154 | 13154 | (13154) | 13154 |
| 12 / external HP | 16 | 16 | 16 | 16 | 16 |
| 13 / voice DC | d | d | d | 0 | d |

Following a default never round-trips it through seven-bit encoding. Raw NRPN/getter
readback rounds to seven bits, while unit readouts format the effective physical value.
The unedited audio path uses the exact profile value and existing profile/hot filter tables.
Editing to the corresponding raw value creates an override and need not reproduce the
unquantized default exactly.

For nonzero macros, with `a = age/127` and `s = DAC/127`, the independent defaults follow
the macro paths:

- AGE reports the 6581 curve interpolated towards 0.25 and range towards 0.3, the 8580
  curve `0.5 + 0.25a`, and non-linearity `min(profile amount × (1 + a), 2)`.
  Without a filter override the actual filter remains the existing Hz/damping blend of
  complete profile and hot rows, not a fresh interpolation in reSIDfp coordinates.
- DAC gives `profile kink + 0.3r(DAC)` and `profile leakage × (1 + 5r(DAC))`, with
  `r(v) = a (0.15 + 0.85a)`, `a = v/127`, the perceptual curve (amended 2026-10-23, below).
- THUMP gives `r(thump)` voice-DAC units; BLEED gives `0.5 × r(bleed)`.
- Other defaults remain those in the table.

### Fixed calibration, not additional expert parameters

The underlying implementation also contains the following model constants. They are
inventoried here so “every parameter” does not silently imply an adjustable transistor
model or an independently editable coefficient for every intermediate calculation:

- The 6581 non-linearity fit has level 0.055 (scaled from 44.1 kHz), level reference
  0.13875, load 0.13125, load reference 1.7375, and floor 240.7 Hz. The amount scales
  level/load and the floor as described earlier; the floor also follows the filter row.
  Load multipliers stay clamped to 0.35..1.3. These are fixed fit/stability constants,
  not six more controls.
- Filter damping is the profile's sixteen-entry `k = 1/Q` row, addressed by the SID
  resonance register. Expert curve/range fitting does not independently refit damping.
  The existing filter cutoff clamp remains 30 Hz..0.45 × sample rate.
- The 6581 filter mix gains are LP 0.6875, BP 0.8125, HP 0.5, with integrator leakage
  `1/3.375` and the existing output polarities/cross-mode terms. They are fixed calibration.
- Output calibration remains `0.1 × 32767` times 1.4736 (6581) or 1.1288 (8580) (with a
  derived thump headroom until the second amendment of 2026-10-23, below: none since). Neither calibration gain nor the digital MASTER is an expert
  analogue control.
- The 6581 waveform DAC's cubic saturation uses gain 1.1 and saturation factor 1.1;
  zero codes remain `0x380` (6581) and `0x800` (8580). Ladder termination is model-specific
  (unterminated 6581, terminated 8580), not another override. Kink/leakage affect both
  waveform and envelope ladders together, not per voice or per DAC independently.
- Combined-waveform pulldown fits remain the three sampled tables per model, not exposed
  per-bit fitting coefficients. Noise TEST reset/fade delays remain 50000/15000 cycles
  (6581) and 986000/314300 cycles (8580). Noise taps and waveform pipeline timings,
  oscillator power-on state, ADSR timing tables and gate-reset policy are digital/model
  behaviour, outside this analogue inventory.

### Application without a chip reset

An expert edit must not call `sid_init`, retrigger a gate, reset oscillator/envelope
state, or replace the chip with a freshly reset spare. Constants and existing-table
choices travel in the character stage; filter and DAC jobs build in bounded main-loop
slices and publish only finished tables at an audio block boundary. The old tables
continue playing until publication. Ramp steps and table jobs retain the existing
poll, click and audio-cost bounds; the old measured costs above are not new measurements.

Filter rows are pointer-backed so their build storage can be borrowed from the shared
spare SID's existing embedded backing. This avoids adding another 8 KiB cutoff buffer
(a complete cutoff-plus-resonance row is 8256 bytes). The shared DAC build table remains
shared too. Arbitration must exclude backing read by either active chip, any queued
stage or pending swap, and any preparation/build already using it. A spare chip's own
embedded memory is not automatically free after row pointers have been exchanged.
When no backing is available, the edit waits rather than overwriting live tables.
Expert rows build in 27 slices (128 tangent/source entries or 192 angle entries per
slice), then fade for 127 ticks. The shared per-pass flag also makes the song's sliced
work and store yield to character-table work, including a two-tick catch-up pass.

Noise feedback keeps a 256-entry byte lookup in SRAM for the authentic fast path;
the small nibble lookup is shared rather than copied into each translation unit.
Stages carry the feedback mask, not four redundant 128-byte lookup copies. At commit,
the chip's small masked lookup changes only when that mask changes. The entry, RAMTEXT
and text image sections end on 16-byte boundaries so compiler-generated lookup constants
keep their required alignment without gaps between the concatenated sections. The normal
and flash-spike builds retain every contiguous-image and RAMTEXT linker assertion.

The expert filter uses **approximate sparse reSIDfp-derived anchors**, generated offline
from the pinned reference. The 6581 grid samples curves 0, 0.25, 0.5, 0.75 and 1 and ranges
0, 0.3, 19/39 and 1; the 8580 samples curves 0, 0.5 and 1. Each fits resonance 8 at
FC 0, 64, …, 1984, 2047. Invalid sub-noise-floor fits clamp to 30 Hz and above-band fits
to 19845 Hz before interpolation.
Interpolation is linear between FC anchors and curve/range coordinates; damping stays
the profile's. This is not an exact full-range reSIDfp model. The unit tests check fitted
cutoff at FC 1024/resonance 8 against fresh measurements at all 23 curve/range coordinates
and the six single-control endpoints (relative error below 0.0001), and monotonic cutoff
at FC 1024 over all 128 settings. They do not establish full-response equivalence between
anchors. Expert jumps retain the existing 30-LSB click-residual ceiling, including AGE
retargets during override release/restoration and held combined-waveform changes.
The original measured profile rows remain the unedited default path.

Combined strength needs the requested tables compiled in. A model built without its
`SIDOU_ALL_COMBINED_PROFILES` / `SIDOU_ALL_COMBINED_PROFILES_8580` flag substitutes AVERAGE
for missing WEAK/STRONG tables. In particular, the earlier 8580 AVERAGE-only build cannot
provide three audible expert choices merely by exposing the selector.
Both models' table flags are now on by default; an old CMake cache must enable the 8580
flag explicitly. Inapplicable controls are grey and show N/A on the page; their popups also show N/A.

### Macro ownership, panel and persistence

An expert edit overrides only that field. A subsequent macro movement releases **only
the related overrides**, restoring their exact profile/macro path:

| Change | Overrides released |
|---|---|
| AGE | Both filter curves, 6581 range, non-linearity (indices 0..3) |
| DAC | Kink and leakage (6, 7); voice DC remains independent |
| THUMP | Output thump (10) |
| BLEED | Filter bleed (5) |
| Base profile | All fourteen |

EXPERT is one additional, end-clamped SELECT stop after FX on the chosen CHIP tab.
ALGORITHM steps through four groups in inventory order, four controls per group;
K1..K4 directly edit those four settings. The last group contains external HP and
voice DC, with two inactive knob positions. This replaces the plan's scrolling
selection/coarse-fine pair: direct knob assignment keeps four physical controls visible
at once. Names are 2×, values 2×/3× with units and knob-colour identification.
EDIT held + ALGORITHM remains the USER save target, not an alternate expert gesture.
The page states **MACRO MOVES RESET ITS OVERRIDES**; the scope is the related fields
above, not unrelated settings. See [panel controls](../reference/controls.md#chip).

[USER schema 2](../reference/profile-format.md) stores the fourteen raw overrides along
with the base profile and four macros; schema 1 loads them as following defaults.
[Settings schema 4](../reference/settings-format.md) stores the keys' raw overrides and
their learned CCs; schemas 1..3 load following defaults with expert learn unbound.
Migration preserves stored macros, so following means their resolved path, not a
quantized snapshot or forced macro zero. These encodings retain the default sentinel
and restore explicit overrides after macro fields. The generated
[MIDI map](../reference/midi.md) assigns the NRPN rows in inventory order; its single
raw default column is not a substitute for the five-profile physical-default table.

Outgoing MIDI snapshots the raw override state separately from resolved UI values.
Model/profile and macro messages precede explicit expert edits; inherited defaults do
not generate additional NRPN edits. A USER pick that releases overrides but keeps the
same base/macros sends the base profile again, then its retained explicit overrides,
so the receiver releases the same fields instead of retaining a stale expert value.
Release obligations remain pending until a releasing message actually goes out, even
if a macro returns to its old wire value while USB is full. Explicit maximum values
learned to a CC are distinct from raw 255 and are never deduplicated against that sentinel.
Unchanged input compares the original 42-parameter prefix in aligned, alias-safe pairs;
expert bytes are revisited only after a raw revision or a relevant macro change.
Ticks doing cold expert metadata or mixed macro restoration use half the normal packet
budget, retaining the original per-tick instruction ceiling without queueing stale edits.

## Amendment, 2026-10-23: DAC, thump and bleed made audible (review E11)

On the FM-1 the user heard no difference across the whole travel of DAC, THUMP and BLEED, and
the ergonomics harness agreed (`docs/reviews/2026-10-22-ergonomics-review.md`, E11). Why:

- **DAC:** a 2R/R ratio 0.3 higher and six times the leakage only bend a saw's ramp: on SAW LEAD
  (a held saw through the low-pass) 0.65 dB and a brightness inside the capture's 6 % jitter.
- **THUMP:** it stepped the output at a volume write only, and since MASTER became a digital gain
  the live instrument never writes `$18`: all it did in live play was its headroom (-1.2 dB).
- **BLEED:** it fed the *unrouted* voices into the filter; every preset routes all its voices
  into the filter, so there was nothing to feed.

What changed (every control at 0 is unchanged bit for bit: the reSIDfp comparisons, the songs'
sound and the profiles hold with their tolerances untouched):

- **A perceptual curve** for DAC, thump and bleed: `r(v) = a (0.15 + 0.85a)`, `a = v/127`
  (`SID_CHARACTER_CURVE_LINEAR`). The first detents are a few % of the effect, 64 about 29 %,
  127 all of it. Age stays linear (it was already heard).
- **DAC: bit errors.** Each ladder bit's weight is scaled by `1 + m p[b]`, with `m` up to 0.5
  (`SID_CHARACTER_MISMATCH`) and one fixed pattern `p` between -1 and 1 per DAC
  (`SID_DAC_MISMATCH_WAVE`, `_ENV`, `sid_dac.h`): resistor mismatch, as a worn chip's. Signs
  alternate, so codes either side of a carry land too high and too low: a triangle (whose codes
  cross every bit) gets grit, its brightness 0.0209 to 0.0541 on the R4AR at 32 to 127
  (`unit/sid_character.rs`); the envelope DAC's low bits err most, so a quiet voice's level
  steps unevenly as its envelope moves (the RMS of its steps' error below 1/8 of its range 0.98
  to 2.42 of the mean step), while its top moves under 1 %. Kink and leakage keep their
  formulas, on the curve. The bit errors have no expert parameter: they follow the macro
  whatever kink and leakage are set to. A held loud saw stays close (the harness: level
  -0.8 dB, brightness in its jitter): a saw's ramp hides DAC errors, physically; a released
  voice's leak still grows 31 to 173 RMS.
- **THUMP at a note's start.** At each gate's rise `sid_write` puts a step into the DC blocker,
  as the volume write does: `thump × 2 × path × volume × A[attack] × (255 - counter)/255`. The
  voice's DC rises with its envelope; `A` is the peak a linear rise over the attack reaches
  through the 16 Hz blocker (0.91 at 2 ms, 0.10 at 100 ms, 0.001 at 8 s), so a pad hardly
  thumps; a retrigger high in the envelope thumps less; `path` is the voice's (1 direct, the
  6581's `mix_lp` (inverting) or the 8580's low-pass switch when routed, 0 for voice 3 off). The
  factor 2 (`THUMP_GATE`) makes a note's thump the louder transient; the headroom stays the
  volume step's, so a note's thump on a full-scale signal saturates for its few ms (fixed by
  the second amendment, below: the output stage). Cost: a
  test and a few multiplies at a gate write, nothing per sample. The CHIP help card says
  **K3 NOTE THUMP**.
- **BLEED: the routed voices leak past the filter**, up to half, unfiltered (`direct +=
  bleed × filt_in` in the bleed's render copy, one multiply-add as before): a closed filter lets
  the bright, dry signal through; on the 6581, whose low-pass inverts, an open one gets
  thinner and quieter. Unrouted voices no longer reach the filter: both directions cost
  another multiply-add a sample, over the worst block's 1.06 of margin.
- **Ramps** are limited by the effect's step, not the detent's: the DACs move as many detents
  a table (up to 6) as keep the effect within 2.75/127 of its amount, the thump up to 4 within
  4/127, the bleed up to 4 within 1/127, and half a detent a tick (`bleed_half`) where one
  detent is more. 0 to 127: age 0.32 s, thump 0.44 s, bleed 1.74 s, DACs 2.61 s (64 tables).
  The click test (30 LSB) now reads, R4AR / 8580 R5 / worn: DACs 21, 21, 29; thump 17, 12, 18;
  bleed 27, 14, 26 (its triangle now routed into a closed low-pass).

**The harness** (SAW LEAD, A3, 32 detents; THUMP's and LIVE K3's windows are now each a new
note from its first sample, since both act where a note starts: `Spec::retrigger`):

| Control | Before: brightness, level | After |
|---|---|---|
| K2 DAC | 2557 to 2587 Hz (jitter), -0.65 dB | 2557 to 2601 Hz (jitter), -0.83 dB |
| K3 THUMP (note starts) | 2188 to 2251 Hz, -1.42 dB (headroom) | 2188 to 2299 Hz, **+3.96 dB** |
| K4 BLEED | 2557 to 2555 Hz, -0.03 dB | **2557 to 7157 Hz (1782 cents), -9.9 dB** |
| LIVE K3 DECAY (note starts) | 2269 to 2274 Hz, +3.2 dB | the same (the firmware did not change) |

No knob finding before or after. The listening files: `character_wavs` in
`unit/sid_character.rs` (ignored; it writes `build/character/`).

**Cost.** Worst blocks (sid_budget), one chip 705.35 to 703.03, two 1,419.55 to 1,414.70
(bounds 711 and 1,426); the 6581's bleed copy 2.3 to 4.9 cheaper, the 8580's 6.0 dearer
(691.02 one chip, 1,408.89 two), every other scenario within 1.2 (a gate write tests the thump). `.ram_text` 46,656 to 46,912
bytes, `.data` +64 (the attack table, read in the drain), `chips` +12; the image +1,712 bytes
(498,208 to 499,920).

## Amendment, 2026-10-23 (2): the thump on the output, so it never clips

The first amendment left a note's thump (twice a volume write's step) in the chip, inside a
headroom sized for the volume step only, so a note's thump on a loud signal clipped for a few
milliseconds. A headroom large enough was weighed and rejected: three voices starting together
thump 5.4 voice-DAC units at 100 % (the 6581's full scale is 6.8), the second chip can thump at
the same moment, and the effects raise the level by up to 3.1 times (a lush reverb's tail, a
fuzz, a flanger's feedback), so the reserve would have cost 5 dB on one chip and over 10 dB with
both and the effects: far too quiet.

What changed:

- **The chip no longer plays the thump.** `sid_write` adds each step (a note's and a volume
  write's, as before) to `thump_pending`, or to `thump_late` with its sample if the chip had
  already rendered part of the block (a song's write in the middle of a block, decision record
  0005's split render). The render is untouched: at any thump the chips' samples are those at
  thump 0, bit for bit. The chip keeps no headroom: its gain is `sid_init`'s at any thump.
- **An output stage plays it** (`app/thump_out.{h,c}`, after the effect slots, before MASTER):
  each chip's steps at its output gain, decaying at its DC blocker's `dc_r` a sample, exactly
  the blocker's step response the chip used to play. A song's late step starts at its sample.
  The thump is fitted into the room the output leaves: on a sample `s` the thump `x` is untouched
  up to half the room left below 32,766 and softly limited beyond (`h + d h / (d + h)`, `h` half
  the room, `d` the excess), so it nears the ceiling and never reaches it. A sample without a
  thump passes bit for bit; one the slots already put on a rail stays there. Under half an LSB
  the stage goes idle (one test a block). A chip B that did not render drops its steps.
- **What it gives up:** the thump no longer passes through the slots (an echo or a reverb does
  not repeat it, a distortion does not shape it): it is a low bump under a note's start, not
  part of the voice. The LIVE view's tap, reading the chip, does not show it.

**Level.** THUMP 0: unchanged, bit for bit. THUMP 100 %: the chip's level rises back to THUMP
0's, +1.19 dB on the 6581 and +0.93 dB on the 8580 (the old headroom's cut), and the thump
itself is as loud as before plus that (a volume step 4,191 to 4,809 LSB on the R4AR, a note's
area +12.8 %). The level no longer depends on THUMP.

**The test** (`app/thump_out.rs`): two full-scale chords (both chips 6581 R4AR, three 50 %
pulses each at volume 15 through an open low-pass, as the presets route them; together 28,718,
-1.15 dB), keys on chip B and a song on chip A, both starting every 81 ms in the same block (A's
from the song's call mid-block), THUMP 100 % on both: before, chip B's mix saturated 1,705
samples (every chord start); now no stage reaches a rail, though the thump moves the output by
up to 26,463 LSB. Through each effect at its loudest corner, the thump adds no sample on a rail
(some effects clip the chords alone: flanger, phaser, autopan, delay and both reverbs at full
feedback, an effects matter the thump does not change); the worst effect the chords alone do not
clip, PING-PONG at full feedback (+1.08 dB), stays clean at THUMP 100 %.

**Cost.** The stage: 30.86 instructions a sample (a stereo frame) while a thump sounds (bound
31, `emu-gate/character.rs`), a take and a test a block otherwise; it takes that from the
6502's headroom while a thump sounds. The chips: a render adds one counter (sid_budget's worst
blocks 703.03 to 703.05 one chip, 1,414.70 to 1,414.74 two); the two-chip device block with
every slot's heaviest 2,388.88 to 2,389.55. `.ram_text` 46,928 to 47,536 bytes, the image
500,672 to 501,264.

## Amendment, 2026-10-23 (3): DAC's grit, heard on every sound

The v0.3.0 device session: DAC still inaudible on SAW LEAD, PWM BASS, NOISE PERC and soft notes
(THUMP and BLEED were heard). Why, measured (`dac_wavs`, `unit/sid_character.rs`, the 6581
R4AR at 0/25/50/75/100 %): the voice DACs' errors are a static map of the waveform's codes,
before the filter. A pulse has two codes, a noise is random anyway, and on a saw a static map
only moves its harmonics' levels: PWM BASS changed by -48 to -27 dB beyond its level, no
brighter; SAW LEAD -38 to -17 dB, +0.6 dB brighter at the top; nothing between the harmonics.
Larger bit errors cannot help (any static map of a periodic wave is harmonic), and the
quantiser-style "stepped" errors tried first (a log converter, 2-bit mantissa, worn bits)
put steps in the output that turn a few LSB of difference between two paths into hundreds:
the click test's ramped-against-smooth residual read 1,918 LSB. So:

- **The grit** (`sid.h`, `grit`): past the physical model, the chip's output through a worn
  converter, `y = x + a x (n - |x|) / 2^17`: `n` a noise (a linear congruential sequence) the
  signal carries, as a worn ladder's current noise, 17 dB under the signal at `a = 1` on a
  loud note and a quiet one alike; and `-x|x|`, a bow (integral non-linearity) that squeezes
  the peaks (6 % at a quarter of full scale), so a chord's notes intermodulate. Continuous in
  `x`, so a small difference in the signal stays small (the click test: DACs 21, 23, 29 LSB,
  as without it: 21, 21, 29). `y` never leaves int16 (`sid_grit_sample`, checked for every
  sample at the noise's ends).
- **Its curve:** `a = sqrt(DAC/127)` (`SID_CHARACTER_GRIT` 1.0), heard from the first detents and
  rising to the top. The physical DAC (kink, leakage, bit errors) is unchanged under it.
- **Its ramp:** the stage carries the asked-for setting from the next tick (`ramp.grit_dac`),
  and the chip slews `a` 48/2^21 a sample (0 to full in a second) to it, the same path however
  the renders split it (`the_grit_slews_the_same_however_the_renders_split`). DAC 0: no pass,
  the chip's output bit for bit (every reSIDfp comparison holds untouched).

**Measured** (beyond a level change / brightness, before → after; `dac_wavs`):

| DAC | SAW LEAD | PWM BASS | soft note | NOISE PERC |
|---|---|---|---|---|
| 25 % | -38.3 → -22.6 dB / +0.05 → +1.39 | -48.1 → -22.7 / 0.00 → +3.38 | -35.4 → -22.4 / +0.05 → +1.21 | -35.9 → -22.4 / 0.00 → +0.21 |
| 50 % | -28.1 → -19.1 / +0.17 → +2.48 | -38.0 → -19.6 / 0.00 → +5.25 | -25.4 → -18.6 / +0.17 → +2.20 | -25.8 → -18.7 / +0.01 → +0.41 |
| 100 % | -17.0 → -13.9 / +0.64 → +4.16 | -27.3 → -16.3 / -0.01 → +7.52 | -14.6 → -12.4 / +0.65 → +3.78 | -15.0 → -12.6 / +0.03 → +0.78 |

The power between SAW LEAD's harmonics -25.2 → -16.5 dB at 100 %, PWM BASS's -17.1 → -14.3.
`dac_is_heard_on_every_sound` holds it: every 16 detents more, at least -25 dB from a quarter,
-17 dB at the top, the notes 3 dB brighter. The harness (K2 on CHIP, SAW LEAD held): brightness
span 118 → 1,463 cents (2,557 → 5,912 Hz, rising smoothly), level 0.8 → 0.9 dB, dead zone 4 → 0
detents, no finding. NOISE PERC changes least: noise riding on noise is the hardest to hear.

**Cost.** The pass, 19.13 instructions a sample on one chip while DAC is not 0 (bound 19.2,
`emu-gate/character.rs`), nothing at 0. It is outside the SID's worst blocks, bounded on its own
as the thump's stage is: with DAC 127 added to the bleed's worst-block scenarios they read 722.24
(one chip) and 1,452.92 (two), over the SID guards (711, 1,426: 703.08 and 1,414.79 without);
the whole interrupt's worst, two chips with every slot's heaviest, 2,379.95, leaves 320 under
2,700 for the two passes' 38. `.ram_text` +128 bytes (the pass 122, the render's test and the
drain's copy), `chips` +52 bytes, no table.
