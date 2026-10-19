// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "gt2_ins.h"

#include <stddef.h>
#include <string.h>

/* Commands use the same operands in patterns and in wavetable $F0..$FE. */
static int command_table(uint32_t command) {
    if ((command >= 1u && command <= 4u) || command == 14u)
        return GT2_STBL;
    if (command >= 8u && command <= 10u)
        return (int)(command - 8u);
    return -1;
}

static void protect(const gt2_song* s, uint8_t high[4], uint32_t table, uint32_t ptr) {
    if (high[table] == GT2_TABLE_ROWS)
        return;
    for (uint32_t steps = 0u; ptr != 0u && ptr <= GT2_TABLE_ROWS && steps < GT2_TABLE_ROWS; steps++) {
        if (ptr > high[table])
            high[table] = (uint8_t)ptr;
        if (table == GT2_STBL)
            break;
        ptr = s->ltable[table][ptr - 1u] == 0xffu ? s->rtable[table][ptr - 1u] : ptr + 1u;
    }
}

static void protect_command(const gt2_song* s, uint8_t high[4], uint32_t command, uint32_t ptr) {
    int table = command_table(command);
    if (table >= 0)
        protect(s, high, (uint32_t)table, ptr);
}

/* Expand a contiguous snapshot through a referenced sequence, without a bitmap.
 * The table's deterministic next-row graph needs at most 255 steps per root. */
static int extend(gt2_ins_export* out, uint32_t table, uint32_t ptr) {
    uint32_t start = out->start[table];
    if (ptr == 0u)
        return 0;
    if (start == 0u || ptr < start)
        return -1;
    if (table != GT2_STBL && out->song->ltable[table][ptr - 1u] == 0xffu)
        return -1;
    for (uint32_t steps = 0u; ptr != 0u && steps < GT2_TABLE_ROWS; steps++) {
        if (ptr < start || ptr > GT2_TABLE_ROWS)
            return -1;
        if (ptr - start < out->rows[table])
            return 0; /* already covered; the outer scan checks newly included command rows */
        uint32_t rows = ptr - start + 1u;
        if (rows > out->rows[table])
            out->rows[table] = (uint8_t)rows;
        if (table == GT2_STBL)
            return 0;
        uint8_t left = out->song->ltable[table][ptr - 1u];
        if (left == 0xffu) {
            ptr = out->song->rtable[table][ptr - 1u];
            if (ptr != 0u && out->song->ltable[table][ptr - 1u] == 0xffu)
                return -1;
        } else {
            ptr++;
        }
    }
    return ptr <= GT2_TABLE_ROWS ? 0 : -1;
}

gt2_err gt2_ins_export_begin(gt2_ins_export* out, const gt2_song* s, uint32_t instrument) {
    if (out == NULL || s == NULL || instrument == 0u || instrument > GT2_MAX_INSTR)
        return GT2_ERR_RANGE;
    memset(out, 0, sizeof *out);
    if (s->pending || s->pending_loose)
        return GT2_ERR_PENDING;
    out->song = s;
    out->instrument = (uint8_t)instrument;
    for (uint32_t t = 0u; t < GT2_TABLES; t++) {
        out->start[t] = s->instr[instrument - 1u][GT2_INS_WAVE + t];
        if (extend(out, t, out->start[t]) != 0)
            return GT2_ERR_RANGE;
    }
    /* Expansion can include rows between two referenced sequences. Check those too. */
    for (uint32_t t = 0u; t < GT2_TABLES; t++) {
        for (uint32_t r = 0u; r < out->rows[t]; r++) {
            uint32_t at = out->start[t] - 1u + r;
            uint8_t left = s->ltable[t][at], right = s->rtable[t][at];
            if (t != GT2_STBL && left == 0xffu) {
                if (extend(out, t, right) != 0)
                    return GT2_ERR_RANGE;
            } else if (t == GT2_WTBL && left >= 0xf0u && left < 0xffu) {
                int target = command_table(left - 0xf0u);
                if (target >= 0 && extend(out, (uint32_t)target, right) != 0)
                    return GT2_ERR_RANGE;
            }
        }
    }
    out->length = 4u + GT2_INSTR_BYTES + GT2_TABLES;
    for (uint32_t t = 0u; t < GT2_TABLES; t++)
        out->length += 2u * out->rows[t];
    return GT2_OK;
}

