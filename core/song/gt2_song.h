// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_GT2_SONG_H
#define SIDOU_GT2_SONG_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define GT2_MAX_SUBTUNES 32u
#define GT2_MONO_CHANNELS 3u
#define GT2_MAX_CHANNELS 6u
#define GT2_MAX_ORDER 254u
#define GT2_MAX_INSTR 63u
#define GT2_INSTR_BYTES 25u
#define GT2_INSTR_NAME 16u
#define GT2_TABLES 4u
#define GT2_TABLE_ROWS 255u
#define GT2_MAX_PATTERNS 208u
#define GT2_MAX_ROWS 128u
#define GT2_ROW_BYTES 4u
#define GT2_STR 32u
#define GT2_DEFAULT_ROWS 64u

enum { GT2_WTBL, GT2_PTBL, GT2_FTBL, GT2_STBL };

enum {
    GT2_INS_AD, GT2_INS_SR, GT2_INS_WAVE, GT2_INS_PULSE, GT2_INS_FILTER, GT2_INS_VIBPARAM,
    GT2_INS_VIBDELAY, GT2_INS_GATETIMER, GT2_INS_FIRSTWAVE, GT2_INS_NAME
};

#define GT2_REPEAT 0xd0u
#define GT2_TRANSDOWN 0xe0u
#define GT2_TRANSUP 0xf0u
#define GT2_LOOPSONG 0xffu
#define GT2_FIRSTNOTE 0x60u
#define GT2_LASTNOTE 0xbcu
#define GT2_NOTE_MAX (GT2_LASTNOTE - GT2_FIRSTNOTE)
#define GT2_REST 0xbdu
#define GT2_KEYOFF 0xbeu
#define GT2_KEYON 0xbfu
#define GT2_ENDPATT 0xffu

#define SONG_REGION_BYTES 65536u
#define GT2_SAVED_MAX 65536u
#define GT2_REGION_MAX 262144u
#define SONG_EXT_BYTES 4096u
#define GT2_PATTERN_MAX_BYTES ((GT2_MAX_ROWS + 1u) * GT2_ROW_BYTES)
#define SONG_ARENA_SPARE 1040u
#define GT2_UNIT 4u
#define GT2_SCRATCH_UNITS (GT2_PATTERN_MAX_BYTES / GT2_UNIT)

typedef uint32_t gt2_span;
#define GT2_SPAN(off4, len) ((uint32_t)(off4) | ((uint32_t)(len) << 16))
#define GT2_SPAN_OFF4(s) ((s) & 0xffffu)
#define GT2_SPAN_LEN(s) ((s) >> 16)

typedef struct {
    uint32_t region_bytes;
    uint32_t arena_offset;
    uint32_t arena_units;
    uint32_t used_units;
    uint32_t bitmap_offset;
    uint8_t subtunes;
    uint8_t channels;
    uint8_t version;
    uint8_t reserved;
    char name[GT2_STR], author[GT2_STR], copyright[GT2_STR];
    uint8_t instr[GT2_MAX_INSTR][GT2_INSTR_BYTES];
    uint8_t ltable[GT2_TABLES][GT2_TABLE_ROWS];
    uint8_t rtable[GT2_TABLES][GT2_TABLE_ROWS];
    gt2_span order[GT2_MAX_SUBTUNES][GT2_MAX_CHANNELS];
    gt2_span pattern[GT2_MAX_PATTERNS];
    uint8_t pend_pattern[GT2_MAX_PATTERNS];
    uint32_t pend_order[(GT2_MAX_SUBTUNES * GT2_MAX_CHANNELS + 31u) / 32u];
    uint16_t pending;
    uint16_t pending_orders;
    uint32_t pending_loose;
    uint32_t pending_old_units;
    uint32_t ext_used;
    uint8_t ext[SONG_EXT_BYTES];
    uint32_t revision;
    uint32_t saved_bytes, saved_epoch, saved_growth;
    uint32_t saved_row_work;
    uint16_t saved_pattern_groups[GT2_MAX_PATTERNS / 16u];
    volatile uint8_t saved_pattern_nonempty[GT2_MAX_PATTERNS];
    uint16_t saved_instr_base[GT2_MAX_INSTR + 1u];
    volatile uint16_t saved_instr_delta[GT2_MAX_INSTR + 1u];
    uint32_t saved_pattern_bits[7], saved_instr_defs[2];
    uint32_t saved_table_bits[GT2_TABLES][8];
    uint32_t saved_subtune_bits;
    uint16_t saved_order_tree[GT2_MAX_SUBTUNES + 1u];
    uint16_t saved_pattern_refs[GT2_MAX_PATTERNS];
    uint8_t frozen;
    uint8_t saved_trim_pending;
} gt2_song;

typedef enum {
    GT2_OK = 0,
    GT2_ERR_RANGE,
    GT2_ERR_ORDER_RULE,
    GT2_ERR_ORDER_FULL,
    GT2_ERR_ROWS,
    GT2_ERR_PATTERNS,
    GT2_ERR_IN_USE,
    GT2_ERR_FULL,
    GT2_ERR_FRAGMENTED,
    GT2_ERR_REGION,
    GT2_ERR_PENDING,
    GT2_ERR_SONG_FULL,
    GT2_ERR_SAVING
} gt2_err;

