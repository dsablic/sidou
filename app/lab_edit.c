// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "lab_edit.h"

#include "sid/sid_filter.h"

void lab_edit_clear(lab_edit* e) {
    e->first = 0;
    e->n = 0;
    e->steps = 0;
}

void lab_edit_get(const live* l, uint8_t out[LAB_EDIT_BYTES]) {
    live_sound_get(l, out);
    out[LAB_EDIT_MODE] = l->envelope_mode;
}

/* Entry k (0 the oldest) in the ring. */
static uint32_t slot(const lab_edit* e, uint32_t k) {
    uint32_t i = e->first + k;
    return i < LAB_EDIT_ENTRIES ? i : i - LAB_EDIT_ENTRIES;
}

/* The oldest step dropped, all its entries. */
static void drop_oldest(lab_edit* e) {
    do {
        e->first = (uint8_t)slot(e, 1);
        e->n--;
    } while (e->n && !(e->at[e->first] & LAB_EDIT_FIRST));
    e->steps--;
}

/* 1 when the newest step holds place p. */
static int newest_holds(const lab_edit* e, uint32_t p) {
    for (uint32_t k = e->n; k--;) {
        uint32_t at = e->at[slot(e, k)];
        if ((at & ~LAB_EDIT_FIRST) == p) {
            return 1;
        }
        if (at & LAB_EDIT_FIRST) {
            return 0;
        }
    }
    return 0;
}

uint32_t lab_edit_push(lab_edit* e, const uint8_t before[LAB_EDIT_BYTES],
                       const uint8_t after[LAB_EDIT_BYTES], int merge) {
    int started = merge && e->steps;
    for (uint32_t p = 0; p < LAB_EDIT_BYTES; p++) {
        if (before[p] == after[p] || (started && merge && newest_holds(e, p))) {
            continue;
        }
        /* Room: the oldest steps go, never the one being made (it holds at most
         * LAB_EDIT_BYTES entries, and the ring has more). */
        while (e->n == LAB_EDIT_ENTRIES) {
            drop_oldest(e);
        }
        uint32_t i = slot(e, e->n++);
        e->at[i] = (uint8_t)(p | (started ? 0u : LAB_EDIT_FIRST));
        e->old[i] = before[p];
        if (!started) {
            started = 1;
            merge = 0; /* a new step: nothing in it to look for */
            if (++e->steps > LAB_EDIT_STEPS) {
                drop_oldest(e);
            }
        }
    }
    return (uint32_t)started;
}

uint32_t lab_edit_undo(lab_edit* e, live* l) {
    if (!e->steps) {
        return 0;
    }
    uint8_t s[LAB_EDIT_BYTES];
    lab_edit_get(l, s);
    uint32_t at;
    do {
        uint32_t i = slot(e, --e->n);
        at = e->at[i];
        s[at & ~LAB_EDIT_FIRST] = e->old[i];
    } while (!(at & LAB_EDIT_FIRST));
    e->steps--;
    live_sound_apply(l, s);
    live_param_set(l, LIVE_PARAM_ENVELOPE_MODE, s[LAB_EDIT_MODE]);
    return 1;
}

/* ---- audible ---- */

/* The levels in whole dB, rounded down: measured by lab_edit.rs on both models (a held C4,
 * LAB_SILENT_MS, the chip alone), each the worst of the waveforms and models it stands for. */
#define NONE (-99)
/* The waveform at full envelope, unfiltered, a pulse at 50 %: $d404's bits 4-7. */
static const int8_t WAVE_DB[2][16] = {
    /* 6581: its combinations barely sound; the 8580's with the pulse fall far as the width
     * leaves 50 % (TRI+PUL 30 dB); noise with any other waveform locks */
    {NONE, -22, -22, NONE, -17, NONE, NONE, NONE, -23, NONE, NONE, NONE, NONE, NONE, NONE, NONE},
    {NONE, -24, -24, -27, -19, NONE, NONE, NONE, -25, NONE, NONE, NONE, NONE, NONE, NONE, NONE},
};
/* The attack, relative to full: its ramp alone (decay and sustain 0), and what it costs a
 * sustained sound (the window spent rising). */
static const int8_t RAMP_DB[16] = {-25, -22, -20, -18, -16, -15, -14, -13,
                                   -12, -8,  -5,  -10, -11, -21, -25, -29};
