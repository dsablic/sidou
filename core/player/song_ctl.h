// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song in the audio interrupt (M2c Task 6): the command ring from the main loop, the
 * event ring back, the frame clock, and the edits applied between frames.
 *
 * The player (gt2_play.h) runs in the audio interrupt, at sample-exact frame boundaries:
 * audio_fill renders chip A up to the sample where a call of the play routine falls
 * (song_ctl_due), makes the call (song_ctl_call), whose writes go straight into chip A
 * with sid_write, and renders the rest of the block (decision record 0005's split render).
 * The host renders a song with the same calls at the same samples, so the two are equal
 * bit for bit.
 *
 * The frame clock: a call every CIA period, gt2reloc's .sid speed (0x4cc7 / N + 1 cycles
 * at PAL's 985,248 Hz, 0x42c6 / N + 1 at NTSC's 1,022,727; 1x is the video frame, 19,656
 * and 17,095 cycles; decision record 0007, "The player's place"), at 24 MHz / 544 samples
 * a second. song_ctl keeps the next call's position in whole samples and a remainder, so
 * the calls never drift. With multispeed up to 4 a call is at least 184 samples from the
 * next, longer than a block (128): a block holds at most one.
 *
 * The command ring (main loop to interrupt, one producer, one consumer, one core): play
 * (a subtune from orderlist positions), stop (each voice's gate off, so releases ring as on
 * the C64), continue, the mute mask (a muted voice's gate goes off, then its writes are
 * dropped), a shape swap (gt2_shape.h), a hand-over of a fast-forwarded player (for a
 * shape change the player's state cannot follow, and M2d's seeks), jam notes, song edits
 * and the call period (M2e's clock following). The main loop's commands of a pass become
 * visible together (song_ctl_commit, with the register ring's: platform/fm1/audio.c), and
 * the interrupt applies them at the start of a block, after the register ring's drain, so
 * between two calls.
 *
 * Song edits in the interrupt are O(1) only (gt2_song.h): a cell write of at most 4 bytes
 * or an index swap. At most SONG_EDITS_PER_BLOCK are applied in a block; the rest wait for
 * the next. Each command carries a sequence number, and the interrupt publishes the last
 * one it applied (`applied`): that is every edit's acknowledgement, one word that cannot
 * overflow, after which the main loop releases what a swap replaced (gt2_edit_release).
 *
 * The event ring (interrupt to main loop): each call's row and note events (gt2_event) with
 * the call's number, sized for a second of them; when the main loop falls behind, the
 * oldest are dropped and counted (song_ctl_events).
 *
 * The shape (gt2_shape.h) is the main loop's: song_ctl widens it for every edit before the
 * edit goes (the player never plays an edit with a shape that lacks it), narrows it with a
 * fresh analysis in the background once edits stop, and hands the player over to a
 * fast-forwarded one when a change of shape is one the player's state cannot follow (the
 * channel count, the state's encodings). Two shape buffers: the one the player follows and
 * the one the main loop builds; a buffer is written again only once the interrupt has
 * applied the command that let it go.
 *
 * Chip A's setup is not song_ctl's: a song's CHPA chunk (M2b Task 4's chip_ctl, through
 * song_chip_hooks) is prepared before the song starts. Without hooks, PLAY sets chip A's
 * envelopes to AUTHENTIC (the gate reset off, decision record 0005) and STOP sets them back
 * to the gate reset the main loop gave (song_ctl_set_keys_gate_reset).
 *
 * Portable C11, no allocation. The interrupt's side (song_ctl_block, song_ctl_due,
 * song_ctl_call, song_ctl_block_end and what they reach) runs from SRAM on the device
 * (platform/fm1/app.ld) and calls nothing in flash. */
#ifndef SIDOU_SONG_CTL_H
#define SIDOU_SONG_CTL_H

#include <stdint.h>

#include "player/gt2_play.h"
#include "player/gt2_shape.h"
#include "sid/sid.h"
#include "song/gt2_song.h"
#include "song/song_ext.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SONG_CMD_RING 32u          /* commands, a power of two */
#define SONG_EDITS_PER_BLOCK 8u
#define SONG_EVENT_RING 256u       /* events, a power of two: a second of them, three
                                    * channels at speed 1 (150) and more */
