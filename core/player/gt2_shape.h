// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_GT2_SHAPE_H
#define SIDOU_GT2_SHAPE_H

#include <stdint.h>

#include "player/gt2_play.h"
#include "song/gt2_song.h"
#include "song/song_ext.h"

#ifdef __cplusplus
extern "C" {
#endif

#define GT2_PATTERN_WORDS ((GT2_MAX_PATTERNS + 31u) / 32u)
#define GT2_TABLE_WORDS 8u

typedef struct {
    uint32_t packed;
    uint32_t pattused[GT2_PATTERN_WORDS];
    uint32_t named[GT2_PLAY_CHANNELS][GT2_PATTERN_WORDS];
    uint32_t scanned[GT2_PATTERN_WORDS];
    uint32_t filled[GT2_PATTERN_WORDS];
    uint32_t instrused[2];
    uint32_t tableused[GT2_TABLES][GT2_TABLE_WORDS];
    uint32_t stbl_calc[GT2_TABLE_WORDS];
    uint32_t seen;
    uint32_t tick0;
    uint8_t transdown, transup;
    uint8_t note_lo, note_hi;
} gt2_base;

enum {
    GT2_SEEN_EFFECTS = 1u << 0,
    GT2_SEEN_FILTER = 1u << 1,
    GT2_SEEN_TRANS = 1u << 2,
    GT2_SEEN_REPEAT = 1u << 3
};

typedef struct {
    const gt2_song* song;
    const gt2_edit* pending;
    uint32_t pending_count;
    uint16_t player_flags, hr_adsr;
    uint8_t mult, opt_pulse, opt_realtime, subtune;
    uint32_t a4_millihz;
    gt2_base base;
    uint8_t phase, i_sub, i_ch, i_row;
    uint16_t i_pos, i_pat;
    int16_t result;
    uint8_t grew;
    uint8_t d_phase, d_ch;
    uint16_t d_i;
    uint32_t d_seen;
    uint32_t d_tick0;
    uint8_t count[3], lowest[3];
    uint8_t rel_any, rel_hi, abs_any, abs_lo, abs_hi;
    const uint8_t* xwave;
    uint8_t xwave_n;
} gt2_analysis;

enum {
    GT2_PLAY_ERR_SUBTUNE = -1,
    GT2_PLAY_ERR_STEREO = -2
};
#define GT2_SHAPE_MORE 0
#define GT2_SHAPE_DONE 1

int gt2_shape_begin(gt2_analysis* a, const gt2_song* s, uint32_t subtune, const gt2_options* o);
int gt2_shape_step(gt2_analysis* a, gt2_shape* out, uint32_t budget);
int gt2_shape_analyse(gt2_analysis* a, const gt2_song* s, uint32_t subtune,
                      const gt2_options* o, gt2_shape* out);

int gt2_shape_widen(gt2_analysis* a, const gt2_edit* e);
int gt2_shape_include_jams(gt2_analysis* a, const uint32_t instruments[2], uint32_t lo,
                          uint32_t hi);
void gt2_shape_derive(gt2_analysis* a, gt2_shape* out);
void gt2_shape_derive_begin(gt2_analysis* a);
int gt2_shape_derive_step(gt2_analysis* a, gt2_shape* out, uint32_t budget);

int gt2_shape_compatible(const gt2_shape* from, const gt2_shape* to);

void gt2_shape_keep_channels(gt2_shape* sh, uint32_t channels);

uint8_t gt2_shape_order_byte(const gt2_analysis* a, uint32_t ch, uint32_t pos);

int gt2_play_init(gt2_player* p, gt2_shape* shape, const gt2_song* s, uint32_t subtune,
                  const gt2_options* o);

#ifdef __cplusplus
}
#endif

#endif
