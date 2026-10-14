<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0004: Effect algorithms for the Distortion, Modulation and FX slots

- **Status:** accepted
- **Date:** 2026-10-15

## Context

M1c Task 5 puts three slots after each chip: Distortion → Modulation → FX,
with stereo from Modulation on. Each slot runs one algorithm at a time. The
constraints:
- pi32v2 at 240 MHz (budgeted) or 360 MHz (measured by Melodee), with a
  single-precision FPU, no libm and about 512 KB of SRAM shared with
  everything (`docs/hardware.md` on `m1a-device`);
- `POOL` is 360 KiB, and the M1a firmware uses 19.1 KiB of it
  (`_pool_end`);
- the hardware float-biquad EQ isn't in fm1-emu, so the gate can't count it.

Measured on pi32v2 (fm1-emu, `build/fm1/*_budget_*.json`):
- the SID core costs 366 instructions per sample per chip on the M0 workload
  (budget 900, worst case about 600);
- the Task 1 chain (crush, tape, wow, CRT, noise) costs 455 at worst.

We don't have per-block costs. Shared by op count, the 455 splits roughly
into about 60 of frame overhead (int16 conversions, smoothing), crush ~30,
tape ~110 (residual ADAA, branchy), wow ~120 (Hermite read, two LFOs,
drift), CRT ~80 and noise ~30. Below, "≈" marks an estimate calibrated
against these figures. Unit costs: a linear delay read ~14, a Hermite read
~30, an int16 store with rounding ~12, and `sid_fx_sin` ~10. The emulator
gate in Task 5 replaces every estimate with a measurement.

## Sources reviewed

