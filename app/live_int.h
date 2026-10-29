// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* live's own, shared by its two halves (not for other files): live.c, the input's and the tick's
 * (the notes, the knobs, the arpeggio's steps, the registers' writers), built at the app's speed;
 * live_setup.c, what runs once a turn, a load or a look (the presets, the parameters, the sound),
 * built for size. */
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

/* a happened before b; stamps wrap. */
static inline int older(uint32_t a, uint32_t b) {
    return (int32_t)(a - b) < 0;
}

/* The registers (live.c): as their names say, each written as it is now. */
void li_write_res_filt(live* l);
void li_write_mode_vol(live* l);
void li_write_envelopes(live* l);
void li_write_controls(live* l);
void li_write_gate_reset(live* l);
/* The character's settings, LIVE_REG_RESET(model), the gate reset. */
void li_reset_chip(live* l);
/* Every register, envelopes before gates. */
void li_write_all(live* l);
/* What the controllers, the vibrato and the LFOs move, written when it changed. */
void li_update_freq(live* l, int v);
void li_update_pws(live* l);
/* The knobs' setters: the field and what it moves. */
void li_set_cutoff(live* l, uint16_t value);
void li_set_resonance(live* l, uint8_t value);
void li_set_pulse_width(live* l, uint16_t value);
/* Voice v's note let go. */
void li_release(live* l, int v);
/* The arpeggio (live.c): the step's length, the voice to the cycle's note, the note that sounds
 * found again after the cycle changed, a note into the held ones. */
void li_arp_rewind(live* l);
void li_arp_sound(live* l);
void li_arp_resync(live* l);
void li_arp_hold(live* l, uint8_t note);
/* The chip's profile and model asked for, the profile for a model, a model at once. */
void li_request_profile(live* l, uint32_t profile);
uint32_t li_model_profile(const live* l, sid_model model);
void li_set_model(live* l, sid_model model);

#endif
