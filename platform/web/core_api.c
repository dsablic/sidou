// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The site's wasm core (tools/wasm-core builds it; web/core.js is its JavaScript side): the
 * device's own .sng reader and writer, GTI5 export and song player, behind a few calls that
 * pass bytes through two buffers in the module's memory. One song at a time, in a region the
 * size of the device's (SONG_REGION_BYTES), read with the device's multispeed cap, so a file
 * is refused here as the FM-1 refuses it, with the same message.
 *
 * Every call is synchronous and allocation-free. The JavaScript side writes a file into
 * sc_in() and reads results from sc_out(), sc_info() and sc_pcm(). */
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "fx/fx_slots.h"
#include "player/gt2_shape.h"
#include "player/song_ctl.h"
#include "sid/sid.h"
#include "sid/sid_character.h"
#include "song/gt2_ins.h"
#include "song/gt2_sng.h"
#include "song/gt2_song.h"
#include "song/song_ext.h"

#ifdef __wasm__
#define EXPORT(name) __attribute__((export_name(#name)))
#else
#define EXPORT(name)
#endif

/* A file in: the device's store holds at most 64 KiB, but a picked GoatTracker file may be
 * larger and is then refused by the reader with the device's message. */
#define IN_MAX (4u * SONG_REGION_BYTES)
/* A file out: the trailer-inclusive save cap, and room for the plain part. */
#define OUT_MAX (GT2_SAVED_MAX + 1024u)
/* The device's sample rate (24 MHz / 544) and the PCM buffer: a second of blocks. */
#define SC_RATE 44118u
#define PCM_BLOCKS 345u
#define PCM_MAX (PCM_BLOCKS * SONG_BLOCK)

static uint32_t region[SONG_REGION_BYTES / 4u];
static gt2_song* song;
static uint8_t in_buf[IN_MAX];
static uint32_t in_len;
static uint8_t out_buf[OUT_MAX];
static uint32_t out_len;
static int16_t pcm[PCM_MAX];

/* The last read's result, as web/core.js reads it: message, then the numbers. */
typedef struct {
    char msg[32];
    int32_t result;  /* gt2_sng_read's, or gt2_sng_check_plain's, or a gt2_err */
    uint32_t version, channels, trailer, gt2_bytes, file_bytes, need_bytes, saved_bytes;
} sc_info_t;
static sc_info_t info;

static song_ctl sc;
static gt2_song* playing;
static gt2_shape shapes[2];
static song_work work;
static sid chip;

EXPORT(sc_in) uint8_t* sc_in(void) {
    return in_buf;
}
EXPORT(sc_in_max) uint32_t sc_in_max(void) {
    return IN_MAX;
}
EXPORT(sc_out) uint8_t* sc_out(void) {
    return out_buf;
}
EXPORT(sc_info) sc_info_t* sc_info(void) {
    return &info;
}
EXPORT(sc_pcm) int16_t* sc_pcm(void) {
    return pcm;
}
EXPORT(sc_rate) uint32_t sc_rate(void) {
    return SC_RATE;
}
EXPORT(sc_pcm_max) uint32_t sc_pcm_max(void) {
    return PCM_MAX;
}

static uint32_t read_mem(void* ctx, uint32_t offset, uint8_t* buf, uint32_t len) {
    const uint8_t* src = ctx;
    uint32_t n = ctx == in_buf ? in_len : out_len;
    if (offset >= n) {
        return 0;
    }
    if (len > n - offset) {
        len = n - offset;
    }
    memcpy(buf, src + offset, len);
    return len;
}

static int write_out(void* ctx, const uint8_t* buf, uint32_t len) {
    (void)ctx;
    if (len > OUT_MAX - out_len) {
        return -1;
    }
    memcpy(out_buf + out_len, buf, len);
    out_len += len;
    return 0;
}

/* Reads the len bytes at sc_in() into a fresh song: gt2_sng_read's result (0 loaded, -1 and
 * the device's message, GT2_READ_SONG_FULL), and sc_info(). A refused file leaves no song. */
