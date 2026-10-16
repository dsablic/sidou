// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song model: the region's layout, the arena, the edit primitives (gt2_song.h).
 * What savesong counts follows GoatTracker 2.77's gsong.c (savesong, countpatternlengths),
 * as decision record 0007 ("Round-trip rules") describes. */
#include "gt2_song.h"

#include <stddef.h>
#include <string.h>

static uint32_t header_bytes(void) {
    return ((uint32_t)sizeof(gt2_song) + 3u) & ~3u;
}

uint32_t gt2_region_min(void) {
    /* The empty song (three 64-row patterns, three orderlists), the spare, the bitmap. */
    return header_bytes() + 4096u;
}

static uint32_t* bitmap(gt2_song* s) {
    return (uint32_t*)((uint8_t*)s + s->bitmap_offset);
}

static const uint32_t* bitmap_c(const gt2_song* s) {
    return (const uint32_t*)((const uint8_t*)s + s->bitmap_offset);
}

static uint8_t* arena(gt2_song* s) {
    return (uint8_t*)s + s->arena_offset;
}

static uint32_t units_of(uint32_t bytes) {
    return (bytes + GT2_UNIT - 1u) / GT2_UNIT;
}

uint32_t gt2_order_span_bytes(gt2_span span) {
    return units_of(GT2_SPAN_LEN(span)) * GT2_UNIT;
}

uint32_t gt2_pattern_span_bytes(gt2_span span) {
    return GT2_SPAN_LEN(span) * GT2_ROW_BYTES;
}

static void mark(gt2_song* s, uint32_t off4, uint32_t units, int used) {
    uint32_t* b = bitmap(s);
    for (uint32_t u = off4; u < off4 + units; u++) {
        if (used)
            b[u >> 5] |= 1u << (u & 31u);
        else
            b[u >> 5] &= ~(1u << (u & 31u));
    }
    if (used)
        s->used_units += units;
    else
        s->used_units -= units;
}

static int unit_used(const gt2_song* s, uint32_t u) {
    return (int)((bitmap_c(s)[u >> 5] >> (u & 31u)) & 1u);
}

/* The first free unit at or after from; arena_units if none. Full words are skipped. */
static uint32_t next_free(const gt2_song* s, uint32_t from) {
    const uint32_t* b = bitmap_c(s);
    uint32_t u = from;
    while (u < s->arena_units) {
        if ((u & 31u) == 0u && b[u >> 5] == 0xffffffffu) {
            u += 32u;
            continue;
        }
        if (!unit_used(s, u))
            return u;
        u++;
    }
    return s->arena_units;
}

/* The first used unit at or after from; arena_units if none. */
static uint32_t next_used(const gt2_song* s, uint32_t from) {
    const uint32_t* b = bitmap_c(s);
    uint32_t u = from;
    while (u < s->arena_units) {
        if ((u & 31u) == 0u && b[u >> 5] == 0u) {
            u += 32u;
            continue;
        }
        if (unit_used(s, u))
            return u;
        u++;
    }
    return s->arena_units;
}

/* The first hole of at least units, at or after from. */
static int find_hole(const gt2_song* s, uint32_t units, uint32_t from, uint32_t* off4) {
    uint32_t u = next_free(s, from);
    while (u < s->arena_units) {
        uint32_t end = next_used(s, u);
        if (end > s->arena_units)
            end = s->arena_units;
        if (end - u >= units) {
            *off4 = u;
            return 0;
        }
        u = next_free(s, end);
    }
    return -1;
}

int gt2_arena_alloc(gt2_song* s, uint32_t bytes, uint32_t* off4) {
    uint32_t units = units_of(bytes);
    if (units == 0u || find_hole(s, units, 0u, off4) != 0)
        return -1;
    mark(s, *off4, units, 1);
    return 0;
}

void gt2_arena_release(gt2_song* s, uint32_t off4, uint32_t bytes) {
    mark(s, off4, units_of(bytes), 0);
}

uint32_t gt2_arena_capacity(const gt2_song* s) {
    uint32_t bytes = s->arena_units * GT2_UNIT;
    return bytes > SONG_ARENA_SPARE ? bytes - SONG_ARENA_SPARE : 0u;
}

