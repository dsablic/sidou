// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "live_view.h"

#include <stddef.h>
#include <string.h>

#include "font.h"
#include "sid/sid_filter.h"
#include "views.h"

#define TEXT VIEWS_TEXT
#define HEADER GRID_ATTR(GRID_BLUE, GRID_LIGHT_BLUE)
#define BRIGHT GRID_ATTR(GRID_WHITE, GRID_BLUE)
#define MID (LIVE_VIEW_SCOPE_PX / 2u)

/* The lower boxes: FILTER and VU from row 19, 8 rows; the slots below them. */
#define LOWER_ROW 19u
#define LOWER_ROWS 8u
#define FILTER_W 18u
#define VU_COL FILTER_W
#define VU_W (GRID_COLS - FILTER_W)
#define VU_METER_COL (VU_COL + 2u)
#define VU_METER_CELLS 9u
#define SLOT_ROW (LOWER_ROW + LOWER_ROWS)
/* The filter's area: 6 rows of 8 pixels, -36 dB at the bottom, +12 at the top. */
#define CURVE_ROWS 6u
#define CURVE_PX (CURVE_ROWS * 8u)
#define CURVE_FLOOR_DB 36.0f
/* The info column right of the envelope bars. */
#define ENV_COL 22u
#define INFO_COL 25u

_Static_assert(LIVE_VIEW_SCOPE_COL + LIVE_VIEW_SCOPE_COLS <= ENV_COL - 1u,
               "the scope fits inside its box");
_Static_assert(LIVE_VIEW_BOX_ROW + SCOPE_VOICES * LIVE_VIEW_BOX_ROWS <= LOWER_ROW,
               "the voices' boxes end above FILTER and VU");
_Static_assert(SLOT_ROW + LIVE_SLOTS == GRID_ROWS, "the slots fill the bottom rows");
_Static_assert(SCOPE_VOICES == LIVE_VOICES, "a scope per voice");

static const char* const NOTE_NAMES[12] = {"C-", "C#", "D-", "D#", "E-", "F-",
                                           "F#", "G-", "G#", "A-", "A#", "B-"};
static const char* const MODE_NAMES[8] = {"OFF", "LP",    "BP",    "LP+BP",
                                          "HP",  "NOTCH", "BP+HP", "ALL"};

static uint32_t length(const char* s) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

/* ---- the instrument's side ---- */

/* The control register voice v plays with (live.c voice_control, without the gate). */
static uint8_t voice_control(const live* l, uint32_t v) {
    const live_preset* p = live_preset_get(l->preset);
    return p && p->mono_sync && v != 0u ? 0u : l->control;
}

static uint8_t wave_color(uint8_t control) {
    switch (control >> 4) {
    case 0: return GRID_LIGHT_BLUE;
    case 1: return GRID_LIGHT_GREEN;
    case 2: return GRID_YELLOW;
    case 4: return GRID_CYAN;
    case 8: return GRID_LIGHT_GREY;
    default: return GRID_WHITE; /* combined */
    }
}

static const char* wave_name(uint8_t control) {
    switch (control >> 4) {
    case 0: return "----";
    case 1: return "TRI";
    case 2: return "SAW";
    case 4: return "PULSE";
    case 8: return "NOISE";
    default: return "MIX";
    }
}

/* Periods across the scope: more for higher notes (A4 three, C6 and up four). */
static uint32_t cycles_for(uint8_t note) {
    if (note == LIVE_NO_NOTE) {
        return 2;
    }
    uint32_t c = note > 24u ? (note - 24u) / 12u : 0u;
    return c < 1u ? 1u : c > 4u ? 4u : c;
}

/* ---- the filter curve ---- */

/* log2(x) for x > 0, within 0.005: the exponent, and a quadratic on the mantissa. */
static float log2_approx(float x) {
    union {
        float f;
        uint32_t u;
    } v = {x};
    float e = (float)((int32_t)((v.u >> 23) & 255u) - 127);
    v.u = (v.u & 0x7FFFFFu) | 0x3F800000u;
    float m = v.f - 1.0f;
    return e + m * (1.3466f - 0.3466f * m);
}

/* The response of the state-variable filter sid.c runs, at the chip's fitted cutoff and
 * damping (sid_cutoff_hz, sid_resonance_k; curve 0, the model's default), for the mode's
 * outputs summed: (lp + bp s + hp s^2) / (s^2 + k s + 1) at s = j f / fc. The column centres
 * step by 400^(1/16) from 30 Hz * 400^(1/32). */