EXPORT(sc_read) int32_t sc_read(uint32_t len) {
    memset(&info, 0, sizeof info);
    song = gt2_song_init(region, sizeof region);
    in_len = len > IN_MAX ? IN_MAX : len;
    gt2_caps caps = {SONG_MULTISPEED_MAX};
    gt2_read_info ri;
    memset(&ri, 0, sizeof ri);
    int r = gt2_sng_read(song, read_mem, in_buf, &caps, &ri);
    memcpy(info.msg, ri.msg, sizeof info.msg);
    info.result = r;
    info.version = ri.version;
    info.channels = ri.channels;
    info.trailer = ri.trailer;
    info.gt2_bytes = ri.gt2_bytes;
    info.file_bytes = ri.file_bytes;
    info.need_bytes = ri.need_bytes;
    info.saved_bytes = ri.saved_bytes;
    if (r != 0) {
        song = NULL;
    }
    playing = NULL; /* the transport's song: the song, or sc_play_pattern's copy */
    return r;
}

/* The song as a .sng at sc_out(): with SIDOU's trailer (1) or plain GoatTracker (0). The
 * length, or -1 (no song, or the writer failed). */
EXPORT(sc_write) int32_t sc_write(int32_t with_trailer) {
    out_len = 0;
    if (!song || gt2_sng_write(song, with_trailer, write_out, NULL) != 0) {
        return -1;
    }
    return (int32_t)out_len;
}

/* After sc_write(0): gt2_sng_check_plain over sc_out() (0 fine, 1 and a warning such as
 * READS BACK AS STEREO in sc_info's msg, -1 unreadable). */
EXPORT(sc_check_plain) int32_t sc_check_plain(void) {
    memset(info.msg, 0, sizeof info.msg);
    if (!song) {
        return -1;
    }
    return gt2_sng_check_plain(song, read_mem, out_buf, info.msg);
}

/* Instrument (1-63) as a GTI5 file at sc_out(): the length, or -gt2_err. */
EXPORT(sc_export_instrument) int32_t sc_export_instrument(uint32_t instrument) {
    out_len = 0;
    if (!song) {
        return -(int32_t)GT2_ERR_RANGE;
    }
    gt2_ins_export ex;
    gt2_err e = gt2_ins_export_begin(&ex, song, instrument);
    if (e != GT2_OK) {
        return -(int32_t)e;
    }
    if (ex.length > OUT_MAX) {
        return -(int32_t)GT2_ERR_SONG_FULL;
    }
    out_len = gt2_ins_export_read(&ex, 0, out_buf, ex.length);
    return (int32_t)out_len;
}

/* The song's model for display: counts, strings, instruments, tables, orderlists, patterns. */
EXPORT(sc_song_word) uint32_t sc_song_word(uint32_t which) {
    if (!song) {
        return 0;
    }
    switch (which) {
    case 0: return song->subtunes;
    case 1: return song->channels;
    case 2: return gt2_saved_instruments(song);
    case 3: return gt2_saved_patterns(song);
    case 4: return gt2_saved_size(song);
    case 5: return song->version;
    default: return 0;
    }
}
EXPORT(sc_song_string) const char* sc_song_string(uint32_t which) {
    if (!song) {
        return NULL;
    }
    return which == 0 ? song->name : which == 1 ? song->author : song->copyright;
}
/* Instrument i (1-63): its 25 bytes in the file's order (the name last, 16 bytes). */
EXPORT(sc_instrument) const uint8_t* sc_instrument(uint32_t i) {
    return song && i >= 1u && i <= GT2_MAX_INSTR ? song->instr[i - 1u] : NULL;
}
EXPORT(sc_table_rows) uint32_t sc_table_rows(uint32_t t) {
    return song && t < GT2_TABLES ? gt2_saved_table_rows(song, t) : 0;
}
/* Table t's left (side 0) or right (side 1) column, 255 bytes. */
EXPORT(sc_table) const uint8_t* sc_table(uint32_t t, uint32_t side) {
    if (!song || t >= GT2_TABLES) {
        return NULL;
    }
    return side ? song->rtable[t] : song->ltable[t];
}
/* An orderlist: its entries, then $FF and the restart position (sc_order_entries + 2 bytes). */
EXPORT(sc_order) const uint8_t* sc_order(uint32_t subtune, uint32_t channel) {
    return song ? gt2_order_data(song, subtune, channel) : NULL;
}
EXPORT(sc_order_entries) uint32_t sc_order_entries(uint32_t subtune, uint32_t channel) {
    return song ? gt2_order_entries(song, subtune, channel) : 0;
}
/* A held pattern's rows, 4 bytes each, the end row included; NULL if not held. */
EXPORT(sc_pattern) const uint8_t* sc_pattern(uint32_t p) {
    return song ? gt2_pattern_data(song, p) : NULL;
}
EXPORT(sc_pattern_rows) uint32_t sc_pattern_rows(uint32_t p) {
    return song ? gt2_pattern_rows(song, p) : 0;
}

