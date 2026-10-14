# M1c: Character layer, implementation plan

<!-- REUSE-IgnoreStart -->

> **For Claude:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> to implement this plan task by task.

**Goal:** the amp-modeller-style "chip character" from design doc section 4.

1. A post-chip colour chain: tape saturation, wow and flutter, noise floor,
   CRT/TV low-pass with hum, and bit-crush. It runs after the SID mixer.
2. The SID analogue model's own parameters (filter curves and
   non-linearity, DACs, combined-waveform strength, output stage) as live,
   continuous controls. Named chip profiles become presets of them, and
   macro knobs move several at once (Task 3).
3. All of these can be changed live (knobs and MIDI), with smoothing.

**Constraints:**
- The audio path is portable C11 with no allocation and no libm (the
  firmware has none). Float is fine.
- **Bypass is exact.** With every control at zero, the output is bit-exact
  with the chain bypassed. The golden hashes stay put, and reSIDfp accuracy
  isn't affected.
- **Cost:** the effects chain costs at most 500 instructions per sample with
  everything on. Check it with an emulator gate scenario (design doc section
  8). The worst-case SID plus effects for 2 chips must stay ≤ 2,300.
- **Parameter changes are smoothed.** No zipper noise and no clicks when a
  control moves. Test this.

**Rules:** the same conventions as M0, M1a and M1b:
- SPDX headers on every file;
- the Co-Authored-By trailer;
- stage only your own files;
- run host commands from `host/` with the Homebrew rustup PATH;
- judge every check by its exit code;
- tolerances only ever get tighter.

---

### Task 1: The post-chip effects chain (`core/fx/`)

This task has no dependency on M1b. Work in the worktree `../sidou-m1c` on
branch `m1c-character`, which starts from `main`.

- **API** in `core/fx/fx.h`:
  ```c
  typedef struct { ... } sid_fx;            /* state + current/target params */
  typedef struct {
      float tape_drive;    /* 0..1  */
      float wow;           /* 0..1  depth */
      float flutter;       /* 0..1  depth */
      float noise;         /* 0..1  hiss level */
      float crt;           /* 0..1  low-pass amount + hum level */
      float crush;         /* 0..1  sample-rate / bit-depth reduction */
  } sid_fx_params;
  void sid_fx_init(sid_fx*, uint32_t sample_rate, uint32_t seed);
  void sid_fx_set(sid_fx*, const sid_fx_params*);   /* targets; smoothed */
  void sid_fx_process(sid_fx*, int16_t* io, uint32_t n);
  ```
- **Effects, in a fixed order:** crush → tape → wow and flutter → CRT → noise.
  - **Tape:** an asymmetric soft saturation with a small DC bias and a
    pre-emphasis shelf, so the high end compresses first. Drive and makeup
    gain are tied, so loudness stays roughly constant.
  - **Wow and flutter:** a fractional delay line with linear interpolation,
    up to about 20 ms. Wow is a slow sine (~0.5 Hz) plus a smoothed random
    drift. Flutter is about 6–10 Hz. The modulation depth is in cents.
  - **CRT/TV:** a one-pole or biquad low-pass that tightens as the amount
    rises, plus quiet 50 Hz mains hum with harmonics.
  - **Noise:** shaped hiss from a deterministic LFSR seeded at init, so host
    and target produce the same output.
  - **Crush:** sample-and-hold decimation plus reduced bit depth.
- **Smoothing:** a one-pole on every parameter, at about 10 ms.
- **Bypass:** when every target and current value is 0, return immediately,
  without touching `io` or the state.
- **Tests** (Rust, `sidou-tests`):
  - exact bypass;
  - each effect's characteristic: tape THD rises with drive and peak gain
    stays bounded; wow measured as frequency modulation; the CRT low-pass
    −3 dB point and hum frequency; noise level and spectrum; crush
    quantisation;
  - stability under extreme settings;
  - zipper-free smoothing (maximum sample-to-sample step during a jump);
  - determinism (the same seed gives the same output).
- **Emulator gate:** a scenario with every effect at maximum on top of the
  triad workload. Report the effects' own cost and assert the budgets above.
  Host and target must stay bit-exact.
- **Commit:** `feat(fx): post-chip colour chain (tape, wow/flutter, CRT, noise, crush)`.

### Task 2: Effects in the live instrument and on the device

Do this after M1a has merged to `main` and this branch has been rebased.

- `live` holds the effect parameters. The device audio ISR runs
  `sid_fx_process` after `sid_render`. Parameter changes cross the register
  ring as pseudo-register entries (as `LIVE_REG_RESET` already does) or
  through a separate small parameter mailbox; pick one and justify it.
- **Controls:** a temporary panel mapping until the CHIP view exists in Task
  4, for example a SHIFT-style layer on K1–K4. Also CC numbers in the M1d
  CC map's range, documented.
- **Tests:** an e2e test in fm1-emu turns tape and CRT up and checks the
  spectrum change. The idle and worst-case main-loop and ISR costs still
  pass.

### Task 3: The SID analogue model as live parameters

