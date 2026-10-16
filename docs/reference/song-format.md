<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Song file format

<!-- REUSE-IgnoreStart -->

SIDOU's song file is GoatTracker 2's `.sng` (decision record
[0007](../decisions/0007-gt2-song-format.md)), optionally followed by SIDOU's trailer. This
page is the layout as SIDOU reads and writes it. The GT2 part follows GoatTracker 2.77's
`readme.txt` section 6.1 and its `gsong.c`; the readme is the authority for what the data
means (sections 3.1-3.5), and is not repeated here.

All multi-byte numbers are little-endian. Offsets are from the start of the part described.

## The GT2 part

### Header

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | ident: `GTS5` (also read: `GTS2`, `GTS3`, `GTS4`; 0007 "Versions") |
| 4 | 32 | song name, zero-padded |
| 36 | 32 | author, zero-padded |
| 68 | 32 | copyright, zero-padded |
| 100 | 1 | subtune count S, 1-32 |

### Orderlists

S × C orderlists, subtune by subtune, channel by channel, where C is 3, or 6 for a
GoatTracker Stereo file (detection below). Each:

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | n: the entries plus one (the `$FF`); 1-255 |
| 1 | n - 1 | entries: `$00-$CF` pattern, `$D0-$DF` repeat, `$E0-$FE` transpose |
| n | 1 | `$FF` (RST) |
| n + 1 | 1 | restart position (an index into the entries) |

SIDOU's reader takes the entries up to the first `$FF` within the n bytes, as GoatTracker's
loader does, and the byte after it as the restart position; there must be one, or the
orderlist is refused. Nothing else is checked: an orderlist that breaks readme 3.1's rules
(a transpose after a repeat, a last entry that is not a pattern) or a mono restart position
at or past its entries loads as it is, since GoatTracker loads, saves and plays it. SIDOU's
player plays it as GoatTracker's `player.s` does; SIDOU's edits never make one, and refuse
an entry write that leaves one breaking the rules (`GT2_ERR_ORDER_RULE`).

### Instruments

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | count I, 0-63 |
| 1 | 25 × I | instruments 1 to I |

An instrument's 25 bytes: AD, SR, wave pointer, pulse pointer, filter pointer, vibrato
parameter (a speedtable pointer), vibrato delay, gate-off timer, first-frame waveform,
16-byte name. (`GTS2` stores the vibrato delay before an old-style vibrato parameter;
0007 "Versions".)

### Tables

Four times, for the wave, pulse, filter and speed tables (`GTS2`: three, no speed table):

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | row count r, 0-255 |
| 1 | r | left bytes |
| 1 + r | r | right bytes |

### Patterns

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | count P, 0-208 |

Then P patterns, numbered from 0:

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | rows m, including the end row; 1-129 |
| 1 | 4 × m | rows: note, instrument, command, data |

Notes: `$60-$BC` C-0 to G#7, `$BD` rest, `$BE` key off, `$BF` key on, `$FF` end of
pattern. Instrument `$00` (none) to `$3F`; command `$0-$F`. A pattern ends at its first
`$FF` row; rows after it in the file are dropped.

The GT2 part ends after the last pattern. Its length is E.

### What SIDOU's reader refuses

Every refusal leaves the song in memory as it was, with a message short enough for a
toast:

| Message | When |
|---|---|
| `SNG: NOT A SONG` | an ident other than `GTS2`-`GTS5` or `GTS!` |
| `SNG: GT1 FORMAT` | `GTS!` (GoatTracker 1) |
| `SNG: SHORT FILE` | the file ends inside the GT2 part |
| `SNG: EDITS PENDING` | a reload while edits made on the song still wait for the audio interrupt (`core/song/gt2_song.h`, the deferred contract) |
| `SNG: BAD SUBTUNES` | a subtune count of 0 or above 32 |
| `SNG: BAD ORDERLIST Vn` | channel n's orderlist (1-6) has no `$FF` with a restart byte after it, or (stereo) a restart byte of its entries plus two or more |
| `SNG: BAD INSTRUMENTS` | more than 63 instruments |
| `SNG: BAD PATTERN n` | pattern n has more than 129 rows, no end row, a note outside `$60-$BF`, an instrument above `$3F` or a command above `$F` before its end |
| `SNG: BAD PATTERNS` | more than 208 patterns |
| `SNG: TOO BIG (60K OF 51K)` | the orderlists and patterns do not fit the arena with its spare: on the device 53,120 bytes (51K) of the 64 KiB content region (`core/song/gt2_song.h`) |
| `SNG: MULTISPEED 8 (MAX 4)` | on the device: the `OPTS` chunk asks for multispeed above 4 (0, 25 Hz, is refused as `MULTISPEED 0`) |

