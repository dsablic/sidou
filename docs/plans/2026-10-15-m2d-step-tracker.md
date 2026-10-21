<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# M2d: STEP and TRACKER, implementation plan

<!-- REUSE-IgnoreStart -->

> **For Claude:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> to implement this plan task by task.

**Goal:** make and play songs on the FM-1. A TRACKER view in GoatTracker's layout (a moving
bar over still rows by default, GoatTracker's scrolling as an option), with editing from the
FM-1's keys, encoders and buttons; pages for the orderlist, the instruments and the tables;
a STEP view where 16 keys are 16 steps and a held step plus a knob is a parameter lock;
transport, mute and solo; and REC, recording the keys into the pattern. All of it edits
the same GT2 song (M2c), so a song started on the FM-1 opens in GoatTracker.

## Status: M2d complete (Tasks 1-7), 2026-10-22

All seven tasks are merged on main. Hardware: nothing of M2d has run on a unit yet; the
button ids (LFO, GLO, ARP, SEQ, PLAY, REC) and the whole of "make a song on the device"
([hardware-test.md](../hardware-test.md#14-make-a-song-on-the-device-m2d)) wait for flash day.

| Task | Done | Commit | Result |
|---|---|---|---|
| 1. The song views' frame | yes | `64bb04e` | SEQ, PLAY, REC, GLO's mute and solo, the header, jam keys |
| 2. TRACKER, shown | yes | `95d713e`, `6e41eb6` | the moving bar and scrolling; TRACKER ROWS persists (settings schema 2) |
| 3. TRACKER, edited | yes | `857250c` and fixes | note, instrument, command and data entry, insert and delete, undo and redo through the player's edit queue |
| 4. ORDER, INSTR and TABLE | yes | `807b006` | the three pages, with the audition that keeps live state (decision B, below) |
| 5. STEP | yes | `0caf368` | 16 keys are 16 steps; a held step plus a knob is a lock; K4 writes `7XY` |
| 6. REC from the keys | yes | `1ac4bb0` | notes at the nearest row, K1 and K2 as locks, NEAREST, OFF and COUNT-IN |
| 7. Docs and the status | yes | this commit | controls, art, hardware test, TESTING, CHANGELOG, this record |

**Two deviations from the plan's text.** The menu lists TRACKER only; SEQ reaches STEP and the
pages (Task 5). TRACKER is one voice at 2x with strips for the other two, not the prototype's
three columns (the readability pass, `64b6f97`). REC's recording leaves TRACKER's cursor pinned
while the song plays rather than following it; this and the missing latency compensation are
in [TODO](../TODO.md) for the ergonomics review and flash day.

**Resources at the end of M2d** (measured 2026-10-22 on main `8f530eb`, a fresh `fm1` build):

| | Measured | Bound | Left |
|---|---:|---:|---:|
| Image, `sidou_app.bin` | **493,840 B** | 512,000 (M2f's entry, `IMAGE_CEILING`) | 18,160 |
| Image against M3's start | 493,840 B | 506,000 | 12,160 |
| RAM region free | **4,256 B** | 4,096 each | 160 |
| POOL free | **4,264 B** | 4,096 each | 168 |
| RAM region + POOL free | **8,520 B** | 7,000 (`FREE_RAM_MIN`) | 1,520 |
| RAMTEXT | **46,592 B** | 49,152 | 2,560 |

Against M3's targets at its start (the M3 plan's "Before M3" table): the image is inside its
506,000 (12,160 B of room), RAMTEXT has 2,560 B free against the 2,048 asked, but **the RAM
region (4,256 B) and POOL (4,264 B) are each about 860 B short of the 5,120 B asked**. That is
the headroom pass's to find before M3 starts; M2d does not close it.

The image is from `ls -l` of the fresh build; the RAM figures are the output of
`the_song_region_and_the_free_ram` (`host/crates/emu-gate/tests/song.rs`), which reads the
link map. **The "free RAM >= 16,000" gate above is out of date and is not met, and was not
the gate in force:** it was a standalone projection. The gate in the code is
`FREE_RAM_MIN` = 22,000 less the merged plans' RAM budgets (M2a 2,000, M2d 6,000, M2e 2,000,
M2f 5,000) = **7,000**, with each region keeping 4,096 B; both hold. Likewise the image
bound "<= 440,000" is M2d's schedule entry, but the schedule's `IN_PROGRESS` is M2f's entry
(512,000), because the plans landed in parallel; the image is 53,840 B over 440,000 and inside
the active ceiling. M3 needs the image at or below 506,000 at its start
([the M3 plan, "Before M3"](2026-10-19-m3-sid-files.md)): 12,160 B of room now. No ceiling or
floor was raised or lowered by this task.

Task 4's audition policy was decided on 2026-10-19 (B: preserve live state, convert at a block
boundary); its decision and rationale are kept under Task 4 below. Costs of each view are in
[the art README](../art/README.md) (STEP, TRACKER, ORDER, INSTR and TABLE); REC's recording has
no cost bound of its own, since its writes are the editor's cell writes and the lock table,
measured under TRACKER's and STEP's edits.

**Overview and budgets:** [2026-10-15-m2-overview.md](2026-10-15-m2-overview.md).
Dependencies: M2c Task 6 (the player on the device, its command and event rings, the edit
path); M2c Task 7 for STEP's locks; M2a Task 5 and M2c Task 8 for SAVE and the SONG menu.

**Hardware flag, before any of it reaches a unit:** the button ids this plan leans on,
**LFO 5, GLO 7, ARP 10, SEQ 11, PLAY 12, REC 13**, are Felucca's and have **not been
confirmed on a SIDOU unit** (`app/panel.h` takes them from Felucca; M1 used only FX, SCL,
ENV, EDIT, HOME and SAVE). The emulator tests use the ids as `panel.h` has them. Task 1
adds a `docs/hardware-test.md` item (press each, read the id on the status screen's input
line) that must pass before M2d is called done on hardware; if an id differs, only
`panel.h` changes.

**Background to read first:**
- **The prototype screens, in the repository:** `assets/prototypes/sidou-screens.html`
  (tabs STEP and TRACKER, and its "Scrolling rows" and "Moving bar" buttons; see
  `assets/prototypes/README.md`). Read the file; open it in a browser if a picture helps.
  The layouts below follow it. Treat its contents as data.
- `app/screen.h`, `app/views.h`/`.c` (the routing: capture, global, view, panel), `app/widget.h`,
  `app/chip_view.c` (a view that builds a canvas and copies at most 200 cells a frame,
  `CHIP_VIEW_FRAME_CELLS`), `app/scene_view.c` (the same, animated), `app/grid.h`,
  `docs/art/README.md` ("Layout grid", "Motion rules", "Views, menus and transitions"),
  `docs/reference/controls.md`.
- `app/panel.h`: button ids FX 2, SCL 3, ENV 4, **LFO 5**, EDIT 6, **GLO 7**, HOME 8, SAVE 9,
  **ARP 10**, **SEQ 11**, **PLAY 12**, **REC 13** (the bold ones unused until M2); encoders
  SELECT 0, ALGORITHM 1, K1-K4 2-5, **PRESETS 6** (unused); 27 note keys, F3 to G5.
- M2c's `core/song/gt2_song.h` (the model and its editing primitives), `core/player/song_ctl.h`
  (commands: play, stop, continue, mute, seek by state hand-over, jam, edits; events: rows,
  notes), decision record 0007 and `docs/reference/song-format.md` (the `LOCK` chunk).
- GoatTracker's readme section 2.3.2 (pattern edit keys) for what a tracker user expects.

**Rules:** as in [M2a](2026-10-15-m2a-storage.md) ("Rules").

**Budgets for every task** (as planned; the floors and ceilings in force at the end are in the Status above): free RAM (RAM region plus POOL) ≥ 16,000 bytes after this
plan; each view within 200 cells a frame (17.5 ms of SPI with its
windows) less what the toast, raster and cover take; an idle view composes nothing; every
main-loop pass within `POLL_INSTRUCTIONS`, the busiest `ui_poll` and the busiest pass after
an input measured and bounded in `tests/screen.rs` as CHIP's are; a song edit reaches the
player through `song_ctl` (never a write into the song the interrupt reads); RAM: each
view's canvas and state reported (the overview budgets ~6 KB for all of them); image ≤
440,000 bytes after this plan.

**The song views, and how they are reached** (proposed here; Task 1 documents the final
form in `docs/reference/controls.md`):
- **SEQ** opens the song views from anywhere and steps through them:
  STEP → TRACKER → ORDER → INSTR → TABLE → STEP. HOME goes back to the view before (LIVE).
  The menu also lists STEP and TRACKER.
