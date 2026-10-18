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
- **Large text** (`grid_text_big`) draws a string 2 or 3 times as large, from the same 8×8
  font and no new data: each character is 2×2 or 3×3 cells, its glyph pixel-doubled or
  tripled and cut into the cells, and each cell draws its own part. A cell holds a **piece
  code** instead of the glyph's (`GRID_BIG_FIRST` = `0x01`: `+ y * 2 + x` at 2×, `+ 4 + y * 3 +
  x` at 3×, `GRID_PIECE_PLAIN` = `0x0E` for a glyph at its own size that no overlay shows
  over, `grid_put_top`), and the grid keeps the character it is a piece of (`src`, 900 bytes
  of RAM). A piece is a blank glyph in the font, so a view that draws over its cells with
  ordinary text simply shows its own glyphs, and large text needs no clearing on a view
  switch; the renderer tests one cell code to tell a piece (about 2 instructions a cell,
  the pieces' own pixels cost about as much as an ordinary cell). 2× is 16-pixel characters
  (15 to a row), 3× 24-pixel (10 to a row); the font's 7×7 ink and one-pixel gap become 14
  and 2, 21 and 3. Used by the value popup, the toasts and the menu (below); the dense
  views (LIVE's scopes, TRACKER, CHIP, STATUS, ABOUT) keep the 8×8.

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

The sheet draws 163 glyphs (lower case reuses upper case). The face is
chamfered capitals with 2-pixel stems, 7×7 pixels in the top-left of the
cell, so text keeps a one-pixel gap to the right and below.
Graphics glyphs fill the whole cell, so they join up. The firmware's glyph
sheet (the CHARSET view in `app/views.c`, opened from the menu) shows them
all, with the palette (the codes `0x01`-`0x0E`, large text's piece codes, blank glyphs, are drawn
as blanks); its fm1-emu snapshot is `tests/golden/lcd/charset.png`.

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
| `0xDC` | `SCOPE_MID`: LIVE's dotted scope centre line, a pixel every 4 on the top row |
| `0x01`–`0x0E` | blank in the font, and the cell codes of large text's pieces (above) |
| `0x00`, `0x0F`–`0x1F`, `0xDD`–`0xFF` | blank, free for later glyphs |

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
  the outer ring (the main menu, drawn at 2× over the whole width, has none).
- **Menus:** a list of items, each a label and a value right-aligned against
  the edge (an arrow for an item that opens a view), SELECT moves the selection, ALGORITHM
  changes the value. The main menu (HOME) is drawn at **2× text** with six items of the
  thirteen showing: a title bar in reverse video (MENU, an arrow up and down for the items out
  of sight, the item's number of the count) in rows 0-2, a line of 14 characters every 3 rows
  from row 4 (a cell of margin each side: a panel's bezel hides its edge), and the selected
  item's whole line in reverse video (no blinking block: a 2×2-cell block would send two
  windows a blink, and a bar reads better at a glance). A line is up to 60 cells, so a scroll
  (every line changes) is drawn a line or two a frame, the selected line first, within 190
  cells less what a popup or a toast draws the same frame (`MENU_FRAME_CELLS`, `menu.budget`),
  and at once for the rows a layer let go or a view switch left (`menu.force_rows`). Labels
  are short so that value and label fit a line: **PRE** is PRESET (the longest preset name,
  NOISE PERC, is 10 characters), **ROWS** TRACKER ROWS, and STORE shows `193K` (kilobytes
  free), `FOREIGN`, `FORMAT?` or `ERASE 20`; the popup says the whole names. The help below
  (`SELECT  MOVE`, `ALG   CHANGE`: ALG is ALGORITHM) is large too. The menus of M2's views
  (and a CHIP-sized menu of the song's) can use the same widget at either size
  (`menu.scale`).
- **Toast:** reverse-video text (blue on light blue) in **2× text** on rows 27-28 for 1.5 s,
  saying what changed: up to 13 characters; a longer one breaks at a space into two lines
  (rows 25-28, up to 26 characters), and one whose words fit neither (`SNG: A-4 PITCH NOT
  PLAYED`) shows at the font's own size on row 27.
- **Popup:** a change of a parameter from the panel (a K knob, ALGORITHM, MASTER) opens a box
  over the middle of the view, rows 10-18 and columns 1-28: a PETSCII frame in light blue on
  black, the parameter's name in yellow at **3× text** (24-pixel characters, up to 8; 2×, up to
  12, for a longer name), its value in white as large, and for a continuous value a bar of
  24 cells (green on dark grey, eighths of a cell, `FONT_H0..8`). It opens 3 rows a frame (78
  cells), stays 1 s after the last change and closes 3 rows a frame, bottom first, the rows it
  lets go composed again by the view under it (the screen forces that). It is a screen layer
  like the toast, over any view: it covers its rows (`screen_cover`, so a view holding to a
  cell budget leaves them alone and sets aside what the popup draws), and guards them from the
  view's own large text (`grid_big_guard`). Its cells are plain pieces
  (`grid_put_top`): the overlays of LIVE's scopes under the box do not show, and the
  overlay changes under it send nothing (`grid_unstale`). It waits while a wipe or a raster
  runs, and a view switch takes it away at once. A change comes in as one byte the input path
  stores (`popup_ask`), and the frame asks the views for the text (`popup_text` in
  `app/views.c`, the one place that names and formats a control's value): MIDI changes do not
  ask (`VIEWS_POPUP_MIDI`).
- **Wipe:** a view opens top first and closes bottom first, 4 rows a frame
  behind a white edge row, so the full redraw a view switch needs spreads
  over 8 frames (about 320 ms). The rows not reached yet keep the old view
  on the panel.
- **Bands:** a cut spread over frames, for a view that comes in under a full-screen picture
  (the title cards, below): the view on top shows 6 rows a frame, top first, with no edge
  row (`SCREEN_BANDS`), 5 frames (200 ms), at most 180 cells, 15.4 ms of SPI, a frame.
- **Raster:** colour bands in the raster ramp's order run down the border,
  one row every other frame, then the border gets its own colour back. A chip
  change runs it once round the ramp (7 steps, 14 frames). It waits while a
  wipe runs.

Measured in fm1-emu (`screen_changes_and_transitions_stay_within_the_budgets`):

| Change | Frames | Windows | SPI bytes | Worst frame |
|---|---|---|---|---|
| Wipe up from the status screen to the menu | 8 | 52 | 78,908 | 11.6 ms |
| Wipe down from the menu to the status screen | 8 | 48 | 77,200 | 10.4 ms |
| Raster and toast (chip change) | 15, every other one | 451 | 123,105 | 10.5 ms |
| Menu idle (the selection: no blink) | none | 0 | 0 | 0 ms |

The animated screens compose every frame (`the_boot_and_about_stay_within_their_budgets`,
260 frames of the full boot and 1920 of About, a full round of its 16 pages), with Metal
Chrome:

| Screen | Cells a frame (average, most) | Windows a frame | SPI a frame (average, worst) | Main loop a frame (average, busiest `ui_poll`) |
|---|---|---|---|---|
| Full boot | 24, 150 | 8.1 | 2.1 ms, 12.8 ms | 37.3k, 33.5k instructions |
| About | 86, 163 | 15.9 | 7.5 ms, 14.0 ms | 117k, 71k instructions |

About costs 2.9M instructions a second (1.2 % of the core at 240 MHz). With Cracktro Chrome
it was 87 cells a frame, 118k and 72k, and the boot 37k and 33k. With Pulse Train
(M1e Task 4) it was 94 cells a frame (148 at most, 12.8 ms), 144k and 91k, and the boot
46k and 37k: the logo was animated, and the chrome logos are still. Metal Chrome's glow is
dithered, mostly runs of one and two pixels: with the decoder's loop over each run, a wipe
frame of its scene took the boot's busiest `ui_poll` to 41.4k; a run of one or two pixels is
now two stores, with no clamp while the longest run still fits (33.5k). A run's pixels are
written two at a 32-bit store, a cell row from two 4-entry tables (the glyph's colours, and
the overlay over the background for a blank glyph row): About was 161k and 105k, the boot
50k and 64k, with a pixel a store. A bitmap's run fills two pixels a store as well.

**Large text** (M2f) costs what it sends: the popup opens and closes 78 cells a frame, a value
change at most about 104 (the value's three rows and the bar), the menu a line or two (60
cells each) a frame, a scroll 3 to 6 frames; none of them more than 160 cells or 14 ms of
SPI a frame in fm1-emu (`the_popup_stays_within_its_budgets_over_every_view`,
`the_menu_stays_within_the_cell_and_spi_budgets_as_it_scrolls_and_under_a_popup` on the host):

| Popup over | Cells a frame (most) | Worst SPI | Busiest `ui_poll` | Static popup, extra a frame |
|---|---|---|---|---|
| LIVE, a note held | 132 | 11.3 ms | 30.7k | 7.3k (LIVE composes every frame) |
| STATUS | 132 | 11.3 ms | 33.7k | 0.15k |
| MENU | 132 | 11.3 ms | 36.5k | 0.15k |
| CHIP | 132 | 11.3 ms | 35.4k | 0.1k |
| SCENE (RULE 30, a note held) | 158 | 13.6 ms | 31.3k | 12.5k (SCENE composes every frame) |

A popup's frame that asks for it opens nothing (the view's own compose of a changed control
takes most of a frame: CHIP's detent is 35.7k in its busiest `ui_poll`, bound 38k); it
draws the rows it adds, and the text once its rows are in, and again only when the view
composed or the text changed. The input path stores two bytes and a pointer (the control and
the view it was made on: 13 instructions on a K1 turn, which is 218 on LIVE and STATUS, 234
on the menu and 230 on SCENE, bound 250; CHIP's 210, bound 215). The popup's state (names,
timer, the source) is not in the screen's struct but in the views', after everything else, so
that no field the input paths reach moved (the tightest input gates have a few
instructions to spare).

A wipe sends only the rows that differ, once each, and its edge row once
more; a raster step recolours the 116 border cells in 58 windows. Every frame
stays inside the 20 ms budget.

A wipe is sized for a view that holds still: the rows it has revealed must not change
under it. The animated views (About, the boot's title card) hold their animation while a
wipe or a raster runs (`screen_animating`), so a wipe into them sends its 150 cells a frame
at most, 12.8 ms. Revealing Metal Chrome's bitmap costs the main loop little more than cells:
the wipe into the title card (its whole scene, 4 rows a frame) 138k instructions a frame and
33k in its busiest `ui_poll`, into About 124k and 64k (Cracktro Chrome's: 134k and 30k,
118k and 65k). Before, About's bars, logo and
scroller moving in the revealed rows made
a frame of the wipe into it 243 cells, 21.0 ms, and the title card's stripes 192 cells,
16.6 ms (`every_wipe_into_an_animated_view_sends_only_its_rows` in fm1-emu, and
`the_wipe_into_about_sends_only_its_rows` and `the_wipe_into_the_title_card_sends_only_its_rows`
on the host, at every phase).

## UI sounds

SOUNDS in the menu (off by default, restored with the settings) plays short sounds on chip B's spare voice
(`app/ui_sound.h`): a cursor blip when the menu's cursor moves (a triangle C-6, two frames), a
confirm when ALGORITHM changes a value or opens a view (a pulse arpeggio E-5 G#5 B-5), a chime
with a toast (a triangle falling E-6 C-6 G-5), an error buzz for a refused action (the
cursor at the menu's end, an action turned counter-clockwise: a narrow pulse at C-2), and
SAVE's drive chunk (noise, then a low pulse, when an explicit save commits). They are
data in GoatTracker's sound-effect format (its readme, section 6.3), stepped at 50 Hz, so they
can be edited in GoatTracker and converted with INS2SND2. They are quiet and short by design:
the instrument is the thing to hear. They never take a voice the keys use: with the keys on
chip A they play on chip B, which is otherwise silent; with the keys on chip B (a song
playing) only on a voice that has finished its release, and a note that needs it takes it
back at once.

## Logo

**Metal Chrome, chosen by the user (replacing Cracktro Chrome)**, is SIDOU's logo: spiked,
blade-cut SIDOU letters (horn serifs, a sharp angled S, diamond counters in the D and O,
fangs on the U) with a chrome bevel running red, light red, orange, yellow and white, a black
outline and a 3-pixel purple extruded shadow, on a dithered purple and red glow (an airbrush
ellipse and a 13-pixel halo round the letters, in the 4 x 4 Bayer order), between warm raster
bars, with stars, a cross-flare glint on the U and the tagline "FORGED IN SILICON FOR THE
FM-1", on black, in the 16 colours only. It is original art made for SIDOU, drawn pixel by
pixel in code: the logo page's variant M1.

![Metal Chrome on the panel](../../assets/logo/metal-chrome.png)

| Asset | What |
|---|---|
| `assets/logo/metal-chrome.js` | The source: the logo page's drawing code (variant M1), verbatim. `node assets/logo/metal-chrome.js` writes the PNGs below; they match Chromium's canvas drawing the page pixel for pixel. |
| `assets/logo/metal-chrome.png` | The scene, 240 x 240, 4 bits a pixel in the art palette's order: what the firmware shows. |
| `assets/logo/metal-chrome-lettering.png`, `-lettering-2x.png` | The lettering, its outline and its shadow alone, 226 x 63, on transparency (palette index 16); the 2x, by nearest neighbour, heads the project README. |
| `docs/art/avatar.png` | The avatar, 48 x 48: the S in its glow, between two stars. |
| `assets/logo/cracktro-chrome.js`, `cracktro-chrome*.png` | Cracktro Chrome (variant V2), exported the same way; its avatar is `cracktro-chrome-avatar.png`. |
| `app/logo_bitmap.c` | Both scenes as `grid_bitmap`s, generated (below). |

**Cracktro Chrome**, the logo until the user chose Metal Chrome and now an alternative:
chunky bevelled SIDOU letters in raster bands (white, light grey, cyan, light blue and blue
over brown, orange, light red and yellow), a black outline and a 4-pixel purple extruded
shadow, in a frozen demo-scene intro: stars, a cold and a warm raster bar and a perspective
grid floor.

The logo is a **pluggable asset** (`app/logo.h`): the boot's title card and About draw
whichever logo `logo_default` names, through a table in flash:

| Field | Metal Chrome (default) | Cracktro Chrome | Pulse Train |
|---|---|---|---|
| `cols`, `rows`: the footprint in cells | 30 x 9 (240 x 72 pixels, the scene's lettering band) | 30 x 8 (240 x 64) | 15 x 5 (120 x 40 pixels) |
| `overlays`: overlay cells at most | 0 | 0 | 55 |
| `cells_per_frame`: the most cells one frame changes | 0 (still) | 0 (still) | 42 |
| `draw(grid, col, row, frame, bg)` | the band in the pixel region | the band in the pixel region | frame `frame` of the animation (25 fps, 0 the still) |
| `scene(grid, frame)`: the title card's full screen | the whole scene in the pixel region | the whole scene in the pixel region | none (NULL) |

`draw` paints every cell of the footprint, glyph, colours, overlay and pixel region (it clears
an overlay it does not need), so a view composes it like the rest of its screen. The declared
cost and overlays are measured (`host/crates/sidou-tests/tests/app/logo.rs`), so a test fails
when a logo's drawing and its table disagree. `overlays` and `cells_per_frame` are 16-bit,
room for a logo up to the whole screen (900 cells).

Where it goes, and a test checks each logo fits (`each_logo_fits_each_consumers_slot`):
- **The title card** shows the logo's scene when it has one, with the version on row 1 and
  PRESS ANY KEY on row 28, both rows black in both chrome scenes (`BOOT_SCENE_VERSION_ROW`,
  `BOOT_SCENE_KEY_ROW`). A logo without a scene gets the old card: a slot 5 rows from row 8,
  between two ramp lines (`BOOT_LOGO_ROW`, `BOOT_LOGO_ROWS`), the name and the version below.
- **About** gives it 9 rows from row 0 (`ABOUT_LOGO_ROW`, `ABOUT_LOGO_ROWS`), centred:
  Metal Chrome's lettering band whole, Cracktro Chrome's on rows 0-7, Pulse Train on rows 2-6.

The emulator checks both against `assets/logo/metal-chrome.png` pixel for pixel: the
title card everywhere but its two text rows, and About's band against the scene's pixel
rows 80-151.

**The lettering band** is the scene's cell rows 10-18 (`LOGO_METAL_BAND_ROW`): the letters'
chrome takes pixel rows 83-141, their black outline and purple shadow 81-145, and nothing of
the letters' red, orange or yellow reaches outside the band; its stars and glow come with it.
The glow spreads beyond the band, so About shows it cut at the band's foot, against its
ramp line (the band's top is the screen's). The lettering is 65
pixel rows with its outline and shadow, one more than Cracktro Chrome's 64-row band holds:
About's band grew from 8 rows to 9, its first ramp line moved to row 9, and the stars between
the ramp lines lost a row; the credits keep theirs. Cracktro Chrome's band is its scene's cell
rows 9-16 (`LOGO_CHROME_BAND_ROW`).

**Pulse Train**, the other alternative (built), on the grid: the letters are a 5 x 7 face at 4x, so
each letter pixel is a cell quadrant. Their 28 pixel rows are seven bands of 3 rows and a
slot, coloured through the raster ramp. A cell row holds two bands: the top one as a `SLOT_*`
glyph in its colour, the bottom one as an overlay in the next band's. The pulse wave (2 pixels
thick, cyan) is an overlay on the two rows below. The stripes roll one band every 4 frames
(frames 2, 6, ...), the wave moves 4 pixels and its duty cycle sweeps every 4 frames (frames
0, 4, ...), and odd frames change nothing. `logo_default = &logo_pulse_train` brings it back
everywhere. Its letter cells use all three colours a cell holds, so raster bars cannot pass
behind them: tried with the letter cells taking the bar's colour as their background, the
bar's lower half took its upper half's colour in every letter cell, the letters' bottom bands
and the cyan wave vanished into bars of the same ramp colours, and About sent 110 cells a
frame instead of 95 (14.3 ms at worst).

The other concepts from the prototype, drawn in code on the cell grid, were Three Voices
(triangle, saw and pulse traces weaving through the letters), READY. (the name typed at the
prompt in quadrant blocks) and 28 Pin (SIDOU on a 28-pin chip whose pins light with the
gates). The logo page's other **metal variants** stay explorations: Blood Chrome (Metal
Chrome's letters in cold steel with a red underglow) and Metal Emblem (a chrome shield with
horns, bolts and the spiked lettering). Its **chapter cards**, full-screen title cards in the
Metal Chrome lettering, became the views' title cards ("Title cards", below).

### Logo bitmaps

A cell holds three colours at most (background, glyph, overlay), so a pixel-art logo in the
16 colours cannot be drawn with glyphs. It is supplied like the font and drawn through the
renderer's pixel region.

- **Source:** an indexed PNG in `assets/logo/`, at most 240 x 240 and in whole 8 x 8 cells,
  whose pixels are art palette colours exactly (its palette in any order); anything else is
  an error.
- **Conversion:** `host/crates/sidou-tests/src/logo.rs` (as `src/font.rs` is for the font)
  writes `app/logo_bitmap.c`, a `grid_bitmap` (`app/grid.h`) for each logo: each pixel row
  run-length coded, a byte a run of 1..16 pixels (`(length - 1) << 4 | colour`), with a table
  of where each row's runs start so any row decodes on its own; rows alike share their runs,
  within a scene and across the scenes (one array of runs). A test fails
  when the file falls behind the PNGs (`SIDOU_LOGO_UPDATE=1` rewrites it, as
  `SIDOU_FONT_UPDATE` and `SIDOU_CREDITS_UPDATE` do), and others check the round trip and
  that the compiled table decodes to the PNG pixel for pixel.
- **Size:** Metal Chrome's whole scene is **9,753 bytes** of flash: 9,273 of runs and 480
  of row offsets, 33.9 % of its 28,800 bytes at 4 bits a pixel; its glow's dither makes runs
  of one and two pixels. Cracktro Chrome's is 4,526 bytes (4,046 and 480, 15.7 %; 6,739
  without sharing rows: the black rows and the 2-pixel bars repeat). Together, sharing their
  black rows, 14,234. So the whole scene goes on the device, and About takes its band from
  the same bitmap.
- **Drawing:** `grid_region(g, col, row, w, h, bitmap, bx, by)` makes w x h cells take their
  background, pixel for pixel, from the bitmap's cells from (bx, by). Their glyphs'
  foreground and their overlays still draw over it, so text and overlays work on a picture
  (the title card's version and PRESS ANY KEY); a blank glyph shows the bitmap alone.
  `grid_render_run` decodes the run's part of each of its 8 pixel rows straight into the
  panel's pixels, two a store (a run of one or two pixels two plain stores), then draws the
  glyph and overlay pixels of the cells that have any. One region at a time: setting it marks the cells it leaves and covers to be sent, the
  same region again sends nothing, and a logo drawn in cells or a view switch clears it.

What works, and what does not, on this panel and budget:

- **A still logo, or a full-screen splash, works.** It goes out once, under a wipe (4 rows a
  frame, 12.8 ms of SPI at most), and costs nothing after that; the decoder runs only for the
  cells being sent. A full 240 x 240 repaint in one frame would be about 77 ms.
- **A 240 x 240 logo needs a title layout of its own.** The title card is the scene, with
  only two lines of text on rows the scene leaves black, and About takes the lettering band.
- **Palette cycling is not free.** The panel takes RGB565 pixels, not indexes into a palette
  it keeps: changing a colour means resending every cell that shows it. Rolling Cracktro
  Chrome's raster bands through the lettering would resend its 207 lettering cells in 22
  windows, 26.7 KB or 17.8 ms of SPI a step, on top of About's own frame (up to 163 cells):
  over the 20 ms budget. The stripes stay still; a roll would need a frame of its own every
  few frames, with About's other motion paused for it.
- **Small logos** of up to three colours a cell (a mono or two-tone mark) need no region:
  overlays carry them, one cell each, as Pulse Train's bands do.

## Boot

The boot sequence (`app/boot.h`) is a view over LIVE at power-on, after the
prototype's `Boot`:

| From | Shows |
|---|---|
| 0 s | `*** SIDOU V<version> ***`, the chip and voices, `READY.` and the cursor |
| 0.8 s | `LOAD"SIDOU",8,1` typed, 70 ms a character |
| 2.2 s | `SEARCHING FOR SIDOU` |
| 3.0 s | `LOADING`, and raster stripes in the border, new every other frame |
| 5.6 s | `READY.`, then `RUN` typed from 6.2 s |
| 7.0 s | the title card: the logo's scene (Metal Chrome's), the version and licence above it, `PRESS ANY KEY` blinking below |
| 10.0 s | LIVE wipes in |

The stripes split each border cell row in two colours at a random height (a lower-eighths
glyph): the 116 border cells, about 10 ms of SPI, every other frame. Every other change goes
out within the budget; the first frame is drawn whole before the display is on.

- **Full once per firmware version:** when the stored version is missing or differs from
  the installed version, the full sequence plays and the current version is saved with the
  settings. Later boots of that version show the title card alone, 1.2 s, then LIVE.
  Reinstalling the same version does not reset this choice. Let the settings save finish
  before powering off: its background write is debounced for 5 s and waits for quiet. A
  foreign store cannot remember the version until explicitly formatted, so it keeps showing
  the full sequence. **HOME held while powering on** always plays the full sequence: once the interrupts
  are on, `platform/fm1/main.c` waits `FM1_DEB_PRESS + 1` key scan frames (3.3 ms; the audio
  already runs) and reads HOME's debounced level. The key scan also makes one press edge for
  a key held at power-on; the boot takes that first HOME press as part of the hold. Without
  HOME held, a HOME press skips like any other button, from the first frame on.
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
budget, on black, laid out around Metal Chrome's lettering band: the logo at the top
(rows 0-8), a ramp line (row 9), the credits over a starfield (heading on row 10, lines from
row 12), another ramp line, and two raster bars sweeping behind a sine scroller (rows 21-29).
The bars are Cracktro Chrome's cold and warm ones (blue, light blue, cyan; brown, orange,
light red), with no white, and the scroller is white, so its letters read over every band.
HOME goes back.

- **Credits** come from `CREDITS.md`: `host/crates/sidou-tests/src/credits.rs` generates
  `app/credits.c` (a page per section, more when it runs over 8 lines, wrapped at 26
  characters with names kept whole, accents dropped for the font). A credit longer than a
  page starts a page and goes on under `<SECTION> (CONT.)`, cut between who and what; no
  other credit shares a page with its tail, so no sentence runs into the next credit. A test fails when it falls behind the file;
  `SIDOU_CREDITS_UPDATE=1` rewrites it. A page shows 4.8 s; a first page names SIDOU, its
  version and licence.
- **The trim**, from the prototype's 240 cells a frame to 87: the logo is still, sent once
  under the wipe; the bars are 5 bands of 4 pixels (two a cell, with a lower-half glyph) and
  move one band at a time, one bar on each odd frame; the credits' rows cascade in and out 4
  frames apart,
  fading through 4 colours a level every other frame, so at most two text rows change in a
  frame; the scroller's letters are overlays shifted 2 pixels a frame, each bobbing a whole cell
  row on a sine two screens long; the stars are one-pixel overlays in three layers moving a
  pixel every 4, 2 or 1 frames, on blank cells only. Measured above (Motion rules).

**Naming:** the instrument is **SIDOU**, always in capitals, in the font or
the logo's own letters. "SID" refers to the chip only when describing what
SIDOU emulates.

## LIVE

LIVE (`app/live_view.h`) is the root view, what the screen shows from power-on, after the
prototype's: a header line in reverse video (chip, preset, octave); each voice in a box
(rows 1-6, 7-12, 13-18) with its oscilloscope, 160 x 32 pixels, its envelope as two columns
of lower eighths (ENV3, 32 levels), and its note, waveform, pulse width and envelope stage;
FILTER, the response of the mode at the cutoff and resonance in 16 columns of lower eighths
(30 Hz..12 kHz, -36..+12 dB, the cutoff's column yellow, recomputed only when they change,
from the core's fitted filter tables); VU, each voice's level and the output's left and
right peaks after the effect slots, 6 dB a cell; and the three effect slots with their
algorithm and Mix. The status screen moves to the menu (STATUS). M2's STEP view will be the
default then.

**The tap.** The audio interrupt renders a block in chunks of 16 samples and after each
writes a point into a ring of 256 (`app/scope.h`): each voice's waveform output (its top 8
bits) and its oscillator's phase (the accumulator's top 8 bits). Once a block it copies each
voice's ENV3, envelope state and sustain level, and raises the left and right peaks with
every 8th frame of the output. Rendering in chunks gives the samples one call gives, so the
audio is unchanged (the device's output matches the host's sample for sample with LIVE
showing). It runs only while LIVE shows: on top, or under MIDI learn's frame. Single producer, single consumer, lock-free: the
interrupt overwrites the oldest points and publishes its head once a block; the main loop
copies the newest 224 and drops the copy if the interrupt came round to it meanwhile.

| Tap | Measured (fm1-emu, `the_live_view_tap_costs_the_audio_interrupt_little`) |
|---|---|
| Points | 2,757 a second (a frame: 110; a read: 224, 81 ms) |
| Audio interrupt | +4.44 instructions a sample (406.71 against 402.27, a chord held, LIVE against the menu, since M1b Task 6's 6581 filter and the points from inside one render a block, `sid_render_tap`; 4.8 at M1e), bounded at 4.5 (`TAP_PER_SAMPLE`, which the effect gates count) |
| Code in `.ram_text` | 32,472 of 49,152 bytes (+308: `scope_render`, `scope_peaks` and their calls) |
| RAM | the ring and envelopes 1,560 bytes; LIVE's state 6,152 (its traces and their columns, the points it reads, its canvas) |

**The scopes are folded by phase** (an equivalent-time display): each point lands in the
column its oscillator's phase belongs to, two to four periods across (more for higher
notes), a column showing the mean of its points and the empty ones a line between their
neighbours. A held note's trace stands still at any pitch, and 110 points a frame draw it
whole where a time base would need 20 times as many. A hard-synced voice is folded by its
sync source's phase, so the sync shape shows. The height is the waveform times the envelope:
flat at rest. A trace that moved by a pixel at most stays as drawn (the fold's wobble), so a
held chord sends about 5 cells a frame.

**The budget.** Each trace is an overlay per cell it crosses (at most 240 of the pool's 255).
Three voices of noise change about 240 cells a frame, 20.5 ms of SPI: over the budget. LIVE
holds itself to 200 cells a frame (`LIVE_VIEW_FRAME_CELLS`, about 17.5 ms with the windows):
the scopes get what the rest of the frame and the screen's toast and raster leave
(`screen_layer_cells`), and a trace that does not fit keeps its last look and goes first next
frame, so the voices take turns. The canvas (everything but the scopes) is composed off the
grid, built whole once and then a part at a time: the header, a voice's box, FILTER, VU and
the slots are drawn again only when what they show changes, an envelope bar or a meter only
its own cells, and a toast's frames put back only the toast's row. Going back to the menu
leaves the traces flat and the envelopes and meters empty, so LIVE comes back at rest rather
than showing what it showed when it went. The tap is
read half way between frames (`views_prepare`), so no main-loop pass is long enough to hold
up MIDI (the M1d latency gate). While a wipe or a raster runs LIVE holds still, redrawing the
overlays a view switch cleared one voice a frame, ahead of the wipe.

Measured in fm1-emu (`live_stays_within_its_budgets`):

| LIVE | Cells a frame (average, most) | Windows a frame | SPI a frame (average, worst) | Main loop a frame (average, busiest `ui_poll`) |
|---|---|---|---|---|
| At rest | 0, 0 | 0 | 0, 0 | 13.7k, 4.2k instructions |
| A chord of saws, from the note-ons (75 frames) | 4.5, 160 | 1.8 | 0.4 ms, 13.9 ms | 47.5k, 59.1k |
| Three voices of noise (75 frames) | 146, 161 | 23.7 | 12.6 ms, 14.0 ms | 141k, 60.5k |

The noise case costs 3.5M instructions a second (1.5 % of the core at 240 MHz). Where a
frame of it goes (each function with its callees): rendering the 146 cells 33k, finding
the runs 16k, LIVE's compose 52k (setting the scope cells' overlays 27k of it), reading and
folding the tap half way between frames 30k. Before the canvas was drawn a part at a time
and the pixels two at a store it was 206k (the compose 84k, the rendering 64k), and the
busiest `ui_poll` 89k. The plan's 70-90k a frame holds for a held chord, not for noise;
the busiest `ui_poll` is half the latency gate's poll allowance (0.5 ms, 120k instructions,
which `screen.rs` holds every view's worst to). With the TRS jack at full speed (a note-on
and its note-off every 1.92 ms, `midi.rs`), LIVE's frames cost 84k (115k before) and the
main loop runs 5.7M instructions a second, 2.4 % of the core. The status screen, idle,
still composes nothing: 7.4k a frame (7.3k before LIVE).

## CHIP

CHIP (`app/chip_view.h`, from the menu behind THE CHIP's card) is a chip's sound on one page,
chip A or chip B, on the status screen's colours: light blue on blue inside the full-block
border, the focus in white, what is off in grey.

![CHIP, the focus on the chip](../../tests/golden/lcd/chip.png)
![CHIP, the focus on DIST](../../tests/golden/lcd/chip-dist.png)
![CHIP's B tab, the focus on the tabs](../../tests/golden/lcd/chip-b.png)
![CHIP's B tab with the keys on chip B](../../tests/golden/lcd/chip-b-keys.png)
![CHIP's A tab during a song, the song's chip](../../tests/golden/lcd/chip-a-song.png)

- **Row 2:** CHIP, and the chips' tabs from column 12, 8 cells each: **A** and **B**, the
  shown one in reverse video (blue on light blue; on white while the tabs have the focus,
  with the play mark in the margin), **KEYS** beside the letter of the keys' chip.
- **Frames** from column 2, 26 wide, one after the other: CHIP (rows 4-7: the profile and the
  model, values right-aligned against the frame), CHARACTER (rows 8-13: AGE, DAC, THUMP,
  BLEED, each a 12-cell bar of `BAR` eighths and its value), then DIST, MOD and FX (4 rows
  each: the slot and its algorithm set into the frame's top edge, and two columns of two
  controls, each its name for the algorithm and its value).
- **The focus:** the focused section's frames and text in white, and a play mark (`PLAY`) in
  the margin at column 1 on each of its frames' top rows; the chip's focus is both its frames.
- **Grey:** a slot's control its algorithm does not use (TAPE's A and B), and every control
  of a slot that is off, under its plain name (AMT, A, B, MIX).
- **Row 3:** the tab's note, right-aligned against the frames: on the B tab FOLLOWS KEYS
  SLOTS SHARED, or SLOTS SHARED WITH CHIP A while the keys play chip B; on a song's A tab SONG
  SETUP  SLOTS SHARED; nothing on chip A with no song.
- **Row 26:** the help line, SELECT MOVE ALGORITHM STEP, on every tab. The toast keeps row 27.

**The budget.** The page is built in a canvas off the grid whenever what it shows changes
(a byte each for the profile, the model, the character, every slot field, the selected slot
and the focus), and its changed cells are copied onto the grid within 200 cells a frame
(`CHIP_VIEW_FRAME_CELLS`) less what the toast, the raster and a cover take, as SCENE copies its
canvas: a frame that changes more spreads over the next ones. The cells MIDI learn's frame or
a toast covers are left alone, and the border while the raster runs. Idle it compares its
bytes and composes nothing. Measured in fm1-emu (`chip_stays_within_its_budgets`):

| CHIP | Cells a frame (average, most) | Worst SPI | Main loop a frame (average, busiest `ui_poll`) |
|---|---|---|---|
| Idle | 0, 0 | 0 | 6.3k, 0.3k instructions |
| A knob's detent (25 frames) | 0.08, 2 | 0.19 ms | 7.7k, 34.2k |
| A tab switch (25 frames; M2b Task 5) | 1.6, 40 | 3.44 ms | 8.3k, 34.8k |
| The busiest: every control over MIDI at once, a focus move and a chip change (50 frames) | 26.9, 200 | 17.62 ms | 37.4k, 28.6k |

The busiest `ui_poll` is a build of the page and the scan of its 900 cells. SELECT,
ALGORITHM and a knob cost the input's call at most 198 instructions, and the busiest main-loop
pass after one 44.5k (SELECT from FX to the chip, three frames changing colour), within the
latency gate's 120k poll allowance together. ALGORITHM on the chip, a profile step, is the
exception: its pass is the chip reset (decision record 0006, "Cost"), as the FX button's and
CC 85's are. At the page's character a step to the 8580 costs 362.6k (bounded at 400k); the
worst, onto WORN at full character, 471.5k (ALGORITHM; CC 85 471.4k, FX 470.7k), bounded at
520k for all three. RAM: the view's state 1,840 bytes, most of it the canvas.

## TRACKER

TRACKER (`app/tracker_view.h`, from SEQ or the menu) is the song's patterns in GoatTracker
2's layout, squeezed to the 30 x 30 grid after the prototype
(`assets/prototypes/sidou-screens.html`): black, the header in reverse video.

![TRACKER, stopped](../../tests/golden/lcd/tracker.png)
![TRACKER, SIDOU LOOP playing, the moving bar](../../tests/golden/lcd/tracker-play.png)
![TRACKER, scrolling](../../tests/golden/lcd/tracker-scroll.png)
![TRACKER, voice 1 muted, REC](../../tests/golden/lcd/tracker-mute-rec.png)

- **Row 0:** the song views' header (`app/song_view.h`): TRACKER, a play mark or a stop
  square, the tempo in BPM, ORD and ROW in hex, REC in red.
- **Row 1:** the voice heads, from columns 3, 12 and 21: V1-V3 in the waveform's colour, its
  name (MUT in light red, SOL in white), a one-cell envelope meter on dark grey.
- **Rows 2-26:** 25 pattern rows. Columns 0-1 the row number (white on the bar, yellow every
  16, light grey every 4, grey); then each voice's row of its pattern, eight cells
  `C-401308`: the note in white (`---` a rest in dark grey, `===` a key off and `+++` a key on
  in grey), the instrument in light blue (`..` none), the command in yellow and its data in
  light green (`...` none). The bar is light blue on blue across columns 0-28, its
  instruments cyan and its empty cells light blue. A silent voice is all dark grey.
- **Column 29:** a dark grey rule and a light blue pip where the bar's row is in its pattern.
- **Row 27:** a rule. **Row 28:** OL, and each voice's orderlist from its entry: the entry in
  white on blue, the two after it in grey (a pattern in hex, `R2` a repeat, `+3` or `-1` a
  transpose, `RST00` the end and its restart position). **Row 29:** the bar row's first
  command, of the voices that sound, spelled out in yellow (`V2 4 22 VIBRATO`).

Each voice shows the pattern of its orderlist entry: while the song plays the entry and row
of its last row event, stopped the cursor's. **The moving bar** (TRACKER ROWS BAR, the
default): the rows stand still and the bar moves down the 16-row page with 4 rows of context
above; at the page's end the window moves a page. **Scrolling** (TRACKER ROWS SCROLL,
GoatTracker's): the bar stays on screen row 14 and the rows move under it.

**The budget.** The header and the heads are built into the song views' shared canvas when
they change (the meters, REC), the rows when the window, the bar, a pattern or a mute does;
only the rows built are compared with the grid, and their changed cells go out top first
within 200 cells a frame (`SONG_VIEW_FRAME_CELLS`) less what the screen's layers take. Measured
in fm1-emu over 30 s of each (`the_tracker_stays_within_its_budgets_while_a_song_plays`):

| 30 s playing | Cells a frame (average, most) | SPI a frame (average, worst) | Main loop a frame (average, busiest `ui_poll`) | Frames at the cap |
|---|---|---|---|---|
| SIDOU LOOP (tempo 6), bar | 25.6, 129 | 2.22, 11.35 ms | 27.6k, 31.4k | 0 of 750 |
| SIDOU LOOP, scroll | 83.4, 200 | 7.30, 17.56 ms | 51.0k, 34.0k | 256 of 750 |
| A dense song at tempo 3 (a note on every row of every voice), bar | 61.6, 200 | 5.35, 17.79 ms | 49.8k, 38.4k | 38 of 750 |
| The same, scroll | 100.4, 200 | 9.08, 17.58 ms | 89.9k, 36.8k | 24 of 750 |

The bar always keeps up: a row step changes its two rows and the line under the rows (60 to
90 cells), and only a page turn (up to 725 cells) is spread over up to four frames, top first,
behind the bar. The scroll changes every row whose text differs from the one above it: with
SIDOU LOOP, at tempo 6, a third of the frames already reach the cap, so the scroll stops
keeping up at about tempo 6 on patterns like it (the rows lag the music by up to a frame or
two, never the sound); a song whose rows look alike (the tempo-3 song, whose cells differ in
a note's name only) changes fewer cells a step and lags less, even at tempo 3. Idle and
stopped it composes nothing (the song views' table in `tests/screen.rs`: 8.1k instructions a
frame idle). RAM: the song views' state 1,920 bytes, most of it their shared canvas.

## SCENE

SCENE (`app/scene_view.h`, the menu's item after STATUS) is M1e's prototype of M2's visuals:
the browser prototype's five generative pieces (`app/scene.h`), ported to the cell grid, each
reacting to the voices, and since M2f a sixth, RED MOON RITE (below), drawn in pixels. SELECT
steps between them; the new one wipes in and a toast names it.
What moves each one is in [controls](../reference/controls.md#scene); in short, a voice's
note-ons trigger its part (a flame's flare, a ripple, a generation, lit stars, an ant's walk),
its envelope (ENV3, from LIVE's tap, which runs while SCENE shows) keeps it going, its pitch
sets a height or a place, a silent voice's part stops, and the cutoff and resonance set the
mood (heat, speed, twist, darker colours). Randomness is a seeded xorshift, so the same music
gives the same frames: the fm1-emu goldens (`tests/golden/lcd/scene-*.png`) are each piece a
second into a held chord.

**FIRE's look.** The flame's ramp is black, red, light red, a thin band of yellow (lime on this
palette) and white; brown is only the ember bed's glow, and orange (olive-brown here) is not
used: with it the fire read as dirt. The cutoff moves where the white starts (heat 196 open,
beyond any heat shut, on a curve that rises fast, so the presets' cutoffs already burn yellow and
white at the bed), not the heat itself, so the flames keep their shape. The fuel sways along the
bed (two sines drifting apart), a cell drifts a column only one time in four, and one cell in
eight cools by three times more, so the flames break into tongues instead of a mound.

**The canvas.** Each piece draws into a 30 x 30 canvas of its own (1,800 bytes); the view
copies it onto the grid. So a piece that changes a few cells a step (the ant, RULE 30) draws
only those, and SCENE's compose still draws every cell (`app/screen.h`). The pieces share their
state in a union, about 1 KB; SCENE's RAM was 2,784 bytes in all, and RED MOON RITE shares
that memory with them (below).

**The budget.** The copy sends at most 200 cells a frame (`SCENE_VIEW_FRAME_CELLS`, about 17.5 ms
of SPI with their windows): changed cells from where the last frame stopped, until the frame has
spent what the screen's toast, raster and cover leave; a frame that changes more spreads over the
next ones, so a changed cell waits at most about 900 / (the cells left to the copy) frames. What
a toast or the raster drew over is put back the same way; what a toast that shows or MIDI
learn's frame covers this frame (`screen_covered`) is left alone, and learn's frame counts what
it sends (`screen_cover`): LIVE does the same. Only under a wipe are rows copied whole (the wipe
holds the ones it has not reached). On top of that, the two pieces that
change most of the screen in the prototype are trimmed to fit, so the cap rarely bites:

| Piece | Prototype (cells a frame) | On the FM-1 |
|---|---|---|
| FIRE | 426 (the whole screen) | the bottom 14 rows, three 10-column flames; it burns every other frame (two steps at once, so the flames rise at the prototype's speed) and draws half its rows a frame, the even ones then the odd ones: 12.5 fps, at most 210 cells a frame. Heat at or below 48 goes out above the ember bed, so silence holds still |
| PLASMA | 500 | the whole screen, every other row a frame; its top speed is 0.8 rad a second (the prototype's 3.7 changed more cells than the budget has), an eighth of it in silence; a note-on's ripple is one ring growing half a cell a frame, not a train of rings, and voice 3's warmth follows its envelope at most 4 steps a frame |
| RULE 30 | 20 | as the prototype: a 60 x 60 ring of generations in quadrant glyphs, at most 3 generations a frame |
| STARS | 25 | 90 stars, a glyph each, drawn far to near |
| ANT | 3 | as the prototype |

Measured in fm1-emu (`scene_stays_within_its_budgets`, 150 frames a piece of a band on the keys:
an arpeggio note every 4 frames, a bass note every 8, the filter wide open):

| SCENE | Cells a frame (average, most) | SPI a frame (average, worst) | Main loop a frame (average, busiest `ui_poll`) |
|---|---|---|---|
| FIRE | 125.0, 159 | 10.93 ms, 13.85 ms | 101.6k, 58.9k instructions |
| PLASMA | 138.4, 200 | 12.29 ms, 17.64 ms | 141.4k, 53.0k |
| RULE 30 | 18.9, 76 | 1.65 ms, 6.60 ms | 32.7k, 21.3k |
| STARS | 39.5, 68 | 3.59 ms, 6.16 ms | 74.1k, 21.1k |
| ANT | 2.7, 9 | 0.25 ms, 0.80 ms | 29.3k, 18.9k |

The piece steps half way between frames (`views_prepare`), as LIVE reads its tap, so a frame's
`ui_poll` is the copy alone; the busiest `ui_poll` (FIRE's two burns, 58.9k) is half the latency
gate's poll allowance (120k). A piece picked (SELECT, or the menu's ALGORITHM) starts in the
compose of the wipe's first frame, not in the input's call (`ui_encoder`), which the main loop
runs in the same pass as a frame between two drains of MIDI; PLASMA's start, 48.8k when it drew
the whole canvas in the input, draws the 4 rows the wipe reveals each frame. The input now costs
at most 203 instructions and the busiest pass after it (from `midi_drain` to `midi_service`)
60.7k, 61k together against the 120k allowance
(`starting_a_piece_holds_the_main_loop_within_the_poll_allowance`). MIDI learn's frame over
SCENE or LIVE stays within 200 cells a frame: it sent 222 over PLASMA and 286 (24.8 ms of SPI)
over LIVE before the views left its cells out of their budget (now at most 200 and 191).
PLASMA costs 3.4M instructions a second (1.4 % of the core at 240 MHz). A chord's note-ons show
within two frames, and once it has died away every piece calms
(`a_note_moves_every_piece_and_silence_calms_it`, cells in 25 frames):

| Piece | Silence before | A chord held | Silence after |
|---|---|---|---|
| FIRE | 0 | 1,580 | 0 |
| PLASMA | 274 | 2,728 | 207 |
| RULE 30 | 0 | 233 | 0 |
| STARS | 205 | 998 | 112 |
| ANT | 0 | 60 | 0 |

The tap adds to the audio interrupt what it adds for LIVE (4.8 instructions a sample), and only
while SCENE shows. The firmware grew 7.5 KB (the pieces' code, the plasma's distance and sine
tables, 1.2 KB).

## Pixel scenes

A drawn vignette (RED MOON RITE, below) is too many colours a cell for glyphs and too big
for a frame buffer (240 x 240 at 4 bits is 28.8 KB of RAM), so it is **composited a tile at a
time, on demand** (`app/scene_px.h`):

- **The grid's painter region.** Besides a bitmap in flash, the grid's pixel region can take
  its pixels from a painter (`grid_region_painter`): a function the renderer calls for each
  cell of the region as it sends it, filling its 8 x 8 colours; glyphs and overlays still draw
  over them. The grid sends only the cells marked (`grid_mark`), so the painter's owner says
  what changed.
- **Layers.** The compositor holds a stack of up to 24 layers in slots, bottom first: fills,
  gradients (a sky computed on the device), images (`scene_image`, placed by their frame's
  top-left, mirrored or not, with an optional 16-colour remap, clipped to a rectangle), marks
  (single pixels: a spark's rays, an arc, small discs), text in the 8 x 8 font, and calls (a
  function that paints a row span: colour cycling over a mask). Each layer knows its box, so a
  tile draws only the layers over it: RED MOON RITE's frames stack 9 to 11 layers and a tile
  visits 2.2 to 2.3 of them on average.
- **What changed.** Setting a slot to a layer that differs from what it held (any field: a
  layer whose drawing changes while its fields do not changes `param`) makes candidates of
  the tiles under its old and new boxes. The scan composites each candidate into an 8 x 8
  scratch, hashes it (FNV-1a, 32 bits: 3,600 bytes for the screen) and marks it only if the
  hash differs from the panel's. The hash is taken again as the grid sends the tile, so it is
  always what the panel shows; a candidate the grid is about to send anyway is dropped
  without compositing. A tile is therefore composited twice when it changes (once to find
  that it did, once to send it) and once when a layer over it changed without changing it.
- **The invariant.** A run is counted as shown, and its tiles' hashes taken, when it is
  rendered, so every rendered run must reach the panel, or the grid be invalidated. A view
  that has the screen back without setting its painter region again (which sends every cell)
  calls `scene_px_init` or `grid_invalidate` first.
- **The refresh sweep.** `scene_px_refresh` marks the next tiles round the screen whatever
  their hashes say: a tile or two a frame sends the whole screen again every 18 to 36 s, a
  safety net for a change the candidates missed (a forgotten `param`), at no RAM cost (a
  position) and about 4.4k instructions and 0.19 ms of SPI a frame for two tiles.
- **Runs, slices and the budget.** `grid_render_run` composites a run's tiles as it renders
  it, and `ui_poll` renders up to two runs a call, so a run takes at most
  `SCENE_PX_RUN_CELLS` (4) of the painter's cells (`grid_region_painter`'s cap). The scan
  takes `SCENE_PX_SCAN_SLICE` (8) candidates a call at most, in passes of the main loop that
  render no run. The emulator gate ties both to the 120k poll allowance at compile time, with
  a song's analysis slice (39k) in the same pass: they were 8 and 16 until RED MOON RITE's own
  tiles were measured (5,930 at worst, over the bench's 5,300). The
  view sends at most 200 cells a frame, and the grid goes on from where it stopped, round
  the screen, so a marked tile waits at most one sweep. 200 cells in as many windows (the
  worst the cap can make) take 18.5 ms of SPI, within the 20 ms.

**What it costs on the device**, in fm1-emu (`emu-gate`'s `scene_budget.rs`, a frame like
RED MOON RITE's with 13 slots, `bench/workload_scene.h`; instructions, a lower bound on
cycles):

| | Instructions |
|---|---|
| A tile composited, average / worst | 1,959 / 4,826 |
| A cell sent (the tile, its hash, the panel's pixels), average / worst | 2,779 / 5,646 |
| A run of 8 cells, worst; two in a ui_poll (Task 3; the cap is 4 since Task 4) | 35.5k; 71k |
| A scan, a candidate (the tile and its hash) | 2,829: 42 tiles a 120k pass |
| A slice of 16 candidates, worst (Task 3; 8 since Task 4) | 51.9k |
| The whole screen sent (the first frame) | 2.49M |
| The fighters step 4 px: 132 candidates, 123 sent | 0.89M |
| The sky's palette changes: 480 candidates, 247 sent | 1.99M |
| The torches tick: 10 candidates, 10 sent | 95k |

As first written a tile cost 4.8k on average and 8.7k at worst, more than half of it finding
a span's run from the start of the row, and a run of 30 painted cells 230k. Three changes
brought it down: the rows' checkpoints (In flash, below), a sprite drawn into the tile a run
at a time instead of a pixel at a time (`scene_image_blend`, with no divide a pixel), and a
tile looking only at the slots in use. A 40 ms frame leaves about 5.2M instructions after
the audio: RED MOON RITE's busiest frames (176 tiles changed in the prototype, perhaps 250
candidates) come to about 1.2M, a quarter of it.

In RAM the compositor is 4,716 bytes on the device (the slots 768 and their boxes 192, the
slots in use 25, the hashes 3,600, a bit a tile for the candidates 113, counters), and the
grid grew 12 bytes (the painter, its context and its cap). The tests (`tests/app/scene_px.rs`)
check tiles against a Rust model of the same layers in random stacks (mirroring,
transparency, order, remaps, clips), the hash against every change (the panel always equals
the composited screen, and exactly the changed tiles are sent), the runs' cap, the sweep
healing a missed change, any interleaving of changes, sliced scans, runs rendered and left
waiting, delayed sends and held rows (400 seeds of 3,000 steps each converge on the screen),
and against frames that `assets/scene/red-moon-rite.js` draws (`tests/golden/scene/`: seven
frames of the arcade's pattern as the layers the compositor stacks, the torches' cycling and
the status bar left out; `--frames` writes them, and `SIDOU_SCENE_UPDATE=1` regenerates them
when Node is installed).

## RED MOON RITE

RED MOON RITE is SCENE's first music-driven vignette for the device (M2f): two barbarians
with broadswords under a blood moon in a torch-lit ruin. The kick is an overhead chop, the
snare a parry with a spark, the hi-hats footwork, the bass decides who presses forward, the
arp flares the torches and the cutoff darkens the sky in three palettes. The bar-4 fill is a
spinning attack that **disarms** the other fighter: his sword spins away and sticks point-down
in the flagstones, and in bar 1 he bends and pulls it free; nobody attacks while he is
unarmed. It is a homage to late-80s fantasy sword-fighting games, not to any one game:
original art in the 16 colours, with no game's name, logo, sprite, pose, arena or HUD.

![RED MOON RITE: guard, a parry, the spinning attack, the sword in flight, in the floor, pulled free](../../assets/scene/red-moon-rite-preview.png)

| Asset | What |
|---|---|
| `assets/scene/red-moon-rite.js` | The source: the Scene Arcade's drawing code (`assets/prototypes/sidou-vignettes.html`: the helpers, the painter, the puppet, RED MOON RITE), verbatim but for the changes its header lists, plus the new drawing below. `node assets/scene/red-moon-rite.js` writes the PNGs. |
| `sky-0.png` .. `sky-2.png` | The three sky palettes with the moon, wisps and hills: the screen's rows 16-139, 240 x 124. |
| `arena.png` | The temple and floor, 240 x 240 in screen coordinates, transparent where the sky shows. |
| `barb-0-<pose>.png`, `barb-1-<pose>.png` | The two fighters (0: bare-headed, dark hair, red headband; 1: steel helmet, red plume) in the eight poses (guard, guard2, raise, chop, block, spinA, spinB, hit): 64 x 124, stored at half width (each stored pixel is two on the screen, `gx = 2`); fighter 1 is drawn mirrored. Hip at (58, 72) of the screen-width sprite; the header gives the placement. |
| `barb-0-<pose>-unarmed.png`, `barb-1-<pose>-unarmed.png` | Each fighter without his sword: guard, guard2, hit, and reach (bending to pull it from the floor). |
| `sword-0.png` .. `sword-7.png` | The sword in flight, eight rotations of 45 degrees, each painted whole (no rotated pixels), grip at the centre of 84 x 84 screen pixels. |
| `sword-stuck.png` | The sword point-down in a flagstone, leaning a little, a crack round its point. |
| `flame-mask.png` | The torches' flame area, where the colour cycling draws. |
| `hud.png`, `hud-blade-empty.png`, `hud-blade-full.png`, `hud-blade-tip.png` | The status bar (black, VS, the red line) and side 0's blade meter as the device stacks it: empty, full (drawn clipped to the stamina) and the tip; side 1 is drawn mirrored (M2f Task 4). |
| `red-moon-rite-motion.txt` | The motion the script computes once for the device: each fighter's sword flight and where it sticks, the spark's three stages (M2f Task 4). |
| `red-moon-rite-preview.png` | Six frames of the vignette playing the arcade's pattern (above). |

All PNGs are indexed: the art palette's 16 colours in order, and index 16 transparent where a
PNG has transparency. A test (`scene_rmr_drawing`, in sidou-tests' app tests) runs the script
twice when Node is installed and fails if the runs differ or the PNGs fall behind it.

**The disarm** replaces the prototype's helmet knock (the helmet flew off fighter 1's head and
he fought bare-headed until bar 1). At the spinning attack's last beat, the other fighter's sword
leaves his hand in the `hit` pose; its middle follows a parabola for 20 frames (0.8 s)
and turns a whole number of times, so it lands point-down with its grip where his hand will
be in `reach`. The fighters hold their ground while it is out of his hand. On the first beat
after it sticks he takes `reach` for 8 frames, then has it back in `guard`: with the arcade's
pattern it lands near bar 1's second beat and is back in his hand by its fourth. The fill's
spinner is whoever leads and he disarms the other, so either fighter can be disarmed (both
have unarmed poses); with the arcade's pattern the leader is always fighter 0, so the preview
shows fighter 1 disarmed.

**The status bar** shows each fighter's stamina as a broadsword: the hilt at the screen's
edge, the blade pointing in toward `VS` in eight notches, each stamina lost taking the notch
at the tip (the tip moves in), what is lost left as a dark ghost of the blade. The prototype's
eight square pips a side were replaced after the review below.

### In flash

`host/crates/sidou-tests/src/scene_art.rs` converts the PNGs into `app/scene_rmr_art.c`
(`SIDOU_SCENE_UPDATE=1` regenerates it; a test fails while it is stale), a `scene_image`
(`app/scene_art.h`) each, decoded a row span at a time by `scene_image_row`, or drawn over a
tile a run at a time by `scene_image_blend`:

- **Run-length rows with transparency.** Each image is cropped to the rectangle its pixels
  cover inside its frame; each pixel row of the rectangle is coded as the logo bitmaps' are (a
  byte a run of 1..16 pixels, `(length - 1) << 4 | colour`, and a table of where each row's runs
  start), with one colour that no image of the set uses (cyan) coding a transparent run, so a
  row decodes with the bitmaps' loop. Rows alike share their runs across every image. A sprite
  keeps its frame's size, its anchor (a fighter's feet at (58, 112) of the 128 x 124 screen
  frame; a sword's grip) and `gx`; drawn mirrored, a row is read from its end and the anchor is
  (frame width - 58, 112), so no mirrored copies are stored.
- **Checkpoints.** A row wider than 32 pixels has a run start at every 32 pixels of it (the
  converter splits a run there) and a byte for each in a checkpoint table, so a span part way
  along a row is found from the checkpoint before it, not from the row's start: a tile of the
  arena once walked 40 runs on average (118 at most) to reach its pixels, 2.6k of a tile's
  4.8k. It takes 4,715 bytes of tables and 1,121 of split runs.
- **Patches.** An unarmed pose is coded as the rectangle where it differs from its armed pose
  (the arm and the sword), over that pose, when that is smaller: guard, guard2 and hit, 120 to
  188 bytes each instead of 813 to 931; reach has no armed pose and is whole.
- **The skies** are a gradient computed on the device (`scene_gradient_row`: the prototype's
  stops and 6-row Bayer seams) and an overlay of the pixels it does not give (the moon, wisps
  and hills): **7,472 bytes for the three, against 11,284 coded whole** (rows shared across
  the three, checkpoint tables aside). The gradient costs a fill a pixel on its flat rows and
  a Bayer compare a pixel on its seam rows (24 of its 124): measured in fm1-emu, 230
  instructions a tile averaged over the screen (about 430 on each of the 480 tiles it
  covers), so it was chosen.
- **The arena** is one image of the whole screen, transparent where the sky shows.

| What | Bytes |
|---|---|
| Fighters, 16 armed poses (781 of checkpoints) | 17,718 |
| Unarmed poses (6 patches, 2 whole reaches) | 2,461 |
| Swords, 8 in flight and stuck | 1,878 |
| Skies (3 overlays) and the arena (3,934 of checkpoints) | 21,880 |
| Together, rows shared: 30,741 of runs, 9,955 of row offsets and checkpoints, 1,098 of tables | **41,794** (40.8 KB of the 48 KB ceiling) |

The kinds are each image's runs and offsets alone; sharing rows across images brings the sum
down to the total. A test prints the total and fails over 48 KB (`the_art_fits_its_flash_ceiling`),
and another decodes every image, mirrored too and from any pixel, back to its PNG pixel for
pixel. Since M2f Task 4 the set also holds the status bar (`hud.png` and the blade meter
empty, full and its tip, 4 images) and the motion tables (below): **43,413 bytes** (42.4 KB of
the 48 KB ceiling), all in the image now that the vignette draws them.

### On the device

`app/scene_rmr.h` is the prototype's logic in C: its `step` (once a row of the music) and its
layers' and stages' state (once a frame), with the disarm, integer only. A host test plays it
against the script frame for frame (`tests/golden/scene/rmr-trace.tsv`, 768 frames of the
arcade's pattern, written with `--frames`: positions, poses, timers, stamina, the sword's path
and where it sticks, the sky's rows; all equal), and another stacks its layers for the
script's golden frames (`tests/golden/scene/rmr-*.png`): equal pixel for pixel but the spin's
arc (the device's sine, `app/sine.h`, within a pixel of the script's) and the torches and the
status bar, which those frames leave out. What the script computes once is in
`assets/scene/red-moon-rite-motion.txt`, which it writes beside the PNGs and the converter
compiles: each fighter's sword flight (19 grips and rotations from his place, then where it
sticks) and the spark's three stages (pixels around the blades' meeting point).

**The music** (`app/scene_song.h`): the song control's row, note and gate events, which the
main loop drains once a pass and hands to each listener (`platform/fm1/main.c`), and chip A's
cutoff as the player wrote it; with no song, the instrument. The voices' roles and drums come
from the song's `SCNE` chunk (`docs/reference/song-format.md`) or are guessed (a noise-first
wavetable is the drums, by its first step's pitch; the lowest other voice the bass; the rest
the arp); the place in the bar is the pattern's row modulo 4 beats of the `MIDI` chunk's rows
a beat (4); muted voices play no part ([controls](../reference/controls.md#red-moon-rite)).

**The layers**, bottom first (21 slots of 24): the status bar, each meter (empty, full clipped
to the stamina, the tip), the sky (one call: each cell row its palette's gradient and overlay,
so the wipe touches 30 tiles a frame), the arena, the two torches (a call each: six colours
cycling up a wobbling flame, sparks over it when flared), the stuck sword behind the fighters,
each fighter's shadow and sprite (mirrored for fighter 1, an unarmed patch over the armed pose),
the spark (a call over the stage's rows), the arc (14 marks), the stuck sword over the hand
that pulls it free, the flying sword, and the cover: over what the screen draws on the scene
this frame (the toast's row, MIDI learn's frame, the raster's border) each cell that is not
blank shows its own background under its glyph. The step is in SCENE's compose, so a frame's
cells go out together after it; the scan then runs a slice of 8 candidates in each `ui_poll`
that has nothing to send, until the frame has spent its 200 cells. Silence calms it: a second
after the last gate the torches and the arc hold still, the refresh sweep stops, and nothing is
sent.

**What it costs**, measured in fm1-emu over 60 s of the demo song
(`red_moon_rite_stays_within_its_budgets_over_a_minute_of_the_song`, `emu-gate` screen.rs;
instructions; rebased onto main `084fea9`):

| RED MOON RITE | Measured | Bound |
|---|---|---|
| Tiles (cells) sent a frame, average / most (the prototype: 117 / 176) | 98.931 / 170 | 109 / 180 |
| SPI a frame, average / worst | 8.727 ms / 14.954 ms | 20 ms |
| Main loop a frame (ui_poll) | 908.6k (17 % of a frame's 5.2M) | 1.0M |
| Busiest ui_poll / main-loop pass | 57,353 / 57,754 | 62k / 63k, each with a song's 39k slice within 120k |
| Busiest scan slice (8 candidates) / run (4 cells) | 56.3k / 24.6k | 62k / 27k |
| Costliest tile / a tile on average | 5,930 / 2,892 | 6,500 |
| The keys' band (scene_stays_within_its_budgets) | 120.5 cells a frame, 1.03M | 133, 1.14M |
| Its start (SELECT, or the menu into it) | 58.7k the busiest pass | 65k |

The pair drifting a pixel a frame is most of it: both fighters' and shadows' tiles change.
The audio is the host's sample for sample while it shows
(`the_first_song_is_bit_exact_while_red_moon_rite_plays_it`, emu-gate song.rs). The LCD goldens
`scene-rmr-song`, `scene-rmr-disarm` and `scene-rmr-stuck` are the demo song a second in, the
fill's landing and the sword in the floor; `scene-rmr` the keys' chord, as for the other pieces.

**In RAM** RED MOON RITE shares the PETSCII pieces' memory (a union in `scene_view`): the
compositor (4,724 bytes) and the vignette's state (136) take the place of a piece and its
canvas (about 2.7 KB); the music hooks (`scene_song`, about 240 bytes) are always there. SCENE
grew 2,318 bytes, 5,112 in all. **In flash**, the art 43,413 bytes and the code about 6.6 KB
(`scene_rmr.c` and `scene_song.c` at -Os, the view's part, the event fan-out), with the
compositor's and decoders' 3.9 KB (M2f Tasks 2-3) that the linker kept out until now: the
image grew 54,416 bytes against main `084fea9`: 479,584 bytes in all, below M2f's 512,000-byte
ceiling. The free-RAM gate measures 15,756 bytes (RAM region 10,060, POOL 5,696), against this base's
9,000-byte gate (22,000 less M2d's 6,000, M2a's 2,000 and M2f's 5,000). The linked RAM use
grew 2,336 bytes against that base, including alignment and the music hooks' data.
Excluding the 4-byte alignment gap before `.bss`, the free region tails total 15,752 bytes.
RAMTEXT is unchanged at 44,404 bytes of 49,152. The code came to about 10.5 KB including
the compositor and decoders, against the 10 KB budget; optional wisp parallax is not done.

### The originality review

The preview and the sprite sheets were set beside a gameplay still of the genre's best-known
C64 game (a one-on-one barbarian duel, 1987), looked at outside the repository and described
here in words only, and checked point by point:

| Point | That game | RED MOON RITE | Verdict |
|---|---|---|---|
| Fighters' builds and costume | Two small, thin hi-res figures, bare-chested in loincloths, the same figure for both players | Two bulky shaded multicolour figures (64 x 124 pixels, `gx = 2`) of SIDOU's painter: one bare-headed with dark hair and a red headband, one in a round steel helmet with a red plume, grey or red loincloths, leather bracers and boots | Different; the nearest point is the genre's archetype both share (bare chest, loincloth, broadsword). Fighter 0's red headband is the detail to watch |
| Poses | Its own animation set | Joint angles of SIDOU's puppet (guard, raise, chop, block, spin, hit, reach), not traced from any game | No pose matched |
| Arena | An enclosed stone hall: a wall with two arched windows and spectators in them, grey flagstones, pillars wrapped in snakes with skulls at the screen's sides; a forest glade elsewhere; a goblin who drags the loser off | An open ruin at night: a raster sky in three palettes, a blood moon, wisps and hills, a broken arch, a broken and a whole plain column with torches, steps, flagstones in perspective; no spectators, no snakes, no skulls, no goblin | Different; both have stone floors and a column toward each edge, plain here |
| HUD | A large title banner at the top centre with the scores under it; each fighter's energy as six red circles in the top corners | Prototype: eight square pips a side from the corners inward, the first two red, then yellow, `VS` between | **Too close** (a row of red markers per fighter in the top corners): replaced by the blade meter, which has no title, no score, no circles, and steel instead of red |
| Finishing moment | A decapitation, and the goblin | A disarm: the sword spins away and sticks in the floor; no head or helmet comes off, no blood, nobody is hurt while unarmed | Different |
| Names and lettering | Its title logo | No logo or title in the piece; `VS` in the 3 x 5 font; no game's name anywhere | Clean |

**Sign-off: signed off by the user on 2026-10-16,** as it is: the fighters, the ruin, the
blade-meter HUD and the disarm. The two readings above were accepted with it: the sword is
back during bar 1 (not before it), and with the arcade's pattern only fighter 1 is disarmed.
For any song (M2f Task 4) the fill's leader disarms the other fighter, whichever he is.

## Title cards

A major view entered from the menu opens behind a **title card** (`app/title_card.h`), after
the logo page's chapter cards: THE and the view's name in Metal Chrome's lettering on a
dithered purple and red glow with stars, a rule under the title and a glint on its last
letter. There is no "CHAPTER" line. **THE SCENE** comes before SCENE, **THE OSCILLATOR**
before LIVE when HOME goes back to it from the menu (not at power-on: the boot's title card
is enough), and **THE CHIP** before the CHIP view. The held frames are `assets/logo/title-oscillator.png`,
`title-scene.png` and `title-chip.png`, drawn by `assets/logo/metal-chrome.js` (`titleCard`,
the page's `chapterCard` adapted to the cell grid) with the glow alone in `title-glow.png`:

| THE OSCILLATOR | THE SCENE | THE CHIP |
|---|---|---|
| ![THE OSCILLATOR](../../assets/logo/title-oscillator.png) | ![THE SCENE](../../assets/logo/title-scene.png) | ![THE CHIP](../../assets/logo/title-chip.png) |

**The sequence** (25 fps):

1. **The glow fades in** through four ordered-dither levels. The glow is drawn a cell at a time
   with the font's dither glyphs D0..D15 (the 4 x 4 Bayer order the art dithers with) in two
   neighbouring stops of black, blue, purple and red, from a table of each cell's level
   (0..47, 16 to a stop; a quarter of the screen, as the glow is symmetric about its centre).
   Level k of 4 is the table's level times k / 4, so the fade costs no flash. Each level holds
   2 frames once all its cells are out: 25 frames in all (1.0 s), the first level spread over
   5 frames as it changes all 900 cells of the menu under it.
2. **The title wipes in** from the left, 3 columns a frame (10 frames, 0.4 s): the card's
   bitmap in the grid's pixel region, grown over the same bitmap cells, so a frame sends the
   90 cells it adds at most.
3. **It holds** 25 frames (1 s), sending nothing.
4. **The view cuts in** under it in bands (5 frames, 0.2 s; it starts there, SCENE's piece
   in the cut's first frame).

About 2.6 s from the input to the view.

**What ends it.** Any button or encoder ends it at the next frame and does nothing else (its
capture takes every event); a note from the keys or MIDI plays as ever and ends it (the
instrument's note-on count, so however short the note; a note held from before does not).
The audio, the keys and MIDI run as always; nothing holds them up. No toast or raster draws
over it, and what changed meanwhile is not toasted after it.

**The option.** CARDS in the menu (after OCTAVE) turns the cards off (ALGORITHM
counter-clockwise) and on (clockwise); on at power-on, not stored yet. With them off, SCENE
and LIVE wipe in as the other views do.

**The bitmaps.** A card's bitmap holds its pixels only in the cells that differ from the glow
(the letters, their halos, the rule, the glint, the stars: 251 cells for THE OSCILLATOR, 331
for THE SCENE, 261 for THE CHIP, a bit each in `title_card_cells`), where the firmware puts a
blank glyph so the bitmap shows alone; in the others it holds the glow's background stop flat,
under the dither glyph the firmware draws there. So the bitmaps compress well. The converter
(`host/crates/sidou-tests/src/title_card.rs`) reads the glow's levels from `title-glow.png`
(refusing a cell that is not a dither glyph of two neighbouring stops, or a glow that is not
symmetric), finds each card's cells and writes `app/title_card_bitmap.c` (SIDOU_LOGO_UPDATE=1
regenerates it, and a drift test fails while it is stale). In flash, the three cards are
**27,087 bytes**: 25,062 of runs shared by the cards, 1,440 of row offsets, 360 of cell bits and
225 of glow levels (27,128 in the firmware's `.rodata`, with their tables), and 0.7 KB of code;
drawn into each bitmap, the glow's dither would make them 57,190. Most of it is the letters'
dithered halos, runs of one or two pixels.

**The budget.** A frame sends at most 200 cells (`TITLE_CARD_FRAME_CELLS`, about 17.5 ms of SPI
with their windows): a step that changes more goes out in bands from where the last frame
stopped. Measured in fm1-emu (`title_cards_stay_within_their_budgets`, from the input to the
end of the cut, nothing played):

| Card | Frames | Cells a frame (average, most) | Worst SPI | Main loop a frame (average, busiest `ui_poll`) | Input | Busiest pass |
|---|---|---|---|---|---|---|
| THE SCENE | 66 | 72.4, 200 | 17.14 ms | 56.0k, 71.6k instructions | 350 (`ui_encoder`) | 73.1k |
| THE OSCILLATOR | 64 | 68.7, 200 | 17.14 ms | 55.1k, 71.3k | 387 (`ui_buttons`) | 72.8k |
| THE CHIP | 66 | 72.4, 200 | 17.14 ms | 55.6k, 71.2k | 364 (`ui_encoder`) | 72.9k |

The busiest frames are the glow's first level: 200 cells, and the scan of the 900 cells for
those that differ. The input and the busiest main-loop pass (`midi_drain` to `midi_service`)
together are 73.4k, within the latency gate's 120k poll allowance (a compile-time tie and a
measured check, as for SCENE). The held frames are checked against their PNGs pixel for
pixel on the host and in fm1-emu (all three, also against the goldens
`tests/golden/lcd/title-chip.png`, `title-scene.png` and `title-oscillator.png`).
