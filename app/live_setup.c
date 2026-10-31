// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "live.h"
#include "live_dice.h"
#include "live_int.h"

#include "fx_live.h"

#include <stddef.h>
#include <string.h>

_Static_assert(LIVE_PARAM_BEND_RANGE == LIVE_PARAM_SLOT_END,
               "every slot field is a parameter, and the expression ones follow");
_Static_assert(LIVE_PARAM_BLEED - LIVE_PARAM_AGE + 2u == LIVE_CHAR_FIELDS,
               "a parameter per character control, in the fields' order");

__attribute__((noinline, unused)) static void macro_changed(live* l, uint32_t field);

#define EXPRESSION_DEFAULTS                                                                        \
    .bend_range = LIVE_BEND_RANGE_DEFAULT, .velocity_target = LIVE_VELOCITY_OFF,                   \
    .mod_target = LIVE_MOD_PULSE_WIDTH, .pressure_target = LIVE_PRESSURE_OFF,                      \
    .vibrato_rate = LIVE_VIBRATO_RATE_DEFAULT, .vibrato_depth = 0, .vibrato_delay = 0

static const live_preset presets[LIVE_PRESET_SLOTS] = {
    {
        .name = "SAW LEAD",
        .control = 0x20,
        .attack_decay = 0x09,
        .sustain_release = 0xa6,
        .pulse_width = 2048,
        .cutoff = 1200,
        .resonance = 6,
        .filter_voices = 0x07,
        .filter_mode = 0x10,
        EXPRESSION_DEFAULTS,
    },
    {
        .name = "PWM BASS",
        .control = 0x40,
        .attack_decay = 0x05,
        .sustain_release = 0x84,
        .pulse_width = 1024,
        .cutoff = 700,
        .resonance = 10,
        .filter_voices = 0x07,
        .filter_mode = 0x10,
        .pwm_depth = 768,
        .pwm_rate = 328,
        EXPRESSION_DEFAULTS,
    },
    {
        .name = "SYNC LEAD",
        .control = 0x22,
        .attack_decay = 0x0a,
        .sustain_release = 0xb8,
        .pulse_width = 2048,
        .cutoff = 1500,
        .resonance = 4,
        .filter_voices = 0x01,
        .filter_mode = 0x10,
        .mono_sync = 1,
        .sync_ratio_q8 = 704,
        EXPRESSION_DEFAULTS,
    },
    {
        .name = "NOISE PERC",
        .control = 0x80,
        .attack_decay = 0x08,
        .sustain_release = 0x08,
        .pulse_width = 2048,
        .cutoff = 1800,
        .resonance = 2,
        .filter_voices = 0x07,
        .filter_mode = 0x50,
        EXPRESSION_DEFAULTS,
    },
    {
        .name = "DICE",
        .control = 0x20,
        .attack_decay = 0x09,
        .sustain_release = 0xa6,
        .pulse_width = 2048,
        .cutoff = 1200,
        .resonance = 6,
        .filter_voices = 0x07,
        .filter_mode = 0x10,
        EXPRESSION_DEFAULTS,
    },
};

const live_preset* live_preset_get(uint8_t index) {
    return index < LIVE_PRESET_SLOTS ? &presets[index] : NULL;
}

static void load_preset(live* l, uint8_t index) {
    const live_preset* p = &presets[index];
    l->preset = index;
    l->loads++;
    l->cutoff = p->cutoff;
    l->resonance = p->resonance;
    l->pulse_width = p->pulse_width;
    l->control = p->control;
    l->attack_decay = p->attack_decay;
    l->sustain_release = p->sustain_release;
    l->filter_voices = p->filter_voices;
    l->filter_mode = p->filter_mode;
    if (!l->bend_range_rpn) {
        l->bend_range = p->bend_range;
    }
    l->velocity_target = p->velocity_target;
    if (l->mod_target != p->mod_target) {
        l->mod_wheel = 0;
    }
    l->mod_target = p->mod_target;
    l->pressure_target = p->pressure_target;
    l->vibrato_rate = p->vibrato_rate;
    l->vibrato_depth = p->vibrato_depth;
    l->vibrato_delay = p->vibrato_delay;
    l->sync_semis = 0;
    l->sync_q8 = p->sync_ratio_q8;
    l->sync_base_q8 = p->sync_ratio_q8;
    l->mono_sync = p->mono_sync;
    l->pwm_depth = p->pwm_depth;
    l->pwm_rate = p->pwm_rate;
    l->tune = 0;
    l->filter_lfo = 0;
    l->reso_link = 0;
    l->pwm_link = 0;
    l->lfo_phase = LFO_CENTRE;
    l->arp_on = 0;
    l->arp_rate = LIVE_ARP_RATE_DEFAULT;
    l->arp_order = LIVE_ARP_UP;
    l->arp_span = 1;
    l->arp_n = 0;
    l->arp_voice = LIVE_ARP_NO_VOICE;
    for (int v = 0; v < LIVE_VOICES; ++v) {
        if (l->voice[v].gate) {
            l->voice[v].gate = 0;
            l->voice[v].stamp = ++l->clock;
        }
    }
    l->user_sound = 0;
    live_sound_get(l, l->base);
    l->base[LAB_SOUND_BEND_RANGE] = p->bend_range;
    live_sound_set_base(l, l->base, p->name);
}

