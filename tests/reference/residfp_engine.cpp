// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "residfp_engine.h"

#include "SID.h"

double residfp_decimate_rate(double sample_rate) {
    const int cycles_per_sample = (int)(SID_CLOCK_PAL / sample_rate * 1024.0);
    return SID_CLOCK_PAL * 1024.0 / cycles_per_sample;
}

ResidfpEngine::ResidfpEngine(sid_model model, double sample_rate, bool decimate)
    : sid_(std::make_unique<reSIDfp::SID>()), sample_rate_(sample_rate) {
    sid_->setChipModel(model == SID_MODEL_6581 ? reSIDfp::MOS6581 : reSIDfp::CSG8580);
    sid_->setSamplingParameters(SID_CLOCK_PAL, decimate ? reSIDfp::DECIMATE : reSIDfp::RESAMPLE,
                               sample_rate);
    sid_->reset();
}

ResidfpEngine::~ResidfpEngine() = default;

void ResidfpEngine::write(uint8_t reg, uint8_t value) { sid_->write(reg, value); }

void ResidfpEngine::render_cycles(unsigned cycles, std::vector<int16_t>& out) {
    std::vector<int16_t> buf((size_t)(cycles * sample_rate_ / SID_CLOCK_PAL) + 64);
    int n = sid_->clock(cycles, buf.data());
    out.insert(out.end(), buf.begin(), buf.begin() + n);
}
