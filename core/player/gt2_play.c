// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song player (gt2_play.h). It follows GoatTracker 2.77's src/player.s (and
 * altplayer.s) routine for routine, its labels named in the comments, and takes its data:
 * the frequency table (src/gplay.c's freqtbllo/freqtblhi). The song analysis transliterates
 * the parts of src/greloc.c's relocator() and exectable() (src/gtable.c) that decide the
 * packed player's shape. greloc.c and gplay.c are GPL-2.0-or-later, whose "any later
 * version" lets this file stay GPL-3.0-or-later; player.s carries its own permissive
 * notice. Where a 6502 flag decides a branch, the C keeps the flag (the carry, mostly) as
 * a variable. */
#include "gt2_play.h"

/* gplay.c's frequency table: C-0 to B-7. In RAM, as the audio path's tables must be on the
 * device (docs/hardware.md, "Code in SRAM"): pi32v2's clang would otherwise put a static
 * that is never written in flash's .rodata, so the section is named, as sid_env.h does. */
#ifdef __PI32V2__
#define GT2_RAM_DATA __attribute__((section(".data.gt2_play")))
#else
#define GT2_RAM_DATA
#endif

GT2_RAM_DATA static const uint8_t freq_lo[96] = {
    0x17, 0x27, 0x39, 0x4b, 0x5f, 0x74, 0x8a, 0xa1, 0xba, 0xd4, 0xf0, 0x0e, 0x2d, 0x4e, 0x71,
    0x96, 0xbe, 0xe8, 0x14, 0x43, 0x74, 0xa9, 0xe1, 0x1c, 0x5a, 0x9c, 0xe2, 0x2d, 0x7c, 0xcf,
    0x28, 0x85, 0xe8, 0x52, 0xc1, 0x37, 0xb4, 0x39, 0xc5, 0x5a, 0xf7, 0x9e, 0x4f, 0x0a, 0xd1,
    0xa3, 0x82, 0x6e, 0x68, 0x71, 0x8a, 0xb3, 0xee, 0x3c, 0x9e, 0x15, 0xa2, 0x46, 0x04, 0xdc,
    0xd0, 0xe2, 0x14, 0x67, 0xdd, 0x79, 0x3c, 0x29, 0x44, 0x8d, 0x08, 0xb8, 0xa1, 0xc5, 0x28,
    0xcd, 0xba, 0xf1, 0x78, 0x53, 0x87, 0x1a, 0x10, 0x71, 0x42, 0x89, 0x4f, 0x9b, 0x74, 0xe2,
    0xf0, 0xa6, 0x0e, 0x33, 0x20, 0xff};
GT2_RAM_DATA static const uint8_t freq_hi[96] = {
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02,
    0x02, 0x02, 0x02, 0x03, 0x03, 0x03, 0x03, 0x03, 0x04, 0x04, 0x04, 0x04, 0x05, 0x05, 0x05,
    0x06, 0x06, 0x06, 0x07, 0x07, 0x08, 0x08, 0x09, 0x09, 0x0a, 0x0a, 0x0b, 0x0c, 0x0d, 0x0d,
    0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x17, 0x18, 0x1a, 0x1b, 0x1d, 0x1f, 0x20,
    0x22, 0x24, 0x27, 0x29, 0x2b, 0x2e, 0x31, 0x34, 0x37, 0x3a, 0x3e, 0x41, 0x45, 0x49, 0x4e,
    0x52, 0x57, 0x5c, 0x62, 0x68, 0x6e, 0x75, 0x7c, 0x83, 0x8b, 0x93, 0x9c, 0xa5, 0xaf, 0xb9,
    0xc4, 0xd0, 0xdd, 0xea, 0xf8, 0xff};

#define NOTES 96u

/* player.s's data bytes. */
#define NOTE 0x60u
#define REST 0xbdu
#define REPEAT 0xd0u
#define TRANSDOWN 0xe0u
#define TRANS 0xf0u
#define LOOP 0xffu
#define TONEPORTA 3u
#define TEMPO_GLOBAL (1u << 16)
#define TEMPO_CHANNEL (1u << 17)
/* The instrument classes the packer sorts by (greloc.c: normal, no-HR, legato). */
enum { CLASS_HR, CLASS_NOHR, CLASS_LEGATO };

typedef struct {
    gt2_player* p;
    gt2_sid_fn sink;
    void* ctx;
} out;

/* --- The song's bytes, through the index each time ------------------------------------ */

static const uint8_t* arena(const gt2_song* s) {
    return (const uint8_t*)s + s->arena_offset;
}

/* A table's left or right byte at a 1-based position; 0 past the table (the packed table
 * ends there too) and at position 0 (the speedtable's extra zero entry). */
static uint8_t tl(const gt2_song* s, uint32_t t, uint32_t pos) {
    return pos - 1u < GT2_TABLE_ROWS ? s->ltable[t][pos - 1u] : 0u;
}

static uint8_t tr(const gt2_song* s, uint32_t t, uint32_t pos) {
    return pos - 1u < GT2_TABLE_ROWS ? s->rtable[t][pos - 1u] : 0u;
}

static const uint8_t* instrument(const gt2_song* s, uint8_t i) {
    static const uint8_t none[GT2_INSTR_BYTES] GT2_RAM_DATA = {0}; /* in RAM, as freq_lo */
    return i >= 1u && i <= GT2_MAX_INSTR ? s->instr[i - 1u] : none;
}

/* The packer's instrument bytes: the gate-off timer's low six bits; the vibrato only with a
 * delay, and the delay less one. */
static uint8_t ins_gatetimer(const gt2_song* s, uint8_t i) {
    return instrument(s, i)[GT2_INS_GATETIMER] & 0x3fu;
}

static uint8_t ins_vibdelay(const gt2_song* s, uint8_t i) {
    uint8_t d = instrument(s, i)[GT2_INS_VIBDELAY];
    return d ? (uint8_t)(d - 1u) : 0u;
}

static uint8_t ins_vibparam(const gt2_song* s, uint8_t i) {
    const uint8_t* ins = instrument(s, i);
    return ins[GT2_INS_VIBDELAY] ? ins[GT2_INS_VIBPARAM] : 0u;
}

/* A pattern's rows (the end row not counted) and its bytes; NULL for a pattern not held
 * (64 empty rows) or a number past the patterns. */
static const uint8_t* pattern(const gt2_song* s, uint8_t pat, uint32_t* rows) {
    gt2_span sp = pat < GT2_MAX_PATTERNS ? s->pattern[pat] : 0u;
    if (sp == 0u) {
        *rows = GT2_DEFAULT_ROWS;
        return 0;
    }
    *rows = GT2_SPAN_LEN(sp) - 1u;
    return arena(s) + GT2_SPAN_OFF4(sp) * GT2_UNIT;
}

/* --- The packed orderlist -------------------------------------------------------------
 * The packer (greloc.c, "Generate songorderlists") writes each entry as it is, except a
 * pattern's number, which it renumbers among the patterns in use (pattmap); a repeat
 * ($D1-$DF) before a pattern, which it writes after the pattern (player.s reads "pattern,
 * repeat"); and a $D0 (R0), which it drops. Then the $FF and the restart byte, a position
 * among the packed entries. player.s steps through the packed bytes with an 8-bit index;
 * the player steps through the model's entries the same way, skipping any $D0, and maps a
 * restart position to the model's.
 *
 * An orderlist that breaks readme 3.1's rules (a transpose last before the $FF) makes
 * player.s read past its restart byte: into the next orderlist the packer wrote (the
 * subtunes' channels in order; with $D0s dropped, an orderlist's last bytes are already the
 * next one's first), and past the last into the patterns. The player reads the same next
 * orderlists; the packed patterns past them it reads as $00. A byte read where the packed
 * player takes it as is (a restart byte) is marked RAW: as a pattern number it is the
 * packer's numbering. */

#define RAW 0x100u

typedef struct {
    const uint8_t* e; /* the entries, the $FF and the restart byte */
    uint32_t n;       /* entries */
} order_view;

static uint32_t entries(const gt2_song* s, uint32_t sub, uint32_t ch) {
    gt2_span sp = sub < s->subtunes ? s->order[sub][ch] : 0u;
    return sp != 0u && GT2_SPAN_LEN(sp) >= 2u ? GT2_SPAN_LEN(sp) - 2u : 0u;
}

/* A subtune the packer writes orderlists for: three non-empty channels. */
static int packed_subtune(const gt2_song* s, uint32_t sub) {
    return entries(s, sub, 0u) && entries(s, sub, 1u) && entries(s, sub, 2u);
}

static order_view order_of(const gt2_song* s, uint32_t sub, uint32_t ch) {
    order_view v = {0, entries(s, sub, ch)};
    if (v.n)
        v.e = arena(s) + GT2_SPAN_OFF4(s->order[sub][ch]) * GT2_UNIT;
    return v;
}

static int is_repeat(uint8_t b) {
    return b > REPEAT && b < TRANSDOWN;
}

/* The model position after i (player.s's INY), past any $D0. */
static uint32_t order_next(order_view v, uint32_t i) {
    i++;
    while (i < v.n && v.e[i] == REPEAT)
        i++;
    return i;
}

/* The model position of packed position r. */
static uint32_t order_at(order_view v, uint32_t r) {
    uint32_t packed = 0u;
    for (uint32_t i = 0u; i < v.n; i++) {
        if (v.e[i] == REPEAT)
            continue;
        if (packed == r)
            return i;
        packed++;
    }
    return v.n + (r - packed);
}

/* The packed byte at model position i of a subtune's channel (RAW marked). */
static uint32_t order_byte(const gt2_player* p, uint32_t sub, uint32_t ch, uint32_t i) {
    const gt2_song* s = p->song;
    for (uint32_t hop = 0u; hop <= GT2_MAX_SUBTUNES * GT2_PLAY_CHANNELS; hop++) {
        order_view v = order_of(s, sub, ch);
        if (i < v.n) {
            uint8_t b = v.e[i];
            if (is_repeat(b) && i + 1u < v.n && v.e[i + 1u] < REPEAT)
                return v.e[i + 1u];
            if (b < REPEAT && i > 0u && is_repeat(v.e[i - 1u]))
                return v.e[i - 1u];
            return b;
        }
        if (v.e == 0)
            return RAW;
        if (i == v.n)
            return LOOP;
        if (i == v.n + 1u)
            return RAW | v.e[v.n + 1u];
        /* Past the restart byte: the next orderlist the packer wrote. */
        uint32_t plen = 0u;
        for (uint32_t k = 0u; k < v.n; k++)
            plen += v.e[k] != REPEAT;
        uint32_t pp = plen + (i - v.n);
        uint32_t q = pp < v.n + 2u ? pp - (plen + 2u) : pp - (v.n + 2u);
        do {
            if (++ch == GT2_PLAY_CHANNELS) {
                ch = 0u;
                if (++sub >= p->songs)
                    return RAW; /* the packed patterns */
            }
        } while (!packed_subtune(s, sub));
        i = order_at(order_of(s, sub, ch), q);
    }
    return RAW;
}

/* The model's pattern for the packer's pattern number q (the q-th pattern in use). */
static uint8_t packed_pattern(const gt2_player* p, uint32_t q) {
    for (uint32_t pat = 0u; pat < GT2_MAX_PATTERNS; pat++) {
        if ((p->pattused[pat >> 5] >> (pat & 31u)) & 1u) {
            if (q == 0u)
                return (uint8_t)pat;
            q--;
        }
    }
    return 0xffu; /* past the pattern table: an empty pattern */
}

/* --- Frequencies ----------------------------------------------------------------------
 * The packed table holds notes first_note to last_note (greloc.c), lows then highs:
 * player.s reads mt_freqtbllo-FIRSTNOTE+y. A note outside the table reads the 6502's
 * memory around it, and the player reads what it can know of that memory:
 * - past the lows, the highs (a note past the table reads its low byte there);
 * - past the highs, the packed song table, whose bytes are addresses that depend on the
 *   assembled player's size: no player without the packed layout knows them;
 * - below the lows, the end of the player's own variables (player.s's mt_chn* blocks, as
 *   the song's assembly lays them out), most of which the player holds: var_byte.
 * A byte it cannot know reads 0, as GoatTracker's editor reads past B-7, and marks the
 * read in `unknown`. */

enum { G_NONE, G_GHOST, G_1, G_2, G_3, G_4, G_5, G_4B, G_1O, G_2O };

/* The packer's number for a pattern (its rank among those in use), or -1. */
static int packed_pattern_number(const gt2_player* p, uint8_t pat) {
    if (pat >= GT2_MAX_PATTERNS || !((p->pattused[pat >> 5] >> (pat & 31u)) & 1u))
        return -1;
    int n = 0;
    for (uint32_t i = 0u; i < pat; i++)
        n += (int)((p->pattused[i >> 5] >> (i & 31u)) & 1u);
    return n;
}

static int class_of(uint8_t g) {
    return (g & 0x40u) ? CLASS_LEGATO : (g & 0x80u) ? CLASS_NOHR : CLASS_HR;
}

/* The packer's number for an instrument: its class's first number, then its rank. */
static int packed_instr_number(const gt2_player* p, uint8_t ins) {
    if (ins == 0u || ins > GT2_MAX_INSTR || !((p->instrused[ins >> 5] >> (ins & 31u)) & 1u))
        return -1;
    int cls = class_of(p->song->instr[ins - 1u][GT2_INS_GATETIMER]);
    int n = 1;
    for (int k = 0; k < cls; k++)
        n += p->class_count[k];
    for (uint32_t i = 1u; i < ins; i++)
        if (((p->instrused[i >> 5] >> (i & 31u)) & 1u) &&
            class_of(p->song->instr[i - 1u][GT2_INS_GATETIMER]) == cls)
            n++;
    return n;
}

/* A variable's byte as player.s holds it, or -1 if the player cannot know it. */
static int group_byte(const gt2_player* p, uint32_t group, uint32_t ch, uint32_t field) {
    const gt2_channel* c = &p->chn[ch];
    int ghost = (p->flags & GT2_PF_GHOST) != 0;
    switch (group) {
    case G_3: {
        const uint8_t v[7] = {
            (uint8_t)(p->songs > 1u ? p->subtune * 3u + ch : ch), 0u, c->tempo, c->counter,
            c->note, 0u, c->gate};
        if (field == 1u)
            return packed_pattern_number(p, c->pattnum);
        if (field == 5u)
            return packed_instr_number(p, c->instr);
        return v[field];
    }
    case G_4: {
        const uint8_t v[7] = {c->vibtime,
                              (p->layout & GT2_LAYOUT_NOINSVIB) ? 0u : c->vibdelay,
                              c->wavetime,
                              ghost ? 0u : c->freqlo,
                              ghost ? 0u : c->freqhi,
                              ghost ? 0u : c->pulselo,
                              ghost ? 0u : c->pulsehi};
        return v[field];
    }
    case G_5: {
        int buffered = (p->flags & GT2_PF_BUFFERED) && !ghost;
        const uint8_t v[7] = {buffered ? c->ad : 0u,
                              buffered ? c->sr : 0u,
                              0u,
                              0u,
                              0u,
                              (p->layout & GT2_LAYOUT_FIXEDPARAMS) ? 0u : c->gatetimer,
                              (p->layout & GT2_LAYOUT_NOCALCSPEED) ? 0u : c->lastnote};
        return v[field];
    }
    case G_4B: {
        const uint8_t v[7] = {c->vibtime,
                              (p->layout & GT2_LAYOUT_NOINSVIB) ? 0u : c->vibdelay,
                              c->wavetime,
                              0u,
                              0u,
                              0u,
                              (p->layout & GT2_LAYOUT_FIXEDPARAMS) ? 0u : c->gatetimer};
        return v[field];
    }
    case G_2O: {
        const uint8_t v[7] = {c->pulsetime, c->pulselo, c->pulsehi, c->vibtime,
                              (p->layout & GT2_LAYOUT_NOINSVIB) ? 0u : c->vibdelay,
                              c->freqlo, c->freqhi};
        return v[field];
    }
    default:
        return -1;
    }
}

/* The byte `back` bytes below the frequency table (1: the last before it). The variables
 * come in blocks of seven a channel, NUMCHANNELS channels a block (player.s, "Normal channel
 * variables" or "Optimized channel variables"), then the ghost registers of full buffering;
 * the frequency table follows them. */
static int var_byte(const gt2_player* p, uint32_t back) {
    uint8_t order[6];
    uint32_t k = 0u;
    if ((p->flags & GT2_PF_GHOST) && !(p->flags & GT2_PF_ZPGHOST))
        order[k++] = G_GHOST;
    if (p->layout & GT2_LAYOUT_NORMAL) {
        if (p->layout & GT2_LAYOUT_G4) {
            if (p->layout & GT2_LAYOUT_G5)
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
    uint32_t size = 7u * p->channels;
    for (uint32_t i = 0u; i < k; i++) {
        uint32_t n = order[i] == G_GHOST ? GT2_SID_REGS : size;
        if (back <= n) {
            uint32_t idx = n - back;
            if (order[i] == G_GHOST)
                return p->regs[idx];
            return group_byte(p, order[i], idx / 7u, idx % 7u);
        }
        back -= n;
    }
    return -1; /* the player's code */
}

static uint8_t freq_mem(gt2_player* p, int32_t off) {
    int32_t count = (int32_t)p->last_note - p->first_note + 1;
    int v = -1;
    if (off < 0)
        v = var_byte(p, (uint32_t)-off);
    else if (off < count)
        v = freq_lo[p->first_note + off];
    else if (off < 2 * count)
        v = freq_hi[p->first_note + off - count];
    if (v < 0) {
        p->unknown = 1u;
        return 0u;
    }
    return (uint8_t)v;
}

static uint8_t freq_byte(gt2_player* p, uint32_t note, int high) {
    int32_t count = (int32_t)p->last_note - p->first_note + 1;
    return freq_mem(p, (int32_t)note - p->first_note + (high ? count : 0));
}

/* --- Writes -------------------------------------------------------------------------- */

static void wr(out* o, uint32_t reg, uint8_t v) {
    o->p->regs[reg] = v;
    if (!(o->p->flags & GT2_PF_GHOST))
        o->sink(o->ctx, (uint8_t)reg, v);
}

/* A write player.s makes at once unbuffered, and keeps in the channel's variable when
 * buffered (mt_chnad and the others), for mt_loadregs. */
static int unbuffered(const out* o) {
    return !(o->p->flags & GT2_PF_BUFFERED);
}

static void set_ad(out* o, gt2_channel* c, uint32_t x, uint8_t v) {
    c->ad = v;
    if (unbuffered(o))
        wr(o, x + 5u, v);
}

static void set_sr(out* o, gt2_channel* c, uint32_t x, uint8_t v) {
    c->sr = v;
    if (unbuffered(o))
        wr(o, x + 6u, v);
}

/* mt_loadregs, buffered: player.s writes AD, SR, pulse, frequency, wave; altplayer.s wave,
 * frequency, pulse, AD, SR. With simple pulse both pulse registers get the low byte. With
 * ghost registers the values go to the ghost registers only (wr). */
static void load_all(out* o, const gt2_channel* c, uint32_t x) {
    uint8_t ph = (o->p->flags & GT2_PF_SIMPLEPULSE) ? c->pulselo : c->pulsehi;
    uint8_t w = (uint8_t)(c->wave & c->gate);
    if (o->p->flags & GT2_PF_ALT) {
        wr(o, x + 4u, w);
        wr(o, x + 0u, c->freqlo);
        wr(o, x + 1u, c->freqhi);
        wr(o, x + 2u, c->pulselo);
        wr(o, x + 3u, ph);
        wr(o, x + 5u, c->ad);
        wr(o, x + 6u, c->sr);
    } else {
        wr(o, x + 5u, c->ad);
        wr(o, x + 6u, c->sr);
        wr(o, x + 2u, c->pulselo);
        wr(o, x + 3u, ph);
        wr(o, x + 0u, c->freqlo);
        wr(o, x + 1u, c->freqhi);
        wr(o, x + 4u, w);
    }
}

/* mt_loadregs. */
static void loadregs(out* o, const gt2_channel* c, uint32_t x) {
    uint8_t w = (uint8_t)(c->wave & c->gate);
    if (!unbuffered(o)) {
        load_all(o, c, x);
    } else if (o->p->flags & GT2_PF_ALT) {
        wr(o, x + 4u, w);
        wr(o, x + 0u, c->freqlo);
        wr(o, x + 1u, c->freqhi);
    } else {
        wr(o, x + 0u, c->freqlo);
        wr(o, x + 1u, c->freqhi);
        wr(o, x + 4u, w);
    }
}

/* player.s's mt_loadregswaveonly: the wave register alone (with ghost registers, the
 * ghost wave register). */
static void loadregs_wave(out* o, const gt2_channel* c, uint32_t x) {
    wr(o, x + 4u, (uint8_t)(c->wave & c->gate));
}

/* --- Tick 0 commands (mt_tick0_0 to mt_tick0_f) ------------------------------------- */

static void all_tempo(gt2_player* p, uint8_t a) {
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++)
        p->chn[ch].tempo = a;
}

/* A command's tick 0 part, with A its parameter (mt_tick0jumptbl). Commands 0-4 set the
 * continuous effect the following ticks run (mt_tick0_34); 5-F act once. */
static void tick0_cmd(out* o, gt2_channel* c, uint32_t x, uint8_t cmd, uint8_t a) {
    gt2_player* p = o->p;
    switch (cmd) {
    case 0u: /* instrument vibrato */
        a = ins_vibparam(p->song, c->instr);
        goto tick0_34;
    case 1u:
    case 2u: /* portamentos: the vibrato's phase restarts */
        c->vibtime = 0u;
        /* fall through */
    case 3u:
    case 4u:
    tick0_34:
        c->param = a;
        c->fx = c->newfx;
        break;
    case 5u:
        set_ad(o, c, x, a);
        break;
    case 6u:
        set_sr(o, c, x, a);
        break;
    case 7u:
        c->wave = a;
        break;
    case 8u:
        c->waveptr = a;
        c->wavetime = 0u;
        break;
    case 9u:
        c->pulseptr = a;
        c->pulsetime = 0u;
        break;
    case 10u:
        p->filttime = 0u;
        p->filtstep = a;
        break;
    case 11u: /* $00 also stops the filter table */
        p->filtctrl = a;
        if (a == 0u)
            p->filtstep = 0u;
        break;
    case 12u:
        p->filtcutoff = a;
        break;
    case 13u: /* the master volume, or the timing mark with author info */
        if ((p->flags & GT2_PF_AUTHORINFO) && a >= 0x10u)
            p->timing_mark = a;
        else
            p->masterfader = a;
        break;
    case 14u: /* funktempo: the speedtable entry's two tempos, all channels on the first */
        p->funktempo[0] = tl(p->song, GT2_STBL, a);
        p->funktempo[1] = tr(p->song, GT2_STBL, a);
        all_tempo(p, 0u);
        break;
    default: /* 15: a channel's tempo ($80 and up) or all three, as assembled */
        if ((a & 0x80u) && (p->tick0 & TEMPO_CHANNEL))
            c->tempo = (uint8_t)(a & 0x7fu);
        else if (p->tick0 & TEMPO_GLOBAL)
            all_tempo(p, a);
        else
            c->tempo = (uint8_t)(a & 0x7fu);
        break;
    }
}

/* --- Continuous effects (mt_setspeedparam to mt_effect_3_found) ----------------------- */

/* The channel's frequency from a note: known, unless a byte came from past the table. */
static void set_note_freq(gt2_player* p, gt2_channel* c, uint8_t note) {
    uint32_t bit = 1u << (uint32_t)(c - p->chn);
    c->lastnote = note; /* mt_wavenoteabs */
    c->vibtime = 0u;    /* mt_wavenote */
    p->unknown = 0u;
    c->freqlo = freq_byte(p, note, 0);
    c->freqhi = freq_byte(p, note, 1);
    p->freq_unknown = (uint8_t)(p->unknown ? p->freq_unknown | bit : p->freq_unknown & ~bit);
}

/* mt_freqadd and mt_freqsub, with the 6502's carry in. */
static void freq_add(gt2_channel* c, uint8_t t1, uint8_t t2, uint32_t carry) {
    uint32_t lo = (uint32_t)c->freqlo + t1 + carry;
    c->freqlo = (uint8_t)lo;
    c->freqhi = (uint8_t)(c->freqhi + t2 + (lo >> 8));
}

static void freq_sub(gt2_channel* c, uint8_t t1, uint8_t t2, uint32_t carry) {
    uint32_t lo = (uint32_t)c->freqlo - t1 - (1u - carry);
    c->freqlo = (uint8_t)lo;
    c->freqhi = (uint8_t)(c->freqhi - t2 - ((lo >> 8) & 1u));
}

/* One tick of effect eff (0 instrument vibrato, 1 and 2 portamentos, 3 toneportamento, 4
 * vibrato) with speedtable entry y. A speed's left byte $80 and up is note-independent:
 * the step between the last note and the next, shifted right by the right byte. */
static void effect(gt2_player* p, gt2_channel* c, uint8_t eff, uint8_t y) {
    const gt2_song* s = p->song;
    p->unknown = 0u;
    uint8_t left = tl(s, GT2_STBL, y), right = tr(s, GT2_STBL, y);
    uint8_t t1, t2;
    int zero; /* the Z flag mt_effect_0 tests */
    if (left & 0x80u) {
        /* mt_calculatedspeed */
        uint32_t n = c->lastnote;
        uint32_t lo = (uint32_t)freq_byte(p, n + 1u, 0) - freq_byte(p, n, 0);
        uint32_t hi = (uint32_t)freq_byte(p, n + 1u, 1) - freq_byte(p, n, 1) - ((lo >> 8) & 1u);
        uint32_t v = ((hi & 0xffu) << 8) | (lo & 0xffu);
        v = right < 16u ? v >> right : 0u;
        t1 = (uint8_t)v;
        t2 = (uint8_t)(v >> 8);
        zero = y == 0u;
    } else {
        t2 = left;
        t1 = right;
        zero = right == 0u;
    }
    int found = 0;
    switch (eff) {
    case 0u: /* mt_effect_0: the instrument's vibrato after its delay */
        if (zero)
            return;
        if (c->vibdelay) {
            c->vibdelay--;
            return;
        }
        /* fall through */
    case 4u: { /* mt_effect_4 */
        uint8_t speed = (uint8_t)(left & 0x7fu);
        if (!(left & 0x80u))
            t2 = 0u;
        uint8_t a = c->vibtime;
        if (!(a & 0x80u) && a > speed)
            a = (uint8_t)~a;
        a = (uint8_t)(a + 2u);
        c->vibtime = a;
        if (a & 1u)
            freq_sub(c, t1, t2, 1u);
        else
            freq_add(c, t1, t2, 0u);
        break;
    }
    case 3u: { /* mt_effect_3: toneportamento; speed 0 ties the note */
        if (y == 0u) {
            found = 1;
            break;
        }
        uint8_t tlo = freq_byte(p, c->note, 0), thi = freq_byte(p, c->note, 1);
        uint32_t dlo = (uint32_t)c->freqlo - tlo;
        uint32_t dhi = (uint32_t)c->freqhi - thi - ((dlo >> 8) & 1u);
        if (!(dhi & 0x100u)) {
            /* at or above the target: down until the offset changes sign */
            uint32_t lo = (dlo & 0xffu) - t1;
            uint32_t hi = (dhi & 0xffu) - t2 - ((lo >> 8) & 1u);
            if (hi & 0x80u)
                found = 1;
            else
                freq_sub(c, t1, t2, (hi >> 8) & 1u ? 0u : 1u);
        } else {
            uint32_t lo = (dlo & 0xffu) + t1;
            uint32_t hi = (dhi & 0xffu) + t2 + (lo >> 8);
            if (!(hi & 0x80u))
                found = 1;
            else
                freq_add(c, t1, t2, hi >> 8);
        }
        break;
    }
    case 1u:
        freq_add(c, t1, t2, 0u);
        break;
    default:
        freq_sub(c, t1, t2, 1u);
        break;
    }
    if (found)
        set_note_freq(p, c, c->note); /* mt_effect_3_found */
    else if (p->unknown)
        p->freq_unknown = (uint8_t)(p->freq_unknown | (1u << (uint32_t)(c - p->chn)));
}

/* --- The channel ----------------------------------------------------------------------- */

static int instr_class(const gt2_player* p, uint8_t i) {
    uint8_t g = instrument(p->song, i)[GT2_INS_GATETIMER];
    if ((g & 0x40u) && i != p->first_legato)
        return CLASS_LEGATO;
    return (g & 0xc0u) ? CLASS_NOHR : CLASS_HR;
}

/* The wavetable's left byte as the packer writes it (greloc.c, "In wavetable, convert
 * waveform values for the playroutine"): the inaudible waveforms $E0-$EF become $00-$0F,
 * and when the song uses delays every waveform moves up by $10, past them. */
static uint8_t wave_left(const gt2_player* p, uint8_t l) {
    uint8_t w = (l >= 0xe0u && l <= 0xefu) ? (uint8_t)(l & 0x0fu) : l;
    if ((p->flags & GT2_PF_WAVEDELAY) && l > 0x0fu && l <= 0xefu)
        w = (uint8_t)(w + 0x10u);
    return w;
}

/* The sequencer (mt_sequencer to mt_repeatdone2), at tick 0 when a pattern is due. y is
 * player.s's 8-bit index: positions wrap past 255. */
static uint32_t step(order_view v, uint32_t y) {
    y = order_next(v, y);
    return y > 0xffu ? y & 0xffu : y;
}

static void sequencer(gt2_player* p, gt2_channel* c, uint32_t ch) {
    order_view v = order_of(p->song, p->subtune, ch);
    uint32_t y = c->songptr;
    uint32_t b = order_byte(p, p->subtune, ch, y);
    if ((uint8_t)b == LOOP) {
        y = order_at(v, order_byte(p, p->subtune, ch, step(v, y)) & 0xffu);
        b = order_byte(p, p->subtune, ch, y);
    }
    uint8_t a = (uint8_t)b;
    if (a >= TRANSDOWN) {
        c->trans = (uint8_t)(a - TRANS);
        y = step(v, y);
        b = order_byte(p, p->subtune, ch, y);
        a = (uint8_t)b;
    }
    if (a >= REPEAT) {
        /* mt_repeat: the pattern before plays again until the count is reached. */
        c->repeat++;
        if ((uint8_t)(a - REPEAT) != c->repeat)
            return;
        c->repeat = 0u;
    } else {
        c->pattnum = (b & RAW) ? packed_pattern(p, a) : a;
        /* the entry in the model: a repeat's pattern sits after it */
        if ((b & RAW) || y >= v.n)
            c->orderpos = 0xffu;
        else if (is_repeat(v.e[y]) && y + 1u < v.n && v.e[y + 1u] < REPEAT)
            c->orderpos = (uint8_t)(y + 1u);
        else
            c->orderpos = (uint8_t)y;
    }
    c->songptr = (uint16_t)step(v, y);
}

/* The pulsetable's byte as packed: with simple pulse a width's high nybble goes into the
 * low byte's low nybble (the one byte is written to both registers), and a modulation
 * speed becomes the same rotated step (greloc.c, "Write table right side"). */
static uint8_t pulse_right(const gt2_player* p, uint32_t pos) {
    const gt2_song* s = p->song;
    uint8_t l = tl(s, GT2_PTBL, pos), r = tr(s, GT2_PTBL, pos);
    if (!(p->flags & GT2_PF_SIMPLEPULSE) || l == LOOP)
        return r;
    if (l >= 0x80u)
        return (uint8_t)((l & 0x0fu) | (r & 0xf0u));
    uint8_t speed = (uint8_t)(r >> 4);
    if (r & 0x80u)
        speed = (uint8_t)((speed | 0xf0u) - 1u);
    return (uint8_t)((speed << 4) | (speed >> 4));
}

/* mt_pulseexec. */
static void pulse_exec(out* o, gt2_channel* c, uint32_t x) {
    gt2_player* p = o->p;
    const gt2_song* s = p->song;
    int simple = (p->flags & GT2_PF_SIMPLEPULSE) != 0;
    uint8_t y = c->pulseptr;
    if (c->pulsetime == 0u) {
        uint8_t l = tl(s, GT2_PTBL, y);
        if (simple && l != LOOP && l > 0x80u)
            l = 0x80u;
        if (l & 0x80u) {
            /* mt_setpulse */
            if (!simple)
                c->pulsehi = l;
            c->pulselo = pulse_right(p, y);
            goto next;
        }
        c->pulsetime = l; /* mt_newpulsemod */
    }
    /* mt_pulsemod */
    {
        uint8_t spd = pulse_right(p, y);
        uint32_t sum = (uint32_t)c->pulselo + spd;
        if (simple) {
            c->pulselo = (uint8_t)(sum + (sum >> 8));
        } else {
            if (spd & 0x80u)
                c->pulsehi--;
            c->pulselo = (uint8_t)sum;
            if (sum >> 8)
                c->pulsehi++;
        }
        if (--c->pulsetime != 0u)
            goto write;
    }
next:
    /* mt_nextpulsestep: the next row a jump: take it now */
    {
        uint32_t n = (uint32_t)y + 1u;
        c->pulseptr = tl(s, GT2_PTBL, n) == LOOP ? tr(s, GT2_PTBL, n) : (uint8_t)n;
    }
write:
    if (unbuffered(o)) {
        wr(o, x + 2u, c->pulselo);
        wr(o, x + 3u, simple ? c->pulselo : c->pulsehi);
    }
}

/* mt_getnewnote: the next row, at the gate-off timer's tick. */
static void getnewnote(out* o, gt2_channel* c, uint32_t x) {
    gt2_player* p = o->p;
    uint32_t rows;
    const uint8_t* pat = pattern(p->song, c->pattnum, &rows);
    uint8_t row[4] = {REST, 0u, 0u, 0u};
    if (pat != 0 && c->pattptr < rows) {
        const uint8_t* r = pat + (uint32_t)c->pattptr * GT2_ROW_BYTES;
        row[0] = r[0];
        row[1] = r[1];
        row[2] = r[2];
        row[3] = r[3];
    }
    c->ev_pending = 1u;
    c->ev_pattern = c->pattnum;
    c->ev_row = c->pattptr;
    c->ev_orderpos = c->orderpos;
    if (row[1] != 0u)
        c->instr = row[1]; /* mt_instr */
    if (!(p->flags & GT2_PF_NOEFFECTS)) {
        /* mt_fx, as packpattern writes the command: a timing mark erased without author
         * info, a tempo less one; command 0 carries no parameter */
        uint8_t cmd = row[2], data = row[3];
        if (cmd == 13u && data > 0x0fu && !(p->flags & GT2_PF_AUTHORINFO))
            cmd = data = 0u;
        if (cmd == 15u && (data & 0x7fu) >= 3u)
            data--;
        c->newfx = cmd;
        if (cmd != 0u)
            c->newparam = data;
    }
    uint8_t note = row[0];
    if (note < REST) {
        /* mt_normalnote: the note, transposed, starts at the next tick 0. */
        c->newnote = (uint8_t)(note + c->trans);
        if (c->newfx != TONEPORTA) {
            int cls = instr_class(p, c->instr);
            if (cls != CLASS_LEGATO) {
                if (cls == CLASS_HR) {
                    if (p->flags & GT2_PF_ALT) {
                        set_ad(o, c, x, p->ad_param);
                        set_sr(o, c, x, p->sr_param);
                    } else {
                        set_sr(o, c, x, p->sr_param);
                        set_ad(o, c, x, p->ad_param);
                    }
                }
                c->gate = 0xfeu; /* mt_skiphr */
            }
        }
    } else if (note > REST) {
        c->gate = (uint8_t)(note | 0xf0u); /* mt_gate: key off $FE, key on $FF */
    }
    /* mt_rest: the next row, or 0 at the pattern's end. */
    uint32_t next = (uint32_t)c->pattptr + 1u;
    c->pattptr = next < rows ? (uint8_t)next : 0u;
    loadregs(o, c, x);
}

/* mt_execchn. */
static void exec_channel(out* o, uint32_t ch) {
    gt2_player* p = o->p;
    const gt2_song* s = p->song;
    gt2_channel* c = &p->chn[ch];
    uint32_t x = ch * 7u;
    uint8_t y, a;

    c->counter--;
    if (c->counter != 0u) {
        /* mt_notick0: reload the counter when it passes 0 */
        if (c->counter & 0x80u) {
            a = c->tempo;
            if (a < 2u) {
                /* funktempo: bounce between the two values; SBC #0 with the carry clear */
                uint8_t i = a;
                c->tempo = (uint8_t)(a ^ 1u);
                a = (uint8_t)(p->funktempo[i] - 1u);
            }
            c->counter = a;
        }
        goto waveexec;
    }

    /* mt_tick0 */
    if (c->ev_pending) {
        gt2_event* e = &p->ev[ch];
        e->flags |= GT2_EV_ROW;
        e->pattern = c->ev_pattern;
        e->row = c->ev_row;
        e->order_pos = c->ev_orderpos;
        c->ev_pending = 0u;
    }
    if (c->pattptr == 0u)
        sequencer(p, c, ch);
    /* mt_nonewpatt */
    c->gatetimer = ins_gatetimer(s, c->instr);
    if (c->newnote != 0u) {
        /* mt_newnoteinit */
        const uint8_t* ins = instrument(s, c->instr);
        c->note = (uint8_t)(c->newnote - NOTE);
        c->fx = 0u;
        c->newnote = 0u;
        c->vibdelay = ins_vibdelay(s, c->instr);
        c->param = ins_vibparam(s, c->instr);
        p->ev[ch].flags |= c->newfx == TONEPORTA ? GT2_EV_SLIDE : GT2_EV_NOTE;
        p->ev[ch].note = c->note;
        p->ev[ch].instr = c->instr;
        if (c->newfx == TONEPORTA)
            goto nonewnoteinit;
        a = ins[GT2_INS_FIRSTWAVE];
        if (a != 0u) {
            if (a >= 0xfeu) {
                c->gate = a; /* mt_skipwave2: gate off ($FE) or on ($FF) */
            } else {
                c->wave = a;
                c->gate = 0xffu;
            }
        }
        /* mt_skipwave */
        if ((p->flags & GT2_PF_ALT) && unbuffered(o))
            loadregs_wave(o, c, x);
        if (ins[GT2_INS_PULSE] != 0u) {
            c->pulseptr = ins[GT2_INS_PULSE];
            c->pulsetime = 0u;
        }
        if (ins[GT2_INS_FILTER] != 0u && !(p->flags & GT2_PF_NOFILTER)) {
            p->filtstep = ins[GT2_INS_FILTER];
            p->filttime = 0u;
        }
        c->waveptr = ins[GT2_INS_WAVE];
        if (p->flags & GT2_PF_ALT) {
            set_ad(o, c, x, ins[GT2_INS_AD]);
            set_sr(o, c, x, ins[GT2_INS_SR]);
        } else {
            set_sr(o, c, x, ins[GT2_INS_SR]);
            set_ad(o, c, x, ins[GT2_INS_AD]);
        }
        /* the command's tick 0 part, then the registers (altplayer.s unbuffered: none) */
        if (!(p->flags & GT2_PF_NOEFFECTS))
            tick0_cmd(o, c, x, c->newfx, c->newparam);
        if (!unbuffered(o))
            loadregs(o, c, x);
        else if (!(p->flags & GT2_PF_ALT))
            loadregs_wave(o, c, x);
        return;
    }
nonewnoteinit:
    /* mt_nonewnoteinit: the command's tick 0 part, then the wavetable */
    if (!(p->flags & GT2_PF_NOEFFECTS))
        tick0_cmd(o, c, x, c->newfx, c->newparam);

waveexec:
    /* mt_waveexec */
    y = c->waveptr;
    if (y == 0u)
        goto wavedone;
    a = wave_left(p, tl(s, GT2_WTBL, y));
    if (p->flags & GT2_PF_WAVEDELAY) {
        if (a < 0x10u) {
            if (a == c->wavetime)
                goto nowavechange;
            c->wavetime++;
            goto wavedone;
        }
        a = (uint8_t)(a - 0x10u); /* mt_nowavedelay */
    } else if (a == 0u) {
        goto nowavechange;
    }
    if (!((p->flags & GT2_PF_WAVECMD) && a >= 0xe0u))
        c->wave = a;
nowavechange: {
    /* the next row a jump: take it now */
    uint8_t cur = tl(s, GT2_WTBL, y);
    uint8_t right = tr(s, GT2_WTBL, y);
    uint32_t next = (uint32_t)y + 1u;
    c->waveptr = tl(s, GT2_WTBL, next) == LOOP ? tr(s, GT2_WTBL, next) : (uint8_t)next;
    if (p->flags & GT2_PF_WAVEDELAY)
        c->wavetime = 0u;
    uint8_t packed = wave_left(p, cur);
    /* the right byte as packed: a command's parameter as it is, a note with its top bit
     * flipped */
    uint8_t is_cmd = cur >= 0xf0u && cur <= 0xfeu;
    if ((p->flags & GT2_PF_WAVECMD) && packed >= 0xe0u) {
        /* mt_execwavecmd: commands 0-4 run their effect now (on any tick), the others
         * their tick 0 part; then mt_done */
        uint8_t cmd = (uint8_t)(packed & 0x0fu);
        uint8_t param = is_cmd ? right : (uint8_t)(right ^ 0x80u);
        if (cmd < 5u) {
            effect(p, c, cmd, param);
        } else {
            /* a waveform read as a command can name a handler the packer left out: its
             * label is then the next handler's */
            while (cmd < 15u && !(p->tick0 & (1u << cmd)))
                cmd++;
            if (cmd < 15u || (p->tick0 & (TEMPO_GLOBAL | TEMPO_CHANNEL)))
                tick0_cmd(o, c, x, cmd, param);
        }
        goto done;
    }
    a = (uint8_t)(right ^ 0x80u);
    if (a == 0u)
        goto wavedone;
    /* mt_wavefreq: a relative note (ADC with the carry clear) or an absolute one */
    if (a & 0x80u)
        a = (uint8_t)((a + c->note) & 0x7fu);
    set_note_freq(p, c, a);
    goto done;
}

wavedone:
    /* the continuous effect, skipped on tick 0 with the realtime optimisation; without
     * pattern commands the instrument's vibrato, from the current instrument */
    if (!((p->flags & GT2_PF_REALTIMEOPT) && c->counter == 0u)) {
        if (p->flags & GT2_PF_NOEFFECTS)
            effect(p, c, 0u, ins_vibparam(s, c->instr));
        else
            effect(p, c, c->fx, c->param);
    }

done:
    /* mt_done: with the pulse optimisation the gate-off timer first, and no pulse on its
     * tick or on a tick 0 that read the orderlist */
    if ((p->flags & GT2_PF_PULSEOPT) && c->counter == c->gatetimer) {
        getnewnote(o, c, x);
        return;
    }
    if (c->pulseptr != 0u &&
        !((p->flags & GT2_PF_PULSEOPT) && c->counter == 0u && c->pattptr == 0u))
        pulse_exec(o, c, x);
    if (!(p->flags & GT2_PF_PULSEOPT) && c->counter == c->gatetimer) {
        getnewnote(o, c, x);
        return;
    }
    loadregs(o, c, x);
}

/* The song's initialisation, the first call after init (mt_play to mt_initchn). */
static void song_init(out* o) {
    gt2_player* p = o->p;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        gt2_channel* c = &p->chn[ch];
        c->songptr = (uint16_t)order_next(order_of(p->song, p->subtune, ch), (uint32_t)-1);
        c->trans = c->repeat = c->pattptr = c->newfx = c->newparam = 0u;
        c->fx = c->param = c->newnote = c->waveptr = c->wave = c->pulseptr = c->pulsetime = 0u;
    }
    if (!(p->flags & GT2_PF_GHOST)) {
        if (p->channels == 2u)
            wr(o, 0x12u, 0u);
        if (p->channels == 1u) {
            wr(o, 0x0bu, 0u);
            wr(o, 0x12u, 0u);
        }
    }
    wr(o, 0x15u, 0u);
    p->filtctrl = 0u;
    p->filtstep = 0u;
    p->initsong = 0u;
    for (uint32_t ch = 0u; ch < p->channels; ch++) {
        gt2_channel* c = &p->chn[ch];
        c->tempo = p->default_tempo;
        c->counter = 1u;
        c->instr = p->reset_instr;
        /* mt_loadregswaveonly; altplayer.s's mt_loadregswave writes the whole channel */
        if ((p->flags & GT2_PF_ALT) && !(p->flags & GT2_PF_GHOST))
            loadregs(o, c, ch * 7u);
        else
            loadregs_wave(o, c, ch * 7u);
    }
}

/* The filter, every call before the channels (mt_filtstep to mt_masterfader). */
static uint8_t filt_left(const gt2_song* s, uint32_t pos) {
    return tl(s, GT2_FTBL, pos);
}

static void filter_exec(out* o) {
    gt2_player* p = o->p;
    const gt2_song* s = p->song;
    if (!(p->flags & GT2_PF_NOFILTER)) {
        uint32_t y = p->filtstep;
        if (y == 0u)
            goto filtdone;
        if (p->filttime != 0u)
            goto filtmod;
        {
            uint8_t l = filt_left(s, y); /* mt_newfiltstep */
            if (l == 0u)
                goto setcutoff;
            if (!(l & 0x80u)) {
                p->filttime = l; /* mt_newfiltmod */
                goto filtmod;
            }
            /* mt_setfilt: the passband (the packer keeps bits 4-6), resonance and
             * channels; a cutoff row right below runs on the same call */
            p->filttype = (uint8_t)(l & 0x70u);
            p->filtctrl = tr(s, GT2_FTBL, y);
            if (filt_left(s, y + 1u) != 0u)
                goto nextstep;
            y++; /* mt_setcutoff2 */
        }
    setcutoff:
        p->filtcutoff = tr(s, GT2_FTBL, y);
        goto nextstep;
    filtmod:
        p->filtcutoff = (uint8_t)(p->filtcutoff + tr(s, GT2_FTBL, y));
        if (--p->filttime != 0u)
            goto filtdone;
    nextstep:
        /* mt_nextfiltstep: the next row a jump: take it now */
        p->filtstep = filt_left(s, y + 1u) == LOOP ? tr(s, GT2_FTBL, y + 1u) : (uint8_t)(y + 1u);
    filtdone:
        wr(o, 0x16u, p->filtcutoff);
    }
    wr(o, 0x17u, p->filtctrl);
    wr(o, 0x18u, (uint8_t)(p->filttype | p->masterfader));
}

/* The main program's copy of the ghost registers (readme 5.1): player.s's full buffering
 * copies the last call's registers $18 down to 0 before it plays (altplayer.s 0 up to $18);
 * with zero-page ghost registers the main program copies them after the call, as the
 * reference harness does (gt2-ref's trace.rs: $18 down to 0, or for altplayer.s the
 * filter's four, then each channel's wave, frequency, pulse, AD and SR). */
GT2_RAM_DATA static const uint8_t ghost_alt[GT2_SID_REGS] = {0x15, 0x16, 0x17, 0x18, 0x04, 0x00, 0x01,
                                                0x02, 0x03, 0x05, 0x06, 0x0b, 0x07, 0x08,
                                                0x09, 0x0a, 0x0c, 0x0d, 0x12, 0x0e, 0x0f,
                                                0x10, 0x11, 0x13, 0x14};

static void ghost_copy(out* o, int forward) {
    for (uint32_t i = 0u; i < GT2_SID_REGS; i++) {
        uint32_t r = forward ? i : GT2_SID_REGS - 1u - i;
        o->sink(o->ctx, (uint8_t)r, o->p->regs[r]);
    }
}

/* The call's gate edges and states, from the wave registers as written. */
static void gate_events(gt2_player* p) {
    for (uint32_t ch = 0u; ch < p->channels; ch++) {
        uint8_t g = (uint8_t)(p->regs[ch * 7u + 4u] & 1u);
        uint8_t was = (uint8_t)((p->gates >> ch) & 1u);
        gt2_event* e = &p->ev[ch];
        e->gate = g;
        if (g && !was)
            e->flags |= GT2_EV_GATE_ON;
        if (!g && was)
            e->flags |= GT2_EV_GATE_OFF;
        p->gates = (uint8_t)((p->gates & ~(1u << ch)) | ((uint32_t)g << ch));
    }
}

void gt2_play_frame(gt2_player* p, gt2_sid_fn sink, void* ctx) {
    out o = {p, sink, ctx};
    int alt = (p->flags & GT2_PF_ALT) != 0;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        gt2_event* e = &p->ev[ch];
        e->flags = e->pattern = e->row = e->order_pos = e->note = e->instr = e->gate = 0u;
    }
    if ((p->flags & GT2_PF_GHOST) && !(p->flags & GT2_PF_ZPGHOST))
        ghost_copy(&o, alt);
    if (p->initsong) {
        song_init(&o);
    } else {
        filter_exec(&o);
        for (uint32_t ch = 0u; ch < p->channels; ch++)
            exec_channel(&o, ch);
    }
    if (p->flags & GT2_PF_ZPGHOST) {
        if (alt) {
            for (uint32_t i = 0u; i < GT2_SID_REGS; i++)
                sink(ctx, ghost_alt[i], p->regs[ghost_alt[i]]);
        } else {
            ghost_copy(&o, 0);
        }
    }
    gate_events(p);
}

