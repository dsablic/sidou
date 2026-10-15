// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid.h"

#include <string.h>

#define OUTPUT_GAIN (0.1f * 32767.0f) /* leaves headroom for the 6581 DC offset and filter peaks */
/* reSIDfp's output level relative to OUTPUT_GAIN: the RMS of one unfiltered voice at
 * volume 15 (sawtooth, triangle, 25% and 50% pulse, at 0x08d6 and 0x1d45), measured
 * through its mixer and output stages: 3.37 dB louder on the 6581, 1.05 dB on the 8580
 * (reference.rs asserts the match). */
#define LEVEL_6581 1.4736f
#define LEVEL_8580 1.1288f
#define DC_BLOCK_HZ 16.0f
#define DRIVE_6581 2.0f
/* sid_clock's step. sid_osc_advance needs freq * cycles < 2^24, so at most 256.
 * Within that, the step only matters for hard sync, which is approximated once per
 * step (see sid_osc_sync); 16 is no longer than sid_render's step at any PAL rate up
 * to 61.5 kHz, so sid_clock is never coarser than rendering. The loop's cost does not
 * matter: sid_clock is for tests and register-accurate seeking, not the audio path. */
#define CLOCK_STEP 16u

static void update_filter(sid* s) {
    float q = 0.707f + (float)s->res / 15.0f;
    sid_svf_set(&s->svf, s->cutoff_g[s->fc], 1.0f / q);
}

