/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
/* The assembler entry point GoatTracker's relocator (greloc.c) calls: assembles the source
 * text in src into dest. 0 on success. SIDOU's own assembler (gt2ref_asm.c) for the dialect
 * of GoatTracker's player.s. */
#ifndef GT2REF_PARSE_H
#define GT2REF_PARSE_H

#include "membuf.h"

int assemble(struct membuf* src, struct membuf* dest);

#include <stddef.h>

/* The same for SIDOU's tests: text in, a malloc'd buffer out (gt2ref_free), strict or not
 * (gt2ref_asm.c). */
int gt2ref_assemble(const unsigned char* text, size_t len, int strict, unsigned char** out,
                    size_t* out_len);

#endif
