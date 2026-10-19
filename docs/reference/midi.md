<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Generated from app/midi_map.c by host/crates/sidou-tests/tests/app/midi_map.rs; do not edit. SIDOU_MIDI_DOC_UPDATE=1 regenerates it. -->
# MIDI: the CC and NRPN map

Every parameter of the live instrument has an NRPN (14-bit, full resolution). All except
the bend range, three expression targets and expert layer also have a fixed CC (7-bit).
MIDI learn can bind a CC to any expert knob. The map uses the same numbers on every channel, on TRS and USB. Notes
play on every channel (omni) by default; MIDI's mode (Modes, below) can give the instrument
one channel and chip A's voices channels of their own. Program change 0..3 selects the
preset (in bank 0, the power-on bank: Program change and banks, below). A program change
loads the preset's cutoff, resonance, pulse width, SID settings (waveform, ring, sync,
the envelope, filter routing and mode) and expression settings (bend range, the targets,
the vibrato) over any edits, even when it selects the preset already playing; volume, the
chip model and character, the envelope mode, the effect slots, the controllers (bend, wheel, aftertouch) and
a bend range set by RPN 0 are kept.

**How the numbers were chosen.** CCs that MIDI gives a meaning are kept for it:
General MIDI's where they fit (1 modulation wheel: the pulse width by default, see
Expression; 7 volume; 71 resonance; 74 cutoff, i.e. brightness; 72, 73 and 75 release,
attack and decay time; 76, 77 and 78 vibrato rate, depth and delay) and left alone where
they do not (0 and 32 bank select, 6 and 38 data entry, 64..69 the pedals, 88
high-resolution velocity, 96..101 increment, decrement, NRPN and RPN, 120..127 channel
mode). The rest is in blocks MIDI leaves undefined: the SID in 20..26 and 79, the
character layer in 85..87 and 89..90, the effect slots in 102..116. NRPNs are grouped by
MSB: 0 the performance controls, 1 the SID, 2 the character layer, 3..5 the effect slots,
6 expression.

CC 71..78 are absolute here, like every CC of the map: the value sets the parameter
across its range. General MIDI 2 makes its sound controllers relative instead, moving the
patch's value up or down around 64; SIDOU does not.

## Receiving values

- **CC, linear curve:** 0..127 spans the whole range, value * max / 127 rounded, so 0 and
  127 are the ends.
- **CC, steps curve:** 0..127 is divided evenly among the parameter's values,
  value * (max + 1) / 128: a switch is off below 64 and on from 64.
- **NRPN:** CC 99 and 98 select the parameter (MSB, LSB). The data is the parameter's own
  value, MSB * 128 + LSB, clamped to its range: CC 6 sets the MSB (the LSB taken as 0),
  CC 38 the LSB (with the MSB of the last CC 6, or, if none came since the selection or
  the last step, the current value's). CC 96 and 97 step the value by one (their data byte
  is ignored).
- **NRPN, parameters up to 0..127** (the 0..15 envelope rates, sustain, resonance and
  waveform, the switches and modes, the effect slots): the value is all in the LSB, with
  an MSB of 0, e.g. 6 = 0, 38 = 9 for 9. CC 6 only stores the MSB and CC 38 applies the
  value, so a sender's 6 = 0, 38 = v goes straight to v and never through 0 (which would
  reset the chip for the chip model, or silence the waveform). A controller that sends
  CC 6 alone ("7-bit NRPN") changes nothing here: use CC 38, or the parameter's plain
  CC.
- **Null and RPN:** NRPN 127/127 and RPN 127/127 select nothing. An RPN (CC 101, 100)
  takes data entry away from the NRPN until CC 99 or 98 comes again. RPN 0/0 (pitch-bend
  sensitivity) sets the bend range: CC 6 the semitones, at once (0..24), CC 96 and 97 a
  semitone up or down; CC 38 (the cents) is ignored. Other RPNs do nothing. CC 121 (reset
  all controllers) and a gap in the input select nothing.
- **Bend range, RPN 0 and NRPN 6/0:** RPN 0 is the channel's setting, as RP-015 has it, so
  program changes keep it until power-off: a sequencer may send it once, before its
  program change. NRPN 6/0 is the preset's, like every NRPN: a program change replaces it
  with the new preset's. Of the two the last one received wins, and NRPN 6/0 ends RPN 0's
  hold: after it, the next program change loads the preset's range again. Neither CC 121
  nor a gap in the input changes the range.
