// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_FILTER_H
#define SIDOU_SID_FILTER_H

#include <stdint.h>

#include "sid_defs.h"
#include "sid_math.h"

/* Cytomic trapezoidal SVF. The 8580 runs it linear (sid_svf_tick); the 6581 through
 * sid_svf_tick_6581, which moves its cutoff with the signal. */
typedef struct {
    float ic1, ic2;
    float lp, bp, hp;
    float a1, a2, a3, k;
    float g;   /* the cutoff from the tables, tan(pi f / fs); the 6581 moves away from it */
    /* The 6581's: the load_ref less the DC load (sid_svf_set_6581), and the band-pass
     * at the level it has unloaded, times the table's damping (sid_nl_tick). */
    float load_at;
    float bpn;
} sid_svf;

/* The 6581 filter's non-linearity (M1b Task 6), a phenomenological fit to reSIDfp's
 * 6581, whose integrators are MOSFET resistors (VCRs) and op-amps working around an
 * operating point:
 * - in the threshold region (FC up to about 600, where the cutoff DAC barely turns the
 *   VCR on) the cutoff rises with the signal's swing, nearly doubling at full scale;
 * - the filter's DC load (the voices routed into it carry a large DC, more with louder
 *   envelopes) lowers the cutoff everywhere and widens it, to half the cutoff and twice
 *   the damping with three loud voices, while the band-pass keeps its peak level.
 * Per sample, with g0 and k the tables' cutoff and damping, and dc the DC the inputs
 * routed into the filter put on it (sid.c, DC_FILTER_INPUT):
 *   m  = clamp(1 + load (load_ref - dc - lp state), SID_NL_M_MIN, SID_NL_M_MAX)
 *   g  = max(g0 m + level (|hp| - level_ref), floor)
 * and the coefficients follow from one Newton step for 1 / (1 + g (g + k)) from the
 * table's a1 (a10), not a per-sample divide. The tick computes the same, rearranged so
 * what is fixed for a render is computed once (sid_nl_coefs):
 *   u  = clamp(A - B lp state, SID_NL_M_MIN g0 - lo, SID_NL_M_MAX g0 - lo)
 *   g  = max(u + level |hp|, floor)
 *   a1 = max(P - Q g (g + k), 0.1 a10)
 * with lo = level level_ref, A = g0 (1 + load (load_ref - dc)) - lo, B = g0 load,
 * P = a10 (2 - a10) and Q = a10^2. The step's error is the load-damping model, not an
 * approximation of it: the step underestimates a1, more the further the load pulls the
 * cutoff down, which raises the damping the filter runs at above the table's k as
 * reSIDfp's damping grows under the load (unit/sid_filter.rs pins it: an exact divide
 * would change the sound), and keeps it stable: any 0 < a1 < 1 / (1 + g^2) is. With
 * d = 1 + g (g + k) and d0 the table's, the step is a1 = a10 t, t = 2 - d / d0, which
 * underestimates 1 / d for any d; the clamp t >= 0.1 (at d > 1.9 d0) holds a1 at 0.1 a10,
 * which still underestimates while d < 10 d0, and overestimates beyond (a swing far past
 * any the voices reach: g would have to grow about threefold at the top of the table, more
 * below it, and SID_NL_M_MAX caps the load's part). SID_NL_M_MIN bounds how
 * far the load goes, as reSIDfp's saturating op-amps bound its output: lower, three
 * voices at full resonance ring past full scale. Each chip keeps its
 * own values (sid.h, sid_set_filter_nonlinearity), so a profile can blend them while the
 * chip plays. Fitted against reSIDfp at 44.1 kHz (M1b Task 6). */
typedef struct {
    float level;     /* g per unit of |hp| above level_ref */
    float level_ref; /* the swing the cutoff tables were fitted at */
    float level_off; /* -level level_ref */
    float load;      /* relative cutoff fall per unit of the low-pass state above load_ref */
    float load_ref;
    float floor;     /* the lowest g: reSIDfp's 6581 cutoff never goes below about 240 Hz */
} sid_nonlin;

#define SID_NL_M_MIN 0.35f
#define SID_NL_M_MAX 1.3f

void sid_svf_reset(sid_svf* f);
/* reSIDfp's filter as SIDOU's fits it (gen/filter.h, sidou-tablegen fit-filter), per
 * model and curve setting (the 6581's filter range and curve, the 8580's curve): the
 * cutoff in Hz for an FC value (0..2047), and the damping k = 1/Q for a resonance value
 * (0..15). Curve 0 is reSIDfp's default for the model; sid_filter_curves counts them (1
 * until M1c Task 3 adds its profiles), and a curve out of range reads curve 0. */
