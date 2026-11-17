// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "song_ratchet.h"

#include <string.h>

#define WAVE_FIRST 0x10u
#define WAVE_END 0xE0u
#define GATE 0x01u
#define DELAY_END 0x10u
#define JUMP 0xFFu
#define NO_NOTE_CHANGE 0x80u
#define NAME_TAIL 3u
#define GATETIMER_FRAMES 0x3Fu
#define RATCHET_GATETIMER 1u

static int gated_wave(uint8_t left) {
    return left >= WAVE_FIRST && left < WAVE_END && (left & GATE);
}

static const uint8_t* wave_row(const gt2_song* s, uint32_t row, uint32_t side) {
    return row < GT2_TABLE_ROWS ? (side ? &s->rtable[GT2_WTBL][row] : &s->ltable[GT2_WTBL][row])
                                : NULL;
}

static uint8_t left_at(const gt2_song* s, uint32_t row) {
    const uint8_t* b = wave_row(s, row, 0u);
    return b ? *b : 0u;
}

uint32_t song_ratchet_rows(const gt2_song* s, uint32_t base, uint32_t count, uint32_t row_frames,
                           uint8_t left[SONG_RATCHET_ROWS], uint8_t right[SONG_RATCHET_ROWS]) {
    uint32_t wave = base && base <= GT2_MAX_INSTR ? s->instr[base - 1u][GT2_INS_WAVE] : 0u;
    uint32_t hit = count > SONG_RATCHET_OFF && count <= SONG_RATCHET_MAX ? row_frames / count : 0u;
    if (!wave || hit < SONG_RATCHET_MIN_FRAMES || !gated_wave(left_at(s, wave - 1u))) {
        return 0u;
    }
    uint8_t on = left_at(s, wave - 1u), note = *wave_row(s, wave - 1u, 1u);
    uint32_t n = 0u;
    for (uint32_t k = 1u; k < count; k++) {
        if (hit > SONG_RATCHET_MIN_FRAMES) {
            left[n] = (uint8_t)(hit - SONG_RATCHET_MIN_FRAMES - 1u);
            right[n++] = NO_NOTE_CHANGE;
        }
        left[n] = (uint8_t)(on & ~GATE);
        right[n++] = note;
        left[n] = on;
        right[n++] = note;
    }
    left[n] = JUMP;
    right[n++] = (uint8_t)wave;
    return n;
}

static uint32_t hits_of(const gt2_song* s, uint32_t wave, uint32_t* jump) {
    uint32_t r = wave - 1u, hits = SONG_RATCHET_OFF;
    while (hits < SONG_RATCHET_MAX) {
        uint32_t off = r + (left_at(s, r) < DELAY_END ? 1u : 0u);
        uint8_t on = left_at(s, off + 1u);
        if (!gated_wave(on) || left_at(s, off) != (uint8_t)(on & ~GATE)) {
            break;
        }
        r = off + 2u;
        hits++;
    }
    *jump = r;
    return hits;
}

uint32_t song_ratchet_count(const gt2_song* s, uint32_t instr) {
    uint32_t wave = instr && instr <= GT2_MAX_INSTR ? s->instr[instr - 1u][GT2_INS_WAVE] : 0u, r;
    if (!wave) {
        return SONG_RATCHET_OFF;
    }
    uint32_t hits = hits_of(s, wave, &r);
    return hits > SONG_RATCHET_OFF && left_at(s, r) == JUMP && *wave_row(s, r, 1u) ? hits
                                                                                 : SONG_RATCHET_OFF;
}

static int same_sound(const uint8_t* a, const uint8_t* b) {
    for (uint32_t k = 0; k < GT2_INS_GATETIMER; k++) {
        if (k != GT2_INS_WAVE && a[k] != b[k]) {
            return 0;
        }
    }
    return 1;
}

uint32_t song_ratchet_base(const gt2_song* s, uint32_t instr) {
    if (song_ratchet_count(s, instr) == SONG_RATCHET_OFF) {
        return instr;
    }
    uint32_t r;
    (void)hits_of(s, s->instr[instr - 1u][GT2_INS_WAVE], &r);
    uint8_t target = *wave_row(s, r, 1u);
    for (uint32_t b = 1u; b <= GT2_MAX_INSTR; b++) {
        const uint8_t* in = s->instr[b - 1u];
        if (b != instr && in[GT2_INS_WAVE] == target && same_sound(in, s->instr[instr - 1u])) {
            return b;
        }
    }
    return instr;
}

uint32_t song_ratchet_copy(const gt2_song* s, uint32_t base, uint32_t count, uint32_t row_frames) {
    uint8_t left[SONG_RATCHET_ROWS], right[SONG_RATCHET_ROWS];
    uint32_t n = song_ratchet_rows(s, base, count, row_frames, left, right);
    for (uint32_t c = 1u; n && c <= GT2_MAX_INSTR; c++) {
        const uint8_t* in = s->instr[c - 1u];
        uint32_t w = in[GT2_INS_WAVE];
        if (c == base || !w || w - 1u + n > GT2_TABLE_ROWS ||
            !same_sound(in, s->instr[base - 1u])) {
            continue;
        }
        if (!memcmp(&s->ltable[GT2_WTBL][w - 1u], left, n) &&
            !memcmp(&s->rtable[GT2_WTBL][w - 1u], right, n)) {
            return c;
        }
    }
    return 0u;
}

void song_ratchet_instr(const gt2_song* s, uint32_t base, uint32_t count, uint32_t wave,
                        uint8_t out[GT2_INSTR_BYTES]) {
    memcpy(out, s->instr[base - 1u], GT2_INSTR_BYTES);
    uint32_t first = s->instr[base - 1u][GT2_INS_WAVE];
    uint8_t timer = (uint8_t)(out[GT2_INS_GATETIMER] & GATETIMER_FRAMES);
    out[GT2_INS_WAVE] = (uint8_t)wave;
    out[GT2_INS_FIRSTWAVE] = left_at(s, first - 1u);
    out[GT2_INS_GATETIMER] = (uint8_t)(out[GT2_INS_GATETIMER] & ~GATETIMER_FRAMES) |
                             (timer > RATCHET_GATETIMER ? RATCHET_GATETIMER : timer);
    uint32_t len = 0u, room = GT2_INSTR_BYTES - GT2_INS_NAME - NAME_TAIL;
    while (len < room && out[GT2_INS_NAME + len]) {
        len++;
    }
    uint8_t* tail = out + GT2_INS_NAME + len;
    tail[0] = ' ';
    tail[1] = 'x';
    tail[2] = (uint8_t)('0' + count);
}
