// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stdint.h>
#include <string.h>

#include "patch_json.h"
#include "sid/sid_defs.h"
#include "song/gt2_sng.h"

#define EXPORT(name) __attribute__((export_name(#name)))
#define IN_MAX 65536u
#define OUT_MAX GT2_SAVED_MAX

static uint32_t region[SONG_REGION_BYTES / 4u];
static char in_buf[IN_MAX];
static uint8_t out_buf[OUT_MAX];
static uint32_t out_len;
static patch_json patch;
static patch_bank bank;
static uint32_t is_bank;

EXPORT(sp_in) char* sp_in(void) {
    return in_buf;
}
EXPORT(sp_in_max) uint32_t sp_in_max(void) {
    return IN_MAX;
}
EXPORT(sp_out) uint8_t* sp_out(void) {
    return out_buf;
}
EXPORT(sp_error) const char* sp_error(void) {
    return is_bank ? bank.error : patch.error;
}
EXPORT(sp_name) const char* sp_name(void) {
    return is_bank ? bank.name : patch.name;
}

static int write_out(void* ctx, const uint8_t* buf, uint32_t len) {
    (void)ctx;
    if (len > OUT_MAX - out_len) {
        return -1;
    }
    memcpy(out_buf + out_len, buf, len);
    out_len += len;
    return 0;
}

EXPORT(sp_patch) int32_t sp_patch(uint32_t len) {
    out_len = 0;
    len = len > IN_MAX ? IN_MAX : len;
    is_bank = (uint32_t)patch_json_is_bank(in_buf, len);
    int r = is_bank ? patch_json_read_bank(&bank, in_buf, len) : patch_json_read(&patch, in_buf, len);
    if (r < 0) {
        return r;
    }
    gt2_song* s = gt2_song_init(region, sizeof region);
    if (!s) {
        return -2;
    }
    int made = is_bank ? patch_json_bank_song(s, &bank, SID_MODEL_6581)
                       : patch_json_song(s, patch.sound, patch.name, SID_MODEL_6581);
    if (made > 0) {
        return -1;
    }
    if (made || gt2_sng_write(s, 1, write_out, NULL)) {
        return -2;
    }
    return r;
}

EXPORT(sp_bank) uint32_t sp_bank(void) {
    return is_bank ? bank.count : 0u;
}
EXPORT(sp_bank_name) const char* sp_bank_name(uint32_t k) {
    return k < bank.count ? bank.names[k] : bank.name;
}

EXPORT(sp_song_bytes) uint32_t sp_song_bytes(void) {
    return out_len;
}
EXPORT(sp_sound) const uint8_t* sp_sound(void) {
    return patch.sound;
}

EXPORT(sp_write) uint32_t sp_write(void) {
    char name[LAB_SOUND_NAME + 1u];
    memcpy(name, in_buf + LAB_SOUND_BYTES, LAB_SOUND_NAME);
    name[LAB_SOUND_NAME] = 0;
    out_len = patch_json_write((const uint8_t*)in_buf, name, (char*)out_buf, OUT_MAX);
    return out_len;
}
