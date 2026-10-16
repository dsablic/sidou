// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "audio.h"

#include "chip_mix.h"
#include "fx/fx_slots.h"
#include "fx_live.h"
#include "live.h"
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
/* The writer bound to chip B (reg_ring_out.chip 1): its boot reset (audio_chip_b_boot), and
 * from M2b Task 4 the keys while they play it. */
static reg_ring_out out_b;
/* Chip B in use as the main loop asks (audio_chip_b_use), and as the last block had it: the
 * interrupt fades at the block where they differ. */
static volatile uint8_t b_want;
static uint8_t b_on;
/* The chip the tap reads (audio_tap_chip): 0 A, 1 B. */
static volatile uint8_t tap_chip;
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
    out_b = out;
    out_b.chip = 1;
}

void* audio_sink_ctx_b(void) {
    return &out_b;
}

/* Chip B's first reset, prepared a slice a pass from the main loop's ticks
 * (audio_chip_b_tick); 0 until it has been sent. */
static uint8_t b_profile, b_model, b_gate_reset, b_made;

void audio_chip_b_boot(sid_model model, uint8_t profile, uint8_t gate_reset) {
    /* The character's settings (live.h: the profile, the controls at 0): the sink keeps them
     * for the preparation, nothing reaches the ring. */
    reg_ring_sink(&out_b, LIVE_REG_CHAR(0), profile);
    for (uint32_t f = 1; f < LIVE_CHAR_FIELDS; f++) {
        reg_ring_sink(&out_b, (uint8_t)LIVE_REG_CHAR(f), 0);
    }
    b_model = (uint8_t)model;
    b_profile = profile;
    b_gate_reset = gate_reset;
    b_made = 0;
}

void audio_chip_b_tick(void) {
    if (b_made || !reg_ring_prepare_step(&out_b, b_profile)) {
        return;
    }
    reg_ring_sink(&out_b, LIVE_REG_RESET, b_model);
    reg_ring_sink(&out_b, LIVE_REG_GATE_RESET, b_gate_reset);
    b_made = 1;
}

void audio_chip_b_use(int on) {
    b_want = on ? 1u : 0u;
}

void audio_tap_chip(uint32_t chip) {
    tap_chip = chip ? 1u : 0u;
}

scope* audio_scope(void) {
    return &tap;
}

void* audio_sink_ctx(void) {
    return &out;
}

void audio_commit(void) {
    reg_ring_commit(&ring);
    reg_ring_pass_end(&chips); /* one slice of a chip's preparation a pass */
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
    sid* a = reg_ring_chip_of(&chips, 0);
    sid* b = reg_ring_chip_of(&chips, 1);
    /* Chip B is in use only once its boot reset has made it (a zeroed sid has a sample rate
     * of 0, and must never render). */
    uint8_t want = (uint8_t)(b_want && b->sample_rate);
    if (!want && !b_on) {
        /* Chip A alone, as M1: sid_render's samples, and the tap's points while LIVE shows. */
        scope_render(&tap, a, s, AUDIO_HALF_FRAMES);
    } else {
        int16_t sb[AUDIO_HALF_FRAMES];
        if (tap_chip) {
            sid_render(a, s, AUDIO_HALF_FRAMES);
            scope_render(&tap, b, sb, AUDIO_HALF_FRAMES);
        } else {
            scope_render(&tap, a, s, AUDIO_HALF_FRAMES);
            sid_render(b, sb, AUDIO_HALF_FRAMES);
        }
        chip_mix(s, sb, AUDIO_HALF_FRAMES,
                 want == b_on ? CHIP_MIX_FULL : want ? CHIP_MIX_IN : CHIP_MIX_OUT);
        b_on = want;
    }
    /* With every slot off this writes the chip's output to both sides unchanged. */
    sid_slots_process(&slots, s, lr, AUDIO_HALF_FRAMES);
    scope_peaks(&tap, lr, AUDIO_HALF_FRAMES);
    for (uint32_t i = 0; i < 2u * AUDIO_HALF_FRAMES; i++) {
        buf[i] = (int32_t)lr[i] * 256;
    }
}
