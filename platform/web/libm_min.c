// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The two libm calls the core's __builtin_fminf/__builtin_fmaxf become on wasm32 (WebAssembly's
 * f32.min and f32.max propagate a NaN; C's return the other operand). */
float fmaxf(float a, float b);
float fminf(float a, float b);

float fmaxf(float a, float b) {
    if (a != a) {
        return b;
    }
    if (b != b) {
        return a;
    }
    return a > b ? a : b;
}

float fminf(float a, float b) {
    if (a != a) {
        return b;
    }
    if (b != b) {
        return a;
    }
    return a < b ? a : b;
}
