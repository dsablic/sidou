// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The packed player's shape (gt2_shape.h). It transliterates the parts of GoatTracker
 * 2.77's src/greloc.c relocator() and src/gtable.c exectable() that decide the packed
 * player's shape, and its pattern, instrument and orderlist packing where the player can
 * read it. greloc.c and gtable.c are GPL-2.0-or-later, whose "any later version" lets this
 * file stay GPL-3.0-or-later. In flash on the device: the main loop's. */
#include "gt2_shape.h"

#include <stddef.h>

#define NOTES 96u
#define NOTE 0x60u
#define REST 0xbdu
#define REPEAT 0xd0u
#define TRANSDOWN 0xe0u
#define TRANS 0xf0u
#define LOOP 0xffu

/* The instrument classes the packer sorts by (greloc.c: normal, no-HR, legato). */
enum { CLASS_HR, CLASS_NOHR, CLASS_LEGATO };

/* What the derivation finds besides the base (gt2_analysis.d_seen). */
enum {
    DSEEN_CALCSPEED = 1u << 0, /* a note-independent speed (left byte $80 and up) in use */
    DSEEN_FILTER = 1u << 1,    /* an instrument or wavetable command starts the filter */
    DSEEN_INSVIB = 1u << 2,    /* an instrument's vibrato */
    DSEEN_NOTFIXED = 1u << 3,  /* an instrument's gate timer or first wave differs */
    DSEEN_WAVEDELAY = 1u << 4,
    DSEEN_WAVECMD = 1u << 5,
    DSEEN_PULSENYB = 1u << 6,  /* a pulse row with a low nybble */
    DSEEN_REL20 = 1u << 7      /* a relative wave note of $20 and up */
};

enum { PH_ORDERS, PH_PATTERNS, PH_DERIVE, PH_DONE };
enum { D_INSTR, D_WAVECMD, D_WTABLE, D_PTABLE, D_FINAL, D_STREAMS, D_END };

#define BIT(b, i) (((b)[(i) >> 5] >> ((i) & 31u)) & 1u)
#define SET(b, i) ((b)[(i) >> 5] |= 1u << ((i) & 31u))
/* What the base gains, noted (gt2_analysis.grew): a widening says whether anything did. */
#define GROW_SET(a, b, i)                                                                     \
    do {                                                                                     \
        if (!BIT(b, i)) {                                                                    \
            SET(b, i);                                                                       \
            (a)->grew = 1u;                                                                  \
        }                                                                                    \
    } while (0)
#define GROW_OR(a, x, v)                                                                      \
    do {                                                                                     \
        if (((x) | (v)) != (x)) {                                                            \
            (x) |= (v);                                                                      \
            (a)->grew = 1u;                                                                  \
        }                                                                                    \
    } while (0)

/* --- The view: the song with the pending edits on top ----------------------------------- */

static uint8_t vb(const gt2_analysis* a, uint32_t off) {
    for (uint32_t k = a->pending_count; k-- > 0u;) {
        const gt2_edit* e = &a->pending[k];
        if (e->kind == GT2_EDIT_CELL && off - e->offset < e->n)
            return e->bytes[off - e->offset];
    }
    return ((const uint8_t*)a->song)[off];
}

static gt2_span vspan(const gt2_analysis* a, uint32_t off) {
    for (uint32_t k = a->pending_count; k-- > 0u;) {
        const gt2_edit* e = &a->pending[k];
        if (e->kind == GT2_EDIT_SWAP && e->offset == off)
            return e->span;
    }
    return *(const gt2_span*)(const void*)((const uint8_t*)a->song + off);
}

#define OFF_INSTR offsetof(gt2_song, instr)
#define OFF_LTABLE offsetof(gt2_song, ltable)
#define OFF_RTABLE offsetof(gt2_song, rtable)
#define OFF_ORDER offsetof(gt2_song, order)
#define OFF_PATTERN offsetof(gt2_song, pattern)

/* A table's left or right byte at a 1-based position; 0 past the table and at 0. */
static uint8_t tl(const gt2_analysis* a, uint32_t t, uint32_t pos) {
    return pos - 1u < GT2_TABLE_ROWS ? vb(a, OFF_LTABLE + t * GT2_TABLE_ROWS + pos - 1u) : 0u;
}

static uint8_t tr(const gt2_analysis* a, uint32_t t, uint32_t pos) {
    return pos - 1u < GT2_TABLE_ROWS ? vb(a, OFF_RTABLE + t * GT2_TABLE_ROWS + pos - 1u) : 0u;
}

/* Instrument i's byte f (i 1..63). */
static uint8_t ins(const gt2_analysis* a, uint32_t i, uint32_t f) {
    return vb(a, OFF_INSTR + (i - 1u) * GT2_INSTR_BYTES + f);
}

static uint32_t arena_off(const gt2_analysis* a, gt2_span sp) {
    return a->song->arena_offset + GT2_SPAN_OFF4(sp) * GT2_UNIT;
}

/* An orderlist's entries (the $FF and restart byte follow), as a region offset. */
static uint32_t order_entries(const gt2_analysis* a, uint32_t sub, uint32_t ch, uint32_t* off) {
    if (sub >= a->song->subtunes)
        return 0u;
    gt2_span sp = vspan(a, OFF_ORDER + (sub * GT2_MAX_CHANNELS + ch) * 4u);
    if (sp == 0u || GT2_SPAN_LEN(sp) < 2u)
        return 0u;
    *off = arena_off(a, sp);
    return GT2_SPAN_LEN(sp) - 2u;
}

static int packed_subtune(const gt2_analysis* a, uint32_t sub) {
    uint32_t off;
    return order_entries(a, sub, 0u, &off) && order_entries(a, sub, 1u, &off) &&
           order_entries(a, sub, 2u, &off);
}

