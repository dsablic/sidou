<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Put a .sid tune on the FM-1, and play it

SIDOU plays PSID `.sid` tunes from its store, the flash it keeps songs and settings in. A tune
goes there over MIDI, as a SysEx upload (the protocol's `PUT`,
[docs/reference/sysex.md](../reference/sysex.md)), from any SysEx tool. On the computer, the
only SIDOU software needed is the command that turns a tune into that upload.

What plays: PSID tunes, one or two SID chips, PAL or NTSC, up to 65,536 bytes a file (every
PSID fits). What does not: RSID tunes (`RSID: NOT YET`: they need a real C64's interrupts and
ROMs), MUS files (`MUS: NOT SUPPORTED`), and damaged files (`BROKEN .SID`, `TOO LONG`).
[docs/reference/psid.md](../reference/psid.md) has the details. Once a tune is on the store, the
menu's **SIDS** plays it ([4. Play it](#4-play-it)).

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

## The starter pack

The repository's `assets/starter/` has eleven tunes, openly licensed, for the store: SIDOU's
five songs and six of GoatTracker's example songs by Cadaver ([the licences](../../LICENSING.md#the-starter-pack)).
One command makes them one upload:

```sh
cargo run --release -p sidou-sysex -- sid --pack ../assets/starter --out syx --trs
```

`syx/pack.syx` holds all eleven, each under its title (`CHROME COMET`, `DOJO`, `IRON RITE`, ...;
a title longer than 16 characters is cut, as `ON A SANCTION FR`). Send it as in 2 or 3: with a
second after each message it is 143 messages, a little over two and a half minutes, and the
screen shows **RECEIVING** and **SAVED** for each tune in turn. The pack is about 29 KB of the
store's 190 KB and 11 of its 48 entries, so about 20 more tunes fit next to it. A tune of
the pack that is already on the FM-1 is replaced by the same tune; to send one alone, name it:
`sidou-sysex sid ../assets/starter/dojo.sid`. A directory on the command line stands for its `.sid`
files, and `--pack` writes any set of tunes as one file, not only this one.

## How many fit

The store holds about 190 KB of songs and tunes, and 48 entries in all (settings, maps, chip
profiles, songs and tunes together): about 30 tunes next to a few songs. When either runs out,
the upload is refused with **STORE FULL** and nothing is written; delete a tune or a song
first (the protocol's `DEL`).

## 4. Play it

On the FM-1, press **HOME** for the menu, turn **SELECT** to **SIDS** and turn **ALGORITHM**
clockwise (or press SAVE). The browser lists the store's tunes by their names, eight a page;
**SELECT** moves through them and **ALGORITHM** clockwise (or SAVE) plays the highlighted one and
opens **NOW PLAYING**: the tune's title, author and year, which of its songs plays (**SONG 1/3**),
the time, PAL or NTSC, its chips, and its three voices.

- **ALGORITHM** in NOW PLAYING steps to the next or the previous song, from its start.
- **PLAY** stops the tune, and plays it again from the start; **EDIT held + PLAY** pauses it and
  goes on.
- **GLO held + the first three keys** mutes a voice, **+ the next three** solos one, as for a song.
- **HOME** goes back to the browser and on to the menu with the tune still playing; the keys
  play the second chip meanwhile (a tune for two chips silences them). HOME out of the browser
  with the tune stopped ends it, and your song comes back.
- From a sequencer, MIDI start plays the song from its start, stop holds it, continue goes on.

A tune takes the memory your song uses. If the song has changes that are not saved, the
browser asks first, **SONG NOT SAVED**: SAVE saves the song (an unnamed one asks for its name),
ALGORITHM again throws the changes away, HOME keeps both. When the tune ends, your song comes
back as it was last saved.

**SETTINGS → SID MODEL**: **AUTO** (the default) plays each tune on the chip its file names (a
6581 or an 8580), **KEYS** on the chip the keys play, with its profile and character.
