// SPDX-FileCopyrightText: 2011-2025 Leandro Nini <drfiemost@users.sourceforge.net>
// SPDX-FileCopyrightText: 2007-2010 Antti Lankila
// SPDX-FileCopyrightText: 2004,2010 Dag Lem <resid@nimrod.no>
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-2.0-or-later
/* A port of reSIDfp's Dac (libresidfp c1791383, Dac.cpp) to float, and how SID.cpp and
 * FilterModelConfig6581 turn it into a voice's output. */
#include "sid_dac.h"

/* reSIDfp's cubic saturation of the 6581's waveform DAC (Dac::getOutput). */
#define SAT_GAIN 1.1f
#define SAT 1.1f
/* The waveform DAC input reSIDfp takes as the voice's zero (SID::setChipModel). */
#define ZERO_6581 0x380u
#define ZERO_8580 0x800u
/* The 6581's voice DC that follows the envelope DAC (FilterModelConfig6581: 0.2143 V
 * per unit), over its voice voltage range (1.5 V). The 8580's is constant, and the
 * output's DC blocker removes it. */
#define VOICE_DC_6581 (0.2143f / 1.5f)

/* Dac::kinkedDac: the weight of each bit of a `bits`-bit R-2R ladder, unterminated (the
 * 6581) or terminated by 2R (the 8580), normalised to sum to 1. */
static void ladder(float* dac, uint32_t bits, float kink, int terminated) {
    const float r_infinity = 1e6f;
    const float r = 1.0f;
    const float r2 = kink * r;
    float vsum = 0.0f;
    for (uint32_t set_bit = 0; set_bit < bits; ++set_bit) {
        float vn = 1.0f;
        float rn = terminated ? r2 : r_infinity;
        uint32_t bit;
        /* The ladder's "tail" resistance, by repeated parallel substitution. */
        for (bit = 0; bit < set_bit; ++bit) {
            rn = rn == r_infinity ? r + r2 : r + (r2 * rn) / (r2 + rn);
        }
        /* Source transformation for the bit's voltage. */
        if (rn == r_infinity) {
            rn = r2;
        } else {
            rn = (r2 * rn) / (r2 + rn);
            vn = vn * rn / r2;
        }
        /* The output voltage, by repeated source transformation from the tail. */
        for (++bit; bit < bits; ++bit) {
            rn += r;
            float i = vn / rn;
            rn = (r2 * rn) / (r2 + rn);
            vn = rn * i;
        }
        dac[set_bit] = vn;
        vsum += vn;
    }
    for (uint32_t i = 0; i < bits; ++i) {
        dac[i] /= vsum;
    }
}

/* Dac::getOutput for every input: an on bit adds its weight, an off bit its leakage.
 * Built up a bit at a time: out[2^b + j] = out[j] + (1 - leakage) w[b]. */
static void outputs(float* out, const float* w, uint32_t bits, float leakage) {
    float base = 0.0f;
    for (uint32_t i = 0; i < bits; ++i) {
        base += w[i] * leakage;
    }
    out[0] = base;
    for (uint32_t b = 0; b < bits; ++b) {
        float step = w[b] - w[b] * leakage;
        for (uint32_t j = 0; j < (1u << b); ++j) {
            out[(1u << b) + j] = out[j] + step;
        }
    }
}

static float saturate(float v) {
    return SAT_GAIN * v + (1.0f - SAT_GAIN) * SAT * v * v * v;
}

void sid_dac_job_start(sid_dac_job* j, sid_dac* d, int is6581, float kink, float leakage) {
    j->d = d;
    j->kink = kink;
    j->leakage = leakage;
    j->zero = 0.0f;
    j->is6581 = (uint8_t)(is6581 != 0);
    j->step = 0;
}

/* The waveform DAC entries a finishing step takes (steps 1.. of SID_DAC_BUILD_STEPS). */
#define FINISH_SHARE ((4096u + SID_DAC_BUILD_STEPS - 2u) / (SID_DAC_BUILD_STEPS - 1u))