void live_init(live* l, live_sink_fn sink, void* sink_ctx, sid_model model) {
    l->sink = sink;
    l->sink_ctx = sink_ctx;
    l->prepare = NULL;
    l->prepare_ctx = NULL;
    l->preparing = 0;
    l->two_chips = 0;
    l->keys_chip = 0;
    l->other_model = (uint8_t)model;
    l->other_profile = (uint8_t)sid_chip_default(model);
    l->keys_mode = LIVE_KEYS_AUTO;
    l->theme = 0;
    l->ui_voice = 0;
    l->settings_dirty = 0;
    l->user_valid = 0;
    l->user_applied = 0;
    l->sounds_valid = 0;
    l->user_load = NULL;
    l->user_ctx = NULL;
    l->user_edit_slot = 0;
    l->model = model;
    l->model_req = model;
    l->octave = 0;
    l->master = LIVE_MASTER_MAX;
    l->volume = 100;
    l->cutoff_pos = 0;
    l->dice_rolls = 0;
    l->arp_pos = 0;
    l->arp_wait = 0;
    l->arp_clocks = 0;
    l->arp_clock_seen = 0;
    for (uint32_t i = 0; i < LIVE_PRESET_SLOTS; ++i) {
        l->edits[i].valid = 0;
    }
    l->clock = 0;
    for (int v = 0; v < LIVE_VOICES; ++v) {
        l->voice[v].note = LIVE_NO_NOTE;
        l->voice[v].gate = 0;
        l->voice[v].stamp = 0;
        l->voice[v].velocity = LIVE_EXPRESSION_MAX;
        l->voice[v].pressure = 0;
        l->voice[v].vibrato_phase = 0;
        l->voice[v].vibrato_wait = 0;
    }
    l->bend = LIVE_BEND_CENTRE;
    l->mod_wheel = 0;
    l->channel_pressure = 0;
    l->last_velocity = LIVE_EXPRESSION_MAX;
    l->mod_target = LIVE_MOD_PULSE_WIDTH;
    l->bend_range_rpn = 0;
    l->envelope_mode = LIVE_ENVELOPE_INSTANT;
    l->profile = (uint8_t)sid_chip_default(model);
    l->profile_req = l->profile;
    l->profile_6581 = (uint8_t)sid_chip_default(SID_MODEL_6581);
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; ++f) {
        l->character[f] = 0;
    }
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        l->expert[f] = SID_EXPERT_DEFAULT;
        l->expert_sent[f] = SID_EXPERT_DEFAULT;
        for (uint32_t n = 0; n < LIVE_USER_PROFILES; n++) {
            l->user_expert[n][f] = SID_EXPERT_DEFAULT;
        }
    }
    l->expert_dirty = 0;
    l->expert_active = 0;
    l->expert_version = 0;
    memset(l->slot, 0, sizeof l->slot);
    load_preset(l, 0);
    li_reset_chip(l);
    li_write_all(l);
    l->fx_select = 0;
    for (uint32_t k = 0; k < LIVE_SLOTS; ++k) {
        for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; ++f) {
            l->slot[k][f] = 0;
            put(l, (uint8_t)LIVE_REG_SLOT(k, f), 0);
        }
    }
}

LIVE_COLD static void arp_start(live* l) {
    int keep = -1;
    l->arp_n = 0;
    for (;;) {
        int v = -1;
        for (int i = 0; i < voice_count(l); ++i) {
            const live_voice* x = &l->voice[i];
            if (x->gate && (v < 0 || older(x->stamp, l->voice[v].stamp))) {
                v = i;
            }
        }
        if (v < 0) {
            break;
        }
        if (l->arp_n < LIVE_ARP_HELD) {
            uint32_t dup = 0;
            for (uint32_t i = 0; i < l->arp_n; i++) {
                dup |= l->arp_held[i] == l->voice[v].note;
            }
            if (!dup) {
                li_arp_hold(l, l->voice[v].note);
            }
        }
        if (keep < 0) {
            keep = v;
            l->voice[v].gate = 0;
        } else {
            li_release(l, v);
        }
    }
    if (keep < 0) {
        return;
    }
    l->voice[keep].gate = 1;
    l->arp_voice = (uint8_t)keep;
    l->arp_pos = 0;
    li_arp_rewind(l);
    li_arp_sound(l);
}

LIVE_COLD static void arp_stop(live* l) {
    if (l->arp_voice == LIVE_ARP_NO_VOICE) {
        return;
    }
    uint8_t notes[LIVE_ARP_HELD];
    uint32_t n = l->arp_n;
    for (uint32_t i = 0; i < n; i++) {
        notes[i] = l->arp_held[i];
    }
    uint8_t velocity = l->voice[l->arp_voice].velocity;
    li_release(l, l->arp_voice);
    l->arp_voice = LIVE_ARP_NO_VOICE;
    l->arp_n = 0;
    for (uint32_t i = 0; i < n; i++) {
        live_note_on(l, notes[i], velocity);
    }
}

LIVE_COLD void live_arp_set(live* l, uint32_t field, uint32_t value) {
    l->settings_dirty = 1;
    switch (field) {
    case LIVE_ARP_ON: {
        uint8_t want = value ? 1u : 0u;
        if (want != l->arp_on) {
            l->arp_on = want;
            if (want) {
                arp_start(l);
            } else {
                arp_stop(l);
            }
        }
        break;
    }
    case LIVE_ARP_RATE:
        value = value < LIVE_ARP_RATES - 1u ? value : LIVE_ARP_RATES - 1u;
        if (value != l->arp_rate) {
            l->arp_rate = (uint8_t)value;
            li_arp_rewind(l);
        }
        break;
    case LIVE_ARP_ORDER:
    case LIVE_ARP_SPAN: {
        uint8_t* f = field == LIVE_ARP_ORDER ? &l->arp_order : &l->arp_span;
        uint32_t lo = field == LIVE_ARP_ORDER ? 0u : 1u;
        uint32_t hi = field == LIVE_ARP_ORDER ? (uint32_t)LIVE_ARP_PLAYED : LIVE_ARP_SPAN_MAX;
        value = value < lo ? lo : value > hi ? hi : value;
        if (value != *f) {
            *f = (uint8_t)value;
            if (l->arp_voice != LIVE_ARP_NO_VOICE) {
                li_arp_resync(l);
            }
        }
        break;
    }
    default: break;
    }
}

LIVE_COLD void live_arp_turn(live* l, uint32_t field, int32_t detents) {
    if (field == LIVE_ARP_ON || field > LIVE_ARP_SPAN) {
        return;
    }
    const uint8_t now = field == LIVE_ARP_RATE ? l->arp_rate
                        : field == LIVE_ARP_ORDER ? l->arp_order
                                                  : l->arp_span;
    detents = detents > 16 ? 16 : detents < -16 ? -16 : detents;
    int32_t to = (int32_t)now + detents;
    live_arp_set(l, field, to < 0 ? 0u : (uint32_t)to);
}

void live_set_preset(live* l, uint8_t index) {
    l->settings_dirty = 1;
    if (index >= LIVE_PRESET_SLOTS) {
        index = LIVE_PRESET_SLOTS - 1;
    }
    load_preset(l, index);
    l->edits[index].valid = 0;
    li_write_all(l);
}

