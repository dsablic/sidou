// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The packed player's shape (M2c Task 6): what gt2reloc's analysis (greloc.c's relocator())
 * makes of a song, for the player (gt2_play.h) to follow. The main loop's, in flash on the
 * device: it walks the song, so it runs in slices (gt2_shape_step), each within a budget of
 * work units the caller sets from the main loop's allowance.
 *
 * An analysis keeps what it has found as it walks (the base: the patterns and instruments
 * in use, the table rows reached, the commands, notes and transposes seen, which channels
 * hold anything) and derives the shape from it (gt2_shape_derive): the flags, the note
 * range, the layout, the packer's numberings and the subtune's packed orderlist streams.
 *
 * Live edits (decision record 0007, "The player's corners"): an edit can change the shape
 * the packer would give the song (a new note range, a first command of a class, a wave
 * delay or command, a pulse low nybble, a channel's first non-rest). gt2_shape_widen adds
 * what one edit brings to the base, in O(1) for a cell (a row, a table or instrument byte,
 * an orderlist byte; a rebuild's swap scans the one pattern or orderlist it replaces), and
 * says whether the shape must be derived again, so the main loop sends the player a wider
 * shape in the same batch as the edit and the player never plays an edit with a shape that
 * lacks it. A base only grows: a shape narrows again only through a fresh analysis, sliced
 * in the background, and a swap. While editing, the target is what the packer would make
 * of the song as it is now; the widened shape is a superset of it until the next fresh
 * analysis ends.
 *
 * The song the analysis reads is the model with the caller's pending edits on top (the
 * view: gt2_analysis.pending), since the audio interrupt applies them later: a widening or
 * a derivation reads what the song will be once they are applied.
 *
 * Portable C11, no allocation, no libm, no calls into libc. */
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
#define GT2_TABLE_WORDS 8u /* rows 0..255, bit r */

/* What the analysis has found (OR-only: gt2_shape_widen only adds). */
typedef struct {
    uint32_t packed;                      /* bit sub: its three orderlists are not empty */
    uint32_t pattused[GT2_PATTERN_WORDS]; /* named by a packed subtune's orderlist */
    uint32_t named[GT2_PLAY_CHANNELS][GT2_PATTERN_WORDS]; /* ... by that channel's */
    uint32_t scanned[GT2_PATTERN_WORDS];  /* its rows are in what follows */
    uint32_t filled[GT2_PATTERN_WORDS];   /* a row with a note, instrument or command */
    uint32_t instrused[2];                /* bit i: instrument i */
    uint32_t tableused[GT2_TABLES][GT2_TABLE_WORDS]; /* rows reached (1..255) */
    uint32_t stbl_calc[GT2_TABLE_WORDS];  /* speedtable rows a 1XY-4XY command names */
    uint32_t seen;                        /* GT2_SEEN_* */
    uint32_t tick0;                       /* gt2_shape.tick0's bits the patterns need */
    uint8_t transdown, transup;           /* the largest transposes */
    uint8_t note_lo, note_hi;             /* the patterns' notes (0..95); lo > hi: none */
} gt2_base;

enum {
    GT2_SEEN_EFFECTS = 1u << 0, /* a pattern command */
    GT2_SEEN_FILTER = 1u << 1,  /* a filter command (A-C) */
    GT2_SEEN_TRANS = 1u << 2,   /* an orderlist transpose */
    GT2_SEEN_REPEAT = 1u << 3   /* an orderlist repeat */
};

typedef struct {
    const gt2_song* song;
    /* The edits the caller has made that the song does not hold yet, oldest first: the
     * analysis reads the song as they will leave it. NULL and 0: none (then the reads are
     * direct). */
    const gt2_edit* pending;
    uint32_t pending_count;
    /* the options */
    uint16_t player_flags, hr_adsr;
    uint8_t mult, opt_pulse, opt_realtime, subtune;
    uint32_t a4_millihz;
    gt2_base base;
    /* the slicing */
    uint8_t phase, i_sub, i_ch, i_row;
    uint16_t i_pos, i_pat;
    int16_t result;
    uint8_t grew; /* the base gained something since gt2_shape_widen began */
    /* the derivation's own (reset by each) */
    uint8_t d_phase, d_ch;
    uint16_t d_i;
    uint32_t d_seen;   /* GT2_DSEEN_* */
    uint32_t d_tick0;
    uint8_t count[3], lowest[3];
    uint8_t rel_any, rel_hi, abs_any, abs_lo, abs_hi;
    /* Wavetable rows no table holds yet that a channel plays (song_ctl_draft): xwave_n left
     * bytes, then from xwave[GT2_DRAFT_ROWS] their right bytes; each derivation reads them as it
     * reads the rows in use. NULL: none. The caller's, set after gt2_shape_begin. */
    const uint8_t* xwave;
    uint8_t xwave_n;
} gt2_analysis;

/* Refusals (gt2_shape_begin, gt2_shape_step's results). */
enum {
    GT2_PLAY_ERR_SUBTUNE = -1, /* the packed song would not hold the subtune */
    GT2_PLAY_ERR_STEREO = -2   /* a GoatTracker Stereo song: gt2reloc packs three channels */
};
#define GT2_SHAPE_MORE 0
#define GT2_SHAPE_DONE 1

/* A slice's work units: one an orderlist entry, a pattern row, a table row or step, an
 * instrument (four table walks besides), a numbering entry, a stream byte. The device's
 * budget is measured in fm1-emu against the main loop's allowance (song_ctl.h). */

/* Starts a fresh analysis of s for subtune with the options (o NULL: GoatTracker's
 * defaults); a->pending is cleared (set it after, if any). 0, or GT2_PLAY_ERR_STEREO. */
int gt2_shape_begin(gt2_analysis* a, const gt2_song* s, uint32_t subtune, const gt2_options* o);
/* One slice of at most about budget units: GT2_SHAPE_MORE, GT2_SHAPE_DONE once out holds
 * the shape, or a GT2_PLAY_ERR_*. out is written only by the slices that derive it. */
int gt2_shape_step(gt2_analysis* a, gt2_shape* out, uint32_t budget);
/* gt2_shape_begin and every step at once (the host's, and the device's tests): 0 or a
 * GT2_PLAY_ERR_*. */
int gt2_shape_analyse(gt2_analysis* a, const gt2_song* s, uint32_t subtune,
                      const gt2_options* o, gt2_shape* out);

/* What edit e (one of a->pending, or about to be) adds to a's base, for a finished
 * analysis: 1 if the shape must be derived again (gt2_shape_derive), 0 if it stays as it
 * is. Bounded: a cell is O(1) besides at most one table walk (255 rows); a pattern's swap
 * scans its rows, an orderlist's its entries and the rows of the patterns it newly names. */
int gt2_shape_widen(gt2_analysis* a, const gt2_edit* e);
/* Include live jam instruments and notes without inventing model rows. Constant work;
 * derive the shape in the usual slices after a 1. lo > hi means no jam notes. */
int gt2_shape_include_jams(gt2_analysis* a, const uint32_t instruments[2], uint32_t lo,
                          uint32_t hi);
/* The shape from a's base and the song (with the pending edits), all at once (about 70k
 * instructions on the device at the worst: more than a main-loop pass has to spare), or in
 * slices: begin, then steps of about budget units each until one returns 1. */
void gt2_shape_derive(gt2_analysis* a, gt2_shape* out);
void gt2_shape_derive_begin(gt2_analysis* a);
int gt2_shape_derive_step(gt2_analysis* a, gt2_shape* out, uint32_t budget);

/* 1 if a player following `from` may follow `to` from its next call (gt2_play_set_shape):
 * the same channels and subtune, and the same encodings of the player's state (simple
 * pulse, wave delays and commands). Otherwise the main loop hands
 * the player over to one fast-forwarded with `to` (song_ctl.h). */
int gt2_shape_compatible(const gt2_shape* from, const gt2_shape* to);

/* The shape with another channel count (its variable layout too): what the player follows
 * until a hand-over brings the channels a widening found (song_ctl.h). */
void gt2_shape_keep_channels(gt2_shape* sh, uint32_t channels);

/* The packed orderlist byte that model entry pos of channel ch (0-2) is, for starting a
 * subtune there (gt2_play_start): the model's entries before it less its R0s, at most 255.
 * Reads a's song with its pending edits. */
uint8_t gt2_shape_order_byte(const gt2_analysis* a, uint32_t ch, uint32_t pos);

/* The host's player: a fresh analysis and gt2_play_start at the subtune's start. On a
 * refusal *p is all zeros. The shape is the caller's, and must outlive the player. */
int gt2_play_init(gt2_player* p, gt2_shape* shape, const gt2_song* s, uint32_t subtune,
                  const gt2_options* o);

#ifdef __cplusplus
}
#endif

#endif
