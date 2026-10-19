// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI in (midi.h) and the update protocol's step 1 (update.h). This file includes Felucca's
 * USB stack (src/usb.c, as it is but for two marked hooks), its update protocol (src/ota.c) and
 * SIDOU's version of its TRS UART driver (usb/midi_uart.c), all GPL-3.0-only: like the HAL, they
 * are static code for one translation unit, and they call back into the hooks defined here
 * before them; ota.c talks through usb.c's static SysEx plumbing. Only this file includes fm1_usb.h, fm1_uart.h and
 * fm1_flash.h (platform/fm1/VENDORED.md).
 *
 * This file is compiled as Felucca compiles its firmware, not with SIDOU's flags
 * (CMakeLists.txt, SIDOU_USB_FIX): built with -mcpu=r3 -O2, usb.c's SIE accesses run too close
 * together and EP1 IN stops on the FM-1 (docs/hardware-results/usb-out-diagnosis.md). */
#include <stdint.h>

#include "fm1_sys.h"
#include "fm1_time.h"
/* The app may erase and program the update staging area (ota.c OTA_AREA, 0xE0000..0xE5000,
 * which holds the update record too) and the store (app/store_layout.h, 0x97000..0xE0000;
 * docs/hardware.md, "Flash map"), nothing else. The update protocol's hooks (ota_flash.h)
 * check the staging area alone; the store's own hooks check its guard band too. */
#include "store_layout.h"
#define FL_RANGE_OK(off, n) (FL_IN(off, n, FL_OTA_LO, FL_OTA_HI) || FL_IN(off, n, STORE_LO, STORE_HI))
#include "fm1_flash.h"

#include "audio.h"
#include "input.h"
#include "midi.h"
#include "midi_out.h"
#include "midi_ring.h"
#include "midi_rx.h"
#include "reg_ring.h"
#include "ui.h"
#include "update.h"
#include "store_fm1.h"

/* The register writes one MIDI entry can cause at most. A program change writes every register
 * (live_set_preset, 25). A CC writes at most 6 (the pulse width or the envelopes on every
 * voice, the latter also for the velocity target): the chip model is only recorded here and
 * live_tick applies it, inside the main loop's own budget (reg_ring.c, REG_RING_WORST_PASS), as
 * it applies pitch bend, aftertouch, the wheel on a target other than the pulse width and RPN 0,
 * which write nothing here, so a flood of 14-bit bends costs one tick's writes. A note-on writes
 * at most 11 (reg_ring.c). A TRS byte completes at most one message; a USB packet up to three,
 * through running status (app/midi_usb.h), e.g. CIN 3 with three data bytes after C0, so three
 * program changes (75) are the worst; two CCs fit in a packet (12), three note-ons (33). An
 * entry after a gap of lost input first restarts the parsers and releases every voice
 * (live_all_notes_off: one gate write per voice): 75 + 3 = 78. */
#define MIDI_ENTRY_WRITES (3u * 25u + LIVE_VOICES)
/* After the last entry, midi_rx_check_lost may find a gap that came behind it and release
 * every voice again: 3 writes beyond any entry's. */
#define MIDI_LOST_WRITES LIVE_VOICES
/* The drain takes an entry only while this many are free after it (1024 - 530 = 494 queued
 * at most), so the rest of the pass never waits for the audio ISR on its own account. */
#define MIDI_DRAIN_RESERVE (MIDI_ENTRY_WRITES + MIDI_LOST_WRITES + REG_RING_WORST_PASS)
_Static_assert(MIDI_DRAIN_RESERVE < REG_RING_SIZE, "the ring leaves MIDI no room");

/* MIDI out's settings until M2 stores settings (CMake: SIDOU_MIDI_OUT_CHANNEL, 1..16, and
 * SIDOU_MIDI_OUT_NRPN; docs/reference/midi.md, "Output"). */
