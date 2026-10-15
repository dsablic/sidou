<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# M2c: The song engine, implementation plan

<!-- REUSE-IgnoreStart -->

> **For Claude:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> to implement this plan task by task.

**Goal:** songs. GoatTracker 2's `.sng` is SIDOU's song file (the user's decision,
2026-10-15): SIDOU reads and writes it directly, with its own extras in a trailer
GoatTracker ignores. A C player on the device plays it, and the host proves the player
right by running GoatTracker's own packed 6502 player beside it and comparing every
frame's SID writes. On the device the player runs in the audio interrupt at sample-exact
frame boundaries, so a song renders on the device bit for bit as on the host.

**Overview and budgets:** [2026-10-15-m2-overview.md](2026-10-15-m2-overview.md).
Dependencies: Tasks 1-5 and 7's host half need nothing; Task 6 needs M2b Task 3 (two chips
rendered, the ring's chip ids); Task 8 needs M2a Task 4 (the store on the device).

**Background to read first:**
- GoatTracker 2's `readme.txt`: section 3 (song data: orderlists, patterns, instruments,
  the four tables, **3.5 playback details**: what happens on each tick), 5.1 (playroutine
  options), 6.1 (the `.sng` layout), 6.3 (the sound-effect format). The source:
  `src/gsong.c` (load and save), `src/player.s` (the 6502 player, legal opcodes only),
  `src/gt2reloc.c` (the command-line packer and its options), `src/gplay.c` (the editor's
  own C playroutine: a second reading, not the reference). Official release:
  <https://sourceforge.net/projects/goattracker2/files/> (`GoatTracker_2.77.zip`); a git
  fork that keeps `.sng` compatible: <https://codeberg.org/Ranzbak/goattracker2-Qt>.
  Licence GPL-2.0-or-later.
- The design doc, sections 5, 6, 9 (layer 3: "the register writes must match frame for
  frame") and 11; decision record 0005 ("What sample-accurate placement would take": the
  split render, and "Song and `.sid` playback ... default to AUTHENTIC").