/* --- Edits: the device's own primitives (gt2_song.h), each made and committed at once (the
 * host's way: gt2_edit_commit), so the model refuses here what it refuses on the FM-1 (LOCK
 * follows a pattern's rows as there, and the save-size cap is the same). A rebuild that finds
 * no hole large enough tidies the arena as the device's editor does (gt2_compact_step, one move
 * at a time) and tries again. Every call returns 0 or a gt2_err (GT2_ERR_SONG_FULL: the song as
 * the FM-1 saves it would pass 64 KiB). The page keeps its own undo: sc_region()'s bytes (the
 * region holds the whole song, offsets only, nothing outside it). */

enum {
    SC_EDIT_INSTR, SC_EDIT_TABLE, SC_EDIT_ROW, SC_EDIT_ORDER, SC_EDIT_ORDER_INSERT,
    SC_EDIT_ORDER_DELETE, SC_EDIT_PUSH_ROW, SC_EDIT_PULL_ROW, SC_EDIT_INSERT_ROW,
    SC_EDIT_DELETE_ROW, SC_EDIT_RESIZE, SC_EDIT_PATTERN_ADD, SC_EDIT_PATTERN_REMOVE
};

/* The pattern the last SC_EDIT_PATTERN_ADD made. */
static uint32_t added;

static gt2_err primitive(uint32_t op, uint32_t a, uint32_t b, uint32_t c, uint32_t d, gt2_edit* e) {
    uint8_t cell[4] = {(uint8_t)d, (uint8_t)(d >> 8), (uint8_t)(d >> 16), (uint8_t)(d >> 24)};
    switch (op) {
    case SC_EDIT_INSTR: return gt2_set_instr(song, a, b, (uint8_t)c, e);
    case SC_EDIT_TABLE: return gt2_set_table(song, a, b, c, (uint8_t)d, e);
    case SC_EDIT_ROW: return gt2_set_row(song, a, b, cell, e);
    case SC_EDIT_ORDER: return gt2_set_order(song, a, b, c, (uint8_t)d, e);
    case SC_EDIT_ORDER_INSERT: return gt2_order_insert(song, a, b, c, (uint8_t)d, e);
    case SC_EDIT_ORDER_DELETE: return gt2_order_delete(song, a, b, c, e);
    case SC_EDIT_PUSH_ROW: return gt2_pattern_push_row(song, a, b, e);
    case SC_EDIT_PULL_ROW: return gt2_pattern_pull_row(song, a, b, e);
    case SC_EDIT_INSERT_ROW: return gt2_pattern_insert_row(song, a, b, e);
    case SC_EDIT_DELETE_ROW: return gt2_pattern_delete_row(song, a, b, e);
    case SC_EDIT_RESIZE: return gt2_pattern_resize(song, a, b, e);
    case SC_EDIT_PATTERN_ADD: return gt2_pattern_add(song, a, &added, e);
    case SC_EDIT_PATTERN_REMOVE: return gt2_pattern_remove(song, a, e);
    default: return GT2_ERR_RANGE;
    }
}

