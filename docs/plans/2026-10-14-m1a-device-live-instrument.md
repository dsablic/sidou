# M1a: Device bring-up and live instrument, implementation plan

<!-- REUSE-IgnoreStart -->

> **For Claude:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> to implement this plan task by task.

**Goal:** a first SIDOU firmware (`sidou.fwsc`) that:
- boots on the M-VAVE FM-1;
- plays one SID chip as a live instrument from the keys and knobs, and from
  MIDI notes and CCs;
- shows its state on the LCD;
- answers the community SysEx update protocol, so users can always leave it
  again;
- is tested end to end in fm1-emu.

**Architecture:**
- `platform/fm1/` holds the device code:
  - Felucca's register-level HAL, vendored unchanged (GPL-3.0-only);
  - Felucca's USB stack and update loader, vendored with minimal adaptation;
  - our own startup code, linker script, main loop, LCD and text renderer.
- `app/` holds portable C with no hardware access: the live-instrument logic
  (presets, voice allocation, knob-to-register mapping, a MIDI byte parser).
  It is unit-tested on the host from Rust like `core/`.
- Packaging is a Rust host tool (`host/crates/sidou-pack`), checked by
  unpacking with fm1-emu's own reader and by booting in fm1-emu.

**Tech stack:**
- device: C11 with the JieLi toolchain in Docker, built by CMake (preset
  `fm1`);
- host: Rust (the cargo workspace in `host/`);
- verification: fm1-emu, both as a library and as a pinned binary.

**Background:**
- M0's results and its plan: `docs/plans/2026-10-14-m0-foundations.md`.
- Design: `docs/plans/2026-10-14-sidou-design.md`, sections 2, 3, 7, 10, 11
  and 16.
- Reference clones of Felucca and fm1-emu are in the session scratchpad, but
  vendor from upstream at the pinned commits listed below.

**Pinned sources:**
- Felucca: `github.com/hugelton/Felucca` at the current `main` HEAD. Record
  the commit in `platform/fm1/VENDORED.md` and in `LICENSING.md`.
- fm1-emu: `e52f4640157f490f8c3af778dd044e9d33e989e8`, as used in M0.
- AC79 SDK blobs: tag `AC79NN_SDK_V1.2.1_2023-12-13`, checked by SHA-256 at
  build time (never committed). Felucca's `tools/build.py:46-50` lists the
  hashes. Licence: Apache-2.0.

**Conventions:** the same as M0.
- SPDX headers on every file. Vendored files keep their original headers and
  get an entry in `VENDORED.md`.
- Each commit ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Stage only the files you changed; never `git commit -a`.
- Host commands run from `host/` with
  `export PATH=/opt/homebrew/opt/rustup/bin:$PATH`.
- The device build is
  `tools/fm1-docker cmake --preset fm1 && tools/fm1-docker cmake --build --preset fm1`.

**Out of scope:**
- the web installer (M4);
- song, step and tracker views (M2);
- chip B and dual-chip operation (M2);
- the character layer (M1c);
- the full MIDI router, clock and SysEx dumps (M1d/M2);
- the second core;
- our own PETSCII-style font (later in M1);
- a real USB VID/PID (before 1.0).

**Safety rules (these never change):**
- Never send `F0 22 24 35 7D F7` (it drops the device into mask ROM mode).
- The package's flash head must be byte-identical to stock V15.
- Code that runs while XIP is off lives in `.ram_text`.
- PB1 (battery) is only ever an analog input.
- PA2 (backlight and the key matrix `/OE`) is only touched with interrupts
  off.
- The watchdog is fed from the main loop.

---

### Task 1: Vendor the Felucca HAL

