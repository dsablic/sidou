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
RGB565). `ui_init` in `platform/fm1/ui.c` byte-swaps it once (`LCD_SWAP`), so
pixels the SPI sends from memory reach the panel high byte first.

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
  (`grid_overlay`, at most `GRID_OVERLAYS` = 255 cells at once, enough for
  LIVE's three scopes of about 80 cells each; the pool costs 2.5 KB of RAM).
  A view switch clears them all.

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

The sheet draws 162 glyphs (lower case reuses upper case). The face is
chamfered capitals with 2-pixel stems, 7×7 pixels in the top-left of the
cell, so text keeps a one-pixel gap to the right and below.
Graphics glyphs fill the whole cell, so they join up. The firmware's glyph
sheet (the CHARSET view in `app/views.c`, opened from the menu) shows them
all, with the palette; its fm1-emu snapshot is
`tests/golden/lcd/charset.png`.

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
| `0xD9`–`0xDB` | `SLOT_L`, `SLOT_R`, `SLOT_LR`: the logo's slotted raster bands, a top half's quadrants 3 rows deep |
| `0x00`–`0x1F`, `0xDC`–`0xFF` | blank, free for later glyphs |

**Provenance and licence.** The font is drawn for SIDOU. Its letters and
digits came from the browser prototype, and the rest of ASCII, the bar
eighths, tees and side arrows were drawn for the sheet in the same style.
The first version of the sheet was too close to `font8x8_basic`, the
public-domain placeholder of M1a: 14 printable glyphs were bit-identical to it
(`# $ & ( ) + . 1 > N W [ ] ^`) and 36 within 4 pixels by the measure below.
Every one of them that could differ was redrawn:
`! " # $ & ( ) + - / 0 1 4 < > @ K M N W [ \ ] ^ { }`.
The Commodore character ROM was not copied either: the face only takes the
C64's spirit (chunky capitals on an 8×8 cell).

`glyphs_are_not_copied_from_font8x8` in
`host/crates/sidou-tests/tests/app/font.rs` keeps it that way. It compares
each printable glyph with `font8x8_basic` (kept as test data in
`host/crates/sidou-tests/tests/data/`, never built into the firmware). The
distance is the number of differing pixels, the smallest over shifts of up
to one pixel in each direction. Every glyph must be at least 5 pixels away;
the median is 7. The exceptions are glyphs whose shape is forced at 8×8 with
2-pixel strokes. Any 8×8 font draws them nearly alike:

| Glyph | Distance | Why it stays close |
|---|---|---|
| `.` | 0 | A 2×2 dot on the baseline. |
| `,` | 0 | That dot with a one-step tail down to the left. |
| `'` | 0 | The comma's shape, raised to the cap line. |
| `_` | 0 | One full-width line on the bottom text row. |
| `` ` `` | 1 | A three-row tick down to the right. |
| `\|` | 2 | A 2-pixel vertical line; font8x8's is broken in the middle. |
| `:` | 4 | Two 2×2 dots. |
| `;` | 4 | The colon with the comma's tail. |
| `I` | 4 | A 2-pixel stem between full-width bars; the 4 pixels are font8x8's narrower bars. |
| `T` | 4 | A 2-pixel stem under a bar; the 4 pixels are font8x8's serifs. |

Licence: **GPL-3.0-or-later**, like the rest of SIDOU's own files, so the
font ships with the firmware under the same terms.

## Motion rules

- **25 fps:** the UI composes a frame every 40 ms (`UI_PERIOD_MS`).
- **Only changed cells go out.** The renderer (`app/grid.h`) keeps the frame
  the panel shows next to the one being composed. Changed cells are sent in
  runs: horizontally adjacent changed cells on a row form one LCD window.
- **SPI budget: at most about 20 ms of SPI per 40 ms frame at steady state.**
  At the LCD's 12 MHz SPI a cell is 128 bytes, 85 µs; a window adds 11
  command bytes (7 µs) and the CPU's command turnaround. 20 ms is about
  230 cells per frame. A full screen is 115,200 bytes, 77 ms.
- **CPU:** composing the status screen and the empty main-loop passes between
  frames cost about 37k instructions a frame, and rendering about 580
  instructions a cell, so a full screen is 2.2 ms of CPU at 240 MHz (fm1-emu,
  `host/crates/emu-gate/tests/screen.rs`).
- **Pieces over budget** drop their frame rate, shrink their area or use a
  coarser palette. A transition may spend a full redraw once, not every
  frame.
- **The main loop never waits on the LCD.** One run goes out per pass while
  the panel is free, rendered while the last one transfers. Nothing in the
  UI touches the audio interrupt or its budget.
- Each screen gets fm1-emu LCD snapshot tests; each animated piece gets a
  busiest-frame cost test (main-loop instructions and SPI bytes).

## Views, menus and transitions

The screen is a stack of **views** (`app/screen.h`): each is a table of
enter, exit, input and compose functions, and the one on top has the focus.
It composes every cell each frame and gets the panel's events first; what it
does not take plays the instrument (`app/views.h`,
[controls](../reference/controls.md)).

- **Frames:** PETSCII box glyphs with the title set into the top edge,
  between spaces (`widget_frame`). A screen keeps the full-block border on
  the outer ring.
- **Menus:** a list of items, each a label and a value right-aligned against
  the frame (an arrow for an item that opens a view). The cursor is a
  **blinking block** in front of the selected item, 8 frames on and 8 off
  (320 ms each). SELECT moves it, ALGORITHM changes the value.
- **Toast:** a line of reverse-video text (blue on light blue) on row 27 for
  1.5 s, saying what changed.
- **Wipe:** a view opens top first and closes bottom first, 4 rows a frame
  behind a white edge row, so the full redraw a view switch needs spreads
  over 8 frames (about 320 ms). The rows not reached yet keep the old view
  on the panel.
- **Raster:** colour bands in the raster ramp's order run down the border,
  one row every other frame, then the border gets its own colour back. A chip
  change runs it once round the ramp (7 steps, 14 frames). It waits while a
  wipe runs.

Measured in fm1-emu (`screen_changes_and_transitions_stay_within_the_budgets`):

| Change | Frames | Windows | SPI bytes | Worst frame |
|---|---|---|---|---|
| Wipe down to the menu | 8 | 51 | 78,513 | 11.2 ms |
| Wipe up to the status screen | 8 | 54 | 77,906 | 10.9 ms |
| Raster and toast (chip change) | 15, every other one | 451 | 123,105 | 10.5 ms |
| Menu idle (the cursor) | 1 in 8 | 1 | 139 | 0.1 ms |

The animated screens compose every frame (`the_boot_and_about_stay_within_their_budgets`,
260 frames of the full boot and 1800 of About, a full round of its 15 pages):

| Screen | Cells a frame (average, most) | Windows a frame | SPI a frame (average, worst) | Main loop a frame (average, busiest `ui_poll`) |
|---|---|---|---|---|
| Full boot | 29, 150 | 9.6 | 2.5 ms, 12.8 ms | 50k, 62k instructions |
| About | 94, 148 | 24.5 | 8.2 ms, 12.8 ms | 161k, 106k instructions |

About costs 4.0M instructions a second (1.7 % of the core at 240 MHz).

A wipe sends only the rows that differ, once each, and its edge row once
more; a raster step recolours the 116 border cells in 58 windows. Every frame
stays inside the 20 ms budget.

A wipe is sized for a view that holds still: the rows it has revealed must not change
under it. The animated views (About, the boot's title card) hold their animation while a
wipe or a raster runs (`screen_animating`), so a wipe into them sends its 150 cells a frame
at most, 12.8 ms. Before, About's bars, logo and scroller moving in the revealed rows made
a frame of the wipe into it 243 cells, 21.0 ms, and the title card's stripes 192 cells,
16.6 ms (`every_wipe_into_an_animated_view_sends_only_its_rows` in fm1-emu, and
`the_wipe_into_about_sends_only_its_rows` and `the_wipe_into_the_title_card_sends_only_its_rows`
on the host, at every phase).

## Logo

The logo is a **pluggable asset** (`app/logo.h`): the boot's title card and About draw
whichever logo `logo_default` names, through a table in flash:

| Field | Pulse Train |
|---|---|
| `cols`, `rows`: the footprint in cells | 15 x 5 (120 x 40 pixels) |
| `overlays`: overlay cells at most | 55 |
| `cells_per_frame`: the most cells one frame changes | 42 |
| `draw(grid, col, row, frame, bg)` | frame `frame` of the animation (25 fps, 0 the still) |

`draw` paints every cell of the footprint, glyph, colours and overlay (it clears an overlay
it does not need), so a view composes it like the rest of its screen. The declared cost and
overlays are measured (`host/crates/sidou-tests/tests/app/logo.rs`), so a test fails when a
logo's drawing and its table disagree. `overlays` and `cells_per_frame` are 16-bit, room for
a logo up to the whole screen (900 cells).

Each place that shows it gives it a slot, and a test checks `logo_default` fits both
(`the_default_logo_fits_each_consumers_slot`): the title card 5 rows from row 8, between its
ramp lines (`BOOT_LOGO_ROW`, `BOOT_LOGO_ROWS`), and About 8 rows from row 2, in its band of
raster bars (`ABOUT_LOGO_ROW`, `ABOUT_LOGO_ROWS`), both centred.

**No transparent background.** About's bars stop at the logo's footprint, a black box. A
cell holds three colours (background, glyph, overlay); Pulse Train's letter cells use all
three, and a bar takes two a cell of its own (a lower-half glyph), so the bars cannot pass
behind the letters. Tried with the letter cells taking the bar's colour as their
background: the bar's lower half took its upper half's colour in every letter cell, the
letters' bottom bands and the cyan wave vanished into bars of the same ramp colours, and
About sent 110 cells a frame instead of 95 (14.3 ms at worst). The gaps between the letters
are 4 pixels, inside cells that hold letter pixels too, so no bar shows between them either.

Four concepts from the prototype, all drawn in code on the cell grid:

| Concept | Idea |
|---|---|
| **Pulse Train** (default, built) | Slotted raster letters on a pulse wave: the SID's signature waveform is the underline. In motion the stripes roll through the raster ramp and the duty cycle sweeps; the boot screen uses the moving version. |
| Three Voices | Triangle, saw and pulse traces weave over and under the letters, one per voice. |
| READY. | The name typed at the prompt in quadrant blocks, with a blinking cursor; it lives on the cell grid, so animating it costs one cell. |
| 28 Pin | SIDOU where the part number would be on a 28-pin chip; the pins light with the voice gates. |

**Pulse Train** on the grid: the letters are a 5 x 7 face at 4x, so each letter pixel is a
cell quadrant. Their 28 pixel rows are seven bands of 3 rows and a slot, coloured through the
raster ramp. A cell row holds two bands: the top one as a `SLOT_*` glyph in its colour, the
bottom one as an overlay in the next band's. The pulse wave (2 pixels thick, cyan) is an
overlay on the two rows below. The stripes roll one band every 4 frames (frames 2, 6, ...),
the wave moves 4 pixels and its duty cycle sweeps every 4 frames (frames 0, 4, ...), and odd
frames change nothing, which leaves them to About's raster bars.

**Decision to confirm with the user:** Pulse Train is the default logo until the user picks
one. Richer pixel-art variants are under consideration too (a chip mascot, cracktro chrome, a
cassette, keycaps, a scope trace, a desk scene, a sprite sheet, a badge), some of them full
240 x 240 bitmaps; changing `logo_default` swaps the logo everywhere.

### Logo bitmaps

A cell holds three colours at most (background, glyph, overlay), so a pixel-art logo in the
16 colours cannot be drawn with glyphs. It would be supplied like the font, and drawn
through a direct pixel region of the renderer. Neither is built yet.

- **Source:** an indexed PNG in `assets/logo/` (e.g. `assets/logo/<name>.png`), at most
  240 x 240, whose palette entries are art palette colours (the table above, exactly); a
  colour outside it is an error. A width and height in whole cells keep the footprint exact.
- **Conversion:** a generator in `host/crates/sidou-tests` (as `src/font.rs` is for the
  font) writes `app/logo_<name>.c`: 4 bits a pixel, each pixel row run-length coded (a run of
  one colour as a count and an index), with a table of row offsets so that any 8-pixel cell
  row can be decoded on its own. A full-screen logo is 28.8 KB unpacked; the runs of flat
  colour in pixel art usually cut that by half or more. A test checks the generated file
  against the PNG and fails when it falls behind (`SIDOU_LOGO_UPDATE=1` rewrites it, as
  `SIDOU_FONT_UPDATE` and `SIDOU_CREDITS_UPDATE` do), and a placeholder PNG of a few cells
  keeps the path tested before a real logo exists.
- **Drawing:** the grid gets a pixel region, a rectangle of cells whose pixels
  `grid_render_run` takes from a callback (here: decode the bitmap's rows for the run's cell
  row) instead of glyphs and overlays. The logo's `draw` sets the region over its footprint
  and marks its cells to be sent when its frame changes. A still bitmap costs its cells once:
  a full screen is 77 ms of SPI, so it goes out under a wipe (8 frames), as a view switch does.
  An animated one (a blinking part) resends only the cells that change.

What works, and what does not, on this panel and budget:

- **A still logo, or a full-screen splash, works.** It goes out once, under a wipe, and costs
  nothing after that; the decoder runs only for the cells being sent.
- **A 240 x 240 logo needs a title layout of its own.** It is the whole screen: no room for
  the title card's ramp lines, name, version and PRESS ANY KEY (a 5-row slot), nor for About's
  credits and scroller (an 8-row band). It replaces the card rather than filling its slot.
- **Palette cycling is not free.** The panel takes RGB565 pixels, not indexes into a palette
  it keeps: changing a colour means resending every cell that shows it. A cycle over a whole
  screen of that colour is a full screen of SPI (77 ms) per step, so it is only affordable
  over a few cells, or a step every several frames.
- **Small logos** of up to three colours a cell (a mono or two-tone mark) need no region:
  overlays carry them, one cell each, as Pulse Train's bands do.

## Boot

The boot sequence (`app/boot.h`) is a view over the status screen at power-on, after the
prototype's `Boot`:

| From | Shows |
|---|---|
| 0 s | `*** SIDOU V<version> ***`, the chip and voices, `READY.` and the cursor |
| 0.8 s | `LOAD"SIDOU",8,1` typed, 70 ms a character |
| 2.2 s | `SEARCHING FOR SIDOU` |
| 3.0 s | `LOADING`, and raster stripes in the border, new every other frame |
| 5.6 s | `READY.`, then `RUN` typed from 6.2 s |
| 7.0 s | the title card: the logo between two ramp lines, `SID FOR THE FM-1`, the version and licence, `PRESS ANY KEY` blinking |
| 10.0 s | the status screen wipes in |

The stripes split each border cell row in two colours at a random height (a lower-eighths
glyph): the 116 border cells, about 10 ms of SPI, every other frame. Every other change goes
out within the budget; the first frame is drawn whole before the display is on.

- **Short by default:** the title card alone, 1.2 s, then the status screen. Without storage
  the firmware cannot tell the first boot after an install from the others, so it does not
  pretend to. **HOME held while powering on** plays the full sequence: once the interrupts
  are on, `platform/fm1/main.c` waits `FM1_DEB_PRESS + 1` key scan frames (3.3 ms; the audio
  already runs) and reads HOME's debounced level. The key scan also makes one press edge for
  a key held at power-on; the boot takes that first HOME press as part of the hold. Without
  HOME held, a HOME press skips like any other button, from the first frame on. With M2's
  storage the full sequence will play once after an install (a flag in flash), and short
  after that ([TODO](../TODO.md)).
- **Skippable by anything:** a button or an encoder ends it at the next frame and does nothing
  else (the boot's capture takes it: no octave step, no chip toggle); a note from the keys or
  MIDI ends it and plays, even one shorter than a frame (`live`'s note-on count tells).
  What MIDI changes meanwhile (the chip, the octave) shows no toast or raster over it.
- **It holds nothing up:** the audio interrupt, the tick (the key scan, MIDI) and the
  interrupts start before the screen's first frame, and the main loop sends that frame like
  any other, without waiting on the panel (a full screen, about 80 ms of SPI on the device);
  the display comes on once it is out. The keys and MIDI play from the start: in fm1-emu the
  audio starts 394 ms after power-on, after `lcd_init`'s 350 ms of reset and wake-up delays
  (`the_audio_and_the_keys_start_before_the_display`). Before, the first frame went out
  whole, blocking, before the audio started.

## About

ABOUT, the menu's last item (`app/about.h`), is the prototype's cracktro trimmed to the SPI
budget, on black: two raster bars sweep behind the logo, a ramp line, the credits over a
starfield, another ramp line, and a sine scroller. HOME goes back.

- **Credits** come from `CREDITS.md`: `host/crates/sidou-tests/src/credits.rs` generates
  `app/credits.c` (a page per section, more when it runs over 8 lines, wrapped at 26
  characters with names kept whole, accents dropped for the font). A credit longer than a
  page starts a page and goes on under `<SECTION> (CONT.)`, cut between who and what; no
  other credit shares a page with its tail, so no sentence runs into the next credit. A test fails when it falls behind the file;
  `SIDOU_CREDITS_UPDATE=1` rewrites it. A page shows 4.8 s; a first page names SIDOU, its
  version and licence.
- **The trim**, from the prototype's 240 cells a frame to 93: the bars are 5 bands of 4
  pixels (two a cell, with a lower-half glyph) and move one band at a time, one bar on each odd
  frame; the logo moves on even frames; the credits' rows cascade in and out 4 frames apart,
  fading through 4 colours a level every other frame, so at most two text rows change in a
  frame; the scroller's letters are overlays shifted 2 pixels a frame, each bobbing a whole cell
  row on a sine two screens long; the stars are one-pixel overlays in three layers moving a
  pixel every 4, 2 or 1 frames, on blank cells only. Measured above (Motion rules).

**Naming:** the instrument is **SIDOU**, always in capitals, in the font or
the logo's own letters. "SID" refers to the chip only when describing what
SIDOU emulates.
