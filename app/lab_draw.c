// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_draw.h"

#include <string.h>

#include "font.h"
#include "fx/fx_slots.h"
#include "lab_lfo.h"
#include "lab_table.h"
#include "lab_view.h"
#include "live_view.h"
#include "sid/sid_filter.h"
#include "sine.h"
#include "views.h"

_Static_assert(LAB_DRAW_ROWS == LAB_VIEW_ART_ROWS, "the drawing fills LAB's area");
_Static_assert(LAB_DRAW_H <= LAB_DRAW_NONE, "a row of the area fits a byte, NONE apart");
_Static_assert(LAB_DRAW_STEP == 2u, "a sample is two columns (the kit's 0xC0)");

int lab_draw_apply(const lab_draw* d, grid* g, const screen* s, uint32_t row0, uint32_t* left) {
    uint8_t prev_lo[LAB_DRAW_LAYERS] = {LAB_DRAW_NONE, LAB_DRAW_NONE, LAB_DRAW_NONE};
    uint8_t prev_hi[LAB_DRAW_LAYERS] = {0, 0, 0};
    int whole = 1;
    for (uint32_t c = 0; c < GRID_COLS; c++) {
        uint32_t words[LAB_DRAW_ROWS * 2u + (LAB_DRAW_ROWS + 3u) / 4u];
        for (uint32_t k = 0; k < sizeof words / sizeof words[0]; k++) {
            words[k] = 0;
        }
        uint8_t (*m)[8] = (uint8_t (*)[8])words;
        uint8_t* has = (uint8_t*)&words[LAB_DRAW_ROWS * 2u];
        for (uint32_t j = 0; j < 8u; j += LAB_DRAW_STEP) {
            lab_draw_col col;
            d->fn(d->ctx, c * 8u + j, &col);
            uint8_t bit = (uint8_t)(0xC0u >> j);
            for (uint32_t l = 0; l < LAB_DRAW_LAYERS; l++) {
                uint32_t a = col.lo[l];
                uint32_t b = col.hi[l];
                if (a == LAB_DRAW_NONE) {
                    prev_lo[l] = LAB_DRAW_NONE;
                    continue;
                }
                if (l != LAB_DRAW_MARKER) {
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
                continue;
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
                whole = 0;
            }
            (*left)--;
        }
    }
    return whole;
}

static const uint16_t DECADE[LAB_DRAW_DECADE] = {
    1000, 1049, 1101, 1155, 1212, 1271, 1334, 1399, 1468, 1540, 1616, 1695, 1778, 1866, 1957, 2054,
    2154, 2260, 2371, 2488, 2610, 2738, 2873, 3014, 3162, 3318, 3481, 3652, 3831, 4019, 4217, 4424,
    4642, 4870, 5109, 5360, 5623, 5900, 6190, 6494, 6813, 7148, 7499, 7867, 8254, 8660, 9085, 9532};
static const uint16_t PERIOD[16] = {9,   32,   63,   95,   149,   220,   267,   313,
                                    392, 977, 1954, 3126, 3907, 11720, 19532, 31251};
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
    return t - 1000u;
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

uint32_t lab_draw_attack_us(uint32_t rate) {
    return PERIOD[rate & 15u] * 25882u / 100u;
}

uint32_t lab_draw_decay_us(uint32_t rate) {
    return PERIOD[rate & 15u] * 767u;
}

static uint32_t periods(uint32_t t, uint32_t period) {
    return t / period * 64u / 65u;
}

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

static uint32_t periods_between(uint32_t hi, uint32_t lo) {
    uint32_t w = 0;
    uint32_t top = 255;
    for (uint32_t i = 0; i < 6u; i++) {
        uint32_t a = lo + 1u > SEG_LO[i] ? lo + 1u : SEG_LO[i];
        uint32_t b = hi < top ? hi : top;
        if (b >= a) {
            w += (b - a + 1u) * SEG_P[i];
        }
        top = SEG_LO[i] - 1u;
    }
    return w;
}

typedef struct {
    uint32_t attack;
    uint32_t decay;
    uint32_t release;
    uint32_t sustain;
    uint32_t gate;
    uint32_t at_gate;
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
    t += t / 2u;
    uint32_t mid = lab_draw_time_us(LAB_DRAW_GATE_X);
    uint32_t last = lab_draw_time_us(LAB_DRAW_GATE_MAX_X);
    return t < mid ? mid : t > last ? last : t;
}

uint32_t lab_draw_level(uint8_t ad, uint8_t sr, uint32_t gate_us, uint32_t t) {
    env_shape e;
    env_init(&e, ad, sr, gate_us);
    return env_level(&e, t);
}

#define OSC_TOP 6u
#define OSC_SPAN 99u
#define ENV_TOP 4u
#define ENV_SPAN 96u
#define OSC_X0 8u
#define OSC_CYCLE 224u
#define OSC_RATIO 384u

static uint8_t y_of(uint32_t v, uint32_t top, uint32_t span) {
    return (uint8_t)(top + (255u - v) * span / 255u);
}

typedef struct {
    uint32_t cycle;
    uint32_t ratio;
    uint16_t pw;
    uint8_t control;
    uint8_t mod;
    uint8_t hl;
} osc_ctx;

static uint32_t osc_wave(const osc_ctx* o, uint32_t p, uint32_t x) {
    uint32_t q = p * o->ratio >> 8 & 0xFFFFu;
    uint32_t c = o->control;
    uint32_t v = 255;
    if (c & 0x10u) {
        uint32_t t = q < 32768u ? q >> 7 : (65535u - q) >> 7;
        v &= o->mod == 1u && p >= 32768u ? 255u - t : t;
    }
    if (c & 0x20u) {
        v &= q >> 8;
    }
    if (c & 0x40u) {
        v &= (q >> 4) >= o->pw ? 255u : 0u;
    }
    if (c & 0x80u) {
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
        out->lo[LAB_DRAW_MARKER] = OSC_TOP;
        out->hi[LAB_DRAW_MARKER] = OSC_TOP + OSC_SPAN;
    }
    if (x < OSC_X0 || x >= OSC_X0 + o->cycle + LAB_DRAW_STEP) {
        return;
    }
    uint32_t p = x >= OSC_X0 + o->cycle ? 0u : (x - OSC_X0) * 65536u / o->cycle;
    uint32_t v = osc_wave(o, p, x);
    out->lo[0] = out->hi[0] = y_of(v, OSC_TOP, OSC_SPAN);
    if (o->hl == 1u && (o->control & 0x40u) && x >= OSC_X0 + LAB_DRAW_STEP && p) {
        uint32_t q = p * o->ratio >> 8 & 0xFFFFu;
        uint32_t b = ((x - LAB_DRAW_STEP - OSC_X0) * 65536u / o->cycle) * o->ratio >> 8 & 0xFFFFu;
        if (((q >> 4) >= o->pw) != ((b >> 4) >= o->pw)) {
            out->lit = 1;
        }
    }
    if (o->mod) {
        uint32_t m = o->mod == 2u ? p >> 8 : p >= 32768u ? 255u : 0u;
        out->lo[1] = out->hi[1] = y_of(m, OSC_TOP, OSC_SPAN);
    }
}

static uint32_t live_pw(const live* l) {
    if (l->layers) {
        return l->voice[0].gate ? l->voice[l->layer_edit].pw_out : live_edit_pulse_width(l);
    }
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        if (l->voice[v].gate) {
            return l->voice[v].pw_out;
        }
    }
    return l->pulse_width;
}

