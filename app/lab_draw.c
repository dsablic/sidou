// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_draw.h"

#include <string.h>

#include "font.h"
#include "fx/fx_slots.h"
#include "lab_view.h"
#include "live_view.h"
#include "sid/sid_filter.h"
#include "sine.h"
#include "views.h"

_Static_assert(LAB_DRAW_ROWS == LAB_VIEW_ART_ROWS, "the drawing fills LAB's area");
_Static_assert(LAB_DRAW_H <= LAB_DRAW_NONE, "a row of the area fits a byte, NONE apart");
_Static_assert(LAB_DRAW_STEP == 2u, "a sample is two columns (the kit's 0xC0)");

/* ---- the kit ---- */

int lab_draw_apply(const lab_draw* d, grid* g, const screen* s, uint32_t row0, uint32_t* left) {
    uint8_t prev_lo[LAB_DRAW_LAYERS] = {LAB_DRAW_NONE, LAB_DRAW_NONE, LAB_DRAW_NONE};
    uint8_t prev_hi[LAB_DRAW_LAYERS] = {0, 0, 0};
    int whole = 1;
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        /* The band's 8 columns into each cell row's mask, a span's pixels at a time; has[r]: bit
         * l, layer l inks the cell, bit 3 a lit pixel. */
        uint32_t words[LAB_DRAW_ROWS * 2u + (LAB_DRAW_ROWS + 3u) / 4u];
        for (uint32_t k = 0; k < sizeof words / sizeof words[0]; k++) {
            words[k] = 0; /* by words: the device's memset goes a byte at a time */
        }
        uint8_t (*m)[8] = (uint8_t (*)[8])words;
        uint8_t* has = (uint8_t*)&words[LAB_DRAW_ROWS * 2u];
        for (uint32_t j = 0; j < 8u; j += LAB_DRAW_STEP) {
            lab_draw_col col;
            d->fn(d->ctx, c * 8u + j, &col);
            uint8_t bit = (uint8_t)(0xC0u >> j); /* the sample's two columns */
            for (uint32_t l = 0; l < LAB_DRAW_LAYERS; l++) {
                uint32_t a = col.lo[l];
                uint32_t b = col.hi[l];
                if (a == LAB_DRAW_NONE) {
                    prev_lo[l] = LAB_DRAW_NONE;
                    continue;
                }
                if (l != LAB_DRAW_MARKER) {
                    /* a trace: joined to the column before, a pixel thicker below */
                    uint32_t ja = a;
                    uint32_t jb = b;
                    if (prev_lo[l] != LAB_DRAW_NONE) {
                        ja = prev_lo[l] < ja ? prev_lo[l] : ja;
                        jb = prev_hi[l] > jb ? prev_hi[l] : jb;
                    }
                    prev_lo[l] = (uint8_t)a;
                    prev_hi[l] = (uint8_t)b;
                    a = ja;
                    b = jb + 1u;
                }
                b = b < LAB_DRAW_H - 1u ? b : LAB_DRAW_H - 1u;
                for (uint32_t y = a; y <= b; y++) {
                    m[y >> 3][y & 7u] |= bit;
                }
                uint8_t flags = (uint8_t)(1u << l | (col.lit >> l & 1u) << 3);
                for (uint32_t r = a >> 3; r <= b >> 3; r++) {
                    has[r] |= flags;
                }
            }
        }
        for (uint32_t r = 0; r < LAB_DRAW_ROWS; r++) {
            uint32_t row = row0 + r;
            uint32_t h = has[r];
            if (!h && !g->ovl[row * GRID_COLS + c]) {
                continue; /* empty, and nothing drawn there */
            }
            uint8_t color = h & 4u   ? d->color[LAB_DRAW_MARKER]
                            : h & 8u ? d->lit_color
                            : h & 1u ? d->color[0]
                                     : d->color[1];
            if (grid_overlay_same(g, c, row, m[r], color)) {
                continue;
            }
            if (!*left || screen_covered(s, c, row)) {
                whole = 0;
                continue;
            }
            if (!h) {
                grid_overlay_clear(g, c, row);
            } else if (grid_overlay(g, c, row, m[r], color) != 0) {
                whole = 0; /* no slot left: the cell keeps its glyph alone */
            }
            (*left)--;
        }
    }
    return whole;
}

/* ---- ENVELOPE's time and levels ---- */

/* 1000 x 10^(i / LAB_DRAW_DECADE): a decade of the time axis. */
static const uint16_t DECADE[LAB_DRAW_DECADE] = {
    1000, 1049, 1101, 1155, 1212, 1271, 1334, 1399, 1468, 1540, 1616, 1695, 1778, 1866, 1957, 2054,
    2154, 2260, 2371, 2488, 2610, 2738, 2873, 3014, 3162, 3318, 3481, 3652, 3831, 4019, 4217, 4424,
    4642, 4870, 5109, 5360, 5623, 5900, 6190, 6494, 6813, 7148, 7499, 7867, 8254, 8660, 9085, 9532};
/* The SID's rate periods, in cycles (the datasheet's 2 ms .. 8 s attacks, three times as long for
 * a decay or release from 255 to 0). */
static const uint16_t PERIOD[16] = {9,   32,   63,   95,   149,   220,   267,   313,
                                    392, 977, 1954, 3126, 3907, 11720, 19532, 31251};
/* The envelope's exponential steps on the way down: a step from a level of at least LO[i] takes
 * P[i] rate periods (reSID's exponential counter). 756 periods from 255 to 0. */
static const uint8_t SEG_LO[6] = {94, 55, 27, 15, 7, 1};
static const uint8_t SEG_P[6] = {1, 2, 4, 8, 16, 30};

uint32_t lab_draw_time_us(uint32_t x) {
    if (x <= LAB_DRAW_T0) {
        return 0;
    }
    uint32_t u = x - LAB_DRAW_T0;
    uint32_t t = DECADE[u % LAB_DRAW_DECADE];
    for (uint32_t d = u / LAB_DRAW_DECADE; d; d--) {
        t *= 10u;
    }
    return t - 1000u; /* 1 ms at T0 is the axis' 0 */
}

