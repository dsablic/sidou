// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_LIVE_INT_H
#define SIDOU_LIVE_INT_H

#include "live.h"

#define CTRL_GATE 0x01u
#define CTRL_SYNC 0x02u
#define CTRL_RING 0x04u
#define CTRL_WAVEFORM 0xf0u
#define LFO_CENTRE 16384u

static inline void put(live* l, uint8_t reg, uint8_t value) {
    l->sink(l->sink_ctx, reg, value);
}

static inline uint8_t voice_count(const live* l) {
    return l->mono_sync ? 1 : LIVE_VOICES;
}

static inline int older(uint32_t a, uint32_t b) {
    return (int32_t)(a - b) < 0;
}

void li_write_res_filt(live* l);
void li_write_mode_vol(live* l);
void li_write_envelopes(live* l);
void li_write_controls(live* l);
void li_write_gate_reset(live* l);
void li_reset_chip(live* l);
void li_write_all(live* l);
void li_update_freq(live* l, int v);
void li_update_pws(live* l);
void li_update_cutoff(live* l);
void li_set_cutoff(live* l, uint16_t value);
void li_set_resonance(live* l, uint8_t value);
void li_set_pulse_width(live* l, uint16_t value);
void li_release(live* l, int v);
void li_arp_rewind(live* l);
void li_arp_sound(live* l);
void li_arp_resync(live* l);
void li_arp_hold(live* l, uint8_t note);
void li_request_profile(live* l, uint32_t profile);
uint32_t li_model_profile(const live* l, sid_model model);
void li_set_model(live* l, sid_model model);

#endif
