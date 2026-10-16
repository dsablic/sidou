// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* Where SIDOU's store lives in the FM-1's 1 MiB NOR flash, and the bytes of it never written
 * (docs/hardware.md, "Flash map").
 *
 * The store is [0x97000, 0xE0000): 292 KiB, 73 sectors of 4 KiB. It is the range the update
 * loader never writes or erases (platform/fm1/loader/ldr_core.c LDR_DATA_LO/HI: its record
 * scan skips it), between the app area (which ends at 0x93000 whatever the store's size) and
 * the update protocol's staging area (platform/fm1/src/ota.c OTA_AREA, 0xE0000), inside the
 * package's VM descriptor ([0x93000, 0xE9000), which nothing on SIDOU acts on). Felucca keeps
 * its main store in the same range.
 *
 * The guard band: bytes 0xF00..0xFFF of every store sector are never programmed. An update
 * record the SPL or the loader could take for one is 80 bytes at a sector's 0xF00 ("4K
 * boundary - 256"), with the magic 0x5441 at +6 and a matching CRC16 (ota.c). An erased band
 * reads 0xFFFF there, so nothing in the store can ever look like a record, wherever the SPL
 * scans (the loader skips this range already). 80 bytes (0xF00..0xF4F) would be enough; the
 * band keeps the whole 256-byte page, so a page program, which wraps inside its page, can
 * never reach it.
 *
 * Portable: the constants only, for the store (app/store.h), the device's flash range gate
 * (platform/fm1/midi.c FL_RANGE_OK) and the host tests (tests/app/store_layout.rs). */
#ifndef SIDOU_STORE_LAYOUT_H
#define SIDOU_STORE_LAYOUT_H

#define STORE_LO 0x97000u     /* the first byte of the store's first sector */
#define STORE_HI 0xE0000u     /* one past the last sector */
#define STORE_SECTOR 0x1000u  /* a 4 KiB erase sector */
#define STORE_SECTORS 73u     /* (STORE_HI - STORE_LO) / STORE_SECTOR */
#define STORE_PAGE 0x100u     /* a page program stays inside one 256-byte page */
/* The bytes of a sector the store may program: 0..0xEFF. 0xF00..0xFFF is the guard band. */
#define STORE_SECTOR_USE 0xF00u

_Static_assert(STORE_LO % STORE_SECTOR == 0 && STORE_HI % STORE_SECTOR == 0,
               "the store is whole sectors");
_Static_assert((STORE_HI - STORE_LO) / STORE_SECTOR == STORE_SECTORS, "73 sectors");
_Static_assert(STORE_SECTOR_USE % STORE_PAGE == 0, "the guard band is a whole page");
_Static_assert(STORE_SECTOR - STORE_SECTOR_USE >= 80u, "the band covers an update record");

#endif