uint32_t lab_draw_x_of_us(uint32_t t) {
    uint32_t lo = 0;
    uint32_t hi = LAB_DRAW_W - 1u;
    while (lo < hi) {
        uint32_t mid = (lo + hi + 1u) / 2u;
        if (lab_draw_time_us(mid) <= t) {
            lo = mid;
        } else {
            hi = mid - 1u;
        }
    }
    return lo;
}

/* A period's microseconds are 1.015 of its cycles at the PAL clock (985,248 Hz). */
uint32_t lab_draw_attack_us(uint32_t rate) {
    return PERIOD[rate & 15u] * 25882u / 100u; /* 255 periods */
}

uint32_t lab_draw_decay_us(uint32_t rate) {
    return PERIOD[rate & 15u] * 767u; /* 756 periods */
}

/* Rate periods (weighted steps) in t microseconds of rate `period`. */
static uint32_t periods(uint32_t t, uint32_t period) {
    return t / period * 64u / 65u;
}

/* The level after w rate periods down from level l. */
static uint32_t level_down(uint32_t l, uint32_t w) {
    for (uint32_t i = 0; i < 6u; i++) {
        if (l < SEG_LO[i]) {
            continue;
        }
        uint32_t cost = (l - SEG_LO[i] + 1u) * SEG_P[i];
        if (w < cost) {
            return l - w / SEG_P[i];
        }
        w -= cost;
        l = SEG_LO[i] - 1u;
    }
    return 0;
}

/* Rate periods from level hi down to level lo. */
static uint32_t periods_between(uint32_t hi, uint32_t lo) {
    uint32_t w = 0;
    uint32_t top = 255;
    for (uint32_t i = 0; i < 6u; i++) {
        /* levels c in [SEG_LO[i], top] each take SEG_P[i] to step down */
        uint32_t a = lo + 1u > SEG_LO[i] ? lo + 1u : SEG_LO[i];
        uint32_t b = hi < top ? hi : top;
        if (b >= a) {
            w += (b - a + 1u) * SEG_P[i];
        }
        top = SEG_LO[i] - 1u;
    }
    return w;
}

/* An envelope's settings, worked out once a drawing. */
typedef struct {
    uint32_t attack;  /* microseconds to 255 */
    uint32_t decay;   /* the decay's rate period */
    uint32_t release; /* the release's */
    uint32_t sustain; /* the level, 0..255 */
    uint32_t gate;    /* the key let go, microseconds */
    uint32_t at_gate; /* the level then */
} env_shape;

static uint32_t held_level(const env_shape* e, uint32_t t) {
    if (t < e->attack) {
        return 255u * t / e->attack;
    }
    uint32_t l = level_down(255, periods(t - e->attack, e->decay));
    return l > e->sustain ? l : e->sustain;
}

static void env_init(env_shape* e, uint8_t ad, uint8_t sr, uint32_t gate_us) {
    e->attack = lab_draw_attack_us(ad >> 4);
    e->decay = PERIOD[ad & 15u];
    e->release = PERIOD[sr & 15u];
    e->sustain = (sr >> 4) * 17u;
    e->gate = gate_us;
    e->at_gate = held_level(e, gate_us);
}

static uint32_t env_level(const env_shape* e, uint32_t t) {
    return t < e->gate ? held_level(e, t)
                       : level_down(e->at_gate, periods(t - e->gate, e->release));
}

uint32_t lab_draw_gate_us(uint8_t ad, uint8_t sr) {
    uint32_t sustain = (sr >> 4) * 17u;
    uint32_t t = lab_draw_attack_us(ad >> 4);
    if (sustain < 255u) {
        t += periods_between(255, sustain) * PERIOD[ad & 15u] / 64u * 65u;
    }
    t += t / 2u; /* the sustain held half as long again */
    uint32_t mid = lab_draw_time_us(LAB_DRAW_GATE_X);
    uint32_t last = lab_draw_time_us(LAB_DRAW_GATE_MAX_X);
    return t < mid ? mid : t > last ? last : t;
}

uint32_t lab_draw_level(uint8_t ad, uint8_t sr, uint32_t gate_us, uint32_t t) {
    env_shape e;
    env_init(&e, ad, sr, gate_us);
    return env_level(&e, t);
}

/* ---- the drawings ---- */

/* The plot's rows of the area: a level or a wave's 0..255 from the bottom row to the top. */
#define OSC_TOP 6u
#define OSC_SPAN 99u
#define ENV_TOP 4u
#define ENV_SPAN 96u
/* OSC's cycle: from OSC_X0, OSC_CYCLE pixels at the note's pitch. */
#define OSC_X0 8u
#define OSC_CYCLE 224u
/* The carrier's ratio to the modulator for ring or sync on a sound of four voices (the modulator
 * is another key's), in 1/256. */
#define OSC_RATIO 384u

static uint8_t y_of(uint32_t v, uint32_t top, uint32_t span) {
    return (uint8_t)(top + (255u - v) * span / 255u);
}

typedef struct {
    uint32_t cycle; /* pixels */
    uint32_t ratio; /* the carrier's, 1/256 */
    uint16_t pw;
    uint8_t control;
    uint8_t mod;    /* 1 ring, 2 sync: the modulator's trace */
    uint8_t hl;
} osc_ctx;

/* The carrier's 8-bit output at the modulator's phase p (0..65535) and pixel x: the shapes its
 * waveform bits select, ANDed (the 6581's combined waveforms); none: 0. */