LIVE_COLD static void edit_store(const live* l, live_edit* e) {
    e->cutoff = l->cutoff;
    e->pulse_width = l->pulse_width;
    e->resonance = l->resonance;
    e->control = l->control;
    e->attack_decay = l->attack_decay;
    e->sustain_release = l->sustain_release;
    e->filter_voices = l->filter_voices;
    e->filter_mode = l->filter_mode;
    e->bend_range = l->bend_range;
    e->velocity_target = l->velocity_target;
    e->mod_target = l->mod_target;
    e->pressure_target = l->pressure_target;
    e->vibrato_rate = l->vibrato_rate;
    e->vibrato_depth = l->vibrato_depth;
    e->vibrato_delay = l->vibrato_delay;
    e->sync_semis = l->sync_semis;
    e->arp_on = l->arp_on;
    e->arp_rate = l->arp_rate;
    e->arp_order = l->arp_order;
    e->arp_span = l->arp_span;
    e->pwm_depth = l->pwm_depth;
    e->pwm_rate = l->pwm_rate;
    e->tune = l->tune;
    e->filter_lfo = l->filter_lfo;
    e->reso_link = l->reso_link;
    e->pwm_link = l->pwm_link;
    e->valid = 1;
}

LIVE_COLD static void edit_apply(live* l, const live_edit* e) {
    l->cutoff = e->cutoff;
    l->pulse_width = e->pulse_width;
    l->resonance = e->resonance;
    l->control = e->control;
    l->attack_decay = e->attack_decay;
    l->sustain_release = e->sustain_release;
    l->filter_voices = e->filter_voices;
    l->filter_mode = e->filter_mode;
    if (!l->bend_range_rpn) {
        l->bend_range = e->bend_range;
    }
    l->velocity_target = e->velocity_target;
    if (l->mod_target != e->mod_target) {
        l->mod_wheel = 0;
    }
    l->mod_target = e->mod_target;
    l->pressure_target = e->pressure_target;
    l->vibrato_rate = e->vibrato_rate;
    l->vibrato_depth = e->vibrato_depth;
    l->vibrato_delay = e->vibrato_delay;
    l->arp_on = e->arp_on;
    l->arp_rate = e->arp_rate;
    l->arp_order = e->arp_order;
    l->arp_span = e->arp_span;
    l->sync_semis = e->sync_semis;
    l->sync_q8 = (uint16_t)((l->sync_base_q8 * live_ratio_q16[e->sync_semis + LIVE_RATIO_SEMIS] +
                             32768u) >> 16);
    l->pwm_depth = e->pwm_depth;
    l->pwm_rate = e->pwm_rate;
    l->tune = e->tune;
    l->filter_lfo = e->filter_lfo;
    l->reso_link = e->reso_link;
    l->pwm_link = e->pwm_link;
}

static void user_leave(live* l) {
    if (live_preset_edited(l)) {
        live_sound_get(l, l->user_edit);
        l->user_edit_slot = l->user_sound;
    } else if (l->user_edit_slot == l->user_sound) {
        l->user_edit_slot = 0;
    }
}

LIVE_COLD void live_recall_preset(live* l, uint8_t q) {
    if (q >= LIVE_PRESET_SLOTS) {
        q = LIVE_PRESET_SLOTS - 1;
    }
    l->settings_dirty = 1;
    if (l->user_sound) {
        user_leave(l);
    } else {
        edit_store(l, &l->edits[l->preset]);
    }
    load_preset(l, q);
    if (l->edits[q].valid) {
        edit_apply(l, &l->edits[q]);
        if (q == LIVE_PRESET_DICE) {
            live_sound_get(l, l->base);
        }
    }
    li_write_all(l);
}

uint32_t live_sound_edited(const live* l, const uint8_t now[LAB_SOUND_BYTES]) {
    if (l->preset == LIVE_PRESET_DICE) {
        return 0;
    }
    for (uint32_t i = 0; i < LAB_SOUND_BYTES; i++) {
        if (now[i] != l->base[i] && (i != LAB_SOUND_BEND_RANGE || !l->bend_range_rpn)) {
            return 1;
        }
    }
    return 0;
}

uint32_t live_preset_edited(const live* l) {
    uint8_t now[LAB_SOUND_BYTES];
    live_sound_get(l, now);
    return live_sound_edited(l, now);
}

LIVE_COLD void live_preset_name(const live* l, char* buf) {
    const char* name = l->base_name;
    uint32_t n = 0;
    if (l->preset == LIVE_PRESET_DICE) {
        uint32_t r = l->dice_rolls % 100u;
        for (; name[n]; n++) {
            buf[n] = name[n];
        }
        buf[n++] = ' ';
        buf[n++] = (char)('0' + r / 10u);
        buf[n++] = (char)('0' + r % 10u);
        buf[n] = 0;
        return;
    }
    for (; name[n] && n + 2u < LIVE_PRESET_NAME_CHARS; n++) {
        buf[n] = name[n];
    }
    if (live_preset_edited(l)) {
        buf[n++] = '*';
    }
    buf[n] = 0;
}

LIVE_COLD void live_preset_params(uint8_t index, uint16_t out[LIVE_PARAM_COUNT]) {
    const live_preset* p = &presets[index < LIVE_PRESET_COUNT ? index : LIVE_PRESET_COUNT - 1u];
    out[LIVE_PARAM_CUTOFF] = p->cutoff;
    out[LIVE_PARAM_RESONANCE] = p->resonance;
    out[LIVE_PARAM_PULSE_WIDTH] = p->pulse_width;
    out[LIVE_PARAM_WAVEFORM] = (uint16_t)(p->control >> 4);
    out[LIVE_PARAM_RING] = (p->control & CTRL_RING) ? 1u : 0u;
    out[LIVE_PARAM_SYNC] = (p->control & CTRL_SYNC) ? 1u : 0u;
    out[LIVE_PARAM_ATTACK] = (uint16_t)(p->attack_decay >> 4);
    out[LIVE_PARAM_DECAY] = (uint16_t)(p->attack_decay & 0x0fu);
    out[LIVE_PARAM_SUSTAIN] = (uint16_t)(p->sustain_release >> 4);
    out[LIVE_PARAM_RELEASE] = (uint16_t)(p->sustain_release & 0x0fu);
    out[LIVE_PARAM_FILTER_MODE] = (uint16_t)((p->filter_mode >> 4) & 7u);
    out[LIVE_PARAM_FILTER_VOICES] = (uint16_t)(p->filter_voices & 7u);
    out[LIVE_PARAM_BEND_RANGE] = p->bend_range;
    out[LIVE_PARAM_VELOCITY_TARGET] = p->velocity_target;
    out[LIVE_PARAM_MOD_TARGET] = p->mod_target;
    out[LIVE_PARAM_PRESSURE_TARGET] = p->pressure_target;
    out[LIVE_PARAM_VIBRATO_RATE] = p->vibrato_rate;
    out[LIVE_PARAM_VIBRATO_DEPTH] = p->vibrato_depth;
    out[LIVE_PARAM_VIBRATO_DELAY] = p->vibrato_delay;
    out[LIVE_PARAM_PWM_DEPTH] = p->pwm_depth;
    out[LIVE_PARAM_PWM_RATE] = (uint16_t)live_pwm_rate(p->pwm_rate);
    out[LIVE_PARAM_TUNE] = LIVE_TUNE_CENTS;
    out[LIVE_PARAM_FILTER_LFO] = 0;
    out[LIVE_PARAM_ARP_RATE] = LIVE_ARP_RATE_DEFAULT;
    out[LIVE_PARAM_RESO_LINK] = LIVE_LINK_MAX;
    out[LIVE_PARAM_PWM_LINK] = LIVE_LINK_MAX;
}

