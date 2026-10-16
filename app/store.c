// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The store's core (store.h); the format is docs/reference/store-format.md. */
#include "store.h"

#include <string.h>

/* A sector's state in sstate's low bits, and flags. */
enum {
    SEC_DIRTY = 0, /* to be erased before any use */
    SEC_CLEAN = 1, /* erase header valid, the rest erased: in the pool */
    SEC_RESERVED = 2, /* taken from the pool in RAM; its claim is not programmed yet */
    SEC_CLAIMED = 3, /* claimed: records from STORE_HEADER up to `free` */
    SEC_STATE = 0x0F,
    SEC_FULL = 0x10, /* nothing more is written into it */
    SEC_GUESS = 0x20, /* the mount could not read its erase count */
};

enum { W_IDLE, W_CLAIM, W_DATA, W_HEAD, W_COMMIT };
enum { JOB_NONE, JOB_ERASE, JOB_COLLECT };
enum { E_KILL, E_ERASE, E_HEADER };
enum { C_SCAN, C_KILL };

#define NONE 0xFFu
#define TOMB_SIZE STORE_REC_OVERHEAD

static const uint8_t MAGIC[8] = {'S', 'I', 'D', 'O', 'U', 'S', 'T', 'R'};

static const uint32_t CRC_TABLE[256] = {
    0x00000000u, 0x77073096u, 0xEE0E612Cu, 0x990951BAu, 0x076DC419u, 0x706AF48Fu,
    0xE963A535u, 0x9E6495A3u, 0x0EDB8832u, 0x79DCB8A4u, 0xE0D5E91Eu, 0x97D2D988u,
    0x09B64C2Bu, 0x7EB17CBDu, 0xE7B82D07u, 0x90BF1D91u, 0x1DB71064u, 0x6AB020F2u,
    0xF3B97148u, 0x84BE41DEu, 0x1ADAD47Du, 0x6DDDE4EBu, 0xF4D4B551u, 0x83D385C7u,
    0x136C9856u, 0x646BA8C0u, 0xFD62F97Au, 0x8A65C9ECu, 0x14015C4Fu, 0x63066CD9u,
    0xFA0F3D63u, 0x8D080DF5u, 0x3B6E20C8u, 0x4C69105Eu, 0xD56041E4u, 0xA2677172u,
    0x3C03E4D1u, 0x4B04D447u, 0xD20D85FDu, 0xA50AB56Bu, 0x35B5A8FAu, 0x42B2986Cu,
    0xDBBBC9D6u, 0xACBCF940u, 0x32D86CE3u, 0x45DF5C75u, 0xDCD60DCFu, 0xABD13D59u,
    0x26D930ACu, 0x51DE003Au, 0xC8D75180u, 0xBFD06116u, 0x21B4F4B5u, 0x56B3C423u,
    0xCFBA9599u, 0xB8BDA50Fu, 0x2802B89Eu, 0x5F058808u, 0xC60CD9B2u, 0xB10BE924u,
    0x2F6F7C87u, 0x58684C11u, 0xC1611DABu, 0xB6662D3Du, 0x76DC4190u, 0x01DB7106u,
    0x98D220BCu, 0xEFD5102Au, 0x71B18589u, 0x06B6B51Fu, 0x9FBFE4A5u, 0xE8B8D433u,
    0x7807C9A2u, 0x0F00F934u, 0x9609A88Eu, 0xE10E9818u, 0x7F6A0DBBu, 0x086D3D2Du,
    0x91646C97u, 0xE6635C01u, 0x6B6B51F4u, 0x1C6C6162u, 0x856530D8u, 0xF262004Eu,
    0x6C0695EDu, 0x1B01A57Bu, 0x8208F4C1u, 0xF50FC457u, 0x65B0D9C6u, 0x12B7E950u,
    0x8BBEB8EAu, 0xFCB9887Cu, 0x62DD1DDFu, 0x15DA2D49u, 0x8CD37CF3u, 0xFBD44C65u,
    0x4DB26158u, 0x3AB551CEu, 0xA3BC0074u, 0xD4BB30E2u, 0x4ADFA541u, 0x3DD895D7u,
    0xA4D1C46Du, 0xD3D6F4FBu, 0x4369E96Au, 0x346ED9FCu, 0xAD678846u, 0xDA60B8D0u,
    0x44042D73u, 0x33031DE5u, 0xAA0A4C5Fu, 0xDD0D7CC9u, 0x5005713Cu, 0x270241AAu,
    0xBE0B1010u, 0xC90C2086u, 0x5768B525u, 0x206F85B3u, 0xB966D409u, 0xCE61E49Fu,
    0x5EDEF90Eu, 0x29D9C998u, 0xB0D09822u, 0xC7D7A8B4u, 0x59B33D17u, 0x2EB40D81u,
    0xB7BD5C3Bu, 0xC0BA6CADu, 0xEDB88320u, 0x9ABFB3B6u, 0x03B6E20Cu, 0x74B1D29Au,
    0xEAD54739u, 0x9DD277AFu, 0x04DB2615u, 0x73DC1683u, 0xE3630B12u, 0x94643B84u,
    0x0D6D6A3Eu, 0x7A6A5AA8u, 0xE40ECF0Bu, 0x9309FF9Du, 0x0A00AE27u, 0x7D079EB1u,
    0xF00F9344u, 0x8708A3D2u, 0x1E01F268u, 0x6906C2FEu, 0xF762575Du, 0x806567CBu,
    0x196C3671u, 0x6E6B06E7u, 0xFED41B76u, 0x89D32BE0u, 0x10DA7A5Au, 0x67DD4ACCu,
    0xF9B9DF6Fu, 0x8EBEEFF9u, 0x17B7BE43u, 0x60B08ED5u, 0xD6D6A3E8u, 0xA1D1937Eu,
    0x38D8C2C4u, 0x4FDFF252u, 0xD1BB67F1u, 0xA6BC5767u, 0x3FB506DDu, 0x48B2364Bu,
    0xD80D2BDAu, 0xAF0A1B4Cu, 0x36034AF6u, 0x41047A60u, 0xDF60EFC3u, 0xA867DF55u,
    0x316E8EEFu, 0x4669BE79u, 0xCB61B38Cu, 0xBC66831Au, 0x256FD2A0u, 0x5268E236u,
    0xCC0C7795u, 0xBB0B4703u, 0x220216B9u, 0x5505262Fu, 0xC5BA3BBEu, 0xB2BD0B28u,
    0x2BB45A92u, 0x5CB36A04u, 0xC2D7FFA7u, 0xB5D0CF31u, 0x2CD99E8Bu, 0x5BDEAE1Du,
    0x9B64C2B0u, 0xEC63F226u, 0x756AA39Cu, 0x026D930Au, 0x9C0906A9u, 0xEB0E363Fu,
    0x72076785u, 0x05005713u, 0x95BF4A82u, 0xE2B87A14u, 0x7BB12BAEu, 0x0CB61B38u,
    0x92D28E9Bu, 0xE5D5BE0Du, 0x7CDCEFB7u, 0x0BDBDF21u, 0x86D3D2D4u, 0xF1D4E242u,
    0x68DDB3F8u, 0x1FDA836Eu, 0x81BE16CDu, 0xF6B9265Bu, 0x6FB077E1u, 0x18B74777u,
    0x88085AE6u, 0xFF0F6A70u, 0x66063BCAu, 0x11010B5Cu, 0x8F659EFFu, 0xF862AE69u,
    0x616BFFD3u, 0x166CCF45u, 0xA00AE278u, 0xD70DD2EEu, 0x4E048354u, 0x3903B3C2u,
    0xA7672661u, 0xD06016F7u, 0x4969474Du, 0x3E6E77DBu, 0xAED16A4Au, 0xD9D65ADCu,
    0x40DF0B66u, 0x37D83BF0u, 0xA9BCAE53u, 0xDEBB9EC5u, 0x47B2CF7Fu, 0x30B5FFE9u,
    0xBDBDF21Cu, 0xCABAC28Au, 0x53B39330u, 0x24B4A3A6u, 0xBAD03605u, 0xCDD70693u,
    0x54DE5729u, 0x23D967BFu, 0xB3667A2Eu, 0xC4614AB8u, 0x5D681B02u, 0x2A6F2B94u,
    0xB40BBE37u, 0xC30C8EA1u, 0x5A05DF1Bu, 0x2D02EF8Du
};