static void osc_init(osc_ctx* o, const live* l, uint32_t hl) {
    o->control = live_edit_control(l);
    o->pw = (uint16_t)live_pw(l);
    o->mod = o->control & 0x04u ? 1u : o->control & 0x02u ? 2u : 0u;
    uint32_t r = l->mono_sync ? l->sync_q8 : o->mod ? OSC_RATIO : 256u;
    o->ratio = r < 256u ? 256u : r > 2048u ? 2048u : r;
    o->cycle = OSC_CYCLE * 65536u / (uint32_t)(65536 + l->tune * 38);
    o->hl = (uint8_t)hl;
}

typedef struct {
    env_shape e;
    uint8_t hl;
    uint8_t dot_x;
    uint8_t dot_y;
} env_ctx;

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
        uint32_t end = lab_draw_time_us(x + LAB_DRAW_STEP);
        end = end < c->e.attack ? end : c->e.attack;
        end = end < c->e.gate ? end : c->e.gate;
        uint32_t top = held_level(&c->e, end);
        level = top > level ? top : level;
    }
    out->lo[0] = out->hi[0] = y_of(level, ENV_TOP, ENV_SPAN);
    out->lit = (uint8_t)(c->hl && env_segment(&c->e, t, level) == c->hl);
    if (c->dot_x != LAB_DRAW_NONE && x + 3u >= c->dot_x && x <= c->dot_x + 3u) {
        out->lo[LAB_DRAW_MARKER] = (uint8_t)(c->dot_y > 3u ? c->dot_y - 3u : 0u);
        out->hi[LAB_DRAW_MARKER] = (uint8_t)(c->dot_y + 3u);
    }
}

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
        last = lab_draw_time_us(last > 3u ? last - 3u : 0u);
        t = t < last ? t : last;
    }
    c->dot_x = (uint8_t)lab_draw_x_of_us(t);
    c->dot_y = y_of(level, ENV_TOP, ENV_SPAN);
}

static uint32_t fenv_now(const live* l) {
    return l->fenv_t_us == LIVE_FENV_IDLE ? 0u : l->fenv_t_us / 10000u + 1u;
}

