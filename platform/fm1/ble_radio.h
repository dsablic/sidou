// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_BLE_RADIO_H
#define SIDOU_FM1_BLE_RADIO_H

#include <stdint.h>

enum {
    BLE_POWER_VDC14_BOOT,
    BLE_POWER_SYSVDD_BOOT,
    BLE_POWER_VDC14,
    BLE_POWER_SYSVDD,
    BLE_POWER_TRIES,
    BLE_POWER_DCDC_MOVED,
    BLE_POWER_CLOCK,
    BLE_POWER_BYTES
};

extern volatile uint8_t ble_radio_wanted;
extern uint8_t ble_radio_addr[6];

void ble_radio_poll(uint32_t now_ms);
int ble_radio_take(uint32_t* pkt);
int ble_radio_send(uint32_t pkt, uint32_t now_ms);
void ble_radio_scan(int on);
int ble_radio_on(void* ctx);
void ble_radio_report(void* ctx, char* buf);
void ble_power_boot(void);
void ble_setting_poll(uint32_t now_ms);
void ble_setting_starting(void);
void ble_setting_started(void);
void ble_radio_set(void* ctx, int on);

#endif
