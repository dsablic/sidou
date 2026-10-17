// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song player's frame path (gt2_play.h). It follows GoatTracker 2.77's src/player.s
 * (and altplayer.s) routine for routine, its labels named in the comments, and takes its
 * data: the frequency table (src/gplay.c's freqtbllo/freqtblhi). gplay.c is
 * GPL-2.0-or-later, whose "any later version" lets this file stay GPL-3.0-or-later;
 * player.s carries its own permissive notice. Where a 6502 flag decides a branch, the C
 * keeps the flag (the carry, mostly) as a variable. The packer's analysis is gt2_shape.c's
 * (the main loop's, in flash on the device); this file runs in the audio interrupt, from
 * SRAM, and reads nothing it would have to search. */
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
 * player.s steps through its packed orderlist bytes with an 8-bit index; the shape holds
 * each channel's 256 bytes from its start (gt2_shape.h builds them: a repeat after its
 * pattern, R0 dropped, the $FF and the restart byte, then the next orderlists the packer
 * wrote), so the sequencer reads one byte a step, whatever the orderlist. A byte marked raw
 * (a restart byte, or past every orderlist) is the packed player's: as a pattern number it
 * is the packer's numbering, which the shape maps back to the model's pattern. */

/* The model pattern the packer numbers q, $FF past the packed patterns (an empty one). A
 * search of the numbering: only a raw orderlist byte (a broken orderlist's) and a start take
 * it, and it is bounded (208 steps), so the shape keeps one table, not two (RAM). */
static uint8_t pattern_of(const gt2_shape* sh, uint32_t q) {
    for (uint32_t pat = 0u; pat < GT2_MAX_PATTERNS; pat++)
        if (sh->pat_num[pat] == q)
            return (uint8_t)pat;
    return 0xffu;
}

static int is_raw(const gt2_shape* sh, uint32_t ch, uint32_t y) {
    return sh->ord_pos[ch][y] == GT2_ORD_RAW;
}

/* --- Frequencies ----------------------------------------------------------------------
 * The packed table holds notes first_note to last_note (greloc.c), lows then highs:
 * player.s reads mt_freqtbllo-FIRSTNOTE+y. A note outside the table reads the 6502's
 * memory around it, and the player reads what it can know of that memory:
 * - past the lows, the highs (a note past the table reads its low byte there);
 * - past the highs, the packed song table, whose bytes are addresses that depend on the
 *   assembled player's size: no player without the packed layout knows them;
 * - below the lows, the end of the player's own variables (player.s's mt_chn* blocks, as
 *   the song's assembly lays them out), most of which the player holds: the shape's var
 *   table says which variable each byte is (gt2_shape.c lays it out).
 * A byte it cannot know reads 0, as GoatTracker's editor reads past B-7, and marks the
 * read in `unknown`. */

/* The byte `back` bytes below the frequency table (1: the last before it), or -1. */
static int var_byte(const gt2_player* p, uint32_t back) {
    const gt2_shape* sh = p->shape;
    if (back > sh->var_count)
        return -1;
    uint8_t v = sh->var[back - 1u];
    if (v == GT2_VAR_ZERO)
        return 0;
    if (v == GT2_VAR_UNKNOWN)
        return -1;
    if (v & GT2_VAR_GHOST)
        return p->regs[v & 0x1fu];
    uint32_t ch = (uint32_t)v >> 5;
    const gt2_channel* c = &p->chn[ch];
    switch (v & 0x1fu) {
    case GT2_VF_ORDIDX:
        return (int)(sh->songs > 1u ? sh->subtune * 3u + ch : ch) & 0xff;
    case GT2_VF_PATTNUM:
        return c->pattnum < GT2_MAX_PATTERNS && sh->pat_num[c->pattnum] != 0xffu
                   ? sh->pat_num[c->pattnum]
                   : -1;
    case GT2_VF_INSTR:
        return c->instr <= GT2_MAX_INSTR && sh->ins_num[c->instr] != 0u ? sh->ins_num[c->instr]
                                                                         : -1;
    case GT2_VF_TEMPO:
        return c->tempo;
    case GT2_VF_COUNTER:
        return c->counter;
    case GT2_VF_NOTE:
        return c->note;
    case GT2_VF_GATE:
        return c->gate;
    case GT2_VF_VIBTIME:
        return c->vibtime;
    case GT2_VF_VIBDELAY:
        return c->vibdelay;
    case GT2_VF_WAVETIME:
        return c->wavetime;
    case GT2_VF_FREQLO:
        return c->freqlo;
    case GT2_VF_FREQHI:
        return c->freqhi;
    case GT2_VF_PULSELO:
        return c->pulselo;
    case GT2_VF_PULSEHI:
        return c->pulsehi;
    case GT2_VF_AD:
        return c->ad;
    case GT2_VF_SR:
        return c->sr;
    case GT2_VF_GATETIMER:
        return c->gatetimer;
    case GT2_VF_LASTNOTE:
        return c->lastnote;
    default:
        return c->pulsetime;
    }
}

static uint8_t freq_mem(gt2_player* p, int32_t off) {
    const gt2_shape* sh = p->shape;
    int32_t count = (int32_t)sh->last_note - sh->first_note + 1;
    int v = -1;
    if (off < 0)
        v = var_byte(p, (uint32_t)-off);
    else if (off < count)
        v = freq_lo[sh->first_note + off];
    else if (off < 2 * count)
        v = freq_hi[sh->first_note + off - count];
    if (v < 0) {
        p->unknown = 1u;
        return 0u;
    }
    return (uint8_t)v;
}

static uint8_t freq_byte(gt2_player* p, uint32_t note, int high) {
    const gt2_shape* sh = p->shape;
    int32_t count = (int32_t)sh->last_note - sh->first_note + 1;
    return freq_mem(p, (int32_t)note - sh->first_note + (high ? count : 0));
}

/* --- Writes -------------------------------------------------------------------------- */

static void wr(out* o, uint32_t reg, uint8_t v) {
    o->p->regs[reg] = v;
    if (!(o->p->shape->flags & GT2_PF_GHOST))
        o->sink(o->ctx, (uint8_t)reg, v);
}

/* A write player.s makes at once unbuffered, and keeps in the channel's variable when
 * buffered (mt_chnad and the others), for mt_loadregs. */
static int unbuffered(const out* o) {
    return !(o->p->shape->flags & GT2_PF_BUFFERED);
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
    uint8_t ph = (o->p->shape->flags & GT2_PF_SIMPLEPULSE) ? c->pulselo : c->pulsehi;
    uint8_t w = (uint8_t)(c->wave & c->gate);
    if (o->p->shape->flags & GT2_PF_ALT) {
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
    } else if (o->p->shape->flags & GT2_PF_ALT) {
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
        if ((p->shape->flags & GT2_PF_AUTHORINFO) && a >= 0x10u)
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
        if ((a & 0x80u) && (p->shape->tick0 & GT2_TICK0_TEMPO_CHANNEL))
            c->tempo = (uint8_t)(a & 0x7fu);
        else if (p->shape->tick0 & GT2_TICK0_TEMPO_GLOBAL)
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
    if ((g & 0x40u) && i != p->shape->first_legato)
        return CLASS_LEGATO;
    return (g & 0xc0u) ? CLASS_NOHR : CLASS_HR;
}

/* The wavetable's left byte as the packer writes it (greloc.c, "In wavetable, convert
 * waveform values for the playroutine"): the inaudible waveforms $E0-$EF become $00-$0F,
 * and when the song uses delays every waveform moves up by $10, past them. */
static uint8_t wave_left(const gt2_player* p, uint8_t l) {
    uint8_t w = (l >= 0xe0u && l <= 0xefu) ? (uint8_t)(l & 0x0fu) : l;
    if ((p->shape->flags & GT2_PF_WAVEDELAY) && l > 0x0fu && l <= 0xefu)
        w = (uint8_t)(w + 0x10u);
    return w;
}

/* The sequencer (mt_sequencer to mt_repeatdone2), at tick 0 when a pattern is due: y is
 * player.s's 8-bit index into the channel's packed orderlist, so positions wrap past 255. */
static void sequencer(gt2_player* p, gt2_channel* c, uint32_t ch) {
    const gt2_shape* sh = p->shape;
    const uint8_t* ord = sh->ord[ch];
    uint8_t y = c->songptr;
    uint8_t a = ord[y];
    if (a == LOOP) {
        y = ord[(uint8_t)(y + 1u)];
        a = ord[y];
    }
    if (a >= TRANSDOWN) {
        c->trans = (uint8_t)(a - TRANS);
        y++;
        a = ord[y];
    }
    if (a >= REPEAT) {
        /* mt_repeat: the pattern before plays again until the count is reached. */
        c->repeat++;
        if ((uint8_t)(a - REPEAT) != c->repeat)
            return;
        c->repeat = 0u;
    } else {
        /* a raw byte past the packed pattern table names an empty pattern ($FF) */
        if (!is_raw(sh, ch, y))
            c->pattnum = a;
        else
            c->pattnum = pattern_of(sh, a);
        c->orderpos = is_raw(sh, ch, y) ? 0xffu : sh->ord_pos[ch][y];
    }
    c->songptr = (uint8_t)(y + 1u);
}

/* The pulsetable's byte as packed: with simple pulse a width's high nybble goes into the
 * low byte's low nybble (the one byte is written to both registers), and a modulation
 * speed becomes the same rotated step (greloc.c, "Write table right side"). */
static uint8_t pulse_right(const gt2_player* p, uint32_t pos) {
    const gt2_song* s = p->song;
    uint8_t l = tl(s, GT2_PTBL, pos), r = tr(s, GT2_PTBL, pos);
    if (!(p->shape->flags & GT2_PF_SIMPLEPULSE) || l == LOOP)
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
    int simple = (p->shape->flags & GT2_PF_SIMPLEPULSE) != 0;
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

/* mt_normalnote's hard restart: a new note (transposed) starts at the next tick 0. */
static void note_start(out* o, gt2_channel* c, uint32_t x, uint8_t newnote) {
    gt2_player* p = o->p;
    c->newnote = newnote;
    if (c->newfx == TONEPORTA)
        return;
    int cls = instr_class(p, c->instr);
    if (cls == CLASS_LEGATO)
        return;
    if (cls == CLASS_HR) {
        if (p->shape->flags & GT2_PF_ALT) {
            set_ad(o, c, x, p->shape->ad_param);
            set_sr(o, c, x, p->shape->sr_param);
        } else {
            set_sr(o, c, x, p->shape->sr_param);
            set_ad(o, c, x, p->shape->ad_param);
        }
    }
    c->gate = 0xfeu; /* mt_skiphr */
}

/* mt_getnewnote: the next row, at the gate-off timer's tick. */
static void getnewnote(out* o, gt2_channel* c, uint32_t x) {
    gt2_player* p = o->p;
    if (p->frozen) {
        /* an empty row, read from nowhere: no note, instrument or command */
        if (!(p->shape->flags & GT2_PF_NOEFFECTS))
            c->newfx = 0u;
        loadregs(o, c, x);
        return;
    }
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
    if (!(p->shape->flags & GT2_PF_NOEFFECTS)) {
        /* mt_fx, as packpattern writes the command: a timing mark erased without author
         * info, a tempo less one; command 0 carries no parameter */
        uint8_t cmd = row[2], data = row[3];
        if (cmd == 13u && data > 0x0fu && !(p->shape->flags & GT2_PF_AUTHORINFO))
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
        note_start(o, c, x, (uint8_t)(note + c->trans));
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
    if (c->pattptr == 0u && !p->frozen)
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
        if ((p->shape->flags & GT2_PF_ALT) && unbuffered(o))
            loadregs_wave(o, c, x);
        if (ins[GT2_INS_PULSE] != 0u) {
            c->pulseptr = ins[GT2_INS_PULSE];
            c->pulsetime = 0u;
        }
        if (ins[GT2_INS_FILTER] != 0u && !(p->shape->flags & GT2_PF_NOFILTER)) {
            p->filtstep = ins[GT2_INS_FILTER];
            p->filttime = 0u;
        }
        c->waveptr = ins[GT2_INS_WAVE];
        if (p->shape->flags & GT2_PF_ALT) {
            set_ad(o, c, x, ins[GT2_INS_AD]);
            set_sr(o, c, x, ins[GT2_INS_SR]);
        } else {
            set_sr(o, c, x, ins[GT2_INS_SR]);
            set_ad(o, c, x, ins[GT2_INS_AD]);
        }
        /* the command's tick 0 part, then the registers (altplayer.s unbuffered: none) */
        if (!(p->shape->flags & GT2_PF_NOEFFECTS))
            tick0_cmd(o, c, x, c->newfx, c->newparam);
        if (!unbuffered(o))
            loadregs(o, c, x);
        else if (!(p->shape->flags & GT2_PF_ALT))
            loadregs_wave(o, c, x);
        return;
    }
nonewnoteinit:
    /* mt_nonewnoteinit: the command's tick 0 part, then the wavetable */
    if (!(p->shape->flags & GT2_PF_NOEFFECTS))
        tick0_cmd(o, c, x, c->newfx, c->newparam);

waveexec:
    /* mt_waveexec */
    y = c->waveptr;
    if (y == 0u)
        goto wavedone;
    a = wave_left(p, tl(s, GT2_WTBL, y));
    if (p->shape->flags & GT2_PF_WAVEDELAY) {
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
    if (!((p->shape->flags & GT2_PF_WAVECMD) && a >= 0xe0u))
        c->wave = a;
nowavechange: {
    /* the next row a jump: take it now */
    uint8_t cur = tl(s, GT2_WTBL, y);
    uint8_t right = tr(s, GT2_WTBL, y);
    uint32_t next = (uint32_t)y + 1u;
    c->waveptr = tl(s, GT2_WTBL, next) == LOOP ? tr(s, GT2_WTBL, next) : (uint8_t)next;
    if (p->shape->flags & GT2_PF_WAVEDELAY)
        c->wavetime = 0u;
    uint8_t packed = wave_left(p, cur);
    /* the right byte as packed: a command's parameter as it is, a note with its top bit
     * flipped */
    uint8_t is_cmd = cur >= 0xf0u && cur <= 0xfeu;
    if ((p->shape->flags & GT2_PF_WAVECMD) && packed >= 0xe0u) {
        /* mt_execwavecmd: commands 0-4 run their effect now (on any tick), the others
         * their tick 0 part; then mt_done */
        uint8_t cmd = (uint8_t)(packed & 0x0fu);
        uint8_t param = is_cmd ? right : (uint8_t)(right ^ 0x80u);
        if (cmd < 5u) {
            effect(p, c, cmd, param);
        } else {
            /* a waveform read as a command can name a handler the packer left out: its
             * label is then the next handler's */
            while (cmd < 15u && !(p->shape->tick0 & (1u << cmd)))
                cmd++;
            if (cmd < 15u || (p->shape->tick0 & (GT2_TICK0_TEMPO_GLOBAL | GT2_TICK0_TEMPO_CHANNEL)))
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
    if (!((p->shape->flags & GT2_PF_REALTIMEOPT) && c->counter == 0u)) {
        if (p->shape->flags & GT2_PF_NOEFFECTS)
            effect(p, c, 0u, ins_vibparam(s, c->instr));
        else
            effect(p, c, c->fx, c->param);
    }

done:
    /* mt_done: with the pulse optimisation the gate-off timer first, and no pulse on its
     * tick or on a tick 0 that read the orderlist */
    if ((p->shape->flags & GT2_PF_PULSEOPT) && c->counter == c->gatetimer) {
        getnewnote(o, c, x);
        return;
    }
    if (c->pulseptr != 0u &&
        !((p->shape->flags & GT2_PF_PULSEOPT) && c->counter == 0u && c->pattptr == 0u))
        pulse_exec(o, c, x);
    if (!(p->shape->flags & GT2_PF_PULSEOPT) && c->counter == c->gatetimer) {
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
        c->songptr = p->start[ch];
        c->trans = c->repeat = c->pattptr = c->newfx = c->newparam = 0u;
        c->fx = c->param = c->newnote = c->waveptr = c->wave = c->pulseptr = c->pulsetime = 0u;
    }
    if (!(p->shape->flags & GT2_PF_GHOST)) {
        if (p->shape->channels == 2u)
            wr(o, 0x12u, 0u);
        if (p->shape->channels == 1u) {
            wr(o, 0x0bu, 0u);
            wr(o, 0x12u, 0u);
        }
    }
    wr(o, 0x15u, 0u);
    p->filtctrl = 0u;
    p->filtstep = 0u;
    p->initsong = 0u;
    for (uint32_t ch = 0u; ch < p->shape->channels; ch++) {
        gt2_channel* c = &p->chn[ch];
        c->tempo = p->shape->default_tempo;
        c->counter = 1u;
        c->instr = p->shape->reset_instr;
        /* mt_loadregswaveonly; altplayer.s's mt_loadregswave writes the whole channel */
        if ((p->shape->flags & GT2_PF_ALT) && !(p->shape->flags & GT2_PF_GHOST))
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
    if (!(p->shape->flags & GT2_PF_NOFILTER)) {
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
    for (uint32_t ch = 0u; ch < p->shape->channels; ch++) {
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
    int alt = (p->shape->flags & GT2_PF_ALT) != 0;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        gt2_event* e = &p->ev[ch];
        e->flags = e->pattern = e->row = e->order_pos = e->note = e->instr = e->gate = 0u;
    }
    if ((p->shape->flags & GT2_PF_GHOST) && !(p->shape->flags & GT2_PF_ZPGHOST))
        ghost_copy(&o, alt);
    if (p->initsong) {
        song_init(&o);
    } else {
        filter_exec(&o);
        for (uint32_t ch = 0u; ch < p->shape->channels; ch++)
            exec_channel(&o, ch);
    }
    if (p->shape->flags & GT2_PF_ZPGHOST) {
        if (alt) {
            for (uint32_t i = 0u; i < GT2_SID_REGS; i++)
                sink(ctx, ghost_alt[i], p->regs[ghost_alt[i]]);
        } else {
            ghost_copy(&o, 0);
        }
    }
    gate_events(p);
}

void gt2_play_start(gt2_player* p, const gt2_song* s, const gt2_shape* shape,
                    const uint8_t* start) {
    /* A loop, not a struct assignment: a compiler may make that a memset call, which the
     * audio interrupt must not reach (it is in flash on the device). */
    volatile uint8_t* q = (volatile uint8_t*)p;
    for (uint32_t i = 0u; i < sizeof *p; i++)
        q[i] = 0u;
    p->song = s;
    p->shape = shape;
    p->initsong = 1u;
    p->masterfader = 0x0fu;
    p->funktempo[0] = 8u;
    p->funktempo[1] = 5u;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        p->start[ch] = start ? start[ch] : 0u;
        p->chn[ch].instr = 1u;
        p->chn[ch].gate = 0xfeu;
        /* mt_chnpattnum starts at 0, the packer's first pattern in use: an orderlist that
         * opens with a repeat (readme 3.1's rules broken) replays it */
        p->chn[ch].pattnum = pattern_of(shape, 0u);
        p->chn[ch].orderpos = 0xffu;
    }
}

void gt2_play_set_shape(gt2_player* p, const gt2_shape* shape) {
    p->shape = shape;
}

void gt2_play_jam(gt2_player* p, uint32_t ch, uint8_t note, uint8_t instr, gt2_sid_fn sink,
                  void* ctx) {
    out o = {p, sink, ctx};
    gt2_channel* c = &p->chn[ch];
    if (instr != 0u)
        c->instr = instr;
    c->newfx = 0u;
    note_start(&o, c, ch * 7u, (uint8_t)(note + NOTE));
}
