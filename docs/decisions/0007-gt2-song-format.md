<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0007: GoatTracker 2's `.sng` is SIDOU's song file (M2c Task 1)

**Status:** accepted, 2026-10-16.

<!-- REUSE-IgnoreStart -->

## Context

Design doc section 5 planned a native song format "close to GoatTracker 2" (a `SIDO`
header and tagged chunks) with a GT2 importer beside it (section 6). On 2026-10-15 the user
decided that SIDOU reads and writes GoatTracker 2's `.sng` itself instead (M2 overview,
"The design changes, and why"):

- GT2 is the most widely used editable C64 song format, and its files open in GoatTracker
  on every desktop: a song begun on the FM-1 can be finished in GoatTracker and come back.
- A second format "close to GT2" would need its own importer, exporter, migrations and
  fixtures, for no musical gain.

SIDOU's own extras (parameter locks, the chips' setups, the effect slots, the playroutine
options GoatTracker keeps outside the file) go in a **trailer** after the GT2 data, which
GoatTracker ignores. The trailer keeps section 5's rules: chunked, versioned, readers skip
what they do not know.

**Sources read** (fetched 2026-10-16, with a generic User-Agent):

| What | Where | SHA-256 |
|---|---|---|
| GoatTracker 2.77 (`readme.txt`, `src/`) | <https://sourceforge.net/projects/goattracker2/files/GoatTracker%202/2.77/GoatTracker_2.77.zip> | `96c2bd6a6ab3aca2f5bb18b1c764ac6ea69ac245cae14002a72cd87c554561ef` |
| GoatTracker 2.76 | `.../GoatTracker%202/2.76/GoatTracker_2.76.zip` | `c1b6b159ec0d37ae68599ac83be8934a71cd543e480eb5225f844b62151cea34` |
| GoatTracker Stereo 2.77 | `.../GoatTracker%202%20Stereo/2.77/GoatTracker_2.77_Stereo.zip` | `5ecfd091e49662732d9e3706017581a4b0d04d89637cfed54c5b8b129a7831c2` |
| GoatTracker 2 Qt fork | <https://codeberg.org/Ranzbak/goattracker2-Qt> at `6c45fafd50d8a25ec720c16977a2a8b4d12a71ef` (2026-06-16), `src/gsong.c` | |
| ChiptuneSAK | <https://github.com/c64cryptoboy/ChiptuneSAK> at `8759135b5c9c26dc9da155b006c575243ba5304a`, `chiptunesak/goat_tracker.py` | |

GoatTracker is © Lasse Öörni and its contributors (`readme.txt`'s header), GPL-2.0-or-later.
2.76's and 2.77's `gsong.c` and `player.s` are byte-identical. Line numbers below are 2.77's
`src/gsong.c` unless another file is named.

## What a `.sng` holds

Readme section 6.1, checked against `loadsong` and `savesong` in `gsong.c`;
`docs/reference/song-format.md` has the layout byte for byte. In order:

1. **Header:** the ident `GTS5`; the song name, author and copyright, 32 bytes each,
   zero-padded; the subtune count (1-32).
2. **Orderlists:** for each subtune, for channels 1, 2, 3: a length byte n (the entries
   **plus the `$FF` end mark**, not counting the restart byte), then n + 1 bytes: the
   entries, `$FF` (RST), the restart position. Entries: `$00-$CF` a pattern; `$D0-$DF`
   repeat the next pattern (`$D0` + k - 1 plays it k times, k 1-16; the editor shows
   R1-RF and R0 for 16); `$E0-$EF` transpose down (the value minus `$F0`: `$EF` is -1,
   `$E1` -15, `$E0` -16); `$F0-$FE` transpose up 0 to 14.
3. **Instruments:** a count (0-63), then 25 bytes each, from instrument 1: AD, SR, the
   wave, pulse and filter table pointers, the vibrato parameter (a speedtable pointer), the
   vibrato delay, the gate-off timer (bit `$80` no hard restart, `$40` no gate-off), the
   first-frame waveform (`$00` unchanged, `$FE` gate off, `$FF` gate on, else a waveform),
   a 16-byte name.
4. **Tables:** for wave, pulse, filter and speed: a row count (0-255), the left bytes, the
   right bytes.
5. **Patterns:** a count (up to 208), then per pattern a row count (**including** the end
   row) and 4 bytes a row: note (`$60-$BC` C-0 to G#7, `$BD` rest, `$BE` key off, `$BF` key
   on, `$FF` end), instrument (`$00-$3F`), command (`$0-$F`), data.

Limits (`gcommon.h`): 32 subtunes, 254 orderlist entries, 63 instruments, 255 rows a
table, 208 patterns, 128 rows a pattern (plus the end row).

## What is not in the file

GoatTracker keeps the playroutine's options in its configuration (`goattrk2.cfg`) or takes
them on `gt2reloc`'s command line (readme 4.4 and 5.1, `gt2reloc.c`'s `main`):

