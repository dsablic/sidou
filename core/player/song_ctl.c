// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The song in the audio interrupt (song_ctl.h). The interrupt's functions come first; on the
 * device app.ld puts them (and only them) in SRAM, one section a function. */
#include "song_ctl.h"

#include <stddef.h>

#include "gen/digi.h"

/* The compiler keeps the stores before it before the ones after (one core: the interrupt
 * either runs before a store or after, never beside it). */
#define COMPILER_FENCE() __asm__ volatile("" ::: "memory")

#define CMD_MASK (SONG_CMD_RING - 1u)
#define EV_MASK (SONG_EVENT_RING - 1u)
#define NO_GATE 0xffu /* a PLAY, STOP or CONTINUE that leaves chip A's gate reset alone */
/* What SONG_LOADING ends in (song_ctl.load_for). */
#define LOAD_PLAY 0u     /* the PLAY command: the song plays */
#define LOAD_PAUSED 1u   /* the PLAY command, paused: the audition, no song played yet */
#define LOAD_WAKE 2u     /* chip A taken again for the audition of a stopped song */
#define LOAD_CONTINUE 3u /* chip A taken again, then CONTINUE */

_Static_assert((SONG_CMD_RING & CMD_MASK) == 0u, "the command ring is a power of two");
_Static_assert((SONG_EVENT_RING & EV_MASK) == 0u, "the event ring is a power of two");
_Static_assert(sizeof(gt2_player) % 4u == 0u, "a hand-over copies the player in words");
_Static_assert(SONG_DIGI_SAMPLES == SONG_DIGI_KINDS - 1u, "a built-in sample for each DIGI kind");

/* --- The audio interrupt ---------------------------------------------------------------- */

/* The player's sink: chip A, less the muted voices; the shadow keeps every write. */
static void song_sink(void* ctx, uint8_t reg, uint8_t value) {
    song_ctl* sc = ctx;
    sc->shadow[reg] = value;
    if (reg < 21u) {
        uint32_t v = reg / 7u;
        if ((((sc->mute | sc->taken) >> v) & 1u) ||
            (!sc->isr_playing && !((sc->player.jam_voices >> v) & 1u)))
            return;
    }
    sid_write(sc->chip, reg, value);
}

/* The shadow's registers into s, less the muted voices'. */
static void song_rewrite(song_ctl* sc, sid* s) {
    for (uint32_t r = 0u; r < GT2_SID_REGS; r++) {
        if (r < 21u) {
            uint32_t v = r / 7u;
            if ((((sc->mute | sc->taken) >> v) & 1u) ||
                (!sc->isr_playing && !((sc->player.jam_voices >> v) & 1u)))
                continue;
        }
        sid_write(s, (uint8_t)r, sc->shadow[r]);
    }
}

/* Voice v's gate off on chip A (its wave register less the gate bit). */
static void song_gate_off(song_ctl* sc, uint32_t v) {
    sid_write(sc->chip, (uint8_t)(v * 7u + 4u), (uint8_t)(sc->shadow[v * 7u + 4u] & 0xfeu));
}

static void song_set_gate_reset(song_ctl* sc, uint8_t g) {
    if (g != NO_GATE)
        sid_set_gate_reset(sc->chip, g);
}

/* The voices left out of the song's writes: muted (the voice's gate off, then nothing of the
 * song's) and taken (the keys', KEYS A: nothing of the song's, the keys' writes stand). Back
 * from either: the song's registers again. */
static void song_voices(song_ctl* sc, uint8_t mute, uint8_t taken) {
    uint8_t was = (uint8_t)(sc->mute | sc->taken);
    uint8_t muting = (uint8_t)(mute & ~sc->mute);
    sc->mute = mute;
    sc->taken = taken;
    uint8_t now = (uint8_t)(mute | taken);
    for (uint32_t v = 0u; v < GT2_PLAY_CHANNELS; v++) {
        uint32_t bit = 1u << v;
        if ((now & bit) && !(was & bit)) {
            if (muting & bit)
                song_gate_off(sc, v);
        } else if (!(now & bit) && (was & bit) &&
                   (sc->isr_playing || ((sc->player.jam_voices >> v) & 1u))) {
            for (uint32_t r = v * 7u; r < v * 7u + 7u; r++)
                sid_write(sc->chip, (uint8_t)r, sc->shadow[r]);
        }
    }
}

static void song_apply(song_ctl* sc, const song_cmd* c) {
    switch (c->kind) {
    case SONG_CMD_PLAY:
        gt2_play_start(&sc->player, sc->song, (const gt2_shape*)c->u.p.ptr,
                       (const uint8_t*)&c->u.p.x);
        sc->calls = 0u;
        sc->next = c->c ? SONG_NEXT_HELD : 0u; /* c: held for M2e's align */
        sc->rem = 0u;
        sc->period.q = c->u.p.y;
        sc->period.r = c->u.p.z & 0xffffffu;
        sc->period.den = (uint32_t)(uintptr_t)c->u.p.ptr2;
        sc->armed = 0u;
        sc->lock_regs = 0u;
        for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
            sc->jam[ch][0] = 0u;
            sc->lock_n[ch] = 0u;
            sc->row_pat[ch] = 0xffu;
        }
        /* the keys' registers out: the song starts from a silent chip, less the voices the
         * keys hold (KEYS A), whose note sounds on */
        for (uint32_t r = 0u; r < GT2_SID_REGS; r++) {
            sc->shadow[r] = 0u;
            if (r >= 21u || !((sc->taken >> (r / 7u)) & 1u))
                sid_write(sc->chip, (uint8_t)r, 0u);
        }
        sc->song_gate = c->a;
        song_set_gate_reset(sc, c->a);
        sc->play_block = sc->blocks;
        sc->isr_playing = (uint8_t)!c->b; /* b: paused, for the audition */
        sc->audition = c->b;
        break;
    case SONG_CMD_STOP: {
        uint8_t cancel = c->b;
        if (c->c)
            cancel |= sc->player.jam_voices;
        sc->isr_playing = 0u;
        sc->audition = 0u;
        sc->armed = 0u;
        sc->player.jam_voices = 0u;
        for (uint32_t v = 0u; v < GT2_PLAY_CHANNELS; v++) {
            if (c->c && sc->jam[v][0] && sc->jam[v][1] != SONG_JAM_RELEASE)
                cancel |= (uint8_t)(1u << v);
            if ((cancel >> v) & 1u)
                sc->player.chn[v].newnote = 0u;
            if (c->c || ((cancel >> v) & 1u))
                sc->jam[v][0] = 0u;
            if (!(((sc->mute | sc->taken) >> v) & 1u))
                song_gate_off(sc, v);
        }
        song_set_gate_reset(sc, c->a);
        break;
    }
    case SONG_CMD_CONTINUE:
        sc->next = c->c ? SONG_NEXT_HELD : 0u;
        sc->rem = 0u;
        song_set_gate_reset(sc, c->a);
        sc->audition = 0u;
        sc->isr_playing = 1u;
        break;
    case SONG_CMD_MUTE:
        song_voices(sc, c->a, sc->taken);
        break;
    case SONG_CMD_SHAPE:
        if (c->a)
            gt2_play_retain_shape(&sc->player, (const gt2_shape*)c->u.p.ptr);
        else
            gt2_play_set_shape(&sc->player, (const gt2_shape*)c->u.p.ptr);
        break;
    case SONG_CMD_HANDOVER:
        sc->armed = 1u;
        sc->armed_now = c->a;
        sc->armed_state = (const gt2_player*)c->u.p.ptr;
        sc->armed_shape = (const gt2_shape*)c->u.p.ptr2;
        sc->armed_call = c->u.p.x;
        sc->armed_seq = c->seq;
        break;
    case SONG_CMD_JAM:
        if (!sc->isr_playing && !sc->audition && sc->player.shape) {
            sc->audition = 1u; /* a stopped song's audition: the calls from now on */
            sc->next = 0u;
            sc->rem = 0u;
        }
        if (c->a < GT2_PLAY_CHANNELS) {
            sc->jam[c->a][0] = 1u;
            sc->jam[c->a][1] = c->b;
            sc->jam[c->a][2] = c->c;
        }
        break;
    case SONG_CMD_EDIT:
        gt2_edit_apply(sc->song, &c->u.edit);
        if (c->a) {
            COMPILER_FENCE();
            for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++)
                sc->lock_pub[ch] = (uint8_t)((c->b >> ch) & 1u);
        }
#ifdef SIDOU_REG_RING_RACE_HOOK
        if (c->u.edit.kind == GT2_EDIT_CELL)
            sc->edit_cells++;
        else if (c->u.edit.kind == GT2_EDIT_SWAP)
            sc->edit_swaps++;
