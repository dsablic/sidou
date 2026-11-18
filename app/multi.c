// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "multi.h"

#include <string.h>

#define REG_FC_LO 0x15u
#define REG_FC_HI 0x16u
#define REG_RES_FILT 0x17u
#define REG_MODE_VOL 0x18u
#define VOICE_REGS 7u
#define CTRL_GATE 0x01u
#define CTRL_NEIGHBOURS 0x06u
#define VOLUME_FULL 0x0fu
#define FILTER_TYPE 0x70u
#define DEFAULT_PROGRAMS {0u, 1u, 3u}

static uint32_t get16(const uint8_t* s, uint32_t at) {
    return (uint32_t)s[at] | (uint32_t)s[at + 1u] << 8;
}

static uint32_t at_most(uint32_t v, uint32_t max) {
    return v < max ? v : max;
}

static int32_t within(uint8_t b, int32_t max) {
    int32_t v = (int8_t)b;
    return v < -max ? -max : v > max ? max : v;
}

static void put(multi* m, uint32_t reg, uint8_t value) {
    if (!m->tune) {
        m->sink(m->sink_ctx, (uint8_t)reg, value);
    }
}

static void put_voice(multi* m, uint32_t v, uint32_t reg, uint8_t value) {
    if (!m->song || m->p[v].gate) {
        put(m, v * VOICE_REGS + reg, value);
    }
}

static int table_on(const multi_part* p) {
    return (p->s.table[0] & LAB_TABLE_ON) && (p->s.table[0] & LAB_TABLE_COUNT_MASK);
}

static uint8_t part_control(const multi_part* p) {
    uint8_t c = table_on(p) ? live_table_step(p->s.table, p->table_pos)[0] : p->s.control;
    return (uint8_t)((c & ~(CTRL_GATE | CTRL_NEIGHBOURS)) | (p->gate ? CTRL_GATE : 0u));
}

static uint32_t vibrato_depth(const multi_part* p) {
    uint32_t d = p->s.vibrato_depth;
    if (p->s.mod_target == LIVE_MOD_VIBRATO) {
        d += p->wheel;
    }
    if (p->s.pressure_target == LIVE_PRESSURE_VIBRATO) {
        d += p->pressure;
    }
    return at_most(d, LIVE_EXPRESSION_MAX);
}

static uint16_t part_freq(const multi_part* p) {
    if (p->note == MULTI_NO_NOTE) {
        return 0;
    }
    uint8_t note = p->note;
    int32_t off = p->bend + p->s.tune;
    if (table_on(p)) {
        off += live_table_pitch(live_table_step(p->s.table, p->table_pos)[1], &note);
    }
    uint32_t depth = vibrato_depth(p);
    if (depth && !p->vibrato_wait) {
        off += live_vibrato_swing(p->vibrato_phase, depth);
    }
    return live_freq_at(note, off);
}

static uint16_t part_pw(const multi_part* p) {
    int32_t pw = live_pwm_swing(p->pulse_width, p->s.pwm_depth, p->lfo_phase);
    if (p->s.pressure_target == LIVE_PRESSURE_PULSE_WIDTH) {
        pw = (int32_t)at_most((uint32_t)pw + p->pressure * (uint32_t)LIVE_PW_PER_VALUE,
                              LIVE_PULSE_WIDTH_MAX);
    }
    if (p->s.velocity_target == LIVE_VELOCITY_PULSE_WIDTH) {
        pw = live_velocity_pulse(pw, p->velocity);
    }
    return (uint16_t)pw;
}

static uint8_t part_sr(const multi_part* p) {
    return p->s.velocity_target == LIVE_VELOCITY_SUSTAIN
               ? live_velocity_sustain(p->s.sustain_release, p->velocity)
               : p->s.sustain_release;
}

static void write_freq(multi* m, uint32_t v) {
    multi_part* p = &m->p[v];
    uint16_t f = part_freq(p);
    p->freq_out = f;
    put_voice(m, v, 0, (uint8_t)f);
    put_voice(m, v, 1, (uint8_t)(f >> 8));
}

static void write_pw(multi* m, uint32_t v) {
    multi_part* p = &m->p[v];
    uint16_t pw = part_pw(p);
    p->pw_out = pw;
    put_voice(m, v, 2, (uint8_t)pw);
    put_voice(m, v, 3, (uint8_t)(pw >> 8));
}

static void write_voice(multi* m, uint32_t v) {
    multi_part* p = &m->p[v];
    put_voice(m, v, 5, p->s.attack_decay);
    put_voice(m, v, 6, part_sr(p));
    write_pw(m, v);
    write_freq(m, v);
    put_voice(m, v, 4, part_control(p));
}

