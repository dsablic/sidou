// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
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

#define SONG_CMD_RING 32u
#define SONG_EDITS_PER_BLOCK 8u
#define SONG_EVENT_RING 256u
#define SONG_INFLIGHT 12u
#define SONG_BLOCK 128u
#define SONG_MULTISPEED_MAX 4u
#define SONG_SLICE_UNITS 200u
#define SONG_SLICE_INSTRUCTIONS 39000u
#define SONG_FF_CALLS 6u
#define SONG_HANDOVER_LEAD 4u

#define SONG_NEXT_HELD 0x40000000u

typedef struct {
    uint16_t frame;
    uint8_t flags;
    uint8_t pattern, row, order_pos;
    uint8_t note, instr;
} song_event;
#define SONG_EV_CHANNEL(e) (((e)->flags >> 5) & 3u)
#define SONG_EV_FLAGS(e) ((e)->flags & 0x1fu)

enum {
    SONG_CMD_PLAY = 1, SONG_CMD_STOP, SONG_CMD_CONTINUE, SONG_CMD_MUTE, SONG_CMD_SHAPE,
    SONG_CMD_HANDOVER, SONG_CMD_JAM, SONG_CMD_EDIT, SONG_CMD_PERIOD, SONG_CMD_ALIGN,
    SONG_CMD_DRAFT, SONG_CMD_HELD
};

#define SONG_HELD_LOCKS (SONG_LOCK_PULSE + 1u)

typedef struct {
    uint8_t kind;
    uint8_t a, b, c;
    uint32_t seq;
    union {
        gt2_edit edit;
        gt2_draft draft;
        uint8_t held[3u * SONG_HELD_LOCKS];
        struct {
            const void* ptr, *ptr2;
            uint32_t x, y, z;
        } p;
    } u;
} song_cmd;

typedef struct {
    uint32_t q, r, den;
} song_period;

typedef struct {
    int (*chip_a_song)(void* ctx, const song_chip_setup* setup);
    void (*chip_a_keys)(void* ctx);
    void (*chip_a_character)(void* ctx, uint32_t field, int32_t value);
    void* ctx;
} song_chip_hooks;

typedef struct {
    gt2_analysis cur;
    gt2_analysis bg;
    gt2_player ff;
    gt2_edit inflight[SONG_INFLIGHT];
    uint32_t inflight_seq[SONG_INFLIGHT];
} song_work;

enum {
    SONG_IDLE,
    SONG_LOADING,
    SONG_PLAYING,
    SONG_STOPPED
};

#define SONG_ROW_LOCKS (2u * (SONG_LOCK_PULSE + 2u))
typedef struct {
    uint8_t pattern, row, param, lo, hi;
} song_row_lock;
typedef struct {
    uint8_t n;
    song_row_lock e[SONG_ROW_LOCKS];
} song_lock_set;

#define SONG_ERR_MULTISPEED (-10)
enum { SONG_DERIVE_EDIT = 1, SONG_DERIVE_HANDOVER, SONG_DERIVE_DONE, SONG_DERIVE_JAM };
#define SONG_QUIET_POLLS 50u

typedef struct song_ctl {
    song_cmd cmd[SONG_CMD_RING];
    volatile uint32_t cmd_head;
    gt2_song* song;
    volatile uint8_t take;
    volatile uint32_t cmd_tail;
    volatile uint32_t applied;
    volatile uint32_t calls;
    volatile uint32_t samples;
    volatile uint32_t call_sample;
    volatile uint32_t ev_head;
    volatile uint32_t handover_seq;
    volatile uint8_t handover_ok;
    volatile uint8_t isr_playing;
    volatile uint8_t audition;
    volatile uint8_t past_b7;
    uint8_t song_gate;
    song_event ev[SONG_EVENT_RING];
    gt2_player player;
    uint8_t shadow[GT2_SID_REGS];
    uint8_t mute;
    uint8_t taken;
    uint8_t armed, armed_now;
    uint8_t jam[GT2_PLAY_CHANNELS][3];
    song_lock_set lock_next[GT2_PLAY_CHANNELS][2];
    volatile uint8_t lock_pub[GT2_PLAY_CHANNELS];
    volatile uint8_t row_seq[GT2_PLAY_CHANNELS];
    uint8_t row_pat[GT2_PLAY_CHANNELS], row_row[GT2_PLAY_CHANNELS];
    uint8_t row_ord[GT2_PLAY_CHANNELS];
    uint8_t lock_n[GT2_PLAY_CHANNELS];
    uint8_t lock_pin;
    song_row_lock lock_cur[GT2_PLAY_CHANNELS][SONG_ROW_LOCKS];
    uint32_t lock_regs;
    uint32_t chance_seed, chance_n;
    uint32_t next;
    uint32_t rem;
    song_period period;
    const gt2_player* armed_state;
    const gt2_shape* armed_shape;
    uint32_t armed_call, armed_seq;
    sid* chip;
    uint32_t blocks;
    uint32_t play_block;
    uint32_t edits_block_max;
    uint32_t edit_cells, edit_swaps;
    gt2_shape* shapes[2];
    song_work* w;
    song_chip_hooks hooks;
    gt2_options opts;
    uint32_t stage;
    uint32_t seq;
    uint32_t ev_tail;
    uint32_t ev_lost;
    uint32_t inflight_n;
    uint32_t busy;
    uint32_t spare_free_at;
    uint32_t stop_seq;
    uint32_t play_seq;
    uint32_t ff_seq, ff_calls, ff_target;
    uint32_t quiet;
    uint32_t slices;
    uint32_t handovers, narrowed, handed_back;
    int16_t error;
    uint8_t state;
    uint8_t active;
    uint8_t spare_wait;
    uint8_t widened;
    uint8_t bg_on;
    uint8_t bg_dirty;
    uint8_t ff_on;
    uint8_t ff_sent, ff_now;
    uint8_t want_handover;
    uint8_t keys_gate_reset;
    uint8_t subtune, start_pos;
    uint8_t start[GT2_PLAY_CHANNELS];
    uint8_t chip_ready, loading_done;
    uint8_t handback;
    uint8_t shapes_used;
    uint8_t row_seen[GT2_PLAY_CHANNELS];
    uint8_t play_sub;
    int16_t char_lock[GT2_PLAY_CHANNELS][4];
    int16_t char_told[4];
    uint8_t deriving;
    uint8_t deriving_for;
    uint8_t auditioning;
    uint8_t chip_held;
    uint8_t load_for;
    uint32_t idle_edits;
    uint32_t edits_submitted, edits_completed;
    uint32_t lock_edit_seq;
    volatile uint32_t lock_edit_rows;
    volatile uint8_t lock_edit_refresh;
    const uint8_t* edit_restore;
    uint8_t edit_restore_n;
    uint8_t hold_next;
    uint8_t ff_stop;
    uint8_t from_top;
    uint32_t jam_instruments[2];
    uint32_t jam_seq[GT2_PLAY_CHANNELS];
    uint8_t jam_wait[GT2_PLAY_CHANNELS][3];
    uint8_t jam_lo, jam_hi, jam_live, jam_release;
    uint8_t mute_user, mute_sent, digi_hold;
    uint8_t digi_any;
    uint32_t digi_ev;
    uint32_t digi_ext;
    uint16_t digi_at;
    uint16_t digi_end;
    int16_t digi_level;
    uint8_t digi_left;
    uint8_t digi_ok;
    uint8_t digi_of[256];
    uint16_t digi_from[SONG_DIGI_KINDS];
    uint32_t jam_at[GT2_PLAY_CHANNELS];
    uint8_t jam_timed;
    gt2_draft draft;
    uint8_t draft_rows[2u * GT2_DRAFT_ROWS];
    uint8_t draft_n, draft_grow;
    uint32_t draft_cov;
} song_ctl;

