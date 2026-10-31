// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid.h"

#include <stddef.h>
#include <string.h>

#ifdef __PI32V2__
__attribute__((section(".data.sid_noise_wb")))
#endif
const uint32_t SID_NOISE_WB[2][16] = {SID_WB_ROW(SID_WB_LO), SID_WB_ROW(SID_WB_HI)};

#ifdef __PI32V2__
__attribute__((section(".pool.sid_noise_jump")))
#endif
uint32_t SID_NOISE_JUMP[23][23];
#ifdef __PI32V2__
__attribute__((section(".pool.sid_noise_wb_byte")))
#endif
uint32_t SID_NOISE_WB_BYTE[256];

#ifdef __PI32V2__
__attribute__((section(".data.sid_pd_rows")))
#endif
const uint16_t* const SID_PD_ROWS[] = SID_PD_ROWS_INIT;

#if !defined(__PI32V2__) && (defined(__GNUC__) || defined(__clang__))
__attribute__((constructor))
#endif
void sid_noise_tables_init(void) {
    for (uint32_t i = 0; i < 23u; ++i) {
        uint32_t sr = 1u << i;
        SID_NOISE_JUMP[0][i] = (sr >> 1) | (((sr ^ (sr >> 5)) & 1u) << 22);
    }
    for (uint32_t k = 1; k < 23u; ++k) {
        const uint32_t* m = SID_NOISE_JUMP[k - 1];
        for (uint32_t i = 0; i < 23u; ++i) {
            uint32_t r = 0;
            for (uint32_t c = m[i], b = 0; c; c >>= 1, ++b) {
                if (c & 1u) {
                    r ^= m[b];
                }
            }
            SID_NOISE_JUMP[k][i] = r;
        }
    }
    for (uint32_t h = 0; h < 256u; ++h) {
        SID_NOISE_WB_BYTE[h] = SID_NOISE_WB[1][h >> 4] | SID_NOISE_WB[0][h & 15u];
    }
}

#define OUTPUT_GAIN (0.1f * 32767.0f)
#define LEVEL_6581 1.4736f
#define LEVEL_8580 1.1288f
#define DC_BLOCK_HZ 16.0f
#define THUMP_GATE 2.0f
#ifdef __PI32V2__
__attribute__((section(".data.sid_thump_attack")))
#endif
static const float THUMP_ATTACK[16] = {0.9059f, 0.6871f, 0.4972f, 0.3773f, 0.2560f, 0.1770f,
                                       0.1461f, 0.1243f, 0.0995f, 0.0398f, 0.0199f, 0.0124f,
                                       0.0099f, 0.0033f, 0.0020f, 0.0012f};

