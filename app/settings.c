// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "settings.h"

#include "chip_spread.h"

#include <string.h>

_Static_assert(LIVE_PARAM_EXPERT == SETTINGS_PARAMS_V1 &&
                   LIVE_PARAM_COUNT == LIVE_PARAM_EXPERT_END + SETTINGS_PARAMS_V8,
               "a new live parameter: append it to the record in a new schema version");
_Static_assert(LIVE_PARAM_EXPERT_END - LIVE_PARAM_EXPERT == SID_EXPERT_COUNT, "the expert parameter range");
_Static_assert(SID_EXPERT_COUNT <= UINT8_MAX, "the expert count fits one byte");
_Static_assert(LIVE_PARAM_CUTOFF < LIVE_PARAM_PULSE_WIDTH, "the two wide parameters, in order");
_Static_assert(SETTINGS_BYTES <= STORE_VALUE_MAX, "the record fits a store value");

/* The parameters a record holds in two bytes: their ranges pass 255. */
static int wide(uint32_t p) {
    return p == LIVE_PARAM_CUTOFF || p == LIVE_PARAM_PULSE_WIDTH;
}

void settings_init(settings* s, live* l, midi_learn* ml, uint8_t* cards, uint8_t* tracker_rows, ui_sound* snd,
                   store* st, const char* version, void (*force)(int on)) {
    memset(s, 0, sizeof *s);
    s->l = l;
    s->ml = ml;
    s->cards = cards;
    s->tracker_rows = tracker_rows;
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
    s->l->settings_dirty =
        (s->seen != s->saved || s->edit_seen != s->edit_saved || s->asked || s->result) ? 2u : 0u;
}

void settings_sounds(settings* s, user_sounds* sounds) {
    s->sounds = sounds;
}

uint32_t settings_gather(const settings* s, uint8_t* out) {
    const live* l = s->l;
    uint32_t n = 0;
    out[n++] = SETTINGS_VERSION;
    out[n++] = *s->cards;
    out[n++] = (uint8_t)(s->snd && s->snd->on);
    out[n++] = l->keys_mode;
    out[n++] = user_sound_preset_byte(l);
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
    out[n++] = *s->tracker_rows;
    out[n++] = l->theme;
    for (uint32_t i = 0; i < STORE_NAME_MAX; i++) {
        out[n++] = (uint8_t)s->last_song[i];
    }
    out[n++] = SID_EXPERT_COUNT;
    for (uint32_t e = 0; e < SID_EXPERT_COUNT; e++) {
        out[n++] = l->expert[e];
    }
    for (uint32_t e = 0; e < SID_EXPERT_COUNT; e++) {
        out[n++] = s->ml ? midi_learn_cc_of(s->ml, LIVE_PARAM_EXPERT + e) : (uint8_t)MIDI_MAP_NO_CC;
    }
    out[n++] = l->spread;
    out[n++] = s->sid_model;
    out[n++] = SETTINGS_PARAMS_V8;
    for (uint32_t p = LIVE_PARAM_EXPERT_END; p < LIVE_PARAM_COUNT; p++) {
        out[n++] = s->ml ? midi_learn_cc_of(s->ml, p) : (uint8_t)MIDI_MAP_NO_CC;
    }
    return n;
}

