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
 * Modulation, FX). The CHIP view (app/chip_view.h) edits the same selected slot. ENV, LFO and
 * FX open LAB's pages from LIVE (app/views.c) and are its pages in LAB (app/lab_view.h); the
 * other views leave them alone. */
#define PANEL_BTN_SCL 3u
#define PANEL_BTN_ENV 4u
#define PANEL_BTN_LFO 5u
#define PANEL_BTN_EDIT 6u
/* The song views (app/song_view.h): GLO held, or latched, turns the leftmost PANEL_GLO_KEYS note
 * keys into the voices' mutes and solos (panel_take_glo), in every view, and the other keys play; SEQ steps through the
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

/* The MASTER pot (panel_master), in its 0..1023 counts (platform/fm1 averages 4 conversions).
 * The gain follows a change of PANEL_MASTER_HYSTERESIS (0.4 % of the travel: no audible step
 * on the audio taper); the popup only a turn of PANEL_MASTER_POPUP (1.6 %) from where the pot
 * last rested, so a reading that wanders by up to 15 counts at rest may nudge the gain but never
 * shows it. The last PANEL_MASTER_END counts at either end read as that end: 0 and 1023 land
 * exactly, and a pot resting there holds them through the noise. A real turn asks for the popup
 * and for PANEL_MASTER_FOLLOW calls (a second at LIVE_TICK_HZ, renewed by each further 1.6 %)
 * every change it lands asks again, so the popup says the value the turn ends at. The asks are
 * at least PANEL_MASTER_ASK_GAP calls apart (120 ms at LIVE_TICK_HZ, three 40 ms frames): the
 * screen opens the popup only in a frame without an ask, so a turn that landed a change every
 * frame kept it shut. A change landed in the gap is asked for when it ends. */
#define PANEL_MASTER_HYSTERESIS 4
#define PANEL_MASTER_POPUP 16
#define PANEL_MASTER_END 8
#define PANEL_MASTER_FOLLOW 100u
#define PANEL_MASTER_ASK_GAP 12u

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
/* LAB's pages (app/lab_view.h), OSC first: their cards' parameters (panel_lab_param). The view
 * turns its cards itself; the page is for the popup's names and MIDI learn. */
#define PANEL_PAGE_LAB_FIRST PANEL_PAGE_EXPERT_END
#define PANEL_PAGE_LAB_END (PANEL_PAGE_LAB_FIRST + 6u)
/* A detent moves a character control this many steps: 32 detents end to end, as a slot's. */
#define PANEL_CHARACTER_DETENT 4

typedef struct {
    /* The note each key sent at key-down, LIVE_NO_NOTE while it is up: key-up releases that
     * note even when the octave changed in between. */
    uint8_t key_note[PANEL_NOTE_KEYS];
    int16_t master_raw;   /* the last pot value applied, -1 for none */
    int16_t master_shown; /* where the turn the popup follows was last real, -1 for none */
    uint8_t master_follow; /* calls left in which a landed change asks the popup again */
    uint8_t master_wait;    /* calls left before the popup may be asked again */
    uint8_t master_pending; /* 1: a landed change waits for its ask */
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
    /* 1 while GLO's layer is latched (a double tap on GLO, app/views.c glo_tap; a tap ends it):
     * the keys are the mutes and solos as with GLO held. GLO's LED is lit. */
    uint8_t glo_latch;
} panel;

/* The LEDs, as Felucca's input state lays out the keys: lit, and dim (the hardware's glow, a
 * thirtieth of lit: set or available, where lit is active now). A bit in both is lit. */
typedef struct {
    uint32_t buttons;     /* bit i: button i lit */
    uint32_t notes;       /* bit k: note key k lit */
    uint32_t buttons_dim; /* dim, and not lit */
    uint32_t notes_dim;
} panel_leds;

/* Half a blink's period (ms): the count-in's, SAVE's question's and a solo's, about 2.8 Hz. */
#define PANEL_BLINK_HALF_MS 180u
/* PLAY is lit on each beat's first PANEL_BEAT_LIT of its 256 (a quarter). */
#define PANEL_BEAT_LIT 64u
#define PANEL_BEAT_NONE 0xffu
/* The arpeggio's rates of this many frames a step or fewer light ARP once a cycle, not each
 * step: faster reads as flicker. A step lights its first half. */
#define PANEL_ARP_FAST_FRAMES 2u
/* The step view's playhead key is inverted this long (ms) as it moves. */
#define PANEL_FLICK_MS 50u
#define PANEL_STEPS 16u
#define PANEL_NO_STEP 0xffu
/* REC's LED (panel_led_in.rec). */
#define PANEL_REC_OFF 0u
#define PANEL_REC_ARMED 1u /* record mode on, the song not recording: dim */
#define PANEL_REC_COUNT 2u /* the count-in: blinking */
#define PANEL_REC_ON 3u    /* recording: lit */

