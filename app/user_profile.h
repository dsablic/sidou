// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The USER chip profiles on the store (M2a Task 6): eight slots, USER 1..8, each a built-in base
 * profile and the character's four controls (docs/reference/profile-format.md). A profile is a
 * chip's setup as a player left it; the CHIP view saves one (EDIT + ALGORITHM picks a slot to
 * save to, SAVE stores) and picks one (ALGORITHM, or CC 85 above the built-in ones).
 *
 * The slots themselves live in `live` (user_valid, user_base, user_ctl: what the picker, CC 85
 * and the CHIP view read, no store needed); this module reads them from the store at boot
 * (user_profiles_load) and writes a slot the player saved. Each slot is its own store key,
 * STORE_KEY_PROFILE(n), so the settings record does not grow and a save writes 7 bytes.
 *
 * Saving: user_profile_save puts the slot into live at once and queues the write; the views'
 * frame ticks it (user_profiles_tick, only while it has work: views.c keeps the settings
 * wake-up bit up) until the store takes the record, then until it commits, and says so (the
 * result). A store that cannot take it now (STORE_BUSY: another write runs, the pool is short,
 * the device is not quiet with programs held) is asked again on later ticks; after
 * USER_PROFILE_FORCE_MS of waiting the platform's forced held operations are asked for, as
 * for an explicit settings save, and the result says GAPS. A foreign store keeps nothing: the
 * slot stays in live until the next boot.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_USER_PROFILE_H
#define SIDOU_USER_PROFILE_H

#include <stdint.h>

#include "live.h"
#include "store.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Slot n (0..7)'s store key and the value's type. The firmware's keys: 0x100 the settings,
 * 0x101..0x108 these. */
#define STORE_KEY_PROFILE(n) (0x101u + (n))
#define USER_PROFILE_TYPE 2u
#define USER_PROFILE_VERSION 1u
/* The value, schema 1: version, base profile, the number of controls (4), the controls. A later
 * schema appends after them (the expert layer, M2b Task 6). */
#define USER_PROFILE_V1_BYTES 7u
#define USER_PROFILE_FORCE_MS 2000u

typedef enum {
    USER_PROFILE_NOTHING = 0,
    USER_PROFILE_SAVED = 1,      /* committed */
    USER_PROFILE_SAVED_GAPS = 2, /* committed, with the held operations forced */
    USER_PROFILE_FOREIGN = 3     /* the store holds another firmware's data: not saved */
} user_profile_result;

typedef struct {
    live* l;   /* whose slots these are */
    store* st; /* NULL: nothing persists */
    uint8_t queued;  /* bit n: slot n waits to be handed to the store */
    uint8_t sent;    /* n + 1: slot n's record is with the store, not yet committed; 0 none */
    uint8_t result;  /* a user_profile_result, until user_profiles_take */
    uint8_t result_slot;
    uint8_t forcing;
    uint32_t since;  /* when the waiting began */
    uint8_t rec[USER_PROFILE_V1_BYTES]; /* the record being written: the store programs from it */
} user_profiles;

void user_profiles_init(user_profiles* u, live* l, store* st);
/* At boot, once the store is mounted: every slot the store holds, into live (user_valid and the
 * rest; a record that is not a profile of this schema is an empty slot). */
void user_profiles_load(user_profiles* u);
/* The store lost everything (a format): every slot empty, nothing queued. */
void user_profiles_forget(user_profiles* u);
/* Slot (0..7) set to the base profile and controls in live, now, and its write queued. 1 if
 * nothing waits on a store (the slot is as saved as it gets), 0 if the write is queued: its end
 * is user_profiles_tick's result. */
int user_profile_save(user_profiles* u, uint32_t slot, uint32_t profile,
                      const uint8_t controls[LIVE_CHAR_FIELDS - 1u], uint32_t now_ms);
/* 1 while there is work: a write queued, one committing, a result not yet taken. */
int user_profiles_busy(const user_profiles* u);
/* Queued writes handed to the store, a commit noticed, forced held operations asked for
 * (force, NULL: none; on 1 or 0) and released. Once a tick (a frame). */
void user_profiles_tick(user_profiles* u, uint32_t now_ms, void (*force)(int on));
/* The result and the slot it was for, once (then USER_PROFILE_NOTHING). */
user_profile_result user_profiles_take(user_profiles* u, uint32_t* slot);

#ifdef __cplusplus
}
#endif

#endif