int settings_apply(settings* s, const uint8_t* rec, uint32_t len) {
    /* Schema 1 or later: the v1 part (a later schema appends). */
    if (len < SETTINGS_V1_BYTES || rec[0] < 1u || rec[5] < SETTINGS_PARAMS_V1 ||
        (rec[0] >= 2u && len < SETTINGS_V2_BYTES) ||
        (rec[0] >= 3u && len < SETTINGS_V3_BYTES) ||
        (rec[0] >= 4u && len < SETTINGS_V4_BYTES)) {
        return 0;
    }
    uint32_t experts = 0;
    if (rec[0] >= 5u) {
        if (len < SETTINGS_V4_BYTES + 1u) {
            return 0;
        }
        experts = rec[SETTINGS_V4_BYTES];
        if (experts < SID_EXPERT_COUNT || len < SETTINGS_V4_BYTES + 1u + 2u * experts) {
            return 0;
        }
        if (rec[0] >= 6u && len < SETTINGS_V4_BYTES + 2u + 2u * experts) {
            return 0;
        }
        if (rec[0] >= 7u && len < SETTINGS_V4_BYTES + 3u + 2u * experts) {
            return 0;
        }
    }
    /* Schema 8: the parameters past the experts, a count and each one's learned CC. */
    uint32_t tail_at = SETTINGS_V4_BYTES + 3u + 2u * experts, tail = 0;
    if (rec[0] >= 8u) {
        if (len < tail_at + 1u || len < tail_at + 1u + rec[tail_at]) {
            return 0;
        }
        tail = rec[tail_at];
    }
    live* l = s->l;
    *s->cards = rec[1] <= 2u ? rec[1] : 1u; /* an old record's 1 (ON) is ONCE */
    if (s->snd) {
        ui_sound_set(s->snd, rec[2] != 0);
    }
    live_set_keys_mode(l, rec[3]);
    /* first: a preset resets its edits. A USER sound's from the store (an empty slot: SAW LEAD);
     * past the USER sounds, clamped to DICE as an older firmware does. */
    uint32_t user = (uint32_t)rec[4] - LIVE_PRESET_SLOTS;
    if (user >= USER_SOUNDS) {
        live_set_preset(l, rec[4]);
    } else if (!user_sound_load(s->sounds, user)) {
        live_set_preset(l, 0);
    }
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
    *s->tracker_rows = (uint8_t)(rec[0] >= 2u && rec[SETTINGS_V1_BYTES] == 1u);
    live_set_theme(l, rec[0] >= 3u ? rec[SETTINGS_V2_BYTES] : 0u); /* an older record: HIGH CONTRAST */
    memset(s->last_song, 0, sizeof s->last_song);
    if (rec[0] >= 4u) {
        for (uint32_t i = 0; i < STORE_NAME_MAX && rec[SETTINGS_V3_BYTES + i]; i++) {
            uint8_t c = rec[SETTINGS_V3_BYTES + i];
            if (c < 0x20u || c > 0x5fu || (i == 0u && c == ' ')) {
                memset(s->last_song, 0, sizeof s->last_song);
                break;
            }
            s->last_song[i] = (char)c;
        }
    }
    l->expert_active = 0;
    int expert_changed = 0;
    for (uint32_t e = 0; e < SID_EXPERT_COUNT; e++) {
        uint32_t value = experts ? rec[SETTINGS_V4_BYTES + 1u + e] : SID_EXPERT_DEFAULT;
        uint8_t raw = value <= SID_CHARACTER_MAX ? (uint8_t)value : SID_EXPERT_DEFAULT;
        expert_changed |= l->expert[e] != raw;
        l->expert[e] = raw;
        l->expert_active |= l->expert[e] != SID_EXPERT_DEFAULT;
        if (s->ml && experts) {
            uint8_t cc = rec[SETTINGS_V4_BYTES + 1u + experts + e];
            if (cc != MIDI_MAP_NO_CC) {
                (void)midi_learn_bind(s->ml, cc, LIVE_PARAM_EXPERT + e);
            }
        }
    }
    l->expert_version += (uint32_t)expert_changed;
    for (uint32_t i = 0; s->ml && i < tail && i < SETTINGS_PARAMS_V8; i++) {
        uint8_t cc = rec[tail_at + 1u + i];
        if (cc != MIDI_MAP_NO_CC) {
            (void)midi_learn_bind(s->ml, cc, LIVE_PARAM_EXPERT_END + i);
        }
    }
    /* SPREAD: mono in an older record, and for a value this firmware does not know. */
    uint8_t spread = rec[0] >= 6u ? rec[SETTINGS_V4_BYTES + 1u + 2u * experts] : 0u;
    chip_spread_set(l, spread <= CHIP_SPREAD_MAX ? spread : 0u);
    /* SID MODEL: AUTO in an older record, and for a value this firmware does not know. */
    uint8_t model = rec[0] >= 7u ? rec[SETTINGS_V4_BYTES + 2u + 2u * experts] : SETTINGS_SID_AUTO;
    s->sid_model = model == SETTINGS_SID_KEYS ? SETTINGS_SID_KEYS : SETTINGS_SID_AUTO;
    l->expert_dirty = 1;
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
    uint8_t rec[STORE_VALUE_MAX];
    int same = 0;
    if (s->st) {
        int n = store_get(s->st, SETTINGS_KEY, rec, sizeof rec, 0);
        same = n > 0 && settings_apply(s, rec, (uint32_t)n);
    }
    /* The unsaved edit over what they loaded, when it is of the sound they loaded. */
    int edit = 0;
    if (s->st) {
        int n = store_get(s->st, USER_SOUND_EDIT_KEY, rec, sizeof rec, 0);
        edit = n > 0 && user_sound_edit_apply(s->l, rec, (uint32_t)n > sizeof rec ? sizeof rec
                                                                                  : (uint32_t)n);
    }
    /* What was loaded counts as saved; a new version differs, and the debounce stores it. */
    uint8_t now[SETTINGS_BYTES];
    uint32_t len = settings_gather(s, now);
    s->seen = store_crc32(0, now, len);
    s->saved = same ? s->seen : 0u;
    len = user_sound_edit_gather(s->l, now);
    s->edit_seen = store_crc32(0, now, len);
    s->edit_saved = edit ? s->edit_seen : 0u;
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
        s->errors = (uint8_t)s->st->errors;
        return 1;
    }
    if (rc == STORE_FOREIGN || rc == STORE_FULL || rc == STORE_INVALID || rc == STORE_ERROR) {
        s->saved = crc; /* nothing to wait for */
        if (s->asked) {
            s->result = rc == STORE_ERROR ? SETTINGS_FAILED : SETTINGS_FOREIGN;
            s->asked = 0;
        }
    }
    return 0;
}