gt2_song* gt2_song_init(void* region, uint32_t bytes);
gt2_song* gt2_song_init_wiped(void* region, uint32_t bytes);
int gt2_song_clear(gt2_song* s);
void gt2_song_wipe(gt2_song* s);
uint32_t gt2_region_min(void);

uint32_t gt2_arena_capacity(const gt2_song* s);
uint32_t gt2_arena_used(const gt2_song* s);
uint32_t gt2_arena_largest_hole(const gt2_song* s);

static inline const uint8_t* gt2_arena(const gt2_song* s) {
    return (const uint8_t*)s + s->arena_offset;
}
const uint8_t* gt2_order_data(const gt2_song* s, uint32_t subtune, uint32_t channel);
uint32_t gt2_order_entries(const gt2_song* s, uint32_t subtune, uint32_t channel);
const uint8_t* gt2_pattern_data(const gt2_song* s, uint32_t pattern);
uint32_t gt2_pattern_rows(const gt2_song* s, uint32_t pattern);

int gt2_order_valid(const uint8_t* entries, uint32_t n);

enum { GT2_EDIT_NONE, GT2_EDIT_CELL, GT2_EDIT_SWAP };
#define GT2_SLOT_NONE 0xffffu
#define GT2_SLOT_ORDER 0x100u
typedef struct {
    uint8_t kind;
    uint8_t n;
    uint8_t bytes[4];
    uint16_t slot;
    uint16_t reserved;
    uint32_t offset;
    gt2_span span;
    gt2_span old_span;
} gt2_edit;

void gt2_edit_apply(gt2_song* s, const gt2_edit* e);
void gt2_edit_release(gt2_song* s, const gt2_edit* e);
void gt2_edit_release_with_locks(gt2_song* s, const gt2_edit* e, const uint8_t* locks, uint32_t n);
void gt2_edit_cancel(gt2_song* s, const gt2_edit* e);
void gt2_edit_commit(gt2_song* s, const gt2_edit* e);

gt2_err gt2_set_row(gt2_song* s, uint32_t pattern, uint32_t row, const uint8_t cell[4],
                    gt2_edit* e);
gt2_err gt2_set_instr(gt2_song* s, uint32_t instr, uint32_t field, uint8_t value,
                      gt2_edit* e);
gt2_err gt2_set_table(gt2_song* s, uint32_t table, uint32_t row, uint32_t side,
                      uint8_t value, gt2_edit* e);
gt2_err gt2_set_order(gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                      uint8_t value, gt2_edit* e);

gt2_err gt2_pattern_resize(gt2_song* s, uint32_t pattern, uint32_t rows, gt2_edit* e);
gt2_err gt2_pattern_insert_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e);
gt2_err gt2_pattern_delete_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e);
gt2_err gt2_pattern_push_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e);
gt2_err gt2_pattern_pull_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e);
gt2_err gt2_pattern_shift_restore(gt2_song* s, uint32_t pattern, uint32_t row, int insert,
                                 const uint8_t cell[4], const uint8_t* locks, uint32_t n,
                                 gt2_edit* e);
gt2_err gt2_pattern_add(gt2_song* s, uint32_t rows, uint32_t* pattern, gt2_edit* e);
gt2_err gt2_pattern_add_at(gt2_song* s, uint32_t pattern, uint32_t rows, gt2_edit* e);
gt2_err gt2_pattern_remove(gt2_song* s, uint32_t pattern, gt2_edit* e);
gt2_err gt2_order_insert(gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                         uint8_t value, gt2_edit* e);
gt2_err gt2_order_delete(gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                         gt2_edit* e);
gt2_err gt2_order_replace(gt2_song* s, uint32_t subtune, uint32_t channel,
                          const uint8_t* entries, uint32_t n, uint8_t restart, gt2_edit* e);
gt2_err gt2_order_pattern_check(const gt2_song* s, uint32_t subtune, uint32_t channel,
                                uint32_t pos, uint32_t pattern, int insert);

int gt2_compact_step(gt2_song* s, gt2_edit* e);

uint32_t gt2_saved_subtunes(const gt2_song* s);
uint32_t gt2_saved_instruments(const gt2_song* s);
uint32_t gt2_saved_patterns(const gt2_song* s);
uint32_t gt2_saved_table_rows(const gt2_song* s, uint32_t table);
uint32_t gt2_saved_size(const gt2_song* s);
uint32_t gt2_saved_trailer_size(uint32_t ext_used);
void gt2_saved_recount(gt2_song* s);
int gt2_saved_recount_step(gt2_song* s, uint32_t* cursor);
int gt2_saved_ext_check(const gt2_song* s, uint32_t ext_used);
void gt2_saved_ext_changed(gt2_song* s, uint32_t old_used);
void gt2_song_changed(gt2_song* s);

int gt2_arena_alloc(gt2_song* s, uint32_t bytes, uint32_t* off4);
void gt2_arena_release(gt2_song* s, uint32_t off4, uint32_t bytes);
uint32_t gt2_order_span_bytes(gt2_span span);
uint32_t gt2_pattern_span_bytes(gt2_span span);

#ifdef __cplusplus
}
#endif

#endif