uint32_t store_crc32(uint32_t crc, const void* p, uint32_t n) {
    const uint8_t* b = p;
    crc = ~crc;
    while (n--) {
        crc = CRC_TABLE[(crc ^ *b++) & 0xFFu] ^ (crc >> 8);
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

static const uint8_t* at(const store* s, uint32_t sec, uint32_t off) {
    return s->flash.map(s->flash.ctx, base(sec) + off);
}

static int all_ff(const uint8_t* p, uint32_t n) {
    for (uint32_t i = 0; i < n; i++) {
        if (p[i] != 0xFFu) {
            return 0;
        }
    }
    return 1;
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

static void drop_key(store* s, store_key* e) {
    *e = s->key[--s->nkeys];
}

static int keyed(uint8_t type) {
    return type <= STORE_TYPE_MAX || type == STORE_TYPE_TOMB;
}

static uint32_t entry_size(const store* s, const store_key* e) {
    return rec_size(rd16(at(s, e->sec, e->off) + 2));
}

/* ---- the pool ---- */

static uint32_t count(const store* s, int state) {
    uint32_t n = 0;
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        n += sec_state(s, i) == state;
    }
    return n;
}

/* The sector in `state` with the lowest erase count, or NONE. */
static uint32_t coldest(const store* s, int state) {
    uint32_t best = NONE;
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        if (sec_state(s, i) == state && (best == NONE || s->sector[i].erases < s->sector[best].erases)) {
            best = i;
        }
    }
    return best;
}

/* A sector garbage collection may take: claimed, not written to, with at most half its
 * records live, so its copies fit a fresh sector whatever the fragmentation. */
static int viable(const store* s, uint32_t sec) {
    return sec_state(s, sec) == SEC_CLAIMED && sec != s->head &&
           !(s->job != JOB_NONE && s->job_sec == sec) && 2u * s->sector[sec].live <= STORE_RECORDS;
}

/* The best victim: the least live data, the oldest on a tie. */
static uint32_t victim(const store* s) {
    uint32_t best = NONE;
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        if (!viable(s, i)) {
            continue;
        }
        const store_sector* c = &s->sector[i];
        if (best == NONE || c->live < s->sector[best].live ||
            (c->live == s->sector[best].live && c->seq < s->sector[best].seq)) {
            best = i;
        }
    }
    return best;
}