#endif
        break;
    case SONG_CMD_ALIGN: {
        int32_t d = (int32_t)(c->u.p.x - sc->samples);
        sc->next = d > 0 ? (uint32_t)d : 0u;
        sc->rem = 0u;
        break;
    }
    case SONG_CMD_PERIOD:
        sc->period.q = c->u.p.x;
        sc->period.r = c->u.p.y;
        sc->period.den = c->u.p.z;
        sc->rem = 0u;
        break;
    default:
        break;
    }
}

void song_ctl_block(song_ctl* sc, sid* chip) {
    sc->chip = chip;
    sc->blocks++;
    /* The keys' voices first (KEYS A): from this block's call on, before any command. */
    uint8_t take = sc->take;
    if (take != sc->taken)
        song_voices(sc, sc->mute, take);
    uint32_t edits = 0u;
    uint32_t tail = sc->cmd_tail;
    uint32_t head = sc->cmd_head;
    COMPILER_FENCE(); /* the head before the commands it publishes */
    while (tail != head) {
        const song_cmd* c = &sc->cmd[tail & CMD_MASK];
        if (c->kind == SONG_CMD_EDIT) {
            if (c->a && (sc->lock_edit_refresh || sc->lock_edit_rows !=
                ((uint32_t)sc->row_seq[0] | (uint32_t)sc->row_seq[1] << 8 |
                 (uint32_t)sc->row_seq[2] << 16))) {
                sc->lock_edit_refresh = 1u;
                break;
            }
            if (edits == SONG_EDITS_PER_BLOCK)
                break; /* the rest wait for the next block */
            edits++;
        }
        song_apply(sc, c);
        sc->applied = c->seq;
        tail++;
    }
    COMPILER_FENCE(); /* the commands read before the slots go back to the producer */
    sc->cmd_tail = tail;
    if (edits > sc->edits_block_max)
        sc->edits_block_max = edits;
}

uint32_t song_ctl_due(const song_ctl* sc, uint32_t n) {
    return (sc->isr_playing || sc->audition) && sc->next < n ? sc->next : n;
}

static void song_handover(song_ctl* sc) {
    if (!sc->armed)
        return;
    if (!sc->armed_now && sc->calls != sc->armed_call) {
        if (sc->calls > sc->armed_call) {
            sc->armed = 0u; /* too late: the main loop fast-forwards further and tries again */
            sc->handover_ok = 0u;
            sc->handover_seq = sc->armed_seq;
        }
        return;
    }
    /* Word by word, not a struct assignment (which may become a memcpy call, in flash). */
    const uint32_t* from = (const uint32_t*)(const void*)sc->armed_state;
    uint32_t* to = (uint32_t*)(void*)&sc->player;
    for (uint32_t i = 0u; i < sizeof(gt2_player) / 4u; i++)
        ((volatile uint32_t*)to)[i] = from[i];
    sc->player.shape = sc->armed_shape;
    if (sc->armed_now)
        sc->calls = sc->armed_call;
    /* chip A as the fast-forwarded player left its registers */
    for (uint32_t r = 0u; r < GT2_SID_REGS; r++)
        sc->shadow[r] = sc->player.regs[r];
    song_rewrite(sc, sc->chip);
    sc->armed = 0u;
    sc->handover_ok = 1u;
    sc->handover_seq = sc->armed_seq;
}

static void song_push_events(song_ctl* sc) {
    uint32_t h = sc->ev_head;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        const gt2_event* e = &sc->player.ev[ch];
        if (e->flags == 0u)
            continue;
        song_event* o = &sc->ev[h & EV_MASK];
        o->frame = (uint16_t)sc->calls;
        o->flags = (uint8_t)((e->flags & 0x1fu) | (ch << 5));
        o->pattern = e->pattern;
        o->row = e->row;
        o->order_pos = e->order_pos;
        o->note = e->note;
        o->instr = e->instr;
        h++;
    }
    COMPILER_FENCE(); /* the events before the head that publishes them */
    sc->ev_head = h;
}

/* A channel's row starts: the published locks that are this row's become the channel's. */
static void song_lock_row(song_ctl* sc, uint32_t ch, const gt2_event* e) {
    const song_lock_set* set = &sc->lock_next[ch][sc->lock_pub[ch] & 1u];
    uint32_t n = 0u;
    for (uint32_t i = 0u; i < set->n && i < SONG_ROW_LOCKS; i++) {
        const song_row_lock* l = &set->e[i];
        if (l->pattern == e->pattern && l->row == e->row) {
            song_row_lock* d = &sc->lock_cur[ch][n++];
            d->pattern = l->pattern;
            d->row = l->row;
            d->param = l->param;
            d->lo = l->lo;
            d->hi = l->hi;
        }
    }
    sc->lock_n[ch] = (uint8_t)n;
    sc->row_pat[ch] = e->pattern;
    sc->row_row[ch] = e->row;
    sc->row_ord[ch] = e->order_pos;
    COMPILER_FENCE(); /* the row before the count that says it changed */
    sc->row_seq[ch] = (uint8_t)(sc->row_seq[ch] + 1u);
}

/* After a call: the locked registers over the player's (from its shadow and the locks), and
 * the shadow's back in those a lock left. */
static void song_lock_apply(song_ctl* sc) {
    uint8_t r[GT2_SID_REGS];
    uint32_t mask = 0u;
    uint8_t drop = (uint8_t)(sc->mute | sc->taken);
    for (uint32_t g = 0u; g < GT2_SID_REGS; g++)
        r[g] = sc->shadow[g];
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        uint32_t x = ch * 7u;
        for (uint32_t i = 0u; i < sc->lock_n[ch]; i++) {
            const song_row_lock* l = &sc->lock_cur[ch][i];
            uint32_t v = (uint32_t)l->lo | (uint32_t)l->hi << 8;
            switch (l->param) {
            case SONG_LOCK_CUTOFF:
                r[0x15] = (uint8_t)((r[0x15] & 0xf8u) | (v & 7u));
                r[0x16] = (uint8_t)(v >> 3);
                mask |= 3u << 0x15;
                break;
            case SONG_LOCK_RESONANCE:
                r[0x17] = (uint8_t)((r[0x17] & 0x0fu) | (v << 4));
                mask |= 1u << 0x17;
                break;
            case SONG_LOCK_FILTER_MODE:
                r[0x18] = (uint8_t)((r[0x18] & 0x8fu) | (v << 4));
                mask |= 1u << 0x18;
                break;
            case SONG_LOCK_ROUTE:
                r[0x17] = (uint8_t)(v ? r[0x17] | (1u << ch) : r[0x17] & ~(1u << ch));
                mask |= 1u << 0x17;
                break;
            case SONG_LOCK_PULSE:
                if ((drop >> ch) & 1u)
                    break;
                r[x + 2u] = (uint8_t)v;
                r[x + 3u] = (uint8_t)((sc->shadow[x + 3u] & 0xf0u) | (v >> 8));
                mask |= 3u << (x + 2u);
                break;
            default:
                break;
            }
        }
    }
    uint32_t all = mask | sc->lock_regs;
    for (uint32_t g = 0u; all != 0u; g++, all >>= 1) {
        if (all & 1u)
            sid_write(sc->chip, (uint8_t)g, ((mask >> g) & 1u) ? r[g] : sc->shadow[g]);
    }
    sc->lock_regs = mask;
}

void song_ctl_call(song_ctl* sc) {
    song_handover(sc);
    uint8_t frozen = (uint8_t)!sc->isr_playing;
    sc->player.frozen = frozen;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        /* a paused start's first call initialises the song (which clears a new note): the
         * jam at the call after */
        if (sc->jam[ch][0] && !(frozen && sc->player.initsong)) {
            sc->jam[ch][0] = 0u;
            if (sc->jam[ch][1] == SONG_JAM_RELEASE) {
                sc->player.chn[ch].gate = 0xfeu; /* as a key-off row */
                continue;
            }
            gt2_play_jam(&sc->player, ch, sc->jam[ch][1], sc->jam[ch][2], song_sink, sc);
            if (frozen)
                sc->player.chn[ch].counter = 1u; /* its tick 0 at this call */
        }
    }
    sc->call_sample = sc->samples + sc->next;
    gt2_play_frame(&sc->player, song_sink, sc);
    sc->past_b7 |= sc->player.freq_unknown;
    song_push_events(sc);
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++)
        if (sc->player.ev[ch].flags & GT2_EV_ROW)
            song_lock_row(sc, ch, &sc->player.ev[ch]);
    song_lock_apply(sc);
    sc->calls++;
    sc->next += sc->period.q;
    sc->rem += sc->period.r;
    if (sc->rem >= sc->period.den) {
        sc->rem -= sc->period.den;
        sc->next++;
    }
}

