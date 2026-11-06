// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "ble_radio.h"

#define RING_PUBLISH() __asm__ volatile("" ::: "memory")
#include "ble.h"
#include "ble_ll.h"
#include "ble_util.h"
#if BLE_CENTRAL
#include "ble_scan.h"
#endif
#include "ble_hw_wl82.c"

#define BLE_RADIO_RING 64u
#define BLE_RADIO_ADDR_STATIC 0xC0u

#define BLE_RADIO_KEY 0x10Au
#define BLE_RADIO_TYPE 1u
#define BLE_RADIO_RECORD_VERSION 1u
#define BLE_RADIO_START_AFTER_MS 3000u
#define BLE_RADIO_SAVE_AFTER_MS 2000u

enum { BLE_RADIO_OFF, BLE_RADIO_READY, BLE_RADIO_QUIET };
enum { BLE_RECORD_VERSION, BLE_RECORD_ON, BLE_RECORD_ADDR, BLE_RECORD_BYTES = BLE_RECORD_ADDR + 6 };
enum { BLE_ROLE_NONE, BLE_ROLE_PERIPHERAL, BLE_ROLE_CENTRAL };

typedef struct {
    uint32_t pkt;
    uint16_t ms;
} ble_radio_out_slot;

#define BLE_RADIO_ZEROED __attribute__((section(".noinit.ble_radio")))

static ble_radio_out_slot ble_radio_out[BLE_RADIO_RING] BLE_RADIO_ZEROED;
static volatile uint32_t ble_radio_out_w, ble_radio_out_r;
static uint32_t ble_radio_in[BLE_RADIO_RING] BLE_RADIO_ZEROED;
static volatile uint32_t ble_radio_in_w, ble_radio_in_r;
static volatile uint8_t ble_radio_role;
static uint8_t ble_radio_state;
static uint8_t ble_radio_addr[6];
static uint8_t ble_radio_was_linked;
static uint8_t ble_radio_loaded;
static uint8_t ble_radio_dirty;
static uint32_t ble_radio_changed_at;
static uint8_t ble_radio_touched;
static uint8_t ble_radio_record[BLE_RECORD_BYTES];
volatile uint8_t ble_radio_wanted;

static const struct ble_rf_trims BLE_RADIO_TRIMS = {{0x0B, 0x0B}, {1, 7, 4, 7, 11, 1, 7}, {0}, {0}};

BLE_API int ble_app_midi_peek(uint32_t* pkt, uint32_t* t_ms) {
    if (ble_radio_out_r == ble_radio_out_w) {
        return 0;
    }
    *pkt = ble_radio_out[ble_radio_out_r % BLE_RADIO_RING].pkt;
    *t_ms = ble_radio_out[ble_radio_out_r % BLE_RADIO_RING].ms;
    return 1;
}

BLE_API void ble_app_midi_pop(void) {
    if (ble_radio_out_r != ble_radio_out_w) {
        ble_radio_out_r++;
    }
}

BLE_API void ble_app_midi_in(uint32_t pkt, uint16_t ts, uint16_t last_ts) {
    (void)ts;
    (void)last_ts;
    if (ble_radio_in_w - ble_radio_in_r >= BLE_RADIO_RING) {
        return;
    }
    ble_radio_in[ble_radio_in_w % BLE_RADIO_RING] = pkt & ~0xF0u;
    RING_PUBLISH();
    ble_radio_in_w++;
}

#if BLE_CENTRAL
static struct ble_scan_tab ble_radio_found BLE_RADIO_ZEROED;

static uint8_t ble_radio_role_now(void) {
    if (ble_central_state() != BLE_CS_IDLE) {
        return BLE_ROLE_CENTRAL;
    }
    return ble_connected() ? BLE_ROLE_PERIPHERAL : BLE_ROLE_NONE;
}
#else
static uint8_t ble_radio_role_now(void) {
    return ble_connected() ? BLE_ROLE_PERIPHERAL : BLE_ROLE_NONE;
}
#endif

BLE_API void ble_app_state(void) {
    ble_radio_role = ble_radio_role_now();
}

