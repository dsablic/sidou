// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "link.h"

#include <string.h>

#include "font.h"
#include "panel.h"
#include "store.h"

#define WANT_HELLO 1u
#define WANT_STOPPED 2u
#define WANT_BEAT 4u
#define WANT_COLOURS 8u
#define NO_GLYPH 0xFFu
#define GLYPHS_A_FRAME ((LINK_PAYLOAD_MAX - 1u) / 8u)
_Static_assert(FONT_LAST < NO_GLYPH, "a glyph code is never NO_GLYPH");
_Static_assert(PANEL_ENCODERS <= 8u && PANEL_KEY_ID0 <= 16u, "the panel fits link_state");

static const uint8_t magic[5] = {0x7D, 'F', 'M', '1', 'L'};

static void all_dirty(link_state* l) {
    memset(l->dirty, 0xFF, sizeof l->dirty);
    l->dirty[sizeof l->dirty - 1u] = (uint8_t)((1u << (GRID_CELLS & 7u)) - 1u);
    l->rows = (1u << GRID_ROWS) - 1u;
}

static void release(link_state* l) {
    l->held = l->pressed = 0;
    memset(l->steps, 0, sizeof l->steps);
}

void link_init(link_state* l, const char* name) {
    memset(l, 0, sizeof *l);
    l->font_next = NO_GLYPH;
    l->name = name;
}

void link_drop(link_state* l) {
    l->on = 0;
    l->want = 0;
    l->font_next = NO_GLYPH;
    release(l);
}

static void command(link_state* l, uint32_t now) {
    uint32_t n = l->n, sum = l->cmd;
    for (uint32_t i = 0; i < n; i++) {
        sum += l->in[i];
    }
    if (!n || (sum & 0x7Fu)) {
        return;
    }
    uint8_t* p = l->in;
    uint32_t len = 0;
    for (uint32_t i = 0; i + 1u < n; i += 8u) {
        uint32_t mask = p[i];
        for (uint32_t k = 1; k < 8u && i + k + 1u < n; k++) {
            p[len++] = (uint8_t)(p[i + k] | ((mask >> (k - 1u)) & 1u) << 7);
        }
    }
    if (l->cmd == LINK_START) {
        l->on = 1;
        l->heard = now;
        l->want = WANT_HELLO;
        l->font_next = NO_GLYPH;
        l->credit = LINK_BURST;
        l->scan = 0;
        release(l);
        all_dirty(l);
        return;
    }
    if (!l->on) {
        l->want |= WANT_STOPPED;
        return;
    }
    switch (l->cmd) {
    case LINK_HEARTBEAT:
        l->heard = now;
        l->want |= WANT_BEAT;
        break;
    case LINK_STOP:
        link_drop(l);
        l->want = WANT_STOPPED;
        break;
    case LINK_KEYFRAME: all_dirty(l); break;
    case LINK_FONT: l->font_next = 0; break;
    case LINK_PALETTE: l->want |= WANT_COLOURS; break;
    case LINK_PANEL:
        if (len >= 4u) {
            uint32_t held = (uint32_t)p[0] | (uint32_t)p[1] << 8;
            l->pressed |= (uint16_t)((uint32_t)p[2] | (uint32_t)p[3] << 8 | (held & ~l->held));
            l->held = (uint16_t)held;
            l->panel_at = now;
            for (uint32_t e = 0; e + 4u < len && e < PANEL_ENCODERS; e++) {
                int32_t s = l->steps[e] + (int8_t)p[4u + e];
                l->steps[e] = (int8_t)(s > 100 ? 100 : s < -100 ? -100 : s);
            }
        }
        break;
    default: break;
    }
}

void link_byte(link_state* l, uint8_t b, uint32_t now_ms) {
    if (b == 0xF0u) {
        l->stage = 1;
        return;
    }
    uint32_t st = l->stage;
    if (!st) {
        return;
    }
    if (b & 0x80u) {
        l->stage = 0;
        if (b == 0xF7u && st == 7u) {
            command(l, now_ms);
        }
        return;
    }
    if (st <= 5u) {
        l->stage = (uint8_t)(b == magic[st - 1u] ? st + 1u : 0u);
    } else if (st == 6u) {
        l->cmd = b;
        l->n = 0;
        l->stage = 7;
    } else if (l->n < LINK_IN_MAX) {
        l->in[l->n++] = b;
    } else {
        l->stage = 0;
    }
}

