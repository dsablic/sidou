// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// extern "C" face of reSIDfp for Rust. No exception crosses it: failures return -1.
#include <cstdint>
#include <new>

#include "SID.h"

using reSIDfp::SID;

extern "C" {

SID* residfp_new(void) { return new (std::nothrow) SID(); }

void residfp_free(SID* s) { delete s; }

int residfp_set_model(SID* s, int csg8580) {
    try {
        s->setChipModel(csg8580 ? reSIDfp::CSG8580 : reSIDfp::MOS6581);
        return 0;
    } catch (...) {
        return -1;
    }
}

int residfp_set_sampling(SID* s, double clock_hz, int decimate, double sample_rate) {
    try {
        s->setSamplingParameters(clock_hz, decimate ? reSIDfp::DECIMATE : reSIDfp::RESAMPLE,
                                 sample_rate);
        return 0;
    } catch (...) {
        return -1;
    }
}

void residfp_reset(SID* s) { s->reset(); }

void residfp_write(SID* s, uint8_t reg, uint8_t value) { s->write(reg, value); }

uint8_t residfp_read(SID* s, uint8_t reg) { return s->read(reg); }

int residfp_clock(SID* s, unsigned cycles, int16_t* buf) { return s->clock(cycles, buf); }
}