- **PLAY** plays the song from the orderlist position the view shows, and stops it; with
  EDIT held, PLAY continues from where it stopped. While it plays, the keys outside the song
  views play chip B (M2b Task 4, KEYS AUTO).
- **SAVE** pressed and released in any song view saves the song (M2a Task 5's table,
  M2c Task 8); held with a knob turned it opens MIDI learn, as everywhere.
- **REC** turns record mode on and off (the views say so in red on row 0).
- **Mute and solo, in every view** (design section 7): **GLO held** and one of the three
  leftmost note keys toggles that voice's mute; GLO held and the next three keys solo voice
  1, 2 or 3 (again: unsolo). The keys do not sound while GLO is held.
- **The keys in the song views** audition the song's current instrument on chip A (a jam
  note through `song_ctl`, on the cursor's voice), not the instrument, so what you hear is
  what goes into the pattern. In LIVE and the other views they play the instrument on the
  keys' chip (M2b Task 4: chip A with no song playing, chip B while one plays, or A with
  KEYS A).

---

### Task 1: The song views' frame: transport, mute and solo, the header

**Files:** `app/song_view.h`/`.c` (new: what the song views share: the header row, the
voice heads, transport state, the mute/solo layer, the jam), `app/views.c`/`.h` (SEQ, PLAY,
REC, GLO routing; the menu's STEP and TRACKER items), `app/panel.c` (GLO's layer), tests.

**Spec:**
- **Row 0, the header** (reverse video, as the prototype): the view's name, `▶` or `■`,
  the tempo in BPM (from the song's tempo, the multispeed, the rows a beat: 4 by default,
  the `MIDI` chunk's value once M2e lands; funktempo shows the average), the position
  (`ORD 03 ROW 1A`), `REC` in red when recording.
- **Row 1, the voice heads:** V1 V2 V3, each with its current waveform's colour (pulse
  cyan, saw yellow, triangle light green, noise light grey) and a one-cell envelope meter
  (from chip A's ENV3 and the event ring's gate states); MUT in light red when muted, SOL
  when soloed.
- **Transport** through `song_ctl`: PLAY from the shown position, stop, EDIT + PLAY
  continue; the views follow the player's row events (the bar or the scroll); with the
  transport stopped they follow the cursor.
- **Mute/solo layer** (GLO held + keys) in every view, before the panel, so a muted voice
  never sounds a key; the state shows in every song view and in LIVE's status line.
- **Jam:** a note key in a song view sends a jam note (the current instrument, the cursor's
  voice) and its release; octave as the panel's.
- The first view registered is a minimal TRACKER placeholder (rows only), replaced in Task 2.

**Tests:** host: the routing (SEQ's cycle, PLAY/stop/continue, REC, GLO layer consuming
keys, SAVE's press in a song view reaching `save_key`'s song target), the BPM formula (tempo 6, 4 rows a beat, PAL: 125.3 BPM; multispeed 2: 250.6), the
header's text; fm1-emu: SEQ opens STEP, PLAY plays the demo song (audio bit-exact with the
host as in M2c Task 6), GLO + key 1 mutes voice 1 within a frame (its writes stop after a
key-off), goldens of the header and heads.

**Docs:** `docs/reference/controls.md` (a "Song views" section: SEQ, PLAY, REC, GLO,
SAVE, the keys), `docs/hardware-test.md` (the button-id check above, the song views'
buttons), `CHANGELOG.md`.

**Commit:** `feat(app): the song views' frame: transport, mute and solo, the header`

---

### Task 2: TRACKER, shown: the moving bar, and scrolling as an option

**Files:** `app/tracker_view.h`/`.c` (new), `app/song_view.c`, `docs/art/README.md`
(a "TRACKER" section), tests, goldens.

**Layout** (the prototype's, on the 30 x 30 grid):
- rows 2-26: 25 pattern rows. Columns 0-1 the row number in hex (white on the playing row,
  yellow every 16, light grey every 4, grey otherwise); then for each voice, from columns
  3, 12 and 21, eight cells `C-401308`: the note (white; `---` empty, `===` key off,
  `+++` key on), the instrument (light blue; `..` none), the command (yellow) and its data
  (light green); `...` for an empty command. A muted voice is drawn in dark grey.
- column 29: the position in the pattern (a pip on a rule);
- row 27: a rule; row 28: the orderlist around the current position for each voice
  (`OL 03 04 R2 05 +3 RST00`); row 29: the current row's command spelled out in yellow
  (`V2 4 22 VIBRATO`), as the prototype.

**The two modes:**
- **Moving bar (default):** the rows stand still and a highlight bar (light blue on blue)
  moves down them. The window shows the 16-row page the play position is in, with 4 rows
  of context above it, and moves a page at a time. A row step changes 2 rows (about 60
  cells); a page turn changes the window (up to 725 cells), which the 200-cell copy spreads
  over 4 frames, top first, behind the bar.
- **Scroll (option, `TRACKER: SCROLL` in the menu, persisted):** GoatTracker's way, the
  playing row fixed at the window's row 12 and the rows moving under it. Every row step can
  change every row: at tempo 6 (a row each 3 frames) that is about 240 cells a frame
  against the 200 budget, so the copy lags behind at fast tempos. Measure it, and say in the
  docs at which tempo the scroll stops keeping up (the bar always does).

**Spec notes:** the view builds into its canvas from the song (read only; the interrupt
plays the same song, but only edits through `song_ctl` change it, and those happen
between frames: reading from the main loop is safe as long as edits are applied by the
interrupt and the view re-reads after an edit event) and copies within its cell budget.

**Tests:** host: the layout's text for fixed songs (each special note, an empty command,
a muted voice); the bar's window and page turns; the scroll's window; fm1-emu: goldens at a
fixed frame in both modes; **the costs:** the demo song at tempo 6 and a test song at tempo
3, 30 s each: cells a frame (average, most), SPI a frame (average, worst), main loop a
frame and the busiest `ui_poll`, in both modes; the bar within 200 cells every frame, the
scroll's lag reported; the numbers into `docs/art/README.md`.

**Commit:** `feat(tracker): the TRACKER view, a moving bar by default and scrolling as an option`

**As built — TRACKER ROWS persistence:** settings schema 2 appends byte 106 (`0` BAR,
`1` SCROLL), leaving every schema-1 field at its original offset. Schema-1 records load
with BAR; invalid mode bytes also select BAR. The unchanged v1 fixture and a new v2
fixture cover compatibility; host tests cover both modes, the five-second background
save from a TRACKER ROWS menu edit alone, and reopening TRACKER in its saved mode after
a simulated power cycle. The menu wakes settings through `live.settings_dirty`, as
CARDS does; no gather is added to idle frames. Settings point directly to the song
view's existing mode byte and apply it before the first frame at boot. No song or store
code changes. A later settings addition can append its field under the next schema.
The dirty-mark mutation check failed at `TRACKER ROWS alone must save` (exit 101)
with the mark removed and passed after restoring it (exit 0). A diff-only review
found no correctness defects. The v1 fixture is unchanged; no golden was regenerated.

Persistence follow-up resources (Task 2 base → schema 2): image **428,724 → 428,792 B**
(sidou-pack); free RAM **18,024 → 18,020 B** (gate 14,000), RAM region used
**53,208 → 53,212 B** of 65,536; POOL **346,560 B** of 352,256 and RAMTEXT
**44,404 B** of 49,152, both unchanged. No budget or tolerance was changed.

Tight gates, before → after: STATUS idle **7,496.6 → 7,496.6 instructions/frame**
(`<7,500`), worst idle `ui_poll` **591 → 591**, zero cells or SPI windows;
HOME into THE OSCILLATOR **429 → 429 input instructions** (`<430`), busiest pass
**71,394 → 71,394**, input plus pass **71,823 → 71,823**, at most 200 cells /
17.14 ms SPI. The extra gathered byte costs idle frames nothing.

Every TRACKER measurement below is identical before and after persistence:

| Case | Instructions/frame | Worst `ui_poll` | Cells/frame (average, most) | SPI/frame ms (average, worst) |
|---|---:|---:|---:|---:|
| Idle | 8,100.84 | 1,327 | 0, 0 | 0, 0 |
| REC | 8,302.76 | 3,946 | 0.12, 3 | 0.011, 0.263 |
| Playing, 100 frames | 25,679.69 | 29,808 | 21.94, 86 | 1.899, 7.559 |
| GLO and a key | 9,846.60 | 27,413 | 1.24, 31 | 0.108, 2.711 |
| SIDOU LOOP, BAR, 30 s | 27,580.02 | 31,432 | 25.57, 129 | 2.218, 11.345 |
| SIDOU LOOP, SCROLL, 30 s | 50,985.75 | 34,036 | 83.36, 200 | 7.296, 17.558 |
| Tempo 3, BAR, 30 s | 49,781.60 | 38,358 | 61.58, 200 | 5.349, 17.785 |
| Tempo 3, SCROLL, 30 s | 89,897.85 | 36,823 | 100.43, 200 | 9.076, 17.580 |

