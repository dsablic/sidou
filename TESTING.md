<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Testing SIDOU

All tests, benchmarks and tools live in the cargo workspace in `host/`. They
call the C core through FFI (`sidou-sys`), so they test the same code that
runs on the device. Commands below run from `host/`. [BUILDING.md](BUILDING.md)
lists the prerequisites.

## Quick reference

| Layer | Command | Where |
|---|---|---|
| Everything except the emulator gate | `cargo test --workspace --release` | all crates |
| Unit tests | `cargo test --release -p sidou-tests --test unit` | `crates/sidou-tests/tests/unit/` |
| App logic (live instrument, register ring, MIDI parser, ring and input, the CC/NRPN map, expression, MIDI out, screen font, cell grid, views and widgets, the store and its layout) | `cargo test --release -p sidou-tests --test app` | `crates/sidou-tests/tests/app/`; `SIDOU_FREQ_TABLE_UPDATE=1` regenerates `app/live_freq.c`, `SIDOU_FONT_UPDATE=1` regenerates `app/font.c` and `app/font.h` from `assets/font/sidou-8x8.txt`, `SIDOU_LOGO_UPDATE=1` `app/logo_bitmap.c` from `assets/logo/metal-chrome.png` and `cracktro-chrome.png` and `app/title_card_bitmap.c` from `assets/logo/title-*.png`, `SIDOU_MIDI_DOC_UPDATE=1` `docs/reference/midi.md` |
| Song files (the `.sng` reader and writer, the trailer, the song model's edits) | `cargo test --release -p sidou-tests --test song`; the long fuzz and edit runs with `-- --ignored` | `crates/sidou-tests/tests/song.rs`, `src/song.rs` |
| Song player reference (GoatTracker's packer and player in a host 6502, Dormann's 6502 test) | `cargo test --release -p gt2-ref`; the assembler cross-check with `--test assembler -- --ignored` (fetches GoatTracker 2.77 into `build/`); `cargo run --release -p gt2-ref --bin gt2-trace -- song.sng` prints a song's writes; **the cross-check**: the same packed player on SIDOU's own 6502 (`core/cpu6502`, three builds), every call's ordered SID writes, cycles and return registers equal to `mos6502`'s, over every example, subtune and option set (`--test sidou_cpu`) and the showcase songs (`cargo test --release -p sidou-tests --test showcase_cpu`) | `crates/gt2-ref/`, `crates/sidou-tests/tests/showcase_cpu.rs` |
| The PSID environment (calls, the bank register, periods and speed bits, PAL/NTSC, late calls, the limits, slicing at every size 1-5,000, the count's wrap), on tunes assembled at test time | `cargo test --release -p sidou-tests --test psid_env` | `crates/sidou-tests/tests/psid_env.rs`, `src/psid_fixtures.rs` (strict-mode `gt2_ref::assemble`), [docs/reference/psid.md](docs/reference/psid.md) |
| The PSID oracle: SIDOU's PSID environment against m6502.h + FM-1 SID's `psid.c`, call by call, with the expected differences (GoatTracker's examples and SIDOU's songs, every subtune, at `$D400` and as 2SID at `$D420`; Task 6's fixtures; the vendored files and the derived copy) | `cargo test --release -p psid-ref`; the user's tunes with `SIDOU_PSID_DIR=<dir> ... --test oracle -- --ignored` (report in `build/psid-ref/report.txt`) | `crates/psid-ref/`, [the section below](#the-psid-oracle-m3) |
| The .sid player (`core/psid/psid_ctl.h`: the look-ahead, the write ring, each write placed at its cycle; against a reference render clocked write by write at the stamps over the oracle's corpus, with skipped main-loop passes, the split budget, a slow init, pause, stop, song changes, 2SID and mutes, the ring's wrap; on fm1-emu each look-ahead pass within `PSID_SLICE_WORST`: `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate --test cpu6502_budget a_look_ahead -- --nocapture`) | `cargo test --release -p sidou-tests --test psid_ctl` | `crates/sidou-tests/tests/psid_ctl.rs`, `src/psid.rs` (the host render) |
| .sid tunes on the device (M3 Task 9: a tune put into the store's flash before the boot, played from the panel (the menu's SIDS, Task 11) over the song's memory; the firmware's audio equal to the host's render through the same chain, sample for sample, for a VBI, a CIA, an NTSC and a 2SID tune, with LIVE showing and without; the song and a tune switched both ways 20 times, the song back from the store byte for byte; the keys on chip B under a 1SID tune, silent under a 2SID tune; PLAY HUNG; the interrupt only in SRAM; the .sid path's worst block within `PSID_ISR_WORST` over the corpus and a write every 4 cycles, each slice within `PSID_SLICE_WORST` and every pass within `POLL_INSTRUCTIONS` with LIVE showing, the RAM, RAMTEXT and image budgets) | `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate --test psid --test psid_budget -- --nocapture`; the interrupt's SRAM check with `--test ram_audio` | `crates/emu-gate/tests/psid.rs`, `psid_budget.rs` (bench/fm1/bench_6502_main.c mode 6), `common/mod.rs` (`render_host_psid`, `put_file`) |
| .sid files on the store (M3 Task 10: the browser's list, the header's refusals, the loading in parts equal to a whole load, the largest file too, within the store's share of a pass; SysEx `PUT` of type 2 checked at its first DATA before any claim or program, `NAK FULL`, the fuzz with type-2 headers; `sidou-sysex sid`, the CLI; in fm1-emu the CLI's `.syx` over TRS stored byte for byte and played, the refusals with no flash program, a 64 KiB tune, the keys used up) | `cargo test --release -p sidou-tests --test app -- sid_io sysex`; `cargo test --release -p sidou-sysex`; `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate --test psid_store -- --nocapture` | `crates/sidou-tests/tests/app/sid_io.rs`, `tests/app/sysex.rs`, `crates/sidou-sysex/tests/sid.rs`, `crates/emu-gate/tests/psid_store.rs`, [the section below](#sid-files-on-the-store-m3-task-10) |
| The SID views (M3 Task 11, `app/sid_view.h`: the SIDS browser, the unsaved-song question each way, NOW PLAYING, PLAY and EDIT + PLAY with a tune, GLO's mutes to the tune, the song views' SID PLAYING, STATUS's 6502 rows, SID MODEL and its settings schema 7, MIDI start, stop and continue to a tune; host LCD goldens `sids-*`, `playing-*`, `song-sid-playing`, `status-tune`, written with `SIDOU_LCD_GOLDEN_UPDATE=1`; in fm1-emu the menu's SIDS, a tune, GLO's mute of voice 2 sample for sample against the host's render muted at the same block, solo, a song step, pause, resume and stop, the question each way, MIDI over TRS, every ui_poll within `SIDS_WORST_POLL`, every pass within `POLL_INSTRUCTIONS`, every frame within 200 cells) | `cargo test --release -p sidou-tests --test app -- sid_view help_view settings midi_sync`; `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate --test psid_views -- --nocapture` | `crates/sidou-tests/tests/app/sid_view.rs`, `crates/emu-gate/tests/psid_views.rs`, `common/mod.rs` (`ui_play_tune`, `ui_end_tune`) |
| The starter pack (M3 Task 13, `assets/starter`: the signed-off eleven tunes and nothing else; each with its `.md`, a licence on the allowlist and the file's SHA-256; each parses, is not RSID and the device accepts it; every song of every tune 10 s in the host render on its chip with no overrun, no late write, no clipping; the files in step with their `.sng` sources; `sidou-sysex sid --pack` on the pack equal to the tunes' uploads end to end; in fm1-emu the pack's one `.syx` over TRS a second a message into the store, byte for byte, with its bytes and keys printed, and the first and eighth tune of the browser playing) | `SIDOU_STARTER_UPDATE=1 cargo test --release -p sidou-tests --test starter` writes the files; `cargo test --release -p sidou-tests --test starter`; `cargo test --release -p sidou-sysex`; `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate --test psid_store starter -- --nocapture` (about a minute) | `crates/sidou-tests/tests/starter.rs`, `src/starter.rs` (the list and the generator), `crates/sidou-sysex/tests/sid.rs`, `crates/emu-gate/tests/psid_store.rs`, [LICENSING.md](LICENSING.md#the-starter-pack) |
| SIDOU's 6502 (Tom Harte's tests, the committed subset; Dormann's; slicing, the count's wrap, the stops, the I/O hooks; on fm1-emu: Dormann, the cost per cycle: `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate --test cpu6502_budget -- --nocapture`) | `cargo test --release -p sidou-tests --test cpu6502`; the full Harte set with `SIDOU_HARTE_DIR=<6502/v1 JSON dir> ... -- --ignored harte_full_set`; the subset's converter `cargo run --release -p sidou-tests --bin harte-subset` | `crates/sidou-tests/tests/cpu6502.rs`, `src/harte.rs`, [the section below](#sidous-6502-m3) |
| Song player against GoatTracker's (every frame's SID writes equal) | `cargo test --release -p sidou-tests --test gt2_match`; the long runs (every example for 30,000 frames, 5,000 generated songs) with `-- --ignored` | `crates/sidou-tests/tests/gt2_match.rs`, `crates/gt2-ref/src/gen.rs`, `src/player.rs` |
| Golden renders | `cargo test --release -p sidou-tests --test golden` | `crates/sidou-tests/tests/golden.rs`, `tests/golden/hashes.txt` |
| reSIDfp reference | `cargo test --release -p sidou-tests --test reference` | `crates/sidou-tests/tests/reference.rs` |
| Filter against reSIDfp (transfer functions, sweeps) | `cargo test --release -p sidou-tests --test filter` | `crates/sidou-tests/tests/filter.rs`, `src/filter_sweeps.rs` |
| Register reads (OSC3, ENV3) | `cargo test --release -p sidou-tests --test register_reads` | `crates/sidou-tests/tests/register_reads.rs` |
| Chip profiles against reSIDfp (each profile's filter and combined waveforms at its settings) | `cargo test --release -p sidou-tests --test profiles` | `crates/sidou-tests/tests/profiles.rs` |
| Expert parameter transport (macro releases, ordered restore, learned maximum and no echo) | `cargo test --locked --release -p sidou-tests --test expert_transport` | `crates/sidou-tests/tests/expert_transport.rs` |
| Generated tables (drift) | `cargo test --release -p sidou-tablegen`; the filter tables with `--test drift -- --ignored` (measures reSIDfp's filter: about 20 s on every core) | `crates/sidou-tablegen/`, `core/sid/gen/` |
| Filter comparison | `cargo run --release -p sidou-tests --bin sidou-compare -- <dir>` | `crates/sidou-tests/src/bin/sidou_compare.rs` |
| Host benchmark | `cargo bench -p sidou-bench` | `crates/sidou-bench/benches/workload.rs` |
| Packager | `cargo test --release -p sidou-pack` (the round trip and golden tests need `build/ac79/`, which `sidou-pack` fills; the stock-return tests need M-VAVE's V15 package in `build/stock/FM-1.fwsc`; they skip without them) | `crates/sidou-pack/` |
| Emulator tests (gates incl. effects, boot, live instrument, screen, MIDI, latency, SRAM audio, package, flash budget) | `SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo nextest run --release -p emu-gate` | `crates/emu-gate/tests/` |
| Flash timing spike (a test firmware, M2a Task 3b) | build it with `-DSIDOU_FLASH_SPIKE=ON` into `build/fm1-spike` (BUILDING.md), then `SIDOU_FM1_BUILD=$PWD/../build/fm1 SIDOU_FM1_SPIKE_BUILD=$PWD/../build/fm1-spike cargo test --release -p emu-gate --test flash_spike -- --nocapture`; its bounds `cargo test --release -p sidou-pack --test flash_spike_guard` | `crates/emu-gate/tests/flash_spike.rs`, `crates/sidou-pack/tests/flash_spike_guard.rs` |
| Ergonomics harness (readability, knob feel, effort per task, feedback speed, consistency, journeys; a report for review, not a gate) | `cargo run --release -p ergo -- --out ../build/ergo` (needs the fm1 build); its unit tests run with the workspace | `crates/ergo/`, [the section below](#the-ergonomics-harness-ergo) |
| Browser demo smoke test (from the repository root) | `tools/web-demo --check` | `web/check.mjs`, `web/check.sh` |
| Songs page's SysEx code, in Node (no browser, no MIDI) | `node web/sysex.test.mjs` (`tools/web-demo --check` runs it first) | `web/sysex.test.mjs`, `web/sysex.js`, `web/sysex-vectors.txt` |

Use `--release` for the tests. The C code is optimised either way (the test
profile sets `opt-level = 3`), but the reference tests do a lot of Rust-side
signal analysis.

## Emulator first, and the first-use tests

**The rule.** Whatever the emulator can check, it checks before a unit is touched: a device
session starts only when the emulator-marked checks of [docs/hardware-test.md](docs/hardware-test.md)
pass, and it asks the user only for the device-only ones (the speaker and the room, the feel,
the real LCD and LEDs as seen, the printed controls and their timing, real USB hosts and
operating systems, the real flash chip, SRAM and power cycles, install and recovery). Each check
there says which it is: *Emulator* with the test that covers it, *Device-only* with why, or
*Emulator (no test yet)*, a test still to write before the next session. A bug found on the device
that the emulator could have seen gets an emulator test, not another device check.

**The first-use tests** (`crates/emu-gate/tests/first_use.rs`) are that rule for a new user's
first hour. v0.4.0's device session found three bugs a scripted run would have caught (a NEW song
silent since v0.2.0, whose test checked a peak level that a click passes; NEW's PULSE and BASS
alike; the first STEP press after PRESETS sounding the previous instrument). Each test drives the
panel as a user does (the menu's SONG pages, STEP, TRACKER, PLAY, GLO, the generative gestures, a
power cycle with the flash kept) and judges the output as a listener would, not sample for
sample (the bit-exact tests are `song.rs`'s and `song_store.rs`'s):
- **heard**: the loudest 10 ms RMS over a floor (`AUDIBLE_RMS`), and silence as every 10 ms
  under `SILENT_RMS`; never a peak;
- **at its pitch**: the chip's frequency register where the note settles and the audio's own
  fundamental (past the onset's thump), within 25 cents;
- **on time**: onsets (a rise in the 10 ms RMS) within `ON_TIME_MS` of their row's time, rows
  480 ms apart at a NEW song's tempo;
- **told apart**: two instruments differ in loudness, length, spectral centroid, zero crossings,
  pitch, the waveforms they go through or their pulse width (`tell_apart`; the envelope's AD and
  SR alone do not count: an ear hardly hears them on a short note).

They cover NEW's fourteen instruments auditioned on STEP and played with PLAY, GLO's mutes,
TRACKER's REC note entry and SEQ through every song view, SONG's LOAD of SIDOU LOOP, SAVE AS and a
power cycle, and STEP's chance, DICE and MUTATE. A test written for a bug fixed on another branch
is `#[ignore = "needs branch ..."]` until that branch merges; one for a bug found and not fixed yet
is `#[ignore = "bug: ..."]`. Run them alone with
`SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo nextest run --release -p emu-gate --test first_use` (about
a minute on every core; `--run-ignored all` adds the ignored ones).

## Test layers

### Unit tests

One module per part of the core: oscillator (`sid_osc`), noise (`sid_noise`),
hard sync (`sid_sync`), envelope (`sid_env`), voice DACs (`sid_dac`, against
reSIDfp's tables), filter (`sid_filter`), the chip (`sid`) and constants
(`sid_defs`). Most pin a documented SID behaviour, such
as the ADSR delay bug, the noise LFSR taps or hard-sync timing. `sid` also
measures the output's worst case (three voices at maximum resonance: the 8580
clips by about 1 dB, as reSIDfp does), and `sid_dac` pins the default tables'
CRC-32s, which the hardware checklist compares with the device's.

The envelope tests compare the batched envelope (`sid_env_advance`: rests, linear
runs, whole rate periods) with a per-cycle Rust port of reSIDfp's
`EnvelopeGenerator`, over random writes at random times. They also cover the gate reset
(`sid_set_gate_reset`), the one option where SIDOU departs from the chip on purpose:
- the fuzz runs again with it on, the reference's LFSR restarting at each gate edge, and
  counts the falling edges it meets in a pipeline, past divider period 1 and frozen at 0;
- a release-6-then-attack-0 retrigger at each of the release period's 267 phases starts
  within 12 cycles with it on (ENV3 within 13); with it off, 8 phases start at once and
  the rest wait over 32000 cycles, as on the chip;
- a note-off at each phase of the decay's period, for SAW LEAD, SYNC LEAD and PWM BASS,
  starts the release within one release period and 3 cycles with it on (SAW LEAD's ENV3
  within 271); with it off, one release period of the phases starts at once and the rest
  wait for the LFSR to come round, as on the chip;
- below sustain 0x5d the release's first step still waits for the exponential divider,
  within its remaining periods, and a counter frozen at 0 stays frozen, the same with it
  on and off.

**A failure means** the core no longer behaves like the chip in that
respect. Fix the code, unless the test itself pinned the wrong behaviour; in
that case, say in the commit which source (reSID, a measurement, a datasheet)
shows the new expectation is right.

The effects have their own modules: `fx` (the Task 1 chain and its blocks)
and `slots`, `slots_dist`, `slots_mod` and `slots_fx` (the effect slots and
their algorithms). Each algorithm is tested for:
- its characteristic: THD, level and aliasing for the drives; LFO rate,
  depth and phase between sides for the modulations; echo time and
  feedback decay for the delays; RT60, damping and stability for the
  reverbs;
- exact bypass, and stereo where it makes stereo (L/R correlation, equal
  power);
- clicks, two ways:
  - a parameter jump against the same move ramped over 0.2 s: the 2nd
    difference of the two runs' difference must stay within 30 LSB (a 100
    LSB step must show);
  - an algorithm switch, whose 2nd difference must stay within 30 LSB of the
    steady state's.

The drives, delays and reverbs run their click tests at low input levels.
At full level their outputs saturate, or carry broadband tails, that differ
between two runs by more than any click. The tests say where.

### Store, settings and SAVE (M2a)

Run the host coverage with `cargo test --locked --release -p sidou-tests --test app store::`;
repeat with `settings::`, `save_key::`, `user_profile::`, `learn_view::` and `midi_learn::`.
After an fm1 build, run the device tests from `host/`:

```sh
SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo nextest run --locked --release -p emu-gate \
    -E 'binary(store) | binary(settings) | binary(user_profile)'
```

`tests/app/store_layout.rs` checks the store's range against the loader, OTA area, app slot
and package VM descriptor, including the erased 256-byte guard band in every sector.
`tests/app/store.rs` exercises the portable store (`app/store.c`) against a Rust NOR model:
programs AND bits, wrap within a 256-byte page, and can be deferred with `STORE_LATER`;
erases and programs can tear, leave faint bits that read differently on later boots, or fail.
This is stronger than the emulator's instantaneous, non-tearing flash model.

- Values and deletions survive remounts, collection and cuts; recovery copies the newest
  record forward and evacuates ambiguous sectors before the session writes. Regression
  cases cover torn claims, erase headers, kills, copy commits and tombstones, repeated short
  boots and near-capacity use. Guard bands remain erased, and an interrupted erase is redone
  before use. Foreign data and newer formats are never silently formatted.
- The erase-ahead pool reserves at least 20 sectors and refills toward 22; short-pool and
  full-store refusals write nothing. Wear counts spread by at most 8 after 10,000 erases.
  Public API calls and mount/poll slices stay within their work allowance.
- Files of up to 65,536 bytes claim all their sectors before writing (18 for the maximum),
  stream with back-pressure and commit whole. Replacement cuts leave the old or new file,
  never neither or mixed data. Tests cover directory order, rename, delete, abort, a sender's
  5 s timeout, every read offset/length, and write offers of 1..1,000 bytes.
- The checked-in format-1 fixture still mounts to the same values and files. Random bytes
  and damaged regions mount safely. The ordinary app tests include bounded power-loss runs;
  the longer runs below are ignored by default.

Run the long value and file sweeps explicitly, from `host/` (they print cut counts):

```sh
cargo test --locked --release -p sidou-tests --test app store::power_loss_long -- --ignored --nocapture
cargo test --locked --release -p sidou-tests --test app store::files_power_loss_long -- --ignored --nocapture
```

The value run uses 400 forked-cut seeds (1,000..1,400) and 3,000 continuing-cut seeds
(10,000..13,000), windows of 1..256 bytes and faint bits for three seeds in four. The file
run uses 60 forked-cut and 300 continuing-cut seeds, the latter with files up to 64 KiB.
For one reproducible seed, `SIDOU_STORE_SWEEP=values:1000` (or `files:10000`) with
`cargo test --locked --release -p sidou-tests --test app store::power_loss_sweep -- --ignored --nocapture`
runs that seed only; `values-forked:N` and `files-forked:N` force the forked variant.

`tests/app/settings.rs` checks every persisted field and default, schema-1 through schema-5
fixtures (schema 1 defaults TRACKER ROWS to BAR; versions before 5 follow profile/macro
expert defaults), invalid records/fields, the versioned full
boot, the 5 s debounce after the last change, explicit commit notification, busy retries,
the 2 s forced-save path and foreign-store refusal. Volume and transient controllers are
not restored. Setters and learn edits wake background saves; an unchanged frame does not
gather the record.

`tests/app/save_key.rs`, `learn_view.rs` and `midi_learn.rs` check clean-release SAVE,
cancellation by an encoder or another button, routing through the views, SAVE-held + knob
learning without changing the parameter, forgetting one binding, SAVE-held + HOME and a
fresh SAVE confirmation to clear all, HOME/timeout cancellation and STORE's two-turn format
confirmation. `tests/app/user_profile.rs` checks the separate 22-byte schema-2 records,
the seven-byte schema-1 migration, raw expert overrides and following defaults, the eight-slot
picker and empty-slot skipping, target changes without chip writes, overwrite confirmation,
both keys and song-chip setups, profile preparation/ramping, boot load, retries and forced
gaps, foreign-store refusal and clearing the slots on format.

In fm1-emu, `settings.rs` changes CARDS, envelope mode and a learned TRS CC, then reboots
with the same flash and checks all three. It checks the full boot once per firmware version,
an explicit SAVE within a second without the debounce, and menu-confirmed foreign formatting.
`user_profile.rs` saves USER 3 from CHIP, reboots, picks it over CC 85 and compares the sound
bit for bit with the host using the saved setup; a separate panel walk checks the picker
skips empty slots. These complement the store's device-level cuts and cost gates below.

### Two chips and the expert layer (M2b)

Run focused host checks from `host/`:

```sh
cargo test --locked --release -p sidou-tests --test unit sid_character::
cargo test --locked --release -p sidou-tests --test app reg_ring::
cargo test --locked --release -p sidou-tests --test app keys_route::
cargo test --locked --release -p sidou-tests --test app chip_view::
cargo test --locked --release -p sidou-tests --test expert_transport
```

The character tests cover all fourteen experts: their physical ranges and defaults,
monotonic filter coordinates, six cutoff endpoints and all 23 curve/range anchors against
the pinned reSIDfp measurement at FC 1024/resonance 8, independent DAC controls, combined
strength's oscillator-mode refresh, noise feedback, and the existing 30-LSB click-residual
bound. The sparse fitted cutoff grid is not a claim of full-response equivalence between
anchors; damping stays the profile's. Following defaults retain exact physical values,
rather than a seven-bit approximation. Held oscillator/envelope state and gates survive
expert changes and the return to profile/macro defaults.

The register-ring tests exercise chip-tagged writes, independent A/B state, the shared
spare and DAC build table, the keys' priority and bounded yielding, and expert filter-row
leases. Producer/interrupt race tests keep rendered chips, queued stages and active jobs
out of a lease; catch-up ticks cannot perform two table builds in one pass. The routing
tests cover keys on A without a song, B during a song, held-key moves and release tails,
and KEYS A's temporary borrowing of song voices.

The CHIP tests cover both tabs, the final EXPERT stop, all four pages' K1–K4 assignments,
inactive model-specific fields, exact unit readouts, named/coloured popups, MIDI learn,
and USER saves from either chip. Clamped or empty profile-picker steps preserve edits;
new built-in profile/model choices release them. Settings v1–4 and USER v1 fixtures
load with following experts; settings v4 retains M2c's last-song filename. Settings v5
and USER v2 round trips retain explicit overrides and learned CCs.
CHIP and the song views borrow the same drawing canvas: regressions interrupt a TRACKER
build, return with HOME without re-entering CHIP, compose under MIDI LEARN, and restore
SONG/TRACKER after a different owner. The view state remains separate; ownership changes
invalidate rendering caches rather than clearing or copying a canvas in an input handler.
`chip_labels_and_legends_use_t1_roles_without_changing_tabs` checks the readable 1×
control/legend roles in both themes, large-name/bar accents, inactive controls and all
expert pages, while preserving the existing tab rendering for the separate T2 task.

The song editors' portable tests are `song_pages`, `song_pattern_at`, `song_order_locks` and
`song_audition_shape` in the app suite. They cover the pages' edits and refusals against the
model, missing-pattern creation, selected versus played subtunes, popups and shared-canvas
transfers; `song_audition_shape` checks an unused instrument, an omitted voice and a note outside
the packed range against independently prepared player state, that a stopped audition never
plays the song's own voices, and that shape changes keep the running registers and PCM.

The emulator's `all_song_edit_pages_keep_the_existing_input_frame_and_spi_budgets` reuses the
unchanged TRACKER input, pass, poll, 200-cell and 20 ms SPI bounds for ORDER, INSTR and TABLE;
their idle frames send nothing. `an_instrument_page_edit_changes_the_next_notes_bit_exact_with_the_host`
edits AD on INSTR during playback and compares every sample with the host's same-block edit; an
unedited control must differ, so the check is not vacuous.

STEP's portable tests are `step_view` in the app suite: the layout, toggles (staccato's key
offs, legato, the last row), paging and the playing page, the lanes, PRESETS, LOCK and SET
shown apart with the value popups, the refusal of a SET on a row with a command, the queue that
keeps taps and knob turns in order while the editor is busy, and two player checks: a STEP-made
song against GoatTracker's own player call for call (the SET holds over the following rows; no
chunk locks in the file), and a lock made on a held step holding for its row only in song
control's output. In the emulator, `the_step_view_matches_its_goldens` and
`the_step_view_stays_within_its_budgets` (with the 30 s runs at both tempos) cover the frames, and
`a_pattern_programmed_on_step_is_the_expected_song_and_plays_bit_exact` and
`a_held_step_and_k1_darkens_that_step_bit_exact_with_the_host` (song.rs) run the keys against the
model's own edits and the host's render, the darkening measured as the song lock test does.

REC's portable tests are `song_rec` in the app suite: the quantising at each fraction of a row
(NEAREST, OFF, and halfway rounding up), a note at the row the voice's own player is on rather
than the cursor's, a key past the pattern's end going to the next entry or round, a recorded
note replacing the note and keeping the row's command, the four-deep queue and `WAITING`, `SONG
FULL` and `SAVING` refusals, K1 and K2 as locks (32 and 1 a detent, clamped, from LIVE's values,
nothing recorded while stopped), the one-bar count-in, the menu's REC item, and STEP's pads (the
staccato key off, a legato lane, the keys above the steps). In the emulator,
`rec_records_a_timed_key_script_on_the_expected_rows_and_plays_back_bit_exact` (song.rs) presses
four keys at chosen calls of SIDOU LOOP's tempo-6 rows and two K1 turns, then checks the pattern
and the lock table the firmware holds against the expected song, and the song's playback
bit-exact with the host's. REC's recording has no timing bound of its own; it rides on the
editor's cell writes, whose costs `tracker_edits_from_the_panel_are_readable_and_bounded` bounds.

`expert_transport` replays outgoing packets into a second instrument. It checks macro
releases after deferred or canceled changes, macro-before-expert restoration, remote
edits after a queued local macro, explicit learned 127 versus the internal following
sentinel, and input that must not echo or freeze inherited values.

`song_snapshot` compiles the device's metadata packing/unpacking helpers and checks
all 128 raw settings, including cutoff/pulse-width high bytes and expert-following 255.
The snapshot fits the unchanged 160-byte I/O buffer; MASTER remains excluded from
song restoration. The extracted song callback regression in `keys_route` covers expert
base/runtime separation under locks and save-time refusal without changing either setup.

After both firmware builds, run the complete emulator gate (not just these focused checks):

```sh
SIDOU_FM1_BUILD=$PWD/../build/fm1 SIDOU_FM1_SPIKE_BUILD=$PWD/../build/fm1-spike \
    cargo nextest run --locked --release -p emu-gate --no-fail-fast
```

That everyday run leaves out the ten-minute clock-drift test
(`a_song_follows_a_steady_clock_for_ten_minutes_without_drifting`, about as long as every other
test together). **Before every release tag**, run the whole gate with `--profile release`,
which includes it (`host/.config/nextest.toml`).

[cargo-nextest](https://nexte.st) (`cargo install --locked cargo-nextest`) runs each test in
its own process, one per core. fm1-emu counts instructions, so no number depends on the
schedule: every JSON report the gate writes (`build/fm1/*.json`, `build/fm1-spike/*.json`) was
byte for byte the same in three parallel runs as in the serial one (2026-10-22). Each test
writes only files named for itself, and nextest shows a failing test's output (`--no-capture`
would run them one at a time again). The wall time is that of the longest test,
`the_players_worst_block_fits_its_budget` (about 13 minutes), so `host/.config/nextest.toml`
starts the long tests first. Without nextest, `cargo test --locked --release -p emu-gate`
(no `--test-threads=1`) runs the binaries one after another with their tests side by side.
No emu-gate test needs to run alone.

The gate steps fm1-core one instruction at a time, since its counts and watches are per
instruction; fm1-core's JIT (`Cpu::jit_on`, AArch64 macOS and Linux, x86-64 Linux and Windows)
runs only through `run_fast`'s blocks, so the measuring loop cannot use it. It does make the
settled flash every `Machine::boot` starts from (`Machine::run_ms_jit`: the first boot's 7 s on
a blank chip, kept for its flash alone), in 0.6 s instead of 5.1 s, once per test process.
`the_jit_settles_the_flash_as_the_stepped_run_does` (boot.rs) checks that flash against the
stepped run's. Every report and printed count was the same with it as without (2026-10-22),
and the tests' times added up to 5,048 s instead of 6,129 s; the wall time stays the player
budget test's.

- `chip_b.rs`: B is never rendered before preparation, costs no extra render while unused,
  fades in/out bit-exactly with the host, and fits the real two-chip ISR budget with every
  slot at its heaviest. It measures the mix and MASTER gain separately.
- `character.rs`: expert NRPNs affect the chip/audio without resets or lost gates; filter
  builds are sliced, stages commit at block boundaries, and all six combined strengths
  have exactly their required shared table blocks. Existing character renders stay
  bit-exact with the host.
- `screen.rs`: both chip tabs and `chip-expert-1.png` through `chip-expert-4.png`, unchanged
  CHIP input/frame limits, and expert idle <7,000 instructions/frame, a detent <8,500,
  worst `ui_poll` <38,000, ≤200 cells/frame and ≤20 ms SPI. STATUS idle stays <7,500,
  HOME input <430 and CHIP K1 <215.
- `user_profile.rs` and `settings.rs`: expert edits survive a save/reboot/recall, the
  saved profile's sound matches the host, and genuine old-schema records still load.
- `midi.rs`: existing mapped/learned TRS floods, USB drain and MIDI-output tick limits,
  including a cold expert snapshot with every parameter learned. `latency.rs` retains
  normal note-on/off bounds while a profile is prepared and character controls sweep.
- `screen.rs` also checks UI sounds off/on and three held notes on B: no stolen voice and
  bit-exact notes when a cursor sound cannot borrow one.

Keep the full checklist above the focused runs: formatting, clippy, release workspace,
UBSan, GCC 13, production/spike builds, packaging, REUSE and `tools/web-demo --check`.
Run long checks detached with separate logs and exit-code files. A missing build variable
can skip an emulator check; a skipped run is not device validation. Hardware-only checks
remain in [the hardware checklist](docs/hardware-test.md#63-two-chips-routing-and-ui-sounds-m2b).

### Song files

`tests/song.rs` covers `core/song` (decision record 0007, `docs/reference/song-format.md`):
- **round trips:** random canonical GTS5 songs (mono and GoatTracker Stereo, generated in
  Rust by `src/song.rs`) read and write back byte for byte; SIDOU's own output, with and
  without its trailer, reads and writes back the same; what `savesong` trims (trailing
  name-only instruments, table rows, unnamed empty patterns, a trailing subtune with an
  empty channel, rows past a pattern's end) and the older idents' conversions (GTS2's
  speedtable, GTS3's pulse speeds, GTS4's legato) are pinned with hand-made files;
- **the trailer:** its layout and CRCs, every chunk model, an unknown chunk kept in place,
  a flipped GT2 byte dropping `LOCK` only, a flipped trailer byte dropping the trailer, and
  every extension size up to 4,096 bytes with no `$FF` in the length;
- **stereo detection:** mono songs of every size from the smallest up (with three end-row
  shapes) and random mono and stereo songs, with trailers, against SIDOU's reader and an
  independent Rust transcription of GoatTracker Stereo's `determinechannels`; a mono file
  whose instrument bytes pass that trial reads as mono; SIDOU's own output without a trailer
  reads back (2,000 songs, many of them look-alikes; 200,000 with `-- --ignored`), unless it
  is also the canonical save of a song with the other channel count, which the test counts
  and `gt2_sng_check_plain` warns of after a plain export; with a trailer, always, those
  ambiguous files included (the trailer records the count);
- **refusals and caps:** each message, the song unchanged after it; a song over the
  device's arena (`SNG: TOO BIG`), multispeed 0 and 8 on the device's caps; device songs
  also enforce the 65,536-byte canonical saved-size cap, including the trailer
  (`song_capacity.rs`): accepted edits remain saveable, refused edits leave the model
  unchanged, and queued edits/cancellation keep the cached size exact;
- **edits:** each primitive's refusals (readme 3.1's orderlist rules, 128 rows, 208
  patterns, 254 entries, the spare); random edit sequences with compaction, compared after
  every step with an independent Rust model, the bitmap checked against the index and the
  spare kept, and every rebuild checked to leave the bytes in use unchanged before its
  swap; compaction from a review's counterexample (full-size patterns each followed by a
  2-unit hole) and from random fragmentation always ends in one hole within 2 x blocks
  steps; **deferred edits:** edits queued, applied later in order as the audio interrupt
  will (M2c Task 6), released after, with the song checked at each application against the
  model as it stood when that edit was made, the last queued edit sometimes cancelled, half
  the songs filled to the cap, the arena's size at rest checked against the model after
  every step (so `GT2_ERR_FULL` is judged with no pending edit counted: a review's repro, a
  waiting rebuild then a shrinking one, is its own test), a reload and a clear refused while
  edits wait, and the deferred contract's refusals (`GT2_ERR_PENDING`) one by one;
- **fuzz:** random bytes and mutated songs of every ident never crash the reader; whatever
  it accepts writes and reads back. `-- --ignored` runs 4,000 fuzz seeds, 2,000 edit
  seeds of 2,000 steps, 2,000 deferred-edit seeds and 3,000 compaction seeds.

### Song player reference

`crates/gt2-ref` is the reference SIDOU's song player is checked against (decision record
0007, "Equivalence, defined" and "The reference harness"). It is test-only.
- **The packer:** GoatTracker 2.77's own song model and relocator
  (`third_party/goattracker2`, unmodified), built by `build.rs` with SIDOU's shim (BME's
  calls, files in memory, and SIDOU's assembler for `player.s`, since GoatTracker's ships
  under a non-commercial licence: `third_party/goattracker2/VENDORED.md`).
  `gt2_ref::pack(sng, options)` is `gt2reloc song.sng out.prg` with those options;
  `gt2_ref::canonical(sng)` is GoatTracker's `loadsong` then `savesong`.
- **The 6502:** the `mos6502` crate's NMOS core behind the `Cpu6502` trait
  (`src/cpu.rs`), so M3's own 6502 can run the same traces. `tests/cpu.rs` runs Klaus
  Dormann's functional test on it (`third_party/6502-tests`), decimal mode included.
- **The trace** (`src/trace.rs`): init once with A the subtune, then play once a frame
  (`max(multiplier, 1)` times with multispeed), each call returning to a sentinel; each
  call's writes to the SID's 25 registers in order. A traced frame is 20 ms; with the
  multiplier 0 (25 Hz) it is one call, 40 ms. With zero-page ghost registers the harness
  runs readme 5.1's copy after each call: `$18` down to 0, or for the alternative player
  (`-A` of `$F000` and up) each channel's wave, frequency, pulse width and ADSR last, the
  filter first. `gt2_ref::pack` and `canonical` fail on a song `loadsong` cannot load (an
  unknown ident, or a short file), and the trace refuses code placed over the SID's window
  (`$D400-$D7FF`, and its registers at `-L`), the return slot `$01FE-$01FF`, the sentinel
  `$FFF0`, or past `$FFFF` (`TraceError::Placement`).
- **The traps:** before each instruction the harness checks the opcode against the 151
  documented ones and fails the trace on any other, with its address and frame
  (`TraceError::Undocumented`); a call past 200,000 instructions is a hang, and an opcode
  the core cannot decode its own error (`TraceError::Undecodable`). SED can be
  trapped too (`Setup::trap_sed`), but Dormann's test proves the core's decimal mode, so it
  is off. M3's own 6502 needs the undocumented opcodes for `.sid` files and must be
  checked with Tom Harte's per-opcode tests for them; this harness only proves that
  GoatTracker's player never runs one.
- `tests/harness.rs`: every example song's every subtune traces 3,000 frames without a
  trap; traces are stable across runs; init and play sit where gt2reloc reports
  (`$1000` and `$1003`, or with `-W` elsewhere), each a JMP; each option changes the
  packed player as it should (multispeed, pulse and realtime optimisation, buffered writes,
  the hard restart ADSR, A-4's pitch), and NTSC does not (it only changes a `.sid`'s
  header); multispeed calls play N times a frame, every call writes, and the trace is not
  1x's; a hand-made player with an undocumented opcode, SED or an endless loop fails with
  its address; the option matrix (default, `-AF000`, `-B1`, `-J1`, `-D1 -E1`,
  `-I0 -O0 -R0`, `-S2`, `-S4`) traces every example 300 frames, pinned by a digest per
  set; the alternative player's ghost-register order; the load and placement refusals.
  `tests/cli.rs`: `gt2-trace` takes good command lines and refuses bad ones.
- `tests/assembler.rs` (`--ignored`): downloads GoatTracker 2.77 into
  `build/goattracker2-2.77/` (SHA-256 checked; curl with a generic User-Agent, unzip, a C
  compiler), checks the vendored files are the release's byte for byte, and packs every
  example with 30 option sets through both GoatTracker's assembler and SIDOU's: the same
  bytes, or refused by both.
- `crates/sidou-tests/tests/song.rs` uses it for the format: every example,
  canonicalised by GoatTracker, reads and writes back identically in SIDOU (all 14 are
  canonical as shipped, and the test prints how many are), and SIDOU's save equals
  GoatTracker's for random songs of every ident, canonical or not (300 seeds; 20,000 with
  `-- --ignored`).

`gt2-trace song.sng [--subtune N] [--frames N] [gt2reloc options]` prints init's writes and
then one line a frame, `register=value` in hex, for comparing a player against the
reference by hand. Its flags are strict: a value out of range, a value on `-N` or `-P`, an
unknown flag or a second song is an error.

### SIDOU's 6502 (M3)

`tests/cpu6502.rs` checks `core/cpu6502` (decision record 0008) in three builds of the same
file (`sidou-sys`'s `build.rs`): the device's (`CPU6502_SMALL`), and the test-only
`CPU6502_HOOK_ALL` as the device builds it and with the I/O accesses inline. With
`HOOK_ALL` every operand access (the zero page's, the I/O page's, the others) and every push
goes through the hooks with the cycle stamp the device gives an I/O access.
- **Tom Harte's SingleStepTests** (`third_party/6502-tests/harte/`: the first 100 tests of
  each opcode, `VENDORED.md`): the final registers (P with bits 4 and 5 masked) and memory
  for all 256 opcodes; the cycle count for the 244 that do not JAM; the 12 JAMs stop with
  `CPU6502_JAM` and the PC on the opcode (Harte's final PC is the byte after it). Each
  hooked access must be the test's bus cycle at its stamp (a load's on its last cycle, a
  read-modify-write's read on its third last and its writes, old then new, on its last
  two, JSR's and BRK's pushes where the real CPU makes them), and with `HOOK_ALL` the
  writes must be all of the test's writes. Half the tests start 4 cycles before the
  32-bit count wraps. The device's build runs with and without its I/O page on ($01 at
  `$37` unless the test sets it), so its own out-of-line accesses are checked where a test
  touches `$D000-$DFFF`.
- **The full set** (`--ignored harte_full_set`, `SIDOU_HARTE_DIR` naming a directory of
  the 256 JSON files, about 80 s): the same over 2,560,000 tests. Measured 2026-10-23: the
  final state 2,560,000 of 2,560,000 on every build, the cycles 2,440,000 (every non-JAM
  test), every stamp of the 1,690,000 hooked tests.
- **The subset's converter** (`src/harte.rs`, `src/bin/harte_subset.rs`):
  `cargo run --release -p sidou-tests --bin harte-subset -- <json dir> <out dir> [count]`
  writes each opcode's first tests in the binary the test reads (`VENDORED.md` has the
  command for the committed files).
- **Klaus Dormann's functional test** (`third_party/6502-tests`, decimal mode included)
  reaches its success trap on every build, in the same cycle count.
- **Slicing:** Dormann's first 100,000 cycles run in slices of every length from 1 to 300
  cycles give the same registers, count, memory and hooked stream as one run; so does a run
  across the 32-bit count's wrap.
- **The stops:** a return to the sentinel (`$0000`/`$0001`) and a jump, return or RTI to the
  KERNAL's interrupt exits (`$EA31`, `$EA81`, `$FEBC`) end a call only when `stops` is set.
- **The I/O page:** the device's build hooks `$D000-$DFFF` only while `io` is set and `$01`
  maps I/O in, with the stamps above.

**On fm1-emu** (Task 2), `crates/emu-gate/tests/cpu6502_budget.rs` runs the device build
(`CPU6502_SMALL`, `-Os`) as pi32v2 code in the bench image `sid_bench_6502`
(`bench/fm1/bench_6502_main.c`; its modes and result words are in the file's header):
- **Dormann's functional test** reaches its success trap `$3469` (96,241,955 cycles). This is
  the decoder check: fm1-emu stops on an encoding it cannot decode, so a finished run means
  the compiler's dispatch and everything else in the core decodes at the pinned emulator.
- **A 10 s workload** from three GoatTracker examples (the three costliest per cycle of the 14;
  `every_example_song`, `--ignored`, measures all) packed by `gt2-ref`'s `pack` with its default
  options: init, then a play call every PAL frame (19,656 cycles, 501 of them), `$01` at `$35`,
  a write hook summing the writes. The writes' count and sum must equal the host 6502's trace,
  no play may run past its frame, and the instructions per 6502 cycle must stay within
  `CPU6502_INSTR_PER_CYCLE` (`common/mod.rs`: the worst song's average plus 5 %).
- **Each opcode alone**: a run that executes nothing (the call's own cost) and a run that
  executes one instruction, per opcode; printed by cycle count, and for `LDA`, `STA` and `INC`
  of `$D400` through the hooks.
Instructions are not device cycles: flash day measures the difference (`docs/hardware.md`,
"Open checks"). `flash_budget.rs` also holds M3's stage (`("M3", 534,000)`, `M3_START` plus
M3's 28,000), which `IMAGE_CEILING` takes over when Task 9 merges device code.

### The PSID oracle (M3)

`crates/psid-ref` (M3 Task 7) runs a tune on SIDOU's PSID environment (`core/psid/psid_env.c` on
`core/cpu6502`, voice shadows of fresh chips answering the SID reads) and on an oracle built
from floooh/chips' `m6502.h` and FM-1 SID's `psid.c` (`third_party/chips`,
`third_party/fm1-sid-player`, both vendored unmodified, test-only), and compares them call by
call. The oracle is `c/psid_oracle.c`, a derived copy of `psid.c` whose only additions, between
`psid-ref: begin` and `end` markers, are its notice, a log of the I/O page's reads and a
memory read-back for `$01`; a test checks the vendored files' SHA-256s and that the copy
without those blocks is the original byte for byte.

- **Each side's calls** (`psid_ref::run`, `psid_ref::sidou::run`): init, then play calls, each
  with its start, its length, `$01` at its start, its SID writes (cycle in the call, register,
  value; SIDOU's with the chip and the address) and its I/O reads (cycle, address, value). The
  oracle is scheduled as `psidlog.c` schedules it (PAL, a frame boundary after init, then
  `psid_period`); SIDOU's side as the player will run it (its clock, the period rule).
- **The comparison** is by call index, so the two sides' different periods do not shift the
  calls. In each call, in time order: `$01` mapping the I/O page the same way, every read
  SIDOU made also made by the oracle at the same cycle and address with the same value
  (m6502.h's dummy reads, which SIDOU's core does not send to the I/O page, are not compared),
  the writes equal (cycle, register, value; the chip aside, since `psid.c` folds every SID into
  one), the lengths equal. SIDOU's chip is checked against each write's address separately. The
  first difference must be one of these, and ends that tune's comparison with the rule named;
  anything else fails the test:

  | # | Expected difference | SIDOU | `psid.c` |
  |---|---|---|---|
  | E1 | the bank register at a call's start | per call, from its address | `$37` once at load; a value that maps the I/O page the same way (`$36`/`$35`/`$37`: neither side has ROMs) shows only to a tune reading `$01`, so the comparison goes on and E1 explains a later difference no other rule does |
  | E2 | a SID read (OSC3, ENV3, the POTs, a write-only register) | the voice shadow | random OSC3, 0 for the rest |
  | E3 | a raster read (`$D011`, `$D012`) | the absolute cycle, PAL or NTSC geometry (one jump at the 32-bit wrap) | from the call's start, PAL |
  | E4 | a CIA 1 timer A read (`$DC04`, `$DC05`) | the counter | the latch |
  | E5 | an interrupt flag read (`$D019`, `$DC0D`, `$DD0D`) | the flags | 0 |
  | E6 | any other I/O read | the last write | 0 |
  | E7 | a call cut at `psid.c`'s cap (2 frames for play, 5 PAL seconds for init) | runs on to its return, or its own limit | stopped |
  | E9 | a third SID's registers | writes dropped (counted), reads 0 | folded into its one SID inside `$D400-$D7FF` (OSC3 random), dropped outside (the expansion areas read 0 on both) |
  | E10 | a JAM | the tune stops (`6502 JAM AT`) | m6502.h stays busy: runs to the cap |
  | E11 | an interrupt call (play address 0) | the IRQ's stack (and the KERNAL entry's through `$0314`: S `$F9`, X = S), P pushed `$20` | S `$FC`, X 0, P pushed as the last call left it |
  | E12 | a second SID outside `$D400-$D7FF` (`$DE00-$DFFF`) | chip B | writes dropped, reads 0 |

  **E8**, the timing between calls (CIA periods under 1,000, the speed bit for songs above 32,
  NTSC frames and the NTSC latch, late calls after an overrun), is checked separately:
  SIDOU's call times against the period rule (`compare::check_timing`, restated from the format
  and the C64), by assertion; the oracle's times are not used.
- **The corpus** (`tests/oracle.rs`): every GoatTracker 2 example and SIDOU's songs
  (`assets/songs/*.sng`), every subtune, packed by `gt2-ref` into PSID v2 at the default options
  and at `$D420` as a 2SID tune (v3, the second SID there), 60 s each in release (10 s in debug):
  44 songs and subtunes, 132,352 calls and 1,987,572 writes, all equal with no difference at
  all. Task 6's fixtures (`sidou_tests::psid_fixtures`), 3 s each, each with the rule and the
  call its first difference must be. Task 4's crafted headers are SIDOU's alone: `psid.c` reads
  past its buffer on them, so the oracle refuses any file SIDOU's parser refuses.
- **The user's tunes** (`--ignored the_users_tunes_against_the_oracle`, `SIDOU_PSID_DIR` naming a
  directory; `SIDOU_PSID_SECONDS`, 60 by default): every `.sid` under it, every subtune, the same
  comparison and timing check, a table of the tunes, the calls compared and what ended each in
  `build/psid-ref/report.txt` (never committed). Measured 2026-10-24 on the M3 study's 24 tunes:
  16 PSID tunes, 88 subtunes, every one equal for its 60 s; the 8 RSID tunes refused.
- **A planted bug is caught:** with `STA abs,X`'s write stamped a cycle early (a scratch change
  to `core/cpu6502/cpu6502.c`'s `W_ABX`), the corpus fails at `2xtest.sng`'s first play call
  (`$D404` at cycle 489 against the oracle's 490).

### .sid tunes on the device (M3 Task 9)

In fm1-emu, `crates/emu-gate/tests/psid.rs` puts a tune into the store's flash dump before the
boot (`common::put_file`, type `STORE_FILE_SID`) and plays it through the firmware's test hook
(`platform/fm1/main.c` `psid_test_hook`: play, stop, end for good; until Task 11's views). The
firmware's audio from power-on equals `common::render_host_psid`: the instrument and chip B as at
boot, chip A no longer rendered from the block the interrupt took the tune (`player_now`), each
of the tune's chips fresh from the block its reset reached the interrupt (`reg_ring_chips.active`),
and from psid_ctl's first block (`psid_ctl.blocks`) sidou_tests::psid's `Player` on those chips,
then the mix, tail gate, slots, thump and MASTER, sample for sample (the blocks read from the
buffer snapshots, the DMA's leading silence searched as `song.rs` does). The tunes: GoatTracker's
`dojo` (VBI, with LIVE's tap and under the menu), `funktest` with song 1 on CIA 1's timer,
`wavecmdtest` at the NTSC clock, `dojo` as 2SID at `$D420`; none may have a late write. The switch
test plays a 1SID and a 2SID tune by turns, 20 times, from LIVE and over a playing song, and each
time the song (saved and loaded as `NIGHT` first) comes back from the store byte for byte (its
file form) and plays again; the keys play chip B under the 1SID tune and are silent under the
2SID one. A tune whose play never returns fails with `PLAY HUNG` (the toast) and a key still
plays.

`psid_budget.rs` measures the audio interrupt's .sid path in the 6502 bench (mode 6, the device
build of psid_ctl_block), each instruction told by its function: the chips' own work (the render
loops and the voices' clocking) aside, the rest of each block (the ring, the render parts'
setup and steps, sid_advance's own code, sid_write and what a write runs) within
`PSID_ISR_WORST`, over the corpus 2 s each and over tunes that write a voice, the cutoff or the
volume every 4 cycles; then the firmware with a tune playing and LIVE showing (each
`psid_ctl_poll` within `PSID_SLICE_WORST`, every pass within `POLL_INSTRUCTIONS`, every audio block
within `AUDIO_TARGET`); then the memory (the tune's state inside the song's union, free RAM,
M3's RAMTEXT, the image within M3's ceiling).

### .sid files on the store (M3 Task 10)

`crates/sidou-tests/tests/app/sid_io.rs` (app/sid_io.c over the simulated store): the browser's
list holds only `.sid` files, in name order, never SysEx's staging; RSID, MUS, broken and too
long headers are refused with their toasts and touch nothing; a tune loaded in parts (the
64 KiB cleared, a pass at a time, then the data read straight into it) equals `psid_ctl_load` of
the whole file in its memory and its writes, for a tune with a load-address prefix and for the
largest file the store holds (`sidou_tests::psid::full_size`, 65,536 bytes); a song change loads
it again; a file replaced or deleted under a loading stops it. `tests/app/sysex.rs`: a `.sid`'s
PUT acknowledged before its claim and claimed at its first DATA (USB and TRS); RSID, MUS,
crafted, too long and non-PSID files refused at that DATA with no claim, owner or program, the
TRS tail dropped; the keys used up (`NAK FULL`); a claim that waits for a record programs the
held DATA; the fuzz of mutated headers and frames over both ports (a file stored only if its
header checks and its bytes came whole; a refused header programs nothing).
`cargo test --locked --release -p sidou-sysex`: `sidou-sysex sid`'s checks (the device's parser),
its store names, its frames, and the CLI on files in a scratch directory.

In fm1-emu, `crates/emu-gate/tests/psid_store.rs` (`SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo
test --release -p emu-gate --test psid_store -- --nocapture`): the CLI's `.syx` sent over the
TRS jack a frame a second stores the tune byte for byte, and it loads and plays through the test
hook; an RSID, a MUS and a crafted file are refused with their toasts and no flash program; a
64 KiB tune goes in and plays, each `sid_io_poll` and `sid_io_open` within `STORE_POLL_WORST`
(30,000) and every pass within `POLL_INSTRUCTIONS`; with the store's 48 keys used up, a tune's
PUT is refused (`STORE FULL`) with no program.

### Song player register match

`crates/sidou-tests/tests/gt2_match.rs` plays a song on SIDOU's C player
(`core/player/gt2_play.c`, through `src/player.rs`) and on GoatTracker's packed player
(`gt2-ref`, packed with the same options) and compares every call's SID writes, in order
(decision record 0007, "Equivalence, defined"). A mismatch reports the frame and call,
both write lists, the last calls before it and the C player's channels.
- **Fixtures** generated in the test from Rust, each feature's: notes, rests, key off and
  on, instrument changes, transposes, repeats and restarts, a 128-row pattern; the
  wavetable (delays, waveforms, inaudible waveforms, notes, jumps, its commands); the
  instrument classes and first waves, instrument 63's tempo; unused channels; orderlists
  that break readme 3.1 (read on into the next orderlist, R0 dropped); subtunes; every
  pattern command and the tempo command's ranges, simple and full pulse; waveforms read as
  commands and the handlers the packer leaves out; a song with no commands; and the
  generated songs' findings, each a fixture of its own.
- **Every example song, every subtune**, 3,000 frames with the defaults, then with each
  option alone (`-O0`, `-R0`, `-S2`, `-S4`, `-S8` and `-S0` on the host, `-N`, `-B`, `-C`,
  `-J`, `-D`, `-H`, `-I0`, `-AF000`, `-A0A00`), and three examples with the full matrix:
  pulse and realtime optimisation, multispeed 1, 2, 4 and 8, PAL and NTSC, the four ways
  of writing, player.s and altplayer.s (256 sets).
- **Generated songs** (`gt2-ref`'s `gen`): valid random songs with every command and
  special note, table programs with jumps and loops, and random options within the
  device's caps; 300 seeds, 5,000 with `--ignored`. A failing seed is printed with its
  options; replay it with `gt2_ref::gen::generate(seed)`, and make its finding a fixture.
- **The one tolerated difference:** a note past B-7 makes the 6502 read its frequency's
  high byte from the packed song table, whose bytes depend on the assembled player's size.
  The C player marks such a channel (`gt2_player.freq_unknown`) and the test accepts a
  different value in that channel's two frequency registers only, counting the calls. Two
  examples do it (`everlasting.sng`, `hyperspace.sng`); fixtures and generated songs must
  not.
- **The events:** every gate edge of each channel's wave register in the reference's
  writes is reported on its call (`GT2_EV_GATE_ON`, `GT2_EV_GATE_OFF`), every note comes
  with its row's event, and the row events follow each channel's orderlist.
- **The cost on the host** is reported, not gated (`the_players_cost_on_the_host`: the
  mean, 99.9th percentile and worst call); the device's instructions a sample are gated in
  the emulator (M2c Task 6).

The ordinary run covers 14 example songs and every subtune at 3,000 frames per
configuration: defaults plus 16 individual/paired option sets, a 256-combination
matrix on three examples, and 300 generated seeds (0..299). The canonicalisation
test in `song.rs` prints how many shipped examples were already canonical and loads
all of them into the device-sized region. The measured totals and the out-of-range
pitch exception's counts are recorded in the
[M2c status](docs/plans/2026-10-15-m2c-song-engine.md#m2c-status).

To look at a mismatch by hand, `gt2-trace` prints the reference's writes for the same song
and options.

### The player in the audio interrupt (M2c Task 6)

On the host, `crates/sidou-tests/tests/song_ctl.rs` (`cargo test --release -p sidou-tests
--test song_ctl`) runs `core/player/song_ctl.h` through `src/transport.rs` as the device
does (a block's commands, its render split at the call, a main-loop pass between blocks):
- the shape's analysis in slices of 1, 3, 64 and SONG_SLICE_UNITS units makes the shape the
  whole analysis makes, on every example and 100 generated songs;
- the frame clock is gt2reloc's CIA period exactly, and every call falls at floor(k x
  samples a call) from the play's block;
- edits of each widening class (a note past the range, a first command, a filter, a new
  instrument's tables, an orderlist rebuild; a wave delay, a pulse low nybble and a
  channel's first note through a hand-over) play, from the edit (or the hand-over), as a
  player started on the edited song and fast-forwarded to the same call, and the shape
  ends on the fresh analysis's once quiet;
- stop, continue, mute, a flood of edits (none lost, at most 8 a block), a swap's old space
  freed only once applied, jam, seek, the events and their overflow, multispeed 8 refused,
  a swapped chip given the player's registers.

`tests/song_locks.rs`: the LOCK chunk sorted, through the trailer and the CRC rule, carried by
row inserts, deletes and cuts (a cancel moves nothing); a locked row's cutoff and pulse width
on the chip after each of its calls, the song's own after, the player as without locks; the
character's locks at the hook. In fm1-emu, `song.rs` plays the first song with its lead's
first bar's cutoff locked low, bit-exact with the host and darker than without, and
PLAYER_WORST_BLOCK runs every example with five locks on as many rows as 512 allow.

`tests/app/demo_song.rs` keeps `assets/songs/sidou-loop.sng` and `app/demo_song.c` in step
with their source (`src/song_embed.rs`; `SIDOU_SONG_UPDATE=1` rewrites them) and checks the
song is canonical GTS5; `gt2_match.rs` plays it against GoatTracker's player.

In fm1-emu, `crates/emu-gate/tests/song.rs`: PLAY plays the first song and the audio equals
the host's render (`common::render_host_song`, which runs the same song_ctl on the host
chip) sample for sample over 10 s, at multispeed 1 and 4 (a song written into the device's
region before PLAY), with edits made while it plays (a test hook, `song_test_hook` in
`platform/fm1/main.c`, each edit's block read from the interrupt's applied count), with the
keys moved to chip B at PLAY (chip B rendered and mixed from the block it goes in use, read
after every block); PLAYER_WORST_BLOCK on the bench image `sid_bench_song`
(every example and subtune at multispeed 1 and 4 and 40 generated seeds, written into its
RAM one at a time, eight cell writes or swaps a block); the analysis's slices and passes
while the largest example loads; the multispeed 8 refusal's toast; stop's releases and chip
A's hand-back; the map (the song's region, the free RAM). `ram_audio.rs` runs the song in
the interrupt for its SRAM checks.

### Songs on the store and About (M2c Task 8)

Host tests cover the sliced reader/writer (`song_stream.rs`), saved-size accounting
(`song_capacity.rs`), store I/O and its runtime bindings (`tests/app/song_io.rs`), the
SONG menu and SAVE routing (`tests/app/views.rs`, `save_key.rs`), and schema-1/2/3/4
migration (`settings.rs`). Schema 4 keeps THEME at byte 107 and adds last-song at
108..123. A refusal before destructive loading leaves the old song and runtime state
alone; a later failure reloads the old file. Saves freeze model edits from acceptance,
including the wait for store space, while playback and performance controls continue.

After an fm1 build, run from `host/`:

```sh
SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo nextest run --locked --release -p emu-gate \
    -E 'binary(song) | binary(song_store) | binary(song_bank) | binary(ram_audio) | binary(flash_budget)'
```

`song_store.rs` checks save/reboot/load against the host audio, bad CRC/capacity/setup
refusals and runtime restoration, unchanged saves writing nothing, frozen edits and the
SAVE snapshot, NEW/SAVE AS/DELETE, boot recall, concurrent forced song/settings saves,
and About's music/restoration or silence for a dirty song. `song_bank.rs` checks bank-1
name-order lookup, missing/busy/dirty recalls, request replacement, stored MIDI setup,
and TRACKER refresh after a same-shaped recall. Both bound complete main-loop passes
by 120,000 instructions. `screen.rs` covers the four SONG pages and their golden/cell/
SPI costs; `tests/app/credits.rs` checks that `CREDITS.md` still generates the checked-in
About pages. Docs-only attribution links must leave that generated output unchanged.

The map test reports RAMTEXT, RAM and POOL separately, with `__song_region` at POOL's
start (65,536 bytes, 4 KiB aligned). Each data region must retain 4,096 bytes; the
combined floor is 7,000 in this integrated tree: M2c's 22,000 less M2a 2,000, M2d 6,000,
M2e 2,000 and M2f 5,000. The active image ceiling stays M2f's 512,000 bytes, not M2c's
historical 414,000. `song.rs` charges eight edits plus isolated row-accounting cost to
the player's worst block (≤60 instructions/sample); its printed remaining budget
includes the player. `slots_budget.rs` reports the headroom without the player, and
`chip_b.rs` measures the extra mix/MASTER costs. A song and M3's 6502 never run together;
[decision record 0008](docs/decisions/0008-m3-6502-and-fm1-sid.md) places M3's CPU outside
the interrupt, leaving this headroom for its stamped register writes.

### Golden renders

`renders_match_recorded_hashes` renders the shared benchmark workload
(`bench/workload.c`) for a 6581 and an 8580 with one chip, and a 6581 with
two chips. It compares the FNV-1a 64 hash of each render with
`tests/golden/hashes.txt`. `workload_never_clips_and_has_no_dc_offset` checks
the same renders for clipping and DC offset.

**A failure means** the output changed, by as little as one bit. If you did
not mean to change the sound, it's a regression. If you did, rewrite the
file:

```sh
SIDOU_GOLDEN_UPDATE=1 cargo test --release -p sidou-tests --test golden
```

Before you commit the new hashes, listen to the change and say in the commit
message why the sound changed. A hash change on its own proves nothing.

Golden hashes depend on exact float results. Compiler flags that relax float
semantics (`-ffast-math`, `-ffp-contract=fast`, passed through `CFLAGS`)
change them; see [BUILDING.md](BUILDING.md).

### reSIDfp reference tests

These render the same register writes on SIDOU and on reSIDfp (libresidfp,
test-only) and compare them:
- **pitch**: an unfiltered sawtooth at A4 must be within 0.1 cents of the
  theoretical frequency and of reSIDfp, for both models (a reSIDfp result off
  from theory points at the harness, not the core);
- **level**: one unfiltered voice (sawtooth, triangle, pulse, at two pitches)
  must be within 0.2 dB of reSIDfp's RMS level, for both models;
- **envelope shape**: the RMS envelope of a note through attack, decay,
  sustain and release, normalised to the sustain level and compared down to
  -20 dB, must be within 0.2 dB of reSIDfp on the 8580, and 0.5 dB on the
  6581.

The tests print the measured values. The M0 values are in
[decision record 0002](docs/decisions/0002-m0-sid-cost.md), M1b's in
[decision record 0003](docs/decisions/0003-m1b-accuracy.md).

**A failure means** SIDOU moved away from reSIDfp on something both
should agree on. The 6581's envelope tolerance is looser: its kinked
envelope DAC leaves 0.31 dB, against the 8580's 0.03 dB.

**Tolerances only get tighter.** Never loosen one to make a failure pass,
unless a decision record in `docs/decisions/` explains why.

### The filter against reSIDfp

`filter` checks the filter tables (`gen/filter.h`, below) at the small-signal
level they were fitted at (one voice of noise at envelope 0x44), where SIDOU's
linear filter should match reSIDfp's:
- **transfer functions**: both engines play noise through the filter and the
  same noise unfiltered, and the ratio of the two is the filter's response
  (`sidou_tablegen::filter_fit`). LP, BP and HP at FC 64 to 512 and
  resonance 0, 8 and 15, compared from half to twice reSIDfp's cutoff and
  below 7 kHz (above, SIDOU's trapezoidal filter departs from the analogue
  response whatever its tables say), each mode at its best level: the 8580
  within 1.0, 0.55 and 0.3 dB, the 6581 within 3.32, 3.13 and 2.89 dB;
- **passband gain**: from the same measurements, each mode's gain in its
  passband (LP f0/8 to f0/3, BP within 1.2x of f0, HP 3 f0 to 9 f0), SIDOU
  minus reSIDfp, held to the measured range: the 8580 within 0.2 dB, the
  6581 within -1.74 to 1.79, -2.02 to 2.06 and -0.40 to 0.52 dB (LP, BP,
  HP; 1.7 to 6.5 dB louder before M1b Task 6);
- **sweeps**: `sidou-compare`'s low-, band- and high-pass noise sweeps at that
  level, as RMS envelopes, the largest window and the RMS over the windows:
  within 1.62, 2.03 and 1.70 dB (RMS 0.54, 0.80, 0.87) on the 8580 and 4.84,
  2.46 and 1.01 dB (RMS 1.07, 0.68, 0.43) on the 6581. These also measure what
  the tables cannot fix (the comment in `filter.rs` lists it; the 8580 misses
  the plan's 1.0 dB, explained in decision record 0003);
- **full-scale sweeps**: the same sweeps at sustain 15, where the 6581's
  filter is level-dependent: the 6581 within 1.88, 1.89 and 1.49 dB (M1b
  Task 6's targets 2.0, 2.0 and 2.1), the 8580 record only (0.83, 2.05,
  1.27);
- **loud sweeps** (6581): three voices at full sustain into the filter,
  noise and a chord, where reSIDfp's 6581 filter is furthest from linear
  (target 3 dB): noise within 2.44, 1.92 and 2.81 dB, the chord within
  2.33, 4.37 and 2.84 dB (its band-pass misses; decision record 0003).

Every ceiling is the measured value rounded up to 0.01 dB, so nothing
regresses unnoticed.

### Chip profiles against reSIDfp

`tests/profiles.rs` (M1c Task 3, [decision record 0006](docs/decisions/0006-chip-profiles.md))
holds each chip profile, at zero character, against reSIDfp set to that profile's settings:
the filter's transfer functions and passband gains as `tests/filter.rs` measures them, with
reSIDfp's filter curve and range set to the profile's (each profile within M1b's
tolerances, or its own ceiling where a profile away from reSIDfp's default curve misses
them), and OSC3 read for read on combined waveforms with reSIDfp's `setCombinedWaveforms`
at the profile's strength. reSIDfp keeps the 6581 filter range once per process, so the
file is its own test binary and its measurements take a lock. It also records, without a
bound, age/heat's two target rows against reSIDfp at their settings. The character's
controls are unit tests (`tests/unit/sid_character.rs`): each one's direction and
measurability at every value, clamping, and clicks, by the effect slots' measure: a jump of
each control from 0 to 127 through the host's sink and its ramp, on a triangle at ten
phases, against the same move made smoothly (the ramp's path at its speed, interpolated at
every sample), keeps the 2nd difference of their difference within 30 LSB; a 100 LSB step
and the same jump at once, for every control, exceed it. The age's blend of two filter
rows (`tests/unit/sid_filter.rs`) is held to the Hz blend of their cutoffs. The register
ring's handling (the ramp tick for tick against the host's sink, the two stages, a tick
that finds both queued, and the interrupt committing a stage in the middle of the choice
of a free DAC table, through a hook only the host build has), live's and the map's are app
tests; `crates/emu-gate/tests/character.rs` sends the CCs to the firmware, checks the chip in
emulated RAM and the audio against the host build, sample for sample, the host replaying
the device's ticks block by block (the drain counts them), does the same for the age that
the CHIP view's K1 turns from the panel (the host replaying the age the drain had after each
block), and checks that the image holds
  only the combined-waveform tables the profiles and expert selector use; `latency.rs`'s sweep case plays a
note while the character's CCs sweep, against ADR 0005's bounds, and bounds every
main-loop pass by the poll allowance; the SID gate's `character_*` scenarios run the
costliest character on the device.

### Register reads against reSIDfp

`register_reads` runs scripts of register writes on SIDOU and reSIDfp and reads
`$1C` (ENV3) or `$1B` (OSC3) from both at the same chip cycles, once per PAL
frame, or every 100 (ENV3) or 7 (OSC3) cycles over a shorter run. The reads
must be equal:
- **ENV3**: every attack, decay and release rate, gate changes in the middle
  of each phase, the ADSR delay bug, sustain changes during decay, and the
  power-on release from 0xaa;
- **OSC3**: triangle, sawtooth, pulse (three widths) and noise, and every
  combined waveform with and without noise, each at four frequencies; noise
  through TEST (short and long enough to fade), a TEST right after an LFSR
  clock, lock-up when combined with other waveforms, TEST releases into
  combined waveforms, switches between sawtooth combinations, and ring
  modulation. With no waveform selected, SIDOU reads 0 where reSIDfp holds
  the last value, and a test pins both.

Neither engine renders audio here: SIDOU advances with `sid_clock`, reSIDfp
with `clockSilent`, so a read can land on any cycle.

**A failure means** a register reads back differently from reSIDfp; the
message gives the cycle and the first differing values. Cases that a later
milestone task fixes are `#[ignore]`d with the task and the reason, and so are
two OSC3 gaps that stay (read-only and inaudible; decision record 0003): the
reads right after a hard sync and a cycle after a pulse-width write. Run them
with `-- --ignored`; they fail, and they should until the gaps are fixed.

### Generated tables

`core/sid/gen/` holds tables generated by `sidou-tablegen`: from reSIDfp, the
combined-waveform pulldown tables for each model and combined-waveform profile
(`gen/grid.h`) and the filter's cutoff and damping per curve setting
(`gen/filter.h`), and our own noise-register jump table (`gen/noise_jump.h`).
The filter tables are measured: `sidou-tablegen fit-filter` plays noise
through reSIDfp's filter (LP, BP and HP, FC every 32 steps and 1023 on the
6581, resonance 0, 8 and 15, and every resonance at six FC values mid-range),
fits SIDOU's filter to each response around its cutoff, and prints how well
it fits. Each
profile is 30,720 bytes per model. Per model, only AVERAGE, reSIDfp's default,
is compiled unless the model's flag (a CMake option of the same name) adds WEAK
and STRONG: `SIDOU_ALL_COMBINED_PROFILES` for the 6581 and
`SIDOU_ALL_COMBINED_PROFILES_8580` for the 8580, both on by default because the
expert selector can use all three on either model. All six represent 184,320 bytes
before lossless shared-block packing; the current firmware's `SID_PD` is 58,488 bytes.
Emu-gate's `character.rs` checks every sampled strength and that no unused blocks remain.

The checked-in headers are the source of truth; the generator is a
documented tool. After changing the generator or the reSIDfp submodule,
regenerate them from `host/`:

```sh
cargo run --release -p sidou-tablegen               # every table
cargo run --release -p sidou-tablegen -- fit-filter  # gen/filter.h, with the fit report
```

The drift tests (`tests/drift.rs`) fail when the checked-in files differ
from what the generator produces:
- `checked_in_tables_match_the_generator`, in every job: the computed tables
  (`grid.h`, `profiles.h`, `pulldown.h`, `player/gen/digi.h`), exactly. If it fails on a new
  platform with nothing changed, that is a libm difference (reSIDfp builds
  the pulldown tables with `pow`, and some entries sit on a threshold), not
  stale tables: keep the checked-in headers.
- `checked_in_filter_tables_match_the_measurement`, `#[ignore]`d: it
  re-measures reSIDfp's filter (about 20 s on 8 cores), so CI runs it in the
  Linux host job only (`cargo test --release -p sidou-tablegen --test drift
  -- --ignored checked_in_filter_tables_match_the_measurement`). It compares
  `filter.h` up to one unit in the last printed digit (0.1 Hz of cutoff,
  0.0001 of k): the fit's own maths uses the `libm` crate, the same on every
  platform, but reSIDfp builds its filter model and resampler with the
  platform's libm, so a value can round the other way elsewhere. Run it after
  changing the fitter or the reSIDfp submodule.

### Filter comparison (`sidou-compare`)

`sidou-compare` renders low-, band- and high-pass noise sweeps on both
engines (`sidou_tests::filter_sweeps`), at the small-signal level the `filter`
tests assert and at full scale (`*_full`), writes each pair as 16-bit WAV
files into `<dir>`, which must exist (default: the current directory) and
prints the largest and the RMS envelope difference per scenario and model. It is for
listening and for tracking the filter's distance from reSIDfp between
milestones. The `sidou_compare_smoke` test only checks that it runs and
writes all 24 files.

### Host benchmark

A criterion benchmark of the shared workload for one and two chips, on both
models. Throughput is reported per chip (samples × chips), and
`workload_begin` is not timed. Reports go to `host/target/criterion/`.

It has no threshold. Use it to compare before and after a change on the
same machine; it says little about the FM-1, where the emulator gate is
the measure.

### Emulator gate

The M0 decision gate. It runs the pi32v2 benchmark programs
(`sid_bench_1` and `sid_bench_2`) in fm1-emu, used as a library, once per
workload scenario (`bench/workload.h`) on each model (6581 and 8580): the triad, plus envelope worst cases
at the fastest rate on all three voices (holding sustain 0xff and 0xaa, gates
toggled every frame or every 128-sample block, and an idle chip), a
combined-waveform scenario (pulse+saw, saw+triangle, and noise+pulse retriggered
through TEST), and noise combined with other waveforms on all three voices
(TEST held, steady until lock-up, retriggered through TEST every other frame
or every block, and from power-on at the highest and a low frequency, with
triangle, sawtooth, ring modulation and sync), and sawtooth and pulse+sawtooth
on all three voices at the top two pitches (0xffff and 0xe000), where the noise
register shifts most often without being heard (the core defers those shifts).
For each it:
- counts the instructions per output sample, and fails if one chip needs
  more than 900, or two chips more than 1,800 (design document, section 8),
  on average or in any 128-sample block (a block's window also holds the
  next block's register writes, which errs on the expensive side);
- renders the same scenario on the host build of the same C code, and fails
  if any sample differs by more than 1 LSB (at M0 they are identical);
- writes `sid_budget_1.json` and `sid_budget_2.json` into the fm1 build
  directory. The top-level fields are the triad's on the 6581, comparable
  with earlier milestones; `scenarios` lists every scenario and
  `max_instructions_per_sample` the most expensive, which must stay within
  `SID_WORST_PER_CHIP` per chip, and `max_worst_block_instructions_per_sample`
  the most expensive block, which must stay within `SID_WORST_BLOCK_ONE_CHIP`
  or `SID_WORST_BLOCK_TWO_CHIPS` (`tests/common/mod.rs`), what the effect
  gates assume.

`SIDOU_GATE_PROFILE=1` also counts the render instructions per function and
adds the top ones to each scenario's report (`profile_instructions_per_sample`),
the same for its most expensive block alone (`worst_block_profile_instructions`)
and every block's cost (`blocks_instructions_per_sample`). It slows the gate
down.

The effects scenarios (`fx_budget.rs`) run the same workload, then the
post-chip chain (`core/fx`) over its output:
- `sid_bench_fx_{1,2}`: every effect at full, settled;
- `sid_bench_fx_worst_{1,2}`: the worst case, where every target alternates
  on every block so the smoothing never settles (each effect recomputes its
  coefficients on every sample) and bit-crush captures on nearly every
  sample.

Each fails if the effects cost more than 500 instructions per sample, if two
chips at the SID's worst block and the LIVE view's tap plus the effects cost
more than 2,300, or if any sample differs from the host build at all. The
SID's worst block is `SID_WORST_BLOCK_TWO_CHIPS` in `tests/common/mod.rs`,
1,440 for two chips: the SID gate's most expensive 128-sample block (1,439.67
since M1b Task 6's 6581 filter), which the SID gate holds it to. The tap is
`TAP_PER_SAMPLE` there, 4.5: LIVE is the view at power-on, so its tap runs by
default, and `ram_audio.rs` holds it to that. The audio interrupt has a deadline per block, so a
block, not a scenario's average, is what the effects must fit beside. They
write `fx_budget_{1,2}.json` and
`fx_worst_budget_{1,2}.json`.

The effect-slot scenarios (`slots_budget.rs`) run `sid_bench_slots_1` and
`sid_bench_slots_2`: the workload, then 17 scenarios over 4,096 samples of
its output, all with every control alternating on every block:
- the slot frame alone (three slots, algorithm NONE);
- each algorithm alone;
- every slot's heaviest together.

They fail if:
- any algorithm costs more than its slot's budget in
  [decision record 0004](docs/decisions/0004-effect-algorithms.md)
  (Distortion 150, Modulation 200, FX 450, the frame 100);
- the heaviest together cost more than 900, or two chips at the SID's worst
  block and the tap plus them more than 2,300;
- the heaviest per slot is not the one the last scenario runs;
- any sample differs from the host build (including the switch scenario's).

Then a switch scenario, measured per 32-sample block: from every slot's heaviest to
fuzz, flanger and light and back, every slot switching (they take turns). In the
second change the Modulation slot wakes from rest during the FX switch. It fails if:
- a block costs more than 1,000;
- more than 80 blocks cost more than 900;
- the worst block plus what else exists today exceeds 2,700: two chips at the
  SID's worst block, the LIVE view's tap and 100 of interrupt overhead. The
  gate prints what is left, the headroom for the future 6502 (M3), as
  `headroom_for_6502_instructions_per_sample` (191 since M1b Task 6, the tap
  counted; 227 before Task 6, without it).

This is 0004's transient allowance. Its output is part of the host comparison.

They write `slots_budget_{1,2}.json` with every algorithm's cost and every switch
block's.

Build the `fm1` preset first (see [BUILDING.md](BUILDING.md)). Without
`SIDOU_FM1_BUILD`, the gate prints a message and passes without running,
which is why the CI job checks that the JSON files exist.

Instruction counts are a lower bound on cycles: the pi32v2 mostly issues one
instruction per cycle, but code fetched from flash (XIP) and divides take
longer. Noise combined with other waveforms leans on divides: each call of
`sid_osc_bulk_limit` makes a few (up to about 20 when it walks far), one
instruction each in the count but more cycles on the device.

The noise worst cases start from the power-on register (0x3fffff, nearly all
ones), the fullest state a workload reaches in its quarter second: the
all-ones register (0x7fffff) needs TEST held through the fade, 0.4 s on the
6581 and about 8 s on the 8580, and otherwise turns up about once in 8.4
million shifts. Only that state at the start of a block takes a block over
the budget (accepted in the M1b review).

fm1-emu at the pinned revision cannot execute one instruction that clang
emits (`{R*4} = [-R]`). The emulator tests step it themselves
(`crates/emu-gate/tests/common/mod.rs`); see [docs/TODO.md](docs/TODO.md).

**A failure means** one of three things: the core got too slow for the
budget, the target and host builds no longer compute the same thing (look
for float flags or undefined behaviour), or the gate could not run the
program (the message names the file or the faulting address).

### Boot test

`crates/emu-gate/tests/boot.rs` boots the firmware image
(`platform/fm1/sidou_app.bin`) in fm1-emu the way the FM-1's SPL starts it:
mapped at `0x02000120`, entered in user mode with `r0` pointing at a
boot-parameter block. The A4 key is held from power-on. It runs 900 ms of
emulated time and checks that:
- the audio interrupt delivers the frames of at least the 500 ms after the
  LCD's start-up delays;
- left and right are equal;
- the output is sample for sample what the host build of the same code renders
  for the same instrument (`live_init`, the MASTER pot, then the A4 note-on,
  which the test finds in one of the first 16 blocks);
- the note is not silent and its pitch is 440 Hz within 0.5 %.

It writes `sidou_boot.json` (frames, peak, measured pitch, the note-on's
block, instructions in and out of interrupts, the largest audio block) into the
fm1 build directory. Like the gate, it skips without `SIDOU_FM1_BUILD`.

`the_store_reads_through_a_plain_xip_window` checks that the boot maps the
flash from 0x93000 up as a plain XIP window before the audio starts (the
window's registers, written once), that a word written into the flash chip
at the store's first byte (`STORE_LO`) reads back through XIP, and that the
boot then writes nothing into the store: the word is not SIDOU's, so the
store mounts the region foreign (docs/hardware.md, "Flash map").

### Flash budget

`crates/emu-gate/tests/flash_budget.rs` holds M2's image schedule (the
ceiling of `sidou_app.bin` after each plan, M2a 372,000 bytes up to M2f
512,000) and fails if the image is over the ceiling of the plan in progress
(`IMAGE_CEILING`): the latest stage whose code is merged (the plans land in
parallel). A raise is a change to the schedule's table, so it reads
as a change to the plan; a test checks the table only rises and leaves M3 at
least 69,564 bytes of the 581,564-byte slot. M3's stage (`M3_START` 506,000 plus M3's 28,000,
534,000) follows the schedule; it is not yet the image's ceiling (`IMAGE_CEILING` is
`M3_START` until Task 9 merges device code), and a test keeps at least 40,000 bytes of the
slot after it for M4 and M5. It prints the image's size and
what is left; it skips without `SIDOU_FM1_BUILD`.

### The store on the device

`crates/emu-gate/tests/store.rs` runs the store on the firmware
(`platform/fm1/store_fm1.c`, M2a Task 4). The firmware's test hook
(`store_fm1_hook` in RAM: a put, a get, a file of a pattern, a collection,
a song playing) does the store operations; the tests write it as a debugger
would. fm1-emu's flash is instantaneous, so a cut is the flash as it was
after the k-th program or erase (`store_fm1_counts`). The timings in
`store_fm1.h` are provisional (the P25Q80H datasheet's maxima), and by the
decision rule no write window fits them: programs are held operations, and
the tests that need windows set `store_fm1_window` in RAM. The tests:
- a value persists across a power cycle (a blank region formats at boot);
- a cut after every operation of a settings save, every third of a file's
  replacement and the collection and erases it leaves, and every 24th of a
  64 KiB file: a fresh boot (or the host's store core on the dump) reads the
  old value or the new, the old file or the new, never a mix;
- in 16-byte windows, every program starts within `STORE_WINDOW_LEAD_US` of
  an audio block's end, some wait for a window, none is held, no block is
  missed, and the driver's masked instructions are within its 100 µs estimate
  at 240 MHz. The printed cost is `fl_prog_ram`, not the whole interrupts-off
  stretch or a measured flash busy time. The window bound is 1,000 µs; held
  operations are bounded at 35 ms, with 10 ms between quiet operations. The
  test checks the hold's silent DMA halves, not a longest masked duration;
- no erase while a chord plays, a CC every 100 ms comes in, or a song plays;
  once quiet (a DAW's clock and active sensing do not count), the erase comes,
  with the output faded to silence;
- the peak scan costs at most 2 instructions a sample, and nothing when off
  (`chip_b.rs`'s M1 comparison runs with it off);
- a 64 KiB file during a held chord, in windows, lands with no erase and no
  hold;
- every `store_poll` within `STORE_POLL_WORST` (30,000), each kind of pass
  (mount slices, its finish, a record, a copy, a file's chunk, a collection's
  scan and kill, forget, an erase, the newest record's copy forward,
  maintenance's plan) at most 4 instructions a work unit (a kind over raises
  its weight in `store.c`), and `store_fm1_service` with the rest of a pass
  within the poll allowance.

`latency.rs`'s save case plays notes while a 64 KiB file is written in
16-byte windows.

### Live instrument test

`crates/emu-gate/tests/live.rs` plays the firmware through fm1-emu's front
panel (keys, encoders, the MASTER pot) and checks:
- a key plays its pitch (A4 and E4, within 0.5 %), and nothing plays before;
- OCT+ doubles the pitch and lights its LED; a held key lights its LED;
- turning K1 down by 40 detents cuts the share of the power above 2 kHz at
  least fourfold;
- MASTER turned down lowers the peak;
- the LCD after boot: the C64 blue at a fixed background pixel, the light blue
  border, text inside, no other colours; nothing is sent while nothing
  changes, and a held note goes out as one run of cells (one LCD window);
- the 6581/8580 toggle: `sid_init` runs in the main loop only (the test counts
  its instructions per call), no audio block gets near the block's
  instruction budget, no block is silent, and the FX LED lights; the device's
  `chips` state, read from emulated RAM through its ELF symbol, shows the
  active chip flipping on each FX press with the right spare model and no lost
  reset;
- the main loop stays under 5M instructions/s (about 2 % of the core) idle and
  with a key held.

### Screen test

`crates/emu-gate/tests/screen.rs` checks what the LCD shows and what drawing
it costs. It reaches each view through the panel, as a player would (HOME,
SELECT, ALGORITHM):
- **snapshots:** the status screen after boot, with a note held and with the
  toast OCT+ shows, MIDI learn's frame asking for a CC and its
  LEARNED toast, the menu, the glyph sheet (every glyph, the palette
  and a pixel overlay), the boot's prompt, loading and title card, About,
  each SCENE piece a second into a held chord, each CHIP page (the focus on
  the chip, DIST, MOD and FX, the character and slots set over MIDI), and
  the held frames of THE CHIP's, THE SCENE's and THE OSCILLATOR's title
  cards (also checked against `assets/logo/title-chip.png`, `title-scene.png`
  and `title-oscillator.png` pixel for pixel)
  match the golden images in `tests/golden/lcd/`, pixel
  for pixel, and every pixel is one of the 16 palette colours. The title
  card (but for its version and PRESS ANY KEY rows) and About's top 72 pixel
  rows are also checked against the logo's source,
  `assets/logo/metal-chrome.png`, pixel for pixel. The status
  screen's version cells and the menu's cursor cell are left out; the cursor
  is checked to blink. Going back from the glyph sheet restores the menu and
  then the status screen exactly, and the toast goes after 1.5 s. The goldens
  are 4-bit indexed PNGs in the art palette;
- **transitions:** sampled every frame, a wipe shows the new view over the
  old one with at most a white edge row between them, the edge moving down
  (opening) or up (closing); a chip change sweeps ramp colours down the
  border in bands, and the border comes back;
- **cost:** the instructions `ui_poll` spends (its callees included) and the
  windows and SPI bytes `lcd.c` counts in each 40 ms frame: idle, for one
  note, the menu idle, a wipe each way and a raster. It prints them as JSON,
  with the SPI time at 12 MHz, and bounds them: nothing is sent idle, one
  note is one window under 1 ms of SPI, every frame of a transition stays
  under the 20 ms SPI budget, a wipe sends no more than a full screen and its
  edge rows, the raster sends every other frame, and no single pass of the
  main loop renders more than two runs. The full boot and a round of About
  are bounded in instructions a frame and in their busiest `ui_poll`, and so
  are the wipes into the title card and About, which reveal the logo's
  bitmap. Each SCENE piece, with a band playing on the keys and the filter
  wide open, is bounded in cells a frame (average and most), in
  instructions a frame and in its busiest `ui_poll` (tied at compile time to
  the latency gate's poll allowance); starting a piece (SELECT, or the
  menu's ALGORITHM) costs the input's call little and the main-loop pass
  after it (`midi_drain` to `midi_service`) stays within the allowance with
  it, a compile-time tie and a measured check; MIDI learn's frame over each
  piece and over LIVE, the band playing, keeps every frame within 200 cells
  and the SPI budget, opening and closing; a chord's note-ons change every
  piece within two frames, and silence calms it. Each title card, from the
  input to the end of the cut after it, keeps every frame within 200 cells
  and the SPI budget, and is bounded in instructions a frame and in its
  busiest `ui_poll`; the input that brings it in and the busiest main-loop
  pass while it shows stay within the poll allowance together (compile-time
  ties); any control skips it without its action, and a note from the keys
  or MIDI plays and ends it. The CHIP view, idle, for a knob's detent and for
  its busiest frames (every control over MIDI at once, a focus move and a
  chip change), keeps every frame within 200 cells and the SPI budget and is
  bounded in instructions a frame and in its busiest `ui_poll`; SELECT,
  ALGORITHM and a knob on it, with the busiest main-loop pass after each,
  stay within the poll allowance (compile-time ties), but a profile step, whose
  pass is the chip reset (bounded on its own); every chip reset at full
  character (ALGORITHM onto each profile, the FX button both ways, CC 85 onto
  each profile) stays within its bound, the worst onto worn; a CC changes the
  page's row and K1 turns it back to its golden. RED MOON RITE, SCENE's
  sixth piece, is in every SCENE check above, and has its own with the demo
  song: goldens a second in, at the fill's landing and with the sword stuck
  (`scene-rmr-song`, `-disarm`, `-stuck`); a minute of the song bounded in
  cells a frame (average and most, against the prototype's 117 and 176),
  SPI, instructions a frame, the busiest `ui_poll` and pass, and on further
  runs the busiest scan slice, run and tile, each tied to the poll allowance
  with a song's analysis slice; a muted drum voice stops the chops and
  parries; and once the song stops the duel calms and sends nothing.
  `song.rs` checks the song bit-exact with the host while it shows.

The host tests cover the same machinery without the device:
`tests/app/screen.rs` (the view stack, focus, routing, the capture, the
toast, the cursor blink, and the wipe and raster frame by frame against the
SPI budget), `grid.rs` (runs, overlays, held rows, and the pixel region:
pixel for pixel against the logo's PNG, text and overlays over it, which
cells it sends when, refusals, and random regions mixed with edits),
`logo_bitmap.rs` (the converter: its round trip, the drift test, the compiled
table against the PNG, the palette and size checks), `logo.rs` (each logo's
footprint, cost and overlays as declared, Metal Chrome's and Cracktro Chrome's bands and scenes
against the PNG), `boot.rs` and `about.rs` (the sequences, the layouts with
either logo, the SPI budget a frame and under the wipes), `widget.rs`
(frames, numbers, bars, the menu), `views.rs`
(HOME and the menu, the panel playing as before in every view, the toasts),
`chip_view.rs` (the CHIP view: its page, the focus, the profile picker and the
slots' algorithms, K1..K4 on the focused section through the view-aware
`panel_knob_param` against what the panel turns, the EDIT shortcut in every view,
MIDI learn binding a CHIP knob, SELECT and ALGORITHM under learn's frame, MIDI
out sending its CCs, MIDI's changes on the page, and a burst within the budget) and
`learn_view.rs` (MIDI learn's frame, gesture, cancel, timeout and clearing),
`scene.rs` (SCENE's pieces against models of their rules: rule 30, Langton's
ants, the starfield's flight, the fire's and the plasma's bounds and
interlacing; what the music moves in each; the view's navigation, its tap,
its budget a frame with a band playing, its wipes, and the canvas coming
back after the screen drew over it, RED MOON RITE among them);
`scene_rmr.rs` (RED MOON RITE's state machine frame for frame against
`assets/scene/red-moon-rite.js`'s trace, `tests/golden/scene/rmr-trace.tsv`;
its layers against the script's golden frames; its painter region, toast and
MIDI learn's frame in SCENE), `scene_song.rs` (its music hooks: roles and
drums guessed and from the `SCNE` chunk, the place in the bar and the fill,
the bass, mutes, SIDOU LOOP through the transport, and the keys);
`title_card.rs` (the cards' sequence
frame by frame, the glow's levels against the art's, the held frames against
their PNGs, the budget, skipping, notes playing through, nothing drawn over
them, the CARDS option) and `title_card_bitmap.rs` (their converter: the drift
test, the round trip, the compiled tables against the PNGs, the font's dither
glyphs against the art's Bayer order, refusals and the size);
`midi_learn.rs` has learn's state machine and its bindings through MIDI in and out.

**A failure means** the screen looks different or costs more. If you meant
the change (a new glyph, a moved label), look at the
`<name>.actual.png` the test writes into the fm1 build directory, then
rewrite the goldens and commit them with the change:

```sh
SIDOU_LCD_GOLDEN_UPDATE=1 SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo test --release -p emu-gate --test screen
```

### MIDI on the host

`sidou-sysex` is a packet library, not a MIDI transport; its one command writes `.sid` tunes as
`.syx` files (above, "`.sid` files on the store").
`cargo test --locked --release -p sidou-sysex` covers every packing length through
256, canonical high-bit masks, checksums and all command payloads. The app tests
in `sysex.rs` send those frames to the C state machine through a simulated store,
then decode its USB event packets. They cover stop-and-wait, pool/full refusals,
CRC/sequence errors, ownership, late bytes, output back-pressure and the two-second
deadline starting at a completed ACK. Deterministic random/mutated-stream tests
exercise the same C machine under ASan and UBSan; no real device is opened.
`gt2_ins.rs` checks indexed GTI5 export, bounded two-pass import, pointer/table
relocation, untouched old rows, freeze/refusal atomicity and saved-size accounting.

`crates/emu-gate/tests/sysex.rs` sends frames through the emulated TRS jack. It
checks a stored song byte-for-byte, bank-1 recall and PLAY; current-song loading;
instrument relocation with a paused player and intervening transport requests;
and pool refusal before a claim/program. Two 64 KiB/chord cases distinguish the
emulator window override (no holds or erases) from the production default (every
program held, no erases, the explicit hold's ramp endpoints and sound between
programs). DATA back-pressure stays below the documented one-second frame gap;
object END uses the existing bounded loader/import allowance. Every combined
main-loop pass remains within `POLL_INSTRUCTIONS`. Emulator timings cannot
validate hardware windows or physical USB output: see hardware checklist 8c/12.

`crates/sidou-tests/tests/app/`:
- `midi_map.rs`: the CC/NRPN map (`app/midi_map.c`). Every parameter has one row, CCs
  and NRPNs are unique and keep clear of the numbers MIDI reserves, ranges and defaults
  match the instrument, each row round-trips through its CC and its NRPN. NRPN parsing:
  MSB then LSB, LSB-only updates, increment and decrement, the null NRPN, RPNs (RPN 0 the
  bend range), state per channel and per input, a small parameter that never passes
  through 0 on its way. `the_reference_is_up_to_date` fails when
  `docs/reference/midi.md` differs from the table; `SIDOU_MIDI_DOC_UPDATE=1` rewrites
  it.
- `expression.rs`: velocity, pitch bend (smooth across all 16384 values, the sync
  lead's two oscillators together), the wheel, aftertouch and the vibrato, at the
  register level.
- `midi_out.rs`: the outgoing USB-MIDI stream: packets, the rate limit, deduplication,
  no echo of MIDI input, and program changes for the panel's presets.
- `live_instrument.rs` and `reg_ring.rs`: the envelope mode. It is INSTANT at power-on,
  one write on a change, kept across presets, and sent after every chip reset before any
  register. Through the register ring it reaches the active chip, and the chip after a
  model change; a retrigger after a slow release starts at once with INSTANT, and waits
  for the ADSR delay with AUTHENTIC, and so does a note-off after a slower decay (SAW
  LEAD's, at each phase of the decay's period: 32 of 44 wait with AUTHENTIC).

### MIDI test

`crates/emu-gate/tests/midi.rs` plays the firmware through fm1-emu's TRS
MIDI jack (UART1, one byte per 320 us) and checks the audio and the
rendered chip's state, read from emulated RAM through the `chips` symbol:
- `90 3C 64` plays C4 (261.63 Hz within 0.5 %) on a gated voice at the
  frequency table's value; `80 3C 00` clears the gate, the note releases
  (still audible 20 ms later) and is silent 0.8 s on;
- CC 74 at 0 sets the cutoff to 0 and cuts the share of the power above
  2 kHz at least fourfold; at 127 it sets 2047;
- program change 1 switches to PWM BASS (pulse wave, cutoff 700), which
  plays C4 at its pitch, and program change 0 back to SAW LEAD;
- 3000 bytes at full speed arrive with nothing lost (`midi_stats` in RAM:
  entries, losses, parser restarts, the longest wait in the ring under
  2 ms), and USB was never polled with the controller up;
- the cost: the main loop stays under 5M instructions/s idle and with the
  jack at full speed, and full-speed TRS MIDI adds under 2M interrupt
  instructions/s. This test reads no firmware symbol, so it also runs on a
  build from before MIDI input, for the difference;
- an NRPN sets the cutoff at full resolution; a pitch bend shifts the measured
  pitch, and the sync lead keeps its ratio while it bends;
- floods of chip-model CCs, pitch bends and program changes neither stall the main
  loop nor lose input;
- a flood of filter CCs over USB (16 a millisecond, put straight into the MIDI in ring:
  fm1-emu has no USB controller) of CC 74 and of CC 71, at age/heat 0 and 127, adds no
  more to the audio interrupt than the ISR's allowance (100 instructions a sample), and
  age 127 no more than 2 a sample over age 0 (the filter is recomputed once a block);
- MIDI out: the panel's changes go out as USB-MIDI packets, MIDI input is not echoed,
  and a tick of MIDI out costs little at its worst, with every parameter learned too;
- MIDI learn: SAVE pressed, K1 turned and CC 14 (a CC the map leaves free) sent over TRS
  bind CC 14 to the cutoff (`learned` and `instrument` read from RAM), which then follows
  CC 14 on any channel while CC 74 still works; HOME cancels a second learn and SAVE three
  times clears; a flood of the learned CC at the jack's full speed costs the main loop no
  more than a flood of a map CC, and both, like a flood of CC 64, stay under 5M
  instructions/s. The cost tests keep the jack busy for their whole window
  (`Machine::midi_paced`: fm1-emu drops a backlog past 3125 bytes) and check that nothing
  was dropped.

### MIDI modes, banks and clock tests (M2e)

`crates/emu-gate/tests/midi_modes.rs`: MULTITIMBRAL set on the MIDI page moves the
instrument to chip B and plays it from channel N + 3, chip A's channels reach the router's
jam voices (`routing` in RAM) and not the instrument, OMNI gives chip A back; a dense
four-channel stream at TRS's full rate keeps every pass within the poll allowance; bank 2
program 3 picks the 8580 R5 profile, bank 1 program 0 asks for the first song, bank 0 is the
presets. `crates/emu-gate/tests/midi_transport.rs`: SIDOU's first song started from a 120
BPM clock over TRS lands its rows within 1 ms of the clock's 16ths from two beats on (the
device's own poll gives the clock its jitter); stop, a song position to bar 3 and continue
play on from row 32; every pass within the allowance. On the host, `tests/app/midi_route.rs`
(the modes, the banks, the `MIDI` chunk), `tests/app/midi_sync.rs` (the clock over the real
song control: phase, transport, clock out's packets) and `tests/app/views.rs` (the MIDI
page).

### Latency test

`crates/emu-gate/tests/latency.rs` injects a note-on into the emulated TRS jack at a
random cycle, each trial from a fresh boot past the title card and its wipe, so the
instrument is idle as a player finds it. It measures, from the note-on's last byte: when
the audio interrupt takes it, when its first non-silent sample is in the DMA buffer, when
that sample leaves the DAC, and when the note's envelope comes up (-30 dB of the held
note's peak). Then, once the note holds its sustain level, a note-off: when the audio
interrupt takes it and when the release's first step (the voice's envelope counter, read
from the rendered chip) is in the buffer. The cases and their trials:
- the status screen idle: 64 trials with the envelope INSTANT (the default), 16 with
  AUTHENTIC (CC 26 = 127 after the boot, checked on the rendered chip's gate reset);
- the screen redrawing (K1-K3 turning): 64, INSTANT;
- the note-on around the USB retry's attach: 32, INSTANT;
- a 64 KiB file being saved in 16-byte write windows (M2a Task 4): 24, INSTANT.

It asserts for every trial:
- taken within one block and one poll (2.9 + 0.5 ms), plus the USB attach's stall when
  the note lands around it;
- in the buffer within that, plus the render allowance: the run's longest audio
  interrupt, measured and held to the audio budget's block (2700 instructions a sample).
  This follows from the bound above, as the taking interrupt is one of the run's; it is
  kept as the plan's statement;
- audible within two blocks of being taken;
- the held note is not silent: its peak is at least half what the host core
  gives the same note (SAW LEAD on the 6581, C4 at velocity 100), with no
  absolute number;
- INSTANT: the envelope is up in the block that takes the note; AUTHENTIC: within the
  ADSR delay's longest (32767 cycles) and the attack;
- the note-off taken within the note-on's bound; INSTANT: the release's first step in the
  block that takes it; AUTHENTIC: within the ADSR delay's longest.

It prints a JSON report per case (the distributions as min, median, mean and max) and
writes it into the fm1 build directory as `latency_<case>_<envelope>.json`.
`SIDOU_LATENCY_TRIALS=<n>` runs every case with n trials; 64 reproduces the run
[decision record 0005](docs/decisions/0005-midi-note-timing.md) has the numbers of:

```sh
SIDOU_LATENCY_TRIALS=64 SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo nextest run --release -p emu-gate -E 'binary(latency)'
```

fm1-emu does not emulate USB (see [docs/TODO.md](docs/TODO.md)), so USB
MIDI is covered by the host tests of the packet conversion
(`tests/app/midi_usb.rs`), the ring (`midi_ring.rs`) and the input logic
(`midi_rx.rs`), and by hardware testing.

### Store layout

`crates/sidou-tests/tests/app/store_layout.rs` checks the store's place in
flash (`app/store_layout.h`; docs/hardware.md, "Flash map"): whole sectors,
exactly the range the update loader never touches (`LDR_DATA_LO/HI`, read
from `platform/fm1/loader/ldr_core.c`'s text, and Felucca's `FL_DATA_LO/HI`),
ending where the update protocol's staging area (`OTA_AREA`) begins, above
the app slot (`app.ld`'s XIP region plus 0x4000 and 0x120), and inside the
package's VM descriptor, read from a package `sidou-pack` builds with
stand-in SDK inputs; and that the guard band (the last page of every
sector) covers where an update record would sit; and that the update
loader's write gate (`platform/fm1/loader/loader.c` `FL_RANGE_OK`) stops
exactly at the store's ends.

### Store

`crates/sidou-tests/tests/app/store.rs` runs the store's core
(`app/store.c`; the format is
[docs/reference/store-format.md](docs/reference/store-format.md)) on a
simulated NOR flash with the part's semantics: a program only clears bits,
within one 256-byte page; an erase sets a sector to 0xFF. The simulator
also answers a share of operations with STORE_LATER (the core must retry),
and records what the store must never do: program the guard band, program
over programmed bytes anything but the bytes already meant there (a claim
programs the erase header again) or, in a header's magic, the kill's zeros
over bits that read 1 (never a 0 over a bit that reads 0), program across a page, use a sector whose erase was cut before erasing it
again, program from a source in flash, run more than one flash operation in
a `store_poll`, or do more than `STORE_POLL_WORK` of work in one (`store.work`,
weighted units of about 4 instructions or fewer each: bytes scanned, CRC
bytes at 3, keys compared, map calls, sector loops; the host proxy for Task
4's instruction gate).
It also counts each page's programs between erases. In **faint** mode a cut
program or erase leaves bits neither programmed nor erased, which every
mount reads anew at random; a cut erase may fall in the part's pre-program
to 0 or in the erase proper, bit by bit. In **pulse** mode an operation's
bits tear together (one progress level an operation, a small spread a bit),
so a cut often leaves every bit of a commit word faint, or none started;
with it, faint bits are also read anew now and then within a session. In
**biased** mode faint bits drift toward erased (each read gives 1 three
times in four). In **suspend** mode an erase answers STORE_LATER midway, the
sector reading as an erase cut short until a later call resumes it (Task 4's
erase-suspend). The hook can also fail a share of operations with no cut
(STORE_ERROR, the part having done part of each). Every API call outside
`store_poll` (put, delete, file begin and rename, collect, erase wanted) is
held to `STORE_POLL_WORK` too. The tests:
- **power loss:** random puts, deletes, collections and refills with windows
  of 1, 3, 4, 8, 12, 16, 64, 100 and 256 bytes, faint bits for half the
  seeds (pulse mode for two faint seeds in three, biased for two in five),
  suspended erases for one seed in four; for every flash operation, the region as a cut during it would
  leave it (a program that cleared any subset of its bits; an erase that left
  all 0xFF, a mix of old bytes and 0xFF, or a part erased; in faint mode,
  faint bits) is mounted and every key must read its value from before the
  interrupted write or after it; every eighth fork, and every fork of an
  erase, also refills, writes, and mounts again, and every key must read what
  the first mount read. Real cuts at random points carry on from the torn
  state. A directed test collects a deletion's sector before the sector of
  the value it deleted. The long version runs thousands of seeds (minutes),
  every window size from 1 to 256 and faint bits for three seeds in four
  (pulse mode for two in three of those):
  `cargo test --release -p sidou-tests --test app store::power_loss_long -- --ignored`;
  `store::power_loss_sweep` runs one of the long runs' seeds
  (`SIDOU_STORE_SWEEP=values:1000` or `files:10000`), for a sweep of every
  seed in parallel processes;
- **faint bits:** a commit word read programmed at one mount and erased or
  half programmed at the next loses nothing written after it; a faint erase
  header is programmed again before its sector takes records; a faint claim
  word loses nothing written after it; a commit word all faint, read
  committed at one mount and erased at the next, after collections in
  between, leaves the old value or the new, never neither, for a small value
  and for a file's replacement; the interrupted write (the newest record)
  counts at the next mount whatever its commit word reads, is copied forward
  before anything else is written (writes wait, STORE_BUSY), and reads the
  same at every later mount; a header cut short, read erased at one mount
  and right at a later one, never counts once recovery's first record has
  completed; a sector read as killed is erased before
  any write or collection, so a tombstone dropped later never lets a deleted
  value back; a collection's copy cut in its commit word and read erased,
  then a deletion and the collections that drop its tombstone, never bring
  the value back (the third review's C1); a write between an erase and its
  erase header lands safely, nothing programming the fresh sector but its
  header (C2); in the runs, after a real cut the interrupted write may still
  read either way until the next session's first record completes (a cut
  before then), never after; a newest record that overflows a full key index
  is the one left out, wherever it lies; in the power-loss runs, every deep
  fork mounted twice must read the same both times;
- **hook errors:** with 3% of operations failed by the hook, at windows 1 to
  256, nothing breaks a rule and every remount reads each key and file as
  before a failed write or after it, every other one exactly;
- **near capacity across boots:** seven 18,000-byte files and two
  1,248-byte values a boot over 40 keys, 400 boots: no write is refused (the
  reviewer's case, which a fixed half-live rule for collections refused
  10% under capacity), and the same with no boots;
- the pool never under its minimum (a write that would take it there is
  refused with STORE_BUSY), and a quiet refill brings it to its target;
- erase counts within 8 of each other after 10,000 erases, cold data
  included;
- a full key index refuses cleanly, and a deletion makes room after the
  collections; an open file's fresh key is kept for it;
- foreign data mounts foreign and nothing is written until a format; a
  newer format version is foreign too; a cut during a blank region's first
  erase can mount foreign;
- a format 1 fixture (`tests/data/store-v1.bin`) mounts to the same values
  (`SIDOU_STORE_FIXTURE_UPDATE=1` writes it, under format 1 only);
- random regions, and real ones with random bits flipped, mount without a
  crash (run them under the sanitizers) and never make the store break a
  rule above;
- the bytes on flash are the documented format;
- **bounds:** a mount's polls check at most a slice (`STORE_MOUNT_SLICE`, or
  one record alone) on blank, random, foreign, fixture and full regions; a
  page takes at most `STORE_PAGE_PROGS` (34) programs between erases at
  windows of 16 or more, with the densest records there are, and one more
  for each cut that falls in the page (also checked at the end of every
  power-loss run); every API call outside `store_poll` within
  `STORE_POLL_WORK`, worst cases included (a full key index, a short pool);
- **files:** a cut at every operation of file writes, replacements,
  deletions, renames, aborts and short ends (some while a sector's claim is
  half programmed) among small writes, collections and refills leaves each
  file its old bytes or its new ones, whole (its CRC checked), never neither
  and never a mix, and a rename never half done; every 32nd fork is mounted
  again and must read the same; real cuts over long runs with files up to
  64 KiB (the long version: `store::files_power_loss_long -- --ignored`); an
  abort or a short end mid-claim, at windows of 1, 4, 8 and 16, leaves the
  sector to be erased, never claimed over; a file of exactly 64 KiB in 18
  sectors; `store_file_begin` claims `ceil(length / 3744)` sectors (and a
  head for the FILE record if the head has no room) and writes nothing; no
  erase between a file's begin and its end; the pool short gives STORE_BUSY
  with nothing written, and the same begin succeeds after a refill; a file
  larger than the free space is refused before anything is written, and a
  replacement is checked against the space the old file frees (three
  near-64 KiB songs, one re-saved at 64 KiB); reads at every offset and
  length; the directory in name order; `store_file_write`'s back-pressure
  with offers of every size from 1 to 1,000 bytes; a replacement leaves the
  old file until the new commits; aborted and short files leave nothing;
  small writes go on while a file is open; an open file whose sender goes
  quiet for `STORE_FILE_IDLE_MS` is aborted (not while the store is the slow
  one), and a format ends an open file.

### Package tests

`crates/sidou-pack/tests/`:
- `ota_step1.rs` runs the firmware's step 1 of the update protocol
  (Felucca's `ota.c`, built for the host with test hooks) against packages
  from `sidou-pack`, the test acting as the installer (`fm1_install.py`'s
  side), on a simulated NOR flash: the identity request is answered with
  `FM-1_900`; a package's loader is staged at `0xE0000` with its head, the
  update record is written and committed after the host's "success", and
  nothing else is written; another board's package (flash.bin of another
  size) and a loader without the marker are refused before any erase; the
  next boot clears the staging area. It needs no downloads (a synthetic
  device head carries the chip key). With M-VAVE's V15 package in
  `build/stock/` (SHA-256 checked; read by the tests only, never packed), a
  device with V15's head takes SIDOU's package without its head being
  written, and the way back works: V15's own package passes `ota.c`'s
  head-identity check and is staged, while a device whose head had been
  changed would refuse it before any erase. fm1-emu has no USB, so this is
  where the protocol and the package format meet before hardware. With the
  fm1 build there, the loader it built (Felucca's) is staged too.
- `flash_hooks.rs` runs the firmware's flash hooks for the update protocol
  (`platform/fm1/ota_flash.h`) with stand-ins for the flash driver, the
  interrupt mask, the audio and the LEDs: erases and writes only inside the
  staging area (misaligned, outside or wrapping requests get -8), never
  while the audio runs (-16, counted), the driver always called with
  interrupts masked and the LED rows dark for an erase, reads in 256-byte
  windows; and that the harness's copy of `FL_IN` is the HAL's.
- `flash_spike_guard.rs` runs the flash spike's bounds
  (`platform/fm1/flash_spike_guard.h`) with stand-ins for its driver on a
  1 MiB flash: at every address of the part (and past it, and wrapping),
  sector erases, page erases, programs and the suspend check reach the
  driver only inside the store's last two sectors `[0xDE000, 0xE0000)`,
  programs only inside one page and below each sector's guard band (0xF00),
  and nothing else changes; the RAM driver's own gate in the spike's build
  is the same; a sector is taken only if blank or the spike's own (its mark,
  the band erased), so one programmed byte, a touched band or a store sector
  (`SIDOUSTR`) is foreign; and the round-trip pattern reads absent, intact
  or damaged.
- `pack.rs` reads a package back with fm1-emu's reader (`fm1_soc::fwsc`): the
  head is the SDK-default one with the SDK's SPL and the chip key, the app
  is at `0x02000120` in the unscrambled app area, the directory, the
  container and the loader's image match; and a golden SHA-256 of a fixed
  tiny package, which Felucca's `fm1pkg_make.py` reproduced byte for byte
  once.

`crates/emu-gate/tests/fx_live.rs` plays the effect slots through the firmware:
- the panel's EDIT layer turns OVERDRIVE on: on a chord, the wave clips (its
  crest factor falls below 0.8 of the same chord through the slot at no
  drive) and EDIT lights up;
- TRS MIDI CCs turn the light reverb on: the output turns stereo and a tail
  follows the note;
- every slot's heaviest (overdrive, chorus, lush) switched in by CC while a
  chord plays: every audio block, switching included, stays within the
  2,700-per-sample audio target; the interrupt runs only from `.ram_text`;
  the main loop stays under 5M instructions/s.

`boot.rs`'s host reference goes through the same slots (all off at boot), and
compares both channels sample for sample.

`crates/emu-gate/tests/package.rs` boots `sidou.fwsc` as fm1-emu boots a
package (`stock_from_package` + `stock_boot`) with the A4 key held: it plays
440 Hz, sample for sample as the bare image does. It writes the audio as
`sidou_package.wav` into the fm1 build directory. A `sidou.fwsc` older than the
build's `sidou_app.bin` fails it, with a message to run `sidou-pack` again: it
holds an earlier build.

The hardware checklist, for a real FM-1, is
[docs/hardware-test.md](docs/hardware-test.md).

### Boot-guard test

`crates/emu-gate/tests/bootguard.rs` checks the boot-loop guard
(`app/bootguard.c`, `boot_guard` in `platform/fm1/main.c`). fm1-emu does
not model a reset, so each reset is a fresh machine that keeps the guard's
`.noinit` words, and a boot dies by being cut off after 600 ms:
- after two such boots the third enters UBOOT: the boot-state mailbox holds
  `usb_update_mode`, the guard is cleared, and the audio never starts;
- a boot that runs past `BOOTGUARD_STABLE_MS` (10 s) clears an earlier
  death.

The guard's rules are host-tested too (`tests/app/bootguard.rs`).

### Flash spike test

`crates/emu-gate/tests/flash_spike.rs` covers the flash timing spike
(`platform/fm1/flash_spike.c`, M2a Task 3b), a test firmware for the user's
FM-1 ([docs/hardware-test.md](docs/hardware-test.md), section 12):
- with `SIDOU_FM1_BUILD`: the default image has no spike symbol in its ELF
  (`flash_spike*`, `*spike_*`: what `nm` lists) and not the spike's sector
  mark in its bytes;
- with `SIDOU_FM1_SPIKE_BUILD` (the spike's build directory): the spike boots
  to its start screen, starts on SAVE and runs to its result screen with the
  TRS jack busy throughout. Every time is 0 (fm1-emu's flash is
  instantaneous), every read-back check is clean, fm1-emu ignores the page
  erase (0x81), about 3125 TRS bytes arrive in the open second and none is
  lost across the 1 ms and 35 ms masks. Every byte of the flash outside the
  two sectors holds a sentinel an erase or program would change (the update
  staging area blank, as `update_boot` erases one in use), and none
  changes; at the end 0xDF000 is blank, 0xDE000 holds the pattern, and both
  guard bands are erased. A second run on that flash takes its own sectors
  and finds the pattern intact; with that pattern damaged, it refuses. A
  foreign byte in either sector (the band included) makes it refuse too, and
  a refusal writes nothing. It writes its
  results to `flash_spike.json` in the spike's build directory.

### SRAM audio test

Checks of where the audio path runs, in
`crates/emu-gate/tests/ram_audio.rs`:
- the boot copies `.ram_text` (the audio path, `platform/fm1/app.ld`) from
  the image to SRAM: emulated RAM at `_rt_start.._rt_end` equals the image
  at `_rt_load`;
- with a key held, every instruction the ALNK0 interrupt runs at its own
  level (interrupts nested in it aside) lies in `.ram_text`; a failure names
  the functions that ran from flash;
- no direct `call` or `goto` crosses between flash and SRAM code, which
  would be at the edge of the pi32v2's branch reach: the firmware links with
  `--emit-relocs`, and every branch relocation's site and target must lie in
  the same region ([docs/hardware.md](docs/hardware.md), "Code in SRAM").
  A function the audio path calls but `app.ld` leaves in flash fails the
  link before any test runs;
- the audio path reads no data from flash: no address relocation in
  `.ram_text` (`MOV_ABS32`, `ABS32`) points into XIP. A constant table that
  audio code reads must be in `.data` on the device; a failure names the
  function and the table. Data reached through a pointer set up elsewhere
  (the combined waveforms' pulldown rows, in flash by design) is not seen.

fm1-emu does not model the I-cache, so the benefit is a hardware
measurement (docs/hardware.md, open checks).

### Browser demo smoke test

`tools/web-demo --check` builds the browser demo (BUILDING.md, "Browser
demo"), serves it on a free localhost port and runs `web/check.mjs` in
headless Chromium (Playwright, pinned in `web/check.sh` and installed with
its browser into `build/web-check/` on first use). It fails when:
- the page logs a console error, throws, or a request fails;
- the audio thread reports an error, or the emulated CPU halts;
- after Start, 3 s of emulated time do not pass within 60 s;
- the LCD has fewer than 5,000 lit pixels, or the screen at 3 s is the
  title card's (0.5 s) unchanged: the boot did not get past it;
- the canvas's LCD area is nearly black;
- holding a note key (H, A4) does not change the screen, or the output's
  RMS (an analyser on the AudioWorklet's output) does not rise above four
  times its level before the key;
- PLAY (Space, button 12) does not raise the RMS above four times its level
  before (SIDOU's first song), or PLAY again does not bring it down to a
  quarter of the song's within 1.5 s.

The same run covers the Songs page (`web/songs.html`, which sends the showcase songs to an
FM-1 over Web MIDI): it loads the page with a fake `navigator.requestMIDIAccess` holding two
port pairs, "Other Synth" and "SIDOU FM-1", the second backed by a simulated device that
follows docs/reference/sysex.md. It fails when the page sends anything before Connect, sends to
the other port, does not ask for SysEx, does not find the device (INQ and DIR), asks anything but
one replace question (the simulated store has a different IRON RITE), or the PUTs it sent do not
carry the four `.sng` files byte for byte (CRC included).

Before the browser, `web/check.sh` runs `node web/sysex.test.mjs`, which needs no browser: the
page's SysEx encoder and decoder (`web/sysex.js`) against `web/sysex-vectors.txt` (the frames
Rust's `sidou-sysex` also encodes and decodes: its test `web_vectors_match_the_canonical_encoder_and_decoder`
fails when the file and the crate disagree), the packing at every payload length, a PUT's three
frames against Python's reference encoder (`tools/sysex_roundtrip.py`'s packing) byte for byte,
and whole INQ, DIR and multi-packet PUT runs against a simulated device that checks what it
receives (checksum, sequence, 16-byte alignment, exact length, CRC), including BUSY, FULL and
I/O NAKs (messages, no ABORT after a NAK) and a silent device (timeout, then ABORT).

It saves a screenshot with the note held to
`build/web-check/screenshot.png` (`SIDOU_WEB_SCREENSHOT` overrides). The
Pages workflow runs it before every deploy.

## The ergonomics harness (`ergo`)

`host/crates/ergo` drives the firmware in fm1-emu the way a player does, scores it against
explicit ergonomic rules, and writes a report for the user to review. It is not a pass/fail
gate: the emu-gate tests stay separate, and the long run is the binary, never `cargo test`.
It reuses emu-gate's helpers (`tests/common/mod.rs`, included by path) for booting, keys,
encoders and the pot, and reads the screen from the firmware's own cell grid and the live
instrument from its RAM. It never opens, enumerates or sends to a USB or MIDI device.

```
cd host
cargo run --release -p ergo -- --out ../build/ergo                    # everything (a few minutes)
cargo run --release -p ergo -- --out ../build/ergo --baseline ../build/ergo-old/metrics.json
cargo run --release -p ergo -- --only readability,journeys            # some checks
cargo run --release -p ergo -- --only knobs --knob K1.live            # one sweep
cargo run --release -p ergo -- --capture full --journey jam --out ../build/ergo   # a capture
cargo run --release -p ergo -- --print-config                         # the thresholds
```

The fm1 build is `$SIDOU_FM1_BUILD`, else `--build`, else `../build/fm1`. Output, under `--out`:
`report.html` (one self-contained page: the summary first, then the findings, the journey
strips, the tables), `findings.json` (every finding with a stable id, for a review to record
accept or reject per id), `journeys.json` (each journey's steps, screenshot and clip paths and
metrics, for an automatic review), `metrics.json` (the numbers `--baseline` compares), and
the screenshots and clips themselves (`journeys/`, `knobs/`, `defaults/`).

### What is measured, and the rules

Every threshold lives in `crates/ergo/src/config.rs` with its source (psychophysics, a standard,
or judgement) and can be overridden with `--config FILE` (`key = value` lines). A finding's
severity is high, medium, low or info.

| Check | Measured | Rule |
|---|---|---|
| Readability (`READ.size.<view>`, `READ.contrast.<view>`) | each text on each captured screen: its scale (1x, 2x, 3x) from the grid's cell codes, its physical glyph height at 23.4 mm per 240 px (8 px x scale x 0.0975 mm), its colour pair's WCAG contrast from the 16-colour palette | text read while playing (values, notes, names) is at least 1.5 mm (2x); contrast at least 4.5:1 at 1x and 3:1 at 2x and larger. Which text is "read while playing" is the table `tables/readability.txt` (per view, by content: notes, values, names), editable, and it adapts to a redesign because it reads the grid, not coordinates |
| Knob feel (`KNOB.<rule>.<control>.<view>`) | each of K1-K4 (LIVE page, EDIT held, CHIP), MASTER and the CHIP profile encoder swept end to end a detent at a time: the parameter read from the live instrument, the detent count, the step in cents (cutoff, from the firmware's own cutoff curve), dB (level) or % (others), and the spectral centroid and RMS of a held note rendered after each step (for the controls heard where a note starts, LIVE K3's decay on SAW LEAD and CHIP K3's thump, a new note from its first sample after each step) | `range`: a cutoff sweep covers 3 octaves or more; `audible`: the whole travel changes brightness by 600 cents or level by 3 dB; `dead`: no 4 detents leave both brightness (5%) and level (1 dB) unchanged; `fine`: a step is above its just-noticeable difference; `jump`: no step above its limit; `mono`: monotonic; `uneven`: step spread under 0.5; `detents`: 8 to 64 detents end to end |
| Effort per task (`TASK.*`) | the journeys' steps tagged `task NAME`: presses, holds, turns and detents, menu levels (views pushed on the stack, a title card not counted), frames, forced waiting | at most 3 actions and 1 menu level (`task_max_actions`, `task_max_menu_levels`); a task that cannot be done is `TASK.missing` |
| Feedback speed (`FEED.*`) | milliseconds from an input to the first changed pixel and to the value popup, over four phases of the 40 ms frame | the screen answers within 100 ms, the popup within 200 ms |
| Consistency (`CONS.*`) | each of K1-K4, SELECT, ALGORITHM and HOME tried in LIVE, the menu, CHIP, TRACKER, SCENE, STATUS, MIDI and About: did the screen, a parameter or the view change | K1-K4 change a parameter, SELECT moves a focus or cursor (and no parameter), ALGORITHM steps a choice, HOME goes back; a control that does nothing is an exception, listed and compared with `docs/reference/controls.md` (`DOCUMENTED_INERT` in `consistency.rs`); drift between the two is a finding |
| Journeys (`JOUR.*`, `SAFE.*`) | the table `tables/journeys.txt`: pick up and jam, sketch a loop, build a song, perform, keep what you made, explore; per journey the time and actions to the first sound and first loop, mode switches, menu trips, dead ends, steps that take both hands off the keys or leave a note unable to sound, forced waiting | a step the firmware cannot do yet is recorded `not yet possible` (`JOUR.nyp`), not failed; an expectation that does not hold is a dead end (`JOUR.dead`); two hands off the keys is `JOUR.hands`; a step that overwrites edits unasked is `SAFE.destructive`; no undo is `SAFE.undo` |
| Musical defaults (`MUS.*`) | each factory preset's held note (F3 and A4), rendered: RMS, peak, clipped samples, DC, centroid | audible, between -30 and -8 dBFS RMS, no clipping, DC under 0.5%, centroid 150 to 5,000 Hz |

### Editing the tables

`crates/ergo/tables/journeys.txt` is the journeys table (its header documents the grammar:
one line a step with actions, a caption and flags). Add a journey or a step and run again;
`--journeys FILE` runs another table. Steps tagged `task NAME` are the common tasks for the
effort check, and the same journeys are the screens for readability. The `tour` journey only
visits screens for it: LIVE (at rest, with a chord, a popup, a toast, and with the chord
arpeggio's badge), the menu, STATUS, the glyph sheet, About, the MIDI page, CHIP, SCENE, the help
card, the five song views (STEP, TRACKER, ORDER, INSTR, TABLE), SETTINGS and SONG's pages (its
actions, LOAD's files, a confirmation and SAVE AS), so their text size and contrast are measured.
A step names a feature it
`needs` (`theme undo help order_view instr_view pattern_edit new_song song_save`): features are
detected from the firmware (strings, symbols, and for `pattern_edit` a probe of REC in TRACKER),
so a step that main cannot do yet is recorded as not yet possible and starts to run when the
feature lands, with no edit. `tables/readability.txt` classifies the text (see its header).

### Recipes: journeys as executable how-tos

A journey is also a **recipe**: a goal, a starting state, skills taught, related recipes, and
steps that each carry the physical control in panel terms, a caption and the outcome the harness
checks (a view, text on the screen, a parameter in a range, sounding or silent, brighter or
darker, a level, text at 2x or larger). `tables/recipes.txt` holds the example recipes (first
sound, shape a sound with K1 to K4, mute and solo while a song plays) and the per-feature checks
the v0.3.0 device session asked for: `first-run` (a fresh install's LOAD lists something to load,
and PLAY plays it) and `arp-visible` (ARP on shows a badge at 2x, readable at arm's length); `--export-recipes ../docs/recipes` writes them
as JSON, one file each, in the schema of [docs/reference/recipe-schema.md](docs/reference/recipe-schema.md);
`--check-recipes ../docs/recipes` runs every JSON recipe in fm1-emu and exits 1 when one no
longer runs (a step the firmware cannot do) or no longer meets its expectations, naming the
step. `crates/ergo/tests/recipes.rs` checks without the emulator that the committed JSON parses,
lints and is what the table exports, and with `SIDOU_FM1_BUILD` that every recipe passes.

The recipes are also the how-to videos' scripts ([docs/videos.md](docs/videos.md)): a recipe
can put showcase songs into the store (`songs`), start where another ends (`after`), play the
keys in time (`play`), tap STEP's keys (`taps`), turn a knob slowly (`sweep`), wait for the
song's next pattern (`sync`) and check what the song's patterns hold (`notes`, `rows`); a
step's `focus` says what the video lights and where it looks. A video is a recipe that passes.

### Baselines

Each run writes `metrics.json`. `--baseline OLD/metrics.json` lists every metric that got worse
(by more than 5%), every new finding and every finding whose severity rose, in the report and on
the console; `--fail-on-regression` makes that the exit status.

### Ergonomics gate (`tools/ergo-gate`)

```sh
tools/ergo-gate build/fm1            # the merge check's command: the whole harness and the recipes
tools/ergo-gate --quick build/fm1    # readability, journeys (the tour) and the recipes only
```

It runs the harness (`--baseline docs/ergo-baseline/metrics.json`) and `--check-recipes docs/recipes`
side by side, and exits 1 on any **new** finding (above info), any finding whose **severity rose**, or
any failing recipe, printing each one (`NEW id (severity): title`, `ROSE id low -> medium`); exit 2
means the harness could not run. Findings the baseline already has pass: they are accepted, not
fixed. A finding that went away is listed, and the baseline should be refreshed to keep the fix.
Metrics more than 5% worse than the baseline are printed as notes and do not fail. The run's files
(`run/report.html`, `run/findings.json`, `harness.log`, `recipes.log`) go to `build/ergo-gate/` (`--out`).

Time, on a loaded machine (load 30): about 4.5 minutes for the whole gate, because the two halves run
at once. Alone, the harness is about 4 minutes (journeys with the tour and readability 105 s,
consistency 92 s, knobs 35 s, feedback 22 s, defaults 6 s) and the recipes about 2.5 minutes.
`--quick` skips knobs, feedback, consistency and defaults and so takes what the recipes take.

**Accepting a change.** If the new or risen finding is right to keep (a screen that stays 1x for a
reason, a knob whose feel is the intent), refresh the baseline in the same commit and say why in the
message:

```sh
tools/ergo-gate build/fm1       # fails and prints the finding: read it first
cd host && cargo run --release -p ergo -- --out ../build/ergo     # the full run
cp ../build/ergo/metrics.json ../docs/ergo-baseline/metrics.json
git add ../docs/ergo-baseline/metrics.json    # commit it with the change, "ergo baseline: why"
```

A fix that removes findings refreshes it the same way, so the next regression is measured against
the better state. The baseline is refreshed from a full run, never a `--quick` one (that would
drop the other checks' findings from it).

### Reusing captures for the manual and for videos

`--capture full --journey ID` writes `OUT/capture/ID/`:

- `frames/00001.png`, ...: every LCD frame at the UI's 25 fps (one every 40 ms of emulated
  time), 240 x 240, 4-bit indexed in the art palette;
- `audio.wav`: the whole journey's audio, 16-bit stereo at 44,118 Hz;
- `frames.json`: the sidecar: `fps`, each frame's file and `ms` from the first frame, the audio's
  `offset_ms` (the audio's first sample against the first frame, within one audio block of
  2.9 ms) and the step `captions` with their `ms`.

Captions come from the journeys table (the third field of a step): they are the manual's and the
subtitles' text. Nothing reads a clock or a random number, so the same build and the same journey
give the same bytes; the capture changes only when the UI or the sound does
(`tests/capture.rs` runs a short journey twice and compares every byte).

`frames.json` also has the recipe's `steps`: each step's title, caption, `focus`, `chapter`
and `ms`/`end_ms`, and each input with its own `ms` and `end_ms`, so a video knows which control
was used when. `--journeys DIR` captures a JSON recipe of that directory (`docs/recipes`), and a
capture whose step does not run as written is an error rather than a video of it.

`tools/make-videos` turns the recipes into the how-to videos ([docs/videos.md](docs/videos.md)):
it runs the captures (cached under `build/videos/cache`, keyed by the firmware image, the
recipe and the harness), renders the frames with Pillow on every core straight into ffmpeg and
writes `build/videos/ID.mp4` (and the square and vertical cuts), `ID.srt`, `ID.wav` and the
frames' SHA-256. The frames are the same for the same build and recipe; the check is to render
twice and compare `ID.frames.sha256`.

## Sanitizers

The C and C++ code can be built with sanitizers through `SIDOU_SANITIZE`:

```sh
# UBSan in trap mode: no runtime needed, works on stable Rust and macOS.
SIDOU_SANITIZE=undefined cargo test --workspace

# ASan: needs nightly Rust, which provides the ASan runtime. CI runs it on Linux.
SIDOU_SANITIZE=address RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly test --workspace -Zbuild-std --target x86_64-unknown-linux-gnu
```

Under UBSan, undefined behaviour stops the test with a trap (SIGILL or
SIGTRAP) rather than a report. Re-run the failing test in a debugger to find the
line.

## Continuous integration

`.github/workflows/ci.yml` runs only on a release tag (`v*`) or when started by
hand from the Actions tab, never on ordinary pushes or pull requests. It is deliberately
small (2026-10-23): the in-depth testing happens before a change merges, where every merge
passes the full local check (both firmware builds, fmt, clippy, the workspace tests, UBSan,
the GCC check, the packager and its tests, the whole emulator gate, REUSE, the browser demo).
CI adds only what the local check cannot: the release package built clean from the tag on
Linux, and booted in fm1-emu.

### What runs when (no run that tests nothing new)

The merge check looks at what a change touches against main and runs only what that can affect
(the user, 2026-10-23: no endless test runs). Every result is the same as the full suite's
would be; nothing is sampled.

| The change touches | The merge check runs | About |
|---|---|---|
| only docs: `*.md`, `docs/`, `LICENSES/`, `.github/` (workflows run on GitHub, not here) | REUSE and `tools/readme-check` | seconds |
| docs and CMakeLists.txt's version line, nothing else (a release bump) | the `fm1` build, the package, the package boot test in fm1-emu, the release-only emulator tests the everyday profile leaves out (`--ignore-default-filter -E 'not default()'`: today the ten-minute clock-drift test), REUSE, `tools/readme-check` | 3 to 5 min |
| anything else | everything; the emulator gate is skipped when the same firmware, package and test inputs already passed it (fm1-emu is deterministic) | 15 to 20 min |

**A release** therefore costs one full run, on the change that brings the code, and no more:

1. Merge the last code change (full check).
2. The release commit: CMakeLists.txt's version, CHANGELOG.md, README.md's status (which
   `tools/readme-check` holds to the CHANGELOG), `docs/release-notes/vX.Y.Z.md`. Version-only
   check, 3 to 5 minutes: it also runs the release-only tests (`host/.config/nextest.toml`'s
   release profile), so nothing the everyday check leaves out goes untested into a release.
3. Build the release package (`-DSIDOU_PRODUCT=FM-1_964`), `tools/preflash` it, and run it on a
   real unit (an install over the previous release and over Felucca at least).
4. Tag `vX.Y.Z`. CI builds the package clean from the tag and boots it; publish it once its
   SHA-256 matches the package tested on the unit.

Do not run the full suite again before tagging: the code it would test has already passed it. `.github/workflows/pages.yml` has
the same triggers; it builds, smoke-tests and deploys the browser demo.

CI is one job, `fm1`, that runs in the prebuilt build image `ghcr.io/dsablic/sidou-build`
(pinned by digest; BUILDING.md, "The build image"), so nothing is installed during a run:

| Step | What it runs |
|---|---|
| build | the `fm1` build, clean from the tag (`tools/fm1-docker` sees it is inside the image and runs CMake directly) |
| package | `sidou-pack` makes `sidou.fwsc` (its inputs cached by `inputs.toml`'s hash) |
| boot | the package boots in fm1-emu (`emu-gate --test package`), and a check that its report exists |
| result | the package's SHA-256 in the run summary; the package is uploaded with `LICENSES/Apache-2.0.txt` and `LICENSES/GPL-3.0-only.txt` as `sidou-fwsc`: the release's file, published once its SHA-256 matches the locally tested package |

**The GCC check** (`tools/gcc-check`) used to be a CI job. It compiles every C file the host
tests build (the `SOURCES` of `sidou-sys/build.rs`, the gt2-ref shim, the packager's harnesses)
with GCC 13 in the build image, with the cc crate's release flags and warnings as errors, as
macOS's clang hides some errors GCC reports. It takes under a minute and is a stage of the
merge check; run it by hand after changing C.

The ASan run (nightly Rust, above) and the MSRV check (`cargo +1.88 check --workspace
--all-targets`) are not in CI; run them by hand when the C/C++ code or the Rust
version requirement changes.

Before opening a pull request, run at least:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --release
```

and `reuse lint` (or `python3 -m reuse lint`) from the repository root.