int sid_dac_job_step(sid_dac_job* j) {
    sid_dac* d = j->d;
    const int is6581 = j->is6581;
    if (j->step >= SID_DAC_BUILD_STEPS) {
        return 1;
    }
    if (j->step == 0u) {
        /* The ladders and the raw outputs; the voice's zero, as the 6581's saturation leaves
         * it (the same value the finished table subtracts from every entry). */
        float w[12];
        ladder(w, 12, j->kink, !is6581);
        outputs(d->wave, w, 12, j->leakage);
        float z = d->wave[is6581 ? ZERO_6581 : ZERO_8580];
        j->zero = is6581 ? saturate(z) : z;
        ladder(w, 8, j->kink, !is6581);
        outputs(d->env, w, 8, j->leakage);
    } else {
        /* Saturated (the 6581), twice reSIDfp's scale, the voice DC added (riding on the
         * envelope DAC). */
        const float zero = j->zero;
        const float dc = is6581 ? VOICE_DC_6581 : 0.0f;
        uint32_t from = (j->step - 1u) * FINISH_SHARE;
        uint32_t to = from + FINISH_SHARE < 4096u ? from + FINISH_SHARE : 4096u;
        for (uint32_t i = from; i < to; ++i) {
            float v = is6581 ? saturate(d->wave[i]) : d->wave[i];
            d->wave[i] = 2.0f * (v - zero + dc);
        }
    }
    j->step++;
    return j->step >= SID_DAC_BUILD_STEPS;
}

void sid_dac_build(sid_dac* d, int is6581, float kink, float leakage) {
    sid_dac_job j;
    sid_dac_job_start(&j, d, is6581, kink, leakage);
    while (!sid_dac_job_step(&j)) {
    }
}

/* The shared default tables: 0 unbuilt, 1 being built, 2 ready. On hosts, tests build
 * chips from several threads; the device builds them in its main loop only, and its
 * compiler has no inline atomics. */
static sid_dac shared[2];
static int shared_state[2];

static float default_kink(int is6581) {
    return is6581 ? SID_DAC_KINK_6581 : SID_DAC_KINK_8580;
}

static float default_leakage(int is6581) {
    return is6581 ? SID_DAC_LEAKAGE_6581 : SID_DAC_LEAKAGE_8580;
}

#ifdef __PI32V2__
/* One context, the main loop: a table can be built a step at a time (sid_dac_default_step)
 * and finished at once by sid_dac_default. */
static sid_dac_job shared_job[2];

int sid_dac_default_ready(int is6581) {
    return shared_state[is6581 ? 0 : 1] == 2;
}

int sid_dac_default_step(int is6581) {
    uint32_t m = is6581 ? 0u : 1u;
    if (shared_state[m] == 2) {
        return 1;
    }
    if (shared_state[m] == 0) {
        sid_dac_job_start(&shared_job[m], &shared[m], is6581, default_kink(is6581),
                          default_leakage(is6581));
        shared_state[m] = 1;
    }
    if (!sid_dac_job_step(&shared_job[m])) {
        return 0;
    }
    shared_state[m] = 2;
    return 1;
}

const sid_dac* sid_dac_default(int is6581) {
    while (!sid_dac_default_step(is6581)) {
    }
    return &shared[is6581 ? 0 : 1];
}
#else
static int claim(int* s) {
    int expected = 0;
    return __atomic_compare_exchange_n(s, &expected, 1, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE);
}
static int ready(const int* s) { return __atomic_load_n(s, __ATOMIC_ACQUIRE) == 2; }
static void publish(int* s) { __atomic_store_n(s, 2, __ATOMIC_RELEASE); }

const sid_dac* sid_dac_default(int is6581) {
    uint32_t m = is6581 ? 0u : 1u;
    if (claim(&shared_state[m])) {
        sid_dac_build(&shared[m], is6581, default_kink(is6581), default_leakage(is6581));
        publish(&shared_state[m]);
    }
    while (!ready(&shared_state[m])) {
        /* another thread is building it */
    }
    return &shared[m];
}

/* Hosts build a table at once: the step is the whole of it. */
int sid_dac_default_ready(int is6581) {
    return ready(&shared_state[is6581 ? 0 : 1]);
}

int sid_dac_default_step(int is6581) {
    (void)sid_dac_default(is6581);
    return 1;
}
#endif
