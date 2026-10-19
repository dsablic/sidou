// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The FM-1 front panel played into live: note keys, buttons, encoders, the MASTER pot and
 * the LEDs.
 *
 * Portable C with no hardware access. The device's input code reads the matrix (Felucca's
 * fm1_input.h) and hands the debounced state here; the host tests drive it directly.
 *
 * Ids are Felucca's (platform/fm1/VENDORED.md): buttons 0..13, note keys 14..40 (F3..G5,
 * here as note-key indexes 0..26), encoders 0..6. */
#ifndef SIDOU_PANEL_H
#define SIDOU_PANEL_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

#define PANEL_NOTE_KEYS 27u
/* Note key 0 plays F3 at octave 0. */
#define PANEL_BASE_NOTE 53u
/* The keys have no velocity: 127 sounds as velocity off does, whatever the preset's velocity
 * target (docs/reference/controls.md). */
#define PANEL_VELOCITY 127u

/* Button ids (fm1_in.buttons bits). */
#define PANEL_BTN_OCT_DOWN 0u
#define PANEL_BTN_OCT_UP 1u
#define PANEL_BTN_FX 2u
/* The effect slots' shortcut, in every view (docs/reference/controls.md): EDIT held turns
 * K1-K4 into the selected slot's Amount, A, B and Mix; SCL selects the next slot (Distortion,
 * Modulation, FX); ENV steps the selected slot's algorithm. The CHIP view (app/chip_view.h)
 * edits the same selected slot. */
#define PANEL_BTN_SCL 3u
#define PANEL_BTN_ENV 4u
#define PANEL_BTN_LFO 5u
#define PANEL_BTN_EDIT 6u
/* The song views (app/song_view.h): GLO held turns the leftmost PANEL_GLO_KEYS note keys
 * into the voices' mutes and solos (panel_take_glo), in every view; SEQ steps through the
 * song views, PLAY plays and stops the song, REC turns record mode on and off. Their ids,
 * like LFO's and ARP's, are Felucca's and not yet confirmed on a SIDOU unit
 * (docs/hardware-test.md). */
#define PANEL_BTN_GLO 7u
/* The views (app/views.h): opens the menu, and goes back. */
#define PANEL_BTN_HOME 8u
/* MIDI learn (app/learn_view.h, docs/reference/controls.md): arms it, and clears. */
#define PANEL_BTN_SAVE 9u
#define PANEL_BTN_ARP 10u
#define PANEL_BTN_SEQ 11u
#define PANEL_BTN_PLAY 12u
#define PANEL_BTN_REC 13u
/* GLO's layer: keys 0-2 mute voices 1-3, keys 3-5 solo them. */
#define PANEL_GLO_KEYS 6u
/* The LED id of note key k is PANEL_KEY_ID0 + k; buttons are their own ids. */
#define PANEL_KEY_ID0 14u

/* Encoder ids (fm1_enc_take). SELECT and ALGORITHM drive the menus (app/views.h). */
#define PANEL_ENC_SELECT 0u
#define PANEL_ENC_ALGORITHM 1u
#define PANEL_ENC_K1 2u
#define PANEL_ENC_K4 5u
/* The big encoder PRESETS steps the preset in every view that does not take it: EDIT held reverts
 * the current preset to its factory settings (live_preset_step). */
#define PANEL_ENC_PRESETS 6u
#define PANEL_ENCODERS 7u

/* The MASTER pot moves the volume only when it changed by at least this much (ADC noise). */
#define PANEL_MASTER_HYSTERESIS 4

/* What K1..K4 turn without EDIT held (panel.page): the views set it for the view that shows
 * (app/views.h), so the panel and MIDI learn, which both ask panel_knob_param, agree. */
#define PANEL_PAGE_LIVE 0u      /* the cutoff, resonance, the sound's own control and release */
#define PANEL_PAGE_CHARACTER 1u /* age/heat, DAC sloppiness, output thump, filter bleed */
#define PANEL_PAGE_SLOT 2u      /* the selected slot's Amount, A, B and Mix */
/* Nothing: the view turns K1..K4 itself (the CHIP view on a song's chip A, whose controls are
 * no live parameter: MIDI learn binds nothing there). EDIT held still turns the slot. */
#define PANEL_PAGE_NONE 3u
#define PANEL_PAGE_EXPERT_FIRST 4u
#define PANEL_PAGE_EXPERT_END (PANEL_PAGE_EXPERT_FIRST + (SID_EXPERT_COUNT + 3u) / 4u)
/* A detent moves a character control this many steps: 32 detents end to end, as a slot's. */
#define PANEL_CHARACTER_DETENT 4

