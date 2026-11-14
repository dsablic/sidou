<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU roadmap

Where SIDOU is going, in the order we mean to do it. Each line says what a player gets; the
why lives in [decision records](decisions/) and the how in [plans](plans/). What shipped is
in [CHANGELOG.md](../CHANGELOG.md) and on the [releases page](https://github.com/dsablic/sidou/releases).
Sizes: S is under a day of work, M a few days, L more.

## Next: Bluetooth connecting out

SIDOU connects to Bluetooth MIDI keyboards (the central role, a DEVICES page, the last device
remembered), the owner's Korg microKEY Air as the test; pairing if the keyboard asks for it
([decision 0011](decisions/0011-bluetooth-midi.md)). Its room, about 9-10 KB with its page: the
2,380 bytes left under the ceiling, `-Oz` for the radio driver (880 bytes, untried on the device)
and hot/cold splits of the gate-bound `-O2` files (`sid_character.c`, `live.c`, `live_view.c`,
`sid_dac.c`, `input.c`; up to 13 KB whole-file, each refused by a gate before). If that is not
enough, SCENE's RED MOON RITE art is the reserve (about 45 KB), only with the owner's yes. M-L.

## 1.10

The owner's list of 2026-11-06, each starting with a short design:

- **MIDI controller maps as text:** writing MIDI LEARN's table, once it is checked whether SysEx
  can reach it.
- **Read USER sounds over SysEx:** the Library page lists the sixteen USER sounds and opens one as
  a patch (today SysEx refuses `USER SOUNDS`; LAB's REC into the song is the way round).

## 2.0

- **Chip B on the second core** ([decision 0010](decisions/0010-second-core-reopened.md)): off by
  default, falling back to one core; the RAMTEXT room is there since 1.7.
- **A layer's level in LAB**, after it (the owner, 2026-11-06): a gain per voice in both render
  loops costs about 7 instructions a sample per chip, leaving 7.1 of the two-chip worst block's
  1,426, and 160 bytes of RAMTEXT; chip B on its own core gives that budget back. It also needs a
  place on LAYERS (its four cards are taken), 3 bytes of the sound and the import
  ([the measurement](https://github.com/dsablic/sidou/blob/v1.7.1/docs/plans/2026-11-05-lab-lfos.md)).
- **SIDOU as a DAW plugin**, starting from its design session (the owner, 2026-11-04): a SIDOU
  edition of fm1-emu's FM-1 plugin (AU, VST3, CLAP) running the real firmware, with a large mirror
  of the screen, real knobs for LAB's pages and LIVE, tracker entry from the computer keyboard and
  side panels drawn from the firmware's state. The spike ([its plan](plans/2026-11-05-daw-plugin-spike.md),
  code in `spikes/daw-plugin/`) runs SIDOU's package at 17.6x real time; stable parameters a LAB
  card, GPL-3.0-or-later. A first release M, the whole edition L.

## Open from earlier releases

- **SIDOU's own USB ID** (0x51D0): waits for pid.codes request #1341. The update loader stays
  Felucca's, byte for byte, on 0x1209:0x0002.
- **The first-run tour on a real FM-1**: the one part of the device session not yet checked there.
- **The quality pass**: performance headroom (the thin spots in the audio-interrupt gates: LIVE
  noise, SELECT into RULE, STATUS idle, the MIDI floods, the USB drain), knob-curve follow-ups from
  the device sessions, and the two `-Oz` puzzles (`gt2_song.c`'s goldens, `fx_slots_init.c`'s
  timing). M-L.
- **Small fixes**: a pulled LAB name comes back after a power cycle (today the sound does, the
  name not); the Live Link smoke test's HOME hold no longer depends on wall-clock timing.

## 1.x

**Songs**

- **Combined drum-and-bass instruments** from DICE: a drum's first frames, then the bass's wave
  program, in one instrument (1.5 did the time-shared form). M.
- **More than 48 keys in the store.** Only if players hit STORE FULL (the starter pack uses 11;
  LAB adds one). M-L; changes the store format.

**The site**

- **The editor's rest:** instrument delete and reorder, pattern renumbering, editing a Stereo
  song's voices 4-6, and the preview's output stage (thump, chip B, MASTER)
  ([editor.html](https://dsablic.github.io/sidou/editor.html)). M.
- **Live Link's scopes**, sent as state ([the protocol](reference/live-link.md)). M.

**Quality**

- **Suggested automations.** A one-line toast offers "record this knob move as locks?" after you
  keep doing it. L, idea; cheap in the main loop.
- **Faster test CI.** An optional sharded emu-gate workflow on GitHub's x86 runners (the gate's
  JIT works there), to take the emulator stage off the local machine. S-M.

## Parked

- **The 8580 mixer's grit** (measured, not kept): the spike (branch `mixer-8580`, not merged) puts
  a quadratic on the 8580's output; its second harmonic lands within 0.8 dB of reSIDfp's, but the
  distance to the CSG8580R5 recordings does not move (0.04 dB at most on every tune), for 80 bytes
  of RAMTEXT and 11 instructions a sample. The reSIDfp op-amp port stays parked for good (the
  owner, 2026-11-04).
- **Wi-Fi and Ableton Link** ([decision 0011](decisions/0011-bluetooth-midi.md)): until Optimist
  runs Link on a real FM-1. Bluetooth audio is dropped.

## Later and ideas

- **A second engine on the FM-1's second core** (research): users want SID beside polyphonic
  synthesis without re-flashing. Flash is the limit (2,380 bytes left in the slot), so it needs
  room found first.
- **Faster firmware switching from the site** (research): the Library page installs another
  firmware package and brings SIDOU back, keeping the songs and sounds on the FM-1.
- **Layer mode.** Two sounds on one key. L; audio interrupt.
- **FM-1 Live Link extras:** a tray app, a share link, iOS.
- **RSID playback.** Not now (see below); revisit if there is demand.

## Not doing

- **RSID playback as built.** SIDOU refuses an RSID tune politely
  ([psid.md](reference/psid.md)): it needs a full C64 environment, not just the chips.
- **Combined-waveform rows in POOL.** POOL has no room for them
  ([flash-space plan](https://github.com/dsablic/sidou/blob/v1.6.0/docs/plans/2026-10-24-flash-space.md)).
- **Recording into a DAW with a plugin.** Superseded by USB audio out, which records over the
  cable.
- **2× oversampling and an idle LED glow.** Rejected: cost against a gain nobody asked for.
- **Our own BLE stack.** Not needed: Optimist's open stack has the radio's controller.
