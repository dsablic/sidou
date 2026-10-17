// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "user_profile.h"

#include <stddef.h>

#include "sid/sid_character.h"

_Static_assert(USER_PROFILE_V1_BYTES == 3u + (LIVE_CHAR_FIELDS - 1u),
               "version, base, count, then the controls");
_Static_assert(LIVE_USER_PROFILES <= 8u, "the queue is one byte of bits");

void user_profiles_init(user_profiles* u, live* l, store* st) {
    u->l = l;
    u->st = st;
    u->queued = 0;
    u->sent = 0;
    u->result = USER_PROFILE_NOTHING;
    u->result_slot = 0;
    u->forcing = 0;
    u->since = 0;
}

/* A stored record as slot n of l: 1 if it is a profile this firmware reads. A later schema's
 * extra bytes are ignored. */
static int take(live* l, uint32_t n, const uint8_t* rec, uint32_t len) {
    uint32_t count = len > 2u ? rec[2] : 0u;
    if (len < 3u || rec[0] < USER_PROFILE_VERSION || count < LIVE_CHAR_FIELDS - 1u ||
        len < 3u + count || rec[1] >= SID_CHIPS) {
        return 0;
    }
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; ++f) {
        if (rec[3u + f] > SID_CHARACTER_MAX) {
            return 0;
        }
    }
    l->user_base[n] = rec[1];
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; ++f) {
        l->user_ctl[n][f] = rec[3u + f];
    }
    l->user_valid = (uint8_t)(l->user_valid | 1u << n);
    return 1;
}

void user_profiles_load(user_profiles* u) {
    live* l = u->l;
    l->user_valid = 0;
    if (!u->st) {
        return;
    }
    for (uint32_t n = 0; n < LIVE_USER_PROFILES; ++n) {
        uint8_t rec[USER_PROFILE_V1_BYTES + 16u];
        int len = store_get(u->st, STORE_KEY_PROFILE(n), rec, sizeof rec, NULL);
        if (len > 0) {
            (void)take(l, n, rec, (uint32_t)len > sizeof rec ? (uint32_t)sizeof rec : (uint32_t)len);
        }
    }
}

void user_profiles_forget(user_profiles* u) {
    u->l->user_valid = 0;
    u->l->user_applied = 0;
    u->queued = 0;
    u->sent = 0;
}

int user_profile_save(user_profiles* u, uint32_t slot, uint32_t profile,
                      const uint8_t controls[LIVE_CHAR_FIELDS - 1u], uint32_t now_ms) {
    live* l = u->l;
    l->user_base[slot] = (uint8_t)profile;
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; ++f) {
        l->user_ctl[slot][f] = controls[f];
    }
    l->user_valid = (uint8_t)(l->user_valid | 1u << slot);
    if (!u->st) {
        return 1;
    }
    if (!u->queued && !u->sent) {
        u->since = now_ms;
    }
    u->queued = (uint8_t)(u->queued | 1u << slot);
    return 0;
}

int user_profiles_busy(const user_profiles* u) {
    return u->queued || u->sent || u->result;
}

/* The lowest queued slot's record, from live's cache, to the store: taken, or never taken
 * (a result), or asked again next tick. */
__attribute__((noinline, unused)) static void submit(user_profiles* u) {
    const live* l = u->l;
    uint32_t n = 0;
    while (!(u->queued >> n & 1u)) {
        n++;
    }
    u->rec[0] = USER_PROFILE_VERSION;
    u->rec[1] = l->user_base[n];
    u->rec[2] = LIVE_CHAR_FIELDS - 1u;
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; ++f) {
        u->rec[3u + f] = l->user_ctl[n][f];
    }
    int rc = store_put(u->st, STORE_KEY_PROFILE(n), USER_PROFILE_TYPE, u->rec, sizeof u->rec);
    if (rc == STORE_OK) {
        u->queued = (uint8_t)(u->queued & ~(1u << n));
        u->sent = (uint8_t)(n + 1u);
    } else if (rc == STORE_FOREIGN || rc == STORE_FULL || rc == STORE_INVALID) {
        u->queued = (uint8_t)(u->queued & ~(1u << n));
        u->result = USER_PROFILE_FOREIGN;
        u->result_slot = (uint8_t)n;
    } /* STORE_BUSY: asked again next tick */
}

/* One tick of work: submit, commit, force. */
__attribute__((noinline, unused)) static void work(user_profiles* u, uint32_t now_ms,
                                                   void (*force)(int on)) {
    if (!u->sent && u->queued && !u->result && !store_busy(u->st)) {
        submit(u);
    }
    if (u->sent && !store_busy(u->st)) {
        u->result = u->forcing ? USER_PROFILE_SAVED_GAPS : USER_PROFILE_SAVED;
        u->result_slot = (uint8_t)(u->sent - 1u);
        u->sent = 0;
        u->since = now_ms; /* the next slot's wait starts when this one ended */
    }
    if (u->queued | u->sent) {
        if (!u->forcing && force && now_ms - u->since >= USER_PROFILE_FORCE_MS) {
            u->forcing = 1;
        }
        if (u->forcing) {
            force(1); /* idempotent: the settings' save may have released it meanwhile */
        }
    } else if (u->forcing) {
        u->forcing = 0;
        if (force) {
            force(0);
        }
    }
}

void user_profiles_tick(user_profiles* u, uint32_t now_ms, void (*force)(int on)) {
    if (u->st && (u->queued | u->sent | u->forcing)) {
        work(u, now_ms, force);
    }
}

user_profile_result user_profiles_take(user_profiles* u, uint32_t* slot) {
    user_profile_result r = (user_profile_result)u->result;
    if (slot) {
        *slot = u->result_slot;
    }
    u->result = USER_PROFILE_NOTHING;
    return r;
}
