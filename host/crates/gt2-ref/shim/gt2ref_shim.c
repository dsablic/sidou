/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
/* What GoatTracker's song and relocation code needs from outside it, for the gt2-ref
 * harness (third_party/goattracker2/VENDORED.md lists every stub):
 * - the BME calls it makes: bytes to and from files, and the "linked datafile" the
 *   relocator reads player.s and altplayer.s from (here the vendored files, by path);
 * - files in memory: GoatTracker's sources are compiled with fopen renamed to
 *   gt2ref_fopen, which hands out the song to read and collects the files written;
 * - the membuf the relocator builds its source and output in;
 * - the editor's screen, sound and playback calls, as stubs that do nothing.
 * Written for SIDOU; nothing in it is GoatTracker's or BME's code. */
#include <setjmp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#include "gt2ref_shim.h"
#include "membuf.h"

/* Files in memory. */

static const unsigned char* in_data;
static size_t in_len;
static FILE* in_file;
static int in_short;
static char* out_buf[2];
static size_t out_len[2];
static char out_name[2][64];

void gt2ref_set_input(const unsigned char* data, size_t len) {
    in_data = data;
    in_len = len;
    in_file = NULL;
    in_short = 0;
}

int gt2ref_input_short(void) {
    return in_short;
}

size_t gt2ref_fread(void* p, size_t size, size_t n, FILE* f) {
    size_t got = fread(p, size, n, f);
    /* fread returns 0 for a size of 0 (an empty table): that is no short read. */
    if (f == in_file && size != 0 && got < n)
        in_short = 1;
    return got;
}

void gt2ref_take_output(const char* name, unsigned char** data, size_t* len) {
    *data = NULL;
    *len = 0;
    for (int i = 0; i < 2; i++) {
        if (out_buf[i] && strcmp(out_name[i], name) == 0) {
            *data = (unsigned char*)out_buf[i];
            *len = out_len[i];
            out_buf[i] = NULL;
            out_len[i] = 0;
            out_name[i][0] = 0;
        }
    }
}

void gt2ref_reset_files(void) {
    for (int i = 0; i < 2; i++) {
        free(out_buf[i]);
        out_buf[i] = NULL;
        out_len[i] = 0;
        out_name[i][0] = 0;
    }
}

FILE* gt2ref_fopen(const char* name, const char* mode) {
    if (mode[0] == 'r') {
        if (strcmp(name, GT2REF_INPUT) != 0 || in_data == NULL)
            return NULL;
        static unsigned char none[1];
        /* An empty song as one byte (fmemopen refuses a size of 0): its ident's read is
         * short all the same. */
        in_file = in_len ? fmemopen((void*)in_data, in_len, "rb") : fmemopen(none, 1, "rb");
        return in_file;
    }
    for (int i = 0; i < 2; i++) {
        if (out_buf[i] == NULL && out_name[i][0] == 0) {
            snprintf(out_name[i], sizeof out_name[i], "%s", name);
            return open_memstream(&out_buf[i], &out_len[i]);
        }
    }
    return NULL;
}

/* BME's file helpers (bme_end.h). */

unsigned fread8(FILE* file) {
    unsigned char b = 0;
    if (gt2ref_fread(&b, 1, 1, file) != 1)
        return 0;
    return b;
}

void fwrite8(FILE* file, unsigned data) {
    unsigned char b = (unsigned char)data;
    fwrite(&b, 1, 1, file);
}

void fwritele16(FILE* file, unsigned data) {
    fwrite8(file, data & 0xff);
    fwrite8(file, data >> 8);
}

/* The linked datafile: the player sources, read from the vendored tree. */

static unsigned char* io_data[4];
static long io_len[4], io_pos[4];

int io_openlinkeddatafile(unsigned char* datafile) {
    (void)datafile;
    return 1;
}

int io_open(char* name) {
    char path[1024];
    snprintf(path, sizeof path, "%s/%s", GT2REF_SRC_DIR, name);
    FILE* f = fopen(path, "rb");
    if (f == NULL)
        return -1;
    for (int h = 0; h < 4; h++) {
        if (io_data[h] == NULL) {
            fseek(f, 0, SEEK_END);
            io_len[h] = ftell(f);
            fseek(f, 0, SEEK_SET);
            io_data[h] = malloc((size_t)io_len[h] + 1);
            if (io_data[h] == NULL || fread(io_data[h], 1, (size_t)io_len[h], f) != (size_t)io_len[h]) {
                free(io_data[h]);
                io_data[h] = NULL;
                fclose(f);
                return -1;
            }
            io_pos[h] = 0;
            fclose(f);
            return h;
        }
    }
    fclose(f);
    return -1;
}

