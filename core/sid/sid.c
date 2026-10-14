// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid.h"

#include <string.h>

#define VOICE_SCALE (1.0f / (2048.0f * 255.0f))
#define OUTPUT_GAIN (0.1f * 32767.0f) /* leaves headroom for the 6581 DC offset and filter peaks */
#define DC_BLOCK_HZ 16.0f
#define DRIVE_6581 2.0f
/* sid_clock's step: sid_osc_advance needs freq * cycles < 2^24, so at most 256. */
#define CLOCK_STEP 16u

static void update_filter(sid* s) {
    float q = 0.707f + (float)s->res / 15.0f;
    sid_svf_set(&s->svf, s->cutoff_g[s->fc], 1.0f / q);
}

void sid_init(sid* s, sid_model model, uint32_t clock_hz, uint32_t sample_rate) {
    memset(s, 0, sizeof *s);
    s->model = model;
    /* reSID 0.16 voice DAC zero level and DC offset per chip revision. */
    s->wave_zero = model == SID_MODEL_6581 ? 0x380 : 0x800;
    s->voice_dc = model == SID_MODEL_6581 ? 0x800 * 0xff : 0;
    s->cycles_per_sample_q16 = ((clock_hz / sample_rate) << 16) |
                               (((clock_hz % sample_rate) << 16) / sample_rate);
    for (int v = 0; v < 3; ++v) {
        s->osc[v].acc = SID_OSC_POWER_ON_ACC;
        s->env[v].counter = SID_ENV_POWER_ON;
        sid_osc_reset(&s->osc[v]);
        sid_env_reset(&s->env[v]);
    }
    float limit = 0.45f * (float)sample_rate;
    for (uint32_t fc = 0; fc < SID_FC_STEPS; ++fc) {
        float hz = sid_cutoff_hz(model, fc);
        if (hz < 30.0f) {
            hz = 30.0f;
        }
        if (hz > limit) {
            hz = limit;
        }
        s->cutoff_g[fc] = sidou_tanf(SIDOU_PI * hz / (float)sample_rate);
    }
    sid_svf_reset(&s->svf, model == SID_MODEL_6581 ? DRIVE_6581 : 0.0f);
    s->dc_r = 1.0f - 2.0f * SIDOU_PI * DC_BLOCK_HZ / (float)sample_rate;
    update_filter(s);
}

void sid_write(sid* s, uint8_t reg, uint8_t v) {
    s->bus = v;
    reg &= 0x1f;
    if (reg < 21) {
        sid_osc* o = &s->osc[reg / 7];
        sid_env* e = &s->env[reg / 7];
        switch (reg % 7) {
        case 0: o->freq = (o->freq & 0xff00u) | v; break;
        case 1: o->freq = (o->freq & 0x00ffu) | ((uint32_t)v << 8); break;
        case 2: o->pw = (o->pw & 0xf00u) | v; break;
        case 3: o->pw = (o->pw & 0x0ffu) | ((uint32_t)(v & 0x0f) << 8); break;
        case 4:
            sid_osc_write_control(o, v);
            sid_env_set_gate(e, v & SID_CTRL_GATE);
            break;
        case 5: sid_env_write_ad(e, v); break;
        case 6: sid_env_write_sr(e, v); break;
        }
        return;
    }
    switch (reg) {
    case 0x15:
        s->fc = (s->fc & 0x7f8u) | (v & 0x07u);
        update_filter(s);
        break;
    case 0x16:
        s->fc = (s->fc & 0x007u) | ((uint32_t)v << 3);
        update_filter(s);
        break;
    case 0x17:
        s->res = v >> 4;
        s->routing = v & 0x0f;
        update_filter(s);
        break;
    case 0x18:
        s->mode_vol = v;
        s->volume = (float)(v & 0x0f) / 15.0f;
        break;
    default: break;
    }
}

static inline uint32_t acc_one_cycle_ago(const sid_osc* o) {
    return o->control & SID_CTRL_TEST ? o->acc : (o->acc - o->freq) & 0xffffffu;
}

/* OSC3 as reSIDfp latches it, which differs from the waveform output by up to two
 * cycles (all "verified by sampling OSC3" there):
 * - the 8580's triangle and sawtooth reach OSC3 one cycle late;
 * - the pulse comparison is one cycle late on both models;
 * - the noise LFSR shifts two cycles after accumulator bit 19 rises, while
 *   sid_osc_advance shifts it at once, so a shift from the last two cycles is undone.
 * The cycle before a hard sync or a TEST release is not reconstructed exactly. */
