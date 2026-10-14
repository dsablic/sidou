// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "render.h"

#include <memory>

#include "residfp_engine.h"

extern "C" {
#include "sid/sid.h"
}

namespace {

constexpr uint32_t kSamplesPerFrame = 882;
constexpr unsigned kCyclesPerFrame = 19705; // 985248 / 50, rounded

template <typename Write>
size_t apply(const std::vector<RegWrite>& writes, size_t next, uint32_t frame, Write write) {
    while (next < writes.size() && writes[next].frame <= frame) {
        write(writes[next].reg, writes[next].value);
        ++next;
    }
    return next;
}

std::vector<float> to_float(const std::vector<int16_t>& in) {
    std::vector<float> out(in.size());
    for (size_t i = 0; i < in.size(); ++i) {
        out[i] = (float)in[i] / 32768.0f;
    }
    return out;
}

} // namespace

std::vector<float> render_sidou(sid_model model, const std::vector<RegWrite>& writes,
                                uint32_t frames) {
    auto s = std::make_unique<sid>();
    sid_init(s.get(), model, SID_CLOCK_PAL, (uint32_t)kRefSampleRate);
    std::vector<int16_t> out(frames * kSamplesPerFrame);
    size_t next = 0;
    for (uint32_t f = 0; f < frames; ++f) {
        next = apply(writes, next, f, [&](uint8_t r, uint8_t v) { sid_write(s.get(), r, v); });
        sid_render(s.get(), out.data() + f * kSamplesPerFrame, kSamplesPerFrame);
    }
    return to_float(out);
}

std::vector<float> render_residfp(sid_model model, const std::vector<RegWrite>& writes,
                                  uint32_t frames, bool decimate) {
    ResidfpEngine e(model, kRefSampleRate, decimate);
    std::vector<int16_t> out;
    size_t next = 0;
    for (uint32_t f = 0; f < frames; ++f) {
        next = apply(writes, next, f, [&](uint8_t r, uint8_t v) { e.write(r, v); });
        e.render_cycles(kCyclesPerFrame, out);
    }
    return to_float(out);
}