#ifndef SIDOU_MIDI_OUT_CHANNEL
#define SIDOU_MIDI_OUT_CHANNEL 1
#endif
#ifndef SIDOU_MIDI_OUT_NRPN
#define SIDOU_MIDI_OUT_NRPN 0
#endif
_Static_assert(SIDOU_MIDI_OUT_CHANNEL >= 1 && SIDOU_MIDI_OUT_CHANNEL <= 16,
               "SIDOU_MIDI_OUT_CHANNEL is 1..16");

static midi_ring in_ring;
static midi_rx rx;
/* MIDI learn's bindings (176 bytes, app/midi_learn.h); read by the emulator tests through the
 * ELF symbol. In POOL (zeroed at boot like .bss): the RAM region keeps its 4 KiB free with
 * Felucca's usb.c and its own MIDI-in ring (emu-gate tests/song.rs). */
static midi_learn learned __attribute__((used, section(".pool")));
/* MIDI's mode (app/midi_route.h); read by the emulator tests through the ELF symbol. */
static midi_route routing __attribute__((used));
/* MIDI clock and transport (app/midi_sync.h); read by the emulator tests through the ELF
 * symbol. Its player is bound by main.c (midi_synced). */
static midi_sync syncing __attribute__((used));
/* Read by the emulator tests through the ELF symbol (its counters, app/midi_out.h). */
static midi_out midi_tx __attribute__((used));
/* fm1_ticks() at the start of the poll that is running: every entry it pushes gets it. */
static uint32_t poll_stamp;

static sysex sidou_sysex __attribute__((used));
uint8_t midi_sysex_running;
uint8_t midi_sysex_writing;
static const volatile uint32_t* sysex_ms;
static uint8_t sidou_outstanding;

static void sidou_trs(void* ctx, midi_sysex_event event, uint8_t byte) {
    ((midi_rx*)ctx)->inputs++;
    (void)event;
    sysex_byte(&sidou_sysex, SYSEX_TRS, byte, *sysex_ms);
    midi_sysex_running = (uint8_t)sysex_active(&sidou_sysex);
    store_fm1_active |= midi_sysex_running;
}

static void sidou_usb(void* ctx, midi_sysex_event event, uint8_t byte) {
    ((midi_rx*)ctx)->inputs++;
    (void)event;
    sysex_byte(&sidou_sysex, SYSEX_USB, byte, *sysex_ms);
    midi_sysex_running = (uint8_t)sysex_active(&sidou_sysex);
    store_fm1_active |= midi_sysex_running;
}

static void sidou_callbacks(void) {
    rx.trs.on_sysex = sidou_trs;
    rx.usb.on_sysex = sidou_usb;
}

/* Diagnostics, read by the emulator tests through the ELF symbol: six uint32_t, in this order. */
static struct {
    uint32_t entries;        /* ring entries taken onto live */
    uint32_t lost;           /* midi_ring_lost at the last drain that saw a loss */
    uint32_t restarts;       /* parser restarts after lost input */
    uint32_t max_wait_ticks; /* the longest an entry waited in the ring, 24 MHz ticks */
    uint32_t usb_polls;      /* USB polls with the controller up */
    uint32_t uart_polls;
} midi_stats __attribute__((used));

/* ---- the hooks of the vendored code ---- */

/* The ring's index stores are ordered by midi_ring.c itself; this orders usb.c's own rings. */
#define RING_PUBLISH() __asm__ volatile("" ::: "memory")
/* The update protocol's SysEx frames and TX queue in usb.c (ota.c below). usb.c also sets
 * usb.uboot_req on the stock UBOOT soft key F0 22 24 35 7D F7; nothing here reads it, and SIDOU
 * never sends it. */
#define FELUCCA_OTA 1
/* Felucca's build switches as its default firmware leaves them, but without the CDC console and
 * the USB audio input: MIDI streaming alone on EP1. */
#define FELUCCA_CDC 0
#define FELUCCA_UAC 0
/* The identity the protocol answers (cmd 0x11), == the package's (sidou-pack --product):
 * "FM-1_900" for development builds, as every community firmware's; CMakeLists.txt. */
