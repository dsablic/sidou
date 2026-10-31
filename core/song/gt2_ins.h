// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_GT2_INS_H
#define SIDOU_GT2_INS_H

#include "gt2_sng.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    const gt2_song* song;
    uint32_t length;
    uint8_t instrument;
    uint8_t start[GT2_TABLES];
    uint8_t rows[GT2_TABLES];
} gt2_ins_export;

gt2_err gt2_ins_export_begin(gt2_ins_export* out, const gt2_song* song, uint32_t instrument);
uint32_t gt2_ins_export_read(void* ctx, uint32_t offset, uint8_t* buf, uint32_t length);

typedef struct {
    gt2_song* song;
    gt2_read_fn read;
    void* ctx;
    uint32_t saved, index;
    uint16_t offset[GT2_TABLES], pattern, row, end;
    uint8_t instr[GT2_INSTR_BYTES];
    uint8_t rows[GT2_TABLES], dest[GT2_TABLES];
    uint8_t instrument, phase, table;
    gt2_err error;
} gt2_ins_import_state;

gt2_err gt2_ins_import_begin(gt2_ins_import_state* state, gt2_song* song, uint32_t instrument,
                             gt2_read_fn read, void* ctx, uint32_t length, int owned);
int gt2_ins_import_step(gt2_ins_import_state* state);
gt2_err gt2_ins_import(gt2_song* song, uint32_t instrument, gt2_read_fn read,
                       void* ctx, uint32_t length);
gt2_err gt2_ins_import_owned(gt2_song* song, uint32_t instrument, gt2_read_fn read,
                             void* ctx, uint32_t length);

#ifdef __cplusplus
}
#endif
#endif
