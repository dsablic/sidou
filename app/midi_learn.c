// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_learn.h"

#include <stddef.h>

_Static_assert(MIDI_MAP_ROWS < MIDI_LEARN_NONE, "a row index fits cc_row");
_Static_assert(LIVE_PARAM_COUNT < 256u, "count and param are bytes");

void midi_learn_init(midi_learn* ml) {
    midi_learn_clear(ml);
    ml->state = MIDI_LEARN_IDLE;
    ml->param = 0;
    ml->result = MIDI_LEARN_NOTHING;
    ml->result_cc = MIDI_MAP_NO_CC;
    ml->since = 0;
}

int midi_learn_learnable(uint32_t cc) {
    switch (cc) {
    case 6:  /* data entry MSB */
    case 38: /* data entry LSB */
    case 96: /* increment */
    case 97: /* decrement */
    case 98: /* NRPN LSB */
    case 99: /* NRPN MSB */
    case 100: /* RPN LSB */
    case 101: /* RPN MSB */
        return 0;
    default: return cc < 120u; /* 120..127: channel mode */
    }
}

int midi_learn_param(uint32_t param) {
    return param < LIVE_PARAM_COUNT && midi_map_for_param(param) != NULL;
}

/* The state machine's end: idle, with what happened for the UI. */
static void end(midi_learn* ml, midi_learn_result r, uint8_t cc) {
    ml->state = MIDI_LEARN_IDLE;
    ml->result = (uint8_t)r;
    ml->result_cc = cc;
}

void midi_learn_button(midi_learn* ml, uint32_t now) {
    ml->since = now;
    switch (ml->state) {
    case MIDI_LEARN_IDLE:
        ml->state = MIDI_LEARN_KNOB;
        ml->result = MIDI_LEARN_NOTHING;
        break;
    case MIDI_LEARN_KNOB: ml->state = MIDI_LEARN_CONFIRM; break;
    case MIDI_LEARN_CONFIRM:
        midi_learn_clear(ml);
        end(ml, MIDI_LEARN_CLEARED, MIDI_MAP_NO_CC);
        break;
    default: { /* MIDI_LEARN_CC */
        uint8_t cc = ml->param_cc[ml->param];
        midi_learn_forget(ml, ml->param);
        end(ml, MIDI_LEARN_FORGOT, cc);
        break;
    }
    }
}

int midi_learn_knob(midi_learn* ml, uint32_t param, uint32_t now) {
    if (ml->state == MIDI_LEARN_IDLE || !midi_learn_param(param)) {
        return 0;
    }
    ml->state = MIDI_LEARN_CC;
    ml->param = (uint8_t)param;
    ml->since = now;
    return 1;
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

/* cc's binding gone, whoever has it. */
static void unbind(midi_learn* ml, uint32_t cc) {
    const midi_map_entry* e = midi_learn_for_cc(ml, cc);
    if (e) {
        ml->param_cc[e->param] = MIDI_MAP_NO_CC;
        ml->cc_row[cc] = MIDI_LEARN_NONE;
        ml->count--;
    }
}

int midi_learn_bind(midi_learn* ml, uint32_t cc, uint32_t param) {
    if (!midi_learn_learnable(cc) || !midi_learn_param(param)) {
        return 0;
    }
    if (ml->param_cc[param] != MIDI_MAP_NO_CC) {
        unbind(ml, ml->param_cc[param]); /* it moves */
    }
    unbind(ml, cc); /* another parameter's: the CC moves */
    const midi_map_entry* e = midi_map_for_param(param);
    ml->cc_row[cc] = (uint8_t)(e - midi_map_row(0));
    ml->param_cc[param] = (uint8_t)cc;
    ml->count++;
    return 1;
}

void midi_learn_forget(midi_learn* ml, uint32_t param) {
    if (param < LIVE_PARAM_COUNT && ml->param_cc[param] != MIDI_MAP_NO_CC) {
        unbind(ml, ml->param_cc[param]);
    }
}

void midi_learn_clear(midi_learn* ml) {
    for (uint32_t cc = 0; cc < 128u; ++cc) {
        ml->cc_row[cc] = MIDI_LEARN_NONE;
    }
    for (uint32_t p = 0; p < LIVE_PARAM_COUNT; ++p) {
        ml->param_cc[p] = MIDI_MAP_NO_CC;
    }
    ml->count = 0;
}
