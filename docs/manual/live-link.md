<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The Live Link page: the FM-1's screen in a browser

The site's [Live Link](https://dsablic.github.io/sidou/link.html) page shows the FM-1's screen
live, as large as your window allows, and puts its buttons and encoders under your mouse or
finger: for a screen you can read from across the room, a projector or a stream, or playing the
FM-1 from the computer. The picture is drawn by SIDOU's own screen code, the same C the FM-1
runs, compiled to WebAssembly, so it is the LCD pixel for pixel.

It talks to the FM-1 over the USB cable with FM-1 Live Link, MIDI SysEx frames on the USB-MIDI
port the FM-1 already has ([docs/reference/live-link.md](../reference/live-link.md)). That needs
**Chrome or Edge** (Web MIDI with SysEx; Safari has none, and Firefox is not supported), and
SIDOU 1.5 or later on the FM-1. Nothing is installed and nothing leaves your computer.

Run locally, `tools/web-demo` serves it at `http://localhost:8642/link.html`
([BUILDING.md](../../BUILDING.md#browser-demo-toolsweb-demo)).

## 1. Connect

Plug in the FM-1 with its USB cable, running SIDOU (not in the updater), and press **Connect**.
The browser asks to use your MIDI devices with SysEx: allow it. The page picks the ports named
`SIDOU FM-1`; if it finds none, pick the FM-1's output and input yourself. Connect sends nothing:
the page sends nothing to any port before Start, and then only to the output shown.

## 2. Start

Press **Start**. The FM-1 answers with its name and version, which the line under the buttons
shows with the frames and bytes a second coming in, and sends its whole screen. While the
session runs the FM-1 shows a small `LINK` in the bottom-left corner of its own screen (the page
shows the screen without it).

A screen at rest sends nothing; a view that moves (the scopes on LIVE, TRACKER playing, SCENE)
sends what changed, at up to about 22 KB a second, so the page may show fewer frames a second
than the LCD but always the newest picture. **Redraw** asks for the whole screen again, should it
ever look wrong; the page also asks by itself when a frame arrives damaged.

**Stop** ends the session, and the FM-1's `LINK` goes. The session also ends by itself:

- when you close the tab, reload it or switch to another tab or window (the page sends Stop; it
  starts again by itself when you come back to the tab);
- 3 s after the FM-1 last heard from the page (the computer asleep, the cable out, a busy
  browser): the page says the FM-1 ended the session, and **Restart** starts it again;
- when the cable is pulled, at once on the FM-1's side.

If the FM-1 does not answer at all, the page says so after 5 s: check that it runs SIDOU 1.5 or
later and that the right ports are picked.

## 3. The panel

The buttons are the FM-1's, by the names SIDOU's manual uses ([controls](../reference/controls.md)):
OCT- and OCT+ beside SELECT and ALGORITHM, K1-K4 and the big PRESETS encoder on the other side,
FX, SCL, ENV, LFO, EDIT, GLO, HOME, SAVE, ARP, SEQ, PLAY and REC under the screen (on a phone
everything is stacked under the screen). They act as the FM-1's own:

- **A button** is held for as long as you hold it (mouse, finger, or Space or Enter on a selected
  button), so the combinations work: EDIT held with an encoder, HOME held for help, SAVE pressed
  and released. Letting go anywhere, or leaving the window, lets it go.
- **An encoder** turns by dragging it up (clockwise) or down, a detent every 10 pixels; with the
  mouse wheel over it; with its **−** and **+**, a detent a tap; or with the arrow keys once it is
  selected (Page Up and Page Down: five detents).

The FM-1 lets go of the page's buttons half a second after it last heard about them, so a
button can never stay stuck if the page goes away while you hold it.

**The keys** under the panel are an octave from C4 to C5. They are not part of Live Link: they
play MIDI notes on channel 1 to the FM-1, as a keyboard plugged into the computer would (SIDOU
plays notes from every channel unless its MIDI page says otherwise).

## Notes

- The font and the colours are built into the page. If a firmware's differ, the page fetches them
  from the FM-1 once and keeps them in the browser, so the next start needs no fetch.
- Live Link and the [Library](library.md) page use the same port: use one of them at a time. The
  FM-1 holds Live Link's frames back while a file transfer runs, so nothing breaks, but the
  picture pauses.
- Live Link is written for any FM-1 firmware to implement; this page draws a screen of 30 x 30
  cells of 8 x 8 pixels, as SIDOU's, and says so when a firmware offers another.
