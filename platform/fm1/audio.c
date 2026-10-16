// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "audio.h"

#include "fx/fx_slots.h"
#include "fx_live.h"
#include "reg_ring.h"
#include "scope.h"
#include "sid/sid.h"

/* The effect slots' seed (their noise and drift generators). */
#define SLOTS_SEED 0x51D0u

/* reg_ring_drain runs from SRAM (app.ld): beyond the direct call range of this file's
 * main-loop code in flash, so those calls go through a pointer the compiler cannot fold back
 * into a direct call (as Felucca's FL_FAR). audio_fill, in SRAM too, calls it directly. */
static uint32_t far_drain(reg_ring* r, reg_ring_chips* c) {
    uint32_t (*volatile fn)(reg_ring*, reg_ring_chips*) = reg_ring_drain;
    return fn(r, c);
}

/* Chips A and B and their shared spare (three sids, ~17 KB each, and three DAC tables), the
 * effect slots (~6.9 KB) and their 128 KiB delay/reverb pool: the big zero-initialised pool,
 * not .bss. */
static reg_ring_chips chips __attribute__((section(".pool")));
static sid_slots slots __attribute__((section(".pool")));
static sid_slot_pool slot_pool __attribute__((section(".pool")));
/* The slots' settings as live sent them; the drain applies them (in the interrupt). */
static fx_live fx;

static reg_ring ring;
static reg_ring_out out;
/* The LIVE view's tap (app/scope.h): the voices every SCOPE_DECIMATE samples, the envelopes
 * and the output's peaks, while the view shows. */
static scope tap;
_Static_assert(AUDIO_HALF_FRAMES % SCOPE_DECIMATE == 0u, "a block is whole chunks of the tap");
static volatile uint8_t running;

/* reg_ring_out.wait: the producer needs the interrupt to drain. Before audio_irqs_on the
 * interrupts are off, and an idle would never wake: drain here instead. */
static void wait_for_drain(void* ctx) {
    (void)ctx;
    if (!running) {
        far_drain(&ring, &chips);
        return;
    }
    /* Sleep until the next interrupt (TIMER5 within 100 us, the block within 2.9 ms). */
    __asm__ volatile("idle" ::: "memory");
}

void audio_init(void) {
    reg_ring_init(&ring);
    scope_init(&tap);
    reg_ring_chips_init(&chips, SID_MODEL_6581, SID_CLOCK_PAL, AUDIO_SAMPLE_RATE);
    sid_slots_init(&slots, AUDIO_SAMPLE_RATE, SLOTS_SEED, &slot_pool);
    fx_live_init(&fx, &slots);
    chips.pseudo = fx_live_pseudo;
    chips.pseudo_ctx = &fx;
    out.ring = &ring;
    out.chips = &chips;
    out.chip = 0; /* the instrument plays chip A until M2b Task 4 */
    out.wait = wait_for_drain;
    out.wait_ctx = 0;
}

scope* audio_scope(void) {
    return &tap;
}

void* audio_sink_ctx(void) {
    return &out;
}

void audio_commit(void) {
    reg_ring_commit(&ring);
}

uint32_t audio_queued(void) {
    return reg_ring_used(&ring);
}

void audio_start(void) {
    reg_ring_commit(&ring);
    far_drain(&ring, &chips);
}

void audio_irqs_on(void) {
    running = 1;
}

void audio_halted(void) {
    running = 0;
}

int audio_running(void) {
    return running;
}

void audio_fill(int32_t* buf) {
    int16_t s[AUDIO_HALF_FRAMES];
    int16_t lr[2u * AUDIO_HALF_FRAMES];
    reg_ring_drain(&ring, &chips);
    /* sid_render's samples, and the tap's points while LIVE shows. */
    scope_render(&tap, reg_ring_chip(&chips), s, AUDIO_HALF_FRAMES);
    /* With every slot off this writes the chip's output to both sides unchanged. */
    sid_slots_process(&slots, s, lr, AUDIO_HALF_FRAMES);
    scope_peaks(&tap, lr, AUDIO_HALF_FRAMES);
    for (uint32_t i = 0; i < 2u * AUDIO_HALF_FRAMES; i++) {
        buf[i] = (int32_t)lr[i] * 256;
    }
}