/* A pattern's rows (the end row not counted) at a region offset; 0 for one not held. */
static uint32_t pattern_rows(const gt2_analysis* a, uint32_t pat, uint32_t* off) {
    gt2_span sp = pat < GT2_MAX_PATTERNS ? vspan(a, OFF_PATTERN + pat * 4u) : 0u;
    if (sp == 0u)
        return 0u;
    *off = arena_off(a, sp);
    return GT2_SPAN_LEN(sp) - 1u;
}

/* --- Walking the song -------------------------------------------------------------------- */

/* gtable.c's exectable: marks the rows a table program reaches from ptr; the rows it took. */
static uint32_t reach(gt2_analysis* a, uint32_t t, uint32_t ptr) {
    uint32_t* used = a->base.tableused[t];
    if (t != GT2_STBL && ptr != 0u && ptr <= GT2_TABLE_ROWS && tl(a, t, ptr) == LOOP)
        return 0u; /* a pointer onto a jump: the packer refuses the song */
    uint32_t steps = 0u;
    for (;;) {
        if (ptr == 0u)
            break;
        if (t != GT2_STBL && ptr > GT2_TABLE_ROWS)
            break;
        if (BIT(used, ptr))
            break;
        SET(used, ptr);
        a->grew = 1u;
        steps++;
        if (t == GT2_STBL)
            break;
        ptr = tl(a, t, ptr) == LOOP ? tr(a, t, ptr) : ptr + 1u;
    }
    return steps;
}

/* One orderlist of a packed subtune (greloc.c, "Process song-orderlists"); its entries. */
static uint32_t scan_order(gt2_analysis* a, uint32_t sub, uint32_t ch, uint32_t from,
                           uint32_t to) {
    gt2_base* b = &a->base;
    uint32_t off;
    uint32_t n = order_entries(a, sub, ch, &off);
    if (to > n)
        to = n;
    for (uint32_t i = from; i < to; i++) {
        uint8_t e = vb(a, off + i);
        if (e < REPEAT) {
            GROW_SET(a, b->pattused, e);
            GROW_SET(a, b->named[ch], e);
        } else if (e < TRANSDOWN) {
            GROW_OR(a, b->seen, GT2_SEEN_REPEAT);
        } else {
            GROW_OR(a, b->seen, GT2_SEEN_TRANS);
            if (e < TRANS) {
                if (TRANS - e > b->transdown) {
                    b->transdown = (uint8_t)(TRANS - e);
                    a->grew = 1u;
                }
            } else if (e - TRANS > b->transup) {
                b->transup = (uint8_t)(e - TRANS);
                a->grew = 1u;
            }
        }
    }
    return n;
}

/* One row of a pattern in use (greloc.c, "Build the pattern-mapping"); its units. */
static uint32_t scan_row(gt2_analysis* a, uint32_t pat, const uint8_t row[4]) {
    gt2_base* b = &a->base;
    uint8_t cmd = row[2], data = row[3];
    uint32_t units = 1u;
    if (row[0] != REST || row[1] != 0u || row[2] != 0u)
        GROW_SET(a, b->filled, pat);
    if (row[1] != 0u && row[1] <= GT2_MAX_INSTR)
        GROW_SET(a, b->instrused, row[1]);
    if (cmd != 0u)
        GROW_OR(a, b->seen, GT2_SEEN_EFFECTS);
    if (cmd >= 8u && cmd <= 10u)
        units += reach(a, cmd - 8u, data);
    if ((cmd >= 1u && cmd <= 4u) || cmd == 14u)
        units += reach(a, GT2_STBL, data);
    if (cmd >= 1u && cmd <= 4u && data != 0u)
        GROW_SET(a, b->stbl_calc, data);
    if (cmd >= 10u && cmd <= 12u)
        GROW_OR(a, b->seen, GT2_SEEN_FILTER); /* packpattern */
    if (cmd >= 5u && cmd <= 13u)
        GROW_OR(a, b->tick0, 1u << cmd);
    if (cmd == 14u)
        GROW_OR(a, b->tick0, 1u << 14 | GT2_TICK0_TEMPO_GLOBAL);
    if (cmd == 15u) {
        if ((data & 0x7fu) < 3u)
            GROW_OR(a, b->tick0, 1u << 14);
        GROW_OR(a, b->tick0, (data & 0x80u) ? GT2_TICK0_TEMPO_CHANNEL : GT2_TICK0_TEMPO_GLOBAL);
    }
    if (row[0] >= NOTE && row[0] <= GT2_LASTNOTE) {
        uint8_t n = (uint8_t)(row[0] - NOTE);
        if (n < b->note_lo) {
            b->note_lo = n;
            a->grew = 1u;
        }
        if (b->note_hi == 0xffu || n > b->note_hi) {
            b->note_hi = n;
            a->grew = 1u;
        }
    }
    return units;
}

static void read_row(const gt2_analysis* a, uint32_t off, uint32_t r, uint8_t row[4]) {
    for (uint32_t k = 0u; k < 4u; k++)
        row[k] = vb(a, off + r * GT2_ROW_BYTES + k);
}

/* Every row of a pattern, at once (a widening). */
static void scan_pattern(gt2_analysis* a, uint32_t pat) {
    uint32_t off;
    uint32_t rows = pattern_rows(a, pat, &off);
    for (uint32_t r = 0u; r < rows; r++) {
        uint8_t row[4];
        read_row(a, off, r, row);
        scan_row(a, pat, row);
    }
    SET(a->base.scanned, pat);
}

/* --- The analysis ------------------------------------------------------------------------ */

