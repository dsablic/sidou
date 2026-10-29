// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's trailer's chunks (song_ext.h; docs/reference/song-format.md has each layout). */
#include "song_ext.h"

#include <string.h>

uint32_t song_crc32(uint32_t crc, const uint8_t* p, uint32_t n) {
    /* Reflected 0x04C11DB7 (0xEDB88320), a nibble at a time. */
    static const uint32_t t[16] = {
        0x00000000u, 0x1db71064u, 0x3b6e20c8u, 0x26d930acu, 0x76dc4190u, 0x6b6b51f4u,
        0x4db26158u, 0x5005713cu, 0xedb88320u, 0xf00f9344u, 0xd6d6a3e8u, 0xcb61b38cu,
        0x9b64c2b0u, 0x86d3d2d4u, 0xa00ae278u, 0xbdbdf21cu};
    crc = ~crc;
    for (uint32_t i = 0; i < n; i++) {
        crc ^= p[i];
        crc = (crc >> 4) ^ t[crc & 15u];
        crc = (crc >> 4) ^ t[crc & 15u];
    }
    return ~crc;
}

static uint32_t rd32(const uint8_t* p) {
    return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24;
}

static int bytes_equal(const uint8_t* a, const uint8_t* b, uint32_t n) {
    for (uint32_t i = 0; i < n; i++)
        if (a[i] != b[i])
            return 0;
    return 1;
}

static void wr32(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}

static uint16_t rd16(const uint8_t* p) {
    return (uint16_t)(p[0] | p[1] << 8);
}

static void wr16(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
}

/* The chunk with tag at or after the area's start: its offset, or -1. */
static int32_t find_at(const gt2_song* s, uint32_t tag) {
    uint32_t at = 0;
    while (at + SONG_CHUNK_HEADER <= s->ext_used) {
        uint32_t len = rd32(s->ext + at + 4u);
        if (len > s->ext_used - at - SONG_CHUNK_HEADER)
            return -1;
        if (rd32(s->ext + at) == tag)
            return (int32_t)at;
        at += SONG_CHUNK_HEADER + len;
    }
    return -1;
}

int song_ext_valid(const gt2_song* s) {
    uint32_t at = 0;
    if (s->ext_used > SONG_EXT_BYTES)
        return 0;
    while (at < s->ext_used) {
        if (s->ext_used - at < SONG_CHUNK_HEADER)
            return 0;
        uint32_t len = rd32(s->ext + at + 4u);
        if (len == 0u || len > s->ext_used - at - SONG_CHUNK_HEADER)
            return 0;
        at += SONG_CHUNK_HEADER + len;
    }
    return 1;
}

const uint8_t* song_ext_find(const gt2_song* s, uint32_t tag, uint32_t* len) {
    int32_t at = find_at(s, tag);
    if (at < 0)
        return NULL;
    *len = rd32(s->ext + at + 4);
    return s->ext + at + SONG_CHUNK_HEADER;
}

static void remove_at(gt2_song* s, uint32_t at, uint32_t size) {
    for (uint32_t i = at; i + size < s->ext_used; i++)
        s->ext[i] = s->ext[i + size];
    s->ext_used -= size;
}

int song_ext_remove(gt2_song* s, uint32_t tag) {
    if (s->frozen)
        return -(int)GT2_ERR_SAVING;
    int32_t at = find_at(s, tag);
    if (at < 0)
        return 0;
    uint32_t size = SONG_CHUNK_HEADER + rd32(s->ext + at + 4);
    int cap = gt2_saved_ext_check(s, s->ext_used - size);
    if (cap != 0)
        return cap;
    uint32_t old_used = s->ext_used;
    remove_at(s, (uint32_t)at, size);
    gt2_saved_ext_changed(s, old_used);
    return 1;
}

int song_ext_set(gt2_song* s, uint32_t tag, const uint8_t* data, uint32_t len) {
    if (s->frozen)
        return -(int)GT2_ERR_SAVING;
    if (len == 0u || tag == SONG_TAG_PAD)
        return -1;
    int32_t at = find_at(s, tag);
    uint32_t old = at < 0 ? 0u : SONG_CHUNK_HEADER + rd32(s->ext + at + 4);
    uint32_t size = SONG_CHUNK_HEADER + len;
    if (len > SONG_EXT_BYTES || s->ext_used - old + size > SONG_EXT_BYTES)
        return -1;
    if (old == size && bytes_equal(s->ext + at + SONG_CHUNK_HEADER, data, len))
        return 0;
    int cap = gt2_saved_ext_check(s, s->ext_used - old + size);
    if (cap != 0)
        return cap;
    uint32_t old_used = s->ext_used;
    if (at < 0) {
        at = (int32_t)s->ext_used;
        s->ext_used += size;
    } else if (size != old) {
        /* Move what follows to its new place: forwards when shrinking, backwards when
         * growing (no memmove on the device). */
        uint32_t tail = (uint32_t)at + old;
        uint32_t count = s->ext_used - tail;
        if (size < old) {
            for (uint32_t i = 0; i < count; i++)
                s->ext[(uint32_t)at + size + i] = s->ext[tail + i];
        } else {
            for (uint32_t i = count; i > 0u; i--)
                s->ext[(uint32_t)at + size + i - 1u] = s->ext[tail + i - 1u];
        }
        s->ext_used = s->ext_used - old + size;
    }
    wr32(s->ext + at, tag);
    wr32(s->ext + at + 4, len);
    for (uint32_t i = 0; i < len; i++)
        s->ext[(uint32_t)at + SONG_CHUNK_HEADER + i] = data[i];
    gt2_saved_ext_changed(s, old_used);
    return 0;
}

