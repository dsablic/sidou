// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "live.h"
#include "live_dice.h"

#include "fx_live.h"
#include "midi_map.h"

#include <stddef.h>
#include <string.h>

__attribute__((noinline, unused)) static void macro_changed(live* l, uint32_t field);

#define CTRL_GATE 0x01u
#define CTRL_SYNC 0x02u
#define CTRL_RING 0x04u
#define REG_FC_LO 0x15u
#define REG_FC_HI 0x16u
#define REG_RES_FILT 0x17u
#define REG_MODE_VOL 0x18u
#define LFO_CENTRE 16384u
/* The highest note that fits 16 bits, the last of live_freq_top_q4's octave (live_freq.c). */
#define NOTE_TOP 106
/* Expression scales: the cutoff moves 8 steps per controller value (0..1016), the pulse
 * width 16 (0..2032); a vibrato depth of 127 is 127 * 64 = 8128 / 8192 semitone. */
#define CUTOFF_PER_VALUE 8
#define PW_PER_VALUE 16

_Static_assert(LIVE_PARAM_BEND_RANGE == LIVE_PARAM_SLOT_END,
               "every slot field is a parameter, and the expression ones follow");
_Static_assert(LIVE_PITCH_SEMITONE == 8192 && LIVE_BEND_CENTRE == 8192u,
               "a bend times the range is the offset in 1/8192 semitone");

/* M1a's behaviour: velocity off, the wheel on the pulse width, aftertouch off, no vibrato
 * until CC 77 or a target asks for it. */
#define EXPRESSION_DEFAULTS                                                                        \
    .bend_range = LIVE_BEND_RANGE_DEFAULT, .velocity_target = LIVE_VELOCITY_OFF,                   \
    .mod_target = LIVE_MOD_PULSE_WIDTH, .pressure_target = LIVE_PRESSURE_OFF,                      \
    .vibrato_rate = LIVE_VIBRATO_RATE_DEFAULT, .vibrato_depth = 0, .vibrato_delay = 0

static const live_preset presets[LIVE_PRESET_SLOTS] = {
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
        EXPRESSION_DEFAULTS,
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
        EXPRESSION_DEFAULTS,
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
        EXPRESSION_DEFAULTS,
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
        EXPRESSION_DEFAULTS,
    },
    {
        .name = "DICE", /* the base a roll starts from: SAW LEAD's values */
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

_Static_assert(LIVE_PARAM_BLEED - LIVE_PARAM_AGE + 2u == LIVE_CHAR_FIELDS,
               "a parameter per character control, in the fields' order");
_Static_assert(LIVE_REG_CHAR(LIVE_CHAR_FIELDS) == LIVE_REG_CHAR_TICK,
               "the settings, then the tick");

void live_char_store(sid_character* c, uint8_t reg, uint8_t value) {
    if (reg >= LIVE_REG_EXPERT_FIRST && reg < LIVE_REG_EXPERT(SID_EXPERT_COUNT)) {
        sid_character_expert_set(c, SID_MODEL_6581, reg - LIVE_REG_EXPERT_FIRST, value);
        return;
    }
    switch (reg) {
    case LIVE_REG_CHAR(0): c->profile = value < SID_CHIPS ? (uint8_t)(value + 1u) : 0u; break;
    case LIVE_REG_CHAR(1): c->age = value; break;
    case LIVE_REG_CHAR(2): c->dac = value; break;
    case LIVE_REG_CHAR(3): c->thump = value; break;
    case LIVE_REG_CHAR(4): c->bleed = value; break;
    default: break;
    }
}

/* The host's DAC table the chip does not point at (it renders only between the sink's
 * calls, so nothing else can), or NULL without tables. */
static sid_dac* host_free_dac(const live_sid_target* t) {
    if (!t->dacs) {
        return NULL;
    }
    return t->chip->dac == &t->dacs[0] ? &t->dacs[1] : &t->dacs[0];
}

static int host_free_filter(const live_sid_target* t, sid_filter_row rows[2]) {
    if (!t->spare) {
        return 0;
    }
    uint32_t n = 0;
    for (uint32_t i = 0; i < 4u; i++) {
        sid_filter_row row = sid_filter_backing(i < 2u ? t->chip : t->spare, i & 1u);
        int used = row.cutoff == t->chip->filter_rows[0].cutoff ||
                   row.cutoff == t->chip->filter_rows[1].cutoff;
        if (t->ramp.filter_building) {
            used |= row.cutoff == t->ramp.filter_job.rows[0].cutoff ||
                    row.cutoff == t->ramp.filter_job.rows[1].cutoff;
        }
        if (!used) {
            rows[n++] = row;
            if (n == 2u) {
                return 1;
            }
        }
    }
    return 0;
}

int live_sid_prepare(void* ctx, uint8_t profile) {
    live_sid_target* t = ctx;
    const sid_chip_profile* p = sid_chip_profile_get(profile);
    if (!p || !t->spare) {
        t->prep_on = 0;
        return 0;
    }
    int same = t->prep_on && t->prep_profile == profile;
    if (same && sid_character_prep_done(&t->prep)) {
        return 1;
    }
    if (!same) {
        sid_character want = t->ramp.target;
        want.profile = (uint8_t)(profile + 1u);
        t->ramp.building = 0;
        t->ramp.filter_building = 0;
        sid_filter_row rows[2];
        if (!host_free_filter(t, rows)) {
            return 0;
        }
        uint32_t defaults = p->model == SID_MODEL_8580 && !t->default_8580_spent
                                ? SID_DAC_BUILD_STEPS
                                : 0u;
        sid_character_prep_start(&t->prep, t->spare, (sid_model)p->model, t->clock_hz,
                                 t->sample_rate, &want, defaults);
        sid_character_prep_rows(&t->prep, rows);
        t->prep_on = 1;
        t->prep_profile = profile;
    } else {
        sid_character_prep_update(&t->prep, &t->ramp.target);
    }
    t->prep_tick = 1;
    int done = sid_character_prep_step(&t->prep, host_free_dac(t));
    if (done && p->model == SID_MODEL_8580) {
        t->default_8580_spent = 1;
    }
    return done;
}

/* The spare live_sid_prepare made whole for a reset to model and the profile just sent, into
 * the chip (reg_ring.c, take_prepared). */
static int take_prepared(live_sid_target* t, sid_model model) {
    if (!t->prep_on || !sid_character_prep_done(&t->prep) || t->prep.model != (uint8_t)model ||
        t->prep.c.profile != t->ramp.target.profile) {
        return 0;
    }
    /* Borrowed rows may live inside either sid: copying backing arrays would overwrite them. */
    memcpy(t->chip, t->spare, offsetof(sid, cutoff_g));
    memcpy((uint8_t*)t->chip + offsetof(sid, filter_t),
           (const uint8_t*)t->spare + offsetof(sid, filter_t),
           sizeof(sid) - offsetof(sid, filter_t));
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        if (t->chip->osc[v].writeback_keep) {
            t->chip->osc[v].writeback = (const uint32_t (*)[16])t->chip->noise_wb;
        }
    }
    sid_character want = t->ramp.target;
    sid_character_ramp_reset(&t->ramp, &t->prep.c, t->prep.table);
    t->ramp.target = want;
    t->prep_on = 0;
    return 1;
}

void live_sid_sink(void* ctx, uint8_t reg, uint8_t value) {
    live_sid_target* t = ctx;
    if (reg == LIVE_REG_RESET) {
        sid_model model = value == SID_MODEL_8580 ? SID_MODEL_8580 : SID_MODEL_6581;
        if (!take_prepared(t, model)) {
            t->prep_on = 0;
            sid_init(t->chip, model, t->clock_hz, t->sample_rate);
            const sid_dac* table =
                sid_character_apply(t->chip, &t->ramp.target, host_free_dac(t));
            sid_character_ramp_reset(&t->ramp, &t->ramp.target, table);
        }
    } else if ((reg >= LIVE_REG_CHAR(0) && reg < LIVE_REG_CHAR(LIVE_CHAR_FIELDS)) ||
               (reg >= LIVE_REG_EXPERT_FIRST && reg < LIVE_REG_EXPERT(SID_EXPERT_COUNT))) {
        live_char_store(&t->ramp.target, reg, value);
    } else if (reg == LIVE_REG_CHAR_TICK) {
        sid_character_stage st;
        /* No table while preparing, nor in the tick that ran a slice (the last one's
         * included): reg_ring.c's character_tick, a tick standing for a pass. */
        sid_dac* table = t->prep_on || t->prep_tick ? NULL : host_free_dac(t);
        sid_filter_row rows[2];
        int filter_free = !t->prep_on && !t->prep_tick && host_free_filter(t, rows);
        t->prep_tick = 0;
        if (sid_character_ramp_tick_tables(&t->ramp, t->chip, table, filter_free ? rows : NULL, &st)) {
            sid_character_commit(t->chip, &st);
        }
    } else if (reg == LIVE_REG_GATE_RESET) {
        sid_set_gate_reset(t->chip, value);
    } else if (!(reg & 0x80u)) {
        sid_write(t->chip, reg, value);
    }
}

