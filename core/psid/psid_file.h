/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-FileCopyrightText: 2026 Joakim Eriksson
 * SPDX-License-Identifier: GPL-3.0-or-later AND BSD-2-Clause */
/* Derived from FM-1 SID's player (psid.c, fm1-emu), BSD-2-Clause, Joakim Eriksson.
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
 * OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE. */
#ifndef SIDOU_PSID_FILE_H
#define SIDOU_PSID_FILE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
    PSID_OK = 0,
    PSID_E_NOT_SID,
    PSID_E_RSID,
    PSID_E_HEADER,
    PSID_E_MUS,
    PSID_E_TOO_LONG,
    PSID_E_LOAD
};

#define PSID_W_START 0x01u
#define PSID_W_PLAYSID 0x02u
#define PSID_W_SID2 0x04u
#define PSID_W_SID3 0x08u
#define PSID_W_MORE_SIDS 0x10u
#define PSID_W_FM_OPL 0x20u

#define PSID_HEADER_SIZE_V1 0x76u
#define PSID_HEADER_SIZE_V2 0x7Cu
#define PSID_STRING_MAX 64u

typedef struct psid_header {
    uint16_t version;
    uint16_t load;
    uint16_t init;
    uint16_t play;
    uint16_t songs;
    uint16_t start;
    uint16_t flags;
    uint32_t speed;
    uint32_t data_off;
    uint32_t data_len;
    uint32_t lengths_off;
    uint16_t sid2;
    uint16_t sid3;
    uint8_t clock;
    uint8_t model1;
    uint8_t model2;
    uint8_t model3;
    uint8_t sids;
    uint8_t warn;
    uint8_t speed_repeats;
    char name[PSID_STRING_MAX];
    char author[PSID_STRING_MAX];
    char released[PSID_STRING_MAX];
} psid_header;

int psid_parse(const uint8_t* b, uint32_t n, psid_header* h);

int psid_speed_bit(const psid_header* h, uint32_t song);

#ifdef __cplusplus
}
#endif

#endif
