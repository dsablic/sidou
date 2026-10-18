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
    through `input_tick_isr`. `input_raw_keys`, for `main.c`'s power-on
    UBOOT gesture before memory is set up, repeats `fm1_input_init`'s pin
    setup and uses `fm1__sr_word`, `fm1__rows` and `FM1_KEYMAP` without the
    scan state;
  - `lcd.c`: `fm1_lcd_hw.h` and `fm1_time.h`;
  - `midi.c`: `fm1_sys.h`, `fm1_time.h` and `fm1_flash.h` (the update
    protocol's flash access), and `fm1_usb.h` and `fm1_uart.h` through the
    vendored `usb/usb.c` and `usb/midi_uart.c`, which it includes with
    `src/ota.c` and its flash hooks `ota_flash.h` (see below). The TIMER5 interrupt in `main.c` calls their
    polls through `midi_usb_isr` and `midi_uart_isr`; as in Felucca, never
    the USB poll nested in the audio interrupt (it waits for the render's end). `fm1_flash.h`'s
    `RAMFN` code lands in `.ram_text` (`app.ld` keeps it), and is called
    through `FL_FAR` pointers: flash and SRAM code are out of each other's
    branch reach ([docs/hardware.md](../../docs/hardware.md));
  - `flash_spike.c`, only in the flash timing spike's build
    (`-DSIDOU_FLASH_SPIKE=ON`, never released): `fm1_sys.h`, `fm1_time.h`
    and a second copy of `fm1_flash.h`, whose `FL_RANGE_OK` there is the
    spike's two sectors (`flash_spike_guard.h`) instead of the staging
    area. Its `RAMFN` code is static to that file, so the copies don't
    clash;
  - the loader (`loader/loader.c`, its own image, Felucca's unmodified):
    `fm1_time.h`, `fm1_sys.h`, `fm1_flash.h`, and Felucca's own
    `src/libc.c`, `src/usb.c` and `src/ota.c`.

  None of `input.c`'s, `lcd.c`'s or `midi.c`'s headers includes `fm1_irq.h`;
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
and region overlaps. Since M1a Task 5b it also puts crt0 alone in `.entry`
at the image's start and fills `.ram_text` with the audio path, by
function section, ahead of `.text`; the image is `.entry`, `.ram_text`,
`.text` and `.data` back to back ([docs/hardware.md](../../docs/hardware.md),
"Code in SRAM").

`lcd.c` is adapted the same way from Felucca's `firmware/src/lcd.c`: the
ST7789 init sequence and the transfer left running until the next access.
Changes: delays from TIMER4 instead of spin loops, no fill, DISPON left to
the UI's first picture, and windows of any size from two buffers (one filled
while the other is sent) instead of Felucca's layers, with counters of the
windows and SPI bytes sent.

### Updating

Copy the HAL files above from the new commit, update the commit here and in
[LICENSING.md](../../LICENSING.md), and describe any local change in this
file. For `usb/`, diff the new upstream files against this commit's and
carry the changes over onto SIDOU's versions.

## Felucca USB stack and TRS MIDI driver (`usb/`)

- **Source:** <https://github.com/hugelton/Felucca>, `firmware/src/usb.c`
  (1379 lines) and `firmware/src/midi_uart.c` (90 lines)
- **Commit:** `a53ecf428c29c780108fd70a31b71a32a6b5c313`, as the HAL.
- **Licence:** GPL-3.0-only, © 2026 Leo Kuroshita (@kurogedelic), Hügelton
  Instruments. Both files keep their original header; each starts with a
  comment that lists its SIDOU changes.
- **Use:** like Felucca's `felucca.c` and the update loader, `midi.c`
  defines the hooks and then includes both files into one translation unit:
  `RING_PUBLISH()`, `FELUCCA_OTA`, `midi_enqueue(data, source)`,
  `midi_in_room()` and `midi_lost(n)`, after `fm1_time.h`. Their own
  `#include "../hal/..."` lines reach `hal/`. Felucca's `libc.c` is not
  needed: neither file calls it, and `libc_min.c` has the `memset` and
  `memcpy` the compiler may call.

### Local changes in `usb.c` (663 lines left)

- **Stripped:** the CDC-ACM console (`FELUCCA_CDC`: EP2 and EP3, line
  coding, `usb_cdc_switch`, the IAD layouts of `FELUCCA_USB_LAYOUT`) and the
  USB audio input (`FELUCCA_UAC`, `FELUCCA_UAC_48K`, `FELUCCA_UAC_AS_FIRST`:
  EP4, the sample ring and its 48 kHz resampler, the rate requests, the
  audio streaming interface). Felucca does not build an editor endpoint into
  `usb.c`: its editor shares the SysEx frames of the update protocol (below).
  The conditionals were resolved with `unifdef -DFELUCCA_CDC=0
  -DFELUCCA_UAC=0 -DFELUCCA_UAC_48K=0 -DFELUCCA_UAC_AS_FIRST=0
  -DFELUCCA_UAC_TONE=0`, then the descriptor macros left over were reduced
  by hand to what that build sends: device class 0, audio control
  (interface 0) and MIDI streaming (interface 1) on EP1, bcdDevice 3.00,
  VID/PID 0x1209:0x0001 (the pid.codes test id; see
  [docs/TODO.md](../../docs/TODO.md)). `get_desc` and `CLEAR_FEATURE` lose
  their CDC cases.
- **UBOOT soft key removed:** `sysex_byte` no longer compares a SysEx with
  `F0 22 24 35 7D F7`, and `usb.uboot_req` is gone. Felucca's main loop
  enters mask-ROM UBOOT on it; SIDOU never sends or handles it (a safety
  rule of the M1a plan).
- **MIDI in:** Felucca's in ring (`midi_in_q`, `midi_in_ms`,
  `midi_in_source`, `mi_w`, `mi_r`, `midi_in_overflow`, its own `fm1_ms`)
  and its `midi_enqueue` are gone. `midi_in_event` passes every event packet
  as it is to the includer's `midi_enqueue(pkt, 1)`, where Felucca kept
  only channel messages and four real-time bytes: SIDOU parses in the main
  loop (`app/midi_usb.c`, `app/midi_rx.c`). `ep1_take`'s back-pressure test
  uses `midi_in_room()`.
- **Update protocol:** `sysex_byte` and the SysEx part of
  `midi_in_event` are only built with `FELUCCA_OTA`, which `midi.c` turns
  on. The rest of the `FELUCCA_OTA` code (the received frame, the SysEx TX
  queue, `ota_wire_send`, `ota_frame_get`, `ota_frame_done`, the upgrade
  request `F0 22 24 35 7F F7`) is unchanged, and so is the `FELUCCA_LOADER`
  code, which this copy never builds: the loader builds from Felucca's own
  `src/usb.c` (below).
- **`usb_detach`:** in the app it masks interrupts around `usb.up = 0` and
  `fm1_usb_off`, so no TIMER5 `usb_poll` that already passed its `usb.up`
  check talks to a controller going off.
- **Retry without blocking:** `usb_start` is split into the reset and
  `usb_attach_now`. `usb_start` (boot, interrupts off) still waits 25 ms
  between them; `usb_retry` (main loop, once a second while USB is off)
  starts the reset and attaches on a later call, 25 ms on. The attach stops
  writing at the third consecutive SIE timeout (the point where Felucca
  leaves USB off anyway) instead of timing out each of its six writes at
  20000 polls apiece.
- **Strings:** manufacturer "SIDOU contributors" and product "SIDOU"
  instead of "Hügelton Instruments" and "Felucca".
- **MIDI out:** `midi_out_event` is called from the main loop
  (`midi.c` `midi_send`, which checks the room first and drops nothing
  itself), not from the audio interrupt, and is no longer marked unused;
  its and `ep1_tx`'s comments say so. The ring and the bus reset's flush
  are unchanged.
- **IN packets of 16 bytes:** `ep1_tx` puts at most 4 event packets
  (`EP1_IN_MAX`, 16 bytes) into an EP1 IN packet, SysEx and MIDI alike,
  where Felucca fills up to 64. Defensive, after a device that sent no IN
  packet ([docs/hardware-test.md](../../docs/hardware-test.md), section 8):
  a longer reply goes as several packets, one a poll.
  Only a bus reset flushes the ring: a `SET_CONFIGURATION(0)` without one
  stops `ep1_tx` but leaves what is queued (up to 64 packets) to go out
  when the host configures the device again.
- The header comment describes the SIDOU build, and `SET_INTERFACE`'s
  comment no longer mentions the audio stream.

### Local changes in `midi_uart.c` (50 lines left)

- Felucca's parser into USB-MIDI packets (`um_len`, the running status,
  the SysEx skip, the real-time filter) is gone: `um_byte` passes every
  byte as it is to `midi_enqueue(b, 2)`, and `app/midi_in.c` parses it in
  the main loop (with SysEx and every real-time byte).
