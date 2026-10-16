/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
/* The assembler entry point GoatTracker's relocator (greloc.c) calls: assembles the source
 * text in src into dest. 0 on success. SIDOU's own assembler (gt2ref_asm.c) for the dialect
 * of GoatTracker's player.s. */
#ifndef GT2REF_PARSE_H
#define GT2REF_PARSE_H

#include "membuf.h"

int assemble(struct membuf* src, struct membuf* dest);

#endif
