<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# 0005: MIDI note timing: notes stay at block starts

- **Status:** accepted
- **Date:** 2026-10-15

## Context

M1d Task 5 asks two things:
- a measured note-on-to-sound latency, bounded by one audio block (2.9 ms)
  plus the 2 kHz UART poll;
- sample-accurate note placement inside the block, using the timestamps M1a
  records, if it fits the budget. Otherwise, this record says why not.

**The pipeline today:**
1. The TRS jack's UART writes each byte into a 128-byte DMA ring.
2. The TIMER5 interrupt polls that ring at 2 kHz and pushes the bytes into
   the MIDI ring (`app/midi_ring.h`). Each entry is stamped with `fm1_ticks()`
   at the start of the poll.
3. The main loop's `midi_drain` parses them onto `live`. A note-on writes
   the voice's registers into the register ring at once: notes are not held
   for the 10 ms tick, as pitch bend, aftertouch and the wheel are
   (`start_voice` in `app/live.c`). The writes are then committed.
4. The audio interrupt applies every committed write at the start of its
   block and renders the block's 128 samples into the free half of the DMA
   buffer. That half plays once the other half has finished.

**How it was measured:** `host/crates/emu-gate/tests/latency.rs`, in fm1-emu
at `9baf604`, with `SIDOU_LATENCY_TRIALS=64`: 64 trials per case (CI runs
fewer; TESTING.md has the command). Each trial boots afresh, past the boot's
title card and its wipe, so the instrument is idle as a player finds it. It
injects a note-on (C4, SAW LEAD) at a random cycle, so its phase against the
block, the poll, the 10 ms tick and the screen's 40 ms compose is random.
Once the note holds its sustain level (140 ms later), a note-off goes in at
a random cycle in a block. Times run from the message's last byte in the
UART; its three bytes take 0.96 ms on the wire before that.
- **Taken:** the audio interrupt that applies the note is entered (the
  voice's envelope gate is set in that block).
- **Buffer:** that interrupt returns, with the note's first non-silent
  sample in the DMA buffer.
- **Audible:** that sample leaves the DAC.
- **Off taken:** the audio interrupt that clears the gate is entered.
- **Release in buffer:** the block with the release's first step returns.
  It is read from the voice's envelope counter in the rendered chip, a
  block at a time, as one step down is one level in 170 of the audio.

"Non-silent" means a deviation from the 20 ms before the note-on, by more
than the baseline's own largest deviation plus 8 LSB. A nonzero test would
fail on the DC level and on the waveforms that voices leak with their gate
off. The note's envelope is measured separately: a deviation past -30 dB of
the held note's peak.

The envelope mode (amendment below) is INSTANT by default; AUTHENTIC, the
chip's timing, is set by CC 26 after the boot, and the test checks that it
reached the rendered chip.

## Measured

Milliseconds from the message's last byte: min / median / mean / max.

| Case | Envelope | Taken | Buffer | Audible | Envelope audible |
|---|---|---|---|---|---|
| Idle (status screen) | INSTANT | 0.17 / 1.66 / 1.57 / 3.28 | 0.40 / 1.89 / 1.79 / 3.50 | 3.10 / 4.59 / 4.49 / 6.20 | 3.19 / 4.68 / 4.58 / 6.29 |
| Busy (K1-K3 turning, the screen redrawing) | INSTANT | 0.07 / 1.43 / 1.60 / 3.28 | 0.30 / 1.66 / 1.83 / 3.50 | 3.00 / 4.36 / 4.52 / 6.20 | 3.07 / 4.42 / 4.59 / 6.27 |
| USB retry (the note-on around `usb_attach_now`) | INSTANT | 0.11 / 1.46 / 1.44 / 2.91 | 0.33 / 1.68 / 1.67 / 3.13 | 3.03 / 4.38 / 4.37 / 5.83 | 3.12 / 4.47 / 4.46 / 5.92 |
| Idle (status screen) | AUTHENTIC | 0.17 / 1.53 / 1.53 / 3.28 | 0.36 / 1.72 / 1.73 / 3.50 | 3.57 / 4.93 / 4.89 / 6.68 | 3.86 / 37.7 / 35.3 / 39.9 |

| Case | Envelope | Off taken | Release in buffer |
|---|---|---|---|
| Idle | INSTANT | 0.13 / 1.64 / 1.66 / 3.24 | 0.34 / 1.84 / 1.86 / 3.44 |
| Busy | INSTANT | 0.13 / 1.71 / 1.73 / 3.05 | 0.34 / 1.91 / 1.93 / 3.25 |
| USB retry | INSTANT | 0.17 / 1.47 / 1.60 / 3.07 | 0.38 / 1.68 / 1.80 / 3.28 |
| Idle | AUTHENTIC | 0.14 / 1.87 / 1.72 / 3.25 | 0.64 / 33.9 / 31.3 / 35.4 |

**What the numbers show:**
- **The pipeline.** In every case, the block that takes the note, or the
  note-off, starts within one block and one poll (3.40 ms) of its arrival.
  The test asserts this for every trial.
- **The render.** The block's samples are in the buffer when the interrupt
  returns, 0.20 to 0.23 ms after it is taken, because `audio_fill` converts
  them at its end. So the plan's bound on the buffer, read literally, is
  missed by up to that render: the idle maximum is 3.50 ms against 3.40 ms.
  The bound leaves out the render (amendment below).
- **Audible.** One more block after the buffer: 3.0 to 6.7 ms.
- **The screen.** The busy screen changes nothing measurable:
  - the longest `ui_poll` is 28.4k instructions (about 0.12 ms);
  - the longest wait in the MIDI ring is 194 us idle and 189 us busy.

  The audio interrupt's render (0.2 ms), which holds off the main loop,
  matters as much as the screen.
- **The USB retry.** While USB is off, `usb_retry`'s attach stalls one
  main-loop pass a second for 300,085 instructions (about 1.25 ms;
  docs/hardware.md). A note-on that arrives just before it waits in the ring
  for up to 0.8 ms. That stall overlaps the wait for the next block, so the
  worst "taken" was 2.91 ms, inside the bound. The worst it can be is the
  bound plus the whole stall, 4.65 ms; the test asserts that.
- **The SID's ADSR delay (AUTHENTIC).** For most notes the envelope comes
  about 32 ms (11 blocks) after the first sample, so the audible onset is
  at 36 to 40 ms; most note-offs hold the sustain level as long before the
  release's first step. The first non-silent sample is not yet the note: it
  is the gated voice's oscillator leaking through the DAC at envelope 0
  (about 40 LSB, -58 dBFS). The real chip behaves the same way, and so does
  reSIDfp:
  - The envelope's rate counter is a 15-bit LFSR with a period of 32767
    cycles. In release at SAW LEAD's rate 6, it resets every 267 cycles; in
    decay at rate 9, every 977.
  - Attack 0 needs the LFSR at position 8. A gate that finds it further on
    waits for the LFSR to come round, up to 32,772 cycles (33.3 ms), before
    the attack takes its first step. Release 6 needs it at 266: a note-off
    that finds it further on, held at the decay's rate, waits the same way.
  - On the host, SIDOU and reSIDfp agree exactly: of release 6's 267 phases,
    tried in turn, 8 start the attack at once (3.0%); of decay 9's 977, 267
    start the release at once (27%; `sid_env.rs`, `sid.rs`).
  - In fm1-emu, 5 of the 64 trials (7.8%) started the attack at once,
    audible from 3.9 ms, and 5 the release. Both gate edges land on block
    starts, so a trial's note-off phase follows its note-on's: the host's
    per-phase figures are the ones a player meets.
- **INSTANT.** The envelope is up in the block that takes the note, and the
  release's first step is in the block that takes the note-off, in all 192
  trials. The envelope's audible onset is the buffer's latency plus one
  block, 3.1 to 6.3 ms; the release's first step is in the buffer 0.34 to
  3.44 ms after the note-off, and audible one block later.

## Timestamps

M1a records no per-byte times. Each MIDI ring entry carries the time of the
poll that took it (`poll_stamp`), which is used only for the longest-wait
diagnostic:
- The UART's RX goes by DMA, so where a byte arrived within the 0.5 ms
  between polls is unknown, and a note-on's three bytes may straddle two
  polls.
- A USB packet that the endpoint held back gets the stamp of the poll that
  finally takes it.
- The register ring has no time at all.

So the best placement the stamps allow is to within one poll, 0.5 ms.

## What sample-accurate placement would take

1. Carry the stamp through `live`, whose sink knows nothing of time, into
   the register ring: a time-marker pseudo-entry before a note's writes.
2. Give the audio interrupt the time its block will play, from the DMA
   half's boundary in TIMER4 ticks.
3. Apply the writes behind a marker at the marker's sample offset. That
   means splitting `sid_render` at each write: render to the offset, write,
   render the rest. `sid_render` already takes any frame count.
4. Delay every note by a constant L, so that no marker is already in the
   past when its block is rendered. A note committed at time c can only go
   into a block rendered at or after c, and that block plays one block
   later. So L must cover the worst commit time after the stamp, plus two
   blocks:
   - with the main loop's measured stalls, 0.5 ms (one poll) plus 0.2 ms
     (one render) plus 5.8 ms: about 6.5 ms;
   - covering the USB retry's stall as well: about 7.6 ms.

   A note that misses its block anyway plays at the next block start, as
   today.

**The cost to the audio interrupt is small.** Each split adds one
`sid_render` call: an out-of-line `fold_owed` and the loop's setup. That is
tens of instructions, an estimate from the code that was not measured. A
block costs about 53k instructions (0.22 ms at 240 MHz) and is gated at
`SID_WORST_PER_CHIP` = 530 per sample per chip. A few splits a block cost
under 0.5%, and `.ram_text` (32,352 of 49,152 bytes) has room for the
marker handling. **It fits the budget.**

**It does not fit cleanly:**
- **The ring invariants change.** Today the interrupt empties the register
  ring every block, and several things rely on that:
  - `MIDI_DRAIN_RESERVE` and `REG_RING_WORST_PASS` size the ring for one
    pass's writes plus one MIDI entry's, because whatever is committed is
    gone by the next block (`platform/fm1/midi.c`, `app/reg_ring.c`);
  - the chip model change waits for an empty ring before it prepares the
    spare chip.

  Held-back entries would stay in the ring for up to two blocks, behind
  everything committed after them (the ring is FIFO, so the panel's writes
  would wait behind a MIDI note's marker too). All of that would need
  redoing.
- **It trades latency for jitter.** Every note would take today's worst
  case, 6.5 to 7.6 ms from the stamp, against 3.0 to 6.3 ms (mean 4.5 ms)
  now. Its jitter would fall from about 3 ms (one block plus the poll) to
  the 0.5 ms of the poll's stamp. A live player feels the extra mean latency on
  every note; the jitter matters most to sequenced, clock-synced parts,
  which are M2's.

When this record was first written, the chip's ADSR delay (most notes 33 ms
late with the factory presets) also made the block's jitter moot. The
envelope option (amendment below) removed that delay, so it is no longer a
reason: the decline rests on the two above.

## Decision

Notes stay applied at the start of the block after they are committed. No
time markers, and no split render.

The latency test (`latency.rs`) asserts the following for every trial:
- the block that takes the note starts within one block and one poll of the
  note's arrival (plus the USB attach's stall when it lands around one);
