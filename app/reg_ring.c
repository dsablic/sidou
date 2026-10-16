// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "reg_ring.h"

#include "fx_live.h"
#include "live.h"

#include <stddef.h>

/* REG_RING_WORST_PASS, the most one main-loop pass can push: the instrument's on one chip,
 * REG_RING_LIVE_PASS (live.c's writes per call), the other chip's, REG_RING_SONG_PASS, and
 * the keys' move, REG_RING_MOVE_PASS (below). The instrument's:
 *   a model change, a reset, the gate reset and every register
 *                                               1 + 1 + 25
 *   (live_tick taking a spare prepared for the profile or model asked for, or, with no
 *   preparer, the FX button: one per pass at most. MIDI and the FX button only record the
 *   request; a tick applies one only if it differs from the chip's, and once one reset is
 *   queued no other spare can be ready in the pass: there is one, and a preparation never
 *   starts while a swap is queued.)
 *   ENV, the selected effect slot's next algorithm  1
 *   (with EDIT held K1-K4 move effect-slot controls instead of the knobs, one write each:
 *   fewer than the knobs' below, so the knob layer is the worst case)
 *   K4 to another preset, every register       25
 *   K1 cutoff 2, K2 resonance 1, K3 pulse width 6
 *                                               9
 *   two live_tick catch-ups (platform/fm1/main.c, TICK_CATCH_UP), each after a MASTER
 *   reading (the volume, 1) and writing the three frequencies (bend, vibrato), the three
 *   pulse widths (the PWM LFO, pressure) and the cutoff (wheel, pressure)
 *                                               2 * (1 + 14) = 30
 *   all 27 note keys pressed and released within the pass, a key's note-on and note-off
 *   11 together (below)
 *                                              27 * 11 = 297
 *   each tick's REG_RING_CHARACTER (the character's tick, below)
 *                                               2
 *                                             391 (REG_RING_LIVE_PASS)
 * Chip A's song control (M2c, through its own reg_ring_out), a provision until M2c measures
 * it: every register and the gate reset (a song's start), and a REG_RING_CHARACTER a tick
 * 25 + 1 + 2 = 28 (REG_RING_SONG_PASS). Not a reset: of the instrument's and the song's, only
 * one per pass (above), and the instrument's above counts it. The other chip is the song's or
 * follows the keys' setup (app/chip_ctl.c: the profile, the reset, the gate reset and a tick,
 * 4, under the song's 28), never both.
 * keys_route_tick (app/keys_route.c), once a pass, at most one of: a move of the keys (the
 * gate-offs on the old chip 3, the gate reset 1 and every register on the new one 25 = 29) or
 * the keys' own chip taken back to the keys' setup after a song (its reset 4 and every
 * register again 26 = 30): REG_RING_MOVE_PASS, 30. 391 + 28 + 30 = 449.
 * A note-on writes at most 11: a retrigger's gate-off (the sync lead steals its one voice) 1,
 * the velocity's sustain level 1, the voice's pulse width 2 (aftertouch MIDI changed since the
 * last write; the voice's own pressure starts again at 0), the cutoff 2 (the wheel MIDI moved
 * since the last tick), the frequency 2, the sync voice's frequency 2, the gate 1. A note-off
 * writes 1. 27 * (11 + 1) would be 324, but 27 * 11 still bounds the keys: the pulse width
 * and the cutoff are stale only after MIDI (before the panel in the pass), a preset or a model
 * change (write_all: 2 in a pass at most) or a note of another velocity, and the panel's notes
 * share one velocity. So each voice's pulse width is written by at most 3 of the pass's
 * note-ons and the cutoff by at most 3; every other note-on writes at most 7, and 27 keys at
 * most 27 * 7 + 3 * 3 * 2 + 3 * 2 + 27 = 240 <= 297.
 * The character (live.h, LIVE_REG_CHAR) adds one entry a tick: its settings never reach the
 * ring (the sink keeps them), a profile change is a model change above, and each tick's
 * LIVE_REG_CHAR_TICK queues one REG_RING_CHARACTER (the stage the ramp made, or none).
 * The host tests push this pass and check that it needs no wait. Bend, aftertouch and the
 * wheel (on a target other than the pulse width) only record their value: live_tick writes
 * it. MIDI input comes on top, first in the pass: the device's drain (platform/fm1/midi.c)
 * stops taking MIDI while fewer than this plus one MIDI entry's worst case and a later gap's
 * releases are free (MIDI_DRAIN_RESERVE). */