void song_ctl_init(song_ctl* sc, gt2_song* song, gt2_shape* shape0, gt2_shape* shape1,
                   song_work* w);
void song_ctl_set_hooks(song_ctl* sc, const song_chip_hooks* hooks);
void song_ctl_set_keys_gate_reset(song_ctl* sc, uint8_t on);

song_period song_period_of(uint32_t multispeed, uint32_t ntsc);

int song_ctl_play(song_ctl* sc, uint32_t subtune, uint32_t pos);
int song_ctl_stop(song_ctl* sc);
int song_ctl_continue(song_ctl* sc);
int song_ctl_mute(song_ctl* sc, uint32_t mask);
int song_ctl_take(song_ctl* sc, uint32_t mask);
int song_ctl_jam(song_ctl* sc, uint32_t voice, uint32_t note, uint32_t instr);
#define SONG_JAM_RELEASE 0xffu
int song_ctl_audition(song_ctl* sc);
int song_ctl_audition_end(song_ctl* sc);
uint32_t song_ctl_shown_state(const song_ctl* sc);
int song_ctl_jam_ready(const song_ctl* sc);
uint32_t song_ctl_again(const song_ctl* sc);
int song_ctl_edit(song_ctl* sc, const gt2_edit* e);
int song_ctl_edit_with_locks(song_ctl* sc, const gt2_edit* e, const uint8_t* locks, uint32_t n);
int song_ctl_set_period(song_ctl* sc, song_period p);
int song_ctl_seek(song_ctl* sc, uint32_t call);
int song_ctl_seek_further(song_ctl* sc, uint32_t call);

void song_ctl_hold(song_ctl* sc, int on);
int song_ctl_align(song_ctl* sc, uint32_t sample);

#define SONG_POLL_SLICE 1u
#define SONG_POLL_RING 2u
void song_ctl_poll(song_ctl* sc, uint32_t flags);
void song_ctl_commit(song_ctl* sc);
uint32_t song_ctl_events(song_ctl* sc, song_event* out, uint32_t n);
int song_ctl_draft(song_ctl* sc, uint32_t ch, uint32_t first, uint32_t n, const uint8_t* left,
                   const uint8_t* right);
int song_ctl_held(song_ctl* sc, uint32_t ch, const uint8_t* locks, uint32_t n);

uint32_t song_ctl_state(const song_ctl* sc);
uint32_t song_ctl_clock_calls(const song_ctl* sc);
uint32_t song_ctl_from_top(const song_ctl* sc);
#define GT2_PLAY_WARN_PAST_B7 (1u << 7)
uint32_t song_ctl_warnings(const song_ctl* sc);

void song_ctl_block(song_ctl* sc, sid* chip);
uint32_t song_ctl_due(const song_ctl* sc, uint32_t n);
void song_ctl_call(song_ctl* sc);
#define SONG_DIGI_SPAN 5u
#define SONG_DIGI_LEVEL 875
void song_ctl_block_end(song_ctl* sc, int16_t* out, uint32_t n);
void song_ctl_chip_swapped(void* ctx, uint32_t chip, sid* s);
void song_ctl_render(song_ctl* sc, sid* chip, int16_t* out, uint32_t n);
uint32_t song_digi_sample(uint32_t kind, uint8_t* codes);

#ifdef __cplusplus
}
#endif

#endif