#if BLE_SMP_LEGACY
BLE_API void ble_app_bond(const uint8_t rand[8], uint16_t ediv, const uint8_t ltk[16]) {
    (void)rand;
    (void)ediv;
    (void)ltk;
}

BLE_API void ble_app_peer_id(const uint8_t irk[16], const uint8_t addr[6], uint8_t addr_rand) {
    (void)irk;
    (void)addr;
    (void)addr_rand;
}
#endif

#if BLE_CEN_PAIR
BLE_API void ble_app_central_keys(const struct ble_keys* k) {
    (void)k;
}
#endif

static void ble_radio_held(int hold) {
    fm1_ble_irqs_hold(hold);
}

static void ble_radio_address(uint8_t a[6]) {
    uint32_t h = fm1_ticks() * 2654435761u;
    uint32_t g = (fm1_ticks() ^ h) * 2246822519u;
    a[0] = (uint8_t)h;
    a[1] = (uint8_t)(h >> 8);
    a[2] = (uint8_t)g;
    a[3] = (uint8_t)(g >> 8);
    a[4] = (uint8_t)(g >> 16);
    a[5] = (uint8_t)(g >> 24) | BLE_RADIO_ADDR_STATIC;
}

extern uint32_t _ble_noinit_start[], _ble_noinit_end[];

static void ble_radio_start(void) {
    for (uint32_t* p = _ble_noinit_start; p < _ble_noinit_end; p++) {
        *p = 0;
    }
    ble_init(ble_radio_addr, 1);
    fm1_irq_off();
    fm1_ble_irq_attach(BLE_HW_IRQ_PRIO);
    fm1_ble_irqs_hold(1);
    fm1_irq_on();
    fm1_rf_cal_on = 1;
    ble_hw_wl82_start(&BLE_RADIO_TRIMS);
    ble_radio_held(1);
    ble_enable(1);
    ble_radio_held(0);
    ble_radio_state = BLE_RADIO_READY;
}

static int ble_radio_addressed(void) {
    return (ble_radio_addr[5] & BLE_RADIO_ADDR_STATIC) == BLE_RADIO_ADDR_STATIC;
}

static void ble_radio_load(void) {
    uint8_t rec[BLE_RECORD_BYTES];
    int n = store_get(store_fm1(), BLE_RADIO_KEY, rec, sizeof rec, 0);
    if (n == (int)sizeof rec && rec[BLE_RECORD_VERSION] == BLE_RADIO_RECORD_VERSION) {
        ble_radio_wanted = rec[BLE_RECORD_ON] != 0;
        for (uint32_t i = 0; i < 6u; i++) {
            ble_radio_addr[i] = rec[BLE_RECORD_ADDR + i];
        }
    }
    if (!ble_radio_addressed()) {
        ble_radio_address(ble_radio_addr);
    }
}

static void ble_radio_save(uint32_t now_ms) {
    store* st = store_fm1();
    if (!ble_radio_dirty || now_ms - ble_radio_changed_at < BLE_RADIO_SAVE_AFTER_MS ||
        store_busy(st)) {
        return;
    }
    uint8_t* rec = ble_radio_record;
    rec[BLE_RECORD_VERSION] = BLE_RADIO_RECORD_VERSION;
    rec[BLE_RECORD_ON] = ble_radio_wanted;
    for (uint32_t i = 0; i < 6u; i++) {
        rec[BLE_RECORD_ADDR + i] = ble_radio_addr[i];
    }
    int rc = store_put(st, BLE_RADIO_KEY, BLE_RADIO_TYPE, rec, BLE_RECORD_BYTES);
    ble_radio_dirty = rc != STORE_OK && rc != STORE_FOREIGN && rc != STORE_FULL &&
                      rc != STORE_INVALID;
    ble_radio_changed_at = now_ms;
}

int ble_radio_on(void* ctx) {
    (void)ctx;
    return ble_radio_wanted;
}

void ble_radio_set(void* ctx, int on) {
    (void)ctx;
    if (ble_radio_wanted != (on != 0)) {
        ble_radio_wanted = (uint8_t)(on != 0);
        ble_radio_dirty = 1;
        ble_radio_touched = 1;
    }
}

