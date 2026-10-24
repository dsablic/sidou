<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# FM-1 hardware reference

Facts about the M-VAVE FM-1 that SIDOU depends on, where each one comes from,
and what still needs checking on a real unit. JieLi's documents are
copyrighted, so we link to them rather than copy them into this repo.

## Sources

| Source | What it has |
|---|---|
| JieLi AC79 SDK, `doc/datasheet/AC791N规格书/` ([Gitee, official](https://gitee.com/Jieli-Tech/fw-AC79_AIoT_SDK), [GitHub mirror](https://github.com/jeffreywugz/fw-AC79_AIoT_SDK/tree/release/AC79NN_SDK_V1.0.3/doc/datasheet/AC791N%E8%A7%84%E6%A0%BC%E4%B9%A6)) | Datasheets for AC7911B, AC7913A0, AC7913A6, AC7915A and AC7916A (V1.0/V1.1, 2021); hardware design guide `AC791N硬件设计指南20220212.pdf`; reference schematics |
| [JieLi AC79 documentation](https://doc.zh-jieli.com/AC79/zh-cn/release_v1.0.3/getting_started/overview/index.html) | SDK overview and peripherals (Chinese) |
| AC79 SDK `cpu/wl82` headers | Register definitions. There is no public register manual. |
| [fm1-emu](https://github.com/joakimeriksson/fm1-emu) `docs/recon.md`, `docs/isa-notes.md` | SoC identification, pi32v2 instruction set, peripheral behaviour as modelled |
| Felucca HAL (`platform/fm1/hal/`, see `VENDORED.md`) | Pin assignments and peripheral use, proven on hardware |
| [aroum/fm1-custom-fw](https://github.com/aroum/fm1-custom-fw) `photos/`, `REVERSE_ENGINEERING.md` | Teardown photos of the board (`DX7 MB V07`), recovery notes |
| [AL-255/FM-1-RE](https://github.com/AL-255/FM-1-RE) `docs/architecture.md` | Stock boot chain and clock setup |
| [Melodee](https://github.com/keremimo/melodee) `BUILDING.md` | Clock and second-core measurements on real units |
| SDK `apps/common/jl_math/`, `include_lib/driver/cpu/wl82/asm/{csfr,cache,eq,src,dma,crc16,hwi,lock}.h`, `cpu/wl82/sdk_ld_sfc.c` | Accelerator APIs, register maps, memory map |

## The chip

- **Family:** JieLi AC791N ("WL82" in the SDK). fm1-emu identified it from
  the firmware package, the SPL and the SDK the stock firmware came from.
- **Package variant: AC7911B8 (QFN48), with 8 Mbit (1 MiB) flash in the
  package.** This is strongly supported, though the part-number line itself
  is not legible:
  - **Teardown photo** (aroum/fm1-custom-fw `photos/photo_01.png`): a 48-pin
    QFN with the JL logo and lot code `C156211-…`. The part-number line isn't
    readable at that resolution.
  - **Pins:** the AC7911B8 datasheet (in the SDK's
    `schematic/AC7911B0_AC7911B8参考原理图/`) lists every external pin the
    firmware uses, including PB6 (the MASTER pot) and PB7 (a key-matrix row).
    The plain AC7911B and the AC7911BA lack PB6/PB7; the AC7913A variants
    lack PC6.
  - **Flash:** the only pin the firmware uses that the AC7911B8 doesn't
    bring out is PD0, which is the flash chip-select (`fm1_flash.h`).
    That fits flash inside the package, and the "8" suffix means 8 Mbit
    flash, which matches the FM-1's 1 MiB.
  - **Earlier mistake:** this document first inferred AC7916A (QFN76) from
    the plain AC7911B datasheet, before the photo and the AC7911B8
    datasheet were checked.
- **CPU:** two 32-bit RISC cores (pi32v2) with a single-precision FPU, rated
  DC–320 MHz, 128 vectored interrupts. The datasheet says 4 priority levels,
  but each IRQ's `ICFG` field holds a 3-bit priority, 0–7 (unresolved).
- **Clock: custom firmwares run at 360 MHz.**
  - M-VAVE's flash head (`isd_config`) sets `SYS_CLK` to 360 MHz, above the
    SDK's 320 MHz table, and the SPL applies it.
  - The stock app then retunes to 240 MHz in its own `pll_clock_init`
    (AL-255 `architecture.md` §3).
  - Firmwares that don't touch the PLL (Felucca, SIDOU) stay at 360 MHz.
    Melodee measured this: 329 dependent adds per µs, on the SPL's rail
    (SYSVDD 1.26 V). Melodee offers an opt-in retune to 320 MHz.
  - The stock firmware is single-core, so no FM-1 was validated running both
    cores at 360 MHz. The core-1 misreads seen on some units may be related.
  - SIDOU's budget still assumes **240 MHz**, which is conservative by about
    1.5×. We only spend the extra headroom after measuring cycles on
    hardware.
- **Caches:** 32 KB I-cache (8-way, 32-byte lines). The D-cache is 32 KB,
  of which about 24 KB is usable after reservations. The SDK can free D-cache
  ways as RAM, 4 KB each, at 0x01F20000 and up; this is not validated on the
  FM-1.
- **On-chip SRAM:** about 512 KB of general RAM (`ram0`
  0x01C00000–0x01C7FE00). The "578 KB" figure includes the cache arrays.
  SIDOU's linker script (`platform/fm1/app.ld`) uses Felucca's regions,
  except that RAMTEXT is 48 KB instead of 24 KB (see "Code in SRAM").
- **Execution from flash:** code runs XIP from the in-package flash through
  the I-cache, over dual-I/O SPI with the SFC at about 90 MHz (at a 360 MHz
  system clock). An I-cache miss is estimated at **400–600 CPU cycles**.
  Hot audio code therefore runs from SRAM (`.ram_text`, a 48 KB region in
  `app.ld`; see "Code in SRAM" below).
- **Call range:** a direct `call` or `goto` reaches about ±4 MB
  (relocation `R_PI32V2_LONG_JUMP_23M2`), and SRAM code at 0x01C0xxxx is
  about 4 MB from XIP code at 0x0200xxxx: calls between the two must go
  through a pointer (Felucca's `FL_FAR`, SIDOU's `far_drain` in `audio.c`).
  This clang has no long-calls option.
- **No overlay or cache-lock facility.** The stock firmware's copy to
  0x04000120 is the SDK's SDRAM data slot. On this chip it copies nothing.
- **In-package memory:** flash only, 8 Mbit. The AC7911B8 has **no SDRAM
  or PSRAM**, so the on-chip SRAM is all there is.
- **Radio:** Bluetooth 5.0 BR/EDR + BLE, up to +15 dBm (the stock firmware
  uses BLE MIDI), and Wi-Fi 4 (802.11n, MCS7, 20/40 MHz). Both share one
  2.4 GHz radio. The antenna is the red wire soldered at pad P1, marked
  天线 ("antenna") on the board. SIDOU's published firmware uses neither
  (design doc section 17).
- **Other peripherals SIDOU uses:**
  - ALNK (I2S) to an external codec at 44117.6 Hz (24 MHz / 544);
  - SPI1 to the ST7789-class 240×240 LCD;
  - UART1 for TRS MIDI (PH8, 31250 baud);
  - USB0 for USB MIDI and updates;
  - the 10-bit SARADC for the MASTER pot and battery sense;
  - TIMER4 as a 24 MHz tick and TIMER5 for the 10 kHz key scan and the
    2 kHz USB and UART polls.
- **A short main-loop step once a second while USB is off.** With no cable at
  power-on the SIE does not answer. `midi.c`'s `usb_retry_quiet` tries again
  every second, a step per main-loop pass:
  - the controller's reset;
  - 25 ms later, `usb_probe`, the attach and one SIE write given 2000 polls;
  - only if the SIE answered, Felucca's own `usb_retry` and `usb_start`, after
    25 ms more in reset.

  fm1-emu, whose SIE never answers, shows the probe at 8,040 instructions, about
  34 µs at 240 MHz. `usb.c`'s `usb_retry` alone would stop the main loop for
  about 28 ms: the 25 ms wait, then six SIE writes timing out at 20000 polls
  each. Meanwhile MIDI input waits in its ring
  (platform/fm1/VENDORED.md).

## Instruction set features

From fm1-emu's ISA notes and the SDK's inline assembly (`jl_math.c`).
pi32v2 has no packed SIMD. It does have:
- dual-issue instruction pairs (the compiler pairs FPU ops with `-mfprev1`);
- hardware loops (`rep N { … }`), including FPU ops inside them;
- 64-bit multiply-accumulate, including a form with **two post-increment
  memory operands** (`acc += [a++=4]*[b++=4](s)`: two loads and a MAC in one
  instruction), which suits filter and mix loops;
- rounding shift `>>>(round)`, `sat16`, saturating adds, `smin`/min-max,
  `clz`, byte reverse;
- read-modify-write to memory (`[p] += r`), register-pair loads,
  post-increment addressing and predicated blocks.

fm1-emu does not yet execute every class (see `docs/TODO.md`); the
memory-operand MAC is probably among them. Divide latency is not documented.

## Hardware blocks

None of the existing FM-1 firmwares uses these. Registers are in the SDK's
`asm/` headers; "confirmed" means seen in SDK source, "inferred" means a
reasonable reading not yet verified.

| Block | What it does | SIDOU use | Status |
|---|---|---|---|
| **FFT + vector engine** (`JL_FFT` at 0x1EE0400, IRQ 57; `fftvec_math_drv.h`) | Real/complex FFT, 10–2048 points including non-power-of-2 sizes; vector add/sub/mul/MAC/dot/sum/min/max/magnitude² on s8/s16/s32 with strides, i.e. matrix-vector. This is what the "AI accelerator" refers to. Data must be in internal RAM. | Spectrum display; block mix, gain or FIR off the CPU. Program the registers directly; the SDK wrapper copies the data. | confirmed in SDK; not in fm1-emu |
| **Hardware EQ** (`JL_EQ`, IRQ 60; `asm/eq.h`) | Cascaded float biquads (up to 63 sections, 8 channels, s16/s32/float I/O) | Static IIR: DC block, tone, and the anti-alias low-pass if the SID is rendered oversampled and decimated. Not for the SID's modulated, non-linear filter. | confirmed in SDK; not in fm1-emu |
| **Sample-rate converter** (`JL_SRC`, IRQ 58; `asm/src.h`) | s16 only, 1–8 channels, rates ≤ 65535 Hz | Render at a SID-friendly rate and convert to 44117.6 Hz. Quality and the 16-bit limit need measuring. | confirmed in SDK |
| **Memory-copy DMA** (`asm/dma.h`) | Synchronous, async and list copies | Framebuffer and buffer copies | confirmed API |
| **CRC16/32** (`JL_CRC`), **AES**, **SHA**, **TRNG** (`JL_RAND`) | Hashing, crypto and randomness | Flash and update checks; random seeds (SID noise stays its own LFSR) | confirmed; CRC and RAND modelled in fm1-emu |
| **Performance counters** (`corex2` block at 0x1EEE000: per-core `IF/RD/WR_UACNT`, `TL_CKCNT`; per-core `TTMR`) | Probably fetch/read/write miss counts plus total clocks | Cycle and cache-miss profiling of the audio ISR on the device | registers confirmed, meaning inferred |
| **Guards and debug** (`WR_LIMITn`, `PC_LIMIT0/1`, `JL_DBG` breakpoints and watchpoint, 4-entry ETM branch trace) | Memory write limits, PC limits, trace | Stack and audio-buffer guards (Melodee pattern) | confirmed |
| **Inter-core** (`lockset/lockclr`, `TESTSET`, soft IRQs) | Spinlocks and signalling | Needed only if the second core is ever used | confirmed |

Also present but not relevant to SIDOU: SBC codec, JPEG, camera ISP, RGB LCD
interface, PDM microphone inputs (PLNK) and SPDIF receive.

## Code in SRAM

Since M1a Task 5b the audio path runs from SRAM: the ALNK0 entry
(`isr_alnk0`) and its C body (`fm1_alnk0_irq`), `audio_fill`, the register
ring's drain, `sid_render` and `sid_write` (2240 bytes of RAMTEXT's 24 KB at
the time). M1c Task 2 added the effect slots (`sid_slots_process`, the switch
path and every algorithm's tick, the two reverbs' ticks, and `fx_live`'s
pseudo-register handler): **22,280 of 24,576 bytes** with M1a's flash driver
(`fl_*_ram`, ~2.8 KB). They fit because the expensive algorithm inits (tables,
reverb layouts) run once in `sid_slots_init`, in flash, as templates that the
interrupt only copies; with those inits in the interrupt it was 26,920
bytes.

M2b Task 3 adds chip B's render beside chip A's, while chip B is in use, and their mix
(`chip_mix`): `.ram_text` 36,720 bytes. Out of use chip B is not rendered (a silent chip
still has a level of its own); its use changes at a block boundary, faded over that block.

**M2b completion, schema-5 build on `m2b`, rebased onto T1 `e88d2e7`:** chip A carries the song;
with KEYS AUTO, the keys play A without a song and B while it plays. B always follows
the keys' setup. KEYS A borrows song voices until key-up instead. UI sounds borrow a free
B voice only, never steal a held note, and are off by default. Both chips mix before the
one shared effect chain and final MASTER gain. Two loud chips in phase can clip the mix
by up to 3.3 dB; no automatic gain or soft knee has been introduced.

The expert layer does not reset either SID: constants and completed tables change at
audio-block boundaries. Filter rows use leased existing SID backing (no new row arrays),
build over 27 ticks, then crossfade over 127 ticks. DAC edits use the one shared build
table. Preparation, character builds, song analysis and store service exclude competing
slices in the same main-loop pass.

| Integrated resource | Measured | Existing limit |
|---|---:|---:|
| Packaged app image | 497,120 bytes | 512,000 bytes (latest merged stage, M2f) |
| RAMTEXT | 46,016 bytes | 49,152 bytes |
| Free RAM region / POOL | 5,380 / 4,472 bytes | ≥4,096 each |
| Free RAM region + POOL | 9,852 bytes | ≥7,000 bytes |
| POOL used | 347,784 bytes | 352,256 bytes |
| Three-SID ring object | 105,408 bytes | Expert growth 1,224 bytes |
| Chip B's marginal state | 35,216 bytes | Original estimate 35,300 bytes |

These include the merged storage, song engine, song views, MIDI and SCENE work; the
standalone M2b 384,000-byte image and 92,000-free-RAM projection is not this build's gate.
Before the expert layer (`64b6f97`), the image was 425,140 bytes, RAMTEXT 45,108 and
combined free RAM 11,492. Both 4 KiB region floors remain unchanged.

M2c Task 8 reserves settings schema 4 for the last-song filename; experts append as
schema 5, keeping those 124 bytes intact. Its larger runtime state initially left
only 2,676 RAM bytes free when combined with experts. CHIP and the song views now
share one 2,700-byte drawing canvas, with compose-time ownership and independent view
state, recovering 2,704 aligned RAM bytes. The song snapshot packs narrow parameters
and cutoff/pulse-width high bytes into the unchanged 160-byte I/O buffer; raw expert
255 stays profile-following. These are main-loop/UI storage changes, not audio changes.
The T1 theme-role rebase and CHIP text-role wiring add 32 image bytes over the previous
497,088-byte build, without changing RAM, RAMTEXT, tab layout or audio behavior.

The emulator's real two-chip ISR with every slot at its heaviest measures 2,387.1172
instructions/sample against 2,700. The conservative 6502 allowance is about **185.07
instructions/sample** after the measured slot-switch peak (964.28125), mix (8.05946)
and MASTER gain (12.0625 steady, 12.08567 with one pot move), not the earlier mix-only
197.44 or fixed-964-slot estimate of 185.35. Recording over USB audio takes up to 2 of
them while a computer records (TIMER5's work nested in a render: emu-gate
`USB_AUDIO_PER_SAMPLE`; slots_budget.rs counts it). Song playback and a future
6502 do not run together. The SID-only worst blocks are 705.3516 / 1,419.5469 for one/two
chips against 711 / 1,426; audio remains bit-exact with the host. These are instruction
counts, not hardware cycle measurements. Listening, clock/cache costs and the real LCD
remain manual checks in [the checklist](hardware-test.md#63-two-chips-routing-and-ui-sounds-m2b).

M1b's exact SID core (register reads, combined waveforms, the envelope
pipeline) brings `.ram_text` to about 32 KB, so RAMTEXT is now **48 KB**
(0x01C00000–0x01C0C000), twice Felucca's 24 KB. Nothing on the hardware set
the 24 KB: it was Felucca's inherited layout. SRAM is not cached, the SDK
treats all of `ram0` as rwx, and the update loader at 0x01C0A800 runs only
from the SPL after a reset, never next to the app.

**Code stays out of Felucca's 8 KB RAM-run debug area** (0x01C0C000–0x01C0E000, between
RAMTEXT and RAM). A release candidate of v0.4.0 ran RAMTEXT up to RAM, over that area. During
its device session (2026-10-24) STEP's edits seemed not to land and the area was suspected; it
was a misread view (TRACKER, where keys only audition without REC), not the SRAM. Nothing shows
the area failing, but nothing proved it safe on the device either, so the release keeps code
below it until a device check does (it would give RAMTEXT 8 KB). RAMTEXT ends at 0x01C0C000: `app.ld` asserts `_rt_end <= 0x01C0C000`, and emu-gate
`ram_audio.rs` checks it too. crt0.S saves its five boot words there as Felucca's does
([hardware-test.md section 17](hardware-test.md#17-code-in-feluccas-debug-area-v040)). The layout:

| Region | Range | Size |
|---|---|---|
| RAMTEXT | 0x01C00000–0x01C0C000 | 48 KiB |
| RAM-run debug area: no code (crt0.S's boot words at 0x01C0DF20) | 0x01C0C000–0x01C0E000 | 8 KiB |
| RAM | 0x01C0E000–0x01C1E000 | 64 KiB |
| POOL | 0x01C1E000–0x01C74000 | 344 KiB |

POOL starts with the song's content region: `__song_region` to `__song_region_end`,
**[0x01C1E000, 0x01C2E000)**, 65,536 bytes, 4 KiB aligned. The chips, the slots'
pool and the rest follow it. M3 overlays this same region with the C64's 64 KiB
address space: a song and a `.sid` never play together.

M2c Task 6 moved the RAM/POOL boundary 8 KiB down (from 0x01C20000) so POOL holds
the song's 64 KiB content region beside chip B. The original standalone M2c floor
was 22,000 bytes free across RAM and POOL (M2a's 2,000 already spent in it). With M2d, M2e and
M2f merged, the overview's gate table makes it **10,000** (its M2f row: 22,000 − 6,000 −
2,000 − 5,000 is 9,000, plus the slack the table keeps); **each region needs 5,120 bytes free**
(M3's start target, 4,096 plus 1 KiB, `docs/plans/2026-10-19-m3-sid-files.md`, "Before M3").
Both gates are tightened from 7,000 and 4,096 (end-of-M2 review 6.1).
The [M2c status](plans/2026-10-15-m2c-song-engine.md#m2c-status) records the current
measurement and the RAM margin left open for the end-of-M2 review.

Measured on `3b6d28d` (M2c Task 9, with the other merged M2 work):

| Region | Used | Capacity | Free | Gate |
|---|---|---|---|---|
| RAMTEXT | 45,216 B | 49,152 B | 3,936 B | used ≤49,152 B |
| RAM (`.data` + `.bss` + `.song_ram`) | 61,420 B | 65,536 B | 4,116 B | free ≥4,096 B |
| POOL | 346,560 B | 352,256 B | 5,696 B | free ≥4,096 B |
| RAM + POOL | 407,980 B | 417,792 B | 9,812 B | free ≥7,000 B |

The RAM region has **only 20 bytes above its floor**; combined slack of 2,812 bytes
does not make those regions interchangeable. This remains an end-of-M2 review item,
not permission to lower a bound. The firmware image is **454,288 bytes**, 57,712 below
the active **512,000-byte M2f ceiling** (the fixed app slot is 581,564 bytes).

**The song player (M2c Task 6)** runs in the audio interrupt too: the whole of
`core/player/gt2_play.c` (the frame path, at `-Os` and without jump tables, which
would be read from flash), `song_ctl`'s interrupt side (`song_ctl_block`, `_due`,
`_call`, `_block_end`, the swap hook `song_ctl_chip_swapped` and their helpers) and
the edit it applies (`gt2_edit_apply`). At boot the song's region, the shapes and
song_ctl are set up and the first song read into the region inside the LCD's 150 ms reset
wait (`lcd_init`'s `during`), and the boot's zeroing skips them (`.song_ram` and the head of
`.pool` in `app.ld`), so the song costs the boot's timeline nothing (the LCD goldens and the
budgets depend on where the audio blocks fall against the frames). Its analysis (`gt2_shape.c`) and `song_ctl`'s
main-loop side stay in flash and reach the frame path through pointers (`far_start`,
`far_frame`). The player's frequency tables are in `.data`; the shapes it follows and
the song are in RAM. At Task 6, `.ram_text` grew from 36,460 to 42,844 bytes;
the integrated M2c measurement is **45,216 of 49,152 bytes** (3,936 left).

On `3b6d28d`, the two-chip/slot-switch budget leaves **205.21875 instructions/sample**
before chip B's mix and MASTER. Their measured costs (8.05946 and up to 12.08567)
are run averages: subtracting them leaves an illustrative **185.07363/sample** with
the song player absent, not proven worst-block headroom. Allowances of 10 and 12.5
would leave **182.71875**, conditional on holding per block; existing tests only
bound run averages (constant gain for MASTER). Per-block accounting remains for the
end-of-M2 audio review. M3's CPU runs outside the ISR (decision record 0008), but its
stamped writes must fit there. See the M2c status for the player's separate worst
block with edits and the full accounting.

The player sweep on that commit covers 193 runs of 145 song/subtune/options cases.
Its worst block, a locked `sixpack.sng` with eight cell edits, is **5,070 + 592
isolated row-accounting instructions = 5,662/block**, **44.234375/sample** against
the unchanged **60** gate. The song test's 161.3/sample remainder includes the player
but excludes mix/MASTER; it must not be mistaken for M3's no-player reserve.

Headroom beyond 48 KB, if the audio path ever needs it (not the debug area, above):
- `-Os` for `core/fx` (about 3.3 KB smaller, at a cycle cost not yet
  measured);
- `-Oz` for `gt2_play.c` (336 bytes, but a fast-forward's slice then goes over
  `SONG_SLICE_INSTRUCTIONS`: emu-gate `song.rs`);
- RAMTEXT moved above the debug area, taking from POOL (RAM and the debug area move up);
- ~~moving the flash driver out of RAMTEXT~~: retired by M2a. The store
  (see "Flash map") programs and erases while the firmware runs, so the
  flash driver (`fl_*_ram`, about 2.8 KB) stays in RAMTEXT for good.

The rules for keeping it that way:
- **Placement is in `platform/fm1/app.ld` only**, by input-section rules on
  function sections (the device build compiles with `-ffunction-sections`).
  The portable C carries no attributes. The `.ram_text` statement comes
  before `.text` in the script, because the first rule that matches an input
  section wins.
- **Every function the audio interrupt reaches is in `.ram_text`.** Static
  helpers that the compiler inlines need nothing. A new non-inlined helper
  (in `sid_env`, `sid_osc` or a new `.c` file) needs its section added to the
  rules: if it is missed, it stays in `.text`, at least 0x400158 bytes from
  SRAM, and the link **usually fails** with "relocation truncated to fit"
  against it. It may still link where the two happen to lie in reach, so the
  link is no guarantee: the relocation test below is the guard.
- **No direct call crosses between flash and SRAM** (see "Call range"
  above). Code shared by both sides is `static inline` (as `sid_svf_set`),
  or the flash side calls through a pointer. The firmware links with
  `--gc-sections`, so host-only functions (as `live_sid_sink`) do not count.
- **The tests** (`emu-gate tests/ram_audio.rs`): the firmware links with
  `--emit-relocs`, and every branch relocation must have its site and its
  target in the same region, which catches a crossing that happens to be in
  reach. No address relocation in `.ram_text` may point into XIP (flash
  data or code). An emulator run checks that the ALNK0 interrupt executes only
  `.ram_text`: that is the only check for code entered through an absolute
  address (the vector, a future callback through a pointer), which links
  wherever it lies. (The disassembly is no authority: JieLi's objdump does
  not decode the FPU instructions and misreads the halfwords after them.)
- **Tables the audio path names are in RAM.** The compiler places a
  `static const` table in `.rodata`, in flash, and a read from it in the
  audio interrupt can miss the D-cache and wait on the flash. So a table that
  `.ram_text` code reads goes to `.data` on the device
  (`__attribute__((section(".data.<name>")))` under `__PI32V2__`, as
  `SID_NOISE_WB`, `SID_ENV_RATE_POS`, `SID_ENV_THRESHOLDS` and the slots'
  `ALGORITHMS`), or is built at boot in POOL (`SID_NOISE_JUMP` and
  `SID_NOISE_WB_BYTE`, computed by `sid_noise_tables_init`, which the boot
  calls in the LCD's reset wait: no copy in the image). `ram_audio.rs` checks every address the
  `.ram_text` code loads. Other constant tables stay in flash.
- **One flash read remains, by design:** the combined waveforms' pulldown
  rows (`gen/pulldown.h`, three strengths on each model since M2b's expert selector:
  184,320 bytes before packing, 52,407 after sharing and overlapping their
  16-entry blocks and sharing the rows that are alike; a lookup reads the
  row's index, then the block), which the
  oscillator reads through `sid_osc.pulldown` while a combined waveform
  plays. They are too big for RAM, and the relocation check cannot see a
  read through a pointer. Whether they cost cache misses in the audio
  interrupt is an open check below. The audio path therefore still needs
  XIP: it can't run while the flash is being written.

## Opportunities, ranked

These follow from the facts above. Each needs measuring before it's adopted.

1. **Run the audio ISR and `sid_render` from SRAM** (`.ram_text`): this
   removes the risk of I-cache misses on flash. It is cheap, and it is the
   biggest determinism win. Done in M1a Task 5b; the gain is still to be
   measured (open checks).
2. **Profile on the device** with the performance counters and TIMER4:
   cycles and misses per audio block, and the real clock.
3. **Hand-written assembly** using the memory-operand MAC, `rep` and rounding
   shifts for filter and mix loops. The C stays the reference, and the
   emulator gate must support the instructions first.
4. **Pick the clock deliberately:** stay at the SPL's 360 MHz, or retune to
   320 MHz for second-core stability.
5. **Hardware EQ** as the anti-alias filter if the SID is rendered
   oversampled, and **SRC** if a native SID rate is worth it.
6. **The FFT engine** for a spectrum or scope view.

## Updates and recovery

- **The update path is Felucca's.** Step 1 of the community update
  protocol runs in SIDOU (Felucca's `ota.c`); the update loader is
  Felucca's, unmodified and byte-identical to Felucca's own build. Once
  step 1 commits, the SPL runs that loader at every power-on until an
  install completes. It writes only the app area `[0x4000, 0x93000)`, never
  the flash head: a device keeps its own (M-VAVE's SPL and isd_config).
- **Identity:** SIDOU development builds answer `FM-1_900`, the same as
  Felucca's development builds (docs/TODO.md).
- **Boot-loop guard:** two early deaths in a row send the third boot to
  UBOOT, with the watchdog stopped first.
- **Recovery paths** if a unit no longer boots: FM-1-transporter
  (<https://github.com/kurogedelic/FM-1-transporter>), and MvaveFM1Unbricker
  with jl-uboot-tool, both community tools, to be confirmed on a unit
  ([hardware-test.md](hardware-test.md)); back to stock with M-VAVE's
  updater or "Return to official V15".

### First real-hardware run (2026-10-17)

[The first-flash report](hardware-results/2026-10-17-first-flash.md) records
v0.2.0-m2-preview installed over ChoralRoot 0.24-beta2. SIDOU booted and played:
the title card, LIVE, keys, knobs, screen and USB-MIDI input worked. USB-MIDI
output almost never reached the host, so identity replies, panel CCs and the
update protocol's read requests failed. Recovery to V15 succeeded from a Mac
through externally entered ROM boot (Flipper Zero and `tools/fm1-unbrick-mac`);
that recovery does not establish store survival through stock firmware.

The run also found a faint tone after releases, MASTER not controlling songs,
and a narrow knob range; macOS retained the previous firmware's USB-MIDI name.
Main now has the USB changes and OCT- power-on recovery gesture, a tail gate,
and MASTER as final digital gain. Those fixes still need their hardware
rechecks; the knob-range UX work is open. See [hardware-test.md](hardware-test.md),
especially the mandatory USB-output release gate in section 8.1.
**The flash timing spike has not run on a unit.** Neither that run nor the
emulator supplies measured program/erase times; the maxima below remain provisional.

## The flash part's timings (open: M2a Task 3b)

The store (M2a) programs and erases the flash while SIDOU runs. With XIP off
and interrupts masked for each operation, neither the audio interrupt nor
TIMER5 runs meanwhile, so the store's write windows and held erases (M2a
Task 4) are sized from the part's real times. fm1-emu can't give them: its
flash is instantaneous.

**The part.** The unit answers JEDEC `85 60 14`. The id table of Puya's
P25Q80H datasheet (8 Mbit, January 2021) lists exactly that id. Its figures,
typical / maximum:

| Operation | Datasheet |
|---|---|
| 4 KiB sector erase (0x20), `tSE` | 8 / 20 ms |
| 256-byte page erase (0x81), `tPE` | 8 / 20 ms |
| page program (0x02), up to 256 bytes, `tPP` | 2 / 3 ms (0.6 / 1.5 ms for the "Q" ordering option) |
| erase or program suspend latency (0x75), `tESL`/`tPSL` | 30 µs max |
| resume (0x7A) to the next suspend, `tERS` | 20 µs min (100 µs for the erase to make progress) |

More from the datasheet:
- Suspend sets SUS1 (status bit 10) for an erase and SUS2 (bit 15) for a
  program. While an erase is suspended, everything outside the sector being
  erased can be read.
- The configure register's DP bit (0x15, bit 7) turns pages to 512 bytes:
  then page program wraps at 512 bytes and 0x81 erases a 512-byte dual page.
  It defaults to 0.
- On partial programs, the datasheet says only that 1 to 256 bytes may be
  programmed per operation, that whole pages programmed once are fastest,
  and it refers to "the P25Q serial flash application note for multiple
  byte program operation within one page". It gives no count of programs a
  page allows before an erase, and it says nothing on programming bits that
  are already 0.
- It doesn't mention ECC.

The datasheet's worst page program (3 ms) is over a write window's
1,000 µs, so the measured times decide.

**The spike** (`platform/fm1/flash_spike.c`, a test firmware built with
`-DSIDOU_FLASH_SPIKE=ON`; [hardware-test.md](hardware-test.md) section 12)
measures all of this on the store's last two sectors and shows it on the
screen. These are the numbers to record, from a real unit (fm1-emu shows 0
for every time):

| Measurement | Unit 1 | Notes |
|---|---|---|
| JEDEC id, SR1/SR2, CR (DP) | | |
| CLOCK, adds/µs (0.8 × MHz: 192 is 240 MHz, 288 is 360 MHz) | | |
| 4 KiB erase: min / mean / max µs (20 runs) | | |
| program 16 bytes: min / mean / max µs (20) | | |
| program 64 bytes (20) | | |
| program 256 bytes (280) | | |
| page erase 0x81: THE PAGE / WIDER / IGNORED / ODD; min / mean / max µs | | |
| suspend: SUS1 n/5, latency min / mean / max µs, READ, END | | |
| 64 programs into one page (PARTIAL) | | |
| zeros over zeros, 3 times (REZERO) | | |
| one byte programmed 8 times (BITWALK) | | |
| a page programmed twice, the AND read back (TWICE; not OK suggests ECC) | | |
| TRS: bytes and lost, open 1 s / 100 × 1 ms masked / 10 × 35 ms masked | | |
| stock round trip: the pattern at 0xDE000 after V15 and back (INTACT / DAMAGED / ABSENT) | | |
| store sectors not blank at the spike's first start, of 73 | | |

**Decision rule** (M2a Task 4 applies it to the recorded numbers):
- **Programs go in windows** if the worst 16-byte program plus the driver's
  overhead is ≤ 1,000 µs. `STORE_WINDOW_BYTES` is the largest of 16, 64 and
  256 whose worst time fits. Otherwise programs become **held operations**
  like erases (quiet only), and settings saves wait for quiet.
- **Erases:**
  - a held 4 KiB sector erase, if its worst time is ≤ 30 ms;
  - else 16 held page erases (0x81) a sector, if the part erases the page
    alone and each is ≤ 30 ms;
  - else erase suspend in ≤ 30 ms slices, if it works (SUS1 set, the read
    correct, the erase finished after the resume);
  - else erases only at boot (before the audio starts, at most
    `STORE_POOL_TARGET` sectors) and on an explicit format, and the pool is
    sized at boot for the session.

  30 ms leaves 5 ms under the 35 ms hold budget for the driver.
- **The TRS input:** a 35 ms hold must lose no TRS byte (the spike's 35 ms
  row, LOST 0). Otherwise the hold budget shrinks until it does.
- **Several programs into a page, and reprogramming:** the store programs a
  record's data, then its header, then its commit flag, and a sector's
  header in two steps (M2a Task 2), often into one page. If PARTIAL, REZERO
  or BITWALK is not OK, or TWICE shows the part does not AND what it is
  given, the format must change before hardware acceptance: one program per page region, no
  bit cleared twice.
- **The clock:** at 360 MHz every instruction-count margin in M2 grows by
  half; at 240 MHz they are as written.

## Flash map

The FM-1's flash is a 1 MiB SPI NOR (JEDEC `85 60 14`, a Puya
P25Q80-class part) of 4 KiB erase sectors and 256-byte program pages. Who
owns which bytes (M2a Task 1; the constants are in `app/store_layout.h`,
and `host/crates/sidou-tests/tests/app/store_layout.rs` checks them against
the loader, the update protocol, `app.ld` and the package):

| Range | Size | Owner |
|---|---|---|
| `[0x000000, 0x004000)` | 16 KiB | the device's flash head (SPL, isd_config): never written by anyone but the vendor |
| `[0x004000, 0x093000)` | 572 KiB | The update loader's app area; SIDOU's slot is fixed (below). |
| `[0x093000, 0x097000)` | 16 KiB | the first 16 KiB of the package's VM region; inside the loader's record scan (it erases update records it finds there); the stock firmware's VM data may live here. Neither SIDOU nor Felucca writes it |
| `[0x097000, 0x0E0000)` | 292 KiB, 73 sectors | **SIDOU's store** (Felucca's main store on Felucca); inside the VM region; the loader's scan skips it |
| `[0x0E0000, 0x0E5000)` | 20 KiB | the update protocol's staging area and its record (0xE4F00); inside the VM region |
| `[0x0E5000, 0x0E9000)` | 16 KiB | the end of the VM region: Felucca's autosave and voice bank, the stock OTA's VM backup (0xE8000); never written by SIDOU |
| `[0x0E9000, 0x100000)` | 92 KiB | not SIDOU's: Felucca's globals (0xFC000) and the rest; never written |

SIDOU's app slot is 581,564 bytes (0x8DFBC), mapped at 0x02000120. The store's
size is not a lever for the image.

**Previous firmware's data is FOREIGN.** Nonblank ChoralRoot/Felucca data in
`[0x97000, 0xE0000)` with no SIDOU headers is not imported or silently erased.
STORE reads FOREIGN, and settings and USER profiles cannot persist there.
Only the menu's explicit confirmation (two clockwise ALGORITHM steps on STORE,
without leaving the item) formats it, destroying that data. A cancelled
confirmation writes nothing. Back up wanted data with the previous firmware
before choosing to format; merely booting SIDOU is not consent.

**The firmware may erase and program two ranges only:** the staging area
and the store (`platform/fm1/midi.c`'s `FL_RANGE_OK`, which the RAM flash
driver checks on every erase and program). The update protocol's hooks
(`platform/fm1/ota_flash.h`) keep to the staging area.

**The VM descriptor.** Besides the app and the whole-flash entry, the
package (`host/crates/sidou-pack`) writes a "VM" region descriptor at
0x93000, 352,256 bytes long: `[0x93000, 0xE9000)`, the SDK's own key-value
store. It covers the store, the staging area and Felucca's autosave and
voice bank. Nothing on SIDOU acts on it: only SDK code reads or formats that
region (the stock app's syscfg); SIDOU links no SDK VM code; the update
loader is SIDOU's own (`platform/fm1/loader/ldr_core.c`) and ignores the
descriptor; and Felucca ships the same descriptor and has kept user data in
`[0x97000, 0xE0000)` on these units since its 1.x releases. What the stock
SPL does with it is not known: an open check (below).

**The guard band.** Bytes `0xF00..0xFFF` of every store sector are never
programmed (`STORE_SECTOR_USE`). An update record the SPL or the loader
could take is 80 bytes at a sector's `0xF00` ("4K boundary - 256"), with
the magic `0x5441` at +6 and a matching CRC16 (`platform/fm1/src/ota.c`).
An erased band reads `0xFFFF` there, so nothing in the store can ever look
like a record, wherever the SPL scans; the loader skips the range already.
80 bytes would be enough: the band keeps the whole 256-byte page so that a
page program, which wraps inside its page, can never reach it.

**The plain XIP window.** At boot, during the panel's wake-up wait in
`lcd_init` (so its cache invalidation, a few milliseconds, costs the boot no
time) and long before the audio starts or anything reads the store, the
firmware maps the flash from 0x93000 up as an unencrypted XIP
window (`flash_plain_window`, Felucca's `fl_plain_window_init`, as the SDK's
`IOCTL_SET_VM_INFO` does), so the store reads straight through
`fm1_xip_ptr` as stored. The update protocol's reads (`ota_fread`) go
through `fl_read_ram` with XIP off, so the window does not change them.

**Partial programs.** The store programs a 256-byte page up to 34 times
between two erases of its sector (`STORE_PAGE_PROGS`, for write windows of
16 bytes or more, plus one for each cut that falls in the page): record
data, headers and commit words, each its own program. It programs bytes
already programmed only with the same bytes, or in the kill: a claim
programs the sector's erase header again (so bits a cut left faint are
programmed for good), a claim read as never made is made again over the
same bytes (a claim's number is the erase count), and the kill programs the
magic's 1 bits to 0 (never a bit that reads 0). A write a cut interrupted is
copied forward, not programmed again. A part with
ECC would need a format rework into 16-byte units each programmed once
([store-format.md](reference/store-format.md), "Partial programs and ECC").
All three are open checks below.

Sources: `platform/fm1/loader/ldr_core.c` (`LDR_APP_*`, `LDR_REC_*`,
`LDR_DATA_*` and the record scan), `platform/fm1/src/ota.c` (`OTA_AREA`,
`OTA_RES`), `platform/fm1/hal/fm1_flash.h` (Felucca's ranges and
`fl_plain_window_init`), `host/crates/sidou-pack/src/lib.rs` (the region
descriptors), `platform/fm1/app.ld` (the app slot).

## How the store writes

`platform/fm1/store_fm1.c` gives the store (`app/store.h`) its flash: reads
straight through the plain XIP window, programs and erases through
`midi.c`'s RAM driver (`flash_store_prog`, `flash_store_erase`), which keeps
to the store's range and never touches a sector's guard band.

- **At boot**, during the LCD's 120 ms wake-up wait (before the audio starts;
  work fitting that deadline does not move the boot's later timing): the
  mount, then the recovery after a cut and the pool's fill to its target, with
  no holds, for at most 100 ms of flash work (`BOOT_WORK_MS`). What is left
  (a blank region's first format, up to 22 erases; another firmware's sectors
  in SIDOU's region, each erased before the session's first write) goes on in
  the main loop as quiet maintenance, an operation a pass, so USB answers
  within a second of power-on whatever the store still has to do (emu-gate
  `usb.rs`, `a_first_boot_over_another_firmwares_sectors_answers_usb_at_once`).
  A wholly foreign region is left untouched, not filled or formatted.
- **The erase-ahead pool** has a minimum of 20 clean sectors and a target of
  22. Writers claim space before programming; a 64 KiB file claims 18 sectors
  and reserves its commit record before accepting data. Short-pool requests
  return BUSY without starting a partial write. Quiet maintenance copies live
  records, then erases victims and dirty sectors to replenish the pool.
- **A program in a window**: interrupts masked, at most
  `STORE_WINDOW_LEAD_US` (250 us) after the end of an audio block (the
  ALNK0 interrupt stamps `audio_block_end` as its last act) and with no
  block due; else `STORE_LATER` at once, and the store tries again on a
  later pass. A window lasts at most `STORE_WINDOW_US` (1,000 us), so it
  ends 211 us before the next block's interrupt at the worst
  (`_Static_assert` in `store_fm1.h`).
- **A held operation** (every erase; programs too while no window fits):
  only when quiet. `audio_hold` fades the output to 0 over one block, then
  writes silence (the chips stand still; the register ring still drains);
  once both DMA halves are silent the operation runs with interrupts masked,
  at most `STORE_HOLD_US` (35 ms), held operations at least 10 ms apart.
- **Quiet**: no song playing, no voice gated, no UI sound, no button down,
  no MIDI input but real-time bytes (F8, FA-FC, FE) for 500 ms, and the
  output under -48 dBFS for as long (a peak scan in `audio_fill`, under 2
  instructions a sample, on only while the store wants a held operation and
  no song plays). -48 dBFS rather than -60, from before the tail gate: the 6581
  model left a floor of about -55 dBFS after notes that never decayed. With the
  chip's filter bleed at 0 the output is now exactly silent about 50 ms after the
  last envelope ends (`app/tail_gate.h`), but a nonzero bleed keeps the floor, so
  the threshold stays.
- **The main loop** runs `store_fm1_service` once a pass after
  `midi_service`: the quiet detector, then `store_poll`, never in a pass that
  ran a chip preparation slice; a store with work in hand runs at every
  pass, an idle one looks once a tick (10 ms) and asks whether maintenance has
  work (a plan of a few thousand instructions) once a second, never in a pass
  whose `store_poll` had work.
- **A forced save**: explicit settings and USER-profile saves bypass the 5 s
  background debounce. If still pending after 2 s, they enable `store_fm1_force`:
  held operations (programs too under the provisional timings) can run without
  quiet, 100 ms apart when not quiet. Completion says `SAVED (GAPS)` or
  `SAVED TO USER n (GAPS)`. Background saves wait and never force gaps.

**The numbers are provisional**: the P25Q80H datasheet's maxima (Table
5-4): a page program up to 3 ms, a 4 KiB sector erase up to 20 ms, erase
suspend latency 30 us. By the decision rule below no window fits a 3 ms
program, so `STORE_WINDOW_BYTES` is 0: programs are held operations for now.
Background saves wait for quiet; an explicit save can force gaps after 2 s.
The sector erase fits a hold. Task 3b's spike replaces
`STORE_PROG16_MAX_US`, `STORE_PROG64_MAX_US`, `STORE_PROG256_MAX_US` and
`STORE_ERASE_MAX_US` in `store_fm1.h`, and `STORE_WINDOW_BYTES` follows.

**The decision rule** (M2a plan, Task 3b): programs in windows if the worst
16-byte program plus the driver's overhead fits 1,000 us, `STORE_WINDOW_BYTES`
the largest of 16, 64 and 256 that fits; else held programs. Erases: a held
sector erase if its worst fits 30 ms; else 16 held page erases (0x81) if each
fits; else erase-suspend slices; else erases only at boot. Only the sector
erase is built: a spike that needs another mode fails `store_fm1.h`'s
`_Static_assert`.

**Timing evidence, not hardware measurements:** emu-gate's `store.rs` enables
16-byte windows in RAM and checks their placement and the program driver's
instructions against the 100 µs overhead estimate at 240 MHz. On main
`b38b8ed`, the longest reported `fl_prog_ram` call is 763 instructions (about
3.18 µs at that assumed clock); the flash busy time is zero in this model. It does not
report a longest complete interrupts-off stretch for either windows or holds.
The bounds remain 1,000 µs per window and 35 ms per held operation (below the
TRS input ring's 40.96 ms capacity), pending the spike and listening/TRS checks.

## Open checks on a real unit

| Check | Why it matters | How |
|---|---|---|
| Legible part-number line | Confirms AC7911B8 beyond the pin and photo evidence. | A sharper photo of the chip (6 screws, one under the sticker). |
| Actual CPU clock | Melodee measured 360 MHz; the budget assumes 240. | Time a dependent-add loop against TIMER4, or read the `TL_CKCNT` counter. |
| Cycles vs instructions | The emulator counts instructions, not cycles. Flash (XIP) fetches and divides cost more. | Read the per-core counters around an audio block; compare `.text` with `.ram_text`. |
| The audio path from SRAM | fm1-emu doesn't model the I-cache, so Task 5b's benefit is unmeasured. | Cycles per audio block (TIMER4 or `TL_CKCNT`) and fetch misses (`IF_UACNT`), with the LCD redrawing, in a build with `.ram_text`'s audio rules and one without. |
| Combined waveforms from flash | The pulldown rows (`SID_PD`, `.rodata`: a row's index entry, then its 16-entry block) are the audio interrupt's only flash data reads. | Cycles per audio block with two chips on combined waveforms against single waveforms (TIMER4 or `TL_CKCNT`), and D-cache misses if a counter shows them. If they cost, move the default profile's rows to POOL (a loaded section, copied at boot). |
| The USB retry's probe | Without a cable, `usb_probe` runs once a second: the attach and one SIE write given 2000 polls (8,040 instructions in fm1-emu). | Time `usb_probe` against TIMER4 on a unit powered on without a cable, and check that a cable plugged in later still enumerates (the probe's 2000 polls are enough for an SIE that answers). |
| Flash program and erase times (M2a Task 3b) | The store's write windows and held erases are sized from them; fm1-emu's flash is instantaneous. Also partial programs per page, reprogramming, ECC, page erase, erase suspend, and whether the stock firmware writes the store's range. | The flash spike ([hardware-test.md](hardware-test.md) section 12); the table and the decision rule are in "The flash part's timings" above. |
| Unknown latencies | Divide latency, IRQ entry latency, `IPMASK` semantics, and whether IRQ priorities have 4 or 8 levels | Microbenchmarks on the device. |
| The SPL and the VM descriptor | The package declares `[0x93000, 0xE9000)` as the SDK's VM region, which holds the store (see "Flash map"). Whether the stock SPL or stock V15 acts on it is not documented. | M2a Task 3b: a test pattern in the store survives a round trip through stock V15 and back. |
| Partial programs per page | Store needs up to 34, plus cuts. | Task 3b: check limits; PARTIAL reads 64 × 4 bytes. |
| Zeros over zeros | Claims reprogram the same bytes. | Task 3b: check the spec, REZERO (3), BITWALK (8). |
| ECC | May require single-shot units. | Task 3b: spec/SFDP and TWICE's two-page-program AND readback. |
| The store's timings | `store_fm1.h`'s program and erase times are the P25Q80H datasheet's maxima; by them no write window fits and settings saves wait for quiet. | M2a Task 3b's spike: page programs of 16, 64 and 256 bytes and sector erases, timed. |
| A write window on the device | A program masks interrupts up to 1,000 us right after a block: it must never drop an audio block or a TRS byte. | hardware-test.md section 11: a save while a chord plays, listened to; MIDI clock into the TRS jack during saves. |
| A hold's gap | A held erase silences the output up to 35 ms when quiet (faded out over a block, back in over one). | hardware-test.md section 11: a pool refill after a chord's release tail, listened to. |
| Second core RAM issue | Some units misread RAM on core 1. | Felucca's core-1 self-test, before SIDOU ever uses core 1. |

[hardware-test.md](hardware-test.md), sections 11 and 12, turn these into checklist
items; its other sections cover installing, recovery and the instrument.

For the partial-program checks, readback success alone does not establish the
part's specification. If zeros-over-zeros is prohibited, the plan's fallback
changes claims to bytes 20-31 only and re-erases clean sectors before first use
(at most 22 a boot). A kill after a cut can still program bits left faint and
reading 1. If the part has ECC, the store needs a format rework into single-shot
units before hardware acceptance; these are not changes already made on main.
