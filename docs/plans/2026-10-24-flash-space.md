<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Flash and RAMTEXT space: where it goes, what to reclaim

**Done for 0.4.0** (branch `space-reclaim`, on `v040-batch2` at `dde85461`, which has the drums
and the MIDI export): the lossless set below, measured on the FM-1_900 build as the merge check
builds it. Outputs are bit-identical (no golden changed; the device plays sample for sample as the
host renders it), and every instruction gate the changes reach passes.

**S1 reverted (2026-10-24): RAMTEXT does not use the debug area yet.** It was reverted when a
device report (STEP's toggles not landing) was blamed on it; the report was a misread view, not
the SRAM, so S1 is unproven rather than failed: a device check of code in that area (hardware-test
section 17) would bring its 8 KB back. Code stays out of 0x01C0C000..0x01C0E000: RAMTEXT is 48 KB again, `app.ld`
asserts `_rt_end <= 0x01C0C000` (`ram_audio.rs` checks it too), and crt0.S saves Felucca's five
boot words there again. The 80 bytes RAMTEXT was over came back from `core/player/song_ctl.c` at `-Oz` (112 bytes
less than `-Os`; the digi drums' loop still costs 9.47 a sample): RAMTEXT 49,120 of 49,152. (The
player, `gt2_play.c`, at `-Oz` would save 336 but puts a fast-forward's six calls at 39,432
instructions, over the 39,000 slice.) The S1 rows,
the debug-area section and the recommendation below are the study as it was; where they say
RAMTEXT gains 7,936 or 8,192 bytes, that gain does not exist.

| Change | Image | RAMTEXT used (of) | RAM region free | POOL free |
|---|---:|---:|---:|---:|
| Base, `dde85461` | 497,176 | 49,152 (49,152) | 6,548 | 12,160 |
| S2: noise tables built at boot into POOL | 494,196 (−2,980) | 49,152 | 9,688 (+3,140) | 9,024 (−3,136) |
| 6: `store.c` at `-Oz` | 492,628 (−1,568) | 49,152 | 9,688 | 9,024 |
| 4: pulldown repacked (overlapping blocks, shared NP rows) | 486,652 (−5,976) | 49,152 | 9,552 (−136: `SID_PD_ROWS`, 120, and alignment) | 9,024 |
| S1: RAMTEXT over the debug area, crt0's boot words dropped (reverted: see above) | 486,620 (−32) | 49,152 (57,344) | 9,552 | 9,024 |
| 1: cutoff tables delta-coded, an inline sequential reader | 474,636 (−11,984) | 49,152 (57,344) | 9,552 | 9,024 |
| S3: digi codes two a byte | **472,224** (−2,412) | **49,184** (57,344; +32) | **11,992** (+2,440) | **9,024** |

In all: the image **−24,952** (33,776 under the 506,000 ceiling), RAMTEXT 8,160 bytes free
(was 0; with S1 reverted it is 48 KB and `song_ctl.c` at `-Oz` leaves 32 free), the RAM region **+5,444**, POOL −3,136, free RAM together 21,016. The pulldown tables are
52,407 bytes (was 58,488), the cutoff codes 15,978 (was 28,672), the digi codes 2,431 (was 4,860).

What moved against the study's estimates: the cutoff reader is inline (`sid_cutoff_seq_hz` in
sid_filter.h, its block seek out of line), 448 bytes more than an out-of-line reader, because the
out-of-line one slowed the character jobs enough to move a profile change's place in the boot's
timeline (character.rs's thump stage measured 32.14 against its 31 bound; inline it is the base's
30.86 again). The pulldown row choice is a 30-entry pointer table (`SID_PD_ROWS`, `.data`), not a
byte map. The digi drums' interrupt loop holds a code's level up to a pointer to the next code's
sample and clamps with min and max, which pays for the nibble's choice: 9.47 instructions a
sample while one plays (10.42 before), so `DIGI_PER_SAMPLE` tightens to 9.5. The boot words in
the debug area were dropped, not moved: nothing read them. The noise tables are built in the
LCD's reset wait (`while_lcd_resets`, before the audio starts), not in `fm1_cstart`: there their
tens of thousands of instructions moved the boot's timeline, and with it three screen goldens (two
SCENE pieces' animation phase, LIVE with chip B), which in the wait stay as they were.