int gt2_shape_begin(gt2_analysis* a, const gt2_song* s, uint32_t subtune, const gt2_options* o) {
    uint8_t* z = (uint8_t*)a;
    for (uint32_t i = 0u; i < sizeof *a; i++)
        z[i] = 0u;
    gt2_options defaults;
    if (o == 0) {
        gt2_options_default(&defaults);
        o = &defaults;
    }
    a->song = s;
    a->player_flags = o->player_flags;
    a->hr_adsr = o->hr_adsr;
    a->mult = o->multispeed;
    a->opt_pulse = o->optimize_pulse;
    a->opt_realtime = o->optimize_realtime;
    a->a4_millihz = o->a4_millihz;
    a->subtune = (uint8_t)(subtune < GT2_MAX_SUBTUNES ? subtune : 0xffu);
    a->base.note_lo = 0xffu;
    a->base.note_hi = 0xffu;
    SET(a->base.instrused, 1u);
    a->phase = PH_ORDERS;
    a->result = GT2_SHAPE_MORE;
    if (s->channels != GT2_MONO_CHANNELS) {
        a->result = GT2_PLAY_ERR_STEREO;
        a->phase = PH_DONE;
    }
    return a->result == GT2_PLAY_ERR_STEREO ? GT2_PLAY_ERR_STEREO : 0;
}

/* The packed subtune index the player plays, or -1. */
static int packed_index(const gt2_analysis* a, uint32_t* songs) {
    uint32_t n = 0u;
    for (uint32_t sub = 0u; sub < GT2_MAX_SUBTUNES; sub++)
        n += (a->base.packed >> sub) & 1u;
    *songs = n;
    if (n == 0u)
        return -1;
    /* With one song the packed init ignores the subtune; with more it is the song's index
     * among the first `songs` subtunes (greloc.c packs subtunes 0 to songs - 1). */
    uint32_t sub = n == 1u ? 0u : a->subtune;
    if (sub >= n || !((a->base.packed >> sub) & 1u))
        return -1;
    return (int)sub;
}

static int derive_step(gt2_analysis* a, gt2_shape* out, uint32_t* budget);

int gt2_shape_step(gt2_analysis* a, gt2_shape* out, uint32_t budget) {
    uint32_t units = 0u;
    while (a->phase != PH_DONE && units < budget) {
        if (a->phase == PH_ORDERS) {
            if (a->i_sub >= GT2_MAX_SUBTUNES) {
                uint32_t songs;
                if (packed_index(a, &songs) < 0) {
                    a->result = GT2_PLAY_ERR_SUBTUNE;
                    a->phase = PH_DONE;
                    break;
                }
                a->phase = PH_PATTERNS;
                continue;
            }
            if (a->i_ch == 0u && a->i_pos == 0u) {
                units++;
                if (!packed_subtune(a, a->i_sub)) {
                    a->i_sub++;
                    continue;
                }
                a->base.packed |= 1u << a->i_sub;
            }
            uint32_t room = budget > units ? budget - units : 1u; /* at least one entry */
            uint32_t to = a->i_pos + (room > GT2_MAX_ORDER + 1u ? GT2_MAX_ORDER + 1u : room);
            uint32_t n = scan_order(a, a->i_sub, a->i_ch, a->i_pos, to);
            if (to >= n) {
                units += n - a->i_pos;
                a->i_pos = 0u;
                if (++a->i_ch == GT2_PLAY_CHANNELS) {
                    a->i_ch = 0u;
                    a->i_sub++;
                }
            } else {
                units += to - a->i_pos;
                a->i_pos = (uint16_t)to;
            }
        } else if (a->phase == PH_PATTERNS) {
            if (a->i_pat >= GT2_MAX_PATTERNS) {
                a->phase = PH_DERIVE;
                a->d_phase = D_INSTR;
                a->d_i = 0u;
                continue;
            }
            uint32_t pat = a->i_pat;
            uint32_t off = 0u;
            uint32_t rows = BIT(a->base.pattused, pat) && !BIT(a->base.scanned, pat)
                                ? pattern_rows(a, pat, &off)
                                : 0u;
            while (a->i_row < rows && units < budget) {
                uint8_t row[4];
                read_row(a, off, a->i_row, row);
                units += scan_row(a, pat, row);
                a->i_row++;
            }
            if (a->i_row >= rows) {
                if (BIT(a->base.pattused, pat))
                    SET(a->base.scanned, pat);
                a->i_row = 0u;
                a->i_pat++;
                units++;
            }
        } else {
            uint32_t left = budget - units;
            int done = derive_step(a, out, &left);
            units = budget - left;
            if (done) {
                a->phase = PH_DONE;
                a->result = GT2_SHAPE_DONE;
            }
        }
    }
    return a->result;
}

int gt2_shape_analyse(gt2_analysis* a, const gt2_song* s, uint32_t subtune,
                      const gt2_options* o, gt2_shape* out) {
    int r = gt2_shape_begin(a, s, subtune, o);
    if (r < 0)
        return r;
    while ((r = gt2_shape_step(a, out, 0xffffffffu)) == GT2_SHAPE_MORE) {
    }
    return r < 0 ? r : 0;
}

/* --- The derivation ---------------------------------------------------------------------- */

static int class_of(uint8_t g) {
    return (g & 0x40u) ? CLASS_LEGATO : (g & 0x80u) ? CLASS_NOHR : CLASS_HR;
}

/* One instrument in use: its class, its table programs, its features. */
static uint32_t derive_instr(gt2_analysis* a, uint32_t i) {
    uint32_t units = 4u;
    int cls = class_of(ins(a, i, GT2_INS_GATETIMER));
    if (a->count[cls]++ == 0u)
        a->lowest[cls] = (uint8_t)i;
    for (uint32_t t = 0u; t < GT2_TABLES; t++)
        units += reach(a, t, ins(a, i, GT2_INS_WAVE + t));
    uint8_t vib = ins(a, i, GT2_INS_VIBPARAM);
    if (vib != 0u && tl(a, GT2_STBL, vib) >= 0x80u)
        a->d_seen |= DSEEN_CALCSPEED;
    if (ins(a, i, GT2_INS_FILTER) != 0u)
        a->d_seen |= DSEEN_FILTER;
    if (vib != 0u)
        a->d_seen |= DSEEN_INSVIB;
    /* FIXEDPARAMS: every instrument the gate-off timer and first wave of instrument 1 */
    uint8_t fw = ins(a, i, GT2_INS_FIRSTWAVE);
    if (ins(a, i, GT2_INS_GATETIMER) != ins(a, 1u, GT2_INS_GATETIMER) ||
        fw != ins(a, 1u, GT2_INS_FIRSTWAVE) || fw == 0u || fw >= 0xfeu)
        a->d_seen |= DSEEN_NOTFIXED;
    return units;
}

