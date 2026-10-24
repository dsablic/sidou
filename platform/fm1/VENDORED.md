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

Not vendored: `fm1_cpu1.{h,S}` (we don't use the second core; `main.c` only
holds it, with `fm1_sys.h`'s `fm1_cpu1_halt`) and
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
    protocol's flash access), and `fm1_usb.h` and `fm1_uart.h` through
    Felucca's `src/usb.c` and the adapted `usb/midi_uart.c`, which it includes
    with `src/ota.c` and its flash hooks `ota_flash.h` (see below). The TIMER5 interrupt in `main.c` calls their
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
file. Copy `src/usb.c` as it is, then put back its two SIDOU hooks if upstream has not taken them (see below). For
`usb/midi_uart.c`, diff the new upstream file against this commit's and carry
the changes over onto SIDOU's version.

## Felucca USB stack (`src/usb.c`, two hooks) and TRS MIDI driver (`usb/`)

Since the USB-MIDI out fix (2026-10-19), the app uses Felucca's USB stack
`src/usb.c`. It is the same copy the update loader builds from (below), byte
for byte but for two marked hooks. SIDOU's adapted copy (`usb/usb.c`) is gone.
The fork could not send USB-MIDI out on the FM-1, and the reasons were not in
its source ([docs/hardware-results/usb-out-diagnosis.md](../../docs/hardware-results/usb-out-diagnosis.md)).

The rule now: **no fork of `usb.c`**. The file stays Felucca's, with at most
one or two hooks, each:

- marked `/* SIDOU hook (upstream candidate): … */`;
- inert when its macro is undefined;
- proposed upstream ([docs/upstream-reports.md](../../docs/upstream-reports.md), item 8).

Everything else SIDOU needs goes in `midi.c`, around the file.

### The hooks in `src/usb.c`

| Hook | Where | Undefined (Felucca, the loader) | SIDOU's `midi.c` |
|---|---|---|---|
| `FELUCCA_USB_MANUFACTURER`, `FELUCCA_USB_PRODUCT`: the bytes of a whole string descriptor each | `STR1`, `STR2` | "Hügelton Instruments", "Felucca" (the loader keeps "Felucca Update") | "SIDOU contributors", "SIDOU FM-1" (the installers find a port by "FM-1") |
| `FELUCCA_MIDI_IN_RAW(pkt)`: every EP1 OUT event packet, before `midi_in_event`'s filter | `midi_in_event`, first | nothing | `usb_raw_in`: the packet into SIDOU's ring |

The loader builds from the same file with neither macro defined. Its
`loader.bin`, and so the package's `ota.bin`, is byte-identical with or
without the hooks (checked 2026-10-20).

### Use

- **Including it:** `midi.c` defines the hooks, then includes `src/usb.c`,
  SIDOU's `usb_audio.c` (below), SIDOU's `usb/midi_uart.c` and `src/ota.c` into
  one translation unit. The hooks are:
  - `RING_PUBLISH()`, `FELUCCA_OTA 1` and `FELUCCA_ID`;
  - `FELUCCA_CDC 0` and `FELUCCA_UAC 1`: Felucca's own switches, MIDI
    streaming on EP1 and the USB audio input on EP4 (since 2026-10-23, below), with
    no CDC console; `FELUCCA_UAC_48K` and `FELUCCA_UAC_AS_FIRST` at `usb.c`'s
    defaults (on); `HALF_FRAMES`, Felucca's block, is SIDOU's (128);
  - the two above.

  The switches and the two string hooks are in `usb_config.h`, which the host
  harness includes too (`host/crates/sidou-pack/harness/uac_harness.c`), so the
  descriptors it tests are the device's.

  `usb.c`'s `sysex_byte` is renamed to `ota_sysex_byte` by a macro around the
  include, because `app/sysex.h` has a `sysex_byte` too. `fm1_delay_ms` goes to
  `usb_reset_wait` (below). No other token is touched.
- **MIDI in:** `usb_raw_in` (the TIMER5 interrupt, in `usb_poll`) puts every
  event packet as it is into SIDOU's ring, stamped by its poll. SIDOU's parser
  takes all of it (`app/midi_usb.c`): channel messages, system common (song
  position, song select, tune request), every real-time byte, and SysEx. A
  full ring is a gap for the parser.
- **Back-pressure:** `ep1_take` NAKs the host while `usb.c`'s own MIDI-in ring
  has fewer than `EP1_ROOM` free, and SysEx never enters that ring. So when
  SIDOU's ring has fewer than `EP1_ROOM` free, `midi.c` sets `usb.c`'s read
  index back (SIDOU never reads that ring) until it looks that full, and
  `usb_in_pump` lets it go once SIDOU's ring has the room again. No hook is
  needed for it.

  `usb.c` still fills its own ring through its filter: channel messages and
  F8/FA/FB/FC. Those are copies. `midi.c`'s `usb_in_pump` empties that ring
  at each `midi_drain`, but only while SIDOU's ring has room for another OUT
  packet. That way `usb.c`'s back-pressure (`ep1_take` NAKs while its own ring
  is nearly full) also holds the host off when SIDOU's ring fills.
