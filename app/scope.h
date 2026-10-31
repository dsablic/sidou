// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SCOPE_H
#define SIDOU_SCOPE_H

#include <stdint.h>

#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCOPE_VOICES 3u
#define SCOPE_DECIMATE 16u
#define SCOPE_RING 256u
#define SCOPE_READ_MAX (SCOPE_RING - 32u)
#define SCOPE_PEAK_STRIDE 8u
#define SCOPE_W 160u
#define SCOPE_METER_MAX 72u

#ifndef __cplusplus
_Static_assert((SCOPE_RING & (SCOPE_RING - 1u)) == 0u, "the ring's size is a power of two");
#endif

typedef struct {
    uint8_t wave[SCOPE_VOICES];
    uint8_t phase[SCOPE_VOICES];
} scope_point;

typedef struct {
    scope_point point[SCOPE_RING];
    volatile uint32_t head;
    volatile uint8_t env[SCOPE_VOICES];
    volatile uint8_t state[SCOPE_VOICES];
    volatile uint8_t sustain[SCOPE_VOICES];
    volatile uint16_t peak[2][2];
    volatile uint8_t peak_slot;
    volatile uint8_t on;
} scope;

void scope_init(scope* sc);

void scope_render(scope* sc, sid* chip, int16_t* out, uint32_t frames);
void scope_peaks(scope* sc, const int16_t* lr, uint32_t frames);

uint32_t scope_read(const scope* sc, uint32_t since, scope_point* out, uint32_t n);
static inline int scope_intact(uint32_t oldest, uint32_t head) {
    return head - oldest < SCOPE_RING;
}
void scope_take_peaks(scope* sc, uint16_t out[2]);

#ifdef SIDOU_REG_RING_RACE_HOOK
void scope_set_race_hook(void (*hook)(void* ctx), void* ctx);
#endif

void scope_trace(const scope_point* p, uint32_t n, uint32_t voice, uint32_t by, uint32_t cycles,
                 uint8_t out[SCOPE_W]);
uint32_t scope_meter(uint32_t amplitude);

#ifdef __cplusplus
}
#endif

#endif