Gates run on the final build (all exit 0): emu-gate `character`, `sid_budget` (worst blocks
703.08 and 1,414.79 against 711 and 1,426), `fx_budget`, `slots_budget`, `chip_b`, `boot`,
`live`, `ram_audio` (RAMTEXT 49,184 of 57,344; the tap 3.81), `flash_budget` (33,776 under the
ceiling), `latency`, `song` (the player's worst block 47.4 of 60; the digi 9.47; RAM region
11,992 and POOL 9,024 free), `screen` (every golden unchanged), `package`, `usb`, `store`,
`song_store`, `settings`, `user_profile`, `bootguard`: 208 tests. The host workspace (2,332
tests, the goldens with them) in release and under UBSan, fmt, clippy, tools/gcc-check, both
firmware builds, sidou-pack and its tests, REUSE. `IMAGE_CEILING` is left at 506,000
here: lowering it by what was reclaimed is the merge's call (bounds only tighten).

Device checks: docs/hardware-test.md section 17 (a build whose RAMTEXT reaches into the old debug
area: boot, a store write, a USB update round trip).

---

Measured on main at `9b713e5d` (FM-1_900 build, as the merge check builds it); RAMTEXT and SRAM also on the drums branch (`d19d5944`). The image
`sidou_app.bin` is **490,492 bytes**: 15,508 under M3's start ceiling (506,000), while M3
alone wants about 28,000. RAMTEXT holds 48,976 of its 49,152 bytes (176 free); M3 wants
about 2 KB of it.

Method: the link was rerun with `-Map` and `--print-gc-sections` (byte-identical ELF), sizes
taken per input section, and each candidate was prototyped in a scratch build where practical.
Prototype edits were reverted; nothing was committed.

## Where the bytes are

| Category | Bytes | Share | Largest items |
|---|---:|---:|---|
| UI views, widgets, strings | 113,822 | 23.2 % | views 20.5 K, live 11.6 K, live_view 10.8 K, song_pages 9.5 K, grid 9.4 K |
| Art and font | 74,272 | 15.1 % | RED MOON RITE art 41.7 K, title cards 19.8 K, Metal Chrome logo 9.7 K, font 2 K |
| Combined-waveform pulldown (`SID_PD`) | 58,488 | 11.9 % | 30 rows: 15,360 index, 43,128 block pool |
| RAMTEXT image (copied to SRAM) | 48,976 | 10.0 % | sid 15.9 K, fx_slots 14.5 K, gt2_play 5.2 K, Felucca flash driver 2.8 K, song_ctl 2.6 K |
| Song engine (gt2, transport) | 40,733 | 8.3 % | gt2_song 7.8 K, song_ctl 7.2 K, gt2_stream 6.0 K, gt2_shape 5.9 K |
| Audio engine, flash side | 31,064 | 6.3 % | sid_character 12.6 K, sid 4.5 K, fx_slots 3.8 K |
| Filter cutoff tables | 29,120 | 5.9 % | `SID_CUTOFF_HZ_6581` 20,480, `_8580` 8,192 |
| Store, SysEx, settings | 25,751 | 5.3 % | store 12.3 K, song_io 4.6 K, sysex 4.4 K |
| MIDI and input routing | 18,436 | 3.8 % | midi_route, input, midi_sync, midi_out |
| SCENE, title and logo code | 17,743 | 3.6 % | scene 6.9 K, scene_rmr 3.2 K, scene_px 2.6 K |
| Platform (boot, main loop, LCD) | 15,904 | 3.2 % | main.c 13.2 K (`fm1_cstart` alone 3.8 K) |
| Felucca vendored (USB, HAL), flash side | 9,344 | 1.9 % | not a target (vendored byte for byte) |
| Other SID tables | 6,805 | 1.4 % | `.data` noise tables 3,268, expert grid 2,640 |

