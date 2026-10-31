<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Button and knob audit (2026-10-25)

Read from the code on main, for the LAB design (https://github.com/dsablic/sidou/blob/v0.5.0/docs/TODO.md "LAB",
[inspiration](2026-10-25-lab-inspiration.md)). Paths are under `app/` unless stated.
Docs: docs/reference/controls.md ("cm").

## How input flows

- Ids: buttons 0-13 are OCT-, OCT+, FX, SCL, ENV, LFO, EDIT, GLO, HOME, SAVE, ARP, SEQ, PLAY, REC
  (panel.h:30-54). Encoders are SELECT 0, ALGORITHM 1, K1-K4 2-5, PRESETS 6 (panel.h:61-67).
  Note keys are ids 14-40. The HAL has 14 buttons and 7 encoders, so **no encoder has a push**
  (platform/fm1/hal/fm1_input.h:145-146).
- Order of routing: capture, then `global()` (views.c:1752), then the top view's `input`, then the
  panel (screen.c:114; views_buttons views.c:2583). Only presses a view or `global` did not
  consume reach `panel_buttons` (views.c:2594-2607).
- **`panel_buttons` runs for every view that does not consume the press** (panel.c:99-118).
  OCT-, OCT+, FX, SCL and ENV therefore act the same in every view, and the panel handles no other
  button. A view can re-bind any of them locally just by consuming the press.
- `global()` takes HOME (1757), SEQ (1763), PLAY (1768), REC (1772) and GLO's tap (1780, then
  passes it on). These work in every view.
- SAVE's press never reaches views or the panel (views.c:2575). `save()` (views.c:2503) picks the
  meaning by view.
- K1-K4 follow the "page". `knob_page()` (views.c:2022) knows only CHIP's page, and LIVE's page
  everywhere else. A view that takes the K knobs for itself (STEP with a step held, TRACKER with
  REC, ORDER/INSTR/TABLE K1/K2) consumes them in its own handler.

## 1. Matrix

U = USED, W = WEAK (label mismatch or rarely needed), F = FREE, C = CONFLICT/INCONSISTENT.
Columns: L = LIVE (TONE page), LS = LIVE slot page (SELECT), ME = menu / SETTINGS / MIDI / SONG
lists, CH = CHIP, SC = SCENE / STATUS / ABOUT / CHARSET, ST = STEP, TR = TRACKER, OR = ORDER,
IN = INSTR, TB = TABLE (and its ARP helper).
"-" in a cell means the same as the cell to its left.

### Buttons (press unless stated)

