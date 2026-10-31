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

## LAB

- Each drawing is sampled every 2 pixel columns (0.2 mm on the panel) to halve the drawing's cost:
  ENVELOPE at every pixel costs about 70,000 instructions; every 2 pixels about 35,000. The spans
  are turned into 8×8 masks on the stack and set as the cells' pixel overlays; no bitmap is kept.
- A cell's colour: the marker's if it has a marker's pixel, else the lit colour if it has a lit
  pixel, else layer 0's, else layer 1's. Only the cells whose overlay differs from what is drawn
  are set, within the budget; what the budget cuts short is set by the next call (the grid is the
  record).
- Traces (layers 0 and 1) are joined to the column before and a pixel thicker below; the marker
  (layer 2) stands alone. A trace's samples are 2 columns wide (the kit's `0xC0` bit).
- ENVELOPE's time axis (decades of 48 pixels from 1 ms): the attack rises across its sample
  because a pixel is 5 % of the time at 8 seconds (400 ms), and a point alone could miss the peak.
  The gate is let go at the middle (about 0.25 s) or once sustain is reached and held half as long
  again, at `LAB_DRAW_GATE_MAX_X` at the latest so the release has room.
- ENVELOPE's curve uses the SID's rate periods (the datasheet's 2 ms to 8 s attacks; a decay or
  release from 255 to 0 is three times as long, 756 periods) and the envelope's exponential steps
  on the way down (reSID's exponential counter). A period's microseconds are 1.015 of its cycles at
  the PAL clock (985,248 Hz).
- OSC's carrier ratio to the modulator for ring or sync on a sound of four voices (the modulator is
  another key's): 384/256 = 1.5. A cent's tuning is 1/1731 of the cycle: `2^(c/1200)` within
  0.05 % at 50 cents.
- OSC's noise: a fixed scribble about the middle, a random level every 4 pixels joined (at full
  height the trace would ink every cell, past the grid's overlays; the hash: `k * 2654435761u`).
- The device's `memset` (USB chip's) goes a byte at a time, so `lab_draw_apply` clears a
  word-by-word loop's buffer with a word-by-word store.
- FILTER's response is calculated from the filter's transfer function with the damping the chip has
  (the resonance's 1/Q, `sid_resonance_k`) and the cutoff the chip has now (`cutoff_out` through
  its curve, `sid_cutoff_hz`). The 6581's modes lose more even open (its high-pass about 11 dB),
  and its cutoff stops at about 240 Hz.
- FX's distortion transfer curves use the overdrive's gain law (`core/fx/fx_overdrive.h`) and the
  clip's cubic; the modulation uses the LFO's rate from the A control (0.15 to 0.05 to 0.1 Hz
  minimum, 26 to 172 column-octaves); the delay's tap distance is log-scaled (10 ms to line's end).
- Undo holds `LAB_EDIT_STEPS` steps in `LAB_EDIT_ENTRIES` ring entries (the byte's place in the
  sound and its old value). A step never takes more entries than the image has bytes, so any one
  step fits. A run (a card or the choice turned again) adds to the newest step only the bytes it
  does not hold yet, so the step keeps the values from before the run's first change.
- Silent as LAB's rolls mean it: a held C-4 at the panel's velocity, 500 ms from its note-on (the
  chip's envelopes at 0 before it, as a played instrument has them), whose output on the chip
  alone, through a DC blocker (a first-order high-pass at about 35 Hz: the chip's DC level moves
  with the envelope, a thump that is not the note), has an RMS under −40 dBFS.
- `lab_audible` estimates in whole dB: the waveform's level on the model (none, noise with another
  waveform, the 6581's combinations that barely sound, the 8580's with the pulse when it falls away
  from 50 %), the pulse's narrowness (the PWM taking half its swing off, a side under 16 steps
  refused), the envelope (the attack's ramp, the decay to sustain; an arpeggio 3 dB more), SYNC
  LEAD's sync 6 dB (more below a ratio of 1, where the voice never ends a cycle), and the filter's
  loss at the cutoff's distance from the note (routed to no mode: refused; a narrow pulse under a
  low-pass near the note counts its fundamental alone; a combination or the ring 6 dB more). The
  resonance's gain, the chip's profile, the effect slots and the controllers are left out, each
  only louder or not the chip's. The estimate must reach `LAB_AUDIBLE_DB` (−38 dB). `lab_edit.rs`
  proves it against renders: no sound it passes is silent.
- Song bridge send: the waveform, ring and sync as a one-row wave program (`ctrl|gate 00`, then
  `FF 00`); AD and SR with GoatTracker's hard restart (gate timer 2, HR on; an attack of 0 is
  raised to 1, which does not click); the pulse as `$8H LL`, the PWM (its link baked) as a pulse
  loop; the filter (mode, resonance with RESO LINK baked at the cutoff, the cutoff's top 8 bits)
  routed to the song's current voice, the filter LFO as a filter loop; the vibrato as the
  instrument's delay and a calculated-mode speed row (50/(2S+4) Hz, a power-of-two depth); the
  arpeggio as the ARP helper's default chord. One-voice sync, the fine tune, the performance targets
  and the effect slots are dropped. Exact round trip: a trailer chunk per sent instrument (`LB` and
  the instrument in hex) keeps the 48-byte sound and a CRC-32 of the instrument's 25 bytes and its
  programs' rows; pull restores the sound exactly while the CRC matches.
- GoatTracker's optimisations (on by default) rest the pulse program on each row's gate-off tick
  and the vibrato on tick 0: one frame in six at tempo 6 (the default), which the pulse loop's and
  the vibrato's frames make up for (`RUNS(f)` macro: `(f) * 5 / 6`).
- LAB sound image: what a USER slot keeps on the store, what `live.base` holds, what LAB compares,
  undoes and rolls. Not in it: the chip's profile and character (they stay the instrument's, CHIP
  and USER 1-8), the model, the envelope mode (instrument-wide), the octave and the volume. A
  reader takes any sound of at least `LAB_SOUND_BYTES_1_0` bytes; the bytes it does not have are at
  their defaults (0).
- LAB composes as CHIP does: the text is built in the canvas when what it shows changed (a hash of
  it, `lab_view.look`), and the canvas's changed cells are copied onto the grid within
  `LAB_VIEW_FRAME_CELLS` a frame less what the screen's layers take; the drawing then takes what is
  left of the budget, drawn again when what it shows changed (`lab_view.art`) or until it is whole.
  When both changed (a card turned while a note sounds), a frame builds one of them by turns, so
  that no frame's `ui_poll` pays for both.
- EDIT + ARP varies: each card the page uses moves 1 to 3 detents either way (clamped) and, one
  time in four (always when it uses no card: an effect slot that is off), the choice a step (not
  ENVELOPE's, the instrument's mode). EDIT + SCL rolls the voice (`live_dice_shape`: the waveform,
  the envelope, the filter, the pulse, the vibrato's depth; the slots, the arpeggio, the PWM and
  the tune left), the sound's base and name kept. A result is never silent (`lab_audible` on the
  chip's model) and never the sound before it or the last result (`lab_view.rolled`): such a result
  is tried again (`LAB_VIEW_VARY_TRIES` times for a variation, `LAB_VIEW_ROLL_TRIES` for a roll,
  and then the smallest move that passes); when none passes the sound stays and the toast says
  `NOTHING AUDIBLE`.
- OSC's sync base at most 4095 (16 times the pitch): the ratio's product stays in 32 bits.

### LEARN

- LEARN's frame (rows 20-25) is drawn over the view underneath (which keeps showing the
  instrument). It takes the panel's events first (`screen_capture`): K1-K4 pick the knob's
  parameter instead of turning it; everything else (the note keys, SCL, OCT, FX, on CHIP SELECT and
  ALGORITHM) goes on as usual. The frame's compose calls the view underneath first, then draws the
  frame over it; it covers its cells (`screen_cover`) so the view underneath that holds to a budget
  (LIVE, SCENE) leaves them alone and counts what the frame sends this frame. It clears the
  overlays the view underneath draws on its cells; the others stay, there and back (`SCREEN_OVER`).
- The frame's input passes SELECT and ALGORITHM on to CHIP underneath only: the menu's ALGORITHM
  would open a view over the frame, SCENE's SELECT would wipe in a piece under it.

## LIVE

### Register sink protocol

The live instrument never touches a sid directly. Every register change goes through a sink
function, called once per write in the order the writes must happen. On the host the sink applies
them to a sid at once; on the device the sink pushes them onto a ring that the audio ISR drains,
so the chip changes only between two rendered blocks.

Besides SID registers 0x00..0x18 a sink receives pseudo-registers, outside the SID's 5-bit
address space so sinks can catch them: LIVE_REG_RESET (0x80, a sid_model: re-initialise the chip
with that model); LIVE_REG_GATE_RESET (0x81, 0 or 1: the envelope mode, which sid_init turns
off); LIVE_REG_CHAR(field) (0x82..0x86: the character's profile and four controls, 0..127 each;
a sink keeps them and applies them to every chip a later reset makes, and ramps the playing chip
towards them at each LIVE_REG_CHAR_TICK); LIVE_REG_CHAR_TICK (0x87, value 0, once every
live_tick).

A model or profile change with a preparer waits for the chip the reset will swap in: live_tick
calls the preparer once a tick, a slice of the work each, and sends the reset once it says the
chip is whole; without one the reset goes at the next tick.

### Pitch offsets and frequency calculation

Pitch offsets are in 1/8192 semitone (Q13): a 14-bit pitch bend times the range in semitones is
exact at every range. live_freq_at returns the frequency register for a note moved by an offset
(the table's value at offset 0, otherwise within 0.6 of the exact value, rising with the offset
and by at most 1 for each 1/8192 semitone). Notes above 106 do not fit 16 bits and are folded
down by whole octaves.

### K1's cutoff walk

K1's walk (live_cutoff_steps): per chip model (6581, 8580), 65 registers rising strictly in
register and in Hz, from the first one a semitone above the chip's lowest cutoff to the highest,
spaced evenly in pitch. live_cutoff_up and live_cutoff_down: per bucket of 32 registers, the
first entry above every register of the bucket and the last entry below it, so a cutoff the table
does not hold (MIDI, a preset, the model changed) starts from just before the first entry beyond
it in the turn's direction.

### The chord arpeggio