#define FELUCCA_ID SIDOU_PRODUCT

/* usb_start's wait after the controller's reset (usb.c's only fm1_delay_ms): 25 ms, unless
 * usb_retry_quiet below has already held the controller in reset that long. */
static uint8_t usb_reset_waited;

static void usb_reset_wait(uint32_t ms) {
    if (!usb_reset_waited) {
        fm1_delay_ms(ms);
    }
}

/* usb.c's two SIDOU hooks (platform/fm1/VENDORED.md). The device presents as "SIDOU" by
 * "SIDOU contributors" (string descriptors: bLength, 3, UTF-16LE). */
#define FELUCCA_USB_MANUFACTURER                                                                   \
    38, 3, 'S', 0, 'I', 0, 'D', 0, 'O', 0, 'U', 0, ' ', 0, 'c', 0, 'o', 0, 'n', 0, 't', 0, 'r', 0, \
        'i', 0, 'b', 0, 'u', 0, 't', 0, 'o', 0, 'r', 0, 's', 0
#define FELUCCA_USB_PRODUCT 12, 3, 'S', 0, 'I', 0, 'D', 0, 'O', 0, 'U', 0
/* Every USB-MIDI event packet, as it came (TIMER5 interrupt, usb_poll), into SIDOU's ring:
 * SIDOU's parser (app/midi_usb.c) takes system common, every real-time byte and SysEx too,
 * which usb.c's own ring leaves out. A full ring is a gap for the parser. */
static void usb_raw_in(uint32_t pkt) {
    if (!midi_ring_push(&in_ring, MIDI_SRC_USB, pkt, poll_stamp)) {
        midi_ring_lose(&in_ring, 1);
    }
}
#define FELUCCA_MIDI_IN_RAW usb_raw_in

/* usb.c's SysEx assembler is renamed: app/sysex.h has a sysex_byte too. Its one delay goes
 * through usb_reset_wait. */
/* clang-format off */
#define sysex_byte ota_sysex_byte
#define fm1_delay_ms usb_reset_wait
#include "src/usb.c"
#undef sysex_byte
#undef fm1_delay_ms
/* clang-format on */

/* usb.c's usb_retry from the main loop, without stopping it. While no host answers (no cable:
 * the SIE stays silent), usb_retry would run usb_start once a second: the controller's reset, a
 * 25 ms wait, the attach and six SIE writes that each time out at 20000 polls, about 28 ms of
 * main loop, and MIDI from the TRS jack waiting with it. Instead, a step a pass:
 * - once a second, the controller into reset (fm1_usb_reset, usb_start's first step);
 * - 25 ms on, a probe: the attach (fm1_usb_attach) and one SIE write (S_POWER, usb_start's
 *   first), given USB_PROBE_POLLS polls to finish, then the reset again. A silent SIE ends the
 *   try there;
 * - an SIE that answered: 25 ms more in reset, then usb_retry, whose usb_start resets (the same
 *   three register writes) and attaches without the wait (usb_reset_wait).
 * The probe's attach lasts microseconds, far below a host's attach debounce. */
#define USB_PROBE_POLLS 2000u

/* The probe: 1 when the SIE answered. Its own function, so the emulator tests find the main
 * loop's longest USB step by its symbol (tests/latency.rs). */
__attribute__((noinline)) static int usb_probe(void) {
    uint32_t n = USB_PROBE_POLLS;
    fm1_usb_attach(ep0buf);
    fm1_usb_sie_wr_start(S_POWER, 0x60u);
    while (!fm1_usb_sie_done() && --n) {
    }
    fm1_usb_reset();
    return n != 0;
}