- A DMA ring overrun reports the bytes lost to `midi_lost(n)` (the main
  loop restarts its parser and releases the notes) instead of clearing the
  parser's state and setting `midi_in_overflow`; `um.msgs` is gone.
- The header comment describes the SIDOU version.

## Felucca's sources for the update path (`src/`, `loader/`), unmodified

Laid out as in Felucca's `firmware/` tree, so the loader's relative includes
(`../src/…`, `../hal/…`) resolve as there.

- **Source:** Felucca's `firmware/src/usb.c`, `src/ota.c`, `src/libc.c` and
  `firmware/loader/` (`crt0_ldr.S`, `ldr_core.c`, `loader.c`, `loader.ld`)
  at the same commit, byte for byte.
- **Licence:** GPL-3.0-only, © 2026 Leo Kuroshita, Hügelton Instruments.
- **The update loader** is built from them unmodified, with exactly the
  commands of Felucca's `tools/build.py` `build_loader()` (CMake target
  `felucca_loader`; no SIDOU toolchain flags): `loader/loader.bin` in the
  build, the RAM image for `0x01C0A800`, which `sidou-pack` wraps into the
  package's `ota.bin` with Felucca's own LZ4 compressor. Once step 1
  commits, the SPL runs this loader at every power-on until an install
  completes, so it is the loader proven on hardware: built from Felucca's
  own tree at this commit with the same toolchain, its `loader.bin` and the
  wrapped `ota.bin` are byte-identical to ours. It is Felucca's loader in
  every respect: USB "Felucca Update", 1209:0002, identity `ota-FM-1_900`,
  the marker `FELUCCA-LOADER-1` (which Felucca, sloop and melodee look for),
  and its own fallback to UBOOT on the stock soft key (the app never sends
  that SysEx, and never handles it).
