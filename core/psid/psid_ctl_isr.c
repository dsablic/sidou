// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "psid_ctl.h"

#define COMPILER_FENCE() __asm__ volatile("" ::: "memory")

#define PSID_ISR_LAST_REG 0x18u

_Static_assert(PSID_BLOCK == 128u, "psid_isr_ramp's shift");
static void psid_isr_ramp(int16_t* x, int down)
{
    for (uint32_t f = 0; f < PSID_BLOCK; f++) {
        int32_t k = (int32_t)(down ? PSID_BLOCK - 1u - f : f);
        x[f] = (int16_t)((x[f] * k) >> 7);
    }
}

static void psid_isr_chip(psid_ctl* c, uint32_t k, sid* s, int16_t* out, int tapped,
                          psid_render_fn render, uint32_t head, uint32_t window)
{
    const uint32_t cps = s->cycles_per_sample_q16;
    uint32_t now = c->clock[k];
    const uint32_t end = now + ((s->cycle_frac_q16 + PSID_BLOCK * cps) >> 16);
    uint32_t n = PSID_BLOCK, parts = 0, late = 0, i;
    if ((uint32_t)(head - c->cur[k]) > window)
        head = c->cur[k] + window;
    for (i = c->cur[k]; i != head; i++) {
        uint32_t j = i & (PSID_RING - 1u);
        uint32_t x = c->ring_cycle[j];
        if ((int32_t)(x - end) >= 0)
            break;
        uint32_t e = c->ring_entry[j];
        if (PSID_RING_CHIP(e) != k)
            continue;
        if (parts < PSID_SPLITS - 1u) {
            uint32_t q = s->cycle_frac_q16, t = now, m = 0;
            for (;;) {
                uint32_t step = (q + cps) >> 16;
                if ((int32_t)(x - (t + step)) < 0)
                    break;
                t += step;
                q = (q + cps) & 0xffffu;
                m++;
            }
            if (m) {
                render(s, out, m, tapped);
                out += m;
                n -= m;
                now = t;
                parts++;
            }
            int32_t d = (int32_t)(x - now);
            if (d > 0) {
                sid_advance(s, (uint32_t)d);
                now = x;
            } else if (d < 0) {
                late++;
            }
        } else {
            if (n) {
                render(s, out, n, tapped);
                n = 0;
                now = end;
                parts++;
            }
            late++;
        }
        sid_write(s, (uint8_t)PSID_RING_REG(e), (uint8_t)PSID_RING_VALUE(e));
    }
    if (n) {
        render(s, out, n, tapped);
        parts++;
    }
    c->cur[k] = i;
    c->clock[k] = end;
    c->late += late;
    if (parts > c->splits_max)
        c->splits_max = parts;
}

static void psid_isr_clear(sid* s)
{
    for (uint32_t r = 0; r <= PSID_ISR_LAST_REG; r++)
        sid_write(s, (uint8_t)r, 0);
}

uint32_t psid_ctl_block(psid_ctl* c, sid* a, sid* b, int16_t* sa, int16_t* sb, uint32_t tapped,
                        psid_render_fn render)
{
    const uint32_t two = b && c->chips == 2u;
    const uint32_t seq = c->want_seq;
    int fade_out = 0, stop = 0;
    uint32_t rendered = 0;
    if (seq != c->done_seq) {
        COMPILER_FENCE();
        switch (c->want) {
        case PSID_ISR_PLAY:
            if (c->mode == PSID_ISR_OFF) {
                c->cur[0] = c->cur[1] = c->tail;
                c->blocks = 0;
                c->fade_in = 0;
                c->mode = PSID_ISR_PLAY;
            } else if (c->mode == PSID_ISR_PAUSE) {
                c->fade_in = 1;
                c->mode = PSID_ISR_PLAY;
            }
            break;
        case PSID_ISR_PAUSE:
            fade_out = c->mode == PSID_ISR_PLAY;
            break;
        case PSID_ISR_STOP:
            fade_out = c->mode == PSID_ISR_PLAY;
            stop = 1;
            break;
        default:
            break;
        }
    }
    if (c->mode == PSID_ISR_PLAY) {
        uint32_t head = c->head;
        COMPILER_FENCE();
        const uint32_t window = two ? PSID_BLOCK_ENTRIES / 2u : PSID_BLOCK_ENTRIES;
        psid_isr_chip(c, 0, a, sa, (int)(tapped & 1u), render, head, window);
        rendered = 1;
        uint32_t t = c->cur[0];
        if (two) {
            psid_isr_chip(c, 1, b, sb, (int)(tapped >> 1 & 1u), render, head, window);
            rendered = 3;
            if ((int32_t)(c->cur[1] - t) < 0)
                t = c->cur[1];
        }
        COMPILER_FENCE();
        c->tail = t;
        c->blocks++;
        if (fade_out || c->fade_in) {
            psid_isr_ramp(sa, fade_out);
            if (two)
                psid_isr_ramp(sb, fade_out);
            c->fade_in = 0;
        }
        if (fade_out)
            c->mode = PSID_ISR_PAUSE;
    } else {
        for (uint32_t f = 0; f < PSID_BLOCK; f++)
            sa[f] = 0;
        if (two && sb)
            for (uint32_t f = 0; f < PSID_BLOCK; f++)
                sb[f] = 0;
    }
    if (stop) {
        if (c->mode != PSID_ISR_OFF) {
            psid_isr_clear(a);
            if (two)
                psid_isr_clear(b);
        }
        c->mode = PSID_ISR_OFF;
        c->tail = c->head;
    }
    if (seq != c->done_seq) {
        COMPILER_FENCE();
        c->done_seq = seq;
    }
    return rendered;
}
