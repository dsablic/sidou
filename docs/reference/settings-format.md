<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Settings record

The settings are one value in the [store](store-format.md), key `0x100`, type `1`.
Schema 2 is 107 bytes; schema 1 is 106 bytes. Multibyte values are little-endian.
Schema 2 keeps schema 1's field offsets and appends TRACKER ROWS.

| Offset | Bytes | Value |
|---|---|---|
| 0 | 1 | Schema version, `2` |
| 1 | 1 | CARDS: `0` off, `1` on |
| 2 | 1 | SOUNDS: `0` off, `1` on |
| 3 | 1 | KEYS: `0` AUTO, `1` A |
| 4 | 1 | Preset index |
| 5 | 1 | Parameter count, `42` |
| 6 | 44 | Parameters in LIVE_PARAM order (app/live.h) |
| 50 | 42 | Learned CC for each parameter; `255` means no binding |
| 92 | 14 | Last-booted firmware version, NUL-padded |
| 106 | 1 | TRACKER ROWS: `0` BAR (default), `1` SCROLL |

CUTOFF and PULSE WIDTH are two bytes each; every other parameter is one byte.
The VOLUME parameter's byte is zero and is ignored when loading: MASTER belongs to
the physical pot. Octave, held notes, controllers and the displayed view are not
part of the record. The remaining parameters include the requested chip model,
profile and character, envelope mode, preset edits and effect-slot settings.

A later schema appends fields. A newer record's schema-1 prefix remains readable;
future readers fill fields absent from older schemas with their defaults. A short
record (less than 106 bytes for schema 1, 107 for schema 2 or later), schema zero or
fewer than 42 parameters is rejected. Schema 1 loads TRACKER ROWS as BAR;
an out-of-range TRACKER ROWS byte also falls back to BAR. Out-of-range
parameter values keep the stored preset's values: `live_set_preset` is applied first.
The store supplies the record's CRC and
atomic commit; there is no second CRC inside this payload.

Settings load before the first instrument tick. A missing record or a different
stored firmware version selects the full boot; the current version is then saved.
An unchanged version selects the short boot, unless HOME is held at power-on.

Persisted setters mark changes for the background save. Five seconds after the
last changed value, one whole record is submitted to the store. Unchanged frames
do not gather or CRC the record. Returning to the already-saved values requires no
background write. An explicit SAVE skips the debounce; a busy store retries, and
after two seconds an explicit save may force held operations and report
`SAVED (GAPS)`. Background saves never force them. Foreign data is never overwritten
without the menu's explicit format confirmation.
