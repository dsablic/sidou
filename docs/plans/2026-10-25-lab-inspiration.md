<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# LAB: what other firmwares do (inspiration, 2026-10-25)

Input for the LAB design session (https://github.com/dsablic/sidou/blob/v0.5.0/docs/TODO.md, "LAB"). Read from cheat sheets and READMEs, not
source; ideas are adopted and improved, never copied (SIDOU's own design).

## FM-1 firmwares

- **Clipper** (closest to LAB; [cheat sheet](https://thefirmwarehub.com/cs/clipper/)): EDIT pages
  incl. MACROS, MORPH, RANDOM, UNDO; macros BRITE/TIMBR/MOTN/SPACE on every engine; 16-step undo,
  K2 compares original and edited; 32 USER slots (K1 slot, K2 load, K3 erase); autosave 2.5 s
  after the last change, restored at power-on.
- **Golana** ([repo](https://github.com/Gadlele/Golana)): every page draws its shape from live
  values (waveforms, filter curve, envelopes, running LFOs); four on-screen knob cards for K1-K4.
- **SID-1** ([repo](https://github.com/putchdev/sid-1); same chip): nine knob pages, ENV drawn as
  an envelope, 128 slots; a web editor with every field at once, playing live on the device.
- **Duell** ([repo](https://github.com/oscillr/DUELL)): a fader from A (saved) to B (a smart
  random variation of A), ROLL, UNDO, AMOUNT; per-sound trim apart from the mixer level.
- **JIANT** ([repo](https://github.com/juanjiant-bit/JIANT)): a roll shows BEFORE / NOW, OCT+ keeps,
  OCT- cancels; undo/redo of a big change; thermal colours for values.
- **Noiz Demons** ([repo](https://github.com/SPIKE-the-Percussionist/noiz_demons)): rolls at scopes
  (all, a page family, operators) with a power from nudge to rewrite; never silent, never the same
  twice in a row; hold SAVE returns to before the roll.
- **Melodee** ([repo](https://github.com/keremimo/melodee)): an on-device sound designer (a goal,
  from INIT or the current tone, compare with the original, name, save); FAV / RECENT / category
  browser; EDIT held undoes, with OCT-/OCT+ through 8 edits.
- **Mini-Tone** ([repo](https://github.com/eldontyrellcorp/Mini-Tone)), **Felucca**
  ([cheat sheet](https://thefirmwarehub.com/cs/felucca/)): hold LFO + a knob is a momentary tweak
  that reverts on release; hold SAVE undoes; 32 user presets with categories and stars; restore
  the last state at power-on; save only while stopped (a limit to avoid).
- **Etch** ([cheat sheet](https://thefirmwarehub.com/cs/etch/)): save asks one more SAVE only when
  the slot is used; a preset that leaves out its drawn wave (a trap to avoid).
- **Obervave** ([repo](https://github.com/zvenson/obervave)): SAVE twice to save; random "within
  useful ranges"; autosave 3 s after a change.

Elsewhere: Elektron's sound pool (users find saving a lock back into a sound fiddly, and copies
don't follow the source); MIDIbox SID's pages (OSC, LFO, ENV, MOD, TRG, KNB) with one encoder.

## The ideas that fit LAB (240 x 240, four knobs)

1. A shape drawn per page from live values (Golana, SID-1).
2. Four knob cards per page: name, value, a bar (Golana, Melodee).
3. Hold-to-peek: a held button + a knob reverts on release; a keep press makes it stay (Mini-Tone).
4. Compare on one control: A/B toggle first (Clipper); a morph later, if the sound interpolates
   (Duell).
5. Undo at two levels: a ring of 8-16 edits, and "before the roll" (Clipper, Melodee, Noiz Demons).
6. Rolls with a scope (page, family, all) and an amount; before/now shown; keep/cancel; never
   silent; never repeated (Noiz Demons, Duell, JIANT).
7. Quick save: the slot on K1, the same key twice confirms; asks once more only over a used slot
   (Etch, Obervave).
8. The edit buffer saved by itself and back at power-on (Clipper, Mini-Tone).
9. A browser with FAV, RECENT, categories; hold SAVE returns to the previous sound (Melodee).
10. A sound's trim apart from the mix level: loading a sound never changes the mix (Duell).
11. SID macros: filter + resonance, pulse width + its modulation, attack/decay (Clipper's idea).
12. Thermal colours for values, a large-text mode (JIANT, Mini-Tone).
13. Song instruments: decide copy or link up front, and say which on screen (Elektron's lesson).

## To avoid

Hidden drift of a saved sound; a save that leaves out part of what LAB shows; long tap sequences
without a hint line; a roll that can't be undone; saving only while stopped (SIDOU saves while
playing).
