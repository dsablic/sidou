// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "bootguard.h"

int bootguard_boot(bootguard* g) {
    if (g->magic != BOOTGUARD_MAGIC) { /* power-on: RAM noise */
        g->magic = BOOTGUARD_MAGIC;
        g->pending = 0;
        g->failed = 0;
    }
    if (g->pending) {
        g->failed++;
    }
    g->pending = 1;
    if (g->failed >= BOOTGUARD_FAILED_MAX) {
        g->failed = 0;
        g->pending = 0;
        return 1;
    }
    return 0;
}

void bootguard_poll(bootguard* g, uint32_t uptime_ms) {
    if (g->pending && uptime_ms >= BOOTGUARD_STABLE_MS) {
        g->pending = 0;
        g->failed = 0;
    }
}

void bootguard_leave(bootguard* g) {
    g->pending = 0;
}
