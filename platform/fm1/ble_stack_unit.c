// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#if SIDOU_BLE_FAKE
#include "ble_aes.c"
#include "ble_scan.c"
#include "ble_midi.c"
#else
#include "ble_prim.c"
#if BLE_LL_ENC
#include "ble_aes.c"
#endif
#include "ble_midi.c"
#include "ble_ll.c"
#include "ble_smp.c"
#include "ble_host.c"
#include "ble_att.c"
#include "ble_store.c"
#include "ble_scan.c"
#include "ble_gattc.c"
#include "ble_central.c"
#endif
