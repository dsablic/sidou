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
#include "demo_song.h"
#if defined(SIDOU_FLASH_SPIKE)
#include "flash_spike.h"
#endif
#include "input.h"
#include "keys_route.h"
#include "lcd.h"
#include "live.h"
#include "midi.h"
#include "panel.h"
#include "player/song_ctl.h"
#include "reg_ring.h"
#include "song/gt2_sng.h"
#include "ui.h"
#include "ui_sound.h"
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
/* live_tick catches up at most this many ticks in one pass (reg_ring.h's worst pass). A pass
 * runs at most one slice of the spare chip's preparation however many ticks it catches up
 * (reg_ring_pass_end, from audio_commit at the pass's end): two would be 2 x 40k on top of a
 * view's 68k, past the 120k poll allowance. */
#define TICK_CATCH_UP 2u

static int32_t abuf[2u * AUDIO_HALF_WORDS] __attribute__((aligned(4)));
/* Survives resets (.noinit): boots that die early in a row end in UBOOT (app/bootguard.h). */
static bootguard boot_guard __attribute__((section(".noinit")));
static volatile uint32_t uptime_ms;
static live instrument;
/* Which chip the keys play, and the two chips' setups (app/keys_route.h). */
static keys_route keys_router;
/* The UI sounds on chip B's spare voice, off by default (app/ui_sound.h). */
static ui_sound ui_sounds;
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

/* --- The song (M2c Task 6): SIDOU's first song, PLAY plays and stops it until M2d's
 * transport. The keys share chip A with it until M2b Task 4 moves them to chip B. --- */

/* The first song (app/demo_song.h) from flash into the song's region: the audio interrupt never
 * reads flash. */
static uint32_t read_demo(void* ctx, uint32_t offset, uint8_t* buf, uint32_t len) {
    (void)ctx;
    if (offset >= demo_song_sng_bytes) {
        return 0;
    }
    uint32_t n = demo_song_sng_bytes - offset;
    n = n < len ? n : len;
    for (uint32_t i = 0; i < n; i++) {
        buf[i] = demo_song_sng[offset + i];
    }
    return n;
}

/* The song's chip A (song_chip_hooks, app/keys_route.h's contract). Called a poll at a time
 * from PLAY on: once the keys have left chip A (or KEYS is A), the song's setup (its CHPA, or
 * the keys' with AUTHENTIC envelopes, decision record 0005) is what chip A should have
 * (keys_route_song_wants, the one writer of song_want: keys_route's tick brings chip A
 * there, preparing it in the background, as it does the other chip for the keys); the song
 * starts once chip A has it. M2c Task 8's save writes song_want back into the CHPA. */
static uint8_t song_setup_on; /* chip A's setup for the song is asked for */
static chip_setup song_want;

static int song_chip_ready(void* ctx, const song_chip_setup* c) {
    keys_route* kr = ctx;
    if (!keys_route_ready_for_song(kr)) {
        return 0;
    }
    if (!song_setup_on) {
        song_want = kr->ctl[0].has; /* chip A as the song finds it (the keys' setup) */
        if (c) {
            song_want.profile = c->profile;
            song_want.controls[0] = c->age;
            song_want.controls[1] = c->dac;
            song_want.controls[2] = c->thump;
            song_want.controls[3] = c->bleed;
            song_want.gate_reset = c->envelope == 0u; /* 0 INSTANT */
        } else {
            song_want.gate_reset = 0u; /* AUTHENTIC */
        }
        keys_route_song_wants(kr, &song_want);
        song_setup_on = 1;
    }
    if (!chip_ctl_has(&kr->ctl[0], &song_want)) {
        return 0;
    }
    song_setup_on = 0;
    return 1;
}

/* Once a stop has reached the interrupt: chip A back to the keys (keys_route takes it back to
 * their setup, and the keys back to it). */
static uint8_t song_mine; /* keys_route_set_song(1) is the transport's */

static void song_chip_back(void* ctx) {
    keys_route_set_song(ctx, 0);
    song_mine = 0;
}

