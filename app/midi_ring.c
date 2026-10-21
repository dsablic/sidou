// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_ring.h"

/* Orders memory accesses against an interrupt on the same core; emits no instruction
 * (as in reg_ring.c: the pi32v2 compiler has no __atomic_signal_fence). */
#define COMPILER_FENCE() __asm__ volatile("" ::: "memory")

void midi_ring_init(midi_ring* r) {
    r->head = 0;
    r->tail = 0;
    r->lost = 0;
    r->epoch = 0;
    r->in_gap = 0;
}

static void lose(midi_ring* r, uint8_t source, uint32_t n) {
    r->lost += n;
    if (!r->in_gap) {
        r->epoch++;
    }
    r->in_gap = (uint8_t)(r->in_gap | (source ? source : 0xFFu)); /* never 0 in a gap */
}

int midi_ring_push(midi_ring* r, uint8_t source, uint32_t data, uint32_t stamp) {
    uint32_t h = r->head;
    if (h - r->tail >= MIDI_RING_SIZE) {
        lose(r, source, 1u);
        return 0;
    }
    COMPILER_FENCE(); /* the tail before the slot it handed back */
    midi_ring_entry* e = &r->e[h & (MIDI_RING_SIZE - 1u)];
    e->stamp = stamp;
    e->data = data;
    e->source = source;
    e->epoch = r->epoch;
    e->gap = r->in_gap;
    r->in_gap = 0;
    COMPILER_FENCE(); /* the entry before the index that publishes it */
    r->head = h + 1u;
    return 1;
}

uint32_t midi_ring_room(const midi_ring* r) {
    return MIDI_RING_SIZE - (r->head - r->tail);
}

void midi_ring_lose(midi_ring* r, uint8_t source, uint32_t n) {
    if (n) {
        lose(r, source, n);
    }
}

int midi_ring_pop(midi_ring* r, midi_ring_entry* out) {
    uint32_t t = r->tail;
    if (t == r->head) {
        return 0;
    }
    COMPILER_FENCE(); /* the head before the entry it publishes */
    *out = r->e[t & (MIDI_RING_SIZE - 1u)];
    COMPILER_FENCE(); /* the entry is read before its slot is handed back */
    r->tail = t + 1u;
    return 1;
}

uint32_t midi_ring_lost(const midi_ring* r) {
    return r->lost;
}

uint8_t midi_ring_epoch(const midi_ring* r) {
    return r->epoch;
}
