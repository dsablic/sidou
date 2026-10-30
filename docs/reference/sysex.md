<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU SysEx protocol, version 1

Hardware acceptance is separate: USB-MIDI output has failed on a real unit, and
flash write-window timings have not been measured. The production-default PUT
uses explicit program holds; hold-free operation requires validated windows.

## Frames and packing

SIDOU accepts `F0 7D 53 44 command sequence packed-data checksum F7` on USB
and TRS MIDI IN. Command and sequence are seven-bit bytes; sequences wrap modulo
128. Payloads contain at most 256 decoded bytes. For each group of one to seven
payload bytes, send a high-bit mask followed by their low seven bits. Bit zero of
the mask belongs to the first byte. Unused mask bits must be zero. An empty
payload has no mask. The sum of command, sequence, all packed bytes and checksum
is zero modulo 128. The manufacturer and `SD` prefix are not checksummed.

Real-time bytes may occur anywhere and do not affect framing or checksums. Any
other status inside a frame aborts it. Oversized, noncanonical and truncated
packets are refused. Frames from another manufacturer are ignored, including
the update protocol; SIDOU's parser neither handles nor generates updater frames.
Transfers belong to one input port; the other port cannot interrupt or continue
them. Input lost on a transfer's port (SIDOU's MIDI-in ring overflowed, or the
TRS UART overran) aborts it: on USB, a reply frame already going out is finished
to its F7 first, then a NAK with reason 9 follows. Input lost on the other port
does not touch it. After a loss, each port's parser starts again at its next F0.

All multibyte payload integers are little-endian. Names are 16-byte fields,
NUL-padded, with one to sixteen characters in `20`–`5F`, not starting with a
space. Bytes following the first NUL must be zero. CRC means IEEE reflected
CRC-32, the same as the store and zlib.

### FM-1 Live Link frames

`F0 7D 46 4D 31 4C` ("FM1L") frames are Live Link's, the screen mirror and remote control
([live-link.md](live-link.md)): the same packing and checksum rule as above, with no sequence
byte, decoded payloads of at most 48 bytes, and their own commands. They share the USB port
with this protocol and never interleave with it: a Live Link frame goes out whole, and none goes
while a transfer of this protocol (or its reply) is under way on USB. On TRS they are ignored.

## Commands

