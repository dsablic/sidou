/* psid-ref: begin */
/* SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 *
 * psid_oracle.c: a test-only derived copy of FM-1 SID's player/psid.c (fm1-emu 7ca5b96;
 * third_party/fm1-sid-player/psid.c, BSD-2-Clause), for the PSID oracle (host/crates/psid-ref;
 * M3 Task 7). SIDOU added only the lines between the psid-ref begin and end markers: this
 * notice, a log of the I/O page's reads (each read's cycle in the call, address and value, as
 * bus_read returned it) and a byte of memory read back (for $01 at a call's start). The test
 * oracle.rs checks that the file without those blocks is the vendored original, byte for byte.
 *
 * Copyright (c) 2026 Joakim Eriksson
 *
 * Redistribution and use in source and binary forms, with or without modification, are
 * permitted provided that the following conditions are met:
 *  1. Redistributions of source code must retain the above copyright notice, this list of
 *     conditions and the following disclaimer.
 *  2. Redistributions in binary form must reproduce the above copyright notice, this list
 *     of conditions and the following disclaimer in the documentation and/or other
 *     materials provided with the distribution.
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY
 * EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL
 * THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
 * SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT
 * OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
 * OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE. */
/* psid-ref: end */
/* player/psid.c - see psid.h
 * SPDX-License-Identifier: BSD-2-Clause */
#include <string.h>
#include "psid.h"

#define CHIPS_IMPL
#define CHIPS_ASSERT(c) ((void)0)
#ifndef assert
#define assert(c) ((void)0)              /* m6502.h marks unreachable decoder slots with it */
#endif
#include "m6502.h"

/* psid-ref: begin */
/* an I/O-page read: cycle = cycles since the start of this call, as a write's */
typedef void (*psid_read_fn)(void *user, uint32_t cycle, uint32_t addr, uint32_t val);
/* psid-ref: end */
struct psid {
    m6502_t cpu;
    uint64_t pins;
    psid_info_t info;
    psid_write_fn wr;
    void *user;
    uint32_t cycle;                       /* cycles into the current call */
    uint32_t song;
    uint16_t cia_latch;
    uint32_t rnd;
    uint8_t mem[65536];
/* psid-ref: begin */
    psid_read_fn rd;                      /* the I/O page's reads, logged (psid_set_read_log) */
    void *rd_user;
/* psid-ref: end */
};

uint32_t psid_size(void) { return (uint32_t)sizeof(psid_t); }
const psid_info_t *psid_info(const psid_t *p) { return &p->info; }

static int io_on(const psid_t *p)
{
    uint8_t b = p->mem[1] & 7;
    return (b & 3) && (b & 4);
}

/* psid-ref: begin */
#define bus_read bus_read_unlogged
/* psid-ref: end */
static uint8_t bus_read(psid_t *p, uint16_t ad)
{
    if (ad >= 0xD000 && ad < 0xE000 && io_on(p)) {
        if (ad >= 0xD400 && ad < 0xD800) {
            if ((ad & 0x1f) == 0x1b) {                 /* OSC3: a running random byte */
                p->rnd = p->rnd * 1103515245u + 12345u;
                return (uint8_t)(p->rnd >> 16);
            }
            return 0;
        }
        if (ad == 0xD012)
            return (uint8_t)((p->cycle / 63) % 312);   /* raster line, for busy-waits */
        if (ad == 0xD011)
            return (uint8_t)((((p->cycle / 63) % 312) >> 1) & 0x80);
        if (ad == 0xDC04)
            return (uint8_t)p->cia_latch;
        if (ad == 0xDC05)
            return (uint8_t)(p->cia_latch >> 8);
        return 0;
    }
    return p->mem[ad];
}

/* psid-ref: begin */
#undef bus_read
/* the original bus_read, each read of the I/O page (while $01 maps it in) reported */
static uint8_t bus_read(psid_t *p, uint16_t ad)
{
    uint8_t v = bus_read_unlogged(p, ad);
    if (ad >= 0xD000 && ad < 0xE000 && io_on(p) && p->rd)
        p->rd(p->rd_user, p->cycle, ad, v);
    return v;
}

/* after psid_load, which clears it */
void psid_set_read_log(psid_t *p, psid_read_fn rd, void *user)
{
    p->rd = rd;
    p->rd_user = user;
}

uint8_t psid_peek(const psid_t *p, uint16_t ad) { return p->mem[ad]; }
/* psid-ref: end */
static void bus_write(psid_t *p, uint16_t ad, uint8_t v)
{
    if (ad >= 0xD000 && ad < 0xE000 && io_on(p)) {
        if (ad >= 0xD400 && ad < 0xD800)
            p->wr(p->user, p->cycle, ad & 0x1fu, v);
        else if (ad == 0xDC04)
            p->cia_latch = (uint16_t)((p->cia_latch & 0xff00) | v);
        else if (ad == 0xDC05)
            p->cia_latch = (uint16_t)((p->cia_latch & 0x00ff) | v << 8);
        return;
    }
    p->mem[ad] = v;
}

