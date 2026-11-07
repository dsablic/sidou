// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "patch_level.h"

#include <string.h>

#define NOTES 3u
#define HOLD_BLOCKS 100u
#define RELEASE_BLOCKS 20u
#define READY_BLOCKS 1000u
#define HEARD_RMS 512u
#define KEPT 8u

static const uint8_t NOTE[NOTES] = {24u, 36u, 48u};
static const uint16_t ATTACK_MS[16] = {2, 8, 16, 24, 38, 56, 68, 80, 100, 250, 500, 800, 1000, 3000, 5000, 8000};

static void block(patch_level_rig* r) {
    song_ctl_poll(&r->sc, SONG_POLL_SLICE | SONG_POLL_RING);
    song_ctl_commit(&r->sc);
    song_ctl_block(&r->sc, &r->chip);
    song_ctl_render(&r->sc, &r->chip, r->pcm, SONG_BLOCK);
    song_ctl_block_end(&r->sc, r->pcm, SONG_BLOCK);
}

static void loudest(patch_level_rig* r, uint32_t blocks, uint64_t top[KEPT]) {
    for (uint32_t b = 0; b < blocks; b++) {
        block(r);
        int64_t mean = 0;
        for (uint32_t i = 0; i < SONG_BLOCK; i++) {
            mean += r->pcm[i];
        }
        mean /= (int64_t)SONG_BLOCK;
        uint64_t sum = 0;
        for (uint32_t i = 0; i < SONG_BLOCK; i++) {
            int64_t d = r->pcm[i] - mean;
            sum += (uint64_t)(d * d);
        }
        for (uint32_t k = 0; k < KEPT; k++) {
            if (sum > top[k]) {
                uint64_t t = top[k];
                top[k] = sum;
                sum = t;
            }
        }
    }
}

static uint32_t root(uint64_t v) {
    uint64_t x = 0, bit = 1ull << 62;
    while (bit > v) {
        bit >>= 2;
    }
    for (; bit; bit >>= 2) {
        if (v >= x + bit) {
            v -= x + bit;
            x = (x >> 1) + bit;
        } else {
            x >>= 1;
        }
    }
    return (uint32_t)x;
}

uint32_t patch_level_rms(patch_level_rig* r, gt2_song* s, uint32_t instr, uint32_t model) {
    memset(r, 0, sizeof *r);
    if (instr < 1u || instr > GT2_MAX_INSTR) {
        return 0;
    }
    sid_character_init(&r->character);
    sid_init(&r->chip, model ? SID_MODEL_8580 : SID_MODEL_6581, SID_CLOCK_PAL, PATCH_LEVEL_RATE);
    sid_character_apply(&r->chip, &r->character, &r->dac);
    song_ctl_init(&r->sc, s, &r->shapes[0], &r->shapes[1], &r->work);
    if (!song_ctl_audition(&r->sc)) {
        return 0;
    }
    for (uint32_t i = 0; i < READY_BLOCKS && !song_ctl_jam_ready(&r->sc); i++) {
        block(r);
    }
    uint64_t top[KEPT] = {0};
    uint32_t attack = ATTACK_MS[s->instr[instr - 1u][GT2_INS_AD] >> 4];
    uint32_t rise = attack * (PATCH_LEVEL_RATE / 1000u) / SONG_BLOCK;
    for (uint32_t n = 0; n < NOTES; n++) {
        song_ctl_jam(&r->sc, 0, NOTE[n], instr);
        loudest(r, n == 0u && rise > HOLD_BLOCKS ? rise : HOLD_BLOCKS, top);
        song_ctl_jam(&r->sc, 0, SONG_JAM_RELEASE, 0);
        loudest(r, RELEASE_BLOCKS, top);
    }
    return root(top[KEPT - 1u] / SONG_BLOCK);
}

static void add(char** at, char* end, const char* t) {
    while (*t && *at + 1 < end) {
        *(*at)++ = *t++;
    }
    **at = 0;
}

int patch_level_check(patch_level_rig* r, gt2_song* s, uint32_t first, uint32_t count,
                      uint32_t style, char* error, uint32_t max) {
    int quiet = 0;
    error[0] = 0;
    for (uint32_t k = 0; k < count; k++) {
        uint32_t heard = (patch_level_rms(r, s, first + k, 0) >= HEARD_RMS) |
                         (patch_level_rms(r, s, first + k, 1) >= HEARD_RMS) << 1;
        if (heard == 3u || (quiet && heard)) {
            continue;
        }
        char* at = error;
        char* end = error + max;
        char num[4] = {(char)('0' + k / 10u), (char)('0' + k % 10u), 0, 0};
        add(&at, end, style == PATCH_LEVEL_BANK ? "patches[" : style == PATCH_LEVEL_STEP ? "instrument" : "the patch");
        if (style == PATCH_LEVEL_BANK) {
            add(&at, end, k < 10u ? num + 1 : num);
            add(&at, end, "]");
        }
        add(&at, end, heard ? " is silent on the " : " is silent on both chips");
        add(&at, end, heard == 1u ? "8580" : heard == 2u ? "6581" : "");
        add(&at, end, ": nothing was heard playing it. Check the wave (none plays nothing; on a 6581 two "
                      "waves together are very quiet), the envelope (a sustain of 0 needs a decay), the "
                      "filter (a cutoff far below the notes with no filter envelope to open it, or a "
                      "high-pass far above them), the pulse width (0 or 100 is silent) and a volume LFO.");
        if (!heard) {
            return -1;
        }
        quiet = 1;
    }
    return quiet;
}
