// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <cstddef>
#include <string>
#include <vector>

/* All functions throw std::invalid_argument on degenerate input (too few
 * samples, an empty envelope, an empty or out-of-range window range). */

/* Fundamental frequency as the peak of a Hann-windowed DFT magnitude searched
 * between f_lo and f_hi. Unlike autocorrelation it is not biased by how sharp
 * (aliased or band-limited) the waveform is. */
double estimate_f0(const std::vector<float>& x, double fs, double f_lo, double f_hi);

/* Mean-removed RMS per window of `window` samples, in dB (not normalised). For a
 * periodic tone, make the window a whole number of periods, or the RMS ripples
 * with the phase at which each window starts. */
std::vector<double> rms_envelope_db(const std::vector<float>& x, size_t window);

/* Shift an envelope so that window `index` is 0 dB; SIZE_MAX means the loudest window. */
std::vector<double> normalized(std::vector<double> env, size_t index);

/* Shift an envelope so that the mean of windows [first, last) is 0 dB. */
std::vector<double> normalized_to_mean(std::vector<double> env, size_t first, size_t last);

/* Largest |a - b| over windows from `first` on where the reference b is above
 * floor_db. NaN if any compared window is NaN, so a comparison against it fails. */
double max_envelope_diff_db(const std::vector<double>& a, const std::vector<double>& b,
                            double floor_db, size_t first);

/* Returns false if the file could not be written completely. */
bool write_wav16(const std::string& path, const std::vector<float>& x, int fs);