/* A wavetable row in use that starts another table (its command). */
static uint32_t derive_wavecmd(gt2_analysis* a, uint32_t r) {
    uint8_t l = tl(a, GT2_WTBL, r), d = tr(a, GT2_WTBL, r);
    if (l < 0xf0u || l > 0xfeu)
        return 1u;
    uint8_t cmd = (uint8_t)(l - 0xf0u);
    uint32_t units = 1u;
    if (cmd >= 1u && cmd <= 4u) {
        units += reach(a, GT2_STBL, d);
        if (d != 0u && tl(a, GT2_STBL, d) >= 0x80u)
            a->d_seen |= DSEEN_CALCSPEED;
    } else if (cmd == 9u) {
        units += reach(a, GT2_PTBL, d);
    } else if (cmd == 10u) {
        units += reach(a, GT2_FTBL, d);
        a->d_seen |= DSEEN_FILTER;
    }
    return units;
}

/* A wavetable row in use ("Process tables"): delays, commands, notes. */
static void derive_wbytes(gt2_analysis* a, uint8_t l, uint8_t d) {
    if (l >= 0x01u && l <= 0x0fu)
        a->d_seen |= DSEEN_WAVEDELAY;
    if (l >= 0xf0u && l <= 0xfeu) {
        a->d_seen |= DSEEN_WAVECMD;
        if (l >= 0xf5u)
            a->d_tick0 |= 1u << (l - 0xf0u);
    }
    if (l < 0xf0u) {
        if (d <= 0x80u) {
            if (!a->rel_any || d > a->rel_hi)
                a->rel_hi = d;
            a->rel_any = 1u;
            if (d >= 0x20u)
                a->d_seen |= DSEEN_REL20;
        } else {
            uint8_t n = (uint8_t)(d & 0x7fu);
            if (!a->abs_any || n < a->abs_lo)
                a->abs_lo = n;
            if (!a->abs_any || n > a->abs_hi)
                a->abs_hi = n;
            a->abs_any = 1u;
        }
    }
}

static void derive_wrow(gt2_analysis* a, uint32_t r) {
    derive_wbytes(a, tl(a, GT2_WTBL, r), tr(a, GT2_WTBL, r));
}

/* player.s's variables below the frequency table, as the assembly lays them out: blocks of
 * seven a channel, NUMCHANNELS channels a block ("Normal channel variables" or "Optimized
 * channel variables"), then the ghost registers of full buffering; the table follows. */
enum { G_GHOST, G_1, G_2, G_3, G_4, G_5, G_4B, G_1O, G_2O };

static uint8_t var_code(const gt2_shape* sh, uint32_t group, uint32_t ch, uint32_t field) {
    int ghost = (sh->flags & GT2_PF_GHOST) != 0;
    int noinsvib = (sh->layout & GT2_LAYOUT_NOINSVIB) != 0;
    int fixed = (sh->layout & GT2_LAYOUT_FIXEDPARAMS) != 0;
    uint8_t c = (uint8_t)(ch << 5);
    uint8_t z = GT2_VAR_ZERO;
    switch (group) {
    case G_3: {
        static const uint8_t f[7] = {GT2_VF_ORDIDX, GT2_VF_PATTNUM, GT2_VF_TEMPO,
                                     GT2_VF_COUNTER, GT2_VF_NOTE, GT2_VF_INSTR, GT2_VF_GATE};
        return (uint8_t)(c | f[field]);
    }
    case G_4: {
        const uint8_t v[7] = {
            (uint8_t)(c | GT2_VF_VIBTIME), noinsvib ? z : (uint8_t)(c | GT2_VF_VIBDELAY),
            (uint8_t)(c | GT2_VF_WAVETIME), ghost ? z : (uint8_t)(c | GT2_VF_FREQLO),
            ghost ? z : (uint8_t)(c | GT2_VF_FREQHI), ghost ? z : (uint8_t)(c | GT2_VF_PULSELO),
            ghost ? z : (uint8_t)(c | GT2_VF_PULSEHI)};
        return v[field];
    }
    case G_5: {
        int buffered = (sh->flags & GT2_PF_BUFFERED) && !ghost;
        const uint8_t v[7] = {
            buffered ? (uint8_t)(c | GT2_VF_AD) : z, buffered ? (uint8_t)(c | GT2_VF_SR) : z, z,
            z, z, fixed ? z : (uint8_t)(c | GT2_VF_GATETIMER),
            (sh->layout & GT2_LAYOUT_NOCALCSPEED) ? z : (uint8_t)(c | GT2_VF_LASTNOTE)};
        return v[field];
    }
    case G_4B: {
        const uint8_t v[7] = {(uint8_t)(c | GT2_VF_VIBTIME),
                              noinsvib ? z : (uint8_t)(c | GT2_VF_VIBDELAY),
                              (uint8_t)(c | GT2_VF_WAVETIME),
                              z,
                              z,
                              z,
                              fixed ? z : (uint8_t)(c | GT2_VF_GATETIMER)};
        return v[field];
    }
    case G_2O: {
        const uint8_t v[7] = {(uint8_t)(c | GT2_VF_PULSETIME), (uint8_t)(c | GT2_VF_PULSELO),
                              (uint8_t)(c | GT2_VF_PULSEHI),   (uint8_t)(c | GT2_VF_VIBTIME),
                              noinsvib ? z : (uint8_t)(c | GT2_VF_VIBDELAY),
                              (uint8_t)(c | GT2_VF_FREQLO),    (uint8_t)(c | GT2_VF_FREQHI)};
        return v[field];
    }
    default:
        return GT2_VAR_UNKNOWN;
    }
}

