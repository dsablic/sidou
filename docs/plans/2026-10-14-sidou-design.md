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
- Showcase: the same core runs in the browser as a demo and companion app.
- Built to last: tested, documented, properly licensed, and able to handle a
  stream of user requests and releases.

Non-goals (for now)

- RSID tunes (these need full CIA/VIC timing), digi/sample playback tricks.
- More than two chips, or one chip per note.
- A new circuit-level 6581 model. It may come later as one more chip profile.

## 2. Hardware constraints (from the reference repos)

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
- **Main loop**: UI, screen drawing, encoders and keys, MIDI parsing, the
  sequencer clock, and flash input/output.
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
  - **LIVE**: keys play an instrument; the knobs control filter, pulse width
    and character; REC records loops.
  - **CHIP**: profile picker, character controls and post-chip effects, set
    separately for chip A and chip B.
- **Always available:** mute and solo per voice; play and stop; the browser.
- **Themes:** a C64 nostalgia theme (PETSCII-style font, C64 palette, border
  colour flashes on screen) and a clean modern theme.
- **Browser:** starter pack, user songs, user `.sid` files. RIP option.
- **SCENE view** (performance, M5): small pixel-art scenes that animate in
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

  Scenes run on the UI core from the same timed events as the audio, so they
  stay in sync and never touch the audio budget. They need a few KB of sprite
  data each. Scene footage also goes into the release videos.

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
4. **Golden renders:** an FNV-1a hash of every preset, demo song and starter
   tune. `GOLDEN_UPDATE=1` is the deliberate way to accept new output.
   Sanity checks for clipping and DC.
5. **Fuzzing:** libFuzzer with ASan and UBSan on every importer and on the
   song loader. A seed corpus comes from the reference tunes.
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
  - one `build.sh` / `tools/build.py`;
  - the JieLi pi32v2 toolchain, fetched by script and checked by SHA-256;
  - the toolchain runs in Docker (`linux/amd64`) on anything except Linux
    x86-64;
  - the AC79 SDK is pinned to a commit, and its blobs are checked by SHA-256;
  - the build is reproducible: same source, same `.fwsc`.
- **Host and web builds:** CMake for host tools and tests; Emscripten for the
  WASM build.
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
- **Installer:** a WebMIDI web installer plus a `tools/sidou_install.py`
  command-line tool, using the community's two-step SysEx loader protocol.
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

- **Attribution:** `CREDITS.md` credits the people and projects whose work
  we build on or learn from (reSID/reSIDfp authors, Felucca and its forks,
  fm1-emu, GoatTracker, SID-Wizard, HVSC). It also credits contributors and
  the users whose requests shaped features.
- **Music:**
  - Each starter-pack tune has a metadata file recording composer, licence or
    written permission, and source.
  - No HVSC tunes ship with the firmware; users load them from their own copy.
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
   - live view on the device (playable instrument with knobs).
3. **M2, songs:**
   - native format and player;
   - STEP and TRACKER views;
   - storage;
   - GT2 import, with the register-match tests.
4. **M3, `.sid` files:**
   - 6502 and PSID playback;
   - mute and solo;
   - browser;
   - starter pack.
5. **M4, showcase and release:**
   - WASM demo, installer, manual;
   - release-video pipeline (demo scripts, emulator capture, composition);
   - 0.1 release, followed by the nightly channel.
6. **M5:** RIP, SID-Wizard import, chip B song tracks, MIDI polish, SCENE
   view (one prototype scene first).

## 15. Language choice

Rule: a modern language must be justified by a smaller footprint, easier
modification or better performance.

| Part | Language | Why |
|---|---|---|
| `core/`, `app/`, `platform/fm1` | C11 | The only pi32v2 compiler is JieLi's clang 4.0.1, a proprietary LLVM 4 fork. Rust needs a modern LLVM backend for the target, and none exists. Zig's C backend produces bulky, extension-heavy C that runs DSP loops slower and is harder to debug. Both fail all three tests. C also gives one codebase for firmware, host and WASM. |
| Build and table generators (`tools/`) | Python 3 | Same as the rest of the FM-1 ecosystem (`build.py`, `fm1pkg_make.py`, installer tools can be adapted). Easy to change. |
| reSIDfp reference harness | C++ | reSIDfp is C++. Used only in tests. |
| Web showcase and installer UI | TypeScript (no framework), Emscripten for the core | Type safety for the WebMIDI/SysEx code that can brick a device. |
| fm1-emu | Rust (upstream) | Used as an external tool; not our code. |

Revisit if a modern LLVM or GCC backend for pi32v2 appears. In that case,
Rust or Zig for `app/` would be the first candidate.

## 16. Open risks and questions

- How well core 1 works on real units, and the exact clock speed, need
  checking on hardware.
- Whether the 6581 nonlinear filter can match reSIDfp within the CPU budget.
  The benchmark in M0 decides this.
- The SID-Wizard format documentation is thin, so we may need to read the
  original source.
- The starter pack needs us to contact composers for permission.
- The PETSCII-style font and theme need someone to design them.