- **SysEx in:** SIDOU's frames reach its parser in the raw stream. `usb.c` also
  collects each SysEx frame for the update protocol. `update_poll` frees SIDOU's
  frames from there (`F0 7D 53 44 …` and universal `F0 7E …`), so the
  updater's frame is free for its next request.
- **Kept as Felucca has it:** VID/PID 0x1209:0x0001. `sysex_byte` sets `usb.uboot_req` on the stock UBOOT
  soft key `F0 22 24 35 7D F7`; nothing in SIDOU reads it, and SIDOU never
  sends that SysEx.
- **The retry without a host:** `usb_retry` runs `usb_start` once a second
  while no host answers. `usb_start` waits out the controller's 25 ms reset
  inside the call, then its six SIE writes time out at 20000 polls each. In
  fm1-emu that stopped the main loop, and TRS MIDI in with it, for 25 ms, and
  then 2.4 ms (emu-gate `tests/latency.rs`). So `midi_service` calls `midi.c`'s
  `usb_retry_quiet` instead, which takes one step per pass:
  - the reset, `fm1_usb_reset`;
  - 25 ms later, `usb_probe`: the attach and one SIE write, given 2000 polls,
    then the reset again. In fm1-emu that is 8,040 instructions. A silent SIE
    ends the try;
  - if the SIE answered, 25 ms more in reset, then `usb_retry`.

  `midi.c` routes `usb.c`'s one `fm1_delay_ms` through `usb_reset_wait`, with a
  macro around the include. So `usb_start` there resets again (the same three
  register writes) and attaches without the wait. At boot, `usb_start` still
  waits the 25 ms.
- **MIDI out:** `midi_send` and the SysEx replies call `midi_out_event` from
  the main loop. `ep1_tx` fills IN packets of up to 64 bytes, SysEx before
  MIDI, as Felucca does.

### USB audio out (`FELUCCA_UAC`, since 2026-10-23)