uint32_t gt2_arena_used(const gt2_song* s) {
    return s->used_units * GT2_UNIT;
}

uint32_t gt2_arena_largest_hole(const gt2_song* s) {
    uint32_t best = 0u;
    uint32_t u = next_free(s, 0u);
    while (u < s->arena_units) {
        uint32_t end = next_used(s, u);
        if (end > s->arena_units)
            end = s->arena_units;
        if (end - u > best)
            best = end - u;
        u = next_free(s, end);
    }
    return best * GT2_UNIT;
}

static void empty_row(uint8_t* r) {
    r[0] = GT2_REST;
    r[1] = 0u;
    r[2] = 0u;
    r[3] = 0u;
}

static void end_row(uint8_t* r) {
    r[0] = GT2_ENDPATT;
    r[1] = 0u;
    r[2] = 0u;
    r[3] = 0u;
}

void gt2_song_wipe(gt2_song* s) {
    uint32_t region = s->region_bytes, arena_offset = s->arena_offset;
    uint32_t units = s->arena_units, bitmap_offset = s->bitmap_offset;
    memset(s, 0, sizeof *s);
    s->region_bytes = region;
    s->arena_offset = arena_offset;
    s->arena_units = units;
    s->bitmap_offset = bitmap_offset;
    memset(bitmap(s), 0, ((units + 31u) / 32u) * 4u);
}

void gt2_song_clear(gt2_song* s) {
    gt2_song_wipe(s);
    s->subtunes = 1u;
    s->channels = GT2_MONO_CHANNELS;
    s->version = 5u;
    /* clearsong: subtune 0's channel c plays pattern c; the patterns are empty, 64 rows. */
    for (uint32_t c = 0; c < GT2_MONO_CHANNELS; c++) {
        uint32_t off4 = 0;
        (void)gt2_arena_alloc(s, 4u, &off4);
        uint8_t* o = arena(s) + off4 * GT2_UNIT;
        o[0] = (uint8_t)c;
        o[1] = GT2_LOOPSONG;
        o[2] = 0u;
        s->order[0][c] = GT2_SPAN(off4, 3u);
        (void)gt2_arena_alloc(s, (GT2_DEFAULT_ROWS + 1u) * GT2_ROW_BYTES, &off4);
        uint8_t* p = arena(s) + off4 * GT2_UNIT;
        for (uint32_t r = 0; r < GT2_DEFAULT_ROWS; r++)
            empty_row(p + r * GT2_ROW_BYTES);
        end_row(p + GT2_DEFAULT_ROWS * GT2_ROW_BYTES);
        s->pattern[c] = GT2_SPAN(off4, GT2_DEFAULT_ROWS + 1u);
    }
}

gt2_song* gt2_song_init(void* region, uint32_t bytes) {
    if (region == NULL || ((uintptr_t)region & 3u) != 0u || bytes < gt2_region_min() ||
        bytes > GT2_REGION_MAX)
        return NULL;
    gt2_song* s = (gt2_song*)region;
    uint32_t hdr = header_bytes();
    uint32_t rest = bytes - hdr;
    /* units * 4 bytes of arena and a bit each, in whole words */
    uint32_t units = (uint32_t)(((uint64_t)rest * 8u) / 33u);
    while (units * GT2_UNIT + ((units + 31u) / 32u) * 4u > rest)
        units--;
    if (units > 0xffffu)
        units = 0xffffu;
    s->region_bytes = bytes;
    s->bitmap_offset = hdr;
    s->arena_offset = hdr + ((units + 31u) / 32u) * 4u;
    s->arena_units = units;
    gt2_song_clear(s);
    return s;
}

const uint8_t* gt2_order_data(const gt2_song* s, uint32_t subtune, uint32_t channel) {
    if (subtune >= GT2_MAX_SUBTUNES || channel >= GT2_MAX_CHANNELS)
        return NULL;
    gt2_span sp = s->order[subtune][channel];
    if (sp == 0u)
        return NULL;
    return gt2_arena(s) + GT2_SPAN_OFF4(sp) * GT2_UNIT;
}

uint32_t gt2_order_entries(const gt2_song* s, uint32_t subtune, uint32_t channel) {
    if (subtune >= GT2_MAX_SUBTUNES || channel >= GT2_MAX_CHANNELS)
        return 0u;
    gt2_span sp = s->order[subtune][channel];
    return sp == 0u ? 0u : GT2_SPAN_LEN(sp) - 2u;
}

