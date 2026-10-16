// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The store's core (store.h); the format is docs/reference/store-format.md. */
#include "store.h"

#include <string.h>

/* A sector's state in sstate's low bits, and flags. */
enum {
    SEC_DIRTY = 0,    /* to be erased before any use */
    SEC_CLEAN = 1,    /* erase header valid, the rest erased: in the pool */
    SEC_RESERVED = 2, /* taken from the pool in RAM; its claim is not programmed yet */
    SEC_CLAIMED = 3,  /* claimed: records from STORE_HEADER up to `free` */
    SEC_STATE = 0x07,
    SEC_REDO = 0x08,  /* a twin (a copy cut short?) found in it: its last record to program again */
    SEC_FULL = 0x10,  /* nothing more is written into it */
    SEC_GUESS = 0x20, /* the mount could not read its erase count */
    SEC_FILE = 0x40,  /* a live (or open) file's: never collected, never a head */
    /* A cut left it: its records end in a cut header, at `free` (claimed), or its kill was cut
     * (dirty: any killed sector). Programmed to zero before the session writes (fix_first). */
    SEC_TORN = 0x80,
};

/* store.work's units: each about 4 instructions or fewer on the device (Task 4 measures it): a
 * byte scanned or copied 1, a CRC byte 3, a key compared 1, a map() call 3, a sector visited in
 * a loop over them 2. */
enum { WORK_CRC = 3, WORK_KEY = 1, WORK_MAP = 3, WORK_SEC = 2 };

/* Most of the least-live sectors garbage collection plans over to find a gain (plan). */
#define PLAN_MAX 8u

enum { W_IDLE, W_CLAIM, W_DATA, W_HEAD, W_COMMIT };
/* The newest record at the mount (newest), then the session's first write completing it
 * (fix_step): none; its header to program again; the same, the record read uncommitted (to
 * index at the mount's finish); its commit word; a chunk was newest (nothing to do). */
enum { FIX_NONE, FIX_HEAD, FIX_NEW, FIX_COMMIT, FIX_CHUNK };
enum { JOB_NONE, JOB_ERASE, JOB_COLLECT };
enum { E_KILL, E_ERASE, E_HEADER };
enum { C_SCAN, C_KILL, C_FORGET };
enum { F_IDLE, F_DATA, F_CLAIM, F_HEAD, F_COMMIT, F_SEALED, F_FILE };

#define NONE 0xFFu

static const uint8_t MAGIC[8] = {'S', 'I', 'D', 'O', 'U', 'S', 'T', 'R'};

/* CRC-32 four bits at a time: a 64-byte table (the store's code is kept small, M2a). */
static const uint32_t CRC_NIB[16] = {
    0x00000000u, 0x1DB71064u, 0x3B6E20C8u, 0x26D930ACu, 0x76DC4190u, 0x6B6B51F4u,
    0x4DB26158u, 0x5005713Cu, 0xEDB88320u, 0xF00F9344u, 0xD6D6A3E8u, 0xCB61B38Cu,
    0x9B64C2B0u, 0x86D3D2D4u, 0xA00AE278u, 0xBDBDF21Cu};

uint32_t store_crc32(uint32_t crc, const void* p, uint32_t n) {
    const uint8_t* b = p;
    crc = ~crc;
    while (n--) {
        crc ^= *b++;
        crc = CRC_NIB[crc & 15u] ^ (crc >> 4);
        crc = CRC_NIB[crc & 15u] ^ (crc >> 4);
    }
    return ~crc;
}

static uint16_t rd16(const uint8_t* p) {
    return (uint16_t)(p[0] | p[1] << 8);
}

static uint32_t rd32(const uint8_t* p) {
    return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24;
}

static void wr16(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
}

static void wr32(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}

static uint32_t pad4(uint32_t n) {
    return (n + 3u) & ~3u;
}

/* A record's bytes: header, data padded to 4, commit word. */
static uint32_t rec_size(uint32_t len) {
    return STORE_REC_HEADER + pad4(len) + 4u;
}

static uint32_t base(uint32_t sec) {
    return STORE_LO + sec * STORE_SECTOR;
}

/* Counts work, from const paths too (a store is never a const object). */
static void work(const store* s, uint32_t n) {
    ((store*)s)->work += n;
}

static const uint8_t* at(const store* s, uint32_t sec, uint32_t off) {
    work(s, WORK_MAP);
    return s->flash.map(s->flash.ctx, base(sec) + off);
}

static uint32_t crc(const store* s, uint32_t c, const void* p, uint32_t n) {
    work(s, WORK_CRC * n);
    return store_crc32(c, p, n);
}

static int all_ff(const uint8_t* p, uint32_t n) {
    for (uint32_t i = 0; i < n; i++) {
        if (p[i] != 0xFFu) {
            return 0;
        }
    }
    return 1;
}

static int all_zero(const uint8_t* p, uint32_t n) {
    uint8_t any = 0;
    for (uint32_t i = 0; i < n; i++) {
        any |= p[i];
    }
    return !any;
}

static int sec_state(const store* s, uint32_t sec) {
    return s->sstate[sec] & SEC_STATE;
}

static void set_state(store* s, uint32_t sec, int state) {
    s->sstate[sec] = (uint8_t)((s->sstate[sec] & SEC_GUESS) | state);
}

static int writable(const store* s) {
    return s->state == STORE_STATE_BLANK || s->state == STORE_STATE_SIDOU;
}

/* ---- the key index ---- */

static store_key* find(store* s, uint32_t key) {
    for (uint32_t i = 0; i < s->nkeys; i++) {
        if (s->key[i].key == key) {
            return &s->key[i];
        }
    }
    return 0;
}

static const store_key* find_c(const store* s, uint32_t key) {
    return find((store*)s, key);
}

/* Keys in use: the index, and an open file's fresh key, taken at its begin. */
static uint32_t keys_taken(const store* s) {
    return s->nkeys + (s->f.phase != F_IDLE && !find_c(s, s->f.key));
}

static void drop_key(store* s, store_key* e) {
    *e = s->key[--s->nkeys];
}

static int keyed(uint8_t type) {
    return type <= STORE_TYPE_MAX || type == STORE_TYPE_TOMB || type == STORE_TYPE_FILE;
}

static uint32_t entry_size(const store* s, const store_key* e) {
    return rec_size(rd16(at(s, e->sec, e->off) + 2));
}

/* ---- the pool ---- */

static uint32_t count(const store* s, int state) {
    uint32_t n = 0;
    work(s, WORK_SEC * STORE_SECTORS);
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        n += sec_state(s, i) == state;
    }
    return n;
}

/* The sector in `state` with the lowest erase count, or NONE. */
static uint32_t coldest(const store* s, int state) {
    uint32_t best = NONE;
    work(s, WORK_SEC * STORE_SECTORS);
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        if (sec_state(s, i) == state &&
            (best == NONE || s->sector[i].erases < s->sector[best].erases)) {
            best = i;
        }
    }
    return best;
}

/* A sector garbage collection may take: claimed, not a live file's, not being written to. */
static int collectable(const store* s, uint32_t sec) {
    return sec_state(s, sec) == SEC_CLAIMED && !(s->sstate[sec] & SEC_FILE) && sec != s->head &&
           sec != s->fres_sec && !(s->job != JOB_NONE && s->job_sec == sec);
}

/* The live record of `sec` with the lowest offset after `after` (its size in *size), or 0: the
 * order a collection copies them in. */
static uint32_t next_live(const store* s, uint32_t sec, uint32_t after, uint32_t* size) {
    uint32_t best = 0;
    work(s, WORK_KEY * s->nkeys);
    for (uint32_t i = 0; i < s->nkeys; i++) {
        const store_key* e = &s->key[i];
        if (e->sec == sec && e->off > after && (!best || e->off < best)) {
            best = e->off;
        }
    }
    if (best) {
        *size = rec_size(rd16(at(s, sec, best) + 2));
    }
    return best;
}

/* Garbage collection's plan. The collectable sectors from the least live (the oldest on a tie):
 * collecting the first k copies their live records, one after another as a collection does,
 * into the head's room and then into fresh sectors (at most `fresh` of them), and gains when
 * the copies fill fewer than k sectors. Returns the first (the victim) if a run of at most
 * `most` of them gains, else NONE; `*gain`, if not NULL, gets the most a run gains. A run that
 * gains goes on gaining as its sectors are collected one by one, so the collections never churn
 * (half-live sectors, say a boot's head of two 1,268-byte records, gain three for two). */
static uint32_t plan(const store* s, uint32_t fresh, uint32_t most, uint32_t* gain) {
    uint8_t order[STORE_SECTORS];
    uint32_t n = 0;
    work(s, WORK_SEC * STORE_SECTORS);
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        if (!collectable(s, i)) {
            continue;
        }
        const store_sector* c = &s->sector[i];
        uint32_t j = n++;
        while (j && (s->sector[order[j - 1]].live > c->live ||
                     (s->sector[order[j - 1]].live == c->live &&
                      s->sector[order[j - 1]].seq > c->seq))) {
            order[j] = order[j - 1];
            j--;
        }
        order[j] = (uint8_t)i;
        work(s, n);
    }
    uint32_t h = s->head, room = 0;
    if (h != NONE && !(s->sstate[h] & SEC_FULL) &&
        (sec_state(s, h) == SEC_CLAIMED || sec_state(s, h) == SEC_RESERVED)) {
        room = STORE_SECTOR_USE - s->sector[h].free - (h == s->fres_sec ? s->fres_bytes : 0u);
    }
    uint32_t used = 0, best = 0;
    for (uint32_t k = 0; k < n && k < most && used <= fresh; k++) {
        uint32_t size = 0;
        for (uint32_t off = next_live(s, order[k], 0, &size); off;
             off = next_live(s, order[k], off, &size)) {
            if (size > room) {
                if (++used > fresh) {
                    k = n; /* no more fresh sectors: the run ends here */
                    break;
                }
                room = STORE_RECORDS;
            }
            room -= size;
        }
        if (k < n && k + 1 > used + best) {
            best = k + 1 - used;
            if (!gain) {
                break;
            }
        }
    }
    if (gain) {
        *gain = best;
    }
    return best ? order[0] : NONE;
}

