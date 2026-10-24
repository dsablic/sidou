<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The PSID file: what SIDOU reads, refuses, and runs

SIDOU plays `.sid` files in the PSID format (the C64 music files of the High Voltage SID
Collection). This page is what SIDOU takes from the format, what it refuses and why, the
environment a tune runs in, and the known limits. The format itself is HVSC's
`SID_file_format.txt` (revision of 2026-10-09, versions 1 to 5); this page restates what
SIDOU relies on in its own words and names every place SIDOU's plan differs from it. The
parser is `core/psid/psid_file.c`; its tests are
`host/crates/sidou-tests/tests/psid_file.rs`. The environment (calls, period, I/O page) is
built by M3's Tasks 6a and 6b; this page fixes what they build.

All numbers in the file are big-endian, except the two bytes of a load address in the data,
which are little-endian (the 6502's order).

## The header

| Offset | Size | Field | SIDOU's use |
|---|---|---|---|
| `+00` | 4 | magic `PSID` or `RSID` | `RSID` is refused |
| `+04` | 2 | version 1-5 | 0 and 6 and up are refused |
| `+06` | 2 | data offset | `$76` (v1) or `$7C` (v2 and up), exactly, and not past the file |
| `+08` | 2 | load address | 0: the data's first two bytes (little-endian) are the load address |
| `+0A` | 2 | init address | 0: the load address |
| `+0C` | 2 | play address | 0: the tune installs its own interrupt handler |
| `+0E` | 2 | songs, 1-256 | |
| `+10` | 2 | start song | 0 or above songs: song 1 |
| `+12` | 4 | speed bits | one bit per song, 0 = vertical blank (the frame), 1 = CIA 1 timer A |
| `+16` | 96 | name, author, released | three strings, see "Strings" |
| `+76` | 2 | flags (v2 and up) | see "Flags" |
| `+78` | 2 | relocation start page, pages | read by nobody: SIDOU never relocates |
| `+7A` | 2 | second and third SID (v3, v4) or multi-SID configuration (v5) | see "More than one SID" |
| `+7C` | | data | |

A v1 header ends at `+76`; the data starts there. Everything v2 and up adds is read only
after the file is known to reach `+7C`.

### Flags

| Bits | Meaning | SIDOU's use |
|---|---|---|
| 0 | MUS data (Compute!'s Sidplayer) | refused: it needs an external player SIDOU does not have |
| 1 | PlaySID specific | plays, with a warning: such tunes use PlaySID's `$D41D` sample mechanism, which is silent here |
| 2-3 | video standard: 0 unknown, 1 PAL, 2 NTSC, 3 both | the tune's clock (below) |
| 4-5 | SID model: 0 unknown, 1 MOS 6581, 2 MOS 8580, 3 both | the chip's profile (below) |
| 6-7 | second SID's model (v3, v4); panning layout (v5, ignored) | |
| 8-9 | third SID's model (v4); panning mode (v5, ignored) | |
| 10 | song lengths appended (v5) | the table is cut off the data; SIDOU does not read the lengths |
| 11 | FM OPL chip needed (v5) | plays, with a warning: there is no OPL, its writes go to RAM |
| 12-15 | reserved | ignored |

A v1 file has no flags: every one reads as 0, and its speed bits repeat (below).

### Strings

Name, author and released are Windows-1252 text. SIDOU's font has the ASCII characters
`$20-$7E`, so every other byte (control codes, and Latin-1 and Windows-1252 letters) is
shown as `?`. The parser copies each string NUL-terminated and cuts it at 63 bytes.

A v1-v4 header holds three fixed 32-byte fields; a field that fills all 32 bytes has no NUL,
and bytes after a field's first NUL are ignored. A v5 header may instead pack the three
strings into the 96 bytes (each starting at the first non-NUL byte after the previous
one's NUL); the reader tries that layout first and falls back to the three fixed fields
unless all three come out non-empty, as the format says.

### More than one SID

- **v2:** the bytes at `+7A`, `+7B` are reserved and ignored.
- **v3, v4:** `+7A` is the second SID's address and (v4) `+7B` the third's, each the middle
  part of `$Dxx0`: `$42-$7E` and `$E0-$FE`, even values only. Any other value means "no such
  SID", as 0 does: the file is accepted and the header carries a warning. A third address
  equal to the second is dropped the same way. The third SID's writes are dropped by the
  environment (`3SID: 3RD SILENT`).
- **v5:** `+7A` holds the number of chips (bits 0-3, 2 to 15; 0 for one chip) and an address
  scheme (bits 4-7; 0-6 are defined, 7-15 are refused). `+7B` is the second SID's start
  address (same encoding; any invalid value means the scheme's own start). The parser
  generates the addresses the way the format lists them (steps of `$20`, `$40`, `$80`,
  `$100`, or alternating `$20, $60`, `$20, $E0`, `$100, $300`; a step that lands in
  `$D800-$DDFF` continues at `$DE00`; a step past `$DFE0` ends the list). A count the scheme
  cannot reach is refused. SIDOU plays two chips: the second and third addresses go in the
  header, chips past the third are dropped, with a warning. With the FM OPL flag set, a chip
  inside `$DF00-$DFFF` is refused, as the format says.

The model of the second and third SID is the first's when its bits are 0 (v3, v4), and always
the first's in v5, as the format says.

## What SIDOU refuses, and why

`psid_parse` checks every field against the file's length before using it, and returns one
code. Nothing is read past the file, and nothing is kept: the result is a `psid_header`
the caller owns.

| Check | Refusal | Why |
|---|---|---|
| shorter than `$76`, or the magic is not `PSID` or `RSID` | `PSID_E_NOT_SID` | not a `.sid` file |
| magic `RSID` | `PSID_E_RSID` | needs a real C64's hardware and a KERNAL; see "Known limits" |
| version not 1-5; data offset not `$76` (v1) or `$7C` (v2 and up); offset past the end | `PSID_E_HEADER` | damaged or crafted; the offset is where everything else is read |
| v2 and up and shorter than `$7C` | `PSID_E_HEADER` | the flags would be read past the end |
| songs not 1-256 | `PSID_E_HEADER` | init takes `songs - 1` in the accumulator, a byte |
| MUS flag | `PSID_E_MUS` | external player needed |
| load address 0 and fewer than two bytes of data | `PSID_E_HEADER` | the load address would be read past the end |
| v5: song-length table does not fit in the file | `PSID_E_HEADER` | |
| v5: bad address scheme or count; chip in `$DF00-$DFFF` with the OPL flag | `PSID_E_HEADER` | the format makes these invalid |
| no data byte, or load + length over `$10000` | `PSID_E_TOO_LONG` | would run off the top of the 6502's memory |
| the data covers `$0000-$0001` | `PSID_E_LOAD` | those are the 6510's port; a tune cannot be loaded over them |

**Accepted, with a correction:** a start song of 0 or above `songs` plays song 1 (warning); an
init address of 0 is the load address; a second or third SID address that is invalid means
no such SID (warning).

**Not checked:** that init and play lie inside the loaded data (the format says init must;
many tunes' play routines are installed by init, and a bad address only makes the tune
stop with the environment's own errors).

What the parser returns for an accepted file: the version; the effective load, init and
play addresses; where the data is in the file (`data_off`, `data_len`: the song-length
table and the two load-address bytes are not in it); songs, start song, speed bits,
flags; the clock and model codes; the second and third SID's addresses (0 for none) and
the chip count; the warnings; and the three strings. Every value satisfies
`data_off + data_len <= n` and `load + data_len <= $10000`; the fuzz test checks both.

## The environment

`core/psid/psid_env.c` (tests: `host/crates/sidou-tests/tests/psid_env.rs`, on tunes built by
the test, `src/psid_fixtures.rs`).

### Memory

Loading a tune zeroes the 6502's 64 KiB, sets the 6510's port to its power-on state (`$00` =
`$2F`, `$01` = `$37`) and copies the data to its load address. Starting a song does not reload
it: a player that changes subtunes loads the tune again first, as a C64 player does, so init
finds memory as the file left it.

### Calls

Init runs with A = song - 1, X = Y = 0, the I flag set and D clear, S = `$FF` after a
pushed return address of `$0000` (an RTS lands on `$0001`). Play runs the same with A = 0.
A play address of 0 calls the tune's own interrupt handler (`$0314` when `$01` maps the
KERNAL in, else `$FFFE`) with P pushed as an interrupt does. A call ends at a return to
`$0000` or `$0001`, or a jump to the KERNAL's interrupt exits `$EA31`, `$EA81`, `$FEBC` (no
ROM is emulated; those addresses stand for them).

What the implementation fixes beyond that:
- A play address of 0 leaves `$01` as the tune set it (its init chose the vector by it). The
  stack is what the interrupt leaves: P pushed (B and I clear, as the interrupted program's).
  Through `$0314` it is also what the KERNAL's entry (`$FF48`) leaves: A, X and Y pushed, and
  X = S, so a handler may end with the KERNAL exit's own `PLA`, `TAY`, `PLA`, `TAX`, `PLA`,
  `RTI` instead of jumping to `$EA31`; S is `$F9` there and `$FC` through `$FFFE`. The
  interrupt sequence's and the KERNAL entry's own cycles are not counted: a call's first
  cycle is the routine's.
- A call whose address is `$0000`, `$0001` or a KERNAL exit (an interrupt vector the tune
  never set, say) returns at once, with nothing run.

### The bank register

`$01` is written before every init and play call, from the call's address, as the format
requires: `$37` below `$A000`, `$36` from `$A000` below `$D000`, `$34` for `$D000-$DFFF`,
`$35` from `$E000`. (Checked against the format's text on 2026-10-23: it matches.)

### Limits

Init may run 5 s of 6502 time (the clock's rate times 5); past that the tune stops with
`INIT TOO LONG`. A play call that runs past its period is an overrun (counted and shown); the
next call then starts when it returns, as a C64's next interrupt waits for the handler's
RTI. A play call that has not returned after one second of 6502 time stops the tune with
`PLAY HUNG`. A JAM stops it with `6502 JAM AT $xxxx`. The 6502 runs whole instructions, so a
stop comes at the first instruction boundary at or past its limit (init's from cycle 0, a
play call's from its start).

### Time

Cycle 0 is init's first cycle. The first play call is at the first period boundary after
init returns, `(init_end / P + 1) * P`; call n + 1 is due one period after call n was due, or
at call n's return if that is later.

Precisely, as a C64's interrupts behave: each call has its interrupt's cycle, and the next
interrupt is one period after it. A call that returns after that interrupt is an overrun:
the next call starts at its return, and every interrupt it missed is one pending interrupt
(the IRQ line stays low until acknowledged), so the next call's interrupt is the last one
that passed, and the one after it comes a period later, on the same grid. A call that
returns exactly on the next interrupt is not an overrun. The time is a 32-bit count compared
with wrapping arithmetic, so it runs past 2^32 cycles (72 minutes) unharmed.

### The period

A VBI song's period is the frame: PAL 19,656 cycles (312 lines of 63), NTSC 17,095 (263 lines
of 65). A CIA song's is CIA 1 timer A's latch + 1 as the latch stands when the previous
call returned, with no floor (FM-1 SID replaces periods under 1,000 cycles with the frame;
SIDOU honours them). The latch starts at the KERNAL's value for the clock: `$4025` PAL,
`$4295` NTSC.

**The speed bit for song n.** The format makes this depend on the flags, and SIDOU follows
it. With PlaySID flag clear in a v2-or-later file, bit `min(n, 32) - 1`: songs above 32 all
take bit 31. For a v1 file, or with the PlaySID flag set, the bits repeat: bit `(n - 1) mod
32`. `psid_speed_bit` does this. (The plan's first draft said bit 31 unconditionally; the
format is the authority.)

### The clock

The video-standard bits: NTSC plays at 1,022,727 Hz with NTSC frames; PAL, unknown and both
play at 985,248 Hz with PAL frames. Chip A (and B for a tune with a second SID) is prepared
at that clock.

### The chip model

With the SID MODEL setting at AUTO (the default), "6581" plays the 6581 R4AR profile
(reSIDfp's defaults), "8580" the 8580 R5, and "unknown" or "both" the keys' setup (the
user's chip choice); the second SID's model comes from its own bits (v3, v4; 0 means as the
first). With SID MODEL at KEYS every tune plays the keys' setup. The character controls
start at zero for AUTO's two named profiles. While a tune plays, CHIP's A tab changes chip A
as it does during a song.

### The I/O page (while `$01` maps it in)

- **SID:** `$D400-$D7FF` is chip A (every 32 bytes a mirror), except a second SID's 32-byte
  window, which is chip B and takes priority. A third SID's writes are dropped and counted.
- **VIC:** `$D012` and bit 7 of `$D011` give the raster line from the absolute cycle,
  `(t % frame) / line_cycles`; `$D019` reads `$81` from the start of a VBI play call until the
  tune writes it, else `$00`; the other VIC registers read back what was last written to
  them (a 64-byte shadow).
- **CIA 1:** timer A's latch (`$DC04`, `$DC05` writes) and its counter (reads, computed from
  the absolute cycle and the latch); `$DC0D` reads `$81` at the start of a CIA play call until
  read, then `$00`; CIA 2's `$DD0D` reads `$00`. The other CIA registers read back what was
  last written (16 bytes each).
- **Colour RAM** (`$D800-$DBFF`) is not kept: writes are dropped and reads return `$00`.
- Everything outside the I/O page, and all of it while `$01` maps RAM, is RAM.

### SID reads

A tune's reads of a SID are answered by that chip's **voice shadow** (`core/sid/sid_shadow.h`):
a copy of the chip's three voices that the main loop clocks with the 6502, since the audio
chip runs a frame or two behind the 6502 and cannot answer. The shadow sees every write the
chip sees, at the same cycle, and runs the chip's own voice code, so it is the chip's voices
at the read's cycle (hard sync aside, below). A tune with two SIDs has a shadow per chip.

- **`$1B`, OSC3:** the top 8 bits of voice 3's waveform output as reSIDfp latches it, every
  waveform and combination, ring modulation and sync included. A read in the same cycle as a
  write to voice 2 or 3 returns the value from before the write, as on the chip.
- **`$1C`, ENV3:** voice 3's envelope counter, exactly as reSIDfp's (the ADSR delay bug
  included), so a tune that waits for its envelope to reach a level waits as long as on a C64.
- **`$19`, `$1A`, the paddles:** `$FF` (nothing connected).
- **Every other register,** the write-only ones and the three unused: the data bus, which
  holds the last value written to any register or read from `$19`-`$1C`, as reSIDfp keeps it
  (`SID.cpp`). Each write, and each read of `$19`-`$1C`, leaves its value there for `0x1d00`
  cycles on a 6581 and `0xa2000` on an 8580; after that the bus reads `$00`. A read of any
  other register halves what is left of that time (the read discharges the bus faster); a time
  halved to 0 from 1 then never runs out, as in reSIDfp, until the next write or read of
  `$19`-`$1C`. The bus starts at `$00`.

Where OSC3 is not reSIDfp's (the shadow's test, `tests/sid_shadow.rs`, skips and counts these
reads; every other read matches reSIDfp's, on both models and every combined-waveform profile):
- **No waveform selected:** SIDOU reads `$00`; reSIDfp holds the last output on its floating
  DAC and fades it after 54,000 (6581) or 800,000 (8580) cycles.
- **One or two cycles after a write** to voice 3's frequency, pulse width or control, or to
  voice 2's frequency: the old values drove those cycles.
- **Hard sync:** a synced voice's own noise clocks and MSB rise earlier in a step are not
  recomputed, so with SYNC set the voices can part from reSIDfp's, and from the audio chip's
  (which steps a sample at a time, the shadow at most 16 cycles while any voice has SYNC set).
  On random streams with sync, 0.2 % to 5 % of OSC3's reads differ from reSIDfp's, no more
  than the audio chip's own at the same cycles.

## Known limits

- **No ROMs.** The KERNAL, BASIC and character ROMs are not there (they are Commodore's).
  Calls end at the KERNAL's interrupt exits instead of running them.
- **No colour RAM.** Reads return 0.
- **No NMI,** no CIA timer B or time-of-day, no VIC interrupts: only the speed bit's frame or
  CIA timer A drives the tune.
- **No RSID.** Those tunes need a cycle-scheduled CIA 1 and 2, VIC raster interrupts, the C64's
  power-on state and a KERNAL. They are refused as `RSID: NOT YET`; a later plan may add them.
  C64 BASIC tunes are RSID, so they are refused too.
- **No MUS.** Compute!'s Sidplayer data needs its player merged in.
- **PlaySID samples are silent:** a tune with the PlaySID flag plays, and its `$D41D` samples
  make no sound.
- **A third SID is dropped,** and v5's chips past the third.
- **FM OPL** (v5) is not emulated.
- **Song lengths** (v5) are not read.