/* OPTS */

void gt2_options_default(gt2_options* o) {
    memset(o, 0, sizeof *o);
    o->hr_adsr = 0x0f00u;
    o->multispeed = 1u;
    o->finevibrato = 1u;
    o->optimize_pulse = 1u;
    o->optimize_realtime = 1u;
    o->sid_address = 0xd400u;
    o->player_page = 0x10u;
    o->zeropage = 0xfcu;
}

void song_opts_encode(const gt2_options* o, uint8_t b[SONG_OPTS_BYTES]) {
    b[0] = 1u;
    wr16(b + 1, o->hr_adsr);
    b[3] = o->multispeed;
    b[4] = o->ntsc;
    wr32(b + 5, o->clock_hz);
    wr32(b + 9, o->a4_millihz);
    b[13] = o->finevibrato;
    b[14] = o->optimize_pulse;
    b[15] = o->optimize_realtime;
    wr16(b + 16, o->player_flags);
    wr16(b + 18, o->sid_address);
    b[20] = o->player_page;
    b[21] = o->zeropage;
    b[22] = o->sid_model;
}

int song_opts_decode(const uint8_t* b, uint32_t len, gt2_options* o) {
    gt2_options_default(o);
    if (len < SONG_OPTS_BYTES || b[0] != 1u)
        return -1;
    o->hr_adsr = rd16(b + 1);
    o->multispeed = b[3];
    o->ntsc = b[4];
    o->clock_hz = rd32(b + 5);
    o->a4_millihz = rd32(b + 9);
    o->finevibrato = b[13];
    o->optimize_pulse = b[14];
    o->optimize_realtime = b[15];
    o->player_flags = rd16(b + 16);
    o->sid_address = rd16(b + 18);
    o->player_page = b[20];
    o->zeropage = b[21];
    o->sid_model = b[22];
    /* What gt2reloc clamps (its main, after the options), clamped as it clamps it. */
    if (o->multispeed > 16u)
        o->multispeed = 16u;
    if (o->optimize_pulse > 1u)
        o->optimize_pulse = 1u;
    if (o->optimize_realtime > 1u)
        o->optimize_realtime = 1u;
    if (o->clock_hz < 100u)
        o->clock_hz = 0u;
    if (o->finevibrato > 2u)
        o->finevibrato = 2u; /* above 1: always, whatever the multispeed */
    /* What has no meaning there: the chunk is not used. */
    if (o->ntsc > 1u || o->sid_model > 1u || (o->player_flags & ~GT2_PLAYER_KNOWN) != 0u) {
        gt2_options_default(o);
        return -1;
    }
    return 0;
}

int song_ext_get_options(const gt2_song* s, gt2_options* o) {
    uint32_t len = 0;
    const uint8_t* d = song_ext_find(s, SONG_TAG_OPTS, &len);
    if (d == NULL) {
        gt2_options_default(o);
        return 0;
    }
    return song_opts_decode(d, len, o) == 0;
}

int song_ext_set_options(gt2_song* s, const gt2_options* o) {
    uint8_t b[SONG_OPTS_BYTES];
    song_opts_encode(o, b);
    return song_ext_set(s, SONG_TAG_OPTS, b, SONG_OPTS_BYTES);
}

/* A chunk of version 1 with at least min bytes: its data, else NULL. */
static const uint8_t* get_v1(const gt2_song* s, uint32_t tag, uint32_t min, uint32_t* len) {
    const uint8_t* d = song_ext_find(s, tag, len);
    if (d == NULL || *len < min || d[0] != 1u)
        return NULL;
    return d;
}

/* CHPA, CHPB */

