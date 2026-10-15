<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Testing SIDOU

All tests, benchmarks and tools live in the cargo workspace in `host/`. They
call the C core through FFI (`sidou-sys`), so they test the same code that
runs on the device. Commands below run from `host/`. [BUILDING.md](BUILDING.md)
lists the prerequisites.

## Quick reference

| Layer | Command | Where |
|---|---|---|
| Everything except the emulator gate | `cargo test --workspace --release` | all crates |
| Unit tests | `cargo test --release -p sidou-tests --test unit` | `crates/sidou-tests/tests/unit/` |
| App logic (live instrument, register ring, MIDI parser, ring and input, the CC/NRPN map, expression, MIDI out, screen font, cell grid, views and widgets) | `cargo test --release -p sidou-tests --test app` | `crates/sidou-tests/tests/app/`; `SIDOU_FREQ_TABLE_UPDATE=1` regenerates `app/live_freq.c`, `SIDOU_FONT_UPDATE=1` regenerates `app/font.c` and `app/font.h` from `assets/font/sidou-8x8.txt`, `SIDOU_LOGO_UPDATE=1` `app/logo_bitmap.c` from `assets/logo/cracktro-chrome.png`, `SIDOU_MIDI_DOC_UPDATE=1` `docs/reference/midi.md` |
| Golden renders | `cargo test --release -p sidou-tests --test golden` | `crates/sidou-tests/tests/golden.rs`, `tests/golden/hashes.txt` |
| reSIDfp reference | `cargo test --release -p sidou-tests --test reference` | `crates/sidou-tests/tests/reference.rs` |
| Filter against reSIDfp (transfer functions, sweeps) | `cargo test --release -p sidou-tests --test filter` | `crates/sidou-tests/tests/filter.rs`, `src/filter_sweeps.rs` |
| Register reads (OSC3, ENV3) | `cargo test --release -p sidou-tests --test register_reads` | `crates/sidou-tests/tests/register_reads.rs` |
| Generated tables (drift) | `cargo test --release -p sidou-tablegen`; the filter tables with `--test drift -- --ignored` (measures reSIDfp's filter: about 20 s on every core) | `crates/sidou-tablegen/`, `core/sid/gen/` |
| Filter comparison | `cargo run --release -p sidou-tests --bin sidou-compare -- <dir>` | `crates/sidou-tests/src/bin/sidou_compare.rs` |
| Host benchmark | `cargo bench -p sidou-bench` | `crates/sidou-bench/benches/workload.rs` |
| Packager | `cargo test --release -p sidou-pack` (the round trip and golden tests need `build/ac79/`, which `sidou-pack` fills; the stock-return tests need M-VAVE's V15 package in `build/stock/FM-1.fwsc`; they skip without them) | `crates/sidou-pack/` |
| Emulator tests (gates incl. effects, boot, live instrument, screen, MIDI, latency, SRAM audio, package) | `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate -- --nocapture --test-threads=1` | `crates/emu-gate/tests/` |

Use `--release` for the tests. The C code is optimised either way (the test
profile sets `opt-level = 3`), but the reference tests do a lot of Rust-side
signal analysis.

## Test layers

### Unit tests

One module per part of the core: oscillator (`sid_osc`), noise (`sid_noise`),
hard sync (`sid_sync`), envelope (`sid_env`), voice DACs (`sid_dac`, against
reSIDfp's tables), filter (`sid_filter`), the chip (`sid`) and constants
(`sid_defs`). Most pin a documented SID behaviour, such
as the ADSR delay bug, the noise LFSR taps or hard-sync timing. `sid` also
measures the output's worst case (three voices at maximum resonance: the 8580
clips by about 1 dB, as reSIDfp does), and `sid_dac` pins the default tables'
CRC-32s, which the hardware checklist compares with the device's.

The envelope tests compare the batched envelope (`sid_env_advance`: rests, linear
runs, whole rate periods) with a per-cycle Rust port of reSIDfp's
`EnvelopeGenerator`, over random writes at random times. They also cover the gate reset
(`sid_set_gate_reset`), the one option where SIDOU departs from the chip on purpose:
- the fuzz runs again with it on, the reference's LFSR restarting at each gate edge, and
  counts the falling edges it meets in a pipeline, past divider period 1 and frozen at 0;
- a release-6-then-attack-0 retrigger at each of the release period's 267 phases starts
  within 12 cycles with it on (ENV3 within 13); with it off, 8 phases start at once and
  the rest wait over 32000 cycles, as on the chip;
- a note-off at each phase of the decay's period, for SAW LEAD, SYNC LEAD and PWM BASS,
  starts the release within one release period and 3 cycles with it on (SAW LEAD's ENV3
  within 271); with it off, one release period of the phases starts at once and the rest
  wait for the LFSR to come round, as on the chip;
- below sustain 0x5d the release's first step still waits for the exponential divider,
  within its remaining periods, and a counter frozen at 0 stays frozen, the same with it
  on and off.

**A failure means** the core no longer behaves like the chip in that
respect. Fix the code, unless the test itself pinned the wrong behaviour; in
that case, say in the commit which source (reSID, a measurement, a datasheet)
shows the new expectation is right.

The effects have their own modules: `fx` (the Task 1 chain and its blocks)
and `slots`, `slots_dist`, `slots_mod` and `slots_fx` (the effect slots and
their algorithms). Each algorithm is tested for:
- its characteristic: THD, level and aliasing for the drives; LFO rate,
  depth and phase between sides for the modulations; echo time and
  feedback decay for the delays; RT60, damping and stability for the
  reverbs;
- exact bypass, and stereo where it makes stereo (L/R correlation, equal
  power);
- clicks, two ways:
  - a parameter jump against the same move ramped over 0.2 s: the 2nd
    difference of the two runs' difference must stay within 30 LSB (a 100
    LSB step must show);
  - an algorithm switch, whose 2nd difference must stay within 30 LSB of the
    steady state's.

The drives, delays and reverbs run their click tests at low input levels.
At full level their outputs saturate, or carry broadband tails, that differ
between two runs by more than any click. The tests say where.

### Golden renders

`renders_match_recorded_hashes` renders the shared benchmark workload
(`bench/workload.c`) for a 6581 and an 8580 with one chip, and a 6581 with
two chips. It compares the FNV-1a 64 hash of each render with
`tests/golden/hashes.txt`. `workload_never_clips_and_has_no_dc_offset` checks
the same renders for clipping and DC offset.

**A failure means** the output changed, by as little as one bit. If you did
not mean to change the sound, it's a regression. If you did, rewrite the
file:

```sh
SIDOU_GOLDEN_UPDATE=1 cargo test --release -p sidou-tests --test golden
```

Before you commit the new hashes, listen to the change and say in the commit
message why the sound changed. A hash change on its own proves nothing.

Golden hashes depend on exact float results. Compiler flags that relax float
semantics (`-ffast-math`, `-ffp-contract=fast`, passed through `CFLAGS`)
change them; see [BUILDING.md](BUILDING.md).

### reSIDfp reference tests

These render the same register writes on SIDOU and on reSIDfp (libresidfp,
test-only) and compare them:
- **pitch**: an unfiltered sawtooth at A4 must be within 0.1 cents of the
  theoretical frequency and of reSIDfp, for both models (a reSIDfp result off
  from theory points at the harness, not the core);
- **level**: one unfiltered voice (sawtooth, triangle, pulse, at two pitches)
  must be within 0.2 dB of reSIDfp's RMS level, for both models;
- **envelope shape**: the RMS envelope of a note through attack, decay,
  sustain and release, normalised to the sustain level and compared down to
  -20 dB, must be within 0.2 dB of reSIDfp on the 8580, and 0.5 dB on the
  6581.

The tests print the measured values. The M0 values are in
[decision record 0002](docs/decisions/0002-m0-sid-cost.md).

**A failure means** SIDOU moved away from reSIDfp on something both
should agree on. The 6581 tolerance is looser because M0 does not model the
6581's non-linear envelope DAC; M1 does and tightens it.

**Tolerances only get tighter.** Never loosen one to make a failure pass,
unless a decision record in `docs/decisions/` explains why.

### The filter against reSIDfp

`filter` checks the filter tables (`gen/filter.h`, below) at the small-signal
level they were fitted at (one voice of noise at envelope 0x44), where SIDOU's
linear filter should match reSIDfp's:
- **transfer functions**: both engines play noise through the filter and the
  same noise unfiltered, and the ratio of the two is the filter's response
  (`sidou_tablegen::filter_fit`). LP, BP and HP at FC 64 to 512 and
  resonance 0, 8 and 15, compared from half to twice reSIDfp's cutoff and
  below 7 kHz (above, SIDOU's trapezoidal filter departs from the analogue
  response whatever its tables say), each mode at its best level: the 8580
  within 1.0, 0.55 and 0.3 dB, the 6581 within 3.4, 3.75 and 3.5 dB;
- **passband gain**: from the same measurements, each mode's gain in its
  passband (LP f0/8 to f0/3, BP within 1.2x of f0, HP 3 f0 to 9 f0), SIDOU
  minus reSIDfp, held to the range measured at M1b Task 5: the 8580 within
  0.2 dB, the 6581 1.7 to 6.5 dB louder (a gap for Task 6);
- **sweeps**: `sidou-compare`'s low-, band- and high-pass noise sweeps at that
  level, as RMS envelopes, the largest window and the RMS over the windows:
  within 1.62, 2.03 and 1.70 dB (RMS 0.54, 0.80, 0.87) on the 8580 and 5.16,
  6.22 and 1.05 dB (RMS 1.19, 2.92, 0.46) on the 6581. These also measure what
  the tables cannot fix (the comment in `filter.rs` lists it);
- **full-scale sweeps**, record only: the same sweeps at sustain 15, where the
  6581's filter is level-dependent (the M1b plan's Task 6), held at the values
  Task 5 measured (rounded up to 0.01 dB) so that they cannot regress
  unnoticed.

