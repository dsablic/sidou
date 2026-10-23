<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# TODO

Follow-ups that aren't tied to a milestone task yet.

## Knob ranges and response curves: every turn should be audible, none too jumpy

- **Where:** `app/midi_map.c` (each parameter's range and curve), `app/panel.c` and every view
  that maps K1–K4 or an encoder; the effect slots' parameter tables (`core/fx/fx_slots.c`).
- **Problem:** parameters were given ranges from their engineering limits (a register's
  width, an algorithm's parameter span), not from what is musically useful. Some knobs may
  spend half their travel where nothing audible changes, or jump too far per detent where
  the ear is sensitive (filter cutoff at the low end, resonance near self-oscillation,
  detune, delay time, pulse width near 0 and 100%).
- **Action** (alongside the UX pass, not a feature milestone):
  - for every knob-controlled parameter, define the **useful range** (the span with
    perceptible, musical change) and a **response curve** across it (linear, log for
    frequencies and times, S-curves where the ends matter less), so equal turns give
    roughly equal perceived change;
  - set per-detent step sizes on the encoders and fine/coarse behaviour (e.g. hold a
    button for fine), and smoothing where a jump would click;
  - measure, don't guess: render each parameter's sweep on the host and check the
    perceptual spacing (spectral centroid, loudness, pitch in cents, delay ms) per detent,
    with a test that flags dead zones (several detents with no measurable change) and
    jumps (one detent with a large change);
  - MIDI keeps full resolution through NRPN; the curves shape the knobs and CCs, and the
    generated `midi.md` documents each curve;
  - check by ear on hardware with players, recorded in `docs/hardware-test.md`.
- The user's priority (2026-10-16): after the features, with the UX pass.
- **Done (ergonomics pass, ergo-live):** K1 walks 64 detents of an even-in-pitch cutoff table
  per chip model (the 8580's first 29 detents are single registers, the chip's own resolution,
  which the plan's table rule gives: see the ergo-live report), MASTER has a -60 dB..0 dB audio
  taper and reads in percent, K3 is the sound's own control (the pulse width in 31 positions of
  1.6 %, the decay in 16, or the sync ratio in semitones) and K4 the release.

## A UX pass: make SIDOU a joy to use (after the features)

- **The harness for it is built:** `cargo run --release -p ergo` (host/crates/ergo,
  [TESTING.md](../TESTING.md#the-ergonomics-harness-ergo)) measures readability (text scale,
  glyph size in mm, contrast), knob and encoder feel (steps in cents, dB and %, dead zones,
  curves), effort per common task, feedback speed, consistency across views and the player's
  journeys, and writes a report with a stable id for every finding. Run it before and after each
  UX change, with `--baseline`, and use its findings as this pass's starting list.

- **Where:** every view, `docs/reference/controls.md`, the design doc's UI section.
- **Problem:** the features are built control by control; nobody has yet looked at the whole
  as a player would. Gestures have piled up (EDIT layer, SAVE held, HOME held, menus, CHIP
  focus), and there is no help on the device.
- **Action (after M2 and M3's features, before the first release):**
  - a UX review of every view and gesture with fresh eyes: what a first-time player tries,
    where they get stuck, which gestures clash or are hard to remember; simplify and make
    controls consistent across views;
  - **integrated help cards**: a press-and-hold (or a HELP gesture) shows a card for the
    current view, in the font and Metal Chrome style, naming what each knob and button does
    *here*, generated from the same tables the panel uses so it can't drift;
  - first-run guidance (the full boot once, then a short "turn K1" tour), clear toasts,
    undo where it matters, and consistent naming;
  - usability checks with real players on hardware, recorded in `docs/hardware-test.md`.
  - **suggested automations**: notice what the player keeps doing and offer to do it for
    them, never on its own. Examples: the same knob swept over the same bars every loop →
    "record this as an automation (per-step locks)?"; the same chord typed repeatedly →
    "make it an arpeggio?"; a filter opened at every song start → "save it in the song?";
    a CC learned and moved like a mod wheel → "map it to vibrato depth?". Suggestions are
    one-line toasts with a single yes gesture, dismissable, learned per session, cheap
    (main-loop pattern counters, no audio cost), and off with one setting.
- **The guiding principle: the antithesis of menu diving.** Everything a player reaches for
  while making music is one gesture away, on the panel, never three pages deep in a menu.
  Menus hold only set-and-forget settings. Measure it: for each common task (change the
  sound, tweak an effect, save, learn a CC, mute a voice, change pattern), count the
  presses from wherever the player is, and cut every path longer than two. The screen always
  says what the knobs do right now, and nothing is ever hidden behind a mode the screen
  doesn't show.
- The user's priority (2026-10-16): finish the features first, then this pass.
- **Done: help cards and the first-run tour (ergonomics pass, flow).** HOLD HOME 600 ms shows a
  card for the view that shows (docs/reference/controls.md "Help"), HOME acting on its release so
  a tap still goes home; after the full boot an install plays, three toasts (TURN K1, PRESS ENV,
  HOLD HOME FOR HELP) lead the way ("At power-on"); title cards play once per power-on by
  default (CARDS: OFF, ONCE, ALWAYS).
- **Done: readability on the panel (2026-10-18).** The first hardware run found the 8×8 font on
  the 30×30 grid almost unreadable on the 1.3-inch panel. Large text (2× and 3× the font, built
  from cells), a value popup over any view when the panel changes a parameter, the menu at
  2× with scroll hints, and large toasts are in (docs/art/README.md "Large text",
  docs/reference/controls.md "Value popup", the readability check in docs/hardware-test.md
  section 6).
- **Done: the dense screens redrawn for 23 mm (2026-10-18).** LIVE, TRACKER, CHIP and STATUS
  follow the screen-readability prototype (assets/prototypes/screen-readability.html) the user
  chose: a high-contrast theme by default (SETTINGS' THEME keeps the C64 look), voice and knob
  colours the same everywhere, LIVE's lanes, thick scopes and knob row, one voice of TRACKER at
  2× with strips and SELECT to switch it, CHIP a section at a time with its four controls at 3×
  (docs/art/README.md "Theme", "LIVE", "CHIP", "TRACKER"). The panel check is in
  docs/hardware-test.md section 6 ("Readability of the redesigned screens"). Left for the
  panel's verdict: ABOUT's credits still fade a row at a time at the font's size (its first
  page's heading is at 2× since the ergonomics review's E20), and the small text (a lane's waveform and stage, the knob names,
  the footers) is still 8×8.

## A soft knee on the two chips' mix (if two loud chips clip)

- **Where:** `app/chip_mix.c` (`chip_mix`), decision record 0004 ("M2b Task 3").
- **Problem:** two chips at their loudest and in phase clip at the mix by up to 3.3 dB (one
  chip's loudest steady sample on the bench is 20,274). A gain on the mix would change chip
  A's level each time chip B goes in or out of use.
- **Action (the review's proposal, if it is heard on hardware):** a hard knee on the sum:
  identity below +-24,576, half the slope above it. About +2 to 4 instructions a sample on
  the mix path only (chip B in use), and no golden moves (none of them has the sum above the
  knee).

## Apply for a pid.codes PID before 1.0

- **Where:** `platform/fm1/src/usb.c` (Felucca's, unmodified), the device
  descriptor (`FELUCCA_USB_PID`, which `midi.c` can define before the
  include).
- **Problem:** SIDOU enumerates as VID/PID 0x1209:0x0001, pid.codes' test
  id, as Felucca does. It is meant for the bench only, and hosts may mix
  SIDOU up with Felucca or anything else that uses it.
- **Action:** apply for a pid.codes PID before 1.0 (an open-source project
  qualifies) and set it in the descriptor. The update loader enumerates as
  0x1209:0x0002 (as Felucca's loader) and needs its own PID too.

## USB audio out, so a computer can record SIDOU over the cable

- **Where:** `platform/fm1/midi.c` (`FELUCCA_UAC 0` before it includes
  `src/usb.c`) and `platform/fm1/audio.c`.
- **Problem:** SIDOU's USB is MIDI only. `midi.c` builds Felucca's
  `src/usb.c` without its USB audio input (`FELUCCA_UAC`), so a DAW can't
  record the FM-1 without an audio interface. `usb.c` has it,
  tested on units: class compliant UAC1, the master output as 16-bit stereo
  at 44.1 kHz on an isochronous asynchronous IN endpoint (EP4), 48 kHz
  through a polyphase resampler, one packet per USB frame from the TIMER5
  ISR, and a sample ring filled by the audio render.
- **Action:** bring the UAC path back into SIDOU's copy as a composite
  device with USB-MIDI, record the change in `VENDORED.md`, and tap the
  post-slot stereo output.
  - **Audio budget:** measure the ring tap and the packet ISR against the
    audio budget gates (the tap adds to the 2,700 sum, as the scope tap does).
  - **RAM:** account for the ring.
  - **Testing:** fm1-emu has no USB (see below), so host-test the ring and
    resampler, and add hardware checks to
    [hardware-test.md](hardware-test.md): enumeration as a class-compliant
    audio device on macOS, Windows, Linux and iPad; 44.1 and 48 kHz;
    recording in a DAW with no dropouts while the LCD redraws; USB-MIDI
    still working alongside.

## A browser editor and librarian over USB (after M2e)

- **Where:** a new page under `web/`, using M2e's SysEx dump protocol
  (`docs/plans/2026-10-15-m2e-midi.md`) and design doc section 11's web companion.
- **Problem:** tracking on four knobs and a 240×240 screen is slow. A computer
  keyboard and a large screen make song and instrument editing far quicker. SLICE64-FM
  (an FM-1 tracker on fm1-editor.com's watch list) shows the value of such an editor.
- **Action:** a Chrome/Edge page using Web MIDI with SysEx. It lists the device's songs
  and profiles; sends and receives `.sng` and `.sid` files; edits instruments, tables and
  patterns with a keyboard; and offers a "plain GT2 export" carrying
  `gt2_sng_check_plain`'s warning. It talks only to a SIDOU that answers the identity
  query, never to the update loader, and never sends the mask-ROM key. Plan it as M2g or
  early M4.

## Mirror the screen over USB

- **Where:** `app/grid.c` (the renderer already sends only changed 8×8 tiles) and a USB
  path next to USB-MIDI (a vendor bulk interface, or CDC; SysEx is too slow for
  full-rate frames).
- **Problem:** demos, screen recordings and the browser editor would benefit from seeing
  the device's screen live on the computer.
- **Update, 2026-10-18:** now FM-1 Live Link in the roadmap (`plans/2026-10-18-roadmap-after-m2.md`,
  Alongside): changed cells rather than tile pixels, a few hundred bytes a frame.
- **Action:** an opt-in mode that copies each sent tile run to USB as well (about
  2.5–20 KB a frame at typical change rates), plus a viewer in the web companion. Measure
  the USB cost against the main-loop budget, and add hardware checks. The same transport
  is half of the separate "remote panel" idea: fm1-emu issue #4.

## Licence labels that don't match the code's origin: done (2026-10-22)

Closed by the end-of-M2 review (section 7). Relabelled: `ota_flash.h` (Felucca's `ota_hw.c`,
GPL-3.0-only, Leo Kuroshita's line), `sid_osc.h` and `sid_env.h` (reSIDfp's
`WaveformGenerator` and `EnvelopeGenerator`, GPL-2.0-or-later with its authors' lines),
`gt2_sng.h` and `gt2_song.h` (Lasse Öörni's line), `usb/midi_uart.c` (SIDOU's line added to
Felucca's). LICENSING.md has rows for each. Judged not derived, labels kept: `sid_filter.h`
(a Cytomic/Simper trapezoidal filter plus SIDOU's fit to reSIDfp's output), `sid.c` (its own
structure), `sid_character.h` (SIDOU's design), `gen/grid.h` (macros and enum names),
`gen/profiles.h` (parameter values, which are facts). The byte-identical Felucca copies keep
their own `Copyright (C)` lines, which REUSE accepts (the vendor-verbatim rule).

## Decide on RSID playback (M3 plan)

- **Update, 2026-10-18:** with the look-ahead of decision record 0008, the 6502's audio
  headroom is no longer the limit; CIA and NMI timing within a frame is.
- **Where:** M3's 6502 and `.sid` player (design doc section 5 currently rejects RSID).
- **Problem:** RSID tunes need CIA timer and NMI emulation, and some need cycle-exact
  interrupt timing. The 6502 has only about 200 instructions a sample of headroom (see
  "The 6502 must fit the audio headroom" below). SLICE64-FM claims RSID playback.
- **Action:** when writing M3's plan, measure what CIA, IRQ and NMI emulation costs, and
  decide: full RSID support, a common subset (CIA-timed tunes without cycle-exact NMI
  digis), or keeping the polite rejection.

## ~~A friendlier ARP editor in TRACKER (M2d)~~

- **Done:** TABLE's ARP helper on the WAVE table writes the wavetable rows from semitone offsets, a
  speed and a loop step (plain GT2) and reads such rows back ([controls](reference/controls.md#arp-helper)).
- **Open, small:** the waveform is kept, not chosen in the helper (K1 on the table's first row
  changes it); no audition of the draft before SAVE; at most 8 steps.

## Generate the active profile's combined-waveform rows into POOL (M2/M3 fallback)

- **Where:** `core/sid/gen/pulldown.h` (`SID_PD`),
  `sid_osc_set_profile` (`core/sid/sid_osc.h`), `chips` in POOL
  (`platform/fm1/audio.c`), and `sidou-tablegen`, which samples the tables
  from reSIDfp.
- **Problem:** the combined-waveform tables are flash data. The firmware holds
  the 6581's three (its chip profiles play WEAK, AVERAGE and STRONG) and the
  8580's AVERAGE: 122,880 bytes as rows, 39,256 since M2 shares their
  16-entry blocks (decision record 0003, Costs). If M3's 6502 needs the
  flash, they can go. The audio interrupt also reads them from
  flash, a row lookup per combined-waveform sample, whose cost is M1b's open
  hardware check ([hardware.md](hardware.md), "Combined waveforms from
  flash").
- **Action:** if the flash runs short, or the hardware shows the ISR's
  flash reads cost, generate the active profile's rows into a POOL buffer
  instead (at a profile change, in the main loop, before the spare chip
  swaps in): 30,720 bytes of RAM per model in use, from POOL's ~115 KB
  free (M1c Task 3: 228,212 of 344,064 bytes used). That needs the rows'
  generator in C (reSIDfp's `WaveformCalculator`, its per-profile
  parameters as data), and it takes the ISR's reads out of flash. Keep the
  output exact: OSC3 against reSIDfp at each profile
  (`tests/profiles.rs`) and the golden hashes.

## The GitHub avatar and social preview are set by hand

- **Where:** the repository's settings on GitHub (Settings, General, Social
  preview) and the owner's account or organisation picture.
- **Problem:** neither can be set from the repository: GitHub shows a
  generic preview and the owner's own picture until someone uploads them.
- **Action:** the owner sets them manually: the avatar from
  `docs/art/avatar.png` (48 x 48, Metal Chrome's S; scale it up by nearest
  neighbour if a larger upload is wanted), and a social preview from
  Metal Chrome (`assets/logo/metal-chrome.png` or the 2x lettering on
  black, padded to GitHub's 1280 x 640). Redo both if the logo changes.

## fm1-emu does not emulate USB

- **Where:** fm1-emu at rev `e52f4640157f490f8c3af778dd044e9d33e989e8`, and still at
  `db8d472e` (2026-10-22).
- **Problem:** the USB0 registers (`0x11800..0x1183F`) are plain storage:
  the SIE never reports a finished access, so SIDOU's USB stack times out
  at boot, leaves USB off and retries once a second. No USB MIDI reaches
  the firmware, and the cost of `usb_poll` with a host attached cannot be
  measured. The emulator tests play MIDI through the TRS jack only; the
  USB packet conversion and the MIDI ring are host-tested. The update
  protocol runs over USB only, so it cannot be driven in fm1-emu either: its
  step 1 (`ota.c`) is host-tested against packages from `sidou-pack`
  (`crates/sidou-pack/tests/ota_step1.rs`), and an install is a hardware
  check ([hardware-test.md](hardware-test.md)).
- **Since the USB fix:** `host/crates/emu-gate/tests/usb.rs` plays a host
  against a model of the controller in the test crate
  (`tests/common/usb_host.rs`, over fm1-emu's plain register storage):
  enumeration, the update protocol's handshake and first read request, and
  K1's and K4's MIDI out. The emulator's other tests still run without a
  host. The model is the firmware's side only: the chip's DMA and timing
  are not in it.
- **Action:** test USB MIDI on hardware ([hardware-test.md](hardware-test.md),
  section 8 and 8.1), and
  measure `usb_poll` there: three SIE round trips per poll at 2 kHz with
  no traffic. Ask upstream whether a USB device model is planned.

## USB retry without a host costs main-loop time on hardware too

- **Where:** `platform/fm1/midi.c` `usb_retry_quiet`, and `platform/fm1/src/usb.c`
  (Felucca's) `usb_retry` and `usb_start`.
- **Problem:** Felucca found that the SIE does not answer without a cable,
  which is also how a battery-powered FM-1 runs. While USB is off,
  `usb_retry_quiet` runs once a second: `fm1_usb_reset` (controller off, pads
  detached), then 25 ms later `usb_probe`, whose `fm1_usb_attach` turns on
  the PLL48 USB clock, the PHY and the D+ pull-up, and whose one SIE write
  times out at 2000 polls; then the reset again, so the controller is off
  between tries (the old fork left the clock, PHY and pull-up on for about
  975 ms of every second). The probe is 8,040 instructions in fm1-emu. The
  supply current with no cable is unmeasured.
- **Action:** on hardware without a cable, measure the probe's time and the
  supply current, and check that a cable plugged in later enumerates within
  a few seconds (the probe's 2000 polls must be enough for an SIE that
  answers).

## SIE timeouts spin inside the TIMER5 interrupt

- **Where:** `platform/fm1/src/usb.c` (Felucca's), `sie_wr` and `sie_rd`,
  called by `usb_poll` from TIMER5 at 2 kHz.
- **Problem:** each SIE access waits for the controller for up to 20000
  polls. If the SIE stops answering while USB is up (a dead clock, an
  unplug the stack does not notice), every `usb_poll` spends several
  timeouts (each about 0.6 ms at 240 MHz, longer than the 0.5 ms poll
  period) inside TIMER5, which outranks the audio interrupt, until 50
  consecutive timeouts set `usb.up = 0`. That can starve the audio render
  for tens of milliseconds and drop blocks.
- **Action:** on hardware, unplug the cable during playback and while the
  host suspends, and watch for audio dropouts. If the SIE can die while up,
  shorten the in-interrupt timeout or give up after the first timeout of a
  poll.

## The device's flash head is never written

- **Where:** the update loader (`platform/fm1/loader/ldr_core.c`) and
  `host/crates/sidou-pack`.
- **Note:** the loader writes only the app area `[0x4000, 0x93000)`, as
  Felucca's does, so an FM-1 keeps its own head (M-VAVE's SPL and
  isd_config, with the 360 MHz clock setting) whatever a package carries.
  SIDOU's packages carry an SDK-default head built as Felucca's
  `fm1pkg_make.py` builds it (the SDK's SPL, Apache-2.0); no byte of
  M-VAVE's firmware is in them. The way back to stock V15 keeps `ota.c`'s
  head-identity check (V15's official loader rewrites the head, so its
  package must carry the device's head), host-tested with the V15 package
  (`crates/sidou-pack/tests/ota_step1.rs`).
- **Action:** none; any future SIDOU tool must keep the head unwritten.

## Recovery paths, if an FM-1 stops booting

- **Where:** the hardware checklist ([hardware-test.md](hardware-test.md)), before any
  hardware flash.
- **Note:** the boot guard enters UBOOT after two early deaths, and the
  update loader (Felucca's) stays in place until an install completes. If
  the device is still stuck, these community recovery paths exist:
  FM-1-transporter (<https://github.com/kurogedelic/FM-1-transporter>),
  and MvaveFM1Unbricker with jl-uboot-tool. How each is used, and in which
  device state it works, is still to be confirmed on a real unit. Return to
  stock with M-VAVE's updater or the installer's "Return to official V15".
- **Action:** check that each recovery path works on a real unit before
  SIDOU packages are offered to anyone ([hardware-test.md](hardware-test.md),
  sections 0, 2 and 10).

## The identity FM-1_900 is shared with Felucca's development builds

- **Decided 2026-10-23:** release builds answer **`FM-1_964`** (`SIDOU_PRODUCT` and
  `sidou-pack --product`); development builds keep `FM-1_900`. There is no registry: of the 80
  Firmware Hub packages checked, 17 ship `FM-1_900` and the rest picked a number, some colliding;
  none uses 964. Set it in the v0.3.0 release build (preflash accepts `FM-1_9XY`).

## Hardware results

- **Where:** [hardware-test.md](hardware-test.md), filled in and attached to
  an issue.
- **Status:** none recorded yet; no FM-1 has run SIDOU. Hardware flashing
  waits until a tester with the recovery tools at hand has run sections 0
  to 2, 9 and 10.
- **Open hardware-only checks it collects:** install and resume (the loader
  appearing as Felucca's), the way back to V15, the boot guard ending in
  UBOOT, `.noinit` across watchdog and P33 resets, the watchdog in UBOOT,
  quick power cycles and brownouts, USB MIDI and its edge cases (no cable,
  unplug, host sleep, the SIE timeouts above), the TRS adapter type, the
  LCD at 12 MHz SPI, the button labels, and the measurements of
  docs/hardware.md's open checks (CPU clock, cycles against instructions,
  the SRAM audio path's gain, latencies, the part number).

## Report fm1-emu: decrementing register block load is not executed

- **Where:** fm1-emu (github.com/joakimeriksson/fm1-emu) at rev
  `e52f4640157f490f8c3af778dd044e9d33e989e8`.
- **Problem:** fm1-isa decodes `r_4_mov_r_4` (`{R*4} = [-R]`, mask
  `0x0804fff0`, match `0x0000eb40`). fm1-core executes only the `[R+]` and
  `[R++]` block moves, so stepping it fails with
  `Unsupported { name: "r_4_mov_r_4" }`.
- **Repro:** JieLi clang 4.0.1 `-mcpu=r3 -O2` emits `41 eb 0a 00`
  (`{r1, r3} = [-r1]`) when loading the previous element of a two-float
  array, as `interpolate()` in `core/sid/sid_filter.c` did until M1b Task 5
  (the bench image before that, `build/fm1/bench/fm1/sid_bench_1.bin`,
  entry `0x02000000`, faults at PC `0x02000e50`). Since Task 5 the cutoff is
  a table lookup and no SIDOU code emits it (the gates count 0 emulated block
  loads); the workaround stays for whatever emits it next.
- **Expected semantics** (what our workaround implements, and what the
  bit-exact host match confirms): the registers load in ascending order from
  `base - 4 * count`, and the base is not written back.
- **Our workaround:** `step_block_load_dec` in
  `host/crates/emu-gate/tests/common/mod.rs`, shared by the emulator tests.
  fm1-emu's own command line has no workaround: until Task 5 it stopped
  SIDOU in `sid_cutoff_hz` during boot (`fm1-emu --fwsc
  build/fm1/sidou.fwsc`). Remove it once upstream supports the instruction.
- **Recheck at `db8d472e` (2026-10-22):** still not executed: fm1-isa decodes `41 eb 0a 00` as
  `r_4_mov_r_4` and `Cpu::step` returns `Unsupported { name: "r_4_mov_r_4" }`, as at
  `7ca5b96c`. The workaround stays.
- **Action:** open an issue (and possibly a PR) upstream with the above, then
  bump the pinned rev.

## A gate that decodes every instruction of the image

- **Why:** a compiler idiom fm1-emu lacks stops the emulator only when a test happens to run
  its path (the and-not read-modify-write with r8-r15, `64 e8 5f bc`, stopped the keys test and
  the browser demo at the first key release in M2d Task 1; fixed upstream in fm1-emu #33, in the
  pin since `db8d472e`).
- **What:** an emu-gate test that walks `.text` and `.ram_text` and fails on any instruction
  fm1-core cannot decode or execute, naming the function from the symbol table. Take the
  boundaries from fm1-core itself (`fm1_core::decode_info`, public since fm1-emu 0.2.0, the
  decoder `fm1-emu --check-decode` uses), not from the vendor objdump: the objdump shows the
  6-byte compare-and-branch (`4x ff ..`) as unknown, and it disassembles the jump tables
  (`.GJTI*`) as code. A first try (2026-10-22) that stepped every objdump line from a fresh
  `Cpu` also flagged dual-issue tails, which run only after their head: step pairs together.

## fm1-emu does not model the watchdog

- **Where:** fm1-emu at rev `db8d472e` (2026-10-22).
- **Problem:** since fm1-emu #34 the P33 interface (`P33_CON` 0x13E08, `P33_DAT` 0x13E0C) is
  modelled as registers that keep what is written (`fm1-soc/src/p33.rs`; before it, reads
  returned 0 and SIDOU's accesses showed as unknown MMIO), but nothing acts on them: no
  watchdog and no reset through `PR_PWR`. So `fm1_wdt_arm`, `fm1_wdt_feed` and `fm1_reboot`
  (Felucca's `fm1_sys.h`) still have no effect, and the boot test cannot tell whether the
  firmware feeds the watchdog, or whether a crash would reset the chip.
- **Action:** ask upstream for a watchdog on the P33 model that resets the emulated chip when
  it is not fed; until then, check watchdog behaviour on the device.

## Possible fm1-emu contributions (only if profiling needs them)

fm1-emu still stops on some pi32v2 instruction classes: saturating `R += R`
variants, register `&=`/`^=` masks, and some FPU and accumulator classes. If
a profiled hot spot needs one of them in hand-written assembly, add support
upstream first, so the emulator gate's bit-exact host-vs-target check keeps
working.

## The 6502 must fit the audio headroom the gates leave

- **Update, 2026-10-18:** [decision record 0008](decisions/0008-m3-6502-and-fm1-sid.md)
  runs the 6502 a frame ahead outside the ISR, with cycle-stamped SID writes, so it no
  longer needs this headroom (about 76 instructions a sample of main-loop time in the
  study). The headroom still bounds applying the stamped writes in the ISR.
- **Where:** `host/crates/emu-gate/tests/slots_budget.rs` (it prints
  `headroom_for_6502_instructions_per_sample`), `tests/common/mod.rs`,
  [decision record 0004](decisions/0004-effect-algorithms.md), "Worst blocks".
- **Problem:** the switch gate holds two chips at the SID's worst block (1,426),
  the LIVE view's tap (4.5; on at power-on), the ISR (100) and the slots' worst
  switching block (964) to the 2,700 audio target, which leaves **205 per
  sample** for the 6502; chip B's mix in the device's interrupt (M2b Task 3, `chip_mix`,
  measured 8.1 a sample with both chips in use) leaves about **197** (M3; 191 after M1b, 227 before M1b Task 6's non-linear
  6581 filter, about 38 per sample per chip, and without the tap; M1c Task 3's
  fmax/fmin clamps in the 6581 filter gave back 14). The old budget reserved
  400. The SID's worst block is the power-on transient of noise combined with
  another waveform (the register shifting until it locks up), in a scenario's
  first block, now on the 6581 (`noise_tri_ring_0800`, 1,425.66, 1,439.67 after
  M1b; the 8580's is 1,410.05). Its profile (`noise_tri_ring_0800`, 8580, one
  chip, block 0, 88.4k instructions): `sid_osc_run_combined` 24,690,
  `sid_osc_advance_slow` 10,114 (the quiet-stretch step, ~60 per call),
  `sid_osc_cycle` 5,004, `sid_noise_clock` 777, envelope attack from 0xaa
  8,052, baseline 39,731 (measured on the 8580 before M1b Task 6). Two chips at
  1,237 or less would give back the 400 (1,232.5 with the tap); the 6581's
  filter was about 38 of each chip's share in M1b.
- **Action:** when the 6502 lands, measure its per-sample worst. If it does not
  fit the headroom, one or more of:
  1. rewrite the combined-noise path (`sid_osc_run_combined`,
     `sid_osc_bulk_limit`, `sid_osc_bulk_commit`) as one tight loop with the
     per-call constants in registers; the transient (about 40.6k above) must
     shrink by about 27% to reach 1,237 for two chips;
  2. give the envelope's attack from 0xaa a fast path (8.1k of every gated
     first block);
  3. have a slot switch wait a block when the SID's block is heavy.
  Keep the output exact (golden hashes, the bulk-against-cycle fuzz).

- **REC from the keys, for flash day** (M2d Task 6, the user 2026-10-21): the key-to-row
  timing is measured at the audio block (about 3 ms) with no latency compensation; try the feel
  on the unit on flash day. (Closed 2026-10-23, the ergonomics review's E03: TRACKER's cursor
  no longer stays pinned while recording; it follows the playing pattern and comes back when
  the song stops.)

## The end-of-M2 review's findings after v0.2.0 (before v0.3.0)

- **Where:** [docs/reviews/2026-10-22-m2-review.md](reviews/2026-10-22-m2-review.md).
- **In v0.2.0:** the five blockers (2.1, 3.1, 5.1, 5.2, 5.3 with 5.6), storage and SysEx
  1.1, 1.2, 1.3 and 4.1, and the licence labels (section 7).
- **Before v0.3.0, every other finding** (v0.3.0 ships with no known bug): the 31 minor
  ones, batched into the ergonomics/UX pass (5.4, REC into a pattern TRACKER doesn't show, is
  fixed: E03 below).
- **Fixed after v0.2.0 (the ergonomics review's E items, ergo-logic):** 1.5 (E01: SAVE, not a
  second turn, confirms STORE's format; NEWER for a newer SIDOU's store), 5.5 (E02: no song
  views over ABOUT's tune), 5.10 (E27: ABOUT keeps the song views' places), 3.2 (E04: REC
  past a repeated pattern's last row lands in its next pass), 3.4, 3.5 and 3.7 (E06: no
  parameter lock lost at a pattern boundary, to a pending edit or after an edit applies); 2.2
  (E07) was already fixed in v0.2.0 with 2.1; 2.3 (E08: a chip swap on a locked row keeps
  the locked cutoff and pulse); the ergonomics review's E09 (PWM BASS's modulation silenced
  narrow widths: it now never sweeps below 3 %); 5.7, 5.8 and 5.11 (E23-E25: REC's toast
  outside the song views, no count-in on entering one mid-song, the REC popup in Hz); 5.9
  (E26: ORDER keeps TRACKER's row), 5.13 (E30: the tour clears only its own toast), 5.12 (E29: toasts wait under a help card), 5.14 (E28: keys under a help card play the view
  under it); 3.3 (E05: REC takes the entry's transpose off the note it writes); 5.4 (E03:
  while REC records TRACKER follows the playing pattern, the cursor back when it stops).
- **Fixed on the gates track (before v0.3.0):** 6.1 (`FREE_RAM_MIN` 7,000 to 10,000, the
  overview's M2f row, and each region 4,096 to 5,120, M3's start target), 6.2
  (`IMAGE_CEILING` 512,000 to 506,000, M3's start target), and section 8's stale CMake cache
  (the image records its build options; `sidou-pack --verify` and `tools/preflash` fail a
  package whose options are not the defaults; configure warns).
- **Fixed on the minors track (2026-10-22):** 2.4 (no `idle` in the register ring's wait with a
  host), 2.5 (the peaks' take loses no block), 3.6 (a file's LOCK chunk checked as it is read),
  4.2 (CLOCK OUT's SPP after a SysEx reply), 4.3 (FB in IDLE starts the song), 4.4 (flash holds
  reported to the clock follower, whose fit keeps the ticks' time line), 4.5 (EP1 NAKs on
  SIDOU's ring), 6.3 and 6.4 (bounds measured again and tightened; TRACKER's "playing" cost now
  measures a moving bar), 6.5 (33b2bda's goldens recorded in the review; the song plays on
  under REC), 6.6 and 6.7 (LIVE's goldens shot at a fixed place in the audio blocks), and
  roadmap item 6's ten-minute clock-drift test. E03 and E05 stay the user's call.
- **From the v0.2.0 fixes:** a sector the store retires for the session (review 1.2) shows
  only in `store_stats` and a lower free figure in MENU; STATUS should say it. A background
  settings save that fails is still not detected (explicit saves report and retry).


## Readability calls for the user (from the 2026-10-22 ergonomics fixes)

- ~~**STEP's pads 10-16** show only their last digit at 2x~~: numbered by beat now (1 2 3 4
  four times; the user, 2026-10-23).
- ~~**Profile words** (BRIGHT, DARK, CLASSIC, CLEAN, GRITTY)~~: show on CHIP and the LIVE band
  (1x beside LIVE's 2x revision name), and now on STATUS and the PROFILE popup with the model
  ("6581 CLASSIC"; the popup at 2x).
- **Still 1x for lack of room:** TABLE's "WAVE 21 REL 00" line, STEP's ruler digits and lane line.

## SysEx over USB while a chord sounds (the user's call)

- **Done 2026-10-22:** SysEx over USB went from about 1 KB/s to 12 KB/s up and 27 KB/s down
  on a real FM-1, silent ([flash day](hardware-results/2026-10-22-flash-day.md)).
- **Open:** with sound playing, an upload keeps the 100 ms gap (`STORE_FORCE_GAP_MS`) between
  the audio holds its flash programs need: about 105 ms a packet, 28 s for 64 KB. A shorter gap
  during SysEx would be faster but dips the audio more often: an audio-feel decision for the
  user, after hearing the current behaviour on the unit.

## Help cards: context-sensitive, complete, and two pages where needed: done (2026-10-23)

The user's call: the 1x text at the foot of the views is unreadable on the device, so the help
card (HOME held) is the only on-device explanation. Done:

- **Context-sensitive** (`app/views.c` help_card_of): LIVE on a slot's page, CHIP by its focus
  (the tabs; the chip with K3 NOTE THUMP; a slot, K1-K4 named for its algorithm; EXPERT), STEP
  with a step held or recording, TRACKER with REC on (editing, or recording: K1/K2 locks and the
  quantise), SETTINGS on STORE; the SONG pages and the name editor had theirs.
- **Two pages** where eight lines aren't enough: the title says 1/2, SELECT turns the page.
- **Complete:** `help_view.rs` `every_control_a_view_takes_is_on_its_card` feeds every view's
  input handler, in every state, each button and encoder, and fails on a control without a line.
- **The 1x bottom hints are gone** from CHIP, STEP, TRACKER, ORDER, INSTR and TABLE (status lines
  stay), and **STEP's pads are numbered by beat** (1 2 3 4 | 1 2 3 4, the second and fourth beats
  shaded, each beat's first number brighter; the held step's number is the HELD STEP frame's).
- ABOUT's first page, drawn whole, has its golden (`about-first.png`): the 2x heading clears the
  logo and the subtitle.

## CHIP SPREAD: dual-SID stereo (the user, 2026-10-23)

- **What:** a SETTINGS item **SPREAD** that pans chip A left and chip B right, from 0 (mono, today's
  sound, bit-identical) to 100 % (A fully left, B fully right), equal-power, before the effect
  slots, which take the stereo pair (Modulation and FX today take mono in: they need a stereo
  input path, or the spread goes after them; decide by cost, keep their bounds). A song on both
  chips, and a song on A with the keys on B, then sit apart, as on HardSID / 2SID setups. Saved
  with the settings; a CC/NRPN like the other settings; on the SETTINGS help card.
- **When:** after the help-card track merges (both touch SETTINGS and the cards). Then a video
  moment (two SIDs, left and right).
- **Done** (2026-10-23): after the slots, not before them (each slot's wet path takes the two
  chips together, the dry paths keep the spread: Distortion and Modulation on a stereo input
  would cost about 300 instructions a sample at their heaviest); [decision record
  0004](decisions/0004-effect-algorithms.md), "The output stage: CHIP SPREAD". The video moment is still to
  do; docs/hardware-test.md section 15 for the unit.

## From the v0.3.0 device session (2026-10-23)

- **ARP's state:** a held chord at the default 1F sounds like a wobble, and the 1x `ARP 1F UP 1OCT`
  line is easy to miss: default rate 3F, and a 2x ARP badge on LIVE while it is on.
- **SAVE as OK everywhere:** the SONG actions, the LOAD list and the confirmations still open with
  ALGORITHM only; SAVE opens the highlighted item on every list screen.
- **A fresh install's LOAD list is empty:** decided 2026-10-23: SIDOU LOOP is listed as a built-in
  and the demo's Songs page (`web/songs.html`) sends the showcase songs to a connected FM-1.
- **The LOAD list's names are 1x:** 2x like the menu and SETTINGS; the harness's tour never visits
  LOAD (or the other SONG pages), so add them to it.
- **Done 2026-10-23:** ORDER, INSTR and TABLE's status line at 2x on rows 28-29 (a longer table meaning
  stays 1x on row 29); TABLE's "K1 LEFT"/"K2 RIGHT" became "K1" "K2" at 2x beside the name.
- **Catch this class automatically** (the harness part is `tools/ergo-gate`, 2026-10-23, in
  TESTING.md "Ergonomics gate"; the controller wires it into the merge check): a test that SAVE opens and HOME goes back on every list
  screen; the ergonomics harness in the merge check, failing on any new finding against the last
  accepted run; first-run and per-feature recipes (a fresh install has something to LOAD; a
  feature that is on shows it readably).

## After v0.3.0: bump fm1-emu past its fixes for our reports

fm1-emu fixed #63 (FM-1 SID's psid_load bounds, PR #68) and #64 (the decrementing block load, PR
#67) on 2026-10-23. Bump the pin past them, remove `step_block_load_dec` from the emulator tests'
`common/mod.rs`, and check the gate is unchanged.

## SETTINGS → RESET (the user, 2026-10-23; after v0.3.1)

One reset for everything that isn't a song: every preset's edits and all settings back to factory,
with a SAVE-confirmed question (RESET ALL SOUNDS AND SETTINGS?); songs and USER chip profiles stay.
Today the only whole reset is a STORE format, which erases the songs too; per preset it is EDIT
held + PRESETS. A help-card line, controls.md, and a test that songs survive it.
