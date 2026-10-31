// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_MIDI_MAP_H
#define SIDOU_MIDI_MAP_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

#define MIDI_MAP_NO_CC 0xffu
#define MIDI_MAP_NO_NRPN 0xffffu
#define MIDI_MAP_RESERVED 0xffu
#define MIDI_MAP_SPREAD 0xfeu
#define MIDI_MAP_CC_SPREAD 117u
#define MIDI_MAP_NRPN(msb, lsb) ((uint16_t)((msb) << 7 | (lsb)))
#define MIDI_MAP_NULL 127u

typedef enum {
    MIDI_CURVE_LINEAR = 0,
    MIDI_CURVE_STEPS = 1,
    MIDI_CURVE_WALK = 2
} midi_map_curve;

typedef struct {
#ifdef SIDOU_MIDI_MAP_DOC
    const char* group;
    const char* name;
    const char* values;
#endif
    uint16_t nrpn;
    uint16_t max;
    uint16_t def;
    uint8_t param;
    uint8_t cc;
    uint8_t curve;
} midi_map_entry;

enum { MIDI_MAP_ROWS = 43u + SID_EXPERT_COUNT + LIVE_PARAM_TAIL };

extern const midi_map_entry midi_map_rows[MIDI_MAP_ROWS];
const midi_map_entry* midi_map_row(uint32_t i);
const midi_map_entry* midi_map_for_param(uint32_t param);
const midi_map_entry* midi_map_for_cc(uint8_t cc);
const midi_map_entry* midi_map_for_nrpn(uint16_t nrpn);
uint32_t midi_map_from_cc(const live* l, const midi_map_entry* e, uint8_t value);
uint8_t midi_map_to_cc(const live* l, const midi_map_entry* e, uint32_t value);
void midi_map_cc_set(live* l, const midi_map_entry* e, uint8_t value);

uint32_t midi_map_param_get(const live* l, uint32_t param);
void midi_map_param_set(live* l, uint32_t param, uint32_t value);
uint32_t midi_map_param_max(uint32_t param);

typedef struct {
    uint8_t nrpn_msb;
    uint8_t nrpn_lsb;
    uint8_t rpn_msb;
    uint8_t rpn_lsb;
    uint8_t rpn;
    uint8_t data_msb;
} midi_map_channel;

#define MIDI_MAP_NO_DATA 0xffu

struct midi_learn;

typedef struct {
    live* l;
    midi_map_channel ch[16];
    struct midi_learn* learn;
    uint8_t (*banks)[2];
} midi_map_port;

void midi_map_port_init(midi_map_port* p, live* l);
void midi_map_cc(midi_map_port* p, uint8_t channel, uint8_t cc, uint8_t value);

#ifdef __cplusplus
}
#endif

#endif
