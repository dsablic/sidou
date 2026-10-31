/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef GT2REF_PARSE_H
#define GT2REF_PARSE_H

#include "membuf.h"

int assemble(struct membuf* src, struct membuf* dest);

#include <stddef.h>

int gt2ref_assemble(const unsigned char* text, size_t len, int strict, unsigned char** out,
                    size_t* out_len);

#endif