int song_ext_get_chip(const gt2_song* s, uint32_t tag, song_chip_setup* c) {
    uint32_t len = 0;
    memset(c, 0, sizeof *c);
    const uint8_t* d = get_v1(s, tag, 9u, &len);
    if (d == NULL || d[8] > SONG_EXPERT_MAX || len < 9u + d[8])
        return 0;
    c->model = d[1];
    c->profile = d[2];
    c->age = d[3];
    c->dac = d[4];
    c->thump = d[5];
    c->bleed = d[6];
    c->envelope = d[7];
    c->expert_count = d[8];
    for (uint32_t i = 0; i < c->expert_count; i++)
        c->expert[i] = d[9u + i];
    return 1;
}

int song_ext_set_chip(gt2_song* s, uint32_t tag, const song_chip_setup* c) {
    uint8_t b[9u + SONG_EXPERT_MAX];
    if (c->expert_count > SONG_EXPERT_MAX || (tag != SONG_TAG_CHPA && tag != SONG_TAG_CHPB))
        return -1;
    b[0] = 1u;
    b[1] = c->model;
    b[2] = c->profile;
    b[3] = c->age;
    b[4] = c->dac;
    b[5] = c->thump;
    b[6] = c->bleed;
    b[7] = c->envelope;
    b[8] = c->expert_count;
    for (uint32_t i = 0; i < c->expert_count; i++)
        b[9u + i] = c->expert[i];
    return song_ext_set(s, tag, b, 9u + c->expert_count);
}

/* SLOT */

int song_ext_get_slots(const gt2_song* s, song_slots* v) {
    uint32_t len = 0;
    memset(v, 0, sizeof *v);
    const uint8_t* d = get_v1(s, SONG_TAG_SLOT, 2u, &len);
    if (d == NULL || d[1] > SONG_SLOTS_MAX || len < 2u + d[1] * SONG_SLOT_FIELDS)
        return 0;
    v->count = d[1];
    for (uint32_t i = 0; i < v->count; i++)
        for (uint32_t f = 0; f < SONG_SLOT_FIELDS; f++)
            v->slot[i][f] = d[2u + i * SONG_SLOT_FIELDS + f];
    return 1;
}

int song_ext_set_slots(gt2_song* s, const song_slots* v) {
    uint8_t b[2u + SONG_SLOTS_MAX * SONG_SLOT_FIELDS];
    if (v->count > SONG_SLOTS_MAX)
        return -1;
    b[0] = 1u;
    b[1] = v->count;
    for (uint32_t i = 0; i < v->count; i++)
        for (uint32_t f = 0; f < SONG_SLOT_FIELDS; f++)
            b[2u + i * SONG_SLOT_FIELDS + f] = v->slot[i][f];
    return song_ext_set(s, SONG_TAG_SLOT, b, 2u + v->count * SONG_SLOT_FIELDS);
}

/* LOCK */

uint32_t song_ext_get_locks(const gt2_song* s, song_lock* out, uint32_t max) {
    uint32_t len = 0;
    const uint8_t* d = get_v1(s, SONG_TAG_LOCK, 3u, &len);
    if (d == NULL)
        return 0u;
    uint32_t n = rd16(d + 1);
    if (n > SONG_LOCKS_MAX || len < 3u + n * 5u)
        return 0u;
    for (uint32_t i = 0; i < n && i < max; i++) {
        const uint8_t* l = d + 3u + i * 5u;
        out[i].pattern = l[0];
        out[i].row = l[1];
        out[i].param = l[2];
        out[i].value = rd16(l + 3);
    }
    return n;
}

static uint32_t lock_key(const song_lock* l) {
    return (uint32_t)l->pattern << 16 | (uint32_t)l->row << 8 | l->param;
}

int song_ext_set_locks(gt2_song* s, const song_lock* locks, uint32_t n) {
    if (s->frozen)
        return -(int)GT2_ERR_SAVING;
    if (n > SONG_LOCKS_MAX)
        return -1;
    for (uint32_t i = 1; i < n; i++)
        if (lock_key(&locks[i - 1u]) >= lock_key(&locks[i]))
            return -1;
    if (n == 0u) {
        int removed = song_ext_remove(s, SONG_TAG_LOCK);
        return removed < 0 ? removed : 0;
    }
    uint32_t len = 3u + n * 5u;
    int32_t at = find_at(s, SONG_TAG_LOCK);
    uint32_t old = at < 0 ? 0u : SONG_CHUNK_HEADER + rd32(s->ext + at + 4);
    if (s->ext_used - old + SONG_CHUNK_HEADER + len > SONG_EXT_BYTES)
        return -1;
    int cap = gt2_saved_ext_check(s, s->ext_used - old + SONG_CHUNK_HEADER + len);
    if (cap != 0)
        return cap;
    /* Built at the area's end, as one chunk: remove the old one, then append. */
    uint32_t old_used = s->ext_used;
    if (at >= 0)
        remove_at(s, (uint32_t)at, old);
    uint8_t* d = s->ext + s->ext_used;
    wr32(d, SONG_TAG_LOCK);
    wr32(d + 4, len);
    d += SONG_CHUNK_HEADER;
    d[0] = 1u;
    wr16(d + 1, n);
    for (uint32_t i = 0; i < n; i++) {
        uint8_t* l = d + 3u + i * 5u;
        l[0] = locks[i].pattern;
        l[1] = locks[i].row;
        l[2] = locks[i].param;
        wr16(l + 3, locks[i].value);
    }
    s->ext_used += SONG_CHUNK_HEADER + len;
    gt2_saved_ext_changed(s, old_used);
    return 0;
}

