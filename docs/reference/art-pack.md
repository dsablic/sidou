<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Art packs (`.sidart`)

An art pack is the pictures one of SCENE's illustrated pieces draws, kept as a file in SIDOU's
store instead of in the firmware. The first, and for now the only, piece is RED MOON RITE; its
art ships as [`assets/art/red-moon-rite.sidart`](../../assets/art/red-moon-rite.sidart). This page
is the format byte for byte, for anyone building a pack: the code is `app/art_pack.c` (the
firmware's checks and loader) and `host/crates/sidou-tests/src/scene_art.rs` (the builder that
makes the shipped pack from `assets/scene/`'s PNGs); the host tools check a pack with the
firmware's own C (`sidou-sysex art`, [below](#sending-one)).

All numbers are little-endian. CRC-32 is IEEE 802.3's, reflected, as zlib's `crc32` and the
store's.

## On the FM-1

- **One pack a piece.** A pack is the store file named after the piece it skins (`RED MOON RITE`),
  of file type 5. A PUT of a new pack replaces the old one, as any PUT of a name does. No other
  type may take that name, and type 5 takes no other name: SysEx refuses both as malformed (1),
  SAVE AS refuses the name.
- **Checked twice.** The SysEx PUT checks the header and the slot table in its first DATA
  (refused with NAK 17, `NOT RED MOON RITE ART`, before anything is claimed or programmed;
  [sysex.md](sysex.md)), and the store checks the bytes against the PUT's CRC as for any file.
  RED MOON RITE checks the whole pack again each time it opens it: the header, every slot, every
  frame inside its slot, every row table and row inside the pack and its page, and the CRC
  (below). A pack that fails is not drawn.
- **Drawn in place.** The firmware reads the pack straight from flash (XIP): it copies nothing but
  a 24-byte description of each frame and a pointer a page into RAM (1,196 bytes, inside SCENE's
  own state).
- **Without a valid pack** RED MOON RITE plays SIDOU's logo intro instead (the SIDOU logo between
  drifting starfields, its glow in the colours of what is played), and its toast says
  `RED MOON RITE ART: LIBRARY`. A pack sent while the intro plays is picked up within 16 frames;
  one deleted or replaced while it is drawn is let go at the next frame.
- **FORMAT** erases the store, the pack with it: RED MOON RITE plays the intro until the pack is
  sent again. A firmware update keeps the store, and the pack.

## Pages