/* run from ad until the routine returns to the sentinel at $0000 / $0001 (RTI / RTS) or into
 * a KERNAL interrupt exit (no ROM here), at most max cycles */
static uint32_t call(psid_t *p, uint16_t ad, uint8_t a, int irq, uint32_t max)
{
    uint64_t pins;
    uint8_t s = 0xff;
    p->mem[0x100 + s--] = 0x00;           /* return address $0000: RTS lands on $0001 */
    p->mem[0x100 + s--] = 0x00;
    if (irq)
        p->mem[0x100 + s--] = m6502_p(&p->cpu) | 0x20;
    m6502_set_s(&p->cpu, s);
    m6502_set_a(&p->cpu, a);
    m6502_set_x(&p->cpu, 0);
    m6502_set_y(&p->cpu, 0);
    pins = M6502_SYNC;                    /* the "prefetch" that starts execution at ad */
    M6502_SET_ADDR(pins, ad);
    M6502_SET_DATA(pins, p->mem[ad]);
    m6502_set_pc(&p->cpu, ad);
    p->cycle = 0;
    while (p->cycle < max) {
        uint16_t addr;
        pins = m6502_tick(&p->cpu, pins);
        p->cycle++;
        addr = M6502_GET_ADDR(pins);
        if ((pins & M6502_SYNC) && (addr <= 1 || addr == 0xEA31 || addr == 0xEA81 || addr == 0xFEBC))
            break;
        if (pins & M6502_RW) {
            M6502_SET_DATA(pins, bus_read(p, addr));
        } else {
            bus_write(p, addr, M6502_GET_DATA(pins));
        }
    }
    p->pins = pins;
    return p->cycle;
}

static uint16_t be16(const uint8_t *b) { return (uint16_t)(b[0] << 8 | b[1]); }

static void str32(char *d, const uint8_t *s)
{
    memcpy(d, s, 32);
    d[32] = 0;
}

int psid_load(psid_t *p, const uint8_t *b, uint32_t n, psid_write_fn wr, void *user)
{
    uint16_t ver, off, flags = 0;
    uint32_t len;
    m6502_desc_t desc;
    if (n < 0x76 || memcmp(b, "PSID", 4))
        return -1;
    memset(p, 0, sizeof *p);
    memset(&desc, 0, sizeof desc);
    m6502_init(&p->cpu, &desc);
    p->wr = wr;
    p->user = user;
    p->cia_latch = 0x4025;
    p->rnd = 0x1234567u;
    ver = be16(b + 4);
    off = be16(b + 6);
    p->info.load = be16(b + 8);
    p->info.init = be16(b + 10);
    p->info.play = be16(b + 12);
    p->info.songs = be16(b + 14);
    p->info.start = be16(b + 16);
    p->info.speed = (uint32_t)be16(b + 18) << 16 | be16(b + 20);
    str32(p->info.name, b + 0x16);
    str32(p->info.author, b + 0x36);
    str32(p->info.released, b + 0x56);
    if (ver >= 2 && n >= 0x7c)
        flags = be16(b + 0x76);
    p->info.model = ((flags >> 4) & 3) == 2 ? 1 : 0;
    if (!p->info.load) {
        p->info.load = (uint16_t)(b[off] | b[off + 1] << 8);
        off += 2;
    }
    if (!p->info.init)
        p->info.init = p->info.load;
    len = n - off;
    if (len > 65536u - p->info.load)
        len = 65536u - p->info.load;
    memcpy(p->mem + p->info.load, b + off, len);
    p->mem[1] = 0x37;
    return 0;
}

uint32_t psid_init(psid_t *p, uint32_t song)
{
    if (!song)
        song = p->info.start ? p->info.start : 1;
    p->song = song;
    m6502_set_p(&p->cpu, 0x24);           /* I set */
    return call(p, p->info.init, (uint8_t)(song - 1), 0, 5u * PSID_PAL_CLOCK);
}

uint32_t psid_period(const psid_t *p)
{
    if (p->info.speed >> ((p->song - 1) & 31) & 1) {
        uint32_t t = (uint32_t)p->cia_latch + 1u;
        return t < 1000 ? PSID_PAL_FRAME : t;
    }
    return PSID_PAL_FRAME;
}

uint32_t psid_play(psid_t *p)
{
    uint16_t ad = p->info.play;
    int irq = 0;
    if (!ad) {                            /* the tune installed its own interrupt handler */
        ad = (p->mem[1] & 2) ? (uint16_t)(p->mem[0x314] | p->mem[0x315] << 8)
                             : (uint16_t)(p->mem[0xfffe] | p->mem[0xffff] << 8);
        irq = 1;
    }
    return call(p, ad, 0, irq, 2u * PSID_PAL_FRAME);
}
