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

/* The true output rate of reSIDfp's DECIMATE mode. ZeroOrderResampler steps by
 * cyclesPerSample = int(clock / fs * 1024) in 1/1024 cycles, so the rate is
 * clock * 1024 / that (PAL at 44100 Hz: 22877 instead of 22877.41, ~44100.80 Hz). */
double residfp_decimate_rate(double sample_rate);

class ResidfpEngine {
public:
    /* decimate: reSIDfp's DECIMATE mode, its ZeroOrderResampler (linear
     * interpolation). Its rate is off only by integer rounding, ~0.0018% fast (see
     * residfp_decimate_rate). The default RESAMPLE (two-pass sinc) sounds better but
     * runs ~0.02% fast (measured: 44110 samples per 44100 expected). */
    ResidfpEngine(sid_model model, double sample_rate, bool decimate);
    ~ResidfpEngine();
    void write(uint8_t reg, uint8_t value);
    void render_cycles(unsigned cycles, std::vector<int16_t>& out);

private:
    std::unique_ptr<reSIDfp::SID> sid_;
    double sample_rate_;
};