The store keeps a file in chunks of 1,248 bytes that are not next to each other in flash
([store-format.md](store-format.md#files)). A pack is laid out in **pages** of the same 1,248
bytes, page `p` being the file's bytes `[1248 p, 1248 p + 1248)`, and nothing that the firmware
reads in one piece may cross a page: the header and the tables (all in page 0), each frame's row
table, and each row's packet. An offset in the pack is a file offset; the firmware turns it into
an address as `page[offset / 1248] + offset % 1248`. A pack is at most 65,536 bytes (53 pages).

## Layout

| Offset | Size | What |
|---|---|---|
| 0 | 32 | the header |
| 32 | 4 × slots | the slot table |
| 32 + 4 × slots | 12 × frames | the frame table |
| ... | | the row tables, then the row packets, each within one page; bytes between them are free (zero in the shipped pack) |
| length - 4 | 4 | CRC-32 of every byte before it |

Since the CRC covers everything before it, the CRC-32 of the whole pack is always the residue
`0x2144DF1C`. The firmware checks that residue against the CRC the store keeps for the file, so it
never reads the pack through to check it.

### The header

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | magic `SIDOUART` |
| 8 | 1 | format version, 1 |
| 9 | 1 | the piece it skins: 1, RED MOON RITE |
| 10 | 1 | slot count (RED MOON RITE: 12) |
| 11 | 1 | frame count (RED MOON RITE: 41) |
| 12 | 4 | the pack's length in bytes, the CRC included |
| 16 | 16 | the pack's name: 1 to 16 characters `20`-`5F`, not starting with a space, NUL-padded |

### The slot table

A slot is one picture the piece draws, in one or more frames of one size. Each entry:

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | the slot's id |
| 1 | 1 | the frame's width, in stored pixels |
| 2 | 1 | the frame's height |
| 3 | 1 | frame count |

A pack must supply exactly the piece's slots, in this order, each at its size and frame count, or
it is refused. RED MOON RITE's (`art_pack_rmr_slots` in `app/art_pack.c`):

| Id | Slot | Width × height | Scale | Frames |
|---|---|---|---|---|
| 1 | the left fighter, armed: guard, guard 2, raise, chop, block, spin A, spin B, hit | 64 × 124 | 2 | 8 |
| 2 | the right fighter, armed (drawn mirrored), the same poses | 64 × 124 | 2 | 8 |
| 3 | the left fighter, unarmed: guard, guard 2, hit, reach | 64 × 124 | 2 | 4 |
| 4 | the right fighter, unarmed | 64 × 124 | 2 | 4 |
| 5 | the flying sword, eight turns | 42 × 84 | 2 | 8 |
| 6 | the sword stuck in the ground | 16 × 44 | 2 | 1 |
| 7 | the sky over its gradient, for a closed, a half-open and an open filter | 240 × 124 | 1 | 3 |
| 8 | the arena: columns, arch, torches' stands, flagstones | 240 × 240 | 1 | 1 |
| 9 | the status bar | 240 × 16 | 1 | 1 |
| 10 | a stamina blade, empty | 240 × 16 | 1 | 1 |
| 11 | a stamina blade, full | 240 × 16 | 1 | 1 |
| 12 | a blade's tip | 6 × 5 | 1 | 1 |

Scale is how many screen pixels wide a stored pixel is (`gx`); it is the piece's, and a frame
must give it. The fighters' shadows, the torches' flames, the sparks, the sword's arc and the
skies' gradients are drawn by code and are not in the pack.

### The frame table

One entry a frame, the slots' frames in slot order (frame 0 is slot 1's first):

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | x of the rectangle the frame's rows cover, inside its slot's frame |
| 1 | 1 | y |
| 2 | 1 | width of that rectangle, at least 1, `x + width` at most the slot's width |
| 3 | 1 | height, at least 1, `y + height` at most the slot's height |
| 4 | 1 | scale (`gx`): the slot's |
| 5 | 1 | the colour code (0-15) that means transparent in this frame's runs |
| 6 | 1 | the anchor's x: where the piece places the frame (a fighter's feet, a sword's grip) |
| 7 | 1 | the anchor's y |
| 8 | 1 | base: `FF`, or an earlier frame of the same size this frame is drawn over (a patch) |
| 9 | 1 | zero |
| 10 | 2 | the file offset of the frame's row table: even, the table within one page |

Outside its rectangle a frame is its base's pixels, or transparent without one: an unarmed pose
can be just the arm and sword that differ from its armed pose.

### Row tables and row packets

A frame's row table holds one 2-byte file offset per row of its rectangle, top to bottom: the
row's packet, which other rows and frames may share. A packet is today's run-length row
([../art/bitmap-codec.md](../art/bitmap-codec.md), raw runs):

- first `(width - 1) / 32` checkpoint bytes: checkpoint `j` is the index, among the runs, of the
  run that starts at pixel `32 j`;
- then one byte a run: the run's length less one in the high nibble (1-16 pixels), its colour
  code in the low nibble (the frame's transparent code drawing nothing). Runs never cross a
  multiple of 32 pixels, and add up to the rectangle's width.

The firmware does not decode every row when it opens a pack; a row whose runs are wrong draws
wrong pixels and nothing else (every write stays inside the span asked for).

## Making one

The shipped pack is built from `assets/scene/`'s PNGs (made by `assets/scene/red-moon-rite.js`,
[../art/README.md](../art/README.md)) by `host/crates/sidou-tests/src/scene_art.rs`:
`SIDOU_SCENE_UPDATE=1 cargo test -p sidou-tests --test app scene_art::the_generated_file_is_up_to_date`
writes it, and that test fails while it is stale. Another pack needs the same slots: the builder
crops each frame, codes its rows, shares identical packets and lays them out by pages.

**The site's [art pack builder](https://dsablic.github.io/sidou/art.html)** ([the manual](../manual/art-packs.md))
makes one in the browser with the same choices (`web/sidart.js`; built from `assets/scene/`'s
PNGs it gives the shipped pack byte for byte) and checks it with the firmware's own loader. It maps
PNGs to slots as follows:

- **One PNG a slot**, its frames side by side in one row in the slot's frame order, each at the
  slot's width × height in stored pixels: a `w × frames` by `h` image (the left fighter armed:
  512 × 124). Any other size is refused.
- **Colours**: each pixel becomes the nearest of the 16 (squared RGB distance); alpha under 128 is
  transparent. The pack's transparent code is the first colour no frame uses, or, when the frames
  use all 16 between them, each frame's first unused colour; a frame of all 16 is refused.
- **Frames**: each is cropped to its pixels' box. An unarmed pose (slots 3 and 4: guard, guard 2,
  hit) is coded as a patch over the same fighter's armed pose when that is smaller.
- **The skies** (slot 7) are painted whole: a pixel equal to the gradient the FM-1 draws for that
  sky, or transparent, is left out of the overlay.

## Sending one

- **The art pack builder** ([../../web/art.html](../../web/art.html)) sends the pack it built.
- **The Library page** ([../../web/library.html](../../web/library.html)): **RED MOON RITE art**
  sends the shipped pack; **Send files…** takes a `.sidart` and sends it under its piece's name.
  The page checks the header, the slots and the CRC first.
- **`sidou-sysex art FILE.sidart [--out DIR]`** checks a pack with the firmware's loader and
  writes its upload (PUT, DATA, END) as a `.syx`; **`sidou-sysex send FILE.sidart`** sends it to
  the FM-1's MIDI port (macOS). `--seed FILE` writes a store region holding the pack alone, as
  the browser demo boots with.