void song_ctl_block_end(song_ctl* sc, int16_t* out, uint32_t n) {
    /* The digi drums (song_ctl.h). A note in this block's call (its sample is in the block)
     * whose instrument is a digi drum starts its sample: the samples before the call are a
     * silent code's, the sample's codes follow. */
    uint32_t at = sc->call_sample - sc->samples;
    if (sc->isr_playing || sc->audition)
        sc->next -= n;
    sc->samples += n;
    if (at < n) {
        const gt2_event* e = sc->player.ev;
        for (uint32_t ok = sc->digi_ok; ok != 0u; ok >>= 1, e++) {
            uint32_t kind = sc->digi_of[e->instr];
            if ((ok & 1u) && (e->flags & GT2_EV_NOTE) && kind) {
                sc->digi_at = sc->digi_from[kind - 1u];
                sc->digi_end = sc->digi_from[kind];
                sc->digi_left = (uint8_t)at;
                sc->digi_level = 0;
            }
        }
    }
    uint32_t i = sc->digi_at;
    if (i == 0u)
        return;
    const uint32_t end = sc->digi_end;
    int32_t level = sc->digi_level;
    int16_t* const last = out + n;
    int16_t* next = out + sc->digi_left; /* where the next code starts */
    do {
        if (out == next) {
            if (i == end) {
                i = 0u;
                next = last;
                break;
            }
            uint32_t two = SONG_DIGI_DATA[i >> 1];
            level = ((int32_t)((two >> ((i & 1u) << 2)) & 15u) - 8) * SONG_DIGI_LEVEL;
            i++;
            next = out + SONG_DIGI_SPAN;
        }
        int32_t x = *out + level;
        x = x < -32768 ? -32768 : x;
        *out = (int16_t)(x > 32767 ? 32767 : x);
    } while (++out < last);
    sc->digi_at = (uint16_t)i;
    sc->digi_left = (uint8_t)(next - last);
    sc->digi_level = (int16_t)level;
}

void song_ctl_chip_swapped(void* ctx, uint32_t chip, sid* s) {
    song_ctl* sc = ctx;
    if (chip != 0u || !(sc->isr_playing || sc->audition))
        return;
    song_rewrite(sc, s);
    /* and the row's locked registers over the shadow's, as after its calls: the new chip
     * would play the song's own cutoff and pulse until the next call */
    sid* was = sc->chip;
    sc->chip = s;
    song_lock_apply(sc);
    sc->chip = was;
    if (sc->song_gate != NO_GATE)
        sid_set_gate_reset(s, sc->song_gate);
}

void song_ctl_render(song_ctl* sc, sid* chip, int16_t* out, uint32_t n) {
    uint32_t at = song_ctl_due(sc, n);
    if (at < n) {
        if (at > 0u)
            sid_render(chip, out, at);
        song_ctl_call(sc);
        sid_render(chip, out + at, n - at);
    } else {
        sid_render(chip, out, n);
    }
}

/* --- The main loop ---------------------------------------------------------------------- */

static void lock_prime(song_ctl* sc);
static void digi_map(song_ctl* sc);
static void lock_character(song_ctl* sc);
static void lock_prepare_edit(song_ctl* sc, song_cmd* c, const gt2_edit* e);
static void lock_refresh_edit(song_ctl* sc);

static void zero(void* p, uint32_t n) {
    uint8_t* q = p;
    for (uint32_t i = 0u; i < n; i++)
        q[i] = 0u;
}

static int same(const void* a, const void* b, uint32_t n) {
    const uint8_t* p = a;
    const uint8_t* q = b;
    for (uint32_t i = 0u; i < n; i++)
        if (p[i] != q[i])
            return 0;
    return 1;
}

uint32_t song_digi_sample(uint32_t kind, uint8_t* codes) {
    if (kind == SONG_DIGI_NONE || kind >= SONG_DIGI_KINDS)
        return 0u;
    uint32_t from = SONG_DIGI_START[kind - 1u], to = SONG_DIGI_START[kind];
    for (uint32_t i = from; codes != NULL && i < to; i++)
        codes[i - from] = (uint8_t)((SONG_DIGI_DATA[i >> 1] >> ((i & 1u) << 2)) & 15u);
    return to - from;
}

void song_ctl_init(song_ctl* sc, gt2_song* song, gt2_shape* shape0, gt2_shape* shape1,
                   song_work* w) {
    zero(sc, sizeof *sc);
    sc->song = song;
    sc->shapes[0] = shape0;
    sc->shapes[1] = shape1;
    sc->w = w;
    sc->keys_gate_reset = 1u;
    sc->song_gate = NO_GATE;
    sc->period = song_period_of(1u, 0u);
    sc->jam_lo = 96u;
    sc->digi_ok = 7u;
    for (uint32_t k = 0u; k < SONG_DIGI_KINDS; k++)
        sc->digi_from[k] = SONG_DIGI_START[k];
    for (uint32_t f = 0u; f < 4u; f++)
        sc->char_told[f] = -1;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++)
        for (uint32_t f = 0u; f < 4u; f++)
            sc->char_lock[ch][f] = -1;
}

void song_ctl_set_hooks(song_ctl* sc, const song_chip_hooks* hooks) {
    sc->hooks = *hooks;
}

void song_ctl_set_keys_gate_reset(song_ctl* sc, uint8_t on) {
    sc->keys_gate_reset = on ? 1u : 0u;
}

static uint32_t gcd(uint32_t a, uint32_t b) {
    while (b != 0u) {
        uint32_t t = a % b;
        a = b;
        b = t;
    }
    return a;
}

/* a * b / d and its remainder, by shifts (no 64-bit division helper on the device). */
static uint32_t muldiv(uint32_t a, uint32_t b, uint32_t d, uint32_t* rem) {
    uint64_t n = (uint64_t)a * b;
    uint64_t q = 0u, r = 0u;
    for (int i = 63; i >= 0; i--) {
        r = (r << 1) | ((n >> i) & 1u);
        if (r >= d) {
            r -= d;
            q |= (uint64_t)1u << i;
        }
    }
    *rem = (uint32_t)r;
    return (uint32_t)q;
}

song_period song_period_of(uint32_t multispeed, uint32_t ntsc) {
    /* gt2reloc's CIA timer (greloc.c, the .sid's speed code): latch value + 1 cycles */
    uint32_t base = ntsc ? 0x42c6u : 0x4cc7u;
    uint32_t clock = ntsc ? 1022727u : 985248u;
    uint32_t cycles = (multispeed ? base / multispeed : base * 2u) + 1u;
    /* samples a call: cycles / clock * 24 MHz / 544 */
    uint32_t num = 24000000u, den = clock * 544u;
    uint32_t g = gcd(num, den);
    num /= g;
    den /= g;
    song_period p;
    p.q = muldiv(cycles, num, den, &p.r);
    p.den = den;
    return p;
}

static void cmd_init(song_cmd* c, uint8_t kind) {
    zero(c, sizeof *c);
    c->kind = kind;
}

static uint32_t room(const song_ctl* sc) {
    return SONG_CMD_RING - (sc->stage - sc->cmd_tail);
}

/* Stages a command; its seq. The caller has checked the room. */
static uint32_t push(song_ctl* sc, const song_cmd* c) {
    song_cmd* d = &sc->cmd[sc->stage & CMD_MASK];
    *d = *c;
    d->seq = ++sc->seq;
    sc->stage++;
    return d->seq;
}

void song_ctl_commit(song_ctl* sc) {
    COMPILER_FENCE(); /* the commands before the head that publishes them */
    sc->cmd_head = sc->stage;
}

static uint8_t spare(const song_ctl* sc) {
    return (uint8_t)(sc->active ^ 1u);
}

/* The player follows shapes[spare] once c (which names it) is applied; the other buffer is
 * free once that has happened. */
static void take_spare(song_ctl* sc, uint32_t seq) {
    sc->active = spare(sc);
    sc->spare_wait = 1u;
    sc->spare_free_at = seq;
}

static uint8_t gate_for_play(const song_ctl* sc) {
    return sc->hooks.chip_a_song ? NO_GATE : 0u; /* AUTHENTIC envelopes: no gate reset */
}

static uint8_t cancel_jams(song_ctl* sc) {
    uint8_t mask = 0u;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        if (sc->jam_wait[ch][0] == 2u)
            mask |= (uint8_t)(1u << ch);
        sc->jam_wait[ch][0] = 0u;
    }
    sc->jam_release = 0u;
    return mask;
}

