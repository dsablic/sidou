<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The Library page: the FM-1's files from a browser

The site's [Library](https://dsablic.github.io/sidou/library.html) page looks after everything
in the FM-1's store, the flash it keeps songs, `.sid` tunes and instruments in: it lists the
files, downloads them, sends your own, renames and deletes them, and backs the whole store up to
one `.zip` and restores it. It also exports any song, from the FM-1 or from a file on your
computer, for GoatTracker 2, and each of its instruments as a GoatTracker `.ins`.

It talks to the FM-1 over USB-MIDI with SIDOU's SysEx protocol
([docs/reference/sysex.md](../reference/sysex.md)), which needs **Chrome or Edge** (Web MIDI with
SysEx; Safari has none, and Firefox is not supported). Exporting a song from a file works in any
browser. The songs are read and written by SIDOU's own song code, the same C the FM-1 runs,
compiled to WebAssembly, so a song the page accepts is one the FM-1 loads, and a song it refuses
is refused with the message the FM-1's screen would show (`SNG: TOO BIG (70K OF 64K)`,
`SNG: MULTISPEED 8`, ...). Nothing is uploaded anywhere: the page works in your browser.

Run locally, `tools/web-demo` serves it at `http://localhost:8642/library.html`
([BUILDING.md](../../BUILDING.md#browser-demo-toolsweb-demo)).

## 1. Connect

Plug in the FM-1 with its USB cable, running SIDOU (not in the updater), and press **Connect**.
The browser asks to use your MIDI devices with SysEx: allow it. The page picks the ports named
`SIDOU FM-1`; if it finds none, pick the FM-1's output and input and press **Check this pair**.
Nothing is sent to any port before Connect, and then only to the pair shown.

The line under the buttons names the firmware and says how many files the store holds and how
much room is free. Free space is a guide: the FM-1 says when a file does not fit.

## 2. The files on the FM-1

Each file has its name (as the FM-1's menus show it), its type (song, `.sid` tune or
instrument; a lifted sound is an instrument), its size and its CRC-32 (the same number `crc32` or
zlib gives for the downloaded file). For each:

- **Download** saves it: a song as `.sng`, a tune as `.sid`, an instrument as `.ins`.
- **Rename** asks for the new name (1 to 16 characters: A-Z, 0-9, space and most punctuation).
  The page reads the file, sends it under the new name and deletes the old one only once the new
  one is stored, so a failure on the way leaves the old file as it was. A name the FM-1 already
  has is replaced only after you confirm.
- **Delete** removes it, after you confirm. Back it up first if you may want it again.
- **Open** (songs) reads the song into the export panel below.

**Send files…** picks files from your computer: `.sng` songs, `.sid` tunes and GoatTracker
`.ins` instruments (the page tells them apart by their contents). It asks for each one's name on
the FM-1, from the file's name (uppercase, characters it cannot hold shown as `?`). A song the
FM-1 could not load is not sent, and the page says why. When the FM-1 already has a file of that
name, an identical one is left alone and a different one is replaced only after you confirm. A
sent song is loaded on the FM-1 from the menu: **SONG, LOAD**. A sent tune plays from **SIDS**
([sid-tunes.md](sid-tunes.md#4-play-it)).

While a song (or a tune) plays on the FM-1, or a computer records its USB audio, the FM-1 writes
nothing, so the music never dips: the page says it is waiting for the FM-1 to be quiet until you
stop the song. When the store is tidying itself the page waits and tries again on its own.

## 3. Back up and restore

**Back up all** reads every file and saves one `.zip`, named for the day
(`fm1-backup-2026-10-30.zip`). It holds each file under its name (`IRON RITE.sng`,
`SANCTION.sid`) and a `sidou-backup.json` that keeps the FM-1's exact names and types, so a name
with a character your computer's files cannot have comes back as it was.

**Restore a backup…** sends a backup's files back. Files the FM-1 holds identically are left
alone; when some are there with different content the page asks once whether to replace them
(Cancel keeps the FM-1's). A restore deletes nothing. A `.zip` you made yourself works too: its
`.sng`, `.sid` and `.ins` files are named from their file names, and anything else is left out
and listed.

The FM-1's private files are never listed by the FM-1, so the page never reads, backs up or
touches them: a song or an instrument it is importing over USB (`MIDI SONG`,
`MIDI INSTRUMENT`), and the copy of an imported song it keeps until you SAVE AS.

## 4. Export a song

Drop a `.sng` on the page, choose one, press **Open** next to a song on the FM-1, or **Open the
FM-1's current song** (what it has loaded now, with the changes you have not saved). The panel
shows the song's name, author, format and contents, and three ways to take it away:

- **Download .sng**: the song as the FM-1 saves it, SIDOU's settings (the chips' setup, the
  effect slots, parameter locks, MIDI, SCENE and the rest) kept in a trailer that GoatTracker 2
  skips over. This is the one to keep, and to send back to an FM-1.
- **Plain GoatTracker .sng**: the song without SIDOU's trailer, for tools that refuse anything
  after the song. GoatTracker reads a plain file's channel count from the file itself, and a few
  songs read as the other count (a 3-channel song as a 6-channel GoatTracker Stereo one, or the
  other way): only SIDOU's trailer tells them apart. The page checks the plain file as GoatTracker
  would read it and warns before saving such a file (`SNG: READS BACK AS STEREO`).
- **Send to the FM-1…**: the song, with its trailer, under a name you give.

Each instrument has a **GTI5** button: GoatTracker's instrument file (`.ins`, the GTI5 format),
with the rows of its wavetable, pulse, filter and speed tables, from the row the instrument starts
at. An instrument whose tables reach back before that row, or into a table it has no start in,
cannot be cut out on its own: its button says it cannot be exported. The FM-1's own export (SysEx
GET of an instrument) uses the same code.

6-channel GoatTracker Stereo songs are read and kept as they are.
