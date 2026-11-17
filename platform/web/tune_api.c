// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stdint.h>
#include <string.h>

#include "psid/psid_env.h"
#include "psid/psid_file.h"
#include "sid/sid.h"
#include "sid/sid_shadow.h"

#define EXPORT(name) __attribute__((export_name(#name)))
#define FILE_MAX (PSID_HEADER_SIZE_V2 + 0x10000u + 2u)
#define WRITES_MAX 8192u
#define SHADOW_RATE 44100u
#define FOREVER (1u << 30)
#define MODEL_8580 2u

typedef struct {
    int32_t result;
    uint32_t version, load, init, play, songs, start, flags, speed, sid2, sid3, clock, model1,
        model2, sids;
    char name[PSID_STRING_MAX];
    char author[PSID_STRING_MAX];
    char released[PSID_STRING_MAX];
} tune_info_t;

typedef struct {
    uint32_t cycle;
    uint8_t chip, reg, value, pad;
} tune_write_t;

static uint8_t file[FILE_MAX];
static uint32_t file_len;
static uint8_t mem[0x10000];
static psid_header header;
static psid_env env;
static sid chips[2];
static sid_shadow shadows[2];
static tune_info_t info;
static tune_write_t writes[WRITES_MAX];
static uint32_t write_count;
static uint32_t dropped;

EXPORT(tune_in) uint8_t* tune_in(void) {
    return file;
}
EXPORT(tune_in_max) uint32_t tune_in_max(void) {
    return FILE_MAX;
}
EXPORT(tune_info) tune_info_t* tune_info(void) {
    return &info;
}
EXPORT(tune_writes) tune_write_t* tune_writes(void) {
    return writes;
}
EXPORT(tune_write_count) uint32_t tune_write_count(void) {
    return write_count;
}
EXPORT(tune_dropped) uint32_t tune_dropped(void) {
    return dropped;
}
EXPORT(tune_call_start) uint32_t tune_call_start(void) {
    return env.call_start;
}
EXPORT(tune_now) uint32_t tune_now(void) {
    return env.cpu.cycles;
}
EXPORT(tune_clock) uint32_t tune_clock(void) {
    return env.clock;
}
EXPORT(tune_next_due) uint32_t tune_next_due(void) {
    return env.next_due;
}

static void sink(void* ctx, uint32_t cycle, uint32_t chip, uint32_t reg, uint32_t value) {
    (void)ctx;
    if (write_count == WRITES_MAX) {
        dropped++;
        return;
    }
    writes[write_count++] = (tune_write_t){cycle, (uint8_t)chip, (uint8_t)reg, (uint8_t)value, 0};
}

static void keep_info(int32_t result) {
    memset(&info, 0, sizeof info);
    info.result = result;
    if (result != PSID_OK) {
        return;
    }
    info.version = header.version;
    info.load = header.load;
    info.init = header.init;
    info.play = header.play;
    info.songs = header.songs;
    info.start = header.start;
    info.flags = header.flags;
    info.speed = header.speed;
    info.sid2 = header.sid2;
    info.sid3 = header.sid3;
    info.clock = header.clock;
    info.model1 = header.model1;
    info.model2 = header.model2;
    info.sids = header.sids;
    memcpy(info.name, header.name, sizeof info.name);
    memcpy(info.author, header.author, sizeof info.author);
    memcpy(info.released, header.released, sizeof info.released);
}

EXPORT(tune_load) int32_t tune_load(uint32_t len) {
    file_len = len > FILE_MAX ? FILE_MAX : len;
    memset(&header, 0, sizeof header);
    int32_t r = psid_parse(file, file_len, &header);
    keep_info(r);
    return r;
}

static sid_model model(uint32_t code) {
    return code == MODEL_8580 ? SID_MODEL_8580 : SID_MODEL_6581;
}

static int32_t run(uint32_t cycles) {
    write_count = 0;
    dropped = 0;
    return psid_env_run(&env, cycles);
}

EXPORT(tune_start) int32_t tune_start(uint32_t song) {
    if (info.result != PSID_OK) {
        return -1;
    }
    psid_env_load(&env, mem, &header, file);
    psid_env_set_sink(&env, sink, NULL);
    sid_init_begin(&chips[0], model(header.model1), env.clock, SHADOW_RATE);
    sid_init_begin(&chips[1], model(header.model2), env.clock, SHADOW_RATE);
    psid_env_set_shadows(&env, &shadows[0], header.sid2 ? &shadows[1] : NULL);
    if (psid_env_start(&env, song) != 0) {
        return -1;
    }
    sid_shadow_init(&shadows[0], &chips[0], 0);
    sid_shadow_init(&shadows[1], &chips[1], 0);
    return run(FOREVER);
}

EXPORT(tune_call) int32_t tune_call(void) {
    int32_t begun = run(env.next_due - env.cpu.cycles);
    return begun == PSID_RUN_MORE ? psid_env_run(&env, FOREVER) : begun;
}
