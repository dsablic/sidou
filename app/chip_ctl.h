// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_CHIP_CTL_H
#define SIDOU_CHIP_CTL_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint8_t profile;
    uint8_t controls[LIVE_CHAR_FIELDS - 1u];
    uint8_t gate_reset;
    uint8_t expert[SID_EXPERT_COUNT];
} chip_setup;

void chip_setup_of(const live* l, chip_setup* out);
void chip_setup_control(chip_setup* setup, uint32_t field, uint8_t value);

typedef struct {
    live_sink_fn sink;
    void* sink_ctx;
    live_prepare_fn prepare;
    void* prepare_ctx;
    chip_setup has;
    uint8_t made;
    uint8_t preparing;
} chip_ctl;

void chip_ctl_init(chip_ctl* c, live_sink_fn sink, void* sink_ctx, live_prepare_fn prepare,
                   void* prepare_ctx);
void chip_ctl_saw(chip_ctl* c, uint8_t reg, uint8_t value);
int chip_ctl_has(const chip_ctl* c, const chip_setup* want);
int chip_ctl_follow(chip_ctl* c, const chip_setup* want);
void chip_ctl_send_settings(chip_ctl* c, const chip_setup* want);
void chip_ctl_stop(chip_ctl* c);

#ifdef __cplusplus
}
#endif

#endif