By section: code 253,909, read-only data 183,892, RAMTEXT 48,976, `.data` 3,681.
String literals total only 6,926 bytes (credits 2,071, views 1,050). Debug and test hooks
add little: `--gc-sections` already drops 114 unused sections, and no kept function is
debug-only beyond a few hundred bytes (`fm1_fault_c`, `usb_probe`). Dead code is not a lever.

Two documents are out of date: LICENSING.md gives the pulldown tables as 39,256 bytes (they
are 58,488 since the 8580's three profiles), and docs/TODO.md's "generate the active profile's
rows into POOL" assumes about 115 KB of POOL free. POOL has 12,160 bytes free and RAM 11,696,
so a 30,720-byte row set for each model in use does not fit; the item should be closed.

## Candidates, by bytes saved per unit of risk

Risk: 1 = none expected; 2 = low, needs a targeted gate run; 3 = cycle gates likely to bite;
5 = product decision or blocked by RAM. "Measured" means a prototype build.

| # | Candidate | Saves (bytes) | How measured | Risk | Cost |
|---|---|---:|---|:-:|---|
| 1 | **Cutoff tables delta-coded**: blocks of 32 codes, a 16-bit first code then signed byte deltas, `0x80` escaping an absolute code; 16-bit block offsets | **12,608** | build; decode compared against all 14,336 original codes, 0 mismatches | 2 | A lookup walks up to 31 deltas, about 90 instructions on average. That is fine for `sid_init` (~0.6 ms) and LIVE's two lookups a frame. The character job steps do 128 to 192 lookups a step, so they need a sequential cursor (the loops run `fc` upward) to keep their gate. Generator change in `sidou-tablegen fit-filter`. |
| 2 | **`-Os` on cold main-loop files** (group A): `sid_character.c`, `sid_dac.c`, `live.c`, `screen.c`, `chip_ctl.c`, `keys_route.c`, `midi_learn.c`, `about.c`, `panel.c`, `song_edit.c`, `midi_map.c`, `boot.c`, `help_view.c`, `title_card.c`, `platform/fm1/ui.c` | **10,208** | build | 2 | Code only; no RAMTEXT change. `-Os` keeps IEEE semantics (`-ffp-contract=off`, no fast-math), so output is bit-exact. Instruction gates still to run: `character.rs`, `screen.rs`, `live.rs`, `boot.rs`. Same pattern as the files already at `-Os` ("each file's size build keeps every emulator gate"). |
| 3 | **Noise tables built at boot**: `SID_NOISE_JUMP` (2^k-shift matrices by squaring) and `SID_NOISE_WB_BYTE` in `.bss`, not `.data` | **2,980** | build; generated tables compared, 0 mismatches | 1 | Same RAM; about 50k instructions once at boot. Call it once (platform boot and the host `sid` setup), not from `sid_init_begin` as the prototype did. `sidou-tablegen` keeps the table as a test oracle. |
| 4 | **Pulldown repacked**: blocks overlap where one ends as another begins (offsets are already byte offsets), and the 6581's three NP rows (4, 9, 14) and the 8580's (19, 24, 29), which are identical, share one index row | **~5,700** (3,690 to 4,033 overlap, plus 2,048 for the shared rows minus a 30-byte row map) | computed from the generated data, not built | 2 | Generator change. `sid_pulldown`'s ISR read path is unchanged. Choosing a row goes through a 30-entry row map (in `.data`: the ISR's `sid_osc_update_mode` reads it). Values are identical, so goldens and the OSC3 profile tests stay put. |
| 5 | `-Os` on render and latency files (groups B and C): `grid.c`, `scene.c`, `scene_px.c`, `scene_view.c`, `live_view.c`, `widget.c`, `main.c`, `input.c`, `lcd.c`, `midi_out.c`, `midi_rx.c`, `midi_in.c` | 12,656 (main.c 3,773, grid 2,662, scene 2,078, live_view 1,932) | build (A+B+C: 467,628) | 4 | These carry the tight gates: boot, title card and SCENE instruction/SPI bounds, LIVE's noise-chord frame, the 0.5 ms MIDI latency (`main.c`'s loop, `input_tick_isr`). Take them one file at a time with their gate. For main.c, moving `fm1_cstart`'s boot-only code into its own `-Os` file gets 1,280 of it without touching the loop. |
| 6 | `store.c` at `-Oz` (now `-Os`) | 1,562 | build | 1 | The store's polls are bounded by work (`STORE_POLL_WORK`), not speed. |
| 7 | `-Oz` for every other cold `-Os` file (views 852, tracker 478, song_io 470, gt2_song 402, ...) | ~6,800 | build (all cold files `-Oz`: 459,244) | 3 | `-Oz` outlines small inline helpers: more calls, so the screen and song gates move more than under `-Os`. |
| 8 | Drop the 8580's WEAK/STRONG combined-waveform profiles (`SIDOU_ALL_COMBINED_PROFILES_8580=OFF`) | 19,216 | build | 5 | Changes how the 8580 sounds with combined waveforms, the expert selector, and the profile tests and goldens. Product decision. |
| 9 | Art LZ-compressed, unpacked to RAM per image | est. 14 K SCENE, 5.4 K logo, 5.3 K title cards (LZ4-HC on the coded pools) | estimate | 5 | Blocked by RAM. The art reads straight from flash with no buffers; the arena alone needs 8.7 K, and each region has about 6.5 K above its 5,120 floor. The logo might borrow `slot_pool` at boot. High complexity, and the boot/SCENE gates are at stake. Not now. |
| 10 | SCENE art in the codec's dictionary mode | est. ~4,000 | estimate from run-byte frequencies | 3 | It was rejected before for SCENE's instruction gate (bitmap-codec.md). |
| 11 | Packed strings (5-bit text, shared dictionary) | est. 2 to 3 K of ~10 K (strings plus help cards) | estimate | 3 | A decoder in every text path; small gain. |
| 12 | Font limited to glyphs 0x20..0xDC | 536 | count | 1 | Needs clamping wherever a byte becomes a glyph index. |

