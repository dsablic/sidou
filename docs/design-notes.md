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
- Cold code builds at `-Os`/`-Oz`; files on instruction-budget paths keep `-O2`.
- The voice shadow (`core/sid/sid_shadow.c`, the .sid player's main-loop copy of the voices)
  runs from flash but calls the chip's own out-of-line voice helpers (`sid_osc_run_combined`,
  `sid_osc_advance_slow`, `sid_noise_clock`, `sid_noise_settle_deferred`, `sid_env_run`,
  `sid_env_advance_slow`) in SRAM through the `*_far` pointers `sid.c` exports
  (`SID_VOICE_FAR`): a direct call cannot reach SRAM from flash (docs/hardware.md, "Call range").
  It saved 7.3 KB of flash copies and leaves `sid.c`'s SRAM code byte for byte as it was. The
  shadow still reads its envelope tables from flash (`SID_ENV_TABLES_IN_FLASH`), and builds
  with `-fno-function-sections` so that none of the helpers it still instantiates is taken
  into SRAM by app.ld's name patterns.

### Toolchain and sections

- The pi32v2 toolchain is JieLi's clang 4.0.1 with a separate `ld` (`cmake/pi32v2.cmake`); it is
  run only through `tools/fm1-docker`. clang searches only its own resource headers, so newlib's
  headers beside it are added as standard include directories.
- `-mcpu=r3` turns on the single-precision FPU (the default CPU calls soft-float helpers, which
  SIDOU does not link); `-mfprev1` lets the compiler dual-issue FPU ops (fm1-emu's
  `docs/isa-notes.md`). Release is `-O2`, set as a cache entry because CMake would otherwise
  append its Clang default `-O3` to an `_INIT` value.
- The firmware builds with `-ffunction-sections -fdata-sections`: one section per function lets
  `app.ld` place single functions (the audio path in SRAM) with input-section rules instead of
  attributes in the portable C, and one per object lets `--gc-sections` drop constants nothing
  reads (retired art, host-only tables).
- The link uses `--gc-sections` (an unused call between flash and SRAM code would otherwise
  have to link, besides the size), `--orphan-handling=error`, and `--emit-relocs`, which keeps
  the relocations in the ELF (not the image) for emu-gate `ram_audio.rs`'s check that no branch
  crosses between flash and SRAM.
- JieLi's `objcopy` takes one `-j` per run, so `sidou_app.bin` is `.entry`, `.ram_text`, `.text`
  and `.data` extracted one by one and concatenated, the order `app.ld` loads them from XIP and
  `fm1_cstart` copies `.ram_text` and `.data` from.
- `-ffp-contract=off` on `sidou_core`: float results must be identical on the host and the
  pi32v2 so their outputs can be compared bit for bit.
- The vendored HAL is a `SYSTEM` include, so its headers' unused static helpers do not warn in
  the files that include them; `fm1_hal_check` includes it directly with `-Wall -Werror
  -Wno-unused-function`, because warnings in SYSTEM headers are hidden and that check is about
  them.

### Per-file optimisation

- A file builds at `-Os`/`-Oz` only while every emulator budget gate that bounds it holds with
  room; the files that stay at `-O2` were each measured.
- `core/player/gt2_play.c` (`-Os`) and `song_ctl.c` (`-Oz`) run once a frame, not once a sample:
  size keeps their SRAM small. `song_ctl.c` at `-Oz` is 112 bytes less than `-Os`, which RAMTEXT
  needs to stay below Felucca's debug area. The player at `-Oz` would save 336 bytes but puts a
  fast-forward's slice over `SONG_SLICE_INSTRUCTIONS` (emu-gate `song.rs`).
- No jump tables (`-fno-jump-tables`) in code the audio interrupt runs from SRAM: a switch's
  table would be read from flash (`.rodata`). That holds for `gt2_play.c`, `song_ctl.c`,
  `gt2_song.c` (its `gt2_edit_apply` is in `.ram_text`), `song_ext.c` and `psid_ctl_isr.c`.
  `cpu6502.c` keeps its jump table: the 6502 never runs in the interrupt.