| Option | `gt2reloc` | Default | Changes the music? |
|---|---|---|---|
| Hard restart ADSR | `-Axxxx` | `$0F00` | yes |
| Buffered SID writes | `-B` | off | write order only |
| Zero-page ghost registers | `-C` | off | no (implies buffered) |
| Sound-effect support | `-D` | off | no (implies buffered) |
| Volume-change support | `-E` | off | no |
| Custom SID clock | `-Fn` | 0 (PAL/NTSC's) | the frequency table |
| Pitch of A-4 | `-Gf` | 0 (the built-in table) | the frequency table |
| Author info, timing mark | `-H` | off | no |
| Optimisations | `-I` | on | no (code size) |
| Full buffering | `-J` | off | write timing |
| SID address | `-Lxxxx` | `$D400` | no |
| NTSC | `-N` / `-P` | PAL | the frequency table |
| Pulse optimisation (skipping) | `-O` | on | yes |
| Realtime-effect optimisation | `-R` | on | yes |
| Multispeed | `-Sn` | 1 (0 is 25 Hz) | yes |
| Fine vibrato | `-V` | on (only below multispeed 2) | yes |
| Player address | `-Wxx` | `$1000` | no |
| Zero page | `-Zxx` | `$FC` | no |
| SID model | editor only | 6581 | the sound |

SIDOU stores all of them in the trailer's `OPTS` chunk, so a song plays the same in SIDOU
and GoatTracker. A file without `OPTS` plays with the defaults above. **The device plays
multispeed 1-4** (M2c's budget: at 8 or more two frames can fall in one audio block); a
song asking for more is refused there with `SNG: MULTISPEED 8 (MAX 4)` and plays on the
host.

A note for the harness (Task 3): `gt2reloc.c` 2.77's `case 'I':` has no `break`, so `-I`
falls through into `-J` and also sets or clears full buffering. The harness never passes
`-I`.

## Versions

- **SIDOU writes `GTS5`**, as `savesong` does.
- **SIDOU reads `GTS2`, `GTS3`, `GTS4` and `GTS5`** and converts the older ones on load
  exactly as `loadsong` does (lines 248-327 and 816-845 of `gsong.c`):
  - `GTS2` (three tables, the vibrato parameter and delay in the other order, old-style
    command data): every vibrato, portamento, toneportamento and funktempo value becomes a
    speedtable entry through `makespeedtable` (`gtable.c`), reusing an equal entry if one
    exists; fine vibrato is GoatTracker's default (on). About forty lines: small enough
    to carry;
  - `GTS2` and `GTS3` (`ident[3] < '4'`): pulse modulation speeds (pulse table rows below
    `$80`) are doubled and clamped to a signed byte;
  - `GTS2`-`GTS4` (`ident[3] < '5'`): a first-frame waveform of `$80` or more sets the
    gate-off timer's `$80` (no hard restart) and loses its top bit; a first-frame waveform
    of 0 sets `$40` (legato).
- `GTS!` (GoatTracker 1) is refused (`SNG: GT1 FORMAT`): its conversion is several hundred
  lines of guesswork about instruments, and GoatTracker 2 itself converts it.
- **Older idents are converted on load**, so saving a song read as `GTS2`-`GTS4` writes a
  different, `GTS5`, file. That is expected, not a round-trip failure.
- **GoatTracker Stereo** writes the same ident with **six** orderlists a subtune. SIDOU
  detects it as GoatTracker Stereo's `determinechannels` does (below), keeps channels 4-6
  in the model and in the file it writes, plays channels 1-3 on chip A, and shows
  `STEREO: CHIP B TRACKS IN M5`.

## Why the trailer works

GoatTracker's loader reads each section's count and data in order and closes the file
without looking at what follows. `loadsong`, 2.77 (identical in 2.76), lines 237-245 and
807:

```c
      // Read patterns
      amount = fread8(handle);
      for (c = 0; c < amount; c++)
      {
        length = fread8(handle) * 4;
        fread(pattern[c], length, 1, handle);
      }
      countpatternlengths();
      songchange();
    }
    ...
    fclose(handle);
```

Nothing checks for the end of the file or for trailing bytes. `savesong` writes the GT2
sections and nothing else, so a song saved by GoatTracker has no trailer (round-trip rule 2).

**The tools, checked by reading each one's loader:**

| Tool | Trailing bytes | Note |
|---|---|---|
| GoatTracker 2.77 | ignored (`loadsong` above) | |
| GoatTracker 2.76 | ignored (`gsong.c` identical to 2.77's) | |
| GoatTracker Stereo 2.77 | ignored; its stereo trial read must not mistake the trailer for orderlists (next section) | its `savesong` writes six orderlists a subtune whatever it loaded (`MAX_CHN` 6): a mono song saved there becomes a stereo file |
| GoatTracker 2 Qt fork @ `6c45fafd` | ignored; its `determinechannels` is Stereo's, and a short read counts as mono | |
| `gt2reloc` 2.77 | ignored: it calls the same `loadsong` (`gt2reloc.c` links `gsong.c`) | Task 3 runs it on files with trailers |
| ChiptuneSAK @ `8759135b` | **refused**: `import_sng_binary_to_parsed_gt` raises "bytes parsed didn't match file bytes length" (`goat_tracker.py` line 868) | a compatibility note: SIDOU's writer can leave the trailer out, for tools like this one |

GoatTracker's loader also trusts every count: a pattern count above 208, an instrument
count above 63, a subtune count above 32 or a pattern above 129 rows overflows its arrays.
SIDOU's reader refuses each of them with a message instead (design section 6: every reader
is a parser of untrusted input).

## The trailer and stereo detection

GoatTracker Stereo 2.77's trial read (its `gsong.c`, lines 1628-1656, `MAX_CHN` 6):

```c
int determinechannels(FILE* handle)
{
  int returnpos = ftell(handle);
  int c, d;
  int songs = fread8(handle);
  unsigned char songbuffer[257];

  for (d = 0; d < songs; d++)
  {
    for (c = 0; c < MAX_CHN; c++)
    {
      int loadsize = fread8(handle);
      loadsize++;
      memset(songbuffer, 0, 257);
      fread(songbuffer, loadsize, 1, handle);

      // Check that each track of each song has a valid endmark.
      // Should fail if it's a mono song (not certain)
      if ((songbuffer[loadsize - 2] != 0xff) || (songbuffer[loadsize - 1] >= loadsize))
      {
        fseek(handle, returnpos, SEEK_SET);
        return 3;
      }
    }
  }
  ...
  return MAX_CHN;
}
```

Each trial read at file offset p takes a length byte n = `file[p]`, reads n + 1 bytes, and
passes only if **`file[p + n] == $FF`** (the end mark) and `file[p + n + 1] < n + 1` (the
restart position). On a mono file, the first 3 × S reads (S subtunes) are the real
orderlists; the next 3 × S run on into the instruments, the tables, the patterns and,
on a small file, past the GT2 part into whatever follows. One failing read ends the trial:
mono.

**How far past the GT2 part a read can look.** The plan's first estimate (three
orderlists, 3 × 257 = 771 bytes) assumed one subtune and that reads past the end keep
passing. The bound is tighter and does not depend on the subtune count:

- Let E be the GT2 part's length (the trailer's first byte is at E). A read that starts
  inside the GT2 part (p ≤ E - 1) checks its end mark at p + n ≤ E - 1 + 255 = **E + 254**.