#define SONG_INFLIGHT 12u          /* edits sent and not yet acknowledged: a block's 8 and more */
#define SONG_BLOCK 128u            /* samples a block (platform/fm1/audio.h) */
#define SONG_MULTISPEED_MAX 4u     /* the device's (gt2_caps): a block holds one call */
/* The main loop's analysis slice, in gt2_shape_step units (fm1-emu measures a slice's
 * instructions against SONG_SLICE_INSTRUCTIONS, tests/song.rs). */
#define SONG_SLICE_UNITS 300u
#define SONG_SLICE_INSTRUCTIONS 39000u
/* Calls a fast-forward runs a slice, and how far ahead of the interrupt's call it hands over
 * (the hand-over is armed for that call). */
#define SONG_FF_CALLS 6u
#define SONG_HANDOVER_LEAD 4u

/* An event out of the ring: one channel's events of one call. */
typedef struct {
    uint16_t frame;    /* the call's number since play, low 16 bits */
    uint8_t flags;     /* GT2_EV_* (bits 0-4), the channel in bits 5-6 */
    uint8_t pattern, row, order_pos; /* GT2_EV_ROW */
    uint8_t note, instr;             /* GT2_EV_NOTE, GT2_EV_SLIDE */
} song_event;
#define SONG_EV_CHANNEL(e) (((e)->flags >> 5) & 3u)
#define SONG_EV_FLAGS(e) ((e)->flags & 0x1fu)

enum {
    SONG_CMD_PLAY = 1, SONG_CMD_STOP, SONG_CMD_CONTINUE, SONG_CMD_MUTE, SONG_CMD_SHAPE,
    SONG_CMD_HANDOVER, SONG_CMD_JAM, SONG_CMD_EDIT, SONG_CMD_PERIOD, SONG_CMD_TAKE
};

typedef struct {
    uint8_t kind;   /* SONG_CMD_* */
    uint8_t a, b, c;
    uint32_t seq;
    union {
        gt2_edit edit;
        struct {
            const void* ptr, *ptr2;
            uint32_t x, y, z;
        } p;
    } u;
} song_cmd;

/* The frame clock: q whole samples a call and r / den more. */
typedef struct {
    uint32_t q, r, den;
} song_period;

/* What PLAY needs from chip A (M2b Task 4's chip_ctl and keys_route); every member may be
 * NULL. */
typedef struct {
    /* Called once a poll from PLAY on until it returns 1: chip A ready for the song (with
     * M2b: keys_route_ready_for_song, the keys off chip A or KEYS A set) and in the song's
     * setup (setup NULL: the song has no CHPA, the keys' setup with AUTHENTIC envelopes;
     * the hook reports a setup it applies, keys_route_song_setup, so the keys come back to
     * theirs at the stop). The song starts at the block after the call that returns 1. */
    int (*chip_a_song)(void* ctx, const song_chip_setup* setup);
    /* Once a stop has reached the interrupt: chip A back to the keys' setup. */
    void (*chip_a_keys)(void* ctx);
    void* ctx;
} song_chip_hooks;

/* The main loop's work space: big, so the caller places it (POOL on the device). */
typedef struct {
    gt2_analysis cur;  /* the shape the player follows, and what widening adds to it */
    gt2_analysis bg;   /* a fresh analysis: for play, or narrowing in the background */
    gt2_player ff;     /* the fast-forwarded player of a hand-over */
    gt2_edit inflight[SONG_INFLIGHT];
    uint32_t inflight_seq[SONG_INFLIGHT];
} song_work;

enum {
    SONG_IDLE,      /* nothing started */
    SONG_LOADING,   /* PLAY asked: the analysis or chip A's setup runs */
    SONG_PLAYING,
    SONG_STOPPED    /* stopped, continue resumes */
};

/* song_ctl_play's refusal besides gt2_shape's (song_ctl.error). */
#define SONG_ERR_MULTISPEED (-10) /* above SONG_MULTISPEED_MAX: two calls in a block */
/* What a derivation in slices is for (song_ctl.deriving). */
enum { SONG_DERIVE_EDIT = 1, SONG_DERIVE_HANDOVER, SONG_DERIVE_DONE };
/* Polls with no edit before a widened shape is narrowed again. */
#define SONG_QUIET_POLLS 50u