static void put16(uint8_t* s, uint32_t at, uint32_t v) {
    s[at] = (uint8_t)v;
    s[at + 1u] = (uint8_t)(v >> 8);
}

static uint32_t get16(const uint8_t* s, uint32_t at) {
    return (uint32_t)s[at] | (uint32_t)s[at + 1u] << 8;
}

void live_sound_get(const live* l, uint8_t out[LAB_SOUND_BYTES]) {
    out[LAB_SOUND_CONTROL] = l->control;
    out[LAB_SOUND_AD] = l->attack_decay;
    out[LAB_SOUND_SR] = l->sustain_release;
    put16(out, LAB_SOUND_PULSE, l->pulse_width);
    put16(out, LAB_SOUND_CUTOFF, l->cutoff);
    out[LAB_SOUND_RESONANCE] = l->resonance;
    out[LAB_SOUND_FILTER_VOICES] = l->filter_voices;
    out[LAB_SOUND_FILTER_MODE] = l->filter_mode;
    out[LAB_SOUND_MONO_SYNC] = l->mono_sync;
    put16(out, LAB_SOUND_SYNC_BASE, l->sync_base_q8);
    out[LAB_SOUND_SYNC_SEMIS] = (uint8_t)l->sync_semis;
    put16(out, LAB_SOUND_PWM_DEPTH, l->pwm_depth);
    put16(out, LAB_SOUND_PWM_RATE, l->pwm_rate);
    out[LAB_SOUND_TUNE] = (uint8_t)l->tune;
    out[LAB_SOUND_BEND_RANGE] = l->bend_range;
    out[LAB_SOUND_VELOCITY] = l->velocity_target;
    out[LAB_SOUND_MOD] = l->mod_target;
    out[LAB_SOUND_PRESSURE] = l->pressure_target;
    out[LAB_SOUND_VIBRATO_RATE] = l->vibrato_rate;
    out[LAB_SOUND_VIBRATO_DEPTH] = l->vibrato_depth;
    out[LAB_SOUND_VIBRATO_DELAY] = l->vibrato_delay;
    out[LAB_SOUND_ARP + LIVE_ARP_ON] = l->arp_on;
    out[LAB_SOUND_ARP + LIVE_ARP_RATE] = l->arp_rate;
    out[LAB_SOUND_ARP + LIVE_ARP_ORDER] = l->arp_order;
    out[LAB_SOUND_ARP + LIVE_ARP_SPAN] = l->arp_span;
    memcpy(&out[LAB_SOUND_SLOTS], l->slot, sizeof l->slot);
    out[LAB_SOUND_FILTER_LFO] = l->filter_lfo;
    out[LAB_SOUND_RESO_LINK] = (uint8_t)l->reso_link;
    out[LAB_SOUND_PWM_LINK] = (uint8_t)l->pwm_link;
}

static const struct {
    uint8_t param, at, mask, shift;
} SOUND_PARAMS[] = {
    {LIVE_PARAM_WAVEFORM, LAB_SOUND_CONTROL, 0xf0u, 4u},
    {LIVE_PARAM_RING, LAB_SOUND_CONTROL, CTRL_RING, 2u},
    {LIVE_PARAM_SYNC, LAB_SOUND_CONTROL, CTRL_SYNC, 1u},
    {LIVE_PARAM_ATTACK, LAB_SOUND_AD, 0xf0u, 4u},
    {LIVE_PARAM_DECAY, LAB_SOUND_AD, 0x0fu, 0u},
    {LIVE_PARAM_SUSTAIN, LAB_SOUND_SR, 0xf0u, 4u},
    {LIVE_PARAM_RELEASE, LAB_SOUND_SR, 0x0fu, 0u},
    {LIVE_PARAM_PULSE_WIDTH, LAB_SOUND_PULSE, 0u, 16u},
    {LIVE_PARAM_CUTOFF, LAB_SOUND_CUTOFF, 0u, 16u},
    {LIVE_PARAM_RESONANCE, LAB_SOUND_RESONANCE, 0xffu, 0u},
    {LIVE_PARAM_FILTER_MODE, LAB_SOUND_FILTER_MODE, 0x70u, 4u},
    {LIVE_PARAM_FILTER_VOICES, LAB_SOUND_FILTER_VOICES, 0x07u, 0u},
    {LIVE_PARAM_VELOCITY_TARGET, LAB_SOUND_VELOCITY, 0xffu, 0u},
    {LIVE_PARAM_MOD_TARGET, LAB_SOUND_MOD, 0xffu, 0u},
    {LIVE_PARAM_PRESSURE_TARGET, LAB_SOUND_PRESSURE, 0xffu, 0u},
    {LIVE_PARAM_VIBRATO_RATE, LAB_SOUND_VIBRATO_RATE, 0xffu, 0u},
    {LIVE_PARAM_VIBRATO_DEPTH, LAB_SOUND_VIBRATO_DEPTH, 0xffu, 0u},
    {LIVE_PARAM_VIBRATO_DELAY, LAB_SOUND_VIBRATO_DELAY, 0xffu, 0u},
};

static int8_t link_of(uint8_t b) {
    int32_t v = (int8_t)b;
    return (int8_t)(v < -LIVE_LINK_MAX ? -LIVE_LINK_MAX : v > LIVE_LINK_MAX ? LIVE_LINK_MAX : v);
}

