// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "user_sound.h"

#include <string.h>

_Static_assert(USER_SOUNDS == LIVE_SOUNDS, "live.h counts the same sounds");
_Static_assert(USER_SOUNDS <= 16u, "live.sounds_valid is 16 bits");
_Static_assert(USER_SOUND_FILE_BYTES <= STORE_FILE_SECTOR_DATA, "the file is one sector's");
_Static_assert(LIVE_PRESET_SLOTS + USER_SOUNDS <= 255u, "the preset byte holds every sound");
_Static_assert(USER_SOUND_EDIT_BYTES <= STORE_VALUE_MAX, "the edit fits a store value");

enum { IDLE, BEGIN, WRITE, COMMIT };

#define PIECE 64u

static int open_file(const store* st, store_file* f, uint32_t* stride) {
    uint8_t h[USER_SOUND_HEADER];
    if (!st || store_file_open(st, STORE_SOUNDS_NAME, f) != STORE_OK ||
        f->type != STORE_FILE_SOUNDS || store_file_read(st, f, 0, h, sizeof h) != (int)sizeof h ||
        h[0] < 1u || h[1] < LAB_SOUND_BYTES_1_0) {
        return 0;
    }
    *stride = 1u + LAB_SOUND_NAME + h[1];
    return h[2] >= USER_SOUNDS && f->length >= USER_SOUND_HEADER + USER_SOUNDS * *stride;
}

static int slot_of(const store* st, const store_file* f, uint32_t stride, uint32_t n,
                   uint8_t rec[USER_SOUND_STRIDE]) {
    uint32_t k = stride < USER_SOUND_STRIDE ? stride : USER_SOUND_STRIDE;
    memset(rec, 0, USER_SOUND_STRIDE);
    if (store_file_read(st, f, USER_SOUND_HEADER + n * stride, rec, k) != (int)k || rec[0] != 1u) {
        return 0;
    }
    for (uint32_t i = 0; i < LAB_SOUND_NAME && rec[1u + i]; i++) {
        if (rec[1u + i] < 0x20u || rec[1u + i] > 0x5fu) {
            return 0;
        }
    }
    return (int)lab_sound_valid(&rec[1u + LAB_SOUND_NAME]);
}

static int load_hook(void* ctx, uint32_t n);

void user_sounds_init(user_sounds* u, live* l, store* st) {
    memset(u, 0, sizeof *u);
    u->l = l;
    u->st = st;
    l->user_load = load_hook;
    l->user_ctx = u;
}

void user_sounds_load(user_sounds* u) {
    live* l = u->l;
    l->sounds_valid = 0;
    store_file f;
    uint32_t stride;
    if (!open_file(u->st, &f, &stride)) {
        return;
    }
    for (uint32_t n = 0; n < USER_SOUNDS; n++) {
        uint8_t rec[USER_SOUND_STRIDE];
        if (slot_of(u->st, &f, stride, n, rec)) {
            l->sounds_valid = (uint16_t)(l->sounds_valid | 1u << n);
        }
    }
}

void user_sounds_forget(user_sounds* u) {
    u->l->sounds_valid = 0;
    u->l->user_edit_slot = 0;
    u->l->user_sound = 0;
    u->state = IDLE;
}

int user_sound_read(const user_sounds* u, uint32_t n, char* name,
                    uint8_t sound[LAB_SOUND_BYTES]) {
    store_file f;
    uint32_t stride;
    uint8_t rec[USER_SOUND_STRIDE];
    if (!u || n >= USER_SOUNDS || !open_file(u->st, &f, &stride) ||
        !slot_of(u->st, &f, stride, n, rec)) {
        return 0;
    }
    memcpy(name, &rec[1], LAB_SOUND_NAME);
    name[LAB_SOUND_NAME] = 0;
    memcpy(sound, &rec[1u + LAB_SOUND_NAME], LAB_SOUND_BYTES);
    return 1;
}