static uint16_t filter_cutoff(const multi* m) {
    const multi_part* p = &m->p[0];
    int32_t c = (m->locks & MULTI_LOCK_CUTOFF) ? m->cutoff_cc : p->s.cutoff;
    if (p->s.mod_target == LIVE_MOD_CUTOFF) {
        c += p->wheel * LIVE_CUTOFF_PER_VALUE;
    }
    if (p->s.pressure_target == LIVE_PRESSURE_CUTOFF) {
        c += p->pressure * LIVE_CUTOFF_PER_VALUE;
    }
    if (p->s.velocity_target == LIVE_VELOCITY_CUTOFF) {
        c -= (int32_t)(LIVE_EXPRESSION_MAX - p->velocity) * LIVE_CUTOFF_PER_VALUE;
    }
    return (uint16_t)(c < 0 ? 0 : at_most((uint32_t)c, LIVE_CUTOFF_MAX));
}

static void write_filter(multi* m) {
    if (m->song || m->tune) {
        return;
    }
    uint16_t c = filter_cutoff(m);
    if (c != m->cutoff_out) {
        m->cutoff_out = c;
        put(m, REG_FC_LO, (uint8_t)(c & 7u));
        put(m, REG_FC_HI, (uint8_t)(c >> 3));
    }
    uint8_t mode = (uint8_t)(m->p[0].s.filter_mode & FILTER_TYPE);
    uint8_t routed = 0;
    for (uint32_t v = 0; mode && v < MULTI_PARTS; v++) {
        routed = (uint8_t)(routed | m->p[v].s.routed << v);
    }
    uint8_t res = (m->locks & MULTI_LOCK_RESONANCE) ? m->resonance_cc : m->p[0].s.resonance;
    if (res != m->resonance_out || routed != m->routed_out) {
        m->resonance_out = res;
        m->routed_out = routed;
        put(m, REG_RES_FILT, (uint8_t)(res << 4 | routed));
    }
    if (mode != m->mode_out) {
        m->mode_out = mode;
        put(m, REG_MODE_VOL, (uint8_t)(mode | VOLUME_FULL));
    }
}

static void unsent(multi* m) {
    m->cutoff_out = MULTI_FILTER_UNSENT;
    m->resonance_out = 0xffu;
    m->routed_out = 0xffu;
    m->mode_out = 0xffu;
}

void multi_fresh(multi* m) {
    unsent(m);
    for (uint32_t v = 0; v < MULTI_PARTS; v++) {
        write_voice(m, v);
    }
    write_filter(m);
}

static void sound_from(multi_sound* s, const uint8_t in[LAB_SOUND_BYTES]) {
    s->control = in[LAB_SOUND_CONTROL];
    s->attack_decay = in[LAB_SOUND_AD];
    s->sustain_release = in[LAB_SOUND_SR];
    s->pulse_width = (uint16_t)at_most(get16(in, LAB_SOUND_PULSE), LIVE_PULSE_WIDTH_MAX);
    s->cutoff = (uint16_t)at_most(get16(in, LAB_SOUND_CUTOFF), LIVE_CUTOFF_MAX);
    s->resonance = (uint8_t)at_most(in[LAB_SOUND_RESONANCE], LIVE_RESONANCE_MAX);
    s->routed = in[LAB_SOUND_FILTER_VOICES] & 1u;
    s->filter_mode = in[LAB_SOUND_FILTER_MODE] & FILTER_TYPE;
    s->bend_range = (uint8_t)at_most(in[LAB_SOUND_BEND_RANGE], LIVE_BEND_RANGE_MAX);
    s->velocity_target = (uint8_t)at_most(in[LAB_SOUND_VELOCITY], LIVE_VELOCITY_PULSE_WIDTH);
    s->mod_target = (uint8_t)at_most(in[LAB_SOUND_MOD], LIVE_MOD_CUTOFF);
    s->pressure_target = (uint8_t)at_most(in[LAB_SOUND_PRESSURE], LIVE_PRESSURE_CUTOFF);
    s->vibrato_rate = (uint8_t)at_most(in[LAB_SOUND_VIBRATO_RATE], LIVE_EXPRESSION_MAX);
    s->vibrato_depth = (uint8_t)at_most(in[LAB_SOUND_VIBRATO_DEPTH], LIVE_EXPRESSION_MAX);
    s->vibrato_delay = (uint8_t)at_most(in[LAB_SOUND_VIBRATO_DELAY], LIVE_EXPRESSION_MAX);
    s->pwm_depth = (uint16_t)at_most(get16(in, LAB_SOUND_PWM_DEPTH), LIVE_PWM_DEPTH_MAX);
    s->pwm_rate = (uint16_t)at_most(get16(in, LAB_SOUND_PWM_RATE), LIVE_PWM_STEP_MAX);
    s->tune = within(in[LAB_SOUND_TUNE], LIVE_TUNE_CENTS) * LIVE_PITCH_SEMITONE / 100 +
              within(in[LAB_SOUND_LAYER_TUNING], LIVE_TRANSPOSE_MAX) * LIVE_PITCH_SEMITONE +
              within(in[LAB_SOUND_LAYER_TUNING + 1u], LIVE_DETUNE_CENTS) * LIVE_PITCH_SEMITONE / 100;
    uint32_t layer = (in[LAB_SOUND_TABLE] & LAB_TABLE_LAYER_MASK) >> LAB_TABLE_LAYER_SHIFT;
    if (layer == 0u || in[LAB_SOUND_LAYERS] == 0u) {
        memcpy(s->table, &in[LAB_SOUND_TABLE], LAB_SOUND_TABLE_BYTES);
    } else {
        memset(s->table, 0, LAB_SOUND_TABLE_BYTES);
    }
}

