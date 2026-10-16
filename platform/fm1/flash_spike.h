// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The flash timing spike (M2a Task 3b): a test firmware, built only with CMake
 * -DSIDOU_FLASH_SPIKE=ON and never released, that measures the flash part's program and
 * erase times on the store's last two sectors and shows them on the screen
 * (docs/hardware-test.md, section 12; the bounds are flash_spike_guard.h's). */
#ifndef SIDOU_FM1_FLASH_SPIKE_H
#define SIDOU_FM1_FLASH_SPIKE_H

/* From main.c's boot, the audio never started, with TIMER5 and interrupts on (the key scan,
 * USB and the TRS UART run). Waits for SAVE, measures, shows the results, and never returns:
 * while it waits and once it is done it calls service (the watchdog, USB's retry, the boot
 * guard, and an install through the update protocol, which ends in a reset). */
_Noreturn void flash_spike_run(void (*service)(void));

#endif
