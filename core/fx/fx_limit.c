// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "fx_limit.h"

#define K ((float)SID_FX_LIMIT_KNEE)
#define C ((float)SID_FX_LIMIT_CEILING)
#define H (C - K)

int16_t sid_fx_limit_round(float x) {
    float m = x >= 0.0f ? x : -x;
    float o = C - H * H / (m - K + H) + 0.5f;
    return (int16_t)(x >= 0.0f ? o : -o);
}