- the note's first sample plays one block after that;
- the note-off is taken within the same bound;
- with the envelope INSTANT, the envelope rises in the block that takes the
  note and the release steps in the block that takes the note-off; with
  AUTHENTIC, within the ADSR delay's longest (plus the attack).

## Amendment: the bound, and the envelope option (2026-10-15)

**The bound.** The plan's "first non-silent sample in the DMA buffer within one
block and one poll" leaves out the render. `audio_fill` writes the block's
samples at its end, so "buffer" is "taken" plus one audio interrupt, and the
idle maximum above misses it by 0.10 ms. `latency.rs` asserts both bounds:
- taken <= block + poll (+ the USB attach's stall around it);
- buffer <= block + poll (+ the stall) + the render allowance.

The second follows from the first, as the taking interrupt is one of the run's;
it is kept as the plan's statement. The allowance is the run's longest audio
interrupt, from entry to return. It is measured, not a margin, and is itself
held to the audio budget's block: 2700 instructions a sample times 128 samples
(design doc section 8). Measured, it is 53,912 to 54,441 cycles (0.225 to
0.227 ms).

**The ADSR delay** follow-up is decided: an option, not a behaviour change.
- The core has a gate reset, `sid_set_gate_reset`. With it on, each gate edge
  restarts the envelope's rate LFSR, so the attack, or the release, takes its
  first step one rate period and 3 cycles after the edge: 12 cycles at
  attack 0, 270 at release 6. The exponential divider's count is left as it
  is, so a release from a low sustain level still waits for it, as on the
  chip. It is off by default, so the core stays bit-exact with reSIDfp.
- The live instrument has ENVELOPE: INSTANT / AUTHENTIC (CC 26, NRPN 1/10),
  INSTANT by default.
- Song and `.sid` playback (M2, M3) will default to AUTHENTIC.

`docs/reference/controls.md` explains it to players.

What it changes, from the tables above:
- **INSTANT:** the audible onset is the buffer's latency plus one block, 3.1
  to 6.3 ms, against 36 to 40 ms for most notes with AUTHENTIC; the release
  starts in the note-off's block instead of up to 33 ms later. The test
  asserts both.
- **The render** is 0.22 to 0.23 ms with INSTANT against 0.20 to 0.22 ms with
  AUTHENTIC. The attack runs in the block that takes the note instead of 33 ms
  later: the same envelope work, sooner. The run's longest interrupt is the same
  within noise either way (54,078 against 53,912 cycles idle, +0.3%).
- With INSTANT, the first non-silent sample comes earlier within its block (the
  envelope, not only the leak), so "audible" falls by about 0.3 ms (median).

## Amendment: songs are frame-exact through the split render (M2c Task 6, 2026-10-16)

The song player takes the split render this record described, for its own writes only:
it runs in the audio interrupt, and `audio_fill` renders chip A up to the sample where a
call of the play routine falls, makes the call (its writes go straight into chip A with
`sid_write`, `core/player/song_ctl.h`), and renders the rest. The calls fall on a
rational clock (gt2reloc's CIA period, 880.16 samples at PAL 1x, kept in whole samples
and a remainder, so it never drifts), so a song's every frame is at its exact sample, and
the host's render, which splits at the same samples, equals the device's bit for bit
(emu-gate `tests/song.rs`). With multispeed up to 4 a block holds at most one call, so a
split is one more `sid_render` call a block at most. The register ring is untouched:
the player is a second writer of chip A, in the interrupt, between the drain and the
render, so none of the ring's invariants above change. Notes from the keys and MIDI keep
this record's block-start placement. Songs default to AUTHENTIC envelopes (the gate reset
off), set on chip A when a song starts.