/* The victim a collection takes now, or NONE. */
static uint32_t victim(const store* s) {
    return plan(s, count(s, SEC_CLEAN), PLAN_MAX, 0);
}

/* Sectors maintenance could still add to the pool: the dirty ones, and what collections gain. */
static uint32_t reclaimable(const store* s) {
    uint32_t gain = 0;
    plan(s, count(s, SEC_CLEAN), STORE_SECTORS, &gain);
    return count(s, SEC_DIRTY) + gain;
}

/* The claimed sector holding the oldest erase count, if the counts spread too far. */
static uint32_t wear_victim(const store* s) {
    uint32_t max = 0, cold = NONE;
    work(s, WORK_SEC * STORE_SECTORS);
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        max = s->sector[i].erases > max ? s->sector[i].erases : max;
        if (sec_state(s, i) == SEC_CLAIMED && !(s->sstate[i] & SEC_FILE) && i != s->head &&
            i != s->fres_sec && (cold == NONE || s->sector[i].erases < s->sector[cold].erases)) {
            cold = i;
        }
    }
    return cold != NONE && max - s->sector[cold].erases > STORE_WEAR_SPREAD ? cold : NONE;
}

/* A place for `size` bytes of records in the head (less the room kept for an open file's
 * FILE record), or a new head from the pool if that leaves more than `floor` sectors in it.
 * The sector, or NONE. */
static uint32_t place(store* s, uint32_t size, uint32_t floor) {
    uint32_t h = s->head;
    uint32_t keep = h == s->fres_sec ? s->fres_bytes : 0u;
    if (h != NONE && !(s->sstate[h] & SEC_FULL) &&
        s->sector[h].free + size + keep <= STORE_SECTOR_USE) {
        return h;
    }
    if (count(s, SEC_CLEAN) <= floor) {
        return NONE;
    }
    h = coldest(s, SEC_CLEAN);
    set_state(s, h, SEC_RESERVED);
    s->sector[h].seq = s->sector[h].erases; /* the claim's number (sector_header) */
    s->sector[h].free = STORE_HEADER;
    s->sector[h].live = 0;
    s->head = (uint8_t)h;
    return h;
}

/* ---- files: where their chunks are ---- */

static uint32_t chunk_len(uint32_t length, uint32_t i) {
    uint32_t rest = length - i * STORE_CHUNK;
    return rest < STORE_CHUNK ? rest : STORE_CHUNK;
}

/* Chunk i's record in its sector: three a sector, at fixed places. */
static uint32_t chunk_off(uint32_t i) {
    return STORE_HEADER + (i % 3u) * (STORE_CHUNK + STORE_REC_OVERHEAD);
}

static uint32_t chunks_of(uint32_t length) {
    return (length + STORE_CHUNK - 1u) / STORE_CHUNK;
}

static uint32_t sectors_of(uint32_t length) {
    return (length + STORE_FILE_SECTOR_DATA - 1u) / STORE_FILE_SECTOR_DATA;
}

/* The records a file's chunks take. */
static uint32_t chunk_bytes(uint32_t length) {
    uint32_t n = chunks_of(length);
    return n ? (n - 1u) * rec_size(STORE_CHUNK) + rec_size(chunk_len(length, n - 1u)) : 0u;
}

/* A FILE record's data (after its 16-byte header): fid, length, CRC, type, sector count, chunk
 * count, name, sectors. */
static const uint8_t* file_data(const uint8_t* rec) {
    return rec + STORE_REC_HEADER;
}

/* A live file's chunks count as live in its sectors (sign 1), or no more (sign -1). */
static void file_live(store* s, const uint8_t* rec, int sign) {
    const uint8_t* d = file_data(rec);
    uint32_t length = rd32(d + 4), chunks = rd16(d + 14);
    for (uint32_t i = 0; i < chunks; i++) {
        uint32_t sec = d[STORE_FILE_HEAD + i / 3u], size = rec_size(chunk_len(length, i));
        if (sign > 0) {
            s->sector[sec].live = (uint16_t)(s->sector[sec].live + size);
            s->live += size;
            s->sstate[sec] |= SEC_FILE;
        } else {
            s->sector[sec].live = (uint16_t)(s->sector[sec].live - size);
            s->live -= size;
            s->sstate[sec] &= (uint8_t)~SEC_FILE;
        }
    }
}

/* ---- programming ---- */

/* Programs part of [off, off + n) of sector `sec` from `src`, from byte `*done` on: the most
 * one program may (the window, the page's end). 1 when the range is done, 0 when not yet
 * (a piece went, or the hook said later), < 0 on an error. */
static int prog_piece(store* s, uint32_t sec, uint32_t off, const uint8_t* src, uint32_t n,
                      uint16_t* done) {
    uint32_t a = base(sec) + off + *done;
    uint32_t k = n - *done;
    uint32_t page = STORE_PAGE - (a & (STORE_PAGE - 1u));
    /* A 4-byte word (a commit word) is one program whatever the window: a cut leaves all 32 of
     * its bits in play, so it reads erased only if every bit it cleared is left faint. */
    uint32_t window = n == 4u && s->window < 4u ? 4u : s->window;
    k = k > window ? window : k;
    k = k > page ? page : k;
    /* The device programs with XIP off: the bytes go from RAM, wherever the caller's are. */
    uint8_t ram[STORE_WINDOW_MAX];
    memcpy(ram, src + *done, k);
    s->work += k;
    int rc = s->flash.prog(s->flash.ctx, a, ram, k);
    if (rc == STORE_LATER) {
        return 0;
    }
    if (rc != STORE_OK) {
        s->errors++;
        return -1;
    }
    *done = (uint16_t)(*done + k);
    return *done == n;
}

/* A sector's header as it should read: the erase header (magic, format, erase count, CRC of
 * bytes 0-15), then the claim (its number, CRC of bytes 0-23, the claim word). The claim's
 * number is the erase count again, so every claim of an erase cycle is the same 32 bytes: a
 * claim the mount read as never made (cut, every bit it touched left faint, or failed) is made
 * again over the same bytes, never different ones. A claim programs all 32 bytes, the erase
 * header again too, so bits a cut left faint in it are programmed for good before the sector
 * takes any record. */
static void sector_header(const store* s, uint32_t sec, uint8_t* h) {
    memcpy(h, MAGIC, 8);
    wr16(h + 8, STORE_FORMAT);
    wr16(h + 10, 0);
    wr32(h + 12, s->sector[sec].erases);
    wr32(h + 16, crc(s, 0, h, 16));
    wr32(h + 20, s->sector[sec].seq);
    wr32(h + 24, crc(s, 0, h, 24));
    wr32(h + 28, 0);
}

static void rec_header(const store_writer* w, uint8_t* h) {
    h[0] = w->type;
    h[1] = 0;
    wr16(h + 2, w->len);
    wr32(h + 4, w->key);
    wr32(h + 8, w->seq);
    wr32(h + 12, w->crc);
}

static void file_fail(store* s, int result);

static void writer_failed(store* s) {
    store_writer* w = &s->w;
    if (!w->copy && w->type == STORE_TYPE_FILE && s->f.phase == F_FILE && w->key == s->f.key) {
        s->f.phase = F_SEALED; /* so file_fail can drop it */
        file_fail(s, STORE_ERROR);
    }
    if (w->phase == W_CLAIM) {
        set_state(s, w->sec, SEC_DIRTY);
    } else {
        s->sstate[w->sec] |= SEC_FULL;
    }
    if (s->head == w->sec) {
        s->head = NONE;
    }
    if (w->copy) {
        s->job = JOB_NONE; /* the collection stops: its victim keeps its records */
    }
    w->phase = W_IDLE;
}

/* The record is committed: the key index follows. */
static void writer_done(store* s) {
    store_writer* w = &s->w;
    uint32_t size = rec_size(w->len);
    store_sector* c = &s->sector[w->sec];
    c->free = (uint16_t)(w->off + size);
    w->phase = W_IDLE;
    s->work += WORK_KEY * s->nkeys;
    store_key* e = find(s, w->key);
    if (w->copy) {
        if (e && e->sec == w->from_sec && e->off == w->from_off) {
            s->sector[w->from_sec].live = (uint16_t)(s->sector[w->from_sec].live - size);
            c->live = (uint16_t)(c->live + size);
            e->sec = w->sec;
            e->off = w->off;
        }
        if (e) {
            e->stale++; /* the original, or this copy of a record superseded meanwhile */
        }
        return;
    }
    if (e) {
        const uint8_t* was = at(s, e->sec, e->off);
        uint32_t old = rec_size(rd16(was + 2));
        s->sector[e->sec].live = (uint16_t)(s->sector[e->sec].live - old);
        s->live -= old;
        e->stale++;
        if (was[0] == STORE_TYPE_FILE) {
            file_live(s, was, -1); /* replaced, renamed or deleted */
        }
    } else if (s->nkeys < STORE_KEYS_MAX) {
        e = &s->key[s->nkeys++];
        e->key = w->key;
        e->stale = 0;
    } else {
        s->corrupt++; /* never: write_record and file_key keep a slot for every new key */
        if (w->type == STORE_TYPE_FILE && s->f.phase == F_FILE && w->key == s->f.key) {
            s->f.phase = F_IDLE;
            s->f.result = STORE_ERROR;
        }
        return;
    }
    e->sec = w->sec;
    e->off = w->off;
    c->live = (uint16_t)(c->live + size);
    s->live += size;
    if (w->type == STORE_TYPE_FILE) {
        file_live(s, at(s, w->sec, w->off), 1);
        if (s->f.phase == F_FILE && w->key == s->f.key) {
            s->f.phase = F_IDLE; /* the open file is committed */
            s->f.result = STORE_OK;
        }
    }
}

