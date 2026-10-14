// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_H
#define SIDOU_SID_H

#include <stdint.h>

#include "sid_defs.h"
#include "sid_env.h"
#include "sid_filter.h"
#include "sid_osc.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FC_STEPS 2048

/* One SID chip. Fields are private to sid.c; the struct is public so callers can
 * allocate it statically. */
typedef struct {
    sid_osc osc[3];
    sid_env env[3];
    sid_svf svf;
    sid_model model;
    uint32_t cycles_per_sample_q16;
    uint32_t cycle_frac_q16;
    int32_t wave_zero;
    int32_t voice_dc;
    uint32_t fc;
    uint8_t res;
    uint8_t routing;
    uint8_t mode_vol;
    float volume;
    float dc_r, dc_x1, dc_y1;
    float cutoff_g[SID_FC_STEPS];
} sid;

/* sample_rate must be in [4000, 65535]. The oscillator step needs
 * freq * cycles < 2^24, so at most 256 chip cycles per sample: the minimum is
 * clock_hz / 256, about 3849 Hz for PAL and 3995 Hz for NTSC. The upper bound
 * keeps the Q16 cycle step in range. */
void sid_init(sid* s, sid_model model, uint32_t clock_hz, uint32_t sample_rate);
void sid_write(sid* s, uint8_t reg, uint8_t value);
void sid_render(sid* s, int16_t* out, uint32_t frames);

#ifdef __cplusplus
}
#endif

#endif