**Inside a sample (M3 Task 5a, 2026-10-24):** a `.sid` tune's writes are stamped with
their 6502 cycle, so M3 splits the render at a cycle, not a sample: `sid_advance` (core/sid/sid.h)
clocks the voices the rest of the way into the next sample and takes those cycles off it,
then the write goes in; the samples stay where they were, and the voices reach the state
`sid_clock` reaches with the write at that cycle. The render loop is the song's.

**Stamped writes, a frame ahead (M3 Task 8, 2026-10-24):** a `.sid` tune's 6502 runs in the
main loop up to two PAL frames ahead of the audio (`core/psid/psid_ctl.h`), and its writes
reach the interrupt through a ring of their own, each with its cycle on the tune's count, not
through the register ring. `psid_ctl_block` renders each chip's block split at every write:
the whole samples that end at or before its cycle (stepping the chip's Q16 accumulator, no
division), then `sid_advance` and `sid_write`. So the split render now serves a whole tune's
writes, inside samples, where the song's served one call a block. Two bounds keep the
interrupt's cost: a chip's block is at most `PSID_SPLITS` renders (32 when written; 16, then 24 from 0.5.x), and the writes past
them go in, in order, at the block's end; a write the chip has already passed (the main loop
fell behind) goes in at once. Both are counted as late, and the audio never waits for the
6502. With no late write the samples are those of a render that clocks the chip write by write
at the stamps (`tests/psid_ctl.rs`: every GoatTracker example and SIDOU song, every subtune,
as 1SID and 2SID tunes, about 650,000 writes, sample for sample). The register ring and its
invariants are again untouched: the keys and MIDI keep block-start placement, and a tune's
mutes go through the register ring (`REG_RING_MUTE`) at a block's start, as everything else
that is not the tune's own does.

