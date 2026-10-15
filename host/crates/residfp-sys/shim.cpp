// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// extern "C" face of reSIDfp for Rust. Every entry point that calls into reSIDfp catches
// all exceptions, so none crosses into Rust: on failure residfp_new returns nullptr and
// the others return -1. residfp_free only runs a destructor, which cannot throw.
#include <cstdint>

#include "Dac.h"
#include "SID.h"
#include "WaveformCalculator.h"

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

// reSIDfp's combined-waveform pulldown tables for a chip model and strength (AVERAGE,
// WEAK, STRONG = 0, 1, 2): 5 rows of 4096 entries (ST, PT, PS, PST, NP), row-major.
int residfp_pulldown_table(int csg8580, int cws, int16_t* out) {
    try {
        const auto model = csg8580 ? reSIDfp::CSG8580 : reSIDfp::MOS6581;
        const auto strength = static_cast<reSIDfp::CombinedWaveforms>(cws);
        const auto table = reSIDfp::WaveformCalculator::buildPulldownTable(model, strength);
        for (unsigned row = 0; row < 5; ++row) {
            for (unsigned i = 0; i < 4096; ++i) {
                out[row * 4096 + i] = (*table)[row][i];
            }
        }
        return 0;
    } catch (...) {
        return -1;
    }
}

// reSIDfp's waveform table: 4 rows of 4096 (none, triangle, sawtooth, ST before pulldown).
int residfp_wave_table(int16_t* out) {
    try {
        const auto table = reSIDfp::WaveformCalculator::getInstance()->getWaveTable();
        for (unsigned row = 0; row < 4; ++row) {
            for (unsigned i = 0; i < 4096; ++i) {
                out[row * 4096 + i] = (*table)[row][i];
            }
        }
        return 0;
    } catch (...) {
        return -1;
    }
}

// reSIDfp's DAC as SID::setChipModel builds it: Dac(bits).kinkedDac(model, leak_level), then
// getOutput(i, saturate) for every input i < 2^bits (the waveform DAC saturates on the
// 6581, the envelope DAC never).
int residfp_dac_table(int csg8580, unsigned bits, double leak_level, int saturate,
                      double* out) {
    try {
        reSIDfp::Dac dac(bits);
        dac.kinkedDac(csg8580 ? reSIDfp::CSG8580 : reSIDfp::MOS6581, leak_level);
        for (unsigned i = 0; i < (1u << bits); ++i) {
            out[i] = dac.getOutput(i, saturate != 0);
        }
        return 0;
    } catch (...) {
        return -1;
    }
}

int residfp_clock_silent(SID* s, unsigned cycles) {
    try {
        s->clockSilent(cycles);
        return 0;
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
