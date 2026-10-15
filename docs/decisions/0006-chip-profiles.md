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
  cannot happen. What it is not:
  - **Not M1b's volume step, closed.** M1b left the 6581's volume-register DC step as a gap:
    reSIDfp's step is 2.65 times SIDOU's with one voice, 1.39 times with three; the extra is
    0.33 to 0.47 units, so thump 42 to 60 is about reSIDfp's step, but only about: the
    thump's step is the same for any voice count, where reSIDfp's grows with the voices'
    DC, and its headroom puts the 6581 0.41 to 0.59 dB below reSIDfp's level there (M1b's
    single-voice level tolerance is 0.2 dB). At the default, 0, the gap stays open, as
    M1b left it.
  - **The MASTER pot clicks with it.** MASTER writes the volume register, so with thump
    on each of its 15 steps steps the output: about 280 LSB a step at 127 on the 6581
    (4,191 for 0 to 15, `unit/sid_character.rs`), as reSIDfp's chip does at every volume
    write. Scaling MASTER digitally while thump is on (the output gain, at no per-sample
    cost) was considered and not done: the live instrument's only volume writes are
    MASTER's, so thump would go silent until M3's player writes `$18`. Thump 0, the
    default, keeps MASTER clean. `docs/reference/controls.md` says so.
- **Filter bleed:** up to half the unfiltered voices also reach the filter's input.

### Ramped, and never in the way of the main loop

A jump of a control is not applied at once: every tick (live sends `LIVE_REG_CHAR_TICK`
after the settings that changed) the sink's ramp (`sid_character_ramp_tick`) moves age,
thump and bleed up to 4 towards the target (0 to 127 in 32 ticks, 0.32 s) and the DACs up
to 32 a table (four tables, 16 ticks). The register ring's sink and the host's
(`live_sid_sink`) run the same ramp at the same ticks.

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
  none free the DACs wait a tick;
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

**No clicks:** a jump of each control from 0 to 127, through the host's sink and its ramp,
on a 110 Hz triangle (the filter as the control needs it: age through the low-pass, the
bleed into the high-pass), at ten phases of the triangle: the largest sample step stays
within 1.25 times the triangle's own slope held at either end. The same jump at once, as
the first build applied it, steps past that for the thump (6581 R4AR 564 LSB against a
slope of 205; 8580 R5 354 against 48; worn 539 against 315) and the bleed (1,698, 1,405,
1,642), and for worn's DACs (1,005 against 539); age and the R4AR's and R5's DACs change
the shape, not the level, at once. The previous test swept a chord of saws, whose own
edges hid any step.

**Age's rows against reSIDfp** (`tests/profiles.rs`, recorded, not bounded: they are
artistic targets): the 6581's (curve 0.25 at range 0.3) 5.04 / 4.81 / 4.40 dB in shape,
passband gain -1.77..-0.29, -1.03..0.98, -0.40..0.05 dB, as far from SIDOU's fit as worn's
row (the model was fitted at the default range); the 8580's (curve 0.75) 0.23 / 0.16 /
0.21 dB, -0.09..-0.01, -0.09..-0.03, -0.05..0.02 dB, within M1b's tolerances.

## Cost

- **Flash:** 352,044 bytes of image on main with M1e's logo and title cards (main alone
  241,512; the first build of this task 410,076), 60.5 % of the app slot's 581,564: the
  6581's WEAK and STRONG combined waveforms (`SIDOU_ALL_COMBINED_PROFILES`, on by default,
  +61,440 bytes; R2 and worn need them), five 6581 filter rows and two 8580 rows (8,256
  bytes a row: 57,792 bytes, from M1b's 16,512), and the code. The 8580's WEAK and STRONG
  stay out (`SIDOU_ALL_COMBINED_PROFILES_8580`, off): its one profile, the R5, plays
  AVERAGE, and the first build carried them, 61,440 bytes no profile read. emu-gate's
  `character.rs` checks the tables the image holds. If M2 or M3 need the flash, the active
  profile's rows can be generated into POOL instead (docs/TODO.md).
- **RAM:** `chips` (POOL) grew from 17,880 to 69,368 bytes: two 17,408-byte DAC tables,
  each chip's second filter row (8,256 bytes), the ramp and the two stages. POOL keeps
  99,248 bytes free. `.ram_text` grew from 33,924 to 36,820 bytes (74.9 % of 48 KB): the
  renders' copies with the bleed, and the age's blend in `sid_write` and the drain.
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
  for the swing made the render 17 dearer and is not used. The chip's grown struct also
  moved the 6581 render's addressing (its loop reads the fields after the tables): with the
  clamps, every 6581 scenario measures 7.0 a sample under main's, the 8580's 0.1 over (the
  render's choice of copy), the output bit for bit the same. Worst blocks now: 710.48 and
  1,425.66 (one and two chips: the default chip's noise_tri_ffff and noise_tri_ring_0800;
  the costliest character's 709.94 and 1,424.80), from the first build's 724.21 and
  1,453.96 and M1b's 717.49 and 1,439.67. `SID_WORST_BLOCK_*` tighten to 711 and 1,426
  (from 725 and 1,454), `SID_WORST_PER_CHIP` to 550 (549.37 measured, from 557).
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
  0.16 M, the filter's two rows 0.23 M, the DAC table 0.08 M), from 0.70 M. `sid_init`
  itself, 147,391 instructions in M1b on the 6581 (204,473 on the 8580), grew to 217,834
  in the first build (this record said 0.31 M: that was the SID gate's whole set-up, the
  default DAC tables included, 241,053 in M1b and 312k then) and is now 160,057 (225,505):
  one pass builds both rows for one curve setting, and the tables, which it writes whole,
  are no longer zeroed first (the device's `memset` goes a byte at a time). The gate's
  set-up is 241,591 again.

## Not done here

Panel access (the CHIP view) is Task 4; until then the character is MIDI only. The plan's
expert layer (every analogue parameter as its own control) goes with the CHIP view's panel
entries (Task 4) and the M1d map's NRPN rows after 2/4; user-saved profiles need storage
and go with M2.
