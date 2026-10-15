// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "reg_ring.h"

#include "live.h"

#include <stddef.h>

/* REG_RING_WORST_PASS, the most one main-loop pass can push (live.c's writes per call):
 *   a model change, a reset, the gate reset and every register
 *                                               1 + 1 + 25
 *   (the FX button, or live_tick applying the model MIDI asked for: one per pass at most.
 *   MIDI only records the request; the panel runs before the ticks and the button settles
 *   any request, and a tick applies one only if it differs from the chip's.)
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
 *                                             389
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
 * The character (live.h, LIVE_REG_CHAR) adds nothing to it: its settings never reach the
 * ring (the sink keeps them), a profile change is a model change above, and a tick's
 * control change (LIVE_REG_CHAR_APPLY) waits for the ring to empty, then queues two entries
 * (REG_RING_CHARACTER and $17), far fewer than the pass's entries it drained.
 * The host tests push this pass and check that it needs no wait. Bend, aftertouch and the
 * wheel (on a target other than the pulse width) only record their value: live_tick writes
 * it. MIDI input comes on top, first in the pass: the device's drain (platform/fm1/midi.c)
 * stops taking MIDI while fewer than this plus one MIDI entry's worst case and a later gap's
 * releases are free (MIDI_DRAIN_RESERVE). */

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
    sid_character_init(&c->character);
    c->applied = c->character;
}

sid* reg_ring_chip(reg_ring_chips* c) {
    return &c->chip[c->active];
}

static void apply(reg_ring_chips* c, reg_ring_entry e) {
    if (!(e.reg & 0x80u)) {
        sid_write(&c->chip[c->active], e.reg, e.value);
        return;
    }
    if (e.reg == LIVE_REG_GATE_RESET) {
        sid_set_gate_reset(&c->chip[c->active], e.value);
        return;
    }
    if (e.reg == REG_RING_CHARACTER) {
        sid_character_commit(&c->chip[c->active], &c->stage);
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

/* The character's DAC table not in use: the one the rendered chip does not point at. With
 * the ring empty nothing queued points at either. */
static sid_dac* free_dac(reg_ring_chips* c) {
    return c->chip[c->active].dac == &c->dac[0] ? &c->dac[1] : &c->dac[0];
}

/* Initialises the spare chip for model with the character (sid_init resets it); the ring
 * must be empty (no swap pending). */
static void prepare_spare(reg_ring_chips* c, uint8_t model) {
    sid_model m = model == SID_MODEL_8580 ? SID_MODEL_8580 : SID_MODEL_6581;
    sid* spare = &c->chip[c->active ^ 1u];
    sid_init(spare, m, c->clock_hz, c->sample_rate);
    sid_character_apply(spare, &c->character, free_dac(c));
    c->applied = c->character;
    c->spare_model = (uint8_t)m;
    COMPILER_FENCE(); /* the chip is ready before the flag says so */
    c->spare_ready = 1;
}

/* What changed in the character since the rendered chip's, onto it, from the main loop; the
 * ring must be empty. The filter tables are rebuilt in place: with nothing queued no
 * sid_write reads them before the $17 write live sends after an age change, and the render
 * reads its coefficients, not the tables. The stage waits in c->stage for the drain. */
static void apply_character(reg_ring_chips* c) {
    sid* s = &c->chip[c->active];
    const sid_character* want = &c->character;
    sid_character* had = &c->applied;
    int profile = want->profile != had->profile;
    if (profile || want->age != had->age) {
        sid_character_filter(s, want);
    }
    const sid_dac* table = s->dac;
    if (profile || want->dac != had->dac) {
        table = NULL; /* the model's default */
        if (sid_character_dac_custom(s, want)) {
            sid_dac* d = free_dac(c);
            sid_character_build_dac(s, want, d);
            table = d;
        }
    }
    sid_character_stage_for(s, want, table, &c->stage);
    *had = *want;
}

/* Commits the pass so far and waits until the interrupt has applied all of it. */
static void wait_empty(reg_ring_out* o, uint32_t* waits) {
    reg_ring_commit(o->ring);
    while (reg_ring_used(o->ring)) {
        (*waits)++;
        o->wait(o->wait_ctx);
    }
}

void reg_ring_sink(void* ctx, uint8_t reg, uint8_t value) {
    reg_ring_out* o = ctx;
    reg_ring* r = o->ring;
    if (reg >= LIVE_REG_CHAR(0) && reg < LIVE_REG_CHAR(LIVE_CHAR_FIELDS)) {
        live_char_store(&o->chips->character, reg, value);
        return;
    }
    if (reg == LIVE_REG_CHAR_APPLY) {
        wait_empty(o, &o->character_waits);
        apply_character(o->chips);
        reg = REG_RING_CHARACTER;
        value = 0;
    }
    if (reg == LIVE_REG_RESET) {
        wait_empty(o, &o->reset_waits);
        prepare_spare(o->chips, value);
        value = o->chips->spare_model;
    }
    while (!reg_ring_push(r, reg, value)) {
        reg_ring_commit(r);
        o->full_waits++;
        o->wait(o->wait_ctx);
    }
}
