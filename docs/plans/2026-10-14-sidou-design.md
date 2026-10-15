# SIDOU — design

Status: draft, agreed in brainstorming on 2026-10-14.

SIDOU is a custom firmware for the M-VAVE FM-1 that turns it into a C64 SID
instrument: a step sequencer with a tracker underneath, a live-play mode, a
`.sid` player, and importers for GoatTracker and SID-Wizard songs. It aims to be
equal parts nostalgia and a serious SID synth that is easy and fun to use.

## 1. Goals and non-goals

Goals

- Sound: a SID model that matches reSIDfp's sound by default, plus a "chip
  character" layer (named chip profiles, age/heat, DAC sloppiness, output thump)
  and post-chip colour (tape saturation, wow and flutter, CRT/TV low-pass, hum,
  bit-crush). All of these can be changed live.
- Two SIDs: chip A plays the song, chip B is for live playing on top of it.
- Workflow: a step view by default, a hex tracker view one button away, a live
  view with loop recording, and a chip picker. All four edit the same song.
- Content: a starter pack of cleared tunes, `.sid` (PSID) playback with
  per-voice mute and solo, GoatTracker `.sng` and SID-Wizard import, and a
  "rip" that turns a playing `.sid` into an editable song.
- MIDI as a first-class citizen: everything on the panel is reachable over
  MIDI, the sequencer syncs to external clock, and each SID voice can be
  played as its own instrument from a DAW (section 16).
- Showcase: the same core runs in the browser as a demo and companion app.
- Built to last: tested, documented, properly licensed, and able to handle a
  stream of user requests and releases.

Non-goals (for now)

- RSID tunes (these need full CIA/VIC timing), digi/sample playback tricks.
- More than two chips, or one chip per note.
- Wireless in the published firmware. BLE MIDI and Wi-Fi are only an
  optional, user-built variant (section 17).
- A new circuit-level 6581 model. It may come later as one more chip profile.

## 2. Hardware constraints (from the reference repos)

The full reference, with datasheet links, the inferred AC7916A package and
the open hardware checks, is in `docs/hardware.md`.

| Item | Fact | Source |
|---|---|---|
| SoC | JieLi AC791N, 2 × pi32v2, ~240 MHz (a 320 MHz variant exists) | fm1-emu README, Melodee CI |
| RAM | ~72 KiB RAM + 360 KiB pool + 24 KiB RAM-text | Felucca `app.ld` |
| Flash | 1 MiB SPI NOR; app slot ~568 KB; raw sectors, no filesystem | Felucca `fm1pkg_make.py`, `storage.c` |
| Audio | I2S codec, 24-bit stereo, ~44.1 kHz (44117.6 measured), 128-frame half buffers | Felucca `fm1_audio.h`, fm1-sid |
| Screen | 240×240 RGB565 over SPI+DMA, no full framebuffer | Felucca `lcd.c`, `gfx.c` |
| Controls | 27 note keys; 14 buttons (FX SCL ENV LFO EDIT GLO HOME SAVE ARP SEQ PLAY REC OCT- OCT+); encoders SELECT, ALGORITHM, PRESETS, KNOB1–4; MASTER pot; per-column LEDs | Felucca `panel.c` |
| MIDI | USB class-compliant + TRS via UART | Felucca `usb.c`, `midi_uart.c` |
| Core 1 | Usable, but **some units misread RAM on core 1**; Felucca ships it off by default | Felucca `fm1_cpu1.h` |

What this means for the design:

- **reSIDfp will not run on the device.** fm1-sid measured reSID 0.16 in fast
  mode at ~440 instructions per sample (~8 % of a core). The higher-quality
  reSID modes and reSIDfp do not fit. So we write our own SID core designed
  for this chip, and use reSIDfp on the computer as the reference we test
  against (section 9).
- **Everything must work on one core.** Core 1 is an optional speed-up (it runs
  chip B and the effects when the self-test passes), never something the
  firmware depends on.
- No full framebuffer: the UI draws partial, dirty-region updates, like
  Felucca.