uint32_t gt2_ins_export_read(void* ctx, uint32_t offset, uint8_t* buf, uint32_t length) {
    const gt2_ins_export* out = ctx;
    if (out == NULL || out->song == NULL || buf == NULL || offset >= out->length)
        return 0u;
    if (length > out->length - offset)
        length = out->length - offset;
    for (uint32_t i = 0u; i < length; i++) {
        uint32_t at = offset + i;
        if (at < 4u) {
            buf[i] = (uint8_t)"GTI5"[at];
        } else if (at < 4u + GT2_INSTR_BYTES) {
            buf[i] = out->song->instr[out->instrument - 1u][at - 4u];
        } else {
            at -= 4u + GT2_INSTR_BYTES;
            for (uint32_t t = 0u; t < GT2_TABLES; t++) {
                uint32_t rows = out->rows[t];
                if (at == 0u) {
                    buf[i] = (uint8_t)rows;
                    break;
                }
                if (at <= rows * 2u) {
                    at--;
                    const uint8_t* side = at < rows ? out->song->ltable[t] : out->song->rtable[t];
                    buf[i] = side[out->start[t] - 1u + at % rows];
                    break;
                }
                at -= 1u + rows * 2u;
            }
        }
    }
    return length;
}

_Static_assert(sizeof(gt2_ins_import_state) <= 150u, "small instrument import state");
_Static_assert(sizeof(gt2_ins_export) <= 150u, "small instrument export state");

static int input_byte(const gt2_ins_import_state* in, uint32_t at, uint8_t* byte) {
    return in->read(in->ctx, at, byte, 1u) == 1u ? 0 : -1;
}

static int translated(const gt2_ins_import_state* in, uint32_t table, uint32_t ptr) {
    uint32_t start = in->instr[GT2_INS_WAVE + table];
    if (ptr == 0u)
        return 0;
    if (ptr < start || start == 0u || ptr - start >= in->rows[table])
        return -1;
    return (int)(ptr - start + in->dest[table] + 1u);
}

static int input_row(const gt2_ins_import_state* in, uint32_t table, uint32_t row, uint8_t pair[2]) {
    if (input_byte(in, in->offset[table] + row, pair) != 0 ||
        input_byte(in, in->offset[table] + in->rows[table] + row, pair + 1u) != 0)
        return -1;
    int target = -1;
    if (table != GT2_STBL && pair[0] == 0xffu)
        target = (int)table;
    else if (table == GT2_WTBL && pair[0] >= 0xf0u && pair[0] < 0xffu)
        target = command_table(pair[0] - 0xf0u);
    if (target >= 0) {
        int ptr = translated(in, (uint32_t)target, pair[1]);
        if (ptr < 0)
            return -1;
        if (ptr != 0 && target != GT2_STBL) {
            uint8_t left;
            uint32_t index = pair[1] - in->instr[GT2_INS_WAVE + (uint32_t)target];
            if (input_byte(in, in->offset[target] + index, &left) != 0 || left == 0xffu)
                return -1;
        }
        pair[1] = (uint8_t)ptr;
    }
    return 0;
}

enum { INS_ROOTS, INS_TABLE_ROOTS, INS_PATTERNS, INS_PREFLIGHT, INS_APPLY, INS_DONE, INS_FAILED };

static gt2_err import_error(gt2_ins_import_state* in, gt2_err error) {
    in->error = error;
    in->phase = INS_FAILED;
    return error;
}