static void curve(live_view* lv, const live* l) {
    uint8_t mode = (uint8_t)(l->filter_mode >> 4 & 7u);
    if (lv->curve_valid && lv->curve_cutoff == l->cutoff_out &&
        lv->curve_resonance == l->resonance && lv->curve_mode == mode &&
        lv->curve_model == (uint8_t)l->model) {
        return;
    }
    lv->curve_valid = 1;
    lv->curve_serial++;
    lv->curve_cutoff = l->cutoff_out;
    lv->curve_resonance = l->resonance;
    lv->curve_mode = mode;
    lv->curve_model = (uint8_t)l->model;
    float fc = sid_cutoff_hz(l->model, 0, l->cutoff_out);
    fc = fc < 30.0f ? 30.0f : fc; /* as sid.c clamps it */
    lv->curve_hz = (uint16_t)(fc + 0.5f);
    float k = sid_resonance_k(l->model, 0, l->resonance);
    const float step = 1.454215f; /* 400^(1/16) */
    float lo = 30.0f;
    float f = 36.1773f; /* 30 * 400^(1/32) */
    lv->mark = LIVE_VIEW_FILTER_COLS - 1u;
    int marked = 0;
    for (uint32_t i = 0; i < LIVE_VIEW_FILTER_COLS; i++, f *= step) {
        float r = f / fc;
        float nre = ((mode & 1u) ? 1.0f : 0.0f) - ((mode & 4u) ? r * r : 0.0f);
        float nim = (mode & 2u) ? r : 0.0f;
        float dre = 1.0f - r * r;
        float dim = k * r;
        float h2 = (nre * nre + nim * nim) / (dre * dre + dim * dim);
        float px = h2 > 1e-12f ? 3.0103f * log2_approx(h2) + CURVE_FLOOR_DB : 0.0f;
        lv->height[i] = (uint8_t)(px <= 0.0f ? 0.0f : px >= (float)CURVE_PX ? (float)CURVE_PX : px);
        lo *= step;
        if (!marked && fc < lo) {
            lv->mark = (uint8_t)i;
            marked = 1;
        }
    }
}

static void columns(const uint8_t y[SCOPE_W], uint32_t w[SCOPE_W]);

/* Every trace flat, as drawn at rest. */
static void flat(live_view* lv) {
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        for (uint32_t x = 0; x < SCOPE_W; x++) {
            lv->y[v][x] = MID;
        }
        columns(lv->y[v], lv->cols[v]);
    }
}

/* ---- reading the tap ---- */

/* What the tap says now: the envelopes, the meters, and each voice's trace into pend (the
 * drawn one, lv->y, stays until the scopes' budget lets it change). */
static void update(live_view* lv, uint8_t pend[SCOPE_VOICES][SCOPE_W]) {
    lv->updates++;
    const live* l = lv->l;
    scope_point* pts = lv->pts;
    uint32_t n = 0;
    uint16_t peak[2] = {0, 0};
    if (lv->sc && lv->on) {
        for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
            lv->env[v] = lv->sc->env[v];
            lv->state[v] = lv->sc->state[v];
            lv->sustain[v] = lv->sc->sustain[v];
        }
        /* Every voice at rest: flat traces and empty levels need no points. */
        if (lv->env[0] | lv->env[1] | lv->env[2]) {
            n = scope_read(lv->sc, lv->since, pts, SCOPE_READ_MAX);
        }
        scope_take_peaks(lv->sc, peak);
    }
    uint32_t level[LIVE_VIEW_METERS] = {0};
    const live_preset* p = live_preset_get(l->preset);
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        int32_t e = lv->env[v];
        /* At rest (and with no tap): flat. A copy the producer overtook (or none yet): the
         * drawn trace again. */
        if (!e || !lv->sc || !lv->on || !n) {
            int flat = !e || !lv->sc || !lv->on;
            for (uint32_t x = 0; x < SCOPE_W; x++) {
                pend[v][x] = flat ? (uint8_t)MID : lv->y[v][x];
            }
            continue;
        }
        uint8_t ctrl = voice_control(l, v);
        uint32_t by = (ctrl & SID_CTRL_SYNC) ? (v + 2u) % SCOPE_VOICES : v;
        uint8_t note = l->voice[p && p->mono_sync ? 0u : v].note;
        uint8_t wave[SCOPE_W];
        scope_trace(pts, n, v, by, cycles_for(note), wave);
        uint32_t most = 0;
        int32_t moved = 0;
        for (uint32_t x = 0; x < SCOPE_W; x++) {
            int32_t w = (int32_t)wave[x] - 128;
            uint32_t a = (uint32_t)(w < 0 ? -w : w);
            most = a > most ? a : most;
            int32_t y = (int32_t)MID - w * e * LIVE_VIEW_AMP / (128 * 255);
            int32_t d = y - (int32_t)lv->y[v][x];
            d = d < 0 ? -d : d;
            moved = d > moved ? d : moved;
            pend[v][x] = (uint8_t)y;
        }
        /* A trace no pixel of which moved by more than one is the last one wobbling (which
         * points the fold got): it stays as drawn, so a note that holds still sends nothing. */
        if (moved <= 1) {
            for (uint32_t x = 0; x < SCOPE_W; x++) {
                pend[v][x] = lv->y[v][x];
            }
        }
        /* The voice's level: its waveform's peak times its envelope. */
        level[v] = most * (uint32_t)e;
    }
    level[SCOPE_VOICES] = peak[0];
    level[SCOPE_VOICES + 1u] = peak[1];
    for (uint32_t m = 0; m < LIVE_VIEW_METERS; m++) {
        uint32_t now = scope_meter(level[m]);
        uint32_t was = lv->meter[m];
        uint32_t fell = was > LIVE_VIEW_FALL ? was - LIVE_VIEW_FALL : 0u;
        /* Up at once; down at most LIVE_VIEW_FALL a frame, and not for a step or two (a
         * steady note's peaks wobble), so a held note's meter holds still. */
        uint32_t next = now >= was ? now : was - now <= 2u && now ? was : now > fell ? now : fell;
        lv->meter[m] = (uint8_t)next;
    }
    lv->prepared = 1;
}

