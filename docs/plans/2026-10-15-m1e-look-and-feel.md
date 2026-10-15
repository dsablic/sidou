# M1e: Look and feel, implementation plan

<!-- REUSE-IgnoreStart -->

> **For Claude:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development.

**Goal:** the fun half of SIDOU. Generative, music-reactive PETSCII-grid
visuals and 80s menus on the FM-1's 240×240 LCD, within strict main-loop
and SPI budgets.

**Reference:** the browser prototype
(https://claude.ai/artifact/Ghyhf6bUkXUFrawXbsc3X5). It shows five screens,
five generative pieces and four logo concepts, and measures how many 8×8
tiles change per frame.

Measured averages (out of 900 tiles; a full redraw takes about 77 ms of
SPI):

| Screen or piece | Tiles changed |
|---|---|
| Fire | about 426 |
| Plasma | about 500 |
| Rule 30 | about 20 |
| Stars | about 25 |
| Langton's ant | about 3 |
| LIVE | about 123 |
| ABOUT | about 240 |
| BOOT | about 125 (peaks of 900) |

**Budget rules** (from design doc "Look and feel"):
- **SPI:** at most about 20 ms of SPI per 40 ms frame at steady state.
  Pieces above that drop their frame rate, shrink their area or use
  coarser palettes.
- **Main loop:** CPU per frame is measured in fm1-emu and asserted. It
  never touches the audio budget.
- **Art:** original only. The C64 influence stays at the level of
  inspiration, with no Commodore marks.

**Rules:**
- the same as M1a (SPDX, trailer, stage only your own files, checks by exit
  code);
- every screen gets e2e LCD snapshot tests in fm1-emu;
- every piece gets a "busiest frame" main-loop cost test.

### Task 1: Art foundation
- `docs/art/README.md`: the 16-colour palette with RGB565 values, layout
  grid (30×30 cells), motion rules (25 fps target, SPI budget), the logo
  choice and naming. The logo is one of the prototype's four concepts; pick
  the "Pulse Train" default unless the user decides otherwise, and note it
  as a decision to confirm.
- **Our own 8×8 PETSCII-style font**, replacing `font8x8_basic`: ASCII plus
  block/line/box glyphs drawn in a source format we own (CC0 or
  GPL-3.0-or-later). A generator turns it into a `const` C array in flash.
  The emulator tests compare a rendered glyph sheet against a golden image.
- **A cell-grid renderer:** a 30×30 char+colour buffer and its previous
  frame. Diff it, then send only the changed cells, coalesced into runs to
  cut SPI commands. Pixel overlays (scopes) are drawn into dirty tiles.

### Task 2: UI framework
- Views with focus, PETSCII frames, menus with a blinking block cursor,
  transitions (wipe, raster), and a toast/status line.
- Input routing from the panel layer M1a built.
- **Tests:** host tests of the view state machine; LCD snapshots.

### Task 3: Boot and About
- **Boot:** `LOAD"SIDOU",8,1` → `SEARCHING FOR SIDOU` → `LOADING`
  (raster-stripe border, at most every other frame) → `READY.` → logo.
  Skippable, and short after the first boot.
  - **Amended (Task 3):** short on every boot (the title card alone), and
    HOME held at power-on plays the full sequence. Without storage the
    firmware cannot tell the first boot after an install from the others;
    "full once per install, short after" waits for M2's storage (a flag in
    flash, [TODO](../TODO.md)). The user has been told.
- **About:** a cracktro with raster bars, a starfield, a sine scroller and
  credits read from `CREDITS.md` at build time, within the SPI budget. Trim
  it from the prototype's 240 tiles to about 120.

### Task 4: LIVE view visualisations
- Per-voice oscilloscopes from a decimated tap of the voice outputs
  (written by the audio ISR into a lock-free ring), envelope bars from ENV3,
  the filter curve, a VU meter, and slot indicators.
- Pick the decimation and tap so the audio ISR cost stays below about 10
  instructions per sample. Assert it in the gate.
- LIVE changes every frame (the scopes, the meters), so it composes every
  frame and does not live under the idle bound of
  `screen_changes_and_transitions_stay_within_the_budgets` (an idle status
  screen composes nothing: about 6.6k instructions a frame, bounded at 7.5k).
  It gets its own busiest-frame bound in the gate, expected at about 70-90k
  instructions a frame.
  - **Amended (Task 4):** LIVE is the root view, what the screen shows from
    power-on; the status screen moves to the menu (STATUS), which M2's STEP
    view will join. The tap is a point every 16 samples (each voice's waveform
    output and oscillator phase) and the output's peaks of every 8th frame,
    only while LIVE shows: 4.8 instructions a sample. The scopes are folded by
    the oscillator's phase rather than drawn on a time base, so a held note
    stands still at any pitch. Three voices of noise move about 240 cells a
    frame, over the 20 ms budget: LIVE holds itself to 200 cells a frame and
    the scopes take turns. A held chord costs about 48k instructions a frame,
    three voices of noise about 141k (206k before the canvas was drawn a part
    at a time and the pixels two at a store), which is over the 70-90k
    expected; the reading of the tap runs half way between
    frames, so no main-loop pass grows long enough to delay MIDI (the M1d
    latency gate). docs/art/README.md, "LIVE", has the numbers.

### Task 5: First SCENE pieces (prototype for M2)
- Rule 30, Langton's ant and the starfield: cheap, and reacting to gates,
  ENV3 and cutoff.
- Fire and plasma at a reduced area or frame rate, to fit the budget.
- **Tests:** snapshot tests, and busiest-frame cost tests.
  - **Amended (Task 5):** SCENE is a menu item (after STATUS); SELECT steps between the five
    pieces (FIRE, PLASMA, RULE 30, STARS, ANT), each wiping in with its name as a toast. Each
    piece draws into a canvas of its own and the view copies it onto the grid within 200 cells
    a frame, going on where it stopped the next frame; the pieces step half way between
    frames. FIRE burns in the bottom 14 rows, every other frame, half its rows a frame; PLASMA
    draws every other row a frame at about a fifth of the prototype's top speed, its ripples single
    rings. LIVE's tap runs while SCENE shows, unchanged (the interrupt's code is the same):
    the envelopes come from it, the gates and notes from the instrument. docs/art/README.md,
    "SCENE", has the numbers.

### Task 6: Optional SID UI sounds
- Blips and a "drive chunk" played on chip B's spare voice when it's idle.
  Off by default, and never steals a playing voice.
- **Moved to M2** (2026-10-15): it needs chip B's spare voice, and chip B arrives with M2.
  M1e is complete without it; M2 takes it up with chip B.

<!-- REUSE-IgnoreEnd -->
