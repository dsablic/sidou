// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_song.h"

#include <string.h>

#include "live.h"
#include "song/song_ext.h"
#include "song_pages.h"

_Static_assert(LAB_SONG_ROWS == SONG_PAGES_ARP_ROWS, "an arpeggio's rows fit a program");

#define JUMP 0xffu
/* GoatTracker's default gate timer: 2 ticks, the hard restart on. */
#define GATE_TIMER 2u
/* A chunk: its version, the rows of each table, the CRC, the sound. */
#define KEPT_VERSION 1u
#define KEPT_HEAD (1u + GT2_TABLES + 4u)

static uint32_t u16_at(const uint8_t* s, uint32_t at) {
    return (uint32_t)s[at] | (uint32_t)s[at + 1u] << 8;
}

static void put16(uint8_t* s, uint32_t at, uint32_t v) {
    s[at] = (uint8_t)v;
    s[at + 1u] = (uint8_t)(v >> 8);
}

static int32_t clampi(int32_t v, int32_t lo, int32_t hi) {
    return v < lo ? lo : v > hi ? hi : v;
}

/* a / b to the nearest, b > 0. */
static int32_t ratio(int32_t a, int32_t b) {
    return (a + b / 2) / b;
}

/* ---- send ---- */

typedef struct {
    lab_song_instr* o;
    uint32_t t, first;
} prog;

static void row(prog* p, uint32_t left, uint32_t right) {
    uint32_t n = p->o->rows[p->t]++;
    p->o->left[p->t][n] = (uint8_t)left;
    p->o->right[p->t][n] = (uint8_t)right;
}

/* A triangle from `top`'s side: a climb of q rows' speed s from where it is to the top, then the
 * sweep of `span` down and up again, round and round. The speed a frame (1..127), the frames of
 * a sweep (two rows at most) and the climb follow the band and the period (frames a cycle). */
static void loop(prog* p, int32_t at, int32_t lo, int32_t hi, int32_t period, int32_t top_max) {
    int32_t span = hi - lo, half = period / 2;
    int32_t s = clampi(ratio(span, half > 0 ? half : 1), 1, 127);
    int32_t q = clampi(ratio(hi - at, s), 0, 127);
    if (at + q * s > top_max)
        q = (top_max - at) / s;
    int32_t top = at + q * s;
    int32_t t = clampi(ratio(span, s), 1, 254);
    if (top - t * s < 0)
        t = top / s;
    if (t < 1)
        return;
    if (q)
        row(p, (uint32_t)q, (uint32_t)s);
    uint32_t down = p->first + p->o->rows[p->t];
    for (int32_t dir = -1; dir <= 1; dir += 2) {
        int32_t left = t;
        while (left > 0) {
            int32_t n = left > 127 ? 127 : left;
            row(p, (uint32_t)n, (uint32_t)(uint8_t)(s * dir));
            left -= n;
        }
    }
    row(p, JUMP, down);
}

/* The place on the model's cutoff walk of c: the last entry at or below it. */
static int32_t place(uint32_t model, uint32_t c) {
    const uint16_t* w = live_cutoff_steps[model & 1u];
    int32_t i = 0;
    while (i + 1 < (int32_t)LIVE_CUTOFF_STEPS && w[i + 1] <= c)
        i++;
    return i;
}

/* 65,536 a cycle each 10 ms tick: the frames (50 Hz) of a cycle. */
static int32_t frames(uint32_t rate) {
    return (int32_t)(32768u / rate);
}

/* GoatTracker's optimisations (on by default) rest the pulse program on each row's gate-off tick
 * and the vibrato on tick 0: one frame in six at tempo 6, the default, which the pulse loop's and
 * the vibrato's frames make up for. */
#define RUNS(f) ((f) * 5 / 6)