### Register reads against reSIDfp

`register_reads` runs scripts of register writes on SIDOU and reSIDfp and reads
`$1C` (ENV3) or `$1B` (OSC3) from both at the same chip cycles, once per PAL
frame, or every 100 (ENV3) or 7 (OSC3) cycles over a shorter run. The reads
must be equal:
- **ENV3**: every attack, decay and release rate, gate changes in the middle
  of each phase, the ADSR delay bug, sustain changes during decay, and the
  power-on release from 0xaa;
- **OSC3**: triangle, sawtooth, pulse (three widths) and noise, and every
  combined waveform with and without noise, each at four frequencies; noise
  through TEST (short and long enough to fade), a TEST right after an LFSR
  clock, lock-up when combined with other waveforms, TEST releases into
  combined waveforms, switches between sawtooth combinations, and ring
  modulation. With no waveform selected, SIDOU reads 0 where reSIDfp holds
  the last value, and a test pins both.

Neither engine renders audio here: SIDOU advances with `sid_clock`, reSIDfp
with `clockSilent`, so a read can land on any cycle.

**A failure means** a register reads back differently from reSIDfp; the
message gives the cycle and the first differing values. Cases that a later
milestone task fixes are `#[ignore]`d with the task and the reason, and so are
two OSC3 gaps that stay (read-only and inaudible; the M1b plan's Task 7): the
reads right after a hard sync and a cycle after a pulse-width write. Run them
with `-- --ignored`.

### Generated tables

`core/sid/gen/` holds tables generated by `sidou-tablegen`: from reSIDfp, the
combined-waveform pulldown tables for each model and combined-waveform profile
(`gen/grid.h`) and the filter's cutoff and damping per curve setting
(`gen/filter.h`), and our own noise-register jump table (`gen/noise_jump.h`).
The filter tables are measured: `sidou-tablegen fit-filter` plays noise
through reSIDfp's filter (LP, BP and HP, FC every 32 steps and 1023 on the
6581, resonance 0, 8 and 15, and every resonance at six FC values mid-range),
fits SIDOU's filter to each response around its cutoff, and prints how well
it fits. Only
AVERAGE, reSIDfp's default, is compiled by default: 30,720 bytes per model,
61,440 for both. Building with
`SIDOU_ALL_COMBINED_PROFILES` (CMake option of the same name) adds WEAK and
STRONG: 184,320 bytes in all. The host tests build with all three.