int song_ctl_play(song_ctl* sc, uint32_t subtune, uint32_t pos) {
    digi_map(sc);
    song_work* w = sc->w;
    if (sc->state == SONG_LOADING || sc->inflight_n || sc->ff_on || sc->spare_wait ||
        sc->deriving || room(sc) < 2u)
        return 0;
    if (sc->state == SONG_PLAYING)
        song_ctl_stop(sc); /* the song plays no more while the next one loads */
    sc->bg_on = 0u;
    if (!song_ext_get_options(sc->song, &sc->opts))
        gt2_options_default(&sc->opts);
    sc->error = 0;
    if (sc->opts.multispeed > SONG_MULTISPEED_MAX) {
        sc->error = SONG_ERR_MULTISPEED;
        return 1;
    }
    int r = gt2_shape_begin(&w->bg, sc->song, subtune, &sc->opts);
    if (r < 0) {
        sc->error = (int16_t)r;
        return 1;
    }
    sc->subtune = (uint8_t)subtune;
    sc->start_pos = (uint8_t)pos;
    sc->digi_hold = 0u; /* a new start: no voice holds a digi drum yet */
    sc->bg_on = 0u;
    sc->chip_ready = 0u;
    sc->loading_done = 0u;
    sc->load_for = LOAD_PLAY;
    sc->auditioning = 0u;
    sc->jam_instruments[0] = sc->jam_instruments[1] = 0u;
    sc->jam_lo = 96u;
    sc->jam_hi = sc->jam_live = sc->jam_release = 0u;
    cancel_jams(sc);
    sc->state = SONG_LOADING;
    return 1;
}

int song_ctl_stop(song_ctl* sc) {
    if (sc->state != SONG_PLAYING || room(sc) < 1u)
        return 0;
    song_cmd c;
    cmd_init(&c, SONG_CMD_STOP);
    c.a = sc->hooks.chip_a_keys ? NO_GATE : sc->keys_gate_reset;
    c.b = cancel_jams(sc);
    sc->ff_stop |= sc->ff_on;
    sc->stop_seq = push(sc, &c);
    sc->handback = 1u;
    sc->state = SONG_STOPPED;
    return 1;
}

/* SONG_LOADING for chip A alone (the song and its shape as they are): its hook, then what
 * `then` says. */
static void retake_chip(song_ctl* sc, uint8_t then) {
    sc->loading_done = 1u;
    sc->chip_ready = 0u;
    sc->load_for = then;
    sc->state = SONG_LOADING;
}

int song_ctl_continue(song_ctl* sc) {
    digi_map(sc);
    if (sc->state != SONG_STOPPED || room(sc) < 1u)
        return 0;
    sc->auditioning = 0u;
    if (!sc->chip_held && !sc->handback) {
        retake_chip(sc, LOAD_CONTINUE); /* chip A went back to the keys meanwhile */
        return 1;
    }
    song_cmd c;
    cmd_init(&c, SONG_CMD_CONTINUE);
    c.a = gate_for_play(sc);
    c.c = sc->hold_next;
    sc->hold_next = 0u;
    push(sc, &c);
    sc->handback = 0u;
    sc->state = SONG_PLAYING;
    return 1;
}

static void push_mute(song_ctl* sc, uint8_t mask) {
    song_cmd c;
    cmd_init(&c, SONG_CMD_MUTE);
    c.a = mask;
    push(sc, &c);
    sc->mute_sent = mask;
}

int song_ctl_mute(song_ctl* sc, uint32_t mask) {
    if (room(sc) < 1u)
        return 0;
    sc->mute_user = (uint8_t)(mask & 7u);
    push_mute(sc, (uint8_t)(sc->mute_user | sc->digi_hold)); /* the digi drums' voices too */
    return 1;
}

int song_ctl_take(song_ctl* sc, uint32_t mask) {
    sc->take = (uint8_t)(mask & 7u); /* the one writer; read at the next block's start */
    return 1;
}

int song_ctl_jam(song_ctl* sc, uint32_t voice, uint32_t note, uint32_t instr) {
    if (voice >= GT2_PLAY_CHANNELS || (note > 95u && note != SONG_JAM_RELEASE) ||
        instr > GT2_MAX_INSTR)
        return 0;
    if (note == SONG_JAM_RELEASE && sc->jam_wait[voice][0]) {
        sc->jam_release |= (uint8_t)(1u << voice);
        return 1;
    }
    if (sc->jam_wait[voice][0])
        return 0;
    if (note != SONG_JAM_RELEASE) {
        if (!sc->shapes_used && sc->state != SONG_LOADING)
            return 0;
        uint32_t used = instr ? instr : sc->player.chn[voice].instr;
        if (used > GT2_MAX_INSTR)
            used = 0u;
        const gt2_shape* sh = sc->shapes[sc->active];
        int ready = sc->shapes_used && sc->state != SONG_LOADING && !sc->ff_on &&
                    !sc->deriving && note >= sh->first_note && note <= sh->last_note &&
                    (used == 0u || sh->ins_num[used] != 0u);
        if (ready && room(sc) < 1u)
            return 0;
        if (used)
            sc->jam_instruments[used >> 5] |= 1u << (used & 31u);
        if (note < sc->jam_lo)
            sc->jam_lo = (uint8_t)note;
        if (note > sc->jam_hi)
            sc->jam_hi = (uint8_t)note;
        sc->jam_live = 1u;
        if (sc->bg_on)
            sc->bg_dirty = 1u;
        if (!ready) {
            sc->jam_wait[voice][0] = 1u;
            sc->jam_wait[voice][1] = (uint8_t)note;
            sc->jam_wait[voice][2] = (uint8_t)instr;
            return 1;
        }
    } else if (room(sc) < 1u) {
        return 0;
    }
    song_cmd c;
    cmd_init(&c, SONG_CMD_JAM);
    c.a = (uint8_t)voice;
    c.b = (uint8_t)note;
    c.c = (uint8_t)instr;
    push(sc, &c);
    return 1;
}

uint32_t song_ctl_shown_state(const song_ctl* sc) {
    if (sc->state == SONG_LOADING && (sc->load_for == LOAD_PAUSED || sc->load_for == LOAD_WAKE))
        return sc->shapes_used ? SONG_STOPPED : SONG_IDLE;
    return sc->state;
}

int song_ctl_jam_ready(const song_ctl* sc) {
    return sc->state == SONG_PLAYING || sc->auditioning;
}

uint32_t song_ctl_again(const song_ctl* sc) {
    if (sc->state != SONG_PLAYING)
        return 0u;
    const volatile gt2_player* p = &sc->player;
    uint32_t mask = 0u;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        uint8_t seq, now, next;
        do { /* the row's pattern and the player's, from one call (row_seq counts the rows) */
            seq = sc->row_seq[ch];
            COMPILER_FENCE();
            now = sc->row_pat[ch];
            next = p->chn[ch].pattnum;
            COMPILER_FENCE();
        } while (seq != sc->row_seq[ch]);
        if (now != 0xffu && now == next)
            mask |= 1u << ch;
    }
    return mask;
}

int song_ctl_audition(song_ctl* sc) {
    digi_map(sc);
    if (sc->state == SONG_PLAYING || sc->state == SONG_LOADING || sc->auditioning)
        return 1;
    if (sc->state == SONG_IDLE) {
        /* no song played yet: its analysis first, then a paused PLAY */
        if (!song_ctl_play(sc, sc->subtune, 0u))
            return 0;
        if (sc->state == SONG_LOADING)
            sc->load_for = LOAD_PAUSED;
        return 1;
    }
    if (sc->chip_held || sc->handback) {
        sc->handback = 0u; /* a stop not yet handed back: chip A stays the song's */
        sc->auditioning = 1u;
        return 1;
    }
    retake_chip(sc, LOAD_WAKE);
    return 1;
}

int song_ctl_audition_end(song_ctl* sc) {
    if (!sc->auditioning)
        return 1;
    if (room(sc) < 1u)
        return 0;
    song_cmd c;
    cmd_init(&c, SONG_CMD_STOP);
    c.a = sc->hooks.chip_a_keys ? NO_GATE : sc->keys_gate_reset;
    c.b = cancel_jams(sc);
    c.c = 1u; /* cancel audition notes, retaining unjammed sequencer lookahead */
    sc->ff_stop |= sc->ff_on;
    sc->stop_seq = push(sc, &c);
    sc->handback = 1u;
    sc->auditioning = 0u;
    return 1;
}

int song_ctl_set_period(song_ctl* sc, song_period p) {
    if (room(sc) < 1u || p.den == 0u || p.q < SONG_BLOCK)
        return 0;
    song_cmd c;
    cmd_init(&c, SONG_CMD_PERIOD);
    c.u.p.x = p.q;
    c.u.p.y = p.r;
    c.u.p.z = p.den;
    push(sc, &c);
    sc->period = p;
    return 1;
}