Not targets: Felucca's USB stack, HAL and flash driver (vendored with their flags); the
Cracktro logo (already out of the image); `-Os`/`-Oz` on the audio path (see RAMTEXT).

## RAMTEXT and SRAM

Measured on main (`9b713e5d`) and on the drums branch (`d19d5944`, built from an exported
copy; its worktree was not touched).

| | main | drums |
|---|---:|---:|
| RAMTEXT used, of 49,152 | 48,976 | 49,152 (full; `song_ctl_block_end` grew 38 → 208, `audio_fill` +4) |
| RAM region free (floor 5,120) | 11,696 | 6,620 (`.data` gains the digi codes, 4,860) |
| POOL free (floor 5,120) | 12,160 | 12,160 |
| Image | 490,492 | 491,908 |

### What is in RAMTEXT, and why

On drums, 49,150 bytes of functions (the full per-function list comes from
`llvm-nm -S -n sidou_app.elf` between `_rt_start` and `_rt_end`):

| Group | Bytes | Largest functions |
|---|---:|---|
| SID render and registers (`sid.c`) | 15,906 | `sid_osc_run_combined` 2,852, `clock_voices` 1,916, `sid_write` 1,892, `sid_env_run` 1,684, `sid_osc_advance_slow` 1,290, the four `render_6581/8580[_bleed]` 3,054, `sid_render_tap` 632, `read_osc3` 420 |
| Effect slots (`fx_slots.c`) | 14,546 | `slots_run` 4,590, `slot_start` 1,538, the ticks (chorus 1,302, phaser 886, flanger 864, echo 692, overdrive 646, fuzz 644, tape 622, wow 612, crt 320), `step_switch` 242 |
| Plate and reverb | 3,002 | `sid_fx_plate_tick` 1,594, `sid_fx_reverb_tick` 1,408 |
| GoatTracker player (`gt2_play.c`, already `-Os`) | 5,216 | `exec_channel` 1,548, `gt2_play_frame` 998, `effect` 502, `freq_byte` 490 |
| Song transport, interrupt side (`song_ctl.c`, already `-Os`) | 2,786 | `song_ctl_block` 1,032, `song_ctl_call` 708, `song_lock_apply` 372, `song_ctl_block_end` 208 |
| Felucca flash driver (`midi.c`'s `.ram_text`) | 2,846 | `fl_prog_ram` 898, `fl_erase4k_ram` 828, `fl_read_ram` 424, `fl_status_ram` 364, `fl_jedec_ram` 332 |
| Drain, fill, mix and the rest | 4,848 | `reg_ring_drain` 1,536, `audio_fill` 1,040, thump 520, spread 306, scope 282, chip_mix 226, tail gate 188, master gain 160, `gt2_edit_apply` 158, `fm1_alnk0_irq` 106 |

**The call graph** was built from the ELF's emitted relocations: call relocations plus every
address taken, the latter counted as a possible indirect call. It has two root sets: the
audio interrupt (`isr_alnk0`) and the flash-hold paths (`fl_*_ram`). The result is the same on
main and drums:

- **46,042 bytes (drums; 45,868 on main) are reachable from the audio interrupt.** Three
  more functions are installed by `audio_init` as hooks the interrupt calls through a
  pointer: `fx_live_pseudo` and `sid_slots_set` (the drain's pseudo-register) and
  `song_ctl_chip_swapped` (the chip swap). Together they are 262 bytes.
- **The 2,846 bytes of the flash driver are the only code reached from the hold paths.**
  They are called through `FL_FAR` from flash code (`flash_store_prog/erase`, `ota_*`,
  `fm1_cstart`, `update_boot`), and they call nothing.
- **Nothing in RAMTEXT is unreachable.** Every byte is either audio-interrupt code or code
  that runs with XIP off.

**What has to run from RAM during a flash hold:** only the flash driver. A held erase
or program runs with interrupts masked after `audio_hold` has silenced the output, and a
program window masks them for at most 1 ms, so no interrupt code runs while XIP is off. The
timer and USB interrupts (`fm1_timer5_irq`, `midi_usb_isr`) already run from flash for that
reason. The audio path is in SRAM for its cycle cost and the branch range. `ram_audio.rs`
makes "the ALNK0 interrupt executes only `.ram_text`" a gate, and no hold requires it. So
code could leave RAMTEXT only by relaxing that rule for interrupt code that runs rarely.
Candidates would be the effect switch path (`slot_start` + `step_switch`, 1,780) and `read_osc3`
(420). Each would be called through a pointer and could miss the I-cache. That is a design
change, not a free move.

**`-Os` for RAMTEXT code that is not cycle-critical:** the code there that runs rarely is
already `-Os` (`gt2_play.c` and `song_ctl.c`, with `-fno-jump-tables`). Everything else runs
every sample or every register write. At `-Os`:

- `fx_slots.c` frees 1,024 bytes. It outlines `resting` (36), which `app.ld` would have to
  list, for a net 988 bytes, at a per-sample cost that `fx_budget.rs` and `slots_budget.rs`
  would have to accept.
- The whole audio path frees 2,012 bytes but outlines nine helpers (~470 bytes) into new
  sections.
- `slot_start` alone shrinks only 262 bytes.

`-Os` is a weak lever here.

### The "8 KB debug area"

0x01C0C000..0x01C0E000 lies between RAMTEXT and RAM. It is Felucca's layout: a space
for RAM-run debug images. In SIDOU its only use is `crt0.S`, which stores five words there for
a boot probe at 0x01C0DF20..0x01C0DF33: `icfg`, `psr`, the SPL's boot-parameter pointer,
`sp` and `ssp`. Nothing in the firmware, host tests, tools or docs reads them. Only
hardware.md's map mentions the address. A release build does not need the area.

The SPL is done before `fm1_cstart` runs, and Felucca's update loader (0x01C0A800) already
lies inside RAMTEXT, so the SRAM below RAM belongs to the app after hand-over. Ending RAMTEXT
at 0x01C0DF00 leaves the probe words where they are and gains 7,936 bytes. Moving the five
words into `.noinit` (NOINIT has 0x3D00 bytes spare) gains the full 8,192.

### SRAM candidates (prototyped on the drums branch's tree)

| # | Candidate | Gains | Risk | Cost |
|---|---|---|:-:|---|
| S1 | **Release the debug area to RAMTEXT** (reverted: code there failed on the device) (`LENGTH = 0xDF00`, the `_rt_end` and size asserts, `ram_audio.rs`'s `RAMTEXT_BYTES`, hardware.md) | **RAMTEXT +7,936** (+8,192 with the probe words in `.noinit`) | 1 | Built: links, RAMTEXT 49,152 of 57,088. No code or cycle change. A boot, store write and update check on the device belongs in hardware-test.md. Part of it could hold interrupt tables instead of code (S2/S3 put them in POOL instead). |
| S2 | **Noise tables built at boot into POOL** (`SID_NOISE_JUMP`, `SID_NOISE_WB_BYTE` in a `.pool.*` section instead of `.data`) | **RAM region +3,140**, POOL −3,140, flash −2,980 | 1 | Candidate 3 above, placed in POOL. Bit-exact (0 mismatches). POOL stays at 9,024 free. |
| S3 | **Digi codes packed two to a byte** (drums: 4-bit codes, one a byte today) | **RAM region +2,430**, flash −2,430 | 2 | The interrupt's digi read gets a shift and a mask, once every `SONG_DIGI_SPAN` samples; `song_ctl.c`'s two code pointers become an index. `tablegen digi` packs the codes. The digi tests and goldens stay as they are (same codes). |
| S4 | Digi codes into POOL instead of RAM (a load-and-copy section like `.data`) | RAM region +2,430 more (after S3) | 2 | An `app.ld` output section with `AT > XIP` and a copy loop in `fm1_cstart`. |
| S5 | `fx_slots.c` at `-Os` | RAMTEXT +988 | 3 | Effect cycles; plus the `resting` rule. |
| S6 | Effect switch path to the main loop | RAMTEXT ~1,780 | 3 | A design change in how a slot switch is applied. |

Measured on the drums tree with S1, S2 and S3 (layout only; S3's reader was not rewritten):
RAMTEXT 49,152 of 57,088 (**7,936 free**, was 0), RAM region **12,188 free** (was 6,620),
POOL 9,024 free (was 12,160), image 486,496 (−5,412).

**Recommended SRAM set: S1 + S2 + S3.** It frees 7,936 bytes of RAMTEXT and 5,570 of the
RAM region, and costs 3,140 of POOL's 12,160. That is about 10.4 KB of SRAM net against a
~3 KB target, and it covers M3's ~2 KB, USB audio's interrupt code and the drums. The risk
is low: S1 needs one hardware boot check, S2 is bit-exact, and S3 adds a shift on a rare
read. S5 and S6 stay in reserve.

## Recommendation

**For 0.4.0 (now):** candidates 1, 3 (as S2, in POOL), 4 and 6, plus S1; S3 with the drums branch. All of them are lossless, with no
golden changes and no RAM change.
Flash reclaimed: 12,608 + 2,980 + ~5,700 + 1,562 = **~22,850 bytes**; RAMTEXT +7,936; RAM region +3,140 (+2,430 more with S3).

**Before M3:** candidate 2 (group A at `-Os`), file by file with each file's gate, for
another 10,208. Then group B/C files and `-Oz` (candidates 5 and 7) as far as their gates
allow. That is up to ~19,000 more, to be used only when a feature needs it.

**Recommended set total:** candidates 1, 2, 3, 4 and 6 reclaim **~33,000 bytes**. The
combined prototype of 1, 2, 3 and 6 built at 463,128 bytes (−27,364, the parts adding up
exactly), and the pulldown repack adds ~5,700, for an image of about **457,400**. That is
48,600 under the 506,000 ceiling: M3's 28,000 fits with ~20 KB to spare for drums, USB audio
and generative STEP. The ceiling can then drop by what is reclaimed (bounds only tighten).
Candidate 8 (19 K) and art compression (~25 K, blocked by RAM) stay in reserve as product
decisions.

Order of work: 3 and 6 first (smallest), then 4 and 1 (tablegen changes; 1 with the
sequential cursor for the character jobs), then S1 with its hardware-test entry, then
candidate 2's files. None of the instruction gates were run for this study (the machine was
at load 110 to 160): candidates 1, 2 and 5 to 7 must pass `character.rs`, `screen.rs`,
`live.rs`, `boot.rs`, `scene_budget.rs` and `latency.rs` in their merge checks.
