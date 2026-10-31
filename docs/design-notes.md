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
- The audio path runs from SRAM (`.ram_text`, placed by app.ld's name rules): the boot copies
  it there and the ALNK0 interrupt executes nothing else, also while a song or a .sid tune
  plays. fm1-emu does not model the I-cache, so the timing benefit is left to hardware
  (docs/hardware.md, open checks). RAMTEXT is 48 KB, ending at Felucca's RAM-run debug area
  (0x01C0C000), which code does not reach until a device check shows that SRAM is the app's.
- No direct `call` or `goto` crosses between flash (XIP, 0x02000000 up) and SRAM code: SRAM is
  about 4 MB below XIP, the edge of a branch's reach, so such a branch either fails the link
  ("relocation truncated to fit", `R_PI32V2_LONG_JUMP_23M2`) or links only while the code
  happens to lie close enough. The firmware links with `--emit-relocs` and `ram_audio.rs`
  requires every branch relocation's site and target in the same region. Address loads
  (`MOV_ABS32`, type 5, and `ABS32`, type 8, in JieLi's binutils) may cross: that is how flash
  code calls SRAM code through a pointer (audio.c `far_drain`) and how the vector table
  reaches `isr_alnk0`. Any other relocation type fails the test until it is classified.
- The audio path reads no data from flash: every address the `.ram_text` code loads lies in
  SRAM (RAM, POOL), where a read in the interrupt cannot miss the cache and wait on the flash.
  A constant table the audio path reads goes to `.data` on the device
  (`__attribute__((section(".data.<name>")))` under `__PI32V2__`, as `SID_NOISE_WB`). Data
  reached through a pointer set up outside the audio path (the combined waveforms' pulldown
  rows in flash by design, the DAC tables in POOL) is not checked.

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
## Tests and budgets

### The emulator harness (emu-gate's `common`)

- `Machine::boot` boots as the device does from its second power-on: the flash of a first boot
  on a blank chip (`settled_flash`, made once per image and test process) with its settings
  stored. `SETTLE_MS` (7 s) covers the settings' debounce (5 s from the load at the screen's
  start) and the held programs' wait for quiet. `boot_blank` is the first power-on after an
  install.
- The settled flash is made with `run_ms_jit`, fm1-core's JIT through fm1-soc's own loop,
  several times faster than the stepped `run_ms`. The JIT runs the same guest instructions as
  the interpreter, so the flash it leaves is the stepped run's byte for byte (`boot.rs` checks
  it). It runs in slices of 4,096 instructions because `skip_idle` jumps the clock: the run
  stops near its end and not many instructions later. It keeps none of the machine's counts or
  watches and has no USB host.
- Every machine checks its stacks when it is dropped: both are painted before the first
  instruction, and each stack's deepest use stays within half of it (the user stack is sized
  at twice its deepest use). With `SIDOU_STACK_LOG` set to a file, each machine appends its
  test's name and both depths, the measurement the sizes come from.
- `Machine::midi_paced` keeps the TRS backlog at 900 bytes, under a third of fm1-emu's
  3,125-byte queue (about 0.3 s): fm1-emu drops what is behind a second's backlog, so a stream
  poured in at once plays half of it and leaves the jack idle for the rest. Topped up every
  millisecond (a byte takes 320 us), the jack never runs dry and never overflows.
- `masked_stalls` stand in for a flash operation the firmware waits on with interrupts masked
  (fm1-emu's flash is instant): the CPU stands still while the clock, the peripherals and the
  scheduled TRS bytes run on. `long_pass` holds the main loop at one PC while the interrupts run
  as usual, as a main-loop pass that long would.
- The panel helpers (`common::panel`) hold a button 30 ms down and 30 ms up, past the key
  scan's debounce both ways. A chord holds the modifier 10 ms before the key it changes, as a
  player holds it: the key scan reads a column every 100 us and takes a key after two scan
  frames, so a modifier and its key pressed in the same emulated ms can arrive in either
  order, and the key alone means something else (EDIT + PLAY is a continue, PLAY alone a play
  from the top). fm1-emu turns an encoder a detent every 8 ms; a turn is waited out at 10 ms a
  detent and 20 ms more.
- The USB host (`common::usb_host`) is fm1-emu's own model of the FM-1's USB0 controller (a
  Mentor MUSB core behind JieLi's register window) and a full-speed host, independent of
  Felucca's driver. The test, not the SoC's clock, runs its 1 ms frames, so it decides when a
  computer is there. fm1-emu hands IN data on only as whole MIDI messages, which lose the
  packets' bounds, so the harness reads the IN packets through the controller's DMA. fm1-emu's
  host cannot send a control request of the test's own, SET_CUR on a running stream (a new
  rate takes a new enumeration, `replug`), tell the device's address, or send more than one
  bulk OUT transaction a frame (docs/upstream-reports.md, row 11).
- The host renders (`common::host_render`) are the firmware's audio path rebuilt from the same
  C on the host, block by block as `audio_fill` runs it: the instrument (live and its preparer,
  so a profile step's reset lands at the device's tick), chip B made with the keys' setup and
  rendered only while in use or until its envelopes reach 0, faded in or out over the block it
  changes in, the tail gate, the effect slots, the thump and MASTER. The tests compare them
  with the emulated device sample for sample; any change to the device's order of work must
  be mirrored here.

### Budgets (emu-gate's `common::budgets`)

A budget is set from a measurement plus a margin, and raising one is the owner's decision.

- `AUDIO_TARGET`, 2,700 instructions a sample, is everything the audio interrupt may do. Two
  compile-time assertions keep a song (two chips at their worst, the effects at their cap, the
  ISR, the tap, the player, a digi drum, USB audio recording) and a .sid tune (two chips, the
  effects, the ISR, the tap and the tune's own interrupt work) within it.
- `SID_WORST_PER_CHIP`, 550: the SID's costliest scenario per chip on average (549.37 two
  chips, 545.30 one, with the 6581 render's clamps as the FPU's `fmax` and `fmin`). It stays as
  the SID's own guard; the effect gates use the worst block instead.
- `SID_WORST_BLOCK_ONE_CHIP` 711 and `SID_WORST_BLOCK_TWO_CHIPS` 1,426: the costliest
  128-sample block. The audio interrupt has a hard per-block deadline, so the effect gates
  pair the slots with this, not with an average. Both are the first block of a combined-noise
  scenario from power-on while the noise register shifts towards its lock-up (decision record
  0004, "Worst blocks"), on the 6581: measured 710.48 and 1,425.66 (the chip character's
  709.94 and 1,424.80; the 8580's 691.16 and 1,409.87).
- `ISR_OVERHEAD`, 100: the audio interrupt's own work around the chips and slots (0004's
  estimate). `TWO_CHIPS_PLUS_EFFECTS`, 2,300: the cap for two chips and the effects.
- `TAP_PER_SAMPLE`, 4.5: LIVE's scope tap, on by default since LIVE is the view at power-on,
  so the effect gates count it with the chips. Measured 4.44 (one render a block feeds the
  points); the look-and-feel plan asked for under about 10.
- `USB_AUDIO_PER_SAMPLE`, 2.0: what recording over USB adds inside the audio interrupt, only
  TIMER5's work nesting in a render now and then (measured 0.87 at 44.1 kHz, 1.11 at 48). The
  rest runs in TIMER5 outside renders: 41.83 a sample at 44.1 kHz, 335.31 at 48 where it
  resamples.
- `PLAYER_WORST_BLOCK`, 60: the song player's worst block, its call and
  `SONG_EDITS_PER_BLOCK` of the costliest edit command (the song engine plan's budget).
- `DIGI_PER_SAMPLE`, 9.5: the digi drums' trigger and mix while one plays without a break
  (measured 9.47 with the codes two a byte). With no digi playing it tests one word a block
  and no example song has a DIGI chunk, so it is added to the player's worst block, not in it.
- `CPU6502_INSTR_PER_CYCLE`, 9.33: SIDOU's 6502 (`CPU6502_SMALL` at `-Os`) in pi32v2
  instructions per 6502 cycle on a GoatTracker workload, the worst of three example songs'
  10 s runs (8.878, wavecmdtest) plus 5 %.
- `SHADOW_FRAME_MEAN` 49,250 and `SHADOW_SLICE_WORST` 42,690: the voice shadow in the main
  loop, measured plus 10 %: its mean a PAL frame (44,828, funktest, 0.93 % of the core at 50
  frames a second) and its dearest slice catch-up (38,809: the 8580, every voice on noise and
  triangle at `$FFFF`, the first slice from power-on). The second is most of
  `PSID_SLICE_WORST`, 40,000, the main-loop pass's 6502 slice with the shadows and the write
  ring, which sizes the slice.
- `PSID_ISR_WORST`, 120: a .sid tune's worst block in the audio interrupt over the chips' own
  render (the stamped writes, the split renders, the mutes and fades). It never runs together
  with the song player's block.
- `SLOTS_SWITCH_WORST`, 964: the effect slots' worst switching block, measured.
- `FREE_RAM_MIN`: the M2 overview's free-RAM row (10,000, each plan's budget counted once)
  less 512 bytes for a .sid tune's own state (it overlays the song's) and 512 for LAB.
  `REGION_FREE_MIN`, 5,120: each region keeps the 4 KiB floor plus 1 KiB.