static void push_shape(song_ctl* sc) {
    song_cmd c;
    cmd_init(&c, SONG_CMD_SHAPE);
    c.a = (uint8_t)(sc->jam_live || sc->state != SONG_PLAYING);
    if (c.a && sc->shapes[spare(sc)]->channels < sc->shapes[sc->active]->channels)
        gt2_shape_keep_channels(sc->shapes[spare(sc)], sc->shapes[sc->active]->channels);
    c.u.p.ptr = sc->shapes[spare(sc)];
    take_spare(sc, push(sc, &c));
}

/* The edit's command, its seq the last of the edits in flight. */
static void send_edit(song_ctl* sc, const gt2_edit* e) {
    song_work* w = sc->w;
    song_cmd c;
    cmd_init(&c, SONG_CMD_EDIT);
    c.u.edit = *e;
    lock_prepare_edit(sc, &c, e);
    uint32_t seq = push(sc, &c);
    if (c.a || sc->edit_restore)
        sc->lock_edit_seq = seq;
    if (sc->inflight_n > 0u && w->inflight_seq[sc->inflight_n - 1u] == 0u) {
        w->inflight_seq[sc->inflight_n - 1u] = seq; /* the held one goes */
    } else {
        w->inflight[sc->inflight_n] = *e;
        w->inflight_seq[sc->inflight_n] = seq;
        sc->inflight_n++;
    }
    w->cur.pending_count = sc->inflight_n;
}

/* The widened shape is whole in the spare: it goes (if it differs), then the held edit. */
static int finish_widen(song_ctl* sc) {
    song_work* w = sc->w;
    if (room(sc) < 2u)
        return 0;
    gt2_shape* next = sc->shapes[spare(sc)];
    const gt2_shape* now = sc->shapes[sc->active];
    if (!same(next, now, sizeof *next)) {
        if (!gt2_shape_compatible(now, next) && !sc->jam_live && sc->state == SONG_PLAYING) {
            /* the player's state cannot follow: the channels it plays stay until a
             * fast-forwarded player takes over with the new shape */
            gt2_shape_keep_channels(next, now->channels);
            sc->want_handover = 1u;
        }
        push_shape(sc);
        sc->widened = 1u;
    }
    send_edit(sc, &w->inflight[sc->inflight_n - 1u]);
    return 1;
}

static void poll_jams(song_ctl* sc) {
    if (!sc->jam_live)
        return;
    uint32_t applied = sc->applied;
    COMPILER_FENCE();
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        uint8_t* wait = sc->jam_wait[ch];
        if (wait[0] != 2u || (int32_t)(applied - sc->jam_seq[ch]) < 0 ||
            sc->jam[ch][0] || sc->player.chn[ch].newnote)
            continue;
        if ((sc->jam_release >> ch) & 1u) {
            if (room(sc) < 1u)
                continue;
            song_cmd c;
            cmd_init(&c, SONG_CMD_JAM);
            c.a = (uint8_t)ch;
            c.b = SONG_JAM_RELEASE;
            push(sc, &c);
            sc->jam_release &= (uint8_t)~(1u << ch);
        }
        wait[0] = 0u;
    }
    if (!sc->jam_live || sc->state == SONG_LOADING || sc->state == SONG_IDLE ||
        sc->deriving || sc->ff_on || sc->spare_wait || sc->inflight_n || sc->want_handover)
        return;
    if (gt2_shape_include_jams(&sc->w->cur, sc->jam_instruments, sc->jam_lo, sc->jam_hi)) {
        sc->bg_on = 0u;
        gt2_shape_derive_begin(&sc->w->cur);
        sc->deriving = sc->deriving_for = SONG_DERIVE_JAM;
        return;
    }
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS && room(sc); ch++) {
        uint8_t* wait = sc->jam_wait[ch];
        if (wait[0] != 1u)
            continue;
        song_cmd c;
        cmd_init(&c, SONG_CMD_JAM);
        c.a = (uint8_t)ch;
        c.b = wait[1];
        c.c = wait[2];
        sc->jam_seq[ch] = push(sc, &c);
        wait[0] = 2u;
    }
}

static int edit_main(song_ctl* sc, const gt2_edit* e) {
    song_work* w = sc->w;
    if (sc->state == SONG_IDLE) {
        if (sc->inflight_n == SONG_INFLIGHT || room(sc) < 1u) {
            sc->busy++;
            return 0;
        }
        send_edit(sc, e);
        sc->edits_submitted++;
        return 1;
    }
    /* A shape command in flight holds the spare a widening may need: the edit waits a pass
     * (the base must not grow without its shape going with the edit). */
    if (sc->state == SONG_LOADING || sc->ff_on || sc->spare_wait || sc->deriving ||
        sc->inflight_n == SONG_INFLIGHT || room(sc) < 2u) {
        sc->busy++;
        return 0;
    }
    w->inflight[sc->inflight_n] = *e;
    w->cur.pending = w->inflight;
    w->cur.pending_count = sc->inflight_n + 1u;
    sc->quiet = 0u;
    if (sc->bg_on)
        sc->bg_dirty = 1u;
    if (gt2_shape_widen(&w->cur, e)) {
        /* The shape again, a slice a poll (a whole derivation is more than a pass has to
         * spare); the edit waits for it, held (seq 0), and goes with it (finish_widen). */
        sc->bg_on = 0u; /* a background analysis gives the spare up */
        w->inflight_seq[sc->inflight_n] = 0u;
        sc->inflight_n++;
        gt2_shape_derive_begin(&w->cur);
        sc->deriving = SONG_DERIVE_EDIT;
        sc->deriving_for = SONG_DERIVE_EDIT;
        sc->edits_submitted++;
        return 1;
    }
    send_edit(sc, e);
    sc->edits_submitted++;
    return 1;
}

int song_ctl_edit_with_locks(song_ctl* sc, const gt2_edit* e, const uint8_t* locks, uint32_t n) {
    if (sc->lock_edit_seq || sc->edit_restore || n > SONG_LOCK_PARAMS ||
        (n && (!locks || e->kind != GT2_EDIT_SWAP || e->slot >= GT2_MAX_PATTERNS))) {
        sc->busy++;
        return 0;
    }
    sc->edit_restore = n ? locks : 0;
    sc->edit_restore_n = (uint8_t)n;
    int result = edit_main(sc, e);
    if (!result) {
        sc->edit_restore = 0;
        sc->edit_restore_n = 0u;
    }
    return result;
}

int song_ctl_edit(song_ctl* sc, const gt2_edit* e) {
    return song_ctl_edit_with_locks(sc, e, 0, 0u);
}

/* The player's frame path runs from SRAM on the device (platform/fm1/app.ld), beyond the
 * direct call range of this side's code in flash: through pointers the compiler cannot fold
 * back into direct calls (as platform/fm1/audio.c's far_drain). */
static void far_start(gt2_player* p, const gt2_song* s, const gt2_shape* sh, const uint8_t* st) {
    void (*volatile fn)(gt2_player*, const gt2_song*, const gt2_shape*, const uint8_t*) =
        gt2_play_start;
    fn(p, s, sh, st);
}

static void far_frame(gt2_player* p, gt2_sid_fn sink, void* ctx) {
    void (*volatile fn)(gt2_player*, gt2_sid_fn, void*) = gt2_play_frame;
    fn(p, sink, ctx);
}

static void start_ff(song_ctl* sc, uint32_t target, uint8_t now) {
    song_work* w = sc->w;
    far_start(&w->ff, sc->song, sc->shapes[now ? sc->active : spare(sc)], sc->start);
    sc->ff_calls = 0u;
    sc->ff_target = target;
    sc->ff_now = now;
    sc->ff_on = 1u;
    sc->ff_sent = 0u;
}

int song_ctl_seek(song_ctl* sc, uint32_t call) {
    if ((sc->state != SONG_PLAYING && sc->state != SONG_STOPPED) || sc->inflight_n || sc->ff_on ||
        sc->ff_stop || sc->deriving)
        return 0;
    start_ff(sc, call, 1u);
    return 1;
}

int song_ctl_seek_further(song_ctl* sc, uint32_t call) {
    if (!sc->ff_on || sc->ff_sent || sc->ff_stop || !sc->ff_now)
        return 0;
    if (call > sc->ff_target)
        sc->ff_target = call;
    return 1;
}

void song_ctl_hold(song_ctl* sc, int on) {
    sc->hold_next = on ? 1u : 0u;
}

int song_ctl_align(song_ctl* sc, uint32_t sample) {
    if (room(sc) < 1u)
        return 0;
    song_cmd c;
    cmd_init(&c, SONG_CMD_ALIGN);
    c.u.p.x = sample;
    push(sc, &c);
    return 1;
}

static void discard(void* ctx, uint8_t reg, uint8_t value) {
    (void)ctx;
    (void)reg;
    (void)value;
}

