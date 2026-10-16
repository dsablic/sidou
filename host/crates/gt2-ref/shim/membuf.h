/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
/* A growable byte buffer with the interface GoatTracker's relocator (greloc.c) uses from
 * its assembler's membuf.h. Written for SIDOU (gt2ref_shim.c); the assembler GoatTracker
 * ships is not used (third_party/goattracker2/VENDORED.md says why). */
#ifndef GT2REF_MEMBUF_H
#define GT2REF_MEMBUF_H

struct membuf {
    void* buf;
    int len;  /* bytes in use */
    int size; /* bytes allocated (zeroed past len) */
};

#define STATIC_MEMBUF_INIT {0, 0, 0}

void membuf_free(struct membuf* sb);
/* The allocated size, as greloc.c's ghost-register scan expects. */
int membuf_get_size(struct membuf* sb);
int membuf_memlen(struct membuf* sb);
void* membuf_get(struct membuf* sb);
void membuf_append(struct membuf* sb, const void* mem, int len);
void membuf_append_char(struct membuf* sb, char c);

#endif
