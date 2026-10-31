<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# FM-1 Live Link, protocol version 1

Live Link lets a computer show an FM-1's screen and press its buttons over the USB cable,
through the USB-MIDI port the FM-1 already has: SysEx frames both ways, no special USB mode,
nothing installed (Web MIDI in Chrome or Edge is enough). It is written so that any FM-1
firmware can implement it; what is SIDOU's own is marked as such.

The viewer is a browser page, the site's [Live Link](https://dsablic.github.io/sidou/link.html)
(`web/link.html`, `web/link.js`; [manual](../manual/live-link.md)): it draws the screen with
SIDOU's own `app/grid.c` and `app/font.c` compiled to wasm (`tools/wasm-link`). The reference decoder is the `fm1-live-link` crate
(`host/crates/live-link`): no dependencies, small enough to port or compile to wasm.

## Frames

Every frame, both ways:

```
F0 7D 46 4D 31 4C cmd packed... checksum F7
```

- `7D` is the non-commercial manufacturer id, `46 4D 31 4C` is "FM1L". A firmware's other
  `7D` frames (SIDOU's own are `7D 53 44`, [sysex.md](sysex.md)) are told apart by these
  bytes.
- `cmd` is a 7-bit command (below).
- The payload is packed seven bytes at a time: a mask byte holding their high bits (bit k for
  byte k of the group), then their low seven bits. An empty payload has no mask. A decoded
  payload is at most **48 bytes**, so a whole frame is at most 64 bytes (22 USB-MIDI event
  packets).
- `checksum` makes `cmd`, every packed byte and itself sum to 0 modulo 128. The `F0` and the
  magic are not summed.
- Multi-byte numbers are little-endian. Real-time bytes may occur anywhere; any other status
  byte ends a frame unread. A frame that fails its checksum, or a command the device does not
  know, is ignored.

## Session

The host starts it; the device never does.

| Byte | Host to device | Payload |
|---|---|---|
| `01` | START | The protocol version the host speaks (1) |
| `02` | HEARTBEAT | Empty; at least every 2 s |
| `03` | STOP | Empty |
| `04` | KEYFRAME | Empty: every cell is sent again |
| `05` | FONT | Empty: the font, as GLYPHS frames |
| `06` | PALETTE | Empty: the palette, as a COLOURS frame |
| `07` | PANEL | Buttons and encoders ([Remote control](#remote-control)) |

| Byte | Device to host | Payload |
|---|---|---|
| `41` | HELLO | The capabilities ([below](#hello)) |
| `42` | BEAT | Empty: the answer to a HEARTBEAT |
| `43` | STOPPED | Empty: no session (after STOP, the timeout, or any command but START without one) |
| `44` | COLOURS | 16 colours, RGB565 each (red in the top 5 bits) |
| `45` | GLYPHS | First code, then 8 rows a glyph (bit 7 the leftmost pixel), up to 5 glyphs |
| `46` | CELLS | Changed cells of one row ([Cells mode](#cells-mode)) |
| `47` | PIXELS | One changed pixel cell |

1. **START.** The device answers HELLO and marks every cell to be sent (a keyframe). A START
   during a session starts it again.
2. **HEARTBEAT** at least every 2 s; the device answers BEAT. **3 s** after the last START or
   HEARTBEAT the session ends by itself and the device sends STOPPED, so a page that was
   closed leaves nothing running.
3. **STOP** ends it at once (answered STOPPED). A session also ends, silently, when USB goes
   away (unplugged, the host asleep).
4. Fetch FONT and PALETTE once (HELLO's CRCs tell whether a cached copy is still right); ask
   for a KEYFRAME whenever the picture may be wrong (frames lost on the host's side).

While a session runs, the device shows `LINK` on its own screen (SIDOU: small, in the
bottom-left corner, over whatever is there; the mirror shows the screen without it).

### HELLO

| Offset | Size | Meaning (SIDOU's value) |
|---|---|---|
| 0 | 1 | Protocol version (1) |
| 1 | 1 | Features: bit 0 cells mode, bit 1 pixel cells, bit 2 remote control (7) |
| 2 | 1 | Columns (30) |
| 3 | 1 | Rows (30) |
| 4 | 1 | Cell width in pixels (8) |
| 5 | 1 | Cell height in pixels (8) |
| 6 | 1 | Colours (16) |
| 7 | 1 | First font code (0) |
| 8 | 1 | Last font code (`DC`); codes past it draw nothing |
| 9 | 1 | First large-text piece code (1) |
| 10 | 1 | Piece codes (14; 0: none) |
| 11 | 1 | Timeout in tenths of a second (30) |
| 12 | 1 | Buttons (14) |
| 13 | 1 | Encoders (7) |
| 14 | 4 | CRC-32 of the font's glyph rows, first code to last (IEEE, as zlib) |
| 18 | 4 | CRC-32 of the palette as COLOURS carries it |
| 22 | 0-16 | The firmware's name and version, ASCII ("SIDOU 1.1.0") |

A host reads the fields it knows and ignores any that a later version adds after them.

## Cells mode

The screen is a grid of cells (SIDOU: 30 x 30 cells of 8 x 8 pixels on the 240 x 240 LCD),
each a font code and a colour byte: foreground in the low nibble, background in the high one.
The device sends a cell whenever its LCD was sent that cell, as the cell is when the frame
goes out: a cell that changed again in between goes once, as it is then. A host that applies
every frame in order has the device's screen within a moment.

**CELLS:** `row`, `col`, then a record for each cell from `col` on along the row, as many as
the payload holds. A record:

| Bytes | Meaning |
|---|---|
| `ch` | The font code |
| `attr` | The colour byte |
| `x` | For a piece code: the character it is a piece of. Otherwise `00`, or `10` + a colour: an overlay, whose 8 mask rows follow |

**PIXELS:** `row`, `col`, one record as above, then the cell's 64 background pixels as colours,
two to a byte (the left one in the high nibble), row by row. A cell sent as PIXELS stays a
pixel cell until it comes as CELLS.

**Drawing a cell** (what the device's LCD shows, pixel for pixel):

1. A **pixel cell:** its 64 pixels; over them the glyph's set pixels in the foreground (a
   piece code draws as an ordinary glyph here: blank in SIDOU's font); over those the
   overlay's set pixels in its colour.
2. A **piece** (`ch` from the first piece code, `p = ch - first`): a part of the character
   `x` drawn larger. Pieces 0-3 are the 2 x 2 cells of a character at twice the size, pieces
   4-12 the 3 x 3 cells of one at three times, piece 13 the character at its own size (on
   top: no overlay shows on it). With `scale` 2, 3 or 1 and `q` = `p`, `p - 4` or 0, the cell's
   pixel (px, py) is the glyph's pixel ((`q mod scale` x 8 + px) / `scale`, (`q / scale` x 8 +
   py) / `scale`), whole-number division, in the foreground if set, else the background. A
   piece has no overlay.
3. Any **other cell:** the glyph `ch`, foreground on background, and the overlay's set pixels
   (if `x` has bit 4) in its colour over both.

A firmware that draws no large text says 0 pieces; one with no pixel cells clears feature
bit 1 and never sends PIXELS.

**Pixels mode** (changed rectangles of RGB565 for firmwares that draw pixels rather than
cells) is reserved for a later version: feature bit 3, commands `48` and up.

## Remote control

**PANEL:** `held` (2 bytes, bit i: button i held now), `pressed` (2 bytes, buttons pressed
since the last PANEL, even if let go again), then one signed byte per encoder from encoder 0
(detents since the last PANEL, + clockwise; fewer bytes: the rest 0). The ids are the
device's (SIDOU uses Felucca's: buttons 0-13, encoders 0-6; [controls](controls.md)).

- A button held in one PANEL and not in the last counts as pressed too.
- The host's buttons are held until the next PANEL, or **500 ms** after the last one: send a
  PANEL at least every 250 ms while anything is held, and one with nothing held to let go.
- They act as the panel's own: merged with the keys the device reads, after its power-on
  combinations (a session starts only after the boot).
- The note keys are not part of PANEL: send them as MIDI notes.

## Pacing

On the device's side a frame:

- goes whole into the USB-MIDI output, so nothing comes between its packets;
- waits while the device's other SysEx is going out or coming in (a file transfer, a reply,
  an update), and while the output has room for fewer than the frame and 32 more event
  packets, which keeps room for MIDI out's CCs and clock out's messages;
- comes out of a budget of 22 decoded bytes a millisecond (22 KB/s, a quiet spell saving at
  most 96).

So a still screen sends nothing, a keyframe of plain cells takes about 130 ms, and a view that
redraws much of itself every frame (a scope's overlays, a pixel scene) shows fewer frames a
second than the LCD: the newest state of each cell, never a backlog. SIDOU's rates, measured
in the emulator: [Measured](#measured).

## Test vectors

Host to device (the session's, and a PANEL: HOME, button 8, held and pressed; K1, encoder 2, a
detent clockwise):

```
START      F0 7D 46 4D 31 4C 01 00 01 7E F7
HEARTBEAT  F0 7D 46 4D 31 4C 02 7E F7
STOP       F0 7D 46 4D 31 4C 03 7D F7
KEYFRAME   F0 7D 46 4D 31 4C 04 7C F7
FONT       F0 7D 46 4D 31 4C 05 7B F7
PALETTE    F0 7D 46 4D 31 4C 06 7A F7
PANEL      F0 7D 46 4D 31 4C 07 00 00 01 00 01 00 00 01 76 F7
```

Device to host:

```
CELLS: row 0, col 0: 'A' in light blue (14) on black
           F0 7D 46 4D 31 4C 46 00 00 00 41 0E 00 6B F7
CELLS: row 5, col 3: a space with an overlay in colour 3 (rows 80 40 00 00 00 00 00 01),
       then piece 0 (the top left of an 'A' at twice the size) in 7E
           F0 7D 46 4D 31 4C 46 20 05 03 20 0E 13 00 40 00 00 00 00 00 00 01 01 00 7E 41 50 F7
```

`host/crates/live-link/tests/vectors.rs` checks these byte for byte;
`host/crates/sidou-tests/tests/app/link.rs` runs SIDOU's device side against the decoder.

## Measured

SIDOU in fm1-emu with its USB host, which takes one 64-byte IN packet a millisecond
(`host/crates/emu-gate/tests/link.rs`, which writes `link_mirror.json`):

| View | Payload bytes a second | Frames a second |
|---|---|---|
| LIVE, a new note every quarter second (the scopes redrawing) | 8,180 | 353 |
| TRACKER playing (the rows scrolling) | 2,528 | 130 |
| SCENE (FIRE, PETSCII cells) | 4,560 | 565 |
| A view at rest | 0 | 0 |

A main-loop pass stayed within 71,800 instructions (the allowance is 120,000), a session's
work at most 7,900 of them; clock out's start, song position and continue reached the host as
without a session, its clocks no less even, and MIDI out's CCs all went.
