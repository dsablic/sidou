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

#include "player/gt2_shape.h"
#include "player/song_ctl.h"
#include "sid/sid.h"
#include "song/gt2_ins.h"
#include "song/gt2_sng.h"
#include "song/gt2_song.h"
#include "song/song_ext.h"

#define EXPORT(name) __attribute__((export_name(#name)))

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

/* --- Preview: the device's song transport (song_ctl) into chip A, as the audio interrupt
 * runs it, a block of SONG_BLOCK samples at a time with a main-loop pass between. */

static void pass(void) {
    song_ctl_poll(&sc, SONG_POLL_SLICE | SONG_POLL_RING);
    song_ctl_commit(&sc);
}

static void block(int16_t* out) {
    song_ctl_block(&sc, &chip);
    song_ctl_render(&sc, &chip, out, SONG_BLOCK);
    song_ctl_block_end(&sc, out, SONG_BLOCK);
}

/* The chip the song asks for: CHPA's model, else OPTS's. */
static sid_model song_model(void) {
    song_chip_setup c;
    if (song_ext_get_chip(song, SONG_TAG_CHPA, &c)) {
        return c.model ? SID_MODEL_8580 : SID_MODEL_6581;
    }
    gt2_options o;
    song_ext_get_options(song, &o);
    return o.sid_model ? SID_MODEL_8580 : SID_MODEL_6581;
}

static int transport(void) {
    if (!song) {
        return 0;
    }
    sid_init(&chip, song_model(), SID_CLOCK_PAL, SC_RATE);
    song_ctl_init(&sc, song, &shapes[0], &shapes[1], &work);
    return 1;
}

/* Plays subtune from its start: 1, or 0 and sc_play_error() (GT2_PLAY_ERR_*, SONG_ERR_*). */
EXPORT(sc_play) int32_t sc_play(uint32_t subtune) {
    if (!transport()) {
        return 0;
    }
    return song_ctl_play(&sc, subtune, 0) != 0;
}
EXPORT(sc_play_error) int32_t sc_play_error(void) {
    return sc.error;
}

/* The audition (the song stopped, jams sound): 1 once a jam would sound, else 0. */
EXPORT(sc_audition) int32_t sc_audition(void) {
    if (!transport() || !song_ctl_audition(&sc)) {
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

/* The next blocks (at most PCM_BLOCKS) of mono 16-bit PCM at sc_rate() into sc_pcm(): the
 * sample count. */
EXPORT(sc_render) uint32_t sc_render(uint32_t blocks) {
    if (!song) {
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

/* Calls of the play routine since the play (for a song's length in the preview). */
EXPORT(sc_calls) uint32_t sc_calls(void) {
    return sc.calls;
}