static void arp_chord(const uint8_t* sound, uint8_t wave, prog* p) {
    static const int8_t CHORDS[3][6] = {{0, 4, 7, 12}, {12, 7, 4, 0}, {0, 4, 7, 12, 7, 4}};
    const uint8_t* a = sound + LAB_SOUND_ARP;
    uint32_t order = a[LIVE_ARP_ORDER] == LIVE_ARP_DOWN ? 1u : a[LIVE_ARP_ORDER] == LIVE_ARP_UP_DOWN ? 2u : 0u;
    song_pages_arp arp;
    song_pages_arp_default(&arp);
    memcpy(arp.off, CHORDS[order], 6u);
    arp.steps = (uint8_t)(order == 2u ? 6u : 4u);
    arp.speed = (uint8_t)clampi(live_arp_steps[a[LIVE_ARP_RATE] % LIVE_ARP_RATES], 1, SONG_PAGES_ARP_SPEED_MAX);
    arp.wave = wave;
    p->o->rows[GT2_WTBL] = (uint8_t)song_pages_arp_encode(&arp, p->first, p->o->left[GT2_WTBL],
                                                          p->o->right[GT2_WTBL]);
}

void lab_song_encode(const uint8_t* sound, const char* name, uint32_t voice, uint32_t model,
                     const uint8_t first[GT2_TABLES], lab_song_instr* o) {
    memset(o, 0, sizeof *o);
    uint8_t* ins = o->instr;
    prog p = {o, GT2_WTBL, first[GT2_WTBL]};
    uint32_t notes = 0u;
    /* the waveform: the wave table holds $10..$DF; $E0..$EF are the waveforms $00..$0F */
    uint32_t ctrl = sound[LAB_SOUND_CONTROL];
    if (ctrl >= 0xe0u) {
        ctrl &= 0x7fu; /* noise with all three: no wave table row says it */
        notes |= LAB_SONG_WAVE;
    }
    uint8_t wave = (uint8_t)(ctrl | 1u);
    if (sound[LAB_SOUND_ARP + LIVE_ARP_ON]) {
        arp_chord(sound, wave, &p);
        notes |= LAB_SONG_ARP;
    } else {
        row(&p, wave < 0x10u ? wave | 0xe0u : wave, 0u);
        row(&p, JUMP, 0u);
    }
    ins[GT2_INS_WAVE] = (uint8_t)p.first;
    ins[GT2_INS_FIRSTWAVE] = wave;
    ins[GT2_INS_GATETIMER] = GATE_TIMER;
    ins[GT2_INS_AD] = sound[LAB_SOUND_AD];
    ins[GT2_INS_SR] = sound[LAB_SOUND_SR];
    if (!(ins[GT2_INS_AD] & 0xf0u)) {
        ins[GT2_INS_AD] |= 0x10u; /* attack 0 under the hard restart's ADSR clicks (GoatTracker) */
        notes |= LAB_SONG_ATTACK;
    }
    uint32_t rate = u16_at(sound, LAB_SOUND_PWM_RATE);
    /* the pulse: its width, and the PWM as a loop (the link baked, LIVE's room kept) */
    if (ctrl & 0x40u) {
        p = (prog){o, GT2_PTBL, first[GT2_PTBL]};
        int32_t pw = (int32_t)u16_at(sound, LAB_SOUND_PULSE);
        int32_t depth = (int32_t)u16_at(sound, LAB_SOUND_PWM_DEPTH);
        int8_t link = (int8_t)sound[LAB_SOUND_PWM_LINK];
        depth += link * (pw > 2048 ? pw - 2048 : 2048 - pw) / 8;
        int32_t room = pw - LIVE_PULSE_FIRST, top = (int32_t)LIVE_PULSE_WIDTH_MAX + 1 - LIVE_PULSE_FIRST - pw;
        depth = clampi(depth, 0, clampi(room < top ? room : top, 0, LIVE_PWM_DEPTH_MAX));
        row(&p, 0x80u | (uint32_t)pw >> 8, (uint32_t)pw & 0xffu);
        if (depth && rate) {
            loop(&p, pw, pw - depth, pw + depth, RUNS(frames(rate)), (int32_t)LIVE_PULSE_WIDTH_MAX);
            notes |= LAB_SONG_PWM;
        }
        if (p.o->left[GT2_PTBL][o->rows[GT2_PTBL] - 1u] != JUMP)
            row(&p, JUMP, 0u);
        ins[GT2_INS_PULSE] = (uint8_t)p.first;
    }
    /* the filter, on the song's voice: mode, resonance (RESO LINK at the cutoff), cutoff */
    if (sound[LAB_SOUND_FILTER_VOICES]) {
        p = (prog){o, GT2_FTBL, first[GT2_FTBL]};
        int32_t c = (int32_t)u16_at(sound, LAB_SOUND_CUTOFF);
        int32_t at = place(model, (uint32_t)c);
        int32_t res = sound[LAB_SOUND_RESONANCE] + (int8_t)sound[LAB_SOUND_RESO_LINK] * (32 - at) / 8;
        row(&p, 0x80u | sound[LAB_SOUND_FILTER_MODE], (uint32_t)clampi(res, 0, 15) << 4 | 1u << (voice % 3u));
        row(&p, 0u, (uint32_t)c >> 3);
        int32_t d = sound[LAB_SOUND_FILTER_LFO];
        notes |= LAB_SONG_FILTER;
        if (d && rate) {
            const uint16_t* w = live_cutoff_steps[model & 1u];
            int32_t hi = clampi(c + w[clampi(at + d, 0, 64)] - w[at], 0, LIVE_CUTOFF_MAX);
            int32_t lo = clampi(c + w[clampi(at - d, 0, 64)] - w[at], 0, LIVE_CUTOFF_MAX);
            uint8_t n = o->rows[GT2_FTBL];
            loop(&p, c >> 3, lo >> 3, hi >> 3, frames(rate), 255);
            notes |= o->rows[GT2_FTBL] != n ? LAB_SONG_LFO : 0u;
        }
        if (p.o->left[GT2_FTBL][o->rows[GT2_FTBL] - 1u] != JUMP)
            row(&p, JUMP, 0u);
        ins[GT2_INS_FILTER] = (uint8_t)p.first;
    }
    /* the vibrato: a calculated speed, a cycle of 2S + 4 frames that run (50 / (2S + 4) Hz less
     * the tick 0s), peaking near (S + 2) / 2 >> shift semitones; LIVE's peaks at depth / 128 */
    uint32_t vr = sound[LAB_SOUND_VIBRATO_RATE], vd = sound[LAB_SOUND_VIBRATO_DEPTH];
    if (vr && vd) {
        p = (prog){o, GT2_STBL, first[GT2_STBL]};
        int32_t s = clampi(ratio(625, 3 * (int32_t)vr) - 2, 0, 127); /* 250 x 5/6 */
        uint32_t x = (uint32_t)(s + 2) * 64u * 1024u / vd, shift = 0u;
        while (shift < 15u && x >= 1448u << shift) /* 1448: the square root of 2, Q10 */
            shift++;
        row(&p, 0x80u | (uint32_t)s, shift);
        ins[GT2_INS_VIBPARAM] = (uint8_t)p.first;
        /* frames before it; 0 would be none (GoatTracker's), so 1 more */
        ins[GT2_INS_VIBDELAY] = (uint8_t)(1u + sound[LAB_SOUND_VIBRATO_DELAY] / 2u);
        notes |= LAB_SONG_VIB;
    }
    for (uint32_t i = 0u; i < GT2_INSTR_NAME && name && name[i]; i++)
        ins[GT2_INS_NAME + i] = (uint8_t)name[i];
    notes |= sound[LAB_SOUND_MONO_SYNC] ? LAB_SONG_SYNC : 0u;
    notes |= sound[LAB_SOUND_TUNE] ? LAB_SONG_TUNE : 0u;
    notes |= sound[LAB_SOUND_VELOCITY] | sound[LAB_SOUND_MOD] | sound[LAB_SOUND_PRESSURE] ? LAB_SONG_PLAY : 0u;
    for (uint32_t k = 0u; k < LIVE_SLOTS; k++)
        notes |= sound[LAB_SOUND_SLOTS + k * LIVE_SLOT_FIELDS] ? LAB_SONG_FX : 0u;
    o->notes = (uint16_t)notes;
}