- `POLL_INSTRUCTIONS`, 120,000: the latency gate's poll allowance, 0.5 ms at the 240 MHz
  clock (the TRS UART is polled at 2 kHz). The main loop drains MIDI between `ui_poll` calls,
  so a longer `ui_poll` would eat into it: every view's worst `ui_poll` stays within it.

### Benchmark gates (sid_budget, fx_budget, slots_budget, scene_budget, cpu6502_budget, psid_budget)

- The benchmark images (`bench/fm1`) run bare in fm1-emu and count instructions between
  `bench_mark` calls; fm1-emu runs one instruction a cycle, so the counts are a lower bound on
  the chip's cycles. Each gate also renders the same workload with the host build and requires
  the target's samples to match it (the SID gate within 1 LSB), so a budget is never met by a broken render.
- `fx_budget`: the post-chip effects with everything on cost at most 500 instructions a sample,
  steady and in the worst case (targets alternating every block so the smoothing never
  settles, the crusher capturing nearly every sample); with two chips at the SID's worst block
  and the tap they stay within `TWO_CHIPS_PLUS_EFFECTS`. The scenario must change more than
  half of the samples, or the comparison would be vacuous.
- The host builds of the benchmark workloads (emu-gate's `src/lib.rs`) share the C workload's
  global state, which is not thread-safe while the tests run in parallel: each render holds
  sidou-sys's workload lock for the whole run.
- `sid_budget`: the M0 budget of the design document's section 8, at most 900 instructions a
  sample per chip, holds for every scenario on both models on average and block by block
  (audio is produced a block at a time; blocks as `workload_render_samples` cuts them, at most
  128 samples and never across an 882-sample frame). Each scenario's host output must peak
  above 1,000, except the near-silent idle scenario, which is there for its cost.
  `SIDOU_GATE_PROFILE=1` counts the render instructions per function and per block (slower).
  pi32v2 mostly issues one instruction a cycle, but XIP loads and divides take longer.
- `slots_budget`: decision record 0004's budgets, Distortion 150, Modulation 200, FX 450 and
  the slot frame 100 instructions a sample, 900 for every slot's heaviest together, with the
  controls moving all the time. While a preset change switches slots (one at a time) a
  32-sample block may cost up to 1,000 (the switching slot's per-sample path and a new
  algorithm's one-off setup), for at most 80 blocks above 900 (two preset changes, about
  30 ms each). The worst switching block plus two chips at their worst block, the tap, the ISR
  and USB audio recording stays within `AUDIO_TARGET`; what is left is the 6502's headroom.
- `scene_budget`: the pixel scene's tile compositor (`app/scene_px.h`) in a frame like RED
  MOON RITE's (13 slots), with its log checked against the host build's (the same runs and
  every tile's hash the same: the device composites the host's pixels). The bounds are about
  10 % over the measurements: a tile 1,959 on average and 4,826 at worst (bounds 2,150 and
  5,300), a cell sent alone 5,646 at worst (6,200), a scan 2,829 a candidate (3,100). A run is
  at most `SCENE_PX_RUN_CELLS` worst cells plus `grid_next_run`, and two runs, 10,000 for the
  rest of a `ui_poll` (its bookkeeping and the window it starts; the view's own gate measures
  the whole `ui_poll`) and a song's analysis slice fit `POLL_INSTRUCTIONS`: a compile-time
  assertion keeps `SCENE_PX_RUN_CELLS` and the cell's cost from growing past it. A scan slice
  (`SCENE_PX_SCAN_SLICE` tiles at the worst with their hash, and the walk over the candidate
  bits) fits it the same way.
- The scene's SPI: each cell is 128 bytes and each window 11 bytes of commands (CASET and
  RASET, 4 bytes each, and RAMWR) at SPI1's 12 MHz (60 MHz / 5); the worst frame SCENE can send,
  every cell a window of its own, stays within about 20 ms of SPI per 40 ms frame
  (docs/art/README.md).
- `cpu6502_budget`: SIDOU's 6502 (the device build of `core/cpu6502`, `CPU6502_SMALL` at
  `-Os`) stepped as pi32v2 code in a bench image. Dormann's functional test reaching its
  success trap is the decoder check: fm1-emu stops on an encoding it cannot decode (decision
  record 0008, Risks), so a finished run means the compiler's dispatch decodes at the pinned
  emulator. The workload is the three costliest of GoatTracker's 14 example songs per 6502
  cycle (the ignored `every_example_song` survey: 8.12 to 8.88), packed by gt2-ref with its
  default options and played for 10 s of 6502 time (501 PAL frames of 19,656 cycles), their
  SID writes equal to the host 6502's. Each opcode is also measured alone, the call's own cost
  taken off; an access through the I/O hooks must cost more than the inline memory access.
- The voice shadow's catch-up is measured in slices of 1,500 cycles, the first estimate of the
  look-ahead's slice; `PSID_SLICE_CYCLES` (1,000) was set from it. The look-ahead's passes are
  measured on the shadow's dearest voices (noise with triangle from power-on, until the noise
  register locks up) on one chip and two with an access every 4 cycles, and on a GoatTracker
  tune. Instructions are not device cycles (docs/hardware.md, "Open checks").
- `psid_budget`: a .sid tune's interrupt path (`psid_ctl_block`: the ring's pops, the split
  renders' steps and their calls' setup, `sid_advance`, `sid_write`, the fades) is measured
  in the 6502 bench (mode 6), each instruction of the block's call told by its function, apart
  from the chips' own work, which the SID's gates bound (the render loops and the voices'
  clocking; the settles a write runs first, the render's per-call setup and OSC3's latch at a
  write are the .sid path's). Its worst block stays within `PSID_ISR_WORST` over the oracle's
  corpus (GoatTracker's examples and SIDOU's songs, 1SID and 2SID) and over synthetic worst
  tunes: an access every 4 cycles for whole frames (about 700 writes a block) to a voice, the
  cutoff (each render part then recomputes the filter) and the volume, and one write on every
  sample (one every 24 cycles, a sample being 22.3: a block at its render limit, 80 entries
  and 23 renders). The bench runs at most 40 main-loop passes before a block; the device makes
  about 30. `SIDOU_PSID_RUNS` picks the runs whose name contains it.
- With a tune playing and LIVE showing (the busiest view, its tap on), each `psid_ctl_poll`
  stays within `PSID_SLICE_WORST` and every main-loop pass within `POLL_INSTRUCTIONS`.
- A tune takes no state of its own outside the song's: audio.c's union fits the song
  transport's and the 6502's 64 KiB is the song's region. The .sid player's code in SRAM
  (psid_ctl's interrupt half, `sid_advance`, audio.c's player paths and its swap hook) stays
  within 1,536 bytes.

### Boot (boot.rs)

- The audio starts before the screen's first frame, and the main loop plays a key held at
  power-on while that frame goes out: the boot holds no sound up. Only the LCD's reset and
  wake-up delays (`lcd_init`, under 400 ms) come first. On the device the first frame is a full
  screen of SPI, about 80 ms; fm1-emu sends SPI without its transfer time, so the emulator
  shows only the order.
- fm1-emu does not decrypt a bare image's XIP, so the plain XIP window over the store (from
  flash 0x93000 up, Felucca's `fl_plain_window_init`) is checked through SFCENC's registers,
  which fm1-emu models, and a byte written into the flash chip read back through XIP.

### Boot guard and watchdog (bootguard.rs)

- fm1-emu resets the chip as the FM-1 does (P33: the watchdog, and PR_PWR, which `fm1_reboot`
  and `fm1_enter_uboot` write): the core starts the firmware again from its entry with the
  chip's registers as the loader left them and RAM kept (the guard's `.noinit` words, the
  boot-state mailbox). A test boot dies as a crash does: the test raises an exception on a
  vector still on its fatal stub, and the firmware's fault handler records it and resets the
  chip. fm1-core does not run the fatal handler's first instruction, `trigger` (the debug
  unit's trace stop, which changes nothing else), so the test steps over it.
- What UBOOT does next is the mask ROM's, which fm1-emu does not have: the tests stop at the
  firmware's reset request (`Machine::stop_at_reset`) and check the request, the
  `usb_update_mode` mailbox and that the boot went no further.
- Felucca 1.6.1's 68-byte crash record overlaps the boot guard: only SIDOU's magic survives
  it (Felucca writes that word only on a second-core crash), so foreign words after a valid
  magic must not count as early deaths.
- The hang build (`-DSIDOU_TEST_DEATH=hang`, `SIDOU_FM1_DEATH_BUILD`) runs hardware checklist
  section 10's first half in the emulator: with OCT- held it hangs 1 s into each boot with the
  interrupts off, the watchdog resets it 8 s after the last feed, twice, and the third boot
  enters UBOOT. In that build `fm1_enter_uboot` is inlined into `fm1_cstart`.

### The decode gate (decode_gate.rs)