/* Sectors maintenance could still add to the pool: the dirty ones, and what collecting the
 * viable ones gains (their copies fill at least STORE_RECORDS - a value's record a sector). */
static uint32_t reclaimable(const store* s) {
    uint32_t n = 0, live = 0;
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        if (viable(s, i)) {
            n++;
            live += s->sector[i].live;
        }
    }
    uint32_t per = STORE_RECORDS - STORE_VALUE_MAX - STORE_REC_OVERHEAD;
    uint32_t used = (live + per - 1u) / per;
    return count(s, SEC_DIRTY) + (n > used ? n - used : 0u);
}

/* The claimed sector holding the oldest erase count, if the counts spread too far. */
static uint32_t wear_victim(const store* s) {
    uint32_t max = 0, cold = NONE;
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        max = s->sector[i].erases > max ? s->sector[i].erases : max;
        if (sec_state(s, i) == SEC_CLAIMED && i != s->head &&
            (cold == NONE || s->sector[i].erases < s->sector[cold].erases)) {
            cold = i;
        }
    }
    return cold != NONE && max - s->sector[cold].erases > STORE_WEAR_SPREAD ? cold : NONE;
}

/* A place for `size` bytes of records in the head, or a new head from the pool if that
 * leaves more than `floor` sectors in it. The sector, or NONE. */
static uint32_t place(store* s, uint32_t size, uint32_t floor) {
    uint32_t h = s->head;
    if (h != NONE && !(s->sstate[h] & SEC_FULL) && s->sector[h].free + size <= STORE_SECTOR_USE) {
        return h;
    }
    if (count(s, SEC_CLEAN) <= floor) {
        return NONE;
    }
    h = coldest(s, SEC_CLEAN);
    set_state(s, h, SEC_RESERVED);
    s->sector[h].seq = s->seq++;
    s->sector[h].free = STORE_HEADER;
    s->sector[h].live = 0;
    s->head = (uint8_t)h;
    return h;
}