- Main-loop and boot work at `-Oz`: `gt2_stream.c` (240 bytes), `song_ext.c` (416), `psid_file.c`
  (1,344), `psid_lift.c`. At `-Os`: `gt2_ins.c`, `gt2_sng.c` (the boot reads its song in the
  LCD's reset wait), `psid_env.c`, `psid_ctl.c` (its main half; its interrupt half,
  `psid_ctl_isr.c`, is `-O2` as the render's callers are).
- `gt2_shape.c` is `-Os -fno-inline-functions`: bounded main-loop analysis; the size build costs
  more per analysis unit, which `SONG_SLICE_UNITS` (`song_ctl.h`) accounts for.
- `cpu6502.c` is `-Os` with `CPU6502_SMALL` (the I/O page's accesses out of line): about 7 % more
  instructions per 6502 cycle than `-O2` with them inline, and about 2,900 bytes smaller.
- `sid_shadow.c` is `-Os -fno-function-sections`: in one section, none of the voice code it
  instantiates from `sid_osc.h` and `sid_env.h` has a name `app.ld`'s `.ram_text` patterns take
  into SRAM.
- `platform/fm1/main.c` is `-Os -mllvm -inline-threshold=225` (-O2's inlining threshold): plain
  `-Os` saved 4,256 bytes but the main loop ran 8.5 % more instructions in a CC flood (5.009M/s
  against `midi.rs`'s 5.0M bound); with the threshold it saves 2,208 bytes at +4.2 % (4.81M/s;
  LIVE's full-speed flood 6.05M/s of 6.3M). `fm1_alnk0_irq`, main.c's one SRAM function, comes
  out byte for byte the same.
- `midi.c` builds with Felucca's flags (`-mcpu=generic -mno-fprev1 -Os`, plus
  `-fdata-sections`), `SIDOU_USB_FIX`: see `platform/fm1/VENDORED.md`, "How `midi.c` is built".
- In `app/`: the store (`store.c`) at `-Oz` (about a third smaller than `-O2` at `-Os`, another
  1,562 bytes at `-Oz`): its polls are bounded by work (`STORE_POLL_WORK`), not time. Settings,
  profiles, song and tune loading, SysEx, routing, the cold view code, LAB, Live Link and
  SCENE's pieces are size-built within their gates (POLL_INSTRUCTIONS, the scene budgets).
  `song_pages.c`, `step_view.c` and `song_gen.c` also take `-fno-inline-functions
  -fno-jump-tables`, so their rendering and edit helpers stay outlined.
- Measured and left at `-O2`: `live.c` (`ui_encoder` over `CHIP_PICK_INPUT` 400, K1 over
  `POPUP_INPUT` 250), `title_card.c` (THE CHIP's worst `ui_poll` 79,349 against 79,000),
  `views.c` at `-Oz` (880 bytes, but K1's popup input reaches 250 of 250; it stays `-Os`),
  `chip_view.c` at `-Oz` (240 bytes; CHIP K1 on DIST reaches 245 of `CHIP_INPUT` 245),
  `grid.c` (3,360 bytes, but the tile renderer's frame grows 15 %: THE CHIP's `ui_poll` 79,774
  over 79,000), `live_view.c` (the instrument's frame, nearest its noise-chord gate) and
  `reg_ring.c` (the audio interrupt's pops). `boot.c`, `sid_character.c` and `sid_dac.c` stay
  at `-O2`: at `-Os` they moved the thump stage over its 31 instructions a sample, the boot
  timeline (LIVE chip B's golden) and a tune's pause.

### `gt2_song.c` at `-Oz`: timing only

- At `-Oz`, `gt2_song.c` is 384 bytes smaller and four LCD goldens move (`live-c64`,
  `live-keys-b`, `scene-rule30`, `scene-stars`). The cause is timing, not a miscompile or
  undefined behaviour. Measured in fm1-emu on `live_shows_the_keys_on_chip_b`'s run (a scratch
  probe, 2026-10-31): both builds reach LIVE at the same millisecond with bit-identical audio up
  to the key press. `-Oz` outlines more helpers (`end_row`, `empty_row`, `order_slot` ...), the
  boot's main loop runs about 39,000 more instructions, and the main loop's phase at the key
  differs: the note lands one 128-frame audio block earlier, so the scope captures another
  phase of the waveform and the level meters differ by a few pixels. Pressing the key 0.1 to
  1.9 ms later on the `-Oz` build gives audio bit-identical to the `-Os` build's from boot to
  the end, and the framebuffer identical to the golden. Of SCENE's six piece goldens at `-Oz`,
  fire, plasma, ant and RED MOON RITE pass; rule30's lower band and stars' field differ.
- The goldens capture a fixed moment after a key press, so any change in the main loop's
  instruction count before the press can move them; they are not a check of the song code.
  `gt2_song.c` stays at `-Os` all the same: at `-Oz` four goldens would have to be regenerated
  for 384 bytes, the owner's call.
- The song code is clean under UBSan: sidou-tests' 419 song tests pass with
  `SIDOU_SANITIZE=undefined` (clang, trapping) with the C at the host's default, at `-Os` and at
  `-Oz`; with GCC 13 (`CC=gcc`, the build image) at `-O2` and `-Oz`, the `song`, `song_capacity`,
  `song_locks`, `song_ctl` and `song_stream` binaries pass (104 tests; `song_snapshot` compiles
  its own C, which does not link against GCC's sanitized library).

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

## SID emulation

### Instruction budgets and SRAM placement

- `sid.c`, `sid_osc.h` and `sid_env.h` run in the audio interrupt from SRAM. `sid_clock`'s step is
  16 cycles: no longer than `sid_render`'s step at any PAL rate up to 61.5 kHz, so `sid_clock` is
  never coarser than rendering. The loop's cost does not matter: `sid_clock` is for tests and
  register-accurate seeking, not the audio path.
- Deferred noise shifts repeat with the register's period (2^23 - 1): fold the count before it can
  wrap (after about 19 hours at the top pitch). A call adds at most one rise per 16 cycles, well
  inside the 2^31 left.
- The power-up envelope: each counter at 0xaa, releasing at rate 0 to 0 (the first step on the third
  cycle, then every 9 cycles, through the exponential divider: about 6 ms), now cycle-exact with
  reSIDfp.

### Output levels and headroom

- `OUTPUT_GAIN` leaves headroom for the 6581 DC offset and filter peaks. reSIDfp's output level
  relative to `OUTPUT_GAIN`: the RMS of one unfiltered voice at volume 15 (sawtooth, triangle and
  50% pulse), measured through its mixer and output stages: 3.37 dB louder on the 6581, 1.05 dB on
  the 8580. At this level three voices at maximum resonance clip the 8580 by up to about 1.1 dB, as
  they clip reSIDfp; the 6581 keeps 0.5 dB of headroom.
- The thump at a note's start: the peak a DC rising linearly over the attack (2 ms to 8 s) reaches
  through the 16 Hz DC blocker, tau / T (1 - e^-T/tau), so a slow attack hardly thumps. The voice
  DC's rise is the louder transient (at thump 1, two voice DAC units for a fast attack's note from
  silence). Neither needs headroom in the chip: the output stage plays both after the effect slots.

### 6581 filter and output path

- Fitted to reSIDfp's 6581 at 44.1 kHz: filter sweeps at small and full scale, cutoffs and damping
  measured from reSIDfp.
- The non-linearity at amount 1: NL_LEVEL 0.055f (g per unit |hp|), NL_LEVEL_REF 0.13875f, NL_LOAD
  0.13125f, NL_LOAD_REF 1.7375f, NL_FLOOR_HZ 240.7f.
- reSIDfp's 6581 filter outputs against its unfiltered voices: the low-pass and high-pass inverted
  (inverting op-amps), the band-pass not; each mode's passband gain (MIX_GAIN_LP 0.6875f,
  MIX_GAIN_BP 0.8125f, MIX_GAIN_HP 0.5f); and its op-amps' finite gain A, which makes the
  integrators leak: the band-pass carries LEAK (1 / A, 1.0f / 3.375f) of the low-pass, the high-pass
  2 LEAK of the band-pass and LEAK^2 of the low-pass.
- The DC each input routed into the filter brings at the operating point: DC_FILTER_INPUT 1.15625f.
  It moves the cutoff only (it enters the load term, not the signal: as a step through a resonant
  low-pass it would ring past full scale where reSIDfp's saturating op-amps do not).
- The external filter's low-pass with reSIDfp's resampling: one pole, fitted to reSIDfp's output at
  44.1 kHz (OUT_LP_HZ 13154.0f), not the external filter's 15.9 kHz nominal pole, which leaves out
  the resampler's roll-off.

### exp_neg approximation

- exp(-x) for x >= 0, without libm: (Taylor series of exp(-x / 16))^16. For `sid_init`'s output
  pole, x = 2 pi OUT_LP_HZ / fs: within 5e-6 relative from 22.05 kHz up, 3e-5 at 16 kHz, 0.3% at 8
  kHz, and 27% low at `sid_init`'s lowest rate, 4 kHz, where exp(-x) is 1e-9. `out_c = 1 - exp_neg`
  is within 5 ULP of exact at every rate `sid_init` accepts.

### Oscillator

- Noise tables the audio path reads: built once before anything runs instead of loaded from the
  image. On the device the boot calls `sid_noise_tables_init` in the LCD's reset wait; on the host
  it is a constructor. On the device they lie in POOL (SRAM), not in `.data`'s copy in flash and in
  the RAM region.
- `SID_NOISE_JUMP[k][i]` is the register after 2^k plain shifts of bit i alone: row 0 is one shift,
  each further row the one before applied to itself. `SID_NOISE_WB_BYTE[h]` is `SID_NOISE_WB`'s two
  halves for the output's bits 4..11 at once.
- Each compiled row's index row in `SID_PD`: `sid_osc_update_mode` reads it in the audio path, so
  RAM on the device.
- The waveform output of the last cycle: what the voice's DAC sees, and OSC3 on the 6581. `src_acc`
  is the ring source's accumulator as this voice's output sees it (after the source's own MSB pull
  for voices 2 and 3, before it for voice 1: reSIDfp computes the outputs in voice order).

### Envelope

- The rate counter is a 15-bit LFSR that starts at 0x7fff and shifts once a cycle until it equals
  the rate's table value; the next cycle reloads it. Its period is 32767, so the LFSR is kept as a
  position, the number of shifts since 0x7fff, and each table value as its position (period - 1). A
  rate lowered below the position has to wait for the LFSR to come round (the ADSR delay bug).
- After the reload come pipelines of one to four cycles: the exponential divider, then the counter
  step (two cycles after the reload in attack or at divider period 1, three otherwise). Gate changes
  go through a two- or three-cycle state pipeline whose first cycle briefly selects the decay rate.
  ENV3 is the counter latched at the start of the cycle.
- `sid_env_advance` is cheap at rest: between matches (`idle`), in a steady state where nothing
  visible changes (`dormant`: frozen at 0, or holding sustain) and in a run of ordinary steps
  (`linear`). Waking from dormant costs a division, at most once per write.
- Gate reset (not the chip's): each gate edge, rising or falling, also restarts the rate LFSR from
  0x7fff, so the attack's or the release's first step comes one rate period and 3 cycles after the
  edge (12 cycles at attack 0) instead of after the ADSR delay. Below a sustain level of 0x5d the
  release's first step still waits for the exponential divider. Off by default (reSIDfp's
  behaviour).

### Filter

- On silence the state decays into subnormals, which may be slow or trap on the pi32v2 FPU. Flush
  explicitly rather than relying on an FPU flush-to-zero mode.
- The 6581 filter's non-linearity, a phenomenological fit to reSIDfp's 6581, whose integrators are
  MOSFET resistors (VCRs) and op-amps working around an operating point: in the threshold region (FC
  up to about 600) the cutoff rises with the signal's swing, nearly doubling at full scale; the
  filter's DC load lowers the cutoff everywhere and widens it, to half the cutoff and twice the
  damping with three loud voices, whilst the band-pass keeps its peak level.
- The step's error is the load-damping model, not an approximation of it: the step underestimates
  a1, more the further the load pulls the cutoff down, which raises the damping the filter runs at
  above the table's k as reSIDfp's damping grows under the load, and keeps it stable. The clamp t >=
  0.1 holds a1 at 0.1 a10. `SID_NL_M_MIN` bounds how far the load goes, as reSIDfp's saturating
  op-amps bound its output.
- The 6581's tick sets bpn: the band-pass times the damping the shifted coefficients have (over the
  table's damping: the mix divides by k), which keeps the band-pass peak where reSIDfp's stays as
  the load widens the band.
- The clamps as fmax and fmin, which the pi32v2's FPU has as instructions: the same values as
  comparisons but for a tie of +0 and -0, which returns either. In the emulator gate they make the
  worst blocks 5.7 instructions a sample cheaper with one chip, 12.2 with two, and the default
  render 1.0 dearer; a fabs for the swing made it 17 dearer.
- Filter tables delta-coded in blocks of 32. `sid_cutoff_seq` reader keeps its place and the next FC
  value costs one step. A code's cutoff in Hz: float bits, the code's 16 bits above an 11-bit
  mantissa tail, the exponent rebased by `SID_CUTOFF_CODE_BIAS`.

### DAC

- Voice DAC model: a port of reSIDfp's Dac to float. The R-2R ladder's 2R/R ratio, the "kink": the
  6581's is non-linear (SID_DAC_KINK_6581 2.20f), the 8580's not (SID_DAC_KINK_8580 2.00f). MOSFET
  leakage: an off bit still adds this fraction of its weight (SID_DAC_LEAKAGE_6581 0.0075f,
  SID_DAC_LEAKAGE_8580 0.0035f).
- The waveform DAC input reSIDfp takes as the voice's zero: ZERO_6581 0x380u, ZERO_8580 0x800u. The
  6581's voice DC that follows the envelope DAC (0.2143 V per unit), over its voice voltage range
  (1.5 V): VOICE_DC_6581 (0.2143f / 1.5f). The 8580's is constant, and the output's DC blocker
  removes it.
- Bit mismatch pattern: at mismatch m, bit b's weight is scaled by 1 + m * SID_DAC_MISMATCH_WAVE[b]
  (the waveform DAC) or SID_DAC_MISMATCH_ENV[b] (the envelope DAC), each between -1 and 1. One fixed
  pattern, as one chip's resistors would have: alternating signs, so codes on either side of a carry
  land too high and too low; on the envelope DAC the low bits err the most, so a quiet voice's level
  steps unevenly as its envelope moves whilst a loud one's barely shifts. At 0 the ladders are
  reSIDfp's exactly.
- `sid_dac_build` is about 13k float operations (94k instructions on the pi32v2). Split into
  SID_DAC_BUILD_STEPS calls of about equal cost.

### Character

- Controls, each 0..127: age/heat: the filter's cutoff and damping drift towards a darker curve and
  the 6581's non-linearity rises to twice the profile's (clamped to 2); DAC sloppiness: the voice
  DACs' 2R/R ratio up, their leakage up, and their bits' weights off; output thump: a DC step at
  each note's start and at each volume write; filter bleed: the voice routing's switches leak: up to
  SID_CHARACTER_BLEED of the voices routed into the filter also reaches the output unfiltered.
- DAC, thump and bleed follow the control on a perceptual curve: at 127 the full amount, at 64 about
  29 % of it, the first detents a few %; age is linear.
- Changes are ramped, so a jump does not click: every 10 ms tick the age and the thump move
  SID_CHARACTER_RAMP towards what was asked (0 to 127 in 32 ticks), the bleed
  SID_CHARACTER_BLEED_RAMP (in 127), and the DACs SID_CHARACTER_DAC_RAMP a table, each table built
  over SID_DAC_BUILD_STEPS ticks (0 to 127 in 172). Each step is as large as the click test allows.
- The DACs' grit as 0..1 for the control's 0..1: its square root (Newton's from above, converged to
  a float's precision for every detent), so the grit's level rises fastest at the first detents,
  where a little of it is already heard, and keeps rising to the top.
- Bilinear curve/range interpolation adds work to each cutoff; use smaller pieces (EXPERT_ROW_G
  128u, EXPERT_ROW_X 192u) than the profile-only preparer.
- The profile's plain word, for the screens: BRIGHT (6581 R2), DARK (R3), CLASSIC (R4AR, reSIDfp's
  default), CLEAN (8580 R5), GRITTY (worn 6581).

### Voice shadow

- The voice shadow: a copy of a chip's three voices that the main loop clocks with a tune's 6502, so
  that the tune's reads of the SID are real. The 6502 runs up to two frames ahead of the audio chip
  (the look-ahead), which therefore cannot answer them; the shadow sees the same writes at the same
  cycles and answers at the read's cycle. The shadow has no filter and no output stage.
- The voices run `sid.c`'s own code, in steps of at most SID_SHADOW_STEP 128u cycles, so with the
  same writes at the same cycles they reach the chip's state exactly, however each side splits its
  cycles, except with hard sync: a synced voice's own noise clocks and MSB rise earlier in a step
  are not recomputed, so with sync on the shadow and the chip can differ in what the synced voice
  did before the restart.
- reSIDfp's bus value lifetimes, in chip cycles: SID_SHADOW_TTL_6581 0x1d00u, SID_SHADOW_TTL_8580
  0xa2000u. Each write and each read of $19-$1C leaves its value with a fresh time to live; a read
  of any other register halves what is left of it; once it runs out, the bus reads 0. A time to live
  halved to 0 (from 1) never runs out until the next write or read of $19-$1C.
- Whilst a voice has SYNC set it steps at most SID_SHADOW_SYNC_STEP 16u, `sid_clock`'s step, never
  coarser than the chip's own render at any PAL rate up to 61.5 kHz: hard sync's stepping difference
  is then no larger than the chip's own against reSIDfp.
- Main context only, from flash on the device. 11,382 bytes of code (`-Os`: a second instance of the
  voices' code, the chip's being in SRAM) and 64 of tables; the struct is 704 bytes on the device.
  The cost, measured on fm1-emu: about 45,000 instructions a PAL frame for a GoatTracker tune (under
  1 % of the core), and up to about 26 an idle cycle whilst every voice runs a combined waveform
  with noise cycle by cycle.

### Voice template

- OSC3 as reSIDfp latches it. On the 6581 that is the waveform output of the last cycle. On the 8580
  the triangle and sawtooth reach it a cycle late: it combines the previous cycle's triangle/saw
  with the pulse and noise of the last one (whose pulse comparison is itself a cycle late on both
  models).
- A read right after a write to voice 2 or 3, before any cycle, returns the value latched before the
  write. A read a cycle or two later is wrong after: a frequency or pulse width write; a control
  write; or a hard sync of voice 3 or voice 2 (with ring modulation).
- With no waveform selected OSC3 reads 0. reSIDfp instead holds the last output on its floating DAC
  input and fades it; SIDOU keeps 0 because programmes rarely read OSC3 with the waveform off, and
  holding the value would cost state on the audio path.

### Waveforms and noise

- The oscillator follows reSIDfp's WaveformGenerator closely enough that OSC3 reads the same values
  at the same cycles, for every waveform. Combined waveforms: the AND of the selected waveforms goes
  through a pulldown table generated from reSIDfp, one per combined-waveform profile (reSIDfp's
  AVERAGE by default). On the 6581, a combination with sawtooth whose output has its top bit low
  pulls the accumulator's MSB low too.
- Noise: a 23-bit LFSR, kept in reSIDfp's bit order. It shifts in two phases, one and two cycles
  after accumulator bit 19 rises. Combined with other waveforms, the output pulls the register's
  output bits down every cycle (write-back), which can lock it at 0; TEST does not clear it but lets
  it fade to all ones, after a delay that depends on the model.
- Fade delays for the register's fade whilst TEST is set (6581R3, 8580R5): SID_NOISE_RESET_6581
  50000u, SID_NOISE_FADE_6581 15000u, SID_NOISE_RESET_8580 986000u, SID_NOISE_FADE_8580 314300u.
- `SID_NOISE_WB`: the register bits an output pulls down. On the device in `.data` (RAM), not with
  the other constant tables in flash: the audio path reads it every cycle whilst noise is combined,
  which makes the SID's most expensive block.
- With noise not selected, or the register locked up at 0, `sid_osc_advance` only counts bit-19
  rises. `sid_noise_settle_deferred` applies them: every counted rise is a shift, except one in the
  last two cycles, which is still in the pipeline. Exact, because nothing writes back into the
  register meanwhile.
- The 8580's triangle/saw latch for OSC3: every cycle with triangle or saw selected it takes that
  cycle's value, and it holds otherwise.
- 6581, sawtooth combined with another waveform: cycles from an accumulator, pulling its MSB low
  whenever the output's top bit is low in a cycle where the MSB is high. Entering the upper half
  from below, the output's top bit is always low, so the MSB never survives its rise and only the
  accumulator's low 23 bits move on.
- Noise combined with other waveforms, TEST clear: runs cycles at once, a window in which nothing
  changes but the accumulator (and, locked up, the shift pipeline). Noise combined with other
  waveforms, TEST set: the accumulator stands still and the pulse is high, so the output feeds back
  into the noise until it settles at a fixed point, and from there only the register's fade changes
  anything.
- `sid_osc_advance` advances cycles (at most 256, and freq * cycles < 2^24). `src_acc` and
  `src_freq` are the ring modulation source's accumulator at the start and its frequency; only noise
  combined with other waveforms uses them, cycle by cycle: wrong for up to the rest of the step if,
  within it, the source had TEST set, was hard-synced, or for voice 1, had its MSB pulled low. A
  6581 MSB pulled low cycle by cycle also clears `msb_rose`, so a hard sync from a rise earlier in
  the same step is dropped.
- Hard sync: restart o as if it had been reset at the moment src's MSB rose. Known simplifications,
  deliberate (inaudible): o's own noise clocks and `msb_rose` from the discarded part of the step
  are not recomputed, and reSID's special case of a sync source that is itself being synced is not
  modelled.

### Math

- SIDOU needs host and pi32v2 float results to be bit-identical; build without `-ffast-math`.
- `sidou_tanf`: tan for 0 <= x < ~1.4 without libm. Taylor series of sin and cos; error < 1e-6
  relative. Pure float ops, so host and target agree.
- `sidou_tanf_fast`: tan for 0 <= x <= 0.45 pi (the filter tables' range) without a series: its Pade
  approximant of order [7/6], within 4e-9 relative there. One divide, for where `sidou_tanf`'s
  twelve are too dear (the age/heat blend after each filter write).
- `sidou_atanf`: atan for g >= 0, within 3e-9 relative in exact arithmetic: reduced to [0, 2 - sqrt
  3], then the Pade approximant of order [5/4]. Three divides at most.
- `sidou_soft_clip`: cubic soft clip: identity near 0, flat at ±1 beyond ±1.5.
## 6502

### Accuracy model

cpu6502.h: SIDOU's 6502 (decision record 0008) is an NMOS 6510 for PSID tunes, stepped an
instruction at a time with exact cycle counts (page crossings and taken branches included). All 256
opcodes: the documented ones, NMOS decimal mode, and the undocumented ones as Tom Harte's
SingleStepTests define them.

cpu6502.h undocumented opcodes: The unstable stores (SHA, SHX, SHY, TAS) store `value & (high byte
of the base + 1)`, and a page crossing puts that value in the address's high byte. ANE and LXA use
the magic constant 0xEE. The twelve JAM opcodes stop the CPU with the PC on the opcode, so a call
again stops again.

cpu6502.c: One switch over the opcodes, the registers in locals, the flags unpacked (N and Z kept as
the last result's byte). The cycle count added from a table before an instruction runs so its
accesses can be stamped back from the end. Base cycles per opcode (NMOS); page crossings and taken
branches add theirs. JAMs are 0.

### I/O hooks

cpu6502.h: The 64 KiB memory is the caller's. While `io` is set and the 6510's port at $01 maps I/O
in (bit 2 and either of bits 0-1), reads and writes of $D000-$DFFF go to the hooks instead. Each
hook gets the access's cycle on the running count: a load's or a store's last cycle; a
read-modify-write reads on its third last and writes twice, the old value on its second last and the
new on its last, as the real CPU does. Fetches, pointers and the stack are never I/O and go to
memory.

cpu6502.c: An operand access to $D000-$DFFF goes to a hook while `io` is set and $01 maps I/O in;
the zero page and the stack never are I/O. The stamps count back from the instruction's end: `cyc`
already holds its last cycle + 1. RDK: a read k cycles from the end; WR: a write on the last; WR2: a
read-modify-write's two writes, the old value, then the new one.

### Cycle counting

cpu6502.h: `cycles` is the tune's running count: the core adds to it and never resets it, and the
limit is compared with wrapping arithmetic, so a count that wraps past 2^32 keeps working.

cpu6502.h `cpu6502_run`: Runs whole instructions until the running count reaches `limit`
((int32_t)(cycles - limit) >= 0), a JAM, or, when `stops` is set, a jump, return or RTI to
$0000-$0001 (the sentinel a call pushes) or a KERNAL interrupt exit. Returns the state, which it
also stores.

cpu6502.h KERNAL exits: The KERNAL's interrupt exits a PSID play routine may end in (with `stops`):
IRQ exits at 0xEA31u and 0xEA81u, NMI exit at 0xFEBCu.

cpu6502.h `cpu6502_reset_regs`: The registers a call starts from: A, X and Y 0, S $FF, P $24
(interrupts off), state RUN. The memory, the PC, the hooks, `io` and the count are left alone.

### Variants

cpu6502.h CPU6502_SMALL: The device's, built at -Os. Calls out of line for an access to the I/O
page, one compare inline. Without it the accesses are inline (larger, about 7% fewer instructions).

cpu6502.h CPU6502_HOOK_ALL: For the tests only. Every operand access, the zero page and the stack's
pushes included, goes through the hooks with the same stamps, whatever the address and $01, so Tom
Harte's tests can check each one's cycle.

cpu6502.c CPU6502_HOOK_ALL: Makes the address and bank tests true and sends the zero page's and the
stack's writes through the hooks too.

### Build

cpu6502.h: Portable C11, no allocation; the device runs it from flash in the main loop.

## Effects

### Post-chip effect chain

- The post-chip colour chain runs in fixed order: crush → tape → wow and flutter → CRT → noise. Each
  block has its own state, reset and tick function so other chains can reuse and reorder them.
- CRT's amount is smoothed on every sample; the others on every other sample with the coefficient
  for two steps `1 - (1 - k)^2`, which halves the smoothing work and the coefficient updates while
  parameters move.
- Each parameter snaps onto its target on a 32-sample grid that restarts whenever the chain starts
  from rest, so block sizes never change the result.
- Exact bypass: when every target and smoothed value is 0 (and wow has glided back to no delay),
  `sid_fx_process` returns at once without touching samples or state. A block whose smoothed
  parameter is 0 is out of the signal path. A block entering the path starts from its rest state.
- Different RNG seeds for wow's drift vs noise's hiss (`seed * 0x9e3779b9u + 1u` vs `seed`).

### ADAA (Antiderivative anti-aliasing)

- First-order ADAA for the drives' waveshapers: output the mean of f over the segment from previous
  input to current, `(F(u) - F(p)) / (u - p)` (Parker, Zavalishin and Le Bivic, DAFx-16; Bilbao et
  al., IEEE SPL 2017). Every shaper is piecewise polynomial, so each piece's mean has a closed form.
- Two ways to use a clip. Residual `r(u) = clip(u) - u`, with u added back undelayed (tape): small
  signals stay flat to Nyquist, but far past the knee the half-sample lag means the output
  approaches `1 + (u - p) / 2`, which a high-gain drive at high frequencies turns into garbage.
  Whole clip (overdrive, fuzz): the output is the mean of a bounded function so it stays bounded at
  any gain, at the cost of ADAA's `(1 + z^-1) / 2` droop on small signals (-1.6 dB at 8 kHz, -7.7 dB
  at 16 kHz), which the drives' pre-filters and gain make moot.
- Soft clip: `c(u) = u - (4/27) u^3` for `|u| ≤ 1.5`, ±1 beyond. Hard clip: `h(u) = clamp(u, -1,
  1)`. Rectified hard clip: `|h(u)|` for the octave-up of a fuzz.

### Chorus

- Topology: string-machine ensemble (Dattorro, "Effect Design" part 2, with Plaits' ensemble
  topology as reference). One delay line read by three taps per side, modulated by slow (~0.6 Hz)
  and fast (~6 Hz) sines, the taps 120 degrees apart and the right side's offset by 60. The
  phase-shifted sines come from one sin/cos pair per LFO by rotation, so six taps cost two sine
  evaluations per LFO.
- Amount: depth, the excursion around centre delay. Param A: rate, 0.25× to 2× the base rates (log).
  Param B: ensemble, how much of the fast LFO is in (0 gives plain slow chorus).
- LFOs, depth and rate are evaluated every 16 samples, at the segment's end, and the tap delays move
  linearly between. The depth goes through one more one-pole per segment, so even a smoothed jump
  changes the taps' read speed (their pitch) a little per segment rather than all at once.
- The two LFOs start out of step: `fast_phase = 0x2aaaaaabu` (2/3 of a turn).

### CRT / TV speaker

- Tone: two cascaded one-pole low-passes into a one-pole high-pass, each at log-frequency (equal
  knob travel, equal musical interval). Each pole is matched: `a = 1 - e^-w` (low-pass) and `r =
  e^-w` (high-pass).
- The filtered signal is mixed in over the first portion of travel (a smoothstep), so the tone is
  the identity at zero although the filters are not. Pulling the coefficients to 1 instead does not
  work: the high-pass then integrates and holds an offset that steps out when the block leaves.
- Hum: 50 Hz with 2nd and 3rd harmonics, read from a one-period wavetable built at init. Harmonic
  formulas: `sin 2t = 2 s c`, `sin 3t = 3 s - 4 s^3`.
- Coefficients come from tables over tone on 128 log-spaced points, interpolated linearly. Reset:
  filters start settled on current input x, so nothing jumps.

### Crush / bit-crush

- Sample-and-hold decimation plus coarser quantiser, each continuous in its own amount. Both at 1
  leave 2 bits at 1 kHz; both at 0 is the identity on int16 input.
- No anti-alias filter: the aliasing is the sound.
- Step inverse from exact power of two of integer part and second fit for `2^-f`, so no division.
- Phase initialised to 1.0 so capture happens on the first sample.

### Delay lines

- Two buffer types: float, and int16 (half memory for long delays; samples are normalised floats at
  interface, stored truncated towards zero).
- Two reads: linear interpolation (cheap; gentle low-pass at fractional delays) and 4-point
  3rd-order Hermite (Catmull-Rom; flat to much higher frequency, for chorus and flanger sweeps).
- int16 stored with truncation towards zero, not rounding: in a feedback loop each pass shrinks
  small values, so a decaying loop reaches exact silence instead of settling into a limit cycle of a
  few LSB.

### Echo / delay

- Mono in, stereo out. Mono: one tap at T on both sides. Ping-pong: taps at T (left) and 2T (right)
  with feedback from 2T, so echoes alternate left, right, left at T apart from a single line.
- Feedback goes through one-pole damping low-pass, and what is written (input plus feedback) through
  soft clip, so loop saturates smoothly.
- Time glides instead of jumping: speed eases up to at most one sample per sample and eases off
  approaching the target (tape-style pitch bend of at most an octave down or fifth up, which itself
  starts and ends smoothly). A fresh start takes the asked time at once.
- Log2 of max/min time ratio by halving (no libm), then `log2(r)` for r in [1, 2) by atanh series.

### Flanger

- Reimplemented from Dattorro, "Effect Design" part 2. Short delay swept by sine, read with Hermite
  interpolation (sweeps fast and far), mixed with input, with feedback through soft clip. Right side
  reads same line with LFO 90 degrees on.
- LFO, depth and rate evaluated every 16 samples, read delays move linearly between; depth gets one
  more one-pole per segment.
- Min delay clamped to at least 1.0 sample because Hermite needs one newer sample.

### Fuzz

- Pre low-pass (~7 kHz, which also tames SID's saw and pulse highs), up to +40 dB gain into hard
  clip, and octave-up blend from rectified clip; both shapers through closed-form ADAA of whole
  function. Then DC blocker. Reimplemented from Parker et al. (DAFx-16), Bilbao et al. (IEEE SPL
  2017) and Zolzer, DAFX.
- Amount: gain `1 + 99 a^2`. Makeup `max(REF, 1 / gain)`: unity whilst gain is low, then clipped
  output held at `REF = 0.18`, a clipped square this high has the RMS of a SID voice.
- Starved bias (up to 0.5 into clip), gated by envelope follower so it only acts whilst there is
  signal, as in a fuzz with sagging supply: silence stays silent, notes come out sputtery and
  asymmetric.

### Limiter

- The output's soft limiter runs once where effect slots convert output to int16. A level passes
  unchanged up to -2.5 dBFS either way; past the knee, slope 1 at knee (no corner), rising ever
  slower towards the ceiling (one inside the rails), which it never reaches.
- A soft knee is not idempotent, so nothing before it limits.
- No setting puts a sample on a rail. A signal that never passes the knee is the same bit for bit.
- Below knee a sample costs the two compares the clamp it replaces cost; past it, a call and a
  divide (out of line: one copy in SRAM). No libm; host and pi32v2 compute same bits.

### Noise floor

- Tape-like hiss. White noise from seeded xorshift32 (so host and target agree) through `1 - 0.6
  z^-1`, which tilts it towards highs (-8 dB at DC, +4 dB at Nyquist relative to white).
- RMS at amount 1 is the uniform white RMS times the tilt filter's gain.

### Overdrive

- Mid-hump pre-filter (x + hump × high-pass at ~700 Hz), gain, tape's soft clip through ADAA of
  whole clip (residual form breaks down at this much gain), post tone low-pass and DC blocker.
- Amount: drive, after gain law of Plaits' Overdrive (Emilie Gillet, MIT): most of rise in top half,
  1 at zero rising to 25 (+28 dB) at 1. Hump grows with it (0 to +8 dB at 1.4 kHz).
- Makeup keeps reference level of 0.25 full scale at about same loudness: `makeup = REF / clip(REF
  drive)`.
- Bias into clip (up to 0.4) for even harmonics; its static output is removed and DC blocker takes
  what rectification leaves. Bias at most 0.4 is below knee: clip there is the cubic.

### Phaser

- Reimplemented from Zolzer, DAFX (phaser) and Zavalishin, The Art of VA Filter Design (first-order
  allpass). Per side: six first-order allpasses sharing one coefficient, swept by sine over
  log-frequency table, with feedback from chain's output to its input. Output is input plus chain,
  which puts notches where chain turns phase by odd multiples of 180 degrees. Right side's LFO runs
  90 degrees on.
- LFO and rate evaluated every 16 samples, coefficients move linearly between.

### Plate reverb

- Dattorro's plate (J. Dattorro, "Effect Design, Part 1: Reverberator and Other Filters", JAES
  45(9), 1997, figure 1 and table 2), reimplemented from paper: bandwidth low-pass, four input
  diffusers, then figure-8 tank of two halves, each a modulated allpass, delay, damping low-pass,
  decay gain, second allpass and second delay, crossing into other half. Seven taps per side.
- Every line is float in one ring of 32768 floats (128 KiB FX pool) with one write pointer. Float so
  reads and writes need no conversion.
- Paper's lengths at 29,761 Hz scaled to sample rate then by scale factor so they fill but not
  exceed ring (0.98 at 44.1 kHz). Margin calculation: a margin of 16 samples for rounding each line
  up to whole sample.
- Values written are flushed so tank never decays into denormals.
- Modulated allpasses swing by 16 samples (scaled) at 1 Hz, two halves in quadrature (sin vs cos),
  LFO evaluated every 16 samples and read offsets moving linearly between.
- No pre-delay: at 0.98 the tank fills the ring.

### Light reverb

- C port of reverb in pichenettes/eurorack (rings/dsp/fx/reverb.h at f4cfa76, MIT). Griesinger
  topology from Dattorro's paper: four allpass diffusers on input, then loop of two branches, each a
  modulated delay read, damping one-pole, two allpasses and delay; two branches feed each other and
  give left/right outputs. Every line lives in one 16-bit ring (32768 samples) with one write
  pointer that moves back a sample at a time.
- Changes from original: C instead of C++ templates; delay lengths and modulation offsets scaled
  from 48 kHz to sample rate (rounded), up to what ring holds (above ~70 kHz they stop growing);
  LFOs are this project's phase-accumulator cosine instead of stmlib's, still updated every 32
  samples; input is mono (FX slot: (L + R) / 2), so input gain doubled to match original's L + R;
  values clamped into int16 range before conversion via explicit comparison (original converts then
  clips; results same in range, nothing undefined out of it).
- Reverb time mapping gives range RT60 about 8 s at top, where Clouds goes to 0.98 (minutes):
  SIDOU's reverb is a song's tail, not a freeze.
- Lines scaled to sample rate, but never past ring: above ~70 kHz lines keep their length in samples
  (shorter in time, smaller room).
- Compression: truncation as original's `static_cast<int32_t>`, not rounding.

### Effect slots

- Three slots after chip mix: Distortion → Modulation → FX → stereo out. Each slot runs one
  algorithm with same five controls: algorithm, Amount, Param A, Param B and Mix (dry/wet).
- Mono up to Modulation; Modulation makes stereo; FX feeds its wet path (L + R) / 2 and keeps dry
  path stereo.
- Amount, A, B and Mix smoothed on every sample: one-pole evaluated every 16 samples (on slots' own
  clock) and followed as straight line between; each snaps onto target on 32-sample grid. Clock
  restarts whenever slots start from rest, so block sizes never change result.
- Switching algorithm fades old wet path out, starts new one from rest (FX slot first clears its
  share of pool, a little per sample), runs it silently until its delay line holds its longest delay
  (so no tap reads cleared part whilst audible), and fades it in: dry path never stops, so no click.
  Slots switch one at a time (in slot order), so preset change costs at most one switching slot's
  overhead at once.
- Exact bypass: a slot whose Mix is 0 and settled, with no switch pending, is out of path and its
  state not touched. With every slot out, `sid_slots_process` writes input to both channels
  unchanged and touches no state.
- A slot coming back from rest starts its algorithm from rest too (no stale tails), in turn with
  switches (until then passes dry signal).
- Distortions made for SID voice at 0.25 of full scale (their drives' knees, makeups and
  quantisers). A voice as chip renders it on device is about 12 dB lower, so slot lifts its input by
  4.0× (power of two, exact) and lowers wet signal by as much again: algorithms hear voice at their
  level, slot's output is at chip's.
- Switch turns: a preset change that switches every slot would run three switches at once. Taking
  turns bounds extra cost to one slot's; full preset change takes up to three switch times. Only
  slot with switch pending may start one, only whilst no other mid-switch. Decided per chunk on
  slots' clock, so where calls end cannot change it.
- Steady path: per-algorithm loops kept small so fm1-emu decodes them (long-range conditional
  branches clang emits for one loop holding every algorithm don't decode). Kept out of line via
  `__attribute__((noinline))` for switching path functions.
- Warm-up samples: an algorithm must run before it is heard, for its longest tap delay + 4.
- Templates: every algorithm's initial state, initialised once by `sid_slots_init` (pure function of
  sample rate and seed). Starting one copies its template, so audio context never runs an init (on
  device inits stay out of SRAM). Delay-line algorithms keep only what follows their line: start
  clears line and copies rest.
- `fx_slots_init.c` in its own file so device builds it for size (`-Os`) whilst `fx_slots.c` whose
  render is SRAM code stays at `-O2`.
- STARTS table and ALGORITHMS array marked for RAM on device (`__attribute__((section(".data.*")))`)
  because read by audio path.
- Slot leaving path only on snap grid (32-sample boundaries), so where calls end cannot move that
  point.

### Tape saturation

- Pre-emphasis shelf lifts highs, biased soft clip saturates (bias makes it asymmetric, so even
  harmonics appear), exact inverse of shelf brings highs back down: high end hits clip harder, so it
  compresses first. Drive and makeup tied (`makeup = 1 / (drive × clip's slope at bias)`), so small
  signals keep unity gain (within 0.001 dB: reciprocal Newton-tracked). Clip's own DC (bias,
  rectification) removed by DC blocker on wet difference only.
- Clip is `u + a r(u)`, with `r(u) = c(u) - u`. So smooth (no corner) at every amount, identity at a
  = 0, full soft clip at a = 1. Only residual r goes through first-order ADAA; u passes undelayed,
  so small signals, where r is negligible, stay flat to Nyquist. Price: residual lags u by half
  sample: when high tone is driven far past knee the two no longer cancel cleanly, and such tones
  clip less hard than without ADAA.
- At amount a: drive `1 + 2a`, shelf boost `1 + 2a` above ~3 kHz, bias `0.15 a`. Bias at most 0.15
  is below knee: clip there is cubic.
- One reciprocal serves both gains. De-emphasis inverts the shelf.

### Tremolo and auto-pan

- Mono in, stereo out, from one sine LFO shaped from sine towards square by driving soft clip and
  renormalising its peak to 1.
- Tremolo: gain `1 - depth (1 - lfo) / 2` on both sides.
- Auto-pan: equal power, so a side panned fully peaks at `sqrt 2` times input (+3 dB): full-scale
  input can clip there. Kept for constant loudness and unchanged centre; slot's Mix or lower Amount
  leaves headroom.

### Utility

- Denormal flush: adding and removing 1e-18 rounds x to multiple of `ulp(1e-18) = 2^-83` when `|x|`
  below ~1e-18, so anything under ~5e-26 becomes exactly 0 and recursive filters never decay into
  denormals (slow, possibly flushed on target but not host). Audio-range values (`|x| > 1e-7`) pass
  unchanged.
- Newton tracking for `1/x`: one Newton step whilst x moved by under 1% since (each squares relative
  error, smoothed values move far less per sample), division on larger jump.
- sin(2 pi phase / 2^32): within 0.1% of full scale. Parabola refined with one more term.
- Smoothstep: `p^2 (3 - 2p)` for p in [0, 1]: crossfade curve with zero slope at both ends, so fade
  has no corner in envelope.
- `sid_fx_exp2(e)` for 0 ≤ e ≤ 30, continuous in e: exact at integers, within 0.3% between.
- Smoothing snap threshold chosen above the gap where one-pole step rounds to nothing near 1 (6e-5
  at 96 kHz), or it would stall short of target.

### Wow and flutter

- Delay line read at fixed centre delay plus modulated excursion. A delay changing at rate D' shifts
  pitch by ratio `1 - D'`, so excursion `A sin(wt)` deviates by at most `A w`; depths set from cents
  that way.
- Wow: ~0.5 Hz sine plus random drift (new random target in [-1, 1] every 0.25 s through two
  cascaded one-poles at ~0.5 Hz, so pitch moves smoothly). Flutter: ~8 Hz sine.
- Only excursion moves; centre delay (half line's worth of excursion) stays put whilst block active,
  so average pitch never shifts.
- Entering: block fills line with centre samples whilst passing input dry, then crossfades from dry
  to delayed signal over 10 ms (smoothstep of position whose speed itself ramps over 32 samples, so
  envelope has no corner, not even when direction reverses mid-fade). Leaving (both depths at 0):
  crossfades back to dry.
- Depths may jump: block slews its own copies, wow over 1 s per full range (deck settling: bounded
  pitch disturbance, about 12 cents at most) and flutter over 50 ms.
- Stale read prevention: after reset line holds old samples, read must never reach them. Fade starts
  once centre samples are in, from then depths start at 0 and grow by at most their slew per sample,
  so m samples later delay is at most centre + 0.0085 m at 44.1 kHz, always short of centre + m
  samples written.
- Linear interpolation used instead of Hermite because Hermite costs ~34 instructions per sample
  more (measured).

## Song model and files

### GTI5 instrument export/import

- Commands use the same operands in patterns and wavetable $F0-$FE.
- Export expands contiguous snapshots through referenced sequences without a bitmap; the table's
  deterministic next-row graph needs at most 255 steps per root.
- Import retains references even in currently unreachable table data and keeps every original base
  intact until all table operands are replayed to prevent corruption.

### SID drum kit

- Decision record 0009: eight drums as plain GoatTracker 2 instruments and table rows that open and
  play in GoatTracker unchanged.
- Kit rows start at row 1 of wavetable and pulse table.
- Every drum's first-frame waveform is $08 (test bit, gate off) to reset oscillator and noise
  register so every hit is identical.
- ADSR delay bugs: two envelope rate counter bugs made drums sound 53 ms after their row in
  GoatTracker's player and reSIDfp (decision record 0009), designed out by setting SR to $00 until
  the drum's SR row at frame 2 or 3 sets its release, and using attack 1 (32 cycles) instead of 0
  which a counter below 10 always reaches.

### .sng reader/writer

- Reader takes file through callback so file is never held twice in RAM.
- Two-pass read: first pass checks everything, changes nothing; only then does second pass load.
  Refused file leaves song unchanged.
- Writer writes what GoatTracker's savesong writes (decision record 0007 rule 1), then trailer if
  asked.
- Orderlists kept as GoatTracker loads them: one breaking readme 3.1 rules loads unchanged because
  GoatTracker plays it.
- GoatTracker Stereo's `determinechannels` is a guess (decision record 0007): GT2 part can pass or
  fail by chance. File read with both channel counts; reading that ends better wins. SIDOU trailer
  records channel count so SIDOU always reads own files correctly.

### Streaming reader/writer

- Each step handles at most one pattern (516 bytes) or about 256 bytes.
- Loading appends packed objects to empty arena without first-fit's O(file) search: sequential
  allocation avoids bitmap search overhead.

### Song model

- Decision record 0007, docs/reference/song-format.md. Region layout: header (gt2_song struct),
  bitmap, arena. Units calculated from remaining bytes: `((rest * 8) / 33)` for units × 4 bytes
  arena + 1 bit per unit.
- Edits in two steps so player never reads half an edit: cell write (one store ≤4 bytes) or rebuild
  (builds new in free arena, yields index entry swap as one aligned 32-bit store). Old space stays
  allocated until swap applied.
- Deferred contract: edit pending from primitive's return until `gt2_edit_release`. Every primitive
  reads index as player does, refuses with `GT2_ERR_PENDING` any edit a pending one would make
  wrong. Pattern: at most one pending swap, no swap while any cell write pending. Cell writes to
  pattern with no pending swap may queue (up to 127). Orderlist: at most one pending edit. Pattern
  removal waits for every pending orderlist edit.
- Player re-reads index entry each tick, never holds arena pointer: edit's release may free and next
  edit reuse any byte index no longer names.
- Arena keeps `SONG_ARENA_SPARE` free when every edit released. Top `GT2_SCRATCH_UNITS` (one largest
  pattern) are compaction's scratch. Rest of spare (524 bytes) holds rebuild's new copy (at most 516
  bytes) while it waits for swap.
- Compaction: each move slides lowest block above lowest gap down; at most 2 × blocks moves leave
  one hole.
- Edit apply for swap: compiler barrier on one core (`atomic_signal_fence` becomes
  `__sync_synchronize` call on pi32v2's clang). Store is one 32-bit st.

### Save accounting

- What savesong counts transliterates GoatTracker 2.77's gsong.c (decision record 0007).
- Pending edits reserve growth conservatively; cancellation gives back reservation.
- Interrupt changes only cell deltas and per-pattern row counts. `uint16_t` sums modulo 65536; at
  most 208 × 128 rows can contribute to any instrument.
- Cell application increments/decrements based on actual old row, so overlapping queued writes need
  no shadow copy.

### Extension area

- docs/reference/song-format.md. Trailer frame: magic, version, length, lead-in (channel count at
  offset 14), GT2 CRC, chunks, trailer CRC.
- CRC-32: zlib's (reflected 0x04C11DB7), nibble at a time, 16-entry table for 4-bit nibbles.
- Chunk move during size change: forwards when shrinking, backwards when growing (no `memmove` on
  device).
- LOCK: parameter locks sorted by pattern, row, parameter. 32-bit mix (multiply-xorshift) for
  generative play's random, seedable and deterministic.

## PSID player

### Environment

- Design follows FM-1 SID's player (psid.c, fm1-emu, BSD-2-Clause, Joakim Eriksson). SIDOU runs init
  in slices, honours CIA periods under 1,000 cycles, schedules late calls as C64's pending interrupt
  does.
- Time is 6502's running cycle count: 0 is init's first cycle. Comparisons are wrapping
  `((int32_t)(a - b))`.
- Init limit: 5 seconds. Play call limit: 1 second.
- Memory layout: STACK 0x100, KERNAL_IRQ_VECTOR 0x0314, HW_IRQ_VECTOR 0xFFFE. Call end detection
  uses sentinel at $0000/$0001 and KERNAL's interrupt exits.
- Bank register ($01): $37 below $A000, $36 up to $CFFF, $34 for $D000-$DFFF, $35 from $E000.
- CIA 1 timer A counter: counting down from latch, period of latch + 1.

### File format

- Design follows FM-1 SID's player. SIDOU checks data offset and load address (FM-1's loader uses
  them unchecked, docs/upstream-reports.md item 1).

### Controller

- Main loop runs 6502 ahead of audio. Audio interrupt places each SID write at cycle 6502 made it.
- Slice bounds pass's work: at most PSID_SLICE_CYCLES (1000) of 6502 time, less when estimated work
  reaches PSID_SLICE_WORK (20000). Sub-slices of PSID_SUB_CYCLES (32). Work estimates:
  PSID_WORK_CYCLE (12) per cycle, PSID_WORK_ACCESS (480) per SID access, PSID_WORK_NOISY (80) per
  cycle of voice combining noise with another waveform.
- Cycles alone cannot bound work: on fm1-emu 6502 costs about 9.3 instructions/cycle, but each SID
  access about 480 more (shadow's step, I/O page, ring). Voice combining noise costs shadow up to
  80/cycle until noise locks (31,000 for first 130 cycles of three such voices).
- Look-ahead limit: PSID_AHEAD_CYCLES (39312, two PAL frames) ahead of chip A's clock.
- Write ring: PSID_RING (1024) entries, 6,144 bytes. Ring entry 16 bits: chip << 13 | register << 8
  | value.
- Interrupt renders block: at most PSID_SPLITS (24) renders per block. PSID_BLOCK_ENTRIES (80) ring
  entries per block. Set by device costs (emu-gate psid_budget.rs) so worst blocks hold budget.
  Corpus' tunes need at most 15 renders and 18 entries/block. 24 renders clear dense players' bursts
  (Thomkat, Kill Me, Last Ninja) with 80 entries.
- Compiler barrier: `atomic_signal_fence` becomes `__sync_synchronize` on pi32v2's clang.
- Interrupt half runs from SRAM (platform/fm1/app.ld), built -O2 without jump tables. Main half runs
  from flash at -Os.

### Instrument lift

- Captures voice's registers over time, converts to GTI5 instrument.
- ADSR delay bugs designed out: gate-off first frame counter runs at release's period; with release
  up to 977 cycles it ran past attack 0's 9.
- Vibrato fit: speeds about turns' spacing, every shift, kept when at least halves what holding
  centre leaves.

## Song player

### GoatTracker 2 equivalence

- Player follows GoatTracker 2.77's player.s and altplayer.s routine for routine. Frequency table
  from gplay.c. gplay.c is GPL-2.0-or-later whose "any later version" lets this file stay
  GPL-3.0-or-later; player.s carries its own permissive notice. Where 6502 flag decides branch, C
  keeps the flag as variable.
- Shape analysis transliterates parts of greloc.c's relocator() and gtable.c's exectable() that
  decide packed player's shape and packing.

### Frame path placement

- Player is frame path: runs in audio interrupt at sample-exact frame boundaries, from SRAM on
  device, compiled -Os (once a frame, not a sample). Calls nothing in flash, reads only O(1) state
  each step. Portable C11, no allocation, no libm, no libc calls.
- Shape analysis is main loop's, in flash, runs in slices within budget.

### The shape

- Shape is what analysis makes of song for player to follow: everything deciding the writes, and
  numberings as tables. Main loop builds it in another buffer and swaps (player never writes one in
  use).
- Live edit can change shape packer would give: new note range, first command of class, wave delay
  or command, pulse low nybble, channel's first non-rest. Shape widening adds what one edit brings
  in O(1), says whether shape must be derived again. Main loop sends player wider shape in same
  batch as edit so player never plays edit with shape that lacks it.
- Base only grows; shape narrows only through fresh analysis, sliced in background, and swap.

### Player data

- Packer rewrites data: instruments reordered (hard restart first), patterns packed, tables
  remapped, wavetable bytes recoded, tempos less one. Player reads model's .sng bytes and applies
  rewrite where it reads, so cell edit plays at once.
- Orderlist edit plays through shape (streams), which main loop rebuilds and swaps with edit.

### Packed orderlist

- player.s steps through packed orderlist bytes with 8-bit index. Shape holds each channel's 256
  bytes: entries (repeat after pattern, R0 dropped), $FF, restart byte, then next orderlists packer
  wrote, then $00s.
- Byte marked raw (restart byte or past every orderlist) is packer's pattern numbering, which shape
  maps back to model's. Orderlist breaking readme 3.1 rules and reading past restart costs read like
  any other.

### Player options

- Hard restart ADSR (altplayer.s from $F000), multispeed (caller's N calls; 0 is 25 Hz), pulse and
  realtime optimisations, buffered writes, sound-effect support, author info, no optimisation, ghost
  registers.
- NTSC changes nothing in packed player, neither does custom SID clock. A-4 pitch not played: player
  keeps built-in table, reports GT2_PLAY_WARN_PITCH.

### Frequencies

- Packed frequency table holds notes first_note to last_note, lows then highs. Note outside table
  reads 6502 memory around it: past lows, the highs; past highs, packed song table (addresses
  depending on player size, GT2_VAR_UNKNOWN); below lows, player's variables. Byte it cannot know
  reads 0, as GoatTracker's editor reads past B-7.

### Variable layout and multispeed

- player.s variables below frequency table: blocks of seven a channel, NUMCHANNELS channels a block,
  then ghost registers of full buffering. Shape's var table describes them.
- Packer numbers instruments hard restart first, then no hard restart, then legato. Multispeed adds
  one to no-HR count (for multispeed stability), moving legato class start past its first
  instrument: that one plays as no-HR.

### Instrument classes and pulse packing

- Packer sorts instruments by class: normal (HR), no-HR, legato. Wavetable's left byte recoded:
  inaudible waveforms $E0-$EF become $00-$0F, with wave delays every waveform moves up by $10.
- Simple pulse: width's high nybble into low byte's low nybble (one byte written to both registers),
  modulation speed becomes same rotated step.

### Table reads

- Table's left or right byte at 1-based position; 0 past table and at position 0 (speedtable's extra
  zero entry). Pattern: NULL for pattern not held (64 empty rows) or number past patterns.

### Packer's instrument bytes

- Gate-off timer's low 6 bits masked. Vibrato only with delay, and delay less one: returns d - 1 if
  d nonzero, else 0.

### Pattern numbering

- Pattern numbering search: model pattern packer numbers q, $FF past packed patterns. Only raw
  orderlist byte and start take it, bounded (208 steps), so shape keeps one table not two.

### Frequency table in RAM

- gt2_freq_lo and gt2_freq_hi in RAM: audio path's tables must be on device (docs/hardware.md).
  pi32v2's clang puts static never written in flash .rodata, so section named, as sid_env.h does.

### Writes and buffering

- Write player.s makes at once unbuffered, kept in channel variable when buffered, for register
  load. Load registers: player.s writes AD, SR, pulse, frequency, wave; altplayer.s writes wave,
  frequency, pulse, AD, SR. Simple pulse: both pulse registers get low byte. Ghost registers: values
  to ghost registers only. Ghost register copy: player.s full buffering copies last call's registers
  before play; with zero-page ghost registers main program copies after call.

### Song control interrupt section

- Interrupt's functions first in song_ctl.c. On device app.ld puts them in SRAM, one section a
  function. Portable C11, no allocation. Interrupt side runs from SRAM, calls nothing in flash.

### Compiler fence

- Compiler fence keeps stores before it before ones after. One core: interrupt either runs before
  store or after, never beside.

### Song in audio interrupt

- Player runs in audio interrupt at sample-exact frame boundaries: audio renders chip A up to sample
  where call falls, makes call (writes straight into chip A), renders rest of block (split render,
  decision record 0005). Host renders song with same calls at same samples, so two equal bit for
  bit.

### Frame clock

- Call every CIA period, gt2reloc's .sid speed (0x4cc7 / N + 1 cycles at PAL 985,248 Hz, 0x42c6 / N
  + 1 at NTSC 1,022,727; 1x is video frame) at 24 MHz / 544 samples a second. Next call's position
  kept in whole samples and remainder, so calls never drift. Multispeed up to 4: call at least 184
  samples from next, longer than block (128), so block holds at most one.

### Command ring

- Main loop to interrupt, one producer, one consumer, one core. Ring size 32, power of two.
  Commands: play, stop (voices' gate off so releases ring), continue, mute mask, shape swap,
  hand-over of fast-forwarded player, jam notes, song edits, call period.
- Main loop's commands of pass visible together. Interrupt applies at block start, after register
  ring drain, between two calls.

### Edits in interrupt

- Song edits in interrupt O(1) only: cell write of at most 4 bytes or index swap. At most 8 applied
  in block; rest wait. Each command carries sequence number; interrupt publishes last applied: every
  edit's acknowledgement, after which main loop releases what swap replaced.

### Event ring

- Interrupt to main loop: each call's row and note events with call's number. Sized for second: 256,
  power of two (three channels at speed 1 is 150 and more). Main loop falls behind: oldest dropped
  and counted.

### Shape buffers

- Two shape buffers: one player follows, one main loop builds. Buffer written again only once
  interrupt applied command that let it go. Main loop widens shape for every edit before edit goes
  (player never plays edit with shape that lacks it), narrows with fresh analysis in background once
  edits stop, hands player over to fast-forwarded one when shape change is one player's state cannot
  follow (channel count, state encodings).

### Hand-over

- Fast-forwarded player for shape change player's state cannot follow, and seeks. Interrupt copies
  word by word (not struct assignment, may become memcpy call in flash), chip A gets fast-forwarded
  player's registers. Draft stays interrupt's.

### Analysis structure and view

- Analysis keeps what it found as it walks (base: patterns and instruments in use, table rows
  reached, commands, notes, transposes, which channels hold anything), derives shape from it. Base
  only grows (OR-only); shape narrows only through fresh analysis, sliced in background.
- Song analysis reads model with caller's pending edits on top (view), since audio interrupt applies
  later: widening or derivation reads what song will be once applied.

### Analysis slicing

- One slice at most about budget units: one an orderlist entry, pattern row, table row or step,
  instrument (four table walks besides), numbering entry, stream byte. Device budget 200 units
  measured in fm1-emu against main loop's allowance 39,000 instructions. 200 not 300: shape built
  for size, costs more per analysis unit, slice stays within bound.

### Walking the song

- Marks rows table program reaches from ptr; returns rows it took. Pointer onto jump: packer refuses
  song. Speedtable breaks immediately, others walk until reaching previously seen row or past end.

### Derivation phases

- Phases: instrument, wavecmd, wtable, ptable, final, streams, end. Instrument: class, table
  programs, features. Wavecmd: wavetable row starting another table. Final: final numbers,
  numberings and layout. Stream: channel's packed orderlist stream.

### Packed subtune index

- With one song packed init ignores subtune; with more it is song's index among first songs
  subtunes. Returns -1 if subtune not packed.

### Shape compatibility

- Shapes compatible if same channels, subtune, songs, and same encodings (simple pulse, wave delay,
  wave command). Otherwise main loop hands player over to one fast-forwarded with new shape.

### Audition

- Jams that sound with song stopped. Calls run on frame clock with sequencer held: voices play
  instruments and tables and jams, no row read. Starting takes chip A as PLAY does.

### Digi drums

- Note whose instrument DIGI chunk marks starts built-in sample at call's sample (one at a time, as
  C64's one volume register: hit cuts last). Muted voice or one keys hold plays none.
- Codes play as 6581 volume DAC makes them, (code - 8) * 875 (875 int16 LSBs a code step), each held
  5 samples (about 8,824 Hz), added to out and clamped to int16.
- Voices whose instrument is digi drum main loop mutes, so instrument's own sound stays off chip
  while sample plays; note of another instrument unmutes.

### DIGI map

- DIGI chunk's map for interrupt, bytes that changed only: entry interrupt reads never 0 on way to
  new value. Built when play, continue or audition starts, and when extension area's size changes.
  Not every pass, which MIDI flood makes many. Updates 256 entries so any instrument byte indexes
  without test in interrupt.
- Song with no digi drum skips events: few loads a pass.

### Parameter locks

- Interrupt never reads extension area. At each row channel starts, main loop resolves filter and
  pulse locks channel's next row may have (pattern's next row, or at its end pattern again and
  orderlist's next) into small table, published by index flip.
- At row's start interrupt takes entries whose row it is, after each row's calls writes locked
  registers over player's (its own state and shadow untouched), until row without them.

### Lock resolution

- Channel's resolved locks for next row, at most: at pattern's last row look-ahead holds row 0 of
  pattern and next one, each with up to one lock of each five filter and pulse parameters. Row holds
  one lock a parameter; undo's restored locks are cut row's, which model then has none of.

### Chance rolls

- Per-step chance: main loop rolls for steps channel may read next, as it resolves locks, tells
  player ones that lose. Seed is app's (time at PLAY); roll's count restarts at each PLAY. Up to two
  rows pattern's end may lead to.

### Wavetable draft

- Wavetable draft heard before written: on one channel wavetable rows read as these, jammed note
  starts wave program. Word aligned wherever: interrupt copies in words, FM-1 faults on misaligned
  word access (1.1.0's ARP helper rebooted device). Shape widens for draft's delays and notes before
  next jam.

### Far calls

- Player's frame path runs from SRAM on device, beyond direct call range of flash code: through
  pointers compiler cannot fold into direct calls.

### Tick 0 commands

- Commands 0-4 set continuous effect following ticks run; 5-F act once. Command 0: instrument
  vibrato. Commands 1-2: portamentos (vibrato phase restarts). Commands 3-4: toneportamento,
  vibrato. Commands 5-F: AD, SR, wave, wavetable start, pulsetable start, filter table start and
  control, cutoff, master volume/timing mark, funktempo, tempo.

### Continuous effects

- One tick of effect (0 instrument vibrato, 1-2 portamentos, 3 toneportamento, 4 vibrato) with
  speedtable entry. Speed's left byte $80 and up is note-independent: step between last note and
  next, shifted right by right byte.

### Frequency calculation

- Channel's frequency from note: known, unless byte came from past table. Marks unknown bit if byte
  from unknown region.

### DEFAULTTEMPO

- Instrument 63's AD less one if it is 2 or more and instrument has no wavetable (readme 3.6), else
  6 ticks a row times multispeed. If ad63 >= 2 and ins 63 wave is 0, use ad63 - 1, else mult * 6 - 1
  (or 5 for 25 Hz multispeed 0).

### Muldiv by shifts

- a * b / d and remainder by shifts. No 64-bit division helper on device. Builds uint64_t numerator,
  divides bit by bit, quotient and remainder both fit in 32 bits.

### Voice masking

- Muted voices: voice's gate off, then writes dropped. Taken voices (keys'): nothing of song's,
  keys' writes stand. Back from either: song's registers again. Song starts from silent chip, less
  voices keys hold, whose note sounds on.

### Load states

- What SONG_LOADING ends in: PLAY command (song plays), PLAY command paused (audition, no song
  played yet), chip A taken again for audition of stopped song, chip A taken again then CONTINUE.

### Held PLAY

- Next call while held PLAY or CONTINUE waits for align: far ahead (hours of blocks). Align: next
  call at sample of interrupt's count, or at next block's start if that passed; frame clock goes on
  from there.

### Derivation purposes

- What derivation in slices is for: edit widened shape, hand-over needs true shape, derivation
  complete, jam instruments/notes or draft needs wider shape.

### Narrowing

- Fresh analysis of song as it is, once edits stopped. 50 polls with no edit before widened shape
  narrowed again. About 70k instructions at worst: more than main-loop pass has to spare, so runs in
  slices.

### Fast-forward and hand-over

- Fast-forwards player from play's start to call, hands over at interrupt's next call. Runs 6 calls
  a slice. Arms 4 calls ahead of interrupt's call. Armed hand-over waits for its call or refuses if
  too late.

### Draft coverage

- Draft needs tracked with bits: delay, relative note, note 20 or more (shape's notes from 0),
  highest relative note. Fresh analysis reads draft's rows, covers what they need. Wider than shape
  sets flag, derived again before next jam.

### Held step

- Held step's locks: channel plays n locks over player's after each call, whatever its rows lock,
  until call with n 0. Locks pinned while held. Copies word by word to avoid memcmp.

### Multispeed error

- Above multispeed max (4): two calls in block. Block is 128 samples; with multispeed 4 call at
  least 184 samples from next.

## FM-1 platform

### Image layout

platform/fm1/app.ld: RAMTEXT at 48 KB (0x01C00000–0x01C0C000) instead of Felucca's 24 KB. RAM and
debug area moved up. RAM/POOL boundary 8 KiB down from Felucca's 0x01C20000 for the song's 64 KiB
content region. Code stays below 0x01C0C000 until a device check proves the SRAM safe above it;
`_rt_end <= 0x01C0C000` assertion enforces.

app.ld: `.ram_text` before `.text`: first rule that matches places it, so `.text` doesn't capture
audio functions first. crt0 alone in `.entry` first (SPL enters image's first byte). Static
assertion `_start == ORIGIN(XIP)`.

CMakeLists.txt: sidou_app.bin is `.entry`, `.ram_text`, `.text` and `.data` joined. fm1_cstart
copies from there. Link uses `--orphan-handling=error`.

emu-gate tests/ram_audio.rs: No direct call may cross flash/SRAM: SRAM at edge of pi32v2's branch
reach (~4 MB) from XIP. Crossing usually fails to link with "relocation truncated to fit", but may
succeed when addresses happen to be in reach. Test checks linker's relocations (firmware links with
`--emit-relocs`) and that ALNK0 interrupt runs only from `.ram_text`. Constant tables stay in flash
except those `.ram_text` reads: definitions put them in `.data` (RAM), test fails on XIP loads.
Pulldown rows stay in flash. docs/hardware.md "Code in SRAM" has the rules.

app.ld: Stacks with 256-byte guard bands. User stack 8 KiB, three times deepest use (2,700 bytes:
every test's machine checks both stacks stay within half). System stack 7,936 bytes, deepest 1,820.

app.ld: POOL starts with song's content region: `__song_region` 65,536 bytes, 4 KiB aligned.
Overlays with C64's 64 KiB address space.

core/sid/sid_dac.c: DAC tables (17,408 bytes per chip model) in POOL, not RAM. Assert catches
fallback into `.bss`: `_pool_dac_end - _pool_dac_start >= 2 * 17408`.

platform/fm1/midi.c: USB DMA buffers (usb.c, `-fdata-sections`): usb.c declares `aligned(4)`, but
EP1 IN buffer at 4 mod 8 sends nothing. EP1 IN on 64-byte boundary, others on 8-byte boundaries. EP4
IN (USB audio, ep4tx) in compiler's merged globals: emu-gate tests/usb.rs checks 8-byte alignment.
USB audio ring (2 KiB) zeroed with POOL.

platform/fm1/audio.c: Song transport's interrupt-side state not zeroed, or a `.sid` tune's over it
(write ring, environment, voice shadows, header) via `.song_noinit` in `.song_ram`.

### Boot

platform/fm1/main.c: Init order: watchdog, fatal vectors, memory, LCD, live instrument, key matrix,
MIDI (TRS UART, USB), audio (ALNK0 IRQ), 10 kHz tick (TIMER5 IRQ, polls MIDI), interrupts on,
screen's first frame.

platform/fm1/fm1_cstart.c: Boot-loop guard first. HAL's second-core hold (fm1_cpu1_halt) at every
boot and before every reset. SIDOU never starts second core, but one left running would execute
whatever its PC points at during RAM clear/fill. HAL builds hold only with `FM1_CPU1_USED`;
`FM1_CPU1_USED 0`.

main.c: Priorities: TIMER5 (4) above ALNK0 (3), so tick keeps pace during half buffer render.
Watchdog `WDT_TIMEOUT 0x0Du` (0x0A is 1 s, each step doubles). `TICK_MS (1000u / LIVE_TICK_HZ)`.
live_tick catches up at most `TICK_CATCH_UP` (2) ticks per pass. A pass runs at most one preparation
slice regardless of ticks caught up: two would be 2×40k on top of view's 68k, past 120k poll
allowance.

platform/fm1/crt0.S: Boot words saved to debug area: `r0`, `icfg`, `psr`, `sp`, `ssp` at 0x1C0DF00 +
0x20/0x24/0x28/0x2C/0x30 before anything else. Stacks set to `_ustack_top` and `_sstack_top`. Final
`goto .` freezes if `fm1_cstart` returns.

platform/fm1/build_options.c: `SIDOU_BUILD_OPTIONS` string in `.sidou_options`. Build metadata
without consuming RAM.

platform/fm1/hal_check.c: Includes every `fm1_*.h` HAL header to verify they compile together with
`FM1_CPU1_USED 0`. Dummy `fm1_fault` links the HAL's crash path.

### Audio

platform/fm1/audio.c, audio.h: `AUDIO_SAMPLE_RATE 44118u` (not 44100): measured rate HAL's dividers
produce. `AUDIO_HALF_FRAMES 128u`: half-buffer size. Double-buffered (256 total). Interrupt fills
one while USB drains the other.

audio.c: `SLOTS_SEED 0x51D0u` for `sid_slots_init` (FX slot randomization). Changing it changes
every reverb/chorus sound slightly.

audio.c: `far_drain` volatile function pointer forces long call across SRAM/flash boundary when
`running` is false. Direct call may link but breaks no-crossing rule. `reg_ring_drain` in flash;
`audio_fill` in SRAM.

audio.c: `song_region` 64 KiB, 4 KiB aligned, in `.pool.song_region`. Overlaid with C64's address
space for `.sid` tune. Assertion `SONG_REGION_BYTES == 0x10000u`.

audio.c: `player_isr` union: song transport/shapes OR psid ctl/work/header. Both players'
interrupt-side state, never both live. In `.song_noinit` (not zeroed). `song_space` union:
`song_work` OR `psid_lift`. Assertion `sizeof(psid_lift) <= sizeof(song_work)`.

audio.c: `__asm__(".set song_transport, player_isr")` exports symbol for linker tests.

audio.c `audio_ramp`: fade over one block. Assertion `AUDIO_HALF_FRAMES == 128u` because ramp shifts
by 7 (k / 128 as gain). `AUDIO_QUIET_PEAK 128`: threshold for peak scanner. Below this for many
blocks = silence.

### Input

platform/fm1/input.c, input.h: `INPUT_FRAME_US 1100u`: scan frame (assertion checks `FM1_NCOL *
FM1_LED_TICK_US`). `INPUT_PRESS_FRAMES 2u`: debounce count (assertion checks `FM1_DEB_PRESS`).

input.c: `INPUT_MASTER_READS 4`: ADC readings averaged (ADC is noisy). `LEDS_PERIOD_MS 10u`: 100 Hz
LED refresh (HAL scans at 909 Hz, human eye sees ~60 Hz).

input.c: `input_raw_keys` disables LED shift register, scans matrix for 14 buttons + all note keys,
restores pins. For recovery mode and bootloader checks (runs before `input_init`). `raw_pins()` sets
all pins to input with pullups, disables shift register.

input.c: Separate dim and on/off per key: `fm1_led_dim` bitfield and `led_key` on/off. Dim keys
(held notes) glow; on keys (buttons) bright.

### LCD

platform/fm1/lcd.c, lcd.h: `LCD_BAUD 4u`: SPI divider after wakeup (~20 MHz). `LCD_W`/`LCD_H 240u`.
`LCD_BUFFER_PIXELS (240 * 8)`: 8 rows, double-buffered (tight RAM).

lcd.c: `LCD_RGB` with `LCD_SWAP`: ST7789 expects RGB565 big-endian, pi32v2 is little-endian. Swap at
compile time.

lcd.c `lcd_init`: reset, 80 ms, software reset (0x01), 150 ms, sleep out (0x11), 120 ms, config.
ST7789 datasheet timing. `during` callback in 150 ms wait (first USB init), `while_waking` in 120 ms
wait (more USB setup). Overlaps slow LCD timing with work.

lcd.c: `busy` flag and `lcd_sync`: SPI DMA is async. `busy` set after `data()`, cleared after
`lcd_sync`. Prevents starting new transfer before last finishes. `window` command (0x2A/0x2B/0x2C):
sets column/row range, streams pixels. 11 bytes total (`LCD_WINDOW_COMMAND_BYTES`).

### libc

platform/fm1/libc_min.c: `memcpy`/`memset`: align to 4-byte boundary, then word-at-a-time (pi32v2's
word load/store is faster). `memcpy` checks both pointers align the same before switching to words
(`!(((uintptr_t)d ^ (uintptr_t)s) & 3u)`). `may_alias` attribute on `word` type tells compiler
aliasing is allowed.

## FM-1 I/O, store and web build

Note: Much of midi.c's and usb_audio.c's rationale is in platform/fm1/VENDORED.md and
docs/hardware.md.

### MIDI interface

- TIMER5 interrupt polls Felucca's USB stack and TRS UART driver (usb/, vendored) each at 2 kHz.
  Push every USB-MIDI packet and TRS byte, stamped with fm1_ticks() at poll, onto ring. Main loop
  drains ring through parser per source onto live.
- Interrupt-masked operations (flash erase or program): polls stamped late stay out of clock's fit
  before drain takes them.
- MIDI inputs non-real-time activity: completed messages and every SysEx byte for store's quiet
  detector. Clock and sensing don't disturb it.
- FM-1 Live Link over USB SysEx: without session main loop pays one test of this byte.
- MIDI send (100 Hz): panel parameter changes out over USB as CCs, NRPNs and program changes into IN
  endpoint's ring, which TIMER5 interrupt sends. Never waits: no room means change stays pending; no
  host means dropped.

### Store write timings and windows

- How store writes while audio plays (docs/hardware.md): program in window with interrupts masked at
  most STORE_WINDOW_LEAD_US (250) after end of audio block and with no block due, ends long before
  next block's interrupt. Else STORE_LATER at once, tries again on later pass.
- Held operation (erase; program when no window fits): only when device quiet, audio faded to
  silence and DMA looping silence, interrupts masked at most STORE_HOLD_US (35,000); else
  STORE_LATER at once, so no caller can start one while audio plays.
- Timings PROVISIONAL: P25Q80H datasheet maxima until spike measures unit's part. To retune: set
  STORE_*_MAX_US from spike; STORE_WINDOW_BYTES and erase mode follow by plan's decision rule.

### Timing constants (provisional)

- STORE_PROG16_MAX_US: 3000 (page program of 16 bytes; tPP: 3 ms for up to 256).
- STORE_PROG64_MAX_US: 3000 (64 bytes).
- STORE_PROG256_MAX_US: 3000 (256 bytes).
- STORE_ERASE_MAX_US: 20000 (4 KiB sector erase, tSE).
- STORE_DRIVER_US: 100 (driver around one operation; estimate).

### Store budgets

- STORE_WINDOW_US: 1000 (write window masks interrupts at most this long).
- STORE_WINDOW_LEAD_US: 250 (starts at most this long after block's end).
- Window's test takes program's own way in off lead, so program starts within it: test passed at
  lead's last tick started program 0.004 µs past it.
- STORE_HOLD_US: 35000 (held operation masks interrupts at most this long).
- STORE_HOLD_GAP_MS: 10 (between held operations: TIMER5 drains TRS ring).
- STORE_SYSEX_GAP_MS: 250 default (build option: cmake -DSIDOU_SYSEX_GAP_MS=100/250/500). Between
  two SysEx packets' holds while only keys sound. Chosen by ear. Asserted 10-2000.
- STORE_QUIET_MS: 500 (quiet: no key, no MIDI input, output low this long).

### Store decision rules

- STORE_WINDOW_BYTES: programs go in windows of largest of 256, 64, 16 bytes whose worst program and
  driver fit STORE_WINDOW_US; 0 when none fits (programs are held operations then, settings save
  waits for quiet). Formula: if PROG256 + DRIVER <= WINDOW_US then 256, else if PROG64 + DRIVER <=
  WINDOW_US then 64, else if PROG16 + DRIVER <= WINDOW_US then 16, else 0.
- Erases: held 4 KiB sector erase if its worst fits 30 ms (35 ms less driver's margin). Other modes
  not built. Asserted: ERASE_MAX_US <= 30000, ERASE_MAX_US + DRIVER_US <= HOLD_US, PROG256_MAX_US +
  DRIVER_US <= HOLD_US.

### Why 250 µs

- Block is 128 frames at 44,117.6 Hz = 2,901 µs. Audio interrupt's worst fill (2,700
  instructions/sample) is 1,440 µs at 240 MHz, so next block's interrupt comes at least 1,461 µs
  after block's end. Window that starts within 250 µs and lasts at most 1000 µs ends 211 µs before
  it at worst.
- Asserted: WINDOW_LEAD_US + WINDOW_US < BLOCK_US - FILL_WORST_US (window never delays next block's
  interrupt).

### Flash spike

- Build option -DSIDOU_FLASH_SPIKE=ON (never released). Measures flash timing on device.
- Includes second copy of fm1_flash.h whose FL_RANGE_OK is spike's two sectors instead of staging
  area. RAMFN code static to that file, so copies don't clash.
- Functions measure: status registers read, SFDP read, page erase time, erase-suspend (starts sector
  erase, waits, suspends 0x75, reads, resumes 0x7A, waits for completion, measures
  suspend/read/resume times).

### UI

- Static grid cells in .pool section.
- Version string checked: SIDOU_VERSION must fit VERSION_CELLS (9).
- Grid run must fit LCD buffer: GRID_RUN_PIXELS <= LCD_BUFFER_PIXELS.
- Stamp mechanism: 2-cell wide stamp at bottom row.

### Update

- Update protocol using Felucca's src/ota.c (details in VENDORED.md).

### USB audio

- Details in `platform/fm1/VENDORED.md`, "USB audio out".
- TIMER5 interrupt runs producer from flash (not in RAMTEXT like rest of audio).
- Block copied from I2S half buffer, producer runs on it in quarters (32 frames each) over 4 ticks.
- Ring placed in POOL by app.ld (2,048 bytes), plus block buffer (1,036 bytes).
- Resampler history (264 bytes), EP4 IN buffer (196 bytes), uac struct (72 bytes) in RAM region.
- Table stays in flash (~7,680 bytes).

### USB config

- Defines switches for usb.c: FELUCCA_CDC 0 (no CDC console), FELUCCA_UAC 1 (USB audio on).
- String descriptors: FELUCCA_USB_MANUFACTURER, FELUCCA_USB_PRODUCT (hooks into src/usb.c).
- Host harness includes this to test same descriptors device has.

### OTA flash

- Flash hooks for src/ota.c step 1 of update protocol.
- Erases and writes only inside staging area 0xE0000-0xE5000.
- Operations in interrupt-off windows, only while audio stopped, else returns -16.

### Web platform core_api.c

- WASM export interface for song player.
- EXPORT macro: `__attribute__((export_name(#name)))` when __wasm__ defined.
- Buffers: IN_MAX = 4 × SONG_REGION_BYTES, OUT_MAX = GT2_SAVED_MAX + 1024, PCM_MAX = PCM_BLOCKS ×
  SONG_BLOCK (PCM_BLOCKS = 345).
- Region 64 KiB aligned (uint32_t array). Song region, input/output buffers, PCM buffer all static.
- Sample rate: SC_RATE = 44118.

### Web platform libm_min.c

- Minimal libm for WASM: fmaxf and fminf only.
- Handles NaN: if either arg is NaN, returns other arg. If both NaN, returns second.

### Web platform link_api.c

- FM-1 Live Link protocol for web (docs/reference/live-link.md).
- Grid dimensions: W = GRID_COLS × GRID_CELL_PX. PAYLOAD_MAX = 48.
- Protocol commands: HELLO 0x41, COLOURS 0x44, GLYPHS 0x45, CELLS 0x46, PIXELS 0x47. OVERLAY 0x10,
  BLANK 0x20.
- Two framebuffers: fb565 (RGB565) and rgba (RGBA8888), both W × W.
- Palette: pal565 (RGB565) and pal_rgba (RGBA8888), 16 colors each.
- Per-cell state: character, attribute, x-position, 8-byte mask, is_pixel flag, 64-byte pixel data.
- Two GRID_RUN_PIXELS buffers for double-buffering, aligned(4).
- Dirty rows: 32-bit bitmask tracking which rows need redrawing.
- Color conversion: RGB565 to RGBA8888 - r expanded 5→8 bits, g expanded 6→8 bits, b expanded 5→8
  bits, alpha = 0xFF.

### Web platform link_font.c

- Copies built-in font to modifiable font_glyphs array.
- font_glyphs: FONT_LAST + 1 glyphs, 8 bytes each, aligned(4).
- link_font_init: copies from font_glyphs_built to runtime font_glyphs.

### Web platform string.h

- WASM string.h stub: declares memset, memcpy, memcmp, strcmp, strcpy.

## Tools

- `tools/fm1-docker` runs a command in the pinned build image (`ghcr.io/dsablic/sidou-build`,
  `tools/docker/Dockerfile`), by the digest in `tools/docker/image.digest` when there is one,
  else `:latest`; when it cannot be pulled, or with `SIDOU_BUILD_LOCAL=1`, it builds the
  Dockerfile locally. The repository is mounted at the same path inside, so CMake caches and
  paths stay valid outside. Inside the image (CI, `SIDOU_BUILD_IMAGE` set) it runs the command
  directly.
- The build image carries everything CI needs (the JieLi toolchain, a pinned Rust, GCC 13, clang
  and libclang, Node and Playwright's Chromium with its system libraries), so no job installs
  anything with apt: an apt step hung `pages.yml` and `ci.yml`'s tag runs. It marks the
  checkout a safe directory, because CI checks the repository out as another user.
- `tools/gcc-check` compiles every C file the host tests build (sidou-sys's `SOURCES`, in
  `build.rs` order, plus the 6502's and the chip's test-only variants, the gt2-ref shim, the
  PSID oracle and sidou-pack's harnesses) with GCC 13 and the cc crate's release flags, warnings
  as errors: macOS's clang hides errors GCC reports, and Linux CI compiles with GCC. GoatTracker's
  own files build without warnings. `uac_harness.c` builds at `-O2`, as its `build.rs` builds it:
  `usb.c`'s unused SIE functions hold pi32v2 instructions.
- `tools/wasm-core` and `tools/wasm-link` build the site's two WebAssembly modules with no
  imports and no libc (`platform/fm1/libc_min.c`, as on the device), compiled by the image's
  clang and linked by the Rust toolchain's `rust-lld -flavor wasm` (or a `wasm-ld` on PATH;
  `SIDOU_WASM_LD`, `SIDOU_WASM_RUST_LLD=1`). The core has a 64 KiB stack and about 1.1 MiB of
  static data (the song region, the buffers, the effect slots' pool and a preview's copy of the
  song); the Live Link viewer a 16 KiB stack and about 0.4 MiB.
- `tools/web-demo` builds fm1-emu's web crate at the revision emu-gate's `Cargo.toml` pins, one
  pin for the gate and the demo; its checkout is forced, which also undoes any edit to a tracked
  file in the fetched copy.
- `tools/preflash` reads local files only: it never opens, enumerates or talks to a USB or MIDI
  device, never imports the installer and never runs the steps it prints (`docs/flash-day.md`
  is the checklist around it). The loader a package must carry is named by its SHA-256
  (`sidou_pack::FELUCCA_LOADER_SHA256`) and its handshake identity.
- `tools/sysex-roundtrip` sends only manufacturer `7D 'SD'` frames, and its disposable file uses
  store type 0x7F, one nothing on the device reads (1 is a song, 2 a .sid, whose header the
  device checks, 3 SysEx's instrument staging; `app/store.h`).
- `tools/readme-check` enforces the Firmware Hub's rules for `fmhub.json`
  (thefirmwarehub.com/submit): "what" is plain English, at most 300 characters, no HTML, code
  or links; "kinds" one of six; and a minor or major release changes the card.
- `tools/fm1-unbrick-mac`: `scsiio/__init__.py` raises ImportError on macOS before anything
  else, so the script registers the package without running it, so that its `common.py`
  imports as on Linux, and supplies the `SCSIDev` that `jltech/uboot.py` imports. Detaching
  macOS's mass-storage driver needs root.