- A read can **start** inside the trailer only after a read that passed with its end mark
  inside the GT2 part (p + n ≤ E - 1), so it starts at p + n + 2 ≤ **E + 1**: at the
  trailer's byte 0 or 1. Its end mark is at its start plus that byte's value: the magic's
  `S` (`$53`) and `I` (`$49`) put them at E + 83 and E + 75.
- So if **the trailer's bytes 0 to 254 hold no `$FF`**, the first read whose end mark
  falls in the trailer fails, and the trial cannot pass anything inside the trailer. The
  restart-byte comparison can only turn a pass into a fail (it compares against the
  trailer's byte where a plain file gives the zero of a short read), so the trailer never
  turns a mono verdict into a stereo one.
- A read of length 0 (loadsize 1) makes the check read `songbuffer[-1]`, undefined in C.
  A read can start in the trailer only at bytes 0 and 1, which are `S` and `I`, never 0.

**The shape that follows:** the magic `SIDOU-X\0`, the version (u16 `01 00`) and the
length (u32, which the writer pads with a `PAD` chunk until none of its bytes is `$FF`)
hold no `$FF`; they take bytes 0-13. **A lead-in of 241 `$00` bytes** fills bytes 14-254.
The first field that can hold `$FF` (the GT2 part's CRC-32) starts at byte 255.

**Stereo files with a trailer** read as stereo: the trial reads exactly the 6 × S real
orderlists, all of which pass, and never reaches the trailer.

The Qt fork's copy of the check is the same, plus a short read counts as mono. ChiptuneSAK's
`is_2sid` counts consecutive orderlists whose byte at p + n is `$FF`, with no other limit;
the same bound (no `$FF` in bytes 0-254) stops it at the trailer, though ChiptuneSAK then
refuses the trailing bytes anyway.

GoatTracker Stereo's own comment says the trial is "not certain" for mono files: a mono
GT2 part can pass all 6 × S reads by itself. M2c Task 2's fuzz found such songs, and one
is easy to make: an instrument whose bytes read `$FF 00 01 $FF 00 01 $FF 00` looks like
three more orderlists. None of GoatTracker's 14 example songs does (their trials fail at
file offsets 117 to 371, inside the instruments and tables), and the trailer cannot make
it more likely. SIDOU's reader takes the same guess, then **settles it with the trailer**:
if the file read with the guessed channel count has no trailer whose CRCs hold, and read
with the other count has one, the other count wins. So SIDOU always reads its own files
right; on a plain GT2 file it agrees with GoatTracker Stereo.

A stereo song's orderlist with an end mark earlier than its length byte says, and a
restart byte of its entries plus two or more, passes the check as read but not once saved
canonically (the restart byte then sits right after the `$FF`). SIDOU's reader refuses such
a stereo song (`SNG: BAD ORDERLIST Vn`), so it never writes a stereo song that reads back
as mono.

## Round-trip rules

The user asked for exactly what round-trips.

1. **A canonical `GTS5` file round-trips byte for byte.** Canonical means one that
   GoatTracker's own `loadsong` then `savesong` leaves unchanged. SIDOU, loading and saving
   such a file without a trailer, writes it back identical, because SIDOU's writer does
   what `savesong` (lines 22-125, and `countpatternlengths`, lines 1320-1351) does:
   - **subtunes:** one more than the highest subtune whose three orderlists are all
     non-empty (at least 1); a trailing subtune with an empty channel is dropped;
   - **orderlists:** the length byte is the entries plus one (the `$FF`), then the entries,
     `$FF` and the restart byte;
   - **instruments:** the count is the highest instrument that is used, where used means
     any of AD, SR, the wave, pulse and filter pointers, the vibrato parameter or the
     vibrato delay is not zero, **or** a row of any pattern names it. Instruments above that are dropped: an instrument holding only
     a name, a gate-off timer or a first-frame waveform, after the last used one, is not
     saved (an unused one in the middle is kept);
   - **tables:** each is cut after its last row whose left or right byte is not zero;
   - **patterns:** the count is one more than the highest pattern that has a row other
     than an empty rest (note `$BD`, the other three bytes 0) before its end mark, or that
     an orderlist of any loaded subtune names (a dropped trailing subtune's too); a
     pattern named but not in the file is an empty 64-row pattern, as GoatTracker's
     `clearpattern` leaves it; each pattern's row count is its rows up to and including the
     first end mark, and the end row's three other bytes are written as they were read;
   - **strings:** the 32 bytes each, as read.

   Two corners, found by M2c Task 2's tests:
   - `savesong` counts the patterns named by every loaded subtune, a trailing subtune it
     drops (one with an empty channel) included. So saving such a song is not itself
     canonical: the next save drops the patterns only that subtune named, then holds.
     SIDOU's writer is `savesong` here too.
   - A pattern not held in memory is written as `clearpattern` leaves it: 64 rests and an
     end row of `$FF 00 00 00`. SIDOU's reader keeps such a pattern, when no orderlist
     names it, as "not held": it takes no arena, and writes back the same bytes.

   A non-canonical file (an older ident, a trailing name-only instrument, untrimmed
   counts, a pattern whose length byte runs past its first end mark, an orderlist whose
   length byte runs past its first `$FF`) loads, and is saved in canonical form, as
   GoatTracker would save it. Task 3 proves the rule against GoatTracker itself: every
   example song, canonicalised by GoatTracker's own `loadsong` and `savesong`, saves
   identically from SIDOU.
2. **A SIDOU file opened and saved in GoatTracker loses the trailer.** The music is
   intact; SIDOU's extras (locks, chip setups, slots, options) are gone, and SIDOU then
   plays it as a plain GT2 song with GoatTracker's default options.
3. **A trailer whose GT2-part CRC no longer matches** (a tool kept the trailing bytes but
   changed the song) is partly kept: the position-free chunks (`OPTS`, `CHPA`, `CHPB`,
   `SLOT`, `MIDI`, `SCNE`, `KEYS`, and unknown ones) are applied; the position-bound one
   (`LOCK`) is dropped, with a toast (`SNG: LOCKS DROPPED`). A trailer whose own CRC fails,
   whose length runs past the file, or whose version is unknown is dropped whole (the
   song still loads, `SNG: EXTRAS DROPPED`).
4. **What SIDOU cannot keep:** songs over the device's caps (the content region's arena,
   multispeed above 4: M2c Task 2) are refused on the device with the reason. They still
   load on the host, and in the companion.

## Equivalence, defined

For each frame, the ordered list of writes to `$D400-$D418` made by one call of
GoatTracker's play routine equals the C player's list for that frame. With multispeed N,
each of the N calls in a frame is compared.

The 6502 spreads its writes over hundreds of cycles inside a frame; the C player's land
together at the frame's sample. That is the documented difference (with buffered writes
on, GoatTracker itself writes a channel's registers together). The SID's state after each
frame is therefore the same, and so is everything a listener can hear, except where in
the frame each write lands: they differ by at most the time one call of the 6502 player
takes, a small part of the 20 ms (19,656-cycle) PAL frame.

## The reference harness

M2c Task 3 builds it: GoatTracker's packed player, relocated by `gt2reloc` with the song's
options, run in a **host-only 6502**: the `mos6502` crate (0.10.1, BSD-3-Clause,
<https://crates.io/crates/mos6502>), behind a `Cpu6502` trait so that M3's own 6502 can
be checked against it later. The design's device 6502 stays in M3.

Considered: `w65c02s` (0.9.2, Zlib) emulates a 65C02, not the NMOS 6502 the C64 has;
`emulator_6502` (1.1.0, MIT) and `yamos6502` (0.1.4, MIT) are less used. `mos6502` is the
most used and permissively licensed; Task 3 confirms it passes Klaus Dormann's functional
test.

The harness checks every opcode before it executes and fails the trace on any of the 105
undocumented ones, with the address and the frame. GoatTracker's `player.s` uses legal
opcodes only, so the harness proves that by execution. M3's own 6502 needs illegal opcodes
for `.sid` files; it must rely on Tom Harte's per-opcode tests for those, not on this
harness.

GoatTracker's source (GPL-2.0-or-later) is test-only: built by the host harness, never
linked into the firmware. SIDOU's C player is written from the readme and `player.s`'s
behaviour, credits GoatTracker, and stays GPL-3.0-or-later, which GPL-2.0-or-later's "any
later version" permits.

## The player's place

The C player runs **in the audio interrupt, at sample-exact frame boundaries** (overview
decision 2): the block is split at the frame's sample, as decision record 0005's "What
sample-accurate placement would take" describes for notes. Why:

- a frame is exactly 19,656 C64 cycles (PAL) whatever the main loop does (a chip
  preparation, a flash write, the USB retry);
- the host renders a song bit for bit as the device does, so the emulator gate's
  host-against-target check covers songs;
- it is the model M3's 6502 will use.

The cost: its tick joins the audio budget (at most 60 instructions a sample in its worst
128-sample block, gated in M2c Task 6) and its code joins RAMTEXT. Song edits reach it
only as O(1) steps (a cell write of at most 4 bytes, or an index swap to a pattern or
orderlist the main loop has rebuilt in free space), at most 8 a block (M2c Tasks 2 and 6).

## SID-Wizard

SID-Wizard import stays in M5, where design section 14 puts it: its format is a different
player model (its own instrument and table layout), and M2's work is GT2, which is now the
song file itself rather than one importer among several.

## Consequences

- `docs/reference/song-format.md` is the byte-level reference for the GT2 part as SIDOU
  reads it and for the trailer.
- M2c Task 2 implements the reader, the writer and the trailer; Task 3 builds the
  reference harness and the canonical round trip against GoatTracker's own code.
- The trailer's magic is `SIDOU-X\0`; the store's magic-number table (M2a Task 2's
  `docs/reference/store-format.md`) lists it once that document exists.

<!-- REUSE-IgnoreEnd -->