- **Chip model:** a change resets the chip, so it waits for the next 10 ms tick and takes
  the last value received. The new chip is prepared in the background a slice a tick, 70 to
  110 ms (the first 8580 after power-on about 150 ms), while the old one plays on, and swaps
  in when it is whole: a stream of changes never holds the instrument up, and the last one
  received is the chip you get. A stream faster than that (a change every 70 ms or less)
  restarts the preparation each time, so the chip swaps only once it stops. The FX button
  asks for a change the same way.
- **Two chips:** everything here (notes, CCs, NRPNs, expression, learn) plays the
  instrument, so the chip the keys play: chip A, or chip B while a song plays on chip A (the
  KEYS setting AUTO; with KEYS A, chip A always). The chip-level CCs (25 model, 85 profile,
  86-90 the character) set the keys' setup, which both chips hold: the other chip follows it
  in the background. On chip A's own channels (Modes, below) they set chip A's setup.
- **Character:** the profile (CC 85) is a chip and a model: a change resets the chip as a
  model change does, and the model follows it. The model (CC 25, the FX button) selects
  the 8580 R5 profile, or goes back to the last 6581 profile. Age/heat, DAC sloppiness,
  output thump and filter bleed change the playing chip without a reset; they too go from
  the next tick, the last value received, and glide there so a jump does not click (age
  and thump 0 to 127 in 0.32 s, the bleed in 1.27 s, the DACs in 1.72 s). The profiles, their sources and
   what each control does: docs/decisions/0006-chip-profiles.md.
- **Expert character (NRPN 2/5..18):** fourteen independent controls, each a raw value
  0..127 mapped to the physical units in the table. Send CC 99 = 2, CC 98 = the LSB,
  CC 6 = 0, then CC 38 = the value. They never reset the chip or interrupt a held note;
  table builds run in slices and their stages swap at an audio block boundary.
  A macro move releases only its related overrides (AGE: curves/range/non-linearity;
  DAC: kink/leakage; THUMP and BLEED: their namesakes). Other experts stay edited.
  Selecting a built-in profile/model releases all overrides; USER profiles recall theirs.
  Outgoing MIDI sends the model/profile/macros before explicit expert overrides, and does
  not emit derived expert NRPNs for a macro move: those would freeze the receiving macro's path.
  USER schema 2 and settings schema 5 keep the override state, not a quantized copy of defaults.
  The 6581/8580 curves apply to their named model; input bias, non-linearity and the
  external low-pass are 6581-only. Inapplicable knobs are grey and say N/A on EXPERT and
  in the value popup. Combined strength's AVERAGE band is labelled MED on the panel.
- **Envelope mode:** 0 INSTANT (the default) restarts each voice's envelope rate counter at
  each gate edge, so a note's attack, and its release at the note-off, take their first
  step within one rate period and 3 chip cycles (12 cycles at attack 0). 1 AUTHENTIC is
  the chip's ADSR delay bug, as reSIDfp has it: after a slower rate (a release before the
  attack, a decay before the release), most notes and note-offs wait up to 33 ms. A change
  counts from the next gate edge, so a sounding note's release follows it.
  [controls.md](controls.md) has the why.
- The selection is kept per channel and per input (TRS, USB).
- CC 120 and 123 release every note.

## Expression

The expression settings are per preset (NRPN group 6, the vibrato also on CC 76..78); the
controllers (bend, wheel, aftertouch) are one set for every channel, like the notes.

- **Pitch bend:** all 14 bits, +-the bend range (2 semitones on every preset; RPN 0 or
  NRPN 6/0 change it, see above). At +-2 one bend step is 0.024 cents. The SID's frequency register
  is coarser than that at every pitch (one step is 0.39 cents at C4, 1.6 cents at C2), so
  the bend moves the pitch one register step at a time, with no larger jumps. The
  frequency between the notes is computed with integers from generated tables, within 0.6
  of a register step of the exact value. Every sounding voice bends, released ones too, and
  a note starts at the current bend. The sync lead's two oscillators take the same bend
  and vibrato, so their ratio, and the timbre, holds, up to the top of voice 1's register
  (from note 90, or lower bent up: [controls.md](controls.md), Presets).