/* --- The song analysis: what greloc.c's relocator() makes of the song ------------------ */

#define BITS(n) (((n) + 31u) / 32u)

typedef struct {
    uint32_t pattused[BITS(GT2_MAX_PATTERNS)];
    uint32_t instrused[BITS(GT2_MAX_INSTR + 1u)];
    uint32_t tableused[GT2_TABLES][BITS(GT2_TABLE_ROWS + 2u)];
} usage;

static int bit(const uint32_t* b, uint32_t i) {
    return (int)((b[i >> 5] >> (i & 31u)) & 1u);
}

static void set_bit(uint32_t* b, uint32_t i) {
    b[i >> 5] |= 1u << (i & 31u);
}

/* gtable.c's exectable: marks the rows a table program reaches from ptr. */
static void exectable(const gt2_song* s, usage* u, uint32_t t, uint32_t ptr) {
    if (t != GT2_STBL && ptr != 0u && ptr <= GT2_TABLE_ROWS && tl(s, t, ptr) == LOOP)
        return; /* a pointer onto a jump: the packer refuses the song */
    for (;;) {
        if (ptr == 0u)
            break;
        if (t != GT2_STBL && ptr > GT2_TABLE_ROWS)
            break;
        if (bit(u->tableused[t], ptr))
            break;
        set_bit(u->tableused[t], ptr);
        if (t == GT2_STBL)
            break;
        ptr = tl(s, t, ptr) == LOOP ? tr(s, t, ptr) : ptr + 1u;
    }
}

