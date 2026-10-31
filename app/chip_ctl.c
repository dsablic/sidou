// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "chip_ctl.h"

#include "sid/sid_character.h"

void chip_setup_of(const live* l, chip_setup* out) {
    out->profile = l->profile;
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
        out->controls[f] = l->character[f];
    }
    out->gate_reset = l->envelope_mode == LIVE_ENVELOPE_INSTANT ? 1u : 0u;
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        out->expert[f] = l->expert[f];
    }
}

void chip_setup_control(chip_setup* setup, uint32_t field, uint8_t value) {
    if (field >= LIVE_CHAR_FIELDS - 1u || setup->controls[field] == value) {
        return;
    }
    sid_character c;
    sid_character_init(&c);
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        sid_character_expert_set(&c, SID_MODEL_6581, f, setup->expert[f]);
    }
    sid_character_macro_changed(&c, field + 1u);
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        setup->expert[f] = c.expert[f];
    }
    setup->controls[field] = value;
}

void chip_ctl_init(chip_ctl* c, live_sink_fn sink, void* sink_ctx, live_prepare_fn prepare,
                   void* prepare_ctx) {
    c->sink = sink;
    c->sink_ctx = sink_ctx;
    c->prepare = prepare;
    c->prepare_ctx = prepare_ctx;
    c->has.profile = 0;
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
        c->has.controls[f] = 0;
    }
    c->has.gate_reset = 0;
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        c->has.expert[f] = SID_EXPERT_DEFAULT;
    }
    c->made = 0;
    c->preparing = 0;
}

void chip_ctl_saw(chip_ctl* c, uint8_t reg, uint8_t value) {
    if (reg == LIVE_REG_CHAR(0)) {
        c->has.profile = value;
    } else if (reg > LIVE_REG_CHAR(0) && reg < LIVE_REG_CHAR(LIVE_CHAR_FIELDS)) {
        c->has.controls[reg - LIVE_REG_CHAR(1)] = value;
    } else if (reg >= LIVE_REG_EXPERT_FIRST && reg < LIVE_REG_EXPERT(SID_EXPERT_COUNT)) {
        c->has.expert[reg - LIVE_REG_EXPERT_FIRST] = value;
    } else if (reg == LIVE_REG_GATE_RESET) {
        c->has.gate_reset = value ? 1u : 0u;
    } else if (reg == LIVE_REG_RESET) {
        c->made = 1;
    }
}

int chip_ctl_has(const chip_ctl* c, const chip_setup* want) {
    if (!c->made || c->has.profile != want->profile || c->has.gate_reset != want->gate_reset) {
        return 0;
    }
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
        if (c->has.controls[f] != want->controls[f]) {
            return 0;
        }
    }
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        if (c->has.expert[f] != want->expert[f]) {
            return 0;
        }
    }
    return 1;
}

static void send(chip_ctl* c, uint8_t reg, uint8_t value) {
    c->sink(c->sink_ctx, reg, value);
    chip_ctl_saw(c, reg, value);
}

__attribute__((noinline, unused)) static void send_expert(chip_ctl* c, const chip_setup* want) {
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        if (c->has.expert[f] != want->expert[f]) {
            send(c, (uint8_t)LIVE_REG_EXPERT(f), want->expert[f]);
        }
    }
}

int chip_ctl_follow(chip_ctl* c, const chip_setup* want) {
    const sid_chip_profile* p = sid_chip_profile_get(want->profile);
    if (!p) {
        return 0;
    }
    send_expert(c, want);
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
        if (!c->made || c->has.controls[f] != want->controls[f]) {
            send(c, (uint8_t)LIVE_REG_CHAR(f + 1u), want->controls[f]);
        }
    }
    if (!c->made || c->has.profile != want->profile) {
        c->preparing = 1;
        if (!c->prepare(c->prepare_ctx, want->profile)) {
            if (c->made) {
                send(c, LIVE_REG_CHAR_TICK, 0);
            }
            return 0;
        }
        c->preparing = 0;
        send(c, LIVE_REG_CHAR(0), want->profile);
        send(c, LIVE_REG_RESET, p->model);
        send(c, LIVE_REG_GATE_RESET, want->gate_reset);
    } else if (c->has.gate_reset != want->gate_reset) {
        send(c, LIVE_REG_GATE_RESET, want->gate_reset);
    }
    send(c, LIVE_REG_CHAR_TICK, 0);
    return chip_ctl_has(c, want);
}

void chip_ctl_send_settings(chip_ctl* c, const chip_setup* want) {
    send_expert(c, want);
    for (uint32_t f = 0; f + 1u < LIVE_CHAR_FIELDS; f++) {
        if (c->has.controls[f] != want->controls[f]) {
            send(c, (uint8_t)LIVE_REG_CHAR(f + 1u), want->controls[f]);
        }
    }
    if (c->has.gate_reset != want->gate_reset) {
        send(c, LIVE_REG_GATE_RESET, want->gate_reset);
    }
}

void chip_ctl_stop(chip_ctl* c) {
    if (c->preparing) {
        c->prepare(c->prepare_ctx, LIVE_PREPARE_CANCEL);
        c->preparing = 0;
    }
}
