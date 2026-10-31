// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FX_ADAA_H
#define SIDOU_FX_ADAA_H

#ifdef __cplusplus
extern "C" {
#endif

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
