// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "ble_keyboard.h"

#define BLE_NAMELESS "MIDI "
#define BLE_NAMELESS_CHARS 5u
#define BLE_HEX "0123456789ABCDEF"

static void ble_copy(uint8_t* to, const uint8_t* from, uint32_t n) {
    for (uint32_t i = 0; i < n; i++) {
        to[i] = from[i];
    }
}

_Static_assert(sizeof(ble_setting_kept) ==
                   1u + 6u + 6u + 1u + BLE_KEYBOARD_NAME + 1u + 1u + 16u + 8u + 2u,
               "the record is the setting's bytes as they lie");

int ble_record_read(ble_setting_kept* out, const uint8_t* rec, uint32_t n) {
    uint32_t version = n ? rec[0] : 0u;
    out->bond.has = 0;
    if (version == BLE_RECORD_VERSION_ON_OFF && n == BLE_RECORD_BYTES_ON_OFF) {
        out->mode = rec[1] != 0;
        ble_copy(out->own, rec + 2, 6);
        out->keyboard.type = BLE_ADDR_NONE;
        return 1;
    }
    uint32_t bytes = version == BLE_RECORD_VERSION_UNBONDED ? BLE_RECORD_BYTES_UNBONDED
                     : version == BLE_RECORD_VERSION        ? BLE_RECORD_BYTES
                                                            : 0u;
    if (!bytes || n != bytes) {
        return 0;
    }
    ble_copy((uint8_t*)out, rec + 1, bytes - 1u);
    out->keyboard.name[BLE_KEYBOARD_NAME] = 0;
    if (out->mode >= BLE_MODES) {
        out->mode = BLE_MODE_OFF;
    }
    return 1;
}

void ble_record_write(const ble_setting_kept* in, uint8_t* rec) {
    rec[0] = BLE_RECORD_VERSION;
    ble_copy(rec + 1, (const uint8_t*)in, sizeof *in);
}

static void ble_nameless(char* name, const uint8_t addr[6]) {
    ble_copy((uint8_t*)name, (const uint8_t*)BLE_NAMELESS, BLE_NAMELESS_CHARS);
    char* q = name + BLE_NAMELESS_CHARS;
    for (uint32_t i = 2; i-- > 0;) {
        *q++ = BLE_HEX[addr[i] >> 4];
        *q++ = BLE_HEX[addr[i] & 15u];
    }
    *q = 0;
}

uint32_t ble_keyboard_list(const struct ble_scan_tab* t, ble_keyboard out[BLE_SCAN_N],
                           uint8_t bars[BLE_SCAN_N]) {
    uint8_t idx[BLE_SCAN_N];
    uint32_t n = ble_scan_list(t, idx);
    for (uint32_t k = 0; k < n; k++) {
        const struct ble_found* e = &t->e[idx[k]];
        ble_keyboard* kb = &out[k];
        ble_copy(kb->addr, e->addr, 6);
        kb->type = (uint8_t)(e->addr_rand ? BLE_ADDR_RANDOM : BLE_ADDR_PUBLIC);
        if (e->name[0]) {
            ble_copy((uint8_t*)kb->name, (const uint8_t*)e->name, sizeof kb->name);
        } else {
            ble_nameless(kb->name, e->addr);
        }
        bars[k] = (uint8_t)ble_scan_bars(t, idx[k]);
    }
    return n;
}