- A compiler idiom fm1-emu lacks stops the emulator only when a test happens to run its path
  (the and-not read-modify-write with r8-r15 once stopped the keys test at the first key
  release). The gate walks the ELF's code sections (`.entry`, `.ram_text`, `.text`) function by
  function, asks fm1-core's decoder (`fm1_core::decode_info`, the one `fm1-emu --check-decode`
  uses) for each instruction's length and class, and steps each instruction once on a scratch
  machine whose registers point at free RAM: a class that decodes but has no semantics fails
  too; a fault from the scratch state does not. A dual-issue head is stepped with its tail.
- The jump tables the compiler leaves in `.text`, between its `.GJTIS`/`.GJTIL` and `.GJTIE`
  marker symbols (the names recur per object file, so a start pairs with the next end above
  it), are skipped; so is what follows the last function (`.rodata`, which the linker script
  puts after the code). The boundaries are fm1-core's own, not the vendor objdump's, which
  shows the 6-byte compare-and-branch as unknown and disassembles the jump tables as code. A
  walk that does not land exactly on its function's end or a jump table's start means a length
  disagrees with the compiler's.

### The flash timing spike (flash_spike.rs)

- The spike (`-DSIDOU_FLASH_SPIKE=ON`, docs/hardware-test.md section 12) is a test firmware
  for measuring the device's flash timings. The default image carries none of it: no spike
  symbol in its ELF and not the spike's sector mark in its bytes. In fm1-emu
  (`SIDOU_FM1_SPIKE_BUILD`) every time reads 0 (the model's flash is instant), the page erase
  (0x81) and the suspend are not modelled, and every check is clean. With every byte outside
  the store's last two sectors set to a sentinel (0x5A and 0xA5 alternately, which no erase or
  program leaves alone) none changes, and the guard band of both sectors stays erased. It
  writes nothing over a sector that is neither blank nor its own, or over a damaged pattern.
  The bounds at every address are sidou-pack's `tests/flash_spike_guard.rs`.
- The spike runs with the TRS jack busy throughout and loses no byte, not even across a 35 ms
  interrupt mask (109 bytes, under the 128-byte ring). The test sets the update staging area
  blank, as on a unit after a completed install: the boot's `update_boot` erases a staging
  area it finds in use, which is the update protocol's cleanup, not the spike's.

### The update package (package.rs)

- The package (`$SIDOU_FWSC`, else `sidou.fwsc` in the fm1 build directory) boots as fm1-emu
  boots one (`stock_from_package` and `stock_boot`) and plays sample for sample as the bare
  image does. A package older than the build's `sidou_app.bin` fails the test: it holds an
  earlier build. It writes `sidou_package.json` and `sidou_package.wav` (A4 held from
  power-on) into the build directory.

### Chip B (chip_b.rs)

- The tests drive chip B as the main loop does: its register writes go into the register ring
  in emulated RAM (entries with `REG_RING_CHIP_B`) and its use is `b_want`, both between two
  instructions of the main loop, outside the audio interrupt with nothing pushed or uncommitted,
  so an injection lands whole before the next block.
- Chip B out of use costs the audio interrupt what the one-chip path did, within 1 a sample:
  397.87 instructions a sample with a chord held on LIVE (the tap on), measured before chip B
  could render. The test sets MASTER full, where its gain stage does not run at unity, so it
  measures what chip B costs.
- With both chips at their worst (noise with ring modulation at 0x0800 on every voice), chip B
  in use and every slot's heaviest at full, the worst block stays within the audio target, and
  chip B's mix (`chip_mix`, measured by its own instructions, 8.1) under 10 a sample.
- MASTER's gain stage at a constant gain (the pot's power-on reading, so every block of the
  gates runs it) measured 12.04 instructions a stereo frame, a little over 5 an audio sample
  (a load, a multiply, the rounding add, a shift, a store, unrolled by four); a ramping block
  measures the same. The bound is 12.5.
- CHIP SPREAD's stage (the side, the stage, which converts the slots' output in their place,
  and the slots' dry gain) measured 50.56 a stereo frame with two chips at their worst, every
  slot's heaviest and SPREAD 100; the bound is 51. The interrupt's mean rises by only about 33
  to 35: the slots' own conversion, about 17, does not run then. At SPREAD 100 a chip sounding
  alone stays centred (the stage does not run: mono's output bit for bit); with both sounding,
  each chip's fundamental is more than 30 dB under on the other side.
- Chip B's fades in and out of use step the output by no more than B's own step, 1/128 of its
  sample and 1 for the rounding, at any sample including the block boundaries (a click test on
  the host's render, which the device matches sample for sample). Chip B plays its power-on
  release out where it is made, so going in use it starts from silence.

### The scope tap (ram_audio.rs)

- The tap's cost is measured on the same build by turns: LIVE on top (the tap on) against the
  menu on top (the tap off), a chord held at its sustain level throughout. LIVE behind THE
  OSCILLATOR's title card keeps the tap off until the card ends.

### The effect slots on the device (fx_live.rs, fx_knobs.rs)

- A CC or a key press may reach the audio interrupt up to 8 blocks after it was sent, and a
  later CC of the same burst up to 3 blocks after the first: over TRS a CC takes about 1 ms at
  31,250 baud and a block 2.9 ms, and the main loop commits what has arrived, so a burst
  spreads over a few blocks. The sample-exact comparison tries every way a burst can land. The
  chip plays before the note (its silent voices leak, as reSIDfp's), so the slots' output
  before the note shows which block each CC reached.
- The overdrive test plays a chord (A4, C#5, E5), so the slot's input is loud enough to clip:
  one SAW LEAD note at the 6581 filter's reSIDfp level clips only to 0.82 of its undriven crest
  factor, short of the 0.8 the test asks. The driven take is compared with the slot on at no
  drive and the same tone and mix, so the drive alone counts; the drive's makeup keeps the
  level within 6 dB.
- A dim LED glows a thirtieth as bright as a lit one (the HAL's dim pulse through
  `fm1_led_dim`); fm1-emu's LEDs are multiplexed, so lit is any level above 0.
- Every distortion's Amount, bottom of the knob against the top on one SAW LEAD note at the
  DAC, changes the sound's shape (the spectrum's difference with the level taken out) by at
  least 1 dB and its level by at most 6 dB: measured 1.5 (CRUSH) to 4.1 dB (LO-FI), TAPE 2.0.
  The slot lifts its input to the algorithms' level; without it the Amount was mostly a volume
  knob.

### The chip's character (character.rs)

- A character CC takes effect up to 8 blocks after it was sent (the MIDI drain, then
  `live_tick` every 10 ms, about 3.4 blocks), and a later CC of a burst in the same block as the
  one before it or up to 5 blocks after (the next tick). The profile CC resets the chip into the
  worn 6581; the four controls ramp it there without a reset, a tick at a time, and the spare
  gets them before the swap. The device's ticks are counted from each block's
  `REG_RING_CHARACTER` entries, so the host render ticks where the device did.
- The thump's output stage (the decay of both chips' levels and, per side, the room left, the
  soft limit's test and the rounding) measured 30.86 instructions a stereo frame while a thump
  sounds; the bound is 31. Idle, the stage is a take and one test a block. DAC's grit (the
  noise, the bow and the slew) measured 19.13 a sample on one chip while DAC is not 0; the
  bound is 19.2, and at DAC 0 the render tests two halfwords. Both are outside the SID's worst
  blocks and take their share from the 6502's headroom while they run.
- The device compiles all three pulldown strengths for both models, used by the built-in
  profiles and the expert selector as `SID_PD`'s shared blocks: each profile reads back as
  reSIDfp's table, its blocks are the ones sidou-tablegen lays out, and the pool holds no
  unused block. Each compiled row's pointer (`SID_PD_ROWS`, in RAM because
  `sid_osc_update_mode` reads it in the audio path) is its index row's in `SID_PD`.

### Settings across power cycles (settings.rs)

- fm1-emu keeps the flash across power cycles in these tests: a machine is powered on with the
  flash a previous one left. The settings record is 6 header bytes, then the parameters with
  the cutoff and the pulse width two bytes each (app/settings.h); an older firmware's record
  is written into the flash copy by the host store, as that firmware would have stored it.
- A foreign store reads foreign until the menu's confirm formats it: a turn arms the
  confirmation and further turns, however fast, only keep it armed; SAVE formats, every sector
  erased (forced: the player asked) with the audio held through each, then the settings are
  stored again.

### LAB (lab.rs)

- `LAB_WORST_POLL`, 90,000: the worst `ui_poll` that composes LAB's frame while a key sounds
  and a card turns every frame (the canvas and the drawing by turns). Measured: ENVELOPE 74.7k
  (its drawing about 45k), OSC 61.3k, a page without a drawing 44.9k. The bound keeps room
  under `POLL_INSTRUCTIONS` for the inputs and MIDI; every LAB input with the busiest pass
  after it stays within the allowance too.
- Every result of EDIT + ARP and EDIT + SCL is heard: a C4 held from its note-on stays above
  `LAB_SILENT_DB` at the device's output, read through the host test's DC blocker with the
  MASTER gain at the pot's power-on reading divided out, so it reads as the chip alone does.
- LAB's undo keeps its 16 steps in `LAB_EDIT_ENTRIES` bytes; a roll takes about a dozen, so a
  run of rolls pushes the oldest out.

### Note latency (latency.rs)

- Note-on to sound is measured end to end from the cycle the note-on's last byte arrives at
  the jack (its three bytes take 0.96 ms on the wire before that), along the whole path: the
  jack's UART DMA, the 2 kHz UART poll into the MIDI ring, `midi_drain` onto live, which writes
  the voice's registers into the register ring at once (notes are never held for the 10 ms
  tick, as bend, aftertouch and the wheel are), the commit, then the audio interrupt applying
  the ring at its block's start. Three points: taken (the interrupt that applies the note is
  entered), buffer (it returns, its block written into the DMA buffer: `audio_fill` writes the
  samples at its end) and audible (that sample leaves the DAC, one block later).
