// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The .sng reader and writer (gt2_sng.h).
 *
 * The format, the older idents' conversions and what the writer trims follow GoatTracker
 * 2.77's gsong.c (loadsong, savesong, countpatternlengths) and gtable.c (makespeedtable),
 * GPL-2.0-or-later, whose "any later version" lets this file stay GPL-3.0-or-later; the
 * channel count follows GoatTracker Stereo 2.77's determinechannels. Decision record 0007
 * quotes and explains each. */
#include "gt2_sng.h"

#include <string.h>

#include "song_ext.h"

/* A cursor over the reader, through a small buffer. */
typedef struct {
    gt2_read_fn read;
    void* ctx;
    uint32_t pos;
    uint32_t buf_off, buf_len;
    uint8_t buf[64];
} cursor;

static void cur_init(cursor* c, gt2_read_fn read, void* ctx, uint32_t pos) {
    c->read = read;
    c->ctx = ctx;
    c->pos = pos;
    c->buf_off = 0;
    c->buf_len = 0;
}

static int cur_byte(cursor* c, uint8_t* b) {
    if (c->pos < c->buf_off || c->pos >= c->buf_off + c->buf_len) {
        uint32_t got = c->read(c->ctx, c->pos, c->buf, (uint32_t)sizeof c->buf);
        c->buf_off = c->pos;
        c->buf_len = got > sizeof c->buf ? (uint32_t)sizeof c->buf : got;
        if (c->buf_len == 0u)
            return -1;
    }
    *b = c->buf[c->pos - c->buf_off];
    c->pos++;
    return 0;
}

static int cur_bytes(cursor* c, uint8_t* out, uint32_t n) {
    uint8_t skip = 0;
    for (uint32_t i = 0; i < n; i++)
        if (cur_byte(c, out ? &out[i] : &skip) != 0)
            return -1;
    return 0;
}

static uint32_t le32(const uint8_t* p) {
    return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24;
}

/* The CRC-32 of the file's bytes [from, to); -1 in *ok if the file is shorter. */
static uint32_t crc_range(gt2_read_fn read, void* ctx, uint32_t from, uint32_t to, int* ok) {
    uint8_t buf[64];
    uint32_t crc = 0;
    *ok = 1;
    while (from < to) {
        uint32_t want = to - from < sizeof buf ? to - from : (uint32_t)sizeof buf;
        uint32_t got = read(ctx, from, buf, want);
        if (got > want)
            got = want;
        if (got == 0u) {
            *ok = 0;
            return 0;
        }
        crc = song_crc32(crc, buf, got);
        from += got;
    }
    return crc;
}

uint32_t gt2_sng_channels(gt2_read_fn read, void* ctx) {
    cursor c;
    uint8_t songs = 0;
    cur_init(&c, read, ctx, 100u);
    if (cur_byte(&c, &songs) != 0)
        songs = 0;
    for (uint32_t d = 0; d < songs; d++) {
        for (uint32_t ch = 0; ch < GT2_MAX_CHANNELS; ch++) {
            uint8_t n = 0;
            uint8_t buf[257];
            if (cur_byte(&c, &n) != 0)
                n = 0;
            uint32_t loadsize = (uint32_t)n + 1u;
            memset(buf, 0, sizeof buf);
            for (uint32_t i = 0; i < loadsize; i++)
                if (cur_byte(&c, &buf[i]) != 0)
                    buf[i] = 0;
            /* A length of 0 makes GoatTracker Stereo read buf[-1]: SIDOU calls it a fail. */
            if (loadsize < 2u || buf[loadsize - 2u] != 0xffu || buf[loadsize - 1u] >= loadsize)
                return GT2_MONO_CHANNELS;
        }
    }
    return GT2_MAX_CHANNELS;
}

/* Messages. */

static void msg_set(char* m, const char* a) {
    uint32_t i = 0;
    while (a[i] && i < 31u) {
        m[i] = a[i];
        i++;
    }
    m[i] = 0;
}

static void msg_add(char* m, const char* a) {
    uint32_t i = 0;
    while (m[i])
        i++;
    while (*a && i < 31u)
        m[i++] = *a++;
    m[i] = 0;
}

static void msg_num(char* m, uint32_t v) {
    char d[11];
    uint32_t n = 0;
    do {
        d[n++] = (char)('0' + v % 10u);
        v /= 10u;
    } while (v && n < sizeof d);
    char r[12];
    for (uint32_t i = 0; i < n; i++)
        r[i] = d[n - 1u - i];
    r[n] = 0;
    msg_add(m, r);
}

