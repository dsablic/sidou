// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stdint.h>

#include "fm1_sys.h"
#include "fm1_time.h"
#include "store_layout.h"
#define FL_RANGE_OK(off, n) (FL_IN(off, n, FL_OTA_LO, FL_OTA_HI) || FL_IN(off, n, STORE_LO, STORE_HI))
#include "fm1_flash.h"

#include "audio.h"
#include "input.h"
#include "link.h"
#include "midi.h"
#include "midi_out.h"
#include "midi_ring.h"
#include "midi_rx.h"
#include "reg_ring.h"
#include "ui.h"
#include "update.h"
#include "store_fm1.h"

#define MIDI_ENTRY_WRITES (3u * 25u + LIVE_VOICES)
#define MIDI_LOST_WRITES LIVE_VOICES
#define MIDI_DRAIN_RESERVE (MIDI_ENTRY_WRITES + MIDI_LOST_WRITES + REG_RING_WORST_PASS)
_Static_assert(MIDI_DRAIN_RESERVE < REG_RING_SIZE, "the ring leaves MIDI no room");

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
static midi_learn learned __attribute__((used, section(".pool")));
static midi_route routing __attribute__((used));
static midi_sync syncing __attribute__((used));
static midi_out midi_tx __attribute__((used));
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

static link_state live_link __attribute__((used, section(".pool")));
uint8_t midi_link_active;

static void sidou_usb(void* ctx, midi_sysex_event event, uint8_t byte) {
    ((midi_rx*)ctx)->inputs++;
    (void)event;
    link_byte(&live_link, byte, *sysex_ms);
    midi_link_active = (uint8_t)(live_link.on | live_link.want);
    sysex_byte(&sidou_sysex, SYSEX_USB, byte, *sysex_ms);
    midi_sysex_running = (uint8_t)sysex_active(&sidou_sysex);
    store_fm1_active |= midi_sysex_running;
}

static void sidou_callbacks(void) {
    rx.trs.on_sysex = sidou_trs;
    rx.usb.on_sysex = sidou_usb;
}

static struct {
    uint32_t entries;
    uint32_t lost;
    uint32_t restarts;
    uint32_t max_wait_ticks;
    uint32_t usb_polls;
    uint32_t uart_polls;
} midi_stats __attribute__((used));

#include "usb_config.h"
#define FELUCCA_ID SIDOU_PRODUCT

static uint8_t usb_reset_waited;

static void usb_reset_wait(uint32_t ms) {
    if (!usb_reset_waited) {
        fm1_delay_ms(ms);
    }
}

static void usb_raw_in(uint32_t pkt);
#define FELUCCA_MIDI_IN_RAW usb_raw_in

/* clang-format off */
#define sysex_byte ota_sysex_byte
#define fm1_delay_ms usb_reset_wait
#include "src/usb.c"
#undef sysex_byte
#undef fm1_delay_ms
/* clang-format on */
#include "usb_audio.c"

static uint32_t in_room(void) {
    return MIDI_RING_SIZE - (in_ring.head - in_ring.tail);
}

static void ep1_hold(void) {
    if (mi_w - mi_r <= MQ - EP1_ROOM) {
        mi_r = mi_w - (MQ - EP1_ROOM + 1u);
    }
}

static void usb_raw_in(uint32_t pkt) {
    (void)midi_ring_push(&in_ring, MIDI_SRC_USB, pkt, poll_stamp);
    if (in_room() < EP1_ROOM) {
        ep1_hold();
    }
}

#define USB_PROBE_POLLS 2000u

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
    static uint8_t step;
    if (usb.up || usb.detached) {
        step = 0;
        return;
    }
    if (step == 0) {
        if (now_ms - t < 1000u) {
            return;
        }
        t = now_ms;
        fm1_usb_reset();
        step = 1;
        return;
    }
    if (now_ms - t <= 25u) {
        return;
    }
    t = now_ms;
    if (step == 1) {
        step = usb_probe() ? 2 : 0;
        return;
    }
    step = 0;
    usb_reset_waited = 1;
    usb_retry(now_ms);
    usb_reset_waited = 0;
}

static int trs_enqueue(uint32_t data, uint32_t source) {
    return midi_ring_push(&in_ring, (uint8_t)source, data, poll_stamp);
}

