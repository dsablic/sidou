// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_KEYS_ROUTE_H
#define SIDOU_KEYS_ROUTE_H

#include <stdint.h>

#include "chip_ctl.h"
#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

#define KEYS_ROUTE_EDIT_CONTROLS 0x0fu
#define KEYS_ROUTE_EDIT_PROFILE 0x10u
#define KEYS_ROUTE_EDIT_ENVELOPE 0x20u
#define KEYS_ROUTE_EDIT_EXPERT_FIRST 6u
enum {
    KEYS_ROUTE_EDIT_EXPERT = ((1u << SID_EXPERT_COUNT) - 1u) << KEYS_ROUTE_EDIT_EXPERT_FIRST,
    KEYS_ROUTE_EDIT_ALL = 0x3fu | KEYS_ROUTE_EDIT_EXPERT
};

typedef struct {
    live* l;
    live_sink_fn sink;
    live_prepare_fn prepare;
    void* out[2];
    chip_ctl ctl[2];
    uint8_t song;
    uint8_t song_on;
    uint8_t player;
    uint8_t tune;
    uint8_t silent;
    uint8_t stale;
    uint8_t under;
    uint8_t taken;
    chip_setup song_want;
    chip_setup tune_b_want;
    uint8_t song_seen;
    uint8_t song_user;
    void (*moved)(void* ctx, uint32_t chip);
    void* moved_ctx;
    void (*taking)(void* ctx, uint32_t voices);
    void* taking_ctx;
    int (*song_edit)(void* ctx, const chip_setup* setup, uint32_t edit_mask);
    void* song_edit_ctx;
} keys_route;

void keys_route_init(keys_route* kr, live* l, live_sink_fn sink, live_prepare_fn prepare,
                     void* out_a, void* out_b);
void keys_route_start(keys_route* kr);
void keys_route_sink(void* ctx, uint8_t reg, uint8_t value);
int keys_route_prepare(void* ctx, uint8_t profile);
void keys_route_tick(keys_route* kr);
void keys_route_set_song(keys_route* kr, int playing);
void keys_route_set_player(keys_route* kr, int owns);
void keys_route_set_tune(keys_route* kr, int on);
void keys_route_set_silent(keys_route* kr, int on);
void keys_route_fresh(keys_route* kr, uint32_t chip);
uint8_t keys_route_voices_taken(const keys_route* kr);
int keys_route_ready_for_song(const keys_route* kr);
void keys_route_song_wants(keys_route* kr, const chip_setup* setup);
int keys_route_song_edit(keys_route* kr, const chip_setup* setup, uint32_t edit_mask);
void keys_route_song_setup(keys_route* kr, const chip_setup* setup);

#ifdef __cplusplus
}
#endif

#endif
