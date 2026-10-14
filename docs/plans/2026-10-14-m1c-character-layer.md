# M1c: Character layer, implementation plan

<!-- REUSE-IgnoreStart -->

> **For Claude:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> to implement this plan task by task.

**Goal:** the amp-modeller-style "chip character" from design doc section 4.

1. A post-chip colour chain: tape saturation, wow and flutter, noise floor,
   CRT/TV low-pass with hum, and bit-crush. It runs after the SID mixer.
2. Named chip profiles with in-chip character controls: age/heat, DAC
   sloppiness, output thump and filter bleed. These are built on M1b's
   generated tables.
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

### Task 3: Chip profiles and in-chip character

Do this after M1b has merged.

- `sid_profile`: per-model tables and parameters from M1b (DAC, cutoff
  curve, 6581 non-linearity), plus named profiles that `sidou-tablegen`
  generates from reSIDfp settings:
  - 6581 R2, 6581 R3, 6581 R4AR, 8580 R5 and "worn 6581";
  - they use `setFilter6581Curve`/`Range`, `setFilter8580Curve`,
    combined-waveform strength and DAC leakage where reSIDfp exposes them.
- **Live character controls:**
  - **Age/heat:** filter curve drift plus non-linearity amount.
  - **DAC sloppiness:** blends towards a leakier DAC table.
  - **Output thump:** the size of the volume-register DC step.
  - **Filter bleed:** unfiltered voices leaking into the filter.

  Each control blends between neighbouring precomputed tables or parameters.
  Nothing is rebuilt on the audio thread.
- **Tests:**
  - each profile at zero character matches reSIDfp for that chip setting,
    within the M1b tolerances;
  - each control's effect is measurable and its direction is right;
  - the blends are click-free.
- **Gate:** worst-case SID + profile + effects within budget.

### Task 4: CHIP view

- The design's CHIP view on the LCD: a profile picker, the character
  controls and the effects, set separately for chip A and chip B (chip B
  arrives in M2; show A only until then).
- The knob and button mapping is documented in `docs/reference/controls.md`.
- **Tests:** e2e LCD snapshots and control round trips.

<!-- REUSE-IgnoreEnd -->