static void derive_var(gt2_shape* sh) {
    uint8_t order[6];
    uint32_t k = 0u;
    if ((sh->flags & GT2_PF_GHOST) && !(sh->flags & GT2_PF_ZPGHOST))
        order[k++] = G_GHOST;
    if (sh->layout & GT2_LAYOUT_NORMAL) {
        if (sh->layout & GT2_LAYOUT_G4) {
            if (sh->layout & GT2_LAYOUT_G5)
                order[k++] = G_5;
            order[k++] = G_4;
        } else {
            order[k++] = G_4B;
        }
        order[k++] = G_3;
        order[k++] = G_2;
        order[k++] = G_1;
    } else {
        order[k++] = G_3;
        order[k++] = G_2O;
        order[k++] = G_1O;
    }
    uint32_t size = 7u * sh->channels;
    uint32_t back = 0u; /* bytes described so far */
    for (uint32_t i = 0u; i < k; i++) {
        uint32_t n = order[i] == G_GHOST ? GT2_SID_REGS : size;
        for (uint32_t j = 1u; j <= n; j++) {
            uint32_t idx = n - j;
            sh->var[back + j - 1u] = order[i] == G_GHOST
                                         ? (uint8_t)(GT2_VAR_GHOST | idx)
                                         : var_code(sh, order[i], idx / 7u, idx % 7u);
        }
        back += n;
    }
    sh->var_count = (uint8_t)back;
}

/* The final numbers (greloc.c's defines), the numberings and the layout. */
static void derive_final(gt2_analysis* a, gt2_shape* sh) {
    const gt2_base* b = &a->base;
    uint32_t flags = a->player_flags;
    uint32_t mult = a->mult;
    int noopt = (flags & GT2_PLAYER_NOOPTIMIZATION) != 0;
    int sfx = (flags & GT2_PLAYER_SOUNDEFFECTS) != 0;
    int ghost = (flags & (GT2_PLAYER_ZPGHOSTREGS | GT2_PLAYER_FULLBUFFERED)) != 0;
    int buffered = ghost || sfx || (flags & GT2_PLAYER_BUFFERED);
    uint32_t songs;
    int sub = packed_index(a, &songs);

    /* the speedtable rows a 1XY-4XY command names, as they are now */
    for (uint32_t r = 1u; r <= GT2_TABLE_ROWS; r++)
        if (BIT(b->stbl_calc, r) && tl(a, GT2_STBL, r) >= 0x80u)
            a->d_seen |= DSEEN_CALCSPEED;
    uint32_t ds = a->d_seen;
    int noeffects = !(b->seen & GT2_SEEN_EFFECTS);
    int nofilter = !(b->seen & GT2_SEEN_FILTER) && !(ds & DSEEN_FILTER);
    int nocalcspeed = !(ds & DSEEN_CALCSPEED);
    int fixedparams = !(ds & DSEEN_NOTFIXED) && mult <= 1u;
    int noinsvib = !(ds & DSEEN_INSVIB);
    int notrans = !(b->seen & GT2_SEEN_TRANS), norepeat = !(b->seen & GT2_SEEN_REPEAT);
    int wavedelay = (ds & DSEEN_WAVEDELAY) != 0, wavecmd = (ds & DSEEN_WAVECMD) != 0;
    int simplepulse = !(ds & DSEEN_PULSENYB);
    uint32_t tick0 = b->tick0 | a->d_tick0;

    /* the notes: the patterns' with the transposes, then the wavetable's */
    int firstnote = (int)NOTES - 1, lastnote = 0, patternlastnote = 0;
    if (b->note_lo <= b->note_hi) {
        firstnote = (int)b->note_lo - (int)b->transdown;
        if (firstnote < 0)
            firstnote = 0;
        lastnote = (int)b->note_hi + (int)b->transup;
        if (lastnote > (int)NOTES - 1)
            lastnote = (int)NOTES - 1;
        patternlastnote = lastnote;
    }
    if (a->rel_any) {
        int nl = (int)a->rel_hi + patternlastnote;
        if (nl > (int)NOTES - 1)
            nl = (int)NOTES - 1;
        if (nl > lastnote)
            lastnote = nl;
    }
    if (ds & DSEEN_REL20)
        firstnote = 0;
    if (a->abs_any) {
        int hi = a->abs_hi > NOTES - 1u ? (int)NOTES - 1 : (int)a->abs_hi;
        if ((int)a->abs_lo < firstnote)
            firstnote = a->abs_lo;
        if (hi > lastnote)
            lastnote = hi;
    }

    uint32_t channels = 3u;
    int chnused[GT2_PLAY_CHANNELS];
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        chnused[ch] = 0;
        for (uint32_t w = 0u; w < GT2_PATTERN_WORDS; w++)
            if (b->named[ch][w] & b->filled[w])
                chnused[ch] = 1;
    }
    if (!chnused[2])
        channels = 2u;
    if (!chnused[1] && !chnused[2])
        channels = 1u;

    if (wavecmd)
        noeffects = 0;
    if (noopt) {
        notrans = norepeat = fixedparams = noinsvib = nocalcspeed = 0;
        tick0 = 0x7fe0u | GT2_TICK0_TEMPO_GLOBAL | GT2_TICK0_TEMPO_CHANNEL;
        noeffects = 0;
        nofilter = 0;
        simplepulse = 0;
        wavedelay = 1;
        wavecmd = 1;
        firstnote = 0;
        lastnote = (int)NOTES - 1;
    }
    if (sfx || ghost)
        channels = 3u;
    if (lastnote < firstnote)
        lastnote = firstnote;
    if (!nocalcspeed)
        lastnote++;
    if (lastnote > (int)NOTES - 1)
        lastnote = (int)NOTES - 1;
    if (sfx) {
        firstnote = 0;
        lastnote = (int)NOTES - 1;
    }

    sh->warnings = a->a4_millihz != 0u ? GT2_PLAY_WARN_PITCH : 0u;
    sh->subtune = (uint8_t)(sub < 0 ? 0 : sub);
    sh->songs = (uint8_t)songs;
    sh->channels = (uint8_t)channels;
    sh->tick0 = tick0;
    int normal = !noeffects || wavedelay || !notrans || !norepeat || !fixedparams || ghost ||
                 buffered || !nocalcspeed;
    int g4 = !ghost || !nocalcspeed;
    uint32_t layout = 0u;
    if (normal)
        layout |= GT2_LAYOUT_NORMAL;
    if (g4)
        layout |= GT2_LAYOUT_G4;
    if (g4 && (buffered || !fixedparams || !nocalcspeed))
        layout |= GT2_LAYOUT_G5;
    if (fixedparams)
        layout |= GT2_LAYOUT_FIXEDPARAMS;
    if (noinsvib)
        layout |= GT2_LAYOUT_NOINSVIB;
    if (nocalcspeed)
        layout |= GT2_LAYOUT_NOCALCSPEED;
    sh->layout = (uint8_t)layout;
    uint32_t f = 0u;
    if (buffered)
        f |= GT2_PF_BUFFERED;
    if (ghost)
        f |= GT2_PF_GHOST;
    if (flags & GT2_PLAYER_ZPGHOSTREGS)
        f |= GT2_PF_ZPGHOST;
    if (a->hr_adsr >= 0xf000u)
        f |= GT2_PF_ALT;
    if (a->opt_pulse)
        f |= GT2_PF_PULSEOPT;
    if (a->opt_realtime)
        f |= GT2_PF_REALTIMEOPT;
    if (flags & GT2_PLAYER_AUTHORINFO)
        f |= GT2_PF_AUTHORINFO;
    if (noeffects)
        f |= GT2_PF_NOEFFECTS;
    if (nofilter)
        f |= GT2_PF_NOFILTER;
    if (simplepulse)
        f |= GT2_PF_SIMPLEPULSE;
    if (wavedelay)
        f |= GT2_PF_WAVEDELAY;
    if (wavecmd)
        f |= GT2_PF_WAVECMD;
    sh->flags = (uint16_t)f;
    sh->ad_param = (uint8_t)(a->hr_adsr >> 8);
    sh->sr_param = (uint8_t)a->hr_adsr;
    /* The packer numbers the instruments hard restart first, then no hard restart, then
     * legato; each channel starts on its number 1. Multispeed adds one to the no-HR count
     * ("for multispeed stability"), which moves the legato class's start past its first
     * instrument: that one plays as no-HR. */
    sh->reset_instr = a->count[CLASS_HR]     ? a->lowest[CLASS_HR]
                      : a->count[CLASS_NOHR] ? a->lowest[CLASS_NOHR]
                                             : a->lowest[CLASS_LEGATO];
    sh->first_legato = mult > 1u ? a->lowest[CLASS_LEGATO] : 0u;
    /* DEFAULTTEMPO: instrument 63's AD less one if it is 2 or more and the instrument has no
     * wavetable (readme 3.6), else 6 ticks a row times the multispeed. */
    uint8_t ad63 = ins(a, GT2_MAX_INSTR, GT2_INS_AD);
    if (ad63 >= 2u && ins(a, GT2_MAX_INSTR, GT2_INS_WAVE) == 0u)
        sh->default_tempo = (uint8_t)(ad63 - 1u);
    else
        sh->default_tempo = (uint8_t)(mult ? mult * 6u - 1u : 5u);
    sh->first_note = (uint8_t)firstnote;
    sh->last_note = (uint8_t)lastnote;

    /* The packer's numberings. */
    uint32_t q = 0u;
    for (uint32_t pat = 0u; pat < GT2_MAX_PATTERNS; pat++) {
        sh->pat_num[pat] = 0xffu;
        if (BIT(b->pattused, pat))
            sh->pat_num[pat] = (uint8_t)q++;
    }
    uint32_t next[3];
    next[CLASS_HR] = 1u;
    next[CLASS_NOHR] = 1u + a->count[CLASS_HR];
    next[CLASS_LEGATO] = next[CLASS_NOHR] + a->count[CLASS_NOHR];
    sh->ins_num[0] = 0u;
    for (uint32_t i = 1u; i <= GT2_MAX_INSTR; i++) {
        sh->ins_num[i] = 0u;
        if (BIT(b->instrused, i))
            sh->ins_num[i] = (uint8_t)next[class_of(ins(a, i, GT2_INS_GATETIMER))]++;
    }
    derive_var(sh);
}

