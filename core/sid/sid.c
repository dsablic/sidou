// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid.h"

#include <stddef.h>
#include <string.h>

#ifdef __PI32V2__
__attribute__((section(".data.sid_noise_wb")))
#endif
const uint32_t SID_NOISE_WB[2][16] = {SID_WB_ROW(SID_WB_LO), SID_WB_ROW(SID_WB_HI)};

/* The noise tables the audio path reads, built once before anything runs instead of loaded
 * from the image: on the device the boot calls sid_noise_tables_init in the LCD's reset wait,
 * before the audio starts (platform/fm1/main.c, while_lcd_resets: the boot's timeline stays
 * as it was), and each bench's main after zeroing memory; on the host it is a constructor,
 * run before main. On the device they lie in POOL (SRAM, as ram_audio.rs requires of
 * what the audio path reads), not in .data's copy in flash and in the RAM region.
 * SID_NOISE_JUMP[k][i] is the register after 2^k plain shifts of bit i alone: row 0 is one
 * shift, each further row the one before applied to itself (sid_noise_jump in sid_osc.h).
 * SID_NOISE_WB_BYTE[h] is SID_NOISE_WB's two halves for the output's bits 4..11 at once.
 * unit/sid_noise.rs checks both against sidou-tablegen's matrices and SID_NOISE_WB. */
#ifdef __PI32V2__
__attribute__((section(".pool.sid_noise_jump")))
#endif
uint32_t SID_NOISE_JUMP[23][23];
#ifdef __PI32V2__
__attribute__((section(".pool.sid_noise_wb_byte")))
#endif
uint32_t SID_NOISE_WB_BYTE[256];

/* Each compiled row's index row in SID_PD (gen/pulldown.h): sid_osc_update_mode reads it in
 * the audio path, so RAM on the device. */
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

#define OUTPUT_GAIN (0.1f * 32767.0f) /* leaves headroom for the 6581 DC offset and filter peaks */
/* reSIDfp's output level relative to OUTPUT_GAIN: the RMS of one unfiltered voice at
 * volume 15 (sawtooth, triangle and 50% pulse, at 0x08d6 and 0x1d45), measured
 * through its mixer and output stages: 3.37 dB louder on the 6581, 1.05 dB on the 8580
 * (reference.rs asserts the match). At this level three voices at maximum resonance
 * clip the 8580 by up to about 1.1 dB, as they clip reSIDfp; the 6581 keeps 0.5 dB of
 * headroom (unit/sid.rs, three_voices_at_maximum_resonance_headroom). */
#define LEVEL_6581 1.4736f
#define LEVEL_8580 1.1288f
#define DC_BLOCK_HZ 16.0f
/* The thump at a note's start (sid_write): the peak a DC rising linearly over the attack (2 ms
 * to 8 s, SID_ENV_RATE_POS's rates) reaches through the 16 Hz DC blocker, tau / T (1 - e^-T/tau)
 * with tau = 1 / (2 pi 16 Hz), so a slow attack hardly thumps. Read where register writes are
 * applied (the audio interrupt's drain): RAM on the device (emu-gate tests/ram_audio.rs). */
/* A note's thump against the volume write's: the voice DC's rise is the louder transient (at
 * thump 1, two voice DAC units for a fast attack's note from silence, where a saw at full
 * envelope swings one either way). Neither needs headroom in the chip: the output stage
 * (app/thump_out.h) plays both after the effect slots, within the room the output leaves. */
#define THUMP_GATE 2.0f
#ifdef __PI32V2__
__attribute__((section(".data.sid_thump_attack")))
#endif
static const float THUMP_ATTACK[16] = {0.9059f, 0.6871f, 0.4972f, 0.3773f, 0.2560f, 0.1770f,
                                       0.1461f, 0.1243f, 0.0995f, 0.0398f, 0.0199f, 0.0124f,
                                       0.0099f, 0.0033f, 0.0020f, 0.0012f};

/* The 6581's filter and output path, fitted to reSIDfp's 6581 at 44.1 kHz (M1b Task 6:
 * filter sweeps at small and full scale and with three loud voices, cutoffs and damping
 * measured from reSIDfp with one to three voices at two envelope levels, passband gains
 * and the volume register's DC steps; docs/plans/2026-10-14-m1b-sound-accuracy.md).
 * The non-linearity (sid_filter.h, sid_nonlin), at amount 1: */
#define NL_LEVEL 0.055f     /* g per unit |hp|, at 44.1 kHz */
#define NL_LEVEL_REF 0.13875f
#define NL_LOAD 0.13125f
#define NL_LOAD_REF 1.7375f
#define NL_FLOOR_HZ 240.7f
/* reSIDfp's 6581 filter outputs against its unfiltered voices: the low-pass and the
 * high-pass inverted (inverting op-amps), the band-pass not; each mode's passband gain;
 * and its op-amps' finite gain A, which makes the integrators leak: the band-pass carries
 * LEAK (1 / A) of the low-pass, the high-pass 2 LEAK of the band-pass and LEAK^2 of the
 * low-pass (a state-variable filter with lossy integrators is a lossless one, retuned,
 * with these outputs; the retuning is in the fitted tables). */
