// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MIDI_RING_H
#define SIDOU_MIDI_RING_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MIDI_RING_SIZE 256u

#define MIDI_SRC_USB 1u
#define MIDI_SRC_TRS 2u

typedef struct {
    uint32_t stamp;
    uint32_t data;
    uint8_t source;
    uint8_t epoch;
    uint8_t gap;
} midi_ring_entry;

typedef struct {
    midi_ring_entry e[MIDI_RING_SIZE];
    volatile uint32_t head;
    volatile uint32_t tail;
    volatile uint32_t lost;
    volatile uint8_t epoch;
    volatile uint8_t in_gap;
} midi_ring;

void midi_ring_init(midi_ring* r);
int midi_ring_push(midi_ring* r, uint8_t source, uint32_t data, uint32_t stamp);
uint32_t midi_ring_room(const midi_ring* r);
void midi_ring_lose(midi_ring* r, uint8_t source, uint32_t n);
int midi_ring_pop(midi_ring* r, midi_ring_entry* out);
uint32_t midi_ring_lost(const midi_ring* r);
uint8_t midi_ring_epoch(const midi_ring* r);

#ifdef __cplusplus
}
#endif

#endif
