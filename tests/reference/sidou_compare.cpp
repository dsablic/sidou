// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <cstdio>
#include <string>

#include "render.h"
#include "signal_metrics.h"

namespace {

struct Scenario {
    const char* name;
    std::vector<RegWrite> writes;
    uint32_t frames;
};

std::vector<RegWrite> noise_sweep(uint8_t mode) {
    std::vector<RegWrite> w = {{0, 0x18, (uint8_t)(mode | 0x0f)}, {0, 0x17, 0x81},
                               {0, 0x00, 0x00}, {0, 0x01, 0x40},
                               {0, 0x05, 0x00}, {0, 0x06, 0xf0}, {0, 0x04, 0x81}};
    for (uint32_t f = 0; f < 64; ++f) {
        w.push_back({f, 0x16, (uint8_t)(f * 4)});
    }
    return w;
}

} // namespace

int main(int argc, char** argv) {
    const std::string dir = argc > 1 ? argv[1] : ".";
    const Scenario scenarios[] = {
        {"lp_noise_sweep", noise_sweep(0x10), 64},
        {"bp_noise_sweep", noise_sweep(0x20), 64},
        {"hp_noise_sweep", noise_sweep(0x40), 64},
    };
    std::printf("%-16s %-8s %s\n", "scenario", "model", "max envelope diff (dB)");
    for (const auto& s : scenarios) {
        for (sid_model m : {SID_MODEL_6581, SID_MODEL_8580}) {
            const char* tag = m == SID_MODEL_6581 ? "6581" : "8580";
            auto ours = render_sidou(m, s.writes, s.frames);
            auto ref = render_residfp(m, s.writes, s.frames);
            write_wav16(dir + "/" + s.name + "_" + tag + "_sidou.wav", ours, 44100);
            write_wav16(dir + "/" + s.name + "_" + tag + "_residfp.wav", ref, 44100);
            double d = max_envelope_diff_db(normalized(rms_envelope_db(ours, 44100, 20), SIZE_MAX),
                                            normalized(rms_envelope_db(ref, 44100, 20), SIZE_MAX), -40, 3);
            std::printf("%-16s %-8s %.1f\n", s.name, tag, d);
        }
    }
    return 0;
}
