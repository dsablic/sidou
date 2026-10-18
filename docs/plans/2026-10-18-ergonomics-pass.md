<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The ergonomics pass

**Status:** written 2026-10-18 from the harness run on firmware 64b6f97 with harness f931433
(`build/ergo/findings.json`, `build/ergo/review.json`) and the heuristic review. The user
accepted every priority (P01-P12) and every real finding, with the review's fixes unless the
code shows a better way. Where this plan improves on a proposed fix it says why.

**Goal:** every finding of that run is fixed, shown to be a false positive by a harness fix, or
deferred here with its spec (TRACKER and CHIP items that wait for M2d Tasks 3-4 and M2b Task 6).
After the merges, `ergo --compare` lists every id below as gone except the deferred ones.

**Read first:** `docs/plans/IMPLEMENTER-BRIEF.md`, `/Users/denis/sidou-opencode/COMMON.md`,
`docs/reference/controls.md`, `docs/art/README.md` ("Theme", "LIVE", "CHIP"), `app/theme.h`,
`app/views.h`, `app/screen.h`, and your track's files below.

## Order, preconditions and merges

Five tracks, each in its own worktree and branch (`ergo-theme`, `ergo-live`, `ergo-sound`,
`ergo-flow`, `ergo-harness`). Files are split so that merges are mechanical; where two tracks
must touch one file, the [file ownership](#file-ownership) table names the functions each may
change.

1. **Phase 0.** `ergo-harness` (host only) and `ergo-theme` Task T1 (the theme roles) start
   from main. T1 is small; the controller merges it as soon as it is green.
2. **Phase 1**, from main with T1 merged: `ergo-theme` continues with T2-T6; `ergo-live`,
   `ergo-flow` and `ergo-sound` (S1, S2) start. Their first step checks that `app/theme.h` has
   the `hint` role; without it they stop and report.
3. **Phase 2:** `ergo-sound` S3 (DICE) starts after `ergo-live` has merged (the controller
   rebases `ergo-sound` onto main and tells the agent to continue).
4. **Merge order:** `ergo-harness` first (the others' journey edits use its flags), then
   `ergo-theme`, `ergo-flow`, `ergo-live`, `ergo-sound`. On a conflict in `CHANGELOG.md`,
   `docs/hardware-test.md`, `views_init`, the `views` struct's tail or a golden PNG, keep both
   sides and regenerate the golden on the merged tree.
5. After the last merge: the controller runs `cargo run -q --release -p ergo -- --out
   ../build/ergo-after` and `-- --compare ../build/ergo/findings.json
   ../build/ergo-after/findings.json`; every id in the coverage table but the `later` and
   `no work` ones must be listed as gone.

**In-flight work this plan does not own.** M2b Task 6 (`/Users/denis/sidou-m2b`, uncommitted:
`chip_view.c`, `live.c`, `views.c`, `settings.c`, `panel.c`, `learn_view.c`, `keys_route.c`),
M2c Task 8 (branch `m2c`: `views.c`, `settings.c`, `keys_route.c`, `chip_view.c`, the menu's
SONG item, settings schema 4) and M2d Tasks 3-4 (`/Users/denis/sidou-m2d`: `tracker_view.c`,
`views.c`, `controls.md`) touch the same files. Starting the ergonomics tracks after those
merge saves three-way merges; the specs below hold either way, and say what to do where the
base differs (the menu's SONG item, TRACKER's controls).

**Settings schema:** no track bumps it. CARDS keeps its byte with a third value (F1);
everything else new is session state or an existing parameter. (M2b Task 6 and M2c Task 8
both take schema 4 in their trees today; one of them must become 5 when they merge.)

## Coverage

Every id of the run (`DATA.items` of the review page, plus the seven the harness fix in
f931433 already removed). P is the review's priority.

| Id | P | Owner | Fixed by |
|---|---|---|---|
| HEUR.live.knob-row-collision | P02 | ergo-live | L2: values at most 3 characters at 2x (`7K2`) |
| HEUR.live.edit-layer-invisible | P02 | ergo-live | L2/L3: the knob row relabels to the slot's controls while EDIT is held or on a slot page |
| CONS.k-meaning | P02 | ergo-harness | H9: a view that labels the parameter K1 turns is consistent |
| READ.contrast.chip | P03 | ergo-theme | T1 roles, T2 CHIP tabs/legends/help; the purple pair is the raster, H2 |
| READ.contrast.learn | P03 | later | the pixels are TRACKER's under the frame: Later-1 |
| READ.contrast.live | P03 | ergo-live | L2: knob labels in `knob_text`, K tags and meter labels in `hint` (T1's roles) |
| READ.contrast.tracker | P03 | later | Later-1 (`tracker_view.c` is M2d's now); T1's `label` change fixes the grey numbers already |
| READ.contrast.midi | P03 | ergo-theme | T3: the MIDI page on the menu renderer |
| READ.contrast.menu | P03 | ergo-theme | T1: C64's menu roles |
| READ.contrast.status | P03 | ergo-theme | T1: `label` light grey |
| HEUR.nav.title-cards-tax | P04 | ergo-flow | F1: cards once per power-on, none back to LIVE |
| TASK.wait.get-back-to-live-from-chip.explore | P04 | ergo-flow | F1 + the explore journey's chip-back wait |
| TASK.wait.open-chip-and-change-the-profile.explore | P04 | ergo-live | L3: the profile is changed on LIVE (ALGORITHM); the task leaves CHIP's steps |
| HEUR.live.idle-encoders | P05 | ergo-live | L3: PRESETS, SELECT, ALGORITHM act on LIVE |
| HEUR.nav.chip-two-levels | P05 | ergo-live | L3: ALGORITHM on LIVE steps the profile |
| CONS.inert.algorithm | P05 | ergo-live | L3 (LIVE); S2 gives SCENE's ALGORITHM a job; H8 exempts the read-only views; TRACKER's from M2d Task 3 |
| CONS.inert.select | P05 | ergo-live | L3 (LIVE); H8 exempts the read-only views |
| JOUR.hands.explore.drive | P05 | ergo-live | L3: SELECT pages LIVE's knob row to the slot, one hand; the journey step uses it |
| TASK.effort.open-chip-and-change-the-profile.explore | P05 | ergo-live | L3: one action on LIVE |
| HEUR.live.k3-dead-on-three-presets | P06 | ergo-live | L3: K3 is the sound's own control (PULSE, RATIO, DECAY) |
| KNOB.detents.K3.live | P06 | ergo-live | L3: PULSE 31 positions, DECAY 16 |
| KNOB.fine.K3.live | P06 | ergo-live | L3: PULSE 1.6 % a detent, one side only |
| KNOB.mono.K1.live | P07 | ergo-live | L1: the per-model monotonic cutoff table |
| KNOB.detents.K1.live | P07 | ergo-live | L1: 64 detents |
| KNOB.fine.K1.live | P07 | ergo-live | L1: about a semitone (100 cents) a detent |
| KNOB.uneven.K1.live | P07 | ergo-live | L1: log-spaced targets |
| KNOB.jump.MASTER | P07 | ergo-live | L1: the -60..0 dB taper (with H7's -40 dB floor for the measure) |
| KNOB.uneven.MASTER | P07 | ergo-live | L1: even dB steps |
| KNOB.fine.MASTER | P07 | ergo-harness | H7: a pot has no detents; the per-detent rule does not apply |
| HEUR.master.coarse-readout | P07 | ergo-live | L1: VOLUME in percent, the bar following it |
| HEUR.live.popup-hides-what-you-tune | P08 | ergo-live | L2: no popup on LIVE and CHIP for what they show; a highlight; a toast replaces a popup |
| HEUR.sound.preset-edits-lost | P09 | ergo-live | L4: an edit buffer per preset, `*` mark |
| SAFE.destructive.jam.next-preset | P09 | ergo-live | L4 + the jam journey's back step; H10 reads it as recoverable |
| SAFE.undo | P09 | no work | M2d Task 3's undo (EDIT + LFO) puts UNDO in the image, which is what the check reads |
| JOUR.nyp.keep.undo | P09 | no work | M2d Task 3 |
| HEUR.perform.glo-layer-hidden | P10 | ergo-live | L5: the GLO overlay, the song strip's badges, GLO latch |
| HEUR.live.song-invisible | P10 | ergo-live | L5: the play mark, beat and BPM on LIVE |
| JOUR.hands.perform.mute-voice-2 | P10 | ergo-live | L5: GLO latch; the journey latches and taps |
| JOUR.hands.perform.unmute-voice-2 | P10 | ergo-live | L5 |
| JOUR.hands.perform.solo-voice-1 | P10 | ergo-live | L5 |
| JOUR.hands.perform.unsolo-voice-1 | P10 | ergo-live | L5 |
| HEUR.midi.page-old-style | P11 | ergo-theme | T3 |
| HEUR.learn.frame-1x-and-residue | P11 | ergo-theme | T4 |
| HEUR.menu.stale-rows-on-spin | P11 | ergo-theme | T5 |
| HEUR.live.octave-small | P11 | ergo-live | L2: the octave at 2x when not 0 |
| READ.size.midi | P11 | ergo-theme | T3 |
| READ.size.live | P11 | ergo-live | L2: the octave and the selected slot's algorithm at 2x; H2 reclassifies legends and the model word |
| READ.size.learn | P11 | ergo-harness | H2: `learn` is glance; the frame itself is T4's |
| HEUR.first-run.no-help | P12 | ergo-flow | F2 help cards on HOME held, F3 the first-run tour |
| CONS.drift.tracker.select | P12 | ergo-harness | H8: the documented-idle table was wrong |
| READ.size.tracker | P12 | ergo-harness | H1 role-based play text, H2 hex row numbers |
| READ.size.chip | P12 | ergo-harness | H1, H2: legends are hint text |
| HEUR.fx.silent-by-default | P01 | ergo-sound | S1: a musical starting point per algorithm |
| KNOB.dead.K1.edit | P01 | ergo-harness | H4: Mix up before sweeping, unused controls skipped |
| KNOB.dead.K2.edit | P01 | ergo-harness | H4 |
| KNOB.dead.K3.edit | P01 | ergo-harness | H4 |
| HEUR.fun.no-happy-accidents | - | ergo-sound | S3: DICE |
| HEUR.scene.fire-dark-in-silence | - | ergo-sound | S2: an idle flame, the piece's name on entry |
| HEUR.menu.scene-buried | - | ergo-theme | T6: the menu's order |
| READ.size.status | - | ergo-harness | H2: STATUS is glance (rule order) |
| JOUR.hands.perform.learn-cc | - | ergo-harness | H3: set-up steps may take two hands |
| MUS.quiet.noise-perc.a4 | - | ergo-harness | H5: a percussive preset is judged by its attack |
| MUS.quiet.noise-perc.f3 | - | ergo-harness | H5 |
| MUS.bright.sync-lead.a4 | - | ergo-harness | H6: a lead's ceiling is 6,500 Hz |
| HEUR.tracker.play-state-tiny | - | later | Later-1 |
| HEUR.tracker.audition-no-feedback | - | later | Later-1 |
| HEUR.tracker.mute-other-voice-quiet | - | later | Later-1 (LIVE's badges are L5's) |
| HEUR.chip.profile-jargon | - | later | Later-2 |
| HEUR.harness.menu-nav-blind | P12 | done | f931433 |
| JOUR.dead.explore.scene, JOUR.dead.explore.scene-next | P12 | done | f931433 |
| CONS.drift.about.algorithm, CONS.drift.scene.algorithm, CONS.drift.status.algorithm, CONS.drift.midi.select | P12 | done | f931433 |
| TASK.effort.change-theme.keep | P12 | done | f931433 |
| JOUR.nyp.build.arrange, .build.edit-instr, .build.to-instr, .build.to-order, .sketch.enter-notes, .keep.fresh | - | no work | M2d Tasks 3-7 |
| JOUR.nyp.build.save-song | - | no work | M2c Task 8 |

The threshold note on `max_detents` and encoder acceleration needs no work: nothing
accelerates yet.

## Budgets

**Measured on main** (a123730; its firmware is 64b6f97's), from `build/fm1/platform/fm1/sidou_app.elf`
the way `song.rs`'s `the_song_region_and_the_free_ram` counts:

| | Used | Free | Floor |
|---|---|---|---|
| RAM region (.data + .bss + .song_ram) | 59,740 of 65,536 | 5,796 | 4,096 a region |
| POOL | 346,560 of 352,256 | 5,696 | 4,096 a region |
| RAM + POOL | | 11,492 | FREE_RAM_MIN 7,000 |
| RAMTEXT | 45,108 of 49,152 | 4,044 | |
| Image | 425,140 | 86,860 | ceiling 512,000 |

**The in-flight M2 work already spends this.** Today's builds in the M2 worktrees, against
main: M2b +1,408 B RAM region, +1,224 B POOL, +812 B RAMTEXT, +40,844 B image; M2c +1,680 B
RAM region, +108 B RAMTEXT, +29,144 B image; M2d +272 B RAM region, +124 B RAMTEXT, +6,224 B
image. Together: the RAM region would have about 2,436 B free (1,660 under its 4 KiB floor),
RAM + POOL about 6,908 (92 under FREE_RAM_MIN), RAMTEXT about 46,152, the image about
501,352 (10,648 under the ceiling). The RAM must be won back before M2's last merge, and this
plan's budgets need 448 B more in the RAM region on top: that is the M2-end review's problem
to solve (one canvas shared by the views that never show together is the obvious source).
This plan does not lower FREE_RAM_MIN or any floor.

**Per track** (each measured against its own base; a track over its budget makes its code
smaller, as COMMON.md says):

| Track | RAM region | POOL | RAMTEXT | Image |
|---|---|---|---|---|
| ergo-theme | 32 | 0 | 0 | 1,200 |
| ergo-live | 256 | 0 | 0 | 4,500 |
| ergo-sound | 64 | 0 | 0 | 1,800 |
| ergo-flow | 96 | 0 | 0 | 2,000 |
| ergo-harness | 0 | 0 | 0 | 0 |
| **All** | **448** | **0** | **0** | **9,500** |

Nothing this plan adds runs in the audio interrupt (`master_gain_of` runs in the main loop,
`platform/fm1/audio.c` `audio_set_master`), so RAMTEXT stays as it is.

**Instruction gates** (emu-gate; they must pass unchanged on every track and on the merged
tree; the shares add up to what is left):

| Gate | Now | Bound | theme | live | sound | flow |
|---|---|---|---|---|---|---|
| STATUS idle (screen.rs, instructions a frame) | ~7,469 | 7,500 | 0 | +10 | 0 | +10 |
| HOME (TITLE_CARD_INPUT, the input that navigates) | 426 | 430 | 0 | 0 | 0 | 0 or less |
| CHIP K1 (CHIP_INPUT) | | 215 | 0 | 0 | 0 | 0 |
| SELECT into RULE 30 (SCENE_INPUT) | 227 | 230 | 0 | 0 | 0 or less | 0 |
| K1 popup input over LIVE/STATUS/menu/SCENE (POPUP_INPUT) | 218 | 250 | 0 | +30 | 0 | 0 |
| LIVE noise (LIVE_NOISE_PER_FRAME) | ~154,400 | 155,000 | 0 | +450 | 0 | +20 |
| USB drain (usb.rs/midi.rs worst pass) | 7,991 | 8,000 | 0 | 0 | 0 | 0 |
| MIDI floods | | within 1 % | 0 | 0 | 0 | 0 |

How the shares are kept: per-frame work goes behind one byte test (`if (v->tour)`), or into a
path that runs only while LIVE shows; nothing is added to the MIDI input path; the popup's
replacement on LIVE and CHIP happens in the frame, not in the input. Measure each gate on a
build of your base (a scratch worktree under `build/scratch/`) and on your branch, and put
both numbers in the report.

## File ownership

A track changes only its own files, or, in a shared file, only the functions named for it.
Everything not listed belongs to nobody in this pass: do not change it.

| File | ergo-theme | ergo-live | ergo-sound | ergo-flow |
|---|---|---|---|---|
| `app/theme.h`, `app/theme.c` | all | | | |
| `app/widget.h`, `app/widget.c` | all | | | |
| `app/learn_view.h` | `LEARN_VIEW_*` defines | | | |
| `app/learn_view.c` | `lines`, `compose`, `finish`, `look` | `pick` (the no-CC toast) | | |
| `app/chip_view.h` | | new fields at the struct's end, `chip_view_flash` | | |
| `app/chip_view.c` | `draw_tabs`, `draw_control`, `draw_names`, the help-line call in `build` | `compose` (one call after `build`), new `highlight` and `chip_view_flash` | `input` (the slot's ALGORITHM line) | |
| `app/views.h` | `VIEWS_MENU_*`, `VIEWS_MIDI_*` | fields at the struct's end | | fields at the struct's end, `views_boot` comment |
| `app/views.c` | `MENU_ITEMS` and its assert, `menu_input`, `menu_compose`, `midi_*`, new `big_menu_compose`, the two `menu_init` lines in `views_init` | `preset_turn`, `param_text`, `popup_text`, `tell`, `views_encoder`'s SAVE branch, new `live_frame` (one call in `views_frame`), a GLO branch at the end of `global` (new `glo_tap`), the VOLUME line of `status_compose` | | `cards_value`, `cards_turn`, `show_card`, `global`'s HOME branch, `curtain`, `views_boot`, new `flow_frame` (one call in `views_frame`), new `home_go`, its lines in `views_init` |
| `app/live.h`, `app/live.c` | | presets, knobs, edits, master, tone, `load_preset`, `live_set_preset`, `live_knob`, new functions | `live_slot_*`, new `live_slot_pick`; in S3 the DICE additions | |
| `app/live_view.h`, `app/live_view.c` | | all | | |
| `app/live_cutoff.c` (new) | | all | | |
| `app/panel.h`, `app/panel.c` | | all | | |
| `app/master_gain.h`, `app/master_gain.c` | | all | | |
| `app/midi_out.c` | | `look` (recall) | S3: `look` (DICE) | |
| `app/screen.c` | | `screen_toast` | | |
| `app/fx_defaults.c` (new) | | | all | |
| `app/live_dice.c`, `app/live_dice.h` (new) | | | all (S3) | |
| `app/scene.c`, `app/scene_view.c`, `app/scene_view.h` | | | all | |
| `app/help_view.c`, `app/help_view.h` (new) | | | | all |
| `app/settings.c`, `app/settings.h` | | | | the CARDS byte |
| `app/CMakeLists.txt`, `host/crates/sidou-sys/build.rs` (SOURCES), `wrapper.h` | | add `live_cutoff.c` | add `fx_defaults.c`, `live_dice.c` | add `help_view.c` |

`app/tracker_view.c` belongs to nobody here (M2d edits it); its findings are Later-1.

**Docs, tables and tests that several tracks edit:**

| File | ergo-theme | ergo-live | ergo-sound | ergo-flow | ergo-harness |
|---|---|---|---|---|---|
| `docs/reference/controls.md` | "The MIDI page"; the Panel row "ALGORITHM, in the menu" (final text below); "Screen"'s menu paragraph | Panel rows K1-K4, MASTER, EDIT, GLO, the new SELECT/ALGORITHM/PRESETS on LIVE rows, the sentence after the table; "Value popup"; "Song views"' paragraph on LIVE's top line; "Presets" first paragraph; "Chip character"'s MASTER paragraph; "MIDI learn"; "Screen"'s LIVE paragraph | "Effect slots" (defaults); the ENV row; "SCENE"; a new "DICE" subsection after "Presets"' Known limit | "At power-on" (the tour); the HOME row; "Routing" item 2; "Title cards"; a new "Help" section after "Title cards" | |
| `docs/art/README.md` | "Theme", "CHIP" (tabs), the menu | "LIVE" | "SCENE" FIRE | the help card | |
| `docs/reference/settings-format.md` | | | | byte 1 | |
| `docs/TODO.md` | | the knob-ranges entry (what is done) | | the UX-pass entry (help, first run: done) | |
| `host/crates/ergo/tables/journeys.txt` | | jam, perform (but `learn-cc`), keep's `shape`, explore's `live-profile` (new), `open-chip`, `chip-wait`, `chip-profile`, `chip-age`, `dist-shortcut`, `drive`, tour's `live-popup`, `chip-popup` | | explore's `chip-back`, `scene-home`; tour's new `help` step | grammar header (`setup`), perform's `learn-cc`, keep's `theme` |
| `host/crates/ergo/tables/recipes.txt`, `docs/recipes/*.json` | | first-sound, shape-a-sound | | | |
| `host/crates/ergo/src/consistency.rs` | | `CONTROLS` (PRESETS), `DOCUMENTED_INERT`'s two `live` lines | `DOCUMENTED_INERT`'s `scene` line | | everything else |
| `host/crates/ergo/src/knobs.rs` | | the `add(...)` calls `K3.live`, `K4.live`, new `PRESETS.live`, `K3.pwm`, `K3.perc`, `K3.sync`; new `P` variants | | | everything else |
| `host/crates/ergo/src/device.rs`, `journeys.rs` | | `panel::PRESETS`, `ENCODERS` | | | everything else |
| `host/crates/ergo/src/*` other | | | | | all |
| `CHANGELOG.md`, `docs/hardware-test.md` | own entry | own entry | own entry | own entry | own entry |

## Rules for every track

- TDD: the failing test first, then the change. Every finding id your track owns gets a test
  that would fail on the base and passes on your branch (named in your section).
- Colours come from theme roles only (`app/theme.h`); a view never names one of the 16
  colours for a purpose of its own. After T1: `value` values, `label` names, `hint` legends
  and help, `dim` only for inactive state (OFF, ---, a released note, an unused control, a
  bar's empty track, the screen's ring), `knob_text[k]` and `voice_text[v]` for 1x text that
  names a knob or a voice, `knob[k]` and `voice[v]` for bars, scopes and 2x text.
- Large text: a line at 2x is at most 14 characters on a 30-cell row (a cell of margin either
  side); a knob column's value at 2x is at most 3 characters.
- Every frame sends at most 200 cells (17.5 ms of SPI); the menu-like lists 190.
- Goldens: regenerate only the ones your section lists, one test at a time with
  `SIDOU_LCD_GOLDEN_UPDATE=1`, look at old and new, and describe the change in the report.
- controls.md and the journeys follow what you change, in the same commit as the change.
  Recipes: edit `host/crates/ergo/tables/recipes.txt`, then from `host/`
  `cargo run -q --release -p ergo -- --export-recipes ../docs/recipes`, and check them with
  `cargo run -q --release -p ergo -- --check-recipes ../docs/recipes` (exit 0).
- Report: `/Users/denis/sidou-opencode/<track>-report.md`, as COMMON.md says, with every gate
  of the budgets table before and after, and RAM region, POOL, RAMTEXT and image before and
  after (`song.rs`'s free-RAM test prints them).

---

## Track ergo-theme

### T1: theme roles (`feat(ui): theme roles that pass contrast: hint, knob and voice text`)

**Files:** `app/theme.h`, `app/theme.c`, `host/crates/sidou-tests/tests/app/theme.rs`,
`docs/art/README.md` ("Theme"), goldens.

1. Add to `theme`, after `dim`: `uint8_t hint;` (legends, help lines, the names under a
   choice), and after `knob[4]`: `uint8_t knob_text[4];` and `uint8_t voice_text[3];` (1x text
   naming a knob or a voice). Update the header comment's role list.
2. The tables (every other field as it is now):

   | Field | HIGH CONTRAST | C64 |
   |---|---|---|
   | bg | BLACK | BLUE |
   | value | WHITE | WHITE |
   | label | **LIGHT_GREY** (was GREY) | **CYAN** (was LIGHT_BLUE) |
   | hint | LIGHT_GREY | CYAN |
   | dim | DARK_GREY | **LIGHT_BLUE** (was GREY) |
   | hot | YELLOW | YELLOW |
   | alert | LIGHT_RED | **YELLOW** (was LIGHT_RED) |
   | cursor | DARK_GREY | DARK_GREY |
   | voice | LIGHT_RED, CYAN, LIGHT_GREEN | **LIGHT_GREY**, CYAN, LIGHT_GREEN |
   | voice_text | LIGHT_GREY, CYAN, LIGHT_GREEN | WHITE, CYAN, LIGHT_GREEN |
   | knob | YELLOW, CYAN, LIGHT_GREEN, LIGHT_RED | YELLOW, CYAN, LIGHT_GREEN, **LIGHT_GREY** |
   | knob_text | YELLOW, CYAN, LIGHT_GREEN, LIGHT_GREY | YELLOW, CYAN, LIGHT_GREEN, WHITE |
   | title_fg / title_bg | **LIGHT_GREY** / BLACK | **BLACK** / LIGHT_BLUE |
   | menu_item / menu_value | WHITE / CYAN | **LIGHT_GREY** / **WHITE** |
   | sel_fg / sel_bg | YELLOW / DARK_GREY | **BLACK** / LIGHT_BLUE |
   | toast_fg / toast_bg | BLACK / YELLOW | **BLUE** / **WHITE** |

   Why: light red on black is 4.42:1, under 4.5 for 1x text, so a knob or voice named at 1x
   in light red (PRESET, BLEED, MIX, the meters' "1") fails; it stays the accent for bars, 2x
   text and scopes (3:1 is enough there). On C64's blue only white, cyan, yellow, light green
   (and light grey at 2x) pass; red cannot be read on blue, so C64's red roles take light
   grey or yellow. The C64 menu keeps its reverse-video look (black on light blue, 3.86:1 at
   2x).
3. Test `theme_roles_pass_contrast` (new, `theme.rs`): with WCAG relative luminance computed
   from `sidou_sys::grid_palette` (the RGB565 expanded as `ergo::screen::palette_rgb` does;
   copy the 10-line formula, sidou-tests does not depend on ergo), for both themes: `value`,
   `label`, `hint`, `hot`, every `knob_text`, every `voice_text` against `bg` at least 4.5;
   `alert`, every `voice`, every `knob`, `menu_item`, `menu_value` against `bg` at least 3.0;
   `sel_fg` on `sel_bg` and `title_fg` on `title_bg` at least 3.0; `toast_fg` on `toast_bg`
   at least 4.5; `value` and `hot` on `cursor` at least 4.5; `popup_name` (THEME_ACCENT read as
   each `knob[k]`) on `popup_bg` at least 3.0. The test fails on the base (GREY label 4.02).
4. Goldens expected to change (the label colour, C64's colours): every LCD golden drawn in the
   high-contrast theme with label text (`live*.png`, `chip*.png`, `menu*.png`, `midi.png`,
   `status*.png`, `learn-*.png`, `popup-*.png`, `tracker*.png`) and `live-c64.png`. Not:
   `about.png`, `boot-*.png`, `title-*.png`, `charset.png`, `scene-*.png` (fixed colours).

### T2: the roles on CHIP and the menu (`fix(ui): CHIP's tabs, legends and help readable`)

**Files:** `app/chip_view.c` (`draw_tabs`, `draw_control`, `draw_names`, the help-line call in
`build`), `app/views.c` (`menu_compose`'s two help lines), tests, goldens.

1. `draw_tabs`: an unfocused tab `GRID_ATTR(th->label, th->bg)`; the focused tab
   `GRID_ATTR(th->bg, th->hot)`; with the focus on the A/B tabs, the CHIP tab
   `GRID_ATTR(th->bg, th->label)`.
2. `draw_control`: the label `GRID_ATTR(used ? th->knob_text[k] : th->dim, th->bg)`; the bar
   keeps `th->knob[k]` (or `dim`); the K1..K4 tag `THEME_ATTR(th, hint)`.
3. `draw_names`: MODEL, PROFILE and ALGORITHM in `hint`. The help line
   `SELECT MOVE ALGORITHM STEP` in `hint`.
4. `menu_compose`: the help lines (`SELECT  MOVE`, `ALG   CHANGE`) in `hint`.
5. Test (sidou-tests `chip_view.rs`) `chip_text_uses_readable_roles`: compose CHIP on the chip
   focus and on DIST in both themes and assert that no text cell outside the bars has a
   foreground of `dim` unless its text is OFF, a 0..127 value of an unused control, or the
   label of an unused control; that the tab row's cells have background `bg` except the
   focused tab's. Goldens: `chip.png`, `chip-dist.png`, `chip-mod.png`, `chip-fx.png`,
   `chip-b.png`, `chip-b-keys.png`, `chip-a-song.png`, `menu.png`, `menu-scrolled.png`.

### T3: the MIDI page on the menu renderer (`feat(ui): the MIDI page at 2x like the menu`)

**Files:** `app/views.c` (`midi_input`, `midi_compose`, `midi_look`, `midi_changed`, new
`big_menu_compose`, `menu_compose`, the `menu_init(&v->midi, ...)` line of `views_init`),
`app/views.h` (`VIEWS_MIDI_*`), screen.rs, goldens, controls.md "The MIDI page".

1. `views_init`: `menu_init(&v->midi, MIDI_ITEMS, VIEWS_MIDI_ITEMS, VIEWS_MENU_SHOWN, v);`
   then `v->midi.scale = 2;`.
2. New `static void big_menu_compose(screen* s, views* v, menu* m, const char* title, grid*
   g)`: what `menu_compose` does now after its look, for any 2x menu: the blank, the title bar
   rows 0-2 (`title` and `n/count`, the arrows at `head[6]` and `head[7]`), `sel_attr` and
   `value_attr` from the theme, the budget (`MENU_FRAME_CELLS` less `screen_layer_cells`),
   `force_rows`, `menu_draw`, and the help lines at `MENU_HELP_ROW` (`SELECT  MOVE`,
   `ALG   CHANGE`) in `hint`. `menu_compose` becomes `menu_look(...)` plus
   `big_menu_compose(s, v, &v->home, "MENU", g)`; `midi_compose` becomes `midi_look(...)` plus
   `big_menu_compose(s, v, &v->midi, "MIDI", g)`. Gone from the page: the ring, SIDOU and the
   version, the frame, the 1x help, READY.
3. `midi_look`: `out[10]` (the blink) is 0 always: the 2x list does not blink.
   `midi_changed` returns `v->midi.pending ||` the look changed, as the menu's does.
4. `midi_input`: the UI sounds as `menu_input` plays them (BLIP on a move, BUZZ at an end,
   CONFIRM on a change, BUZZ when ALGORITHM changes nothing).
5. The popup: ALGORITHM on the page names the item and its value. Emu test
   `the_midi_page_popup_names_the_item_and_its_value`: open MIDI, SELECT 1 (CHANNEL),
   ALGORITHM +1, run 200 ms; the views' `popup.name` is `CHANNEL` and `popup.value` is `2`
   (read through `sidou_sys::ilp32::views::popup` and `screen_popup_state`'s offsets), then
   golden `midi-popup.png`. If the value is empty, fix `popup_text`'s MIDI branch until it
   is not; if it passes at once, say in the report that the harness's capture came before the
   popup's value rows were drawn.
6. Lines fit: the longest are `ARP RATE 32 FR`, `CLOCK OUT  OFF` (14 characters).
7. Tests: rename and update `the_midi_page_matches_its_golden_image_and_stays_within_the_budgets`
   (every frame within 190 cells while scrolling 9 detents), golden `midi.png`. Docs: controls.md
   "The MIDI page": "six rows at 2x, like the main menu; the title bar says which of the ten";
   docs/art/README.md's menu paragraph names the MIDI page too.

### T4: the learn frame at 2x (`fix(ui): MIDI learn's frame at 2x, and nothing left when it closes`)

**Files:** `app/learn_view.h` (defines), `app/learn_view.c` (`lines`, `look`, `compose`,
`finish`), screen.rs, sidou-tests `learn_view.rs`, goldens.

1. Defines: `LEARN_VIEW_COL 0u`, `LEARN_VIEW_ROW 20u`, `LEARN_VIEW_W 30u`, `LEARN_VIEW_H 6u`,
   `LEARN_VIEW_CHARS 14u`. No box: the band of rows 20-25 in `bg`, three lines at 2x from
   column 1, on rows 20, 22 and 24 (180 cells, inside a frame's 200; a box would make it 240).
2. The lines (at most 14 characters; line 1 in `hot`, lines 2-3 in `value`):
   - waiting for a CC: `LEARN <PARAM>` (`LEARN CUTOFF`, `LEARN DIST MIX`), `SEND A CC`, and
     `SAVE FORGET 21` when the parameter has a CC (`SAVE FORGET127` for three digits), else
     `HOME CANCEL`;
   - the clear question: `CLEAR ALL?`, `SAVE YES`, `HOME NO`.
3. `finish`: after `screen_pop(lv->s, SCREEN_OVER)`, set `lv->s->redraw = 1` and
   `lv->s->redraw_rows |= FRAME_ROWS`, so the view under it composes the band's rows again.
4. Tests: sidou-tests `learn_view_lines_fit_at_2x` (every parameter a knob can pick, every
   state: each line at most 14 characters); emu `learn_leaves_nothing_when_it_closes`: TRACKER
   stopped, capture rows 20-25 of the LCD; SAVE held + K1 opens the frame; HOME cancels; after
   2,000 ms (the toast gone) rows 20-25 equal the capture. Goldens `learn-cc.png`,
   `learn-learned.png`. Every frame of `midi_learn_over_scene_and_live_stays_within_the_budget`
   within 200 cells.
5. controls.md "MIDI learn" is ergo-live's; tell ergo-live nothing: its text below already
   names the 2x lines.

### T5: the menu after a fast spin (`fix(ui): one selected line in the menu, however fast SELECT turns`)

**Files:** `app/widget.h` (`menu` gets `uint8_t barred;`), `app/widget.c` (`draw_big`,
`menu_init`), sidou-tests `widget.rs`, screen.rs, `docs/hardware-test.md` (a menu-spin check).

1. `barred`, bit k: line k was last drawn in `sel_attr`. `menu_init` clears it.
2. `draw_big`'s order: pass 0 the forced lines (as now); pass 1 first the selected line, then
   every line whose `barred` bit is set but is not the selected one (a stale bar), both drawn
   whatever the budget; then the others within the budget, as now. Each drawn line updates its
   `barred` bit. So every frame ends with exactly one bar, and the rest follow within the
   frames the budget needs.
3. Tests: sidou-tests `a_fast_spin_never_shows_two_selected_lines`: a 15-item scale-2 menu,
   budget 190; 8 moves of one item, one `menu_draw` a frame with the grid's pending cells
   flushed between (`grid_flush` or whatever `widget.rs` uses now); after each frame exactly
   one line's cells have `sel_attr`; within 3 frames after the last move the grid equals a
   fresh draw of the same menu. Emu `a_fast_select_spin_leaves_one_selected_line`: open the
   menu, `turn(SELECT, 8)` in one call, then on each of the next 6 frames exactly one line's
   column-0 cells show `sel_bg`.

### T6: the menu's order (`fix(ui): the playing views first in the menu`)

**Files:** `app/views.h` (`VIEWS_MENU_*`), `app/views.c` (`MENU_ITEMS` and its
`_Static_assert`), tests that select by index, controls.md (the Panel row and "Screen"),
docs/art/README.md.

1. The order: CHIP, TRACKER, SCENE, SONG (if your base has M2c's SONG item; else leave it
   out), PRE, OCTAVE, KEYS, SOUNDS, ROWS, CARDS, STORE, THEME, STATUS, MIDI, CHARSET, ABOUT.
   Renumber `VIEWS_MENU_*` to match and keep the assert listing them all.
2. Every test that selects a menu item by index (emu `select(&mut m, n)`, sidou-tests
   `views.rs`) follows the new indexes; the harness selects by text and needs nothing.
3. controls.md, the Panel row "ALGORITHM (encoder 1), in the menu", final text: "changes the
   item under the selection, which the popup also says in full: PRESET (PRE in the menu),
   OCTAVE, KEYS, SOUNDS, TRACKER ROWS (ROWS), CARDS (ONCE, ALWAYS or OFF: [title
   cards](#title-cards)), STORE ([format confirmation](#settings-and-store)), THEME; turned
   clockwise on CHIP opens the CHIP view, on TRACKER the song views, on SCENE the generative
   pieces, on STATUS the status screen, on MIDI the MIDI page, on CHARSET the glyph sheet, on
   ABOUT the credits". "Screen"'s menu paragraph lists the new order.
4. Goldens `menu.png`, `menu-scrolled.png`.

---

## Track ergo-live

LIVE's new layout, which L2-L5 build (rows and columns of the 30 x 30 grid):

| Rows | Columns | What |
|---|---|---|
| 0-1 | 0-21 | the preset's name at 2x in `value`, then `*` when it is edited (`SAW LEAD*`, `NOISE PERC*`) |
| 0-1 | 22-23 | while a song plays: the play mark (FONT_PLAY) at 2x, in `hot` for the first 120 ms of a bar's first beat, in `value` for the first 120 ms of every other beat, in `label` between |
| 0-1 | 26-29 | the octave at 2x in `hot` when it is not 0: `+1`, `-3`; nothing at 0 |
| 2-13 | | the three lanes, unchanged |
| 14 | 0- | the knob row's page in `hot`, 1x: `TONE`, `DIST`, `MOD`, `FX`; with EDIT held, the selected slot's (`DIST`) |
| 15 | per knob | the label: on TONE `CUTOFF`, `RESO`, K3's (`PULSE`, `RATIO`, `DECAY`), `RELEASE`; on a slot page the slot's control names (`chip_view_label`: DRIVE, TONE, BIAS, MIX); in `knob_text[k]`, `dim` for an unused control; reversed (`GRID_ATTR(th->bg, th->knob_text[k])`) for 1,000 ms after that knob turned |
| 16-17 | per knob | the value at 2x in `value` (`dim` unused), at most 3 characters |
| 18 | per knob | the bar in `knob[k]` over `dim` (`dim` fill for an unused control) |
| 19 | per knob | `K1`..`K4` in `hint` |
| 20 | 0-6 | while a song plays: the BPM, `125 BPM`, in `label` |
| 20 | 24, 26, 28 | while a song plays or a voice is muted or soloed: `1` `2` `3`, each in `label`, `dim` while muted, `GRID_ATTR(th->bg, th->hot)` while soloed |
| 21 | 0, 10, 24 | `FILTER` (label); `CHIP 6581` (label, the chip's model); `KEYS:B` (hot) while the keys play chip B |
| 22-23 | 0-9, 10-17 | the filter mode at 2x (value, `dim` OFF); the profile word at 2x in `value` (`R2`, `R3`, `R4AR`, `R5`, `WORN`, `USR3`), reversed for 1,000 ms after ALGORITHM changed it |
| 24 | 0, 5, 9 | `DIST` `MOD` `FX` at 1x: `value` when the slot has an algorithm, `dim` when OFF; the selected one reversed (`GRID_ATTR(th->bg, th->hot)`) |
| 25-26 | 0-17 | the selected slot's algorithm at 2x (`OVERDRIVE`, `PING-PONG`; `OFF` in dim), reversed in `hot` while the knob row shows that slot (a slot page or EDIT held) and for 1,000 ms after ALGORITHM changed it |
| 22-26 | 19-28 | VU, unchanged (labels `1 2 3 L R` in `hint`) |
| 21-26 | all | while GLO's layer is on (held or latched): row 21 `GLO  MUTE SOLO` (or `GLO LATCHED`) in hot; rows 22-23 six boxes at 2x at columns 0, 5, 10, 15, 20, 25: `M1 M2 M3 S1 S2 S3`, each in `label`, an active mute `GRID_ATTR(th->bg, th->alert)`, an active solo `GRID_ATTR(th->bg, th->hot)`; rows 24-26 blank |
| 27-28 | | the toasts' rows, nothing of LIVE's |

### L1: knob curves (`feat(live): an even cutoff knob and an audio taper on MASTER`)

**Files:** new `app/live_cutoff.c` (generated), `app/live.h`, `app/live.c` (`live_knob`'s
cutoff case, `load_preset`, `live_set_master`), `app/master_gain.h`, `app/master_gain.c`,
`app/views.c` (`param_text`'s VOLUME, `status_compose`'s VOLUME line), new sidou-tests
`live_cutoff.rs`, `master_gain.rs`, `live_instrument.rs`, emu tests that assume a linear
MASTER, CMakeLists/build.rs, controls.md, docs/TODO.md.

1. **The cutoff table.** `live_cutoff.c`, generated by sidou-tests `live_cutoff.rs` with
   `SIDOU_CUTOFF_TABLE_UPDATE=1` (as `live_freq.rs` does `live_freq.c`), from the firmware's own
   `sid_cutoff_hz(model, 0, fc)`, for each model m (0 6581, 1 8580):
   - `hz(r)` for r = 0..2047; r0 = the smallest r with `hz(r) >= hz(0) * 2^(1/12)` (a semitone
     above the floor: the 6581's bottom registers all sit on its 238 Hz floor);
     `f_lo = hz(r0)`, `f_hi = max(hz)`;
   - `live_cutoff_steps[m][i]`, i = 0..64 (`LIVE_CUTOFF_STEPS 65`): entry 0 is r0; entry i is
     the smallest r greater than entry i-1 with `hz(r) >= f_lo * (f_hi / f_lo)^(i / 64)` and
     `hz(r) > hz(entry i-1)`; if none, 2047. The test asserts both registers and Hz rise
     strictly along the table, so the 6581's dips at 512 and 1024 are skipped over;
   - `live_cutoff_up[m][b]` and `live_cutoff_down[m][b]` (`int8_t`, b = 0..63, a bucket of 32
     registers): the smallest i whose entry is above every register of bucket b (65 if none),
     and the largest i whose entry is below every register of it (-1 if none).
2. **K1** (`live_knob`, `LIVE_KNOB_CUTOFF`): with `m = l->model`,
   ```
   int32_t pos = l->cutoff_pos;
   if (live_cutoff_steps[m][pos] != l->cutoff) {   /* moved by MIDI, a preset or the model */
       pos = steps > 0 ? live_cutoff_up[m][l->cutoff >> 5] - 1 : live_cutoff_down[m][l->cutoff >> 5] + 1;
   }
   pos = clamp(pos + steps, 0, 64);
   l->cutoff_pos = (uint8_t)pos;
   set_cutoff(l, live_cutoff_steps[m][pos]);
   ```
   If the new entry is not beyond the cutoff in the turn's direction (a MIDI cutoff above the
   table's top entry, or below its first), the cutoff stays. `cutoff_pos` is a new `uint8_t` at
   the end of `live`, 0 after `live_init`. MIDI, NRPN and
   the settings keep the full 11 bits; only the knob walks the table. `LIVE_CUTOFF_DETENT`
   goes. Cost: the K1 input over LIVE (POPUP_INPUT) may rise by at most 15 instructions here
   (L3 adds LIVE's input call): keep the steady path to the equality test and one table load.
3. **MASTER's taper.** `master_gain_of(v, max)`: `v >= max` is `MASTER_UNITY`; otherwise
   `x = (v * 1024 + max / 2) / max` (0..1023), `i = x >> 4`, `f = x & 15`,
   `g = (MASTER_TAPER[i] * (16 - f) + MASTER_TAPER[i + 1] * f + 8) >> 4`, with
   `MASTER_TAPER[0] = 0` and `MASTER_TAPER[i] = round(32768 * 10^(-3 * (1 - i / 64)))` for
   i = 1..64 (-60 dB at i = 1 rising 0.94 dB an entry to 0 dB at 64), a `const uint16_t[65]`
   in `master_gain.c`. Why not the proposed cube: a cube's steps are 26 / p dB, 4.6 times
   uneven between a quarter and full travel; this taper's are even in dB. Update the header
   comment (it says linear now).
4. **The readout.** `l->volume` becomes the pot's travel in percent, 0..100
   (`(value * 100 + LIVE_MASTER_MAX / 2) / LIVE_MASTER_MAX`), set in `live_set_master` (not
   per frame: STATUS idle reads the byte as before). The popup: `VOLUME`, value `72%`, the bar
   `l->master` of `LIVE_MASTER_MAX`. STATUS: `status_param(..., "VOLUME", l->volume, 100,
   th->value)`.
5. Tests: `live_cutoff.rs` `the_cutoff_table_is_generated_and_rises` (drift check and the
   monotonic asserts above; each step between 40 and 160 cents for both models);
   `live_instrument.rs` `k1_walks_the_table_and_snaps_once` (from the preset's 1200, +1 lands
   above 1200 and on the table, -1 below; 64 detents span the table; a MIDI cutoff in between
   is left in the right direction); `master_gain.rs` `master_has_an_even_db_taper` (0 is 0,
   1023 is unity, monotonic, from -40 dB up every 16 pot steps rise between 0.8 and 1.1 dB),
   and the table's values. Emu tests that compare MASTER against a linear gain follow the new
   `master_gain_of` (they call it through `common::master_stage`, so most need nothing; any
   assertion of the old linear values changes, which is the intended behaviour, not a loosened
   bound). Goldens: `status.png`, `status-a4.png`, `status-toast.png` (VOLUME in percent).
6. Docs: controls.md Panel rows K1 ("cutoff, 64 detents from a semitone above the chip's
   lowest to its highest, even in pitch") and MASTER; "Chip character"'s MASTER paragraph
   (the taper replaces "the gain is linear in the pot"; full is still exactly the chips'
   output); the popup table's VOLUME row (`72%`). docs/TODO.md's knob-ranges entry: a line
   saying K1, MASTER and K3 are done here.

### L2: LIVE's knob row and the popup policy (`feat(live): the knob row says what K1-K4 turn, and the popup keeps out of the way`)

**Files:** `app/live_view.h`, `app/live_view.c` (all), `app/chip_view.h`, `app/chip_view.c`
(`compose`, new `highlight`, new `chip_view_flash`), `app/views.c` (`tell`, new `live_frame`
and its call, `popup_text`), `app/screen.c` (`screen_toast`), tests, goldens, controls.md
("Value popup", "Screen"'s LIVE paragraph).

1. **Values at most 3 characters** (`live_view_knob_text`): the cutoff below 1,000 Hz as
   digits (`980`, `30`), from 1,000 to 9,949 Hz resistor style rounded to 100 Hz (`1K2`, `7K2`,
   `9K9`), from 9,950 Hz whole kilohertz (`10K`, `12K`); resonance `0`..`15`; K3 and K4 as L3
   says. The popup keeps `live_view_cutoff_text` (`7.2K`, `11.4K`). Values start at their
   column's first cell; columns stay 0, 8, 15, 22 (8, 7, 7, 8 wide), so at least one blank
   cell follows every value.
2. **The layout** of the table above: the header (name, `*` from L4, play mark from L5,
   octave), row 14's page tag, the knob row, row 20 (L5), the bottom band. `live_view` gets
   (at the struct's end): `uint8_t page;` (0 TONE, 1 a slot: the selected one, set by L3),
   `uint8_t edit;` (EDIT held), `uint8_t hl;` (0 none, 1..4 a knob, 5 the profile, 6 the
   slot's algorithm), `uint32_t hl_until;` and the L5 fields. The look gets the bytes these
   parts show (page, edit, hl, the slot page's four values and its algorithm, the profile,
   the octave); `LIVE_VIEW_LOOK` grows to match. While `page` is 1 or `edit` is set, the knob
   row shows the selected slot's Amount, A, B and Mix with `chip_view_label`'s names.
3. **The popup policy** (new `static void live_frame(views* v, uint32_t now_ms)` in views.c,
   called in `views_frame` immediately before `screen_frame`):
   - it takes views_frame's `top` (`shows(v)`, already computed there) as a parameter:
     `live_frame(v, top, now_ms)`, and does everything below only when `top` is LIVE, but for
     the CHIP case; on STATUS its cost is two tests (the share in the budgets table);
   - it sets `v->live.edit` from `v->p->held` (EDIT) when `v->p` is set;
   - if `v->s.popup_ask` holds a control's ask (`ASK_BASE` set, `ASK_PARAM` clear) and the view
     that shows (`shows(v)`) is LIVE: for K1..K4 call `live_view_flash(&v->live, k + 1, now)`,
     for ALGORITHM `live_view_flash(&v->live, 5 or 6, now)` (5 on TONE, 6 on a slot page), for
     PRESETS nothing; then clear `popup_ask`. MASTER keeps its popup;
   - if it is CHIP: K1..K4 without EDIT, or with EDIT while the focus is on a slot, call
     `chip_view_flash(&v->chip, k, now)` and clear the ask; ALGORITHM clears the ask (the
     choice shows at 3x); EDIT + K1..K4 with the chip focus keeps the popup (the slot is not on
     the page);
   - every other view keeps its popups as now.
   `live_view_flash(lv, what, now)` sets `hl = what`, `hl_until = now + SCREEN_POPUP_MS`; the
   compose drops `hl` once `s->now >= hl_until`. `chip_view_flash(cv, k, now)` sets the CHIP
   view's new `hl` (k + 1) and `hl_until`; its look includes `hl`; `compose` calls the new
   `static void highlight(chip_view* cv)` right after `build(cv)`, which rewrites the turned
   control's label cells in the canvas to `GRID_ATTR(th->bg, th->knob_text[k])` (`dim` labels
   stay dim). This happens in the frame, so the input paths (POPUP_INPUT, CHIP_INPUT) do not
   change.
4. **Toasts on LIVE** (`tell`): with LIVE on top (`screen_top(&v->s) == &live_view_view`) the
   slot and algorithm toasts are not shown (the band shows them at 2x); the octave and chip
   toasts are, as everywhere else.
5. **A toast replaces a popup** (`screen_toast`): a popup open, opening or closing closes at
   once (its rows are composed again by the view this frame), and the toast draws. Why: the
   OCTAVE +1 under VOLUME and DIST OVERDRIVE over the meters stacked.
6. Tests (sidou-tests `live_view.rs`): `every_knob_value_fits_its_column` (every cutoff
   register on both models, every resonance, every K3 and K4 value of L3, every slot value:
   at most 3 characters); `the_slot_page_names_the_slot_controls` (page 1 on DIST OVERDRIVE:
   row 15 reads DRIVE, TONE, BIAS, MIX; TAPE: A and B in dim). Emu (screen.rs):
   `a_knob_on_live_highlights_its_column_and_opens_no_popup` (K1 on LIVE: no popup within
   300 ms, row 15 columns 0-5 reversed in yellow, back after 1,200 ms; golden `live-highlight.png`
   replaces `popup-live.png`); `a_knob_on_chip_highlights_and_opens_no_popup` (golden
   `chip-highlight.png` replaces `popup-chip.png`); `edit_held_relabels_the_knob_row` (EDIT
   held on LIVE: row 15 reads the slot's names, row 14 `DIST`); `a_toast_closes_an_open_popup`
   (MASTER moved, then OCT+: the popup is gone in the next frame and the toast shows);
   `the_octave_shows_at_2x_when_not_0`. Update
   `a_knob_popup_matches_its_goldens_over_live_and_chip_and_gives_the_view_back` to the new
   policy (the popup over STATUS and the menu stays). Goldens: `live.png`, `live-a4.png`,
   `live-chord.png`, `live-keys-b.png`, `live-c64.png`, `live-mutes.png`; `popup-live.png` and
   `popup-chip.png` are removed for `live-highlight.png` and `chip-highlight.png`. The other
   CHIP goldens do not change (the highlight shows only after a turn).
7. Docs: controls.md "Value popup" (where it opens now, the highlight, a toast replaces it),
   "Screen"'s LIVE paragraph (the table above in prose), docs/art/README.md "LIVE".

### L3: the encoders and K3/K4 on LIVE (`feat(live): PRESETS, SELECT and ALGORITHM play on LIVE; K3 is the sound's own control, K4 its release`)

**Files:** `app/panel.h`, `app/panel.c`, `app/live.h`, `app/live.c`, `app/live_view.c`
(input), `app/views.c` (`preset_turn`, `popup_text`, `param_text`, `views_encoder`'s SAVE
branch), `app/learn_view.c` (`pick`), the harness lines of the ownership table, tests,
journeys, recipes, controls.md.

1. **PRESETS** (`#define PANEL_ENC_PRESETS 6u`): in every view a view does not take it, it
   steps the preset: `panel_encoder`, after the K1..K4 range test fails, calls
   `live_preset_step(l, steps, p->held >> PANEL_BTN_EDIT & 1u)`. Clockwise the next, clamped at
   both ends. With EDIT held it reverts the current preset to its factory values (L4). The
   menu's PRE item turns the same way (`preset_turn` calls `live_preset_step(l, steps, 0)`).
   The popup asks for it as for any encoder; it shows `PRESET` and the name (with `*`), and on
   LIVE `live_frame` drops it (the header says it).
2. **K4 is RELEASE** (`LIVE_PARAM_RELEASE`, 0..15, a step a detent) on LIVE's page.
   `live_knob_id` becomes `LIVE_KNOB_CUTOFF, LIVE_KNOB_RESONANCE, LIVE_KNOB_TONE,
   LIVE_KNOB_RELEASE`; `panel_knob_param`'s K4 is `LIVE_PARAM_RELEASE` (learnable, a CC on MIDI
   out like any parameter).
3. **K3 is the sound's own control** (`LIVE_KNOB_TONE`), chosen by the sound, not the preset's
   index, by `live_tone(const live* l)` returning:
   - `LIVE_TONE_RATIO` when the preset is mono-sync (SYNC LEAD): the sync ratio in semitones,
     -12..+12 from the preset's (`+0`), a semitone a detent. New `int8_t sync_semis` and
     `uint16_t sync_q8` at the end of `live`: `sync_q8 = (preset ratio * LIVE_RATIO_Q16[semis
     + 12] + 32768) >> 16`, `LIVE_RATIO_Q16[k] = round(65536 * 2^((k - 12) / 12))` (25 entries in
     `live.c`, checked by a host test), and `voice_freq` uses `l->sync_q8` instead of the
     preset's. Not a MIDI parameter and not saved (it has no CC; the settings record does not
     change): documented as the panel's alone. `panel_knob_param`'s K3 is `LIVE_PARAM_COUNT`
     then, so MIDI learn on it says `RATIO: NO CC` (`pick` in learn_view.c: the toast K4's
     preset used to give, with the K3 label);
   - `LIVE_TONE_PULSE` when the waveform has the pulse bit (PWM BASS): the pulse width on one
     side only, positions i = 0..30, `pw = 128 + 64 * i` (3 % to 50 %, 1.6 % a detent);
     clockwise wider. A pulse width off the grid or above 2048 (MIDI) is mirrored
     (`4096 - pw`) and rounded to the nearest position before the step;
   - `LIVE_TONE_DECAY` otherwise (SAW LEAD, NOISE PERC): `LIVE_PARAM_DECAY`, 0..15.
   Labels `RATIO`, `PULSE`, `DECAY`; values `+0` / `-12`, `50%` / `3%`, `0`..`15`.
   `panel_knob_param`'s K3 is `LIVE_PARAM_PULSE_WIDTH`, `LIVE_PARAM_DECAY` or
   `LIVE_PARAM_COUNT` accordingly. Why not vibrato on SAW LEAD (considered): its change is a
   pitch wobble, not a timbre, and CC 77 already reaches it; decay is heard on every saw and
   noise sound.
4. **LIVE's input** (`live_view_view` gets `input`; `live_view` keeps `live* l`, no longer
   `const`, and `live_view_init` takes `live*`):
   - SELECT pages the knob row: TONE, DIST, MOD, FX, clamped at both ends; a slot page selects
     that slot (`live_fx_select`), and SCL on a slot page moves the page with the slot;
   - ALGORITHM on TONE steps the profile through the five built-in ones, clamped:
     `live_param_set(l, LIVE_PARAM_PROFILE, clamp(base + steps, 0, SID_CHIPS - 1))`, base the
     applied USER's base profile if `user_applied`, else `profile_req`; on a slot page it steps
     that slot's algorithm, clamped (`live_slot_set(l, slot, LIVE_SLOT_ALGORITHM, ...)`; S3
     switches it to `live_slot_pick`);
   - K1..K4 on a slot page turn the slot's Amount, A, B, Mix (`live_slot_turn`) and return 1;
     on TONE the input returns 0 and the panel turns them (so LIVE's TONE path pays only the
     call);
   - MIDI learn on a slot page: in `views_encoder`'s SAVE branch, when LIVE shows and
     `v->live.page` is 1, pass `p->held | 1u << PANEL_BTN_EDIT` to `learn_view_open`, so the
     knob picks the slot's control.
   Not `knob_page`: changing it costs the HOME gate.
5. Tests: sidou-tests `panel.rs` `presets_steps_the_preset_in_every_view`,
   `k4_turns_the_release`; `live_instrument.rs` `k3_follows_the_sound` (SAW LEAD DECAY, PWM
   BASS PULSE, SYNC LEAD RATIO, NOISE PERC DECAY; a sound switched to pulse by CC becomes
   PULSE), `k3_is_audible_on_every_preset` (render 0.5 s of A3 through the host chip at K3's
   preset value and 8 detents away: the RMS of the difference is at least -30 dB of the
   note's RMS on all four presets), `k4_release_is_audible`, `ratio_table_is_exact` (each
   entry within 1 of the formula); emu screen.rs `select_pages_and_algorithm_steps_the_profile_on_live`
   (SELECT +1: row 14 `DIST`, `fx_select` 0; ALGORITHM +1 on TONE: `profile_req` goes from
   R4AR to R5, the band shows `R5`, no popup). Midi learn: `k3_ratio_has_no_cc`.
6. **The harness** (lines listed under ownership): `device.rs` `pub const PRESETS: u8 = 6;`,
   `journeys.rs` `ENCODERS` adds `("PRESETS", panel::PRESETS)`; `consistency.rs` `CONTROLS`
   adds `("PRESETS", panel::PRESETS)` and `DOCUMENTED_INERT` loses `("live", "SELECT")` and
   `("live", "ALGORITHM")`; `knobs.rs`: `K3.live` sweeps `P::Decay` on SAW LEAD, `K4.live`
   sweeps `P::Release`, new `PRESETS.live` (`P::Preset`, a choice), `K3.pwm` (setup `turn
   PRESETS 1`, `P::PulseWidth`), `K3.perc` (setup `turn PRESETS 3`, `P::Decay`), `K3.sync`
   (setup `turn PRESETS 2`, `P::SyncSemis` reading `live.sync_semis`); add those `P` variants.
7. **Journeys** (`journeys.txt`): jam `shape-cutoff`, `shape-resonance` flag `change` for
   `popup`; `shape-pulse` becomes `Turn K3 | Turn K3 for the sound's own control | down A3;
   turn K3 6; turn K3 -6; up A3 | change; clip 300`; `next-preset` becomes `Turn PRESETS to the
   next sound | Turn PRESETS to change the sound | turn PRESETS 1; note A3 600 | sound; clip
   300; task change preset; destructive; defaults`; perform `panel-preset` `turn PRESETS -2 |
   text SAW LEAD; task change preset`; keep `shape` `change; remember`; explore: a new first
   step `live-profile | Pick the next profile on LIVE | ALGORITHM steps the profile | turn
   ALGORITHM 1; note A3 500 | sound; clip 300; task change the profile`, `open-chip` and
   `chip-wait` lose their task, `chip-profile` loses its task, `chip-age` `change` for `popup`,
   `dist-shortcut` `change; sound; clip 300; task change DIST algorithm` (no toast on LIVE),
   `drive` becomes `Set DIST's drive on its page | SELECT to DIST, then K1 | down A3; turn
   SELECT 1; turn K1 20; turn SELECT -1; up A3 | change; clip 300`; tour `live-popup` and
   `chip-popup` `change` for `popup`. Recipes: first-sound `boot-ends` `value octave 0 0` for
   `text OCT+0`; shape-a-sound `close-the-filter` and `add-resonance` `change` for `popup`,
   `next-sound` `turn PRESETS 1` with its caption and `do` text; export and check them.
8. Docs: controls.md Panel rows K3 ("the sound's own control: RATIO on a sync sound, PULSE on
   a pulse sound, DECAY otherwise"), K4 (release), new rows "SELECT, on LIVE", "ALGORITHM, on
   LIVE", "PRESETS (encoder 6)"; the sentence after the table: "The other buttons (LFO, ARP)
   do nothing yet; SELECT does nothing on STATUS and ABOUT, ALGORITHM nothing on STATUS and
   ABOUT." (TRACKER's per M2d at your base); "Presets": PRESETS, not K4; "MIDI learn"
   step 1: the frame (T4's lines: `LEARN CUTOFF`, `SEND A CC`, `HOME CANCEL`), K3 RATIO has no
   CC and says so, K4 is the release.

### L4: an edit memory per preset (`feat(live): each preset keeps your edits while you browse`)

**Files:** `app/live.h`, `app/live.c`, `app/live_view.c` (the `*`), `app/midi_out.c` (`look`),
`app/views.c` (`param_text`/`popup_text`'s preset name), tests, journeys, controls.md.

1. `live_edit` (in live.h): `uint16_t cutoff, pulse_width; uint8_t resonance, control,
   attack_decay, sustain_release, filter_voices, filter_mode, bend_range, velocity_target,
   mod_target, pressure_target, vibrato_rate, vibrato_depth, vibrato_delay; int8_t
   sync_semis; uint8_t valid;`. `live` gets `live_edit edits[LIVE_PRESET_COUNT];` at its end
   (80 bytes), all invalid after `live_init`.
2. `live_recall_preset(l, q)` (what PRESETS uses): store the current state into
   `edits[l->preset]` (valid), `load_preset(l, q)` (the factory's), then, if `edits[q]` is
   valid, apply its fields, and `write_all(l)`. `live_set_preset` (MIDI's program change, the
   settings' load) is unchanged: the factory's over any edits, as midi.md says. The revert
   (EDIT + PRESETS): `live_set_preset(l, l->preset)` and `edits[l->preset].valid = 0`.
3. `live_preset_edited(const live* l)`: 1 when any of `live_edit`'s fields differs from the
   factory preset's (`sync_semis` from 0). The header shows `*` after the name while it is 1
   (a look byte); `live_preset_name(l, buf)` gives `SAW LEAD` or `SAW LEAD*` for the popup and
   the menu's PRE.
4. MIDI out: a recall sends the program change and then, as CCs, the restored parameters that
   differ from the factory preset. In `look`'s preset branch, `wire[p]` for the loaded
   parameters is the factory value (new `live_preset_params(index, out)` gives what
   `live_set_preset` would set), not `now[p]`, so the diff sends the rest.
5. Tests: `live_instrument.rs` `a_preset_remembers_its_edits` (edit SAW LEAD's cutoff, PRESETS
   +1, -1: the edit is back and `*` shows), `program_change_loads_the_factory`,
   `edit_presets_reverts`; `midi_out.rs` `a_recall_sends_the_program_and_the_edits`. Emu:
   golden `live-edited.png` (SAW LEAD* after K1 -10).
6. Journeys: jam gets, after `next-preset`, `step back-preset | Back to your sound | Turning
   back brings your edits back | turn PRESETS -1 | recalled`, and `shape-pulse` gets
   `remember`. Docs: controls.md "Presets" (the memory, `*`, EDIT + PRESETS, not saved but for
   the current preset, which the settings keep as before).

### L5: GLO and the song on LIVE (`feat(live): GLO's layer shown and latchable, the song's state on LIVE`)

**Files:** `app/panel.h`, `app/panel.c` (`panel_note_keys`, `panel_get_leds`), `app/views.c`
(`global`'s GLO branch, new `glo_tap`, `live_frame`), `app/live_view.c`, tests, journeys, controls.md.

1. **Latch:** `panel` gets `uint8_t glo_latch;`. A new branch at the end of `global` (after
   REC's, so HOME's path is untouched): a GLO press calls `glo_tap(v)` and returns 0 (the press
   goes on to the panel as before). `glo_tap`: not latched, and GLO's previous press was at most
   400 ms ago (`v->glo_tap_ms`, the screen's `v->s.now`): `v->p->glo_latch = 1`; latched: a press
   sets it to 0; it records `glo_tap_ms`. A toast says `GLO LATCHED` or `GLO OFF`. GLO's LED is
   lit while latched. Nothing runs in `views_buttons`' every-pass path: the USB drain's worst
   pass (7,991 of 8,000) includes it.
2. **The layer:** `panel_note_keys`'s `glo` is GLO held or `glo_latch`; then only the leftmost
   `PANEL_GLO_KEYS` (6) keys are the mutes and solos and sound nothing; the other keys play.
   This changes "the keys sound nothing while GLO is held" for held GLO too, so held and
   latched are one rule (a deliberate change: say it in controls.md).
3. **On LIVE:** `live_frame` sets `v->live.glo` (bit 0 the layer on, bit 1 latched),
   `v->live.mutes` (as now), and, while a song plays (`song_view_playing`), `song_bpm`
   (`song_view_shown_bpm`) and `beat` (0 none, 1 a beat's first row, 2 a bar's: from
   `song_view_position`'s row and the MIDI route's rows a beat, 4 without one); the header's
   play mark and row 20 follow the layout table; the mutes no longer replace the preset's
   name. The GLO overlay replaces rows 21-26 while `glo` bit 0 is set.
4. Tests: `panel.rs` `glo_double_tap_latches_and_a_tap_releases`, `glo_layer_lets_the_other_keys_play`;
   emu `live_shows_the_song_and_the_mutes` (PLAY on LIVE: the play mark at columns 22-23, `125
   BPM` on row 20; GLO latched + key 2: the overlay's M2 lit, voice 2's badge dim; golden
   `live-song.png`, `live-glo.png`; `live-mutes.png` changes).
5. Journeys: perform `mute-voice-2` `Latch GLO and press key 2 | Double-tap GLO, then key 2 |
   press GLO; press GLO; note F#3 200 | quieter; task mute voice 2; probe`, `unmute-voice-2`
   `note F#3 200 | louder; task unmute voice 2`, `solo-voice-1` `note G#3 200 | quieter; task
   solo voice 1; probe`, `unsolo-voice-1` `note G#3 200 | louder; task unsolo voice 1`, then
   new `step unlatch | Let GLO go | A tap on GLO ends the layer | press GLO | -`. If the
   harness's two `press GLO` land more than 400 ms apart (its `press` holds a button for some
   frames), stop and report the timing rather than lengthening the window. Docs: controls.md GLO row, "Song views" paragraph on LIVE's top line, the
   screen.

---

## Track ergo-sound

### S1: effects you hear on pick (`feat(fx): a slot picked from the panel starts from a musical setting`)

**Files:** new `app/fx_defaults.c` (and its declaration in `app/live.h`), `app/live.c`
(`live_slot_next_algorithm`, new `live_slot_pick`), `app/chip_view.c` (`input`'s slot
ALGORITHM line), tests, controls.md ("Effect slots", the ENV row).

1. `const uint8_t live_fx_defaults[LIVE_SLOTS][8][4]` (Amount, A, B, Mix per algorithm, 0..127;
   OFF's row zeros):

   | Slot | Algorithm | Amount | A | B | Mix |
   |---|---|---|---|---|---|
   | DIST | TAPE | 48 | 0 | 0 | 127 |
   | DIST | OVERDRIVE | 52 | 64 | 32 | 127 |
   | DIST | FUZZ | 48 | 0 | 32 | 127 |
   | DIST | CRUSH | 64 | 32 | 0 | 127 |
   | DIST | LO-FI | 64 | 16 | 16 | 127 |
   | MOD | CHORUS | 64 | 32 | 0 | 64 |
   | MOD | FLANGER | 64 | 24 | 96 | 64 |
   | MOD | PHASER | 64 | 32 | 64 | 64 |
   | MOD | TREMOLO | 80 | 48 | 0 | 127 |
   | MOD | AUTO-PAN | 96 | 32 | 0 | 127 |
   | MOD | WOW | 48 | 32 | 0 | 127 |
   | FX | DELAY | 48 | 64 | 48 | 40 |
   | FX | PING-PONG | 48 | 64 | 48 | 40 |
   | FX | REVERB | 72 | 48 | 80 | 40 |
   | FX | LUSH | 80 | 48 | 96 | 40 |
2. `live_slot_pick(l, slot, alg)`: if the slot's Amount and Mix are both 0, or its four
   controls equal the defaults of the algorithm it has now (untouched since it was picked),
   and `alg` is not OFF, set `alg` and then its four defaults (`live_slot_set` each);
   otherwise set `alg` alone. No new state, and nothing on the MIDI path: an algorithm CC
   still goes through `live_param_set` and loads nothing. Why this rule and not a "touched"
   flag: stepping OFF, TAPE, OVERDRIVE keeps giving each algorithm its own start until the
   player turns a control, and it needs no RAM.
3. Callers: `live_slot_next_algorithm` (ENV), CHIP's ALGORITHM on a slot (`input`); LIVE's
   ALGORITHM on a slot page in S3.
4. Tests: `live_instrument.rs` `a_silent_slot_picks_up_a_musical_start`,
   `an_edited_slot_keeps_its_values`, `a_midi_algorithm_loads_nothing`; `fx_live.rs`
   `every_default_is_audible` (a 220 Hz saw, 0.5 s, through `sid_slots` at each algorithm's
   defaults against its dry signal: the RMS of the difference at least -20 dB of the dry
   RMS). Emu `env_on_live_is_heard`: ENV once, `live.slot[0]` is {1, 48, 0, 0, 127}.
5. Docs: controls.md "Effect slots" (the table above; "everything is off by default" stays),
   the ENV row.

### S2: SCENE (`feat(scene): FIRE burns low in silence; the piece's name on entry; ALGORITHM steps the piece`)

**Files:** `app/scene.c` (`fire_burn`), `app/scene_view.c` (`enter`, `input`, the compose),
`app/scene_view.h`, `host/crates/ergo/src/consistency.rs` (the `scene` line), tests, goldens,
controls.md "SCENE", docs/art/README.md.

1. `fire_burn`: a silent voice's `hot` falls to `FIRE_IDLE_HOT` (120), not to 0: `hot -=
   (hot + 3) / 4` and then `hot = hot < FIRE_IDLE_HOT ? FIRE_IDLE_HOT : hot`. At 120 the bed
   burns at about 94 and a flame a few cell rows high flickers (cooling about 6 a heat row).
2. The name on entry: `enter` sets `sv->say = 1` (new field); the compose, when the scene is
   on top (not under a title card, `screen_top(s) == &scene_view_view`), calls
   `screen_toast(s, scene_names[sv->which], SCREEN_TOAST_MS)` once and clears `say`. `input`
   drops its own `screen_toast` (the compose says it), which also shortens the SELECT path.
3. ALGORITHM on SCENE steps the piece exactly as SELECT does (the same code); remove
   `("scene", "ALGORITHM")` from the harness's `DOCUMENTED_INERT`.
4. Tests: emu `fire_keeps_a_low_flame_in_silence` (FIRE, no note, 3 s: at least 20 cells of
   rows 22-28 are red or light red on the LCD); `entering_scene_names_the_piece` (after the
   card, `FIRE` toasts); `algorithm_steps_the_piece`. SCENE_INPUT stays under 230 (it
   should fall). Goldens: `scene-fire.png` (the flame), any `scene-*.png` whose capture now
   includes the entry toast (allowed: say which).
5. Docs: controls.md "SCENE" table's FIRE row ("silent, it burns low"), its SELECT/ALGORITHM
   sentence; docs/art/README.md's FIRE.

### S3: DICE (after ergo-live has merged) (`feat(live): DICE, a new sound at every turn`)

**Files:** new `app/live_dice.c`, `app/live_dice.h`, `app/live.h`, `app/live.c`
(`live_preset_step`, `live_set_preset`'s clamp, `presets[]`, `live_preset_name`),
`app/live_view.c` (LIVE's slot ALGORITHM to `live_slot_pick`), `app/midi_out.c` (`look`),
tests, controls.md (a "DICE" subsection after "Presets"' Known limit).

1. `LIVE_PRESET_SLOTS 5u`: the four presets and DICE at index 4 (`LIVE_PRESET_COUNT` stays 4,
   what program changes select). `presets[4]` is `DICE`, SAW LEAD's values otherwise: the
   base a roll starts from. `live_set_preset` clamps to `LIVE_PRESET_SLOTS - 1` (the settings
   load DICE back); midi_rx and midi_route already refuse a program of 4.
2. `live_preset_step`: clockwise from NOISE PERC onto DICE recalls DICE's last roll if
   `edits[4]` is valid, else rolls; clockwise again while on DICE rolls a new one each detent;
   counter-clockwise from DICE recalls NOISE PERC (storing the roll). `edits` grows to
   `LIVE_PRESET_SLOTS`.
3. `live_dice_roll(live* l, uint32_t seed)`: xorshift32 from `seed | 1`; `seed` is
   `l->note_ons * 2654435761u ^ l->clock * 40503u ^ ++l->dice_rolls * 0x9E3779B9u ^
   l->lfo_phase` (`dice_rolls` a new `uint8_t`). Load `presets[4]`, then set through
   `live_param_set`: waveform r % 8: 0-2 saw, 3-4 pulse, 5-6 triangle, 7 noise; ring (triangle
   only) when r % 4 == 0; sync (saw or pulse) when r % 8 == 0; attack r % 7; decay 4 + r % 8;
   sustain 0 for noise, else 4 + r % 12; release 3 + r % 8; cutoff `live_cutoff_steps[model][16 +
   r % 45]`; resonance r % 13; filter mode r % 8: 0-4 LP, 5 BP, 6 HP, 7 LP+HP; filter voices
   all; pulse width (pulse) `128 + 64 * (4 + r % 27)`; vibrato depth 8 + r % 33 when r % 8 ==
   0, else 0; one `r` per choice, in this order. No effect slot: they are the instrument's, not
   a preset's, and a roll that changed them would carry into every preset.
4. The name: `DICE 07` (`dice_rolls` mod 100, two digits) in the header and `live_preset_name`;
   no `*` for DICE.
5. MIDI out: no program change for DICE (index 4): its parameters go out as CCs (the preset
   branch of `look` is skipped for index 4 and the diff sends them).
6. LIVE's ALGORITHM on a slot page calls `live_slot_pick`.
7. Tests: `live_dice.rs` (new) `every_roll_stays_in_its_ranges` (10,000 seeds),
   `a_roll_is_kept_and_recalled`, `dice_is_saved_and_loaded` (settings record with preset 4);
   emu `presets_reaches_dice_and_rolls` (golden `live-dice.png`).
8. Docs: controls.md "DICE" subsection; the PRESETS row (DICE after NOISE PERC).

---

## Track ergo-flow

### F1: title cards once (`feat(ui): a title card the first time only, none on the way back`)

**Files:** `app/views.h`, `app/views.c` (`cards_value`, `cards_turn`, `show_card`, `global`'s
card), `app/settings.c` (byte 1), tests, journeys, controls.md "Title cards",
settings-format.md.

1. `v->cards`: 0 OFF, 1 ONCE (the default, `views_init` keeps 1), 2 ALWAYS. The menu's value
   `OFF`, `ONCE`, `ALWAYS`; ALGORITHM clockwise steps OFF, ONCE, ALWAYS, counter-clockwise
   back, clamped.
2. `show_card(v, id)`: ALWAYS shows it; ONCE shows THE CHIP and THE SCENE the first time each
   since power-on (`v->cards_seen`, bit id, new `uint8_t` at the struct's end) and never THE
   OSCILLATOR; OFF none. The way back to LIVE from the menu is a wipe, with no card, unless
   ALWAYS.
3. Settings byte 1 holds 0, 1 or 2 (`out[1] = *s->cards`; on load `rec[1] <= 2 ? rec[1] :
   1`). No schema change: an old record's 1 (ON) reads as ONCE, the new default; an old
   firmware reads 2 as on. settings-format.md says so.
4. Tests: emu `a_card_plays_once_then_not` (CHIP from the menu twice: the card the first time,
   not the second; HOME from the menu to LIVE: no card); `cards_always_plays_every_card`;
   `cards_off_plays_none`; sidou-tests `settings.rs` `cards_byte_reads_old_and_new`. The title
   card goldens stay; their tests set ALWAYS where they open a card a second time
   (`into_oscillator_card`).
5. Journeys: explore `chip-back` `press HOME; press HOME; wait 400`; `scene-home` `press HOME;
   press HOME; wait 400`.

### F2: help cards on HOME held (`feat(ui): hold HOME for a help card`)

**Files:** new `app/help_view.c`, `app/help_view.h`; `app/views.h`, `app/views.c` (`global`,
`curtain`, new `home_go`, new `flow_frame`), tests, goldens, controls.md (HOME row,
"Routing" item 2, a "Help" section), docs/art/README.md.

1. **HOME acts on its release.** `global` on HOME's press sets `v->home_down = 1`,
   `v->home_at = v->s.now` and `format_arm = 0`, and consumes it. `flow_frame` (below), once a
   frame (not while `v->p` is NULL): while `home_down` is 1 and HOME is no longer held
   (`v->p->held`, which
   `panel_buttons` keeps), it calls the new `home_go(v)`, which does what the press did before
   (the menu from LIVE, back one view elsewhere, F1's card rule), and clears `home_down`; while
   `home_down` is 2 (this hold opened a help card) the release only clears it. A tap between
   two frames works the same (the press set `home_down`, the next frame sees HOME up). A
   capture (learn, the boot, a card) and SAVE held still take the press first, so `home_down`
   stays 0. Why: the hold must not open the menu first; SAVE already acts on its release. Why in
   the frame and not in `views_buttons`: that runs every main-loop pass, inside the USB drain's
   worst pass (9 instructions from its bound). The HOME gate (TITLE_CARD_INPUT) keeps measuring
   HOME's press in `ui_buttons` (it falls); the navigation now runs in the frame, inside the
   frame's own bounds (TITLE_CARD_WORST_POLL, the passes' bounds), which stay as they are.
2. **The card.** `flow_frame(v, now)` (called in `views_frame` right after
   `learn_view_frame`; it returns at once when `home_down`, `tour` and the help card's done flag
   are all 0, its whole idle cost): if `home_down == 1`, HOME is still held, `now - home_at >=
   600` and no curtain shows, `help_view_open(&v->help, &v->s, card)` for the view that shows, and
   `home_down = 2`. `curtain` is also true while the help card is on top (SAVE then ends it and
   saves nothing). The card: a view pushed with `SCREEN_CUT`, a capture like the title card's
   (its `enter` calls `screen_capture`): any button press or encoder turn closes it and does
   nothing else; notes play; it stays open after HOME's release until then. `flow_frame` pops
   it when its capture marked it done (as `card_frame` does a title card).
3. **Drawing:** the title bar rows 0-2 in `title_fg` on `title_bg`: `HELP` and the card's name
   right-aligned (`LIVE`, `MENU`, `CHIP`, `TRACKER`, `SCENE`, `ALL`) at 2x; then up to 8 lines
   at 2x from row 4, one every 3 rows, 14 characters from column 1: the control left in `hot`,
   what it does right-aligned in `value`. Lines are drawn within 190 cells a frame, the first
   frame's blank first (`help_view` keeps a bitmask of the lines drawn: 1 byte); no canvas.
4. **The cards** (`help_view.c`, flash):

   | Card (shown for) | Lines |
   |---|---|
   | LIVE (LIVE) | SELECT PAGE; ALG PROFILE; PRESETS SOUND; EDIT+K FX; GLO+KEY MUTE; SAVE+K LEARN; SEQ SONG; HOME MENU |
   | MENU (the menu, MIDI) | SELECT MOVE; ALG CHANGE; ALG OPEN; SAVE KEEP; HOME BACK; HOLD HOME HELP |
   | CHIP (CHIP) | SELECT FOCUS; ALG STEP; K1-K4 VALUES; EDIT+ALG SAVE; SAVE STORE; HOME BACK |
   | TRACKER (the song views) | SELECT ROW; ALG VOICE; KEYS AUDITION; PLAY PLAY; REC RECORD; GLO+KEY MUTE; SEQ NEXT VIEW; HOME BACK |
   | SCENE (SCENE) | SELECT PIECE; ALG PIECE; K1 K2 MOOD; KEYS PLAY; HOME BACK |
   | ALL (STATUS, ABOUT, CHARSET, anything else) | HOME BACK; SAVE KEEP; SEQ SONG; GLO+KEY MUTE; HOLD HOME HELP |

   TRACKER's lines follow controls.md at your base (M2d Task 3 gives SELECT the row and
   ALGORITHM the voice); if your base differs, write what controls.md says. A host test
   checks every line fits 14 characters with a space between the two parts.
5. Tests: emu `holding_home_shows_the_help_card_and_a_tap_still_goes_home` (on LIVE: HOME
   pressed and released in 100 ms opens the menu; held 800 ms shows the LIVE card, golden
   `help-live.png`, and the release leaves it; SELECT closes it, LIVE is back, the selection
   untouched); `help_cards_match_their_goldens` (`help-menu.png`, `help-chip.png`,
   `help-tracker.png`, `help-scene.png`); `the_help_card_stays_within_the_frame_budget`
   (200 cells a frame). Every emu test that holds HOME with `m.key(HOME, true)` and checks the
   view before releasing it changes to release first (screen.rs has eight).
6. Journeys: tour, after `live-again`, `step help | The help card | - | hold HOME 800 | text
   HELP` and `step help-close | Close it | - | turn SELECT 1 | view live`. The harness's `help`
   feature (the image contains HELP) then reads present.

### F3: the first-run tour (`feat(ui): three toasts that lead the way after an install`)

**Files:** `app/views.h`, `app/views.c` (`views_boot`, `flow_frame`), tests, controls.md "At
power-on".

1. `views_boot(v, full, home_held)`: `v->tour = full && !home_held` (the full boot because the
   version changed, not because HOME asked).
2. In `flow_frame`, while `v->tour`, once the boot has ended (top not the boot): step 1 toasts
   `TURN K1` for 8,000 ms and keeps `tour_ref` = the cutoff; it ends when the cutoff or the
   selected slot's Amount changed, or after 8 s. Step 2, 1,600 ms later (after the action's own
   toast): `PRESS ENV` for 8,000 ms, until the selected slot's algorithm changed or 8 s. Step 3,
   1,600 ms later: `HOLD HOME FOR HELP` (two lines) for 8,000 ms, until a help card opened or 8
   s. Then `tour = 0`. Fields at the struct's end: `uint8_t tour; uint32_t tour_at; uint16_t
   tour_ref;`. Not saved: the version is saved after the boot as before, so the next boot has
   no tour.
3. Tests: emu `the_tour_follows_the_first_boot_after_an_install` (a store whose version
   differs: after the full boot `TURN K1` toasts; K1 +1: `PRESS ENV` 1.6 s later; ENV:
   `HOLD HOME FOR HELP`; HOME held: no more toasts); `no_tour_when_home_asked_for_the_boot`;
   `no_tour_on_a_normal_boot`. STATUS idle and LIVE noise within their shares.
4. Docs: controls.md "At power-on" (the tour). docs/TODO.md's UX-pass entry: help cards and
   the first-run tour done.

---

## Track ergo-harness

All in `host/crates/ergo` (and its tables), with unit tests in each module. No firmware
change. Each H item is a commit `fix(ergo): ...`.

- **H1 roles for the readability check.** `readability.rs`: a run's class also depends on its
  colours, read from the firmware's themes (`sidou_sys::theme_get(0)` and `(1)`): a run whose
  foreground is that theme's `label`, `hint` or `dim` (on its `bg`) is never `play` (it is
  `glance`); a run in `dim` on `bg` is exempt from the contrast rule (inactive state, as WCAG
  exempts it) unless the `hint` class below matched it. Tests:
  `label_and_hint_coloured_text_is_not_play_text`, `dim_text_is_exempt_unless_it_is_a_hint`.
- **H2 the table and transitions.** `tables/readability.txt`: new class `hint` (judged for
  contrast whatever its colour, never for size) and pattern `hex` (a run of two-hex-digit tokens
  only: `00`, `04 05 06`); the rules, in this order, before the `*` rules: `status | glance |
  any`, `learn | glance | any`, `tracker | glance | hex`, `* | hint | words
  SELECT,ALGORITHM,ALG,HOME,MOVE,STEP,CHANGE,BACK,CANCEL,VOICE,MODEL,PROFILE,BPM,OL,K1`; replace
  `tracker | play | any` by those. `judge` skips a shot taken while the screen's raster, a wipe
  or a title card runs (read `screen.raster_frames`, `screen.wipe` and the top view through
  `sidou_sys::ilp32`): the purple `M` and `6` on CHIP were the raster over the border. Tests per
  rule; `the_raster_is_not_read`.
- **H3 set-up steps.** journeys grammar: step flag `setup` (two hands allowed: no
  `JOUR.hands`); the header documents it; perform's `learn-cc` and keep's `theme` get it. Test.
- **H4 the EDIT sweeps.** `knobs.rs`: the `K1.edit`..`K3.edit` setup becomes `press ENV; press
  ENV; holdturn EDIT K4 32` (OVERDRIVE at Mix 127); a sweep of a control the algorithm does not
  use (`chip_view_label` returns 0) is skipped and reported as `skipped`; the dead-zone rule's
  brightness floor is 8 % (`config.rs`, `jnd_centroid_pct` stays 5 for the per-detent rule).
  Tests.
- **H5 percussive presets.** `MUS.quiet`: a preset whose sustain is 0 is judged by its loudest
  50 ms RMS window in the first 300 ms, not the held note's RMS. Test.
- **H6 bright leads.** `MUS.bright`: `centroid_max_hz` 6,500 for a preset with sync or ring on
  (SYNC LEAD), 5,000 otherwise; config key `centroid_max_hz_lead`. Test.
- **H7 pots.** `KNOB.fine` does not apply to a pot (detents `None`); `KNOB.jump` and
  `KNOB.uneven` measure a pot's steps only between points at or above -40 dB of full scale.
  Tests.
- **H8 documented idle controls.** `consistency.rs` `DOCUMENTED_INERT` loses `("tracker",
  "SELECT")` (controls.md documents it) and, if controls.md at your base documents ALGORITHM in
  TRACKER (M2d Task 3), `("tracker", "ALGORITHM")`; `CONS.inert.*` counts only views with
  something to change (not `status`, `about`, `charset`). Tests.
- **H9 K's meaning.** `CONS.k-meaning` is raised only for a view where K1 moved a parameter
  and neither the screen nor the popup shows that parameter's label (the names of
  `learn_view_param_name`, or a slot control's `chip_view_label`). Test with CHIP (AGE on
  screen: consistent).
- **H10 recoverable steps.** A `destructive` step raises no `SAFE.destructive` when a later
  step of the same journey has `recalled` and passes, with a `remember` before the destructive
  step. Test.
- **H11 `--compare OLD NEW`.** Both are `findings.json` files (or run directories holding
  one). Prints `gone N`, `new N`, `still N`, then each id with its title under its heading,
  and writes `compare.json` (`{"gone": [...], "new": [...], "still": [...]}`) next to NEW;
  exit 0. Add it to `USAGE`. Tests on two small fixture files.

Checks: `cargo fmt`, clippy, `cargo test --locked --release -p ergo`, the workspace tests, and
a full harness run on main's fm1 build (`cargo run -q --release -p ergo -- --out
../build/scratch/ergo`) with its `--compare` against `../build/ergo/findings.json`: the report
lists the ids that went (expected: READ.size.learn, READ.size.status, READ.size.tracker,
READ.size.chip, JOUR.hands.perform.learn-cc, KNOB.dead.K1-3.edit, MUS.quiet.noise-perc.a4/f3,
CONS.drift.tracker.select, CONS.k-meaning, KNOB.fine.MASTER, MUS.bright.sync-lead.a4) and
every new id with its reason. Do not run the emu-gate or touch firmware.

---

## Later

**Later-1, TRACKER (after M2d Tasks 3-4 merge; `app/tracker_view.c`).** For
READ.contrast.tracker, READ.contrast.learn, HEUR.tracker.play-state-tiny,
HEUR.tracker.audition-no-feedback, HEUR.tracker.mute-other-voice-quiet:
1. Roles: `OL`, `BPM` and the hint line in `hint`; the instrument on the current row's bar in
   `value` (light red on dark grey is 2.05:1); the command on the bar in `value`; MUT and REC
   no longer 1x in `alert` (4.42:1): MUT 2x in `alert` beside the pattern, REC 2x in `alert` at
   columns 18-23 of rows 0-1 when the voice is not muted, else at 1x in `hot`.
2. The play mark at 2x at columns 22-23 of rows 0-1: FONT_PLAY in `hot` while playing, the stop
   square in `label` while stopped (REC then moves to column 18, row 1).
3. Audition: while a key auditions, the current row shows the note ghosted at 2x in `label` with
   the instrument's number (`A-4 02`), and the hint line names the instrument (`I02` and its
   name, up to 16 characters at 1x in `label`).
4. Mutes: row 2 shows all three voices' numbers (the shown one too) at 1x: `voice_text[v]`,
   `dim` while muted, `GRID_ATTR(th->bg, th->hot)` while soloed.
Tests: golden updates of the `tracker*.png`, an audition golden, a contrast test like T2's.

**Later-2, CHIP's profile words (after M2b Task 6; `app/chip_view.c`, `app/live_view.c`).** For
HEUR.chip.profile-jargon: a word under the profile on CHIP's names row and after it on LIVE's
band, from decision record 0006's measurements: R2 `BRIGHT` (566 / 10,484 Hz at FC 256 /
1024), R3 `DARK` (239 / 6,732), R4AR `CLASSIC` (reSIDfp's default), R5 `CLEAN` (the 8580),
WORN `GRITTY` (darker, narrower, the strong waveforms and leakage), a USER profile `USER`.
`sid_chip_profile` gets a `word`; the popup is not used (L2); LIVE shows the word at 1x in
`label` on row 21 after `CHIP 6581`.

## No work

SAFE.undo and JOUR.nyp.keep.undo (M2d Task 3's undo), the other JOUR.nyp ids (M2d Tasks 3-7,
M2c Task 8), the seven ids f931433 removed, and HEUR.harness.menu-nav-blind (done).