void live_view_prepare(live_view* lv) {
    if (lv->sc && lv->on) {
        update(lv, lv->pend);
    }
}

void live_view_init(live_view* lv, const live* l, scope* sc) {
    memset(lv, 0, sizeof *lv); /* not a const template: 6 KB of zeros in flash */
    lv->l = l;
    lv->sc = sc;
    flat(lv);
}

void live_view_tap(live_view* lv, int on) {
    if (!on) {
        lv->prepared = 0; /* what it prepared goes stale */
    }
    if (!lv->sc) {
        return;
    }
    if (on && !lv->on) {
        lv->since = lv->sc->head;
    }
    if (!on && lv->on) {
        /* Rest, until the tap has points again (live_view.h). */
        flat(lv);
        for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
            lv->env[v] = 0;
            lv->state[v] = 0;
            lv->sustain[v] = 0;
        }
        for (uint32_t m = 0; m < LIVE_VIEW_METERS; m++) {
            lv->meter[m] = 0;
        }
    }
    lv->on = (uint8_t)(on != 0);
    lv->sc->on = lv->on;
}

/* ---- drawing ---- */

/* The frame is composed here, overwriting freely, and goes to the grid a row at a time: each
 * cell is written once, so a row that shows what it showed marks nothing to send. */
typedef live_view_canvas canvas;

static void put(canvas* cv, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr) {
    if (col < GRID_COLS && row < GRID_ROWS) {
        cv->ch[row][col] = ch;
        cv->attr[row][col] = attr;
    }
}

static uint32_t text(canvas* cv, uint32_t col, uint32_t row, const char* s, uint8_t attr) {
    while (*s) {
        put(cv, col++, row, (uint8_t)*s++, attr);
    }
    return col;
}

static void fill(canvas* cv, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t ch,
                 uint8_t attr) {
    for (uint32_t r = row; r < row + h; r++) {
        for (uint32_t c = col; c < col + w; c++) {
            put(cv, c, r, ch, attr);
        }
    }
}

/* A rounded frame with its title set into the top edge, between spaces. */
static void box(canvas* cv, uint32_t col, uint32_t row, uint32_t w, uint32_t h, const char* title) {
    uint32_t right = col + w - 1u;
    uint32_t bottom = row + h - 1u;
    fill(cv, col + 1u, row, w - 2u, 1, FONT_BOX_H, TEXT);
    fill(cv, col + 1u, bottom, w - 2u, 1, FONT_BOX_H, TEXT);
    fill(cv, col, row + 1u, 1, h - 2u, FONT_BOX_V, TEXT);
    fill(cv, right, row + 1u, 1, h - 2u, FONT_BOX_V, TEXT);
    put(cv, col, row, FONT_ROUND_TL, TEXT);
    put(cv, right, row, FONT_ROUND_TR, TEXT);
    put(cv, col, bottom, FONT_ROUND_BL, TEXT);
    put(cv, right, bottom, FONT_ROUND_BR, TEXT);
    uint32_t c = text(cv, col + 2u, row, " ", TEXT);
    c = text(cv, c, row, title, TEXT);
    put(cv, c, row, ' ', TEXT);
}

static void header(const live_view* lv, canvas* cv) {
    const live* l = lv->l;
    fill(cv, 0, 0, GRID_COLS, 1, ' ', HEADER);
    text(cv, 1, 0, "LIVE", HEADER);
    put(cv, 6, 0, FONT_PLAY, HEADER);
    text(cv, 8, 0, l->model == SID_MODEL_8580 ? "8580" : "6581", HEADER);
    const live_preset* p = live_preset_get(l->preset);
    text(cv, 13, 0, p ? p->name : "?", HEADER);
    char oct[6] = "OCT+0";
    int32_t o = l->octave;
    oct[3] = o < 0 ? '-' : '+';
    oct[4] = (char)('0' + (o < 0 ? -o : o));
    text(cv, GRID_COLS - 1u - 5u, 0, oct, HEADER);
}

static uint32_t scope_top(uint32_t v) {
    return LIVE_VIEW_BOX_ROW + v * LIVE_VIEW_BOX_ROWS + 1u;
}

/* A trace as a word per pixel column, bit 31 - p for pixel row p, each column joined to the
 * one before it. */
