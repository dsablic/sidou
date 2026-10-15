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

## USB audio out, so a computer can record SIDOU over the cable

- **Where:** `platform/fm1/usb/usb.c` (SIDOU's USB-MIDI-only copy) and
  `platform/fm1/audio.c`.
- **Problem:** SIDOU's USB is MIDI only. Felucca's USB audio input
  (`FELUCCA_UAC`) was left out of SIDOU's copy, so a DAW can't record the
  FM-1 without an audio interface. Felucca's original
  (`platform/fm1/src/usb.c`, which the update loader builds from) has it,
  tested on units: class compliant UAC1, the master output as 16-bit stereo
  at 44.1 kHz on an isochronous asynchronous IN endpoint (EP4), 48 kHz
  through a polyphase resampler, one packet per USB frame from the TIMER5
  ISR, and a sample ring filled by the audio render.
- **Action:** bring the UAC path back into SIDOU's copy as a composite
  device with USB-MIDI, record the change in `VENDORED.md`, and tap the
  post-slot stereo output.
  - **Audio budget:** measure the ring tap and the packet ISR against the
    audio budget gates (the tap adds to the 2,700 sum, as the scope tap does).
  - **RAM:** account for the ring.
  - **Testing:** fm1-emu has no USB (see below), so host-test the ring and
    resampler, and add hardware checks to
    [hardware-test.md](hardware-test.md): enumeration as a class-compliant
    audio device on macOS, Windows, Linux and iPad; 44.1 and 48 kHz;
    recording in a DAW with no dropouts while the LCD redraws; USB-MIDI
    still working alongside.

## The full boot sequence once after an install (M2 storage)

- **Where:** `app/boot.h`, `views_boot` in `platform/fm1/ui.c`.
- **Problem:** the plan asks for a boot that is short after the first boot.
  Without storage the firmware cannot tell the first boot after an install
  from the others: RAM is no help (whether `.noinit` survives a power cut is
  unknown, and a brownout may keep it; hardware-test.md section 10), so the
  boot is short every time and HOME held at power-on plays the full one.
- **Action:** with M2's flash storage, keep a "seen" flag (or the version
  last booted): play the full sequence when it is missing or differs, then
  set it. HOME held at power-on keeps playing the full one.

## The GitHub avatar and social preview are set by hand

- **Where:** the repository's settings on GitHub (Settings, General, Social
  preview) and the owner's account or organisation picture.
- **Problem:** neither can be set from the repository: GitHub shows a
  generic preview and the owner's own picture until someone uploads them.