/* Orders memory accesses against an interrupt on the same core; emits no instruction.
 * (The pi32v2 compiler lowers __atomic_signal_fence to a __sync_synchronize call, which has
 * no implementation there; an empty asm with a memory clobber is the same compiler barrier.) */
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
    COMPILER_FENCE(); /* the entries before the index that publishes them */
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
    COMPILER_FENCE(); /* the head before the entry it publishes */
    *out = r->e[t & (REG_RING_SIZE - 1u)];
    COMPILER_FENCE(); /* the entry is read before its slot is handed back */
    r->tail = t + 1u;
    return 1;
}

/* The entries' chip bit (reg_ring.h) is clear on every register and pseudo-register live
 * sends and on the drain's own, and the slots' range is apart from both chips': nothing else
 * reaches the ring. */
_Static_assert(0x18u < REG_RING_CHIP_B && (0x18u | REG_RING_CHIP_B) < 0x80u,
               "chip B's SID registers, 0x40..0x58, are below the pseudo-registers");
_Static_assert(LIVE_REG_RESET == 0x80u && LIVE_REG_GATE_RESET == 0x81u &&
                   LIVE_REG_CHAR_TICK == 0x87u && REG_RING_CHARACTER == 0x88u,
               "chip A's pseudo-registers are 0x80..0x88");
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
    /* Chip A only: B is made by its first reset (live_init's for its writer), and nothing
     * renders it before. More work here would move the boot's timing. */
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
    c->default_8580_spent = 0;
}

sid* reg_ring_chip(reg_ring_chips* c) {
    return &c->chip[c->active[0]];
}

sid* reg_ring_chip_of(reg_ring_chips* c, uint32_t chip) {
    return &c->chip[c->active[chip & 1u]];
}

/* The sid neither chip renders, by the drain's choice (active) or the producer's (next). */
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
    /* An if chain, not a switch: a jump table would be data in flash, which the audio
     * interrupt's SRAM code must not read (platform/fm1/app.ld, emu-gate's ram_audio.rs). */
    uint32_t x = REG_RING_CHIP_OF(reg);
    uint8_t bare = REG_RING_BARE(reg);
    sid* s = &c->chip[c->active[x]];
    if (bare == LIVE_REG_GATE_RESET) {
        sid_set_gate_reset(s, e.value);
    } else if (bare == REG_RING_CHARACTER) {
        if (e.value < 2u) {
            sid_character_commit(s, &c->stage[x][e.value]);
            COMPILER_FENCE(); /* the stage is read before the producer may write it again */
            c->stage_pending[x][e.value] = 0;
        }
        c->ticks[x]++;
    } else if (bare == LIVE_REG_RESET) {
        if (c->spare_ready && c->spare_for == x && c->spare_model == e.value) {
            /* the spare (third), written out: apply runs from SRAM and calls nothing in flash */
            c->active[x] = (uint8_t)(3u - c->active[0] - c->active[1]);
            c->spare_ready = 0;
        } else {
            c->lost_resets++;
        }
    } else if (!x && c->pseudo) { /* chip B's unknown pseudo-registers: dropped */
        c->pseudo(c->pseudo_ctx, reg, e.value);
    }
}

uint32_t reg_ring_drain(reg_ring* r, reg_ring_chips* c) {
    reg_ring_entry e;
    uint32_t n = 0;
    while (reg_ring_pop(r, &e)) {
        apply(c, e);
        n++;
    }
    return n;
}

/* A character DAC table nothing renders from or will: not a sid either chip's queued entries
 * end on (next), not a chip's active one while a swap to its next is queued, not a queued
 * stage's, not one a ramp or the spare's preparation is building. NULL when all three are
 * taken. The drain frees a table (a swap, a commit) or hands one from a queued stage to a
 * chip (a commit), so the reads go against the drain's order. A commit writes the chip's
 * dac, then clears the stage's flag (apply): read the queued stages first, then the chips,
 * and a commit between the two leaves its table taken in either read (still queued when the
 * stages were read, or the chip's when the chips were). The other way round, a commit
 * between them would hide it from both (the chip read before it, the flag after), and a
 * table the chip plays would come back free (app test reg_ring.rs plays that interrupt). A
 * swap after the stages were read only frees the old active chip's table, which then reads
 * as taken or free, both safe. The tables being built are the producer's own. */
