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
at `e52f4640`, 64 trials per case. Each trial boots afresh and injects a
note-on (C4, SAW LEAD) at a random cycle, so its phase against the block,
the poll, the 10 ms tick and the screen's 40 ms compose is random. Times
run from the note-on's last byte in the UART; its three bytes take
0.96 ms on the wire before that.
- **Taken:** the audio interrupt that applies the note is entered (the
  voice's envelope gate is set in that block).
- **Buffer:** that interrupt returns, with the note's first non-silent
  sample in the DMA buffer.
- **Audible:** that sample leaves the DAC.

"Non-silent" means a deviation from the 20 ms before the note-on, by more
than the baseline's own largest deviation plus 8 LSB. A nonzero test would
fail on the DC level and on the waveforms that voices leak with their gate
off. The note's envelope is measured separately: a deviation past -30 dB of
the held note's peak.

## Measured

Milliseconds from the note-on's last byte: min / median / mean / max.

| Case | Taken | Buffer | Audible | Envelope audible |
|---|---|---|---|---|
| Idle (status screen) | 0.25 / 1.95 / 1.88 / 3.25 | 0.44 / 2.14 / 2.07 / 3.45 | 3.65 / 5.32 / 5.25 / 6.65 | 3.98 / 38.2 / 36.3 / 39.9 |
| Busy (K1-K3 turning, the screen redrawing) | 0.26 / 1.91 / 1.87 / 3.17 | 0.46 / 2.11 / 2.07 / 3.37 | 3.66 / 5.30 / 5.27 / 6.59 | 4.58 / 38.4 / 37.3 / 39.8 |
| USB retry (the note-on around `usb_attach_now`) | 0.15 / 1.83 / 1.66 / 2.95 | 0.34 / 2.02 / 1.85 / 3.15 | 3.55 / 5.23 / 5.06 / 6.35 | 36.8 / 37.8 / 37.8 / 38.9 |

**What the numbers show:**
- **The pipeline.** In every case, the block that takes the note starts
  within one block and one poll (3.40 ms) of the note's arrival. The test
  asserts this for every trial.
- **The render.** The block's samples are in the buffer when the interrupt
  returns, 0.19 to 0.23 ms after it is taken, because `audio_fill` converts
  them at its end. So the plan's bound on the buffer, read literally, is
  missed by up to that render: the idle maximum is 3.45 ms against 3.40 ms.
  The bound leaves out the render. The test asserts "taken" against it, and
  "buffer" is "taken" plus one render.
- **Audible.** One more block after the buffer: 3.5 to 6.7 ms.
- **The screen.** The busy screen changes nothing measurable:
  - the longest `ui_poll` is 41.5k instructions (one strip drawn, about
    0.17 ms);
  - the longest wait in the MIDI ring is 117 us idle and 124 us busy.

  The audio interrupt's render (0.2 ms), which holds off the main loop,
  matters as much as the screen.
- **The USB retry.** While USB is off, `usb_retry`'s attach stalls one
  main-loop pass a second for 300,085 instructions (about 1.25 ms;
  docs/hardware.md). A note-on that arrives just before it waits in the ring
  for up to 1.0 ms. That stall overlaps the wait for the next block, so the
  worst "taken" was 2.95 ms, inside the bound. The worst it can be is the
  bound plus the whole stall, 4.65 ms; the test asserts that.
- **The SID's ADSR delay.** For most notes, the envelope comes about 32 ms
  (11 blocks) after the first sample, so the audible onset is at 36 to
  40 ms. The first non-silent sample is not yet the note: it is the gated
  voice's oscillator leaking through the DAC at envelope 0 (about 40 LSB,
  -58 dBFS). The real chip behaves the same way, and so does reSIDfp:
  - The envelope's rate counter is a 15-bit LFSR with a period of 32767
    cycles. In release at SAW LEAD's rate 6, it resets every 267 cycles.
  - Attack 0 needs the LFSR at position 8. A gate that finds it further on
    waits for the LFSR to come round, up to 32,772 cycles (33.3 ms), before
    the attack takes its first step.
  - On the host, SIDOU and reSIDfp agree exactly: 13 of 400 phases (3.3%)
    start at once, and the rest wait 33.0 to 33.3 ms. Every one of release 6's
    267 phases, tried in turn, gives 8 at once (3.0%; `sid_env.rs`).
  - In fm1-emu, 6 of the 192 trials (3.1%) started at once, audible from
    4.0 ms. The rest waited.

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
block costs about 47k instructions (0.197 ms at 240 MHz) and is gated at
`SID_WORST_PER_CHIP` = 530 per sample per chip. A few splits a block cost
under 0.5%, and `.ram_text` (32,048 of 49,152 bytes) has room for the
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
  case, 6.5 to 7.6 ms from the stamp, against 3.5 to 6.7 ms (mean 5.25 ms)
  now. Its jitter would fall from about 3 ms (one block plus the poll) to
  the 0.5 ms of the poll's stamp. A live player feels the extra mean latency on
  every note; the jitter matters most to sequenced, clock-synced parts,
  which are M2's.
