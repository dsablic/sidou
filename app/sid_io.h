// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_IO_H
#define SIDOU_SID_IO_H

#include <stdint.h>

#include "psid/psid_ctl.h"
#include "store.h"

#define SID_IO_HEAD 0x80u
#define SID_IO_CLEAR_BYTES 16384u
#define SID_IO_PASS_BYTES 2048u

enum {
    SID_IO_OK = 0,
    SID_IO_NOT_FOUND,
    SID_IO_READ,
    SID_IO_RSID,
    SID_IO_MUS,
    SID_IO_BROKEN,
    SID_IO_TOO_LONG
};

typedef struct {
    const store* st;
    psid_ctl* ctl;
    const psid_header* h;
    store_file file;
    uint32_t at;
    uint8_t stage;
} sid_io;

int sid_io_next(const store* st, store_file* f);
int sid_io_refusal(int psid_code);
const char* sid_io_message(int result);
int sid_io_open(sid_io* io, const store* st, const char* name, psid_header* h);
int sid_io_load(sid_io* io, psid_ctl* c, const psid_header* h);
int sid_io_poll(sid_io* io);
int sid_io_busy(const sid_io* io);

#endif