With the arpeggio on, the notes held (keys' and MIDI's) are one voice's, which steps through
them, a note every LIVE_ARP_RATES frames (50 Hz, a C64 player's), its gate held from the first
note to the release of the last: only the frequency moves, at a tick, so the chip changes pitch
between two rendered blocks with no new attack. The other voices stay free. A fresh preset's rate
(load_preset) is 3F, three frames a note: at 1F a held chord sounds like a wobble; at 3F each
note is heard. Rates from LIVE_ARP_RATE_CLOCK count MIDI clocks while they come (a 32nd, a 16th,
an 8th), else as many frames.

### Velocity targets

The SID has no per-voice VCA: these stand in for one. A velocity of 127 sounds as OFF does; lower
ones lower the target (the sustain level scaled, the filter 8 steps lower per velocity below 127,
the pulse width narrower).

### Envelope modes

LIVE_ENVELOPE_INSTANT (the default): the rate counter restarts at each gate edge, so the attack
at a note-on and the release at a note-off take their first step within one rate period and 3
chip cycles, whatever the rate before them. An instrument played live wants every note, and every
note's end, on time. LIVE_ENVELOPE_AUTHENTIC: the chip's ADSR delay bug, as reSIDfp has it: after
a slower rate most notes and note-offs wait up to 33 ms for the rate counter to come round.

### PWM and filter LFO

The pulse-width LFO's depth narrows near the ends of the range, so that its sweep never goes
thinner than K3's narrowest width (LIVE_PULSE_FIRST), nor as thin at the wide end: clamped to 0,
a width is no pulse but a DC level, and the voice went silent for part of every cycle (PWM BASS
at a 9 % width, a quarter of its 2 s cycle). One LFO: the PWM's and the filter's, at the PWM's
rate.

### Macro links

The macro links (RESO LINK, PWM LINK): the resonance written is the parameter plus reso_link ×
(32 − the place of the cutoff written on K1's walk) / 8, clamped to 0..15 (positive opens it as
the cutoff closes; negative, the squelch, as it opens); the PWM's depth written is the parameter
plus pwm_link × the width's distance from 2048 / 8, clamped to 0..LIVE_PWM_DEPTH_MAX. They act
wherever the cutoff or the width move (K1, K3, LAB, CC 74, the wheel, the filter LFO).

### SYNC LEAD (mono_sync)

SYNC LEAD's one-voice sync: one note at a time on voice 1, hard-synced to voice 3, which runs
silently at the note's pitch while voice 1 runs sync_ratio_q8 / 256 times higher. The sync lead's
voices 1 and 3 take the same pitch, so the ratio holds through a bend and the vibrato. A known
limit: at 2.75 times the pitch the register is full from note 90 (F#6) at rest, from lower notes
bent or vibrato'd up (88 at +2 semitones). Voice 3 still sets the pitch; the ratio, and the
timbre, shrink toward the top. Dropping voice 1 an octave instead would jump the timbre mid-bend,
so it saturates.

### Voice allocation

A voice for a new note: a free one (the longest released), the UI sound's when the others are all
held (a UI sound gives its voice back, with the instrument's envelope), else the one held longest.

### Expression scales

The cutoff moves 8 steps per controller value (0..1016), the pulse width 16 (0..2032); a vibrato
depth of 127 is 127 × 64 = 8128 / 8192 semitone.

### DICE randomisation

DICE's roll uses a xorshift32 generator (the state never 0), the seed mixed with the roll's
number. The waveform's distribution: 3 of 8 saw, 2 pulse, 2 triangle, 1 noise. Ring on a triangle
and sync on a saw or pulse are drawn whether or not the choice applies: one number a choice. The
filter LFO is off.

### Knob walks

Knob walks (live_walk.c, docs/reference/controls.md "Knob curves"): the values a knob lands on,
rising strictly, each one detent from the next, so a parameter whose ear is logarithmic moves by
even steps of what is heard. They choose the values only: a stored value keeps its meaning, and a
value off the walk (a preset, MIDI, a sound saved before) is kept until a turn, whose first
detent lands on the first entry beyond it the turn's way (none that way: it stays). PWM Rate: 32
positions, geometric 0.05..12.75 Hz, in phase steps. PWM Depth: 32, square law 0..LIVE_PWM_DEPTH_MAX.
Vibrato Rate: 24, geometric 0.5..12.7 Hz. Vibrato Depth: 32: 0, then geometric 6..127
(±4.7..±99 cents). Vibrato Delay: 24: 0, then quasi-log 40 ms..1.27 s.

The effect slots' shapes (32 over 0..127): LIN even, 4 a detent (5 mid-way, clear of the steeper
ends); LOG 0, then geometric 5..127 (+1 steps where geometric is finer); EQP equal power, 127
sin(π/2 i/31), dense at the top (+1 at the top where finer). A slot's control's shape is chosen
per algorithm and field by its rendered response (host knob_curves.rs).

A walk's CC position and a position's CC without a division (MIDI out sends a CC a parameter,
emu-gate midi.rs bounds a tick): round(cc (n − 1) / 127) = (cc POS + 2048) >> 12 and round(i 127
/ (n − 1)) = (i CC + 128) >> 8, exact over 0..127 and 0..n − 1.

### MASTER

The MASTER pot's volume (0..1023) is a digital gain on the final stereo output
(platform/fm1/audio.c): after the chips, the effect slots and chip B's mix, so it scales
everything the instrument plays (the keys, a song on either chip, UI sounds) and the chips' own
volume registers keep their musical meaning (a song's fades and its 6581 volume digi trick play
relative to MASTER). The live instrument's volume register is always 15.

The pot's 0..1023 maps through an audio taper, −60 dB to 0 dB in even steps of dB (a pot's travel
feels even), so full is exactly unity (MASTER_UNITY, 1.0) and the stage does nothing at all there
(the output is the chips' bit for bit); 0 is silence. A change ramps linearly over the block it
lands in, so the pot never zippers.

### Tail gate

With every envelope at 0 and its gate off a SID chip still puts out the waveform's leak through
the voice DACs' envelope zero: a faint tone, about −50 dBFS, which never decays. The chips are
left exactly as they are; this stage mutes the block they produce instead. Once the chip (or both,
with chip B rendering) has had every envelope at 0 with its gate off, and a filter bleed of 0,
for TAIL_HOLD_BLOCKS blocks (16, 2048 samples, about 46 ms), the next block fades to 0 linearly
(TAIL_BLOCK samples) and the output stays exactly 0 until a voice gates on or a bleed is set. A
bleed keeps the leak: it is a deliberate effect of the character. The hold is longer than the
gaps between the notes of a phrase, so they play untouched.

### Output thump

The character's thump is a step at each note's start and volume write that the chip's DC blocker
turns into a decaying bump. The chips leave it out of their render; here each chip's steps are
taken (at its output gain) and played as its blocker would have played them, a step that decays
by the chip's dc_r a sample. Played last (after the chips, chip B's mix and the effect slots,
before MASTER), the thump is fitted into the room the output leaves: a sample of the stereo
output s gets the thump x where |s + x| stays within THUMP_OUT_CEILING (one inside the rails) with
room to spare, and the part of x beyond half the room left is softly limited (x′ = h + d h / (d +
h), h half the room, d the excess), so the output nears the ceiling but never reaches it. A
signal without a thump passes bit for bit, a sample already on a rail stays as it is, and no
thump clips: not on two chips at once, not after an effect's gain, at no setting. The thump does
not pass through the slots (an echo or a reverb does not repeat it, a distortion does not shape
it): it is a low bump under the note's start, not part of the voice. Nothing runs while no thump
sounds (the stage is idle once a bump has decayed under half an LSB, which no longer changes a
sample).

### Effect slots protocol

live keeps each slot's settings (algorithm, Amount, A, B, Mix; the controls 0..127 as MIDI) and
sends every change down its sink as a pseudo-register, LIVE_REG_SLOT(slot, field), value
(0xA0..0xB4: the base 0xA0 has no bit in slot << 3 | field, and bit 7 marks a pseudo-register).
On the device that is the register ring, so a slot change reaches the audio interrupt in order
with the SID writes of the same pass and is applied at the start of a block, like them. The slots
are one chain on the mix of both chips, so their pseudo-registers name no chip: a sink bound to
chip B sends them as they are. On the host fx_live_sid_sink applies everything at once.

Each slot's defaults (Amount, A, B, Mix): what an algorithm picked from the panel starts from,
each one audible at once. NONE's row is zeros.

### LIVE view

LIVE composes every frame (no changed function), within a budget of LIVE_VIEW_FRAME_CELLS cells
(200; about 17.5 ms of SPI, inside the 20 ms budget): the scopes get what the rest of the frame
and the screen's toast, raster and cover (MIDI learn's frame over it) leave, and a trace that
does not fit waits for a later frame (three voices of noise take turns). The canvas (small text
and bars) is built whole once; after that each part (the header, a voice's lane, the knob row,
the bottom band) is drawn again only when what it shows changed, and a meter only its own cells:
a frame of moving meters costs a few cells, not the 900 of the canvas. The large text is drawn on
the grid itself (grid_text_big) for the rows that were copied. While a wipe or a raster runs it
draws what it drew last, as About does: the transitions are sized for a still view.

The scopes come from the audio interrupt's tap: the newest points, folded by the oscillator's
phase (scope_trace), `cycles` periods across the width, more for higher notes; a hard-synced
voice is folded by its sync source's phase, so the sync shape shows. The trace's height is the
waveform times the voice's envelope: flat at rest. The trace is 160 × 32 pixels, 2 pixels thick,
an overlay on each cell it crosses, over a dotted centre line, in the voice's colour (dark grey
and flat when the voice is silent).