The checked-in headers are the source of truth; the generator is a
documented tool. After changing the generator or the reSIDfp submodule,
regenerate them from `host/`:

```sh
cargo run --release -p sidou-tablegen               # every table
cargo run --release -p sidou-tablegen -- fit-filter  # gen/filter.h, with the fit report
```

The drift tests (`tests/drift.rs`) fail when the checked-in files differ
from what the generator produces:
- `checked_in_tables_match_the_generator`, in every job: the computed tables
  (`grid.h`, `noise_jump.h`, `wave_*.h`), exactly. If it fails on a new
  platform with nothing changed, that is a libm difference (reSIDfp builds
  the pulldown tables with `pow`, and some entries sit on a threshold), not
  stale tables: keep the checked-in headers.
- `checked_in_filter_tables_match_the_measurement`, `#[ignore]`d: it
  re-measures reSIDfp's filter (about 20 s on 8 cores), so CI runs it in the
  Linux host job only (`cargo test --release -p sidou-tablegen --test drift
  -- --ignored checked_in_filter_tables_match_the_measurement`). It compares
  `filter.h` up to one unit in the last printed digit (0.1 Hz of cutoff,
  0.0001 of k): the fit's own maths uses the `libm` crate, the same on every
  platform, but reSIDfp builds its filter model and resampler with the
  platform's libm, so a value can round the other way elsewhere. Run it after
  changing the fitter or the reSIDfp submodule.

### Filter comparison (`sidou-compare`)

`sidou-compare` renders low-, band- and high-pass noise sweeps on both
engines (`sidou_tests::filter_sweeps`), at the small-signal level the `filter`
tests assert and at full scale (`*_full`), writes each pair as 16-bit WAV
files into `<dir>`, which must exist (default: the current directory) and
prints the largest and the RMS envelope difference per scenario and model. It is for
listening and for tracking the filter's distance from reSIDfp between
milestones. The `sidou_compare_smoke` test only checks that it runs and
writes all 24 files.

### Host benchmark

A criterion benchmark of the shared workload for one and two chips, on both
models. Throughput is reported per chip (samples × chips), and
`workload_begin` is not timed. Reports go to `host/target/criterion/`.

It has no threshold. Use it to compare before and after a change on the
same machine; it says little about the FM-1, where the emulator gate is
the measure.

### Emulator gate

