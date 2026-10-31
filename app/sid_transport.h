// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_TRANSPORT_H
#define SIDOU_SID_TRANSPORT_H

#include <stdint.h>

#include "psid/psid_file.h"
#include "store.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_T_NONE 0u
#define SID_T_LOADING 1u
#define SID_T_PLAYING 2u
#define SID_T_PAUSED 3u
#define SID_T_STOPPED 4u
#define SID_T_FAILED 5u
#define SID_T_ENDING 6u

#define SID_LIFT_OFF 0u
#define SID_LIFT_ARMED 1u
#define SID_LIFT_TAKING 2u
#define SID_OFFER_NONE 0u
#define SID_OFFER_READY 1u
#define SID_OFFER_IMPORTING 2u
#define SID_OFFER_NAME 16u

typedef struct {
    uint8_t state;
    uint8_t chips;
    uint8_t shadow;
    uint16_t song;
    uint32_t seconds;
    uint32_t cycles;
    uint16_t load;
    uint16_t ticks_k;
    uint32_t overruns;
    uint32_t late;
    uint32_t dropped;
    uint8_t env[3];
    uint8_t model[2];
    char name[STORE_NAME_MAX + 1u];
    const psid_header* h;
    uint8_t lift;
    uint8_t lift_voice;
    uint8_t offer;
    uint8_t offer_instr;
    uint8_t offer_playing;
    char offer_name[SID_OFFER_NAME + 1u];
} sid_status;

typedef struct {
    int (*play)(void* ctx, const char* name);
    int (*start)(void* ctx, uint32_t song);
    int (*stop)(void* ctx);
    int (*pause)(void* ctx);
    int (*resume)(void* ctx);
    int (*end)(void* ctx);
    int (*mute)(void* ctx, uint32_t mask);
    void (*status)(void* ctx, sid_status* out);
    int (*lift)(void* ctx, uint32_t voice);
    int (*import)(void* ctx, int go);
    void* ctx;
} sid_transport;

#ifdef __cplusplus
}
#endif

#endif