typedef struct {
    /* --- written by the main loop, read by the interrupt --- */
    song_cmd cmd[SONG_CMD_RING];
    volatile uint32_t cmd_head; /* commands before it are visible */
    gt2_song* song;
    /* --- written by the interrupt, read by the main loop --- */
    volatile uint32_t cmd_tail;     /* commands before it are applied */
    volatile uint32_t applied;      /* the last applied command's seq */
    volatile uint32_t calls;        /* play routine calls since play */
    volatile uint32_t ev_head;
    volatile uint32_t handover_seq; /* the last hand-over the interrupt took or refused */
    volatile uint8_t handover_ok;
    volatile uint8_t isr_playing;
    volatile uint8_t past_b7;       /* channels that played a note past the table */
    uint8_t song_gate;              /* the gate reset PLAY gave chip A, or $FF */
    song_event ev[SONG_EVENT_RING];
    /* --- the interrupt's --- */
    gt2_player player;
    uint8_t shadow[GT2_SID_REGS]; /* the player's writes to chip A */
    uint8_t mute;                 /* bit v: voice v muted */
    uint8_t taken;                /* bit v: voice v the keys' (KEYS A, M2b's keys_route) */
    uint8_t armed, armed_now;     /* a hand-over waiting for its call */
    uint8_t jam[GT2_PLAY_CHANNELS][3]; /* a jam waiting for the next call: on, note, instr */
    uint32_t next;                /* samples from this block's start to the next call */
    uint32_t rem;
    song_period period;
    const gt2_player* armed_state;
    const gt2_shape* armed_shape;
    uint32_t armed_call, armed_seq;
    sid* chip;                    /* chip A this block */
    uint32_t blocks;              /* blocks since init (tests read it) */
    uint32_t play_block;          /* the block the last play started in (tests read it) */
    uint32_t edits_block_max;     /* the most edits one block applied */
    /* --- the main loop's --- */
    gt2_shape* shapes[2];
    song_work* w;
    song_chip_hooks hooks;
    gt2_options opts;
    uint32_t stage;         /* commands before it are pushed (committed or not) */
    uint32_t seq;           /* the last command's */
    uint32_t ev_tail;
    uint32_t ev_lost;       /* events dropped: the main loop fell a ring behind */
    uint32_t inflight_n;    /* edits in w->inflight, oldest first */
    uint32_t busy;          /* edits refused for want of room (song_ctl_edit's 0) */
    uint32_t spare_free_at; /* the seq after which the spare shape buffer is free */
    uint32_t stop_seq;
    uint32_t ff_seq, ff_calls, ff_target;
    uint32_t quiet;         /* polls since the last edit */
    uint32_t slices;        /* analysis and fast-forward slices run */
    uint32_t handovers, narrowed, handed_back; /* counts, for the tests */
    int16_t error;          /* the last refusal (GT2_PLAY_ERR_*, SONG_ERR_*), 0 none */
    uint8_t state;          /* SONG_* */
    uint8_t active;         /* the shape the player follows (or will): shapes[active] */
    uint8_t spare_wait;     /* the other buffer is the interrupt's until spare_free_at */
    uint8_t widened;        /* the player's shape is wider than a fresh analysis's */
    uint8_t bg_on;          /* the background analysis runs (into the spare) */
    uint8_t bg_dirty;       /* an edit went while it ran */
    uint8_t ff_on;          /* a fast-forward runs (or waits for its hand-over) */
    uint8_t ff_sent, ff_now;
    uint8_t want_handover;  /* the shape the player follows keeps fewer channels: hand over */
    uint8_t keys_gate_reset;
    uint8_t subtune, start_pos;
    uint8_t start[GT2_PLAY_CHANNELS];
    uint8_t chip_ready, loading_done;
    uint8_t handback;       /* a stop went: hand chip A back once it is applied */
    uint8_t shapes_used;    /* a play has gone: the interrupt's player holds a shape */
    uint8_t deriving;       /* SONG_DERIVE_*: a derivation into the spare runs a slice a poll */
    uint8_t deriving_for;
} song_ctl;

/* The song in its region, its two shape buffers and its work space, all the caller's;
 * nothing plays. */
void song_ctl_init(song_ctl* sc, gt2_song* song, gt2_shape* shape0, gt2_shape* shape1,
                   song_work* w);
void song_ctl_set_hooks(song_ctl* sc, const song_chip_hooks* hooks);
/* The gate reset STOP gives chip A back without hooks (the keys' envelope mode). */
void song_ctl_set_keys_gate_reset(song_ctl* sc, uint8_t on);

/* The call period for multispeed N (0: 25 Hz) on PAL or NTSC. */
song_period song_period_of(uint32_t multispeed, uint32_t ntsc);

