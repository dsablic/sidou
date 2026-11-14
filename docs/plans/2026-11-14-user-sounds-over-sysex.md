<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Reading USER sounds over SysEx: design and plan

1.10's second item ([roadmap](../ROADMAP.md)): the site and `sidou-sysex` read the sixteen USER
sounds LAB kept and open each as a `sidou-patch/1` patch. Agreed with the owner on 2026-11-14:
**read only**.

## Design

- **Firmware.** GET (target 0) of the name `USER SOUNDS` returns the store file
  ([sound-format.md](../reference/sound-format.md#the-user-sounds), type 4, 1,955 bytes today). DIR
  still omits it; PUT and DEL still refuse it. `reserved()` in `app/sysex.c` becomes a check per
  command, GET excepting this one name. An FM-1 with no file answers NAK NOT FOUND. Its flash cost
  is measured and reported; the image's ceiling is not raised.
- **Reader.** One C function in `platform/web` (beside `patch_json.c`) that takes the file's bytes
  and gives, per slot, used, the name and the sound, reading older files (45, 48, 68 sound bytes)
  with the missing bytes at their defaults as the firmware does. Prefer calling the firmware's
  own decoding (`app/user_sound.c`, `app/lab_sound.*`) over a copy, if that does not grow the
  firmware; a used slot becomes a patch through `patch_json_write`.
- **Site.** The Library page gets a USER sounds button by the FM-1's file list: it GETs the file,
  lists U01..U16 by name (empty slots greyed), and a click opens the sound as a patch with the
  patch's actions (listen, link, edit as text, into a bank or song). No file: "No USER sounds yet:
  keep one with SAVE in LAB."
- **CLI.** `sidou-sysex user-sounds [--out DIR]` reads the file from the FM-1 and writes one
  `.json` patch per used slot (`U01-NAME.json`); `--file PATH` reads a saved file instead. Tested
  against the fake FM-1 only.

## Plan (test first, one commit per step)

1. **Emulator test first** (`host/crates/emu-gate/tests/sysex.rs`): keep two sounds in LAB (as the
   LAB tests do), then over USB a GET of `USER SOUNDS` returns the store file byte for byte; DIR
   does not list it; a PUT and a DEL of the name are refused and change nothing; with no file, GET
   is NOT FOUND. It fails; then change `app/sysex.c` and pass. Report the image size before and
   after.
2. **Reader** with host tests (`host/crates/sidou-patch/tests/`): every slot of a file the firmware
   wrote reads back to the sound and name LAB kept; files in the 1.0, 1.1-1.5 and 1.6 sizes read
   with defaults as `user_sound.c` reads them; damaged headers and slots read as empty. Rust
   wrapper in `sidou-patch`.
3. **CLI** `user-sounds` with tests against the fake FM-1 (`host/crates/sidou-sysex/tests/`).
4. **Site:** `platform/web/patch_api.c` export, `web/patch.js`, `web/library.js` and
   `library.html`, the simulated FM-1 (`web/fake-fm1.mjs`) serving the file, tests in
   `web/library.test.mjs` and `web/patch.test.mjs`; the patch wasm within 88 KiB.
5. **Docs:** `docs/reference/sysex.md` (GET's one exception), `sound-format.md`, the Library's
   help text, `docs/design-notes.md`, TESTING.md's rows, README, `web/index.html`, `fmhub.json`
   if it fits, CHANGELOG `[Unreleased]`, the roadmap item removed.
6. `tools/merge-check` (the controller runs it).