static uint32_t osc_wave(const osc_ctx* o, uint32_t p, uint32_t x) {
    uint32_t q = p * o->ratio >> 8 & 0xFFFFu;
    uint32_t c = o->control;
    uint32_t v = 255;
    if (c & 0x10u) {
        uint32_t t = q < 32768u ? q >> 7 : (65535u - q) >> 7;
        v &= o->mod == 1u && p >= 32768u ? 255u - t : t; /* ring: the modulator's top bit */
    }
    if (c & 0x20u) {
        v &= q >> 8;
    }
    if (c & 0x40u) {
        v &= (q >> 4) >= o->pw ? 255u : 0u;
    }
    if (c & 0x80u) {
        /* a fixed scribble about the middle: a random level every 4 pixels, joined (at full
         * height the trace would ink every cell, past the grid's overlays) */
        uint32_t k = x / 4u;
        uint32_t a = (k * 2654435761u) >> 26;
        uint32_t b = ((k + 1u) * 2654435761u) >> 26;
        v &= 96u + a + (b - a) * (x % 4u) / 4u;
    }
    return c & 0xF0u ? v : 0u;
}

static void osc_col(const void* ctx, uint32_t x, lab_draw_col* out) {
    const osc_ctx* o = ctx;
    out->lo[0] = out->lo[1] = out->lo[2] = LAB_DRAW_NONE;
    out->lit = 0;
    if (o->hl == 4u && (x == OSC_X0 + OSC_CYCLE - 1u || x == OSC_X0 + OSC_CYCLE - 2u)) {
        out->lo[LAB_DRAW_MARKER] = OSC_TOP; /* the note's own cycle, against the tuned one */
        out->hi[LAB_DRAW_MARKER] = OSC_TOP + OSC_SPAN;
    }
    if (x < OSC_X0 || x >= OSC_X0 + o->cycle + LAB_DRAW_STEP) {
        return;
    }
    /* the sample after the cycle is the next one's start: a saw's drop, a pulse's last edge */
    uint32_t p = x >= OSC_X0 + o->cycle ? 0u : (x - OSC_X0) * 65536u / o->cycle;
    uint32_t v = osc_wave(o, p, x);
    out->lo[0] = out->hi[0] = y_of(v, OSC_TOP, OSC_SPAN);
    if (o->hl == 1u && (o->control & 0x40u) && x >= OSC_X0 + LAB_DRAW_STEP && p) {
        uint32_t q = p * o->ratio >> 8 & 0xFFFFu;
        uint32_t b = ((x - LAB_DRAW_STEP - OSC_X0) * 65536u / o->cycle) * o->ratio >> 8 & 0xFFFFu;
        if (((q >> 4) >= o->pw) != ((b >> 4) >= o->pw)) {
            out->lit = 1; /* the pulse's edge */
        }
    }
    if (o->mod) {
        uint32_t m = o->mod == 2u ? p >> 8 : p >= 32768u ? 255u : 0u;
        out->lo[1] = out->hi[1] = y_of(m, OSC_TOP, OSC_SPAN);
    }
}

/* The pulse width a sounding voice has now (the LFO's), else the sound's. */
static uint32_t live_pw(const live* l) {
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        if (l->voice[v].gate) {
            return l->voice[v].pw_out;
        }
    }
    return l->pulse_width;
}

static void osc_init(osc_ctx* o, const live* l, uint32_t hl) {
    o->control = l->control;
    o->pw = (uint16_t)live_pw(l);
    o->mod = l->control & 0x04u ? 1u : l->control & 0x02u ? 2u : 0u;
    uint32_t r = l->mono_sync ? l->sync_q8 : o->mod ? OSC_RATIO : 256u;
    o->ratio = r < 256u ? 256u : r > 2048u ? 2048u : r;
    /* a cent's 1/1731 of the cycle: 2^(c / 1200) within 0.05 % at 50 */
    o->cycle = OSC_CYCLE * 65536u / (uint32_t)(65536 + l->tune * 38);
    o->hl = (uint8_t)hl;
}

typedef struct {
    env_shape e;
    uint8_t hl;
    uint8_t dot_x; /* LAB_DRAW_NONE: no dot */
    uint8_t dot_y;
} env_ctx;

/* The segment of time t: 1 the attack, 2 the decay, 3 the sustain, 4 the release (the cards'). */
static uint32_t env_segment(const env_shape* e, uint32_t t, uint32_t level) {
    if (t >= e->gate) {
        return 4;
    }
    if (t < e->attack) {
        return 1;
    }
    return level > e->sustain ? 2u : 3u;
}

static void env_col(const void* ctx, uint32_t x, lab_draw_col* out) {
    const env_ctx* c = ctx;
    out->lo[0] = out->lo[1] = out->lo[2] = LAB_DRAW_NONE;
    uint32_t t = lab_draw_time_us(x);
    uint32_t level = env_level(&c->e, t);
    if (t < c->e.attack && t < c->e.gate) {
        /* the attack rises across the sample: its top is where the sample's time ends (a
         * pixel is 5 % of the time, 400 ms at 8 s: a point alone could miss the peak) */
        uint32_t end = lab_draw_time_us(x + LAB_DRAW_STEP);
        end = end < c->e.attack ? end : c->e.attack;
        end = end < c->e.gate ? end : c->e.gate;
        uint32_t top = held_level(&c->e, end);
        level = top > level ? top : level;
    }
    out->lo[0] = out->hi[0] = y_of(level, ENV_TOP, ENV_SPAN);
    out->lit = (uint8_t)(c->hl && env_segment(&c->e, t, level) == c->hl);
    if (c->dot_x != LAB_DRAW_NONE && x + 3u >= c->dot_x && x <= c->dot_x + 3u) {
        /* a white block of about 7 pixels on the light grey curve */
        out->lo[LAB_DRAW_MARKER] = (uint8_t)(c->dot_y > 3u ? c->dot_y - 3u : 0u);
        out->hi[LAB_DRAW_MARKER] = (uint8_t)(c->dot_y + 3u);
    }
}

/* The voice the tap's envelope is followed for: the loudest (0..2), or -1 with none. */
static int32_t loudest(const scope* sc) {
    int32_t v = -1;
    uint32_t most = 0;
    for (uint32_t k = 0; sc && k < SCOPE_VOICES; k++) {
        if (sc->env[k] > most) {
            most = sc->env[k];
            v = (int32_t)k;
        }
    }
    return v;
}