static void usb_retry_quiet(uint32_t now_ms) {
    static uint32_t t;
    static uint8_t step; /* 0 idle, 1 reset before the probe, 2 reset before the attach */
    if (usb.up || usb.detached) {
        step = 0;
        return;
    }
    if (step == 0) {
        if (now_ms - t < 1000u) {
            return; /* usb_retry's own pace */
        }
        t = now_ms;
        fm1_usb_reset();
        step = 1;
        return;
    }
    if (now_ms - t <= 25u) {
        return; /* > 25 whole ms: at least 25 ms */
    }
    t = now_ms;
    if (step == 1) {
        step = usb_probe() ? 2 : 0;
        return;
    }
    step = 0;
    usb_reset_waited = 1;
    usb_retry(now_ms); /* its last call was at least 1 s ago */
    usb_reset_waited = 0;
}

/* A TRS byte into SIDOU's ring (source 2), stamped by its poll. 0 when the ring is full.
 * midi_uart.c calls midi_enqueue and midi_lost, renamed: usb.c has its own midi_enqueue, the
 * entry into its own MIDI-in ring. */
static int trs_enqueue(uint32_t data, uint32_t source) {
    return midi_ring_push(&in_ring, (uint8_t)source, data, poll_stamp);
}

static void trs_lost(uint32_t n) {
    midi_ring_lose(&in_ring, n);
}

/* clang-format off */
#define midi_enqueue trs_enqueue
#define midi_lost trs_lost
#include "usb/midi_uart.c"
#undef midi_enqueue
#undef midi_lost
#include "src/ota.c"
/* clang-format on */

_Static_assert(MIDI_SRC_USB == 1u && MIDI_SRC_TRS == 2u, "USB packets are 1, TRS bytes 2");

/* usb.c's own MIDI-in ring (channel messages and four real-time bytes, from its filter) holds
 * copies of what usb_raw_in already gave SIDOU's ring: emptied here (main loop), but only while
 * SIDOU's ring has room for another OUT packet. So usb.c's back-pressure, which NAKs the host
 * while its own ring is nearly full (ep1_take), follows SIDOU's ring when that one fills. */
static void usb_in_pump(void) {
    if (mi_r == mi_w && !midi_in_overflow) {
        return;
    }
    if (midi_ring_room(&in_ring) >= EP1_ROOM) {
        uint32_t f = irq_save();
        mi_r = mi_w;
        midi_in_overflow = 0;
        irq_restore(f);
    }
}

static int sidou_send(void* ctx, const uint8_t packet[4]) {
    (void)ctx;
    if (!midi_usb_live() || mo_w - mo_r >= MQ || so_w != so_r || sx_busy) {
        return 0;
    }
    uint32_t word = (uint32_t)packet[0] | (uint32_t)packet[1] << 8 |
                    (uint32_t)packet[2] << 16 | (uint32_t)packet[3] << 24;
    midi_out_event(word);
    sidou_outstanding = 1;
    return 1;
}

static void sidou_writing(void* ctx, int on) {
    (void)ctx;
    midi_sysex_writing = (uint8_t)(on != 0);
}

void midi_sysex_bind(const sysex_hooks* hooks, const volatile uint32_t* now_ms) {
    sysex_hooks bound = *hooks;
    bound.send = sidou_send;
    bound.writing = sidou_writing;
    sysex_ms = now_ms;
    static const uint8_t version[4] = {SIDOU_VERSION_MAJOR, SIDOU_VERSION_MINOR,
                                      SIDOU_VERSION_PATCH, 0};
    sysex_init(&sidou_sysex, store_fm1(), &bound, version, store_fm1_window != 0);
}

void midi_sysex_service(uint32_t now_ms) {
    if (!midi_sysex_running) {
        return;
    }
    if (sidou_sysex.owner == SYSEX_USB && !midi_usb_live()) {
        sysex_reset(&sidou_sysex);
        if (!sysex_active(&sidou_sysex)) {
            midi_sysex_running = 0;
            return;
        }
    }
    sidou_sysex.windows = store_fm1_window != 0;
    sysex_poll(&sidou_sysex, now_ms);
    midi_sysex_running = (uint8_t)sysex_active(&sidou_sysex);
}

/* ---- midi.h ---- */