int user_sound_load(user_sounds* u, uint32_t n) {
    char name[LAB_SOUND_NAME + 1u];
    uint8_t sound[LAB_SOUND_BYTES];
    if (!user_sound_read(u, n, name, sound)) {
        return 0;
    }
    live_load_user(u->l, n, sound, name);
    return 1;
}

static int load_hook(void* ctx, uint32_t n) {
    return user_sound_load(ctx, n);
}

int user_sound_save(user_sounds* u, uint32_t n, const char* name,
                    const uint8_t sound[LAB_SOUND_BYTES], uint32_t now_ms) {
    if (n >= USER_SOUNDS || (u->st && u->state != IDLE)) {
        return -1;
    }
    live* l = u->l;
    uint8_t* rec = u->rec;
    memset(rec, 0, sizeof u->rec);
    rec[0] = 1u;
    for (uint32_t i = 0; i < LAB_SOUND_NAME && name[i]; i++) {
        rec[1u + i] = (uint8_t)name[i];
    }
    memcpy(&rec[1u + LAB_SOUND_NAME], sound, LAB_SOUND_BYTES);
    l->sounds_valid = (uint16_t)(l->sounds_valid | 1u << n);
    live_sound_set_base(l, sound, name);
    l->user_sound = (uint8_t)(n + 1u);
    l->settings_dirty = 1;
    if (!u->st) {
        return 1;
    }
    u->slot = (uint8_t)n;
    u->state = BEGIN;
    u->since = now_ms;
    return 0;
}

int user_sounds_busy(const user_sounds* u) {
    return u->state != IDLE || u->result;
}

static void finish(user_sounds* u, user_sound_result r) {
    u->result = (uint8_t)r;
    u->result_slot = u->slot;
    u->state = IDLE;
}

static void piece(const user_sounds* u, const store_file* f, uint32_t stride, int old, uint32_t at,
                  uint8_t* p, uint32_t n) {
    static const uint8_t HEADER[USER_SOUND_HEADER] = {USER_SOUND_VERSION, LAB_SOUND_BYTES,
                                                      USER_SOUNDS};
    while (n) {
        uint32_t k = 1u;
        if (at < USER_SOUND_HEADER) {
            *p = HEADER[at];
        } else {
            uint32_t slot = (at - USER_SOUND_HEADER) / USER_SOUND_STRIDE;
            uint32_t i = (at - USER_SOUND_HEADER) % USER_SOUND_STRIDE;
            k = USER_SOUND_STRIDE - i < n ? USER_SOUND_STRIDE - i : n;
            uint32_t have = !old || i >= stride ? 0u : stride - i < k ? stride - i : k;
            if (slot == u->slot) {
                memcpy(p, &u->rec[i], k);
            } else if (!have ||
                       store_file_read(u->st, f, USER_SOUND_HEADER + slot * stride + i, p, have) !=
                           (int)have) {
                memset(p, 0, k);
            } else {
                memset(p + have, 0, k - have);
            }
        }
        at += k;
        p += k;
        n -= k;
    }
}

