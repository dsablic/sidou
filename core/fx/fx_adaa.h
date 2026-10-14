// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_ADAA_H
#define SIDOU_FX_ADAA_H

/* First-order antiderivative anti-aliasing (ADAA) for the drives' waveshapers: instead of
 * f(u), output the mean of f over the segment from the previous input p to this one,
 * (F(u) - F(p)) / (u - p) (Parker, Zavalishin and Le Bivic, DAFx-16; Bilbao, Esqueda,
 * Parker and Valimaki, IEEE SPL 24(7), 2017).
 *
 * Every shaper here is piecewise polynomial, so each piece's mean has a closed form: on
 * one piece there is no division and nothing is ill-conditioned. A segment across a
 * breakpoint is split there and the pieces' means are weighted by their lengths (nothing
 * cancels; the length is not 0, as the ends are on different pieces).
 *
 * Two ways to use a clip:
 * - the residual r(u) = clip(u) - u, with u added back undelayed (tape): small signals stay
 *   flat to Nyquist, but the half-sample lag of the distortion against u means that far
 *   past the knee the two no longer cancel: the output approaches 1 + (u - p) / 2, which a
 *   high-gain drive at high frequencies turns into garbage;
 * - the whole clip (overdrive, fuzz): the output is the mean of a bounded function, so it
 *   stays bounded at any gain, at the cost of ADAA's own (1 + z^-1) / 2 droop on small
 *   signals (-1.6 dB at 8 kHz, -7.7 dB at 16 kHz), which the drives' pre-filters and
 *   gain make moot. */

#ifdef __cplusplus
extern "C" {
#endif

/* ---- soft clip: c(u) = u - (4/27) u^3 for |u| <= 1.5, +-1 beyond (sidou_soft_clip) ---- */

#define SID_FX_ADAA_SOFT_KNEE 1.5f

static inline float sid_fx_adaa_soft_in(float a, float b) {
    return (-1.0f / 27.0f) * (a + b) * (a * a + b * b);
}

static inline float sid_fx_adaa_soft_hi(float a, float b) {
    return 1.0f - 0.5f * (a + b);
}

static inline float sid_fx_adaa_soft_lo(float a, float b) {
    return -1.0f - 0.5f * (a + b);
}

/* Mean of r(u) = c(u) - u between p and u. */
static inline float sid_fx_adaa_soft(float u, float p) {
    const float k = SID_FX_ADAA_SOFT_KNEE;
    if (u <= k && u >= -k && p <= k && p >= -k) {
        return sid_fx_adaa_soft_in(u, p);
    }
    if (u > k && p > k) {
        return sid_fx_adaa_soft_hi(u, p);
    }
    if (u < -k && p < -k) {
        return sid_fx_adaa_soft_lo(u, p);
    }
    float lo = u < p ? u : p;
    float hi = u < p ? p : u;
    float a = lo;
    float sum = 0.0f;
    if (a < -k) {
        sum += (-k - a) * sid_fx_adaa_soft_lo(a, -k);
        a = -k;
    }
    float b = hi < k ? hi : k;
    sum += (b - a) * sid_fx_adaa_soft_in(a, b);
    if (hi > k) {
        sum += (hi - k) * sid_fx_adaa_soft_hi(k, hi);
    }
    return sum / (hi - lo);
}

/* Mean of c(u) itself between p and u. */
static inline float sid_fx_adaa_soft_clip(float u, float p) {
    const float k = SID_FX_ADAA_SOFT_KNEE;
    if (u <= k && u >= -k && p <= k && p >= -k) {
        return 0.5f * (u + p) + sid_fx_adaa_soft_in(u, p);
    }
    if (u > k && p > k) {
        return 1.0f;
    }
    if (u < -k && p < -k) {
        return -1.0f;
    }
    float lo = u < p ? u : p;
    float hi = u < p ? p : u;
    float a = lo;
    float sum = 0.0f;
    if (a < -k) {
        sum -= -k - a;
        a = -k;
    }
    float b = hi < k ? hi : k;
    sum += (b - a) * (0.5f * (a + b) + sid_fx_adaa_soft_in(a, b));
    if (hi > k) {
        sum += hi - k;
    }
    return sum / (hi - lo);
}

/* ---- hard clip: h(u) = clamp(u, -1, 1) ---- */

/* Mean of h(u) itself between p and u: u inside, +-1 beyond. */
static inline float sid_fx_adaa_hard_clip(float u, float p) {
    if (u <= 1.0f && u >= -1.0f && p <= 1.0f && p >= -1.0f) {
        return 0.5f * (u + p);
    }
    if (u > 1.0f && p > 1.0f) {
        return 1.0f;
    }
    if (u < -1.0f && p < -1.0f) {
        return -1.0f;
    }
    float lo = u < p ? u : p;
    float hi = u < p ? p : u;
    float a = lo > -1.0f ? lo : -1.0f;
    float b = hi < 1.0f ? hi : 1.0f;
    float sum = (b - a) * 0.5f * (a + b);
    if (lo < -1.0f) {
        sum -= -1.0f - lo;
    }
    if (hi > 1.0f) {
        sum += hi - 1.0f;
    }
    return sum / (hi - lo);
}


/* ---- rectified hard clip: |h(u)|, the octave-up of a fuzz ---- */

/* Mean of |h(u)| between p and u: pieces 1 (u < -1), -u, u, 1 (u > 1). Used directly (no
 * residual: it is all distortion). */
static inline float sid_fx_adaa_rect(float u, float p) {
    float lo = u < p ? u : p;
    float hi = u < p ? p : u;
    if (lo >= 0.0f && hi <= 1.0f) {
        return 0.5f * (lo + hi);
    }
    if (hi <= 0.0f && lo >= -1.0f) {
        return -0.5f * (lo + hi);
    }
    if (lo >= 1.0f || hi <= -1.0f) {
        return 1.0f;
    }
    /* Split at -1, 0 and 1. */
    float sum = 0.0f;
    float a = lo;
    if (a < -1.0f) {
        sum += -1.0f - a;
        a = -1.0f;
    }
    if (a < 0.0f) {
        float b = hi < 0.0f ? hi : 0.0f;
        sum += (b - a) * (-0.5f * (a + b));
        a = b;
    }
    if (a < hi && a < 1.0f) {
        float b = hi < 1.0f ? hi : 1.0f;
        sum += (b - a) * (0.5f * (a + b));
        a = b;
    }
    if (hi > 1.0f) {
        sum += hi - a;
    }
    float d = hi - lo;
    if (d < 1e-6f) {
        float m = 0.5f * (lo + hi);
        m = m < 0.0f ? -m : m;
        return m > 1.0f ? 1.0f : m;
    }
    return sum / d;
}

#ifdef __cplusplus
}
#endif

#endif