void midi_init(live* l, keys_route* kr) {
    midi_ring_init(&in_ring);
    midi_rx_init(&rx, l, midi_ring_epoch(&in_ring));
    midi_route_init(&routing, l, kr, NULL); /* the player's side: song control's (midi.h) */
    midi_rx_route(&rx, &routing);
    midi_sync_init(&syncing);
    midi_rx_sync(&rx, &syncing);
    midi_out_init(&midi_tx, l, (uint8_t)(SIDOU_MIDI_OUT_CHANNEL - 1), SIDOU_MIDI_OUT_NRPN != 0);
    midi_rx_watch(&rx, &midi_tx);
    midi_learn_init(&learned);
    midi_rx_learn(&rx, &learned);
    midi_out_learn(&midi_tx, &learned);
    sidou_callbacks();
    uart_midi_init();
    usb_start();
}

midi_learn* midi_learned(void) {
    return &learned;
}

midi_route* midi_routed(void) {
    return &routing;
}

void midi_tick(void) {
    midi_route_tick(&routing);
    midi_sync_pass(); /* the anchor, the follower's rows and its lost clock, 100 times a second */
}

midi_sync* midi_synced(void) {
    return &syncing;
}

/* Called from midi_tick (100 Hz: the anchor, the follower's rows, the lost clock), from
 * midi_drain after a clock or a transport message (the start's alignment), and from main.c's
 * play() while CLOCK OUT is on (its clocks' times, about once a millisecond). */
void midi_sync_pass(void) {
    uint32_t t, smp;
    audio_song_anchor(&t, &smp);
    if (t == 0u && smp == 0u) {
        return; /* no block yet */
    }
    midi_sync_anchor(&syncing, t, smp);
    if (!(syncing.want | syncing.aligning | syncing.fresh | syncing.clock.active |
          syncing.clock_out)) {
        return; /* no clock, no request, nothing to send */
    }
    uint32_t now = fm1_ticks();
    midi_sync_rows(&syncing, routing.setup.rows_per_beat);
    uint32_t pkt[MIDI_SYNC_OUT_MAX];
    uint32_t n = midi_sync_poll(&syncing, now, pkt);
    /* No host, or one asleep: clock out's packets are dropped (midi_send's rule). The
     * update's SysEx shares the ring's room with nothing else: a full ring drops them too. */
    int up = *(volatile uint8_t*)&usb.config && !usb.suspended;
    for (uint32_t i = 0; i < n && up; i++) {
        midi_out_event(pkt[i]);
    }
}

void midi_usb_isr(void) {
    poll_stamp = fm1_ticks();
    if (usb.up) {
        midi_stats.usb_polls++;
    }
    usb_poll();
}

void midi_uart_isr(void) {
    poll_stamp = fm1_ticks();
    midi_stats.uart_polls++;
    uart_midi_poll();
}

uint32_t midi_inputs(void) {
    return rx.inputs;
}

__attribute__((noinline, unused)) static int drain_pending(uint8_t epoch) {
    midi_ring_entry e;
    uint32_t n = 0;
    uint32_t restarts = rx.restarts;
    /* Leave the entry, a later gap and the rest of the pass their worst cases
     * (MIDI_DRAIN_RESERVE); what stays in the ring is taken after the next audio block. */
    while (audio_queued() + MIDI_DRAIN_RESERVE <= REG_RING_SIZE &&
           midi_ring_pop(&in_ring, &e)) {
        uint32_t wait = fm1_ticks() - e.stamp;
        if (wait > midi_stats.max_wait_ticks) {
            midi_stats.max_wait_ticks = wait;
        }
        midi_rx_entry(&rx, &e);
        n++;
    }
    /* MIDI out's second look, once a pass (midi_rx_watch). Since the first, nothing but this
     * loop changed a parameter: midi_drain runs to completion in the main loop, the panel
     * changes parameters only in main.c's play(), after it (ui_poll only reads live), and the
     * interrupts (key scan, MIDI polls, audio render) change none. */
    midi_rx_settle(&rx);
    if (syncing.fresh | syncing.want) {
        midi_sync_pass(); /* a clock or a transport message: its commands go with the pass's */
    }
    if (in_ring.head == in_ring.tail) {
        /* All taken: a gap after the last entry, with nothing behind it yet. Its epoch was
         * read before the ring was seen empty, so every entry before it has been taken. */
        midi_rx_check_lost(&rx, epoch);
    }
    midi_stats.entries += n;
    if (rx.restarts != restarts) {
        sysex_reset(&sidou_sysex);
        midi_sysex_running = (uint8_t)sysex_active(&sidou_sysex);
        sidou_callbacks();
        midi_stats.lost = midi_ring_lost(&in_ring);
        midi_stats.restarts = rx.restarts;
        return 1; /* the restart released the notes: there are writes to commit */
    }
    return n != 0;
}

