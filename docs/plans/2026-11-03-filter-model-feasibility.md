<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# A reduced reSIDfp 6581 filter: feasibility study

**Status:** proposed, 2026-11-03. The prototype has not started. Against real chips, SIDOU's 6581
filter output is already inside the chip spread (see "Measured" below), so it is not worth
prototyping for accuracy. A study, not a change: nothing here reaches `rel160` or `main` without
the owner's decision.

## Why

SIDOU's 6581 filter is a fitted model of reSIDfp's. Decision 0003 and the 6581 filter section of
[design-notes.md](../design-notes.md) record what is still off; the largest is the loud chord
(three voices, resonance 0, cutoff swept above the chord): band-pass 4.37 dB against a 3 dB
target, reSIDfp's band-pass about 4.5 dB louder and its low-pass about 3 dB quieter over the sweep.

Tried on 2026-11-03 and not kept: `SID_NL_M_MIN` lowered (0.30, 0.25, 0.20), a soft saturation on
both integrator states, on the band-pass state alone, and on the filter's input. Each moves the
band-pass and low-pass errors in opposite directions. Fitting all three modes at once seems to
need reSIDfp's circuit model itself.

## Why reSIDfp's model cannot be ported as it stands

Its 6581 tables, from `third_party/libresidfp/src/FilterModelConfig*.{h,cpp}`:

| Table | What it is | Size |
|---|---|---|
| `mixer` | the op-amp's solution for 1 to 7 inputs, 65,536 entries each | about 3.7 MB |
| `summer` | the same for 2 to 6 inputs | about 2.6 MB |
| `volume` | 16 gain curves, one per volume, 65,536 entries each | 2 MB |
| `resonance` | 16 curves, one per resonance | 2 MB |
| `vcr_n_Ids_term` | the VCR's log-squared current term, as doubles | 512 KB |
| `opamp_rev` | the inverse op-amp transfer curve | 128 KB |
| `vcr_nVg` | `nVddt - sqrt(x)` | 128 KB |

About 11 MB in all, sampled at 16-bit resolution for speed on a PC. Beyond the tables:

- **Clock rate.** reSIDfp solves the integrators every SID cycle (about 985 kHz PAL); SIDOU steps
  once a sample, about 22 times less often.
- **SRAM.** The filter runs in the audio interrupt from SRAM: RAMTEXT has 16 of 49,152 bytes free.
- **Flash.** 10,080 bytes free below the image ceiling (557,920 of 568,000).
- **CPU.** 360 to 540 of 900 instructions a sample a chip are used today; two chips double the
  filter's cost.

These figures are from the 1.5 image, measured on 2026-11-03.

## Why a reduced model might work

Most of the 11 MB is redundancy, not information:

- `volume`, `resonance`, `summer`, `mixer` and `opamp_rev` are smooth one-dimensional curves: a
  coarse table with interpolation (a few hundred points a curve) or a polynomial or spline fit
  should stand in for each.
- `vcr_nVg` can be computed (a fast square root) rather than stored.
- `vcr_n_Ids_term` needs exp and log, too dear on the pi32v2: a short interpolated table.

Plausibly tens of KB instead of megabytes. Not measured.

## The study

1. **Host prototype.** In `sidou-tests` (Rust, or C beside the core), reSIDfp's 6581 integrator
   and summer and mixer path with compressed tables, stepped once a sample. The stiff integrator
   needs an implicit step or 2 to 4 times oversampling: both are tried.
2. **Accuracy.** The existing sweeps, against today's numbers:
   `cargo test --release -p sidou-tests --test filter -- --nocapture` (small signal, full scale,
   noise3, the chord; LP, BP, HP), the transfer functions and the headroom test (-0.53 dB today).
3. **Cost.** Table bytes, flash bytes and instructions a sample a chip: an estimate first, then,
   if the accuracy is worth it, a bench in fm1-emu with the `sid_budget` gate.
4. **Report.** Go or no-go, with the accuracy table and the cost table side by side, and what
   would have to move out of RAMTEXT and flash to make room.

## How it is run

- In its own worktree and branch, off `rel160`; the layered-sound work there is not touched.
- Targeted tests only, under `nice`; the full emu-gate runs only through the locked merge check.
- cargo needs `PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`, run
  from `host/`.
- No budget is raised to make it fit: that is the owner's decision.

## Weighed against

The gap is 2 to 4 dB in loud-filter cases: a slightly different filter character, not a different
synth. The same flash and SRAM are wanted by 1.6's layered sounds and table import, which close
most of what the comparison video showed. The study comes after them, unless the owner decides
otherwise.

## Measured

