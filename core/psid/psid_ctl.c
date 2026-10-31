// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "psid_ctl.h"

#include <stddef.h>

#define COMPILER_FENCE() __asm__ volatile("" ::: "memory")

static void ring_sink(void* ctx, uint32_t cycle, uint32_t chip, uint32_t reg, uint32_t value)
{
    psid_ctl* c = (psid_ctl*)ctx;
    uint32_t i = c->stage;
    if (i - c->tail >= PSID_RING) {
        c->dropped++;
        return;
    }
    c->ring_cycle[i & (PSID_RING - 1u)] = cycle;
    c->ring_entry[i & (PSID_RING - 1u)] = PSID_RING_ENTRY(chip, reg, value);
    c->stage = i + 1u;
    c->writes++;
    if (c->lift)
        psid_lift_write(c->lift, chip, reg, value);
}

static void ask(psid_ctl* c, uint8_t want)
{
    c->want = want;
    COMPILER_FENCE();
    c->want_seq = c->want_seq + 1u;
}

static void mute_chips(psid_ctl* c, uint32_t mask)
{
    if (!c->hooks.mute)
        return;
    c->hooks.mute(c->hooks.ctx, 0, mask & 7u);
    if (c->chips == 2u)
        c->hooks.mute(c->hooks.ctx, 1, mask >> 3 & 7u);
}

void psid_ctl_init(psid_ctl* c, psid_work* w, uint8_t* mem, const psid_ctl_hooks* hooks)
{
    psid_ctl_hooks none = {NULL, NULL, NULL};
    c->head = c->tail = c->stage = 0;
    c->want = PSID_ISR_OFF;
    c->want_seq = c->done_seq = 0;
    c->chips = 1;
    c->clock[0] = c->clock[1] = 0;
    c->late = c->blocks = 0;
    c->cur[0] = c->cur[1] = 0;
    c->mode = PSID_ISR_OFF;
    c->fade_in = 0;
    c->splits_max = 0;
    c->w = w;
    c->mem = mem;
    c->h = NULL;
    c->file = NULL;
    c->hooks = hooks ? *hooks : none;
    c->lift = NULL;
    c->slices = c->writes = c->dropped = 0;
    c->error = 0;
    c->state = PSID_EMPTY;
    c->paused_from = PSID_INIT;
    c->started = 0;
    c->stop_wait = 0;
    c->next_song = 0;
    c->mute = 0;
}

static int quiet(const psid_ctl* c)
{
    return !c->stop_wait && (c->state == PSID_EMPTY || c->state == PSID_LOADED ||
                             c->state == PSID_STOPPED || c->state == PSID_FAILED);
}

static void env_load(psid_ctl* c, int over)
{
    psid_env* e = &c->w->env;
    if (over)
        psid_env_load_over(e, c->mem, c->h);
    else
        psid_env_load(e, c->mem, c->h, c->file);
    psid_env_set_sink(e, ring_sink, c);
    psid_env_set_shadows(e, &c->w->shadow[0], c->chips == 2u ? &c->w->shadow[1] : NULL);
}

static int load(psid_ctl* c, const psid_header* h, const uint8_t* file, int over)
{
    if (!quiet(c))
        return 0;
    c->h = h;
    c->file = file;
    c->chips = (uint8_t)(h->sid2 ? 2u : 1u);
    env_load(c, over);
    c->state = PSID_LOADED;
    c->error = 0;
    return 1;
}

int psid_ctl_load(psid_ctl* c, const psid_header* h, const uint8_t* file)
{
    return load(c, h, file, 0);
}

int psid_ctl_load_over(psid_ctl* c, const psid_header* h)
{
    return load(c, h, NULL, 1);
}

void psid_ctl_load_part(psid_ctl* c, uint32_t offset, const uint8_t* bytes, uint32_t n)
{
    psid_env_load_part(&c->w->env, offset, bytes, n);
}

static int begin(psid_ctl* c, uint32_t song)
{
    psid_env* e = &c->w->env;
    if (psid_env_start(e, song) != 0)
        return 0;
    for (uint32_t k = 0; k < c->chips; k++) {
        sid* s = c->hooks.chip ? c->hooks.chip(c->hooks.ctx, k) : NULL;
        if (s)
            sid_shadow_init(&c->w->shadow[k], s, 0);
    }
    c->stage = 0;
    c->head = 0;
    c->tail = 0;
    c->clock[0] = c->clock[1] = 0;
    c->late = 0;
    c->writes = c->dropped = 0;
    c->started = 0;
    c->error = 0;
    c->state = PSID_INIT;
    mute_chips(c, c->mute);
    return 1;
}

int psid_ctl_play(psid_ctl* c, uint32_t song)
{
    if (!quiet(c) || c->state == PSID_EMPTY)
        return 0;
    if (c->state != PSID_LOADED) {
        if (!c->file)
            return 0;
        env_load(c, 0);
    }
    if (song == 0u)
        song = c->h->start;
    if (song < 1u || song > c->h->songs)
        return 0;
    return begin(c, song);
}

static void halt(psid_ctl* c, uint8_t state)
{
    if (c->started)
        ask(c, PSID_ISR_STOP);
    c->stop_wait = c->started;
    c->started = 0;
    c->state = state;
    if (!c->stop_wait)
        mute_chips(c, 0);
}

int psid_ctl_stop(psid_ctl* c)
{
    if (c->state != PSID_INIT && c->state != PSID_PLAYING && c->state != PSID_PAUSED)
        return c->state != PSID_EMPTY;
    c->next_song = 0;
    halt(c, PSID_STOPPED);
    return 1;
}

