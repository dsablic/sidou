// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_INPUT_H
#define SIDOU_FM1_INPUT_H

#include <stdint.h>

#include "live.h"
#include "panel.h"

#define INPUT_RAW_NOTES (1u << 31)
uint32_t input_raw_keys(void);
void input_raw_dark(void);
void input_init(void);
void input_tick_isr(void);
uint32_t input_frame(void);
#define INPUT_FRAME_US 1100u
#define INPUT_PRESS_FRAMES 2u
void input_poll(panel* p, live* l);
uint32_t input_buttons(void);
uint32_t input_buttons_at_power_on(void);
int32_t input_master(void);
void input_leds_dark(void);
void input_leds(const panel* p, const live* l, uint32_t now_ms);

#endif