| Byte | Command | Request or response payload |
|---|---|---|
| `01` | INQ | Request empty; reply: protocol/version/store/free/pool/flags (layout below) |
| `02` | DIR | Request empty; each reply: name[16], type u8, length u32, CRC u32; END terminates the listing |
| `03` | GET | target u8, instrument u8, name[16] |
| `04` | PUT | target u8, instrument u8, name[16], type u8, length u32, CRC u32 |
| `05` | DATA | 1–256 file bytes; nonfinal upload payloads must be multiples of 16 |
| `06` | END | Empty; finishes a PUT or GET, or terminates DIR |
| `07` | DEL | name[16] |
| `08` | ABORT | Empty; cancels the transfer |
| `7D` | WAIT | Empty, device to host only; the packet whose ACK waits for quiet ([below](#waiting-for-quiet)) |
| `7E` | ACK | Empty; sequence identifies the packet acknowledged |
| `7F` | NAK | reason u8, clean pool sectors u8 |

Targets are 0: named store file, 1: instrument 1–63 of the current song, and
2: current song. Unused instrument/name fields are zero. Files are at most
65,536 bytes. A file's type is 1–127 (1 is a song, 2 a PSID `.sid` tune, below); targets 1
and 2 use type 1.
Target 1 is GoatTracker's GTI5
instrument format, including its four tables; table references are relocated
on import. Target 2 is a canonical GTS5 song including SIDOU's trailer.
Temporary imports use the reserved names `MIDI INSTRUMENT` and `MIDI SONG`;
ordinary file operations cannot use those names, and DIR omits them. Type 1
on target 0 labels a file as a song; it does not validate its contents or load it.

### `.sid` tunes (type 2)

A PUT of type 2 to target 0 is a PSID tune, and SIDOU checks its header before anything of it
is stored, with the parser that plays it ([psid.md](psid.md#what-sidou-refuses-and-why)):

- The PUT is acknowledged without claiming the store's sectors. Its first DATA must hold the
  file's first 128 bytes (a file shorter than that in one DATA); a shorter first DATA is
  refused as malformed (1). A declared length under 118 bytes (`$76`, the smallest header) is
  refused at the PUT (14).
- That DATA's header is checked against the declared length: an RSID file is refused with 12,
  a MUS file (Compute!'s Sidplayer data) with 13, a broken header (not a PSID, a version, data
  offset or song count the file does not hold, data over `$0000-$0001`) with 14, and data that
  runs past `$FFFF` with 15. Nothing is claimed or programmed for a refused tune.
- Then the sectors are claimed as for any PUT, and BUSY (4) or FULL (5, also when the store's
  48 keys are used up) answers this first DATA rather than the PUT. From there it is any PUT:
  each DATA's ACK once it is programmed, END's once the file commits.

The data and the rest of the file are not checked at PUT (a tune that runs wild is stopped when
it plays). Type 2 means a tune only on target 0; a tune stored under any other type is never
offered as one. `sidou-sysex sid` writes a tune's PUT as a `.syx` (below), and
[the manual](../manual/sid-tunes.md) says how to send one.

For PUT, each DATA begins at a 16-byte-aligned file offset. Every nonfinal
payload is 16, 32, …, 256 bytes; the final payload may be any length from 1 to
256 bytes, provided it reaches the exact declared length. An empty file has no
DATA packets. These transfer rules keep the store's page partial-program count
within its limit of 34; they do not restrict the packing codec, which supports
every decoded length from zero to 256, or the store's arbitrary-byte write windows.

INQ's reply layout is protocol u8, firmware version 4×u8, store format u16,
free bytes u32, clean pool sectors u8, flags u8 (13 decoded bytes).
INQ flags bit 0 means the device currently allows windowed storage writes;
other bits are reserved zero. Firmware version bytes are major, minor, patch,
and revision, each 0–127. Free bytes are advisory: fragmentation, pool state
and concurrent writes can prevent a claim even when that number is sufficient.

NAK reasons are 1: malformed/range, 2: checksum, 3: sequence, 4: BUSY,
5: FULL, 6: not found, 7: I/O or invalid content, 8: timeout, 9: aborted, and for a `.sid`
12: RSID, 13: MUS, 14: broken header, 15: data past `$FFFF` (10 and 11 are never sent).
The toasts for 12-15 are `RSID: NOT YET`, `MUS: NOT SUPPORTED`, `BROKEN .SID` and `TOO LONG`.
Errors end the transfer and release its resources after any irrevocable store
commit or already-started object import drains. A USB error produces a NAK; both ports report a toast. Unknown USB
commands are malformed; TRS ignores commands outside its upload-only subset.

## Pacing and transactions

USB is stop-and-wait. The host does not send the next packet until its ACK.
After GET, SIDOU sends PUT metadata, DATA packets, then END; after DIR, DIR
entries then END. The host ACKs each response packet before SIDOU produces the
next, including ACK of the final END. The first response uses the request sequence
and each subsequent response increments it modulo 128. An incoming PUT's DATA and
END similarly increment the metadata sequence. INQ returns INQ metadata; DEL
returns ACK after its deletion commits. Neither reply requires a host ACK.
ABORT is accepted at any sequence from the transfer's owner. Duplicate or
out-of-order DATA packets abort rather than being written twice; retry the
whole transfer after a NAK or a lost ACK.

A sender that gets NAK BUSY (4) is not failing for good: the NAK ended the transfer, so it
waits and sends the whole PUT again from its first packet, whichever packet was refused. The
Songs page waits 1 s, 2 s, 4 s, then 5 s between tries, for about a minute in all, and shows
that it is waiting; any other reason stops the transfer.

The Songs to MIDI page reads with GET: target 2 for the current song, or target 0 and a name from
DIR (type 1) for a stored one. It ACKs every packet, checks the announced length and CRC, and
shows a refusal in words: BUSY (4: a song saving or loading, or an import under way), FULL (5:
the song is too big to read out), not found (6: deleted since the DIR) or I/O (7).

For incoming PUT, the initial packet declares exact length and CRC. SIDOU claims
all store sectors with `store_file_begin` before acknowledging it (a `.sid`'s at its first
DATA, above). If the clean
pool cannot cover the file, it answers BUSY with the current pool size; FULL
means the file cannot fit. No transfer waits for an erase. A record the store is
writing when PUT or DEL arrives (a settings save, or a maintenance collection's
copy, whose programs wait for quiet while keys, a song or MIDI notes sound) does
not refuse it: the transfer owns that record's programs, waits for it to commit
(one short hold, at most two seconds, else BUSY), then claims or deletes. A
collection under way waits for the file and goes on after it. ACK of DATA is delayed
until its bytes have been programmed. END checks length and CRC before the FILE
record may commit; its ACK waits for that commit and any object import acceptance.
An incomplete or CRC-bad upload does not replace an existing file. ABORT before
the END commit cancels replacement; once a commit is irrevocable, abort/reset
cannot roll it back and program ownership remains until it drains.
An object import that has started applying also drains before its model freeze
is released, even after ABORT, lost input or a USB disconnect.

TRS has no reply path. Only PUT/DATA/END/ABORT perform work there. Wait **one
second after EACH complete frame**, including PUT metadata, every DATA and file END,
before sending another frame. Use payloads of at most 256 bytes and the alignment
rules above. This conservative pacing covers the production-default held-program
path with its 256-byte store write size; wire rate alone does not pace storage.
A packet arriving before the previous one is programmed aborts the transfer.
Errors have a toast, never a USB reply for a TRS request. The one-second gap is a
sender requirement, not a hardware-validated throughput measurement.
After a TRS refusal, remaining DATA/END frames are dropped without replacing
the refusal toast. A fresh PUT or ABORT starts a new attempt.
After an instrument or current-song END, allow **15 seconds** for the bounded
import/loader, or wait for the SAVED/refusal toast, before starting another transfer.

A partial frame times out after two seconds without a non-real-time byte. Between
packets a transfer times out two seconds after the last ACK's final USB-MIDI
packet was accepted by the output packetiser (or the
corresponding completed operation on TRS). Time spent programming a packet is not
sender idle time; neither is pending import work or output back-pressure. A flash
failure terminates it separately. The store's five-second
idle backstop excludes storage back-pressure. A running transfer prevents quiet
maintenance erases. With the provisional no-window flash configuration, a PUT
is an explicit write owner, like SAVE: its programs use the existing forced-gap
audio hold/mute/ramp path, not a speculative window. A DATA packet's programs (and
END's FILE record) share one hold, a program a store pass. While a song plays or a
computer records the FM-1's USB audio there is no hold at all: the write waits for
quiet ([below](#waiting-for-quiet)). While only the keys sound (a gated voice, a UI
sound or a button), the hold is released after each packet so audio can ramp back up,
and the next one starts at least 250 ms later (the build option `SIDOU_SYSEX_GAP_MS`,
[BUILDING.md](../../BUILDING.md); a SAVE's forced erases keep 100 ms); while it is
silent (no song, gated voice, UI sound or button, and the output low for 500 ms), the
hold stays on between packets.
Ownership never enables refill erases. DEL
also owns its explicit programs and can use forced holds with windows disabled.
Disabled windows alone do not cause BUSY. With write windows enabled, including
the emulator override, these programs are hold-free. Hold-free hardware operation
depends on the flash timing run in [hardware checklist section 12][flash-timing].

[flash-timing]: ../hardware-test.md#12-flash-timing-spike-m2a-task-3b

### Waiting for quiet

While a song (or a `.sid` tune) plays, or a computer records the FM-1's USB audio, a write
whose programs are due waits for quiet instead of muting the music for them: a DATA
packet's ACK, END's (its FILE record), DEL's, and a PUT's or DEL's that waits for a record
being written (above). On USB the device sends `F0 7D 53 44 7D seq checksum F7` (WAIT, empty)
at once and every two seconds while it waits, `seq` being the packet whose ACK it holds back;
the screen says `UPLOAD WAITS FOR QUIET` as often. A sender that knows WAIT starts its ACK
timeout again at each one (the Songs page says it is waiting for the FM-1 to be quiet). Once
the song stops (or the recording does), the programs run and the ACK follows. A wait has a
limit: after 60 seconds the transfer ends with NAK BUSY (4); a sender should not retry that
BUSY on its own, since the song is still playing. Time spent waiting is not sender idle
time, and the two seconds a PUT or DEL waits for a record do not count it.

Older senders: WAIT is new in 1.1. A sender that drops frames it does not know waits its own
ACK timeout and fails if the song plays on past it; one that treats an unknown frame as an
error fails at the first WAIT. Either way nothing is replaced, and the upload goes through
with the song stopped. A sender on a fixed pause (one second after each message, as the SysEx
tools in [the manual](../manual/sid-tunes.md) send) is refused BUSY while a song plays: its
next message comes while the last still waits. TRS has no reply path: a TRS upload waits with
the toast alone, and its one-second gap does not cover a wait; stop the song first. With
validated write windows, programs need no hold and nothing waits.

Current-song/instrument operations refuse a busy or saving song. A download freezes
model edits until completion/abort. Instrument transfers require a stopped song;
import checks tables and free destination rows before changing it, and preserves
the song on refusal. Table allocation uses contiguous trailing rows; holes in
existing tables are not reused. A valid GTI5 can therefore receive FULL even if
unused rows exist elsewhere. Import also enforces the song's saved-size limit.
Instrument import invalidates the old paused-player analysis; the next PLAY
analyses the new instrument. Song PUT refuses unsaved edits rather than discarding
them. After its private staging file commits, it runs the normal stopped song
loader and waits for its file/content validations. END ACK and SAVED follow
successful loading; a refusal produces NAK on USB and a toast. Staging files do
not take bank-program numbers or appear in the SONG browser; SAVE AS gives an
imported song its user-facing name.
While that private staging file is the loaded song's rollback source, another
current-song PUT returns BUSY with **SAVE AS FIRST** before claiming or writing.
Use SAVE AS with a nonreserved name (or load another saved song) before replacing
it. This keeps an invalid subsequent upload from destroying the recovery source.
There is one packet buffer, no complete dump in RAM, and no second song region.

## Universal inquiry

`F0 7E id 06 01 F7` with id 0 or `7F` receives
`F0 7E 7F 06 02 7D 53 44 01 00 major minor patch revision F7` on USB.
Other IDs and TRS inquiries have no reply. The updater's identity command remains
independent and unchanged. USB replies are host-tested; hardware USB-MIDI output
remains an open acceptance item until its separate transport fix is validated.

## Example

An empty INQ at sequence zero is `F0 7D 53 44 01 00 7F F7`.
The host-only `sidou-sysex` library encodes/decodes packets and packing; it opens
no MIDI or USB device. It validates individual packet shapes,
not transfer sequencing, file CRCs or contents. Its one command,
`sidou-sysex sid FILE.sid... [--out DIR] [--trs]`, writes each tune's whole upload (PUT,
DATA, END, one SysEx message a packet) as a `.syx`, after checking it with SIDOU's own PSID
parser, and names it in the store from its title: uppercase, characters `20`-`5F` (others
`?`), 16 at most, `-2`, `-3`, ... on a clash within the run (HVSC's `<?>` or no title: the
file's name). It reports a refused tune with the device's reason and writes no `.syx` for it.
`--trs` changes no byte (the frames are the same on both ports); it prints TRS's pacing
instead of USB's. Use a SysEx utility that can wait
for USB ACKs (or apply the TRS frame gap) for hardware checks.