/* One edit (op and its arguments: SC_EDIT_ROW's d is the cell, its first byte lowest). */
EXPORT(sc_edit) int32_t sc_edit(uint32_t op, uint32_t a, uint32_t b, uint32_t c, uint32_t d) {
    if (!song) {
        return GT2_ERR_RANGE;
    }
    gt2_edit e;
    /* Each compaction move frees no space but joins holes; it ends within two moves a block
     * (gt2_song.h), so a bound of the arena's units is generous. */
    for (uint32_t tries = 0; tries < 2u * GT2_REGION_MAX / GT2_UNIT; tries++) {
        memset(&e, 0, sizeof e);
        gt2_err r = primitive(op, a, b, c, d, &e);
        if (r == GT2_ERR_FRAGMENTED) {
            gt2_edit m;
            memset(&m, 0, sizeof m);
            if (gt2_compact_step(song, &m) <= 0) {
                return r;
            }
            gt2_edit_commit(song, &m);
            continue;
        }
        if (r == GT2_OK) {
            gt2_edit_commit(song, &e);
        }
        return r;
    }
    return GT2_ERR_FRAGMENTED;
}

EXPORT(sc_added) uint32_t sc_added(void) {
    return added;
}

/* The song's whole state, for the page's undo: copy it out before an edit, back to undo. */
EXPORT(sc_region) uint8_t* sc_region(void) {
    return (uint8_t*)region;
}
EXPORT(sc_region_bytes) uint32_t sc_region_bytes(void) {
    return sizeof region;
}

/* A pattern row's parameter locks (song_ext.h's LOCK): their count; sc_out() holds them,
 * SONG_LOCK_BYTES each (pattern, row, parameter, value u16). */
EXPORT(sc_locks_row) uint32_t sc_locks_row(uint32_t pattern, uint32_t row) {
    uint32_t n = 0;
    const uint8_t* r = song ? song_locks_row(song, pattern, row, &n) : NULL;
    if (!r) {
        return 0;
    }
    memcpy(out_buf, r, n * SONG_LOCK_BYTES);
    return n;
}

/* --- Preview: the device's song transport (song_ctl) into chip A, as the audio interrupt
 * runs it, a block of SONG_BLOCK samples at a time with a main-loop pass between. Chip A has
 * the song's setup (CHPA: the profile, the character's four controls, the experts and the
 * envelope mode; a character lock moves them at once, where the FM-1 glides), and
 * sc_render_out puts it through the effect slots (SLOT) to stereo as the FM-1's output does.
 * Not here: the thump's output stage, chip B, CHIP SPREAD, MASTER and the tail gate. */

static uint32_t scratch[SONG_REGION_BYTES / 4u];
static sid_character character, base_character;
static sid_dac dac;
static uint8_t envelope; /* CHPA's: 1 AUTHENTIC */
static sid_slots slots;
static sid_slot_pool slot_pool;
static int16_t pcm_out[2u * PCM_MAX];
#define SC_SLOTS_SEED 0x51D0u /* platform/fm1/audio.c's */

static void pass(void) {
    song_ctl_poll(&sc, SONG_POLL_SLICE | SONG_POLL_RING);
    song_ctl_commit(&sc);
}

static void block(int16_t* out) {
    song_ctl_block(&sc, &chip);
    song_ctl_render(&sc, &chip, out, SONG_BLOCK);
    song_ctl_block_end(&sc, out, SONG_BLOCK);
}

/* CHPA as the FM-1 accepts it (platform/fm1/main.c's setup_valid): 1 and the chunk. */
static int song_setup(const gt2_song* s, song_chip_setup* c) {
    if (!song_ext_get_chip(s, SONG_TAG_CHPA, c)) {
        return 0;
    }
    const sid_chip_profile* p = sid_chip_profile_get(c->profile);
    if (!p || p->model != c->model || ((c->age | c->dac | c->thump | c->bleed) & 0x80u) ||
        c->envelope > 1u || c->expert_count > SONG_EXPERT_MAX) {
        return 0;
    }
    for (uint32_t f = 0; f < c->expert_count && f < SID_EXPERT_COUNT; f++) {
        if (c->expert[f] > SID_CHARACTER_MAX && c->expert[f] != SID_EXPERT_DEFAULT) {
            return 0;
        }
    }
    return 1;
}

