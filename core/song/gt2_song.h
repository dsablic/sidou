// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song model (M2c Task 2): a GoatTracker 2 song as SIDOU holds it, in one content region
 * the caller places (decision record 0007, docs/reference/song-format.md).
 *
 * The region is one object: the device's is SONG_REGION_BYTES (64 KiB, the block M3's 6502
 * memory will overlay); the host may give more (up to GT2_REGION_MAX). It starts with a
 * gt2_song (below): the counts, the strings, the instruments and the four tables at
 * GoatTracker's full size, an index of the orderlists and patterns, and the extension area
 * (the trailer's chunks, song_ext.h). Then an allocation bitmap, then the content arena:
 * the orderlists and patterns, each found through its index entry (a gt2_span), so
 * patterns of any length up to 128 rows pack tight.
 *
 * Edits come in two steps, so the player never reads half an edit (M2c Task 6):
 * - a cell write: one store of at most 4 bytes in place (a pattern row, an instrument byte,
 *   a table row, an orderlist entry or restart position);
 * - a rebuild: anything that moves rows or changes a length (a push/pull, a resize, entries
 *   inserted or deleted, a pattern added or removed) builds the new pattern or orderlist in
 *   free arena space, touching no byte in use, and yields a swap: the index entry's new
 *   value, one aligned 32-bit store. The old space stays allocated until the swap is known
 *   applied (gt2_edit_release).
 * Each primitive returns its edit (a gt2_edit) unapplied; gt2_edit_apply makes the store and
 * gt2_edit_release frees what a swap replaced. gt2_edit_commit does both, as the host does.
 *
 * The deferred contract (M2c Task 6 queues the edits, the audio interrupt applies them in
 * order, the main loop releases each after its acknowledgement). An edit is pending from
 * the primitive's return until its gt2_edit_release (or gt2_edit_cancel). Every primitive
 * reads the index as the player does, so the model refuses, with GT2_ERR_PENDING and no
 * change, any edit that a pending one would make wrong:
 * - a pattern has at most one pending swap, and no swap while any of its cell writes is
 *   pending (a rebuild would copy rows the queue has not written yet); cell writes to a
 *   pattern with no pending swap may queue up (up to 127), since each holds its own row;
 * - an orderlist has at most one pending edit of either kind (a cell write is checked
 *   against the entries around it);
 * - an orderlist edit naming a pattern waits for that pattern's pending swap, and a
 *   pattern's removal for every pending orderlist edit (each checks the other);
 * - gt2_compact_step does nothing (-1) while any of those is pending.
 * - a rebuild that fits the song at rest (every pending edit released) but finds no room
 *   now, the old copies still held, waits too; GT2_ERR_FULL only if it would not fit at
 *   rest either.
 * The instruments and tables take cell writes only and hold no slot. They can still be
 * refused by the serialized-size cap or a save freeze. Reload and clear wait for every
 * pending edit, including these loose writes.
 *
 * The player (Task 6) re-reads the index entry each tick and never holds an arena pointer
 * from one tick to the next: an edit's release may free, and the next edit reuse, any
 * byte the index no longer names. The caller applies the
 * edits in the order it made them and releases each only once applied; the reader and
 * gt2_song_clear assume nothing is pending.
 *
 * The arena keeps SONG_ARENA_SPARE free when every edit is released. The top
 * GT2_SCRATCH_UNITS of it (one largest pattern) are compaction's scratch, which first fit
 * never allocates from; the rest of the spare (524 bytes) holds a rebuild's new copy (at
 * most 516 bytes) while it waits for its swap. A song whose content would leave less is
 * refused. When no hole is large enough, gt2_compact_step moves one block (a rebuild of
 * its own) at a time; it always ends with the free space in one hole, within two steps a
 * block.
 *
 * Portable C11, no allocation; the main loop's code, not the audio path's. */
#ifndef SIDOU_GT2_SONG_H
#define SIDOU_GT2_SONG_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* GoatTracker 2's limits (gcommon.h). */
#define GT2_MAX_SUBTUNES 32u
#define GT2_MONO_CHANNELS 3u
#define GT2_MAX_CHANNELS 6u    /* GoatTracker Stereo */
#define GT2_MAX_ORDER 254u     /* orderlist entries, the $FF not counted */
#define GT2_MAX_INSTR 63u
#define GT2_INSTR_BYTES 25u
#define GT2_INSTR_NAME 16u
#define GT2_TABLES 4u
#define GT2_TABLE_ROWS 255u
#define GT2_MAX_PATTERNS 208u
#define GT2_MAX_ROWS 128u      /* the end row not counted */
#define GT2_ROW_BYTES 4u
#define GT2_STR 32u
#define GT2_DEFAULT_ROWS 64u   /* a pattern the file does not hold (GoatTracker's clearpattern) */

