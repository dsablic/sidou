<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 2026-10-17: SIDOU's first flash on a real FM-1, and its recovery

SIDOU v0.2.0-m2-preview was the first SIDOU build to run on real hardware. It booted and
played. Its USB-MIDI output did not work, though, so it could not be updated over USB. The
unit was brought back to M-VAVE's official V15 through the chip's ROM boot mode, entirely
from a Mac. This page records what was seen, what was tried and what fixed it.

## Unit and builds

| | |
|---|---|
| Unit | M-VAVE FM-1 (JieLi WL82, flash ID 0x856014, chip key 0x980F) |
| Firmware before | ChoralRoot 0.24-beta2, 1-core (`FM-1_920`; USB 1209:0001, "ChoralRoot FM-1") |
| SIDOU package | v0.2.0-m2-preview, tag commit 2fd6a54, `sidou.fwsc` SHA-256 `4132aa0b1f5cdfde82e1c4ff410186f6fd45d253c4ddec33d08ef2b20857ee96` (the release asset; a local build gives the same bytes) |
| Installer | ChoralRoot's `tools/fm1_install.py` (repository head 13b0adb) |
| Host | macOS on Apple silicon |
| Firmware after | official M-VAVE V15 (`FM-1_015`, `FM-1.fwsc` SHA-256 `db1642b2…edb8a`) |

## The install

1. `fm1_install.py --info` identified ChoralRoot 0.24-beta2 and allowed an install over it.
   `fm1_install.py --backup .` saved ChoralRoot's user data (an FM6 patch bank) to a JSON file.
2. `fm1_install.py sidou.fwsc --yes`: step 1 (ChoralRoot staged the package's loader, which
   is Felucca's) and step 2 (the loader wrote the app area) both completed, and the write
   reached 100 %. The installer then timed out after 40 s waiting for the new firmware to
   answer its identity query. On USB, though, the unit came back as "SIDOU" (1209:0001): SIDOU
   was running.

## What SIDOU did on the hardware

**Worked:** boot, the title card, LIVE, the note keys and sound, knobs, the screen, and
USB-MIDI **input**. A note-on/off sent from the Mac played. The update protocol's upgrade
request (`F0 22 24 35 7F F7`) was received and acted on: the audio stopped (a click) and the
unit rebooted once its update session failed.

**Findings:**

1. **USB-MIDI output is (almost always) dead. Critical: it breaks the update path.**
   - The identity handshake (`F0 00 32 45 00 00 00 40 7F F7`) got no reply: on many
     attempts, with ChoralRoot's installer and with a raw rtmidi script (SysEx input
     enabled), after cold boots, warm reboots and USB replugs.
   - During the upgrade attempt, none of the update session's package read requests (cmd
     0x30) reached the host. So SIDOU could not run step 1 of any install, and could not be
     updated over USB.
   - Parameter output also failed: K1 sweeps (CC 74) and K4 preset steps (program changes)
     arrived 0 times.
   - Exactly once, right after the install and a replug, six CC 74 messages did arrive. That
     run is unexplained.
   - Static analysis found SIDOU's USB stack byte-identical to Felucca a53ecf4 apart from
     the documented changes. In an emulator with a USB model added, SIDOU answers the
     handshake and sends the CCs. So the cause is a runtime difference only SIDOU has on the
     hardware. The leading suspects:
     - the `idle` instruction at the end of the main loop, which Felucca, ChoralRoot and the
       loader never execute (the USB block DMAs IN packets while the core sleeps);
     - USB polled inside the audio interrupt, which Felucca defers.
2. **A faint tone never stops after notes.** After a note's release a very quiet sound
   remains. The emulator shows the same thing: a floor around -55 dBFS with the 6581 model,
   from the modelled oscillator / DAC leak.
3. **A playing song ignores MASTER.** It plays at full volume whatever the MASTER pot says.
   This is probably the GT2 player writing the SID's volume register, which MASTER also
   uses.
