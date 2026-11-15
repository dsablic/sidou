// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "ble_aes.h"
#include "ble_midi.h"

#define BLE_FAKE_PDUS 8u
#define BLE_FAKE_HEARD_EVERY_MS 50u
#define BLE_FAKE_MIDI 32u
#define BLE_FAKE_PAIR_MS 40u
#define BLE_FAKE_PASSKEYS 1000000u

enum { BLE_FAKE_OPEN, BLE_FAKE_JUST_WORKS, BLE_FAKE_PASSKEY };

enum {
    BLE_FAKE_OFF,
    BLE_FAKE_ADVERTISING,
    BLE_FAKE_SCANNING,
    BLE_FAKE_INITIATING,
    BLE_FAKE_LINKED
};

volatile uint8_t ble_fake_pdu[BLE_FAKE_PDUS][BLE_RADIO_PDU];
volatile uint8_t ble_fake_pdu_len[BLE_FAKE_PDUS];
volatile uint8_t ble_fake_away;
volatile uint8_t ble_fake_secure;
volatile uint8_t ble_fake_computer;
volatile uint8_t ble_fake_midi[BLE_FAKE_MIDI];
volatile uint8_t ble_fake_midi_len;
volatile uint8_t ble_fake_ll;
volatile uint32_t ble_fake_advertised;
volatile uint32_t ble_fake_typed = BLE_NO_PASSKEY;
volatile uint8_t ble_fake_forgot;
volatile uint8_t ble_fake_bonded;
volatile uint32_t ble_fake_pairings;
volatile uint32_t ble_fake_encryptions;

static uint8_t ble_fake_enabled, ble_fake_scan_wish, ble_fake_central, ble_fake_leaving;
static uint8_t ble_fake_cs, ble_fake_fail, ble_fake_next;
static uint8_t ble_fake_target[6], ble_fake_target_rand;
static uint32_t ble_fake_now, ble_fake_heard_at, ble_fake_pair_at;
static struct ble_peer ble_fake_peer;
static uint8_t ble_fake_pairing, ble_fake_auth;
static uint32_t ble_fake_shown;
static uint8_t ble_fake_ltk[16];
static const uint8_t BLE_FAKE_PREQ[7] = {0x01, 0x03, 0x00, 0x01, 0x10, 0x01, 0x01};
static const uint8_t BLE_FAKE_PRES[7] = {0x02, 0x00, 0x00, 0x01, 0x10, 0x01, 0x01};
static struct ble_midi_dec ble_fake_dec;

static void ble_radio_held(int hold) {
    (void)hold;
}

static void ble_radio_power_up(void) {
}

static void ble_fake_idle(void) {
    ble_fake_ll = !ble_fake_enabled    ? BLE_FAKE_OFF
                  : ble_fake_scan_wish ? BLE_FAKE_SCANNING
                                       : BLE_FAKE_ADVERTISING;
    ble_fake_advertised += ble_fake_ll == BLE_FAKE_ADVERTISING;
}

static void ble_fake_closed(void) {
    if (ble_fake_central) {
        ble_fake_fail = ble_fake_fail == BLE_CF_NONE ? BLE_CF_LOST : ble_fake_fail;
        ble_fake_cs = BLE_CS_IDLE;
    }
    ble_fake_central = 0;
    ble_fake_leaving = 0;
    ble_fake_ll = BLE_FAKE_OFF;
    ble_fake_idle();
    ble_app_state();
}

BLE_API void ble_init(const uint8_t addr[6], uint8_t addr_random) {
    (void)addr;
    (void)addr_random;
    ble_fake_ll = BLE_FAKE_OFF;
}

BLE_API void ble_enable(int on) {
    ble_fake_enabled = (uint8_t)(on != 0);
    if (ble_fake_ll == BLE_FAKE_LINKED) {
        ble_fake_leaving |= (uint8_t)!on;
    } else if (on ? ble_fake_ll == BLE_FAKE_OFF : 1) {
        ble_fake_idle();
    }
}

BLE_API int ble_connected(void) {
    return ble_fake_ll == BLE_FAKE_LINKED;
}

BLE_API int ble_midi_ready(void) {
    return ble_fake_ll == BLE_FAKE_LINKED && !ble_fake_central;
}

BLE_API void ble_ll_scan(int on) {
    ble_fake_scan_wish = (uint8_t)(on != 0);
    if (ble_fake_enabled && ble_fake_ll != BLE_FAKE_INITIATING && ble_fake_ll != BLE_FAKE_LINKED) {
        ble_fake_idle();
    }
}

