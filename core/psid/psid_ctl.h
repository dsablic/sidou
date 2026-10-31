// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_PSID_CTL_H
#define SIDOU_PSID_CTL_H

#include <stdint.h>

#include "psid/psid_env.h"
#include "psid/psid_file.h"
#include "psid/psid_lift.h"
#include "sid/sid.h"
#include "sid/sid_shadow.h"

#ifdef __cplusplus
extern "C" {
#endif

#define PSID_BLOCK 128u
#define PSID_RING 1024u

#define PSID_SLICE_CYCLES 1000u
#define PSID_SUB_CYCLES 32u
#define PSID_WORK_CYCLE 12u
#define PSID_WORK_ACCESS 480u
#define PSID_WORK_NOISY 80u
#define PSID_SLICE_WORK 20000u
#define PSID_AHEAD_CYCLES 39312u
#define PSID_SLICE_ROOM (PSID_SLICE_WORK / PSID_WORK_ACCESS + 1u + PSID_SUB_CYCLES / 3u + 2u)
#define PSID_SPLITS 24u
#define PSID_BLOCK_ENTRIES 80u

#define PSID_RING_ENTRY(chip, reg, value) \
    ((uint16_t)((uint32_t)(chip) << 13 | ((uint32_t)(reg) & 0x1fu) << 8 | ((uint32_t)(value) & 0xffu)))
#define PSID_RING_CHIP(e) ((uint32_t)(e) >> 13)
#define PSID_RING_REG(e) (((uint32_t)(e) >> 8) & 0x1fu)
#define PSID_RING_VALUE(e) ((uint32_t)(e) & 0xffu)

enum {
    PSID_EMPTY = 0,
    PSID_LOADED,
    PSID_INIT,
    PSID_PLAYING,
    PSID_PAUSED,
    PSID_STOPPED,
    PSID_FAILED
};

enum { PSID_ISR_OFF = 0, PSID_ISR_PLAY, PSID_ISR_PAUSE, PSID_ISR_STOP };

typedef struct {
    sid* (*chip)(void* ctx, uint32_t chip);
    void (*mute)(void* ctx, uint32_t chip, uint32_t mask);
    void* ctx;
} psid_ctl_hooks;

typedef struct {
    psid_env env;
    sid_shadow shadow[2];
} psid_work;

typedef void (*psid_render_fn)(sid* s, int16_t* out, uint32_t n, int tapped);

typedef struct psid_ctl {
    uint32_t ring_cycle[PSID_RING];
    uint16_t ring_entry[PSID_RING];
    volatile uint32_t head;
    volatile uint32_t want_seq;
    volatile uint8_t want;
    volatile uint8_t chips;
    volatile uint32_t tail;
    volatile uint32_t done_seq;
    volatile uint32_t clock[2];
    volatile uint32_t late;
    volatile uint32_t blocks;
    uint32_t cur[2];
    uint8_t mode;
    uint8_t fade_in;
    uint32_t splits_max;
    psid_work* w;
    uint8_t* mem;
    const psid_header* h;
    const uint8_t* file;
    psid_ctl_hooks hooks;
    psid_lift* lift;
    uint32_t stage;
    uint32_t slices;
    uint32_t writes;
    uint32_t dropped;
    int16_t error;
    uint8_t state;
    uint8_t paused_from;
    uint8_t started;
    uint8_t stop_wait;
    uint16_t next_song;
    uint8_t mute;
} psid_ctl;

void psid_ctl_init(psid_ctl* c, psid_work* w, uint8_t* mem, const psid_ctl_hooks* hooks);
int psid_ctl_load(psid_ctl* c, const psid_header* h, const uint8_t* file);
int psid_ctl_load_over(psid_ctl* c, const psid_header* h);
void psid_ctl_load_part(psid_ctl* c, uint32_t offset, const uint8_t* bytes, uint32_t n);
int psid_ctl_play(psid_ctl* c, uint32_t song);
int psid_ctl_stop(psid_ctl* c);
int psid_ctl_pause(psid_ctl* c);
int psid_ctl_resume(psid_ctl* c);
int psid_ctl_subtune(psid_ctl* c, uint32_t song);
int psid_ctl_mute(psid_ctl* c, uint32_t mask);
int psid_ctl_poll(psid_ctl* c);
uint32_t psid_ctl_state(const psid_ctl* c);
uint32_t psid_ctl_ahead(const psid_ctl* c);

uint32_t psid_ctl_block(psid_ctl* c, sid* a, sid* b, int16_t* sa, int16_t* sb, uint32_t tapped,
                        psid_render_fn render);

#ifdef __cplusplus
}
#endif

#endif