typedef struct {
    /* The note each key sent at key-down, LIVE_NO_NOTE while it is up: key-up releases that
     * note even when the octave changed in between. */
    uint8_t key_note[PANEL_NOTE_KEYS];
    int16_t master_raw; /* the last pot value applied, -1 for none */
    uint32_t held;      /* the debounced buttons (bit i: button i), for the EDIT layer */
    /* A PANEL_PAGE_*, PANEL_PAGE_LIVE from panel_init. Derived, not state: the views
     * (views_buttons, views_encoder) set it from the view that shows before every event and
     * again before the panel reads it; nothing else should rely on it between events. */
    uint8_t page;
    /* PLAY's LED: the song plays (or starts), as the views' transport has it (main.c). */
    uint8_t playing;
    /* GLO's layer: the presses of the leftmost PANEL_GLO_KEYS keys while GLO was held, not
     * yet taken (panel_take_glo), bit k: key k. */
    uint8_t glo;
    /* Keys that sound nothing until they go up (bit k: key k): pressed while GLO was held,
     * or taken by a view (panel_quiet_keys). */
    uint32_t quiet;
    /* The keys that sound a note (key_note set), bit k: key k: with none down, pressed,
     * sounding or quiet, panel_note_keys has nothing to do. */
    uint32_t sounding;
    /* Called (with master_ctx) when the MASTER pot moves the volume: the views ask for the
     * popup (views_set_panel). NULL: none. Only a move that lands calls it, so a pot at rest
     * costs nothing. */
    void (*master_hook)(void* ctx);
    void* master_ctx;
} panel;

/* The LEDs, as Felucca's input state lays out the keys. */
typedef struct {
    uint32_t buttons; /* bit i: button i */
    uint32_t notes;   /* bit k: note key k */
} panel_leds;

void panel_init(panel* p);
/* down: the debounced note keys (bit k = key k); pressed: keys that went down since the last
 * call (Felucca's note edges), which catches a press and release between two calls. A key
 * that goes down plays live_key_note of its base note; a key that goes up releases the note
 * it played; a key pressed again while held retriggers. With GLO held (p->held, from the
 * last panel_buttons) a key that goes down sounds nothing (until it goes up, GLO held or
 * not), and the leftmost PANEL_GLO_KEYS keys' presses are kept for panel_take_glo; a key
 * that sounds already sounds on until it goes up. A quiet key (panel_quiet_keys) sounds
 * nothing until it goes up. */
void panel_note_keys(panel* p, live* l, uint32_t down, uint32_t pressed);
/* GLO's presses since the last call (bit k: key k of the PANEL_GLO_KEYS), cleared. */
uint32_t panel_take_glo(panel* p);
/* The keys in mask are another's until they go up (a view's: the song views' jam): a note
 * they sound is released, and they sound nothing until then. */
void panel_quiet_keys(panel* p, live* l, uint32_t mask);
/* Button press edges: OCT-, OCT+, FX (the 6581/8580 toggle), SCL and ENV (the effect slots);
 * held: the debounced buttons, which says whether EDIT is held for panel_encoder. */
void panel_buttons(panel* p, live* l, uint32_t pressed, uint32_t held);
/* The parameter encoder enc turns with the buttons held (bit i: button i) on p's page: with
 * EDIT held, K1..K4 the selected slot's Amount, A, B and Mix on any page; without, on
 * PANEL_PAGE_LIVE K1 the cutoff, K2 the resonance, K3 the sound's own control (the pulse width,
 * the decay, or on a sync sound the ratio, which is no parameter) and K4 the release, on PANEL_PAGE_CHARACTER K1..K4
 * age/heat, DAC sloppiness, output thump and filter bleed, on PANEL_PAGE_SLOT the selected
 * slot's Amount, A, B and Mix, on an EXPERT page its four consecutive parameters (one raw
 * step per detent). LIVE_PARAM_COUNT for unused final-page expert controls, K3 on PANEL_PAGE_LIVE without EDIT on a
 * sync sound (the ratio), for K1..K4 on PANEL_PAGE_NONE without EDIT, and every other encoder. p NULL: PANEL_PAGE_LIVE.
 * panel_encoder turns what this names, and MIDI learn picks the knob's parameter here, so the
 * two cannot disagree. */
uint32_t panel_knob_param(const panel* p, const live* l, uint32_t held, uint32_t enc);
/* Detent steps of encoder enc (+ = clockwise): what panel_knob_param names, a slot control by
 * LIVE_SLOT_DETENT a detent, a character control by PANEL_CHARACTER_DETENT, clamped at both
 * ends, and on PANEL_PAGE_LIVE live's knobs. */
void panel_encoder(panel* p, live* l, uint32_t enc, int32_t steps);
/* A MASTER pot reading, 0..1023 (a negative reading, an ADC timeout, is ignored). */
void panel_master(panel* p, live* l, int32_t raw);
/* Held keys, the octave on OCT- (below 0) or OCT+ (above 0), FX while the 8580 plays, EDIT
 * while the selected slot has an algorithm, and PLAY while the song plays (panel.playing). */
panel_leds panel_get_leds(const panel* p, const live* l);

#ifdef __cplusplus
}
#endif

#endif
