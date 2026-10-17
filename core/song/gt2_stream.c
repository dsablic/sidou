// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "gt2_stream.h"

#include <stddef.h>
#include <string.h>

#include "song_ext.h"

enum {
    R_GUESS = 1, R_START, R_IDENT, R_HEADER, R_ORDER, R_INSTR_COUNT, R_INSTR,
    R_TABLE_COUNT, R_TABLE_SIDE, R_PATTERN_COUNT, R_PATTERN, R_PATTERN_BODY, R_MISSING,
    R_CONVERT, R_TRAILER, R_TRAILER_CRC, R_CHUNK, R_CHUNK_DATA, R_FINISH,
    R_WIPE, R_BITMAP, R_RECOUNT, W_HEADER = 40,
    W_ORDER, W_INSTR_COUNT, W_INSTR, W_TABLE_COUNT, W_TABLE_SIDE,
    W_PATTERN_COUNT, W_PATTERN, W_TRAILER, W_LEAD, W_GTCRC, W_EXT,
    W_PAD_HEADER, W_PAD, W_CRC, DONE = 100, FAILED
};

_Static_assert(sizeof(gt2_stream) <= 1024u, "song IO workspace exceeds budget");

static uint32_t get32(const uint8_t* b) {
    return (uint32_t)b[0] | (uint32_t)b[1] << 8 | (uint32_t)b[2] << 16 | (uint32_t)b[3] << 24;
}

static void set32(uint8_t* b, uint32_t v) {
    for (uint32_t i = 0; i < 4u; i++)
        b[i] = (uint8_t)(v >> (8u * i));
}

static int bytes_at(gt2_stream* w, uint32_t at, uint8_t* b, uint32_t n) {
    while (n) {
        uint32_t got = w->read(w->ctx, at, b, n);
        if (!got || got > n)
            return -1;
        at += got;
        b += got;
        n -= got;
    }
    return 0;
}

static int take(gt2_stream* w, uint32_t n) {
    if (bytes_at(w, w->pos, w->buf, n) != 0)
        return -1;
    w->crc = song_crc32(w->crc, w->buf, n);
    w->pos += n;
    return 0;
}

static void message(gt2_stream* w, const char* text) {
    uint32_t i = 0;
    while (text[i] && i < sizeof w->info.msg - 1u) {
        w->info.msg[i] = text[i];
        i++;
    }
    w->info.msg[i] = 0;
}

static void append(gt2_stream* w, const char* text) {
    uint32_t i = 0;
    while (w->info.msg[i]) {
        i++;
    }
    while (*text && i < sizeof w->info.msg - 1u)
        w->info.msg[i++] = *text++;
    w->info.msg[i] = 0;
}

static void number(gt2_stream* w, uint32_t v) {
    char digits[11], out[12];
    uint32_t n = 0;
    do {
        digits[n++] = (char)('0' + v % 10u);
        v /= 10u;
    } while (v);
    for (uint32_t i = 0; i < n; i++)
        out[i] = digits[n - 1u - i];
    out[n] = 0;
    append(w, out);
}

static int reject(gt2_stream* w, const char* text) {
    message(w, w->loading ? "SNG: READ ERROR" : text);
    w->state = w->loading ? FAILED : R_FINISH;
    w->rank = 0;
    return w->loading ? -1 : 0;
}

static int bad_pattern(gt2_stream* w) {
    int result = reject(w, "SNG: BAD PATTERN ");
    if (!w->loading)
        number(w, w->index);
    return result;
}

static void reset_parse(gt2_stream* w, uint8_t channels) {
    memset(&w->info, 0, sizeof w->info);
    memset(w->named, 0, sizeof w->named);
    w->info.channels = channels;
    w->pos = w->crc = w->index = w->sub = w->channel = 0;
    w->top_named = w->top_instr = w->top_pattern = 0;
    w->canonical = 1;
    w->state = R_IDENT;
}