Measured on 2026-11-03 against real-chip recordings, on branch `filter-metrics` (off `rel160`).
The harness is `filter-metrics` in `sidou-tests`; the method is in
[design-notes.md](../design-notes.md), "Tests: Real chips (filter-metrics)".

### Verdict

**SIDOU's 6581 filter output already sits inside the spread between real chips, about as close
to them as reSIDfp's.** Over 14 filter-heavy tunes, the mean band-energy distance to the real
6581s is 2.87 dB for SIDOU, 2.94 dB for reSIDfp, and 3.13 dB between the real chips themselves.
SIDOU is inside the chip spread on 11 of the 14 tunes. On the other three it is outside
together with reSIDfp, by about as much:

- **bluemax2001** (band-pass all the time): SIDOU is 2.98 dB past the farthest chip pair, and
  reSIDfp is 3.17 dB past it. Both are off below 200 Hz: -12 dB at 50 Hz and -6 to -8 dB at
  125 to 160 Hz.
- **lightforce** (0.54 dB past the farthest pair) and **sanxion** (0.32 dB): this is the method's
  floor. On the three unfiltered controls, SIDOU sits 0.25 to 0.41 dB past the chip spread, and
  reSIDfp sits the same distance past it on two of them.

So a reduced reSIDfp model would bring SIDOU closer to reSIDfp, but not closer to the real chips.
The loud-chord band-pass gap of decision 0003 (4.37 dB against reSIDfp) does not appear in
music against real chips: the level-contour error (lvl) has the same mean, 1.3 dB, for SIDOU,
reSIDfp and the chip pairs. **On this evidence the prototype is not worth starting for
accuracy.**

Where SIDOU is off, band by band, over the filter tunes:

- From 500 Hz to 6.3 kHz SIDOU is 0.85 to 1.3 dB brighter than the chips. reSIDfp is 1.1 to
  2.0 dB brighter from 1 to 6.3 kHz.
- At 125 to 160 Hz SIDOU is 1.1 to 1.3 dB lower; reSIDfp is 0.75 to 0.8 dB lower.
- The chip pairs differ by 1.1 to 2.9 dB a band (absolute).
- The largest offsets for single tunes:
  - delta: +3.2 to +5.4 dB from 1.26 to 6.3 kHz, against reSIDfp's +4 to +8.6 dB;
  - cauldron2remix: +1.9 to +3.9 dB from 400 Hz to 5 kHz, against reSIDfp's +1.1 to +2.7 dB;
  - bluemax2001: the low bands above.

Two findings are not about the 6581 filter:

- **The 8580 and switchblade.** Each tune has only one 8580 recording, so there is no chip spread.
  Against the CSG8580R5, SIDOU's distance is 2.29 dB and reSIDfp's 2.07. SIDOU is 0.4 to 1.0 dB
  farther on delta, shortcircuit, suntanflower and terracresta, and its level contour is worse
  (terracresta lvl 1.1, against 0.2). It is also 0.6 dB farther on switchblade, which never uses
  the filter. switchblade is also the one control where SIDOU's 6581 is outside and reSIDfp's is
  not (1.27, against 0.73; the chips 0.78). Both of these point at the voice or mixer path, not
  the filter. The harmonic probe below finds such a difference.
- **$D418.** Galway's Parallax (HVSC #49, song 1) never writes `$D418`. SIDOU renders silence,
  while all three recordings play the tune, and reSIDfp's 6581 gives -35 dB RMS at volume 0.
  The harness writes `$D418 = $0F` at cycle 0, as the recordings' player evidently did. It is not
  checked what sidplayfp's PSID driver does here. This is worth a look in `psid_env` (not done:
  `core/` is out of scope).

### The tunes

The recordings come from SOASC. Each tune's song 1 was recorded on a real C64, one recording
per chip. The `.sid` files come from the same HVSC #49 tree, and all of them are PSID.
"filter" is the share of the first 30 s with a voice routed into a filter mode. There are 14
filter tunes and 3 unfiltered controls (0%).