/* What the LEDs need beyond the panel and live (the views fill it, views_leds): all of it zero
 * but beat and step_head, which are PANEL_BEAT_NONE and PANEL_NO_STEP, shows nothing of it. */
typedef struct {
    uint32_t ms;        /* the clock the blinks run on */
    uint8_t beat;       /* the song's place in its beat, 0..255, PANEL_BEAT_NONE: not playing */
    uint8_t rec;        /* PANEL_REC_* */
    uint8_t silent;     /* the voices the mutes and solos keep silent (bit v: voice v) */
    uint8_t solo;       /* the soloed voices */
    uint8_t ask;        /* 1: a question waits for SAVE to confirm */
    uint8_t step_on;    /* 1: the STEP view shows: its page is on the keys */
    uint8_t step_head;  /* the playhead's step on the page, PANEL_NO_STEP: none */
    uint16_t step_set;  /* the steps with a note (bit s) */
    uint16_t step_rows; /* the steps that exist: a pattern's last page may be short */
    uint32_t head_ms;   /* when the playhead came to step_head */
} panel_led_in;

void panel_init(panel* p);
/* down: the debounced note keys (bit k = key k); pressed: keys that went down since the last
 * call (Felucca's note edges), which catches a press and release between two calls. A key
 * that goes down plays live_key_note of its base note; a key that goes up releases the note
 * it played; a key pressed again while held retriggers. With GLO held (p->held, from the
 * last panel_buttons) or latched (p->glo_latch) one of the leftmost PANEL_GLO_KEYS keys that
 * goes down sounds nothing (until it goes up, GLO held or not) and its press is kept for
 * panel_take_glo; the other keys play as always; a key that sounds already sounds on until it
 * goes up. A quiet key (panel_quiet_keys) sounds
 * nothing until it goes up. */
void panel_note_keys(panel* p, live* l, uint32_t down, uint32_t pressed);
/* GLO's presses since the last call (bit k: key k of the PANEL_GLO_KEYS), cleared. */
uint32_t panel_take_glo(panel* p);
/* The keys in mask are another's until they go up (a view's: the song views' jam): a note
 * they sound is released, and they sound nothing until then. */
void panel_quiet_keys(panel* p, live* l, uint32_t mask);
/* Button press edges: OCT-, OCT+ and SCL (the next effect slot);
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
/* Card k (0..3) of LAB's page `page` (0 OSC .. 4 FX, app/lab_view.h): OSC the pulse width (no
 * parameter, LIVE_PARAM_COUNT, while it reads RATIO: a mono-sync sound without the pulse bit),
 * the PWM's depth and rate and the fine tune; ENVELOPE the attack, decay, sustain and release;
 * FILTER the cutoff, resonance and routing, and nothing for K4; MOD the vibrato's rate, depth and
 * delay and the velocity target; FX the selected slot's Amount, A, B and Mix. What
 * panel_knob_param names on a LAB page, EDIT held or not. LIVE_PARAM_COUNT past the pages. */
uint32_t panel_lab_param(const live* l, uint32_t page, uint32_t k);
/* Detent steps of encoder enc (+ = clockwise): what panel_knob_param names, a slot control a
 * position of its walk a detent (live_walk_slot), a character control by PANEL_CHARACTER_DETENT,
 * clamped at both ends, and on PANEL_PAGE_LIVE live's knobs. */
void panel_encoder(panel* p, live* l, uint32_t enc, int32_t steps);
/* A MASTER pot reading, 0..1023 (a negative reading, an ADC timeout, is ignored). */
void panel_master(panel* p, live* l, int32_t raw);
/* Held keys, the octave on OCT- (below 0) or OCT+ (above 0), FX while the 8580 plays, EDIT
 * while the selected slot has an algorithm, GLO while latched, and with in (NULL: none of
 * this, the panel's own lights):
 * - ARP dim while on, lit on each arpeggio step's first half (once a cycle at the fastest rates);
 * - PLAY dim while the song plays, lit on each beat's first quarter;
 * - REC dim while armed, blinking through the count-in, lit while recording;
 * - SAVE blinking while a question waits for it;
 * - with GLO held or latched, keys 1-3 lit for a voice that sounds, keys 4-6 blinking for a solo
 *   and dim otherwise;
 * - in STEP the keys 1-16 show the page, lit a note and dim an empty step, the playhead's key
 *   inverted for PANEL_FLICK_MS, in place of the held keys' lights. */
panel_leds panel_get_leds(const panel* p, const live* l, const panel_led_in* in);

#ifdef __cplusplus
}
#endif

#endif
