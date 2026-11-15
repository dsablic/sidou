// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_BLE_KEYBOARD_H
#define SIDOU_FM1_BLE_KEYBOARD_H

#include <stdint.h>

#include "ble_scan.h"

#define BLE_KEYBOARD_NAME BLE_NAME_MAX
#define BLE_RECORD_VERSION_ON_OFF 1u
#define BLE_RECORD_VERSION 2u
#define BLE_RECORD_BYTES_ON_OFF 8u

enum { BLE_MODE_OFF, BLE_MODE_ON, BLE_MODE_KEYBOARD, BLE_MODES };
enum { BLE_ADDR_NONE, BLE_ADDR_PUBLIC, BLE_ADDR_RANDOM };

typedef struct {
    uint8_t addr[6];
    uint8_t type;
    char name[BLE_KEYBOARD_NAME + 1u];
} ble_keyboard;

typedef struct {
    uint8_t mode;
    uint8_t own[6];
    ble_keyboard keyboard;
} ble_setting_kept;

#define BLE_RECORD_BYTES (1u + sizeof(ble_setting_kept))

extern ble_setting_kept ble_radio_setting;
void ble_setting_remember(const ble_keyboard* k);

int ble_record_read(ble_setting_kept* out, const uint8_t* rec, uint32_t n);
void ble_record_write(const ble_setting_kept* in, uint8_t* rec);
uint32_t ble_keyboard_list(const struct ble_scan_tab* t, ble_keyboard out[BLE_SCAN_N],
                           uint8_t bars[BLE_SCAN_N]);

#endif