void link_mark(link_state* l, uint32_t row, uint32_t col, uint32_t len) {
    if (row >= GRID_ROWS) {
        return;
    }
    l->rows |= 1u << row;
    for (uint32_t i = row * GRID_COLS + col, e = i + len; i < e && i < (row + 1u) * GRID_COLS;
         i++) {
        l->dirty[i >> 3] |= (uint8_t)(1u << (i & 7u));
    }
}

uint32_t link_take(link_state* l, uint32_t* pressed) {
    *pressed = l->pressed;
    l->pressed = 0;
    return l->held;
}

int32_t link_steps(link_state* l, uint32_t enc) {
    int32_t s = l->steps[enc & 7u];
    l->steps[enc & 7u] = 0;
    return s;
}

static void put32(uint8_t* p, uint32_t v) {
    for (uint32_t i = 0; i < 4u; i++) {
        p[i] = (uint8_t)(v >> (8u * i));
    }
}

static uint32_t hello(const link_state* l, uint8_t* p) {
    static const uint8_t caps[14] = {
        LINK_VERSION, LINK_HAS_CELLS | LINK_HAS_PIXEL_CELLS | LINK_HAS_PANEL,
        GRID_COLS, GRID_ROWS, GRID_CELL_PX, GRID_CELL_PX, 16, 0, FONT_LAST, GRID_BIG_FIRST,
        GRID_BIG_PIECES, LINK_TIMEOUT_MS / 100u, PANEL_KEY_ID0, PANEL_ENCODERS};
    memcpy(p, caps, sizeof caps);
    put32(p + 14, store_crc32(0, font_glyphs, sizeof font_glyphs));
    put32(p + 18, store_crc32(0, grid_palette, sizeof grid_palette));
    uint32_t n = 22;
    for (const char* s = l->name; s && *s && n < 38u; s++) {
        p[n++] = (uint8_t)*s;
    }
    return n;
}

static int pixel_cell(const grid* g, uint32_t i) {
    uint32_t col = i % GRID_COLS, row = i / GRID_COLS;
    return (g->region_bitmap || (g->region_painter && g->region_peek)) &&
           col - g->region_col < g->region_w && row - g->region_row < g->region_h;
}

static uint32_t record(const grid* g, uint32_t i, int pixels, uint8_t* o) {
    uint8_t ch = g->shown_ch[i];
    o[0] = ch;
    o[1] = g->shown_attr[i];
    o[2] = 0;
    if (!pixels && (uint8_t)(ch - GRID_BIG_FIRST) < GRID_BIG_PIECES) {
        o[2] = g->src[i];
        return 3;
    }
    if (!g->ovl[i]) {
        return 3;
    }
    uint32_t s = g->ovl[i] - 1u;
    o[2] = (uint8_t)(LINK_OVERLAY | (g->ovl_color[s] & 15u));
    memcpy(o + 3, g->ovl_mask[s], 8);
    return 11;
}

static void pixels(const grid* g, uint32_t i, uint8_t* o) {
    uint32_t col = i % GRID_COLS, row = i / GRID_COLS;
    uint8_t px[64];
    const grid_bitmap* b = g->region_bitmap;
    if (b) {
        for (uint32_t r = 0; r < 8u; r++) {
            grid_bitmap_row(b, (g->region_bx + col - g->region_col) * 8u,
                            (g->region_by + row - g->region_row) * 8u + r, 8, px + r * 8u);
        }
    } else {
        g->region_peek(g->region_ctx, col, row, px);
    }
    for (uint32_t k = 0; k < 32u; k++) {
        o[k] = (uint8_t)(px[2u * k] << 4 | (px[2u * k + 1u] & 15u));
    }
}

static int is_dirty(const link_state* l, uint32_t i) {
    return l->dirty[i >> 3] >> (i & 7u) & 1u;
}

static void clean(link_state* l, uint32_t i) {
    l->dirty[i >> 3] &= (uint8_t)~(1u << (i & 7u));
}