/* --- The main loop ------------------------------------------------------------------------
 * Every command returns 1, or 0 if it cannot go now (the ring or the edits in flight are
 * full, a shape change is under way): nothing changed then, and the caller asks again at
 * the next pass. */

/* Plays subtune from orderlist entry pos of each channel (0: the start). The analysis runs
 * a slice a poll (SONG_LOADING), then chip A's setup; the song starts at the block after. */
int song_ctl_play(song_ctl* sc, uint32_t subtune, uint32_t pos);
int song_ctl_stop(song_ctl* sc);
int song_ctl_continue(song_ctl* sc);
int song_ctl_mute(song_ctl* sc, uint32_t mask);
/* The voices the keys hold on chip A (KEYS A: M2b's keys_route_voices_taken): the song's
 * writes to them are dropped and the keys' stand (no gate-off, unlike a mute); a voice given
 * back gets the song's registers again at the next block. */
int song_ctl_take(song_ctl* sc, uint32_t mask);
/* Voice v takes note (0 C-0 ... 95) and instrument at its next tick 0, as if its next row
 * held them (no command): M2d's audition, M2e's multitimbral mode. */
int song_ctl_jam(song_ctl* sc, uint32_t voice, uint32_t note, uint32_t instr);
/* An edit a gt2_song primitive returned: song_ctl owns it from a 1 on (it releases it once
 * applied); with 0 it is still the caller's (pending: send it again, or gt2_edit_cancel). An
 * edit that widens the shape is held while the shape is derived again, a slice a poll, and
 * goes with it; the next edit waits (0) until then. */
int song_ctl_edit(song_ctl* sc, const gt2_edit* e);
int song_ctl_set_period(song_ctl* sc, song_period p);
/* Fast-forwards a player from the play's start to call `call` and hands it over at the
 * interrupt's next call (M2d's seeks). */
int song_ctl_seek(song_ctl* sc, uint32_t call);

/* Once a pass: what the interrupt has applied (releases, hand-overs), with SONG_POLL_RING
 * chip A's hand-back after a stop (song_chip_hooks.chip_a_keys: the pass's register-ring
 * writes for song control, at most REG_RING_SONG_PASS; the caller leaves the flag out of a
 * pass whose ring budget other paths took, M2b's keys_route), then with SONG_POLL_SLICE one
 * slice of the analysis, a derivation or the fast-forward (SONG_SLICE_UNITS; the caller
 * leaves it out of a pass that already ran a chip preparation's slice). */
#define SONG_POLL_SLICE 1u
#define SONG_POLL_RING 2u
void song_ctl_poll(song_ctl* sc, uint32_t flags);
/* Makes the pass's commands visible to the interrupt (with reg_ring_commit). */
void song_ctl_commit(song_ctl* sc);
/* Up to n events, oldest first; the count. Dropped ones are counted in ev_lost. */
uint32_t song_ctl_events(song_ctl* sc, song_event* out, uint32_t n);
/* SONG_* */
uint32_t song_ctl_state(const song_ctl* sc);
/* GT2_PLAY_WARN_* of the shape the player follows, and GT2_PLAY_WARN_PAST_B7 once a note
 * past the table has played (gt2_player.freq_unknown): the SONG view's hint. */
#define GT2_PLAY_WARN_PAST_B7 (1u << 7)
uint32_t song_ctl_warnings(const song_ctl* sc);

/* --- The audio interrupt --------------------------------------------------------------- */

/* At a block's start, after the register ring's drain: the commands (chip: chip A now). */
void song_ctl_block(song_ctl* sc, sid* chip);
/* The sample of this block (0..n-1) where the next call falls, or n if none does. */
uint32_t song_ctl_due(const song_ctl* sc, uint32_t n);
/* The call due at song_ctl_due's sample: the play routine, writing into chip A. */
void song_ctl_call(song_ctl* sc);
/* At the block's end (n samples). */
void song_ctl_block_end(song_ctl* sc, uint32_t n);
/* Chip A was swapped for a prepared one (reg_ring's swap hook): the player's registers go
 * into it again before the block renders, so a song plays on through a profile change. */
void song_ctl_chip_swapped(void* ctx, uint32_t chip, sid* s);
/* Chip A's block, split at the call: sid_render, the call, sid_render (the host's render;
 * the device's audio_fill does the same with the LIVE view's tap). */
void song_ctl_render(song_ctl* sc, sid* chip, int16_t* out, uint32_t n);

#ifdef __cplusplus
}
#endif

#endif
