// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_H
#define SIDOU_SID_H

#include <stdint.h>

#include "sid_defs.h"
#include "sid_env.h"
#include "sid_filter.h"
#include "sid_osc.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SID_FC_STEPS 2048

/* One SID chip. Fields are private to sid.c; the struct is public so callers can
 * allocate it statically. */
typedef struct {
    sid_osc osc[3];
    sid_env env[3];
    sid_svf svf;
    sid_model model;
    uint32_t cycles_per_sample_q16;
    uint32_t cycle_frac_q16;
    int32_t wave_zero;
    int32_t voice_dc;
    uint32_t fc;
    uint8_t res;
    uint8_t routing;
    uint8_t mode_vol;
    uint8_t bus; /* last value on the data bus, returned by reads of write-only registers */
    uint8_t osc3_held; /* a write since the last cycle: OSC3 is osc3_latch */
    uint8_t osc3_latch;
    uint8_t control_age;   /* cycles since voice 3's last control write, up to 2 */
    uint32_t tri_saw_latch; /* 8580: voice 3's triangle/saw, latched with the old waveform */
    float volume;
    float dc_r, dc_x1, dc_y1;
    float cutoff_g[SID_FC_STEPS];
} sid;

/* sample_rate must be in [4000, 65535]. The oscillator step needs
 * freq * cycles < 2^24, so at most 256 chip cycles per sample: the minimum is
 * clock_hz / 256, about 3849 Hz for PAL and 3995 Hz for NTSC. The upper bound
 * keeps the Q16 cycle step in range.
 * The chip powers up as reSIDfp's does: each envelope counter at 0xaa, releasing at
 * rate 0 to 0 (the first step on the third cycle, then every 9 cycles, through the
 * exponential divider: about 6 ms), and each accumulator at 0x555555. Until M1b Task 2
 * that release ran one step behind reSIDfp's; it is now cycle-exact. */
void sid_init(sid* s, sid_model model, uint32_t clock_hz, uint32_t sample_rate);
void sid_write(sid* s, uint8_t reg, uint8_t value);
void sid_render(sid* s, int16_t* out, uint32_t frames);

/* Reads a register. $19/$1A (paddles) read 0xFF, $1B is OSC3 (the top 8 bits of voice
 * 3's waveform output, as reSIDfp latches it; 0 with no waveform selected), $1C is
 * ENV3 (voice 3's envelope counter at the start of the last cycle). Like the chip, every
 * read or write leaves its value on the data bus, and the write-only registers read
 * back whatever is there. On the chip that value fades after a while; that is not
 * modelled. */
uint8_t sid_read(sid* s, uint8_t reg);
/* sid_write latches OSC3 (read_osc3, a few dozen instructions) before the first write
 * to voice 2 or 3 after each cycle, so a read before the next cycle sees the old value.
 * On the device that runs wherever writes are applied (the audio ISR draining the
 * register ring). */

/* Combined-waveform profile: SID_PROFILE_WEAK, SID_PROFILE_AVERAGE (reSIDfp's default,
 * and ours) or SID_PROFILE_STRONG, reSIDfp's three sampled chips (gen/grid.h). They
 * are discrete, not points on a scale. Anything else gets AVERAGE, and so do WEAK and
 * STRONG unless the core is built with SIDOU_ALL_COMBINED_PROFILES (61,440 bytes of
 * tables for both models by default, 184,320 with it). On the device, call it where
 * register writes are applied, through the register ring, never concurrently with
 * sid_render. */
void sid_set_combined_profile(sid* s, int profile);

/* Advances the oscillators and envelopes by exactly `cycles` chip cycles, without
 * producing output; the filter and the output stage do not run. sid_render advances
 * the same code a sample's worth of cycles at a time, and both reach the same state,
 * except with hard sync: sync is approximated once per step, and the steps differ
 * (16 cycles here). For reading OSC3 and ENV3 at exact cycles, as the reference tests
 * do. */
void sid_clock(sid* s, uint32_t cycles);

#ifdef __cplusplus
}
#endif

#endif
