<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Whole songs as text (`sidou-song/1`) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** a `sidou-song/1` JSON song (sections of step-grid strings, patches as instruments, a
play order) converts to an ordinary `.sng` on the host, on the site and in `sidou-sysex`.

**Architecture:** one C converter beside `sidou-step/1`'s in `platform/web/patch_json.c`, built on
the firmware's own song code (`song_io_template`, `gt2_set_row`, `song_lock_put`, the patch
placement `place_patch`). Rust (`sidou-patch`'s `json.rs`) wraps it for tests, docs and
`sidou-sysex song`; the site reaches it through `platform/web/patch_api.c` in the patch wasm.
Design: [2026-11-14-songs-as-text-design.md](2026-11-14-songs-as-text-design.md).

**Tech stack:** C11 (the converter), Rust (tests, CLI), JavaScript (site), fm1-emu (emu-gate).

---

## Ground rules for whoever runs this

- Work in the worktree `/Users/denis/sidou-songs` (branch `songs-text`, from `main`). Rust on
  PATH: `export PATH="/opt/homebrew/opt/rustup/bin:$PATH" CARGO_TERM_COLOR=never`.
- CLAUDE.md applies: no code comments, DRY code and tests (shared test fixtures in
  `host/crates/sidou-patch/tests/common/mod.rs`), every test proves behaviour, test first.
- Run only targeted tests while working (`nice`); the full check is `tools/merge-check`, once,
  at the end. Never touch the FM-1.
- Budgets are not raised without the owner: the patch wasm's is 80 KiB (71.7 KB today,
  `web/patch.test.mjs`).
- Mirror `sidou-step/1` everywhere: its reader (`patch_json_read_step`, `step_cells`,
  `step_instrument`, `step_checked`), its song (`patch_json_step_song`), its Rust wrapper
  (`json::step_song`, `json::check_step`), its CLI (`sidou-sysex step`), its site path
  (`STEP_KEY`, `sp_step_instr`) and its tests (`tests/patch_step.rs`). Read each before its task.

## Tempo: what the format can promise

`song_view_bpm10` (`app/song_view.c:571`) gives the BPM a song shows: PAL 50.125 Hz, four rows a
beat, funktempo's two speeds averaged. One speed per call at 1x leaves only half steps:
6 → 125.3, 6.5 → 115.7, 5.5 → 136.7, 7 → 107.4. The converter takes `bpm`, picks the speed pair
(a, b), b - a at most 1, whose `song_view_bpm10` is nearest, and reports that BPM. Multispeed
would be finer but runs the instruments' tables faster, so it is not used. The site and the CLI
say "asked 120, plays 115.7" (or 125.3) so a model learns the steps; the docs list them.

---

### Task 1: the worktree and the fixtures

**Files:**
- Modify: `host/crates/sidou-patch/tests/common/mod.rs` (two fixtures)

**Step 1:** `git worktree add /Users/denis/sidou-songs -b songs-text main`; link the stock
package: `ln -s /Users/denis/sidou/build/stock /Users/denis/sidou-songs/build/stock` (mkdir
`build` first).

**Step 2:** add two fixtures beside `ACID`, `BEAT` and `TUNE`, used by every later test:

```rust
pub const GROOVE: &str = r#"{"format": "sidou-song/1", "name": "GROOVE", "bpm": 125,
  "instruments": {"bass": {"name": "303", "wave": "saw",
    "envelope": {"attack": 0, "decay": 8, "sustain": 8, "release": 8},
    "filter": {"cutoff_hz": 2077, "resonance": 12, "envelope": {"depth": 40, "decay": 6}}}},
  "sections": {"A": {"rows": 16, "voices": [
    {"instrument": "bass", "notes": "C2! . C2~ D#2 . . G2 . C2 . . . A#1 . C2 ."},
    {"instrument": "kit", "notes": "k . h . s . h . k k h . s . h dh"},
    null]}},
  "order": ["A"]}"#;

pub const THREE_PARTS: &str = r#"{"format": "sidou-song/1", "name": "THREE PARTS", "bpm": 116,
  "instruments": {"lead": {"name": "LEAD", "wave": "pulse"}},
  "sections": {
    "A": {"rows": 8, "voices": [
      {"instrument": "bass", "notes": "C2 - . C2 D#2 - . G1"},
      {"instrument": "kit", "notes": "k . s . k k s ."},
      {"instrument": "lead", "notes": "C4 - - - D#4 - - -"}]},
    "B": {"rows": 8, "voices": [
      {"instrument": "bass", "notes": "F2 - . F2 G#2 - . C2"},
      {"instrument": "kit", "notes": "k . s . k k s ."},
      {"instrument": "lead", "notes": "F4 - - - G#4 - G4 -"}]},
    "C": {"rows": 4, "voices": [null, {"instrument": "kit", "notes": "s s s s"}, null]}},
  "order": ["A", "A", "B", "A", "C"]}"#;
```