The overlays are taken to be the drawn traces (lv->y, in lv->color) while the grid's pool has as
many free slots as LIVE left it: only LIVE uses overlays while it shows, and a view switch frees
them all. When they are not, every voice is marked to be drawn again whole. Fresh traces (pend)
take the cells the frame has left once everything else is composed and what it sends and what the
screen's layers and the cover add are set aside; voices to draw again go one a frame, top first,
which keeps ahead of a wipe (it reveals voice 1's scope in its first frame, voice 2's in its
second, voice 3's in its fourth). A trace no pixel of which moved by more than one is the last
one wobbling (which points the fold got): it stays as drawn, so a note that holds still sends
nothing. A voice's level: its waveform's peak times its envelope. The meters: up at once; down at
most LIVE_VIEW_FALL a frame, and not for a step or two (a steady note's peaks wobble), so a held
note's meter holds still.

### USER chip profiles

The USER chip profiles (eight slots, USER 1..8): each a built-in base profile, the
character's four controls and expert overrides. A profile is a chip's setup as a player left it.
The slots themselves live in `live` (user_valid, user_base, user_ctl, user_expert: what the
picker, CC 85 and the CHIP view read, no store needed); user_profiles reads them from the store
at boot and writes a slot the player saved. Each slot is its own store key, STORE_KEY_PROFILE(n),
so the settings record does not grow with the saved slots.

Schema 1 (USER_PROFILE_V1_BYTES): version, base profile, control count (4), controls. Schema 2
(USER_PROFILE_V2_BYTES) appends the expert count and raw overrides (255 follows the
profile/macros). A later schema's extra bytes are ignored.

user_profile_save puts the slot into live at once and queues the write; the views' frame ticks it
until the store takes the record, then until it commits, and says so (the result). A store that
cannot take it now (STORE_BUSY) is asked again on later ticks; after USER_PROFILE_FORCE_MS (2 s)
of waiting the platform's forced held operations are asked for, as for an explicit settings save,
and the result says GAPS. A foreign store keeps nothing: the slot stays in live until the next
boot.

### USER sounds

LAB's USER sounds (sixteen slots, USER 1..16): each a sound (app/lab_sound.h) and its name, in
one private store file (STORE_SOUNDS_NAME, type STORE_FILE_SOUNDS); and the unsaved edit, one
small record (USER_SOUND_EDIT_KEY) the settings' debounce writes before the settings record and
settings_load applies after it. The slots are not cached in RAM: live.sounds_valid says which
hold a sound (user_sounds_load, at boot), and a slot is read from the store when it is loaded.

The file: [version][sound bytes][slots], then per slot [used][name][sound]. A keep rewrites the
file whole, streamed through the store's own staging buffer: the other slots from the file as it
is, the kept one from a copy here, so a write costs one slot's bytes of RAM. The file is the old
one until the new one commits (a power cut leaves either), and a later version's file is read for
this version's bytes of each slot (an older file's slot has fewer bytes: the rest are their
defaults, 0).

user_sound_save sets the slot in live at once (its bit, and the sound as the current sound's
base: a keep is of the sound playing) and queues the write; the views' frame ticks it while it
has work, until the file commits, and says so once (the result). One write at a time: a keep
while one is written is refused (try again).

The unsaved edit (key 0x109, type 4): [version][preset][edited][name][sound bytes][sound]. The
preset byte is the settings record's: 0..LIVE_PRESET_SLOTS − 1 a built-in or DICE,
LIVE_PRESET_SLOTS + n USER n + 1.

## Boot, help, about and the scope

**Scope (LIVE's audio tap).** The ring is a history, not a queue: single producer (audio
interrupt), single consumer (main loop), one core; the producer never waits or looks at the
consumer, overwrites the oldest point, and publishes the head once a block (after the points);
the consumer takes the newest points it wants and drops what it overtook. Drawn phase-folded (an
equivalent-time display): each waveform point lands by its oscillator's phase, not its time, so
the trace stays still and shows a waveform whole at any pitch. Points are decimated (every 16
samples: 2757 points a second, 110 a frame) to keep far fewer than a time-base display would
need. The ring's size is a power of two for masking. One render a block keeps a render's
per-call setup out of the tap's cost. The tap runs only while the consumer asks for it (`on`):
nothing is added to the interrupt while LIVE is not showing. Compiler fences order memory
accesses (the points before the head, the head before the points, the copy before the head that
says whether it held); the pi32v2 compiler has no `__atomic_signal_fence`. Peak meter: 6 dB per
8 steps (8 log₂(amplitude); 64 and below empty, 32768 full).

**Help.** Drawn in units (the title bar, then each line), sized so no frame sends more than 200
cells (HELP_FRAME_CELLS) whatever it covers. A card takes at most two pages, eight lines each;
what a line does can be the state's own words (a first byte HELP_TEXT_K1..K4 the name of that
knob's control of the slot in `arg`, HELP_TEXT_QUANT the quantise in `arg`). Lines fit 14
characters, the control and what it does.

**About.** Frame-by-frame cost planning: the logo is still (a bitmap sent once, under the
wipe); the bars are five bands of 4 pixels, one moves a band on each odd frame, so each moves
at most every 4 frames; the credits fade on even frames, two text rows at most at a time; the
scroller moves 2 pixels a frame, each letter a white overlay over the bars' cells (no bar has
white in it), bobbing a whole cell row at a time on a sine; the stars are one-pixel overlays in
three layers, moving a pixel every 4, 2 or 1 frames. It composes every frame and holds still
while a wipe or a raster runs (screen_animating): the wipe sends its own rows and nothing else.
The credits pages cascade: each row appears ABOUT_ROW_DELAY frames (4) after the one above and
is shown ABOUT_ROW_FRAMES frames (86), fading through 4 levels a level every other frame (so at
most two rows change in a frame, all on even frames). The bars' budget: moving them recolours
the border's 116 cells, about 10 ms of SPI, so they change every other frame at most
(BOOT_STRIPE_MS = 80 ms). Silent mode (unsaved changes) selects different scroller text.
Credits are generated from CREDITS.md by a host test (with SIDOU_CREDITS_UPDATE=1): a page per
section, each line wrapped to CREDITS_COLS characters (26, inside the frame and its margin) in
upper case and the screen font's ASCII (accented letters lose their accents); a credit longer
than a page starts one and goes on under "<SECTION> (CONT.)", and nothing shares a page with its
tail.

