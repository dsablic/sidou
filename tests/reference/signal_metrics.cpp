// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "signal_metrics.h"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <fstream>

namespace {

double dft_magnitude(const std::vector<float>& x, const std::vector<double>& window, double f,
                     double fs) {
    const double w = 2.0 * M_PI * f / fs;
    double re = 0, im = 0;
    for (size_t i = 0; i < x.size(); ++i) {
        re += window[i] * x[i] * std::cos(w * (double)i);
        im -= window[i] * x[i] * std::sin(w * (double)i);
    }
    return std::sqrt(re * re + im * im);
}

} // namespace

double estimate_f0(const std::vector<float>& x, double fs, double f_lo, double f_hi) {
    std::vector<double> window(x.size());
    for (size_t i = 0; i < x.size(); ++i) {
        window[i] = 0.5 - 0.5 * std::cos(2.0 * M_PI * (double)i / (double)(x.size() - 1));
    }
    // Coarse scan, then golden-section search around the best bin.
    const double step = fs / (double)x.size() / 4.0;
    double best_f = f_lo, best = -1;
    for (double f = f_lo; f <= f_hi; f += step) {
        double m = dft_magnitude(x, window, f, fs);
        if (m > best) {
            best = m;
            best_f = f;
        }
    }
    double a = best_f - step, b = best_f + step;
    const double g = (std::sqrt(5.0) - 1.0) / 2.0;
    double c = b - g * (b - a), d = a + g * (b - a);
    double mc = dft_magnitude(x, window, c, fs), md = dft_magnitude(x, window, d, fs);
    while (b - a > 1e-4) {
        if (mc > md) {
            b = d;
            d = c;
            md = mc;
            c = b - g * (b - a);
            mc = dft_magnitude(x, window, c, fs);
        } else {
            a = c;
            c = d;
            mc = md;
            d = a + g * (b - a);
            md = dft_magnitude(x, window, d, fs);
        }
    }
    return 0.5 * (a + b);
}

std::vector<double> rms_envelope_db(const std::vector<float>& x, double fs, double window_ms) {
    const size_t w = (size_t)(fs * window_ms / 1000.0);
    std::vector<double> db;
    for (size_t start = 0; start + w <= x.size(); start += w) {
        double mean = 0;
        for (size_t i = start; i < start + w; ++i) {
            mean += x[i];
        }
        mean /= (double)w;
        double sq = 0;
        for (size_t i = start; i < start + w; ++i) {
            sq += (x[i] - mean) * (x[i] - mean);
        }
        db.push_back(10.0 * std::log10(sq / (double)w + 1e-20));
    }
    return db;
}

std::vector<double> normalized(std::vector<double> env, size_t index) {
    const double ref = index == SIZE_MAX ? *std::max_element(env.begin(), env.end()) : env.at(index);
    for (double& v : env) {
        v -= ref;
    }
    return env;
}

double max_envelope_diff_db(const std::vector<double>& a, const std::vector<double>& b,
                            double floor_db, size_t first) {
    double worst = 0;
    for (size_t i = first; i < std::min(a.size(), b.size()); ++i) {
        if (b[i] > floor_db) {
            worst = std::max(worst, std::fabs(a[i] - b[i]));
        }
    }
    return worst;
}

void write_wav16(const std::string& path, const std::vector<float>& x, int fs) {
    std::ofstream f(path, std::ios::binary);
    auto u32 = [&](uint32_t v) { f.write(reinterpret_cast<const char*>(&v), 4); };
    auto u16 = [&](uint16_t v) { f.write(reinterpret_cast<const char*>(&v), 2); };
    const uint32_t bytes = (uint32_t)x.size() * 2;
    f.write("RIFF", 4);
    u32(36 + bytes);
    f.write("WAVEfmt ", 8);
    u32(16);
    u16(1);
    u16(1);
    u32((uint32_t)fs);
    u32((uint32_t)fs * 2);
    u16(2);
    u16(16);
    f.write("data", 4);
    u32(bytes);
    for (float s : x) {
        u16((uint16_t)(int16_t)std::clamp(s * 32767.0f, -32768.0f, 32767.0f));
    }
}
