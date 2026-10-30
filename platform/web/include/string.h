// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The wasm core has no libc: the core's string.h calls, from platform/fm1/libc_min.c. */
#ifndef SIDOU_WASM_STRING_H
#define SIDOU_WASM_STRING_H

#include <stddef.h>

void* memset(void* dst, int c, size_t n);
void* memcpy(void* dst, const void* src, size_t n);
int memcmp(const void* a, const void* b, size_t n);
int strcmp(const char* a, const char* b);
char* strcpy(char* dst, const char* src);

#endif
