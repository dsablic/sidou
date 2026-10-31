// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
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

int residfp_set_filter6581_range(SID* s, double range) {
    try {
        s->setFilter6581Range(range);
        return 0;
    } catch (...) {
        return -1;
    }
}

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
