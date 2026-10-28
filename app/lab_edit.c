// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_edit.h"

void lab_edit_clear(lab_edit* e) {
    e->first = 0;
    e->n = 0;
    e->steps = 0;
}

void lab_edit_get(const live* l, uint8_t out[LAB_EDIT_BYTES]) {
    live_sound_get(l, out);
    out[LAB_EDIT_MODE] = l->envelope_mode;
}

/* Entry k (0 the oldest) in the ring. */
static uint32_t slot(const lab_edit* e, uint32_t k) {
    uint32_t i = e->first + k;
    return i < LAB_EDIT_ENTRIES ? i : i - LAB_EDIT_ENTRIES;
}

/* The oldest step dropped, all its entries. */
static void drop_oldest(lab_edit* e) {
    do {
        e->first = (uint8_t)slot(e, 1);
        e->n--;
    } while (e->n && !(e->at[e->first] & LAB_EDIT_FIRST));
    e->steps--;
}

/* 1 when the newest step holds place p. */
static int newest_holds(const lab_edit* e, uint32_t p) {
    for (uint32_t k = e->n; k--;) {
        uint32_t at = e->at[slot(e, k)];
        if ((at & ~LAB_EDIT_FIRST) == p) {
            return 1;
        }
        if (at & LAB_EDIT_FIRST) {
            return 0;
        }
    }
    return 0;
}

uint32_t lab_edit_push(lab_edit* e, const uint8_t before[LAB_EDIT_BYTES],
                       const uint8_t after[LAB_EDIT_BYTES], int merge) {
    int started = merge && e->steps;
    for (uint32_t p = 0; p < LAB_EDIT_BYTES; p++) {
        if (before[p] == after[p] || (started && merge && newest_holds(e, p))) {
            continue;
        }
        /* Room: the oldest steps go, never the one being made (it holds at most
         * LAB_EDIT_BYTES entries, and the ring has more). */
        while (e->n == LAB_EDIT_ENTRIES) {
            drop_oldest(e);
        }
        uint32_t i = slot(e, e->n++);
        e->at[i] = (uint8_t)(p | (started ? 0u : LAB_EDIT_FIRST));
        e->old[i] = before[p];
        if (!started) {
            started = 1;
            merge = 0; /* a new step: nothing in it to look for */
            if (++e->steps > LAB_EDIT_STEPS) {
                drop_oldest(e);
            }
        }
    }
    return (uint32_t)started;
}

uint32_t lab_edit_undo(lab_edit* e, live* l) {
    if (!e->steps) {
        return 0;
    }
    uint8_t s[LAB_EDIT_BYTES];
    lab_edit_get(l, s);
    uint32_t at;
    do {
        uint32_t i = slot(e, --e->n);
        at = e->at[i];
        s[at & ~LAB_EDIT_FIRST] = e->old[i];
    } while (!(at & LAB_EDIT_FIRST));
    e->steps--;
    live_sound_apply(l, s);
    live_param_set(l, LIVE_PARAM_ENVELOPE_MODE, s[LAB_EDIT_MODE]);
    return 1;
}
