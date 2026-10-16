<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# GoatTracker 2.77, vendored for the song player's reference (test-only)

**Test-only.** These files are built by `host/crates/gt2-ref` for the register-match tests
(M2c; decision record [0007](../../docs/decisions/0007-gt2-song-format.md)) and are never
built into the firmware.

- **Source:** GoatTracker 2.77, the official release,
  <https://sourceforge.net/projects/goattracker2/files/GoatTracker%202/2.77/GoatTracker_2.77.zip>
  (fetched 2026-10-16).
- **SHA-256 of the zip:** `96c2bd6a6ab3aca2f5bb18b1c764ac6ea69ac245cae14002a72cd87c554561ef`.
- **Local changes:** none. Every file is byte-identical to the release's (CRLF line endings
  kept; `.gitattributes` marks the tree `-text`), which `gt2-ref`'s `assembler` test
  checks against a fresh download.
- **Licences** (`REUSE.toml`):
  - the C sources and headers, `readme.txt`: GPL-2.0-or-later, © Lasse Öörni and
    GoatTracker's contributors (the readme's header names them); `copying` is the GPL's
    text;
  - `src/player.s`, `src/altplayer.s`: their own permissive notice ("This playroutine
    source code does not fall under the GPL license! Use it, or song binaries created from
    it freely for any purpose, commercial or noncommercial."),
    [LicenseRef-GoatTracker-Playroutine](../../LICENSES/LicenseRef-GoatTracker-Playroutine.txt),
    © Lasse Öörni;
  - `examples/*.sng`: the example songs GoatTracker ships, distributed with it under its
    GPL-2.0-or-later; their authors are in each song's header and in
    `examples/goatcompo.txt`.

## The files

| File | What the harness does with it |
|---|---|
| `src/gt2reloc.c` | the command-line packer: its globals and option defaults; its `main` is compiled as `gt2reloc_main` and never called; it includes `greloc.c` |
| `src/greloc.c` | the packer/relocator (`relocator()`) |
| `src/gsong.c` | the song model: `loadsong`, `savesong`, `clearsong`, `countpatternlengths` |
| `src/gtable.c`, `src/ginstr.c`, `src/gorder.c`, `src/gpattern.c` | the table and instrument helpers the song code calls (`gettablelen`, `makespeedtable`, `clearinstr`, `songchange`, ...); their editor code compiles but never runs |
| `src/gplay.c` | the editor's C playroutine: its channel state and frequency tables, which `clearsong` and the packer use |
| `src/*.h` | GoatTracker's headers, as those files include them |
| `src/player.s`, `src/altplayer.s` | the 6502 player the packer assembles (`altplayer.s` for a hard restart ADSR of `$F000` and up) |
| `readme.txt`, `copying` | the documentation the format and the player are specified by, and the licence |
| `examples/*.sng`, `examples/goatcompo.txt` | GoatTracker's 14 example songs, and their credits |

## Not taken in

- **`src/asm/`**, the 6502 cross-assembler from Exomizer 2 beta by Magnus Lind, which the
  packer calls. Its licence allows use "for any non-commercial, non-profit purpose" only:
  not free software, and not compatible with SIDOU's GPL-3.0-or-later. SIDOU assembles
  `player.s` with its own assembler instead (`host/crates/gt2-ref/shim/gt2ref_asm.c`,
  written for SIDOU from the dialect's behaviour). The `assembler` test
  (`cargo test --release -p gt2-ref --test assembler -- --ignored`) fetches the release
  into `build/goattracker2-2.77/` (never committed), builds the packer once with Exomizer's
  assembler and once with SIDOU's, and compares them on every example song and 30 option
  sets: 350 packs byte-identical, the 70 the original refuses refused by both (2026-10-16).
  Two quirks of the original are kept, so the bytes match: LDY's absolute indexed form is
  written `,y` and assembles as `$BC` (LDY abs,X), which `player.s` relies on; BVS
  assembles as `$B0` (BCS), which `player.s` never uses.
- `src/bme/` (Lasse Öörni's SDL-based library), the editor's display, sound and SID code,
  reSID and reSID-fp: the harness needs none of them.

## The shim (`host/crates/gt2-ref/shim/`, GPL-3.0-or-later, written for SIDOU)

| File | Stands in for |
|---|---|
| `bme.h` | BME's `bme.h`: declares the BME calls the song code makes (`fread8`, `fwrite8`, `fwritele16`, `io_open`, `io_lseek`, `io_read8`, `io_close`, `io_openlinkeddatafile`, `win_quitted`) and gives the editor's `KEY_*` codes distinct values |
| `membuf.h`, `parse.h` | the assembler's interface `greloc.c` includes: `struct membuf` and its calls, `assemble()` |
| `gt2ref_shim.c` | those calls; files in memory (`fopen` renamed to `gt2ref_fopen`: the song to read, the files written); `exit` renamed to `gt2ref_exit` (the relocator exits on an assembly error; the harness returns instead); the player sources read from this directory; stubs for the editor's `datafile`, `key`, `rawkey`, `shiftpressed`, `sidreg`, `editstring`, `resettime`, `incrementtime`, `sound_suspend`, `sound_flush` |
| `gt2ref_asm.c` | Exomizer's assembler (above) |
| `gt2ref.c` | `gt2reloc`'s `main`: its option defaults, checks and calls, with the options as values (`gt2ref_pack`); `loadsong` then `savesong` (`gt2ref_canonical`) |

GoatTracker's files are compiled unmodified with `-Dfopen=gt2ref_fopen -Dexit=gt2ref_exit
-Dmain=gt2reloc_main` and their warnings silenced (`host/crates/gt2-ref/build.rs`).

**Found while building it** (0007 records what matters for the player): `gt2reloc.c`'s
`case 'I':` has no `break`, so `-I` also sets or clears full buffering (the harness sets
the options directly); NTSC (`-N`) changes only a `.sid` file's header and speed code, so a
packed `.prg` is the same for PAL and NTSC; fine vibrato (`-V`) is read after the song is
loaded, so it changes nothing in `gt2reloc` (only the editor's GTS2 conversion uses it).
