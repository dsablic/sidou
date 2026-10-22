<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Bitmap row format

The logos, title cards and RED MOON RITE use this format. The asset tools' `bitmap_codec`
model and the compositor's `bitmap_row` and `bitmap_run` readers implement the same specification.
Pixels, transparency, cropping, patches, mirroring and colour remapping are unchanged by the encoding.

## Pools and row addresses

An image's `data` points to a pool in flash. A pool starts with sixteen bytes:

| Bytes | Meaning |
|---|---|
| 0–14 | Fifteen original run bytes in dictionary mode; zero in raw-run mode |
| 15 | Low four bits: checkpoint interval's log2; bit 7: raw-run mode; other bits zero |
| 16 onward | Row packets, with common byte substrings shared |

`rows[y]` is a 16-bit **byte offset** from that image's pool pointer to its row packet.
Several pools may be concatenated in the generated `RUNS` array; each image's existing
`data` pointer selects its pool. Offsets are local to that pool, not to the whole array.
The logos' file instead gives each pool its own array (`METAL_CHROME_RUNS`,
`CRACKTRO_CHROME_RUNS`), so the firmware's `--gc-sections` drops a pool no shown logo reads:
Cracktro Chrome, kept for the host tests, costs the image nothing.

## Row packets and checkpoints

For row width `w` and checkpoint interval `S`, a packet starts with `k = (w - 1) / S`
checkpoint bytes, followed by the coded runs. The intervals are:

| Reader | `S`, in stored pixels |
|---|---:|
| Title cards | 64 |
| Metal Chrome | 256 (raw runs, preserving the boot decoder's cost) |
| Cracktro Chrome | 256 (its 240-pixel rows need no checkpoints) |
| RED MOON RITE | 32 (raw runs) |

A dictionary-mode checkpoint byte is the number of **coded nibbles** between the preceding
checkpoint and this one; the first counts from the payload's start. A raw-run checkpoint
is an **absolute byte offset** from the payload's start. Runs are split at every checkpoint,
so a checkpoint always enters a run at its first pixel. An interval holds at most `3 * S`
nibbles: at most one three-nibble escaped run per pixel. Intervals with checkpoints are at
most 64 pixels, hence at most 192 nibbles, within a byte. The 256-pixel profile is allowed
only when the whole row fits that interval and needs no checkpoint.

To enter at pixel `x`, let `j = x / S`. The cursor starts at nibble offset
`2 * (rows[y] + k)`, plus the first `j` checkpoint bytes summed in dictionary mode, or
twice checkpoint `j - 1` in raw-run mode (`j = 0` adds nothing). It starts at pixel `j * S`.
Only the remaining prefix
within that interval is walked. No earlier pixels or rows need decoding or buffering.

## Run payload

Raw-run mode stores the original run bytes directly. The target's hot row loops keep their
byte-pointer readers, without a per-run mode test. It is used where dictionary lookup's
instruction cost does not fit the existing gate; packet substring sharing still reduces size.
SCENE's reader is specialized to the fixed 32-pixel raw-run profile; its converter and tests
enforce that profile. The grid reader supports both modes and reads the interval from the pool.

Dictionary mode uses the following representation:

Nibbles are read **low half of the byte first**. A nibble cursor counts from the pool's
first byte, including its codebook and row headers.

| First nibble | Decoded run byte | Nibbles consumed |
|---|---|---:|
| 0–14 | The corresponding codebook byte | 1 |
| 15 | The next two nibbles, low then high | 3 |

An original run byte is `((length - 1) << 4) | colour`: 1–16 pixels of palette index 0–15.
For example, escaped nibbles `F, 4, A` give byte `A4`: eleven pixels of colour 4.
SCENE's image-specific `clear` colour continues to mean transparency.

The decoded lengths add up to the row width exactly. There is no terminator; an unused
high nibble at the packet's end is zero padding. Row packets begin on byte boundaries,
but runs and checkpoints may enter either nibble. The readers trust the generated assets
for these invariants; pixel-for-pixel round-trip tests cover every bitmap and partial span.

## Encoder policy

The tools split runs at checkpoints, count run bytes across unique `(interval, row)` pairs,
and choose the fifteen most frequent bytes. Ties use ascending run-byte value. This makes
generation deterministic even when hash-table iteration order changes.

Within a pool, identical packets share their address. A packet may also reuse any matching
byte substring, or overlap the pool's suffix before appending its remainder. These are
absolute row addresses, not runtime back-references: decoding has no history or recursion.
The packer measures source order, longest packets first (lexical ties), lexical order, and
reverse source order, keeping the smallest result and the first on a tie.

When interval and mode match, the encoder measures one shared pool against one independent pool per image and chooses
the smaller byte count, preferring the shared pool on a tie. Thus sharing cannot cost more
than separate images, even when they favour different codebooks. Different intervals use
independent pools. Raw-run absolute checkpoint offsets must fit a byte. An offset past 65,535 bytes
is rejected rather than truncated.

The crop/patch selection model still uses original run-byte costs; generated totals count
the actual pools, row tables and image metadata. The title-card comparison retains its
original whole-PNG byte-run control (57,190 bytes); its numerical ceilings are unchanged.

## RAM and regeneration

The codebook and checkpoints are read directly from flash. No dictionary, history buffer,
row buffer or frame buffer is allocated. A 32-bit nibble cursor replaces the target's
32-bit stream pointer; output uses the existing tile and LCD buffers. Bitmap palette pairs
and painter pixels share one existing-size 64-byte scratch union, never live together.
Read-only image/palette inputs and writable output buffers do not overlap.

From `host/`, regenerate the three C asset files, not the LCD goldens:

```sh
SIDOU_LOGO_UPDATE=1 SIDOU_SCENE_UPDATE=1 \
  cargo test --locked --release -p sidou-tests --test app the_generated_file_is_up_to_date
```

The app tests compare decoded pixels against every source PNG, including clipped spans,
mirroring and remapping. Emulator checks compare the existing LCD goldens and enforce the
unchanged boot, title-card and SCENE instruction/SPI bounds.
