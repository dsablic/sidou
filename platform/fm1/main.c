// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU boot and main loop. Init order after fm1-sid's firmware/main.c (fm1-emu) and
 * Felucca's src/main.c: watchdog first, fatal vectors, memory, then the LCD, the live
 * instrument and the key matrix, audio (ALNK0 IRQ) and the 10 kHz tick (TIMER5 IRQ),
 * interrupts on, and a main loop that plays the panel into the instrument, redraws the
 * screen and feeds the watchdog.
 *
 * This file includes the HAL's interrupt, audio and timer headers; input.c and lcd.c include
 * the input and LCD ones (platform/fm1/VENDORED.md). */
#include <stdint.h>

#include "fm1_sys.h" /* before fm1_irq.h: FM1_RESET_PREP */

#include "fm1_audio.h"
#include "fm1_irq.h"
#include "fm1_time.h"
#include "fm1_timer.h"

#include "audio.h"
#include "input.h"
#include "lcd.h"
#include "live.h"
#include "panel.h"
#include "reg_ring.h"
#include "ui.h"

extern uint32_t _data_start[], _data_end[], _data_load[], _bss_start[], _bss_end[];
extern uint32_t _pool_start[], _pool_end[], _rt_start[], _rt_end[], _rt_load[];
typedef void (*ctor_fn)(void);
extern ctor_fn __init_array_start[], __init_array_end[];

/* The IRQ entry wrappers (hal/fm1_isr.S) and the C bodies they call. */
void isr_alnk0(void);
void isr_timer5(void);
void fm1_alnk0_irq(void);
void fm1_timer5_irq(void);
void fm1_cstart(void);

/* Priorities: TIMER5 above ALNK0, so the tick keeps its pace while a half buffer renders. */
#define PRIO_ALNK0 3u
#define PRIO_TIMER5 4u
/* fm1_wdt_arm: 0x0A is 1 s, each step doubles it. */
#define WDT_TIMEOUT 0x0Du
#define TICK_MS (1000u / LIVE_TICK_HZ)
/* live_tick catches up at most this many ticks in one pass (reg_ring.h's worst pass). */
#define TICK_CATCH_UP 2u

static int32_t abuf[2u * AUDIO_HALF_WORDS] __attribute__((aligned(4)));
static volatile uint32_t uptime_ms;
static live instrument;
static panel front;

void fm1_alnk0_irq(void) {
    uint8_t p = fm1_audio_pending();
    fm1_audio_ack_aux(p);
    if (p & FM1_AUDIO_HALF) {
        audio_fill(&abuf[fm1_audio_free_half() * AUDIO_HALF_WORDS]);
        fm1_audio_ack_half();
    }
}

void fm1_timer5_irq(void) {
    static uint32_t last, acc;
    uint32_t now = fm1_ticks();
    fm1_timer5_ack();
    input_tick_isr();
    acc += now - last;
    last = now;
    while (acc >= 1000u * FM1_TICKS_PER_US) {
        acc -= 1000u * FM1_TICKS_PER_US;
        uptime_ms++;
    }
}

/* fm1_fault_c (fm1_irq.h) has already recorded the crash in fm1_crash (.noinit), where the
 * next boot can find it. Stop the DMA so the buffer does not loop, then reset. */
static void fm1_fault(const fm1_crash_t* c) {
    (void)c;
    fm1_audio_stop();
    fm1_reboot();
}

/* One main-loop pass: the panel into the instrument, the 100 Hz work, then the queued
 * register writes go to the audio interrupt in one batch. */
static void play(uint32_t now, uint32_t* next_tick) {
    input_poll(&front, &instrument);
    for (uint32_t n = 0; n < TICK_CATCH_UP && (int32_t)(now - *next_tick) >= 0; n++) {
        *next_tick += TICK_MS;
        panel_master(&front, &instrument, input_master());
        live_tick(&instrument);
    }
    if ((int32_t)(now - *next_tick) >= 0) {
        *next_tick = now + TICK_MS; /* too far behind: skip, the LFO only slows down */
    }
    audio_commit();
    input_leds(&front, &instrument);
}

static void sidou_main(void) {
    /* Interrupts are still off: lcd_init and input_init write port A, which carries PA2. */
    lcd_init();
    audio_init();
    live_init(&instrument, reg_ring_sink, audio_sink_ctx(), SID_MODEL_6581);
    panel_init(&front);
    input_init();
    panel_master(&front, &instrument, input_master());
    audio_start();
    ui_init(&instrument);
    fm1_audio_init(abuf, AUDIO_HALF_WORDS, isr_alnk0, PRIO_ALNK0);
    fm1_timer5_start(isr_timer5, PRIO_TIMER5);
    fm1_irq_enable_all();
    uint32_t next_tick = uptime_ms + TICK_MS;
    for (;;) {
        fm1_wdt_feed();
        uint32_t now = uptime_ms;
        play(now, &next_tick);
        ui_poll(&instrument, now);
        /* Sleep until the next interrupt: saves power, and fm1-emu skips ahead. */
        __asm__ volatile("idle");
    }
}

void fm1_cstart(void) {
    uint32_t *d, *s;
    fm1_time_init();
    fm1_wdt_arm(WDT_TIMEOUT);
    fm1_irq_init();
    for (d = _bss_start; d < _bss_end; d++) {
        *d = 0;
    }
    for (d = _pool_start; d < _pool_end; d++) {
        *d = 0;
    }
    for (s = _data_load, d = _data_start; d < _data_end; s++, d++) {
        *d = *s;
    }
    for (s = _rt_load, d = _rt_start; d < _rt_end; s++, d++) {
        *d = *s;
    }
    for (ctor_fn* c = __init_array_start; c < __init_array_end; c++) {
        (*c)();
    }
    fm1_mailbox_clear();
    sidou_main();
}