What GoatTracker accepts and plays oddly (a restart position past the end, a transpose
after a repeat, a missing pattern) loads: those are the editing rules of readme 3.1, which
SIDOU's editing primitives enforce and its player follows as GoatTracker's does. An
orderlist naming a pattern the file does not have gets an empty 64-row pattern, as
GoatTracker's loader leaves one. An empty 64-row pattern no orderlist names is kept as
"not held": it takes no room, and the writer writes it back as the same 64 rests.

### Mono or stereo

A GoatTracker Stereo file has the same ident and six orderlists a subtune. SIDOU's reader
decides as GoatTracker Stereo's `determinechannels` does: starting at the subtune count,
it trial-reads 6 × S orderlists; a read at offset p with length byte n passes when the
byte at p + n is `$FF` and the byte at p + n + 1 is below n + 1 (a byte past the end of
the file counts as 0; a length byte of 0 fails). All passing: stereo, else mono. The guess
is GoatTracker Stereo's, and can be wrong, so SIDOU also reads the file with the other
count and takes the reading that ends better, in this order: at a SIDOU trailer whose CRCs
hold and that records that reading's channel count; at a trailer whose CRCs hold; at one
whose own CRC holds; at a damaged trailer's magic; at the file's last byte. When both end
equally well, the reading whose counts are what `savesong` would write (the last subtune's
first three orderlists, the last instrument, each table's last row and the last pattern in
use) wins over one whose counts are not; otherwise the guess stands (0007, "The trailer and
stereo detection"). A stereo song whose orderlist has a restart byte of its entries plus two
or more is refused (`SNG: BAD ORDERLIST Vn`): saved, it would read as mono.

### Writing

SIDOU writes `GTS5` with the counts trimmed as GoatTracker's `savesong` trims them
(0007, "Round-trip rules", rule 1): the subtunes, the instruments, each table and the
patterns; each orderlist and pattern written up to its end mark.

## SIDOU's trailer

Optional. A file without one is a plain GT2 song, played with GoatTracker's default
options. The writer can leave it out (for tools that refuse trailing bytes, such as
ChiptuneSAK; 0007). Without it, a rare file is the canonical save of a mono song and of a stereo
one alike, which no reader can tell apart; after a plain export `gt2_sng_check_plain`
reads the file back and warns (`SNG: READS BACK AS STEREO`, `SNG: READS BACK AS MONO`) if it
would load with the other channel count.

### Layout

The trailer starts at E, right after the last pattern. L is its total length.

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | magic `SIDOU-X` and a 0 byte: `53 49 44 4F 55 2D 58 00` |
| 8 | 2 | trailer version, 1 |
| 10 | 4 | L, from the magic to the end, the final CRC included; **no byte of it is `$FF`** |
| 14 | 1 | the GT2 part's channel count: 3, or 6 for a GoatTracker Stereo song (0: not recorded) |
| 15 | 240 | lead-in: `$00` bytes |
| 255 | 4 | CRC-32 of the GT2 part, file bytes 0 to E - 1 |
| 259 | L - 263 | chunks |
| L - 4 | 4 | CRC-32 of the trailer's bytes 0 to L - 5 |

**Why bytes 0-254 hold no `$FF`:** GoatTracker Stereo's stereo check (above) can look at
most 254 bytes past the GT2 part, and only at bytes whose `$FF` would let a read pass.
With no `$FF` there, no trial read can pass inside the trailer, so a mono file with a
trailer stays mono in GoatTracker Stereo and in SIDOU (0007, "The trailer and stereo
detection", has the bound). The writer keeps L free of `$FF` by adding a `PAD` chunk of
the smallest size that does it.

**CRC-32** is the common one (as zlib's `crc32`): polynomial `0x04C11DB7` reflected
(`0xEDB88320`), initial value and final XOR `0xFFFFFFFF`. The CRC of `123456789` is
`0xCBF43926`.

### Reading the trailer

After the GT2 part, a reader looks for the magic at E:

- no magic (or fewer than 8 bytes left): no trailer; anything after E is ignored, as
  GoatTracker ignores it;
- a version other than 1, an L shorter than 263 or running past the end of the file, or
  a trailer CRC that does not match: the trailer is dropped whole and the song loads
  without it (`SNG: EXTRAS DROPPED`);
- a GT2-part CRC that does not match (a tool changed the song but kept the bytes): the
  position-bound chunks (`LOCK`) are dropped (`SNG: LOCKS DROPPED`) and the rest kept;
- bytes after L are ignored.

### Chunks

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | tag, four ASCII characters |
| 4 | 4 | length d of the data, at least 1 |
| 8 | d | data; its first byte is the chunk's own version |

Readers skip tags they do not know, and a chunk version they do not know. Unknown chunks
are kept, and written back unchanged in their place, so a newer SIDOU's chunks survive an
older one's save. Every chunk SIDOU keeps (the trailer less its header, lead-in, CRCs and
`PAD`) fits in the song's extension area, `SONG_EXT_BYTES` = 4,096 bytes.

| Tag | Holds | Position-bound | Defined by |
|---|---|---|---|
| `OPTS` | the playroutine options | no | M2c Task 2 |
| `CHPA` | chip A's setup | no | M2c Task 2; expert values M2b |
| `CHPB` | chip B's setup | no | M2c Task 2; expert values M2b |
| `SLOT` | the effect slots | no | M2c Task 2 |
| `LOCK` | parameter locks | **yes** | M2c Task 2; semantics M2c Task 7 |
| `MIDI` | MIDI mode, channels, rows per beat | no | M2c Task 2; semantics M2e |
| `SCNE` | the SCENE piece, the voices' roles and the drums | no | M2c Task 2; semantics M2f Task 4 |
| `KEYS` | the keys' live preset and setup | no | M2c Task 2 |
| `PAD ` | padding (a space ends the tag) | no | skipped, never kept |
| `TRKB` | reserved: chip B's tracks (M5) | | |
| `RIPD` | reserved: rip data (M5) | | |

### Version 1 of each chunk

**`OPTS`** (23 bytes), every `gt2reloc` option (readme 4.4) and the editor's SID model:

| Offset | Size | Field | `gt2reloc` | Default |
|---|---|---|---|---|
| 0 | 1 | version, 1 | | |
| 1 | 2 | hard restart ADSR | `-A` | `$0F00` |
| 3 | 1 | multispeed: 0 (25 Hz), 1-16 | `-S` | 1 |
| 4 | 1 | timing: 0 PAL, 1 NTSC | `-P`, `-N` | 0 |
| 5 | 4 | custom SID clock, Hz (0: PAL's or NTSC's); gt2reloc 2.77 parses `-F` and never uses it, so it changes nothing played | `-F` | 0 |
| 9 | 4 | pitch of A-4, thousandths of a Hz (0: the built-in table); SIDOU's player keeps the built-in table and reports a non-zero value (`GT2_PLAY_WARN_PITCH`) | `-G` | 0 |
| 13 | 1 | fine vibrato: 0 off, 1 on (below 2x multispeed), 2 always | `-V` | 1 |
| 14 | 1 | pulse optimisation: 0 off, 1 on | `-O` | 1 |
| 15 | 1 | realtime-effect optimisation: 0 off, 1 on | `-R` | 1 |
| 16 | 2 | player flags, GoatTracker's `PLAYER_*` bits shifted right by 3: bit 0 buffered writes (`-B`), 1 sound effects (`-D`), 2 volume change (`-E`), 3 author info (`-H`), 4 zero-page ghost registers (`-C`), 5 optimisations **off** (`-I0`), 6 full buffering (`-J`) | | 0 |
| 18 | 2 | SID address | `-L` | `$D400` |
| 20 | 1 | player address, high byte | `-W` | `$10` |
| 21 | 1 | zero-page address | `-Z` | `$FC` |
| 22 | 1 | SID model: 0 6581, 1 8580 | (editor) | 0 |

A reader clamps what `gt2reloc` clamps, as it does: multispeed above 16 to 16, pulse and
realtime optimisation above 1 to 1, a custom clock below 100 Hz to 0, fine vibrato above 2
to 2. A timing byte or SID model above 1, or a player flag above bit 6, has no meaning: the
chunk is not used and the defaults stand (the chunk itself is kept, as written).

**`CHPA`, `CHPB`** (9 + x bytes): a chip's setup.

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | version, 1 |
| 1 | 1 | model: 0 6581, 1 8580 |
| 2 | 1 | profile: `sid_character.profile` (0 the model's default, else 1 + a `SID_CHIP_*`) |
| 3 | 4 | the character: age/heat, DAC sloppiness, thump, bleed, each 0-127 |
| 7 | 1 | envelope mode: 0 INSTANT, 1 AUTHENTIC (songs default to AUTHENTIC, 0005) |
| 8 | 1 | x, the count of expert values |
| 9 | x | the expert values (M2b's expert layer; 0 of them until then) |

**`SLOT`** (2 + 5 × k bytes): the effect slots, as `live` keeps them.

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | version, 1 |
| 1 | 1 | k, the slot count (3) |
| 2 | 5 × k | per slot: algorithm, Amount, A, B, Mix (the controls 0-127) |

**`LOCK`** (3 + 5 × k bytes): parameter locks, sorted by pattern, row, parameter.

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | version, 1 |
| 1 | 2 | k, the lock count, at most 512 |
| 3 | 5 × k | per lock: pattern, row, parameter, value (u16) |

A lock replaces one parameter for one row of a pattern, on each channel that plays the
pattern (M2c Task 7): the song's own value returns at the next row without it, and the
player's own state is not changed, so its tables run on underneath.

| Parameter | Value | Writes |
|---|---|---|
| 0 cutoff | 0-2047 | `$D415`'s low 3 bits and `$D416` |
| 1 resonance | 0-15 | `$D417`'s high nybble |
| 2 filter mode | 0-7: bit 0 low-pass, 1 band-pass, 2 high-pass | `$D418` bits 4-6 |
| 3 routing | 0-1: the row's voice into the filter | `$D417`'s bit for the voice |
| 4 pulse width | 0-4095 | the row's voice's `$D402`, `$D403`'s low nybble |
| 5-8 age/heat, DAC sloppiness, thump, bleed | 0-127 | chip A's character, with chip_ctl's glide (0.32 s end to end: coarse by nature) |

The filter and pulse locks are written over the player's registers after each call of the
row, in the audio interrupt; at most 8 resolve for a channel's next row (a pattern's next
row, or at its end the pattern again and the orderlist's next one). A pattern's rebuild
carries its locks with its rows: inserted and deleted rows move them, a cut drops those past
it, a removed pattern drops all of its.

**`MIDI`** (4 + c bytes, and 5 of options since M2e: 9 + c), MIDI's mode for the song
(`app/midi_route.h`; [midi.md](midi.md), Modes):

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | version, 1 |
| 1 | 1 | mode: 0 LIVE, 1 MULTITIMBRAL, 2 WHOLE-CHIP A, 3 WHOLE-CHIP B, 4 OMNI |
| 2 | 1 | rows per beat, for the clock (1-16) |
| 3 | 1 | c, the channel count |
| 4 | c | MIDI channels, 0-15, one per voice: chip A's voices 1-3, then the instrument's (chip B). MULTITIMBRAL N, N+1, N+2, N+3; WHOLE-CHIP A N, N, N, N+3; LIVE and WHOLE-CHIP B one, N; OMNI none |
| 4 + c | 1 | flags: bit 0 LEGATO |
| 5 + c | 1 | WHOLE-CHIP A's option: 0 POLY, 1 CHORD, 2 ARP |
| 6 + c | 2 | the stored chord: semitones above the note on voices 2 and 3 (0-24) |
| 8 + c | 1 | the arpeggio's rate, frames a step (1-32), 0 a 16th of the MIDI clock |

A reader takes N from the first channel. A chunk that ends after the channels (M2c's) has
no options: the reader takes them from the device's setting. An unknown mode is the
setting's; values out of range are clamped.

**`SCNE`** (3 + v bytes, or 4 + v + 2 × d with a drum table): what RED MOON RITE hears in
the song (`app/scene_song.h`; docs/art/README.md "RED MOON RITE").

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | version, 1 |
| 1 | 1 | the SCENE piece (kept; M2 does not act on it) |
| 2 | 1 | v, the voice count |
| 3 | v | each voice's role: 0 AUTO (guessed), 1 DRUMS, 2 BASS, 3 ARP, 4 LEAD (as ARP), 5 NONE |
| 3 + v | 1 | d, the drum table's pairs, 0-16 (absent: none) |
| 4 + v | 2 × d | per pair: an instrument (1-63) and its drum: 1 KICK, 2 SNARE, 3 HAT |

A voice left AUTO, and a drum voice's instrument not in the table, are guessed as
`app/scene_song.h` describes. A reader older than the drum table reads the roles and stops.

**`KEYS`** (2 + 2 × q bytes): the keys' live preset and their chip setup, as `live_params`
reports them (`app/live.h`, `LIVE_PARAM_*`; the model, envelope mode, profile and
character are among them).

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | version, 1 |
| 1 | 1 | q, the parameter count (`LIVE_PARAM_COUNT` when written) |
| 2 | 2 × q | the parameters in `LIVE_PARAM_*` order, u16 each |

**`PAD `** (1 + p bytes): version 1, then p bytes of `$00`.

<!-- REUSE-IgnoreEnd -->