- **Control rate:** bend, aftertouch, the wheel (on a target other than the pulse width)
  and the vibrato apply at the instrument's 100 Hz tick: up to 10 ms after the message,
  twice a C64 player's frame rate. A flood of messages costs one update per tick.
- **Vibrato:** a triangle per voice, from 0 at each note-on once its delay is over. Rate
  CC 76 (0.1 Hz steps, up to 12.7 Hz), depth CC 77 (127 is +-99 cents), delay CC 78
  (10 ms steps). The wheel and aftertouch add to the depth where they are assigned to the
  vibrato, up to 127. Updated at 100 Hz, a triangle of +-d cents at f Hz moves 4 * d * f /
  100 cents a tick: 6 cents at +-25 cents and 6 Hz, 24 cents at the full depth. Above
  about 8 Hz a cycle has fewer than 12 steps and the vibrato starts to sound stepped.
- **Modulation wheel (CC 1):** by default it sets the pulse width across its range, as
  M1a did and the map's row says. The mod wheel target (NRPN 6/2) moves it to the vibrato
  depth (it adds) or to the cutoff (it opens the filter above the cutoff that is set, 8
  steps per value). A new target starts the wheel at 0.
- **Aftertouch:** channel and polyphonic. A voice's pressure is the larger of the
  channel's and its note's, and a note-on starts its own at 0. The target (NRPN 6/3): off
  (the default), the vibrato depth (it adds), the pulse width (it raises the voice's, 16
  steps per value) or the cutoff (it opens the filter 8 steps per value of the most
  pressed voice: the SID has one filter).
- **Velocity:** the SID has no amplifier per voice, so velocity sets one of three things,
  per preset (NRPN 6/1): off (the default, full level), the sustain level (scaled by
  velocity / 127, written before the gate; the attack still peaks at the full level), the
  cutoff (8 steps lower per velocity below 127, by the last note: one filter) or the pulse
  width (the voice's pulse narrowed by velocity / 127; heard on the pulse waveform only).
  127 always sounds as off does. The panel's keys play velocity 127, so they sound as off
  does whatever the target.
- **Reset:** CC 121 centres the bend and sets the wheel and aftertouch to 0 (the pulse
  width is a parameter, not the wheel's position, and is kept). A gap in the input does
  the same, so a lost return to the centre cannot leave the pitch bent.

## Output

SIDOU sends its parameter changes over USB (the USB-MIDI IN endpoint, cable 0), so a
DAW can record the panel as automation and play it back. Nothing goes out of the TRS
jack, which is MIDI IN only.

- **What:** a parameter that changes on the panel (the knobs, MASTER, the FX button,
  the effect-slot controls) goes out as the map's CC: the value as the CC curve gives it,
  round(value * 127 / max). A preset from the panel (PRESETS) goes out as a program change, which carries the
  preset's factory settings; the edits PRESETS brings back with it (SIDOU keeps each preset's
  edits while another plays) go after it as the CCs of the parameters that differ.
- **High resolution:** an option, off by default, sends the parameters of more than
  128 values (cutoff, pulse width, volume) as their NRPNs instead, at full resolution.
  A parameter with no CC (the bend range, the expression targets) always goes as its
  NRPN, and so does the pulse width while the mod wheel drives another target (CC 1 is
  then the wheel). A change of the mod target goes before the tick's other parameters
  (after a program change), so CC 1 is never read with the old target. An NRPN is 4 CCs, without running status: 99 and 98 its
  number, 6 and 38 the value's MSB and LSB (value / 128, value % 128), which is the
  order the input above takes; no null NRPN follows.
- **Channel:** 1 by default. Until M2 stores settings, the channel and the
  high-resolution option are build options (CMake `SIDOU_MIDI_OUT_CHANNEL`, 1..16, and
  `SIDOU_MIDI_OUT_NRPN`). The input follows MIDI's mode (Modes, below).
- **No echo:** a MIDI-only change, from either port, is never sent back:
  the sender has it, and a DAW recording with MIDI thru on would otherwise loop. If
  MIDI changes a parameter after the panel did, and before the panel's change went out,
  MIDI's wins and nothing is sent. So TRS input is not passed on to USB: SIDOU is no
  MIDI thru, and only the panel's changes go out.
  If an earlier local profile/macro is still pending when a remote expert edit arrives,
  that expert is restored after the local message so the local release cannot erase it.
- **Rate:** at the instrument's 100 Hz tick, at most one message per parameter (its
  latest value) and 16 packets in all a tick. A value that sends as the one last
  sent (or received), such as a pulse-width move within one CC step, sends nothing.
  What does not fit waits for the next tick, still as one latest value: nothing queues
  up. With no host (no cable, or the computer asleep) the changes are dropped, so a host
  that comes later gets none stale.
  A cold expert snapshot or mixed macro/expert restoration uses half the packet limit
  for that tick, keeping input handling inside the same instruction budget. Later ticks
  continue with the latest values; no obsolete intermediate edits are queued.
