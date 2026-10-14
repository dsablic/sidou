// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "audio.h"

#include "reg_ring.h"
#include "sid/sid.h"

/* reg_ring_drain runs from SRAM (app.ld): beyond the direct call range of this file's
 * main-loop code in flash, so those calls go through a pointer the compiler cannot fold back
 * into a direct call (as Felucca's FL_FAR). audio_fill, in SRAM too, calls it directly. */
static uint32_t far_drain(reg_ring* r, reg_ring_chips* c) {
    uint32_t (*volatile fn)(reg_ring*, reg_ring_chips*) = reg_ring_drain;
    return fn(r, c);
}

/* Two chips of ~8.5 KB each: the big zero-initialised pool, not .bss. */
static reg_ring_chips chips __attribute__((section(".pool")));
static reg_ring ring;
static reg_ring_out out;
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
    reg_ring_chips_init(&chips, SID_MODEL_6581, SID_CLOCK_PAL, AUDIO_SAMPLE_RATE);
    out.ring = &ring;
    out.chips = &chips;
    out.wait = wait_for_drain;
    out.wait_ctx = 0;
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
    reg_ring_drain(&ring, &chips);
    sid_render(reg_ring_chip(&chips), s, AUDIO_HALF_FRAMES);
    for (uint32_t i = 0; i < AUDIO_HALF_FRAMES; i++) {
        int32_t v = (int32_t)s[i] * 256;
        buf[2 * i] = v;
        buf[2 * i + 1] = v;
    }
}
