// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_PANEL_H
#define SIDOU_PANEL_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

#define PANEL_NOTE_KEYS 27u
#define PANEL_BASE_NOTE 53u
#define PANEL_VELOCITY 127u

#define PANEL_BTN_OCT_DOWN 0u
#define PANEL_BTN_OCT_UP 1u
#define PANEL_BTN_FX 2u
#define PANEL_BTN_SCL 3u
#define PANEL_BTN_ENV 4u
#define PANEL_BTN_LFO 5u
#define PANEL_BTN_EDIT 6u
#define PANEL_BTN_GLO 7u
#define PANEL_BTN_HOME 8u
#define PANEL_BTN_SAVE 9u
#define PANEL_BTN_ARP 10u
#define PANEL_BTN_SEQ 11u
#define PANEL_BTN_PLAY 12u
#define PANEL_BTN_REC 13u
#define PANEL_GLO_KEYS 6u
#define PANEL_KEY_ID0 14u

#define PANEL_ENC_SELECT 0u
#define PANEL_ENC_ALGORITHM 1u
#define PANEL_ENC_K1 2u
#define PANEL_ENC_K4 5u
#define PANEL_ENC_PRESETS 6u
#define PANEL_ENCODERS 7u

#define PANEL_MASTER_HYSTERESIS 4
#define PANEL_MASTER_POPUP 16
#define PANEL_MASTER_END 8
#define PANEL_MASTER_FOLLOW 100u
#define PANEL_MASTER_ASK_GAP 12u

#define PANEL_PAGE_LIVE 0u
#define PANEL_PAGE_CHARACTER 1u
#define PANEL_PAGE_SLOT 2u
#define PANEL_PAGE_NONE 3u
#define PANEL_PAGE_EXPERT_FIRST 4u
#define PANEL_PAGE_EXPERT_END (PANEL_PAGE_EXPERT_FIRST + (SID_EXPERT_COUNT + 3u) / 4u)
#define PANEL_PAGE_LAB_FIRST PANEL_PAGE_EXPERT_END
#define PANEL_PAGE_LAB_END (PANEL_PAGE_LAB_FIRST + 6u)
#define PANEL_CHARACTER_DETENT 4

typedef struct {
    uint8_t key_note[PANEL_NOTE_KEYS];
    int16_t master_raw;
    int16_t master_shown;
    uint8_t master_follow;
    uint8_t master_wait;
    uint8_t master_pending;
    uint32_t held;
    uint8_t page;
    uint8_t playing;
    uint8_t glo;
    uint32_t quiet;
    uint32_t sounding;
    void (*master_hook)(void* ctx);
    void* master_ctx;
    uint8_t glo_latch;
} panel;

typedef struct {
    uint32_t buttons;
    uint32_t notes;
    uint32_t buttons_dim;
    uint32_t notes_dim;
} panel_leds;

#define PANEL_BLINK_HALF_MS 180u
#define PANEL_BEAT_LIT 64u
#define PANEL_BEAT_NONE 0xffu
#define PANEL_ARP_FAST_FRAMES 2u
#define PANEL_FLICK_MS 50u
#define PANEL_STEPS 16u
#define PANEL_NO_STEP 0xffu
#define PANEL_REC_OFF 0u
#define PANEL_REC_ARMED 1u
#define PANEL_REC_COUNT 2u
#define PANEL_REC_ON 3u

typedef struct {
    uint32_t ms;
    uint8_t beat;
    uint8_t rec;
    uint8_t silent;
    uint8_t solo;
    uint8_t ask;
    uint8_t step_on;
    uint8_t step_head;
    uint16_t step_set;
    uint16_t step_rows;
    uint32_t head_ms;
} panel_led_in;

void panel_init(panel* p);
void panel_note_keys(panel* p, live* l, uint32_t down, uint32_t pressed);
uint32_t panel_take_glo(panel* p);
void panel_quiet_keys(panel* p, live* l, uint32_t mask);
void panel_buttons(panel* p, live* l, uint32_t pressed, uint32_t held);
uint32_t panel_knob_param(const panel* p, const live* l, uint32_t held, uint32_t enc);
uint32_t panel_lab_param(const live* l, uint32_t page, uint32_t k);
void panel_encoder(panel* p, live* l, uint32_t enc, int32_t steps);
void panel_master(panel* p, live* l, int32_t raw);
panel_leds panel_get_leds(const panel* p, const live* l, const panel_led_in* in);

#ifdef __cplusplus
}
#endif

#endif
