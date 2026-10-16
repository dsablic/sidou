<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The store's format on flash

SIDOU keeps its settings, MIDI learn's map, user chip profiles and files in a
log-structured store in the FM-1's flash: `[0x97000, 0xE0000)`, 73 sectors
of 4 KiB ([hardware.md](../hardware.md), "Flash map"). This page is the
format byte for byte, for a reader of a flash dump. The code is
`app/store.h` and `app/store.c`; the tests are
`host/crates/sidou-tests/tests/app/store.rs`.

All numbers are little-endian. CRC-32 is IEEE 802.3's, reflected, as zlib's
`crc32` (`store_crc32`).

## Magic numbers

| Bytes | Where | What |
|---|---|---|
| `SIDOUSTR` | offset 0 of a store sector | the sector's erase header (this page) |
| `SIDOU-X\0` | after the last pattern of a `.sng` file | a song file's SIDOU trailer (`docs/reference/song-format.md`, M2c Task 1) |

## A sector

```
0x000  header (32 bytes)
0x020  records, back to back, each 4-byte aligned
...
0xF00  guard band: never programmed (0xFF), to the sector's end
```

The guard band is the sector's last page: an update record the SPL or the
update loader could take is 80 bytes at a sector's `0xF00`, and an erased
band never matches its magic ([hardware.md](../hardware.md), "Flash map").
Records take at most `0xEE0` (3,808) bytes a sector.

### The header

Written in two steps.

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | magic `SIDOUSTR` |
| 8 | 2 | format version (`STORE_FORMAT`, 1) |
| 10 | 2 | zero |
| 12 | 4 | the sector's erase count |
| 16 | 4 | CRC-32 of bytes 0-15 |
| 20 | 4 | the sector's sequence number |
| 24 | 4 | CRC-32 of bytes 0-23 |
| 28 | 4 | claim word, `0x00000000` |

- **The erase header** (bytes 0-19) is programmed right after the sector's
  erase completes. A sector with a valid erase header whose bytes 20 to the
  end all read 0xFF is **clean**: it is in the erase-ahead pool.
- **The claim** (bytes 20-31) is programmed when the sector is taken into
  use, before its first record. Sequence numbers come from one counter,
  shared with the records; the newest is the highest.
- **A sector without a valid erase header is dirty, whatever it reads**: an
  erase cut short can leave bytes that read 0xFF and hold no program. So is a
  clean sector with a cut claim (a bad claim CRC, or a claim word not 0).
  A dirty sector is erased before any use.
- **Before a sector with a valid header is erased, its magic is programmed
  to zero** (the kill). An erase cut short can leave any mix of old bytes and
  0xFF, the header included; a killed magic can never read `SIDOUSTR` again.
  Bytes 8-19 still check against `SIDOUSTR` (the CRC at 16), so the sector's
  erase count survives the kill. This is the one program over bytes already
  programmed: it only clears bits.

### A record

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | type |
| 1 | 1 | flags (0) |
| 2 | 2 | data length |
| 4 | 4 | key |
| 8 | 4 | sequence number |
| 12 | 4 | CRC-32 of bytes 0-11, then of the data |
| 16 | len | data, then 0xFF up to a multiple of 4 |
| 16 + len rounded up to 4 | 4 | commit word, `0x00000000` |

- Types 0x00-0x7F are the caller's (a small value); 0x80 is a **tombstone**
  (the key was deleted, no data); 0x81 a file's **chunk**, 0x82 a **FILE
  record** (below). 0xFF is never a type: an erased header reads 0xFF.
- Keys 0-0x7FFFFFFF are the caller's; `0x80000000 | n` are files'.
- A small value is at most 1,248 bytes.
- **Order of programming:** the data, then the header, then the commit
  word. All three land on erased bytes. A record counts only with a good CRC
  and its commit word programmed; erased (`0xFFFFFFFF`) means the write was
  cut.
- **The end of a sector's records** is the first header that reads all
  0xFF, if the rest of the sector's record area is 0xFF too. Anything else
  after the last valid record (a cut write: data under an erased header, a
  bad CRC, no commit word) makes the sector full: nothing more is ever
  programmed into it.

## Files

