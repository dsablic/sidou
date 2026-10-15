// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! M1b's tolerances for the filter against reSIDfp (tests/filter.rs, the transfer functions at
//! FC 64..512 and resonance 0, 8 and 15), shared with tests/profiles.rs, which holds each chip
//! profile to them where it meets them. Only ever tightened.

/* The transfer functions: the largest error in dB around the cutoff, LP, BP, HP.
 * Measured at M1b Task 5: 8580 0.97, 0.49, 0.22 (the low-pass's at 7 kHz, warping);
 * 6581 3.34, 3.69, 3.45, at FC 384 and resonance 15, in the threshold region (FC up to
 * about 600) where reSIDfp's 6581 damping drifts with FC and one value per resonance
 * cannot follow it; elsewhere about 2 dB (its filter is no state-variable filter: away
 * from the cutoff its band-pass and high-pass level off). At M1b Task 6 (the 6581's
 * non-linearity and output mix) 6581 3.31, 3.12, 2.88, the largest now at FC 64 and
 * resonance 15. */
pub const MAX_TRANSFER_DB_8580: [f64; 3] = [1.0, 0.55, 0.3];
pub const MAX_TRANSFER_DB_6581: [f64; 3] = [3.32, 3.13, 2.89];

/* The passband gain, SIDOU minus reSIDfp, lowest to highest over the same settings, LP,
 * BP, HP (the high-pass only where 3 f0 is below tests/filter.rs's TOP_HZ): the low-pass
 * from f0/8 to f0/3, the band-pass within 1.2x of f0, the high-pass from 3 f0 to 9 f0.
 * Both transfer functions are against their own engine's unfiltered output, so the levels
 * are matched.
 * Measured at M1b Task 5 (rounded outwards to 0.01 dB): 8580 -0.11 to 0.00, -0.17 to
 * -0.05, -0.05 to 0.03; 6581 1.66 to 4.99, -1.61 to 3.04, 5.63 to 6.47 (its filter up to
 * 6.5 dB louder than reSIDfp's). M1b Task 6 gave the 6581 reSIDfp's mix of the filter's
 * outputs (each mode's gain, the integrators' leakage): -1.74 to 1.78, -2.01 to 2.06,
 * -0.40 to 0.52; what is left follows the threshold region's damping drift, as above. */
pub const PASSBAND_GAIN_DB_8580: [(f64, f64); 3] = [(-0.12, 0.01), (-0.17, -0.04), (-0.05, 0.03)];
pub const PASSBAND_GAIN_DB_6581: [(f64, f64); 3] = [(-1.74, 1.79), (-2.02, 2.06), (-0.40, 0.52)];