static void writer_step(store* s) {
    store_writer* w = &s->w;
    int rc = 0;
    if (w->phase == W_CLAIM) {
        uint8_t h[STORE_HEADER];
        sector_header(s, w->sec, h);
        rc = prog_piece(s, w->sec, 0, h, STORE_HEADER, &w->done);
        if (rc > 0) {
            set_state(s, w->sec, SEC_CLAIMED);
            w->phase = w->len ? W_DATA : W_HEAD;
            w->done = 0;
            return;
        }
    } else if (w->phase == W_DATA) {
        if (w->src) {
            rc = prog_piece(s, w->sec, w->off + STORE_REC_HEADER, w->src, w->len, &w->done);
        } else {
            rc = prog_piece(s, w->sec, w->off + STORE_REC_HEADER,
                            s->flash.map(s->flash.ctx, w->from + STORE_REC_HEADER), w->len,
                            &w->done);
        }
        if (rc > 0) {
            w->phase = W_HEAD;
            w->done = 0;
            return;
        }
    } else if (w->phase == W_HEAD) {
        uint8_t h[STORE_REC_HEADER];
        rec_header(w, h);
        rc = prog_piece(s, w->sec, w->off, h, STORE_REC_HEADER, &w->done);
        if (rc > 0) {
            w->phase = W_COMMIT;
            w->done = 0;
            return;
        }
    } else if (w->phase == W_COMMIT) {
        static const uint8_t zero[4] = {0, 0, 0, 0};
        rc = prog_piece(s, w->sec, w->off + STORE_REC_HEADER + pad4(w->len), zero, 4, &w->done);
        if (rc > 0) {
            writer_done(s);
            return;
        }
    }
    if (rc < 0) {
        writer_failed(s);
    }
}

static void write_at(store* s, uint32_t sec, uint32_t key, uint8_t type, const void* data,
                     uint32_t len);

/* Starts writing a record into a place taken now. */
static int write_record(store* s, uint32_t key, uint8_t type, const void* data, uint32_t len,
                        uint32_t floor) {
    uint32_t size = rec_size(len);
    store_key* e = find(s, key);
    if (!e && keys_taken(s) >= STORE_KEYS_MAX) {
        return STORE_FULL;
    }
    uint32_t old = e ? entry_size(s, e) : 0u;
    if (s->live - old + size > STORE_CAPACITY && size > old) {
        return STORE_FULL;
    }
    uint32_t sec = place(s, size, floor);
    if (sec == NONE) {
        return reclaimable(s) ? STORE_BUSY : STORE_FULL;
    }
    write_at(s, sec, key, type, data, len);
    return STORE_OK;
}

/* Sets the writer to program a record at the end of sector `sec`'s records. */
static void write_at(store* s, uint32_t sec, uint32_t key, uint8_t type, const void* data,
                     uint32_t len) {
    store_writer* w = &s->w;
    w->src = data;
    w->from = 0;
    w->key = key;
    w->seq = s->seq++;
    w->type = type;
    w->len = (uint16_t)len;
    w->sec = (uint8_t)sec;
    w->off = s->sector[sec].free;
    w->copy = 0;
    w->done = 0;
    uint8_t h[STORE_REC_HEADER];
    rec_header(w, h);
    w->crc = crc(s, crc(s, 0, h, 12), data, len);
    w->phase = sec_state(s, sec) == SEC_RESERVED ? W_CLAIM : (len ? W_DATA : W_HEAD);
}

/* ---- the mount ---- */

/* The mount's place in a sector: what its next poll checks. */
enum { M_HEAD, M_CLEAN, M_ANY, M_TORN, M_RECORDS, M_TAIL };

static int erase_header_ok(const uint8_t* h, uint32_t* version) {
    for (uint32_t i = 0; i < 8; i++) {
        if (h[i] != MAGIC[i]) {
            return 0;
        }
    }
    *version = rd16(h + 8);
    return rd16(h + 10) == 0 && rd32(h + 16) == store_crc32(0, h, 16);
}

/* A header whose magic was killed (bits only cleared) before an erase: its erase count, if
 * the rest still checks against the magic. */
static int killed(const uint8_t* h, uint32_t* erases) {
    for (uint32_t i = 0; i < 8; i++) {
        if (h[i] & ~MAGIC[i]) {
            return 0;
        }
    }
    uint8_t c[16];
    memcpy(c, MAGIC, 8);
    memcpy(c + 8, h + 8, 8);
    *erases = rd32(h + 12);
    return rd32(h + 16) == store_crc32(0, c, 16);
}

/* An erase header whose program was cut holds every bit of the magic and the version (bits only
 * cleared toward them); the rest of its sector reads erased (M_TORN checks). */
static int torn_header(const uint8_t* h) {
    static const uint8_t want[12] = {'S', 'I', 'D', 'O', 'U', 'S', 'T', 'R', STORE_FORMAT, 0, 0, 0};
    for (uint32_t i = 0; i < 12; i++) {
        if ((h[i] & want[i]) != want[i]) {
            return 0;
        }
    }
    return 1;
}

/* Checks that bytes [mount_off, end) of the mounting sector read 0xFF, within `*budget`: 1
 * when they all do, 0 when the budget ran out first (mount_off is where to go on), -1 at a
 * byte that does not (mount_off is that byte). */
static int ff_scan(store* s, uint32_t sec, uint32_t end, uint32_t* budget) {
    const uint8_t* p = at(s, sec, 0);
    uint32_t off = s->mount_off, stop = end - off > *budget ? off + *budget : end;
    while (off < stop && p[off] == 0xFFu) {
        off++;
    }
    uint32_t n = off - s->mount_off + (off < stop);
    *budget -= n < *budget ? n : *budget;
    s->work += n;
    s->mount_off = (uint16_t)off;
    return off < stop ? -1 : off == end;
}

/* The sector's header, a fixed cost (its 32 bytes, their CRCs, a claimed sector's guard band).
 * 1 when that settles the sector; else mount_phase is what to check next, from mount_off. */
static int classify(store* s, uint32_t sec) {
    const uint8_t* h = at(s, sec, 0);
    store_sector* c = &s->sector[sec];
    uint32_t version = 0, erases = 0;
    c->seq = 0;
    c->free = STORE_HEADER;
    c->live = 0;
    c->erases = 0;
    s->sstate[sec] = SEC_DIRTY | SEC_GUESS;
    s->work += STORE_HEADER + WORK_CRC * (16u + 24u + 16u); /* the header and its CRCs */
    if (erase_header_ok(h, &version)) {
        if (version == 0 || version > STORE_FORMAT) {
            s->mount_newer = 1; /* a newer firmware's: not ours to touch */
            return 1;
        }
        s->mount_sidou = 1;
        s->sstate[sec] = SEC_DIRTY;
        c->erases = rd32(h + 12);
        if (all_ff(h + 20, 12)) {
            s->mount_phase = M_CLEAN; /* clean if the rest reads erased */
            s->mount_off = STORE_HEADER;
            return 0;
        }
        /* A claim cut short: a bad CRC, or its claim word never programmed. A claim word with
         * any bit programmed counts: its CRC checks, so the cut came during the word itself. */
        if (rd32(h + 24) != store_crc32(0, h, 24) || rd32(h + 28) == 0xFFFFFFFFu) {
            return 1;
        }
        s->sstate[sec] = SEC_CLAIMED;
        c->seq = rd32(h + 20);
        if (c->seq >= s->seq) {
            s->seq = c->seq + 1u;
        }
        s->work += STORE_SECTOR - STORE_SECTOR_USE;
        if (!all_ff(h + STORE_SECTOR_USE, STORE_SECTOR - STORE_SECTOR_USE)) {
            s->corrupt++; /* the guard band was written: read it, write no more into it */
            s->sstate[sec] |= SEC_FULL;
        }
        s->mount_phase = M_RECORDS;
        s->mount_off = STORE_HEADER;
        return 0;
    }
    if (killed(h, &erases)) {
        s->mount_sidou = 1;
        /* A kill whose zeros may be faint (even reading zero now): its records could read valid
         * at a later mount. Killed again before the session writes; the page is garbage. */
        s->sstate[sec] = SEC_DIRTY | SEC_TORN;
        c->erases = erases;
        return 1;
    }
    s->mount_phase = M_ANY; /* blank, a cut erase header, or someone else's */
    s->mount_off = 0;
    return 0;
}

/* A record at `off` of a claimed sector: its size if all but its commit word checks, else 0;
 * `*committed` whether its commit word reads programmed too (not: a write cut before its commit
 * word, or one whose commit word reads erased now and may read programmed at the next mount). */