| Tune | HVSC #49 | Recordings | filter |
|---|---|---|---|
| bluemax2001 | GAMES/A-F/Blue_Max_2001 | FLAC: R2, R3, R4, R5 | 100% BP |
| cauldron2remix | MUSICIANS/L/Linus/Cauldron_II_Remix | FLAC: R2, R3, R4, R5 | 82% LP/BP/HP |
| suntanflower | MUSICIANS/Z/Zyron/Suntan_Flower | FLAC: R2, R3, R4, R5 | 100% LP+BP |
| switchblade | MUSICIANS/D/Daglish_Ben/Switchblade | FLAC: R2, R3, R4, R5 | 0% (control) |
| delta | MUSICIANS/H/Hubbard_Rob/Delta | MP3 224k: R3, R4, R5 | 100% LP |
| ikplus | MUSICIANS/H/Hubbard_Rob/IK_plus | MP3: R3, R4, R5 | 100% LP/BP |
| lightforce | MUSICIANS/H/Hubbard_Rob/Lightforce | MP3: R3, R4, R5 | 100% LP |
| nemesis | MUSICIANS/H/Hubbard_Rob/Nemesis_the_Warlock | MP3: R3, R4, R5 | 100% LP |
| sanxion | MUSICIANS/H/Hubbard_Rob/Sanxion | MP3: R3, R4, R5 | 46% LP |
| starpaws | MUSICIANS/H/Hubbard_Rob/Star_Paws | MP3: R3, R4, R5 | 100% LP/BP |
| thundercats | MUSICIANS/H/Hubbard_Rob/Thundercats | MP3: R3, R4, R5 | 77% LP/BP |
| spellbound | MUSICIANS/H/Hubbard_Rob/Spellbound | MP3: R3, R4, R5 | 0% (control) |
| zoids | MUSICIANS/H/Hubbard_Rob/Zoids | MP3: R3, R4, R5 | 0% (control) |
| miamivice | MUSICIANS/G/Galway_Martin/Miami_Vice | MP3: R3, R4, R5 | 100% LP+HP |
| shortcircuit | MUSICIANS/G/Galway_Martin/Short_Circuit | MP3: R3, R4, R5 | 100% LP |
| streethawk | MUSICIANS/G/Galway_Martin/Street_Hawk | MP3: R3, R4, R5 | 100% LP |
| terracresta | MUSICIANS/G/Galway_Martin/Terra_Cresta | MP3: R3, R4, R5 | 100% LP |

R2, R3 and R4 are MOS6581R2, R3 and R4; R5 is CSG8580R5. Some candidates were dropped:

- Wizball's song 1 is a 6 s jingle.
- Abase, Kentilla, Parallax and Times of Lore do not use the filter in their first 30 s.
- Arkanoid, Combat School, Game Over, BMX Kidz, Chimera and Mega Apocalypse are RSID.

### Distance per tune

Shape is the mean absolute 1/3-octave band difference (dB) after level-aligning each window. The
render's output stage is fitted, and each value is averaged over the chips of the same model.
lvl is the RMS of the per-window level difference after the global gain. "6581 chips" is the mean
over the pairs of 6581 chips, and "chip max" is the farthest pair. "SIDOU vs reSIDfp" compares the
two 6581 renders directly: same timeline, no fit.

