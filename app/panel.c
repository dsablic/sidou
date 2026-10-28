// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "panel.h"

#include <stddef.h>

void panel_init(panel* p) {
    for (uint32_t k = 0; k < PANEL_NOTE_KEYS; ++k) {
        p->key_note[k] = LIVE_NO_NOTE;
    }
    p->master_raw = -1;
    p->master_shown = -1;
    p->master_follow = 0;
    p->master_wait = 0;
    p->master_pending = 0;
    p->held = 0;
    p->page = PANEL_PAGE_LIVE;
    p->playing = 0;
    p->glo = 0;
    p->glo_latch = 0;
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
    int glo = ((p->held >> PANEL_BTN_GLO) | p->glo_latch) & 1u; /* held, or latched */
    /* Kept in a local and stored once, a known-set bit cleared with ^= (fewer memory
     * read-modify-writes in the loop; it began as a dodge of an and-not form fm1-emu could not
     * decode, fixed upstream in #33). */
    uint32_t quiet = p->quiet;
    uint32_t toggles = 0;
    for (uint32_t k = 0; k < PANEL_NOTE_KEYS; ++k) {
        uint32_t bit = 1u << k;
        if (quiet & bit) {
            if ((pressed & bit) && glo && k < PANEL_GLO_KEYS) {
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
            if (glo && k < PANEL_GLO_KEYS) {
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
    if (page >= PANEL_PAGE_LAB_FIRST) { /* LAB's cards, EDIT held or not */
        return panel_lab_param(l, page - PANEL_PAGE_LAB_FIRST, k);
    }
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

uint32_t panel_lab_param(const live* l, uint32_t page, uint32_t k) {
    static const uint8_t CARDS[5][4] = {
        {LIVE_PARAM_PULSE_WIDTH, LIVE_PARAM_PWM_DEPTH, LIVE_PARAM_PWM_RATE, LIVE_PARAM_TUNE},
        {LIVE_PARAM_ATTACK, LIVE_PARAM_DECAY, LIVE_PARAM_SUSTAIN, LIVE_PARAM_RELEASE},
        {LIVE_PARAM_CUTOFF, LIVE_PARAM_RESONANCE, LIVE_PARAM_FILTER_VOICES, LIVE_PARAM_COUNT},
        {LIVE_PARAM_VIBRATO_RATE, LIVE_PARAM_VIBRATO_DEPTH, LIVE_PARAM_VIBRATO_DELAY,
         LIVE_PARAM_VELOCITY_TARGET},
        {0, 0, 0, 0}, /* FX: the selected slot's */
    };
    _Static_assert(PANEL_PAGE_LAB_END - PANEL_PAGE_LAB_FIRST == sizeof CARDS / sizeof CARDS[0],
                   "a row of cards a LAB page");
    _Static_assert(LIVE_PARAM_COUNT <= 255u, "a card's parameter fits a byte");
    if (page >= sizeof CARDS / sizeof CARDS[0] || k > 3u) {
        return LIVE_PARAM_COUNT;
    }
    if (page == 4u) {
        return LIVE_PARAM_SLOT + l->fx_select * LIVE_SLOT_FIELDS + LIVE_SLOT_AMOUNT + k;
    }
    if (page == 0u && k == 0u && l->mono_sync && !(l->control & 0x40u)) {
        return LIVE_PARAM_COUNT; /* RATIO: the sync ratio's semitones, no parameter */
    }
    return CARDS[page][k];
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

_Static_assert(LIVE_PARAM_BLEED - LIVE_PARAM_AGE + 1u == PANEL_ENC_K4 - PANEL_ENC_K1 + 1u &&
                   LIVE_PARAM_BLEED - LIVE_PARAM_AGE + 1u <= sizeof ((live*)0)->character,
               "the CHARACTER page's knobs are live's character fields, age first");

void panel_encoder(panel* p, live* l, uint32_t enc, int32_t steps) {
    uint32_t k = enc - PANEL_ENC_K1;
    if (steps == 0 || k > PANEL_ENC_K4 - PANEL_ENC_K1) { /* not K1..K4 */
        if (steps != 0 && enc == PANEL_ENC_PRESETS) {
            live_preset_step(l, steps, p->held >> PANEL_BTN_EDIT & 1u);
        }
        return;
    }
    /* What panel_knob_param names, by the page: each page's own turn, without the parameter's
     * number between (the K1 input is gated by its cost: emu-gate screen.rs CHIP_INPUT and
     * POPUP_INPUT). */
    uint32_t page = p->page;
    uint32_t edit = p->held >> PANEL_BTN_EDIT & 1u;
    if (page == PANEL_PAGE_LIVE && !edit) {
        live_knob(l, (live_knob_id)k, steps); /* the common case */
    } else if (edit || page == PANEL_PAGE_SLOT) {
        live_slot_turn(l, l->fx_select, LIVE_SLOT_AMOUNT + k, steps); /* Amount, A, B, Mix */
    } else if (page == PANEL_PAGE_CHARACTER) {
        uint32_t param = LIVE_PARAM_AGE + k; /* age, DAC, thump, bleed */
        int32_t d = steps > 64 ? 64 : steps < -64 ? -64 : steps; /* 0..127 end to end in 32 */
        /* live_param_get's and live_param_max's answers for these four, read directly */
        int32_t v = (int32_t)l->character[k] + d * PANEL_CHARACTER_DETENT;
        int32_t max = (int32_t)SID_CHARACTER_MAX;
        live_param_set(l, param, (uint32_t)(v < 0 ? 0 : v > max ? max : v));
    } else if (page >= PANEL_PAGE_EXPERT_FIRST) {
        uint32_t param = panel_knob_param(p, l, p->held, enc);
        if (param != LIVE_PARAM_COUNT) {
            expert_turn(l, param, steps);
        }
    } /* PANEL_PAGE_NONE: the view's own */
}

void panel_master(panel* p, live* l, int32_t raw) {
    if (raw < 0) {
        return;
    }
    if (raw > (int32_t)LIVE_MASTER_MAX - PANEL_MASTER_END) {
        raw = LIVE_MASTER_MAX;
    } else if (raw < PANEL_MASTER_END) {
        raw = 0;
    }
    int first = p->master_raw < 0;
    int32_t d = raw - p->master_raw;
    if (p->master_follow && !--p->master_follow) {
        p->master_shown = p->master_raw; /* the turn is over: the next is measured from here */
    }
    if (p->master_wait) {
        p->master_wait--;
    }
    /* the ends always land, so the volume can reach 0 and 15 */
    if (first || d >= PANEL_MASTER_HYSTERESIS || d <= -PANEL_MASTER_HYSTERESIS ||
        ((raw == 0 || raw == (int32_t)LIVE_MASTER_MAX) && d != 0)) {
        p->master_raw = (int16_t)raw;
        live_set_master(l, (uint16_t)raw);
        int32_t turn = raw - p->master_shown;
        if (first || turn >= PANEL_MASTER_POPUP || turn <= -PANEL_MASTER_POPUP) {
            p->master_shown = (int16_t)raw;
            p->master_follow = first ? 0u : (uint8_t)PANEL_MASTER_FOLLOW;
            p->master_pending = 1;
        } else if (p->master_follow) {
            p->master_pending = 1;
        }
    }
    /* An ask at most every PANEL_MASTER_ASK_GAP calls, the last value asked once it is free:
     * the screen opens the popup only in frames that bring no ask (app/screen.c popup_step). */
    if (p->master_pending && !p->master_wait) {
        p->master_pending = 0;
        p->master_wait = first ? 0u : (uint8_t)PANEL_MASTER_ASK_GAP;
        if (p->master_hook) {
            p->master_hook(p->master_ctx);
        }
    }
}

static const panel_led_in NO_IN = {0, PANEL_BEAT_NONE, 0, 0, 0, 0, 0, PANEL_NO_STEP, 0, 0, 0};

/* ARP's flash: the arpeggio steps, and its step began less than half a step ago (a clock rate
 * with a clock counts its clocks, else frames). The fastest rates flash once a cycle. */
static int arp_flash(const live* l) {
    if (l->arp_voice == LIVE_ARP_NO_VOICE) {
        return 0;
    }
    uint32_t steps = live_arp_steps[l->arp_rate];
    int clocked = l->arp_rate >= LIVE_ARP_RATE_CLOCK && l->arp_clock_seen;
    uint32_t total = clocked ? steps : steps * LIVE_ARP_TICKS_PER_FRAME;
    uint32_t left = clocked ? l->arp_clocks : l->arp_wait;
    if (left > total || (steps <= PANEL_ARP_FAST_FRAMES && !clocked && l->arp_pos != 0u)) {
        return 0;
    }
    return (total - left) * 2u < total;
}

__attribute__((noinline, unused)) static void glo_layer(const panel_led_in* in, int blink,
                                                        panel_leds* o) {
    uint32_t keys = (1u << PANEL_GLO_KEYS) - 1u;
    o->notes &= ~keys;
    o->notes_dim &= ~keys;
    for (uint32_t v = 0; v < PANEL_GLO_KEYS / 2u; ++v) {
        if (!(in->silent >> v & 1u)) {
            o->notes |= 1u << v; /* sounding */
        }
        if (in->solo >> v & 1u) {
            if (blink) {
                o->notes |= 1u << (v + PANEL_GLO_KEYS / 2u);
            }
        } else {
            o->notes_dim |= 1u << (v + PANEL_GLO_KEYS / 2u);
        }
    }
}

__attribute__((noinline, unused)) static void step_page(const panel_led_in* in, panel_leds* o) {
    uint32_t keys = (1u << PANEL_STEPS) - 1u;
    uint32_t set = in->step_set & in->step_rows;
    o->notes &= ~keys;
    o->notes_dim &= ~keys;
    o->notes |= set;
    o->notes_dim |= in->step_rows & ~set;
    uint32_t bit = in->step_head < PANEL_STEPS ? 1u << in->step_head : 0u;
    if ((bit & in->step_rows) && in->ms - in->head_ms < PANEL_FLICK_MS) {
        o->notes ^= bit; /* a note goes dark, an empty step lights */
        o->notes_dim &= ~bit;
    }
}

panel_leds panel_get_leds(const panel* p, const live* l, const panel_led_in* in) {
    panel_leds o = {0, 0, 0, 0};
    if (!in) {
        in = &NO_IN;
    }
    int blink = (in->ms / PANEL_BLINK_HALF_MS & 1u) == 0u;
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
    if (l->arp_on) {
        o.buttons |= (arp_flash(l) ? 1u : 0u) << PANEL_BTN_ARP;
        o.buttons_dim |= 1u << PANEL_BTN_ARP;
    }
    if (p->playing) {
        o.buttons |= (in->beat < PANEL_BEAT_LIT ? 1u : 0u) << PANEL_BTN_PLAY;
        o.buttons_dim |= 1u << PANEL_BTN_PLAY;
    }
    if (in->rec == PANEL_REC_ON || (in->rec == PANEL_REC_COUNT && blink)) {
        o.buttons |= 1u << PANEL_BTN_REC;
    } else if (in->rec == PANEL_REC_ARMED) {
        o.buttons_dim |= 1u << PANEL_BTN_REC;
    }
    if (in->ask && blink) {
        o.buttons |= 1u << PANEL_BTN_SAVE;
    }
    if (p->glo_latch) {
        o.buttons |= 1u << PANEL_BTN_GLO;
    }
    if (in->step_on) {
        step_page(in, &o);
    }
    if (((p->held >> PANEL_BTN_GLO) | p->glo_latch) & 1u) {
        glo_layer(in, blink, &o);
    }
    o.buttons_dim &= ~o.buttons;
    o.notes_dim &= ~o.notes;
    return o;
}
