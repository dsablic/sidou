// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song player's audio-interrupt cost (M2c Task 6, PLAYER_WORST_BLOCK): song_ctl's block
 * as platform/fm1/audio.c runs it (the commands with SONG_EDITS_PER_BLOCK edits, the call
 * if one falls in the block, the block's end), its writes into a real chip, without the
 * chip's render. The emulator test (emu-gate tests/song.rs) puts the song's region at
 * SONG_ADDR (a song read and laid out by the host, as the device's region) and the
 * parameters at PARAM_ADDR before it starts: [0] the subtune, [1] the blocks, [2] the edits'
 * kind (0 4-byte cell writes, 1 index swaps, 2 none). Marks: the start, the play's analysis (every
 * slice), a derivation of the whole shape (a widening's worst), then each block's start and
 * end. RESULT_ADDR [2] gets song_ctl's error, [3] the calls made. */
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
    volatile uint32_t* result = (volatile uint32_t*)RESULT_ADDR;
    volatile uint32_t* param = (volatile uint32_t*)PARAM_ADDR;
    gt2_song* song = (gt2_song*)SONG_ADDR;
    uint32_t blocks = param[1], kind = param[2];

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
                edit(song, kind);
            }
        }
        song_ctl_commit(&sc);
        bench_mark();
        song_ctl_block(&sc, &chip);
        uint32_t at = song_ctl_due(&sc, SONG_BLOCK);
        if (at < SONG_BLOCK) {
            song_ctl_call(&sc);
        }
        song_ctl_block_end(&sc, SONG_BLOCK);
        bench_mark();
        song_ctl_poll(&sc, 0u);
    }
    result[1] = 0;
    result[2] = (uint32_t)(int32_t)sc.error;
    result[3] = sc.calls;
    result[0] = RESULT_MAGIC;
    for (;;) {
    }
}
