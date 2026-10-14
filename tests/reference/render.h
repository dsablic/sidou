// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <cstdint>
#include <vector>

extern "C" {
#include "sid/sid_defs.h"
}

struct RegWrite {
    uint32_t frame;
    uint8_t reg;
    uint8_t value;
};

constexpr double kRefSampleRate = 44100.0;

/* Both renderers apply writes at PAL frame (50 Hz) boundaries and return
 * float samples in [-1, 1]. */
std::vector<float> render_sidou(sid_model model, const std::vector<RegWrite>& writes,
                                uint32_t frames);
/* decimate: see ResidfpEngine. Use it when timing or pitch is measured; the
 * true output rate is then residfp_decimate_rate(kRefSampleRate). */
std::vector<float> render_residfp(sid_model model, const std::vector<RegWrite>& writes,
                                  uint32_t frames, bool decimate = false);