- J. Dattorro, "Effect Design":
  - [Part 1: Reverberator and Other Filters](https://ccrma.stanford.edu/~dattorro/EffectDesignPart1.pdf),
    JAES 45(9), 1997;
  - [Part 2: Delay-Line Modulation and Chorus](https://ccrma.stanford.edu/~dattorro/EffectDesignPart2.pdf);
  - [Part 3: Oscillators: Sinusoidal and Pseudonoise](https://ccrma.stanford.edu/~dattorro/EffectDesignPart3.pdf).
- J. O. Smith, [*Physical Audio Signal Processing*](https://ccrma.stanford.edu/~jos/pasp/)
  (CCRMA online), including the
  [Freeverb analysis](https://ccrma.stanford.edu/~jos/pasp/Freeverb.html).
- U. Zölzer (ed.), [*DAFX: Digital Audio Effects*, 2nd ed.](https://www.wiley.com/en-us/DAFX%3A+Digital+Audio+Effects%2C+2nd+Edition-p-9780470665992),
  Wiley 2011.
- W. Pirkle, [*Designing Audio Effect Plugins in C++*, 2nd ed.](https://www.routledge.com/Designing-Audio-Effect-Plugins-in-C-For-AAX-AU-and-VST3-with-DSP-Theory/Pirkle/p/book/9781138591936),
  Routledge 2019.
- V. Zavalishin, *The Art of VA Filter Design*, rev. 2.1.0
  ([NI-hosted PDF](https://www.native-instruments.com/fileadmin/ni_media/downloads/pdf/VAFilterDesign_2.1.0.pdf)).
  Web search lists this link, but it returned 404 to a direct fetch on
  2026-10-15.
- Valhalla DSP, "Getting Started With Reverb Design", parts 1–4 (2021;
  [part 1](https://valhalladsp.com/2021/09/20/getting-started-with-reverb-design-part-1-dev-environments/embed)).
  The site refuses scripted fetches, so this is confirmed by search only.
- Signalsmith Audio, ["Let's Write a Reverb"](https://signalsmith-audio.co.uk/writing/2021/lets-write-a-reverb/).
- ADAA:
  - Parker, Zavalishin and Le Bivic, ["Reducing the Aliasing of Nonlinear Waveshaping Using Continuous-Time Convolution"](https://www.dafx.de/paper-archive/2016/dafxpapers/20-DAFx-16_paper_41-PN.pdf),
    DAFx-16;
  - Bilbao, Esqueda, Parker and Välimäki, ["Antiderivative Antialiasing for Memoryless Nonlinearities"](https://ieeexplore.ieee.org/document/7865908),
    IEEE SPL 24(7), 2017;
  - J. Chowdhury, [ADAA notebook](https://ccrma.stanford.edu/~jatin/Notebooks/adaa.html).
- J. Chowdhury:
  - ["Real-Time Physical Modelling for Analog Tape Machines"](https://dafx.de/paper-archive/details/Z5Ow_E5YHVxvndTAyeEuzA),
    DAFx 2019;
  - ["chowdsp_wdf: An Advanced C++ Library for Wave Digital Circuit Modelling"](https://arxiv.org/abs/2210.12554),
    2022.

## Open code and licences

All checked on 2026-10-15 against the files linked.

| Project | Licence (SPDX) | GPL-3.0 compatible | Notes | Use |
|---|---|---|---|---|
| Mutable Instruments eurorack: Clouds, Rings, Elements, Plaits, Beads `dsp/fx/*` | MIT ([README](https://github.com/pichenettes/eurorack/blob/master/README.md): STM32 projects MIT, AVR projects GPL-3.0; per-file headers, e.g. [rings/dsp/fx/reverb.h](https://github.com/pichenettes/eurorack/blob/master/rings/dsp/fx/reverb.h)) | yes ([FSF: Expat](https://www.gnu.org/licenses/license-list.html#Expat)) | "Mutable Instruments" is a registered trademark; the README asks derivatives not to use the name | **port** the reverb topology and constants; follow the ensemble topology |
| [stmlib](https://github.com/pichenettes/stmlib/blob/master/LICENSE) | MIT | yes | | reference only |
| [DaisySP](https://github.com/electro-smith/DaisySP/blob/master/LICENSE) | MIT | yes | chorus, flanger, phaser, overdrive and tremolo are in this repo | reference only |
| [DaisySP-LGPL](https://github.com/electro-smith/DaisySP-LGPL/blob/main/LICENSE) | LGPL-2.1 | yes ([FSF](https://www.gnu.org/licenses/license-list.html#LGPLv2.1)) | `reverbsc`, `bitcrush` and `fold` live here; ReverbSc reserves `float aux_[98936]` (387 KiB) | **not used**: too large, and we avoid the relink obligation |
| [Airwindows](https://github.com/airwindows/airwindows/blob/master/LICENSE) | MIT | yes | double-precision plugin code | listening reference only |
| Freeverb (Jezar) | public domain ([readme mirror](https://github.com/sinshu/freeverb/blob/master/readme.txt); [PASP](https://ccrma.stanford.edu/~jos/pasp/Freeverb.html)) | yes ([FSF](https://www.gnu.org/licenses/license-list.html#PublicDomain)) | 16 combs and 8 allpasses for stereo; metallic on bright tones | rejected |
| CloudSeed ([original](https://github.com/ValdemarOrn/CloudSeed/blob/master/license.txt), [CloudSeedCore](https://github.com/GhostNoteAudio/CloudSeedCore/blob/master/LICENSE)) | MIT | yes | many diffuser stages and multitap lines: far over budget | rejected |
| [chowdsp_utils](https://github.com/Chowdhury-DSP/chowdsp_utils/blob/master/LICENSE.md) | per module: `chowdsp_math`, `_buffers` and `_simd` are BSD-3-Clause; `chowdsp_waveshapers`, `_reverb`, `_filters` and `_dsp_utils` say "GPLv3" (-only or -or-later is not stated) | yes; GPL-3.0-only would make a ported file GPL-3.0-only (allowed by LICENSING.md, but avoided) | | reference only |
| [chowdsp_wdf](https://github.com/Chowdhury-DSP/chowdsp_wdf/blob/main/LICENSE) | BSD-3-Clause | yes ([FSF](https://www.gnu.org/licenses/license-list.html#ModifiedBSD)) | C++14 templates | reference only |
| [ChowTape / AnalogTapeModel](https://github.com/jatinchowdhury18/AnalogTapeModel/blob/master/LICENSE) | GPL-3.0 (version suffix not stated; the iOS build is also MPL-2.0) | yes | a hysteresis solver with oversampling, many times our budget | rejected; the existing tape stays |

**No incompatible licence was found.** The only non-permissive parts are:
- DaisySP-LGPL (LGPL-2.1);
- ChowTape and the GPL modules of chowdsp_utils (GPL-3.0).

We take code from neither.

## Decision

Each choice below lists:
- the algorithm and its source;
- **P** (port: code translated to our C) or **R** (reimplemented from the
  published description);
- the cost in pi32v2 instructions per sample (≈ estimated);
- the memory it needs.

### Distortion (mono)

| Entry | Algorithm, source | | Cost | Memory |
|---|---|---|---|---|
| Tape | existing `fx_tape.h`: emphasis, biased cubic clip, residual ADAA1 | — | ≈110 | 48 B |
| Overdrive | Mid-hump pre-filter (x + k·one-pole HP at ~700 Hz), the tape's cubic clip with **residual ADAA1** (shared, not copied), Amount → gain law after Plaits `Overdrive`, Param A = post tone (one-pole LP), Param B = bias | R; gain law follows Plaits (MIT) | ≈100 | 40 B |
| Fuzz | Pre low-pass (~7 kHz, one-pole) → hard clip u ↦ clamp(u, ±1) with closed-form ADAA1 of the residual (piecewise linear means, no division off the knee), Param B = bias (gated "starved" asymmetry), Param A = blend with full-wave rectified \|x\| (octave-up, also ADAA1) | R from Parker 2016 / Bilbao 2017, Zölzer (nonlinear processing chapter) | ≈120 | 40 B |
| Crush | existing `fx_crush.h` | — | ≈30 | 12 B |
| Lo-fi | existing `fx_crt.h` + `fx_noise.h` | — | ≈110 | ~1 KB tables |

**Anti-aliasing: first-order ADAA, no oversampling.**
- **Why not oversampling:** 2× oversampling needs a halfband up and down
  (~2 × 10 MACs) plus a second nonlinearity evaluation, about 60–80 extra.
  The hardware EQ could filter, but it is block-based and not emulated.
- **What ADAA keeps:** the residual form keeps small signals flat to
  Nyquist and costs only a branchy mean.
- **Fuzz:** the pre-filter also tames the SID's saw and pulse highs, the
  main source of aliasing.
- **When to revisit:** if the alias test (a 5 kHz saw at full drive, with
  aliases at −60 dB against the reference) fails, Fuzz gets 2×
  oversampling within its 150 budget.

**Why these suit a SID:**
- The SID's pulse is already square, so clipping it adds little. The drives
  act mainly on resonant filter sweeps, triangle and mixes.
- The mid-hump and the bias give overdrive and fuzz audibly different
  odd/even balances.

### Modulation (mono in, stereo out)

| Entry | Algorithm, source | | Cost | Memory |
|---|---|---|---|---|
| Chorus/ensemble | One float line, 3 taps per side at 120° phases, slow (~0.6 Hz) + fast (~6 Hz) LFO sum (Solina-style); linear reads (`fx_delay.h`). Dattorro Part 2; topology as Plaits `ensemble.h` | R (Plaits MIT as reference) | ≈130 | 1,024 float = 4 KiB |
| Flanger | Hermite read, 0.1–10 ms sweep, feedback ±0.9 through a soft clip; R tap at 90° LFO offset | R from Dattorro Part 2 | ≈100 | 512 float = 2 KiB |
| Phaser | 6 first-order allpasses with a shared coefficient per side, coefficient from an LFO over a log-frequency table (CRT-style interpolation), feedback; L/R LFOs in quadrature | R from Zölzer, Zavalishin (TPT allpass) | ≈110 | 64 B |
| Tremolo/auto-pan | `sid_fx_sin` LFO, shape (sine→square by soft clip), gain or equal-power pan via sin/cos approximations | R | ≈40 | 16 B |
| Wow & flutter | existing `fx_wow.h`, mono to both sides | — | ≈120 | 4 KiB |

**Why these suit a SID:**
- The SID is mono and bone dry. An ensemble widens it the way 80s string
  machines widened their dividers.
- The flanger and phaser emphasise its sweeping pulse width.

### FX (stereo in; the wet path takes (L+R)/2, the dry path stays stereo)

| Entry | Algorithm, source | | Cost | Memory |
|---|---|---|---|---|
| Delay | One int16 line; mono = tap at T; ping-pong = taps at T (L) and 2T (R), feedback from 2T (equal-time ping-pong needs only one line); one-pole damping and a soft clip in the feedback; linear read (Hermite only while T glides) | R | ≈90 | 65,536 int16 = **128 KiB** (1.49 s mono, 0.74 s per side ping-pong) |
| Reverb light | MI Rings/Clouds reverb: 4 input allpasses, then a two-branch loop (2 allpasses + delay each) with a one-pole damping per branch, 2 slow LFO-modulated reads, all lines in one ring with one write pointer (FxEngine style); delays scaled 48→44.1 kHz | **P** (MIT) | ≈260 | ~19.9k samples in a 32,768 int16 ring = 64 KiB |
| Reverb lush | Dattorro plate (Part 1, fig. 1): pre-delay, 4 input diffusers, a figure-8 tank with 2 modulated allpasses, 2 fixed allpasses, 4 delays, bandwidth and damping one-poles, 7 output taps per side; delays scaled ×1.48 to 44.1 kHz, then ×0.98 to fit the same 32,768 ring | R from the paper | ≈380 | 64 KiB (shared) |

- **Why these suit a SID:**
  - Short, bright envelopes need a dense, smooth tail without a metallic
    ring, which rules out Freeverb.
  - The modulated tank (MI and Dattorro both have one) smears the SID's
    perfectly stable pitch.
  - Rhythmic ping-pong is the classic chiptune echo.
- **Light and lush:** light is a proven embedded design (Clouds and Rings
  run it on STM32F4 modules, at 32 kHz and 48 kHz by their LFO settings).
  Lush ships only if the gate measures it within the FX budget with two
  chips.
- **Switching inside the FX slot:**
  - Delay and reverb share one 128 KiB pool, because only one runs at a
    time.
  - A switch fades the old wet path out over 20 ms, clearing the ring
    incrementally (~40 extra instructions per sample during the fade), then
    fades the new one in.
  - The dry path is continuous, so there is no click; the old tail is cut.
  - Running both at once would double the slot's cost.

## Slot budgets

At 240 MHz and 44.1 kHz: 240e6 / 44,100 ≈ **5,442 instructions per sample**
per core, at 1 IPC. The design doc reserves ≤ 2,700 (50 %) for audio, and
the rest goes to UI, MIDI and the sequencer.

| Block | Budget | Heaviest entry (≈) |
|---|---|---|
| Distortion | 150 | Fuzz 120 |
| Modulation | 200 | Chorus 130 |
| FX | 450 | Lush 380 |
| Slot frame: int16↔float, 4 smoothed parameters × 3 slots, mix, crossfade, stereo out | 100 | — |
| **Slots total** | **900** | ≈730 |

**Worst cases:**

| Case | SID | Slots | 6502 (future) | ISR overhead (estimate) | Total | Share of one core |
|---|---|---|---|---|---|---|
| Measured SID (worst ~600 × 2) | 1,200 | 900 | 400 | 100 | **2,600** | 48 % |
| SID at its budget (900 × 2) | 1,800 | 900 | 400 | 100 | 3,200 | 59 % |

- The measured case fits the 2,700 audio target with 100 to spare. Without
  the future 6502, SID plus effects is 2,100, inside Task 1's 2,300 cap.
- The budget case is over the target and invokes the design doc's cut
  order (chip B oversampling first).
- 360 MHz (8,163 per sample) would hold even the budget case at 39 %, but
  we spend that only after cycles are measured on a unit.
- The gate asserts each slot budget separately and the 2,300 total for two
  chips.

## SRAM budget

| Buffer | Size |
|---|---|
| FX pool (delay, or either reverb) | 128 KiB |
| Modulation lines (chorus 4 KiB, flanger 2 KiB; wow's 4 KiB is already in `sid_fx`) | 6 KiB new |
| **Effects total** | **≈ 138 KiB of POOL's 360 KiB** |

- After the M1a firmware (19.1 KiB) and a future 64 KiB C64 RAM image,
  about 139 KiB of POOL remains.
- Effect buffers are int16 wherever they're long. Float is used only where
  a short line is read many times (modulation).

## Attribution

- **CREDITS.md**, under "Signal processing":
  - Emilie Gillet / Mutable Instruments: the reverb ported from
    Rings/Clouds, and the ensemble topology from Plaits;
  - Jon Dattorro: the plate reverb, and the chorus and flanger designs;
  - Julian Parker, Vadim Zavalishin, Efflam Le Bivic, Stefan Bilbao, Fabián
    Esqueda and Vesa Välimäki: ADAA;
  - Jatin Chowdhury: tape-modelling background.
- **LICENSING.md**, third-party table: one row for the ported reverb, with
  origin `pichenettes/eurorack` at a pinned commit, licence MIT, use
  "compiled into the firmware". That ported file:
  - keeps Emilie Gillet's copyright line and the MIT notice in its header,
    as dual `SPDX-FileCopyrightText` lines;
  - carries `SPDX-License-Identifier: MIT`;
  - adds the MIT text to `LICENSES/MIT.txt` (REUSE).
- **Not under the MI name:** the reverb is never called by the MI module's
  name in the UI (the README asks this).
- **Reimplementations from papers** carry only our header and a comment
  citing the paper. "Follows" (as for the Plaits gain law) needs no licence
  row, just the credit.

## Consequences

- Task 5 builds:
  - Overdrive, Fuzz, Chorus, Flanger, Phaser, Tremolo/auto-pan, Delay and
    the two reverbs;
  - a shared residual-ADAA helper taken out of `fx_tape.h`.
- The emulator gate measures every algorithm and replaces each "≈" here.
  An entry that is over its slot budget gets cheaper (fewer taps or stages)
  before the budget moves.
- Not verified:
  - the Zavalishin and Valhalla links (they refuse scripted fetches; found
    by search);
  - whether chowdsp's and ChowTape's "GPLv3" is -only or -or-later. This
    doesn't matter, because we take no code from them.

## Measured (Task 5)

The emulator gate (`slots_budget.rs`, pi32v2 in fm1-emu, every control
moving all the time) replaces the estimates above. Instructions per sample:

| Entry | Slot | Measured | Estimate | Budget |
|---|---|---|---|---|
| Tape | Distortion | 116 | ≈110 | 150 |
| Overdrive | Distortion | 148 | ≈100 | 150 |
| Fuzz | Distortion | 107 | ≈120 | 150 |
| Crush | Distortion | 52 | ≈30 | 150 |
| Lo-fi | Distortion | 115 | ≈110 | 150 |
| Chorus/ensemble | Modulation | 183 | ≈130 | 200 |
| Flanger | Modulation | 158 | ≈100 | 200 |
| Phaser | Modulation | 154 | ≈110 | 200 |
| Tremolo | Modulation | 101 | ≈40 | 200 |
| Auto-pan | Modulation | 155 | ≈40 | 200 |
| Wow & flutter | Modulation | 143 | ≈120 | 200 |
| Delay | FX | 144 | ≈90 | 450 |
| Ping-pong | FX | 161 | ≈90 | 450 |
| Reverb light | FX | 373 | ≈260 | 450 |
| Reverb lush | FX | 421 | ≈380 | 450 |
| Slot frame | — | 83 | — | 100 |
| Heaviest of each together (overdrive, chorus, lush) | — | 836 | ≈730 | 900 |

- One chip plus the slots: 1,202 (22 % of a 240 MHz core).
- Two chips plus the slots: 1,576 (29 %), within Task 1's 2,300.
- RAM: 136,328 bytes. That is the 128 KiB pool plus 5,256 bytes of slot
  state (the Modulation union is the largest part, at 4.1 KiB).

How the frame and the modulations got within budget (the first build
measured a 256 frame, chorus 289 and lush 829):
- the frame runs in 16-sample chunks on its own clock, controls following
  their one-pole as straight lines between chunk starts;
- each algorithm has its own loop in the steady state;
- the modulations evaluate their LFOs every 16 samples and interpolate.

Deviations from the decision above:
- **Overdrive and fuzz use ADAA of the whole clip, not of its residual.**
  - The residual form adds u back undelayed. Far past the knee, u and the
    residual's half-sample lag no longer cancel, and the output approaches
    1 + (u - p) / 2: at 25-100x gain, garbage.
  - The tape keeps the residual form (its drive is 3x at most), so small
    signals stay flat.
- **The alias target of -60 dB is not reachable at full drive.**
  - Measured with an 8 kHz sine and a 5 kHz band-limited saw at half scale:
    overdrive's worst alias below 15 kHz is -28.9 / -27.7 dBc, fuzz's
    -28.5 / -31.0 dBc.
  - An f64 model of the hard clip at these gains gives -14 to -22 dBc
    without ADAA, and -36 to -54 dBc with 2x oversampling (63-tap
    half-bands) on top, well over Fuzz's budget.
  - The tests pin -25 dBc.
- **Lush stores floats.**
  - Its lines are float in the 128 KiB pool (32,768 samples), scaled
    x1.48 then x0.98 as planned.
  - int16 conversions and clamps made it 829. Float costs 421.
  - There is no room for a pre-delay, so B is the input bandwidth.
- **Both reverbs are capped at an RT60 of about 8 s.**
  - Clouds' time goes to 0.98 (tails of minutes); light is capped at 0.82.
  - Dattorro's decay is capped at 0.85.
- **The delay reads linearly.** Its time glides at most 0.5 samples per
  sample (half that in ping-pong), so no Hermite read is needed.
- **The light reverb's LFOs** are this project's phase-accumulator cosine,
  stepped every 32 samples like the original, instead of stmlib's
  oscillator.