static void columns(const uint8_t y[SCOPE_W], uint32_t w[SCOPE_W]) {
    uint32_t prev = y[0];
    for (uint32_t x = 0; x < SCOPE_W; x++) {
        uint32_t lo = y[x] < prev ? y[x] : prev;
        uint32_t hi = y[x] < prev ? prev : y[x];
        prev = y[x];
        hi = hi < LIVE_VIEW_SCOPE_PX ? hi : LIVE_VIEW_SCOPE_PX - 1u;
        lo = lo < hi ? lo : hi;
        /* Bits 31 - lo down to 31 - hi. */
        w[x] = (0xFFFFFFFFu >> lo) & (0xFFFFFFFFu << (31u - hi));
    }
}

/* Cell row r of the 8 columns w8 as an overlay mask (bit 7 the leftmost pixel): their bytes
 * for the cell, transposed (Hacker's Delight, transpose8rS32). */
static void cell_mask(const uint32_t* w8, uint32_t r, uint8_t m[8]) {
    uint32_t sh = 24u - 8u * r;
    uint32_t x = (w8[0] >> sh & 255u) << 24 | (w8[1] >> sh & 255u) << 16 |
                 (w8[2] >> sh & 255u) << 8 | (w8[3] >> sh & 255u);
    uint32_t y = (w8[4] >> sh & 255u) << 24 | (w8[5] >> sh & 255u) << 16 |
                 (w8[6] >> sh & 255u) << 8 | (w8[7] >> sh & 255u);
    uint32_t t = (x ^ (x >> 7)) & 0x00AA00AAu;
    x = x ^ t ^ (t << 7);
    t = (y ^ (y >> 7)) & 0x00AA00AAu;
    y = y ^ t ^ (t << 7);
    t = (x ^ (x >> 14)) & 0x0000CCCCu;
    x = x ^ t ^ (t << 14);
    t = (y ^ (y >> 14)) & 0x0000CCCCu;
    y = y ^ t ^ (t << 14);
    t = (x & 0xF0F0F0F0u) | ((y >> 4) & 0x0F0F0F0Fu);
    y = ((x << 4) & 0xF0F0F0F0u) | (y & 0x0F0F0F0Fu);
    x = t;
    for (uint32_t i = 0; i < 4u; i++) {
        m[i] = (uint8_t)(x >> (24u - 8u * i));
        m[4u + i] = (uint8_t)(y >> (24u - 8u * i));
    }
}

/* Sets cell (c, r) of voice v's scope from the columns w. */
static void cell_apply(grid* g, uint32_t v, uint32_t c, uint32_t r, const uint32_t w[SCOPE_W],
                       uint8_t color) {
    const uint32_t* w8 = &w[c * 8u];
    uint32_t col = LIVE_VIEW_SCOPE_COL + c;
    uint32_t any = (w8[0] | w8[1] | w8[2] | w8[3] | w8[4] | w8[5] | w8[6] | w8[7]) >> (24u - 8u * r);
    if (!(any & 255u)) {
        grid_overlay_clear(g, col, scope_top(v) + r);
        return;
    }
    uint8_t m[8];
    cell_mask(w8, r, m);
    grid_overlay(g, col, scope_top(v) + r, m, color);
}

/* Bit c * 4 + r of the result (cells of columns 0..7 in the first word, and so on): the
 * cells of voice v whose overlay differs from w's. With `known`, the overlays are the drawn
 * trace's (lv->cols, in its colour), and only the cells where the columns differ are
 * compared. */
static uint32_t changed_cells(const live_view* lv, const grid* g, uint32_t v,
                              const uint32_t w[SCOPE_W], uint8_t color, int known,
                              uint32_t out[3]) {
    uint32_t n = 0;
    const uint32_t* old = lv->cols[v];
    out[0] = out[1] = out[2] = 0;
    for (uint32_t c = 0; c < LIVE_VIEW_SCOPE_COLS; c++) {
        uint32_t diff = 0;
        if (known) {
            for (uint32_t j = 0; j < 8u; j++) {
                diff |= w[c * 8u + j] ^ old[c * 8u + j];
            }
        }
        for (uint32_t r = 0; r < LIVE_VIEW_SCOPE_ROWS; r++) {
            int differs;
            if (known) {
                differs = (diff >> (24u - 8u * r) & 255u) != 0u;
            } else {
                uint8_t m[8];
                cell_mask(&w[c * 8u], r, m);
                differs = !grid_overlay_same(g, LIVE_VIEW_SCOPE_COL + c, scope_top(v) + r, m, color);
            }
            if (differs) {
                uint32_t bit = c * LIVE_VIEW_SCOPE_ROWS + r;
                out[bit >> 5] |= 1u << (bit & 31u);
                n++;
            }
        }
    }
    return n;
}

/* Voice v's drawn trace (lv->y) onto every cell of its scope. */
static void redraw_voice(live_view* lv, grid* g, uint32_t v) {
    uint32_t* w = lv->cols[v];
    uint8_t color = wave_color(voice_control(lv->l, v));
    for (uint32_t c = 0; c < LIVE_VIEW_SCOPE_COLS; c++) {
        for (uint32_t r = 0; r < LIVE_VIEW_SCOPE_ROWS; r++) {
            cell_apply(g, v, c, r, w, color);
        }
    }
    lv->color[v] = color;
}

