<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]
### Added
- Design document, licensing, credits and contribution guidelines.
- SID core in C11 (`core/sid/`): oscillators, noise, hard sync and ring
  modulation, envelope generator with the ADSR delay bug, a
  zero-delay-feedback state-variable filter (its cutoff and resonance fitted
  to reSIDfp's filter since M1b),
  mixer and DC blocker, for the 6581 and the 8580.
- Rust host workspace (`host/`): FFI to the C core, unit and golden-render
  tests, and a criterion benchmark.
- reSIDfp reference harness: pitch and envelope tests, and the
  `sidou-compare` filter comparison tool.
- Pinned JieLi pi32v2 toolchain in Docker, and a bare-metal pi32v2 benchmark
  program.
- Emulator benchmark gate: runs the pi32v2 build in fm1-emu, enforces 900
  instructions per sample per chip, and checks the output against the host
  build.
- CI: host tests on Linux and macOS with UBSan; ASan on Linux; MSRV check, REUSE lint,
  and the firmware build with the emulator gate.
- Building and testing guides, and decision records for the host tooling
  and the M0 SID cost.
- FM-1 platform (`platform/fm1/`): Felucca's HAL (GPL-3.0-only), startup
  code and memory map, and a firmware image that boots, runs the audio
  interrupt and plays a SID test tone; an fm1-emu boot test checks it.
- The firmware plays the live instrument from the FM-1's keys, K1-K4, MASTER,
  OCT-/OCT+ and FX (6581/8580), lights the held keys and the octave, and shows
  a status screen on the LCD. Register writes reach the audio interrupt
  through a ring that never drops; a model change swaps to a chip prepared in
  the main loop. fm1-emu tests play it end to end.
- MIDI in over the TRS jack and USB (Felucca's USB stack, adapted: USB-MIDI
  only): notes on every channel, CC 74/71/1/7, program change; a lock-free
  ring from the 2 kHz polls to the main loop, one parser per source, exact
  recovery from lost input.
- The audio path (the ALNK0 interrupt, the render, the register drain) runs
  from SRAM; tests check that the interrupt runs nowhere else and that no
  call crosses between flash and SRAM code.
- The community SysEx update protocol, step 1 (Felucca's `ota.c`), and
  Felucca's update loader, unmodified and byte-identical to Felucca's own
  build; a boot-loop guard that sends a third early death in a row to UBOOT.
- `sidou-pack`: the `.fwsc` packager (a port of Felucca's), byte-identical to
  Felucca's packer on the same inputs; CI builds `sidou.fwsc`, boots it in
  fm1-emu and uploads it.
- The M1a hardware checklist (`docs/hardware-test.md`), checklist builds that
  die early on purpose while OCT- is held at power-on (`SIDOU_TEST_DEATH`),
  and a package SIDOU refuses (`sidou-pack --test-wrong-size`).
- Post-chip colour chain (`core/fx/`): bit-crush, tape saturation, wow and
  flutter, CRT/TV low-pass with mains hum, and a noise floor, as separate
  blocks with smoothed parameters and an exact bypass; an emulator gate
  scenario with every effect at full.
- Effect slots after the chip mix (`core/fx/fx_slots.h`): Distortion (tape,
  overdrive, fuzz, crush, lo-fi), Modulation (chorus/ensemble, flanger,
  phaser, tremolo, auto-pan, wow and flutter) and FX (delay, ping-pong, a
  light reverb ported from Emilie Gillet's Rings/Clouds reverb (MIT), and
  a Dattorro plate), each with Amount, A, B and Mix, smoothed and with
  crossfaded algorithm switches; stereo output; an emulator gate per
  algorithm and slot.
- The firmware runs the effect slots after the chip with true stereo output.
  Their settings ride the register ring as pseudo-registers; a temporary panel
  layer (EDIT + K1-K4, SCL, ENV) and CC 102-116 control them, and the status
  screen shows them (`docs/reference/controls.md`).
- The art direction (`docs/art/`): palette, 30x30 cell grid, motion rules and
  logo concepts. SIDOU's own 8x8 PETSCII-style font, generated from a glyph
  sheet, replaces the font8x8 placeholder. A cell-grid renderer sends only
  changed cells, in runs; the status screen runs on it, and fm1-emu tests
  compare the screen with golden images and bound what drawing costs.
- A view framework on the cell grid (`app/screen.h`, `app/widget.h`): a
  stack of views with focus, panel events routed to the view on top, PETSCII
  frames, menus with a blinking block cursor, a toast line, and wipe and
  raster transitions inside the SPI budget. HOME opens a menu (chip, preset,
  octave, the glyph sheet); every other control plays as before in every
  view. The overlay pool grows to 255 cells for LIVE's scopes.
- A boot sequence and an About screen (`app/boot.h`, `app/about.h`): the
  logo's title card at power-on, or with HOME held the full
  `LOAD"SIDOU",8,1` sequence with loading stripes; any control skips it
  without its action, and a note skips it and plays. ABOUT in the menu is a
  cracktro with raster bars, stars, a sine scroller and the credits,
  generated from CREDITS.md. The logo is a pluggable asset (`app/logo.h`),
  Pulse Train for now.
- MIDI, M1d: every live, SID and effect-slot parameter in one CC/NRPN map
  (`app/midi_map.c`), with NRPNs at full resolution (11-bit cutoff, 12-bit
  pulse width), running NRPN/RPN state per channel and input, and the
  published chart (`docs/reference/midi.md`) generated from the table, with a
  drift test.
- Expression: velocity (to the sustain level, cutoff or pulse width, per
  preset), pitch bend (+-2 semitones by default, RPN 0 or NRPN 6/0 up to
  +-24), the modulation wheel and channel and polyphonic aftertouch (to
  vibrato depth, pulse width or cutoff), and a per-voice vibrato with rate,
  depth and delay.
- MIDI out over USB: the panel's parameter changes as CCs (or NRPNs), K4 as
  a program change, rate-limited, deduplicated and never echoing MIDI input.
- A note-on-to-sound latency test over emulated TRS MIDI, and decision record
  0005 (notes stay at block starts).
- ENVELOPE: INSTANT / AUTHENTIC (CC 26, NRPN 1/10). The SID core has a gate
  reset option, `sid_set_gate_reset` (off by default, so the core stays
  bit-exact with reSIDfp), that restarts the envelope's rate counter at each
  gate edge and skips the ADSR delay, for the attack at a note-on and the
  release at a note-off. The live instrument turns it on by default: INSTANT.
  AUTHENTIC is the chip's timing.
- MIDI learn: hold SAVE, turn a knob (EDIT held for the effect slots), send a
  CC, and the knob's parameter follows that CC on every channel and both ports,
  besides its own; HOME or 10 s cancel, SAVE instead of the CC forgets one,
  SAVE three times clears all. A learned CC wins over the map's, panel changes
  go out on it, and a learned CC from MIDI is never echoed. In RAM until M2's
  storage; `docs/reference/midi.md` documents it from the code.
