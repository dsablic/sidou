// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_OSC_H
#define SIDOU_SID_OSC_H

#include <stdint.h>

#define SID_CTRL_GATE 0x01u
#define SID_CTRL_SYNC 0x02u
#define SID_CTRL_RING 0x04u
#define SID_CTRL_TEST 0x08u
#define SID_CTRL_TRI 0x10u
#define SID_CTRL_SAW 0x20u
#define SID_CTRL_PULSE 0x40u
#define SID_CTRL_NOISE 0x80u

#define SID_LFSR_RESET 0x7ffff8u

typedef struct {
    uint32_t acc;
    uint32_t freq;
    uint32_t pw;
    uint32_t lfsr;
    uint32_t overshoot; /* accumulator units past the last MSB rise, for hard sync */
    uint8_t control;
    uint8_t msb_rose;
} sid_osc;

static inline void sid_osc_reset(sid_osc* o) {
    o->acc = 0;
    o->freq = 0;
    o->pw = 0;
    o->lfsr = SID_LFSR_RESET;
    o->overshoot = 0;
    o->control = 0;
    o->msb_rose = 0;
}

static inline void sid_lfsr_clock(sid_osc* o) {
    uint32_t bit = ((o->lfsr >> 22) ^ (o->lfsr >> 17)) & 1u;
    o->lfsr = ((o->lfsr << 1) | bit) & 0x7fffffu;
}

static inline uint32_t sid_noise_output(uint32_t r) {
    return ((r & 0x400000u) >> 11) | ((r & 0x100000u) >> 10) | ((r & 0x010000u) >> 7) |
           ((r & 0x002000u) >> 5) | ((r & 0x000800u) >> 4) | ((r & 0x000080u) >> 1) |
           ((r & 0x000010u) << 1) | ((r & 0x000004u) << 2);
}

static inline void sid_osc_write_control(sid_osc* o, uint8_t v) {
    if ((v & SID_CTRL_TEST) && !(o->control & SID_CTRL_TEST)) {
        o->acc = 0;
        o->lfsr = SID_LFSR_RESET;
    }
    o->control = v;
}

/* cycles is at most ~23, so freq * cycles < 2^24 and at most one MSB rise can occur. */
static inline void sid_osc_advance(sid_osc* o, uint32_t cycles) {
    o->msb_rose = 0;
    if (o->control & SID_CTRL_TEST) {
        return;
    }
    uint32_t prev = o->acc;
    uint32_t next = prev + o->freq * cycles;
    uint32_t noise_clocks = ((next + 0x80000u) >> 20) - ((prev + 0x80000u) >> 20);
    while (noise_clocks--) {
        sid_lfsr_clock(o);
    }
    if (((next + 0x800000u) >> 24) != ((prev + 0x800000u) >> 24)) {
        o->msb_rose = 1;
        o->overshoot = (next - 0x800000u) & 0xffffffu;
    }
    o->acc = next & 0xffffffu;
}

/* Hard sync: restart o as if it had been reset at the moment src's MSB rose. */
static inline void sid_osc_sync(sid_osc* o, const sid_osc* src) {
    uint32_t cycles_after = src->overshoot / src->freq;
    o->acc = (o->freq * cycles_after) & 0xffffffu;
}

static inline uint32_t sid_osc_output(const sid_osc* o, const sid_osc* src) {
    uint32_t c = o->control;
    if (!(c & 0xf0u)) {
        return 0;
    }
    uint32_t out = 0xfffu;
    if (c & SID_CTRL_TRI) {
        uint32_t msb = o->acc & 0x800000u;
        if (c & SID_CTRL_RING) {
            msb ^= src->acc & 0x800000u;
        }
        out &= ((msb ? ~o->acc : o->acc) >> 11) & 0xfffu;
    }
    if (c & SID_CTRL_SAW) {
        out &= o->acc >> 12;
    }
    if (c & SID_CTRL_PULSE) {
        out &= ((c & SID_CTRL_TEST) || (o->acc >> 12) >= o->pw) ? 0xfffu : 0u;
    }
    if (c & SID_CTRL_NOISE) {
        out &= sid_noise_output(o->lfsr);
    }
    return out;
}

#endif