static int chip_a_song(void* ctx, const song_chip_setup* setup) {
    (void)ctx;
    (void)setup;
    sid_set_gate_reset(&chip, !envelope);
    return 1;
}

static void chip_a_character(void* ctx, uint32_t field, int32_t value) {
    (void)ctx;
    uint8_t* to[4] = {&character.age, &character.dac, &character.thump, &character.bleed};
    const uint8_t* from[4] = {&base_character.age, &base_character.dac, &base_character.thump,
                              &base_character.bleed};
    if (field >= 4u) {
        return;
    }
    *to[field] = value < 0 ? *from[field] : (uint8_t)value;
    sid_character_change(&chip, &character, &dac);
}

/* The chip and the slots the song asks for: CHPA's model and character, else OPTS's model with
 * the default character and AUTHENTIC envelopes; SLOT's three slots, settled. */
static int transport(gt2_song* s) {
    playing = s;
    if (!s) {
        return 0;
    }
    song_chip_setup c;
    sid_character_init(&character);
    envelope = 1u;
    sid_model model;
    if (song_setup(s, &c)) {
        model = c.model ? SID_MODEL_8580 : SID_MODEL_6581;
        character.profile = (uint8_t)(c.profile + 1u);
        character.age = c.age;
        character.dac = c.dac;
        character.thump = c.thump;
        character.bleed = c.bleed;
        for (uint32_t f = 0; f < SID_EXPERT_COUNT; f++) {
            sid_character_expert_set(&character, model, f,
                                     f < c.expert_count ? c.expert[f] : SID_EXPERT_DEFAULT);
        }
        envelope = c.envelope;
    } else {
        gt2_options o;
        song_ext_get_options(s, &o);
        model = o.sid_model ? SID_MODEL_8580 : SID_MODEL_6581;
    }
    base_character = character;
    sid_init(&chip, model, SID_CLOCK_PAL, SC_RATE);
    sid_character_apply(&chip, &character, &dac);
    song_ctl_init(&sc, s, &shapes[0], &shapes[1], &work);
    song_chip_hooks hooks = {chip_a_song, NULL, chip_a_character, NULL};
    song_ctl_set_hooks(&sc, &hooks);
    sid_slots_init(&slots, SC_RATE, SC_SLOTS_SEED, &slot_pool);
    song_slots v;
    if (song_ext_get_slots(s, &v)) {
        for (uint32_t k = 0; k < v.count && k < SID_SLOT_COUNT; k++) {
            const float to1 = 1.0f / 127.0f;
            sid_slot_params p = {v.slot[k][0], (float)v.slot[k][1] * to1, (float)v.slot[k][2] * to1,
                                 (float)v.slot[k][3] * to1, (float)v.slot[k][4] * to1};
            sid_slots_set(&slots, k, &p);
        }
        /* The FM-1's slots took the song's settings when it loaded, well before PLAY: here two
         * seconds of silence bring them there (their switch, its warm-up and fade). */
        memset(pcm, 0, SONG_BLOCK * sizeof pcm[0]);
        for (uint32_t b = 0; b < 2u * PCM_BLOCKS; b++) {
            sid_slots_process(&slots, pcm, pcm_out, SONG_BLOCK);
        }
    }
    return 1;
}

/* Plays subtune from orderlist entry pos of each channel (0: its start): 1, or 0 and
 * sc_play_error() (GT2_PLAY_ERR_*, SONG_ERR_*). */
EXPORT(sc_play_from) int32_t sc_play_from(uint32_t subtune, uint32_t pos) {
    if (!transport(song)) {
        return 0;
    }
    return song_ctl_play(&sc, subtune, pos) != 0;
}
EXPORT(sc_play) int32_t sc_play(uint32_t subtune) {
    return sc_play_from(subtune, 0);
}
EXPORT(sc_play_error) int32_t sc_play_error(void) {
    return sc.error;
}

