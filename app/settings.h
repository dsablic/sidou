// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The settings that persist (M2a Task 5): one record on the store (app/store.h), versioned.
 *
 * What it holds: CARDS, the UI sounds (SOUNDS), KEYS, the preset and every live parameter (the
 * keys' chip model, profile and character, the effect slots, the envelope mode and the
 * preset's edits: the instrument as it was left; not the MASTER volume, which is the pot),
 * MIDI learn's bindings, the firmware version last booted (for the full boot once after
 * an install), TRACKER ROWS, THEME, the last song, SPREAD and SID MODEL. Not the octave, held notes, controllers
 * or view on screen.
 *
 * The record, little-endian bytes (SETTINGS_BYTES; docs/reference/settings-format.md):
 *   0  schema version (SETTINGS_VERSION)
 *   1  CARDS (0 off, 1 on)
 *   2  SOUNDS (0 off, 1 on)
 *   3  KEYS (a live_keys_mode)
 *   4  the preset: 0..4 a built-in or DICE, 5 + n USER sound n + 1 (an older firmware clamps it
 *      to DICE)
 *   5  the live parameters that follow (42 in v1)
 *   6  each live parameter in order, one byte each but the cutoff and the pulse width (two);
 *      the volume (the MASTER pot's) is 0 and ignored
 *  50  MIDI learn: each parameter's bound CC (MIDI_MAP_NO_CC: none), 42 bytes
 *  92  the firmware version, NUL-padded, SETTINGS_VERSION_CHARS bytes
 * 106  TRACKER ROWS (0 BAR, 1 SCROLL; schema 2)
 * 107  THEME (0 HIGH CONTRAST, 1 C64; schema 3)
 * 108  last song's filename, 16 NUL-padded bytes (schema 4); absent: built-in song
 * 124  expert count, then raw expert overrides, then their learned CCs (schema 5)
 * 125 + 2 x the expert count  SPREAD, 0..100 (schema 6; app/chip_spread.h)
 * 126 + 2 x the expert count  SID MODEL (SETTINGS_SID_*; schema 7; app/sid_view.h)
 * 127 + 2 x the expert count  the count of live parameters past the experts (3: PWM depth, PWM
 *      rate, fine tune), then each one's learned CC (schema 8; their values are not here)
 * A later schema only appends fields: an older firmware reads a newer record's v1 part, and a
 * newer one fills what an older record lacks with its defaults (settings_load's migrations).
 *
 * Saving: a background save 5 s (SETTINGS_DEBOUNCE_MS) after the last change of a persisted
 * value, the record written whole as one store_put; SAVE saves at once (settings_save_now). A
 * store that cannot take it now (STORE_BUSY: a write runs, the pool is short, or the device is
 * not quiet yet with programs held) is asked again on later ticks; an explicit save still
 * waiting after SETTINGS_FORCE_MS forces the held operations (the force hook: short gaps the
 * user asked for) and says so (SAVED (GAPS)). A foreign store keeps nothing.
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_SETTINGS_H
#define SIDOU_SETTINGS_H

#include <stdint.h>

#include "live.h"
#include "midi_learn.h"
#include "store.h"
#include "ui_sound.h"
#include "user_sound.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The record's store key and type. The firmware's keys start at 0x100 (0x101.. are Task 6's
 * user profiles); those below are the emulator tests' (platform/fm1/store_fm1.h's hook). */
#define SETTINGS_KEY 0x100u
#define SETTINGS_TYPE 1u
#define SETTINGS_VERSION 8u
#define SETTINGS_VERSION_CHARS 14u /* "V" and the version, as the status screen shows it */
#define SETTINGS_PARAMS_V1 42u
/* The live parameters past the experts whose learned CCs schema 8 appends (their values are the
 * sound's, kept with it, not here). */
#define SETTINGS_PARAMS_V8 3u
#define SETTINGS_V1_BYTES (6u + SETTINGS_PARAMS_V1 + 2u + SETTINGS_PARAMS_V1 + SETTINGS_VERSION_CHARS)
#define SETTINGS_V2_BYTES (SETTINGS_V1_BYTES + 1u)
#define SETTINGS_V3_BYTES (SETTINGS_V2_BYTES + 1u)
#define SETTINGS_V4_BYTES (SETTINGS_V3_BYTES + STORE_NAME_MAX)
enum {
    SETTINGS_V5_BYTES = SETTINGS_V4_BYTES + 1u + 2u * SID_EXPERT_COUNT,
    SETTINGS_V6_BYTES = SETTINGS_V5_BYTES + 1u,
    SETTINGS_V7_BYTES = SETTINGS_V6_BYTES + 1u,
    SETTINGS_V8_BYTES = SETTINGS_V7_BYTES + 1u + SETTINGS_PARAMS_V8,
    SETTINGS_BYTES = SETTINGS_V8_BYTES
};
/* SID MODEL: a .sid tune's chips as its header names them (AUTO, the default: a named model plays
 * reSIDfp's profile of it, "unknown" and "both" the keys' setup), or the keys' setup for every
 * tune (KEYS). */
