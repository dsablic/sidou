// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "ble_radio.h"
#include "ble_keyboard.h"
#include "midi.h"

#define RING_PUBLISH() __asm__ volatile("" ::: "memory")
#include "ble.h"
#include "ble_ll.h"
#include "ble_util.h"
#include "ble_scan.h"

#define BLE_RADIO_RING 64u

#define BLE_RADIO_START_AFTER_MS 3000u
#define BLE_RADIO_AGE_MS 1000u
#define BLE_RADIO_BROWSE_MS 500u
#define BLE_RADIO_PDU 40u

enum { BLE_RADIO_OFF, BLE_RADIO_READY, BLE_RADIO_QUIET };
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
static struct ble_scan_tab ble_radio_found BLE_RADIO_ZEROED;
static ble_keyboard ble_radio_listed[BLE_SCAN_N] BLE_RADIO_ZEROED;
static struct ble_peer ble_radio_peer BLE_RADIO_ZEROED;
static volatile uint8_t ble_radio_role;
static uint8_t ble_radio_state;
static uint8_t ble_radio_was_linked;
static uint8_t ble_radio_following;
static uint8_t ble_radio_central_was;
static uint8_t ble_radio_keyboard_up;
static uint8_t ble_radio_refused;
static uint32_t ble_radio_now, ble_radio_browsed_at, ble_radio_aged_at;
uint8_t ble_radio_awake = 1;

#if SIDOU_BLE_FAKE
#include "ble_radio_fake.c"
#else
#include "ble_hw_wl82.c"

static const struct ble_rf_trims BLE_RADIO_TRIMS = {{0x0B, 0x0B}, {1, 7, 4, 7, 11, 1, 7}, {0}, {0}};

static void ble_radio_air(uint32_t now_ms) {
    (void)now_ms;
}

static void ble_radio_held(int hold) {
    fm1_ble_irqs_hold(hold);
}

static void ble_radio_power_up(void) {
    fm1_irq_off();
    fm1_ble_irq_attach(BLE_HW_IRQ_PRIO);
    fm1_ble_irqs_hold(1);
    fm1_irq_on();
    fm1_rf_cal_on = 1;
    ble_hw_wl82_start(&BLE_RADIO_TRIMS);
}
#endif

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

