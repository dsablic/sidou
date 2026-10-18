// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song model: the region's layout, the arena, the edit primitives (gt2_song.h).
 * What savesong counts (gt2_saved_*) transliterates GoatTracker 2.77's gsong.c count loops
 * (savesong, countpatternlengths), GPL-2.0-or-later, whose "any later version" lets this
 * file stay GPL-3.0-or-later; decision record 0007 ("Round-trip rules") describes them. */
#include "gt2_song.h"
#include "song_ext.h"

#include <stddef.h>
#include <string.h>

static uint32_t pattern_prefix(const gt2_song* s, uint32_t n);
static uint32_t order_prefix(const gt2_song* s, uint32_t n);
static void saved_pattern_refresh(gt2_song* s, uint32_t p);
static void saved_pattern_mark(gt2_song* s, uint32_t p);
static void saved_instr_refresh(gt2_song* s, uint32_t instr);
static void saved_table_refresh(gt2_song* s, uint32_t table, uint32_t row);
static void saved_order_refs(gt2_song* s, const uint8_t* o, uint32_t n, int add);
static void saved_order_refresh(gt2_song* s, uint32_t d);
static gt2_err saved_check(const gt2_song* s, uint32_t growth);
static uint32_t pattern_growth(const gt2_song* s, uint32_t pattern);
static uint32_t instrument_growth(const gt2_song* s, uint32_t instr);
static uint32_t saved_cell_growth(const gt2_song* s, const gt2_edit* e, uint32_t slot);
static uint32_t saved_order_growth(const gt2_song* s, uint32_t d, uint32_t ch,
                                  const uint8_t* data, uint32_t n);
static uint32_t saved_size_compute(const gt2_song* s);

/* An internal invariant, checked in host builds (the tests and fuzzers trap on it) and
 * compiled out on the device and with NDEBUG, as sid_osc.h's SID_DEBUG_ASSERT. */
#if !defined(NDEBUG) && !defined(__PI32V2__) && (defined(__GNUC__) || defined(__clang__))
#define GT2_ASSERT(c) ((c) ? (void)0 : __builtin_trap())
#else
#define GT2_ASSERT(c) ((void)0)
#endif

#if !defined(__PI32V2__)
#define GT2_ROW_WORK(s) ((s)->saved_row_work++)
#else
#define GT2_ROW_WORK(s) ((void)0)
#endif

_Static_assert(SONG_ARENA_SPARE >= (GT2_SCRATCH_UNITS * GT2_UNIT) + GT2_PATTERN_MAX_BYTES,
               "the spare holds the scratch and a rebuild's new copy");
_Static_assert(GT2_MAX_PATTERNS <= GT2_SLOT_ORDER &&
                   GT2_SLOT_ORDER + GT2_MAX_SUBTUNES * GT2_MAX_CHANNELS < GT2_SLOT_NONE,
               "slot numbers");
_Static_assert(GT2_MAX_PATTERNS * GT2_MAX_ROWS < 65536u, "instrument counts fit modulo 65536");
_Static_assert(GT2_MAX_ROWS <= 255u, "a pattern's nonempty-row count fits one byte");

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

/* Marks units used or free; each must change (a double allocation or free is a bug). */
static void mark(gt2_song* s, uint32_t off4, uint32_t units, int used) {
    uint32_t* b = bitmap(s);
    GT2_ASSERT(off4 + units <= s->arena_units);
    for (uint32_t u = off4; u < off4 + units; u++) {
        uint32_t bit = 1u << (u & 31u);
        GT2_ASSERT(((b[u >> 5] & bit) != 0u) != (used != 0));
        if (used)
            b[u >> 5] |= bit;
        else
            b[u >> 5] &= ~bit;
    }
    GT2_ASSERT(used || s->used_units >= units);
    if (used)
        s->used_units += units;
    else
        s->used_units -= units;
}