enum { GT2_WTBL, GT2_PTBL, GT2_FTBL, GT2_STBL };

/* An instrument's bytes, in the file's order. */
enum {
    GT2_INS_AD, GT2_INS_SR, GT2_INS_WAVE, GT2_INS_PULSE, GT2_INS_FILTER, GT2_INS_VIBPARAM,
    GT2_INS_VIBDELAY, GT2_INS_GATETIMER, GT2_INS_FIRSTWAVE, GT2_INS_NAME
};

/* Orderlist entries and pattern notes (readme 6.1). */
#define GT2_REPEAT 0xd0u
#define GT2_TRANSDOWN 0xe0u
#define GT2_TRANSUP 0xf0u
#define GT2_LOOPSONG 0xffu
#define GT2_FIRSTNOTE 0x60u
#define GT2_LASTNOTE 0xbcu
#define GT2_REST 0xbdu
#define GT2_KEYOFF 0xbeu
#define GT2_KEYON 0xbfu
#define GT2_ENDPATT 0xffu

/* The content region. */
#define SONG_REGION_BYTES 65536u
#define GT2_SAVED_MAX 65536u /* STORE_FILE_MAX; the device's trailer-inclusive save limit */
#define GT2_REGION_MAX 262144u   /* spans count 4-byte units in 16 bits */
#define SONG_EXT_BYTES 4096u
/* The largest pattern: 128 rows and the end row, 4 bytes each. */
#define GT2_PATTERN_MAX_BYTES ((GT2_MAX_ROWS + 1u) * GT2_ROW_BYTES)
/* Free at rest: the largest pattern (516 bytes) twice over, plus slack: once as compaction's
 * scratch, once for a rebuild's new copy. */
#define SONG_ARENA_SPARE 1040u
#define GT2_UNIT 4u
/* Compaction's scratch at the arena's top: the largest pattern, in units. */
#define GT2_SCRATCH_UNITS (GT2_PATTERN_MAX_BYTES / GT2_UNIT)

/* An index entry: the arena offset in 4-byte units (bits 0-15) and the length (bits 16-31:
 * an orderlist's bytes, its entries, $FF and restart position; a pattern's rows, the end
 * row included). 0: none. One 32-bit word, so a swap is one store. */
typedef uint32_t gt2_span;
#define GT2_SPAN(off4, len) ((uint32_t)(off4) | ((uint32_t)(len) << 16))
#define GT2_SPAN_OFF4(s) ((s) & 0xffffu)
#define GT2_SPAN_LEN(s) ((s) >> 16)