TRACKER input / busiest following pass, also unchanged: SEQ from LIVE **4,155 /
47,964**, SEQ in TRACKER **228 / 5,035**, REC **200 / 5,035**, PLAY **1,880 /
29,605**, REC again **201 / 30,368**, GLO and a key **71 / 29,709**. All remain
under the existing 4,600 input / 55,000 pass bounds. The 30 s runs each cover 750
frames: BAR/SCROLL cut-short counts are **0/243** for SIDOU LOOP and **34/24** at
tempo 3, unchanged; all frames stay within 200 cells and 20 ms SPI.

Checks (exit codes): fm1 build **0**; flash-spike build **0**; GCC 13 on
`app/settings.c` **0** and `app/views.c` **0**; `cargo fmt --all --check` **0**;
`cargo clippy --locked --workspace --all-targets -- -D warnings` **0**;
`cargo test --locked --workspace --release` **0**;
`SIDOU_SANITIZE=undefined cargo test --locked --workspace` **0**;
sidou-pack with CI's app/loader/output arguments (cached inputs, `--offline`) **0**;
`cargo test --locked --release -p sidou-pack -- --nocapture` **0**;
full emu-gate with both firmware build paths and `--nocapture --test-threads=1`
**0**; `uvx --offline --from 'reuse[charset-normalizer]' reuse lint` **0**;
`tools/web-demo --check` **0** (cached dependencies only).
The base build, base screen budgets, base SRAM/song-region tests and base package
all exited **0**; the final targeted STATUS, title-card and RAM gates also exited
**0**. The initial new-feature tests failed as intended (**101**); the complete
host app suite then passed (**0**), as did the restored dirty-mark regression
(**0**). Nothing remains open for this follow-up. Hardware flags from Tasks 1/2
and provisional flash timings remain unchanged; no hardware was accessed.

---

### Task 3: TRACKER, edited

**Files:** `app/tracker_view.c`, `core/song/gt2_song.c` (any editing primitive missing),
`core/player/song_ctl.c` (the edit commands), tests.

**Controls in TRACKER** (proposed; documented in `controls.md`):
- **SELECT** moves the cursor a row (EDIT held: a readable page of 8); **ALGORITHM** moves it to
  the next voice and switches the shown voice. Editing uses the redesigned 2×, 12-row view,
  not the original three-column layout. The row number is a block and the edited field is
  reverse yellow at 2×, including command/data while selected.
