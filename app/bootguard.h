// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_BOOTGUARD_H
#define SIDOU_BOOTGUARD_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BOOTGUARD_MAGIC 0x53424754u
#define BOOTGUARD_FAILED_MAX 2u
#define BOOTGUARD_STABLE_MS 10000u
#define BOOTGUARD_CHECK 0x9E3779B9u

typedef struct {
    uint32_t magic;
    uint32_t pending;
    uint32_t failed;
    uint32_t check;
} bootguard;

int bootguard_boot(bootguard* g);
void bootguard_poll(bootguard* g, uint32_t uptime_ms);
void bootguard_leave(bootguard* g);

#ifdef __cplusplus
}
#endif

#endif
