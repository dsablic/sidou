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
| 20 | 4 | the claim's number: the erase count again |
| 24 | 4 | CRC-32 of bytes 0-23 |
| 28 | 4 | claim word, `0x00000000` |

- **The erase header** (bytes 0-19) is programmed right after the sector's
  erase completes. A sector with a valid erase header whose bytes 20 to the
  end all read 0xFF is **clean**: it is in the erase-ahead pool.
- **The claim** (bytes 20-31) is programmed when the sector is taken into
  use, before its first record, in one go with the erase header again: the
  claim programs bytes 0-31, and bytes 0-19 are the same bytes once more, so
  a faint erase header (below) is programmed for good before the sector holds
  anything. The claim's number is the sector's erase count again, so every
  claim of one erase cycle is the same 32 bytes: a claim cut so early that
  the next mount reads the sector clean (every bit it touched left faint), or
  one the flash driver failed, is made again over the same bytes, never over
  different ones. (Regions written before this rule hold a sequence number
  there; either reads the same.)
- **A sector without a valid erase header is dirty, whatever it reads**: an
  erase cut short can leave bytes that read 0xFF and hold no program. So is a
  clean sector with a cut claim: a bad claim CRC, or a claim word that reads
  `0xFFFFFFFF`. A claim word with any bit programmed counts (its CRC checks,
  so the cut came during the word). A dirty sector is erased before any use.
