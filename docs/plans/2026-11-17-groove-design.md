<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Groove: design

What the grooveboxes on the FM-1 (SLOOP, JIANT, Hortator) have and SIDOU lacked, agreed with the
owner on 2026-11-17. The next release, 1.21. Built in this order, each merged on its own: swing,
Euclid, ratchets, the converter.

## Decided

- **Swing lives in the song**, as GoatTracker's funktempo (option A): it is saved in the `.sng`
  and plays the same in GoatTracker. Not a global groove, not a live nudge.
- **One SWING value per song**, beside its tempo (option A); per-pattern swing stays the raw
  FUNKTEMPO command in TRACKER.
- **Euclidean rhythms are a STEP lane mode steered by knobs** (option A), not a DICE roll.
- **The `.sid` converter makes an editable, musical song** (option A), not a literal frame copy.

## Swing

- The song views get **SWING**: STRAIGHT, LIGHT, SHUFFLE, HARD, with the big popup. At speed 6
  they are funktempo 6/6, 7/5, 8/4 and 9/3 (50, 58, 67 and 75 % of a row pair); at other speeds
  the same ratios rounded to whole ticks, and a level a speed cannot give is refused in the popup.
- It writes the funktempo into the song's first row; TRACKER shows that row's FUNKTEMPO. The BPM
  shown stays the two speeds' average, as today.
- `.sng`, the site's editor and `sidou-song/1` (a `swing` field) carry it.

## Ratchets

- A **RATCHET** lock per STEP step: OFF, x2, x3, x4, set like the other locks, drawn as small ticks
  on the step.
- A ratcheted step plays a copy of its instrument whose wave table gates the note off and on 2, 3
  or 4 times within the step, so GoatTracker plays it too. Copies are shared (one per instrument
  and count); out of instrument slots, the lock is refused with a toast.

## Euclidean lanes

- A STEP lane's **EUCLID** mode: **HITS** (0 to the lane's length, spread as evenly as possible,
  Bjorklund) and **ROTATE**. A change is heard on the next pass.
- The hits are written as real steps, so the lane's note, locks and ratchets apply; EUCLID off
  keeps the steps as they are. DICE is unchanged.

## The `.sid` converter (the site)

- On the Library page: drop a `.sid`; PSID first, run by SIDOU's own 6502 code in the browser
  (RSID needs a fuller C64 and comes after, if PSID works well). A sub-tune and a length (60 s).
- From each frame's SID writes: the row grid and tempo (note onsets; SET TEMPO and swing), notes
  (pitches, arpeggios as an arpeggio wave table, slides as portamento), and per voice a handful
  of instruments clustered from recurring waveform, ADSR, pulse and filter shapes, with their wave,
  pulse and filter tables. Six-voice tunes use both chips.
- Out: a `.sng` to open in the editor, send or download; the original and the conversion played
  side by side; the simplifications listed (hard restarts, digis). Local only, nothing uploaded;
  the docs say a converted song keeps the original's copyright.

## Budgets

Swing, ratchets and Euclid are main-loop and song-data work: nothing in the audio interrupt, a
few KB of flash of the ~31 KB free. The converter is site only.

## Tests

- Swing: every level at speeds 3-8, the `.sng` round trip, the timing in fm1-emu against
  GoatTracker's player.
- Ratchets: the retriggers frame by frame, instrument sharing, the slot limit.
- Euclid: the known patterns (E(3,8), E(5,8), E(7,16) and others), rotation, locks kept.
- Converter: notes, tempo and instruments on known starter-pack tunes; the converted song
  close to the original in the emulator.