static uint32_t rec_check(const store* s, uint32_t sec, uint32_t off, int* committed) {
    if (off + STORE_REC_OVERHEAD > STORE_SECTOR_USE) {
        return 0;
    }
    const uint8_t* h = at(s, sec, off);
    uint32_t len = rd16(h + 2), size = rec_size(len);
    if (h[0] > STORE_TYPE_FILE || off + size > STORE_SECTOR_USE ||
        (h[0] == STORE_TYPE_TOMB && len != 0) || (h[0] <= STORE_TYPE_MAX && len > STORE_VALUE_MAX) ||
        (h[0] == STORE_TYPE_CHUNK && (len == 0 || len > STORE_CHUNK)) ||
        (h[0] == STORE_TYPE_FILE &&
         (len < STORE_FILE_HEAD || len > STORE_FILE_HEAD + STORE_FILE_SECTORS))) {
        return 0;
    }
    /* The commit word never programmed: the write was cut. Any bit of it programmed counts (a
     * cut during the word itself leaves bits faint, which may read either way at each mount;
     * its header and data were programmed before it, whole, as the CRC shows). */
    *committed = rd32(h + STORE_REC_HEADER + pad4(len)) != 0xFFFFFFFFu;
    uint32_t c = crc(s, crc(s, 0, h, 12), h + STORE_REC_HEADER, len);
    return c == rd32(h + 12) ? size : 0u;
}

static void index_record(store* s, uint32_t sec, uint32_t off) {
    const uint8_t* h = at(s, sec, off);
    uint32_t seq = rd32(h + 8);
    if (seq >= s->seq) {
        s->seq = seq + 1u;
    }
    if (!keyed(h[0])) {
        return;
    }
    uint32_t key = rd32(h + 4);
    s->work += WORK_KEY * s->nkeys;
    store_key* e = find(s, key);
    if (!e) {
        if (s->nkeys >= STORE_KEYS_MAX) {
            s->corrupt++;
            return;
        }
        e = &s->key[s->nkeys++];
        e->key = key;
        e->sec = (uint8_t)sec;
        e->off = (uint16_t)off;
        e->stale = 0;
        return;
    }
    const uint8_t* cur = at(s, e->sec, e->off);
    uint32_t cur_seq = rd32(cur + 8);
    e->stale++;
    if (seq == cur_seq) {
        /* A record and its copy: the same bytes (the same CRC), whichever wins. A collection was
         * cut: the copy in flight, the last record of its sector, may end in a faint commit word,
         * so the last records of both sectors are programmed again before the session writes. */
        if (rd32(h + 12) != rd32(cur + 12) || rd16(cur + 2) != rd16(h + 2) || h[0] != cur[0]) {
            s->corrupt++;
        }
        s->sstate[sec] |= SEC_REDO;
        s->sstate[e->sec] |= SEC_REDO;
        if (s->sector[sec].seq <= s->sector[e->sec].seq) {
            return; /* either holds it: the first found, or the one with the higher claim number */
        }
    } else if (seq < cur_seq) {
        return;
    }
    e->sec = (uint8_t)sec;
    e->off = (uint16_t)off;
}

/* The write a cut interrupted, if any, is the newest record: one write is in flight at a time,
 * and each takes the next sequence number. A record whose CRC checks (its data and header went
 * whole) counts as committed when it is the newest keyed one, whatever its commit word reads: a
 * cut during the commit word can leave all its bits faint, read programmed at one mount and
 * erased at the next. The session's first write programs it again (fix_step), so it stays.
 * A committed chunk of a file begun after it (its key, the file's id, newer) shows the session
 * after it ran: nothing to complete. Tracks the newest: keyed records by number, chunks by id. */
static void newest(store* s, uint32_t sec, uint32_t off, int committed) {
    const uint8_t* h = at(s, sec, off);
    int chunk = h[0] == STORE_TYPE_CHUNK;
    if (chunk ? !committed : !keyed(h[0])) {
        return;
    }
    uint32_t seq = rd32(h + (chunk ? 4 : 8));
    if (seq >= s->seq) {
        s->seq = seq + 1u; /* its number is taken, committed or not */
    }
    if (s->fix_kind == FIX_NONE || seq > s->fix_seq) {
        s->fix_kind = (uint8_t)(chunk ? FIX_CHUNK : committed ? FIX_HEAD : FIX_NEW);
        s->fix_seq = seq;
        s->fix_sec = (uint8_t)sec;
        s->fix_off = (uint16_t)off;
    }
}

/* A claimed sector's records, within `*budget` (a record starts only if its CRC fits what is
 * left, or if it is the poll's first). 1 when the sector's records are done. */
static int scan_records(store* s, uint32_t sec, uint32_t* budget) {
    store_sector* c = &s->sector[sec];
    for (;;) {
        uint32_t off = s->mount_off;
        if (off + STORE_REC_OVERHEAD > STORE_SECTOR_USE) {
            c->free = (uint16_t)off;
            s->sstate[sec] |= SEC_FULL;
            return 1;
        }
        const uint8_t* h = at(s, sec, off);
        uint32_t len = rd16(h + 2);
        /* What checking it may take: its header scanned, its CRC, a key
         * lookup, the maps. */
        uint32_t cost = STORE_REC_HEADER + WORK_CRC * (12u + (len <= STORE_CHUNK ? len : 0u)) +
                        WORK_KEY * STORE_KEYS_MAX + 8u * WORK_MAP;
        if (cost > *budget && *budget < STORE_MOUNT_SLICE) {
            return 0; /* the next poll */
        }
        uint32_t w0 = s->work;
        s->work += STORE_REC_HEADER;
        if (all_ff(h, STORE_REC_HEADER)) {
            /* The end, unless a cut write left data after an erased header (M_TAIL). */
            c->free = (uint16_t)off;
            s->mount_phase = M_TAIL;
            s->mount_off = (uint16_t)(off + STORE_REC_HEADER);
            *budget -= STORE_REC_HEADER < *budget ? STORE_REC_HEADER : *budget;
            return 0;
        }
        int committed = 0;
        uint32_t size = rec_check(s, sec, off, &committed);
        if (size) {
            newest(s, sec, off, committed);
        }
        if (!size || !committed) {
            c->free = (uint16_t)off; /* a cut write: the sector takes no more */
            s->sstate[sec] |= SEC_FULL;
            /* A header cut short, not yet programmed to zero: faint bits in it may read right at a
             * later mount, and the newest record it would be then is not the write it was. */
            if (!size && !all_zero(h, STORE_REC_HEADER)) {
                s->sstate[sec] |= SEC_TORN;
            }
            return 1;
        }
        index_record(s, sec, off);
        s->mount_off = (uint16_t)(off + size);
        uint32_t spent = s->work - w0;
        *budget -= spent < *budget ? spent : *budget;
        if (!*budget) {
            return 0;
        }
    }
}

/* A FILE record the mount found: whole, with every chunk where it says, in sectors no other
 * file holds. */
static int file_ok(store* s, const uint8_t* rec) {
    const uint8_t* d = file_data(rec);
    uint32_t fid = rd32(d), length = rd32(d + 4), nsec = d[13], chunks = rd16(d + 14);
    if (length > STORE_FILE_MAX || nsec != sectors_of(length) || chunks != chunks_of(length) ||
        rd16(rec + 2) != STORE_FILE_HEAD + nsec) {
        return 0;
    }
    s->work += nsec * nsec + STORE_REC_HEADER * chunks;
    for (uint32_t i = 0; i < nsec; i++) {
        uint32_t sec = d[STORE_FILE_HEAD + i];
        if (sec >= STORE_SECTORS || sec_state(s, sec) != SEC_CLAIMED ||
            (s->sstate[sec] & SEC_FILE)) {
            return 0;
        }
        for (uint32_t j = 0; j < i; j++) {
            if (d[STORE_FILE_HEAD + j] == sec) {
                return 0;
            }
        }
    }
    for (uint32_t i = 0; i < chunks; i++) {
        uint32_t sec = d[STORE_FILE_HEAD + i / 3u], off = chunk_off(i), len = chunk_len(length, i);
        const uint8_t* h = at(s, sec, off);
        if (off + rec_size(len) > s->sector[sec].free || h[0] != STORE_TYPE_CHUNK ||
            rd16(h + 2) != len || rd32(h + 4) != fid || rd32(h + 8) != i) {
            return 0;
        }
    }
    return 1;
}

static void mount_finish(store* s) {
    uint32_t known = 0;
    s->work += 2u * WORK_SEC * STORE_SECTORS + 4u * s->nkeys; /* its loops, file_ok aside */
    if (s->fix_kind == FIX_NEW) {
        /* The newest record, read uncommitted: it counts (newest). Indexed last, so if the index
         * is full it is the one left out. */
        index_record(s, s->fix_sec, s->fix_off);
        s->sector[s->fix_sec].free =
            (uint16_t)(s->fix_off + rec_size(rd16(at(s, s->fix_sec, s->fix_off) + 2)));
    }
    s->fix_kind = s->fix_kind == FIX_HEAD || s->fix_kind == FIX_NEW ? FIX_HEAD : FIX_NONE;
    s->fix_done = 0;
    s->seq++; /* a number skipped: a write cut in its header (unread now) may hold the next */
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        if (!(s->sstate[i] & SEC_GUESS) && s->sector[i].erases > known) {
            known = s->sector[i].erases;
        }
    }
    s->live = 0;
    for (uint32_t i = 0; i < s->nkeys;) {
        const uint8_t* rec = at(s, s->key[i].sec, s->key[i].off);
        if (rec[0] == STORE_TYPE_FILE && !file_ok(s, rec)) {
            s->corrupt++; /* a damaged file: dropped */
            drop_key(s, &s->key[i]);
            continue;
        }
        uint32_t size = rec_size(rd16(rec + 2));
        s->sector[s->key[i].sec].live = (uint16_t)(s->sector[s->key[i].sec].live + size);
        s->live += size;
        if (rec[0] == STORE_TYPE_FILE) {
            file_live(s, rec, 1);
        }
        i++;
    }
    /* No sector from before the mount takes another record: the first write starts a fresh
     * head. A cut during the last session's last write can leave faint bits in that sector, read
     * one way now and another at the next mount; nothing written now may follow them. */
    s->head = NONE;
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        if (s->sstate[i] & SEC_GUESS) {
            s->sector[i].erases = known; /* unknown: count it among the most worn */
            s->sstate[i] &= (uint8_t)~SEC_GUESS;
        }
        if (sec_state(s, i) == SEC_CLAIMED) {
            s->sstate[i] |= SEC_FULL;
        }
    }
    if (s->mount_newer) {
        s->state = STORE_STATE_FOREIGN;
    } else if (s->mount_sidou) {
        s->state = STORE_STATE_SIDOU;
    } else {
        s->state = s->mount_blank ? STORE_STATE_BLANK : STORE_STATE_FOREIGN;
    }
}