static void trs_lost(uint32_t n) {
    midi_ring_lose(&in_ring, MIDI_SRC_TRS, n);
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

static void usb_in_pump(void) {
    uint32_t room = in_room();
    if (mi_r == mi_w && !midi_in_overflow) {
        if (room < EP1_ROOM) {
            uint32_t f = irq_save();
            ep1_hold();
            irq_restore(f);
        }
        return;
    }
    if (room >= EP1_ROOM) {
        uint32_t f = irq_save();
        mi_r = mi_w;
        midi_in_overflow = 0;
        irq_restore(f);
    }
}

static int sidou_send(void* ctx, const uint8_t packet[4]) {
    (void)ctx;
    if (!midi_usb_live() || mo_w - mo_r >= MQ - MIDI_SYNC_OUT_MAX || so_w != so_r || sx_busy) {
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

void midi_sysex_service(uint32_t now_ms, int loud) {
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
    sidou_sysex.loud = (uint8_t)(loud != 0);
    sysex_poll(&sidou_sysex, now_ms);
    midi_sysex_running = (uint8_t)sysex_active(&sidou_sysex);
}

void midi_link_mark(uint32_t row, uint32_t col, uint32_t len) {
    link_mark(&live_link, row, col, len);
}

int midi_link_on(void) {
    return live_link.on;
}

uint32_t midi_link_buttons(uint32_t* pressed) {
    return link_take(&live_link, pressed);
}

int32_t midi_link_steps(uint32_t enc) {
    return link_steps(&live_link, enc);
}

void midi_link_run(const grid* g, uint32_t now_ms) {
    if (!midi_usb_live()) {
        link_drop(&live_link);
        midi_link_active = 0;
        return;
    }
    uint32_t used = mo_w - mo_r;
    int busy = midi_sysex_running || sx_busy || so_w != so_r;
    uint8_t f[LINK_FRAME_MAX + 2u];
    uint32_t n = link_poll(&live_link, g, now_ms, used < MQ ? MQ - used : 0u, busy, f);
    f[n] = f[n + 1u] = 0;
    for (uint32_t i = 0; i < n; i += 3u) {
        uint32_t cin = i + 3u >= n ? 4u + n - i : 4u;
        midi_out_event(cin | (uint32_t)f[i] << 8 | (uint32_t)f[i + 1u] << 16 |
                       (uint32_t)f[i + 2u] << 24);
    }
    if (n) {
        sidou_outstanding = 1;
    }
    midi_link_active = (uint8_t)(live_link.on | live_link.want);
}

void midi_init(live* l, keys_route* kr) {
    midi_ring_init(&in_ring);
    midi_rx_init(&rx, l, midi_ring_epoch(&in_ring));
    midi_route_init(&routing, l, kr, NULL);
    midi_rx_route(&rx, &routing);
    midi_sync_init(&syncing);
    midi_rx_sync(&rx, &syncing);
    midi_out_init(&midi_tx, l, (uint8_t)(SIDOU_MIDI_OUT_CHANNEL - 1), SIDOU_MIDI_OUT_NRPN != 0);
    midi_rx_watch(&rx, &midi_tx);
    midi_learn_init(&learned);
    midi_rx_learn(&rx, &learned);
    midi_out_learn(&midi_tx, &learned);
    sidou_callbacks();
    link_init(&live_link, "SIDOU " SIDOU_VERSION);
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
    midi_sync_pass();
}

midi_sync* midi_synced(void) {
    return &syncing;
}

void midi_sync_pass(void) {
    uint32_t t, smp;
    audio_song_anchor(&t, &smp);
    if (t == 0u && smp == 0u) {
        return;
    }
    midi_sync_anchor(&syncing, t, smp);
    if (!(syncing.want | syncing.aligning | syncing.fresh | syncing.clock.active |
          syncing.clock_out)) {
        return;
    }
    uint32_t now = fm1_ticks();
    midi_sync_rows(&syncing, routing.setup.rows_per_beat);
    syncing.out_common_wait = sidou_sysex.tx_active;
    int up = *(volatile uint8_t*)&usb.config && !usb.suspended;
    uint32_t used = mo_w - mo_r;
    uint32_t room = used < MQ ? MQ - used : 0u;
    syncing.out_room = (uint8_t)(!up || room > MIDI_SYNC_OUT_MAX ? MIDI_SYNC_OUT_MAX : room);
    uint32_t pkt[MIDI_SYNC_OUT_MAX];
    uint32_t n = midi_sync_poll(&syncing, now, pkt);
    for (uint32_t i = 0; i < n && up; i++) {
        midi_out_event(pkt[i]);
    }
}

void midi_clock_held(uint32_t from, uint32_t to) {
    midi_sync_held(&syncing, from, to);
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

static void sysex_gap(uint8_t sources) {
    sources = sources & (MIDI_SRC_USB | MIDI_SRC_TRS) ? sources : MIDI_SRC_USB | MIDI_SRC_TRS;
    if (sources & MIDI_SRC_USB) {
        sysex_lost(&sidou_sysex, SYSEX_USB);
    }
    if (sources & MIDI_SRC_TRS) {
        sysex_lost(&sidou_sysex, SYSEX_TRS);
    }
    midi_sysex_running = (uint8_t)sysex_active(&sidou_sysex);
}

__attribute__((noinline, unused)) static int drain_pending(uint8_t epoch) {
    midi_ring_entry e;
    uint32_t n = 0;
    uint32_t restarts = rx.restarts;
    while (audio_queued() + MIDI_DRAIN_RESERVE <= REG_RING_SIZE &&
           midi_ring_pop(&in_ring, &e)) {
        uint32_t wait = fm1_ticks() - e.stamp;
        if (wait > midi_stats.max_wait_ticks) {
            midi_stats.max_wait_ticks = wait;
        }
        if (e.epoch != rx.epoch) {
            sysex_gap(e.gap);
        }
        midi_rx_entry(&rx, &e);
        n++;
    }
    midi_rx_settle(&rx);
    if (syncing.fresh | syncing.want) {
        midi_sync_pass();
    }
    if (in_ring.head == in_ring.tail) {
        uint8_t sources = in_ring.in_gap;
        if (in_ring.epoch != epoch) {
            sources = 0;
        }
        if (midi_rx_check_lost(&rx, epoch, sources)) {
            sysex_gap(sources);
        }
    }
    midi_stats.entries += n;
    if (rx.restarts != restarts) {
        midi_stats.lost = midi_ring_lost(&in_ring);
        midi_stats.restarts = rx.restarts;
        return 1;
    }
    return n != 0;
}

int midi_drain(void) {
    usb_in_pump();
    uint8_t epoch = in_ring.epoch;
    if (in_ring.head == in_ring.tail && epoch == rx.epoch) {
        return 0;
    }
    return drain_pending(epoch);
}

void midi_send(void) {
    if (sidou_sysex.tx_active) {
        return;
    }
    if (!*(volatile uint8_t*)&usb.config || usb.suspended) {
        midi_out_drop(&midi_tx, rx.l);
        return;
    }
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

int midi_usb_recording(void) {
    return uac.alt && *(volatile uint8_t*)&usb.config && uac.flowing;
}

int midi_usb_live(void) {
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

static uint8_t flash_ok;
static void (*commit_hook)(void);

static uint32_t ota_now_ms(void) {
    static uint32_t last, acc, ms;
    uint32_t now = fm1_ticks();
    acc += now - last;
    last = now;
    ms += acc / (1000u * FM1_TICKS_PER_US);
    acc %= 1000u * FM1_TICKS_PER_US;
    return ms;
}

static void ota_drop_input(void) {
    midi_ring_entry e;
    while (midi_ring_pop(&in_ring, &e)) {
    }
    usb_in_pump();
}

static void ota_idle(void) {
    fm1_wdt_feed();
    ota_drop_input();
}

#include "ota_flash.h"

static void ota_show(uint32_t step, int32_t code) {
    ui_update(step, code);
    if (step >= 9u) {
        fm1_delay_ms(1500);
    }
}

static void ota_commit(const uint8_t* parm) {
    if (commit_hook) {
        commit_hook();
    }
    usb_detach();
    fm1_delay_ms(30);
    fm1_enter_update(parm);
}

#define FLASH_JEDEC 0x856014u

void update_boot(void) {
    uint32_t f = irq_save();
    uint32_t id = FL_FAR(fl_jedec_ram)();
    irq_restore(f);
    flash_ok = id == FLASH_JEDEC;
    if (flash_ok) {
        ota_boot_cleanup();
    }
}

void flash_plain_window(void) {
    fl_plain_window_init();
}

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
    uint32_t f = irq_save();
    uint32_t id = FL_FAR(fl_jedec_ram)();
    irq_restore(f);
    flash_ok = id == FLASH_JEDEC;
    return flash_ok;
}

int update_poll(void) {
    if (!sx_ready && !usb.ota_req) {
        return 0;
    }
    if (sx_ready && sx_frame_len && (sx_frame[0] == 0x7Eu || sx_frame[0] == 0x7Du)) {
        ota_frame_done();
        if (!usb.ota_req) {
            return 0;
        }
    }
    if (sidou_sysex.tx_active) {
        return 0;
    }
    if (sidou_outstanding) {
        if (mo_w != mo_r) {
            return 0;
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
        ota_show(9, -17);
        return -17;
    }
    commit_hook = before_reset;
    return ota_session();
}