static void copy_name(char* to, const char* from) {
    uint32_t n = 0;
    for (; from[n] && n < LAB_SOUND_NAME; n++) {
        to[n] = from[n];
    }
    to[n] = 0;
}

static int read_sound(const multi* m, uint32_t program, char name[LAB_SOUND_NAME + 1u],
                      uint8_t sound[LAB_SOUND_BYTES]) {
    if (program < LIVE_PRESET_COUNT) {
        live_preset_sound(program, sound);
        copy_name(name, live_preset_get((uint8_t)program)->name);
        return 1;
    }
    if (program == MULTI_PROGRAM_KEYS && m->l) {
        live_sound_get(m->l, sound);
        live_preset_name(m->l, name);
        return 1;
    }
    if (program >= MULTI_PROGRAM_USER && program < MULTI_PROGRAMS && m->user) {
        return m->user(m->user_ctx, program - MULTI_PROGRAM_USER, name, sound);
    }
    return 0;
}

LIVE_COLD static int load(multi* m, uint32_t part, uint32_t program) {
    char name[LAB_SOUND_NAME + 1u];
    uint8_t sound[LAB_SOUND_BYTES];
    if (part >= MULTI_PARTS || !read_sound(m, program, name, sound)) {
        return 0;
    }
    multi_part* p = &m->p[part];
    sound_from(&p->s, sound);
    copy_name(p->s.name, name);
    p->s.program = (uint8_t)program;
    p->pulse_width = p->s.pulse_width;
    p->table_pos = 0;
    p->table_wait = 0;
    return 1;
}

int multi_program(multi* m, uint32_t part, uint32_t program) {
    if (!load(m, part, program)) {
        return 0;
    }
    write_voice(m, part);
    write_filter(m);
    return 1;
}

static int program_exists(const multi* m, uint32_t program) {
    if (program <= MULTI_PROGRAM_KEYS) {
        return program < LIVE_PRESET_COUNT || m->l != 0;
    }
    return program >= MULTI_PROGRAM_USER && program < MULTI_PROGRAMS && m->l &&
           (m->l->sounds_valid >> (program - MULTI_PROGRAM_USER) & 1u);
}

uint32_t multi_program_step(const multi* m, uint32_t program, int32_t steps) {
    int32_t dir = steps < 0 ? -1 : 1;
    uint32_t at = program;
    for (int32_t n = steps < 0 ? -steps : steps; n > 0; n--) {
        uint32_t k = at;
        do {
            k = (uint32_t)((int32_t)k + dir);
        } while (k < MULTI_PROGRAMS && !program_exists(m, k));
        if (k >= MULTI_PROGRAMS) {
            break;
        }
        at = k;
    }
    return at;
}

LIVE_COLD void multi_init(multi* m, live_sink_fn sink, void* sink_ctx, const live* l,
                          const keys_route* kr) {
    memset(m, 0, sizeof *m);
    m->sink = sink;
    m->sink_ctx = sink_ctx;
    m->l = l;
    m->kr = kr;
    m->profile = kr ? kr->ctl[0].has.profile : 0u;
    unsent(m);
    static const uint8_t programs[MULTI_PARTS] = DEFAULT_PROGRAMS;
    for (uint32_t v = 0; v < MULTI_PARTS; v++) {
        multi_part* p = &m->p[v];
        p->note = MULTI_NO_NOTE;
        p->velocity = LIVE_EXPRESSION_MAX;
        (void)load(m, v, programs[v]);
    }
}

