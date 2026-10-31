// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_USER_PROFILE_H
#define SIDOU_USER_PROFILE_H

#include <stdint.h>

#include "live.h"
#include "store.h"

#ifdef __cplusplus
extern "C" {
#endif

#define STORE_KEY_PROFILE(n) (0x101u + (n))
#define USER_PROFILE_TYPE 2u
#define USER_PROFILE_VERSION 2u
#define USER_PROFILE_V1_BYTES 7u
enum {
    USER_PROFILE_V2_BYTES = USER_PROFILE_V1_BYTES + 1u + SID_EXPERT_COUNT,
    USER_PROFILE_BYTES = USER_PROFILE_V2_BYTES
};
#define USER_PROFILE_FORCE_MS 2000u

typedef enum {
    USER_PROFILE_NOTHING = 0,
    USER_PROFILE_SAVED = 1,
    USER_PROFILE_SAVED_GAPS = 2,
    USER_PROFILE_FOREIGN = 3,
    USER_PROFILE_FAILED = 4
} user_profile_result;

typedef struct {
    live* l;
    store* st;
    uint8_t queued;
    uint8_t sent;
    uint8_t result;
    uint8_t result_slot;
    uint8_t forcing;
    uint8_t errors;
    uint32_t since;
    uint8_t rec[USER_PROFILE_BYTES];
} user_profiles;

void user_profiles_init(user_profiles* u, live* l, store* st);
void user_profiles_load(user_profiles* u);
void user_profiles_forget(user_profiles* u);
int user_profile_save(user_profiles* u, uint32_t slot, uint32_t profile,
                      const uint8_t controls[LIVE_CHAR_FIELDS - 1u],
                      const uint8_t expert[SID_EXPERT_COUNT], uint32_t now_ms);
int user_profiles_busy(const user_profiles* u);
void user_profiles_tick(user_profiles* u, uint32_t now_ms, void (*force)(int on));
user_profile_result user_profiles_take(user_profiles* u, uint32_t* slot);

#ifdef __cplusplus
}
#endif

#endif