/* The packed bytes of model entry i of an orderlist: a repeat goes after its pattern. */
static uint8_t packed_entry(const gt2_analysis* a, uint32_t off, uint32_t n, uint32_t i) {
    uint8_t e = vb(a, off + i);
    int rep = e > REPEAT && e < TRANSDOWN;
    if (rep && i + 1u < n && vb(a, off + i + 1u) < REPEAT)
        return vb(a, off + i + 1u);
    if (e < REPEAT && i > 0u) {
        uint8_t prev = vb(a, off + i - 1u);
        if (prev > REPEAT && prev < TRANSDOWN)
            return prev;
    }
    return e;
}

/* Channel ch's packed orderlist stream from its start (gt2_shape.ord): its own entries,
 * $FF, the restart byte, then the orderlists the packer writes after it (the subtunes'
 * channels in order, those of packed subtunes below `songs`), then $00s. */
static void derive_stream(gt2_analysis* a, gt2_shape* sh, uint32_t ch) {
    uint8_t* ord = sh->ord[ch];
    uint8_t* pos = sh->ord_pos[ch];
    uint32_t k = 0u;
    uint32_t sub = sh->subtune, c = ch;
    int own = 1;
    while (k < GT2_ORDER_STREAM) {
        uint32_t off = 0u;
        uint32_t n = order_entries(a, sub, c, &off);
        if (n == 0u)
            break; /* none (a subtune a widening still counts as packed): as past the last */
        for (uint32_t i = 0u; i < n && k < GT2_ORDER_STREAM; i++) {
            uint8_t e = vb(a, off + i);
            if (e == REPEAT)
                continue; /* R0: the packer drops it */
            uint8_t p = 0xffu;
            if (own) {
                p = (uint8_t)i;
                if (e > REPEAT && e < TRANSDOWN && i + 1u < n && vb(a, off + i + 1u) < REPEAT)
                    p = (uint8_t)(i + 1u); /* the entry in the model: after its repeat */
            }
            pos[k] = p;
            ord[k++] = packed_entry(a, off, n, i);
        }
        if (k < GT2_ORDER_STREAM) {
            pos[k] = 0xffu;
            ord[k++] = LOOP;
        }
        if (k < GT2_ORDER_STREAM) {
            pos[k] = GT2_ORD_RAW;
            ord[k++] = vb(a, off + n + 1u);
        }
        own = 0;
        /* the next orderlist the packer wrote */
        int found = 0;
        while (!found) {
            if (++c == GT2_PLAY_CHANNELS) {
                c = 0u;
                if (++sub >= sh->songs)
                    break;
            }
            found = packed_subtune(a, sub);
        }
        if (!found)
            break;
    }
    /* past the last orderlist: the packed patterns, which the player reads as $00 */
    for (; k < GT2_ORDER_STREAM; k++) {
        pos[k] = GT2_ORD_RAW;
        ord[k] = 0u;
    }
}