/* What the interrupt has applied since the last poll. */
static void poll_applied(song_ctl* sc, int may_ring) {
    if (sc->lock_edit_seq && sc->lock_edit_refresh)
        lock_refresh_edit(sc);
    song_work* w = sc->w;
    uint32_t a = sc->applied;
    uint32_t k = 0u;
    while (k < sc->inflight_n && w->inflight_seq[k] != 0u &&
           (int32_t)(a - w->inflight_seq[k]) >= 0) {
        if (w->inflight_seq[k] == sc->lock_edit_seq && sc->edit_restore_n)
            gt2_edit_release_with_locks(sc->song, &w->inflight[k], sc->edit_restore, sc->edit_restore_n);
        else
            gt2_edit_release(sc->song, &w->inflight[k]);
        if (w->inflight_seq[k] == sc->lock_edit_seq) {
            sc->edit_restore = 0;
            sc->edit_restore_n = 0u;
            sc->lock_edit_seq = 0u;
            sc->lock_edit_refresh = 0u;
        }
        sc->edits_completed++;
        k++;
    }
    if (k > 0u) {
        for (uint32_t i = k; i < sc->inflight_n; i++) {
            w->inflight[i - k] = w->inflight[i];
            w->inflight_seq[i - k] = w->inflight_seq[i];
        }
        sc->inflight_n -= k;
        w->cur.pending_count = sc->inflight_n;
    }
    if (sc->spare_wait && (int32_t)(a - sc->spare_free_at) >= 0)
        sc->spare_wait = 0u;
    /* A completed replay owns the spare even if STOP followed it before this poll.
     * Once STOP is acknowledged, no still-armed replay can complete after this read. */
    if (sc->ff_sent && sc->handover_seq == sc->ff_seq) {
        COMPILER_FENCE();
        sc->ff_sent = 0u;
        if (sc->handover_ok) {
            sc->ff_on = 0u;
            if (!sc->ff_now) {
                sc->active = spare(sc);
                sc->want_handover = 0u;
            }
            sc->handovers++;
        }
    }
    if (sc->ff_stop && (int32_t)(a - sc->stop_seq) >= 0) {
        sc->ff_stop = 0u;
        if (sc->ff_on) {
            sc->ff_on = sc->ff_sent = 0u;
            sc->want_handover = 1u;
        }
    }
    if (sc->handback && may_ring && (int32_t)(a - sc->stop_seq) >= 0) {
        sc->handback = 0u;
        sc->chip_held = 0u;
        for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++)
            for (uint32_t f = 0u; f < 4u; f++)
                sc->char_lock[ch][f] = -1;
        lock_character(sc); /* the song's own character again, before the keys take it */
        if (sc->hooks.chip_a_keys)
            sc->hooks.chip_a_keys(sc->hooks.ctx);
        sc->handed_back++;
    }
}

/* One slice of the fast-forward, and its hand-over once it is ahead of the interrupt. */
static void poll_ff(song_ctl* sc) {
    song_work* w = sc->w;
    if (sc->ff_sent || sc->ff_stop || sc->handback)
        return;
    uint32_t target = sc->ff_now ? sc->ff_target : sc->calls + SONG_HANDOVER_LEAD;
    for (uint32_t i = 0u; i < SONG_FF_CALLS && sc->ff_calls < target; i++) {
        far_frame(&w->ff, discard, 0);
        sc->ff_calls++;
    }
    if (sc->ff_calls < target || room(sc) < 1u)
        return;
    song_cmd c;
    cmd_init(&c, SONG_CMD_HANDOVER);
    c.a = sc->ff_now;
    c.u.p.ptr = &w->ff;
    c.u.p.ptr2 = sc->shapes[sc->ff_now ? sc->active : spare(sc)];
    c.u.p.x = sc->ff_calls;
    sc->ff_seq = push(sc, &c);
    sc->ff_sent = 1u;
}

/* The analysis for PLAY, then chip A, then the PLAY command. */
static void poll_loading(song_ctl* sc, int may_slice) {
    song_work* w = sc->w;
    if (!sc->loading_done) {
        if (!may_slice)
            return;
        sc->slices++;
        int r = gt2_shape_step(&w->bg, sc->shapes[spare(sc)], SONG_SLICE_UNITS);
        if (r == GT2_SHAPE_MORE)
            return;
        if (r < 0) {
            sc->error = (int16_t)r;
            /* a stopped player keeps its song and shape; with none, nothing reads them */
            sc->state = sc->shapes_used ? SONG_STOPPED : SONG_IDLE;
            return;
        }
        sc->loading_done = 1u;
        sc->play_sub = sc->shapes[spare(sc)]->subtune;
        w->cur = w->bg;
        w->cur.pending = w->inflight;
        w->cur.pending_count = 0u;
        for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++)
            sc->start[ch] = sc->start_pos ? gt2_shape_order_byte(&w->cur, ch, sc->start_pos) : 0u;
    }
    if (!sc->chip_ready) {
        if (sc->hooks.chip_a_song) {
            song_chip_setup setup;
            int has = song_ext_get_chip(sc->song, SONG_TAG_CHPA, &setup) > 0;
            if (!sc->hooks.chip_a_song(sc->hooks.ctx, has ? &setup : 0))
                return;
        }
        sc->chip_ready = 1u;
        sc->chip_held = 1u;
    }
    if (sc->load_for == LOAD_WAKE) {
        sc->auditioning = 1u; /* the first jam starts the calls */
        sc->state = SONG_STOPPED;
        return;
    }
    if (room(sc) < 1u)
        return;
    if (sc->load_for == LOAD_CONTINUE) {
        song_cmd k;
        cmd_init(&k, SONG_CMD_CONTINUE);
        k.a = gate_for_play(sc);
        k.c = sc->hold_next;
        sc->hold_next = 0u;
        push(sc, &k);
        sc->handback = 0u;
        sc->state = SONG_PLAYING;
        return;
    }
    song_period p = song_period_of(sc->opts.multispeed, sc->opts.ntsc);
    sc->period = p;
    lock_prime(sc);
    song_cmd c;
    cmd_init(&c, SONG_CMD_PLAY);
    c.a = gate_for_play(sc);
    c.b = (uint8_t)(sc->load_for == LOAD_PAUSED);
    c.c = sc->hold_next; /* M2e's clock: the first call waits for an align */
    sc->hold_next = 0u;
    c.u.p.ptr = sc->shapes[spare(sc)];
    c.u.p.ptr2 = (const void*)(uintptr_t)p.den;
    uint8_t* st = (uint8_t*)&c.u.p.x;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++)
        st[ch] = sc->start[ch];
    c.u.p.y = p.q;
    c.u.p.z = p.r;
    take_spare(sc, push(sc, &c));
    sc->shapes_used = 1u;
    sc->widened = 0u;
    sc->want_handover = 0u;
    sc->handback = 0u;
    if (sc->load_for == LOAD_PAUSED) {
        sc->auditioning = 1u;
        sc->state = SONG_STOPPED;
        return;
    }
    sc->state = SONG_PLAYING;
}

/* Narrowing: a fresh analysis of the song as it is, once edits have stopped. */
static void poll_narrow(song_ctl* sc, int may_slice) {
    song_work* w = sc->w;
    if (!sc->bg_on) {
        if (!sc->widened || sc->quiet < SONG_QUIET_POLLS || sc->inflight_n || sc->ff_on ||
            sc->spare_wait || !may_slice)
            return;
        if (gt2_shape_begin(&w->bg, sc->song, sc->subtune, &sc->opts) < 0)
            return;
        gt2_shape_include_jams(&w->bg, sc->jam_instruments, sc->jam_lo, sc->jam_hi);
        sc->bg_on = 1u;
        sc->bg_dirty = 0u;
        return;
    }
    if (!may_slice)
        return;
    sc->slices++;
    int r = gt2_shape_step(&w->bg, sc->shapes[spare(sc)], SONG_SLICE_UNITS);
    if (r == GT2_SHAPE_MORE)
        return;
    sc->bg_on = 0u;
    if (r < 0 || sc->bg_dirty || sc->inflight_n)
        return; /* the song changed under it: again once quiet */
    const gt2_shape* now = sc->shapes[sc->active];
    gt2_shape* next = sc->shapes[spare(sc)];
    w->cur = w->bg;
    w->cur.pending = w->inflight;
    w->cur.pending_count = 0u;
    sc->widened = 0u;
    if (same(next, now, sizeof *next))
        return;
    if (gt2_shape_compatible(now, next) || sc->jam_live || sc->state != SONG_PLAYING) {
        if (room(sc) < 1u) {
            sc->widened = 1u; /* again later */
            return;
        }
        push_shape(sc);
        sc->narrowed++;
    } else {
        start_ff(sc, 0u, 0u); /* the spare holds the fresh shape */
    }
}