const uint8_t* gt2_pattern_data(const gt2_song* s, uint32_t pattern) {
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return NULL;
    return gt2_arena(s) + GT2_SPAN_OFF4(s->pattern[pattern]) * GT2_UNIT;
}

uint32_t gt2_pattern_rows(const gt2_song* s, uint32_t pattern) {
    if (pattern >= GT2_MAX_PATTERNS)
        return 0u;
    gt2_span sp = s->pattern[pattern];
    return sp == 0u ? GT2_DEFAULT_ROWS + 1u : GT2_SPAN_LEN(sp);
}

int gt2_order_valid(const uint8_t* e, uint32_t n) {
    uint32_t i = 0;
    while (i < n) {
        if (e[i] >= GT2_TRANSDOWN && e[i] < GT2_LOOPSONG)
            i++;
        if (i < n && e[i] >= GT2_REPEAT && e[i] < GT2_TRANSDOWN)
            i++;
        if (i >= n || e[i] >= GT2_REPEAT)
            return 0;
        i++;
    }
    return 1;
}

/* Edits. */

void gt2_edit_apply(gt2_song* s, const gt2_edit* e) {
    uint8_t* base = (uint8_t*)s;
    if (e->kind == GT2_EDIT_CELL) {
        for (uint32_t i = 0; i < e->n; i++)
            base[e->offset + i] = e->bytes[i];
    } else if (e->kind == GT2_EDIT_SWAP) {
        *(gt2_span*)(void*)(base + e->offset) = e->span;
    }
}

static int is_pattern_slot(uint32_t offset) {
    return offset >= offsetof(gt2_song, pattern) &&
           offset < offsetof(gt2_song, pattern) + sizeof(((gt2_song*)0)->pattern);
}

static uint32_t span_bytes_at(uint32_t offset, gt2_span sp) {
    return is_pattern_slot(offset) ? gt2_pattern_span_bytes(sp) : gt2_order_span_bytes(sp);
}

void gt2_edit_release(gt2_song* s, const gt2_edit* e) {
    if (e->kind == GT2_EDIT_SWAP && e->old_span != 0u)
        gt2_arena_release(s, GT2_SPAN_OFF4(e->old_span), span_bytes_at(e->offset, e->old_span));
}

void gt2_edit_cancel(gt2_song* s, const gt2_edit* e) {
    if (e->kind == GT2_EDIT_SWAP && e->span != 0u)
        gt2_arena_release(s, GT2_SPAN_OFF4(e->span), span_bytes_at(e->offset, e->span));
}

void gt2_edit_commit(gt2_song* s, const gt2_edit* e) {
    gt2_edit_apply(s, e);
    gt2_edit_release(s, e);
}

static void cell(gt2_edit* e, uint32_t offset, const uint8_t* bytes, uint32_t n) {
    memset(e, 0, sizeof *e);
    e->kind = GT2_EDIT_CELL;
    e->n = (uint8_t)n;
    e->offset = offset;
    for (uint32_t i = 0; i < n; i++)
        e->bytes[i] = bytes[i];
}

static int row_valid(const uint8_t* r) {
    return r[0] >= GT2_FIRSTNOTE && r[0] <= GT2_KEYON && r[1] <= GT2_MAX_INSTR && r[2] <= 0x0fu;
}

gt2_err gt2_set_row(const gt2_song* s, uint32_t pattern, uint32_t row, const uint8_t c[4],
                    gt2_edit* e) {
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    gt2_span sp = s->pattern[pattern];
    if (row + 1u >= GT2_SPAN_LEN(sp) || !row_valid(c))
        return GT2_ERR_RANGE;
    cell(e, s->arena_offset + GT2_SPAN_OFF4(sp) * GT2_UNIT + row * GT2_ROW_BYTES, c, 4u);
    return GT2_OK;
}

gt2_err gt2_set_instr(const gt2_song* s, uint32_t instr, uint32_t field, uint8_t value,
                      gt2_edit* e) {
    (void)s;
    if (instr < 1u || instr > GT2_MAX_INSTR || field >= GT2_INSTR_BYTES)
        return GT2_ERR_RANGE;
    cell(e, (uint32_t)offsetof(gt2_song, instr) + (instr - 1u) * GT2_INSTR_BYTES + field, &value,
         1u);
    return GT2_OK;
}

