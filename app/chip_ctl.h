// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* One chip's setup, kept or followed (M2b Task 4): what the chip has (its profile, the
 * character's four controls, the gate reset, whether its first reset has reached it), and a
 * tick that brings it to a setup it should have.
 *
 * Two uses (app/keys_route.h): the chip the keys play gets its setup from live, which writes
 * through the keys' sink, and chip_ctl_saw records what reached it; the other chip follows the
 * keys' setup in the background (chip_ctl_follow), so the keys sound the same whichever chip
 * they move to. Following prepares the chip a slice a tick through its preparer
 * (live_prepare_fn: reg_ring_prepare_step on the device), sends the reset when it is whole,
 * and every tick the controls that changed and LIVE_REG_CHAR_TICK, so the chip's ramp takes
 * it there. A reset is never sent unprepared: the chip's first one included (chip B's at
 * boot). It writes no SID register: whoever plays the chip writes those.
 *
 * Portable C with no hardware access. Main context. */
#ifndef SIDOU_CHIP_CTL_H
#define SIDOU_CHIP_CTL_H

#include <stdint.h>

#include "live.h"

#ifdef __cplusplus
extern "C" {
#endif

/* A chip setup: a profile (a SID_CHIP_*, the model follows it), the character's controls
 * (age, DAC, thump, bleed, 0..127) and the gate reset (1: live's INSTANT envelope). */
typedef struct {
    uint8_t profile;
    uint8_t controls[LIVE_CHAR_FIELDS - 1u];
    uint8_t gate_reset;
    uint8_t expert[SID_EXPERT_COUNT];
} chip_setup;

/* The keys' setup: live's, as its chip has it (the profile applied, the controls asked for,
 * the envelope mode). */
void chip_setup_of(const live* l, chip_setup* out);
void chip_setup_control(chip_setup* setup, uint32_t field, uint8_t value);

typedef struct {
    live_sink_fn sink;
    void* sink_ctx;
    live_prepare_fn prepare;
    void* prepare_ctx;
    chip_setup has; /* what the chip has, as last sent to it */
    uint8_t made;   /* 1 once a reset has reached it */
    uint8_t preparing; /* 1 while chip_ctl_follow waits for its preparation */
} chip_ctl;

/* Nothing sent yet: the chip is not made. */
void chip_ctl_init(chip_ctl* c, live_sink_fn sink, void* sink_ctx, live_prepare_fn prepare,
                   void* prepare_ctx);
/* A write that reached the chip from its player (the keys: live through keys_route): the
 * character's settings, the reset, the gate reset are recorded. */
void chip_ctl_saw(chip_ctl* c, uint8_t reg, uint8_t value);
/* 1 if the chip is made with the setup's profile and gate reset and has been sent its controls
 * (its ramp may still be on its way to them). */
int chip_ctl_has(const chip_ctl* c, const chip_setup* want);
/* One tick towards want, once a tick at most: the controls that changed, a slice of the
 * preparation and, once it is whole, the profile, the reset and the gate reset; the gate
 * reset alone if only it changed; then the character's tick. 1 when chip_ctl_has. */
int chip_ctl_follow(chip_ctl* c, const chip_setup* want);
/* The controls and the gate reset that differ from want, sent once (no tick, no reset): for
 * the keys' own chip, whose ticks are live's. */
void chip_ctl_send_settings(chip_ctl* c, const chip_setup* want);
/* Stops following for now: a preparation it waits for is dropped (the chip becomes the keys'
 * or a song's). */
void chip_ctl_stop(chip_ctl* c);

#ifdef __cplusplus
}
#endif

#endif