/* GoatTracker's makespeedtable (gtable.c), on the song's speedtable, makenew 0: an equal
 * entry if there is one, else the first empty one; -1 if data is 0 or the table is full. */
enum { MST_NOFINEVIB, MST_FINEVIB, MST_FUNKTEMPO, MST_PORTAMENTO };

static int makespeedtable(gt2_song* s, uint32_t data, int mode) {
    uint8_t l = 0, r = 0;
    if (!data)
        return -1;
    switch (mode) {
    case MST_NOFINEVIB:
        l = (uint8_t)((data & 0xf0u) >> 4);
        r = (uint8_t)((data & 0x0fu) << 4);
        break;
    case MST_FINEVIB:
        l = (uint8_t)((data & 0x70u) >> 4);
        r = (uint8_t)(((data & 0x0fu) << 4) | ((data & 0x80u) >> 4));
        break;
    case MST_FUNKTEMPO:
        l = (uint8_t)((data & 0xf0u) >> 4);
        r = (uint8_t)(data & 0x0fu);
        break;
    default: /* MST_PORTAMENTO */
        l = (uint8_t)((data << 2) >> 8);
        r = (uint8_t)((data << 2) & 0xffu);
        break;
    }
    for (uint32_t c = 0; c < GT2_TABLE_ROWS; c++)
        if (s->ltable[GT2_STBL][c] == l && s->rtable[GT2_STBL][c] == r)
            return (int)c;
    for (uint32_t c = 0; c < GT2_TABLE_ROWS; c++) {
        if (!s->ltable[GT2_STBL][c] && !s->rtable[GT2_STBL][c]) {
            s->ltable[GT2_STBL][c] = l;
            s->rtable[GT2_STBL][c] = r;
            return (int)c;
        }
    }
    return -1;
}

/* GTS2's commands with a speedtable parameter (loadsong's "Convert speedtable-requiring
 * commands"); fine vibrato is GoatTracker's default, on. */
static void gts2_row(gt2_song* s, uint8_t* row) {
    switch (row[2]) {
    case 14: /* CMD_FUNKTEMPO */
        row[3] = (uint8_t)(makespeedtable(s, row[3], MST_FUNKTEMPO) + 1);
        break;
    case 1: /* CMD_PORTAUP */
    case 2: /* CMD_PORTADOWN */
    case 3: /* CMD_TONEPORTA */
        row[3] = (uint8_t)(makespeedtable(s, row[3], MST_PORTAMENTO) + 1);
        break;
    case 4: /* CMD_VIBRATO */
        row[3] = (uint8_t)(makespeedtable(s, row[3], MST_FINEVIB) + 1);
        break;
    default:
        break;
    }
}

/* The conversions loadsong makes after any older ident. */
static void convert_old(gt2_song* s, uint32_t version) {
    if (version < 4u) {
        /* Pulse modulation speeds of songs before v2.4, doubled. */
        for (uint32_t c = 0; c < GT2_TABLE_ROWS; c++) {
            if (s->ltable[GT2_PTBL][c] < 0x80u && s->rtable[GT2_PTBL][c]) {
                int speed = (int)(int8_t)s->rtable[GT2_PTBL][c] * 2;
                if (speed > 127)
                    speed = 127;
                if (speed < -128)
                    speed = -128;
                s->rtable[GT2_PTBL][c] = (uint8_t)speed;
            }
        }
    }
    if (version < 5u) {
        /* The old legato and no-hard-restart parameters, for every instrument slot. */
        for (uint32_t c = 0; c < GT2_MAX_INSTR; c++) {
            uint8_t* in = s->instr[c];
            if (in[GT2_INS_FIRSTWAVE] >= 0x80u) {
                in[GT2_INS_GATETIMER] |= 0x80u;
                in[GT2_INS_FIRSTWAVE] &= 0x7fu;
            }
            if (!in[GT2_INS_FIRSTWAVE])
                in[GT2_INS_GATETIMER] |= 0x40u;
        }
    }
}

static int has_ff(uint32_t v) {
    for (uint32_t i = 0; i < 4u; i++)
        if (((v >> (8u * i)) & 0xffu) == 0xffu)
            return 1;
    return 0;
}

