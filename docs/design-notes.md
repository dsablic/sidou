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
