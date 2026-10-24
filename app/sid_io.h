// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* .sid tunes on the store (M3 Task 10; docs/plans/2026-10-19-m3-sid-files.md, "How tunes reach
 * the device"): the files of type STORE_FILE_SID for the browser, in name order, and a tune's
 * loading into psid_ctl: its header checked first (the file's first SID_IO_HEAD bytes and its
 * length are all psid_parse reads), then the 6502's 64 KiB as at power-on and the data read
 * straight from flash into it at the load address (no buffer), a part a pass. Each pass's part
 * is within the store's share of a pass (STORE_POLL_WORST, 30,000 instructions: the emulator
 * test psid_store.rs measures every sid_io_poll), and the caller gives sid_io_poll only passes
 * the store does no work in (audio_work_slice on the device).
 *
 * A song change of a tune loaded so (psid_ctl_load's file NULL: psid_ctl_subtune refuses) is the
 * caller's: psid_ctl_stop, and once the stop has played out, sid_io_load again and the parts,
 * then psid_ctl_play with the song. The file is opened again by name for each load, so a file
 * replaced or deleted meanwhile (a SysEx PUT or DEL) is read as it is then, or refused.
 *
 * Portable C11, no allocation: the main loop's, one caller. */
#ifndef SIDOU_SID_IO_H
#define SIDOU_SID_IO_H

#include <stdint.h>

#include "psid/psid_ctl.h"
#include "store.h"

/* The header's bytes read: psid_parse reads to $7D at most (a v2+ header and a load address
 * at the data's start). */
#define SID_IO_HEAD 0x80u
/* The 6502's memory cleared in a pass, and the data read into it in a pass (sid_io_poll). */
#define SID_IO_CLEAR_BYTES 16384u
#define SID_IO_PASS_BYTES 2048u

/* Results: SID_IO_OK, or why a tune does not load (sid_io_message's text for each). */
enum {
    SID_IO_OK = 0,
    SID_IO_NOT_FOUND, /* no .sid file of that name (or a store name SysEx keeps) */
    SID_IO_READ,      /* the file went (deleted or replaced) or a read failed */
    SID_IO_RSID,      /* psid_parse's PSID_E_RSID */
    SID_IO_MUS,       /* PSID_E_MUS */
    SID_IO_BROKEN,    /* PSID_E_NOT_SID, PSID_E_HEADER, PSID_E_LOAD */
    SID_IO_TOO_LONG   /* PSID_E_TOO_LONG */
};

typedef struct {
    const store* st;
    psid_ctl* ctl;
    const psid_header* h;
    store_file file;
    uint32_t at;   /* the stage's next address (the clear) or file offset (the data) */
    uint8_t stage; /* sid_io.c's */
} sid_io;

/* The next .sid file after f->name in name order (f->name "" for the first): 1 with f filled,
 * 0 at the end. SysEx's own names are skipped. */
int sid_io_next(const store* st, store_file* f);
/* psid_parse's refusal as a SID_IO_* result (PSID_OK: SID_IO_OK). */
int sid_io_refusal(int psid_code);
/* The toast for a result: "RSID: NOT YET", "MUS: NOT SUPPORTED", "BROKEN .SID", "TOO LONG",
 * "SID NOT FOUND", "SID: READ ERROR"; "" for SID_IO_OK. */
const char* sid_io_message(int result);
/* Opens the store's .sid file `name` and parses its header into h: SID_IO_OK, or why not (h
 * then empty). Reads SID_IO_HEAD bytes and compares names: a pass's work. */
int sid_io_open(sid_io* io, const store* st, const char* name, psid_header* h);
/* Starts loading the tune io has open, header h (sid_io_open's, kept by the caller while the
 * tune is loaded), into c: 0 while c is not quiet (psid_ctl_load's), else 1 and the work is
 * sid_io_poll's. Nothing heavy here. */
int sid_io_load(sid_io* io, psid_ctl* c, const psid_header* h);
/* One pass of the loading: 1 while there is more, 0 once the tune is loaded (psid_ctl_play may
 * go), -SID_IO_READ if the file went (the tune is not loaded then). At most SID_IO_CLEAR_BYTES
 * cleared or SID_IO_PASS_BYTES read a call. */
int sid_io_poll(sid_io* io);
/* 1 while sid_io_load's work is not done. */
int sid_io_busy(const sid_io* io);

#endif