/* ---- the chunk ---- */

uint32_t lab_song_tag(uint32_t instr) {
    static const char HEX[] = "0123456789ABCDEF";
    return SONG_TAG('L', 'B', HEX[instr >> 4 & 15u], HEX[instr & 15u]);
}

static const uint8_t PTR[GT2_TABLES] = {GT2_INS_WAVE, GT2_INS_PULSE, GT2_INS_FILTER, GT2_INS_VIBPARAM};

/* The CRC of an instrument's bytes and its programs' rows, read through get. */
static uint32_t crc_rows(const uint8_t* ins, const uint8_t rows[GT2_TABLES],
                         const uint8_t* (*get)(const void*, uint32_t, uint32_t, uint32_t), const void* ctx) {
    uint32_t crc = song_crc32(0u, ins, GT2_INSTR_BYTES);
    for (uint32_t t = 0u; t < GT2_TABLES; t++) {
        for (uint32_t i = 0u; ins[PTR[t]] && i < rows[t] && ins[PTR[t]] + i <= GT2_TABLE_ROWS; i++) {
            crc = song_crc32(crc, get(ctx, t, i, 0u), 1u);
            crc = song_crc32(crc, get(ctx, t, i, 1u), 1u);
        }
    }
    return crc;
}

typedef struct {
    const gt2_song* s;
    uint32_t base[GT2_TABLES];
} song_rows;