/* Where first fit may allocate: below the scratch. */
static uint32_t alloc_limit(const gt2_song* s) {
    return s->arena_units - GT2_SCRATCH_UNITS;
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

/* The first hole of at least units, at or after from, below the scratch. */
static int find_hole(const gt2_song* s, uint32_t units, uint32_t from, uint32_t* off4) {
    uint32_t limit = alloc_limit(s);
    uint32_t u = next_free(s, from);
    while (u < limit) {
        uint32_t end = next_used(s, u);
        if (end > limit)
            end = limit;
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
    uint32_t best = 0u, limit = alloc_limit(s);
    uint32_t u = next_free(s, 0u);
    while (u < limit) {
        uint32_t end = next_used(s, u);
        if (end > limit)
            end = limit;
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

static void song_clear(gt2_song* s) {
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
    gt2_saved_recount(s);
}

int gt2_song_clear(gt2_song* s) {
    if (s->frozen || s->pending != 0u || s->pending_loose != 0u)
        return -1;
    uint32_t revision = s->revision;
    song_clear(s);
    s->revision = revision + 1u;
    return 0;
}

gt2_song* gt2_song_init(void* region, uint32_t bytes) {
    if (region == NULL || ((uintptr_t)region & 3u) != 0u || bytes < gt2_region_min() ||
        bytes > GT2_REGION_MAX)
        return NULL;
    _Static_assert((sizeof(gt2_song) + 3u) / 4u * 4u + 4096u <= GT2_REGION_MAX, "region");
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
    song_clear(s);
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
        if (e->slot < GT2_MAX_PATTERNS) {
            const uint8_t* old = base + e->offset;
            uint32_t was = old[0] != GT2_REST || old[1] || old[2] || old[3];
            uint32_t now = e->bytes[0] != GT2_REST || e->bytes[1] || e->bytes[2] || e->bytes[3];
            GT2_ROW_WORK(s);
            s->saved_pattern_nonempty[e->slot] =
                (uint8_t)(s->saved_pattern_nonempty[e->slot] + now - was);
            uint32_t before = old[1], after = e->bytes[1];
            if (before != after) {
                s->saved_instr_delta[before] = (uint16_t)(s->saved_instr_delta[before] - 1u);
                s->saved_instr_delta[after] = (uint16_t)(s->saved_instr_delta[after] + 1u);
            }
        }
        for (uint32_t i = 0; i < e->n; i++)
            base[e->offset + i] = e->bytes[i];
    } else if (e->kind == GT2_EDIT_SWAP) {
        /* The new copy's bytes (written by the main loop) before the index entry that makes
         * it the player's: a compiler barrier on one core (atomic_signal_fence becomes a call
         * of __sync_synchronize on pi32v2's clang, which the audio interrupt cannot make). The
         * store is one 32-bit st (M2c Task 6 checked the fm1 disassembly). */
        __asm__ volatile("" ::: "memory");
        *(volatile gt2_span*)(void*)(base + e->offset) = e->span;
    }
}

static int is_pattern_slot(uint32_t offset) {
    return offset >= offsetof(gt2_song, pattern) &&
           offset < offsetof(gt2_song, pattern) + sizeof(((gt2_song*)0)->pattern);
}

static uint32_t span_bytes_at(uint32_t offset, gt2_span sp) {
    return is_pattern_slot(offset) ? gt2_pattern_span_bytes(sp) : gt2_order_span_bytes(sp);
}

/* Pending edits (gt2_song.h, the deferred contract). */

static int pattern_pending(const gt2_song* s, uint32_t p) {
    return s->pend_pattern[p] != 0u;
}

static int pattern_swap_pending(const gt2_song* s, uint32_t p) {
    return (s->pend_pattern[p] & 0x80u) != 0u;
}

static int prefix_pending(const gt2_song* s, uint32_t pattern) {
    return s->saved_trim_pending != 0u && pattern + 1u >= s->saved_trim_pending;
}

static uint32_t order_index(uint32_t subtune, uint32_t channel) {
    return subtune * GT2_MAX_CHANNELS + channel;
}

static int order_pending(const gt2_song* s, uint32_t i) {
    return (s->pend_order[i >> 5] >> (i & 31u)) & 1u;
}

/* An edit made on slot: pending until released. */
static void pend(gt2_song* s, gt2_edit* e, uint32_t slot) {
    if (s->pending == 0u && s->pending_loose == 0u) {
        s->saved_epoch = gt2_saved_size(s);
        s->saved_growth = 0u;
    }
    s->saved_growth += e->reserved;
    e->slot = (uint16_t)slot;
    if (slot == GT2_SLOT_NONE) {
        s->pending_loose++;
        return;
    }
    if (e->kind == GT2_EDIT_SWAP && e->old_span != 0u)
        s->pending_old_units += units_of(span_bytes_at(e->offset, e->old_span));
    if (slot >= GT2_SLOT_ORDER) {
        uint32_t i = slot - GT2_SLOT_ORDER;
        GT2_ASSERT(!order_pending(s, i));
        s->pend_order[i >> 5] |= 1u << (i & 31u);
        s->pending_orders++;
    } else if (e->kind == GT2_EDIT_SWAP) {
        GT2_ASSERT(s->pend_pattern[slot] == 0u);
        s->pend_pattern[slot] = 0x80u;
    } else {
        GT2_ASSERT(s->pend_pattern[slot] < 0x7fu);
        s->pend_pattern[slot]++;
    }
    s->pending++;
}

static void unpend(gt2_song* s, const gt2_edit* e) {
    uint32_t slot = e->slot;
    if (e->kind == GT2_EDIT_NONE)
        return;
    if (slot == GT2_SLOT_NONE) {
        GT2_ASSERT(s->pending_loose > 0u);
        s->pending_loose--;
        return;
    }
    if (e->kind == GT2_EDIT_SWAP && e->old_span != 0u) {
        uint32_t units = units_of(span_bytes_at(e->offset, e->old_span));
        GT2_ASSERT(s->pending_old_units >= units);
        s->pending_old_units -= units;
    }
    if (slot >= GT2_SLOT_ORDER) {
        uint32_t i = slot - GT2_SLOT_ORDER;
        GT2_ASSERT(order_pending(s, i));
        s->pend_order[i >> 5] &= ~(1u << (i & 31u));
        s->pending_orders--;
    } else if (e->kind == GT2_EDIT_SWAP) {
        GT2_ASSERT(s->pend_pattern[slot] == 0x80u);
        s->pend_pattern[slot] = 0u;
        if (slot + 1u == s->saved_trim_pending)
            s->saved_trim_pending = 0u;
    } else {
        GT2_ASSERT((s->pend_pattern[slot] & 0x7fu) != 0u);
        s->pend_pattern[slot]--;
    }
    GT2_ASSERT(s->pending > 0u);
    s->pending--;
}

static void edit_release(gt2_song* s, const gt2_edit* e, const uint8_t* locks, uint32_t nlocks) {
    if (e->kind != GT2_EDIT_NONE &&
        (e->kind != GT2_EDIT_SWAP || e->bytes[0] != 0u ||
         GT2_SPAN_LEN(e->span) != GT2_SPAN_LEN(e->old_span))) {
        if (e->slot < GT2_MAX_PATTERNS) {
            if (e->kind == GT2_EDIT_SWAP) {
                uint32_t old = e->old_span ? gt2_pattern_span_bytes(e->old_span)
                                          : (GT2_DEFAULT_ROWS + 1u) * GT2_ROW_BYTES;
                uint32_t now = e->span ? gt2_pattern_span_bytes(e->span)
                                      : (GT2_DEFAULT_ROWS + 1u) * GT2_ROW_BYTES;
                uint32_t group = e->slot / 16u;
                s->saved_pattern_groups[group] =
                    (uint16_t)(s->saved_pattern_groups[group] - old + now);
                if (e->bytes[0] != 0u) {
                    const uint8_t* data = gt2_arena(s) + GT2_SPAN_OFF4(e->old_span) * GT2_UNIT;
                    uint32_t rows = e->old_span ? GT2_SPAN_LEN(e->old_span) - 1u : 0u;
                    for (uint32_t r = 0; r < rows; r++) {
                        GT2_ROW_WORK(s);
                        uint32_t instr = data[r * GT2_ROW_BYTES + 1u];
                        if (instr <= GT2_MAX_INSTR)
                            s->saved_instr_base[instr] = (uint16_t)(s->saved_instr_base[instr] - 1u);
                    }
                    saved_pattern_refresh(s, e->slot);
                }
            } else {
                saved_pattern_mark(s, e->slot);
            }
        } else if (e->slot != GT2_SLOT_NONE) {
            uint32_t index = e->slot - GT2_SLOT_ORDER;
            uint32_t d = index / GT2_MAX_CHANNELS, ch = index % GT2_MAX_CHANNELS;
            if (e->kind == GT2_EDIT_SWAP) {
                const uint8_t* old = gt2_arena(s) + GT2_SPAN_OFF4(e->old_span) * GT2_UNIT;
                saved_order_refs(s, old, GT2_SPAN_LEN(e->old_span) - 2u, 0);
                saved_order_refs(s, gt2_order_data(s, d, ch), gt2_order_entries(s, d, ch), 1);
                uint32_t before = GT2_SPAN_LEN(e->old_span), after = GT2_SPAN_LEN(e->span);
                for (uint32_t i = d + 1u; i <= GT2_MAX_SUBTUNES; i += i & (0u - i))
                    s->saved_order_tree[i] = (uint16_t)(s->saved_order_tree[i] - before + after);
            } else {
                uint32_t base = s->arena_offset + GT2_SPAN_OFF4(s->order[d][ch]) * GT2_UNIT;
                if (e->offset - base < gt2_order_entries(s, d, ch)) {
                    uint8_t old = (uint8_t)e->span;
                    saved_order_refs(s, &old, 1u, 0);
                    saved_order_refs(s, e->bytes, 1u, 1);
                }
            }
            saved_order_refresh(s, d);
        } else if (e->offset >= offsetof(gt2_song, instr) &&
                   e->offset < offsetof(gt2_song, instr) + sizeof s->instr) {
            saved_instr_refresh(s, (e->offset - (uint32_t)offsetof(gt2_song, instr)) /
                                       GT2_INSTR_BYTES + 1u);
        } else if (e->offset >= offsetof(gt2_song, ltable) &&
                   e->offset < offsetof(gt2_song, rtable) + sizeof s->rtable) {
            uint32_t base = e->offset >= offsetof(gt2_song, rtable)
                                ? (uint32_t)offsetof(gt2_song, rtable)
                                : (uint32_t)offsetof(gt2_song, ltable);
            uint32_t index = e->offset - base;
            saved_table_refresh(s, index / GT2_TABLE_ROWS, index % GT2_TABLE_ROWS);
        }
        gt2_song_changed(s);
    }
    if (e->kind == GT2_EDIT_SWAP && e->old_span != 0u)
        gt2_arena_release(s, GT2_SPAN_OFF4(e->old_span), span_bytes_at(e->offset, e->old_span));
    /* A pattern's rebuild moves its locks with its rows, once the player has the new copy
     * (M2c Task 7): an edit never applied (gt2_edit_cancel) moves nothing. */
    if (e->kind == GT2_EDIT_SWAP && e->bytes[0] != 0u && is_pattern_slot(e->offset)) {
        uint8_t frozen = s->frozen;
        s->frozen = 0u;
        song_locks_pattern_edit(s, e->bytes[0], e->slot,
                                (uint32_t)e->bytes[1] | (uint32_t)e->bytes[2] << 8);
        song_locks_restore_reserved(s, locks, nlocks);
        s->frozen = frozen;
    }
    unpend(s, e);
    s->saved_bytes = saved_size_compute(s);
    if (s->pending == 0u && s->pending_loose == 0u)
        s->saved_growth = 0u;
}

void gt2_edit_release(gt2_song* s, const gt2_edit* e) {
    edit_release(s, e, 0, 0u);
}

void gt2_edit_release_with_locks(gt2_song* s, const gt2_edit* e, const uint8_t* locks, uint32_t n) {
    edit_release(s, e, locks, n);
}

void gt2_edit_cancel(gt2_song* s, const gt2_edit* e) {
    s->saved_growth -= e->reserved;
    if (e->kind == GT2_EDIT_SWAP && e->span != 0u)
        gt2_arena_release(s, GT2_SPAN_OFF4(e->span), span_bytes_at(e->offset, e->span));
    unpend(s, e);
    if (s->pending == 0u && s->pending_loose == 0u)
        s->saved_growth = 0u;
}

void gt2_edit_commit(gt2_song* s, const gt2_edit* e) {
    void (*volatile apply)(gt2_song*, const gt2_edit*) = gt2_edit_apply;
    apply(s, e);
    gt2_edit_release(s, e);
}

static void cell(gt2_edit* e, uint32_t offset, const uint8_t* bytes, uint32_t n) {
    memset(e, 0, sizeof *e);
    e->kind = GT2_EDIT_CELL;
    e->n = (uint8_t)n;
    e->offset = offset;
    e->slot = GT2_SLOT_NONE;
    for (uint32_t i = 0; i < n; i++)
        e->bytes[i] = bytes[i];
}

static int row_valid(const uint8_t* r) {
    return r[0] >= GT2_FIRSTNOTE && r[0] <= GT2_KEYON && r[1] <= GT2_MAX_INSTR && r[2] <= 0x0fu;
}

static gt2_err cell_check(const gt2_song* s, gt2_edit* e, uint32_t slot) {
    uint32_t growth = saved_cell_growth(s, e, slot);
    gt2_err err = saved_check(s, growth);
    if (err == GT2_OK)
        e->reserved = s->region_bytes <= SONG_REGION_BYTES ? (uint16_t)growth : 0u;
    return err;
}

gt2_err gt2_set_row(gt2_song* s, uint32_t pattern, uint32_t row, const uint8_t c[4],
                    gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    if (pattern_swap_pending(s, pattern) || s->pend_pattern[pattern] >= 0x7fu)
        return GT2_ERR_PENDING;
    gt2_span sp = s->pattern[pattern];
    if (row + 1u >= GT2_SPAN_LEN(sp) || !row_valid(c))
        return GT2_ERR_RANGE;
    if (prefix_pending(s, pattern) && (c[0] != GT2_REST || c[1] || c[2] || c[3]))
        return GT2_ERR_PENDING;
    cell(e, s->arena_offset + GT2_SPAN_OFF4(sp) * GT2_UNIT + row * GT2_ROW_BYTES, c, 4u);
    gt2_err err = cell_check(s, e, pattern);
    if (err != GT2_OK)
        return err;
    pend(s, e, pattern);
    return GT2_OK;
}

gt2_err gt2_set_instr(gt2_song* s, uint32_t instr, uint32_t field, uint8_t value,
                      gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (instr < 1u || instr > GT2_MAX_INSTR || field >= GT2_INSTR_BYTES)
        return GT2_ERR_RANGE;
    cell(e, (uint32_t)offsetof(gt2_song, instr) + (instr - 1u) * GT2_INSTR_BYTES + field, &value,
         1u);
    gt2_err err = cell_check(s, e, GT2_SLOT_NONE);
    if (err != GT2_OK)
        return err;
    pend(s, e, GT2_SLOT_NONE);
    return GT2_OK;
}

gt2_err gt2_set_table(gt2_song* s, uint32_t table, uint32_t row, uint32_t side,
                      uint8_t value, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (table >= GT2_TABLES || row >= GT2_TABLE_ROWS || side > 1u)
        return GT2_ERR_RANGE;
    uint32_t base =
        side ? (uint32_t)offsetof(gt2_song, rtable) : (uint32_t)offsetof(gt2_song, ltable);
    cell(e, base + table * GT2_TABLE_ROWS + row, &value, 1u);
    gt2_err err = cell_check(s, e, GT2_SLOT_NONE);
    if (err != GT2_OK)
        return err;
    pend(s, e, GT2_SLOT_NONE);
    return GT2_OK;
}

static int order_index_ok(const gt2_song* s, uint32_t subtune, uint32_t channel) {
    return subtune < s->subtunes && channel < s->channels && s->order[subtune][channel] != 0u;
}

gt2_err gt2_set_order(gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                      uint8_t value, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (!order_index_ok(s, subtune, channel))
        return GT2_ERR_RANGE;
    uint32_t slot = GT2_SLOT_ORDER + order_index(subtune, channel);
    if (order_pending(s, order_index(subtune, channel)) ||
        (value < GT2_REPEAT && pattern_swap_pending(s, value)))
        return GT2_ERR_PENDING;
    const uint8_t* o = gt2_order_data(s, subtune, channel);
    uint32_t n = gt2_order_entries(s, subtune, channel);
    uint32_t base = s->arena_offset + GT2_SPAN_OFF4(s->order[subtune][channel]) * GT2_UNIT;
    if (pos == n) {
        if (n > 0u ? value >= n : value != 0u)
            return GT2_ERR_RANGE;
        cell(e, base + n + 1u, &value, 1u);
        pend(s, e, slot);
        return GT2_OK;
    }
    if (pos > n || value == GT2_LOOPSONG)
        return GT2_ERR_RANGE;
    if (value < GT2_REPEAT && prefix_pending(s, value))
        return GT2_ERR_PENDING;
    if (value < GT2_REPEAT && s->pattern[value] == 0u)
        return GT2_ERR_RANGE;
    uint8_t t[GT2_MAX_ORDER];
    for (uint32_t i = 0; i < n; i++)
        t[i] = o[i];
    t[pos] = value;
    if (!gt2_order_valid(t, n))
        return GT2_ERR_ORDER_RULE;
    cell(e, base + pos, &value, 1u);
    e->span = o[pos];
    gt2_err err = cell_check(s, e, slot);
    if (err != GT2_OK)
        return err;
    pend(s, e, slot);
    return GT2_OK;
}

/* Rebuilds. */

/* Room for a rebuild that replaces old_bytes with new_bytes: the spare is kept once the swap
 * is released, and a hole fits the new copy now. */
static int scratch_busy(const gt2_song* s) {
    return next_used(s, alloc_limit(s)) < s->arena_units;
}

/* Room for a rebuild that replaces old_bytes with new_bytes: the spare is kept once every
 * pending edit is released (the size at rest: the old copies pending swaps hold do not
 * count; FULL only if it does not fit even so), and a hole fits the new copy now (else
 * PENDING while old copies wait for release, FRAGMENTED otherwise: compact, then again).
 * While the scratch holds a block (half a compaction move), nothing is allocated, so the
 * hole that block came from stays for the move's second half: FRAGMENTED, compact once
 * more. */
static gt2_err rebuild_alloc(gt2_song* s, uint32_t old_bytes, uint32_t new_bytes,
                             uint32_t* off4) {
    uint32_t at_rest = (s->used_units - s->pending_old_units) * GT2_UNIT;
    uint32_t after = at_rest - units_of(old_bytes) * GT2_UNIT + units_of(new_bytes) * GT2_UNIT;
    if (after > gt2_arena_capacity(s))
        return GT2_ERR_FULL;
    if (scratch_busy(s) || gt2_arena_alloc(s, new_bytes, off4) != 0)
        return s->pending != 0u ? GT2_ERR_PENDING : GT2_ERR_FRAGMENTED;
    return GT2_OK;
}

static uint32_t pattern_slot(uint32_t pattern) {
    return (uint32_t)offsetof(gt2_song, pattern) + pattern * (uint32_t)sizeof(gt2_span);
}

static uint32_t order_slot(uint32_t subtune, uint32_t channel) {
    return (uint32_t)offsetof(gt2_song, order) +
           order_index(subtune, channel) * (uint32_t)sizeof(gt2_span);
}

/* A swap of the index entry at offset, pending on its slot. */
static void swap(gt2_song* s, gt2_edit* e, uint32_t offset, gt2_span span, gt2_span old_span,
                 uint32_t growth) {
    memset(e, 0, sizeof *e);
    e->kind = GT2_EDIT_SWAP;
    e->offset = offset;
    e->span = span;
    e->old_span = old_span;
    e->reserved = s->region_bytes <= SONG_REGION_BYTES ? (uint16_t)growth : 0u;
    uint32_t slot = is_pattern_slot(offset)
                        ? (offset - (uint32_t)offsetof(gt2_song, pattern)) / sizeof(gt2_span)
                        : GT2_SLOT_ORDER +
                              (offset - (uint32_t)offsetof(gt2_song, order)) / sizeof(gt2_span);
    pend(s, e, slot);
}

/* A new pattern of rows rows (the end row included) from the old one: rows [0, keep_at) as
 * they are, an empty row at insert_at if it is below rows, rows from drop_at + 1 on shifted,
 * the old end row last. A pattern not held reads as GT2_DEFAULT_ROWS empty rows. */
static gt2_err pattern_rebuild_full(gt2_song* s, uint32_t pattern, uint32_t rows, int insert_at,
                                   int drop_at, const uint8_t* restore, const uint8_t* locks,
                                   uint32_t nlocks, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    gt2_span old = s->pattern[pattern];
    uint32_t off4 = 0;
    if (pattern_pending(s, pattern))
        return GT2_ERR_PENDING;
    uint32_t old_bytes = old ? gt2_pattern_span_bytes(old)
                             : (GT2_DEFAULT_ROWS + 1u) * GT2_ROW_BYTES;
    uint32_t new_bytes = rows * GT2_ROW_BYTES;
    uint32_t growth = new_bytes > old_bytes ? new_bytes - old_bytes : 0u;
    uint32_t op = old == 0u ? SONG_LOCKS_REMOVE
                  : insert_at >= 0 ? (rows == GT2_SPAN_LEN(old) ? SONG_LOCKS_PUSH : SONG_LOCKS_INSERT)
                  : drop_at >= 0 ? SONG_LOCKS_DELETE : SONG_LOCKS_RESIZE;
    uint32_t lock_row = insert_at >= 0 ? (uint32_t)insert_at
                                     : drop_at >= 0 ? (uint32_t)drop_at : rows - 1u;
    if (op == SONG_LOCKS_PUSH)
        lock_row |= (rows - 1u) << 8;
    uint32_t before_trailer = gt2_saved_trailer_size(s->ext_used);
    uint32_t ext_used;
    if (restore) {
        uint32_t restored_row = insert_at >= 0 ? (uint32_t)insert_at : rows - 2u;
        gt2_err check = song_locks_restore_check(s, op, pattern, lock_row, restored_row,
                                                locks, nlocks, &ext_used);
        if (check != GT2_OK)
            return check;
        if (restore[0] != GT2_REST || restore[1] || restore[2] || restore[3])
            growth += pattern_growth(s, pattern);
        growth += instrument_growth(s, restore[1]);
    } else
        ext_used = song_locks_pattern_size(s, op, pattern, lock_row);
    uint32_t after_trailer = gt2_saved_trailer_size(ext_used);
    if (after_trailer > before_trailer)
        growth += after_trailer - before_trailer;
    gt2_err cap = saved_check(s, growth);
    if (cap != GT2_OK)
        return cap;
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
    if (restore) {
        uint32_t restored_row = insert_at >= 0 ? (uint32_t)insert_at : rows - 2u;
        memcpy(dst + restored_row * GT2_ROW_BYTES, restore, GT2_ROW_BYTES);
    }
    swap(s, e, pattern_slot(pattern), GT2_SPAN(off4, rows), old, growth);
    return GT2_OK;
}

static gt2_err pattern_rebuild(gt2_song* s, uint32_t pattern, uint32_t rows, int insert_at,
                               int drop_at, gt2_edit* e) {
    return pattern_rebuild_full(s, pattern, rows, insert_at, drop_at, 0, 0, 0u, e);
}

/* The locks' rule for a pattern rebuild's swap, carried in the edit (gt2_edit_release). */
static gt2_err with_locks(gt2_err err, gt2_edit* e, uint32_t op, uint32_t row) {
    if (err == GT2_OK) {
        e->bytes[0] = (uint8_t)op;
        e->bytes[1] = (uint8_t)row;
        e->bytes[2] = 0u;
    }
    return err;
}

gt2_err gt2_pattern_resize(gt2_song* s, uint32_t pattern, uint32_t rows, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    if (rows < 1u || rows > GT2_MAX_ROWS)
        return GT2_ERR_ROWS;
    return with_locks(pattern_rebuild(s, pattern, rows + 1u, -1, -1, e), e, SONG_LOCKS_RESIZE,
                      rows);
}

gt2_err gt2_pattern_insert_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    uint32_t body = GT2_SPAN_LEN(s->pattern[pattern]) - 1u;
    if (row > body)
        return GT2_ERR_RANGE;
    if (body + 1u > GT2_MAX_ROWS)
        return GT2_ERR_ROWS;
    return with_locks(pattern_rebuild(s, pattern, body + 2u, (int)row, -1, e), e,
                      SONG_LOCKS_INSERT, row);
}

gt2_err gt2_pattern_push_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    uint32_t body = GT2_SPAN_LEN(s->pattern[pattern]) - 1u;
    if (row >= body)
        return GT2_ERR_RANGE;
    gt2_err err = with_locks(pattern_rebuild(s, pattern, body + 1u, (int)row, -1, e), e,
                             SONG_LOCKS_PUSH, row);
    if (err == GT2_OK)
        e->bytes[2] = (uint8_t)body; /* the locks reaching it go */
    return err;
}

gt2_err gt2_pattern_pull_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    uint32_t body = GT2_SPAN_LEN(s->pattern[pattern]) - 1u;
    if (row >= body)
        return GT2_ERR_RANGE;
    return with_locks(pattern_rebuild(s, pattern, body + 1u, -1, (int)row, e), e,
                      SONG_LOCKS_DELETE, row);
}