/* The unsaved edit to the store, once it is free (the settings record goes first): 1 if it
 * took it. */
static int write_edit(settings* s, const uint8_t* rec, uint32_t len, uint32_t crc) {
    if (!s->st) {
        s->edit_saved = crc;
        return 1;
    }
    if (store_busy(s->st)) {
        return 0;
    }
    memcpy(s->edit, rec, len);
    int rc = store_put(s->st, USER_SOUND_EDIT_KEY, USER_SOUND_EDIT_TYPE, s->edit, len);
    if (rc == STORE_OK || rc == STORE_FOREIGN || rc == STORE_FULL || rc == STORE_INVALID ||
        rc == STORE_ERROR) {
        s->edit_saved = crc; /* taken, or nothing to wait for */
    }
    return rc == STORE_OK;
}

/* The unsaved edit once the settings record is saved and no explicit save waits on it: by the
 * same debounce, or right after an explicit save (its result is the settings record's). */
static void edit_due(settings* s, const uint8_t* edit, uint32_t len, uint32_t crc, int explicit,
                     uint32_t now_ms) {
    if (!s->asked && s->seen == s->saved && crc != s->edit_saved &&
        (explicit || now_ms - s->changed_at >= SETTINGS_DEBOUNCE_MS)) {
        (void)write_edit(s, edit, len, crc);
    }
    pending(s);
}

/* Gather a changed record, save when due, and finish an explicit save. */
__attribute__((noinline, unused)) static void update(settings* s, uint32_t now_ms) {
    uint8_t rec[SETTINGS_BYTES];
    uint32_t len = settings_gather(s, rec);
    uint32_t crc = store_crc32(0, rec, len);
    uint8_t edit[USER_SOUND_EDIT_BYTES];
    uint32_t edit_len = user_sound_edit_gather(s->l, edit);
    uint32_t edit_crc = store_crc32(0, edit, edit_len);
    int explicit = s->asked != 0;
    s->l->settings_dirty = 0;
    if (s->ml) {
        s->ml->settings_dirty = 0;
    }
    if (crc != s->seen || edit_crc != s->edit_seen) {
        s->seen = crc;
        s->edit_seen = edit_crc;
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
        edit_due(s, edit, edit_len, edit_crc, explicit, now_ms);
        return;
    }
    if (s->asked == 2 && (!s->st || !store_busy(s->st))) {
        /* Idle after it took the record: committed, unless a program failed meanwhile (the
         * write is undecided then): not saved, and the debounce writes it again. */
        if (s->st && (uint8_t)s->st->errors != s->errors) {
            s->result = SETTINGS_FAILED;
            s->saved = ~s->saved;
            s->changed_at = now_ms;
        } else {
            s->result = s->forcing ? SETTINGS_SAVED_GAPS : SETTINGS_SAVED;
        }
        s->asked = 0;
    } else if (!s->forcing && now_ms - s->asked_at >= SETTINGS_FORCE_MS && s->force) {
        s->forcing = 1;
        s->force(1);
    }
    if (!s->asked && s->forcing) {
        s->forcing = 0;
        s->force(0);
    }
    edit_due(s, edit, edit_len, edit_crc, explicit, now_ms);
}