- **REC on (edit mode):** a note key writes its note (the panel's octave) and the current
  instrument into the cursor's row and voice, and the cursor advances by the step size;
  **ARP** writes a key off (`===`); **LFO** clears the row's note (rest), a second LFO press
  on the same row clears the whole row; **K1** the row's instrument, **K2** its command (0-F), **K3** the data's
  high nibble, **K4** the low one (so a row is edited without moving across its columns);
  **PRESETS** turned sets the current instrument (shown in row 29 when it changes);
  **EDIT held + PRESETS** inserts (clockwise) or deletes (counter-clockwise) a row at the
  cursor, pattern length kept as GoatTracker does; **EDIT held + ALGORITHM** sets the step
  size (0-16).
- **REC off:** the keys audition (Task 1's jam); knobs do what they do in LIVE.
- **One step of undo:** EDIT held + LFO undoes the last edit (the inverse operation kept in
  RAM; a second press redoes).
- **Every edit reaches the interrupt as M2c Task 6's O(1) steps only**, including while the
  song plays: a note, instrument or command entered is a **cell write** (the row's 4
  bytes); an insert or delete of a row is a **rebuild** the main loop makes in free arena
  space (M2c Task 2) followed by one **index swap**; the view redraws the changed row when
  the edit's acknowledgement comes back. Locks (M2c Task 7) move with their rows (the
  `LOCK` table rebuilt and swapped the same way).
- **The big edits, decided:** an edit that needs many swaps (deleting a pattern used in
  many orderlists, renumbering patterns) is split into one swap per orderlist (at most 3
  per subtune, 96 for 32 subtunes) queued as separate commands; `SONG_EDITS_PER_BLOCK`
  (8) spreads them over blocks, so the worst block stays inside `PLAYER_WORST_BLOCK` (60).
  The song plays the old form until the last swap of a split edit lands: a split edit is
  applied in an order that keeps every intermediate song valid (orderlists first, the
  pattern last). A rebuild that cannot find free space runs compaction first (M2c Task 2),
  in slices within the poll allowance; while it runs, further edits wait (toast
  `TIDYING`).
- No edit while a save streams (M2c Task 8: `SAVING`).
- GoatTracker's rules are kept (an edit that would break one is refused with a toast).

**Tests:** host: each control's edit on the model (an independent Rust model of the
pattern compared after each); insert and delete at the ends; undo and redo; refused edits;
edits during playback equal (in the player's output) the edited song played from the same
state; no edit applies anything but cell writes and swaps (a test hook counts them); a
pattern deletion across 32 subtunes keeps every block within `PLAYER_WORST_BLOCK`; fm1-emu: enter a four-note phrase with REC on, play it, the audio bit-exact with the
host's render of the expected song; the edit's pass within the poll allowance; goldens.

**Commit:** `feat(tracker): edit patterns from the keys and encoders, through the player's edit queue`

**As built — Task 3 (base `64b6f97`):** editing uses the readable, one-voice, 12-row TRACKER.
SELECT moves rows and EDIT + SELECT moves its **8-row page**, rather than the old plan's 16;
ALGORITHM changes both cursor and shown voice. REC pins the edit cursor while the song keeps
playing; with REC off the bar follows playback. The number is a yellow block, note/instrument
focus is reverse yellow at 2×, and command/data expands to 2× while selected. K1–K4 use the
value popup. LFO clears the note, a second press on the same row clears all four fields;
EDIT + LFO toggles undo/redo, also with REC off. PRESETS selects the entry/audition instrument;
EDIT + ALGORITHM selects step 0–16. Note entry wraps within the pattern after advancing.

The old WIP primitives were ported, not cherry-picked: fixed-length push/pull, order replacement
and PUSH lock movement. Independent Rust models cover 404 shifts at every position of lengths
1/2/7/64/128, dropped-tail locks, cancellation, pending conflicts and 400 seeded replacements.
The editor retains a private rebuild under backpressure, compacts one acknowledged swap at a
time, and retains just the lost row and its locks for undo (**200 B on ILP32**). Idle edits now
use the interrupt queue too. Host-only hooks count CELL and SWAP applications; no bulk edit
operation was added to the interrupt. The 32-subtune deletion regression performs 96 order
replacements followed by pattern removal: **97 swaps, zero cell writes**, at most **8/block**
idle and **1/block** playing, valid intermediate songs, acknowledged arena reuse, and playback
state comparison against an independently edited song.

Live lock edits required more than moving the extension records: old lookahead caches could
apply a moved lock to its old row. Prospective caches now publish with the pattern swap; if a
row advances before publication the command waits for a refreshed sequence-checked snapshot.
Undo restores discarded locks before its completion ticket advances. Character-row updates
continue while the edit owns the lookahead buffers. Regressions cover these races, sequence
wrap, guard changes after acceptance, cancellation of unsubmitted FULL edits, and keeping a
pending edit's transport alive across HOME/rebinding attempts. A diff-only review found these
lifecycle/cache issues; they were fixed, not waived.

The input cost of preparing an insert was **6,629 instructions**, failing the unchanged
4,600 input bound. Rebuild preparation moved to the frame's bounded editor poll: insert is
now **410**, delete **412**, undo **392**, redo **383**; their busiest passes are
**19,995 / 19,979 / 23,249 / 23,233** (bound 55,000). Fast value turns finish the preceding
acknowledgement before reading the next value; the emulator checks that a multi-detent command
and nibble edit really produces `423`, rather than silently dropping detents. Other edit
inputs/passes: row **243 / 15,795**, instrument **1,146 / 30,872**, command
**1,409 / 28,530**, high data **1,421 / 28,002**, low data **1,422 / 27,597**,
key off **1,232 / 20,661**, rest **1,205 / 20,476**, whole-row clear **1,215 / 20,476**.
While playing: instrument **1,547 / 30,872**, command **1,829 / 29,045**, data
**1,433 / 28,244**, insert **414 / 35,960**, under the same unchanged bounds.
The fragmented-arena emulator test shows TIDYING and finishes within the primitive's move
bound: input **410**, busiest pass **27,547**, **281** further display frames to completion.
It awaits the algorithmic preparation/acknowledgement bound rather than assuming that a
multi-slice compaction completes within a fixed four-second observation window.
The four-note emulator test compares the panel-built song with the independently expected
serialized song, then checks its playback bit-exact against the host from a fresh SID state:
the audition's oscillator/envelope history is deliberately excluded from that comparison.
The existing live-edit stream test separately covers uninterrupted host/target playback.
The first full emulator run found that this replay helper prepared a later edit before
consuming the preceding block's swap acknowledgement. It now polls acknowledgements before
replaying edits at their recorded blocks, as the device does between blocks. The unchanged
bit-exact test passes at edit blocks **1214, 1214, 1215, 1215**; neither its audio comparison
nor any performance bound was weakened.

Resources, base → Task 3: sidou-pack app **425,140 → 431,364 B** (≤440,000);
RAM region used **59,740 → 60,012 B** of 65,536; POOL **346,560 B** of 352,256,
unchanged; free RAM together **11,492 → 11,220 B**. The merged-plan floor is **7,000 B**
(M2c 22,000 less M2a 2,000, M2d 6,000, M2e 2,000 and M2f 5,000), not the standalone
16,000 projection above. RAMTEXT **45,108 → 45,232 B** of 49,152. The existing 2,700-byte
canvas is reused; the editor adds no canvas. The link script explicitly starts `.text` after
the RAMTEXT image so newly aligned flash constants cannot insert an unrepresented binary gap;
the contiguous-image assertion and every resource bound remain unchanged.

TRACKER costs, base → Task 3 (instructions/frame and busiest `ui_poll`):

| Case | Instructions/frame | Busiest `ui_poll` | Cells/frame, average / most | SPI ms/frame, average / most |
|---|---:|---:|---:|---:|
| Idle | 8,016.52 → 8,117.52 | 1,268 → 1,369 | 0 / 0 | 0 / 0 |
| REC entry | 8,283.44 → 9,938.56 | 5,379 → 19,473 | 0.68 / 17 | 0.059 / 1.480 |
| Playing with REC on | 25,776.50 → 8,191.32 | 23,900 → 16,091 | 0.01 / 1 | 0.001 / 0.093 |
| GLO and a key | 9,585.72 → 9,735.80 | 18,843 → 19,283 | 0.44 / 11 | 0.038 / 0.961 |
| LOOP BAR, 30 s | 27,660.83 → 27,820.17 | 23,900 → 24,264 | 30.42 / 161 | 2.625 / 14.069 |
| LOOP SCROLL, 30 s | 59,823.88 → 60,035.58 | 22,811 → 23,127 | 59.86 / 177 | 5.285 / 15.559 |
| Tempo 3 BAR, 30 s | 46,521.16 → 46,743.55 | 24,193 → 24,559 | 56.53 / 173 | 4.869 / 15.093 |
| Tempo 3 SCROLL, 30 s | 64,910.23 → 65,209.19 | 21,169 → 21,535 | 41.04 / 174 | 3.646 / 15.288 |

The four 30-second runs' cells/SPI are unchanged, with **zero** cut-short frames in both
builds. REC entry changes the cursor as well as the header; playing with REC on now keeps
that cursor still, explaining the lower cost. All existing TRACKER bounds are unchanged.
Common view housekeeping adds **2 instructions/frame** outside song views: STATUS idle
**7,469.4 → 7,471.4** (<7,500), LIVE noise **154,438.13 → 154,440.13** (<155,000).
HOME is **426** (<430), CHIP K1 **211** (<215), SELECT to RULE 30 **227** (<230), and the USB 16-CC drain
**7,991** (<8,000), with zero losses in the MIDI flood cases.
PLAYER_WORST_BLOCK remains **60**. Across 193 runs of 145 songs, the measured worst block
is **5,052 → 5,121 instructions**, **39.47 → 40.01/sample**; eight edits without a call
cost **560 → 629**, and the worst call remains **4,492**. The analysis slice remains
**33,819** (≤39,000), its worst complete main-loop pass **70,111** (≤120,000).
With both chips, the tap, ISR and worst slot switch, the song player leaves **165.5
instructions/sample** below the 2,700 audio target. No gate or tolerance was loosened.

Task 3 final checks (all exit **0**): fm1 configure/build; flash-spike configure/build;
GCC 13 on every changed app/core C file; `cargo fmt --all --check`; workspace/all-target
Clippy with `-D warnings`; locked release workspace tests; locked UBSan workspace tests;
CI-argument sidou-pack and fresh sidou-pack tests; the complete emu-gate with both
`SIDOU_FM1_BUILD` and `SIDOU_FM1_SPIKE_BUILD`, `--nocapture --test-threads=1`
(the corrected full rerun, with `--no-fail-fast`); offline REUSE lint; and
`tools/web-demo --check` using cached dependencies. Release and UBSan app suites each
pass **891 tests**, with 3 pre-existing ignored tests. The targeted four-note phrase,
live-edit replay, stopped/playing edit-cost and algorithm-bounded TIDYING tests also exit
**0**. The base build, base screen-budget run, base pack and base player-budget run exit
**0**. Initial red tests and the first full replay failure are described above; all final
production checks are green. Task 4's unregistered preflight regressions are not claimed
as passing production tests.

Goldens: `tracker`, `tracker-v2`, `tracker-v3`, `tracker-play` and `tracker-scroll` only change
the hint to SELECT: ROW; `tracker-mute-rec` also shows the pinned pattern/row and yellow edit
cursor instead of following the playing row. New `tracker-edited` shows row 01's reverse-yellow
2× `423`; `tracker-instrument-popup` shows the large INSTRUMENT / 02 popup. Old and new PNGs
were viewed. No unrelated screen golden was regenerated.

Storage integration remains explicit: `st_song_edit_saving_and_size_guard` is the named
prospective guard for M2c Task 8's save exclusion and serialized-size cap, neither present on
main at this base. Host/UI tests exercise SAVING and SONG FULL refusals with large toasts.
Save/load initiation must finish pending UI requests and accepted commands; even a fixed-length cell edit can grow
serialized instrument/table counts. No hardware was accessed; the button-id flags remain.

**Task 3 integration on `82f7357` (M2c Task 8):** rebased onto the stored-song/schema-4
implementation, retaining both build-source lists, both test modules, the SONG menu and
its borrowed large-text canvas, and the readable TRACKER editor. The original feature
commit is now `d9c1b11`; Task 4 preparation remains unregistered and separate.

The earlier guard placeholder is now wired to real save/load state. Prospective size is
checked by the model's authoritative reservation logic before allocation or mutation;
`GT2_ERR_SONG_FULL` shows SONG FULL, `GT2_ERR_SAVING` shows SAVING, and a destructive
song-I/O operation shows SONG BUSY. An emulator panel test starts from an exactly
65,536-byte song and confirms that K1 growth is refused without changing its bytes.
Push/pull and whole-order replacement retain the cap/freeze accounting, including
trailer-padding growth when tail locks disappear and pending pattern-prefix trims.

Undo now preflights its restored row/instrument prefix and packed locks together, before
allocating a rebuilt pattern. Its checked release restores locks while frozen, before
reservation recomputation and completion. Concurrent restore preparation waits for all
pending edits, protecting the shared 512-lock/4,096-byte extension limits without new
RAM fields. A primitive prepared before SAVE may drain while frozen, so SAVE_WAIT and
an unsent edit cannot deadlock. Loads wait for private model reservations too; a successful
load clears the old undo history, cursor and pending PLAY/jam intent. Regression tests
cover these lifecycles, exact-cap refusals, cancellation and unchanged-model failures.

The main baseline has only **4,116 B free in RAM**, 20 B above the unchanged 4,096-B
regional gate. Song I/O's explicitly initialized state moved to POOL; no buffer or
behavior was removed. Baseline → integration: RAM used **61,420 → 61,260 B**, RAM free
**4,116 → 4,276 B**; POOL used **346,560 → 346,960 B**, POOL free **5,696 → 5,296 B**;
combined free **9,812 → 9,572 B** against unchanged FREE_RAM_MIN **7,000 B**. RAMTEXT
**45,216 → 45,340 B**. The app image is **454,284 → 461,196 B**; the cumulative main
stage is M2f's unchanged **512,000-B** ceiling, rather than the old standalone M2d
projection. All regional, aggregate, flash and audio bounds remain intact.

Early integration gates pass unchanged: STATUS idle **7,480.2 → 7,482.2** instructions/frame;
all HOME/CHIP/SCENE input checks and stopped/playing TRACKER edit budgets pass. The real
size-cap panel regression, loaded-song history regression, 20 editor regressions and
native workspace/UBSan suites pass. The final post-rebase checklist is complete below.

The first complete integration run exposed a neutral-compaction codegen path using a
memory-form bit-clear the pinned emulator does not decode (the baseline contains the same
instruction family). Neutral swaps now bypass semantic accounting; real bit updates use
a scalar load/register operation/store. Old spans, pending flags, reservations and saved
size still finish normally. Targeted compaction passes at **439 input / 27,353 busiest
pass**, with the unchanged 4,600/55,000 bounds; the change also removes 144 image bytes.
The bank/save row hooks now await exact row bytes plus apply-and-release completion rather
than assuming that an accepted, shape-widening edit finishes within 10 ms. Their timeout,
data-preservation and pass bounds are unchanged. The phrase fixture admits only known
new setup chunks captured by M2c on audition, retaining byte-exact expected GT2 data,
pre-existing chunks and the full audio comparison. All four focused regressions pass.

**Final integration gates:** STATUS idle **7,482.2** (<7,500); HOME **426** (<430);
CHIP K1 **211** (<215); SELECT to RULE 30 **227** (<230); LIVE noise
**154,349.05 instructions/frame** (<155,000); USB's 16-CC drain **7,991** (<8,000).
All MIDI floods and every TRACKER budget pass unchanged. PLAYER_WORST_BLOCK remains
60: **5,731 instructions / 44.77 per sample**, including **592** isolated row-accounting
instructions. Eight edits without a call cost **647**, the worst call remains **4,492**;
the complete audio budget leaves **160.7 instructions/sample** of headroom.

**Final integration checks — all exit 0:** normal fm1 configure/build; flash-spike
configure/build; GCC 13 on every changed app/core C file; workspace fmt check; locked
workspace/all-target Clippy with `-D warnings`; locked release workspace tests;
locked UBSan workspace tests; fresh CI-argument sidou-pack and sidou-pack tests;
the complete emu-gate with both build variables and `--nocapture --test-threads=1`
(`--no-fail-fast` collected all binaries); offline REUSE lint; and
`tools/web-demo --check` with cached dependencies. The app suite has **943 passing**
tests and 3 pre-existing ignored tests. The real-cap panel, frozen-save, bank refusal,
four-note audio, loaded-history and fragmented-arena regressions all pass. No new
golden was regenerated during this integration; the earlier intended TRACKER goldens
are unchanged. No hardware was accessed. Task 4's audition-policy checkpoint below
remains open and its preparation is not registered in production builds.

Task 3 completion snapshot, before Task 4 integration (`git log --oneline -5`):

```text
5d5d4c7 fix(tracker): validate editing with stored songs and the size cap
d9c1b11 feat(tracker): edit patterns from the keys and encoders, through the player's edit queue
82f7357 fix(app): integrate stored songs with readable screens
b478453 fix(app): integrate songs with large-text UI and MIDI
9248402 feat(app): songs on the store: load, save, new, About's music
```

`git status --short` at that boundary (only unregistered Task 4 preparation):

```text
?? app/song_pages.c
?? app/song_pages.h
?? host/crates/sidou-tests/tests/app/song_audition_shape.rs
?? host/crates/sidou-tests/tests/app/song_order_locks.rs
?? host/crates/sidou-tests/tests/app/song_pages.rs
?? host/crates/sidou-tests/tests/app/song_pattern_at.rs
```

---

### Task 4: ORDER, INSTR and TABLE pages

**Files:** `app/order_view.c`, `app/instr_view.c`, `app/table_view.c` (new; or one file
with three views if it reads better), tests, goldens.

**Spec:**
- **ORDER:** the three channels' orderlists of the current subtune, as columns of entries
  (pattern numbers, `R2` repeats, `+3`/`-1` transposes, `RST` and its restart position);
  SELECT moves, ALGORITHM changes voice, K1 the entry's value, K2 its kind (pattern,
  repeat, transpose), EDIT + PRESETS inserts or deletes an entry, ARP sets RST here; PLAY
  plays from the cursor's entry. A pattern number past the last pattern creates an empty
  pattern of the default length (64). A subtune selector on row 2 (ALGORITHM on it).
