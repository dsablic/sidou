<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The videos, polished: phase 1

**Status:** phase 1, 2026-11-16, branch `video-polish` from `main` `60064fa0`. One video rendered
before and after for the owner to judge; nothing re-rendered or published yet. The design is in
[design-notes.md](../design-notes.md) ("How-to videos' pacing and motion").

## The video measured

**whats-new** (wide, 1920 x 1080): captions, SCENE (DEMO, SCOPE, RED MOON RITE), LAB, cards and
the BLUETOOTH menus. first-sound and shape-in-lab were measured too, but neither has a screen
zoom (no recipe has: every region is the full 30 columns wide, so the zoom formula gives 1.0) and
their LCD frames never tore; whats-new shows the most of what the owner saw.

`build/polish/before.mp4` comes from main's tooling, `build/polish/after.mp4` from this branch.
Figures from `tools/videos/measure.py`; the raw ones are `build/polish/before.json` and
`after.json`.

## Before and after

| | before | after |
|---|---|---|
| Length (whole video) | 58.6 s | 90.0 s |
| Frame rate | 25 fps | 60 fps (LCD at its own 25) |
| Steps shown shorter than their caption's reading time (180 wpm + 1 s) | 6 of 9 | 0 of 9 |
| Shortest time after a step's last action | 45 ms | 1500 ms |
| Caption shown before the action | 0 ms | 700 ms |
| Camera moves / reversals within 1 s | 0 / 0 (nothing zooms) | 0 / 0 |
| Pointer jumps (moves under 400 ms) | 22 | 0 |
| Per-step raster wipes, sparkles | 9, on every region step | 0 |
| LCD ticks showing a half-drawn screen | 100 of 2167 (4.6%; SCENE and the BLUETOOTH menus) | 0 (held back; longest hold 450 ms) |
| Loudness, true peak (the MP4) | -16.0 LUFS, -2.0 dBTP | -16.0 LUFS, -1.9 dBTP |
| Segment joins | 5 ms fades | 300 ms crossfades |

Each step, time on screen against its caption's reading time (ms):

| Step | read | before | after |
|---|---|---|---|
| keyboard | 8667 | 9011 | 9731 |
| pair (card) | 12333 | 5640 | 12333 |
| demo | 7000 | 9040 | 11202 |
| scope | 8000 | 5760 | 8000 |
| duel | 13667 | 5000 | 13666 |
| art (card) | 11333 | 5040 | 11333 |
| glow | 6333 | 4440 | 6601 |
| wheel | 8000 | 8080 | 9807 |
| get-it (card) | 3667 | 3269 | 4007 |

The half-drawn count for "before" is from the same capture: the ticks where the LCD the old
sampler took differed from the last fully drawn one.

## What changed

- **ergo's full capture:** the caption shows 700 ms before the step's first action; the step
  runs on until its caption is read and 1.5 s past its last action; only fully drawn LCD frames
  are recorded. Tests: `cargo test --release -p ergo --test capture`.
- **The renderer:** 60 fps; a camera that moves only on a real region change (900 ms, quintic
  ease, a second's hold, pixel-exact zoom steps); the pointer and the label glide; the caption
  in a fixed lower third with 200 ms fades, one at a time; the headline crossfades; the panel's
  dimming no longer flashes each step; no per-step wipes or sparkles; a soft shadow under the
  LCD's bezel; 300 ms crossfades between the parts; a render fails off the loudness target.
  Tests: `python -m unittest test_videos` in `tools/videos`.

## Open for the owner

- **Length:** whats-new grows from 59 s to 90 s at 180 words a minute; the captions, written as
  a sidecar, are long for a picture. Shorter captions (or 200 wpm) would bring it back to about a
  minute.
- **The caption in the picture:** it was a sidecar only; it is now in the lower third and still
  in the `.srt`.
- **Phase 2:** re-render all 22 in the three cuts, refresh the durations in docs/videos.md, the
  previews and the release assets; check the square and vertical layouts (moved to make room for
  the caption, not yet looked at).
