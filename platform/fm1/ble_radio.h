// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_BLE_RADIO_H
#define SIDOU_FM1_BLE_RADIO_H

#include <stdint.h>

extern volatile uint8_t ble_radio_wanted;

void ble_radio_poll(uint32_t now_ms);
int ble_radio_take(uint32_t* pkt);
int ble_radio_send(uint32_t pkt, uint32_t now_ms);
void ble_radio_scan(int on);
int ble_radio_on(void* ctx);
void ble_radio_set(void* ctx, int on);

#endif