`THREE_PARTS`' kit voice is the same in A and B (pattern sharing); C leaves voices 1 and 3 out.

**Step 3:** commit (`test(sidou-patch): sidou-song/1 fixtures`).

### Task 2: the format's types and its kind

**Files:**
- Modify: `platform/web/patch_json.h` (after `patch_step`)
- Modify: `platform/web/patch_json.c` (`patch_json_kind`)
- Modify: `host/crates/sidou-patch/src/json.rs` (`song_song`, `check_song`, as `step_made`)
- Create: `host/crates/sidou-patch/tests/patch_song.rs`

**Step 1: the failing test.** `patch_song.rs`, with a `song_of(text) -> Song` helper as
`patch_step.rs` has: `GROOVE` converts and `Song::device().read` takes it; `json::kind` (or the
existing kind check the Rust side uses) tells a song from a patch, a bank and a STEP pattern.

**Step 2:** run `cargo test -p sidou-patch --test patch_song`; it fails to compile.

**Step 3: the types**, sized by the format's limits:

```c
#define PATCH_SONG_FORMAT "sidou-song/1"
#define PATCH_SONG_SECTIONS 16u
#define PATCH_SONG_ROWS 128u
#define PATCH_SONG_ORDER 64u
#define PATCH_SONG_INSTRUMENTS 16u
#define PATCH_SONG_LABEL 8u
enum { PATCH_JSON_SONG_TEXT = PATCH_JSON_STEP_TEXT + 1 };
enum { PATCH_SONG_REST_VOICE = 0xFF };

typedef struct {
    uint8_t note, drum, accent, slide, hold, rest;
} patch_song_cell;

typedef struct {
    char label[PATCH_SONG_LABEL + 1u];
    uint8_t rows;
    uint8_t instr[LIVE_VOICES];
    patch_song_cell cells[LIVE_VOICES][PATCH_SONG_ROWS];
} patch_song_section;

typedef struct {
    char name[PATCH_BANK_NAME + 1u];
    uint16_t bpm, bpm_played10;
    uint8_t sections, order_len, instruments;
    uint8_t order[PATCH_SONG_ORDER];
    char labels[PATCH_SONG_INSTRUMENTS][PATCH_SONG_LABEL + 1u];
    patch_json patches[PATCH_SONG_INSTRUMENTS];
    patch_song_section section[PATCH_SONG_SECTIONS];
    char error[PATCH_JSON_ERROR_MAX];
} patch_song;

int patch_json_read_song(patch_song* sg, const char* text, uint32_t len);
int patch_json_song_song(gt2_song* s, patch_song* sg, uint32_t model);
```

`instr` holds an index into `patches`, or the built-ins (`PATCH_STEP_PULSE`, `_BASS`, `_KIT`
offset past the patches; pick one encoding and name it), or `PATCH_SONG_REST_VOICE`. If the struct
is too big for the wasm's static memory, measure before shrinking (`cells` is 16 x 3 x 128 x 6).

**Step 4:** `patch_json_kind` returns `PATCH_JSON_SONG_TEXT` for the format; stub the reader and
the song so the test links and fails on its assertion, then commit the failing test with the stub.

### Task 3: the reader, field by field, with its errors

**Files:** `platform/web/patch_json.c`, `tests/patch_song.rs`

**Step 1: the failing tests**, one table test, not one test each:

```rust
#[test]
fn every_mistake_is_refused_in_words_a_model_can_act_on() {
    for (edit, want) in [
        (r#""bpm": 125"#, r#""bpm": 400"#, "bpm: is out of range. Allowed: 60 to 300"),
        ...
    ] { ... assert!(err.contains(want), "{err}") }
}
```