void settings_tick(settings* s, uint32_t now_ms) {
    if (!s->started) {
        return;
    }
    int dirty = (s->l->settings_dirty & 1u) || (s->ml && s->ml->settings_dirty);
    /* The unsaved edit waits for the store the settings record keeps busy (seconds while the
     * keys play): it is gathered again only once the store can take it, not every frame. */
    int edit = s->edit_seen != s->edit_saved && !(s->st && store_busy(s->st));
    if (dirty || s->asked ||
        ((s->seen != s->saved || edit) && now_ms - s->changed_at >= SETTINGS_DEBOUNCE_MS)) {
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

void settings_reset(settings* s) {
    uint8_t rec[SETTINGS_BYTES];
    uint32_t len = settings_gather(s, rec); /* the version and the last song stay */
    uint16_t factory[LIVE_PARAM_COUNT] = {0};
    live_preset_params(0, factory);
    factory[LIVE_PARAM_ENVELOPE_MODE] = LIVE_ENVELOPE_INSTANT;
    factory[LIVE_PARAM_PROFILE] = (uint16_t)sid_chip_default(SID_MODEL_6581);
    rec[1] = 1u; /* CARDS: ONCE */
    rec[2] = 0u; /* SOUNDS off */
    rec[3] = LIVE_KEYS_AUTO;
    rec[4] = 0u;
    uint32_t n = 6;
    for (uint32_t p = 0; p < SETTINGS_PARAMS_V1; p++) {
        rec[n++] = (uint8_t)factory[p];
        if (wide(p)) {
            rec[n++] = (uint8_t)(factory[p] >> 8);
        }
    }
    rec[SETTINGS_V1_BYTES] = 0u;                         /* TRACKER ROWS: BAR */
    rec[SETTINGS_V2_BYTES] = 0u;                         /* THEME: HIGH CONTRAST */
    memset(rec + SETTINGS_V4_BYTES + 1u, SID_EXPERT_DEFAULT, SID_EXPERT_COUNT);
    rec[SETTINGS_V5_BYTES] = 0u; /* SPREAD: mono */
    rec[SETTINGS_V6_BYTES] = SETTINGS_SID_AUTO;
    for (uint32_t i = 0; i < LIVE_PRESET_SLOTS; i++) {
        s->l->edits[i].valid = 0; /* every preset's edits: the factory's */
    }
    (void)settings_apply(s, rec, len);
    s->l->settings_dirty = 1;
}

void settings_forget(settings* s) {
    s->saved = 0;
    s->edit_saved = 0;
    s->l->settings_dirty |= 2u;
}

void settings_set_song(settings* s, const char* name) {
    char next[STORE_NAME_MAX + 1u] = {0};
    for (uint32_t i = 0; i < STORE_NAME_MAX && name[i]; i++) {
        uint8_t c = (uint8_t)name[i];
        if (c < 0x20u || c > 0x5fu || (i == 0u && c == ' ')) {
            return;
        }
        next[i] = (char)c;
    }
    if (memcmp(next, s->last_song, sizeof next) != 0) {
        memcpy(s->last_song, next, sizeof next);
        s->l->settings_dirty = 1u;
    }
}

settings_result settings_take_result(settings* s) {
    settings_result r = (settings_result)s->result;
    s->result = SETTINGS_NOTHING;
    if (s->l->settings_dirty == 2u) {
        pending(s);
    }
    return r;
}