/* ---- programming ---- */

/* Programs part of [off, off + n) of sector `sec` from `src`, from byte `*done` on: the most
 * one program may (the window, the page's end). 1 when the range is done, 0 when not yet
 * (a piece went, or the hook said later), < 0 on an error. */
static int prog_piece(store* s, uint32_t sec, uint32_t off, const uint8_t* src, uint32_t n, uint16_t* done) {
    uint32_t a = base(sec) + off + *done;
    uint32_t k = n - *done;
    uint32_t page = STORE_PAGE - (a & (STORE_PAGE - 1u));
    k = k > s->window ? s->window : k;
    k = k > page ? page : k;
    int rc = s->flash.prog(s->flash.ctx, a, src + *done, k);
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

/* The same, for a range copied from flash at `from`: a piece goes through buf. */
static int copy_piece(store* s, uint32_t sec, uint32_t off, uint32_t from, uint32_t n, uint16_t* done) {
    uint32_t a = base(sec) + off + *done;
    uint32_t k = n - *done;
    uint32_t page = STORE_PAGE - (a & (STORE_PAGE - 1u));
    k = k > s->window ? s->window : k;
    k = k > page ? page : k;
    memcpy(s->buf, s->flash.map(s->flash.ctx, from + *done), k);
    uint16_t piece = 0;
    int rc = prog_piece(s, sec, off + *done, s->buf, k, &piece);
    if (rc < 0) {
        return -1;
    }
    if (rc == 0) {
        return 0;
    }
    *done = (uint16_t)(*done + k);
    return *done == n;
}

static void rec_header(const store_writer* w, uint8_t* h) {
    h[0] = w->type;
    h[1] = 0;
    wr16(h + 2, w->len);
    wr32(h + 4, w->key);
    wr32(h + 8, w->seq);
    wr32(h + 12, w->crc);
}

static void writer_failed(store* s) {
    store_writer* w = &s->w;
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
        uint32_t old = entry_size(s, e);
        s->sector[e->sec].live = (uint16_t)(s->sector[e->sec].live - old);
        s->live -= old;
        e->stale++;
    } else {
        e = &s->key[s->nkeys++];
        e->key = w->key;
        e->stale = 0;
    }
    e->sec = w->sec;
    e->off = w->off;
    c->live = (uint16_t)(c->live + size);
    s->live += size;
}