const live_preset* live_preset_get(uint8_t index) {
    return index < LIVE_PRESET_SLOTS ? &presets[index] : NULL;
}

static void put(live* l, uint8_t reg, uint8_t value) {
    l->sink(l->sink_ctx, reg, value);
}

static uint8_t voice_count(const live* l) {
    return l->mono_sync ? 1 : LIVE_VOICES;
}

uint16_t live_freq_at(uint8_t note, int32_t offset) {
    int32_t n = note & 0x7f;
    while (n > NOTE_TOP) {
        n -= 12; /* as the table folds it */
    }
    if (offset == 0) {
        return live_freq_table[n];
    }
    /* Clamp before adding: no overflow for any offset. */
    const int32_t top = NOTE_TOP * LIVE_PITCH_SEMITONE + LIVE_PITCH_SEMITONE - 1;
    if (offset < -top) {
        offset = -top;
    } else if (offset > top) {
        offset = top;
    }
    int32_t p = n * LIVE_PITCH_SEMITONE + offset;
    if (p < 0) {
        p = 0;
    } else if (p > top) {
        p = top;
    }
    uint32_t whole = (uint32_t)p >> 13;
    uint32_t frac = (uint32_t)p & 8191u;
    /* 2^(frac / 8192 / 12) - 1 in Q20: 64 table steps a semitone, linear between them. */
    uint32_t j = frac >> 7, r = frac & 127u;
    uint32_t d = live_freq_fine_q20[j] +
                 (((live_freq_fine_q20[j + 1] - live_freq_fine_q20[j]) * r + 64u) >> 7);
    /* The note's octave of the top one, and its frequency there in 1/16ths (< 2^20). */
    uint32_t below = (uint32_t)NOTE_TOP - whole;
    uint32_t octaves = below / 12u;
    uint32_t b = live_freq_top_q4[11u - below % 12u];
    /* b * d >> 20 in 32 bits: b = hi * 1024 + lo, each product under 2^26. */
    uint32_t prod = (b >> 10) * d + (((b & 1023u) * d + 512u) >> 10);
    uint32_t q4 = b + ((prod + 512u) >> 10);
    uint32_t shift = 4u + octaves;
    uint32_t f = (q4 + (1u << (shift - 1u))) >> shift;
    return f > 0xffffu ? 0xffffu : (uint16_t)f;
}

/* The larger of the channel's pressure and the voice's own. */
static uint8_t voice_pressure(const live* l, int v) {
    uint8_t p = l->voice[v].pressure;
    return l->channel_pressure > p ? l->channel_pressure : p;
}

/* The voice's vibrato depth: the setting, plus the wheel and its pressure where they are
 * assigned to it, up to 127. */
static uint32_t vibrato_depth(const live* l, int v) {
    uint32_t d = l->vibrato_depth;
    if (l->mod_target == LIVE_MOD_VIBRATO) {
        d += l->mod_wheel;
    }
    if (l->pressure_target == LIVE_PRESSURE_VIBRATO) {
        d += voice_pressure(l, v);
    }
    return d > LIVE_EXPRESSION_MAX ? LIVE_EXPRESSION_MAX : d;
}

/* The voice's pitch offset in 1/8192 semitone: the bend, the fine tune and its vibrato, a
 * triangle from 0 (up first) once the delay is over. */
static int32_t pitch_offset(const live* l, int v) {
    int32_t off = ((int32_t)l->bend - (int32_t)LIVE_BEND_CENTRE) * (int32_t)l->bend_range;
    if (l->tune) {
        off += l->tune * LIVE_PITCH_SEMITONE / 100;
    }
    const live_voice* x = &l->voice[v];
    uint32_t depth = vibrato_depth(l, v);
    if (depth && !x->vibrato_wait) {
        int32_t p = x->vibrato_phase;
        int32_t t = p < 16384 ? p : p < 49152 ? 32768 - p : p - 65536; /* -16384..16384 */
        off += t * (int32_t)depth / 256;
    }
    return off;
}

static uint16_t voice_freq(const live* l, int v) {
    int src = l->mono_sync ? 0 : v;
    uint8_t note = l->voice[src].note;
    if (note == LIVE_NO_NOTE || (l->mono_sync && v == 1)) {
        return 0;
    }
    /* The sync lead's voices 1 and 3 take the same pitch, so the ratio holds through a bend
     * and the vibrato. */
    uint32_t f = live_freq_at(note, pitch_offset(l, src));
    if (l->mono_sync && v == 0) {
        f = (f * l->sync_q8) >> 8;
        /* A known limit (docs/reference/controls.md, Presets): at 2.75 times the pitch the
         * register is full from note 90 (F#6) at rest, from lower notes bent or vibrato'd up
         * (88 at +2 semitones). Voice 3 still sets the pitch; the ratio, and the timbre,
         * shrink toward the top. Dropping voice 1 an octave instead would jump the timbre
         * mid-bend, so it saturates. */
        if (f > 0xffffu) {
            f = 0xffffu;
        }
    }
    return (uint16_t)f;
}

static uint8_t voice_control(const live* l, int v) {
    if (l->mono_sync && v != 0) {
        return 0; /* voice 3 only clocks the sync; voice 2 is unused */
    }
    return (uint8_t)(l->control | (l->voice[v].gate ? CTRL_GATE : 0u));
}

static void write_freq(live* l, int v) {
    uint16_t f = voice_freq(l, v);
    l->voice[v].freq_out = f;
    put(l, (uint8_t)(v * 7), (uint8_t)f);
    put(l, (uint8_t)(v * 7 + 1), (uint8_t)(f >> 8));
}

static void write_control(live* l, int v) {
    put(l, (uint8_t)(v * 7 + 4), voice_control(l, v));
}

static uint16_t voice_pw(const live* l, int v);
static uint16_t cutoff_effective(const live* l);

static void write_pw(live* l, int v) {
    uint16_t pw = voice_pw(l, v);
    l->voice[v].pw_out = pw;
    put(l, (uint8_t)(v * 7 + 2), (uint8_t)pw);
    put(l, (uint8_t)(v * 7 + 3), (uint8_t)(pw >> 8));
}

static void write_cutoff_as(live* l, uint16_t c) {
    l->cutoff_out = c;
    put(l, REG_FC_LO, (uint8_t)(c & 7u));
    put(l, REG_FC_HI, (uint8_t)(c >> 3));
}

static void write_cutoff(live* l) {
    write_cutoff_as(l, cutoff_effective(l));
}

/* The voice's sustain/release: the sustain level scaled by its velocity when that is the
 * velocity's target (127 keeps it). */
static uint8_t voice_sr(const live* l, int v) {
    if (l->velocity_target != LIVE_VELOCITY_SUSTAIN) {
        return l->sustain_release;
    }
    uint32_t s = ((uint32_t)(l->sustain_release >> 4) * l->voice[v].velocity + 63u) / 127u;
    return (uint8_t)(s << 4 | (l->sustain_release & 0x0fu));
}

static void write_res_filt(live* l) {
    put(l, REG_RES_FILT, (uint8_t)(l->resonance << 4 | l->filter_voices));
}

static void write_mode_vol(live* l) {
    put(l, REG_MODE_VOL, (uint8_t)(l->filter_mode | LIVE_CHIP_VOLUME));
}

