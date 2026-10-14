// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <cstdint>
#include <memory>
#include <vector>

extern "C" {
#include "sid/sid_defs.h"
}

namespace reSIDfp {
class SID;
}

class ResidfpEngine {
public:
    /* exact_rate: reSIDfp's DECIMATE (linear interpolation) keeps the exact output
     * rate; its RESAMPLE (two-pass sinc) sounds better but its intermediate rate
     * makes it run ~0.02% fast (measured: 44110 samples per 44100 expected). */
    ResidfpEngine(sid_model model, double sample_rate, bool exact_rate);
    ~ResidfpEngine();
    void write(uint8_t reg, uint8_t value);
    void render_cycles(unsigned cycles, std::vector<int16_t>& out);

private:
    std::unique_ptr<reSIDfp::SID> sid_;
    double sample_rate_;
};
