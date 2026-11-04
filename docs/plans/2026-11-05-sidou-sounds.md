<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU's sound bank on the site: design

**Status:** design for 1.7, 2026-11-05.

## Why

A good bank matters to players as much as features; the only widely available SID banks
(Elektron's SidStation factory bank, MIDIbox's) are not ours to redistribute. SIDOU ships its own.

## What the player does

The Library page has a section, **SIDOU's sounds**: sixteen sounds, each with its name and a line
on what it is, a **Send to FM-1** button and a download link. The send goes the site's existing
SysEx path (the store's PUT, an identical song left alone), as one song, `SIDOU SOUNDS`, one
instrument a sound with its `LBnn` chunk; on the FM-1, ENV on INSTR pulls each into LAB exactly,
and SAVE keeps it. One song, not sixteen: the store holds 48 files.

## The sounds

Original, made with SIDOU's own parameters (`host/crates/sidou-patch/src/sounds.rs`), each showing
a 1.6 or 1.7 page: a table kick, snare and laser, a chip chord, an arpeggio lead, a filter-envelope
acid line and pad, LFO strings, wobble, S&H and tremolo, layered saws, a sync lead, a ring bell, a
noise hat and a flute. Each is valid, passes LAB's audibility estimate on both chips, renders above
-40 dBFS on both (sidou-tests' `lab_edit.rs`), and pulls back from the song exactly.

## Files

`assets/sounds/sidou-sounds.sng` and `.json` are written from the definitions by sidou-patch's test
(`SIDOU_SOUNDS_UPDATE=1`, as the MIDI reference is); `tools/web-demo` copies them to the site's
`sounds/`. Firmware: nothing.
