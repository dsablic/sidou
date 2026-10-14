// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

#include <cmath>
#include <cstdlib>
#include <fstream>
#include <iomanip>
#include <map>
#include <sstream>
#include <string>
#include <vector>

extern "C" {
#include "workload.h"
}

namespace {

struct Case {
    std::string name;
    uint32_t chips;
    sid_model model;
};

const Case kCases[] = {
    {"workload_6581_1chip", 1, SID_MODEL_6581},
    {"workload_8580_1chip", 1, SID_MODEL_8580},
    {"workload_6581_2chip", 2, SID_MODEL_6581},
};

std::vector<int16_t> render(const Case& c) {
    std::vector<int16_t> out(workload_sample_count());
    workload_begin(c.chips, c.model);
    workload_render_samples(out.data(), (uint32_t)out.size());
    return out;
}

uint64_t fnv1a(const std::vector<int16_t>& samples) {
    uint64_t h = 0xcbf29ce484222325ull;
    for (int16_t s : samples) {
        uint16_t u = (uint16_t)s;
        for (int byte = 0; byte < 2; ++byte) {
            h ^= (u >> (8 * byte)) & 0xffu;
            h *= 0x100000001b3ull;
        }
    }
    return h;
}

std::map<std::string, std::string> load(const char* path) {
    std::map<std::string, std::string> hashes;
    std::ifstream in(path);
    std::string name, hash;
    while (in >> name >> hash) {
        hashes[name] = hash;
    }
    return hashes;
}

std::string hex(uint64_t v) {
    std::ostringstream s;
    s << std::hex << std::setw(16) << std::setfill('0') << v;
    return s.str();
}

bool updating() {
    const char* v = std::getenv("SIDOU_GOLDEN_UPDATE");
    return v && std::string(v) == "1";
}

} // namespace

TEST(Golden, RendersMatchRecordedHashes) {
    if (updating()) {
        std::ofstream out(SIDOU_GOLDEN_FILE);
        for (const Case& c : kCases) {
            out << c.name << ' ' << hex(fnv1a(render(c))) << '\n';
        }
        out.close();
        if (!out) {
            FAIL() << "could not write " << SIDOU_GOLDEN_FILE;
        }
        GTEST_SKIP() << "rewrote " << SIDOU_GOLDEN_FILE;
    }
    auto expected = load(SIDOU_GOLDEN_FILE);
    for (const Case& c : kCases) {
        ASSERT_TRUE(expected.count(c.name))
            << c.name << " missing; run with SIDOU_GOLDEN_UPDATE=1 and review the diff";
        EXPECT_EQ(hex(fnv1a(render(c))), expected[c.name])
            << c.name << " changed. If intended, rerun with SIDOU_GOLDEN_UPDATE=1";
    }
}

TEST(Golden, WorkloadNeverClipsAndHasNoDcOffset) {
    // The 6581's volume write at frame 0 is a real DC step; the 16 Hz blocker needs
    // ~50 ms to absorb it, so the mean is taken after that.
    const size_t settle = 2205;
    for (const Case& c : kCases) {
        auto out = render(c);
        double sum = 0;
        for (size_t i = 0; i < out.size(); ++i) {
            ASSERT_GT(out[i], -32768) << c.name;
            ASSERT_LT(out[i], 32767) << c.name;
            if (i >= settle) {
                sum += out[i];
            }
        }
        EXPECT_LT(std::fabs(sum / (double)(out.size() - settle)), 200.0) << c.name;
    }
}
