/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
/* The harness's files in memory (gt2ref_shim.c). GoatTracker's sources are compiled with
 * fopen renamed to gt2ref_fopen (gt2ref_rename.h): opening GT2REF_INPUT for reading gives the song set by
 * gt2ref_set_input; opening any name for writing collects the bytes for
 * gt2ref_take_output. */
#ifndef GT2REF_SHIM_H
#define GT2REF_SHIM_H

#include <stddef.h>
#include <stdio.h>

#define GT2REF_INPUT "gt2ref.sng"

FILE* gt2ref_fopen(const char* name, const char* mode);
void gt2ref_set_input(const unsigned char* data, size_t len);
/* The bytes written to name (malloc'd, the caller frees them), or NULL. */
void gt2ref_take_output(const char* name, unsigned char** data, size_t* len);
void gt2ref_reset_files(void);
/* GoatTracker's sources are compiled with fread renamed to gt2ref_fread: a read of the song that
 * gets less than it asked for is recorded, so a short file is an error, not zeros.
 * gt2ref_input_short says whether one has happened since gt2ref_set_input. */
size_t gt2ref_fread(void* p, size_t size, size_t n, FILE* f);
int gt2ref_input_short(void);

/* GoatTracker's sources are compiled with exit renamed to gt2ref_exit: inside gt2ref_guard's
 * callback it returns to gt2ref_guard (which then returns -1), elsewhere it exits. */
void gt2ref_exit(int status);
int gt2ref_guard(void (*fn)(void));

#endif
