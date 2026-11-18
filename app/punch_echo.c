// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "punch_echo.h"

#include "fx_live.h"

static void send(const punch_echo* e, uint8_t reg, uint8_t value) {
    e->l->sink(e->l->sink_ctx, reg, value);
}

static void field(const punch_echo* e, uint32_t f, uint8_t value) {
    send(e, (uint8_t)LIVE_REG_SLOT(SID_SLOT_FX, f), value);
}

static int is_delay(uint32_t algorithm) {
    return algorithm == SID_FXSLOT_DELAY || algorithm == SID_FXSLOT_PINGPONG;
}

static void throw_now(punch_echo* e) {
    const uint8_t* own = e->l->slot[SID_SLOT_FX];
    uint32_t alg = is_delay(own[LIVE_SLOT_ALGORITHM]) ? own[LIVE_SLOT_ALGORITHM] : SID_FXSLOT_DELAY;
    const uint8_t* shape = live_fx_defaults[SID_SLOT_FX][alg];
    int keep = is_delay(own[LIVE_SLOT_ALGORITHM]);
    field(e, LIVE_SLOT_ALGORITHM, (uint8_t)alg);
    field(e, LIVE_SLOT_AMOUNT, PUNCH_ECHO_FEEDBACK);
    field(e, LIVE_SLOT_A, keep ? own[LIVE_SLOT_A] : shape[LIVE_SLOT_A - 1u]);
    field(e, LIVE_SLOT_B, keep ? own[LIVE_SLOT_B] : shape[LIVE_SLOT_B - 1u]);
    field(e, LIVE_SLOT_MIX, PUNCH_ECHO_MIX);
    send(e, LIVE_REG_FX_SEND, LIVE_SLOT_MAX);
    e->state = PUNCH_ECHO_THROWING;
}

static void give_back(punch_echo* e) {
    for (uint32_t f = 0; f < LIVE_SLOT_FIELDS; f++) {
        field(e, f, e->l->slot[SID_SLOT_FX][f]);
    }
    send(e, LIVE_REG_FX_SEND, LIVE_SLOT_MAX);
    e->state = PUNCH_ECHO_OFF;
}

void punch_echo_init(punch_echo* e, live* l) {
    e->l = l;
    e->state = PUNCH_ECHO_OFF;
    e->until = 0;
}

void punch_echo_hold(punch_echo* e, int on, uint32_t now_ms) {
    if (on && e->state != PUNCH_ECHO_THROWING) {
        throw_now(e);
    } else if (!on && e->state == PUNCH_ECHO_THROWING) {
        send(e, LIVE_REG_FX_SEND, 0);
        e->state = PUNCH_ECHO_TAIL;
        e->until = now_ms + PUNCH_ECHO_TAIL_MS;
    }
}

void punch_echo_tick(punch_echo* e, uint32_t now_ms) {
    if (e->state == PUNCH_ECHO_TAIL && (int32_t)(now_ms - e->until) >= 0) {
        give_back(e);
    }
}
