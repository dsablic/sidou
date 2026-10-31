// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "keys_route.h"

#include "sid/sid_character.h"

#define REG_FC_LO 0x15u
#define REG_MODE_VOL 0x18u

static int keys_under_song(const keys_route* kr) {
    return kr->song && kr->l->keys_chip == 0u && !kr->tune;
}

static void silent_sink(void* ctx, uint8_t reg, uint8_t value) {
    (void)ctx;
    (void)reg;
    (void)value;
}

static uint8_t held_voices(const live* l) {
    uint8_t m = 0;
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        if (l->voice[v].gate) {
            m = (uint8_t)(m | 1u << v);
        }
    }
    const live_preset* p = live_preset_get(l->preset);
    if (p && p->mono_sync && (m & 1u)) {
        m = (uint8_t)(m | 1u << 2);
    }
    return m;
}

static void retarget(keys_route* kr) {
    live* l = kr->l;
    if (kr->silent) {
        l->sink = silent_sink;
        l->sink_ctx = kr;
    } else if (keys_under_song(kr)) {
        l->sink = keys_route_sink;
        l->sink_ctx = kr;
    } else {
        l->sink = kr->sink;
        l->sink_ctx = kr->out[l->keys_chip & 1u];
    }
}

void keys_route_init(keys_route* kr, live* l, live_sink_fn sink, live_prepare_fn prepare,
                     void* out_a, void* out_b) {
    kr->l = l;
    kr->sink = sink;
    kr->prepare = prepare;
    kr->out[0] = out_a;
    kr->out[1] = out_b;
    chip_ctl_init(&kr->ctl[0], sink, out_a, prepare, out_a);
    chip_ctl_init(&kr->ctl[1], sink, out_b, prepare, out_b);
    kr->song = 0;
    kr->song_on = 0;
    kr->player = 0;
    kr->tune = 0;
    kr->silent = 0;
    kr->stale = 0;
    kr->under = 0;
    kr->taken = 0;
    kr->song_seen = 0;
    kr->song_user = 0;
    kr->song_want = (chip_setup){0};
    for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
        kr->song_want.expert[f] = SID_EXPERT_DEFAULT;
    }
    kr->tune_b_want = kr->song_want;
    kr->moved = 0;
    kr->moved_ctx = 0;
    kr->taking = 0;
    kr->taking_ctx = 0;
    kr->song_edit = 0;
    kr->song_edit_ctx = 0;
}

void keys_route_start(keys_route* kr) {
    kr->l->two_chips = 1;
    kr->l->keys_chip = 0;
    retarget(kr);
    chip_setup_of(kr->l, &kr->ctl[0].has);
    kr->ctl[0].made = 1;
}

static int setup_reg(uint8_t reg) {
    return reg == LIVE_REG_RESET || reg == LIVE_REG_GATE_RESET || reg == LIVE_REG_CHAR_TICK ||
           (reg >= LIVE_REG_CHAR(0) && reg < LIVE_REG_CHAR(LIVE_CHAR_FIELDS)) ||
           (reg >= LIVE_REG_EXPERT_FIRST && reg < LIVE_REG_EXPERT(SID_EXPERT_COUNT));
}

void keys_route_sink(void* ctx, uint8_t reg, uint8_t value) {
    keys_route* kr = ctx;
    if (keys_under_song(kr)) {
        if (setup_reg(reg) || (reg >= REG_FC_LO && reg <= REG_MODE_VOL)) {
            return;
        }
        if (reg < REG_FC_LO) {
            uint32_t v = reg / 7u;
            uint8_t now = held_voices(kr->l);
            uint8_t was = kr->taken;
            kr->taken = now;
            if ((now & ~was) && kr->taking) {
                kr->taking(kr->taking_ctx, now);
            }
            if (!(now & 1u << v) && !((was & 1u << v) && reg == v * 7u + 4u)) {
                return;
            }
        }
    }
    kr->sink(kr->out[kr->l->keys_chip & 1u], reg, value);
}

int keys_route_prepare(void* ctx, uint8_t profile) {
    keys_route* kr = ctx;
    if (keys_under_song(kr) || kr->silent) {
        return 1;
    }
    return kr->prepare(kr->out[kr->l->keys_chip & 1u], profile);
}

static void update_under(keys_route* kr) {
    if (kr->song && !kr->song_seen) {
        keys_route_song_wants(kr, &kr->ctl[0].has);
        kr->song_user = 0;
    }
    kr->song_seen = kr->song;
    uint8_t now = (uint8_t)keys_under_song(kr);
    if (now && !kr->under) {
        kr->stale = 1;
        kr->taken = held_voices(kr->l);
        if (kr->taken && kr->taking) {
            kr->taking(kr->taking_ctx, kr->taken);
        }
    }
    if (now != kr->under) {
        kr->under = now;
        retarget(kr);
    }
}

void keys_route_set_song(keys_route* kr, int playing) {
    kr->song_on = playing ? 1u : 0u;
    kr->song = (uint8_t)(kr->song_on | kr->player);
    update_under(kr);
}