/* In the LCD's reset wait, before anything else: the song's region and the first song in it. */
static void song_load(void) {
    gt2_song* s = audio_song_boot();
    gt2_caps caps = {SONG_MULTISPEED_MAX};
    gt2_read_info info;
    /* The song is SIDOU's own: it always reads (host tests check it does, with these caps). */
    gt2_sng_read(s, read_demo, 0, &caps, &info);
}

static void song_boot(void) {
    song_chip_hooks hooks = {song_chip_ready, song_chip_back, &keys_router};
    song_ctl_set_hooks(audio_song(), &hooks);
}

static uint8_t song_toggle;   /* a PLAY press waiting for the transport to take it */
static int16_t song_error;    /* the refusal last shown */
static uint32_t song_warned;  /* the warnings shown since the play */

/* A refusal or a warning as a toast (the SONG view's hint until it exists, M2c Task 8). */
static void song_tell(song_ctl* sc) {
    if (sc->error != song_error) {
        song_error = sc->error;
        if (song_error == SONG_ERR_MULTISPEED) {
            /* as the reader says it (gt2_sng.c) */
            char t[32] = "SNG: MULTISPEED ";
            uint32_t k = 16u, m = sc->opts.multispeed;
            if (m >= 10u) {
                t[k++] = (char)('0' + m / 10u);
            }
            t[k++] = (char)('0' + m % 10u);
            const char* tail = " (MAX 4)";
            while (*tail) {
                t[k++] = *tail++;
            }
            t[k] = 0;
            ui_toast(t, 2000u);
        } else if (song_error == GT2_PLAY_ERR_STEREO) {
            ui_toast("SNG: STEREO, NOT PLAYED", 2000u);
        } else if (song_error != 0) {
            ui_toast("SNG: NOTHING TO PLAY", 2000u);
        }
    }
    if (song_ctl_state(sc) != SONG_PLAYING) {
        return;
    }
    uint32_t w = song_ctl_warnings(sc) & ~song_warned;
    if (w & GT2_PLAY_WARN_PITCH) {
        ui_toast("SNG: A-4 PITCH NOT PLAYED", 2000u);
    } else if (w & GT2_PLAY_WARN_PAST_B7) {
        ui_toast("SNG: NOTE PAST B-7", 2000u);
    }
    song_warned |= w;
}

/* fm1-emu's tests only (emu-gate tests/song.rs, "edits while playing"): edit requests a test
 * writes into RAM, which the main loop makes with the song model's primitives and sends as
 * M2d's editor will. op: 0 none, 1 a row write (a: pattern, b: row, cell), 2 a row inserted
 * and 3 a row deleted (a rebuild: a swap); the main loop clears op once the edit is sent and
 * records the song's block count then (the edit applies at the next block). Nothing on the
 * device writes it. */
typedef struct {
    volatile uint32_t op;
    uint32_t a, b;
    uint8_t cell[4];
    volatile uint32_t block;
} song_test_req;
#define SONG_TEST_REQS 4u
song_test_req song_test_hook[SONG_TEST_REQS];

static void song_test_edits(song_ctl* sc) {
    for (uint32_t i = 0; i < SONG_TEST_REQS; i++) {
        song_test_req* r = &song_test_hook[i];
        if (r->op == 0u) {
            continue;
        }
        gt2_edit e;
        gt2_err err = GT2_ERR_RANGE;
        if (r->op == 1u) {
            err = gt2_set_row(sc->song, r->a, r->b, r->cell, &e);
        } else if (r->op == 2u) {
            err = gt2_pattern_insert_row(sc->song, r->a, r->b, &e);
        } else if (r->op == 3u) {
            err = gt2_pattern_delete_row(sc->song, r->a, r->b, &e);
        }
        if (err == GT2_ERR_PENDING) {
            return; /* in order: this one again next pass */
        }
        if (err == GT2_OK && !song_ctl_edit(sc, &e)) {
            gt2_edit_cancel(sc->song, &e);
            return;
        }
        r->block = sc->blocks;
        r->op = 0u;
    }
}