/* Up to STORE_MOUNT_SLICE bytes of checks in the mounting sector (a record's CRC may take one
 * poll alone); the poll after the last sector's finishes the mount. */
static void mount_step(store* s) {
    uint32_t sec = s->mount_sec, budget = STORE_MOUNT_SLICE;
    if (sec == STORE_SECTORS) {
        mount_finish(s);
        return;
    }
    int done = s->mount_phase == M_HEAD ? classify(s, sec) : 0;
    budget -= s->work < budget ? s->work : budget; /* the header's checks count too */
    while (!done && budget) {
        int r;
        switch (s->mount_phase) {
        case M_CLEAN:
            r = ff_scan(s, sec, STORE_SECTOR, &budget);
            if (r > 0) {
                s->sstate[sec] = SEC_CLEAN;
            } else if (r < 0) {
                s->corrupt++; /* programmed bytes in a sector never claimed */
            }
            done = r != 0;
            break;
        case M_ANY:
            r = ff_scan(s, sec, STORE_SECTOR, &budget);
            if (r < 0) {
                s->mount_blank = 0;
                if (s->mount_off < 20u && torn_header(at(s, sec, 0))) {
                    s->mount_phase = M_TORN; /* if the rest reads erased */
                    s->mount_off = 20;
                    break;
                }
            }
            done = r != 0;
            break;
        case M_TORN:
            r = ff_scan(s, sec, STORE_SECTOR, &budget);
            s->mount_sidou |= r > 0;
            done = r != 0;
            break;
        case M_RECORDS:
            done = scan_records(s, sec, &budget);
            if (!done && s->mount_phase == M_RECORDS) {
                budget = 0;
            }
            break;
        default: /* M_TAIL: data after the erased header ends a cut write: full */
            r = ff_scan(s, sec, STORE_SECTOR_USE, &budget);
            if (r < 0) {
                /* A write cut before or in its header (faint bits in it may read right later). */
                s->sstate[sec] |= SEC_FULL | SEC_TORN;
            }
            done = r != 0;
            break;
        }
    }
    if (done) {
        s->mount_sec++;
        s->mount_off = 0;
        s->mount_phase = M_HEAD;
    }
}

/* ---- maintenance ---- */

/* Records of `key` in `sec` other than its live one. */
static uint32_t others_in(store* s, uint32_t sec, const store_key* e) {
    uint32_t n = 0;
    for (uint32_t off = STORE_HEADER; off < s->sector[sec].free;) {
        const uint8_t* h = at(s, sec, off);
        s->work += STORE_REC_HEADER;
        if (keyed(h[0]) && rd32(h + 4) == e->key && !(e->sec == sec && e->off == off)) {
            n++;
        }
        off += rec_size(rd16(h + 2));
    }
    return n;
}

/* A killed sector's records count no more: the key index follows, a slice a poll from
 * job_off. 1 when it is done and the sector is dirty. */
static int forget(store* s, uint32_t sec) {
    while (s->job_off < s->sector[sec].free) {
        if (s->work >= STORE_MOUNT_SLICE) {
            return 0;
        }
        uint32_t off = s->job_off;
        const uint8_t* h = at(s, sec, off);
        s->work += STORE_REC_HEADER + WORK_KEY * s->nkeys;
        store_key* e = keyed(h[0]) ? find(s, rd32(h + 4)) : 0;
        if (e && e->sec == sec && e->off == off) {
            drop_key(s, e); /* a tombstone with nothing left to hide */
        } else if (e && e->stale) {
            e->stale--;
        }
        s->job_off = (uint16_t)(off + rec_size(rd16(h + 2)));
    }
    s->live -= s->sector[sec].live;
    s->sector[sec].live = 0;
    set_state(s, sec, SEC_DIRTY);
    return 1;
}

/* Programs the magic to zero if the sector's header could still read valid: 1 when it did,
 * 2 when there was nothing to kill (no operation), 0 when the hook said later, -1 on an error. */
static int kill(store* s, uint32_t sec) {
    const uint8_t* h = at(s, sec, 0);
    int live = 0;
    for (uint32_t i = 0; i < 8; i++) {
        if (h[i] & ~MAGIC[i]) {
            return 2; /* never a valid header */
        }
        live |= h[i];
    }
    if (!live) {
        return 2;
    }
    static const uint8_t zero[8] = {0, 0, 0, 0, 0, 0, 0, 0};
    uint16_t done = s->job_done;
    int rc = prog_piece(s, sec, 0, zero, 8, &done);
    s->job_done = (uint8_t)done;
    return rc;
}

static void erase_step(store* s) {
    uint32_t sec = s->job_sec;
    if (s->job_phase == E_KILL) {
        int rc = kill(s, sec);
        if (rc < 0) {
            s->job = JOB_NONE;
        }
        if (rc <= 0) {
            return;
        }
        s->job_phase = E_ERASE;
        s->job_done = 0;
        if (rc == 1) {
            return; /* the kill was this call's operation */
        }
    }
    if (s->job_phase == E_ERASE) {
        int rc = s->flash.erase(s->flash.ctx, base(sec));
        if (rc == STORE_LATER) {
            return;
        }
        if (rc != STORE_OK) {
            s->errors++;
            s->job = JOB_NONE;
            return;
        }
        s->sector[sec].erases++;
        s->job_phase = E_HEADER;
        return;
    }
    uint8_t h[STORE_HEADER];
    sector_header(s, sec, h);
    uint16_t done = s->job_done;
    int rc = prog_piece(s, sec, 0, h, 20, &done);
    s->job_done = (uint8_t)done;
    if (rc < 0) {
        s->job = JOB_NONE;
    } else if (rc > 0) {
        set_state(s, sec, SEC_CLEAN);
        s->sector[sec].free = STORE_HEADER;
        s->sector[sec].live = 0;
        s->job = JOB_NONE;
        s->state = STORE_STATE_SIDOU; /* a blank region's first header formats it */
    }
}

static int fix_first(const store* s);

/* Starts copying the record at `off` of `from` into the head, as a collection does (its number
 * kept); its first operation runs now. No room: the collection stops. */
static void copy_record(store* s, uint32_t from, uint32_t off) {
    const uint8_t* h = at(s, from, off);
    uint32_t len = rd16(h + 2), sec = place(s, rec_size(len), 0);
    if (sec == NONE) {
        s->job = JOB_NONE; /* no room: try again when the pool refilled */
        return;
    }
    store_writer* w = &s->w;
    w->src = 0;
    w->from = base(from) + off;
    w->key = rd32(h + 4);
    w->seq = rd32(h + 8);
    w->crc = rd32(h + 12);
    w->type = h[0];
    w->len = (uint16_t)len;
    w->sec = (uint8_t)sec;
    w->off = s->sector[sec].free;
    w->copy = 1;
    w->from_sec = (uint8_t)from;
    w->from_off = (uint16_t)off;
    w->done = 0;
    w->phase = sec_state(s, sec) == SEC_RESERVED ? W_CLAIM : (len ? W_DATA : W_HEAD);
    writer_step(s);
}

/* The next step of a collection: skips the victim's dead records (a slice a poll), starts the
 * next live one's copy, or (all copied) kills the victim and forgets its records. */
static void collect_step(store* s) {
    uint32_t v = s->job_sec;
    if (s->job_phase == C_SCAN) {
        while (s->job_off < s->sector[v].free) {
            if (s->work >= STORE_MOUNT_SLICE) {
                return; /* the next poll goes on */
            }
            uint32_t off = s->job_off;
            const uint8_t* h = at(s, v, off);
            uint32_t len = rd16(h + 2), size = rec_size(len);
            if (h[0] == STORE_TYPE_TOMB && s->work > STORE_MOUNT_SLICE / 2u) {
                return; /* a tombstone's check walks the sector: a poll of its own */
            }
            s->job_off = (uint16_t)(off + size);
            s->work += STORE_REC_HEADER + WORK_KEY * s->nkeys;
            store_key* e = keyed(h[0]) ? find(s, rd32(h + 4)) : 0;
            if (!e || e->sec != v || e->off != off) {
                continue; /* superseded: garbage */
            }
            if (h[0] == STORE_TYPE_TOMB && e->stale <= others_in(s, v, e)) {
                continue; /* nothing older outside the victim: dropped with it */
            }
            copy_record(s, v, off);
            return;
        }
        s->job_phase = C_KILL;
        s->job_done = 0;
    }
    if (s->job_phase == C_KILL) {
        int rc = kill(s, v);
        if (rc < 0) {
            s->job = JOB_NONE;
        }
        if (rc <= 0) {
            return;
        }
        s->job_phase = C_FORGET;
        s->job_off = STORE_HEADER;
        if (rc == 1) {
            return; /* the kill was this call's operation */
        }
    }
    if (forget(s, v)) {
        s->job = JOB_NONE;
    }
}

static void start_job(store* s, int job, uint32_t sec) {
    s->job = (uint8_t)job;
    s->job_done = 0;
    s->job_sec = (uint8_t)sec;
    s->job_phase = job == JOB_ERASE ? E_KILL : C_SCAN;
    s->job_off = job == JOB_ERASE ? 0u : STORE_HEADER;
}

