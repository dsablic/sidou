<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The editor: a song in the browser

The site's [editor](https://dsablic.github.io/sidou/editor.html) opens an FM-1 song, from a file
or from the FM-1 itself, and edits its instruments, tables, patterns and orderlists, plays them
with SIDOU's own player, and saves the song back to the FM-1.

The editing is done by SIDOU's own song code, the same C the FM-1 runs, compiled to WebAssembly.
Each change is one the FM-1's own song views can make, made the same way, so:

- an edit the FM-1 would refuse is refused here, with its words: **SONG FULL** (no room left in
  the song's memory, or the song as the FM-1 saves it would be over 64 KiB), **INVALID EDIT** (a
  value out of range, a transpose after a repeat, a pattern an orderlist still plays);
- parameter locks (set on the FM-1's STEP view) move with their rows when rows are inserted or
  deleted, and go with a row that is deleted or cut off;
- a song the editor saves is one the FM-1 loads.

Editing a file works in any browser. Reaching the FM-1 needs **Chrome or Edge** (Web MIDI with
SysEx), as the [Library page](library.md) does. Nothing is uploaded anywhere. Run locally,
`tools/web-demo` serves it at `http://localhost:8642/editor.html`
([BUILDING.md](../../BUILDING.md#browser-demo-toolsweb-demo)).

## 1. Open a song

- **A file:** drop a `.sng` on the page or press to choose one.
- **From the FM-1:** plug it in (running SIDOU), press **Connect** and allow MIDI with SysEx. Then
  **Open the FM-1's current song** (what it has loaded now, with any changes not yet saved there)
  or pick a saved song and **Open**. Nothing is sent to any port before Connect, and then only to
  the pair shown.
- **From the Library:** **Edit** next to a song on the FM-1, or under a song opened there.

A song the FM-1 could not load is not opened, and the page says why with the FM-1's message
(`SNG: TOO BIG (70K OF 64K)`, `SNG: MULTISPEED 8`, ...).

At the top: the song's name, where it came from, and its size as the FM-1 saves it, of the 65,536
bytes a song can be. **Undo** and **Redo** (Ctrl+Z, Ctrl+Shift+Z; Cmd on a Mac) step through
every change since the song was opened. The line under them says what the last change was, or
why it was refused.

## 2. Instruments

As the FM-1's INSTR view shows an instrument: its nine parameter bytes in hex (ATTACK/DECAY,
SUSTAIN/RELEASE, the wave, pulse and filter table pointers, VIBRATO, VIB DELAY, HR/GATE,
1ST WAVE) and its name. Type a new value and press Enter (or leave the field). The table
pointers count rows from 01, as TABLE numbers them; 00 is none. **Download .ins** saves the
instrument as GoatTracker's instrument file (GTI5) with its table rows, as the Library does.

The preview's keys play the instrument shown here.

## 3. Tables

As the FM-1's TABLE view: WAVE, PULSE, FILTER and SPEED, each row's left and right byte in hex
and, beside them, what the row does in TABLE's own words (`WAVE 41 C-4`, `SLIDE 20 10`,
`JUMP 01`, `CUTOFF 00 80`). The rows in use and a few more are shown; **Show all 255 rows** shows
the rest.

## 4. Patterns

A pattern's rows, as TRACKER shows them: the note (`C-4`, `C#4`; `---` none, `===` key off,
`+++` key on), the instrument (`..` none), the command and its parameter, and the row's parameter
locks. Pick the pattern above the grid; its length is beside it.

With a note selected, the computer keyboard is a piano, as in GoatTracker: Z S X D C V G B H N J M
from the preview's octave, Q 2 W 3 E R 5 T 6 Y 7 U an octave up. A key writes its note with the
instrument shown on Instruments, plays it, and moves to the next row. Delete or Backspace
clears the note, `=` is key off. You can also type a note and press Enter.

For the selected row (click its number, or any of its fields):

- **Insert (keep length)** and **Delete (keep length)** are TRACKER's INS and DEL: an empty row
  goes in here and the last row drops off, or this row goes and an empty one comes in at the end.
- **Insert row** and **Delete row** make the pattern a row longer or shorter.

**Set length** gives the pattern 1 to 128 rows (rows cut off take their locks with them).
**New pattern** makes an empty one, of the length in the box, under the lowest free number; put
it in an orderlist to hear it in the song. **Remove** takes out a pattern no orderlist plays.

## 5. Orderlists

Each voice's orderlist of the subtune picked, as the FM-1's ORDER view: `04` plays pattern 04,
`R3` plays the next pattern 3 times (`R0` 16 times), `+2` or `-3` transposes what follows, and
the last row (`RST`) is where the voice goes on after its last entry. Edit an entry in place, or
choose the voice, the position and an entry and press **Insert** (it goes in before that
position) or **Delete**. An orderlist must end with a pattern, and a transpose cannot follow a
repeat: the FM-1's rules, so the page refuses what breaks them.

A GoatTracker Stereo song (six voices, two chips) is read and kept as it is; its voices 4-6 are
shown but not edited here, and the preview cannot play it.

## 6. Preview

SIDOU's own player and SID, in the page:

- **The keys** (on screen, or the computer keyboard when no field is selected) play the instrument
  shown on Instruments, up to three notes at once; **Oct −** and **Oct +** move them.
- **Play pattern** plays the selected pattern over and over on the voice picked, the others
  silent.
- **Play song** plays the song from the orderlist position in **from position** (0: its start).
- **Stop** stops it, the notes' releases ringing on as on the FM-1.

It plays the song's chip setup (its 6581 or 8580 profile, the character's age, DAC, thump and
bleed controls, the expert settings and the envelope mode) and its effect slots, in stereo, as the
FM-1's output does. Not heard here: the thump on the output, the second chip (and CHIP SPREAD),
MASTER, and the character's glide (a character lock changes it at once, where the FM-1 glides).
Playing uses the song as it is when you press Play; press it again after more changes. Nothing
sounds until you press Play or a key.

## 7. Save

- **Save to the FM-1** stores the song in the FM-1's store under the name in the box (from where
  it came, or the song's name). A song of that name already there is replaced only after you
  confirm; an identical one is left alone. On the FM-1, load it from SONG, LOAD.
- **Load now** sends the song to the FM-1 as its current song, at once, in place of the song it
  has loaded (it is not saved there under a name until you SAVE AS on the FM-1). The FM-1 refuses
  this while its own song has changes it has not saved, and while its current song is one that
  came from a computer and has no name yet: its screen says **SAVE AS FIRST**. SAVE AS on the FM-1
  (or load another song there), then press Load now again.
- **Download .sng** saves the song as the FM-1 saves it, SIDOU's settings kept in its trailer;
  **Plain GoatTracker .sng** leaves them out, with the Library's warning when GoatTracker would
  read it with another channel count ([library.md](library.md#4-export-a-song)).

The FM-1 writes nothing while a song plays on it (or a computer records its USB audio): the page
says it is waiting until you stop the song.

## What it does not do (yet)

Instruments are not deleted or reordered and patterns are not renumbered (the FM-1 does neither);
a song's name and author, its subtunes and its SIDOU settings (chip setup, effect slots, locks)
are edited on the FM-1, not here.
