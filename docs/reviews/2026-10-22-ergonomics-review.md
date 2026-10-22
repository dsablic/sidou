<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU ergonomics review, 2026-10-22 (roadmap item 5)

Firmware and harness: main 21e2ebd1 (v0.2.0 plus docs), image 495,104 bytes. Harness run
`cargo run --release -p ergo -- --out ../build/ergo`, then again with `--baseline`.
Files, all in `/Users/denis/sidou-opencode/ergo-2026-10-22/`:

- `report.html`: the harness's own report, with screenshots and audio clips (open it from that folder).
- `findings.json`, `metrics.json`, `journeys.json`, `knobs/`, `defaults/`, `journeys/`: its data.
- `findings-vs-baseline.json`: the run made with `--baseline`; its `regressions` list is the 46 metric changes.
- `baseline-main-051a1a1.metrics.json`: the baseline copied from `docs/ergo-baseline/`.
- `extra-views/`: screens the shipped harness never visits (ORDER, INSTR, TABLE, STEP held, help over STEP, SONG menu, GLO latch, DICE), made with a scratch tour file (`extra-journey.txt`, `--only readability --journeys`), with their own `findings.json`. No harness code was changed.

The baseline folder holds only `metrics.json`, no `findings.json`, so `--compare` cannot be used against it. I compared the `findings` map inside the baseline's `metrics.json` instead.

## 1. Summary