- `app/reg_ring.h` (the comment on M3's player writing registers from the interrupt),
  `platform/fm1/audio.c`, `platform/fm1/app.ld`, `docs/hardware.md` ("Code in SRAM": every
  function the interrupt reaches is in `.ram_text`, no direct call between flash and SRAM,
  tables the audio path reads are in RAM), `host/crates/emu-gate/tests/common/mod.rs`
  (`render_host`, the budget constants), `tests/ram_audio.rs`.
- How SIDOU vendors third-party sources and fetches test-only ones: `platform/fm1/VENDORED.md`,
  `third_party/libresidfp` (a submodule, test-only), `REUSE.toml`, `LICENSING.md`.

**Rules:** as in [M2a](2026-10-15-m2a-storage.md) ("Rules"): SPDX headers, the
`Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` trailer after a blank
line, stage only your own files, the host, device and emulator commands, the GCC check in
`gcc:13`, checks by exit code, bounds only tighten, CI only by hand, no tags. Plus:
- **Licensing:** GoatTracker's code is GPL-2.0-or-later. It is used (a) as test-only
  reference code built by the host harness, never linked into the firmware, and (b) as the
  specification SIDOU's player follows. Where SIDOU's C follows `player.s` routine for
  routine, or takes its data (the frequency tables, the vibrato and portamento
  arithmetic), the file says so, carries an `SPDX-FileCopyrightText` line for Lasse Öörni
  and GoatTracker's contributors besides SIDOU's, and stays GPL-3.0-or-later, which
  GPL-2.0-or-later's "any later version" permits. `LICENSING.md` and `CREDITS.md` record it.
  Never copy GoatTracker's editor or UI code.
- Every reader of a file is a parser of untrusted input (design section 6): fuzzed, failing
  with a clear message, never a crash.

**Budgets:** the player's tick ≤ 60 instructions a sample in its worst 128-sample block,
and the whole interrupt within 2,700 with two chips, the slots and the player (Task 6);
RAMTEXT within 48 KB (projected ~42 KB after this plan, report it); POOL: the song's
content arena and tables ≤ 41 KB; every main-loop pass within `POLL_INSTRUCTIONS`; image
≤ 414,000 bytes after this plan.

---

### Task 1: The format decision and the extension's specification

A research and writing task; no code. Everything later in M2 cites it.

**Files:**
- `docs/decisions/0007-gt2-song-format.md` (new decision record):
  - **Context:** the user's decision; the alternatives (a SIDOU chunked format with a GT2
    importer, as design section 5 had it) and why GT2 itself is better (overview).
  - **What a `.sng` holds** (readme 6.1, checked against `gsong.c`): header `GTS5`, three
    32-byte strings, subtune count; per subtune and channel an orderlist (length, data, the
    `$FF` RST and its restart position; `$00-$CF` patterns, `$D0-$DF` repeats, `$E0-$FE`
    transposes); instruments (count, then 25 bytes each from 1: AD, SR, wave, pulse and
    filter pointers, vibrato parameter, vibrato delay, gate-off timer, first-frame wave,
    16-byte name); four tables (wave, pulse, filter, speed: a count, then left bytes, then
    right bytes); patterns (count, then per pattern a row count and 4 bytes a row:
    note `$60-$BC`, rest `$BD`, key off `$BE`, key on `$BF`, end `$FF`; instrument; command;
    data). Limits: 32 subtunes, 254 orderlist entries, 63 instruments, 255 rows a table,
    208 patterns, 128 rows a pattern.
  - **What is not in the file:** the playroutine options GoatTracker keeps in its
    configuration or on `gt2reloc`'s command line (readme 5.1, `gt2reloc.c`): the hard
    restart ADSR (`-A`, default `$0F00`), multispeed (`-M`), fine vibrato, pulse and realtime
    optimisations (`-O`, `-R`), NTSC, buffered writes, zero-page ghost registers, the SID
    model. SIDOU stores them in its trailer (the `OPTS` chunk) so a song plays the same in
    both.
  - **Versions:** SIDOU writes `GTS5`; reads `GTS3`, `GTS4`, `GTS5` (and `GTS2` as
    `gsong.c` converts it, if the conversion is small; otherwise refuse with a message).
    Saving a song read as `GTS3` or `GTS4` writes `GTS5` (round trip rule 1 is for `GTS5`).
    GoatTracker Stereo writes the same ident with six orderlists per subtune: SIDOU detects
    it as `gsong.c`'s fork does (trial-read six orderlists), keeps channels 4-6 in the model
    and in the file, plays 1-3 on chip A, and shows `STEREO: CHIP B TRACKS IN M5`.
  - **Why the trailer works:** GoatTracker's loader reads the sections in order and closes
    the file without looking further (`gsong.c`'s `loadsong`); quote the lines and the
    version checked. Check, and list in the record, the tools tried: GoatTracker 2.77 (and
    2.76 if at hand), GoatTracker Stereo 2.77, the Qt fork, `gt2reloc`, ChiptuneSAK's reader.
    Any tool that rejects trailing bytes is a compatibility note in the record.
  - **Round trip rules** (the user asked for exactly what round-trips):
    1. a GT2 file without a trailer, loaded and saved unchanged by SIDOU, is byte for byte
       the same file (SIDOU writes counts and lengths as GoatTracker's `savesong` would, so
       a GoatTracker-saved file is canonical);
    2. a SIDOU file opened and saved in GoatTracker loses the trailer: the music is intact,
       SIDOU's extras (locks, chip settings, options) are gone, and SIDOU plays it as a plain
       GT2 song with GoatTracker's default options;
    3. a trailer whose GT2-part CRC no longer matches (a tool kept the bytes but changed the
       song) is partly kept: position-free chunks (options, chip, slots, MIDI, scene) are
       applied; position-bound ones (locks) are dropped, with a toast;
    4. what SIDOU cannot keep: nothing of the GT2 part; songs over the device's caps (Task 2)
       are refused on the device with the reason (they still load on the host).
  - **Equivalence, defined:** for each frame, the ordered list of writes to `$D400-$D418`
    made by one call of the player equals the C player's. The 6502 spreads its writes over
    hundreds of cycles inside a frame; the C player's land at the frame's sample. That is
    the documented difference (with buffered writes on, GoatTracker itself writes a channel
    at once). The SID's state after each frame is therefore the same, and so is everything a
    listener can hear except sub-millisecond timing within a frame.
  - **The reference harness** (Task 3): GoatTracker's packed player, relocated by
    `gt2reloc` with the song's options, in a host-only 6502 (the `mos6502` crate, 0.10.1,
    BSD-3-Clause: <https://crates.io/crates/mos6502>), behind a trait so M3's own 6502 can be
    checked against it later. The design's device 6502 stays in M3. Considered: `w65c02s`
    (a 65C02, not NMOS), `emulator_6502`, `yamos6502` (MIT): `mos6502` is the most used and
    permissively licensed; Task 3 confirms it passes Klaus Dormann's functional test.
  - **The player's place:** the audio interrupt, at sample-exact frame boundaries (overview
    decision 2), with the reasons and the cost.
  - **SID-Wizard** stays in M5 (design section 14), with the reason.