/* --- Locks, the main loop's side ----------------------------------------------------------- */

static uint8_t lock_order_byte(const gt2_song* song, uint32_t offset, const gt2_edit* edit) {
    if (edit && edit->kind == GT2_EDIT_CELL && offset >= edit->offset &&
        offset - edit->offset < edit->n)
        return edit->bytes[offset - edit->offset];
    return ((const uint8_t*)song)[offset];
}

/* The pattern channel ch's orderlist plays after model entry pos (the restart's after the
 * last), or -1. An unapplied edit is overlaid without changing the player's model. */
static int next_pattern(const song_ctl* sc, uint32_t ch, uint32_t pos, const gt2_edit* edit) {
    uint32_t sub = sc->play_sub;
    if (edit && edit->slot != GT2_SLOT_ORDER + sub * GT2_MAX_CHANNELS + ch)
        edit = 0;
    gt2_span span = edit && edit->kind == GT2_EDIT_SWAP ? edit->span : sc->song->order[sub][ch];
    if (span == 0u || GT2_SPAN_LEN(span) <= 2u)
        return -1;
    uint32_t n = GT2_SPAN_LEN(span) - 2u;
    uint32_t base = sc->song->arena_offset + GT2_SPAN_OFF4(span) * GT2_UNIT;
    uint32_t i = pos + 1u;
    for (uint32_t step = 0u; step <= n + 1u; step++, i++) {
        if (i >= n) {
            uint8_t restart = lock_order_byte(sc->song, base + n + 1u, edit);
            i = restart < n ? restart : 0u;
        }
        uint8_t value = lock_order_byte(sc->song, base + i, edit);
        if (value < GT2_REPEAT)
            return value;
    }
    return -1;
}

/* A row's filter and pulse locks into set (at most SONG_ROW_LOCKS). */
static void lock_add_row(const song_ctl* sc, song_lock_set* set, uint32_t pattern,
                         uint32_t row, const gt2_edit* edit) {
    uint32_t source = row;
    int empty = 0;
    if (edit && edit->slot == pattern) {
        uint32_t cut = edit->bytes[1];
        if (edit->span == 0u || row + 1u >= GT2_SPAN_LEN(edit->span)) {
            empty = 1;
        } else switch (edit->bytes[0]) {
        case SONG_LOCKS_INSERT:
        case SONG_LOCKS_PUSH:
            if (row == cut)
                empty = 1;
            else if (row > cut)
                source--;
            break;
        case SONG_LOCKS_DELETE:
            if (row >= cut)
                source++;
            break;
        case SONG_LOCKS_REMOVE:
            empty = 1;
            break;
        default:
            break;
        }
    }
    uint32_t n = 0u;
    const uint8_t* rec = empty ? 0 : song_locks_row(sc->song, pattern, source, &n);
    for (uint32_t i = 0u; i < n && set->n < SONG_ROW_LOCKS; i++, rec += SONG_LOCK_BYTES) {
        if (rec[2] > SONG_LOCK_PULSE && rec[2] != SONG_LOCK_CHANCE)
            continue; /* the character's: the hook's */
        song_row_lock* l = &set->e[set->n++];
        l->pattern = rec[0];
        l->row = (uint8_t)row;
        l->param = rec[2];
        l->lo = rec[3];
        l->hi = rec[4];
    }
    if (edit) {
        for (uint32_t i = 0u; i < sc->edit_restore_n && set->n < SONG_ROW_LOCKS; i++) {
            rec = sc->edit_restore + i * SONG_LOCK_BYTES;
            if (rec[0] != pattern || rec[1] != row ||
                (rec[2] > SONG_LOCK_PULSE && rec[2] != SONG_LOCK_CHANCE))
                continue;
            song_row_lock* l = &set->e[set->n++];
            l->pattern = rec[0];
            l->row = rec[1];
            l->param = rec[2];
            l->lo = rec[3];
            l->hi = rec[4];
        }
    }
}

/* The locks channel ch's next row may have after its row (pattern, row, its entry ord),
 * into the table the interrupt does not read, then published. */
static void lock_build(song_ctl* sc, uint32_t ch, uint32_t pattern, uint32_t row,
                       uint32_t ord, int first, const gt2_edit* edit) {
    song_lock_set* set = &sc->lock_next[ch][(sc->lock_pub[ch] & 1u) ^ 1u];
    set->n = 0u;
    uint32_t rows = gt2_pattern_rows(sc->song, pattern) - 1u;
    if (edit && edit->slot == pattern)
        rows = edit->span ? GT2_SPAN_LEN(edit->span) - 1u : 0u;
    if (first) {
        lock_add_row(sc, set, pattern, 0u, edit);
    } else if (row + 1u < rows) {
        lock_add_row(sc, set, pattern, row + 1u, edit);
    } else {
        /* the pattern's end: the pattern again (a repeat), or the orderlist's next */
        lock_add_row(sc, set, pattern, 0u, edit);
        int next = ord != 0xffu ? next_pattern(sc, ch, ord, edit) : -1;
        if (next >= 0 && (uint32_t)next != pattern)
            lock_add_row(sc, set, (uint32_t)next, 0u, edit);
    }
}

static void lock_resolve(song_ctl* sc, uint32_t ch, uint32_t pattern, uint32_t row,
                         uint32_t ord, int first) {
    lock_build(sc, ch, pattern, row, ord, first, 0);
    COMPILER_FENCE(); /* the table before the index that publishes it */
    sc->lock_pub[ch] = (uint8_t)(sc->lock_pub[ch] ^ 1u);
    /* the chances of the steps it holds: a roll each, the losers' keys to the player */
    const song_lock_set* set = &sc->lock_next[ch][sc->lock_pub[ch] & 1u];
    gt2_channel* c = &sc->player.chn[ch];
    uint32_t k = 0u;
    c->skip[0] = c->skip[1] = 0u;
    for (uint32_t i = 0u; i < set->n && k < 2u; i++) {
        const song_row_lock* l = &set->e[i];
        if (l->param == SONG_LOCK_CHANCE &&
            !song_chance_plays(sc->chance_seed, sc->chance_n++, l->lo))
            c->skip[k++] = (uint16_t)((l->pattern + 1u) << 8 | l->row);
    }
}

static void lock_prepare_edit(song_ctl* sc, song_cmd* c, const gt2_edit* e) {
    uint32_t order = GT2_SLOT_ORDER + sc->play_sub * GT2_MAX_CHANNELS;
    int edits_order = e->slot >= order && e->slot < order + GT2_PLAY_CHANNELS &&
                      (e->kind == GT2_EDIT_CELL || e->kind == GT2_EDIT_SWAP);
    int edits_pattern = e->kind == GT2_EDIT_SWAP && e->slot < GT2_MAX_PATTERNS && e->bytes[0];
    if (!sc->shapes_used || (!edits_pattern && !edits_order))
        return;
    uint32_t rows = 0u;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        uint8_t seq, pattern, row, ord;
        do {
            seq = sc->row_seq[ch];
            COMPILER_FENCE();
            pattern = sc->row_pat[ch];
            row = sc->row_row[ch];
            ord = sc->row_ord[ch];
            COMPILER_FENCE();
        } while (seq != sc->row_seq[ch]);
        rows |= (uint32_t)seq << (ch * 8u);
        if (pattern == 0xffu) {
            int first = next_pattern(sc, ch, (uint32_t)sc->start_pos - 1u, e);
            if (first >= 0)
                lock_build(sc, ch, (uint32_t)first, 0u, 0xffu, 1, e);
            else
                sc->lock_next[ch][(sc->lock_pub[ch] & 1u) ^ 1u].n = 0u;
        } else {
            lock_build(sc, ch, pattern, row, ord, 0, e);
        }
        c->b |= (uint8_t)(((sc->lock_pub[ch] & 1u) ^ 1u) << ch);
    }
    sc->lock_edit_rows = rows;
    c->a = 1u;
}

static void lock_refresh_edit(song_ctl* sc) {
    /* The ISR leaves this command queued while refresh is set, so its reserved buffers
     * can be rebuilt without either publishing half a table or racing a second edit. */
    sc->lock_edit_refresh = 1u;
    COMPILER_FENCE();
    if ((int32_t)(sc->applied - sc->lock_edit_seq) < 0) {
        song_cmd* c = &sc->cmd[(sc->lock_edit_seq - 1u) & CMD_MASK];
        lock_prepare_edit(sc, c, &c->u.edit);
    }
    COMPILER_FENCE();
    sc->lock_edit_refresh = 0u;
}