static uint8_t* allocate(gt2_stream* w, uint32_t bytes, gt2_span* span, uint32_t len) {
    gt2_song* s = w->song;
    if (w->info.need_bytes + bytes > gt2_arena_capacity(s))
        return NULL;
    /* Loading appends packed objects to an empty arena, never first-fit's O(file) search. */
    uint32_t off = s->used_units, units = (bytes + 3u) / GT2_UNIT;
    uint32_t* bitmap = (uint32_t*)((uint8_t*)s + s->bitmap_offset);
    for (uint32_t u = off; u < off + units; u++)
        bitmap[u / 32u] |= 1u << (u % 32u);
    s->used_units += units;
    *span = GT2_SPAN(off, len);
    return (uint8_t*)s + s->arena_offset + off * GT2_UNIT;
}

/* GoatTracker's makespeedtable, fine-vibrato enabled, same conversion as gt2_sng. */
static uint8_t speed(gt2_song* s, uint8_t data, uint32_t mode) {
    if (!data)
        return 0;
    uint8_t l, r;
    if (mode == 14u) {
        l = data >> 4;
        r = data & 15u;
    } else if (mode == 4u) {
        l = (data & 0x70u) >> 4;
        r = (uint8_t)((data & 15u) << 4 | (data & 0x80u) >> 4);
    } else {
        l = data >> 6;
        r = (uint8_t)(data << 2);
    }
    for (uint32_t i = 0; i < GT2_TABLE_ROWS; i++)
        if (s->ltable[GT2_STBL][i] == l && s->rtable[GT2_STBL][i] == r)
            return (uint8_t)(i + 1u);
    for (uint32_t i = 0; i < GT2_TABLE_ROWS; i++) {
        if (!(s->ltable[GT2_STBL][i] | s->rtable[GT2_STBL][i])) {
            s->ltable[GT2_STBL][i] = l;
            s->rtable[GT2_STBL][i] = r;
            return (uint8_t)(i + 1u);
        }
    }
    return 0;
}

static void drop_trailer(gt2_stream* w) {
    w->has_opts = 0;
    w->multispeed = 1;
    w->info.trailer = GT2_TRAILER_DROPPED;
    w->info.file_bytes = w->info.gt2_bytes;
    w->rank = 4;
    if (w->loading)
        w->song->ext_used = 0;
    message(w, "SNG: EXTRAS DROPPED");
    w->state = R_FINISH;
}

const gt2_read_info* gt2_stream_info(const gt2_stream* w) {
    return &w->info;
}

void gt2_stream_read_begin(gt2_stream* w, gt2_song* song, gt2_read_fn read,
                           void* ctx, const gt2_caps* caps) {
    memset(w, 0, sizeof *w);
    w->song = song;
    w->read = read;
    w->ctx = ctx;
    w->max_multispeed = caps ? caps->max_multispeed : 0;
    w->state = R_GUESS;
    w->guess = GT2_MAX_CHANNELS;
    w->pos = 100;
    if (song->frozen) {
        message(w, "SNG: SAVING");
        w->state = FAILED;
    } else if (song->pending || song->pending_loose) {
        message(w, "SNG: EDITS PENDING");
        w->state = FAILED;
    }
}

