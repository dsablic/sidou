// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The .sng reader and writer (M2c Task 2; docs/reference/song-format.md).
 *
 * The reader takes the file through a callback that hands out bytes in pieces at any
 * offset (a buffer on the host, the store's file reader on the device), so the file is
 * never held twice in RAM. It reads twice: a first pass checks everything (the structure,
 * GoatTracker Stereo's channel count, the trailer and its CRCs, the device's caps, the
 * arena's room) and changes nothing; only then does the second pass load the song. So a
 * refused file leaves the song as it was; only a reader that answers differently the
 * second time (SNG: READ ERROR) leaves it part-loaded.
 *
 * The writer writes what GoatTracker's savesong writes (decision record 0007, rule 1), then
 * the trailer if asked.
 *
 * Every reader of a file is a parser of untrusted input (design doc section 6): no input
 * can make it read or write outside its buffers, and every refusal has a message short
 * enough for a toast. Portable C11, no allocation. */
#ifndef SIDOU_GT2_SNG_H
#define SIDOU_GT2_SNG_H

#include <stdint.h>

#include "gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Up to len bytes of the file from offset into buf; fewer only at the file's end. */
typedef uint32_t (*gt2_read_fn)(void* ctx, uint32_t offset, uint8_t* buf, uint32_t len);
/* len bytes written; 0, or non-zero for a failure (the write stops). */
typedef int (*gt2_write_fn)(void* ctx, const uint8_t* buf, uint32_t len);

/* What the player can play: the device's multispeed cap is 4 (M2c's budget); 0 is no cap
 * (the host). */
typedef struct {
    uint8_t max_multispeed;
} gt2_caps;

enum {
    GT2_TRAILER_NONE,          /* a plain GT2 file */
    GT2_TRAILER_READ,          /* read and kept */
    GT2_TRAILER_DROPPED,       /* damaged, an unknown version or too large: dropped whole */
    GT2_TRAILER_LOCKS_DROPPED  /* the GT2 part's CRC failed: LOCK dropped, the rest kept */
};

typedef struct {
    char msg[32];        /* the refusal, or a warning about the trailer; "" if none */
    uint8_t version;     /* the ident's digit, 2..5 */
    uint8_t channels;    /* 3, or 6 (GoatTracker Stereo) */
    uint8_t trailer;     /* GT2_TRAILER_* */
    uint8_t reserved;
    uint32_t gt2_bytes;  /* the GT2 part's length */
    uint32_t file_bytes; /* the GT2 part and the trailer, as read */
    uint32_t need_bytes; /* the arena bytes the content takes */
} gt2_read_info;

/* 0 and the song loaded, or -1 and info->msg. caps NULL: no cap. */
int gt2_sng_read(gt2_song* s, gt2_read_fn read, void* ctx, const gt2_caps* caps,
                 gt2_read_info* info);

/* 0, or -1 if the writer failed. with_trailer 0 writes the plain GT2 part (for tools that
 * refuse trailing bytes). */
int gt2_sng_write(const gt2_song* s, int with_trailer, gt2_write_fn write, void* ctx);

/* GoatTracker Stereo's determinechannels over the file (0007): 6 if every trial read of
 * 6 x subtunes orderlists, from the subtune count at offset 100, passes; else 3. */
uint32_t gt2_sng_channels(gt2_read_fn read, void* ctx);

#ifdef __cplusplus
}
#endif

#endif
