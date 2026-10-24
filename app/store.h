// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's store: a log-structured, power-loss-safe key-value store in the flash region of
 * app/store_layout.h, for the settings, MIDI learn's map, user chip profiles and
 * files. The format is docs/reference/store-format.md, byte for byte.
 *
 * Portable C11, no allocation, no hardware: the flash is reached only through store_flash,
 * which the caller supplies (platform/fm1 on the device, a NOR simulator in the host tests).
 * Reads are free (map: a pointer into flash). Programs and erases are operations the core
 * drives from store_poll, at most one a call, never waiting and never retrying inside a call:
 * a hook may answer STORE_LATER (not now: the device has no write window open) and the core
 * keeps the operation for a later call.
 *
 * How it stays safe at any power cut:
 * - every sector carries a header: an erase header (magic, format, erase count, CRC),
 *   programmed right after the erase, then a claim (its number: the erase count, a CRC and
 *   a claim word), programmed when the sector is taken into use, with the erase header again
 *   (the same bytes). A sector without a valid erase header is dirty whatever it reads, and is
 *   erased before any use;
 * - a record is programmed data first, then its header, then its commit word; it counts only
 *   with a good CRC and the commit word programmed (any of its bits: see faint bits below).
 *   Anything after a sector's last valid record makes the sector full: nothing is ever
 *   programmed over bytes not erased, but the kill and the claim's erase header;
 * - every record carries a sequence number; the newest wins. Each write takes the next one,
 *   a copy made by garbage collection too, and one write is in flight at a time, so the write
 *   a cut interrupted is always the newest record;
 * - before a sector with a valid header is erased, its magic is killed: its 1 bits that read
 *   1 are programmed to 0 (only those: never a 0 over a bit that reads 0). An erase cut short can leave any mix
 *   of old bytes and 0xFF, header included, and a killed sector never counts again;
 * - a deletion is a tombstone record, carried forward by garbage collection while an older
 *   record of its key survives in another sector;
 * - faint bits: a cut during a program or an erase can leave bits neither programmed nor
 *   erased, read one way at one mount and another at the next. Four rules:
 *   every boot starts a fresh head (no sector from before the mount takes another record);
 *   a commit or claim word counts with any of its bits programmed, and a claim programs the
 *   erase header again (the same bytes);
 *   the newest record counts when its CRC checks, whatever its commit word reads (a cut in
 *   the commit word can leave all its bits faint), and is copied forward (a new number,
 *   programmed whole) before the session writes anything else;
 *   ambiguous sectors are erased before the session writes: a cut tail (a bad header, a record
 *   whose CRC checks but reads uncommitted, data after an erased header), a sector read as
 *   killed, a torn erase or claim. A claimed one is evacuated first (its live records copied
 *   with new numbers). Only a completed erase settles faint bits, and no collection drops a
 *   tombstone while such a sector could still read otherwise. Until this recovery is done
 *   (maintenance runs it, first), writes and collections answer STORE_BUSY: at most the copy
 *   forward, one sector's evacuation and two erases after a cut (more after hook errors).
 *   What is left: a write cut in its header can read as never made at the next mount and as
 *   made (and count) at a later one, if no record of the next session completes between the
 *   two (docs/reference/store-format.md, "Faint bits and the head");
 * - a hook error leaves its write undecided, as a cut does: a later mount may read it either
 *   way until maintenance has evacuated and erased its sector, so a value can change across a
 *   reboot with no cut (store_stats.errors counts the errors);
 * - a sector whose maintenance fails twice in a row (its kill, erase or erase header: a worn
 *   part) is retired for the session (store_stats.retired), so it never holds back the other
 *   erases, the collections or the writes; while one that may still read otherwise is retired,
 *   collections keep every tombstone. The next mount finds it as the part left it.
 *
 * The erase-ahead pool: clean, unclaimed sectors. Whoever writes claims its sectors from the
 * pool before the first byte, so no write ever waits for an erase: a small record's new head
 * sector (refused with STORE_BUSY if that would take the pool under STORE_POOL_MIN), a file's
 * sectors (store_file_begin). Erases and garbage collection run only as maintenance, when the
 * caller says the device is quiet (store_quiet), and refill the pool toward STORE_POOL_TARGET.
 * A collection takes the least-live sector, whatever its fill, when collecting the least-live
 * sectors one after another would fill fewer sectors with their copies than it frees.
 *
 * Wear: sectors are taken from the pool and erased lowest erase count first, and when the
 * erase counts spread more than STORE_WEAR_SPREAD, maintenance moves the coldest claimed
 * sector's live records so it cycles too; a live file's sectors never move, so stored files
 * take their sectors out of the rotation. NOR parts endure about 100k erases a sector. A
 * settings save a minute for ten years is about 5.3M record writes; at about 200 records a
 * sector (records of ~16 bytes of data) that is 26k sector fills, spread over the 53 sectors
 * outside the pool's minimum (the pool's sectors rotate with the rest): under 500 erases a
 * sector. Files wear far more, a save erasing as many sectors as the file has:
 *   a 20 KB song (6 sectors) saved every 2 min for 4 h a day, one 64 KB song stored besides
 *   (55 sectors rotate): 720 erases a day, ~13 a sector, ~21 years to 100k;
 *   a 64 KB autosave (18 sectors) every minute for 8 h a day, two 64 KB songs stored besides
 *   (37 sectors rotate): 8,640 a day, ~233 a sector, ~1.2 years.
 * So large files are never autosaved by the minute, and a save of a file that has not changed
 * since its last save is skipped (M2c Task 8). Each boot's fresh head adds at most one sector
 * fill a boot.
 *
 * Partial programs: a page takes at most STORE_PAGE_PROGS programs between erases. Bytes already
 * programmed are programmed again only by a claim (the erase header's same bytes, and a claim's
 * same bytes again after a cut left it reading never made) and by a kill after a kill cut short
 * or failed, over the bits it left faint and reading 1 (a kill programs only the magic's bits
 * that read 1). A part with ECC would need 16-byte units each
 * programmed once (a format rework); docs/hardware.md's open checks ask the spike (M2a Task 3b).
 *
 * Work outside store_poll: store_put, store_delete, store_file_begin, store_file_rename,
 * store_collect and store_erase_wanted each do at most STORE_POLL_WORK (store_put's data CRC
 * included, or garbage collection's plan when the pool is short; the host tests check every
 * call); store_get, store_file_open, store_file_read and store_dir_next read and compare (a walk
 * of the key index, 48 names at most).
 *
 * Files (songs, dumps): a name, a type and up to STORE_FILE_MAX bytes, written as chunk
 * records of STORE_CHUNK bytes, three to a sector, into sectors claimed for the file alone, then
 * committed whole by one FILE record in the head. store_file_begin claims every sector the file
 * needs, and the FILE record's place, before it returns, so a file never waits for an erase.
 * Until its FILE record commits a file does not exist: replacing one leaves the old until the
 * new commits, and a cut at any point leaves the old file or the new. Chunks never move: a live
 * file's sectors are never collected, and its FILE record lists them, so a read goes straight
 * to a chunk through XIP. One file is written at a time; small writes go on meanwhile while the
 * head sector has room, and maintenance waits until the file is committed, aborted or timed out
 * (store_file_idle).
 *
 * RAM: the store struct, under 2 KB (a sector index, a key index, a 256-byte buffer that also
 * stages a file's bytes, the file being written). */
#ifndef SIDOU_STORE_H
#define SIDOU_STORE_H

#include <stdint.h>

#include "store_layout.h"

#define STORE_FORMAT 1u  /* the format version in every erase header */
#define STORE_HEADER 32u /* a sector's header; records from here */
#define STORE_RECORDS (STORE_SECTOR_USE - STORE_HEADER) /* 0xEE0: 3,808 bytes of records */
#define STORE_REC_HEADER 16u
#define STORE_REC_OVERHEAD 20u     /* the header and the commit word */
#define STORE_VALUE_MAX 1248u      /* the largest small value (as a file chunk's data) */
#define STORE_KEYS_MAX 48u         /* small values and files together */
#define STORE_KEY_MAX 0x7FFFFFFFu  /* the largest caller key; keys above are the store's */
#define STORE_TYPE_MAX 0x7Fu       /* the largest caller type; types above are the store's */
#define STORE_TYPE_TOMB 0x80u      /* a tombstone: the key was deleted */
#define STORE_TYPE_CHUNK 0x81u     /* a file's chunk: key the file's id, seq the chunk's index */
#define STORE_TYPE_FILE 0x82u      /* a file's FILE record, which commits it */
#define STORE_KEY_FILE 0x80000000u /* files' keys: STORE_KEY_FILE | a slot */
#define STORE_NAME_MAX 16u         /* a file name's characters */
#define STORE_FILE_SECTORS 18u     /* the sectors of the largest file */
#define STORE_FILE_HEAD 32u        /* a FILE record's data before its sector list */
#define STORE_FILE_SONG 1u         /* file types: 1..STORE_TYPE_MAX; a GoatTracker 2 song (M2c) */
#define STORE_FILE_SID 2u          /* a PSID .sid tune (M3) */
#define STORE_FILE_GTI 3u          /* SysEx's GTI5 instrument staging (MIDI INSTRUMENT, private) */
#define STORE_WINDOW_MAX 256u      /* the most one program writes: a page */
#define STORE_FILE_MAX 65536u      /* the largest file */
#define STORE_CHUNK 1248u          /* a file chunk's data: three chunk records fill a sector */
#define STORE_FILE_SECTOR_DATA (3u * STORE_CHUNK) /* 3,744 bytes of a file a sector */
#define STORE_GC_RESERVE 2u /* the pool's last sectors, which only garbage collection takes */
/* Enough for the largest file and the garbage collector's reserve. */
#define STORE_POOL_MIN ((STORE_FILE_MAX + STORE_RECORDS - 1u) / STORE_RECORDS + STORE_GC_RESERVE)
#define STORE_POOL_TARGET (STORE_POOL_MIN + 2u)
/* The most live data the store takes (records with their headers): the sectors outside the
 * pool's minimum at a file's packing. 198,432 bytes. */
#define STORE_CAPACITY ((STORE_SECTORS - STORE_POOL_MIN) * STORE_FILE_SECTOR_DATA)
#define STORE_WEAR_SPREAD 4u    /* erase counts further apart than this: move the coldest */
/* The bytes one store_poll checks (reads, CRCs, compares) while mounting, or skips of a
 * collection's victim: a record whose CRC is more is checked alone in its poll. */
#define STORE_MOUNT_SLICE 512u
/* The most work one store_poll does (store.work: units of about 4 instructions or fewer each,
 * store.c's WORK_*): a record's CRC alone, a walk of a sector's records, a slice and garbage
 * collection's plan. The host proxy for Task 4's STORE_POLL_WORST (30,000 instructions):
 * 7,168 x 4 is 28,672; Task 4 measures the instructions a unit of each kind of pass. */
#define STORE_POLL_WORK 7168u
/* The most programs one 256-byte page takes between two erases of its sector (each a partial
 * program of the page), for a window of 16 bytes or more: records' data pieces, headers and
 * commit words (densest: 4-byte values, 3 programs in 24 bytes), and on a sector's first page
 * its erase header, its claim (which programs the erase header again, the same bytes) and the
 * kill: 34, the maximum over every sequence of records. Nothing adds programs at a boot or a
 * recovery (a recovered write is copied, not programmed again); only a cut does, at most one
 * program to the page it falls in (a claim or kill cut short is made again), and a hook error
 * on a kill (made again). The host tests check it. A smaller window takes more (one program a
 * byte at 1). */
#define STORE_PAGE_PROGS 34u
#define STORE_FILE_IDLE_MS 5000u /* an open file's sender gone this long: the file is aborted */

_Static_assert(STORE_POOL_MIN == 20u, "ceil(65,536 / 3,808) + 2");
_Static_assert((STORE_FILE_MAX + STORE_FILE_SECTOR_DATA - 1u) / STORE_FILE_SECTOR_DATA +
                       STORE_GC_RESERVE <=
                   STORE_POOL_MIN,
               "the pool covers the largest file at the chunks' real packing");
_Static_assert(3u * (STORE_CHUNK + STORE_REC_OVERHEAD) <= STORE_RECORDS, "3 chunks a sector");
_Static_assert(STORE_VALUE_MAX + STORE_REC_OVERHEAD <= STORE_RECORDS, "a value fits a sector");
_Static_assert(STORE_FILE_SECTORS* STORE_FILE_SECTOR_DATA >= STORE_FILE_MAX, "the largest file");

/* Results. A hook returns STORE_OK, STORE_LATER or an error (any other value). */
enum {
    STORE_OK = 0,
    STORE_LATER = 1, /* a hook: not now, ask again on a later store_poll */
    STORE_BUSY = -1, /* not now: the pool is short (it refills when quiet), or a write runs */
    STORE_FULL = -2, /* it does not fit, even after garbage collection */
    STORE_NOT_FOUND = -3,
    STORE_FOREIGN = -4, /* the region holds another firmware's data: nothing is written */
    STORE_INVALID = -5, /* a bad argument */
    /* A flash operation failed; from a write's start, the store was never mounted (the platform
     * leaves it so on a flash part it does not know): nothing is written, for good. */
    STORE_ERROR = -6,
    STORE_EXISTS = -7,  /* a file of that name exists */
    STORE_TIMEOUT = -8, /* an open file took no bytes for STORE_FILE_IDLE_MS: aborted */
};

/* The region, as the mount found it. */
typedef enum {
    STORE_STATE_UNMOUNTED,
    STORE_STATE_MOUNTING,
    STORE_STATE_BLANK,   /* all 0xFF and no headers: formats lazily as the pool fills */
    STORE_STATE_SIDOU,   /* SIDOU's headers */
    STORE_STATE_FOREIGN, /* not blank, no SIDOU headers: never formatted silently */
} store_state_id;

/* The flash, as the caller supplies it. Offsets are the flash's own (STORE_LO..STORE_HI). */
typedef struct {
    const uint8_t* (*map)(void* ctx, uint32_t off); /* read: a pointer into flash */
    /* Programs n bytes (1..STORE_WINDOW_MAX) from RAM, within one 256-byte page. */
    int (*prog)(void* ctx, uint32_t off, const void* p, uint32_t n);
    int (*erase)(void* ctx, uint32_t sector_off); /* one 4 KiB sector */
    void* ctx;
} store_flash;

/* A sector as the store sees it (store.c keeps the state). */
typedef struct {
    uint32_t erases; /* also its claim's number, as the header has it */
    uint16_t free; /* the end of its valid records: where the next one goes */
    uint16_t live; /* bytes of live records in it */
} store_sector;

typedef struct {
    uint32_t key;
    uint16_t off; /* its newest record: sector and offset */
    uint8_t sec;
    uint8_t pad;
    uint16_t stale; /* its other valid records, in claimed sectors (older, or copies) */
    uint16_t pad2;
} store_key;

/* The record being programmed, one operation a store_poll. */
typedef struct {
    const uint8_t* src; /* the data in RAM, or NULL: copied from flash at `from` */
    uint32_t from;
    uint32_t key;
    uint32_t seq;
    uint32_t crc;
    uint16_t len;
    uint16_t off;      /* where it goes */
    uint16_t done;     /* data bytes programmed */
    uint16_t from_off; /* a copy's original, for the key index */
    uint8_t sec;
    uint8_t type;
    uint8_t phase;
    uint8_t copy; /* 1: garbage collection's copy */
    uint8_t from_sec;
    uint8_t pad[3];
} store_writer;

/* The file being written. */
typedef struct {
    uint32_t key;
    uint32_t fid; /* its chunks' key */
    uint32_t length;
    uint32_t taken; /* bytes accepted from the caller */
    uint32_t crc;   /* of the bytes programmed so far */
    uint32_t rcrc;  /* the chunk record's CRC so far */
    uint16_t chunk; /* the chunk being programmed */
    uint16_t cdone; /* its data bytes programmed */
    uint16_t pdone; /* bytes of a header or claim being programmed */
    uint16_t fill;  /* bytes waiting in buf, a ring ... */
    uint16_t start; /* ... from here */
    uint8_t phase;
    uint8_t closing; /* store_file_end was called */
    uint8_t type;
    uint8_t nsec;
    int8_t result; /* the last file's: STORE_OK, STORE_BUSY while open, an error */
    char name[STORE_NAME_MAX];
    uint8_t sectors[STORE_FILE_SECTORS];
    uint8_t flush;
    uint16_t idle; /* ms without a byte (store_file_idle) */
} store_file_writer;

typedef struct {
    store_flash flash;
    uint32_t seq;     /* the next sequence number (records, files' ids) */
    uint32_t live;    /* bytes of live records */
    uint32_t corrupt; /* inconsistencies the mount saw */
    uint32_t errors;  /* flash operations that failed */
    uint32_t window;  /* the most bytes one program writes */
    store_sector sector[STORE_SECTORS];
    uint8_t sstate[STORE_SECTORS];
    uint8_t state; /* store_state_id */
    uint8_t head;  /* the sector small records go to, or 0xFF */
    uint8_t quiet;
    uint8_t nkeys;
    store_key key[STORE_KEYS_MAX];
    store_writer w;
    store_file_writer f;
    uint16_t mount_off; /* the mount's place: a sector and an offset in it */
    uint8_t mount_sec;
    uint8_t mount_sidou;
    uint8_t mount_blank;
    uint8_t mount_newer;
    uint8_t job; /* maintenance: none, an erase, a collection */
    uint8_t job_sec;
    uint8_t job_phase;
    uint8_t job_done; /* bytes of the header being programmed */
    uint16_t job_off;
    uint16_t fres_bytes; /* the room kept for the open file's FILE record ... */
    uint8_t fres_sec;    /* ... in this sector, or 0xFF */
    uint8_t mount_phase; /* what the mount checks next in mount_sec */
    uint32_t fix_seq;    /* the newest record at the mount (store.c newest): its number, ... */
    uint16_t fix_off;    /* ... its place ... */
    uint8_t fix_sec;
    uint8_t fix_kind;    /* ... and whether it is still to copy forward (FIX_*) */
    uint8_t rec;         /* 1 while the mount's recovery has work: the session waits for it */
    uint8_t retired;     /* sectors retired this session (store.c retire) */
    uint8_t unsure;      /* 1 if one of them may still read otherwise: tombstones are kept */
    uint8_t file_wait;   /* a file waits to begin (store_file_wait): maintenance takes no step */
    uint32_t work;       /* the last store_poll's work: bytes read, and 4 per key compared */
    uint8_t buf[STORE_WINDOW_MAX]; /* a collection's copy, the open file's bytes, a rename */
} store;

typedef struct {
    /* Bytes that still fit: STORE_CAPACITY less the live records. It can overstate: live records
     * scattered over many sectors (fragmentation) hold sectors whose rest only a collection
     * frees, and a file needs whole sectors. */
    uint32_t free;
    uint32_t live;
    uint32_t pool;  /* clean sectors ready to claim */
    uint32_t dirty; /* sectors waiting for an erase */
    uint32_t claimed;
    uint32_t erases_min;
    uint32_t erases_max;
    uint32_t keys; /* indexed keys: a deleted key's tombstone holds its slot until collected */
    uint32_t corrupt;
    uint32_t errors;
    uint32_t retired; /* sectors maintenance failed on twice in a row, out of use this session */
} store_stats;

/* `window`: the most bytes a program may write (1..STORE_WINDOW_MAX; the device's write
 * window, M2a Task 4), but a 4-byte commit word is always one program, so a window under 4
 * still programs 4 bytes then. The store is unmounted until store_mount: reads find nothing,
 * and writes answer STORE_ERROR. */
void store_init(store* s, const store_flash* flash, uint32_t window);
/* Starts the mount; store_poll does it in slices. Writes wait (STORE_BUSY) for recovery, which
 * maintenance runs first: the newest record copied forward, and after a cut a sector or two
 * evacuated and erased. The first small write starts a fresh head sector, so it needs the pool
 * over STORE_POOL_MIN. So fill the pool at boot (quiet maintenance until store_erase_wanted is
 * 0): it absorbs both. */
void store_mount(store* s);
store_state_id store_state(const store* s);
/* Advances the mount, the record being written or maintenance, at most one flash operation.
 * Returns 1 while it has work (store_busy), 0 when idle. */
int store_poll(store* s);
/* 1 while a mount or a write runs (a put or delete not yet committed, a collection's copy). */
int store_busy(const store* s);
/* The caller says the device is quiet: maintenance (erases, garbage collection, the pool's
 * refill) may start. Maintenance already started continues either way. */
void store_quiet(store* s, int quiet);
/* 1 while maintenance has work: a sector to erase, a collection to make or run. */
int store_erase_wanted(const store* s);

/* The newest value of `key`: copies up to `cap` bytes into `buf` and returns its length (which
 * may be more than cap), or STORE_NOT_FOUND. `type`, if not NULL, gets its type. */
int store_get(const store* s, uint32_t key, void* buf, uint32_t cap, uint8_t* type);
/* Writes `len` bytes (up to STORE_VALUE_MAX) as `key`'s new value, of `type` (0..STORE_TYPE_MAX).
 * The data stays the caller's until store_busy is 0: store_poll programs it from there, a piece
 * at a time through a RAM copy (the device programs with XIP off, so no program reads flash:
 * the data may be anywhere, flash included). The place is taken before this returns. STORE_OK: it commits on later store_polls (a power cut
 * before then leaves the old value). STORE_BUSY: not now (a write runs, a new head sector would
 * take the pool under its minimum, or the mount's recovery is not done); STORE_FULL: not even
 * after garbage collection. */
int store_put(store* s, uint32_t key, uint8_t type, const void* data, uint32_t len);
/* Deletes `key` (a tombstone). STORE_OK also when it does not exist. The tombstone keeps the
 * key's slot in the index (STORE_KEYS_MAX) until garbage collection drops it, once no older
 * record of the key survives elsewhere. */
int store_delete(store* s, uint32_t key);
/* Asks for a garbage collection of the best victim now, quiet or not (its erase still waits for
 * quiet). STORE_NOT_FOUND when no sector would gain, STORE_BUSY while maintenance runs, before
 * the mount's recovery, or while a killed sector waits for its erase. */
int store_collect(store* s);
/* Every sector dirty, all 73 to be erased by maintenance (when quiet) before anything is
 * written (writes answer STORE_BUSY until then): for a region another firmware left
 * (STORE_STATE_FOREIGN), after the user confirmed. Loses everything. */
void store_format(store* s);
void store_stats_get(const store* s, store_stats* out);

/* A committed file, as store_file_open and store_dir_next give it. */
typedef struct {
    uint32_t key;
    uint32_t fid;
    uint32_t length;
    uint32_t crc; /* CRC-32 of its bytes */
    uint16_t chunks;
    uint8_t type;
    uint8_t nsec;
    char name[STORE_NAME_MAX + 1]; /* NUL-terminated */
    uint8_t sectors[STORE_FILE_SECTORS];
} store_file;

/* Starts writing a file of exactly `length` bytes (0..STORE_FILE_MAX) named `name` (1 to 16
 * characters 0x20..0x5F, the font's upper case, not starting with a space), of `type`
 * (1..STORE_TYPE_MAX). Claims all its sectors and its FILE record's place before it returns,
 * writing nothing yet. A file of that name is replaced when this one commits. STORE_BUSY: not now
 * (the pool, less garbage collection's reserve, cannot cover it until a quiet refill, or a record
 * is being written, or a file is open, or the mount's recovery is not done); STORE_FULL: not even
 * after garbage collection; STORE_INVALID: a bad name, type or length. A collection under way
 * waits for the file (between its copies) and goes on after it. */
int store_file_begin(store* s, const char* name, uint8_t type, uint32_t length);
/* With `on`, a file waits to begin (or to be deleted): store_file_begin (store_file_delete)
 * said STORE_BUSY while a record was being written (store_busy), and maintenance takes no
 * further step, so the record commits on store_poll's own and the next call finds the writer
 * free. 0 once the caller has begun or gives up. */
void store_file_wait(store* s, int on);
/* Offers bytes of the open file: takes what fits the 256-byte staging buffer and returns how
 * many (0 while it is full: offer the rest after a store_poll). store_poll programs them. */
uint32_t store_file_write(store* s, const void* p, uint32_t n);
/* Drains a stop-and-wait batch without changing the platform's program window.
 * Nonfinal batches end at a 16-byte file offset and contain at least 16 buffered
 * bytes; the final file tail may be shorter. Once requested, any smaller remainder
 * at a page/ring/chunk boundary also drains. Wait for !store_file_pending before
 * offering the next batch. No extra staging buffer is used. */
void store_file_flush(store* s);
/* Nonzero until buffered bytes and their chunk headers/commits have finished.
 * Zero for a writer waiting for more data or store_file_end (and when idle).
 * This is not the whole-file commit result; use store_file_result for that. */
int store_file_pending(const store* s);
/* Commits the open file once store_poll has programmed it: its FILE record follows its last
 * chunk. With fewer bytes taken than its length, the file is aborted and STORE_INVALID returned.
 * store_file_result tells when it is done. */
int store_file_end(store* s);
/* Drops the open file: its sectors go back to the pool or to garbage collection (a sector whose
 * claim was half programmed is erased first). Once its FILE record is being programmed (after
 * store_file_end, its chunks all written; store_file_result still STORE_BUSY), the file is past
 * aborting: it commits. */
void store_file_abort(store* s);
/* STORE_BUSY while a file is open or committing, else the last file's result: STORE_OK
 * (committed), STORE_INVALID (ended short), STORE_ERROR (a flash operation failed),
 * STORE_NOT_FOUND (aborted, or a format meanwhile) or STORE_TIMEOUT (its sender went quiet). */
int store_file_result(const store* s);
/* The caller's clock, once a main-loop pass: `ms` milliseconds passed since the last call. An
 * open file that takes no bytes for STORE_FILE_IDLE_MS is aborted (STORE_TIMEOUT), so a SysEx
 * sender that disappears cannot hold the store: while a file is open, maintenance waits, and
 * small writes go on only while the head sector has room (a new head would take the pool the
 * file has claimed). The time while the staging buffer is full (the store, not the sender, is
 * the slow one) and after store_file_end does not count. */
void store_file_idle(store* s, uint32_t ms);
int store_file_open(const store* s, const char* name, store_file* f);
/* Up to n bytes of f from `off` into p, straight from flash. Returns how many, or
 * STORE_NOT_FOUND once the file was deleted or replaced. */
int store_file_read(const store* s, const store_file* f, uint32_t off, void* p, uint32_t n);
/* The committed files in name order: f->name "" for the first; 1 with f filled, 0 at the end. */
int store_dir_next(const store* s, store_file* f);
int store_file_delete(store* s, const char* name);
/* STORE_EXISTS if `to` names another file. */
int store_file_rename(store* s, const char* from, const char* to);

/* CRC-32 (IEEE 802.3, reflected, as zlib's crc32): `crc` 0 to start, or a previous result. */
uint32_t store_crc32(uint32_t crc, const void* p, uint32_t n);

#endif