LIVE_COLD void live_sound_apply(live* l, const uint8_t in[LAB_SOUND_BYTES]) {
    l->settings_dirty = 1;
    uint8_t mono = in[LAB_SOUND_MONO_SYNC] ? 1u : 0u;
    if (mono != l->mono_sync) {
        live_all_notes_off(l);
        l->mono_sync = mono;
        li_write_controls(l);
    }
    uint32_t base = get16(in, LAB_SOUND_SYNC_BASE);
    int32_t semis = (int8_t)in[LAB_SOUND_SYNC_SEMIS];
    semis = semis < -LIVE_RATIO_SEMIS ? -LIVE_RATIO_SEMIS
            : semis > LIVE_RATIO_SEMIS ? LIVE_RATIO_SEMIS
                                       : semis;
    l->sync_base_q8 = (uint16_t)(base < LAB_SOUND_SYNC_BASE_MAX ? base : LAB_SOUND_SYNC_BASE_MAX);
    l->sync_semis = (int8_t)semis;
    l->sync_q8 = (uint16_t)((l->sync_base_q8 * live_ratio_q16[semis + LIVE_RATIO_SEMIS] +
                             32768u) >> 16);
    uint32_t depth = get16(in, LAB_SOUND_PWM_DEPTH), rate = get16(in, LAB_SOUND_PWM_RATE);
    l->pwm_depth = (uint16_t)(depth < LIVE_PWM_DEPTH_MAX ? depth : LIVE_PWM_DEPTH_MAX);
    l->pwm_rate = (uint16_t)(rate < LIVE_PWM_STEP_MAX ? rate : LIVE_PWM_STEP_MAX);
    int32_t tune = (int8_t)in[LAB_SOUND_TUNE];
    l->tune = (int8_t)(tune < -LIVE_TUNE_CENTS ? -LIVE_TUNE_CENTS
                       : tune > LIVE_TUNE_CENTS ? LIVE_TUNE_CENTS
                                                : tune);
    live_param_set(l, LIVE_PARAM_FILTER_LFO, in[LAB_SOUND_FILTER_LFO]);
    live_param_set(l, LIVE_PARAM_RESO_LINK, (uint32_t)(link_of(in[LAB_SOUND_RESO_LINK]) + LIVE_LINK_MAX));
    live_param_set(l, LIVE_PARAM_PWM_LINK, (uint32_t)(link_of(in[LAB_SOUND_PWM_LINK]) + LIVE_LINK_MAX));
    for (uint32_t i = 0; i < sizeof SOUND_PARAMS / sizeof SOUND_PARAMS[0]; i++) {
        uint32_t at = SOUND_PARAMS[i].at;
        uint32_t v = SOUND_PARAMS[i].shift == 16u
                         ? get16(in, at)
                         : (uint32_t)(in[at] & SOUND_PARAMS[i].mask) >> SOUND_PARAMS[i].shift;
        live_param_set(l, SOUND_PARAMS[i].param, v);
    }
    if (in[LAB_SOUND_BEND_RANGE] != l->bend_range) {
        live_param_set(l, LIVE_PARAM_BEND_RANGE, in[LAB_SOUND_BEND_RANGE]);
    }
    for (uint32_t f = LIVE_ARP_SPAN; f > LIVE_ARP_ON; f--) {
        live_arp_set(l, f, in[LAB_SOUND_ARP + f]);
    }
    live_arp_set(l, LIVE_ARP_ON, in[LAB_SOUND_ARP + LIVE_ARP_ON]);
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; f++) {
            live_slot_set(l, k, f, in[LAB_SOUND_SLOTS + k * LIVE_SLOT_FIELDS + f]);
        }
    }
    for (int v = 0; v < LIVE_VOICES; ++v) {
        li_update_freq(l, v);
    }
    li_update_pws(l);
}

LIVE_COLD void live_load_user(live* l, uint32_t n, const uint8_t sound[LAB_SOUND_BYTES],
                              const char* name) {
    l->settings_dirty = 1;
    load_preset(l, 0);
    live_sound_apply(l, sound);
    live_sound_set_base(l, sound, name);
    l->user_sound = (uint8_t)(n + 1u);
    li_write_all(l);
}

LIVE_COLD void live_sound_set_base(live* l, const uint8_t in[LAB_SOUND_BYTES], const char* name) {
    l->settings_dirty = 1;
    if (in != l->base) {
        memcpy(l->base, in, LAB_SOUND_BYTES);
    }
    if (name) {
        uint32_t n = 0;
        for (; name[n] && n < LAB_SOUND_NAME; n++) {
            l->base_name[n] = name[n];
        }
        l->base_name[n] = 0;
    }
}

const uint32_t live_ratio_q16[2 * LIVE_RATIO_SEMIS + 1] = {
    32768,  34716,  36781,  38968,  41285,  43740,  46341,  49097,  52016,
    55109,  58386,  61858,  65536,  69433,  73562,  77936,  82570,  87480,
    92682,  98193,  104032, 110218, 116772, 123715, 131072,
};

static uint32_t dice_seed(const live* l) {
    return l->note_ons * 2654435761u ^ l->clock * 40503u ^ l->lfo_phase;
}

#define STEP_END (LIVE_PRESET_SLOTS + LIVE_SOUNDS)

LIVE_COLD void live_preset_step(live* l, int32_t steps, int revert) {
    if (revert) {
        if (l->user_sound) {
            live_sound_apply(l, l->base);
            l->settings_dirty = 1;
            if (l->user_edit_slot == l->user_sound) {
                l->user_edit_slot = 0;
            }
        } else {
            live_set_preset(l, l->preset);
        }
        return;
    }
    uint32_t at = l->user_sound ? LIVE_PRESET_SLOTS - 1u + l->user_sound : l->preset;
    uint32_t to = at;
    for (uint32_t i = steps < 0 ? (uint32_t)-steps : (uint32_t)steps; i; i--) {
        uint32_t n = to;
        do {
            n = steps < 0 ? n - 1u : n + 1u;
        } while (n >= LIVE_PRESET_SLOTS && n < STEP_END &&
                 !(l->sounds_valid >> (n - LIVE_PRESET_SLOTS) & 1u));
        if (n >= STEP_END) {
            break;
        }
        to = n;
    }
    if (to == at) {
        if (at == LIVE_PRESET_DICE) {
            live_dice_roll(l, dice_seed(l));
        }
    } else if (to < LIVE_PRESET_SLOTS) {
        uint32_t fresh = to == LIVE_PRESET_DICE && !l->edits[to].valid;
        live_recall_preset(l, (uint8_t)to);
        if (fresh) {
            live_dice_roll(l, dice_seed(l));
        }
    } else {
        uint32_t n = to - LIVE_PRESET_SLOTS;
        l->settings_dirty = 1;
        if (l->user_sound) {
            user_leave(l);
        } else {
            edit_store(l, &l->edits[l->preset]);
        }
        if (l->user_load && l->user_load(l->user_ctx, n)) {
            if (l->user_edit_slot == n + 1u) {
                live_sound_apply(l, l->user_edit);
            }
        } else {
            l->sounds_valid = (uint16_t)(l->sounds_valid & ~(1u << n));
            if (!l->user_sound) {
                edit_apply(l, &l->edits[l->preset]);
            }
        }
    }
}