/* Where the envelope the tap reads is on the curve: by its state and level. */
static void env_dot(env_ctx* c, const scope* sc) {
    c->dot_x = LAB_DRAW_NONE;
    int32_t v = loudest(sc);
    if (v < 0) {
        return;
    }
    const env_shape* e = &c->e;
    uint32_t level = sc->env[v];
    uint32_t t;
    if (sc->state[v] == SID_ENV_ATTACK) {
        t = e->attack * level / 255u;
    } else if (sc->state[v] == SID_ENV_RELEASE) {
        t = e->gate + (level < e->at_gate ? periods_between(e->at_gate, level) * e->release / 64u * 65u : 0u);
    } else {
        t = e->attack + periods_between(255, level) * e->decay / 64u * 65u;
        uint32_t last = lab_draw_x_of_us(e->gate);
        last = lab_draw_time_us(last > 3u ? last - 3u : 0u); /* on the plateau, before the key */
        t = t < last ? t : last;
    }
    c->dot_x = (uint8_t)lab_draw_x_of_us(t);
    c->dot_y = y_of(level, ENV_TOP, ENV_SPAN);
}

/* ---- FILTER ---- */

/* The response's columns: from FILT_X0 at 30 Hz, FILT_PX_OCT a octave, to 12 kHz at FILT_X1;
 * its rows: +6 dB at the top to -54 dB at FILT_SPAN. */
#define FILT_X0 8u
#define FILT_X1 232u
#define FILT_PX_OCT 26u
#define FILT_SPAN 88u
#define FILT_REACH 24u

/* 2^(n / 26) in Q12 (n signed): a half a column's step of the axis, and the effects' rates. */
static const uint16_t EXP2[26] = {4096, 4207, 4320, 4437, 4557, 4680, 4806, 4936, 5070,
                                  5207, 5347, 5492, 5640, 5793, 5949, 6110, 6275, 6444,
                                  6619, 6797, 6981, 7170, 7363, 7562, 7767, 7976};
/* log2(1 + i / 16) in Q8. */
static const uint16_t LOG2[17] = {0,   22,  44,  63,  82,  100, 118, 134, 150,
                                  165, 179, 193, 207, 220, 232, 244, 256};

static uint32_t exp2q(int32_t n) {
    int32_t k = n >= 0 ? n / 26 : -((25 - n) / 26);
    uint32_t t = EXP2[n - k * 26];
    return k >= 0 ? t << (k < 18 ? k : 18) : -k < 32 ? t >> -k : 0u;
}

/* log2 v in Q8 (0 for 0). */
static int32_t log2q8(uint64_t v) {
    if (!v) {
        return 0;
    }
    uint32_t p = 63u - (uint32_t)__builtin_clzll(v);
    uint32_t m = (uint32_t)(p >= 8u ? v >> (p - 8u) : v << (8u - p)); /* 256..511 */
    uint32_t i = (m - 256u) >> 4;
    uint32_t f = m & 15u;
    return (int32_t)(p * 256u + LOG2[i] + (LOG2[i + 1u] - LOG2[i]) * f / 16u);
}

typedef struct {
    uint16_t xc;   /* the cutoff's column */
    uint16_t invq; /* the damping 1 / Q in Q8 */
    uint8_t mode;  /* $d418 bits 4-6 shifted down: 1 LP, 2 BP, 4 HP */
    uint8_t hl;
} filt_ctx;

static void filt_col(const void* ctx, uint32_t x, lab_draw_col* out) {
    const filt_ctx* f = ctx;
    out->lo[0] = out->lo[1] = out->lo[2] = LAB_DRAW_NONE;
    out->lit = 0;
    if (x < FILT_X0 || x > FILT_X1) {
        return;
    }
    int32_t level = 0; /* log2 of the power gain, Q8 */
    if (f->mode) {
        int32_t d = ((int32_t)x - f->xc) * 2;
        uint32_t w = exp2q(d); /* (f / fc)^2 in Q12 */
        int64_t re = (f->mode & 1u ? 4096 : 0) - (f->mode & 4u ? (int64_t)w : 0);
        uint64_t num = (uint64_t)(re * re) + (f->mode & 2u ? (uint64_t)w * 4096u : 0u);
        int64_t one = 4096 - (int64_t)w;
        uint64_t den = (uint64_t)(one * one) + (uint64_t)w * f->invq * f->invq / 16u;
        level = num ? log2q8(num) - log2q8(den) : -20 * 256;
    }
    int32_t y = (512 - level) * (int32_t)FILT_SPAN / 5120;
    y = y < 0 ? 0 : y > (int32_t)FILT_SPAN - 1 ? (int32_t)FILT_SPAN - 1 : y;
    out->lo[0] = out->hi[0] = (uint8_t)y;
    if (f->hl == 2u && x + FILT_REACH >= f->xc && x <= f->xc + FILT_REACH) {
        out->lit = 1; /* the resonance: the curve about the cutoff */
    }
    if (f->mode && (x & ~1u) == (f->xc & ~1u)) {
        out->lo[1] = 0; /* the cutoff: a line down the area */
        out->hi[1] = FILT_SPAN - 2u;
        out->lit |= (uint8_t)((f->hl == 1u) << 1);
    }
}

static void filt_init(filt_ctx* f, const live* l, uint32_t hl) {
    float hz = sid_cutoff_hz(l->model, 0, l->cutoff_out);
    uint32_t h = (uint32_t)(hz < 30.0f ? 30.0f : hz > 12000.0f ? 12000.0f : hz);
    f->xc = (uint16_t)(FILT_X0 + (uint32_t)(log2q8(h) - log2q8(30)) * FILT_PX_OCT / 256u);
    float k = sid_resonance_k(l->model, 0, l->resonance) * 256.0f;
    f->invq = (uint16_t)(k > 512.0f ? 512.0f : k);
    f->mode = l->filter_mode >> 4 & 7u;
    f->hl = (uint8_t)hl;
}

/* ---- MOD ---- */

/* One window of LAB_DRAW_MOD_TICKS ticks (100 Hz) over the columns from MOD_X0: the vibrato
 * (band A), the PWM (band B) and the arpeggio's staircase (band C), their centres and bases. */
