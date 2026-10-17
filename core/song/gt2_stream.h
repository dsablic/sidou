// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_GT2_STREAM_H
#define SIDOU_GT2_STREAM_H

#include "gt2_sng.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The song and source file stay frozen until done.
 * Readers are destructive: the caller restores its previous file on failure.
 * Each step handles at most one pattern (516 bytes), otherwise about 256 bytes.
 * The reader's step returns 0 pending, 1 done, -1 error.
 * Do not resume a workspace after starting another operation in it. */
typedef struct {
    gt2_song* song;
    const gt2_song* source;
    gt2_read_fn read;
    void* ctx;
    gt2_read_info info, best;
    uint32_t state, pos, index, sub, channel, count, side;
    uint32_t crc, gt_crc, trailer_at, trailer_len, chunk_at, chunk_len;
    uint32_t chunk_tag, kept, copied, wipe_at, rank, best_rank;
    uint32_t top_named, top_instr, top_pattern, instruments, patterns;
    uint32_t out_len, out_at, pad, table_rows[4];
    uint8_t named[GT2_MAX_PATTERNS];
    uint8_t buf[GT2_PATTERN_MAX_BYTES];
    uint8_t guess, pass, loading, canonical, best_canonical, max_multispeed;
    uint8_t trailer_channels, has_opts, multispeed, raw_multispeed;
    uint8_t with_trailer, saved_subtunes, saved_instr, saved_patterns;
} gt2_stream;

void gt2_stream_read_begin(gt2_stream* w, gt2_song* song, gt2_read_fn read,
                           void* ctx, const gt2_caps* caps);
int gt2_stream_read_step(gt2_stream* w);
/* The song's save accounting must be current (edits released, or recount completed).
 * step returns 1..min(capacity,256) bytes, 0 complete, -1 error. capacity must be positive.
 * Keep the returned bytes until the sink accepts them; only then call step again. */
void gt2_stream_write_begin(gt2_stream* w, const gt2_song* song, int with_trailer);
int gt2_stream_write_step(gt2_stream* w, uint8_t* buf, uint32_t capacity);
const gt2_read_info* gt2_stream_info(const gt2_stream* w);

#ifdef __cplusplus
}
#endif
#endif