- **Before a sector that may hold a valid header is erased, its magic is
  killed**: the magic's 1 bits that read 1 are programmed to 0, and only
  those, so the kill never programs a 0 over a bit that reads 0. An erase cut
  short can leave any mix of old bytes and 0xFF, the header included; a
  killed magic can never read `SIDOUSTR` again. Bytes 8-19 still check
  against `SIDOUSTR` (the CRC at 16), so the sector's erase count survives the
  kill. A magic with a bit set that `SIDOUSTR` has clear (a blank sector) was
  never valid and is not killed; one whose kill was cut or failed is killed
  again before its erase, over the bits that still read 1.

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
  record** (below). 0x83-0xFE are not records in format 1 (a record of one
  ends its sector's records, as a cut write does); 0xFF is never a type: an
  erased header reads 0xFF.
- Keys 0-0x7FFFFFFF are the caller's; `0x80000000 | n` are files'.
- A small value is at most 1,248 bytes.
- **Order of programming:** the data, then the header, then the commit
  word, each its own program (the commit word always one 4-byte program,
  whatever the write window). All three land on erased bytes. A record counts
  only with a good CRC and its commit word programmed: erased (`0xFFFFFFFF`)
  means the write was cut; any bit of it programmed counts, since the header
  and data were programmed whole before it (the CRC shows it). So a commit
  word with any bit programmed for good reads the same verdict at every
  mount; one all of whose programmed bits are faint may not ("Faint bits and
  the head"). Its sequence number is taken either way: a header whose CRC
  checks moves the next number past its own even while its commit word reads
  erased.
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

## Faint bits and the head

A cut during a program or an erase can leave bits **faint**: neither
programmed nor erased, read one way at one mount and another at the next.
Four rules keep them harmless:

- **Every boot starts a fresh head.** No sector from before the mount takes
  another record: the first record written after a mount claims a new sector
  from the pool, so nothing is ever written after bits the last session's
  last write may have left faint. It costs at most one partly used sector a
  boot, which garbage collection reclaims.
- A commit word or claim word counts with **any** of its bits programmed
  (above), and a claim programs the erase header again (above).
- **The newest record is copied forward.** Every write takes the next
  sequence number (a collection's copy too), one write is in flight at a
  time, and a file's chunks carry the file's id, also a sequence number: so
  the write a cut interrupted is the **newest** record. A commit word all of
  whose programmed bits are faint (likely: its 32 bits are programmed by one
  pulse, so a cut leaves them charged alike) can read erased at one mount and
  programmed at another, so the newest keyed record counts as committed when
  its CRC checks, whatever its commit word reads. Before anything else is
  written, maintenance copies it forward: a new record of the same key and
  bytes, a new number, programmed whole. Whatever the faint bits read later,
  the copy is newer. (A newest record read uncommitted is indexed last at the
  mount, so if the key index is full it is the one left out, counted as
  corruption. If a committed chunk of a file begun after it is the newest,
  the session after it ran, and there is nothing to copy.)
- **Ambiguous sectors are erased before the session writes.** Ambiguous: a
  claimed sector whose records end in a **cut tail** (a header that fails its
  CRC, a record whose CRC checks but reads uncommitted, data after a header
  that reads erased), a sector read as **killed** (a kill cut short can leave
  zeros faint, reading zero now and one later), and a sector torn otherwise
  (an erase or a claim cut short). Faint bits there can read otherwise at a
  later mount: a cut header can come to read valid, a killed sector's records
  back. Only a completed erase settles them. So before the session's first
  write, maintenance erases each ambiguous sector, a claimed one evacuated
  first (its live records copied out with new numbers, as a collection
  does). In order: the copy forward, the dirty ambiguous sectors' erases,
  then the evacuations, so no collection ever drops a tombstone while a
  sector holding an older record of its key could still read otherwise.
  A sector read as killed whose zeros are faint reads killed either way
  until its erase, and nothing relies on its records being gone before that
  erase completes.

Until this **recovery** is done, the store answers `STORE_BUSY` to writes,
files and collections; maintenance runs it first. After a cut it is at most
the copy forward, one sector's evacuation (a few records) and two erases
(the cut sector's, and a collection's killed victim: maintenance erases a
killed victim before anything else, and `store_collect` waits for that
erase); hook errors can add one evacuation each. The device starts on it at
boot, before the audio starts, for at most 100 ms of flash work after the
mount, and goes on in the main loop (an operation a pass, held while quiet),
with USB and the watchdog served between operations.

**A collection's victims** are killed, then erased before any other
maintenance (so a cut never finds more than one waiting).

What is left:

- **A write cut in its header** whose faint bits read wrong at one mount
  (the write never happened, no trace of it if it had no data: a tombstone)
  and right at a later one counts at the later one, as the newest record, if
  no record of the next session completed between the two (a second cut
  before recovery's first copy, or before the first write of a session with
  nothing to recover). Once a record of the next session has completed, it is
  newer, and the cut write reads uncommitted and never counts. It is always
  the write the cut interrupted, never anything else.
- **Sequence numbers.** A mount skips 256 numbers past the newest it read: a
  write cut in its header, unread, may hold one of them (after writes the
  hook failed, and files aborted before a chunk, whose numbers left no record
  that reads). More than 255 such numbers in a row before a cut could let a
  session's record share the cut write's number. The numbers wrap after
  about 16 million boots.
- **Hook errors** (a flash operation the driver reports failed, the part
  having done part of it) leave the write undecided, as a cut does: a later
  mount may read it as made or not, on a reboot with no cut, until
  maintenance has evacuated and erased its sector (it does so first, at the
  next quiet). A failed collection copy holds its original's bytes, so it
  reads the same either way.
- **A worn sector** whose maintenance fails twice in a row (its kill, its
  erase or its erase header) is retired for the session: it leaves the pool,
  recovery and collections, so it never holds back the other sectors or the
  writes. Nothing on the part records it; the next mount finds the sector as
  the part left it and maintenance tries it twice again. While a retired
  sector may still read otherwise (ambiguous), collections keep every
  tombstone.
- **A kill cut short** whose zeros were left faint and read 1 at the next
  mount leaves the sector reading valid (or partly killed); the next kill
  programs those bits again (zeros over faint zeros: the only such program
  besides the claim's).

## Which record wins

- Each key's **newest sequence number** wins; a tombstone as the winner
  means the key is deleted.
- Garbage collection copies a live record into another sector **with a new
  sequence number**, so no two records share one. (Regions written before
  this rule may hold a record and its copy with one number, the same bytes:
  either stands for the key; the mount takes the first found. Two with the same number and
  different CRCs are counted as corruption, `store_stats`.)
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
- **SIDOU's, with another firmware's data in some sectors** (Felucca run
  after SIDOU, saving over part of SIDOU's store): the region is SIDOU's, and
  those sectors read dirty and ambiguous, as any sector without a valid
  header: recovery erases them before the session's first write (in the
  background, above). They stay ambiguous rather than plain dirty because
  nothing tells another firmware's bytes from an erase of SIDOU's own sector
  cut short, whose faint bits could read its old records back later.

## The erase-ahead pool

Clean, unclaimed sectors. `STORE_POOL_MIN` is `ceil(65,536 / 3,808) + 2` =
20: the largest file plus garbage collection's two-sector reserve.
Maintenance refills the pool toward `STORE_POOL_TARGET` (22) when the device
is quiet, erasing dirty sectors first, then collecting. A collection takes
the sector with the least live data (the colder on a tie), but only when it
gains: collecting the least-live sectors one after another copies their
live records, back to back, into the head's room and then into fresh
sectors, and the run must fill fewer sectors than it frees (`plan` in
`store.c`; say three sectors two-thirds live give one). A run that gains
goes on gaining as its sectors are collected, so collections never churn,
and sectors of any fill can go (a fixed "at most half live" rule left each
boot's head, two values of 1,268 bytes, uncollectable, and the store full
10% under its capacity). A write claims its sectors from the pool before its first
byte; a small value's new head sector is refused (`STORE_BUSY`) when it
would take the pool under its minimum, and a file's sectors, with the place
for its FILE record, when they would take it into garbage collection's
reserve. While a file is open, maintenance waits (an open file that takes no
byte for 5 s is aborted, `store_file_idle`). Sectors leave the pool, and are
erased, lowest erase count first; when the counts spread by more than 4,
maintenance moves the coldest sector's records so it cycles too.

## Partial programs and ECC

Between two erases of its sector, a 256-byte page takes at most
**`STORE_PAGE_PROGS` = 34 programs** for a write window of 16 bytes or more
(the device's windows are 16, 64 or 256 bytes): record data in window-sized
pieces, record headers and commit words, each its own program (the densest
are 4-byte values: 3 programs in 24 bytes), and on a sector's first page its
erase header, its claim and the kill: 34, the maximum over every sequence
of records (`STORE_PAGE_PROGS` in `app/store.h`; the host tests count every
page's programs). Nothing adds programs at a boot or in recovery (the
interrupted write is copied, not programmed again). Only a cut can: a claim
cut so that the next mount reads it never made is made again, and a kill cut
short is made again, one more program in that page, at most one for each
cut that falls there (and one for each kill the flash driver fails).

Two kinds of program touch bytes already programmed. The claim programs
the erase header's bytes again, and a claim read as never made programs its
own bytes again (the same bytes: zeros over zeros, which settles bits a cut
left faint). The kill programs the magic's 1 bits to 0, never a bit already
0 (bits a cut kill left faint and reading 1 are programmed again by the
next kill). A part that limits partial programs per page
below 34, or forbids programming bits already programmed, needs a change:
for zeros over zeros, the claim would program bytes 20-31 only, and every
clean sector a mount finds would be erased again before its first claim (at
most 22 erases a boot). A part with **ECC**, whose code words cover fixed
units (often 16 bytes) that may be programmed only once, would need records
as 16-byte single-shot units, a rework of this format. Whether the FM-1's
part has any of these is an open check ([hardware.md](../hardware.md), "Open
checks on a real unit").

## Wear

Sectors leave the pool and are erased lowest erase count first, and when
the erase counts spread by more than 4, maintenance moves the coldest
sector's records. A live file's sectors never move, so files stored take
their sectors out of the rotation. At about 100,000 erases a sector:

| Case | Erases a sector a day | Years to 100,000 |
|---|---|---|
| A 20 KB song (6 sectors) saved every 2 min for 4 h a day, one 64 KB song stored besides (55 sectors rotate): 720 erases a day | ~13 | ~21 |
| A 64 KB autosave (18 sectors) every minute for 8 h a day, two 64 KB songs stored besides (37 sectors rotate): 8,640 erases a day | ~233 | ~1.2 |

So large files are never autosaved by the minute, and a save of a file that
has not changed since its last save is skipped (M2c Task 8). Small values
are cheap: a settings save a minute for ten years is under 500 erases a
sector.

## Stats

`store_stats.free` is `STORE_CAPACITY` less the live records. It can
overstate what fits: live records scattered over many sectors hold sectors
whose rest only a collection frees, and a file needs whole sectors.
`store_stats.keys` counts tombstones too: a deleted key keeps its slot in
the 48-key index until garbage collection drops its tombstone (once no older
record of it survives elsewhere), so a store with a full index and
tombstones refuses new keys (`STORE_FULL`) until collections run.

## A cut during a blank region's first erase

A blank region formats lazily: maintenance erases its first sectors as the
pool fills. A part's sector erase may program the sector to 0 before it
erases it; a cut then can leave a sector that is neither blank nor SIDOU's
(no header of any kind), and the region mounts **foreign** on a unit that
never held anything. As for any foreign region, nothing is written until the
user confirms a format; M2a Task 5's format screen must expect it.

## What the firmware keeps in it

| Key | Type | Value |
|---|---|---|
| `0x100` | 1 | the [settings record](settings-format.md) |
| `0x101`..`0x108` | 2 | the eight [USER chip profiles](profile-format.md) |
| `0x80000000 \| slot` | `0x82` | a file (a song, M2c) |

Keys below `0x100` belong to the emulator tests (`platform/fm1/store_fm1.c`'s hook).

## The test fixture

`host/crates/sidou-tests/tests/data/store-v1.bin` is a format 1 region,
checked in, that every later version must mount to the same values and
files. On disk
it is the region's runs of bytes other than 0xFF: each `[u32 flash offset]
[u16 length][bytes]`; the rest of the region is 0xFF.
