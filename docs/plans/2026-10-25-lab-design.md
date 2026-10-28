<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# LAB: shape one sound and keep it (design, 2026-10-25)

Agreed with the user in a design session after v0.5.0. Inputs:
[https://github.com/dsablic/sidou/blob/v0.5.0/docs/TODO.md](https://github.com/dsablic/sidou/blob/v0.5.0/docs/TODO.md) "LAB", [what other firmwares do](2026-10-25-lab-inspiration.md),
[the button audit](2026-10-25-button-audit.md).

**The goal:** the FM-1 is an ideal device for experimenting on the go. LAB is where a LIVE sound
is shaped with every parameter visible and kept with one press. **Ergonomics first** (the user):
you see what you are tweaking and what it does; everything is logical and clearly visible.

## Decisions

| Question | Decision |
|---|---|
| What a sound holds | The voice (waveform, ring, sync, ADSR, pulse width and PWM, filter mode, cutoff, resonance, voice to filter, vibrato, ARP) and the three effect slots. **Not** the chip's profile and character: they stay global (CHIP, USER 1-8). |
| How LAB is reached | **The labels lead:** on LIVE, ENV, LFO and FX open LAB at their page; also a LAB item in the menu. In LAB, SELECT steps the pages; HOME goes back to LIVE with the sound kept as edited. |
| A page | **The shape on top, four cards below:** the top two thirds draw what the page shapes from the live values; four cards (name, value, bar) for K1-K4; the card being turned lights, and so does its part of the drawing. ALGORITHM picks the page's discrete choice. |
| Keeping | **One press:** SAVE keeps. A sound loaded from a USER slot is updated in place; any other goes to the next free USER slot with an automatic name (`KEPT AS U03 LAB 03`). EDIT + SAVE is the slot-and-name page. PRESETS on LIVE steps the built-ins, DICE, then USER 1-16. The unsaved edit comes back at power-on. |
| Experimenting | A/B compare, undo, the variation rolls. Hold-to-peek later. |
| Songs | **LAB's second step:** a sound sent to the current song as a GoatTracker instrument (the voice part; the effects stay on LIVE, and the screen says so), and an instrument pulled into LAB. Its own design: a LIVE sound and a GoatTracker instrument describe ADSR, pulse and filter differently (the wave, pulse and filter tables). |

## The pages

| Page | Drawn | K1 · K2 · K3 · K4 | ALGORITHM | Reached by |
|---|---|---|---|---|
| OSC | one cycle of the waveform at its pulse width; ring and sync as a second trace | pulse width · PWM depth · PWM rate · fine tune | the waveform (TRI, SAW, PUL, NOI, the combined ones), then +RING, +SYNC | SELECT |
| ENVELOPE | the ADSR curve to scale in ms, the gate's length marked; the segment turned lights | attack · decay · sustain · release | the envelope mode | ENV |
| FILTER | the mode's response with the cutoff and the resonance peak at their values | cutoff · resonance · voice to filter · (free) | LP, BP, HP, notch | SELECT |
| MOD | the vibrato's and the PWM's waves over a second; ARP's steps when on | vibrato rate · depth · delay · velocity target | ARP off or its order | LFO |
| FX | the three slots as a chain (OFF dim); the selected slot's shape where it has one | the slot's Amount · A · B · Mix | the slot's algorithm (OFF included) | FX; SCL the next slot |

On every page: the header (page name, `2/5`, the sound's name with `*` when edited, **A** or **B**
while comparing); a one-line hint at the bottom (e.g. `EDIT: A/B · +LFO UNDO · +ARP VARY`), so
nothing in LAB is found only on the help card. The keys play, and the drawings move with the live
values (the envelope runs, the filter sweeps).

Open, for the plan: FILTER's K4 (free) and MOD's K4 (velocity target): keep or find better.

## Controls in LAB

| Control | Action |
|---|---|
| SELECT | the next or previous page |
| K1-K4 | the page's cards |
| ALGORITHM | the page's choice |
| ENV · LFO · FX | ENVELOPE · MOD · FX |
| SCL | the next effect slot (FX page) |
| SAVE | keep (in place for a USER sound, else the next free slot) |
| EDIT + SAVE | the slot-and-name page: K1 the slot, the keys the name, SAVE writes, a used slot asks once more |
| EDIT tapped alone | A/B: the sound as kept or loaded / the edit |
| EDIT + LFO | undo (16 steps; a roll is one step) |
| EDIT + ARP | a small variation of this page (as MUTATE): never silent, never the same twice |
| EDIT + SCL | a bigger roll of the whole sound (as DICE) |
| PRESETS | load another sound into LAB |
| HOME | back to LIVE, the sound kept as edited |

The note keys, OCT, PLAY, SEQ, REC, GLO and MASTER act as everywhere, so a song plays on while a
sound is shaped. SAVE + a knob is MIDI learn of that card's parameter, as on LIVE (every LAB
parameter has a CC).

## The one change on LIVE

ENV, LFO and FX open LAB at their page. The 6581/8580 toggle leaves FX (ALGORITHM on TONE already
steps the profile, both models among them; FX's LED may still show the 8580); "next algorithm"
leaves ENV for ALGORITHM on LAB's FX page. The first-run tour's PRESS ENV then opens LAB. LIVE's
help card, the cheat sheet, the demo page, docs/reference/controls.md, docs/hardware-test.md and
the tests change in the same commit.

## Data

- A sound: about 64 bytes and a 12-character name. 16 USER slots in one store chunk with its own
  format version (as the USER profiles); about 1.3 KB of the store.
- The unsaved edit: one more record, written by the 5 s debounce; back at power-on.
- An older firmware ignores the chunk.

## Costs

- **RAM is the constraint:** the A copy (about 64 bytes), the undo ring (16 steps, about 48
  bytes), the page and flags: about 120 bytes, where M3 left about 48 of `FREE_RAM_MIN`'s budget
  (the device has about 16 KB free; the budget keeps room for later). The plan first wins RAM back
  or moves the budget, on numbers; undo can shrink to 8 steps.
- **No audio change:** LAB sets parameters LIVE already has; RAMTEXT is not touched.
- **Flash:** the view and its drawings; about 7 KB under M3's ceiling, and M4's and M5's room.
- **Input plumbing** (the audit, section 5): EDIT's tap needs a release-acting spoil flag like
  save_key's; each LAB page needs a panel page id and its CC mapping for MIDI learn; `knob_page`,
  `shown()` and `help_card_of` branches; the popups; a help card per page with its goldens.

## Testing

- Host: save and load, the in-place / next-free rule, the chunk's version, undo, the rolls (never
  silent, never repeated, undoable), A/B.
- fm1-emu: every page's golden, the hint line, ENV / LFO / FX from LIVE, PRESETS into USER 1-16,
  the edit through a power cycle, MIDI learn on a card.
- The ergo harness: a new journey, "keep a happy accident" (LIVE, ENV, two turns, SAVE): presses
  and time.
- On the device, with the user: the drawings' readability and the keep flow.

## Later

The song bridge (above); hold-to-peek; macros (filter + resonance, pulse + its modulation);
a browser with FAV and RECENT; a morph between A and B.
