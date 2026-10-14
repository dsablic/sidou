// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The boot-loop guard: a firmware that crashes or hangs early in every boot ends in UBOOT,
 * which still accepts updates, instead of looping for ever.
 *
 * Portable C with no hardware access. On the device the state lives in .noinit RAM, which a
 * reset keeps and power-on fills with noise (hence the magic). As Felucca's (src/main.c,
 * fm1_cstart):
 * - each boot that finds the previous one still pending counts it as failed: a crash, a
 *   watchdog reset or a power cut before it was stable;
 * - the boot after BOOTGUARD_FAILED_MAX failed ones enters UBOOT instead of starting;
 * - a boot is stable once the main loop has run BOOTGUARD_STABLE_MS: the count is cleared;
 * - an intentional reset (an update's commit) is not a failure. */
#ifndef SIDOU_BOOTGUARD_H
#define SIDOU_BOOTGUARD_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BOOTGUARD_MAGIC 0x53424754u /* "TGBS" */
#define BOOTGUARD_FAILED_MAX 2u
#define BOOTGUARD_STABLE_MS 10000u

typedef struct {
    uint32_t magic;
    uint32_t pending; /* this boot has not been stable yet */
    uint32_t failed;  /* boots in a row that died before they were stable */
} bootguard;

/* First thing at boot. Returns 1 if the firmware must enter UBOOT now (the guard is cleared
 * first, so the boot after UBOOT starts afresh), else 0. */
int bootguard_boot(bootguard* g);
/* Main loop, with the time since boot. */
void bootguard_poll(bootguard* g, uint32_t uptime_ms);
/* Before an intentional reset: it does not count as a failed boot. */
void bootguard_leave(bootguard* g);

#ifdef __cplusplus
}
#endif

#endif
