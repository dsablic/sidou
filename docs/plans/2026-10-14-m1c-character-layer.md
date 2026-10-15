# M1c: Character layer, implementation plan

<!-- REUSE-IgnoreStart -->

**Status: complete** (2026-10-15). Tasks 1-5 are on `main`; Task 4, the CHIP view, closed it.
What moved on is listed under each task's results and in CHANGELOG.md, "M1 status".

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

*Amended during implementation:* Task 5's three slots replaced the single
colour chain, so this task wires the slots, not `sid_fx_process`, and its
tests use OVERDRIVE and the reverb, not tape and CRT.

- `live` holds each slot's algorithm and Amount/A/B/Mix. The device audio
  ISR runs `sid_slots_process` after `sid_render` and writes true L/R.
  Parameter changes cross the register ring as pseudo-register entries
  (`0xA0 | slot << 3 | field`, as `LIVE_REG_RESET` already does), so they
  arrive in order with the same pass's SID writes and are never dropped; the
  drain hands them to `fx_live`.
- **Controls:** a temporary panel mapping until the CHIP view exists in Task
  4: EDIT held turns K1–K4 into the selected slot's Amount, A, B and Mix, SCL
  selects the slot, ENV steps its algorithm. Also CC 102–116 (slot × field)
  in the M1d CC map's range, documented in `docs/reference/controls.md`.
- **Tests:** fm1-emu e2e tests: the panel turns OVERDRIVE on and the wave
  clips; CCs turn the reverb on in stereo with a tail; every slot's heaviest
  algorithm fits the audio budget and runs only from SRAM. The idle and
  worst-case main-loop and ISR costs still pass.

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
  - *Filter curves (from M1b):* `gen/filter.h` holds one row per curve
    setting (`FILTER_CURVES` in `sidou-tablegen`; reSIDfp's defaults are
    row 0), and `sid_set_filter_curve(chip, curve)` (`sid.h`) selects one
    per chip: it rebuilds the chip's cutoff and damping tables in the main
    loop, never in the audio interrupt, and the curve applies at the next
    `$15`-`$17` write. Continuous control interpolates between rows or
    replaces this with the swap below.
  - *reSIDfp's 6581 range is process-global* (`setFilter6581Range` changes
    every 6581 in the process): measure 6581 settings one after another,
    never in concurrent test threads, unless the tests share a lock or run in
    their own binaries; `filter_fit::fit_setting` sets the range explicitly
    each time.
  - *"Worn 6581" needs defining* before it can be generated: which reSIDfp
    settings (curve, range, DAC leakage) or SIDOU-only parameters it is, and
    from what reference.
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

**Results after Task 3** (2026-10-15; decision record
[0006](../decisions/0006-chip-profiles.md) has the detail; revised after review the same
day). As scoped for this round: the profiles and the four macro controls. Deferred: the
expert layer (every analogue parameter as its own control), to the CHIP view's panel
entries (Task 4) and the M1d map's NRPN rows after 2/4; user-saved profiles, which need
storage, to M2.
- **Profiles** (`sidou-tablegen` `profiles.rs` -> `gen/profiles.h`, and their filter rows
  in `gen/filter.h`): 6581 R4AR and 8580 R5 are reSIDfp's defaults; 6581 R2 (curve 0.58,
  WEAK combined waveforms, sampled from an R2) and R3 (curve 0.42, AVERAGE, sampled from
  an R3) have SIDOU-chosen filter curves, since no per-revision filter values are
  documented; "worn 6581" is artistic (curve 0.35 at range 0.4, STRONG, a 2.30 ladder with
  three times the leakage, non-linearity 1.5). Age/heat drifts towards a 6581 row at curve
  0.25, range 0.3, and an 8580 row at curve 0.75 (the 8580's cutoff rises as its curve
  falls, against reSIDfp's header). All three of the 6581's combined-waveform profiles are
  now compiled by default (the R3 and the R4AR share AVERAGE, sampled from an R3); the
  8580's WEAK and STRONG, which no profile plays, stay out.
- **"Worn 6581" defined** as above, and the 6581 range's process-global state handled:
  `filter_fit::apply_range` before every 6581 measurement, `tests/profiles.rs` in its own
  binary with a lock.
- **Against reSIDfp at zero character:** R4AR and R5 within M1b's tolerances; R2 and R3
  up to 0.22 and 0.58 dB past M1b's passband-gain range (the 6581 mix and non-linearity
  were fitted at the default curve); worn 5.9 / 5.7 / 5.2 dB in shape, by design. Combined
  waveforms read for read.
- **The 6581's lowest cutoff** now follows the cutoff table at FC 0 and stops growing with
  the non-linearity amount above 1 (bit for bit M1b's at the default).
- **Output thump and M1b's volume-register DC step:** a DC the volume scales, put into the
  DC blocker's state at the `$18` write (no per-sample cost), up to 1.0 voice-DAC unit;
  reSIDfp's extra step (0.33 to 0.47) is thump 42 to 60. The output gain falls by the
  thump's share, so the step never takes a full-scale signal past full scale (the clipping
  that kept the step out of M1b). 0 is M1b's behaviour.
  Thump only approximates that step (the same for any voice count, and its headroom puts
  the 6581 0.41 to 0.59 dB under reSIDfp's level there), so at the default the gap stays
  open; with thump on, the MASTER pot clicks at each of its volume steps.
- **Smoothed, without stalling:** a control's jump ramps, a tick at a time (age, thump and
  bleed 0 to 127 in 0.32 s, the DACs in four tables over 0.16 s), through two alternating
  stages the drain commits at a block boundary; age is a blend of two filter rows at each
  filter write, no table, and a DAC table builds over four ticks. A tick's character work
  is at most 28k instructions (from 0.20 M plus a wait for the ring to empty); a note-on
  while CC 86 and 87 sweep stays within ADR 0005's bounds, every main-loop pass within the
  poll allowance. A jump of any control on a triangle steps it at most 1.25 times its own
  slope (the test the plan asks for: the earlier sweep's saw chord hid any step).
- **Monotonic:** each control's measure at every one of its 128 values (brightness at
  five ages).