static sid_dac* free_dac(reg_ring_chips* c) {
    /* each chip's two stages, its next and active sids and its ramp's job; the spare's */
    const sid_dac* taken[2u * REG_RING_CHIPS + 3u * REG_RING_CHIPS + 1u];
    uint32_t n = 0;
    for (uint32_t x = 0; x < REG_RING_CHIPS; ++x) {
        for (uint32_t k = 0; k < 2u; ++k) {
            if (c->stage_pending[x][k]) {
                taken[n++] = c->stage[x][k].dac;
            }
        }
    }
    COMPILER_FENCE(); /* the stages before the chips (above) */
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

/* Initialises the spare for chip x for model with its character, at once (sid_init resets
 * it); the ring must be empty (no swap or stage queued). Any ramp still running on the chip
 * ends there: the spare has every control as asked. */
static void prepare_spare(reg_ring_chips* c, uint32_t x, uint8_t model) {
    sid_model m = model == SID_MODEL_8580 ? SID_MODEL_8580 : SID_MODEL_6581;
    uint8_t spare = third(c->active);
    c->prep_on = 0; /* its sid is this one */
    sid* s = &c->chip[spare];
    sid_init(s, m, c->clock_hz, c->sample_rate);
    c->ramp[x].building = 0;
    const sid_dac* table = sid_character_apply(s, &c->ramp[x].target, free_dac(c));
    sid_character_ramp_reset(&c->ramp[x], &c->ramp[x].target, table);
    c->next[x] = spare;
    c->spare_model = (uint8_t)m;
    c->spare_for = (uint8_t)x;
    COMPILER_FENCE(); /* the chip is ready before the flag says so */
    c->spare_ready = 1;
}

/* The spare whole for a reset of chip x to model, prepared for the profile live has just
 * sent: the chip's sid from now on (next), its character the one the ramp starts from, the
 * settings as live last sent them still the ramp's target. */
static int take_prepared(reg_ring_chips* c, uint32_t x, uint8_t model) {
    sid_character_ramp* ramp = &c->ramp[x];
    if (!c->prep_on || c->prep_chip != x || !sid_character_prep_done(&c->prep) ||
        c->prep.model != model || c->prep.c.profile != ramp->target.profile) {
        return 0;
    }
    sid_character want = ramp->target;
    sid_character_ramp_reset(ramp, &c->prep.c, c->prep.table);
    ramp->target = want;
    c->next[x] = third(c->next);
    c->spare_model = model;
    c->spare_for = (uint8_t)x;
    c->prep_on = 0;
    COMPILER_FENCE(); /* the chip is ready before the flag says so */
    c->spare_ready = 1;
    return 1;
}

int reg_ring_prepare_step(void* ctx, uint8_t profile) {
    reg_ring_out* o = ctx;
    reg_ring_chips* c = o->chips;
    uint32_t x = o->chip & 1u;
    const sid_chip_profile* p = sid_chip_profile_get(profile);
    if (!p) { /* LIVE_PREPARE_CANCEL */
        c->prep_wait[x] = 0;
        if (c->prep_on && c->prep_chip == x) {
            c->prep_on = 0;
        }
        return 0;
    }
    int keys = x == c->keys;
    if (c->prep_on && c->prep_chip != x) {
        if (!keys) {
            c->prep_wait[x] = 1; /* the other chip's: this one waits */
            return 0;
        }
        c->prep_on = 0; /* the keys' chip first: the other's is dropped, and asked again */
        c->prep_wait[x ^ 1u] = 1;
    }
    int same = c->prep_on && c->prep_profile == profile;
    if (same && sid_character_prep_done(&c->prep)) {
        return 1;
    }
    if (c->prep_pass || (!keys && c->build_pass)) {
        if (!keys) {
            c->prep_wait[x] = 1;
        }
        return 0; /* one build's work a pass: the next tick's pass takes it */
    }
    if (!same) {
        if (c->spare_ready || (!keys && c->ramp[x ^ 1u].building)) {
            c->prep_on = 0;
            c->prep_wait[x] = 1;
            return 0;
        }
        c->ramp[x ^ 1u].building = 0; /* the other's table, unplayed: it starts it again */
        sid_character want = c->ramp[x].target;
        want.profile = (uint8_t)(profile + 1u);
        c->ramp[x].building = 0;
        uint32_t defaults = p->model == SID_MODEL_8580 && !c->default_8580_spent
                                ? SID_DAC_BUILD_STEPS
                                : 0u;
        sid_character_prep_start(&c->prep, &c->chip[third(c->next)], (sid_model)p->model,
                                 c->clock_hz, c->sample_rate, &want, defaults);
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

/* The character's tick (LIVE_REG_CHAR_TICK) of the out's chip, on the sid its queued entries
 * end on: the ramp's next step into a stage that is not queued. The REG_RING_CHARACTER value.
 * A pass runs one build's work: a slice, or one DAC table step (40k + 28k on top of a view's
 * 68k would pass the poll allowance), and the keys' chip (reg_ring_set_keys) goes first:
 * - the keys' chip's ramp gets no table only while its own chip is being prepared or in a
 *   pass that ran a slice (the last one's included: the spare is then taken, prep_on clear);
 *   a table the other chip's ramp or preparation is building, if it needs it, is taken back
 *   (that build starts again later);
 * - the other chip's ramp gets no table while a preparation runs, in a pass that ran a slice
 *   or a table step, while the keys' ramp builds, and while the keys' request waits; and its
 *   own build pauses in a pass the keys' chip has built in (it ticks after the keys').
 * So the keys' chip changes as one chip alone would (sidou-tests' the_keys_chip_plays_as_if_
 * alone), and the other catches up when the keys' chip is quiet. */
static uint8_t character_tick(reg_ring_out* o) {
    reg_ring_chips* c = o->chips;
    uint32_t x = o->chip & 1u;
    uint32_t y = x ^ 1u;
    int keys = x == c->keys;
    sid_character_ramp* ramp = &c->ramp[x];
    if (!keys && ramp->building && (c->prep_pass || c->build_pass)) {
        return REG_RING_NO_STAGE; /* the keys' chip built this pass: a tick later */
    }
    const volatile uint8_t* pending = c->stage_pending[x];
    uint32_t k = !pending[0] ? 0u : !pending[1] ? 1u : 2u;
    if (k == 2u) {
        o->character_defers++;
        return REG_RING_NO_STAGE;
    }
    int busy = keys ? (c->prep_on && c->prep_chip == x) || c->prep_pass
                    : c->prep_on || c->prep_pass || c->build_pass || c->ramp[y].building ||
                          c->prep_wait[y];
    sid_dac* table = busy ? NULL : free_dac(c);
    if (keys && !busy && !table && sid_character_ramp_wants_table(ramp)) {
        /* The third table is the other chip's build: take it back, unplayed. */
        if (c->ramp[y].building) {
            c->ramp[y].building = 0;
        } else if (c->prep_on && c->prep_chip == y && c->prep.job.d) {
            c->prep_on = 0;
            c->prep_wait[y] = 1;
        }
        table = free_dac(c);
    }
    uint8_t was = ramp->building;
    int changed = sid_character_ramp_tick(ramp, &c->chip[c->next[x]], table, &c->stage[x][k]);
    if (was || ramp->building) {
        c->build_pass = 1;
    }
    if (!changed) {
        return REG_RING_NO_STAGE;
    }
    c->stage_pending[x][k] = 1;
    return (uint8_t)k;
}

/* Commits the pass so far and waits until the interrupt has applied all of it. */
static void wait_empty(reg_ring_out* o, uint32_t* waits) {
    reg_ring_commit(o->ring);
    while (reg_ring_used(o->ring)) {
        (*waits)++;
        o->wait(o->wait_ctx);
    }
}

/* Pushes, waiting for room (the ring never drops). */
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
    if (!(reg & 0x80u)) { /* a SID register, most of them: the chip's bit and the push */
        push_waiting(o, (uint8_t)(reg | x << 6), value);
        return;
    }
    if (reg >= LIVE_REG_CHAR(0) && reg < LIVE_REG_CHAR(LIVE_CHAR_FIELDS)) {
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