- **`src/ota.c`, step 1 of the update protocol,** is also the app's:
  `midi.c` includes it after `usb/usb.c` and supplies its hooks, adapted
  from Felucca's `src/ota_hw.c`: the clock (TIMER4), the watchdog, the
  flash hooks in `ota_flash.h` (erases and writes inside the staging area
  `0xE0000..0xE5000` only, in interrupt-off windows and only while the
  audio is stopped, else -16; host-tested), the progress on the screen
  (`ui_update`), and the commit (the boot guard told, USB detached,
  `fm1_enter_update`). `FELUCCA_ID` is SIDOU's identity, `SIDOU_PRODUCT`
  (`FM-1_900` for development builds, the same as Felucca's development
  builds; distinct digits are for later, with the community).
- `src/usb.c` is the loader's only: the app uses the adapted copy in
  `usb/usb.c`.

### Checked against the v1.6.1 tag (2026-10-18)

Felucca's tag `v1.6.1` is commit `a53ecf4` itself, so the copies above already are
1.6.1's, and nothing was re-vendored. `loader/crt0_ldr.S`, `ldr_core.c`, `loader.c`,
`loader.ld` and `src/ota.c`, `src/usb.c`, `src/libc.c` were compared byte for byte with the
tag: no difference (diff summary: empty). The 1.6.1 release note, "a package made for
another device is refused before anything is erased", is therefore already in both places:

- the loader (`ldr_core.c`, `ldr_session`): `flash.bin` must be exactly `LDR_APP_HI`
  (0x93000) bytes, else it returns -14 before the chip-key check, the CRC pass and every
  erase;
- step 1 (`src/ota.c`, `update_session`, the app's own copy): `flash.bin` must be exactly
  `OTA_FL_LEN`, else FAILED -15 before the staging area is erased.

Tests, in `host/crates/sidou-pack/tests/ota_step1.rs`:
`another_boards_package_is_refused_before_anything_is_erased` (step 1, -15, flash
untouched) and `the_loader_refuses_another_boards_package_before_anything_is_erased` (the
loader's `ldr_session` on `harness/ota_harness.c`, which includes the unmodified
`loader/ldr_core.c` with counting flash hooks: a flash.bin 4 KiB short and 4 KiB over give
-14 with zero erases, programs and record clears; the right size, as the control, writes).
Felucca commits after the tag (to 2026-10-16) change only `eng_phase.c` and `ui_menu.c`, which SIDOU does not vendor.