/* The traces as overlays over the dotted centre lines (the canvas has those).
 *
 * The overlays are taken to be the drawn traces (lv->y, in lv->color) while the grid's pool
 * has as many free slots as LIVE left it: only LIVE uses overlays while it shows, and a view
 * switch frees them all. When they are not, every voice is marked to be drawn again whole.
 *
 * Fresh traces (pend) take the cells the frame has left of LIVE_VIEW_FRAME_CELLS once
 * everything else is composed and `reserved` (what the screen's layers add after the
 * compose, screen_layer_cells) is set aside: a voice whose changes do not fit keeps its last
 * trace and goes first next frame, so the voices take turns when they all move (noise). Without pend (a wipe
 * or a raster runs) the drawn traces stay; voices to draw again go one a frame, top first,
 * which keeps ahead of a wipe (it reveals voice 1's scope in its first frame, voice 2's in its
 * second, voice 3's in its fourth) and keeps the wipe's first frame cheap. */
static void scopes(live_view* lv, grid* g, uint8_t pend[SCOPE_VOICES][SCOPE_W],
                   uint32_t reserved) {
    uint32_t w[SCOPE_W];
    uint32_t cells[3];
    if (!lv->drawn || g->ovl_nfree != lv->ovl_free) {
        lv->stale = (1u << SCOPE_VOICES) - 1u;
    }
    lv->drawn = 1;
    if (!pend) {
        for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
            if (lv->stale >> v & 1u) {
                redraw_voice(lv, g, v);
                lv->stale &= (uint8_t)~(1u << v);
                break;
            }
        }
        lv->ovl_free = g->ovl_nfree;
        return;
    }
    uint32_t used = grid_pending(g) + reserved;
    uint32_t left = used < LIVE_VIEW_FRAME_CELLS ? LIVE_VIEW_FRAME_CELLS - used : 0u;
    uint32_t first = lv->next_voice % SCOPE_VOICES;
    uint32_t skipped = SCOPE_VOICES;
    for (uint32_t k = 0; k < SCOPE_VOICES; k++) {
        uint32_t v = (first + k) % SCOPE_VOICES;
        uint8_t color = wave_color(voice_control(lv->l, v));
        int known = !(lv->stale >> v & 1u) && color == lv->color[v];
        if (known) {
            uint32_t x = 0;
            while (x < SCOPE_W && pend[v][x] == lv->y[v][x]) {
                x++;
            }
            if (x == SCOPE_W) {
                continue; /* nothing new */
            }
        }
        columns(pend[v], w);
        uint32_t n = changed_cells(lv, g, v, w, color, known, cells);
        if (n > left) {
            skipped = skipped < SCOPE_VOICES ? skipped : v;
            lv->held++;
            continue;
        }
        left -= n;
        for (uint32_t bit = 0; bit < LIVE_VIEW_SCOPE_COLS * LIVE_VIEW_SCOPE_ROWS; bit++) {
            if (cells[bit >> 5] >> (bit & 31u) & 1u) {
                cell_apply(g, v, bit / LIVE_VIEW_SCOPE_ROWS, bit % LIVE_VIEW_SCOPE_ROWS, w, color);
            }
        }
        for (uint32_t x = 0; x < SCOPE_W; x++) { /* not memcpy: the device's copies bytes */
            lv->y[v][x] = pend[v][x];
            lv->cols[v][x] = w[x];
        }
        lv->color[v] = color;
        lv->stale &= (uint8_t)~(1u << v);
    }
    lv->next_voice = (uint8_t)(skipped < SCOPE_VOICES ? skipped : (first + 1u) % SCOPE_VOICES);
    lv->ovl_free = g->ovl_nfree;
}

static const char* stage_name(uint8_t state, uint8_t env, uint8_t sustain) {
    if (env == 0u) {
        return "---";
    }
    if (state == SID_ENV_ATTACK) {
        return "ATK";
    }
    if (state == SID_ENV_RELEASE) {
        return "REL";
    }
    return env == sustain ? "SUS" : "DEC";
}

/* A cell into the canvas and, with g, onto the grid as well: a part drawn on its own, with
 * the rows around it as they are. */
static void cell(canvas* cv, grid* g, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr) {
    put(cv, col, row, ch, attr);
    if (g) {
        grid_put(g, col, row, ch, attr);
    }
}

/* Voice v's envelope: 32 eighths over the 4 rows, ENV3 / 8. */
static void env_bar(const live_view* lv, canvas* cv, grid* g, uint32_t v) {
    uint32_t row = LIVE_VIEW_BOX_ROW + v * LIVE_VIEW_BOX_ROWS;
    uint8_t attr = GRID_ATTR(wave_color(voice_control(lv->l, v)), GRID_BLACK);
    uint32_t level = ((uint32_t)lv->env[v] + 4u) >> 3;
    for (uint32_t j = 0; j < LIVE_VIEW_SCOPE_ROWS; j++) {
        uint32_t below = (LIVE_VIEW_SCOPE_ROWS - 1u - j) * 8u;
        uint32_t fill8 = level > below ? level - below : 0u;
        fill8 = fill8 > 8u ? 8u : fill8;
        cell(cv, g, ENV_COL, row + 1u + j, (uint8_t)(FONT_V0 + fill8), attr);
        cell(cv, g, ENV_COL + 1u, row + 1u + j, (uint8_t)(FONT_V0 + fill8), attr);
    }
}