/* The region's start. Every field is the song's state; nothing points outside the region. */
typedef struct {
    uint32_t region_bytes;
    uint32_t arena_offset;  /* bytes from the region's start */
    uint32_t arena_units;   /* the arena's size in 4-byte units */
    uint32_t used_units;    /* allocated, unreleased swaps included */
    uint32_t bitmap_offset; /* one bit per arena unit, 1 in use */
    uint8_t subtunes;       /* 1..32 */
    uint8_t channels;       /* 3, or 6 for a GoatTracker Stereo song */
    uint8_t version;        /* the ident's digit the song was read as, 2..5 (written as 5) */
    uint8_t reserved;
    char name[GT2_STR], author[GT2_STR], copyright[GT2_STR];
    uint8_t instr[GT2_MAX_INSTR][GT2_INSTR_BYTES]; /* instrument 1 is instr[0] */
    uint8_t ltable[GT2_TABLES][GT2_TABLE_ROWS];
    uint8_t rtable[GT2_TABLES][GT2_TABLE_ROWS];
    gt2_span order[GT2_MAX_SUBTUNES][GT2_MAX_CHANNELS];
    gt2_span pattern[GT2_MAX_PATTERNS]; /* 0: not held, an empty 64-row pattern (never named) */
    /* Pending edits (the deferred contract above), the main loop's bookkeeping: */
    uint8_t pend_pattern[GT2_MAX_PATTERNS]; /* bit 7 a swap, bits 0-6 cell writes */
    uint32_t pend_order[(GT2_MAX_SUBTUNES * GT2_MAX_CHANNELS + 31u) / 32u]; /* a bit each */
    uint16_t pending;                       /* all pending edits that hold a slot */
    uint16_t pending_orders;                /* of which on orderlists */
    uint32_t pending_loose;                 /* pending instrument and table cell writes */
    uint32_t pending_old_units; /* old copies pending swaps will release: used_units less
                                 * this is the song's size at rest */
    uint32_t ext_used;                  /* bytes of ext in use */
    uint8_t ext[SONG_EXT_BYTES];        /* the trailer's chunks (song_ext.h) */
    /* Save accounting: the interrupt changes only cell deltas and per-pattern row counts.
     * Rebuild instrument counts have a separate main-loop writer. Their uint16_t sums are
     * modulo 65536; at most 208 * 128 rows can contribute to any instrument. IRQ-owned
     * fields are volatile; applied acknowledgements delimit stable main-loop snapshots. */
    uint32_t revision;
    uint32_t saved_bytes, saved_epoch, saved_growth;
    uint32_t saved_row_work; /* Host instrumentation: accounting row inspections; zero on target. */
    uint16_t saved_pattern_groups[GT2_MAX_PATTERNS / 16u];
    volatile uint8_t saved_pattern_nonempty[GT2_MAX_PATTERNS];
    uint16_t saved_instr_base[GT2_MAX_INSTR + 1u];
    volatile uint16_t saved_instr_delta[GT2_MAX_INSTR + 1u];
    uint32_t saved_pattern_bits[7], saved_instr_defs[2];
    uint32_t saved_table_bits[GT2_TABLES][8];
    uint32_t saved_subtune_bits;
    uint16_t saved_order_tree[GT2_MAX_SUBTUNES + 1u];
    uint16_t saved_pattern_refs[GT2_MAX_PATTERNS];
    uint8_t frozen; /* Refuses new edits; edits accepted before the save still apply/release. */
    uint8_t saved_trim_pending; /* Removed trailing pattern + 1; its future hole is not reserved. */
} gt2_song;

/* Errors. Every primitive refuses what would break GoatTracker's rules (readme 3.1) or the
 * arena's, and changes nothing then. */
typedef enum {
    GT2_OK = 0,
    GT2_ERR_RANGE,      /* an index or value out of range */
    GT2_ERR_ORDER_RULE, /* a transpose after a repeat, or the last entry not a pattern */
    GT2_ERR_ORDER_FULL, /* 254 entries */
    GT2_ERR_ROWS,       /* above 128 rows, or below 1 */
    GT2_ERR_PATTERNS,   /* 208 patterns */
    GT2_ERR_IN_USE,     /* a pattern an orderlist names */
    GT2_ERR_FULL,       /* the content would leave less than SONG_ARENA_SPARE free once
                         * every pending edit is released */
    GT2_ERR_FRAGMENTED, /* no hole large enough: gt2_compact_step, then again */
    GT2_ERR_REGION,     /* the region is too small or too large */
    GT2_ERR_PENDING,    /* a pending edit would make this one wrong, or holds the room it
                         * needs: again after the next release */
    GT2_ERR_SONG_FULL,  /* the canonical .sng, including its trailer, exceeds GT2_SAVED_MAX */
    GT2_ERR_SAVING      /* edits are frozen while a save streams */
} gt2_err;

/* Formats an empty song into region (bytes long, 4-byte aligned): one subtune whose three
 * orderlists play patterns 0, 1, 2 (64 empty rows each), as GoatTracker's clearsong leaves
 * it. NULL if the region is outside [gt2_region_min(), GT2_REGION_MAX] or the patterns do
 * not fit. */
gt2_song* gt2_song_init(void* region, uint32_t bytes);
/* Empties an existing song in place (its region kept), as gt2_song_init leaves it. 0, or
 * -1 (nothing done) while any edit is pending. */
int gt2_song_clear(gt2_song* s);
/* Empties it entirely: no subtune, no orderlist, no pattern (the reader's start). */
void gt2_song_wipe(gt2_song* s);
uint32_t gt2_region_min(void);

/* The arena's capacity for content, SONG_ARENA_SPARE taken off, and its use. */
uint32_t gt2_arena_capacity(const gt2_song* s);
uint32_t gt2_arena_used(const gt2_song* s);
/* The largest free hole first fit can use (the scratch not counted), bytes. */
uint32_t gt2_arena_largest_hole(const gt2_song* s);

