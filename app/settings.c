// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "settings.h"

#include <string.h>

_Static_assert(LIVE_PARAM_COUNT == SETTINGS_PARAMS_V1,
               "a new live parameter: append it to the record in a new schema version");
_Static_assert(LIVE_PARAM_CUTOFF < LIVE_PARAM_PULSE_WIDTH, "the two wide parameters, in order");
_Static_assert(SETTINGS_BYTES <= STORE_VALUE_MAX, "the record fits a store value");

/* The parameters a record holds in two bytes: their ranges pass 255. */
static int wide(uint32_t p) {
    return p == LIVE_PARAM_CUTOFF || p == LIVE_PARAM_PULSE_WIDTH;
}

void settings_init(settings* s, live* l, midi_learn* ml, uint8_t* cards, ui_sound* snd,
                   store* st, const char* version, void (*force)(int on)) {
    memset(s, 0, sizeof *s);
    s->l = l;
    s->ml = ml;
    s->cards = cards;
    s->snd = snd;
    s->st = st;
    s->version = version;
    s->force = force;
    if (ml) {
        ml->settings_wake = &l->settings_dirty;
    }
}

/* Setters write 1; 2 keeps the debounce, retry or unconsumed toast awake without gathering. */
static void pending(settings* s) {
    s->l->settings_dirty = (s->seen != s->saved || s->asked || s->result) ? 2u : 0u;
}

uint32_t settings_gather(const settings* s, uint8_t* out) {
    const live* l = s->l;
    uint32_t n = 0;
    out[n++] = SETTINGS_VERSION;
    out[n++] = (uint8_t)(*s->cards != 0);
    out[n++] = (uint8_t)(s->snd && s->snd->on);
    out[n++] = l->keys_mode;
    out[n++] = l->preset;
    out[n++] = SETTINGS_PARAMS_V1;
    for (uint32_t p = 0; p < SETTINGS_PARAMS_V1; p++) {
        uint32_t v = p == LIVE_PARAM_VOLUME ? 0u : live_param_get(l, p); /* the pot's */
        out[n++] = (uint8_t)v;
        if (wide(p)) {
            out[n++] = (uint8_t)(v >> 8);
        }
    }
    for (uint32_t p = 0; p < SETTINGS_PARAMS_V1; p++) {
        out[n++] = s->ml ? midi_learn_cc_of(s->ml, p) : (uint8_t)MIDI_MAP_NO_CC;
    }
    for (uint32_t i = 0; i < SETTINGS_VERSION_CHARS; i++) {
        out[n++] = 0;
    }
    for (uint32_t i = 0; i < SETTINGS_VERSION_CHARS && s->version[i]; i++) {
        out[n - SETTINGS_VERSION_CHARS + i] = (uint8_t)s->version[i];
    }
    return n;
}

int settings_apply(settings* s, const uint8_t* rec, uint32_t len) {
    /* Schema 1 or later: the v1 part (a later schema appends). */
    if (len < SETTINGS_V1_BYTES || rec[0] < 1u || rec[5] < SETTINGS_PARAMS_V1) {
        return 0;
    }
    live* l = s->l;
    *s->cards = (uint8_t)(rec[1] != 0);
    if (s->snd) {
        ui_sound_set(s->snd, rec[2] != 0);
    }
    live_set_keys_mode(l, rec[3]);
    live_set_preset(l, rec[4]); /* first: a preset resets its edits */
    uint32_t n = 6;
    for (uint32_t p = 0; p < SETTINGS_PARAMS_V1; p++) {
        uint32_t v = rec[n++];
        if (wide(p)) {
            v |= (uint32_t)rec[n++] << 8;
        }
        if (p != LIVE_PARAM_VOLUME && v <= live_param_max(p)) {
            live_param_set(l, p, v); /* the volume is the MASTER pot's */
        }
    }
    if (s->ml) {
        midi_learn_clear(s->ml);
        for (uint32_t p = 0; p < SETTINGS_PARAMS_V1; p++) {
            if (rec[n + p] != MIDI_MAP_NO_CC) {
                (void)midi_learn_bind(s->ml, rec[n + p], p);
            }
        }
    }
    n += SETTINGS_PARAMS_V1;
    for (uint32_t i = 0; i < SETTINGS_VERSION_CHARS; i++) {
        char c = s->version[i];
        if ((uint8_t)c != rec[n + i]) {
            return 0;
        }
        if (!c) {
            break;
        }
    }
    return 1;
}