gt2_err gt2_set_table(const gt2_song* s, uint32_t table, uint32_t row, uint32_t side,
                      uint8_t value, gt2_edit* e) {
    (void)s;
    if (table >= GT2_TABLES || row >= GT2_TABLE_ROWS || side > 1u)
        return GT2_ERR_RANGE;
    uint32_t base =
        side ? (uint32_t)offsetof(gt2_song, rtable) : (uint32_t)offsetof(gt2_song, ltable);
    cell(e, base + table * GT2_TABLE_ROWS + row, &value, 1u);
    return GT2_OK;
}

static int order_index_ok(const gt2_song* s, uint32_t subtune, uint32_t channel) {
    return subtune < s->subtunes && channel < s->channels && s->order[subtune][channel] != 0u;
}

gt2_err gt2_set_order(const gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                      uint8_t value, gt2_edit* e) {
    if (!order_index_ok(s, subtune, channel))
        return GT2_ERR_RANGE;
    const uint8_t* o = gt2_order_data(s, subtune, channel);
    uint32_t n = gt2_order_entries(s, subtune, channel);
    uint32_t base = s->arena_offset + GT2_SPAN_OFF4(s->order[subtune][channel]) * GT2_UNIT;
    if (pos == n) {
        if (n > 0u ? value >= n : value != 0u)
            return GT2_ERR_RANGE;
        cell(e, base + n + 1u, &value, 1u);
        return GT2_OK;
    }
    if (pos > n || value == GT2_LOOPSONG)
        return GT2_ERR_RANGE;
    if (value < GT2_REPEAT && s->pattern[value] == 0u)
        return GT2_ERR_RANGE;
    uint8_t t[GT2_MAX_ORDER];
    for (uint32_t i = 0; i < n; i++)
        t[i] = o[i];
    t[pos] = value;
    if (!gt2_order_valid(t, n))
        return GT2_ERR_ORDER_RULE;
    cell(e, base + pos, &value, 1u);
    return GT2_OK;
}

/* Rebuilds. */

/* Room for a rebuild that replaces old_bytes with new_bytes: the spare is kept once the swap
 * is released, and a hole fits the new copy now. */
static gt2_err rebuild_alloc(gt2_song* s, uint32_t old_bytes, uint32_t new_bytes,
                             uint32_t* off4) {
    uint32_t after = gt2_arena_used(s) - units_of(old_bytes) * GT2_UNIT +
                     units_of(new_bytes) * GT2_UNIT;
    if (after > gt2_arena_capacity(s))
        return GT2_ERR_FULL;
    if (gt2_arena_alloc(s, new_bytes, off4) != 0)
        return GT2_ERR_FRAGMENTED;
    return GT2_OK;
}

static void swap(gt2_edit* e, uint32_t offset, gt2_span span, gt2_span old_span) {
    memset(e, 0, sizeof *e);
    e->kind = GT2_EDIT_SWAP;
    e->offset = offset;
    e->span = span;
    e->old_span = old_span;
}

static uint32_t pattern_slot(uint32_t pattern) {
    return (uint32_t)offsetof(gt2_song, pattern) + pattern * (uint32_t)sizeof(gt2_span);
}

static uint32_t order_slot(uint32_t subtune, uint32_t channel) {
    return (uint32_t)offsetof(gt2_song, order) +
           (subtune * GT2_MAX_CHANNELS + channel) * (uint32_t)sizeof(gt2_span);
}

/* A new pattern of rows rows (the end row included) from the old one: rows [0, keep_at) as
 * they are, an empty row at insert_at if it is below rows, rows from drop_at + 1 on shifted,
 * the old end row last. A pattern not held reads as GT2_DEFAULT_ROWS empty rows. */
