// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "reg_ring.h"

#include "fx_live.h"
#include "live.h"

#include <stddef.h>

#define COMPILER_FENCE() __asm__ volatile("" ::: "memory")

#ifdef SIDOU_REG_RING_RACE_HOOK
static _Thread_local void (*race_hook)(void* ctx);
static _Thread_local void* race_ctx;

void reg_ring_set_race_hook(void (*hook)(void* ctx), void* ctx) {
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

void reg_ring_init(reg_ring* r) {
    r->head = 0;
    r->tail = 0;
    r->stage = 0;
}

int reg_ring_push(reg_ring* r, uint8_t reg, uint8_t value) {
    if (r->stage - r->tail >= REG_RING_SIZE) {
        return 0;
    }
    reg_ring_entry* e = &r->e[r->stage & (REG_RING_SIZE - 1u)];
    e->reg = reg;
    e->value = value;
    r->stage++;
    return 1;
}

void reg_ring_commit(reg_ring* r) {
    COMPILER_FENCE();
    r->head = r->stage;
}

uint32_t reg_ring_used(const reg_ring* r) {
    return r->stage - r->tail;
}

int reg_ring_pop(reg_ring* r, reg_ring_entry* out) {
    uint32_t t = r->tail;
    if (t == r->head) {
        return 0;
    }
    COMPILER_FENCE();
    *out = r->e[t & (REG_RING_SIZE - 1u)];
    COMPILER_FENCE();
    r->tail = t + 1u;
    return 1;
}

_Static_assert(0x18u < REG_RING_CHIP_B && (0x18u | REG_RING_CHIP_B) < 0x80u,
               "chip B's SID registers, 0x40..0x58, are below the pseudo-registers");
_Static_assert(LIVE_REG_RESET == 0x80u && LIVE_REG_GATE_RESET == 0x81u &&
                   LIVE_REG_CHAR_TICK == 0x87u && REG_RING_CHARACTER == 0x88u,
               "chip A's pseudo-registers are 0x80..0x88");
_Static_assert(REG_RING_MUTE == 0x89u && REG_RING_MUTE < LIVE_REG_EXPERT_FIRST &&
                   !REG_RING_IS_SLOT(REG_RING_MUTE | REG_RING_CHIP_B),
               "the mute, 0x89 (chip B's 0xC9), is apart from the expert's and the slots'");
_Static_assert(!REG_RING_IS_SLOT(REG_RING_CHARACTER) &&
                   !REG_RING_IS_SLOT(REG_RING_CHARACTER | REG_RING_CHIP_B) &&
                   !REG_RING_IS_SLOT(LIVE_REG_RESET | REG_RING_CHIP_B),
               "chip B's pseudo-registers, 0xC0..0xC8, are not the slots'");
_Static_assert(REG_RING_IS_SLOT(LIVE_REG_SLOT(0, 0)) &&
                   REG_RING_IS_SLOT(LIVE_REG_SLOT(LIVE_SLOTS - 1u, LIVE_SLOT_FIELDS - 1u)) &&
                   !(LIVE_REG_SLOT(LIVE_SLOTS - 1u, LIVE_SLOT_FIELDS - 1u) & REG_RING_CHIP_B),
               "the slots' pseudo-registers are 0xA0..0xBF and name no chip");
_Static_assert(REG_RING_CHIP_B == 1u << 6 && REG_RING_CHIP_OF(REG_RING_CHIP_B) == 1u,
               "a chip's bit is its number << 6 (reg_ring_sink)");
_Static_assert(REG_RING_SIDS == 3u, "the spare is the sid neither chip renders: 3 - a - b");
_Static_assert(!REG_RING_IS_SLOT(LIVE_REG_CHAR(0)) && !REG_RING_IS_SLOT(LIVE_REG_CHAR(LIVE_CHAR_FIELDS - 1u)) &&
                   !REG_RING_IS_SLOT(LIVE_REG_CHAR_TICK) && !REG_RING_IS_SLOT(LIVE_REG_RESET) &&
                   LIVE_REG_CHAR(LIVE_CHAR_FIELDS) <= 0xa0u,
               "reg_ring_sink takes a slot register before the character's, the tick and the reset");
_Static_assert(LIVE_REG_EXPERT(SID_EXPERT_COUNT) <= 0xa0u &&
                   !REG_RING_IS_SLOT(LIVE_REG_EXPERT_FIRST) &&
                   !REG_RING_IS_SLOT(LIVE_REG_EXPERT_FIRST | REG_RING_CHIP_B),
               "expert settings stay outside SID, character and shared-slot registers");

void reg_ring_chips_init(reg_ring_chips* c, sid_model model, uint32_t clock_hz,
                         uint32_t sample_rate) {
    c->clock_hz = clock_hz;
    c->sample_rate = sample_rate;
    c->spare_ready = 0;
    c->spare_model = 0;
    c->spare_for = 0;
    c->lost_resets = 0;
    c->pseudo = 0;
    c->pseudo_ctx = 0;
    c->swapped = 0;
    c->swapped_ctx = 0;
    sid_init(&c->chip[0], model, clock_hz, sample_rate);
    sid_character first;
    sid_character_init(&first);
    for (uint32_t x = 0; x < REG_RING_CHIPS; ++x) {
        c->active[x] = (uint8_t)x;
        c->next[x] = (uint8_t)x;
        sid_character_ramp_reset(&c->ramp[x], &first, NULL);
        c->stage_pending[x][0] = c->stage_pending[x][1] = 0;
        c->ticks[x] = 0;
    }
    c->prep_on = 0;
    c->prep_profile = 0;
    c->prep_chip = 0;
    c->prep_pass = 0;
    c->build_pass = 0;
    c->keys = 0;
    c->prep_wait[0] = c->prep_wait[1] = 0;
    c->prep_age[0] = c->prep_age[1] = 0;
    c->default_8580_spent = 0;
    c->filter_leases = 0;
}

sid* reg_ring_chip(reg_ring_chips* c) {
    return &c->chip[c->active[0]];
}

sid* reg_ring_chip_of(reg_ring_chips* c, uint32_t chip) {
    return &c->chip[c->active[chip & 1u]];
}

static uint8_t third(const volatile uint8_t* pair) {
    return (uint8_t)(3u - pair[0] - pair[1]);
}

static void apply(reg_ring_chips* c, reg_ring_entry e) {
    uint8_t reg = e.reg;
    if (!(reg & 0x80u)) {
        sid_write(&c->chip[c->active[REG_RING_CHIP_OF(reg)]], REG_RING_BARE(reg), e.value);
        return;
    }
    if (REG_RING_IS_SLOT(reg)) {
        if (c->pseudo) {
            c->pseudo(c->pseudo_ctx, reg, e.value);
        }
        return;
    }
    uint32_t x = REG_RING_CHIP_OF(reg);
    uint8_t bare = REG_RING_BARE(reg);
    sid* s = &c->chip[c->active[x]];
    if (bare == LIVE_REG_GATE_RESET) {
        sid_set_gate_reset(s, e.value);
    } else if (bare == REG_RING_CHARACTER) {
        if (e.value < 2u) {
            sid_character_commit_later(s, &c->stage[x][e.value]);
            COMPILER_FENCE();
            c->stage_pending[x][e.value] = 0;
        }
        c->ticks[x]++;
    } else if (bare == REG_RING_MUTE) {
        sid_set_mute(s, e.value);
    } else if (bare == LIVE_REG_RESET) {
        if (c->spare_ready && c->spare_for == x && c->spare_model == e.value) {
            c->active[x] = (uint8_t)(3u - c->active[0] - c->active[1]);
            c->spare_ready = 0;
            sid_set_mute(&c->chip[c->active[x]], s->mute);
        } else {
            c->lost_resets++;
        }
    } else if (!x && c->pseudo) {
        c->pseudo(c->pseudo_ctx, reg, e.value);
    }
}

uint32_t reg_ring_drain(reg_ring* r, reg_ring_chips* c) {
    reg_ring_entry e;
    uint32_t n = 0;
    uint8_t was[REG_RING_CHIPS];
    for (uint32_t x = 0; x < REG_RING_CHIPS; ++x) {
        was[x] = c->active[x];
    }
    while (reg_ring_pop(r, &e)) {
        apply(c, e);
        n++;
    }
    if (c->swapped) {
        for (uint32_t x = 0; x < REG_RING_CHIPS; ++x) {
            if (c->active[x] != was[x]) {
                c->swapped(c->swapped_ctx, x, &c->chip[c->active[x]]);
            }
        }
    }
    return n;
}

static sid_dac* free_dac(reg_ring_chips* c) {
    const sid_dac* taken[2u * REG_RING_CHIPS + 3u * REG_RING_CHIPS + 1u];
    uint32_t n = 0;
    for (uint32_t x = 0; x < REG_RING_CHIPS; ++x) {
        for (uint32_t k = 0; k < 2u; ++k) {
            if (c->stage_pending[x][k]) {
                taken[n++] = c->stage[x][k].dac;
            }
        }
    }
    COMPILER_FENCE();
    RACE_POINT();
    for (uint32_t x = 0; x < REG_RING_CHIPS; ++x) {
        uint8_t active = c->active[x];
        uint8_t next = c->next[x];
        taken[n++] = c->chip[next].dac;
        if (active != next) {
            taken[n++] = c->chip[active].dac;
        }
        if (c->ramp[x].building) {
            taken[n++] = c->ramp[x].job.d;
        }
    }
    if (c->prep_on && c->prep.job.d) {
        taken[n++] = c->prep.job.d;
    }
    for (uint32_t i = 0; i < REG_RING_SIDS; ++i) {
        const sid_dac* d = &c->dac[i];
        uint32_t k = 0;
        while (k < n && taken[k] != d) {
            k++;
        }
        if (k == n) {
            return &c->dac[i];
        }
    }
    return NULL;
}

__attribute__((noinline, unused)) static int free_filter(reg_ring_chips* c, sid_filter_row rows[2]) {
    const float* taken[24];
    uint32_t n = 0;
    for (uint32_t x = 0; x < REG_RING_CHIPS; x++) {
        for (uint32_t k = 0; k < 2u; k++) {
            if (c->stage_pending[x][k]) {
                taken[n++] = c->stage[x][k].filter_rows[0].cutoff;
                taken[n++] = c->stage[x][k].filter_rows[1].cutoff;
            }
        }
    }
    COMPILER_FENCE();
    RACE_POINT();
    for (uint32_t x = 0; x < REG_RING_CHIPS; x++) {
        uint8_t active = c->active[x], next = c->next[x];
        taken[n++] = c->chip[next].filter_rows[0].cutoff;
        taken[n++] = c->chip[next].filter_rows[1].cutoff;
        if (active != next) {
            taken[n++] = c->chip[active].filter_rows[0].cutoff;
            taken[n++] = c->chip[active].filter_rows[1].cutoff;
        }
        if (c->ramp[x].filter_building) {
            taken[n++] = c->ramp[x].filter_job.rows[0].cutoff;
            taken[n++] = c->ramp[x].filter_job.rows[1].cutoff;
        }
    }
    if (c->prep_on) {
        taken[n++] = c->prep.rows[0].cutoff;
        taken[n++] = c->prep.rows[1].cutoff;
    }
    uint32_t count = 0;
    for (uint32_t i = 0; i < 2u * REG_RING_SIDS; i++) {
        sid_filter_row row = sid_filter_backing(&c->chip[i / 2u], i & 1u);
        uint32_t k = 0;
        while (k < n && taken[k] != row.cutoff) {
            k++;
        }
        if (k == n) {
            rows[count++] = row;
            if (count == 2u) {
                return 1;
            }
        }
    }
    return 0;
}

void reg_ring_power_on(sid* s) {
    for (int v = 0; v < 3; ++v) {
        sid_env_power_on_done(&s->env[v]);
    }
}

static void prepare_spare(reg_ring_chips* c, uint32_t x, uint8_t model) {
    sid_model m = model == SID_MODEL_8580 ? SID_MODEL_8580 : SID_MODEL_6581;
    uint8_t spare = third(c->active);
    c->prep_on = 0;
    sid* s = &c->chip[spare];
    c->ramp[x].building = 0;
    c->ramp[x].filter_building = 0;
    const sid_dac* table;
    if (!c->filter_leases) {
        sid_init(s, m, c->clock_hz, c->sample_rate);
        table = sid_character_apply(s, &c->ramp[x].target, free_dac(c));
    } else {
        sid_filter_row rows[2];
        if (!free_filter(c, rows)) {
            return;
        }
        sid_character_prep_start(&c->prep, s, m, c->clock_hz, c->sample_rate, &c->ramp[x].target, 0);
        sid_character_prep_rows(&c->prep, rows);
        while (!sid_character_prep_step(&c->prep, free_dac(c))) {
        }
        table = c->prep.table;
    }
    sid_character_ramp_reset(&c->ramp[x], &c->ramp[x].target, table);
    if (x) {
        reg_ring_power_on(s);
    }
    c->next[x] = spare;
    c->spare_model = (uint8_t)m;
    c->spare_for = (uint8_t)x;
    COMPILER_FENCE();
    c->spare_ready = 1;
}

static int take_prepared(reg_ring_chips* c, uint32_t x, uint8_t model) {
    sid_character_ramp* ramp = &c->ramp[x];
    if (!c->prep_on || c->prep_chip != x || !sid_character_prep_done(&c->prep) ||
        c->prep.model != model || c->prep.c.profile != ramp->target.profile) {
        return 0;
    }
    sid_character want = ramp->target;
    sid_character_ramp_reset(ramp, &c->prep.c, c->prep.table);
    ramp->target = want;
    if (x) {
        reg_ring_power_on(c->prep.s);
    }
    c->next[x] = third(c->next);
    c->spare_model = model;
    c->spare_for = (uint8_t)x;
    c->prep_on = 0;
    COMPILER_FENCE();
    c->spare_ready = 1;
    return 1;
}

static void wait_tick(reg_ring_chips* c, uint32_t x) {
    c->prep_wait[x] = 1;
    if (c->prep_age[x] < 255u) {
        c->prep_age[x]++;
    }
}

int reg_ring_prepare_step(void* ctx, uint8_t profile) {
    reg_ring_out* o = ctx;
    reg_ring_chips* c = o->chips;
    uint32_t x = o->chip & 1u;
    const sid_chip_profile* p = sid_chip_profile_get(profile);
    if (!p) {
        c->prep_wait[x] = 0;
        c->prep_age[x] = 0;
        if (c->prep_on && c->prep_chip == x) {
            c->prep_on = 0;
        }
        return 0;
    }
    int keys = x == c->keys;
    if (c->prep_on && c->prep_chip != x) {
        if (!keys) {
            wait_tick(c, x);
            return 0;
        }
        c->prep_on = 0;
        c->prep_wait[x ^ 1u] = 1;
    }
    int same = c->prep_on && c->prep_profile == profile;
    if (same && sid_character_prep_done(&c->prep)) {
        return 1;
    }
    if (c->prep_pass || c->build_pass) {
        if (!keys) {
            wait_tick(c, x);
        }
        return 0;
    }
    if (!same) {
        if (c->spare_ready || (!keys && (c->ramp[x ^ 1u].building || c->ramp[x ^ 1u].filter_building))) {
            c->prep_on = 0;
            wait_tick(c, x);
            return 0;
        }
        c->ramp[x ^ 1u].building = 0;
        c->ramp[x ^ 1u].filter_building = 0;
        sid_character want = c->ramp[x].target;
        want.profile = (uint8_t)(profile + 1u);
        c->ramp[x].building = 0;
        c->ramp[x].filter_building = 0;
        c->prep_on = 0;
        sid_filter_row rows[2];
        if (c->filter_leases && !free_filter(c, rows)) {
            wait_tick(c, x);
            return 0;
        }
        uint32_t defaults = p->model == SID_MODEL_8580 && !c->default_8580_spent
                                ? SID_DAC_BUILD_STEPS
                                : 0u;
        sid_character_prep_start(&c->prep, &c->chip[third(c->next)], (sid_model)p->model,
                                 c->clock_hz, c->sample_rate, &want, defaults);
        if (c->filter_leases) {
            sid_character_prep_rows(&c->prep, rows);
        }
        c->prep_on = 1;
        c->prep_profile = profile;
        c->prep_chip = (uint8_t)x;
        c->prep_wait[x] = 0;
    } else {
        sid_character_prep_update(&c->prep, &c->ramp[x].target);
    }
    o->slices++;
    o->sliced = 1;
    c->prep_pass = 1;
    int done = sid_character_prep_step(&c->prep, free_dac(c));
    if (done) {
        c->prep_age[x] = 0;
    }
    if (done && p->model == SID_MODEL_8580) {
        c->default_8580_spent = 1;
    }
    return done;
}

void reg_ring_pass_end(reg_ring_chips* c) {
    c->prep_pass = 0;
    c->build_pass = 0;
}

void reg_ring_set_keys(reg_ring_chips* c, uint32_t chip) {
    c->keys = (uint8_t)(chip & 1u);
}

static uint8_t character_tick(reg_ring_out* o) {
    reg_ring_chips* c = o->chips;
    uint32_t x = o->chip & 1u;
    uint32_t y = x ^ 1u;
    int keys = x == c->keys;
    sid_character_ramp* ramp = &c->ramp[x];
    if ((ramp->building || ramp->filter_building) && (c->prep_pass || c->build_pass)) {
        return REG_RING_NO_STAGE;
    }
    const volatile uint8_t* pending = c->stage_pending[x];
    uint32_t k = !pending[0] ? 0u : !pending[1] ? 1u : 2u;
    if (k == 2u) {
        o->character_defers++;
        return REG_RING_NO_STAGE;
    }
    int yield = keys && c->prep_age[y] >= REG_RING_YIELD_TICKS;
    int busy = keys ? (c->prep_on && c->prep_chip == x) || c->prep_pass || c->build_pass || yield
                    : c->prep_on || c->prep_pass || c->build_pass || c->ramp[y].building ||
                          c->ramp[y].filter_building ||
                          c->prep_wait[y];
    sid_dac* table = busy ? NULL : free_dac(c);
    if (keys && !busy && !table && sid_character_ramp_wants_table(ramp)) {
        if (c->ramp[y].building) {
            c->ramp[y].building = 0;
        } else if (c->prep_on && c->prep_chip == y && c->prep.job.d) {
            c->prep_on = 0;
            c->prep_wait[y] = 1;
        }
        table = free_dac(c);
    }
    uint8_t was = ramp->building;
    uint8_t filter_was = ramp->filter_building;
    sid_filter_row rows[2];
    uint32_t filter_mask = (1u << SID_EXPERT_FILTER_6581_CURVE) |
                           (1u << SID_EXPERT_FILTER_6581_RANGE) |
                           (1u << SID_EXPERT_FILTER_8580_CURVE);
    int filter_free = !busy && !ramp->filter_building &&
                      (ramp->filter_restore ||
                       ((ramp->target.expert_mask | ramp->filter_now.expert_mask) & filter_mask)) &&
                      free_filter(c, rows);
    if (filter_free) {
        c->filter_leases = 1;
    }
    int changed = sid_character_ramp_tick_tables(ramp, &c->chip[c->next[x]], table,
                                                filter_free ? rows : NULL, &c->stage[x][k]);
    if (was || ramp->building || filter_was || ramp->filter_building) {
        c->build_pass = 1;
        o->sliced = 1;
    }
    if (!changed) {
        return REG_RING_NO_STAGE;
    }
    c->stage_pending[x][k] = 1;
    return (uint8_t)k;
}

static void wait_empty(reg_ring_out* o, uint32_t* waits) {
    reg_ring_commit(o->ring);
    while (reg_ring_used(o->ring)) {
        (*waits)++;
        o->wait(o->wait_ctx);
    }
}

static void push_waiting(reg_ring_out* o, uint8_t reg, uint8_t value) {
    reg_ring* r = o->ring;
    while (!reg_ring_push(r, reg, value)) {
        reg_ring_commit(r);
        o->full_waits++;
        o->wait(o->wait_ctx);
    }
}

void reg_ring_sink(void* ctx, uint8_t reg, uint8_t value) {
    reg_ring_out* o = ctx;
    uint32_t x = o->chip & 1u;
    if (!(reg & 0x80u)) {
        push_waiting(o, (uint8_t)(reg | x << 6), value);
        return;
    }
    if (REG_RING_IS_SLOT(reg)) {
        push_waiting(o, reg, value);
        return;
    }
    if ((reg >= LIVE_REG_CHAR(0) && reg < LIVE_REG_CHAR(LIVE_CHAR_FIELDS)) ||
        (reg >= LIVE_REG_EXPERT_FIRST && reg < LIVE_REG_EXPERT(SID_EXPERT_COUNT))) {
        live_char_store(&o->chips->ramp[x].target, reg, value);
        return;
    }
    if (reg == LIVE_REG_CHAR_TICK) {
        value = character_tick(o);
        reg = REG_RING_CHARACTER;
    }
    if (reg == LIVE_REG_RESET) {
        uint8_t model = value == SID_MODEL_8580 ? SID_MODEL_8580 : SID_MODEL_6581;
        if (!take_prepared(o->chips, x, model)) {
            o->unprepared_resets++;
            wait_empty(o, &o->reset_waits);
            prepare_spare(o->chips, x, model);
        }
        value = o->chips->spare_model;
    }
    if (!REG_RING_IS_SLOT(reg) && x) {
        reg |= REG_RING_CHIP_B;
    }
    push_waiting(o, reg, value);
}
