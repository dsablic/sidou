<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# PUNCH: design

Momentary performance effects for 2.0, agreed with the owner on 2026-11-18 (option A: a page whose
note keys are pads; no held-button chords).

- **Getting there:** FX on any song view (STEP, TRACKER, ORDER, INSTR, TABLE) opens PUNCH; HOME
  goes back. The song plays on.
- **The pads:** note keys 1-6 (C to F), lit while held, several at once:
  1. **MUTE**: the song silent, its timing running on.
  2. **FILTER SLAM**: cutoff to the bottom, resonance up; released, back as it was.
  3. **PITCH DIVE**: every note bends down an octave over about half a second (tape stop).
  4. **STUTTER**: the current beat repeats until released, then the song is where it would have
     been.
  5. **ECHO THROW**: the delay's send high; the tail rings on after release.
  6. **CHIP SWAP**: the song on the other SID model (6581 and 8580).
- **The other keys** do nothing on PUNCH; the page says `KEYS ARE PADS`. Stopped, the pads do
  nothing and the page says `PLAY TO PUNCH`.
- **Rules:** every effect is undone exactly at release; nothing is written into the song.
- **Cost:** STUTTER is the player repeating rows (no audio buffer); the others push existing SID
  parameters or the effect slot's send. A few KB of flash. Proven in fm1-emu; the owner tries it on
  the device.
- **Not now:** freeze and reverse (they need an audio buffer the RAM has no room for); a latch per
  pad (option C) may come later.