- **The chip's ADSR delay dwarfs it.** With the factory presets (attack 0
  after a slower release), about 97% of notes start their attack about
  33 ms after the gate, and the rest at once. The gate's exact cycle within
  the block sets the LFSR phase, so placing gates sample-accurately leaves
  that 0-or-33 ms choice as random as before. Removing 2.9 ms of block
  jitter changes nothing audible while a 33 ms one remains.

## Decision

Notes stay applied at the start of the block after they are committed. No
time markers, and no split render.

The latency test (`latency.rs`) asserts the following for every trial:
- the block that takes the note starts within one block and one poll of the
  note's arrival (plus the USB attach's stall when it lands around one);
- the note's first sample plays one block after that;
- its envelope rises within the ADSR delay's longest plus the attack.

## Amendment: the bound, and the attack option (2026-10-15)

**The bound.** The plan's "first non-silent sample in the DMA buffer within one
block and one poll" leaves out the render. `audio_fill` writes the block's
samples at its end, so "buffer" is "taken" plus one audio interrupt, and the
idle maximum above missed it by 0.05 ms. `latency.rs` now asserts both bounds:
- taken <= block + poll (+ the USB attach's stall around it);
- buffer <= block + poll (+ the stall) + the render allowance.

The allowance is the run's longest audio interrupt, from entry to return. It is
measured, not a margin, and is itself held to the audio budget's block: 2700
instructions a sample times 128 samples (design doc section 8). Measured, it is
52,081 to 54,452 cycles (0.217 to 0.227 ms).

**The ADSR delay** follow-up is decided: an option, not a behaviour change.
- The core has a gate reset, `sid_set_gate_reset`. With it on, a rising gate
  restarts the envelope's rate LFSR, so attack 0 steps 12 cycles after the gate.
  It is off by default, so the core stays bit-exact with reSIDfp.
- The live instrument has ATTACK: INSTANT / AUTHENTIC (CC 26, NRPN 1/10),
  INSTANT by default.
- Song and `.sid` playback (M2, M3) will default to AUTHENTIC.

`docs/reference/controls.md` explains it to players.

Measured again at the same seeds, 64 trials a case. AUTHENTIC is set by CC 26
after the boot, which shifts the trials' phases. Milliseconds, min / median /
mean / max:

| Case | Attack | Taken | Buffer | Audible | Envelope in buffer | Envelope audible |
|---|---|---|---|---|---|---|
| Idle | INSTANT | 0.25 / 1.95 / 1.88 / 3.25 | 0.47 / 2.17 / 2.10 / 3.48 | 3.17 / 4.87 / 4.80 / 6.17 | 0.47 / 2.17 / 2.10 / 3.48 | 3.28 / 4.98 / 4.91 / 6.29 |
| Busy | INSTANT | 0.26 / 1.91 / 1.87 / 3.17 | 0.48 / 2.14 / 2.10 / 3.40 | 3.18 / 4.83 / 4.79 / 6.09 | 0.48 / 2.14 / 2.10 / 3.40 | 3.27 / 4.93 / 4.89 / 6.21 |
| USB retry | INSTANT | 0.15 / 1.83 / 1.66 / 2.95 | 0.37 / 2.05 / 1.88 / 3.17 | 3.07 / 4.75 / 4.58 / 5.87 | 0.37 / 2.05 / 1.88 / 3.17 | 3.19 / 4.86 / 4.69 / 5.99 |
| Idle | AUTHENTIC | 0.25 / 1.72 / 1.72 / 3.26 | 0.45 / 1.92 / 1.92 / 3.45 | 3.59 / 5.12 / 5.09 / 6.66 | 0.89 / 33.7 / 31.4 / 35.4 | 3.68 / 38.1 / 35.5 / 39.9 |
| Busy | AUTHENTIC | 0.06 / 1.67 / 1.67 / 3.16 | 0.26 / 1.86 / 1.87 / 3.35 | 3.46 / 5.08 / 5.07 / 6.56 | 1.09 / 33.8 / 32.8 / 35.3 | 3.88 / 38.3 / 37.1 / 39.8 |
| USB retry | AUTHENTIC | 0.15 / 1.83 / 1.66 / 2.95 | 0.34 / 2.02 / 1.85 / 3.15 | 3.55 / 5.23 / 5.06 / 6.35 | 32.3 / 34.0 / 33.8 / 35.1 | 36.8 / 37.8 / 37.8 / 38.9 |

- **INSTANT:** the envelope is up in the block that takes the note, in every
  trial, so the audible onset collapses to the buffer's latency plus one block:
  3.2 to 6.3 ms, against 36 to 40 ms for most notes before. The test asserts
  that.
- **AUTHENTIC:** unchanged from above. The envelope is up within the ADSR
  delay's longest and the attack.
- **The render** is 0.22 to 0.23 ms with INSTANT against 0.20 to 0.22 ms with
  AUTHENTIC. The attack runs in the block that takes the note instead of 33 ms
  later: the same envelope work, sooner. The run's longest interrupt is the same
  within noise either way (54,088 against 53,913 cycles idle, +0.3%).
- With INSTANT, the first non-silent sample comes earlier within its block (the
  envelope, not only the leak). So "audible" falls by about 0.4 ms.

## Consequences

- **Measured latency:** the note-on's first sample reaches the buffer in
  0.34 to 3.45 ms and the DAC in 3.5 to 6.7 ms. These figures exclude the
  envelope. With the factory presets, the envelope's audible onset is 36 to
  40 ms, or 4 to 7 ms for the 3% of notes that escape the ADSR delay.
- **Revisit sample-accurate placement:**
  - with M2's clock and transport, where sequenced notes want the jitter
    gone and can take a fixed delay;
  - if the UART is read per byte (an RX interrupt) or the stamps get finer
    than a poll;
  - once the ADSR delay has a decision (below).
- **Follow-ups:**
  - **The ADSR delay is a playability question, for its own decision.**
    Decided: the attack option (amendment above). A
    synth that answers 3% of notes at once and the rest 33 ms late is the
    real chip's behaviour, and the reason SID music uses a hard restart.
    Options include:
    - a hard restart at note-off (rates 0 while the voice rests, which costs
      the release tail);
    - presets whose release is no slower than their attack;
    - an "immediate attack" option that resets the rate LFSR at the gate.
      The chip cannot do that, so it would be a departure from reSIDfp
      behind a setting.
  - **The USB retry's stall** adds up to 1.25 ms to a note in one main-loop
    pass a second while USB is off. It is already open in docs/hardware.md
    ("The USB retry's stall"): time it on a unit first.
  - **The About page** (m1e-look) composes every frame, about 161k
    instructions (0.67 ms), several times the status screen's longest
    `ui_poll`. Measure it with `latency.rs` on that branch. If a stall then
    pushes "taken" past the bound, drain MIDI between compose chunks, or
    before and after `ui_poll`.
