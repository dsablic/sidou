// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* UI sounds on chip B's spare voice (M2b Task 7, M1e Task 6): a cursor blip, a confirm, a
 * toast's chime, an error buzz and SAVE's drive "chunk", off by default (the menu's SOUNDS).
 *
 * Each sound is data in GoatTracker's sound-effect format (its readme, section 6.3): attack/
 * decay, sustain/release, a pulse byte (written to both $D402 and $D403), then the frames:
 * $01-$81 a waveform (control register, the gate in it), kept until the next; $82-$DF a note,
 * D-0 to B-7, which every frame has; $00 the end. They are stepped at 50 Hz (every other
 * live_tick), so a sound edited in GoatTracker and converted with INS2SND2 plays as it does
 * there.
 *
 * A sound never steals: it plays on a voice of chip B that is free, and every write goes
 * through chip B's sink.
 * - The keys play chip A: chip B is all free (voice 3, chip B's volume up, no filter).
 * - The keys play chip B (a song on chip A): a free voice is one whose gate is off and whose
 *   release has run out (live's voice state and the release time), the newest of them, so not
 *   the one live's allocator takes next; none under mono sync (it uses voices 1 and 3). While
 *   a sound holds the voice, live's allocator skips it (live.ui_voice) and its writes to the
 *   voice (and its $D417 bit: no filter for the borrowed voice) are held back by a filter on
 *   live's sink; a note that finds the other two voices busy takes the voice back at once, and
 *   the sound stops. When the sound and its release are done the voice gets live's registers
 *   back (live_rewrite_voice).
 * - A sound stops, writing nothing more, if the keys move between chips while it plays.
 * Never on chip A; never while a song track plays on chip B (M5). While a sound and its
 * release play, chip B is in use (ui_sound_busy: AUDIO_B_UI, platform/fm1/audio.h).
 *
 * Portable C with no hardware access. Main context. */
#ifndef SIDOU_UI_SOUND_H
#define SIDOU_UI_SOUND_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    UI_SOUND_BLIP = 0,    /* the menu's cursor moved */
    UI_SOUND_CONFIRM = 1, /* a value changed with ALGORITHM, a view opened */
    UI_SOUND_CHIME = 2,   /* a toast */
    UI_SOUND_BUZZ = 3,    /* a refused action */
    UI_SOUND_CHUNK = 4,   /* SAVE's drive chunk (M2a Task 5) */
    UI_SOUND_COUNT = 5
} ui_sound_id;

#define UI_SOUND_NONE 0xffu
/* The longest a sound's data may be (INS2SND2's limit). */
#define UI_SOUND_MAX_BYTES 128u

typedef struct {
    live* l;
    live_sink_fn sink; /* chip B's */
    void* sink_ctx;
    uint8_t on;    /* the SOUNDS setting: 0 at init */
    uint8_t id;    /* the sound playing, UI_SOUND_NONE */
    uint8_t pos;   /* its next byte */
    uint8_t voice; /* the voice it holds, UI_SOUND_NONE */
    uint8_t wave;  /* the waveform its frames play */
    uint8_t phase; /* the 50 Hz step: every other tick */
    uint8_t keys;  /* the keys' chip when it started */
    uint16_t tail; /* ticks of release left after its end */
    uint16_t free_in[LIVE_VOICES]; /* ticks until each voice's release has run out */
    uint32_t stamp[LIVE_VOICES];   /* each voice's stamp as last seen */
    live_sink_fn live_sink;        /* live's own sink while the filter stands in for it */
    void* live_ctx;
} ui_sound;

/* Off; chip B's sink (reg_ring_sink and its out bound to chip B on the device). */
void ui_sound_init(ui_sound* us, live* l, live_sink_fn sink, void* sink_ctx);
/* SOUNDS on (1) or off (0); off stops a sound playing (its voice given back). */
void ui_sound_set(ui_sound* us, int on);
/* Plays a sound, if SOUNDS is on and a voice is free (above); nothing otherwise. */
void ui_sound_play(ui_sound* us, ui_sound_id id);
/* Once a live_tick, after it: the voices' releases, the sound's next frame (at 50 Hz), its
 * end and its release. */
void ui_sound_tick(ui_sound* us);
/* 1 while a sound or its release plays: chip B in use for it. */
int ui_sound_busy(const ui_sound* us);
/* The data of a sound (GoatTracker's format), for the tests and the docs. */
const uint8_t* ui_sound_data(ui_sound_id id);

#ifdef __cplusplus
}
#endif

#endif