static const uint8_t* song_byte(const void* ctx, uint32_t t, uint32_t i, uint32_t side) {
    const song_rows* r = ctx;
    const uint8_t* table = side ? r->s->rtable[t] : r->s->ltable[t];
    return table + r->base[t] + i - 1u;
}

static const uint8_t* instr_byte(const void* ctx, uint32_t t, uint32_t i, uint32_t side) {
    const lab_song_instr* in = ctx;
    return side ? &in->right[t][i] : &in->left[t][i];
}

uint32_t lab_song_crc(const gt2_song* s, uint32_t instr, const uint8_t rows[GT2_TABLES]) {
    const uint8_t* ins = s->instr[instr - 1u];
    song_rows r = {s, {ins[PTR[0]], ins[PTR[1]], ins[PTR[2]], ins[PTR[3]]}};
    return crc_rows(ins, rows, song_byte, &r);
}

uint32_t lab_song_crc_of(const lab_song_instr* in) {
    return crc_rows(in->instr, in->rows, instr_byte, in);
}

static uint32_t rd32(const uint8_t* p) {
    return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24;
}

int lab_song_kept(const gt2_song* s, uint32_t instr, uint8_t rows[GT2_TABLES], uint8_t* sound) {
    uint32_t len = 0u;
    const uint8_t* d = song_ext_find(s, lab_song_tag(instr), &len);
    if (!d || len < KEPT_HEAD + LAB_SOUND_BYTES_1_0 || d[0] != KEPT_VERSION ||
        lab_song_crc(s, instr, d + 1) != rd32(d + 1u + GT2_TABLES))
        return 0;
    uint8_t snd[LAB_SOUND_BYTES] = {0};
    uint32_t n = len - KEPT_HEAD < LAB_SOUND_BYTES ? len - KEPT_HEAD : LAB_SOUND_BYTES;
    memcpy(snd, d + KEPT_HEAD, n);
    if (!lab_sound_valid(snd))
        return 0;
    memcpy(rows, d + 1, GT2_TABLES);
    memcpy(sound, snd, LAB_SOUND_BYTES);
    return 1;
}