int gt2_play_init(gt2_player* p, const gt2_song* s, uint32_t subtune, const gt2_options* opt) {
    uint8_t* q = (uint8_t*)p;
    for (uint32_t i = 0u; i < sizeof *p; i++)
        q[i] = 0u;
    if (s->channels != GT2_MONO_CHANNELS)
        return GT2_PLAY_ERR_STEREO;
    gt2_options defaults;
    if (opt == 0) {
        gt2_options_default(&defaults);
        opt = &defaults;
    }
    uint32_t flags = opt->player_flags;
    uint32_t mult = opt->multispeed;
    int noopt = (flags & GT2_PLAYER_NOOPTIMIZATION) != 0;
    int sfx = (flags & GT2_PLAYER_SOUNDEFFECTS) != 0;
    int ghost = (flags & (GT2_PLAYER_ZPGHOSTREGS | GT2_PLAYER_FULLBUFFERED)) != 0;
    int buffered = ghost || sfx || (flags & GT2_PLAYER_BUFFERED);

    usage u;
    uint8_t* z = (uint8_t*)&u;
    for (uint32_t i = 0u; i < sizeof u; i++)
        z[i] = 0u;

    /* The orderlists of the subtunes with three non-empty channels (greloc.c, "Process
     * song-orderlists"). */
    uint32_t songs = 0u, transdown = 0u, transup = 0u;
    int notrans = 1, norepeat = 1;
    int chnused[GT2_PLAY_CHANNELS] = {0, 0, 0};
    for (uint32_t sub = 0u; sub < GT2_MAX_SUBTUNES; sub++) {
        if (!packed_subtune(s, sub))
            continue;
        for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
            const uint8_t* e = arena(s) + GT2_SPAN_OFF4(s->order[sub][ch]) * GT2_UNIT;
            for (uint32_t i = 0u; i < entries(s, sub, ch); i++) {
                uint8_t b = e[i];
                if (b < REPEAT) {
                    uint32_t rows;
                    const uint8_t* pat = pattern(s, b, &rows);
                    set_bit(u.pattused, b);
                    for (uint32_t r = 0u; pat != 0 && r < rows; r++) {
                        const uint8_t* row = pat + r * GT2_ROW_BYTES;
                        if (row[0] != REST || row[1] != 0u || row[2] != 0u)
                            chnused[ch] = 1;
                    }
                } else if (b < TRANSDOWN) {
                    norepeat = 0;
                } else {
                    notrans = 0;
                    if (b < TRANS) {
                        if (TRANS - b > transdown)
                            transdown = TRANS - b;
                    } else if (b - TRANS > transup) {
                        transup = b - TRANS;
                    }
                }
            }
        }
        songs++;
    }
    if (songs == 0u)
        return GT2_PLAY_ERR_SUBTUNE;
    /* With one song the packed init ignores the subtune; with more it is the song's index
     * among the first `songs` subtunes (greloc.c packs subtunes 0 to songs - 1). */
    uint32_t sub = songs == 1u ? 0u : subtune;
    if (sub >= songs || !packed_subtune(s, sub))
        return GT2_PLAY_ERR_SUBTUNE;

    uint32_t channels = 3u;
    if (!chnused[2])
        channels = 2u;
    if (!chnused[1] && !chnused[2])
        channels = 1u;

    /* The patterns: instruments, commands, notes (greloc.c, "Build the pattern-mapping"). */
    int noeffects = 1, nofilter = 1, nocalcspeed = 1;
    /* The tick 0 handlers the packer assembles (NOSETAD and the others): bit 5-14 for
     * commands 5-E, and the tempo command's two parts. */
    uint32_t tick0 = 0u;
    int firstnote = (int)NOTES - 1, lastnote = 0, patternlastnote = 0;
    set_bit(u.instrused, 1u);
    for (uint32_t pat = 0u; pat < GT2_MAX_PATTERNS; pat++) {
        if (!bit(u.pattused, pat))
            continue;
        uint32_t rows;
        const uint8_t* d = pattern(s, (uint8_t)pat, &rows);
        for (uint32_t r = 0u; d != 0 && r < rows; r++) {
            const uint8_t* row = d + r * GT2_ROW_BYTES;
            uint8_t cmd = row[2], data = row[3];
            if (row[1] != 0u && row[1] <= GT2_MAX_INSTR)
                set_bit(u.instrused, row[1]);
            if (cmd != 0u)
                noeffects = 0;
            if (cmd >= 8u && cmd <= 10u)
                exectable(s, &u, cmd - 8u, data);
            if ((cmd >= 1u && cmd <= 4u) || cmd == 14u)
                exectable(s, &u, GT2_STBL, data);
            if (cmd >= 1u && cmd <= 4u && data != 0u && tl(s, GT2_STBL, data) >= 0x80u)
                nocalcspeed = 0;
            if (cmd >= 10u && cmd <= 12u)
                nofilter = 0; /* packpattern */
            if (cmd >= 5u && cmd <= 13u)
                tick0 |= 1u << cmd;
            if (cmd == 14u)
                tick0 |= 1u << 14 | TEMPO_GLOBAL;
            if (cmd == 15u) {
                if ((data & 0x7fu) < 3u)
                    tick0 |= 1u << 14;
                tick0 |= (data & 0x80u) ? TEMPO_CHANNEL : TEMPO_GLOBAL;
            }
            if (row[0] >= NOTE && row[0] <= 0xbcu) {
                int nf = (int)row[0] - (int)NOTE - (int)transdown;
                int nl = (int)row[0] - (int)NOTE + (int)transup;
                if (nf < 0)
                    nf = 0;
                if (nl > (int)NOTES - 1)
                    nl = (int)NOTES - 1;
                if (nf < firstnote)
                    firstnote = nf;
                if (nl > lastnote)
                    patternlastnote = lastnote = nl;
                if (nf > lastnote)
                    patternlastnote = lastnote = nf;
            }
        }
    }

    /* The instruments in use: their classes, in the packer's order hard restart, no hard
     * restart, legato; the table programs they start. */
    uint32_t count[3] = {0u, 0u, 0u};
    uint8_t lowest[3] = {0u, 0u, 0u};
    int fixedparams = 1, noinsvib = 1;
    for (uint32_t i = 1u; i <= GT2_MAX_INSTR; i++) {
        if (!bit(u.instrused, i))
            continue;
        const uint8_t* ins = s->instr[i - 1u];
        int cls = class_of(ins[GT2_INS_GATETIMER]);
        if (count[cls]++ == 0u)
            lowest[cls] = (uint8_t)i;
        for (uint32_t t = 0u; t < GT2_TABLES; t++)
            exectable(s, &u, t, ins[GT2_INS_WAVE + t]);
        if (ins[GT2_INS_VIBPARAM] != 0u && tl(s, GT2_STBL, ins[GT2_INS_VIBPARAM]) >= 0x80u)
            nocalcspeed = 0;
        if (ins[GT2_INS_FILTER] != 0u)
            nofilter = 0;
        if (ins[GT2_INS_VIBPARAM] != 0u)
            noinsvib = 0;
        /* FIXEDPARAMS: every instrument the gate-off timer and first wave of instrument 1 */
        const uint8_t* one = s->instr[0];
        uint8_t fw = ins[GT2_INS_FIRSTWAVE];
        if (ins[GT2_INS_GATETIMER] != one[GT2_INS_GATETIMER] ||
            fw != one[GT2_INS_FIRSTWAVE] || fw == 0u || fw >= 0xfeu)
            fixedparams = 0;
    }
    if (mult > 1u)
        fixedparams = 0;
    /* The tables the wavetable's commands start. */
    for (uint32_t r = 1u; r <= GT2_TABLE_ROWS; r++) {
        uint8_t l = tl(s, GT2_WTBL, r), d = tr(s, GT2_WTBL, r);
        if (!bit(u.tableused[GT2_WTBL], r) || l < 0xf0u || l > 0xfeu)
            continue;
        uint8_t cmd = (uint8_t)(l - 0xf0u);
        if (cmd >= 1u && cmd <= 4u) {
            exectable(s, &u, GT2_STBL, d);
            if (d != 0u && tl(s, GT2_STBL, d) >= 0x80u)
                nocalcspeed = 0;
        } else if (cmd == 9u) {
            exectable(s, &u, GT2_PTBL, d);
        } else if (cmd == 10u) {
            exectable(s, &u, GT2_FTBL, d);
            nofilter = 0;
        }
    }

    /* The table rows in use ("Process tables"). */
    int wavedelay = 0, wavecmd = 0, simplepulse = 1;
    for (uint32_t r = 1u; r <= GT2_TABLE_ROWS; r++) {
        if (!bit(u.tableused[GT2_WTBL], r))
            continue;
        uint8_t l = tl(s, GT2_WTBL, r), d = tr(s, GT2_WTBL, r);
        if (l >= 0x01u && l <= 0x0fu)
            wavedelay = 1;
        if (l >= 0xf0u && l <= 0xfeu) {
            wavecmd = 1;
            if (l >= 0xf5u)
                tick0 |= 1u << (l - 0xf0u);
        }
        if (l < 0xf0u) {
            if (d <= 0x80u) {
                int nl = (int)d + patternlastnote;
                if (nl > (int)NOTES - 1)
                    nl = (int)NOTES - 1;
                if (d >= 0x20u)
                    firstnote = 0;
                if (nl > lastnote)
                    lastnote = nl;
            } else {
                int n = d & 0x7f;
                int nl = n > (int)NOTES - 1 ? (int)NOTES - 1 : n;
                if (n < firstnote)
                    firstnote = n;
                if (nl > lastnote)
                    lastnote = nl;
            }
        }
    }
    for (uint32_t r = 1u; r <= GT2_TABLE_ROWS; r++) {
        if (!bit(u.tableused[GT2_PTBL], r))
            continue;
        uint8_t l = tl(s, GT2_PTBL, r), d = tr(s, GT2_PTBL, r);
        if (l != 0xffu && (d & 0x0fu))
            simplepulse = 0;
    }
    if (wavecmd)
        noeffects = 0;
    if (noopt) {
        notrans = norepeat = fixedparams = noinsvib = nocalcspeed = 0;
        tick0 = 0x7fe0u | TEMPO_GLOBAL | TEMPO_CHANNEL;
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

    p->song = s;
    if (opt->a4_millihz != 0u)
        p->warnings |= GT2_PLAY_WARN_PITCH;
    p->subtune = (uint8_t)sub;
    p->songs = (uint8_t)songs;
    for (uint32_t i = 0u; i < BITS(GT2_MAX_PATTERNS); i++)
        p->pattused[i] = u.pattused[i];
    for (uint32_t i = 0u; i < BITS(GT2_MAX_INSTR + 1u); i++)
        p->instrused[i] = u.instrused[i];
    for (uint32_t k = 0u; k < 3u; k++)
        p->class_count[k] = (uint8_t)count[k];
    /* How player.s lays out its variables, for reads below the frequency table. */
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
    p->layout = (uint8_t)layout;
    p->tick0 = tick0;
    p->channels = (uint8_t)channels;
    uint32_t f = 0u;
    if (buffered)
        f |= GT2_PF_BUFFERED;
    if (ghost)
        f |= GT2_PF_GHOST;
    if (flags & GT2_PLAYER_ZPGHOSTREGS)
        f |= GT2_PF_ZPGHOST;
    if (opt->hr_adsr >= 0xf000u)
        f |= GT2_PF_ALT;
    if (opt->optimize_pulse)
        f |= GT2_PF_PULSEOPT;
    if (opt->optimize_realtime)
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
    p->flags = (uint16_t)f;
    p->ad_param = (uint8_t)(opt->hr_adsr >> 8);
    p->sr_param = (uint8_t)opt->hr_adsr;
    /* The packer numbers the instruments hard restart first, then no hard restart, then
     * legato; each channel starts on its number 1. Multispeed adds one to the no-HR count
     * ("for multispeed stability"), which moves the legato class's start past its first
     * instrument: that one plays as no-HR. */
    p->reset_instr = count[CLASS_HR] ? lowest[CLASS_HR]
                     : count[CLASS_NOHR] ? lowest[CLASS_NOHR]
                                         : lowest[CLASS_LEGATO];
    p->first_legato = mult > 1u ? lowest[CLASS_LEGATO] : 0u;
    /* DEFAULTTEMPO: instrument 63's AD less one if it is 2 or more and the instrument has no
     * wavetable (readme 3.6), else 6 ticks a row times the multispeed. */
    const uint8_t* last = s->instr[GT2_MAX_INSTR - 1u];
    if (last[GT2_INS_AD] >= 2u && last[GT2_INS_WAVE] == 0u)
        p->default_tempo = (uint8_t)(last[GT2_INS_AD] - 1u);
    else
        p->default_tempo = (uint8_t)(mult ? mult * 6u - 1u : 5u);
    p->first_note = (uint8_t)firstnote;
    p->last_note = (uint8_t)lastnote;
    p->initsong = 1u;
    p->masterfader = 0x0fu;
    p->funktempo[0] = 8u;
    p->funktempo[1] = 5u;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        p->chn[ch].instr = 1u;
        p->chn[ch].gate = 0xfeu;
        /* mt_chnpattnum starts at 0, the packer's first pattern in use: an orderlist that
         * opens with a repeat (readme 3.1's rules broken) replays it */
        p->chn[ch].pattnum = packed_pattern(p, 0u);
        p->chn[ch].orderpos = 0xffu;
    }
    return 0;
}
