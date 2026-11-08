// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "fm1_sys.h"
#include "fm1_flash_access.h"

#include "ble_radio.h"
#include "store_fm1.h"

#define BLE_RADIO_ADDR_STATIC 0xC0u
#define BLE_RADIO_KEY 0x10Au
#define BLE_RADIO_TYPE 1u
#define BLE_RADIO_RECORD_VERSION 1u
#define BLE_RADIO_SAVE_AFTER_MS 2000u
#define BLE_RADIO_STARTING 0x42545354u

enum { BLE_RECORD_VERSION, BLE_RECORD_ON, BLE_RECORD_ADDR, BLE_RECORD_BYTES = BLE_RECORD_ADDR + 6 };

volatile uint8_t ble_radio_wanted;
volatile uint8_t ble_power_report[BLE_POWER_BYTES];
uint8_t ble_radio_addr[6];
static uint8_t ble_radio_loaded;
static uint8_t ble_radio_dirty;
static uint8_t ble_radio_touched;
static uint8_t ble_radio_ran;
static uint8_t ble_radio_failed;
static uint32_t ble_radio_changed_at;
static uint8_t ble_radio_record[BLE_RECORD_BYTES];
static uint32_t ble_radio_start_mark __attribute__((section(".noinit"), used));

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
    if (ble_radio_start_mark == BLE_RADIO_STARTING) {
        ble_radio_start_mark = 0;
        ble_radio_failed = 1;
        ble_radio_wanted = 0;
        ble_radio_dirty = 1;
        ble_radio_touched = 1;
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
    ble_radio_dirty = rc == STORE_BUSY || rc == STORE_ERROR;
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
        ble_radio_dirty = !on || ble_radio_ran;
        ble_radio_touched = ble_radio_dirty;
        ble_radio_awake = 1;
    }
}

#define BLE_REPORT_HEX "0123456789ABCDEF"

void ble_radio_report(void* ctx, char* buf) {
    (void)ctx;
    char* q = buf;
    *q++ = ble_radio_failed ? '!' : 'P';
    for (uint32_t i = 0; i < BLE_POWER_BYTES; i++) {
        uint8_t v = ble_power_report[i];
        *q++ = ' ';
        *q++ = BLE_REPORT_HEX[v >> 4];
        *q++ = BLE_REPORT_HEX[v & 15u];
    }
    *q = 0;
}

#define BLE_RAIL_VDC14 7u
#define BLE_RAIL_SYSVDD 15u
#define BLE_RAIL_TRIES 3u
#define BLE_CLK_SYS_CON (*(volatile uint32_t*)0x10014u)

void ble_power_boot(void) {
    uint32_t f = irq_save();
    ble_power_report[BLE_POWER_VDC14_BOOT] = (uint8_t)fm1_vdc14();
    ble_power_report[BLE_POWER_SYSVDD_BOOT] = (uint8_t)fm1_sysvdd();
    uint32_t tries = 1;
    for (;; tries++) {
        fm1_vdc14_raise(BLE_RAIL_VDC14);
        fm1_sysvdd_raise(BLE_RAIL_SYSVDD);
        if ((fm1_vdc14() == BLE_RAIL_VDC14 && fm1_sysvdd() == BLE_RAIL_SYSVDD) ||
            tries == BLE_RAIL_TRIES) {
            break;
        }
    }
    ble_power_report[BLE_POWER_VDC14] = (uint8_t)fm1_vdc14();
    ble_power_report[BLE_POWER_SYSVDD] = (uint8_t)fm1_sysvdd();
    ble_power_report[BLE_POWER_TRIES] = (uint8_t)tries;
    ble_power_report[BLE_POWER_DCDC_MOVED] = (uint8_t)fm1_power_stock();
    ble_power_report[BLE_POWER_CLOCK] = (uint8_t)BLE_CLK_SYS_CON;
    irq_restore(f);
}

void ble_setting_poll(uint32_t now_ms) {
    if (!ble_radio_loaded) {
        ble_radio_loaded = 1;
        ble_radio_load();
    }
    if (ble_radio_touched) {
        ble_radio_touched = 0;
        ble_radio_changed_at = now_ms;
    }
    ble_radio_save(now_ms);
}

int ble_setting_pending(void) {
    return !ble_radio_loaded || ble_radio_dirty || ble_radio_touched;
}

void ble_setting_starting(void) {
    ble_radio_start_mark = BLE_RADIO_STARTING;
}

void ble_setting_started(void) {
    ble_radio_start_mark = 0;
    ble_radio_ran = 1;
    ble_radio_dirty = 1;
    ble_radio_touched = 1;
}