/* The content's bytes in the arena. */
static inline const uint8_t* gt2_arena(const gt2_song* s) {
    return (const uint8_t*)s + s->arena_offset;
}
/* An orderlist's bytes (entries, $FF, restart) and a pattern's rows; NULL if none. */
const uint8_t* gt2_order_data(const gt2_song* s, uint32_t subtune, uint32_t channel);
uint32_t gt2_order_entries(const gt2_song* s, uint32_t subtune, uint32_t channel);
const uint8_t* gt2_pattern_data(const gt2_song* s, uint32_t pattern);
/* Rows, the end row included; GT2_DEFAULT_ROWS + 1 for a pattern not held. */
uint32_t gt2_pattern_rows(const gt2_song* s, uint32_t pattern);

/* 1 if the entries (n of them, no $FF) follow readme 3.1: each step an optional transpose,
 * an optional repeat, then a pattern; so a transpose never follows a repeat and the last
 * entry is a pattern. An empty orderlist is valid (GoatTracker's subtunes past the first). */
int gt2_order_valid(const uint8_t* entries, uint32_t n);

/* An edit, as a primitive returns it. */
enum { GT2_EDIT_NONE, GT2_EDIT_CELL, GT2_EDIT_SWAP };
#define GT2_SLOT_NONE 0xffffu /* an instrument or table cell */
#define GT2_SLOT_ORDER 0x100u /* slots from here: orderlist subtune * 6 + channel */
typedef struct {
    uint8_t kind;
    uint8_t n;          /* CELL: bytes, 1..4 */
    uint8_t bytes[4];   /* CELL */
    uint16_t slot;      /* the pattern (0-207) or orderlist it holds pending, or GT2_SLOT_NONE */
    uint16_t reserved;
    uint32_t offset;    /* bytes from the region's start: the cell, or the index entry */
    gt2_span span;      /* SWAP: the new index entry */
    gt2_span old_span;  /* SWAP: what the swap replaces, released by gt2_edit_release */
} gt2_edit;

/* The cell's bytes, or the index entry (a volatile aligned 32-bit store after a release
 * fence). A row cell also adjusts its nonempty count and at most two instrument bins from
 * the actual old row, so overlapping queued writes need no shadow copy. No row scan or
 * variable-length work; the audio interrupt's. */
void gt2_edit_apply(gt2_song* s, const gt2_edit* e);
/* After the edit is applied: frees a swap's old span, and ends the edit's pending. */
void gt2_edit_release(gt2_song* s, const gt2_edit* e);
/* Release of a checked shift restore: the same packed locks supplied at preparation,
 * still alive and unchanged. Restores them before accounting/completion, even if frozen. */
void gt2_edit_release_with_locks(gt2_song* s, const gt2_edit* e, const uint8_t* locks, uint32_t n);
/* An edit never applied (taken out of the queue): frees a swap's new span, and ends its
 * pending. */
void gt2_edit_cancel(gt2_song* s, const gt2_edit* e);
/* Apply, then release. */
void gt2_edit_commit(gt2_song* s, const gt2_edit* e);

/* Cell writes. */
gt2_err gt2_set_row(gt2_song* s, uint32_t pattern, uint32_t row, const uint8_t cell[4],
                    gt2_edit* e);
gt2_err gt2_set_instr(gt2_song* s, uint32_t instr, uint32_t field, uint8_t value,
                      gt2_edit* e); /* instr 1..63, field 0..24 */
/* side 0 the left byte, 1 the right (they are 255 bytes apart: one cell each). */
gt2_err gt2_set_table(gt2_song* s, uint32_t table, uint32_t row, uint32_t side,
                      uint8_t value, gt2_edit* e);
gt2_err gt2_set_order(gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                      uint8_t value, gt2_edit* e); /* pos == entries: the restart position */

/* Rebuilds. */
gt2_err gt2_pattern_resize(gt2_song* s, uint32_t pattern, uint32_t rows, gt2_edit* e);
gt2_err gt2_pattern_insert_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e);
gt2_err gt2_pattern_delete_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e);
/* Tracker INS/DEL on a body row: keep the length and end row. Push inserts an empty row
 * and drops the last body row; pull drops row and appends an empty body row. Locks follow
 * the retained rows; the dropped row's locks go. row must be below the end row. */