static void voice(const live_view* lv, canvas* cv, uint32_t v) {
    const live* l = lv->l;
    uint32_t row = LIVE_VIEW_BOX_ROW + v * LIVE_VIEW_BOX_ROWS;
    uint8_t ctrl = voice_control(l, v);
    uint8_t color = wave_color(ctrl);
    char title[10] = "V1 ";
    title[1] = (char)('1' + v);
    const char* wn = wave_name(ctrl);
    for (uint32_t i = 0; wn[i] && i < 5u; i++) {
        title[3u + i] = wn[i];
        title[4u + i] = 0;
    }
    box(cv, 0, row, ENV_COL, LIVE_VIEW_BOX_ROWS, title);
    fill(cv, LIVE_VIEW_SCOPE_COL, scope_top(v) + MID / 8u, LIVE_VIEW_SCOPE_COLS, 1,
         FONT_SCOPE_MID, TEXT);

    env_bar(lv, cv, NULL, v);

    const live_voice* x = &l->voice[v];
    char note[5] = "---";
    if (ctrl && x->note != LIVE_NO_NOTE) {
        note[0] = NOTE_NAMES[x->note % 12u][0];
        note[1] = NOTE_NAMES[x->note % 12u][1];
        if (x->note < 12u) {
            note[2] = '-';
            note[3] = '1';
            note[4] = 0;
        } else {
            note[2] = (char)('0' + x->note / 12u - 1u);
        }
    }
    text(cv, INFO_COL, row + 1u, note, ctrl && x->gate ? BRIGHT : TEXT);
    char wave4[5] = {0};
    for (uint32_t i = 0; wn[i] && i < 4u; i++) {
        wave4[i] = wn[i];
    }
    text(cv, INFO_COL, row + 2u, wave4, GRID_ATTR(color, GRID_BLUE));
    if (ctrl & 0x40u) {
        char pw[5] = "PW00";
        uint32_t pc = ((uint32_t)x->pw_out * 99u + 2047u) / 4095u;
        pw[2] = (char)('0' + pc / 10u);
        pw[3] = (char)('0' + pc % 10u);
        text(cv, INFO_COL, row + 3u, pw, TEXT);
    }
    const char* st = stage_name(lv->state[v], lv->env[v], lv->sustain[v]);
    text(cv, INFO_COL, row + 4u, st, st[0] == '-' ? TEXT : BRIGHT);
}

/* "1234" or "1.2K" for hz. */
static void hz_text(char* buf, uint32_t hz) {
    uint32_t n = 0;
    char digits[8];
    uint32_t k = 0;
    uint32_t whole = hz < 1000u ? hz : (hz + 50u) / 1000u;
    do {
        digits[k++] = (char)('0' + whole % 10u);
        whole /= 10u;
    } while (whole);
    while (k) {
        buf[n++] = digits[--k];
    }
    if (hz >= 1000u) {
        buf[n++] = '.';
        buf[n++] = (char)('0' + (hz + 50u) / 100u % 10u);
        buf[n++] = 'K';
    }
    buf[n] = 0;
}

static void filter(const live_view* lv, canvas* cv) {
    const live* l = lv->l;
    char title[16] = "FILTER ";
    const char* mode = MODE_NAMES[l->filter_mode >> 4 & 7u];
    for (uint32_t i = 0; mode[i]; i++) {
        title[7u + i] = mode[i];
        title[8u + i] = 0;
    }
    box(cv, 0, LOWER_ROW, FILTER_W, LOWER_ROWS, title);
    for (uint32_t i = 0; i < LIVE_VIEW_FILTER_COLS; i++) {
        uint32_t h = lv->height[i];
        int mark = i == lv->mark;
        uint8_t cap = GRID_ATTR(mark ? GRID_YELLOW : GRID_WHITE, GRID_BLUE);
        uint8_t body = GRID_ATTR(mark ? GRID_YELLOW : GRID_LIGHT_BLUE, GRID_BLUE);
        for (uint32_t j = 0; j < CURVE_ROWS; j++) {
            uint32_t row = LOWER_ROW + CURVE_ROWS - j; /* j = 0: the bottom row */
            uint32_t below = j * 8u;
            uint32_t fill8 = h > below ? h - below : 0u;
            if (fill8 > 8u && j + 1u < CURVE_ROWS) {
                put(cv, 1u + i, row, FONT_D8, body);
            } else if (fill8) {
                put(cv, 1u + i, row, (uint8_t)(FONT_V0 + (fill8 > 8u ? 8u : fill8)), cap);
            }
        }
    }
    /* " FC 1.2K RES 15 " on the bottom edge. */
    char line[20] = " FC ";
    char hz[8];
    hz_text(hz, lv->curve_hz);
    uint32_t n = length(line);
    for (uint32_t i = 0; hz[i]; i++) {
        line[n++] = hz[i];
    }
    const char* res = " RES ";
    for (uint32_t i = 0; res[i]; i++) {
        line[n++] = res[i];
    }
    if (l->resonance >= 10u) {
        line[n++] = '1';
    }
    line[n++] = (char)('0' + l->resonance % 10u);
    line[n++] = ' ';
    line[n] = 0;
    text(cv, 1, LOWER_ROW + LOWER_ROWS - 1u, line, TEXT);
}