A file (a song, a dump) is a name of 1 to 16 characters (0x20-0x5F, the
screen font's upper case, not starting with a space), a type (1 is a
GoatTracker 2 song) and up to 65,536 bytes. It is written as **chunk
records**, then committed by one **FILE record**.

**A chunk record** (type 0x81): key = the file's id, sequence number = the
chunk's index (from 0), data = 1,248 bytes of the file (the last chunk
shorter). A chunk record takes 16 + 1,248 + 4 = 1,268 bytes, so three fill a
sector's records (3,804 of 3,808 bytes): a sector carries 3,744 bytes of a
file and a 64 KiB file takes 18 sectors. A file's chunks go only into the
sectors claimed for it, at fixed places: chunk `i` is in the file's sector
`i / 3`, at offset `0x20 + (i % 3) × 1268`.

**A FILE record** (type 0x82), in a head sector: key = the file's key
(`0x80000000 | n`, which stays the file's across replacements and renames),
data:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | the file's id: its chunks' key (a sequence number, new for every write) |
| 4 | 4 | length in bytes |
| 8 | 4 | CRC-32 of the file's bytes |
| 12 | 1 | type |
| 13 | 1 | sector count, `ceil(length / 3744)` |
| 14 | 2 | chunk count, `ceil(length / 1248)` |
| 16 | 16 | name, NUL-padded |
| 32 | sector count | the sectors, in order, as indexes from the store's first (0 = 0x97000) |

- **A file without its FILE record does not exist.** Its chunks are written
  first, then the FILE record commits it whole. Replacing a file writes the
  new one completely, then its FILE record (the same key, a newer sequence
  number) supersedes the old one; the old chunks become garbage. Renaming
  writes the FILE record again with the new name; deleting writes a
  tombstone on its key. So a cut at any point leaves the old file or the new.
- **Chunks never move:** a live file's sectors hold nothing else and are
  never collected; they become garbage, whole, when the file is replaced or
  deleted. A read goes straight to a chunk through XIP.
- The mount checks a FILE record against its chunks (each where it says,
  with the file's id and its index, inside its sector's valid records); a
  file that does not check is dropped and counted as corruption.

## Which record wins

- Each key's **newest sequence number** wins; a tombstone as the winner
  means the key is deleted.
- Garbage collection copies a live record into another sector **with its
  sequence number unchanged**. So two valid records of a key can share a
  number: a record and its copy, which hold the same bytes. Of the two, the
  one in the sector with the higher sector sequence number wins. Two with the
  same number and different bytes cannot arise; the mount counts it as
  corruption (`store_stats`) and takes the same choice.
- **Tombstones are carried forward** by garbage collection while any older
  record of their key survives in another sector; otherwise they are
  dropped with their sector.

## The region

- **Blank:** every byte 0xFF. The store formats lazily: maintenance erases
  sectors and writes their erase headers as the pool fills.
- **SIDOU's:** at least one SIDOU header (valid, killed, or an erase header
  whose program was cut). An older `STORE_FORMAT` would be read and migrated
  in RAM; a newer one makes the region foreign.
- **Foreign:** neither: another firmware's data, such as Felucca's projects.
  It is never formatted silently: the store writes nothing until the user
  confirms a format.

## The erase-ahead pool

Clean, unclaimed sectors. `STORE_POOL_MIN` is `ceil(65,536 / 3,808) + 2` =
20: the largest file plus garbage collection's two-sector reserve.
Maintenance refills the pool toward `STORE_POOL_TARGET` (22) when the device
is quiet, erasing dirty sectors first, then collecting the sector with the
least live data. A write claims its sectors from the pool before its first
byte; a small value's new head sector is refused (`STORE_BUSY`) when it
would take the pool under its minimum, and a file's sectors, with the place
for its FILE record, when they would take it into garbage collection's
reserve. While a file is open, maintenance waits. Sectors leave the pool, and are
erased, lowest erase count first; when the counts spread by more than 4,
maintenance moves the coldest sector's records so it cycles too.

## The test fixture

`host/crates/sidou-tests/tests/data/store-v1.bin` is a format 1 region,
checked in, that every later version must mount to the same values and
files. On disk
it is the region's runs of bytes other than 0xFF: each `[u32 flash offset]
[u16 length][bytes]`; the rest of the region is 0xFF.