| Tune | SIDOU 6581 | reSIDfp 6581 | 6581 chips | chip max | SIDOU 6581 is | SIDOU 8580 vs R5 | reSIDfp 8580 vs R5 | SIDOU vs reSIDfp |
|---|---|---|---|---|---|---|---|---|
| bluemax2001 | 8.18 lvl 2.1 | 8.38 lvl 2.2 | 3.85 lvl 1.1 | 5.21 | out by 2.98 | 5.96 | 5.91 | 2.20 |
| cauldron2remix | 3.62 lvl 3.1 | 3.30 lvl 3.0 | 3.56 lvl 2.0 | 4.73 | inside | 8.35 | 8.22 | 2.65 |
| delta | 5.22 lvl 0.5 | 5.14 lvl 0.7 | 8.75 lvl 0.7 | 9.36 | inside | 1.69 | 1.00 | 1.91 |
| ikplus | 2.82 lvl 1.1 | 2.61 lvl 1.2 | 2.86 lvl 1.1 | 2.88 | inside | 1.97 | 1.95 | 2.74 |
| lightforce | 1.60 lvl 0.8 | 1.40 lvl 0.8 | 1.04 lvl 0.5 | 1.06 | out by 0.54 | 1.07 | 1.10 | 0.99 |
| miamivice | 2.22 lvl 1.6 | 2.26 lvl 1.7 | 3.35 lvl 2.7 | 3.35 | inside | 0.94 | 1.09 | 1.46 |
| nemesis | 1.93 lvl 0.9 | 2.20 lvl 0.9 | 2.65 lvl 0.8 | 2.65 | inside | 1.07 | 1.05 | 0.96 |
| sanxion | 1.60 lvl 0.7 | 1.66 lvl 0.7 | 1.25 lvl 0.9 | 1.27 | out by 0.32 | 1.29 | 1.31 | 0.99 |
| shortcircuit | 1.93 lvl 0.7 | 2.08 lvl 0.6 | 2.38 lvl 0.7 | 2.44 | inside | 1.46 | 0.90 | 1.59 |
| starpaws | 1.56 lvl 0.6 | 1.71 lvl 0.7 | 1.54 lvl 0.7 | 1.58 | inside | 1.20 | 1.18 | 0.98 |
| streethawk | 2.56 lvl 2.1 | 2.71 lvl 2.2 | 2.76 lvl 2.2 | 2.77 | inside | 3.07 | 2.98 | 1.07 |
| suntanflower | 1.83 lvl 0.7 | 1.85 lvl 0.7 | 2.01 lvl 0.7 | 2.44 | inside | 0.84 | 0.46 | 1.80 |
| terracresta | 3.37 lvl 1.5 | 3.68 lvl 1.5 | 5.49 lvl 2.4 | 5.76 | inside | 1.71 | 0.73 | 2.04 |
| thundercats | 1.80 lvl 1.4 | 2.11 lvl 1.4 | 2.33 lvl 2.2 | 2.35 | inside | 1.41 | 1.16 | 1.27 |
| **mean, 14 filter tunes** | **2.87 lvl 1.3** | **2.94 lvl 1.3** | **3.13 lvl 1.3** | | 11 inside | **2.29** | **2.07** | |
| spellbound (control) | 1.47 lvl 0.6 | 1.52 lvl 0.5 | 1.05 lvl 0.6 | 1.06 | out by 0.41 | 1.41 | 1.45 | 0.53 |
| switchblade (control) | 1.27 lvl 0.7 | 0.73 lvl 0.5 | 0.78 lvl 0.8 | 0.89 | out by 0.37 | 1.29 | 0.67 | 1.41 |
| zoids (control) | 1.33 lvl 0.6 | 1.37 lvl 0.6 | 1.07 lvl 0.6 | 1.09 | out by 0.25 | 1.26 | 1.31 | 0.47 |
| **mean, 3 controls** | **1.36 lvl 0.6** | **1.21 lvl 0.6** | **0.97 lvl 0.7** | | | **1.32** | **1.14** | |

By chip revision, over the filter tunes:

| | against R2 (3 tunes) | against R3 (14) | against R4 (14) | against R5 (14) |
|---|---|---|---|---|
| SIDOU 6581 | 4.30 | 2.38 | 3.42 | 3.17 |
| reSIDfp 6581 | 4.63 | 2.21 | 3.64 | 2.84 |
| SIDOU 8580 | 6.20 | 2.85 | 4.05 | 2.29 |
| R2 | | 2.84 | 2.63 | 6.84 |
| R3 | 2.80 | | 3.24 | 2.80 |
| R4 | 2.52 | 3.40 | | 4.11 |

Both renders are closest to the R3 recording. The R4 recordings are the farthest from everything,
including the other chips: on delta and terracresta, R4 is 5 to 9 dB from every other source.
That is either the chip or that recording, and it accounts for much of those two tunes' chip
spread. Without R4, delta leaves SIDOU 1.0 dB farther from R3 than reSIDfp (3.57, against 2.54),
which is the largest gap specific to SIDOU in the set.

### Harmonics against reSIDfp

A triangle tone, routed through the filter or past it ("direct"), at four levels: one voice at
sustain 4, 8 and 15, and three voices in unison at 15. H2 to H5 are given relative to the
fundamental. The cutoff is FC 512 on the 6581 and FC 256 on the 8580, where reSIDfp's cutoff f0
measures 1841 Hz and 1778 Hz. The LP tone is f0/5 (368 and 356 Hz) and the BP tone is f0. Each
cell is SIDOU minus reSIDfp in dB, at the four levels.

| | H2 | H4 | reSIDfp's H2 (dB) |
|---|---|---|---|
| 6581 direct | +1.0, +2.0, +6.5, -10.1 | +0.7, +2.0, +3.7, -7.1 | -28.7, -29.7, -34.2, -17.6 |
| 6581 LP | +0.5, -8.5, -8.0, -9.6 | -8.3, -5.2, -6.8, -5.0 | -28.2, -18.7, -17.7, -7.8 |
| 6581 BP | -5.0, -4.1, -6.1, -3.4 | -7.2, +11.2, -9.4, -14.5 | -24.8, -24.6, -20.9, -22.1 |
| 8580 direct | -58, -67, -75, -87 | -51, -54, -66, -78 | -51.6, -45.4, -39.9, -30.2 |
| 8580 LP | -58, -73, -76, -88 | -57, -67, -62, -77 | -53.9, -45.6, -40.0, -31.7 |

