// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "live.h"
#include "live_int.h"

#include "fx_live.h"
#include "midi_map.h"

#include <stddef.h>
#include <string.h>

#define REG_FC_LO 0x15u
#define REG_FC_HI 0x16u
#define REG_RES_FILT 0x17u
#define REG_MODE_VOL 0x18u
#define NOTE_TOP 106
#define CUTOFF_PER_VALUE 8
#define PW_PER_VALUE 16

_Static_assert(LIVE_PITCH_SEMITONE == 8192 && LIVE_BEND_CENTRE == 8192u,
               "a bend times the range is the offset in 1/8192 semitone");

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

static int take_prepared(live_sid_target* t, sid_model model) {
    if (!t->prep_on || !sid_character_prep_done(&t->prep) || t->prep.model != (uint8_t)model ||
        t->prep.c.profile != t->ramp.target.profile) {
        return 0;
    }
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

uint16_t live_freq_at(uint8_t note, int32_t offset) {
    int32_t n = note & 0x7f;
    while (n > NOTE_TOP) {
        n -= 12;
    }
    if (offset == 0) {
        return live_freq_table[n];
    }
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
    uint32_t j = frac >> 7, r = frac & 127u;
    uint32_t d = live_freq_fine_q20[j] +
                 (((live_freq_fine_q20[j + 1] - live_freq_fine_q20[j]) * r + 64u) >> 7);
    uint32_t below = (uint32_t)NOTE_TOP - whole;
    uint32_t octaves = below / 12u;
    uint32_t b = live_freq_top_q4[11u - below % 12u];
    uint32_t prod = (b >> 10) * d + (((b & 1023u) * d + 512u) >> 10);
    uint32_t q4 = b + ((prod + 512u) >> 10);
    uint32_t shift = 4u + octaves;
    uint32_t f = (q4 + (1u << (shift - 1u))) >> shift;
    return f > 0xffffu ? 0xffffu : (uint16_t)f;
}

static uint8_t voice_pressure(const live* l, int v) {
    uint8_t p = l->voice[v].pressure;
    return l->channel_pressure > p ? l->channel_pressure : p;
}

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

static int32_t pitch_offset(const live* l, int v) {
    int32_t off = ((int32_t)l->bend - (int32_t)LIVE_BEND_CENTRE) * (int32_t)l->bend_range;
    if (l->tune) {
        off += l->tune * LIVE_PITCH_SEMITONE / 100;
    }
    const live_voice* x = &l->voice[v];
    uint32_t depth = vibrato_depth(l, v);
    if (depth && !x->vibrato_wait) {
        int32_t p = x->vibrato_phase;
        int32_t t = p < 16384 ? p : p < 49152 ? 32768 - p : p - 65536;
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
    uint32_t f = live_freq_at(note, pitch_offset(l, src));
    if (l->mono_sync && v == 0) {
        f = (f * l->sync_q8) >> 8;
        if (f > 0xffffu) {
            f = 0xffffu;
        }
    }
    return (uint16_t)f;
}

static uint8_t voice_control(const live* l, int v) {
    if (l->mono_sync && v != 0) {
        return 0;
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

static uint8_t voice_sr(const live* l, int v) {
    if (l->velocity_target != LIVE_VELOCITY_SUSTAIN) {
        return l->sustain_release;
    }
    uint32_t s = ((uint32_t)(l->sustain_release >> 4) * l->voice[v].velocity + 63u) / 127u;
    return (uint8_t)(s << 4 | (l->sustain_release & 0x0fu));
}

uint32_t live_resonance_effective(const live* l) {
    int32_t r = l->resonance;
    if (l->reso_link) {
        r += l->reso_link * (32 - (int32_t)live_cutoff_place(l, l->cutoff_out)) / 8;
        r = r < 0 ? 0 : r > (int32_t)LIVE_RESONANCE_MAX ? (int32_t)LIVE_RESONANCE_MAX : r;
    }
    return (uint32_t)r;
}

void li_write_res_filt(live* l) {
    l->res_out = (uint8_t)live_resonance_effective(l);
    put(l, REG_RES_FILT, (uint8_t)(l->res_out << 4 | l->filter_voices));
}

void li_write_mode_vol(live* l) {
    put(l, REG_MODE_VOL, (uint8_t)(l->filter_mode | LIVE_CHIP_VOLUME));
}

void li_write_envelopes(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        put(l, (uint8_t)(v * 7 + 5), l->attack_decay);
        put(l, (uint8_t)(v * 7 + 6), voice_sr(l, v));
    }
}

void li_write_controls(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        write_control(l, v);
    }
}

void li_write_gate_reset(live* l) {
    put(l, LIVE_REG_GATE_RESET, l->envelope_mode == LIVE_ENVELOPE_INSTANT ? 1u : 0u);
}

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

void li_reset_chip(live* l) {
    write_character(l);
    put(l, LIVE_REG_RESET, (uint8_t)l->model);
    li_write_gate_reset(l);
}

void li_write_all(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        put(l, (uint8_t)(v * 7 + 5), l->attack_decay);
        put(l, (uint8_t)(v * 7 + 6), voice_sr(l, v));
        write_freq(l, v);
        write_pw(l, v);
        write_control(l, v);
    }
    write_cutoff(l);
    li_write_res_filt(l);
    li_write_mode_vol(l);
}

void live_links_update(live* l) {
    if (l->reso_link && live_resonance_effective(l) != l->res_out) {
        li_write_res_filt(l);
    }
}

uint32_t live_pwm_depth_effective(const live* l) {
    int32_t d = l->pwm_depth;
    if (l->pwm_link) {
        int32_t away = l->pulse_width > 2048u ? (int32_t)l->pulse_width - 2048
                                                : 2048 - (int32_t)l->pulse_width;
        d += l->pwm_link * away / 8;
        d = d < 0 ? 0 : d > (int32_t)LIVE_PWM_DEPTH_MAX ? (int32_t)LIVE_PWM_DEPTH_MAX : d;
    }
    return (uint32_t)d;
}

static uint16_t pw_effective(const live* l) {
    int32_t pw = l->pulse_width;
    int32_t depth = (int32_t)live_pwm_depth_effective(l);
    int32_t room = pw - LIVE_PULSE_FIRST;
    int32_t top = (int32_t)LIVE_PULSE_WIDTH_MAX + 1 - LIVE_PULSE_FIRST - pw;
    room = room < top ? room : top;
    depth = depth < room ? depth : room;
    if (depth > 0) {
        int32_t phase = l->lfo_phase;
        int32_t tri = phase < 32768 ? phase : 65536 - phase;
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

static uint8_t most_pressure(const live* l) {
    uint8_t p = l->channel_pressure;
    for (int v = 0; v < LIVE_VOICES; ++v) {
        if (l->voice[v].gate && l->voice[v].pressure > p) {
            p = l->voice[v].pressure;
        }
    }
    return p;
}

uint32_t live_cutoff_place(const live* l, uint32_t c) {
    const uint16_t* t = live_cutoff_steps[l->model];
    if (t[l->cutoff_pos] == c) {
        return l->cutoff_pos;
    }
    int32_t p = live_cutoff_down[l->model][(c >> 5) & 63u];
    while (p < 64 && t[p + 1] <= c) {
        p++;
    }
    return p < 0 ? 0u : (uint32_t)p;
}

static int32_t lfo_swing(const live* l) {
    const uint16_t* t = live_cutoff_steps[l->model];
    int32_t pos = (int32_t)live_cutoff_place(l, l->cutoff);
    int32_t phase = l->lfo_phase;
    int32_t tri = (phase < 32768 ? phase : 65536 - phase) - (int32_t)LFO_CENTRE;
    int32_t q = pos * 256 + tri * l->filter_lfo / 64;
    q = q < 0 ? 0 : q > 64 * 256 ? 64 * 256 : q;
    int32_t i = q >> 8;
    int32_t at = t[i] + (i < 64 ? ((t[i + 1] - t[i]) * (q & 255)) >> 8 : 0);
    return at - t[pos];
}

static uint16_t cutoff_effective(const live* l) {
    int32_t c = l->cutoff;
    if (l->filter_lfo) {
        c += lfo_swing(l);
    }
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

void li_update_freq(live* l, int v) {
    if (voice_freq(l, v) != l->voice[v].freq_out) {
        write_freq(l, v);
    }
}

static void update_pw(live* l, int v) {
    if (voice_pw(l, v) != l->voice[v].pw_out) {
        write_pw(l, v);
    }
}

void li_update_pws(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        update_pw(l, v);
    }
}

void li_update_cutoff(live* l) {
    uint16_t c = cutoff_effective(l);
    if (c != l->cutoff_out) {
        write_cutoff_as(l, c);
    }
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
        x->gate = 0;
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
    if (l->velocity_target == LIVE_VELOCITY_SUSTAIN) {
        put(l, (uint8_t)(v * 7 + 6), voice_sr(l, v));
    }
    update_pw(l, v);
    li_update_cutoff(l);
    write_freq(l, v);
    if (l->mono_sync) {
        write_freq(l, 2);
    }
    write_control(l, v);
}

static int free_voice(live* l) {
    int v = pick_voice(l, 0);
    if (v < 0 && l->ui_voice) {
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

void li_release(live* l, int v) {
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
            li_release(l, v);
        }
    }
}

void live_all_notes_off(live* l) {
    for (int v = 0; v < LIVE_VOICES; ++v) {
        if (l->voice[v].gate) {
            li_release(l, v);
        }
    }
    l->arp_n = 0;
    l->arp_voice = LIVE_ARP_NO_VOICE;
}

const uint8_t live_arp_steps[LIVE_ARP_RATES] = {1, 2, 3, 4, 6, 8, 3, 6, 12};

static uint32_t arp_period(const live* l) {
    uint32_t n = (uint32_t)l->arp_n * l->arp_span;
    return l->arp_order == LIVE_ARP_UP_DOWN && n > 1u ? 2u * n - 2u : n;
}

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

void li_arp_rewind(live* l) {
    l->arp_wait = (uint8_t)(live_arp_steps[l->arp_rate] * LIVE_ARP_TICKS_PER_FRAME);
    l->arp_clocks = live_arp_steps[l->arp_rate];
}

void li_arp_sound(live* l) {
    int v = l->arp_voice;
    l->voice[v].note = arp_note_at(l, l->arp_pos);
    li_update_freq(l, v);
    if (l->mono_sync) {
        li_update_freq(l, 2);
    }
}

static void arp_step(live* l) {
    l->arp_pos = (uint8_t)((l->arp_pos + 1u) % arp_period(l));
    li_arp_sound(l);
}

LIVE_COLD void li_arp_resync(live* l) {
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

void li_arp_hold(live* l, uint8_t note) {
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

static void arp_note_on(live* l, uint8_t note, uint8_t velocity) {
    for (uint32_t i = 0; i < l->arp_n; i++) {
        if (l->arp_held[i] == note) {
            return;
        }
    }
    if (l->arp_n == LIVE_ARP_HELD) {
        (void)arp_drop(l, l->arp_held[0]);
    }
    li_arp_hold(l, note);
    if (l->arp_voice != LIVE_ARP_NO_VOICE) {
        li_arp_resync(l);
        return;
    }
    int v = free_voice(l);
    l->arp_voice = (uint8_t)v;
    l->arp_pos = 0;
    li_arp_rewind(l);
    start_voice(l, v, arp_note_at(l, 0), velocity);
    l->note_ons++;
}

static void arp_note_off(live* l, uint8_t note) {
    if (!arp_drop(l, note) || l->arp_voice == LIVE_ARP_NO_VOICE) {
        return;
    }
    if (!l->arp_n) {
        li_release(l, l->arp_voice);
        l->arp_voice = LIVE_ARP_NO_VOICE;
        return;
    }
    li_arp_resync(l);
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

void li_set_cutoff(live* l, uint16_t value) {
    if (value != l->cutoff) {
        l->cutoff = value;
        l->settings_dirty = 1;
        li_update_cutoff(l);
    }
}

void li_set_resonance(live* l, uint8_t value) {
    if (value != l->resonance) {
        l->resonance = value;
        l->settings_dirty = 1;
        li_write_res_filt(l);
    }
}

void li_set_pulse_width(live* l, uint16_t value) {
    l->settings_dirty = 1;
    l->pulse_width = value;
    li_update_pws(l);
}

uint32_t live_tone(const live* l) {
    if (l->mono_sync) {
        return LIVE_TONE_RATIO;
    }
    return l->control & 0x40u ? LIVE_TONE_PULSE : LIVE_TONE_DECAY;
}

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
        l->settings_dirty = 1;
        li_update_freq(l, 0);
        break;
    }
    case LIVE_TONE_PULSE: {
        int32_t pw = l->pulse_width > 2048u ? 4096 - (int32_t)l->pulse_width : (int32_t)l->pulse_width;
        int32_t pos = (pw - LIVE_PULSE_FIRST + LIVE_PULSE_STEP / 2) / LIVE_PULSE_STEP;
        pos = pw < LIVE_PULSE_FIRST ? 0 : pos;
        pos += delta;
        pos = pos < 0 ? 0 : pos > LIVE_PULSE_POSITIONS - 1 ? LIVE_PULSE_POSITIONS - 1 : pos;
        li_set_pulse_width(l, (uint16_t)(LIVE_PULSE_FIRST + LIVE_PULSE_STEP * pos));
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
        const uint16_t* t = live_cutoff_steps[l->model];
        int32_t pos = l->cutoff_pos;
        if (t[pos] != l->cutoff) {
            uint32_t b = l->cutoff >> 5;
            pos = delta > 0 ? live_cutoff_up[l->model][b] - 1 : live_cutoff_down[l->model][b] + 1;
            if (pos == (delta > 0 ? 64 : 0)) {
                break;
            }
        }
        pos += delta > 128 ? 128 : delta < -128 ? -128 : delta;
        pos = pos < 0 ? 0 : pos > 64 ? 64 : pos;
        l->cutoff_pos = (uint8_t)pos;
        li_set_cutoff(l, t[pos]);
        break;
    }
    case LIVE_KNOB_RESONANCE:
        li_set_resonance(l, (uint8_t)step_clamped(l->resonance, delta, 1, LIVE_RESONANCE_MAX));
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

static sid_model profile_model(uint32_t profile) {
    return (sid_model)sid_chip_profile_get(profile)->model;
}

void li_request_profile(live* l, uint32_t profile) {
    l->settings_dirty = 1;
    l->profile_req = (uint8_t)profile;
    l->model_req = profile_model(profile);
    if (l->model_req == SID_MODEL_6581) {
        l->profile_6581 = (uint8_t)profile;
    }
}

uint32_t li_model_profile(const live* l, sid_model model) {
    return model == SID_MODEL_8580 ? sid_chip_default(SID_MODEL_8580) : l->profile_6581;
}

static void set_profile(live* l, uint32_t profile) {
    li_request_profile(l, profile);
    l->profile = (uint8_t)profile;
    l->model = l->model_req;
    li_reset_chip(l);
    li_write_all(l);
}

void li_set_model(live* l, sid_model model) {
    set_profile(l, li_model_profile(l, model));
}

static void update_character(live* l) {
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; ++f) {
        if (l->character[f] != l->character_sent[f]) {
            put(l, (uint8_t)LIVE_REG_CHAR(f + 1u), l->character[f]);
            l->character_sent[f] = l->character[f];
        }
    }
    put(l, LIVE_REG_CHAR_TICK, 0);
}

void live_set_master(live* l, uint16_t value) {
    if (value > LIVE_MASTER_MAX) {
        value = LIVE_MASTER_MAX;
    }
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
    if ((uint32_t)cc - LIVE_CC_ARP <= LIVE_CC_ARP_SPAN - LIVE_CC_ARP && cc != LIVE_CC_ARP_RATE) {
        static const uint8_t VALUES[4] = {2, LIVE_ARP_RATES, 4, LIVE_ARP_SPAN_MAX};
        uint32_t f = (uint32_t)cc - LIVE_CC_ARP;
        live_arp_set(l, f, (uint32_t)value * VALUES[f] / 128u + (f == LIVE_ARP_SPAN));
        return;
    }
    if (cc == LIVE_CC_MOD_WHEEL && l->mod_target != LIVE_MOD_PULSE_WIDTH) {
        l->mod_wheel = value;
        return;
    }
    const midi_map_entry* e = midi_map_for_cc(cc);
    if (e && e->param != MIDI_MAP_RESERVED) {
        midi_map_cc_set(l, e, value);
    }
}

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

void live_reset_controllers(live* l) {
    l->bend = LIVE_BEND_CENTRE;
    l->mod_wheel = 0;
    l->channel_pressure = 0;
    for (int v = 0; v < LIVE_VOICES; ++v) {
        l->voice[v].pressure = 0;
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
    out[LIVE_PARAM_FILTER_LFO] = l->filter_lfo;
    out[LIVE_PARAM_ARP_RATE] = l->arp_rate;
    out[LIVE_PARAM_RESO_LINK] = (uint16_t)(l->reso_link + LIVE_LINK_MAX);
    out[LIVE_PARAM_PWM_LINK] = (uint16_t)(l->pwm_link + LIVE_LINK_MAX);
}

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
        uint32_t algorithm = l->slot[slot][LIVE_SLOT_ALGORITHM] & 7u;
        const uint8_t* t =
            live_walk_fx[live_walk_shapes[slot][algorithm] >> 2u * (field - LIVE_SLOT_AMOUNT) & 3u];
        uint32_t v = l->slot[slot][field];
        int32_t pos = 0;
        for (int32_t s = 16; s; s >>= 1) {
            pos += t[pos + s] <= v ? s : 0;
        }
        if (t[pos] != v && detents < 0) {
            pos++;
        }
        pos += detents;
        live_slot_set(l, slot, field, t[pos < 0 ? 0 : pos > 31 ? 31 : pos]);
    }
}

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
        return;
    }
    const uint8_t* was = live_fx_defaults[slot][had & 7u];
    if ((s[LIVE_SLOT_AMOUNT] | s[LIVE_SLOT_MIX]) &&
        (s[LIVE_SLOT_AMOUNT] != was[0] || s[LIVE_SLOT_A] != was[1] ||
         s[LIVE_SLOT_B] != was[2] || s[LIVE_SLOT_MIX] != was[3])) {
        return;
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
    li_write_res_filt(l);
}

uint8_t live_voice_control(const live* l, uint32_t v) {
    return v < LIVE_VOICES ? voice_control(l, (int)v) : 0u;
}

void live_rewrite(live* l) {
    li_write_gate_reset(l);
    li_write_all(l);
}

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
    if (l->filter_lfo || live_pwm_depth_effective(l)) {
        l->lfo_phase = (uint16_t)(l->lfo_phase + l->pwm_rate);
    }
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
        li_update_freq(l, v);
    }
    li_update_pws(l);
    li_update_cutoff(l);
    live_links_update(l);
}
