// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_table.h"

#include <string.h>

#include "screen.h"

#define HEAD 0u
#define LOOP (LAB_SOUND_TABLE_LOOP - LAB_SOUND_TABLE)
#define RATE (LAB_SOUND_TABLE_RATE - LAB_SOUND_TABLE)
#define STEP(n) (LAB_SOUND_TABLE_STEPS - LAB_SOUND_TABLE + 2u * (n))
#define CODE_END 0u
#define CODE_OFF 1u
#define CODE_FIRST_WAVE 2u
#define WAVE_BITS 0xF6u

static const uint8_t CODE_CONTROL[LAB_TABLE_WAVE_CODES] = {0x00, 0x00, 0x10, 0x20, 0x40, 0x80, 0x30,
                                                           0x50, 0x60, 0x70, 0x14, 0x22, 0x42};
static const char CODES[LAB_TABLE_WAVE_CODES][4] = {"END", "OFF", "TRI", "SAW", "PUL", "NOI", "T+S",
                                                    "T+P", "S+P", "TSP", "TRR", "SAS", "PUS"};

uint32_t lab_table_steps(const live* l) {
    return l->table[HEAD] & LAB_TABLE_COUNT_MASK;
}

uint32_t lab_table_on(const live* l) {
    return (l->table[HEAD] & LAB_TABLE_ON) != 0u;
}

uint32_t lab_table_layer(const live* l) {
    return (l->table[HEAD] & LAB_TABLE_LAYER_MASK) >> LAB_TABLE_LAYER_SHIFT;
}

static uint32_t code_of(uint8_t control) {
    for (uint32_t i = CODE_OFF; i < LAB_TABLE_WAVE_CODES; i++) {
        if (CODE_CONTROL[i] == control) {
            return i;
        }
    }
    return LAB_TABLE_WAVE_CODES;
}

static uint32_t nearest_code(uint8_t control) {
    uint32_t c = code_of(control & WAVE_BITS);
    if (c == LAB_TABLE_WAVE_CODES) {
        c = code_of(control & 0xF0u);
    }
    return c == LAB_TABLE_WAVE_CODES ? CODE_FIRST_WAVE : c;
}

const char* lab_table_wave_code(uint8_t control, char out[4]) {
    static const char HEX[] = "0123456789ABCDEF";
    uint32_t c = code_of(control);
    if (c < LAB_TABLE_WAVE_CODES) {
        return CODES[c];
    }
    out[0] = HEX[control >> 4];
    out[1] = HEX[control & 15u];
    out[2] = 0;
    return out;
}

void lab_table_note_text(uint8_t note, char out[5]) {
    if (lab_table_absolute(note)) {
        screen_note_text(note - LAB_TABLE_NOTE_NONE, out);
        return;
    }
    int32_t n = lab_table_offset(note);
    uint32_t i = 0;
    if (n) {
        out[i++] = n < 0 ? '-' : '+';
    }
    n = n < 0 ? -n : n;
    i += screen_decimal(out + i, (uint32_t)n);
    out[i] = 0;
}

static void copy(char* out, const char* s) {
    while ((*out++ = *s++)) {
    }
}

int lab_table_card(const live* l, uint32_t step, uint32_t k, char* name, char* value,
                   uint32_t* fill, uint32_t* max) {
    static const char* const NAMES[4] = {"STEP", "WAVE", "NOTE", "FRAMES"};
    uint32_t steps = lab_table_steps(l);
    const uint8_t* st = &l->table[STEP(step < steps ? step : 0u)];
    int fresh = step >= steps;
    copy(name, NAMES[k & 3u]);
    *fill = 0;
    *max = 1;
    switch (k) {
    case 0u:
        if (fresh) {
            copy(value, "NEW");
        } else {
            value[screen_decimal(value, step + 1u)] = 0;
        }
        *fill = step;
        *max = LAB_TABLE_STEPS_MAX - 1u;
        break;
    case 1u: {
        char hex[4];
        copy(value, fresh ? "-" : lab_table_wave_code(st[0], hex));
        *fill = fresh ? 0u : nearest_code(st[0]);
        *max = LAB_TABLE_WAVE_CODES - 1u;
        break;
    }
    case 2u:
        if (fresh) {
            copy(value, "-");
        } else {
            lab_table_note_text(st[1], value);
        }
        *fill = fresh ? 0u : (uint32_t)(lab_table_offset(st[1]) + LAB_TABLE_NOTE_SPAN);
        *max = 2u * LAB_TABLE_NOTE_SPAN;
        break;
    default: {
        uint32_t frames = l->table[RATE] + 1u;
        value[screen_decimal(value, frames)] = 0;
        *fill = frames - 1u;
        *max = LAB_TABLE_RATE_MAX - 1u;
        break;
    }
    }
    return lab_table_on(l) != 0u;
}