static void write_envelopes(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        put(l, (uint8_t)(v * 7 + 5), l->attack_decay);
        put(l, (uint8_t)(v * 7 + 6), voice_sr(l, v));
    }
}

static void write_controls(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        write_control(l, v);
    }
}

static void write_gate_reset(live* l) {
    put(l, LIVE_REG_GATE_RESET, l->envelope_mode == LIVE_ENVELOPE_INSTANT ? 1u : 0u);
}

/* The character's settings: the profile and each control, as sent from now on. */
static void write_character(live* l) {
    put(l, LIVE_REG_CHAR(0), l->profile);
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; ++f) {
        put(l, (uint8_t)LIVE_REG_CHAR(f + 1u), l->character[f]);
        l->character_sent[f] = l->character[f];
    }
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        if (l->expert[f] != l->expert_sent[f]) {
            put(l, (uint8_t)LIVE_REG_EXPERT(f), l->expert[f]);
            l->expert_sent[f] = l->expert[f];
        }
    }
}

/* A chip reset with the model, after the character's settings (the sink gives them to the
 * new chip), then the envelope mode, which sid_init turned off: before any register, so
 * before any gate. */
static void reset_chip(live* l) {
    write_character(l);
    put(l, LIVE_REG_RESET, (uint8_t)l->model);
    write_gate_reset(l);
}

/* Every register, envelopes before gates. */
static void write_all(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        put(l, (uint8_t)(v * 7 + 5), l->attack_decay);
        put(l, (uint8_t)(v * 7 + 6), voice_sr(l, v));
        write_freq(l, v);
        write_pw(l, v);
        write_control(l, v);
    }
    write_cutoff(l);
    write_res_filt(l);
    write_mode_vol(l);
}

/* The pulse width after the LFO, clamped to 12 bits. The LFO's depth narrows near the ends of
 * the range, so that its sweep never goes thinner than K3's narrowest width (LIVE_PULSE_FIRST),
 * nor as thin at the wide end: clamped to 0, a width is no pulse but a DC level, and the voice
 * went silent for part of every cycle (PWM BASS at a 9 % width, a quarter of its 2 s cycle). */
static uint16_t pw_effective(const live* l) {
    int32_t pw = l->pulse_width;
    int32_t depth = l->pwm_depth;
    int32_t room = pw - LIVE_PULSE_FIRST;
    int32_t top = (int32_t)LIVE_PULSE_WIDTH_MAX + 1 - LIVE_PULSE_FIRST - pw;
    room = room < top ? room : top;
    depth = depth < room ? depth : room;
    if (depth > 0) {
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

static uint16_t clamp_u16(int32_t x, int32_t max) {
    return (uint16_t)(x < 0 ? 0 : x > max ? max : x);
}

/* The voice's pulse width: after the LFO, raised by its pressure, then narrowed by its
 * velocity (the distance to the nearer edge scaled, so above 2048 it narrows from the top). */
static uint16_t voice_pw(const live* l, int v) {
    int32_t pw = pw_effective(l);
    if (l->pressure_target == LIVE_PRESSURE_PULSE_WIDTH) {
        pw = clamp_u16(pw + voice_pressure(l, v) * PW_PER_VALUE, (int32_t)LIVE_PULSE_WIDTH_MAX);
    }
    if (l->velocity_target == LIVE_VELOCITY_PULSE_WIDTH) {
        int32_t edge = pw <= 2048 ? pw : 4096 - pw;
        int32_t d = (edge * l->voice[v].velocity + 63) / 127;
        pw = clamp_u16(pw <= 2048 ? d : 4096 - d, (int32_t)LIVE_PULSE_WIDTH_MAX);
    }
    return (uint16_t)pw;
}

/* The pressure of the most pressed held voice, or the channel's if more. */
static uint8_t most_pressure(const live* l) {
    uint8_t p = l->channel_pressure;
    for (int v = 0; v < LIVE_VOICES; ++v) {
        if (l->voice[v].gate && l->voice[v].pressure > p) {
            p = l->voice[v].pressure;
        }
    }
    return p;
}

/* The cutoff written: the parameter, opened by the wheel and the pressure, lowered by the
 * last note's velocity, where they are assigned to it. One filter for every voice. */
static uint16_t cutoff_effective(const live* l) {
    int32_t c = l->cutoff;
    if (l->mod_target == LIVE_MOD_CUTOFF) {
        c += l->mod_wheel * CUTOFF_PER_VALUE;
    }
    if (l->pressure_target == LIVE_PRESSURE_CUTOFF) {
        c += most_pressure(l) * CUTOFF_PER_VALUE;
    }
    if (l->velocity_target == LIVE_VELOCITY_CUTOFF) {
        c -= (int32_t)(LIVE_EXPRESSION_MAX - l->last_velocity) * CUTOFF_PER_VALUE;
    }
    return clamp_u16(c, (int32_t)LIVE_CUTOFF_MAX);
}

/* Write only what changed: the frequencies, pulse widths and cutoff the controllers, the
 * vibrato and the LFO move. */
static void update_freq(live* l, int v) {
    if (voice_freq(l, v) != l->voice[v].freq_out) {
        write_freq(l, v);
    }
}

static void update_pw(live* l, int v) {
    if (voice_pw(l, v) != l->voice[v].pw_out) {
        write_pw(l, v);
    }
}

static void update_pws(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        update_pw(l, v);
    }
}

static void update_cutoff(live* l) {
    uint16_t c = cutoff_effective(l); /* once: a K1 detent pays for it on the input path */
    if (c != l->cutoff_out) {
        write_cutoff_as(l, c);
    }
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
        l->bend_range = p->bend_range; /* RPN 0's is the channel's: kept */
    }
    l->velocity_target = p->velocity_target;
    if (l->mod_target != p->mod_target) {
        l->mod_wheel = 0; /* a new target starts from 0 */
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
    l->lfo_phase = LFO_CENTRE;
    /* The arpeggio: the factory's, off, with every note released below. */
    l->arp_on = 0;
    l->arp_rate = LIVE_ARP_RATE_DEFAULT;
    l->arp_order = LIVE_ARP_UP;
    l->arp_span = 1;
    l->arp_n = 0;
    l->arp_voice = LIVE_ARP_NO_VOICE;
    for (int v = 0; v < LIVE_VOICES; ++v) {
        if (l->voice[v].gate) {
            l->voice[v].gate = 0;
            l->voice[v].stamp = ++l->clock; /* released now, like a note-off */
        }
    }
    /* The base: the preset as loaded, the slots as they are (a built-in has none of its own). */
    l->user_sound = 0;
    live_sound_get(l, l->base);
    l->base[LAB_SOUND_BEND_RANGE] = p->bend_range; /* not RPN 0's, the channel's */
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
    memset(l->slot, 0, sizeof l->slot); /* for the base load_preset takes; sent below */
    load_preset(l, 0);
    reset_chip(l);
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
        if (l->voice[v].gate == gated && v + 1 != l->ui_voice &&
            (best < 0 || older(l->voice[v].stamp, l->voice[best].stamp))) {
            best = v;
        }
    }
    return best;
}

static void start_voice(live* l, int v, uint8_t note, uint8_t velocity) {
    live_voice* x = &l->voice[v];
    if (x->gate) {
        x->gate = 0; /* a 0 -> 1 gate edge restarts the attack */
        write_control(l, v);
    }
    x->note = note;
    x->gate = 1;
    x->stamp = ++l->clock;
    x->velocity = velocity;
    x->pressure = 0;
    x->vibrato_phase = 0;
    x->vibrato_wait = l->vibrato_delay;
    l->last_velocity = velocity;
    /* Velocity: the sustain level before the gate, the pulse width, or the filter. */
    if (l->velocity_target == LIVE_VELOCITY_SUSTAIN) {
        put(l, (uint8_t)(v * 7 + 6), voice_sr(l, v));
    }
    update_pw(l, v);
    update_cutoff(l);
    write_freq(l, v);
    if (l->mono_sync) {
        write_freq(l, 2);
    }
    write_control(l, v);
}

/* A voice for a new note: a free one (the longest released), the UI sound's when the others are
 * all held, else the one held longest. */