/* The maintenance due now, if any: a dirty sector to erase while the pool is under its
 * target, else a victim to collect (one that needs no room, or with a clean sector for its
 * copies), else, with the pool full, the coldest sector to move. JOB_NONE if nothing. */
static int due(const store* s, uint32_t* sec) {
    uint32_t pool = count(s, SEC_CLEAN);
    if (pool < STORE_POOL_TARGET) {
        *sec = coldest(s, SEC_DIRTY);
        if (*sec != NONE) {
            return JOB_ERASE;
        }
        *sec = victim(s); /* plans within the pool it has */
        return *sec != NONE ? JOB_COLLECT : JOB_NONE;
    }
    *sec = wear_victim(s);
    return *sec != NONE ? JOB_COLLECT : JOB_NONE;
}

static int next_job(store* s) {
    uint32_t sec;
    int job = due(s, &sec);
    if (job != JOB_NONE) {
        start_job(s, job, sec);
    }
    return job != JOB_NONE;
}

static void job_step(store* s) {
    if (s->job == JOB_ERASE) {
        erase_step(s);
    } else {
        collect_step(s);
    }
}

/* ---- writing a file ---- */

static void chunk_header(const store_file_writer* f, uint8_t* h) {
    h[0] = STORE_TYPE_CHUNK;
    h[1] = 0;
    wr16(h + 2, chunk_len(f->length, f->chunk));
    wr32(h + 4, f->fid);
    wr32(h + 8, f->chunk);
    wr32(h + 12, f->rcrc);
}

static void chunk_start(store_file_writer* f) {
    uint8_t h[STORE_REC_HEADER];
    f->cdone = 0;
    f->pdone = 0;
    f->rcrc = 0;
    chunk_header(f, h);
    f->rcrc = store_crc32(0, h, 12);
}

/* Drops the open file: sectors never programmed go back to the pool, the others to garbage
 * collection. */
static void file_fail(store* s, int result) {
    store_file_writer* f = &s->f;
    if (f->phase == F_IDLE || f->phase == F_FILE) {
        return;
    }
    /* A claim cut short (or failed: the hook may have cleared bits) is not clean any more. */
    uint32_t torn = f->phase == F_CLAIM && (f->pdone || result == STORE_ERROR)
                        ? f->sectors[f->chunk / 3u]
                        : NONE;
    for (uint32_t i = 0; i < f->nsec; i++) {
        uint32_t sec = f->sectors[i];
        s->sstate[sec] &= (uint8_t)~SEC_FILE;
        if (sec_state(s, sec) == SEC_RESERVED) {
            set_state(s, sec, sec == torn ? SEC_DIRTY : SEC_CLEAN);
        } else {
            s->sstate[sec] |= SEC_FULL; /* a chunk may be cut after its last committed one */
        }
    }
    s->fres_sec = NONE;
    s->fres_bytes = 0;
    f->phase = F_IDLE;
    f->result = (int8_t)result;
}

/* The open file's FILE record, into the room kept for it. */
static void file_commit(store* s) {
    store_file_writer* f = &s->f;
    uint8_t* d = s->buf;
    wr32(d, f->fid);
    wr32(d + 4, f->length);
    wr32(d + 8, f->crc);
    d[12] = f->type;
    d[13] = f->nsec;
    wr16(d + 14, chunks_of(f->length));
    memcpy(d + 16, f->name, STORE_NAME_MAX);
    memcpy(d + STORE_FILE_HEAD, f->sectors, f->nsec);
    uint32_t sec = s->fres_sec, size = s->fres_bytes;
    s->fres_sec = NONE;
    s->fres_bytes = 0;
    if ((s->sstate[sec] & SEC_FULL) ||
        (sec_state(s, sec) != SEC_CLAIMED && sec_state(s, sec) != SEC_RESERVED)) {
        sec = place(s, size, 0); /* a write failed there meanwhile: the place moves */
        if (sec == NONE) {
            f->phase = F_SEALED;
            file_fail(s, STORE_ERROR);
            return;
        }
    }
    f->phase = F_FILE;
    write_at(s, sec, f->key, STORE_TYPE_FILE, d, STORE_FILE_HEAD + f->nsec);
    writer_step(s);
}

/* The open file's next operation. 1 if it ran one (or started its FILE record), 0 while it waits
 * for bytes or for store_file_end. */
static int file_step(store* s) {
    store_file_writer* f = &s->f;
    uint32_t sec = f->sectors[f->chunk / 3u];
    int rc;
    if (f->phase == F_SEALED) {
        if (!f->closing) {
            return 0;
        }
        file_commit(s);
        return 1;
    }
    if (f->phase == F_FILE) {
        return 0;
    }
    if (f->phase == F_DATA && f->cdone == 0 && sec_state(s, sec) == SEC_RESERVED) {
        f->phase = F_CLAIM;
        f->pdone = 0;
    }
    if (f->phase == F_CLAIM) {
        uint8_t h[STORE_HEADER];
        sector_header(s, sec, h);
        rc = prog_piece(s, sec, 0, h, STORE_HEADER, &f->pdone);
        if (rc > 0) {
            set_state(s, sec, SEC_CLAIMED);
            f->phase = F_DATA;
            f->pdone = 0;
        }
    } else if (f->phase == F_DATA) {
        uint32_t len = chunk_len(f->length, f->chunk);
        uint32_t a = base(sec) + chunk_off(f->chunk) + STORE_REC_HEADER + f->cdone;
        uint32_t k = len - f->cdone, page = STORE_PAGE - (a & (STORE_PAGE - 1u));
        k = k > s->window ? s->window : k;
        k = k > page ? page : k;
        k = k > STORE_WINDOW_MAX - f->start ? STORE_WINDOW_MAX - f->start : k; /* the ring's end */
        if (f->fill < k) {
            return 0; /* waiting for bytes */
        }
        const uint8_t* src = s->buf + f->start;
        uint16_t done = 0;
        rc = prog_piece(s, sec, chunk_off(f->chunk) + STORE_REC_HEADER + f->cdone, src, k, &done);
        if (rc > 0) {
            f->rcrc = crc(s, f->rcrc, src, k);
            f->crc = crc(s, f->crc, src, k);
            f->fill = (uint16_t)(f->fill - k);
            f->start = (uint16_t)((f->start + k) % STORE_WINDOW_MAX);
            f->cdone = (uint16_t)(f->cdone + k);
            if (f->cdone == len) {
                f->phase = F_HEAD;
                f->pdone = 0;
            }
        }
    } else if (f->phase == F_HEAD) {
        uint8_t h[STORE_REC_HEADER];
        chunk_header(f, h);
        rc = prog_piece(s, sec, chunk_off(f->chunk), h, STORE_REC_HEADER, &f->pdone);
        if (rc > 0) {
            f->phase = F_COMMIT;
            f->pdone = 0;
        }
    } else {
        static const uint8_t zero[4] = {0, 0, 0, 0};
        uint32_t len = chunk_len(f->length, f->chunk);
        rc = prog_piece(s, sec, chunk_off(f->chunk) + STORE_REC_HEADER + pad4(len), zero, 4,
                        &f->pdone);
        if (rc > 0) {
            s->sector[sec].free = (uint16_t)(chunk_off(f->chunk) + rec_size(len));
            if (++f->chunk == chunks_of(f->length)) {
                f->phase = F_SEALED;
            } else {
                chunk_start(f);
                f->phase = F_DATA;
            }
        }
    }
    if (rc < 0) {
        file_fail(s, STORE_ERROR);
    }
    return 1;
}

/* ---- names ---- */

/* `name` padded with NULs to STORE_NAME_MAX bytes; 0 if it is not a file name. */
static int name_pad(const char* name, char* out) {
    uint32_t n = 0;
    while (name && n <= STORE_NAME_MAX && name[n]) {
        n++;
    }
    if (!name || n == 0 || n > STORE_NAME_MAX || name[0] == ' ') {
        return 0;
    }
    for (uint32_t i = 0; i < STORE_NAME_MAX; i++) {
        char c = i < n ? name[i] : 0;
        if (i < n && (c < 0x20 || c > 0x5F)) {
            return 0;
        }
        out[i] = c;
    }
    return 1;
}

static int name_cmp(const char* a, const char* b) {
    for (uint32_t i = 0; i < STORE_NAME_MAX; i++) {
        if (a[i] != b[i]) {
            return (uint8_t)a[i] < (uint8_t)b[i] ? -1 : 1;
        }
    }
    return 0;
}

/* The key of the committed file named `name` (padded), or 0. */
static store_key* file_find(store* s, const char* name) {
    for (uint32_t i = 0; i < s->nkeys; i++) {
        const uint8_t* rec = at(s, s->key[i].sec, s->key[i].off);
        if (rec[0] == STORE_TYPE_FILE && name_cmp((const char*)file_data(rec) + 16, name) == 0) {
            return &s->key[i];
        }
    }
    return 0;
}

/* A key for a new file: a deleted file's, or the first free one. 0 if the index is full. */
static uint32_t file_key(store* s) {
    for (uint32_t i = 0; i < s->nkeys; i++) {
        if (s->key[i].key >= STORE_KEY_FILE &&
            at(s, s->key[i].sec, s->key[i].off)[0] == STORE_TYPE_TOMB) {
            return s->key[i].key;
        }
    }
    if (keys_taken(s) >= STORE_KEYS_MAX) {
        return 0;
    }
    for (uint32_t k = STORE_KEY_FILE;; k++) {
        if (!find(s, k)) {
            return k;
        }
    }
}