static void writer_step(store* s) {
    store_writer* w = &s->w;
    int rc = 0;
    if (w->phase == W_CLAIM) {
        uint8_t claim[24];
        memcpy(claim, at(s, w->sec, 0), 20);
        wr32(claim + 20, s->sector[w->sec].seq);
        uint8_t p[12];
        wr32(p, s->sector[w->sec].seq);
        wr32(p + 4, store_crc32(0, claim, 24));
        wr32(p + 8, 0);
        rc = prog_piece(s, w->sec, 20, p, 12, &w->done);
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
            rc = copy_piece(s, w->sec, w->off + STORE_REC_HEADER, w->from + STORE_REC_HEADER, w->len, &w->done);
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

/* Starts writing a record into a place taken now. */
static int write_record(store* s, uint32_t key, uint8_t type, const void* data, uint32_t len, uint32_t floor) {
    uint32_t size = rec_size(len);
    store_key* e = find(s, key);
    if (!e && s->nkeys >= STORE_KEYS_MAX) {
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
    w->crc = store_crc32(store_crc32(0, h, 12), data, len);
    w->phase = sec_state(s, sec) == SEC_RESERVED ? W_CLAIM : (len ? W_DATA : W_HEAD);
    return STORE_OK;
}

/* ---- the mount ---- */

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

/* An erase header whose program was cut: its bytes hold every bit of the magic and the
 * version (bits only cleared toward them), and the rest of the sector is erased. */
static int torn_header(const uint8_t* h) {
    uint8_t want[12] = {'S', 'I', 'D', 'O', 'U', 'S', 'T', 'R', STORE_FORMAT, 0, 0, 0};
    for (uint32_t i = 0; i < 12; i++) {
        if ((h[i] & want[i]) != want[i]) {
            return 0;
        }
    }
    return all_ff(h + 20, STORE_SECTOR - 20u);
}

static void classify(store* s, uint32_t sec) {
    const uint8_t* h = at(s, sec, 0);
    store_sector* c = &s->sector[sec];
    uint32_t version = 0, erases = 0;
    c->seq = 0;
    c->free = STORE_HEADER;
    c->live = 0;
    c->erases = 0;
    s->sstate[sec] = SEC_DIRTY | SEC_GUESS;
    if (erase_header_ok(h, &version)) {
        if (version == 0 || version > STORE_FORMAT) {
            s->mount_newer = 1; /* a newer firmware's: not ours to touch */
            return;
        }
        s->mount_sidou = 1;
        s->sstate[sec] = SEC_DIRTY;
        c->erases = rd32(h + 12);
        if (all_ff(h + 20, 12)) {
            if (all_ff(h + 32, STORE_SECTOR - 32u)) {
                s->sstate[sec] = SEC_CLEAN;
            } else {
                s->corrupt++; /* programmed bytes in a sector never claimed */
            }
            return;
        }
        if (rd32(h + 24) != store_crc32(0, h, 24) || rd32(h + 28) != 0) {
            return; /* a claim cut short */
        }
        s->sstate[sec] = SEC_CLAIMED;
        c->seq = rd32(h + 20);
        if (c->seq >= s->seq) {
            s->seq = c->seq + 1u;
        }
        if (!all_ff(h + STORE_SECTOR_USE, STORE_SECTOR - STORE_SECTOR_USE)) {
            s->corrupt++; /* the guard band was written: read it, write no more into it */
            s->sstate[sec] |= SEC_FULL;
        }
        s->mount_off = STORE_HEADER;
        return;
    }
    if (killed(h, &erases)) {
        s->mount_sidou = 1;
        s->sstate[sec] = SEC_DIRTY;
        c->erases = erases;
        return;
    }
    if (all_ff(h, STORE_SECTOR)) {
        return;
    }
    s->mount_blank = 0;
    if (torn_header(h)) {
        s->mount_sidou = 1;
    }
}

/* A record at `off` of a claimed sector: its size if it is valid, else 0. */
static uint32_t rec_valid(const store* s, uint32_t sec, uint32_t off) {
    if (off + STORE_REC_OVERHEAD > STORE_SECTOR_USE) {
        return 0;
    }
    const uint8_t* h = at(s, sec, off);
    uint32_t len = rd16(h + 2), size = rec_size(len);
    if (h[0] == 0xFFu || off + size > STORE_SECTOR_USE ||
        (h[0] == STORE_TYPE_TOMB && len != 0) || (h[0] <= STORE_TYPE_MAX && len > STORE_VALUE_MAX)) {
        return 0;
    }
    if (rd32(h + STORE_REC_HEADER + pad4(len)) != 0) {
        return 0;
    }
    uint32_t crc = store_crc32(store_crc32(0, h, 12), h + STORE_REC_HEADER, len);
    return crc == rd32(h + 12) ? size : 0u;
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
        /* A record and its copy: the same bytes, whichever wins. */
        uint32_t len = rd16(h + 2);
        if (rd32(h + 12) != rd32(cur + 12) || rd16(cur + 2) != len || h[0] != cur[0]) {
            s->corrupt++;
        } else {
            for (uint32_t i = 0; i < len; i++) {
                if (h[STORE_REC_HEADER + i] != cur[STORE_REC_HEADER + i]) {
                    s->corrupt++;
                    break;
                }
            }
        }
        if (s->sector[sec].seq <= s->sector[e->sec].seq) {
            return;
        }
    } else if (seq < cur_seq) {
        return;
    }
    e->sec = (uint8_t)sec;
    e->off = (uint16_t)off;
}

/* Up to STORE_MOUNT_SLICE bytes of a claimed sector's records. 1 when the sector is done. */
static int scan_records(store* s, uint32_t sec) {
    uint32_t budget = STORE_MOUNT_SLICE;
    store_sector* c = &s->sector[sec];
    while (budget) {
        uint32_t off = s->mount_off;
        if (off + STORE_REC_OVERHEAD > STORE_SECTOR_USE) {
            c->free = (uint16_t)off;
            s->sstate[sec] |= SEC_FULL;
            return 1;
        }
        const uint8_t* h = at(s, sec, off);
        if (all_ff(h, STORE_REC_HEADER)) {
            /* The end, unless a cut write left data after an erased header. */
            c->free = (uint16_t)off;
            if (!all_ff(h, STORE_SECTOR_USE - off)) {
                s->sstate[sec] |= SEC_FULL;
            }
            return 1;
        }
        uint32_t size = rec_valid(s, sec, off);
        if (!size) {
            c->free = (uint16_t)off; /* a cut write: the sector takes no more */
            s->sstate[sec] |= SEC_FULL;
            return 1;
        }
        index_record(s, sec, off);
        s->mount_off = (uint16_t)(off + size);
        budget = size >= budget ? 0u : budget - size;
    }
    return 0;
}

static void mount_finish(store* s) {
    uint32_t known = 0;
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        if (!(s->sstate[i] & SEC_GUESS) && s->sector[i].erases > known) {
            known = s->sector[i].erases;
        }
    }
    s->head = NONE;
    for (uint32_t i = 0; i < STORE_SECTORS; i++) {
        if (s->sstate[i] & SEC_GUESS) {
            s->sector[i].erases = known; /* unknown: count it among the most worn */
            s->sstate[i] &= (uint8_t)~SEC_GUESS;
        }
        if (sec_state(s, i) == SEC_CLAIMED && !(s->sstate[i] & SEC_FULL) &&
            s->sector[i].free + STORE_REC_OVERHEAD <= STORE_SECTOR_USE &&
            (s->head == NONE || s->sector[i].seq > s->sector[s->head].seq)) {
            s->head = (uint8_t)i;
        }
    }
    s->live = 0;
    for (uint32_t i = 0; i < s->nkeys; i++) {
        uint32_t size = entry_size(s, &s->key[i]);
        s->sector[s->key[i].sec].live = (uint16_t)(s->sector[s->key[i].sec].live + size);
        s->live += size;
    }
    if (s->mount_newer) {
        s->state = STORE_STATE_FOREIGN;
    } else if (s->mount_sidou) {
        s->state = STORE_STATE_SIDOU;
    } else {
        s->state = s->mount_blank ? STORE_STATE_BLANK : STORE_STATE_FOREIGN;
    }
}

static void mount_step(store* s) {
    uint32_t sec = s->mount_sec;
    if (s->mount_off == 0) {
        classify(s, sec);
    }
    if (s->mount_off == 0 || scan_records(s, sec)) {
        s->mount_off = 0;
        if (++s->mount_sec == STORE_SECTORS) {
            mount_finish(s);
        }
    }
}

/* ---- maintenance ---- */

/* Records of `key` in `sec` other than its live one. */
static uint32_t others_in(const store* s, uint32_t sec, const store_key* e) {
    uint32_t n = 0;
    for (uint32_t off = STORE_HEADER; off < s->sector[sec].free;) {
        const uint8_t* h = at(s, sec, off);
        if (keyed(h[0]) && rd32(h + 4) == e->key && !(e->sec == sec && e->off == off)) {
            n++;
        }
        off += rec_size(rd16(h + 2));
    }
    return n;
}

/* A killed sector's records count no more: the key index follows. */
static void forget(store* s, uint32_t sec) {
    for (uint32_t off = STORE_HEADER; off < s->sector[sec].free;) {
        const uint8_t* h = at(s, sec, off);
        store_key* e = keyed(h[0]) ? find(s, rd32(h + 4)) : 0;
        if (e && e->sec == sec && e->off == off) {
            drop_key(s, e); /* a tombstone with nothing left to hide */
        } else if (e && e->stale) {
            e->stale--;
        }
        off += rec_size(rd16(h + 2));
    }
    s->live -= s->sector[sec].live;
    s->sector[sec].live = 0;
    set_state(s, sec, SEC_DIRTY);
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
    uint8_t h[20];
    memcpy(h, MAGIC, 8);
    wr16(h + 8, STORE_FORMAT);
    wr16(h + 10, 0);
    wr32(h + 12, s->sector[sec].erases);
    wr32(h + 16, store_crc32(0, h, 16));
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

/* The next step of a collection: skips the victim's dead records, starts the next live one's
 * copy, or (all copied) kills the victim. */
static void collect_step(store* s) {
    uint32_t v = s->job_sec;
    if (s->job_phase == C_SCAN) {
        while (s->job_off < s->sector[v].free) {
            uint32_t off = s->job_off;
            const uint8_t* h = at(s, v, off);
            uint32_t len = rd16(h + 2), size = rec_size(len);
            s->job_off = (uint16_t)(off + size);
            store_key* e = keyed(h[0]) ? find(s, rd32(h + 4)) : 0;
            if (!e || e->sec != v || e->off != off) {
                continue; /* superseded: garbage */
            }
            if (h[0] == STORE_TYPE_TOMB && e->stale <= others_in(s, v, e)) {
                continue; /* nothing older outside the victim: dropped with it */
            }
            uint32_t sec = place(s, size, 0);
            if (sec == NONE) {
                s->job = JOB_NONE; /* no room: try again when the pool refilled */
                return;
            }
            store_writer* w = &s->w;
            w->src = 0;
            w->from = base(v) + off;
            w->key = rd32(h + 4);
            w->seq = rd32(h + 8);
            w->crc = rd32(h + 12);
            w->type = h[0];
            w->len = (uint16_t)len;
            w->sec = (uint8_t)sec;
            w->off = s->sector[sec].free;
            w->copy = 1;
            w->from_sec = (uint8_t)v;
            w->from_off = (uint16_t)off;
            w->done = 0;
            w->phase = sec_state(s, sec) == SEC_RESERVED ? W_CLAIM : (len ? W_DATA : W_HEAD);
            writer_step(s);
            return;
        }
        s->job_phase = C_KILL;
        s->job_done = 0;
    }
    int rc = kill(s, v);
    if (rc < 0) {
        s->job = JOB_NONE;
    } else if (rc > 0) {
        forget(s, v);
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
        *sec = victim(s);
        if (*sec != NONE && (pool || !s->sector[*sec].live)) {
            return JOB_COLLECT;
        }
        return JOB_NONE;
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

/* ---- the API ---- */

void store_init(store* s, const store_flash* flash, uint32_t window) {
    memset(s, 0, sizeof *s);
    s->flash = *flash;
    s->window = window == 0 || window > STORE_WINDOW_MAX ? STORE_WINDOW_MAX : window;
    s->head = NONE;
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
    return s->job != JOB_NONE || due(s, &sec) != JOB_NONE;
}

int store_poll(store* s) {
    if (s->state == STORE_STATE_MOUNTING) {
        mount_step(s);
        return 1;
    }
    if (!writable(s)) {
        return 0;
    }
    if (s->w.phase != W_IDLE) {
        writer_step(s);
        return 1;
    }
    if (s->job == JOB_NONE && !(s->quiet && next_job(s))) {
        return 0;
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
    memcpy(buf, h + STORE_REC_HEADER, len < cap ? len : cap);
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

int store_delete(store* s, uint32_t key) {
    if (key > STORE_KEY_MAX) {
        return STORE_INVALID;
    }
    int rc = write_check(s);
    if (rc) {
        return rc;
    }
    store_key* e = find(s, key);
    if (!e || at(s, e->sec, e->off)[0] == STORE_TYPE_TOMB) {
        return STORE_OK;
    }
    rc = write_record(s, key, STORE_TYPE_TOMB, 0, 0, STORE_POOL_MIN);
    /* A full store must still take a deletion: it may take the pool one under its minimum. */
    return rc == STORE_FULL ? write_record(s, key, STORE_TYPE_TOMB, 0, 0, STORE_POOL_MIN - 1u) : rc;
}

int store_collect(store* s) {
    if (!writable(s) || s->job != JOB_NONE || s->w.phase != W_IDLE) {
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
    s->w.phase = W_IDLE;
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