/* Once a pass: PLAY's presses, then the transport's poll (a slice of its analysis); the
 * voices the keys take from the song under KEYS A. */
static uint8_t song_taken;

static void song_pass(void) {
    song_ctl* sc = audio_song();
    if (panel_take_play(&front) & 1u) {
        song_toggle ^= 1u;
    }
    if (song_toggle) {
        uint32_t st = song_ctl_state(sc);
        if (st == SONG_PLAYING) {
            song_toggle = !song_ctl_stop(sc);
        } else if (st != SONG_LOADING && song_ctl_play(sc, 0, 0)) {
            song_toggle = 0;
            song_warned = 0;
            song_setup_on = 0;
            if (song_ctl_state(sc) == SONG_LOADING) {
                keys_route_set_song(&keys_router, 1); /* the keys leave chip A */
                song_mine = 1;
            }
        }
    }
    uint8_t taken = keys_route_voices_taken(&keys_router);
    if (taken != song_taken && song_ctl_take(sc, taken)) {
        song_taken = taken;
    }
    song_test_edits(sc);
    audio_song_poll();
    uint32_t now_st = song_ctl_state(sc);
    if (song_mine && (now_st == SONG_IDLE || now_st == SONG_STOPPED) && !sc->handback) {
        keys_route_set_song(&keys_router, 0); /* stopped, or a refused play: the keys come back */
        song_mine = 0;
    }
    song_tell(sc);
    uint32_t st = song_ctl_state(sc);
    front.playing = st == SONG_PLAYING || st == SONG_LOADING;
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
        ui_sound_tick(&ui_sounds);
    }
    song_pass();
    if (tick_due) {
        /* Once a pass, after its live ticks: the keys' chip has had its turn, then the other
         * chip follows the keys' setup (chip B's first reset at boot among it) and the keys
         * move. */
        keys_route_tick(&keys_router);
        audio_chip_b_use(AUDIO_B_UI, ui_sound_busy(&ui_sounds)); /* chip B in use for a sound */
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

#if defined(SIDOU_FLASH_SPIKE)
/* The flash spike's build only (flash_spike.h): what its waits run, the main loop's services
 * but play and the screen, so the normal installer reaches it at any time but mid-measurement. */
static void spike_service(void) {
    uint32_t now = uptime_ms;
    fm1_wdt_feed();
    midi_service(now);
    bootguard_poll(&boot_guard, now);
    if (update_poll()) {
        upgrade();
    }
}
#endif

static void sidou_main(void) {
    /* Interrupts are still off: lcd_init and input_init write port A, which carries PA2. */
    lcd_init(song_load);
    audio_init();
    /* The keys play chip A, as in M1; chip B is made with the keys' setup from the first
     * ticks, prepared in the background (its first reset at boot only: app/reg_ring.h). */
    keys_route_init(&keys_router, &instrument, reg_ring_sink, reg_ring_prepare_step,
                    audio_sink_ctx(), audio_sink_ctx_b());
    keys_router.moved = audio_keys;
    live_init(&instrument, keys_route_sink, &keys_router, SID_MODEL_6581);
    /* From here on a profile or model change prepares the spare chip a slice a tick. */
    live_set_preparer(&instrument, keys_route_prepare, &keys_router);
    keys_route_start(&keys_router);
    ui_sound_init(&ui_sounds, &instrument, reg_ring_sink, audio_sink_ctx_b());
    song_boot();
    panel_init(&front);
    input_init();
    panel_master(&front, &instrument, input_master());
    midi_init(&instrument);
    update_boot();
#if defined(SIDOU_FLASH_SPIKE)
    /* Not SIDOU: the flash spike, with the audio never started (flash_spike.h). The views are
     * set up only for the update protocol's progress frame (ui_update). */
    fm1_timer5_start(isr_timer5, PRIO_TIMER5);
    fm1_irq_enable_all();
    ui_init(&instrument, uptime_ms, 0);
    flash_spike_run(spike_service);
#endif
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
    ui_set_keys(&keys_router);
    ui_set_sounds(&ui_sounds);
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
