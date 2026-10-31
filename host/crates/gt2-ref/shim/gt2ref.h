/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef GT2REF_H
#define GT2REF_H

#include <stddef.h>

typedef struct {
    unsigned hr_adsr;
    unsigned multiplier;
    unsigned ntsc;
    unsigned custom_clock;
    float a4_hz;
    unsigned finevibrato;
    unsigned optimize_pulse;
    unsigned optimize_realtime;
    unsigned player_version;
    unsigned sid_address;
    unsigned player_address;
    unsigned zeropage;
    unsigned sid_model;
} gt2ref_options;

void gt2ref_default_options(gt2ref_options* o);

int gt2ref_pack(const unsigned char* sng, size_t len, const gt2ref_options* o,
                unsigned char** prg, size_t* prg_len);

int gt2ref_canonical(const unsigned char* sng, size_t len, unsigned char** out,
                     size_t* out_len);

void gt2ref_free(void* p);

#endif