- The bounds, for every trial: taken within a block and a poll (plus the USB attach's stall
  around it); buffer within that plus the render allowance, the run's longest audio
  interrupt, which is itself held to `AUDIO_TARGET` a block (decision record 0005's amendment:
  the plan's bound left out the render); audible within taken and two blocks. With the
  envelope INSTANT (the default, each gate edge restarting the rate counter) the envelope is up
  in the block that takes the note and the release's first step in the block that takes the
  note-off. With AUTHENTIC both may wait out the SID's ADSR delay, up to a rate LFSR period
  (32,767 cycles, 33 ms) plus the attack, a block's granularity on top: a release rate's
  period (267 cycles at SAW LEAD's release 6) is longer than attack 0's, so a gate that finds
  the LFSR past it waits for it to come round (core/sid/sid_env.h).
- "Sound" is a deviation from the pre-note baseline (the 20 ms before), 8 LSB past its largest
  deviation, not a nonzero sample: the chip's output at rest is a DC level the DC blocker
  settles, and a gated-off voice still leaks its waveform as reSIDfp's does. The first sample
  past it is the gated voice's oscillator leaking through the DAC at envelope 0 (about 40
  LSB); the note's envelope is taken at -30 dB of the held note's peak, and the release from
  the rendered chip's envelope counter (its first step is one level in 170 in the audio). The
  held note's peak must be at least half the host core's, at the MASTER pot's gain.
- Each trial boots afresh (fm1-emu's state cannot be cloned) and injects the note-on at a random
  cycle in a 40 ms window (a reproducible xorshift), so its phase against the audio block
  (2.9 ms), the poll (0.5 ms), the 10 ms tick and the 40 ms compose is random; the note-off
  140 ms later (the ADSR delay, the attack and SAW LEAD's decay to its sustain), at a random
  cycle in a block.
- The cases: the status screen at rest; busy (K1-K3 turning, every compose drawing); USB (the
  note-on around the main loop's longest stall, the USB retry's attach once a second with no
  host, `usb_probe`); the character's CCs sweeping (age and DAC, so the ramp builds a DAC table
  a step a tick; their bytes go in only while the jack is idle before a note, and since the
  changing DACs change what silent voices leak, the case is measured against the same run
  without the notes, sample for sample: fm1-emu is deterministic); a profile step in flight
  (CC 85 between two profiles faster than either's preparation, which takes at least 7 ticks,
  so the spare is prepared a slice every tick and never swaps in); and a file being saved (the
  store's programs in 16-byte write windows with interrupts masked). Every case also holds each
  main-loop pass to `POLL_INSTRUCTIONS`, since a pass that long would hold a note back as much.
- `SIDOU_LATENCY_TRIALS` sets every case's trial count (64 reproduces decision record 0005's
  run); the default counts keep CI short. Each case writes `latency_<case>_<envelope>.json`
  into the build directory.

### First use (first_use.rs)

- The first-use tests drive the panel as a player does on a fresh song and judge the result
  as an ear would, not sample for sample (the bit-exact tests are song.rs's and
  song_store.rs's): a sound loud enough over its length (RMS over 10 ms, at least 250 at the
  chips' scale at the pot's power-on reading, which a click of a few samples never reaches;
  silence under 40, the tail gate and the thump leave a little), its pitch (the chip's
  frequency register and the audio's own fundamental, over 8,192 samples from 50 ms after the
  onset, past the onset's thump that an estimate over the start takes for a low note), on time
  (onsets at the rows' times) and each instrument told apart from the others.
- Two sounds are told apart by a clear difference in loudness, length (10 ms windows within
  20 dB of the loudest), brightness (the spectral centroid), busyness (zero crossings a
  second), how much holds on (the mean RMS 50 to 150 ms after the loudest, over the loudest),
  how pitched it is (the normalised autocorrelation's highest peak at 2 to 20 ms lags), pitch
  (more than a semitone) or pulse width (a tenth). AD and SR are not part of it: an ear hardly
  hears PULSE's from BASS's on a short note. A digi drum (its voice muted, the sample on the
  volume register) is judged by length and brightness.
- An onset is a 10 ms window over half the floor and at least 1.8 times the quietest of the
  20 ms before it (a rise, not the last note's tail), every 5 ms, searched from 40 ms before
  its row to 80 ms after; it may be half a player frame and a 5 ms window from its time. The
  chip kit's drums sound a frame after their row (their first frame is the test bit with the
  gate off, decision record 0009); the lead, the bass and the digi drums at once.
- PRESETS is followed by a step press 150 ms later, as a hand moves from the knob to the keys;
  the first press after PRESETS must audition the instrument PRESETS picked. The device once
  played the previous one there and fm1-emu does not reproduce it, so the test stays as the
  guard.
- REC from the keys is compensated: the device places a key 6.0 ms before it reads it
  (`REC_LAG_SAMPLES`: the double buffer, less half a block, and the debounce). The test places
  a key a known time before or after a row's first sample leaves the DAC, using song
  control's sample count against the DAC's clock (each audio interrupt's snapshot places its
  block's samples, since the DAC never stops), and sweeps where OFF turns over: at 0 with the
  6.0 ms compensation.
- PLAY on a fresh boot is quiet until the song's first note, 140 ms later: chip B plays its
  power-on envelope release out where it is made, so the keys moving to it make no thud.

### USB and MIDI (usb.rs, midi.rs, sysex.rs, link.rs)

- A 256-byte SysEx DATA packet and its ACK take 46.9 ms up (PUT) and 10.9 ms down (GET) with
  nothing playing; with a chord held on the keys, 263.2 ms up, because the store waits
  `STORE_SYSEX_GAP_MS` (250 ms by default, the build option `SIDOU_SYSEX_GAP_MS`) between two
  packets' flash holds while the keys sound, and 10.4 ms down.
- Recording over USB audio adds 3.0 instructions a sample to the audio interrupt with
  fm1-emu's estimated SIE round trip of 1 us (measured 2.72 at 44.1 kHz, 2.76 at 48), and
  `USB_AUDIO_PER_SAMPLE` with the SIE answering at once, the firmware's own cost (0.73 and
  0.76). The device's real round trip is unmeasured (docs/hardware-test.md section 19), so the
  tests measure both.
- The USB rules the tests hold: packets carry at most 64 bytes (whole event packets), no SIE
  access happens inside the audio interrupt, and the main loop never sleeps in `idle` while a
  host is configured. A device that broke them received packets but a host saw almost none of
  what it sent.
- MIDI input at the jack's full speed may add at most 1.5M instructions a second to the main
  loop on the status screen (measured 1.33M: 2.11M idle, 3.44M busy), 0.55 % of the core; on
  LIVE the main loop's cost with the jack at full speed stays within 6.3M a second (measured
  5.73M, 2.27M idle), where the scopes, envelope bars, note names and meters cost about 84k
  instructions a frame against 14k at rest.
- MIDI out's idle tick looks at every parameter for at most 22 instructions each (measured
  21.6) and drops every unchanged one for at most 46 each (45.6); a tick with every parameter
  changed stays within 4.6k (walked parameters find their CC by halves, about 100 a packet).
- Live Link's tests run on USB frames (`usb_frames`), so their panel presses are timed in
  frames rather than with the panel helpers.

### LCD goldens (emu-gate's `screen`)

- The goldens in `tests/golden/lcd/` are 4-bit indexed PNGs in the art palette
  (docs/art/README.md), the panel's RGB565 truncation of it, so every pixel of the screen must
  be one of the 16 colours. `SIDOU_LCD_GOLDEN_UPDATE=1` rewrites them; a mismatch writes the
  screen as `<name>.actual.png` into the fm1 build directory.
- A PNG is written whole under a name of its own and renamed into place: two tests that write
  the same `<name>.actual.png` (the golden test on a mismatch, the song pages' review test
  always) never leave a torn file for the reader.
- Cells left out of a comparison are painted blue in both images. The version text changes with
  every release, so it is masked: the status screen's (row 2, right-aligned before column 21;
  platform/fm1/ui.c asserts at compile time that "V" and the version fit those cells), the boot
  prompt's banner (row 3), the title card's version line and About's 2x heading.
- A shot is taken half way between two frames (`to_mid_frame`): the panel then holds a whole
  frame, and what follows starts at the same phase every run. `settle` waits 700 ms first,
  long enough for a wipe (8 frames) or a raster (14 frames) to end.
- HOME acts on its release, at the next frame (app/views.c `views_flow`). The wipe and budget
  tests hold it two frame periods (`tap_home`), so the frames that follow are the navigation's
  and the time to the middle of a frame stays where the goldens were made.
- The LIVE reference shot waits for the boot's last rows: the boot can end with rows still to
  go out (a frame sends at most `LIVE_VIEW_FRAME_CELLS`).
- The preset's cutoff is off K1's table: the highlight test turns K1 one detent first, so that
  the turn back lands where it started.
- The C64-theme golden starts its note on the views' frame 139, and About's goldens are taken at
  its animation's frames 9 and 40 (the first page whole: its last line at full brightness from
  frame 26, the heading fading from 80). The tests wait for those frames, so the shots do not
  move when the menu gains items.

### The value popup's budgets

- The popup gate turns K1 once (the popup opens, holds a second and closes: 3 rows, 78 cells, a
  frame each way, within `POPUP_OPEN_FRAMES`, 70), then every other frame for a second, then
  holds it static, over LIVE and SCENE with a note held, STATUS, the menu, CHIP and the MIDI
  page. K1 opens no popup over LIVE and CHIP (they highlight the knob), so the MASTER pot, whose
  popup shows everywhere, stands for it there. A static popup draws its box again only when the
  view under it composed and sends nothing; LIVE, SCENE and the MIDI page move on their own, so
  their windows are not counted. The bounds are about 15 % over the measurement with the
  large text (most cells a frame 132, 158 over RULE 30; worst SPI 11.3 ms, 13.6 over SCENE;
  busiest `ui_poll` 30.7k LIVE, 33.7k STATUS, 36.5k menu, 35.4k CHIP, 31.3k SCENE; a static
  popup's extra a frame 7.3k over LIVE, 12.5k over SCENE, about 0.15k elsewhere):
  `POPUP_WORST_POLL` 42,000, `POPUP_STATIC_EXTRA` 15,000.
- A turn that asks for the popup: the input's call (`ui_encoder`, which stores the ask) within
  `POPUP_INPUT`, 250, and the busiest pass after it (the frame asks the views for the text and
  draws the popup's first rows) within `POPUP_PASS`, 70,000. Measured: K1 218 on LIVE and STATUS
  (234 over the menu, 230 on SCENE), K2 170 to 186, ALGORITHM 166, SELECT 129; the busiest pass
  37.2k on the views that compose little and 59.8k on SCENE. K4 (the preset) loads a preset and
  is no popup's cost (1,185 and a pass of 89k over LIVE). CHIP has its own bound (`CHIP_INPUT`).
- `POPUP_GONE_MS`, 2 s: a popup closes a second after its last detent, plus the three frames
  it takes to close, with room. The gates of what a view itself costs when a control changes
  clear the popup's source (`without_popups`): its hundreds of cells would drown the view's.
- The MIDI page's busiest `ui_poll`, `MIDI_PAGE_WORST_POLL` 23,000, is about 10 % over the
  measurement (20.6k, the wipe that opens it; an edit of MODE 20.3k). The large list stays
  within 190 cells a frame and does not blink.

### LCD budgets (emu-gate's `screen`)

- `SPI_HZ`, 12 MHz: platform/fm1/lcd.c runs SPI1 at 60 MHz / (4 + 1). `SPI_BUDGET_MS`, 20:
  docs/art/README.md allows at most about 20 ms of SPI per 40 ms frame at steady state.
- A frame's cost is counted per compose, not per 40 ms of time: from its compose (ui.c's
  `last_compose`, stamped as the compose starts; its cells go out in later `ui_poll` calls) to
  the next one's. A window of time can hold the tail of one frame and the whole of the next,
  which no frame sends. The stamp is read every 100 us: a compose's own call is longer, so its
  cells are never counted before it. A sent cell is a 128-byte window body, each window 11
  command bytes (platform/fm1/lcd.h `lcd_count`).
- The wipe test samples the panel every frame from half way between two frames and classes
  each cell row as the view under the menu's, the menu's, both, the wipe's white edge or
  neither: every sample must be the menu over the view with at most an edge between them.
- The raster on a chip change is checked on the left border: screen_ramp's bands in order (its
  purple band shows as light grey there) against the status screen's ring in the theme's dim
  colour.
- An idle frame composes nothing (app/screen.h): the views compare what they show with their
  last compose, and the polls between frames (about 390, one per wake-up) return at once with
  nothing to send. Measured 6.6k instructions a frame on the status screen (37k when every
  frame composed); bounded at 7,500. One note on the status screen is one run of two or three
  cells, under 1 ms of SPI, and costs one compose (about 26k) and the run over idle: under
  30,000. The menu blinks its cursor, one cell every 8 frames.
- A wipe sends each row that changes once and its edge row once more (at most the screen and 8
  rows), and renders a full screen (about 580 instructions a cell) and the edge rows: under
  800k over idle. A raster step sends the 116 border cells every other frame. Every
  transition's `ui_poll` stays under 100k.

### The boot and the tour

- The short boot shows the title card for 1.2 s from about 0.4 s in, then an 8-frame wipe to
  LIVE: it ends between 1.6 and 2.2 s.
- HOME held at power-on is held 600 ms: the key scan starts with the interrupts after the LCD's
  init, about 400 ms in, and platform/fm1/main.c reads it 3 ms later. From the full boot's
  first frame (about 400 ms in, app/boot.h): LOADING from 3.0 s to 5.6 s, RUN typed by 6.5 s,
  the title card from 7.0 s to 10.0 s, then the wipe. The shots fall at 4.3 s (the loading
  stripes on the border), 7.2 s (the prompt: rows 7-13 hold READY., LOAD..., SEARCHING...,
  LOADING, READY. and RUN, where LIVE has its second voice), 8.0 s (the title card) and 11.2 s
  (LIVE); the title card's wipe starts 7.0 s in, so its budget is measured from 6.8 s.
- The title card is Metal Chrome's scene (assets/logo/metal-chrome.png) pixel for pixel but for
  its version and key lines.
- The tour's step (`views.tour`): 0 off, 1 the boot, 2 TURN K1, 3 and 5 the gaps, 4 PRESS ENV,
  6 HOLD HOME FOR HELP; each toast waits for its action, 1.6 s after the one before.
- A button during the boot skips it and does nothing else: FX would open LAB and OCT+ step the
  octave, both visible on the screen. HOME not held at power-on is a skip like any other.
- The full boot is measured for 260 frames from HOME held at power-on: every frame after the
  first (drawn whole before the display is on) within the SPI budget. About is measured over a
  full round of its pages (the first, SIDOU's, and the credits, 4.8 s each).
- About sends at most 120 cells a frame on average (the prototype sent 240). It composes every
  frame, so its busiest `ui_poll` is one compose and a run and every frame costs about the
  same. With Cracktro Chrome (a still bitmap at the top, the bars behind the scroller) its
  busiest `ui_poll` measured 72k and 118k instructions a frame with the rendering of its 87
  cells, 3.0M a second (1.2 % of 240 MHz): bounded at 80,000 and 130,000. The full boot
  measured 33k and 37k: bounded at 38,000 and 42,000.
- A wipe frame sends at most `SCREEN_WIPE_ROWS` rows and its edge row; the view under a wipe
  holds still (app/screen.h). The wipe into About is started at every 5 ms phase against the
  firmware's frames. Revealing a bitmap a few rows a frame (all of the title card, About's
  lettering band) costs no more than revealing cells: measured with Cracktro Chrome, About's
  wipe 65k busiest `ui_poll` and 118k a frame, the title card's 30k and 134k (4 rows of its
  scene and the edge a frame); bounded at `WIPE_WORST_POLL` 72,000 and `WIPE_PER_FRAME`
  150,000.

