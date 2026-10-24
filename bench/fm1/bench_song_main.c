// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song player's audio-interrupt cost (M2c Task 6, PLAYER_WORST_BLOCK): song_ctl's block
 * as platform/fm1/audio.c runs it (the commands with SONG_EDITS_PER_BLOCK edits, the call
 * if one falls in the block, the block's end), its writes into a real chip, without the
 * chip's render. The emulator test (emu-gate tests/song.rs) puts the song's region at
 * SONG_ADDR (a song read and laid out by the host, as the device's region) and the
 * parameters at PARAM_ADDR before it starts: [0] the subtune, [1] the blocks, [2] the edits'
 * kind (0 name cells, 1 index swaps, 2 none, 3 real row cells on an isolated song).
 * Marks: the start, the play's analysis (every
 * slice), a derivation of the whole shape (a widening's worst), then each block's start and
 * end. RESULT_ADDR [2] gets song_ctl's error, [3] the calls made, [4] the commands sent. */
#include <stddef.h>
#include <stdint.h>

#include "player/song_ctl.h"
#include "sid/sid.h"

#define RESULT_ADDR 0x01C80000u
#define RESULT_MAGIC 0x53494442u /* "SIDB" */
#define SONG_ADDR 0x01CA0000u
#define PARAM_ADDR 0x01CB0000u

extern uint32_t __data_start[], __data_end[], __data_load[], __bss_start[], __bss_end[];

static sid chip;
static song_ctl sc;
static gt2_shape shapes[2];
static gt2_shape scratch;
static song_work work;
static uint32_t row_region[SONG_REGION_BYTES / sizeof(uint32_t)];
static gt2_edit row_edits[SONG_EDITS_PER_BLOCK];
static int16_t block_out[SONG_BLOCK]; /* the block the digi drums add to (none play here) */

__attribute__((noinline)) void bench_mark(void) { __asm__ volatile("" ::: "memory"); }

/* An edit that changes nothing and holds no slot: the costliest kinds of each, as the
 * interrupt applies them. A 4-byte cell over the song's name; a swap of a pattern's index
 * entry that names nothing, to nothing. */
static void edit(gt2_song* s, uint32_t kind) {
    if (kind > 1u) {
        return; /* none: the call alone */
    }
    gt2_edit e;
    uint8_t* z = (uint8_t*)&e;
    for (uint32_t i = 0; i < sizeof e; i++) {
        z[i] = 0;
    }
    e.slot = GT2_SLOT_NONE;
    if (kind == 0u) {
        e.kind = GT2_EDIT_CELL;
        e.n = 4;
        e.offset = offsetof(gt2_song, name);
        for (uint32_t i = 0; i < 4u; i++) {
            e.bytes[i] = (uint8_t)s->name[i];
        }
    } else {
        e.kind = GT2_EDIT_SWAP;
        e.offset = offsetof(gt2_song, pattern) + (GT2_MAX_PATTERNS - 1u) * 4u;
    }
    if (song_ctl_edit(&sc, &e)) {
        s->pending_loose++; /* the bookkeeping a primitive does, which the release undoes */
    }
}

int main(void) {
    for (uint32_t *d = __data_start, *s = __data_load; d < __data_end;) {
        *d++ = *s++;
    }
    for (uint32_t* b = __bss_start; b < __bss_end;) {
        *b++ = 0;
    }
    sid_noise_tables_init();
    volatile uint32_t* result = (volatile uint32_t*)RESULT_ADDR;
    volatile uint32_t* param = (volatile uint32_t*)PARAM_ADDR;
    gt2_song* song = (gt2_song*)SONG_ADDR;
    uint32_t blocks = param[1], kind = param[2];
    gt2_song* row_song = NULL;
    gt2_err row_error = GT2_OK;
    if (kind == 3u) {
        row_song = gt2_song_init(row_region, sizeof row_region);
        if (row_song == NULL) {
            row_error = GT2_ERR_REGION;
        } else {
            const uint8_t seed[4] = {GT2_REST, 0u, 0u, 1u};
            row_error = gt2_set_row(row_song, 0u, 0u, seed, &row_edits[0]);
            if (row_error == GT2_OK)
                gt2_edit_commit(row_song, &row_edits[0]);
        }
    }

    sid_init(&chip, SID_MODEL_6581, SID_CLOCK_PAL, 44118u);
    song_ctl_init(&sc, song, &shapes[0], &shapes[1], &work);
    bench_mark();
    song_ctl_play(&sc, param[0], 0);
    while (song_ctl_state(&sc) == SONG_LOADING) {
        song_ctl_poll(&sc, SONG_POLL_SLICE);
    }
    song_ctl_commit(&sc);
    bench_mark();
    gt2_shape_derive(&work.cur, &scratch);
    bench_mark();
    for (uint32_t b = 0; b < blocks; b++) {
        if (b > 0u) {
            for (uint32_t k = 0; k < SONG_EDITS_PER_BLOCK; k++) {
                if (kind == 3u) {
                    if (row_error == GT2_OK) {
                        const uint8_t cell[4] = {GT2_REST, k & 1u ? 0u : GT2_MAX_INSTR,
                                                 0u, k & 1u ? 1u : 0u};
                        row_error = gt2_set_row(row_song, 0u, 0u, cell, &row_edits[k]);
                    }
                } else {
                    edit(song, kind);
                }
            }
        }
        song_ctl_commit(&sc);
        bench_mark();
        if (b > 0u && kind == 3u && row_error == GT2_OK)
            for (uint32_t k = 0; k < SONG_EDITS_PER_BLOCK; k++)
                gt2_edit_apply(row_song, &row_edits[k]);
        song_ctl_block(&sc, &chip);
        uint32_t at = song_ctl_due(&sc, SONG_BLOCK);
        if (at < SONG_BLOCK) {
            song_ctl_call(&sc);
        }
        song_ctl_block_end(&sc, block_out, SONG_BLOCK);
        bench_mark();
        if (b > 0u && kind == 3u && row_error == GT2_OK)
            for (uint32_t k = 0; k < SONG_EDITS_PER_BLOCK; k++)
                gt2_edit_release(row_song, &row_edits[k]);
        song_ctl_poll(&sc, 0u);
    }
    result[1] = 0;
    result[2] = row_error != GT2_OK ? (uint32_t)row_error : (uint32_t)(int32_t)sc.error;
    result[3] = sc.calls;
    result[4] = sc.cmd_head;
    result[0] = RESULT_MAGIC;
    for (;;) {
    }
}