int lab_song_keep(gt2_song* s, uint32_t instr, const lab_song_instr* in, const uint8_t* sound) {
    uint32_t len = 0u, kept = 0u;
    uint8_t rows[GT2_TABLES], snd[LAB_SOUND_BYTES];
    if (!song_ext_find(s, lab_song_tag(instr), &len)) {
        for (uint32_t i = 1u; i <= GT2_MAX_INSTR; i++)
            kept += song_ext_find(s, lab_song_tag(i), &len) != NULL;
        for (uint32_t i = 1u; kept >= LAB_SONG_KEPT_MAX && i <= GT2_MAX_INSTR; i++) {
            if (song_ext_find(s, lab_song_tag(i), &len) && !lab_song_kept(s, i, rows, snd) &&
                song_ext_remove(s, lab_song_tag(i)) == 1)
                kept--; /* a chunk whose instrument has been edited since: no longer exact */
        }
        if (kept >= LAB_SONG_KEPT_MAX)
            return -1;
    }
    uint8_t d[KEPT_HEAD + LAB_SOUND_BYTES];
    d[0] = KEPT_VERSION;
    memcpy(d + 1, in->rows, GT2_TABLES);
    uint32_t crc = lab_song_crc_of(in);
    for (uint32_t i = 0u; i < 4u; i++)
        d[1u + GT2_TABLES + i] = (uint8_t)(crc >> (8u * i));
    memcpy(d + KEPT_HEAD, sound, LAB_SOUND_BYTES);
    return song_ext_set(s, lab_song_tag(instr), d, sizeof d) ? -1 : 0;
}

/* ---- pull ---- */

/* A pulse or filter program as it plays: its first set value, the band it moves in after that,
 * the frames of a loop (0: none), and whether it moves at all. */
typedef struct {
    int32_t set, lo, hi, loop;
    uint32_t moves, sets;
} run;

#define RUN_FRAMES 1200u
#define RUN_ROWS 24u

static void play(const gt2_song* s, uint32_t t, uint32_t ptr, run* r) {
    uint16_t seen[RUN_ROWS] = {0};
    uint32_t at = ptr, frame = 0u;
    int32_t v = 0;
    memset(r, 0, sizeof *r);
    r->lo = 0x7fffffff;
    r->hi = -1;
    while (at && at <= GT2_TABLE_ROWS && frame < RUN_FRAMES) {
        uint8_t l = s->ltable[t][at - 1u], rt = s->rtable[t][at - 1u];
        if (at >= ptr && at - ptr < RUN_ROWS && !seen[at - ptr])
            seen[at - ptr] = (uint16_t)(frame + 1u);
        if (l == JUMP) {
            if (rt >= ptr && rt - ptr < RUN_ROWS && seen[rt - ptr] && rt <= at) {
                r->loop = (int32_t)(frame - (seen[rt - ptr] - 1u));
                return;
            }
            at = rt;
            continue;
        }
        if (t == GT2_PTBL && (l & 0x80u)) {
            v = (int32_t)((l & 0x0fu) << 8 | rt);
            r->set = r->sets++ ? r->set : v;
            frame++;
        } else if (t == GT2_FTBL && (l & 0x80u || !l)) {
            if (l) { /* the type row: a cutoff row right below runs with it */
                if (at < GT2_TABLE_ROWS && !s->ltable[t][at]) {
                    at++;
                    rt = s->rtable[t][at - 1u];
                } else {
                    at++;
                    continue;
                }
            }
            v = rt;
            r->set = r->sets++ ? r->set : v;
            frame++;
        } else {
            for (uint32_t k = 0u; k < l; k++, frame++) {
                v = t == GT2_PTBL ? (v + (int8_t)rt) & 0xfff : (v + rt) & 0xff;
                r->lo = v < r->lo ? v : r->lo;
                r->hi = v > r->hi ? v : r->hi;
            }
            r->moves = 1u;
        }
        at++;
    }
}

