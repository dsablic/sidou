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
Dependencies: Tasks 1-5b and 7's host half need nothing; **Task 6 needs M2b Tasks 3 and 4**
(two chips rendered, the ring's chip ids, `chip_ctl` and the keys' move to chip B);
**Task 7 needs M2b Task 4** (`chip_ctl`, for character locks) and Task 6; **Task 8 needs
M2a Tasks 4 and 5** (the store on the device, and SAVE's gesture).

**Background to read first:**
- GoatTracker 2's `readme.txt`: section 3 (song data: orderlists, patterns, instruments,
  the four tables, **3.5 playback details**: what happens on each tick), 5.1 (playroutine
  options), 6.1 (the `.sng` layout), 6.3 (the sound-effect format). The source:
  `src/gsong.c` (load and save, and `determinechannels`, GoatTracker Stereo's trial read),
  `src/player.s` (the 6502 player, legal opcodes only), `src/gt2reloc.c` (the command-line
  packer and its options), `src/gplay.c` (the editor's own C playroutine: a second reading,
  not the reference). Official release:
  <https://sourceforge.net/projects/goattracker2/files/> (`GoatTracker_2.77.zip`); a git
  fork that keeps `.sng` compatible: <https://codeberg.org/Ranzbak/goattracker2-Qt>.
  Licence GPL-2.0-or-later.
- The design doc, sections 5, 6, 8 (the audio budget), 9 (layer 3: "the register writes
  must match frame for frame") and 11; decision record 0005 ("What sample-accurate
  placement would take": the split render, and "Song and `.sid` playback ... default to
  AUTHENTIC").
- `app/reg_ring.h` (the comment on M3's player writing registers from the interrupt),
  `platform/fm1/audio.c`, `platform/fm1/app.ld` (the RAM/POOL boundary at 0x01C20000),
  `docs/hardware.md` ("Code in SRAM": every function the interrupt reaches is in
  `.ram_text`, no direct call between flash and SRAM, tables the audio path reads are in
  RAM), `host/crates/emu-gate/tests/common/mod.rs` (`render_host`, the budget constants),
  `tests/ram_audio.rs`.
- How SIDOU vendors third-party sources and fetches test-only ones: `platform/fm1/VENDORED.md`,
  `third_party/libresidfp` (a submodule, test-only), `REUSE.toml`, `LICENSING.md`.
- M2b's `app/chip_ctl.h` and `app/keys_route.h` (Task 6 on): chip A's setup, and the keys'
  move to chip B while a song plays.

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

**Budgets:**
- the player's tick **and the block's song edits** ≤ 60 instructions a sample in their
  worst 128-sample block (`PLAYER_WORST_BLOCK`, Task 6), and the whole interrupt within
  2,700 with two chips, the slots and the player;
- **multispeed ≤ 4 on the device**: a frame is then at least 4,987 µs apart, longer than a
  block (2,901 µs), so a block holds at most one frame boundary. At 8 or more two frames
  can fall in one block, about 94 instructions a sample; such songs are refused on the
  device with the reason (`SNG: MULTISPEED 8 (MAX 4)`) and play on the host;
- RAMTEXT within 48 KB (projected ~43 KB after this plan, report it);
- **the song's content region: one contiguous 64 KiB (65,536-byte) block in POOL**, holding
  the arena, the fixed tables and the extension: the block M3's 6502 memory will overlay
  (a song and a `.sid` never play at once). Player state and rings in the RAM region,
  ≤ 4,000 bytes;
- **free RAM (RAM region plus POOL) ≥ 22,000 bytes** after this plan;
- every main-loop pass within `POLL_INSTRUCTIONS`; image ≤ 414,000 bytes after this plan.

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
    both. The device accepts multispeed 1-4 (the budget above).
  - **Versions:** SIDOU writes `GTS5`; reads `GTS3`, `GTS4`, `GTS5` (and `GTS2` as
    `gsong.c` converts it, if the conversion is small; otherwise refuse with a message).
    **Older idents are converted on load**, as GoatTracker's loader does, so saving a song
    read as `GTS2`-`GTS4` writes a different (`GTS5`) file: expected, not a round-trip
    failure. GoatTracker Stereo writes the same ident with six orderlists per subtune: SIDOU
    detects it as `gsong.c`'s `determinechannels` does (trial-read six orderlists), keeps
    channels 4-6 in the model and in the file, plays 1-3 on chip A, and shows
    `STEREO: CHIP B TRACKS IN M5`.
  - **Why the trailer works:** GoatTracker's loader reads the sections in order and closes
    the file without looking further (`gsong.c`'s `loadsong`); quote the lines and the
    version checked. Check, and list in the record, the tools tried: GoatTracker 2.77 (and
    2.76 if at hand), GoatTracker Stereo 2.77, the Qt fork, `gt2reloc`, ChiptuneSAK's reader.
    Any tool that rejects trailing bytes is a compatibility note in the record.
  - **The trailer and stereo detection:** on a small mono file, `determinechannels`'s trial
    read of orderlists 4-6 can run past the GT2 part into the trailer, and a trailer byte
    that passed its end-mark check would make a mono song look stereo. Quote the check from
    `gsong.c` (which bytes it reads and compares, and how far past the GT2 part three trial
    reads can reach: at most three orderlists of up to 255 entries plus their length and
    restart bytes, 3 × 257 = 771 bytes), and **shape the trailer's start so no trial read
    can pass inside it**: the magic and the fixed header contain no byte the check accepts
    (with a `$FF` end mark: no `$FF` in the magic, the version, or the length field, which
    the writer pads with a `PAD` chunk so none of its bytes is `$FF`), followed, if the
    reach is longer than the fixed header, by a **lead-in of `$00` bytes** to the reach's
    end (at most 771 bytes; part of the trailer's length, skipped by readers). State the
    exact lead-in length the check needs.
  - **Round trip rules** (the user asked for exactly what round-trips):
    1. **a canonical `GTS5` file round-trips byte for byte**: one that GoatTracker's own
       load and save leave unchanged. SIDOU, loading and saving such a file without a
       trailer, writes it back identical (SIDOU's writer does what `savesong` does: the
       counts it trims, the order, and **dropping instruments that hold only a name**, as
       `savesong` does). A non-canonical file (an older ident, a name-only instrument,
       untrimmed counts) is saved in canonical form, as GoatTracker would save it;
    2. a SIDOU file opened and saved in GoatTracker loses the trailer: the music is intact,
       SIDOU's extras (locks, chip settings, options) are gone, and SIDOU plays it as a plain
       GT2 song with GoatTracker's default options;
    3. a trailer whose GT2-part CRC no longer matches (a tool kept the bytes but changed the
       song) is partly kept: position-free chunks (options, chip, slots, MIDI, scene) are
       applied; position-bound ones (locks) are dropped, with a toast;
    4. what SIDOU cannot keep: songs over the device's caps (the content region, multispeed
       above 4: Task 2) are refused on the device with the reason (they still load on the
       host).
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
    permissively licensed; Task 3 confirms it passes Klaus Dormann's functional test. The
    harness traps any undocumented opcode the player executes; M3's own 6502 needs illegal
    opcodes for `.sid` files and must rely on Tom Harte's per-opcode tests for them, not on
    this harness.
  - **The player's place:** the audio interrupt, at sample-exact frame boundaries (overview
    decision 2), with the reasons and the cost; song edits reach it only as O(1) steps
    (Task 6).
  - **SID-Wizard** stays in M5 (design section 14), with the reason.
- `docs/reference/song-format.md` (new): the `.sng` layout as SIDOU reads it (a reference,
  citing the readme, not copying it), and **SIDOU's trailer**, byte for byte:
  - after the last pattern: magic `SIDOU-X` and a 0 byte (8 bytes), the trailer version
    (u16, little-endian, 1), its total length (u32, from the magic to the end; no byte
    `$FF`), then the lead-in the stereo check needs (above), then the CRC-32 of every byte
    of the file before the magic (the GT2 part);
  - then chunks: a 4-character tag, a u32 length, the data (the first data byte is the
    chunk's own version). Readers skip unknown tags. Tags in M2: `OPTS` (playroutine
    options), `CHPA` and `CHPB` (each chip's profile, character controls and expert values,
    model, envelope mode), `SLOT` (the effect slots), `LOCK` (parameter locks, Task 7),
    `MIDI` (mode, channels, rows per beat for the clock: M2e), `SCNE` (the SCENE piece and
    the voices' roles: M2f), `KEYS` (the keys' live preset and setup), `PAD` (padding);
    reserved: `TRKB` (M5's chip B tracks), `RIPD` (M5's rip data);
  - then the CRC-32 of the trailer itself (from the magic to before this field);
  - add the magic to `docs/reference/store-format.md`'s "Magic numbers" table (M2a Task 2).
- `docs/plans/2026-10-14-sidou-design.md`: already amended by the plan's commit (overview);
  check that sections 5 and 6 point to these two documents.

**Tests:** none (a document). The reviewer checks the record's claims against `gsong.c` and
the readme.

**Commit:** `docs: decision record 0007, GT2 .sng as the song file, and the trailer's spec`

---

### Task 2: The song model, the `.sng` reader and writer, the trailer

**Files:**
- `core/song/gt2_song.h`, `gt2_song.c` (new, portable C11, no allocation): the model.
  - **One content region** the caller places (`song_region`, `SONG_REGION_BYTES` = 65,536
    on the device; the host builds may use more): the fixed tables at GoatTracker's full
    size (instruments 63 × 25 = 1,575 bytes, the four tables 4 × 255 × 2 = 2,040, the
    strings 96, counts), the **extension area** (`SONG_EXT_BYTES` 4,096: the trailer's
    chunks as the model holds them, locks included), and the **content arena** (the rest,
    about 57,700 bytes) holding the orderlists and the patterns, each found through an
    index (offset, length) so patterns of any length up to 128 rows pack tight. The region
    is one object (M3's 6502 memory overlays the whole of it; overview, RAM), not a static
    inside the song.
  - **Edits in two steps, so the player never reads a half-made pattern** (Task 6):
    - *cell writes:* a pattern row's 4 bytes, an instrument's byte, a table entry, an
      orderlist entry, the tempo: one store each, in place;
    - *rebuilds:* anything that changes a length (insert or delete rows, resize a pattern,
      insert or delete orderlist entries, add or remove a pattern) builds the new pattern
      or orderlist **in free arena space**, never touching bytes in use, and produces a
      **swap**: the index entry's new (offset, length). Applying the swap is one index
      write. The old space is freed only after the swap is known applied (Task 6's
      acknowledgement).
    - The arena keeps **`SONG_ARENA_SPARE` = 1,040 bytes free** at all times (the largest
      pattern, 128 × 4 + 1 = 513 bytes, twice over, plus slack), so any single rebuild has
      room; a song whose content would leave less is refused (`SNG: TOO BIG (58K OF 56K)`).
      **Compaction** is a series of rebuilds (move a pattern into the spare, swap, free the
      old place), each bounded, done by the main loop when fragmentation leaves no 513-byte
      hole.
    - Each primitive is bounded in cost and refuses what would break GoatTracker's rules
      (readme 3.1: transpose before repeat, a pattern number last before RST; 128 rows;
      208 patterns) with an error code. On the host the same functions apply their cell
      writes and swaps at once.
- `core/song/gt2_sng.c` (reader and writer):
  - **Streaming reader:** `gt2_sng_read(song*, reader)` where `reader` hands out bytes in
    pieces (a buffer on the host, the store's file reader on the device, M2a Task 3), so
    the file is never held twice in RAM. Stereo detection as `determinechannels` (Task 1).
  - **Writer:** `gt2_sng_write(song*, writer)`, writing counts and lengths exactly as
    `gsong.c`'s `savesong` does (read it: what it trims, in what order, name-only
    instruments dropped), then the trailer with Task 1's lead-in and `PAD`.
  - Validation with messages short enough for a toast (`SNG: BAD ORDERLIST V2`,
    `SNG: TOO BIG (58K OF 56K)`, `SNG: MULTISPEED 8 (MAX 4)`); every failure leaves the
    song as it was before the call (the device reload rule: Task 8).
- `core/song/song_ext.c` (the trailer): read (verify both CRCs, skip unknown tags, apply
  rule 3 of 0007), write; a model for each M2 chunk (`OPTS` with every `gt2reloc` option,
  `CHPA`/`CHPB`, `SLOT`, `LOCK`, `MIDI`, `SCNE`, `KEYS`), and room for the reserved ones (an
  unknown chunk read from a file is kept and written back unchanged, so a newer SIDOU's
  chunks survive an older one's save), all within `SONG_EXT_BYTES`.
- Bindings in `host/crates/sidou-sys`; the C added to the host build and the `fm1` CMake.

**Tests** (`host/crates/sidou-tests/tests/song.rs`, new):
- **round trip of SIDOU's own output:** a song written by SIDOU, read and written again, is
  byte for byte the same file (with and without a trailer); and canonical `GTS5` fixtures
  generated in the test from Rust (no name-only instruments, trimmed counts) round-trip byte
  for byte. (GoatTracker's own examples are canonicalised by GoatTracker first and
  round-trip-tested in Task 3, which builds `gsong.c`.)
- name-only instruments dropped on save, as `savesong` does; a `GTS4` fixture saves as
  `GTS5`;
- **stereo detection with a trailer:** mono files of every size from the smallest valid
  song up, each with a trailer, detect as mono (SIDOU's reader, and a Rust transcription of
  `determinechannels` run over the file); stereo files detect as stereo;
- the trailer: written, read back equal; with an unknown chunk kept; a flipped byte in the
  GT2 part drops `LOCK` and keeps the rest; a flipped byte in the trailer drops the whole
  trailer (the song still loads); no byte of the length field is `$FF`;
- the caps: a song over the arena is refused with its size in the message; multispeed 8 is
  refused on the device build's caps; the host build's large region loads every example;
- **edits:** each rule refusal; every primitive's rebuild touches only free space (a test
  build poisons the in-use bytes' shadow and checks none changed before the swap); random
  edit sequences, with compaction, keep the arena consistent and the spare ≥ 1,040 bytes
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
  relocation code it compiles with, `gsong.c`), `readme.txt`, `copying`, `examples/*.sng`.
  A `VENDORED.md` with the zip's URL and SHA-256, the file list and any build shim. REUSE:
  GPL-2.0-or-later, © Lasse Öörni and the contributors in `authors`. **Test-only:** never
  built into the firmware; `LICENSING.md` says so. If an unmodified build is impossible
  (SDL or editor globals), a SIDOU shim file (GPL-3.0-or-later) provides the stubs, and
  `VENDORED.md` lists them.
- `host/crates/gt2-ref/` (new crate, `publish = false`, test-only):
  - `build.rs` builds `gt2reloc`'s packing path with `cc` into a library, exposing
    `gt2ref_pack(sng bytes, options) -> (prg bytes, load address, init, play)` and
    **`gt2ref_canonical(sng bytes) -> sng bytes`** (GoatTracker's own `loadsong` then
    `savesong`); or, if `main` cannot be split cleanly, builds `gt2reloc` as an executable
    the crate runs (and the canonicaliser as a second small executable over `gsong.c`).
  - `src/cpu.rs`: a `Cpu6502` trait (reset, step, registers) and its `mos6502` (0.10.1)
    implementation, with a 64 KiB memory whose writes to `$D400-$D418` are logged in order.
    **The harness traps at run time:** before each step it checks the opcode at PC against
    the 151 documented NMOS opcodes and fails the trace (with the address and the frame) on
    any other; it fails likewise on `SED` executed if the crate's decimal mode is not proven
    by Dormann's test. A packed player that runs only documented opcodes in binary mode is
    then proven by execution, not by a scan.
  - `src/trace.rs`: pack the song for a subtune and options, load the `.prg`, call init
    (A = subtune) and then play once per frame through a trampoline that ends in a known
    address; return each frame's write list. A step limit per call catches a hang.
  - `src/bin/gt2-trace.rs`: `gt2-trace song.sng --subtune 0 --frames 3000 [options]`
    prints the frames' writes (for debugging the C player).
- `third_party/6502-tests/`: Klaus Dormann's `6502_functional_test.bin` (and its listing)
  at a pinned commit (GPL-3.0, test-only, REUSE annotated), with a `VENDORED.md`.

**Tests (in `gt2-ref`, and the canonical round trip in `sidou-tests`):**
- the CPU: Dormann's functional test passes on the `mos6502` implementation (decimal mode
  included if the crate supports it; if not, record it, and the `SED` trap stands);
- the traps: a hand-made `.prg` with an undocumented opcode fails the trace with its
  address; every example song traces without a trap;
- the harness: an example song's first frames are stable across runs; a packed song's init
  and play addresses match `gt2reloc`'s report; options change the packed code as expected
  (multispeed, NTSC tables);
- **the canonical round trip** (`sidou-tests/tests/song.rs`, with `gt2-ref` as a dev
  dependency): **every example in `examples/`, canonicalised first** by
  `gt2ref_canonical`, loads in SIDOU's reader and saves to identical bytes; the test also
  reports how many examples were already canonical as shipped.

**Docs:** `LICENSING.md`, `CREDITS.md` (GoatTracker: the format, the reference player),
`TESTING.md` (the harness, `gt2-trace`, the traps; a note that M3's 6502 needs Tom Harte's
tests for illegal opcodes), `BUILDING.md` (nothing new to install, if `cc` builds it).

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

### Task 5a: The C player, part 2: pulse, filter and speed tables, every command

**Files:** `core/player/gt2_play.c`, tests.

**Spec:**
- The **pulsetable**, the **filtertable** (run every tick, as readme 3.5 says), the
  **speedtable** (vibrato, portamento speeds; the fine-vibrato option's table reading);
- every pattern command `0XY-FXY` (readme 3.2), including the one-shot commands letting
  `1XY-4XY` or instrument vibrato continue underneath, funktempo (`EXY`) and per-channel
  tempo (`F8X+`), the master volume and the timing mark (`DXY`).

**Tests:** **every example song, every subtune,** each frame's writes equal the
reference's, 3,000 frames or two loops, with GoatTracker's default options (keep the run
under a few minutes with `--release`; a long run `--ignored`); plus hand-made fixtures,
generated from Rust, for each command and each table's jumps and special values, every
frame equal. Report the examples' coverage (songs, subtunes, frames).

**Commit:** `feat(player): GT2 player in C, part 2: tables and commands; register match on every example`

---

### Task 5b: The C player, part 3: the options, generated songs, the events

**Files:** `core/player/gt2_play.c`, a Rust song generator
(`host/crates/gt2-ref/src/gen.rs`), tests.

**Spec:**
- **The options:** pulse and realtime optimisations (what each skips on which tick),
  multispeed (`-M`: the play routine called N times a frame; the C player is called N times
  too; the device accepts N ≤ 4, the host any N GoatTracker does), NTSC, buffered writes;
  the zero-page ghost registers change nothing the C player writes (the reference's copy
  loop is part of the harness);
- a **row and note event** out of each frame (for Task 6's event ring, M2d's views, M2e's
  clock, M2f's scene): per channel, a new row (pattern, row, orderlist position), a note-on
  (note, instrument), the gate's state.

**Tests:**
- **the options matrix:** pulse and realtime optimisation on and off, multispeed 2 and 4
  (and 8 on the host only), NTSC, buffered writes; all examples with the defaults (Task 5a)
  and three examples with the full matrix, each frame equal;
- **generated songs:** a Rust generator of valid random songs (instruments, tables with
  jumps and every command, patterns using every command and special note, random options
  within the device's caps), a few hundred seeds, every frame equal; a failing seed is
  printed and becomes a fixture;
- the events: every note-on the reference's writes imply (a gate rising in `$D404`) is
  reported, on the right frame.

**Docs:** `TESTING.md` (the register-match tests, how to run the long ones, `gt2-trace`).

**Commit:** `feat(player): GT2 player in C, part 3: options, generated songs, row and note events`

---

### Task 6: The player on the device: in the audio interrupt, frame-exact

**Needs:** M2b Tasks 3 and 4 (`chip_ctl`, `keys_route`).

**Prerequisites from the review of Tasks 4-5b** (2026-10-16; each is part of this task):
- **The shape goes stale on live edits.** `gt2_play_init` runs greloc's analysis once and
  the player follows that shape (decision record 0007, "The player's corners"); an edit
  can change it (a new note range, a first command of a class, a wave delay or command, a
  pulse low nybble, a channel's first non-rest). So:
  - split `gt2_shape_analyse(song, opts, &shape)` (the main loop's, walks the song) from
    `gt2_play_start` and `gt2_play_set_shape` (O(1) each, the interrupt's);
  - **widen** the shape in O(1) for each edit (note range, command class, wave delay or
    command, pulse nybble, channel used), sent in the same command-ring batch as the edit,
    so the player never plays an edit with a shape that lacks it;
  - **narrow** it by a background analysis, sliced across polls, and a shape swap;
  - a change of the channel count goes through the shadow player's state hand-over (the
    seek path), not a swap;
  - a differential test: an edit stream played with the incremental shape equals the
    player re-initialised on the edited song and fast-forwarded to the same frame;
  - a note in 0007: while editing, the target is "what the packer would make of the song
    as it is now".
- **Init's cost.** The orderlist pass rescans each named pattern once per orderlist entry
  (typically ~60k steps, up to ~25M at the limits: 32 subtunes, 254 entries, 128 rows).
  Keep a "not empty" bit per pattern (updated by the row writes) and slice the analysis
  across polls, or run it behind a loading state; either way within `POLL_INSTRUCTIONS`.
- **A call's worst case against the gate.** `PLAYER_WORST_BLOCK` (60 instructions a sample
  × 128 samples ≈ 7,680 a block) holds one call and 8 edits. `order_at` is O(entries), and
  a broken orderlist's read walks past its restart into the next ones. Cache each
  channel's restart as a model position (updated by an orderlist swap), and flatten a
  broken orderlist's packed byte stream at analysis time. Measure the worst call in
  fm1-emu over the examples and the generated songs.
- **RAMTEXT.** The player's frame path is ~6.3 KB at `-O2` (~5.7 KB at `-Os`); init and
  the analysis ~3.1 KB. Put init and the analysis in a flash file (`gt2_shape.c`); build
  the frame path at `-Os`; move the cold below-the-table reads (`var_byte`, `group_byte`)
  into the analysis (a table it fills); check that `song_init`'s zeroing emits no
  `memset`/`memcpy` call. The overview budgeted ~5 KB for all of this task's RAMTEXT; the
  estimate is now **~7.5 KB** (the frame path at `-Os`, the rings, the edit application
  and the split render), so RAMTEXT after M2 is ~45.5 KB of 48 (the overview says so).
- **Multispeed timing.** A C64 calls a multispeed or 25 Hz tune from a CIA timer of
  `0x4cc7/N + 1` cycles (PAL; NTSC `0x42c6/N + 1`), gt2reloc's `.sid` speed code: 25 Hz is
  39,311 cycles (two video frames are 39,312), NTSC ×3 is 3 × 5,699 = 17,097 (a frame is
  17,095). Decide what "frame-exact" means for N ≠ 1 (the CIA's period, or the video
  frame divided by N) and record it in 0007; the host render follows the same choice.
- **Row events.** Channels the packer does not play (`gt2_player.channels` < 3) emit no
  row events; M2d's cursors must follow their orderlists another way.
- **A UI hint.** When `freq_unknown` fires (a note past B-7, 0007), or init reports
  `GT2_PLAY_WARN_PITCH` (OPTS asks for another A-4), the SONG view shows a hint.

**keys_route's contract (M2b review, app/keys_route.h):** the transport starts a song only
once `keys_route_ready_for_song` (the keys have left chip A, or KEYS is A); the setup chip A
is to have (the song's `CHPA`, at the start and at every change) goes through the want path,
`keys_route_song_wants`, the one writer of `keys_route.song_want`, which keys_route brings
chip A to in the background (the CHIP view's A tab edits the same want: the latest wins), and
the song saves `song_want` back into its `CHPA` (`keys_route_song_setup` only records a setup
chip A already holds); a profile change of chip A during a song is a prepared chip reset, and
the reset chip has none of the song's registers: the player's register shadow rewrites them
after the swap (`app/reg_ring.c`, above); on the falling edge of a voice's bit in
`keys_route_voices_taken` the player rewrites that voice's registers. Measure the shared
pass: the song control's writes to chip A (`REG_RING_SONG_PASS`, 28) in one pass with
keys_route taking chip A back to the keys' setup or moving the keys (30) and chip B following
(4) must stay within `REG_RING_WORST_PASS`; and under KEYS A, both chips following in one
pass (chip A the song's setup, chip B the keys': up to 8 sink writes each, 4 of them queued)
with chip A's character tick every song tick.

**Files:** `core/player/song_ctl.h`/`.c` (new, portable: the command and event rings, the
frame clock, the edit application), `platform/fm1/audio.c`/`.h`, `app/reg_ring.c` (the swap
re-applies the player's registers), `platform/fm1/app.ld` (`.ram_text` rules, the RAM/POOL
boundary), `platform/fm1/main.c`, `app/panel.c` (PLAY), `app/keys_route.c` (the song-playing
input becomes real), `app/demo_song.c` (generated) and `assets/songs/` (the first song),
emu-gate tests, `tests/common/mod.rs`, `docs/hardware.md`.

**Spec:**
- **The frame clock:** a PAL frame is 19,656 C64 cycles at 985,248 Hz (50.125 Hz), NTSC
  17,095 at 1,022,727 Hz; the sample rate is 24 MHz / 544. `song_ctl` keeps the exact
  rational position of the next frame in samples (no drift over hours), times the
  multispeed factor (1-4). `audio_fill` renders chip A up to the frame's sample, runs
  `gt2_play_frame` writing **straight into chip A** with `sid_write`, and renders the rest
  (decision record 0005's split render; with multispeed ≤ 4 a block holds at most one
  frame boundary). Chip B and the drain are as before; the drain runs first, at the block's
  start.
- **Chip A's registers from two sources:** the player (in the interrupt) and chip A's
  `chip_ctl` (resets and character, through the ring). On a swap (a profile change of chip
  A), the drain re-writes the player's shadow of the 25 registers into the new chip before
  the block renders, so the song plays on through a profile change. Update the comment in
  `reg_ring.h` that M3 was to revisit: the player is the first writer outside the drain, on
  one core, in the same interrupt, so it never races the drain's commits.
- **PLAY and the chips (M2b Task 4):** PLAY moves the keys to chip B and, if the song has a
  `CHPA` that differs from chip A's setup, prepares chip A first; the transport starts on
  the first block after chip A is ready (measure the delay: ≤ 13 ticks). Stop hands chip A
  back to the keys' setup as M2b Task 4 says.
- **Songs default to AUTHENTIC** envelopes (gate reset off), per decision record 0005;
  chip A's envelope mode is the song's (`CHPA`), not the instrument's.
- **The command ring** (main loop to interrupt, SPSC, applied at a block's start):
  play (subtune, from an orderlist position), stop (gates off through the player's
  key-off, so releases ring as on the C64), continue, mute mask (a muted voice's writes are
  dropped after a key-off is written; solo is a mask), a state hand-over (for M2d's seeks:
  the main loop fast-forwards a shadow player and hands its state over), jam notes (a note
  and an instrument on a voice, for M2d's audition and M2e's multitimbral mode), song edits
  (below), the frame period (M2e's clock following).
- **Song edits in the interrupt are O(1) only.** The interrupt applies exactly two kinds,
  between frames, so the player never reads half an edit:
  - a **cell write** (Task 2: a pattern row's 4 bytes, an instrument byte, a table entry,
    an orderlist entry, the tempo), at most 4 bytes;
  - an **index swap**: a pattern's or an orderlist's (offset, length) entry replaced by one
    the main loop has already built in free arena space (Task 2's rebuilds).
  Each applied edit is acknowledged in the event ring with its sequence number; only then
  does the main loop free a swapped-out pattern's space. At most **`SONG_EDITS_PER_BLOCK`
  = 8** edits are applied in a block; more wait for the next block. Nothing in the
  interrupt loops over a pattern, an orderlist or the arena.
- **The event ring** (interrupt to main loop): Task 5b's row and note events with the frame
  number, and the edits' acknowledgements; sized for a second of events; overflow drops the
  oldest row event (never an acknowledgement: their ring part is sized for the command
  ring's depth) and counts it.
- **Where things live:**
  - the **content region** (Task 2's `song_region`, 65,536 bytes, one block, aligned to
    4 KiB) in POOL, with a link-map symbol (`__song_region`) so M3 can overlay the 6502's
    memory on it; the player's state, its shadow registers and the rings in the RAM
    region (≤ 4,000 bytes);
  - **the RAM/POOL boundary:** POOL had 99,248 bytes free at the start of M2; chip B
    (35,300) and the region (65,536) need 100,836. Move the boundary in `app.ld` down by
    **8 KiB** (POOL from 0x01C1E000; the RAM region 64 KiB), keeping at least 4 KiB free
    in each, and update `docs/hardware.md`'s layout table. Report both regions' free bytes;
  - `gt2_play_frame`, `song_ctl`'s interrupt side and the split in `.ram_text` by `app.ld`
    rules, with no call into flash (no `memcpy`/`memset` from `libc_min.c`, which is in
    flash: write loops, or put the helpers in `.ram_text` too). The frequency tables in
    `.data` (RAM), as `docs/hardware.md` requires for tables the audio path reads. Compile
    the player with `-Os` if RAMTEXT gets short (it runs once a frame).
- **The first song:** `assets/songs/` gets a short original loop for tests and demos,
  written for SIDOU (a script or a `.sng` made in GoatTracker), GPL-3.0-or-later, with a
  metadata file (title, author, licence, source: design section 11). A converter
  (`host/crates/sidou-tests/src/song_embed.rs`, as `logo.rs` does for bitmaps) writes
  `app/demo_song.c`; boot copies it into the region (the interrupt never reads flash data).
  PLAY (button 12) plays and stops it until M2d's transport arrives.

**Tests (fm1-emu, `tests/song.rs`, new):**
- **bit-exact:** the firmware plays the demo song from PLAY; the host renders the same song
  (extend `render_host`: the player, chip A, chip B out of use, the slots, with the same
  frame clock) and the outputs are equal sample for sample over 10 s, PAL, and with
  multispeed 4;
- **the budget:** `PLAYER_WORST_BLOCK` (new in `common/mod.rs`, fails above **60**
  instructions a sample over the block): the player's tick **plus `SONG_EDITS_PER_BLOCK`
  of the costliest edit command** (an index swap or a 4-byte cell write, whichever costs
  more, measured), over the example songs embedded one at a time in a test build,
  multispeed 1 and 4; and the whole interrupt with two chips at their worst, the slots'
  heaviest and the player within `AUDIO_TARGET`; report the 6502's remaining headroom;
- **edits while playing:** a stream of cell writes and swaps (a test hook) while the song
  plays: the output equals the host's render of the same edits at the same blocks; no
  block applies more than 8; a swapped-out pattern's space is reused only after its
  acknowledgement;
- a profile change of chip A while the song plays: no gap (the song's writes continue on
  the new chip; the output stays bit-exact with the host doing the same);
- PLAY with a `CHPA` that differs: the start delay reported, and the keys sound on chip B;
- a multispeed 8 song is refused with its toast;
- `ram_audio.rs`: the interrupt runs only `.ram_text`, no relocation crosses, no load from
  XIP; report RAMTEXT;
- the command ring: stop releases (envelopes decay), continue resumes on the next frame,
  mute silences a voice within a frame, a flood of commands never overflows silently;
- the map: `__song_region` is 65,536 bytes, 4 KiB aligned, in POOL; free RAM (RAM region
  plus POOL) ≥ 22,000 bytes, and each region ≥ 4 KiB free.

**Docs:** `docs/hardware.md` ("Code in SRAM": the player; the layout table with the moved
boundary and the region), decision record 0005 (an amendment: songs are frame-exact through
the split render), `docs/reference/controls.md` (PLAY, until M2d), `TESTING.md`,
`CHANGELOG.md`.

**Commit:** `feat(fm1): the song player in the audio interrupt, frame-exact, bit-exact with the host`

**Results (2026-10-16, measured in fm1-emu):** the player's worst block 30.7 instructions a
sample (of 60; a call at most 3,409, eight cell writes 515); RAMTEXT 43,284 of 49,152 bytes;
the analysis's slices at most 33.8k (of SONG_SLICE_INSTRUCTIONS 39k), PLAY to the first call
32 ms; the image 398 KB; free RAM 22,116 bytes (the RAM region 16,420, POOL 5,696), 116 over
the floor of 22,000. **The lever if RAM runs short:** the two shape buffers (2 x 2,168
bytes in the RAM region), mostly each channel's flattened orderlist and its entry map (2 x
768 bytes a buffer): keep the map only for orderlists that break readme 3.1's rules, or
derive the model's entry from the packed byte in the main loop when it reads an event.

---

### Task 7: Parameter locks

**Needs:** Task 6 and M2b Task 4 (`chip_ctl`, for the character locks).

**Files:** `core/song/song_ext.c` (the `LOCK` chunk), `core/player/song_ctl.c` (applying
them), `app/chip_ctl.c` (character locks), tests.

**Spec:**
- A lock is (channel's pattern number, row, parameter, value). Parameters in M2: the 11-bit
  cutoff, the resonance, the filter mode, the voice's routing into the filter, the 12-bit
  pulse width (per voice), and the chip's character: age/heat, DAC sloppiness, thump,
  bleed. (STEP, M2d Task 5, writes waveform changes as GoatTracker's own `7XY` command,
  which persists, and everything else as locks, which hold for one row.)
- **Semantics (as a parameter lock on a step sequencer):** the locked value replaces the
  parameter **for that row only**; at the next row without a lock, the song's own value
  returns. The player's own state is not changed by a lock, so a song's register match
  without locks is untouched, and a filter or pulse table keeps running underneath.
- **Filter and pulse locks** are applied in the interrupt, after the player's writes in
  each frame of the row (the locked register's value replaces the player's for those
  frames). The lookup is O(1) per row: the main loop resolves the next row's locks (from
  the row event) into a small per-row table the interrupt reads (at most 8 entries).
- **Character locks** go to the main loop as events (the row event says which lock), and
  `chip_ctl` moves the control as a MIDI CC would, with its glide (0006: 0.32 s end to end):
  coarse by nature, documented as such.
- At most `SONG_LOCKS_MAX` (512) locks a song, kept sorted in the extension area; adding or
  removing one is a rebuild of the `LOCK` table in the extension's free space and a swap,
  as Task 2's patterns.

**Tests:** host: a song with locks plays the reference's writes except the locked
registers on the locked rows, which hold the lock's value, and return after; locks
round-trip through the trailer; edits that move or delete rows move or delete their locks
(the editing primitives of Task 2 carry them); the CRC rule drops them; fm1-emu: a locked
cutoff audible as a darker row (spectrum), bit-exact with the host; `PLAYER_WORST_BLOCK`
still ≤ 60 with locks on every row.

**Commit:** `feat(song): parameter locks for the filter, the pulse and the character`

---

### Task 8: Songs on the device: the store, the SONG menu, About's music

**Needs:** M2a Task 4 (the store on the device) and M2a Task 5 (SAVE's gesture, `save_key`).

**Files:** `app/song_io.h`/`.c` (new: load and save through the store's files),
`app/views.c` (a SONG item), `app/save_key.c` (the song views' precedence), `app/about.c`,
`app/settings.c` (the last song, a migration), `assets/songs/` (an About tune), tests.

**Spec:**
- **Load:** stop the transport, then stream the store file through `gt2_sng_read` (M2a
  Task 3's reader) straight into the region, in slices that keep each main-loop pass within
  the poll allowance. RAM has no room for a second region, so there is no staging copy: on
  a failure the previous song is reloaded from its own file (a new, unsaved song is lost;
  the load asks first if the song has unsaved changes). A refused song leaves a toast with
  the reason.
- **Save:** SAVE's press in a song view (M2a Task 5's table: STEP, TRACKER and its ORDER,
  INSTR and TABLE pages) writes the current song to its file (the trailer included:
  options, chip A's and the keys' setups, slots, locks), streamed through the store:
  `store_file_begin` claims the file's sectors first (M2a Task 3), then the writer streams.
  `STORE_BUSY` follows M2a Task 4's rule (wait for quiet, then held erases after 2 s).
- **Saves and wear (a constraint):** a save erases as many sectors as the song has (a 64 KB
  song, 18). Never autosave a large song by the minute (a 64 KB autosave every minute, 8 h a
  day, wears the store out in about 1.2 years, `docs/reference/store-format.md`, "Wear"), and
  skip a save, explicit or background, when the song has not changed since its last save
  (a SAVE of an unchanged song toasts `SONG SAVED` and writes nothing). Any autosave added
  later saves on a change, at most every few minutes, and not for songs over a few sectors.
- **The idle timeout:** an open store file that takes no byte for 5 s is aborted
  (`store_file_idle`, M2a). A save streams from RAM and never waits that long; but any
  protocol that streams a song into a file and acknowledges each block only once it is
  programmed (M2e's `PUT`), with blocks under 256 bytes, must count the flash's program time
  in the gap between blocks: at the slowest window (Task 3b's numbers) a block's bytes may
  take several passes to program, and the sender waits for each `ACK`.
- **Edits are frozen while a save streams:** from `store_file_begin` to `store_file_end`
  (about 3 s for a 64 KiB song at 64 bytes a window, less for most), every edit the song
  views or MIDI would make is refused with the toast `SAVING`, and the views show `SAVING`
  in their header; playback, mute, solo and jam notes go on (they do not change the song).
  A snapshot was the alternative; it would need a second 64 KiB region. So the file is
  always the song as it was when SAVE was pressed.
- **The SONG menu item** opens a list: LOAD (the store's songs), NEW (the template below),
  SAVE AS (a name editor: SELECT moves the cursor, ALGORITHM steps the character, 16
  characters), DELETE (a confirm). Within 200 cells a frame and the poll allowance.
- **The new-song template:** one subtune; channels 1-3 each an orderlist of one 64-row
  pattern; instrument 1 a pulse lead with a pulse table, instrument 2 a bass, instrument 3
  a noise drum; tempo 6 (125 BPM at 4 rows a beat, 50 Hz).
- **The last song** loads at boot (settings), stopped.
- **About's music, decided** (moved from M1 to M2, CHANGELOG's M1 status): About plays a
  short original tune on chip A while it shows, **if nothing is playing and the loaded song
  has no unsaved changes**: it copies the tune (from `app/demo_song.c`, flash) into the
  region, plays it, and on close reloads the loaded song from its file, stopped (the load
  above, a few passes). With unsaved changes, About shows its scroller silent, with
  `SAVE YOUR SONG FOR MUSIC` in it. No second region: RAM has no room for one (overview).

**Tests:** fm1-emu: save a song, reboot, load it, it plays bit-exact with the host; a
corrupt file is refused with a toast and the old song stays; a song over the cap is refused
with its size; an edit during a save is refused with `SAVING` and the saved file equals the
song at SAVE's press; NEW, SAVE AS with a name, DELETE; About plays its tune and the song
comes back unchanged after; About with unsaved changes is silent and the song untouched;
every pass within the poll allowance during a load and a save; goldens of the SONG pages.

**Docs:** `docs/reference/controls.md` (SONG, SAVE in the song views), `docs/art/README.md`
(the SONG pages), `docs/hardware-test.md` (save, power off, load), `CHANGELOG.md`.

**Commit:** `feat(app): songs on the store: load, save, new, About's music`

**Capacity decision (2026-10-17, the user's option 1):** the content arena's limit
does not imply that the canonical file fits the store. Empty, unheld patterns below
the highest saved pattern expand to 64 rests on disk. Device songs therefore also
have a **65,536-byte serialized-size cap, trailer included**. The model maintains
the saved size incrementally; an edit that would exceed it is refused with
`SONG FULL`, leaving the song unchanged. Loads whose saved form would exceed it
are refused with a read error and a size reason. No pattern renumbering, silent
data loss, or store-format change is permitted. Host reference songs can retain
the larger host region's capacity.

**Original results (2026-10-18, `6e41eb6` base; superseded by the integration results below).**
- `song_io` loads and saves store files through a bounded `gt2_stream` state machine;
  there is no second content region. Store-file CRCs are checked before destructive
  loading. A preflight refusal preserves RAM; a later failure reloads the previous file.
  Invalid device chip setups are refused rather than leaving PLAY waiting forever.
- SONG offers LOAD, NEW, a 16-character SAVE AS editor and confirmed DELETE. It reuses
  the song views' canvas, including their 200-cell copy budget. SAVE's precedence lives
  in the existing views router; `save_key`'s clean-release/learn gesture is unchanged.
- SAVE freezes new model edits as soon as the action is accepted, including a pool
  wait: stronger than the original begin-to-end rule, to preserve the requested snapshot.
  Previously accepted edits finish first. Small chip/keys/slot metadata shares the
  existing 256-byte producer buffer until the file begins. Playback, mute, solo and jam
  continue; the two forced-write owners (song and settings) cannot cancel each other.
- Point-edit accounting reads one actual predecessor row in the interrupt, regardless
  of pattern length; releasing a cell reads no pattern rows. Separate interrupt and
  main-loop instrument counters support overlapping writes and cancellation. Structural
  rebuilds account their old/new content during their existing bounded structural work;
  neither kind reserializes the song. The size total and its highest-used summaries are cached.
- The capacity regression saves at 64,658, 64,914, 65,170 and 65,426 bytes; the next
  addition is refused unchanged. Canonical oversized loads return `GT2_READ_SONG_FULL`
  (`-2`). Random edits, cancellation, counter wrap and queued trailing-pattern removal
  are regression-tested. The model header is 10,844 bytes; device arena capacity is
  51,992 bytes, inside the same 65,536-byte region.
- CHPA captures the unlocked song setup, including explicit CHIP edits during a lock.
  KEYS also appends the preset identity, which determines voice allocation; older readers
  ignore that optional byte. Settings schema 4 appends the last song after schema 3's
  THEME byte and schema 2's TRACKER ROWS; all four schemas have migration fixtures.
  On the `051a1a1` base THEME is reserved and retained, default 0 HIGH CONTRAST,
  until the readable-screens implementation lands.
- About uses the existing original SIDOU LOOP, from `demo_song`, and restores the loaded
  song stopped. Dirty songs stay silent with SAVE YOUR SONG FOR MUSIC. No PUT or large-song
  autosave was added. Flash timings remain M2a's provisional datasheet timings.
- Full COMMON checks all exit 0. Application 508,480 bytes of 512,000; RAMTEXT
  44,512 of 49,152; RAM free 8,228 and POOL free 5,696, together 13,924 against main's
  9,000-byte floor (M2a, M2d and M2f). No bound was loosened.
- The player gate charges 5,036 + 592 measured isolated row-apply instructions = 5,628
  a block, 44.0/sample of 60. SONG I/O's tested busiest combined pass is 58,233 of 120,000.
  SONG pages peak at 108 cells, 9.32 ms SPI and a 31,620-instruction `ui_poll`.
  STATUS idle stays 7,496.6/frame of <7,500; HOME stays 429 of <430. CHIP's initial
  225-instruction regression was removed: K1 is 217 of <220.
- Goldens: the menu gains SONG; four new SONG pages. FIRE's flicker and RULE 30's
  automaton differ only in animation phase from the longer menu walk; both old/new PNGs
  were viewed. Other existing LCD goldens are unchanged.

---

### Task 8 integration results: `051a1a1` base

Task 8 now includes the base's large-text menu/popups/toasts, USER profiles,
MASTER/tail gate and M2e transport changes. SONG's headings, actions and
confirmations use 2× text; SAVE AS splits 16 characters into two eight-character
lines, while directory filenames remain 1× to fit completely.

Bank 1 recalls the zero-based song in name order, stopped. Lookup advances one
directory entry per eligible poll and skips other file types. Dirty or busy
recalls are refused, missing programs preserve the song, and a newer request can
replace a pending lookup. Only MIDI programs 0..127 are addressable; recalls do
not queue behind a file operation. Saves/loads include MIDI mode/options and
rows per beat; missing MIDI chunks retain the current setup. CLOCK/CLOCK OUT
stay global, and successful loads discard stale transport requests.

Schema 4 is 124 bytes: TRACKER ROWS at 106, reserved THEME at 107 (default 0,
HIGH CONTRAST), last-song at 108..123. Schema 1/2/3/4 fixtures and per-schema
host tests verify defaults, unchanged prefixes, exact round trips and short
record refusal. THEME is retained opaquely until readable-screens lands; the
coordinator's next rebase remains separate from this `051a1a1` integration.

The authoritative unlocked CHPA base is initialized on every load, including
stopped MIDI ownership. Explicit profile/envelope/four-control edit masks keep
USER/CC values that equal active locks, while rejected edits preserve identity.
The inherited MULTI/WHOLE A device player-callback gap and USER-profile saves
capturing shown lock values remain follow-up work, not Task 8 implementations.

Compared with `051a1a1`: application 423,736 → **452,276 bytes** (sidou-pack),
RAMTEXT 45,108 → **45,216 / 49,152 bytes**, combined free RAM 13,284 →
**11,508 bytes** against the unchanged **7,000-byte** floor. The final free RAM
is 5,812 in RAM plus 5,696 in POOL. Package 610,090 bytes, loader 8,216 bytes.

All seven changed old/new goldens were viewed: menu count 1/14 → 1/15; scrolled
menu moves ROWS offscreen and includes SONG; four SONG pages become large text;
RULE 30's lower rows change only in animation phase. No tolerances were changed.

The full schema-4 screen run passes all 58 tests: STATUS idle **7,472.4/frame**
against <7,500; HOME to THE OSCILLATOR **426** against <430; CHIP K1 maximum
**211** against <215; SELECT to RULE 30 **227** against <230. HOME's integration
regression at 430 was removed by ordering the CHIP route before ABOUT. Large
SONG pages peak at **35,410 ui_poll instructions**, **200 cells / 17.15 ms SPI**.
MIDI mapped/learned CC floods are **4,942,343 / 4,937,099.5 instructions/s**
against <5M; LIVE full-speed flood **5,927,048/s** against <6.3M; USB 16k CC
flood **10,479,408/s** against <10.5M; USB drain **7,991** against <8,000.
The full-speed stream and program-change floods lose no bytes or restarts.
The player sweep covers **193 runs of 145 songs**: worst block **5,070 + 592
isolated row-apply instructions = 5,662**, **44.2/sample** against 60; two-chip
and slot worst-case assumptions leave **161.3/sample** of the 2,700 target.
All five bank and six song-store emulator tests pass. Their maximum combined
passes are **93,335 / 120,000** for bank integration and **60,800 / 120,000**
for store operations, including concurrent forced song/settings saves.
The complete COMMON checklist exits **0** on schema 4: fm1/spike builds, strict
GCC 13, fmt, Clippy, workspace release, UBSan, sidou-pack and subsequent tests,
full emu-gate with both build variables, REUSE and web-demo. The full emulator
run passes **182 tests**, including all 58 screen tests. Each long check has its
own recorded exit code; externally interrupted earlier runs were not counted.

### Task 9: Docs and the M2c status

- `CHANGELOG.md`, `TESTING.md`, `LICENSING.md`, `CREDITS.md`, `docs/hardware.md`, this
  plan's tasks marked done with their results: the player's worst block with edits, the
  6502's remaining headroom, RAMTEXT, POOL and the RAM region (free RAM against the
  ≥ 22,000 gate), the region's address, the image against the ceiling, the register-match
  coverage (songs, subtunes, option combinations, generated seeds, examples canonical as
  shipped); `flash_budget.rs`'s `IMAGE_CEILING` moved to the schedule's M2c entry
  (414,000) or lowered to the measured image plus a margin.

**Commit:** `docs: M2c complete, the song engine's results`

---

## Risks

- **`player.s` corners** (undocumented behaviour, illegal gate-off timer values that halt
  GoatTracker's playback) need test songs; the generated-song test will find most. A corner
  the C player cannot match without cost is recorded in decision record 0007 with its
  frames, never hidden by loosening the test.
- **The harness's build:** `gt2reloc` may pull in editor code; the shim approach is the
  fallback. If even that fails, run `gt2reloc` as an external tool built once, with its
  path in an environment variable, and skip the match tests without it (as the emulator
  tests skip without a build): weaker, so prefer the library.
- **RAMTEXT** (projected ~43 KB of 48): the player at `-Os`; M3 needs room (overview).
- **The arena** (about 56 KB usable after the spare) refuses some big GT2 songs on the
  device; the host and the companion still handle them. Report how many of the examples fit.
- **Frame timing inside a frame** differs from the 6502's by design (0007).

<!-- REUSE-IgnoreEnd -->
