// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_THUMP_OUT_H
#define SIDOU_THUMP_OUT_H

#include <stdint.h>

#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

#define THUMP_OUT_CEILING 32766

typedef struct {
    float level[2];
    float r[2];
    float late[2];
    uint32_t late_at[2];
} thump_out;

void thump_out_take(thump_out* t, uint32_t chip, sid* s);

void thump_out_drop(sid* s);

void thump_out_apply(thump_out* t, int16_t* lr, uint32_t frames);

#ifdef __cplusplus
}
#endif

#endif