uint32_t lab_song_pull(const gt2_song* s, uint32_t instr, uint32_t model, uint8_t* sound) {
    uint8_t rows[GT2_TABLES];
    if (lab_song_kept(s, instr, rows, sound))
        return LAB_SONG_EXACT;
    const uint8_t* ins = s->instr[instr - 1u];
    uint32_t notes = 0u;
    sound[LAB_SOUND_AD] = ins[GT2_INS_AD];
    sound[LAB_SOUND_SR] = ins[GT2_INS_SR];
    notes |= ins[GT2_INS_GATETIMER] != GATE_TIMER ? LAB_SONG_GATE : 0u;
    sound[LAB_SOUND_MONO_SYNC] = 0u;
    sound[LAB_SOUND_TUNE] = 0u;
    sound[LAB_SOUND_RESO_LINK] = sound[LAB_SOUND_PWM_LINK] = 0u;
    for (uint32_t k = 0u; k < LIVE_SLOTS; k++)
        sound[LAB_SOUND_SLOTS + k * LIVE_SLOT_FIELDS] = 0u;
    /* the wave program: one waveform row, or an arpeggio, else a drum's */
    uint32_t w = ins[GT2_INS_WAVE];
    uint32_t ctrl = ins[GT2_INS_FIRSTWAVE];
    sound[LAB_SOUND_ARP + LIVE_ARP_ON] = 0u;
    if (w) {
        const uint8_t* l = s->ltable[GT2_WTBL] + w - 1u;
        const uint8_t* r = s->rtable[GT2_WTBL] + w - 1u;
        song_pages_arp a;
        uint32_t used = 0u;
        if (song_pages_arp_decode(l, r, w, GT2_TABLE_ROWS + 1u - w, &a, &used) && a.steps > 1u) {
            uint32_t k = 0u;
            while (k + 1u < LIVE_ARP_RATE_CLOCK && live_arp_steps[k] < a.speed)
                k++;
            sound[LAB_SOUND_ARP + LIVE_ARP_ON] = 1u;
            sound[LAB_SOUND_ARP + LIVE_ARP_RATE] = (uint8_t)k;
            ctrl = a.wave;
            notes |= LAB_SONG_ARP;
        } else {
            ctrl = l[0] >= 0xe0u && l[0] < 0xf0u ? l[0] & 0x0fu : l[0];
            if (l[0] < 0x10u || l[0] >= 0xf0u || (r[0] && r[0] != 0x80u) ||
                (w < GT2_TABLE_ROWS && l[1] != JUMP))
                notes |= LAB_SONG_DRUM; /* a delay or command first, a note, more waveforms */
        }
    }
    if ((ctrl & 0xf0u) && ctrl < 0xf0u) /* a waveform: not $09's hard restart, not a command */
        sound[LAB_SOUND_CONTROL] = (uint8_t)(ctrl & 0xf6u);
    /* the pulse program: its width, a loop as the PWM */
    run pr, fr; /* each played before it is read */
    uint32_t p = ins[GT2_INS_PULSE], rate = 0u, depth = 0u;
    if (p) {
        play(s, GT2_PTBL, p, &pr);
        if (pr.sets)
            put16(sound, LAB_SOUND_PULSE, (uint32_t)pr.set);
        if (pr.loop && pr.moves) {
            depth = (uint32_t)(pr.hi - pr.lo) / 2u;
            rate = (uint32_t)clampi(ratio(163840, 6 * pr.loop), 1, LIVE_PWM_STEP_MAX); /* RUNS */
        } else if (pr.moves) {
            notes |= LAB_SONG_SWEEP;
        }
    }
    put16(sound, LAB_SOUND_PWM_DEPTH, depth > LIVE_PWM_DEPTH_MAX ? LIVE_PWM_DEPTH_MAX : depth);
    /* the filter program: its mode, resonance and routing, cutoff, a loop as the filter LFO */
    uint32_t f = ins[GT2_INS_FILTER];
    sound[LAB_SOUND_FILTER_VOICES] = 0u;
    sound[LAB_SOUND_FILTER_LFO] = 0u;
    if (f) {
        uint8_t l = s->ltable[GT2_FTBL][f - 1u], r = s->rtable[GT2_FTBL][f - 1u];
        if ((l & 0x80u) && l != JUMP) {
            sound[LAB_SOUND_FILTER_MODE] = l & 0x70u;
            sound[LAB_SOUND_RESONANCE] = r >> 4;
            sound[LAB_SOUND_FILTER_VOICES] = r & 7u ? 7u : 0u;
        }
        play(s, GT2_FTBL, f, &fr);
        if (fr.sets)
            put16(sound, LAB_SOUND_CUTOFF, (uint32_t)fr.set << 3);
        if (fr.loop && fr.moves) {
            int32_t d = (place(model, (uint32_t)fr.hi << 3) - place(model, (uint32_t)fr.lo << 3) + 1) / 2;
            sound[LAB_SOUND_FILTER_LFO] = (uint8_t)clampi(d, 1, LIVE_FILTER_LFO_MAX);
            uint32_t frate = (uint32_t)clampi(ratio(32768, fr.loop), 1, LIVE_PWM_STEP_MAX);
            if (rate && rate != frate)
                notes |= LAB_SONG_LFO; /* one LFO in LIVE: the PWM's rate */
            rate = rate ? rate : frate;
        } else if (fr.moves) {
            notes |= LAB_SONG_SWEEP;
        }
    }
    put16(sound, LAB_SOUND_PWM_RATE, rate ? rate : u16_at(sound, LAB_SOUND_PWM_RATE));
    /* the vibrato: a calculated speed back to LIVE's rate and depth; another as near as C-4's */
    uint32_t v = ins[GT2_INS_VIBDELAY] ? ins[GT2_INS_VIBPARAM] : 0u; /* delay 0: none */
    sound[LAB_SOUND_VIBRATO_DEPTH] = 0u;
    if (v) {
        uint8_t l = s->ltable[GT2_STBL][v - 1u], r = s->rtable[GT2_STBL][v - 1u];
        int32_t sp = l & 0x7f;
        int32_t d = l & 0x80u ? (r < 16u ? ((sp + 2) * 64) >> r : 0) : (sp + 2) * r * 64 / 265;
        notes |= l & 0x80u ? 0u : LAB_SONG_VIB;
        sound[LAB_SOUND_VIBRATO_RATE] = (uint8_t)clampi(ratio(1250, 3 * (2 * sp + 4)), 1, LIVE_EXPRESSION_MAX);
        sound[LAB_SOUND_VIBRATO_DEPTH] = (uint8_t)clampi(d, 1, LIVE_EXPRESSION_MAX);
        sound[LAB_SOUND_VIBRATO_DELAY] = (uint8_t)clampi((ins[GT2_INS_VIBDELAY] - 1) * 2, 0, LIVE_EXPRESSION_MAX);
    }
    return notes;
}

