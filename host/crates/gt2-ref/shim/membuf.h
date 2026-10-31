/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef GT2REF_MEMBUF_H
#define GT2REF_MEMBUF_H

struct membuf {
    void* buf;
    int len;
    int size;
};

#define STATIC_MEMBUF_INIT {0, 0, 0}

void membuf_free(struct membuf* sb);
int membuf_get_size(struct membuf* sb);
int membuf_memlen(struct membuf* sb);
void* membuf_get(struct membuf* sb);
void membuf_append(struct membuf* sb, const void* mem, int len);
void membuf_append_char(struct membuf* sb, char c);

#endif