static const uint8_t METER_ROWS[LIVE_VIEW_METERS] = {1, 2, 3, 5, 6};

/* Meter m's bar. */
static void meter_bar(const live_view* lv, canvas* cv, grid* g, uint32_t m) {
    uint32_t row = LOWER_ROW + METER_ROWS[m];
    uint32_t px = lv->meter[m];
    for (uint32_t c = 0; c < VU_METER_CELLS; c++) {
        uint32_t fill8 = px >= 8u ? 8u : px;
        px -= fill8;
        uint8_t fg = c < 5u ? GRID_LIGHT_GREEN : c < 7u ? GRID_YELLOW : GRID_LIGHT_RED;
        cell(cv, g, VU_METER_COL + c, row, (uint8_t)(FONT_BAR0 + fill8), GRID_ATTR(fg, GRID_BLACK));
    }
}

static void meters(const live_view* lv, canvas* cv) {
    static const char LABELS[LIVE_VIEW_METERS] = {'1', '2', '3', 'L', 'R'};
    box(cv, VU_COL, LOWER_ROW, VU_W, LOWER_ROWS, "VU");
    for (uint32_t m = 0; m < LIVE_VIEW_METERS; m++) {
        uint32_t row = LOWER_ROW + METER_ROWS[m];
        uint8_t color = m < SCOPE_VOICES ? wave_color(voice_control(lv->l, m)) : GRID_WHITE;
        put(cv, VU_COL + 1u, row, (uint8_t)LABELS[m], GRID_ATTR(color, GRID_BLUE));
        meter_bar(lv, cv, NULL, m);
    }
}

static void slots(const live_view* lv, canvas* cv) {
    const live* l = lv->l;
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        uint32_t row = SLOT_ROW + k;
        uint8_t alg = l->slot[k][LIVE_SLOT_ALGORITHM];
        if (k == l->fx_select) {
            put(cv, 0, row, FONT_PLAY, BRIGHT);
        }
        text(cv, 1, row, views_slot_name(k), TEXT);
        text(cv, 6, row, views_algorithm_name(k, alg), alg ? BRIGHT : TEXT);
        text(cv, 17, row, "MIX", TEXT);
        /* The mix in bar eighths: 8 cells, 64 steps. */
        uint32_t px = (uint32_t)l->slot[k][LIVE_SLOT_MIX] * 64u / LIVE_SLOT_MAX;
        for (uint32_t c = 0; c < 8u; c++) {
            uint32_t fill8 = px >= 8u ? 8u : px;
            px -= fill8;
            put(cv, 21u + c, row, (uint8_t)(FONT_BAR0 + fill8), alg ? BRIGHT : TEXT);
        }
    }
}

/* What each part of the canvas shows, as bytes (LIVE_VIEW_LOOK of them): a part is drawn
 * again when its bytes change. */
#define KEY_HEADER 0u
#define KEY_VOICE 3u /* 5 a voice */
#define KEY_ENV (KEY_VOICE + 5u * SCOPE_VOICES)
#define KEY_FILTER (KEY_ENV + SCOPE_VOICES)
#define KEY_VU (KEY_FILTER + 1u)
#define KEY_METER (KEY_VU + SCOPE_VOICES)
#define KEY_SLOTS (KEY_METER + LIVE_VIEW_METERS)
#define KEY_END (KEY_SLOTS + 1u + 2u * LIVE_SLOTS)
_Static_assert(KEY_END == LIVE_VIEW_LOOK, "LIVE_VIEW_LOOK holds every part's bytes");

static void look(const live_view* lv, uint8_t out[LIVE_VIEW_LOOK]) {
    const live* l = lv->l;
    out[KEY_HEADER] = (uint8_t)l->model;
    out[KEY_HEADER + 1u] = l->preset;
    out[KEY_HEADER + 2u] = (uint8_t)l->octave;
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        uint8_t ctrl = voice_control(l, v);
        const live_voice* x = &l->voice[v];
        uint8_t* k = &out[KEY_VOICE + 5u * v];
        k[0] = ctrl;
        k[1] = ctrl ? x->note : LIVE_NO_NOTE;
        k[2] = (uint8_t)(ctrl && x->gate);
        k[3] = (ctrl & 0x40u) ? (uint8_t)(((uint32_t)x->pw_out * 99u + 2047u) / 4095u) : 0xFFu;
        k[4] = (uint8_t)stage_name(lv->state[v], lv->env[v], lv->sustain[v])[0];
        out[KEY_ENV + v] = (uint8_t)((lv->env[v] + 4u) >> 3);
        out[KEY_VU + v] = wave_color(ctrl);
    }
    out[KEY_FILTER] = lv->curve_serial; /* the mode, cutoff and resonance (curve) */
    for (uint32_t m = 0; m < LIVE_VIEW_METERS; m++) {
        out[KEY_METER + m] = lv->meter[m];
    }
    out[KEY_SLOTS] = l->fx_select;
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        out[KEY_SLOTS + 1u + 2u * k] = l->slot[k][LIVE_SLOT_ALGORITHM];
        out[KEY_SLOTS + 2u + 2u * k] = l->slot[k][LIVE_SLOT_MIX];
    }
}

