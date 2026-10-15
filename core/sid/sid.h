// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SID_H
#define SIDOU_SID_H

#include <stdint.h>

#include "sid_dac.h"
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
    uint32_t sample_rate;
    uint32_t fc;
    uint8_t res;
    uint8_t routing;
    uint8_t mode_vol;
    uint8_t bus; /* last value on the data bus, returned by reads of write-only registers */
    uint8_t osc3_held; /* a write since the last cycle: OSC3 is osc3_latch */
    uint8_t written;   /* a voice write since the last step: clear the pulse latches */
    uint8_t osc3_latch;
    uint8_t control_age;   /* cycles since voice 3's last control write, up to 2 */
    uint8_t filter_curve;  /* the curve setting cutoff_g and res_k come from (gen/filter.h) */
    uint8_t filter_toward; /* the one cutoff_x_to and res_k_to come from */
    uint8_t filter_dirty;  /* a write to $15-$17 since the coefficients were computed */
    float volume;
    float output_gain; /* to int16, at reSIDfp's level for the model */
    float dc_r, dc_x1, dc_y1;
    /* The filter's tables, in RAM for sid_update_filter, for two curve settings, and how far
     * the filter is from the first towards the second (filter_t, 0..1; sid_set_filter_blend):
     * the first's cutoff per FC value as the SVF's g, tan(pi f / fs), the second's as the
     * angle, x = pi f / fs (cutoff_g = tan x; the blend is of x, so of Hz), and each's damping
     * per resonance value (sid_resonance_k). sid_init sets both to the default curve setting
     * and filter_t to 0. */
    float cutoff_g[SID_FC_STEPS];
    float res_k[16];
    float cutoff_x_to[SID_FC_STEPS];
    float res_k_to[16];
    float filter_t;
    const sid_dac* dac; /* shared between chips: sid_dac_default, or sid_set_dac */
    /* The 6581's output path (M1b Task 6; the 8580 does not use these): its filter's
     * non-linearity (sid_filter.h), the mix of the filter's outputs as $18 and $17 set it
     * (mix_*: each mode's gain and polarity, and its op-amps' leakage between the modes),
     * the DC load the inputs routed into the filter put on it (dc_in), and the external
     * filter's low-pass (out_c, out_lp). */
    sid_nonlin nl;
    float mix_lp, mix_bp, mix_bpn, mix_hp, dc_in;
    float out_c, out_lp;
    /* The character's output stage (sid_character.h; both models, 0 from sid_init): thump,
     * a DC the volume register scales (in voice DAC units, sid_dac.h), so a volume change
     * steps the output (sid_write puts the step into the DC blocker: nothing per sample, and
     * the 6581's 13 kHz output pole does not smooth it). A thump change itself only moves
     * output_gain (the headroom, sid_character.h); each volume write's step uses the thump in
     * force at that write, so a step up and the step down again need not cancel when the
     * thump changed between them: what is left is a DC, which the DC blocker removes. Bleed:
     * the share of the unfiltered voices that also reaches the filter's input (a multiply and
     * an add per sample, in a copy of the render that runs only while it is not 0). */
    float thump, bleed;
} sid;

/* The 6581's band-pass mix gain (sid.c: its filter outputs against its unfiltered voices). */
#define SID_MIX_GAIN_BP 0.8125f

/* The cutoff at an FC value, as the SVF's g, t of the way from the first curve setting's
 * towards the second's in Hz (sid_set_filter_blend): at t 0 the first's table entry exactly,
 * else tan of the angles' blend (sidou_atanf, sidou_tanf_fast: about 40 instructions). */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline float sid_blended_cutoff(const sid* s, uint32_t fc, float t) {
    float g = s->cutoff_g[fc];
    if (t != 0.0f) {
        float x = sidou_atanf(g);
        g = sidou_tanf_fast(x + t * (s->cutoff_x_to[fc] - x));
    }
    return g;
}

/* The filter's coefficients from its tables at the chip's FC and resonance, filter_t of the
 * way from the first curve setting's towards the second's (the cutoff in Hz and the damping
 * blended; at 0 the first's exactly), and the 6581's band-pass mix, which follows the
 * damping. A write to $15-$17 only sets filter_dirty, and sid_render_tap runs this once
 * before the block's first sample: up to about 70 instructions at an age/heat blend (an atan
 * and a tan), so a flood of filter writes costs the audio interrupt one, not one a write.
 * That is what recomputing at every write would play, bit for bit: the coefficients depend
 * only on the registers and tables, a render reads them only at its start, and the drain
 * applies a block's writes before it. Inline: sid_render_tap (the audio path, in SRAM on the
 * device), the main context and sid_character_commit in the register ring's drain (SRAM) all
 * call it, and no direct call may cross between SRAM and flash (platform/fm1/app.ld). */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((always_inline))