uint32_t sid_filter_curves(sid_model model);
float sid_cutoff_hz(sid_model model, uint32_t curve, uint32_t fc);

/* A curve's cutoffs read FC value after FC value: the codes are delta-coded in blocks of
 * 32 (gen/filter.h), so sid_cutoff_hz walks up to 31 deltas from its block's start, while
 * this reader keeps its place and the next FC value costs one step. Any other FC value
 * reads as sid_cutoff_hz does. The loops that fill a filter row (sid_init, the character
 * jobs) go through it. */
typedef struct {
    const uint8_t* run; /* the model's code stream */
    const uint16_t* at; /* the curve's block offsets in it */
    const uint8_t* p;   /* the code after the one read last */
    uint32_t fc;        /* the FC value read last; 0xffffffff for none */
    uint32_t code;      /* its code */
} sid_cutoff_seq;

void sid_cutoff_seq_init(sid_cutoff_seq* q, sid_model model, uint32_t curve);
/* The code of any FC value (fc < 2048), from its block's start: the reader's slow path. */
uint32_t sid_cutoff_seq_seek(sid_cutoff_seq* q, uint32_t fc);

/* A code's cutoff in Hz: float bits, the code's 16 bits above an 11-bit mantissa tail, the
 * exponent rebased by SID_CUTOFF_CODE_BIAS (gen/filter.h, which defines it the same). */
#define SID_CUTOFF_CODE_BIAS 126u

/* The cutoff of FC value fc (its low 11 bits) on the reader's curve. Inline: the row loops
 * keep the reader in registers, and the next FC value costs a byte and an add. */
static inline float sid_cutoff_seq_hz(sid_cutoff_seq* q, uint32_t fc) {
    uint32_t code;
    fc &= 2047u;
    if (fc == q->fc + 1u && (fc & 31u)) {
        const uint8_t* p = q->p;
        int32_t d = (int8_t)*p++;
        if (d == -128) {
            code = (uint32_t)p[0] << 8 | p[1];
            p += 2;
        } else {
            code = (q->code + (uint32_t)d) & 0xffffu;
        }
        q->p = p;
        q->fc = fc;
        q->code = code;
    } else {
        code = sid_cutoff_seq_seek(q, fc);
    }
    union {
        uint32_t bits;
        float hz;
    } value = {(code << 11) + (SID_CUTOFF_CODE_BIAS << 23)};
    return value.hz;
}
float sid_resonance_k(sid_model model, uint32_t curve, uint32_t res);

/* Inline: sid_write (the audio path, in SRAM on the device) and sid_init (in flash) both call
 * it, and a call between flash and SRAM is out of the pi32v2's direct call range
 * (platform/fm1/app.ld). */
static inline void sid_svf_set(sid_svf* f, float g, float k) {
    f->k = k;
    f->g = g;
    f->a1 = 1.0f / (1.0f + g * (g + k));
    f->a2 = g * f->a1;
    f->a3 = g * f->a2;
}

/* The 6581's coefficients: sid_svf_set's, and the DC load `dc` (sid_nonlin). Like
 * sid_svf_set, in the audio path's writes. */
static inline void sid_svf_set_6581(sid_svf* f, const sid_nonlin* n, float g, float k,
                                    float dc) {
    sid_svf_set(f, g, k);
    f->load_at = n->load_ref - dc;
}

/* On silence the state decays into subnormals, which may be slow or trap on the pi32v2
 * FPU. Flush explicitly rather than relying on an FPU flush-to-zero mode. */
static inline void sid_svf_flush(sid_svf* f) {
    if (f->ic1 < 1e-15f && f->ic1 > -1e-15f) {
        f->ic1 = 0.0f;
    }
    if (f->ic2 < 1e-15f && f->ic2 > -1e-15f) {
        f->ic2 = 0.0f;
    }
}

static inline void sid_svf_tick(sid_svf* f, float v0) {
    float v3 = v0 - f->ic2;
    float v1 = f->a1 * f->ic1 + f->a2 * v3;
    float v2 = f->ic2 + f->a2 * f->ic1 + f->a3 * v3;
    f->ic1 = 2.0f * v1 - f->ic1;
    f->ic2 = 2.0f * v2 - f->ic2;
    f->lp = v2;
    f->bp = v1;
    f->hp = v0 - f->k * v1 - v2;
    sid_svf_flush(f);
}

