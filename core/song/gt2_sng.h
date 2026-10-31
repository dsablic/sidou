// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_GT2_SNG_H
#define SIDOU_GT2_SNG_H

#include <stdint.h>

#include "gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef uint32_t (*gt2_read_fn)(void* ctx, uint32_t offset, uint8_t* buf, uint32_t len);
typedef int (*gt2_write_fn)(void* ctx, const uint8_t* buf, uint32_t len);

typedef struct {
    uint8_t max_multispeed;
} gt2_caps;

enum {
    GT2_TRAILER_NONE,
    GT2_TRAILER_READ,
    GT2_TRAILER_DROPPED,
    GT2_TRAILER_LOCKS_DROPPED
};

typedef struct {
    char msg[32];
    uint8_t version;
    uint8_t channels;
    uint8_t trailer;
    uint8_t reserved;
    uint32_t gt2_bytes;
    uint32_t file_bytes;
    uint32_t need_bytes;
    uint32_t saved_bytes;
} gt2_read_info;

#define GT2_READ_SONG_FULL (-2)
#define GT2_READ_SAVING (-3)

int gt2_sng_read(gt2_song* s, gt2_read_fn read, void* ctx, const gt2_caps* caps,
                 gt2_read_info* info);

int gt2_sng_write(const gt2_song* s, int with_trailer, gt2_write_fn write, void* ctx);

int gt2_sng_check_plain(const gt2_song* s, gt2_read_fn read, void* ctx, char msg[32]);

uint32_t gt2_sng_channels(gt2_read_fn read, void* ctx);

#ifdef __cplusplus
}
#endif

#endif
