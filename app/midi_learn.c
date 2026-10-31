// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_learn.h"

#include <stddef.h>

_Static_assert(MIDI_MAP_ROWS < MIDI_LEARN_NONE, "a row index fits cc_row");
_Static_assert(LIVE_PARAM_COUNT < 256u, "count and param are bytes");

void midi_learn_init(midi_learn* ml) {
    ml->settings_wake = NULL;
    midi_learn_clear(ml);
    ml->state = MIDI_LEARN_IDLE;
    ml->param = 0;
    ml->result = MIDI_LEARN_NOTHING;
    ml->result_cc = MIDI_MAP_NO_CC;
    ml->since = 0;
}

int midi_learn_learnable(uint32_t cc) {
    switch (cc) {
    case 0:
    case 32:
    case 6:
    case 38:
    case 96:
    case 97:
    case 98:
    case 99:
    case 100:
    case 101:
        return 0;
    default: return cc < 120u;
    }
}

int midi_learn_param(uint32_t param) {
    return param < LIVE_PARAM_COUNT && midi_map_for_param(param) != NULL;
}

static void end(midi_learn* ml, midi_learn_result r, uint8_t cc) {
    ml->state = MIDI_LEARN_IDLE;
    ml->result = (uint8_t)r;
    ml->result_cc = cc;
}

int midi_learn_open(midi_learn* ml, uint32_t param, uint32_t now) {
    if (!midi_learn_param(param)) {
        return 0;
    }
    if (ml->state == MIDI_LEARN_IDLE) {
        ml->result = MIDI_LEARN_NOTHING;
    }
    ml->state = MIDI_LEARN_CC;
    ml->param = (uint8_t)param;
    ml->since = now;
    return 1;
}

int midi_learn_forget_picked(midi_learn* ml, uint32_t now) {
    if (ml->state != MIDI_LEARN_CC) {
        return 0;
    }
    ml->since = now;
    uint8_t cc = ml->param_cc[ml->param];
    if (cc == MIDI_MAP_NO_CC) {
        return 0;
    }
    midi_learn_forget(ml, ml->param);
    end(ml, MIDI_LEARN_FORGOT, cc);
    return 1;
}

void midi_learn_ask_clear(midi_learn* ml, uint32_t now) {
    if (ml->state != MIDI_LEARN_IDLE) {
        ml->state = MIDI_LEARN_CONFIRM;
        ml->since = now;
    }
}

void midi_learn_confirm(midi_learn* ml) {
    if (ml->state == MIDI_LEARN_CONFIRM) {
        midi_learn_clear(ml);
        end(ml, MIDI_LEARN_CLEARED, MIDI_MAP_NO_CC);
    }
}

void midi_learn_cancel(midi_learn* ml) {
    if (ml->state != MIDI_LEARN_IDLE) {
        end(ml, MIDI_LEARN_CANCELLED, MIDI_MAP_NO_CC);
    }
}

int midi_learn_tick(midi_learn* ml, uint32_t now) {
    if (ml->state == MIDI_LEARN_IDLE || now - ml->since < MIDI_LEARN_TIMEOUT_MS) {
        return 0;
    }
    end(ml, MIDI_LEARN_TIMED_OUT, MIDI_MAP_NO_CC);
    return 1;
}

midi_learn_result midi_learn_take(midi_learn* ml) {
    midi_learn_result r = (midi_learn_result)ml->result;
    ml->result = MIDI_LEARN_NOTHING;
    return r;
}

const midi_map_entry* midi_learn_for_cc(const midi_learn* ml, uint32_t cc) {
    if (cc > 127u || ml->cc_row[cc] == MIDI_LEARN_NONE) {
        return NULL;
    }
    return midi_map_row(ml->cc_row[cc]);
}

int midi_learn_takes(const midi_learn* ml, uint32_t cc) {
    return midi_learn_for_cc(ml, cc) != NULL ||
           (ml->state == MIDI_LEARN_CC && midi_learn_learnable(cc));
}

const midi_map_entry* midi_learn_route(midi_learn* ml, uint8_t cc) {
    if (ml->state == MIDI_LEARN_CC && midi_learn_bind(ml, cc, ml->param)) {
        end(ml, MIDI_LEARN_LEARNED, cc);
    }
    return midi_learn_for_cc(ml, cc);
}

uint8_t midi_learn_cc_of(const midi_learn* ml, uint32_t param) {
    return param < LIVE_PARAM_COUNT ? ml->param_cc[param] : (uint8_t)MIDI_MAP_NO_CC;
}

static void settings_changed(midi_learn* ml) {
    ml->settings_dirty = 1;
    if (ml->settings_wake) {
        *ml->settings_wake = 1;
    }
}

static void unbind(midi_learn* ml, uint32_t cc) {
    const midi_map_entry* e = midi_learn_for_cc(ml, cc);
    if (e) {
        ml->param_cc[e->param] = MIDI_MAP_NO_CC;
        ml->cc_row[cc] = MIDI_LEARN_NONE;
        ml->count--;
        settings_changed(ml);
    }
}

int midi_learn_bind(midi_learn* ml, uint32_t cc, uint32_t param) {
    if (!midi_learn_learnable(cc) || !midi_learn_param(param)) {
        return 0;
    }
    if (ml->param_cc[param] != MIDI_MAP_NO_CC) {
        unbind(ml, ml->param_cc[param]);
    }
    unbind(ml, cc);
    const midi_map_entry* e = midi_map_for_param(param);
    ml->cc_row[cc] = (uint8_t)(e - midi_map_row(0));
    ml->param_cc[param] = (uint8_t)cc;
    ml->count++;
    settings_changed(ml);
    return 1;
}

void midi_learn_forget(midi_learn* ml, uint32_t param) {
    if (param < LIVE_PARAM_COUNT && ml->param_cc[param] != MIDI_MAP_NO_CC) {
        unbind(ml, ml->param_cc[param]);
    }
}

void midi_learn_clear(midi_learn* ml) {
    settings_changed(ml);
    for (uint32_t cc = 0; cc < 128u; ++cc) {
        ml->cc_row[cc] = MIDI_LEARN_NONE;
    }
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        ml->param_cc[p] = MIDI_MAP_NO_CC;
    }
    ml->count = 0;
}