/* One step of the derivation; 1 once out is whole. */
static int derive_step(gt2_analysis* a, gt2_shape* out, uint32_t* budget) {
    while (*budget > 0u) {
        uint32_t units = 1u;
        switch (a->d_phase) {
        case D_INSTR:
            if (a->d_i == 0u) {
                a->d_seen = 0u;
                a->d_tick0 = 0u;
                a->count[0] = a->count[1] = a->count[2] = 0u;
                a->lowest[0] = a->lowest[1] = a->lowest[2] = 0u;
                a->rel_any = a->rel_hi = a->abs_any = a->abs_lo = a->abs_hi = 0u;
                a->d_i = 1u;
            }
            if (a->d_i > GT2_MAX_INSTR) {
                a->d_phase = D_WAVECMD;
                a->d_i = 1u;
                break;
            }
            if (BIT(a->base.instrused, a->d_i))
                units = derive_instr(a, a->d_i);
            a->d_i++;
            break;
        case D_WAVECMD:
            if (a->d_i > GT2_TABLE_ROWS) {
                a->d_phase = D_WTABLE;
                a->d_i = 1u;
                break;
            }
            if (BIT(a->base.tableused[GT2_WTBL], a->d_i))
                units = derive_wavecmd(a, a->d_i);
            a->d_i++;
            break;
        case D_WTABLE:
            if (a->d_i > GT2_TABLE_ROWS) {
                /* and a draft's rows, as if a channel's program reached them (gt2_analysis.xwave) */
                for (uint32_t i = 0u; a->xwave && i < a->xwave_n; i++) {
                    derive_wbytes(a, a->xwave[i], a->xwave[i + GT2_DRAFT_ROWS]);
                    /* and its delays from the start: a held note keeps its encoding, and a turn
                     * of the speed may bring them */
                    a->d_seen |= DSEEN_WAVEDELAY;
                }
                a->d_phase = D_PTABLE;
                a->d_i = 1u;
                break;
            }
            if (BIT(a->base.tableused[GT2_WTBL], a->d_i))
                derive_wrow(a, a->d_i);
            a->d_i++;
            break;
        case D_PTABLE:
            if (a->d_i > GT2_TABLE_ROWS) {
                a->d_phase = D_FINAL;
                break;
            }
            if (BIT(a->base.tableused[GT2_PTBL], a->d_i)) {
                uint8_t l = tl(a, GT2_PTBL, a->d_i), d = tr(a, GT2_PTBL, a->d_i);
                if (l != 0xffu && (d & 0x0fu))
                    a->d_seen |= DSEEN_PULSENYB;
            }
            a->d_i++;
            break;
        case D_FINAL:
            derive_final(a, out);
            units = GT2_MAX_PATTERNS + GT2_TABLE_ROWS + GT2_VAR_MAX;
            a->d_phase = D_STREAMS;
            a->d_ch = 0u;
            break;
        case D_STREAMS:
            if (a->d_ch >= GT2_PLAY_CHANNELS) {
                a->d_phase = D_END;
                break;
            }
            derive_stream(a, out, a->d_ch++);
            units = GT2_ORDER_STREAM;
            break;
        default:
            return 1;
        }
        *budget = units >= *budget ? 0u : *budget - units;
    }
    return a->d_phase == D_END;
}

void gt2_shape_derive_begin(gt2_analysis* a) {
    a->d_phase = D_INSTR;
    a->d_i = 0u;
}

int gt2_shape_derive_step(gt2_analysis* a, gt2_shape* out, uint32_t budget) {
    if (budget == 0u)
        budget = 1u;
    return derive_step(a, out, &budget);
}

void gt2_shape_derive(gt2_analysis* a, gt2_shape* out) {
    gt2_shape_derive_begin(a);
    while (!gt2_shape_derive_step(a, out, 0xffffffffu)) {
    }
}

/* --- Widening ---------------------------------------------------------------------------- */

int gt2_shape_include_jams(gt2_analysis* a, const uint32_t instruments[2], uint32_t lo,
                          uint32_t hi) {
    a->grew = 0u;
    for (uint32_t i = 0u; i < 2u; i++)
        GROW_OR(a, a->base.instrused[i], instruments[i]);
    if (lo <= hi && hi < NOTES) {
        if (lo < a->base.note_lo) {
            a->base.note_lo = (uint8_t)lo;
            a->grew = 1u;
        }
        if (a->base.note_hi == 0xffu || hi > a->base.note_hi) {
            a->base.note_hi = (uint8_t)hi;
            a->grew = 1u;
        }
    }
    return a->grew;
}

