<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Design notes

Why SIDOU's code is the way it is, by area. Code carries no explanatory comments; the decisions
behind it live here. Cross-cutting decisions also have records in [decisions/](decisions/).
The cleanup before 1.5's release moves the decisions now written as code comments here.

## Hardware

- The FM-1's CPU faults on any misaligned 16- or 32-bit RAM access; fm1-emu does not model it.
  Structures copied word by word, especially in the audio interrupt, are aligned to 4 bytes with
  a static assertion (1.1's ARP helper crash). Genuinely packed data is read and written with
  byte copies (`memcpy`), never through a wider pointer.
- Every emu-gate test enforces the rule: `common::step`, which `Machine::run_cycles` and the
  bench runners use, steps through a bus that panics on a misaligned 16- or 32-bit RAM access,
  naming the PC, the address and the width. The check is one mask and branch per access; the
  `lab` binary ran as fast with it as without (under load, within noise). The JIT runs
  (`run_ms_jit`: the settled first boot) and `decode_gate`'s scratch steps are not checked; the
  first boot is also run stepped in `boot.rs`. The first sweep with the check on for every test
  (1.5, 2026-10-31) found no misaligned access in SIDOU's code, Felucca's `usb.c` or
  `third_party/`; with 1.1's fix reverted it fails at once on the draft copy's word store
  (`song_draft`).

## Flash and build flags

- The app slot is 581,564 bytes; each release plan sets a ceiling, `IMAGE_CEILING` in emu-gate's
  `tests/flash_plan`, which `flash_budget.rs` and `psid_budget.rs` both check (`psid_budget.rs`
  kept a copy once, stale at 552,000 through 1.1). A compile-time assertion keeps 13,564 bytes
  of the slot free above the ceiling for felucca-bsp and later.
- The ceilings by plan: M2a 372,000, M2b 384,000, M2c 414,000, M2d 440,000, M2e 452,000, M2f
  512,000; M3 534,000 (its 506,000 start plus 28,000); 1.0 552,000 (LAB's estimate and 1.0's
  small features, the user's raise on 2026-10-28); 1.1 556,000 (after a 3-5 KB win-back, the
  user's decision on 2026-10-29); 1.5 568,000 (after a 2-4 KB win-back, 2026-10-30). A raise is
  the owner's decision, recorded in the release's plan.
- Cold code builds at `-Os`/`-Oz`; files on instruction-budget paths keep `-O2`. `gt2_song.c`
  stays at `-Os`: at `-Oz` it moved four LCD goldens, cause not yet found.
- The voice shadow (`core/sid/sid_shadow.c`, the .sid player's main-loop copy of the voices)
  runs from flash but calls the chip's own out-of-line voice helpers (`sid_osc_run_combined`,
  `sid_osc_advance_slow`, `sid_noise_clock`, `sid_noise_settle_deferred`, `sid_env_run`,
  `sid_env_advance_slow`) in SRAM through the `*_far` pointers `sid.c` exports
  (`SID_VOICE_FAR`): a direct call cannot reach SRAM from flash (docs/hardware.md, "Call range").
  It saved 7.3 KB of flash copies and leaves `sid.c`'s SRAM code byte for byte as it was. The
  shadow still reads its envelope tables from flash (`SID_ENV_TABLES_IN_FLASH`), and builds
  with `-fno-function-sections` so that none of the helpers it still instantiates is taken
  into SRAM by app.ld's name patterns.

## Controls

- EDIT + SCL, EDIT + ARP and EDIT + LFO are chords whichever finger lands first: a hand's two
  fingers rarely land together. One mechanism (`screen_lone` in `app/screen.h`) serves LAB, STEP
  and TRACKER: a button pressed without EDIT is remembered while it stays down. EDIT pressed
  then makes it the chord. Its lone action is either done at its press (`SCREEN_LONE_DONE`:
  LAB's ARP and LFO, STEP's LFO) or held back (`SCREEN_LONE_HELD_BACK`: SCL's next effect slot,
  STEP's ARP gate, TRACKER's key off and rest with REC on) until it is let go or another control
  is used, which does the held-back action first.

## Ergonomics

- The popup rule: an encoder that changes a parameter shown in small print opens the value
  popup, its name and value large. A view that already shows the turned value large (3x, or 2x
  where the text is short) lights it in place instead: the popup would cover the very value and
  meters being tuned. The views that do: LIVE (the knob row's values at 3x, the profile word,
  the slot band at 2x, the arpeggio's rate, order and span on the knob row while ARP is held),
  CHIP (its four controls, the profile or the slot's algorithm, EXPERT 1/4, all at 3x), LAB
  (the cards at 3x, the choice at 2x). EDIT + K1-K4 over LIVE turns the selected slot, whose
  controls are small on the knob row: that one opens the popup.
- The other turns that open none: TRACKER's ALGORITHM (the voice at 2x in the header), SCENE's
  ALGORITHM (the piece's name in the large toast), SAVE AS's ALGORITHM (the name at 2x), the
  list actions (SONG, SIDS) and the turns that change nothing (STATUS, ABOUT, INSTR's
  ALGORITHM, K1 and K2 on ORDER's subtune selector, K2 on its restart marker). SELECT never
  opens it: it moves a cursor or a focus, which the screen shows.
- One table holds every exception, `NO_POPUP` in sidou-tests' `app/popup.rs`; the sweep beside
  it turns every encoder, with EDIT and without, in every view and state it lists, and fails on
  a turn that opens no popup and is not in the table, and on one that opens a popup the table
  says it does not. It found two popups that named the wrong control: EDIT + K1-K4 with ARP
  held on LIVE (the arpeggio turns, the popup said the slot's control) and K1 or K2 where
  ORDER, INSTR or TABLE take the knob for nothing (the popup said LIVE's cutoff or resonance).

## Host test harness

### reSIDfp as the reference

- reSIDfp (`third_party/libresidfp`, GPL-2.0-or-later) is a test-only reference, reached through
  `residfp-sys`; it is never linked into firmware.
- `residfp-sys/build.rs` does not take upstream's `-ffast-math -fno-unsafe-math-optimizations`:
  the reference follows strict IEEE semantics, like SIDOU's own core. It builds with
  `-ffp-contract=off` so that the tables sidou-tablegen takes from reSIDfp come out the same on
  every host (arm64 compilers contract to fused multiply-adds by default, x86-64 ones do not).
- `shim.cpp` catches every exception in each entry point that calls into reSIDfp, so none
  crosses into Rust: `residfp_new` returns a null pointer and the others -1; `residfp_free` only
  runs a destructor, which cannot throw.
- reSIDfp writes a clock's samples without a bound: `Sid::clock` asserts the buffer holds
  `Sid::max_samples(cycles)` (the cycles at the sampling ratio, plus 64).
- reSIDfp keeps the 6581 filter's range (`setFilter6581Range`, the VCR's uCox) once per process:
  setting it changes every 6581 chip, existing ones included. Its default is no call at all
  (uCox 20e-6, about 0.487 on that scale); the filter curve is kept per chip and model. So
  profiles.rs is its own test binary, and every reSIDfp filter measurement there holds
  `RESIDFP_FILTER_RANGE_IS_PER_PROCESS` and sets the range first.
- `setCombinedWaveforms` rebuilds the pulldown tables of the chip's current model, so it is set
  after the model.
- `clockSilent` (`Sid::clock_silent`) clocks the three waveform generators and voice 3's envelope
  only: all that OSC3 ($1B) and ENV3 ($1C) read. Voices 1 and 2's envelopes stand still.
- `ResidfpEngine::new(.., decimate)`: reSIDfp's DECIMATE mode (its ZeroOrderResampler, linear
  interpolation) is the choice wherever timing or pitch is measured. Its rate is off only by
  integer rounding: the resampler steps by `int(clock / fs * 1024)` in 1/1024 cycles
  (`residfp_decimate_rate`; PAL at 44,100 Hz is about 44,100.80 Hz, 0.0018 % fast). RESAMPLE
  (two-pass sinc) sounds better but runs about 0.02 % fast (44,110 samples per 44,100 expected).
- reSIDfp's `reset()` leaves each envelope counter at its power-on 0xAA and each accumulator at
  0x555555, as on the chip; `sid_init` powers SIDOU up in the same state, so both engines start a
  script of writes alike, with no warm-up.
- `render_sidou` / `render_residfp` apply writes at PAL frame (50 Hz) boundaries and return float
  samples in [-1, 1]; `PAL_CYCLES_PER_FRAME` is 985,248 / 50 rounded, 19,705.
- `register_probe::probe` reads OSC3 and ENV3 from both engines at the same chip cycles without
  rendering audio (SIDOU through `sid_clock`, the same voice clocking as `sid_render`; reSIDfp
  through `clockSilent`): neither a PAL frame nor 100 cycles is a whole number of samples at any
  supported rate, so an audio render cannot place a read on an exact cycle. A write at cycle `c`
  happens after `c` cycles have run and before a read at that cycle; `Mismatch::index` k was read
  at cycle `(k + 1) * interval`.

### The C core through FFI (sidou-sys)

- `sidou-sys/build.rs` compiles the C core, the bench workload and the portable app logic
  (`app/`) with `cc` and generates their FFI with bindgen. `wrap_static_fns` makes header-only
  `static inline` functions callable: bindgen writes an extern wrapper for each into a C file
  compiled with the rest. The headers' private `static const` tables (`HEADER_PRIVATE_TABLES`)
  are blocklisted: they have no symbol to link to.
- The host build uses `-ffp-contract=off`, as the firmware does: float results must be identical
  on host and pi32v2 so their outputs can be compared.
- The host build defines `SIDOU_ALL_COMBINED_PROFILES` and `SIDOU_ALL_COMBINED_PROFILES_8580`,
  as the firmware's does (platform/fm1/CMakeLists.txt): the host tests check every
  combined-waveform profile of both models. A profile not compiled in plays AVERAGE.
- Host-only defines: `SIDOU_FONT_CHECK` aborts on a glyph code above `FONT_LAST`, which the font
  does not hold in flash (that is how the tests prove nothing draws one); `SIDOU_MIDI_MAP_DOC`
  builds in the CC/NRPN map's text for the generated reference; `SIDOU_REG_RING_RACE_HOOK` is the
  register ring's test hook (the tests play the audio interrupt inside the producer's choice of a
  free DAC table). The device leaves them out.
- A second bindgen pass for `--target=i686-unknown-none` gives the structs' field offsets and
  sizes as the pi32v2 lays them out (ILP32: 4-byte pointers and ints, no 64-bit fields in them),
  from bindgen's layout assertions (`sidou_sys::ilp32`). The emulator tests read the firmware's
  chips, MIDI state, the LIVE view's tap, the title card and the song transport from its RAM with
  them; the host's own layout differs wherever a struct holds a pointer.
- The 6502 is built twice more for the tests only, with every operand access through the hooks
  (`CPU6502_HOOK_ALL`) so that Tom Harte's tests check each access's cycle stamp:
  `cpu6502_run_hook_all` as the device builds it (`CPU6502_SMALL`, the I/O accesses out of line)
  and `cpu6502_run_hook_inline` with them inline.
- The chip is built once more for the mute's reference (`SID_TEST_ZERO_VOICE`), each of its
  external symbols renamed with a `_zero_voice` suffix (`ZERO_VOICE_SYMBOLS`): the voices
  `sid_test_set_zero_voices` names (bit v) output 0, on the calling thread. It shares the other
  files' DACs and filter curves.
- The C workload keeps global state and is neither reentrant nor thread-safe, while the tests run
  in parallel: every caller goes through `Workload::lock`, and `Workload` is not `Sync` (its
  `PhantomData<Cell<()>>`) so a `&Workload` cannot reach a second thread.

### Helpers and signal metrics

- `assert_float_eq` is GoogleTest's EXPECT_FLOAT_EQ: within 4 ULPs (`MAX_ULPS`), comparing the
  floats' biased representations; NaN fails. `assert_near!` is EXPECT_NEAR: |a - b| <= tolerance,
  NaN fails.
- `xorshift_noise` is deterministic full-range test noise (xorshift32), independent of the C
  code. `tone_amp` is the amplitude in LSB of the component at a frequency, by projection onto sin
  and cos; `thd` is harmonics 2 to 10 against the fundamental, as an amplitude ratio.
- `estimate_f0` takes the fundamental as the peak of a Hann-windowed DFT magnitude (a quarter-bin
  coarse scan, then a golden-section search around the best bin): unlike autocorrelation it is
  not biased by how sharp (aliased or band-limited) the waveform is.
- `rms_envelope_db` is the mean-removed RMS per window, not normalised; for a periodic tone the
  window must be a whole number of periods, or the RMS ripples with each window's start phase.
- `max_envelope_diff_db` / `rms_envelope_diff_db` compare only windows where the reference is
  above `floor_db` and return NaN if any compared window is NaN, so a comparison against it fails;
  the RMS one is 0 when no window is compared. `normalized_to_max` takes the first of equal maxima
  and never lets a NaN replace it (as std::max_element). The metrics panic on degenerate input.
- `settle_filter_coefficients` gives the filter's coefficients as the next render takes them: a
  write to $15-$17 only marks the filter, and a render recomputes it before its first sample. It
  renders no sample, so the voices and the output stage are as they were.
- `FIRMWARE_BLOCK` (128 samples) is the block the firmware renders the colour chain and the effect
  slots in; `Fx::run` and `Slots::run` use it.
- `rough_pitch_hz` counts rising zero crossings per second: a rough pitch for the real-chip tests.

### Benchmark and golden workload

- `sidou-bench` runs the shared workload for {1, 2} chips x {6581, 8580}. Its throughput counts
  samples times chips, so criterion reports the rate per chip; `workload_begin` (`sid_init`, which
  fills the cutoff table) is setup and is not timed.
- The golden renders are FNV-1a 64 hashes in tests/golden/hashes.txt; `SIDOU_GOLDEN_UPDATE=1`
  rewrites the file.
- `workload_never_clips_and_has_no_dc_offset`: the 6581's volume write at frame 0 is a real DC
  step, and the 16 Hz blocker needs about 50 ms to absorb it, so the mean is taken after
  `DC_BLOCKER_SETTLE_SAMPLES` (2,205). `MAX_DC_OF_RMS` (0.07) is relative to the signal, so the
  output level does not move the bound; the 6581's DC is about 5.6 % of its RMS, from the voice DC
  that follows the envelopes.

## SID core

### Tolerances against reSIDfp

- Tolerances are tightened over time and never loosened to make a failure pass without the reason
  written down; a ceiling set from a measurement is the measured value rounded up to 0.01 dB (the
  measurement is deterministic; the margin covers last-digit libm differences in reSIDfp between
  platforms).
- `MAX_PITCH_CENTS` (0.1): SIDOU was 0.0014 cents from theory and from reSIDfp at M0, reSIDfp
  under 0.0001 cents from theory; since both engines power up alike (M1b), SIDOU is 0.0002 cents
  from both.
- `MAX_ENVELOPE_DB_8580` (0.2 dB) is coupled to `ENVELOPE_FLOOR_DB`: the 8580's worst window sat at
  the floor, where the release-slope difference grows largest, so a new floor means re-checking
  it. Measured: 0.44 at M0, 0.39 once both engines powered up alike, 0.40 with ENV3 bit-exact,
  0.030 with reSIDfp's DACs (M1b). `MAX_ENVELOPE_DB_6581` (0.5 dB): 2.04 at M0 without the 6581's
  kinked envelope DAC, 0.31 with reSIDfp's DACs and the envelope-following voice DC.
- `ENVELOPE_FLOOR_DB` (-20 dB): the 8580's release fell about 5.5 % steeper than reSIDfp's (55.9
  against 53.0 dB/s) with both envelope counters releasing alike, so the difference was in the
  output path: the envelope DAC's leakage, which keeps an off bit conducting (0.35 % of its weight
  on the 8580). With it, 53.2 against 52.8 dB/s and 0.14 dB apart at -30 dB. Below that both
  engines near the leakage floor and drift apart (1.5 dB at -40 dB), so the floor stays at -20 dB.
- `MAX_LEVEL_DB` (0.2 dB): one unfiltered voice's RMS against reSIDfp's (saw, triangle and 50 %
  pulse at A4, 0x1d45, and A2, 0x08d6) is within 0.12 dB on both models once the output is scaled
  to reSIDfp's level. The margin covers waveforms and pitches: reSIDfp's output stage rolls off the
  top harmonics a little (0.25 dB less for a bright wave at 0x4000) and its 6581 mixer compresses
  louder mixes (three voices: 0.5-2 dB), so only one voice is compared.
- `SAW_A4_HZ` is A4 from the register value 0x1d45, not 440 Hz; `ENVELOPE_WINDOW` (401 samples)
  is four of its periods (400.9 samples at 44.1 kHz), so a window's RMS does not ripple with the
  phase it starts at.
- `SKIP_WINDOWS` (1): the first 9.1 ms is the start-up transient (in window 0 the 6581s are
  0.29 dB apart, the 8580s 0.25 dB). `SUSTAIN_FIRST`/`SUSTAIN_LAST` (17/43) are the sustain
  plateau, 154.6 to 391.0 ms; the gate goes off at 400 ms. `PAST_THE_ATTACK_AND_DC_SETTLING`
  (samples 17,640..52,920) starts at 0.4 s, past the attack and the DC blockers' settling.

### Register reads against reSIDfp (register_reads.rs)

- ENV3 ($1C) and OSC3 ($1B) must read back reSIDfp's values at the same chip cycles on both
  models. Each case runs a script of writes on both engines and reads once a PAL frame, or every
  100 (ENV3) or 7 (OSC3) cycles where the timing inside a frame matters; a read at the start of
  frame N comes after frame N's writes. Cases that stay off by decision are `#[ignore]`d with the
  reason, so `cargo test -- --ignored` shows where they stand.
- `RESID_RATE_PERIODS` only size the scripts long enough for the slowest phase.
  `RATE_PERIODS_FROM_FF_DOWN_TO_0` is 756: the exponential divider takes 162 x 1 + 39 x 2 + 28 x 4
  + 12 x 8 + 8 x 16 + 6 x 30 rate periods from 0xff to 0.
- The ENV3 scripts write only the gate bit to voice 3's control register: ENV3 does not depend on
  the waveform. Frame 0 of every script is the power-on release from 0xaa to 0.
- OSC3 within a frame is read every 7 cycles: 7 is prime to every frequency's step pattern, so
  reads land on every phase of the one- and two-cycle OSC3 delays.
- The noise scripts run voice 3 at `NOISE_SCRIPT_FREQ` (0x7531), which clocks the LFSR every 35 or
  36 cycles: accumulator bit 19 rises at cycles 6, 41, 76, ... from power-on, and 18 cycles after
  a TEST release.
- OSC3 with no waveform selected: SIDOU reads 0; reSIDfp holds the last waveform output on its
  floating DAC input and fades it bit by bit only after 54,000 (6581) or 800,000 (8580) cycles.
  Programs rarely read OSC3 with the waveform off, so SIDOU keeps 0
  (`osc3_with_no_waveform_reads_0_where_residfp_holds_the_last_value` pins both).
- OSC3 after hard sync stays off by one or two reads a sync: `read_osc3` rebuilds the previous
  cycle from the current state, which the restarted accumulator has lost. On the 6581 reSIDfp
  still reads the last value before the sync for one cycle where SIDOU already reads 0; on the
  8580 two reads differ, as its triangle/saw reach OSC3 a cycle late. Read-only and inaudible;
  pinned by the ignored `hard_sync_while_deferring_reads_osc3_exactly`.
- OSC3 after a pulse width write stays off too: on the chip the write lands on OSC3 a cycle late,
  while `read_osc3` recomputes the comparison from the current width and ignores `pulse_held`.
  Read-only and inaudible; pinned by the ignored `pulse_width_write_reads_osc3_exactly`.

### Filter against reSIDfp (filter.rs, profiles.rs, sidou-compare)

- The filter is compared with reSIDfp's at the small-signal level the tables were fitted at
  (sidou-tablegen fit-filter), where SIDOU's linear filter should match: the transfer functions
  (noise through the filter against the same noise unfiltered, measured alike on both engines,
  LP/BP/HP around the cutoff) and each mode's passband gain; and the sweeps as RMS envelopes, at
  the small-signal level and, recorded only, at full scale. `below_and_not_nan` makes a NaN
  measurement fail.
- `TOP_HZ` (7 kHz): transfer functions are compared from half to twice reSIDfp's cutoff and below
  7 kHz. Higher up SIDOU's trapezoidal filter, exact at its cutoff, departs from the analogue
  response it is fitted to (frequency warping: 1.6 dB at 8 kHz for a 3.5 kHz low-pass). Each
  transfer function is relative to its own engine's unfiltered output, so the levels are matched.
  `TransferErrors` covers FC values whose reSIDfp cutoff is up to about 3.5 kHz, at resonance 0, 8
  and 15.
- `filter_limits` holds M1b's tolerances; profiles.rs holds each chip profile to them where it
  meets them. `MAX_TRANSFER_DB_*` (largest error around the cutoff, LP, BP, HP), measured: 8580
  0.97, 0.49, 0.22 (the low-pass's at 7 kHz, warping); 6581 3.31, 3.12, 2.88, the largest at FC 64
  and resonance 15, in the threshold region (FC up to about 600) where reSIDfp's 6581 damping
  drifts with FC and one value per resonance cannot follow it. Elsewhere about 2 dB: reSIDfp's
  6581 filter is no state-variable filter, and away from the cutoff its band-pass and high-pass
  level off.
- `PASSBAND_GAIN_DB_*` (SIDOU minus reSIDfp, LP from f0/8 to f0/3, BP within 1.2x of f0, HP from
  3 f0 to 9 f0 where 3 f0 is below `TOP_HZ`), measured: 8580 -0.11 to 0.00, -0.17 to -0.05, -0.05
  to 0.03; 6581, with reSIDfp's mix of the filter's outputs (each mode's gain, the integrators'
  leakage), -1.74 to 1.78, -2.01 to 2.06, -0.40 to 0.52 (6.5 dB louder before that mix); what is
  left follows the threshold region's damping drift.
- `filter_sweeps::sweep`: one voice of noise (frequency 0x4000) at resonance 8, the cutoff's high
  byte rising by 4 every frame for `FRAMES` (64 frames, 1.28 s), compared as RMS envelopes in
  `WINDOW` (882 samples, 20 ms; noise has no period to align to), each normalised to its loudest
  compared window. The first three windows are not compared: window 0 holds the click of the
  0 -> 15 volume step and is the loudest of all, so normalising to the loudest compared window
  keeps the baseline on the filtered noise.
- `SMALL` (sustain 4) is the small-signal level the tables were fitted at (envelope 0x44, about a
  quarter of full scale); `FULL` (15) is full scale, where the 6581's filter is level-dependent.
  The reference tests assert the small-signal sweeps; sidou-compare writes both as WAV files.
- `SWEEP_SMALL_*`, largest window LP/BP/HP: 8580 1.62, 2.02, 1.70; 6581 4.84, 2.46, 1.01, RMS 1.07,
  0.68, 0.43. The 8580's 1.0 dB target is missed while the near-cutoff transfer functions are
  within 1 dB: the low-pass's largest window is a normalisation artefact at low FC, the
  band-pass's frequency warping near Nyquist, and the high-pass lacks reSIDfp's 15.9 kHz output
  low-pass (adding it helps the 8580's LP and HP but worsens its BP and the 6581, so it is not the
  fix).
- The 6581's sweep band-pass is furthest off at the top of the sweep, where its cutoff passes
  20 kHz: reSIDfp's band-pass levels off while SIDOU's falls away. Its low-pass's largest window is
  the fourth, where reSIDfp's output still carries the volume write's DC step through its 1.6 Hz
  high-pass (SIDOU's DC blocker is at 16 Hz).
- `SWEEP_FULL_*`, recorded only (SIDOU's filter is linear and fitted at the small-signal level):
  the 6581's met their targets (LP and BP 2.0, HP 2.1) at 1.88, 1.89, 1.49, RMS 0.52, 0.58, 0.84.
  The 6581's high-pass is level-dependent at FC >= 1,760, around SIDOU's 0.45 fs cutoff clamp.
- `LOUD_6581` (`loud_filter_sweeps_mos6581`, `loud_writes`): three voices at full sustain into the
  filter, three noise voices or a C major chord (saw, 50 % pulse, triangle), where reSIDfp's 6581
  filter is furthest from linear (its cutoff halves and its damping doubles under the voices' DC
  load). Target 3 dB; measured noise 2.44, 1.91, 2.81, chord 2.33, 4.37, 2.84. The chord's
  band-pass misses: reSIDfp's saturating op-amps keep it louder, which the bound on the load,
  `SID_NL_M_MIN`, stands in for at the headroom the 6581 must keep.

### Chip profiles against reSIDfp (profiles.rs)

- Each profile at zero character is compared with reSIDfp at its settings: the filter as
  filter.rs measures the default (reSIDfp's curve and range set to the profile's), and the
  combined waveforms, OSC3 read once a frame and every 7 cycles, equal read for read. Some
  combinations are silent on some chips (a weak pulldown), so a profile only has to be heard
  somewhere; `told_apart` proves the comparison distinguishes the strengths.
- "worn 6581" goes beyond reSIDfp (a DAC it cannot build, more non-linearity): its filter and
  combined waveforms are compared, its DACs are SIDOU's (sid_character.rs). Its filter is reSIDfp's
  curve 0.35 at range 0.4, which SIDOU's model, fitted at the default range, follows less closely
  (4.2, 4.0, 3.7 dB at non-linearity 1); its non-linearity of 1.5 departs from reSIDfp by design.
- `limits`: `filter_limits`, except where a profile away from reSIDfp's default curve misses them;
  then the measured value, only ever tightened. The 6581's output mix and non-linearity were
  fitted at the default setting, so the passband gain spreads further from it: R2 (curve 0.58)
  2.62/2.38/2.81 dB, gain -1.52..1.80, -2.24..1.18, -0.29..0.49; R3 (0.42) 3.324/3.130/2.888,
  -1.64..2.26, -2.60..2.18, -0.39..0.54 (its shape errors equal the R4AR's within 0.01 dB, from the
  threshold region the two share); worn 5.92/5.68/5.16, -1.67..2.00, -1.66..2.21, -0.42..0.10.
- `the_age_rows_against_residfp_recorded`: age/heat's two rows (the 6581's curve 0.25 at range
  0.3, the 8580's curve 0.75) are recorded, not held to a bound: they are artistic targets, and
  age 127 doubles the 6581's non-linearity besides. Decision record 0006 has the numbers.

### Cutoff table (cutoff_codes.rs)

- Every measured cutoff survives the flash encoding and the C decoder within
  `MAX_RELATIVE_ERROR` (2.5e-4).
- All 14,336 codes go through `sid_cutoff_hz` and the sequential reader (in order, then with gaps,
  repeats and steps back). The codes are the fit's (the drift test re-measures them), so this
  holds the C decoder to the table without the half-minute fit.

### Chip (sid.rs)

- Voice 3 off is compared against the same chip with voice 3 silent, not against zero: silent
  voices are not exactly 0, the DACs leak, as reSIDfp's do.
- `write_voice_3_control_unlatched` forgets the OSC3 latched before the write: the OSC3 tests poke
  the accumulators directly instead of running cycles.
- Power-on: release rate 0 from 0xaa reaches 0 well under 10,000 cycles (reSIDfp: about 7 ms).
  reSIDfp latches ENV3 at the start of each cycle; nothing is latched before the first, and the
  first power-on release step lands in the third cycle. From power-on the 8580's triangle/saw
  latch holds 0x555 (reSIDfp's initial value).
- A read of a readable register drives the bus too, as in reSIDfp.
- `three_resonant_voices_leave_the_6581_headroom_and_clip_the_8580_like_residfp`: three voices in
  phase at maximum resonance, every mode and unfiltered, swept over the cutoff, pulse and saw at
  two pitches, at reSIDfp's level (`LEVEL_6581`/`LEVEL_8580`). The volume scales the output after
  the filter, so volume 7's peak times 15/7 is volume 15's before the int16 clamp. The 6581 keeps
  0.53 dB of headroom (bound -0.2 dB); the 8580 clips by design, about 1.07 dB over full scale,
  where reSIDfp's own output saturates too (checked in the same test): bound 1.2 dB.
- The gate-reset tests use SAW LEAD's voice 3 (attack 0, decay 9, sustain 10, release 6; decay 9's
  period 977 cycles, release 6's 267). ENV3 shows a step a cycle after the counter takes it
  (`ENV3_ONE_CYCLE_LATE`), so with the gate reset ENV3 rises within 9 + 3 + 1 cycles of a gate
  (`RISE_MAX`) and falls within 267 + 3 + 1 of a note-off (`FALL_MAX`) at every phase. Without it,
  as reSIDfp has the ADSR delay, 8 of release 6's 267 phases start at once; on release the 267 that
  find the LFSR at or before release 6's position start at once and the 710 past it wait.
- `sid_render_tap` renders exactly `sid_render`'s samples on both models and runs the tap after
  every `every` samples and after the last short chunk, with the voices as those samples left them.
- `sid_set_filter_nonlinearity` scales the 6581's non-linearity: 1 is `sid_init`'s default
  (reSIDfp's), 0 a linear filter. A profile can blend it while the chip plays, so each call writes
  one float at a time; the 8580 stays linear. It is clamped to 0..2: below -0.5 the filter runs to
  inf and NaN at low cutoffs (and NaN to int16 is undefined), and from about 3 the band-pass
  saturates at FC 0.
- As in reSIDfp, the 6581's low-pass and high-pass come out inverted against the unfiltered voices
  and its band-pass does not (the summer and the integrators are inverting op-amps); a mixed mode
  such as LP+BP depends on it. The 8580 keeps all three in phase.
- The 6581's band-pass mix is over the damping (`sid_svf_tick_6581`'s bpn), so a $15/$16 write
  after new tables (`sid_set_filter_curve`) takes the new damping and the mix with it. Its lowest
  cutoff follows the cutoff table at FC 0 as a curve or a blend moves it (darker settings reach
  lower, as reSIDfp's do); an amount above 1 does not raise it, and at the default curve and
  amounts up to 1 it is the fitted floor bit for bit.

### Envelope (sid_env.rs)

- reSIDfp: "counter is not changed on reset". Reset sets the rate LFSR to reload on the next cycle,
  so the first release step comes 3 cycles in (LFSR reload, exponential pipeline, envelope
  pipeline), then one every 9 cycles. Reset enables the counter whatever its value, so a counter at
  0 steps down to 0xff and releases from there.
- From period 2 on the exponential pipeline takes two cycles instead of one, so the first step at
  period 2 comes a cycle later still.
- The ADSR delay bug: attack 15's period is 31,251; 1,000 cycles in, the period drops to 9, but the
  LFSR is already past that position and runs through all 32,767 states before it matches again.
- `rate_period` is a rate's LFSR position plus one (`RELEASE_6_PERIOD`, 267). `ATTACK_0_FROM_RELOAD`
  (9 + 3): attack 0's first step from a rate counter reloaded at the gate, one period then the LFSR
  reload and the envelope pipeline. `RELEASE_FROM_RELOAD` (3): a release's first step after one
  release period from a counter reloaded at the falling edge, the divider at period 1.
- Authentic, the default, is the ADSR delay bug as reSIDfp has it: attack 0 matches at LFSR
  position 8, which the attack rate takes two cycles after the gate, so a gate that finds the LFSR
  too far on (259 of release 6's 267 phases) waits for it to come round, about 32,767 cycles. With
  the gate reset the rate counter restarts at the gate and every phase starts at once.
- `RELEASE_FASTER_THAN_DECAY` (SAW LEAD, SYNC LEAD, PWM BASS at velocity 127; their sustain keeps
  the divider at period 1): without the gate reset the release rate matches only once the LFSR
  reaches its position, so an edge that finds it past (held at the decay rate it runs up to the
  decay's position) waits; one release period of phases starts at once.
- Below 0x5d the divider counts rate periods before a step (2, 4 or 8). The gate reset restarts
  only the LFSR; the divider's count carries on, so the first step comes at its next firing, then
  the reload and the exponential (2) and envelope (1) pipelines. A counter frozen at 0 by sustain 0
  stays frozen on release, with or without the reset.
- `RefEnv` is an independent per-cycle port of reSIDfp's EnvelopeGenerator (libresidfp at
  c1791383) with the real 15-bit rate LFSR, plus SIDOU's gate reset (`sid_set_gate_reset`: the LFSR
  back to 0x7fff on each gate edge, rising and falling), which is not reSIDfp's. `ADSRTABLE` is
  reSIDfp's rate table; `lfsr_positions` counts the shifts from 0x7fff to each LFSR value, the
  positions `sid_env` counts in.
- `assert_settled_copy_matches_reference` compares a settled copy so the envelope may stay at rest
  across steps; the counter and ENV3 must be current even at rest, because the audio path and $1C
  read them directly.
- `jump_ahead_matches_per_cycle_reference`: `sid_env_advance` jumps over quiet stretches; reSIDfp
  clocks every cycle. Random writes, some within a few cycles of each other so they land in the
  pipelines, and resets half the time right after a gate change, must leave both in the same
  state, with the gate reset too. `FallingEdges` counts what the reset's falling edges meet (a
  pipeline in flight, a divider past period 1, a counter frozen at 0); the test requires enough
  of each.
- Steady states rest across calls; waking them must land where the per-cycle clock does, at every
  divider period a held sustain can have (1, 2, 4, 8) and when frozen. Rests longer than `idle`
  can count (2^32 cycles) equal the state after T mod (rate period x divider period).

### Filter (sid_filter.rs)

- `PI` is `SIDOU_PI`, a float literal in C that bindgen exposes as f64; both round to the same f32
  (a const assertion).
- The 6581 tick without non-linearity is the linear filter, up to the rounding of its Newton step
  and of the band-pass it rebuilds.
- In the threshold region the 6581's cutoff rises with the swing and a DC load lowers it, as
  reSIDfp's; neither moves the 8580's. The cutoff follows the kinked cutoff DAC (reSIDfp's), not
  FC: ordered by the DAC's output it never falls, and where a high bit turns on it drops with the
  DAC.
- The coefficients come from one Newton step for 1 / (1 + g (g + k)), and the step's error is the
  model: under load the step underestimates a1, which raises the damping the filter runs at above
  the table's k, as reSIDfp's damping grows under load. An exact divide would keep k and change the
  sound (`the_6581_newton_step_is_the_load_damping`). Any 0 < a1 <= 1 / (1 + g (g + k)) is stable.
- The curve settings of the chip profiles (`FILTER_CURVES`, gen/filter.h): five on the 6581
  (reSIDfp's default, R2, R3, worn, age/heat's) and two on the 8580 (the default, age/heat's); a
  curve out of range is the default, row 0. The tables are rebuilt outside the audio path and apply
  at the next filter register write, as `sid_init`'s do.
- `sid_set_filter_blend`: the first setting's tables are `sid_set_filter_curve`'s g bit for bit,
  the second's their angles (whose tangents are another chip's g); at each filter write the
  cutoff's angle and the damping are (1 - t) of the first's plus t of the second's, so the blend
  is of the cutoff in Hz (within 1e-5 over every profile's curve and hot row). t is clamped, a
  curve the model lacks changes nothing, and `sid_set_filter_toward` builds the second tables
  alone.

### DACs (sid_dac.rs)

- `ZERO_6581`, `ZERO_8580` and `VOICE_DC_6581` are reSIDfp's constants. The envelope DAC is
  reSIDfp's envDAC as floats; the waveform DAC is reSIDfp's oscDAC minus its value at the zero, at
  twice its scale, with the 6581's voice DC added.
- An off bit still conducts: at envelope 0 a voice is quieter, not silent. Every chip of a model
  shares one default table. The 8580's terminated ladder at 2R/R = 2 has equal steps; the kink
  moves the 6581's midscale step.
- `the_default_tables_keep_their_pinned_crcs`: CRC-32 (zlib's) of the floats' IEEE-754 bit
  patterns, little-endian, `wave` then `env`, as the device holds `sid_dac`, so a device-side CRC
  over the struct gives the same value (docs/hardware-test.md). `sid_dac_build` runs in float on
  both, but the device's division need not round as the host's or fm1-emu's soft FPU do, so a
  change shows here first.
- `sid_dac_job` builds `sid_dac_build`'s table in `SID_DAC_BUILD_STEPS` steps for a main loop that
  must not stall (the character ramp): the same table bit for bit whatever was in it before, done
  only after the last step, and a step after the last does nothing.

### Oscillator and noise (sid_osc.rs, sid_noise.rs, sid_sync.rs, sid_osc_fuzz.rs)

- reSIDfp's triangle table is the top 12 bits folded, shifted up one, so bit 0 is always 0 (all
  4 x 4096 entries checked). Ring modulation is reSIDfp's index = acc ^ (~source & 0x800000);
  with sawtooth selected too it does not touch the index. "Accumulator is not changed on reset."
- Every combination, in every profile and on both models, goes through reSIDfp's pulldown table:
  the AND of the waveforms, then the table. A locked-up noise register relies on row[0] == 0: an
  output of 0 stays 0.
- The noise register is in reSIDfp's bit order (bit 22 is where the new bit enters);
  `residfp_shift` is one shift outside TEST: up one, new bit = bit 0 ^ bit 5. It shifts two cycles
  after bit 19 rises, whatever the step sizes.
- `sid_osc_pull_msb` relies on the 6581's sawtooth combinations having their top bit low in the
  cycle the accumulator enters its upper half (top 12 bits 0x800 to 0x80f), pulse high or low, in
  every profile.
- `assert_same_osc` applies deferred shifts (a locked-up register) before comparing; osc3 and pulse
  are kept only while running cycle by cycle (locked up, OSC3 is computed instead); `tri_saw` is
  the 8580's OSC3 latch, which the 6581 does not use.
- The boot-built noise tables (`sid_noise_tables_init`, a constructor) are the generated ones:
  `SID_NOISE_JUMP` is sidou-tablegen's 2^k-shift matrices and `SID_NOISE_WB_BYTE[h]` the writeback
  of the output's bits 4..11; every byte is an output bit's writeback, so no two agree. 2^23 - 1
  shifts bring every nonzero register back; without noise selected the shifts are only counted,
  and settling applies them exactly.
- A noise-combined sync source runs cycle by cycle; its overshoot must still count to the end of
  the step, as `sid_osc_sync` expects.
- `bulk_steps_match_strictly_cycle_by_cycle_under_adversarial_fuzz`: `sid_osc_advance`'s bulk
  steps against strictly cycle by cycle, every field after every step: long TEST holds through
  fade events, long stretches without noise then whatever the next write selects, register
  refills, every waveform, random hard syncs and ring sources that leave their linear course.
  `FUZZ_OPS` and `FUZZ_SEEDS` scale it up.
- Control writes in the fuzz go as `sid_write` does them: latch the 8580's triangle/saw, then
  write. A refill pokes the state behind the core's back, so it first applies any deferred shifts,
  then recomputes what the core derives from the register, as a write would.
- `a_pulse_width_written_above_a_still_accumulator_ends_the_bulk_window_after_one_cycle` is
  directed because the fuzz missed it: with the accumulator standing still, a width written above
  it turns the pulse off after the first cycle, so the bulk window for pulse combined with noise
  must end there (`sid_osc_bulk_limit`'s `lim = 1`). Only registers whose noise survives the
  pulldown reach the window (0xff0, 0xf80, 0xfe0, 0x3f0).

### Stamped writes and the voice mute (sid_advance.rs)

- `sid_advance` clocks the voices into the next sample so a write lands at its cycle;
  `sid_set_mute` takes a voice out of the mix and leaves everything else alone. The placement is
  checked against `sid_clock` with the write at the same cycle, and the mute against the
  `SID_TEST_ZERO_VOICE` build, whose muted voices output 0.
- `render_then_advance_to` places a write as the audio interrupt does: whole samples up to the
  write's cycle, then `sid_advance` into the next sample. Sample n ends at cycle n * step >> 16
  because `sid_init`'s cycle fraction starts at 0 (`SID_INIT_CYCLE_FRAC_Q16`).
- `observable_voices` compares what the voices show of themselves (accumulator, registers,
  waveform output, envelope, OSC3 and ENV3 as a read returns them), leaving out bookkeeping that
  depends on how the cycles were stepped (idle counts, deferred noise shifts).
- `random_write` mostly keeps TEST off so the voices play a waveform with the gate in both states,
  and sets the volume to full so every voice is heard. Voice 1 at frequency 1 with no waveform
  counts the cycles the voices have run, which checks the Q16 arithmetic across a million advances.
- An advance of 0 is not even the end of a step: a write's pulse latch and OSC3's held value last
  until the next cycle.
- Mutes act on the output only: a muted chip's voices, hard sync and ring modulation included,
  stay those of the same chip unmuted, write by write, and so do its reads; the 6581's filter load
  (`dc_in`) follows the routing register, not the mute. Bits 3-7 of the mute are ignored.

### Voice shadow (sid_shadow.rs)

- The voice shadow (core/sid/sid_shadow.h) is a copy of a chip's voices that the main loop clocks
  with a tune's 6502, for real OSC3, ENV3 and bus reads. It is checked against reSIDfp (random
  writes and reads, every model and combined-waveform profile), against a rendering chip fed the
  GoatTracker examples' write streams as the PSID environment stamps them, and at the bus's
  lifetimes, the paddles, the filter registers and wrapping cycles.
- `every_read_is_residfps_but_the_listed_gaps`: the `Gap` cases are what SIDOU's OSC3 does not
  keep: `FloatingDac` (no waveform: reSIDfp holds its floating DAC's last output, SIDOU reads 0);
  `AfterWrite` (one or two cycles after a write to voice 3's frequency, pulse width or control or
  to voice 2's frequency, where the old values drove those cycles); `OnTheBus` (the bus holding an
  OSC3 value that was not reSIDfp's). Each gap is counted and printed; any other difference fails.
- OSC3 is latched before the first write to voice 2 or 3 in a cycle, with voice 3's control as it
  was then; a read until the next cycle returns that value, so its gap is judged at the latch.
- The random streams must reach what they are for: every waveform and combination in OSC3's
  reads, and the envelope through most of its range in ENV3's (more than 200 distinct values in
  release, 80 in debug). `random_accesses` sends a gap past the bus lifetime now and then.
- With hard sync, SIDOU's OSC3 parts from reSIDfp's on more cycles than right after a restart (a
  synced voice's own noise clocks and MSB rise earlier in a step are not recomputed). The shadow
  must add nothing: its OSC3 misses are no more than the audio chip's own on the same accesses.
- The bus starts empty, as after reSIDfp's reset. A read of a write-only register halves the
  lifetime left; a read of $19-$1C refreshes it; halved to 0 it never fades, as in reSIDfp.
- `settled_voices` compares the noise register settled: how many shifts are owed depends on how
  the cycles were stepped, what they come to does not.
- Two shadows live in the main loop's overlay; as the pi32v2 lays one out (ILP32) it is 704 bytes:
  the voices 564, the noise write-back table's copy 128, the rest 12.

### Character (sid_character.rs)

- `sid_character`'s profile is 1 + the id. `chord_with_character` applies the character after
  `sid_init`, as a reset gives it, then plays `LOW_PASSED_CHORD`: saws (voice 3 a pulse) through
  the low-pass at FC 1024, resonance 8, so the filter, the DACs and the output stage are all in
  play. `brightness` (the first difference's RMS against the signal's) is a crude high-pass,
  enough to rank brightness.
- The default profiles at zero character are `sid_init` exactly, bit for bit, so the golden hashes
  and reSIDfp accuracy are untouched; so is the zero-initialised character (profile 0, the
  model's default).
- The macro at 127 applies the kink, the leakage and the bit errors at their full amounts
  (`SID_DAC_KINK_6581 + SID_CHARACTER_KINK`, `SID_DAC_LEAKAGE_6581 * SID_CHARACTER_LEAK`,
  `SID_CHARACTER_MISMATCH`).
- Thump: the output stage fits it into the room the output leaves (app/thump_out.h), so the
  chip's gain is `sid_init`'s at any thump and its render carries no thump (the steps wait in
  `thump_pending`). At 0 the 8580 barely steps at a volume write (no voice DC); the 6581's own step
  at 0 points the other way, so its size first falls. Measured as the step's signed area.
- The jump test's timing: `LIVE_TICK` is the live instrument's tick in samples; `BEFORE`/`AFTER`
  cover the longest ramp (the DACs': 64 tables, each over `SID_DAC_BUILD_STEPS` ticks) and some.
  The reference ramps the control over 0.2 s (`RAMP`, as the effect slots' click tests).
- `held_triangle_routed_for`: one triangle at 110 Hz at full sustain, with the filter as each
  control needs it to be heard. Age/heat moves the cutoff of the voice through the low-pass at
  resonance 15 at FC 1536 (lower, where the coefficients' step is small, a jump at once is 10 to 20
  LSB: no click to see); the bleed sends the unrouted voice into the high-pass at FC 384.
- `Jump`: `Ramped` goes to the host's sink, which ramps it a tick at a time
  (`sid_character_ramp_tick`); `AtOnce` applies it at once (`sid_character_change`); `Smooth` is the
  reference, the control at its value every sample, the DAC tables every 16 samples. The DACs'
  grit goes to the asked-for setting at the tick it is asked for and the chip slews it.
- The jump test meets the triangle at ten phases (its 401-sample period against a 441-sample tick
  puts the jump a tenth of a period apart each time). The measure sees a CLICK-sized step, and the
  same jump taken at once for every control but the thump, which moves a held note not at all: it
  acts at the next note's start or volume write, on the output stage.
- From every control at 127 back to 0 gives the profile's own chip again: age moves only the
  stage's `filter_t` (the tables stay), so the first row's tables are `sid_init`'s throughout.
- `character_wavs` and `dac_wavs` (ignored) are listening files, not checks: `cargo test --release
  -p sidou-tests --test unit -- --ignored character_wavs` writes build/character/, 12 dB louder
  than the chip, at 0, 25, 50, 75 and 100 % of each control's travel.
- `dac_is_heard_on_every_sound`: the v0.3.0 device session heard no DAC on PWM BASS, NOISE PERC or
  soft notes. What DAC adds beyond a change of level (`added_beyond_a_gain_db`) must grow with
  every step and reach -25 dB of the sound from a quarter of its travel and -17 dB at the top, on
  SAW LEAD, PWM BASS, a soft decaying note and NOISE PERC alike. Before the grit PWM BASS had -33
  and -27 dB, almost all a change of its harmonics' levels.
- `dac_sounds` measure DAC as live plays them on the 6581 R4AR (live's chip). The grit is
  measured from half a second into the note: the power between the note's harmonics (more than
  6 Hz from any, above 20 Hz) against the whole's over 32,768 samples (a Hann window).
- The grit is the formula's value exactly for every int16 sample and amount (it never wraps), and
  slews by `SID_GRIT_SLEW` a sample (a second from 0 to full) on the same path however the renders
  split (blocks of 128, 441 or a sample).

### Click metric

- `max_second_diff` is the click detector: a step of h shows as h, a one-sample spike of h as 2h,
  while a tone of amplitude A at f only reaches A (2 pi f / fs)^2.
- The click tests run a control that jumps at T_ON and back at T_OFF (J) against a reference that
  ramps it over `RAMP`, 0.2 s (R). Both trace the same path at different speeds, so J - R is a
  slowly changing difference of two smooth outputs; a 110 Hz input keeps its second difference to a
  few LSB while a click in either run shows at full size. Each test adds a synthetic `CLICK`
  (100 LSB) step to J and requires the metric to see it above `RESIDUAL_MAX` (30 LSB), so it
  cannot go blind. In slots.rs the step goes on J's left channel only; the residual is the worse
  of the two.
- `assert_click_free_at` takes the input level: the drives are tested below their hard-clipping
  level, where J and R are smooth; clipped near-squares at slightly different gains differ by
  whole edges, which no click metric can see past.
- `worst_second_diff_at_and_between_switches` holds each algorithm 0.5 s on a 110 Hz tone; a switch
  may not step more than `RESIDUAL_MAX` above the steady signal's own.
- Crush and the noise (Lo-fi B) are steppy or random by nature: their blocks have exact-model click
  tests (`noise_jumps_leave_no_click`, `crush_jumps_leave_no_click`). `crush_model` is
  `crush_tick`'s quantiser operation for operation in f32; `to_i16_like_sid_fx` rounds half away
  from zero and clamps, as `sid_fx_to_i16`.

### Colour chain (fx.rs)

- Noise alone on silence is only the hiss; on a signal it is signal + hiss. Rounding the sum may
  differ by one LSB from summing the rounded parts, on under 1 % of samples.
- Tape: up to SID-like levels (the chip peaks well below 0.3 full scale) loudness holds within
  1.5 dB; louder input is compressed, which is the point. It is asymmetric: even harmonics too.
- Tape compresses highs before lows at SID-like levels; far hotter high tones clip less, because
  the residual ADAA lags the linear part by half a sample (fx_tape.h): at 0.7 full scale the 8 kHz
  gain is about that of 200 Hz. The test pins that inversion too.
- Tape aliasing: an 8 kHz half-scale tone folds back every harmonic from the 3rd; the 4.1 kHz alias
  is the worst. Before ADAA (b19a9f9, full drive): 4.1 kHz -16.8, worst -10.9, total -9.7 dBc.
  ADAA acts on the clip's residual only, so a tiny tone passes at unity up to 16 kHz.
- A tape reset straight into full drive was once NaN with Newton-tracked reciprocals; the
  reciprocals stay within the square of a 1 % step (1e-4). `blocks_accept_any_amount_on_any_sample`
  draws mostly the extremes, which jump the furthest.
- Wow is about 0.5 Hz, 4 to 5 cycles in the 9.5 s window (the test accepts 3 to 8). Switched off
  or changed, the block slews the depth over `SID_FX_WOW_DEPTH_S` (at most 20 cycles over 15 cents
  switching off, dry within 0.25 s). A ramp input makes delayed - dry a constant, so the crossfade
  envelope reads off the output: a linear fade reversed mid-way bends by about 7e-3 in a sample, a
  corner-free one about 1e-4; the bound is 3e-4.
- CRT: the 150 Hz high-pass is far below the corner (under 0.05 dB of the -3 dB there); at full
  tone the two leave a band, and fc/4, where they overlap least, must pass within 1 dB. The cascade
  is measured against matched poles at the log-mapped frequencies, f_lp = LP_MIN (LP_MAX /
  LP_MIN)^(1 - t), f_hp = HP_MIN (HP_MAX / HP_MIN)^t.
- The hiss is tilted up, so its lag-1 correlation is clearly negative (-0.35 to -0.5) and every
  other lag is white (under 0.02). Crush's fast ramp (500 LSB a sample) moves more than one level
  between captures, so every capture shows.
- Out-of-range and NaN targets are clamped; everything but the additive noise and hum at full
  gives silence out of silence. The one-pole smoother is at 63.2 % after 10 ms, then lands on the
  target exactly. A block off long enough to go idle (the bypass) restarts independently of where
  the earlier blocks fell.
- Hermite delay reads are exact on a ramp and at integer delays; a 3 kHz half-scale tone at
  fractional delays measures each kind of line.

### Effect slots (slots.rs, slots_dist.rs, slots_fx.rs, slots_mod.rs)

- `Slots` boxes the slots and the pool they borrow so the pointer the C side keeps is stable;
  `Slots::new` fills the pool with garbage (`POOL_GARBAGE`, 0x5a5a): the FX slot must clear it
  before use.
- NONE at full mix is the identity up to the output's soft limiter (`sid_fx_to_i16_limited`),
  which shapes only what passes its knee. The settings that clipped before the limiter (the
  modulations with feedback or gain, every FX at full feedback) on two chips at their loudest in
  phase (29,990 out of the mix, `LOUDEST_TWO_CHIPS_IN_PHASE`) put no sample on a rail, and each
  still reaches the limiter, so the test sees it work.
- `sid_slots_dry` (the share the chip spread's side takes) is 1 with every slot off or at NONE,
  the product of 1 - Mix of those in the path once their Mix has settled, 0 with a slot fully wet,
  and moves smoothly while a slot switches. `sid_slots_process_float` is `sid_slots_process_wide`
  before its conversion: its floats through `sid_fx_to_i16_limited` are the int16 output bit for
  bit.
- Drives: THD rises until the clip saturates (a square's, about 48 %, is the ceiling); makeup keeps
  a 0.25 full-scale tone within a few dB at any drive. The fuzz's bias needs signal: silence stays
  silent at full bias and gain. `a_without_character` is A with no effect on the drive's character
  (overdrive's tone fully open, fuzz without the octave).
- Drive aliasing, an 8 kHz sine and a band-limited 5 kHz saw at half scale (folds onto a true
  harmonic skipped): overdrive -28.9 / -27.7, fuzz -28.5 / -31.0 dBc. An f64 model of the hard
  clip at these gains gives -14 to -22 dBc without ADAA and only -36 to -54 even with 2x
  oversampling on top: record 0004's -60 dB is out of reach at full drive within the budget.
- Drive jumps run below hard clipping: 0.03 full scale as the algorithms hear it (the input lifted
  by `SID_SLOT_DIST_GAIN`); switches at full level are `switching_between_drives_is_click_free`.
  The fuzz octave's rectified cusp at every zero crossing is real signal (a second difference of
  about 60 LSB at 0.03), so the octave's controls run at 0.01; the fuzz's gain spans 40 dB and its
  Amount jump is checked up to where it clips.
- FX: a 5 ms burst at 300 Hz peaks about 55 samples in; echoes follow at T, 2T, ... with nothing
  between. Full feedback on full-scale noise saturates smoothly, then dies away (0.95 a repeat).
  Switching clears the pool first: a loud delay tail, then lush (which fills the pool), then the
  delay on silence replays nothing.
- `rt60` is Schroeder integration of the (L, R) energy decay over -5 to -25 dB; `brightness` is the
  first difference's RMS over the RMS. FX jumps run the delays at 0.02 full scale (at 0.95
  feedback a 110 Hz tone builds up 20x where its echoes line up, and a saturating loop is no click
  test) and the reverbs at 0.1 (their broadband tail's second difference grows with level; a click
  does not).
- A delay time jump on a 0.5 full-scale tone glides with its speed easing in and out, so the read
  position never turns a corner. Algorithm switches at Mix 0, cut into odd block sizes at random,
  must not change the output: a fade can end off the snap grid with Mix 0. Each reverb line's last
  sample is within its ring at any sample rate.
- Modulation: three chorus taps of white noise sum incoherently (-4.8 dB) and linear interpolation
  at random fractions loses about 1.8 dB more, hence -6.5 +- 1 dB; on a tone three taps 120 degrees
  apart, each phase-modulated by beta = 2 pi f D, average to J0(beta). The chorus's right first tap
  runs 60 degrees on; the flanger's right side 90 (A = 0.5 is the geometric middle of its rate
  range); the phaser's two sides are in quadrature. Autopan is equal power: L^2 + R^2 against
  2 x^2 per 10 ms.

## 6502 core

### Tom Harte's and Dormann's tests (cpu6502.rs, harte.rs)

- With `CPU6502_HOOK_ALL` every operand access goes through the hooks, so each one's cycle stamp
  is checked against Tom Harte's per-cycle bus log, and every write is one of the test's.
- harte.rs reads SingleStepTests' `6502/v1` (MIT): one JSON file per opcode, each test an initial
  and final state and the bus cycles. The committed subset (`third_party/6502-tests/harte/`,
  VENDORED.md) is the first `DEFAULT_TESTS_PER_OPCODE` (100) tests of each file in a little-endian
  binary, the layout of `harte2bin.py`: a `u32` count, then per test the initial and final state
  (`pc: u16, s, a, x, y, p: u8, n: u16`, then n times `address: u16, value: u8`) and the cycles
  (`n: u16`, then n times `address: u16, value: u8, read: u8`). The names are not kept. The JSON
  reader handles only what the tests use: no escapes in strings, no fractions.
- `harte_full_set` (ignored) runs the full set from `SIDOU_HARTE_DIR`. 169 opcodes have a memory
  operand or a push, so 16,900 tests are stamped under HOOK_ALL. Half the tests start at
  `JUST_BEFORE_THE_WRAP` (0xffff_fffc) so the stamps and the limit wrap.
- A JAM leaves the PC on its opcode; Harte's final PC is the byte after it.
- $01 is $37 (`PORT_IO_MAPPED_IN`) unless the test sets it, so the device variant hooks the I/O
  page too; $34 (all RAM) and $31 (bit 2 clear: the character ROM) map no I/O.
- An INC of an I/O register is read on its third-last cycle, then the old value is written, then
  the new (the 6502's read-modify-write).
- Dormann's functional test succeeds at its "jmp *" at $3469 (6502_functional_test.lst); a trap is
  an instruction that leaves the PC where it is.

## Audio output

### Chip mix (chip_mix.rs)

- One chip's largest sample after the power-on transient is 20,274 (gate_every_frame on the
  6581), measured on every bench scenario on both models over the run's second half; the first
  half has the DC blocker settling, up to 23,834, still under the limiter's knee. Two chips that
  loud and in phase reach the limiter at the mix by up to 4.3 dB; under 12,288 each they never
  do. Recorded in decision record 0004; the 20,300 bound holds the figure.
- The two chips' sum is kept whole past int16: saturated into A (the sum itself whenever it fits)
  and the rest in `over`, so the slots' soft limiter (core/fx/fx_limit.h) sees the whole sum and
  never a clamp. Two chips at 20,274 in phase come out at about 29,990, limited, not clipped.
- The fades are linear over a 128-sample block: a fade-in takes (i + 1) / 128 of B at sample i,
  ending at all of it; a fade-out (127 - i) / 128, ending at none, so a DC level of B moves by at
  most 1/128 of itself a sample.

### Chip spread (chip_spread.rs)

- At unity (mid 1, side 0) the stage converts the slots' float output exactly as the slots do
  (`sid_fx_to_i16_limited`, past the knee too), so switching the stage in or out makes no step.
- SPREAD 100: chip A alone is all left at sqrt 2 (+3 dB); with chip B in use, A left and B right,
  each at sqrt 2, within the side's halving (1 LSB). The side's fades are chip_mix's, so A + B and
  A - B agree on how much of B there is.
- Separation only while both chips sound: it glides in over `CHIP_SPREAD_GLIDE` blocks (about
  0.1 s, held to 0.08-0.12 s at 44.1 kHz) and back after `CHIP_SPREAD_HOLD` blocks of quiet.
  Hysteresis: a level between `CHIP_SPREAD_OFF` and `CHIP_SPREAD_ON` does not make a quiet chip
  sound but keeps a sounding one sounding, so a phrase's gaps never move the image. Any one voice
  counts (the envelope counters are ORed).
- audio_fill glides the mid from 1 to cos phi and the side from 0 to sin phi by the glide's share,
  each block's gains ramping from the last block's: no step over a few LSB on the loudest side.
- SPREAD is CC 117 on the steps curve (0..127 over 0..100) and NRPN 0/4 in percent, clamped, saved
  with the settings; it is not a live parameter, so MIDI learn and MIDI out leave it alone.

### MASTER gain (master_gain.rs)

- MASTER is a digital gain after the chips (app/master_gain.h), not the chip's volume register:
  $D418's volume stays at 15 (`LIVE_CHIP_VOLUME`) so a song's own $D418 writes keep their meaning,
  and a song's volume fade composes multiplicatively with MASTER. The displayed volume is the
  pot's travel in percent.
- The taper's table is 0, then round(32768 * 10^(-3 (1 - i / 64))) for i = 1..64: -60 dB at 1,
  0 dB at 64; pots whose x = (p * 1024 + 511) / 1023 is 16 i sit on entry i. From -40 dB up,
  every 16 pot steps rise between 0.8 and 1.1 dB.
- At unity the stage does not run: the output is the chip's bit for bit. 0 stays 0 at every gain,
  so a silent tail stays exactly silent. A change ramps over the 128-sample block, sample i at
  from + (to - from) (i + 1) / 128, ending on `to`.

### Tail gate (tail_gate.rs)

- Once the chip's voices are all done, the tail gate fades out the leak the voice DACs put out
  through the envelope's zero, and the output becomes exactly silent with no click. A bleed keeps
  the leak (a deliberate effect of the character), and a later note is the chip's own output.
- Against the chip's own output the fade is a ramp (steps of at most 8 LSB); a hard mute would
  step by the output's value at that sample: the leak (about 100 LSB on a triangle) plus the DC
  blocker's tail. Notes closer than the hold (`TAIL_HOLD_BLOCKS`) play untouched; the gate lifts at
  gate-on.
- `RELEASE_TO_ZERO_MS` is the release from sustain 15 to 0 by release nibble; `SETTLE_SAMPLES`
  adds the hold, the one fade block and one block of slack.

### Thump on the output (thump_out.rs)

- The thump moved from the chips to the output stage because, played by the chips on top of
  full-scale chords, both chips and chip B's mix clipped at every start.
- A step on silence plays as the DC blocker's step response: the step at the chip's output gain on
  the first sample, then times dc_r each sample, rounded half away from zero, until under half an
  LSB, where the stage goes idle. A song's write after n samples of the block starts at sample n.
- On a sample at s, a thump x is untouched up to half the room s leaves below
  `THUMP_OUT_CEILING`, and soft-limited beyond it (growing with x, never reaching the ceiling). A
  sample already on a rail stays there; against the signal's sign the room is the whole way to the
  far rail.
- `chords`: a song on chip A and the keys on chip B (6581 R4AR), three pulse voices each at full
  sustain and volume 15 through an open low-pass, both starting in the same block (A's from the
  song's call at `SONG_CALL_SAMPLE`, mid-block), every `CHORD_EVERY_BLOCKS` (28), released after
  `RELEASE_AFTER_BLOCKS` (21): a fast release, so each start thumps fully. `effects` puts each
  effect at the corner of its controls that measured loudest on these chords (gains 1 to about 3).
- The chords are 88 % of full scale together at the mix, which the soft limiter takes to 27,327.
  With THUMP 100 % on both chips no stage reaches a rail with no effect or the worst clean effect,
  though the thump adds thousands of LSB at each start; with any effect the thump puts no sample on
  a rail.

## Live instrument

### Chip resets, presets and K1-K4 (live_instrument.rs)

- Live sends the character's settings (the profile, then its four controls, `LIVE_CHAR_FIELDS`)
  before every chip reset, and the envelope mode (`LIVE_REG_GATE_RESET`) right after the reset,
  before any register. The default is instant: the chip's gate reset on before any gate.
- The instant envelope against the ADSR delay, on reSIDfp with SAW LEAD on a 6581: release 6's
  period is 267 cycles (about 12 samples), so 24 phases cover two periods, and release 6 from
  sustain 10 reaches 0 in about 0.6 s, hence a second's render before the retrigger. Decay 9's
  period is 977 cycles, so 44 note-off phases cover it. Instant releases within 13 samples
  (267 + 3 cycles after a restarted rate counter); authentic waits when the note-off finds the rate
  LFSR past release 6's position, 710 of 977 phases: 32 of the 44 sampled.
- Gate off and on with no chip cycles between must still latch the attack, and the envelope state
  follows the gate two cycles later, as on the chip (reSIDfp's pipeline).
- A profile step is a chip reset, like a model change: held notes' gates are written again on the
  new chip so they attack again rather than stop. A profile sets the model, and going back to the
  6581 restores the last 6581 profile asked for.
- The character controls are recorded and the tick sends what changed, once, then
  `LIVE_REG_CHAR_TICK` every tick: the sink ramps the chip towards them. No reset and no $17 write:
  the stage's commit sets the filter's coefficients.
- PWM's sweep narrows near the pulse-width ends and never goes thinner than K3's narrowest width: a
  width of 0 is a DC level, and on PWM BASS at K3's narrow positions an LFO below 0 silenced the
  note for part of every cycle (ergonomics review E09).
- K3 on PWM BASS is the pulse width on 31 positions, 128 + 64 i (3 % to 50 %), one side of the
  square only; a width off the grid (from MIDI) is mirrored above a half and rounded to the nearest
  position before the step. On SYNC LEAD it is the ratio, -12..+12 semitones around the preset's
  2.75, a semitone a detent; a preset load puts the ratio back (no CC, not saved).
- `live_ratio_q16` is 2^((k - 12) / 12) * 65536 rounded for -12..+12 semitones; no entry sits near
  a rounding tie, so the rounding is unambiguous.
- K1's walk: from a cutoff off the table (the preset's 1200, or a MIDI value between entries) the
  first detent lands on the table in the turn's direction; past the table's ends a turn outwards
  leaves the cutoff and a turn inwards walks from there; 64 detents span the table.
- Each preset keeps an edit memory (the sync ratio included, starred in the name); a program
  change (MIDI, settings) loads the factory settings and drops the kept edits.
- The panel has no velocity: its keys play 127, which sounds as velocity off on every velocity
  target (full sustain level, pulse width and cutoff).

### Expression (expression.rs)

- Controllers (pitch bend, modulation wheel, aftertouch) are recorded when their message arrives
  and applied by live_tick; only velocity applies at the note-on. A flood of bends between two
  ticks writes each voice's frequency once.
- The bend's offset is (bend - 8192) * range in 1/8192 semitone. A 14-bit step at +-2 semitones is
  0.024 cents, finer than the register's step at every pitch (0.026 cents even at $FFFF, about
  0.39 at C4), so across all 16,384 values at C4 the frequency never falls and moves at most one
  register step (0.44 cents at the bottom, A#3). The top is one 14-bit step short of D4.
- The sync lead plays the note on voice 3 (the sync master) and the ratio above it on voice 1;
  both take the same bend and vibrato, so the ratio and the timbre hold. Voice 1 (2.75 times the
  pitch) tops out at 0xffff from note 90 at rest and from note 88 bent up 2 semitones, as
  controls.md's Presets section says.
- The vibrato is a triangle LFO starting at 0, up first, stepping at the tick rate: depth d at f Hz
  moves 4 * d * f / 100 cents a tick, 24 cents at 6 Hz and full depth (+-99 cents), 6 cents at a
  typical +-25. Depth 64 is half a semitone each way.
- With everything moving at once a tick writes three frequencies, three pulse widths and the
  cutoff: 14 register writes at most, reg_ring.c's per-tick budget.

### Modulation and links (live_instrument.rs, live_macro.rs)

- The filter LFO (FILTER's K4 in LAB) is a triangle over K1's cutoff walk at the PWM's rate:
  `RATE_A_CYCLE_IN_16_TICKS` (125) is 6.25 Hz, a whole cycle in exactly 16 ticks; `RATE_1_HZ` (20)
  checks the sweep is smooth (never more than an entry's step a tick), clamped at the walk's ends.
  It needs no curve: each depth detent is one of K1's cutoff detents, even in pitch, and its CC
  (STEPS over 33 values) reaches every one.
- RESO LINK: the resonance follows the cutoff's place on K1's walk, link x (32 - place) / 8 on top
  of the parameter, clamped; positive opens it as the cutoff closes, negative (squelch) as it
  opens. A link at 0 gives the parameter back; CC 74, the mod wheel and the filter LFO move it too.
- PWM LINK: the PWM depth follows the width's distance from 50 %, link x distance / 8 on top of
  the parameter, clamped; the LFO runs while the depth it gives is not 0. The links are NRPN-only
  parameters (MIDI learn reaches them), 0..16 for -8..+8, centred on `LIVE_LINK_MAX`; a preset
  loads them at none.
- The arpeggio rate is a parameter (LAB's MOD K4) on CC 28's map row with the same rates as the CC
  had, an NRPN and learnable.

### Effect slots through the ring (fx_live.rs)

- Slot settings travel as pseudo-registers through the device's own path: live -> reg_ring_sink
  -> ring -> drain -> pseudo handler -> slots, interleaved with SID writes in the order live sent
  them. The drain has no SID-write hook, so the test's Recorder sees SID writes as changes of the
  active chip's cutoff and resonance.
- A slot pseudo-register such as 0xA1 would alias to SID register 0x01 if a sink masked it; the
  host sink never writes a slot register to the chip.
- Every slot algorithm's defaults are audible: on a 220 Hz saw, the RMS of wet minus dry over the
  last 0.4 s is at least 0.1 of the dry RMS.
- Every distortion's Amount is heard on a SAW LEAD A4 at the level the chip gives it on the device
  (-23 dBFS at its peak, a 6581 at 44.1 kHz), not only at the algorithms' reference level (0.25
  full scale): bottom to top the wave's shape changes by at least -15 dB of itself, the note never
  vanishes, and its level moves by 6 dB at most (Amount is a drive, not a volume knob). Fed the
  chip's samples unscaled, TAPE's shape changed by -31 dB, OVERDRIVE's and FUZZ's mostly the level.

### Register ring (reg_ring.rs)

- `sid_write` masks the register to 5 bits: a reset pseudo-register (0x80) would land on 0x00,
  voice 1's low frequency, so the drain never passes a reset entry to it; unknown pseudo-registers
  (0x9f) are dropped too.
- A reset swaps to a chip `sid_init` left authentic; the entry after it sets the option on the new
  chip. Mutes are the chip's: a model change swapping chip A to the spare carries them along.
- The reference is `live_sid_sink` writing each register into the chip at once (`DirectSink`); the
  device side (`Device`) runs live through the ring into two chips, the wait callback playing the
  audio interrupt and `block` playing the pass's end (`audio_commit`) and the interrupt at a block
  boundary. `Recorder` counts the character's tick instead of logging it, so register logs show
  only real writes.
- The character's controls ramp the playing chip a tick at a time (`sid_character_ramp_tick`): 32
  ticks for age, about 20 for the thump, about 130 for the bleed and 261 for the DACs (64 tables,
  each over `SID_DAC_BUILD_STEPS` ticks). A profile is a reset; every reset keeps the controls.
  Nothing waits for the interrupt, and no table a chip plays from changes under it.
- Two ticks in one pass (the main loop catching up) queue both character stages; a third before
  the interrupt took either finds neither free and waits a tick (`character_defers`) rather than
  write a queued stage or wait for the interrupt.
- The race the hook provokes (`reg_ring_set_race_hook`): a stage naming table A is queued while the
  chip plays B, and the interrupt commits it in the middle of the next tick's `free_dac` choice.
  The choice must not be A, which the chip now plays, or the ramp would build its next table over
  it. DACs from 0 to 127 build four tables into dac[0], dac[1], dac[0], dac[1]: the third is queued
  while the chip plays the second, and the fourth is the choice.
- The worst main-loop pass (`REG_RING_WORST_PASS`) follows the device's input order: buttons, note
  keys, encoders, then each tick catch-up after a MASTER reading. It holds the model toggle, the
  slot's next algorithm, the sync lead, MIDI expression and every key pressed and let go within the
  pass. Without the model toggle the pass skips the sink's wait for an empty ring. A worst pass is
  over 200 entries, so twice the ring's size in passes fills it, and nothing is dropped.
- With a keys move in it, `REG_RING_WORST_PASS` covers a preset (every register), the FX button's
  request, every key down and up, then the move to chip B (gate-offs and every register again),
  with no wait for ring room.
- A song's tick under KEYS A has both chips following a setup (chip A the song's, chip B the
  keys'), each `chip_ctl_follow` at its worst (a CHPA with another profile and every control, on
  the tick its preparation ends), plus chip A's character tick: `2 * worst + 1` entries, within
  `REG_RING_SONG_PASS`. A keys move has its own share (`REG_RING_MOVE_PASS`). Song control puts
  nothing in the ring: PLAY, the stop's gates and the mute are the interrupt's.
- The staged spare preparation (`sid_character_prep`) must equal `sid_init` +
  `sid_character_apply` at once, byte for byte (filter tables, coefficients, non-linearity,
  stage; the DAC tables compared by content), for every profile on its own model and the other,
  in the slices `sid_character_prep_slices` says. A preparer's first 8580 preparation costs
  `SID_DAC_BUILD_STEPS` more slices for the 8580's default DACs.
- During a preparation another profile restarts it (the newest wins); the same profile with only
  the controls changed goes on (the DACs' setting only until the filter rows are done, slice 7;
  the ramp takes the rest after the swap); back to the chip's own profile drops it with no reset.
  A preparation never starts on a spare still queued to swap.
- Two chips share one ring, one spare and one build table: chip B's registers are 0x40..0x58 and
  its pseudo-registers 0xC0..0xC8; the slots' entries name neither chip. B's pseudo-registers the
  drain does not know (0xC2..0xC7) are dropped, not handed to the slots' handler. The host
  reference for chip B plays its power-on as the ring's chip B has it (`reg_ring_power_on`).
- One preparation at a time: when A and B ask at the same tick, A's runs and B's runs from the tick
  after A's reset. Two DAC ramps take turns on the one build table (172 ticks alone, about twice
  together). One build's work a pass: the pass that takes the spare starts no ramp DAC table, two
  catch-up ticks run one slice, and a build dropped unplayed does no work.
- The keys' chip (`reg_ring_set_keys`) comes first within an age bound: once the other chip's
  request has waited `REG_RING_YIELD_TICKS`, the keys' ramp yields (its table in hand finished), so
  a continuous DAC sweep on the keys' chip cannot starve the other; the test bound is the yield, a
  keys' table (4), B's own slices (11) and slack. The keys' own request preempts the other's
  preparation, and the keys' chip plays byte for byte as a lone chip would.
- `every_ring_test_passes_alone` reruns the binary once a test (`--exact`, one thread): the
  default DAC tables (sid_dac.c) are process state, and what one test leaves there must never
  change what another measures.

### Keys routing and chip setups (keys_route.rs)

- The device's instrument is live on keys_route over two chips on one register ring; `Keys` builds
  exactly that, and a main-loop pass with a tick runs live's tick, then keys_route's.
- With no song the keys play chip A; chip B is made with the keys' setup in the background
  (prepared, never reset at once) and holds it after every change.
- A song starting on chip A moves the keys to chip B with the same setup: a key held across the
  start is released on A and sounds again on B. When the song stops with a key held, the keys stay
  on B until it is released, then go back to A. While the song plays the keys' setup changes act
  on B; when it stops, A is prepared to the keys' setup first, and only then do the keys return.
- KEYS A: keys_route tells the song a voice is the keys' (`keys_route.taking`) before the first of
  that voice's registers goes to the ring, so the song can leave it alone from the block those
  registers reach. It tells nothing at the release: the main loop gives the voice back once the
  key's gate-off is in.
- KEYS A during a song: the song keeps the filter and the volume (the instrument's $15-$18 writes
  do not reach the chip) and its setup (the FX button does not reset the chip); the voices the keys
  do not hold keep the song's registers byte for byte. Under mono sync (SYNC LEAD) the keys hold
  voice 3, the sync source, with voice 1.
- Under a .sid tune KEYS A acts as AUTO: no voice can be taken from a tune. A 2SID tune silences
  the keys and makes both chips fresh with the tune's setups one at a time: two requests in one
  tick would take the shared spare chip from each other for ever.
- A song started before chip B is ready finds the keys still on chip A, which is then already the
  song's: `keys_route_ready_for_song` is 0 and the instrument's setup, filter and untaken voices
  stay off chip A until the keys move; KEYS A is ready at once.
- The want path is the one writer of `song_want`: A-tab edits and the song's CHPA go through it in
  turn, the latest wins. A MIDI mode that gives chip A to the player counts as a song playing;
  chip-level CCs on chip A's channels go to chip A's setup, not to the keys'.
- `chip_edits_under_character_locks_preserve_the_unlocked_song_base` compiles main.c's song
  callbacks with their real dependencies; only `audio_song` and `ui_toast` stand in for platform
  services. It checks setup state, not rendered audio.

### Knob walks (knob_curves.rs)

- Every walked parameter rises with every detent, each detent moves what the ear hears by a step
  within 0.5x..2x of the median step, a value off the walk moves the way the knob turned, and CC
  0..127 reaches every position, a position's CC coming back as itself.
- The ear's measure per walk (`own_walks`): PWM HZ, log Hz of the phase step (65536 a cycle, 100
  ticks a second); PWM depth, the square root of the swing (the ear decides); VIB HZ, log Hz; VIB
  DEPTH, the swing's cents logarithmically (127 = +-99 cents); VIB DELAY, log ms. A value of 0
  (off) has no logarithm and is left out. The ends: PWM HZ 0.05..12.75 Hz, VIB HZ 0.5..12.7 Hz;
  PWM HZ's 0.05 Hz units cannot tell the three slowest positions apart.
- Effect slot controls take the most even of three shapes (LIN, LOG, EQP), measured by the
  algorithm's own law (core/fx headers) or by rendering through the slots: `AddedLevelDb` (what
  the control adds over the sound at 0), `SecondEchoOverFirstDb`, `LogRt60`. A frequency or time
  the algorithm already maps logarithmically is measured by its value.
- Laws: CRUSH's bits are 6 dB of noise floor each and its hold rate's log Hz is linear in Hz;
  modulation depths are their swing logarithmically (Weber); reverb damping is the loop low-pass's
  log Hz; diffusion and bandwidth are blends.
- Two controls stay linear by exception: OVERDRIVE's drive (Plaits' law, made for a linear knob)
  and FUZZ's (1 + 99 a^2, whose clip starts abruptly at a SID voice's level). No shape of the three
  makes them even; the ear check in docs/hardware-test.md decides.
- The rendered measures warm the slots up for 70,000 samples (the FX pool's longest delay line,
  1.49 s, plus the fade-in) and fork each value from a copy of that state; 4,410 samples after a
  change let the smoothing settle. Slots are measured at Mix 127 unless Mix is the control.
- The test voice is a SID voice's level: a 441 Hz sine for the drives, a 220 Hz saw of 20 harmonics
  otherwise, 0.25 of full scale; the distortion slot lifts its input by `SID_SLOT_DIST_GAIN`, so
  its voice comes in that much lower. Echo: a 5 ms 1 kHz burst; the feedback's echo is read at 2T
  (DELAY) or 3T (PING-PONG, whose 2T echo is the other side's tap). Decay: RT60 from the Schroeder
  integral's fall from -5 to -20 dB, times 4.

### LIVE view and scope (live_view.rs, scope.rs)

- LIVE holds itself to a frame budget of cells the panel has yet to get (`LIVE_VIEW_FRAME_CELLS`):
  three voices of noise at full sustain move every scope cell their trace crosses every frame, and
  the scopes take turns. The budget test puts up a toast every 7 frames (so its first frames meet
  the scopes at every phase of their turns) and marks 60 cells stale every frame, which come before
  the scopes' share, so the budget binds.
- `Chip` applies the recorder's register file before each block, as the device's drain applies the
  ring; the tap (`scope_render`) runs only while LIVE shows, including under MIDI learn's frame
  over LIVE, and not under learn over the menu.
- The knob row's values fit their columns: K1 three characters at 2x, K2 two, K3 three in its 7
  cells, K4 the release 0..15; the slot page's controls 0..127. A bar is the knob's share of its
  range in eighths of cells.
- The scope's producer writes a point's slot, then the head; `scope_read` drops a copy the producer
  overtook. The peak take is one atomic read-and-clear, so a peak the interrupt raises during a
  take comes out of this take or the next (two plain stores lost a block between them; provoked
  with `scope_set_race_hook`). Only frames on `SCOPE_PEAK_STRIDE` are looked at for the peaks.

### LAB (lab_edit.rs, lab_song.rs, lab_sound.rs, lab_view.rs)

- `held_c4_db` measures a sound as `LAB_SILENT_MS` of a held C4 at the panel's velocity on the host
  chip: the RMS through a DC blocker (first-order high-pass, pole 0.995, about 35 Hz) in dBFS. The
  chip's DC level moves with the envelope (a thump at the gate, a ramp under a slow attack), which
  is not heard, so it is filtered out.
- The host chip's envelopes start at reSIDfp's power-on 170: `Chip::take_envelopes_to_zero` first
  plays a chord with the shortest envelope and lets it go, as a played instrument has them.
- `lab_audible_passes_no_silent_sound_and_every_built_in` runs 2,000 random sounds on each model by
  default (`LAB_SOUNDS`, `LAB_SEED` run more or others; 30,000 on each of six seeds passed when the
  predicate was made). It also asks the predicate to pass at least 12 % of the heard random
  sounds, so it is not merely strict; the share is low because random sounds are odd and the
  predicate keeps a margin there.
- The low-pass shut below the note is checked silent on the 8580 only: the 6581's filter stops at
  about 240 Hz. A pulse of no width is silent on the 8580 and leaks a little on the 6581; the
  predicate refuses it on both.
- A sent instrument's PWM is a pulse program: the centre, a climb to the top, then down and up the
  whole band. At rate 655 (about 1 Hz) that is 40 frames that run out of 48 at tempo 6, the pulse
  resting on each row's gate-off tick; played, the period is 50 frames within two.
- The vibrato is approximated: LIVE's 6.0 Hz is 8.3 frames a cycle; the nearest GT2 speed is 6
  frames that run, 7.2 on average with the tick-0 frames it rests on (6.9 Hz). The test reads the
  frequencies before the pattern's end, where the song starts over.
- The `LBnn` chunk keeps the LAB sound with a CRC of the instrument's bytes and its programs' rows:
  the sound comes back exactly while they are unedited and not once a byte, a row, the name or the
  pulse program's jump changes. At most 16 chunks are kept; a stale one makes room. The chunk sits
  in the trailer, which GoatTracker's own player skips.
- A pull of an edited instrument decodes: a GT2 instrument has no room for the effect slots (off),
  the tune or the sync (none); the player's bend range is kept.
- PWM rates step by 0.05 Hz: rate 10 is PWM BASS's 2 s cycle, a phase step of 328 a tick.
- `envelope_draws_to_scale` holds the drawn attack and decay to the datasheet's within 20 %. The
  filter's drawing has a log axis from 30 Hz to 12 kHz (column = 8 + 26 log2(Hz / 30)), the cutoff
  line within 12 pixels. MOD's vibrato window is 204 columns from x 32 for 128 ticks.

### DICE, UI sounds, USER profiles and sounds

- `DICE_HASH` was measured on the firmware before DICE's roll was split from its load
  (`live_dice_shape`, LAB's EDIT + SCL): 2,000 rolls on each model, FNV-1a over 1.0's sound bytes
  and base. DICE leaves 1.1's fields (the filter LFO, the links) off, which the test asserts.
- UI sounds play GoatTracker's sound-effect format (readme 6.3) on chip B's spare voice, a frame
  every other tick (50 Hz). A just-released voice is not free until its release has run out (SAW
  LEAD: 204 ms); the sound does not wait, it does not play. While a sound holds a voice, live
  leaves it alone; a note that finds the other two voices held takes it back at once.
- USER profile records live under key 0x101 + slot, beside the settings' 0x100; `REG_CHAR` (0x82)
  is `LIVE_REG_CHAR(0)`, the profile's register, followed by age, DAC, thump and bleed. CC 85 spans
  the choices 0..12 in steps of 128/13.
- On a store never mounted, a slot's save says at once that it was not saved, and the slot still
  applies until reboot; a program error after the store took the record is not SAVED. A short pool
  needs quiet or a force (`USER_PROFILE_FORCE_MS`).
- LAB's USER sounds are sixteen slots in one private file, rewritten whole one keep at a time, plus
  the unsaved edit, a small record that the settings' debounce writes and settings_load applies.
  The device loads the sounds before the settings, as `Unit` does. The edit is written first, so a
  cut never brings back an older sound over a later settings record.
- 1.0's sounds are 45 bytes ("at least 45, a missing tail is the defaults"): every slot reads with
  1.1's fields at their defaults, and fewer than 45 bytes is not a sound. 0.5.0 reads only the
  settings record and the USER profiles, so it boots a store this firmware wrote to the same
  instrument, and no list shows the USER sounds file.

## MIDI

### Clock follower (midi_clock.rs)

- The synthetic wires (`Port`): TRS is the sender's jitter (up to 0.5 ms), the byte's 0.32 ms on
  the wire (10 bits at 31250 baud), then the 2 kHz poll; USB is the host's scheduling (up to 1 ms),
  the 1 ms frame that carries the packet, then the 2 kHz poll. A constant latency is not phase
  error: the phase is checked against the time sent plus the wire's mean delay.
- No tempo before `MIDI_CLOCK_LOCK` clocks; after the lock within two beats, tempo within 0.1 % and
  phase within 1 ms, across the stamps' wrap too. A clock that stops is lost after
  `MIDI_CLOCK_LOST_MS` (300 ms); without a poll, the clock after the gap reports the loss itself.
- A tempo ramp is followed without overshoot: never beyond the tempos of the clocks the window
  (`MIDI_CLOCK_WINDOW`) has seen.
- Store hold (35 ms, interrupts masked): clocks arriving in it are stamped at the first poll after
  it. Told through midi_clock_hold, tempo and phase stay within bounds; untold, the late stamps
  pull the fit off (at least 4 of 16 seeded runs break a bound), which proves the test bites.
- The frame period (midi_clock_frame_q16) lands tracker rows on the clock; Funktempo uses the
  pair's average; when late, the next beat is shorter by the lateness spread over its frames, at
  most 1 %. `PAL_FRAME` is 312 lines of 63 cycles at 985,248 Hz.

### Song sync and clock out (midi_sync.rs)

- `Rig::hold` models a flash operation with interrupts masked (store_fm1.c's operate): no block
  renders meanwhile, the MIDI that comes in is stamped at its end, and the hold is reported
  (midi_sync_held).
- Three 35 ms holds while following a clock: the late clocks stay out of the fit and the fit keeps
  the clock's own time line, so the song realigns at most once a hold. The 105 ms lost is taken up
  at 1 % of a beat a beat: 21 beats, after which every row is on its 16th again.
- A continue with no song position for a song never played is a start; a request left waiting
  would hijack the next PLAY from the panel (review 4.3).
- Clock out: SPP then FB wait while a SysEx frame is part way out, since an SPP (system common)
  would end the host's frame (review 4.2). A PLAY from the top after a stop sends FA, not SPP + FB.
  Transport waits whole for USB ring room (`out_room`); clocks that do not fit go later, never
  dropped.
- While a .sid tune is loaded, start, stop and continue are the tune's (midi_sync_tune) and the
  song gets nothing; CLOCK OFF ignores them for the tune too.

### Input ring and parsers (midi_ring.rs, midi_rx.rs, midi_usb.rs)

- A ring gap records which sources lost input (review 4.1): the ring holds the sources' bits until
  the next push, which carries and clears them; an unknown source sets every bit. Only the parser
  of the source that lost input restarts, so a TRS overflow cannot cut a USB SysEx transfer; every
  note is released either way.
- A restart keeps the SysEx callbacks: the frame cut by the gap is dropped and the next one is
  passed on whole from its F0.
- midi_in_channel (a USB packet's whole channel message) must give exactly what feeding its bytes
  does: same messages, SysEx events and parser state.
- A USB-MIDI event packet is `[cable << 4 | cin, m0, m1, m2]`, read little-endian from the
  endpoint; the cable number is ignored. CIN 0xF carries any unparsed single byte, a SysEx data
  byte among them, not only real-time bytes. A host sends SysEx as CIN 4 for each full 3 bytes but
  the last, then CIN 5, 6 or 7 for the 1, 2 or 3 bytes that end it.

### Map, learn and out (midi_map.rs, midi_out.rs, learn_view.rs)

- docs/reference/midi.md is generated from midi_map.rs (`SIDOU_MIDI_DOC_UPDATE=1`).
  `GENERATED_DOC_LICENCE_HEADER` is the generated file's own licence header, between
  `REUSE-IgnoreStart`/`End` so reuse does not read it as midi_map.rs's.
- RPN 0 (pitch-bend sensitivity) is the channel's state per RP-015, so a DAW's bend range sent once
  before its program change holds; neither reset all controllers nor an input gap resets it. NRPN
  6/0 (the bend range's row) is a preset edit like every NRPN; the last of RPN 0 and NRPN 6/0 wins.
- The envelope mode is CC 26 / NRPN 1/10, next to the chip model (CC 25, NRPN 1/5): a setting of
  the chip rather than of the preset; CC 26 is in 20..31, which MIDI leaves undefined, as are the
  arpeggio's CCs. The character's rows are CC 85 (profile: the five built-ins, then USER 1..8), 86
  (age), 87 (DAC), 89 (thump), 90 (bleed) and NRPN 2/0..4.
- Every USER profile slot is filled in the round-trip tests (slot n holds built-in profile n % 5)
  so the profile parameter's values above the built-ins apply; an empty slot does nothing.
- MIDI out's rate test changes every parameter but the volume, the filter routing (at their
  maximum already) and the chip model (the worn 6581 profile set after it takes the 6581 back):
  24 NRPN and 33 CC packets at 16 a tick. An NRPN never splits across ticks.
- The mod target goes out before a pulse width that depends on it, or the receiver takes CC 1 as
  the vibrato wheel; with no room for the target's NRPN, nothing goes before it.
- A song from the SONG menu goes out as bank 1 and its program; the next panel preset selects bank
  0 again first.
- A CC neither the map nor learn takes changes no parameter, so a pass of it costs MIDI out no look
  (midi_rx_watch); once learn waits for it or is bound to it, it does.
- MIDI learn's view drains the grid between frames as the device does: LIVE's scopes, held to a
  budget, come back in the frame that opens learn, so the test drains several frames before
  checking that an idle frame sends nothing.

## Screen and views

### SPI and frame budgets (screen.rs, tests/app/main.rs)

- A frame's SPI is priced as the panel's bus runs: SPI1 at 60 MHz / (4 + 1) = 12 MHz
  (platform/fm1/lcd.c, `SPI_HZ`), 128 bytes a cell (8 x 8 RGB565 pixels) and 11 command bytes to
  open each window (lcd.h; `WINDOW_COMMAND_BYTES` in screen.rs, `WINDOW_BYTES` in main.rs).
  `SPI_BUDGET_MS` is 20 ms: at most about half of a 40 ms frame goes to SPI (docs/art/README.md).
- Every transition frame (wipes, banded cuts, the raster, a toast during a wipe) is held to the
  SPI budget and to the wipe's rows plus its edge (`assert_within_transition_budget`). A wipe frame
  sends its `SCREEN_WIPE_ROWS` new rows and its edge row at most, so the view under a wipe must hold
  still; a wipe that has not started and ended in `WIPE_FRAMES_MAX` (40) frames fails.
- The test `drain` renders every run it hands out, as platform/fm1/ui.c does, because a painter
  region (RED MOON RITE's) takes each tile's hash as it is painted; a drain that only counted runs
  would leave those hashes stale. The run buffer is `u32`-backed so it is 4-byte aligned, as
  grid_render_run takes it.
- The raster ramps the border, which carries the view's text, so its purple band is drawn light
  grey there (`ramp_under_text`).
- A view pushed with SCREEN_OVER (MIDI learn's frame) comes in at once and leaves the overlays of
  the view under it on the grid, there and back: that view goes on showing, and clearing them
  would resend every cell they covered.
- A banded cut (SCREEN_BANDS, the title cards' end) reveals the view below `SCREEN_BAND_ROWS` rows
  a frame, top first, with no edge row. A reveal (`screen_reveal`, the boot's title card changing
  its whole look) wipes the same view in anew but keeps the stack, the focus and the capture.
- The cover (`screen_cover`, what the view on top draws over the view under it) and a showing
  toast's text are left out of a view's budget (`screen_pending`); the cover's cells count with the
  layers' (`screen_layer_cells`) for the frame it was set in, and a toast of no time covers
  nothing.

### Font and grid (font.rs, grid.rs)

- Glyphs whose shape is forced at 8x8 with 2-pixel strokes (a dot, a tick, a bar, or a sans-serif
  stem whose only difference from font8x8 is its serifs: `.,:;'`_|IT`) may sit within 5 pixels of
  font8x8_basic; every other glyph must be at least 5 pixels away or it would be a copy. Each
  forced glyph is justified in docs/art/README.md ("Provenance and licence"). font8x8_basic is
  public domain and used as test data only.
- Box lines are 2 pixels thick through rows 3-4 and columns 3-4, so frames join up at the cell
  edges. Dither levels are ordered: each adds pixels to the one below it.
- The overlay pool holds `GRID_OVERLAYS` slots, between 240 and 255: LIVE's three scopes of 10 x 8
  cells take 240, and a slot index still fits the u8 each cell keeps.
- The test model decodes a grid_bitmap's runs itself rather than through app/grid.c, and works
  large text out with divisions, so the composed frame is an independent reference
  (`Screen::composed_from_scratch`).
- A region that grows or shrinks over the same bitmap cells sends only the cells it adds or leaves,
  so a picture revealed a few columns a frame (a title card's wipe) sends each cell once.

### Value popup and toasts (popup.rs)

- The popup box's rows: the name from row 1, the value from row 4, the bar on row 7; the bar is 24
  cells of 8 pixels (`BAR_CELLS`, `bar_px_of`). The tests use STATUS for knob popups because LIVE
  and CHIP light their knob label instead (`status_ui`); MASTER's popup opens over any view.
- A popup is whole after 3 rows a frame, following any raster or wipe that runs. It stays while the
  control keeps changing and closes `SCREEN_POPUP_MS` (25 frames) after the last change, again 3
  rows a frame, bottom first.
- A popup over any view sends at most 200 cells a frame (`POPUP_FRAME_CELLS`); the menu, scrolling
  and under a popup, at most 176 cells and 15.10 ms of SPI (`MENU_FRAME_CELLS`, `MENU_SPI_MS`).
- The two-chip menu item names fit 12 characters at 2x. The toast is one line of at most 13
  characters at 2 x 2 cells on rows 27 and 28 in reverse video; longer text breaks at the last
  space that fits onto rows 25 to 28; text that cannot break into two lines of 13 is ordinary text
  on one row.
- `a_turn_after_boot_opens_the_master_popup_mid_travel` plays the boot and a turn a tick at a time
  (four ticks a 40 ms frame): the pot read at power-on, resting with noise, then a slow turn whose
  landed changes come every frame. The screen opens the popup only in a frame without an ask
  (`popup_step`), and on the FM-1 it stayed shut until the turn's end; the test requires it open
  within 15 frames of the turn starting.
- The MASTER pot test models ADC noise with an LCG (`noisy_pot_reading`) and turns at a count a tick
  and at 40 a tick, so slow and fast turns ask for the popup and the gain lands within
  `PANEL_MASTER_HYSTERESIS` (exactly at the ends).

### Front panel and views (panel.rs, views.rs, widget.rs)

- The song buttons' ids (LFO 5, GLO 7, ARP 10, SEQ 11, PLAY 12, REC 13) are Felucca's and not yet
  confirmed on a SIDOU unit (docs/hardware-test.md); `the_song_buttons_have_feluccas_ids` pins them
  so a change is deliberate. `panel_quiet_keys` exists for the song views' jam: a view takes keys,
  and a note they already sound is released.
- `Ui::without_cards` turns the title cards off for the routing and transition tests, which the
  cards would sit over. `Ui::press` runs the frame's flow step after a HOME tap, because HOME acts
  on its release. `Ui::frame` runs views_idle until it reports no work, as platform/fm1/ui.c does
  (RED MOON RITE's scan).
- The menu's selection is a reverse-video line, not a blinking cursor, so a waiting menu sends
  nothing; a blinking 2 x 2 cursor would cost 2 windows a blink (emu-gate allows at most 2 in 16
  frames). A menu at scale 2 draws two lines a call within `MENU_BUDGET_CELLS` (190), the selected
  one first; what something drew over is drawn again regardless of the quota.
- The MIDI page opens within a ui_poll of 23k instructions: the blank, the title bar and the help
  first, then two lines a frame. LOAD lists the built-in SIDOU LOOP first, marked with a dot, then
  the store's files, eight a page; DELETE does not list it.
- The views set the panel's page before and after each event; panel_knob_param reads that page, so
  MIDI learn binds exactly what the panel turns. During a song CHIP's A tab edits the song's setup
  (keys_route.song_want) and the panel page is PANEL_PAGE_NONE, so learn binds nothing and says
  SONG SETUP: NO CC. What Amount, A and B do per algorithm comes from core/fx/fx_slots.h; an unused
  field keeps its generic name.
- The theme's roles pass the WCAG 2 contrast ratio: 4.5:1 (`WCAG_AA_TEXT`) for small text, 3:1
  (`WCAG_AA_LARGE_TEXT`) for 2x text and accents. An id the firmware does not know is the default
  theme, and a new instrument has it.

### Help card and first-run tour (help_view.rs)

- HOME acts on its release: a tap goes home; a hold of 600 ms opens the card of the view that
  shows, in the state it is in. SELECT turns a card of two pages; any other button or encoder
  closes it and does nothing else; the note keys keep playing the view under it. No frame of the
  card sends more than 200 cells. On a full stack the card has its own slot (screen_push_over).
- Undo keeps one step, and the song views that undo (STEP and TRACKER) say so on their cards.
- `every_control_a_view_takes_is_on_its_card` probes each state's input handler with every button
  but those the global handler takes first (HOME, SEQ, PLAY, REC; SAVE is save_key's), and every
  encoder both ways, alone and with EDIT held. STEP's lone SCL is skipped: it waits there for EDIT.
  Then it adds what acts outside the view's input: HOME, SAVE where save() has a meaning of its
  own (`saves`), the keys and SEQ in a song view, K1..K4 where chip_view_page gives the panel a
  page, REC in LAB, and ENV, LFO and FX on INSTR.
- A card line's control words map to controls in `controls_of`: E+ or EDIT+ means with EDIT held;
  another modifier (STEP+, GLO+, SAVE+, ARP+, SCL+) names a gesture no probe makes; info lines
  (SETTINGS KEPT, MACROS RESET, ...) name no control.
- The list-screen check skips UNSAVED: its question has its own answers, checked in sid_view.rs.
  SAVE AS's name is a value ALGORITHM edits, not an item.
- The first-run tour follows the full boot an install plays: TURN K1, PRESS ENV, HOLD HOME FOR
  HELP. An answered step's toast goes at once and the next comes 1.6 s later; unanswered, a step
  shows 8 s, then 1.6 s to the next. A toast raised under a help card waits until the card closes.

### Boot, About and title cards (boot.rs, about.rs, title_card.rs)

- The first boot frame draws the whole screen (`ui_init`, before the display is on); the SPI
  budget holds from the second frame.
- HOME held at power-on starts the full boot; the key scan also reports the held key's press edge
  once, and that edge is part of the hold, not a skip. Without the hold, HOME skips from the very
  first frame. The full boot also plays once after an install (no settings record, or another
  version's), and there the first HOME skips it.
- A note on and off between two frames (a fast tap, or MIDI) leaves no gate for the frame to see,
  yet still skips the boot and plays.
- The title card is Metal Chrome's scene pixel for pixel but for the version and PRESS ANY KEY rows
  (`BOOT_SCENE_*_ROW`), which are black in the scene so the text reads as on any black row.
- Song views opened over ABOUT edited ABOUT's tune and the close lost the edits (ergonomics review
  E02): while ABOUT's tune is the song, SEQ and the menu's TRACKER are refused with a toast.
- About's first heading is at 2x (E20): SIDOU and the version centred, or a version too long for
  both alone. The cracktro averages at most 120 cells a frame. The scroller's overlays sit over the
  bars (app/about.c asserts it); it is white and no bar colour is white, so it always reads.
- A wipe into About or into the title card sends only its rows, because app/screen.h sizes a wipe
  for a view that holds still under it.
- The title glow sends each level's cells in frames of at most 200 cells; the first level changes
  the whole menu (900 cells, 5 frames). The wipe adds 3 columns a frame (90 cells), the hold sends
  nothing, and the cut to the view comes in bands.
- The glow is brightest at the centre and black at the corners, and never goes past stop 2 (purple,
  with red as its dither): the firmware's colours are a stop and the next one. The font's dither
  glyphs D0..D16 are the 4 x 4 Bayer matrix the art dithers with (Dn sets a pixel where the matrix
  is below n), so the glow drawn in glyphs is the art's.

### Logos (logo.rs)

- Flash cost against 4 bits a pixel raw: Cracktro Chrome is a quarter of raw; Metal Chrome's
  dithered glow (single pixels of blue, purple and light red over black) makes short runs, a third.
- Cracktro Chrome's `CRACKTRO_BAND` (cell rows 9..17) holds its pixel rows 76..135 (row 75 is its
  black outline on black); Metal Chrome's `METAL_BAND` (cell rows 10..19, pixel rows 80..151) holds
  the chrome, the black outline and the purple shadow 3 pixels down and right (81..145). The
  letters' colours appear nowhere else in the sky; the glow dither reaches 13 pixels beyond the
  letters and the band cuts it.
- Each logo fits each consumer: the boot title card (`BOOT_LOGO_*`) and About's band
  (`ABOUT_LOGO_*`), centred, its overlays sharing the pool with About's stars (one a cell at most)
  and scroller (two a column at most); app/boot.c and app/about.c assert the slots sit between the
  ramp lines. Pulse Train's cost is measured over 200 frames: two cycles of the stripes and more
  than one of the duty sweep.

### SCENE (scene.rs)

- Rule 30 runs on a ring of 60 as Wolfram states it: new = left XOR (centre OR right). The canvas
  cell (c, r) shows the quadrants of generation rows 2r and 2r + 1, cells 2c and 2c + 1, in the
  colour of the lower row where it has a pixel.
- Langton's ants walk a 30 x 30 torus: on an empty cell turn right and paint it, on a painted one
  turn left and clear it, then step. A silent voice's ant is grey. The ant draws only the cells it
  changes; a board cleared whole (`SCENE_ANT_RESET`) catches up in 5 frames (900 cells, 200 a
  frame).
- A note-on lights its voice's stars even before its envelope rises: the tap's ENV3 is still 0 when
  the note-on comes before the audio has run a block. The light goes early only once the voice is
  let go and silent.
- FIRE's ramp has brown only in the ember bed and no orange (olive-brown on this palette); yellow
  is a thin band between light red and white. A silent fire is not dark: flames fall to
  `FIRE_IDLE_HOT` (120) and flicker a few rows high in red. The cutoff moves the top of the ramp,
  not the heat. The idle flame moves about a third of what the band moves, so FIRE's "a note moves
  it" test asks for twice the quiet motion, the other pieces three times.
- The plasma level (0..19) of a cell is a dither glyph over two neighbours of the ramp; its
  distance table is round(4 hypot(dx, dy)); its whole-turn sine is `sine_q7` byte for byte.
- A piece begun (`scene_begin`) and drawn a wipe's rows at a time (`scene_fill`) equals the piece
  started whole (`scene_start`); PLASMA, the costliest start, draws no row before it is asked. A
  piece starts in the compose of the wipe's first frame, not in the input's call.
- The budget tests use a band (an arpeggio every 4 frames, a bass every 8, a high note every 16)
  and a simple envelope per voice standing in for the tap's ENV3; `scene_on` opens the cutoff wide
  for the busiest plasma and the hottest fire. Under MIDI learn's frame the view leaves the frame's
  cells alone and counts what it sends; a showing toast's cells cost the copy nothing.
- SYNC LEAD sounds on voice 1 alone (voices 2 and 3 have no waveform: they clock the sync), so
  SCENE hears them as silent and their parts stay still.

### RED MOON RITE (scene_art.rs, scene_px.rs, scene_rmr.rs, scene_song.rs)

- The art in flash is held to a 48 KiB ceiling (`CEILING`, set by the M2f scene plan); the skies are
  stored as gradients plus overlays because that is smaller than whole images.
- A fighter's anchor is his feet: the prototype drew him from (x - 58, FEET - 112), mirrored from
  (x - 70, ...), 70 = 128 - 58. The arena is opaque under the sky's last rows (136-139) and below;
  the flames are the prototype's mask, 10 x 12 above each torch at (26, 104) and (212, 104).
- The spark's stages are the script's: 141, 109 and 77 pixels drawn, a disc of 29 among them; a
  pixel drawn twice keeps its last colour.
- The PNGs in assets/scene/ are compared decoded (size, depth, palette, transparency, pixels), not
  byte for byte: Node's zlib may compress the same image to other bytes (needs Node, skipped
  without it).
- Against the script's golden frames the device's torches and the 16-row status bar are left out
  (`left_out_of_the_script_goldens`). The spin's arc uses the device's sine (app/sine.h), so up to
  two pixels per arc mark may differ. The sky takes the cutoff's palette at the prototype's 0.36
  and 0.68 of the cutoff's range (737 and 1392 of 2047).
- The compositor's hashes hold under any interleaving the view can make (slots changed at random,
  the scan in slices, a rendered run left waiting while the scene changes on, sends delayed, the
  refresh sweep): once all is scanned and sent the panel is the screen (app/scene_px.h). The
  refresh sweep resends two tiles a frame, every tile once in 450 frames, which heals a change the
  scene missed.
- The vignette: three passes of the four bars give a disarm each; the fill is bar 4's last beat,
  which it turns into the spinning attack and the disarm. The bass split: 50 (D3) and up gives the
  right barbarian the lead (`SCENE_SONG_BASS_SPLIT`).
- SIDOU LOOP's instruments: 1 the bass (saw), 2 the lead (pulse), 3-5 kick, snare and hat (noise
  first, at absolute notes $D4, $DC and $DF). Under KEYS A the keys' voices count as muted. With no
  song the instrument plays: a noise voice's note-ons are the drums, the lowest held note the bass,
  the others the arp, and a free clock makes a row every `SCENE_SONG_FREE_FRAMES` frames.
- A BASS+DRUMS voice (DICE's time-shared kit and bass) gives its kit's kick and snare as drums and
  its other notes as the bass; a SIDOU before 1.5 reads the role as none of its own, so the voice
  plays no part. An SCNE chunk without the drum table reads as no drums.

### Live Link mirror (link.rs)

- The newest state wins: cells changed again before the link sent them go once, as they are then.
  A painter region without a peek sends its cells as cells, since the mirror cannot read them.

## Song engine

### GoatTracker match (gt2_match.rs)

- Equivalence (decision record 0007): for each frame, each call's SID writes from SIDOU's C player
  equal those of GoatTracker 2.77's packed player, packed by gt2reloc with the same options and run
  in gt2-ref's host 6502.
- `equal_but_for_unknown_freqs`: calls are tolerated only where the sole difference is the
  frequency of a channel `gt2_player::freq_unknown` marks: the 6502 read bytes past the packed
  frequency table there, which only the packed layout knows. Fixtures never do; among the examples
  only `everlasting.sng` (a drum on an absolute wavetable note $F0, note 112) and `hyperspace.sng`
  (note 85 arpeggiated an octave up, after about 9 minutes) play notes past B-7, whose frequency
  high byte comes from the song table after the frequency table.
- Full buffering outside the zero page writes the previous call's registers, so the tolerated
  channels are the previous call's (`writes_lag_a_call`), and its events lead the writes.
- The packer packs long runs of rests 64 rows at a time; with no pattern commands it leaves the
  effect code out, and the instrument vibrato then follows the current instrument, also between a
  row's fetch and its tick 0.
- Orderlists breaking readme 3.1's rules (a transpose after a repeat or last, two transposes or two
  repeats in a row, R0 before the restart position) stop GoatTracker's editor, but the packer packs
  them (dropping R0, $D0) and player.s plays the bytes, so SIDOU plays them as packed.
- With delays and commands in the wavetable the packer moves waveforms $D0-$DF to $E0-$EF, and
  player.s runs them as commands too. A command no row uses has no handler in the packed player:
  its label is the next assembled handler's, so $D5 (AD, unused) runs SR's handler, $DD the master
  volume's, $DF the tempo's.
- The device plays multispeed up to 4; the host any multispeed gt2reloc packs (8 in the full
  option matrix). A note's row event holds the note transposed by -16..14.
- `the_players_cost_on_the_host` only reports; the device's instructions a sample are gated in
  fm1-emu. The worst call on the host is mostly the host's own interruptions.
- Regressions found by generated songs: an orderlist opening with a repeat replays pattern number
  0 before reading any, the packer's first pattern in use (`repeat_first_song`); before a channel's
  first note its last note is 0, and a note-independent vibrato reads the frequencies of notes 0
  and 1, below the packed table, where the 6502 reads its own channel variables
  (`before_the_first_note_song`). A failing generated seed becomes a fixture.
- An A-4 pitch (gt2reloc's -G, SIDOU's OPTS a4_millihz) makes another frequency table, which the
  player does not compute (no libm on the device): init reports it. gt2reloc reads -F but never
  uses it, so it raises no warning.

### GoatTracker reference crate (gt2-ref)

- gt2-ref compiles GoatTracker 2.77's code with `fopen`, `fread`, `exit`, `malloc`, `free` and
  `stdout` renamed to the shim's (`gt2ref_rename.h`); GoatTracker's warnings are silenced, as they
  are not ours to fix. GoatTracker and the shim are one library because they call each other,
  which a one-pass linker resolves only within an archive (`compile_intermediates`, then
  `objects`).
- The `mos6502` crate (0.10.1) takes 8 cycles for RMW absolute,X when the index crosses a page
  (ASL, ROL, LSR, ROR, DEC, INC); the NMOS 6502 always takes 7 (the dummy read happens either way;
  Tom Harte's tests agree). The `Mos6502` wrapper takes the extra cycle back (`RMW_ABSOLUTE_X`).
- `Access` sets how cpu6502 reaches the SID: `HookAll` (every operand through hooks, any -L
  works), `Device` (only $D000-$DFFF through hooks, the `CPU6502_SMALL` build, so the SID must be
  in that page), `HookInline` (as HookAll with the I/O inline).
- The assembler's strict mode is the true 6502 (ldy abs,x is $BC, ldy abs,y an error, BVS $70, and
  BIT, BRK, CLD, CLV, RTI, SED); non-strict is GoatTracker's dialect with its quirks (ldy abs,y
  assembles as LDY abs,X, BVS as $B0).
- `song::determinechannels` is GoatTracker Stereo 2.77's `determinechannels` transcribed on its
  own, not from SIDOU's C: 6 if every trial read passes, else 3; a length of 0
  (`songbuffer[-1]`) counts as a fail, as SIDOU defines it, and bytes past the end read as 0.

### Song model and .sng files (song.rs)

- `empty_song_is_goattrackers_cleared_song`: clearsong then savesong gives one subtune where
  channel c plays pattern c, patterns 0-2 empty at 64 rows, all three written because they are
  named. Unused instruments after the last used are dropped on save; one in the middle is kept, and
  an instrument a pattern names counts as used.
- Counts are trimmed as savesong trims them: a trailing zero table row, a trailing empty unnamed
  pattern, a trailing subtune with an empty channel and an orderlist length byte past its $FF;
  rows after a pattern's end row are dropped; a pattern named but not in the file is an empty
  64-row pattern.
- Older idents are converted and saved as GTS5: GTS4's old legato (first-frame wave $00) and
  no-hard-restart ($89) are converted, GTS3's pulse modulation speeds doubled. GTS2's vibrato,
  portamento and funktempo parameters become speedtable entries (fine vibrato $34 is left 3, right
  $40; portamento 8 is $0020; funktempo $96 is 9, 6).
- A song over the device's arena is refused with the sizes while the host's region loads it;
  multispeed 8 is refused by the device's caps and read on the host's, and the refusal names the
  file's multispeed, not gt2reloc's clamp of it.
- Trailer rules: an unknown chunk between known ones is kept in its place (`ext_set` replaces in
  place); the fixed header and lead-in (bytes 0-254) hold no $FF; a flipped byte in the GT2 part
  drops LOCK and keeps the rest; a flipped byte in the trailer drops the whole trailer and the
  song still loads; bytes after a plain GT2 part that are not a trailer are ignored, as GoatTracker
  does. The writer pads where the length would hold $FF, and the reader drops the PAD again.
- The smallest valid song grows a byte at a time: an orderlist entry adds one byte, a table row
  two, a pattern row four.
- Edit refusals: orderlists take a transpose before a repeat and a pattern last; a pattern in use
  is not removed; 208 patterns and 254 orderlist entries at most; a song's content may not eat
  into the spare (FULL). The arena bitmap is exactly the union of the index's blocks, which never
  overlap, and the spare is kept (`assert_bitmap_is_the_index_blocks_union`).
- `random_edits`: whatever the edits, the song saves and reads back the same. A trailing subtune
  with an empty channel is dropped, but savesong still counts the patterns only it names (0007):
  the next save drops them too, then the file holds.
- The reader fuzz (`fuzz_reader`, `fuzz_reader_long`, run under UBSan and ASan in the sanitizer
  builds): random bytes and mutated songs never crash it, and whatever it accepts it writes and
  reads back.
- GoatTracker Stereo's channel-count guess: instrument 1's bytes can read as three more orderlists
  ($FF end marks, restarts 0), so a one-subtune mono song passes all six trial reads. Without a
  trailer the six-channel reading does not end at the file's end and the three-channel one does,
  so SIDOU reads it as mono, as GoatTracker 2.77 does; with a trailer, the trailer settles it
  (`reviewers_mono_file` is the review's 405-byte case).
- Whatever SIDOU writes without a trailer reads back as the same song, unless the file is also the
  canonical save of a song with the other channel count, which no reader can tell apart; with the
  trailer, always. Such a file ends both readings at the same byte, so the shared trailer settles
  it only through the channel count it records (`the_trailer_records_the_channel_count`).
- Compaction never dead-ends: a review's counterexample of full-size patterns each followed by a
  2-unit hole leaves free space well above the spare but no hole for a 128-row pattern and no
  single helpful move. `compaction_random` fragments with mostly near-full patterns and must reach
  one hole within 2 x `arena_blocks` steps.
- Deferred edits: the main loop makes and queues edits; the audio interrupt applies up to 3 a step,
  in order, and acknowledges each; the main loop releases each after its acknowledgement. A
  compaction step is refused while an edit holding a slot is pending; instrument and table cells
  hold no slot. FULL is judged on the song at rest: only when the largest rebuild would not fit it.
- The deferred contract's refusals (gt2_song.h): one pending swap per pattern and no cell write
  behind it (it would hold the old copy's offset); no compaction while anything is pending; an
  orderlist edit naming the pattern waits for its swap; a pattern being removed is not reused until
  released; instruments and tables are never refused. A waiting rebuild never makes a shrinking
  one FULL: it waits (PENDING) or goes ahead.
- OPTS values gt2reloc clamps are clamped as it clamps them (-S above 16, -O and -R above 1, -F
  below 100, -V above 1); multispeed 8 and 0 (25 Hz) are kept; values with no meaning (the NTSC byte
  above 1, a SID model above 1, an unknown player flag) make the chunk invalid and the defaults
  stand.
- Orderlists GoatTracker loads and plays though they break readme 3.1 are kept as they are and
  saved as GoatTracker saves them; the C player plays them as player.s does. The edit primitives
  never make one, an entry write on one is refused, and an edit that writes the restart position
  sets it back in range.
- `random_canonical` makes songs GoatTracker's load and save leave unchanged (the last pattern
  named, the last instrument's AD non-zero, `USED_INSTRUMENT_AD_BIT`). `set_mono_from_spec` builds
  a mono song through the edit primitives because the reader would take an ambiguous file as
  stereo. `Song::duplicate` copies the region byte for byte: it holds no pointers.

### Player in the audio interrupt (song_ctl.rs)

- song_ctl.rs runs the player in the audio interrupt on the host: the shape's analysis in slices
  (the same shape as at once, for every example and a hundred generated songs), the frame clock,
  the transport's commands, and edits while a song plays.
- The call period is gt2reloc's CIA timer, exact: q + r / den == cycles * 24e6 / (clock * 544).
  The transport plays what `gt2_play_init`'s player plays, call for call, every call at floor(k x
  samples a call) from the play's block.
- `edit_case`: from the call after every edit has reached the interrupt (and, when the shape's
  change is one the state cannot follow, from the hand-over), the reference is a player started on
  the edited song and fast-forwarded to the same call; once quiet, a fresh analysis of the edited
  song is the shape the player ends on. `edit_song`'s pattern 1 is read by nothing before call 90.
- Widening cases: AD and tempo commands never used before, a new instrument's wavetable program
  with a calculated speed, a new orderlist entry naming a new pattern. Hand-overs: a wave delay
  changes how the player's wavetable state reads (the packer recodes the waveforms); a channel's
  first note, where it held only rests so the packer played two channels.
- More edits than the ring and the edits in flight hold each wait and go later; none is lost, and
  no block applies more than `SONG_EDITS_PER_BLOCK`. A rebuild's old copy stays allocated until the
  interrupt has applied its swap.
- The events are the player's, with the call's number; when the main loop does not read them for
  long, the ring keeps the newest and counts the dropped. A multispeed the device cannot play in
  one call a block (past 4) is refused. A chip swapped in (a profile change) gets the player's
  registers before it renders.
- KEYS A: a voice the keys take keeps their writes (the song's are dropped, and no gate-off is
  written over the keys' note); given back, it gets the song's registers again at the next block.
  A take holds from the interrupt's next block with no command behind it, so no pass, full ring or
  edit leaves the song a call in which to write over the keys' first note.
- Audition: with no song played yet, the analysis then a paused start; a jam sounds the instrument
  at the next call and no row is read. An audition after a stop, chip A handed back meanwhile,
  takes chip A again and leaves the song where it stopped.
- TABLE's ARP helper is heard before SAVE as a draft: a jam with the draft sounds call for call as
  the same jam once the rows are in the table, the song's own rows untouched; a turn while the jam
  holds plays the new step within a few calls with no new note; with the song playing, only the
  draft's channel plays it. The note's first call holds the voice's last frequency (the hard
  restart's).

### Parameter locks (song_locks.rs)

- A locked row's register holds the lock's value after each of its calls, the player itself plays
  as without locks, and the song's own value is back at the next row. A put on an existing
  pattern/row/param is a new value, not a new lock. A changed GT2 part fails its CRC, so the locks
  go.
- A LOCK chunk whose CRCs hold but whose records are out of range or out of order is dropped when
  read, by both readers (the block reader and the streamed `gt2_stream` the device loads from the
  store): a filter-mode lock of 8 would set $18's bit 7 (voice 3 off), and unsorted records hide
  locks from the row search.
- At a pattern's last row the look-ahead holds row 0 of the pattern and of the next; with all five
  filter and pulse locks on both, the next pattern's row 0 plays all five (eight slots lost its
  ROUTE and PULSE).
- Changing a lock in place while a row edit is pending is refused as adding one is, so the value
  never lands on rows about to move. A row that starts while an edit's look-ahead is in flight
  (`lock_edit_seq`) is resolved once it lands, not marked seen and skipped.
- A chip swapped on a locked row gets the row's locked cutoff and pulse with the player's
  registers. Per-step chance (`SONG_LOCK_CHANCE`): the roll is seedable, a chance of c plays about c
  in 100, and streaks that read as a pattern are checked (a run of 12 equal rolls is rare at 50 %).
- A held STEP step plays its locks: its audition gives the same registers call for call as the
  sequencer playing the row; while held its locks stay over the rows the song plays, and at its
  release they go from the next call.

### Song editor races (song_edit.rs)

- A shift publishes its post-edit lock cache in the same block as the pattern swap: the ISR may
  start row 1 then and must never see the old row 1 lock.
- `interrupt_starts_row` provokes the interrupt starting the next row (or another channel's
  locked row) after the editor sent its command but before the commit is visible; the reserved
  cache is rebuilt and another channel's character locks are not delayed.
- Ordinary row shifts may move or discard lock records but never consume the restore capacity
  (`check_restore_serialization`). An order edit waits for the row-sequence refresh even when an
  unrelated voice advances meanwhile and a preceding shape command acknowledges.
- The pages' fake queue holds every edit until an explicit audio-boundary acknowledgement, which
  catches any write to the live song that bypasses the queue.

### Song pages, STEP, REC and TRACKER (song_pages.rs, step_view.rs, song_rec.rs, tracker_view.rs)

- The batch writer (the ARP helper's and LAB's send) writes table rows first, then the instrument,
  one acknowledged edit per differing byte, so the instrument's pointers never name rows not
  written yet; a guard refusal ends the batch.
- LAB's send takes the lowest free instrument and puts each program after its table's last row (a
  speed row the table has is shared). EDIT + send over an unedited sent instrument writes its
  programs where they were and blanks rows no longer needed; over one edited since, the programs
  go after the tables' last rows and its old rows stay.
- The ARP helper writes plain GT2 wavetable rows, which GoatTracker's packed player plays as the
  helper says: each step's pitch for `speed` ticks, repeating from the loop step or the last held.
  At 1 tick every row carries the waveform and the program ends with a jump to the loop step;
  slower, the later rows are delays of `speed - 1`. Offsets below the note are 7-bit two's
  complement. One step, and a loop of one, sound as a hold, so they need not decode to the same
  steps.
- `FROZEN_MELODIC` and `FROZEN_DRUMS` are DICE's rolls with seed 7 from before DICE had settings: a
  song with no DICE chunk, or with the defaults, must keep rolling exactly them.
  `RHYTHM_HIT_PERCENT_AT_MEDIUM` is each place's chance of a hit for the rhythms past STRAIGHT
  (controls.md "STEP"). DICE rolls from a seed mixed with the press's time;
  `roll_with_seed_at_a_fixed_time` fixes the clock so two sessions roll alike.
- DICE by the lane's role (app/song_gen.c): chords on an ARP lane go through shared wavetable chord
  programs written once after the table's last row (TABLE FULL with nothing written when there is
  no room); on a BASS+DRUMS lane the kit's kick and snare go on the rhythm's strong rows and the
  lane's instrument between; the kit is found by its bytes, NO DRUM KIT without it. The role is
  kept in the SCNE chunk, which an older SIDOU reads as written.
- In TRACKER with REC on and the song stopped, ARP alone writes a key off and LFO alone a rest, so
  each waits for EDIT: landing a few ms before EDIT is the same MUTATE as EDIT first, with nothing
  written. Alone they write when let go, when another control is used, and at once while REC
  records.
- REC's key lag is `REC_LAG_SAMPLES` (265 samples, 6.0 ms) in 256ths of a call (`LAG`): in OFF a key
  less than LAG into a row was played in the row before; in NEAREST the half-way point moves on by
  LAG. A key the lag puts before row 0 goes to the last row of the entry that played before it,
  unless what played before is not certain (the first entry, a play started on this row, a repeat
  between).
- REC subtracts the entry's transpose so the pattern plays back the pitch heard; a note the
  transpose takes out of C-0..G#7 is refused with NOTE OUT OF RANGE (0xBD..0xBF are REST, KEYOFF
  and KEYON). A key past a repeated pattern's last row goes to row 0 while the player repeats it
  (`song_status.again`), to the next entry on the last pass.
- A row step in TRACKER sends at most 80 cells (`ROW_STEP_CELLS_MAX`, 73 measured) and a bar step
  104 (`BAR_STEP_CELLS_MAX`, 95 measured: its two rows at 2x, the strips and the position), each
  about 10 % above the measurement. A page turn is spread over frames, each within
  `SONG_VIEW_FRAME_CELLS` (200).
- `song_view_bpm10` is checked against the formula: the CIA period for multispeed N is 0x4cc7 / N +
  1 on PAL (0x42c6 on NTSC), 25 Hz is 2 x 0x4cc7 + 1; funktempo's two speeds are summed, so 5 and 7
  play as 6. The fake binding (`song_view::Fake`) plays only once the keys left chip A.
- A STEP-made song plays as GoatTracker's player does, call for call; a SET persists from its row
  to the next bar's note.

### Song files and controls (song_io.rs, demo_song.rs)

- ABOUT's tune and the song coming back at its close are returning reads (the song views keep their
  places); a load, NEW and a lost file's built-in stand-in are not (ergonomics review E27).
- LOAD's built-in song (SIDOU LOOP, in the image) loads like a file but is unnamed, unchanged and
  replaces no stored file: SAVE writes a store copy. An unchanged SAVE is skipped only while the
  file it would save to is still the one loaded or saved; after a SysEx DEL or a replacing PUT it
  writes again.
- A .sid tune takes the song's memory (song_io_away): every action is refused and nothing is read;
  when the tune is done the song comes back from its file by name, as saved, its edits gone. A
  sound edit is no edit a tune would lose: song_io_edited says 0 while song_io_changed says 1.
- LAB's USER sounds' file is no song's: SAVE AS, a load and a delete refuse its name, and SONG's
  list leaves it out.

### Drum kit and digi drums (kit.rs, dice.rs, digi.rs)

- `every_new_instrument_starts_on_time` plays every kit drum and NEW's lead and bass through
  GoatTracker's player on reSIDfp and SIDOU at nine frame lengths (rate 0's period is 9 cycles,
  so every phase of the rate counter at the gate). It guards two ADSR delay bugs: a drum's release
  in its gate-off first frame made drums 53 ms late, so a drum's SR is $00 until its SR row; and at
  attack 0 the gate selected the decay's rate for a cycle, so every attack is 1.
- At tempo 5 and up a key off on the next row (STEP's staccato) leaves a drum's envelope as without
  it, since its release is its decay and its SR row comes first. RIM is excluded: its release is
  shorter than its decay, its key off heard.
- The kick falls from its click to a body near 58 Hz with most weight below 120 Hz, the toms by a
  third or more, the high tom a fifth above the low; the clap's three bursts are in its first
  120 ms. NEW's bass is the lead an octave down ($74) with a quarter-width pulse row of its own.
- 1.0 wrote a three-byte DICE chunk (version, density, scale); it reads with rhythm and range at
  their defaults, and 1.1 writes five. Byte 2 is lane 1's scale (what 1.0 reads), bytes 5 and 6
  lanes 2 and 3's, each falling back to lane 1's when short or out of range.
- The digi drums (decision record 0009): GoatTracker keeps the GT2 part and drops the DIGI chunk.
  The built-in samples are 4-bit codes ending at rest (8), under `MAX_SAMPLE_CODES` (5,000)
  together. A digi note holds each code `SONG_DIGI_SPAN` samples at (code - 8) * `SONG_DIGI_LEVEL`
  over the chip's output, its voice's gate off meanwhile; the 8580 gets the 6581's level, and a
  muted voice plays no digi.

### Showcase songs and the starter pack (showcase.rs, starter.rs, song_embed.rs)

- Showcase songs use only GoatTracker's own sounds so they play the same in GoatTracker
  (row-by-row filter moves are its `C` command and filter table, not parameter locks) and play on
  chip A only. The `CHPA` trailer adds the chip: a 6581 song takes the 6581 R2
  (`PROFILE_6581_R2`) with the AUTHENTIC envelope, a song's default (decision record 0005); the
  device refuses a CHPA whose profile is not its model's (SNG: BAD CHIP SETUP).
- Each showcase song is a canonical GoatTracker file whose every write of a pass equals
  GoatTracker 2.77's packed player in gt2-ref at gt2reloc's defaults, and renders one pass of
  60-90 s on both models with no silent second and no sample at full scale. `SIDOU_SHOWCASE_WAV`
  writes the renders (`SIDOU_SHOWCASE_STEMS=1` one per voice); stems leave the other voices
  unwritten, so a voice synced by a silent one loses that.
- `Kit` notation: a row is `.`/`.N` rest(s), `^` key off, `+` key on, a note (`C-4`, `C#4`, `Eb4`)
  or an alias, then optionally `/instrument` and `@X` with two hex digits or `=` and a speedtable
  name; `|` is ignored. An orderlist is pattern names, `+N`/`-N` transposes, `xN` repeats and `|`
  the restart. Table jumps are relative to the program (`jump(k)`, 1-based; `jump(0)` stops).
- Starter pack: every tune has its `.md` with an allowlisted licence (`LICENCES`; a written
  permission needs a file in docs/permissions/) and the file's SHA-256, is not RSID, and plays 10 s
  on its own model with no late write, clipping or silence. `SIDOU_STARTER_UPDATE=1` rewrites the
  files from their sources.
- A multispeed tune gets the ten-byte stub GoatTracker's own `.sid` writer (greloc.c) puts before
  the player, `LDX #lo / STX $DC04 / LDX #hi / STX $DC05`, setting CIA 1's timer to a frame over
  the multiple; the tune loads and inits ten bytes lower and the stub falls through into the init.
  A song without a `CHPA` is GoatTracker's default, a 6581.
- `transport::Transport` runs `song_ctl`'s two sides as the device does, adding the digi drums
  after the tail gate as audio_fill does. `player_state` compares players by their bytes up to the
  last field (the tail padding is not data), pointers left out.

## PSID player

### Transport and placement (psid_ctl.rs)

- `PASSES` (8): the device makes about 29 main-loop passes a block (a TIMER5 tick each, 10 kHz,
  against 345 blocks a second); 8 keep the 6502 ahead (a block is 2,850 cycles, a slice up to
  1,000, fewer where the work estimate stops it).
- `AHEAD_BOUND`: the 6502 runs ahead of chip A by at most the look-ahead, a slice and one
  instruction's overshoot of the slice's budget (`LONGEST_INSTRUCTION_OVERSHOOT`, 7).
- In each block the first `PSID_SPLITS` - 1 renders end at writes (those with a sample before them
  since the last); the remaining writes go in order at the block's end, as late writes.
- The flash day's dense players (docs/hardware-results/2026-10-25-m3-flash-day.md): Thomkat
  writes 23 to 26 times in 16 to 19 samples, Kill Me about as many; writes 15 to 18 cycles apart (a
  sample is 22.3) exceeded the 15 renders 16 splits allow and came late.
- The ring's worst case is STA abs unrolled, a write every four cycles: the 6502 waits for room and
  nothing is dropped; each block takes `PSID_BLOCK_ENTRIES` entries. A write every 240 cycles for
  3 s wraps the ring 12 times and its indexes cross 2^32.
- A mute is the chip's own fader on its output, so a stop is compared with a run muted at that
  block; a play after a stop loads the tune again and starts at init.
- `psid_ctl` is 6,256 bytes ILP32 (laid over the song's overlay), with the lift's pointer;
  `psid_env` is 200 bytes, its share of the main loop's overlay with the I/O page's shadows.

### Environment (psid_env.rs, psid_fixtures.rs)

- FM-1 SID replaces CIA periods under 1,000 cycles with the frame; the C64 does not, and neither
  does SIDOU (`short_cia_periods_are_honoured`).
- The speed bits: bit n-1 for song n; above 32 a v2+ file with the PlaySID flag clear keeps using
  bit 31; with the PlaySID flag, or in v1, the 32 bits repeat.
- Play address 0 goes through the KERNAL's $0314 vector with its stack (S after the sentinel, P
  and the KERNAL entry's A, X, Y), leaving $01 as the tune set it; with the KERNAL out it goes
  through $FFFE and RTI. With $0314 never set it goes to $0000, the sentinel, and returns at once.
- An init that never returns stops after 5 s (5 s of either clock is a multiple of JMP's 3
  cycles); a play call that never returns stops after a second, at the first JMP boundary past it.
  An overrun's missed interrupts are one pending interrupt, taken at the return. A budget that ends
  on the due cycle starts the call and runs none of it.
- The SID windows are $D400-$D7FF every 32 bytes; a second SID at $D420 takes priority and chip A's
  mirrors resume at $D440; at $DE00 it is outside the SID range. A third SID's writes are dropped
  and counted, its reads are 0. The VIC's registers mirror every 64 bytes, a CIA's every 16.
- FM-1 SID's raster reads 0 from each call's start and its CIA timer read returns the latch;
  SIDOU's raster comes from the absolute cycle and its timer counts down from the latch from its
  underflow. A write-only SID register reads the bus until its lifetime is up: 0x1d00 cycles on a
  6581, 0xa2000 on an 8580.
- Every tune is a fixture built at test time with gt2-ref's assembler in strict mode (illegal
  opcodes through `.BYTE`); none is music, each shows one behaviour through its writes, with its
  cycles as named constants: `INIT_RECORD` (12, writes A, X, Y), `BANK_RECORD` (12, maps I/O in so
  the write lands whatever the bank), `IRQ_BANK_RECORD` (7, leaves the bank, since the next call's
  vector depends on it), `COUNT` (12).
- Timing fixtures rely on exact cycle counts: `delay(n)` is n * 1286 + 1; `exact(n)` takes exactly
  n cycles (0, or 2 and up: no instruction takes 1), never leaving 1 cycle for the last part. Read
  fixtures fix the read-to-write distance the tests assert (OSC3 4, ENV3 6, $D012 8 or 4, CIA
  counter reads 4); `bus_read(d)` needs d of 4 or more.

### Header parser (psid_file.rs)

- Every file is built by the test; none is a tune. `parse` takes an exact-size heap copy, so a
  sanitizer sees any read past the end. The fuzz must reach acceptance (`a > 10 && m > 100`), or
  the invariants prove nothing.
- Too short is "not a SID" before the magic is looked at, so a short RSID is too; an RSID is
  refused with its own code even when otherwise malformed (the magic is what the user needs).
- A v1 header has no flags; the bytes after it are data. Load address 0 reads the effective one
  from the data's first two bytes, as psid_ctl_load does.
- v3/v4 model bits 6-7 and 8-9 are SID 2 and 3's, 0 meaning "as the first"; v2 has none, and v5
  ignores them. Every string's last byte is a NUL whatever the file holds. v5 packs the strings,
  and a 32-byte name with no NUL falls back to three fixed fields (the format's own example).
- v5 multi-SID: the schemes' address lists and largest counts as the format lists them (scheme 3
  from $DE00 reaches $DF00 and no further); an invalid start is the scheme's own; reserved schemes
  are refused; past three chips the rest are dropped with a warning. $DF00 holds a chip, so FM/OPL
  there is refused. The embedded song-lengths table (bit 10, v5 only) is not data.
- FM-1 SID's loader mishandles crafted files (docs/upstream-reports.md, 1): a data offset at or past
  the file's end, or the old loader's length wrapping with a nonzero load address. The parser,
  sid_io ("PAST", refused as BROKEN .SID before anything loads) and the SysEx engine ("CRAFTED",
  a header and one 16-byte packet) all refuse them.

### Lift (psid_lift.rs)

- A lift captures a voice's next note from a tune's writes and turns it into a GT2 instrument. The
  capture starts at the armed voice's gate-on, keeps a frame a call, and ends at the gate-off (that
  call not kept), at the next gate-on (also a hard restart inside a call), or after a second. In a
  2SID tune either chip's voice starts it; a 1SID lift never watches chip B.
- Frame 0 keeps the note before's pitch, as GoatTracker's player leaves it; frame 1 is the base
  note. An SR change becomes a $F6 wave row; a filter-routed voice gets filter rows.
- A register written on and on within a call is a digi and is refused; eight writes of a register
  in a call are a player's business, not a digi.
- GoatTracker's pulse and realtime optimisations skip table steps and effects at the song's own
  row ticks, so they are off when a lift plays back. Under a fitted vibrato the frequency is
  approximate (GoatTracker's calculated vibrato steps by the note's step >> a shift): its mean
  difference is at most half of what holding the centre note would leave. GoatTracker's simple
  pulse writes the middle byte to both registers, so the low nybble is the high one's.

### On the host (psid.rs, psid-ref)

- `psid::Player` runs `psid_ctl`'s main and interrupt halves as the device does. The mute hook goes
  straight into the chip because the host's pass runs between blocks, where the device's
  REG_RING_MUTE reaches the chip; the render hook is the plain render.
- `as_applied` places each logged write where the interrupt applied it: in the first block that
  had it published, at its cycle, or at the block's start if the chip had passed it (late). With no
  split budget spent, `reference_render` of these is the player's output sample for sample.
- `DEVICE_SAMPLE_RATE` is 24 MHz / 544 rounded, 44,118 Hz; `FILE_MAX` is `STORE_FILE_MAX` (64 KiB).
  `model_of` plays a header's model as the oracle does: the 8580 for "8580", else the 6581. `grown`
  fills a tune out to the largest file (zero page above $7C zero, the rest a pattern, as a C64's RAM
  holds something there too) and it plays as the original does.
- `c/psid_oracle.c` is a derived copy of FM-1 SID's `player/psid.c` (fm1-emu, BSD-2-Clause) with
  SIDOU's additions between `/* psid-ref: begin */` and `/* psid-ref: end */`; oracle.rs proves that
  removing the marked blocks gives the vendored original byte for byte. It keeps all its comments,
  upstream's and SIDOU's, for that fidelity.
- `packed` builds the oracle's corpus with GoatTracker's own packer: PSID v2, PAL, VBI; at $D420 a
  v3 header naming it the second SID.

### Loading and SID views (sid_io.rs, sid_view.rs)

- The browser's list never shows the SysEx staging file MIDI INSTRUMENT, although it carries file
  type 2: that type was the staging file's before .sid files took it.
- A tune loads in parts: the 64 KiB cleared `SID_IO_CLEAR_BYTES` a poll, then the data read straight
  into it `SID_IO_PASS_BYTES` a poll; memory and writes then equal psid_ctl_load of the whole file,
  for `FILE_MAX` too. The memory starts with garbage (`SONG_BYTES_LEFT_IN_MEMORY`), the song's
  bytes on the device.
- A tune loaded in parts has no file to reload a subtune from: a song change loads the tune again
  from the store once it is quiet (stopped and its stop acknowledged by the interrupt's half). A
  playing tune is not quiet, so sid_io_load refuses.
- The views are tested against a fake of the tune's player (`Tune`, app/sid_transport.h) standing
  for main.c's binding. Every frame stays within `SONG_VIEW_FRAME_CELLS`; `sid_view::settle`
  asserts it.
- The LCD goldens (tests/golden/lcd/, `SIDOU_LCD_GOLDEN_UPDATE=1`) are 4-bit indexed PNGs in the art
  palette in sRGB, the palette emu-gate's goldens use (`sidou_tests::logo::PALETTE`).
- STATUS's 6502 rows replace the sound's parameters on page 1 while a tune is loaded: the state,
  cycles a second, the main loop's share ("--" before the first reading), overruns, late writes,
  the shadow and a third SID's dropped writes.

## Store, settings and SysEx

### Flash layout (store_layout.rs)

- The store is whole 4 KiB sectors, [`STORE_LO`, `STORE_HI`): 73 sectors, 292 KiB. The range is
  the update loader's `LDR_DATA_LO`..`LDR_DATA_HI` and Felucca's main store
  `FL_DATA_LO`..`FL_DATA_HI`. The loader writes only the app area and erases what looks like an
  update record except in that range: exactly the store. Its write gate (`FL_RANGE_OK`) is the
  app area up to the store and from the store's end on. store_layout.rs reads those sources as
  text, so a change there fails it.
- The store ends where the update protocol's staging area begins (ota.c `OTA_AREA` 0xE0000, 0x5000
  long): they touch, never overlap. Both lie inside the package's VM descriptor (0x93000, 352,256
  bytes); VM is the container, PRCT covers [0, 0x93000) and app.bin and cfg_tool.bin are app-area
  offsets, so the overlap check skips them (`CONTAINERS_AND_APP_RELATIVE_ENTRIES`).
- The guard band is the last 256-byte page of every sector (`STORE_SECTOR_USE` 0xF00) and stays
  erased; a page program wraps inside its page, so it cannot reach it. An update record is 80 bytes
  at "4 KiB boundary - 256" (ota.c `OTA_RES`), its magic 0x5441 at +6: all inside the band, which
  reads 0xFF, so the loader never takes store data for an update record.
- The app slot: app.ld's `XIP ORIGIN = 0x02000120, LENGTH = 0x8DFBC`. The SPL maps flash 0x4000 at
  XIP 0x02000000 and app.bin sits at 0x120 of it, so the slot ends at 0x4000 + 0x120 + LENGTH,
  below 0x93000 and the store. sidou-tests depends on sidou-pack for the package's descriptors.

### The simulated part (store.rs)

- `store.rs` runs app/store.c on a simulated NOR part with the real part's semantics: a program
  only clears bits, within one 256-byte page; an erase sets a sector to 0xFF (store-format.md is
  the format). It flags what the store must never do: program the guard band; program bytes not
  erased (the same bytes again are fine, and so is a kill over bits still meant 1); program across
  a page; use a sector whose erase was cut before erasing it again; program from a source in flash;
  run more than one flash operation in a `store_poll`.
- It answers a share of operations with STORE_LATER (`Sim::later`), fails a share with STORE_ERROR
  after doing part of them (`Sim::fail`), suspends erases midway as erase-suspend would
  (`Sim::suspend_percent`; a power cycle abandons it, the sector stays torn), and models worn
  sectors in which every erase and program fails (`Sim::worn`).
- Faint bits: a cut can leave bits neither programmed nor erased. Each mount reads them anew, 0 or
  1 at random, until a program clears or an erase sets them (`Cells::faint`, `Sim::reread`); with
  `Sim::rereads_in_session` they also read anew within a session (marginal bits), and with
  `Cells::drifts_to_erased` each read takes 1 three times in four.
- How a cut tears an operation (`Tear`): a cut program clears some of its bits; with faint bits on,
  each is done, left faint or not started (`BitFate`: under 40 done, under 75 faint). With
  `Tear::pulse` an operation's bits share one progress (a program pulse charges them together). A
  cut erase reads all 0xFF, any mix of old bytes and 0xFF, or erased up to some point; with faint
  bits, cut during the part's pre-program to 0 or during the erase proper.
- `Replay` forks the cells at every logged operation, each torn in turn, then applied whole for the
  next. `Run::check_forks` mounts every fork, and every `deep_check_every`th (8) also refills and
  writes on it and mounts again.
- `Sim::page_progs_max` counts the programs into each page since its sector's last erase, less one
  for each cut that fell in it (a claim cut and read as never made is made again): the measure
  behind `STORE_PAGE_PROGS`.

### Power-loss runs (store.rs)

- After a cut a key reads its committed value or the interrupted write's. Until a record of the
  next session completes, a mount may read the interrupted write as made or not (store-format.md,
  "What is left"; `Run::undecided`); once a mount has read a value, the next mount reads the same.
- A write never takes the pool under its minimum (a deletion in a full store one under); garbage
  collection's copies may use its reserve below that, until the victim's erase.
- Fault mix per seed (`set_tear`): for two seeds in three an operation's bits tear together and
  faint bits also read anew within a session; for two in five faint bits drift toward erased; for
  one in four erases are suspended midway now and then.
- The quick runs take window sizes in turn (`WINDOWS`): the device's (16, 64, 256), the smallest
  (1), and sizes that split the 32-byte sector header, the 16-byte record header and the 4-byte
  commit word unevenly. File runs keep files small at the smallest windows, or the run takes
  minutes. The long runs (ignored; TESTING.md) take thousands of seeds and every window from 1 to
  256; `power_loss_sweep` runs one of their seeds for a parallel sweep (`SIDOU_STORE_SWEEP`).

### Store rules (store.rs)

- The store's RAM on the device is at most 2,000 bytes, the plan's budget (`STORE_RAM_BUDGET`).
- Foreign data: one foreign sector in an otherwise blank region makes it foreign. Nothing is
  written until the user confirms a format, then nothing until every sector is erased: a foreign
  sector left would make the next mount foreign again. A cut during a blank region's first erase
  can leave a sector neither blank nor SIDOU's (the pre-program to 0), so the format screen must
  expect FOREIGN there too.
- The tombstone rule: the sector holding a deletion may be collected before the sector holding the
  value it deleted, and the deletion must not come undone at any cut; a tombstone goes only once it
  has nothing older to hide.
- Format migration: `tests/data/store-v1.bin` is a STORE_FORMAT 1 region that every later version
  must mount to the same values; `SIDOU_STORE_FIXTURE_UPDATE=1` writes it, under format 1 only. On
  disk it is runs of non-0xFF bytes, each `[u32 flash offset][u16 length][bytes]` little-endian (a
  run ends at 8 bytes of 0xFF in a row or at 65,535 bytes).
- A file may begin while a collection is under way: a SysEx PUT waits only for the record being
  written, so a PUT with keys held does not wait for a collection that can only finish when quiet.
  The collection waits for the file (store_poll serves it first); while a file waits to begin,
  maintenance takes no step and nothing is erased.
- An open file's fresh key is taken at its begin, so a small write cannot take the key index's last
  slot from under it. A file whose sender went away is aborted after `STORE_FILE_IDLE_MS` without a
  byte; time with the staging buffer full does not count, and an ended file commits however long
  the store takes.
- Replacing a file is checked against the space it frees: three songs of 17 sectors (63,648 bytes)
  nearly fill the store (three of 64 KiB never fit: 3 x 18 sectors is more than the 53 outside the
  pool's minimum), and re-saving one at the largest size succeeds.
- Faint structures: nothing written after a faint commit word, erase header, claim word or kill may
  depend on it, and once a mount has read it valid it stays so. A faint erase header is programmed
  again by the sector's claim before records go in; a sector read as killed is erased before the
  session writes or collects (never killed again), and nothing but its erase header may program it
  before it joins the pool clean.
- A commit word is one program whatever the window, so a cut leaves all 32 bits in play; at a
  window of 1 a cut in its first byte left 8, often all faint, and the write read committed at one
  mount and cut at the next.
- The newest record is special: a write cut in its commit word (read erased) counts at every mount
  and is copied forward (a new number, programmed whole) before the session writes, so a later
  delete and collection never bring an older value back; an older record whose commit word reads
  erased never counts. The newest record completed at the mount is indexed last, so with the index
  full it is the one left out.
- The work bound (host proxy for `STORE_POLL_WORST`): a mount's polls check at most a slice
  (`STORE_MOUNT_SLICE` bytes) or one record alone, when its CRC and key lookup are more; every
  other poll, and the API calls outside store_poll at their worst (a full key index, records spread
  over many sectors, the pool at its minimum), at most `STORE_POLL_WORK`.
- A store the platform never mounted (an unknown flash part) refuses every write at once and for
  good with STORE_ERROR; a BUSY answer kept a SONG SAVE waiting for ever (review 1.1). A sector
  whose erase always fails is retired for the session after its second failed job in a row, so
  maintenance goes on with the others (review 1.2); store_stats counts it.

### Settings record (settings.rs)

- Byte 1 is CARDS: 0 OFF, 1 ONCE (an old record's ON), 2 ALWAYS; anything else reads as ONCE. The
  meaning was widened without a schema change.
- A later schema appends fields: this firmware reads a newer record's v1 part and skips a later
  schema's extra expert values before the learn bindings. Schema 6 appended SPREAD (a schema 5
  record, or a SPREAD over 100, loads 0); schema 7 SID MODEL (AUTO otherwise); schema 8 the learned
  CCs of the parameters past the experts, which schema 9 ends with their count, so a later
  schema's longer list is read for the ones this firmware knows. Those parameters' values are the
  sound's, not this record's.
- An out-of-range value (a corrupt field the CRC still matched, or a range a later firmware
  narrowed) is skipped and the parameter keeps its value.
- `tests/data/settings-v1.bin` (schema 1) and `settings-v3.bin` (schema 2 with THEME appended) are
  fixtures every later schema must load to the same values; `SIDOU_SETTINGS_FIXTURE_UPDATE=1`
  writes the v1 fixture, for schema 1 only.
- An explicit save writes the unsaved sound edit first, then the record, then commits. On a short
  pool an explicit save waits, then after `SETTINGS_FORCE_MS` forces the held operations and says
  SAVED (GAPS). On a store never mounted it says it failed at once; a program error after the store
  took it says it failed, not SAVED, and the debounce writes again.
- `a_failed_background_save_is_retried_lands_on_recovery_and_says_so_once` needs a failure rate at
  which the save keeps failing over four debounces; which rate does depends on the simulated
  draws and so on the records' sizes, so it tries a list (`failure_rates_to_try`).
- RESET puts every setting and every preset's edits back to a first boot's, keeping the version,
  the last song and the MIDI learn bindings and leaving other store keys alone, and saves at once.
  A reset whose save had to be forced still says it reset.

### SAVE and STORE (save_key.rs)

- SAVE saves at a clean release, a press and release within one input frame included. Another
  button pressed (even in the same frame) or a knob turned while SAVE is down spoils that save; a
  button held from before and let go meanwhile does not (only presses count).
- The tests take the value popup's source aside (`Saving::popup_source`): the popup would sit on
  the menu cells they read. A save answers once both records are kept: until then a toast showing
  is not its answer (`save_and_wait_for_its_toast`).
- In the menu SAVE is the menu's OK: on a value item it saves nothing; SETTINGS has no SAVE action
  away from STORE and RESET (the settings save themselves).
- A foreign store is armed for a format by one clockwise ALGORITHM turn; SELECT away and back or a
  counter-clockwise turn disarms; only SAVE confirms. A fast ALGORITHM spin, one event of many
  detents or a stream of single ones, arms and never formats (ergonomics review E01). A newer
  SIDOU's store is foreign to this one, but the menu says NEWER.
- RESET: ALGORITHM clockwise asks, SAVE alone confirms; moving the cursor, counter-clockwise and
  HOME cancel and write nothing. STATUS shows the store's health only when it is not healthy: the
  errors since boot and the sectors retired, right-aligned on the READY row.

### Boot-loop guard (bootguard.rs)

- The guard lives in .noinit RAM, so the tests play a reset by calling `bootguard_boot` again on
  the same state.
- Felucca keeps its own boot guard one word into the same RAM (magic "DRGB", failed, pending). With
  SIDOU's magic still in place, words Felucca (or anything else) left behind start a fresh guard
  rather than counting as failed boots.

### SysEx engine (sysex.rs)

- A real flash hook may defer programming beyond the sender's deadline, so a DATA's ACK waits for
  the store and the sender deadline does not run while it waits. A store backstop may end the
  engine's claim before the engine runs; a later reset must not release a claim another owner has
  taken since.
- .sid PUTs are acknowledged before any claim and checked at their first DATA, which must hold the
  first 128 bytes (or the whole of a shorter file); a file shorter than any header is refused at
  PUT. A song (type 1) is never checked as a tune, so an RSID's bytes go in as a song.
- A .sid PUT whose claim waits for a record holds its first DATA; a byte of another frame while it
  is held is BUSY and never written over the held packet.
- LAB's USER sounds file is the firmware's own: SysEx never lists, fetches, replaces or deletes it.
- Wait for quiet: while held programs must wait (a song plays, a computer records the USB audio), a
  DATA's ACK waits; a WAIT (`WAIT_COMMAND`, 0x7d) with that DATA's sequence goes out at once and
  every `SYSEX_WAIT_EVERY_MS`, with the toast UPLOAD WAITS FOR QUIET. The wait ends
  `SYSEX_QUIET_MAX_MS` after it began with NAK BUSY, nothing replaced.
- TRS has no reply path, so a wait there is the toast alone; the one-second frame gap a TRS sender
  keeps is not enough for a wait, so a frame that comes while it waits aborts the upload.

## Generated files

### Generators and their checks

- The generated C files (app/font.c and font.h, credits.c, demo_song.c, logo_bitmap.c,
  title_card_bitmap.c, scene_rmr_art.c, live_cutoff.c, live_freq.c) carry only their SPDX header
  and one "Generated by ...; do not edit" line naming the generator. Their up-to-date tests compare
  the whole file, and each generator's update variable rewrites it (`check_or_write(update)` both
  generates and lists stale files). Generated lines stay within 100 columns.
- The generators write the generated files' own SPDX headers from string literals, fenced by
  `REUSE-IgnoreStart`/`End` so reuse does not read them as the generator's licence.
- `app/demo_song.c` and `assets/songs/sidou-loop.sng` (SIDOU LOOP, which boot copies into the song
  region) are generated from `sidou_tests::song_embed` (`SIDOU_SONG_UPDATE=1`). The song is a
  canonical GTS5 file (decision record 0007): GoatTracker's own load and save give it back byte for
  byte, so it opens in GoatTracker unchanged.
- app/logo_bitmap.c and app/title_card_bitmap.c come from assets/logo/ (`SIDOU_LOGO_UPDATE=1`
  rewrites both); app/live_cutoff.c and app/live_freq.c from `SIDOU_CUTOFF_TABLE_UPDATE=1` and
  `SIDOU_FREQ_TABLE_UPDATE=1`.

### Font (font.rs)

- 8x8 glyphs, top row first, bit 7 the leftmost pixel; ASCII 0x20..0x7E (lower case drawn as upper
  case) and the graphics after; every code 0x00..0xFF has a glyph (`GLYPHS`), blank unless the
  sheet draws one. The firmware keeps codes 0..`FONT_LAST` (0xDC) in .rodata, read by the renderer
  only; the codes above are blank and never drawn, so `compiled_glyph` returns blank for them.
- The table is 4-byte aligned because grid.c tests a glyph for blank as two words.

### Credits (credits.rs)

- `app/credits.c` comes from CREDITS.md (`SIDOU_CREDITS_UPDATE=1`). The test that everyone is on a
  page reads the bullets with its own small parser (`bullets`, `name_parts`), not the generator's,
  so it does not share the generator's mistakes.
- Each `## Section` becomes pages headed by its title; text before the first goes under "CREDITS".
  A bullet `- **Who**: what` (or starting in bold without a colon) gives "who" lines (brighter) and
  "what" lines. Lines wrap at `CREDITS_COLS` in upper case; accents are dropped, quotes and dashes
  made ASCII, anything else outside the font becomes '?'.
- Names wrap between names, never inside one that fits a line: each part at most 4 words, split
  after ", " and " AND ", and before " (" when both halves fit ("EMILIE GILLET" / "(MUTABLE
  INSTRUMENTS)"); other text wraps at any space.
- A bullet stays on one page when it fits; a longer one starts its own page and goes over as many
  `<SECTION> (CONT.)` pages as it needs, cut where who ends and what begins. Nothing follows it on
  its last page: the credits are attribution, and a bullet's tail never runs into the next one.

### Bitmaps (logo.rs, bitmap_codec.rs)

- `bitmap_codec`'s run dictionary (a pool's first fifteen bytes are its codebook) and checkpointed
  row packets (docs/art/bitmap-codec.md) let any row or span decode without another row or a
  scratch buffer; a byte holds a run of at most 16 pixels; checkpoints every `GRID_BITMAP_SEEK`
  pixels. Pools are shared across images or kept independent, whichever is smaller.
  `byte_run_size` is the original byte-run coding, kept as the size comparison's control.
- Each logo's run pool is its own array, named after its first logo: the firmware links only the
  logos it shows (`--gc-sections` with `-fdata-sections`), so a pool no shown logo reads (Cracktro
  Chrome's, which the host tests keep) stays out of the image.
- A logo PNG is indexed, its colours art palette colours (in any order), at most the screen and in
  whole 8 x 8 cells. The default logo comes first (app/logo.h).

### RED MOON RITE's art (scene_art.rs)

- Transparency is a colour no image of the set uses (`clear_colour`); each image is cropped to the
  rectangle its pixels cover. An unarmed pose is a patch over its armed pose when that is smaller,
  and an identical pose a one-pixel patch (`PATCH_OF_AN_IDENTICAL_POSE`); the decoders draw a
  patch's base in the patch's frame, so the generator asserts they match.
- A sky is a gradient computed on the device plus the pixels it does not give (moon, wisps, hills),
  smaller than the whole sky (`skies_both_ways` measures the other choice). The three palettes
  follow red-moon-rite.js's PALS from row 16 to 140 with 6-row seams. A row wider than 32 pixels
  has a run start every 32, with checkpoints in its row packets.
- The flash estimate uses the pi32v2 (ILP32) struct sizes: `scene_image` 24 bytes (ten bytes, two
  of padding, three pointers), `scene_gradient` four bytes and a pointer, a stop 2.

### Title cards (title_card.rs)

- The glow is drawn a cell at a time with the font's dither glyphs D0..D15 in two neighbouring
  stops of its colours (black, blue, purple, red), at level L = 16 i + n (stop i, 0..`MAX_LEVEL`
  47: red never shows whole). The firmware keeps a quarter of the level table (the glow is
  symmetric about the centre) and fades in through k = 1..4 (floor(L * k / 4)).
- A card's bitmap holds only the cells that differ from the glow (letters, halos, rule, glint,
  stars), the glow's background stop flat in the others, and a bit a cell says which.
  `glow_levels` reads a cell's background at pixel (0, 3) (`NEVER_DITHERED_X/Y`): Bayer's 15 there
  is never set below D16.
- All three cards, their glow from 225 bytes of levels, stay within `CARDS_FLASH_BYTES_MAX`
  (27,087), under half the whole-PNG byte-run control (57,190 bytes when the ceiling was set). The
  ceiling stays although the newer codec also shrinks that control.

### Note and cutoff tables (live_freq.rs, live_cutoff.rs)

- `live_freq_table` is round(f * 2^24 / 985248) for MIDI notes 0..127, f = 440 * 2^((n - 69) /
  12), notes above 106 folded down by octaves to fit 16 bits; `live_freq_top_q4` is notes 95..106
  in 1/16ths; `live_freq_fine_q20` is round(2^20 * 2^(j / 768)) - 2^20 for j = 0..64, 1/64-semitone
  steps. `live_freq_at` moves between notes with the last two, within 0.6 of the exact value at
  every 1/8192 semitone, never falling and never rising by more than 1.
- K1's cutoff walk is generated from the firmware's own `sid_cutoff_hz`: per model, 65 registers
  evenly spaced in pitch from a semitone above the lowest cutoff to the highest, then per bucket of
  32 registers the first entry above it (`live_cutoff_up`) and the last below (`live_cutoff_down`).
- Step tolerances: the 6581's range is 6.8 octaves, steps of about 128 cents (40..160 allowed); the
  8580's is 11 octaves (6.8 Hz to 14.3 kHz), steps of about 207 cents (40..230), its first 29
  entries single registers (the chip's own resolution, coarse at the bottom).

## Host tools

### sidou-sysex

- `sidou-sysex` builds SIDOU SysEx packets on the host and opens no MIDI or USB device: a SysEx
  tool sends the `.syx` it writes (docs/manual/sid-tunes.md).
- `Packet` validates single packets, not transfer sequencing, file contents or CRCs.
  `Packet::decode` expects exactly one frame and discards MIDI real-time bytes (F8-FF) anywhere,
  even outside the delimiters. Upload sessions need 16-byte-aligned DATA offsets and nonfinal DATA
  lengths divisible by 16; one DATA alone cannot know the final offset, so its validation accepts
  any nonempty length.
- NAK reasons 10 and 11 are the device's own toasts and never sent; a `.sid`'s refusals are 12-15.
- `sidou-sysex sid` checks a tune with the device's own header parser (core/psid/psid_file.c
  through sidou-sys), so whatever the device would refuse is refused on the host with the same
  reason, the device's toast first, then why.
- The store name comes from the title (the file's name when it is empty or HVSC's `<?>`), uppercase
  in SIDOU's characters ($20-$5F, else `?`), at most 16; a clash with the run's names or the
  store's own (`MIDI INSTRUMENT`, `MIDI SONG`) takes `-2`, `-3`, ... in its last characters.
  `--pack` writes the tunes' uploads end to end into one `pack.syx` (the starter pack), a refused
  tune named on stderr and left out; emu-gate's `psid_store` loads the pack into an emulated store.

### sidou-pack harness

- `harness/felucca/uac_test.c` is Felucca's upstream test code (GPL-3.0-only), kept byte for byte,
  comments included.

### Licences of the host crates

- `gt2-ref` links GoatTracker 2.77's GPL-2.0-or-later code (third_party/goattracker2) with
  SIDOU's GPL-3.0-or-later shim; `psid-ref` links m6502.h (zlib) and a derived copy of FM-1
  SID's psid.c (BSD-2-Clause) with SIDOU's GPL-3.0-or-later code. Neither is ever built into
  the firmware.
- `sidou-pack` is a port of Felucca's GPL-3.0-only packer. The files it fetches on first use
  (`inputs.toml`, never committed, pinned by SHA-256, cached in build/ac79/) are JieLi's AC79 SDK
  (Apache-2.0) at tag AC79NN_SDK_V1.2.1_2023-12-13, as Felucca's tools/build.py pins them: the
  SPL (uboot.boot) goes into every package's head, cfg_tool.bin and the default EQ table into its
  app area. Every package ships with LICENSES/Apache-2.0.txt.
- `sidou-tablegen` takes its fit's transcendental functions from a pure-Rust libm, the same on
  every platform: the platform's own libm may differ in the last digit.

## Web

### Demo page (index.html, patch-main.sh, serve.py)

- index.html wraps fm1-emu's web frontend (MIT): main.js, persistence.js, worklet.js, jit.js,
  fwsc.js and fm1_web.wasm are upstream's, copied at the pinned revision by tools/web-demo. main.js
  finds its elements by id and observes `<header>` and `.bar`: a redesign keeps every id it uses.
- `.panel-box`'s max-width keeps the whole synth in view on a laptop (upstream's rule; `--above` and
  `--aspect` come from main.js); the box is the canvas's size, so Start can sit on the LCD.
  `#start`'s `--lcd-centre-x/-y` are the LCD's centre: 480 x 480 at (370, 42) on fm1-emu's
  1280 x 820 panel.
- The site's palette is the C64's (docs/art/README.md): blue, light blue, cyan, light green, yellow,
  light red; every page uses the same variables.
- patch-main.sh changes fm1-emu's main.js; each edit must apply exactly once or the build stops, so
  a new upstream revision gets a look. The default clock is 240 MHz, not 100: emu-gate measures the
  audio budget at 240 MHz, and fewer instructions a second can starve the audio in emulated time.
  Start is enabled only once its handler is attached (main.js awaits the catalog and the wasm
  first, and an earlier click did nothing).
- serve.py sends `Cache-Control: no-store` so a rebuild shows on reload. fm1-emu's page needs no
  COOP/COEP headers: it uses no SharedArrayBuffer. Port 0 picks a free port; the first line printed
  names it (check.sh reads it).

### Smoke test (check.mjs, check.sh)

- check.sh runs the Node tests first (no browser, no MIDI), then Playwright, pinned at
  `playwright=1.63.0` and installed with its headless Chromium into build/web-check on first use;
  on CI with `--with-deps` unless the build image already has the libraries
  (`SIDOU_BUILD_IMAGE`).
- check.mjs waits for 3 s of emulated time: the short boot's title card is 0 to 1.2 s, then the
  status screen wipes in; `PAST_TITLE_CARD_START_MS` (500) captures the card to prove the screen
  changes after it. fm1-emu sends the LCD picture only when it changed, so the watcher keeps the
  last.
- Boot flash sectors reach the audio thread packed as `BOOT_SECTOR_BYTES` (4 + 4096) each;
  persistence is fm1-emu's persistence.js (IndexedDB per package). SIDOU stores its settings 5 s
  after the first boot; Save now keeps that flash and the next visit boots with it.
- `LCD_ON_CANVAS`: fm1-live's panel draws the LCD at 2x from (370, 42). `KEY_DEBOUNCE_MS` (150)
  holds a key past the key scan's debounce; `NOTE_RELEASE_MS` (800) is A4's release; `RING_OUT_MS`
  (1500) lets the song's releases ring out before "stopped" is measured.
- The fake Web MIDI has two port pairs: "Other Synth" must stay silent and "SIDOU FM-1" is a
  simulated device following docs/reference/sysex.md. `fakeLinkMidi` answers with SIDOU's own Live
  Link frames (link-fixtures.json) and ends the session `AUTO_STOP_MS` (3 s) after the last
  heartbeat, as the device does; it is a function declaration because Playwright serialises it into
  the page.

### Wasm core (core.js)

- core.js is the JavaScript side of sidou-core.wasm (tools/wasm-core): the FM-1's own .sng reader
  and writer, GTI5 export and player. A file is refused as the FM-1 refuses it, with its screen's
  message (SNG: BAD PATTERNS, SNG: TOO BIG (70K OF 64K), ...).
- Edits are the FM-1's own primitives (gt2_song.h), so a refusal is the FM-1's: an EditError whose
  code is a gt2_err, with nothing changed; `EDIT_ERR` puts the FM-1's own word first where its
  screen has one. pushRow/pullRow are TRACKER's INS and DEL. savedBytes is the song as the FM-1
  saves it, trailer included, against the 65,536-byte cap.
- write({ trailer: false }) is plain GoatTracker; its `warning` says when reading it back gives
  another channel count, since only the trailer told them apart.
- Audio is made in `BLOCK_FRAMES` (128), the FM-1's audio interrupt block; renderOut and
  renderBlocks go through the effect slots as interleaved stereo; renderSong, renderPattern and
  renderInstrument are mono chip A before the slots, at the device's 44,118 Hz. `JAM_RELEASE`
  (0xff) as a jam note lets the voice go.

### Song editor (editor.js, song-edit.js, preview.js)

- Nothing is sent to MIDI before Connect and only to the selected pair; nothing sounds before a
  press of Play or a key. A song handed over by the Library's Edit travels in sessionStorage (this
  tab only). The computer keyboard follows GoatTracker's piano layout.
- SongSession's undo and redo keep whole snapshots of the song's 64 KiB region, `limit` of them;
  apply() puts the song back on a refusal (fn may have made some of several edits). The words for
  cells, orderlist entries and table rows are ported from app/tracker_view.c and app/song_pages.c,
  and editor.test.mjs checks them against the C.
- The preview runs the wasm core in an AudioWorklet, a 128-sample block at a time, through the
  effect slots, resampled linearly from 44,118 Hz (`Resampler`: equal rates pass every frame
  unchanged). The worklet is sent the module's bytes, not a compiled WebAssembly.Module, which does
  not reach an AudioWorklet in every browser.
- Without AudioWorklet (an old browser, a page not on https or localhost) it renders a stretch on
  the page's thread: 30 s of the song, 8 s of a pattern, a key held half a second and rung for a
  second. After a stop the tails ring `TAIL_SECONDS` (2). Play reads the song afresh. A preview's
  effect slot is proven heard by the output's two sides differing (the chorus spreads it).

### Library backups (store.js, zip.js)

- store.js never sees the store's private files (MIDI SONG, MIDI INSTRUMENT, SAVE AS's staging
  files): the FM-1 does not list them. typeOfFile reads the content's idents (GTS2-5, GTI2-5,
  PSID/RSID) before the name.
- storeName names a file as `sidou-sysex sid` names tunes. A send leaves an identical file (same
  name, length and CRC) alone and replaces a different one only when asked. rename is GET, PUT
  under the new name, DEL the old: the old file stays until the new one has committed. restore
  stops at the first refusal with what was done in `done`.
- zip.js writes files uncompressed (`METHOD_STORED`): the store is small and its songs compress
  little. The reader takes stored files and, where the platform has DecompressionStream, deflated
  ones (a backup re-zipped by another tool). No ZIP64, encryption or spanning; entries carry UTF-8
  names (`FLAG_UTF8_NAMES`).

### SysEx client (sysex.js, fake-fm1.mjs)

- sysex.js is SIDOU's SysEx protocol version 1, ported from tools/sysex_roundtrip.py's packing and
  framing; sysex.test.mjs checks it against the vectors sidou-sysex checks (sysex-vectors.txt) and
  Python's reference encoder.
- busyWaits: after NAK BUSY a sender waits 1 s, 2 s, 4 s, then 5 s each until about `total` ms are
  spent; a NAK ends the transfer, so the whole PUT is sent again. A BUSY that ends a wait for quiet
  is not retried (the minute is up: someone has to stop the song). DEL retries BUSY as PUT does.
- WAIT restarts the ACK timeout; the device sends one every 2 s and gives up after a minute.
  loadNow (PUT target 2) resolves once the song has loaded; its NAK BUSY (unsaved edits, an unnamed
  USB song, busy or saving) is not retried, as ending those is the player's job.
- GET of the current song (target 2) is counted and checksummed in a pass before the first reply;
  the length and CRC announced are checked and a mismatch ends in ABORT.
- fake-fm1.mjs's Device checks every frame as the real one does; its options (`corruptDataByte`,
  `nakEveryGet`, `waitForQuiet`, `saveAsFirst`) provoke each refusal. LiveLinkDevice ends the
  session 3 s after the last START or HEARTBEAT; `deaf` loses heartbeats, `mute` answers nothing.

### Live Link viewer (link.js, link.html)

- `MAGIC` is the non-commercial SysEx id 0x7D, then "FM1L". The checksum makes the command, the
  packed bytes and itself sum to 0 modulo 128. A later version's HELLO fields after the known ones
  are ignored. crc32 is sysex.js's (IEEE, as zlib).
- `HEARTBEAT_MS` (2000): the device stops 3 s after the last heartbeat. `PANEL_REPEAT_MS` (200): a
  held button is sent again this often; the device lets go 500 ms after the last PANEL. `STEPS_MS`
  (25): encoder steps wait this long for more (a wheel or a drag sends many). `KEYFRAME_GAP_MS`
  (1000): at most one KEYFRAME asked for a lost frame this often. `SILENT_MS` (5000) is silence.
- The font and palette: the viewer's built-in ones if HELLO's CRCs match, else a cached copy that
  does (localStorage by CRC), else fetched; a full or blocked store only means fetching again.
- The page sends STOP when hidden or closed and START again when shown after a hide stopped it.
- link.html scales the 240 x 240 screen by a whole number of device pixels; the room is the panel's
  width less `AROUND_SCREEN_PX`. An encoder turns a detent per `DRAG_PX_PER_DETENT` (10) px dragged
  (up is clockwise) and per `WHEEL_DELTA_PER_DETENT` (40) of wheel delta. The note keys are an
  octave from `MIDI_C4` on MIDI channel 1 to the FM-1's output, not part of PANEL.

### Songs to MIDI (songmidi.js)

- walkSong follows GoatTracker 2.77's playroutine (player.s, as gplay.c has it) call by call: tick
  counters, tempo (speed, funktempo, the F and E commands, channel tempo), the sequencer, the
  pattern fetch gatetimer frames before a row starts, the instruments' first-frame waveform and
  their legato and no-hard-restart flags.
- A note is the row's note plus the orderlist transpose. The wavetable's note column moves it: a
  relative note ($00-$7F, an unsigned byte sum, so it wraps under a transpose) or a fixed one
  ($81-$FF); when that differs from the sounding pitch the file ends the note and starts the next
  in the same call (an arpeggio is a run of tied notes). Vibrato, portamento and other effects do
  not move it; a note is `pending` until the wavetable's first step names its pitch.
- A note is a stretch with the SID's gate bit high as the player sets it (the gate flag and the
  waveform byte), so a drum whose wavetable drops the gate is as short as it sounds.
- Time is counted in player calls: call 0 only sets the voices up, so the first tick 0 is call 1.
  A call is 1/50 s on PAL, 1/60 on NTSC, divided by the multispeed (0 is 25 Hz). `MAX_FRAMES` (60 *
  60 * 50 * 4) is a hang guard: an hour of 50 Hz calls at 4x.
- The default tempo: the last instrument slot's (63) AD, if >= 2 and with no wave pointer, is
  GoatTracker's default speed (greloc.c). The packed player starts the funktempo pair at 8 and 5
  calls (mt_funktempotbl); E00 leaves the pair as it is (the editor's player; the packed one reads
  a byte before the speedtable).
- Reading: GTS3 and GTS4 kept legato and no-hard-restart in the first-frame waveform. Stereo is
  GoatTracker Stereo's determinechannels, both readings tried as SIDOU does: the one ending at a
  trailer that records its channel count wins, then at any trailer, then at the file's end; a tie
  goes to the guess. Only OPTS, MIDI and the channel count are read from the trailer, so its CRCs
  are not checked: a damaged one can only give a wrong tempo.
- toMidi writes format 1: a conductor track and one track a voice. A MIDI tick is 1/`TICKS_PER_CALL`
  (20) of a call, fewer if the beat would need more than `MAX_PPQ` (32767), so every note lands on
  a whole tick at one constant tempo. Speed and funktempo changes are markers, not tempo events,
  which would move every later tick. Events at one tick are offs before ons.
- songmidi.test.mjs compares the walk with GoatTracker 2.77's packed player (fixtures from gt2-ref's
  midi_walk.rs): gate runs up to each voice's loop point; pitch from the call after a note starts
  (the register still holds the previous note at its first call), skipping calls where commands
  1-4 put the register between table notes. Glides ending on a table note the walk does not follow
  are allowed at most one call in `WRONG_PITCH_AT_MOST_ONE_IN` (2000).
- With the GoatTracker examples checked out (`SONGS_WITH_GT2_EXAMPLES`, 17 songs) the comparison
  must cover more than 17,000 gate runs, 100,000 calls and 20,000 ties to mean something.

### Fixtures from the host (web_core.rs, web_link.rs)

- web_core.rs writes web/core-fixtures.json: what the host build of the same C makes of SIDOU's
  songs and GoatTracker's examples, so core.test.mjs can check the wasm build gives the same bytes
  and samples (`SIDOU_WEB_CORE_FIXTURE_UPDATE=1`). `fingerprint` is SHA-256's first 8 bytes, not
  the CRC: two files with SIDOU's trailer that differ only in what it records have the same CRC-32
  as a whole.
- The core's transport (`preview`) mirrors core_api.c's: chip A in the song's setup (CHPA as the
  FM-1 accepts it, else OPTS's model), the chip hooks (`ChipHooks`) and the effect slots from SLOT,
  settled by two seconds of silence. The editor fixture runs sc_edit, compacting and retrying when
  fragmented; `comet_with_locks_slots_and_character` adds what the showcase songs lack.
- `save_size_refusals` meets both refusals as the editor does: the arena (128-row patterns until
  FULL) and the 64 KiB save (one-row patterns up to 207, 51-206 removed again, each then saved as 64
  empty rows the arena does not hold, then 128-row patterns until SONG FULL).
- web_link.rs writes web/link-fixtures.json: app/link.c over a screen with plain and large text,
  overlays and a painter region, the frames it sends, the spec's test vectors and the panel's
  picture as the CRC-32 of its RGB565 pixels, which `Mirror::render` must give from the frames
  alone (`SIDOU_WEB_LINK_FIXTURE_UPDATE=1`). `paint_diagonal_stripes` paints four colours so a
  pixel out of place shows.

## Sanitizer builds

- `host/build-support/sanitize.rs` is included by sidou-sys's and residfp-sys's build scripts so both
  read `SIDOU_SANITIZE` the same way. `undefined` traps on undefined behaviour with no runtime
  library, so it works on macOS and on stable Rust: clang and newer GCC spell it
  `-fsanitize-trap=undefined`, GCC 12 only `-fsanitize-undefined-trap-on-error`, so the first the
  compiler accepts is used. `address` needs the Rust side built with `-Zsanitizer=address` (nightly),
  which provides the ASan runtime.
