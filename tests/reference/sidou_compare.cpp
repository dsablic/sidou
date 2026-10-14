// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <cstdint>
#include <cstdio>
#include <string>
#include <utility>

#include "render.h"
#include "signal_metrics.h"

namespace {

constexpr size_t kWindow = 882; /* 20 ms; noise has no period to align to */
/* Windows 0-2 (the first 60 ms) are not compared; window 0 holds the click of
 * the 0 -> 15 volume step and is the loudest of all. Each curve is normalised
 * to its loudest *compared* window, so the baseline tracks the filtered noise,
 * not the click; the -40 dB floor stays relative to that peak. */
constexpr size_t kFirstWindow = 3;
constexpr double kFloorDb = -40.0;

std::vector<double> envelope(const std::vector<float>& x) {
    auto env = rms_envelope_db(x, kWindow);
    return normalized_to_max(env, kFirstWindow, env.size());
}

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
            const std::string stem = dir + "/" + s.name + "_" + tag;
            for (const auto& [path, x] : {std::pair{stem + "_sidou.wav", &ours},
                                          std::pair{stem + "_residfp.wav", &ref}}) {
                if (!write_wav16(path, *x, 44100)) {
                    std::fprintf(stderr, "cannot write %s\n", path.c_str());
                    return 1;
                }
            }
            double d = max_envelope_diff_db(envelope(ours), envelope(ref), kFloorDb, kFirstWindow);
            std::printf("%-16s %-8s %.1f\n", s.name, tag, d);
        }
    }
    return 0;
}
