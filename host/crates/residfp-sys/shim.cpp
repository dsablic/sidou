// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// extern "C" face of reSIDfp for Rust. Every entry point that calls into reSIDfp catches
// all exceptions, so none crosses into Rust: on failure residfp_new returns nullptr and
// the others return -1. residfp_free only runs a destructor, which cannot throw.
#include <cstdint>

#include "SID.h"

using reSIDfp::SID;

extern "C" {

SID* residfp_new(void) {
    try {
        return new SID();
    } catch (...) {
        return nullptr;
    }
}

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

int residfp_reset(SID* s) {
    try {
        s->reset();
        return 0;
    } catch (...) {
        return -1;
    }
}

int residfp_write(SID* s, uint8_t reg, uint8_t value) {
    try {
        s->write(reg, value);
        return 0;
    } catch (...) {
        return -1;
    }
}

int residfp_read(SID* s, uint8_t reg) {
    try {
        return s->read(reg);
    } catch (...) {
        return -1;
    }
}

int residfp_clock(SID* s, unsigned cycles, int16_t* buf) {
    try {
        return s->clock(cycles, buf);
    } catch (...) {
        return -1;
    }
}
}