uint32_t song_lock_max(uint32_t param) {
    static const uint16_t max[SONG_LOCK_PARAMS] = {2047u, 15u, 7u, 1u, 4095u,
                                                   127u,  127u, 127u, 127u, 100u};
    return param < SONG_LOCK_PARAMS ? max[param] : 0u;
}

uint32_t song_mix(uint32_t x) {
    x = (x ^ x >> 16) * 0x7feb352du;
    x = (x ^ x >> 15) * 0x846ca68bu;
    return x ^ x >> 16;
}

int song_chance_plays(uint32_t seed, uint32_t n, uint32_t chance) {
    return (song_mix(seed + n * 0x9e3779b9u) >> 8) % 100u < chance;
}

/* The LOCK chunk's data (version 1) and its record count; NULL if none or malformed. */
static uint8_t* locks_of(gt2_song* s, int32_t* at, uint32_t* n) {
    *at = find_at(s, SONG_TAG_LOCK);
    if (*at < 0)
        return NULL;
    uint32_t len = rd32(s->ext + *at + 4);
    uint8_t* d = s->ext + *at + SONG_CHUNK_HEADER;
    if (len < 3u || d[0] != 1u)
        return NULL;
    *n = rd16(d + 1);
    if (*n > SONG_LOCKS_MAX || len < 3u + *n * SONG_LOCK_BYTES)
        return NULL;
    return d;
}

/* The chunk at at to data length len, what follows moved (no memmove on the device). */
static void chunk_resize_apply(gt2_song* s, int32_t at, uint32_t len, int reserved) {
    uint32_t old = rd32(s->ext + at + 4);
    uint32_t old_used = s->ext_used;
    uint32_t tail = (uint32_t)at + SONG_CHUNK_HEADER + old;
    uint32_t count = s->ext_used - tail;
    uint32_t to = (uint32_t)at + SONG_CHUNK_HEADER + len;
    if (len < old) {
        for (uint32_t i = 0; i < count; i++)
            s->ext[to + i] = s->ext[tail + i];
    } else {
        for (uint32_t i = count; i > 0u; i--)
            s->ext[to + i - 1u] = s->ext[tail + i - 1u];
    }
    s->ext_used = s->ext_used - old + len;
    wr32(s->ext + at + 4, len);
    if (reserved)
        gt2_song_changed(s);
    else
        gt2_saved_ext_changed(s, old_used);
}

static int chunk_resize(gt2_song* s, int32_t at, uint32_t len, int reserved) {
    uint32_t old = rd32(s->ext + at + 4);
    if (len > old && s->ext_used + (len - old) > SONG_EXT_BYTES)
        return -1;
    int cap = reserved ? 0 : gt2_saved_ext_check(s, s->ext_used - old + len);
    if (cap != 0)
        return cap;
    chunk_resize_apply(s, at, len, reserved);
    return 0;
}

static uint32_t rec_key(const uint8_t* r) {
    return (uint32_t)r[0] << 16 | (uint32_t)r[1] << 8 | r[2];
}

/* The first record at or after key. */
static uint32_t lower_bound(const uint8_t* recs, uint32_t n, uint32_t key) {
    uint32_t lo = 0, hi = n;
    while (lo < hi) {
        uint32_t mid = (lo + hi) / 2u;
        if (rec_key(recs + mid * SONG_LOCK_BYTES) < key)
            lo = mid + 1u;
        else
            hi = mid;
    }
    return lo;
}

int song_ext_locks_check(gt2_song* s) {
    int32_t at = find_at(s, SONG_TAG_LOCK);
    if (at < 0)
        return 0;
    uint32_t len = rd32(s->ext + at + 4);
    const uint8_t* d = s->ext + at + SONG_CHUNK_HEADER;
    if (len < 3u || d[0] != 1u)
        return 0; /* another version's: kept as it came, never read as locks */
    uint32_t n = rd16(d + 1);
    int bad = n > SONG_LOCKS_MAX || len < 3u + n * SONG_LOCK_BYTES;
    for (uint32_t i = 0; i < n && !bad; i++) {
        const uint8_t* r = d + 3u + i * SONG_LOCK_BYTES;
        bad = r[0] >= GT2_MAX_PATTERNS || r[1] >= GT2_MAX_ROWS || r[2] >= SONG_LOCK_PARAMS ||
              rd16(r + 3) > song_lock_max(r[2]) ||
              (i && rec_key(r - SONG_LOCK_BYTES) >= rec_key(r));
    }
    if (!bad)
        return 0;
    remove_at(s, (uint32_t)at, SONG_CHUNK_HEADER + len);
    return 1;
}