static const int8_t ATTACK_DB[16] = {0, 0, 0, 0, 0, -1, -1, -1, -1, -2, -5, -9, -11, -21, -25, -29};
/* The decay to sustain 0, and the sustain level. */
static const int8_t DECAY_DB[16] = {-25, -22, -20, -18, -16, -15, -14, -13,
                                    -12, -8,  -5,  -3,  -3,  -1,  -1,  -1};
static const int8_t SUSTAIN_DB[16] = {NONE, -21, -17, -14, -12, -10, -8, -7,
                                      -6,   -5,  -4,  -3,  -2,  -2,  -1, 0};

/* The pulse's level against a square's, by one side of it in steps: the whole of it, or (fund)
 * its fundamental's alone (sin(pi side / 4096)), which is what a low-pass near the note leaves. */
static int32_t pulse_db(int32_t side, int fund) {
    static const int16_t AT[7] = {1024, 512, 256, 128, 64, 32, 16};
    static const int8_t DB[2][7] = {{-2, -4, -7, -10, -13, -16, -19},
                                    {-3, -9, -15, -21, -27, -33, -39}};
    for (uint32_t i = 0; i < 7u; i++) {
        if (side >= AT[i]) {
            return DB[fund][i];
        }
    }
    return NONE;
}

/* The filter's loss on a note at note Hz through mode (1..7) at the cutoff hz, on the 8580 or the
 * 6581 (m6581); tri: the triangle, whose harmonics are weak, so a high-pass or a band-pass above
 * the note takes more of it. In half octaves from the note: below it a low-pass (12 dB an octave)
 * and a band-pass (6) lose, above it a band-pass and a high-pass. The 6581's modes lose more
 * even open (its high-pass about 11 dB), and its cutoff stops at about 240 Hz. */
static int32_t filter_db(uint32_t mode, float hz, float note, int tri, int m6581, int* low) {
    int32_t above = 0, below = 0;
    for (float f = note * 1.41421f; f <= hz && above < 24; f *= 1.41421f) {
        above++;
    }
    for (float f = note; f > hz && below < 24; f *= 0.70711f) {
        below++;
    }
    int near = above <= 2 && below <= 2;
    *low = (mode == 1u || mode == 3u) && above <= 4; /* a low-pass within 2 octaves of the note */
    switch (mode) {
    case 5u: /* notch: a dip near the note, deep on a triangle (its fundamental is most of it) */
        if (tri && above <= 1 && below <= 1) {
            return -20;
        }
        return near ? -10 : m6581 ? -3 : -1;
    case 7u: return near ? -10 : -6;               /* all */
    case 1u: /* LP */
    case 3u: /* LP+BP */
        if (below) {
            return (m6581 ? -9 : -5) - 6 * (below - 1);
        }
        if (mode == 3u) {
            return -7;
        }
        /* the 6581's low-pass takes 8 dB off a note up to an octave under its cutoff */
        return !m6581 ? -3 : above <= 2 ? -9 : above <= 4 ? -7 : above <= 6 ? -5 : -4;
    case 4u: /* HP */
        if (m6581) {
            return below ? -11 : tri ? -11 - 2 * above : -11 - above;
        }
        return below ? -2 : tri ? -5 - 5 * above : -2 - 2 * above;
    default: /* BP, BP+HP */
        if (below) {
            return (m6581 ? -6 : -3) - 3 * (below - 1);
        }
        if (m6581) {
            return mode == 6u || tri ? -6 - above : -6 - above / 2;
        }
        return tri ? -4 - (7 * above) / 2 : -2 - 2 * above;
    }
}

