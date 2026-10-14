// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "signal_metrics.h"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <fstream>
#include <limits>
#include <numeric>
#include <stdexcept>

namespace {

constexpr double kPi = 3.14159265358979323846;

double dft_magnitude(const std::vector<float>& x, const std::vector<double>& window, double f,
                     double fs) {
    const double w = 2.0 * kPi * f / fs;
    double re = 0, im = 0;
    for (size_t i = 0; i < x.size(); ++i) {
        re += window[i] * x[i] * std::cos(w * (double)i);
        im -= window[i] * x[i] * std::sin(w * (double)i);
    }
    return std::sqrt(re * re + im * im);
}

} // namespace

double estimate_f0(const std::vector<float>& x, double fs, double f_lo, double f_hi) {
    if (x.size() < 2) {
        throw std::invalid_argument("estimate_f0: need at least 2 samples");
    }
    std::vector<double> window(x.size());
    for (size_t i = 0; i < x.size(); ++i) {
        window[i] = 0.5 - 0.5 * std::cos(2.0 * kPi * (double)i / (double)(x.size() - 1));
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

std::vector<double> rms_envelope_db(const std::vector<float>& x, size_t w) {
    if (w == 0) {
        throw std::invalid_argument("rms_envelope_db: window must not be empty");
    }
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

std::vector<double> normalized_to_max(std::vector<double> env, size_t first, size_t last) {
    if (first >= last || last > env.size()) {
        throw std::invalid_argument("normalized_to_max: bad window range");
    }
    const double ref =
        *std::max_element(env.begin() + (std::ptrdiff_t)first, env.begin() + (std::ptrdiff_t)last);
    for (double& v : env) {
        v -= ref;
    }
    return env;
}

std::vector<double> normalized_to_mean(std::vector<double> env, size_t first, size_t last) {
    if (first >= last || last > env.size()) {
        throw std::invalid_argument("normalized_to_mean: bad window range");
    }
    const double ref =
        std::accumulate(env.begin() + (std::ptrdiff_t)first, env.begin() + (std::ptrdiff_t)last, 0.0) /
        (double)(last - first);
    for (double& v : env) {
        v -= ref;
    }
    return env;
}

double max_envelope_diff_db(const std::vector<double>& a, const std::vector<double>& b,
                            double floor_db, size_t first) {
    double worst = 0;
    for (size_t i = first; i < std::min(a.size(), b.size()); ++i) {
        if (std::isnan(a[i]) || std::isnan(b[i])) {
            return std::numeric_limits<double>::quiet_NaN();
        }
        if (b[i] > floor_db) {
            worst = std::max(worst, std::fabs(a[i] - b[i]));
        }
    }
    return worst;
}

bool write_wav16(const std::string& path, const std::vector<float>& x, int fs) {
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
    f.close();
    return !f.fail();
}