/* The trailer at e: GT2_TRAILER_*; the chunk bytes to keep in *keep; the options; the GT2
 * part's channel count it records (0 if none). Loads the kept chunks into s if s is not
 * NULL. */
static int trailer(gt2_read_fn read, void* ctx, uint32_t e, gt2_song* s, uint32_t* keep,
                   uint32_t* length, gt2_options* opts, int* has_opts, uint8_t* channels) {
    uint8_t h[SONG_TRAILER_HEADER + 1u];
    cursor c;
    *keep = 0;
    *length = 0;
    *has_opts = 0;
    *channels = 0;
    gt2_options_default(opts);
    cur_init(&c, read, ctx, e);
    if (cur_bytes(&c, h, 8u) != 0 || memcmp(h, SONG_TRAILER_MAGIC, 8u) != 0)
        return GT2_TRAILER_NONE;
    if (cur_bytes(&c, h + 8u, 7u) != 0)
        return GT2_TRAILER_DROPPED;
    uint32_t version = (uint32_t)h[8] | (uint32_t)h[9] << 8;
    uint32_t len = le32(h + 10);
    if (version != SONG_TRAILER_VERSION || len < SONG_TRAILER_MIN || len > 0x7fffffffu - e)
        return GT2_TRAILER_DROPPED;
    int ok = 0;
    uint8_t stored[4];
    cur_init(&c, read, ctx, e + len - 4u);
    if (cur_bytes(&c, stored, 4u) != 0)
        return GT2_TRAILER_DROPPED;
    if (crc_range(read, ctx, e, e + len - 4u, &ok) != le32(stored) || !ok)
        return GT2_TRAILER_DROPPED;
    cur_init(&c, read, ctx, e + SONG_TRAILER_FF_FREE);
    if (cur_bytes(&c, stored, 4u) != 0)
        return GT2_TRAILER_DROPPED;
    int gt2_ok = crc_range(read, ctx, 0u, e, &ok) == le32(stored) && ok;
    uint8_t chans = h[SONG_TRAILER_CHANNELS];
    if (chans == GT2_MONO_CHANNELS || chans == GT2_MAX_CHANNELS)
        *channels = chans;
    /* The chunks: well-formed, and what is kept fits the extension area. */
    uint32_t at = e + SONG_TRAILER_CHUNKS, end = e + len - 4u, kept = 0;
    while (at < end) {
        uint8_t ch[SONG_CHUNK_HEADER];
        if (end - at < SONG_CHUNK_HEADER)
            return GT2_TRAILER_DROPPED;
        cur_init(&c, read, ctx, at);
        if (cur_bytes(&c, ch, SONG_CHUNK_HEADER) != 0)
            return GT2_TRAILER_DROPPED;
        uint32_t tag = le32(ch), clen = le32(ch + 4);
        if (clen == 0u || clen > end - at - SONG_CHUNK_HEADER)
            return GT2_TRAILER_DROPPED;
        int keep_it = tag != SONG_TAG_PAD && (gt2_ok || tag != SONG_TAG_LOCK);
        if (keep_it) {
            if (clen > SONG_EXT_BYTES || kept + SONG_CHUNK_HEADER + clen > SONG_EXT_BYTES)
                return GT2_TRAILER_DROPPED;
            if (s) {
                for (uint32_t i = 0; i < SONG_CHUNK_HEADER; i++)
                    s->ext[kept + i] = ch[i];
                if (cur_bytes(&c, s->ext + kept + SONG_CHUNK_HEADER, clen) != 0)
                    return GT2_TRAILER_DROPPED;
                s->ext_used = kept + SONG_CHUNK_HEADER + clen;
            }
            if (tag == SONG_TAG_OPTS && !*has_opts) {
                uint8_t o[SONG_OPTS_BYTES];
                uint32_t n = clen < SONG_OPTS_BYTES ? clen : SONG_OPTS_BYTES;
                cur_init(&c, read, ctx, at + SONG_CHUNK_HEADER);
                if (cur_bytes(&c, o, n) != 0)
                    return GT2_TRAILER_DROPPED;
                *has_opts = song_opts_decode(o, n, opts) == 0;
            }
            kept += SONG_CHUNK_HEADER + clen;
        }
        at += SONG_CHUNK_HEADER + clen;
    }
    *keep = kept;
    *length = len;
    return gt2_ok ? GT2_TRAILER_READ : GT2_TRAILER_LOCKS_DROPPED;
}