Cover, each by editing `GROOVE` or `THREE_PARTS` (a `with(text, from, to)` helper in
`common`): an unknown top field (allowed list), `format`, `name` (as STEP's), `bpm` out of
range, an `instruments` entry that is not a patch (the patch reader's error prefixed
`instruments.bass`), more than 16 instruments, a section without `rows`, `rows` 0 and 129, more
than three voices, an unknown instrument name (`sections.A.voices[0].instrument: no instrument
"lead2". Instruments: bass, kit, pulse`), `order` empty, `order[n]` naming no section (`Sections:
A, B, C`), more than 64 order entries.

**Step 2:** run, fail. **Step 3:** write the reader with the step reader's machinery (`reader`,
`fail`, `syntax`, `string`, `field` tables, `patch_json_read` over each patch's span as the bank
does). Text before the first `{` and after the last `}` is skipped, as for patches.
**Step 4:** run, pass. **Step 5:** commit.

### Task 4: the grid's tokens

**Files:** `platform/web/patch_json.c`, `tests/patch_song.rs`

**Step 1: the failing tests.** Extend the table: `"C#9"` (`token 7 "C#9" is not a note. Allowed:
C0 to G#7, - . and ! or ~ after a note`), `"C2!!"`, `"k"` on a non-kit voice (`drums need the
kit`), `"C2"` on the kit voice (the drum tokens listed), 15 tokens for 16 rows (`15 tokens, rows
is 16`), tabs and double spaces accepted as separators.
**Step 2:** fail. **Step 3:** a `grid_cells(reader*, const char* notes, uint32_t rows, int kit,
patch_song_cell* out)` that splits on whitespace and reads each token; the note name reuses
`note_of`, the drums map `k s h o c t T r dk ds dc dh` to `DRUMS`' order. Token numbers in errors
count from 1. **Step 4:** pass. **Step 5:** commit.

### Task 5: the song: patterns, orderlists, name

**Files:** `platform/web/patch_json.c` (`patch_json_song_song`), `tests/patch_song.rs`

**Step 1: the failing tests:**
- `GROOVE`'s voice 1 rows: a note writes `[GT2_FIRSTNOTE + n, instr, 0, 0]`, `-` writes
  `GT2_REST`, `.` writes `GT2_KEYOFF`, and accent and slide are the locks STEP puts
  (`song_lock_get` or the Rust side's lock reader, as `patch_step.rs` checks them).
- **The same as STEP:** a lane written as `sidou-step/1` (gate legato, the same notes, accent and
  slide) and as one `sidou-song/1` section gives equal pattern rows and locks.
- The kit voice's cells use `SONG_IO_KIT + drum - 1` as STEP's do.
- `THREE_PARTS`' orderlists: voice 2's A and B entries are the same pattern number (shared), each
  voice's list follows `order` and ends in GoatTracker's loop to 0; voices left out in C play an
  empty pattern of C's rows (one shared rest pattern per row count).
- The song's name is `name`, upper case.
- A song too big is refused (`SONG FULL`, naming the sections and rows): a fixture of 16 sections
  of 128 rows with every row a different note on all three voices.

**Step 2:** fail. **Step 3:** implement. Start from `template_read` as STEP does, then:
1. place each instrument with `place_patch` (and the built-ins as STEP picks them);
2. clear the template's patterns and orderlists (read how `song_io_template` lays them out; the
   NEW song has one pattern per voice);
3. for each section and voice build the cells, find an equal pattern already made (compare the
   rows and the locks) or make one (`gt2_pattern_resize`/new pattern edit, then `gt2_set_row`,
   then `song_lock_put`);
4. write each voice's orderlist from `order` with the loop entry (`gt2_*` order edits; read
   `core/song` for the primitives and their refusals);
5. the name, `gt2_saved_recount`, the scene's roles as STEP sets them (`auto`).
Every `gt2_err` that is not `GT2_OK` becomes the song's error in words. **Step 4:** pass.
**Step 5:** commit.

### Task 6: tempo

**Files:** `platform/web/patch_json.c`, `tests/patch_song.rs`

**Step 1: the failing test:** for bpm 60..300 in steps of 5, the converted song's speeds (the
song's tempo as the player starts it; find where `song_io_template` sets 6) give, through
`song_view_bpm10`, the nearest BPM any allowed pair gives, and `bpm_played10` is that value.
**Step 2:** fail. **Step 3:** write the pair search over speeds 2..32 (b = a or a + 1) and set the
song's start tempo (funktempo as GoatTracker stores it: read `third_party/goattracker2` and
`core/player` for the speedtable/F00 form the player honours). **Step 4:** pass.
**Step 5:** commit.

### Task 7: Rust and the CLI