static void file_fill(const uint8_t* rec, const store_key* e, store_file* out) {
    const uint8_t* d = file_data(rec);
    out->key = e->key;
    out->fid = rd32(d);
    out->length = rd32(d + 4);
    out->crc = rd32(d + 8);
    out->type = d[12];
    out->nsec = d[13];
    out->chunks = rd16(d + 14);
    memcpy(out->name, d + 16, STORE_NAME_MAX);
    out->name[STORE_NAME_MAX] = 0;
    memset(out->sectors, 0xFF, sizeof out->sectors);
    memcpy(out->sectors, d + STORE_FILE_HEAD, out->nsec);
}

/* ---- completing the interrupted write ---- */

/* A sector whose records end in a cut header (SEC_TORN), or NONE. */
/* A cut header's first (the one a cut during the fix could still bring back), then a cut kill,
 * then a twin's sector. */
static uint32_t torn(const store* s) {
    uint32_t best = NONE, rank = 0;
    work(s, WORK_SEC * STORE_SECTORS);
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        uint32_t r = !(s->sstate[i] & SEC_TORN) ? (s->sstate[i] & SEC_REDO ? 1u : 0u)
                     : sec_state(s, i) == SEC_DIRTY ? 2u
                                                     : 3u;
        if (r > rank) {
            best = i;
            rank = r;
        }
    }
    return best;
}

/* The offset of the last record of `sec`'s records, or 0 if it has none. */
static uint32_t last_record(const store* s, uint32_t sec) {
    uint32_t last = 0;
    for (uint32_t off = STORE_HEADER; off < s->sector[sec].free;) {
        last = off;
        off += rec_size(rd16(at(s, sec, off) + 2));
    }
    return last;
}

/* Programs a cut header to zero (it can never read valid again, whatever its faint bits read),
 * then the newest record's header and commit word again, the same bytes, so bits a cut left
 * faint in them are programmed for good (newest). */
static void fix_step(store* s) {
    static const uint8_t zero[STORE_REC_HEADER] = {0};
    uint32_t t = torn(s);
    if (t != NONE && !(s->sstate[t] & SEC_TORN)) {
        /* A twin's sector: its last record's header, then its commit word, again. */
        uint32_t off = last_record(s, t);
        if (!off || sec_state(s, t) != SEC_CLAIMED) {
            s->sstate[t] &= (uint8_t)~SEC_REDO;
            return;
        }
        const uint8_t* h = at(s, t, off);
        uint8_t head[STORE_REC_HEADER];
        memcpy(head, h, STORE_REC_HEADER);
        int commit = s->redo_phase;
        int rc = commit ? prog_piece(s, t, off + STORE_REC_HEADER + pad4(rd16(h + 2)), zero, 4,
                                     &s->fix_done)
                        : prog_piece(s, t, off, head, STORE_REC_HEADER, &s->fix_done);
        if (rc != 0) {
            s->fix_done = 0;
            s->redo_phase = (uint8_t)(rc > 0 && !commit);
            if (rc < 0 || commit) {
                s->sstate[t] &= (uint8_t)~SEC_REDO;
            }
        }
        return;
    }
    if (t != NONE) {
        int dirty = sec_state(s, t) == SEC_DIRTY; /* a cut kill: the magic again */
        int rc = prog_piece(s, t, dirty ? 0u : s->sector[t].free, zero, dirty ? 8u : STORE_REC_HEADER,
                            &s->fix_done);
        if (rc != 0) {
            s->sstate[t] &= (uint8_t)~SEC_TORN; /* done, or failed (counted in errors) */
            s->fix_done = 0;
        }
        return;
    }
    const uint8_t* h = at(s, s->fix_sec, s->fix_off);
    int rc;
    if (s->fix_kind == FIX_HEAD) {
        uint8_t head[STORE_REC_HEADER];
        memcpy(head, h, STORE_REC_HEADER);
        rc = prog_piece(s, s->fix_sec, s->fix_off, head, STORE_REC_HEADER, &s->fix_done);
        if (rc > 0) {
            s->fix_kind = FIX_COMMIT;
            s->fix_done = 0;
            return;
        }
    } else {
        uint32_t at_commit = s->fix_off + STORE_REC_HEADER + pad4(rd16(h + 2));
        rc = prog_piece(s, s->fix_sec, at_commit, zero, 4, &s->fix_done);
        if (rc > 0) {
            s->fix_kind = FIX_NONE;
            return;
        }
    }
    if (rc < 0) {
        s->fix_kind = FIX_NONE; /* counted in errors; it may stay faint */
    }
}

/* 1 when the next operation must wait for the fix: any record written (the caller's, a file's,
 * a collection's copy: a cut during it must not find a cut header reading valid, or the newest
 * record faint), and a collection's kill, which could drop older records. Erases (and the kill
 * of a dirty sector, and the erase header) may go first. */
static int fix_first(const store* s) {
    if (s->fix_kind == FIX_NONE && torn(s) == NONE) {
        return 0;
    }
    return s->w.phase != W_IDLE || s->f.phase != F_IDLE || s->job == JOB_COLLECT;
}

/* ---- the API ---- */

void store_init(store* s, const store_flash* flash, uint32_t window) {
    memset(s, 0, sizeof *s);
    s->flash = *flash;
    s->window = window == 0 || window > STORE_WINDOW_MAX ? STORE_WINDOW_MAX : window;
    s->head = NONE;
    s->fres_sec = NONE;
    s->f.result = STORE_NOT_FOUND;
    s->state = STORE_STATE_UNMOUNTED;
}

void store_mount(store* s) {
    store_flash flash = s->flash;
    store_init(s, &flash, s->window);
    s->state = STORE_STATE_MOUNTING;
    s->mount_blank = 1;
    s->seq = 1;
}

store_state_id store_state(const store* s) {
    return (store_state_id)s->state;
}

int store_busy(const store* s) {
    return s->state == STORE_STATE_MOUNTING || s->w.phase != W_IDLE;
}

void store_quiet(store* s, int quiet) {
    s->quiet = quiet != 0;
}

int store_erase_wanted(const store* s) {
    if (!writable(s)) {
        return 0;
    }
    uint32_t sec;
    return s->f.phase == F_IDLE && (s->job != JOB_NONE || due(s, &sec) != JOB_NONE);
}

int store_poll(store* s) {
    s->work = 0;
    if (s->state == STORE_STATE_MOUNTING) {
        mount_step(s);
        return 1;
    }
    if (!writable(s)) {
        return 0;
    }
    /* Maintenance waits for an open file. */
    if (s->w.phase == W_IDLE && s->f.phase == F_IDLE && s->job == JOB_NONE &&
        !(s->quiet && next_job(s))) {
        return 0;
    }
    if (fix_first(s)) {
        fix_step(s);
        return 1;
    }
    if (s->w.phase != W_IDLE) {
        writer_step(s);
        return 1;
    }
    if (s->f.phase != F_IDLE) {
        return file_step(s);
    }
    job_step(s);
    return 1;
}

int store_get(const store* s, uint32_t key, void* buf, uint32_t cap, uint8_t* type) {
    if (!writable(s)) {
        return STORE_NOT_FOUND;
    }
    const store_key* e = find_c(s, key);
    if (!e) {
        return STORE_NOT_FOUND;
    }
    const uint8_t* h = at(s, e->sec, e->off);
    if (h[0] == STORE_TYPE_TOMB) {
        return STORE_NOT_FOUND;
    }
    uint32_t len = rd16(h + 2);
    if (len && cap) {
        memcpy(buf, h + STORE_REC_HEADER, len < cap ? len : cap);
    }
    if (type) {
        *type = h[0];
    }
    return (int)len;
}

static int write_check(const store* s) {
    if (s->state == STORE_STATE_FOREIGN) {
        return STORE_FOREIGN;
    }
    if (!writable(s) || s->w.phase != W_IDLE) {
        return STORE_BUSY;
    }
    return STORE_OK;
}

int store_put(store* s, uint32_t key, uint8_t type, const void* data, uint32_t len) {
    if (key > STORE_KEY_MAX || type > STORE_TYPE_MAX || len > STORE_VALUE_MAX || (!data && len)) {
        return STORE_INVALID;
    }
    int rc = write_check(s);
    return rc ? rc : write_record(s, key, type, data, len, STORE_POOL_MIN);
}

static int delete_key(store* s, uint32_t key) {
    int rc;
    store_key* e = find(s, key);
    if (!e || at(s, e->sec, e->off)[0] == STORE_TYPE_TOMB) {
        return STORE_OK;
    }
    rc = write_record(s, key, STORE_TYPE_TOMB, 0, 0, STORE_POOL_MIN);
    /* A full store must still take a deletion: it may take the pool one under its minimum. */
    return rc == STORE_FULL ? write_record(s, key, STORE_TYPE_TOMB, 0, 0, STORE_POOL_MIN - 1u) : rc;
}

int store_delete(store* s, uint32_t key) {
    if (key > STORE_KEY_MAX) {
        return STORE_INVALID;
    }
    int rc = write_check(s);
    return rc ? rc : delete_key(s, key);
}

int store_collect(store* s) {
    if (!writable(s) || s->job != JOB_NONE || s->w.phase != W_IDLE || s->f.phase != F_IDLE) {
        return writable(s) ? STORE_BUSY : STORE_FOREIGN;
    }
    uint32_t v = victim(s);
    if (v == NONE) {
        return STORE_NOT_FOUND;
    }
    start_job(s, JOB_COLLECT, v);
    return STORE_OK;
}

void store_format(store* s) {
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        set_state(s, i, SEC_DIRTY);
        s->sector[i].live = 0;
        s->sector[i].free = STORE_HEADER;
    }
    s->nkeys = 0;
    s->live = 0;
    s->head = NONE;
    s->job = JOB_NONE;
    s->fix_kind = FIX_NONE;
    s->w.phase = W_IDLE;
    if (s->f.phase != F_IDLE) {
        s->f.phase = F_IDLE;
        s->f.result = STORE_NOT_FOUND;
    }
    s->fres_sec = NONE;
    s->fres_bytes = 0;
    s->state = STORE_STATE_BLANK;
}