#endif
static inline void sid_update_filter(sid* s) {
    const float t = s->filter_t;
    float g = sid_blended_cutoff(s, s->fc, t), k = s->res_k[s->res];
    if (t != 0.0f) {
        k += t * (s->res_k_to[s->res] - k);
    }
    if (s->model == SID_MODEL_6581) {
        sid_svf_set_6581(&s->svf, &s->nl, g, k, s->dc_in);
        float bp = (s->mode_vol & 0x20u) ? 1.0f : 0.0f;
        s->mix_bpn = SID_MIX_GAIN_BP * bp / s->svf.k;
    } else {
        sid_svf_set(&s->svf, g, k);
    }
}

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

/* A tap on a render (sid_render_tap), with the chip as the samples so far left it: its
 * voices (osc, env) are current; its filter and output stage may not be stored back yet. */
typedef void (*sid_tap)(void* ctx, const sid* s);
/* sid_render, calling tap after every `every` samples (every >= 1) and after the last,
 * shorter chunk: the samples are exactly sid_render's. One call for a block, however many
 * taps, so a tap costs its own work and nothing of a render's per-call setup (app/scope.c,
 * the LIVE view's tap). The tap runs inside the render: on the device, in the audio
 * interrupt, so it is short and in SRAM (platform/fm1/app.ld). */
void sid_render_tap(sid* s, int16_t* out, uint32_t frames, uint32_t every, sid_tap tap,
                    void* ctx);

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
 * STRONG unless the core is built with the model's flag: SIDOU_ALL_COMBINED_PROFILES for
 * the 6581 (on by default: the chip profiles play all three), SIDOU_ALL_COMBINED_PROFILES_8580
 * for the 8580 (off: its profile plays AVERAGE). 30,720 bytes of tables per model and
 * profile. On the device, call it where
 * register writes are applied, through the register ring, never concurrently with
 * sid_render. */
void sid_set_combined_profile(sid* s, int profile);

/* Gate reset, an option the real chip does not have: with it on, each gate edge restarts
 * the voice's envelope rate counter, so the attack (rising) or the release (falling) takes
 * its first step one rate period and 3 cycles after the edge (12 cycles at attack 0) instead
 * of waiting up to 32767 cycles for the counter to come round past a shorter period (the
 * ADSR delay bug; sid_env.h). Any nonzero `on` is on. Off is the
 * chip and reSIDfp, bit for bit; sid_init turns it off, so set it again after every
 * sid_init. Applies to all three voices. On the device, call it where register writes are
 * applied (the audio interrupt draining the register ring). */
void sid_set_gate_reset(sid* s, int on);

/* Selects the chip's filter curve setting (sid_filter_curves; 0, reSIDfp's default, is
 * what sid_init sets): rebuilds its cutoff and damping tables, about 2048 tangents, as
 * sid_set_filter_blend(s, curve, curve, 0) (the second tables a copy). Returns
 * 0, or -1 for a curve the model does not have, changing nothing. Never from the audio
 * path (on the device it is in flash, and too slow for the interrupt): call it from the
 * main loop. It changes only the chip's tables, one float at a time, which the render reads
 * (in the audio interrupt on the device) one entry after a filter write, so an update
 * meanwhile takes an old or a new entry. The filter's coefficients follow at the next write
 * to $15-$17 (from the render after it):
 * rewrite one (through the register ring on the device) to apply the curve. */
int sid_set_filter_curve(sid* s, uint32_t curve);

/* sid_set_filter_curve between two curve settings: builds the tables for curve (2048
 * tangents) and for toward (its angles: no tangent) and sets how far the filter is from the
 * first towards the second, t (clamped to [0, 1]; NaN is 0). At each write to $15-$17 the
 * cutoff in Hz and the damping are then (1 - t) of curve's plus t of toward's
 * (sid_update_filter, which blends the angles, pi f / fs, each end clamped to the tables'
 * 30 Hz .. 0.45 fs): t 0 is curve's table entries exactly, so the reSIDfp comparisons at a
 * profile's own setting hold bit for bit. M1c Task 3's age/heat moves t alone, a number in
 * the chip's stage, as it plays (sid_character_commit): no table is built. The chip
 * keeps curve as its filter_curve and toward as its filter_toward. Returns 0, or -1 for a
 * curve the model does not have, changing nothing. The same contract as
 * sid_set_filter_curve: main loop only, the coefficients follow at the next write to $15-$17
 * (or sid_filter_refresh). */