static uint32_t cells(link_state* l, const grid* g, uint8_t* cmd, uint8_t* p) {
    uint32_t i = GRID_CELLS;
    for (uint32_t k = 0; k <= GRID_ROWS && l->rows; k++) {
        uint32_t r = l->scan / GRID_COLS, c = l->scan % GRID_COLS;
        if (l->rows >> r & 1u) {
            for (; c < GRID_COLS && !is_dirty(l, r * GRID_COLS + c); c++) {
            }
            if (c < GRID_COLS) {
                i = r * GRID_COLS + c;
                break;
            }
            if (l->scan % GRID_COLS == 0u) {
                l->rows &= ~(1u << r);
            }
        }
        l->scan = (uint16_t)(r + 1u == GRID_ROWS ? 0u : (r + 1u) * GRID_COLS);
    }
    if (i == GRID_CELLS) {
        return 0;
    }
    p[0] = (uint8_t)(i / GRID_COLS);
    p[1] = (uint8_t)(i % GRID_COLS);
    uint32_t n = 2;
    if (pixel_cell(g, i)) {
        *cmd = LINK_PIXELS;
        n += record(g, i, 1, p + n);
        pixels(g, i, p + n);
        n += 32u;
        clean(l, i);
        i++;
    } else {
        *cmd = LINK_CELLS;
        uint32_t end = i - i % GRID_COLS + GRID_COLS;
        uint8_t r[11];
        do {
            uint32_t k = record(g, i, 0, r);
            if (n + k > LINK_PAYLOAD_MAX) {
                break;
            }
            memcpy(p + n, r, k);
            n += k;
            clean(l, i);
            i++;
        } while (i < end && is_dirty(l, i) && !pixel_cell(g, i));
    }
    l->scan = (uint16_t)(i < GRID_CELLS ? i : 0u);
    return n;
}

static uint32_t frame(uint8_t* out, uint32_t cmd, const uint8_t* p, uint32_t n) {
    out[0] = 0xF0u;
    memcpy(out + 1, magic, sizeof magic);
    uint8_t* o = out + 6;
    *o++ = (uint8_t)cmd;
    uint32_t sum = cmd;
    for (uint32_t i = 0; i < n; i += 7u) {
        uint8_t* m = o++;
        uint32_t mask = 0;
        for (uint32_t k = 0; k < 7u && i + k < n; k++) {
            mask |= (uint32_t)(p[i + k] >> 7) << k;
            *o = p[i + k] & 0x7Fu;
            sum += *o++;
        }
        *m = (uint8_t)mask;
        sum += mask;
    }
    *o++ = (uint8_t)(-sum & 0x7Fu);
    *o++ = 0xF7u;
    return (uint32_t)(o - out);
}

uint32_t link_poll(link_state* l, const grid* g, uint32_t now_ms, uint32_t room, int busy,
                   uint8_t* out) {
    if (l->on && now_ms - l->heard >= LINK_TIMEOUT_MS) {
        link_drop(l);
        l->want = WANT_STOPPED;
    }
    if (l->held && now_ms - l->panel_at >= LINK_RELEASE_MS) {
        l->held = 0;
    }
    uint32_t dt = now_ms - l->last;
    int32_t credit = l->credit + (int32_t)(dt < LINK_BURST ? dt : LINK_BURST) * LINK_RATE;
    l->credit = credit > LINK_BURST ? LINK_BURST : credit;
    l->last = now_ms;
    if (busy || room < LINK_FRAME_PACKETS + LINK_RING_KEEP) {
        return 0;
    }
    uint8_t p[LINK_PAYLOAD_MAX];
    uint8_t cmd;
    uint32_t n = 0;
    uint32_t want = l->want;
    if (want) {
        uint32_t bit = want & -want;
        l->want = (uint8_t)(want & ~bit);
        if (bit == WANT_HELLO) {
            cmd = LINK_HELLO;
            n = hello(l, p);
        } else if (bit == WANT_STOPPED) {
            cmd = LINK_STOPPED;
        } else if (bit == WANT_BEAT) {
            cmd = LINK_BEAT;
        } else {
            cmd = LINK_COLOURS;
            for (uint32_t c = 0; c < 16u; c++) {
                p[2u * c] = (uint8_t)grid_palette[c];
                p[2u * c + 1u] = (uint8_t)(grid_palette[c] >> 8);
            }
            n = 32;
        }
    } else if (!l->on || l->credit <= 0) {
        return 0;
    } else if (l->font_next != NO_GLYPH) {
        cmd = LINK_GLYPHS;
        uint32_t c = l->font_next, k = FONT_LAST + 1u - c;
        k = k < GLYPHS_A_FRAME ? k : GLYPHS_A_FRAME;
        p[0] = (uint8_t)c;
        memcpy(p + 1, font_glyphs[c], k * 8u);
        n = 1u + k * 8u;
        l->font_next = (uint8_t)(c + k > FONT_LAST ? NO_GLYPH : c + k);
    } else {
        n = cells(l, g, &cmd, p);
        if (!n) {
            return 0;
        }
    }
    l->credit -= (int32_t)n;
    l->frames++;
    l->bytes += n;
    return frame(out, cmd, p, n);
}