static int free_voice(live* l) {
    int v = pick_voice(l, 0);
    if (v < 0 && l->ui_voice) {
        /* The others all held: a UI sound gives its voice back (app/ui_sound.h), with the
         * instrument's envelope (the sound had its own). */
        v = l->ui_voice - 1;
        l->ui_voice = 0;
        put(l, (uint8_t)(v * 7 + 5), l->attack_decay);
        put(l, (uint8_t)(v * 7 + 6), voice_sr(l, v));
    }
    if (v < 0) {
        v = pick_voice(l, 1);
    }
    return v;
}

static void arp_note_on(live* l, uint8_t note, uint8_t velocity);
static void arp_note_off(live* l, uint8_t note);

void live_note_on(live* l, uint8_t note, uint8_t velocity) {
    if (note > 127) {
        return;
    }
    if (velocity == 0) {
        live_note_off(l, note);
        return;
    }
    if (l->arp_on) {
        arp_note_on(l, note, velocity > 127 ? 127 : velocity);
        return;
    }
    int v = -1;
    for (int i = 0; i < voice_count(l); ++i) {
        if (l->voice[i].gate && l->voice[i].note == note) {
            v = i;
        }
    }
    if (v < 0) {
        v = free_voice(l);
    }
    start_voice(l, v, note, velocity > 127 ? 127 : velocity);
    l->note_ons++;
}

static void release(live* l, int v) {
    l->voice[v].gate = 0;
    l->voice[v].stamp = ++l->clock;
    write_control(l, v);
}

void live_note_off(live* l, uint8_t note) {
    if (l->arp_on) {
        arp_note_off(l, note);
        return;
    }
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
    l->arp_n = 0;
    l->arp_voice = LIVE_ARP_NO_VOICE;
}

/* ---- the chord arpeggio ---- */

const uint8_t live_arp_steps[LIVE_ARP_RATES] = {1, 2, 3, 4, 6, 8, 3, 6, 12};

/* Positions in a cycle: the held notes times the span, and up-down back without its ends. */
static uint32_t arp_period(const live* l) {
    uint32_t n = (uint32_t)l->arp_n * l->arp_span;
    return l->arp_order == LIVE_ARP_UP_DOWN && n > 1u ? 2u * n - 2u : n;
}

/* The note at position pos (< arp_period) of the cycle. */
static uint8_t arp_note_at(const live* l, uint32_t pos) {
    uint32_t n = (uint32_t)l->arp_n * l->arp_span;
    uint32_t q = pos;
    if (l->arp_order == LIVE_ARP_UP_DOWN && q >= n) {
        q = 2u * n - 2u - q;
    } else if (l->arp_order == LIVE_ARP_DOWN) {
        q = n - 1u - q;
    }
    uint32_t octave = q / l->arp_n;
    uint32_t k = q % l->arp_n;
    uint32_t note = l->arp_order == LIVE_ARP_PLAYED ? l->arp_held[k] : l->arp_sorted[k];
    uint32_t up = note + 12u * octave;
    return (uint8_t)(up <= 127u ? up : note);
}

/* The step's length: ticks, and with a clock rate the clocks. */
static void arp_rewind(live* l) {
    l->arp_wait = (uint8_t)(live_arp_steps[l->arp_rate] * LIVE_ARP_TICKS_PER_FRAME);
    l->arp_clocks = live_arp_steps[l->arp_rate];
}

/* The arpeggio's voice to the cycle's note at its position: the frequency alone, the gate held
 * (the sync lead's voice 3 sets its pitch: both move). */
static void arp_sound(live* l) {
    int v = l->arp_voice;
    l->voice[v].note = arp_note_at(l, l->arp_pos);
    update_freq(l, v);
    if (l->mono_sync) {
        update_freq(l, 2);
    }
}

static void arp_step(live* l) {
    l->arp_pos = (uint8_t)((l->arp_pos + 1u) % arp_period(l));
    arp_sound(l);
}

/* After the notes or the cycle changed: the position of the note that sounds, looking on from
 * where it was (up-down has most notes twice: the direction holds); if it is gone, the position
 * before where it was, so the next step takes what followed it. */
LIVE_COLD static void arp_resync(live* l) {
    uint32_t period = arp_period(l);
    uint32_t pos = l->arp_pos % period;
    uint8_t sounding = l->voice[l->arp_voice].note;
    for (uint32_t k = 0; k < period; k++) {
        uint32_t p = (pos + k) % period;
        if (arp_note_at(l, p) == sounding) {
            l->arp_pos = (uint8_t)p;
            return;
        }
    }
    l->arp_pos = (uint8_t)((pos + period - 1u) % period);
}

/* note into the held notes: last in the order played, in its place among the sorted. */
static void arp_hold(live* l, uint8_t note) {
    uint32_t n = l->arp_n;
    uint32_t i = n;
    l->arp_held[n] = note;
    while (i > 0u && l->arp_sorted[i - 1u] > note) {
        l->arp_sorted[i] = l->arp_sorted[i - 1u];
        i--;
    }
    l->arp_sorted[i] = note;
    l->arp_n = (uint8_t)(n + 1u);
}

/* note out of both lists; 0 if it was not held. */
static int arp_drop(live* l, uint8_t note) {
    uint32_t n = l->arp_n;
    int found = 0;
    for (uint32_t i = 0, j = 0; i < n; i++) {
        if (l->arp_held[i] == note) {
            found = 1;
        } else {
            l->arp_held[j++] = l->arp_held[i];
        }
    }
    if (!found) {
        return 0;
    }
    for (uint32_t i = 0, j = 0; i < n; i++) {
        if (l->arp_sorted[i] != note) {
            l->arp_sorted[j++] = l->arp_sorted[i];
        }
    }
    l->arp_n = (uint8_t)(n - 1u);
    return 1;
}

/* The first note held starts the voice on the cycle's first note; another joins the cycle, which
 * keeps its time. A note held already is held once. */
static void arp_note_on(live* l, uint8_t note, uint8_t velocity) {
    for (uint32_t i = 0; i < l->arp_n; i++) {
        if (l->arp_held[i] == note) {
            return;
        }
    }
    if (l->arp_n == LIVE_ARP_HELD) {
        (void)arp_drop(l, l->arp_held[0]); /* the oldest goes */
    }
    arp_hold(l, note);
    if (l->arp_voice != LIVE_ARP_NO_VOICE) {
        arp_resync(l);
        return;
    }
    int v = free_voice(l);
    l->arp_voice = (uint8_t)v;
    l->arp_pos = 0;
    arp_rewind(l);
    start_voice(l, v, arp_note_at(l, 0), velocity);
    l->note_ons++;
}

/* The last note let go releases the voice (its release rings at the pitch it had). */
static void arp_note_off(live* l, uint8_t note) {
    if (!arp_drop(l, note) || l->arp_voice == LIVE_ARP_NO_VOICE) {
        return;
    }
    if (!l->arp_n) {
        release(l, l->arp_voice);
        l->arp_voice = LIVE_ARP_NO_VOICE;
        return;
    }
    arp_resync(l);
}

/* ON: the held voices' notes, oldest first, into the arpeggio, on the oldest one's voice. */
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
                arp_hold(l, l->voice[v].note);
            }
        }
        if (keep < 0) {
            keep = v;
            l->voice[v].gate = 0; /* out of the search; its gate is back on below, unwritten */
        } else {
            release(l, v);
        }
    }
    if (keep < 0) {
        return;
    }
    l->voice[keep].gate = 1;
    l->arp_voice = (uint8_t)keep;
    l->arp_pos = 0;
    arp_rewind(l);
    arp_sound(l);
}

/* OFF: each held note on a voice of its own, as if played again in the order played. */
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
    release(l, l->arp_voice);
    l->arp_voice = LIVE_ARP_NO_VOICE;
    l->arp_n = 0;
    for (uint32_t i = 0; i < n; i++) {
        live_note_on(l, notes[i], velocity);
    }
}