## Amendment: MIDI clock and transport place the song's calls (M2e Task 2, 2026-10-17)

The record's revisit "with M2's clock and transport" is done for the song: a sequencer's
clock now places the player's calls, not the notes of the keys or MIDI.

- **Placement:** each clock is stamped at the poll that took it (the TRS UART's 2 kHz, USB's
  1 ms frames), the clock follower (app/midi_clock.h) fits a line through the last 96, and
  the call due at a 16th goes at that clock's fitted time plus a fixed delay
  (`MIDI_SYNC_DELAY`, 352 samples, 8.0 ms): the block being heard, the next (rendered
  already when the command is sent) and the stamp's age. song_ctl's commands place a call at
  any sample (`song_ctl_align`, and a held start that waits for one), so the rows are
  sample-exact against the fit: the jitter of the stamps is gone, the fixed delay stays.
- **Measured:** rows against the 16ths of a clock with the TRS poll's jitter, from two beats
  after the start: host 0.29-0.31 ms worst at 90, 120 and 125.3 BPM; fm1-emu over TRS at
  120 BPM 0.03 ms. The cost: none in the audio interrupt beyond the align command's handling
  (a subtraction) and two words a call and a block; in the main loop, a pass with the clock
  and its row's period update stays far below the poll allowance (36.9k at worst with the
  song starting; the dense four-channel stream 25k).