static int ble_fake_slot_heard(uint32_t i) {
    return ble_fake_pdu_len[i] && !ble_fake_away;
}

BLE_API uint8_t ble_ll_scan_take(uint8_t* pdu, uint16_t* rssi) {
    if (ble_fake_ll != BLE_FAKE_SCANNING ||
        ble_fake_now - ble_fake_heard_at < BLE_FAKE_HEARD_EVERY_MS) {
        return 0;
    }
    for (uint32_t k = 0; k < BLE_FAKE_PDUS; k++) {
        uint32_t i = (ble_fake_next + k) % BLE_FAKE_PDUS;
        if (ble_fake_slot_heard(i)) {
            ble_fake_next = (uint8_t)(i + 1u);
            ble_fake_heard_at = ble_fake_now;
            for (uint32_t b = 0; b < ble_fake_pdu_len[i]; b++) {
                pdu[b] = ble_fake_pdu[i][b];
            }
            *rssi = 0;
            return ble_fake_pdu_len[i];
        }
    }
    return 0;
}

BLE_API int ble_central_connect(const struct ble_peer* p) {
    if (!ble_fake_enabled || ble_fake_ll == BLE_FAKE_LINKED) {
        return 0;
    }
    ble_fake_peer = *p;
    ble_fake_pairing = 0;
    ble_fake_shown = BLE_NO_PASSKEY;
    ble_cpy(ble_fake_target, p->addr, 6);
    ble_fake_target_rand = p->addr_rand;
    ble_fake_ll = BLE_FAKE_INITIATING;
    ble_fake_cs = BLE_CS_CONNECTING;
    ble_fake_fail = BLE_CF_NONE;
    return 1;
}

BLE_API void ble_central_cancel(void) {
    if (ble_fake_ll == BLE_FAKE_INITIATING) {
        ble_fake_cs = BLE_CS_IDLE;
        ble_fake_ll = BLE_FAKE_OFF;
        ble_fake_idle();
    } else if (ble_fake_ll == BLE_FAKE_LINKED && ble_fake_central) {
        ble_fake_leaving = 1;
    }
}

BLE_API uint8_t ble_central_state(void) {
    return ble_fake_cs;
}

BLE_API uint8_t ble_central_fail(void) {
    return ble_fake_fail;
}

BLE_API int ble_central_pairing(void) {
    return ble_fake_pairing;
}

BLE_API uint32_t ble_central_passkey(void) {
    return ble_fake_pairing ? ble_fake_shown : BLE_NO_PASSKEY;
}

static void ble_fake_random(uint8_t salt, uint8_t out[16]) {
    uint8_t in[16] = {salt, (uint8_t)ble_fake_pairings, (uint8_t)ble_fake_now};
    static const uint8_t KEY[16] = {'S', 'I', 'D', 'O', 'U', ' ', 'F', 'A', 'K', 'E'};
    ble_aes128(KEY, in, out);
}

static void ble_fake_confirm(uint32_t passkey, const uint8_t r[16], uint8_t out[16]) {
    uint8_t tk[16] = {0};
    if (passkey != BLE_NO_PASSKEY) {
        ble_smp_passkey_tk(passkey, tk);
    }
    ble_smp_c1(tk, r, BLE_FAKE_PREQ, BLE_FAKE_PRES, 1, ble_radio_setting.own, ble_fake_target_rand,
               ble_fake_target, out);
}

static void ble_fake_leave(uint8_t why) {
    ble_fake_fail = why;
    ble_fake_pairing = 0;
    ble_fake_leaving = 1;
}

static void ble_fake_pair_start(void) {
    if (ble_fake_secure == BLE_FAKE_PASSKEY && !(ble_fake_peer.sec & BLE_PEER_MITM)) {
        ble_fake_leave(BLE_CF_NEED_MITM);
        return;
    }
    uint8_t r[16];
    ble_fake_random(0, r);
    ble_fake_pairing = 1;
    ble_fake_auth = ble_fake_secure == BLE_FAKE_PASSKEY;
    ble_fake_shown = ble_fake_auth ? (uint32_t)(r[0] | r[1] << 8 | r[2] << 16) % BLE_FAKE_PASSKEYS
                                   : BLE_NO_PASSKEY;
    ble_fake_pair_at = ble_fake_now;
}

