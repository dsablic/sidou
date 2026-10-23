<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0009: The SID drum kit and digi drums

**Status:** accepted, 2026-10-23 (the user approved both parts and the digis' place in the file).

<!-- REUSE-IgnoreStart -->

## Context

The roadmap's "From the other SID firmwares" item (docs/plans/2026-10-18-roadmap-after-m2.md):
drums on the chip's own voices for STEP's lanes, and short samples played through the 6581's
volume register as C64 tunes do. Melodee's SID KIT and SID DIGI and SID-1's sample kit were
the inspiration; none of their code or data is used (the "adopt, then improve" rule): the
kit is GoatTracker 2 instruments tuned here by measurement, and the samples are synthesized
here by a generator in the repository.

## The kit

Eight GoatTracker 2 instruments and their table rows (`core/song/gt2_kit.c`): plain GT2, so a
song that uses them opens and plays in GoatTracker unchanged
(`host/crates/sidou-tests/tests/gt2_match.rs`, `the_drum_kit_matches`, plays a beat of all
eight against GoatTracker's packed player, register for register, at 1x and 2x).

| Drum | AD SR | Wavetable, a row a frame (notes absolute) | Measured (tests/kit.rs) |
|---|---|---|---|
| KICK | 09 09 | pulse C-4, triangle C-3, F-2, D-2, C-2, B-1, A#1 held | 99 Hz falling to 58 Hz; the band below 120 Hz is -2.4 dB of it on the 6581, -2.7 dB on the 8580 |
| SNARE | 06 06 | pulse D-4, noise F#7, A#6, E-6 held | |
| CL HAT | 02 02 | noise B-7 | -24 dB in 40 ms (6581), 30 ms (8580) |
| OP HAT | 08 08 | noise G#7 | -24 dB in 200 ms |
| CLAP | 05 05 | noise C-7, silence, noise, silence, noise A#6 | three bursts, a dip of 10 dB or more between them |
| TOM LO | 08 08 | triangle A-3, F#3, E-3, D-3, C#3 held | 167 Hz falling to 139 Hz |
| TOM HI | 07 07 | triangle E-4, C#4, B-3, A-3, G#3 held | 251 Hz falling to 208 Hz: a fifth above |
| RIM | 02 01 | pulse A#5, noise E-7 | |

Choices, each measured on both models by rendering through the song transport:

- **First frame `$08`** (the test bit, gate off) for every drum. The test bit resets the
  oscillator and the noise register, so every hit is the same; with the gate off the envelope
  starts whole at the wavetable's first row. GoatTracker's default first wave, `$09`, gates
  on in that silent frame: a short drum's envelope had decayed half its length before it
  sounded (the closed hat peaked at -37 dB, against -24 dB now). Every drum so sounds one
  frame after its row, and its sound does not depend on the row's note.
- **The kick's body is a triangle near 58 Hz** reached in five frames; a pulse frame first
  gives the click. Its share of energy under 120 Hz is the same on both models (the test
  holds it within 2 dB), so it has weight on the 8580 as on the 6581.
- **The clap's gaps are the inaudible waveform `$E1`** (gate held, no waveform), not a gate off
  and on: switching from release back to attack hits the ADSR delay bug (the rate counter
  wraps, about 33 ms), and the bursts merged into one.
- **No filter**: the filter is the chip's, one for the three voices, and a drum that set it
  would change the song's lead.

**How STEP gets them: NEW's song holds the kit.** A NEW song is a pulse lead (01), a bass (02),
then the eight drums (03-0A), the kit's table rows first (its pointers are rows from 1). STEP's
PRESETS (and TRACKER's) popup now names the instrument (`03 KICK`, `05 CL HAT`), so a lane's
drum is picked by name with the knob a lane already uses. Considered and left for later: an
"add the kit" action for an existing song. It needs free instrument slots and table rows
moved into a song that may use them, through the stopped-model instrument import
(`gt2_ins_import`, today SysEx's); NEW and the names give the kit to STEP with no new control.

## Digi drums

### In the file

A SIDOU chunk, **`DIGI`**, in the trailer GoatTracker skips (decision record 0007), marks
instruments as digi drums, each naming a built-in sample:

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | version, 1 |
| 1 | 1 | k, the entries (at most 16) |
| 2 | 2 × k | per entry: an instrument (1-63) and a built-in sample: 1 KICK, 2 SNARE, 3 CLAP, 4 HAT |

The instrument's own GT2 content is the **fallback** GoatTracker plays (and SIDOU mutes on its
voice while the sample plays): in a NEW song the digi instruments 0B-0E (`DG KICK`, `DG SNARE`,
`DG CLAP`, `DG HAT`) are copies of the matching chip-kit drums. Position-free (kept when the GT2
part's CRC fails); an entry with an instrument out of range or an unknown sample is ignored, the
chunk kept as written. Built-in samples only: no upload in this version. A NEW load adds the
chunk (`app/song_io.c`); the template itself stays a plain GTS5 stream.

### The samples

Four, synthesized by `sidou-tablegen` (`host/crates/sidou-tablegen/src/digi.rs`, the recipe in
its comments; checked in as `core/player/gen/digi.h`, the drift test keeps them equal). Simple
synthesis from a fixed-seed noise generator, sines and exponential envelopes, no recorded
material: SIDOU's own work, GPL-3.0-or-later (LICENSING.md).

- 4-bit codes, the values a C64 player writes to `$D418`'s low nibble, rest at 8, **two a
  byte** since v0.4.0 (first a code a byte; packing them freed 2,430 bytes of the RAM region
  once RAMTEXT had room for the nibble's choice: [flash and RAMTEXT
  space](../plans/2026-10-24-flash-space.md)).
- One code every 5 output samples: 44,118 / 5 = 8,824 Hz.
- KICK 220 ms (1,942 codes), SNARE 117 ms (1,034), CLAP 185 ms (1,631), HAT 29 ms (253), each
  cut where it falls to the rest: 4,860 codes, 2,431 bytes after a pad code (index 0 means
  none to the interrupt), in flash and copied to RAM at boot (the audio interrupt reads no
  flash).

### Playback, and its cost

A C64 plays a volume-register digi by writing each code to `$D418`; what is heard is the
6581's mixer DC times the volume. SIDOU's chip model carries no such DC (it is the
character's thump, off by default), and writing the chip's volume 8,824 times a second would
split each block's render some 23 times and scale the song's voices with it. So SIDOU plays
what the 6581's volume DAC makes of the codes: `(code - 8) × SONG_DIGI_LEVEL` (875, so a full
code is about a chip kick's peak), held for the code's 5 samples, added to the block after the
tail gate (so a digi in a quiet song is not muted with the chips' leak), before the effect
slots, clamped to int16. The chip's volume register stays the song's. One digi plays at a time
(a C64 has one volume register): a new hit cuts the last.

- **Trigger and mix** (audio interrupt, `song_ctl_block_end`): when the block held a call (its
  sample, `call_sample`, is in the block), each of that call's notes (a row, a jam: STEP's
  audition too) whose instrument the chunk marks starts its sample at the call's own sample;
  the samples before it are a silent code's. A voice that is muted, soloed out or the keys'
  plays none (`digi_ok`, the main loop's). Then the codes are added.
- **The instrument's own sound** (main loop, `song_ctl_poll`): a second reader of the event
  ring follows each voice's instrument, and the voices whose instrument is a digi drum join the
  mutes the interrupt is sent (`SONG_CMD_MUTE`, with the user's: a mute's gate off, then none
  of the song's writes, and the registers back at the next note of another instrument). A digi
  instrument's first frame is silent with the kit's first wave `$08` (and GoatTracker's default
  `$09`), and the mute lands within a block or two, before its second frame; a note of another
  instrument right after a digi drum on that voice so starts a few milliseconds late. Done in
  the interrupt this would cost about 130 more bytes of SRAM (the sink's, the rewrite's and the
  mutes' tests, and the note's): there are none.
- **The map**: the main loop turns the chunk into an instrument-to-sample table each pass,
  writing only the bytes that change; 256 entries, so any instrument byte indexes it with no
  test in the interrupt (the reader refuses instruments past 63, and those entries stay 0).
- **Cost**: 10.42 instructions a sample while a digi plays without a break (emu-gate
  `a_playing_digi_drum_costs_the_interrupt_its_bound`), one test a block while none plays.
  With the codes two a byte (v0.4.0) the nibble's choice costs about 1.2 more, which the loop
  wins back by holding each code's level up to a pointer to the next code's sample (no count a
  sample) and clamping with min and max: 9.47, bound `DIGI_PER_SAMPLE` 9.5. The audio target's
  sum (two chips at their worst, the slots' cap, the tap, the ISR, the player's worst block and
  a digi) is 2,474 of 2,700. The system
  stack's deepest use is unchanged (3,272 bytes of 7,936 in the emulator tests that play one).
- **SRAM**: the interrupt's code grew by 176 bytes (`song_ctl_block_end` from 38 to 208, the
  rest alignment), exactly what RAMTEXT had free. **RAMTEXT is now full** (49,152 of 49,152):
  the next audio-path code needs room made first (decision for the user: e.g. building the
  slots' switch path for size, or a smaller debug area). v0.4.0 gave RAMTEXT that debug area
  (56 KB); packing the codes took 32 more bytes of it.

The host's render (`host/crates/sidou-tests` Transport, emu-gate's `render_host_song`) calls
`song_ctl_block_end` with its block as the device does, and the device plays a song with the
kit and a digi drum sample for sample as the host renders it
(`the_drum_kit_and_a_digi_drum_play_bit_exact_with_the_host`).

### On an 8580

A real 8580's mixer has almost no DC, so volume-register digis are nearly silent on it (the
"digi boost" modification, a resistor on EXT IN, puts it back). SIDOU plays the digi at the
6581's level whatever chip A's model: as a 6581, or an 8580 with the digi boost. The chip kit
is the part of a song that sounds like the chip model.

## Numbers

Measured on the fm1 build (`FM-1_900`), against main at `fd45e64d`:

| | Before | After the kit | After the digis |
|---|---|---|---|
| Image (sidou_app.bin) | 485,720 | 486,168 (+448) | 491,908 (+5,740: 4,860 the samples) |
| Left under IMAGE_CEILING (506,000) | 20,280 | 19,832 | 14,092 |
| RAMTEXT (.ram_text) | 48,976 of 49,152 | 48,976 | **49,152 of 49,152** |
| RAM region free | 11,784 | 11,784 | 6,632 (the samples 4,860, the state) |
| POOL free | 12,160 | 12,160 | 12,160 |
| Free RAM together (FREE_RAM_MIN 10,000) | 23,944 | 23,944 | 18,792 |

<!-- REUSE-IgnoreEnd -->
