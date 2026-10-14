<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# TODO

Follow-ups that aren't tied to a milestone task yet.

## Apply for a pid.codes PID before 1.0

- **Where:** `platform/fm1/usb/usb.c`, the device descriptor
  (`FELUCCA_USB_PID`).
- **Problem:** SIDOU enumerates as VID/PID 0x1209:0x0001, pid.codes' test
  id, as Felucca does. It is meant for the bench only, and hosts may mix
  SIDOU up with Felucca or anything else that uses it.
- **Action:** apply for a pid.codes PID before 1.0 (an open-source project
  qualifies) and set it in the descriptor. The update loader enumerates as
  0x1209:0x0002 (as Felucca's loader) and needs its own PID too.

## fm1-emu does not emulate USB

- **Where:** fm1-emu at rev `e52f4640157f490f8c3af778dd044e9d33e989e8`.
- **Problem:** the USB0 registers (`0x11800..0x1183F`) are plain storage:
  the SIE never reports a finished access, so SIDOU's USB stack times out
  at boot, leaves USB off and retries once a second. No USB MIDI reaches
  the firmware, and the cost of `usb_poll` with a host attached cannot be
  measured. The emulator tests play MIDI through the TRS jack only; the
  USB packet conversion and the MIDI ring are host-tested. The update
  protocol runs over USB only, so it cannot be driven in fm1-emu either: its
  step 1 (`ota.c`) is host-tested against packages from `sidou-pack`
  (`crates/sidou-pack/tests/ota_step1.rs`), and an install is a hardware
  check (M1a Task 7).
- **Action:** test USB MIDI on hardware (the M1a hardware checklist), and
  measure `usb_poll` there: three SIE round trips per poll at 2 kHz with
  no traffic. Ask upstream whether a USB device model is planned.

## USB retry without a host costs main-loop time on hardware too

- **Where:** `platform/fm1/usb/usb.c`, `usb_retry` and `usb_attach_now`.
- **Problem:** Felucca found that the SIE does not answer without a cable,
  which is also how a battery-powered FM-1 runs. While USB is off,
  `usb_retry` runs once a second: `fm1_usb_reset` (controller off, pads
  detached), then 25 ms later `usb_attach_now`, whose `fm1_usb_attach`
  turns on the PLL48 USB clock, the PHY and the D+ pull-up, and whose SIE
  writes then time out three times at 20000 polls each. Nothing turns the
  controller off after that failed attach, so the clock, PHY and pull-up
  stay on until the next retry's reset: on for about 975 ms of every
  second, off for 25 ms. The cost in fm1-emu, which behaves the same way:
  the main loop went from 1.4M to 2.0M instructions/s idle with MIDI in,
  0.4M/s of it these timeouts (about 1.75 ms of blocked main loop a second
  at 240 MHz on hardware, if the hardware's timeouts are as long) and the
  rest the per-wake MIDI drain check. The supply current with the PHY on
  and no cable is unmeasured.
- **Action:** on hardware without a cable, measure the main loop's blocked
  time and the supply current across a retry. If either matters, back the
  retry off (e.g. after a few failures, retry every 10 s, or only when VBUS
  is seen, if the chip reports it) and shorten the attach's timeout.

## SIE timeouts spin inside the TIMER5 interrupt

- **Where:** `platform/fm1/usb/usb.c`, `sie_wr` and `sie_rd` (inherited
  from Felucca), called by `usb_poll` from TIMER5 at 2 kHz.
- **Problem:** each SIE access waits for the controller for up to 20000
  polls. If the SIE stops answering while USB is up (a dead clock, an
  unplug the stack does not notice), every `usb_poll` spends several
  timeouts (each about 0.6 ms at 240 MHz, longer than the 0.5 ms poll
  period) inside TIMER5, which outranks the audio interrupt, until 50
  consecutive timeouts set `usb.up = 0`. That can starve the audio render
  for tens of milliseconds and drop blocks.
- **Action:** on hardware, unplug the cable during playback and while the
  host suspends, and watch for audio dropouts. If the SIE can die while up,
  shorten the in-interrupt timeout or give up after the first timeout of a
  poll.

## Packages carry M-VAVE's flash head: clear it before a public release

- **Where:** `host/crates/sidou-pack` (`inputs.toml`, `flash_image`).
- **Problem:** a safety rule says the package's flash head is byte-identical
  to stock V15, so a tool that writes the whole flash.bin leaves the head
  as it was (M-VAVE's isd_config: the 360 MHz clock setting, the chip key).
  `sidou-pack` therefore copies the first 0x4000 bytes of M-VAVE's official
  `FM-1.fwsc` (fetched, SHA-256 pinned, never committed) into every package.
  Felucca, sloop and melodee packages carry a head generated from SDK
  defaults instead (15,031 of its 16,384 bytes differ from stock V15's),
  which their loaders never write. The stock head holds JieLi's SPL
  (Apache-2.0) and M-VAVE's configuration, whose licence M-VAVE does not
  state.
- **Action:** before publishing packages (releases, a web installer), get
  M-VAVE's permission or decide on another head; the CI artifact is
  internal.

## The SPL must decode lz4_flex's LZ4 blocks

- **Where:** `host/crates/sidou-pack` (`wrap_loader`).
- **Problem:** the update loader's blocks come from `lz4_flex` (the plan's
  choice). Felucca's packages, proven on hardware, use Felucca's own greedy
  compressor: the same LZ4 block format, other bytes (35 bytes longer for
  the M1a loader). Felucca's decoder reads `lz4_flex`'s blocks back exactly,
  but the SPL's decoder has not seen them.
- **Action:** on hardware, install a `sidou.fwsc` (M1a Task 7) and check
  that the loader starts. If it does not, port Felucca's `lz4blk.py`
  compressor.

## Report fm1-emu: decrementing register block load is not executed

- **Where:** fm1-emu (github.com/joakimeriksson/fm1-emu) at rev
  `e52f4640157f490f8c3af778dd044e9d33e989e8`.
- **Problem:** fm1-isa decodes `r_4_mov_r_4` (`{R*4} = [-R]`, mask
  `0x0804fff0`, match `0x0000eb40`). fm1-core executes only the `[R+]` and
  `[R++]` block moves, so stepping it fails with
  `Unsupported { name: "r_4_mov_r_4" }`.
- **Repro:** JieLi clang 4.0.1 `-mcpu=r3 -O2` emits `41 eb 0a 00`
  (`{r1, r3} = [-r1]`) when loading the previous element of a two-float
  array, as in `interpolate()` in `core/sid/sid_filter.c`. Running our bench
  image (`build/fm1/bench/fm1/sid_bench_1.bin`, entry `0x02000000`) faults at
  PC `0x02000e50`.
- **Expected semantics** (what our workaround implements, and what the
  bit-exact host match confirms): the registers load in ascending order from
  `base - 4 * count`, and the base is not written back.
- **Our workaround:** `step_block_load_dec` in
  `host/crates/emu-gate/tests/common/mod.rs`, shared by the emulator gate
  and the boot test. Remove it once upstream supports the instruction.
- **Action:** open an issue (and possibly a PR) upstream with the above, then
  bump the pinned rev.

## fm1-emu does not model P33 or the watchdog

- **Where:** fm1-emu at rev `e52f4640157f490f8c3af778dd044e9d33e989e8`.
- **Problem:** the P33 interface (`P33_CON` 0x13E08, `P33_DAT` 0x13E0C)
  is not modelled: reads return 0, so `fm1_wdt_arm`, `fm1_wdt_feed` and
  `fm1_reboot` (Felucca's `fm1_sys.h`) run without effect. The boot test
  cannot tell whether the firmware feeds the watchdog, or whether a crash
  would reset the chip.
- **Action:** ask upstream for a P33 model with a watchdog that resets the
  emulated chip when it is not fed; until then, check watchdog behaviour on
  the device.

## Possible fm1-emu contributions (only if profiling needs them)

fm1-emu still stops on some pi32v2 instruction classes: saturating `R += R`
variants, register `&=`/`^=` masks, and some FPU and accumulator classes. If
a profiled hot spot needs one of them in hand-written assembly, add support
upstream first, so the emulator gate's bit-exact host-vs-target check keeps
working.
