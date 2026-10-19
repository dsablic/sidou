// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "panel.h"

#include <stddef.h>

void panel_init(panel* p) {
    for (uint32_t k = 0; k < PANEL_NOTE_KEYS; ++k) {
        p->key_note[k] = LIVE_NO_NOTE;
    }
    p->master_raw = -1;
    p->held = 0;
    p->page = PANEL_PAGE_LIVE;
    p->playing = 0;
    p->glo = 0;
    p->quiet = 0;
    p->sounding = 0;
    p->master_hook = NULL;
    p->master_ctx = NULL;
}

static void key_up(panel* p, live* l, uint32_t k) {
    live_note_off(l, p->key_note[k]);
    p->key_note[k] = LIVE_NO_NOTE;
    p->sounding ^= 1u << k; /* set: key_up runs on a sounding key only */
}

void panel_note_keys(panel* p, live* l, uint32_t down, uint32_t pressed) {
    if (!(down | pressed | p->sounding | p->quiet)) {
        return; /* most calls: no key */
    }
    int glo = (p->held >> PANEL_BTN_GLO) & 1u;
    /* Kept in a local and stored once: pi32v2's read-modify-write "and not" on memory, which
     * clearing a bit of p->quiet in place compiles to, is one fm1-emu does not decode. */
    uint32_t quiet = p->quiet;
    uint32_t toggles = 0;
    for (uint32_t k = 0; k < PANEL_NOTE_KEYS; ++k) {
        uint32_t bit = 1u << k;
        if (quiet & bit) {
            if ((pressed & bit) && glo) {
                toggles |= bit; /* pressed again */
            }
            if (!(down & bit)) {
                quiet ^= bit; /* gone up: the panel's again from the next press */
            }
            continue;
        }
        if (p->key_note[k] != LIVE_NO_NOTE && (pressed & bit)) {
            key_up(p, l, k); /* let go and pressed again since the last call */
        }
        if (p->key_note[k] == LIVE_NO_NOTE && ((down | pressed) & bit)) {
            if (glo) {
                /* GLO's layer: the key is a mute or a solo, and sounds nothing */
                toggles |= bit;
                quiet |= down & bit;
                continue;
            }
            uint8_t n = live_key_note(l, (uint8_t)(PANEL_BASE_NOTE + k));
            if (n != LIVE_NO_NOTE) {
                live_note_on(l, n, PANEL_VELOCITY);
                p->key_note[k] = n;
                p->sounding |= bit;
            }
        }
        if (p->key_note[k] != LIVE_NO_NOTE && !(down & bit)) {
            key_up(p, l, k);
        }
    }
    p->quiet = quiet;
    p->glo |= (uint8_t)(toggles & ((1u << PANEL_GLO_KEYS) - 1u));
}

uint32_t panel_take_glo(panel* p) {
    uint32_t t = p->glo;
    p->glo = 0;
    return t;
}

void panel_quiet_keys(panel* p, live* l, uint32_t mask) {
    mask &= (1u << PANEL_NOTE_KEYS) - 1u;
    for (uint32_t k = 0; mask >> k; ++k) {
        uint32_t bit = 1u << k;
        if (!(mask & bit)) {
            continue;
        }
        if (p->key_note[k] != LIVE_NO_NOTE) {
            key_up(p, l, k);
        }
        p->quiet |= bit;
    }
}

void panel_buttons(panel* p, live* l, uint32_t pressed, uint32_t held) {
    p->held = held;
    if (pressed & (1u << PANEL_BTN_SCL)) {
        live_fx_select_next(l);
    }
    if (pressed & (1u << PANEL_BTN_ENV)) {
        live_slot_next_algorithm(l, l->fx_select);
    }
    if (pressed & (1u << PANEL_BTN_OCT_DOWN)) {
        live_button(l, LIVE_BUTTON_OCT_DOWN);
    }
    if (pressed & (1u << PANEL_BTN_OCT_UP)) {
        live_button(l, LIVE_BUTTON_OCT_UP);
    }
    if (pressed & (1u << PANEL_BTN_FX)) {
        live_button(l, LIVE_BUTTON_FX);
    }
}

uint32_t panel_knob_param(const panel* p, const live* l, uint32_t held, uint32_t enc) {
    static const uint8_t KNOBS[] = {LIVE_PARAM_CUTOFF, LIVE_PARAM_RESONANCE,
                                    LIVE_PARAM_PULSE_WIDTH, LIVE_PARAM_RELEASE};
    if (enc < PANEL_ENC_K1 || enc > PANEL_ENC_K4) {
        return LIVE_PARAM_COUNT;
    }
    uint32_t k = enc - PANEL_ENC_K1;
    uint32_t page = p ? p->page : PANEL_PAGE_LIVE;
    if ((held & (1u << PANEL_BTN_EDIT)) || page == PANEL_PAGE_SLOT) {
        /* K1..K4: Amount, A, B, Mix */
        return LIVE_PARAM_SLOT + l->fx_select * LIVE_SLOT_FIELDS +
               LIVE_SLOT_AMOUNT + k;
    }
    if (page == PANEL_PAGE_CHARACTER) {
        return LIVE_PARAM_AGE + k; /* age, DAC, thump, bleed */
    }
    if (page == PANEL_PAGE_NONE) {
        return LIVE_PARAM_COUNT;
    }
    if (page >= PANEL_PAGE_EXPERT_FIRST && page < PANEL_PAGE_EXPERT_END) {
        uint32_t index = (page - PANEL_PAGE_EXPERT_FIRST) * 4u + k;
        return index < SID_EXPERT_COUNT ? LIVE_PARAM_EXPERT + index : LIVE_PARAM_COUNT;
    }
    if (k == 2u) { /* the sound's own control */
        uint32_t tone = live_tone(l);
        return tone == LIVE_TONE_PULSE ? LIVE_PARAM_PULSE_WIDTH
               : tone == LIVE_TONE_DECAY ? LIVE_PARAM_DECAY
                                         : LIVE_PARAM_COUNT;
    }
    return KNOBS[k];
}

