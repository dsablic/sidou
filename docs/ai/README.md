<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU for coding agents

How a coding CLI (Claude Code, Codex, Gemini CLI, Aider, ...) makes a sound, a bank or a STEP
pattern for SIDOU, checks it, and sends it to an FM-1. Everything runs from a clone of this
repository; nothing needs a network.

## The formats

- One LAB sound: `sidou-patch/1`, [docs/reference/patch-json.md](../reference/patch-json.md),
  schema [patch.schema.json](../reference/patch.schema.json).
- Up to sixteen sounds as one song: `sidou-bank/1` (same page), schema
  [bank.schema.json](../reference/bank.schema.json).
- One STEP lane as a song: `sidou-step/1` (same page). Examples: `assets/steps/*.json`.
- What the engine can do, the ranges and how to make SID sounds sound good:
  [docs/prompt-to-patch.md](../prompt-to-patch.md) (the site's `llms.txt`). Read it before writing
  a patch. SIDOU's sixteen sounds as patches: `assets/sounds/patches/*.json`.

## Build the tool once

```sh
cd host && cargo build --release -p sidou-sysex
# the tool: host/target/release/sidou-sysex
```

## Make and check

```sh
sidou-sysex patch acid.json --out out    # out/ACID.sng and out/ACID.syx, a one-instrument song
sidou-sysex bank kit.json --out out      # the bank's name: out/<NAME>.sng and .syx
sidou-sysex step line.json --out out     # a song that opens on STEP
```

Each runs the converter the FM-1's site runs (`platform/web/patch_json.c`). A refusal exits 1 and
prints one line naming the field, what is wrong and what is allowed, for example
`acid.json: filter.resonance: 20 is too high. Allowed: 0 to 15.`: fix that field and run again. A
patch only one chip would hear is written, with a `warning:` line. `--8580` makes the song for an
8580 (the default is the 6581). The `.syx` is the store PUT for any SysEx sender; the `.sng` is the
song for the Library page or `send`.

## Send (macOS)

```sh
sidou-sysex send out/ACID.sng                      # to the MIDI port named "SIDOU FM-1"
sidou-sysex send out/ACID.sng --port "SIDOU FM-1" --name "ACID 2"
```

- **Ask the user before sending to their FM-1, every time.** Make and check freely; sending writes
  to the device's store.
- `send` names the port and the file first, then checks that the port answers as SIDOU (its INQ)
  and sends nothing otherwise. It sends songs (`.sng`), `.sid` tunes and GoatTracker instruments
  (`.ins`) to the store, by their content, and refuses anything else: never a firmware package or
  update frames.
- The stop-and-wait protocol is [docs/reference/sysex.md](../reference/sysex.md)'s: each frame
  waits for its ACK; WAIT (the FM-1 holding a write while a song plays) keeps waiting; BUSY sends
  the whole file again after 1, 2, 4, then 5 s, for about a minute; any other NAK stops with its
  reason in words.
- A file the FM-1 has already, byte for byte, is left alone. One of that name with other content is
  refused unless `--replace` is given (ask the user first).
- On the FM-1 afterwards: SONG, LOAD the song; for a patch or a bank, SEQ to INSTR, an instrument
  and ENV opens it in LAB, and SAVE keeps it as a USER sound; a STEP pattern opens on STEP.
- Elsewhere than macOS, `send` says so: use the Library page (https://dsablic.github.io/sidou/library.html)
  in Chrome or Edge.

## Change an existing sound

A song's LAB sounds and SIDOU's sixteen open as patches on the Library page (Patch, JSON). Keep
every field you were not asked to change, change only what was asked, and keep the name.
