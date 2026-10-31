// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "live_view.h"

#include <stddef.h>
#include <string.h>

#include "font.h"
#include "sid/sid_filter.h"
#include "views.h"

#define MID (LIVE_VIEW_SCOPE_PX / 2u)

#define NOTE_COL 24u
#define VU_LABEL_COL 20u
#define VU_METER_COL 21u
#define VU_METER_CELLS 9u
#define ROWS(row, n) (((1u << (n)) - 1u) << (row))
#define ALL_ROWS ((1u << GRID_ROWS) - 1u)
#define HEADER_ROWS ROWS(0u, 2u)
#define KNOB_ROWS ROWS(LIVE_VIEW_PAGE_ROW, LIVE_VIEW_TAG_ROW_END - LIVE_VIEW_PAGE_ROW)
#define BAND_ROWS ROWS(LIVE_VIEW_BAND_ROW, LIVE_VIEW_BAND_ROWS)
#if defined(__clang__)
#define NO_UNROLL _Pragma("clang loop unroll(disable)")
#else
#define NO_UNROLL
#endif

static const uint8_t KNOB_COL[4] = {0, 8, 15, 22};
static const uint8_t KNOB_W[4] = {8, 7, 7, 8};
#define MODEL_COL 11u
#define PLAY_COL 22u
#define BADGE_COL 24u
static const uint8_t SLOT_COL[LIVE_SLOTS] = {0, 5, 9};

_Static_assert(LIVE_VIEW_SCOPE_COL + LIVE_VIEW_SCOPE_COLS < NOTE_COL, "the scope ends before the note");
_Static_assert(LIVE_VIEW_LANE_ROW + SCOPE_VOICES * LIVE_VIEW_LANE_ROWS <= LIVE_VIEW_PAGE_ROW,
               "the lanes end above the knobs");
_Static_assert(LIVE_VIEW_ALG_ROW + 2u <= LIVE_VIEW_BAND_ROW + LIVE_VIEW_BAND_ROWS &&
                   LIVE_VIEW_METER_ROW + LIVE_VIEW_METERS <= LIVE_VIEW_BAND_ROW + LIVE_VIEW_BAND_ROWS,
               "the slots and the meters are inside the bottom band");
_Static_assert(VU_METER_COL + VU_METER_CELLS <= GRID_COLS, "the meters fit the row");
_Static_assert(LIVE_VIEW_ARP_ROW >= LIVE_VIEW_BAND_ROW + LIVE_VIEW_BAND_ROWS &&
                   LIVE_VIEW_ARP_ROW + 2u <= GRID_ROWS,
               "the arpeggio's badge is under the band");
_Static_assert(SCOPE_VOICES == LIVE_VOICES, "a scope per voice");

static const char* const MODE_NAMES[8] = {"OFF", "LP",    "BP",    "LP+BP",
                                          "HP",  "NOTCH", "BP+HP", "ALL"};

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
    default: return GRID_WHITE;
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

static uint32_t cycles_for(uint8_t note) {
    if (note == LIVE_NO_NOTE) {
        return 2;
    }
    uint32_t c = note > 24u ? (note - 24u) / 12u : 0u;
    return c < 1u ? 1u : c > 4u ? 4u : c;
}

static void columns(const uint8_t y[SCOPE_W], uint32_t w[SCOPE_W]);

