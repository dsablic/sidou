// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The libc the compiler calls on its own (struct zeroing and copies) and the song reader's
 * memcmp; there is no libc. In flash: the audio interrupt calls none of them. */
#include <stddef.h>
#include <stdint.h>

void* memset(void* dst, int c, size_t n);
void* memcpy(void* dst, const void* src, size_t n);
int memcmp(const void* a, const void* b, size_t n);
int strcmp(const char* a, const char* b);
char* strcpy(char* dst, const char* src);

/* A word of the bytes it aliases (the copies and fills go a word at a time where they can). */
typedef uint32_t __attribute__((may_alias)) word;

void* memset(void* dst, int c, size_t n) {
    unsigned char* d = dst;
    for (; n && ((uintptr_t)d & 3u); n--) {
        *d++ = (unsigned char)c;
    }
    uint32_t w = (unsigned char)c * 0x01010101u;
    for (; n >= 4u; n -= 4u, d += 4) {
        *(word*)(void*)d = w;
    }
    while (n--) {
        *d++ = (unsigned char)c;
    }
    return dst;
}

/* A word at a time while both are at the same offset in a word (a canvas row into the grid's,
 * a struct copy), else a byte. */
void* memcpy(void* dst, const void* src, size_t n) {
    unsigned char* d = dst;
    const unsigned char* s = src;
    if (!(((uintptr_t)d ^ (uintptr_t)s) & 3u)) {
        for (; n && ((uintptr_t)d & 3u); n--) {
            *d++ = *s++;
        }
        for (; n >= 4u; n -= 4u, d += 4, s += 4) {
            *(word*)(void*)d = *(const word*)(const void*)s;
        }
    }
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

int strcmp(const char* a, const char* b) {
    while (*a && *a == *b) {
        a++;
        b++;
    }
    return (unsigned char)*a - (unsigned char)*b;
}

char* strcpy(char* dst, const char* src) {
    char* out = dst;
    while ((*out++ = *src++) != 0) {
    }
    return dst;
}