gt2_err gt2_pattern_push_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e);
gt2_err gt2_pattern_pull_row(gt2_song* s, uint32_t pattern, uint32_t row, gt2_edit* e);
/* Fixed-length shift plus the inverse's lost row and packed row locks. insert selects
 * push (restored row at cursor) or pull (restored row at tail). Complete preflight occurs
 * before allocation; the private copy must not be patched afterwards. Keep locks unchanged
 * through gt2_edit_release_with_locks when n is nonzero. */
gt2_err gt2_pattern_shift_restore(gt2_song* s, uint32_t pattern, uint32_t row, int insert,
                                 const uint8_t cell[4], const uint8_t* locks, uint32_t n,
                                 gt2_edit* e);
/* The lowest pattern number not held and not named, made with rows empty rows. */
gt2_err gt2_pattern_add(gt2_song* s, uint32_t rows, uint32_t* pattern, gt2_edit* e);
/* Only a pattern no orderlist names; it becomes "not held". */
gt2_err gt2_pattern_remove(gt2_song* s, uint32_t pattern, gt2_edit* e);
gt2_err gt2_order_insert(gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                         uint8_t value, gt2_edit* e);
gt2_err gt2_order_delete(gt2_song* s, uint32_t subtune, uint32_t channel, uint32_t pos,
                         gt2_edit* e);
/* Replace all entries and restart in one swap, with the same rules and deferred contract
 * as the other order edits. entries may be NULL only when n == 0 (restart then 0). */
gt2_err gt2_order_replace(gt2_song* s, uint32_t subtune, uint32_t channel,
                          const uint8_t* entries, uint32_t n, uint8_t restart, gt2_edit* e);

/* One compaction move (a rebuild): returns 1 and the move's swap if it moved a block, 0 if
 * the free space is already one hole at the arena's end, -1 (nothing done) while an edit is
 * pending. Each move slides the lowest block above the lowest gap down into it: straight
 * in if it fits, else through the scratch (one move out, the next one down). So, with no
 * rebuild between steps, the lowest gap's start only rises, and at most 2 x blocks moves
 * leave one hole. Edits between steps can undo some of that, but each adds at most one
 * hole (its old copy's place) and never a block (a rebuild's new copy replaces its old
 * one; a removal takes one away), so once they stop the bound holds again from there. */
int gt2_compact_step(gt2_song* s, gt2_edit* e);

/* What savesong would write (0007, rule 1): the subtune, instrument and pattern counts and
 * each table's rows. */
uint32_t gt2_saved_subtunes(const gt2_song* s);
uint32_t gt2_saved_instruments(const gt2_song* s);
uint32_t gt2_saved_patterns(const gt2_song* s);
uint32_t gt2_saved_table_rows(const gt2_song* s, uint32_t table);
/* Exact canonical length with the trailer, after all edits have been released. Pending
 * edits reserve growth conservatively until the whole batch ends; a cancellation gives
 * back its reservation. Applying edits still performs only the original O(1) stores. */
uint32_t gt2_saved_size(const gt2_song* s);
uint32_t gt2_saved_trailer_size(uint32_t ext_used);
/* After direct model writes (loaders/templates), with no pending edits. step: cursor starts
 * at zero; 0 means more work, 1 means done. Each call handles at most one bounded object. */
void gt2_saved_recount(gt2_song* s);
int gt2_saved_recount_step(gt2_song* s, uint32_t* cursor);
/* Extension mutation preflight, and notification after a successful change. Indexed edits
 * must settle first, so their deferred lock trimming cannot race extension replacement. */
int gt2_saved_ext_check(const gt2_song* s, uint32_t ext_used);
void gt2_saved_ext_changed(gt2_song* s, uint32_t old_used);
void gt2_song_changed(gt2_song* s);

/* Arena allocation (the reader's and the primitives'): first fit below the scratch, 4-byte
 * units. alloc returns 0 and the unit offset, or -1 if no hole fits. */
int gt2_arena_alloc(gt2_song* s, uint32_t bytes, uint32_t* off4);
void gt2_arena_release(gt2_song* s, uint32_t off4, uint32_t bytes);
/* The arena bytes an index entry holds. */
uint32_t gt2_order_span_bytes(gt2_span span);
uint32_t gt2_pattern_span_bytes(gt2_span span);

#ifdef __cplusplus
}
#endif

#endif
