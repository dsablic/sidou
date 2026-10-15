// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "live.h"

#include "fx_live.h"

#include <stddef.h>

#define CTRL_GATE 0x01u
#define REG_FC_LO 0x15u
#define REG_FC_HI 0x16u
#define REG_RES_FILT 0x17u
#define REG_MODE_VOL 0x18u
#define LFO_CENTRE 16384u

static const live_preset presets[LIVE_PRESET_COUNT] = {
    {
        .name = "SAW LEAD",
        .control = 0x20, /* saw */
        .attack_decay = 0x09,
        .sustain_release = 0xa6,
        .pulse_width = 2048,
        .cutoff = 1200,
        .resonance = 6,
        .filter_voices = 0x07,
        .filter_mode = 0x10, /* low-pass */
    },
    {
        .name = "PWM BASS",
        .control = 0x40, /* pulse */
        .attack_decay = 0x05,
        .sustain_release = 0x84,
        .pulse_width = 1024,
        .cutoff = 700,
        .resonance = 10,
        .filter_voices = 0x07,
        .filter_mode = 0x10,
        .pwm_depth = 768,
        .pwm_rate = 328, /* a 2 s cycle */
    },
    {
        .name = "SYNC LEAD",
        .control = 0x22, /* saw, hard-synced to voice 3 */
        .attack_decay = 0x0a,
        .sustain_release = 0xb8,
        .pulse_width = 2048,
        .cutoff = 1500,
        .resonance = 4,
        .filter_voices = 0x01,
        .filter_mode = 0x10,
        .mono_sync = 1,
        .sync_ratio_q8 = 704, /* 2.75 */
    },
    {
        .name = "NOISE PERC",
        .control = 0x80, /* noise */
        .attack_decay = 0x08,
        .sustain_release = 0x08,
        .pulse_width = 2048,
        .cutoff = 1800,
        .resonance = 2,
        .filter_voices = 0x07,
        .filter_mode = 0x50, /* low-pass + high-pass: a notch */
    },
};

void live_sid_sink(void* ctx, uint8_t reg, uint8_t value) {
    live_sid_target* t = ctx;
    if (reg == LIVE_REG_RESET) {
        sid_model model = value == SID_MODEL_8580 ? SID_MODEL_8580 : SID_MODEL_6581;
        sid_init(t->chip, model, t->clock_hz, t->sample_rate);
    } else if (!(reg & 0x80u)) {
        sid_write(t->chip, reg, value);
    }
}

const live_preset* live_preset_get(uint8_t index) {
    return index < LIVE_PRESET_COUNT ? &presets[index] : NULL;
}

static const live_preset* cur(const live* l) {
    return &presets[l->preset];
}

static void put(live* l, uint8_t reg, uint8_t value) {
    l->sink(l->sink_ctx, reg, value);
}

static uint8_t voice_count(const live* l) {
    return cur(l)->mono_sync ? 1 : LIVE_VOICES;
}

static uint16_t voice_freq(const live* l, int v) {
    const live_preset* p = cur(l);
    int src = p->mono_sync ? 0 : v;
    uint8_t note = l->voice[src].note;
    if (note == LIVE_NO_NOTE || (p->mono_sync && v == 1)) {
        return 0;
    }
    uint32_t f = live_freq_table[note & 0x7f];
    if (p->mono_sync && v == 0) {
        f = (f * p->sync_ratio_q8) >> 8;
        if (f > 0xffffu) {
            f = 0xffffu;
        }
    }
    return (uint16_t)f;
}

static uint8_t voice_control(const live* l, int v) {
    const live_preset* p = cur(l);
    if (p->mono_sync && v != 0) {
        return 0; /* voice 3 only clocks the sync; voice 2 is unused */
    }
    return (uint8_t)(p->control | (l->voice[v].gate ? CTRL_GATE : 0u));
}

static void write_freq(live* l, int v) {
    uint16_t f = voice_freq(l, v);
    put(l, (uint8_t)(v * 7), (uint8_t)f);
    put(l, (uint8_t)(v * 7 + 1), (uint8_t)(f >> 8));
}

static void write_control(live* l, int v) {
    put(l, (uint8_t)(v * 7 + 4), voice_control(l, v));
}