_Static_assert(LIVE_PARAM_DAC == LIVE_PARAM_AGE + 1 && LIVE_PARAM_THUMP == LIVE_PARAM_AGE + 2 &&
                   LIVE_PARAM_BLEED == LIVE_PARAM_AGE + 3,
               "K1..K4 are the character's controls in order");
_Static_assert(LIVE_SLOT_MIX == LIVE_SLOT_AMOUNT + 3, "K1..K4 are a slot's controls in order");

/* live_knob's knobs are K1..K4 in order: the cutoff, resonance, the sound's own control and the
 * release that panel_knob_param names. */
_Static_assert(LIVE_KNOB_CUTOFF == 0 && LIVE_KNOB_RESONANCE == 1 && LIVE_KNOB_TONE == 2 &&
                   LIVE_KNOB_RELEASE == 3 && PANEL_ENC_K4 - PANEL_ENC_K1 == 3,
               "K1..K4 are live_knob's knobs");

__attribute__((noinline, unused)) static void expert_turn(live* l, uint32_t param, int32_t steps) {
    if (param >= LIVE_PARAM_EXPERT && param < LIVE_PARAM_EXPERT_END) {
        int32_t d = steps > 127 ? 127 : steps < -127 ? -127 : steps;
        int32_t value = (int32_t)live_param_get(l, param) + d;
        live_param_set(l, param, (uint32_t)(value < 0 ? 0 : value > 127 ? 127 : value));
    }
}

void panel_encoder(panel* p, live* l, uint32_t enc, int32_t steps) {
    if (steps == 0) {
        return;
    }
    if (enc - PANEL_ENC_K1 > PANEL_ENC_K4 - PANEL_ENC_K1) { /* not K1..K4 */
        if (enc == PANEL_ENC_PRESETS) {
            live_preset_step(l, steps, p->held >> PANEL_BTN_EDIT & 1u);
        }
        return;
    }
    if (p->page == PANEL_PAGE_LIVE && !(p->held >> PANEL_BTN_EDIT & 1u)) {
        /* LIVE's page, the common case: K1..K4 are live's knobs (what panel_knob_param names) */
        live_knob(l, (live_knob_id)(enc - PANEL_ENC_K1), steps);
        return;
    }
    uint32_t param = panel_knob_param(p, l, p->held, enc);
    if (param == LIVE_PARAM_COUNT && p->page >= PANEL_PAGE_NONE) {
        return; /* the view's own */
    }
    if (param >= LIVE_PARAM_SLOT && param < LIVE_PARAM_SLOT_END) {
        uint32_t i = param - LIVE_PARAM_SLOT;
        live_slot_turn(l, i / LIVE_SLOT_FIELDS, i % LIVE_SLOT_FIELDS, steps);
    } else if (param >= LIVE_PARAM_AGE && param <= LIVE_PARAM_BLEED) {
        int32_t d = steps > 64 ? 64 : steps < -64 ? -64 : steps; /* 0..127 end to end in 32 */
        int32_t v = (int32_t)live_param_get(l, param) + d * PANEL_CHARACTER_DETENT;
        int32_t max = (int32_t)live_param_max(param);
        live_param_set(l, param, (uint32_t)(v < 0 ? 0 : v > max ? max : v));
    } else {
        if (p->page >= PANEL_PAGE_EXPERT_FIRST) {
            expert_turn(l, param, steps);
            return;
        }
        live_knob(l, (live_knob_id)(enc - PANEL_ENC_K1), steps);
    }
}

void panel_master(panel* p, live* l, int32_t raw) {
    if (raw < 0) {
        return;
    }
    if (raw > (int32_t)LIVE_MASTER_MAX) {
        raw = LIVE_MASTER_MAX;
    }
    int32_t d = raw - p->master_raw;
    /* the ends always land, so the volume can reach 0 and 15 */
    if (p->master_raw < 0 || d >= PANEL_MASTER_HYSTERESIS || d <= -PANEL_MASTER_HYSTERESIS ||
        ((raw == 0 || raw == (int32_t)LIVE_MASTER_MAX) && d != 0)) {
        p->master_raw = (int16_t)raw;
        live_set_master(l, (uint16_t)raw);
        if (p->master_hook) {
            p->master_hook(p->master_ctx);
        }
    }
}

panel_leds panel_get_leds(const panel* p, const live* l) {
    panel_leds o = {0, 0};
    for (uint32_t k = 0; k < PANEL_NOTE_KEYS; ++k) {
        if (p->key_note[k] != LIVE_NO_NOTE) {
            o.notes |= 1u << k;
        }
    }
    if (l->octave < 0) {
        o.buttons |= 1u << PANEL_BTN_OCT_DOWN;
    }
    if (l->octave > 0) {
        o.buttons |= 1u << PANEL_BTN_OCT_UP;
    }
    if (l->model == SID_MODEL_8580) {
        o.buttons |= 1u << PANEL_BTN_FX;
    }
    if (l->slot[l->fx_select][LIVE_SLOT_ALGORITHM] != 0) {
        o.buttons |= 1u << PANEL_BTN_EDIT;
    }
    if (p->playing) {
        o.buttons |= 1u << PANEL_BTN_PLAY;
    }
    return o;
}