gt2_err gt2_pattern_shift_restore(gt2_song* s, uint32_t pattern, uint32_t row, int insert,
                                 const uint8_t cell[4], const uint8_t* locks, uint32_t n,
                                 gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (s->pending || s->pending_loose)
        return GT2_ERR_PENDING;
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u || !cell || !row_valid(cell))
        return GT2_ERR_RANGE;
    uint32_t body = GT2_SPAN_LEN(s->pattern[pattern]) - 1u;
    if (row >= body)
        return GT2_ERR_RANGE;
    gt2_err err = with_locks(pattern_rebuild_full(s, pattern, body + 1u,
                                                 insert ? (int)row : -1, insert ? -1 : (int)row,
                                                 cell, locks, n, e), e,
                             insert ? SONG_LOCKS_PUSH : SONG_LOCKS_DELETE, row);
    if (err == GT2_OK && insert)
        e->bytes[2] = (uint8_t)body;
    return err;
}

gt2_err gt2_pattern_delete_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    uint32_t body = GT2_SPAN_LEN(s->pattern[pattern]) - 1u;
    if (row >= body)
        return GT2_ERR_RANGE;
    if (body < 2u)
        return GT2_ERR_ROWS;
    return with_locks(pattern_rebuild(s, pattern, body, -1, (int)row, e), e, SONG_LOCKS_DELETE,
                      row);
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
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (rows < 1u || rows > GT2_MAX_ROWS)
        return GT2_ERR_ROWS;
    for (uint32_t p = 0; p < GT2_MAX_PATTERNS; p++) {
        if (s->pattern[p] == 0u && !pattern_pending(s, p) && !pattern_named(s, p)) {
            gt2_err err = pattern_rebuild(s, p, rows + 1u, -1, -1, e);
            if (err == GT2_OK)
                *pattern = p;
            return with_locks(err, e, SONG_LOCKS_REMOVE, 0u); /* a fresh pattern: none */
        }
    }
    return GT2_ERR_PATTERNS;
}