static void write_pw(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        put(l, (uint8_t)(v * 7 + 2), (uint8_t)l->pw_out);
        put(l, (uint8_t)(v * 7 + 3), (uint8_t)(l->pw_out >> 8));
    }
}

static void write_cutoff(live* l) {
    put(l, REG_FC_LO, (uint8_t)(l->cutoff & 7u));
    put(l, REG_FC_HI, (uint8_t)(l->cutoff >> 3));
}

static void write_res_filt(live* l) {
    put(l, REG_RES_FILT, (uint8_t)(l->resonance << 4 | cur(l)->filter_voices));
}

static void write_mode_vol(live* l) {
    put(l, REG_MODE_VOL, (uint8_t)(cur(l)->filter_mode | l->volume));
}

/* Every register, envelopes before gates. */
static void write_all(live* l) {
    const live_preset* p = cur(l);
    for (int v = 0; v < LIVE_VOICES; ++v) {
        put(l, (uint8_t)(v * 7 + 5), p->attack_decay);
        put(l, (uint8_t)(v * 7 + 6), p->sustain_release);
        write_freq(l, v);
        put(l, (uint8_t)(v * 7 + 2), (uint8_t)l->pw_out);
        put(l, (uint8_t)(v * 7 + 3), (uint8_t)(l->pw_out >> 8));
        write_control(l, v);
    }
    write_cutoff(l);
    write_res_filt(l);
    write_mode_vol(l);
}

/* The pulse width after the LFO, clamped to 12 bits. */
static uint16_t pw_effective(const live* l) {
    int32_t pw = l->pulse_width;
    uint16_t depth = cur(l)->pwm_depth;
    if (depth) {
        int32_t phase = l->lfo_phase;
        int32_t tri = phase < 32768 ? phase : 65536 - phase; /* 0..32768 */
        pw += (tri - (int32_t)LFO_CENTRE) * depth / (int32_t)LFO_CENTRE;
    }
    if (pw < 0) {
        pw = 0;
    } else if (pw > (int32_t)LIVE_PULSE_WIDTH_MAX) {
        pw = LIVE_PULSE_WIDTH_MAX;
    }
    return (uint16_t)pw;
}

static void load_preset(live* l, uint8_t index) {
    const live_preset* p = &presets[index];
    l->preset = index;
    l->cutoff = p->cutoff;
    l->resonance = p->resonance;
    l->pulse_width = p->pulse_width;
    l->lfo_phase = LFO_CENTRE;
    l->pw_out = pw_effective(l);
    for (int v = 0; v < LIVE_VOICES; ++v) {
        if (l->voice[v].gate) {
            l->voice[v].gate = 0;
            l->voice[v].stamp = ++l->clock; /* released now, like a note-off */
        }
    }
}

void live_init(live* l, live_sink_fn sink, void* sink_ctx, sid_model model) {
    l->sink = sink;
    l->sink_ctx = sink_ctx;
    l->model = model;
    l->octave = 0;
    l->master = LIVE_MASTER_MAX;
    l->volume = 15;
    l->clock = 0;
    for (int v = 0; v < LIVE_VOICES; ++v) {
        l->voice[v].note = LIVE_NO_NOTE;
        l->voice[v].gate = 0;
        l->voice[v].stamp = 0;
    }
    load_preset(l, 0);
    put(l, LIVE_REG_RESET, (uint8_t)model);
    write_all(l);
    l->fx_select = 0;
    for (uint32_t k = 0; k < LIVE_SLOTS; ++k) {
        for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; ++f) {
            l->slot[k][f] = 0;
            put(l, (uint8_t)LIVE_REG_SLOT(k, f), 0);
        }
    }
}

/* a happened before b; stamps wrap. */
static int older(uint32_t a, uint32_t b) {
    return (int32_t)(a - b) < 0;
}

static int pick_voice(const live* l, uint8_t gated) {
    int best = -1;
    for (int v = 0; v < voice_count(l); ++v) {
        if (l->voice[v].gate == gated &&
            (best < 0 || older(l->voice[v].stamp, l->voice[best].stamp))) {
            best = v;
        }
    }
    return best;
}

