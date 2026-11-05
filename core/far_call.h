// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FAR_CALL_H
#define SIDOU_FAR_CALL_H

#ifdef __PI32V2__
static inline __attribute__((always_inline)) void* sidou_far(void* p) {
    void* volatile q = p;
    return q;
}
#define SIDOU_FAR(fn) ((__typeof__(&fn))sidou_far((void*)&fn))
#else
#define SIDOU_FAR(fn) (fn)
#endif

#endif