#define MIX_GAIN_LP 0.6875f
#define MIX_GAIN_BP SID_MIX_GAIN_BP
#define MIX_GAIN_HP 0.5f
#define MIX_LEAK (1.0f / 3.375f)
/* The DC each input routed into the filter (a voice or EXT IN) brings at the operating
 * point of reSIDfp's summer: the filter's load. It moves the cutoff only (it enters the
 * load term, not the signal: as a step through a resonant low-pass it would ring past
 * full scale where reSIDfp's saturating op-amps do not). */
#define DC_FILTER_INPUT 1.15625f
/* The external filter's low-pass with reSIDfp's resampling after it: one pole, fitted to
 * reSIDfp's output at 44.1 kHz (13.2 kHz), not the external filter's 15.9 kHz nominal
 * pole, which leaves out the resampler's roll-off. */
#define OUT_LP_HZ 13154.0f
/* sid_clock's step. sid_osc_advance needs freq * cycles < 2^24, so at most 256.
 * Within that, the step only matters for hard sync: the restart is exact, but what the
 * synced voice did earlier in the step (its noise clocks, its MSB rise) is not
 * recomputed (see sid_osc_sync); 16 is no longer than sid_render's step at any PAL rate
 * up to 61.5 kHz, so sid_clock is never coarser than rendering. The loop's cost does not
 * matter: sid_clock is for tests and register-accurate seeking, not the audio path. */
#define CLOCK_STEP 16u

/* update_mix_bpn, update_filter and update_mix are always inline: sid_init (flash) and
 * sid_write (SRAM, the audio path) both call them, and no direct call may cross between the
 * two (platform/fm1/app.ld).
 * The 6581's band-pass mix: over the damping (sid_svf_tick_6581's bpn), so every write
 * that sets the damping or the mode sets it (sid_update_filter has the same). */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void update_mix_bpn(sid* s) {
    float bp = (s->mode_vol & 0x20u) ? 1.0f : 0.0f;
    s->mix_bpn = MIX_GAIN_BP * bp / s->svf.k;
}

/* The filter's coefficients and the band-pass mix (sid.h). */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void update_filter(sid* s) {
    sid_update_filter(s);
}

