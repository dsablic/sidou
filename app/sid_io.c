// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "sid_io.h"

#include <string.h>

enum { STAGE_IDLE, STAGE_CLEAR, STAGE_DATA };

/* SysEx's own files (app/sysex.c): MIDI INSTRUMENT's staging was stored as type 2 before .sid
 * files were; never a tune. */
static int reserved(const char* name) {
    return !strcmp(name, "MIDI INSTRUMENT") || !strcmp(name, "MIDI SONG");
}

int sid_io_next(const store* st, store_file* f) {
    while (store_dir_next(st, f) == 1) {
        if (f->type == STORE_FILE_SID && !reserved(f->name)) {
            return 1;
        }
    }
    return 0;
}

int sid_io_refusal(int psid_code) {
    switch (psid_code) {
    case PSID_OK: return SID_IO_OK;
    case PSID_E_RSID: return SID_IO_RSID;
    case PSID_E_MUS: return SID_IO_MUS;
    case PSID_E_TOO_LONG: return SID_IO_TOO_LONG;
    default: return SID_IO_BROKEN;
    }
}

const char* sid_io_message(int result) {
    switch (result) {
    case SID_IO_OK: return "";
    case SID_IO_NOT_FOUND: return "SID NOT FOUND";
    case SID_IO_RSID: return "RSID: NOT YET";
    case SID_IO_MUS: return "MUS: NOT SUPPORTED";
    case SID_IO_TOO_LONG: return "TOO LONG";
    case SID_IO_BROKEN: return "BROKEN .SID";
    default: return "SID: READ ERROR";
    }
}

int sid_io_open(sid_io* io, const store* st, const char* name, psid_header* h) {
    uint8_t head[SID_IO_HEAD];
    memset(h, 0, sizeof *h);
    io->st = st;
    io->stage = STAGE_IDLE;
    if (reserved(name) || store_file_open(st, name, &io->file) != STORE_OK ||
        io->file.type != STORE_FILE_SID) {
        return SID_IO_NOT_FOUND;
    }
    uint32_t n = io->file.length < SID_IO_HEAD ? io->file.length : SID_IO_HEAD;
    if (store_file_read(st, &io->file, 0, head, n) != (int)n) {
        return SID_IO_READ;
    }
    /* The file's length with its first bytes: every check psid_parse makes against the rest
     * is a bound, and it reads nothing past $7D. */
    int r = sid_io_refusal(psid_parse(head, io->file.length, h));
    if (r != SID_IO_OK) {
        memset(h, 0, sizeof *h);
    }
    return r;
}

int sid_io_load(sid_io* io, psid_ctl* c, const psid_header* h) {
    if (!psid_ctl_load_over(c, h)) {
        return 0;
    }
    io->ctl = c;
    io->h = h;
    io->at = 2u; /* $00/$01: the port, as psid_ctl_load_over set them */
    io->stage = STAGE_CLEAR;
    return 1;
}

int sid_io_poll(sid_io* io) {
    if (io->stage == STAGE_CLEAR) {
        uint32_t left = 0x10000u - io->at;
        uint32_t n = left < SID_IO_CLEAR_BYTES ? left : SID_IO_CLEAR_BYTES;
        memset(io->ctl->mem + io->at, 0, n);
        io->at += n;
        if (io->at == 0x10000u) {
            io->at = io->h->data_off;
            io->stage = STAGE_DATA;
        }
        return 1;
    }
    if (io->stage != STAGE_DATA) {
        return 0;
    }
    /* Straight into the 6502's memory: psid_parse has the data within the file and within
     * $0002-$FFFF. */
    const psid_header* h = io->h;
    uint32_t end = h->data_off + h->data_len;
    uint32_t n = end - io->at < SID_IO_PASS_BYTES ? end - io->at : SID_IO_PASS_BYTES;
    uint8_t* to = io->ctl->mem + h->load + (io->at - h->data_off);
    if (store_file_read(io->st, &io->file, io->at, to, n) != (int)n) {
        io->stage = STAGE_IDLE;
        return -SID_IO_READ;
    }
    io->at += n;
    if (io->at < end) {
        return 1;
    }
    io->stage = STAGE_IDLE;
    return 0;
}

int sid_io_busy(const sid_io* io) {
    return io->stage != STAGE_IDLE;
}