__attribute__((noinline, unused)) static void work(user_sounds* u) {
    store* st = u->st;
    switch (u->state) {
    case BEGIN: {
        int rc = store_file_begin(st, STORE_SOUNDS_NAME, STORE_FILE_SOUNDS, USER_SOUND_FILE_BYTES);
        if (rc == STORE_OK) {
            store_file_wait(st, 0);
            u->at = 0;
            u->errors = (uint8_t)st->errors;
            u->state = WRITE;
        } else if (rc == STORE_BUSY) {
            store_file_wait(st, store_busy(st));
        } else {
            store_file_wait(st, 0);
            finish(u, rc == STORE_FOREIGN ? USER_SOUND_FOREIGN
                      : rc == STORE_FULL  ? USER_SOUND_FULL
                                          : USER_SOUND_FAILED);
        }
        break;
    }
    case WRITE: {
        store_file f;
        uint32_t stride = USER_SOUND_STRIDE;
        int old = open_file(st, &f, &stride);
        while (u->at < USER_SOUND_FILE_BYTES) {
            uint8_t p[PIECE];
            uint32_t n = USER_SOUND_FILE_BYTES - u->at < PIECE ? USER_SOUND_FILE_BYTES - u->at
                                                              : PIECE;
            piece(u, &f, stride, old, u->at, p, n);
            uint32_t took = store_file_write(st, p, n);
            u->at = (uint16_t)(u->at + took);
            if (took < n) {
                return;
            }
        }
        (void)store_file_end(st);
        u->state = COMMIT;
        break;
    }
    case COMMIT: {
        int rc = store_file_result(st);
        if (rc == STORE_BUSY) {
            break;
        }
        finish(u, rc != STORE_OK || (uint8_t)st->errors != u->errors ? USER_SOUND_FAILED
                  : u->forcing                                       ? USER_SOUND_SAVED_GAPS
                                                                     : USER_SOUND_SAVED);
        break;
    }
    default: break;
    }
}

void user_sounds_tick(user_sounds* u, uint32_t now_ms, void (*force)(int on)) {
    if (!u->st || (u->state == IDLE && !u->forcing)) {
        return;
    }
    if (u->state != IDLE) {
        work(u);
    }
    if (u->state != IDLE) {
        if (!u->forcing && force && now_ms - u->since >= USER_SOUND_FORCE_MS) {
            u->forcing = 1;
        }
        if (u->forcing) {
            force(1);
        }
    } else if (u->forcing) {
        u->forcing = 0;
        if (force) {
            force(0);
        }
    }
}

user_sound_result user_sounds_take(user_sounds* u, uint32_t* slot) {
    user_sound_result r = (user_sound_result)u->result;
    if (slot) {
        *slot = u->result_slot;
    }
    u->result = USER_SOUND_NOTHING;
    return r;
}

uint8_t user_sound_preset_byte(const live* l) {
    return l->user_sound ? (uint8_t)(LIVE_PRESET_SLOTS + l->user_sound - 1u) : l->preset;
}

uint32_t user_sound_edit_gather(const live* l, uint8_t* out) {
    out[0] = USER_SOUND_EDIT_VERSION;
    out[1] = user_sound_preset_byte(l);
    memset(&out[3], 0, LAB_SOUND_NAME);
    for (uint32_t i = 0; i < LAB_SOUND_NAME && l->base_name[i]; i++) {
        out[3u + i] = (uint8_t)l->base_name[i];
    }
    out[3u + LAB_SOUND_NAME] = LAB_SOUND_BYTES;
    live_sound_get(l, &out[4u + LAB_SOUND_NAME]);
    out[2] = (uint8_t)live_sound_edited(l, &out[4u + LAB_SOUND_NAME]);
    return USER_SOUND_EDIT_BYTES;
}

int user_sound_edit_apply(live* l, const uint8_t* rec, uint32_t len) {
    const uint32_t at = 4u + LAB_SOUND_NAME;
    if (len < at || rec[0] < 1u || rec[at - 1u] < LAB_SOUND_BYTES_1_0 || len < at + rec[at - 1u] ||
        rec[1] != user_sound_preset_byte(l)) {
        return 0;
    }
    uint8_t s[LAB_SOUND_BYTES];
    uint32_t n = rec[at - 1u] < LAB_SOUND_BYTES ? rec[at - 1u] : LAB_SOUND_BYTES;
    memset(s, 0, sizeof s);
    memcpy(s, &rec[at], n);
    if (!lab_sound_valid(s)) {
        return 0;
    }
    live_sound_apply(l, s);
    if (!l->user_sound && l->preset == LIVE_PRESET_DICE) {
        live_sound_get(l, l->base);
    }
    return 1;
}
