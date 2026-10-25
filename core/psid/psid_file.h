/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-License-Identifier: GPL-3.0-or-later AND BSD-2-Clause */
/* The PSID file's header (M3 Task 4; docs/reference/psid.md): one pure function that checks
 * every field against the file's length before using it, and the result.
 *
 * The design follows FM-1 SID's player (psid.c, fm1-emu, BSD-2-Clause, Joakim Eriksson):
 * the header read, the load address from the data when the header's is 0, init defaulting
 * to the load address. Its loader uses the data offset and the load address unchecked
 * (docs/upstream-reports.md, 1); SIDOU's checks them all.
 *
 * Redistribution and use in source and binary forms, with or without modification, are
 * permitted provided that the following conditions are met:
 *  1. Redistributions of source code must retain the above copyright notice, this list of
 *     conditions and the following disclaimer.
 *  2. Redistributions in binary form must reproduce the above copyright notice, this list
 *     of conditions and the following disclaimer in the documentation and/or other
 *     materials provided with the distribution.
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY
 * EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL
 * THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
 * SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT
 * OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
 * OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 *
 * Portable C11, no allocation. */
#ifndef SIDOU_PSID_FILE_H
#define SIDOU_PSID_FILE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
    PSID_OK = 0,
    PSID_E_NOT_SID,  /* shorter than $76, or the magic is neither PSID nor RSID */
    PSID_E_RSID,     /* magic RSID: a real C64 is needed (M3 rejects them) */
    PSID_E_HEADER,   /* version, data offset, songs, or a field the file is too short for */
    PSID_E_MUS,      /* flags bit 0: Compute!'s Sidplayer data */
    PSID_E_TOO_LONG, /* no data, or load + length over $10000 */
    PSID_E_LOAD      /* the data covers $0000-$0001, the 6510's port */
};

/* Warnings: an accepted file whose header SIDOU corrected or cannot fully honour. */
#define PSID_W_START 0x01u     /* start song 0 or above songs: song 1 */
#define PSID_W_PLAYSID 0x02u   /* PlaySID specific: its $D41D samples are silent */
#define PSID_W_SID2 0x04u      /* an invalid second SID address: no second SID */
#define PSID_W_SID3 0x08u      /* an invalid or repeated third SID address: none */
#define PSID_W_MORE_SIDS 0x10u /* v5: chips past the third are dropped */
#define PSID_W_FM_OPL 0x20u    /* v5: an FM OPL chip is needed; there is none */

#define PSID_HEADER_SIZE_V1 0x76u
#define PSID_HEADER_SIZE_V2 0x7Cu
#define PSID_STRING_MAX 64u /* a string's bytes with its NUL; longer ones are cut */

typedef struct psid_header {
    uint16_t version; /* 1-5 */
    uint16_t load;    /* the effective load address */
    uint16_t init;    /* the effective init address */
    uint16_t play;    /* 0: the tune installs its own interrupt handler */
    uint16_t songs;   /* 1-256 */
    uint16_t start;   /* 1..songs */
    uint16_t flags;   /* 0 for v1 */
    uint32_t speed;
    uint32_t data_off;    /* the data's first byte in the file (past a load-address prefix) */
    uint32_t data_len;    /* 1..$10000 - load */
    uint32_t lengths_off; /* v5: the song-length table in the file, 0 if none (not read) */
    uint16_t sid2;        /* the second and third SID's address ($Dxx0), 0 for none */
    uint16_t sid3;
    uint8_t clock;   /* flags bits 2-3: 0 unknown, 1 PAL, 2 NTSC, 3 both */
    uint8_t model1;  /* the SID models, as the file's 2-bit codes */
    uint8_t model2;  /* 0 in the file means "as the first": resolved here */
    uint8_t model3;
    uint8_t sids;    /* chips the header lists: 1, 2, 3, or more for v5 */
    uint8_t warn;    /* PSID_W_* */
    uint8_t speed_repeats; /* songs above 32 repeat the speed bits (v1, PlaySID) */
    char name[PSID_STRING_MAX];
    char author[PSID_STRING_MAX];
    char released[PSID_STRING_MAX];
} psid_header;

/* Parses and checks the header of the file b[0..n): PSID_OK, or the first refusal (docs/
 * reference/psid.md, "What SIDOU refuses"). Pure: reads only b[0..n), writes only *h (zeroed
 * first, so a refusal leaves it empty). On PSID_OK, data_off + data_len <= n and
 * load + data_len <= $10000. */
int psid_parse(const uint8_t* b, uint32_t n, psid_header* h);

/* Song `song` (1-based)'s speed bit: 0 = vertical blank, 1 = CIA 1 timer A. */
int psid_speed_bit(const psid_header* h, uint32_t song);

#ifdef __cplusplus
}
#endif

#endif