gt2_err gt2_ins_import_begin(gt2_ins_import_state* in, gt2_song* s, uint32_t instrument,
                             gt2_read_fn read, void* ctx, uint32_t length, int owned) {
    if (in == NULL)
        return GT2_ERR_RANGE;
    memset(in, 0, sizeof *in);
    if (s == NULL || read == NULL || instrument == 0u || instrument > GT2_MAX_INSTR ||
        length < 33u || length > 33u + GT2_TABLES * GT2_TABLE_ROWS * 2u)
        return import_error(in, GT2_ERR_RANGE);
    if (s->frozen && !owned)
        return import_error(in, GT2_ERR_SAVING);
    if (s->pending || s->pending_loose)
        return import_error(in, GT2_ERR_PENDING);
    in->song = s;
    in->read = read;
    in->ctx = ctx;
    in->instrument = (uint8_t)instrument;
    uint8_t magic[4];
    if (read(ctx, 0u, magic, 4u) != 4u || memcmp(magic, "GTI5", 4u) != 0 ||
        read(ctx, 4u, in->instr, GT2_INSTR_BYTES) != GT2_INSTR_BYTES)
        return import_error(in, GT2_ERR_RANGE);
    uint32_t at = 4u + GT2_INSTR_BYTES;
    for (uint32_t t = 0u; t < GT2_TABLES; t++) {
        if (at >= length || input_byte(in, at++, in->rows + t) != 0 ||
            in->rows[t] * 2u > length - at)
            return import_error(in, GT2_ERR_RANGE);
        uint32_t start = in->instr[GT2_INS_WAVE + t];
        if ((in->rows[t] == 0u) != (start == 0u) ||
            (in->rows[t] != 0u && start + in->rows[t] - 1u > GT2_TABLE_ROWS))
            return import_error(in, GT2_ERR_RANGE);
        in->offset[t] = (uint16_t)at;
        in->dest[t] = (uint8_t)gt2_saved_table_rows(s, t);
        at += in->rows[t] * 2u;
    }
    if (at != length)
        return import_error(in, GT2_ERR_RANGE);
    uint32_t count = gt2_saved_instruments(s);
    uint32_t next_count = GT2_MAX_INSTR;
    for (; next_count > 0u; next_count--) {
        const uint8_t* def = next_count == instrument ? in->instr : s->instr[next_count - 1u];
        uint8_t defined = 0u;
        for (uint32_t field = 0u; field <= GT2_INS_VIBDELAY; field++)
            defined |= def[field];
        if (defined || (uint16_t)(s->saved_instr_base[next_count] +
                                  s->saved_instr_delta[next_count]) != 0u)
            break;
    }
    in->saved = gt2_saved_size(s) - count * GT2_INSTR_BYTES + next_count * GT2_INSTR_BYTES;
    return GT2_OK;
}