gt2_err gt2_pattern_remove(gt2_song* s, uint32_t pattern, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (pattern >= GT2_MAX_PATTERNS || s->pattern[pattern] == 0u)
        return GT2_ERR_RANGE;
    /* The orderlists are checked as they will stand: none may have an edit pending. */
    if (pattern_pending(s, pattern) || s->pending_orders != 0u)
        return GT2_ERR_PENDING;
    if (pattern_named(s, pattern))
        return GT2_ERR_IN_USE;
    uint32_t old = gt2_pattern_span_bytes(s->pattern[pattern]);
    uint32_t deflt = (GT2_DEFAULT_ROWS + 1u) * GT2_ROW_BYTES;
    uint32_t growth = deflt > old ? deflt - old : 0u;
    uint32_t before_trailer = gt2_saved_trailer_size(s->ext_used);
    uint32_t after_trailer = gt2_saved_trailer_size(
        song_locks_pattern_size(s, SONG_LOCKS_REMOVE, pattern, 0u));
    if (after_trailer > before_trailer)
        growth += after_trailer - before_trailer;
    int trailing = pattern != 0u && pattern + 1u == gt2_saved_patterns(s);
    if (trailing && s->region_bytes <= SONG_REGION_BYTES) {
        for (uint32_t p = pattern + 1u; p < GT2_MAX_PATTERNS; p++)
            if (pattern_pending(s, p))
                return GT2_ERR_PENDING;
    }
    if (trailing)
        growth = 0u; /* Trimming the last pattern saves at least nine bytes, padding included. */
    gt2_err cap = saved_check(s, growth);
    if (cap != GT2_OK)
        return cap;
    swap(s, e, pattern_slot(pattern), 0u, s->pattern[pattern], growth);
    if (trailing && s->region_bytes <= SONG_REGION_BYTES)
        s->saved_trim_pending = (uint8_t)(pattern + 1u);
    return with_locks(GT2_OK, e, SONG_LOCKS_REMOVE, 0u);
}

