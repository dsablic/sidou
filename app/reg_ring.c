// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "reg_ring.h"

#include "live.h"

/* REG_RING_WORST_PASS, the most one main-loop pass can push (live.c's writes per call):
 *   FX, a reset and every register         1 + 25
 *   ENV, the selected effect slot's next algorithm  1
 *   (with EDIT held K1-K4 move effect-slot controls instead of the knobs, one write each:
 *   fewer than the knobs' below, so the knob layer is the worst case)
 *   K4 to another preset, every register       25
 *   K1 cutoff 2, K2 resonance 1, K3 pulse width 6, MASTER volume 1
 *                                              10
 *   two live_tick catch-ups, pulse width 6 each 12
 *   all 27 note keys pressed and released within the pass: a note-on is at most 6 (a
 *   retrigger's gate-off, the frequency, the sync voice's frequency, the gate), a
 *   note-off 1                           27 * 7 = 189
 *                                             263
 * An upper bound: a key's note-off leaves the gate off, so the next note-on writes 5. The
 * host tests push this pass (223 entries) and check that it needs no wait.
 * MIDI input comes on top, first in the pass: the device's drain (platform/fm1/midi.c) stops
 * taking MIDI while fewer than this plus one MIDI message's worst case (25) are free. */

/* Orders memory accesses against an interrupt on the same core; emits no instruction.
 * (The pi32v2 compiler lowers __atomic_signal_fence to a __sync_synchronize call, which has
 * no implementation there; an empty asm with a memory clobber is the same compiler barrier.) */
#define COMPILER_FENCE() __asm__ volatile("" ::: "memory")

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

void reg_ring_chips_init(reg_ring_chips* c, sid_model model, uint32_t clock_hz,
                         uint32_t sample_rate) {
    c->clock_hz = clock_hz;
    c->sample_rate = sample_rate;
    c->active = 0;
    c->spare_ready = 0;
    c->spare_model = 0;
    c->lost_resets = 0;
    c->pseudo = 0;
    c->pseudo_ctx = 0;
    sid_init(&c->chip[0], model, clock_hz, sample_rate);
}

sid* reg_ring_chip(reg_ring_chips* c) {
    return &c->chip[c->active];
}

static void apply(reg_ring_chips* c, reg_ring_entry e) {
    if (!(e.reg & 0x80u)) {
        sid_write(&c->chip[c->active], e.reg, e.value);
        return;
    }
    if (e.reg != LIVE_REG_RESET) {
        if (c->pseudo) {
            c->pseudo(c->pseudo_ctx, e.reg, e.value);
        }
        return;
    }
    if (c->spare_ready && c->spare_model == e.value) {
        c->active ^= 1u;
        c->spare_ready = 0;
    } else {
        c->lost_resets++;
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

/* Initialises the spare chip for model; the ring must be empty (no swap pending). */
static void prepare_spare(reg_ring_chips* c, uint8_t model) {
    sid_model m = model == SID_MODEL_8580 ? SID_MODEL_8580 : SID_MODEL_6581;
    sid_init(&c->chip[c->active ^ 1u], m, c->clock_hz, c->sample_rate);
    c->spare_model = (uint8_t)m;
    COMPILER_FENCE(); /* the chip is ready before the flag says so */
    c->spare_ready = 1;
}

void reg_ring_sink(void* ctx, uint8_t reg, uint8_t value) {
    reg_ring_out* o = ctx;
    reg_ring* r = o->ring;
    if (reg == LIVE_REG_RESET) {
        reg_ring_commit(r);
        while (reg_ring_used(r)) {
            o->reset_waits++;
            o->wait(o->wait_ctx);
        }
        prepare_spare(o->chips, value);
        value = o->chips->spare_model;
    }
    while (!reg_ring_push(r, reg, value)) {
        reg_ring_commit(r);
        o->full_waits++;
        o->wait(o->wait_ctx);
    }
}
