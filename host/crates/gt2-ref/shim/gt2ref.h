/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
/* The gt2-ref harness's C entry points (gt2ref.c), over GoatTracker 2.77's own song and
 * relocation code (third_party/goattracker2): what gt2reloc does from its command line,
 * without the command line, and loadsong then savesong. Not thread-safe: GoatTracker's
 * code keeps the song in globals (the Rust side holds a lock). */
#ifndef GT2REF_H
#define GT2REF_H

#include <stddef.h>

/* gt2reloc's options (readme 4.4), set as its main sets them. */
typedef struct {
    unsigned hr_adsr;           /* -A, 0x0F00 */
    unsigned multiplier;        /* -S, 1 (0: 25 Hz) */
    unsigned ntsc;              /* -N: 1 (the .sid header and speed code only) */
    unsigned custom_clock;      /* -F, 0 */
    float a4_hz;                /* -G, 0: the built-in frequency table */
    unsigned finevibrato;       /* -V, 1 */
    unsigned optimize_pulse;    /* -O, 1 */
    unsigned optimize_realtime; /* -R, 1 */
    unsigned player_version;    /* GoatTracker's PLAYER_* bits: B 8, D 16, E 32, H 64, C 128,
                                   no optimisation 256, J 512 */
    unsigned sid_address;       /* -L, 0xD400 */
    unsigned player_address;    /* -W << 8, 0x1000 */
    unsigned zeropage;          /* -Z, 0xFC */
    unsigned sid_model;         /* 0 6581, 1 8580 */
} gt2ref_options;

void gt2ref_default_options(gt2ref_options* o);

/* The song packed as a .prg (load address first), as `gt2reloc song.sng out.prg` with the
 * options writes it. 0 and *prg (free with gt2ref_free); -2 if loadsong could not load the
 * song (an unknown ident, or a file shorter than its contents say); -1 if the packer
 * refused it. */
int gt2ref_pack(const unsigned char* sng, size_t len, const gt2ref_options* o,
                unsigned char** prg, size_t* prg_len);

/* GoatTracker's loadsong, then savesong: the song as GoatTracker writes it. -2 if the song
 * did not load (as gt2ref_pack), -1 if savesong failed. */
int gt2ref_canonical(const unsigned char* sng, size_t len, unsigned char** out,
                     size_t* out_len);

void gt2ref_free(void* p);

#endif