void store_stats_get(const store* s, store_stats* out) {
    memset(out, 0, sizeof *out);
    out->live = s->live;
    out->free = s->live < STORE_CAPACITY ? STORE_CAPACITY - s->live : 0u;
    out->pool = count(s, SEC_CLEAN);
    out->dirty = count(s, SEC_DIRTY);
    out->claimed = count(s, SEC_CLAIMED) + count(s, SEC_RESERVED);
    out->erases_min = 0xFFFFFFFFu;
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        uint32_t e = s->sector[i].erases;
        out->erases_min = e < out->erases_min ? e : out->erases_min;
        out->erases_max = e > out->erases_max ? e : out->erases_max;
    }
    out->keys = s->nkeys;
    out->corrupt = s->corrupt;
    out->errors = s->errors;
}

/* ---- files ---- */

int store_file_begin(store* s, const char* name, uint8_t type, uint32_t length) {
    char padded[STORE_NAME_MAX];
    if (!name_pad(name, padded) || type == 0 || type > STORE_TYPE_MAX || length > STORE_FILE_MAX) {
        return STORE_INVALID;
    }
    int rc = write_check(s);
    if (rc) {
        return rc;
    }
    if (s->f.phase != F_IDLE || s->job == JOB_COLLECT) {
        return STORE_BUSY; /* one file at a time; a collection's copies use the buffer */
    }
    store_key* e = file_find(s, padded);
    uint32_t key = e ? e->key : file_key(s);
    if (!key) {
        return STORE_FULL;
    }
    uint32_t nsec = sectors_of(length), frec = rec_size(STORE_FILE_HEAD + nsec);
    /* A file it replaces frees its records when this one commits. */
    uint32_t old =
        e ? entry_size(s, e) + chunk_bytes(rd32(file_data(at(s, e->sec, e->off)) + 4)) : 0u;
    if (s->live - old + chunk_bytes(length) + frec > STORE_CAPACITY) {
        return STORE_FULL;
    }
    uint32_t h = s->head, head_room = h != NONE && !(s->sstate[h] & SEC_FULL) &&
                                      s->sector[h].free + frec <= STORE_SECTOR_USE;
    uint32_t need = nsec + !head_room, pool = count(s, SEC_CLEAN);
    if (pool < need + STORE_GC_RESERVE) {
        return pool + reclaimable(s) >= need + STORE_GC_RESERVE ? STORE_BUSY : STORE_FULL;
    }
    store_file_writer* f = &s->f;
    for (uint32_t i = 0; i < nsec; i++) {
        uint32_t sec = coldest(s, SEC_CLEAN);
        set_state(s, sec, SEC_RESERVED);
        s->sstate[sec] |= SEC_FILE;
        s->sector[sec].seq = s->sector[sec].erases;
        s->sector[sec].free = STORE_HEADER;
        s->sector[sec].live = 0;
        f->sectors[i] = (uint8_t)sec;
    }
    s->fres_sec = (uint8_t)(head_room ? h : place(s, frec, 0));
    s->fres_bytes = (uint16_t)frec;
    f->key = key;
    f->fid = s->seq++;
    f->length = length;
    f->taken = 0;
    f->crc = 0;
    f->chunk = 0;
    f->fill = 0;
    f->start = 0;
    f->closing = 0;
    f->idle = 0;
    f->type = type;
    f->nsec = (uint8_t)nsec;
    memcpy(f->name, padded, STORE_NAME_MAX);
    f->result = STORE_BUSY;
    chunk_start(f);
    f->phase = length ? F_DATA : F_SEALED;
    return STORE_OK;
}

uint32_t store_file_write(store* s, const void* p, uint32_t n) {
    store_file_writer* f = &s->f;
    if (f->phase == F_IDLE || f->phase == F_FILE || f->closing || !n) {
        return 0;
    }
    uint32_t room = STORE_WINDOW_MAX - f->fill, left = f->length - f->taken;
    n = n > room ? room : n;
    n = n > left ? left : n;
    uint32_t to = (f->start + f->fill) % STORE_WINDOW_MAX, first = STORE_WINDOW_MAX - to;
    first = first > n ? n : first;
    memcpy(s->buf + to, p, first);
    memcpy(s->buf, (const uint8_t*)p + first, n - first);
    f->fill = (uint16_t)(f->fill + n);
    f->taken += n;
    f->idle = 0;
    return n;
}

int store_file_end(store* s) {
    store_file_writer* f = &s->f;
    if (f->phase == F_IDLE || f->phase == F_FILE || f->closing) {
        return STORE_INVALID;
    }
    if (f->taken < f->length) {
        file_fail(s, STORE_INVALID);
        return STORE_INVALID;
    }
    f->closing = 1;
    return STORE_OK;
}

void store_file_abort(store* s) {
    file_fail(s, STORE_NOT_FOUND);
}

int store_file_result(const store* s) {
    return s->f.phase != F_IDLE ? STORE_BUSY : s->f.result;
}

void store_file_idle(store* s, uint32_t ms) {
    store_file_writer* f = &s->f;
    if (f->phase == F_IDLE || f->phase == F_FILE || f->closing || f->fill == STORE_WINDOW_MAX) {
        return; /* no file waiting for bytes; or the store, not the sender, is the slow one */
    }
    uint32_t idle = f->idle + (ms < STORE_FILE_IDLE_MS ? ms : STORE_FILE_IDLE_MS);
    f->idle = (uint16_t)(idle < STORE_FILE_IDLE_MS ? idle : STORE_FILE_IDLE_MS);
    if (f->idle >= STORE_FILE_IDLE_MS) {
        file_fail(s, STORE_TIMEOUT);
    }
}

int store_file_open(const store* s, const char* name, store_file* f) {
    char padded[STORE_NAME_MAX];
    if (!writable(s) || !name_pad(name, padded)) {
        return STORE_NOT_FOUND;
    }
    const store_key* e = file_find((store*)s, padded);
    if (!e) {
        return STORE_NOT_FOUND;
    }
    file_fill(at(s, e->sec, e->off), e, f);
    return STORE_OK;
}

int store_file_read(const store* s, const store_file* f, uint32_t off, void* p, uint32_t n) {
    const store_key* e = writable(s) ? find_c(s, f->key) : 0;
    const uint8_t* rec = e ? at(s, e->sec, e->off) : 0;
    if (!rec || rec[0] != STORE_TYPE_FILE || rd32(file_data(rec)) != f->fid) {
        return STORE_NOT_FOUND;
    }
    if (off >= f->length) {
        return 0;
    }
    n = n > f->length - off ? f->length - off : n;
    uint8_t* out = p;
    for (uint32_t done = 0; done < n;) {
        uint32_t i = (off + done) / STORE_CHUNK, o = (off + done) % STORE_CHUNK;
        uint32_t k = chunk_len(f->length, i) - o;
        k = k > n - done ? n - done : k;
        memcpy(out + done, at(s, f->sectors[i / 3u], chunk_off(i) + STORE_REC_HEADER + o), k);
        done += k;
    }
    return (int)n;
}

int store_dir_next(const store* s, store_file* f) {
    char prev[STORE_NAME_MAX];
    const uint8_t* best = 0;
    const store_key* best_e = 0;
    if (!writable(s)) {
        return 0;
    }
    uint32_t n = 0;
    while (n < STORE_NAME_MAX && f->name[n]) {
        n++;
    }
    for (uint32_t i = 0; i < STORE_NAME_MAX; i++) {
        prev[i] = i < n ? f->name[i] : 0;
    }
    for (uint32_t i = 0; i < s->nkeys; i++) {
        const uint8_t* rec = at(s, s->key[i].sec, s->key[i].off);
        if (rec[0] != STORE_TYPE_FILE) {
            continue;
        }
        const char* name = (const char*)file_data(rec) + 16;
        if ((!f->name[0] || name_cmp(name, prev) > 0) &&
            (!best || name_cmp(name, (const char*)file_data(best) + 16) < 0)) {
            best = rec;
            best_e = &s->key[i];
        }
    }
    if (!best) {
        return 0;
    }
    file_fill(best, best_e, f);
    return 1;
}

int store_file_delete(store* s, const char* name) {
    char padded[STORE_NAME_MAX];
    if (!name_pad(name, padded)) {
        return STORE_INVALID;
    }
    int rc = write_check(s);
    if (rc) {
        return rc;
    }
    store_key* e = file_find(s, padded);
    return e ? delete_key(s, e->key) : STORE_NOT_FOUND;
}

int store_file_rename(store* s, const char* from, const char* to) {
    char a[STORE_NAME_MAX], b[STORE_NAME_MAX];
    if (!name_pad(from, a) || !name_pad(to, b)) {
        return STORE_INVALID;
    }
    int rc = write_check(s);
    if (rc) {
        return rc;
    }
    if (s->f.phase != F_IDLE || s->job == JOB_COLLECT) {
        return STORE_BUSY; /* the buffer is the open file's or the collection's */
    }
    store_key* e = file_find(s, a);
    if (!e) {
        return STORE_NOT_FOUND;
    }
    store_key* other = file_find(s, b);
    if (other) {
        return other == e ? STORE_OK : STORE_EXISTS;
    }
    const uint8_t* rec = at(s, e->sec, e->off);
    uint32_t len = rd16(rec + 2);
    memcpy(s->buf, file_data(rec), len);
    memcpy(s->buf + 16, b, STORE_NAME_MAX);
    return write_record(s, e->key, STORE_TYPE_FILE, s->buf, len, STORE_POOL_MIN);
}