/* One pass over the GT2 part: checks it (s NULL) or loads it into s. 0, or -1 and the
 * message. The patterns the orderlists name are marked in named[]. *canonical (if not NULL)
 * is 1 if every count is what savesong would write for the content read (gt2_saved_*), as
 * in any file GoatTracker or SIDOU saved: the last subtune's first three orderlists, the last
 * instrument, each table's last row and the last pattern all in use. */
static int gt2_part(gt2_read_fn read, void* ctx, gt2_song* s, gt2_read_info* info,
                    uint8_t named[GT2_MAX_PATTERNS], int* canonical) {
    cursor c;
    uint8_t b[4];
    uint32_t need = 0;
    int canon = 1;
    uint32_t top_named = 0, top_instr = 0, top_pattern = 0;
    cur_init(&c, read, ctx, 0u);
    if (cur_bytes(&c, b, 4u) != 0) {
        msg_set(info->msg, "SNG: SHORT FILE");
        return -1;
    }
    if (b[0] != 'G' || b[1] != 'T' || b[2] != 'S') {
        msg_set(info->msg, "SNG: NOT A SONG");
        return -1;
    }
    if (b[3] == '!') {
        msg_set(info->msg, "SNG: GT1 FORMAT");
        return -1;
    }
    if (b[3] < '2' || b[3] > '5') {
        msg_set(info->msg, "SNG: NOT A SONG");
        return -1;
    }
    uint32_t version = (uint32_t)(b[3] - '0');
    info->version = (uint8_t)version;
    if (cur_bytes(&c, s ? (uint8_t*)s->name : NULL, GT2_STR) != 0 ||
        cur_bytes(&c, s ? (uint8_t*)s->author : NULL, GT2_STR) != 0 ||
        cur_bytes(&c, s ? (uint8_t*)s->copyright : NULL, GT2_STR) != 0)
        goto short_file;
    uint8_t subtunes = 0;
    if (cur_byte(&c, &subtunes) != 0)
        goto short_file;
    if (subtunes == 0u || subtunes > GT2_MAX_SUBTUNES) {
        msg_set(info->msg, "SNG: BAD SUBTUNES");
        return -1;
    }
    if (s) {
        s->subtunes = subtunes;
        s->channels = info->channels;
        s->version = (uint8_t)version;
    }
    /* Orderlists. */
    for (uint32_t d = 0; d < subtunes; d++) {
        for (uint32_t ch = 0; ch < info->channels; ch++) {
            uint8_t n = 0, o[256];
            if (cur_byte(&c, &n) != 0 || cur_bytes(&c, o, (uint32_t)n + 1u) != 0)
                goto short_file;
            uint32_t i = 0;
            while (i < n && o[i] != GT2_LOOPSONG)
                i++;
            /* A stereo song's orderlist must stay one GoatTracker Stereo's check passes once
             * saved ($FF, then a restart below the entries plus two), or the file it is
             * saved as would read as mono. */
            if (n == 0u || i >= n ||
                (info->channels == GT2_MAX_CHANNELS && o[i + 1u] >= i + 2u)) {
                msg_set(info->msg, "SNG: BAD ORDERLIST V");
                msg_num(info->msg, ch + 1u);
                return -1;
            }
            for (uint32_t k = 0; k < i; k++) {
                if (o[k] < GT2_REPEAT) {
                    named[o[k]] = 1u;
                    if (o[k] > top_named)
                        top_named = o[k];
                }
            }
            if (d + 1u == subtunes && d > 0u && ch < GT2_MONO_CHANNELS && i == 0u)
                canon = 0;
            need += ((i + 2u + 3u) / 4u) * 4u;
            if (s) {
                uint32_t off4 = 0;
                if (gt2_arena_alloc(s, i + 2u, &off4) != 0)
                    goto read_error;
                uint8_t* dst = (uint8_t*)s + s->arena_offset + off4 * GT2_UNIT;
                for (uint32_t k = 0; k < i + 2u; k++)
                    dst[k] = o[k];
                s->order[d][ch] = GT2_SPAN(off4, i + 2u);
            }
        }
    }
    /* Instruments. */
    uint8_t instruments = 0;
    if (cur_byte(&c, &instruments) != 0)
        goto short_file;
    if (instruments > GT2_MAX_INSTR) {
        msg_set(info->msg, "SNG: BAD INSTRUMENTS");
        return -1;
    }
    for (uint32_t i = 0; i < instruments; i++) {
        uint8_t in[GT2_INSTR_BYTES];
        if (cur_bytes(&c, in, GT2_INSTR_BYTES) != 0)
            goto short_file;
        if (in[GT2_INS_AD] || in[GT2_INS_SR] || in[GT2_INS_WAVE] || in[GT2_INS_PULSE] ||
            in[GT2_INS_FILTER] || in[GT2_INS_VIBDELAY] || in[GT2_INS_VIBPARAM])
            top_instr = i + 1u;
        if (s) {
            uint8_t* to = s->instr[i];
            for (uint32_t k = 0; k < GT2_INSTR_BYTES; k++)
                to[k] = in[k];
            if (version == 2u) {
                /* GTS2: the vibrato delay, then an old vibrato parameter. */
                to[GT2_INS_VIBDELAY] = in[5];
                to[GT2_INS_VIBPARAM] = (uint8_t)(makespeedtable(s, in[6], MST_FINEVIB) + 1);
            }
        }
    }
    /* Tables: three in GTS2, four after. */
    uint32_t tables = version == 2u ? 3u : GT2_TABLES;
    for (uint32_t t = 0; t < tables; t++) {
        uint8_t rows = 0, l = 0, r = 0;
        if (cur_byte(&c, &rows) != 0)
            goto short_file;
        if (rows > 0u) {
            /* Each side read up to its last byte, then that byte on its own. */
            if (cur_bytes(&c, s ? s->ltable[t] : NULL, rows - 1u) != 0 || cur_byte(&c, &l) != 0 ||
                cur_bytes(&c, s ? s->rtable[t] : NULL, rows - 1u) != 0 || cur_byte(&c, &r) != 0)
                goto short_file;
            if (s) {
                s->ltable[t][rows - 1u] = l;
                s->rtable[t][rows - 1u] = r;
            }
            if (!(l | r))
                canon = 0;
        }
    }
    /* Patterns. */
    uint8_t patterns = 0;
    if (cur_byte(&c, &patterns) != 0)
        goto short_file;
    if (patterns > GT2_MAX_PATTERNS) {
        msg_set(info->msg, "SNG: BAD PATTERNS");
        return -1;
    }
    for (uint32_t p = 0; p < patterns; p++) {
        uint8_t m = 0;
        if (cur_byte(&c, &m) != 0)
            goto short_file;
        uint32_t end = m, ok = m >= 1u && m <= GT2_MAX_ROWS + 1u, deflt = 1;
        uint8_t* dst = NULL;
        if (ok && s) {
            /* The rows up to the end row; a file whose second pass disagrees is a read
             * error, found when the end row is not where the first pass saw it. */
            uint32_t off4 = 0;
            if (gt2_arena_alloc(s, m * GT2_ROW_BYTES, &off4) != 0)
                goto read_error;
            dst = (uint8_t*)s + s->arena_offset + off4 * GT2_UNIT;
            s->pattern[p] = GT2_SPAN(off4, m);
        }
        for (uint32_t r = 0; r < m; r++) {
            uint8_t row[4];
            if (cur_bytes(&c, row, 4u) != 0)
                goto short_file;
            if (s && version == 2u)
                gts2_row(s, row);
            if (r < end) {
                if (row[0] == GT2_ENDPATT) {
                    end = r;
                    deflt &= row[1] == 0u && row[2] == 0u && row[3] == 0u;
                } else if (row[0] < GT2_FIRSTNOTE || row[0] > GT2_KEYON ||
                           row[1] > GT2_MAX_INSTR || row[2] > 0x0fu) {
                    ok = 0;
                } else {
                    int rest = row[0] == GT2_REST && row[1] == 0u && row[2] == 0u && row[3] == 0u;
                    deflt &= rest;
                    if (!rest)
                        top_pattern = p;
                    if (row[1] > top_instr)
                        top_instr = row[1];
                }
                if (dst && ok)
                    for (uint32_t k = 0; k < 4u; k++)
                        dst[r * GT2_ROW_BYTES + k] = row[k];
            }
        }
        if (!ok || end >= m) {
            msg_set(info->msg, "SNG: BAD PATTERN ");
            msg_num(info->msg, p);
            return -1;
        }
        if (deflt && end == GT2_DEFAULT_ROWS && !named[p]) {
            /* An empty 64-row pattern no orderlist names is what the writer writes for a
             * pattern not held: not held, it takes no arena. */
            if (s) {
                gt2_arena_release(s, GT2_SPAN_OFF4(s->pattern[p]), m * GT2_ROW_BYTES);
                s->pattern[p] = 0u;
            }
            continue;
        }
        need += (end + 1u) * GT2_ROW_BYTES;
        if (s && end + 1u < m) {
            /* Give back the rows after the end row. */
            gt2_span sp = s->pattern[p];
            gt2_arena_release(s, GT2_SPAN_OFF4(sp) + end + 1u, (m - end - 1u) * GT2_ROW_BYTES);
            s->pattern[p] = GT2_SPAN(GT2_SPAN_OFF4(sp), end + 1u);
        }
    }
    /* Patterns the orderlists name that the file does not hold: empty, 64 rows. */
    for (uint32_t p = patterns; p < GT2_MAX_PATTERNS; p++) {
        if (!named[p])
            continue;
        need += (GT2_DEFAULT_ROWS + 1u) * GT2_ROW_BYTES;
        if (s) {
            uint32_t off4 = 0;
            if (gt2_arena_alloc(s, (GT2_DEFAULT_ROWS + 1u) * GT2_ROW_BYTES, &off4) != 0)
                goto read_error;
            uint8_t* dst = (uint8_t*)s + s->arena_offset + off4 * GT2_UNIT;
            for (uint32_t r = 0; r < GT2_DEFAULT_ROWS; r++) {
                dst[r * 4u] = GT2_REST;
                dst[r * 4u + 1u] = dst[r * 4u + 2u] = dst[r * 4u + 3u] = 0u;
            }
            dst[GT2_DEFAULT_ROWS * 4u] = GT2_ENDPATT;
            dst[GT2_DEFAULT_ROWS * 4u + 1u] = dst[GT2_DEFAULT_ROWS * 4u + 2u] =
                dst[GT2_DEFAULT_ROWS * 4u + 3u] = 0u;
            s->pattern[p] = GT2_SPAN(off4, GT2_DEFAULT_ROWS + 1u);
        }
    }
    if (s)
        convert_old(s, version);
    if (top_named > top_pattern)
        top_pattern = top_named;
    if (canonical)
        *canonical = canon && instruments == top_instr && patterns == top_pattern + 1u;
    info->gt2_bytes = c.pos;
    info->need_bytes = need;
    return 0;
short_file:
    msg_set(info->msg, s ? "SNG: READ ERROR" : "SNG: SHORT FILE");
    return -1;
read_error:
    msg_set(info->msg, "SNG: READ ERROR");
    return -1;
}

