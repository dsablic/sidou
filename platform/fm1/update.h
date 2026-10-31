// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_FM1_UPDATE_H
#define SIDOU_FM1_UPDATE_H

#include <stdint.h>

void update_boot(void);
void flash_plain_window(void);
int update_poll(void);
int update_session(void (*before_reset)(void));

#endif