__attribute__((noinline, unused)) static void apply_user(live* l, uint32_t n) {
    if (n >= LIVE_USER_PROFILES || !(l->user_valid >> n & 1u)) {
        return;
    }
    li_request_profile(l, l->user_base[n]);
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; ++f) {
        l->character[f] = l->user_ctl[n][f];
    }
    l->expert_active = 0;
    uint32_t changed = 0;
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        changed |= l->expert[f] != l->user_expert[n][f];
        l->expert[f] = l->user_expert[n][f];
        l->expert_active |= l->expert[f] != SID_EXPERT_DEFAULT;
    }
    l->expert_dirty = 1;
    l->expert_version += changed != 0;
    l->user_applied = (uint8_t)(n + 1u);
}

void live_button(live* l, live_button_id button) {
    switch (button) {
    case LIVE_BUTTON_OCT_DOWN:
        if (l->octave > LIVE_OCTAVE_MIN) {
            --l->octave;
        }
        break;
    case LIVE_BUTTON_OCT_UP:
        if (l->octave < LIVE_OCTAVE_MAX) {
            ++l->octave;
        }
        break;
    case LIVE_BUTTON_FX:
        if (l->expert_active) {
            macro_changed(l, 0);
        }
        if (l->prepare) {
            sid_model m = l->model_req == SID_MODEL_6581 ? SID_MODEL_8580 : SID_MODEL_6581;
            l->user_applied = 0;
            li_request_profile(l, li_model_profile(l, m));
        } else {
            l->user_applied = 0;
            li_set_model(l, l->model == SID_MODEL_6581 ? SID_MODEL_8580 : SID_MODEL_6581);
        }
        break;
    default: break;
    }
}

void live_bend_range_rpn(live* l, uint32_t semitones) {
    l->settings_dirty = 1;
    l->bend_range = (uint8_t)(semitones > LIVE_BEND_RANGE_MAX ? LIVE_BEND_RANGE_MAX : semitones);
    l->bend_range_rpn = 1;
}

uint32_t live_param_max(uint32_t param) {
    switch (param) {
    case LIVE_PARAM_CUTOFF: return LIVE_CUTOFF_MAX;
    case LIVE_PARAM_RESONANCE: return LIVE_RESONANCE_MAX;
    case LIVE_PARAM_PULSE_WIDTH: return LIVE_PULSE_WIDTH_MAX;
    case LIVE_PARAM_VOLUME: return LIVE_MASTER_MAX;
    case LIVE_PARAM_WAVEFORM:
    case LIVE_PARAM_ATTACK:
    case LIVE_PARAM_DECAY:
    case LIVE_PARAM_SUSTAIN:
    case LIVE_PARAM_RELEASE: return 15u;
    case LIVE_PARAM_RING:
    case LIVE_PARAM_SYNC:
    case LIVE_PARAM_MODEL:
    case LIVE_PARAM_ENVELOPE_MODE: return 1u;
    case LIVE_PARAM_FILTER_MODE:
    case LIVE_PARAM_FILTER_VOICES: return 7u;
    case LIVE_PARAM_BEND_RANGE: return LIVE_BEND_RANGE_MAX;
    case LIVE_PARAM_VELOCITY_TARGET: return LIVE_VELOCITY_PULSE_WIDTH;
    case LIVE_PARAM_MOD_TARGET: return LIVE_MOD_CUTOFF;
    case LIVE_PARAM_PRESSURE_TARGET: return LIVE_PRESSURE_CUTOFF;
    case LIVE_PARAM_VIBRATO_RATE:
    case LIVE_PARAM_VIBRATO_DEPTH:
    case LIVE_PARAM_VIBRATO_DELAY: return LIVE_EXPRESSION_MAX;
    case LIVE_PARAM_PROFILE: return SID_CHIPS - 1u + LIVE_USER_PROFILES;
    case LIVE_PARAM_AGE:
    case LIVE_PARAM_DAC:
    case LIVE_PARAM_THUMP:
    case LIVE_PARAM_BLEED: return SID_CHARACTER_MAX;
    case LIVE_PARAM_PWM_DEPTH: return LIVE_PWM_DEPTH_MAX;
    case LIVE_PARAM_PWM_RATE: return LIVE_PWM_RATE_MAX;
    case LIVE_PARAM_TUNE: return 2u * LIVE_TUNE_CENTS;
    case LIVE_PARAM_FILTER_LFO: return LIVE_FILTER_LFO_MAX;
    case LIVE_PARAM_ARP_RATE: return LIVE_ARP_RATES - 1u;
    case LIVE_PARAM_RESO_LINK:
    case LIVE_PARAM_PWM_LINK: return 2u * LIVE_LINK_MAX;
    default:
        if (param >= LIVE_PARAM_EXPERT && param < LIVE_PARAM_EXPERT_END) {
            return SID_CHARACTER_MAX;
        }
        if (param >= LIVE_PARAM_SLOT && param < LIVE_PARAM_SLOT_END) {
            uint32_t slot = (param - LIVE_PARAM_SLOT) / LIVE_SLOT_FIELDS;
            uint32_t field = (param - LIVE_PARAM_SLOT) % LIVE_SLOT_FIELDS;
            return field == LIVE_SLOT_ALGORITHM ? live_slot_algorithms(slot) - 1u : LIVE_SLOT_MAX;
        }
        return 0;
    }
}

static int set_bits(uint8_t* field, uint32_t mask, uint32_t value) {
    uint8_t now = (uint8_t)((*field & ~mask) | (value & mask));
    if (now == *field) {
        return 0;
    }
    *field = now;
    return 1;
}

