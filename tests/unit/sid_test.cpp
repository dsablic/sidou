// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <gtest/gtest.h>

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <vector>

extern "C" {
#include "sid/sid.h"
}

namespace {

std::vector<int16_t> render(sid* s, uint32_t n) {
    std::vector<int16_t> out(n);
    sid_render(s, out.data(), n);
    return out;
}

int peak(const std::vector<int16_t>& v) {
    int p = 0;
    for (int16_t x : v) {
        p = std::max(p, std::abs((int)x));
    }
    return p;
}

void saw_a4(sid* s) {
    sid_write(s, 0x00, 0x45); // 440 Hz at PAL: 0x1d45
    sid_write(s, 0x01, 0x1d);
    sid_write(s, 0x05, 0x00);
    sid_write(s, 0x06, 0xf0);
    sid_write(s, 0x18, 0x0f);
    sid_write(s, 0x04, 0x21);
}

} // namespace

TEST(Sid, CyclesPerSampleIsPalClockOverSampleRate) {
    static sid s;
    sid_init(&s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
    EXPECT_EQ(s.cycles_per_sample_q16, (uint32_t)(((uint64_t)SID_CLOCK_PAL << 16) / 44100));
}

TEST(Sid, An8580IsSilentAfterInit) {
    static sid s;
    sid_init(&s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
    EXPECT_EQ(peak(render(&s, 4410)), 0);
}

TEST(Sid, GatedSawProducesUnclippedSound) {
    static sid s;
    sid_init(&s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
    saw_a4(&s);
    auto out = render(&s, 4410);
    EXPECT_GT(peak(out), 1000);
    EXPECT_LT(peak(out), 32767);
}

TEST(Sid, VolumeWriteOnA6581ClicksThenSettles) {
    static sid s;
    sid_init(&s, SID_MODEL_6581, SID_CLOCK_PAL, 44100);
    sid_write(&s, 0x18, 0x0f);
    auto click = render(&s, 10);
    EXPECT_GT(peak(click), 1000);
    render(&s, 44100);
    EXPECT_LT(peak(render(&s, 100)), 50);
}

TEST(Sid, Voice3OffMutesAnUnfilteredVoice3) {
    static sid s;
    sid_init(&s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
    sid_write(&s, 0x0e, 0x45);
    sid_write(&s, 0x0f, 0x1d);
    sid_write(&s, 0x14, 0xf0);
    sid_write(&s, 0x18, 0x8f);
    sid_write(&s, 0x12, 0x21);
    EXPECT_EQ(peak(render(&s, 4410)), 0);
}

TEST(Sid, LowPassFilterRemovesTopEnd) {
    static sid a, b;
    for (sid* s : {&a, &b}) {
        sid_init(s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
        saw_a4(s);
    }
    sid_write(&b, 0x16, 0x10); // fc ~ 128 -> 800 Hz
    sid_write(&b, 0x17, 0x01); // voice 1 through the filter
    sid_write(&b, 0x18, 0x1f); // low-pass, volume 15
    auto dry = render(&a, 4410), wet = render(&b, 4410);
    auto hf_energy = [](const std::vector<int16_t>& v) {
        double e = 0;
        for (size_t i = 1; i < v.size(); ++i) {
            double d = (double)v[i] - v[i - 1];
            e += d * d;
        }
        return e;
    };
    EXPECT_LT(hf_energy(wet), hf_energy(dry) * 0.1);
}

namespace {

void expect_dc_blocker_never_subnormal(sid* s, int seconds) {
    std::vector<int16_t> block(4410);
    for (int i = 0; i < seconds * 10; ++i) {
        sid_render(s, block.data(), (uint32_t)block.size());
        ASSERT_NE(std::fpclassify(s->dc_x1), FP_SUBNORMAL) << "block " << i;
        ASSERT_NE(std::fpclassify(s->dc_y1), FP_SUBNORMAL) << "block " << i;
    }
}

} // namespace

TEST(Sid, DcBlockerOnA6581VolumeStepNeverGoesSubnormal) {
    static sid s;
    sid_init(&s, SID_MODEL_6581, SID_CLOCK_PAL, 44100);
    sid_write(&s, 0x18, 0x0f);
    expect_dc_blocker_never_subnormal(&s, 20);
}

TEST(Sid, DcBlockerAfterAn8580NoteNeverGoesSubnormal) {
    static sid s;
    sid_init(&s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
    saw_a4(&s);
    render(&s, 4410);
    sid_write(&s, 0x04, 0x20);
    expect_dc_blocker_never_subnormal(&s, 20);
}

TEST(Sid, Voice3OffKeepsAFilteredVoice3) {
    static sid s;
    sid_init(&s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
    sid_write(&s, 0x0e, 0x45);
    sid_write(&s, 0x0f, 0x1d);
    sid_write(&s, 0x14, 0xf0);
    sid_write(&s, 0x16, 0xff);
    sid_write(&s, 0x17, 0x04);
    sid_write(&s, 0x18, 0x9f);
    sid_write(&s, 0x12, 0x21);
    EXPECT_GT(peak(render(&s, 4410)), 1000);
}

TEST(Sid, CutoffLowBitsComeFromRegister15) {
    static sid a, b;
    for (sid* s : {&a, &b}) {
        sid_init(s, SID_MODEL_8580, SID_CLOCK_PAL, 44100);
        saw_a4(s);
        sid_write(s, 0x16, 0x02);
        sid_write(s, 0x17, 0x01);
        sid_write(s, 0x18, 0x1f);
    }
    sid_write(&b, 0x15, 0x07);
    EXPECT_EQ(a.fc, 0x010u);
    EXPECT_EQ(b.fc, 0x017u);
    EXPECT_NE(render(&a, 4410), render(&b, 4410));
}