- **Jam notes (TIGHT):** the option the record sketched for notes that drive the player
  (Task 3's jam notes: their stamp carried through song control and placed at a fixed delay)
  waits for song control's jam binding (https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2e-midi.md, Task 3's results):
  there are no jam notes on the device to measure yet. The mechanism is the one above (an
  aligned command at the stamp plus the delay), so its cost is the align's. Decided: not
  now; live notes to the keys' chip stay as M1 decided (block-start placement, 3.0 to 6.7 ms).

## Consequences

- **Measured latency:** the note-on's first sample reaches the buffer in
  0.30 to 3.50 ms and the DAC in 3.0 to 6.7 ms. With the envelope INSTANT
  (the default), the envelope's audible onset is 3.1 to 6.3 ms and the
  release starts in the note-off's block. With AUTHENTIC, both are 36 to
  40 ms for most notes, as on the chip.
- **Revisit sample-accurate placement:**
  - with M2's clock and transport, where sequenced notes want the jitter
    gone and can take a fixed delay;
  - if the UART is read per byte (an RX interrupt) or the stamps get finer
    than a poll.
- **Follow-ups:**
  - **The ADSR delay** was a playability question, for its own decision.
    A synth that answers 3% of notes at once and the rest 33 ms late is the
    real chip's behaviour, and the reason SID music uses a hard restart.
    The options were:
    - a hard restart at note-off (rates 0 while the voice rests, which costs
      the release tail);
    - presets whose release is no slower than their attack;
    - an "immediate attack" option that resets the rate LFSR at the gate.
      The chip cannot do that, so it is a departure from reSIDfp behind a
      setting.

    The third was taken, at both gate edges, since a note-off meets the same
    delay: the envelope option (amendment above).
  - **The USB retry's stall** adds up to 1.25 ms to a note in one main-loop
    pass a second while USB is off. It is already open in docs/hardware.md
    ("The USB retry's stall"): time it on a unit first.
  - **The About page** composes every frame, about 161k instructions
    (0.67 ms), several times the status screen's longest `ui_poll`.
    `latency.rs` does not open it yet. If a stall there pushes "taken" past
    the bound, drain MIDI between compose chunks, or before and after
    `ui_poll`.