const uint8_t* song_locks_row(const gt2_song* s, uint32_t pattern, uint32_t row, uint32_t* n) {
    int32_t at;
    uint32_t count = 0;
    const uint8_t* d = locks_of((gt2_song*)s, &at, &count);
    *n = 0;
    if (d == NULL)
        return NULL;
    const uint8_t* recs = d + 3;
    uint32_t key = (pattern & 0xffu) << 16 | (row & 0xffu) << 8;
    uint32_t i = lower_bound(recs, count, key);
    uint32_t j = i;
    while (j < count && (rec_key(recs + j * SONG_LOCK_BYTES) >> 8) == key >> 8)
        j++;
    *n = j - i;
    return j > i ? recs + i * SONG_LOCK_BYTES : NULL;
}

int song_lock_put(gt2_song* s, uint32_t pattern, uint32_t row, uint32_t param, uint32_t value) {
    if (s->frozen)
        return -(int)GT2_ERR_SAVING;
    if (pattern >= GT2_MAX_PATTERNS || row >= GT2_MAX_ROWS || param >= SONG_LOCK_PARAMS ||
        value > song_lock_max(param))
        return -1;
    int32_t at;
    uint32_t n = 0;
    uint8_t* d = locks_of(s, &at, &n);
    if (d == NULL) {
        if (find_at(s, SONG_TAG_LOCK) >= 0)
            return -1; /* malformed: left alone */
        song_lock first = {(uint8_t)pattern, (uint8_t)row, (uint8_t)param, (uint16_t)value};
        return song_ext_set_locks(s, &first, 1u);
    }
    uint32_t key = pattern << 16 | row << 8 | param;
    uint32_t i = lower_bound(d + 3, n, key);
    uint8_t* r = d + 3 + i * SONG_LOCK_BYTES;
    if (i < n && rec_key(r) == key) {
        if (rd16(r + 3) == value)
            return 0;
        if (s->pending)
            return -(int)GT2_ERR_PENDING; /* as an added lock: rows may move under it */
        wr16(r + 3, value);
        gt2_song_changed(s);
        return 0;
    }
    if (n >= SONG_LOCKS_MAX)
        return -1;
    int resized = chunk_resize(s, at, 3u + (n + 1u) * SONG_LOCK_BYTES, 0);
    if (resized != 0)
        return resized;
    d = s->ext + at + SONG_CHUNK_HEADER;
    r = d + 3 + i * SONG_LOCK_BYTES;
    for (uint32_t k = (n - i) * SONG_LOCK_BYTES; k > 0u; k--)
        r[SONG_LOCK_BYTES + k - 1u] = r[k - 1u];
    r[0] = (uint8_t)pattern;
    r[1] = (uint8_t)row;
    r[2] = (uint8_t)param;
    wr16(r + 3, value);
    wr16(d + 1, n + 1u);
    return 0;
}

/* Keeps the records keep(record) says, in order; returns how many went. */
typedef int (*lock_keep_fn)(uint8_t* rec, uint32_t a, uint32_t b, uint32_t c);

static int locks_filter(gt2_song* s, lock_keep_fn keep, uint32_t a, uint32_t b,
                        uint32_t c, int reserved) {
    if (s->frozen)
        return 0u;
    int32_t at;
    uint32_t n = 0;
    uint8_t* d = locks_of(s, &at, &n);
    if (d == NULL)
        return 0u;
    uint32_t kept = 0u;
    for (uint32_t i = 0; i < n; i++) {
        uint8_t record[SONG_LOCK_BYTES];
        memcpy(record, d + 3u + i * SONG_LOCK_BYTES, SONG_LOCK_BYTES);
        kept += keep(record, a, b, c) != 0;
    }
    uint32_t old = SONG_CHUNK_HEADER + rd32(s->ext + at + 4);
    uint32_t size = kept ? SONG_CHUNK_HEADER + 3u + kept * SONG_LOCK_BYTES : 0u;
    int cap = reserved ? 0 : gt2_saved_ext_check(s, s->ext_used - old + size);
    if (cap != 0)
        return cap;
    uint32_t w = 0;
    for (uint32_t i = 0; i < n; i++) {
        uint8_t* r = d + 3 + i * SONG_LOCK_BYTES;
        if (!keep(r, a, b, c))
            continue;
        if (w != i)
            for (uint32_t k = 0; k < SONG_LOCK_BYTES; k++)
                d[3 + w * SONG_LOCK_BYTES + k] = r[k];
        w++;
    }
    if (w == 0u) {
        uint32_t old_used = s->ext_used;
        remove_at(s, (uint32_t)at, old);
        if (reserved)
            gt2_song_changed(s);
        else
            gt2_saved_ext_changed(s, old_used);
    } else if (w != n) {
        wr16(d + 1, w);
        (void)chunk_resize(s, at, 3u + w * SONG_LOCK_BYTES, reserved);
    }
    if (w == n)
        gt2_song_changed(s);
    return (int)(n - w);
}

