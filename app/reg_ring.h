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
#include "sid/sid_character.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Entries in the ring, a power of two. The worst case of one main-loop pass is 391 entries
 * (reg_ring.c, REG_RING_WORST_PASS), so a pass never waits for room on its own account. The
 * rest is MIDI's: the device's drain (platform/fm1/midi.c) takes an entry only while the
 * pass's worst case and the entry's still fit (at most 552 entries queued before it): between
 * two audio blocks (2.9 ms) it takes up to 23 program changes, 25 writes each, where the TRS
 * jack delivers 9. At 512 it took 2, and a flood overflowed the MIDI ring in about 150 ms.
 * 2 bytes an entry: 2 KiB of .bss. */
#define REG_RING_SIZE 1024u
#define REG_RING_WORST_PASS 391u

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

/* The drain's own pseudo-register, one for each of live's LIVE_REG_CHAR_TICK (live never sends
 * it): the value is the stage to commit to the active chip (reg_ring_chips, stage[0] or [1]),
 * or REG_RING_NO_STAGE for a tick that changed nothing. */
#define REG_RING_CHARACTER 0x88u
#define REG_RING_NO_STAGE 0xffu

/* Gets the pseudo-registers the drain does not handle itself (the effect slots',
 * app/fx_live.h), from the audio interrupt. */
typedef void (*reg_ring_pseudo_fn)(void* ctx, uint8_t reg, uint8_t value);

/* Two chips, one of them rendered. The drain swaps them at a LIVE_REG_RESET entry once the
 * producer has initialised the spare for that model.
 *
 * The chip's character (sid_character.h; live.h, LIVE_REG_CHAR) is the producer's: the sink
 * keeps the settings live sends (ramp.target) and applies them in the main loop, to the spare
 * before every swap (all of them at once, as sid_init reset them), and to the rendered chip a
 * tick at a time (sid_character_ramp_tick at each LIVE_REG_CHAR_TICK), never waiting for the
 * audio interrupt:
 * - the ramp's DAC tables go into whichever of `dac` neither chip that renders or will (the
 *   active one, and the spare while a swap is queued) nor a queued stage points at; with
 *   none free the DACs wait a tick;
 * - a tick's stage goes into whichever of `stage` is not queued (stage_pending), and the
 *   sink pushes REG_RING_CHARACTER with its index, at which the drain copies it into the
 *   rendered chip (sid_character_commit) and clears its flag; with both queued (two ticks in
 *   one pass, then a third before the interrupt drained either) the tick waits, counted in
 *   reg_ring_out.character_defers.
 * The producer writes a stage only while its flag is clear, sets the flag before the entry
 * that names it is committed (reg_ring_commit's fence orders the stores), and the drain
 * clears it only after the copy (a fence between): one core, the interrupt runs to its end
 * before the producer resumes, so neither ever sees the other's half-written stage. So
 * everything the render reads that the character sets changes at one block boundary, in
 * order with the register writes around it, and the host's sink (live_sid_sink), which
 * runs the same ramp at the same ticks, makes the same chip.
 *
 * The filter tables (sid.h: cutoff_g, res_k and the hot row's) never change on a chip that
 * renders: age/heat is the stage's filter_t, and a profile's tables are built on the spare
 * before the swap. The render never reads them; only sid_update_filter does, at a $15-$17
 * write and at a commit, both applied by the drain; and each entry is an aligned 32-bit
 * store, so even sid_set_filter_curve on a playing chip (sid.h allows it from the main
 * loop) would give such a write an old or a new entry, never a torn one. That rests on
 * every register write reaching a chip through this drain, on one core: M3's 6502 player,
 * if it writes registers from the audio interrupt or from core 1, must revisit it (a write
 * applied outside the drain races its commits and the main loop's spare). */
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
    sid_character_ramp ramp; /* producer: the settings and the ramp towards them */
    uint8_t next; /* producer: the chip the queued entries end on, the last one prepared */
    volatile uint8_t stage_pending[2]; /* producer sets, the drain clears (above) */
    volatile uint32_t ticks; /* drain: REG_RING_CHARACTER entries applied, every tick's */
    sid_character_stage stage[2];
    sid_dac dac[2]; /* the character's DAC tables, when they are not the model's defaults */
} reg_ring_chips;

/* chip[active] is chip 0, initialised for model; chip 1 is left zeroed; no pseudo handler;
 * the character the model's default. */
void reg_ring_chips_init(reg_ring_chips* c, sid_model model, uint32_t clock_hz,
                         uint32_t sample_rate);
/* The chip to render. */
sid* reg_ring_chip(reg_ring_chips* c);

/* Consumer: applies every committed entry in order. A SID register goes to sid_write on the
 * active chip; a LIVE_REG_RESET swaps to the spare (it never reaches sid_write, which would
 * alias it to register 0x00); a LIVE_REG_GATE_RESET goes to sid_set_gate_reset on the active
 * chip, so after a reset the new one; a REG_RING_CHARACTER counts a tick (ticks) and commits
 * the stage it names to the active chip (sid_character_commit); any other pseudo-register
 * goes to the pseudo handler, if there is one. Returns the entries applied. */
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
    uint32_t character_defers; /* ticks that found both stages queued (reg_ring_chips) */
} reg_ring_out;

/* A live_sink_fn: ctx is a reg_ring_out*. The character's settings (LIVE_REG_CHAR) are kept,
 * not queued; LIVE_REG_CHAR_TICK runs the ramp's tick on the chip and queues
 * REG_RING_CHARACTER, without waiting; LIVE_REG_RESET waits for the ring to empty and
 * initialises the spare with the model and the character. */
void reg_ring_sink(void* ctx, uint8_t reg, uint8_t value);

#ifdef __cplusplus
}
#endif

#endif