### LIVE's scope and meter goldens (`LiveShot`)

- The LIVE goldens with notes (live-a4, live-chord) are captured as a function of the audio
  alone, not of where the main loop's passes, its frames and the key scan's frames fall against
  the audio blocks, which code changes anywhere move (live-chord flipped between two scope
  phases; the L and R meters moved by a column with commits that changed no level, by moving
  the main loop and the audio interrupt against the frames). Three things decide the picture:
  - which blocks a note's register writes land in: each press goes in right after a scan frame
    closes, so the scan takes it a fixed number of frames later (a press 2, a release 8:
    FM1_DEB_PRESS, FM1_DEB_RELEASE), chosen so that it falls 10 % to 50 % into the block it is
    meant for (the main loop's note-on and commit then have half a block before the next);
  - which blocks the frame's preparation reads (`live_view_prepare`: the newest points, the
    peaks since the last): the shot is a frame whose preparation falls 40 % to 60 % into a
    block, so its own and the last one's (40 ms earlier, 21 % on) are far from either edge;
  - the traces' and meters' hold (a trace that moves by a pixel or less, a meter that drops by
    two steps or less, stays as drawn): it depends on every frame before, so both are let go
    just before the shot (the drawn traces marked as moved, the meters at 0), and the shot's
    frame draws them from its own preparation alone.
  The voices' oscillators run from the machine's boot (no note before), so their phases follow
  from the blocks the notes land in, and the shot is a fixed 517 blocks (1.5 s) after the note.
