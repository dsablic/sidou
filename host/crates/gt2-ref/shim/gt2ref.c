/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
/* The harness's entry points (gt2ref.h): GoatTracker 2.77's gt2reloc main, its option
 * handling and checks, called with the options as values instead of a command line. */
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "goattrk2.h"
#include "gt2ref.h"
#include "membuf.h"
#include "gt2ref_shim.h"

/* gt2reloc.c's globals (its main is compiled as gt2reloc_main, never called). */
extern char songfilename[];
extern char packedsongname[];
extern unsigned char freqtbllo[], freqtblhi[];
extern unsigned customclockrate;
/* greloc.c's source and output buffers, left full when its exit returns early. */
extern struct membuf src, dest;

#define PACKED_NAME "gt2ref.prg"
#define SAVED_NAME "gt2ref-out.sng"
#define FREQ_NOTES 96

static unsigned char default_lo[FREQ_NOTES], default_hi[FREQ_NOTES];
static int saved_defaults;

void gt2ref_default_options(gt2ref_options* o) {
    memset(o, 0, sizeof *o);
    o->hr_adsr = 0x0f00;
    o->multiplier = 1;
    o->finevibrato = 1;
    o->optimize_pulse = 1;
    o->optimize_realtime = 1;
    o->sid_address = 0xd400;
    o->player_address = 0x1000;
    o->zeropage = 0xfc;
}

/* What gt2reloc's main does before it reads its options: the channels, a cleared song,
 * then the song loaded. The defaults of every option, as gt2reloc.c initialises them. */
static void gt2ref_load(const unsigned char* sng, size_t len) {
    if (!saved_defaults) {
        memcpy(default_lo, freqtbllo, FREQ_NOTES);
        memcpy(default_hi, freqtblhi, FREQ_NOTES);
        saved_defaults = 1;
    }
    memcpy(freqtbllo, default_lo, FREQ_NOTES);
    memcpy(freqtblhi, default_hi, FREQ_NOTES);
    playerversion = 0;
    fileformat = FORMAT_PRG;
    zeropageadr = 0xfc;
    playeradr = 0x1000;
    sidmodel = 0;
    multiplier = 1;
    adparam = 0x0f00;
    ntsc = 0;
    sidaddress = 0xd400;
    finevibrato = 1;
    optimizepulse = 1;
    optimizerealtime = 1;
    customclockrate = 0;
    usefinevib = 0;
    basepitch = 0.0f;
    gt2ref_reset_files();
    gt2ref_set_input(sng, len);
    initchannels();
    clearsong(1, 1, 1, 1, 1);
    strcpy(songfilename, GT2REF_INPUT);
    loadsong();
}

int gt2ref_pack(const unsigned char* sng, size_t len, const gt2ref_options* o,
                unsigned char** prg, size_t* prg_len) {
    gt2ref_load(sng, len);
    strcpy(packedsongname, PACKED_NAME);
    fileformat = FORMAT_PRG;
    adparam = o->hr_adsr;
    multiplier = o->multiplier;
    ntsc = o->ntsc;
    customclockrate = o->custom_clock;
    basepitch = o->a4_hz;
    finevibrato = o->finevibrato;
    optimizepulse = o->optimize_pulse;
    optimizerealtime = o->optimize_realtime;
    playerversion = o->player_version;
    sidaddress = o->sid_address;
    playeradr = (int)o->player_address;
    zeropageadr = (int)o->zeropage;
    sidmodel = o->sid_model;
    /* gt2reloc's "Validate parameters". */
    sidmodel &= 1;
    adparam &= 0xffff;
    zeropageadr &= 0xff;
    playeradr &= 0xff00;
    sidaddress &= 0xffff;
    if (multiplier > 16)
        multiplier = 16;
    if ((finevibrato == 1) && (multiplier < 2))
        usefinevib = 1;
    if (finevibrato > 1)
        usefinevib = 1;
    if (optimizepulse > 1)
        optimizepulse = 1;
    if (optimizerealtime > 1)
        optimizerealtime = 1;
    if (customclockrate < 100)
        customclockrate = 0;
    if (basepitch < 0.0f)
        basepitch = 0.0f;
    if (basepitch > 0.0f)
        calculatefreqtable();
    /* gt2reloc reports its sizes on stdout: not into the caller's output. */
    fflush(stdout);
    int saved = dup(1), null = open("/dev/null", O_WRONLY);
    if (saved >= 0 && null >= 0)
        dup2(null, 1);
    int ok = gt2ref_guard(relocator) == 0;
    fflush(stdout);
    if (saved >= 0 && null >= 0)
        dup2(saved, 1);
    if (saved >= 0)
        close(saved);
    if (null >= 0)
        close(null);
    gt2ref_take_output(PACKED_NAME, prg, prg_len);
    gt2ref_set_input(NULL, 0);
    if (!ok) {
        membuf_free(&src);
        membuf_free(&dest);
        free(*prg);
        *prg = NULL;
    }
    return ok && *prg ? 0 : -1;
}

int gt2ref_canonical(const unsigned char* sng, size_t len, unsigned char** out,
                     size_t* out_len) {
    gt2ref_load(sng, len);
    strcpy(songfilename, SAVED_NAME);
    int ok = savesong();
    gt2ref_take_output(SAVED_NAME, out, out_len);
    gt2ref_set_input(NULL, 0);
    return ok && *out ? 0 : -1;
}

void gt2ref_free(void* p) {
    free(p);
}
