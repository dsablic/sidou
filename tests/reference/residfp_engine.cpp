// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "residfp_engine.h"

#include <stdexcept>

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
    release_power_on_envelopes();
}

/* reSIDfp, like the real chip, powers up each envelope counter at 0xaa and keeps
 * it across reset; our core starts at 0. Release all three to 0 so both engines
 * start the scripted writes from the same state (M1 decides whether our core
 * should mimic 0xaa). With AD = SR = 0 the release from 0xaa takes 671 rate
 * periods of 9 cycles (~7 ms measured); 50 ms is plenty. clockSilent() would only clock
 * voice 3's envelope, so this clocks into a discarded buffer. Every register
 * written here ends up 0, the same as after reset. */
void ResidfpEngine::release_power_on_envelopes() {
    for (uint8_t v = 0; v < 3; ++v) {
        sid_->write((uint8_t)(v * 7 + 4), 0x00);
        sid_->write((uint8_t)(v * 7 + 5), 0x00);
        sid_->write((uint8_t)(v * 7 + 6), 0x00);
    }
    std::vector<int16_t> discard;
    render_cycles(SID_CLOCK_PAL / 20, discard);
    /* ENV3 is the only readable counter; voices 1 and 2 ran the same release. */
    if (sid_->read(0x1c) != 0) {
        throw std::logic_error("reSIDfp envelope did not release to 0");
    }
}

ResidfpEngine::~ResidfpEngine() = default;

void ResidfpEngine::write(uint8_t reg, uint8_t value) { sid_->write(reg, value); }

void ResidfpEngine::render_cycles(unsigned cycles, std::vector<int16_t>& out) {
    std::vector<int16_t> buf((size_t)(cycles * sample_rate_ / SID_CLOCK_PAL) + 64);
    int n = sid_->clock(cycles, buf.data());
    out.insert(out.end(), buf.begin(), buf.begin() + n);
}