int gt2_stream_read_step(gt2_stream* w) {
    gt2_song* s = w->song;
    uint32_t n;
    switch (w->state) {
    case R_GUESS:
        if (!w->count) {
            if (bytes_at(w, 100, w->buf, 1) != 0 || !w->buf[0]) {
                w->state = R_START;
                break;
            }
            w->count = w->buf[0] * GT2_MAX_CHANNELS;
            w->pos = 101;
        }
        memset(w->buf, 0, 258);
        (void)bytes_at(w, w->pos, w->buf, 1);
        n = w->buf[0] + 1u;
        memset(w->buf, 0, n);
        (void)bytes_at(w, w->pos + 1u, w->buf, n);
        if (n < 2u || w->buf[n - 2u] != 255u || w->buf[n - 1u] >= n) {
            w->guess = GT2_MONO_CHANNELS;
            w->state = R_START;
        } else {
            w->pos += n + 1u;
            if (++w->index == w->count)
                w->state = R_START;
        }
        break;
    case R_START:
        reset_parse(w, w->guess);
        break;
    case R_IDENT:
        if (take(w, 4) != 0)
            return reject(w, "SNG: SHORT FILE");
        if (memcmp(w->buf, "GTS", 3))
            return reject(w, "SNG: NOT A SONG");
        if (w->buf[3] < '2' || w->buf[3] > '5')
            return reject(w, w->buf[3] == '!' ? "SNG: GT1 FORMAT" : "SNG: NOT A SONG");
        w->info.version = w->buf[3] - '0';
        w->state = R_HEADER;
        break;
    case R_HEADER:
        if (take(w, 97) != 0)
            return reject(w, "SNG: SHORT FILE");
        w->count = w->buf[96];
        if (!w->count || w->count > GT2_MAX_SUBTUNES)
            return reject(w, "SNG: BAD SUBTUNES");
        if (w->loading) {
            memcpy(s->name, w->buf, GT2_STR);
            memcpy(s->author, w->buf + 32, GT2_STR);
            memcpy(s->copyright, w->buf + 64, GT2_STR);
            s->version = w->info.version;
            s->channels = w->info.channels;
            s->subtunes = (uint8_t)w->count;
        }
        w->state = R_ORDER;
        break;
    case R_ORDER: {
        if (take(w, 1) != 0)
            return reject(w, "SNG: SHORT FILE");
        n = w->buf[0];
        if (take(w, n + 1u) != 0)
            return reject(w, "SNG: SHORT FILE");
        uint32_t end = 0;
        while (end < n && w->buf[end] != GT2_LOOPSONG)
            end++;
        if (!n || end >= n || (w->info.channels == 6 && w->buf[end + 1u] >= end + 2u)) {
            message(w, "SNG: BAD ORDERLIST V");
            number(w, w->channel + 1u);
            w->rank = 0;
            w->state = w->loading ? FAILED : R_FINISH;
            break;
        }
        for (uint32_t i = 0; i < end; i++) {
            if (w->buf[i] < GT2_REPEAT) {
                w->named[w->buf[i]] = 1;
                if (w->buf[i] > w->top_named)
                    w->top_named = w->buf[i];
            }
        }
        if (w->sub + 1u == w->count && w->sub && w->channel < 3u && !end)
            w->canonical = 0;
        n = (end + 5u) / 4u * 4u;
        if (w->loading) {
            uint8_t* dst = allocate(w, n, &s->order[w->sub][w->channel], end + 2u);
            if (!dst)
                return reject(w, "SNG: READ ERROR");
            memcpy(dst, w->buf, end + 2u);
        }
        w->info.need_bytes += n;
        if (++w->channel == w->info.channels) {
            w->channel = 0;
            if (++w->sub == w->count)
                w->state = R_INSTR_COUNT;
        }
        break;
    }
    case R_INSTR_COUNT:
        if (take(w, 1) != 0)
            return reject(w, "SNG: SHORT FILE");
        w->instruments = w->buf[0];
        if (w->instruments > GT2_MAX_INSTR)
            return reject(w, "SNG: BAD INSTRUMENTS");
        w->index = 0;
        w->state = R_INSTR;
        break;
    case R_INSTR:
        if (w->index == w->instruments) {
            w->index = 0;
            w->state = R_TABLE_COUNT;
            break;
        }
        if (take(w, GT2_INSTR_BYTES) != 0)
            return reject(w, "SNG: SHORT FILE");
        if (w->buf[0] || w->buf[1] || w->buf[2] || w->buf[3] || w->buf[4] ||
            w->buf[5] || w->buf[6])
            w->top_instr = w->index + 1u;
        if (w->loading) {
            memcpy(s->instr[w->index], w->buf, GT2_INSTR_BYTES);
            if (w->info.version == 2) {
                s->instr[w->index][GT2_INS_VIBDELAY] = w->buf[5];
                s->instr[w->index][GT2_INS_VIBPARAM] = speed(s, w->buf[6], 4);
            }
        }
        w->index++;
        break;
    case R_TABLE_COUNT:
        if (w->index == (w->info.version == 2 ? 3u : 4u)) {
            w->state = R_PATTERN_COUNT;
            break;
        }
        if (take(w, 1) != 0)
            return reject(w, "SNG: SHORT FILE");
        w->count = w->buf[0];
        w->side = 0;
        w->copied = 0;
        w->state = R_TABLE_SIDE;
        break;
    case R_TABLE_SIDE:
        if (take(w, w->count) != 0)
            return reject(w, "SNG: SHORT FILE");
        if (w->count)
            w->copied |= w->buf[w->count - 1u];
        if (w->loading)
            memcpy(w->side ? s->rtable[w->index] : s->ltable[w->index], w->buf, w->count);
        if (++w->side == 2) {
            if (w->count && !w->copied)
                w->canonical = 0;
            w->index++;
            w->state = R_TABLE_COUNT;
        }
        break;
    case R_PATTERN_COUNT:
        if (take(w, 1) != 0)
            return reject(w, "SNG: SHORT FILE");
        w->patterns = w->buf[0];
        if (w->patterns > GT2_MAX_PATTERNS)
            return reject(w, "SNG: BAD PATTERNS");
        w->index = 0;
        w->state = R_PATTERN;
        break;
    case R_PATTERN: {
        if (w->index == w->patterns) {
            w->state = R_MISSING;
            break;
        }
        if (take(w, 1) != 0)
            return reject(w, "SNG: SHORT FILE");
        n = w->buf[0];
        if (!n || n > GT2_MAX_ROWS + 1u)
            return bad_pattern(w);
        if (take(w, n * 4u) != 0)
            return reject(w, "SNG: SHORT FILE");
        w->count = n;
        w->copied = 0;
        w->state = R_PATTERN_BODY;
        break;
    }
    case R_PATTERN_BODY: {
        n = w->count;
        if (w->loading && w->info.version == 2 && w->copied < n) {
            uint8_t* row = w->buf + w->copied++ * 4u;
            if ((row[2] >= 1u && row[2] <= 4u) || row[2] == 14u)
                row[3] = speed(s, row[3], row[2]);
            break;
        }
        uint32_t end = n, deflt = 1;
        for (uint32_t r = 0; r < n; r++) {
            uint8_t* row = w->buf + r * 4u;
            if (r >= end)
                continue;
            if (row[0] == GT2_ENDPATT) {
                end = r;
                deflt &= !(row[1] || row[2] || row[3]);
            } else {
                if (row[0] < GT2_FIRSTNOTE || row[0] > GT2_KEYON ||
                    row[1] > GT2_MAX_INSTR || row[2] > 15u)
                    return bad_pattern(w);
                int rest = row[0] == GT2_REST && !(row[1] || row[2] || row[3]);
                deflt &= (uint32_t)rest;
                if (!rest)
                    w->top_pattern = w->index;
                if (row[1] > w->top_instr)
                    w->top_instr = row[1];
            }
        }
        if (end == n)
            return bad_pattern(w);
        if (!(deflt && end == GT2_DEFAULT_ROWS && !w->named[w->index])) {
            n = (end + 1u) * 4u;
            if (w->loading) {
                uint8_t* dst = allocate(w, n, &s->pattern[w->index], end + 1u);
                if (!dst)
                    return reject(w, "SNG: READ ERROR");
                memcpy(dst, w->buf, n);
            }
            w->info.need_bytes += n;
        }
        w->index++;
        w->state = R_PATTERN;
        break;
    }
    case R_MISSING:
        if (w->index == GT2_MAX_PATTERNS) {
            w->info.gt2_bytes = w->pos;
            w->info.file_bytes = w->pos;
            w->gt_crc = w->crc;
            w->canonical &= w->instruments == w->top_instr &&
                w->patterns == (w->top_named > w->top_pattern ? w->top_named : w->top_pattern) + 1u;
            w->index = 0;
            w->state = w->loading ? R_CONVERT : R_TRAILER;
            break;
        }
        if (w->named[w->index]) {
            n = (GT2_DEFAULT_ROWS + 1u) * 4u;
            if (w->loading) {
                uint8_t* dst = allocate(w, n, &s->pattern[w->index], GT2_DEFAULT_ROWS + 1u);
                if (!dst)
                    return reject(w, "SNG: READ ERROR");
                memset(dst, 0, n);
                for (uint32_t r = 0; r < GT2_DEFAULT_ROWS; r++)
                    dst[r * 4u] = GT2_REST;
                dst[GT2_DEFAULT_ROWS * 4u] = GT2_ENDPATT;
            }
            w->info.need_bytes += n;
        }
        w->index++;
        break;
    case R_CONVERT:
        if (w->info.version < 4) {
            for (uint32_t i = 0; i < GT2_TABLE_ROWS; i++) {
                if (s->ltable[GT2_PTBL][i] < 128u && s->rtable[GT2_PTBL][i]) {
                    int v = (int)(int8_t)s->rtable[GT2_PTBL][i] * 2;
                    s->rtable[GT2_PTBL][i] = (uint8_t)(v > 127 ? 127 : v < -128 ? -128 : v);
                }
            }
        }
        if (w->info.version < 5) {
            for (uint32_t i = 0; i < GT2_MAX_INSTR; i++) {
                uint8_t* in = s->instr[i];
                if (in[GT2_INS_FIRSTWAVE] >= 128u) {
                    in[GT2_INS_GATETIMER] |= 128u;
                    in[GT2_INS_FIRSTWAVE] &= 127u;
                }
                if (!in[GT2_INS_FIRSTWAVE])
                    in[GT2_INS_GATETIMER] |= 64u;
            }
        }
        w->state = R_TRAILER;
        break;
    case R_TRAILER:
        w->trailer_at = w->info.gt2_bytes;
        w->has_opts = 0;
        w->multispeed = 1;
        w->raw_multispeed = 1;
        w->trailer_channels = 0;
        w->kept = 0;
        if (bytes_at(w, w->trailer_at, w->buf, 8) != 0 ||
            memcmp(w->buf, SONG_TRAILER_MAGIC, 8)) {
            w->rank = w->read(w->ctx, w->trailer_at, w->buf, 1) == 0 ? 2u : 0u;
            w->state = R_FINISH;
            break;
        }
        if (bytes_at(w, w->trailer_at + 8u, w->buf, 7) != 0) {
            drop_trailer(w);
            break;
        }
        w->trailer_len = get32(w->buf + 2);
        if (w->buf[0] != 1 || w->buf[1] || w->trailer_len < SONG_TRAILER_MIN ||
            w->trailer_len > 0x7fffffffu - w->trailer_at) {
            drop_trailer(w);
            break;
        }
        w->trailer_channels = w->buf[6];
        w->pos = w->trailer_at;
        w->crc = 0;
        w->state = R_TRAILER_CRC;
        break;
    case R_TRAILER_CRC:
        n = w->trailer_at + w->trailer_len - 4u - w->pos;
        if (n) {
            if (take(w, n > 256u ? 256u : n) != 0)
                drop_trailer(w);
            break;
        }
        if (bytes_at(w, w->pos, w->buf, 4) != 0 || get32(w->buf) != w->crc ||
            bytes_at(w, w->trailer_at + SONG_TRAILER_FF_FREE, w->buf, 4) != 0) {
            drop_trailer(w);
            break;
        }
        w->info.trailer = get32(w->buf) == w->gt_crc ? GT2_TRAILER_READ : GT2_TRAILER_LOCKS_DROPPED;
        w->chunk_at = w->trailer_at + SONG_TRAILER_CHUNKS;
        w->state = R_CHUNK;
        break;
    case R_CHUNK: {
        uint32_t end = w->trailer_at + w->trailer_len - 4u;
        if (w->chunk_at == end) {
            w->info.file_bytes = w->trailer_at + w->trailer_len;
            w->rank = (w->info.trailer == GT2_TRAILER_READ ? 8u : 6u) +
                (w->trailer_channels == w->info.channels);
            if (w->loading)
                s->ext_used = w->kept;
            if (w->info.trailer == GT2_TRAILER_LOCKS_DROPPED)
                message(w, "SNG: LOCKS DROPPED");
            w->state = R_FINISH;
            break;
        }
        if (end - w->chunk_at < 8u || bytes_at(w, w->chunk_at, w->buf, 8) != 0) {
            drop_trailer(w);
            break;
        }
        w->chunk_tag = get32(w->buf);
        w->chunk_len = get32(w->buf + 4);
        if (!w->chunk_len || w->chunk_len > end - w->chunk_at - 8u) {
            drop_trailer(w);
            break;
        }
        if (w->chunk_tag == SONG_TAG_PAD ||
            (w->chunk_tag == SONG_TAG_LOCK && w->info.trailer == GT2_TRAILER_LOCKS_DROPPED)) {
            w->chunk_at += 8u + w->chunk_len;
            break;
        }
        if (w->chunk_len > SONG_EXT_BYTES || w->kept + 8u + w->chunk_len > SONG_EXT_BYTES) {
            drop_trailer(w);
            break;
        }
        if (w->loading)
            memcpy(s->ext + w->kept, w->buf, 8);
        w->copied = 0;
        w->state = R_CHUNK_DATA;
        break;
    }
    case R_CHUNK_DATA:
        n = w->chunk_len - w->copied;
        if (n > 256u)
            n = 256u;
        if (bytes_at(w, w->chunk_at + 8u + w->copied, w->buf, n) != 0) {
            drop_trailer(w);
            break;
        }
        if (!w->copied && w->chunk_tag == SONG_TAG_OPTS && !w->has_opts) {
            gt2_options opts;
            if (song_opts_decode(w->buf, n, &opts) == 0) {
                w->has_opts = 1;
                w->multispeed = opts.multispeed;
                w->raw_multispeed = w->buf[3];
            }
        }
        if (w->loading)
            memcpy(s->ext + w->kept + 8u + w->copied, w->buf, n);
        w->copied += n;
        if (w->copied == w->chunk_len) {
            w->kept += 8u + w->chunk_len;
            w->chunk_at += 8u + w->chunk_len;
            w->state = R_CHUNK;
        }
        break;
    case R_FINISH:
        if (w->loading) {
            if (w->info.gt2_bytes != w->best.gt2_bytes || w->info.need_bytes != w->best.need_bytes ||
                w->info.version != w->best.version || w->info.trailer != w->best.trailer) {
                message(w, "SNG: READ ERROR");
                w->state = FAILED;
                break;
            }
            if (w->max_multispeed && (w->multispeed == 0 || w->multispeed > w->max_multispeed)) {
                message(w, "SNG: MULTISPEED ");
                number(w, w->raw_multispeed);
                append(w, " (MAX ");
                number(w, w->max_multispeed);
                append(w, ")");
                w->state = FAILED;
            } else {
                w->index = 0;
                w->state = R_RECOUNT;
            }
            break;
        }
        if (!w->pass || w->rank > w->best_rank ||
            (w->rank == w->best_rank && w->rank && w->canonical && !w->best_canonical)) {
            w->best = w->info;
            w->best_rank = w->rank;
            w->best_canonical = w->canonical;
        }
        if (!w->pass && w->rank < 9u) {
            w->pass = 1;
            reset_parse(w, w->guess == 6 ? 3 : 6);
            break;
        }
        w->info = w->best;
        if (!w->info.gt2_bytes) {
            w->state = FAILED;
            break;
        }
        if (w->info.need_bytes > gt2_arena_capacity(s)) {
            message(w, "SNG: TOO BIG (");
            number(w, (w->info.need_bytes + 1023u) / 1024u);
            append(w, "K OF ");
            number(w, gt2_arena_capacity(s) / 1024u);
            append(w, "K)");
            w->state = FAILED;
            break;
        }
        w->wipe_at = offsetof(gt2_song, subtunes);
        w->state = R_WIPE;
        break;
    case R_WIPE:
        n = sizeof *s - w->wipe_at;
        if (n > 256u)
            n = 256u;
        memset((uint8_t*)s + w->wipe_at, 0, n);
        w->wipe_at += n;
        if (w->wipe_at == sizeof *s) {
            s->used_units = 0;
            w->wipe_at = 0;
            w->state = R_BITMAP;
        }
        break;
    case R_BITMAP: {
        uint32_t size = (s->arena_units + 31u) / 32u * 4u;
        n = size - w->wipe_at;
        if (n > 256u)
            n = 256u;
        memset((uint8_t*)s + s->bitmap_offset + w->wipe_at, 0, n);
        w->wipe_at += n;
        if (w->wipe_at == size) {
            w->loading = 1;
            reset_parse(w, w->best.channels);
        }
        break;
    }
    case R_RECOUNT:
        if (gt2_saved_recount_step(s, &w->index)) {
            w->info.saved_bytes = gt2_saved_size(s);
            if (s->region_bytes <= SONG_REGION_BYTES && w->info.saved_bytes > GT2_SAVED_MAX) {
                message(w, "SNG: SONG FULL (64K MAX)");
                w->state = FAILED;
            } else {
                w->state = DONE;
            }
        }
        break;
    default:
        break;
    }
    return w->state == DONE ? 1 : w->state == FAILED ? -1 : 0;
}