#define SETTINGS_SID_AUTO 0u
#define SETTINGS_SID_KEYS 1u
#define SETTINGS_DEBOUNCE_MS 5000u
#define SETTINGS_FORCE_MS 2000u

/* What an explicit save came to (settings_take_result). */
typedef enum {
    SETTINGS_NOTHING = 0,
    SETTINGS_SAVED = 1,      /* committed */
    SETTINGS_SAVED_GAPS = 2, /* committed, with the held operations forced */
    SETTINGS_FOREIGN = 3,    /* the store holds another firmware's data: nothing saved */
    SETTINGS_FAILED = 4      /* a flash error, or no usable flash: not saved */
} settings_result;

typedef struct {
    live* l;
    midi_learn* ml; /* NULL: no bindings */
    uint8_t* cards; /* the views' CARDS */
    uint8_t* tracker_rows; /* the song view's scroll mode */
    ui_sound* snd;  /* NULL: no SOUNDS */
    store* st;      /* NULL: nothing persists */
    const char* version;
    void (*force)(int on); /* the platform's forced held operations, or NULL */
    uint8_t out[SETTINGS_BYTES]; /* the record being written: the store programs it from here */
    uint32_t seen;               /* CRC of the record as last gathered, ... */
    uint32_t saved;              /* ... and as last written */
    uint32_t changed_at;
    uint32_t asked_at; /* an explicit save's start */
    uint8_t asked;     /* 1 an explicit save waits to be taken, 2 to commit */
    uint8_t forcing;
    uint8_t result; /* a settings_result, until settings_take_result */
    uint8_t started;
    uint8_t errors; /* the store's error count (its low byte) when it took the explicit save */
    char last_song[STORE_NAME_MAX + 1u];
    uint8_t sid_model; /* SID MODEL: SETTINGS_SID_* */
    /* The USER sounds (settings_sounds; NULL: none), which a preset byte of 5 + n loads from; the
     * unsaved edit (app/user_sound.h's record) being written: the store programs it from here;
     * its CRC as last gathered and as last written. */
    user_sounds* sounds;
    uint8_t edit[USER_SOUND_EDIT_BYTES];
    uint32_t edit_seen;
    uint32_t edit_saved;
} settings;

void settings_init(settings* s, live* l, midi_learn* ml, uint8_t* cards, uint8_t* tracker_rows, ui_sound* snd,
                   store* st, const char* version, void (*force)(int on));
/* The USER sounds a record's preset byte may name (5 + n): set before settings_load. */
void settings_sounds(settings* s, user_sounds* sounds);
/* The record as it stands now, into out (SETTINGS_BYTES); its length. */
uint32_t settings_gather(const settings* s, uint8_t* out);
/* Applies a record (of len bytes, any schema): every field it has, defaults for the fields a
 * later schema added. 1 if its firmware version is s->version, 0 if not (or if it is not a
 * record: nothing applied). */
int settings_apply(settings* s, const uint8_t* rec, uint32_t len);
/* At boot, after the store's mount, live_init and user_sounds_load, before the first tick: the
 * stored record applied, if any, then the unsaved edit (app/user_sound.h) over it. 1 if it was there and of this firmware version: the short boot; 0 (a first
 * boot, a new version, no record): the full boot once, and the version is saved. */
int settings_load(settings* s, uint32_t now_ms);
/* Main loop, once a tick: the debounce, and an explicit save's progress; the unsaved edit is
 * written by the same debounce, before the settings record (a cut between them leaves the new
 * sound, never an older edit over a newer record). Persisted setters
 * mark live.settings_dirty or midi_learn.settings_dirty; unchanged ticks gather no record.
 * live.settings_dirty stays nonzero through the debounce, retries and an unconsumed result,
 * so a frame caller can skip the call entirely when it is zero. */
void settings_tick(settings* s, uint32_t now_ms);
/* SAVE: the settings saved now, skipping the debounce; settings_take_result tells when. */
void settings_save_now(settings* s, uint32_t now_ms);
/* RESET: every preset's edits and every setting in the record back to the factory's (as a
 * first boot has them); the version, the last song and the MIDI learn bindings stay, and so do the songs and the USER
 * chip profiles (their own store keys). Persists by the debounce or settings_save_now. */
void settings_reset(settings* s);
/* The store lost the record (a format): it is written again once the store takes it. */
void settings_forget(settings* s);
/* The last loaded or saved song; an empty name boots the built-in song. */
void settings_set_song(settings* s, const char* name);
/* An explicit save's end, once (then SETTINGS_NOTHING). */
settings_result settings_take_result(settings* s);

#ifdef __cplusplus
}
#endif

#endif
