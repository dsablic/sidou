// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_GT2_INS_H
#define SIDOU_GT2_INS_H

#include "gt2_sng.h"

#ifdef __cplusplus
extern "C" {
#endif

/* GTI5 (GoatTracker readme 6.2). Keep the source stopped, edits settled and frozen
 * throughout begin, CRC prepass and replay. No instrument-sized buffer is needed.
 * A slice starts at its instrument pointer; dependencies below that pointer or
 * in a table with no instrument pointer cannot be represented and are refused. */
typedef struct {
    const gt2_song* song;
    uint32_t length;
    uint8_t instrument;
    uint8_t start[GT2_TABLES];
    uint8_t rows[GT2_TABLES];
} gt2_ins_export;

gt2_err gt2_ins_export_begin(gt2_ins_export* out, const gt2_song* song, uint32_t instrument);
/* gt2_read_fn-compatible indexed reader; short only at EOF. */
uint32_t gt2_ins_export_read(void* ctx, uint32_t offset, uint8_t* buf, uint32_t length);

typedef struct {
    gt2_song* song;
    gt2_read_fn read;
    void* ctx;
    uint32_t saved, index;
    uint16_t offset[GT2_TABLES], pattern, row, end;
    uint8_t instr[GT2_INSTR_BYTES];
    uint8_t rows[GT2_TABLES], dest[GT2_TABLES];
    uint8_t instrument, phase, table;
    gt2_err error;
} gt2_ins_import_state;

/* The caller owns a stopped model with no pending edits and current save accounting.
 * Preflight reads the entire immutable source before the first write. Replay must
 * return the same bytes without I/O failures (e.g. a committed, pinned store file),
 * just as gt2_sng_read requires. No other reader/writer may observe the apply pass.
 * Allocation uses trailing rows only, preserving all old data and referenced zero
 * rows, including references from other instruments, patterns and table commands.
 * Invalid content: RANGE; no trailing room: FULL; serialized cap: SONG_FULL.
 * Ordinary import refuses a save freeze. import_owned is only for the platform
 * which acquired that freeze itself, never to bypass somebody else's save. */
/* begin performs only header preflight. Keep the model stopped/frozen and the source
 * pinned until step completes. owned means the caller acquired the existing freeze.
 * Each step examines at most eight reference roots (255 graph hops each), eight
 * pattern rows/empty patterns, or eight table rows (at most 24 source bytes).
 * step returns 0 pending, 1 done, -1 failed (state.error); failure before replay
 * changes no song bytes. After replay starts the source must not fail or change.
 * The state may reuse the caller's gt2_stream workspace. */
gt2_err gt2_ins_import_begin(gt2_ins_import_state* state, gt2_song* song, uint32_t instrument,
                             gt2_read_fn read, void* ctx, uint32_t length, int owned);
int gt2_ins_import_step(gt2_ins_import_state* state);
gt2_err gt2_ins_import(gt2_song* song, uint32_t instrument, gt2_read_fn read,
                       void* ctx, uint32_t length);
gt2_err gt2_ins_import_owned(gt2_song* song, uint32_t instrument, gt2_read_fn read,
                             void* ctx, uint32_t length);

#ifdef __cplusplus
}
#endif
#endif
