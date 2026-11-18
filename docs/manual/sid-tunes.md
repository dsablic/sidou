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
menu's **SIDS** plays it ([4. Play it](#4-play-it)), and **REC** takes a voice's sound into LAB
([5. Lift a sound](#5-lift-a-sound)).

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

If the tool reports a refusal part way (the screen shows BUSY, as the store is tidying its
flash), wait about 20 seconds and send the whole file again; the Songs page on the demo site
does this by itself.

**Stop the song first.** While a song or a tune plays on the FM-1, or a computer records its
USB audio, the FM-1 writes nothing to flash, so the music never dips: the screen says
**UPLOAD WAITS FOR QUIET** and each message waits until the song stops (a minute at most,
then the upload is refused). A tool that sends on a timer does not wait with it and is refused
BUSY; stop the song and send again. The Songs page waits, and says so. With only the keys
sounding the upload goes on, the sound dipping briefly four times a second.

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
- **K1** bends the tune's filter cutoff (32 a detent, up to 1024 either way) and **K2** its
  resonance (up to 15 either way); **K3** picks a voice and **K4** gives it another waveform:
  **TRI**, **SAW**, **PUL** or **NOI**, or **TUNE'S**, its own. The tune keeps its notes, its
  gate and its sync and ring. Each turn shows the value large (**TWEAK / CUT +96**), and
  **TWEAKED** under the voices says what is bent. Back at 0 a knob gives the tune its own value
  at once; leaving NOW PLAYING, another song or a stop gives everything back. EDIT held with a
  knob is still the effect slot's.
- **PLAY** stops the tune, and plays it again from the start; **EDIT held + PLAY** pauses it and
  goes on.
- **GLO held + the first three keys** mutes a voice, **+ the next three** solos one, as for a song.
  They are the tune's: it starts with every voice on, and your song comes back with its own.
- **REC** asks which voice to lift, and note key 1, 2 or 3 takes its sound into LAB
  ([5. Lift a sound](#5-lift-a-sound)).
- **HOME** goes back to the browser and on to the menu with the tune still playing; the keys
  play the second chip meanwhile (a tune for two chips silences them). HOME out of the browser
  with the tune stopped ends it, and your song comes back.
- From a sequencer, MIDI start plays the song from its start, stop holds it, continue goes on.

A tune takes the memory your song uses. If the song has changes that are not saved (its
patterns, tables, instruments or settings), the browser asks first, as SONG's LOAD does:
**DISCARD UNSAVED SONG?** over **SAVE AGAIN**. SAVE again throws the changes away and plays the
tune; HOME goes back and keeps the song (to keep its changes, save it from SONG, then choose the
tune). The knobs do nothing on the question. When the tune ends, your song comes back as it was last saved. The sound on the keys is not
the song's memory: a sound changed on LIVE or in LAB asks nothing, and stays as it is when the
song comes back.

**SETTINGS → SID MODEL**: **AUTO** (the default) plays each tune on the chip its file names (a
6581 or an 8580), **KEYS** on the chip the keys play, with its profile and character.

## 5. Lift a sound

A tune's sound can come with you. In NOW PLAYING, while the tune plays, press **REC**: the
screen asks **LIFT WHICH VOICE? 1 2 3** and note keys 1, 2 and 3 light up. Watch the voice bars
for the one that plays the part you want and press its key. SIDOU keeps the last second or so of
the tune's registers, so it takes that voice's most recent note at once; **LAB** opens with it,
named **LIFTED V2** (for voice 2). If the voice has played nothing lately, the screen says
**WAITING FOR V2** and takes its next note. REC again or HOME cancels the question or the wait.

In LAB the lifted sound is a sound like any other: the keys play it, every page shapes it, and
**SAVE** keeps it as a USER sound named LIFTED V2 (**EDIT + SAVE** gives it a name of your own).
A USER sound you were playing before stays as it was. **HOME** goes back to the tune, still
playing. To put the sound in a song, REC in LAB sends it to the song once the tune is over
([LAB's song bridge](lab.md#send-it-to-the-song-and-pull-one-back)).

What LAB takes from the voice, read from its registers as the tune writes them:

- **Waveform and envelope**: the note's waveform, attack, decay, sustain and release.
- **Pulse width**: where it starts; a width that swings up and down becomes LAB's PWM, at the
  depth and rate it swung.
- **Filter**: only if the voice goes through the filter; the cutoff, mode and resonance, and a
  cutoff that swings becomes the filter LFO.
- **Vibrato**: a pitch that swings around the note becomes LAB's vibrato, its rate, depth and
  delay.
- **Not held**: a sweep that only goes one way (the toast names it: `LIFTED V2 ~SWEEP`), notes
  that change within the sound (`~DRUM`), what the tune does before the note starts (its hard
  restart), the other voices that sync or ring-modulate it, the volume.

**Digis** (samples played through the registers) cannot be lifted: `DIGI: CAN'T LIFT`. A tune
for two chips silences the keys while it plays, so LAB plays the lifted sound once the tune
stops.

## 6. Convert a tune into a song

The site's [Library page](library.md#7-convert-a-sid-tune-into-a-song) turns a PSID tune into a
GoatTracker song the FM-1 plays, the editor opens and the song views edit: not a copy of the
tune's register writes, but rows, notes and instruments, as a tracker musician would have written
them. Everything runs in the browser; nothing is uploaded.

How it works:

- **The tune is run** by SIDOU's own 6502 and PSID environment (the firmware's C, compiled to
  WebAssembly), for the chosen sub-tune and length, and every SID write is recorded with its
  cycle, for each chip of a 2SID tune.
- **Notes** are the frames where a voice's gate rises; a note's pitch is the first one it holds
  (an attack's short jump, like a drum's falling pitch or an octave on the first frame, is skipped),
  read from its frequency against GoatTracker's note table.
- **The row grid** is the coarsest one that holds nine onsets in ten: one tempo, or a pair that
  alternates (swing, written as funktempo with the pair in the speed table). A tune whose grid
  changes is cut where it changes, and SET TEMPO goes on the row where it does.
- **Instruments**: each note's attack and decay, sustain and release, its first eight frames of
  waveform and pitch (noise at an absolute pitch, the rest relative to the note), an arpeggio
  that repeats after them as a looping wave table, and its pulse width and its sweep up and down. Notes with the
  same shape share an instrument; each voice keeps its six most used, and a rarer note takes the
  closest. The voice that drives the filter most gets the filter's mode, resonance, routing and
  cutoff in its instruments' filter table.
- **Patterns**: each voice's rows are cut into patterns of 16, 32 or 64 rows (whichever makes the
  song smallest), and a pattern heard again, or again a few semitones away, is played again with a
  repeat or a transpose in the orderlist. A pitch the note moves to and holds becomes a tie note;
  a gate that closes long after the attack becomes a key-off.
- **Six voices**: a tune that uses its second chip becomes a GoatTracker Stereo song. The FM-1
  plays its first three voices (chip A) and keeps the others, as for any GoatTracker Stereo song.

What it simplifies, listed on the page for each tune: hard restarts become GoatTracker's own (the
gate off two frames before a note, ADSR `$0F00`), digi samples (fast writes to the volume
register) are dropped, vibrato is dropped (the note plays at its centre pitch), notes off the grid
move to the nearest row, and ring and sync detail, filter sweeps and pulse sweeps past their first
turn are simplified. RSID tunes are not converted
yet: they need a whole C64.

How close it comes: the page plays the song in the FM-1's own player and counts, for each voice,
the original's notes it plays at the same pitch and within two frames of the same time. On the
starter pack's eleven tunes (made with GoatTracker), 60 seconds each, 6,183 of the 6,214 notes come
back at their pitch and time (no voice under 98 %), and the tempo is the one their source songs set.

**The tune's sounds.** The instruments the conversion found are offered as LAB sounds too
([the Library page](library.md#the-tunes-sounds)): the envelope as LAB's envelope; the wave table
as LAB's table (waves and pitch steps one frame each, a loop kept, 16 steps at most, so a longer
table is cut); a pulse width and its sweep as the pulse width and a triangle PWM; and the filter's
mode, cutoff and resonance, on all three voices (LAB's filter takes all three or none). Vibrato,
table delays and the filter's moves in the tune are not carried over, and each sound says what it
simplified. Listen to one, keep it in the browser, edit it as text, send it to the FM-1 or share
its link.

**Copyright.** A converted song, and the sounds taken from the tune, are still the original's music: it keeps the original's copyright,
and the page carries the tune's author and release line into the song. Share a conversion only as
the original's licence allows.
