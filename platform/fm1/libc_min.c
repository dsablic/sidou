// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include <stddef.h>
#include <stdint.h>

void* memset(void* dst, int c, size_t n);
void* memcpy(void* dst, const void* src, size_t n);
int memcmp(const void* a, const void* b, size_t n);
int strcmp(const char* a, const char* b);
char* strcpy(char* dst, const char* src);

typedef uint32_t __attribute__((may_alias)) word;

void* memset(void* dst, int c, size_t n) {
    unsigned char* d = dst;
    if (n >= 8u) {
        for (; (uintptr_t)d & 3u; n--) {
            *d++ = (unsigned char)c;
        }
        word* dw = (word*)(void*)d;
        uint32_t w = (unsigned char)c * 0x01010101u;
        size_t k = n >> 2;
        n &= 3u;
        do {
            *dw++ = w;
        } while (--k);
        d = (unsigned char*)dw;
    }
    while (n--) {
        *d++ = (unsigned char)c;
    }
    return dst;
}

void* memcpy(void* dst, const void* src, size_t n) {
    unsigned char* d = dst;
    const unsigned char* s = src;
    if (n >= 8u && !(((uintptr_t)d ^ (uintptr_t)s) & 3u)) {
        for (; (uintptr_t)d & 3u; n--) {
            *d++ = *s++;
        }
        word* dw = (word*)(void*)d;
        const word* sw = (const word*)(const void*)s;
        size_t k = n >> 2;
        n &= 3u;
        do {
            *dw++ = *sw++;
        } while (--k);
        d = (unsigned char*)dw;
        s = (const unsigned char*)sw;
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