static int keep_not_key(uint8_t* r, uint32_t pattern, uint32_t row, uint32_t param) {
    return !(r[0] == pattern && r[1] == row && r[2] == param);
}

int song_lock_remove(gt2_song* s, uint32_t pattern, uint32_t row, uint32_t param) {
    if (s->frozen)
        return -(int)GT2_ERR_SAVING;
    int removed = locks_filter(s, keep_not_key, pattern, row, param, 0);
    return removed < 0 ? removed : removed != 0;
}

/* The pattern edit's rule on one record; it may move the record's row (the order holds: the
 * rows of one pattern move together). */
static int keep_edit(uint8_t* r, uint32_t op, uint32_t pattern, uint32_t row) {
    if (r[0] != pattern)
        return 1;
    switch (op) {
    case SONG_LOCKS_PUSH:
        if (r[1] >= (row & 0xffu)) {
            if (r[1] + 1u >= (row >> 8))
                return 0;
            r[1]++;
        }
        return 1;
    case SONG_LOCKS_INSERT:
        if (r[1] >= row) {
            if (r[1] + 1u >= GT2_MAX_ROWS)
                return 0;
            r[1]++;
        }
        return 1;
    case SONG_LOCKS_DELETE:
        if (r[1] == row)
            return 0;
        if (r[1] > row)
            r[1]--;
        return 1;
    case SONG_LOCKS_RESIZE:
        return r[1] < row;
    default:
        return 0;
    }
}

uint32_t song_locks_pattern_size(const gt2_song* s, uint32_t op, uint32_t pattern, uint32_t row) {
    int32_t at;
    uint32_t n = 0u, kept = 0u;
    const uint8_t* d = locks_of((gt2_song*)s, &at, &n);
    if (d == NULL)
        return s->ext_used;
    for (uint32_t i = 0; i < n; i++) {
        uint8_t record[SONG_LOCK_BYTES];
        memcpy(record, d + 3u + i * SONG_LOCK_BYTES, SONG_LOCK_BYTES);
        kept += keep_edit(record, op, pattern, row) != 0;
    }
    if (kept == n)
        return s->ext_used;
    uint32_t old = SONG_CHUNK_HEADER + rd32(s->ext + at + 4);
    return s->ext_used - old + (kept ? SONG_CHUNK_HEADER + 3u + kept * SONG_LOCK_BYTES : 0u);
}

void song_locks_pattern_edit(gt2_song* s, uint32_t op, uint32_t pattern, uint32_t row) {
    (void)locks_filter(s, keep_edit, op, pattern, row, 1);
}

gt2_err song_locks_restore_check(const gt2_song* s, uint32_t op, uint32_t pattern,
                                uint32_t row, uint32_t restored_row, const uint8_t* locks,
                                uint32_t n, uint32_t* ext_used) {
    if (n > SONG_LOCK_PARAMS || (n && !locks))
        return GT2_ERR_RANGE;
    for (uint32_t i = 0u; i < n; i++) {
        const uint8_t* rec = locks + i * SONG_LOCK_BYTES;
        if (rec[0] != pattern || rec[1] != restored_row || rec[2] >= SONG_LOCK_PARAMS ||
            rd16(rec + 3) > song_lock_max(rec[2]) ||
            (i && rec_key(rec - SONG_LOCK_BYTES) >= rec_key(rec)))
            return GT2_ERR_RANGE;
    }
    int32_t at;
    uint32_t count = 0u, final = n;
    const uint8_t* d = locks_of((gt2_song*)s, &at, &count);
    if (!d && at >= 0)
        return GT2_ERR_RANGE;
    for (uint32_t i = 0u; i < count; i++) {
        uint8_t rec[SONG_LOCK_BYTES];
        memcpy(rec, d + 3u + i * SONG_LOCK_BYTES, SONG_LOCK_BYTES);
        if (!keep_edit(rec, op, pattern, row))
            continue;
        uint32_t j = 0u;
        while (j < n && rec_key(locks + j * SONG_LOCK_BYTES) != rec_key(rec))
            j++;
        final += j == n;
    }
    uint32_t old = d ? SONG_CHUNK_HEADER + rd32(s->ext + at + 4) : 0u;
    *ext_used = s->ext_used - old + (final ? SONG_CHUNK_HEADER + 3u + final * SONG_LOCK_BYTES : 0u);
    return final > SONG_LOCKS_MAX || *ext_used > SONG_EXT_BYTES ? GT2_ERR_FULL : GT2_OK;
}

