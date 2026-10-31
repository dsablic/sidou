// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_MIDI_H
#define SIDOU_FM1_MIDI_H

#include <stdint.h>

#include "grid.h"
#include "keys_route.h"
#include "live.h"
#include "midi_learn.h"
#include "midi_route.h"
#include "midi_sync.h"
#include "sysex.h"

void midi_init(live* l, keys_route* kr);
midi_route* midi_routed(void);
void midi_tick(void);
midi_sync* midi_synced(void);
void midi_sync_pass(void);
void midi_clock_held(uint32_t from, uint32_t to);
midi_learn* midi_learned(void);
void midi_usb_isr(void);
void midi_uart_isr(void);
int midi_drain(void);
void midi_send(void);
void midi_service(uint32_t now_ms);
int midi_usb_live(void);
uint32_t midi_inputs(void);
void midi_sysex_bind(const sysex_hooks* hooks, const volatile uint32_t* now_ms);
void midi_sysex_service(uint32_t now_ms, int loud);
int midi_usb_recording(void);
extern uint8_t midi_link_active;
void midi_link_mark(uint32_t row, uint32_t col, uint32_t len);
int midi_link_on(void);
void midi_link_run(const grid* g, uint32_t now_ms);
uint32_t midi_link_buttons(uint32_t* pressed);
int32_t midi_link_steps(uint32_t enc);
extern uint8_t midi_sysex_running;
extern uint8_t midi_sysex_writing;

#if defined(SIDOU_FLASH_SPIKE)
void midi_spike_trs(uint32_t* bytes, uint32_t* lost);
#endif

#endif