**Findings went from 51 to 26** (high 12 to 6, medium 23 to 17, low 16 to 3). 38 ids are gone and 13 are new.
Of the 26 now reported, **21 are harness false positives** (section 3) and only **5 are real** (the CHIP B purple, the STEP
view, TRACKER's small text and contrast). The 13 new ones are 11 stale harness probes and calibration, plus 2 real ones on the
STEP view, which is new since the baseline. A harness fix is still needed so that the v0.3.0 re-run is clean (roadmap 6 says every
finding must be fixed or shown false by a harness fix).

What improved (baseline to now, harness numbers):

| Area | Before | Now |
|---|---|---|
| Time to first sound after power-on (explore journey) | 3,015 ms | 15 ms |
| Steps where the sound stops (perform / explore) | 5 / 1 | 0 / 0 |
| Back from CHIP to LIVE: forced wait | 2,800 ms | 400 ms |
| Explore journey's forced waiting | 13.8 s | 9.5 s |
| Cutoff K1 on LIVE | 128 detents, a 5-detent dead zone, 358-cent worst step, median 37 cents | 64 detents, no dead zone, 158-cent worst step, median 126 cents |
| MASTER | 6.0 dB worst step, 5-detent dead zone | 1.0 dB worst step, no dead zone |
| K3 on LIVE | 128 detents of 0.8 %, no real job on 3 presets | 15 to 31 positions, the sound's own control |
| K4 release | 3 detents (33 % a step) | 15 detents |
| EDIT-held K1 to K3 (effect slot) | dead zones of 5 | none in K1 and K2; K3 still shows one 4-detent window (not flagged) |
| Colour pairs below contrast (LIVE, menu, MIDI, SCENE) | 4, 2, 1, 1 | 0, 0, 0, 0 |
| CHIP, TRACKER, learn frame | 3, 4, 3 | 2, 2, 1 |
| Play-time text under 1.5 mm (LIVE, CHIP, MIDI) | 26, 8, 6 | 0, 0, 0 |
| TRACKER play-time text under 1.5 mm | 55 | 3 (the count is not strictly comparable: the harness now treats hex row numbers and legends as glance text) |
| Input feedback | all under 100 ms | all under 100 ms (worst 81 ms: K1 on TRACKER, MASTER) |

Got worse, honestly: opening TRACKER now takes 2 SEQ presses (STEP opens first), so "actions to first sound" in the sketch
journey went 1 to 2 and 330 to 415 ms. Opening the menu from LIVE takes 71 ms against 41 ms (still under 100). Neither is a bug; E22 is the taste call.

What is still flagged in plain words: the new STEP view, the ORDER, INSTR and TABLE views and TRACKER use 1x text that is too small and
too dim to read while playing; CHIP's B tab is purple on black; the REC and song-view flows have a stack of small bugs from the
end-of-M2 review (5.4 to 5.14) that nobody has fixed yet; and several song-editing bugs lose or misplace notes.

**Where the harness cannot see.** I looked at the screens it does not visit and found findings it never reports (E13, E17, E18).
The shipped tour has no ORDER, INSTR, TABLE, SONG menu, SAVE AS or DICE. It also cannot hear a silent band in a knob sweep
(E09), cannot judge the release knob (K4.live renders no held note) or the percussive DECAY (K3.perc: -74 dBFS in the window).
Adding those to the tour and the sweeps is part of the harness fix (F6).

## 2. Open findings (ordered by impact on a player)

Recommendation: **A** = Accept (fix before v0.3.0), **R** = Reject (reason given), **?** = Ask (taste call, options given).
Effort: S under half a day, M about a day, L several days. "Review 5.x" means `docs/reviews/2026-10-22-m2-review.md`; its line numbers are from before the v0.2.0 fixes, so `views.c` lines have since moved by about 20, and nothing in `app/` has changed since except `keys_route`.

### Lose or misplace the player's work

| ID | Where | What a player experiences | Evidence | Effort | Rec |
|---|---|---|---|---|---|
| E01 | Menu, STORE | After a downgrade, or with a store another program wrote, one fast turn of ALGORITHM on STORE formats it and the songs are gone, with no second confirmation. | Review 1.5, `app/views.c:439` (`steps > 1` arms and confirms in one event) | S | **A**. Require a separate confirming turn, and say "newer SIDOU format" apart from "foreign". **Fixed:** ALGORITHM only arms (FORMAT?, the popup SAVE FORMATS) and SAVE confirms; a newer SIDOU's store reads NEWER (NEWER SIDOU in the popup). |
| E02 | SEQ over ABOUT | Open the menu, ABOUT, then SEQ and edit: the edits go into ABOUT's built-in tune, SAVE says LOADING, and HOME HOME brings your own song back with the edits silently gone. | Review 5.5, `app/views.c:941-947`, `app/song_io.c:78-178` (`io->about`) | S | **A**. Refuse SEQ while ABOUT shows (a toast says why). **Fixed:** SEQ and the menu's TRACKER are refused while ABOUT's tune is the song (LEAVE ABOUT FIRST; LOADING while its close reads the song back). |
| E03 | TRACKER, REC while the song plays | You record a part over a song with several patterns; the notes land in the playing pattern, but TRACKER shows the cursor's pattern, so they appear nowhere. controls.md promises the notes appear. | Review 5.4, `app/tracker_view.c:119-126` against `app/song_view.c:357` | M | **A**. While recording show the voice's playing lane (the fix the review proposes). |
| E04 | REC, orderlist with a repeat | A note played late in a pattern's last row goes into the next orderlist entry instead of the repeated pattern. | Review 3.2, `app/song_view.c:382-391` | M | **A**. **Fixed:** the status's `again` (`song_ctl_again`: the next row is in the playing row's pattern) keeps a key past the last row in the repeated pattern. |
| E05 | REC, orderlist with a transpose | Record a C-4 on an entry with transpose +2 and it plays back as D-4 (the transpose is added again). | Review 3.3, `app/song_view.c:464` | S | **?** Options: (a) subtract the transpose when recording and refuse if it leaves the range; (b) leave it and say so in controls.md. I recommend (a): what you hear while recording is what should replay. |
| E06 | Parameter locks (STEP held + knob) | A lock on a step can vanish in three cases: (1) two adjacent patterns whose first rows both carry all five lock types lose two; (2) turning a locked step's knob while an INS is still pending lands the value on a row that then shifts; (3) a row boundary right after an edit drops that row's locks. | Review 3.4 (`song_ctl.h:168`, `SONG_ROW_LOCKS 8`), 3.5 (`song_ext.c:479`), 3.7 (`song_ctl.c:1393`) | M | **A**. Raise the slot count to 10, check the pending edit on the in-place path, and do not advance `row_seen` while an edit is open. **Fixed** as recommended (`SONG_ROW_LOCKS` is `2 * 5`). |
| E07 | PLAY with KEYS A | You hold a key while pressing PLAY; the note goes silent until you release it. | Review 2.2, `core/player/song_ctl.c:79-83` | S | **A**. **Already fixed** by 879d30f6 (78d155e, review 2.1/2.2): PLAY skips the taken voices' registers; `play_leaves_the_voices_the_keys_hold_sounding` (host/crates/sidou-tests/tests/song_ctl.rs) checks it and passes. Closed. |
| E08 | Profile change during a song | Changing the chip profile while a locked row plays makes the cutoff jump for up to about 20 ms. | Review 2.3, `core/player/song_ctl.c:397-404` | M | **A** (rare, but v0.3.0 ships with no known bug). **Fixed:** `song_ctl_chip_swapped` writes the row's locked registers (`song_lock_apply`) onto the new chip after the shadow. |
| E33 | MIDI minors | A SPP can cut into a SysEx reply (4.2); a FB request with no position hijacks a later panel PLAY (4.3); F8s during a flash write are stamped late (4.4); a dropped USB packet is counted twice (4.6). 4.5 (USB SysEx back-pressure) needs an upstream hook. | Review 4.2 to 4.6 | S each, 4.5 L | **A** for 4.2, 4.3, 4.4, 4.6. **?** for 4.5: either write the hook (an upstream candidate, so it follows the "adopt, then improve" rule) or document the 1 KB limit for v0.3.0. |

### How it sounds and responds

| ID | Where | What a player experiences | Evidence | Effort | Rec |
|---|---|---|---|---|---|
| E09 | LIVE, K3 on PWM BASS | Turning PULSE from thin to wide: the sound fades, goes completely silent for 4 detents (about 9 % to 14 % width), then comes back louder at the next detent. The first detents are louder than the ones after them, so the knob does not behave like a width control at its start. | `knobs/K3.pwm.json`: level -39, -41, -45, -56 dBFS, then -120 (silence) for 4 detents, then -41 and rising to -33. The harness does not flag it (its dead-zone test ignores silent points). | M | **A**. First find out whether it is the tail gate (`app/tail_gate.c`) muting a quiet note or the pulse itself, then either bound the range where it sounds or fix the gate. Also add a silent-band check to the harness (F6). **Fixed. Cause:** neither the gate nor the DAC, but the PWM LFO: `pw_effective` added +/-768 (the preset's depth) to K3's width and clamped at 0, and a width of 0 is a DC level, so below 18.75 % the note went silent for part of every 2 s cycle (fm1-emu, held A3, 47 ms windows: at 9.4 % eight to nine windows of -120 dBFS each cycle; at 3 % about 42 % of it). The sweep's silent band in the harness was the LFO's trough passing while the knob moved, not a band of widths. The depth now narrows near the ends so the sweep never goes below 3 % (or above 97 %); fm1-emu then shows no silent window at 3 %, 9.4 % or 25 %. The harness check (F6) stays with the harness fix. |
| E10 | Overall level | At full MASTER the presets peak at about -20 dBFS and sit at -26 to -30 dBFS RMS, 20 dB under full scale. SIDOU is quiet next to other line-level gear. The taper makes it easy to turn down but no knob position goes above unity. | Baseline `musical.*.peak_dbfs` -19.4 to -20.9; now -25.4 to -27.0 only because the emulated pot sits at 88 % (see F3). | M | **?** Options: (a) leave it and say in the manual "turn MASTER up"; (b) raise the chips' output gain by 6 dB (a gain stage; two chips plus effects then clip sooner; ties to TODO's soft-knee entry); (c) a MASTER BOOST setting in the menu. I recommend (b) only after hearing it on the device. |
| E11 | CHIP K2 DAC, K3 THUMP, K4 BLEED; LIVE K3 DECAY on SAW LEAD | The harness finds no measurable change over their whole travel: level moves 0.6 to 1.2 dB across 32 detents, brightness stays inside its 6 % jitter. Each has a window of 4 detents with nothing (one less than the dead-zone limit, so it is not flagged). | `knobs/K2.chip.json`, `K3.chip.json`, `K4.chip.json`, `K3.live.json` (decay on a held note is not audible after the note reaches sustain, so that one is partly the measure's fault) | S to measure | **?** Listen on the device at flash day. If a player cannot hear them, widen the ranges; if only transients change, say so on the card. Do not change anything blind. |

### Readability and what the screen says

| ID | Where | What a player experiences | Evidence | Effort | Rec |
|---|---|---|---|---|---|
| E12 | STEP (the first song view SEQ opens) | The step numbers 1 to 16 are tiny (0.78 mm), black on light red (4.42:1), grey on dark grey (3.26:1), and the knob legend "K1 CUTOFF K2 RESO" is dark grey on black (2.16:1). 15 of 16 play-time texts are under the 1.5 mm minimum. | `READ.size.step`, `READ.contrast.step`; `journeys/keep/04-fresh.png` | M | **A**. Numbers at 2x on the pads (the pads are large, so the space is there), the `hint` role for legends. |
| E13 | ORDER, INSTR, TABLE | ORDER's "PATTERN" labels and the pattern numbers are small and dim (3.26:1 and 3.63:1). The numbers 00 to 09 are 1x. TABLE's column hint and the "WAVE 21 REL 00" line are 1x. INSTR's three-line legend at the bottom is cramped against its last row. | `extra-views/findings.json` (READ.size.song_pages_order high, READ.contrast.song_pages_order high, READ.size.song_pages_table medium); `extra-views/01-order.png` to `03-table.png`. The shipped harness does not visit these views. | M | **A**. Add them to the tour (F6), then bring them to the same roles as TRACKER. |
| E14 | TRACKER and every song view | "BPM" and "OL" are dark grey on black (2.16:1). The instrument number on the current-row bar is light red on dark grey (2.05:1). Three play-time texts are under 1.5 mm. The learn frame's flagged pair is this same pixel (see F5). | `READ.contrast.tracker`, `READ.size.tracker`; `extra-views/findings.json` (BPM in ORDER, INSTR, TABLE, STEP) | S | **A**. The plan's Later-1 step 1: `BPM`, `OL` and the hint line in `hint`, the instrument and command on the bar in `value`. One shared fix for all song views. |
| E15 | TRACKER | The play state is a tiny mark; keys auditioning show nothing on the row; the two voices you are not editing show only as strips, so a muted voice is hard to tell. | Plan "Later-1" steps 2 to 4 (`HEUR.tracker.play-state-tiny`, `.audition-no-feedback`, `.mute-other-voice-quiet`). The harness no longer reports these ids; the plan owns them. | M | **A**. The plan has the exact spec (play mark at 2x; ghosted note and instrument on audition; all three voice numbers on row 2). |
| E16 | CHIP, tab B | Chip B's model name and "MODEL" label are purple on black (2.60:1 at 2x and 3x), so the 3580 and the first letters are hard to read. | `READ.contrast.chip`; `journeys/tour/22-chip-tab-b.png` | S | **A**. A lighter purple for text; the raster behind can stay. |
| E17 | SAVE AS, and the SONG file pages | The key legend on the name editor ("SELECT CURSOR / ALGORITHM CHARACTER / SAVE WRITE SONG") is 1x grey, and the title "SAVE AS" is dim grey at 2x. You see this page once per new song, with both hands on the encoders. | `journeys/build/06-save-song.png`, `extra-views/09-menu.png`. Not measured by the harness. | S | **A**. Same `hint`/`label` roles as the menu; add the pages to the tour. |
| E18 | LIVE, band under the knobs | With a 5-letter filter name (NOTCH) the name runs into the profile: it reads "NOTCHR4AR". Seen on DICE 01, which picked a notch filter. | `extra-views/14-dice.png` (DICE 01) | S | **A**. Shorten the filter word or move the profile one column. |
| E19 | LIVE, "TONE" row and the knob tags | The heading TONE sits directly on top of the knob labels (CUTOFF, RESO, DECAY, RELEASE) with no gap, so it reads as one block; "K1 K2 K3 K4" tags and the DIST/MOD/FX tabs are 1x. The harness passes this screen (0 failing pairs, no text under minimum). | `journeys/tour/01-live-idle.png`, `extra-views/13-glo.png` | S | **?** Options: leave as it is for the panel's verdict at flash day (the TODO already says the 8x8 small text waits for that), or drop the TONE heading since the tabs show the page. I recommend deciding from the device. |
| E20 | ABOUT | The credits fade a row at a time and cut off mid-word in a still frame ("ONG F", "S AVE"); the first page's heading does not fit at 2x. | `journeys/tour/13-about.png`; TODO says "left for the panel's verdict" | S | **?** Options: (a) leave; (b) rework the credit scroller to whole rows. Nobody plays with ABOUT open, so I would only fix the heading. |
| E21 | CHIP and LIVE band, the profile name | R2, R3, R4AR, R5, WORN mean nothing to a new player. | Plan "Later-2" (`HEUR.chip.profile-jargon`) with record 0006's words: BRIGHT, DARK, CLASSIC, CLEAN, GRITTY, USER | M | **A**. Spec is written. |

### Flow and small bugs the review deferred (5.7 to 5.14)

| ID | Where | What a player experiences | Evidence | Effort | Rec |
|---|---|---|---|---|---|
| E22 | SEQ | TRACKER takes two SEQ presses (STEP first), ORDER three, INSTR four, TABLE five, then round; from the menu TRACKER takes two. | `task.open-tracker.sketch.actions` 1 to 2; `journey.sketch.actions_to_first_sound` 1 to 2 | S | **?** Options: (a) keep as is: STEP is the easy first view; (b) SEQ re-opens the song view you used last, STEP the first time. I recommend (b): a player who lives in TRACKER presses SEQ once. **Done (b):** SEQ from outside the song views reopens the one that showed last, STEP the first time after power-on; in them it steps as before. Kept in RAM only: the settings record has no similar field, and one would need a schema 6. |
| E23 | REC on LIVE | REC toggles with no sign outside the song views, and on LIVE a COUNT-IN toast can appear though nothing records. | Review 5.7, `app/song_view.c:253` | S | **A**. **Fixed:** outside the song views REC toasts REC ON / REC OFF and starts no count-in. |
| E24 | Count-in | Opening a song view while the song already plays starts a count-in. | Review 5.8, `app/song_view.c:523-534` (`was_playing`) | S | **A**. **Fixed:** entering a song view reads `was_playing` afresh (`song_view.shown`): a song that already plays counts nothing in. |
| E25 | REC knob popup | K1 while recording says `CUTOFF 1024` (a raw lock value) where every other view says Hz; during the count-in K1 opens a popup though it does nothing. | Review 5.11, `app/tracker_view.c:601-618`, `app/step_view.c:986-992` | S | **A**. **Fixed:** the popup says the cutoff in Hz (`live_view_fc_text`); during a count-in TRACKER's K1/K2 do nothing and open no popup, STEP's act as in LIVE. |
| E26 | ORDER then TRACKER | Passing through ORDER puts TRACKER's cursor back on row 0. | Review 5.9, `app/song_pages.c:199` | S | **A**. **Fixed:** ORDER's cursor starts on the song views' entry, and the row goes to 0 only when the entry or subtune changes. |
| E27 | ABOUT | Looking at ABOUT resets the song views (lanes, instrument, cursors). | Review 5.10 (`main.c` load path, `views_song_loaded`) | S | **A**. **Fixed:** ABOUT's tune and the song coming back are returning reads (`song_io_returning`); `views_song_back` keeps the cursors, instrument, subtune, STEP's lanes and page and the pages' cursors. |
| E28 | Help card over a song view | Notes you play under the card sound with LIVE's instrument and are not recorded. | Review 5.14 | S | **A**. **Fixed:** `views_note_keys` routes the keys by the view under the card. |
| E29 | Help card and toasts | A toast other than the tour's draws over the card; a frame can reach about 290 cells against the 200 budget. | Review 5.12, `app/help_view.c:110` | S | **A**. Hold toasts while the card shows. **Fixed:** a toast raised under the card waits (`screen_toast_take`, `views.toast_held`) and shows after it; the card's budget counts the layers' cells. |
| E30 | First-run tour | Pressing ENV within 8 s of an install boot: "DIST TAPE" never shows (the tour's clear erases it). | Review 5.13, `app/views.c:1805-1832` (`tour_frame`) | S | **A**. **Fixed:** the tour ends only its own toast (`tour_clear`). |
| E31 | Menu | 17 lines. THEME is the 12th, below KEYS, SOUNDS, ROWS, CARDS and STORE; set-and-forget settings and actions share one list. | `journeys/tour/07-menu.png`; the harness could not measure CHANGE THEME at all (F2) | M | **?** Options: (a) keep one list; (b) a SETTINGS item that holds KEYS, SOUNDS, ROWS, CARDS, STORE, THEME, REC (the menu then has about 11 lines and the TODO's "menus hold only set-and-forget settings" is clearer). I recommend (b), but it moves things the help card and docs name. |
| E32 | Undo | One step (EDIT + LFO) on song cell edits only, no undo on STEP's pads. | controls.md "EDIT + LFO", review "Editing TRACKER"; TODO "undo where it matters" | L | **?** Options: keep one step (a row is 200 bytes of RAM per undo level), or add a second level. I recommend keeping one step for v0.3.0 and telling players on the help card. |

### Rejected

| ID | Where | What a player experiences | Evidence | Effort | Rec |
|---|---|---|---|---|---|
| R1 | Menu names PRE, ROWS, REC | Short names for PRESET, TRACKER ROWS, REC QUANTISE. | `app/views.c:955-958` comment: the 14-character lines hold the longest value (NOISE PERC) beside them, and the popup says the full name | - | **R**: width-bound by design; the popup already spells them out. |
| R2 | Toasts over CHIP's footer legend and LIVE's band | A one-second toast covers part of a legend. | `journeys/tour/20-chip-dist.png`, `05-live-toast.png` | - | **R**: transient, and it names what you just changed. |
| R3 | K1 on LIVE, lowest 9 of 64 detents | Brightness moves only 4 % over the first 9 detents (cutoff 253 to 364 Hz). | `knobs/K1.live.json` | - | **R**: a closed filter passes only the fundamental; the table is even in pitch on purpose. |
| R4 | K2 resonance, lowest third | The first 5 of 15 detents change little at the default cutoff. | `knobs/K2.live.json` | - | **R**: resonance is heard where the cutoff meets a partial; a sweep on one note at one cutoff understates it. |
| R5 | TODO "suggested automations" | Toast offers such as "record this knob move as locks?" | `docs/TODO.md` "A UX pass" | L | **R** for v0.3.0: a feature, not a finding; revisit after the release. |
| R6 | TODO "a friendlier ARP editor in TRACKER" | Arpeggios are raw wavetable rows. | `docs/TODO.md:209` | L | **R** for v0.3.0: a feature; GoatTracker files still work. |

### Not in this list

The review's non-UX items (2.4, 2.5, 3.6, 6.x, 7.x, the stale-CMake check) change no screen or gesture; they stay on the v0.3.0 list in `docs/TODO.md` and the roadmap. Nothing is recorded yet in `docs/hardware-test.md` from players on a device; flash day (roadmap 4) is where E10, E11, E19 and E20 get their verdicts. I confirmed the rest of the ergonomics plan's coverage table gone by the harness's numbers or by screens I looked at (profile on LIVE, `*` mark on an edited preset, GLO strip, help cards, the new menu order, the idle SCENE flame). Not looked at on screen: EDIT-held relabelling of the knob row and DICE beyond one screen (E18).

## 3. Harness false positives (21 of the 26 reported ids)

| ID | Ids | Why it is false | Fix |
|---|---|---|---|
| F1 | `JOUR.nyp.build.arrange`, `.build.edit-instr`, `.build.to-instr`, `.build.to-order`, `.sketch.enter-notes`, `.keep.undo`, and `SAFE.undo` (7) | The firmware has them all now (M2d). The feature probes look for old names and strings. ORDER and INSTR exist as `song_pages_order_view` and `song_pages_instr_view`, but `detect_features` (`journeys.rs`) looks for `order_view_view` and `instr_view_view`. Undo exists (EDIT + LFO, `app/song_edit.c`) but is probed as the string "UNDO", which the screen never shows. The pattern-edit probe (`probe_pattern_edit`) reads rows that start with two hex digits and a space, but TRACKER's rows are now separate 2x runs, so it reads only the orderlist and sees no change (ERGO_DEBUG shows `before 05 06 / after 05 06`). The keep journey's undo step presses ARP; it should hold EDIT and press LFO. | Probe by symbol (`song_pages_*`), by behaviour for undo, and read the new row layout. Make the journeys use the real gestures. |
| F2 | `HARN.nav.keep.theme`, `TASK.fail.change-theme.keep`, `HARN.nav.build.back`, `JOUR.dead.build.save-song` (4) | The journeys are out of step with the firmware, not the other way round. "Start a new song" presses SEQ, which leaves STEP open; the next step presses HOME, which goes back one view instead of opening the menu, so THEME is never reached (the screen is LIVE). "Save the song" presses SAVE on an unnamed song; that opens SAVE AS by design (controls.md "SAVE"), not a toast; then "Back to LIVE" presses HOME once, which only closes SAVE AS. | The `keep` journey goes back to LIVE first; the `build` journey expects the SAVE AS page, writes a name, saves, then goes home. |
| F3 | `MUS.quiet.*`: pwm-bass, saw-lead, sync-lead at A4 and F3 (6); noise-perc A4 and F3 (2) = 8 | The emulated MASTER pot reads 900 of 1023 (`emu-gate/tests/common/mod.rs:691`). Since `4d640237` the pot is an audio taper (-60 dB to 0), so 88 % is -7.2 dB where the old linear pot gave -1.1 dB: every level dropped by 6.1 dB (a uniform 6.1 dB drop in every `musical.*` rms and peak metric). Add 6.1 dB and every one of these is inside the -30 to -8 dBFS window (noise-perc A4 -26.5, PWM BASS -26.5, SAW LEAD -28.3, SYNC LEAD -27.7). | Measure at the pot's top (or at the taper's unity). The real question is E10. |
| F4 | `CONS.drift.tracker.algorithm` (1) | controls.md says ALGORITHM changes TRACKER's voice (Panel table and Song views). The harness's "documented idle" table (`consistency.rs:102`) still lists TRACKER's ALGORITHM as idle. | Remove that row. |
| F5 | `READ.contrast.learn` (1) | The flagged pair is the instrument number "02" on TRACKER's current-row bar, drawn under the learn frame; the frame's own text passes. Same pixels as `READ.contrast.tracker`, so one fix (E14) clears both. | Merge the two, or mask TRACKER's rows under the frame. |

Also, not false positives but gaps in what the harness measures (fix them with F1 to F5 in one harness commit, F6): it never visits ORDER, INSTR, TABLE, the SONG pages, SAVE AS, a held STEP or DICE; it ignores silent points in a knob sweep (E09); it cannot score release (K4.live: every level -120 by design) or percussive decay (K3.perc: -74 dBFS); and its dead-zone test needs more than 4 detents, so a 4-detent window shows only in `metrics.json`.

## 4. Counts and the fix list

- Open findings: 33 rows in section 2 (E01 to E33; R1 to R6 are the rejected ones, counted separately).
- **Accept: 25.** E01 to E04, E06 to E09, E12 to E18, E21, E23 to E30, E33 (the MIDI minors except 4.5).
- **Ask: 8.** E05, E10, E11, E19, E20, E22, E31, E32 (E33's 4.5 is a ninth sub-question).
- **Reject: 6.** R1 to R6.
- **Harness false positives: 21 ids in 5 groups** (F1 to F5), plus one harness commit (F6) for the gaps.

Suggested order if you accept: E01, E02, E03 and E04 first (work can be lost), then E09 (understand it before the flash day), then the readability bundle E12 to E17 with the harness fix F6 so the new screens stay gated, then the 5.x batch E23 to E30 (all S).

## The user's decisions (2026-10-23)

- **E22:** SEQ reopens the song view used last (STEP the first time). **Done.**
- **E31:** a SETTINGS item holds KEYS, SOUNDS, ROWS, CARDS, STORE, THEME and REC.
- **E03:** while REC records, TRACKER follows the voice's playing pattern; the cursor comes back
  when recording stops.
- **E05:** REC writes what was heard: the entry's transpose is subtracted, with a toast if the
  note leaves the range.
- Taken with the review's recommendation, not asked: **E20** fixes ABOUT's first heading only;
  **E32** keeps one undo step and says so on the help card.
- From the device session: E10, E11, E19, the readability calls in TODO.md, and the SysEx gap
  while sound plays.

## From the device session (2026-10-23)

Run on the FM-1 with the user, on the sysex-speed build (`af7382f6`):

- **Passed:** K1 CC 74 and K4 CC 72 over USB, also after a replug; the OCT- gesture into ROM
  boot (`WL80UBOOT1.00`) and a normal boot after; CHIP B's names readable; PWM BASS's K3 never
  drops out at narrow widths (E09 confirmed); a 4 KB SysEx round trip with nothing held.
- **E10:** loud enough; the level stays.
- **E11:** DAC, THUMP and BLEED are inaudible: widen their ranges until a player hears them
  (measured, within the reSIDfp reference tolerances at their defaults).
- **The 1x text at the bottom of the views is too small to read:** drop it (the user's choice
  b); each view's help card (HOME held) carries what it said.
- **STEP's pads:** numbered by beat, 1 2 3 4 | 1 2 3 4 | …, alternate beats shaded, each
  beat's first step brighter; the exact step number shows in the popup when a step is held.
- **Bug: an upload while panel keys are held is refused with BUSY** (pool 22, every try), but
  works at once with nothing held. Hardware-test 8c says it must be accepted; the emulator's
  "chord sounding" test passed, so it does not hold panel keys the way the device does.
- **Opening a menu item** by turning ALGORITHM clockwise was not discoverable: the user had to
  ask. Consider a key press too (the menu work, E31).
