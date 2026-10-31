// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "scope.h"

#include <string.h>

#define COMPILER_FENCE() __asm__ volatile("" ::: "memory")

#define MASK (SCOPE_RING - 1u)

#ifdef SIDOU_REG_RING_RACE_HOOK
static _Thread_local void (*race_hook)(void* ctx);
static _Thread_local void* race_ctx;

void scope_set_race_hook(void (*hook)(void* ctx), void* ctx) {
    race_hook = hook;
    race_ctx = ctx;
}

#define RACE_POINT()                                                                         \
    do {                                                                                     \
        if (race_hook) {                                                                     \
            race_hook(race_ctx);                                                             \
        }                                                                                    \
    } while (0)
#else
#define RACE_POINT() ((void)0)
#endif

void scope_init(scope* sc) {
    memset(sc, 0, sizeof *sc);
}

typedef struct {
    scope* sc;
    uint32_t h;
} tap_at;

static void scope_tap_point(void* ctx, const sid* chip) {
    tap_at* t = ctx;
    scope_point* p = &t->sc->point[t->h++ & MASK];
    const sid_osc* o = chip->osc;
    p->wave[0] = (uint8_t)(o[0].out >> 4);
    p->wave[1] = (uint8_t)(o[1].out >> 4);
    p->wave[2] = (uint8_t)(o[2].out >> 4);
    p->phase[0] = (uint8_t)(o[0].acc >> 16);
    p->phase[1] = (uint8_t)(o[1].acc >> 16);
    p->phase[2] = (uint8_t)(o[2].acc >> 16);
}

void scope_render(scope* sc, sid* chip, int16_t* out, uint32_t frames) {
    if (!sc->on) {
        sid_render(chip, out, frames);
        return;
    }
    tap_at t = {sc, sc->head};
    sid_render_tap(chip, out, frames, SCOPE_DECIMATE, scope_tap_point, &t);
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        const sid_env* e = &chip->env[v];
        sc->env[v] = e->env3;
        sc->state[v] = (uint8_t)e->state;
        sc->sustain[v] = e->sustain;
    }
    COMPILER_FENCE();
    sc->head = t.h;
}

void scope_peaks(scope* sc, const int16_t* lr, uint32_t frames) {
    if (!sc->on) {
        return;
    }
    volatile uint16_t* pk = sc->peak[sc->peak_slot & 1u];
    uint32_t l = pk[0];
    uint32_t r = pk[1];
    for (uint32_t i = 0; i < 2u * frames; i += 2u * SCOPE_PEAK_STRIDE) {
        int32_t a = lr[i];
        int32_t b = lr[i + 1u];
        uint32_t al = (uint32_t)(a < 0 ? -a : a);
        uint32_t ar = (uint32_t)(b < 0 ? -b : b);
        l = al > l ? al : l;
        r = ar > r ? ar : r;
    }
    pk[0] = (uint16_t)l;
    pk[1] = (uint16_t)r;
}

uint32_t scope_read(const scope* sc, uint32_t since, scope_point* out, uint32_t n) {
    uint32_t h = sc->head;
    uint32_t avail = h - since;
    if (n > SCOPE_READ_MAX) {
        n = SCOPE_READ_MAX;
    }
    if (n > avail) {
        n = avail;
    }
    uint32_t oldest = h - n;
    COMPILER_FENCE();
    uint32_t at = oldest & MASK;
    uint32_t first = n < SCOPE_RING - at ? n : SCOPE_RING - at;
    memcpy(out, &sc->point[at], first * sizeof *out);
    memcpy(out + first, &sc->point[0], (n - first) * sizeof *out);
    COMPILER_FENCE();
    return scope_intact(oldest, sc->head) ? n : 0u;
}

void scope_take_peaks(scope* sc, uint16_t out[2]) {
    uint32_t k = sc->peak_slot & 1u;
    sc->peak_slot = (uint8_t)(k ^ 1u);
    COMPILER_FENCE();
    RACE_POINT();
    out[0] = sc->peak[k][0];
    out[1] = sc->peak[k][1];
    sc->peak[k][0] = 0;
    sc->peak[k][1] = 0;
}

void scope_trace(const scope_point* p, uint32_t n, uint32_t voice, uint32_t by, uint32_t cycles,
                 uint8_t out[SCOPE_W]) {
    uint16_t sum[SCOPE_W];
    uint8_t count[SCOPE_W];
    uint8_t bin[SCOPE_W];
    cycles = cycles < 1u ? 1u : cycles > 8u ? 8u : cycles;
    uint32_t per = SCOPE_W / cycles;
    for (uint32_t b = 0; b < per; b++) {
        sum[b] = 0;
        count[b] = 0;
    }
    uint32_t first = per;
    for (uint32_t i = 0; i < n; i++) {
        uint32_t b = (uint32_t)p[i].phase[by] * per >> 8;
        if (count[b] < 255u) {
            sum[b] = (uint16_t)(sum[b] + p[i].wave[voice]);
            count[b]++;
        }
        first = b < first ? b : first;
    }
    if (first == per) {
        for (uint32_t x = 0; x < SCOPE_W; x++) {
            out[x] = 128;
        }
        return;
    }
    for (uint32_t b = 0; b < per; b++) {
        bin[b] = count[b] ? (uint8_t)((sum[b] + count[b] / 2u) / count[b]) : 0u;
    }
    uint32_t a = first;
    do {
        uint32_t d = 1;
        while (d < per && !count[(a + d) % per]) {
            d++;
        }
        uint32_t b = (a + d) % per;
        int32_t va = bin[a];
        int32_t vb = bin[b];
        for (uint32_t i = 1; i < d; i++) {
            bin[(a + i) % per] = (uint8_t)(va + (vb - va) * (int32_t)i / (int32_t)d);
        }
        a = b;
    } while (a != first);
    for (uint32_t x = 0; x < SCOPE_W; x++) {
        out[x] = bin[x % per];
    }
}

uint32_t scope_meter(uint32_t amplitude) {
    if (amplitude == 0) {
        return 0;
    }
    uint32_t msb = 0;
    while (amplitude >> (msb + 1u)) {
        msb++;
    }
    uint32_t frac = (msb >= 3u ? amplitude >> (msb - 3u) : amplitude << (3u - msb)) & 7u;
    uint32_t eighths = msb * 8u + frac;
    if (eighths <= 48u) {
        return 0;
    }
    eighths -= 48u;
    return eighths < SCOPE_METER_MAX ? eighths : SCOPE_METER_MAX;
}