`usb.c`'s audio input is on, as in Felucca's own firmware: a class-compliant
UAC1 input, 16-bit stereo at 44.1 kHz or 48 kHz (the host's choice, SET_CUR),
on an isochronous asynchronous IN endpoint (EP4), beside USB-MIDI in one
device (bcdDevice 3.28; the configuration descriptor grows from 101 to 178
bytes). Nothing in `usb.c` changed: its producer (`uac_render_start`, then
`uac_tap` a block at a time, which at 48 kHz resamples through
`src/uac_fir.h`'s polyphase FIR) fills a ring that its consumer
(`uac_service`, one packet a USB frame) empties.

- **`src/uac_fir.h`**, the resampler's table, which `usb.c` includes with
  `FELUCCA_UAC_48K`: Felucca's file as it is, from Felucca's tree at `5738617`
  (2026-10-16, the copy at hand; its changes after 1.6.1 do not touch it, nor
  `usb.c`, which is byte-identical there but for SIDOU's hooks). Felucca's own
  test (below) checks every tap against the prototype computed in double
  precision.
- **Who calls the producer.** Felucca calls it from its audio interrupt
  (`audio.c`: `uac_render_start` before a render, `uac_tap` after each 32-frame
  block). SIDOU's audio interrupt runs from `.ram_text` and reads no flash
  (`app.ld`; emu-gate `tests/ram_audio.rs`), and the producer does not fit
  there: it is about 800 bytes of code (432 of them the resampler) against
  RAMTEXT's last bytes, and its 7,680-byte table would have to move to RAM. Nothing
  of it must run during a flash operation (those mask every interrupt). So the
  audio interrupt does nothing for USB audio, and SIDOU's `usb_audio.c`
  (included after `usb.c`, its interface `usb_audio.h`) runs the producer from
  the TIMER5 interrupt, from flash:
  - at the first tick after a render (`main.c`, `audio_block_end` moved and no
    render open), the block is still in the I2S half buffer the render wrote,
    the DMA's free half, until the DMA moves on (which raises the half's
    pending flag; then the block is counted as skipped). While a host records,
    `usb_audio_take` copies it (int16 << 8 back to int16) into a buffer in POOL;
  - `usb_audio_tick`, every tick, runs `usb.c`'s producer on it:
    `uac_render_start`, then `uac_tap` on a quarter of the block (32 frames,
    Felucca's own block size) a tick, so no tick is long (at 48 kHz about
    11,000 instructions);
  - TIMER5 calls `uac_service` nested in a render at the USB poll's tick
    (`usb_audio_isr`), as Felucca's `fm1_timer5_irq` does: `usb_poll` waits for
    the render's end, and a render can outlast a USB frame.

  The block is what the DAC plays (after the effects, the limiter, MASTER and the
  hold's ramps; a held block is silence). The ring's fill is taken when the block
  goes in, its lowest point, so it moves with the render's length from block to
  block (Felucca's taps spread over the render move it less). Within SIDOU's audio
  budget the ring never runs dry (host-tested,
  `host/crates/sidou-pack/tests/usb_audio.rs`).
- **RAM:** in POOL the ring (2,048 bytes, placed by `app.ld`) and the block
  (1,036); in the RAM region the resampler's history (264), EP4's IN buffer (196)
  and `uac` (72), which the compiler merges into globals no linker rule can name
  (emu-gate `tests/usb.rs` checks EP4's buffer is on an 8-byte boundary, as EP1's
  must be). RAMTEXT: none. The table stays in flash.
- **Tests.** Felucca's `tests/uac_test.c`, as it is, from the same tree
  (`host/crates/sidou-pack/harness/felucca/uac_test.c`; only its `main` is
  renamed, by a define), runs on SIDOU's `usb.c` without CDC: the descriptors as
  a host parses them, the ring against packets, the rates, the resampler against
  a double-precision one, the 48 kHz stream. SIDOU's own (`tests/usb_audio.rs`)
  check SIDOU's descriptors and the hand-over in time against Felucca's producer
  fed directly; emu-gate `tests/usb.rs` records the DAC's output through a model
  of EP4, bit for bit at 44.1 kHz.

### How `midi.c` is built (the flags rule)

`midi.c`, the translation unit that contains `usb.c`, is compiled with
Felucca's flags: no `-mcpu` (clang's generic pi32v2) and `-Os`. The rest of
SIDOU uses `-mcpu=r3 -mfprev1 -O2`. It is also compiled with `-fdata-sections`.
`CMakeLists.txt` appends these options to `midi.c` alone (`SIDOU_USB_FIX`,
on by default; off only to reproduce the failure). Why:

- With SIDOU's flags, the compiler inlines `sie_rd` and `sie_wr` into
  `usb_poll`, and the indexed SIE accesses run back to back. On the FM-1,
  EP1 IN then stops after the first replies, while enumeration and EP1 OUT
  keep working. The same source with Felucca's flags works. The flag
  bisection and the codegen comparison are in
  [usb-out-diagnosis.md](../../docs/hardware-results/usb-out-diagnosis.md).
  The likely cause: the SIE's done bit comes before the access has really
  latched.
- `generic` has no FPU, and SIDOU links no soft-float helpers. Keep
  `midi.c` free of floating point; a float there fails to link.
- `usb.c` declares its DMA buffers `aligned(4)`, but on the FM-1 an EP1 IN
  buffer at 4 mod 8 is never sent. With `-fdata-sections` each buffer gets
  its own section. `app.ld` puts them at `.bss`'s start: `ep1tx` on a 64-byte
  boundary, as in the builds tested on the device, then `ep0buf` and `ep1rx`
  on 8-byte boundaries, packed to save RAM.

`main.c` also holds the second core first thing at every boot
(`fm1_cpu1_halt`, `hal/fm1_sys.h`, built there with `FM1_CPU1_USED`), as
Felucca's `fm1_cstart` does.

### Local changes in `usb/midi_uart.c` (50 lines left)

From Felucca's `firmware/src/midi_uart.c` (90 lines) at the HAL's commit:

- Felucca's parser into USB-MIDI packets (`um_len`, the running status,
  the SysEx skip, the real-time filter) is gone. `um_byte` passes every
  byte as it is to `midi_enqueue(b, 2)`, and `app/midi_in.c` parses it in
  the main loop (with SysEx and every real-time byte). `midi.c` renames
  `midi_enqueue` and `midi_lost` to its own `trs_enqueue` and `trs_lost`
  around the include, so they don't clash with `usb.c`'s `midi_enqueue`.
- A DMA ring overrun reports the bytes lost to `midi_lost(n)` (the main
  loop restarts its parser and releases the notes) instead of clearing the
  parser's state and setting `midi_in_overflow`; `um.msgs` is gone.
- The header comment describes the SIDOU version.

## Felucca's sources for the update path (`src/`, `loader/`), unmodified

Laid out as in Felucca's `firmware/` tree, so the loader's relative includes
(`../src/…`, `../hal/…`) resolve as there.

- **Source:** Felucca's `firmware/src/usb.c`, `src/ota.c`, `src/libc.c` and
  `firmware/loader/` (`crt0_ldr.S`, `ldr_core.c`, `loader.c`, `loader.ld`)
  at the same commit, byte for byte. The one exception is `src/usb.c`'s two
  SIDOU hooks (above). They are inert in the loader's build, whose
  `loader.bin` is unchanged by them.
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
  `midi.c` includes it after `src/usb.c` and supplies its hooks, adapted
  from Felucca's `src/ota_hw.c`: the clock (TIMER4), the watchdog, the
  flash hooks in `ota_flash.h` (erases and writes inside the staging area
  `0xE0000..0xE5000` only, in interrupt-off windows and only while the
  audio is stopped, else -16; host-tested), the progress on the screen
  (`ui_update`), and the commit (the boot guard told, USB detached,
  `fm1_enter_update`). `FELUCCA_ID` is SIDOU's identity, `SIDOU_PRODUCT`
  (`FM-1_900` for development builds, the same as Felucca's development
  builds; distinct digits are for later, with the community).
- `src/usb.c` is also the app's own USB stack, with its two hooks (above).

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

### Checked again (2026-10-22, pre-flash safety)

- **Still 1.6.1, nothing to re-vendor.** `v1.6.1` is still the newest Felucca release and still
  `a53ecf42`. Every vendored file (`hal/`, `loader/`, `src/libc.c`, `src/ota.c`) was compared
  byte for byte with the tag again: identical. `src/usb.c` differs from the tag only by the two
  SIDOU hooks above (`diff` shows nothing else). No upstream change between the vendored commit
  and 1.6.1 reaches SIDOU's image, because they are the same commit. Commits after the tag, to
  `63f88ec1` (2026-10-22), touch `README.md`, `CREDITS.md`, `LICENSING.md`, `eng_phase.c`,
  `ui_menu.c`, `fx.c`, `tools/gen_waves.py`, the web page and a test: none vendored.
- **The loader is Felucca 1.6.1's, by its bytes.** The fm1 build's `loader/loader.bin` (8,216
  bytes, SHA-256 `2cde42b729d787a3e591ed4eaaffbbf33cebee7d58421d33bf6b6d27cc03f1aa`) is the
  loader that Felucca's own release package `felucca-1.6.1.fwsc` (SHA-256 `dff3a38f…d909`)
  carries, unwrapped from its `ota.bin`. `sidou-pack --verify` reports it as
  `loader_release Felucca 1.6.1` and `loader_identity ota-FM-1_900`; `tools/preflash` fails a
  package whose loader is anything else, and `ota_step1.rs`'s
  `the_built_loader_is_felucca_1_6_1s` fails the build's. The pinned SHA-256 is
  `sidou_pack::FELUCCA_LOADER_SHA256`: re-vendoring a newer loader means updating it there.