static void start_voice(live* l, int v, uint8_t note) {
    if (l->voice[v].gate) {
        l->voice[v].gate = 0; /* a 0 -> 1 gate edge restarts the attack */
        write_control(l, v);
    }
    l->voice[v].note = note;
    l->voice[v].gate = 1;
    l->voice[v].stamp = ++l->clock;
    write_freq(l, v);
    if (cur(l)->mono_sync) {
        write_freq(l, 2);
    }
    write_control(l, v);
}

void live_note_on(live* l, uint8_t note, uint8_t velocity) {
    if (note > 127) {
        return;
    }
    if (velocity == 0) {
        live_note_off(l, note);
        return;
    }
    int v = -1;
    for (int i = 0; i < voice_count(l); ++i) {
        if (l->voice[i].gate && l->voice[i].note == note) {
            v = i;
        }
    }
    if (v < 0) {
        v = pick_voice(l, 0);
    }
    if (v < 0) {
        v = pick_voice(l, 1);
    }
    start_voice(l, v, note);
}

static void release(live* l, int v) {
    l->voice[v].gate = 0;
    l->voice[v].stamp = ++l->clock;
    write_control(l, v);
}

void live_note_off(live* l, uint8_t note) {
    for (int v = 0; v < voice_count(l); ++v) {
        if (l->voice[v].gate && l->voice[v].note == note) {
            release(l, v);
        }
    }
}

void live_all_notes_off(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        if (l->voice[v].gate) {
            release(l, v);
        }
    }
}

uint8_t live_key_note(const live* l, uint8_t base_note) {
    int32_t n = (int32_t)base_note + 12 * l->octave;
    return base_note <= 127 && n >= 0 && n <= 127 ? (uint8_t)n : LIVE_NO_NOTE;
}

/* now + delta * step, saturated to [0, max]. delta is clamped first so the int32
 * arithmetic cannot overflow (and needs no 64-bit helpers): every range is at most
 * 4095 steps wide, so 4096 detents always reach the far end. */
static uint16_t step_clamped(uint16_t now, int32_t delta, int32_t step, uint16_t max) {
    if (delta > 4096) {
        delta = 4096;
    } else if (delta < -4096) {
        delta = -4096;
    }
    int32_t x = (int32_t)now + delta * step;
    if (x < 0) {
        return 0;
    }
    return x > max ? max : (uint16_t)x;
}

static void set_cutoff(live* l, uint16_t value) {
    if (value != l->cutoff) {
        l->cutoff = value;
        write_cutoff(l);
    }
}

static void set_resonance(live* l, uint8_t value) {
    if (value != l->resonance) {
        l->resonance = value;
        write_res_filt(l);
    }
}

static void update_pw(live* l) {
    uint16_t pw = pw_effective(l);
    if (pw != l->pw_out) {
        l->pw_out = pw;
        write_pw(l);
    }
}

static void set_pulse_width(live* l, uint16_t value) {
    l->pulse_width = value;
    update_pw(l);
}

void live_set_preset(live* l, uint8_t index) {
    if (index >= LIVE_PRESET_COUNT) {
        index = LIVE_PRESET_COUNT - 1;
    }
    load_preset(l, index);
    write_all(l);
}

void live_knob(live* l, live_knob_id knob, int32_t delta) {
    switch (knob) {
    case LIVE_KNOB_CUTOFF:
        set_cutoff(l, step_clamped(l->cutoff, delta, LIVE_CUTOFF_DETENT, LIVE_CUTOFF_MAX));
        break;
    case LIVE_KNOB_RESONANCE:
        set_resonance(l, (uint8_t)step_clamped(l->resonance, delta, 1, LIVE_RESONANCE_MAX));
        break;
    case LIVE_KNOB_PULSE_WIDTH:
        set_pulse_width(l, step_clamped(l->pulse_width, delta, LIVE_PULSE_WIDTH_DETENT,
                                        LIVE_PULSE_WIDTH_MAX));
        break;
    case LIVE_KNOB_PRESET: {
        uint8_t p = (uint8_t)step_clamped(l->preset, delta, 1, LIVE_PRESET_COUNT - 1);
        if (p != l->preset) {
            live_set_preset(l, p);
        }
        break;
    }
    default: break;
    }
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
        l->model = l->model == SID_MODEL_6581 ? SID_MODEL_8580 : SID_MODEL_6581;
        put(l, LIVE_REG_RESET, (uint8_t)l->model);
        write_all(l);
        break;
    default: break;
    }
}

