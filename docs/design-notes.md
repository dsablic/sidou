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
- `STORE_POLL_WORST`, 30,000 instructions, is the store's share of a main-loop pass ; each kind of `store_poll` work's cost a unit, measured on polls of at least
  256 units, times `STORE_POLL_WORK` stays within it.
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