int midi_drain(void) {
    usb_in_pump();
    /* The epoch first, then the emptiness: a gap after this read is caught next time.
     * Keep the worker's saved registers and packet workspace out of this 10 kHz path. */
    uint8_t epoch = in_ring.epoch;
    if (in_ring.head == in_ring.tail && epoch == rx.epoch) {
        return 0;
    }
    return drain_pending(epoch);
}

void midi_send(void) {
    if (sidou_sysex.tx_active) {
        return; /* channel status must not split a SysEx frame */
    }
    /* No host, or one asleep or gone (usb.c: the frame number stopped): forget the changes
     * rather than queue them for a host that may come back much later. Both flags change in
     * the TIMER5 interrupt, so both are read through volatile: suspended is declared so, and
     * config is read as usb.c's ota_wire_send reads it. */
    if (!*(volatile uint8_t*)&usb.config || usb.suspended) {
        midi_out_drop(&midi_tx, rx.l);
        return;
    }
    /* The ring's room now; the TIMER5 interrupt only ever frees more (ep1_tx).
     * A known race: a bus reset between here and midi_out_event (usb_poll, in the interrupt)
     * sets config to 0, so midi_out_event discards the rest of this tick's packets, while MIDI
     * out counts them as sent (its wire). It is the same as a reset one poll later, which
     * flushes the ring with them in it (mo_r = mo_w): MIDI out's wire is what it handed to
     * USB, never what a host received, and a host that enumerates anew starts from it. */
    uint32_t room = MQ - (mo_w - mo_r);
    uint32_t pkt[MIDI_OUT_TICK_PACKETS];
    uint32_t n = midi_out_tick(&midi_tx, rx.l, room, pkt);
    for (uint32_t i = 0; i < n; i++) {
        midi_out_event(pkt[i]);
    }
}

void midi_service(uint32_t now_ms) {
    usb_retry_quiet(now_ms);
}

int midi_usb_live(void) {
    /* Each changes in the TIMER5 interrupt: read through volatile, as midi_send reads them. */
    return *(volatile uint8_t*)&usb.up && *(volatile uint8_t*)&usb.config && !usb.suspended;
}

#if defined(SIDOU_FLASH_SPIKE)
void midi_spike_trs(uint32_t* bytes, uint32_t* lost) {
    midi_ring_entry e;
    while (midi_ring_pop(&in_ring, &e)) {
    }
    *bytes = um.bytes;
    *lost = um.drops;
}
#endif

/* ---- ota.c's hooks (Felucca's ota_hw.c, adapted) ---- */

static uint8_t flash_ok;           /* the JEDEC id matched at boot */
static void (*commit_hook)(void);  /* update_session's before_reset */

/* Monotonic milliseconds from TIMER4, across its wrap (178.9 s); as the loader's ldr_ms. */
static uint32_t ota_now_ms(void) {
    static uint32_t last, acc, ms;
    uint32_t now = fm1_ticks();
    acc += now - last;
    last = now;
    ms += acc / (1000u * FM1_TICKS_PER_US);
    acc %= 1000u * FM1_TICKS_PER_US;
    return ms;
}

/* The TIMER5 interrupt keeps polling USB while the session waits for the host. */
static void ota_idle(void) {
    fm1_wdt_feed();
}

