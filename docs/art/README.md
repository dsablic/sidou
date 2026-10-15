<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU art direction

This page fixes the palette, font, layout grid, motion rules and logo before
screens multiply (design document, section 7 "Look and feel"). Every screen
and piece follows it. The visual reference is the browser prototype
(<https://claude.ai/artifact/Ghyhf6bUkXUFrawXbsc3X5>), which runs the same
grid, palette and font.

The look: an 80s home computer, C64 culture in spirit. Art is original only:
no Commodore marks, logos or trade dress, and no characters, sprites or names
from existing games.

## Palette

Sixteen colours, the C64's, after Philip "Pepto" Timmermann's measurements of
the VIC-II. The firmware's table is `grid_palette` in `app/grid.c` (plain
RGB565); the LCD driver sends it high byte first.

| Index | Name (`GRID_…`) | sRGB | RGB565 |
|---|---|---|---|
| 0 | `BLACK` | `#000000` | `0x0000` |
| 1 | `WHITE` | `#FFFFFF` | `0xFFFF` |
| 2 | `RED` | `#68372B` | `0x69A5` |
| 3 | `CYAN` | `#70A4B2` | `0x7536` |
| 4 | `PURPLE` | `#6F3D86` | `0x69F0` |
| 5 | `GREEN` | `#588D43` | `0x5C68` |
| 6 | `BLUE` | `#352879` | `0x314F` |
| 7 | `YELLOW` | `#B8C76F` | `0xBE2D` |
| 8 | `ORANGE` | `#6F4F25` | `0x6A64` |
| 9 | `BROWN` | `#433900` | `0x41C0` |
| 10 | `LIGHT_RED` | `#9A6759` | `0x9B2B` |
| 11 | `DARK_GREY` | `#444444` | `0x4228` |
| 12 | `GREY` | `#6C6C6C` | `0x6B6D` |
| 13 | `LIGHT_GREEN` | `#9AD284` | `0x9E90` |
| 14 | `LIGHT_BLUE` | `#6C5EB5` | `0x6AF6` |
| 15 | `LIGHT_GREY` | `#959595` | `0x94B2` |

RGB565 keeps the top 5, 6 and 5 bits of red, green and blue.

Default use:
- **Screen:** light blue text on blue, inside a light blue border (the
  READY. screen).
- **Voices, by waveform:** pulse cyan, saw yellow, triangle light green,
  noise light grey.
- **Raster ramp** (logo stripes, raster bars): white, yellow, light green,
  cyan, light blue, purple, light red.
- **Warnings and errors:** light red; **OK:** light green.

## Layout grid

The 240×240 LCD is a grid of **30×30 cells of 8×8 pixels**, the same medium
as the C64's 40×25 PETSCII screen. Column 0 is the left edge and row 0 the
top; a cell is at pixel (8 × column, 8 × row).

- Each cell holds a font code and a colour byte: foreground in the low
  nibble, background in the high nibble (`GRID_ATTR(fg, bg)`).
- **Frames** take the outer ring (rows 0 and 29, columns 0 and 29). Content
  starts at column 2, leaving a one-cell margin inside the frame.
- **Title** on row 2; a status or toast line on row 26 or 27.
- Text is upper case. Lower case codes show as upper case.
- Pixel art beyond the font (oscilloscopes, the logo) is drawn as **pixel
  overlays** on cells: an 8×8 one-colour mask over the cell's glyph
  (`grid_overlay`, at most `GRID_OVERLAYS` cells at once).

## Font

`assets/font/sidou-8x8.txt` is the source: one glyph per entry, 8 rows of
`#` and `.`, in a text format described at the top of the file.
`host/crates/sidou-tests` generates `app/font.c` (a `const` table of 256
glyphs, 2 KiB in flash) and `app/font.h` (`FONT_<NAME>` codes) from it; a test
fails when they fall behind the sheet. To change a glyph, edit the sheet and
run, from `host/`:

```sh
SIDOU_FONT_UPDATE=1 cargo test --release -p sidou-tests --test app font
```