uint32_t lab_audible(const uint8_t s[LAB_SOUND_BYTES], uint32_t model) {
    uint32_t wave = s[LAB_SOUND_CONTROL] >> 4;
    int32_t db = WAVE_DB[model ? 1 : 0][wave];
    if (db == NONE) {
        return 0;
    }
    int32_t side = 2048; /* the pulse: its narrowest side, the PWM taking half its swing off */
    if (wave & 4u) {
        int32_t pw = (int32_t)(s[LAB_SOUND_PULSE] | (s[LAB_SOUND_PULSE + 1u] & 15u) << 8);
        int32_t depth = (int32_t)(s[LAB_SOUND_PWM_DEPTH] | s[LAB_SOUND_PWM_DEPTH + 1u] << 8);
        int32_t away = pw > 2048 ? pw - 2048 : 2048 - pw;
        depth += (int8_t)s[LAB_SOUND_PWM_LINK] * away / 8; /* PWM LINK's term */
        depth = depth < 0 ? 0 : depth > (int32_t)LIVE_PWM_DEPTH_MAX ? (int32_t)LIVE_PWM_DEPTH_MAX : depth;
        side = (pw > 2048 ? 4096 - pw : pw) - depth / 2;
        int32_t p = pulse_db(side, 0);
        if (p == NONE) {
            return 0;
        }
        db += p;
    }
    float note = 261.6f; /* C4; SYNC LEAD's one-voice sync sounds at the ratio's pitch */
    if (s[LAB_SOUND_MONO_SYNC]) {
        int32_t semis = (int8_t)s[LAB_SOUND_SYNC_SEMIS];
        semis = semis < -LIVE_RATIO_SEMIS  ? -LIVE_RATIO_SEMIS
                : semis > LIVE_RATIO_SEMIS ? LIVE_RATIO_SEMIS
                                           : semis;
        uint32_t base = (uint32_t)(s[LAB_SOUND_SYNC_BASE] | s[LAB_SOUND_SYNC_BASE + 1u] << 8);
        uint32_t q8 = (uint32_t)(((uint64_t)base * live_ratio_q16[semis + LIVE_RATIO_SEMIS]) >> 16);
        /* the voice heard is cut short by the one that resets it (6 dB, measured against the
         * renders); below a ratio of 1 it never ends a cycle either: its swing is the ratio's */
        if (q8 < 32u) {
            return 0;
        }
        db += q8 >= 256u ? -6 : q8 >= 128u ? -12 : q8 >= 64u ? -18 : -24;
        note *= (float)q8 * (1.0f / 256.0f);
    }
    uint32_t a = s[LAB_SOUND_AD] >> 4, d = s[LAB_SOUND_AD] & 15u, su = s[LAB_SOUND_SR] >> 4;
    int32_t held = DECAY_DB[d];
    if (su && SUSTAIN_DB[su] > held) {
        held = SUSTAIN_DB[su];
    }
    held += ATTACK_DB[a];
    db += held > RAMP_DB[a] ? held : RAMP_DB[a];
    if (s[LAB_SOUND_ARP + LIVE_ARP_ON]) {
        db -= 3; /* the arpeggio plays the note again and again: each step's attack again */
    }
    if (s[LAB_SOUND_FILTER_VOICES] & 7u) { /* a voice routed: the note may be one of them */
        uint32_t mode = s[LAB_SOUND_FILTER_MODE] >> 4 & 7u;
        if (!mode) {
            return 0; /* routed to no output */
        }
        uint32_t fc = (uint32_t)(s[LAB_SOUND_CUTOFF] | s[LAB_SOUND_CUTOFF + 1u] << 8) & 2047u;
        sid_model m = model ? SID_MODEL_8580 : SID_MODEL_6581;
        int low;
        int32_t f = filter_db(mode, sid_cutoff_hz(m, 0, fc), note, (int)(wave & 1u), !model, &low);
        uint32_t lfo = s[LAB_SOUND_FILTER_LFO];
        if (lfo) {
            /* the filter LFO: the worst of its sweep's two ends, K1's walk lfo detents each way */
            const uint16_t* t = live_cutoff_steps[m];
            int32_t pos = 0;
            while (pos < 64 && t[pos + 1] <= fc) {
                pos++;
            }
            int32_t off = (int32_t)fc - t[pos];
            for (int32_t way = -1; way <= 1; way += 2) {
                int32_t at = pos + way * (int32_t)lfo;
                int32_t r = t[at < 0 ? 0 : at > 64 ? 64 : at] + off;
                int end_low;
                int32_t e = filter_db(mode, sid_cutoff_hz(m, 0, (uint32_t)(r < 0 ? 0 : r > 2047 ? 2047 : r)),
                                      note, (int)(wave & 1u), !model, &end_low);
                if (e < f) {
                    f = e;
                    low = end_low;
                }
            }
        }
        db += f;
        if (low && (wave & 4u)) { /* a narrow pulse's level is in its partials, which it takes */
            db += pulse_db(side, 1) - pulse_db(side, 0);
        }
        /* a combination or the ring puts the sound's partials where the curves above do not
         * follow them: 6 dB more */
        if ((wave & (wave - 1u)) || (s[LAB_SOUND_CONTROL] & 0x04u)) {
            db -= 6;
        }
    }
    return db >= LAB_AUDIBLE_DB;
}

uint32_t lab_edit_hash(const uint8_t s[LAB_SOUND_BYTES]) {
    uint32_t h = 2166136261u;
    for (uint32_t i = 0; i < LAB_SOUND_BYTES; i++) {
        h = (h ^ s[i]) * 16777619u;
    }
    return (h ^ h >> 16) & 0xFFFFu;
}
