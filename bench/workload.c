// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "workload.h"

#include "sid/sid.h"

#define SAMPLES_PER_FRAME (WORKLOAD_SAMPLE_RATE / 50u)
#define BLOCK 128u

typedef struct {
    uint16_t frame;
    uint8_t reg;
    uint8_t value;
} reg_write;

/* C major triad (PAL frequencies), every voice through a resonant low-pass;
 * the cutoff opens at frame 3, voice 3 turns to noise at frame 6, all gates
 * close at frame 9. */
static const reg_write WRITES[] = {
    {0, 0x18, 0x1f}, {0, 0x17, 0x87}, {0, 0x15, 0x00}, {0, 0x16, 0x40},
    {0, 0x00, 0x67}, {0, 0x01, 0x11}, {0, 0x05, 0x09}, {0, 0x06, 0xa9}, {0, 0x04, 0x21},
    {0, 0x07, 0xed}, {0, 0x08, 0x15}, {0, 0x09, 0x00}, {0, 0x0a, 0x08}, {0, 0x0c, 0x09},
    {0, 0x0d, 0xa9}, {0, 0x0b, 0x41},
    {0, 0x0e, 0x13}, {0, 0x0f, 0x1a}, {0, 0x13, 0x09}, {0, 0x14, 0xa9}, {0, 0x12, 0x11},
    {3, 0x16, 0x80}, {6, 0x12, 0x81},
    {9, 0x04, 0x20}, {9, 0x0b, 0x40}, {9, 0x12, 0x80},
};

static sid chips[WORKLOAD_MAX_CHIPS];
static uint32_t chip_count;
static uint32_t position;
static uint32_t next_write;

void workload_begin(uint32_t count, sid_model model) {
    chip_count = count > WORKLOAD_MAX_CHIPS ? WORKLOAD_MAX_CHIPS : count;
    position = 0;
    next_write = 0;
    for (uint32_t c = 0; c < chip_count; ++c) {
        sid_init(&chips[c], model, SID_CLOCK_PAL, WORKLOAD_SAMPLE_RATE);
    }
}

void workload_render_samples(int16_t* out, uint32_t n) {
    int16_t mix[BLOCK];
    while (n) {
        uint32_t frame = position / SAMPLES_PER_FRAME;
        while (next_write < sizeof WRITES / sizeof WRITES[0] && WRITES[next_write].frame <= frame) {
            for (uint32_t c = 0; c < chip_count; ++c) {
                sid_write(&chips[c], WRITES[next_write].reg, WRITES[next_write].value);
            }
            ++next_write;
        }
        uint32_t len = SAMPLES_PER_FRAME - position % SAMPLES_PER_FRAME;
        if (len > BLOCK) {
            len = BLOCK;
        }
        if (len > n) {
            len = n;
        }
        sid_render(&chips[0], out, len);
        for (uint32_t c = 1; c < chip_count; ++c) {
            sid_render(&chips[c], mix, len);
            for (uint32_t i = 0; i < len; ++i) {
                out[i] = (int16_t)(((int32_t)out[i] + mix[i]) / 2);
            }
        }
        out += len;
        n -= len;
        position += len;
    }
}

uint32_t workload_sample_count(void) { return WORKLOAD_SAMPLES; }