void song_locks_restore_reserved(gt2_song* s, const uint8_t* locks, uint32_t n) {
    for (uint32_t i = 0u; i < n; i++) {
        int32_t at;
        uint32_t count = 0u;
        uint8_t* d = locks_of(s, &at, &count);
        if (!d) {
            at = (int32_t)s->ext_used;
            wr32(s->ext + at, SONG_TAG_LOCK);
            wr32(s->ext + at + 4, 3u);
            s->ext_used += SONG_CHUNK_HEADER + 3u;
            d = s->ext + at + SONG_CHUNK_HEADER;
            d[0] = 1u;
            wr16(d + 1, 0u);
        }
        const uint8_t* rec = locks + i * SONG_LOCK_BYTES;
        uint32_t pos = lower_bound(d + 3u, count, rec_key(rec));
        if (pos == count || rec_key(d + 3u + pos * SONG_LOCK_BYTES) != rec_key(rec)) {
            chunk_resize_apply(s, at, 3u + (count + 1u) * SONG_LOCK_BYTES, 1);
            for (uint32_t k = count; k > pos; k--)
                memcpy(d + 3u + k * SONG_LOCK_BYTES, d + 3u + (k - 1u) * SONG_LOCK_BYTES,
                       SONG_LOCK_BYTES);
            wr16(d + 1, ++count);
        }
        memcpy(d + 3u + pos * SONG_LOCK_BYTES, rec, SONG_LOCK_BYTES);
        gt2_song_changed(s);
    }
}

/* MIDI, SCNE */

static int get_counted(const gt2_song* s, uint32_t tag, uint32_t fixed, uint32_t max,
                       uint8_t* head, uint8_t* list) {
    uint32_t len = 0;
    const uint8_t* d = get_v1(s, tag, 1u + fixed, &len);
    if (d == NULL)
        return 0;
    uint32_t count = d[fixed];
    if (count > max || len < 1u + fixed + count)
        return 0;
    for (uint32_t i = 0; i < fixed; i++)
        head[i] = d[1u + i];
    for (uint32_t i = 0; i < count; i++)
        list[i] = d[1u + fixed + i];
    return 1;
}

int song_ext_get_midi(const gt2_song* s, song_midi* v) {
    uint8_t head[3];
    memset(v, 0, sizeof *v);
    if (!get_counted(s, SONG_TAG_MIDI, 3u, SONG_MIDI_CHANNELS_MAX, head, v->channel))
        return 0;
    v->mode = head[0];
    v->rows_per_beat = head[1];
    v->count = head[2];
    uint32_t len = 0;
    const uint8_t* d = song_ext_find(s, SONG_TAG_MIDI, &len);
    uint32_t at = 4u + v->count;
    if (len >= at + SONG_MIDI_OPTION_BYTES) {
        v->has_options = 1u;
        v->flags = d[at];
        v->whole = d[at + 1u];
        v->chord[0] = d[at + 2u];
        v->chord[1] = d[at + 3u];
        v->arp_rate = d[at + 4u];
    }
    return 1;
}

int song_ext_set_midi(gt2_song* s, const song_midi* v) {
    uint8_t b[4u + SONG_MIDI_CHANNELS_MAX + SONG_MIDI_OPTION_BYTES];
    if (v->count > SONG_MIDI_CHANNELS_MAX)
        return -1;
    b[0] = 1u;
    b[1] = v->mode;
    b[2] = v->rows_per_beat;
    b[3] = v->count;
    uint32_t n = 4u;
    for (uint32_t i = 0; i < v->count; i++)
        b[n++] = v->channel[i];
    b[n++] = v->flags;
    b[n++] = v->whole;
    b[n++] = v->chord[0];
    b[n++] = v->chord[1];
    b[n++] = v->arp_rate;
    return song_ext_set(s, SONG_TAG_MIDI, b, n);
}

int song_ext_get_scene(const gt2_song* s, song_scene* v) {
    uint8_t head[2];
    memset(v, 0, sizeof *v);
    if (!get_counted(s, SONG_TAG_SCNE, 2u, SONG_SCENE_VOICES_MAX, head, v->role))
        return 0;
    v->piece = head[0];
    v->count = head[1];
    /* The drum table after the roles, if the chunk has one (M2f): a count and its pairs. */
    uint32_t len = 0;
    const uint8_t* d = song_ext_find(s, SONG_TAG_SCNE, &len);
    uint32_t at = 3u + v->count;
    if (len > at && d[at] <= SONG_SCENE_DRUMS_MAX && len >= at + 1u + 2u * d[at]) {
        v->drums = d[at];
        for (uint32_t i = 0; i < v->drums; i++) {
            v->drum[i][0] = d[at + 1u + 2u * i];
            v->drum[i][1] = d[at + 2u + 2u * i];
        }
    }
    return 1;
}