/* The 6581 tick's coefficients for a render (sid_nonlin above): what does not change
 * between register writes, from sid_svf_set_6581's (g0 = g, a10 = a1, k, load_at) and the
 * chip's non-linearity. */
typedef struct {
    float a, b;       /* u = a - b ic2 */
    float u_lo, u_hi; /* u's clamp: the load's part between SID_NL_M_MIN and SID_NL_M_MAX */
    float level, floor;
    float p, q;   /* a1 = p - q g (g + k): the Newton step from a10 */
    float a1_min; /* 0.1 a10 */
    float k;
} sid_nl_coefs;

static inline sid_nl_coefs sid_nl_coefs_of(const sid_svf* f, const sid_nonlin* n) {
    const float g0 = f->g, a10 = f->a1, off = n->level_off;
    sid_nl_coefs c;
    c.a = g0 * (1.0f + n->load * f->load_at) + off;
    c.b = g0 * n->load;
    c.u_lo = SID_NL_M_MIN * g0 + off;
    c.u_hi = SID_NL_M_MAX * g0 + off;
    c.level = n->level;
    c.floor = n->floor;
    c.p = a10 * (2.0f - a10);
    c.q = a10 * a10;
    c.a1_min = 0.1f * a10;
    c.k = f->k;
    return c;
}

/* The 6581's tick on its state, the integrators (ic1, ic2) and the outputs, with the
 * render's coefficients (sid_nl_coefs_of): sid_render keeps them all in locals for a
 * render, sid_svf_tick_6581 in the struct. It also sets bpn: the band-pass times the
 * damping the shifted coefficients have (over the table's damping: the mix divides by k),
 * which keeps the band-pass peak where reSIDfp's stays as the load widens the band.
 * Whatever a1 is, ic1 + g v3 = v1 / a1, so (ic1 - v1 + g (v3 - g v1)) / g is that damping
 * times v1; it costs the tick's one divide. */
static inline void sid_nl_tick(float* ic1, float* ic2, float* lp, float* bp, float* hp,
                               float* bpn, float v0, const sid_nl_coefs* c) {
    /* The clamps as fmax and fmin, which the pi32v2's FPU has as instructions (fm1-emu's
     * fm1-isa, fpu.rs): the same values as comparisons but for a tie of +0 and -0, which
     * returns either (no NaN reaches them; floor and a1_min are above 0 at any amount but 0,
     * and at 0 a g of 0 divides by zero below anyway). The host matches. In the emulator gate
     * they make the worst blocks 5.7 instructions a sample cheaper with one chip, 12.2 with
     * two (the bleed's render copy), and the default render 1.0 dearer; a fabs for the swing
     * made it 17 dearer (M1c Task 3). */
    float u = __builtin_fminf(__builtin_fmaxf(c->a - c->b * *ic2, c->u_lo), c->u_hi);
    float swing = *hp < 0.0f ? -*hp : *hp;
    float g = __builtin_fmaxf(u + c->level * swing, c->floor);
    float a1 = __builtin_fmaxf(c->p - c->q * (g * (g + c->k)), c->a1_min);
    float a2 = g * a1;
    float a3 = g * a2;
    float c1 = *ic1, c2 = *ic2;
    float v3 = v0 - c2;
    float v1 = a1 * c1 + a2 * v3;
    float v2 = c2 + a2 * c1 + a3 * v3;
    *bpn = (c1 - v1 + g * (v3 - g * v1)) / g;
    c1 = 2.0f * v1 - c1;
    c2 = 2.0f * v2 - c2;
    *lp = v2;
    *bp = v1;
    *hp = v0 - c->k * v1 - v2;
    /* sid_svf_flush's reason. */
    *ic1 = c1 < 1e-15f && c1 > -1e-15f ? 0.0f : c1;
    *ic2 = c2 < 1e-15f && c2 > -1e-15f ? 0.0f : c2;
}

static inline void sid_svf_tick_6581(sid_svf* f, const sid_nonlin* n, float v0) {
    const sid_nl_coefs c = sid_nl_coefs_of(f, n);
    sid_nl_tick(&f->ic1, &f->ic2, &f->lp, &f->bp, &f->hp, &f->bpn, v0, &c);
}

#endif