## 3. Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ platform/fm1     HAL: keys, encoders, LEDs, LCD, I2S, MIDI, │
│                  flash, core-1 mailbox, boot. Proven        │
│                  register-level code from Felucca/fm1_open; │
│                  rewritten where it is slow or tangled.     │
│ platform/host    WAV/CLI renderer, test harness             │
│ platform/web     WASM build + FM-1-shaped browser UI        │
├─────────────────────────────────────────────────────────────┤
│ app/             views (STEP, TRACKER, LIVE, CHIP), browser,│
│                  sequencer clock, live recorder, MIDI map,  │
│                  song store                                 │
├─────────────────────────────────────────────────────────────┤
│ core/            no platform code, no allocation after init │
│   sid/           SID model + character layer + post-chip FX │
│   cpu6502/       6502 + minimal C64 memory map (PSID)       │
│   player/        song engine (GT2-style tables & patterns)  │
│   import/        psid, gt2 .sng, SID-Wizard, register rip   │
│   format/        song file format, versioned + migrations   │
│   midi/          parser, router, clock follower, SysEx      │
└─────────────────────────────────────────────────────────────┘
```

Rules

- `core/` and `app/` are portable C (C11), with no dynamic allocation after
  init and fixed-size pools. They build unchanged for firmware, host and WASM.
- The HAL is a narrow C interface (`platform.h`). Anything that touches
  registers lives only in `platform/fm1`.
- We take inspiration from existing projects but are not tied to them. Code is
  reused only when it is good: correct, fast and readable. Otherwise we write
  it again, using the existing project as the spec and as a reference to test
  against.

### Threading and data flow

- **Audio callback** (I2S DMA interrupt, 128 frames):
  - runs chip A and chip B, then the post-chip effects, then mixes;
  - in `.sid` mode, the 6502 runs here too, for exactly the number of cycles in
    the block, so its register writes land on the right samples.
- **Main loop**: UI, screen drawing, encoders and keys, MIDI parsing and
  routing, the sequencer clock, and flash input/output. MIDI bytes are
  timestamped where they arrive (UART DMA, USB poll), so parsing in the main
  loop does not add timing jitter (section 16).
- **Between them**: a single-producer single-consumer ring buffer of
  time-stamped events (`{sample_offset, chip, reg, value}` plus
  character-parameter events). Parameter changes are smoothed on the audio side.
- **Core 1 (optional)**: renders chip B and the effects through the mailbox.
  It is enabled per device after a boot-time RAM self-test passes, and
  otherwise everything falls back to core 0.

## 4. Sound engine

### SID core

- Runs at the sample rate, with an internal block size of 128.
  - The oscillators accumulate at the chip's clock rate, so the 6502 timing
    and pitches match a PAL or NTSC C64 exactly.
  - The saw and pulse edges are band-limited (polyBLEP or minBLEP) so they
    don't alias at 44.1 kHz.
- The envelope generator is exact, including the ADSR delay bug, which
  trackers rely on for "hard restart".
- Combined waveforms use lookup tables sampled from real 6581 and 8580 chips
  (the same data reSID uses, with attribution).
- Filter: a state-variable filter with a separate cutoff curve for each
  profile. On the 6581 it adds a nonlinear stage built from tables, which we
  precompute from reSIDfp's model on the computer.
  - The filter runs at 2× oversampling only if the CPU budget allows (decided
    by the benchmark in section 8).
- Output stage: the voice DACs (including the 6581's non-linear DAC), the
  volume-register DAC (so "digi" volume clicks are audible), and DC offset.

### Character layer (amp-modeller style)

The SID's analogue model is itself under the player's control. Its
parameters are live, continuous controls:
- **filter:** curves and range, resonance mapping, 6581 non-linearity, DC
  bias and bleed;
- **DACs:** kink and leakage;
- **waveforms:** combined-waveform strength;
- **output stage:** voice DC and the external filter.

The tables behind them are generated over a parameter grid from reSIDfp and
interpolated on the device. Named chip profiles are presets of these
parameters, and the macro controls below move several at once.

- **Chip profiles:** 6581 R2, 6581 R3, 6581 R4AR, 8580 R5, and a "worn 6581".
  Each profile is a small set of curves, generated offline by `tools/` from
  reSIDfp settings or measurements and baked into the firmware.
- **In-chip controls:** age/heat (drift of the filter curve and distortion
  amount), DAC sloppiness, output thump, filter bleed.
  - Changing these live means blending between neighbouring precomputed
    tables. Nothing is rebuilt on the audio thread.
- **Post-chip effects:** tape saturation, wow and flutter, noise floor,
  CRT/TV low-pass and hum, bit-crush. The order is fixed and the cost is
  bounded.
- All controls can be:
  - moved live from a knob;
  - recorded as automation;
  - set per step as a parameter lock.

### Accuracy target

- With the character controls at zero, each profile renders the
  reference-suite tunes within a set error limit of reSIDfp's output for the
  same chip model.
  - The limit is a spectral and RMS error, decided once the first core exists.
- The sequence of register writes must match exactly (see testing).

## 5. Song model and playback

- **Native format.** It is close to GoatTracker 2, so GT2 import is lossless:
  - instruments: ADSR, waveform table, pulse table, filter table, speed table,
    vibrato, hard restart;
  - patterns: rows of note / instrument / command / data;
  - an order list for each voice.
- **SIDOU additions:**
  - per-step parameter locks for character and filter parameters;
  - chip B tracks;
  - per-song chip profile and character settings.
- **Storage:** chunked and versioned (`SIDO` header, then tagged chunks).
  - Readers skip chunks they don't recognise.
  - Every format change gets a migration and a fixture test, so files from
    older versions keep loading as the project grows.
- **Amended (M2 plan, 2026-10-15): GoatTracker 2's `.sng` is the song file.**
  The user decided that SIDOU reads and writes GT2 `.sng` (GTS5) directly
  instead of a format "close to GT2" with an importer: the GT2 part is
  lossless by construction, and songs move freely between SIDOU and
  GoatTracker. SIDOU's additions above (locks, chip B's setup, the chip
  profile and character, the effect slots, and the playroutine options GT2
  keeps outside the file) go in a chunked, versioned trailer after the last
  pattern, which GoatTracker's loader ignores; readers still skip unknown
  chunks, and the migration-and-fixture rule applies to the trailer. Decision
  record 0007 and `docs/reference/song-format.md` (M2c Task 1) have the
  details and exactly what round-trips.
- **Amended (M2 plan, 2026-10-15): where the player runs.** The native
  player is C and runs in the audio interrupt at sample-exact frame
  boundaries (the block is split at the frame), as the 6502 will in M3, so a
  song renders on the device bit for bit as on the host. Main-loop code
  talks to it through a command ring and hears its rows and notes through an
  event ring (M2c Task 6).
- **Playback modes:**
  1. *Native song*: our player engine writes chip registers once per frame
     (50 Hz PAL, or multispeed).
  2. *`.sid` file*: the PSID player code runs in the 6502. The voice
     mute/solo and the chip profile still apply, because they act on the
     emulated chip.
- **Live view:** chip B, a chosen instrument, and the keys.
  - The knobs control cutoff, resonance, pulse width and two character
    controls you assign.
  - REC records notes and knob moves into the current pattern, quantised as
    you choose.

## 6. Import pipeline

| Source | Method | Fidelity |
|---|---|---|
| GoatTracker 2 `.sng` | Parse straight into the native format | Lossless (target) |
| SID-Wizard `.swm` | Parse and convert to the native format. Unsupported features are reported. | High; phase 2 |
| PSID `.sid` | Play in the 6502 | Exact (bounded by the SID model) |
| PSID → song ("RIP") | Log register writes per frame, then: detect notes from frequency; group ADSR, waveform and pulse settings into instruments; turn waveform/pulse/filter sweeps into tables; split into patterns | Approximate. Shown as "ripped". The original `.sid` stays linked so you can A/B compare. |

- **Amended (M2 plan, 2026-10-15):** the GT2 row is no longer an import:
  `.sng` is SIDOU's own song file (section 5). SID-Wizard stays in M5, as
  section 14 has it.
- Every importer is a parser of untrusted input. Each one is fuzzed (section
  9) and fails with a clear message on the screen, never a crash.
- `.sid` support is PSID v1–v4 only. RSID files, and tunes that need CIA
  timing beyond the basic timer, are detected and rejected politely.

## 7. User interface

- **Views** (SHIFT + view key; exact buttons to be decided against the
  `panel.c` layout):
  - **STEP**: 16 keys are steps. Hold a step and turn a knob to set a
    parameter lock. Turn the encoder to page through a 64-step pattern. Three
    voice lanes, plus a lane for chip B.
  - **TRACKER**: a hex grid with three columns and GT2-style commands.
    - **Amended (M2 plan, 2026-10-15):** by default the rows stand still and
      a bar moves down them, a page at a time, because GoatTracker's
      scrolling changes most of the screen every row (about 725 cells, more
      than the SPI budget at fast tempos, as the prototype's two modes
      showed); scrolling is an option (M2d Task 2).
  - **LIVE**: keys play an instrument; the knobs control filter, pulse width
    and character; REC records loops.
  - **CHIP**: profile picker, character controls and post-chip effects, set
    separately for chip A and chip B.
    - **Amended (M2 plan, 2026-10-15):** the effect slots stay one chain on
      the mix of both chips. A second chain's delay and reverb pool (128 KiB)
      does not fit the SRAM next to chip B and a song (M2 overview, RAM).
      Profiles and character are per chip.
- **Always available:** mute and solo per voice; play and stop; the browser.
- **Themes:** a C64 nostalgia theme (PETSCII-style font, C64 palette, border
  colour flashes on screen) and a clean modern theme.
- **Browser:** starter pack, user songs, user `.sid` files. RIP option.
- **SCENE view** (performance, M2): generative PETSCII-grid scenes (see
  "Look and feel") rather than drawn pixel art. Small scenes that animate in
  time with the music, like the Pocket Operator screens. There are no game
  mechanics.
  - **Voice gates** trigger a character or element per voice, which stops when
    the voice is muted.
  - **Pitch** sets height or position.
  - **Filter cutoff and resonance** set the mood (light, fog, glow).
  - **Tempo and step** loop the scene with the pattern.
  - **Chip B** has its own soloist.
  - **Chip character** controls show up visually (scanlines, colour bleed,
    wobble).

  Scenes use only original art evoking C64 culture, such as a demo-scene
  raster stage, a cassette deck, or a bedroom coder whose screen shows the
  live pattern. No characters, sprites or names from existing games.

  - **Amended (M2 plan, 2026-10-15):** besides the generative PETSCII
    pieces (M1e), SCENE gets drawn pixel-art vignettes in late-C64 style,
    homages to game genres with original characters: the first is RED MOON
    RITE (a barbarian duel under a blood moon, driven by the drums, bass,
    arp, fill and cutoff), the second, optional, DUSK DUEL (a karate duel at
    sunset). Their art is drawn by code (the prototype's painter and puppet),
    exported to indexed PNGs and converted to run-length sprites; the device
    composites changed tiles on demand, with no frame buffer (M2f). The
    rule above holds: no game's characters, sprites, layouts or names, and no
    gore; each vignette gets an originality review.

  Scenes run on the UI core from the same timed events as the audio, so they
  stay in sync and never touch the audio budget. They need a few KB of sprite
  data each. Scene footage also goes into the release videos.

### Look and feel

Fun is a requirement. SIDOU should feel like an 80s machine and play like
an instrument.

- **Generative, not illustrated.** Visuals are computed character-grid
  pieces, not static images or generated artwork. The 240×240 LCD with an
  8×8 font is a 30×30 cell grid, the same medium as the C64's 40×25 PETSCII
  screen. Pieces are small algorithms in C, such as:
  - Doom fire, plasma and starfields;
  - Rule 30 and Langton's ant;
  - reaction-diffusion and a Lorenz attractor;
  - raster bars, scrollers and split-flap type.

  They react to the music: voice gates step automata, envelopes heat the
  fire, cutoff drives plasma speed. Only changed cells are sent over SPI,
  which suits the LCD bandwidth.

  Inspiration: [ascii.rest](https://ascii.rest/#pieces) (236 animated
  text-grid pieces, MIT). We write our own C and credit any piece we adapt.
- **Art direction:** a C64/80s look with our own 8×8 PETSCII-style font,
  the C64 palette, PETSCII frames and icons, and a blinking block cursor.
  An art direction page (`docs/art/`) fixes the palette, font, layout grid
  and motion rules before screens multiply.
- **Boot:** `LOAD"SIDOU",8,1` → `SEARCHING…` → `READY.` with raster-bar
  border flashes. It can be skipped and is short on later boots.
- **Menus** in C64 BASIC/KERNAL style. An "About" page works as a
  cracktro: sine scroller, starfield, credits, music on the SID.
- **Visualisations in the play views:**
  - per-voice oscilloscopes coloured by waveform;
  - envelope bars from ENV3;
  - a live filter-curve display;
  - VU meter and spectrum (possibly on the hardware FFT engine);
  - the SCENE view's music-driven pixel vignettes.
- **Optional UI sounds played on the SID itself** (blips, a drive "chunk"
  on save).
- **Budget rules:**
  - A full LCD redraw takes about 77 ms over SPI, so animation uses small
    dirty strips at about 25 fps.
  - Everything runs in the main loop and never touches the audio budget.
  - The emulator tests measure main-loop load, including a "busiest screen"
    scenario.
- **Original art only** (section 11).

## 8. Performance budget

At 240 MHz and 44.1 kHz, one core has about 5,400 cycles per stereo sample.

| Block | Target cycles/sample |
|---|---|
| Chip A (SID + character) | ≤ 900 |
| Chip B | ≤ 900 |
| 6502 at ~1 MHz (≈ 22 C64 cycles per sample) | ≤ 400 |
| Post-chip FX + mix | ≤ 500 |
| **Audio total** | **≤ 2,700 (50 % of one core)** |

- The rest of the core is left for the UI, MIDI and the sequencer, so
  everything fits on one core.
- **Enforcement** (borrowed from Felucca):
  - a host baseline of instructions per sample per block, failing at +25 %;
  - a static count of loop instructions in the pi32v2 disassembly, failing at
    +10 %;
  - an on-device and fm1-emu cycle counter shown on a debug page.
- **If we go over budget, cut in this order:**
  1. chip B's filter oversampling;
  2. chip B's character tables (it falls back to a plain 8580 model);
  3. the post-chip effects.

## 9. Testing

Layers (all run in CI):

1. **Unit tests (host):**
   - SID: oscillators, noise LFSR, envelope generator including the ADSR bug,
     combined waveforms, filter response;
   - 6502: Klaus Dormann's functional tests and Tom Harte's SingleStepTests;
   - format round-trips and migration fixtures.
2. **Reference tests against reSIDfp (host only).** reSIDfp is fetched and
   used only by the test harness; it is never linked into the firmware.
   - For each chip profile and each reference tune, render with both engines
     and compare within the agreed limit.
3. **Player tests against the original GT2 player.**
   - Run GoatTracker's own packed player for a song in our 6502, and our
     native player for the imported `.sng`.
   - The register writes must match frame for frame.
   - **Amended (M2 plan, 2026-10-15):** in M2 "our 6502" is a host-only
     one, the `mos6502` crate (BSD-3-Clause), behind a trait, with
     GoatTracker's `gt2reloc` packing the song (test-only, GPL-2.0-or-later,
     never in the firmware). M3's device 6502 is later checked against it.
     "Frame for frame" means each frame's ordered list of SID writes
     (decision record 0007).
4. **Golden renders:** an FNV-1a hash of every preset, demo song and starter
   tune. `GOLDEN_UPDATE=1` is the deliberate way to accept new output.
   Sanity checks for clipping and DC.
5. **Fuzzing:** `cargo fuzz` (libFuzzer) on every importer and on the song
   loader, with the C code built under ASan and UBSan. A seed corpus comes
   from the reference tunes.
6. **UI snapshots:** the host renders every view to PNG, plus a layout lint
   (text overflow, contrast).
7. **End-to-end in fm1-emu:** boot the real `.fwsc` headless and run:
   - `--keys` scripts that load a song, press PLAY, mute a voice and turn a
     knob;
   - `--wav` capture, compared with the host render;
   - `--snap` screenshots compared with the UI snapshots.
8. **Hardware checklist** before each release, run on a real FM-1: core-1
   self-test, install/restore, battery, MIDI over both ports.

## 10. Tooling and release engineering

- **Build:**
  - CMake (cross-compiling only), driven through `tools/fm1-docker`;
  - the JieLi pi32v2 toolchain, fetched by script and checked by SHA-256;
  - the toolchain runs in Docker (`linux/amd64`) on anything except Linux
    x86-64;
  - the AC79 SDK is pinned to a commit, and its blobs are checked by SHA-256;
  - the build is reproducible: same source, same `.fwsc`.
- **Host and web builds:** a cargo workspace (`host/`) for every host tool,
  test and benchmark, building the C core through the `cc` crate;
  Emscripten for the WASM build.
- **CI (GitHub Actions)** on every pull request:
  - firmware build, host tests, fuzz smoke (short), CPU budgets;
  - fm1-emu end-to-end;
  - `reuse lint`;
  - docs build.
- **Nightly CI:** long fuzzing runs, a nightly `.fwsc` with SHA256SUMS and
  provenance, and the nightly web demo.
- **Releases:**
  - SemVer, git tags, and `CHANGELOG.md` in Keep a Changelog style;
  - the release workflow attaches the `.fwsc`, SHA256SUMS and licence
    bundle;
  - the firmware identity string encodes the version.
- **Installer:** a WebMIDI web installer plus a `sidou-install` command-line
  tool in the host workspace, using the community's two-step SysEx loader protocol.
  - Back up user songs before flashing.
  - A "return to stock" path that accepts only the stock image with a known
    SHA-256.
  - Clear warnings about switching from other custom firmwares.
- **Web:** GitHub Pages hosts:
  - the showcase (WASM build plus a starter-pack jukebox);
  - the installer;
  - the manual;
  - a link to fm1-emu running the real image.
- **USB identity:** do not ship with the pid.codes test VID/PID. Apply for a
  proper pid.codes ID before 1.0.
- **Release videos, generated from the emulator.** Other FM-1 projects
  promote releases with emulator-captured demo videos. We generate ours in CI
  so they always match the shipped firmware:
  1. **Demo scripts** (`demos/*.keys`): fm1-emu key scripts with timing, one
     per feature. Each changelog entry names the demo that shows it. The
     scripts also run as end-to-end tests.
  2. **Capture:** fm1-emu runs the release `.fwsc` headless and records screen
     frames (`--snap` at video rate) and audio (`--wav`) for each script.
     Same script, same footage.
  3. **Composition:** HyperFrames (HTML/GSAP) or Remotion (React) renders the
     footage inside an FM-1 frame. It adds an on-screen key and knob overlay
     driven by the script, captions from the changelog, and a C64-style
     title card. The choice between the two tools is made in M4.
  4. **Output:** a "what's new" video per release, plus a short clip per
     feature, attached to the GitHub release and embedded in the manual.
     Uploading to YouTube is a manual step.

## 11. Licensing and attribution

- **Licence:** SIDOU's own files are GPL-3.0-or-later. The firmware image as
  a whole is distributed under GPL-3.0, because it may include GPL-3.0-only
  HAL code from Felucca.
- **REUSE compliance:**
  - an SPDX header in every file;
  - `REUSE.toml` for data and binary files;
  - the full licence texts in `LICENSES/`;
  - `reuse lint` in CI.
- **`LICENSING.md`** lists every third-party component with its origin, licence
  and how it is used (linked into the firmware, test-only, or reference only).
  Expected entries:

| Component | Licence | Use |
|---|---|---|
| Felucca HAL pieces (if reused) | GPL-3.0-only | linked |
| reSID combined-waveform samples / curve data | GPL-2.0-or-later | data in firmware |
| reSIDfp | GPL-2.0-or-later | test reference only, not linked |
| 6502 core (e.g. floooh/chips m6502, if adopted) | zlib | linked |
| fm1-emu | MIT | test tool only |
| JieLi AC79 SDK blobs (`uboot.boot`, `cfg_tool.bin`, eq cfg) | Apache-2.0 | packaged |
| Klaus Dormann / Tom Harte 6502 test suites | own licences | test data |
| GoatTracker 2 (`gt2reloc`, `player.s`, example songs) (amended, M2) | GPL-2.0-or-later | test-only reference; SIDOU's player follows `player.s` and credits it |
| `mos6502` crate (amended, M2) | BSD-3-Clause | test-only (the host 6502 for the register-match tests) |

- **Attribution:** `CREDITS.md` credits the people and projects whose work
  we build on or learn from (reSID/reSIDfp authors, Felucca and its forks,
  fm1-emu, GoatTracker, SID-Wizard, HVSC). It also credits contributors and
  the users whose requests shaped features.
- **Music:** C64 game music is still under copyright, held by composers or
  publishers. HVSC is tolerated by the scene, but it is not a licence we can
  pass on. So music is split into two tiers:
  - **In the firmware (starter pack):** only openly licensed tunes (e.g.
    Creative Commons, from modern SID composers) and tunes we commission in
    the native format, written to show off SIDOU's features. Each tune has a
    metadata file recording composer, licence or written permission, and
    source.
    - Licences must allow redistribution, including commercial
      redistribution, because GPL firmware can be sold pre-flashed. A
    "non-commercial only" licence does not qualify.
  - **User import (not in the firmware):** people bring their own chiptunes.
    The browser companion loads `.sid`, GoatTracker and SID-Wizard files, or
    tunes from the user's own HVSC copy, onto the device. Optional playlists
    ("Hubbard essentials") are lists of references, not tunes.
- **Fonts and graphics:**
  - We draw our own PETSCII-style font, released CC0 or GPL. The Commodore
    character ROM font is not used.
  - The C64 palette values are fine to use.
- **Trademarks:** "Commodore", "C64" and "M-VAVE" are used only to describe
  compatibility. No logos.

## 12. Documentation

Each kind of document has one job:

- `README.md`: what it is, a demo link, install, status.
- `docs/manual/`: the user manual. It is published to Pages and also
  available offline from the web app. It includes a getting-started guide
  and how-tos ("rip a tune", "jam over a .sid").
- `docs/reference/`: song format spec, SysEx/installer protocol, control map,
  tracker command reference.
- `docs/architecture.md`, `BUILDING.md`, `TESTING.md` for contributors.
- `docs/decisions/`: short decision records for choices like the ones in
  this document.
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md` (how to report
  bricking or installer issues).

## 13. Handling user requests and updates

- **GitHub Issues:** bug reports only. The issue form requires the firmware
  version, whether it happened on hardware or in the emulator, and a song
  file or key script to reproduce it.
- **GitHub Discussions › Ideas:** feature requests. Popular ideas move to a
  public roadmap (a GitHub Project).
- **Labels:** `area/*`, `good first issue`, `needs-hardware`.
- Each release's changelog credits the people who reported issues or
  suggested features.
- **Song files stay compatible** across versions (format migrations plus
  fixtures), so updating never costs anyone their songs.
- **Nightly channel** for testers, and stable releases for everyone else.

## 14. Milestones

1. **M0, foundations:**
   - repo scaffolding, licences, CI skeleton;
   - host build of the SID core;
   - reSIDfp reference harness;
   - benchmark on fm1-emu with a minimal HAL.

   *The decision point:* does the SID core fit the cycle budget?
2. **M1, sound:**
   - full SID core and the chip profiles;
   - character layer and post-chip effects;
   - live view on the device (playable instrument with knobs);
   - MIDI: parser, live mode, CC map, MIDI learn (section 16);
   - **M1e, look and feel:**
     - art direction page;
     - our own font;
     - UI widget and transition framework within the strip budget;
     - boot intro, themed menus, About cracktro;
     - scopes, envelope bars and VU in the LIVE view.
3. **M2, songs** (amended by the M2 plan, 2026-10-15; the plans are
   `docs/plans/2026-10-15-m2-overview.md` and its m2a-m2f files):
   - GoatTracker 2 `.sng` as the native song file (read and written, with
     SIDOU's trailer), and the player, with the register-match tests against
     GoatTracker's own player (was: "native format and player" and "GT2
     import, with the register-match tests");
   - STEP and TRACKER views;
   - storage;
   - chip B, the CHIP view's B tab, the character's expert layer and UI
     sounds (moved from M1);
   - MIDI: clock and transport, multitimbral and whole-chip modes, program
     change, SysEx dumps;
   - SCENE view, with one vignette first: RED MOON RITE (DUSK DUEL optional).
4. **M3, `.sid` files:**
   - 6502 and PSID playback;
   - mute and solo;
   - browser;
   - starter pack (openly licensed and commissioned tunes);
   - user import of chiptunes (`.sid`, `.sng`, SID-Wizard) through the
     companion, with optional playlists for the user's own HVSC copy.
5. **M4, showcase and release:**
   - WASM demo, installer, manual;
   - release-video pipeline (demo scripts, emulator capture, composition);
   - 0.1 release, followed by the nightly channel.
6. **M5:** RIP, SID-Wizard import, chip B song tracks, MIDI extras (NRPN,
   poly aftertouch, sequencer MIDI out), more SCENE scenes.
7. **After M5 (optional):** BLE MIDI feasibility spike for the user-built
   wireless variant (section 17).

## 15. Language choice

Rule: a modern language must be justified by a smaller footprint, easier
modification or better performance.

| Part | Language | Why |
|---|---|---|
| `core/`, `app/`, `platform/fm1` | C11 | The only pi32v2 compiler is JieLi's clang 4.0.1, a proprietary LLVM 4 fork. Rust needs a modern LLVM backend for the target, and none exists. Zig's C backend produces bulky, extension-heavy C that runs DSP loops slower and is harder to debug. Both fail all three tests. C also gives one codebase for firmware, host and WASM. |
| Profiled hot spots on the device | pi32v2 assembly (only when needed) | Used only when an algorithmic fix and restructured C still miss the budget. The C version stays the reference and the host build always uses it. The assembly is fm1-only, chosen at build time, and must be bit-exact with the C: the emulator gate's host-vs-target sample comparison proves it. |
| All host tooling: tests, benchmarks, reference comparisons, emulator gate, table generators, packaging (`host/`) | Rust (cargo workspace) | One host language and one harness (`cargo test`, criterion). Rust is needed anyway because fm1-emu is a Rust library. It calls the C core through FFI (`cc` crate), so the code that runs on the device is tested directly. Third-party C++ (reSIDfp, test-only) is compiled by `cc`; our only C++ is a small (~60-line) `extern "C"` wrapper around its class API. Decided 2026-10-14, replacing C++ tests and Python tools. |
| Web showcase and installer UI | TypeScript (no framework), Emscripten for the core | Type safety for the WebMIDI/SysEx code that can brick a device. |
| fm1-emu | Rust (upstream) | Used as a library by the emulator gate; not our code. |

Revisit if a modern LLVM or GCC backend for pi32v2 appears. In that case,
Rust or Zig for `app/` would be the first candidate.

## 16. MIDI

MIDI is first-class: everything on the panel can be done over MIDI, and the
sequencer fits into a MIDI setup as easily as it plays alone.

**Ports.** USB MIDI (class-compliant, in and out) and TRS MIDI in. The FM-1
has no TRS out, so all MIDI output goes over USB. Both inputs merge into one
stream.

**Architecture.**
- `core/midi/` is portable C. It handles:
  - **the parser:** running status, real-time bytes inside other messages,
    and SysEx streaming without buffering a whole dump;
  - **the router:** channel to voice or chip, CC to parameter;
  - **the clock follower;**
  - **SysEx handlers.**

  It is unit-tested on the host and fuzzed like the importers.
- **Timestamps:** each byte is stamped on arrival in the HAL (UART DMA, USB
  poll), converted to a sample position, and put on the same event queue the
  sequencer uses. MIDI is sample-accurate.
- **Latency target:** at most one audio block (2.9 ms) plus the USB poll
  interval, from note-on to sound. This is measured in fm1-emu as part of
  the tests.

**Modes**, saved per song:
- **Live:** one channel plays chip B. It is three-voice polyphonic with voice
  stealing, or mono/legato.
- **Multitimbral:** chip A's voices on channels N, N+1, N+2, each its own
  instrument, and chip B on a fourth. This is how a DAW plays or sequences
  the SID directly.
- **Whole-chip:** one channel plays all three voices of a chip, with chord
  and arpeggio options (the classic one-voice "chord" arpeggio included).
- **Omni** is available for quick setups.

**Expression.**
- **Velocity, pitch bend** (range per instrument), **mod wheel,** and
  **channel and polyphonic aftertouch** are all assignable. Typical targets
  are pulse width, cutoff, resonance, character controls and envelope
  scaling.
- **The CC map is fixed and published:** every engine and character
  parameter has a CC. 11-bit cutoff and 12-bit pulse width also have NRPNs
  for full resolution.
- **MIDI learn** remaps knobs.
- **Knob moves are sent out over USB** as CCs, so a DAW can record them as
  automation.

**Clock and transport.**
- **Clock in:** follows external MIDI clock, smoothed so it doesn't jitter.
- **Clock out** over USB.
- **Start, stop, continue** and **song position pointer**, in both directions.
- **`.sid` playback:** start and stop follow transport. Tempo lock applies
  when the tune's player runs at frame rate. Tunes that set their own timer
  are start/stop only.

**Program change** (with bank select) switches songs and instruments.

**SysEx.** The protocol is documented in `docs/reference/`. It covers:
- device inquiry (identity);
- instrument and song dump and load;
- the companion app's transfers;
- installer and backup messages, kept compatible with the FM-1 community
  loader protocol where possible.

During development we use the non-commercial ID 0x7D; we'll look at a proper
manufacturer ID before 1.0.

**Sequencer MIDI out:** tracks can send notes and CCs out over USB, so SIDOU
can sequence other gear as a SID tracker.

**Documentation and tests.**
- `docs/reference/` gets a MIDI implementation chart and the CC/NRPN map.
- Tests cover:
  - the parser (unit and fuzz);
  - the router modes;
  - clock following with jittered input;
  - end-to-end in fm1-emu (MIDI events in `--keys` scripts, including a
    note-on-to-audio latency check).

**By milestone:**

| Milestone | MIDI work |
|---|---|
| M1 | Parser, live mode, velocity, bend, the CC map, MIDI learn, latency test. |
| M2 | Clock in and out, transport, multitimbral and whole-chip modes, program change, instrument and song SysEx. |
| M3 | Transport during `.sid` playback. |
| M4 | Implementation chart in the manual. |
| M5 | NRPN high resolution, poly aftertouch, sequencer MIDI out. |

## 17. Optional wireless (user-built)

The FM-1 ships with BLE MIDI, and its AC791N SoC also has a Wi-Fi radio.
Published SIDOU firmware uses neither. Wireless can be an opt-in variant that
users build themselves, at their own risk.

**Why it can't be in the published firmware:**
- **Closed stacks.** JieLi's Bluetooth and Wi-Fi stacks exist only as
  closed binary libraries in its SDK. Distributing a binary that links them
  with SIDOU's GPL-3.0-only code (vendored from Felucca) would break the
  licence, commercial or not.
- **Private builds are allowed.** GPL does not restrict a user who builds
  the combination for themselves without distributing it.
- **Certification.** The device is certified as using BLE. Wi-Fi is a
  transmitter mode it was never certified for.

**Model:**
- **Source only:** we publish the integration source and an opt-in build
  setting (`SIDOU_WIRELESS=ble`, later `=wifi`).
- **Local build:** the build downloads JieLi's SDK libraries on the user's
  machine and links them locally.
- **Never published:** CI never builds, tests or publishes the wireless
  variant, and no release artefact contains vendor libraries.
- **Opt-in and warned:** it is off by default. The docs explain the licence
  reason and the user's responsibility, and Wi-Fi carries an explicit
  regulatory warning. Not legal advice.

**Engineering risk:**
- **Runtime:** the vendor stacks expect the SDK's RTOS runtime, memory layout
  and interrupt setup, while SIDOU is bare-metal.
- **RAM:** the BLE stack takes a large share of about 430 KB, competing
  with two SIDs, the sequencer and the UI.
- **Interrupts:** radio interrupts must not disturb the audio deadline.

M-VAVE's `fm1_open` already implements BLE MIDI on this SDK and is the
reference.

**Plan:**
1. **BLE feasibility spike (time-boxed, after M5):** build `fm1_open`'s BLE
   MIDI path, then measure RAM, flash and interrupt latency next to SIDOU's
   audio engine. Write a decision record: go or no-go, and how
   (embed the SDK runtime or port SIDOU onto it).
2. **If go:** BLE MIDI as a user-built variant. BLE MIDI messages feed the
   same timestamped MIDI path (section 16).
3. **Wi-Fi only after BLE works:** network installs, companion-app sync and
   RTP-MIDI, with the regulatory warning.

## 18. Open risks and questions

- How well core 1 works on real units, and the exact clock speed, need
  checking on hardware.
- Whether the 6581 nonlinear filter can match reSIDfp within the CPU budget.
  The benchmark in M0 decides this.
- The SID-Wizard format documentation is thin, so we may need to read the
  original source.
- The starter pack needs openly licensed tunes found or commissioned.
- The PETSCII-style font and theme need someone to design them.
