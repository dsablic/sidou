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

| Drum | AD, SR row | Wavetable, a row a frame (notes absolute; SR the `F6` row, holding the row before) | Measured (tests/kit.rs) |
|---|---|---|---|
| KICK | 19, 09 | triangle C-4, F-2, SR, D-2, C-2, B-1, A#1 held | 262 Hz falling to 58 Hz; the band below 120 Hz is -3.1 dB of it on the 6581, -3.5 dB on the 8580 |
| SNARE | 16, 06 | pulse D-4, noise F#7, SR, A#6, E-6 held | |
| CL HAT | 12, 02 | noise B-7, SR | -24 dB in 40 ms on both models |
| OP HAT | 18, 08 | noise G#7, SR | -24 dB in 220 ms (6581), 200 ms (8580) |
| CLAP | 15, 05 | noise C-7, SR, silence, noise, silence, noise A#6 | three bursts (the first two frames), a dip of 20 dB or more between them |
| TOM LO | 18, 08 | triangle A-3, F#3, SR, E-3, D-3, C#3 held | 221 Hz falling to 139 Hz |
| TOM HI | 17, 07 | triangle E-4, C#4, SR, B-3, A-3, G#3 held | 330 Hz falling to 208 Hz: a fifth above |
| RIM | 12, 01 | pulse A#5, noise E-7, SR | |

Every instrument's SR is `00`; its SR row (`F6`, the wavetable's command 6 with the SR byte)
sets sustain 0 and the release. The measurements above are the kit as of 2026-10-24 (see
"The ADSR delay bug at the gate" below: the first measurements were of drums that sounded
33 ms late, their first gated frame lost).

Choices, each measured on both models by rendering through the song transport:

- **First frame `$08`** (the test bit, gate off) for every drum. The test bit resets the
  oscillator and the noise register, so every hit is the same; with the gate off the envelope
  starts whole at the wavetable's first row. GoatTracker's default first wave, `$09`, gates
  on in that silent frame: a short drum's envelope had decayed half its length before it
  sounded (the closed hat peaked at -37 dB, against -24 dB now), and on the chip the voice's
  DC rises with the envelope in that frame, a thump 20 ms before the drum (in reSIDfp, for
  the closed hat on the 8580, -7 dB of the hat's first frame). Every drum so sounds one frame
  after its row, and its sound does not depend on the row's note.
- **The kick's body is a triangle near 58 Hz** reached by frame 7; a frame of triangle at C-4
  first gives the click (a pulse there, heard once the drums sounded on time, took the low
  band to -4.9 dB of the kick on the 6581). Its share of energy under 120 Hz is the same on both models (the test
  holds it within 2 dB), so it has weight on the 8580 as on the 6581.
- **The clap's gaps are the inaudible waveform `$E1`** (gate held, no waveform), not a gate off
  and on: switching from release back to attack hits the ADSR delay bug (the rate counter
  wraps, about 33 ms), and the bursts merged into one.
- **Attack 1 (8 ms to the top), SR `00` until the SR row**: see the next section.
- **No filter**: the filter is the chip's, one for the three voices, and a drum that set it
  would change the song's lead.

### The ADSR delay bug at the gate (2026-10-24)

The first-use emulator tests (emu-gate `first_use.rs`,
`the_kits_drums_sound_one_frame_after_their_row`) found every chip drum sounding about 53 ms
after its row, not 20: a flam against a lead note on the same row, KICK's click frame lost.
It is the instruments, not SIDOU's player or chip: GoatTracker 2.77's own packed player
(gt2-ref) driving reSIDfp gives the same ENV3, cycle for cycle, as SIDOU's chip, on both
models (`tests/kit.rs`, `every_new_instrument_starts_on_time`). Two ways the envelope's rate
counter (a 15-bit LFSR, compared for equality with the rate's period) ran past the attack's
period, so the attack waited for it to wrap, 32,767 cycles (33 ms):

1. **The release in the gate-off first frame.** The hard restart's `$0F00` (release 0, period
   9) leaves the counter below 9; the note's first frame then writes the drum's own SR with the
   gate off, and the counter ran at the release's period (up to 977 cycles at release 9). At the
   gate, attack 0's period 9 was behind it nearly always: 20 + 33 ms. Fix: every SR is `00`
   (the counter stays below 9) and a wavetable SR row sets the release in frame 2 or 3, before
   STEP's staccato key off at tempo 5 and up (the key off comes the gate-off timer's two frames
   before the next row, frame 4 at tempo 6), so a staccato drum fades as a held one
   (`a_staccato_drum_sounds_as_a_held_one`). At tempo 4 and below a staccato drum other than the
   hats and the clap releases fast from frame 2. The SR row's frame holds the row before it,
   which the table above shows.
2. **The decay's rate for a cycle at the gate.** As the gate goes on, the chip (reSIDfp's state
   pipeline, from sampling ENV3) compares the counter with the decay's period for one cycle; if
   the counter sits at attack 0's 8 then, it steps to 9 and the attack waits out the wrap: one
   phase of the counter in nine, 33 ms late, whatever the instrument before. GoatTracker's usual
   attack-0 instruments share it. NEW's lead (AD `09`) hit it in the first-use tests once a fix
   to the auditions' release (`14711ace`) moved the phase: PULSE 33 ms after its row. Fix: the
   attack is 1 (period 32, 8 ms to the top), which a counter below 10 always reaches: the kit's
   drums and NEW's lead and bass (`19` and `12`). The test checks nine frame lengths, every
   phase.

The SID model keeps both behaviours (they are reSIDfp's and the chip's). Songs saved from NEW
before keep the old bytes. The kit is 45 wavetable rows (was 38).

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
- **The map**: the main loop turns the chunk into an instrument-to-sample table when a play,
  a continue or an audition starts and when the extension area's size changes (not every pass:
  a MIDI flood makes passes many, and the main-loop budgets in emu-gate's midi.rs hold), writing
  only the bytes that change; a song with no digi drum skips the event reading; 256 entries, so any instrument byte indexes it with no
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