__attribute__((noinline, unused)) static void macro_changed(live* l, uint32_t field) {
    uint32_t mask;
    switch (field) {
    case 0:
        mask = (1u << SID_EXPERT_COUNT) - 1u;
        l->expert_active = 0;
        break;
    case 1:
        mask = (1u << SID_EXPERT_FILTER_6581_CURVE) | (1u << SID_EXPERT_FILTER_6581_RANGE) |
               (1u << SID_EXPERT_FILTER_8580_CURVE) | (1u << SID_EXPERT_NONLINEARITY);
        break;
    case 2:
        mask = (1u << SID_EXPERT_DAC_KINK) | (1u << SID_EXPERT_DAC_LEAKAGE);
        break;
    case 3: mask = 1u << SID_EXPERT_THUMP; break;
    case 4: mask = 1u << SID_EXPERT_BLEED; break;
    default: return;
    }
    uint32_t changed = 0;
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        if ((mask & (1u << f)) && l->expert[f] != SID_EXPERT_DEFAULT) {
            l->expert[f] = SID_EXPERT_DEFAULT;
            changed = 1;
        }
    }
    if (changed) {
        l->expert_dirty = 1;
        l->expert_version++;
    }
}

__attribute__((noinline, unused)) static uint32_t expert_get(const live* l, uint32_t f) {
    if (l->expert[f] != SID_EXPERT_DEFAULT) {
        return l->expert[f];
    }
    sid_character c;
    c.profile = (uint8_t)(l->profile_req + 1u);
    c.age = l->character[0];
    c.dac = l->character[1];
    c.thump = l->character[2];
    c.bleed = l->character[3];
    c.expert_mask = 0;
    return sid_character_expert_get(&c, l->model_req, f);
}

void live_param_set(live* l, uint32_t param, uint32_t value) {
    if (param == LIVE_PARAM_CUTOFF) {
        l->settings_dirty = 1;
        li_set_cutoff(l, (uint16_t)(value > LIVE_CUTOFF_MAX ? LIVE_CUTOFF_MAX : value));
        return;
    }
    if (param >= LIVE_PARAM_COUNT) {
        return;
    }
    uint32_t max = live_param_max(param);
    if (value > max) {
        value = max;
    }
    if (param != LIVE_PARAM_VOLUME) {
        l->settings_dirty = 1;
    }
    switch (param) {
    case LIVE_PARAM_RESONANCE: li_set_resonance(l, (uint8_t)value); break;
    case LIVE_PARAM_PULSE_WIDTH: li_set_pulse_width(l, (uint16_t)value); break;
    case LIVE_PARAM_VOLUME: live_set_master(l, (uint16_t)value); break;
    case LIVE_PARAM_WAVEFORM:
        if (set_bits(&l->control, CTRL_WAVEFORM, value << 4)) {
            li_write_controls(l);
        }
        break;
    case LIVE_PARAM_RING:
        if (set_bits(&l->control, CTRL_RING, value ? CTRL_RING : 0u)) {
            li_write_controls(l);
        }
        break;
    case LIVE_PARAM_SYNC:
        if (set_bits(&l->control, CTRL_SYNC, value ? CTRL_SYNC : 0u)) {
            li_write_controls(l);
        }
        break;
    case LIVE_PARAM_ATTACK:
        if (set_bits(&l->attack_decay, 0xf0u, value << 4)) {
            li_write_envelopes(l);
        }
        break;
    case LIVE_PARAM_DECAY:
        if (set_bits(&l->attack_decay, 0x0fu, value)) {
            li_write_envelopes(l);
        }
        break;
    case LIVE_PARAM_SUSTAIN:
        if (set_bits(&l->sustain_release, 0xf0u, value << 4)) {
            li_write_envelopes(l);
        }
        break;
    case LIVE_PARAM_RELEASE:
        if (set_bits(&l->sustain_release, 0x0fu, value)) {
            li_write_envelopes(l);
        }
        break;
    case LIVE_PARAM_FILTER_MODE:
        if (set_bits(&l->filter_mode, 0x70u, value << 4)) {
            li_write_mode_vol(l);
        }
        break;
    case LIVE_PARAM_FILTER_VOICES:
        if (set_bits(&l->filter_voices, 0x07u, value)) {
            li_write_res_filt(l);
        }
        break;
    case LIVE_PARAM_MODEL:
        if (l->expert_active) {
            macro_changed(l, 0);
        }
        l->user_applied = 0;
        li_request_profile(l, li_model_profile(l, value ? SID_MODEL_8580 : SID_MODEL_6581));
        break;
    case LIVE_PARAM_PROFILE:
        if (value < SID_CHIPS) {
            if (l->expert_active) {
                macro_changed(l, 0);
            }
            l->user_applied = 0;
            li_request_profile(l, value);
        } else {
            apply_user(l, value - SID_CHIPS);
        }
        break;
    case LIVE_PARAM_AGE:
    case LIVE_PARAM_DAC:
    case LIVE_PARAM_THUMP:
    case LIVE_PARAM_BLEED:
        if (l->expert_active && l->character[param - LIVE_PARAM_AGE] != value) {
            macro_changed(l, param - LIVE_PARAM_AGE + 1u);
        }
        l->character[param - LIVE_PARAM_AGE] = (uint8_t)value;
        break;
    case LIVE_PARAM_BEND_RANGE:
        l->bend_range = (uint8_t)value;
        l->bend_range_rpn = 0;
        break;
    case LIVE_PARAM_VELOCITY_TARGET:
        if (l->velocity_target != value) {
            l->velocity_target = (uint8_t)value;
            li_write_envelopes(l);
        }
        break;
    case LIVE_PARAM_MOD_TARGET:
        if (l->mod_target != value) {
            l->mod_target = (uint8_t)value;
            l->mod_wheel = 0;
        }
        break;
    case LIVE_PARAM_PRESSURE_TARGET: l->pressure_target = (uint8_t)value; break;
    case LIVE_PARAM_VIBRATO_RATE: l->vibrato_rate = (uint8_t)value; break;
    case LIVE_PARAM_VIBRATO_DEPTH: l->vibrato_depth = (uint8_t)value; break;
    case LIVE_PARAM_VIBRATO_DELAY: l->vibrato_delay = (uint8_t)value; break;
    case LIVE_PARAM_ENVELOPE_MODE:
        if (l->envelope_mode != value) {
            l->envelope_mode = (uint8_t)value;
            li_write_gate_reset(l);
        }
        break;
    case LIVE_PARAM_PWM_DEPTH:
        l->pwm_depth = (uint16_t)value;
        li_update_pws(l);
        break;
    case LIVE_PARAM_PWM_RATE: l->pwm_rate = (uint16_t)live_pwm_step(value); break;
    case LIVE_PARAM_TUNE:
        l->tune = (int8_t)((int32_t)value - LIVE_TUNE_CENTS);
        for (int v = 0; v < LIVE_VOICES; ++v) {
            li_update_freq(l, v);
        }
        break;
    case LIVE_PARAM_FILTER_LFO:
        l->filter_lfo = (uint8_t)value;
        li_update_cutoff(l);
        break;
    case LIVE_PARAM_ARP_RATE: live_arp_set(l, LIVE_ARP_RATE, value); break;
    case LIVE_PARAM_RESO_LINK:
        if (l->reso_link != (int8_t)((int32_t)value - LIVE_LINK_MAX)) {
            l->reso_link = (int8_t)((int32_t)value - LIVE_LINK_MAX);
            li_write_res_filt(l);
        }
        break;
    case LIVE_PARAM_PWM_LINK:
        l->pwm_link = (int8_t)((int32_t)value - LIVE_LINK_MAX);
        li_update_pws(l);
        break;
    default:
        if (param >= LIVE_PARAM_EXPERT && param < LIVE_PARAM_EXPERT_END) {
            if (l->expert[param - LIVE_PARAM_EXPERT] != value) {
                l->expert[param - LIVE_PARAM_EXPERT] = (uint8_t)value;
                l->expert_active = 1;
                l->expert_dirty = 1;
                l->expert_version++;
            }
        } else {
            live_slot_set(l, (param - LIVE_PARAM_SLOT) / LIVE_SLOT_FIELDS,
                          (param - LIVE_PARAM_SLOT) % LIVE_SLOT_FIELDS, (int32_t)value);
        }
        break;
    }
}