LIVE_COLD void live_arp_set(live* l, uint32_t field, uint32_t value) {
    l->settings_dirty = 1; /* the sound's: the unsaved edit keeps it */
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
            arp_rewind(l);
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
                arp_resync(l);
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

void live_arp_realtime(live* l, uint8_t status) {
    if (status == 0xF8u) {
        l->arp_clock_seen = LIVE_ARP_CLOCK_TICKS;
        if (l->arp_voice != LIVE_ARP_NO_VOICE && l->arp_rate >= LIVE_ARP_RATE_CLOCK &&
            --l->arp_clocks == 0u) {
            l->arp_clocks = live_arp_steps[l->arp_rate];
            arp_step(l);
        }
    } else if (status == 0xFAu) {
        l->arp_clocks = live_arp_steps[l->arp_rate];
    }
}

/* A tick: a step when its ticks are up, unless a clock rate has a clock to step it. */
static void arp_tick(live* l) {
    if (l->arp_clock_seen) {
        l->arp_clock_seen--;
        if (l->arp_rate >= LIVE_ARP_RATE_CLOCK) {
            return;
        }
    }
    if (l->arp_voice != LIVE_ARP_NO_VOICE && --l->arp_wait == 0u) {
        l->arp_wait = (uint8_t)(live_arp_steps[l->arp_rate] * LIVE_ARP_TICKS_PER_FRAME);
        arp_step(l);
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
        l->settings_dirty = 1;
        update_cutoff(l);
    }
}

static void set_resonance(live* l, uint8_t value) {
    if (value != l->resonance) {
        l->resonance = value;
        l->settings_dirty = 1;
        write_res_filt(l);
    }
}

static void set_pulse_width(live* l, uint16_t value) {
    l->settings_dirty = 1;
    l->pulse_width = value;
    update_pws(l);
}

void live_set_preset(live* l, uint8_t index) {
    l->settings_dirty = 1;
    if (index >= LIVE_PRESET_SLOTS) {
        index = LIVE_PRESET_SLOTS - 1; /* the settings load DICE back */
    }
    load_preset(l, index);
    l->edits[index].valid = 0; /* the factory's, over any edits it kept */
    write_all(l);
}

/* The current settings of the fields live_edit keeps. */
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
    e->valid = 1;
}

/* e over what load_preset has just loaded (a bend range RPN 0 set stays; a new mod target starts
 * the wheel at 0, as load_preset does). */
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
    l->arp_on = e->arp_on; /* no note is held after load_preset: the next one starts it */
    l->arp_rate = e->arp_rate;
    l->arp_order = e->arp_order;
    l->arp_span = e->arp_span;
    l->sync_semis = e->sync_semis;
    l->sync_q8 = (uint16_t)((l->sync_base_q8 * live_ratio_q16[e->sync_semis + LIVE_RATIO_SEMIS] +
                             32768u) >> 16);
    l->pwm_depth = e->pwm_depth;
    l->pwm_rate = e->pwm_rate;
    l->tune = e->tune;
}

/* A USER sound is left: its edit, if it has one, is kept for it (live.user_edit), over the one
 * kept for the USER sound left before it. A USER sound's preset is SAW LEAD's (live_load_user):
 * its edits are never SAW LEAD's. */
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
            live_sound_get(l, l->base); /* DICE's base is its roll, as it was left */
        }
    }
    write_all(l);
}

uint32_t live_preset_edited(const live* l) {
    if (l->preset == LIVE_PRESET_DICE) {
        return 0;
    }
    uint8_t now[LAB_SOUND_BYTES];
    live_sound_get(l, now);
    if (l->bend_range_rpn) {
        now[LAB_SOUND_BEND_RANGE] = l->base[LAB_SOUND_BEND_RANGE]; /* the channel's, not the sound's */
    }
    return !lab_sound_equal(now, l->base);
}

LIVE_COLD void live_preset_name(const live* l, char* buf) {
    const char* name = l->base_name;
    uint32_t n = 0;
    if (l->preset == LIVE_PRESET_DICE) {
        /* DICE and the number of the roll it holds, two digits */
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
}

static void put16(uint8_t* s, uint32_t at, uint32_t v) {
    s[at] = (uint8_t)v;
    s[at + 1u] = (uint8_t)(v >> 8);
}

static uint32_t get16(const uint8_t* s, uint32_t at) {
    return (uint32_t)s[at] | (uint32_t)s[at + 1u] << 8;
}

/* Straight-line: live_preset_edited packs the sound for every look of LIVE's header. */
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
}

/* The parameters a sound sets through live_param_set, and where it holds each: (offset, mask,
 * shift) of the byte or (offset, 0, 16) of a 16-bit field. */
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

LIVE_COLD void live_sound_apply(live* l, const uint8_t in[LAB_SOUND_BYTES]) {
    l->settings_dirty = 1;
    uint8_t mono = in[LAB_SOUND_MONO_SYNC] ? 1u : 0u;
    if (mono != l->mono_sync) {
        live_all_notes_off(l); /* the voices change roles: none is left sounding */
        l->mono_sync = mono;
        write_controls(l);
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
    for (uint32_t i = 0; i < sizeof SOUND_PARAMS / sizeof SOUND_PARAMS[0]; i++) {
        uint32_t at = SOUND_PARAMS[i].at;
        uint32_t v = SOUND_PARAMS[i].shift == 16u
                         ? get16(in, at)
                         : (uint32_t)(in[at] & SOUND_PARAMS[i].mask) >> SOUND_PARAMS[i].shift;
        live_param_set(l, SOUND_PARAMS[i].param, v);
    }
    /* a bend range RPN 0 set holds unless the sound's differs (live_param_set ends it) */
    if (in[LAB_SOUND_BEND_RANGE] != l->bend_range) {
        live_param_set(l, LIVE_PARAM_BEND_RANGE, in[LAB_SOUND_BEND_RANGE]);
    }
    /* the arpeggio's settings, then on or off: a held chord goes into it at the sound's rate */
    for (uint32_t f = LIVE_ARP_SPAN; f > LIVE_ARP_ON; f--) {
        live_arp_set(l, f, in[LAB_SOUND_ARP + f]);
    }
    live_arp_set(l, LIVE_ARP_ON, in[LAB_SOUND_ARP + LIVE_ARP_ON]);
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; f++) {
            live_slot_set(l, k, f, in[LAB_SOUND_SLOTS + k * LIVE_SLOT_FIELDS + f]);
        }
    }
    /* the pitch, the ratio and the LFO change no parameter: written as a tick writes them */
    for (int v = 0; v < LIVE_VOICES; ++v) {
        update_freq(l, v);
    }
    update_pws(l);
}

LIVE_COLD void live_load_user(live* l, uint32_t n, const uint8_t sound[LAB_SOUND_BYTES],
                              const char* name) {
    l->settings_dirty = 1;
    load_preset(l, 0); /* the frame: the voices released, the edits kept for SAW LEAD stay */
    live_sound_apply(l, sound);
    live_sound_set_base(l, sound, name);
    l->user_sound = (uint8_t)(n + 1u);
    write_all(l);
}

