<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Put a .sid tune on the FM-1

SIDOU plays PSID `.sid` tunes from its store, the flash it keeps songs and settings in. A tune
goes there over MIDI, as a SysEx upload (the protocol's `PUT`,
[docs/reference/sysex.md](../reference/sysex.md)), from any SysEx tool. On the computer, the
only SIDOU software needed is the command that turns a tune into that upload.

What plays: PSID tunes, one or two SID chips, PAL or NTSC, up to 65,536 bytes a file (every
PSID fits). What does not: RSID tunes (`RSID: NOT YET`: they need a real C64's interrupts and
ROMs), MUS files (`MUS: NOT SUPPORTED`), and damaged files (`BROKEN .SID`, `TOO LONG`).
[docs/reference/psid.md](../reference/psid.md) has the details. Playing them from the panel
comes with the SIDS browser, M3's next step; this build stores them.

## 1. Make the .syx

In `host/`, with Rust installed ([BUILDING.md](../../BUILDING.md)):

```sh
cargo run --release -p sidou-sysex -- sid first.sid second.sid broken.sid --out syx
```

Each tune becomes one `.syx` in `syx/` (`syx/first.syx`, ...), one SysEx message for every
256 bytes of the tune and two more. Its name on the device comes from the tune's title:
uppercase, 16 characters at most, other characters shown as `?`; a second tune of the same
title gets `-2`. The command checks each tune with the same parser the FM-1 uses, prints its
title, author, songs and chips, and refuses what the FM-1 would refuse, by name:

```
first.sid -> syx/first.syx as NIGHT DRIVE: Night Drive by A. Composer, 3 songs, 1 chip; 17 messages
second.sid -> syx/second.syx as NIGHT DRIVE-2: Night Drive by A. Composer, 1 song, 2 chips; 11 messages
broken.sid: refused: RSID: NOT YET: it needs a real C64's interrupts, which SIDOU does not have
```

The command opens no MIDI port; a SysEx tool sends the file. A tune of the same name already
on the FM-1 is replaced.

## 2. Send it over USB

Connect the FM-1 by USB; it is the MIDI port **SIDOU FM-1**. In the SysEx tool, choose that
port for output, load the `.syx` and send it **with a pause of one second after each message**
(the FM-1 writes each message to flash before it takes the next; without the pause it refuses
the upload). A 4 KiB tune takes about 20 seconds.

- **macOS:** SysEx Librarian (Snoize): its preferences set the pause between messages.
- **Windows:** MIDI-OX: SysEx, Configure, the delay after each F7, in milliseconds.
- **Linux:** `amidi -p hw:X,0 -s first.syx -i 1000` (`amidi -l` lists the ports; `-i` is the
  pause in milliseconds).

The screen shows **RECEIVING**, then **SAVED** when the tune is on the store. A refusal shows
instead (`RSID: NOT YET`, `BROKEN .SID`, `STORE FULL`, ...), and nothing of the tune is
written. A tool that waits for the FM-1's reply to each message (its ACK) may go faster than a
second.

## 3. Or over the TRS MIDI IN

From a MIDI interface's OUT to the FM-1's TRS MIDI IN (the FM-1 has no TRS out, so there are
no replies): the same `.syx`, the same one second after each message, which the TRS path needs
whatever the tool. `--trs` prints that pacing and the time for the files:

```sh
cargo run --release -p sidou-sysex -- sid first.sid --out syx --trs
```

A refusal over TRS shows on the screen and the rest of that upload is ignored; send the file
again from its start.

## How many fit

The store holds about 190 KB of songs and tunes, and 48 entries in all (settings, maps, chip
profiles, songs and tunes together): about 30 tunes next to a few songs. When either runs out,
the upload is refused with **STORE FULL** and nothing is written; delete a tune or a song
first (the protocol's `DEL`).