- **Action:** the owner sets them manually: the avatar from
  `docs/art/avatar.png` (48 x 48, Cracktro Chrome's S; scale it up by nearest
  neighbour if a larger upload is wanted), and a social preview from
  Cracktro Chrome (`assets/logo/cracktro-chrome.png` or the 2x lettering on
  black, padded to GitHub's 1280 x 640). Redo both if the logo changes.

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
  check ([hardware-test.md](hardware-test.md)).
- **Action:** test USB MIDI on hardware ([hardware-test.md](hardware-test.md),
  section 8), and
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

## The device's flash head is never written

- **Where:** the update loader (`platform/fm1/loader/ldr_core.c`) and
  `host/crates/sidou-pack`.
- **Note:** the loader writes only the app area `[0x4000, 0x93000)`, as
  Felucca's does, so an FM-1 keeps its own head (M-VAVE's SPL and
  isd_config, with the 360 MHz clock setting) whatever a package carries.
  SIDOU's packages carry an SDK-default head built as Felucca's
  `fm1pkg_make.py` builds it (the SDK's SPL, Apache-2.0); no byte of
  M-VAVE's firmware is in them. The way back to stock V15 keeps `ota.c`'s
  head-identity check (V15's official loader rewrites the head, so its
  package must carry the device's head), host-tested with the V15 package
  (`crates/sidou-pack/tests/ota_step1.rs`).
- **Action:** none; any future SIDOU tool must keep the head unwritten.

## Recovery paths, if an FM-1 stops booting

- **Where:** the hardware checklist ([hardware-test.md](hardware-test.md)), before any
  hardware flash.
- **Note:** the boot guard enters UBOOT after two early deaths, and the
  update loader (Felucca's) stays in place until an install completes. If
  the device is still stuck, these community recovery paths exist:
  FM-1-transporter (<https://github.com/kurogedelic/FM-1-transporter>),
  and MvaveFM1Unbricker with jl-uboot-tool. How each is used, and in which
  device state it works, is still to be confirmed on a real unit. Return to
  stock with M-VAVE's updater or the installer's "Return to official V15".
- **Action:** check that each recovery path works on a real unit before
  SIDOU packages are offered to anyone ([hardware-test.md](hardware-test.md),
  sections 0, 2 and 10).

## The identity FM-1_900 is shared with Felucca's development builds

- **Where:** `SIDOU_PRODUCT` (`platform/fm1/CMakeLists.txt`) and
  `sidou-pack --product`.
- **Problem:** installers take the identity as `FM-1_<digits>` and use it
  to check an install; Felucca, sloop and melodee development builds all
  answer `FM-1_900` (releases `FM-1_9XY`), and so does SIDOU. An installer
  cannot tell them apart, only that the install reports what the package
  carries.
- **Action:** agree distinct digits with the community before SIDOU
  releases (e.g. a range for SIDOU), and use them in release builds.

## Hardware results

- **Where:** [hardware-test.md](hardware-test.md), filled in and attached to
  an issue.
- **Status:** none recorded yet; no FM-1 has run SIDOU. Hardware flashing
  waits until a tester with the recovery tools at hand has run sections 0
  to 2, 9 and 10.
- **Open hardware-only checks it collects:** install and resume (the loader
  appearing as Felucca's), the way back to V15, the boot guard ending in
  UBOOT, `.noinit` across watchdog and P33 resets, the watchdog in UBOOT,
  quick power cycles and brownouts, USB MIDI and its edge cases (no cable,
  unplug, host sleep, the SIE timeouts above), the TRS adapter type, the
  LCD at 12 MHz SPI, the button labels, and the measurements of
  docs/hardware.md's open checks (CPU clock, cycles against instructions,
  the SRAM audio path's gain, latencies, the part number).

## Report fm1-emu: decrementing register block load is not executed

- **Where:** fm1-emu (github.com/joakimeriksson/fm1-emu) at rev
  `e52f4640157f490f8c3af778dd044e9d33e989e8`.
- **Problem:** fm1-isa decodes `r_4_mov_r_4` (`{R*4} = [-R]`, mask
  `0x0804fff0`, match `0x0000eb40`). fm1-core executes only the `[R+]` and
  `[R++]` block moves, so stepping it fails with
  `Unsupported { name: "r_4_mov_r_4" }`.
- **Repro:** JieLi clang 4.0.1 `-mcpu=r3 -O2` emits `41 eb 0a 00`
  (`{r1, r3} = [-r1]`) when loading the previous element of a two-float
  array, as `interpolate()` in `core/sid/sid_filter.c` did until M1b Task 5
  (the bench image before that, `build/fm1/bench/fm1/sid_bench_1.bin`,
  entry `0x02000000`, faults at PC `0x02000e50`). Since Task 5 the cutoff is
  a table lookup and no SIDOU code emits it (the gates count 0 emulated block
  loads); the workaround stays for whatever emits it next.
- **Expected semantics** (what our workaround implements, and what the
  bit-exact host match confirms): the registers load in ascending order from
  `base - 4 * count`, and the base is not written back.
- **Our workaround:** `step_block_load_dec` in
  `host/crates/emu-gate/tests/common/mod.rs`, shared by the emulator tests.
  fm1-emu's own command line has no workaround: until Task 5 it stopped
  SIDOU in `sid_cutoff_hz` during boot (`fm1-emu --fwsc
  build/fm1/sidou.fwsc`). Remove it once upstream supports the instruction.
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

## The 6502 must fit the audio headroom the gates leave

- **Where:** `host/crates/emu-gate/tests/slots_budget.rs` (it prints
  `headroom_for_6502_instructions_per_sample`), `tests/common/mod.rs`,
  [decision record 0004](decisions/0004-effect-algorithms.md), "Worst blocks".
- **Problem:** the switch gate holds two chips at the SID's worst block (1,440),
  the LIVE view's tap (4.5; on at power-on), the ISR (100) and the slots' worst
  switching block (964) to the 2,700 audio target, which leaves **191 per
  sample** for the 6502 (M3; 227 before M1b Task 6's non-linear 6581 filter,
  about 38 per sample per chip, and without the tap). The old budget reserved
  400. The SID's worst block is the power-on transient of noise combined with
  another waveform (the register shifting until it locks up), in a scenario's
  first block, now on the 6581 (`noise_tri_ring_0800`, 1,439.67; the 8580's is
  1,409.87). Its profile (`noise_tri_ring_0800`, 8580, one
  chip, block 0, 88.4k instructions): `sid_osc_run_combined` 24,690,
  `sid_osc_advance_slow` 10,114 (the quiet-stretch step, ~60 per call),
  `sid_osc_cycle` 5,004, `sid_noise_clock` 777, envelope attack from 0xaa
  8,052, baseline 39,731 (measured on the 8580 before M1b Task 6). Two chips at
  1,237 or less would give back the 400 (1,232.5 with the tap); the 6581's
  filter is about 38 of each chip's share.
- **Action:** when the 6502 lands, measure its per-sample worst. If it does not
  fit the headroom, one or more of:
  1. rewrite the combined-noise path (`sid_osc_run_combined`,
     `sid_osc_bulk_limit`, `sid_osc_bulk_commit`) as one tight loop with the
     per-call constants in registers; the transient (about 40.6k above) must
     shrink by about 27% to reach 1,237 for two chips;
  2. give the envelope's attack from 0xaa a fast path (8.1k of every gated
     first block);
  3. have a slot switch wait a block when the SID's block is heavy.
  Keep the output exact (golden hashes, the bulk-against-cycle fuzz).