LIVE_COLD void live_sound_set_base(live* l, const uint8_t in[LAB_SOUND_BYTES], const char* name) {
    l->settings_dirty = 1; /* the unsaved edit holds the name and the mark */
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

uint32_t live_tone(const live* l) {
    if (l->mono_sync) {
        return LIVE_TONE_RATIO;
    }
    return l->control & 0x40u ? LIVE_TONE_PULSE : LIVE_TONE_DECAY;
}

/* K3: the sync ratio, the pulse width or the decay, by the sound (live_tone). */
__attribute__((noinline, unused)) LIVE_COLD static void tone_turn(live* l, int32_t delta) {
    delta = delta > 64 ? 64 : delta < -64 ? -64 : delta;
    switch (live_tone(l)) {
    case LIVE_TONE_RATIO: {
        int32_t semis = l->sync_semis + delta;
        semis = semis < -LIVE_RATIO_SEMIS ? -LIVE_RATIO_SEMIS
                : semis > LIVE_RATIO_SEMIS ? LIVE_RATIO_SEMIS
                                           : semis;
        l->sync_semis = (int8_t)semis;
        l->sync_q8 = (uint16_t)((l->sync_base_q8 * live_ratio_q16[semis + LIVE_RATIO_SEMIS] +
                                 32768u) >> 16);
        l->settings_dirty = 1; /* the sound's: the unsaved edit keeps it */
        update_freq(l, 0);
        break;
    }
    case LIVE_TONE_PULSE: {
        /* The grid's positions, one side of the square: a width off it, or above a half (MIDI),
         * is mirrored and taken to the nearest position first. */
        int32_t pw = l->pulse_width > 2048u ? 4096 - (int32_t)l->pulse_width : (int32_t)l->pulse_width;
        int32_t pos = (pw - LIVE_PULSE_FIRST + LIVE_PULSE_STEP / 2) / LIVE_PULSE_STEP;
        pos = pw < LIVE_PULSE_FIRST ? 0 : pos;
        pos += delta;
        pos = pos < 0 ? 0 : pos > LIVE_PULSE_POSITIONS - 1 ? LIVE_PULSE_POSITIONS - 1 : pos;
        set_pulse_width(l, (uint16_t)(LIVE_PULSE_FIRST + LIVE_PULSE_STEP * pos));
        break;
    }
    default:
        live_param_set(l, LIVE_PARAM_DECAY, step_clamped(l->attack_decay & 15u, delta, 1, 15));
        break;
    }
}

void live_knob(live* l, live_knob_id knob, int32_t delta) {
    switch (knob) {
    case LIVE_KNOB_CUTOFF: {
        /* 64 detents along live_cutoff_steps, even in pitch. The table rises strictly, so from
         * an entry a turn lands on a different one (set_cutoff ignores the same, at an end). */
        const uint16_t* t = live_cutoff_steps[l->model];
        int32_t pos = l->cutoff_pos;
        if (t[pos] != l->cutoff) {
            /* A cutoff the table does not hold (MIDI, a preset, the model): start just before
             * the first entry beyond it in the turn's direction (the buckets of live_cutoff_up
             * and _down). With none that way, those give 64 and 0: the cutoff stays. */
            uint32_t b = l->cutoff >> 5;
            pos = delta > 0 ? live_cutoff_up[l->model][b] - 1 : live_cutoff_down[l->model][b] + 1;
            if (pos == (delta > 0 ? 64 : 0)) {
                break;
            }
        }
        pos += delta > 128 ? 128 : delta < -128 ? -128 : delta;
        pos = pos < 0 ? 0 : pos > 64 ? 64 : pos;
        l->cutoff_pos = (uint8_t)pos;
        set_cutoff(l, t[pos]);
        break;
    }
    case LIVE_KNOB_RESONANCE:
        set_resonance(l, (uint8_t)step_clamped(l->resonance, delta, 1, LIVE_RESONANCE_MAX));
        break;
    case LIVE_KNOB_TONE:
        tone_turn(l, delta);
        break;
    case LIVE_KNOB_RELEASE:
        live_param_set(l, LIVE_PARAM_RELEASE,
                       step_clamped(l->sustain_release & 15u, delta, 1, 15));
        break;
    default: break;
    }
}

/* What a roll starts from: the notes played, the clock and the LFO's phase are what no two turns
 * share (the roll adds its own count). */
static uint32_t dice_seed(const live* l) {
    return l->note_ons * 2654435761u ^ l->clock * 40503u ^ l->lfo_phase;
}

/* The sounds PRESETS steps through, in order: the built-ins and DICE (0..LIVE_PRESET_SLOTS - 1),
 * then USER 1.. at LIVE_PRESET_SLOTS + n. */
#define STEP_END (LIVE_PRESET_SLOTS + LIVE_SOUNDS)

LIVE_COLD void live_preset_step(live* l, int32_t steps, int revert) {
    if (revert) {
        if (l->user_sound) {
            live_sound_apply(l, l->base); /* the sound as kept */
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
        do { /* a detent on, past the USER slots that hold nothing */
            n = steps < 0 ? n - 1u : n + 1u;
        } while (n >= LIVE_PRESET_SLOTS && n < STEP_END &&
                 !(l->sounds_valid >> (n - LIVE_PRESET_SLOTS) & 1u));
        if (n >= STEP_END) { /* the end, either way (below 0 wraps) */
            break;
        }
        to = n;
    }
    if (to == at) {
        if (at == LIVE_PRESET_DICE) {
            live_dice_roll(l, dice_seed(l)); /* DICE again: a new roll */
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
                live_sound_apply(l, l->user_edit); /* as it was left; the base stays */
            }
        } else { /* the store did not give it: not a sound, and the one playing stays */
            l->sounds_valid = (uint16_t)(l->sounds_valid & ~(1u << n));
            if (!l->user_sound) {
                edit_apply(l, &l->edits[l->preset]);
            }
        }
    }
}

static sid_model profile_model(uint32_t profile) {
    return (sid_model)sid_chip_profile_get(profile)->model;
}

/* Records a profile asked for: the model follows it, and a 6581 one is the one to go back
 * to. */
static void request_profile(live* l, uint32_t profile) {
    l->settings_dirty = 1;
    l->profile_req = (uint8_t)profile;
    l->model_req = profile_model(profile);
    if (l->model_req == SID_MODEL_6581) {
        l->profile_6581 = (uint8_t)profile;
    }
}

/* USER slot n set as the keys' setup: its base profile as a profile change (the spare prepared,
 * then swapped), its controls as the character's. An empty slot does nothing. Cold: picked
 * by hand or by a CC, never per frame. */
__attribute__((noinline, unused)) static void apply_user(live* l, uint32_t n) {
    if (n >= LIVE_USER_PROFILES || !(l->user_valid >> n & 1u)) {
        return;
    }
    request_profile(l, l->user_base[n]);
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

/* The profile for a model: the 8580's, or the last 6581 one asked for. */
static uint32_t model_profile(const live* l, sid_model model) {
    return model == SID_MODEL_8580 ? sid_chip_default(SID_MODEL_8580) : l->profile_6581;
}

/* A reset of the chip with the profile and its model, then every register: every parameter
 * is kept. It also settles any pending request (profile_req, model_req). */
static void set_profile(live* l, uint32_t profile) {
    request_profile(l, profile);
    l->profile = (uint8_t)profile;
    l->model = l->model_req;
    reset_chip(l);
    write_all(l);
}

static void set_model(live* l, sid_model model) {
    set_profile(l, model_profile(l, model));
}

/* The character's controls that changed since they were last sent, then the tick, every
 * tick: the sink ramps the chip towards them a tick at a time (sid_character_ramp_tick). */
static void update_character(live* l) {
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; ++f) {
        if (l->character[f] != l->character_sent[f]) {
            put(l, (uint8_t)LIVE_REG_CHAR(f + 1u), l->character[f]);
            l->character_sent[f] = l->character[f];
        }
    }
    put(l, LIVE_REG_CHAR_TICK, 0);
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
        if (l->prepare) { /* the other model than the one asked for: live_tick prepares it */
            sid_model m = l->model_req == SID_MODEL_6581 ? SID_MODEL_8580 : SID_MODEL_6581;
            l->user_applied = 0;
            request_profile(l, model_profile(l, m));
        } else { /* at once, at a human's pace; over any request still pending */
            l->user_applied = 0;
            set_model(l, l->model == SID_MODEL_6581 ? SID_MODEL_8580 : SID_MODEL_6581);
        }
        break;
    default: break;
    }
}

void live_set_master(live* l, uint16_t value) {
    if (value > LIVE_MASTER_MAX) {
        value = LIVE_MASTER_MAX;
    }
    /* A digital gain on the final output (app/master_gain.h; the main loop hands l->master to
     * the audio path): the chip's volume register stays at LIVE_CHIP_VOLUME, so a song's own
     * writes to it (a fade, the 6581's volume digi trick) keep their meaning relative to MASTER. */
    l->master = value;
    l->volume = (uint8_t)((value * 100u + LIVE_MASTER_MAX / 2u) / LIVE_MASTER_MAX);
}

void live_cc(live* l, uint8_t cc, uint8_t value) {
    if (value > 127) {
        return;
    }
    if (cc == LIVE_CC_ALL_SOUND_OFF || cc == LIVE_CC_ALL_NOTES_OFF) {
        live_all_notes_off(l);
        return;
    }
    if (cc == LIVE_CC_RESET_ALL) {
        live_reset_controllers(l);
        return;
    }
    if ((uint32_t)cc - LIVE_CC_ARP <= LIVE_CC_ARP_SPAN - LIVE_CC_ARP) {
        /* the arpeggio's: on from 64, the rest scaled over their values */
        static const uint8_t VALUES[4] = {2, LIVE_ARP_RATES, 4, LIVE_ARP_SPAN_MAX};
        uint32_t f = (uint32_t)cc - LIVE_CC_ARP;
        live_arp_set(l, f, (uint32_t)value * VALUES[f] / 128u + (f == LIVE_ARP_SPAN));
        return;
    }
    if (cc == LIVE_CC_MOD_WHEEL && l->mod_target != LIVE_MOD_PULSE_WIDTH) {
        l->mod_wheel = value; /* live_tick applies it */
        return;
    }
    const midi_map_entry* e = midi_map_for_cc(cc);
    if (e && e->param != MIDI_MAP_RESERVED) {
        live_param_set(l, e->param, midi_map_from_cc(e, value));
    }
}

