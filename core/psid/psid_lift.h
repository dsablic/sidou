// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* An instrument lifted from a .sid tune (1.5 design, section 3; docs/manual/sid-tunes.md, "Lift
 * a sound"): one voice's next note captured from the tune's writes as the 6502 makes them, and
 * turned into a GoatTracker 2 instrument (GTI5, gt2_ins.h's format) the song can import.
 *
 * The capture. Armed for a voice (psid_lift_arm), it watches the voice's control register on
 * the chips the tune has; the first gate-on (the gate bit going 0 to 1 in a write) after the arm
 * starts it, on that chip. From then on, at the end of each play call (psid_lift_call), the
 * voice's seven registers and $15-$18 as last written are one frame, for at most
 * PSID_LIFT_FRAMES calls (a second at 50 Hz; a multispeed or CIA tune's calls are its frames).
 * It ends early at the voice's gate-off (the note's length is the song's, not the instrument's)
 * or its next gate-on (the call that has either is not kept). A register written more than
 * PSID_LIFT_DIGI times in one call of the note marks it a digi (samples played through the
 * registers), which no instrument can hold. The writes come from the main loop's sink
 * (psid_ctl.c ring_sink, psid_lift_write): the audio interrupt never sees any of this.
 *
 * The conversion (psid_lift_convert), frame by frame as GoatTracker's player runs an
 * instrument (core/player/gt2_play.c): the gate-on frame is the instrument's AD, SR and first
 * waveform; the tables run from the frame after it.
 * - Wave rows: the control register as the waveform (a change, or $00), the frequency as a
 *   note relative to the base note (the nearest note of the first frame that sets the pitch:
 *   frame 0 unless its frequency is the previous note's, as GoatTracker's own players leave it;
 *   NTSC frequencies scaled to PAL first); a run of unchanged frames is a delay row. An AD or
 *   SR change is a $F5 or $F6 row (as the drum kit's SR rows), which takes its frame: a waveform
 *   or note change in the same frame moves to the next.
 * - Vibrato: a frequency that swings around a note (three turns or more, off the note's
 *   frequency) is fitted with the instrument's vibrato in calculated mode (a speed table row:
 *   the speed and the shift of the note's step, the delay) when the fit at least halves the
 *   difference that holding the centre note leaves; else it is dropped
 *   (PSID_LIFT_F_NO_VIBRATO) and the centre held.
 * - Pulse rows (when a frame uses the pulse waveform): the width set at the first frame, then
 *   runs of equal steps (up to 127 a frame) as modulation rows, a set row at every jump.
 * - Filter rows only when the voice is routed to the filter ($17's voice bit) in a frame: the
 *   mode and resonance with the voice's own bit, the cutoff's high byte (GoatTracker's 8-bit
 *   cutoff), runs of steps as modulation rows. Approximate (PSID_LIFT_F_FILTER): the filter is
 *   the whole chip's.
 * - Not captured: what a player does before the gate (hard restart patterns), the other
 *   voices' sync and ring sources, the volume.
 *
 * Portable C11, no allocation; one caller, the main loop. The struct is big (the frames and
 * the file): on the device it overlays the song's work space in POOL while the tune has the
 * song's memory (platform/fm1/audio.c). */
#ifndef SIDOU_PSID_LIFT_H
#define SIDOU_PSID_LIFT_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PSID_LIFT_FRAMES 50u  /* a second of PAL frames */
#define PSID_LIFT_REGS 11u    /* a frame: the voice's 7 registers, then $15-$18 */
#define PSID_LIFT_DIGI 8u     /* more writes to one register in a call: a digi */
#define PSID_LIFT_FILE_MAX 256u /* the GTI5 file: one store staging buffer (store.h) */
#define PSID_LIFT_ROWS 112u   /* a table's rows at most: the file holds 111 */

/* A frame's bytes. */
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

/* psid_lift.state */
enum {
    PSID_LIFT_OFF = 0,
    PSID_LIFT_ARMED, /* waiting for the voice's gate-on */
    PSID_LIFT_TAKING, /* the note's frames */
    PSID_LIFT_TAKEN  /* done: psid_lift_convert */
};

/* psid_lift_convert's refusals (negative), its "again" and its flags. */
#define PSID_LIFT_AGAIN 0      /* too much for the file: fewer frames, at the next call */
#define PSID_LIFT_E_DIGI (-1)  /* DIGI: CAN'T LIFT */
#define PSID_LIFT_E_EMPTY (-2) /* no frame: the gate went off in the gate-on call */
#define PSID_LIFT_F_NO_VIBRATO 0x01u /* a vibrato no instrument vibrato follows: dropped */
#define PSID_LIFT_F_VIBRATO 0x02u    /* a vibrato fitted */
#define PSID_LIFT_F_FILTER 0x04u     /* filter rows (approximate) */
#define PSID_LIFT_F_SHORT 0x08u      /* frames dropped from the end to fit the file */

typedef struct {
    uint8_t state;  /* PSID_LIFT_* */
    uint8_t voice;  /* 0..2 */
    uint8_t chips;  /* the chips watched: 1, or 2 for a 2SID tune */
    uint8_t chip;   /* the chip whose gate-on started the capture */
    uint8_t frames; /* frames kept */
    uint8_t digi;   /* a register took more than PSID_LIFT_DIGI writes in a call of the note */
    uint8_t ended;  /* the gate went off, or on again, in this call: it is not kept */
    uint8_t base;   /* psid_lift_convert's base note (0 C-0): the note that plays it as lifted */
    uint8_t cut;    /* psid_lift_convert dropped frames from the end to fit the file */
    uint8_t count[PSID_LIFT_REGS]; /* this call's writes to each */
    uint8_t regs[2][25];           /* each chip's registers $00-$18 as last written */
    uint8_t before[2][2];          /* each chip's voice frequency as the last call left it */
    uint8_t frame[PSID_LIFT_FRAMES][PSID_LIFT_REGS];
    uint8_t file[PSID_LIFT_FILE_MAX]; /* psid_lift_convert's GTI5 */
    /* psid_lift_convert's scratch: a table's rows, each frame's note and kind */
    uint8_t row[2][PSID_LIFT_ROWS];
    uint8_t note[PSID_LIFT_FRAMES];
    uint8_t kind[PSID_LIFT_FRAMES];
} psid_lift;

/* Nothing captured, nothing watched. */
void psid_lift_init(psid_lift* l);
/* Arms the capture of voice `voice` (0..2) on the tune's `chips` (1, or 2: either chip's voice
 * starts it), or with voice above 2 disarms it; frames of an earlier capture are dropped. The
 * registers as last written are kept: the sink tracks them whatever the state. */
void psid_lift_arm(psid_lift* l, uint32_t voice, uint32_t chips);
/* A SID write, as the environment's sink sees it (chip 0 A, 1 B; reg 0-31). */
void psid_lift_write(psid_lift* l, uint32_t chip, uint32_t reg, uint32_t value);
/* A play call returned: a frame, the end of the capture. */
void psid_lift_call(psid_lift* l);
/* The tune stopped: a capture under way ends with the frames it has; an armed one is off. */
void psid_lift_stop(psid_lift* l);

/* The captured note (state PSID_LIFT_TAKEN) as a GTI5 file in l->file: its length (at most
 * PSID_LIFT_FILE_MAX), a PSID_LIFT_E_* refusal, or PSID_LIFT_AGAIN when the note does not fit
 * the file: an eighth of its frames are dropped from its end, and the next call tries again (a
 * call is one attempt, so a main-loop pass does one: 13,407 instructions on the device for The
 * Consultant's lead, emu-gate psid_views.rs). `ntsc`: the tune's clock is NTSC. `name`: the
 * instrument's name (16 characters at most kept). *flags gets PSID_LIFT_F_*. */
int psid_lift_convert(psid_lift* l, int ntsc, const char* name, uint32_t* flags);

#ifdef __cplusplus
}
#endif

#endif
