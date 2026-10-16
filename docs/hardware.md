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
- **A main-loop stall once a second while USB is off.** With no cable at
  power-on the SIE does not answer, and `usb_retry` (platform/fm1/usb/usb.c)
  attaches again every second. Each SIE write in the attach polls for up to
  20000 iterations, and the attach gives up at the third timeout. That is
  about 300k instructions in one main-loop pass. fm1-emu, whose SIE never
  answers, shows it: `usb_attach_now` takes 300,085 instructions, so one
  10 ms window a second runs 316k main-loop instructions against 17k in the
  others. At 240 MHz that is about 1.3 ms, longer if the register reads are
  slow. Meanwhile MIDI input waits in its ring and the tick catches up
  (`TICK_CATCH_UP`). MIDI out is not involved. Polling less, or giving up at
  the first timeout, would change Felucca's hardware-tested attach, so this
  waits for a measurement on a unit (below).

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

M1b's exact SID core (register reads, combined waveforms, the envelope
pipeline) brings `.ram_text` to about 32 KB, so RAMTEXT is now **48 KB**
(0x01C00000–0x01C0C000), twice Felucca's 24 KB. Nothing on the hardware set
the 24 KB: it was Felucca's inherited layout. SRAM is not cached, the SDK
treats all of `ram0` as rwx, and the update loader at 0x01C0A800 runs only
from the SPL after a reset, never next to the app. The layout:

| Region | Range | Size |
|---|---|---|
| RAMTEXT | 0x01C00000–0x01C0C000 | 48 KB |
| RAM-run debug area (crt0.S's boot words at 0x01C0DF20) | 0x01C0C000–0x01C0E000 | 8 KB |
| RAM | 0x01C0E000–0x01C20000 | 72 KB |
| POOL | 0x01C20000–0x01C74000 | 336 KB |

Headroom beyond 48 KB, if the audio path ever needs it:
- `-Os` for `core/fx` (about 3.3 KB smaller, at a cycle cost not yet
  measured);
- RAMTEXT further up, taking from POOL (RAM and the debug area move with
  it);
- moving the flash driver out of RAMTEXT (it only runs with the audio
  stopped, so it could be copied into the slots' pool for an update), which
  touches the vendored update path. It also needs POOL to be executable:
  `platform/fm1/app.ld` declares it `(rw)` today, so the linker would have
  to declare it `(rwx)` (and nothing on the chip may forbid fetching from
  it).

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
  `SID_NOISE_WB`, `SID_NOISE_JUMP`, `SID_ENV_RATE_POS`,
  `SID_ENV_THRESHOLDS` and the slots' `ALGORITHMS`). `ram_audio.rs` checks
  every address the `.ram_text` code loads. Other constant tables stay in
  flash.
- **One flash read remains, by design:** the combined waveforms' pulldown
  rows (`gen/wave_*.h`, 122,880 bytes since M1c's chip profiles: the 6581's
  three strengths and the 8580's AVERAGE), which the
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
  given, the format changes before Task 4: one program per page region, no
  bit cleared twice.
- **The clock:** at 360 MHz every instruction-count margin in M2 grows by
  half; at 240 MHz they are as written.

## Open checks on a real unit

| Check | Why it matters | How |
|---|---|---|
| Legible part-number line | Confirms AC7911B8 beyond the pin and photo evidence. | A sharper photo of the chip (6 screws, one under the sticker). |
| Actual CPU clock | Melodee measured 360 MHz; the budget assumes 240. | Time a dependent-add loop against TIMER4, or read the `TL_CKCNT` counter. |
| Cycles vs instructions | The emulator counts instructions, not cycles. Flash (XIP) fetches and divides cost more. | Read the per-core counters around an audio block; compare `.text` with `.ram_text`. |
| The audio path from SRAM | fm1-emu doesn't model the I-cache, so Task 5b's benefit is unmeasured. | Cycles per audio block (TIMER4 or `TL_CKCNT`) and fetch misses (`IF_UACNT`), with the LCD redrawing, in a build with `.ram_text`'s audio rules and one without. |
| Combined waveforms from flash | The pulldown rows (6 KB per waveform and chip, `.rodata`) are the audio interrupt's only flash data reads. | Cycles per audio block with two chips on combined waveforms against single waveforms (TIMER4 or `TL_CKCNT`), and D-cache misses if a counter shows them. If they cost, move the default profile's rows to POOL (a loaded section, copied at boot). |
| The USB retry's stall | Without a cable, the attach `usb_retry` runs once a second times out three SIE writes in one main-loop pass (300k instructions in fm1-emu). | Time `usb_attach_now` against TIMER4 on a unit powered on without a cable. If it is well over a millisecond, poll less or stop at the first timeout. |
| Flash program and erase times (M2a Task 3b) | The store's write windows and held erases are sized from them; fm1-emu's flash is instantaneous. Also partial programs per page, reprogramming, ECC, page erase, erase suspend, and whether the stock firmware writes the store's range. | The flash spike ([hardware-test.md](hardware-test.md) section 12); the table and the decision rule are in "The flash part's timings" above. |
| Unknown latencies | Divide latency, IRQ entry latency, `IPMASK` semantics, and whether IRQ priorities have 4 or 8 levels | Microbenchmarks on the device. |
| Second core RAM issue | Some units misread RAM on core 1. | Felucca's core-1 self-test, before SIDOU ever uses core 1. |

[hardware-test.md](hardware-test.md), sections 11 and 12, turn these into checklist
items; its other sections cover installing, recovery and the instrument.