/* A new orderlist: the entries t (n of them), $FF, restart. */
static gt2_err order_rebuild(gt2_song* s, uint32_t subtune, uint32_t channel, const uint8_t* t,
                             uint32_t n, uint8_t restart, gt2_edit* e) {
    gt2_span old = s->order[subtune][channel];
    uint32_t off4 = 0;
    uint32_t growth = saved_order_growth(s, subtune, channel, t, n);
    gt2_err cap = saved_check(s, growth);
    if (cap != GT2_OK)
        return cap;
    gt2_err err = rebuild_alloc(s, gt2_order_span_bytes(old), n + 2u, &off4);
    if (err != GT2_OK)
        return err;
    uint8_t* dst = arena(s) + off4 * GT2_UNIT;
    for (uint32_t i = 0; i < n; i++)
        dst[i] = t[i];
    dst[n] = GT2_LOOPSONG;
    dst[n + 1u] = restart;
    swap(s, e, order_slot(subtune, channel), GT2_SPAN(off4, n + 2u), old, growth);
    e->bytes[0] = 1u; /* A semantic rewrite, even at the same length; compaction stays unmarked. */
    return GT2_OK;
}

gt2_err gt2_order_insert(gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                         uint8_t value, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (!order_index_ok(s, subtune, channel))
        return GT2_ERR_RANGE;
    if (order_pending(s, order_index(subtune, channel)) ||
        (value < GT2_REPEAT && pattern_swap_pending(s, value)))
        return GT2_ERR_PENDING;
    const uint8_t* o = gt2_order_data(s, subtune, channel);
    uint32_t n = gt2_order_entries(s, subtune, channel);
    if (pos > n || value == GT2_LOOPSONG)
        return GT2_ERR_RANGE;
    if (value < GT2_REPEAT && prefix_pending(s, value))
        return GT2_ERR_PENDING;
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
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (!order_index_ok(s, subtune, channel))
        return GT2_ERR_RANGE;
    if (order_pending(s, order_index(subtune, channel)))
        return GT2_ERR_PENDING;
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

gt2_err gt2_order_replace(gt2_song* s, uint32_t subtune, uint32_t channel,
                          const uint8_t* entries, uint32_t n, uint8_t restart, gt2_edit* e) {
    if (s->frozen)
        return GT2_ERR_SAVING;
    if (!order_index_ok(s, subtune, channel) || (n != 0u && entries == NULL))
        return GT2_ERR_RANGE;
    if (n > GT2_MAX_ORDER)
        return GT2_ERR_ORDER_FULL;
    if (order_pending(s, order_index(subtune, channel)))
        return GT2_ERR_PENDING;
    for (uint32_t i = 0; i < n; i++) {
        uint8_t v = entries[i];
        if (v < GT2_REPEAT && (pattern_swap_pending(s, v) || prefix_pending(s, v)))
            return GT2_ERR_PENDING;
        if (v == GT2_LOOPSONG || (v < GT2_REPEAT && s->pattern[v] == 0u))
            return GT2_ERR_RANGE;
    }
    if (!gt2_order_valid(entries, n))
        return GT2_ERR_ORDER_RULE;
    if (n > 0u ? restart >= n : restart != 0u)
        return GT2_ERR_RANGE;
    return order_rebuild(s, subtune, channel, entries, n, restart, e);
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
    swap(s, e, slot, GT2_SPAN(to4, GT2_SPAN_LEN(sp)), sp, 0u);
}

int gt2_compact_step(gt2_song* s, gt2_edit* e) {
    if (s->frozen)
        return -1;
    if (s->pending != 0u)
        return -1;
    uint32_t limit = alloc_limit(s);
    gt2_span sp = 0;
    uint32_t gap = next_free(s, 0u);
    /* A block in the scratch (the last move's) goes down into the first hole that holds it.
     * One does: the gap it left, grown by the block's old place, which nothing allocates
     * from meanwhile (rebuild_alloc); uninterrupted, that is the lowest gap. */
    uint32_t sc = next_used(s, limit);
    if (sc < s->arena_units) {
        uint32_t slot = owner_of(s, sc, &sp), to4 = 0;
        GT2_ASSERT(slot != 0u);
        if (slot == 0u || find_hole(s, units_of(span_bytes_at(slot, sp)), 0u, &to4) != 0) {
            GT2_ASSERT(0);
            return 0;
        }
        move_block(s, slot, sp, to4, e);
        return 1;
    }
    if (gap >= limit)
        return 0;
    uint32_t b = next_used(s, gap);
    if (b >= limit)
        return 0; /* one hole, from gap to the scratch */
    /* The block right above the lowest gap: into the gap if it fits, else out to the
     * scratch (the next step brings it down). */
    uint32_t slot = owner_of(s, b, &sp);
    GT2_ASSERT(slot != 0u);
    if (slot == 0u)
        return 0;
    uint32_t units = units_of(span_bytes_at(slot, sp));
    move_block(s, slot, sp, b - gap >= units ? gap : limit, e);
    return 1;
}

/* What savesong writes. */

static uint32_t highest_bit(uint32_t bits) {
    uint32_t bit = 0u;
    if (bits >= 0x10000u) {
        bits >>= 16;
        bit += 16u;
    }
    if (bits >= 0x100u) {
        bits >>= 8;
        bit += 8u;
    }
    if (bits >= 0x10u) {
        bits >>= 4;
        bit += 4u;
    }
    if (bits >= 4u) {
        bits >>= 2;
        bit += 2u;
    }
    return bit + (bits >= 2u);
}

uint32_t gt2_saved_subtunes(const gt2_song* s) {
    uint32_t bits = s->saved_subtune_bits | 1u;
    return highest_bit(bits) + 1u;
}

uint32_t gt2_saved_patterns(const gt2_song* s) {
    for (uint32_t i = 7u; i > 0u; i--)
        if (s->saved_pattern_bits[i - 1u])
            return (i - 1u) * 32u + highest_bit(s->saved_pattern_bits[i - 1u]) + 1u;
    return 1u;
}

uint32_t gt2_saved_instruments(const gt2_song* s) {
    uint32_t highest = 0u;
    for (uint32_t i = 2u; i > 0u; i--) {
        uint32_t bits = s->saved_instr_defs[i - 1u];
        if (bits) {
            highest = (i - 1u) * 32u + highest_bit(bits);
            break;
        }
    }
    for (uint32_t instr = GT2_MAX_INSTR; instr > highest; instr--)
        if ((uint16_t)(s->saved_instr_base[instr] + s->saved_instr_delta[instr]) != 0u)
            return instr;
    return highest;
}

uint32_t gt2_saved_table_rows(const gt2_song* s, uint32_t table) {
    if (table >= GT2_TABLES)
        return 0u;
    for (uint32_t i = 8u; i > 0u; i--)
        if (s->saved_table_bits[table][i - 1u])
            return (i - 1u) * 32u + highest_bit(s->saved_table_bits[table][i - 1u]) + 1u;
    return 0u;
}

static void set_bit(uint32_t* bits, uint32_t index, int value) {
    uint32_t bit = 1u << (index & 31u);
    uint32_t word = bits[index >> 5];
    __asm__ volatile("" ::: "memory");
    if (value)
        word |= bit;
    else
        word &= ~bit;
    bits[index >> 5] = word;
}

static uint32_t pattern_prefix(const gt2_song* s, uint32_t n) {
    uint32_t sum = 0u;
    uint32_t groups = n / 16u;
    for (uint32_t i = 0; i < groups; i++)
        sum += s->saved_pattern_groups[i];
    for (uint32_t p = groups * 16u; p < n; p++)
        sum += 1u + gt2_pattern_rows(s, p) * GT2_ROW_BYTES;
    return sum;
}

static uint32_t order_prefix(const gt2_song* s, uint32_t n) {
    uint32_t sum = 0u;
    for (; n > 0u; n &= n - 1u)
        sum += s->saved_order_tree[n];
    return sum;
}

uint32_t gt2_saved_trailer_size(uint32_t ext_used) {
    uint32_t length = SONG_TRAILER_MIN + ext_used;
    if ((length & 0xffu) == 0xffu || ((length >> 8) & 0xffu) == 0xffu) {
        length += SONG_CHUNK_HEADER + 1u;
        while ((length & 0xffu) == 0xffu || ((length >> 8) & 0xffu) == 0xffu)
            length++;
    }
    return length;
}

uint32_t gt2_saved_size(const gt2_song* s) {
    return s->saved_bytes;
}

static uint32_t saved_size_compute(const gt2_song* s) {
    uint32_t size = 107u + order_prefix(s, gt2_saved_subtunes(s)) +
                    GT2_INSTR_BYTES * gt2_saved_instruments(s) +
                    pattern_prefix(s, gt2_saved_patterns(s));
    for (uint32_t t = 0; t < GT2_TABLES; t++)
        size += 2u * gt2_saved_table_rows(s, t);
    return size + gt2_saved_trailer_size(s->ext_used);
}

static void saved_pattern_mark(gt2_song* s, uint32_t p) {
    set_bit(s->saved_pattern_bits, p,
            s->saved_pattern_nonempty[p] != 0u || s->saved_pattern_refs[p] != 0u);
}

static void saved_pattern_refresh(gt2_song* s, uint32_t p) {
    if (p >= GT2_MAX_PATTERNS)
        return;
    uint8_t nonempty = 0u;
    const uint8_t* data = gt2_pattern_data(s, p);
    if (data) {
        uint32_t n = GT2_SPAN_LEN(s->pattern[p]) - 1u;
        for (uint32_t r = 0; r < n; r++) {
            GT2_ROW_WORK(s);
            const uint8_t* row = data + r * GT2_ROW_BYTES;
            nonempty += (uint8_t)(row[0] != GT2_REST || row[1] || row[2] || row[3]);
            uint32_t instr = row[1];
            if (instr <= GT2_MAX_INSTR)
                s->saved_instr_base[instr] = (uint16_t)(s->saved_instr_base[instr] + 1u);
        }
    }
    s->saved_pattern_nonempty[p] = nonempty;
    saved_pattern_mark(s, p);
}

static void saved_instr_refresh(gt2_song* s, uint32_t instr) {
    const uint8_t* in = s->instr[instr - 1u];
    set_bit(s->saved_instr_defs, instr,
            in[0] || in[1] || in[2] || in[3] || in[4] || in[5] || in[6]);
}

static void saved_table_refresh(gt2_song* s, uint32_t table, uint32_t row) {
    set_bit(s->saved_table_bits[table], row, s->ltable[table][row] || s->rtable[table][row]);
}

static void saved_order_refs(gt2_song* s, const uint8_t* o, uint32_t n, int add) {
    for (uint32_t i = 0; i < n; i++) {
        GT2_ROW_WORK(s);
        uint32_t p = o[i];
        if (p >= GT2_REPEAT)
            continue;
        if (add)
            s->saved_pattern_refs[p]++;
        else
            s->saved_pattern_refs[p]--;
        saved_pattern_mark(s, p);
    }
}

static void saved_order_refresh(gt2_song* s, uint32_t d) {
    int used = gt2_order_entries(s, d, 0u) && gt2_order_entries(s, d, 1u) &&
               gt2_order_entries(s, d, 2u);
    set_bit(&s->saved_subtune_bits, d, used);
}

int gt2_saved_recount_step(gt2_song* s, uint32_t* cursor) {
    uint32_t at = (*cursor)++;
    if (at == 0u) {
        memset(s->saved_pattern_groups, 0,
               offsetof(gt2_song, frozen) - offsetof(gt2_song, saved_pattern_groups));
        s->saved_epoch = 0u;
        s->saved_growth = 0u;
        s->saved_bytes = 0u;
        s->saved_row_work = 0u;
        s->saved_trim_pending = 0u;
        return 0;
    }
    at--;
    if (at < GT2_MAX_PATTERNS) {
        uint32_t bytes = 1u + gt2_pattern_rows(s, at) * GT2_ROW_BYTES;
        s->saved_pattern_groups[at / 16u] =
            (uint16_t)(s->saved_pattern_groups[at / 16u] + bytes);
        saved_pattern_refresh(s, at);
        return 0;
    }
    at -= GT2_MAX_PATTERNS;
    if (at < GT2_MAX_SUBTUNES * GT2_MAX_CHANNELS) {
        uint32_t d = at / GT2_MAX_CHANNELS, ch = at % GT2_MAX_CHANNELS;
        if (d < s->subtunes && ch < s->channels) {
            uint32_t n = gt2_order_entries(s, d, ch);
            saved_order_refs(s, gt2_order_data(s, d, ch), n, 1);
            for (uint32_t i = d + 1u; i <= GT2_MAX_SUBTUNES; i += i & (0u - i))
                s->saved_order_tree[i] = (uint16_t)(s->saved_order_tree[i] + n + 3u);
            saved_order_refresh(s, d);
        }
        return 0;
    }
    at -= GT2_MAX_SUBTUNES * GT2_MAX_CHANNELS;
    if (at < GT2_MAX_INSTR) {
        saved_instr_refresh(s, at + 1u);
        return 0;
    }
    at -= GT2_MAX_INSTR;
    if (at < GT2_TABLES * GT2_TABLE_ROWS) {
        saved_table_refresh(s, at / GT2_TABLE_ROWS, at % GT2_TABLE_ROWS);
        return 0;
    }
    s->saved_bytes = saved_size_compute(s);
    return 1;
}

void gt2_saved_recount(gt2_song* s) {
    uint32_t cursor = 0u;
    while (!gt2_saved_recount_step(s, &cursor)) {}
}

void gt2_song_changed(gt2_song* s) {
    s->revision++;
}

static gt2_err saved_check(const gt2_song* s, uint32_t growth) {
    if (s->region_bytes > SONG_REGION_BYTES || s->channels == 0u)
        return GT2_OK;
    uint32_t base = s->pending || s->pending_loose ? s->saved_epoch + s->saved_growth
                                                : gt2_saved_size(s);
    return growth <= GT2_SAVED_MAX && base <= GT2_SAVED_MAX - growth
               ? GT2_OK : GT2_ERR_SONG_FULL;
}

int gt2_saved_ext_check(const gt2_song* s, uint32_t ext_used) {
    if (s->frozen)
        return -(int)GT2_ERR_SAVING;
    if (s->pending)
        return -(int)GT2_ERR_PENDING;
    uint32_t before = gt2_saved_trailer_size(s->ext_used);
    uint32_t after = gt2_saved_trailer_size(ext_used);
    return saved_check(s, after > before ? after - before : 0u) == GT2_OK
               ? 0 : -(int)GT2_ERR_SONG_FULL;
}

void gt2_saved_ext_changed(gt2_song* s, uint32_t old_used) {
    uint32_t before = gt2_saved_trailer_size(old_used);
    uint32_t after = gt2_saved_trailer_size(s->ext_used);
    if ((s->pending || s->pending_loose) && after > before)
        s->saved_growth += after - before;
    s->saved_bytes = s->saved_bytes - before + after;
    gt2_song_changed(s);
}

static uint32_t pattern_growth(const gt2_song* s, uint32_t pattern) {
    uint32_t count = gt2_saved_patterns(s);
    return pattern + 1u > count ? pattern_prefix(s, pattern + 1u) - pattern_prefix(s, count) : 0u;
}

static uint32_t instrument_growth(const gt2_song* s, uint32_t instr) {
    uint32_t count = gt2_saved_instruments(s);
    return instr > count ? (instr - count) * GT2_INSTR_BYTES : 0u;
}

static uint32_t saved_cell_growth(const gt2_song* s, const gt2_edit* e, uint32_t slot) {
    if (slot < GT2_SLOT_ORDER) {
        int active = e->bytes[0] != GT2_REST || e->bytes[1] || e->bytes[2] || e->bytes[3];
        return (active ? pattern_growth(s, slot) : 0u) + instrument_growth(s, e->bytes[1]);
    }
    if (slot != GT2_SLOT_NONE) {
        uint32_t index = slot - GT2_SLOT_ORDER;
        uint32_t d = index / GT2_MAX_CHANNELS, ch = index % GT2_MAX_CHANNELS;
        uint32_t base = s->arena_offset + GT2_SPAN_OFF4(s->order[d][ch]) * GT2_UNIT;
        return e->offset - base < gt2_order_entries(s, d, ch) && e->bytes[0] < GT2_REPEAT
                   ? pattern_growth(s, e->bytes[0]) : 0u;
    }
    if (e->offset < offsetof(gt2_song, ltable)) {
        uint32_t offset = e->offset - (uint32_t)offsetof(gt2_song, instr);
        return offset % GT2_INSTR_BYTES <= GT2_INS_VIBDELAY && e->bytes[0]
                   ? instrument_growth(s, offset / GT2_INSTR_BYTES + 1u) : 0u;
    }
    uint32_t base = e->offset >= offsetof(gt2_song, rtable)
                        ? (uint32_t)offsetof(gt2_song, rtable) : (uint32_t)offsetof(gt2_song, ltable);
    uint32_t offset = e->offset - base, row = offset % GT2_TABLE_ROWS + 1u;
    uint32_t count = gt2_saved_table_rows(s, offset / GT2_TABLE_ROWS);
    return e->bytes[0] && row > count ? 2u * (row - count) : 0u;
}

static uint32_t saved_order_growth(const gt2_song* s, uint32_t d, uint32_t ch,
                                  const uint8_t* data, uint32_t n) {
    uint32_t old = gt2_order_entries(s, d, ch), highest = 0u;
    for (uint32_t i = 0; i < n; i++)
        if (data[i] < GT2_REPEAT && data[i] > highest)
            highest = data[i];
    uint32_t growth = (n > old ? n - old : 0u) + pattern_growth(s, highest);
    uint32_t subtunes = gt2_saved_subtunes(s);
    if (n > 0u && d >= subtunes)
        growth += order_prefix(s, d + 1u) - order_prefix(s, subtunes);
    return growth;
}
