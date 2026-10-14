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
  DC–320 MHz, 128 vectored interrupts, 4 priority levels.
  - fm1-emu and SIDOU's budget assume **240 MHz**. That is unconfirmed (see
    open checks).
- **Caches:** 32 KB I-cache (8-way). The D-cache is 24 KB 6-way according to
  the datasheet, and 32 KB according to the SDK README.
- **On-chip SRAM:** 578 KB (SDK documentation). SIDOU's linker script uses
  the same regions as Felucca (`platform/fm1/app.ld`).
- **In-package memory:** flash only, 8 Mbit. The AC7911B8 has **no SDRAM
  or PSRAM**, so the on-chip SRAM is all there is.
- **Radio:** Bluetooth 5.0 BR/EDR + BLE (the stock firmware uses BLE MIDI)
  and Wi-Fi 4. SIDOU's published firmware uses neither (design doc section
  17).
- **Other peripherals SIDOU uses:**
  - ALNK (I2S) to an external codec at 44117.6 Hz (24 MHz / 544);
  - SPI1 to the ST7789-class 240×240 LCD;
  - UART1 for TRS MIDI (PH8, 31250 baud);
  - USB0 for USB MIDI and updates;
  - the 10-bit SARADC for the MASTER pot and battery sense;
  - TIMER4 as a 24 MHz tick and TIMER5 for the 10 kHz key scan.

## Instruction set features

From fm1-emu's ISA notes. pi32v2 has no packed SIMD. It does have:
- dual-issue instruction pairs (the compiler pairs FPU ops with `-mfprev1`);
- hardware loops (`rep N { … }`);
- 64-bit multiply-accumulate;
- saturating add variants, `smin`/min-max and `clz`;
- register-pair loads, post-increment addressing and predicated blocks.

fm1-emu does not yet execute every class (see `docs/TODO.md`).

## Open checks on a real unit

| Check | Why it matters | How |
|---|---|---|
| Legible part-number line | Confirms AC7911B8 beyond the pin and photo evidence. | A sharper photo of the chip (6 screws, one under the sticker). |
| Actual CPU clock | The budget assumes 240 MHz. At 320 MHz there is a third more headroom. | Read the PLL configuration the SPL leaves (SDK `cpu/wl82` clock registers), or time a known loop against TIMER4. |
| Cycles vs instructions | The emulator counts instructions, not cycles. Flash (XIP) fetches and divides cost more. | Time `sid_render` blocks with TIMER4 on the device. |
| Second core RAM issue | Some units misread RAM on core 1. | Felucca's core-1 self-test, before SIDOU ever uses core 1. |

`docs/hardware-test.md` (M1a Task 7) turns these into checklist items.