/* value * max / 127, rounded. */
static uint16_t scale7(uint8_t value, uint32_t max) {
    return (uint16_t)((value * max + 63u) / 127u);
}

void live_set_master(live* l, uint16_t value) {
    if (value > LIVE_MASTER_MAX) {
        value = LIVE_MASTER_MAX;
    }
    l->master = value;
    uint8_t vol = (uint8_t)((value * 15u + LIVE_MASTER_MAX / 2u) / LIVE_MASTER_MAX);
    if (vol != l->volume) {
        l->volume = vol;
        write_mode_vol(l);
    }
}

void live_cc(live* l, uint8_t cc, uint8_t value) {
    if (value > 127) {
        return;
    }
    switch (cc) {
    case LIVE_CC_VOLUME: live_set_master(l, scale7(value, LIVE_MASTER_MAX)); break;
    case LIVE_CC_PULSE_WIDTH: set_pulse_width(l, scale7(value, LIVE_PULSE_WIDTH_MAX)); break;
    case LIVE_CC_RESONANCE: set_resonance(l, (uint8_t)(value >> 3)); break;
    case LIVE_CC_CUTOFF: set_cutoff(l, scale7(value, LIVE_CUTOFF_MAX)); break;
    case LIVE_CC_ALL_SOUND_OFF:
    case LIVE_CC_ALL_NOTES_OFF: live_all_notes_off(l); break;
    default:
        if (cc >= LIVE_CC_SLOT_FIRST && cc <= LIVE_CC_SLOT_LAST) {
            uint32_t slot = (cc - LIVE_CC_SLOT_FIRST) / LIVE_SLOT_FIELDS;
            uint32_t field = (cc - LIVE_CC_SLOT_FIRST) % LIVE_SLOT_FIELDS;
            int32_t v = value;
            if (field == LIVE_SLOT_ALGORITHM) {
                v = (int32_t)(value * live_slot_algorithms(slot) / 128u);
            }
            live_slot_set(l, slot, field, v);
        }
        break;
    }
}

uint32_t live_slot_algorithms(uint32_t slot) {
    static const uint8_t COUNT[LIVE_SLOTS] = {SID_DIST_COUNT, SID_MOD_COUNT, SID_FXSLOT_COUNT};
    return slot < LIVE_SLOTS ? COUNT[slot] : 0u;
}

void live_slot_set(live* l, uint32_t slot, uint32_t field, int32_t value) {
    if (slot >= LIVE_SLOTS || field >= LIVE_SLOT_FIELDS) {
        return;
    }
    int32_t max = field == LIVE_SLOT_ALGORITHM ? (int32_t)live_slot_algorithms(slot) - 1
                                               : (int32_t)LIVE_SLOT_MAX;
    value = value < 0 ? 0 : value > max ? max : value;
    if (l->slot[slot][field] != (uint8_t)value) {
        l->slot[slot][field] = (uint8_t)value;
        put(l, (uint8_t)LIVE_REG_SLOT(slot, field), (uint8_t)value);
    }
}

void live_slot_turn(live* l, uint32_t slot, uint32_t field, int32_t detents) {
    if (slot < LIVE_SLOTS && field < LIVE_SLOT_FIELDS && field != LIVE_SLOT_ALGORITHM) {
        live_slot_set(l, slot, field, l->slot[slot][field] + detents * LIVE_SLOT_DETENT);
    }
}

void live_slot_next_algorithm(live* l, uint32_t slot) {
    if (slot < LIVE_SLOTS) {
        uint32_t next = l->slot[slot][LIVE_SLOT_ALGORITHM] + 1u;
        live_slot_set(l, slot, LIVE_SLOT_ALGORITHM,
                      next < live_slot_algorithms(slot) ? (int32_t)next : 0);
    }
}

void live_fx_select_next(live* l) {
    l->fx_select = (uint8_t)((l->fx_select + 1u) % LIVE_SLOTS);
}

void live_tick(live* l) {
    const live_preset* p = cur(l);
    if (!p->pwm_depth) {
        return;
    }
    l->lfo_phase = (uint16_t)(l->lfo_phase + p->pwm_rate);
    update_pw(l);
}