The M0 decision gate. It runs the pi32v2 benchmark programs
(`sid_bench_1` and `sid_bench_2`) in fm1-emu, used as a library, once per
workload scenario (`bench/workload.h`) on each model (6581 and 8580): the triad, plus envelope worst cases
at the fastest rate on all three voices (holding sustain 0xff and 0xaa, gates
toggled every frame or every 128-sample block, and an idle chip), a
combined-waveform scenario (pulse+saw, saw+triangle, and noise+pulse retriggered
through TEST), and noise combined with other waveforms on all three voices
(TEST held, steady until lock-up, retriggered through TEST every other frame
or every block, and from power-on at the highest and a low frequency, with
triangle, sawtooth, ring modulation and sync), and sawtooth and pulse+sawtooth
on all three voices at the top two pitches (0xffff and 0xe000), where the noise
register shifts most often without being heard (the core defers those shifts).
For each it:
- counts the instructions per output sample, and fails if one chip needs
  more than 900, or two chips more than 1,800 (design document, section 8),
  on average or in any 128-sample block (a block's window also holds the
  next block's register writes, which errs on the expensive side);
- renders the same scenario on the host build of the same C code, and fails
  if any sample differs by more than 1 LSB (at M0 they are identical);
- writes `sid_budget_1.json` and `sid_budget_2.json` into the fm1 build
  directory. The top-level fields are the triad's on the 6581, comparable
  with earlier milestones; `scenarios` lists every scenario and
  `max_instructions_per_sample` the most expensive, which must stay within
  `SID_WORST_PER_CHIP` per chip, and `max_worst_block_instructions_per_sample`
  the most expensive block, which must stay within `SID_WORST_BLOCK_ONE_CHIP`
  or `SID_WORST_BLOCK_TWO_CHIPS` (`tests/common/mod.rs`), what the effect
  gates assume.

`SIDOU_GATE_PROFILE=1` also counts the render instructions per function and
adds the top ones to each scenario's report (`profile_instructions_per_sample`),
the same for its most expensive block alone (`worst_block_profile_instructions`)
and every block's cost (`blocks_instructions_per_sample`). It slows the gate
down.

The effects scenarios (`fx_budget.rs`) run the same workload, then the
post-chip chain (`core/fx`) over its output:
- `sid_bench_fx_{1,2}`: every effect at full, settled;
- `sid_bench_fx_worst_{1,2}`: the worst case, where every target alternates
  on every block so the smoothing never settles (each effect recomputes its
  coefficients on every sample) and bit-crush captures on nearly every
  sample.

Each fails if the effects cost more than 500 instructions per sample, if two
chips at the SID's worst block and the LIVE view's tap plus the effects cost
more than 2,300, or if any sample differs from the host build at all. The
SID's worst block is `SID_WORST_BLOCK_TWO_CHIPS` in `tests/common/mod.rs`,
1,440 for two chips: the SID gate's most expensive 128-sample block (1,439.67
since M1b Task 6's 6581 filter), which the SID gate holds it to. The tap is
`TAP_PER_SAMPLE` there, 4.5: LIVE is the view at power-on, so its tap runs by
default, and `ram_audio.rs` holds it to that. The audio interrupt has a deadline per block, so a
block, not a scenario's average, is what the effects must fit beside. They
write `fx_budget_{1,2}.json` and
`fx_worst_budget_{1,2}.json`.

The effect-slot scenarios (`slots_budget.rs`) run `sid_bench_slots_1` and
`sid_bench_slots_2`: the workload, then 17 scenarios over 4,096 samples of
its output, all with every control alternating on every block:
- the slot frame alone (three slots, algorithm NONE);
- each algorithm alone;
- every slot's heaviest together.

They fail if:
- any algorithm costs more than its slot's budget in
  [decision record 0004](docs/decisions/0004-effect-algorithms.md)
  (Distortion 150, Modulation 200, FX 450, the frame 100);
- the heaviest together cost more than 900, or two chips at the SID's worst
  block and the tap plus them more than 2,300;
- the heaviest per slot is not the one the last scenario runs;
- any sample differs from the host build (including the switch scenario's).

Then a switch scenario, measured per 32-sample block: from every slot's heaviest to
fuzz, flanger and light and back, every slot switching (they take turns). In the
second change the Modulation slot wakes from rest during the FX switch. It fails if:
- a block costs more than 1,000;
- more than 80 blocks cost more than 900;
- the worst block plus what else exists today exceeds 2,700: two chips at the
  SID's worst block, the LIVE view's tap and 100 of interrupt overhead. The
  gate prints what is left, the headroom for the future 6502 (M3), as
  `headroom_for_6502_instructions_per_sample` (191 since M1b Task 6, the tap
  counted; 227 before Task 6, without it).

This is 0004's transient allowance. Its output is part of the host comparison.

They write `slots_budget_{1,2}.json` with every algorithm's cost and every switch
block's.

Build the `fm1` preset first (see [BUILDING.md](BUILDING.md)). Without
`SIDOU_FM1_BUILD`, the gate prints a message and passes without running,
which is why the CI job checks that the JSON files exist.

Instruction counts are a lower bound on cycles: the pi32v2 mostly issues one
instruction per cycle, but code fetched from flash (XIP) and divides take
longer. Noise combined with other waveforms leans on divides: each call of
`sid_osc_bulk_limit` makes a few (up to about 20 when it walks far), one
instruction each in the count but more cycles on the device.

The noise worst cases start from the power-on register (0x3fffff, nearly all
ones), the fullest state a workload reaches in its quarter second: the
all-ones register (0x7fffff) needs TEST held through the fade, 0.4 s on the
6581 and about 8 s on the 8580, and otherwise turns up about once in 8.4
million shifts. Only that state at the start of a block takes a block over
the budget (accepted in the M1b review).

fm1-emu at the pinned revision cannot execute one instruction that clang
emits (`{R*4} = [-R]`). The emulator tests step it themselves
(`crates/emu-gate/tests/common/mod.rs`); see [docs/TODO.md](docs/TODO.md).

**A failure means** one of three things: the core got too slow for the
budget, the target and host builds no longer compute the same thing (look
for float flags or undefined behaviour), or the gate could not run the
program (the message names the file or the faulting address).

### Boot test

`crates/emu-gate/tests/boot.rs` boots the firmware image
(`platform/fm1/sidou_app.bin`) in fm1-emu the way the FM-1's SPL starts it:
mapped at `0x02000120`, entered in user mode with `r0` pointing at a
boot-parameter block. The A4 key is held from power-on. It runs 900 ms of
emulated time and checks that:
- the audio interrupt delivers the frames of at least the 500 ms after the
  LCD's start-up delays;
- left and right are equal;
- the output is sample for sample what the host build of the same code renders
  for the same instrument (`live_init`, the MASTER pot, then the A4 note-on,
  which the test finds in one of the first 16 blocks);
- the note is not silent and its pitch is 440 Hz within 0.5 %.

It writes `sidou_boot.json` (frames, peak, measured pitch, the note-on's
block, instructions in and out of interrupts, the largest audio block) into the
fm1 build directory. Like the gate, it skips without `SIDOU_FM1_BUILD`.

### Live instrument test

`crates/emu-gate/tests/live.rs` plays the firmware through fm1-emu's front
panel (keys, encoders, the MASTER pot) and checks:
- a key plays its pitch (A4 and E4, within 0.5 %), and nothing plays before;
- OCT+ doubles the pitch and lights its LED; a held key lights its LED;
- turning K1 down by 40 detents cuts the share of the power above 2 kHz at
  least fourfold;
- MASTER turned down lowers the peak;
- the LCD after boot: the C64 blue at a fixed background pixel, the light blue
  border, text inside, no other colours; nothing is sent while nothing
  changes, and a held note goes out as one run of cells (one LCD window);
- the 6581/8580 toggle: `sid_init` runs in the main loop only (the test counts
  its instructions per call), no audio block gets near the block's
  instruction budget, no block is silent, and the FX LED lights; the device's
  `chips` state, read from emulated RAM through its ELF symbol, shows the
  active chip flipping on each FX press with the right spare model and no lost
  reset;
- the main loop stays under 5M instructions/s (about 2 % of the core) idle and
  with a key held.

### Screen test

`crates/emu-gate/tests/screen.rs` checks what the LCD shows and what drawing
it costs. It reaches each view through the panel, as a player would (HOME,
SELECT, ALGORITHM):
- **snapshots:** the status screen after boot, with a note held and with the
  toast OCT+ shows, MIDI learn's frame asking for a knob and for a CC and its
  LEARNED toast, the menu, the glyph sheet (every glyph, the palette
  and a pixel overlay), the boot's prompt, loading and title card, and About
  match the golden images in `tests/golden/lcd/`, pixel
  for pixel, and every pixel is one of the 16 palette colours. The title
  card (but for its version and PRESS ANY KEY rows) and About's top 64 pixel
  rows are also checked against the logo's source,
  `assets/logo/cracktro-chrome.png`, pixel for pixel. The status
  screen's version cells and the menu's cursor cell are left out; the cursor
  is checked to blink. Going back from the glyph sheet restores the menu and
  then the status screen exactly, and the toast goes after 1.5 s. The goldens
  are 4-bit indexed PNGs in the art palette;
- **transitions:** sampled every frame, a wipe shows the new view over the
  old one with at most a white edge row between them, the edge moving down
  (opening) or up (closing); a chip change sweeps ramp colours down the
  border in bands, and the border comes back;
- **cost:** the instructions `ui_poll` spends (its callees included) and the
  windows and SPI bytes `lcd.c` counts in each 40 ms frame: idle, for one
  note, the menu idle, a wipe each way and a raster. It prints them as JSON,
  with the SPI time at 12 MHz, and bounds them: nothing is sent idle, one
  note is one window under 1 ms of SPI, every frame of a transition stays
  under the 20 ms SPI budget, a wipe sends no more than a full screen and its
  edge rows, the raster sends every other frame, and no single pass of the
  main loop renders more than two runs. The full boot and a round of About
  are bounded in instructions a frame and in their busiest `ui_poll`, and so
  are the wipes into the title card and About, which reveal the logo's
  bitmap.

The host tests cover the same machinery without the device:
`tests/app/screen.rs` (the view stack, focus, routing, the capture, the
toast, the cursor blink, and the wipe and raster frame by frame against the
SPI budget), `grid.rs` (runs, overlays, held rows, and the pixel region:
pixel for pixel against the logo's PNG, text and overlays over it, which
cells it sends when, refusals, and random regions mixed with edits),
`logo_bitmap.rs` (the converter: its round trip, the drift test, the compiled
table against the PNG, the palette and size checks), `logo.rs` (each logo's
footprint, cost and overlays as declared, Cracktro Chrome's band and scene
against the PNG), `boot.rs` and `about.rs` (the sequences, the layouts with
either logo, the SPI budget a frame and under the wipes), `widget.rs`
(frames, numbers, bars, the menu), `views.rs`
(HOME and the menu, the panel playing as before in every view, the toasts) and
`learn_view.rs` (MIDI learn's frame, gesture, cancel, timeout and clearing);
`midi_learn.rs` has learn's state machine and its bindings through MIDI in and out.

**A failure means** the screen looks different or costs more. If you meant
the change (a new glyph, a moved label), look at the
`<name>.actual.png` the test writes into the fm1 build directory, then
rewrite the goldens and commit them with the change:

```sh
SIDOU_LCD_GOLDEN_UPDATE=1 SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate --test screen
```

### MIDI on the host

`crates/sidou-tests/tests/app/`:
- `midi_map.rs`: the CC/NRPN map (`app/midi_map.c`). Every parameter has one row, CCs
  and NRPNs are unique and keep clear of the numbers MIDI reserves, ranges and defaults
  match the instrument, each row round-trips through its CC and its NRPN. NRPN parsing:
  MSB then LSB, LSB-only updates, increment and decrement, the null NRPN, RPNs (RPN 0 the
  bend range), state per channel and per input, a small parameter that never passes
  through 0 on its way. `the_reference_is_up_to_date` fails when
  `docs/reference/midi.md` differs from the table; `SIDOU_MIDI_DOC_UPDATE=1` rewrites
  it.
- `expression.rs`: velocity, pitch bend (smooth across all 16384 values, the sync
  lead's two oscillators together), the wheel, aftertouch and the vibrato, at the
  register level.
- `midi_out.rs`: the outgoing USB-MIDI stream: packets, the rate limit, deduplication,
  no echo of MIDI input, and program changes for the panel's presets.
- `live_instrument.rs` and `reg_ring.rs`: the envelope mode. It is INSTANT at power-on,
  one write on a change, kept across presets, and sent after every chip reset before any
  register. Through the register ring it reaches the active chip, and the chip after a
  model change; a retrigger after a slow release starts at once with INSTANT, and waits
  for the ADSR delay with AUTHENTIC, and so does a note-off after a slower decay (SAW
  LEAD's, at each phase of the decay's period: 32 of 44 wait with AUTHENTIC).

### MIDI test

`crates/emu-gate/tests/midi.rs` plays the firmware through fm1-emu's TRS
MIDI jack (UART1, one byte per 320 us) and checks the audio and the
rendered chip's state, read from emulated RAM through the `chips` symbol:
- `90 3C 64` plays C4 (261.63 Hz within 0.5 %) on a gated voice at the
  frequency table's value; `80 3C 00` clears the gate, the note releases
  (still audible 20 ms later) and is silent 0.8 s on;
- CC 74 at 0 sets the cutoff to 0 and cuts the share of the power above
  2 kHz at least fourfold; at 127 it sets 2047;
- program change 1 switches to PWM BASS (pulse wave, cutoff 700), which
  plays C4 at its pitch, and program change 0 back to SAW LEAD;
- 3000 bytes at full speed arrive with nothing lost (`midi_stats` in RAM:
  entries, losses, parser restarts, the longest wait in the ring under
  2 ms), and USB was never polled with the controller up;
- the cost: the main loop stays under 5M instructions/s idle and with the
  jack at full speed, and full-speed TRS MIDI adds under 2M interrupt
  instructions/s. This test reads no firmware symbol, so it also runs on a
  build from before MIDI input, for the difference;
- an NRPN sets the cutoff at full resolution; a pitch bend shifts the measured
  pitch, and the sync lead keeps its ratio while it bends;
- floods of chip-model CCs, pitch bends and program changes neither stall the main
  loop nor lose input;
- MIDI out: the panel's changes go out as USB-MIDI packets, MIDI input is not echoed,
  and a tick of MIDI out costs little at its worst, with every parameter learned too;
- MIDI learn: SAVE pressed, K1 turned and CC 14 (a CC the map leaves free) sent over TRS
  bind CC 14 to the cutoff (`learned` and `instrument` read from RAM), which then follows
  CC 14 on any channel while CC 74 still works; HOME cancels a second learn and SAVE three
  times clears; a flood of the learned CC at the jack's full speed costs the main loop no
  more than a flood of a map CC, and both, like a flood of CC 64, stay under 5M
  instructions/s. The cost tests keep the jack busy for their whole window
  (`Machine::midi_paced`: fm1-emu drops a backlog past 3125 bytes) and check that nothing
  was dropped.

### Latency test

`crates/emu-gate/tests/latency.rs` injects a note-on into the emulated TRS jack at a
random cycle, each trial from a fresh boot past the title card and its wipe, so the
instrument is idle as a player finds it. It measures, from the note-on's last byte: when
the audio interrupt takes it, when its first non-silent sample is in the DMA buffer, when
that sample leaves the DAC, and when the note's envelope comes up (-30 dB of the held
note's peak). Then, once the note holds its sustain level, a note-off: when the audio
interrupt takes it and when the release's first step (the voice's envelope counter, read
from the rendered chip) is in the buffer. The cases and their trials:
- the status screen idle: 64 trials with the envelope INSTANT (the default), 16 with
  AUTHENTIC (CC 26 = 127 after the boot, checked on the rendered chip's gate reset);
- the screen redrawing (K1-K3 turning): 64, INSTANT;
- the note-on around the USB retry's attach: 32, INSTANT.

It asserts for every trial:
- taken within one block and one poll (2.9 + 0.5 ms), plus the USB attach's stall when
  the note lands around it;
- in the buffer within that, plus the render allowance: the run's longest audio
  interrupt, measured and held to the audio budget's block (2700 instructions a sample).
  This follows from the bound above, as the taking interrupt is one of the run's; it is
  kept as the plan's statement;
- audible within two blocks of being taken;
- INSTANT: the envelope is up in the block that takes the note; AUTHENTIC: within the
  ADSR delay's longest (32767 cycles) and the attack;
- the note-off taken within the note-on's bound; INSTANT: the release's first step in the
  block that takes it; AUTHENTIC: within the ADSR delay's longest.

It prints a JSON report per case (the distributions as min, median, mean and max) and
writes it into the fm1 build directory as `latency_<case>_<envelope>.json`.
`SIDOU_LATENCY_TRIALS=<n>` runs every case with n trials; 64 reproduces the run
[decision record 0005](docs/decisions/0005-midi-note-timing.md) has the numbers of:

```sh
SIDOU_LATENCY_TRIALS=64 SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate --test latency -- --nocapture --test-threads=1
```

fm1-emu does not emulate USB (see [docs/TODO.md](docs/TODO.md)), so USB
MIDI is covered by the host tests of the packet conversion
(`tests/app/midi_usb.rs`), the ring (`midi_ring.rs`) and the input logic
(`midi_rx.rs`), and by hardware testing.

### Package tests

`crates/sidou-pack/tests/`:
- `ota_step1.rs` runs the firmware's step 1 of the update protocol
  (Felucca's `ota.c`, built for the host with test hooks) against packages
  from `sidou-pack`, the test acting as the installer (`fm1_install.py`'s
  side), on a simulated NOR flash: the identity request is answered with
  `FM-1_900`; a package's loader is staged at `0xE0000` with its head, the
  update record is written and committed after the host's "success", and
  nothing else is written; another board's package (flash.bin of another
  size) and a loader without the marker are refused before any erase; the
  next boot clears the staging area. It needs no downloads (a synthetic
  device head carries the chip key). With M-VAVE's V15 package in
  `build/stock/` (SHA-256 checked; read by the tests only, never packed), a
  device with V15's head takes SIDOU's package without its head being
  written, and the way back works: V15's own package passes `ota.c`'s
  head-identity check and is staged, while a device whose head had been
  changed would refuse it before any erase. fm1-emu has no USB, so this is
  where the protocol and the package format meet before hardware. With the
  fm1 build there, the loader it built (Felucca's) is staged too.
- `flash_hooks.rs` runs the firmware's flash hooks for the update protocol
  (`platform/fm1/ota_flash.h`) with stand-ins for the flash driver, the
  interrupt mask, the audio and the LEDs: erases and writes only inside the
  staging area (misaligned, outside or wrapping requests get -8), never
  while the audio runs (-16, counted), the driver always called with
  interrupts masked and the LED rows dark for an erase, reads in 256-byte
  windows; and that the harness's copy of `FL_IN` is the HAL's.
- `pack.rs` reads a package back with fm1-emu's reader (`fm1_soc::fwsc`): the
  head is the SDK-default one with the SDK's SPL and the chip key, the app
  is at `0x02000120` in the unscrambled app area, the directory, the
  container and the loader's image match; and a golden SHA-256 of a fixed
  tiny package, which Felucca's `fm1pkg_make.py` reproduced byte for byte
  once.

`crates/emu-gate/tests/fx_live.rs` plays the effect slots through the firmware:
- the panel's EDIT layer turns OVERDRIVE on: the wave clips (its crest factor
  falls) and EDIT lights up;
- TRS MIDI CCs turn the light reverb on: the output turns stereo and a tail
  follows the note;
- every slot's heaviest (overdrive, chorus, lush) switched in by CC while a
  chord plays: every audio block, switching included, stays within the
  2,700-per-sample audio target; the interrupt runs only from `.ram_text`;
  the main loop stays under 5M instructions/s.

`boot.rs`'s host reference goes through the same slots (all off at boot), and
compares both channels sample for sample.

`crates/emu-gate/tests/package.rs` boots `sidou.fwsc` as fm1-emu boots a
package (`stock_from_package` + `stock_boot`) with the A4 key held: it plays
440 Hz, sample for sample as the bare image does. It writes the audio as
`sidou_package.wav` into the fm1 build directory. A `sidou.fwsc` older than the
build's `sidou_app.bin` fails it, with a message to run `sidou-pack` again: it
holds an earlier build.

The hardware checklist, for a real FM-1, is
[docs/hardware-test.md](docs/hardware-test.md).

### Boot-guard test

`crates/emu-gate/tests/bootguard.rs` checks the boot-loop guard
(`app/bootguard.c`, `boot_guard` in `platform/fm1/main.c`). fm1-emu does
not model a reset, so each reset is a fresh machine that keeps the guard's
`.noinit` words, and a boot dies by being cut off after 600 ms:
- after two such boots the third enters UBOOT: the boot-state mailbox holds
  `usb_update_mode`, the guard is cleared, and the audio never starts;
- a boot that runs past `BOOTGUARD_STABLE_MS` (10 s) clears an earlier
  death.

The guard's rules are host-tested too (`tests/app/bootguard.rs`).

### SRAM audio test

Checks of where the audio path runs, in
`crates/emu-gate/tests/ram_audio.rs`:
- the boot copies `.ram_text` (the audio path, `platform/fm1/app.ld`) from
  the image to SRAM: emulated RAM at `_rt_start.._rt_end` equals the image
  at `_rt_load`;
- with a key held, every instruction the ALNK0 interrupt runs at its own
  level (interrupts nested in it aside) lies in `.ram_text`; a failure names
  the functions that ran from flash;
- no direct `call` or `goto` crosses between flash and SRAM code, which
  would be at the edge of the pi32v2's branch reach: the firmware links with
  `--emit-relocs`, and every branch relocation's site and target must lie in
  the same region ([docs/hardware.md](docs/hardware.md), "Code in SRAM").
  A function the audio path calls but `app.ld` leaves in flash fails the
  link before any test runs;
- the audio path reads no data from flash: no address relocation in
  `.ram_text` (`MOV_ABS32`, `ABS32`) points into XIP. A constant table that
  audio code reads must be in `.data` on the device; a failure names the
  function and the table. Data reached through a pointer set up elsewhere
  (the combined waveforms' pulldown rows, in flash by design) is not seen.

fm1-emu does not model the I-cache, so the benefit is a hardware
measurement (docs/hardware.md, open checks).

## Sanitizers

The C and C++ code can be built with sanitizers through `SIDOU_SANITIZE`:

```sh
# UBSan in trap mode: no runtime needed, works on stable Rust and macOS.
SIDOU_SANITIZE=undefined cargo test --workspace

# ASan: needs nightly Rust, which provides the ASan runtime. CI runs it on Linux.
SIDOU_SANITIZE=address RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly test --workspace -Zbuild-std --target x86_64-unknown-linux-gnu
```

Under UBSan, undefined behaviour stops the test with a trap (SIGILL or
SIGTRAP) rather than a report. Re-run the failing test in a debugger to find the
line.

## Continuous integration

`.github/workflows/ci.yml` runs on every push to `main` and every pull
request:

| Job | What it runs |
|---|---|
| `host` (Ubuntu and macOS) | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --release`, and the tests again with `SIDOU_SANITIZE=undefined`; on Ubuntu also the filter tables' drift test (ignored elsewhere) |
| `asan` (Ubuntu, nightly Rust) | the tests with `SIDOU_SANITIZE=address` |
| `msrv` (Ubuntu, Rust 1.87) | `cargo check` |
| `reuse` | `reuse lint` |
| `fm1` (Ubuntu) | the `fm1` build in Docker; `sidou-pack` makes `sidou.fwsc` (its inputs cached by `inputs.toml`'s hash) and its tests run with the inputs and with M-VAVE's V15 package (fetched and SHA-checked for the stock-return tests only); the emulator tests (gate, boot, live, MIDI, latency, SRAM, boot guard, package boot); a check that the JSON reports exist. The reports are uploaded as the `sid-budget` artifact, the package with `LICENSES/Apache-2.0.txt` and `LICENSES/GPL-3.0-only.txt` as `sidou-fwsc` |

Before opening a pull request, run at least:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --release
```

and `reuse lint` (or `python3 -m reuse lint`) from the repository root.