- **Cost:** nothing per sample but the bleed, a multiply-add in a copy of each render used
  only while it is on. Worst blocks 710.48 and 1,425.66 (one and two chips, the default
  chip's again since `sid_nl_tick`'s clamps became the FPU's fmax and fmin;
  `SID_WORST_BLOCK_*` 711 and 1,426); 2,269.47 of 2,300 (30.53 to spare) and 205 left for
  the 6502. Flash 352,044 bytes on main (241,512 without this task; the 6581's combined-
  waveform tables and the filter rows are most of it; the 8580's WEAK and STRONG left out),
  RAM +51,488 bytes of POOL (two DAC tables, a second filter row per chip).
- **Live:** `LIVE_PARAM_PROFILE`, `_AGE`, `_DAC`, `_THUMP`, `_BLEED` on the reserved rows,
  recorded and sent at the tick; the sink keeps the settings, gives them to the spare
  before every swap and ramps the playing chip towards them. MIDI only; panel access with
  Task 4.

### Task 4: CHIP view

- The design's CHIP view on the LCD: a profile picker, the character
  controls and the effects, set separately for chip A and chip B (chip B
  arrives in M2; show A only until then).
- The knob and button mapping is documented in `docs/reference/controls.md`.
- **Tests:** e2e LCD snapshots and control round trips.

**Results after Task 4** (2026-10-15). `app/chip_view.h`, the menu's first item (it replaces
the menu's 6581/8580 item: the profile picker sets the model, and FX still toggles it), behind
THE CHIP's title card:
- **The page:** chip A's profile and model, the four character controls as bars, and the three
  slots with their algorithm and Amount, A, B and Mix, each control named for what it does in
  its algorithm; the A tab, and B greyed as "B M2".
- **Controls:** SELECT moves the focus (the chip, DIST, MOD, FX); ALGORITHM steps the profile
  or the slot's algorithm; K1..K4 turn the focused section's four values. The slot in focus
  is live's selected slot. `panel_knob_param` reads the panel's page, which the views set for
  the view that shows, so the panel, MIDI learn (AGE for K1 on the chip) and MIDI out (CC 86)
  agree. A change from MIDI shows at the next frame.
- **The EDIT layer** stays as a shortcut in every view (controls.md, "CHIP", says why).
- **Budget:** built in a canvas and copied within 200 cells a frame, as SCENE; idle 6.3k
  instructions a frame and nothing sent; the busiest frames 17.6 ms of SPI, the busiest
  `ui_poll` 34.2k; an input at most 198 instructions, the busiest pass after one 44.5k
  (docs/art/README.md, "CHIP"). A profile step's pass is the chip reset, 362.6k, as the FX
  button's: bounded on its own, outside the 120k poll allowance.
- **Tests:** host state-machine and round-trip tests (`tests/app/chip_view.rs`), fm1-emu
  goldens of the four pages and THE CHIP, a CC and K1 there and back, the costs, and K1 on the
  device turning the chip's age, the audio bit-exact with the host (`character.rs`).
- **Not done:** the expert layer (every analogue parameter as its own panel entry and NRPN,
  decision record 0006) and chip B's half of the page go to M2.

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

- **Step 0: research (before any code).** Produce
  `docs/decisions/0004-effect-algorithms.md`, covering:
  - **Sources:** Dattorro "Effect Design" 1–3 (JAES 1997); J. O. Smith,
    *Physical Audio Signal Processing* (CCRMA); Zölzer, *DAFX*; Pirkle,
    *Designing Audio Effect Plugins in C++*; Zavalishin, *The Art of VA
    Filter Design*; Valhalla DSP blog; Signalsmith "Let's Write a Reverb";
    ADAA papers (Parker/Esqueda/Bilbao/Välimäki, DAFx 2016 on);
    Chowdhury's tape (DAFx 2019) and `chowdsp` wave-digital pedal work.
  - **Open code, for reference or reuse:** Mutable Instruments
    (Clouds/Rings/Elements/Plaits fx), DaisySP, Airwindows, Freeverb,
    CloudSeed. Verify each licence and whether it is compatible with
    GPL-3.0 before taking anything.
  - **One choice per slot entry:** the algorithm, its source, whether we
    port code or implement from the description, the expected cost on
    pi32v2 (instructions per sample) and its memory, and why it suits a
    SID.
  - **Distortion design:** ADAA waveshapers; a tape model in the Chowdhury
    style, or a cheaper approximation if the budget demands it.
  - **FX design:** the reverb is based on an embedded-grade design
    (Mutable Instruments or a Dattorro plate).
  - **Attribution:** what goes in `CREDITS.md` and `LICENSING.md`.
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
