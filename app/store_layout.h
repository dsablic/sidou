// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_STORE_LAYOUT_H
#define SIDOU_STORE_LAYOUT_H

#define STORE_LO 0x97000u
#define STORE_HI 0xE0000u
#define STORE_SECTOR 0x1000u
#define STORE_SECTORS 73u
#define STORE_PAGE 0x100u
#define STORE_SECTOR_USE 0xF00u

_Static_assert(STORE_LO % STORE_SECTOR == 0 && STORE_HI % STORE_SECTOR == 0,
               "the store is whole sectors");
_Static_assert((STORE_HI - STORE_LO) / STORE_SECTOR == STORE_SECTORS, "73 sectors");
_Static_assert(STORE_SECTOR_USE % STORE_PAGE == 0, "the guard band is a whole page");
_Static_assert(STORE_SECTOR - STORE_SECTOR_USE >= 80u, "the band covers an update record");

#endif