static void fenv_dot(env_ctx* c, const live* l) {
    c->dot_x = LAB_DRAW_NONE;
    if (!l->fenv_depth || l->fenv_t_us == LIVE_FENV_IDLE) {
        return;
    }
    uint32_t level = lab_draw_level(l->fenv_ad, l->fenv_sr, l->fenv_gate_us, l->fenv_t_us);
    uint32_t shown_gate = c->e.gate;
    uint32_t t = l->fenv_gate_us == LIVE_FENV_IDLE
                     ? (l->fenv_t_us < shown_gate ? l->fenv_t_us : shown_gate)
                     : shown_gate + (l->fenv_t_us - l->fenv_gate_us);
    uint32_t x = lab_draw_x_of_us(t);
    c->dot_x = (uint8_t)(x < LAB_DRAW_W - 4u ? x : LAB_DRAW_W - 4u);
    c->dot_y = y_of(level, ENV_TOP, ENV_SPAN);
}

#define FILT_X0 8u
#define FILT_X1 232u
#define FILT_PX_OCT 26u
#define FILT_SPAN 88u
#define FILT_REACH 24u

static const uint16_t EXP2[26] = {4096, 4207, 4320, 4437, 4557, 4680, 4806, 4936, 5070,
                                  5207, 5347, 5492, 5640, 5793, 5949, 6110, 6275, 6444,
                                  6619, 6797, 6981, 7170, 7363, 7562, 7767, 7976};
static const uint16_t LOG2[17] = {0,   22,  44,  63,  82,  100, 118, 134, 150,
                                  165, 179, 193, 207, 220, 232, 244, 256};

static uint32_t exp2q(int32_t n) {
    int32_t k = n >= 0 ? n / 26 : -((25 - n) / 26);
    uint32_t t = EXP2[n - k * 26];
    return k >= 0 ? t << (k < 18 ? k : 18) : -k < 32 ? t >> -k : 0u;
}

static int32_t log2q8(uint64_t v) {
    if (!v) {
        return 0;
    }
    uint32_t p = 63u - (uint32_t)__builtin_clzll(v);
    uint32_t m = (uint32_t)(p >= 8u ? v >> (p - 8u) : v << (8u - p));
    uint32_t i = (m - 256u) >> 4;
    uint32_t f = m & 15u;
    return (int32_t)(p * 256u + LOG2[i] + (LOG2[i + 1u] - LOG2[i]) * f / 16u);
}

typedef struct {
    uint16_t xc;
    uint16_t invq;
    uint8_t mode;
    uint8_t hl;
} filt_ctx;

static void filt_col(const void* ctx, uint32_t x, lab_draw_col* out) {
    const filt_ctx* f = ctx;
    out->lo[0] = out->lo[1] = out->lo[2] = LAB_DRAW_NONE;
    out->lit = 0;
    if (x < FILT_X0 || x > FILT_X1) {
        return;
    }
    int32_t level = 0;
    if (f->mode) {
        int32_t d = ((int32_t)x - f->xc) * 2;
        uint32_t w = exp2q(d);
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
        out->lit = 1;
    }
    if (f->mode && (x & ~1u) == (f->xc & ~1u)) {
        out->lo[1] = 0;
        out->hi[1] = FILT_SPAN - 2u;
        out->lit |= (uint8_t)((f->hl == 1u) << 1);
    }
}

static uint32_t filt_x(const live* l, uint32_t c) {
    float hz = sid_cutoff_hz(l->model, 0, c);
    uint32_t h = (uint32_t)(hz < 30.0f ? 30.0f : hz > 12000.0f ? 12000.0f : hz);
    return FILT_X0 + (uint32_t)(log2q8(h) - log2q8(30)) * FILT_PX_OCT / 256u;
}

static void filt_init(filt_ctx* f, const live* l, uint32_t hl) {
    f->xc = (uint16_t)filt_x(l, l->cutoff_out);
    float k = sid_resonance_k(l->model, 0, l->resonance) * 256.0f;
    f->invq = (uint16_t)(k > 512.0f ? 512.0f : k);
    f->mode = l->filter_mode >> 4 & 7u;
    f->hl = (uint8_t)hl;
}

#define MOD_X0 32u
#define MOD_W 204u
#define MOD_TICKS 128u
#define MOD_A_CY 18u
#define MOD_B_CY 54u
#define MOD_C_BASE 102u
#define MOD_AMP 14u
#define MOD_STEPS 65u

typedef struct {
    uint16_t vstep, pstep;
    uint16_t cycle;
    uint16_t arp_inv;
    uint8_t vamp;
    uint8_t pamp;
    uint8_t delay;
    uint8_t hl;
    uint8_t stairs[MOD_STEPS];
} mod_ctx;

