// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The register queue from the main loop to the audio interrupt, and the two chips it
 * drives.
 *
 * Portable C with no hardware access, so the host tests run the very code the device runs.
 *
 * The main loop (the producer) turns live's register writes into ring entries through
 * reg_ring_sink. The audio interrupt (the consumer) applies them with reg_ring_drain at the
 * start of each block, so a chip only ever changes between two rendered blocks.
 *
 * Single producer, single consumer, one core: the consumer is an interrupt that preempts the
 * producer, never the other way round. Each index is written by one side only, and the
 * compiler fences keep the entries' stores before the index store that publishes them.
 *
 * Batches: pushed entries stay private to the producer until reg_ring_commit. The main loop
 * commits once per pass, so the interrupt sees the whole of a pass's writes or none of them
 * (unless the ring fills: then the sink commits early and waits).
 *
 * Overflow never drops: a dropped write could leave a gate stuck or the wrong chip model.
 * When the ring is full the sink commits and calls the wait callback until the consumer has
 * made room. On the device that sleeps until the next interrupt; the producer must therefore
 * never run with interrupts off once the audio runs (before that, the callback drains).
 *
 * Model change (LIVE_REG_RESET): re-initialising a chip costs far more than a block's time
 * budget allows inside the interrupt (sid_init computes 2048 cutoff coefficients). So the
 * sink does it in the main loop, on the chip the interrupt is not rendering, and the drain
 * only swaps which chip is active when it reaches the reset entry. Before that the sink waits
 * for the ring to empty, so no earlier reset (and swap) is still pending. */
#ifndef SIDOU_REG_RING_H
#define SIDOU_REG_RING_H

#include <stdint.h>

#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Entries in the ring, a power of two. The worst case of one main-loop pass is 263 entries
 * (reg_ring.c, REG_RING_WORST_PASS), so a pass never waits for room on its own account. */
#define REG_RING_SIZE 512u
#define REG_RING_WORST_PASS 263u

typedef struct {
    uint8_t reg; /* a SID register 0x00..0x18, or a live pseudo-register (bit 7 set) */
    uint8_t value;
} reg_ring_entry;

typedef struct {
    reg_ring_entry e[REG_RING_SIZE];
    volatile uint32_t head; /* producer: entries before it are visible to the consumer */
    volatile uint32_t tail; /* consumer: entries before it have been applied */
    uint32_t stage;         /* producer only: entries before it are pushed */
} reg_ring;

void reg_ring_init(reg_ring* r);
/* Producer. 1 on success, 0 when the ring is full (nothing is written). */
int reg_ring_push(reg_ring* r, uint8_t reg, uint8_t value);
/* Producer: makes the pushed entries visible to the consumer. */
void reg_ring_commit(reg_ring* r);
/* Producer: entries pushed and not yet applied (committed or not). */
uint32_t reg_ring_used(const reg_ring* r);
/* Consumer. 1 and the oldest committed entry, or 0 when there is none. */
int reg_ring_pop(reg_ring* r, reg_ring_entry* out);

/* Gets the pseudo-registers the drain does not handle itself (the effect slots',
 * app/fx_live.h), from the audio interrupt. */
typedef void (*reg_ring_pseudo_fn)(void* ctx, uint8_t reg, uint8_t value);

/* Two chips, one of them rendered. The drain swaps them at a LIVE_REG_RESET entry once the
 * producer has initialised the spare for that model. */
typedef struct {
    sid chip[2];
    volatile uint8_t active;       /* the rendered chip; changed by the drain only */
    volatile uint8_t spare_ready;  /* producer sets it after initialising the spare */
    volatile uint8_t spare_model;  /* the sid_model the spare was initialised with */
    volatile uint32_t lost_resets; /* resets that found no spare ready (a bug if not 0) */
    uint32_t clock_hz;
    uint32_t sample_rate;
    reg_ring_pseudo_fn pseudo; /* NULL: other pseudo-registers are ignored */
    void* pseudo_ctx;
} reg_ring_chips;

/* chip[active] is chip 0, initialised for model; chip 1 is left zeroed; no pseudo handler. */
void reg_ring_chips_init(reg_ring_chips* c, sid_model model, uint32_t clock_hz,
                         uint32_t sample_rate);
/* The chip to render. */
sid* reg_ring_chip(reg_ring_chips* c);

/* Consumer: applies every committed entry in order. A SID register goes to sid_write on the
 * active chip; a LIVE_REG_RESET swaps to the spare (it never reaches sid_write, which would
 * alias it to register 0x00); any other pseudo-register goes to the pseudo handler, if
 * there is one. Returns the entries applied. */
uint32_t reg_ring_drain(reg_ring* r, reg_ring_chips* c);

/* The producer's side, the context of reg_ring_sink. */
typedef struct {
    reg_ring* ring;
    reg_ring_chips* chips;
    /* Called while the producer waits for the consumer: the device sleeps until the next
     * interrupt (or drains itself before the audio runs); the host tests drain. */
    void (*wait)(void* ctx);
    void* wait_ctx;
    uint32_t full_waits;  /* waits for room in a full ring */
    uint32_t reset_waits; /* waits for the ring to empty before a model change */
} reg_ring_out;

/* A live_sink_fn: ctx is a reg_ring_out*. */
void reg_ring_sink(void* ctx, uint8_t reg, uint8_t value);

#ifdef __cplusplus
}
#endif

#endif
