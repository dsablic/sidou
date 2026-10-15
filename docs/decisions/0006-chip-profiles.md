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

Each is 0..127 (MIDI CC 86, 87, 89, 90; NRPN 2/1..4), recorded and applied at the next
10 ms tick:
- **Age/heat:** the cutoff and damping tables blend linearly in Hz from the profile's row
  towards its hot row (127: the hot row), and the 6581 non-linearity rises to twice the
  profile's (clamped to 2).
- **DAC sloppiness:** the voice DACs' 2R/R ratio rises by 0.3 and their leakage to six
  times the profile's at 127; a new table is built.
- **Output thump:** a DC the volume register scales, up to 1.0 voice-DAC unit at volume
  15, so a volume write steps the output. M1b left the 6581's volume-register DC step as a
  gap: reSIDfp's step is 2.65 times SIDOU's with one voice, 1.39 times with three; the
  extra is 0.33 to 0.47 units, so thump 42 to 60 is about reSIDfp's. The step goes
  straight into the DC blocker's state at the `$18` write (the blocker's own response to
  a DC step), so it costs nothing per sample. Headroom: the output gain falls so that full
  scale moves out by the thump, the largest step a volume write can add (6581 at 127:
  -1.20 dB), so a signal that reached full scale plus the step still does: the clipping M1b
  saw, matching the step alone, cannot happen.
- **Filter bleed:** up to half the unfiltered voices also reach the filter's input.

Profiles go through a chip reset (the spare-chip swap). The register ring's sink keeps the
settings and applies them all to the spare in `prepare_spare` before the swap, since
`sid_init` resets them; the controls change the playing chip with the ring empty: the
filter tables rebuilt in place (nothing reads them before the `$17` write live sends),
the DACs built into whichever of two tables the playing chip does not use, and what the
render reads (the non-linearity, the DAC pointer, thump, bleed, the output gain) copied in
by the drain at one block boundary (`REG_RING_CHARACTER`, `sid_character_commit`).

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

**Controls** (`tests/unit/sid_character.rs`): each moves its measure monotonically
(age: the cutoff at FC 1024 and the chord's brightness fall, the non-linearity rises;
DAC: a released voice's leakage, 6581 R4AR 31 to 177 RMS; thump: the step at a volume
write, 23 to 4,191 LSB on the 6581; bleed: the filter's added output, 294 to 894 RMS),
clamps (past 127 is 127, a bad profile is the model's default), and a sweep of any control
a step per block never steps the output more than the sound held anywhere along the way.

## Cost

- **Flash:** 362,412 bytes of image (from 193,848): every combined-waveform profile now
  compiled (`SIDOU_ALL_COMBINED_PROFILES` on by default, +122,880 bytes; R2 and worn need
  WEAK and STRONG), five 6581 filter rows and two 8580 rows (8,256 bytes a row: 57,792
  bytes, from M1b's 16,512), and the code. The app slot holds 581,564 bytes: 62 % used.
- **RAM:** `chips` (POOL) grew from 17,880 to 52,764 bytes: two 17,408-byte DAC tables for
  the character and its settings and stage. `.ram_text` grew from 33,924 to 35,596 bytes
  (72.4 % of 48 KB), most of it the renders' copies with the bleed.
- **Per sample:** nothing for profile, age, DAC or thump (tables and parameters made in the
  main loop; thump at the `$18` write). The bleed costs a multiply-add in a copy of each
  model's render that runs only while it is not 0, so the default chip measures as before
  (717.59 and 1,439.86 worst blocks, one and two chips). Worst blocks with the costliest
  character (worn, bleed 127): 724.21 and 1,453.96 (worn's profile 1.07 and 2.19, the
  bleed 5.55 and 11.91, with the filter's data-dependent work on the voices it now gets).
  `SID_WORST_BLOCK_*` rise to 725 and 1,454.
- **Gates:** 1,454 + 4.5 (tap) + 838.97 = 2,297.47 of 2,300 (2.53 to spare, from 16.53);
  the 2,700 target leaves the 6502 about 177 (from 191).
- **Main loop:** MIDI out's look and live's snapshot cover 42 parameters instead of 37
  (MIDI out idle 906 instructions a tick; its emulator bound is now per parameter, the
  same 24.3 a parameter). The extra main-loop work moves when the LCD test's chord keys
  are scanned, so the LIVE chord's golden image (`tests/golden/lcd/live-chord.png`) was
  re-recorded: the same notes, E-4 and E-5 on swapped voices.
- **Main loop, the character:** a profile change is a reset (sid_init and the character: 0.70 M
  instructions for the worn profile at full character, where sid_init alone is 0.31 M);
  a control change waits for the ring to empty and
  rebuilds what changed (2,048 tangents for age, a DAC table for sloppiness), at most once
  a tick.

## Not done here

Panel access (the CHIP view) is Task 4; until then the character is MIDI only. The plan's
expert layer (every analogue parameter as its own control) and user-saved profiles are not
in this task.