int psid_ctl_pause(psid_ctl* c)
{
    if (c->state != PSID_INIT && c->state != PSID_PLAYING)
        return c->state == PSID_PAUSED;
    c->paused_from = c->state;
    c->state = PSID_PAUSED;
    if (c->started)
        ask(c, PSID_ISR_PAUSE);
    return 1;
}

int psid_ctl_resume(psid_ctl* c)
{
    if (c->state != PSID_PAUSED)
        return c->state == PSID_INIT || c->state == PSID_PLAYING;
    c->state = c->paused_from;
    if (c->started)
        ask(c, PSID_ISR_PLAY);
    return 1;
}

int psid_ctl_subtune(psid_ctl* c, uint32_t song)
{
    if (!c->file || c->state == PSID_EMPTY || c->stop_wait)
        return 0;
    if (song == 0u)
        song = c->h->start;
    if (song < 1u || song > c->h->songs)
        return 0;
    if (c->state == PSID_INIT || c->state == PSID_PLAYING || c->state == PSID_PAUSED)
        halt(c, PSID_STOPPED);
    c->next_song = (uint16_t)song;
    if (!c->stop_wait) {
        c->next_song = 0;
        return psid_ctl_play(c, song);
    }
    return 1;
}

int psid_ctl_mute(psid_ctl* c, uint32_t mask)
{
    c->mute = (uint8_t)(mask & 0x3fu);
    if (c->state == PSID_INIT || c->state == PSID_PLAYING || c->state == PSID_PAUSED)
        mute_chips(c, c->mute);
    return 1;
}

uint32_t psid_ctl_state(const psid_ctl* c)
{
    return c->state;
}

uint32_t psid_ctl_ahead(const psid_ctl* c)
{
    int32_t d = (int32_t)(psid_env_now(&c->w->env) - c->clock[0]);
    return d > 0 ? (uint32_t)d : 0u;
}

static uint32_t noisy(const psid_ctl* c)
{
    uint32_t n = 0;
    for (uint32_t k = 0; k < c->chips; k++)
        for (uint32_t v = 0; v < 3u; v++) {
            const sid_osc* o = &c->w->shadow[k].osc[v];
            n += o->slow && (o->control >> 4) > 8u;
        }
    return n;
}

static int ring_full(const psid_ctl* c)
{
    return PSID_RING - (c->stage - c->tail) < PSID_SLICE_ROOM;
}

static void maybe_start(psid_ctl* c)
{
    const psid_env* e = &c->w->env;
    if (!c->started && c->state != PSID_PAUSED &&
        (e->phase != PSID_PHASE_INIT || psid_env_now(e) >= psid_env_period(e) || ring_full(c))) {
        c->started = 1;
        ask(c, PSID_ISR_PLAY);
    }
}

int psid_ctl_poll(psid_ctl* c)
{
    psid_env* e = &c->w->env;
    if (c->stop_wait) {
        if (c->done_seq != c->want_seq)
            return 0;
        c->stop_wait = 0;
        mute_chips(c, 0);
        if (c->next_song) {
            uint32_t song = c->next_song;
            c->next_song = 0;
            (void)psid_ctl_play(c, song);
        }
    }
    if (c->state != PSID_INIT && c->state != PSID_PLAYING && c->state != PSID_PAUSED)
        return 0;
    maybe_start(c);
    if (psid_ctl_ahead(c) >= PSID_AHEAD_CYCLES || ring_full(c))
        return 0;
    uint32_t ran = 0, work = 0, accesses = e->sid_accesses, nv = noisy(c);
    int r;
    do {
        uint32_t t = psid_env_now(e), left = PSID_SLICE_CYCLES - ran, run = PSID_SUB_CYCLES;
        if (e->phase == PSID_PHASE_WAIT && !nv && (int32_t)(e->next_due - t) > (int32_t)run)
            run = e->next_due - t;
        r = psid_env_run(e, left < run ? left : run);
        if (r == PSID_RUN_CALL_DONE && c->lift)
            psid_lift_call(c->lift);
        uint32_t now = psid_env_now(e), d = now - t;
        nv = noisy(c);
        if (nv)
            for (uint32_t k = 0; k < c->chips; k++)
                sid_shadow_clock(&c->w->shadow[k], now);
        ran += d;
        work += d * (PSID_WORK_CYCLE + nv * PSID_WORK_NOISY) +
                (e->sid_accesses - accesses) * PSID_WORK_ACCESS;
        accesses = e->sid_accesses;
    } while (r != PSID_RUN_INIT_DONE && r < PSID_RUN_INIT_TOO_LONG && ran < PSID_SLICE_CYCLES &&
             work < PSID_SLICE_WORK);
    uint32_t now = psid_env_now(e);
    for (uint32_t k = 0; k < c->chips; k++)
        sid_shadow_clock(&c->w->shadow[k], now);
    COMPILER_FENCE();
    c->head = c->stage;
    c->slices++;
    e->late = c->late;
    if (r == PSID_RUN_INIT_DONE) {
        if (c->state == PSID_PAUSED)
            c->paused_from = PSID_PLAYING;
        else
            c->state = PSID_PLAYING;
    } else if (r >= PSID_RUN_INIT_TOO_LONG) {
        c->next_song = 0;
        halt(c, PSID_FAILED);
        c->error = (int16_t)r;
        return 1;
    }
    maybe_start(c);
    return 1;
}
