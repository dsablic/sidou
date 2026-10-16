// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's store: a log-structured, power-loss-safe key-value store in the flash region of
 * app/store_layout.h, for the settings, MIDI learn's map, user chip profiles and (Task 3)
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
 *   programmed right after the erase, then a claim (the sector's sequence number, a CRC and
 *   a claim word), programmed when the sector is taken into use. A sector without a valid
 *   erase header is dirty whatever it reads, and is erased before any use;
 * - a record is programmed data first, then its header, then its commit word; it counts only
 *   with a good CRC and the commit word programmed. Anything after a sector's last valid
 *   record makes the sector full: nothing is ever programmed over bytes not erased;
 * - every record carries a sequence number; the newest wins. A copy made by garbage
 *   collection keeps its record's number, and of two equal numbers the one in the newer
 *   sector wins (they hold the same bytes);
 * - before a sector with a valid header is erased, its magic is programmed to zero (killed):
 *   an erase cut short can leave any mix of old bytes and 0xFF, header included, and a killed
 *   sector never counts again, whatever the erase left. (The one program over programmed
 *   bytes: it only clears bits, which NOR allows.)
 * - a deletion is a tombstone record, carried forward by garbage collection while an older
 *   record of its key survives in another sector.
 *
 * The erase-ahead pool: clean, unclaimed sectors. Whoever writes claims its sectors from the
 * pool before the first byte, so no write ever waits for an erase: a small record's new head
 * sector (refused with STORE_BUSY if that would take the pool under STORE_POOL_MIN), a file's
 * sectors (Task 3). Erases and garbage collection run only as maintenance, when the caller
 * says the device is quiet (store_quiet), and refill the pool toward STORE_POOL_TARGET.
 *
 * Wear: sectors are taken from the pool and erased lowest erase count first, and when the
 * erase counts spread more than STORE_WEAR_SPREAD, maintenance moves the coldest claimed
 * sector's live records so it cycles too. NOR parts endure about 100k erases a sector. A
 * settings save a minute for ten years is about 5.3M record writes; at about 200 records a
 * sector (records of ~16 bytes of data) that is 26k sector fills, spread over the 53 sectors
 * outside the pool's minimum (the pool's sectors rotate with the rest): under 500 erases a
 * sector.
 *
 * RAM: the store struct, about 1.9 KB (a sector index, a key index, a 256-byte buffer). */
#ifndef SIDOU_STORE_H
#define SIDOU_STORE_H

#include <stdint.h>

#include "store_layout.h"

#define STORE_FORMAT 1u /* the format version in every erase header */
#define STORE_HEADER 32u /* a sector's header; records from here */
#define STORE_RECORDS (STORE_SECTOR_USE - STORE_HEADER) /* 0xEE0: 3,808 bytes of records */
#define STORE_REC_HEADER 16u
#define STORE_REC_OVERHEAD 20u /* the header and the commit word */
#define STORE_VALUE_MAX 1248u /* the largest small value (a file chunk's data, Task 3) */
#define STORE_KEYS_MAX 48u /* small values and files together */
#define STORE_KEY_MAX 0x7FFFFFFFu /* the largest caller key; keys above are the store's */
#define STORE_TYPE_MAX 0x7Fu /* the largest caller type; types above are the store's */
#define STORE_TYPE_TOMB 0x80u /* a tombstone: the key was deleted */
#define STORE_WINDOW_MAX 256u /* the most one program writes: a page */
#define STORE_FILE_MAX 65536u /* the largest file (Task 3) */
#define STORE_CHUNK 1248u /* a file chunk's data (Task 3): three chunk records fill a sector */
#define STORE_FILE_SECTOR_DATA (3u * STORE_CHUNK) /* 3,744 bytes of a file a sector */
#define STORE_GC_RESERVE 2u /* the pool's last sectors, which only garbage collection takes */
/* Enough for the largest file and the garbage collector's reserve. */
#define STORE_POOL_MIN ((STORE_FILE_MAX + STORE_RECORDS - 1u) / STORE_RECORDS + STORE_GC_RESERVE)
#define STORE_POOL_TARGET (STORE_POOL_MIN + 2u)
/* The most live data the store takes (records with their headers): the sectors outside the
 * pool's minimum at a file's packing. 198,432 bytes. */
#define STORE_CAPACITY ((STORE_SECTORS - STORE_POOL_MIN) * STORE_FILE_SECTOR_DATA)
#define STORE_WEAR_SPREAD 4u /* erase counts further apart than this: move the coldest */
#define STORE_MOUNT_SLICE 1024u /* the record bytes one store_poll checks while mounting */