int song_ext_set_scene(gt2_song* s, const song_scene* v) {
    uint8_t b[3u + SONG_SCENE_VOICES_MAX + 1u + 2u * SONG_SCENE_DRUMS_MAX];
    if (v->count > SONG_SCENE_VOICES_MAX || v->drums > SONG_SCENE_DRUMS_MAX)
        return -1;
    b[0] = 1u;
    b[1] = v->piece;
    b[2] = v->count;
    uint32_t n = 3u;
    for (uint32_t i = 0; i < v->count; i++)
        b[n++] = v->role[i];
    if (v->drums) { /* none: the chunk as M2c wrote it */
        b[n++] = v->drums;
        for (uint32_t i = 0; i < v->drums; i++) {
            b[n++] = v->drum[i][0];
            b[n++] = v->drum[i][1];
        }
    }
    return song_ext_set(s, SONG_TAG_SCNE, b, n);
}

/* KEYS */

int song_ext_get_keys(const gt2_song* s, song_keys* v) {
    uint32_t len = 0;
    memset(v, 0, sizeof *v);
    v->preset = SONG_KEYS_PRESET_NONE;
    const uint8_t* d = get_v1(s, SONG_TAG_KEYS, 2u, &len);
    if (d == NULL || d[1] > SONG_KEYS_MAX || len < 2u + 2u * d[1])
        return 0;
    v->count = d[1];
    for (uint32_t i = 0; i < v->count; i++)
        v->param[i] = rd16(d + 2u + 2u * i);
    if (len > 2u + 2u * v->count)
        v->preset = d[2u + 2u * v->count];
    return 1;
}

int song_ext_set_keys(gt2_song* s, const song_keys* v) {
    uint8_t b[3u + 2u * SONG_KEYS_MAX];
    if (v->count > SONG_KEYS_MAX)
        return -1;
    b[0] = 1u;
    b[1] = v->count;
    for (uint32_t i = 0; i < v->count; i++)
        wr16(b + 2u + 2u * i, v->param[i]);
    uint32_t len = 2u + 2u * v->count;
    if (v->preset != SONG_KEYS_PRESET_NONE)
        b[len++] = v->preset;
    return song_ext_set(s, SONG_TAG_KEYS, b, len);
}

/* DICE */

int song_ext_get_dice(const gt2_song* s, uint8_t* density, uint8_t* scale) {
    uint32_t len = 0;
    const uint8_t* d = get_v1(s, SONG_TAG_DICE, 3u, &len);
    *density = d != NULL && d[1] < SONG_DICE_DENSITIES ? d[1] : SONG_DICE_DENSITY_DEFAULT;
    *scale = d != NULL && d[2] < SONG_DICE_SCALES ? d[2] : 0u;
    return d != NULL;
}

int song_ext_set_dice(gt2_song* s, uint32_t density, uint32_t scale) {
    uint8_t b[3] = {1u, (uint8_t)density, (uint8_t)scale};
    return song_ext_set(s, SONG_TAG_DICE, b, 3u);
}

/* DIGI */

int song_ext_get_digi(const gt2_song* s, song_digi* v) {
    uint32_t len = 0;
    memset(v, 0, sizeof *v);
    const uint8_t* d = get_v1(s, SONG_TAG_DIGI, 2u, &len);
    if (d == NULL || d[1] > SONG_DIGI_MAX || len < 2u + 2u * d[1])
        return 0;
    v->count = d[1];
    memcpy(v->entry, d + 2, 2u * v->count);
    return 1;
}

int song_ext_set_digi(gt2_song* s, const song_digi* v) {
    uint8_t b[2u + 2u * SONG_DIGI_MAX];
    if (v->count > SONG_DIGI_MAX)
        return -1;
    b[0] = 1u;
    b[1] = v->count;
    memcpy(b + 2, v->entry, 2u * v->count);
    return song_ext_set(s, SONG_TAG_DIGI, b, 2u + 2u * v->count);
}

void song_ext_digi_map(const gt2_song* s, uint8_t map[GT2_MAX_INSTR + 1u]) {
    song_digi v;
    memset(map, 0, GT2_MAX_INSTR + 1u);
    if (s->ext_used > SONG_EXT_BYTES || !song_ext_get_digi(s, &v))
        return; /* (an area being read in: nothing yet) */
    for (uint32_t i = 0; i < v.count; i++) {
        uint32_t instr = v.entry[i][0], kind = v.entry[i][1];
        if (instr >= 1u && instr <= GT2_MAX_INSTR && kind > SONG_DIGI_NONE && kind < SONG_DIGI_KINDS)
            map[instr] = (uint8_t)kind;
    }
}
