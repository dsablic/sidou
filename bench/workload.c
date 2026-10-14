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
static const reg_write TRIAD[] = {
    {0, 0x18, 0x1f}, {0, 0x17, 0x87}, {0, 0x15, 0x00}, {0, 0x16, 0x40},
    {0, 0x00, 0x67}, {0, 0x01, 0x11}, {0, 0x05, 0x09}, {0, 0x06, 0xa9}, {0, 0x04, 0x21},
    {0, 0x07, 0xed}, {0, 0x08, 0x15}, {0, 0x09, 0x00}, {0, 0x0a, 0x08}, {0, 0x0c, 0x09},
    {0, 0x0d, 0xa9}, {0, 0x0b, 0x41},
    {0, 0x0e, 0x13}, {0, 0x0f, 0x1a}, {0, 0x13, 0x09}, {0, 0x14, 0xa9}, {0, 0x12, 0x11},
    {3, 0x16, 0x80}, {6, 0x12, 0x81},
    {9, 0x04, 0x20}, {9, 0x0b, 0x40}, {9, 0x12, 0x80},
};

/* The envelope worst cases are the fastest rate (a match every 9 cycles, 2.5 per
 * sample) on all three voices. The triad's notes, unfiltered: */
#define NOTES(ad, sr, ctrl)                                                                 \
    {0, 0x00, 0x67}, {0, 0x01, 0x11}, {0, 0x05, ad}, {0, 0x06, sr}, {0, 0x04, ctrl},        \
        {0, 0x07, 0xed}, {0, 0x08, 0x15}, {0, 0x0c, ad}, {0, 0x0d, sr}, {0, 0x0b, ctrl},    \
        {0, 0x0e, 0x13}, {0, 0x0f, 0x1a}, {0, 0x13, ad}, {0, 0x14, sr}, {0, 0x12, ctrl}

/* Attack and decay rate 0, holding sustain 0xff. */
static const reg_write SUSTAIN_FF[] = {{0, 0x18, 0x0f}, NOTES(0x00, 0xf0, 0x21)};

/* The same, holding sustain 0xaa, so decay runs through the divider first. */
static const reg_write SUSTAIN_AA[] = {{0, 0x18, 0x0f}, NOTES(0x00, 0xa0, 0x21)};

/* ADSR 0000, every gate toggled every frame: attack, decay to 0 and release at rate 0
 * over and over. */
#define GATES(f, ctrl) {f, 0x04, ctrl}, {f, 0x0b, ctrl}, {f, 0x12, ctrl}
static const reg_write GATE_EVERY_FRAME[] = {
    {0, 0x18, 0x0f},   NOTES(0x00, 0x00, 0x20), GATES(0, 0x21),  GATES(1, 0x20),
    GATES(2, 0x21),    GATES(3, 0x20),          GATES(4, 0x21),  GATES(5, 0x20),
    GATES(6, 0x21),    GATES(7, 0x20),          GATES(8, 0x21),  GATES(9, 0x20),
    GATES(10, 0x21),   GATES(11, 0x20),         GATES(12, 0x21),
};

/* Nothing but the volume: the power-on envelopes release to 0 at rate 0 and stay
 * frozen there. */
static const reg_write IDLE[] = {{0, 0x18, 0x0f}};

/* ADSR 0000 as in GATE_EVERY_FRAME, but the gates toggle at the start of every block
 * (128 samples, about 2.9 ms; see gate_every_block below), so the envelopes never
 * settle: attack and release at rate 0, over and over. */
static const reg_write GATE_EVERY_BLOCK[] = {{0, 0x18, 0x0f}, NOTES(0x00, 0x00, 0x20)};

typedef struct {
    const char* name;
    const reg_write* writes;
    uint32_t count;
    /* Called with each chip and the block number before every block, or NULL. */
    void (*per_block)(sid* s, uint32_t block);
} scenario;

static void gate_every_block(sid* s, uint32_t block) {
    uint8_t control = block & 1u ? 0x20 : 0x21;
    sid_write(s, 0x04, control);
    sid_write(s, 0x0b, control);
    sid_write(s, 0x12, control);
}

#define SCENARIO(name, w) {name, w, sizeof w / sizeof w[0], 0}
static const scenario SCENARIOS[WORKLOAD_SCENARIOS] = {
    SCENARIO("triad", TRIAD),
    SCENARIO("sustain_ff", SUSTAIN_FF),
    SCENARIO("sustain_aa", SUSTAIN_AA),
    SCENARIO("gate_every_frame", GATE_EVERY_FRAME),
    SCENARIO("idle", IDLE),
    {"gate_every_block", GATE_EVERY_BLOCK, sizeof GATE_EVERY_BLOCK / sizeof GATE_EVERY_BLOCK[0],
     gate_every_block},
};

static sid chips[WORKLOAD_MAX_CHIPS];
static uint32_t chip_count;
static uint32_t position;
static uint32_t next_write;
static uint32_t blocks;
static const scenario* current = &SCENARIOS[WORKLOAD_TRIAD];

void workload_begin(uint32_t count, sid_model model) {
    workload_begin_scenario(count, model, WORKLOAD_TRIAD);
}

void workload_begin_scenario(uint32_t count, sid_model model, uint32_t which) {
    current = &SCENARIOS[which < WORKLOAD_SCENARIOS ? which : WORKLOAD_TRIAD];
    chip_count = count < 1u ? 1u : count > WORKLOAD_MAX_CHIPS ? WORKLOAD_MAX_CHIPS : count;
    position = 0;
    blocks = 0;
    next_write = 0;
    for (uint32_t c = 0; c < chip_count; ++c) {
        sid_init(&chips[c], model, SID_CLOCK_PAL, WORKLOAD_SAMPLE_RATE);
    }
}

void workload_render_samples(int16_t* out, uint32_t n) {
    int16_t mix[BLOCK];
    while (n) {
        uint32_t frame = position / SAMPLES_PER_FRAME;
        while (next_write < current->count && current->writes[next_write].frame <= frame) {
            const reg_write* w = &current->writes[next_write];
            for (uint32_t c = 0; c < chip_count; ++c) {
                uint8_t value = w->value;
                if (c > 0 && w->reg == 0x01) {
                    value = (uint8_t)(value + 0x04u);
                }
                sid_write(&chips[c], w->reg, value);
            }
            ++next_write;
        }
        if (current->per_block) {
            for (uint32_t c = 0; c < chip_count; ++c) {
                current->per_block(&chips[c], blocks);
            }
        }
        ++blocks;
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

const char* workload_scenario_name(uint32_t which) {
    return which < WORKLOAD_SCENARIOS ? SCENARIOS[which].name : 0;
}