static gt2_err pattern_rebuild(gt2_song* s, uint32_t pattern, uint32_t rows, int insert_at,
                               int drop_at, gt2_edit* e) {
    gt2_span old = s->pattern[pattern];
    uint32_t off4 = 0;
    gt2_err err = rebuild_alloc(s, old ? gt2_pattern_span_bytes(old) : 0u,
                                rows * GT2_ROW_BYTES, &off4);
    if (err != GT2_OK)
        return err;
    uint8_t* dst = arena(s) + off4 * GT2_UNIT;
    const uint8_t* src = old ? arena(s) + GT2_SPAN_OFF4(old) * GT2_UNIT : NULL;
    uint32_t old_body = old ? GT2_SPAN_LEN(old) - 1u : 0u;
    uint32_t r_src = 0;
    for (uint32_t r = 0; r + 1u < rows; r++) {
        if ((int)r == insert_at) {
            empty_row(dst + r * GT2_ROW_BYTES);
            continue;
        }
        if ((int)r_src == drop_at)
            r_src++;
        if (r_src < old_body) {
            for (uint32_t b = 0; b < GT2_ROW_BYTES; b++)
                dst[r * GT2_ROW_BYTES + b] = src[r_src * GT2_ROW_BYTES + b];
        } else {
            empty_row(dst + r * GT2_ROW_BYTES);
        }
        r_src++;
    }
    uint8_t* end = dst + (rows - 1u) * GT2_ROW_BYTES;
    if (src) {
        for (uint32_t b = 0; b < GT2_ROW_BYTES; b++)
            end[b] = src[old_body * GT2_ROW_BYTES + b];
    } else {
        end_row(end);
    }
    swap(e, pattern_slot(pattern), GT2_SPAN(off4, rows), old);
    return GT2_OK;
}

gt2_err gt2_pattern_resize(gt2_song* s, uint32_t pattern, uint32_t rows, gt2_edit* e) {
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    if (rows < 1u || rows > GT2_MAX_ROWS)
        return GT2_ERR_ROWS;
    return pattern_rebuild(s, pattern, rows + 1u, -1, -1, e);
}

gt2_err gt2_pattern_insert_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e) {
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    uint32_t body = GT2_SPAN_LEN(s->pattern[pattern]) - 1u;
    if (row > body)
        return GT2_ERR_RANGE;
    if (body + 1u > GT2_MAX_ROWS)
        return GT2_ERR_ROWS;
    return pattern_rebuild(s, pattern, body + 2u, (int)row, -1, e);
}

gt2_err gt2_pattern_delete_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e) {
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    uint32_t body = GT2_SPAN_LEN(s->pattern[pattern]) - 1u;
    if (row >= body)
        return GT2_ERR_RANGE;
    if (body < 2u)
        return GT2_ERR_ROWS;
    return pattern_rebuild(s, pattern, body, -1, (int)row, e);
}

static int pattern_named(const gt2_song* s, uint32_t pattern) {
    for (uint32_t d = 0; d < s->subtunes; d++) {
        for (uint32_t c = 0; c < s->channels; c++) {
            const uint8_t* o = gt2_order_data(s, d, c);
            uint32_t n = gt2_order_entries(s, d, c);
            for (uint32_t i = 0; i < n; i++)
                if (o[i] == pattern)
                    return 1;
        }
    }
    return 0;
}

gt2_err gt2_pattern_add(gt2_song* s, uint32_t rows, uint32_t* pattern, gt2_edit* e) {
    if (rows < 1u || rows > GT2_MAX_ROWS)
        return GT2_ERR_ROWS;
    for (uint32_t p = 0; p < GT2_MAX_PATTERNS; p++) {
        if (s->pattern[p] == 0u && !pattern_named(s, p)) {
            gt2_err err = pattern_rebuild(s, p, rows + 1u, -1, -1, e);
            if (err == GT2_OK)
                *pattern = p;
            return err;
        }
    }
    return GT2_ERR_PATTERNS;
}

gt2_err gt2_pattern_remove(gt2_song* s, uint32_t pattern, gt2_edit* e) {
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    if (pattern_named(s, pattern))
        return GT2_ERR_IN_USE;
    swap(e, pattern_slot(pattern), 0u, s->pattern[pattern]);
    return GT2_OK;
}