- **H3 and H5** agree within 2 dB for one voice, except the 6581 BP's H5, which is 3.7 to 8.9 dB
  lower. With three voices, the 6581 direct H3 is 3.6 dB higher and the 6581 LP's H3 5.3 dB
  lower. On the 8580, H3 and H5 agree within 1.2 dB everywhere.
- **6581.** SIDOU's direct H2 is the waveform DAC's -27.7 dB at every level. Through the LP it
  stays near that with one voice (-27.7 to -25.7 dB) and rises to -17.4 dB with three. reSIDfp's
  LP H2 climbs from -28 to -8 dB, so from sustain 8 up SIDOU's is 8 to 10 dB lower. In the
  direct path, reSIDfp's mixer first cancels part of the DAC's H2 and then adds to it: SIDOU's H2
  is 1 to 6.5 dB higher with one voice and 10 dB lower with three.
- **8580.** SIDOU's voice and mixer path is linear: its even harmonics sit at the -100 dB floor.
  reSIDfp's direct path already has H2 at -52 dB at sustain 4, rising to -30 dB with three voices,
  and its filter adds nothing on top. So the 8580 difference is in the mixer, not the filter.
  That fits SIDOU's 8580 being 0.4 to 1.0 dB farther from the CSG8580R5 recordings than reSIDfp on
  some tunes, but this measurement does not prove the link.
- The real chips' own distortion is not measured here. Against the recordings, SIDOU's missing
  6581 filter distortion does not show as a gap: SIDOU and reSIDfp are equally far from the
  chips.

### Noise in the method

- **Alignment.** Renders and chip recordings are aligned by cross-correlating their log
  spectrograms over ±2 s, in 256-sample steps, then refined by waveform cross-correlation within
  ±256 samples. Envelope and onset correlations were tried first. They sometimes locked onto the
  wrong bar of periodic music, and that moved a tune's distance by up to 3 dB (lightforce 1.6,
  against 4.5). With the spectrogram correlation, a render's lags to a tune's chips match the
  chips' offsets to each other within about 0.02 s, which is far below the 186 ms analysis
  window. The SOASC recordings of one tune are in sync with each other within ±0.1 s, except the
  8580's (up to 1 s off). Remaining risk: tunes with few events (bluemax2001 has 2594 writes in
  30 s).
- **Output stage.** The C64 output stage and the recording chain are unknown. Each comparison fits
  a one-pole high-pass (0 to 160 Hz) and a one-pole low-pass (6 kHz to none) to the test signal,
  chip pairs included. This could also absorb part of a real difference in brightness. The fitted
  low-pass for SIDOU's 8580 is often 13 kHz, against none for reSIDfp. Even with the fit,
  emulation against chip sits 0.25 to 0.4 dB past the chip spread on unfiltered material; that is
  the floor.
- **Chip instances.** There is one chip per revision. R4 is an outlier on delta and terracresta,
  and 8580 has no spread at all, so the "spread" is a sample of two to four chips, not a
  population.
- **Encoding.** 13 of the 17 tunes come only as 224 kbit/s MP3. The 16 kHz band is unreliable:
  -3.4 dB for SIDOU, -4.9 for reSIDfp and -1.7 between chips.
- **Scope.** Only song 1 is used, and only its first 30 s, with the `.sid` files of HVSC #49 (the
  tree the recordings were made from). The renders are at 44.1 kHz with the PAL clock, and the
  harness writes `$D418 = $0F` at cycle 0.

### Reproduce

The recordings and `.sid` files have no licence: they stay outside the repository, and CI
downloads nothing. To lay them out:

- The SOASC mirror is `https://cdimage.debian.org/mirror/media/Oakvalley/soasc/hvsc/`. The
  recordings are `049/{FLAC,MP3}/<path>_T001.sid_<CHIP>.{flac,mp3}` and the tunes are
  `049/SID/<path>.sid`.
- Put them in `<dir>/<tune>/sid/tune.sid` and `<dir>/<tune>/<CHIP>/rec.flac` (or `rec.mp3`).
- Decode each recording to `<dir>/<tune>/<CHIP>/rec.f32` (mono, 44.1 kHz, 32-bit float):
  `ffmpeg -i rec.flac -t 45 -ac 1 -ar 44100 -f f32le rec.f32`.

Then, from `host/`:

```
cargo run --release -p sidou-tests --bin filter-metrics -- chips <dir>
cargo run --release -p sidou-tests --bin filter-metrics -- harmonics
cargo run --release -p sidou-tests --bin filter-metrics -- usage <sid>...
cargo run --release -p sidou-tests --bin filter-metrics -- render <sid> <out dir>
```

On an M-series Mac with four threads, `chips` takes about 3 min for the 17 tunes and
`harmonics` about 2 s.