The sheet draws 159 glyphs (lower case reuses upper case). The face is
chamfered capitals with 2-pixel stems, 7×7 pixels in the top-left of the
cell, so text keeps a one-pixel gap to the right and below.
Graphics glyphs fill the whole cell, so they join up. The firmware's glyph
sheet screen (`UI_CHARSET` in `platform/fm1/ui.h`) shows them all, with the
palette; its fm1-emu snapshot is `tests/golden/lcd/charset.png`.

| Codes | Glyphs |
|---|---|
| `0x20`–`0x7E` | ASCII; `a`–`z` reuse `A`–`Z` |
| `0x7F` | `FULL` block (also the cursor) |
| `0x80`–`0x8F` | quadrants `Q0`–`Q15`; bit 0 top left, 1 top right, 2 bottom left, 3 bottom right |
| `0x90`–`0x98` | lower eighths `V0`–`V8`, filled from the bottom |
| `0x99`–`0xA1` | left eighths `H0`–`H8`, filled from the left |
| `0xA2`–`0xAA` | bar eighths `BAR0`–`BAR8`: left eighths on rows 1–6, for meters |
| `0xAB`–`0xBB` | ordered-dither levels `D0`–`D16` (4×4 Bayer, 4 × n pixels set) |
| `0xBC`–`0xCA` | frames: `BOX_H`, `BOX_V`, square and `ROUND_` corners, tees, `CROSS` |
| `0xCB`–`0xD8` | icons: dots, star, ball, pip, arrows, play, note, square |
| `0x00`–`0x1F`, `0xD9`–`0xFF` | blank, free for later glyphs |

**Provenance and licence.** The font is original, drawn for SIDOU in the
browser prototype and ported from it into the sheet; the missing ASCII
glyphs (`` & @ \ ^ ` { | } ~ ``) and the bar eighths, tees and side arrows were
drawn for the sheet in the same style. It is not derived from the Commodore
character ROM or any other font. Licence: **GPL-3.0-or-later**, like the
rest of SIDOU's own files, so it ships with the firmware under the same terms.
It replaces the public-domain `font8x8_basic` placeholder of M1a.

## Motion rules

- **25 fps:** the UI composes a frame every 40 ms (`UI_PERIOD_MS`).
- **Only changed cells go out.** The renderer (`app/grid.h`) keeps the frame
  the panel shows next to the one being composed. Changed cells are sent in
  runs: horizontally adjacent changed cells on a row form one LCD window.
- **SPI budget: at most about 20 ms of SPI per 40 ms frame at steady state.**
  At the LCD's 12 MHz SPI a cell is 128 bytes, 85 µs; a window adds 11
  command bytes (7 µs) and the CPU's command turnaround. 20 ms is about
  230 cells per frame. A full screen is 115,200 bytes, 77 ms.
- **CPU:** composing a screen costs about 20k instructions a frame, and
  rendering about 580 instructions a cell, so a full screen is 2.2 ms of CPU
  at 240 MHz (fm1-emu, `host/crates/emu-gate/tests/screen.rs`).
- **Pieces over budget** drop their frame rate, shrink their area or use a
  coarser palette. A transition may spend a full redraw once, not every
  frame.
- **The main loop never waits on the LCD.** One run goes out per pass while
  the panel is free, rendered while the last one transfers. Nothing in the
  UI touches the audio interrupt or its budget.
- Each screen gets fm1-emu LCD snapshot tests; each animated piece gets a
  busiest-frame cost test (main-loop instructions and SPI bytes).

## Logo

Four concepts from the prototype, all drawn in code on the cell grid:

| Concept | Idea |
|---|---|
| **Pulse Train** (default) | Slotted raster letters on a pulse wave: the SID's signature waveform is the underline. In motion the stripes roll through the raster ramp and the duty cycle sweeps; the boot screen uses the moving version. |
| Three Voices | Triangle, saw and pulse traces weave over and under the letters, one per voice. |
| READY. | The name typed at the prompt in quadrant blocks, with a blinking cursor; it lives on the cell grid, so animating it costs one cell. |
| 28 Pin | SIDOU where the part number would be on a 28-pin chip; the pins light with the voice gates. |

**Decision to confirm with the user:** Pulse Train is the default logo
until the user picks one of the four.

**Naming:** the instrument is **SIDOU**, always in capitals, in the font or
the logo's own letters. "SID" refers to the chip only when describing what
SIDOU emulates.
