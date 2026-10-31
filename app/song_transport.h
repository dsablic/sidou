// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SONG_TRANSPORT_H
#define SIDOU_SONG_TRANSPORT_H

#include <stdint.h>

#include "song/gt2_song.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SONG_T_VOICES 3u

#define SONG_T_IDLE 0u
#define SONG_T_STARTING 1u
#define SONG_T_PLAYING 2u
#define SONG_T_STOPPED 3u

#define SONG_T_RELEASE 0xffu
#define SONG_T_NOWHERE 0xffu

typedef struct {
    uint8_t state;
    uint8_t multispeed;
    uint8_t ntsc;
    uint8_t mute;
    uint8_t speed[2];
    uint8_t order_pos[SONG_T_VOICES];
    uint8_t row[SONG_T_VOICES];
    uint8_t wave[SONG_T_VOICES];
    uint8_t env[SONG_T_VOICES];
    uint8_t subtune;
    uint16_t row_call[SONG_T_VOICES];
    uint16_t now_call;
    uint8_t now_frac;
    uint8_t again;
    uint16_t rec_lag;
    uint32_t applied;
} song_status;

typedef struct {
    uint8_t state;
    uint8_t seeking;
    uint8_t speed[2];
    uint8_t from_top;
    uint32_t calls;
    uint32_t call_sample;
    uint32_t period_q16;
} song_clock;

enum { SONG_EDIT_ROW = 1, SONG_EDIT_PUSH, SONG_EDIT_PULL };
typedef struct {
    uint32_t kind;
    uint32_t pattern, row;
    uint8_t cell[GT2_ROW_BYTES];
} song_edit_request;
enum { SONG_EDIT_GUARD_OK, SONG_EDIT_GUARD_SAVING, SONG_EDIT_GUARD_FULL, SONG_EDIT_GUARD_BUSY };

typedef struct {
    int (*play)(void* ctx, uint32_t order_pos);
    int (*stop)(void* ctx);
    int (*resume)(void* ctx);
    int (*mute)(void* ctx, uint32_t mask);
    int (*jam)(void* ctx, uint32_t voice, uint32_t note, uint32_t instr);
    void (*status)(void* ctx, song_status* out);
    void* ctx;
    const gt2_song* song;
    int (*hold)(void* ctx, uint32_t on);
    int (*align)(void* ctx, uint32_t sample);
    int (*period)(void* ctx, uint32_t q16);
    int (*seek)(void* ctx, uint32_t call);
    int (*seek_further)(void* ctx, uint32_t call);
    void (*clock)(void* ctx, song_clock* out);
    int (*edit)(void* ctx, const gt2_edit* edit, uint32_t* ticket);
    int (*edit_done)(void* ctx, uint32_t ticket);
    int (*edit_guard)(void* ctx, const song_edit_request* request);
    int (*edit_with_locks)(void* ctx, const gt2_edit* edit, const uint8_t* locks,
                           uint32_t nlocks, uint32_t* ticket);
    int (*play_from)(void* ctx, uint32_t subtune, uint32_t order_pos);
    int (*draft)(void* ctx, uint32_t voice, uint32_t first, uint32_t n, const uint8_t* left,
                 const uint8_t* right);
    int (*held)(void* ctx, uint32_t voice, const uint8_t* locks, uint32_t n);
} song_transport;

#ifdef __cplusplus
}
#endif

#endif