- **Limit:** a parameter the preset loads (not the volume, the chip model, the effect
  slots or a bend range set by RPN 0) that the panel also changes in the same tick as the
  preset counts as loaded by it and is not sent.

## MIDI learn

Any knob's parameter can also follow a CC of your choice: hold SAVE and turn the knob (EDIT held
for the effect-slot layer), then send the CC ([controls.md](controls.md), MIDI learn, has
the gesture, how to cancel and how to clear). What is learned:

- **Which CCs:** 0..119, except 0, 6, 32, 38, 96, 97, 98, 99, 100 and 101, which carry bank select (0 and 32, see
  Program change and banks) and NRPN and RPN data entry and selection; 120..127 are
  channel mode messages. A CC from either port (TRS or USB) and on
  any channel: like the map, a learned CC is omni.
- **Added, and it wins:** the parameter's own CC and NRPN still work. If the learned CC was
  another parameter's, it now moves the learned parameter only; the other keeps its NRPN,
  and gets its CC back when the binding is forgotten. CC 1 learned for a parameter no longer
  moves the modulation wheel.
- **One each way:** a parameter has at most one learned CC and a CC at most one parameter;
  learning again moves the binding. The gesture reaches the 19 parameters on the knobs
  (the cutoff, resonance and pulse width, CHIP's Age, DAC, Thump and Bleed, and each
  effect slot's Amount, A, B and Mix), so
  there are at most 19 bindings.
- **Values:** through the parameter's curve and range in the map below, as its own CC.
- **Output:** a panel change to a learned parameter goes out on the learned CC, so a DAW
  records what the controller sends and plays it back onto the same parameter. A parameter
  whose own CC another parameter learned goes out as its NRPN. A learned CC from MIDI is
  never echoed, as any other.
- **Kept:** across program changes and power cycles, saved with the settings five seconds
  after the last change, or at once with SAVE. An armed learn with no step for 10 s is
  cancelled.

## Modes

MIDI's mode says which channel plays what. It is set on the MIDI page (HOME, MIDI:
[controls.md](controls.md)) and saved with a song in its `MIDI` chunk
([song-format.md](song-format.md)); the page's own setting stays in RAM until the settings
persist. N is the page's CHANNEL, 1..16, and N + 1.. wrap round from 16 to 1.

