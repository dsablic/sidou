// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <cstddef>
#include <string>
#include <vector>

/* Fundamental frequency as the peak of a Hann-windowed DFT magnitude searched
 * between f_lo and f_hi. Unlike autocorrelation it is not biased by how sharp
 * (aliased or band-limited) the waveform is. */
double estimate_f0(const std::vector<float>& x, double fs, double f_lo, double f_hi);

/* Mean-removed RMS per window, in dB (not normalised). */
std::vector<double> rms_envelope_db(const std::vector<float>& x, double fs, double window_ms);

/* Shift an envelope so that window `index` is 0 dB; SIZE_MAX means the loudest window. */
std::vector<double> normalized(std::vector<double> env, size_t index);

/* Largest |a - b| over windows from `first` on where the reference b is above floor_db. */
double max_envelope_diff_db(const std::vector<double>& a, const std::vector<double>& b,
                            double floor_db, size_t first);

void write_wav16(const std::string& path, const std::vector<float>& x, int fs);