**Logo (pluggable asset).** A logo is a table in flash: its footprint, cost, a draw function
for one frame of its animation into the footprint, and optionally a full-screen scene. Draw
sets every cell of the footprint (glyph, colours, overlay, pixel region), so the caller
composes it like any other part, every frame or once. Its animation runs on the caller's frame
count (25 fps); frame 0 is the still. Its background is its own: no transparent one. The grid
has one pixel region and it is the logo's: a screen shows one bitmap logo at a time, and a view
switch clears it. Pulse Train is drawn in code with font glyphs and overlays; it changes only
on odd frames (the stripes on frames 2, 6, 10, ..., the pulse on 4, 8, ...). The chrome logos
(Metal Chrome and Cracktro Chrome) are bitmaps (generated from assets/logo/*.png): their draw
hands the footprint to the renderer's pixel region, which shows the bitmap's cells pixel for
pixel; the cells are blank (white on the background, no overlay) so the bitmap shows alone, and
what the caller writes in them afterwards draws over it. Still: no overlays, and no cell
changes after the first frame.

**Title card.** Sequence: the glow fades in through ordered-dither levels (a purple and red
glow drawn a cell at a time with the font's dither glyphs in two neighbouring stops of black,
blue, purple and red), each level holding TITLE_CARD_LEVEL_HOLD frames (2, the page's preview)
once all its cells are out; the title wipes in from the left, TITLE_CARD_WIPE_COLS columns (3)
a frame, the bitmap growing over the same bitmap cells, so each frame sends the cells it adds
and no others; it holds TITLE_CARD_HOLD_FRAMES frames (25, 1 s); it ends. Budget: every frame
sends at most TITLE_CARD_FRAME_CELLS cells (200, about 17.5 ms of SPI, inside the 20 ms
budget). A step that changes more goes out in bands, top first, from where the last frame
stopped, until every cell of it is out. Ends on: a button or an encoder (swallows the event
and does nothing else), or a note from the keys or MIDI (which also plays).

**Boot.** Two sequences: the full one (after an install, or HOME held at power-on) is the
banner and READY., LOAD"SIDOU",8,1 typed, SEARCHING FOR SIDOU, LOADING with raster stripes in
the border, READY. and RUN, then the title card (the logo's scene or a slot with ramp lines);
the short one (the default) is the title card alone. HOME held at power-on: the first HOME
press edge reported by the key scan (after the debounce, about 3 ms) is part of the hold, not
a skip. The boot never holds the instrument up: audio, the tick, MIDI and the interrupts start
before the screen's first frame, and the main loop plays the keys and MIDI while it runs.
Anything ends it: a button or encoder (swallows the event), and a note from the keys or MIDI
(which also plays). The loading stripes: xorshift32 for repeatability (the same step gives the
same stripes), each cell row of the border two colours split at a random height (a lower
eighths glyph); 116 border cells, about 10 ms of SPI, change every other frame at most
(BOOT_STRIPE_MS = 80 ms). The logo holds still while the wipe into the card runs (the wipe is
sized for a view that does not move).

**Boot guard.** Firmware that crashes or hangs early in every boot ends in UBOOT after
BOOTGUARD_FAILED_MAX failed boots (2), which still accepts updates. The state lives in .noinit
RAM: a reset keeps it, power-on fills it with noise. A magic word and a check word (magic ^
pending ^ failed ^ BOOTGUARD_CHECK) seal the guard: a guard whose check does not match is taken
for power-on noise or another firmware's, so only counts SIDOU wrote send a boot to UBOOT. Each
boot that finds the previous one still pending counts it as failed (a crash, a watchdog reset
or a power cut before it was stable); a boot is stable once the main loop has run
BOOTGUARD_STABLE_MS (10 s), and the count is cleared. An intentional reset (an update's commit)
is not a failure (bootguard_leave clears `pending`).

**UI sounds (chip B's spare voice).** Each sound is data in GoatTracker's sound-effect format
(readme section 6.3): attack/decay, sustain/release, a pulse byte, then the frames: $01-$81 a
waveform kept until the next, $82-$DF a note (D-0 to B-7, which every frame has), $00 the end.
Stepped at 50 Hz (every other live_tick). A sound never steals: it plays on a voice of chip B
that is free. The keys play chip A: chip B is all free (voice 3, chip B's volume up, no
filter). The keys play chip B (a song on chip A): a free voice is one whose gate is off and
whose release has run out (live's voice state and the release time), the newest of them (so not
the one live's allocator takes next); none under mono sync (voices 1 and 3). While a sound
holds the voice, live's allocator skips it (`live.ui_voice`, the voice number plus one) and its
writes to the voice (and its $D417 filter bit) are held back by a filter on live's sink; a note
that finds the other two voices busy takes the voice back at once, and the sound stops. When
the sound and its release are done, the voice gets live's registers back (live_rewrite_voice).
A sound stops, writing nothing more, if the keys move between chips while it plays.

**SAVE gesture.** SAVE pressed and released with no knob turned and no other button pressed
while it was down is a save (save_key_buttons returns SAVE_KEY_SAVE at the release); a knob
turned or another button pressed while SAVE is down spoils that save (save_key_spoil), and the
release saves nothing. SAVE's press never reaches the views or the panel (the views read
save_key's held bit directly for cycle savings while routing).

**Sine (screen animations).** Integer only, from a quarter-wave table in flash (65 entries,
sin(2π i / 256) × 127 for i = 0..64); symmetry gives the full turn.

**Bitmap codec.** Run dictionary and checkpointed row packets (documented in
docs/art/bitmap-codec.md). Nibble cursors for decoding. Raw vs coded modes (bit 128 of byte
15): raw mode uses byte runs and cumulative row offsets; coded mode uses a dictionary and delta
offsets. The logos' scenes and the title cards are coded bitmaps: Metal Chrome 9730 bytes (vs
28800 unpacked), Cracktro Chrome 3971 bytes, title cards 19754 bytes (17729 coded data, 1440
row offsets, 360 cell bits, 225 glow levels). Generated by host tests from assets/logo/*.png.

## MIDI, SysEx and link

**The MIDI clock follower** estimates tempo and phase with a least-squares line through the last
96 clocks (four beats, `MIDI_CLOCK_WINDOW`): stamp = a + b × clock number. A fit, not a
second-order loop (PLL), because the fit's slope is a weighted average of the intervals, so it
never overshoots a tempo step or ramp where a PLL rings, it locks in known time with no state to
settle (exact for a steady clock however few clocks), and for the input's bounded white noise (TRS
poll's 0.5 ms, USB's 1 ms frames) a line's least-squares fit is the best unbiased estimate. Four
beats, not two: the bounds (0.1 % tempo, 1 ms phase) must hold from two beats after the clock
starts, when 48 clocks are fitted, and 48-clock fit is at its limit; clocks in USB's 1 ms frames
give up to 0.085 % tempo error at 200 BPM. Grown to 96, the error halves once four beats have run.
The cost: the fit lags a ramp by about two beats (the window's middle), settles a step within four,
and its ring takes 768 bytes. The x axis is the clock's number since start, so a clock left out
(a hold's) leaves a hole rather than shifting the clocks after it.

**Holds** mask flash erases (up to 35 ms): TRS bytes that arrive meanwhile are stamped late,
together, at the first poll after. The caller reports each hold (`midi_clock_hold`) before feeding
the messages it stamped late; clocks stamped from the hold's start to one clock period after its
end count for the song position but stay out of the fit. A clock more than 300 ms late is lost: the
fit is dropped, the next clock starts a new one.

**MIDI sync timing.** Every call lands `MIDI_SYNC_DELAY` samples (352, 8 ms) after its clock
reached the jack: the block being heard and the next (already rendered when the command is sent),
plus the stamp's age (the poll, the drain, up to 96 samples). A constant a sequencer can compensate
for. The stamps are fm1_ticks (24 MHz); the player counts samples. `midi_sync_anchor` ties the two
once a block: the block that starts at sample `samples` was rendered at tick `ticks`. The player's
samples stop while a flash hold masks the interrupts and the ticks do not, so the clock's fit is
kept on the ticks' time line and its times turned into the player's samples by the latest anchor.
The commands and clock out are in the player's samples.

**MIDI parsing** is a state machine, one byte at a time. Running status: channel messages may omit
their status. Real-time bytes (F8..FF) are reported where they occur, even inside another message
or a SysEx, and change no state. SysEx (F0 .. F7) is passed byte by byte to the callback:
`MIDI_SYSEX_START`, `MIDI_SYSEX_DATA` for each, then `MIDI_SYSEX_END` or `MIDI_SYSEX_ABORT` (the
status byte that cut it short; an F0 inside aborts it and starts a new one). Real-time inside it is
not part of it. System common (F1..F7) cancels running status. An incomplete message is dropped
when a new status byte arrives. Note-on with velocity 0 is reported as note-off.

**One parser per source** (TRS, USB), because sources interleave: a TRS message arrives a byte per
320 µs, so USB messages come between its bytes, and a parser shared by both would take a USB status
byte as the end of the TRS message. USB packets go through `midi_usb_bytes` first (the 32-bit word
read little-endian: CIN in bits 0..3, cable in bits 4..7, MIDI_0..MIDI_2 in bits 8..31; the CIN
gives the byte count), so both sources share the one parser implementation.

**The MIDI input ring** moves entries from the TIMER5 interrupt (polls USB and TRS UART at 2 kHz)
to the main loop. 256 entries: 256 TRS bytes are 82 ms at 31250 baud. Single producer, single
consumer, one core: the producer is an interrupt that preempts the consumer. Each index is written
by one side only; compiler fences keep an entry's stores before the index store that publishes it,
and its loads before the index store that hands its slot back. A full ring drops the new entry and
counts it in `lost`; so does the producer for input it lost before the ring
(`midi_ring_lose`). Each run of losses with no successful push between them is one gap and starts a
new `epoch`; every entry carries the epoch it was pushed in. The consumer sees exactly where input
went missing: between two entries whose epochs differ (by 1: a gap needs a push to end it), or
after the last entry if the ring's epoch is ahead of it. A gap also says whose input it lost (the
sources' bits: `in_gap` until the entry after it carries them), so a SysEx transfer on the other
port goes on. The USB side never loses: it leaves a packet in the endpoint (the host is NAKed and
retries) while the ring has less room than a packet's events.

**Lost input:** the parsers of the sources it lost restart, every note is released and the
controllers reset (bend centred, wheel and pressure 0) exactly where the gap is, so what came
before it is taken whole, what was cut by it is dropped (a message is never completed with bytes
from after the gap), and a note-off or a bend back to the centre that may be among the lost cannot
leave a note hanging or bent. The other source's parser goes on, so a SysEx frame on it is not cut
by the gap.

**MIDI out** is diff-based: `midi_out` keeps every parameter's value as it last saw it
(`live_param_get`) and compares. `midi_out_local` takes the changes since the last look as the
panel's (anything but MIDI input): they are pending. `midi_out_remote` takes them as MIDI input's:
the sender has the value already, so they are never echoed, on whichever port they came in (TRS or
USB), and a pending panel change of the same parameter is dropped (the last change was MIDI's). The
caller looks locally before taking MIDI input and remotely right after it, once for a whole pass of
input when nothing else changes a parameter meanwhile. `midi_out_tick`, once per 10 ms tick, takes
a local look, then sends what is pending with each parameter's value at that moment: at most one
message per parameter per tick, the latest value winning, at most `MIDI_OUT_TICK_PACKETS` (16)
packets a tick, round robin over the parameters so none starves; the mod target first, since it
says what CC 1 is (the pulse width or the wheel), and nothing before it while it waits for room.
What does not fit (the budget, or the room the caller has) stays pending, one flag per parameter:
nothing queues beyond that. Cold expert metadata or restoration halves the tick's packet budget.

**Encoding.** A parameter's CC, value `midi_map_to_cc`. A parameter of more than 128 values
(cutoff, pulse width, volume) goes as its NRPN instead, at full resolution, when the high-resolution
option is on. A parameter with no CC (the bend range, the expression targets) always goes as its
NRPN, and so does the pulse width while the mod wheel drives another target (CC 1 is then the
wheel's). An NRPN is four CCs: 99 the MSB and 98 the LSB of its number, 6 the value's MSB and 38
its LSB (value >> 7, value & 127), the order `midi_map_cc` applies at full resolution and, for a
small parameter, at the LSB. No null NRPN follows.

**MIDI learn** binds CCs to parameters at run time, over the published map, added to it (the
parameter's own CC still works); a learned CC wins (if it was another parameter's CC, it now drives
the learned parameter only); one CC per parameter and one parameter per CC (learning a parameter
again moves it to the new CC, learning a bound CC for another parameter moves the CC); the value
goes through the parameter's curve and range from its map row. Learnable CCs are 0..119 but bank
select (0, 32), data entry, increment, decrement, NRPN and RPN (6, 38, 96..101): `midi_map_cc`
takes those before any binding. Every lookup is O(1): a CC's binding is one byte of `cc_row` (its
map row), a parameter's one byte of `param_cc`. The bindings are omni (a learned CC means the same
on every channel and on TRS and USB). An armed learn with no step for 10 seconds is cancelled.

**NRPN data entry** (CC 6, 38, 96, 97) applies a parameter selected by CC 99/98 (or RPN 101/100).
For a parameter of more than 128 values (max >= 128) CC 6 applies the MSB at once (the LSB taken as
0) and CC 38 refines it. For a smaller one (max < 128: the value is all in the LSB) CC 6 only
stores the MSB and CC 38 applies the value, so a sender's 6 = 0, 38 = v never passes through 0 (a
chip model reset, a silent waveform, a cut envelope). CC 6 alone does not change such a parameter:
send CC 38, or the plain CC. `midi_map_cc` keeps the selection (CC 99/98, RPN 101/100) and data
entry state (CC 6/38, increment 96, decrement 97) per channel, for one input (a `midi_map_port`
each). The NRPN/RPN state is per channel and per source (TRS and USB separate).

**The published map** has one table row per parameter, with its CC (7-bit) and NRPN (14-bit),
range, curve and default. A CC sets the parameter through its curve: `LINEAR` spans the range,
value × max / 127 rounded (needs max >= 127); `STEPS` divides 0..127 evenly among the values,
value × (max + 1) / 128 (needs max < 128); `WALK` lands on the positions of the knob's walk, so the
CC moves as the knob does and 0..127 reaches every position. An NRPN carries the parameter's own
value, MSB × 128 + LSB, clamped to its range: the full 11 bits of the cutoff and 12 of the pulse
width.

**MIDI's modes** route input channels. OMNI (the default): every channel plays the instrument, on
the chip the keys play. LIVE: channel N alone plays the instrument; the others are ignored.
MULTITIMBRAL: channels N, N+1, N+2 play chip A's voices 1, 2, 3, each a jam voice of the player
with its own GoatTracker instrument (program change n is instrument n + 1), its own pitch bend (the
range per channel, RPN 0) and its own notes: one at a time, the last one held sounding (a release
goes back to the note held before it), with the LEGATO option a change of note while one is held
glides on without a new attack. Channel N+3 plays the instrument, on chip B. WHOLE-CHIP A: channel
N plays all three voices of chip A with one GoatTracker instrument (channel N's program): three-
voice polyphony, a new note taking the voice whose note started longest ago when all three sound.
Options: CHORD (one key plays the stored chord) and ARP (the held notes cycled on voice 1 in the
order they were pressed, a step every `arp_rate` player frames, or every 16th of the MIDI clock
with `arp_rate` 0, so voices 2 and 3 stay the song's). Channel N+3 plays the instrument, on chip B.
WHOLE-CHIP B: channel N plays the instrument on chip B (all three of its voices); chip A is left to
the song. In MULTITIMBRAL and both WHOLE-CHIP modes chip A belongs to the player, song or not. On
chip A's channels (the jam channels): notes 12..107 (GoatTracker's C-0..B-7) play; others are
ignored. Velocity is not used (GoatTracker's instruments have none). Pitch bend: ±the channel's
range (2 semitones, RPN 0 sets it per channel), applied at the next tick, coalesced. CC 74
(cutoff) and CC 71 (resonance) lock chip A's filter to the value, over the song's filter writes,
until CC 121 or a change of mode. Program change n (0..62) picks the channel's instrument n + 1;
others are ignored.

**Program change and bank select** (CC 0 MSB, CC 32 LSB) select a channel's bank, MSB × 128 + LSB,
held until changed (from either port; kept across lost input); a program change means what the
channel's bank says. Bank 0, the channel's sound: on the instrument's channel the instrument's
preset (0..LIVE_PRESET_COUNT − 1), on a chip A channel (MULTITIMBRAL, WHOLE-CHIP A) GoatTracker
instrument n + 1. Bank 1, songs: program n asks for the store's n-th song in name order, loaded
stopped, on the instrument's channel or channel N only. Bank 2, the chip's profiles: 0..4 the
built-in ones, on the instrument's channel live's profile (the keys' setup), on a chip A channel
chip A's (its want path); 5..12 USER 1..8 (the saved base and controls; an empty slot does
nothing). CC 0 and 32 are bank select on every channel the mode routes: MIDI learn leaves them
alone.

**The Link protocol** carries the screen to a Mac client over a SysEx stream (USB only, not
TRS) at 22 bytes per ms, burst 96. Commands are 7-bit encoded: a mask byte (high bit of the next
7 data bytes, bits 0..6) followed by the 7 data bytes with their high bits cleared, repeated; a
checksum (the two's complement of the sum, masked to 7 bits) follows. Frame: F0, magic (7D 'FM1L'),
command, payload (7-bit encoded), checksum, F7. Commands from the client: START (opens the link,
the device responds with HELLO, then sends the font, palette and screen), HEARTBEAT (the client
sends every 600 ms, the device responds with BEAT), STOP, KEYFRAME (the device resends everything),
FONT (the device sends the glyphs), PALETTE (the device sends the colours), PANEL (the client's
held keys and encoder steps). From the device: HELLO (the device's name, the font and palette
CRCs), BEAT, STOPPED, COLOURS (the 16 palette entries, 2 bytes each), GLYPHS (up to 5 glyphs, 8
bytes each), CELLS (a run of character cells: row, column, then records), PIXELS (a pixel cell: row,
column, record, then 32 bytes of 4-bit pixels). LINK_TIMEOUT_MS (3000) without a message from the
client closes the link. Panel keys held for LINK_RELEASE_MS (500) without a PANEL message are
released.

**SysEx protocol** (0x7D 'SD'): INQ (inquiry: the device replies with version, format, free space),
DIR (directory: the device sends a DIR message per file, then END), GET (the device sends the
file's metadata, then DATA messages of the file, then END), PUT (the host sends metadata, the
device acknowledges, the host sends DATA messages, then END; the device acknowledges each and
commits at END), DEL (the host names a file to delete), ABORT. Replies: ACK, NAK (with error
code), STILL (the device is busy, sent every 2 seconds during a long operation). A message times
out after 2 seconds. The frame: F0, 7D 'SD', command, sequence (0..127, wraps), payload (7-bit
encoded as Link's: mask byte + 7 data bytes, repeated), checksum (sum of command, sequence,
payload and checksum is 0 mod 128), F7. The sequence is the sender's, incremented per message; the
receiver's reply carries it back. A command after ACK or NAK that does not match the sequence fails
with SYSEX_SEQUENCE. Reserved names: "MIDI INSTRUMENT", "MIDI SONG", the sounds file; PUT or DEL of
them fails with SYSEX_MALFORMED. The device has one 256-byte packet buffer; PUT with the buffer busy
(another operation running) fails with SYSEX_BUSY. A .sid file's PUT is checked at the first DATA
packet before the file is opened (parsing the PSID header); RSID, MUS, broken and too-long files
fail there.

## The register ring

- The ring (`app/reg_ring.h`) carries every register write from the main loop to the audio
  interrupt, which drains it at the start of each block, so a chip only ever changes between two
  rendered blocks. Single producer, single consumer, one core: the interrupt preempts the main
  loop, never the other way round. Each index is written by one side only; a compiler fence keeps
  an entry's stores before the index store that publishes it. The fence is an empty `asm` with a
  memory clobber: the pi32v2 compiler lowers `__atomic_signal_fence` to a `__sync_synchronize`
  call that has no implementation there.
- Entries stay private until `reg_ring_commit`, once a pass, so the interrupt sees a pass's writes
  whole or not at all. A full ring never drops a write (a dropped one could leave a gate stuck or
  the wrong model): the sink commits and waits for the consumer, which on the device sleeps until
  the next interrupt. So the producer must never run with interrupts off once the audio runs.
- 1024 entries, 2 bytes each. The worst main-loop pass is 449 (`REG_RING_WORST_PASS`): the
  instrument's 391 on one chip (`REG_RING_LIVE_PASS`), the song control's 28 on the other and a
  move of the keys' 30. The instrument's: a model change with its reset, gate reset and registers
  (27), ALGORITHM (1), a preset's registers (25), K1-K3 (9), two catch-up ticks (30), all 27 note
  keys pressed and released in the pass (27 × 11 = 297) and two character ticks (2). 27 × 11
  bounds the keys although a note-on can write 12: the pulse width and the cutoff are stale only
  after MIDI, a preset, a model change or a note of another velocity, and the panel's notes share
  one velocity. The host tests push this pass and check it needs no wait. The rest of the ring is
  MIDI's: the device's drain takes an entry only while the pass's worst case and the entry's still
  fit, which takes up to 19 program changes between two blocks where the TRS jack delivers 9. At
  512 entries it took 2, and a flood overflowed the MIDI ring in about 150 ms.
- Both chips share one ring, so a pass's writes to A and B stay in order. Bit 6 of an entry's
  register names chip B (A 0x00..0x18 and 0x80..0x89, B 0x40..0x58 and 0xC0..0xC9); the effect
  slots' pseudo-registers 0xA0..0xBF name no chip (one chain on the mix of both). The drain's own
  pseudo-registers are `REG_RING_CHARACTER` (0x88: commit a character stage) and `REG_RING_MUTE`
  (0x89: a tune's mutes and solos act on the chip, not on its writes; a swap carries the mute to
  the new sid). The drain dispatches with an `if` chain, not a `switch`: a jump table would be
  data in flash, which the interrupt's SRAM code must not read (`platform/fm1/app.ld`, emu-gate's
  `ram_audio.rs`).
- **Model changes.** Re-initialising a chip costs about 0.5 M instructions with a profile's tables,
  far more than a block allows inside the interrupt and more than a pass's 120k poll allowance. So
  there are three sids, chips A and B and a spare: the main loop prepares the spare a slice a tick
  (`reg_ring_prepare_step`, at most `REG_RING_PREPARE_SLICE` instructions, leaving MIDI's drain 12k
  beside the busiest view's 68k pass) while the chips play on, and the drain swaps it in when it
  reaches the reset entry, which live sends once the spare is whole. A reset with no preparation
  waits for the ring to empty and prepares at once. At most one slice runs a pass, and a pass that
  ran one starts no DAC table; requests faster than a preparation (about 70 ms) restart it, so the
  chip swaps once they stop. Chip B is made by its first reset, prepared like any other in the
  main loop's first ticks: an unprepared one would move the boot's timeline.
- **Character.** The sink keeps the character's settings and applies them in the main loop, never
  waiting for the interrupt: all at once to the spare before a swap, and to a playing chip a tick at
  a time into one of two stages, which the drain commits at `REG_RING_CHARACTER`. The producer
  writes a stage only while its pending flag is clear and sets the flag before the entry that
  names it is committed; the drain clears it after the copy. With both stages queued the tick
  waits (`character_defers`). Everything the character sets therefore changes at one block
  boundary, in order with the writes around it, and the host's `live_sid_sink`, running the same
  ramp at the same ticks, makes the same chip.
- **DAC tables.** Each chip plays one table and a ramp builds into the third, one build at a time,
  so ramps on A and B take turns (a table is 4 ticks). A free table is one no chip renders from or
  will: `free_dac` reads the queued stages first, then the chips, because a commit writes the
  chip's table and then clears the stage's flag; read the other way round, a commit between the
  reads would hide a playing table from both (sidou-tests' `reg_ring.rs` plays that interrupt
  through `SIDOU_REG_RING_RACE_HOOK`). The 8580's default DACs are built on first use and the
  first 8580 preparation spends their slices whether done or not, so the slice count is the same
  on the host and the device.
- **The keys' chip goes first** (`reg_ring_set_keys`): a preparation of the other chip is dropped
  for its request, a table the other's ramp builds is taken back, and the other's work yields a
  pass to its own, so the keys' chip changes exactly as one chip alone would. Once the other
  chip's request has waited `REG_RING_YIELD_TICKS` (16) ticks, the keys' ramp starts no new table
  until it is served, so a DAC sweep on the keys cannot hold chip B's preparation off for ever.
- **Filter tables** never change on a chip that renders: age/heat is the stage's `filter_t`, and a
  profile's tables are built on the spare. Only `sid_update_filter` reads them, in the audio
  interrupt, and each entry is an aligned 32-bit store, so even `sid_set_filter_curve` on a
  playing chip gives an old or a new entry, never a torn one. That rests on every register write
  reaching a chip through the drain or from the audio interrupt itself: the song player writes
  chip A from the same interrupt between the drain and the render, and a swap of chip A gives the
  player its registers again through the swap hook before the block renders. A 6502 on the other
  core must keep to the same, writing only through the ring.
- A fresh sid's envelopes release from `SID_ENV_POWER_ON`. Chip A renders all the time and plays
  that out at once; chip B renders only while in use, so its release would play under its first
  user's registers (a 150 ms DC decay, a thud before the first note). Each sid made for chip B is
  set to where that release ends before any write, set rather than rendered: rendering the
  release (about 450 samples) took a pass past its instruction budget (`song_store.rs`).

## Store and settings

**Format and versioning.** The store is format version 1 (`STORE_FORMAT`); every erase header
carries it. Settings use schema version 9 (`SETTINGS_VERSION`); a later schema only appends
fields, so an older firmware reads a newer record's v1 part and a newer one fills what an older
record lacks with defaults.

**Power-loss safety.** Every sector has an erase header (magic, format, erase count, CRC)
programmed right after the erase, then a claim (its number—the erase count again—a CRC and a
claim word), programmed when taken into use, with the erase header again (the same bytes). A
record is data first, then header, then commit word; it counts only with a good CRC and the
commit word programmed (any of its bits). Each write takes the next sequence number; the newest
wins. Before a sector with a valid header is erased, its magic is killed: its 1 bits that read 1
are programmed to 0 (never a 0 over a bit that reads 0). A cut during a program or erase can
leave faint bits (neither programmed nor erased, read one way at one mount and another at the
next): every boot starts a fresh head (no sector from before the mount takes another record), a
commit or claim word counts with any of its bits programmed, the newest record counts when its
CRC checks whatever its commit word reads, and ambiguous sectors (a cut tail, a killed sector, a
torn erase or claim) are erased before the session writes. A claimed one is evacuated first.
Until recovery is done, writes and collections answer `STORE_BUSY`. A hook error leaves its write
undecided as a cut does; a later mount may read it either way until maintenance has evacuated and
erased its sector.

**Erase-ahead pool.** Clean, unclaimed sectors. Whoever writes claims from the pool before the
first byte, so no write waits for an erase: a small record's new head sector takes one (refused
with `STORE_BUSY` if that would take the pool under `STORE_POOL_MIN`: 20), a file's sectors
claim all of them at once (the file waits if the pool cannot cover it and the garbage
collector's reserve). Erases and garbage collection run only when the caller says the device is
quiet (`store_quiet`), and refill the pool toward `STORE_POOL_TARGET` (22). A collection takes
the least-live sector when collecting the least-live sectors one after another would fill fewer
sectors with their copies than it frees.

**Wear leveling.** Sectors are taken from the pool and erased lowest erase count first. When the
erase counts spread more than `STORE_WEAR_SPREAD` (4), maintenance moves the coldest claimed
sector's live records so it cycles too; a live file's sectors never move, so stored files take
their sectors out of the rotation. NOR parts endure about 100k erases a sector. Settings saved
once a minute for ten years is about 5.3M record writes; at about 200 records a sector (records
of ~16 bytes of data) that is 26k sector fills, spread over the 53 sectors outside the pool's
minimum: under 500 erases a sector. Files wear far more, a save erasing as many sectors as the
file has: a 20 KB song (6 sectors) saved every 2 min for 4 h a day, one 64 KB song stored
besides (55 sectors rotate): 720 erases a day, ~13 a sector, ~21 years to 100k; a 64 KB
autosave (18 sectors) every minute for 8 h a day, two 64 KB songs stored besides (37 sectors
rotate): 8,640 a day, ~233 a sector, ~1.2 years. So large files are never autosaved by the
minute, and a save of a file that has not changed since its last save is skipped. Each boot's
fresh head adds at most one sector fill a boot.

**Partial programs.** A page takes at most `STORE_PAGE_PROGS` (34) programs between erases.
Bytes already programmed are programmed again only by a claim (the erase header's same bytes, and
a claim's same bytes again after a cut left it reading never made) and by a kill after a kill
cut short or failed, over the bits it left faint and reading 1 (a kill programs only the magic's
bits that read 1). A part with ECC would need 16-byte units each programmed once (a format
rework).

**Work budgets.** `store_put`, `store_delete`, `store_file_begin`, `store_file_rename`,
`store_collect` and `store_erase_wanted` each do at most `STORE_POLL_WORK` (7,168 units of about
4 instructions each): a record's CRC alone, a walk of a sector's records, a slice
(`STORE_MOUNT_SLICE`: 512 bytes) and garbage collection's plan. The mount works in slices of
`STORE_MOUNT_SLICE` bytes of checks in each poll (a record's CRC may take one poll alone).
`store.work` counts: a byte scanned or copied 1 unit, a CRC byte 4, a key compared 1, a `map()`
call 3, a sector visited in a loop over them 3, a byte programmed 8, a flash operation 300, an
erase 500, a byte the mount checks for 0xFF 2, and a record the mount checks 64.

**Files.** A name, a type and up to `STORE_FILE_MAX` (65,536) bytes, written as chunk records of
`STORE_CHUNK` (1,248) bytes, three to a sector, into sectors claimed for the file alone, then
committed whole by one FILE record in the head. `store_file_begin` claims every sector the file
needs, and the FILE record's place, before it returns, so a file never waits for an erase. Until
its FILE record commits a file does not exist: replacing one leaves the old until the new
commits, and a cut at any point leaves the old file or the new. Chunks never move: a live file's
sectors are never collected. One file is written at a time; small writes go on meanwhile while
the head sector has room. The store struct is under 2 KB: a sector index, a key index, a
256-byte buffer that also stages a file's bytes, the file being written.

**Guard band.** Bytes 0xF00..0xFFF of every store sector are never programmed. An update record
the SPL or the loader could take for one is 80 bytes at a sector's 0xF00 ("4K boundary - 256"),
with the magic 0x5441 at +6 and a matching CRC16. An erased band reads 0xFFFF there, so nothing
in the store can ever look like a record, wherever the SPL scans (the loader skips the range
already). 80 bytes (0xF00..0xF4F) would be enough; the band keeps the whole 256-byte page, so a
page program, which wraps inside its page, can never reach it.

**Memory layout.** The store is [0x97000, 0xE0000): 292 KiB, 73 sectors of 4 KiB. It is the
range the update loader never writes or erases (`LDR_DATA_LO/HI`: its record scan skips it),
between the app area (which ends at 0x93000 whatever the store's size) and the update protocol's
staging area (`OTA_AREA`, 0xE0000), inside the package's VM descriptor ([0x93000, 0xE9000)).
Felucca keeps its main store in the same range.

**Settings record.** One record on the store, versioned. Holds: CARDS, UI sounds, KEYS, the
preset and every live parameter (the keys' chip model, profile and character, effect slots,
envelope mode and preset's edits—the instrument as it was left; not the MASTER volume, which is
the pot), MIDI learn's bindings, firmware version last booted (for the full boot once after an
install), TRACKER ROWS, THEME, the last song, SPREAD and SID MODEL. Not the octave, held notes,
controllers or view on screen. Schema 1 or later; a later schema only appends fields.

**Settings save.** Background save 5 s (`SETTINGS_DEBOUNCE_MS`) after the last change of a
persisted value, the record written whole as one `store_put`. A store that cannot take it now
(`STORE_BUSY`: a write runs, the pool is short, or the device is not quiet yet with programs
held) is asked again on later ticks; an explicit save (`settings_save_now`) still waiting after
`SETTINGS_FORCE_MS` (2,000 ms) forces the held operations (the force hook: short gaps the user
asked for) and says so (SAVED (GAPS)). The unsaved edit (the current USER sound's record) is
written by the same debounce, before the settings record: a cut between them leaves the new
sound, never an older edit over a newer record. A background write the store took, now
committed: a program error meanwhile left it undecided, so both records are written again by the
next debounce (an explicit save checks its own). The second failure in a row says so once; a
success clears the count.

## SCENE

**Frame budgets.** A frame sends at most `SCENE_VIEW_FRAME_CELLS` (200) cells: 200 cells and
their windows are about 17.5 ms of SPI (one window a cell), inside the 20 ms budget. FIRE burns
every other frame, two steps at once (12.5 fps, rising as fast as at 25), and each frame draws
half its rows, alternately, so a frame sends at most half the area. PLASMA draws every other row
a frame. The tile compositor's grid run takes at most `SCENE_PX_RUN_CELLS` (4) cells: two runs
of them, each at the worst cell RED MOON RITE draws, stay within a ui_poll's allowance with a
song's analysis slice in the same pass. The scan works in slices of `SCENE_PX_SCAN_SLICE` (8)
candidates a ui_poll: a slice of the worst tiles RED MOON RITE draws, with the scan's walk over
the candidate bits, stays within the pass's allowance, a song's analysis slice with it. A pass
that scans a slice renders no run.

**Randomness.** Seeded xorshift for determinism: the same music gives the same frames. The state
is `scene_random(uint32_t* state)`.

**FIRE.** Three flames, a voice each (columns 0-9, 10-19, 20-29), in the bottom
`SCENE_FIRE_ROWS` (14) rows, two heat rows a cell (an upper-half glyph). A voice's envelope
heats its flame, a note-on flares it, its pitch sets how high it burns (higher notes cool
slower: the cooling is `13 - (pitch - 36) * 6 / 48 - resonance / 4`, clamped to 2), the cutoff
how hot it looks (red, up to yellow and white near the bed: the white starts at
`fire_white(cutoff)` on a curve that rises fast and flattens), the resonance lowers the cooling.
The fuel sways along the bed (two sines drifting apart) and a cell in eight cools by 3 cool
more: the flames break into tongues. A silent voice's flame dies down to the brown ember bed
(`FIRE_IDLE_HOT`: 120), which holds still. The ember bed: the bottom heat row glows at least
`FIRE_EMBER` (38, brown), and heat at or below `FIRE_DARK` (48) goes out. The fire burns every
other frame, two steps at once so the flames rise as fast as at 25 fps, and draws half its rows
a frame, the even ones on the frame it burns and the odd ones on the next: 12.5 fps, half the
area a frame.

**PLASMA.** Four sines over the screen in a dither ramp. The cutoff sets its speed (0.1 to
0.8 rad a second, on its square), the resonance its twist, the voices' envelopes speed it up
(silence runs an eighth of it) and voice 3's warms it (up to 0.18 of the range, moving at most
`WARM_STEP` (4) a frame toward its envelope, so a drum's envelope does not move every cell at
once); a note-on drops a bright ring, growing from a place its pitch sets (the pitch class
across, the pitch up). Each sine runs at its own multiple of the speed (1, 0.7 back, 1.3 and 1.7
back). The four sums' range is -508..508; a level is one of 20 (5 ramp steps of 4 dithers). A
frame draws every other row, alternately. A ripple: a bright ring `RING_W` (8) quarter cells
wide each side, growing `RING_SPEED` (2) quarter cells a frame from its note-on; its strength at
the note-on (`RIPPLE_AMP`: 224, 0.22 of the plasma's range) falls exp(-3 t), 0.887 a frame. One
ring, not a train of them: a frame changes the cells on its edge, not every cell round it.

**RULE 30.** Wolfram's rule 30 on a ring of 60 cells (`SCENE_RULE30_W`), a generation a row of
quadrant pixels, written down the screen by a moving head that clears two rows ahead of it. Each
note-on steps it once (`SCENE_RULE30_MAX_STEPS`: 3 at most), the row in that voice's colour;
voice 3's note-on also flips the cell its pitch picks; a held voice keeps it stepping with its
envelope (`SCENE_ENV_STEP`: 512, so a full envelope is every other frame). A low cutoff darkens
the colours (under `SCENE_DARK_CUTOFF`: 512). The start draws `SCENE_RULE30_START` (24)
generations in grey, so it starts with its pattern.

**STARS.** A starfield flying out of the centre, a third of the stars a voice's. A voice's
envelope speeds its stars (cruise + envelope * 180 / 256), a note-on lights them in its colour
for `SCENE_STARS_LIT` (5) frames, the latest note's pitch steers the centre across, and the
cutoff sets the cruising speed (`8 + cutoff * 24 / 2047`). Depth layers: 0..3 (far to near),
each drawn with a different glyph and colour. The stars come closer at their speed; a star at
`STAR_NEAR` (205) or closer wraps to `STAR_FAR` (4,096) and a new place.

**ANT.** Three Langton's ants, a voice each. A note-on walks its ant `SCENE_ANT_STEPS` (3) steps
and a sounding voice keeps it walking with its envelope (the envelope adds up each frame, a step
every `SCENE_ENV_STEP` of it); the trail takes the voice's colour, darker under a low cutoff. A
silent voice's ant stops and greys. The board clears and the ants start again after
`SCENE_ANT_RESET` (9,000) steps.

**Pixel art images.** Format: a frame of fw x fh stored pixels (gx screen pixels wide: 2 for C64
wide multicolour, 1 for backgrounds), of which the w x h rectangle from (x, y) is coded; the
rest is transparent or, for a patch, the base image's. Rows use the shared pool format in
raw-run mode at `SCENE_ART_SEEK` (32) pixels: `rows[r]` is a byte offset from data, the runs
adding up to w exactly. The colour `clear` codes a transparent run. A row wider than
`SCENE_ART_SEEK` has a run start at every `SCENE_ART_SEEK` pixels of it (the converter splits a
run there), and the row packet holds its checkpoints, so a span part way along a row is found
from the checkpoint before it. The decoders trust the converter for what they do not check:
every row's runs add up to w exactly, a patch's frame (fw, fh, gx) is its base's, and gx is 1 or
2. (ax, ay) is the anchor, in screen pixels of the unmirrored frame. Drawn mirrored, the frame's
stored pixel x is read from fw - 1 - x and the anchor is (fw * gx - ax, ay); no mirrored copies
are stored.

**Tile compositor.** No frame buffer (240 x 240 at 4 bits would be 28.8 KB). The scene is a
stack of layers in slots (`SCENE_PX_LAYERS`: 24), drawn bottom first: fills, gradients (a
computed sky), images (backgrounds and sprites, each placed by its frame's top-left, mirrored or
not, with an optional colour remap), marks (single pixels: sparks, arcs, small discs), text in
the 8 x 8 font and calls (a function that paints a row span: colour cycling over a mask). Each
layer knows its box, so a tile visits only the layers over it. The scene owns the grid's painter
region: the grid sends only the cells it marks, and composites each again as it sends it.
Setting a slot to a layer that differs from what it held makes candidates of the tiles under
both its old and its new box; `scene_px_scan` composites each candidate into a scratch tile,
hashes it (FNV-1a, 32 bits), and marks it only if the hash differs from that of what the panel
shows. The hash of a tile is taken again as it is sent, so it is always the panel's. The scan
works in slices between frames. The grid sends the marked cells round the screen from where it
stopped, so none waits more than a sweep. A layer's identity is its fields: a layer whose
drawing changes while its fields do not changes `param`. A refresh sweep (`scene_px_refresh`, a
tile or two a frame) sends every tile again in turn regardless, so a change missed that way
heals within a sweep. The invariant: every run `grid_next_run` hands out with the scene's cells
in it reaches the panel, or the grid is invalidated. A run is counted as shown, and its tiles'
hashes are taken, when it is rendered; one rendered and then dropped leaves hashes of pixels the
panel never got. So a view that has the screen back without its painter region being set again
through `grid_region_painter` calls `scene_px_init` or `grid_invalidate` before its next frame.
The cost: `grid_render_run` composites a run's tiles as it renders it (ui_poll renders up to two
runs a call), so a run takes at most `SCENE_PX_RUN_CELLS` of them.

**RED MOON RITE.** Two barbarians duel under a blood moon, and the music plays the fight. The
vignette (240 x 240 pixels) runs on the tile compositor. Deterministic: the same music gives the
same frames. A kick: the leader raises his sword and chops overhead; a snare: the other parries,
a spark where the blades meet, and the leader loses a stamina notch; a hat: both shift between
their two guards; the bass (high/low): gives the right/left barbarian the lead, the pair drifts
toward the one giving ground; an arp note: the torches flare; the fill (the last beat of every
fourth bar): the leader's spinning attack disarms the other, his sword flies up spinning (the
eight `scene_rmr_sword` frames, `scene_rmr_flights`' path) and sticks point-down in the
flagstones with a small shake; while one is unarmed nobody attacks or parries; each bar's first
row refills an empty stamina meter; the cutoff: the sky's palette (0..2, from darkest to
brightest), wiped down a cell row a frame. Silence calms it: `SCENE_RMR_CALM_FRAMES` (25) frames
after the last gate went off, the torches and the spin's arc hold still, and once the timers
have run out nothing changes. The torches' flames: six colours cycling up through the flame,
which wobbles; flared, taller and with sparks over it. The prototype's cycle stage, in fixed
point (Q12) with the screen's sine: `sin(y * 0.9 + frame * 0.7)` as 256ths of a turn. The
spin's arc: 14 pixels on an ellipse round the spinning fighter, turning. A frame is spread over
the main loop's passes: the compose (the vignette's step: its music, its state, its layers set
again, which makes candidates of the tiles under what moved; a top layer over what the screen
draws over the scene this frame; the cells the screen drew over put back blank within the
frame's budget; the refresh sweep's tile while the music moves it; the cells the budget leaves
for the scan); the scan, a slice of `SCENE_PX_SCAN_SLICE` candidates in each ui_poll that has
nothing to send: each composited, and marked to be sent only if it changed. The scan stops
marking once the frame has spent its `SCENE_VIEW_FRAME_CELLS`, and goes on from that tile at the
next frame.

**RED MOON RITE music.** From a song: the voices' roles (DRUMS, BASS, ARP/LEAD) from the song's
SCNE chunk, and where it leaves a voice AUTO, guessed from the current and the last bar: a voice
that played an instrument whose wavetable starts with noise (a waveform byte with bit 7) is
DRUMS; of the others, the one whose lowest note is lowest is BASS, the rest ARP. A drum voice's
note is a KICK, SNARE or HAT: the SCNE chunk's drum table's for its instrument, else by the
drum's pitch, the first wavetable step's note (an absolute note, or the row's note moved by a
relative one): below `SCENE_SONG_KICK_BELOW` (100) a kick, from `SCENE_SONG_HAT_FROM` (106) a
hat, a snare between (MIDI numbers). The place: a row is each row event of voice 1
(`SCENE_SONG_CLOCK`); its place in the bar is the pattern's row modulo 4 beats of the song's
rows a beat (the MIDI chunk's, else `SCENE_SONG_RPB`: 4), and the bar counts each time that
place goes back; the fill is the last beat of every fourth bar. The bass's note is high when it
is above the middle of the bass's range over the current and the last bar (the prototype's 50,
D3, while that range is a single note: `SCENE_SONG_BASS_SPLIT`). Every voice's gate; a voice
muted (or held by the keys, KEYS A) plays no part. From the instrument (no song playing): each
note-on is a hit by its voice's waveform as LIVE shows it: with noise a drum by its pitch, as
above; else the lowest note held is the bass, the others the arp. A free clock makes a row every
`SCENE_SONG_FREE_FRAMES` (3) frames while a note is held.

## CHIP

**Chip control (chip_ctl).** A reset is never sent unprepared: the chip's first one included (chip
B's at boot). When following, the controls are sent first: the sink keeps them for the preparation
and the ramp, then the profile just before the reset so the prepared spare chip is for it.

**Chip mixing (chip_mix).** The fade is CHIP_MIX_FADE samples (128: one audio block). Chip B going
in or out of use fades linearly over that block, so the level a silent chip has of its own (DC,
thump, output filter settling) never steps: sample i of a fade-in takes (i + 1) / 128 of B, of a
fade-out (127 - i) / 128. Integer-only: b × w >> 7, an arithmetic shift, so the host mixes bit for
bit as the device does. Headroom: two chips at full scale do not fit int16, so the sum is kept
whole — in a[i] (the sum itself when it fits, bit for bit as ever) and, in a block where some sum
does not fit, a[i] saturated to int16 and over[i] what saturating took off — so the slots' soft
limiter is the one place the sum is limited. One chip's loudest steady sample on the bench
scenarios is 20,274: two that loud and in phase reach the limiter, one never does.

**Chip spread (chip_spread).** SPREAD separates the chips, so it acts only while both sound: a chip
sounding alone stays in the centre as mono at any SPREAD. With phi = SPREAD / 100 × π / 4, the mid
(A + B) takes cos phi and the side (A - B) sin phi: L = mid cos phi + side sin phi, R = mid cos phi
- side sin phi. Equal power at every SPREAD (cos² + sin² = 1 for each chip), and at 100 % chip A is
on the left alone, B on the right, each √2 (+3 dB) where mono gave it 1 on both sides. The gains
are computed by Taylor series to phi^9 and phi^8 (the terms left under 3×10⁻⁸ there). The
separation glides in as the second chip starts sounding and out as one goes quiet: phi is SPREAD's
angle times the glide's share (`share`), 0 to 1 over CHIP_SPREAD_GLIDE blocks (32: about 0.1 s), and the
gains move linearly between the mono pair and SPREAD's (within 0.7 dB of equal power half way). At
a share of 0 the stage does not run: mono, bit for bit. A chip sounds while one of its envelopes is at
CHIP_SPREAD_ON or more (16: -24 dB); both stay sounding until one has had all three under
CHIP_SPREAD_OFF (4: -36 dB) for CHIP_SPREAD_HOLD blocks (16: 46 ms): the hysteresis keeps the gaps
between a phrase's notes from fluttering the image. The thresholds are powers of two so one test
runs on the counters' OR. Every slot's wet path takes the mono mix (the FX slot's always took (L +
R) / 2), so the side passes the slots' dry paths only: a reverb or a delay keeps the dry spread and
adds its own stereo tail; a slot fully wet hears the chips as one. Each slot taking a stereo input
would run Distortion and Modulation twice (about 300 instructions a sample at their heaviest): over
the slots' budget.

**CHIP view (chip_view).** A frame sends at most CHIP_VIEW_FRAME_CELLS cells (200: about 17.5 ms
of SPI with their windows, inside the 20 ms budget). The page is built off the grid in a canvas,
then its changed cells are copied onto the grid within the budget less what the toast, the raster
and a cover take; what does not fit goes on from that cell the next frame. Cells covered this frame
(MIDI learn's frame, a toast) are left alone; the border's cells while the raster runs too. Under a
wipe or a banded cut, the rows it reveals are copied whole. The tabs: chip A during a song is the
song's, so its tab shows the song's setup and edits it (K1-K4 turned by the view itself,
PANEL_PAGE_NONE, which MIDI learn binds nothing to); every other tab shows the keys' setup, live's.
When a knob just turned (SCREEN_POPUP_MS), its label is shown reversed in the knob's readable
colour (a dim label stays dim): the canvas after the page is built.

## Views, screen and panel

**View dispatch (views).** The menu's frame is MENU_FRAME_CELLS cells (178): a scroll's third line waits a frame, so a scroll over the shorter menu's
action lines stays within 15.1 ms. The MIDI page draws MIDI_LINES lines per frame (2): the
compose's cost is its ui_poll's. Every frame the instrument's state is compared with the last
frame's: a change of octave, effect slot or algorithm shows a toast, whatever made it (panel, menu
or MIDI), and a chip model change also runs the raster over the border; not while the boot shows,
which nothing draws over. A control's own popup says what it changed: no toast then.

**Screen (screen).** The layers: a wipe reveals SCREEN_WIPE_ROWS rows a frame (4: 5 rows of 30
cells, 19.3 KB, 12.8 ms of SPI, inside the 20 ms budget), behind a one-row white edge; a banded cut
SCREEN_BAND_ROWS rows a frame with no edge (6 rows, 23 KB, 15.4 ms, a full screen in 5 frames). The
raster moves every other frame (SCREEN_RASTER_FRAMES): a step recolours the 116 border cells
(15.5 KB and 10.3 ms of SPI), so 5.2 ms a frame on average. The border's cells carry the view's
text (MODEL, the first digits): purple on black is 2.6:1, so the band that would be purple is light
grey (6.9:1) for readability; the ramp itself stays. The toast: large text (2 times the font) on
SCREEN_TOAST_ROW and the row below it, SCREEN_TOAST_LINE characters a line at most (13); a text of
more than a line breaks at a space into a second line above (rows 25-28), or when its words do not
fit two lines, ordinary text on one row. The popup opens and closes SCREEN_POPUP_STEP rows a frame
(3: 78 cells). The block cursor is on for SCREEN_BLINK_FRAMES frames, then off as long (8 frames:
320 ms at 25 fps).

**Grid (grid).** The 240 × 240 screen as 30 × 30 cells of 8 × 8 pixels. Only changed cells go out:
grid_next_run hands out the changed cells one run at a time, a run being horizontally adjacent
changed cells on one row (one LCD window). A cell can carry a pixel overlay: GRID_OVERLAYS at once
(255: LIVE's three scopes take about 240; one costs 10 bytes of RAM, 2,550 bytes total). Large text
draws each character scale × scale cells (2 or 3), from the same 8 × 8 font: a cell's code is its
part's (a piece, GRID_BIG_FIRST..) and the grid keeps the character it is a piece of. Dirty cells
are marked against the composed cell, not the shown one: comparing with shown_ch/shown_attr saves
nothing while a screen clears before it redraws (1 % more main-loop instructions per composed
frame). The cell arrays are word-aligned for may_alias access (dirty4's).

**Widgets (widget).** A menu at scale 2 (a line is up to 60 cells) draws the lines whose text
changed since they were drawn, the selected one first, while what the grid has pending
(grid_pending) leaves room for another line within the budget (0: no limit); the first line makes
progress even while a view switch has old cells pending. Rows in force_rows are drawn regardless of
budget unless force_rows includes MENU_BUDGET_FORCED. Deferred forced rows are invalidated so the
next call restores them.

**Panel (panel).** The note keys have no velocity: 127 sounds as velocity off does, whatever the
preset's velocity target. The MASTER pot's gain follows a change of PANEL_MASTER_HYSTERESIS counts
(4: 0.4 % of the travel, no audible step on the audio taper); the popup only a turn of
PANEL_MASTER_POPUP counts from where the pot last rested (16: 1.6 %), so a reading that wanders by
up to 15 counts at rest may nudge the gain but never shows it. The last PANEL_MASTER_END counts at
either end read as that end (8): 0 and 1023 land exactly, and a pot resting there holds them
through the noise. The asks are at least PANEL_MASTER_ASK_GAP calls apart (12: 120 ms at
LIVE_TICK_HZ, three 40 ms frames), so the screen opens the popup only in a frame without an ask,
and a turn that landed a change every frame does not keep it shut. A character control's detent
moves PANEL_CHARACTER_DETENT steps (4): 32 detents end to end, as a slot's. GLO's layer: keys 0-2
mute voices 1-3, keys 3-5 solo them.

**Keys routing (keys_route).** live plays the instrument through this sink, which sends its writes
to the chip the keys play: chip A with no song playing; chip B while a song plays
on chip A (KEYS AUTO, the default); chip A always with KEYS A, where during a song a key takes a
voice from it. While a song plays on chip A with the keys on it, chip A is the song's: live's setup
writes (the character, resets, the gate reset) and its writes to $15-$18 (the filter and the
volume) are dropped, and of a voice's registers only those of the voices the keys hold get through,
and each voice's gate-off as the keys let it go. The setup chip A is to have (the song's CHPA, at
its start and at every change) goes to keys_route_song_wants, the one writer of song_want:
keys_route brings chip A to it in the background (prepared, a slice a tick). A profile change of
chip A during a song is a chip reset (prepared, then swapped in): the reset chip has none of the
song's registers, so the register shadow rewrites them after the swap. A voice the keys take goes
to the taking callback before the first of its registers goes to chip A's sink, so the song can
leave it alone from the block those registers reach. The keys' setup travels with the keys: live's
model, profile, character and envelope mode are the keys' setup; the chip the keys play gets it
from live, and the other chip follows it in the background, a slice a tick after the keys' own
chip, so chip B holds the keys' setup whenever the keys move to it. Moves happen in
keys_route_tick, once a pass after the live ticks: to chip B, once chip B has the keys' setup (the
notes the keys hold on chip A are released there, and every register is written to chip B, so the
keys still down sound again on it); back to chip A, once chip A has the keys' setup again and no
key is held, so nothing is cut; until then the keys stay on chip B, which sounds the same.

**Themes (theme).** Two themes: HIGH CONTRAST (the default, black with white values, light grey
labels, dark grey for inactive) and C64 (blue background, reverse-video menu bars). A view never
names a colour of the 16 for a purpose of its own: it asks the theme for the role (`t->value`,
`t->knob[0]`) and builds its cell attributes with THEME_ATTR. THEME_ACCENT: a popup colour that
takes the accent of the control (the knob's colour).

**The view stack (screen).** A view is a table of functions in flash (enter, exit, input,
compose, changed); the view on top has the focus, is composed and gets the events, and a pop
gives the view under it the focus back with its state as it left it. Push and pop are for input
handlers and the code that drives the screen: from a view's enter or exit the screen refuses
(-1). An event goes to the capture first (a modal grab such as MIDI learn, which sees HOME too
and may take it as cancel), then the global handler (HOME), then the view on top; 0 from all of
them lets the caller fall back on the panel's own mapping. A capture belongs to the stack as it
was when set and is released when a pop takes the stack below that depth. One slot past
`SCREEN_DEPTH` is kept for the help card (`screen_push_over`), so a held HOME shows it over a
full stack. A view composes only when its changed function says so (or it has none) or the
screen forces it (a switch, `screen_redraw`, while a layer runs and on the frame after), so an
idle screen sends nothing; a compose that leaves a cell alone keeps whatever the grid holds. The
layers are run by one test of `layers`, so a frame with none costs that alone. The popup's
state sits after the screen's other fields: the views reach those through small offsets, which
it must not move.

## Songs: STEP, TRACKER, SONG and SIDS

### Song editing and transport

- A shift's inverse (undo) records only the row it discarded and that row's locks; shift previews
  wait for completion before showing. Row writes may be prepared immediately.
- Main-loop edits: completion means applied AND released, including the main-loop lock move, not
  just a command acknowledgement. A save must not start while accepted edits await completion;
  guard refusals cannot revoke an edit after ownership has transferred to the player.
- Unchanged SAVE is skipped only while `target` names the same store file the model came from
  (`io->fid`); a SysEx DEL or PUT may have deleted or replaced it meanwhile.
- The song's edit queue and file I/O share one gt2_stream workspace; reads and writes reuse it.
- `GT2_SAVED_MAX == STORE_FILE_MAX` (a static assertion): device song edits keep within the
  store's file capacity.

### NEW's template

- NEW's song is produced byte by byte (no buffer held): a plain GTS5 stream with subtune 1, three
  64-row patterns, a pulse lead and a bass (instruments 1-2), then the SID drum kit (3-10, its
  table rows first), and the digi drums (11-14, each a copy of its chip drum). The DIGI chunk
  marks them.
- The lead and the bass attack at 1 (8 ms), not 0: at the gate the chip compares the envelope's
  rate counter with decay's period for a cycle, and one phase in nine the counter stepped past
  attack 0's 9 and the note waited out its wrap (33 ms). The ADSR delay bug; the kit's drums
  share this workaround.

### Generative play (DICE, MUTATE, sweep)

- Each run is ordinary cell writes through the song's editor (`song_edit.h`), one a turn of
  `song_gen_drain`, so each is an edit as any other: the pattern stays plain GT2 data and undo
  steps back a cell at a time.
- What "random" means: `song_mix` of the seed and the row. The same seed makes the same pattern.
- DICE on an ARP lane writes chords: each hit a note, the lane's instrument and GoatTracker's 8XX
  (wavetable pointer) at one of the song's shared chord programs (MAJ, MIN, SUS4, dominant
  seventh). The missing programs are written first through the batch writer. A chord whose tones
  are in the lane's scale, the seed's pick.
- BASS+DRUMS lane: the kit's kick and snare (from `gt2_kit.h`) on the rhythm's strong rows and
  the lane's instrument (the bass) between. STRAIGHT and OFFBEAT have a backbeat, 3-3-2 the
  tresillo's kicks, FOUR a kick on every beat, BROKEN a breakbeat.
- The filter sweep: a C command (FILTER CUTOFF) every `SONG_GEN_SWEEP_EVERY` (2) rows from
  `SWEEP_LOW` (0x10) to `SWEEP_HIGH` (0xF0) or back, where the row has no command.

### SONG pages (ORDER, INSTR, TABLE)

- Prospective `edit_guard` requests: `pattern` = subtune/instrument/table, `row` =
  entry/field/row, `cell[0]` = value, `cell[1]` = voice (ORDER) or side (TABLE). RST uses `row
  == entries`. Storage guards must handle these kinds as well as pattern-row requests.
- An empty pattern is published first; only a later acknowledged step can name it (`auxiliary`
  flag).
- The batch writer: runs of instrument bytes and table rows, written one differing byte per
  acknowledged edit through the edit queue. A byte the song already has is skipped. A write the
  guard refuses ends them (the status says why).
- TABLE's ARP helper: an arpeggio as semitone offsets over the played note, written as plain GT2
  wavetable rows. The draft goes to the player (the keys play it) while the helper is open, on
  the voice the keys play; once closed and its rows written, the draft is taken off.
- LAB's send: the sound as instrument, named, its filter on the song views' voice, through the
  batch writer (works while the song plays). Each program goes after its table's last row, or
  over the rows the instrument's own kept programs had (an unedited sent instrument's that no
  other instrument points into), the rows they no longer need blanked. A speed row the table has
  already is shared.

### REC while the song plays

- REC records the note keys and K1/K2 knobs (cutoff and resonance locks) through the editor's
  cell writes and the lock table, as the views' own edits. "SONG FULL" and other refusals toast.
- REC's clock: the call each voice's row began on and the last call made, in the play's count
  (low 16 bits), and how far toward the next call it is now, in 256ths. Where in its row a key
  pressed now falls.
- The lag: what the player heard lags the render by the output's double buffer, and the key
  reaches the main loop after its debounce (`REC_LAG_SAMPLES` in `platform/fm1/main.c`). REC
  places a key that much earlier.
- Quantise: QUANT_NEAREST (default) places a key at the nearest row (halfway rounds up);
  QUANT_OFF at the row it is in; QUANT_COUNT with a bar's count-in (four times rows-per-beat),
  which starts only when a song view shows.

### STEP

- Two kinds of parameter lock: LOCK (K1 cutoff, K2 resonance, K3 pulse width; with EDIT held
  K1-K4 the character, `SONG_LOCK_*`) holds for that row only and lives in the LOCK chunk; SET
  (K4's waveform) is GoatTracker's own `7XY` command on that row, which holds until the next
  waveform change, and is refused where the row's command column is taken.
- A step that needs two rows written (the note and the key off after it) sends the second once
  the first is applied (`follow`).

### TRACKER

- Two modes (`song_view.scroll`, kept with the settings): BAR (default) the rows stand still and
  the bar (dark grey) moves down them; the window is the 8-row page the bar's row is in, with 4
  rows of context above it, and moves a page at a time. SCROLL (GoatTracker's) the bar's row
  stays on the window's row 5 and the rows move under it. Every row step can change every row: at
  fast tempos the copy lags.
- The row drawing takes the cursor row, the voice's colour and a field to highlight
  (`TRACKER_VIEW_FIELD_*`): an edit cursor on a note, an instrument or a command is the same
  drawing with a field named.

### .sid tunes (SIDS)

- Each `sid_io_poll` pass's part is within the store's share of a pass (`STORE_POLL_WORST`:
  30,000 instructions); the caller gives `sid_io_poll` only passes the store does no work in
  (`audio_work_slice` on the device). The emulator test `psid_store.rs` measures it.
- A song change of a tune loaded so (`psid_ctl_load`'s file NULL: `psid_ctl_subtune` refuses) is
  the caller's: `psid_ctl_stop`, and once the stop has played out, `sid_io_load` again and the
  parts, then `psid_ctl_play` with the song. The file is opened again by name for each load, so a
  file replaced or deleted meanwhile (a SysEx PUT or DEL) is read as it is then, or refused.
- With no tune loaded and the song edited since its last save (`song_io_edited`: its patterns,
  tables, instruments, settings; a sound edit on LIVE or in LAB is not the song's, and the tune
  leaves it as it is), a tune asks first: SAVE saves the song and the tune loads once it is
  saved; the question waits for an unnamed song's SAVE AS.

### Song views' transport and keys

- The mutes to the player: the song's, or while a tune has the song's memory the tune's
  (`song_view.tune`). GLO's mutes and solos reach the tune while away: voice v of both chips for
  a 2SID tune.
- The keys' notes are live's MIDI notes: GoatTracker's C-0 is MIDI's 12 (`GT2_NOTE0`). A jam note
  that cannot go now is dropped (late notes being worse than none).