/* Plays one pattern over and over on voice `channel` (0-2), the others muted: a copy of the
 * song whose first subtune's orderlists all name it, so the song is untouched. 1, or 0 and
 * sc_play_error() (or a gt2_err from making the copy, negated, minus 100). */
EXPORT(sc_play_pattern) int32_t sc_play_pattern(uint32_t pattern, uint32_t channel) {
    if (!song || channel >= GT2_MONO_CHANNELS || song->channels != GT2_MONO_CHANNELS) {
        sc.error = song && song->channels != GT2_MONO_CHANNELS ? -2 : -1;
        return 0;
    }
    memcpy(scratch, region, sizeof region);
    gt2_song* s = (gt2_song*)(void*)scratch;
    uint8_t entry = (uint8_t)pattern;
    for (uint32_t ch = 0; ch < GT2_MONO_CHANNELS; ch++) {
        gt2_edit e;
        memset(&e, 0, sizeof e);
        gt2_err r = gt2_order_replace(s, 0, ch, &entry, 1, 0, &e);
        while (r == GT2_ERR_FRAGMENTED) {
            gt2_edit m;
            memset(&m, 0, sizeof m);
            if (gt2_compact_step(s, &m) <= 0) {
                break;
            }
            gt2_edit_commit(s, &m);
            r = gt2_order_replace(s, 0, ch, &entry, 1, 0, &e);
        }
        if (r != GT2_OK) {
            sc.error = (int16_t)(-100 - (int32_t)r);
            playing = NULL;
            return 0;
        }
        gt2_edit_commit(s, &e);
    }
    transport(s);
    if (!song_ctl_play(&sc, 0, 0)) {
        return 0;
    }
    song_ctl_mute(&sc, 7u & ~(1u << channel));
    return 1;
}

/* The audition (the song stopped, jams sound): 1 once a jam would sound, else 0. */
EXPORT(sc_audition) int32_t sc_audition(void) {
    if (!transport(song) || !song_ctl_audition(&sc)) {
        return 0;
    }
    for (uint32_t i = 0; i < 1000u && !song_ctl_jam_ready(&sc); i++) {
        pass();
        block(pcm);
    }
    return song_ctl_jam_ready(&sc);
}

/* Voice v takes note (0 C-0 .. 95; SONG_JAM_RELEASE lets it go) and instrument at its next
 * tick 0. */
EXPORT(sc_jam) int32_t sc_jam(uint32_t voice, uint32_t note, uint32_t instrument) {
    int r = song_ctl_jam(&sc, voice, note, instrument);
    pass();
    return r;
}

/* Stops what plays (each voice's release rings on, as the FM-1's STOP). */
EXPORT(sc_stop) int32_t sc_stop(void) {
    if (!playing) {
        return 0;
    }
    int r = song_ctl_stop(&sc);
    pass();
    return r;
}

/* The next blocks (at most PCM_BLOCKS) of chip A's mono 16-bit PCM at sc_rate() into sc_pcm():
 * the sample count. */
EXPORT(sc_render) uint32_t sc_render(uint32_t blocks) {
    if (!playing) {
        return 0;
    }
    if (blocks > PCM_BLOCKS) {
        blocks = PCM_BLOCKS;
    }
    for (uint32_t b = 0; b < blocks; b++) {
        pass();
        block(pcm + b * SONG_BLOCK);
    }
    return blocks * SONG_BLOCK;
}

/* The same through the effect slots: interleaved stereo (left first) into sc_pcm_out(), the
 * frame count. */
EXPORT(sc_render_out) uint32_t sc_render_out(uint32_t blocks) {
    uint32_t n = sc_render(blocks);
    sid_slots_process(&slots, pcm, pcm_out, n);
    return n;
}
EXPORT(sc_pcm_out) int16_t* sc_pcm_out(void) {
    return pcm_out;
}

/* Calls of the play routine since the play (for a song's length in the preview). */
EXPORT(sc_calls) uint32_t sc_calls(void) {
    return sc.calls;
}