- fm1-emu's audio block is 128 samples of 5,440 cycles (240 MHz, 24 MHz / 544 a sample); the
  key scan's frame is 11 TIMER5 ticks of 100 us (platform/fm1/hal/fm1_input.h), and
  `fm1_in.frames`, the scan frames closed, sits at offset 276 in its struct.
- The chord's notes (A3, E4, E5, so the scopes show 2, 3 and 4 periods) go in two blocks
  (5.8 ms) apart, more than a key scan frame (about 1.1 ms) and its debounce: each is its own
  note-on, in order, and live's voice allocation, not where the passes fall against the scan,
  puts them on voices 1, 2 and 3. Elsewhere chords held on chip B are pressed 5 ms apart for
  the same reason.
- At rest a scope's trace is a flat line two pixels thick across its 160 columns (the dotted
  centre line under it); a held note stands still, its trace folded by the oscillator's phase.

### LIVE's budgets

- LIVE draws each part of its canvas again only when it changes. Measured (docs/art/README.md,
  "LIVE"): at rest 13.7k instructions a frame (it composes, sends nothing); a chord of saws
  47.5k (5 cells a frame once it holds still); three voices of noise at full sustain 141.2k
  (146 cells a frame: noise moves every scope cell its trace crosses, every frame, so the
  voices take turns at the frame's cells). The busiest `ui_poll`, 60.5k, is a frame of noise's
  compose; the tap is read half way between frames. Bounds: `LIVE_WORST_POLL` 68,000, idle
  15,500, the saw chord 53,000, noise 155,000.
- The chord arpeggio at its fastest rate (1F: a note a frame, a frame being two steps) changes
  its voice's note and trace every frame. Measured 58.5k a frame (the saw chord's three still
  voices 42.5k; noise's 138.3k), the worst `ui_poll` 27.1k, 80 cells at most; bounded with the
  saw chord's margin (11 %) at `LIVE_ARP_PER_FRAME` 65,000. ARP held while K1 turns sets the
  rate, and its release then switches nothing.

### SCENE's budgets

- A SCENE golden is a fixed frame with a known audio state: a chord held for a second from the
  piece's start. The tap runs under SCENE and goes off under the menu.
- A chord's note-ons show on every piece within two frames (the key scan's debounce, then a
  frame). FIRE's idle flame flickers in silence (about half of what a chord moves), so the
  chord must move it at least half as much again; the other pieces calm to a third.
- SCENE is measured with the band playing (an arpeggio note every 4 frames, let go after 3, a
  bass note every 8, let go after 6) and the filter wide open (the busiest PLASMA, the hottest
  FIRE), 150 frames a piece, and the wipe to the next piece with a chord held. The bounds are
  about 10 % over the measurement (docs/art/README.md, "SCENE"), never looser than at first:
  cells a frame (average, most) FIRE 125.0 and 159, PLASMA 138.4 and 200 (the cap), RULE 30 18.9
  and 76, STARS 39.5 and 68, ANT 2.7 and 9; instructions a frame 101.6k, 141.4k, 32.7k, 74.1k
  and 29.3k; SPI a frame at worst 13.85, 17.64, 6.60, 6.16 and 0.80 ms. RED MOON RITE drifts its
  pair a pixel a frame: 133 cells and 1.14M instructions a frame. The busiest `ui_poll` of any
  PETSCII piece is FIRE's (58.9k: its two burns, half way between frames): `SCENE_WORST_POLL`
  62,000. RED MOON RITE's `ui_poll`s render the compositor's runs (two of 8 painted cells,
  bounded by scene_budget.rs) and have their own bound, as has the wipe into it from ANT.
- MIDI learn's frame over SCENE and LIVE comes in at once over a view that composes every frame:
  the view leaves it the cells it covers (app/screen.h `screen_cover`) and its overlays stay
  (`SCREEN_OVER`), so no frame, the frame's first and the one after it closes included, sends
  more than the view's frame cells or the SPI budget.