4. **The knob range is too small.** A full K1 sweep moves the cutoff only a little (see the
   UX-pass entry in https://github.com/dsablic/sidou/blob/v0.5.0/docs/TODO.md on knob ranges and curves).
5. **macOS caches USB-MIDI devices by USB ID.** The Felucca family (ChoralRoot, Felucca,
   SIDOU's development builds) all use 1209:0001, so after switching firmware CoreMIDI kept
   the old device record: the port was still called "Felucca". Deleting the device in
   **Audio MIDI Setup › Window › Show MIDI Studio** (and any stale "Felucca Update" entry)
   and replugging gives a fresh port. This was not the cause of finding 1: output still
   failed with a fresh port.

## Recovery

SIDOU v0.2.0-m2-preview has no way into an update mode or ROM boot from the panel. The 7D
soft key is not handled, there is no power-on gesture, and the boot guard needs two early
crashes, which couldn't be triggered. Its USB output was dead too, so the only way left was
the chip's own ROM boot mode, forced from outside.

1. **Into ROM boot with a Flipper Zero.**
   - [flipper-fm1-recovery](https://github.com/merthsoft/flipper-fm1-recovery)'s
     `fm1_boot_key.fap` was built with ufbt against the Flipper's own SDK (official firmware
     1.5.1, API 88.2; the app was developed on Momentum) and installed with `ufbt launch`.
   - Its pin check (Right) must run with **nothing** in the Flipper's USB port; with the
     FM-1 still plugged in it reports "pull check FAILED".
   - Then: FM-1 off, cable from the FM-1 to the Flipper, OK, FM-1 on. When the app says
     "Move FM1 cable to PC now", move the cable to the Mac.
   - The Mac shows `WL80UBOOT1.00`, USB 4C4A:8057. The unit stays there while powered; a
     power cycle starts the installed firmware again, unharmed.
2. **A Linux VM did not work.**
   - MvaveFM1Unbricker needs Linux or Windows, so a Debian VM in UTM was tried first.
   - Through UTM's USB 3.0 controller the device arrived and Linux's own SCSI probe
     succeeded, but the unbricker's first SCSI command failed (host status 07) and the ROM
     dropped out of boot mode.
   - Through the USB 2.0 controller the full-speed device never reached the guest.
3. **Writing V15 from macOS.**
   - [tools/fm1-unbrick-mac](../../tools/fm1-unbrick-mac/README.md) gives the unmodified
     unbricker a USB Bulk-Only-Transport back end over libusb.
   - `sudo … fm1_unbrick_mac.py info` reported chip key 0x980F and flash ID 0x856014.
   - `… restore FM-1.fwsc` then:
     - read the whole flash twice and compared the copies;
     - wrote V15 to 0x4000..0x92FFF;
     - read it back byte-for-byte identical.
   - After a power cycle the unit came up as stock V15 (USB 4C4A:C755, "FM-1").

The flash contents from before the restore are in the unbricker's backup folder on the
computer that ran it; they are not part of this repository.

## What changes because of this

- **The USB fix (in progress):**
  - no `idle` while USB is configured;
  - no USB polling nested in the audio interrupt;
  - EP1 IN packets of at most 16 bytes;
  - a power-on gesture (OCT- held) into ROM boot, so a broken update path no longer needs
    outside hardware to recover.
- **A release gate:** before any release, docs/hardware-test.md must show all of these on a
  real unit:
  - `fm1_install.py --info` answering SIDOU;
  - a reinstall over SIDOU completing;
  - knob CCs and program changes arriving at the host.
- **Findings 2 and 3** are being fixed in the emulator: the release tail goes to true
  silence unless the bleed character is raised, and MASTER becomes a digital gain after the
  chips.
- **Recovery is known to work from a Mac** with a Flipper Zero (or the FM-1 Transporter) and
  tools/fm1-unbrick-mac. docs/hardware-test.md section 0 lists it.
