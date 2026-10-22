// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The chips' thump on the final output (platform/fm1/audio.c): after the chips, chip B's mix
 * and the effect slots, before MASTER.
 *
 * The character's thump (core/sid/sid.h thump_pending, sid_character.h) is a step at each
 * note's start and volume write that the chip's DC blocker turns into a decaying bump. The
 * chips leave it out of their render; here each chip's steps are taken (at its output gain)
 * and played as its blocker would have played them, a step that decays by the chip's dc_r a
 * sample. Played last, the thump is fitted into the room the output leaves: a sample of the
 * stereo output s gets the thump x where |s + x| stays within THUMP_OUT_CEILING with room to
 * spare, and the part of x beyond half the room left is softly limited (x' = h + d h / (d + h),
 * h half the room, d the excess), so the output nears the ceiling but never reaches it. A
 * signal without a thump passes bit for bit, a sample already on a rail stays as it is, and no
 * thump clips: not on two chips at once, not after an effect's gain, at no setting.
 *
 * The thump does not pass through the slots (an echo or a reverb does not repeat it, a
 * distortion does not shape it): it is a low bump under the note's start, not part of the
 * voice. Nothing runs while no thump sounds (the stage is idle once a bump has decayed under
 * half an LSB, which no longer changes a sample). Portable C with no hardware access;
 * thump_out_apply runs in the audio interrupt (platform/fm1/app.ld puts it in SRAM). */
#ifndef SIDOU_THUMP_OUT_H
#define SIDOU_THUMP_OUT_H

#include <stdint.h>

#include "sid/sid.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The largest magnitude a thump takes a sample to: one inside the rails, so no thump puts a
 * sample on one. */
#define THUMP_OUT_CEILING 32766

typedef struct {
    float level[2];     /* each chip's thump in LSBs on the next sample */
    float r[2];         /* each chip's decay a sample: its DC blocker's dc_r */
    float late[2];      /* a step that starts in the block at late_at (a song's write) */
    uint32_t late_at[2];
} thump_out;

/* Takes chip `chip`'s (0 or 1) thump steps since the last take (sid.h thump_pending and
 * thump_late, cleared) at its output gain, decaying at its DC blocker's rate: a step written
 * before the chip rendered from the block's first sample, one written after it rendered n
 * samples from sample n. After the chip's block is rendered, before thump_out_apply. */
void thump_out_take(thump_out* t, uint32_t chip, sid* s);

/* Clears a chip's thump steps without playing them (a chip that did not render: none of its
 * output was heard). */
void thump_out_drop(sid* s);

/* Adds the thump to `frames` interleaved stereo frames (L, R) in place, as above. Nothing
 * is touched while no thump sounds. */
void thump_out_apply(thump_out* t, int16_t* lr, uint32_t frames);

#ifdef __cplusplus
}
#endif

#endif
