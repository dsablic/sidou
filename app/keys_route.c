// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "keys_route.h"

#include "sid/sid_character.h"

#define REG_FC_LO 0x15u
#define REG_MODE_VOL 0x18u

/* KEYS A during a song: the keys play chip A, which keeps the song's setup and filter. */
static int keys_under_song(const keys_route* kr) {
    return kr->song && kr->l->keys_chip == 0u && kr->l->keys_mode == LIVE_KEYS_A;
}

/* live's sink: the keys' chip's own (no layer in between, so the instrument's writes cost what
 * they did in M1), or, under a song with KEYS A, keys_route_sink, which holds back what the
 * song keeps. */
static void retarget(keys_route* kr) {
    live* l = kr->l;
    if (keys_under_song(kr)) {
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
    kr->stale = 0;
    kr->under = 0;
    kr->moved = 0;
    kr->moved_ctx = 0;
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
           (reg >= LIVE_REG_CHAR(0) && reg < LIVE_REG_CHAR(LIVE_CHAR_FIELDS));
}

void keys_route_sink(void* ctx, uint8_t reg, uint8_t value) {
    keys_route* kr = ctx;
    if (keys_under_song(kr) && (setup_reg(reg) || (reg >= REG_FC_LO && reg <= REG_MODE_VOL))) {
        return; /* the song's setup, filter and volume */
    }
    kr->sink(kr->out[kr->l->keys_chip & 1u], reg, value);
}

int keys_route_prepare(void* ctx, uint8_t profile) {
    keys_route* kr = ctx;
    if (keys_under_song(kr)) {
        return 1; /* the song's chip keeps its setup: nothing to prepare */
    }
    return kr->prepare(kr->out[kr->l->keys_chip & 1u], profile);
}

/* Whether the keys play under a song (KEYS A) changes with the song and the setting: when it
 * starts, chip A has the keys' setup as the song takes it; it keeps it (or the song's, M2c)
 * until it ends, and is then taken back to the keys' (keys_route_tick). */
static void update_under(keys_route* kr) {
    uint8_t now = (uint8_t)keys_under_song(kr);
    if (now && !kr->under) {
        chip_setup_of(kr->l, &kr->ctl[0].has);
        kr->stale = 1;
    }
    if (now != kr->under) {
        kr->under = now;
        retarget(kr);
    }
}

void keys_route_set_song(keys_route* kr, int playing) {
    kr->song = playing ? 1u : 0u;
    update_under(kr);
}

uint8_t keys_route_voices_taken(const keys_route* kr) {
    if (!keys_under_song(kr)) {
        return 0;
    }
    uint8_t m = 0;
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        if (kr->l->voice[v].gate) {
            m = (uint8_t)(m | 1u << v);
        }
    }
    return m;
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
        kr->prepare(kr->out[from], LIVE_PREPARE_CANCEL); /* asked again on the new chip */
    }
    chip_ctl_stop(&kr->ctl[to]);
    /* The chip left behind has the keys' setup, as live gave it. */
    chip_setup_of(l, &kr->ctl[from].has);
    kr->ctl[from].made = 1;
    if (to == 1u) {
        /* The song takes chip A: the notes the keys hold there are released. */
        for (uint32_t v = 0; v < LIVE_VOICES; v++) {
            uint8_t c = live_voice_control(l, v);
            if (c & 1u) {
                kr->sink(kr->out[0], (uint8_t)(v * 7u + 4u), (uint8_t)(c & ~1u));
            }
        }
    }
    l->keys_chip = (uint8_t)to;
    retarget(kr);
    if (kr->moved) {
        kr->moved(kr->moved_ctx, to);
    }
    live_rewrite(l); /* every register on the new chip: the keys still down sound again */
}

/* The other chip's model and profile, for the views. */
static void tell(keys_route* kr) {
    live* l = kr->l;
    const chip_ctl* o = &kr->ctl[(l->keys_chip & 1u) ^ 1u];
    const sid_chip_profile* p = sid_chip_profile_get(o->has.profile);
    l->other_profile = o->has.profile;
    l->other_model = p ? p->model : l->other_model;
}

void keys_route_tick(keys_route* kr) {
    live* l = kr->l;
    update_under(kr); /* the KEYS setting may have changed */
    chip_setup want;
    chip_setup_of(l, &want);
    uint32_t keys = l->keys_chip & 1u;
    uint32_t target = kr->song && l->keys_mode == LIVE_KEYS_AUTO ? 1u : 0u;
    uint32_t other = keys ^ 1u;
    if (target != keys && !kr->stale && chip_ctl_has(&kr->ctl[target], &want) &&
        (target == 1u || !keys_held(l)) && !l->preparing) {
        move(kr, target);
        keys = target;
        other = keys ^ 1u;
    } else if (!(other == 0u && kr->song)) {
        /* The other chip follows the keys' setup, unless it is the song's. */
        chip_ctl_follow(&kr->ctl[other], &want);
    } else {
        chip_ctl_stop(&kr->ctl[other]);
    }
    chip_ctl* own = &kr->ctl[keys];
    if (keys_under_song(kr)) {
        /* chip A keeps what the song left it */
    } else if (kr->stale) {
        /* The keys' own chip after a song kept its setup (KEYS A): back to the keys' setup
         * (its profile prepared and reset, then every register again; or only the settings
         * that differ), then live's writes keep it there. */
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
        chip_setup_of(l, &own->has); /* live keeps its own chip at the keys' setup */
        own->made = 1;
    }
    tell(kr);
}
