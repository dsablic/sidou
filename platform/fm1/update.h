// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The community SysEx update protocol, step 1 (Felucca's ota.c, vendored in usb/): answer
 * the installer's identity request, and on its upgrade request stage the package's update
 * loader in flash and restart into it. Step 2 is the loader's (platform/fm1/loader).
 *
 * Over USB only, as Felucca: the protocol needs replies, and the TRS jack is MIDI IN only.
 * The stock UBOOT soft key (F0 22 24 35 7D F7) is not part of it and is never handled.
 * Implemented in midi.c, which holds the USB stack the protocol talks through. */
#ifndef SIDOU_FM1_UPDATE_H
#define SIDOU_FM1_UPDATE_H

#include <stdint.h>

/* Boot, interrupts off, before the audio starts: checks the flash chip and erases a staging
 * area left from an update that did not finish (the SPL would otherwise run its loader). */
void update_boot(void);
/* Main loop: answers identity requests. Returns 1 when the host asked to upgrade. */
int update_poll(void);
/* After update_poll returned 1 and the caller stopped the audio for good (audio_halted):
 * runs step 1 (seconds; the progress on the screen). On success it calls before_reset and
 * restarts into the loader, never returning. Otherwise nothing was committed and it returns
 * Felucca's failure code (< 0). */
int update_session(void (*before_reset)(void));

#endif
