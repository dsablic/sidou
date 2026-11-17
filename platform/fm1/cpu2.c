// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "cpu2.h"

#include <string.h>

#include "far_call.h"
#include "fm1_time.h"
#include "sid/sid_osc.h"

#define CPU2_METER_BLOCKS 345u
#define CPU2_BLOCK_TICKS_X10 \
    ((uint32_t)((uint64_t)AUDIO_HALF_FRAMES * FM1_TICKS_PER_US * 10000000u / AUDIO_SAMPLE_RATE))
#define CPU2_TICKS_SHOWN_MAX 400000u
#define CPU2_HEX_DIGITS 8u

typedef struct {
    sid* chip;
    int16_t* out;
    scope* tap;
} cpu2_block;

static cpu2_block chip_b_block __attribute__((aligned(32), section(".noinit")));
static uint8_t chip_b_on_core0;
static struct {
    uint32_t sum, blocks, window, worst, seq;
} meter __attribute__((section(".noinit")));

static const uint8_t SELF_TEST_PATCH[][2] = {
    {0x00, 0xd6}, {0x01, 0x1c}, {0x02, 0x00}, {0x03, 0x08}, {0x05, 0x09}, {0x06, 0xf5},
    {0x07, 0x00}, {0x08, 0x2a}, {0x0c, 0x00}, {0x0d, 0xa0},
    {0x0e, 0x6b}, {0x0f, 0x0e}, {0x13, 0x22}, {0x14, 0xc4},
    {0x15, 0x03}, {0x16, 0x60}, {0x17, 0xf7}, {0x18, 0x3f},
    {0x04, 0x41}, {0x0b, 0x81}, {0x12, 0x15},
};

static void cpu2_render(sid* chip, int16_t* out, scope* tap) {
    if (tap) {
        scope_render(tap, chip, out, AUDIO_HALF_FRAMES);
    } else {
        sid_render(chip, out, AUDIO_HALF_FRAMES);
    }
}

static void cpu2_render_job(uint32_t arg) {
    const cpu2_block* b = (const cpu2_block*)(uintptr_t)arg;
    cpu2_render(b->chip, b->out, b->tap);
}

void cpu2_meter_clear(void) {
    meter = (__typeof__(meter)){0};
}

void cpu2_meter_block(uint32_t ticks) {
    meter.sum += ticks;
    if (ticks > meter.worst) {
        meter.worst = ticks;
    }
    if (++meter.blocks == CPU2_METER_BLOCKS) {
        meter.window = meter.sum;
        meter.sum = 0;
        meter.blocks = 0;
        meter.seq++;
    }
}

int cpu2_chip_b_away(sid* b, int16_t* out, scope* tap) {
    if (chip_b_on_core0) {
        return 0;
    }
    chip_b_block.chip = b;
    chip_b_block.out = out;
    chip_b_block.tap = tap;
    return cpu2_core1_run(cpu2_render_job, (uint32_t)(uintptr_t)&chip_b_block);
}

void cpu2_chip_b_back(int away, sid* b, int16_t* out, scope* tap) {
    if (!away || cpu2_core1_wait() != 0) {
        cpu2_render(b, out, tap);
    }
}

static void self_test_patch(sid* s) {
    sid_init(s, SID_MODEL_6581, SID_CLOCK_PAL, AUDIO_SAMPLE_RATE);
    for (uint32_t i = 0; i < sizeof SELF_TEST_PATCH / sizeof SELF_TEST_PATCH[0]; i++) {
        SIDOU_FAR(sid_write)(s, SELF_TEST_PATCH[i][0], SELF_TEST_PATCH[i][1]);
    }
}

static int self_test_blocks(sid pair[2]) {
    int16_t mine[AUDIO_HALF_FRAMES], theirs[AUDIO_HALF_FRAMES];
    cpu2_block job = {&pair[1], theirs, 0};
    for (uint32_t k = 0; k < CPU2_SELF_TEST_BLOCKS; k++) {
        if (!SIDOU_FAR(cpu2_core1_run)(cpu2_render_job, (uint32_t)(uintptr_t)&job)) {
            return 0;
        }
        SIDOU_FAR(sid_render)(&pair[0], mine, AUDIO_HALF_FRAMES);
        if (SIDOU_FAR(cpu2_core1_wait)() != 0) {
            return 0;
        }
        if (memcmp(mine, theirs, sizeof mine) != 0) {
            return -1;
        }
    }
    return 1;
}

void cpu2_self_test(sid pair[2]) {
    sid_noise_tables_init();
    self_test_patch(&pair[0]);
    self_test_patch(&pair[1]);
    if (self_test_blocks(pair) < 0) {
        SIDOU_FAR(cpu2_core1_retire)(CPU2_GAVE_UP_SELF_TEST);
    }
    memset(pair, 0, 2u * sizeof(sid));
}

static uint16_t tenths_of_a_block(uint32_t ticks) {
    uint32_t shown = ticks < CPU2_TICKS_SHOWN_MAX ? ticks : CPU2_TICKS_SHOWN_MAX;
    return (uint16_t)(shown * 10000u / CPU2_BLOCK_TICKS_X10);
}

static char* put_text(char* at, const char* s) {
    while (*s) {
        *at++ = *s++;
    }
    *at = 0;
    return at;
}

static char* put_hex(char* at, uint32_t v) {
    for (uint32_t i = CPU2_HEX_DIGITS; i-- > 0; v >>= 4) {
        at[i] = "0123456789ABCDEF"[v & 15u];
    }
    at[CPU2_HEX_DIGITS] = 0;
    return at + CPU2_HEX_DIGITS;
}

static void fault_lines(char detail[][VIEWS_CORES_DETAIL], const cpu2_core1* c) {
    put_hex(put_text(put_hex(put_text(detail[1], "PC "), c->fault_pc), " DBG "), c->fault_dbg);
    put_hex(put_text(put_text(put_text(detail[2], "CORE "), c->fault_cpu ? "1" : "0"), " EMU "),
            c->fault_emu);
}

static void cores_report(views_cores_report* out) {
    cpu2_core1 c;
    cpu2_core1_state(&c);
    memset(out, 0, sizeof *out);
    out->seq = meter.seq;
    out->jobs = c.jobs;
    out->audio_avg = tenths_of_a_block(meter.window / CPU2_METER_BLOCKS);
    out->audio_worst = tenths_of_a_block(meter.worst);
    out->running = c.on;
    out->chip_b_core = (uint8_t)(c.on && !chip_b_on_core0);
    put_text(out->why, c.why);
    put_hex(put_text(out->detail[0], "C1_CON AT BOOT "), c.con_at_boot);
    if (c.faulted) {
        fault_lines(out->detail, &c);
    }
}

static void cores_chip_b(uint32_t core) {
    chip_b_on_core0 = (uint8_t)(core == 0u);
    meter.worst = 0;
}

const views_cores cpu2_cores = {cores_report, cores_chip_b, cpu2_core1_unbar};
