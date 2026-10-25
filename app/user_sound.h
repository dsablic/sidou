// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* LAB's USER sounds on the store (docs/reference/sound-format.md): sixteen slots, USER 1..16, each
 * a sound (app/lab_sound.h) and its name, in one private store file (STORE_SOUNDS_NAME, type
 * STORE_FILE_SOUNDS); and the unsaved edit, one small record (USER_SOUND_EDIT_KEY) the settings'
 * debounce writes after the settings record and settings_load applies after it.
 *
 * The slots are not cached in RAM: live.sounds_valid says which hold a sound (user_sounds_load,
 * at boot), and a slot is read from the store when it is loaded (user_sound_read). A keep
 * (user_sound_save) rewrites the file whole, streamed through the store's own staging buffer:
 * the other slots from the file as it is, the kept one from a copy here, so a write costs one
 * slot's bytes of RAM. The file is the old one until the new one commits (a power cut leaves
 * either), and a later version's file is read for this version's bytes of each slot.
 *
 * Saving: user_sound_save sets the slot in live at once (its bit, and the sound as the current
 * sound's base: a keep is of the sound playing) and queues the write; the views' frame ticks it
 * (user_sounds_tick) while it has work, until the file commits, and says so once (the result),
 * as app/user_profile.h's saves do: a busy store is asked again, held operations are forced after
 * USER_SOUND_FORCE_MS, a foreign or full store keeps nothing (the slot stays in live until the
 * next boot). One write at a time: a keep while one is written is refused (try again).
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_USER_SOUND_H
#define SIDOU_USER_SOUND_H

#include <stdint.h>

#include "lab_sound.h"
#include "live.h"
#include "store.h"

#ifdef __cplusplus
extern "C" {
#endif

#define USER_SOUNDS 16u
#define USER_SOUND_VERSION 1u
/* The file: [version][sound bytes][slots], then per slot [used][name][sound]. */
#define USER_SOUND_HEADER 3u
#define USER_SOUND_STRIDE (1u + LAB_SOUND_NAME + LAB_SOUND_BYTES)
#define USER_SOUND_FILE_BYTES (USER_SOUND_HEADER + USER_SOUNDS * USER_SOUND_STRIDE)
#define USER_SOUND_FORCE_MS 2000u

/* The unsaved edit: key 0x109 (after the USER profiles' 0x101..0x108), type 4:
 * [version][preset][edited][name][sound bytes][sound]. The preset byte is the settings
 * record's: 0..LIVE_PRESET_SLOTS - 1 a built-in or DICE, LIVE_PRESET_SLOTS + n USER n + 1. */
#define USER_SOUND_EDIT_KEY 0x109u
#define USER_SOUND_EDIT_TYPE 4u
#define USER_SOUND_EDIT_VERSION 1u
#define USER_SOUND_EDIT_BYTES (4u + LAB_SOUND_NAME + LAB_SOUND_BYTES)

typedef enum {
    USER_SOUND_NOTHING = 0,
    USER_SOUND_SAVED = 1,      /* committed */
    USER_SOUND_SAVED_GAPS = 2, /* committed, with the held operations forced */
    USER_SOUND_FOREIGN = 3,    /* the store holds another firmware's data: not saved */
    USER_SOUND_FULL = 4,       /* the store has no room for it: not saved */
    USER_SOUND_FAILED = 5      /* a flash error, or no usable flash: not saved */
} user_sound_result;

typedef struct {
    live* l;   /* whose sounds these are */
    store* st; /* NULL: nothing persists */
    uint8_t state; /* the write: idle, to begin, streaming, committing (user_sound.c) */
    uint8_t slot;  /* the slot being written */
    uint16_t at;   /* bytes of the file offered to the store */
    uint8_t result; /* a user_sound_result, until user_sounds_take */
    uint8_t result_slot;
    uint8_t forcing;
    uint8_t errors;  /* the store's error count (its low byte) when the file began */
    uint32_t since;  /* when the waiting began */
    uint8_t rec[USER_SOUND_STRIDE]; /* the slot being written, as the file holds it */
} user_sounds;

void user_sounds_init(user_sounds* u, live* l, store* st);
/* At boot, once the store is mounted, before the settings are applied: which slots hold a sound
 * (live.sounds_valid; a slot that is not a valid sound of this or a later version is empty). */
void user_sounds_load(user_sounds* u);
/* The store lost everything (a format): every slot empty, the current sound no USER one, no
 * write. */
void user_sounds_forget(user_sounds* u);
/* Slot n (0..USER_SOUNDS - 1) as the store holds it: its name (NUL-terminated, LAB_SOUND_NAME + 1
 * bytes) and sound, 1; 0 for an empty or bad slot (checked with lab_sound_valid). */
int user_sound_read(const user_sounds* u, uint32_t n, char* name, uint8_t sound[LAB_SOUND_BYTES]);
/* USER sound n loaded as the current sound (live_load_user): 1, or 0 for an empty slot. */
int user_sound_load(user_sounds* u, uint32_t n);
/* Keeps sound under name (up to LAB_SOUND_NAME characters) in slot n: live has it at once (its
 * bit, the base and the name, live.user_sound n + 1) and the write is queued. 1 if nothing waits
 * on a store, 0 if queued (user_sounds_tick's result tells its end), -1 if a write runs: nothing
 * done. */
int user_sound_save(user_sounds* u, uint32_t n, const char* name,
                    const uint8_t sound[LAB_SOUND_BYTES], uint32_t now_ms);
/* 1 while there is work: a write queued or running, a result not yet taken. */
int user_sounds_busy(const user_sounds* u);
/* The write: begun, streamed, committed; forced held operations asked for (force, NULL: none;
 * on 1 or 0) and released. Once a tick (a frame). */
void user_sounds_tick(user_sounds* u, uint32_t now_ms, void (*force)(int on));
/* The result and the slot it was for, once (then USER_SOUND_NOTHING). */
user_sound_result user_sounds_take(user_sounds* u, uint32_t* slot);

/* The unsaved edit (for app/settings.c). The record of the sound playing, into out
 * (USER_SOUND_EDIT_BYTES); its length. */
uint32_t user_sound_edit_gather(const live* l, uint8_t* out);
/* The settings record's preset byte for the sound playing. */
uint8_t user_sound_preset_byte(const live* l);
/* A record over what the settings loaded: when its preset is the one playing, its sound is
 * applied (the base kept, so the edited mark comes back; DICE's base is the sound, as it was
 * left). 1 if applied. */
int user_sound_edit_apply(live* l, const uint8_t* rec, uint32_t len);

#ifdef __cplusplus
}
#endif

#endif