- A piece starts in the compose of the first frame of the wipe that brings it in (SELECT, or
  the menu's ALGORITHM on SCENE), inside `ui_poll`, not in the input's call (`ui_encoder`).
  The input and a frame can fall in one main-loop pass, between two drains of MIDI, so the
  busiest input and the busiest pass after it (from `midi_drain` to `midi_service`: the drain,
  `play()` and `ui_poll`'s frame with the start) stay within the poll allowance together.
  Measured, the busiest of every piece: SELECT 203 (to RULE 30) and a pass of 60.7k (to FIRE:
  a frame of its burns); the menu's ALGORITHM 170 and 60.7k. Bounded about 10 % over:
  `SCENE_INPUT` 230, `SCENE_START_PASS` 67,000. PLASMA, the costliest start, drew its whole
  canvas in the input's call (48.8k) until the start moved into the frame (32.7k a pass).
- With the title cards on, the menu's ALGORITHM pushes THE SCENE's card over SCENE as well
  (350, within `TITLE_CARD_INPUT`) and the piece starts in the cut after the card; those passes
  are measured from the card's hold.
- SELECT from RED MOON RITE to FIRE clears its painter region, which marks every cell, and the
  busiest pass after SELECT to it, or the menu into it, is one of its `ui_poll`s (its start, or
  two runs of the compositor): `RMR_INPUT` 1,050 and `RMR_START_PASS` 65,000, about 10 % over
  the measurement.

### CHIP's budgets

- CHIP's bounds are about 10 % over the measurement (docs/art/README.md, "CHIP"): idle 6.3k
  instructions a frame (it composes nothing), a knob's detent 7.7k (it builds the page in its
  canvas and copies the two cells that changed; its busiest `ui_poll`, 34.2k, is the build and
  the scan of 900 cells), the busiest 37.4k: a burst of every control over MIDI with a focus
  move and a chip change (its raster and toast), whose first frame fills the frame's cells
  and the rest goes out in the frames after (26.9 cells a frame, 17.62 ms of SPI at worst).
  Bounds: 7,000, 8,500 (a tab switch shares it), 41,000; `CHIP_WORST_POLL` 38,000.
- SELECT, ALGORITHM and a knob on CHIP: the input's call (`ui_encoder`) within `CHIP_INPUT`,
  245, and the busiest pass after it within `CHIP_PASS`, 49,000. Measured: the input 240 at
  most (K1 on DIST, which stores the popup's ask and finds the control's place on its knob
  curve by halves), the busiest pass 44.5k (SELECT from FX back to the chip: three frames
  change colour).
- ALGORITHM on a slot that has never been touched: `live_slot_pick` loads the algorithm's start
  (up to four registers sent from the input, each about 70 instructions through the ring's
  sink), so it has its own bound, `CHIP_PICK_INPUT` 400 (measured 301, 360 and 389 at most; an
  edited slot's, 202).
- A profile step (ALGORITHM on the chip, as LIVE's ALGORITHM, CC 25 and CC 85 do) prepares the
  spare chip a slice a tick (app/reg_ring.h `reg_ring_prepare_step`; decision record 0006,
  "Cost") and swaps it in once it is whole, so its passes are the others' plus one slice
  (`REG_RING_PREPARE_SLICE`, the most one slice may take, measured at full character onto every
  profile), within the poll allowance like every pass. A pass runs one slice however many ticks
  it catches up (`reg_ring_pass_end`, platform/fm1/main.c `TICK_CATCH_UP`), and no ramp table
  step in it, so the busiest view's pass and a slice fit together.
- The chip reset test sets the character to full (age, DAC, thump and bleed at 127) and waits
  2.5 s: the DACs' ramp, the longest, takes 1.72 s. It counts the slices of each reset against
  the host's `sid_character_prep_slices`; the first preparation onto the 8580 spends
  `SID_DAC_BUILD_STEPS` more on its default DACs (`default_8580_spent`). CC 85 spans 0..12 over
  0..127 (the profiles, then USER 1..8). Only live_init's reset is unprepared.
- The DAC sweep test moves CC 87 every 10 ms while CC 85 steps R4AR to WORN and back on LIVE with
  a chord of noise held (its busiest view), at several phases of the sweep against the step:
  no pass runs a slice and a ramp table step together.

### Title cards and the help card

- The cards' setting defaults to ONCE (THE CHIP and THE SCENE the first time each, never THE
  OSCILLATOR); the tests that need every card set ALWAYS in the views' RAM, the menu's walk to
  it being the host tests'. THE OSCILLATOR's budget is counted from the frame after HOME is let
  go, so its tap is not waited out.
- A card's held frame is shot a frame after its phase reaches `TITLE_CARD_HOLD`, half way
  between two frames, and compared with its golden and with its source PNG
  (assets/logo/title-<name>.png) pixel for pixel.
- A card from its start to the end of the cut under it (the glow, the wipe, the 1 s hold, the
  cut) takes 60 to 72 frames. Bounds about 10 % over the cards' measurement: 66 frames, 72.4
  cells a frame (200 at most: the glow's first level, and its scan of the screen's 900 cells
  for those that differ), 17.14 ms of SPI at worst, 55.1k to 56.0k instructions a frame, busiest
  `ui_poll` 71.2k to 71.6k, the input 350 to 387 (`ui_encoder` for CHIP and SCENE, `ui_buttons`
  for HOME), the busiest pass 72.8k to 73.1k: `TITLE_CARD_WORST_POLL` 79,000,
  `TITLE_CARD_PER_FRAME` 62,000, `TITLE_CARD_INPUT` 430, `TITLE_CARD_PASS` 80,000.
- Any control on a card ends it at the next frame and does nothing else (FX would open LAB from
  LIVE, OCT+ step the octave, SELECT pick a piece); a note on a card plays at once, from the
  keys or MIDI, and ends it.
- The help card shows once HOME has been held about 600 ms (the test accepts 560 ms on, at the
  frames' granularity, and a second and a half at most); it is drawn whole in six frames
  (400 ms). Letting HOME go leaves it; K1 closes it and does nothing else; SELECT turns a card
  of two pages. HOME tapped still acts on its release.

### The song views' budgets

- TRACKER idle composes nothing. The header and the heads are built again when they change
  (REC, the meters), the rows when the window, the bar, a pattern or a mute does (a row step:
  the build reads 25 rows of three patterns), and only the rows built are compared and copied.
  Measured with SIDOU LOOP and bounded at most 10 % over: idle 7.2k instructions a frame (the
  song pages' idle frames, 6.7k, share the bound), REC on 9.2k (26 cells at most), playing in
  BAR 24.2k (158 cells, 13.8 ms of SPI), GLO and a key 9.0k: `TRACKER_IDLE_PER_FRAME` 7,900,
  `TRACKER_CHANGE_PER_FRAME` 10,100, `TRACKER_PLAYING_PER_FRAME` 26,600. `TRACKER_WORST_POLL`,
  33,000, is 7 % over its busiest user, the song pages' encoder redraws (INSTR's PRESETS,
  30,744).
- With REC on TRACKER's cursor, and so the bar, stays where it is, so the playing measurement
  turns REC off first; otherwise it measures a still view. The song plays on under REC (its
  calls go on), which keeps the REC golden still.
- The inputs (`ui_buttons` for SEQ, REC and PLAY; GLO's key goes through the note keys' pass,
  `ui_note_keys` being inlined into `input_poll`'s caller, so only its pass is measured) and the
  busiest pass after each: SEQ from LIVE 3.5k (the push: the wipe's set-up over LIVE's
  overlays) and a pass of 45.1k (the wipe's first frame: the whole canvas built and copied);
  PLAY 2.0k (`song_ctl_play`) and 24.1k; SEQ in TRACKER and REC 0.4k and 22.8k to 34.9k; GLO's
  key 26.5k. `TRACKER_INPUT` 4,600, `TRACKER_PASS` 55,000.
- STEP is bounded about 10 % over its measurement (docs/art/README.md, "STEP"). The step keys go
  through the note keys' pass, so only their pass is measured.
- The playing runs, 30 s each, use SIDOU LOOP (tempo 6) and a tempo 3 song (a row every 3 calls,
  251 BPM: every voice a note on every row of a 64-row pattern), in BAR and SCROLL (the views'
  TRACKER ROWS, set in RAM; the menu's path is the host tests'). Measured (docs/art/README.md,
  "TRACKER"): SIDOU LOOP in BAR 27.6k instructions a frame, 129 cells at most, no frame at the
  cap; in SCROLL 51.0k, 256 frames at the cap; the tempo 3 song in BAR 49.8k (38 frames at the
  cap: page turns), in SCROLL 89.9k; busiest `ui_poll` 38.4k. BAR keeps every frame within
  `SONG_VIEW_FRAME_CELLS` and the SPI budget; SCROLL holds to the cells as well, and what does
  not fit waits (the frames cut short, `song_view.held`: its lag). Bounds: BAR 55,000, SCROLL
  99,000, `TRACKER_PLAY_WORST_POLL` 42,000.
- TRACKER's row 0 is window row 4 (the page starts at row -4): its note and instrument are 2x
  from cell row 11 (each character's top left piece, its character in the grid's `src`), the
  command small on row 12.
- A tracker compaction makes at most two moves a block, with a preparation and an
  acknowledgement poll each, so it completes within 4 frames a block and 4 more.
- Every view's `ui_poll` and input bounds are checked against `POLL_INSTRUCTIONS` at compile
  time: a compose that grows past it fails here, not as a late note somewhere else. A pass that runs a slice of the song's analysis or
  fast-forward (`song_ctl_poll`, never in a pass that ran a chip preparation's slice,
  platform/fm1/audio.c `audio_song_poll`) and the busiest view's work still fit; song.rs
  measures the slices against `SONG_SLICE_INSTRUCTIONS`.

### UI sounds

- A UI sound plays on chip B and marks it in use with `b_want`'s UI bit (platform/fm1/audio.h
  `AUDIO_B_UI`). The menu-move blip is C-6 (1,046.5 Hz) on a triangle; the test reads the DFT's
  magnitude at that pitch over the first 4,096 samples of the move against the same run with
  SOUNDS off, after 800 ms for the SOUNDS confirm's own sound and its release.
- With three notes held on chip B (a song's hook on, so the keys play chip B) a blip finds no
  free voice: it does not sound, and the output is the SOUNDS-off run's sample for sample.

### RED MOON RITE

- RED MOON RITE is measured over a minute of the demo song: cells a frame against the
  prototype's 117 on average and 176 at most, SPI, the main loop's instructions a frame, the
  busiest `ui_poll` and pass; then, on runs of 20 s each (fm1-emu is deterministic), the busiest
  call of each kind a `ui_poll` makes for it: a scan slice (`scene_view_idle`), a run rendered
  (`grid_render_run`, two a `ui_poll` at most) and the costliest tile (`scene_px_tile`).
  Measured: 98.8 cells a frame on average and 170 at most (the plan allowed 180), 8.7 ms of SPI
  a frame and 15.0 at worst, 904k instructions a frame, the busiest `ui_poll` 56.4k, pass 57.5k,
  scan slice 56.3k, run 24.6k, the costliest tile 5,930. That tile is over the bench's
  (scene_budget.rs `TILE_WORST` 5,300: two fighters, the spark and the arc over the arena), so
  the run and slice caps are 4 and 8 cells (app/scene_px.h): two runs, or a slice, and a song's
  analysis slice fit a pass. Bounds about 10 % over.
- The demo song's fourth bar ends in the fill (about 8 s in, at 6 frames a row): the spinning
  attack lands and knocks the sword from his hand, then the sword sticks in the floor; the
  goldens are taken before the sword's recovery. Voice 3 is the drums: with its bit set in the
  interrupt's mute mask (`song_ctl.mute`, as a mute command sets it) the chops and parries stop.

### USER profiles (`user_profile.rs`)

- The character's DACs ramp over 1.72 s, so the test waits 2.5 s after any profile change before
  it reads the instrument.
- CC 85 spans the five profiles and the eight USER slots in steps of 128/13: 43 is the worn
  6581, 70 is USER 3. A CC takes effect within 8 blocks of its arrival, the window the host
  render's search covers.

### The store on the device (`store.rs`)

- fm1-emu's flash is instant: its operations take no time and never tear, so a power cut is the
  flash as it is after the k-th operation. The cut test checks after every operation of a
  settings save, every third of a file replaced (with the collection and erases it leaves) and
  every 24th of a 64 KiB file, each plus the last; the host store mounts and checks each dump.
- The program timings in `store_fm1.h` are provisional, the P25Q80H datasheet's maxima. No
  write window fits them, so programs are held operations; the tests that need windows set
  `store_fm1_window` in RAM.
- The tests drive the store through `store_fm1_hook`, written in emulated RAM as a debugger
  would; nothing on the device writes it. They mirror `store.c`'s private enums (the jobs and
  their phases, the mount's phases) and `store_fm1_state`'s layout: a change there is mirrored
  in the test.
- A boot runs 400 ms before a test reaches the store: `lcd_init`'s reset (150 ms) and wake-up
  wait (120 ms), during which the store mounts, recovers and fills its pool, then the main loop.
- `STORE_POLL_WORST`, 30,000 instructions, is the store's share of a main-loop pass: each kind
  of `store_poll` work's cost a unit, measured on polls of at least 256 units, times
  `STORE_POLL_WORK` stays within it.
- A hold fades the output to silence over two halves (256 frames), which play after the
  operation; a DAW's clock (F8 at 120 BPM) and active sensing do not keep the store from
  going quiet, a CC every 100 ms, a held chord and a playing song do.
- The peak scan turns on at maintenance's next ask (`WANT_MS`, once a second); the test waits
  1.3 s, a full period and a little over, whatever phase the last write left it in, and holds
  a chord throughout so that no hold changes the cost it measures (2 instructions a sample at
  most).

### .sid tunes against the host (`psid.rs`, `psid_views.rs`)

- The DMA plays the zeroed buffer before the first half the audio interrupt fills, so a
  comparison with a host render tries up to four leading silent halves.
- Under a 1SID tune the keys' registers reach chip B in the block it goes in use or one either
  side; the comparison tries the three.
- A CIA tune is made from a GoatTracker example by setting song 1's speed bit: the KERNAL's
  CIA 1 latch, which the tune's init leaves as it is, times it.
- The tests read the song back from `__song_region` byte for byte into a host `Song`: the
  model holds offsets, not pointers, laid out alike on the device and the host. Its file
  leaves out the free arena, as the store keeps it.
- A tune whose play call never returns stops after one second of 6502 time with PLAY HUNG;
  the keys go on playing on chip B.

### .sid files into the store (`psid_store.rs`)

- The tests send `sidou-sysex`'s frames into the emulated TRS jack, a second after each frame's
  last byte on the wire (320 us a byte), as docs/reference/sysex.md asks of a sender over TRS.
  The 64 KiB tune goes faster, each frame once the last is programmed (the transfer back in
  UPLOAD with the next sequence and the store's staging empty, or idle once END commits): the
  fastest a sender may go.
- The crafted refusal is FM-1 SID's bounds bug (docs/upstream-reports.md, row 1): a data offset
  past the file's end.
- Loading a 64 KiB tune, `sid_io_poll` clears the tune's memory in four passes of 16 KiB, then
  copies the data 2 KiB a pass; each stays within `STORE_POLL_WORST`.
- The starter pack takes at most a quarter of the store's bytes and leaves at least 20 of its
  keys free.

### The SID views (`psid_views.rs`)

- `SIDS_WORST_POLL`, 39,500 instructions: the SID views' busiest `ui_poll`, measured 35,909
  (the browser's first frames as it opens from the menu, its canvas built whole and copied; a
  page read 35,489; NOW PLAYING as a tune plays 18,048) plus 10 %. A compile-time assertion
  ties it to `POLL_INSTRUCTIONS`, as every view's is (`screen.rs`).
- `LIFT_CONVERT_WORST`, 40,000: `psid_lift_convert` runs in a main-loop pass of its own
  (`lift_pass`), so it stays within the slice a tune's pass may take (`PSID_SLICE_WORK`).
- A frame's cells come from `lcd_count` between two composes: each LCD window costs 11 bytes
  and a cell 128; the song views' frames stay within 200 cells.
- GLO pressed again within 450 ms latches its layer (a double tap), so the test waits that long
  after each GLO chord.
- With voice 2 muted, a solo of voice 3 wins: voices 1 and 2 are muted on the chip.
- PLAY on any view stops a playing tune; STATUS's meter (the last four bytes of its look: the
  CPU share in tenths of a percent and the ticks a thousand cycles) then reads 0.
- A sound edit on the built-in loop (PRESETS, a knob) lives in the song's trailer (KEYS, SLOT),
  so the test compares the loop's GoatTracker part alone after a tune.

### Song files in the store (`song_store.rs`, `song_bank.rs`)

- The tests drive song I/O through `song_io_test_hook` (`main.c`: op, a 17-byte name, the
  result after padding) and row edits through `song_test_hook`, both written in emulated RAM;
  the main loop takes a request and clears its op.
- A broken file is the song with its last 17 bytes cut: its headers are valid, so the load
  fails only after the reader has overwritten RAM, and the previous song must be restored.
- A byte flipped in a stored song's GoatTracker title after the mount is store corruption
  (FILE CRC on the next load), not an edit.
- A song SAVE during playback forces held programs; LIVE's SAVE meanwhile is a separate
  settings transaction with its own force permission, and neither stops the song.
- On a flash part the store does not know, `store_fm1_boot` leaves the store unmounted (the test
  unmounts it after the boot): SAVE fails within 500 ms and leaves no freeze, hold or force.
- A pitched note crosses zero hundreds of times a second; an envelope on an oscillator whose
  frequency is never set only thumps, so the NEW song's audition and playback are checked by
  zero crossings and RMS.

### The song player on the device (`song.rs`)

- At PLAY the device puts chip B in use at once and the keys' writes reach it at the main-loop
  pass's end, so a host comparison tries the rewrite in the block chip B goes in use and one
  either side.
- `PLAYER_WORST_BLOCK`'s gate runs 193 benches, each in its own fm1-emu: every example song and
  subtune at multispeed 1 and 4 for 3,000 calls (a call every 880 / multispeed samples),
  each first subtune locked (five locks a row, filter and pulse, up to `SONG_LOCKS_MAX`) and
  chanced (a 50 % chance on as many rows), and 40 generated songs within the device's caps;
  SONG_EDITS_PER_BLOCK name-cell edits every block, swaps on a quarter of the runs (they cost
  less). The benches spread over every core and their results are taken in order, so the
  maxima and the worst's name are a sequential run's: 14 minutes on one core, under 4 on ten.
  The isolated row-apply cost is added to the worst block. The song player and the 6502 never
  run together, so the 6502's headroom is what the audio target leaves after two chips at their
  worst, the tap, the ISR, the slots' worst switching block and the player.
- K1 turns STEP's cutoff lock 32 a detent from the middle, 1024: 30 detents down is 64.
- The darker-bar checks measure the power above 1.5 kHz over 8,192 samples of the whole mix
  (the bass and drums are not filtered) in the locked bar's middle and two bars on: the locked
  bar under 0.85 of the unlocked, two bars on within 5 %.
- A song's own `$D418` writes (volume 15) would override a chip-volume MASTER; MASTER is a gain
  stage after the chips, so the song scales with the pot along the audio taper and at full
  MASTER is the song's own output, which the bit-exact tests compare with.
- REC timing: a play starts from a fast-forwarded hand-over, so the song's call count does not
  begin at its first row; the test counts calls from the first row the player starts after
  PLAY. A key pressed in a row's first half lands on that row, in its second half on the next
  (tempo 6: six calls a row).
- The song-view mute (GLO and the first key) reaches the interrupt within a UI frame (40 ms).

### Live Link

- The host side of FM-1 Live Link (`host/crates/live-link`, the protocol in
  docs/reference/live-link.md) has no dependencies and no I/O: the bytes come and go through
  the caller (Web MIDI in a page, the emulator's USB host in the tests), so it compiles to wasm
  as it is. Its reference decoder rebuilds the device's screen from what it streams and draws
  it as the device's LCD shows it.
- The decoder accepts only the canonical packing (a group's mask of the high bits, then the low
  seven bits of up to seven bytes): a group with no byte after its mask, a mask bit with no
  byte, or a byte with its high bit set is refused, and a frame whose checksum does not sum to
  0 modulo 128 is rejected and counted.

### The ergonomics harness

- The harness (`host/crates/ergo`, docs/TESTING.md "The ergonomics harness") drives the
  firmware in fm1-emu and never touches a USB or MIDI device. It includes emu-gate's `common`
  by path rather than copying it, so both drive the same machine; its runs are independent,
  each in its own `Machine`, and spread over every core with the results kept in the items'
  order.
- It is deterministic: nothing reads a clock or a random number, so one journey captured twice
  gives the same frames, audio and sidecar byte for byte, and docs and videos regenerate only
  when the UI changes. `--capture full` stamps each audio sample from the cycle its DMA half
  finished playing (`half_played`), so frames and audio stay in exact sync.
- An idle screen composes nothing, so its last frame never gets 20 ms old: after 80 ms of
  looking for the mid-frame point, the harness takes the phase as it is. Expectations are
  polled every frame for up to about 3 s (a toast or a popup shows within a few frames, a wipe
  or a card takes longer).
- Every threshold lives in `config.rs`, overridable with `--config FILE`, each from one of three
  sources: psychophysics (textbook just-noticeable differences, rounded), a standard (WCAG,
  ISO) or this project's judgement, to be tuned in review:
  - Readability: the FM-1's lit screen is the player's measured width (240 px across it, 0.0975
    mm a pixel). Play-time text is at least the 2x font, 1.56 mm: ISO 9241-303 and common
    guidance ask 3 to 5 mm at arm's length for handheld displays, so this is already the
    compromise the screen forces, and it keeps play-time text off the 0.78 mm 1x font.
    Contrast follows WCAG 2.x AA, normal size for 1x, large for 2x and 3x.
  - Knobs: pitch JND 3 to 6 cents for a trained listener on a steady tone (Moore); a cutoff
    step worth a detent at about a quarter tone on a sustained saw; level JND about 1 dB
    (Jesteadt et al.); brightness (spectral centroid) 5 %, about 85 cents, with a 6 % noise
    floor for dead zones (fixed-note captures jitter that much); timbre 1.5 dB RMS between log
    spectra, above what the same note differs by from one window to the next, because centroid
    and level miss tone changes the spectrum shows. A knob needs 20 to 100 distinguishable
    positions (no step under 1 % of the range); a cutoff step over a third of an octave sounds
    stepped; a level step over a halving (6 dB) is a jump; step sizes spread over 0.5 (std over
    mean) mean some are twice others. A K1 sweep covers at least three octaves of cutoff, or it
    cannot open and close a sound. A full sweep takes at most 64 detents (under three turns of a
    24-detent encoder, a sweep a hand makes in a second).
  - Tasks: a common task needs at most three actions (the UX pass asked two) and few menu
    levels; the first changed pixel comes within about 100 ms of an input, which still feels
    immediate (Nielsen, after Miller), at the UI's 25 fps; the value popup opens 3 rows a
    frame, so it is due within a few frames.
  - Musical defaults: each factory preset's held note is neither buried nor hot, unclipped,
    without DC and within a reasonable brightness (hard-sync and ring-modulated leads are
    allowed brighter). A percussive preset is judged on its attack and what follows, 20 to
    420 ms after the key: it has decayed by the end of a long note by design.