static void emit(gt2_stream* w, const uint8_t* p, uint32_t n, uint32_t next) {
    if (n)
        memcpy(w->buf, p, n);
    w->crc = song_crc32(w->crc, w->buf, n);
    w->out_at = 0;
    w->out_len = n;
    w->state = next;
}

static void emit8(gt2_stream* w, uint32_t v, uint32_t next) {
    uint8_t b = (uint8_t)v;
    emit(w, &b, 1, next);
}

static int has_ff(uint32_t v) {
    for (uint32_t i = 0; i < 4u; i++)
        if (((v >> (8u * i)) & 255u) == 255u)
            return 1;
    return 0;
}

void gt2_stream_write_begin(gt2_stream* w, const gt2_song* song, int with_trailer) {
    memset(w, 0, sizeof *w);
    w->source = song;
    w->with_trailer = with_trailer != 0;
    w->saved_patterns = (uint8_t)gt2_saved_patterns(song);
    w->saved_instr = (uint8_t)gt2_saved_instruments(song);
    w->saved_subtunes = (uint8_t)gt2_saved_subtunes(song);
    for (uint32_t t = 0; t < GT2_TABLES; t++)
        w->table_rows[t] = gt2_saved_table_rows(song, t);
    w->state = W_HEADER;
}

static void write_advance(gt2_stream* w) {
    const gt2_song* s = w->source;
    uint32_t n;
    switch (w->state) {
    case W_HEADER:
        memcpy(w->buf, "GTS5", 4);
        memcpy(w->buf + 4, s->name, 32);
        memcpy(w->buf + 36, s->author, 32);
        memcpy(w->buf + 68, s->copyright, 32);
        w->buf[100] = w->saved_subtunes;
        w->out_len = 101;
        w->out_at = 0;
        w->crc = song_crc32(0, w->buf, 101);
        w->sub = w->channel = 0;
        w->state = W_ORDER;
        break;
    case W_ORDER: {
        const uint8_t* od = gt2_order_data(s, w->sub, w->channel);
        n = od ? gt2_order_entries(s, w->sub, w->channel) : 0;
        w->buf[0] = (uint8_t)(n + 1u);
        if (od)
            memcpy(w->buf + 1, od, n + 2u);
        else {
            w->buf[1] = GT2_LOOPSONG;
            w->buf[2] = 0;
        }
        w->out_len = n + 3u;
        w->out_at = 0;
        w->crc = song_crc32(w->crc, w->buf, w->out_len);
        if (++w->channel == s->channels) {
            w->channel = 0;
            if (++w->sub == w->saved_subtunes)
                w->state = W_INSTR_COUNT;
        }
        break;
    }
    case W_INSTR_COUNT:
        w->index = 0;
        emit8(w, w->saved_instr, W_INSTR);
        break;
    case W_INSTR:
        if (w->index == w->saved_instr) {
            w->index = 0;
            w->state = W_TABLE_COUNT;
        } else {
            emit(w, s->instr[w->index++], GT2_INSTR_BYTES, W_INSTR);
        }
        break;
    case W_TABLE_COUNT:
        if (w->index == 4u) {
            w->state = W_PATTERN_COUNT;
        } else {
            w->side = 0;
            emit8(w, w->table_rows[w->index], W_TABLE_SIDE);
        }
        break;
    case W_TABLE_SIDE:
        emit(w, w->side ? s->rtable[w->index] : s->ltable[w->index], w->table_rows[w->index],
             W_TABLE_SIDE);
        if (++w->side == 2u) {
            w->index++;
            w->state = W_TABLE_COUNT;
        }
        break;
    case W_PATTERN_COUNT:
        w->index = 0;
        w->side = 0;
        emit8(w, w->saved_patterns, W_PATTERN);
        break;
    case W_PATTERN: {
        if (w->index == w->saved_patterns) {
            w->state = w->with_trailer ? W_TRAILER : DONE;
            break;
        }
        const uint8_t* pd = gt2_pattern_data(s, w->index);
        n = pd ? GT2_SPAN_LEN(s->pattern[w->index]) : GT2_DEFAULT_ROWS + 1u;
        if (!w->side) {
            w->side = 1;
            emit8(w, n, W_PATTERN);
        } else {
            if (pd) {
                emit(w, pd, n * 4u, W_PATTERN);
            } else {
                memset(w->buf, 0, n * 4u);
                for (uint32_t i = 0; i + 1u < n; i++)
                    w->buf[i * 4u] = GT2_REST;
                w->buf[(n - 1u) * 4u] = GT2_ENDPATT;
                w->out_len = n * 4u;
                w->out_at = 0;
                w->crc = song_crc32(w->crc, w->buf, w->out_len);
            }
            w->index++;
            w->side = 0;
        }
        break;
    }
    case W_TRAILER:
        w->gt_crc = w->crc;
        w->crc = 0;
        w->trailer_len = SONG_TRAILER_MIN + s->ext_used;
        w->pad = 0;
        w->side = has_ff(w->trailer_len);
        if (w->side) {
            while (has_ff(w->trailer_len + 9u + w->pad))
                w->pad++;
            w->trailer_len += 9u + w->pad;
        }
        memcpy(w->buf, SONG_TRAILER_MAGIC, 8);
        w->buf[8] = 1;
        w->buf[9] = 0;
        set32(w->buf + 10, w->trailer_len);
        w->buf[14] = s->channels;
        w->out_len = 15;
        w->out_at = 0;
        w->crc = song_crc32(0, w->buf, 15);
        w->state = W_LEAD;
        break;
    case W_LEAD:
        memset(w->buf, 0, SONG_TRAILER_FF_FREE - 15u);
        w->out_len = SONG_TRAILER_FF_FREE - 15u;
        w->out_at = 0;
        w->crc = song_crc32(w->crc, w->buf, w->out_len);
        w->state = W_GTCRC;
        break;
    case W_GTCRC:
        set32(w->buf, w->gt_crc);
        w->out_len = 4;
        w->out_at = 0;
        w->crc = song_crc32(w->crc, w->buf, 4);
        w->copied = 0;
        w->state = W_EXT;
        break;
    case W_EXT:
        n = s->ext_used - w->copied;
        if (n > 256u)
            n = 256u;
        emit(w, s->ext + w->copied, n, W_EXT);
        w->copied += n;
        if (w->copied == s->ext_used)
            w->state = w->side ? W_PAD_HEADER : W_CRC;
        break;
    case W_PAD_HEADER:
        set32(w->buf, SONG_TAG_PAD);
        set32(w->buf + 4, w->pad + 1u);
        w->buf[8] = 1;
        w->out_len = 9;
        w->out_at = 0;
        w->crc = song_crc32(w->crc, w->buf, 9);
        w->state = W_PAD;
        break;
    case W_PAD:
        n = w->pad > 256u ? 256u : w->pad;
        memset(w->buf, 0, n);
        w->out_len = n;
        w->out_at = 0;
        w->crc = song_crc32(w->crc, w->buf, n);
        w->pad -= n;
        if (!w->pad)
            w->state = W_CRC;
        break;
    case W_CRC:
        set32(w->buf, w->crc);
        w->out_len = 4;
        w->out_at = 0;
        w->state = DONE;
        break;
    default:
        break;
    }
}

int gt2_stream_write_step(gt2_stream* w, uint8_t* buf, uint32_t capacity) {
    if (!capacity || !buf || w->state == FAILED) {
        message(w, "SNG: WRITE ERROR");
        w->state = FAILED;
        return -1;
    }
    while (w->out_at == w->out_len) {
        if (w->state == DONE)
            return 0;
        write_advance(w);
    }
    uint32_t n = w->out_len - w->out_at;
    if (n > capacity)
        n = capacity;
    if (n > 256u)
        n = 256u;
    memcpy(buf, w->buf + w->out_at, n);
    w->out_at += n;
    return (int)n;
}
