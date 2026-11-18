// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_PUNCH_ECHO_H
#define SIDOU_PUNCH_ECHO_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

#define PUNCH_ECHO_FEEDBACK 96u
#define PUNCH_ECHO_MIX 72u
#define PUNCH_ECHO_TAIL_MS 4000u

enum { PUNCH_ECHO_OFF, PUNCH_ECHO_THROWING, PUNCH_ECHO_TAIL };

typedef struct {
    live* l;
    uint8_t state;
    uint32_t until;
} punch_echo;

void punch_echo_init(punch_echo* e, live* l);
void punch_echo_hold(punch_echo* e, int on, uint32_t now_ms);
void punch_echo_tick(punch_echo* e, uint32_t now_ms);

#ifdef __cplusplus
}
#endif

#endif