/* The SID parameters' bits. */
#define CTRL_WAVEFORM 0xf0u

void live_bend(live* l, uint16_t bend) {
    l->bend = bend > LIVE_BEND_MAX ? (uint16_t)LIVE_BEND_MAX : bend;
}

void live_pressure(live* l, uint8_t pressure) {
    l->channel_pressure = pressure > 127 ? 127 : pressure;
}

void live_poly_pressure(live* l, uint8_t note, uint8_t pressure) {
    for (int v = 0; v < voice_count(l); ++v) {
        if (l->voice[v].gate && l->voice[v].note == note) {
            l->voice[v].pressure = pressure > 127 ? 127 : pressure;
        }
    }
}

void live_bend_range_rpn(live* l, uint32_t semitones) {
    l->settings_dirty = 1;
    l->bend_range = (uint8_t)(semitones > LIVE_BEND_RANGE_MAX ? LIVE_BEND_RANGE_MAX : semitones);
    l->bend_range_rpn = 1;
}

void live_reset_controllers(live* l) {
    l->bend = LIVE_BEND_CENTRE;
    l->mod_wheel = 0;
    l->channel_pressure = 0;
    for (int v = 0; v < LIVE_VOICES; ++v) {
        l->voice[v].pressure = 0;
    }
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

/* Replaces the bits of mask in *field by value (already in place); 1 if that changed it. */
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
        set_cutoff(l, (uint16_t)(value > LIVE_CUTOFF_MAX ? LIVE_CUTOFF_MAX : value));
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
    case LIVE_PARAM_RESONANCE: set_resonance(l, (uint8_t)value); break;
    case LIVE_PARAM_PULSE_WIDTH: set_pulse_width(l, (uint16_t)value); break;
    case LIVE_PARAM_VOLUME: live_set_master(l, (uint16_t)value); break;
    case LIVE_PARAM_WAVEFORM:
        if (set_bits(&l->control, CTRL_WAVEFORM, value << 4)) {
            write_controls(l);
        }
        break;
    case LIVE_PARAM_RING:
        if (set_bits(&l->control, CTRL_RING, value ? CTRL_RING : 0u)) {
            write_controls(l);
        }
        break;
    case LIVE_PARAM_SYNC:
        if (set_bits(&l->control, CTRL_SYNC, value ? CTRL_SYNC : 0u)) {
            write_controls(l);
        }
        break;
    case LIVE_PARAM_ATTACK:
        if (set_bits(&l->attack_decay, 0xf0u, value << 4)) {
            write_envelopes(l);
        }
        break;
    case LIVE_PARAM_DECAY:
        if (set_bits(&l->attack_decay, 0x0fu, value)) {
            write_envelopes(l);
        }
        break;
    case LIVE_PARAM_SUSTAIN:
        if (set_bits(&l->sustain_release, 0xf0u, value << 4)) {
            write_envelopes(l);
        }
        break;
    case LIVE_PARAM_RELEASE:
        if (set_bits(&l->sustain_release, 0x0fu, value)) {
            write_envelopes(l);
        }
        break;
    case LIVE_PARAM_FILTER_MODE:
        if (set_bits(&l->filter_mode, 0x70u, value << 4)) {
            write_mode_vol(l);
        }
        break;
    case LIVE_PARAM_FILTER_VOICES:
        if (set_bits(&l->filter_voices, 0x07u, value)) {
            write_res_filt(l);
        }
        break;
    case LIVE_PARAM_MODEL: /* only recorded: live_tick applies it */
        if (l->expert_active) {
            macro_changed(l, 0);
        }
        l->user_applied = 0;
        request_profile(l, model_profile(l, value ? SID_MODEL_8580 : SID_MODEL_6581));
        break;
    case LIVE_PARAM_PROFILE: /* the same; USER 1..8 above the built-in ones */
        if (value < SID_CHIPS) {
            if (l->expert_active) {
                macro_changed(l, 0);
            }
            l->user_applied = 0;
            request_profile(l, value);
        } else {
            apply_user(l, value - SID_CHIPS);
        }
        break;
    case LIVE_PARAM_AGE:
    case LIVE_PARAM_DAC:
    case LIVE_PARAM_THUMP:
    case LIVE_PARAM_BLEED: /* recorded: live_tick sends them */
        if (l->expert_active && l->character[param - LIVE_PARAM_AGE] != value) {
            macro_changed(l, param - LIVE_PARAM_AGE + 1u);
        }
        l->character[param - LIVE_PARAM_AGE] = (uint8_t)value;
        break;
    /* Expression: recorded; live_tick writes what they change, except the velocity target,
     * which rewrites the envelopes at once (each voice keeps its velocity). */
    case LIVE_PARAM_BEND_RANGE: /* a preset edit: RPN 0's no longer holds */
        l->bend_range = (uint8_t)value;
        l->bend_range_rpn = 0;
        break;
    case LIVE_PARAM_VELOCITY_TARGET:
        if (l->velocity_target != value) {
            l->velocity_target = (uint8_t)value;
            write_envelopes(l);
        }
        break;
    case LIVE_PARAM_MOD_TARGET:
        if (l->mod_target != value) {
            l->mod_target = (uint8_t)value;
            l->mod_wheel = 0; /* a new target starts from 0 */
        }
        break;
    case LIVE_PARAM_PRESSURE_TARGET: l->pressure_target = (uint8_t)value; break;
    case LIVE_PARAM_VIBRATO_RATE: l->vibrato_rate = (uint8_t)value; break;
    case LIVE_PARAM_VIBRATO_DEPTH: l->vibrato_depth = (uint8_t)value; break;
    case LIVE_PARAM_VIBRATO_DELAY: l->vibrato_delay = (uint8_t)value; break;
    case LIVE_PARAM_ENVELOPE_MODE: /* for the gate edges from now on: a held note's release too */
        if (l->envelope_mode != value) {
            l->envelope_mode = (uint8_t)value;
            write_gate_reset(l);
        }
        break;
    /* The sound's own: what they move is written at once, as a tick would write it. */
    case LIVE_PARAM_PWM_DEPTH:
        l->pwm_depth = (uint16_t)value;
        update_pws(l);
        break;
    case LIVE_PARAM_PWM_RATE: l->pwm_rate = (uint16_t)live_pwm_step(value); break;
    case LIVE_PARAM_TUNE:
        l->tune = (int8_t)((int32_t)value - LIVE_TUNE_CENTS);
        for (int v = 0; v < LIVE_VOICES; ++v) {
            update_freq(l, v);
        }
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

void live_params_base(const live* l, uint16_t out[LIVE_PARAM_EXPERT]) {
    out[LIVE_PARAM_CUTOFF] = l->cutoff;
    out[LIVE_PARAM_RESONANCE] = l->resonance;
    out[LIVE_PARAM_PULSE_WIDTH] = l->pulse_width;
    out[LIVE_PARAM_VOLUME] = l->master;
    out[LIVE_PARAM_WAVEFORM] = (uint16_t)(l->control >> 4);
    out[LIVE_PARAM_RING] = (l->control & CTRL_RING) ? 1u : 0u;
    out[LIVE_PARAM_SYNC] = (l->control & CTRL_SYNC) ? 1u : 0u;
    out[LIVE_PARAM_ATTACK] = (uint16_t)(l->attack_decay >> 4);
    out[LIVE_PARAM_DECAY] = l->attack_decay & 0x0fu;
    out[LIVE_PARAM_SUSTAIN] = (uint16_t)(l->sustain_release >> 4);
    out[LIVE_PARAM_RELEASE] = l->sustain_release & 0x0fu;
    out[LIVE_PARAM_FILTER_MODE] = (l->filter_mode >> 4) & 7u;
    out[LIVE_PARAM_FILTER_VOICES] = l->filter_voices & 7u;
    out[LIVE_PARAM_MODEL] = l->model_req == SID_MODEL_8580 ? 1u : 0u;
    for (uint32_t s = 0; s < LIVE_SLOTS; ++s) {
        for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; ++f) {
            out[LIVE_PARAM_SLOT + s * LIVE_SLOT_FIELDS + f] = l->slot[s][f];
        }
    }
    out[LIVE_PARAM_BEND_RANGE] = l->bend_range;
    out[LIVE_PARAM_VELOCITY_TARGET] = l->velocity_target;
    out[LIVE_PARAM_MOD_TARGET] = l->mod_target;
    out[LIVE_PARAM_PRESSURE_TARGET] = l->pressure_target;
    out[LIVE_PARAM_VIBRATO_RATE] = l->vibrato_rate;
    out[LIVE_PARAM_VIBRATO_DEPTH] = l->vibrato_depth;
    out[LIVE_PARAM_VIBRATO_DELAY] = l->vibrato_delay;
    out[LIVE_PARAM_ENVELOPE_MODE] = l->envelope_mode;
    out[LIVE_PARAM_PROFILE] = l->profile_req;
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; ++f) {
        out[LIVE_PARAM_AGE + f] = l->character[f];
    }
}

