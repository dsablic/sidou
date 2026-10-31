// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_PLATE_H
#define SIDOU_FX_PLATE_H

#include <stdint.h>

#include "fx_util.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FX_PLATE_LEN 32768u
#define SID_FX_PLATE_LINES 12
#define SID_FX_PLATE_LFO_HZ 1.0f
#define SID_FX_PLATE_SEGMENT 16u

typedef struct {
    uint32_t base[SID_FX_PLATE_LINES];
    uint32_t length[SID_FX_PLATE_LINES];
    uint32_t tap[14];
    float excursion;
    uint32_t wp;
    uint32_t lfo_phase, lfo_inc;
    uint32_t left;
    float mod_l, mod_r, step_l, step_r;
    float bw, damp_l, damp_r;
    float tank_l, tank_r;
} sid_fx_plate;

void sid_fx_plate_init(sid_fx_plate* p, uint32_t sample_rate);
void sid_fx_plate_tick(sid_fx_plate* p, float* buf, float m, float decay, float damping,
                       float bandwidth, float* l, float* r);

#ifdef __cplusplus
}
#endif

#endif
