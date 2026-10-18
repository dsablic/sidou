<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Recipe schema

A **recipe** is a how-to that the emulator can run: a goal, a starting state, and steps, each
with the physical control, a caption and an outcome the harness checks. The same file is the
human doc's source and the agent-readable form, and it is verified on every build, so it cannot
go stale. Recipes are journeys of the ergonomics harness (`host/crates/ergo`, TESTING.md "The
ergonomics harness") in machine form: authored in `host/crates/ergo/tables/*.txt`, one JSON
file each in [`docs/recipes/`](../recipes/).

```
cd host
cargo run --release -p ergo -- --export-recipes ../docs/recipes   # tables/recipes.txt -> JSON
cargo run --release -p ergo -- --check-recipes ../docs/recipes    # runs each in fm1-emu; exit 1 on a failure
```

## File: `sidou-recipe/1`

```json
{
  "schema": "sidou-recipe/1",
  "id": "shape-a-sound",
  "title": "Shape a sound with K1 to K4",
  "goal": "one line",
  "prerequisites": {"start": "power-on | live", "text": "what must be true before step 1"},
  "skills": ["K1 cutoff", "..."],
  "related": ["first-sound", "..."],
  "steps": [ { "...": "see below" } ]
}
```

- `id` is the file name without `.json`, lower case with dashes, and is stable: other recipes
  and the docs link to it.
- `prerequisites.start`: `power-on` (time 0, the boot running) or `live` (booted, on LIVE with
  the first preset). Anything more is said in `text`.
- `related` ids must exist (the lint checks).

### Step

```json
{
  "id": "close-the-filter",
  "title": "Close the filter",
  "caption": "Turn K1 counter-clockwise about a third of a turn: the sound darkens.",
  "control": "Turn K1 counter-clockwise about a third of a turn",
  "inputs": [{"op": "turn", "encoder": "K1", "detents": -60}, {"op": "wait", "ms": 400}],
  "expect": [{"kind": "audio", "assert": "darker"}, {"kind": "param", "name": "cutoff", "min": 0, "max": 1100}],
  "needs": [],
  "harness": {"clip_ms": null, "probe": false, "task": null, "milestone": null,
              "destructive": false, "remember": false, "defaults": false}
}
```

- `caption` (required): the manual's text and the video subtitle.
- `control`: the physical control in panel terms. Made from `inputs` when the recipe's source
  gives none.
- `inputs`: what the emulator does, in order. `button` is OCT-, OCT+, FX, SCL, ENV, LFO, EDIT,
  GLO, HOME, SAVE, ARP, SEQ, PLAY or REC; `encoder` is SELECT, ALGORITHM, K1 to K4; notes are
  F3 to G5 (C4 is middle C). Ops:

  | op | fields | does |
  |---|---|---|
  | `press` | button | pressed and let go |
  | `hold` | button, ms | held |
  | `turn` | encoder, detents | turned (negative: counter-clockwise) |
  | `hold_turn` | button, encoder, detents | a button held while an encoder turns |
  | `hold_key` | button, note | a button held while a note key is pressed (GLO + key) |
  | `note` | notes[], ms | keys held together, then let go |
  | `down`, `up` | note | a key held across steps, then let go |
  | `select_item` | text | the menu's selection moved onto the line showing the text |
  | `midi` | bytes[] | bytes into the TRS jack |
  | `pot` | value (0-1023) | the MASTER pot |
  | `clock` | bpm, ms, start | MIDI clock |
  | `wait` | ms | nothing happens |
  | `boot`, `power_cycle` | | wait out the boot; switch off and on over the same flash |

- `expect` (at least one): what must hold when the step is done; the harness waits up to about
  3.5 s for it. Screen:
  `{"kind": "view", "value": "tracker"}` (the firmware's view name), `{"kind": "text", "value": "M2"}`
  and `text_absent` (case-insensitive, anywhere on the screen), `{"kind": "param", "name": "cutoff",
  "min": 0, "max": 1100}` (a live parameter: cutoff 0-2047, resonance 0-15, pulse_width 0-4095,
  master 0-1023, preset, octave, profile, age, dac, thump, bleed, `dist.algorithm`, `mod.amount`, ...),
  `popup`, `toast`, `screen_changes`, `recalled`. Audio:
  `{"kind": "audio", "assert": "sounding | silent | brighter | darker | louder | quieter"}`
  (`brighter`/`darker` compare the spectral centroid, `louder`/`quieter` the RMS, of the last
  400 ms against the 400 ms before the step: they need a sound before the step, 5% and 1 dB
  by default) and `{"kind": "audio_level", "min_dbfs": -45, "max_dbfs": -8}`.
- `needs`: a feature the step depends on (`theme undo help order_view instr_view pattern_edit
  new_song song_save`). A journey records a step whose feature is missing as "not yet possible";
  a recipe **fails** on it: a recipe only teaches what the synth does.
- `harness`: what the ergonomics checks use (`task`: the common-task name for the effort count,
  `destructive`, `clip_ms`, ...); a recipe may leave them out.

## The validator

`--check-recipes DIR` checks each file against this schema (a field path in the message), runs
the steps in order in fm1-emu, and fails a recipe whose step cannot run or whose expectations do
not hold, naming the step. `tests/recipes.rs` also checks, without the emulator, that every
file in `docs/recipes/` parses, passes the lint (unique step ids, a caption and an outcome on
every step, a goal, skills, related ids that exist) and is what `--export-recipes` writes from
`tables/recipes.txt`.