void live_params_tail(const live* l, uint16_t out[LIVE_PARAM_COUNT]) {
    out[LIVE_PARAM_PWM_DEPTH] = l->pwm_depth;
    out[LIVE_PARAM_PWM_RATE] = (uint16_t)live_pwm_rate(l->pwm_rate);
    out[LIVE_PARAM_TUNE] = (uint16_t)(l->tune + LIVE_TUNE_CENTS);
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
    return (rate * 32768u + 500u) / 1000u; /* 65536 x 0.05 / 100 = 32.768 a rate */
}

uint32_t live_pwm_rate(uint32_t step) {
    uint32_t rate = (step * 1000u + 16384u) / 32768u;
    return rate < LIVE_PWM_RATE_MAX ? rate : LIVE_PWM_RATE_MAX;
}

/* Each slot's algorithm count; live_slot_pick's own table, its algorithms are below 8
 * (live_fx_defaults). */
static const uint8_t SLOT_ALGORITHMS[LIVE_SLOTS] = {SID_DIST_COUNT, SID_MOD_COUNT,
                                                    SID_FXSLOT_COUNT};

uint32_t live_slot_algorithms(uint32_t slot) {
    return slot < LIVE_SLOTS ? SLOT_ALGORITHMS[slot] : 0u;
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
        l->settings_dirty = 1;
        put(l, (uint8_t)LIVE_REG_SLOT(slot, field), (uint8_t)value);
    }
}

void live_slot_turn(live* l, uint32_t slot, uint32_t field, int32_t detents) {
    if (slot < LIVE_SLOTS && field < LIVE_SLOT_FIELDS && field != LIVE_SLOT_ALGORITHM) {
        live_slot_set(l, slot, field, l->slot[slot][field] + detents * LIVE_SLOT_DETENT);
    }
}

/* A slot's field to value (already in range), sent when it changed. */
static void slot_store(live* l, uint32_t slot, uint32_t field, uint8_t value) {
    uint8_t* s = l->slot[slot];
    if (s[field] != value) {
        s[field] = value;
        l->settings_dirty = 1;
        put(l, (uint8_t)LIVE_REG_SLOT(slot, field), value);
    }
}

void live_slot_pick(live* l, uint32_t slot, uint32_t algorithm) {
    if (slot >= LIVE_SLOTS) {
        return;
    }
    const uint8_t* s = l->slot[slot];
    uint32_t last = SLOT_ALGORITHMS[slot] - 1u;
    algorithm = algorithm > last ? last : algorithm;
    uint32_t had = s[LIVE_SLOT_ALGORITHM];
    if (had == algorithm) {
        return;
    }
    slot_store(l, slot, LIVE_SLOT_ALGORITHM, (uint8_t)algorithm);
    if (!algorithm) {
        return; /* OFF keeps the controls: the next algorithm starts from them */
    }
    /* silent, or still what the algorithm it had started with */
    const uint8_t* was = live_fx_defaults[slot][had & 7u];
    if ((s[LIVE_SLOT_AMOUNT] | s[LIVE_SLOT_MIX]) &&
        (s[LIVE_SLOT_AMOUNT] != was[0] || s[LIVE_SLOT_A] != was[1] ||
         s[LIVE_SLOT_B] != was[2] || s[LIVE_SLOT_MIX] != was[3])) {
        return; /* edited: the player's values stay */
    }
    const uint8_t* d = live_fx_defaults[slot][algorithm];
    for (uint32_t f = 0; f < 4u; f++) {
        slot_store(l, slot, LIVE_SLOT_AMOUNT + f, d[f]);
    }
}

void live_slot_next_algorithm(live* l, uint32_t slot) {
    if (slot < LIVE_SLOTS) {
        uint32_t next = l->slot[slot][LIVE_SLOT_ALGORITHM] + 1u;
        live_slot_pick(l, slot, next < live_slot_algorithms(slot) ? next : 0u);
    }
}

void live_fx_select_next(live* l) {
    live_fx_select(l, l->fx_select + 1u);
}

void live_fx_select(live* l, uint32_t slot) {
    l->fx_select = (uint8_t)(slot % LIVE_SLOTS);
}

void live_rewrite_voice(live* l, uint32_t v) {
    if (v >= LIVE_VOICES) {
        return;
    }
    int i = (int)v;
    put(l, (uint8_t)(i * 7 + 5), l->attack_decay);
    put(l, (uint8_t)(i * 7 + 6), voice_sr(l, i));
    write_freq(l, i);
    write_pw(l, i);
    write_control(l, i);
    write_res_filt(l);
}

uint8_t live_voice_control(const live* l, uint32_t v) {
    return v < LIVE_VOICES ? voice_control(l, (int)v) : 0u;
}

void live_rewrite(live* l) {
    write_gate_reset(l);
    write_all(l);
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

/* A profile asked for that the chip does not have: with a preparer, a slice of its chip a
 * tick and the reset once that chip is whole; without, the reset at once. A request that went
 * back to the chip's own profile drops the preparation. */
static void apply_profile(live* l) {
    if (l->profile_req == l->profile) {
        if (l->preparing) {
            l->preparing = 0;
            l->prepare(l->prepare_ctx, LIVE_PREPARE_CANCEL);
        }
        return;
    }
    if (l->prepare) {
        l->preparing = 1;
        if (!l->prepare(l->prepare_ctx, l->profile_req)) {
            return;
        }
        l->preparing = 0;
    }
    set_profile(l, l->profile_req);
}

void live_tick(live* l) {
    if (l->expert_dirty) {
        l->expert_active = 0;
        for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
            l->expert_active |= l->expert[f] != SID_EXPERT_DEFAULT;
            if (l->expert[f] != l->expert_sent[f]) {
                put(l, (uint8_t)LIVE_REG_EXPERT(f), l->expert[f]);
                l->expert_sent[f] = l->expert[f];
            }
        }
        l->expert_dirty = 0;
    }
    apply_profile(l);
    update_character(l);
    if (l->pwm_depth) {
        l->lfo_phase = (uint16_t)(l->lfo_phase + l->pwm_rate);
    }
    /* The vibrato: rate / 10 Hz, 65536 a cycle, so 655.36 * rate / 100 per tick. */
    uint32_t step = ((uint32_t)l->vibrato_rate * 65536u + 500u) / 1000u;
    for (int v = 0; v < LIVE_VOICES; ++v) {
        live_voice* x = &l->voice[v];
        if (x->vibrato_wait) {
            x->vibrato_wait--;
        } else {
            x->vibrato_phase = (uint16_t)(x->vibrato_phase + step);
        }
    }
    if (l->arp_voice != LIVE_ARP_NO_VOICE || l->arp_clock_seen) {
        arp_tick(l);
    }
    for (int v = 0; v < LIVE_VOICES; ++v) {
        update_freq(l, v);
    }
    update_pws(l);
    update_cutoff(l);
}
