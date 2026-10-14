// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_H
#define SIDOU_FX_H

/* The post-chip colour chain, in a fixed order: crush -> tape -> wow and flutter -> CRT
 * -> noise. A thin composition of the blocks in fx_*.h, each with its own state and tick
 * function, so other chains can reuse and reorder them.
 *
 * Every parameter is smoothed with a one-pole (SID_FX_SMOOTH_MS). A block whose smoothed
 * parameter is 0 is out of the signal path. When every target and smoothed value is 0
 * (and wow has glided back to no delay) sid_fx_process returns at once without touching
 * the samples or the state, so the bypass is exact. */

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
    float tape_drive; /* 0..1 */
    float wow;        /* 0..1 depth */
    float flutter;    /* 0..1 depth */
    float noise;      /* 0..1 hiss level */
    float crt;        /* 0..1 low-pass amount + hum level */
    float crush;      /* 0..1 sample-rate / bit-depth reduction */
} sid_fx_params;

/* Fields are private to fx.c; the struct is public so callers can allocate it. */
typedef struct {
    sid_fx_params target, cur;
    float smooth_coef;
    uint32_t active; /* SID_FX_ON_* bits: blocks in the path on the last sample */
    sid_fx_crush crush;
    sid_fx_tape tape;
    sid_fx_wow wow;
    sid_fx_crt crt;
    sid_fx_noise noise;
} sid_fx;

/* sample_rate must be in [8000, 96000]. */
void sid_fx_init(sid_fx* fx, uint32_t sample_rate, uint32_t seed);
/* New targets, each clamped to [0, 1] (NaN gives 0); smoothed from the current values. */
void sid_fx_set(sid_fx* fx, const sid_fx_params* p);
void sid_fx_process(sid_fx* fx, int16_t* io, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