/* ota_in_area, flash_write_allowed, ota_erase, ota_prog, ota_fread (host-tested). */
#include "ota_flash.h"

static void ota_show(uint32_t step, int32_t code) {
    ui_update(step, code);
    if (step >= 9u) {
        fm1_delay_ms(1500); /* the result stays readable */
    }
}

static void ota_commit(const uint8_t* parm) {
    if (commit_hook) {
        commit_hook();
    }
    usb_detach();
    fm1_delay_ms(30);
    fm1_enter_update(parm); /* the record into RAM, then a core reset: the SPL runs the loader */
}

/* ---- update.h ---- */

/* The FM-1's 1 MiB NOR (fm1_flash.h). */
#define FLASH_JEDEC 0x856014u

void update_boot(void) {
    uint32_t f = irq_save(); /* (its sti is harmless here: no interrupt source is on yet) */
    uint32_t id = FL_FAR(fl_jedec_ram)();
    irq_restore(f);
    flash_ok = id == FLASH_JEDEC;
    if (flash_ok) {
        ota_boot_cleanup();
    }
}

void flash_plain_window(void) {
    /* Maps flash from 0x93000 up as an unencrypted XIP window (fm1_flash.h), so the store
     * reads straight through fm1_xip_ptr. The update protocol's reads (ota_fread) go through
     * fl_read_ram with XIP off, so the window does not change them. */
    fl_plain_window_init();
}

/* ---- store_fm1.h: the store's flash operations ---- */

/* The store's range only, never a sector's guard band (app/store_layout.h). */
static int store_range(uint32_t off, uint32_t n) {
    return FL_IN(off, n, STORE_LO, STORE_HI) &&
           (off & (STORE_SECTOR - 1u)) + n <= STORE_SECTOR_USE;
}

int flash_store_prog(uint32_t off, const void* p, uint32_t n) {
    uint32_t took;
    if (!flash_ok || !store_range(off, n)) {
        return -8;
    }
    return FL_FAR(fl_prog_ram)(off, p, n, &took);
}

int flash_store_erase(uint32_t off) {
    uint32_t took;
    if (!flash_ok || !store_range(off, STORE_SECTOR_USE) || (off & (STORE_SECTOR - 1u))) {
        return -8;
    }
    return FL_FAR(fl_erase4k_ram)(off, &took);
}

int flash_store_ok(void) {
    uint32_t f = irq_save(); /* its sti is harmless at boot: no interrupt source is on yet */
    uint32_t id = FL_FAR(fl_jedec_ram)();
    irq_restore(f);
    flash_ok = id == FLASH_JEDEC;
    return flash_ok;
}

int update_poll(void) {
    if (!sx_ready && !usb.ota_req) {
        return 0; /* the usual case, at every main-loop wake */
    }
    if (sx_ready && sx_frame_len &&
        (sx_frame[0] == 0x7Eu ||
         (sx_frame_len >= 3u && sx_frame[0] == 0x7Du && sx_frame[1] == 0x53u &&
          sx_frame[2] == 0x44u))) {
        /* The raw USB stream already reached SIDOU's parser (usb_raw_in). Free the updater's
         * unrelated copy now, so it can collect a request while our reply streams. */
        ota_frame_done();
        if (!usb.ota_req) {
            return 0;
        }
    }
    if (sidou_sysex.tx_active) {
        return 0; /* finish this frame before the updater's independent reply producer */
    }
    if (sidou_outstanding) {
        if (mo_w != mo_r) {
            return 0; /* OTA's higher-priority queue must not overtake the queued F7 */
        }
        sidou_outstanding = 0;
    }
    ota_service();
    if (usb.ota_req) {
        usb.ota_req = 0;
        return 1;
    }
    return 0;
}

int update_session(void (*before_reset)(void)) {
    if (!flash_ok) {
        ota_show(9, -17); /* not the flash chip we know: nothing is written */
        return -17;
    }
    commit_hook = before_reset;
    return ota_session();
}