/* A new orderlist: the entries t (n of them), $FF, restart. */
static gt2_err order_rebuild(gt2_song* s, uint32_t subtune, uint32_t channel, const uint8_t* t,
                             uint32_t n, uint8_t restart, gt2_edit* e) {
    gt2_span old = s->order[subtune][channel];
    uint32_t off4 = 0;
    gt2_err err = rebuild_alloc(s, gt2_order_span_bytes(old), n + 2u, &off4);
    if (err != GT2_OK)
        return err;
    uint8_t* dst = arena(s) + off4 * GT2_UNIT;
    for (uint32_t i = 0; i < n; i++)
        dst[i] = t[i];
    dst[n] = GT2_LOOPSONG;
    dst[n + 1u] = restart;
    swap(e, order_slot(subtune, channel), GT2_SPAN(off4, n + 2u), old);
    return GT2_OK;
}

gt2_err gt2_order_insert(gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                         uint8_t value, gt2_edit* e) {
    if (!order_index_ok(s, subtune, channel))
        return GT2_ERR_RANGE;
    const uint8_t* o = gt2_order_data(s, subtune, channel);
    uint32_t n = gt2_order_entries(s, subtune, channel);
    if (pos > n || value == GT2_LOOPSONG)
        return GT2_ERR_RANGE;
    if (value < GT2_REPEAT && s->pattern[value] == 0u)
        return GT2_ERR_RANGE;
    if (n >= GT2_MAX_ORDER)
        return GT2_ERR_ORDER_FULL;
    uint8_t t[GT2_MAX_ORDER];
    for (uint32_t i = 0, j = 0; i <= n; i++) {
        if (i == pos)
            t[j++] = value;
        if (i < n)
            t[j++] = o[i];
    }
    if (!gt2_order_valid(t, n + 1u))
        return GT2_ERR_ORDER_RULE;
    uint8_t restart = o[n + 1u];
    if (pos <= restart && restart < n)
        restart++;
    return order_rebuild(s, subtune, channel, t, n + 1u, restart, e);
}

gt2_err gt2_order_delete(gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                         gt2_edit* e) {
    if (!order_index_ok(s, subtune, channel))
        return GT2_ERR_RANGE;
    const uint8_t* o = gt2_order_data(s, subtune, channel);
    uint32_t n = gt2_order_entries(s, subtune, channel);
    if (pos >= n)
        return GT2_ERR_RANGE;
    uint8_t t[GT2_MAX_ORDER];
    for (uint32_t i = 0, j = 0; i < n; i++)
        if (i != pos)
            t[j++] = o[i];
    if (!gt2_order_valid(t, n - 1u))
        return GT2_ERR_ORDER_RULE;
    uint8_t restart = o[n + 1u];
    if (pos < restart)
        restart--;
    if (n - 1u == 0u)
        restart = 0u;
    else if (restart >= n - 1u)
        restart = (uint8_t)(n - 2u);
    return order_rebuild(s, subtune, channel, t, n - 1u, restart, e);
}

/* Compaction. */

/* The index entry whose block starts at unit off4: its slot offset, or 0 if none. */
static uint32_t owner_of(const gt2_song* s, uint32_t off4, gt2_span* span) {
    for (uint32_t p = 0; p < GT2_MAX_PATTERNS; p++) {
        if (s->pattern[p] != 0u && GT2_SPAN_OFF4(s->pattern[p]) == off4) {
            *span = s->pattern[p];
            return pattern_slot(p);
        }
    }
    for (uint32_t d = 0; d < GT2_MAX_SUBTUNES; d++) {
        for (uint32_t c = 0; c < GT2_MAX_CHANNELS; c++) {
            if (s->order[d][c] != 0u && GT2_SPAN_OFF4(s->order[d][c]) == off4) {
                *span = s->order[d][c];
                return order_slot(d, c);
            }
        }
    }
    return 0u;
}

static void move_block(gt2_song* s, uint32_t slot, gt2_span sp, uint32_t to4, gt2_edit* e) {
    uint32_t bytes = span_bytes_at(slot, sp);
    mark(s, to4, units_of(bytes), 1);
    uint8_t* a = arena(s);
    const uint8_t* src = a + GT2_SPAN_OFF4(sp) * GT2_UNIT;
    uint8_t* dst = a + to4 * GT2_UNIT;
    for (uint32_t i = 0; i < bytes; i++)
        dst[i] = src[i];
    swap(e, slot, GT2_SPAN(to4, GT2_SPAN_LEN(sp)), sp);
}