void lab_table_choice(const live* l, char* out) {
    if (!lab_table_on(l)) {
        copy(out, "TABLE OFF");
    } else if (!l->table[LOOP]) {
        copy(out, "PLAY ONCE");
    } else {
        copy(out, "LOOP FROM ");
        out[10 + screen_decimal(out + 10, l->table[LOOP])] = 0;
    }
}

static int32_t clamp(int32_t v, int32_t lo, int32_t hi) {
    return v < lo ? lo : v > hi ? hi : v;
}

static void set_steps(uint8_t* t, uint32_t steps) {
    t[HEAD] = (uint8_t)((t[HEAD] & ~LAB_TABLE_COUNT_MASK) | steps);
    if (t[LOOP] > steps) {
        t[LOOP] = (uint8_t)steps;
    }
    if (!steps) {
        t[HEAD] &= (uint8_t)~LAB_TABLE_ON;
    }
}

static uint8_t turned_note(uint8_t note, int32_t d) {
    if (lab_table_absolute(note)) {
        return (uint8_t)clamp((int32_t)note + d, (int32_t)LAB_TABLE_NOTE_NONE + 1,
                              (int32_t)LAB_TABLE_NOTE_LAST);
    }
    int32_t n = clamp(lab_table_offset(note) + d, -LAB_TABLE_NOTE_SPAN, LAB_TABLE_NOTE_SPAN);
    return (uint8_t)(n < 0 ? (int32_t)LAB_TABLE_NOTE_NONE + n : n);
}

uint32_t lab_table_turn(live* l, uint32_t step, uint32_t k, int32_t d) {
    uint32_t steps = lab_table_steps(l);
    if (!lab_table_on(l)) {
        return step;
    }
    if (k == 0u) {
        uint32_t last = steps < LAB_TABLE_STEPS_MAX ? steps : LAB_TABLE_STEPS_MAX - 1u;
        return (uint32_t)clamp((int32_t)step + d, 0, (int32_t)last);
    }
    uint8_t t[LAB_SOUND_TABLE_BYTES];
    memcpy(t, l->table, sizeof t);
    if (k == 3u) {
        t[RATE] = (uint8_t)clamp((int32_t)t[RATE] + d, 0, LAB_TABLE_RATE_MAX - 1);
    } else {
        if (step >= steps) {
            memcpy(&t[STEP(step)], &t[STEP(step - 1u)], 2u);
            set_steps(t, ++steps);
        }
        uint8_t* st = &t[STEP(step)];
        if (k == 2u) {
            st[1] = turned_note(st[1], d);
        } else {
            int32_t c = (int32_t)nearest_code(st[0]);
            int32_t to = clamp(c + d, c == CODE_OFF ? CODE_END : CODE_OFF,
                               (int32_t)LAB_TABLE_WAVE_CODES - 1);
            if (to == CODE_END) {
                set_steps(t, step);
            } else {
                st[0] = CODE_CONTROL[to];
            }
        }
    }
    live_table_set(l, t);
    uint32_t now = lab_table_steps(l);
    return step <= now ? step : now;
}

void lab_table_choice_turn(live* l, int32_t d) {
    uint8_t t[LAB_SOUND_TABLE_BYTES];
    memcpy(t, l->table, sizeof t);
    uint32_t steps = lab_table_steps(l);
    int32_t at = !lab_table_on(l) ? 0 : 1 + t[LOOP];
    int32_t to = clamp(at + d, 0, 1 + (int32_t)(steps ? steps : 1u));
    if (to == at) {
        return;
    }
    if (!to) {
        t[HEAD] &= (uint8_t)~LAB_TABLE_ON;
    } else {
        if (!steps) {
            t[STEP(0)] = live_layer_control_of(l, l->layer_edit);
            t[STEP(0) + 1u] = 0;
            steps = 1u;
        }
        uint32_t layer = l->layers ? l->layer_edit : 0u;
        if (!at) {
            t[HEAD] = (uint8_t)(LAB_TABLE_ON | layer << LAB_TABLE_LAYER_SHIFT | steps);
        }
        t[LOOP] = (uint8_t)(to - 1);
    }
    live_table_set(l, t);
}