/* One reading of the file with a channel count: the GT2 part checked, then the trailer. */
typedef struct {
    gt2_read_info info;
    uint8_t named[GT2_MAX_PATTERNS];
    uint32_t keep, length;
    gt2_options opts;
    int has_opts;
    int ok;        /* the GT2 part parsed */
    int canonical; /* and its counts are savesong's */
    uint8_t trailer_channels;
    int t;  /* GT2_TRAILER_* after it */
} attempt;

static void attempt_run(attempt* a, gt2_read_fn read, void* ctx, uint32_t channels) {
    memset(a, 0, sizeof *a);
    a->info.channels = (uint8_t)channels;
    a->ok = gt2_part(read, ctx, NULL, &a->info, a->named, &a->canonical) == 0;
    a->t = a->ok ? trailer(read, ctx, a->info.gt2_bytes, NULL, &a->keep, &a->length, &a->opts,
                           &a->has_opts, &a->trailer_channels)
                 : GT2_TRAILER_NONE;
}

/* How well the reading's GT2 part ends where a song file ends, best first: at a trailer
 * whose CRCs hold (and better if it records this reading's channel count), at one whose own
 * CRC holds (the GT2 part's failed; the same), at a trailer's magic (the trailer damaged),
 * at the file's last byte; 0 anywhere else or if the part did not parse. */
