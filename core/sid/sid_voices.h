// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#if !defined(SID_VOICES_T) || !defined(SID_VOICES_CLOCK) || !defined(SID_VOICES_OSC3)
#error "sid_voices.h: define SID_VOICES_T, SID_VOICES_CLOCK and SID_VOICES_OSC3 first"
#endif

#include <stdint.h>

#include "sid.h"

static inline void SID_VOICES_CLOCK(SID_VOICES_T* s, uint32_t cycles) {
    s->vf.osc3_held = 0;
    if (s->vf.control_age < 2u) {
        s->vf.control_age =
            (uint8_t)(s->vf.control_age + cycles > 2u ? 2u : s->vf.control_age + cycles);
    }
    sid_osc* o = s->osc;
    uint32_t acc2 = o[2].acc;
    o[2].src_mask = 0xffffffu >> o[1].pull_low;
    o[1].src_mask = 0xffffffu >> o[0].pull_low;
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
    if (s->vf.written) {
        o[0].pulse_held = o[1].pulse_held = o[2].pulse_held = 0;
        s->vf.written = 0;
    }
    o[0].out = sid_osc_output(&o[0], sid_osc_acc_last(&o[2]));
    o[1].out = sid_osc_output(&o[1], o[0].acc);
    o[2].out = sid_osc_output(&o[2], o[1].acc);
}

static uint8_t SID_VOICES_OSC3(const SID_VOICES_T* s) {
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
        uint32_t ramp = s->vf.control_age == 1
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
