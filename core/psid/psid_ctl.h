// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The .sid player (M3 Task 8; docs/plans/2026-10-19-m3-sid-files.md, "Look-ahead and the
 * stamped writes"): the main loop runs a tune's 6502 (psid_env.h) ahead of the audio, and the
 * audio interrupt places each of its SID writes at the cycle the 6502 made it.
 *
 * The main half (psid_ctl.c, one caller: the main loop) loads a tune into the environment,
 * starts, pauses, resumes, stops and changes its song, and once a pass (psid_ctl_poll) runs one
 * slice of the 6502 while it is less than PSID_AHEAD_CYCLES ahead of chip A's clock and the
 * write ring has room for a slice's writes (PSID_SLICE_ROOM). Ahead enough, it idles, and the
 * pass goes to everything else. A slice is at most PSID_SLICE_CYCLES of the 6502's time, and
 * less when its estimated work (the SID accesses, the shadows' dearest voices) reaches
 * PSID_SLICE_WORK: the slice bounds the pass's work (PSID_SLICE_CYCLES, below, says how). Each
 * slice ends with the voice shadows clocked to the 6502's time (sid_shadow_clock), so no access
 * in the next has more than a slice to catch up.
 *
 * The write ring (one producer, the main loop, one consumer, the audio interrupt, one core, as
 * reg_ring's): each SID write's cycle on the tune's running count and the chip, register and
 * value in 16 bits (PSID_RING_ENTRY). A slice's writes become visible together at its end.
 *
 * The interrupt half (psid_ctl_isr.c: psid_ctl_block) renders a block of each chip in use and
 * places each write from the ring at its cycle. Each chip keeps the cycles it has clocked since
 * the tune's cycle 0 (psid_ctl.clock): to place a write at cycle X it renders the whole samples
 * that end at or before X (stepping the chip's Q16 accumulator, no division), then sid_advance
 * clocks the voices the rest of the way into the next sample and the write goes in with
 * sid_write (sid.h; decision record 0005's split render, inside a sample). A chip's block is at
 * most PSID_SPLITS renders; the writes past that are applied in order at the block's end, and
 * counted late, as is a write whose cycle the chip has passed already (the 6502 fell behind):
 * the audio never waits for the 6502. The render loop is the chip's own, so the samples are
 * the ones a render clocked write by write at the stamps gives, whenever no write was late.
 *
 * The audio starts at the first block after the 6502 has run one period of init (or init has
 * returned): a slow init plays from silence in real time, as on a C64, and the main loop stays
 * live through it. Paused or stopped, the chips are not rendered (silence) and their clocks
 * stand still; a pause fades its first block out and a resume its first block in (audio_ramp's
 * ramp), and a stop fades its block out and then clears the chips' registers.
 *
 * The voice mutes act on the chips, not on the tune's writes (sid_set_mute through reg_ring's
 * REG_RING_MUTE: the main loop's hook, psid_ctl_hooks.mute); a stop clears them on the chips
 * once the interrupt has played it.
 *
 * Portable C11, no allocation. The interrupt half calls nothing in flash but through its
 * render pointer and sid.c's RAMTEXT functions (sid_advance, sid_write): on the device it runs
 * from SRAM (platform/fm1/app.ld, Task 9), built -O2 without jump tables (core/CMakeLists.txt).
 * The main half runs from flash at -Os. */
#ifndef SIDOU_PSID_CTL_H
#define SIDOU_PSID_CTL_H

#include <stdint.h>

#include "psid/psid_env.h"
#include "psid/psid_file.h"
#include "sid/sid.h"
#include "sid/sid_shadow.h"

#ifdef __cplusplus
extern "C" {
#endif

#define PSID_BLOCK 128u /* samples a block (platform/fm1/audio.h AUDIO_HALF_FRAMES) */
#define PSID_RING 1024u /* write ring entries, a power of two: 6,144 bytes */

/* The look-ahead's slice: what one main-loop pass runs of the 6502, within PSID_SLICE_WORST
 * (40,000 instructions on the device, the plan's budget, emu-gate's common/mod.rs). Cycles
 * alone cannot bound it: on fm1-emu (emu-gate cpu6502_budget.rs) the 6502 costs about 9.3
 * instructions a cycle, but each SID access about 480 more (the shadow's step to its cycle, the
 * I/O page's path, the ring: a write every 4 cycles is 130 a cycle), and a voice combining noise
 * with another waveform costs its shadow up to about 80 a cycle until its noise register locks
 * up (sid_osc.h: 31,000 for the first 130 cycles of three such voices, the setup's writes
 * among them). So a slice is runs of PSID_SUB_CYCLES, and after each the pass adds the run's
 * estimated work (PSID_WORK_CYCLE a cycle, PSID_WORK_ACCESS an access, PSID_WORK_NOISY a cycle
 * of each such voice), and stops once it reaches PSID_SLICE_WORK or the slice has run
 * PSID_SLICE_CYCLES; a run can add one run's worst on top. While such a voice plays, the
 * shadows are clocked after every run, so none has more than a run of it to catch up; between
 * calls (nothing runs, no access can come) one run reaches the next call. Measured, every pass
 * of the worst cases (one and two chips: those voices from power-on, an access every 4 cycles,
 * writes, OSC3 reads) is at most 34,048; a GoatTracker tune's at most 7,249 (2.1 a 6502 cycle on
 * average). A tune that accesses the SID every few cycles for long does not keep up (its writes
 * come late); the audio never waits. Task 9 checks the firmware's pass. */
#define PSID_SLICE_CYCLES 1000u
#define PSID_SUB_CYCLES 32u
#define PSID_WORK_CYCLE 12u
#define PSID_WORK_ACCESS 480u
#define PSID_WORK_NOISY 80u
#define PSID_SLICE_WORK 20000u
/* How far ahead of chip A's clock the 6502 may run: two PAL frames. */
#define PSID_AHEAD_CYCLES 39312u
/* The ring room a slice needs: the writes its work estimate allows before it stops, and the
 * last run's (a SID write takes at least 3 cycles: a read-modify-write's two writes come 1
 * cycle apart, but the instruction takes 6), with one instruction's overshoot (2 more). */
#define PSID_SLICE_ROOM (PSID_SLICE_WORK / PSID_WORK_ACCESS + 1u + PSID_SUB_CYCLES / 3u + 2u)
/* A chip's renders a block at most (writes that would split it further go at the block's end,
 * late). Set from the device's split cost by Task 9 (psid_budget.rs) so that the worst block
 * holds PSID_ISR_WORST. */
#define PSID_SPLITS 32u

/* A ring entry's 16 bits: chip << 13 | register << 8 | value (register 0-31). */
#define PSID_RING_ENTRY(chip, reg, value) \
    ((uint16_t)((uint32_t)(chip) << 13 | ((uint32_t)(reg) & 0x1fu) << 8 | ((uint32_t)(value) & 0xffu)))
#define PSID_RING_CHIP(e) ((uint32_t)(e) >> 13)
#define PSID_RING_REG(e) (((uint32_t)(e) >> 8) & 0x1fu)
#define PSID_RING_VALUE(e) ((uint32_t)(e) & 0xffu)

/* psid_ctl_state. */
enum {
    PSID_EMPTY = 0, /* nothing loaded */
    PSID_LOADED,    /* a tune loaded, not started */
    PSID_INIT,      /* its init runs (the audio starts a period into it) */
    PSID_PLAYING,   /* init returned: the play calls run */
    PSID_PAUSED,    /* paused, from INIT or PLAYING */
    PSID_STOPPED,   /* stopped (or a song change waits for the stop to play out) */
    PSID_FAILED     /* the tune stopped itself: psid_ctl.error is the PSID_RUN_* code */
};

/* What the main loop asks of the interrupt (psid_ctl.want), and its mode (psid_ctl.mode). */
enum { PSID_ISR_OFF = 0, PSID_ISR_PLAY, PSID_ISR_PAUSE, PSID_ISR_STOP };

/* What psid_ctl needs from the chips; every member may be NULL. */
typedef struct {
    /* The sid that plays chip `chip` (0 A, 1 B) now (on the device, reg_ring_chip_of): the
     * shadows start from its voices when a song starts, while the interrupt renders none of
     * the tune's chips. */
    sid* (*chip)(void* ctx, uint32_t chip);
    /* Chip `chip`'s voice mute (bit v mutes voice v): on the device REG_RING_MUTE through the
     * chip's reg_ring_out, so it reaches the chip at a block's start. */
    void (*mute)(void* ctx, uint32_t chip, uint32_t mask);
    void* ctx;
} psid_ctl_hooks;

/* The main loop's work space (POOL on the device, overlaying the song's: Task 9). */
typedef struct {
    psid_env env;
    sid_shadow shadow[2]; /* chip A's and B's, which answer the tune's SID reads */
} psid_work;

/* A part of a chip's block (platform/fm1/audio.c render_part: sid_render, or the LIVE view's
 * tapped render), from the interrupt. */
typedef void (*psid_render_fn)(sid* s, int16_t* out, uint32_t n, int tapped);

typedef struct psid_ctl {
    /* --- the write ring: written by the main loop, read by the interrupt --- */
    uint32_t ring_cycle[PSID_RING]; /* each write's cycle on the tune's running count */
    uint16_t ring_entry[PSID_RING]; /* PSID_RING_ENTRY */
    volatile uint32_t head;         /* entries before it are visible */
    /* --- written by the main loop, read by the interrupt --- */
    volatile uint32_t want_seq; /* a request (want) for each new value */
    volatile uint8_t want;      /* PSID_ISR_PLAY, _PAUSE or _STOP */
    volatile uint8_t chips;     /* the tune's chips: 1, or 2 for a 2SID tune */
    /* --- written by the interrupt, read by the main loop --- */
    volatile uint32_t tail;      /* entries before it are applied */
    volatile uint32_t done_seq;  /* the last request it has carried out (a fade played) */
    volatile uint32_t clock[2];  /* each chip's cycles clocked since the tune's cycle 0 */
    volatile uint32_t late;      /* writes applied after their cycle */
    volatile uint32_t blocks;    /* blocks rendered since the audio started */
    /* --- the interrupt's --- */
    uint32_t cur[2];    /* each chip's next entry (a chip skips the other's) */
    uint8_t mode;       /* PSID_ISR_OFF, _PLAY or _PAUSE (stopped is off) */
    uint8_t fade_in;    /* the next rendered block ramps in (after a pause) */
    uint32_t splits_max; /* the most renders a chip's block took (tests read it) */
    /* --- the main loop's --- */
    psid_work* w;
    uint8_t* mem;                /* the tune's 64 KiB */
    const psid_header* h;        /* the caller's, as long as the tune is loaded */
    const uint8_t* file;         /* the whole file, or NULL when it came in parts */
    psid_ctl_hooks hooks;
    uint32_t stage;   /* entries before it are pushed */
    uint32_t slices;  /* slices run */
    uint32_t writes;  /* SID writes pushed since the song started */
    uint32_t dropped; /* writes the ring had no room for (never: PSID_SLICE_ROOM) */
    int16_t error;    /* PSID_FAILED: the PSID_RUN_* code */
    uint8_t state;    /* PSID_* */
    uint8_t paused_from; /* PSID_INIT or PSID_PLAYING */
    uint8_t started;  /* the audio has been asked to start */
    uint8_t stop_wait; /* a stop went: the interrupt has not played it yet */
    uint16_t next_song; /* a song change waiting for its stop to play out, 0 none */
    uint8_t mute;     /* bits 0-2 chip A's voices, 3-5 chip B's (psid_ctl_mute) */
} psid_ctl;

/* --- The main loop ------------------------------------------------------------------------
 * Commands return 1, or 0 if they cannot go now (nothing changed then; ask again at the next
 * pass). */

/* The player over its work space and the tune's 64 KiB (on the device the song's region):
 * nothing loaded. hooks may be NULL. */
void psid_ctl_init(psid_ctl* c, psid_work* w, uint8_t* mem, const psid_ctl_hooks* hooks);
/* Loads the tune h (psid_parse's, kept by the caller as long as the tune is loaded) with its
 * data from `file`, the whole file (kept as long as the tune is loaded: a song change loads it
 * again), or NULL: then the caller brings the data in with psid_ctl_load_part, and a song
 * change is the caller's (psid_ctl_load again, then psid_ctl_play). Stops nothing: 0 while a
 * tune plays or a stop has not played out yet. */
int psid_ctl_load(psid_ctl* c, const psid_header* h, const uint8_t* file);
void psid_ctl_load_part(psid_ctl* c, uint32_t offset, const uint8_t* bytes, uint32_t n);
/* Starts song (1..songs, 0 the header's start song): init at cycle 0, the shadows from the
 * chips (psid_ctl_hooks.chip). From LOADED, or from STOPPED or FAILED when the file was kept
 * (loaded again first). 0 for a song out of range too. */
int psid_ctl_play(psid_ctl* c, uint32_t song);
/* Stops the tune: the interrupt fades its block out and clears the chips' registers, then the
 * chips' mutes are cleared (at a later poll, once it has played). */
int psid_ctl_stop(psid_ctl* c);
int psid_ctl_pause(psid_ctl* c);
int psid_ctl_resume(psid_ctl* c);
/* A song change: the tune stops as psid_ctl_stop, and once that has played (a poll) it is
 * loaded again and song starts from init. 0 without a kept file (psid_ctl_load's). */
int psid_ctl_subtune(psid_ctl* c, uint32_t song);
/* The voice mutes, bits 0-2 for chip A's voices and 3-5 for chip B's (a 2SID tune's; GLO's
 * mutes set both): to the chips through psid_ctl_hooks.mute now, and at each song's start. */
int psid_ctl_mute(psid_ctl* c, uint32_t mask);
/* Once a pass: what the interrupt has carried out (a stop's end, a song change waiting for
 * it), then at most one slice of the 6502 while it is less than PSID_AHEAD_CYCLES ahead of chip
 * A's clock and the ring has PSID_SLICE_ROOM free. 1 when a slice ran (the pass's slice flag,
 * audio_work_slice on the device). */
int psid_ctl_poll(psid_ctl* c);
/* PSID_* */
uint32_t psid_ctl_state(const psid_ctl* c);
/* The 6502's lead on chip A's clock, in cycles (0 when the chip is ahead). */
uint32_t psid_ctl_ahead(const psid_ctl* c);

/* --- The audio interrupt --------------------------------------------------------------- */

/* A block: the main loop's request first (a start, a pause, a resume, a stop), then each chip
 * of the tune (a; b for a 2SID tune, else unused and may be NULL) rendered into sa and sb,
 * PSID_BLOCK samples, with the ring's writes placed at their cycles; bit c of `tapped` renders
 * chip c tapped (the LIVE view's scope). Not playing: sa (and sb) silence, the chips not
 * rendered. Returns the chips rendered (bit 0 A, bit 1 B). */
uint32_t psid_ctl_block(psid_ctl* c, sid* a, sid* b, int16_t* sa, int16_t* sb, uint32_t tapped,
                        psid_render_fn render);

#ifdef __cplusplus
}
#endif

#endif