void multi_note_on(multi* m, uint32_t part, uint8_t note, uint8_t velocity, uint32_t legato) {
    if (part >= MULTI_PARTS || note > 127u) {
        return;
    }
    multi_part* p = &m->p[part];
    p->note = note;
    if (legato && p->gate) {
        write_freq(m, part);
        return;
    }
    if (m->song && m->taking) {
        m->taking(m->taking_ctx, 1u << part);
    }
    if (p->gate) {
        p->gate = 0;
        put_voice(m, part, 4, part_control(p));
    }
    p->gate = 1;
    p->velocity = velocity > 127u ? 127u : velocity;
    p->pressure = 0;
    p->vibrato_phase = 0;
    p->vibrato_wait = p->s.vibrato_delay;
    p->table_pos = 0;
    p->table_wait = 0;
    write_voice(m, part);
    write_filter(m);
}

void multi_note_off(multi* m, uint32_t part) {
    if (part >= MULTI_PARTS || !m->p[part].gate) {
        return;
    }
    multi_part* p = &m->p[part];
    p->gate = 0;
    put(m, part * VOICE_REGS + 4u, part_control(p));
}

void multi_bend(multi* m, uint32_t part, int32_t offset) {
    if (part < MULTI_PARTS) {
        m->p[part].bend = offset;
    }
}

void multi_wheel(multi* m, uint32_t part, uint8_t value) {
    if (part >= MULTI_PARTS) {
        return;
    }
    multi_part* p = &m->p[part];
    if (p->s.mod_target == LIVE_MOD_PULSE_WIDTH) {
        p->pulse_width = (uint16_t)live_wheel_pulse_width(value);
    } else {
        p->wheel = value;
    }
}

void multi_pressure(multi* m, uint32_t part, uint8_t value) {
    if (part < MULTI_PARTS) {
        m->p[part].pressure = value > 127u ? 127u : value;
    }
}

void multi_reset_controllers(multi* m, uint32_t part) {
    if (part < MULTI_PARTS) {
        m->p[part].wheel = 0;
        m->p[part].pressure = 0;
    }
}

void multi_reset(multi* m) {
    for (uint32_t v = 0; v < MULTI_PARTS; v++) {
        multi_note_off(m, v);
        multi_reset_controllers(m, v);
        m->p[v].bend = 0;
    }
    m->locks = 0;
    write_filter(m);
}

void multi_filter(multi* m, uint32_t locks, uint32_t cutoff, uint32_t resonance) {
    m->locks = (uint8_t)locks;
    m->cutoff_cc = (uint16_t)at_most(cutoff, LIVE_CUTOFF_MAX);
    m->resonance_cc = (uint8_t)at_most(resonance, LIVE_RESONANCE_MAX);
    write_filter(m);
}

static void follow_chip(multi* m) {
    const keys_route* kr = m->kr;
    if (!kr) {
        return;
    }
    uint8_t song = kr->song_on;
    uint8_t tune = kr->tune;
    uint8_t profile = kr->ctl[0].has.profile;
    int back = (m->song && !song) || (m->tune && !tune) || profile != m->profile;
    m->song = song;
    m->tune = tune;
    m->profile = profile;
    if (back) {
        multi_fresh(m);
    }
}

static void table_tick(multi* m, uint32_t v) {
    multi_part* p = &m->p[v];
    if (++p->table_wait <= p->s.table[LAB_SOUND_TABLE_RATE - LAB_SOUND_TABLE]) {
        return;
    }
    p->table_wait = 0;
    uint32_t next = live_table_next(p->s.table, p->table_pos);
    if (next != p->table_pos) {
        p->table_pos = (uint8_t)next;
        put_voice(m, v, 4, part_control(p));
    }
}

void multi_tick(multi* m) {
    follow_chip(m);
    m->table_frame ^= 1u;
    for (uint32_t v = 0; v < MULTI_PARTS; v++) {
        multi_part* p = &m->p[v];
        if (p->vibrato_wait) {
            p->vibrato_wait--;
        } else {
            p->vibrato_phase = (uint16_t)(p->vibrato_phase + live_vibrato_step(p->s.vibrato_rate));
        }
        if (p->s.pwm_depth) {
            p->lfo_phase = (uint16_t)(p->lfo_phase + p->s.pwm_rate);
        }
        if (!m->table_frame && p->note != MULTI_NO_NOTE && table_on(p)) {
            table_tick(m, v);
        }
        if (part_freq(p) != p->freq_out) {
            write_freq(m, v);
        }
        if (part_pw(p) != p->pw_out) {
            write_pw(m, v);
        }
    }
    write_filter(m);
}

uint8_t multi_taken(const multi* m) {
    if (!m->song) {
        return 0;
    }
    uint8_t taken = 0;
    for (uint32_t v = 0; v < MULTI_PARTS; v++) {
        taken = (uint8_t)(taken | m->p[v].gate << v);
    }
    return taken;
}