| Control | L | LS | ME | CH | SC | ST | TR | OR | IN | TB |
|---|---|---|---|---|---|---|---|---|---|---|
| OCT- / OCT+ | U octave (panel.c:111-116) | - | U (also the OCTAVE item) | - | - | U (audition octave) | - | - | - | - |
| OCT- + OCT+ together | F | F | F | F | F | F | F | F | F | F |
| FX | W toggles 6581/8580, clears a USER profile (live.c:1458-1467) | - | W | W | W | W | W | W | W | W |
| SCL | W next slot, no page change | U next slot, page follows (live_view.c:1185) | W | W moves focus on a slot | W | C: next slot; EDIT+SCL = DICE when no step is held and not recording (step_view.c:1051) | W | W | W | W |
| ENV | W next algorithm of the selected slot, wraps through OFF (live.c:1935) | - | W | W | W | W | W | W | W | W |
| LFO | F | F | F | F | F | U step held: clear locks (step_view.c:1057); F otherwise | U REC on: clear note, 2nd press clears the row (tracker_view.c:747-767); F with REC off | F | F | F |
| EDIT (press alone) | F | F | F | F | F | F | F | F | F | F |
| EDIT held (modifier, see combos below) | U | U | U | U | F | U | U | U | U | F |
| GLO | W tap-tap latch (views.c:1735-1749) | - | - | - | - | - | - | - | - | - |
| HOME tap | U menu | U | U back | U | U | U | U | U | U | U (helper: close) |
| HOME hold 600 ms | U help card (views.c:2036, 2219) | - | - | - | - | - | - | - | - | - |
| SAVE | W settings now (views.c:2547-2551) | - | U OK of the list | U USER slot, else settings | W settings | U song | - | - | - | U song; helper rows when the helper is open |
| SAVE held + K1-K4 | U MIDI learn (views.c:2656-2668) | U (slot params) | U | U (page's params) | U | U (K when no step is held) | U | U | U | U |
| ARP | U tap on release switches the chord arp (live_view.c:1170-1175, 1216) | - | F | F | F | C: lane gate STACCATO/LEGATO (step_view.c:1064); EDIT+ARP = MUTATE | C: REC on = key off; REC off = F (tracker_view.c:747) | C: set restart (song_pages.c:439) | F | U opens the ARP helper (song_pages.c:510); F inside it |
| SEQ | U opens the last song view (views.c:1763) | - | - | - | W: refused over ABOUT's tune | U next song view | - | - | U; EDIT+SEQ = jump to the table (song_pages.c:538) | - |
| PLAY | U plays/stops the song; EDIT+PLAY resumes (views.c:1768) | - | - | - | - | - | - | - | - | - |
| REC | W only a toast and an armed flag (views.c:1772-1777) | - | W | W | W | U (STEP records while playing) | U (note entry) | W | W | W |

### Encoders and pot

| Control | L | LS | ME | CH | SC | ST | TR | OR | IN | TB |
|---|---|---|---|---|---|---|---|---|---|---|
| SELECT | U pages TONE/DIST/MOD/FX (live_view.c:1185) | U | U cursor | U focus | C: SCENE changes piece, the rest F | C: lane | C: row (EDIT: 8 rows) | U cursor (EDIT: 12, a page) | U parameter (EDIT: 8) | U row (EDIT: 12, a page) |
| ALGORITHM | W profile (not an algorithm) (1196-1201) | U slot algorithm | U value | U | C: SCENE changes piece (same as SELECT); F on STATUS/ABOUT/CHARSET | C: page (step held: chance) | C: voice (EDIT: edit step) | U subtune / voice | F | U table choice |
| PRESETS | U preset (panel.c:175-178) | - | - | - | - | C: lane instrument (step held: the step's note) | C: REC on = instrument; EDIT+ = insert/delete row; REC off = preset | U EDIT+: insert/delete; else preset | U instrument | U preset |
| K1-K4 | U (TONE) | U slot Amount/A/B/Mix | U | U page | U | U until a step is held; then locks | U until REC is on | K1/K2 value/kind; K3/K4 as LIVE | K1/K2 byte; K3/K4 as LIVE | K1/K2 bytes; K3/K4 as LIVE |
| ARP held + K1-K3 | U rate, order, span (live_view.c:1177) | F | F | F | F | F | F | F | F | F |
| EDIT + K1-K4 | U slot (panel.c:187) | U | U (popup only) | U slot (chip focus), expert: slot | U | C: character locks with a step held (step_view.c:1121) | C: with REC on it edits the cell, not the slot (tracker_view.c:815-837) | C: K1/K2 ignore EDIT (song_pages.c:454) | C: same (490) | C: same (526) |
| EDIT + ALGORITHM | U slot algorithm (live_view.c:1184) | U | F | U save target | F | step held: chance | U edit step | F | F | F |
| EDIT + PRESETS | U revert preset | - | - | - | - | C: not a revert, instrument | U insert/delete (REC on) | U insert/delete | C: not a revert | U revert |
| MASTER | U | U | U | U | U | U | U | U | U | U |
| Note keys | play | play | play | play | play | steps 1-16 toggle, keys 17+ audition | audition / write with REC | audition | audition | audition |

Other combos, in all views: GLO held (or latched) + keys 1-6 = mute/solo (no step toggles in STEP).
Step key held in STEP + K1/K2/K3/K4/ALG/PRESETS/LFO = locks, chance, note, clear.

## 2. Gesture inventory

- **Holds:** HOME 600 ms (help); EDIT as a modifier (about 14 combos above); ARP held + K1-K3
  (LIVE only); SAVE held + K1-K4 (learn); SAVE held + HOME (clear all inside the learn frame,
  otherwise HOME is consumed: views.c:2576); GLO held + keys; step key held + knobs (STEP);
  EDIT+PLAY; EDIT+SEQ on INSTR. At power-on: HOME held (full boot), OCT- alone held about 1 s (ROM
  boot, which must stay untouched: cm:30-40).
- **Double-tap:** only GLO, 400 ms (views.c:1735). That is the precedent for any timed gesture.
- **Release-acting buttons:** only HOME (views.c:2232-2236), SAVE (save_key.c:12-19) and ARP on LIVE
  (live_view.c:1216). The rest act on press.
- **Free globally:**
  - OCT- + OCT+ together. A chord is possible by testing `held`, but the first press has already
    moved the octave, so it needs a compensation or a release-acting rule.
  - HOME + anything. HOME's release always acts (menu or back) unless it is held 600 ms or SAVE
    consumed it. Nothing spoils it, so a HOME combo needs a spoil flag like save_key.
  - SEQ, PLAY and REC held, and LFO, ENV, SCL and FX held. They have no hold meaning, but they
    already act on press.
  - EDIT tap alone. EDIT has no action of its own, but it would need a spoil flag so that using it
    as a modifier does not also trigger the tap.
  - EDIT + REC, EDIT + OCT, EDIT + FX, EDIT + GLO. EDIT + SAVE is not free: it saves, because
    only a press while SAVE is down spoils it.
- **Not free:** SAVE hold without a knob. A clean release saves at any length, and nothing times
  it.

## 3. Label fit

| Label | LIVE | Song views |
|---|---|---|
| OCT- / OCT+ | fits | fits (audition octave) |
| FX | **no**: chip model, not effects | same |
| SCL | **no**: next effect slot (scale only in STEP's DICE and MUTATE) | EDIT+SCL = DICE in STEP, partial fit; otherwise as LIVE |
| ENV | **no**: next slot algorithm, not the envelope. The envelope sits on K3 (decay) and K4 (release) | same |
| LFO | **no**: nothing on LIVE, although the engine has a pulse LFO (live.c:479-493, on K3) | clear and undo, not an LFO |
| EDIT | partly: layer for slots | modifier, with different meanings per view |
| GLO | weak (mute/solo of the song's voices) | weak |
| HOME, SEQ, PLAY, REC | fit | fit |
| SAVE | partial: saves settings, not the sound | fits (song) |
| ARP | fits (chord arp) | STEP gate, TRACKER key off, ORDER restart: weak. TABLE helper: fits |
| ALGORITHM | steps the chip profile on TONE, which is not an algorithm | voice, page, chance: weak |

## 4. Hidden actions (no on-screen hint outside the HOME-hold card)

The views draw no help line (cm:516-517), so everything below is discoverable only from the
HOME-hold card (help_view.c), except what the screen reflects while the control is held
(LIVE's knob row follows EDIT and ARP: views.c:2863-2867).

- EDIT+K1-K4 slot, EDIT+ALG slot algorithm, EDIT+PRESETS revert, EDIT+SELECT (TRACKER page),
  EDIT+PLAY resume, EDIT+SEQ jump, EDIT+LFO undo, EDIT+ARP MUTATE, EDIT+SCL DICE.
- ARP tap versus hold, and ARP+K.
- SAVE+K learn and SAVE+HOME.
- GLO double-tap latch and GLO+key.
- HOME hold, and HOME held at power-on.
- OCT- held at power-on.
- Step key held + K/ALG/PRESETS/LFO.
- The first-run tour teaches only K1, ENV and HOLD HOME (views.c:2132-2147).

## 5. Constraints

- **Timing.** The HAL debounce is 2 frames for a press (about 1-2 ms) and 8 frames for a release
  (about 9 ms; fm1_input.h:107-108), so taps and a 400 ms window are safe. Holds are measured per
  UI frame (about 40 ms: panel.h:78), as HOME's 600 ms is.
- **Press-acting buttons.** FX, SCL, ENV, OCT and LFO act on press. A hold or chord on them needs a
  release-acting rule like ARP-on-LIVE or HOME, or the press's action will fire first.
- **SAVE's spoil rule** (save_key.c:6-23, views.c:2561-2580):
  - Another button press or any encoder turn spoils the save. The other control still acts.
  - A note-key press does not spoil it, so SAVE + step key still saves at release.
  - The turn of K1-K4 while SAVE is down opens learn, and the parameter does not move.
  - LAB's quick keep must be a tap. Nothing times SAVE, so a hold-SAVE "undo" would also save.
- **MIDI learn.**
  - `panel_knob_param` (panel.c:121-151) names the parameter by the panel's page. The page ids
    (panel.h:89-96) are LIVE, CHARACTER, SLOT, NONE and EXPERT.
  - `knob_page` and `shown()` special-case CHIP only (views.c:2004-2024).
  - A new view with its own K meaning reads as page NONE, and learn then toasts "SONG SETUP: NO CC"
    (learn_view.c:240).
  - Learn's frame forwards SELECT and ALGORITHM only to CHIP (learn_view.c:180-183).
  - Every LAB page needs a page id and a parameter-to-CC mapping, or its knobs cannot be learned.
- **Help cards.**
  - HOME hold needs a card per view and state (`help_card_of`, views.c:2080ff).
  - A host test requires every control the view's input handler takes to be on the card
    (help_view.c:14-20, cm:547-556).
  - Goldens exist under `tests/golden/lcd/help-*.png`. A card is 8 lines of 14 characters, 2 pages
    at most.
- **Popups.** The knob popups and LIVE's/CHIP's own flash are wired by encoder id in
  `live_frame` (views.c:2860-2905), so LAB needs its own handling.
- **Stack depth 4**, with one extra slot for the card (cm:564).
- **LIVE-side relabel costs.** Tour (`tour_watch`), the LIVE cards (help_view.c:27-33), goldens,
  README, web/index.html, docs/hardware-test.md and the emulator tests all assume ENV = next
  algorithm.

## 6. Recommendations for LAB

1. Bind ENV, LFO and FX locally in LAB, which is cheap because a view that consumes a press hides
   it from the panel. ENV = envelope page, LFO = modulation page (pulse LFO, vibrato), FX =
   effects page (slots, SCL keeps "next slot" there). Pressing the same button again returns to
   the last page. LIVE stays as it is for now, so the tour, cards and demo are untouched.
2. Give filter and pulse pages to SELECT (TONE, FILTER, PULSE, ENV, LFO, FX), which is already the
   page knob everywhere. The three buttons are shortcuts, not the only way in.
3. Optionally, from LIVE, ENV/LFO/FX open LAB at that page, which makes the labels lead everywhere.
   If that is chosen, do it as one decision that updates the tour text, the LIVE cards, the demo
   page and the hardware-test rows together.
4. Chip toggle: the profile step on ALGORITHM already moves the model (cm:615), so the toggle can
   leave FX. If a one-press toggle is still wanted, the OCT- + OCT+ chord is the one free,
   deliberate pair. FX's LED can keep showing the 8580.
5. Next algorithm moves to ALGORITHM on a slot page (already live_view.c:1195). Per-slot bypass
   becomes "turn to OFF" or a repeat press on the FX page. Losing ENV's cycle through OFF is the
   one real regression to accept or fix.
6. Quick keep is a SAVE tap with a LAB branch in `save()` (views.c:2503). Do not use hold-SAVE
   undo, since the release saves.
7. Undo reuses the existing EDIT+LFO ("ONE UNDO" in STEP and TRACKER). If LFO is a page button in
   LAB, keep EDIT+LFO as undo anyway, with a hint line. Roll = EDIT+ARP (small moves, as STEP's
   MUTATE) and full roll = EDIT+SCL (as STEP's DICE), so LAB shares STEP's vocabulary.
8. A/B compare could be an EDIT tap, which needs a release-acting spoil flag like save_key. Failing
   that, use the OCT- + OCT+ chord. Pick one, since the chord is also a candidate for the chip toggle.
9. Fix or document the EDIT+K inconsistencies (STEP held step, TRACKER REC, ORDER/INSTR/TABLE K1/K2).
   The TODO says "nothing behind hidden EDIT combinations", so LAB should not add more.
10. Plan the plumbing before the UI: panel pages and CC mapping for each LAB page (learn),
    `knob_page`/`shown()`/`help_card_of` branches, popup handling, and the cards with goldens.
    Keep PLAY, SEQ and REC global so the song keeps playing while shaping.

Open before the design: whether attack and sustain have CCs or `LIVE_PARAM_*` entries (the
envelope page depends on it).

## Disagreements between the code and docs/reference/controls.md

- cm:129 and cm:62-63 say EDIT+K1-K4 works in every view. The code breaks it in TRACKER with REC
  on, STEP with a step held, and K1/K2 on ORDER/INSTR/TABLE.
- cm:54 says SCENE takes SELECT. The code takes ALGORITHM too (scene_view.c:251).
- cm:155-156 does not list INSTR's ALGORITHM, which is free.
- cm:127 says PRESETS is the preset "in every view (TRACKER takes it with REC on)". STEP and INSTR
  take it always, and ORDER with EDIT.