int io_lseek(int h, int bytes, int whence) {
    if (whence == SEEK_SET)
        io_pos[h] = bytes;
    else if (whence == SEEK_CUR)
        io_pos[h] += bytes;
    else
        io_pos[h] = io_len[h] + bytes;
    return (int)io_pos[h];
}

int io_read8(int h) {
    if (io_pos[h] >= io_len[h])
        return 0;
    return io_data[h][io_pos[h]++];
}

void io_close(int h) {
    free(io_data[h]);
    io_data[h] = NULL;
}

/* GoatTracker's stdout */

FILE* gt2ref_quiet(void) {
    static FILE* quiet;
    if (quiet == NULL)
        quiet = fopen("/dev/null", "w");
    return quiet ? quiet : stderr;
}

/* exit, returning to gt2ref_guard */

static jmp_buf guard;
static int guarded;

/* The blocks GoatTracker allocates inside gt2ref_guard and has not freed: the relocator's
 * work buffers (greloc.c's songwork, pattwork, instrwork), which are its locals, so its
 * exit on an assembly error loses them. gt2ref_guard frees them on that path. */
#define GUARDED_BLOCKS 16
static void* guarded_block[GUARDED_BLOCKS];
static int guarded_blocks;

void* gt2ref_malloc(size_t size) {
    void* p = malloc(size);
    if (p == NULL || !guarded)
        return p;
    if (guarded_blocks == GUARDED_BLOCKS) {
        fprintf(stderr, "gt2ref: more than %d blocks allocated in the relocator\n",
                GUARDED_BLOCKS);
        abort();
    }
    guarded_block[guarded_blocks++] = p;
    return p;
}

void gt2ref_gt_free(void* p) {
    for (int i = 0; i < guarded_blocks; i++) {
        if (guarded_block[i] == p) {
            guarded_block[i] = guarded_block[--guarded_blocks];
            break;
        }
    }
    free(p);
}

void gt2ref_exit(int status) {
    if (guarded)
        longjmp(guard, 1);
    exit(status);
}

int gt2ref_guard(void (*fn)(void)) {
    guarded = 1;
    guarded_blocks = 0;
    if (setjmp(guard)) {
        guarded = 0;
        while (guarded_blocks > 0)
            free(guarded_block[--guarded_blocks]);
        return -1;
    }
    fn();
    guarded = 0;
    guarded_blocks = 0;
    return 0;
}

int win_quitted = 0;

/* The editor's state and calls that the song, table and relocation code names: stubs.
 * gt2reloc's main reads the datafile; the harness never calls it (gt2ref.c). */
unsigned char datafile[1];
int key, rawkey, shiftpressed;
unsigned char sidreg[0x19];

void editstring(char* buffer, int maxlength) {
    (void)buffer;
    (void)maxlength;
}
void resettime(void) {}
void incrementtime(void) {}
void sound_suspend(void) {}
void sound_flush(void) {}

/* membuf (left out when the harness is checked against GoatTracker's own assembler,
 * which brings its own: GT2REF_EXOMIZER_ASM, docs in VENDORED.md) */

#ifndef GT2REF_EXOMIZER_ASM
void membuf_free(struct membuf* sb) {
    free(sb->buf);
    sb->buf = NULL;
    sb->len = 0;
    sb->size = 0;
}

int membuf_get_size(struct membuf* sb) {
    return sb->size;
}

int membuf_memlen(struct membuf* sb) {
    return sb->len;
}

void* membuf_get(struct membuf* sb) {
    return sb->buf;
}

void membuf_append(struct membuf* sb, const void* mem, int len) {
    if (sb->len + len > sb->size) {
        int size = sb->size ? sb->size : 1024;
        while (size < sb->len + len)
            size *= 2;
        char* b = realloc(sb->buf, (size_t)size);
        if (b == NULL) {
            fprintf(stderr, "gt2ref: out of memory\n");
            exit(1);
        }
        memset(b + sb->size, 0, (size_t)(size - sb->size));
        sb->buf = b;
        sb->size = size;
    }
    memcpy((char*)sb->buf + sb->len, mem, (size_t)len);
    sb->len += len;
}

void membuf_append_char(struct membuf* sb, char c) {
    membuf_append(sb, &c, 1);
}
#endif
