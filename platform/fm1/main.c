// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU boot and main loop. Init order after fm1-sid's firmware/main.c (fm1-emu) and
 * Felucca's src/main.c: watchdog first, fatal vectors, memory, then the LCD, the live
 * instrument, the key matrix and MIDI in (TRS UART, USB), audio (ALNK0 IRQ) and the 10 kHz
 * tick (TIMER5 IRQ, which also polls MIDI), interrupts on, the screen's first frame (sent by
 * the main loop), and a main loop that plays MIDI and the panel into the instrument, redraws
 * the screen, answers the update protocol and feeds the watchdog. The boot-loop guard comes first of all (fm1_cstart).
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
#include "bootguard.h"
#include "input.h"
#include "lcd.h"
#include "live.h"
#include "midi.h"
#include "panel.h"
#include "reg_ring.h"
#include "ui.h"
#include "update.h"

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
/* Survives resets (.noinit): boots that die early in a row end in UBOOT (app/bootguard.h). */
static bootguard boot_guard __attribute__((section(".noinit")));
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

/* Every 5th tick (2 kHz) each, on different ticks, as Felucca does: USB on tick 0, the TRS
 * UART on tick 2. Nested in the audio ISR too: neither touches what the render does. */
#define MIDI_POLL_TICKS 5u

void fm1_timer5_irq(void) {
    static uint32_t last, acc, sub;
    uint32_t now = fm1_ticks();
    fm1_timer5_ack();
    input_tick_isr();
    if (sub == 0u) {
        midi_usb_isr();
    } else if (sub == 2u) {
        midi_uart_isr();
    }
    if (++sub == MIDI_POLL_TICKS) {
        sub = 0;
    }
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
 * register writes go to the audio interrupt in one batch. The panel only changes when the
 * key scan finishes a frame (~1.1 ms), so a wake-up from the 10 kHz tick in between does
 * nothing here. */
static void play(uint32_t now, uint32_t* next_tick, uint32_t* last_frame) {
    uint32_t frame = input_frame();
    int tick_due = (int32_t)(now - *next_tick) >= 0;
    if (frame == *last_frame && !tick_due) {
        return;
    }
    *last_frame = frame;
    input_poll(&front, &instrument);
    for (uint32_t n = 0; n < TICK_CATCH_UP && (int32_t)(now - *next_tick) >= 0; n++) {
        *next_tick += TICK_MS;
        panel_master(&front, &instrument, input_master());
        live_tick(&instrument);
    }
    if (tick_due) {
        midi_send(); /* once a pass at most, so at most once a tick */
    }
    if ((int32_t)(now - *next_tick) >= 0) {
        *next_tick = now + TICK_MS; /* too far behind: skip, the LFO only slows down */
    }
    audio_commit();
    input_leds(&front, &instrument);
}

#if defined(SIDOU_TEST_DEATH_MS)
/* Hardware-checklist builds only (docs/hardware-test.md; CMake SIDOU_TEST_DEATH): with OCT-
 * held at power-on, die SIDOU_TEST_DEATH_MS after boot as an early crash would, before the
 * boot is stable, so the boot guard sends the third such boot to UBOOT. Without the key the
 * build runs as the normal one, so the normal installer always reaches it. A hang with
 * interrupts off lets the watchdog reset the chip; SIDOU_TEST_DEATH_REBOOT resets it through
 * P33 as the fault handler does. */
static uint8_t test_death_armed;

/* Right after interrupts are on: the key scan needs a few frames to debounce (as Felucca reads
 * its power-on combinations). */
static void test_death_arm(void) {
    fm1_delay_ms(30);
    test_death_armed = (input_buttons() >> PANEL_BTN_OCT_DOWN) & 1u;
}

static void test_death(uint32_t now) {
    if (!test_death_armed || now < SIDOU_TEST_DEATH_MS) {
        return;
    }
#if SIDOU_TEST_DEATH_REBOOT
    fm1_reboot();
#else
    __asm__ volatile("cli" ::: "memory");
    for (;;) {
    }
#endif
}
#endif

/* Before an intentional reset: not a failed boot. */
static void leave_on_purpose(void) {
    bootguard_leave(&boot_guard);
}

/* The host asked to upgrade (update.h): the audio stops for good, step 1 runs, and on success
 * the chip restarts into the update loader. If nothing was committed (refused, failed), the
 * firmware restarts the same way it booted. */
static void upgrade(void) {
    fm1_audio_stop();
    audio_halted();
    update_session(leave_on_purpose);
    leave_on_purpose();
    fm1_reboot();
}

static void sidou_main(void) {
    /* Interrupts are still off: lcd_init and input_init write port A, which carries PA2. */
    lcd_init();
    audio_init();
    live_init(&instrument, reg_ring_sink, audio_sink_ctx(), SID_MODEL_6581);
    /* From here on a profile or model change prepares the spare chip a slice a tick. */
    live_set_preparer(&instrument, reg_ring_prepare_step, audio_sink_ctx());
    panel_init(&front);
    input_init();
    panel_master(&front, &instrument, input_master());
    midi_init(&instrument);
    update_boot();
    audio_start();
    /* Sound first: the audio interrupt, the tick (the key scan, MIDI, the clock) and the
     * interrupts start before the screen's first frame, which the main loop then sends without
     * waiting on it (ui_init). */
    fm1_audio_init(abuf, AUDIO_HALF_WORDS, isr_alnk0, PRIO_ALNK0);
    fm1_timer5_start(isr_timer5, PRIO_TIMER5);
    fm1_irq_enable_all();
    audio_irqs_on();
    /* HOME held at power-on plays the full boot (app/boot.h): its level once the key scan
     * has debounced it, about 3 ms after the tick started; the audio already runs. */
    uint32_t held = input_buttons_at_power_on();
    /* uptime_ms counts every timer tick since TIMER4 started (fm1_timer5_irq). */
    ui_init(&instrument, uptime_ms, (int)((held >> PANEL_BTN_HOME) & 1u));
#if defined(SIDOU_TEST_DEATH_MS)
    test_death_arm();
#endif
    uint32_t next_tick = uptime_ms + TICK_MS;
    uint32_t last_frame = input_frame();
    for (;;) {
        fm1_wdt_feed();
        uint32_t now = uptime_ms;
        if (midi_drain()) {
            audio_commit();
        }
        play(now, &next_tick, &last_frame);
        ui_poll(now);
        midi_service(now);
        bootguard_poll(&boot_guard, now);
#if defined(SIDOU_TEST_DEATH_MS)
        test_death(now);
#endif
        if (update_poll()) {
            upgrade();
        }
        /* Sleep until the next interrupt: saves power, and fm1-emu skips ahead. */
        __asm__ volatile("idle");
    }
}

void fm1_cstart(void) {
    uint32_t *d, *s;
    fm1_time_init();
    fm1_wdt_arm(WDT_TIMEOUT);
    /* First, before anything that may crash: the third boot in a row that dies before it is
     * stable goes to UBOOT, which still accepts updates (not the 7D soft key: a direct entry). */
    if (bootguard_boot(&boot_guard)) {
        /* The watchdog armed above would reset the chip out of UBOOT while a host installs. */
        fm1_wdt_stop();
        fm1_enter_uboot();
    }
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
