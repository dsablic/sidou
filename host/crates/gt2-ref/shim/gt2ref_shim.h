/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef GT2REF_SHIM_H
#define GT2REF_SHIM_H

#include <stddef.h>
#include <stdio.h>

#define GT2REF_INPUT "gt2ref.sng"

FILE* gt2ref_fopen(const char* name, const char* mode);
void gt2ref_set_input(const unsigned char* data, size_t len);
void gt2ref_take_output(const char* name, unsigned char** data, size_t* len);
void gt2ref_reset_files(void);
size_t gt2ref_fread(void* p, size_t size, size_t n, FILE* f);
int gt2ref_input_short(void);

void gt2ref_exit(int status);
int gt2ref_guard(void (*fn)(void));
void* gt2ref_malloc(size_t size);
void gt2ref_gt_free(void* p);

FILE* gt2ref_quiet(void);

#endif