static void ble_radio_follow(void) {
    if (ble_radio_state == BLE_RADIO_READY && !ble_radio_wanted) {
        ble_radio_held(1);
        ble_enable(0);
        ble_radio_held(0);
        ble_radio_state = BLE_RADIO_QUIET;
    } else if (ble_radio_state == BLE_RADIO_QUIET && ble_radio_wanted) {
        ble_radio_held(1);
        ble_enable(1);
        ble_radio_held(0);
        ble_radio_state = BLE_RADIO_READY;
    }
}

static int ble_radio_midi_up(void) {
    return ble_radio_state == BLE_RADIO_READY && ble_radio_role != BLE_ROLE_NONE &&
           ble_midi_ready();
}

#if BLE_CENTRAL
#define BLE_RADIO_AGE_MS 1000u

static void ble_radio_devices(uint32_t now_ms) {
    static uint32_t aged;
    uint8_t pdu[40];
    uint16_t rssi;
    uint8_t n;
    while ((n = ble_ll_scan_take(pdu, &rssi)) != 0) {
        ble_scan_add(&ble_radio_found, pdu, n, rssi, now_ms);
    }
    if (now_ms - aged > BLE_RADIO_AGE_MS) {
        aged = now_ms;
        ble_scan_age(&ble_radio_found, now_ms);
    }
    uint8_t idx[BLE_SCAN_N];
    if (ble_radio_role != BLE_ROLE_NONE || !ble_scan_list(&ble_radio_found, idx)) {
        return;
    }
    struct ble_peer p;
    ble_zero((uint8_t*)&p, sizeof p);
    ble_cpy(p.addr, ble_radio_found.e[idx[0]].addr, 6);
    p.addr_rand = ble_radio_found.e[idx[0]].addr_rand;
    ble_radio_held(1);
    (void)ble_central_connect(&p);
    ble_radio_held(0);
}

void ble_radio_scan(int on) {
    if (ble_radio_state != BLE_RADIO_READY) {
        return;
    }
    if (on) {
        ble_scan_clear(&ble_radio_found);
    }
    ble_radio_held(1);
    ble_ll_scan(on);
    ble_radio_held(0);
}
#else
static void ble_radio_devices(uint32_t now_ms) {
    (void)now_ms;
}

void ble_radio_scan(int on) {
    (void)on;
}
#endif

static void ble_radio_link_watch(void) {
    uint8_t linked = ble_radio_role != BLE_ROLE_NONE;
    if (ble_radio_was_linked && !linked) {
        midi_ble_dropped();
    }
    ble_radio_was_linked = linked;
}

void ble_radio_poll(uint32_t now_ms) {
    if (!ble_radio_loaded) {
        ble_radio_loaded = 1;
        ble_radio_load();
    }
    if (ble_radio_touched) {
        ble_radio_touched = 0;
        ble_radio_changed_at = now_ms;
    }
    ble_radio_save(now_ms);
    ble_radio_link_watch();
    if (ble_radio_state == BLE_RADIO_OFF) {
        if (ble_radio_wanted && now_ms >= BLE_RADIO_START_AFTER_MS) {
            ble_radio_start();
        }
        return;
    }
    ble_radio_follow();
    ble_radio_devices(now_ms);
    if (!ble_radio_midi_up()) {
        ble_radio_out_r = ble_radio_out_w;
    }
}

int ble_radio_take(uint32_t* pkt) {
    if (ble_radio_in_r == ble_radio_in_w) {
        return 0;
    }
    *pkt = ble_radio_in[ble_radio_in_r % BLE_RADIO_RING];
    RING_PUBLISH();
    ble_radio_in_r++;
    return 1;
}

int ble_radio_send(uint32_t pkt, uint32_t now_ms) {
    if (!ble_radio_midi_up() || ble_radio_out_w - ble_radio_out_r >= BLE_RADIO_RING) {
        return 0;
    }
    ble_radio_out[ble_radio_out_w % BLE_RADIO_RING].pkt = pkt;
    ble_radio_out[ble_radio_out_w % BLE_RADIO_RING].ms = (uint16_t)now_ms;
    RING_PUBLISH();
    ble_radio_out_w++;
    return 1;
}
