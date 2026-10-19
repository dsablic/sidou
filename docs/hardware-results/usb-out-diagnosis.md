<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# USB-MIDI out on the FM-1: diagnosis and fix (2026-10-17 to 2026-10-19)

A summary. The full record is on the `usbdbg` branch (never released), in
`docs/hardware-results/usb-out-diagnosis.md` there. It has every diagnostic
build, its SHA-256, the device results and the test procedure.

## The fault

On a real FM-1, SIDOU received USB-MIDI (notes, the update protocol's
requests) but sent almost nothing back
([first flash](2026-10-17-first-flash.md), [second flash](2026-10-18-second-flash.md)).
Enumeration and EP1 OUT worked. EP1 IN stopped after the first packet or two:
the identity reply to Felucca's installer, K1's CCs, the heartbeat. So
`fm1_install.py` could not reinstall over SIDOU. fm1-emu never showed it,
because it has no USB controller, and the test crate's host model answers
whatever the firmware asks.

## The cause: the compiler flags of the file that includes `usb.c`

The bisection worked from both sides. Felucca 1.6.1, built from source,
answered 10/10 handshakes. SIDOU with only USB started (no screen, audio, keys
or store) answered 0/10, even on Felucca's `usb.c`. These made no difference
on the device:

- the clocks: identical, by an emulator register trace of both start-ups;
- the memory map, both ways, and code run from SRAM;
- the main loop's `idle`;
- the start-up order and Felucca's reset-reason read and guards;
- the boot guard;
- the heartbeat;
- USB audio;
- the descriptors;
- the LCD DMA;
- the render load;
- where the ALNK DMA buffer sits.

The one difference left was the build. The same minimal build, compiled as
Felucca compiles its firmware (no `-mcpu`, `-Os`), answered 10/10. With
SIDOU's flags (`-mcpu=r3 -mfprev1 -O2`) it answered 0/10.

The codegen of `midi.c` (which includes `usb.c`) under both flag sets has no
exotic instructions on the USB path with `r3`:

- no hardware loops, and no paired or merged stores to the SFRs;
- the same `csync` after each CON1 write;
- the same start-of-frame clear.

The structural difference is inlining. At `-O2`, `sie_rd` and `sie_wr` are
inlined into `usb_poll`, so consecutive SIE accesses (INDEX then TXCSR1,
TXCSR1's read then its write, the count and address then TxPktRdy) run a few
cycles apart. At `-Os` each is a call. The likely mechanism, not proven: the
SIE sets its done bit (CON1 bit 15) before it has latched the access, so an
access that soon after the last one hits stale state.

Two more findings on the way:

- **EP1 IN's DMA buffer must be 8-byte aligned.** `usb.c` declares it
  `aligned(4)`. At 4 mod 8, TxPktRdy never clears and nothing is sent.
  Felucca's own layout happens to place it at 8.
- **SysEx replies split into short IN packets** (SIDOU's 16-byte cap from
  `dfa3415`) were no help. Felucca's 64-byte packets are what works.

## The fix (`SIDOU_USB_FIX`, on by default)

- **Felucca's `src/usb.c`, byte for byte but for two marked hooks.**
  SIDOU's fork (`usb/usb.c`) is gone. The hooks are inert in Felucca's own
  build and the loader's, and are proposed upstream:
  - SIDOU's USB strings;
  - every OUT event packet raw to SIDOU's parser.

  Everything else SIDOU needs is in `midi.c`, around the include
  ([VENDORED.md](../../platform/fm1/VENDORED.md)).
- **`midi.c` compiled with Felucca's flags** (`-mcpu=generic -mno-fprev1 -Os`
  appended to SIDOU's), and with `-fdata-sections`. The rest of SIDOU keeps
  `-mcpu=r3 -mfprev1 -O2`.
- **`usb.c`'s EP buffers aligned** by `app.ld`: EP1 IN's on 64 bytes, the
  other two on 8.
- **The second core held** first thing at boot (`fm1_cpu1_halt`), as
  Felucca does.
- **SIDOU's own TRS driver** (`usb/midi_uart.c`): every byte goes to SIDOU's
  parser in the main loop.
- **`usb_retry_quiet` in `midi.c`:** the retry without a host stays out of
  the main loop's way. A short probe (8,040 instructions in fm1-emu) runs
  first, and Felucca's own `usb_retry` follows only if the SIE answered.
  `usb.c`'s `usb_retry` alone stops the main loop for about 28 ms once a
  second: its 25 ms wait, then six SIE timeouts. emu-gate's latency tests
  caught it.

## On the device (2026-10-19)

- **FULL-FIX** (the fix with the `usbdbg` page, a heartbeat and a loader
  escape): 10/10 handshakes with a heartbeat every second. The installer's
  `--info` answers `FM-1_900`.
- **Reinstalls over USB with Felucca's `fm1_install.py`:**

  | From | To | Result |
  |---|---|---|
  | FULL-FIX | FULL-FIX-NC (the fix alone) | "done: the FM-1 runs FM-1_900" |
  | FULL-FIX-NC | stock V15 | "done: FM-1_015" |
  | V15 | FULL-FIX-NC | written; the installer reports "did not come back", because the port name differs, but the device runs it |

  After the last install: 10/10 handshakes, and K1 sent CC 74 (326 messages).

The build on this branch is FULL-FIX's structure, plus SIDOU's TRS driver and
the quiet retry. The `usbdbg` branch's FULL-FIX3 and FULL-FIX3-NC packages
are these builds, for a last check on the device.

## Upstream

Two reports for Felucca are drafted in [upstream-reports.md](../upstream-reports.md):

- the buffers' alignment;
- the SIE accesses' timing under `-O2 -mcpu=r3`.
