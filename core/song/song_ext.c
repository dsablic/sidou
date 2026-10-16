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

int song_ext_remove(gt2_song* s, uint32_t tag) {
    int32_t at = find_at(s, tag);
    if (at < 0)
        return 0;
    uint32_t size = SONG_CHUNK_HEADER + rd32(s->ext + at + 4);
    for (uint32_t i = (uint32_t)at; i + size < s->ext_used; i++)
        s->ext[i] = s->ext[i + size];
    s->ext_used -= size;
    return 1;
}

int song_ext_set(gt2_song* s, uint32_t tag, const uint8_t* data, uint32_t len) {
    if (len == 0u || tag == SONG_TAG_PAD)
        return -1;
    int32_t at = find_at(s, tag);
    uint32_t old = at < 0 ? 0u : SONG_CHUNK_HEADER + rd32(s->ext + at + 4);
    uint32_t size = SONG_CHUNK_HEADER + len;
    if (len > SONG_EXT_BYTES || s->ext_used - old + size > SONG_EXT_BYTES)
        return -1;
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
    if (n > SONG_LOCKS_MAX)
        return -1;
    for (uint32_t i = 1; i < n; i++)
        if (lock_key(&locks[i - 1u]) >= lock_key(&locks[i]))
            return -1;
    if (n == 0u) {
        (void)song_ext_remove(s, SONG_TAG_LOCK);
        return 0;
    }
    uint32_t len = 3u + n * 5u;
    int32_t at = find_at(s, SONG_TAG_LOCK);
    uint32_t old = at < 0 ? 0u : SONG_CHUNK_HEADER + rd32(s->ext + at + 4);
    if (s->ext_used - old + SONG_CHUNK_HEADER + len > SONG_EXT_BYTES)
        return -1;
    /* Built at the area's end, as one chunk: remove the old one, then append. */
    (void)song_ext_remove(s, SONG_TAG_LOCK);
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
    return 0;
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

static int set_counted(gt2_song* s, uint32_t tag, const uint8_t* head, uint32_t fixed,
                       const uint8_t* list, uint32_t count, uint32_t max) {
    uint8_t b[16];
    if (count > max || 1u + fixed + count > sizeof b)
        return -1;
    b[0] = 1u;
    for (uint32_t i = 0; i < fixed; i++)
        b[1u + i] = head[i];
    for (uint32_t i = 0; i < count; i++)
        b[1u + fixed + i] = list[i];
    return song_ext_set(s, tag, b, 1u + fixed + count);
}

int song_ext_get_midi(const gt2_song* s, song_midi* v) {
    uint8_t head[3];
    memset(v, 0, sizeof *v);
    if (!get_counted(s, SONG_TAG_MIDI, 3u, SONG_MIDI_CHANNELS_MAX, head, v->channel))
        return 0;
    v->mode = head[0];
    v->rows_per_beat = head[1];
    v->count = head[2];
    return 1;
}

int song_ext_set_midi(gt2_song* s, const song_midi* v) {
    uint8_t head[3] = {v->mode, v->rows_per_beat, v->count};
    return set_counted(s, SONG_TAG_MIDI, head, 3u, v->channel, v->count,
                       SONG_MIDI_CHANNELS_MAX);
}

int song_ext_get_scene(const gt2_song* s, song_scene* v) {
    uint8_t head[2];
    memset(v, 0, sizeof *v);
    if (!get_counted(s, SONG_TAG_SCNE, 2u, SONG_SCENE_VOICES_MAX, head, v->role))
        return 0;
    v->piece = head[0];
    v->count = head[1];
    return 1;
}

int song_ext_set_scene(gt2_song* s, const song_scene* v) {
    uint8_t head[2] = {v->piece, v->count};
    return set_counted(s, SONG_TAG_SCNE, head, 2u, v->role, v->count, SONG_SCENE_VOICES_MAX);
}

/* KEYS */

int song_ext_get_keys(const gt2_song* s, song_keys* v) {
    uint32_t len = 0;
    memset(v, 0, sizeof *v);
    const uint8_t* d = get_v1(s, SONG_TAG_KEYS, 2u, &len);
    if (d == NULL || d[1] > SONG_KEYS_MAX || len < 2u + 2u * d[1])
        return 0;
    v->count = d[1];
    for (uint32_t i = 0; i < v->count; i++)
        v->param[i] = rd16(d + 2u + 2u * i);
    return 1;
}

int song_ext_set_keys(gt2_song* s, const song_keys* v) {
    uint8_t b[2u + 2u * SONG_KEYS_MAX];
    if (v->count > SONG_KEYS_MAX)
        return -1;
    b[0] = 1u;
    b[1] = v->count;
    for (uint32_t i = 0; i < v->count; i++)
        wr16(b + 2u + 2u * i, v->param[i]);
    return song_ext_set(s, SONG_TAG_KEYS, b, 2u + 2u * v->count);
}