static int32_t tri(uint32_t phase) {
    int32_t p = (int32_t)(phase & 0xFFFFu);
    return p < 16384 ? p : p < 49152 ? 32768 - p : p - 65536;
}

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
    uint32_t t16 = (x - MOD_X0) * 10u;
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
        uint32_t b = m->stairs[(t16 + 20u) * m->arp_inv >> 16];
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

#define FX_Y0 48u
#define FX_H 55u
#define FX_PX0 92u
#define FX_WAVE_MS 2000u
#define FX_TAPS_MS 1500u
#define FX_X0 8u
#define FX_W 224u

enum { FX_NONE, FX_XFER, FX_WAVE, FX_TAPS, FX_DECAY };

typedef struct {
    uint32_t hz_q8;
    uint16_t gap;
    uint8_t kind;
    uint8_t slot, alg;
    uint8_t amount, a, b;
    uint8_t hl;
} fx_ctx;

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
    } else {
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
    return (3 * v - (v * v >> 8) * v / 256) / 2;
}

static uint32_t fx_hz_q8(const fx_ctx* f) {
    static const uint8_t MIN_Q8[SID_MOD_COUNT] = {0, 38, 13, 13, 26, 26, 0};
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
                s += ((s < 0 ? -127 : 127) - s) * f->b / 127;
            } else if (f->alg == SID_MOD_WOW) {
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
        uint32_t k = (x - FX_X0 + 1u) / gap;
        if (x == FX_X0) {
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
    if (f->kind == FX_TAPS) {
        uint32_t t = 10u * exp2q((int32_t)f->a * (f->alg == SID_FXSLOT_DELAY ? 188 : 161) / 127) >> 12;
        f->gap = (uint16_t)(t * FX_W / FX_TAPS_MS < 2u ? 2u : t * FX_W / FX_TAPS_MS);
    }
}

#define MAC_X0 8u
#define MAC_X1 128u
#define MAC_W 104u
#define MAC_TOP 6u
#define MAC_SPAN 84u

typedef struct {
    int8_t reso_link, pwm_link;
    uint8_t res, place;
    uint16_t depth, pw;
    uint8_t hl;
} mac_ctx;

static uint8_t mac_y(int32_t v, int32_t max) {
    v = v < 0 ? 0 : v > max ? max : v;
    return (uint8_t)(MAC_TOP + (uint32_t)(max - v) * MAC_SPAN / (uint32_t)max);
}

static void mac_col(const void* ctx, uint32_t x, lab_draw_col* out) {
    const mac_ctx* m = ctx;
    out->lo[0] = out->lo[1] = out->lo[2] = LAB_DRAW_NONE;
    out->lit = 0;
    if (x >= MAC_X0 && x <= MAC_X0 + MAC_W) {
        int32_t p = (int32_t)((x - MAC_X0) * 64u / MAC_W);
        uint8_t y = mac_y(m->res + m->reso_link * (32 - p) / 8, (int32_t)LIVE_RESONANCE_MAX);
        out->lo[0] = out->hi[0] = y;
        out->lit = (uint8_t)(m->hl == 1u);
        uint32_t at = MAC_X0 + (uint32_t)m->place * MAC_W / 64u;
        if ((x & ~1u) == (at & ~1u)) {
            out->lo[2] = (uint8_t)(y > 4u ? y - 4u : 0u);
            out->hi[2] = (uint8_t)(y + 4u);
        }
    } else if (x >= MAC_X1 && x <= MAC_X1 + MAC_W) {
        int32_t w = (int32_t)((x - MAC_X1) * 4095u / MAC_W);
        int32_t away = w > 2048 ? w - 2048 : 2048 - w;
        uint8_t y = mac_y(m->depth + m->pwm_link * away / 8, (int32_t)LIVE_PWM_DEPTH_MAX);
        out->lo[1] = out->hi[1] = y;
        out->lit = (uint8_t)((m->hl == 2u) << 1);
        uint32_t at = MAC_X1 + (uint32_t)m->pw * MAC_W / 4095u;
        if ((x & ~1u) == (at & ~1u)) {
            out->lo[2] = (uint8_t)(y > 4u ? y - 4u : 0u);
            out->hi[2] = (uint8_t)(y + 4u);
        }
    }
}

static void mac_init(mac_ctx* m, const live* l, uint32_t hl) {
    m->reso_link = l->reso_link;
    m->pwm_link = l->pwm_link;
    m->res = l->resonance;
    m->place = (uint8_t)live_cutoff_place(l, l->cutoff_out);
    m->depth = l->pwm_depth;
    m->pw = l->pulse_width;
    m->hl = (uint8_t)hl;
}

#define TBL_STEP_W 15u
#define TBL_BAR_FROM 4u
#define TBL_BAR_TO 10u
#define TBL_MID (LAB_DRAW_H / 2u)
#define TBL_PX_A_SEMI 2
#define TBL_DOT 3u
#define TBL_C4 48

typedef struct {
    uint8_t y[LAB_TABLE_STEPS_MAX];
    uint8_t steps, edit, playing, pos, hl;
} tbl_ctx;

static void tbl_init(tbl_ctx* t, const live* l, uint32_t hl) {
    t->steps = (uint8_t)(lab_table_on(l) ? lab_table_steps(l) : 0u);
    t->edit = l->table_edit;
    uint32_t v = live_table_playing(l);
    t->playing = (uint8_t)v;
    t->pos = v ? l->table_pos[v - 1u] : 0u;
    t->hl = (uint8_t)hl;
    for (uint32_t n = 0; n < t->steps; n++) {
        uint8_t note = l->table[LAB_SOUND_TABLE_STEPS - LAB_SOUND_TABLE + 2u * n + 1u];
        int32_t off = lab_table_absolute(note) ? (int32_t)(note - LAB_TABLE_NOTE_NONE) - TBL_C4
                                                : lab_table_offset(note);
        off = off < -LAB_TABLE_NOTE_SPAN ? -LAB_TABLE_NOTE_SPAN
              : off > LAB_TABLE_NOTE_SPAN ? LAB_TABLE_NOTE_SPAN
                                          : off;
        t->y[n] = (uint8_t)((int32_t)TBL_MID - off * TBL_PX_A_SEMI);
    }
}

static void tbl_col(const void* ctx, uint32_t x, lab_draw_col* out) {
    const tbl_ctx* t = ctx;
    out->lo[0] = out->lo[1] = out->lo[2] = LAB_DRAW_NONE;
    out->lit = 0;
    uint32_t n = x / TBL_STEP_W, in = x % TBL_STEP_W;
    if (n >= t->steps || in < TBL_BAR_FROM || in > TBL_BAR_TO) {
        return;
    }
    uint32_t y = t->y[n];
    uint32_t layer = n == t->edit ? 1u : 0u;
    out->lo[layer] = (uint8_t)(y < TBL_MID ? y : TBL_MID);
    out->hi[layer] = (uint8_t)(y > TBL_MID ? y : TBL_MID + 1u);
    out->lit = (uint8_t)((t->hl >= 2u && t->hl <= 3u && layer) << 1);
    if (t->playing && n == t->pos) {
        uint32_t top = out->lo[layer];
        out->lo[2] = (uint8_t)(top > 2u * TBL_DOT ? top - 2u * TBL_DOT : 0u);
        out->hi[2] = (uint8_t)(out->lo[2] + TBL_DOT);
    }
}

#define LFO_PANEL_W 120u
#define LFO_PANEL_PAD 6u
#define LFO_AMP 44
#define LFO_SH_STEPS 6u

typedef struct {
    uint8_t shape[LAB_SOUND_LFOS];
    uint8_t edit, hl;
    uint16_t phase[LAB_SOUND_LFOS];
} lfo_ctx;

static int32_t lfo_shape_at(uint32_t shape, uint32_t p) {
    static const int8_t HOLD[LFO_SH_STEPS] = {40, -70, 90, -20, 60, -100};
    switch (shape) {
    case LAB_LFO_TRIANGLE: return (int32_t)(p < 32768u ? 2u * p : 2u * (65535u - p)) - 32768;
    case LAB_LFO_SAW_UP: return (int32_t)p - 32768;
    case LAB_LFO_SAW_DOWN: return 32767 - (int32_t)p;
    case LAB_LFO_SQUARE: return p < 32768u ? 32767 : -32768;
    case LAB_LFO_SAMPLE_HOLD: return HOLD[p * LFO_SH_STEPS >> 16] * 327;
    default: return 0;
    }
}

static void lfo_init(lfo_ctx* c, const live* l, uint32_t hl) {
    for (uint32_t n = 0; n < LAB_SOUND_LFOS; n++) {
        c->shape[n] = (uint8_t)lab_lfo_shape_of(l, n);
        c->phase[n] = l->lfo_phase_n[n];
    }
    c->edit = l->lfo_edit;
    c->hl = (uint8_t)hl;
}

static void lfo_col(const void* ctx, uint32_t x, lab_draw_col* out) {
    const lfo_ctx* c = ctx;
    out->lo[0] = out->lo[1] = out->lo[2] = LAB_DRAW_NONE;
    out->lit = 0;
    uint32_t n = x / LFO_PANEL_W, in = x % LFO_PANEL_W;
    if (n >= LAB_SOUND_LFOS || in < LFO_PANEL_PAD || in >= LFO_PANEL_W - LFO_PANEL_PAD) {
        return;
    }
    uint32_t w = LFO_PANEL_W - 2u * LFO_PANEL_PAD;
    uint32_t p = (in - LFO_PANEL_PAD) * 65536u / w;
    int32_t y = (int32_t)(LAB_DRAW_H / 2u) - lfo_shape_at(c->shape[n], p) * LFO_AMP / 32768;
    uint32_t layer = n == c->edit ? 0u : 1u;
    out->lo[layer] = out->hi[layer] = (uint8_t)y;
    out->lit = (uint8_t)(layer == 0u && c->hl >= 1u && c->hl <= 4u);
    uint32_t at = LFO_PANEL_PAD + (uint32_t)c->phase[n] * w / 65536u;
    if (c->shape[n] && (in & ~1u) == (at & ~1u)) {
        out->lo[2] = (uint8_t)(y > 4 ? y - 4 : 0);
        out->hi[2] = (uint8_t)(y + 4);
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
        h = (h ^ live_edit_control(l) ^ (uint32_t)l->layer_edit << 8) * 16777619u;
        h = (h ^ live_pw(l)) * 16777619u;
        h = (h ^ l->sync_q8 ^ (uint32_t)l->mono_sync << 16) * 16777619u;
        h = (h ^ (uint8_t)l->tune) * 16777619u;
    } else if (page == LAB_PAGE_ENVELOPE) {
        h = (h ^ live_edit_attack_decay(l) ^ (uint32_t)live_edit_sustain_release(l) << 8 ^
             (uint32_t)l->layer_edit << 16) * 16777619u;
        int32_t v = loudest(sc);
        if (v >= 0) {
            h = (h ^ sc->env[v] ^ (uint32_t)sc->state[v] << 8) * 16777619u;
        }
    } else if (page == LAB_PAGE_FILTER_ENV) {
        h = (h ^ l->fenv_ad ^ (uint32_t)l->fenv_sr << 8 ^ (uint32_t)(uint8_t)l->fenv_depth << 16) *
            16777619u;
        h = (h ^ fenv_now(l)) * 16777619u;
    } else if (page == LAB_PAGE_FILTER) {
        h = (h ^ l->cutoff_out ^ (uint32_t)l->resonance << 12 ^ (uint32_t)l->filter_mode << 16 ^
             (uint32_t)l->model << 24) * 16777619u;
    } else if (page == LAB_PAGE_MOD) {
        h = (h ^ l->vibrato_rate ^ (uint32_t)l->vibrato_depth << 8 ^ (uint32_t)l->vibrato_delay << 16 ^
             (uint32_t)l->arp_on << 24) * 16777619u;
        h = (h ^ l->pwm_depth ^ (uint32_t)l->pwm_rate << 16) * 16777619u;
        h = (h ^ l->arp_rate ^ (uint32_t)l->arp_order << 8 ^ (uint32_t)l->arp_span << 16) * 16777619u;
    } else if (page == LAB_PAGE_MACRO) {
        h = (h ^ live_cutoff_place(l, l->cutoff_out) ^ (uint32_t)l->resonance << 8 ^
             (uint32_t)(uint8_t)l->reso_link << 16 ^ (uint32_t)(uint8_t)l->pwm_link << 24) *
            16777619u;
        h = (h ^ l->pulse_width ^ (uint32_t)l->pwm_depth << 16) * 16777619u;
    } else if (page == LAB_PAGE_LAYERS) {
        h = (h ^ l->layers ^ (uint32_t)l->layer_edit << 8 ^ (uint32_t)l->filter_voices << 16) *
            16777619u;
        for (uint32_t v = 0; v < LIVE_VOICES; v++) {
            h = (h ^ live_layer_control_of(l, v) ^ (uint32_t)(uint8_t)l->transpose[v] << 8 ^
                 (uint32_t)(uint8_t)l->detune[v] << 16) * 16777619u;
        }
    } else if (page == LAB_PAGE_LFO) {
        for (uint32_t n = 0; n < LAB_SOUND_LFOS; n++) {
            h = (h ^ l->lfo[n][0] ^ (uint32_t)l->lfo[n][1] << 8 ^ (uint32_t)l->lfo[n][2] << 16 ^
                 (uint32_t)(l->lfo_phase_n[n] >> 11) << 24) * 16777619u;
        }
        h = (h ^ l->lfo_edit) * 16777619u;
    } else if (page == LAB_PAGE_TABLE) {
        for (uint32_t i = 0; i < LAB_SOUND_TABLE_BYTES; i++) {
            h = (h ^ l->table[i]) * 16777619u;
        }
        uint32_t v = live_table_playing(l);
        h = (h ^ l->table_edit ^ v << 8 ^ (uint32_t)(v ? l->table_pos[v - 1u] : 0u) << 16 ^
             (uint32_t)l->layers << 24) * 16777619u;
    } else if (page != LAB_DRAW_BLANK) {
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
    mac_ctx mc;
    tbl_ctx tb;
    lfo_ctx lc;
    if (hl >= 1u && hl <= 4u) {
        d.lit_color = th->knob[hl - 1u];
    }
    if (page == LAB_PAGE_OSC) {
        osc_init(&o, l, hl);
        d.fn = osc_col;
        d.ctx = &o;
        d.color[0] = live_view_wave_color(o.control);
        d.color[LAB_DRAW_MARKER] = th->knob[3];
    } else if (page == LAB_PAGE_ENVELOPE) {
        uint8_t ad = live_edit_attack_decay(l), sr = live_edit_sustain_release(l);
        env_init(&e.e, ad, sr, lab_draw_gate_us(ad, sr));
        e.hl = (uint8_t)(hl <= 4u ? hl : 0u);
        env_dot(&e, sc);
        d.fn = env_col;
        d.ctx = &e;
    } else if (page == LAB_PAGE_FILTER_ENV) {
        env_init(&e.e, l->fenv_ad, l->fenv_sr, lab_draw_gate_us(l->fenv_ad, l->fenv_sr));
        e.hl = (uint8_t)(hl <= 4u ? hl : 0u);
        fenv_dot(&e, l);
        d.fn = env_col;
        d.ctx = &e;
        d.color[0] = l->fenv_depth ? th->value : th->dim;
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
    } else if (page == LAB_PAGE_MACRO) {
        mac_init(&mc, l, hl);
        d.fn = mac_col;
        d.ctx = &mc;
        d.color[0] = l->reso_link ? th->value : th->dim;
        d.color[1] = l->pwm_link ? th->value : th->dim;
        d.color[LAB_DRAW_MARKER] = th->hot;
    } else if (page == LAB_PAGE_LFO) {
        lfo_init(&lc, l, hl);
        d.fn = lfo_col;
        d.ctx = &lc;
        d.color[0] = lc.shape[lc.edit] ? th->value : th->dim;
        d.color[1] = th->dim;
        d.color[LAB_DRAW_MARKER] = th->hot;
    } else if (page == LAB_PAGE_TABLE) {
        tbl_init(&tb, l, hl);
        d.fn = tbl_col;
        d.ctx = &tb;
        d.color[0] = th->value;
        d.color[1] = th->knob[0];
        d.color[LAB_DRAW_MARKER] = th->hot;
    } else if (page != LAB_DRAW_BLANK && page != LAB_PAGE_LAYERS) {
        fx_init(&x, l, hl);
        d.fn = x.kind == FX_NONE ? none_col : fx_col;
        d.ctx = &x;
        d.color[0] = th->value;
        d.color[1] = th->dim;
        d.color[LAB_DRAW_MARKER] = th->hot;
    }
    return lab_draw_apply(&d, g, s, row0, left);
}

static void gtext(grid_canvas* c, uint32_t row, uint32_t col, const char* s, uint8_t attr) {
    for (; *s && col < GRID_COLS; s++, col++) {
        c->ch[row][col] = (uint8_t)*s;
        c->attr[row][col] = attr;
    }
}

static void guides_page(grid_canvas* c, const live* l, uint32_t page, uint32_t hl, const theme* th,
                        uint32_t row0) {
    uint32_t last = row0 + LAB_DRAW_ROWS - 1u;
    uint8_t hint = THEME_ATTR(th, hint);
    if (page == LAB_PAGE_FILTER) {
        if (l->filter_lfo) {
            const uint16_t* t = live_cutoff_steps[l->model];
            int32_t pos = (int32_t)live_cutoff_place(l, l->cutoff);
            int32_t off = (int32_t)l->cutoff - t[pos];
            int32_t a = pos - l->filter_lfo < 0 ? 0 : pos - l->filter_lfo;
            int32_t b = pos + l->filter_lfo > 64 ? 64 : pos + l->filter_lfo;
            int32_t ra = t[a] + off, rb = t[b] + off;
            uint32_t x0 = filt_x(l, (uint32_t)(ra < 0 ? 0 : ra));
            uint32_t x1 = filt_x(l, (uint32_t)(rb > 2047 ? 2047 : rb));
            uint8_t attr = GRID_ATTR(hl == 4u ? th->knob[3] : th->dim, th->bg);
            for (uint32_t col = x0 / 8u; col <= x1 / 8u && col < GRID_COLS; col++) {
                for (uint32_t r = 0; r + 1u < LAB_DRAW_ROWS; r++) {
                    c->ch[row0 + r][col] = FONT_D2;
                    c->attr[row0 + r][col] = attr;
                }
            }
        }
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
        if (f.kind == FX_NONE) {
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

static void short_wave(uint8_t control, char* out) {
    static const char LETTER[4] = {'T', 'S', 'P', 'N'};
    static const char* const ONE[4] = {"TRI", "SAW", "PUL", "NOI"};
    uint32_t w = control >> 4, n = 0, only = 0;
    for (uint32_t b = 0; b < 4u; b++) {
        if (w >> b & 1u) {
            only = b;
            n++;
        }
    }
    char* e = out;
    if (n == 1u) {
        for (const char* p = ONE[only]; *p; p++) {
            *e++ = *p;
        }
    } else if (n == 0u) {
        *e++ = '-';
    } else {
        for (uint32_t b = 0; b < 4u; b++) {
            if (w >> b & 1u) {
                *e++ = LETTER[b];
            }
        }
    }
    if (control & 0x06u) {
        *e++ = control & 0x04u ? 'R' : 'Y';
    }
    *e = 0;
}

static void signed_text(int32_t v, const char* unit, char* out) {
    char* e = out;
    *e++ = v < 0 ? '-' : '+';
    uint32_t m = (uint32_t)(v < 0 ? -v : v);
    if (m >= 10u) {
        *e++ = (char)('0' + m / 10u);
    }
    *e++ = (char)('0' + m % 10u);
    for (; *unit; unit++) {
        *e++ = *unit;
    }
    *e = 0;
}

static void guides_layers(grid_canvas* c, const live* l, const theme* th, uint32_t row0) {
    for (uint32_t v = 0; v < LIVE_VOICES; v++) {
        uint32_t col = v * 10u;
        int on = v <= l->layers;
        uint8_t fg = on ? th->voice_text[v] : th->dim;
        uint8_t attr = GRID_ATTR(fg, th->bg);
        char text[8] = {'L', (char)('1' + v), 0};
        (void)grid_canvas_text_big(c, col, row0 + 1u, 2,  text,
                                   on && v == l->layer_edit ? GRID_ATTR(th->bg, fg) : attr);
        short_wave(live_layer_control_of(l, v), text);
        (void)grid_canvas_text_big(c, col, row0 + 4u, 2, text, attr);
        signed_text(l->transpose[v], "", text);
        (void)grid_canvas_text_big(c, col, row0 + 7u, 2, text, attr);
        signed_text(l->detune[v], "C", text);
        (void)grid_canvas_text_big(c, col, row0 + 10u, 2, text, attr);
    }
}

static void guides_table(grid_canvas* c, const live* l, const theme* th, uint32_t row0) {
    uint32_t steps = lab_table_on(l) ? lab_table_steps(l) : 0u;
    uint32_t loop = l->table[LAB_SOUND_TABLE_LOOP - LAB_SOUND_TABLE];
    if (steps && loop) {
        uint32_t row = row0 + LAB_DRAW_ROWS - 1u;
        uint32_t col = ((loop - 1u) * TBL_STEP_W + TBL_BAR_FROM) / 8u;
        gtext(c, row, col, "^LOOP", THEME_ATTR(th, hint));
    }
}

void lab_draw_guides(grid_canvas* c, const live* l, uint32_t page, uint32_t hl, const theme* th,
                     uint32_t row0) {
    if (page == LAB_PAGE_OSC && !l->layer_edit && (l->control & 0x40u) && l->pwm_depth &&
        !l->mono_sync && !(l->control & 0x06u)) {
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
    } else if (page == LAB_PAGE_LAYERS) {
        guides_layers(c, l, th, row0);
    } else if (page == LAB_PAGE_TABLE) {
        guides_table(c, l, th, row0);
    } else if (page == LAB_PAGE_LFO) {
        uint32_t row = row0 + LAB_DRAW_ROWS - 1u;
        for (uint32_t n = 0; n < LAB_SOUND_LFOS; n++) {
            char t[6] = {(char)('1' + n), ' ', 0};
            const char* code = lab_lfo_shape_of(l, n) ? lab_lfo_dest_code(lab_lfo_dest_of(l, n)) : "OFF";
            for (uint32_t i = 0; code[i]; i++) {
                t[2u + i] = code[i];
                t[3u + i] = 0;
            }
            gtext(c, row, 1u + n * (LFO_PANEL_W / 8u), t,
                  n == l->lfo_edit ? THEME_ATTR(th, value) : THEME_ATTR(th, hint));
        }
    } else if (page == LAB_PAGE_MACRO) {
        uint32_t row = row0 + LAB_DRAW_ROWS - 1u;
        gtext(c, row, 1, "CUT>RESO", THEME_ATTR(th, hint));
        gtext(c, row, 16, "WIDTH>PWM", THEME_ATTR(th, hint));
    } else if (page == LAB_PAGE_FILTER || page == LAB_PAGE_MOD || page == LAB_PAGE_FX) {
        guides_page(c, l, page, hl, th, row0);
    } else if (page == LAB_PAGE_ENVELOPE || page == LAB_PAGE_FILTER_ENV) {
        int filter = page == LAB_PAGE_FILTER_ENV;
        uint32_t x = lab_draw_x_of_us(lab_draw_gate_us(filter ? l->fenv_ad : live_edit_attack_decay(l),
                                                       filter ? l->fenv_sr : live_edit_sustain_release(l)));
        uint32_t key = (x + 4u) / 8u;
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