void keys_route_set_player(keys_route* kr, int owns) {
    kr->player = owns ? 1u : 0u;
    kr->song = (uint8_t)(kr->song_on | kr->player);
    update_under(kr);
}

void keys_route_set_tune(keys_route* kr, int on) {
    kr->tune = on ? 1u : 0u;
    update_under(kr);
}

void keys_route_set_silent(keys_route* kr, int on) {
    uint8_t was = kr->silent;
    kr->silent = on ? 1u : 0u;
    if (was && !kr->silent) {
        kr->stale = 1;
    }
    retarget(kr);
}

void keys_route_fresh(keys_route* kr, uint32_t chip) {
    kr->ctl[chip & 1u].made = 0;
}

uint8_t keys_route_voices_taken(const keys_route* kr) {
    return keys_under_song(kr) ? held_voices(kr->l) : 0u;
}

int keys_route_ready_for_song(const keys_route* kr) {
    return kr->l->keys_chip == 1u || (kr->l->keys_mode == LIVE_KEYS_A && !kr->tune) || kr->silent;
}

void keys_route_song_wants(keys_route* kr, const chip_setup* setup) {
    if (setup->profile != kr->song_want.profile) {
        kr->song_user = 0;
    }
    kr->song_want = *setup;
}

int keys_route_song_edit(keys_route* kr, const chip_setup* setup, uint32_t edit_mask) {
    if (kr->song_edit) {
        return kr->song_edit(kr->song_edit_ctx, setup, edit_mask);
    }
    keys_route_song_wants(kr, setup);
    return 1;
}

void keys_route_song_setup(keys_route* kr, const chip_setup* setup) {
    kr->song_user = 0;
    kr->ctl[0].has = *setup;
    kr->ctl[0].made = 1;
    keys_route_song_wants(kr, setup);
}

static int keys_held(const live* l) {
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        if (l->voice[v].gate) {
            return 1;
        }
    }
    return 0;
}

static void move(keys_route* kr, uint32_t to) {
    live* l = kr->l;
    uint32_t from = l->keys_chip & 1u;
    if (l->preparing) {
        kr->prepare(kr->out[from], LIVE_PREPARE_CANCEL);
    }
    chip_ctl_stop(&kr->ctl[to]);
    if (!kr->stale) {
        chip_setup_of(l, &kr->ctl[from].has);
        kr->ctl[from].made = 1;
    }
    if (to == 1u) {
        kr->stale = 0;
        for (uint32_t v = 0; v < LIVE_VOICES; v++) {
            uint8_t c = live_voice_control(l, v);
            if (c & 1u) {
                kr->sink(kr->out[0], (uint8_t)(v * 7u + 4u), (uint8_t)(c & ~1u));
            }
        }
    }
    l->keys_chip = (uint8_t)to;
    if (to == 0u && kr->song) {
        kr->stale = 1;
        kr->taken = held_voices(l);
        kr->under = 1;
    }
    retarget(kr);
    if (kr->moved) {
        kr->moved(kr->moved_ctx, to);
    }
    live_rewrite(l);
}

static void tell(keys_route* kr) {
    live* l = kr->l;
    const chip_ctl* o = &kr->ctl[(l->keys_chip & 1u) ^ 1u];
    const sid_chip_profile* p = o->made ? sid_chip_profile_get(o->has.profile) : 0;
    l->other_profile = o->made ? o->has.profile : LIVE_PROFILE_NONE;
    l->other_model = p ? p->model : l->other_model;
}

void keys_route_tick(keys_route* kr) {
    live* l = kr->l;
    update_under(kr);
    if (kr->silent) {
        if (chip_ctl_follow(&kr->ctl[1], &kr->tune_b_want)) {
            chip_ctl_follow(&kr->ctl[0], &kr->song_want);
        }
        tell(kr);
        return;
    }
    chip_setup want;
    chip_setup_of(l, &want);
    uint32_t keys = l->keys_chip & 1u;
    uint32_t target = kr->song && (l->keys_mode == LIVE_KEYS_AUTO || kr->tune) ? 1u : 0u;
    uint32_t other = keys ^ 1u;
    int ready = target == 1u ? chip_ctl_has(&kr->ctl[1], &want)
                             : !keys_held(l) && (kr->song || chip_ctl_has(&kr->ctl[0], &want));
    if (target != keys && ready && !l->preparing) {
        move(kr, target);
        keys = target;
        other = keys ^ 1u;
    } else if (!(other == 0u && kr->song)) {
        chip_ctl_follow(&kr->ctl[other], &want);
    } else {
        chip_ctl_follow(&kr->ctl[0], &kr->song_want);
    }
    chip_ctl* own = &kr->ctl[keys];
    if (keys_under_song(kr)) {
        chip_ctl_follow(own, &kr->song_want);
    } else if (kr->stale) {
        if (own->has.profile != want.profile) {
            if (!l->preparing && chip_ctl_follow(own, &want)) {
                live_rewrite(l);
                kr->stale = 0;
            }
        } else {
            chip_ctl_send_settings(own, &want);
            live_rewrite(l);
            kr->stale = 0;
        }
    } else {
        chip_setup_of(l, &own->has);
        own->made = 1;
    }
    tell(kr);
}