#define NL_LEVEL 0.055f
#define NL_LEVEL_REF 0.13875f
#define NL_LOAD 0.13125f
#define NL_LOAD_REF 1.7375f
#define NL_FLOOR_HZ 240.7f
#define MIX_GAIN_LP 0.6875f
#define MIX_GAIN_BP SID_MIX_GAIN_BP
#define MIX_GAIN_HP 0.5f
#define MIX_LEAK (1.0f / 3.375f)
#define DC_FILTER_INPUT 1.15625f
#define OUT_LP_HZ 13154.0f
#define CLOCK_STEP 16u

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void update_mix_bpn(sid* s) {
    float bp = (s->mode_vol & 0x20u) ? 1.0f : 0.0f;
    s->mix_bpn = MIX_GAIN_BP * bp / s->svf.k;
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void update_filter(sid* s) {
    sid_update_filter(s);
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void update_mix(sid* s) {
    if (s->model != SID_MODEL_6581) {
        return;
    }
    uint32_t routed = 0;
    for (uint32_t v = 0; v < 4u; ++v) {
        routed += (s->routing >> v) & 1u;
    }
    float lp = (s->mode_vol & 0x10u) ? 1.0f : 0.0f;
    float bp = (s->mode_vol & 0x20u) ? 1.0f : 0.0f;
    float hp = (s->mode_vol & 0x40u) ? 1.0f : 0.0f;
    s->mix_lp = -MIX_GAIN_LP * lp + MIX_GAIN_BP * MIX_LEAK * bp -
                MIX_GAIN_HP * MIX_LEAK * MIX_LEAK * hp;
    update_mix_bpn(s);
    s->mix_bp = -MIX_GAIN_HP * 2.0f * MIX_LEAK * hp;
    s->mix_hp = -MIX_GAIN_HP * hp;
    s->dc_in = s->filter_bias * (float)routed;
    s->svf.load_at = s->nl.load_ref - s->dc_in;
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void update_voice_masks(sid* s) {
    uint32_t off = s->routing | s->mute | ((s->mode_vol & 0x80u) ? 4u : 0u);
    s->voice_path = (uint8_t)((s->routing & 7u & ~(uint32_t)s->mute) * SID_PATH_FILTER |
                              (~off & 7u) * SID_PATH_DIRECT);
}

static float exp_neg(float x) {
    float y = x / 16.0f;
    float e = 1.0f - y * (1.0f - y / 2.0f * (1.0f - y / 3.0f * (1.0f - y / 4.0f * (1.0f - y / 5.0f))));
    for (int i = 0; i < 4; ++i) {
        e *= e;
    }
    return e;
}

void sid_external_coefficients(uint32_t sample_rate, float lp_hz, float hp_hz,
                               float* out_c, float* dc_r) {
    *out_c = 1.0f - exp_neg(2.0f * SIDOU_PI * lp_hz / (float)sample_rate);
    *dc_r = 1.0f - 2.0f * SIDOU_PI * hp_hz / (float)sample_rate;
}

static inline float cutoff_x(const sid* s, float hz) {
    float limit = 0.45f * (float)s->sample_rate;
    if (hz < 30.0f) {
        hz = 30.0f;
    }
    if (hz > limit) {
        hz = limit;
    }
    return SIDOU_PI * hz / (float)s->sample_rate;
}

static float cutoff_g(const sid* s, float hz) {
    return sidou_tanf(cutoff_x(s, hz));
}

void sid_nonlin_for_rows(const sid* s, float amount, float t, const sid_filter_row* rows,
                         sid_nonlin* out) {
    if (!(amount >= 0.0f)) {
        amount = 0.0f;
    } else if (amount > 2.0f) {
        amount = 2.0f;
    }
    float fs = (float)s->sample_rate;
    *out = s->nl;
    out->level = amount * NL_LEVEL * (44100.0f / fs);
    out->level_off = -out->level * out->level_ref;
    out->load = amount * NL_LOAD;
    float scale = amount < 1.0f ? amount : 1.0f;
    float table = sid_filter_cutoff(rows, 0, t) / cutoff_g(s, sid_cutoff_hz(s->model, 0, 0));
    out->floor = scale * sidou_tanf(SIDOU_PI * NL_FLOOR_HZ / fs) * table;
}

void sid_nonlin_for(const sid* s, float amount, float t, sid_nonlin* out) {
    sid_nonlin_for_rows(s, amount, t, s->filter_rows, out);
}

void sid_set_filter_nonlinearity(sid* s, float amount) {
    sid_nonlin n;
    sid_nonlin_for(s, amount, s->filter_t, &n);
    s->nl.level = n.level;
    s->nl.level_off = n.level_off;
    s->nl.load = n.load;
    s->nl.floor = n.floor;
}

float sid_base_output_gain(const sid* s) {
    return OUTPUT_GAIN * (s->model == SID_MODEL_6581 ? LEVEL_6581 : LEVEL_8580);
}

static void build_filter_row(const sid* s, uint32_t curve, int angles, float* g, float* k) {
    sid_cutoff_seq q;
    sid_cutoff_seq_init(&q, s->model, curve);
    for (uint32_t fc = 0; fc < SID_FC_STEPS; ++fc) {
        float hz = sid_cutoff_seq_hz(&q, fc);
        g[fc] = angles ? cutoff_x(s, hz) : cutoff_g(s, hz);
    }
    for (uint32_t r = 0; r < 16u; ++r) {
        k[r] = sid_resonance_k(s->model, curve, r);
    }
}

static void build_filter_tables(sid* s, uint32_t curve, uint32_t toward) {
    if (toward == curve) {
        sid_cutoff_seq q;
        sid_cutoff_seq_init(&q, s->model, curve);
        for (uint32_t fc = 0; fc < SID_FC_STEPS; ++fc) {
            float x = cutoff_x(s, sid_cutoff_seq_hz(&q, fc));
            s->filter_rows[0].cutoff[fc] = sidou_tanf(x);
            s->filter_rows[1].cutoff[fc] = x;
        }
        for (uint32_t r = 0; r < 16u; ++r) {
            s->filter_rows[0].res[r] = s->filter_rows[1].res[r] =
                sid_resonance_k(s->model, curve, r);
        }
    } else {
        build_filter_row(s, curve, 0, s->filter_rows[0].cutoff, s->filter_rows[0].res);
        build_filter_row(s, toward, 1, s->filter_rows[1].cutoff, s->filter_rows[1].res);
    }
    s->filter_curve = (uint8_t)curve;
    s->filter_toward = (uint8_t)toward;
}

int sid_set_filter_toward(sid* s, uint32_t toward) {
    if (toward >= sid_filter_curves(s->model)) {
        return -1;
    }
    if (toward != s->filter_toward) {
        build_filter_row(s, toward, 1, s->filter_rows[1].cutoff, s->filter_rows[1].res);
        s->filter_toward = (uint8_t)toward;
    }
    return 0;
}

int sid_set_filter_curve(sid* s, uint32_t curve) {
    return sid_set_filter_blend(s, curve, curve, 0.0f);
}

int sid_set_filter_blend(sid* s, uint32_t curve, uint32_t toward, float t) {
    uint32_t n = sid_filter_curves(s->model);
    if (curve >= n || toward >= n) {
        return -1;
    }
    if (!(t > 0.0f)) {
        t = 0.0f;
    } else if (t > 1.0f) {
        t = 1.0f;
    }
    build_filter_tables(s, curve, toward);
    s->filter_t = t;
    return 0;
}

_Static_assert(offsetof(sid, res_k) == offsetof(sid, cutoff_g) + sizeof(float) * SID_FC_STEPS &&
                   offsetof(sid, cutoff_x_to) == offsetof(sid, res_k) + sizeof(float) * 16u &&
                   offsetof(sid, res_k_to) ==
                       offsetof(sid, cutoff_x_to) + sizeof(float) * SID_FC_STEPS &&
                   offsetof(sid, filter_t) == offsetof(sid, res_k_to) + sizeof(float) * 16u,
               "the filter tables lie together, from cutoff_g to filter_t");

void sid_init_begin(sid* s, sid_model model, uint32_t clock_hz, uint32_t sample_rate) {
    memset(s, 0, offsetof(sid, cutoff_g));
    memset(&s->filter_t, 0, sizeof *s - offsetof(sid, filter_t));
    s->model = model;
    s->filter_rows[0] = sid_filter_backing(s, 0);
    s->filter_rows[1] = sid_filter_backing(s, 1);
    s->filter_bias = DC_FILTER_INPUT;
    s->noise_writeback = 127u;
    s->dac = sid_dac_default(model == SID_MODEL_6581);
    s->output_gain = OUTPUT_GAIN * (model == SID_MODEL_6581 ? LEVEL_6581 : LEVEL_8580);
    s->cycles_per_sample_q16 = ((clock_hz / sample_rate) << 16) |
                               (((clock_hz % sample_rate) << 16) / sample_rate);
    for (int v = 0; v < 3; ++v) {
        s->osc[v].acc = SID_OSC_POWER_ON_ACC;
        sid_osc_set_model(&s->osc[v], model == SID_MODEL_6581);
        s->osc[v].tri_saw = 0x555u;
        s->env[v].counter = SID_ENV_POWER_ON;
        sid_osc_reset(&s->osc[v]);
        sid_env_reset_settled(&s->env[v]);
    }
    s->sample_rate = sample_rate;
    sid_svf_reset(&s->svf);
    s->dc_r = 1.0f - 2.0f * SIDOU_PI * DC_BLOCK_HZ / (float)sample_rate;
    s->nl.level_ref = NL_LEVEL_REF;
    s->nl.load_ref = NL_LOAD_REF;
    s->out_c = 1.0f - exp_neg(2.0f * SIDOU_PI * OUT_LP_HZ / (float)sample_rate);
}

void sid_filter_row_part(sid* s, int second, uint32_t curve, uint32_t from, uint32_t to) {
    float* row = s->filter_rows[second != 0].cutoff;
    if (to > SID_FC_STEPS) {
        to = SID_FC_STEPS;
    }
    sid_cutoff_seq q;
    sid_cutoff_seq_init(&q, s->model, curve);
    for (uint32_t fc = from; fc < to; ++fc) {
        float hz = sid_cutoff_seq_hz(&q, fc);
        row[fc] = second ? cutoff_x(s, hz) : cutoff_g(s, hz);
    }
    if (to == SID_FC_STEPS) {
        float* k = s->filter_rows[second != 0].res;
        for (uint32_t r = 0; r < 16u; ++r) {
            k[r] = sid_resonance_k(s->model, curve, r);
        }
        if (second) {
            s->filter_toward = (uint8_t)curve;
        } else {
            s->filter_curve = (uint8_t)curve;
        }
    }
}

void sid_init_end(sid* s) {
    sid_set_filter_nonlinearity(s, 1.0f);
    update_filter(s);
    update_mix(s);
}

void sid_init(sid* s, sid_model model, uint32_t clock_hz, uint32_t sample_rate) {
    sid_init_begin(s, model, clock_hz, sample_rate);
    build_filter_tables(s, 0, 0);
    sid_init_end(s);
}

void sid_filter_refresh(sid* s) {
    s->filter_dirty = 0;
    update_filter(s);
    update_mix(s);
}

#define SID_VOICES_T sid
#define SID_VOICES_CLOCK clock_voices
#define SID_VOICES_OSC3 read_osc3
#include "sid_voices.h"

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void thump_step(sid* s, float step) {
    if (s->thump_rendered == 0u) {
        s->thump_pending += step;
    } else {
        s->thump_late += step;
        s->thump_late_at = s->thump_rendered;
    }
}

void sid_write(sid* s, uint8_t reg, uint8_t v) {
    s->bus = v;
    reg &= 0x1f;
    if (reg >= 0x07 && reg <= 0x12 && !s->vf.osc3_held) {
        s->vf.osc3_latch = read_osc3(s);
        s->vf.osc3_held = 1;
    }
    if (reg == 0x12) {
        s->vf.control_age = 0;
    }
    if (reg < 21) {
        s->vf.written = 1;
        sid_osc* o = &s->osc[reg / 7];
        sid_env* e = &s->env[reg / 7];
        switch (reg % 7) {
        case 0: sid_osc_write_freq(o, (o->freq & 0xff00u) | v); break;
        case 1: sid_osc_write_freq(o, (o->freq & 0x00ffu) | ((uint32_t)v << 8)); break;
        case 2: sid_osc_write_pw(o, (o->pw & 0xf00u) | v); break;
        case 3: sid_osc_write_pw(o, (o->pw & 0x0ffu) | ((uint32_t)(v & 0x0f) << 8)); break;
        case 4: {
            uint32_t src_acc = reg >= 7 ? s->osc[reg / 7 - 1].acc : sid_osc_acc_last(&s->osc[2]);
            sid_osc_latch_ramp(o, src_acc);
            sid_osc_write_control(o, v);
            if (s->thump != 0.0f && (v & SID_CTRL_GATE) && !e->gate) {
                uint32_t voice = reg / 7u;
                float path = 1.0f;
                if (s->routing & (1u << voice)) {
                    path = s->model == SID_MODEL_6581 ? s->mix_lp
                                                      : (s->mode_vol & 0x10u) ? 1.0f : 0.0f;
                } else if (voice == 2u && (s->mode_vol & 0x80u)) {
                    path = 0.0f;
                }
                thump_step(s, s->thump * THUMP_GATE * path * s->volume *
                                  THUMP_ATTACK[e->attack] * (float)(255u - e->counter) *
                                  (1.0f / 255.0f));
            }
            sid_env_set_gate(e, v & SID_CTRL_GATE);
            break;
        }
        case 5: sid_env_write_ad(e, v); break;
        case 6: sid_env_write_sr(e, v); break;
        }
        return;
    }
    switch (reg) {
    case 0x15:
        s->fc = (s->fc & 0x7f8u) | (v & 0x07u);
        s->filter_dirty = 1;
        break;
    case 0x16:
        s->fc = (s->fc & 0x007u) | ((uint32_t)v << 3);
        s->filter_dirty = 1;
        break;
    case 0x17:
        s->res = v >> 4;
        s->routing = v & 0x0f;
        s->filter_dirty = 1;
        update_mix(s);
        break;
    case 0x18: {
        float volume = (float)(v & 0x0f) / 15.0f;
        if (s->thump != 0.0f) {
            thump_step(s, s->thump * (volume - s->volume));
        }
        s->mode_vol = v;
        s->volume = volume;
        update_mix(s);
        break;
    }
    default: break;
    }
}

void sid_set_dac(sid* s, const sid_dac* dac) {
    s->dac = dac;
}

void sid_set_gate_reset(sid* s, int on) {
    for (int v = 0; v < 3; ++v) {
        s->env[v].gate_reset = on ? 1u : 0u;
    }
}

const uint16_t* const* sid_combined_table(sid_model model, uint32_t profile) {
    if (profile >= SID_PROFILES) {
        profile = SID_PROFILE_AVERAGE;
    }
    return model == SID_MODEL_6581 ? &SID_PD_ROWS[5u * SID_COMBINED_TABLE_6581(profile)]
                                   : &SID_PD_ROWS[SID_PD_8580_ROW + 5u * SID_COMBINED_TABLE_8580(profile)];
}

void sid_set_combined_profile(sid* s, int profile) {
    uint32_t p = profile >= 0 && profile < SID_PROFILES ? (uint32_t)profile : SID_PROFILE_AVERAGE;
    for (int v = 0; v < 3; ++v) {
        sid_osc_set_profile(&s->osc[v], p);
    }
}

uint8_t sid_read(sid* s, uint8_t reg) {
    switch (reg & 0x1f) {
    case 0x19:
    case 0x1a: s->bus = 0xff; break;
    case 0x1b: s->bus = s->vf.osc3_held ? s->vf.osc3_latch : read_osc3(s); break;
    case 0x1c: s->bus = s->env[2].env3; break;
    default: break;
    }
    return s->bus;
}

#ifdef SID_TEST_ZERO_VOICE
static _Thread_local uint32_t sid_test_zero_voices;
void sid_test_set_zero_voices(uint32_t mask);
void sid_test_set_zero_voices(uint32_t mask) {
    sid_test_zero_voices = mask;
}
#endif

static inline float voice_out(const sid* s, int v) {
#ifdef SID_TEST_ZERO_VOICE
    if ((sid_test_zero_voices >> v) & 1u) {
        return 0.0f;
    }
#endif
    return s->dac->wave[s->osc[v].out] * s->dac->env[s->env[v].counter];
}

static inline void fold_owed_inline(sid* s) {
    for (int v = 0; v < 3; ++v) {
        if (s->osc[v].owed >= 0x80000000u) {
            s->osc[v].owed -= 0x7fffffu * 0x100u;
        }
    }
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline))
#endif
static void fold_owed(sid* s) {
    fold_owed_inline(s);
}

void sid_clock(sid* s, uint32_t cycles) {
    fold_owed_inline(s);
    while (cycles) {
        uint32_t step = cycles < CLOCK_STEP ? cycles : CLOCK_STEP;
        clock_voices(s, step);
        cycles -= step;
    }
}

void sid_advance(sid* s, uint32_t k) {
    SID_DEBUG_ASSERT(k < sid_next_cycles(s));
    if (k == 0u) {
        return;
    }
    clock_voices(s, k);
    s->cycle_frac_q16 -= k << 16;
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void render_loop(sid* s, int16_t* out, uint32_t frames, uint32_t every,
                               sid_tap tap, void* ctx, const int bleed_on) {
    const float bleed = s->bleed;
    int16_t* const last = out + frames;
    while (out < last) {
        int16_t* const stop = last - out > (ptrdiff_t)every ? out + every : last;
        for (; out < stop; ++out) {
            s->cycle_frac_q16 += s->cycles_per_sample_q16;
            uint32_t cycles = s->cycle_frac_q16 >> 16;
            s->cycle_frac_q16 &= 0xffffu;

            clock_voices(s, cycles);

            float filt_in = 0.0f, direct = 0.0f;
            const uint32_t path = s->voice_path;
            for (int v = 0; v < 3; ++v) {
                float x = voice_out(s, v);
                if (path & (SID_PATH_FILTER << v)) {
                    filt_in += x;
                } else if (path & (SID_PATH_DIRECT << v)) {
                    direct += x;
                }
            }
            if (bleed_on) {
                direct += bleed * filt_in;
            }
            sid_svf_tick(&s->svf, filt_in);
            float filtered = 0.0f;
            if (s->mode_vol & 0x10u) {
                filtered += s->svf.lp;
            }
            if (s->mode_vol & 0x20u) {
                filtered += s->svf.bp;
            }
            if (s->mode_vol & 0x40u) {
                filtered += s->svf.hp;
            }

            float x = (direct + filtered) * s->volume;
            float y = x - s->dc_x1 + s->dc_r * s->dc_y1;
            if (y < 1e-15f && y > -1e-15f) {
                y = 0.0f;
            }
            s->dc_x1 = x;
            s->dc_y1 = y;

            float o = y * s->output_gain;
            o += o >= 0.0f ? 0.5f : -0.5f;
            if (o > 32767.0f) {
                o = 32767.0f;
            } else if (o < -32768.0f) {
                o = -32768.0f;
            }
            *out = (int16_t)o;
        }
        if (tap) {
            tap(ctx, s);
        }
    }
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void render_6581_body(sid* s, int16_t* out, uint32_t frames, uint32_t every,
                                    sid_tap tap, void* ctx, const int bleed_on) {
    const sid_nl_coefs c = sid_nl_coefs_of(&s->svf, &s->nl);
    float ic1 = s->svf.ic1, ic2 = s->svf.ic2, lp = s->svf.lp, bp = s->svf.bp, hp = s->svf.hp;
    float bpn = s->svf.bpn;
    const float mix_lp = s->mix_lp, mix_bp = s->mix_bp, mix_bpn = s->mix_bpn, mix_hp = s->mix_hp;
    const float vol_c = s->volume * s->out_c, out_c = s->out_c;
    float out_lp = s->out_lp, dc_x1 = s->dc_x1, dc_y1 = s->dc_y1;
    const float dc_r = s->dc_r, gain = s->output_gain;
    const float bleed = s->bleed;
    int16_t* const last = out + frames;
    while (out < last) {
        int16_t* const stop = last - out > (ptrdiff_t)every ? out + every : last;
        for (; out < stop; ++out) {
            s->cycle_frac_q16 += s->cycles_per_sample_q16;
            uint32_t cycles = s->cycle_frac_q16 >> 16;
            s->cycle_frac_q16 &= 0xffffu;

            clock_voices(s, cycles);

            float filt_in = 0.0f, direct = 0.0f;
            const uint32_t path = s->voice_path;
            for (int v = 0; v < 3; ++v) {
                float x = voice_out(s, v);
                if (path & (SID_PATH_FILTER << v)) {
                    filt_in += x;
                } else if (path & (SID_PATH_DIRECT << v)) {
                    direct += x;
                }
            }
            if (bleed_on) {
                direct += bleed * filt_in;
            }
            sid_nl_tick(&ic1, &ic2, &lp, &bp, &hp, &bpn, filt_in, &c);
            float filtered = mix_lp * lp + mix_bpn * bpn + mix_bp * bp + mix_hp * hp;
            float x = out_lp + (vol_c * (direct + filtered) - out_c * out_lp);
            if (x < 1e-15f && x > -1e-15f) {
                x = 0.0f;
            }
            out_lp = x;

            float y = x - dc_x1 + dc_r * dc_y1;
            if (y < 1e-15f && y > -1e-15f) {
                y = 0.0f;
            }
            dc_x1 = x;
            dc_y1 = y;
            float o = y * gain;
            o += o >= 0.0f ? 0.5f : -0.5f;
            if (o > 32767.0f) {
                o = 32767.0f;
            } else if (o < -32768.0f) {
                o = -32768.0f;
            }
            *out = (int16_t)o;
        }
        if (tap) {
            tap(ctx, s);
        }
    }
    s->svf.ic1 = ic1;
    s->svf.ic2 = ic2;
    s->svf.lp = lp;
    s->svf.bp = bp;
    s->svf.hp = hp;
    s->svf.bpn = bpn;
    s->out_lp = out_lp;
    s->dc_x1 = dc_x1;
    s->dc_y1 = dc_y1;
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline))
#endif
static void render_6581(sid* s, int16_t* out, uint32_t frames, uint32_t every, sid_tap tap,
                        void* ctx) {
    render_6581_body(s, out, frames, every, tap, ctx, 0);
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static void render_6581_bleed(sid* s, int16_t* out, uint32_t frames, uint32_t every,
                              sid_tap tap, void* ctx) {
    render_6581_body(s, out, frames, every, tap, ctx, 1);
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline))
#endif
static void render_8580(sid* s, int16_t* out, uint32_t frames, uint32_t every, sid_tap tap,
                        void* ctx) {
    render_loop(s, out, frames, every, tap, ctx, 0);
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline, unused))
#endif
static void render_8580_bleed(sid* s, int16_t* out, uint32_t frames, uint32_t every,
                              sid_tap tap, void* ctx) {
    render_loop(s, out, frames, every, tap, ctx, 1);
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((noinline))
#endif
static void grit(sid* s, int16_t* out, uint32_t frames) {
    const int32_t to = (int32_t)s->grit << 7;
    int32_t a = (int32_t)s->grit_now;
    const int32_t d = to >= a ? (int32_t)SID_GRIT_SLEW : -(int32_t)SID_GRIT_SLEW;
    const int32_t lo = to < a ? to : a, hi = to < a ? a : to;
    uint32_t r = s->grit_seed;
    for (uint32_t i = 0; i < frames; ++i) {
        a += d;
        a = a < lo ? lo : a > hi ? hi : a;
        r = r * 1664525u + 1013904223u;
        out[i] = sid_grit_sample(out[i], (int32_t)r >> 16, a >> 7);
    }
    s->grit_now = (uint32_t)a;
    s->grit_seed = r;
}

void sid_render(sid* s, int16_t* out, uint32_t frames) {
    sid_render_tap(s, out, frames, frames, 0, 0);
}

void sid_render_tap(sid* s, int16_t* out, uint32_t frames, uint32_t every, sid_tap tap,
                    void* ctx) {
    fold_owed(s);
    s->thump_rendered += frames;
    if (s->filter_dirty) {
        s->filter_dirty = 0;
        update_filter(s);
    }
    update_voice_masks(s);
    int bleed = s->bleed != 0.0f;
    if (s->model == SID_MODEL_6581) {
        if (bleed) {
            render_6581_bleed(s, out, frames, every, tap, ctx);
        } else {
            render_6581(s, out, frames, every, tap, ctx);
        }
    } else if (bleed) {
        render_8580_bleed(s, out, frames, every, tap, ctx);
    } else {
        render_8580(s, out, frames, every, tap, ctx);
    }
    if (s->grit | s->grit_now) {
        grit(s, out, frames);
    }
}

void (*const sid_noise_settle_deferred_far)(sid_osc* o) = sid_noise_settle_deferred;
void (*const sid_noise_clock_far)(sid_osc* o, uint32_t prev, uint32_t next,
                                   uint32_t cycles) = sid_noise_clock;
void (*const sid_osc_run_combined_far)(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                        uint32_t src_freq) = sid_osc_run_combined;
void (*const sid_osc_advance_slow_far)(sid_osc* o, uint32_t cycles, uint32_t src_acc,
                                        uint32_t src_freq) = sid_osc_advance_slow;
void (*const sid_env_run_far)(sid_env* e, uint32_t cycles, int may_rest) = sid_env_run;
void (*const sid_env_advance_slow_far)(sid_env* e, uint32_t cycles) = sid_env_advance_slow;
