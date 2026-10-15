// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// extern "C" face of reSIDfp for Rust. Every entry point that calls into reSIDfp catches
// all exceptions, so none crosses into Rust: on failure residfp_new returns nullptr and
// the others return -1. residfp_free only runs a destructor, which cannot throw.
#include <cstdint>

#include "Dac.h"
#include "FilterModelConfig6581.h"
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

// The filter's curve setting (SID::setFilter6581Curve or setFilter8580Curve, 0..1, reSIDfp's
// default 0.5), for the chip's own model.
int residfp_set_filter_curve(SID* s, int csg8580, double curve) {
    try {
        if (csg8580) {
            s->setFilter8580Curve(curve);
        } else {
            s->setFilter6581Curve(curve);
        }
        return 0;
    } catch (...) {
        return -1;
    }
}

// The 6581 filter's range (SID::setFilter6581Range, 0..1). It sets the VCR's uCox in the
// 6581 filter model, which reSIDfp keeps once per process: every 6581 chip follows it.
int residfp_set_filter6581_range(SID* s, double range) {
    try {
        s->setFilter6581Range(range);
        return 0;
    } catch (...) {
        return -1;
    }
}

// The 6581's cutoff DAC as Filter6581 uses it: FilterModelConfig6581::getDAC(curve), the
// normalised VCR gate voltage Vw for each of the 2048 FC values.
int residfp_filter6581_dac(double curve, uint16_t* out) {
    try {
        uint16_t* dac = reSIDfp::FilterModelConfig6581::getInstance()->getDAC(curve);
        for (unsigned i = 0; i < 2048; ++i) {
            out[i] = dac[i];
        }
        delete[] dac;
        return 0;
    } catch (...) {
        return -1;
    }
}

// The combined-waveform strength (SID::setCombinedWaveforms; AVERAGE, WEAK, STRONG = 0, 1,
// 2): it rebuilds the chip's pulldown tables for its model.
int residfp_set_combined_waveforms(SID* s, int cws) {
    try {
        s->setCombinedWaveforms(static_cast<reSIDfp::CombinedWaveforms>(cws));
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