| Mode | What plays |
|---|---|
| LIVE | channel N plays the instrument, on the chip the keys play; the other channels are ignored |
| MULTI | channels N, N+1 and N+2 play chip A's voices 1, 2 and 3, each a jam voice of the player with its own GoatTracker instrument, pitch bend and notes (one at a time, the last held sounding; LEGATO glides between them); N+3 plays the instrument on chip B |
| WHOLE A | channel N plays chip A's three voices with one GoatTracker instrument: POLY (three voices, the oldest note stolen), CHORD (one key plays the stored chord) or ARP (the held notes cycled on voice 1, every few frames or every 16th of the clock); N+3 plays the instrument on chip B |
| WHOLE B | channel N plays the instrument on chip B; chip A is the song's |
| OMNI | every channel plays the instrument, on the chip the keys play (the default, M1's) |

- **Chip A is the player's** in MULTI and both WHOLE modes, with or without a song: the
  instrument (the panel's keys, and its MIDI channel) plays chip B, with the keys' setup, as
  while a song plays.
- **Chip A's channels:** notes 12..107 (GoatTracker's C-0..B-7), without velocity (its
  instruments have none). Program change n (0..62) picks GoatTracker instrument n + 1
  for the channel's next note. Pitch bend bends the channel's voice by the channel's own
  range, 2 semitones until RPN 0 sets it, at the 100 Hz tick. CC 74 (cutoff) and 71
  (resonance) lock chip A's filter at the value, over the song's, until CC 121 or a change
  of mode. CC 25, 26, 85, 86, 87, 89 and 90 set chip A's model, envelope mode, profile and
  character. CC 120 and 123 release the channel's voices; CC 121 also centres its bend.
  Nothing else (the map, MIDI learn) reaches them: those are the instrument's channel's.
- **A jam note takes its voice from the song** while it sounds (the song's writes to it
  are held back) and gives it back at its release.
- **The options:** LEGATO (MULTI, CHORD) moves to the next note without a new attack while
  one is held. CHORD plays the note and its two intervals (CHORD 2 and CHORD 3 on the page,
  +4 and +7 by default: a major triad). ARP cycles the held notes, in the order they
  were pressed, on voice 1, a step every ARP RATE frames (3 by default), or every 16th of
  the MIDI clock (6 clocks) with the rate at CLOCK.
- **Not yet on the device:** chip A's channels are routed and tracked, and sound once song
  control (M2c) takes jam releases, voices and bends.

## Program change and banks

CC 0 (bank select MSB) and CC 32 (LSB) choose a channel's bank, MSB * 128 + LSB, and keep it
until they change: bank 1 is CC 0 = 0, CC 32 = 1. A program change then means what the
channel's bank says. Bank 0, the power-on bank, is M1's meaning on the instrument's channel.

| Bank | Program n selects |
|---|---|
| 0 | the channel's sound: on the instrument's channel its preset 0..3 (M1's); on a chip A channel (MULTI, WHOLE A) GoatTracker instrument n + 1, 0..62 |
| 1 | songs: program n loads the store's n-th song in name order, stopped; on the instrument's channel or channel N only |
| 2 | the chip's profile: 0 6581 R2, 1 6581 R3, 2 6581 R4AR, 3 8580 R5, 4 worn 6581, 5..12 USER 1..8; the instrument's (the keys' setup) on its channel, chip A's on chip A's |

- Any other bank, or a program past a bank's range, does nothing. The bank is kept per
  channel for both ports together, and across a gap in the input.
- **Songs:** a load takes a few passes; a second request before it is done replaces the
  first. Until the device keeps songs (M2c), the request waits and nothing loads.
- **USER profiles:** until the device keeps them (M2a), the request waits and nothing
  changes.
- **Out:** a preset from the panel goes out as its program change in bank 0, with CC 0 and 32
  first when the last bank sent was another (and then the parameters of its kept edits); a song loaded from the SONG menu goes out as
  CC 0 = 0, CC 32 = 1 and its program.

## Clock and transport

SIDOU's song follows a sequencer's MIDI clock, or leads one. The MIDI page's CLOCK (AUTO,
FOLLOW, OFF; AUTO at power-on) and CLOCK OUT (OFF at power-on) set it
([controls.md](controls.md)); both are kept in RAM until the settings are saved.

- **In** (TRS and USB, merged): timing clock (F8), start (FA), continue (FB), stop (FC),
  song position (F2) and song select (F3). With CLOCK at AUTO or FOLLOW and a clock arriving,
  the song's frame clock follows the clock: its rows land on the clock's 16ths, at the
  song's rows a beat (ROWS/BEAT, 4 by default: a row a 16th). The tempo comes from a fit
  through the last 96 clocks, known after 24; each row's error goes into the next
  beat's period (at most 1 % of a beat), and an error over 2.0 ms re-aligns at once.
- **Start** plays the song from its start on the next clock; **continue** goes on on the
  next clock from where it stopped, or from a song position; **stop** stops it, its
  releases ringing as the chip's would. **Song position** (in 16ths: rows = 16ths x rows a
  beat / 4) is taken while the song is stopped or not started, and sought in the
  background; a continue that comes during the seek starts when it is done, as far on as
  the clock went meanwhile. A song position while the song plays is ignored. **Song
  select** picks the subtune the next start plays.
- **The delay:** each row sounds 8.0 ms after its clock reached the jack: the block
  being heard, the next one (already rendered), and the poll. It is a constant, for a
  sequencer's output latency compensation.
- **A clock that stops** (300 ms without one): with AUTO the song carries on at the last
  tempo; with FOLLOW it stops. CLOCK OFF ignores clock and transport: the song runs on its
  own frames, and start and stop come from the panel only.
- **Out** (USB only: the TRS jack is MIDI IN only), with CLOCK OUT on and no clock coming
  in: 24 clocks a beat at the song's tempo (a beat is its calls a row x rows a beat), each
  sent in the first pass after its time, within a pass and USB's 1 ms frame; start (FA) when
  the song starts, stop (FC) when it stops, and song position then continue (F2, FB) when it
  goes on from where it stopped. A clock that comes in is never sent back: while one is
  followed, nothing goes out.

## SysEx

[SysEx protocol, version 1](sysex.md) specifies framing, packing, payload fields,
CRC-32, refusals and transfer pacing. SIDOU frames begin `F0 7D 53 44` (`SD`);
they are separate from the firmware updater. Universal device inquiry to id 0
or broadcast `7F` has an identity reply on USB only.

| Byte | Command | Function |
|---|---|---|
| `01` | INQ | Protocol/firmware version, store format, free bytes, clean pool and window flag |
| `02` | DIR | Stream directory name, type, length and CRC; omit reserved temporary imports |
| `03` | GET | Download a named file, current-song GTI5 instrument or current GTS5 song with trailer |
| `04` | PUT | Declare target, type, length and CRC before uploading |
| `05` | DATA | 1–256 decoded file bytes; nonfinal uploads must be multiples of 16 |
| `06` | END | Finish a transfer or directory listing |
| `07` | DEL | Delete a named store file; USB ACK follows commit |
| `08` | ABORT | Cancel the owning port's transfer at any sequence |
| `7E` | ACK | Acknowledge one frame's sequence |
| `7F` | NAK | Refusal reason and current clean-pool sector count |

- **USB:** stop-and-wait in both directions. PUT claims all sectors before its first
  ACK; DATA ACKs wait for programming. ACK every GET/DIR response, including END.
- **TRS:** PUT/DATA/END/ABORT only, without replies. Wait **one second after EACH
  complete frame**, including metadata, DATA and file END; wire rate alone is not safe.
  After instrument/current-song END, allow 15 seconds for the bounded loader/import.
- **Storage:** active transfers block maintenance erases. With windows disabled,
  explicit PUT/DEL programs use SAVE-style audio mute/ramp holds, released between
  programs. Window-enabled programs are hold-free; hardware timings still require
  checklist section 12. Disabled windows alone do not cause BUSY.
- **Imports:** instrument tables need contiguous trailing rows; holes are not reused.
  Current-song END ACK follows committed storage and completed, stopped loading;
  validation failures return NAK on USB and a refusal toast. Staging files do not
  appear in SONG or consume bank-1 program numbers. SAVE AS names an imported song.
- **Host tooling:** `sidou-sysex` is a packet/packing library only: no CLI, no device
  access. USB-MIDI output remains a separate hardware blocker (checklist 8.1);
  host/emulator checks are not hardware acceptance.

## The map

In NRPN order.

| Group | Parameter | CC | NRPN (MSB/LSB) | Range | Curve | Default | Values |
|---|---|---|---|---|---|---|---|
| Live | Cutoff | 74 | 0/0 (0) | 0..2047 | linear | 1200 | 0..2047, the SID's 11 bits |
| Live | Resonance | 71 | 0/1 (1) | 0..15 | steps | 6 | 0..15 |
| Live | Pulse width | 1 | 0/2 (2) | 0..4095 | linear | 2048 | 0..4095, the SID's 12 bits; the PWM presets move around it; CC 1 while it is the mod wheel's target (the default) |
| Live | Volume | 7 | 0/3 (3) | 0..1023 | linear | 1023 | 0..1023 as the MASTER pot, the final output's digital gain |
| SID | Waveform | 20 | 1/0 (128) | 0..15 | steps | 2 | sum of 1 triangle, 2 saw, 4 pulse, 8 noise; 0 silent |
| SID | Ring modulation | 21 | 1/1 (129) | 0..1 | steps | 0 | 0 off, 1 on (by the voice before) |
| SID | Sync | 22 | 1/2 (130) | 0..1 | steps | 0 | 0 off, 1 on (to the voice before) |
| SID | Filter mode | 23 | 1/3 (131) | 0..7 | steps | 1 | sum of 1 low-pass, 2 band-pass, 4 high-pass |
| SID | Filter routing | 24 | 1/4 (132) | 0..7 | steps | 7 | sum of 1 voice 1, 2 voice 2, 4 voice 3 |
| SID | Chip model | 25 | 1/5 (133) | 0..1 | steps | 0 | 0 6581, 1 8580 |
| SID | Attack | 73 | 1/6 (134) | 0..15 | steps | 0 | 0..15, the SID's rates |
| SID | Decay | 75 | 1/7 (135) | 0..15 | steps | 9 | 0..15, the SID's rates |
| SID | Sustain | 79 | 1/8 (136) | 0..15 | steps | 10 | 0..15, the level |
| SID | Release | 72 | 1/9 (137) | 0..15 | steps | 6 | 0..15, the SID's rates |
| SID | Envelope mode | 26 | 1/10 (138) | 0..1 | steps | 0 | 0 instant (the rate counter restarts at each gate edge), 1 authentic (the chip's ADSR delay: an attack or release up to 33 ms late after a slower rate) |
| Character | Profile | 85 | 2/0 (256) | 0..12 | steps | 2 | 0 6581 R2, 1 6581 R3, 2 6581 R4AR (reSIDfp's default), 3 8580 R5, 4 worn 6581 (artistic), 5..12 USER 1..8 (an empty slot does nothing: it is skipped); sets the chip model too (a chip reset) |
| Character | Age/heat | 86 | 2/1 (257) | 0..127 | linear | 0 | 0 the profile .. 127: the filter's cutoff drifts darker, the 6581's non-linearity doubles |
| Character | DAC sloppiness | 87 | 2/2 (258) | 0..127 | linear | 0 | 0 the profile .. 127: the voice DACs leak up to 6 times as much and kink more |
| Character | Output thump | 89 | 2/3 (259) | 0..127 | linear | 0 | 0 none .. 127: a volume change steps the output (the 6581's operating point) |
| Character | Filter bleed | 90 | 2/4 (260) | 0..127 | linear | 0 | 0 none .. 127: half the unfiltered voices also reach the filter |
| Expert | 6581 filter curve | - | 2/5 (261) | 0..127 | linear | 64 | 0..127 = reSIDfp curve 0..1 |
| Expert | 6581 filter range | - | 2/6 (262) | 0..127 | linear | 62 | 0..127 = reSIDfp range 0..1 |
| Expert | 8580 filter curve | - | 2/7 (263) | 0..127 | linear | 64 | 0..127 = reSIDfp curve 0..1 |
| Expert | 6581 non-linearity | - | 2/8 (264) | 0..127 | linear | 64 | 0..127 = amount 0..2 |
| Expert | 6581 filter input DC bias | - | 2/9 (265) | 0..127 | linear | 64 | 0..127 = 0..2.3125 voice-DAC units |
| Expert | Filter bleed | - | 2/10 (266) | 0..127 | linear | 0 | 0..127 = 0..50 percent of unfiltered voices |
| Expert | DAC ladder kink | - | 2/11 (267) | 0..127 | linear | 64 | 0..127 = 2R/R ratio 1.8..2.6 |
| Expert | DAC leakage | - | 2/12 (268) | 0..127 | linear | 6 | 0..127 = leakage 0..0.15 |
| Expert | Combined-waveform strength | - | 2/13 (269) | 0..127 | linear | 64 | 0..31 WEAK, 32..95 AVERAGE, 96..127 STRONG |
| Expert | Noise write-back | - | 2/14 (270) | 0..127 | linear | 127 | 0 off; 1..127 progressively connect 1..8 write-back taps; 127 authentic |
| Expert | Volume thump | - | 2/15 (271) | 0..127 | linear | 0 | 0..127 = 0..1 voice-DAC units at volume 15 |
| Expert | 6581 external low-pass | - | 2/16 (272) | 0..127 | linear | 81 | 0..127 = 1000..20000 Hz |
| Expert | External high-pass | - | 2/17 (273) | 0..127 | linear | 20 | 0..127 = 0..100 Hz |
| Expert | Voice DC offset | - | 2/18 (274) | 0..127 | linear | 60 | 0..127 = 0..0.6 voice-DAC units |
| Distortion | Algorithm | 102 | 3/0 (384) | 0..5 | steps | 0 | 0 OFF, 1 TAPE, 2 OVERDRIVE, 3 FUZZ, 4 CRUSH, 5 LO-FI |
| Distortion | Amount | 103 | 3/1 (385) | 0..127 | linear | 0 | 0..127 |
| Distortion | A | 104 | 3/2 (386) | 0..127 | linear | 0 | 0..127 |
| Distortion | B | 105 | 3/3 (387) | 0..127 | linear | 0 | 0..127 |
| Distortion | Mix | 106 | 3/4 (388) | 0..127 | linear | 0 | 0 dry .. 127 wet |
| Modulation | Algorithm | 107 | 4/0 (512) | 0..6 | steps | 0 | 0 OFF, 1 CHORUS, 2 FLANGER, 3 PHASER, 4 TREMOLO, 5 AUTO-PAN, 6 WOW |
| Modulation | Amount | 108 | 4/1 (513) | 0..127 | linear | 0 | 0..127 |
| Modulation | A | 109 | 4/2 (514) | 0..127 | linear | 0 | 0..127 |
| Modulation | B | 110 | 4/3 (515) | 0..127 | linear | 0 | 0..127 |
| Modulation | Mix | 111 | 4/4 (516) | 0..127 | linear | 0 | 0 dry .. 127 wet |
| FX | Algorithm | 112 | 5/0 (640) | 0..4 | steps | 0 | 0 OFF, 1 DELAY, 2 PING-PONG, 3 REVERB, 4 LUSH |
| FX | Amount | 113 | 5/1 (641) | 0..127 | linear | 0 | 0..127 |
| FX | A | 114 | 5/2 (642) | 0..127 | linear | 0 | 0..127 |
| FX | B | 115 | 5/3 (643) | 0..127 | linear | 0 | 0..127 |
| FX | Mix | 116 | 5/4 (644) | 0..127 | linear | 0 | 0 dry .. 127 wet |
| Expression | Bend range | - | 6/0 (768) | 0..24 | steps | 2 | semitones each way; also RPN 0 (CC 101/100 = 0/0, then CC 6 the semitones) |
| Expression | Velocity target | - | 6/1 (769) | 0..3 | steps | 0 | 0 off (full level), 1 sustain level, 2 cutoff (lower), 3 pulse width (narrower) |
| Expression | Mod wheel target | - | 6/2 (770) | 0..2 | steps | 0 | 0 pulse width (CC 1 sets it), 1 vibrato depth (adds), 2 cutoff (opens) |
| Expression | Aftertouch target | - | 6/3 (771) | 0..3 | steps | 0 | 0 off, 1 vibrato depth (adds), 2 pulse width (raises), 3 cutoff (opens) |
| Expression | Vibrato rate | 76 | 6/4 (772) | 0..127 | linear | 60 | 0..12.7 Hz in 0.1 Hz steps |
| Expression | Vibrato depth | 77 | 6/5 (773) | 0..127 | linear | 0 | 0 off .. 127 +-99 cents, a triangle |
| Expression | Vibrato delay | 78 | 6/6 (774) | 0..127 | linear | 0 | 0..1.27 s after each note-on, in 10 ms |

## By CC

| CC | Parameter |
|---|---|
| 1 | Live Pulse width |
| 7 | Live Volume |
| 20 | SID Waveform |
| 21 | SID Ring modulation |
| 22 | SID Sync |
| 23 | SID Filter mode |
| 24 | SID Filter routing |
| 25 | SID Chip model |
| 26 | SID Envelope mode |
| 71 | Live Resonance |
| 72 | SID Release |
| 73 | SID Attack |
| 74 | Live Cutoff |
| 75 | SID Decay |
| 76 | Expression Vibrato rate |
| 77 | Expression Vibrato depth |
| 78 | Expression Vibrato delay |
| 79 | SID Sustain |
| 85 | Character Profile |
| 86 | Character Age/heat |
| 87 | Character DAC sloppiness |
| 89 | Character Output thump |
| 90 | Character Filter bleed |
| 102 | Distortion Algorithm |
| 103 | Distortion Amount |
| 104 | Distortion A |
| 105 | Distortion B |
| 106 | Distortion Mix |
| 107 | Modulation Algorithm |
| 108 | Modulation Amount |
| 109 | Modulation A |
| 110 | Modulation B |
| 111 | Modulation Mix |
| 112 | FX Algorithm |
| 113 | FX Amount |
| 114 | FX A |
| 115 | FX B |
| 116 | FX Mix |
