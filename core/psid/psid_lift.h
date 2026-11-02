// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_PSID_LIFT_H
#define SIDOU_PSID_LIFT_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PSID_LIFT_FRAMES 50u
#define PSID_LIFT_REGS 11u
#define PSID_LIFT_DIGI 8u
#define PSID_LIFT_FILE_MAX 256u
#define PSID_LIFT_ROWS 112u
#define PSID_LIFT_CHIP_REGS 25u
#define PSID_LIFT_RECORD (PSID_LIFT_CHIP_REGS + 2u)
#define PSID_LIFT_PAST 64u
#define PSID_LIFT_NO_VOICE 3u

enum {
    PSID_LIFT_FREQ_LO = 0,
    PSID_LIFT_FREQ_HI,
    PSID_LIFT_PW_LO,
    PSID_LIFT_PW_HI,
    PSID_LIFT_CTRL,
    PSID_LIFT_AD,
    PSID_LIFT_SR,
    PSID_LIFT_FC_LO,
    PSID_LIFT_FC_HI,
    PSID_LIFT_RES,
    PSID_LIFT_MODE
};

enum {
    PSID_LIFT_OFF = 0,
    PSID_LIFT_ARMED,
    PSID_LIFT_TAKING,
    PSID_LIFT_TAKEN
};

#define PSID_LIFT_AGAIN 0
#define PSID_LIFT_E_DIGI (-1)
#define PSID_LIFT_E_EMPTY (-2)
#define PSID_LIFT_F_NO_VIBRATO 0x01u
#define PSID_LIFT_F_VIBRATO 0x02u
#define PSID_LIFT_F_FILTER 0x04u
#define PSID_LIFT_F_SHORT 0x08u

typedef struct {
    uint8_t rec[PSID_LIFT_PAST][PSID_LIFT_RECORD];
    uint8_t head;
    uint8_t filled;
} psid_lift_past;

typedef struct {
    psid_lift_past* past;
    uint8_t state;
    uint8_t voice;
    uint8_t chips;
    uint8_t chip;
    uint8_t frames;
    uint8_t digi;
    uint8_t base;
    uint8_t cut;
    uint8_t step;
    uint8_t fit_next;
    uint8_t fit_last;
    uint8_t fit_speed;
    uint8_t fit_shift;
    uint8_t centre;
    uint8_t start;
    uint8_t delay;
    uint16_t pos;
    uint32_t flags;
    uint32_t best;
    uint8_t edge[2];
    uint8_t before[2];
    uint8_t regs[2][PSID_LIFT_CHIP_REGS];
    uint8_t count[2][PSID_LIFT_CHIP_REGS];
    uint8_t frame[PSID_LIFT_FRAMES][PSID_LIFT_REGS];
    uint8_t file[PSID_LIFT_FILE_MAX];
    uint8_t row[2][PSID_LIFT_ROWS];
    uint8_t note[PSID_LIFT_FRAMES];
    uint8_t kind[PSID_LIFT_FRAMES];
} psid_lift;

void psid_lift_init(psid_lift* l, psid_lift_past* past, uint32_t chips);
void psid_lift_forget(psid_lift* l);
uint32_t psid_lift_take(psid_lift* l, uint32_t voice);
void psid_lift_write(psid_lift* l, uint32_t chip, uint32_t reg, uint32_t value);
void psid_lift_call(psid_lift* l);
void psid_lift_stop(psid_lift* l);

int psid_lift_convert(psid_lift* l, int ntsc, const char* name, uint32_t* flags);

#ifdef __cplusplus
}
#endif

#endif