static void flat(live_view* lv) {
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        for (uint32_t x = 0; x < SCOPE_W; x++) {
            lv->y[v][x] = MID;
        }
        columns(lv->y[v], lv->cols[v]);
    }
}

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
        if (lv->env[0] | lv->env[1] | lv->env[2]) {
            n = scope_read(lv->sc, lv->since, pts, SCOPE_READ_MAX);
        }
        scope_take_peaks(lv->sc, peak);
    }
    uint32_t level[LIVE_VIEW_METERS] = {0};
    const live_preset* p = live_preset_get(l->preset);
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        int32_t e = lv->env[v];
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
        if (moved <= 1) {
            for (uint32_t x = 0; x < SCOPE_W; x++) {
                pend[v][x] = lv->y[v][x];
            }
        }
        level[v] = most * (uint32_t)e;
    }
    level[SCOPE_VOICES] = peak[0];
    level[SCOPE_VOICES + 1u] = peak[1];
    for (uint32_t m = 0; m < LIVE_VIEW_METERS; m++) {
        uint32_t now = scope_meter(level[m]);
        uint32_t was = lv->meter[m];
        uint32_t fell = was > LIVE_VIEW_FALL ? was - LIVE_VIEW_FALL : 0u;
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

void live_view_init(live_view* lv, live* l, scope* sc) {
    memset(lv, 0, sizeof *lv);
    lv->l = l;
    lv->sc = sc;
    flat(lv);
}

void live_view_tap(live_view* lv, int on) {
    if (!on) {
        lv->prepared = 0;
    }
    if (!lv->sc) {
        return;
    }
    if (on && !lv->on) {
        lv->since = lv->sc->head;
    }
    if (!on && lv->on) {
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
}

uint8_t live_view_voice_control(const live* l, uint32_t v) {
    return voice_control(l, v);
}

uint8_t live_view_wave_color(uint8_t control) {
    return wave_color(control);
}

static void hz_text(char* buf, uint32_t hz, uint32_t knob) {
    uint32_t n = 0;
    char digits[4];
    uint32_t k = 0;
    uint32_t whole = hz < 1000u ? hz : (hz + (knob && hz >= 9950u ? 500u : 50u)) / 1000u;
    do {
        digits[k++] = (char)('0' + whole % 10u);
        whole /= 10u;
    } while (whole);
    while (k) {
        buf[n++] = digits[--k];
    }
    if (hz >= 1000u) {
        char tenth = (char)('0' + (hz + 50u) / 100u % 10u);
        if (!knob) {
            buf[n++] = '.';
            buf[n++] = tenth;
            buf[n++] = 'K';
        } else {
            buf[n++] = 'K';
            if (hz < 9950u) {
                buf[n++] = tenth;
            }
        }
    }
    buf[n] = 0;
}

void live_view_fc_text(const live* l, uint32_t fc, char* buf) {
    float hz = sid_cutoff_hz(l->model, 0, fc);
    hz_text(buf, (uint32_t)((hz < 30.0f ? 30.0f : hz) + 0.5f), 0);
}

void live_view_cutoff_text(const live* l, char* buf) {
    live_view_fc_text(l, l->cutoff_out, buf);
}

typedef live_view_canvas canvas;

static void put(canvas* cv, uint32_t col, uint32_t row, uint8_t ch, uint8_t attr) {
    if (col < GRID_COLS && row < GRID_ROWS) {
        cv->ch[row][col] = ch;
        cv->attr[row][col] = attr;
    }
}

static uint32_t text(canvas* cv, uint32_t col, uint32_t row, const char* s, uint8_t attr) {
    if (row >= GRID_ROWS) {
        for (; *s; s++) {
            col++;
        }
        return col;
    }
    uint8_t* ch = cv->ch[row];
    uint8_t* at = cv->attr[row];
    for (; *s; s++, col++) {
        if (col < GRID_COLS) {
            ch[col] = (uint8_t)*s;
            at[col] = attr;
        }
    }
    return col;
}

static void fill(canvas* cv, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint8_t ch,
                 uint8_t attr) {
    if (col >= GRID_COLS || row >= GRID_ROWS) {
        return;
    }
    w = w < GRID_COLS - col ? w : GRID_COLS - col;
    h = h < GRID_ROWS - row ? h : GRID_ROWS - row;
    for (uint32_t r = row; r < row + h; r++) {
        memset(&cv->ch[r][col], ch, w);
        memset(&cv->attr[r][col], attr, w);
    }
}

static int muted(const live_view* lv, uint32_t v) {
    uint32_t solo = (uint32_t)lv->mutes >> 3 & 7u;
    return solo ? !(solo >> v & 1u) : (int)((uint32_t)lv->mutes >> v & 1u);
}

static uint32_t lane_row(uint32_t v) {
    return LIVE_VIEW_LANE_ROW + v * LIVE_VIEW_LANE_ROWS;
}

static uint32_t scope_top(uint32_t v) {
    return lane_row(v);
}

static void columns(const uint8_t y[SCOPE_W], uint32_t w[SCOPE_W]) {
    uint32_t prev = y[0];
    for (uint32_t x = 0; x < SCOPE_W; x++) {
        uint32_t now = y[x];
        uint32_t lo = now < prev ? now : prev;
        uint32_t hi = now < prev ? prev : now;
        prev = now;
        lo = lo < LIVE_VIEW_SCOPE_PX - 1u ? lo : LIVE_VIEW_SCOPE_PX - 1u;
        hi = hi + 1u < LIVE_VIEW_SCOPE_PX - 1u ? hi + 1u : LIVE_VIEW_SCOPE_PX - 1u;
        w[x] = (0xFFFFFFFFu >> lo) & ~(0x7FFFFFFFu >> hi);
    }
}

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

static uint8_t trace_color(const live_view* lv, const theme* th, uint32_t v) {
    return lv->env[v] ? th->voice[v] : th->dim;
}

static void redraw_voice(live_view* lv, grid* g, uint32_t v, uint8_t color) {
    uint32_t* w = lv->cols[v];
    for (uint32_t c = 0; c < LIVE_VIEW_SCOPE_COLS; c++) {
        for (uint32_t r = 0; r < LIVE_VIEW_SCOPE_ROWS; r++) {
            cell_apply(g, v, c, r, w, color);
        }
    }
    lv->color[v] = color;
}

static void recolor_voice(grid* g, uint32_t v, uint8_t color) {
    for (uint32_t r = 0; r < LIVE_VIEW_SCOPE_ROWS; r++) {
        for (uint32_t c = 0; c < LIVE_VIEW_SCOPE_COLS; c++) {
            grid_overlay_color(g, LIVE_VIEW_SCOPE_COL + c, scope_top(v) + r, color);
        }
    }
}

static void scopes(live_view* lv, const screen* s, grid* g, const theme* th,
                   uint8_t pend[SCOPE_VOICES][SCOPE_W]) {
    uint32_t w[SCOPE_W];
    uint32_t cells[3];
    if (!lv->drawn || g->ovl_nfree != lv->ovl_free) {
        lv->stale = (1u << SCOPE_VOICES) - 1u;
    }
    lv->drawn = 1;
    if (!pend) {
        for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
            if (lv->stale >> v & 1u) {
                redraw_voice(lv, g, v, trace_color(lv, th, v));
                lv->stale &= (uint8_t)~(1u << v);
                break;
            }
        }
        lv->ovl_free = g->ovl_nfree;
        return;
    }
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        uint8_t color = trace_color(lv, th, v);
        if (color != lv->color[v] && !(lv->stale >> v & 1u)) {
            recolor_voice(g, v, color);
            lv->color[v] = color;
        }
    }
    uint32_t used = screen_pending(s) + screen_layer_cells(s);
    uint32_t left = used < LIVE_VIEW_FRAME_CELLS ? LIVE_VIEW_FRAME_CELLS - used : 0u;
    uint32_t first = lv->next_voice % SCOPE_VOICES;
    uint32_t skipped = SCOPE_VOICES;
    for (uint32_t k = 0; k < SCOPE_VOICES; k++) {
        uint32_t v = (first + k) % SCOPE_VOICES;
        uint8_t color = trace_color(lv, th, v);
        int known = !(lv->stale >> v & 1u);
        if (known) {
            uint32_t x = 0;
            while (x < SCOPE_W && pend[v][x] == lv->y[v][x]) {
                x++;
            }
            if (x == SCOPE_W) {
                continue;
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
        for (uint32_t x = 0; x < SCOPE_W; x++) {
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

static void note_text(const live_view* lv, uint32_t v, char note[4]) {
    const live_voice* x = &lv->l->voice[v];
    note[0] = note[1] = note[2] = '-';
    note[3] = 0;
    if (voice_control(lv->l, v) && x->note != LIVE_NO_NOTE) {
        note[0] = screen_note_names[x->note % 12u][0];
        note[1] = screen_note_names[x->note % 12u][1];
        note[2] = x->note < 12u ? '-' : (char)('0' + x->note / 12u - 1u);
    }
}

static void header(const live_view* lv, canvas* cv, const theme* th) {
    (void)lv;
    fill(cv, 0, 0, GRID_COLS, 2, ' ', THEME_ATTR(th, value));
}

__attribute__((noinline, unused)) LIVE_COLD static void header_big(const live_view* lv, grid* g, const theme* th) {
    char name[LIVE_PRESET_NAME_CHARS];
    live_preset_name(lv->l, name);
    grid_text_big(g, 0, 0, 2, name, THEME_ATTR(th, value));
    if (lv->playing) {
        const char mark[2] = {(char)FONT_PLAY, 0};
        grid_text_big(g, PLAY_COL, 0, 2, mark,
                      lv->beat == 2u ? THEME_ATTR(th, hot) : lv->beat ? THEME_ATTR(th, value)
                                                                      : THEME_ATTR(th, label));
    }
    int32_t o = lv->l->octave;
    if (o) {
        char oct[3] = {o < 0 ? '-' : '+', (char)('0' + (o < 0 ? -o : o)), 0};
        grid_text_big(g, GRID_COLS - 4u, 0, 2, oct, THEME_ATTR(th, hot));
    }
}

static void lane(const live_view* lv, canvas* cv, const theme* th, uint32_t v) {
    uint32_t row = lane_row(v);
    const uint8_t blank = THEME_ATTR(th, value);
    fill(cv, 0, row, GRID_COLS, LIVE_VIEW_LANE_ROWS, ' ', blank);
    fill(cv, LIVE_VIEW_SCOPE_COL, row + MID / 8u, LIVE_VIEW_SCOPE_COLS, 1, FONT_SCOPE_MID,
         THEME_ATTR(th, dim));
    const char* wn = wave_name(voice_control(lv->l, v));
    char w3[4] = {wn[0], wn[1], wn[2], 0};
    text(cv, 0, row + 3u, w3, THEME_ATTR(th, label));
    const char* st = stage_name(lv->state[v], lv->env[v], lv->sustain[v]);
    text(cv, NOTE_COL, row + 3u, st, st[0] == '-' ? THEME_ATTR(th, dim) : THEME_ATTR(th, label));
}

static void lane_big(const live_view* lv, grid* g, const theme* th, uint32_t v) {
    uint32_t row = lane_row(v) + 1u;
    char digit[2] = {(char)('1' + v), 0};
    grid_text_big(g, 0, row, 2, digit, GRID_ATTR(muted(lv, v) ? th->dim : th->voice[v], th->bg));
    char note[4];
    note_text(lv, v, note);
    const live_voice* x = &lv->l->voice[v];
    int on = voice_control(lv->l, v) && x->gate;
    grid_text_big(g, NOTE_COL, row, 2, note, GRID_ATTR(on ? th->value : th->dim, th->bg));
}

void live_view_knob_text(const live* l, uint32_t k, char* buf) {
    uint32_t n = 0;
    switch (k) {
    case 0: {
        float fc = sid_cutoff_hz(l->model, 0, l->cutoff_out);
        hz_text(buf, (uint32_t)((fc < 30.0f ? 30.0f : fc) + 0.5f), 1);
        return;
    }
    case 1:
        if (l->res_out >= 10u) {
            buf[n++] = '1';
        }
        buf[n++] = (char)('0' + l->res_out % 10u);
        break;
    case 2: {
        uint32_t tone = live_tone(l);
        if (tone == LIVE_TONE_RATIO) {
            int32_t s = l->sync_semis;
            uint32_t a = (uint32_t)(s < 0 ? -s : s);
            buf[n++] = s < 0 ? '-' : '+';
            if (a >= 10u) {
                buf[n++] = (char)('0' + a / 10u);
            }
            buf[n++] = (char)('0' + a % 10u);
        } else if (tone == LIVE_TONE_PULSE) {
            uint32_t pw = l->pulse_width > 2048u ? 4096u - l->pulse_width : l->pulse_width;
            uint32_t pc = (pw * 99u + 2047u) / 4095u;
            if (pc >= 10u) {
                buf[n++] = (char)('0' + pc / 10u);
            }
            buf[n++] = (char)('0' + pc % 10u);
            buf[n++] = '%';
        } else {
            uint32_t d = l->attack_decay & 15u;
            if (d >= 10u) {
                buf[n++] = '1';
            }
            buf[n++] = (char)('0' + d % 10u);
        }
        break;
    }
    default: {
        uint32_t r = l->sustain_release & 15u;
        if (r >= 10u) {
            buf[n++] = '1';
        }
        buf[n++] = (char)('0' + r % 10u);
        break;
    }
    }
    buf[n] = 0;
}

static inline __attribute__((always_inline)) int slot_page(const live_view* lv) {
    return lv->page || lv->edit;
}

LIVE_COLD void live_view_arp_text(const live* l, uint32_t k, char* buf) {
    static const char ORDER[4][3] = {"UP", "DN", "UD", "AS"};
    uint32_t n = 0;
    if (k == 0u) {
        uint32_t steps = live_arp_steps[l->arp_rate % LIVE_ARP_RATES];
        if (l->arp_rate >= LIVE_ARP_RATE_CLOCK) {
            uint32_t note = 96u / steps;
            buf[n++] = '/';
            if (note >= 10u) {
                buf[n++] = (char)('0' + note / 10u);
            }
            buf[n++] = (char)('0' + note % 10u);
        } else {
            buf[n++] = (char)('0' + steps);
            buf[n++] = 'F';
        }
    } else if (k == 1u) {
        buf[n++] = ORDER[l->arp_order & 3u][0];
        buf[n++] = ORDER[l->arp_order & 3u][1];
    } else {
        buf[n++] = (char)('0' + l->arp_span);
    }
    buf[n] = 0;
}

LIVE_COLD static int knob_label(const live_view* lv, uint32_t k, char* buf) {
    static const char* const TONE[4] = {"CUTOFF", "RESO", "PULSE", "RELEASE"};
    static const char* const K3[3] = {"RATIO", "PULSE", "DECAY"};
    static const char* const ARP[4] = {"RATE", "ORDER", "SPAN", "-"};
    if (lv->arp || !slot_page(lv)) {
        const char* t = lv->arp ? ARP[k] : k == 2u ? K3[live_tone(lv->l)] : TONE[k];
        uint32_t n = 0;
        for (; t[n]; n++) {
            buf[n] = t[n];
        }
        buf[n] = 0;
        return !lv->arp || k < 3u;
    }
    const live* l = lv->l;
    uint32_t slot = l->fx_select % LIVE_SLOTS;
    return chip_view_label(slot, l->slot[slot][LIVE_SLOT_ALGORITHM], LIVE_SLOT_AMOUNT + k, buf);
}

__attribute__((noinline, unused)) LIVE_COLD static void knob_value(const live_view* lv, uint32_t k, char* buf) {
    if (lv->arp) {
        if (k < 3u) {
            live_view_arp_text(lv->l, k, buf);
        } else {
            buf[0] = 0;
        }
        return;
    }
    if (!slot_page(lv)) {
        live_view_knob_text(lv->l, k, buf);
        return;
    }
    const live* l = lv->l;
    uint32_t v = l->slot[l->fx_select % LIVE_SLOTS][LIVE_SLOT_AMOUNT + k];
    uint32_t n = 0;
    if (v >= 100u) {
        buf[n++] = (char)('0' + v / 100u);
    }
    if (v >= 10u) {
        buf[n++] = (char)('0' + v / 10u % 10u);
    }
    buf[n++] = (char)('0' + v % 10u);
    buf[n] = 0;
}

__attribute__((noinline, unused)) LIVE_COLD static uint32_t knob_px(const live_view* lv, uint32_t k, uint32_t cells) {
    const live* l = lv->l;
    uint32_t value, max;
    if (lv->arp) {
        static const uint8_t MAX[4] = {LIVE_ARP_RATES - 1u, LIVE_ARP_PLAYED, LIVE_ARP_SPAN_MAX - 1u, 1u};
        value = k == 0u ? l->arp_rate : k == 1u ? l->arp_order : k == 2u ? l->arp_span - 1u : 0u;
        max = MAX[k & 3u];
    } else if (slot_page(lv)) {
        value = l->slot[l->fx_select % LIVE_SLOTS][LIVE_SLOT_AMOUNT + k];
        max = LIVE_SLOT_MAX;
    } else {
        uint32_t tone = live_tone(l);
        value = k == 0u   ? l->cutoff_out
                : k == 1u ? l->res_out
                : k == 3u ? (uint32_t)(l->sustain_release & 15u)
                : tone == LIVE_TONE_RATIO ? (uint32_t)(l->sync_semis + LIVE_RATIO_SEMIS)
                : tone == LIVE_TONE_PULSE ? (l->pulse_width > 2048u ? 4096u - l->pulse_width
                                                                   : l->pulse_width)
                                          : (uint32_t)(l->attack_decay & 15u);
        max = k == 0u   ? LIVE_CUTOFF_MAX
              : k == 1u ? LIVE_RESONANCE_MAX
              : k == 3u ? 15u
              : tone == LIVE_TONE_RATIO ? 2u * LIVE_RATIO_SEMIS
              : tone == LIVE_TONE_PULSE ? 2048u
                                        : 15u;
    }
    return (value > max ? max : value) * (cells * 8u) / max;
}

LIVE_COLD static void knobs(const live_view* lv, canvas* cv, const theme* th, uint32_t hl) {
    fill(cv, 0, LIVE_VIEW_PAGE_ROW, GRID_COLS, LIVE_VIEW_TAG_ROW_END - LIVE_VIEW_PAGE_ROW, ' ',
         THEME_ATTR(th, value));
    text(cv, 0, LIVE_VIEW_PAGE_ROW,
         lv->arp ? "ARP" : slot_page(lv) ? views_slot_name(lv->l->fx_select % LIVE_SLOTS) : "TONE",
         THEME_ATTR(th, hot));
    for (uint32_t k = 0; k < 4u; k++) {
        uint32_t col = KNOB_COL[k];
        uint32_t cells = KNOB_W[k] - 1u;
        char name[CHIP_VIEW_LABEL + 2u];
        int used = knob_label(lv, k, name);
        uint8_t fg = used ? th->knob_text[k] : th->dim;
        text(cv, col, LIVE_VIEW_KNOB_ROW, name,
             hl == k + 1u ? GRID_ATTR(th->bg, fg) : GRID_ATTR(fg, th->bg));
        uint32_t px = knob_px(lv, k, cells);
        for (uint32_t i = 0; i < cells; i++) {
            uint32_t f = px >= 8u ? 8u : px;
            px -= f;
            cv->ch[LIVE_VIEW_KNOB_BAR_ROW][col + i] = (uint8_t)(FONT_BAR0 + f);
            cv->attr[LIVE_VIEW_KNOB_BAR_ROW][col + i] =
                GRID_ATTR(used ? th->knob[k] : th->dim, th->dim);
        }
        char tag[3] = {'K', (char)('1' + k), 0};
        text(cv, col, LIVE_VIEW_KNOB_TAG_ROW, tag, THEME_ATTR(th, hint));
    }
}

__attribute__((noinline, unused)) LIVE_COLD static void knobs_big(const live_view* lv, grid* g, const theme* th) {
    for (uint32_t k = 0; k < 4u; k++) {
        char v[5];
        int used = 1;
        if (slot_page(lv) || lv->arp) {
            char name[CHIP_VIEW_LABEL + 2u];
            used = knob_label(lv, k, name);
        }
        knob_value(lv, k, v);
        grid_text_big(g, KNOB_COL[k], LIVE_VIEW_KNOB_VALUE_ROW, 2, v,
                      used ? THEME_ATTR(th, value) : THEME_ATTR(th, dim));
    }
}

static const uint8_t METER_LABEL[LIVE_VIEW_METERS] = {'1', '2', '3', 'L', 'R'};

static void meter_bar(const live_view* lv, canvas* cv, grid* g, const theme* th, uint32_t m) {
    uint32_t row = LIVE_VIEW_METER_ROW + m;
    uint32_t px = lv->meter[m];
    uint8_t fg = m < SCOPE_VOICES ? th->voice[m] : th->value;
    uint8_t* ch = &cv->ch[row][VU_METER_COL];
    uint8_t* at = &cv->attr[row][VU_METER_COL];
    uint8_t on = GRID_ATTR(fg, th->bg);
    uint8_t off = THEME_ATTR(th, value);
NO_UNROLL
    for (uint32_t c = 0; c < VU_METER_CELLS; c++) {
        uint32_t fill8 = px >= 8u ? 8u : px;
        px -= fill8;
        ch[c] = fill8 ? (uint8_t)(FONT_BAR0 + fill8) : (uint8_t)' ';
        at[c] = fill8 ? on : off;
    }
    if (g) {
NO_UNROLL
        for (uint32_t c = 0; c < VU_METER_CELLS; c++) {
            grid_put(g, VU_METER_COL + c, row, ch[c], at[c]);
        }
    }
}

__attribute__((noinline, unused)) LIVE_COLD static void profile_word(const live* l, char out[6]) {
    uint32_t k = 0;
    if (l->user_applied) {
        out[k++] = 'U';
        out[k++] = 'S';
        out[k++] = 'R';
        out[k++] = (char)('0' + l->user_applied);
        out[k] = 0;
        return;
    }
    const sid_chip_profile* p = sid_chip_profile_get(l->profile_req);
    const char* w = p ? p->name : "?";
    if (w[0] >= '0' && w[0] <= '9') {
        while (*w && *w != ' ') {
            w++;
        }
        w += *w == ' ';
    }
    for (; *w && *w != ' ' && k < 4u; w++) {
        out[k++] = *w >= 'a' && *w <= 'z' ? (char)(*w - 'a' + 'A') : *w;
    }
    out[k] = 0;
}

LIVE_COLD static void strip(const live_view* lv, canvas* cv, const theme* th) {
    fill(cv, 0, LIVE_VIEW_STRIP_ROW, GRID_COLS, 1, ' ', THEME_ATTR(th, value));
    fill(cv, 0, LIVE_VIEW_ARP_ROW, GRID_COLS, 2, ' ', THEME_ATTR(th, value));
    if (lv->playing && lv->bpm) {
        char b[4];
        uint32_t n = 0;
        for (uint32_t d = lv->bpm >= 100u ? 100u : lv->bpm >= 10u ? 10u : 1u; d; d /= 10u) {
            b[n++] = (char)('0' + lv->bpm / d % 10u);
        }
        b[n] = 0;
        text(cv, text(cv, 0, LIVE_VIEW_STRIP_ROW, b, THEME_ATTR(th, label)), LIVE_VIEW_STRIP_ROW,
             " BPM", THEME_ATTR(th, label));
    }
    for (uint32_t v = 0; v < SCOPE_VOICES && (lv->playing || lv->mutes); v++) {
        put(cv, BADGE_COL + 2u * v, LIVE_VIEW_STRIP_ROW, (uint8_t)('1' + v),
            lv->mutes >> (3u + v) & 1u ? GRID_ATTR(th->bg, th->hot)
            : muted(lv, v)             ? THEME_ATTR(th, dim)
                                       : THEME_ATTR(th, label));
    }
}

__attribute__((noinline, unused)) LIVE_COLD static void arp_big(const live_view* lv, grid* g, const theme* th) {
    if (!lv->l->arp_on) {
        return;
    }
    char w[16];
    uint32_t n = 0;
    for (uint32_t k = 0; k < 3u; k++) {
        live_view_arp_text(lv->l, k, &w[n]);
        while (w[n]) {
            n++;
        }
        w[n++] = ' ';
    }
    w[n - 1u] = 'O';
    w[n++] = 'C';
    w[n++] = 'T';
    w[n] = 0;
    grid_text_big(g, 0, LIVE_VIEW_ARP_ROW, 2, "ARP", THEME_ATTR(th, hot));
    grid_text_big(g, 8, LIVE_VIEW_ARP_ROW, 2, w, THEME_ATTR(th, value));
}

__attribute__((noinline, unused)) LIVE_COLD static void band(const live_view* lv, canvas* cv, const theme* th) {
    const live* l = lv->l;
    fill(cv, 0, LIVE_VIEW_BAND_ROW, GRID_COLS, LIVE_VIEW_BAND_ROWS, ' ', THEME_ATTR(th, value));
    if (lv->glo & 1u) {
        text(cv, 0, LIVE_VIEW_BAND_ROW, lv->glo & 2u ? "GLO LATCHED" : "GLO  MUTE SOLO",
             THEME_ATTR(th, hot));
        return;
    }
    text(cv, 0, LIVE_VIEW_BAND_ROW, "FILTER", THEME_ATTR(th, label));
    text(cv, MODEL_COL, LIVE_VIEW_BAND_ROW, l->model == SID_MODEL_8580 ? "CHIP 8580" : "CHIP 6581",
         THEME_ATTR(th, label));
    if (l->two_chips && l->keys_chip) {
        text(cv, GRID_COLS - 6u, LIVE_VIEW_BAND_ROW, "KEYS:B", THEME_ATTR(th, hot));
    } else {
        text(cv, MODEL_COL + 10u, LIVE_VIEW_BAND_ROW,
             l->user_applied ? "USER" : sid_chip_profile_word(l->profile_req),
             THEME_ATTR(th, label));
    }
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        uint8_t fg = l->slot[k][LIVE_SLOT_ALGORITHM] ? th->value : th->dim;
        text(cv, SLOT_COL[k], LIVE_VIEW_SLOT_ROW, views_slot_name(k),
             k == l->fx_select ? GRID_ATTR(th->bg, th->hot) : GRID_ATTR(fg, th->bg));
    }
    for (uint32_t m = 0; m < LIVE_VIEW_METERS; m++) {
        put(cv, VU_LABEL_COL, LIVE_VIEW_METER_ROW + m, METER_LABEL[m], THEME_ATTR(th, hint));
        meter_bar(lv, cv, NULL, th, m);
    }
}

__attribute__((noinline, unused)) LIVE_COLD static void band_big(const live_view* lv, grid* g, const theme* th, uint32_t hl) {
    const live* l = lv->l;
    if (lv->glo & 1u) {
        static const char* const BOX[6] = {"M1", "M2", "M3", "S1", "S2", "S3"};
        for (uint32_t i = 0; i < 6u; i++) {
            uint8_t lit = i < 3u ? th->alert : th->hot;
            grid_text_big(g, 5u * i, LIVE_VIEW_MODE_ROW, 2, BOX[i],
                          lv->mutes >> i & 1u ? GRID_ATTR(th->bg, lit) : THEME_ATTR(th, label));
        }
        return;
    }
    uint32_t mode = l->filter_mode >> 4 & 7u;
    grid_text_big(g, 0, LIVE_VIEW_MODE_ROW, 2, MODE_NAMES[mode],
                  GRID_ATTR(mode ? th->value : th->dim, th->bg));
    char word[6];
    profile_word(l, word);
    grid_text_big(g, MODEL_COL, LIVE_VIEW_MODE_ROW, 2, word,
                  hl == LIVE_VIEW_HL_PROFILE ? GRID_ATTR(th->bg, th->value) : THEME_ATTR(th, value));
    uint32_t k = l->fx_select % LIVE_SLOTS;
    uint32_t alg = l->slot[k][LIVE_SLOT_ALGORITHM];
    grid_text_big(g, 0, LIVE_VIEW_ALG_ROW, 2, views_algorithm_name(k, alg),
                  slot_page(lv) || hl == LIVE_VIEW_HL_ALGORITHM
                      ? GRID_ATTR(th->bg, th->hot)
                      : alg ? THEME_ATTR(th, value) : THEME_ATTR(th, dim));
}

#define KEY_THEME 0u
#define KEY_HEADER 1u
#define KEY_VOICE 5u
#define KEY_STRIP (KEY_VOICE + 6u * SCOPE_VOICES)
#define KEY_KNOBS (KEY_STRIP + 6u)
#define KEY_BAND (KEY_KNOBS + 11u)
#define KEY_METER (KEY_BAND + 11u)
#define KEY_END (KEY_METER + LIVE_VIEW_METERS)
_Static_assert(KEY_END == LIVE_VIEW_LOOK, "LIVE_VIEW_LOOK holds every part's bytes");

static uint8_t highlight(const live_view* lv, uint32_t now) {
    return lv->hl && (int32_t)(now - lv->hl_until) < 0 ? lv->hl : 0u;
}

void live_view_flash(live_view* lv, uint32_t what, uint32_t now_ms) {
    lv->hl = (uint8_t)what;
    lv->hl_until = now_ms + SCREEN_POPUP_MS;
}

static void look(const live_view* lv, uint8_t out[LIVE_VIEW_LOOK], uint32_t now) {
    const live* l = lv->l;
    uint8_t hl = highlight(lv, now);
    out[KEY_THEME] = l->theme;
    out[KEY_HEADER] = l->preset;
    out[KEY_HEADER + 1u] = (uint8_t)l->octave;
    out[KEY_HEADER + 2u] = l->preset == LIVE_PRESET_DICE ? l->dice_rolls : (uint8_t)live_preset_edited(l);
    out[KEY_HEADER + 3u] = (uint8_t)(lv->playing | lv->beat << 1);
    out[KEY_STRIP] = lv->playing;
    out[KEY_STRIP + 1u] = (uint8_t)lv->bpm;
    out[KEY_STRIP + 2u] = (uint8_t)(lv->bpm >> 8);
    out[KEY_STRIP + 3u] = lv->mutes;
    out[KEY_STRIP + 4u] = (uint8_t)(l->arp_on | l->arp_order << 1 | l->arp_span << 3);
    out[KEY_STRIP + 5u] = l->arp_rate;
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        uint8_t ctrl = voice_control(l, v);
        const live_voice* x = &l->voice[v];
        uint8_t* k = &out[KEY_VOICE + 6u * v];
        k[0] = ctrl;
        k[1] = ctrl ? x->note : LIVE_NO_NOTE;
        k[2] = (uint8_t)(ctrl && x->gate);
        k[3] = (uint8_t)stage_name(lv->state[v], lv->env[v], lv->sustain[v])[0];
        k[4] = (uint8_t)(lv->env[v] != 0u);
        k[5] = (uint8_t)muted(lv, v);
    }
    uint8_t* k = &out[KEY_KNOBS];
    uint32_t slot = l->fx_select % LIVE_SLOTS;
    k[0] = (uint8_t)(lv->arp ? 0x10u : slot_page(lv) ? 1u + slot : 0u);
    k[1] = hl >= 1u && hl <= 4u ? hl : 0u;
    k[2] = slot_page(lv) && !lv->arp ? l->slot[slot][LIVE_SLOT_ALGORITHM] : 0u;
    if (lv->arp) {
        k[3] = l->arp_rate;
        k[4] = l->arp_order;
        k[5] = l->arp_span;
        k[6] = k[7] = k[8] = k[9] = k[10] = 0;
    } else if (slot_page(lv)) {
        for (uint32_t f = 0; f < 4u; f++) {
            k[3u + f] = l->slot[slot][LIVE_SLOT_AMOUNT + f];
        }
        k[7] = k[8] = k[9] = k[10] = 0;
    } else {
        k[3] = (uint8_t)l->cutoff_out;
        k[4] = (uint8_t)(l->cutoff_out >> 8);
        k[5] = l->res_out;
        k[6] = (uint8_t)l->pulse_width;
        k[7] = (uint8_t)(l->pulse_width >> 8);
        k[8] = (uint8_t)(l->sustain_release & 15u);
        k[9] = (uint8_t)(l->attack_decay & 15u);
        k[10] = (uint8_t)((uint8_t)l->sync_semis ^ live_tone(l) << 6);
    }
    uint8_t* b = &out[KEY_BAND];
    b[0] = (uint8_t)(l->filter_mode >> 4 & 7u);
    b[1] = (uint8_t)(l->model | (l->two_chips && l->keys_chip) << 1);
    b[2] = l->profile_req;
    b[3] = l->user_applied;
    b[4] = l->fx_select;
    for (uint32_t i = 0; i < LIVE_SLOTS; i++) {
        b[5u + i] = l->slot[i][LIVE_SLOT_ALGORITHM];
    }
    b[8] = (uint8_t)((hl >= LIVE_VIEW_HL_PROFILE ? hl : 0u) | slot_page(lv) << 4);
    b[9] = lv->glo;
    b[10] = lv->mutes;
    for (uint32_t m = 0; m < LIVE_VIEW_METERS; m++) {
        out[KEY_METER + m] = lv->meter[m];
    }
}

static int differs(const live_view* lv, const uint8_t* now, uint32_t from, uint32_t n) {
    for (uint32_t i = from; i < from + n; i++) {
        if (now[i] != lv->drawn_look[i]) {
            return 1;
        }
    }
    return 0;
}

static uint32_t parts(live_view* lv, canvas* cv, const theme* th,
                      const uint8_t now[LIVE_VIEW_LOOK]) {
    uint32_t rows = 0;
    if (differs(lv, now, KEY_HEADER, 4u)) {
        header(lv, cv, th);
        rows |= HEADER_ROWS;
    }
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        if (differs(lv, now, KEY_VOICE + 6u * v, 6u)) {
            lane(lv, cv, th, v);
            rows |= ROWS(lane_row(v), LIVE_VIEW_LANE_ROWS);
        }
    }
    if (differs(lv, now, KEY_STRIP, 6u)) {
        strip(lv, cv, th);
        rows |= ROWS(LIVE_VIEW_STRIP_ROW, 1u) | ROWS(LIVE_VIEW_ARP_ROW, 2u);
    }
    if (differs(lv, now, KEY_KNOBS, 11u)) {
        knobs(lv, cv, th, now[KEY_KNOBS + 1u]);
        rows |= KNOB_ROWS;
    }
    if (differs(lv, now, KEY_BAND, 11u)) {
        band(lv, cv, th);
        rows |= BAND_ROWS;
    }
    return rows;
}

static void build(live_view* lv, canvas* cv, const theme* th, const uint8_t now[LIVE_VIEW_LOOK]) {
    fill(cv, 0, LIVE_VIEW_LANE_ROW + SCOPE_VOICES * LIVE_VIEW_LANE_ROWS,
         GRID_COLS, LIVE_VIEW_PAGE_ROW - LIVE_VIEW_LANE_ROW - SCOPE_VOICES * LIVE_VIEW_LANE_ROWS, ' ',
         THEME_ATTR(th, value));
    fill(cv, 0, LIVE_VIEW_BAND_ROW + LIVE_VIEW_BAND_ROWS, GRID_COLS,
         GRID_ROWS - LIVE_VIEW_BAND_ROW - LIVE_VIEW_BAND_ROWS, ' ', THEME_ATTR(th, value));
    strip(lv, cv, th);
    header(lv, cv, th);
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        lane(lv, cv, th, v);
    }
    knobs(lv, cv, th, now[KEY_KNOBS + 1u]);
    band(lv, cv, th);
}

static void big_items(const live_view* lv, grid* g, const theme* th, uint32_t rows, uint32_t hl) {
    if (rows & HEADER_ROWS) {
        header_big(lv, g, th);
    }
    for (uint32_t v = 0; v < SCOPE_VOICES; v++) {
        if (rows & ROWS(lane_row(v) + 1u, 2u)) {
            lane_big(lv, g, th, v);
        }
    }
    if (rows & ROWS(LIVE_VIEW_KNOB_VALUE_ROW, 2u)) {
        knobs_big(lv, g, th);
    }
    if (rows & (ROWS(LIVE_VIEW_MODE_ROW, 2u) | ROWS(LIVE_VIEW_ALG_ROW, 2u))) {
        band_big(lv, g, th, hl);
    }
    if (rows & ROWS(LIVE_VIEW_ARP_ROW, 2u)) {
        arp_big(lv, g, th);
    }
}

static int input(screen* s, void* state, const screen_event* e) {
    (void)s;
    live_view* lv = state;
    if (e->id - PANEL_ENC_K1 <= 3u && !lv->page && !(e->held >> PANEL_BTN_ARP & 1u)) {
        return 0;
    }
    live* l = lv->l;
    if (e->kind != SCREEN_TURN) {
        if (e->id != PANEL_BTN_ARP) {
            return 0;
        }
        lv->arp_down = 1;
        lv->arp_turned = 0;
        return 1;
    }
    if ((e->held >> PANEL_BTN_ARP & 1u) && e->id - PANEL_ENC_K1 <= 3u) {
        lv->arp_turned = 1;
        live_arp_turn(l, LIVE_ARP_RATE + e->id - PANEL_ENC_K1, e->steps);
        return 1;
    }
    if (e->id <= PANEL_ENC_ALGORITHM) {
        uint32_t slot_held = lv->page || (e->held >> PANEL_BTN_EDIT & 1u);
        if (e->id == PANEL_ENC_SELECT) {
            int32_t at = lv->page ? 1 + (int32_t)(l->fx_select % LIVE_SLOTS) : 0;
            int32_t to = at + e->steps;
            to = to < 0 ? 0 : to > (int32_t)LIVE_SLOTS ? (int32_t)LIVE_SLOTS : to;
            lv->page = (uint8_t)(to != 0);
            if (to) {
                live_fx_select(l, (uint32_t)to - 1u);
            }
            return 1;
        }
        if (slot_held) {
            uint32_t k = l->fx_select % LIVE_SLOTS;
            int32_t to = (int32_t)l->slot[k][LIVE_SLOT_ALGORITHM] + e->steps;
            live_slot_pick(l, k, to < 0 ? 0u : (uint32_t)to);
            return 1;
        }
        int32_t base = l->user_applied ? l->user_base[l->user_applied - 1u] : l->profile_req;
        int32_t to = base + e->steps;
        live_param_set(l, LIVE_PARAM_PROFILE, (uint32_t)(to < 0 ? 0 : to > (int32_t)SID_CHIPS - 1 ? (int32_t)SID_CHIPS - 1 : to));
        return 1;
    }
    if (lv->page && e->id - PANEL_ENC_K1 <= 3u) {
        live_slot_turn(l, l->fx_select % LIVE_SLOTS, LIVE_SLOT_AMOUNT + e->id - PANEL_ENC_K1, e->steps);
        return 1;
    }
    return 0;
}

void live_view_buttons(live_view* lv, uint32_t held) {
    if (lv->arp_down && !(held >> PANEL_BTN_ARP & 1u)) {
        lv->arp_down = 0;
        if (!lv->arp_turned) {
            live_arp_set(lv->l, LIVE_ARP_ON, !lv->l->arp_on);
        }
    }
}

static void live_compose(screen* s, void* state, grid* g) {
    live_view* lv = state;
    canvas* cv = &lv->cv;
    const theme* th = theme_get(lv->l->theme);
    int fresh = !screen_animating(s);
    if (fresh) {
        if (!lv->prepared) {
            update(lv, lv->pend);
        }
        lv->prepared = 0;
    }
    uint8_t now[LIVE_VIEW_LOOK];
    look(lv, now, s->now);
    uint32_t rows;
    uint32_t soft = 0;
    if (!lv->built || now[KEY_THEME] != lv->drawn_look[KEY_THEME]) {
        lv->built = 1;
        lv->rebuilds++;
        build(lv, cv, th, now);
        rows = ALL_ROWS;
        lv->owed = 0;
    } else {
        rows = parts(lv, cv, th, now);
        soft = ((s->redraw ? s->redraw_rows : 0u) | lv->owed) & ~rows;
        rows |= soft;
        lv->owed = 0;
    }
    for (uint32_t r = 0; r < GRID_ROWS; r++) {
        if (rows >> r & 1u) {
            if ((soft >> r & 1u) && fresh &&
                screen_pending(s) + screen_layer_cells(s) + GRID_COLS > LIVE_VIEW_FRAME_CELLS) {
                lv->owed |= 1u << r;
                rows &= ~(1u << r);
                continue;
            }
            grid_row(g, r, cv->ch[r], cv->attr[r]);
        }
    }
    if (rows) {
        big_items(lv, g, th, rows, now[KEY_BAND + 8u] & 15u);
    }
    for (uint32_t m = 0; m < LIVE_VIEW_METERS; m++) {
        if (now[KEY_METER + m] != lv->drawn_look[KEY_METER + m] && !(lv->glo & 1u)) {
            meter_bar(lv, cv, g, th, m);
        }
    }
    memcpy(lv->drawn_look, now, sizeof now);
    scopes(lv, s, g, th, fresh ? lv->pend : NULL);
}

const view live_view_view = {"LIVE", NULL, NULL, input, live_compose, NULL};
