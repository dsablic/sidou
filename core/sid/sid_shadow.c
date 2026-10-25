// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Main-loop code, from flash: the envelope tables it reads stay there (sid_env.h). */
#define SID_ENV_TABLES_IN_FLASH
#include "sid_shadow.h"

#include <string.h>

/* The chip's own voice code (sid_voices.h) over the shadow: shadow_step, a step of the voices,
 * and shadow_osc3, OSC3 as sid_read reconstructs it. */
#define SID_VOICES_T sid_shadow
#define SID_VOICES_CLOCK shadow_step
#define SID_VOICES_OSC3 shadow_osc3
#include "sid_voices.h"

void sid_shadow_init(sid_shadow* sh, const sid* chip, uint32_t now) {
    memset(sh, 0, sizeof *sh);
    memcpy(sh->osc, chip->osc, sizeof sh->osc);
    memcpy(sh->env, chip->env, sizeof sh->env);
    sh->vf = chip->vf;
    sh->model = chip->model;
    sh->ttl = chip->model == SID_MODEL_6581 ? SID_SHADOW_TTL_6581 : SID_SHADOW_TTL_8580;
    sh->now = now;
    const uint32_t(*own)[16] = (const uint32_t(*)[16])chip->noise_wb;
    memcpy(sh->noise_wb, chip->noise_wb, sizeof sh->noise_wb);
    for (int v = 0; v < 3; ++v) {
        if (sh->osc[v].writeback == own) {
            sh->osc[v].writeback = (const uint32_t(*)[16])sh->noise_wb;
        }
    }
}

void sid_shadow_clock(sid_shadow* sh, uint32_t x) {
    uint32_t n = x - sh->now;
    if ((int32_t)n <= 0) {
        return;
    }
    sh->now = x;
    /* The bus ages by the whole span at once, as reSIDfp's ageBusValue does in its clock. */
    if (sh->bus_ttl != 0u) {
        if (n >= sh->bus_ttl) {
            sh->bus = 0;
            sh->bus_ttl = 0;
        } else {
            sh->bus_ttl -= n;
        }
    }
    /* Deferred noise shifts, folded as sid_clock folds them (sid.c, fold_owed): a span adds at
     * most one rise per 16 cycles, and a span is under 2^31 cycles. */
    for (int v = 0; v < 3; ++v) {
        if (sh->osc[v].owed >= 0x80000000u) {
            sh->osc[v].owed -= 0x7fffffu * 0x100u;
        }
    }
    /* The controls change only at writes, which clock up to their cycle first. */
    const uint32_t most =
        ((sh->osc[0].control | sh->osc[1].control | sh->osc[2].control) & SID_CTRL_SYNC)
            ? SID_SHADOW_SYNC_STEP
            : SID_SHADOW_STEP;
    while (n) {
        uint32_t step = n < most ? n : most;
        shadow_step(sh, step);
        n -= step;
    }
}

void sid_shadow_write(sid_shadow* sh, uint32_t x, uint32_t reg, uint32_t value) {
    sid_shadow_clock(sh, x);
    uint8_t v = (uint8_t)value;
    sh->bus = v;
    sh->bus_ttl = sh->ttl;
    reg &= 0x1fu;
    if (reg >= 21u) {
        return; /* the filter, the volume and the read-only registers: the bus only */
    }
    /* The voices as sid_write changes them (sid.c; the thump, which is the chip's output
     * stage, aside): OSC3 latched in the last cycle before a write to voice 2 or 3, the control
     * write's age, the pulse latches to clear after the next step. */
    if (reg >= 0x07u && reg <= 0x12u && !sh->vf.osc3_held) {
        sh->vf.osc3_latch = shadow_osc3(sh);
        sh->vf.osc3_held = 1;
    }
    if (reg == 0x12u) {
        sh->vf.control_age = 0;
    }
    sh->vf.written = 1;
    sid_osc* o = &sh->osc[reg / 7u];
    sid_env* e = &sh->env[reg / 7u];
    switch (reg % 7u) {
    case 0: sid_osc_write_freq(o, (o->freq & 0xff00u) | v); break;
    case 1: sid_osc_write_freq(o, (o->freq & 0x00ffu) | ((uint32_t)v << 8)); break;
    case 2: sid_osc_write_pw(o, (o->pw & 0xf00u) | v); break;
    case 3: sid_osc_write_pw(o, (o->pw & 0x0ffu) | ((uint32_t)(v & 0x0f) << 8)); break;
    case 4:
        /* The ring source as this voice's output sees it (sid_osc_output). */
        sid_osc_latch_ramp(o, reg >= 7u ? sh->osc[reg / 7u - 1u].acc
                                        : sid_osc_acc_last(&sh->osc[2]));
        sid_osc_write_control(o, v);
        sid_env_set_gate(e, v & SID_CTRL_GATE);
        break;
    case 5: sid_env_write_ad(e, v); break;
    default: sid_env_write_sr(e, v); break;
    }
}

uint32_t sid_shadow_read(sid_shadow* sh, uint32_t x, uint32_t reg) {
    sid_shadow_clock(sh, x);
    switch (reg & 0x1fu) {
    case 0x19:
    case 0x1a: sh->bus = 0xff; break; /* paddles: nothing connected */
    case 0x1b: sh->bus = sh->vf.osc3_held ? sh->vf.osc3_latch : shadow_osc3(sh); break;
    case 0x1c: sh->bus = sh->env[2].env3; break;
    default:
        /* A write-only or unused register: the bus, which the read discharges faster (reSIDfp
         * halves what is left of its lifetime). */
        sh->bus_ttl /= 2u;
        return sh->bus;
    }
    sh->bus_ttl = sh->ttl;
    return sh->bus;
}
