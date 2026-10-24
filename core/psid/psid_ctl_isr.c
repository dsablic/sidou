// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The .sid player's interrupt half (psid_ctl.h): a block of each chip, the ring's writes placed
 * at their cycles. On the device every function here runs from SRAM (platform/fm1/app.ld, Task
 * 9: .text.psid_ctl_block and the psid_isr_* helpers, if the compiler keeps them out of line),
 * built -O2 without jump tables (core/CMakeLists.txt); it calls only sid.c's RAMTEXT functions
 * and the render pointer. */
#include "psid_ctl.h"

#define COMPILER_FENCE() __asm__ volatile("" ::: "memory")

/* The last SID register a stop clears: $18, the mode and volume. */
#define PSID_ISR_LAST_REG 0x18u

/* audio_ramp's ramp (platform/fm1/audio.c) on one chip's samples: down from full to 0 (down 1)
 * or up from 0 (down 0). */
_Static_assert(PSID_BLOCK == 128u, "psid_isr_ramp's shift");
static void psid_isr_ramp(int16_t* x, int down)
{
    for (uint32_t f = 0; f < PSID_BLOCK; f++) {
        int32_t k = (int32_t)(down ? PSID_BLOCK - 1u - f : f);
        x[f] = (int16_t)((x[f] * k) >> 7);
    }
}

/* Chip k's block into out: its writes from its cursor on, each at its cycle, while the block's
 * renders last (PSID_SPLITS), then late at the block's end. The ring is in cycle order (one
 * 6502), so the first entry of either chip at or past the block's last cycle ends the scan.
 * The block's cycles are the samples' Q16 steps summed, which no sid_advance changes (it moves
 * cycles from the next sample to before it), so they are known at the start. */
static void psid_isr_chip(psid_ctl* c, uint32_t k, sid* s, int16_t* out, int tapped,
                          psid_render_fn render, uint32_t head)
{
    const uint32_t cps = s->cycles_per_sample_q16;
    uint32_t now = c->clock[k];
    const uint32_t end = now + ((s->cycle_frac_q16 + PSID_BLOCK * cps) >> 16);
    uint32_t n = PSID_BLOCK, parts = 0, late = 0, i;
    for (i = c->cur[k]; i != head; i++) {
        uint32_t j = i & (PSID_RING - 1u);
        uint32_t x = c->ring_cycle[j];
        if ((int32_t)(x - end) >= 0)
            break; /* this entry and every later one: a later block's */
        uint32_t e = c->ring_entry[j];
        if (PSID_RING_CHIP(e) != k)
            continue; /* the other chip's, which its own cursor takes */
        if (parts < PSID_SPLITS - 1u) {
            /* The whole samples that end at or before x (x is before the block's end: fewer
             * than n), stepped as the render steps them. */
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
                sid_advance(s, (uint32_t)d); /* d is under the next sample's cycles */
                now = x;
            } else if (d < 0) {
                late++; /* the chip has passed it: the 6502 fell behind */
            }
        } else {
            if (n) { /* the split budget is spent: the rest of the block in one render */
                render(s, out, n, tapped);
                n = 0;
                now = end;
                parts++;
            }
            late++; /* at the block's end */
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
        COMPILER_FENCE(); /* the number before the request it publishes */
        switch (c->want) {
        case PSID_ISR_PLAY:
            if (c->mode == PSID_ISR_OFF) { /* a song's start: the ring and clocks are from 0 */
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
        COMPILER_FENCE(); /* the head before the entries it publishes */
        psid_isr_chip(c, 0, a, sa, (int)(tapped & 1u), render, head);
        rendered = 1;
        uint32_t t = c->cur[0];
        if (two) {
            psid_isr_chip(c, 1, b, sb, (int)(tapped >> 1 & 1u), render, head);
            rendered = 3;
            if ((int32_t)(c->cur[1] - t) < 0)
                t = c->cur[1];
        }
        COMPILER_FENCE(); /* the entries are read before their slots are handed back */
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
        if (c->mode != PSID_ISR_OFF) { /* the tune has written the chips: clear them */
            psid_isr_clear(a);
            if (two)
                psid_isr_clear(b);
        }
        c->mode = PSID_ISR_OFF;
        c->tail = c->head;
    }
    if (seq != c->done_seq) {
        COMPILER_FENCE(); /* the request carried out before the number that says so */
        c->done_seq = seq;
    }
    return rendered;
}
