// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "live.h"

#include "fx/fx_slots.h"

/* Amount, A, B, Mix per slot and algorithm: what an algorithm picked from the panel starts
 * from (live_slot_pick), each one audible at once. NONE's row is zeros. Rows past a slot's
 * last algorithm are zeros too. */
const uint8_t live_fx_defaults[LIVE_SLOTS][8][4] = {
    [0] = {
        [SID_DIST_TAPE] = {80, 0, 0, 127},
        [SID_DIST_OVERDRIVE] = {52, 64, 32, 127},
        [SID_DIST_FUZZ] = {48, 0, 32, 127},
        [SID_DIST_CRUSH] = {64, 32, 0, 127},
        [SID_DIST_LOFI] = {64, 16, 16, 127},
    },
    [1] = {
        [SID_MOD_CHORUS] = {64, 32, 0, 64},
        [SID_MOD_FLANGER] = {64, 24, 96, 64},
        [SID_MOD_PHASER] = {64, 32, 64, 64},
        [SID_MOD_TREMOLO] = {100, 64, 0, 127},
        [SID_MOD_AUTOPAN] = {96, 32, 0, 127},
        [SID_MOD_WOW] = {48, 32, 0, 127},
    },
    [2] = {
        [SID_FXSLOT_DELAY] = {48, 64, 48, 40},
        [SID_FXSLOT_PINGPONG] = {48, 64, 48, 40},
        [SID_FXSLOT_REVERB_LIGHT] = {72, 48, 80, 40},
        [SID_FXSLOT_REVERB_LUSH] = {80, 48, 96, 40},
    },
};