#define MOD_X0 32u
#define MOD_W 204u
#define MOD_TICKS 128u
#define MOD_A_CY 18u
#define MOD_B_CY 54u
#define MOD_C_BASE 102u
#define MOD_AMP 14u
/* Most steps ARP has in the window: the fastest rate, 1 frame (32 in 1/16 ticks). */
#define MOD_STEPS 65u

typedef struct {
    uint16_t vstep, pstep; /* LFO phase a tick (65536 a cycle) */
    uint16_t cycle;        /* the vibrato's cycle in 1/16 ticks (0: none) */
    uint16_t arp_inv;      /* 65536 / a step's 1/16 ticks, rounded up (0: ARP off) */
    uint8_t vamp;          /* the vibrato's swing in pixels */
    uint8_t pamp;          /* the PWM's */
    uint8_t delay;         /* the vibrato's, in ticks */
    uint8_t hl;
    uint8_t stairs[MOD_STEPS]; /* the arpeggio's row at each of the window's steps */
} mod_ctx;

/* The LFO's triangle at a phase: -16384..16384, up first. */
static int32_t tri(uint32_t phase) {
    int32_t p = (int32_t)(phase & 0xFFFFu);
    return p < 16384 ? p : p < 49152 ? 32768 - p : p - 65536;
}

/* The arpeggio's note at step idx, of a C major chord (played G C E), in semitones. */
static uint32_t arp_semis(const live* l, uint32_t idx) {
    static const uint8_t SORTED[3] = {0, 4, 7};
    static const uint8_t PLAYED[3] = {7, 0, 4};
    uint32_t n = 3u * (l->arp_span ? l->arp_span : 1u);
    uint32_t order = l->arp_order & 3u;
    uint32_t period = order == LIVE_ARP_UP_DOWN && n > 1u ? 2u * n - 2u : n;
    uint32_t q = idx % period;
    if (order == LIVE_ARP_UP_DOWN && q >= n) {
        q = 2u * n - 2u - q;
    } else if (order == LIVE_ARP_DOWN) {
        q = n - 1u - q;
    }
    return (order == LIVE_ARP_PLAYED ? PLAYED : SORTED)[q % 3u] + 12u * (q / 3u);
}

static void mod_col(const void* ctx, uint32_t x, lab_draw_col* out) {
    const mod_ctx* m = ctx;
    out->lo[0] = out->lo[1] = out->lo[2] = LAB_DRAW_NONE;
    out->lit = 0;
    if (x < MOD_X0 || x >= MOD_X0 + MOD_W) {
        return;
    }
    uint32_t t16 = (x - MOD_X0) * 10u; /* ticks in 1/16: the window is 127.5 ticks */
    uint32_t dq = (uint32_t)m->delay * 16u;
    int32_t v = 0;
    if (t16 >= dq) {
        v = tri((t16 - dq) * m->vstep >> 4) * m->vamp >> 14;
    }
    out->lo[0] = out->hi[0] = (uint8_t)((int32_t)MOD_A_CY - v);
    out->lit = (uint8_t)(m->hl == 2u || (m->hl == 3u && t16 < dq) ||
                         (m->hl == 1u && t16 >= dq && t16 < dq + m->cycle));
    out->lo[1] = out->hi[1] = (uint8_t)((int32_t)MOD_B_CY - (tri(t16 * m->pstep >> 4) * m->pamp >> 14));
    if (m->arp_inv) {
        uint32_t a = m->stairs[t16 * m->arp_inv >> 16];
        uint32_t b = m->stairs[(t16 + 20u) * m->arp_inv >> 16]; /* two columns on */
        out->lo[LAB_DRAW_MARKER] = (uint8_t)(a < b ? a : b);
        out->hi[LAB_DRAW_MARKER] = (uint8_t)((a > b ? a : b) + 1u);
    }
}

static void mod_init(mod_ctx* m, const live* l, uint32_t hl) {
    m->vstep = (uint16_t)(((uint32_t)l->vibrato_rate * 65536u + 500u) / 1000u);
    m->pstep = l->pwm_rate;
    m->vamp = (uint8_t)(l->vibrato_depth * MOD_AMP / LIVE_EXPRESSION_MAX);
    m->cycle = l->vibrato_rate ? (uint16_t)(1000u * 16u / l->vibrato_rate) : 0u;
    m->pamp = (uint8_t)(l->pwm_depth * MOD_AMP / LIVE_PWM_DEPTH_MAX);
    m->delay = l->vibrato_delay;
    m->hl = (uint8_t)hl;
    m->arp_inv = 0;
    if (l->arp_on) {
        uint32_t step = live_arp_steps[l->arp_rate % LIVE_ARP_RATES] * 2u * 16u;
        m->arp_inv = (uint16_t)((65536u + step - 1u) / step);
        for (uint32_t i = 0; i < MOD_STEPS; i++) {
            m->stairs[i] = (uint8_t)(MOD_C_BASE - arp_semis(l, i) * 3u / 2u);
        }
    }
}

/* ---- FX ---- */

/* The selected slot's shape in the rows from FX_Y0, FX_H high: a transfer curve in a square of
 * FX_H + 1 columns from FX_PX0, a wave over the MOD window's columns, the delay's taps and the
 * reverb's decay over the OSC's. */
#define FX_Y0 48u
#define FX_H 55u
#define FX_PX0 92u
#define FX_WAVE_MS 2000u
#define FX_TAPS_MS 1500u
#define FX_X0 8u
#define FX_W 224u

enum { FX_NONE, FX_XFER, FX_WAVE, FX_TAPS, FX_DECAY };

typedef struct {
    uint32_t hz_q8; /* a wave's rate, 1/256 Hz */
    uint16_t gap;   /* the taps' distance, pixels */
    uint8_t kind;
    uint8_t slot, alg;
    uint8_t amount, a, b;
    uint8_t hl;
} fx_ctx;

