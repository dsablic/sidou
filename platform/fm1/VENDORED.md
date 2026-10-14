<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Vendored code in `platform/fm1/`

## Felucca HAL (`hal/`)

- **Source:** <https://github.com/hugelton/Felucca>, `firmware/hal/`
- **Commit:** `a53ecf428c29c780108fd70a31b71a32a6b5c313` (Felucca 1.6.1,
  2026-10-15 +0900)
- **Licence:** GPL-3.0-only, © 2026 Leo Kuroshita (@kurogedelic), Hügelton
  Instruments. Each file keeps its original header.
- **Local changes:** none. The files are byte-identical to upstream.

| File | What it is |
|---|---|
| `fm1_adc.h` | SARADC: MASTER pot and battery (PB1, analog only) |
| `fm1_audio.h` | ALNK0 I2S audio out, double-buffered DMA |
| `fm1_cc.h` | `FM1_INLINE` |
| `fm1_flash.h` | SPI NOR erase/program from `.ram_text` |
| `fm1_gpio.h` | GPIO port registers |
| `fm1_input.h` | key/button/encoder matrix and LEDs (TIMER5 scan) |
| `fm1_irq.h` | interrupt controller, vector table, fault record |
| `fm1_isr.S` | IRQ entry wrappers `isr_timer5`, `isr_alnk0` |
| `fm1_lcd_hw.h` | LCD SPI1 wiring and backlight |
| `fm1_led_anim.h` | power-on LED sweep frames |
| `fm1_sys.h` | P33, watchdog, reset reason, reboot, UBOOT entry |
| `fm1_time.h` | TIMER4 24 MHz time base |
| `fm1_timer.h` | TIMER5 10 kHz tick |
| `fm1_uart.h` | UART1 MIDI IN |
| `fm1_usb.h` | USB0 device controller registers |
| `fm1_vec.S` | the 128 fatal-vector stubs |
| `fm1_xip.h` | XIP window addressing |

Not vendored: `fm1_cpu1.{h,S}` (we don't use the second core) and
`fm1_guard.h` (optional, can be added later).

### Using the HAL

- The headers define `static` functions and state and are meant to be
  included by one translation unit each. `fm1_irq.h` also defines two
  non-static symbols, `fm1_crash` and `fm1_fault_c`, so a second
  translation unit that includes it (directly or through `fm1_audio.h`
  or `fm1_timer.h`) fails to link.
- Who includes what:
  - `main.c`: `fm1_sys.h`, `fm1_irq.h`, `fm1_audio.h`, `fm1_time.h`,
    `fm1_timer.h`;
  - `input.c`: `fm1_input.h` (with `fm1_time.h`, `fm1_gpio.h` and
    `fm1_led_anim.h`) and `fm1_adc.h`. The scan state `fm1_in` is static
    there, so the TIMER5 interrupt in `main.c` reaches `fm1_input_tick`
    through `input_tick_isr`;
  - `lcd.c`: `fm1_lcd_hw.h` and `fm1_time.h`.

  None of `input.c`'s or `lcd.c`'s headers includes `fm1_irq.h`;
  `fm1_time.h` holds only `static inline` helpers and no state, so several
  files may include it.
- Include `fm1_sys.h` before `fm1_irq.h`: `fm1_irq.h` uses its
  `FM1_RESET_PREP`.
- The including file defines `static void fm1_fault(const fm1_crash_t *)`.
- `fm1_isr.S` calls the C bodies `fm1_timer5_irq` and `fm1_alnk0_irq`.
- PA2 is the LCD backlight and the key matrix's `/OE`, and the TIMER5 scan
  writes port A: `lcd_init` and `input_init` run before interrupts are
  enabled, and nothing else writes port A from the main loop.
- `hal_check.c` (target `fm1_hal_check`) compiles every header and both
  `.S` files in the fm1 build.

### Adapted from Felucca (not vendored)

`crt0.S` and `app.ld` follow Felucca's `firmware/crt0.S` and
`firmware/app.ld` at the same commit and keep its licence (GPL-3.0-only)
and copyright next to ours. Changes: `app.ld` drops the second core's
`.c1_text`, collects `.init_array`, and adds `ASSERT`s for the image layout
and region overlaps.

`lcd.c` is adapted the same way from Felucca's `firmware/src/lcd.c`: the
ST7789 init sequence and the strip transfer left running until the next
access. Changes: delays from TIMER4 instead of spin loops, no fill, DISPON
left to the UI's first picture, and one strip buffer instead of Felucca's
layers.

### Updating

Copy the HAL files above from the new commit, update the commit here and in
[LICENSING.md](../../LICENSING.md), and describe any local change in this
file.

## font8x8 (`font/`)

- **Source:** <https://github.com/dhepper/font8x8>, `font8x8_basic.h`
- **Commit:** `8e279d2d864e79128e96188a6b9526cfa3fbfef9` (2019-07-08)
- **Licence:** public domain (Daniel Hepper, after Marcel Sondaar and IBM's
  VGA fonts); see `REUSE.toml` and `LICENSES/LicenseRef-PublicDomain.txt`.
- **Local changes:** none. The array is not `const`, so its 1 KiB lands in
  `.data` (RAM). Only `ui.c` includes it: the header defines the array.
- A placeholder until our own PETSCII-style font.