int gt2_ins_import_step(gt2_ins_import_state* in) {
    if (in == NULL || in->phase == INS_FAILED || in->song == NULL)
        return -1;
    gt2_song* s = in->song;
    if (in->phase == INS_DONE)
        return 1;
    for (uint32_t work = 0u; work < 8u; work++) {
        if (in->phase == INS_ROOTS) {
            uint32_t t = in->index % GT2_TABLES, i = in->index / GT2_TABLES;
            protect(s, in->dest, t, s->instr[i][GT2_INS_WAVE + t]);
            if (++in->index == GT2_MAX_INSTR * GT2_TABLES) {
                in->index = 0u;
                in->phase = INS_TABLE_ROOTS;
                return 0;
            }
        } else if (in->phase == INS_TABLE_ROOTS) {
            uint32_t t = in->index / GT2_TABLE_ROWS, r = in->index % GT2_TABLE_ROWS;
            uint8_t left = s->ltable[t][r], right = s->rtable[t][r];
            /* Retain references even in currently unreachable table data. */
            if (t != GT2_STBL && left == 0xffu)
                protect(s, in->dest, t, right);
            else if (t == GT2_WTBL && left >= 0xf0u && left < 0xffu)
                protect_command(s, in->dest, left - 0xf0u, right);
            if (++in->index == GT2_TABLES * GT2_TABLE_ROWS) {
                in->phase = INS_PATTERNS;
                return 0;
            }
        } else if (in->phase == INS_PATTERNS) {
            if (in->pattern == GT2_MAX_PATTERNS) {
                in->phase = INS_PREFLIGHT;
                in->end = (uint16_t)gt2_saved_table_rows(s, 0u);
                return 0;
            }
            const uint8_t* data = gt2_pattern_data(s, in->pattern);
            if (data == NULL || in->row + 1u >= gt2_pattern_rows(s, in->pattern)) {
                in->pattern++;
                in->row = 0u;
            } else {
                protect_command(s, in->dest, data[in->row * 4u + 2u], data[in->row * 4u + 3u]);
                in->row++;
            }
        } else {
            uint32_t t = in->table;
            if (t == GT2_TABLES) {
                if (in->phase == INS_PREFLIGHT) {
                    if (s->region_bytes <= SONG_REGION_BYTES && in->saved > GT2_SAVED_MAX) {
                        import_error(in, GT2_ERR_SONG_FULL);
                        return -1;
                    }
                    in->phase = INS_APPLY;
                    in->table = 0u;
                    return 0;
                }
                /* Keep every original base intact until all table operands are replayed. */
                for (uint32_t table = 0u; table < GT2_TABLES; table++)
                    in->instr[GT2_INS_WAVE + table] = in->rows[table] ? (uint8_t)(in->dest[table] + 1u) : 0u;
                memcpy(s->instr[in->instrument - 1u], in->instr, GT2_INSTR_BYTES);
                uint32_t bit = 1u << (in->instrument & 31u);
                uint8_t defined = 0u;
                for (uint32_t field = 0u; field <= GT2_INS_VIBDELAY; field++)
                    defined |= in->instr[field];
                if (defined)
                    s->saved_instr_defs[in->instrument >> 5] |= bit;
                else
                    s->saved_instr_defs[in->instrument >> 5] &= ~bit;
                s->saved_bytes = in->saved;
                gt2_song_changed(s);
                in->phase = INS_DONE;
                return 1;
            }
            if ((uint32_t)in->dest[t] + in->rows[t] > GT2_TABLE_ROWS) {
                import_error(in, GT2_ERR_FULL);
                return -1;
            }
            if (in->row == in->rows[t]) {
                in->table++;
                in->row = 0u;
                if (in->table < GT2_TABLES)
                    in->end = (uint16_t)gt2_saved_table_rows(s, in->table);
                continue;
            }
            uint8_t pair[2];
            if (input_row(in, t, in->row, pair) != 0) {
                import_error(in, GT2_ERR_RANGE);
                return -1;
            }
            uint32_t dest = in->dest[t] + in->row;
            if (in->phase == INS_PREFLIGHT) {
                if (t != GT2_STBL && ((in->row == 0u && pair[0] == 0xffu) ||
                    (in->row + 1u == in->rows[t] && pair[0] != 0xffu))) {
                    import_error(in, GT2_ERR_RANGE);
                    return -1;
                }
                if (pair[0] || pair[1]) {
                    in->saved += 2u * (dest + 1u - in->end);
                    in->end = (uint16_t)(dest + 1u);
                }
            } else {
                s->ltable[t][dest] = pair[0];
                s->rtable[t][dest] = pair[1];
                if (pair[0] || pair[1])
                    s->saved_table_bits[t][dest >> 5] |= 1u << (dest & 31u);
            }
            in->row++;
        }
    }
    return 0;
}

static gt2_err import(gt2_song* s, uint32_t instrument, gt2_read_fn read,
                       void* ctx, uint32_t length, int owned) {
    gt2_ins_import_state in;
    gt2_err error = gt2_ins_import_begin(&in, s, instrument, read, ctx, length, owned);
    if (error != GT2_OK)
        return error;
    while (gt2_ins_import_step(&in) == 0) {}
    return in.error;
}

gt2_err gt2_ins_import(gt2_song* s, uint32_t instrument, gt2_read_fn read,
                       void* ctx, uint32_t length) {
    return import(s, instrument, read, ctx, length, 0);
}

gt2_err gt2_ins_import_owned(gt2_song* s, uint32_t instrument, gt2_read_fn read,
                             void* ctx, uint32_t length) {
    return import(s, instrument, read, ctx, length, 1);
}