void sid_init(sid* s, sid_model model, uint32_t clock_hz, uint32_t sample_rate) {
    memset(s, 0, sizeof *s);
    s->model = model;
    s->dac = sid_dac_default(model == SID_MODEL_6581);
    s->output_gain = OUTPUT_GAIN * (model == SID_MODEL_6581 ? LEVEL_6581 : LEVEL_8580);
    s->cycles_per_sample_q16 = ((clock_hz / sample_rate) << 16) |
                               (((clock_hz % sample_rate) << 16) / sample_rate);
    for (int v = 0; v < 3; ++v) {
        s->osc[v].acc = SID_OSC_POWER_ON_ACC;
        sid_osc_set_model(&s->osc[v], model == SID_MODEL_6581);
        s->osc[v].tri_saw = 0x555u; /* reSIDfp's power-on latch; reset keeps it */
        s->env[v].counter = SID_ENV_POWER_ON;
        sid_osc_reset(&s->osc[v]);
        sid_env_reset_settled(&s->env[v]); /* zeroed above: nothing to settle */
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

static uint8_t read_osc3(const sid* s);

void sid_write(sid* s, uint8_t reg, uint8_t v) {
    s->bus = v;
    reg &= 0x1f;
    /* OSC3 latched in the last cycle, before a write to voice 2 (the ring source) or
     * voice 3 changes what read_osc3 would reconstruct it from. */
    if (reg >= 0x07 && reg <= 0x12 && !s->osc3_held) {
        s->osc3_latch = read_osc3(s);
        s->osc3_held = 1;
    }
    if (reg == 0x12) {
        s->control_age = 0;
    }
    if (reg < 21) {
        s->written = 1;
        sid_osc* o = &s->osc[reg / 7];
        sid_env* e = &s->env[reg / 7];
        switch (reg % 7) {
        case 0: sid_osc_write_freq(o, (o->freq & 0xff00u) | v); break;
        case 1: sid_osc_write_freq(o, (o->freq & 0x00ffu) | ((uint32_t)v << 8)); break;
        case 2: sid_osc_write_pw(o, (o->pw & 0xf00u) | v); break;
        case 3: sid_osc_write_pw(o, (o->pw & 0x0ffu) | ((uint32_t)(v & 0x0f) << 8)); break;
        case 4: {
            /* The ring source as this voice's output sees it (sid_osc_output). */
            uint32_t src_acc = reg >= 7 ? s->osc[reg / 7 - 1].acc : sid_osc_acc_last(&s->osc[2]);
            sid_osc_latch_ramp(o, src_acc);
            sid_osc_write_control(o, v);
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

/* OSC3 as reSIDfp latches it. On the 6581 that is the waveform output of the last
 * cycle. On the 8580 the triangle and sawtooth reach it a cycle late: it combines the
 * previous cycle's triangle/saw with the pulse and noise of the last one (whose pulse
 * comparison is itself a cycle late on both models). Noise combined with other
 * waveforms runs cycle by cycle and keeps its OSC3. Otherwise the previous cycle is
 * reconstructed from the current state. A read right after a write to voice 2 or 3,
 * before any cycle, returns the value latched before the write (sid_write). A read a
 * cycle or two later is wrong after:
 * - a frequency or pulse width write (the old values drove those cycles);
 * - a control write: the pulse comparison from before a TEST change (the 8580's
 *   triangle/saw latch, tri_saw, is exact: it holds the last triangle/saw value,
 *   0x555 from power-on, and is used for the one cycle after the write);
 * - a hard sync of voice 3 or voice 2 (with ring modulation), whose accumulator
 *   restarted mid-step.
 * With no waveform selected OSC3 reads 0. reSIDfp instead holds the last output on its
 * floating DAC input and fades it after 54,000 (6581) or 800,000 (8580) cycles; SIDOU
 * keeps 0 because programs rarely read OSC3 with the waveform off, and holding the
 * value would cost state on the audio path. */
static uint8_t read_osc3(const sid* s) {
    const sid_osc* o = &s->osc[2];
    const sid_osc* src = &s->osc[1];
    uint32_t wf = (uint32_t)o->control >> 4;
    if (!wf) {
        return 0;
    }
    if (o->cycle_mode) {
        return (uint8_t)(o->osc3 >> 4);
    }
    uint32_t before = sid_osc_acc_before(o);
    uint32_t pulse = sid_osc_pulse(o, before);
    uint32_t out;
    if (s->model == SID_MODEL_8580 && (wf & 3u)) {
        uint32_t ramp = s->control_age == 1
                            ? o->tri_saw
                            : sid_wave_ramp(wf, sid_wave_index(o->control, before,
                                                               sid_osc_acc_before(src)));
        out = sid_osc_combine_ramp(o, ramp, pulse, o->noise);
    } else {
        uint32_t ix = sid_wave_index(o->control, sid_osc_acc_last(o), src->acc);
        out = sid_osc_combine(o, ix, pulse, o->noise);
    }
    return (uint8_t)(out >> 4);
}

void sid_set_dac(sid* s, const sid_dac* dac) {
    s->dac = dac;
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
    case 0x1a: s->bus = 0xff; break; /* paddles: nothing connected */
    case 0x1b: s->bus = s->osc3_held ? s->osc3_latch : read_osc3(s); break;
    case 0x1c: s->bus = s->env[2].env3; break;
    default: break;
    }
    return s->bus;
}

static inline void clock_voices(sid* s, uint32_t cycles) {
    s->osc3_held = 0;
    s->control_age = (uint8_t)(s->control_age + cycles > 2u ? 2u : s->control_age + cycles);
    sid_osc* o = s->osc;
    /* Each voice's sync and ring modulation source is the voice before it. */
    uint32_t acc2 = o[2].acc;
    /* Voices 2 and 3 see their source after its MSB pull (reSIDfp's output order). */
    o[2].src_mask = o[1].pull_low ? 0x7fffffu : 0xffffffu;
    o[1].src_mask = o[0].pull_low ? 0x7fffffu : 0xffffffu;
    sid_osc_advance(&o[2], cycles, o[1].acc, o[1].freq);
    sid_osc_advance(&o[1], cycles, o[0].acc, o[0].freq);
    sid_osc_advance(&o[0], cycles, acc2, o[2].freq);
    for (int v = 0; v < 3; ++v) {
        const sid_osc* src = &o[v ? v - 1 : 2];
        if ((o[v].control & SID_CTRL_SYNC) && src->msb_rose) {
            sid_osc_sync(&o[v], src);
        }
        sid_env_advance(&s->env[v], cycles);
    }
    if (s->written) {
        /* Pulse comparisons latched by writes (sid_osc_write_pw/_control) last until
         * after the step that follows them. */
        o[0].pulse_held = o[1].pulse_held = o[2].pulse_held = 0;
        s->written = 0;
    }
    /* The output feeds the voice DAC, which leaks it even with the envelope at 0, and
     * the noise write-back (a shift's after a control write takes the old waveform's
     * last output). With no waveform selected it is 0; reSIDfp holds its floating DAC
     * value instead, which SIDOU does not keep (see read_osc3). */
    o[0].out = sid_osc_output(&o[0], sid_osc_acc_last(&o[2]));
    o[1].out = sid_osc_output(&o[1], o[0].acc);
    o[2].out = sid_osc_output(&o[2], o[1].acc);
}


static inline float voice_out(const sid* s, int v) {
    return s->dac->wave[s->osc[v].out] * s->dac->env[s->env[v].counter];
}

/* Deferred noise shifts repeat with the register's period (2^23 - 1): fold the count
 * before it can wrap (after about 19 hours at the top pitch). A call adds at most one
 * rise per 16 cycles (2^28 for sid_clock's largest), well inside the 2^31 left. */
static inline void fold_owed_inline(sid* s) {
    for (int v = 0; v < 3; ++v) {
        if (s->osc[v].owed >= 0x80000000u) {
            s->osc[v].owed -= 0x7fffffu * 0x100u;
        }
    }
}

/* Out of line, so it does not crowd sid_render's loop. */
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

void sid_render(sid* s, int16_t* out, uint32_t frames) {
    fold_owed(s);
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
        float o = y * s->output_gain;
        o += o >= 0.0f ? 0.5f : -0.5f;
        if (o > 32767.0f) {
            o = 32767.0f;
        } else if (o < -32768.0f) {
            o = -32768.0f;
        }
        out[i] = (int16_t)o;
    }
}