int gt2_compact_step(gt2_song* s, gt2_edit* e) {
    uint32_t gap = next_free(s, 0u);
    if (gap >= s->arena_units)
        return 0;
    uint32_t b = next_used(s, gap);
    if (b >= s->arena_units)
        return 0;
    uint32_t gap_units = b - gap;
    gt2_span sp = 0;
    uint32_t slot = owner_of(s, b, &sp);
    if (slot != 0u) {
        uint32_t units = units_of(span_bytes_at(slot, sp));
        uint32_t to4 = 0;
        /* Down into the gap if it fits there, else out to the first hole that fits. */
        if (gap_units >= units) {
            move_block(s, slot, sp, gap, e);
            return 1;
        }
        if (find_hole(s, units, b + units, &to4) == 0) {
            move_block(s, slot, sp, to4, e);
            return 1;
        }
    }
    /* Fill the lowest gap with a block from above that fits in it. */
    for (uint32_t u = next_used(s, b); u < s->arena_units;) {
        gt2_span other = 0;
        uint32_t oslot = owner_of(s, u, &other);
        uint32_t units = oslot ? units_of(span_bytes_at(oslot, other)) : 1u;
        if (oslot != 0u && units <= gap_units) {
            move_block(s, oslot, other, gap, e);
            return 1;
        }
        u = next_used(s, u + units);
    }
    return 0;
}

/* What savesong writes. */

uint32_t gt2_saved_subtunes(const gt2_song* s) {
    /* Mono and Stereo GoatTracker alike look at channels 1-3 only. */
    uint32_t c = s->subtunes - 1u;
    for (;;) {
        if (gt2_order_entries(s, c, 0) && gt2_order_entries(s, c, 1) &&
            gt2_order_entries(s, c, 2))
            break;
        if (c == 0u)
            break;
        c--;
    }
    return c + 1u;
}

uint32_t gt2_saved_patterns(const gt2_song* s) {
    uint32_t highest = 0;
    for (uint32_t p = 0; p < GT2_MAX_PATTERNS; p++) {
        const uint8_t* d = gt2_pattern_data(s, p);
        if (d == NULL)
            continue;
        uint32_t body = GT2_SPAN_LEN(s->pattern[p]) - 1u;
        for (uint32_t r = 0; r < body; r++) {
            const uint8_t* row = d + r * GT2_ROW_BYTES;
            if (row[0] != GT2_REST || row[1] || row[2] || row[3])
                highest = p;
        }
    }
    for (uint32_t d = 0; d < s->subtunes; d++) {
        for (uint32_t c = 0; c < s->channels; c++) {
            const uint8_t* o = gt2_order_data(s, d, c);
            uint32_t n = gt2_order_entries(s, d, c);
            for (uint32_t i = 0; i < n; i++)
                if (o[i] < GT2_REPEAT && o[i] > highest)
                    highest = o[i];
        }
    }
    return highest + 1u;
}

uint32_t gt2_saved_instruments(const gt2_song* s) {
    uint32_t highest = 0;
    for (uint32_t p = 0; p < GT2_MAX_PATTERNS; p++) {
        const uint8_t* d = gt2_pattern_data(s, p);
        if (d == NULL)
            continue;
        uint32_t body = GT2_SPAN_LEN(s->pattern[p]) - 1u;
        for (uint32_t r = 0; r < body; r++)
            if (d[r * GT2_ROW_BYTES + 1u] > highest)
                highest = d[r * GT2_ROW_BYTES + 1u];
    }
    for (uint32_t i = 1; i <= GT2_MAX_INSTR; i++) {
        const uint8_t* in = s->instr[i - 1u];
        if (in[GT2_INS_AD] || in[GT2_INS_SR] || in[GT2_INS_WAVE] || in[GT2_INS_PULSE] ||
            in[GT2_INS_FILTER] || in[GT2_INS_VIBDELAY] || in[GT2_INS_VIBPARAM])
            if (i > highest)
                highest = i;
    }
    return highest;
}

uint32_t gt2_saved_table_rows(const gt2_song* s, uint32_t table) {
    uint32_t c = GT2_TABLE_ROWS;
    while (c > 0u && !(s->ltable[table][c - 1u] | s->rtable[table][c - 1u]))
        c--;
    return c;
}