- `docs/reference/song-format.md` (new): the `.sng` layout as SIDOU reads it (a reference,
  citing the readme, not copying it), and **SIDOU's trailer**, byte for byte:
  - after the last pattern: magic `SIDOU-X` and a 0 byte (8 bytes), the trailer version
    (u16, little-endian, 1), its total length (u32, from the magic to the end), the CRC-32
    of every byte of the file before the magic (the GT2 part);
  - then chunks: a 4-character tag, a u32 length, the data (the first data byte is the
    chunk's own version). Readers skip unknown tags. Tags in M2: `OPTS` (playroutine
    options), `CHPA` and `CHPB` (each chip's profile, character controls and expert values,
    model, envelope mode), `SLOT` (the effect slots), `LOCK` (parameter locks, Task 7),
    `MIDI` (mode, channels, rows per beat for the clock: M2e), `SCNE` (the SCENE piece and
    the voices' roles: M2f), `KEYS` (chip B's live preset and settings); reserved: `TRKB`
    (M5's chip B tracks), `RIPD` (M5's rip data);
  - then the CRC-32 of the trailer itself (from the magic to before this field).
- `docs/plans/2026-10-14-sidou-design.md`: already amended by the plan's commit (overview);
  check that sections 5 and 6 point to these two documents.

**Tests:** none (a document). The reviewer checks the record's claims against `gsong.c` and
the readme.

**Commit:** `docs: decision record 0007, GT2 .sng as the song file, and the trailer's spec`

---

### Task 2: The song model, the `.sng` reader and writer, the trailer

**Files:**
- `core/song/gt2_song.h`, `gt2_song.c` (new, portable C11, no allocation): the model.
  - Fixed tables at GoatTracker's full size: instruments (63 × 25 bytes), the four tables
    (4 × 255 × 2), the strings, subtune count.
  - A **content arena** (`SONG_ARENA_BYTES`, 32 KiB on the device; the host builds can use
    more) holding the orderlists and the patterns, each found through an index
    (offset, length) so patterns of any length up to 128 rows pack tight. This arena is the
    one M3's 6502 memory will share (overview, RAM): keep it a separate object the caller
    places (POOL on the device), not a static inside the song.
  - Editing primitives that keep the arena packed (insert or delete rows, resize a
    pattern, add or remove a pattern, edit an orderlist), each bounded in cost, each
    refusing what would break GoatTracker's rules (readme 3.1: transpose before repeat,
    a pattern number last before RST; 128 rows; 208 patterns) with an error code.