_Static_assert(STORE_POOL_MIN == 20u, "ceil(65,536 / 3,808) + 2");
_Static_assert((STORE_FILE_MAX + STORE_FILE_SECTOR_DATA - 1u) / STORE_FILE_SECTOR_DATA +
                       STORE_GC_RESERVE <=
                   STORE_POOL_MIN,
               "the pool covers the largest file at the chunks' real packing");
_Static_assert(3u * (STORE_CHUNK + STORE_REC_OVERHEAD) <= STORE_RECORDS, "3 chunks a sector");
_Static_assert(STORE_VALUE_MAX + STORE_REC_OVERHEAD <= STORE_RECORDS, "a value fits a sector");

/* Results. A hook returns STORE_OK, STORE_LATER or an error (any other value). */
enum {
    STORE_OK = 0,
    STORE_LATER = 1, /* a hook: not now, ask again on a later store_poll */
    STORE_BUSY = -1, /* not now: the pool is short (it refills when quiet), or a write runs */
    STORE_FULL = -2, /* it does not fit, even after garbage collection */
    STORE_NOT_FOUND = -3,
    STORE_FOREIGN = -4, /* the region holds another firmware's data: nothing is written */
    STORE_INVALID = -5, /* a bad argument */
    STORE_ERROR = -6, /* a flash operation failed */
};

/* The region, as the mount found it. */
typedef enum {
    STORE_STATE_UNMOUNTED,
    STORE_STATE_MOUNTING,
    STORE_STATE_BLANK, /* all 0xFF and no headers: formats lazily as the pool fills */
    STORE_STATE_SIDOU, /* SIDOU's headers */
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
    uint32_t seq; /* its claim's sequence number */
    uint32_t erases;
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
    uint16_t off; /* where it goes */
    uint16_t done; /* data bytes programmed */
    uint16_t from_off; /* a copy's original, for the key index */
    uint8_t sec;
    uint8_t type;
    uint8_t phase;
    uint8_t copy; /* 1: garbage collection's copy */
    uint8_t from_sec;
    uint8_t pad[3];
} store_writer;

typedef struct {
    store_flash flash;
    uint32_t seq; /* the next sequence number (records and claims) */
    uint32_t live; /* bytes of live records */
    uint32_t corrupt; /* inconsistencies the mount saw */
    uint32_t errors; /* flash operations that failed */
    uint32_t window; /* the most bytes one program writes */
    store_sector sector[STORE_SECTORS];
    uint8_t sstate[STORE_SECTORS];
    uint8_t state; /* store_state_id */
    uint8_t head; /* the sector small records go to, or 0xFF */
    uint8_t quiet;
    uint8_t nkeys;
    store_key key[STORE_KEYS_MAX];
    store_writer w;
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
    uint8_t buf[STORE_WINDOW_MAX];
} store;

typedef struct {
    uint32_t free; /* bytes that still fit (STORE_CAPACITY less the live records) */
    uint32_t live;
    uint32_t pool; /* clean sectors ready to claim */
    uint32_t dirty; /* sectors waiting for an erase */
    uint32_t claimed;
    uint32_t erases_min;
    uint32_t erases_max;
    uint32_t keys;
    uint32_t corrupt;
    uint32_t errors;
} store_stats;

/* `window`: the most bytes a program may write (1..STORE_WINDOW_MAX; the device's write
 * window, M2a Task 4). The store is unmounted until store_mount. */
void store_init(store* s, const store_flash* flash, uint32_t window);
/* Starts the mount; store_poll does it in slices. */
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
 * The data stays the caller's until store_busy is 0: store_poll programs it from there. The
 * place is taken before this returns. STORE_OK: it commits on later store_polls (a power cut
 * before then leaves the old value). STORE_BUSY: not now (a write runs, or a new head sector
 * would take the pool under its minimum); STORE_FULL: not even after garbage collection. */
int store_put(store* s, uint32_t key, uint8_t type, const void* data, uint32_t len);
/* Deletes `key` (a tombstone). STORE_OK also when it does not exist. */
int store_delete(store* s, uint32_t key);
/* Asks for a garbage collection of the best victim now, quiet or not (its erase still waits for
 * quiet). STORE_NOT_FOUND when no sector would gain, STORE_BUSY while maintenance runs. */
int store_collect(store* s);
/* Every sector dirty, to be erased by maintenance: for a region another firmware left
 * (STORE_STATE_FOREIGN), after the user confirmed. Loses everything. */
void store_format(store* s);
void store_stats_get(const store* s, store_stats* out);

/* CRC-32 (IEEE 802.3, reflected, as zlib's crc32): `crc` 0 to start, or a previous result. */
uint32_t store_crc32(uint32_t crc, const void* p, uint32_t n);

#endif