/* The distortion's output (-256..256) for an input x of the same range. */
static int32_t fx_xfer(const fx_ctx* f, int32_t x) {
    int32_t a = f->amount * 256 / 127;
    int32_t b = f->b * 256 / 127;
    if (f->alg == SID_DIST_CRUSH) {
        int32_t bits = 16 - 12 * a / 256;
        int32_t n = bits >= 6 ? 64 : 1 << bits;
        int32_t i = (x + 256) * n >> 9;
        i = i < n - 1 ? i : n - 1;
        return i * 512 / (n - 1) - 256;
    }
    int32_t g;
    int32_t bias;
    if (f->alg == SID_DIST_TAPE) {
        g = 256 + 2 * a;
        bias = a * 38 / 256;
    } else { /* the overdrive's gain law (core/fx/fx_overdrive.h), the fuzz's alike */
        int32_t a2 = a * a >> 8;
        int32_t a3 = a2 * a >> 8;
        int32_t a7 = (a3 * a2 >> 8) * a2 >> 8;
        g = 256 + a / 2 + 24 * a7 - a3 / 2;
        bias = b * 102 / 256;
    }
    int32_t v = g * x / 256 + bias;
    v = v < -2048 ? -2048 : v > 2048 ? 2048 : v;
    if (f->alg == SID_DIST_FUZZ || v >= 256 || v <= -256) {
        return v < -256 ? -256 : v > 256 ? 256 : v;
    }
    return (3 * v - (v * v >> 8) * v / 256) / 2; /* the clip's cubic */
}

/* The rate of a modulation slot, in 1/256 Hz, from its A control (0..127). */
static uint32_t fx_hz_q8(const fx_ctx* f) {
    static const uint8_t MIN_Q8[SID_MOD_COUNT] = {0, 38, 13, 13, 26, 26, 0}; /* 0.15 .. 0.05 .. 0.1 */
    static const uint8_t OCT26[SID_MOD_COUNT] = {0, 78, 172, 172, 172, 172, 0};
    return (uint32_t)MIN_Q8[f->alg] * exp2q((int32_t)f->a * OCT26[f->alg] / 127) >> 12;
}

static void fx_col(const void* ctx, uint32_t x, lab_draw_col* out) {
    const fx_ctx* f = ctx;
    out->lo[0] = out->lo[1] = out->lo[2] = LAB_DRAW_NONE;
    out->lit = (uint8_t)(f->hl >= 1u && f->hl <= 3u);
    uint32_t base = FX_Y0 + FX_H;
    int32_t y;
    switch (f->kind) {
    case FX_XFER:
        if (x < FX_PX0 || x > FX_PX0 + FX_H) {
            return;
        }
        {
            int32_t xi = ((int32_t)(x - FX_PX0) * 512 + 27) / (int32_t)FX_H - 256;
            out->lo[0] = out->hi[0] = (uint8_t)(FX_Y0 + (256 - fx_xfer(f, xi)) * (int32_t)FX_H / 512);
            out->lo[1] = out->hi[1] = (uint8_t)(FX_Y0 + (256 - xi) * (int32_t)FX_H / 512);
            out->lit &= 1u;
        }
        return;
    case FX_WAVE:
        if (x < MOD_X0 || x >= MOD_X0 + MOD_W) {
            return;
        }
        {
            uint32_t ph = (x - MOD_X0) * f->hz_q8 * 643u >> 16;
            int32_t s = sine_q7(ph);
            int32_t amp = f->amount * 26 / 127;
            if (f->alg == SID_MOD_TREMOLO || f->alg == SID_MOD_AUTOPAN) {
                s += ((s < 0 ? -127 : 127) - s) * f->b / 127; /* the shape: sine to square */
            } else if (f->alg == SID_MOD_WOW) { /* a slow wow with the flutter on it */
                s = sine_q7((x - MOD_X0) * 128u * 643u >> 16);
                s = (s * f->amount * 2 + sine_q7((x - MOD_X0) * 2048u * 643u >> 16) * f->a) / (3 * 127);
                amp = 26;
            }
            y = (int32_t)(FX_Y0 + FX_H / 2u) - s * amp / 127;
            out->lo[0] = out->hi[0] = (uint8_t)y;
        }
        return;
    case FX_TAPS: {
        if (x < FX_X0 || x >= FX_X0 + FX_W) {
            return;
        }
        uint32_t mono = f->alg == SID_FXSLOT_DELAY;
        uint32_t gap = f->gap;
        uint32_t k = (x - FX_X0 + 1u) / gap; /* the tap in this sample's two columns */
        if (x == FX_X0) { /* the input */
            out->lo[LAB_DRAW_MARKER] = FX_Y0;
            out->hi[LAB_DRAW_MARKER] = (uint8_t)base;
        } else if (k && (x - FX_X0 + 1u) - k * gap < 2u) {
            uint32_t fb = f->amount * 243u / 127u;
            uint32_t amp = 205u;
            for (uint32_t j = 1; j < k; j++) {
                amp = amp * fb >> 8;
            }
            uint32_t l = mono ? 0u : 1u - (k & 1u);
            out->lo[l] = (uint8_t)(base - amp * FX_H / 256u);
            out->hi[l] = (uint8_t)base;
            out->lit = (uint8_t)(out->lit << l);
        }
        return;
    }
    default: {
        if (x < FX_X0 || x >= FX_X0 + FX_W) {
            return;
        }
        uint32_t half = 100u + f->amount * 1400u / 127u;
        uint32_t n = (x - FX_X0) * FX_WAVE_MS / FX_W * 26u / half;
        n = n < 400u ? n : 400u;
        out->lo[0] = out->hi[0] = (uint8_t)(base - exp2q(-(int32_t)n) * FX_H / 4096u);
        return;
    }
    }
}