/* An orderlist of subtune sub changed (channel ch, or all three if ch is 3): scan it and
 * the patterns it newly names. */
static void widen_order(gt2_analysis* a, uint32_t sub) {
    gt2_base* b = &a->base;
    if (!packed_subtune(a, sub))
        return; /* the packer writes no orderlists for it (and an emptied one stays) */
    GROW_OR(a, b->packed, 1u << sub);
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++)
        scan_order(a, sub, ch, 0u, GT2_MAX_ORDER);
    for (uint32_t pat = 0u; pat < GT2_MAX_PATTERNS; pat++)
        if (BIT(b->pattused, pat) && !BIT(b->scanned, pat))
            scan_pattern(a, pat);
}

int gt2_shape_widen(gt2_analysis* a, const gt2_edit* e) {
    const gt2_song* s = a->song;
    a->grew = 0u;
    uint32_t off = e->offset;
    int derive = 0;
    if (e->kind == GT2_EDIT_SWAP) {
        if (off >= OFF_PATTERN && off < OFF_PATTERN + GT2_MAX_PATTERNS * 4u) {
            uint32_t pat = (off - OFF_PATTERN) / 4u;
            if (BIT(a->base.pattused, pat))
                scan_pattern(a, pat);
        } else if (off >= OFF_ORDER && off < OFF_ORDER + sizeof s->order) {
            uint32_t slot = (off - OFF_ORDER) / 4u;
            if (slot % GT2_MAX_CHANNELS < GT2_PLAY_CHANNELS) {
                widen_order(a, slot / GT2_MAX_CHANNELS);
                derive = 1; /* the streams */
            }
        }
    } else if (e->kind == GT2_EDIT_CELL) {
        if (off >= OFF_INSTR && off < OFF_INSTR + sizeof s->instr) {
            uint32_t i = (off - OFF_INSTR) / GT2_INSTR_BYTES + 1u;
            derive = BIT(a->base.instrused, i) || i == GT2_MAX_INSTR || i == 1u;
        } else if (off >= OFF_LTABLE && off < OFF_LTABLE + 2u * sizeof s->ltable) {
            uint32_t k = off - OFF_LTABLE;
            uint32_t t = (k / GT2_TABLE_ROWS) % GT2_TABLES;
            for (uint32_t j = 0u; j < e->n; j++) {
                uint32_t row = (k + j) % GT2_TABLE_ROWS + 1u;
                if (!BIT(a->base.tableused[t], row) && !(t == GT2_STBL && BIT(a->base.stbl_calc, row)))
                    continue;
                derive = 1;
                /* the program may now go elsewhere from this row */
                if (t != GT2_STBL) {
                    uint32_t next = tl(a, t, row) == LOOP ? tr(a, t, row) : row + 1u;
                    reach(a, t, next);
                }
            }
        } else if (off >= s->arena_offset) {
            /* a pattern row or an orderlist byte */
            for (uint32_t pat = 0u; pat < GT2_MAX_PATTERNS; pat++) {
                uint32_t poff;
                uint32_t rows = pattern_rows(a, pat, &poff);
                if (rows == 0u || off < poff || off >= poff + rows * GT2_ROW_BYTES)
                    continue;
                if (BIT(a->base.pattused, pat)) {
                    uint8_t row[4];
                    read_row(a, poff, (off - poff) / GT2_ROW_BYTES, row);
                    scan_row(a, pat, row);
                }
                goto widened;
            }
            for (uint32_t sub = 0u; sub < s->subtunes; sub++) {
                for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
                    uint32_t ooff;
                    uint32_t n = order_entries(a, sub, ch, &ooff);
                    if (n == 0u || off < ooff || off >= ooff + n + 2u)
                        continue;
                    widen_order(a, sub);
                    derive = 1; /* the streams */
                    goto widened;
                }
            }
        }
    }
widened:
    return derive || a->grew;
}

void gt2_shape_keep_channels(gt2_shape* sh, uint32_t channels) {
    sh->channels = (uint8_t)channels;
    derive_var(sh);
}

int gt2_shape_compatible(const gt2_shape* from, const gt2_shape* to) {
    /* How the state reads: the pulse in one byte or two, the wavetable's timer and recoded
     * waveforms. A first command of any class (NOEFFECTS) or a filter (NOFILTER) changes what
     * runs from the next call, not what the state means. */
    const uint32_t encodings = GT2_PF_SIMPLEPULSE | GT2_PF_WAVEDELAY | GT2_PF_WAVECMD;
    return from->channels == to->channels && from->subtune == to->subtune &&
           from->songs == to->songs && ((from->flags ^ to->flags) & encodings) == 0u;
}

uint8_t gt2_shape_order_byte(const gt2_analysis* a, uint32_t ch, uint32_t pos) {
    uint32_t songs;
    int sub = packed_index(a, &songs);
    uint32_t off;
    uint32_t n = sub < 0 ? 0u : order_entries(a, (uint32_t)sub, ch, &off);
    uint32_t k = 0u;
    for (uint32_t i = 0u; i < pos && i < n; i++)
        k += vb(a, off + i) != REPEAT;
    return (uint8_t)(k > 0xffu ? 0xffu : k);
}

int gt2_play_init(gt2_player* p, gt2_shape* shape, const gt2_song* s, uint32_t subtune,
                  const gt2_options* o) {
    gt2_analysis a;
    int r = gt2_shape_analyse(&a, s, subtune, o, shape);
    if (r < 0) {
        uint8_t* q = (uint8_t*)p;
        for (uint32_t i = 0u; i < sizeof *p; i++)
            q[i] = 0u;
        return r;
    }
    /* through a pointer: the frame path is in SRAM on the device (song_ctl.c, far_start) */
    void (*volatile start)(gt2_player*, const gt2_song*, const gt2_shape*, const uint8_t*) =
        gt2_play_start;
    p->draft = 0; /* none (gt2_play_start keeps it) */
    start(p, s, shape, 0);
    return 0;
}
