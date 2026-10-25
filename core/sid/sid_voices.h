// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

/* The voices' own code, shared by the chip (sid.c) and the voice shadow (sid_shadow.c): a step
 * of the three oscillators and envelopes, and OSC3 as a read reconstructs it. A template, which
 * each of the two files includes once with its own names:
 * - SID_VOICES_T, the struct the code works on: one with osc[3], env[3], vf (sid_vflags) and
 *   model, as sid and sid_shadow both have;
 * - SID_VOICES_CLOCK and SID_VOICES_OSC3, the names of the two functions it defines.
 * sid.c's are clock_voices and read_osc3, in SRAM on the device (platform/fm1/app.ld); the
 * shadow's are its own, in flash. A template rather than functions of (osc, env, flags): over
 * one struct pointer the compiler gives the chip's code exactly what it gave before the shadow
 * shared it (with three pointers it reordered the hard-sync test, 3 instructions a sample dearer
 * in sid_budget's sync scenario). Private to those two files. */

#if !defined(SID_VOICES_T) || !defined(SID_VOICES_CLOCK) || !defined(SID_VOICES_OSC3)
#error "sid_voices.h: define SID_VOICES_T, SID_VOICES_CLOCK and SID_VOICES_OSC3 first"
#endif

#include <stdint.h>

#include "sid.h"

/* Advances the voices `cycles` chip cycles (1 to 256: sid_osc_advance needs freq * cycles <
 * 2^24) as one step: the voices' sync and ring modulation from the voice before each, the pulse
 * latches of the writes since the last step cleared, and each voice's waveform output of the
 * step's last cycle. */
static inline void SID_VOICES_CLOCK(SID_VOICES_T* s, uint32_t cycles) {
    s->vf.osc3_held = 0;
    if (s->vf.control_age < 2u) { /* 2 from the third cycle on: no store once it is there */
        s->vf.control_age =
            (uint8_t)(s->vf.control_age + cycles > 2u ? 2u : s->vf.control_age + cycles);
    }
    sid_osc* o = s->osc;
    /* Each voice's sync and ring modulation source is the voice before it. */
    uint32_t acc2 = o[2].acc;
    /* Voices 2 and 3 see their source after its MSB pull (reSIDfp's output order): its lower
     * half while it pulls every rise low (pull_low is 0 or 1, so a shift, not a select). */
    o[2].src_mask = 0xffffffu >> o[1].pull_low;
    o[1].src_mask = 0xffffffu >> o[0].pull_low;
    sid_osc_advance(&o[2], cycles, o[1].acc, o[1].freq);
    sid_osc_advance(&o[1], cycles, o[0].acc, o[0].freq);
    sid_osc_advance(&o[0], cycles, acc2, o[2].freq);
    for (int v = 0; v < 3; ++v) {
        const sid_osc* src = &o[v ? v - 1 : 2];
        if ((o[v].control & SID_CTRL_SYNC) && src->msb_rose) {
            sid_osc_sync(&o[v], src);
        }
        sid_env_advance(&s->env[v], cycles);
    }
    if (s->vf.written) {
        /* Pulse comparisons latched by writes (sid_osc_write_pw/_control) last until
         * after the step that follows them. */
        o[0].pulse_held = o[1].pulse_held = o[2].pulse_held = 0;
        s->vf.written = 0;
    }
    /* The output feeds the voice DAC, which leaks it even with the envelope at 0, and
     * the noise write-back (a shift's after a control write takes the old waveform's
     * last output). With no waveform selected it is 0; reSIDfp holds its floating DAC
     * value instead, which SIDOU does not keep (see SID_VOICES_OSC3). */
    o[0].out = sid_osc_output(&o[0], sid_osc_acc_last(&o[2]));
    o[1].out = sid_osc_output(&o[1], o[0].acc);
    o[2].out = sid_osc_output(&o[2], o[1].acc);
}

/* OSC3 as reSIDfp latches it. On the 6581 that is the waveform output of the last
 * cycle. On the 8580 the triangle and sawtooth reach it a cycle late: it combines the
 * previous cycle's triangle/saw with the pulse and noise of the last one (whose pulse
 * comparison is itself a cycle late on both models). Noise combined with other
 * waveforms runs cycle by cycle and keeps its OSC3. Otherwise the previous cycle is
 * reconstructed from the current state. A read right after a write to voice 2 or 3,
 * before any cycle, returns the value latched before the write (sid_write). A read a
 * cycle or two later is wrong after:
 * - a frequency or pulse width write (the old values drove those cycles; the pulse
 *   comparison sid_write latches, pulse_held, is not used here);
 * - a control write: the pulse comparison from before a TEST change (the 8580's
 *   triangle/saw latch, tri_saw, is exact: it holds the last triangle/saw value,
 *   0x555 from power-on, and is used for the one cycle after the write);
 * - a hard sync of voice 3 or voice 2 (with ring modulation): the previous cycle is
 *   rebuilt from the restarted accumulator, which has lost it (one read per sync on the
 *   6581, two on the 8580; register_reads.rs has both gaps as ignored tests).
 * With no waveform selected OSC3 reads 0. reSIDfp instead holds the last output on its
 * floating DAC input and fades it after 54,000 (6581) or 800,000 (8580) cycles; SIDOU
 * keeps 0 because programs rarely read OSC3 with the waveform off, and holding the
 * value would cost state on the audio path. */
static uint8_t SID_VOICES_OSC3(const SID_VOICES_T* s) {
    const sid_osc* o = &s->osc[2];
    const sid_osc* src = &s->osc[1];
    uint32_t wf = (uint32_t)o->control >> 4;
    if (!wf) {
        return 0;
    }
    if (o->cycle_mode) {
        return (uint8_t)(o->osc3 >> 4);
    }
    uint32_t before = sid_osc_acc_before(o);
    uint32_t pulse = sid_osc_pulse(o, before);
    uint32_t out;
    if (s->model == SID_MODEL_8580 && (wf & 3u)) {
        uint32_t ramp = s->vf.control_age == 1
                            ? o->tri_saw
                            : sid_wave_ramp(wf, sid_wave_index(o->control, before,
                                                               sid_osc_acc_before(src)));
        out = sid_osc_combine_ramp(o, ramp, pulse, o->noise);
    } else {
        uint32_t ix = sid_wave_index(o->control, sid_osc_acc_last(o), src->acc);
        out = sid_osc_combine(o, ix, pulse, o->noise);
    }
    return (uint8_t)(out >> 4);
}