static uint8_t read_osc3(const sid* s) {
    const sid_osc* o = &s->osc[2];
    const sid_osc* src = &s->osc[1];
    uint32_t c = o->control;
    if (!(c & 0xf0u)) {
        return 0;
    }
    uint32_t out = 0xfffu;
    if (c & (SID_CTRL_TRI | SID_CTRL_SAW)) {
        sid_osc ramp = *o;
        sid_osc ring = *src;
        ramp.control = (uint8_t)(c & ~(SID_CTRL_PULSE | SID_CTRL_NOISE));
        if (s->model == SID_MODEL_8580) {
            ramp.acc = acc_one_cycle_ago(o);
            ring.acc = acc_one_cycle_ago(src);
        }
        out &= sid_osc_output(&ramp, &ring);
    }
    if (c & SID_CTRL_PULSE) {
        out &= ((c & SID_CTRL_TEST) || (acc_one_cycle_ago(o) >> 12) >= o->pw) ? 0xfffu : 0u;
    }
    if (c & SID_CTRL_NOISE) {
        uint32_t lfsr = o->lfsr;
        uint32_t before = (o->acc - 2u * o->freq) & 0xffffffu;
        uint32_t now = o->acc < before ? o->acc + 0x1000000u : o->acc;
        if (!(c & SID_CTRL_TEST) && ((now + 0x80000u) >> 20) != ((before + 0x80000u) >> 20)) {
            /* Shift back: old bit 22 = new bit 0 ^ old bit 17 (= new bit 18). */
            lfsr = (lfsr >> 1) | (((lfsr ^ (lfsr >> 18)) & 1u) << 22);
        }
        out &= sid_noise_output(lfsr);
    }
    return (uint8_t)(out >> 4);
}

uint8_t sid_read(sid* s, uint8_t reg) {
    switch (reg & 0x1f) {
    case 0x19:
    case 0x1a: s->bus = 0xff; break; /* paddles: nothing connected */
    case 0x1b: s->bus = read_osc3(s); break;
    case 0x1c: s->bus = s->env[2].counter; break;
    default: break;
    }
    return s->bus;
}

static inline void clock_voices(sid* s, uint32_t cycles) {
    for (int v = 0; v < 3; ++v) {
        sid_osc_advance(&s->osc[v], cycles);
    }
    for (int v = 0; v < 3; ++v) {
        const sid_osc* src = &s->osc[(v + 2) % 3];
        if ((s->osc[v].control & SID_CTRL_SYNC) && src->msb_rose) {
            sid_osc_sync(&s->osc[v], src);
        }
        sid_env_advance(&s->env[v], cycles);
    }
}

void sid_clock(sid* s, uint32_t cycles) {
    while (cycles) {
        uint32_t step = cycles < CLOCK_STEP ? cycles : CLOCK_STEP;
        clock_voices(s, step);
        cycles -= step;
    }
}

static inline float voice_out(const sid* s, int v) {
    uint32_t wave = sid_osc_output(&s->osc[v], &s->osc[(v + 2) % 3]);
    int32_t x = ((int32_t)wave - s->wave_zero) * (int32_t)s->env[v].counter + s->voice_dc;
    return (float)x * VOICE_SCALE;
}

void sid_render(sid* s, int16_t* out, uint32_t frames) {
    for (uint32_t i = 0; i < frames; ++i) {
        s->cycle_frac_q16 += s->cycles_per_sample_q16;
        uint32_t cycles = s->cycle_frac_q16 >> 16;
        s->cycle_frac_q16 &= 0xffffu;

        clock_voices(s, cycles);

        float filt_in = 0.0f, direct = 0.0f;
        for (int v = 0; v < 3; ++v) {
            float x = voice_out(s, v);
            if (s->routing & (1u << v)) {
                filt_in += x;
            } else if (!(v == 2 && (s->mode_vol & 0x80u))) {
                direct += x;
            }
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
        /* Same reason as sid_svf_tick: on constant input y decays into denormals. */
        if (y < 1e-15f && y > -1e-15f) {
            y = 0.0f;
        }
        s->dc_x1 = x;
        s->dc_y1 = y;

        /* Round half away from zero, clamped so the cast stays in range. */
        float o = y * OUTPUT_GAIN;
        o += o >= 0.0f ? 0.5f : -0.5f;
        if (o > 32767.0f) {
            o = 32767.0f;
        } else if (o < -32768.0f) {
            o = -32768.0f;
        }
        out[i] = (int16_t)o;
    }
}
