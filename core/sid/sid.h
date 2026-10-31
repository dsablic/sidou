// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_H
#define SIDOU_SID_H

#include <stdint.h>

#include "sid_dac.h"
#include "sid_defs.h"
#include "sid_env.h"
#include "sid_filter.h"
#include "sid_osc.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FC_STEPS 2048

typedef struct {
    float* cutoff;
    float* res;
} sid_filter_row;

typedef struct {
    uint8_t osc3_held;
    uint8_t written;
    uint8_t osc3_latch;
    uint8_t control_age;
} sid_vflags;

typedef struct {
    sid_osc osc[3];
    sid_env env[3];
    sid_svf svf;
    sid_model model;
    uint32_t cycles_per_sample_q16;
    uint32_t cycle_frac_q16;
    uint32_t sample_rate;
    uint32_t fc;
    uint8_t res;
    uint8_t routing;
    uint8_t mode_vol;
    uint8_t bus;
    sid_vflags vf;
    uint8_t filter_curve;
    uint8_t filter_toward;
    uint8_t filter_dirty;
    float volume;
    float output_gain;
    float dc_r, dc_x1, dc_y1;
    float cutoff_g[SID_FC_STEPS];
    float res_k[16];
    float cutoff_x_to[SID_FC_STEPS];
    float res_k_to[16];
    float filter_t;
    const sid_dac* dac;
    sid_nonlin nl;
    float mix_lp, mix_bp, mix_bpn, mix_hp, dc_in;
    float out_c, out_lp;
    float thump, bleed;
    float thump_pending, thump_late;
    uint32_t thump_rendered, thump_late_at;
    uint16_t grit;
    uint32_t grit_now;
    uint32_t grit_seed;
    float filter_bias;
    sid_filter_row filter_rows[2];
    uint32_t noise_wb[2][16];
    uint8_t noise_writeback;
    uint8_t mute;
    uint8_t voice_path;
} sid;

#define SID_PATH_FILTER 0x01u
#define SID_PATH_DIRECT 0x10u

#define SID_GRIT_ONE 16384u
#define SID_GRIT_SLEW 48u
#define SID_GRIT_NOISE 17

_Static_assert(SID_GRIT_ONE == 1u << 14 && SID_GRIT_NOISE == 17, "sid_grit_sample's shifts");
#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline int16_t sid_grit_sample(int32_t x, int32_t n, int32_t a) {
    int32_t e = (x * ((n - (x < 0 ? -x : x)) >> 1)) >> 16;
    return (int16_t)(x + ((e * a) >> 14));
}

static inline sid_filter_row sid_filter_backing(sid* s, uint32_t second) {
    sid_filter_row row = {second ? s->cutoff_x_to : s->cutoff_g,
                          second ? s->res_k_to : s->res_k};
    return row;
}

#define SID_MIX_GAIN_BP 0.8125f

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline float sid_filter_cutoff(const sid_filter_row* rows, uint32_t fc, float t) {
    float g = rows[0].cutoff[fc];
    if (t != 0.0f) {
        float x = sidou_atanf(g);
        g = sidou_tanf_fast(x + t * (rows[1].cutoff[fc] - x));
    }
    return g;
}

static inline float sid_blended_cutoff(const sid* s, uint32_t fc, float t) {
    return sid_filter_cutoff(s->filter_rows, fc, t);
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void sid_update_filter(sid* s) {
    const float t = s->filter_t;
    float g = sid_blended_cutoff(s, s->fc, t), k = s->filter_rows[0].res[s->res];
    if (t != 0.0f) {
        k += t * (s->filter_rows[1].res[s->res] - k);
    }
    if (s->model == SID_MODEL_6581) {
        sid_svf_set_6581(&s->svf, &s->nl, g, k, s->dc_in);
        float bp = (s->mode_vol & 0x20u) ? 1.0f : 0.0f;
        s->mix_bpn = SID_MIX_GAIN_BP * bp / s->svf.k;
    } else {
        sid_svf_set(&s->svf, g, k);
    }
}

void sid_init(sid* s, sid_model model, uint32_t clock_hz, uint32_t sample_rate);
void sid_init_begin(sid* s, sid_model model, uint32_t clock_hz, uint32_t sample_rate);
void sid_filter_row_part(sid* s, int second, uint32_t curve, uint32_t from, uint32_t to);
void sid_init_end(sid* s);
void sid_write(sid* s, uint8_t reg, uint8_t value);
void sid_render(sid* s, int16_t* out, uint32_t frames);

typedef void (*sid_tap)(void* ctx, const sid* s);
void sid_render_tap(sid* s, int16_t* out, uint32_t frames, uint32_t every, sid_tap tap,
                    void* ctx);

uint8_t sid_read(sid* s, uint8_t reg);

void sid_set_combined_profile(sid* s, int profile);
const uint16_t* const* sid_combined_table(sid_model model, uint32_t profile);

void sid_set_gate_reset(sid* s, int on);

int sid_set_filter_curve(sid* s, uint32_t curve);

int sid_set_filter_blend(sid* s, uint32_t curve, uint32_t toward, float t);

int sid_set_filter_toward(sid* s, uint32_t toward);

void sid_filter_refresh(sid* s);

float sid_base_output_gain(const sid* s);
void sid_external_coefficients(uint32_t sample_rate, float lp_hz, float hp_hz,
                               float* out_c, float* dc_r);

void sid_nonlin_for(const sid* s, float amount, float t, sid_nonlin* out);
void sid_nonlin_for_rows(const sid* s, float amount, float t, const sid_filter_row* rows,
                         sid_nonlin* out);

void sid_set_filter_nonlinearity(sid* s, float amount);

void sid_set_dac(sid* s, const sid_dac* dac);

static inline uint32_t sid_next_cycles(const sid* s) {
    return (s->cycle_frac_q16 + s->cycles_per_sample_q16) >> 16;
}

void sid_advance(sid* s, uint32_t k);

static inline void sid_set_mute(sid* s, uint8_t mask) {
    s->mute = (uint8_t)(mask & 7u);
}

void sid_clock(sid* s, uint32_t cycles);

#ifdef __cplusplus
}
#endif

#endif