- **INSTR:** the current instrument (PRESETS selects it) with GoatTracker's nine
  parameters by name (ATTACK/DECAY, SUSTAIN/RELEASE, WAVE PTR, PULSE PTR, FILTER PTR,
  VIBRATO, VIB DELAY, HR/GATE, 1ST WAVE) in hex, and its name (the M2c name editor); SELECT
  moves, K1 coarse (a nibble), K2 fine; the keys audition it.
- **TABLE:** one of the four tables (ALGORITHM picks WAVE, PULSE, FILTER, SPEED) as rows of
  left and right bytes with GoatTracker's meanings spelled on row 29 for the cursor's row
  (`WAVE 41 C-3`, `JUMP 05`); SELECT moves, K1 the left byte, K2 the right; from INSTR,
  EDIT + SEQ jumps to the instrument's pointer in its table.
- All within the budgets; edits through `song_ctl`.

**Tests:** host: each page's edits on the model and their refusals; fm1-emu: goldens of
each page; an instrument edited on INSTR changes the sound of the next note (audio against
the host's render of the expected song).

**Commit:** `feat(app): the ORDER, INSTR and TABLE pages`

**Task 4 audition-policy checkpoint — decided:** Task 4 is not
registered on `m2d`. Preparation had grown into an uncommitted page integration and an
experimental state-preserving audition implementation before the controller's explicit hold.
All of it is now parked as **`71e6856`**, branch **`m2d-task4-preparation-170e182`**, outside
the production rebase. It is not a completed Task 4.

**Decided 2026-10-19 by the controller: B** — preserve live state, with conversion at a
block boundary after bounded main-loop preparation. A new note triggers normally; an
explicit edit retains its intended effect. Never sound unrequested song notes while
stopped. If B cannot meet every unchanged gate after real effort, stop and report the
measurements; do not fall back to A or C. Task 4 proceeds as separate commits after the
cleaned Task 3 delivery below. The options and recommendation following this decision are
retained as its rationale, not as an open policy question.

**What needs a decision.** An audition can require a player shape different from the one
derived from the song: an instrument not used by any pattern, a third voice omitted by the
packed song, a valid 0–95 note outside its packed frequency range, or a new pulse/wave table
encoding. Native reproductions show skipped filter processing, an unpacked third voice that
never starts, and zero/wrong frequency outside the packed note range. Existing used-shape
auditions work. The decision is how an incompatible request affects a running song and any
already-held audition: preserve them, refuse the request, or explicitly allow a reset.
This is a shared-engine limitation, not just a page-layout issue: TRACKER's instrument
control can also select an instrument outside the packed song's used set. INSTR makes that
boundary central by exposing all instruments and their tables.
The transition can come from a new key, an instrument/table edit while a note is held, or
background narrowing after editing. Refusing only new keys would not protect an existing
held audition from the latter two paths.

**What the plan does and does not say.** The shared contract at lines 63–75 assigns transport
to PLAY and audition of the current instrument/cursor voice to the keys; Task 4's INSTR
specification above says its keys audition the selected instrument. There is no direct
contradiction in that intent. The plan does not specify packed-shape transitions, whether a
held envelope/table may retrigger, or whether an incompatible audition may be refused while
playing. A stop-only limitation or an implicit restart would therefore be a product-policy
addition, not an implementation detail already authorized by the text.

**Current engine boundary (rebased production sources).** `song_ctl_jam` only queues the
voice/note/instrument (`core/player/song_ctl.c:610`); `song_ctl_audition` prepares a paused
song-derived player, not an arbitrary-instrument audition shape (`:633`). Frequency reads
are relative to `first_note`/`last_note` (`core/player/gt2_play.c:202–222`), filter execution
depends on the derived feature flag (`:742`), and channel work is bounded by the derived
channel count (`:874`). Incompatibility includes channel count and pulse/wave state encoding
(`core/player/gt2_shape.c:976–982`). The existing edit/narrowing paths use a fresh player and
fast-forward handover (`core/player/song_ctl.c:708–724`, `:803–814`, `:905–914`, `:1038–1046`);
the handover copies that player's state/registers (`:209–230`). It does not preserve arbitrary
held jam history and can introduce sequenced notes during stopped audition. That last effect
is a bug to prevent under every option, not a proposed feature.

1. **A: bounded safe restart/refusal.** When stopped, rebuild a frozen audition-only state
   and retrigger held audition voices; never start other song voices. While playing, accept
   compatible auditions but refuse an incompatible one with **STOP TO AUDITION**; leave
   playback and held notes unchanged. Also refuse an incompatible shape-changing edit while
   an audition is held (**RELEASE TO EDIT**), and postpone narrowing until release. This has
   the smallest state/conversion surface, but
   held notes can audibly restart when stopped and some live auditions are unavailable.
   It changes the unrestricted audition expectation.
2. **B: preserve live state.** Prepare the needed capabilities in bounded main-loop slices,
   convert/retain current voice, pulse/wave encoding and held-jam state, then publish at a
   block boundary. Playback position and held envelopes/table phases do not restart; stopped
   audition never advances unjammed song sequencers. A requested new note still triggers
   normally, and an explicit edit still has its intended musical effect; preservation means
   no additional whole-player reset or replay. This best matches the stated controls,
   but adds engine state/lifetime complexity and needs stricter conversion/audio tests.
3. **C: explicit reset policy.** Permit an incompatible request to visibly stop/restart
   playback or replace/retrigger held audition state, with the transition shown to the user.
   Never silently sound song notes while claiming to be stopped. This makes replay reuse
   simpler, but is musically disruptive and needs approval plus amended transport/audition
   documentation.

**Recommendation: B**, with the preservation behavior above as the acceptance contract and
all current image/RAM/IRQ/input gates unchanged. It satisfies the plan without a new stop-only
restriction or an audible transport surprise. The parked prototype is evidence to evaluate,
not permission to proceed; it found real cancellation/encoding hazards and needs rebase and
full validation if approved. If B cannot meet the gates, stop and report rather than silently
falling back to A or C. A is the conservative alternative if reduced scope is preferred.

---

### Task 5: STEP

**Files:** `app/step_view.h`/`.c` (new), `core/song/song_ext.c` (locks, if Task 7 of M2c
left anything), tests, goldens, `docs/art/README.md` (a "STEP" section).

**Layout** (the prototype's):
- row 0 the header, rows 2-16 a frame `PATTERN 01  64 STEPS` with the page selector (1-4)
  and the step ruler on row 3, then four lanes of 16 step cells (columns 5-20): V1, V2, V3
  with their names, current notes and waveforms, and **B** greyed (`CHIP B IN M5`); a step
  with a note is a pip in the voice's colour, a step with a lock carries a notch (the
  prototype's `PIPL`), the playing step white;
- rows 17-22 a `HELD STEP` frame: the four knobs' parameters, their values and bars,
  `LOCK` in yellow on locked ones;
- rows 24-28 the 16 keys as two rows of eight pads (the physical keys' picture); row 29 the
  help line `HOLD KEY+KNOB LOCK  ENC PAGE`.

**Controls:**
- **The 16 leftmost note keys are steps 1-16** of the page. A press toggles a note on the
  selected lane at that step (the lane's default note and instrument; off writes a rest). A
  **held key plus a knob** locks that knob's parameter on that step.
- **SELECT** selects the lane; **ALGORITHM** pages through the pattern (4 pages for 64
  steps; patterns of other lengths show as many pages as they need); **PRESETS** sets the
  lane's instrument, or with a step held the step's note.
- **The knobs on a held step:** K1 cutoff, K2 resonance, K3 pulse width, K4 waveform; with
  EDIT also held, the character: K1 age/heat, K2 DAC, K3 thump, K4 bleed.
- **Two kinds, one meaning each, shown apart (decided):**
  - **SET** (native, persists): K4's **waveform** on a held step is written as
    GoatTracker's `7XY` command on that row, which holds until the next waveform change, as
    GoatTracker plays it. If the row's command column is taken, the step refuses it (toast
    `ROW HAS A COMMAND`). Shown in the HELD STEP frame as `SET` in light blue, and on the
    step as a light-blue notch.
  - **LOCK** (SIDOU's, one row): K1 cutoff, K2 resonance, K3 pulse width and the character
    (EDIT held) go in the `LOCK` chunk (M2c Task 7) and hold **for that row only**. Shown as
    `LOCK` in yellow, and on the step as the prototype's yellow notch (`PIPL`).
  - Why: a cutoff written as `CXY` would persist while one written as a lock would not,
    depending on its value; the same knob must mean one thing. A persistent cutoff change is
    the TRACKER's `CXY` command, entered there. Document both in
    `docs/reference/song-format.md` and `controls.md`.
- A lane's **gate length:** a step note is followed by a key off on the next empty row
  (staccato, the default) or held to the next note (legato), a lane setting written into
  the pattern itself (real key-off rows), so GoatTracker plays it the same.

**Tests:** host: toggling, paging, SET and LOCK (a SET persists over the following rows in
the player's output, a LOCK returns after its row), SET refused on a row with a command,
both shown distinctly (the frame's text and colours), legato and staccato rows; a STEP-made song played by the reference harness
(M2c Task 3) equals SIDOU's player with the native locks (register match) and with the
chunk locks off; fm1-emu: goldens, a pattern programmed from the keys plays bit-exact with
the host, a held step + K1 darkens that step (spectrum), the costs as Task 2's.

**Commit:** `feat(step): the STEP view, 16 keys as steps and parameter locks on held steps`

**As built — Task 5 (base `d76b57a`):** STEP is `app/step_view.c`/`.h`, the first song view (SEQ now
steps STEP, TRACKER, ORDER, INSTR, TABLE and round; the tests that reached TRACKER with one SEQ
press two, and the ergo harness's journeys, tables and recipes say so). Layout and controls are
the plan's, in TRACKER's readable 2x style and the theme's roles, with these decisions the plan
left open:

- **A step toggles when its key is let go**, not when it goes down, and only if no knob was turned
  while it was held: pressing a key to hold it for a lock must not also write a note. The key
  still auditions the step's note as it goes down.
- **The gate is ARP** (staccato, the default, or legato, per lane, shown under the lane); a
  staccato lane writes a key off on the next row if that row has no note (a row with only a
  command counts as empty and keeps its command) and takes it away with the note; where the row
  before a removed note holds a note, the removed note's row becomes its key off.
- **PRESETS** sets the lane's instrument; with a step held, that step's note, which is then the
  lane's note for the steps toggled after it. **LFO** with a step held clears its locks and SET.
- **K1-K4 values:** a lock starts from the middle of its range (cutoff 1024, resonance 8, pulse
  width 2048, the character 64) and a detent steps it by 32, 1, 64 and 2; K4's waveform is OFF,
  TRI, SAW, PULSE, NOISE (`7XY` with `$11 $21 $41 $81`), turning OFF removes the command.
- **ALGORITHM pages while stopped; while the song plays the page follows the playing step**, and
  stopping leaves it there. The pads are the keys' picture (a note, the held step), not the playhead.
- **Edits wait their turn.** The first key after a pause starts the audition, which holds edits
  for a while, and the editor takes one cell write at a time; detents of K4 or PRESETS and
  toggles that arrive meanwhile wait in a queue of four besides the one in flight (turns of one
  knob on one step join), the key off after a note is a second write sent when the first is
  applied, and a fifth is refused with `WAITING`. A first version dropped them: the emulator's two
  detents of K4 right after a key went down gave one.
- **Locks are the main loop's** `song_lock_put` (song_ctl resolves them at the row's start, as
  M2c Task 7 built it); nothing was left to do in `core/song/song_ext.c`.
- **Not done here:** the menu lists TRACKER only (Task 1's "STEP and TRACKER" menu items are not
  added: SEQ reaches STEP, and a menu item would move the menu's goldens); REC's recording while
  the song plays is Task 6.

Resources: the image 477,184 -> **486,832 B (+9,648)**, the RAM region free 4,464 -> **4,368 B (-96)**
with POOL free 4,456 unchanged and RAMTEXT 46,592 unchanged (together 8,920 -> 8,824, floor 7,000;
each region keeps 4,096). `step_view.c` is built `-Os -fno-inline-functions -fno-jump-tables` as
`song_pages.c` is. The unchanged tight gates are identical on the base and this build: STATUS idle
6,364.5 instructions/frame, THE OSCILLATOR's input 427 (THE CHIP 415, THE SCENE 417), CHIP K1 211,
SELECT to RULE 30 input 156, LIVE noise 153,357; no existing golden changed. STEP's own costs
(new code, bounds set from these) are in docs/art/README.md "STEP": idle 7.2k instructions a
frame, the busiest `ui_poll` 45.7k (a PLAY's analysis slice with a compose), SEQ's pass 49.6k
(68.4k before the view built its zones as the wipe reaches them), PRESETS' pass 54.4k.

Tests: `tests/app/step_view.rs` (22), `song.rs`'s two key-driven tests (the expected song and bit
exact audio, SET and lock included; the darkened lead bar), `screen.rs`'s goldens (`step`,
`step-held`, `step-lock`, `step-play`) and budgets. A STEP-made song plays as GoatTracker's
player plays it, call for call, with the SET holding over the following rows.

---

### Task 6: REC from the keys while the song plays

**Files:** `app/song_view.c`, `app/step_view.c`, `app/tracker_view.c`, tests.

**Spec:**
- With REC on **and the song playing**, a note key writes its note into the cursor's voice
  at the row it was played in, quantised to the nearest row (the event ring's row times
  against the key's time), replacing what was there; the key also sounds (jam). In STEP,
  REC + a key while playing writes a step on the selected lane.
- K1 and K2 moves while recording become cutoff and resonance locks on the rows they
  happened in (the design's "REC records notes and knob moves into the current pattern").
- Quantise options (menu): NEAREST (default), OFF (the row it falls in), and a count-in of
  one bar before recording starts.

**Tests:** host: quantisation (a key at each fraction of a row lands on the expected row);
knob moves to locks; fm1-emu: a timed key script recorded at tempo 6 lands on the expected
rows (compare the pattern through a test hook) and plays back bit-exact with the host.

**Commit:** `feat(app): REC records the keys and the filter knobs into the playing pattern`

---

### Task 7: Docs and the M2d status

- `docs/reference/controls.md` (the song views in full), `docs/art/README.md` (STEP,
  TRACKER, the pages, with the measured costs), `docs/hardware-test.md` (make a song on the
  device: keys, knobs, REC, mute, save, reload), `TESTING.md`, `CHANGELOG.md`, this plan's
  tasks marked done with their results (free RAM against the ≥ 16,000 gate),
  `flash_budget.rs`'s `IMAGE_CEILING` moved to the schedule's M2d entry (440,000) or lowered
  to the measured image plus a margin.

**Done 2026-10-22** (docs only; no code, bound or golden changed): the tasks are marked in the
Status at the top, with the measured image and free memory. The 16,000 and 440,000 figures
are obsolete as the Status explains, so `IMAGE_CEILING` stays at M2f's 512,000 (the schedule
only rises, and it is the active stage); the image is 18,160 B below it.

**Commit:** `docs: M2d complete, STEP and TRACKER`

---

## Task 3 revalidation on main `170e182`

Task 4 is on hold for the controller's audition-policy answer (checkpoint above). All of its
uncommitted preparation, including the experimental audition conversion, was parked as
`71e6856` on `m2d-task4-preparation-170e182` before rebasing. That commit is not an ancestor
of `m2d`; no ORDER/INSTR/TABLE source or test module is registered here.

The three completed TRACKER commits rebased without conflicts onto `170e182` as `5350083`,
`8f85b3b` and `3c1f076`. Main's CHIP/song shared inactive canvas, schema-5 settings, SONG
store/save/load behavior, theme T1, emulator `7ca5b96` pin and preflash tooling are retained.
Preflash and real-device tools were not run. SDK inputs were already cached; no access to
the controller's main worktree or web fetch was needed. The web emulator revision was
imported from the local Cargo git cache.

The final default fm1 and spike directories were recreated rather than reusing the old
CMake caches. Both `SIDOU_ALL_COMBINED_PROFILES` and
`SIDOU_ALL_COMBINED_PROFILES_8580` are explicitly **ON**, confirmed in the fresh cache.

Two Task 3 integration corrections, not Task 4 implementation:

- The first fresh link had **5,524 B RAM free / 4,056 B POOL free**: combined RAM passed,
  but POOL failed its unchanged 4,096-B floor by 40 B. Returning the 416-B song-I/O object
  from POOL to ordinary RAM gives **5,108 B / 4,472 B**, with the same **9,580 B** combined.
  The shared canvas now leaves room in RAM; no state or memory floor was reduced.
- Recording now takes its screen from the actual routed TRACKER view, including beneath
  MIDI learn, not the last canvas owner or an enter/leave pointer. Red-first cases exposed
  a lost first REC note after navigation and lost recording after nested learn/SEQ/HOME.
  The latter also exposed a learn capture/completion lifetime crash: learn now yields to a
  child and cannot pop it when finishing. Three regressions cover CHIP/SONG/SAVE-AS before
  composition, nested learn and deferred SAVE completion. The focused full app suite has
  **977 passing / 3 ignored** cases; fresh diff review found no further concrete defect.
  Canvas ownership still changes only at compose, never on enter/exit. Audition behavior
  and the song-edit cell/swap protocol are unchanged.

The six retained TRACKER goldens were viewed before and after and regenerated on only their
two named tests. The ordinary/voice/play/scroll footer says **SELECT: ROW** rather than
SELECT: VOICE, as Task 3's redesigned controls require. Muted REC pins the manual cursor at
**PAT 04 / ORD 00 / ROW 00**, with the selected note highlighted, rather than following
playback to PAT 05 / ORD 01 / ROW 03. No other golden, including `live-mutes.png`, changed.

Fresh linked snapshot: image **504,016 B** versus main's
**497,120 B** (**+6,896 B**; **7,984 B below** the unchanged 512,000-B ceiling), RAM free
**5,108 B**, POOL free **4,472 B**, combined **9,580 B** (floor 7,000), RAMTEXT **46,144 B**
(limit 49,152). Main's firmware sources are unchanged between `1e2bb90` and `170e182`, so
the directly measured main-image baseline still applies. No bound or tolerance was loosened.

**Validation outcome: blocked, not a green integration.** The first full emulator invocation
was interrupted before its overall exit file was written. After the server restart, that
missing result was not treated as success: the complete invocation was rerun fully detached,
with both fresh build variables and `--nocapture --test-threads=1` (plus `--no-fail-fast`).
The resumed run completed every binary and doctests, recording **exit 101: 190 passed,
2 failed, 0 ignored**. The only failing target is `screen` (**61 passed / 2 failed**). The
song binary passed all **15** cases, including the phrase's bit-exact audio and the player
budget. Focused reruns and the completed full run confirm the same two screen failures:

- `live_matches_its_golden_images_and_its_scopes_follow_the_notes`: **183 pixels** differ in
  `live-chord.png`, first `(63,23)`. Old/new PNGs were viewed: notes, controls and layout match,
  while the saw scopes differ in phase/shape. No LIVE drawing change is intended here; the
  cause is not established. COMMON's animation-phase exception names SCENE, not LIVE.
- `tracker_edits_from_the_panel_are_readable_and_bounded`: **1,059 pixels** differ in
  `tracker-edited.png`, first `(219,0)`, including header colour and activity strips. The
  selected field and entered row data look the same; that is not sufficient evidence to
  accept the remaining differences.

Neither unexpected golden was regenerated, masked or given a tolerance. Work stops for the
controller under COMMON's unexpected-golden rule. The separate Task 4 policy hold also
remains. The checklist has finished, but it is not green.

| Check | Recorded exit |
|---|---:|
| Fresh fm1 configure/build, final rebuild | 0 |
| Fresh flash-spike configure/build, final rebuild | 0 |
| GCC 13 on every changed app/core C source | 0 |
| Workspace formatting | 0 |
| Locked workspace/all-target Clippy with `-D warnings` | 0 |
| Locked release workspace tests | 0 |
| Locked workspace tests with C/C++ UBSan | 0 |
| sidou-pack using the CI app/loader arguments | 0 |
| Locked release sidou-pack tests | 0 |
| REUSE (cached offline invocation) | 0 |
| `tools/web-demo --check`, emulator `7ca5b96` | 0 |
| Two named TRACKER header/scroll golden updates | 0 each |
| Focused unexpected LIVE golden confirmation | **101** |
| Focused unexpected edited-TRACKER golden confirmation | **101** |
| Complete detached emulator rerun | **101: 190 passed, 2 failed** |

Measured assertions in the completed emulator rerun:

| Gate | Measured | Unchanged bound |
|---|---:|---:|
| STATUS idle | **6,341.0 instr/frame** | <7,500 |
| HOME / THE OSCILLATOR input | **426** | <430 |
| CHIP K1 | **209** | <215 |
| SELECT → RULE30 input | **227** | <230 |
| LIVE noise | **153,324.11 instr/frame** | <155,000 |
| USB drain, 16 CCs | **7,917** | <8,000 |
| PLAYER worst block | **5,738 / 128 = 44.83 instr/sample** | ≤60 |
| TRACKER compaction request | **439 input / 27,776 busiest pass** | <4,600 / <55,000 |
| TRACKER cell edits | **max 2,048 input / 30,875 busiest pass** | <4,600 / <55,000 |

MIDI flood assertions and TRACKER idle/input/playing cost assertions reached in the run
passed; the edited-TRACKER test then failed on its golden, so later assertions in that test
are not claimed. The 30-second playing measurements were:

| Case | Instr/frame | Worst poll | Most cells | Worst SPI ms |
|---|---:|---:|---:|---:|
| LOOP BAR | 26,848.61 | 24,371 | 161 | 14.069 |
| LOOP SCROLL | 59,135.35 | 23,218 | 177 | 15.559 |
| Tempo 3 BAR | 45,787.53 | 24,681 | 173 | 15.093 |
| Tempo 3 SCROLL | 64,318.51 | 21,657 | 174 | 15.288 |

Each completed case reported zero frames cut short. Existing 200-cell, 20-ms, per-mode
instruction and poll limits remain unchanged. Files in the uncommitted rebase correction
were reviewed with fresh context; no further concrete code defect was found. They are
parked as WIP because validation is blocked, not presented as a passing feature commit.

Rebased integration snapshot: **`6ddf5f7`** (WIP). The following log/status was captured
after that commit and before the report-only commit recording this snapshot:

```text
6ddf5f7 wip: revalidate TRACKER on main 170e182
3c1f076 docs: record the rebased TRACKER verification snapshot
8f85b3b fix(tracker): validate editing with stored songs and the size cap
5350083 feat(tracker): edit patterns from the keys and encoders, through the player's edit queue
170e182 docs(hardware): the store check names Felucca only
```

`git status --short`: empty. Task 4 preparation remains on its separate WIP branch; no push,
tag, real-device access or unexpected-golden update was performed. The temporary baseline
worktree was removed; blocked-run logs and actual PNGs remain as diagnostic evidence.

The resumed verification above was run from report HEAD `a564041`, with the same code as
`6ddf5f7`. Its result is recorded in `build/scratch/m2d-170e182/emu-resumed.exit` (101), with
the full log alongside it. Every other final checklist exit file is 0. No source or golden
was changed during the rerun, and no verification process remains running. This completion
record is documentation only; Task 4 still awaits the controller's policy answer.

## Rebase onto the flash win-back, main `c1831e9`

Before this rebase, all previously running checks were finished and their exit files read:
release workspace **0**, UBSan workspace **0**, and the completed emulator rerun **101**
(190 passed, 2 unexpected golden failures). No old check was left running or treated as an
unrecorded success.

Rebased onto `c1831e9` without conflicts. `app/CMakeLists.txt` was verified to retain both
TRACKER's `song_edit.c` source and main's cold-file `-Os` settings for `midi_route.c` and
`song_view.c`. The tighter bitmap codec is retained unchanged. Task 4 preparation remains
separate at `71e6856`; no page or audition-policy implementation was applied.

Fresh default fm1 and spike directories were created, with both combined-profile options
explicitly ON. sidou-pack reports **492,480 B** of app image versus main's **485,568 B**:
**+6,912 B**, with **19,520 B** left below the unchanged 512,000-B ceiling. This is
**11,536 B smaller** than the preceding M2d image. RAM remains **5,108 B free + 4,472 B
POOL free = 9,580 B**, and RAMTEXT remains **46,144 B**. No floor, bound, profile or behavior
was removed to achieve the image result.

Post-rebase verification completed, with every command judged by its own exit file:

| Check | Exit |
|---|---:|
| Fresh fm1 configure/build | 0 |
| Fresh flash-spike configure/build | 0 |
| GCC 13 on every changed app/core C source | 0 |
| Workspace formatting | 0 |
| Locked workspace/all-target Clippy, `-D warnings` | 0 |
| Locked release workspace tests | 0 |
| Locked workspace tests with C/C++ UBSan | 0 |
| sidou-pack, using the CI app/loader arguments | 0 |
| Locked release sidou-pack tests | 0 |
| REUSE, cached offline invocation | 0 |
| `tools/web-demo --check` | 0 |
| Free-RAM/region map gate | 0 |
| Full detached emulator run, both build variables | **101: 190 passed, 2 failed** |

All binaries and doctests finished. The only failing target is still `screen`, with **61
passed / 2 failed**: `live-chord.png` differs by **183 pixels**, and `tracker-edited.png` by
**1,059 pixels**, at the same first coordinates as the preceding rebase. Neither unexpected
golden was regenerated, masked or loosened, and no golden was changed on this rebase. The
bitmap codec was retained, not modified to accommodate the failures. The integration is
therefore still **blocked, not green** under COMMON's unexpected-golden rule. Task 4's
separate audition-policy hold also remains.

Numeric gate measurements before → after the flash-win-back rebase:

| Gate | Before (`170e182` base) | After (`c1831e9` base) | Unchanged bound |
|---|---:|---:|---:|
| STATUS idle instr/frame | 6,341.0 | **6,362.0** | <7,500 |
| HOME input | 426 | **426** | <430 |
| CHIP K1 | 209 | **209** | <215 |
| SELECT → RULE30 | 227 | **227** | <230 |
| LIVE noise instr/frame | 153,324.11 | **153,371.99** | <155,000 |
| USB drain, 16 CCs | 7,917 | **7,917** | <8,000 |
| PLAYER worst instr/sample | 44.83 | **44.83** | ≤60 |

TRACKER's measured compaction request remains **439** input instructions, with busiest
pass **27,802**. Cell edits reached at most **2,048** input instructions and **30,933** for
the busiest pass. The completed 30-second playing cases remain within all existing
instruction/poll/cell/SPI bounds and report zero frames cut short. As before, the edited
TRACKER test fails at its golden, so assertions later in that particular function are not
claimed as executed. The song binary passes all 15 cases, including bit-exact phrase audio
and the 193-run / 145-song player budget matrix.

The tested code was frozen at rebase HEAD `89dfc60`; only this report changed during the
checks. There are no running checks left. Results and actual PNGs are retained under the
worktree's `build/` directories as blocker evidence. No push, tag or real-device operation
was performed, and no Task 4 page or audition-policy implementation was applied.

Rebased log captured before this report-only update:

```text
89dfc60 docs: record completed rebase emulator verification
6134ff4 docs: record blocked TRACKER revalidation on 170e182
5475468 wip: revalidate TRACKER on main 170e182
8c44ced docs: record the rebased TRACKER verification snapshot
e1bc8b3 fix(tracker): validate editing with stored songs and the size cap
```

At capture, `git status --short` listed only this report; all tested source and goldens were
unchanged. `c1831e9` is an ancestor of this branch, and Task 4 WIP `71e6856` is not.

## Controller approval and Task 3 history cleanup

The controller viewed reference, actual and diff images and approved these updates on
2026-10-19:

- **`tracker-edited.png`:** all differing pixels are label roles moving grey → light grey
  (row numbers, BPM, the orderlist, ORD/ROW and empty-note dashes). This is theme T1's
  `e88d2e7` role change, merged after the golden was made, not an editing regression.
  An independent indexed-pixel check confirms **1,059 differences, all palette 12 → 15**.
- **`live-chord.png`:** layout, notes and controls are identical; only the scope traces
  differ in capture phase. TRACKER's code shifts main-loop timing, so the capture samples
  a slightly different audio moment. Bit-exact song tests establish unchanged audio.
  The indexed-pixel check confirms **183** changed pixels, confined to trace colours and
  background, with the same palette and no layout/note/control change.
- **`tracker-instrument-popup.png`:** accepting the edited-row golden exposed this next
  comparison, previously unreachable after the first failure. Both images were viewed and
  an indexed-pixel check confirms **830 differences, all palette 12 → 15**. The controller
  explicitly approved this third T1 label-role update; the popup, layout and data agree.

For this rebase only, the controller also approved any further TRACKER golden whose changed
pixels are exclusively palette 12 → 15, provided each image is checked and its count recorded.
The two TRACKER images listed above are the updates using that rule; no other image was
updated under it. Any other change still requires a stop and report.

Only the two named emulator tests were run with `SIDOU_LCD_GOLDEN_UPDATE=1`. A temporary
filename restriction ensured only the approved files could be rewritten and kept all other
comparisons strict; it was removed afterwards, leaving the test helper unchanged. No
tolerance or numeric gate changed. These controller decisions supersede the corresponding
unexpected-golden and audition-policy holds recorded above.

## Risks

- **The control mapping** is new and dense; it is a proposal for the user to try in the
  emulator before hardware (a `demos/` key script per view helps: design section 10).
- **Scroll mode at fast tempos** cannot keep within the SPI budget; it lags, by design.
- **Editing while playing** relies on the edit queue; big edits are split into swaps and
  spread over blocks (Task 3), so they take a few blocks to land.
- **The button ids** LFO, GLO, ARP, SEQ, PLAY and REC are unconfirmed on a SIDOU unit (the
  hardware flag above).
- **Chip B's lane** waits for M5.

<!-- REUSE-IgnoreEnd -->