static int attempt_rank(const attempt* a, gt2_read_fn read, void* ctx) {
    uint8_t b = 0;
    int match = a->trailer_channels == a->info.channels;
    if (!a->ok)
        return 0;
    switch (a->t) {
    case GT2_TRAILER_READ:
        return 8 + match;
    case GT2_TRAILER_LOCKS_DROPPED:
        return 6 + match;
    case GT2_TRAILER_DROPPED:
        return 4;
    default:
        return read(ctx, a->info.gt2_bytes, &b, 1u) == 0u ? 2 : 0;
    }
}

int gt2_sng_read(gt2_song* s, gt2_read_fn read, void* ctx, const gt2_caps* caps,
                 gt2_read_info* info) {
    attempt a, other;
    memset(info, 0, sizeof *info);
    /* GoatTracker Stereo's check is a guess ("not certain", 0007): a GT2 part can pass it,
     * or fail it, by chance, and a mono part's six-channel misreading, shifted by a few
     * bytes, can even fall back into step and end where the right reading ends. So the file
     * is also read with the other channel count, and the reading that ends better wins
     * (attempt_rank): at a SIDOU trailer that records its channel count, so SIDOU always
     * reads its own files right; then at any trailer, then at the file's end, so a mono file
     * whose instrument bytes pass the trial reads but whose six-channel reading stops short
     * reads as mono, as GoatTracker 2.77 opens it. When both end equally well, the one whose
     * counts are savesong's wins (gt2_part's canonical): a file GoatTracker or SIDOU saved
     * has them, a misreading almost never. A plain file both readings take as canonical is
     * the save of a mono song and of a stereo one alike; the guess stands. */
    uint32_t guess = gt2_sng_channels(read, ctx);
    attempt_run(&a, read, ctx, guess);
    int ra = attempt_rank(&a, read, ctx);
    if (ra < 9) {
        attempt_run(&other, read, ctx,
                    guess == GT2_MAX_CHANNELS ? GT2_MONO_CHANNELS : GT2_MAX_CHANNELS);
        int ro = attempt_rank(&other, read, ctx);
        if (ro > ra || (ro == ra && ro > 0 && other.canonical && !a.canonical))
            a = other;
    }
    *info = a.info;
    uint8_t* named = a.named;
    uint32_t keep = a.keep, length = a.length;
    gt2_options opts = a.opts;
    int has_opts = a.has_opts, ok = a.ok, t = a.t;
    if (!ok)
        return -1;
    if (info->need_bytes > gt2_arena_capacity(s)) {
        msg_set(info->msg, "SNG: TOO BIG (");
        msg_num(info->msg, (info->need_bytes + 1023u) / 1024u);
        msg_add(info->msg, "K OF ");
        msg_num(info->msg, gt2_arena_capacity(s) / 1024u);
        msg_add(info->msg, "K)");
        return -1;
    }
    uint32_t e = info->gt2_bytes;
    uint32_t ms = has_opts ? opts.multispeed : 1u;
    if (caps && caps->max_multispeed && (ms == 0u || ms > caps->max_multispeed)) {
        msg_set(info->msg, "SNG: MULTISPEED ");
        msg_num(info->msg, ms);
        msg_add(info->msg, " (MAX ");
        msg_num(info->msg, caps->max_multispeed);
        msg_add(info->msg, ")");
        return -1;
    }
    /* Pass 2: load. */
    uint8_t version = info->version, channels = info->channels;
    uint32_t need = info->need_bytes;
    gt2_song_wipe(s);
    memset(named, 0, GT2_MAX_PATTERNS);
    if (gt2_part(read, ctx, s, info, named, NULL) != 0 || info->gt2_bytes != e ||
        info->need_bytes != need || info->version != version || info->channels != channels) {
        msg_set(info->msg, "SNG: READ ERROR");
        return -1;
    }
    if (t == GT2_TRAILER_READ || t == GT2_TRAILER_LOCKS_DROPPED) {
        uint32_t keep2 = 0, length2 = 0;
        uint8_t ch2 = 0;
        int t2 = trailer(read, ctx, e, s, &keep2, &length2, &opts, &has_opts, &ch2);
        if (t2 != t || keep2 != keep) {
            s->ext_used = 0;
            msg_set(info->msg, "SNG: READ ERROR");
            return -1;
        }
        s->ext_used = keep;
        info->file_bytes = e + length;
    } else {
        info->file_bytes = e;
    }
    info->trailer = (uint8_t)t;
    if (t == GT2_TRAILER_DROPPED)
        msg_set(info->msg, "SNG: EXTRAS DROPPED");
    else if (t == GT2_TRAILER_LOCKS_DROPPED)
        msg_set(info->msg, "SNG: LOCKS DROPPED");
    return 0;
}