BLE_API void ble_app_state(void) {
    ble_radio_role = ble_central_state() != BLE_CS_IDLE ? BLE_ROLE_CENTRAL
                     : ble_connected()                  ? BLE_ROLE_PERIPHERAL
                                                        : BLE_ROLE_NONE;
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

extern uint32_t _ble_noinit_start[], _ble_noinit_end[];

static void ble_radio_follow(void) {
    uint8_t mode = ble_radio_setting.mode;
    if (mode == ble_radio_following) {
        return;
    }
    ble_radio_following = mode;
    ble_radio_keyboard_up = 0;
    ble_radio_refused = 0;
    ble_radio_held(1);
    ble_central_cancel();
    ble_ll_scan(mode == BLE_MODE_KEYBOARD);
    ble_enable(0);
    ble_enable(mode != BLE_MODE_OFF);
    ble_radio_held(0);
    ble_radio_state = mode != BLE_MODE_OFF ? BLE_RADIO_READY : BLE_RADIO_QUIET;
}

static void ble_radio_start(void) {
    for (uint32_t* p = _ble_noinit_start; p < _ble_noinit_end; p++) {
        *p = 0;
    }
    ble_setting_starting();
    ble_init(ble_radio_setting.own, 1);
    ble_radio_power_up();
    ble_radio_following = BLE_MODES;
    ble_radio_follow();
    ble_setting_started();
}

static int ble_radio_midi_up(void) {
    return ble_radio_state == BLE_RADIO_READY && ble_radio_role != BLE_ROLE_NONE &&
           ble_midi_ready();
}

static int ble_radio_browsing(void) {
    return ble_radio_now - ble_radio_browsed_at < BLE_RADIO_BROWSE_MS;
}

static void ble_radio_heard(int keep) {
    uint8_t pdu[BLE_RADIO_PDU];
    uint16_t rssi;
    uint8_t n;
    while ((n = ble_ll_scan_take(pdu, &rssi)) != 0) {
        if (keep) {
            ble_scan_add(&ble_radio_found, pdu, n, rssi, ble_radio_now);
        }
    }
    if (ble_radio_now - ble_radio_aged_at > BLE_RADIO_AGE_MS) {
        ble_radio_aged_at = ble_radio_now;
        ble_scan_age(&ble_radio_found, ble_radio_now);
    }
}

static void ble_radio_central_watch(void) {
    uint8_t now = ble_central_state();
    if (now == ble_radio_central_was) {
        return;
    }
    ble_radio_central_was = now;
    if (now == BLE_CS_READY) {
        ble_radio_keyboard_up = 1;
        ble_radio_tell(BLE_KEYBOARD_CONNECTED, ble_radio_setting.keyboard.name);
    } else if (now == BLE_CS_IDLE) {
        if (ble_central_fail() == BLE_CF_NEED_PAIR) {
            ble_radio_refused = 1;
            ble_radio_tell(BLE_KEYBOARD_PAIRING, ble_radio_setting.keyboard.name);
        } else if (ble_radio_keyboard_up) {
            ble_radio_tell(BLE_KEYBOARD_LOST, ble_radio_setting.keyboard.name);
        }
        ble_radio_keyboard_up = 0;
    }
}

static void ble_radio_look(void) {
    const ble_keyboard* k = &ble_radio_setting.keyboard;
    if (ble_radio_following != BLE_MODE_KEYBOARD || k->type == BLE_ADDR_NONE || ble_radio_refused ||
        ble_radio_browsing() || ble_central_state() != BLE_CS_IDLE) {
        return;
    }
    ble_cpy(ble_radio_peer.addr, k->addr, 6);
    ble_radio_peer.addr_rand = k->type == BLE_ADDR_RANDOM;
    ble_radio_held(1);
    (void)ble_central_connect(&ble_radio_peer);
    ble_radio_held(0);
}

static void ble_radio_link_watch(void) {
    uint8_t linked = ble_radio_role != BLE_ROLE_NONE;
    if (ble_radio_was_linked && !linked) {
        midi_ble_dropped();
    }
    ble_radio_was_linked = linked;
}

static int ble_radio_needed(void) {
    return ble_radio_setting.mode || ble_radio_state == BLE_RADIO_READY || ble_radio_was_linked ||
           ble_setting_pending();
}

void ble_radio_poll(uint32_t now_ms) {
    ble_radio_now = now_ms;
    ble_setting_poll(now_ms);
    ble_radio_link_watch();
    if (ble_radio_state == BLE_RADIO_OFF) {
        if (ble_radio_setting.mode && now_ms >= BLE_RADIO_START_AFTER_MS) {
            ble_radio_start();
        }
    } else {
        ble_radio_air(now_ms);
        ble_radio_follow();
        ble_radio_heard(ble_radio_browsing());
        ble_radio_central_watch();
        ble_radio_look();
        if (!ble_radio_midi_up()) {
            ble_radio_out_r = ble_radio_out_w;
        }
    }
    ble_radio_awake = (uint8_t)ble_radio_needed();
}

uint32_t ble_radio_keyboard(void* ctx, const char** name) {
    (void)ctx;
    *name = ble_radio_setting.keyboard.name;
    if (ble_radio_setting.keyboard.type == BLE_ADDR_NONE) {
        return BLE_KEYBOARD_NONE;
    }
    if (ble_radio_refused) {
        return BLE_KEYBOARD_PAIRING;
    }
    return ble_radio_state == BLE_RADIO_READY && ble_radio_central_was == BLE_CS_READY
               ? BLE_KEYBOARD_CONNECTED
               : BLE_KEYBOARD_LOOKING;
}

uint32_t ble_radio_devices(void* ctx, int fresh, const char** names, uint8_t* bars) {
    (void)ctx;
    if (ble_radio_state != BLE_RADIO_READY) {
        return 0;
    }
    if (fresh) {
        ble_radio_heard(0);
        ble_scan_clear(&ble_radio_found);
        ble_radio_keyboard_up = 0;
        ble_radio_held(1);
        ble_central_cancel();
        ble_radio_held(0);
    }
    ble_radio_browsed_at = ble_radio_now;
    uint32_t n = ble_keyboard_list(&ble_radio_found, ble_radio_listed, bars);
    for (uint32_t i = 0; i < n; i++) {
        names[i] = ble_radio_listed[i].name;
    }
    return n;
}

void ble_radio_pick(void* ctx, uint32_t i) {
    (void)ctx;
    ble_radio_refused = 0;
    ble_radio_browsed_at = ble_radio_now - BLE_RADIO_BROWSE_MS;
    ble_setting_remember(&ble_radio_listed[i]);
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
