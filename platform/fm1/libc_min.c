// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The libc the compiler calls on its own (struct zeroing and copies) and the song reader's
 * memcmp; there is no libc. In flash: the audio interrupt calls none of them. */
#include <stddef.h>

void* memset(void* dst, int c, size_t n);
void* memcpy(void* dst, const void* src, size_t n);
int memcmp(const void* a, const void* b, size_t n);

void* memset(void* dst, int c, size_t n) {
    unsigned char* d = dst;
    while (n--) {
        *d++ = (unsigned char)c;
    }
    return dst;
}

void* memcpy(void* dst, const void* src, size_t n) {
    unsigned char* d = dst;
    const unsigned char* s = src;
    while (n--) {
        *d++ = *s++;
    }
    return dst;
}

int memcmp(const void* a, const void* b, size_t n) {
    const unsigned char* p = a;
    const unsigned char* q = b;
    for (; n; n--, p++, q++) {
        if (*p != *q) {
            return *p < *q ? -1 : 1;
        }
    }
    return 0;
}