uint32_t live_param_get(const live* l, uint32_t param) {
    switch (param) {
    case LIVE_PARAM_CUTOFF: return l->cutoff;
    case LIVE_PARAM_RESONANCE: return l->resonance;
    case LIVE_PARAM_PULSE_WIDTH: return l->pulse_width;
    case LIVE_PARAM_VOLUME: return l->master;
    case LIVE_PARAM_WAVEFORM: return l->control >> 4;
    case LIVE_PARAM_RING: return (l->control & CTRL_RING) ? 1u : 0u;
    case LIVE_PARAM_SYNC: return (l->control & CTRL_SYNC) ? 1u : 0u;
    case LIVE_PARAM_ATTACK: return l->attack_decay >> 4;
    case LIVE_PARAM_DECAY: return l->attack_decay & 0x0fu;
    case LIVE_PARAM_SUSTAIN: return l->sustain_release >> 4;
    case LIVE_PARAM_RELEASE: return l->sustain_release & 0x0fu;
    case LIVE_PARAM_FILTER_MODE: return (l->filter_mode >> 4) & 7u;
    case LIVE_PARAM_FILTER_VOICES: return l->filter_voices & 7u;
    case LIVE_PARAM_MODEL: return l->model_req == SID_MODEL_8580 ? 1u : 0u;
    case LIVE_PARAM_BEND_RANGE: return l->bend_range;
    case LIVE_PARAM_VELOCITY_TARGET: return l->velocity_target;
    case LIVE_PARAM_MOD_TARGET: return l->mod_target;
    case LIVE_PARAM_PRESSURE_TARGET: return l->pressure_target;
    case LIVE_PARAM_VIBRATO_RATE: return l->vibrato_rate;
    case LIVE_PARAM_VIBRATO_DEPTH: return l->vibrato_depth;
    case LIVE_PARAM_VIBRATO_DELAY: return l->vibrato_delay;
    case LIVE_PARAM_ENVELOPE_MODE: return l->envelope_mode;
    case LIVE_PARAM_PROFILE: return l->profile_req;
    case LIVE_PARAM_AGE:
    case LIVE_PARAM_DAC:
    case LIVE_PARAM_THUMP:
    case LIVE_PARAM_BLEED: return l->character[param - LIVE_PARAM_AGE];
    case LIVE_PARAM_PWM_DEPTH: return l->pwm_depth;
    case LIVE_PARAM_PWM_RATE: return live_pwm_rate(l->pwm_rate);
    case LIVE_PARAM_TUNE: return (uint32_t)(l->tune + LIVE_TUNE_CENTS);
    case LIVE_PARAM_FILTER_LFO: return l->filter_lfo;
    case LIVE_PARAM_ARP_RATE: return l->arp_rate;
    case LIVE_PARAM_RESO_LINK: return (uint32_t)(l->reso_link + LIVE_LINK_MAX);
    case LIVE_PARAM_PWM_LINK: return (uint32_t)(l->pwm_link + LIVE_LINK_MAX);
    default:
        if (param >= LIVE_PARAM_EXPERT && param < LIVE_PARAM_EXPERT_END) {
            return expert_get(l, param - LIVE_PARAM_EXPERT);
        }
        if (param >= LIVE_PARAM_SLOT && param < LIVE_PARAM_SLOT_END) {
            return l->slot[(param - LIVE_PARAM_SLOT) / LIVE_SLOT_FIELDS]
                          [(param - LIVE_PARAM_SLOT) % LIVE_SLOT_FIELDS];
        }
        return 0;
    }
}

void live_params_wire(const live* l, uint16_t out[LIVE_PARAM_COUNT]) {
    live_params_base(l, out);
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        out[LIVE_PARAM_EXPERT + f] = l->expert[f];
    }
    live_params_tail(l, out);
}

void live_params(const live* l, uint16_t out[LIVE_PARAM_COUNT]) {
    live_params_base(l, out);
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        out[LIVE_PARAM_EXPERT + f] = (uint16_t)expert_get(l, f);
    }
    live_params_tail(l, out);
}

uint32_t live_pwm_step(uint32_t rate) {
    rate = rate < LIVE_PWM_RATE_MAX ? rate : LIVE_PWM_RATE_MAX;
    return (rate * 32768u + 500u) / 1000u;
}

uint32_t live_pwm_rate(uint32_t step) {
    uint32_t rate = (step * 1000u + 16384u) / 32768u;
    return rate < LIVE_PWM_RATE_MAX ? rate : LIVE_PWM_RATE_MAX;
}

void live_set_keys_mode(live* l, uint32_t mode) {
    l->settings_dirty = 1;
    l->keys_mode = mode == LIVE_KEYS_A ? (uint8_t)LIVE_KEYS_A : (uint8_t)LIVE_KEYS_AUTO;
}

void live_set_theme(live* l, uint32_t theme) {
    l->settings_dirty = 1;
    l->theme = (uint8_t)(theme < LIVE_THEMES ? theme : 0u);
}

void live_set_preparer(live* l, live_prepare_fn prepare, void* ctx) {
    if (l->preparing) {
        l->prepare(l->prepare_ctx, LIVE_PREPARE_CANCEL);
    }
    l->prepare = prepare;
    l->prepare_ctx = ctx;
    l->preparing = 0;
}
