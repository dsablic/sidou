// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
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