/* The 6581's mix of the filter's outputs and its load, for $17 and $18 (sid.h). */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void update_mix(sid* s) {
    if (s->model != SID_MODEL_6581) {
        return;
    }
    uint32_t routed = 0;
    for (uint32_t v = 0; v < 4u; ++v) {
        routed += (s->routing >> v) & 1u; /* the voices and EXT IN */
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

/* exp(-x) for x >= 0, without libm: (Taylor series of exp(-x / 16))^16. For sid_init's
 * output pole, x = 2 pi OUT_LP_HZ / fs: within 5e-6 relative from 22.05 kHz up (x < 3.75),
 * 3e-5 at 16 kHz, 0.3% at 8 kHz, and 27% low at sid_init's lowest rate, 4 kHz (x = 20.7),
 * where exp(-x) is 1e-9. out_c = 1 - exp_neg is within 5 ULP of exact at every rate
 * sid_init accepts. */
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

/* A cutoff in Hz, clamped to 30 Hz .. 0.45 fs, as the angle pi f / fs (the second filter
 * tables' entries, sid.h) and as the SVF's g, its tangent (the first's). */
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
    if (!(amount >= 0.0f)) { /* NaN too */
        amount = 0.0f;
    } else if (amount > 2.0f) {
        amount = 2.0f;
    }
    float fs = (float)s->sample_rate;
    *out = s->nl;
    out->level = amount * NL_LEVEL * (44100.0f / fs);
    out->level_off = -out->level * out->level_ref;
    out->load = amount * NL_LOAD;
    /* The lowest cutoff is the chip's, not the non-linearity's: it follows the cutoff at FC 0,
     * blended as sid_update_filter blends it, against the default curve setting's (1 exactly
     * at the default), and the amount scales it only up to 1 (0: none, the filter linear). */
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

/* A curve setting's cutoff and damping tables: the cutoff (clamped to 30 Hz .. 0.45 fs) as
 * the SVF's g (the first tables) or as its angle (the second's). */
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

/* The chip's tables for curve and toward (sid.h). For one curve setting (sid_init's, and
 * sid_set_filter_curve's) one pass: each FC's lookup and angle once, for both tables. */
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
    if (!(t > 0.0f)) { /* NaN too */
        t = 0.0f;
    } else if (t > 1.0f) {
        t = 1.0f;
    }
    build_filter_tables(s, curve, toward);
    s->filter_t = t;
    return 0;
}

/* sid_init zeroes the chip but its filter tables, which build_filter_tables writes whole:
 * they are half the struct, and the device's memset goes a byte at a time. */
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
        s->osc[v].tri_saw = 0x555u; /* reSIDfp's power-on latch; reset keeps it */
        s->env[v].counter = SID_ENV_POWER_ON;
        sid_osc_reset(&s->osc[v]);
        sid_env_reset_settled(&s->env[v]); /* zeroed above: nothing to settle */
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

static uint8_t read_osc3(const sid* s);

/* A thump step (sid.h): from the next block's first sample, or from this one's sample the
 * chip has rendered up to. Always inline: sid_write runs in SRAM. */
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
            if (s->thump != 0.0f && (v & SID_CTRL_GATE) && !e->gate) {
                /* The thump at a note's start (sid.h): the voice DC's rise as the envelope
                 * attacks from where it is to the top, the volume scaling it, as large as
                 * the DC blocker lets a ramp of the attack's length peak (THUMP_ATTACK). A DC
                 * takes the voice's path: straight to the output, or through the filter's
                 * low-pass if it is routed there (inverted on the 6581, sid.h mix_lp; the
                 * band- and high-pass block it). Left for the output stage as the volume
                 * write's step is (below). */
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
    /* $15-$17 only mark the filter: sid_render_tap recomputes its coefficients once, from
     * the registers as the block's writes left them (sid.h, sid_update_filter). */
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
            /* The thump (sid.h): a DC the volume scales, which the output's DC blocker
             * removes again, so all it ever does is step the output as the volume changes:
             * the blocker's own response to it, which the output stage plays (sid.h
             * thump_pending; the render pays nothing). */
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

/* OSC3 as reSIDfp latches it. On the 6581 that is the waveform output of the last
 * cycle. On the 8580 the triangle and sawtooth reach it a cycle late: it combines the
 * previous cycle's triangle/saw with the pulse and noise of the last one (whose pulse
 * comparison is itself a cycle late on both models). Noise combined with other
 * waveforms runs cycle by cycle and keeps its OSC3. Otherwise the previous cycle is
 * reconstructed from the current state. A read right after a write to voice 2 or 3,
 * before any cycle, returns the value latched before the write (sid_write). A read a
 * cycle or two later is wrong after:
 * - a frequency or pulse width write (the old values drove those cycles; the pulse
 *   comparison sid_write latches, pulse_held, is not used here);
 * - a control write: the pulse comparison from before a TEST change (the 8580's
 *   triangle/saw latch, tri_saw, is exact: it holds the last triangle/saw value,
 *   0x555 from power-on, and is used for the one cycle after the write);
 * - a hard sync of voice 3 or voice 2 (with ring modulation): the previous cycle is
 *   rebuilt from the restarted accumulator, which has lost it (one read per sync on the
 *   6581, two on the 8580; register_reads.rs has both gaps as ignored tests).
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

/* render_8580's loop, in chunks of `every` samples with a tap after each (sid_render_tap;
 * sid_render's is one chunk and no tap): M0's linear filter and mix, unchanged. With
 * bleed_on (a constant: each model's render has a copy with it and one without, so a chip
 * with no bleed pays nothing for it) the routed voices also reach the output unfiltered
 * (sid.h). */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void render_loop(sid* s, int16_t* out, uint32_t frames, uint32_t every,
                               sid_tap tap, void* ctx, const int bleed_on) {
    /* In a local, as render_6581_body keeps it: nothing writes the chip during a render. */
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
            for (int v = 0; v < 3; ++v) {
                float x = voice_out(s, v);
                if (s->routing & (1u << v)) {
                    filt_in += x;
                } else if (!(v == 2 && (s->mode_vol & 0x80u))) {
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
            *out = (int16_t)o;
        }
        if (tap) {
            tap(ctx, s);
        }
    }
}

/* One function per model, so neither model's code depends on the other's, and the model
 * test stays out of the per-sample loop. The 6581's runs its non-linear filter
 * (sid_nl_tick) and its output path (sid.h), and keeps what it reads every sample in
 * locals: nothing writes the chip during a render (sid_write runs between them), and the
 * compiler cannot know that through the pointer. The tap (sid_render_tap) sees its voices;
 * its filter's state is stored back once, at the end. bleed_on as render_loop's. */
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
            for (int v = 0; v < 3; ++v) {
                float x = voice_out(s, v);
                if (s->routing & (1u << v)) {
                    filt_in += x;
                } else if (!(v == 2 && (s->mode_vol & 0x80u))) {
                    direct += x;
                }
            }
            if (bleed_on) {
                direct += bleed * filt_in;
            }
            sid_nl_tick(&ic1, &ic2, &lp, &bp, &hp, &bpn, filt_in, &c);
            float filtered = mix_lp * lp + mix_bpn * bpn + mix_bp * bp + mix_hp * hp;
            /* The output low-pass. At volume 0 it decays towards 0 (sid_svf_flush's reason). */
            float x = out_lp + (vol_c * (direct + filtered) - out_c * out_lp);
            if (x < 1e-15f && x > -1e-15f) {
                x = 0.0f;
            }
            out_lp = x;

            /* render_8580's DC blocker and rounding. */
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

/* The grit (sid.h) on a render's samples, a moving towards the stage's by SID_GRIT_SLEW a
 * sample until it is there: the same path however the renders split it. */
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
    s->thump_rendered += frames; /* where a thump step written next starts (sid.h) */
    if (s->filter_dirty) {
        s->filter_dirty = 0;
        update_filter(s);
    }
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