- `core/song/gt2_sng.c` (reader and writer):
  - **Streaming reader:** `gt2_sng_read(song*, reader)` where `reader` hands out bytes in
    pieces (a buffer on the host, the store's file reader on the device, M2a Task 3), so
    the file is never held twice in RAM.
  - **Writer:** `gt2_sng_write(song*, writer)`, writing counts and lengths exactly as
    `gsong.c`'s `savesong` does (read it: what it trims, in what order), then the trailer.
  - Validation with messages short enough for a toast (`SNG: BAD ORDERLIST V2`,
    `SNG: TOO BIG (41K OF 32K)`); every failure leaves the song as it was before the call.
- `core/song/song_ext.c` (the trailer): read (verify both CRCs, skip unknown tags, apply
  rule 3 of 0007), write; a model for each M2 chunk (`OPTS` with every `gt2reloc` option,
  `CHPA`/`CHPB`, `SLOT`, `LOCK`, `MIDI`, `SCNE`, `KEYS`), and room for the reserved ones (an
  unknown chunk read from a file is kept and written back unchanged, so a newer SIDOU's
  chunks survive an older one's save).
- Bindings in `host/crates/sidou-sys`; the C added to the host build and the `fm1` CMake.

**Tests** (`host/crates/sidou-tests/tests/song.rs`, new):
- **round trip, byte for byte:** every `.sng` in GoatTracker's `examples/` (test data, from
  Task 3's third-party copy, or Task 2 copies them first) loads and saves to identical bytes;
- the trailer: written, read back equal; with an unknown chunk kept; a flipped byte in the
  GT2 part drops `LOCK` and keeps the rest; a flipped byte in the trailer drops the whole
  trailer (the song still loads);
- the caps: a song over the arena is refused with its size in the message; the host
  build's large arena loads every example;
- editing primitives: each rule refusal; random edit sequences keep the arena consistent
  (an independent Rust model of the song compared after each step);
- **fuzz:** random bytes and mutated example files never crash the reader (ASan and UBSan);
  if the workspace has a `cargo fuzz` setup, add a target `sng_read`, else a proptest-style
  test with a fixed seed count plus an `--ignored` long run.

**Commit:** `feat(song): GT2 .sng read and written as SIDOU's song file, with its trailer`

---

### Task 3: The reference harness: GoatTracker's player in a host 6502

**Files:**
- `third_party/goattracker2/` (new): from the official `GoatTracker_2.77.zip`, unmodified,
  only what the harness builds and the tests read: `src/gt2reloc.c` and what it needs
  (`player.s`, `altplayer.s`, the assembler under `src/asm/`, the headers and the song and
  relocation code it compiles with), `readme.txt`, `copying`, `examples/*.sng`. A
  `VENDORED.md` with the zip's URL and SHA-256, the file list and any build shim. REUSE:
  GPL-2.0-or-later, © Lasse Öörni and the contributors in `authors`. **Test-only:** never
  built into the firmware; `LICENSING.md` says so. If an unmodified build is impossible
  (SDL or editor globals), a SIDOU shim file (GPL-3.0-or-later) provides the stubs, and
  `VENDORED.md` lists them.
- `host/crates/gt2-ref/` (new crate, `publish = false`, test-only):
  - `build.rs` builds `gt2reloc`'s packing path with `cc` into a library, exposing
    `gt2ref_pack(sng bytes, options) -> (prg bytes, load address, init, play)`; or, if its
    `main` cannot be split cleanly, builds `gt2reloc` as an executable the crate runs.
  - `src/cpu.rs`: a `Cpu6502` trait (reset, step, registers) and its `mos6502` (0.10.1)
    implementation, with a 64 KiB memory whose writes to `$D400-$D418` are logged in order.
  - `src/trace.rs`: pack the song for a subtune and options, load the `.prg`, call init
    (A = subtune) and then play once per frame through a trampoline that ends in a known
    address; return each frame's write list. A step limit per call catches a hang.
  - `src/bin/gt2-trace.rs`: `gt2-trace song.sng --subtune 0 --frames 3000 [options]`
    prints the frames' writes (for debugging the C player).
- `third_party/6502-tests/`: Klaus Dormann's `6502_functional_test.bin` (and its listing)
  at a pinned commit (GPL-3.0, test-only, REUSE annotated), with a `VENDORED.md`.

**Tests (in `gt2-ref`):**
- the CPU: Dormann's functional test passes on the `mos6502` implementation (decimal mode
  included if the crate supports it; if not, record it: GoatTracker's player does not use
  decimal mode, which the test checks by scanning the packed code for `SED`);
- the harness: an example song's first frames are stable across runs; a packed song's init
  and play addresses match `gt2reloc`'s report; options change the packed code as expected
  (multispeed, NTSC tables).

**Docs:** `LICENSING.md`, `CREDITS.md` (GoatTracker: the format, the reference player),
`TESTING.md` (the harness, `gt2-trace`), `BUILDING.md` (nothing new to install, if `cc`
builds it).

**Commit:** `test(gt2-ref): GoatTracker's packed player in a host 6502, as the player's reference`

---

### Task 4: The C player, part 1: orderlists, patterns, notes, instruments, the wavetable

**Files:** `core/player/gt2_play.h`, `gt2_play.c` (new, portable, no allocation, no libm, no
calls into libc: the device runs it from SRAM, Task 6), tests.

**Spec (this part):**
- `gt2_play_init(player*, const song*, subtune, const gt2_options*)` and
  `gt2_play_frame(player*, gt2_write_fn sink, void* ctx)`: one call is one call of
  GoatTracker's play routine; it emits the SID writes it makes, in `player.s`'s order for
  the options (unbuffered: as each channel's code reaches them; buffered: a channel's
  registers together at its end), then the filter's.
- This part covers: the orderlists (patterns, repeats, transposes, RST and restart), the
  tempo and its ticks (readme 3.5: tick 0 initialisation, tick 1 audible, the gate-off timer
  ticks before the next row, hard restart with the global HR ADSR and per instrument `$80`
  and `$40` bits, the first-frame waveform and its `$00/$FE/$FF` meanings), notes, rest, key
  off and key on with the gate mask, instruments (AD, SR, pointers), the **wavetable**
  (delays, waveforms, inaudible waveforms, absolute and relative notes, jumps, its commands
  `F0-FE`), the frequency tables (PAL and NTSC, as `player.s`'s data).
- `player.s` is the specification; `gplay.c` is a second reading where the assembly is
  unclear (and where they differ, the packed player wins: the test says so).

**Tests** (`host/crates/gt2-ref/tests/match.rs` or `sidou-tests/tests/gt2_match.rs`, needing
the harness): for songs that use only this part's features (write small `.sng` fixtures in
the test, generated from Rust so they need no editor), every frame's writes equal the
reference's, for at least 3,000 frames (60 s) or two loops of the song, on PAL, with the
default options. On a mismatch, report the frame, the channel's state and both write lists.

**Commit:** `feat(player): GT2 player in C, part 1: orderlists, notes, instruments, wavetable`

---

### Task 5: The C player, part 2: pulse, filter, speed tables, commands, options

**Files:** `core/player/gt2_play.c`, tests.

**Spec:**
- The **pulsetable**, the **filtertable** (run every tick, as readme 3.5 says), the
  **speedtable** (vibrato, portamento speeds; the fine-vibrato option);
- every pattern command `0XY-FXY` (readme 3.2), including the one-shot commands letting
  `1XY-4XY` or instrument vibrato continue underneath, funktempo (`EXY`) and per-channel
  tempo (`F8X+`), the master volume and the timing mark (`DXY`);
- the options: pulse and realtime optimisations (what each skips on which tick),
  multispeed (`-M`: the play routine called N times a frame; the C player is called N times
  too), NTSC, buffered writes; the zero-page ghost registers change nothing the C player
  writes (the reference's copy loop is part of the harness);
- a **row and note event** out of each frame (for Task 6's event ring, M2d's views, M2e's
  clock, M2f's scene): per channel, a new row (pattern, row, orderlist position), a note-on
  (note, instrument), the gate's state.

**Tests:**
- **every example song, every subtune,** each frame's writes equal the reference's, 3,000
  frames or two loops, with GoatTracker's default options; then a matrix of options (pulse
  and realtime optimisation on and off, multispeed 2 and 4, NTSC, buffered writes) on a
  subset (all examples with defaults, three with the matrix: keep the run under a few
  minutes with `--release`; a long run `--ignored`);
- **generated songs:** a Rust generator of valid random songs (instruments, tables with
  jumps and every command, patterns using every command and special note), a few hundred
  seeds, every frame equal; a failing seed is printed and becomes a fixture;
- the events: every note-on the reference's writes imply (a gate rising in `$D404`) is
  reported, on the right frame.

**Docs:** `TESTING.md` (the register-match tests, how to run the long ones, `gt2-trace`).

**Commit:** `feat(player): GT2 player in C, part 2: tables, commands, options; register match on every example`

---

### Task 6: The player on the device: in the audio interrupt, frame-exact

**Files:** `core/player/song_ctl.h`/`.c` (new, portable: the command and event rings and
the frame clock), `platform/fm1/audio.c`/`.h`, `app/reg_ring.c` (the swap re-applies the
player's registers), `platform/fm1/app.ld` (`.ram_text` rules), `platform/fm1/main.c`,
`app/panel.c` (PLAY), `app/demo_song.c` (generated) and `assets/songs/` (the first song),
emu-gate tests, `tests/common/mod.rs`.

**Spec:**
- **The frame clock:** a PAL frame is 19,656 C64 cycles at 985,248 Hz (50.125 Hz), NTSC
  17,095 at 1,022,727 Hz; the sample rate is 24 MHz / 544. `song_ctl` keeps the exact
  rational position of the next frame in samples (no drift over hours), times the
  multispeed factor. `audio_fill` renders chip A up to the frame's sample, runs
  `gt2_play_frame` writing **straight into chip A** with `sid_write`, and renders the rest
  (decision record 0005's split render; a block can hold one frame boundary, two at high
  multispeed). Chip B and the drain are as before; the drain runs first, at the block's
  start.
- **Chip A's registers from two sources:** the player (in the interrupt) and chip A's
  `chip_ctl` (resets and character, through the ring). On a swap (a profile change of chip
  A), the drain re-writes the player's shadow of the 25 registers into the new chip before
  the block renders, so the song plays on through a profile change. Update the comment in
  `reg_ring.h` that M3 was to revisit: the player is the first writer outside the drain, on
  one core, in the same interrupt, so it never races the drain's commits.
- **Songs default to AUTHENTIC** envelopes (gate reset off), per decision record 0005;
  chip A's envelope mode is the song's (`CHPA`), not the instrument's.
- **The command ring** (main loop to interrupt, SPSC, applied at a block's start):
  play (subtune, from an orderlist position), stop (gates off through the player's
  key-off, so releases ring as on the C64), continue, mute mask (a muted voice's writes are
  dropped after a key-off is written; solo is a mask), a state hand-over (for M2d's seeks:
  the main loop fast-forwards a shadow player and hands its state over), jam notes (a note
  and an instrument on a voice, for M2d's audition and M2e's multitimbral mode), edits to
  the song (M2d: applied between frames, so the player never reads half an edit), the
  frame period (M2e's clock following). Each command is bounded in cost.
- **The event ring** (interrupt to main loop): Task 5's row and note events with the frame
  number; sized for a second of events; overflow drops the oldest and counts it.
- **Where things live:** the song's fixed tables and content arena in POOL; the player's
  state and the rings in RAM; `gt2_play_frame`, `song_ctl`'s interrupt side and the split
  in `.ram_text` by `app.ld` rules, with no call into flash (no `memcpy`/`memset` from
  `libc_min.c`, which is in flash: write loops, or put the helpers in `.ram_text` too). The
  frequency tables in `.data` (RAM), as `docs/hardware.md` requires for tables the audio path
  reads. Compile the player with `-Os` if RAMTEXT gets short (it runs once a frame).
- **The first song:** `assets/songs/` gets a short original loop for tests and demos,
  written for SIDOU (a script or a `.sng` made in GoatTracker), GPL-3.0-or-later, with a
  metadata file (title, author, licence, source: design section 11). A converter
  (`host/crates/sidou-tests/src/song_embed.rs`, as `logo.rs` does for bitmaps) writes
  `app/demo_song.c`; boot copies it into the arena (the interrupt never reads flash data).
  PLAY (button 12) plays and stops it until M2d's transport arrives.

**Tests (fm1-emu, `tests/song.rs`, new):**
- **bit-exact:** the firmware plays the demo song from PLAY; the host renders the same song
  (extend `render_host`: the player, chip A, chip B silent, the slots, with the same frame
  clock) and the outputs are equal sample for sample over 10 s, PAL, and with multispeed 4;
- **the budget:** the player's worst block, in instructions a sample over the block
  (`PLAYER_WORST_BLOCK`, new in `common/mod.rs`, fails above **60**), over the example songs
  embedded one at a time in a test build, multispeed 1 and 4; and the whole interrupt with
  two chips at their worst, the slots' heaviest and the player within `AUDIO_TARGET`;
  report the 6502's remaining headroom;
- a profile change of chip A while the song plays: no gap (the song's writes continue on
  the new chip; the output stays bit-exact with the host doing the same);
- `ram_audio.rs`: the interrupt runs only `.ram_text`, no relocation crosses, no load from
  XIP; report RAMTEXT;
- the command ring: stop releases (envelopes decay), continue resumes on the next frame,
  mute silences a voice within a frame, a flood of commands never overflows silently.

**Docs:** `docs/hardware.md` ("Code in SRAM": the player), decision record 0005 (an
amendment: songs are frame-exact through the split render), `docs/reference/controls.md`
(PLAY, until M2d), `TESTING.md`, `CHANGELOG.md`.

**Commit:** `feat(fm1): the song player in the audio interrupt, frame-exact, bit-exact with the host`

---

### Task 7: Parameter locks

**Files:** `core/song/song_ext.c` (the `LOCK` chunk), `core/player/song_ctl.c` (applying
them), `app/chip_ctl.c` (character locks), tests.

**Spec:**
- A lock is (channel's pattern number, row, parameter, value). Parameters in M2: the 11-bit
  cutoff, the resonance, the filter mode, the voice's routing into the filter, the 12-bit
  pulse width (per voice), and the chip's character: age/heat, DAC sloppiness, thump,
  bleed. (STEP's native-first rule, M2d Task 5, puts what GoatTracker can say itself into
  the pattern's command column instead.)
- **Semantics (as a parameter lock on a step sequencer):** the locked value replaces the
  parameter **for that row only**; at the next row without a lock, the song's own value
  returns. The player's own state is not changed by a lock, so a song's register match
  without locks is untouched, and a filter or pulse table keeps running underneath.
- **Filter and pulse locks** are applied in the interrupt, after the player's writes in
  each frame of the row (the locked register's value replaces the player's for those
  frames).
- **Character locks** go to the main loop as events (the row event says which lock), and
  `chip_ctl` moves the control as a MIDI CC would, with its glide (0006: 0.32 s end to end):
  coarse by nature, documented as such.
- At most `SONG_LOCKS_MAX` (512) locks a song, kept sorted for a lookup per row in bounded
  time.

**Tests:** host: a song with locks plays the reference's writes except the locked
registers on the locked rows, which hold the lock's value, and return after; locks
round-trip through the trailer; edits that move or delete rows move or delete their locks
(the editing primitives of Task 2 carry them); the CRC rule drops them; fm1-emu: a locked
cutoff audible as a darker row (spectrum), bit-exact with the host.

**Commit:** `feat(song): parameter locks for the filter, the pulse and the character`

---

### Task 8: Songs on the device: the store, the SONG menu, About's music

**Files:** `app/song_io.h`/`.c` (new: load and save through the store's files),
`app/views.c` (a SONG item), `app/about.c`, `app/settings.c` (the last song, a
migration), `assets/songs/` (an About tune), tests.

**Spec:**
- **Load:** stop the transport, then stream the store file through `gt2_sng_read` (M2a
  Task 3's reader) straight into the arena, in slices that keep each main-loop pass within
  the poll allowance. RAM has no room for a second arena, so there is no staging copy: on a
  failure the previous song is reloaded from its own file (a new, unsaved song is lost;
  the load asks first if the song has unsaved changes). A refused song leaves a toast with
  the reason.
- **Save:** SAVE (M2a Task 5's press) writes the current song to its file (the trailer
  included: options, chip A's and B's settings, slots, locks), streamed through the store.
- **The SONG menu item** opens a list: LOAD (the store's songs), NEW (the template below),
  SAVE AS (a name editor: SELECT moves the cursor, ALGORITHM steps the character, 16
  characters), DELETE (a confirm). Within 200 cells a frame and the poll allowance.
- **The new-song template:** one subtune; channels 1-3 each an orderlist of one 64-row
  pattern; instrument 1 a pulse lead with a pulse table, instrument 2 a bass, instrument 3
  a noise drum; tempo 6 (125 BPM at 4 rows a beat, 50 Hz).
- **The last song** loads at boot (settings), stopped.
- **About's music** (moved from M1 to M2, CHANGELOG's M1 status): About plays a short
  original tune on chip A while it shows, if no song is playing, and restores the loaded
  song's state when it closes. The tune lives in flash (`app/demo_song.c`), copied into
  the arena only if no song is loaded, or played from a second, small arena (decide from
  the RAM figures; say which).

**Tests:** fm1-emu: save a song, reboot, load it, it plays bit-exact with the host; a
corrupt file is refused with a toast and the old song stays; a song over the cap is refused
with its size; NEW, SAVE AS with a name, DELETE; About plays and stops its tune; every pass
within the poll allowance during a load and a save; goldens of the SONG pages.

**Docs:** `docs/reference/controls.md` (SONG, SAVE), `docs/art/README.md` (the SONG pages),
`docs/hardware-test.md` (save, power off, load), `CHANGELOG.md`.

**Commit:** `feat(app): songs on the store: load, save, new, About's music`

---

### Task 9: Docs and the M2c status

- `CHANGELOG.md`, `TESTING.md`, `LICENSING.md`, `CREDITS.md`, `docs/hardware.md`, this
  plan's tasks marked done with their results: the player's worst block, the 6502's
  remaining headroom, RAMTEXT, POOL, the image against the ceiling, the register-match
  coverage (songs, subtunes, option combinations, generated seeds).

**Commit:** `docs: M2c complete, the song engine's results`

---

## Risks and open questions

- **`player.s` corners** (undocumented behaviour, illegal gate-off timer values that halt
  GoatTracker's playback) need test songs; the generated-song test will find most. A corner
  the C player cannot match without cost is recorded in decision record 0007 with its
  frames, never hidden by loosening the test.
- **The harness's build:** `gt2reloc` may pull in editor code; the shim approach is the
  fallback. If even that fails, run `gt2reloc` as an external tool built once, with its
  path in an environment variable, and skip the match tests without it (as the emulator
  tests skip without a build): weaker, so prefer the library.
- **RAMTEXT** (projected ~42 KB of 48): the player at `-Os`; M3 needs room (overview).
- **The content arena's size** (32 KiB) refuses some big GT2 songs on the device; the host
  and the companion still handle them. Report how many of the examples fit.
- **Frame timing inside a frame** differs from the 6502's by design (0007).

<!-- REUSE-IgnoreEnd -->