/* At PLAY: each channel's first row, from the play's entry. */
static void lock_prime(song_ctl* sc) {
    sc->chance_n = 0u;
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        int p = next_pattern(sc, ch, (uint32_t)sc->start_pos - 1u, 0);
        sc->lock_next[ch][(sc->lock_pub[ch] & 1u) ^ 1u].n = 0u;
        if (p >= 0)
            lock_resolve(sc, ch, (uint32_t)p, 0u, 0xffu, 1);
        sc->row_seen[ch] = sc->row_seq[ch];
        for (uint32_t f = 0u; f < 4u; f++)
            sc->char_lock[ch][f] = -1;
    }
}

/* The character's locks the channels' rows hold, told to the hook as they change. */
static void lock_character(song_ctl* sc) {
    for (uint32_t f = 0u; f < 4u; f++) {
        int16_t v = -1;
        for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS && v < 0; ch++)
            v = sc->char_lock[ch][f];
        if (v != sc->char_told[f]) {
            sc->char_told[f] = v;
            if (sc->hooks.chip_a_character)
                sc->hooks.chip_a_character(sc->hooks.ctx, f, v);
        }
    }
}

/* The rows the interrupt has started since the last poll. */
static void poll_locks(song_ctl* sc) {
    for (uint32_t ch = 0u; ch < GT2_PLAY_CHANNELS; ch++) {
        uint8_t seq = sc->row_seq[ch];
        if (seq == sc->row_seen[ch])
            continue;
        COMPILER_FENCE();
        uint32_t pattern = sc->row_pat[ch], row = sc->row_row[ch], ord = sc->row_ord[ch];
        COMPILER_FENCE();
        if (sc->row_seq[ch] != seq)
            continue; /* another row started while read: the next poll */
        /* Only lookahead publication waits for the edit's reserved buffers; the current
         * row's character locks still reach their main-loop hook on this pass. The row stays
         * unseen until its look-ahead is resolved: a row that started after this poll's
         * poll_applied, with the edit applied meanwhile, is resolved at the next poll. */
        if (!sc->lock_edit_seq) {
            sc->row_seen[ch] = seq;
            lock_resolve(sc, ch, pattern, row, ord, 0);
        }
        uint32_t n = 0u;
        const uint8_t* rec = song_locks_row(sc->song, pattern, row, &n);
        for (uint32_t f = 0u; f < 4u; f++)
            sc->char_lock[ch][f] = -1;
        for (uint32_t i = 0u; i < n; i++, rec += SONG_LOCK_BYTES)
            if (rec[2] >= SONG_LOCK_AGE && rec[2] <= SONG_LOCK_BLEED)
                sc->char_lock[ch][rec[2] - SONG_LOCK_AGE] = (int16_t)(rec[3] | rec[4] << 8);
    }
    lock_character(sc);
}

/* The DIGI chunk's map for the interrupt (song_ctl.digi_of), the bytes that changed only: an
 * entry the interrupt reads is never 0 on its way to its new value. Built when a play, a
 * continue or an audition starts, and when the extension area's size changes (a NEW load adds
 * the chunk): not every pass, which a MIDI flood makes many. */
static void digi_map(song_ctl* sc) {
    uint8_t map[GT2_MAX_INSTR + 1u];
    song_ext_digi_map(sc->song, map);
    uint8_t any = 0u;
    for (uint32_t i = 0u; i <= GT2_MAX_INSTR; i++) {
        if (sc->digi_of[i] != map[i])
            sc->digi_of[i] = map[i];
        any |= map[i];
    }
    sc->digi_any = any;
    sc->digi_ext = sc->song->ext_used;
}

/* The digi drums' main-loop side (song_ctl.h, song_ctl_block_end), once a poll: the voices
 * whose digi drums may sound, and the mute of the voices whose instrument is a digi drum, from
 * the notes the interrupt reports (a second reader of the event ring, which leaves
 * song_ctl_events' alone). A song with no digi drum skips the events: a few loads a pass. */
static void digi_poll(song_ctl* sc) {
    if (sc->song->ext_used != sc->digi_ext)
        digi_map(sc);
    uint32_t head = sc->ev_head;
    COMPILER_FENCE(); /* the head before the events it publishes */
    if (sc->digi_any) {
        uint32_t t = head - sc->digi_ev > SONG_EVENT_RING ? head - SONG_EVENT_RING : sc->digi_ev;
        for (; t != head; t++) {
            const song_event* e = &sc->ev[t & EV_MASK];
            if (SONG_EV_FLAGS(e) & GT2_EV_NOTE) {
                uint8_t bit = (uint8_t)(1u << SONG_EV_CHANNEL(e));
                sc->digi_hold = (uint8_t)(sc->digi_of[e->instr] ? sc->digi_hold | bit
                                                               : sc->digi_hold & ~bit);
            }
        }
    }
    sc->digi_ev = head;
    sc->digi_ok = (uint8_t)(~(sc->mute_user | sc->take) & 7u);
    if ((uint8_t)(sc->mute_user | sc->digi_hold) != sc->mute_sent && room(sc) >= 1u)
        push_mute(sc, (uint8_t)(sc->mute_user | sc->digi_hold));
}

void song_ctl_poll(song_ctl* sc, uint32_t flags) {
    int may_slice = (flags & SONG_POLL_SLICE) != 0;
    digi_poll(sc);
    poll_applied(sc, (flags & SONG_POLL_RING) != 0);
    poll_jams(sc);
    if (sc->quiet < 0xffffffffu)
        sc->quiet++;
    switch (sc->state) {
    case SONG_LOADING:
        poll_loading(sc, may_slice);
        break;
    case SONG_PLAYING:
    case SONG_STOPPED:
        if (sc->state == SONG_PLAYING)
            poll_locks(sc);
        if (sc->deriving) {
            if (!may_slice)
                break;
            sc->slices++;
            if (sc->deriving == SONG_DERIVE_DONE ||
                gt2_shape_derive_step(&sc->w->cur, sc->shapes[spare(sc)], SONG_SLICE_UNITS)) {
                sc->deriving = SONG_DERIVE_DONE;
                if (sc->deriving_for == SONG_DERIVE_HANDOVER &&
                    (sc->jam_live || sc->state != SONG_PLAYING)) {
                    sc->deriving_for = SONG_DERIVE_JAM;
                    sc->want_handover = 0u;
                }
                if (sc->deriving_for == SONG_DERIVE_HANDOVER) {
                    start_ff(sc, 0u, 0u);
                    sc->deriving = 0u;
                } else if (sc->deriving_for == SONG_DERIVE_JAM) {
                    if (room(sc)) {
                        if (!same(sc->shapes[spare(sc)], sc->shapes[sc->active], sizeof(gt2_shape)))
                            push_shape(sc);
                        sc->widened = 1u;
                        sc->deriving = 0u;
                    }
                } else if (finish_widen(sc)) {
                    sc->deriving = 0u;
                }
            }
            break;
        }
        if (sc->want_handover && !sc->ff_on && !sc->inflight_n && !sc->spare_wait) {
            /* the true shape, into the spare, for the fast-forward: a slice a poll */
            gt2_shape_derive_begin(&sc->w->cur);
            sc->deriving = sc->deriving_for = sc->jam_live || sc->state != SONG_PLAYING
                ? SONG_DERIVE_JAM : SONG_DERIVE_HANDOVER;
            if (sc->deriving_for == SONG_DERIVE_JAM)
                sc->want_handover = 0u;
            break;
        }
        if (sc->ff_on) {
            if (may_slice) {
                sc->slices++;
                poll_ff(sc);
            }
        } else {
            poll_narrow(sc, may_slice);
        }
        break;
    default:
        break;
    }
}

uint32_t song_ctl_events(song_ctl* sc, song_event* out, uint32_t n) {
    uint32_t got = 0u;
    while (got < n) {
        uint32_t h = sc->ev_head;
        COMPILER_FENCE();
        if (h - sc->ev_tail > SONG_EVENT_RING) {
            sc->ev_lost += h - sc->ev_tail - SONG_EVENT_RING;
            sc->ev_tail = h - SONG_EVENT_RING;
        }
        if (sc->ev_tail == h)
            break;
        song_event e = sc->ev[sc->ev_tail & EV_MASK];
        COMPILER_FENCE();
        if (sc->ev_head - sc->ev_tail > SONG_EVENT_RING)
            continue; /* overwritten while read: count it and go on from the oldest left */
        out[got++] = e;
        sc->ev_tail++;
    }
    return got;
}

uint32_t song_ctl_state(const song_ctl* sc) {
    return sc->state;
}

uint32_t song_ctl_warnings(const song_ctl* sc) {
    uint32_t w = sc->shapes[sc->active]->warnings;
    if (sc->past_b7)
        w |= GT2_PLAY_WARN_PAST_B7;
    return w;
}