/* ---- the words, the free instrument and rows ---- */

void lab_song_words(uint32_t notes, int dropped, char* out, uint32_t max) {
    static const char* const WORDS[] = {"ATTACK", "PWM",  "FILTER", "LFO",  "VIB", "ARP", "WAVE",
                                        "DRUM",   "SWEEP", "GATE",  "SYNC", "TUNE", "PLAY", "FX"};
    uint32_t n = 0u;
    for (uint32_t b = dropped ? 10u : 0u; b < (dropped ? 14u : 10u); b++) {
        const char* w = WORDS[b];
        uint32_t k = 0u;
        while (w[k])
            k++;
        if (!(notes >> b & 1u) || n + k + (n ? 1u : 0u) >= max)
            continue;
        if (n)
            out[n++] = ' ';
        memcpy(out + n, w, k);
        n += k;
    }
    out[n] = 0;
}

uint32_t lab_song_free(const gt2_song* s) {
    for (uint32_t i = 0u; i < GT2_MAX_INSTR; i++) {
        uint32_t any = 0u;
        for (uint32_t k = 0u; k < GT2_INSTR_BYTES; k++)
            any |= s->instr[i][k];
        if (!any)
            return i + 1u;
    }
    return 0u;
}

uint32_t lab_song_table_end(const gt2_song* s, uint32_t t) {
    uint32_t n = GT2_TABLE_ROWS;
    while (n && !(s->ltable[t][n - 1u] | s->rtable[t][n - 1u]))
        n--;
    return n;
}

int lab_song_shared(const gt2_song* s, uint32_t instr, uint32_t t, uint32_t first, uint32_t n) {
    for (uint32_t i = 1u; i <= GT2_MAX_INSTR; i++) {
        uint32_t at = s->instr[i - 1u][PTR[t]];
        if (i != instr && at >= first && at < first + n)
            return 1;
    }
    return 0;
}