**Files:** `host/crates/sidou-patch/src/json.rs`, `host/crates/sidou-sysex/src/main.rs`,
`host/crates/sidou-sysex/src/patch.rs`, `host/crates/sidou-sysex/tests/patch.rs`

**Step 1: the failing test:** `sidou-sysex song GROOVE.json -o x.sng` writes the bytes
`json::song_song` makes and prints `asked 125, plays 125.3`; a broken file exits non-zero with the
reader's words. Mirror the `step` test.
**Step 2:** fail. **Step 3:** add `Some("song") => patch_main(rest, song_file)` and its usage line.
**Step 4:** pass. **Step 5:** commit.

### Task 8: the site

**Files:** `platform/web/patch_api.c`, `web/patch.js`, `web/library.js`, `web/patch.test.mjs`,
`web/library.html` (only if a label is needed)

**Step 1: the failing tests** in `web/patch.test.mjs` (run against the host build as the others):
`GROOVE` through the wasm gives the host's bytes; `linkOf` uses `#song=` for a song and `fromLink`
reads it back; the played BPM is reported; the wasm stays within 80 KiB.
**Step 2:** `tools/wasm-patch && node web/patch.test.mjs`; fail. **Step 3:** `patch_api.c` gets
the song kind (a static `patch_song`, its error, name and `sp_song_bpm10`); `patch.js` gets
`SONG_KEY = "#song="`; the Library shows a song like a STEP pattern (preview, send, and Edit,
which hands it to the editor through `editSong`). **Step 4:** pass; **check the size**. If over
80 KiB, stop and report the number (the owner decides). **Step 5:** commit.

### Task 9: the emulator

**Files:** `host/crates/emu-gate/tests/first_use.rs` (or a new `song_text.rs` if it reads
better), using `common::put_file`

**Step 1: the test:** `THREE_PARTS` converted, put in the store as the saved song
(`STORE_FILE_SONG`, as `song_bank.rs:414` does), booted, LOADed and played: each section's lead
note heard (`AUDIBLE_RMS`), at its pitch within 25 cents, onsets within `ON_TIME_MS` of their rows
at the played BPM, and C's two silent voices silent (`SILENT_RMS`). Use `first_use.rs`' judges;
move any it needs into `common` rather than copy them.
**Step 2:** run `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo nextest run --release -p emu-gate --test
<file>` after an fm1 build (`tools/fm1-docker cmake --preset fm1 && tools/fm1-docker cmake
--build --preset fm1`). It should pass at once (no firmware change); if it fails, the converter
is wrong: debug it, don't loosen the judges. **Step 3:** commit.

### Task 10: the instructions for models

**Files:** `host/crates/sidou-patch/tests/data/` (a song template beside `prompt-to-patch.md`),
`host/crates/sidou-patch/tests/patch_docs.rs`, `docs/prompt-to-patch-song.md` (generated),
`docs/prompt-to-patch.md` and `-full.md` (generated), `docs/reference/patch.schema.json`,
`tools/web-demo` / `web/check.sh` (copy `llms-song.txt`)

**Step 1: the failing test:** in `patch_docs.rs`, the song file is current against its template
(`SIDOU_PATCH_DOC_UPDATE=1` writes it), every example in it converts, and `llms.txt` names
`llms-song.txt`. The file: the format, the tokens, the drum tokens, the BPM steps, `GROOVE` and
`THREE_PARTS` as the worked examples, and two to four things to try in the editor.
**Step 2:** fail. **Step 3:** write the template and the generator lines; extend the schema with
`sidou-song/1`. Follow the existing instructions' voice (written for the weakest model that may
read it; see the design notes on the instructions). **Step 4:** pass;
`tools/web-demo --check --skip-firmware` passes. **Step 5:** commit.

### Task 11: the docs that stay current, then the merge

**Files:** `README.md`, `web/index.html`, `fmhub.json`, `CHANGELOG.md` (`[Unreleased]`),
`docs/design-notes.md` (the converter's decisions: tempo steps, pattern sharing, the built-ins),
`docs/reference/patch-json.md`, `TESTING.md` (the table's patch row gains `patch_song.rs`),
`docs/ROADMAP.md` (the item done under 1.10)

**Step 1:** write them; `tools/readme-check` passes. **Step 2:** commit. **Step 3:**
`tools/merge-check` in the worktree; on GREEN, fast-forward `main`, push, remove the worktree
and the branch.
