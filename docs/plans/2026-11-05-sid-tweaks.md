<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Live tweaks on a playing .sid: design

**Status:** design for 1.7, 2026-11-05. **Parked, not implemented**: the session that built 1.7's other items ran out of room for it; nothing of it is in the firmware. The RAMTEXT it needs exists since 1.7 moved the GoatTracker player to flash.

## What the player does

In NOW PLAYING, while a tune plays (beside GLO's mutes and solos):

| Control | Does |
|---|---|
| K1 | bends the tune's filter cutoff: -1024..+1024 on its 11 bits, 32 a detent (`CUT +96`) |
| K2 | bends its resonance, -15..+15 (`RESO +3`) |
| K3 | picks the voice K4 acts on, 1..3 |
| K4 | that voice's waveform: `TUNE'S` (the tune's own), `TRI`, `SAW`, `PUL`, `NOI`: the tune's gate, sync, ring and test bits stay |

Each turn opens the value popup (name and value large). The screen shows a TWEAK line while any
tweak is on. Back at 0 (or `TUNE'S`) each gives the tune its own value at once; leaving NOW
PLAYING, another song or a stop gives everything back.

## Engine

The tune's register writes reach the chip in the audio interrupt (`psid_isr_chip`, SRAM). A write
goes through one small function there: it keeps the tune's raw value of `$D415..$D417` and the
three controls, and when a tweak is on writes the bent value (the cutoff from both raw bytes plus
the bend, clamped; the resonance nybble plus its bend; the control's waveform nybble replaced). A
new tweak (a sequence number the main loop bumps) rewrites those registers from the raw values at
the next block, so a turn is heard within 2.9 ms even if the tune never writes them again.

## Budgets (estimate)

RAMTEXT about 200 bytes (room since 1.7's move of the GoatTracker player); RAM 60 bytes (the raw
registers of two chips, the tweak state); ISR: 3 instructions a write with no tweak, about 15 with
one; flash about 1 KB (the view, the popups).