static void fx_init(fx_ctx* f, const live* l, uint32_t hl) {
    f->slot = l->fx_select % LIVE_SLOTS;
    const uint8_t* s = l->slot[f->slot];
    f->alg = s[LIVE_SLOT_ALGORITHM];
    f->amount = s[LIVE_SLOT_AMOUNT];
    f->a = s[LIVE_SLOT_A];
    f->b = s[LIVE_SLOT_B];
    f->hl = (uint8_t)hl;
    f->kind = FX_NONE;
    if (f->slot == SID_SLOT_DIST && f->alg != SID_DIST_NONE && f->alg != SID_DIST_LOFI) {
        f->kind = FX_XFER;
    } else if (f->slot == SID_SLOT_MOD && f->alg != SID_MOD_NONE) {
        f->kind = FX_WAVE;
    } else if (f->slot == SID_SLOT_FX && f->alg != SID_FXSLOT_NONE) {
        f->kind = f->alg <= SID_FXSLOT_PINGPONG ? FX_TAPS : FX_DECAY;
    }
    f->hz_q8 = f->kind == FX_WAVE ? fx_hz_q8(f) : 0u;
    if (f->kind == FX_TAPS) { /* the time: 10 ms to the line's end, log */
        uint32_t t = 10u * exp2q((int32_t)f->a * (f->alg == SID_FXSLOT_DELAY ? 188 : 161) / 127) >> 12;
        f->gap = (uint16_t)(t * FX_W / FX_TAPS_MS < 2u ? 2u : t * FX_W / FX_TAPS_MS);
    }
}

static void none_col(const void* ctx, uint32_t x, lab_draw_col* out) {
    (void)ctx;
    (void)x;
    out->lo[0] = out->lo[1] = out->lo[2] = LAB_DRAW_NONE;
    out->lit = 0;
}

uint32_t lab_draw_look(const live* l, const scope* sc, uint32_t page, uint32_t hl) {
    uint32_t h = (page * 31u + hl) * 2654435761u ^ l->theme;
    if (page == LAB_PAGE_OSC) {
        h = (h ^ l->control) * 16777619u;
        h = (h ^ live_pw(l)) * 16777619u;
        h = (h ^ l->sync_q8 ^ (uint32_t)l->mono_sync << 16) * 16777619u;
        h = (h ^ (uint8_t)l->tune) * 16777619u;
    } else if (page == LAB_PAGE_ENVELOPE) {
        h = (h ^ l->attack_decay ^ (uint32_t)l->sustain_release << 8) * 16777619u;
        int32_t v = loudest(sc);
        if (v >= 0) {
            h = (h ^ sc->env[v] ^ (uint32_t)sc->state[v] << 8) * 16777619u;
        }
    } else if (page == LAB_PAGE_FILTER) {
        h = (h ^ l->cutoff_out ^ (uint32_t)l->resonance << 12 ^ (uint32_t)l->filter_mode << 16 ^
             (uint32_t)l->model << 24) * 16777619u;
    } else if (page == LAB_PAGE_MOD) {
        h = (h ^ l->vibrato_rate ^ (uint32_t)l->vibrato_depth << 8 ^ (uint32_t)l->vibrato_delay << 16 ^
             (uint32_t)l->arp_on << 24) * 16777619u;
        h = (h ^ l->pwm_depth ^ (uint32_t)l->pwm_rate << 16) * 16777619u;
        h = (h ^ l->arp_rate ^ (uint32_t)l->arp_order << 8 ^ (uint32_t)l->arp_span << 16) * 16777619u;
    } else {
        const uint8_t* s = l->slot[l->fx_select % LIVE_SLOTS];
        h = (h ^ l->fx_select ^ (uint32_t)s[0] << 8 ^ (uint32_t)s[1] << 16 ^ (uint32_t)s[2] << 24) *
            16777619u;
        h = (h ^ s[3]) * 16777619u;
    }
    return h;
}

int lab_draw_page(const live* l, const scope* sc, uint32_t page, uint32_t hl, const theme* th,
                  grid* g, const screen* s, uint32_t row0, uint32_t* left) {
    lab_draw d = {none_col, NULL, {th->label, th->label, th->value}, th->hot};
    osc_ctx o;
    env_ctx e;
    filt_ctx f;
    mod_ctx m;
    fx_ctx x;
    if (hl >= 1u && hl <= 4u) {
        d.lit_color = th->knob[hl - 1u];
    }
    if (page == LAB_PAGE_OSC) {
        osc_init(&o, l, hl);
        d.fn = osc_col;
        d.ctx = &o;
        d.color[0] = live_view_wave_color(l->control);
        d.color[LAB_DRAW_MARKER] = th->knob[3];
    } else if (page == LAB_PAGE_ENVELOPE) {
        env_init(&e.e, l->attack_decay, l->sustain_release,
                 lab_draw_gate_us(l->attack_decay, l->sustain_release));
        e.hl = (uint8_t)(hl <= 4u ? hl : 0u);
        env_dot(&e, sc);
        d.fn = env_col;
        d.ctx = &e;
    } else if (page == LAB_PAGE_FILTER) {
        filt_init(&f, l, hl);
        d.fn = filt_col;
        d.ctx = &f;
        d.color[0] = f.mode ? th->value : th->dim;
        d.color[1] = th->label;
    } else if (page == LAB_PAGE_MOD) {
        mod_init(&m, l, hl);
        d.fn = mod_col;
        d.ctx = &m;
        d.color[0] = th->value;
        d.color[1] = l->control & 0x40u ? th->label : th->dim;
        d.color[LAB_DRAW_MARKER] = th->hot;
    } else {
        fx_init(&x, l, hl);
        d.fn = x.kind == FX_NONE ? none_col : fx_col;
        d.ctx = &x;
        d.color[0] = th->value;
        d.color[1] = th->dim;
        d.color[LAB_DRAW_MARKER] = th->hot;
    }
    return lab_draw_apply(&d, g, s, row0, left);
}

/* s at (row, col) in attr, 1x. */
static void gtext(grid_canvas* c, uint32_t row, uint32_t col, const char* s, uint8_t attr) {
    for (; *s && col < GRID_COLS; s++, col++) {
        c->ch[row][col] = (uint8_t)*s;
        c->attr[row][col] = attr;
    }
}