int settings_load(settings* s, uint32_t now_ms) {
    uint8_t rec[SETTINGS_BYTES];
    int same = 0;
    if (s->st) {
        int n = store_get(s->st, SETTINGS_KEY, rec, sizeof rec, 0);
        same = n > 0 && settings_apply(s, rec, (uint32_t)n);
    }
    /* What was loaded counts as saved; a new version differs, and the debounce stores it. */
    uint8_t now[SETTINGS_BYTES];
    uint32_t len = settings_gather(s, now);
    s->seen = store_crc32(0, now, len);
    s->saved = same ? s->seen : 0u;
    s->changed_at = now_ms;
    s->started = 1;
    pending(s);
    if (s->ml) {
        s->ml->settings_dirty = 0;
    }
    return same;
}

/* Hands the record to the store: 1 if it took it (it commits on its own polls). */
static int write(settings* s, const uint8_t* rec, uint32_t len, uint32_t crc) {
    if (!s->st) {
        return 1;
    }
    if (store_busy(s->st)) {
        return 0; /* the last write still programs from out */
    }
    memcpy(s->out, rec, len);
    int rc = store_put(s->st, SETTINGS_KEY, SETTINGS_TYPE, s->out, len);
    if (rc == STORE_OK) {
        s->saved = crc;
        return 1;
    }
    if (rc == STORE_FOREIGN || rc == STORE_FULL || rc == STORE_INVALID) {
        s->saved = crc; /* nothing to wait for */
        if (s->asked) {
            s->result = SETTINGS_FOREIGN;
            s->asked = 0;
        }
    }
    return 0;
}

/* Gather a changed record, save when due, and finish an explicit save. */
__attribute__((noinline, unused)) static void update(settings* s, uint32_t now_ms) {
    uint8_t rec[SETTINGS_BYTES];
    uint32_t len = settings_gather(s, rec);
    uint32_t crc = store_crc32(0, rec, len);
    s->l->settings_dirty = 0;
    if (s->ml) {
        s->ml->settings_dirty = 0;
    }
    if (crc != s->seen) {
        s->seen = crc;
        s->changed_at = now_ms;
    }
    if (s->asked == 1) {
        if (write(s, rec, len, crc)) {
            s->asked = 2;
        }
    } else if (crc != s->saved && now_ms - s->changed_at >= SETTINGS_DEBOUNCE_MS) {
        (void)write(s, rec, len, crc);
    }
    if (!s->asked) {
        pending(s);
        return;
    }
    if (s->asked == 2 && (!s->st || !store_busy(s->st))) {
        s->result = s->forcing ? SETTINGS_SAVED_GAPS : SETTINGS_SAVED;
        s->asked = 0;
    } else if (!s->forcing && now_ms - s->asked_at >= SETTINGS_FORCE_MS && s->force) {
        s->forcing = 1;
        s->force(1);
    }
    if (!s->asked && s->forcing) {
        s->forcing = 0;
        s->force(0);
    }
    pending(s);
}

void settings_tick(settings* s, uint32_t now_ms) {
    if (!s->started) {
        return;
    }
    int dirty = (s->l->settings_dirty & 1u) || (s->ml && s->ml->settings_dirty);
    if (dirty || s->asked ||
        (s->seen != s->saved && now_ms - s->changed_at >= SETTINGS_DEBOUNCE_MS)) {
        update(s, now_ms);
    }
}

void settings_save_now(settings* s, uint32_t now_ms) {
    if (!s->asked) {
        s->asked = 1;
        s->asked_at = now_ms;
    }
    settings_tick(s, now_ms);
    if (s->asked) {
        s->l->settings_dirty |= 2u;
    }
}

void settings_forget(settings* s) {
    s->saved = 0;
    s->l->settings_dirty |= 2u;
}

settings_result settings_take_result(settings* s) {
    settings_result r = (settings_result)s->result;
    s->result = SETTINGS_NOTHING;
    if (s->l->settings_dirty == 2u) {
        pending(s);
    }
    return r;
}