Do this after M1b has merged. The user's intent is direct control over the
analogue modelling parameters, not just named presets. So:

- **`sid_analog` parameter set** (per chip, continuous, documented ranges).
  At minimum:
  - **filter:** 6581 curve and range, 8580 curve, resonance mapping,
    6581 non-linearity amount (opamp/VCR), voice-DC bias into the
    filter, filter bleed;
  - **DACs:** waveform and envelope DAC non-linearity (kink factor) and
    leakage;
  - **waveforms:** combined-waveform strength (continuous between
    reSIDfp's WEAK, AVERAGE and STRONG) and noise write-back on/off;
  - **output stage:** voice DC offsets (the volume "thump"), external
    filter corner frequencies, mixer saturation.
- **Where the values come from:** `sidou-tablegen` generates the tables over
  a grid of these parameters from reSIDfp's own settings
  (`setFilter6581Curve`/`Range`, `setFilter8580Curve`,
  `setCombinedWaveforms`, DAC leakage and so on). Parameters that reSIDfp
  doesn't expose are modelled directly.
- **How the device applies a change:** the main loop interpolates the
  affected tables and coefficients and swaps them in at a block boundary,
  the same pattern as the chip-model toggle. Nothing heavy runs on the audio
  thread. Changes are smoothed or crossfaded, so they don't click.
- **Three layers of control:**
  1. *Expert:* every parameter directly, as panel entries and MIDI
     NRPN/CC (in the M1d map).
  2. *Profiles:* 6581 R2, R3, R4AR, 8580 R5 and "worn 6581" are named
     presets of the parameter set. Users can also save their own.
  3. *Macros:* age/heat, DAC sloppiness, thump and bleed each move
     several parameters along curated paths.
- **Tests:**
  - each profile at default settings matches reSIDfp for that chip setting,
    within M1b's tolerances;
  - each parameter's effect is measurable, goes the right way, and stays
    monotonic over its range;
  - interpolation between grid points is smooth (no discontinuity at grid
    boundaries);
  - parameter sweeps don't click;
  - the bypass and default paths stay bit-exact with the golden hashes.
- **Gate:** the worst case of SID + parameter changes + effects stays within
  budget. Also budget the main-loop cost of recomputing tables.
- **Memory:** report the flash cost of the grids and the RAM cost of the
  active and staging tables per chip.

### Task 4: CHIP view

- The design's CHIP view on the LCD: a profile picker, the character
  controls and the effects, set separately for chip A and chip B (chip B
  arrives in M2; show A only until then).
- The knob and button mapping is documented in `docs/reference/controls.md`.
- **Tests:** e2e LCD snapshots and control round trips.

<!-- REUSE-IgnoreEnd -->

### Task 5: Effect slots: Distortion, Modulation, FX (stereo)

**Signal chain:** chip (with its character) → **Distortion** → **Modulation**
→ **FX** → stereo out. Each slot runs one algorithm at a time and can be
bypassed exactly. The slot order is fixed.

| Slot | Algorithms |
|---|---|
| Distortion | Tape saturation, Overdrive, Fuzz, Bit-crush/decimate, Lo-fi (CRT/TV low-pass + hum + noise) |
| Modulation | Chorus/ensemble, Flanger, Phaser, Tremolo/auto-pan, Wow & flutter |
| FX | Delay (mono or ping-pong; free or tempo-synced once M2's clock exists), Reverb (light and lush variants) |

- **Reuse Task 1's blocks:** tape, crush, Lo-fi, and wow & flutter are
  rearranged into slots, not rewritten. Task 1 keeps each effect as a
  separate block with its own state and parameters.
- **Parameters per slot:** algorithm, Amount, Param A, Param B, Mix. All are
  smoothed, and switching algorithm crossfades (no click).
- **Stereo:** the chip path stays mono up to Modulation. Chorus, auto-pan,
  ping-pong delay and reverb produce stereo. The audio ISR writes true
  L and R.
- **Memory:** delay and reverb buffers live in SRAM pools sized from a
  documented budget (for example 1 s of mono delay is 88 KB at 16-bit).
  Report the total. Reverb's light variant must fit two-chip worst-case
  budgets. Lush is allowed only if its measured cost leaves headroom.
- **Tests** for each algorithm:
  - its characteristic (THD for drives, modulation rate and depth,
    delay time and feedback decay, reverb RT60 and stability);
  - exact bypass;
  - click-free algorithm switches and parameter jumps;
  - stereo correctness (mono input gives the expected L/R decorrelation);
  - the same output on host and target.
- **Emulator gate:** worst-case scenarios with every slot on (heaviest
  algorithm per slot), for one and two chips. Assert the budgets and report
  the cost of each algorithm.
- **Controls:** the device maps the selected slot to K1–K4 (Amount, A, B,
  Mix), with a button to choose the slot and algorithm. There are MIDI
  CC/NRPN in the M1d map. The slot states are saved in presets and songs
  (M2 adds per-step parameter locks).
- **Commit:** `feat(fx): distortion, modulation and FX slots with stereo output`.