**Files:**
- `platform/fm1/hal/*`: copied unchanged from Felucca `firmware/hal/`,
  excluding `fm1_cpu1.*` (we don't use the second core) and `fm1_guard.h`
  (optional, added later if needed).
- `platform/fm1/VENDORED.md`: the source repo, commit and file list, plus
  any local changes and why.
- `LICENSES/GPL-3.0-only.txt`, from `python3 -m reuse download GPL-3.0-only`.
- `LICENSING.md`: add a row for the vendored HAL (GPL-3.0-only, linked into
  the firmware), and note that the firmware image as a whole is therefore
  GPL-3.0-only.
- `CREDITS.md`: no change, since Felucca is already credited.

**Steps:**
1. Copy the files.
2. Check that each one compiles on its own: a translation unit
   `platform/fm1/hal_check.c` that includes every header, with a stub
   `fm1_fault`, built by the fm1 preset as an object library.
3. Run `reuse lint`.
4. Commit: `feat(fm1): vendor Felucca HAL (GPL-3.0-only)`.

### Task 2: Boot, audio and a test tone in fm1-emu

**Files:**
- `platform/fm1/crt0.S`: the SPL enters at 0x02000120, and the first
  instruction must be `goto +4; rts`. Save `icfg`/`psr`/`r0` as Felucca
  does, `cli`, set both stacks, then call `fm1_cstart`.
- `platform/fm1/app.ld`: Felucca's memory map, with RAMTEXT, RAM, POOL,
  NOINIT and the stacks at the addresses from `app.ld`. Keep
  `.ram_text`/`.noinit`. Use `ASSERT`s for overlaps and
  `--orphan-handling=error`, as in the M0 benchmark.
- `platform/fm1/main.c`, following fm1-sid's `main.c:1055-1146` init order:
  1. `fm1_time_init`, `fm1_wdt_arm(0x0D)`, `fm1_irq_init`;
  2. `.bss`/`.data`/constructors;
  3. `fm1_mailbox_clear`;
  4. audio init with a 2 × 128-frame buffer;
  5. enable IRQs;
  6. main loop that feeds the watchdog.

  The ISR bodies are named `fm1_alnk0_irq` and `fm1_timer5_irq`. Define
  `fm1_fault` to record the crash in `.noinit` and reboot.
- `platform/fm1/audio.c`:
  - fill a free half-buffer from `sid_render` into 128 mono int16 samples,
    written to L and R as `(int32)s << 8`;
  - sample rate 44118 (24 MHz / 544 = 44117.6 Hz);
  - for this task the SID plays a fixed saw tone, so we can tell it boots.
- `platform/fm1/CMakeLists.txt` produces `sidou_app.elf` and
  `sidou_app.bin` (`.text` and `.data` joined as in M0), plus `.dis`.
- `host/crates/emu-gate/tests/boot.rs` (new): load `sidou_app.bin` into
  fm1-emu through the library API. Use the same approach as M0, or the
  stock-boot pieces (`stock_boot`) if a boot-parameter block is needed; the
  image expects `r0` to point at one. Run 500 ms of emulated time and assert
  that the audio output is not silent and its pitch matches the test tone
  (reuse the DFT estimator from `sidou-tests`, moving it to a shared crate if
  needed).

**Steps:**
1. Write the e2e test first; it fails because there is no image yet.
2. Implement the firmware.
3. Build, then run the test.
4. Commit: `feat(fm1): boot, audio ISR and SID test tone`.

### Task 3: Portable live-instrument logic (`app/`)

**Files:**
- `app/live.h`, `app/live.c`: pure C, no hardware. Holds a `live` state with
  the preset, octave, chip model and parameter values, plus a 3-voice
  allocator. The API:
  - `live_init(live*, sid*)`
  - `live_note_on(live*, note, velocity)`
  - `live_note_off(live*, note)`
  - `live_knob(live*, knob_id, delta)`
  - `live_button(live*, button_id)`
  - `live_cc(live*, cc, value)`
  - `live_set_master(live*, 0..1023)`

  All of these change SID registers only through `sid_write`.
  - **Voice allocation:** take a free voice first, otherwise steal the oldest
    note. A note-off releases the gate.
  - **Presets** (at least 4, each defined as data): saw lead, pulse bass with
    PWM, sync lead (using SID hard sync), and noise percussion. Each preset
    sets waveform, ADSR, pulse width, filter routing, cutoff and resonance.
  - **Knobs:**
    - K1: cutoff (11-bit; one detent = 16 steps)
    - K2: resonance
    - K3: pulse width
    - K4: preset
    - MASTER: volume
  - **Buttons:**
    - OCT-/OCT+: octave
    - FX: toggles 6581/8580 (re-initialises the chip with the new model
      while keeping the parameters)
  - **Pitch:** a PAL frequency table for MIDI notes 0–127 (computed, not
    typed in: `round(f × 2^24 / 985248)`, generated at init or by a test-checked
    constant table).
- `app/midi_in.h`, `app/midi_in.c`: a byte-stream parser. It handles running
  status, real-time bytes inside other messages, and ignores SysEx except for
  forwarding it to a callback. It emits note on/off (velocity 0 counts as
  off), CC and pitch bend. It is a pure state machine.
- `host/crates/sidou-sys`: extend the bindings to `app/`.
- `host/crates/sidou-tests/tests/app/`: unit tests, written first, for:
  - note on/off gating and register values;
  - voice stealing order;
  - octave limits;
  - preset switching;
  - knob clamping;
  - model toggle preserving parameters;
  - the frequency table against the formula;
  - the MIDI parser (running status, real-time interleaving, velocity-0
    note-off, SysEx skipping, truncated messages).

  Add a fuzz-style property test as well: random byte streams must never
  produce an out-of-range note or channel.
- `CMakeLists.txt`: add the `app` library for the fm1 build.

**Commit:** `feat(app): live instrument logic and MIDI parser with host tests`.

### Task 4: Keys, knobs, LEDs and the LCD

**Files:**
- `platform/fm1/input.c`: in the timer ISR, call `fm1_input_tick`. In the
  main loop, read edges and encoder deltas and turn them into `live_*` calls.
  Key ids 14–40 are notes F3 to G5, with the octave offset coming from
  `live`. Use Felucca's button and encoder ids (see `VENDORED.md`). Light the
  LEDs for held keys and the current octave.
- **Concurrency:** `live_*` calls write SID registers. To keep the audio ISR
  consistent, put register writes from the main loop on a lock-free SPSC ring
  `{reg, value}`, which the audio ISR drains at the start of each block. The
  `live` logic writes into this ring through a `sid_write` sink on the
  device. The host tests keep calling `sid_write` directly; abstract it as a
  function pointer in `live` or as a small `reg_sink` interface.
- **Carry-overs from the Task 3 review.** These are required.
  - **Ring drain:** the drain switches on `reg & 0x80`. `LIVE_REG_RESET`
    (0x80, value = model) must never reach `sid_write`, which would mask it
    to register 0x00. Add a host test that feeds the device drain function a
    0x80 entry.
  - **Model toggle cost:** a `sid_init` from inside the audio ISR (2048
    `tanf` today) must not overrun a block. Pick one and measure it in the
    emulator:
    1. precompute both models' cutoff tables;
    2. build the new chip in the main loop and swap a pointer at a block
       boundary;
    3. explicitly accept and mute a one-block gap.

    M1b Task 5 replaces runtime `tanf` with generated tables, which makes
    option 1 natural.
  - **Ring size and overflow:** a single `write_all` is 26 entries, and FX,
    preset, notes and tick can stack up in one loop iteration. Size the ring
    for the worst case and make overflow block or retry, never drop: a
    dropped write means a stuck gate or the wrong model. Test the worst-case
    burst.
  - **Key note-off:** each physical key remembers the note it sent at
    key-down and releases that note, so an octave change while a key is held
    can't leave a note stuck.
- `platform/fm1/lcd.c`:
  - ST7789 init (Felucca's sequence) through `fm1_lcd_hw_*`;
  - a 240 × 24-pixel strip renderer with dirty rows, so there is no full
    framebuffer; at least 240 × 24 × 2 bytes, in RAM for DMA, RGB565
    byte-swapped;
  - an 8×8 bitmap font from `font8x8_basic` (public domain), vendored with
    attribution, as a placeholder until our own PETSCII-style font exists.
- `platform/fm1/ui.c`: a status screen redrawn at most every 40 ms and only
  when something changed. It shows:
  - the title "SIDOU" and the version;
  - chip model, preset name and octave;
  - cutoff, resonance, pulse width and volume as numbers and bars;
  - the held notes.

  C64 palette: the classic light blue on dark blue.
- e2e test `host/crates/emu-gate/tests/live.rs`, using fm1-emu's script
  actions (key down and up, `K1:+n`, `pot:`) or their library equivalents:
  - pressing a key sounds at the right pitch;
  - turning K1 down changes the spectrum (energy above 2 kHz drops);
  - OCT+ doubles the pitch;
  - an LCD snapshot after boot is not uniform and matches the C64 background
    colour at a fixed pixel.

**Commit:** `feat(fm1): keys, knobs, LEDs and status LCD driving the live instrument`.

### Task 5: MIDI in (TRS and USB) and the USB stack

**Files:**
- `platform/fm1/usb/`: vendor Felucca's `src/usb.c` and `src/midi_uart.c`
  (GPL-3.0-only), plus the minimal shims they need (`fm1_ms`,
  `RING_PUBLISH`, the parts of `libc.c` they use). Strip CDC, USB audio and
  editor endpoints; keep USB-MIDI and the update protocol entry (Task 6).
  Note every change in `VENDORED.md`. The USB descriptor keeps pid.codes
  test VID/PID 0x1209:0x0001 for now, with the TODO in `docs/TODO.md`
  ("apply for a pid.codes PID before 1.0").
- **Polling:** run `usb_poll()` and `uart_midi_poll()` every 5th TIMER5 tick
  (2 kHz), as Felucca does. Each byte or packet is timestamped with
  `fm1_ticks()` (24 MHz) and goes into a ring the main loop drains into
  `midi_in` → `live`.
- **Default MIDI mapping (M1a):**
  - omni notes play the live instrument;
  - CC 74 is cutoff (scaled to 11 bits), CC 71 resonance, CC 1 pulse width
    and CC 7 volume;
  - program change selects the preset.

  The full CC map and MIDI learn come in M1d.
- e2e test (TRS only, because fm1-emu doesn't emulate USB): a `midi:903c64`
  note-on through the script sounds C4 at the right pitch; a note-off
  releases it; CC 74 changes the cutoff.

**Commit:** `feat(fm1): TRS and USB MIDI input driving the live instrument`.

### Task 6: Update protocol, loader and packaging (`sidou-pack`)

**Files:**
- `platform/fm1/loader/`: vendor Felucca's update loader
  (`firmware/loader/*`, GPL-3.0-only). Build it with the fm1 preset as
  `ota.bin`: RAM-only at 0x01C0A800, ≤ 0x14000 bytes, starting with the
  marker. Change the identity marker to SIDOU's (`SIDOU-LOADER-1`) only if
  the community tools don't depend on Felucca's; otherwise keep it and
  document why.
- The app implements step 1 of the update protocol (`fm1_install.py`
  semantics), so community installers and the stock V15 return path work:
  - identity request `cmd 0x11` → `FM-1_9xx`-style product id, with a SIDOU
    id string;
  - upgrade request `F0 22 24 35 7F F7`;
  - staged loader read with `0x30` chunks ≤ 512 bytes;
  - write the loader to flash 0xE0000;
  - reply `0xE0000000`;
  - write the update record and reset.

  This is vendored from Felucca's `ota.c` with minimal changes. It includes
  the head-identity check that allows V15.
- **Boot-loop guard.** This is a safety requirement, and it must land before
  any hardware flash. Felucca's `fm1_cstart` (`firmware/src/main.c:336-349`)
  keeps a `bootguard` in `.noinit` and calls `fm1_enter_uboot()` after 2
  failed boots. SIDOU needs the same, so a crash early in boot, or a
  watchdog reset loop, ends in UBOOT (which still accepts updates) instead of
  looping forever. The counter is cleared once the main loop has run stably
  for N seconds. Test it in the emulator: force an early fault twice, then
  check that the code enters UBOOT. If fm1-emu can't model the reset, test
  the counter logic on the host instead.
- `host/crates/sidou-pack` (Rust): a port of Felucca's `tools/fm1pkg_make.py`
  (flash head with CRC16 and JLFS entries, ENC/SFC scrambling with key
  0x980F, the `ufw` container with marker insertion), the loader wrapping
  (two 32-byte heads plus LZ4 blocks; use the `lz4_flex` block API) and the
  AC79 blob fetch.
  - **AC79 blobs:** downloaded on first use into `build/ac79/` and checked
    against the pinned SHA-256s. They are never committed, and
    `LICENSES/Apache-2.0.txt` covers them.
  - **CLI:**
    `sidou-pack --app build/fm1/.../sidou_app.bin --loader build/fm1/.../ota.bin -o build/fm1/sidou.fwsc`.
  - **Tests:**
    - **round trip:** unpack the produced `.fwsc` with
      `fm1_soc::fwsc` (fm1-emu, MIT) and compare the app bytes and the head
      against Felucca's documented layout;
    - **golden:** pack a fixed tiny app plus loader and record its SHA-256;
    - **cross-check (one-time, documented):** Felucca's Python tool, run once
      by hand on the same inputs, must produce byte-identical output. Record
      the result in the commit message. This is not a CI dependency.
- e2e test: boot `sidou.fwsc` through the package path (`--fwsc` semantics
  through the library: `stock_from_package` + `stock_boot`) and assert the
  live tone plays. A second test drives the update protocol's identity
  request through UART MIDI (if fm1-emu routes SysEx from `midi:` actions)
  and checks the reply. If it can't, document the gap in `docs/TODO.md` and
  rely on hardware testing.
- CI (`.github/workflows/ci.yml`, fm1 job): after the Docker build, run
  `cargo run -p sidou-pack` to produce `sidou.fwsc`, run the e2e tests,
  upload `sidou.fwsc` as an artifact.

**Commits:**
- `feat(fm1): SysEx update protocol and vendored loader`
- `feat(host): sidou-pack .fwsc packager`
- `ci: package and boot sidou.fwsc in fm1-emu`

### Task 7: Hardware checklist and docs

**Files:**
- `docs/hardware-test.md`: a manual checklist for a real FM-1 (anyone with a
  device can run it). It covers:
  - install with the community installer: Felucca's `fm1_install.py`, or
    the aroum/fm1-custom-fw flasher that accepts a `.fwsc`;
  - boot;
  - each key and its LED;
  - each knob, MASTER, OCT±, FX (model toggle);
  - TRS and USB MIDI note and CC;
  - the LCD;
  - return to stock V15;
  - reinstall SIDOU.

  Each item has a pass/fail box and room for notes on the button-mapping
  calibration (the printed labels are guesses).
- `README.md`: how to try M1a (build, package, run in fm1-emu with the CLI,
  install on hardware at your own risk with the community tools), with clear
  warnings.
- `CHANGELOG.md`, `LICENSING.md` (vendored files, font8x8, AC79 blobs,
  `lz4_flex`), `docs/TODO.md` (VID/PID, hardware results).

**Commit:** `docs: M1a hardware checklist and how to try the live instrument`.

---

## Done when

- `sidou.fwsc` builds in CI, unpacks correctly, and boots in fm1-emu.
- The e2e tests pass for the tone, keys, knobs, octave, model toggle, LCD,
  and TRS MIDI notes and CC.
- All host tests (including the new `app/` tests) pass, and so do clippy,
  fmt, UBSan and ASan.
- The M0 emulator gate still passes.
- `reuse lint` passes, and every vendored file is listed with its licence.
- The hardware checklist exists. Hardware results are recorded when someone
  runs it (this doesn't block M1a in CI).

<!-- REUSE-IgnoreEnd -->
