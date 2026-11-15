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

enum {
    BLE_KEYBOARD_NONE,
    BLE_KEYBOARD_LOOKING,
    BLE_KEYBOARD_CONNECTED,
    BLE_KEYBOARD_PAIRING,
    BLE_KEYBOARD_LOST,
    BLE_KEYBOARD_PASSKEY
};

extern uint8_t ble_radio_awake;

void ble_radio_poll(uint32_t now_ms);
int ble_radio_take(uint32_t* pkt);
int ble_radio_send(uint32_t pkt, uint32_t now_ms);
int ble_radio_mode(void* ctx);
uint32_t ble_radio_keyboard(void* ctx, const char** name);
uint32_t ble_radio_devices(void* ctx, int fresh, const char** names, uint8_t* bars);
void ble_radio_pick(void* ctx, uint32_t i);
void ble_radio_tell(uint32_t event, const char* name);
void ble_radio_report(void* ctx, char* buf);
void ble_power_boot(void);
void ble_setting_poll(uint32_t now_ms);
int ble_setting_pending(void);
void ble_setting_starting(void);
void ble_setting_started(void);
void ble_radio_set(void* ctx, int mode);

#endif
