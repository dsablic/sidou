// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_H
#define SIDOU_FX_H

#include <stdint.h>

#include "fx_crt.h"
#include "fx_crush.h"
#include "fx_noise.h"
#include "fx_tape.h"
#include "fx_wow.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    float tape_drive;
    float wow;
    float flutter;
    float noise;
    float crt;
    float crush;
} sid_fx_params;

typedef struct {
    sid_fx_params target, cur;
    float smooth_coef, smooth_coef2;
    uint32_t active;
    uint32_t clock;
    sid_fx_crush crush;
    sid_fx_tape tape;
    sid_fx_wow wow;
    sid_fx_crt crt;
    sid_fx_noise noise;
} sid_fx;

void sid_fx_init(sid_fx* fx, uint32_t sample_rate, uint32_t seed);
void sid_fx_set(sid_fx* fx, const sid_fx_params* p);
void sid_fx_process(sid_fx* fx, int16_t* io, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