static void ble_fake_pair_step(void) {
    if (ble_fake_auth && ble_fake_typed == BLE_NO_PASSKEY) {
        return;
    }
    if (ble_fake_now - ble_fake_pair_at < BLE_FAKE_PAIR_MS) {
        return;
    }
    uint8_t mrand[16], srand[16], mconf[16], sconf[16], stk[16];
    ble_fake_random(1, mrand);
    ble_fake_random(2, srand);
    uint32_t typed = ble_fake_auth ? ble_fake_typed : BLE_NO_PASSKEY;
    ble_fake_confirm(ble_fake_shown, srand, mconf);
    ble_fake_confirm(typed, srand, sconf);
    if (!ble_eq(mconf, sconf, 16)) {
        ble_fake_leave(BLE_CF_PAIRING);
        return;
    }
    uint8_t tk[16] = {0};
    if (ble_fake_auth) {
        ble_smp_passkey_tk(typed, tk);
    }
    ble_smp_s1(tk, srand, mrand, stk);
    struct ble_keys k;
    k.has = (uint8_t)(BLE_KEYS_LTK | (ble_fake_auth ? BLE_KEYS_AUTH : 0u));
    ble_fake_random(3, k.ltk);
    ble_aes128(stk, k.ltk, ble_fake_ltk);
    ble_cpy(k.ltk, ble_fake_ltk, 16);
    ble_cpy(k.rand, srand, 8);
    k.ediv = (uint16_t)(mrand[0] | mrand[1] << 8);
    ble_fake_bonded = 1;
    ble_fake_forgot = 0;
    ble_fake_pairings++;
    ble_fake_pairing = 0;
    ble_app_central_keys(&k);
    ble_fake_cs = BLE_CS_READY;
    ble_app_state();
}

static void ble_fake_secure_setup(void) {
    if (ble_fake_pairing) {
        ble_fake_pair_step();
    } else if (!ble_fake_peer.bonded) {
        ble_fake_pair_start();
    } else if (ble_fake_bonded && !ble_fake_forgot && ble_eq(ble_fake_peer.ltk, ble_fake_ltk, 16)) {
        ble_fake_encryptions++;
        ble_fake_cs = BLE_CS_READY;
        ble_app_state();
    } else {
        ble_fake_leave(BLE_CF_KEY);
    }
}

static int ble_fake_target_heard(void) {
    for (uint32_t i = 0; i < BLE_FAKE_PDUS; i++) {
        const volatile uint8_t* p = ble_fake_pdu[i];
        uint32_t tx = p[0] >> 6 & 1u;
        if (ble_fake_slot_heard(i) && tx == ble_fake_target_rand &&
            ble_eq((const uint8_t*)p + 2, ble_fake_target, 6)) {
            return 1;
        }
    }
    return 0;
}

static void ble_fake_midi_in(void* ctx, uint32_t pkt, uint16_t ts) {
    ble_app_midi_in(pkt, ts, *(const uint16_t*)ctx);
}

static void ble_fake_notify(void) {
    uint8_t p[BLE_FAKE_MIDI];
    uint16_t n = ble_fake_midi_len;
    for (uint32_t i = 0; i < n; i++) {
        p[i] = ble_fake_midi[i];
    }
    ble_fake_midi_len = 0;
    uint16_t last = ble_midi_last_ts(p, n);
    (void)ble_midi_dec(&ble_fake_dec, p, n, ble_fake_midi_in, &last);
}

static void ble_fake_link(uint8_t central) {
    ble_fake_ll = BLE_FAKE_LINKED;
    ble_fake_central = central;
    ble_fake_cs = central ? BLE_CS_SETUP : ble_fake_cs;
    ble_midi_dec_reset(&ble_fake_dec);
    ble_app_state();
}

static void ble_radio_air(uint32_t now_ms) {
    ble_fake_now = now_ms;
    uint8_t ll = ble_fake_ll;
    int gone = ble_fake_central ? ble_fake_away : !ble_fake_computer;
    if (ll == BLE_FAKE_LINKED && (ble_fake_leaving || gone)) {
        ble_fake_closed();
    } else if (ll == BLE_FAKE_ADVERTISING && ble_fake_computer) {
        ble_fake_link(0);
    } else if (ll == BLE_FAKE_INITIATING && ble_fake_target_heard()) {
        ble_fake_link(1);
    } else if (ble_fake_cs == BLE_CS_SETUP && ble_fake_secure && !ble_fake_leaving) {
        ble_fake_secure_setup();
    } else if (ble_fake_cs == BLE_CS_SETUP && !ble_fake_secure) {
        ble_fake_cs = BLE_CS_READY;
        ble_app_state();
    } else if (ll == BLE_FAKE_LINKED && ble_fake_midi_len) {
        ble_fake_notify();
    }
}