/* now[from .. from + n) differs from what was drawn. */
static int differs(const live_view* lv, const uint8_t* now, uint32_t from, uint32_t n) {
    for (uint32_t i = from; i < from + n; i++) {
        if (now[i] != lv->drawn_look[i]) {
            return 1;
        }
    }
    return 0;
}

#define ALL_ROWS ((1u << GRID_ROWS) - 1u)
#define LOWER_MASK (((1u << LOWER_ROWS) - 1u) << LOWER_ROW)

/* The parts whose bytes changed, drawn again into the canvas: the rows they cover. */
static uint32_t parts(live_view* lv, canvas* cv, const uint8_t now[LIVE_VIEW_LOOK]) {
    uint32_t rows = 0;
    if (differs(lv, now, KEY_HEADER, 3u)) {
        header(lv, cv); /* fills its row */
        rows |= 1u;
    }
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        if (differs(lv, now, KEY_VOICE + 5u * v, 5u)) {
            uint32_t row = LIVE_VIEW_BOX_ROW + v * LIVE_VIEW_BOX_ROWS;
            fill(cv, 0, row, GRID_COLS, LIVE_VIEW_BOX_ROWS, ' ', TEXT);
            voice(lv, cv, v);
            rows |= ((1u << LIVE_VIEW_BOX_ROWS) - 1u) << row;
        }
    }
    if (differs(lv, now, KEY_FILTER, 1u)) {
        fill(cv, 0, LOWER_ROW, FILTER_W, LOWER_ROWS, ' ', TEXT);
        filter(lv, cv);
        rows |= LOWER_MASK;
    }
    if (differs(lv, now, KEY_VU, SCOPE_VOICES)) {
        fill(cv, VU_COL, LOWER_ROW, VU_W, LOWER_ROWS, ' ', TEXT);
        meters(lv, cv);
        rows |= LOWER_MASK;
    }
    if (differs(lv, now, KEY_SLOTS, 1u + 2u * LIVE_SLOTS)) {
        fill(cv, 0, SLOT_ROW, GRID_COLS, LIVE_SLOTS, ' ', TEXT);
        slots(lv, cv);
        rows |= ((1u << LIVE_SLOTS) - 1u) << SLOT_ROW;
    }
    return rows;
}

static void live_compose(screen* s, void* state, grid* g) {
    live_view* lv = state;
    canvas* cv = &lv->cv;
    /* Still while a wipe or a raster runs (app/screen.h): the last frame's look again. */
    int fresh = !screen_animating(s);
    curve(lv, lv->l); /* changes only with the filter's settings: not held */
    if (fresh) {
        /* Mid-frame, the main loop prepared it (live_view_prepare); otherwise now. */
        if (!lv->prepared) {
            update(lv, lv->pend);
        }
        lv->prepared = 0;
    }
    uint8_t now[LIVE_VIEW_LOOK];
    look(lv, now);
    uint32_t rows;
    if (!lv->built) {
        lv->built = 1;
        lv->rebuilds++;
        fill(cv, 0, 0, GRID_COLS, GRID_ROWS, ' ', TEXT);
        header(lv, cv);
        for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
            voice(lv, cv, v);
        }
        filter(lv, cv);
        meters(lv, cv);
        slots(lv, cv);
        rows = ALL_ROWS;
    } else {
        /* The rows the screen drew over (a switch, a toast, a raster) are the canvas's again. */
        rows = parts(lv, cv, now) | (s->redraw ? s->redraw_rows : 0u);
    }
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (rows >> r & 1u) {
            grid_row(g, r, cv->ch[r], cv->attr[r]);
        }
    }
    /* The envelope bars and the meters, cell by cell where only they changed. */
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        if (now[KEY_ENV + v] != lv->drawn_look[KEY_ENV + v]) {
            env_bar(lv, cv, g, v);
        }
    }
    for (uint32_t m = 0; m < LIVE_VIEW_METERS; m++) {
        if (now[KEY_METER + m] != lv->drawn_look[KEY_METER + m]) {
            meter_bar(lv, cv, g, m);
        }
    }
    memcpy(lv->drawn_look, now, sizeof now);
    scopes(lv, g, fresh ? lv->pend : NULL, screen_layer_cells(s));
}

/* Composed every frame (no changed function). */
const view live_view_view = {"LIVE", NULL, NULL, NULL, live_compose, NULL};
