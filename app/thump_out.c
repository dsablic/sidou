// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "thump_out.h"

void thump_out_take(thump_out* t, uint32_t chip, sid* s) {
    t->level[chip] += s->thump_pending * s->output_gain;
    t->late[chip] += s->thump_late * s->output_gain;
    t->late_at[chip] = s->thump_late_at;
    t->r[chip] = s->dc_r;
    s->thump_pending = s->thump_late = 0.0f;
    s->thump_rendered = s->thump_late_at = 0u;
}

void thump_out_drop(sid* s) {
    s->thump_pending = s->thump_late = 0.0f;
    s->thump_rendered = s->thump_late_at = 0u;
}

static int16_t thump_add(int32_t s, float x) {
    int up = x >= 0.0f;
    float m = up ? x : -x;
    float h = (float)(up ? THUMP_OUT_CEILING - s : THUMP_OUT_CEILING + s) * 0.5f;
    if (m > h) {
        if (h <= 0.0f) {
            return (int16_t)s;
        }
        float d = m - h;
        m = h + d * h / (d + h);
    }
    float o = (float)s + (up ? m : -m);
    o += o >= 0.0f ? 0.5f : -0.5f;
    return (int16_t)o;
}

__attribute__((noinline, unused)) static void thump_run(thump_out* t, int16_t* lr, uint32_t from, uint32_t to) {
    float a = t->level[0], b = t->level[1];
    const float ra = t->r[0], rb = t->r[1];
    for (uint32_t f = from; f < to; f++) {
        float x = a + b;
        a *= ra;
        b *= rb;
        lr[2u * f] = thump_add(lr[2u * f], x);
        lr[2u * f + 1u] = thump_add(lr[2u * f + 1u], x);
    }
    t->level[0] = a;
    t->level[1] = b;
}

void thump_out_apply(thump_out* t, int16_t* lr, uint32_t frames) {
    if (t->level[0] == 0.0f && t->level[1] == 0.0f && t->late[0] == 0.0f &&
        t->late[1] == 0.0f) {
        return;
    }
    uint32_t f = 0;
    for (;;) {
        int c = -1;
        for (int k = 0; k < 2; k++) {
            if (t->late[k] != 0.0f && (c < 0 || t->late_at[k] < t->late_at[c])) {
                c = k;
            }
        }
        if (c < 0) {
            break;
        }
        uint32_t at = t->late_at[c] < frames ? t->late_at[c] : frames;
        if (at < f) {
            at = f;
        }
        thump_run(t, lr, f, at);
        t->level[c] += t->late[c];
        t->late[c] = 0.0f;
        f = at;
    }
    thump_run(t, lr, f, frames);
    float a = t->level[0], b = t->level[1];
    if ((a < 0.0f ? -a : a) + (b < 0.0f ? -b : b) < 0.5f) {
        t->level[0] = t->level[1] = 0.0f;
    }
}