/* The words of FILTER's, MOD's and FX's drawings: FILTER's axis and the voices routed (V1 V2 V3
 * at 2x, in the voices' colours, dark when not routed, reversed while K3 is turned), MOD's bands
 * and times, FX's chain of slots (the selected one marked, a slot that is off dark) and the
 * legend or the name of what its shape is. */
static void guides_page(grid_canvas* c, const live* l, uint32_t page, uint32_t hl, const theme* th,
                        uint32_t row0) {
    uint32_t last = row0 + LAB_DRAW_ROWS - 1u;
    uint8_t hint = THEME_ATTR(th, hint);
    if (page == LAB_PAGE_FILTER) {
        gtext(c, last, 6, "100", hint);
        gtext(c, last, 17, "1K", hint);
        gtext(c, last, 27, "10K", hint);
        for (uint32_t v = 0; v < 3u; v++) {
            char s[3] = {'V', (char)('1' + v), 0};
            uint8_t on = l->filter_voices >> v & 1u;
            uint8_t fg = on ? th->voice_text[v] : th->dim;
            (void)grid_canvas_text_big(c, v * 5u, row0 + 11u, 2, s,
                                       hl == 3u && on ? GRID_ATTR(th->bg, fg) : GRID_ATTR(fg, th->bg));
        }
    } else if (page == LAB_PAGE_MOD) {
        gtext(c, row0 + 2u, 0, "VIB", hint);
        gtext(c, row0 + 6u, 0, "PWM", hint);
        gtext(c, row0 + 10u, 0, "ARP", l->arp_on ? hint : THEME_ATTR(th, dim));
        gtext(c, last, 14, "0.5S", hint);
        gtext(c, last, 24, "1S", hint);
    } else {
        uint32_t sel = l->fx_select % LIVE_SLOTS;
        for (uint32_t s = 0; s < LIVE_SLOTS; s++) {
            uint32_t alg = l->slot[s][LIVE_SLOT_ALGORITHM];
            uint32_t row = row0 + 2u * s;
            uint8_t on = s == sel ? THEME_ATTR(th, value) : THEME_ATTR(th, label);
            if (s == sel) {
                for (uint32_t r = 0; r < 2u; r++) {
                    c->ch[row + r][0] = FONT_FULL;
                    c->attr[row + r][0] = hl == 4u ? GRID_ATTR(th->knob[3], th->bg) : THEME_ATTR(th, hot);
                }
            }
            gtext(c, row, 2, views_slot_name(s), s == sel ? THEME_ATTR(th, hot) : hint);
            (void)grid_canvas_text_big(c, 7, row, 2, views_algorithm_name(s, alg),
                                       alg ? on : THEME_ATTR(th, dim));
        }
        fx_ctx f;
        fx_init(&f, l, hl);
        if (f.kind == FX_NONE) { /* no shape to draw: the slot's name, at 2x */
            const char* n = views_algorithm_name(sel, f.alg);
            uint32_t len = 0;
            while (n[len]) {
                len++;
            }
            (void)grid_canvas_text_big(c, (GRID_COLS - 2u * len) / 2u, row0 + 9u, 2, n, THEME_ATTR(th, dim));
        } else {
            static const char* const LEGEND[5] = {"", "INPUT TO OUTPUT", "2 SECONDS", "1.5 SECONDS",
                                                  "2 SECONDS"};
            gtext(c, last, 0, LEGEND[f.kind], hint);
        }
    }
}

void lab_draw_guides(grid_canvas* c, const live* l, uint32_t page, uint32_t hl, const theme* th,
                     uint32_t row0) {
    if (page == LAB_PAGE_OSC && (l->control & 0x40u) && l->pwm_depth && !l->mono_sync &&
        !(l->control & 0x06u)) {
        /* the PWM's range: where the pulse's edge goes, dotted behind it */
        uint32_t cycle = OSC_CYCLE * 65536u / (uint32_t)(65536 + l->tune * 38);
        int32_t a = (int32_t)l->pulse_width - (int32_t)l->pwm_depth;
        int32_t b = (int32_t)l->pulse_width + (int32_t)l->pwm_depth;
        a = a < 0 ? 0 : a;
        b = b > 4095 ? 4095 : b;
        uint32_t x0 = OSC_X0 + cycle * (uint32_t)a / 4096u;
        uint32_t x1 = OSC_X0 + cycle * (uint32_t)b / 4096u;
        uint8_t attr = GRID_ATTR(hl == 2u || hl == 3u ? th->knob[hl - 1u] : th->dim, th->bg);
        for (uint32_t col = x0 / 8u; col <= x1 / 8u && col < GRID_COLS; col++) {
            for (uint32_t r = 0; r + 1u < LAB_DRAW_ROWS; r++) {
                c->ch[row0 + r][col] = FONT_D2;
                c->attr[row0 + r][col] = attr;
            }
        }
    } else if (page == LAB_PAGE_FILTER || page == LAB_PAGE_MOD || page == LAB_PAGE_FX) {
        guides_page(c, l, page, hl, th, row0);
    } else if (page == LAB_PAGE_ENVELOPE) {
        /* the key let go: a line at its column; the time's ticks under the curve */
        uint32_t x = lab_draw_x_of_us(lab_draw_gate_us(l->attack_decay, l->sustain_release));
        uint32_t key = (x + 4u) / 8u; /* a cell's left edge, within 4 pixels of the key's */
        for (uint32_t r = 0; r + 1u < LAB_DRAW_ROWS; r++) {
            c->ch[row0 + r][key] = FONT_H1;
            c->attr[row0 + r][key] = THEME_ATTR(th, dim);
        }
        static const char* const TICK[4] = {"10MS", "0.1S", "1S", "10S"};
        static const uint8_t TICK_COL[4] = {5, 11, 18, 23};
        uint32_t row = row0 + LAB_DRAW_ROWS - 1u;
        for (uint32_t k = 0; k < 4u; k++) {
            uint32_t col = TICK_COL[k];
            for (const char* p = TICK[k]; *p; p++) {
                c->ch[row][col] = (uint8_t)*p;
                c->attr[row][col++] = THEME_ATTR(th, hint);
            }
        }
    }
}