int sid_set_filter_blend(sid* s, uint32_t curve, uint32_t toward, float t);

/* Builds the chip's second tables (sid_set_filter_blend's toward: angles, no tangent) for
 * toward, unless they are toward's already (filter_toward), and keeps the first and
 * filter_t: for a chip whose first curve setting is right (sid_character_filter after
 * sid_init). Returns 0, or -1 for a curve the model does not have, changing nothing.
 * sid_set_filter_blend's contract. */
int sid_set_filter_toward(sid* s, uint32_t toward);

/* Recomputes the filter's coefficients and the 6581's output mix from the tables and the
 * registers, as a write to $17 would, without touching the data bus. Main context, on a
 * chip that is not rendering (sid_character_apply after sid_init); in flash on the device. */
void sid_filter_refresh(sid* s);

/* The output gain sid_init sets for the chip's model (int16 per unit of the mix): what
 * sid_character scales for the thump's headroom. */
float sid_base_output_gain(const sid* s);

/* The non-linearity sid_set_filter_nonlinearity would give the chip for an amount with its
 * filter blended t of the way (filter_t's, sid_set_filter_blend; the lowest cutoff follows
 * the blended cutoff at FC 0), without changing the chip: for code that applies it later
 * (sid_character_stage_for). */
void sid_nonlin_for(const sid* s, float amount, float t, sid_nonlin* out);

/* Scales the 6581 filter's non-linearity (sid_filter.h, sid_nonlin): its cutoff's
 * rise with the swing in the threshold region, its fall under the DC load of the voices
 * routed into it, and its lowest cutoff. 1 is reSIDfp's, what sid_init sets; 0 makes the
 * filter linear; between and beyond scale it (M1c Task 3's chip profiles and its age/heat
 * control blend it), except the lowest cutoff, which the amount scales only up to 1 and
 * which follows the chip's cutoff table at FC 0 (against the default curve setting's:
 * reSIDfp's lowest cutoff moves with its curve and range), so set the filter curve first.
 * The amount is clamped to [0, 2], and NaN is 0: below about -0.5 the
 * filter runs to inf and NaN at low cutoffs, and from about 3 the band-pass saturates at
 * FC 0. The 8580 ignores it.
 * It writes four floats (nl.level, level_off, load, floor), one at a time. A render takes
 * them once, when it starts (sid_render, sid_render_tap), so an update that races one
 * gives that one block some old and some new values: benign. Main context only: it and
 * sidou_tanf are in flash on the device, so never call it from the audio interrupt or
 * through the register ring. sid_init resets the amount to 1, so after a model switch
 * (M1c: prepare_spare runs sid_init on the spare chip) apply the profile's amount to the
 * spare again before the swap. */
void sid_set_filter_nonlinearity(sid* s, float amount);

/* Points the chip at other voice DACs (sid_dac_build), which must outlive it; sid_init
 * sets reSIDfp's defaults for the model (sid_dac_default), shared by every chip. Never
 * rebuild a table (sid_dac_build) while an active chip points at it: sid_render reads it
 * without a lock, so it would play a half-built table. Build a new table and point the
 * chip at it here (through the register ring on the device); reuse the old one only
 * once no chip points at it. */
void sid_set_dac(sid* s, const sid_dac* dac);

/* Advances the oscillators and envelopes by exactly `cycles` chip cycles, without
 * producing output; the filter and the output stage do not run. sid_render advances
 * the same code a sample's worth of cycles at a time, and both reach the same state,
 * except with hard sync, and then only in what the synced voice did earlier in the step:
 * its restart is exact (sid_osc_sync counts the source's overshoot to the end of the
 * step), but its own noise clocks and MSB rise before the sync are not recomputed, and
 * the steps differ (16 cycles here). For reading OSC3 and ENV3 at exact cycles, as the
 * reference tests do; OSC3 on the cycles right after a sync is not exact (read_osc3). */
void sid_clock(sid* s, uint32_t cycles);

#ifdef __cplusplus
}
#endif

#endif