/* The writer. */

typedef struct {
    gt2_write_fn write;
    void* ctx;
    uint32_t crc;
    int err;
} out;

static void put(out* o, const uint8_t* p, uint32_t n) {
    if (o->err || n == 0u)
        return;
    o->crc = song_crc32(o->crc, p, n);
    o->err = o->write(o->ctx, p, n);
}

static void put8(out* o, uint32_t v) {
    uint8_t b = (uint8_t)v;
    put(o, &b, 1u);
}

static void put32(out* o, uint32_t v) {
    uint8_t b[4] = {(uint8_t)v, (uint8_t)(v >> 8), (uint8_t)(v >> 16), (uint8_t)(v >> 24)};
    put(o, b, 4u);
}

int gt2_sng_write(const gt2_song* s, int with_trailer, gt2_write_fn write, void* ctx) {
    out o = {write, ctx, 0u, 0};
    put(&o, (const uint8_t*)"GTS5", 4u);
    put(&o, (const uint8_t*)s->name, GT2_STR);
    put(&o, (const uint8_t*)s->author, GT2_STR);
    put(&o, (const uint8_t*)s->copyright, GT2_STR);
    uint32_t subtunes = gt2_saved_subtunes(s);
    put8(&o, subtunes);
    for (uint32_t d = 0; d < subtunes; d++) {
        for (uint32_t ch = 0; ch < s->channels; ch++) {
            const uint8_t* od = gt2_order_data(s, d, ch);
            if (od == NULL) {
                static const uint8_t empty[3] = {1u, GT2_LOOPSONG, 0u};
                put(&o, empty, 3u);
                continue;
            }
            uint32_t n = gt2_order_entries(s, d, ch);
            put8(&o, n + 1u);
            put(&o, od, n + 2u);
        }
    }
    uint32_t instruments = gt2_saved_instruments(s);
    put8(&o, instruments);
    for (uint32_t i = 0; i < instruments; i++)
        put(&o, s->instr[i], GT2_INSTR_BYTES);
    for (uint32_t t = 0; t < GT2_TABLES; t++) {
        uint32_t rows = gt2_saved_table_rows(s, t);
        put8(&o, rows);
        put(&o, s->ltable[t], rows);
        put(&o, s->rtable[t], rows);
    }
    uint32_t patterns = gt2_saved_patterns(s);
    put8(&o, patterns);
    for (uint32_t p = 0; p < patterns; p++) {
        const uint8_t* pd = gt2_pattern_data(s, p);
        if (pd) {
            uint32_t rows = GT2_SPAN_LEN(s->pattern[p]);
            put8(&o, rows);
            put(&o, pd, rows * GT2_ROW_BYTES);
        } else {
            static const uint8_t rest[4] = {GT2_REST, 0u, 0u, 0u};
            static const uint8_t end[4] = {GT2_ENDPATT, 0u, 0u, 0u};
            put8(&o, GT2_DEFAULT_ROWS + 1u);
            for (uint32_t r = 0; r < GT2_DEFAULT_ROWS; r++)
                put(&o, rest, 4u);
            put(&o, end, 4u);
        }
    }
    if (with_trailer) {
        uint32_t gt2_crc = o.crc;
        uint32_t len = SONG_TRAILER_MIN + s->ext_used, pad = 0;
        int padded = 0;
        if (has_ff(len)) {
            padded = 1;
            while (has_ff(len + SONG_CHUNK_HEADER + 1u + pad))
                pad++;
            len += SONG_CHUNK_HEADER + 1u + pad;
        }
        o.crc = 0;
        put(&o, (const uint8_t*)SONG_TRAILER_MAGIC, 8u);
        put8(&o, SONG_TRAILER_VERSION);
        put8(&o, 0u);
        put32(&o, len);
        static const uint8_t zeros[16] = {0};
        put8(&o, s->channels);
        for (uint32_t i = 1; i < SONG_TRAILER_LEAD_IN;) {
            uint32_t n = SONG_TRAILER_LEAD_IN - i < 16u ? SONG_TRAILER_LEAD_IN - i : 16u;
            put(&o, zeros, n);
            i += n;
        }
        put32(&o, gt2_crc);
        put(&o, s->ext, s->ext_used);
        if (padded) {
            put32(&o, SONG_TAG_PAD);
            put32(&o, 1u + pad);
            put8(&o, 1u);
            for (uint32_t i = 0; i < pad;) {
                uint32_t n = pad - i < 16u ? pad - i : 16u;
                put(&o, zeros, n);
                i += n;
            }
        }
        put32(&o, o.crc);
    }
    return o.err ? -1 : 0;
}
