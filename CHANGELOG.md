<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]
### Added
- Design document, licensing, credits and contribution guidelines.
- SID core in C11 (`core/sid/`): oscillators, noise, hard sync and ring
  modulation, envelope generator with the ADSR delay bug, a
  zero-delay-feedback state-variable filter (its cutoff and resonance fitted
  to reSIDfp's filter since M1b; on the 6581 non-linear since M1b Task 6, its
  cutoff moving with the swing and the DC load, with reSIDfp's mix of the
  outputs and an output low-pass, scaled per chip by
  `sid_set_filter_nonlinearity`, 0 to 2),
  mixer and DC blocker, for the 6581 and the 8580.
- Rust host workspace (`host/`): FFI to the C core, unit and golden-render
  tests, and a criterion benchmark.
- reSIDfp reference harness: pitch and envelope tests, and the
  `sidou-compare` filter comparison tool.
- M1b, the SID core against reSIDfp (decision record 0003):
  - register reads: OSC3 (`$1B`) and ENV3 (`$1C`) bit-exact with reSIDfp,
    the paddles at 0xFF and the last bus value for the rest; the envelopes
    power on at 0xaa, and their timing follows reSIDfp cycle for cycle;
  - combined waveforms from reSIDfp's pulldown tables (AVERAGE by default;
    WEAK and STRONG with `SIDOU_ALL_COMBINED_PROFILES`), and the noise
    register's write-back and lock-up;
  - reSIDfp's kinked waveform and envelope DACs, the 6581's voice DC
    following its envelope DAC, and the output at reSIDfp's level;
  - the filter's cutoff and damping measured from reSIDfp and fitted, one
    row per curve setting (`sid_set_filter_curve`);
  - `sidou-tablegen`, which generates the tables in `core/sid/gen/`, with
    drift tests in CI;
  - tests: register reads against reSIDfp, the level of one voice, the
    filter's transfer functions, passband gain and small-signal, full-scale
    and loud sweeps, each held at its measured value.
- Pinned JieLi pi32v2 toolchain in Docker, and a bare-metal pi32v2 benchmark
  program.
- Emulator benchmark gate: runs the pi32v2 build in fm1-emu, enforces 900
  instructions per sample per chip, and checks the output against the host
  build.
- CI: host tests on Linux and macOS with UBSan; ASan on Linux; MSRV check, REUSE lint,
  and the firmware build with the emulator gate.
- Building and testing guides, and decision records for the host tooling,
  the M0 SID cost and M1b's accuracy results.
- FM-1 platform (`platform/fm1/`): Felucca's HAL (GPL-3.0-only), startup
  code and memory map, and a firmware image that boots, runs the audio
  interrupt and plays a SID test tone; an fm1-emu boot test checks it.
- The firmware plays the live instrument from the FM-1's keys, K1-K4, MASTER,
  OCT-/OCT+ and FX (6581/8580), lights the held keys and the octave, and shows
  a status screen on the LCD. Register writes reach the audio interrupt
  through a ring that never drops; a model change swaps to a chip prepared in
  the main loop. fm1-emu tests play it end to end.
- MIDI in over the TRS jack and USB (Felucca's USB stack, adapted: USB-MIDI
  only): notes on every channel, CC 74/71/1/7, program change; a lock-free
  ring from the 2 kHz polls to the main loop, one parser per source, exact
  recovery from lost input.
- The audio path (the ALNK0 interrupt, the render, the register drain) runs
  from SRAM; tests check that the interrupt runs nowhere else and that no
  call crosses between flash and SRAM code.
- The community SysEx update protocol, step 1 (Felucca's `ota.c`), and
  Felucca's update loader, unmodified and byte-identical to Felucca's own
  build; a boot-loop guard that sends a third early death in a row to UBOOT.
- `sidou-pack`: the `.fwsc` packager (a port of Felucca's), byte-identical to
  Felucca's packer on the same inputs; CI builds `sidou.fwsc`, boots it in
  fm1-emu and uploads it.
- The M1a hardware checklist (`docs/hardware-test.md`), checklist builds that
  die early on purpose while OCT- is held at power-on (`SIDOU_TEST_DEATH`),
  and a package SIDOU refuses (`sidou-pack --test-wrong-size`).
- Post-chip colour chain (`core/fx/`): bit-crush, tape saturation, wow and
  flutter, CRT/TV low-pass with mains hum, and a noise floor, as separate
  blocks with smoothed parameters and an exact bypass; an emulator gate
  scenario with every effect at full.
- Effect slots after the chip mix (`core/fx/fx_slots.h`): Distortion (tape,
  overdrive, fuzz, crush, lo-fi), Modulation (chorus/ensemble, flanger,
  phaser, tremolo, auto-pan, wow and flutter) and FX (delay, ping-pong, a
  light reverb ported from Emilie Gillet's Rings/Clouds reverb (MIT), and
  a Dattorro plate), each with Amount, A, B and Mix, smoothed and with
  crossfaded algorithm switches; stereo output; an emulator gate per
  algorithm and slot.
- The firmware runs the effect slots after the chip with true stereo output.
  Their settings ride the register ring as pseudo-registers; a temporary panel
  layer (EDIT + K1-K4, SCL, ENV) and CC 102-116 control them, and the status
  screen shows them (`docs/reference/controls.md`).
- The art direction (`docs/art/`): palette, 30x30 cell grid, motion rules and
  logo concepts. SIDOU's own 8x8 PETSCII-style font, generated from a glyph
  sheet, replaces the font8x8 placeholder. A cell-grid renderer sends only
  changed cells, in runs; the status screen runs on it, and fm1-emu tests
  compare the screen with golden images and bound what drawing costs.
- A view framework on the cell grid (`app/screen.h`, `app/widget.h`): a
  stack of views with focus, panel events routed to the view on top, PETSCII
  frames, menus with a blinking block cursor, a toast line, and wipe and
  raster transitions inside the SPI budget. HOME opens a menu (chip, preset,
  octave, the glyph sheet); every other control plays as before in every
  view. The overlay pool grows to 255 cells for LIVE's scopes.
- A boot sequence and an About screen (`app/boot.h`, `app/about.h`): the
  logo's title card at power-on, or with HOME held the full
  `LOAD"SIDOU",8,1` sequence with loading stripes; any control skips it
  without its action, and a note skips it and plays. ABOUT in the menu is a
  cracktro with raster bars, stars, a sine scroller and the credits,
  generated from CREDITS.md. The logo is a pluggable asset (`app/logo.h`).
- Cracktro Chrome, chosen by the user, is SIDOU's logo: bevelled chrome
  letters in raster bands with a purple shadow, over stars, raster bars and a
  grid floor (`assets/logo/`, its drawing code and indexed PNGs). The renderer
  gets a pixel region (`grid_region`): cells that take their background from a
  run-length coded bitmap in flash, with text and overlays still drawn over
  it; the scene is 4.5 KB, generated into `app/logo_bitmap.c` from its PNG.
  The title card is the whole scene, revealed under the wipe; About is laid
  out around its lettering band, with the bars behind a white scroller, and
  costs less than with Pulse Train, which stays as the alternative. The
  README carries the lettering, and `docs/art/avatar.png` the avatar.
- Metal Chrome, chosen by the user (replacing Cracktro Chrome), is SIDOU's
  logo: spiked, blade-cut metal letters with a red to white chrome bevel and a
  purple shadow on a dithered purple and red glow, with raster bars, stars, a
  cross-flare glint and a tagline (`assets/logo/metal-chrome.js` and its
  PNGs, pixel for pixel Chromium's drawing of the logo page). It is the boot's
  title card and About's lettering band (9 rows now), the README's lettering
  and the avatar; Cracktro Chrome and Pulse Train stay as alternatives. Its
  scene is 9.8 KB of flash (its glow's dither makes short runs), 14.2 KB with
  Cracktro Chrome's in one generated `app/logo_bitmap.c`; the decoder stores
  a run of one or two pixels without a loop, so the boot's busiest `ui_poll`
  stays at 33.5k instructions.
- LIVE, the screen at power-on (`app/live_view.h`): an oscilloscope per voice,
  envelope bars, the filter's curve, VU meters and the effect slots, after the
  prototype's. The audio interrupt taps the voices every 16 samples into a
  lock-free ring (`app/scope.h`), from inside one render a block
  (`sid_render_tap`), only while LIVE shows (4.44 instructions a sample, which
  the effect gates count), and the scopes fold the points by the oscillator's phase, so a held
  note stands still. LIVE holds itself to 200 cells a frame; the status screen
  moves to the menu (STATUS).
- SCENE, from the menu (`app/scene_view.h`): the prototype's five generative
  pieces, FIRE, PLASMA, RULE 30, STARS and ANT (`app/scene.h`), reacting to
  the voices' note-ons, envelopes (LIVE's tap, which runs while SCENE shows)
  and pitches, and to the cutoff and resonance; SELECT steps between them.
  FIRE and PLASMA are trimmed to the SPI budget (half their rows a frame, a
  smaller fire, a slower plasma), and SCENE holds itself to 200 cells a
  frame, leaving out what a toast or MIDI learn's frame covers (as LIVE
  does). A piece starts in the frame after SELECT, not in the input's call.
- Title cards (`app/title_card.h`): a short card in Metal Chrome's lettering
  on a dithered purple and red glow with stars when a major view is entered
  from the menu: THE SCENE before SCENE, THE OSCILLATOR before LIVE back from
  the menu (not at power-on), and THE CHIP ready for the CHIP view. The glow
  fades in through four dither levels drawn with the font's dither glyphs,
  the title wipes in, it holds a second, and the view cuts in in bands (a new
  transition, `SCREEN_BANDS`); about 2.6 s. Any control skips it and does
  nothing else, a note plays and ends it, and MIDI and the audio are never
  held up. CARDS in the menu turns them off (on by default). The titles are
  pre-rendered (`assets/logo/title-*.png`, from `metal-chrome.js`) and
  converted into `app/title_card_bitmap.c`: 27.1 KB of flash for all three.
  Each frame sends at most 200 cells (17.1 ms of SPI); the busiest `ui_poll`
  is 71.6k instructions, the input and the busiest main-loop pass 73.4k,
  within the 120k poll allowance. A bitmap region grown over the same cells
  sends only what it adds.
- MIDI, M1d: every live, SID and effect-slot parameter in one CC/NRPN map
  (`app/midi_map.c`), with NRPNs at full resolution (11-bit cutoff, 12-bit
  pulse width), running NRPN/RPN state per channel and input, and the
  published chart (`docs/reference/midi.md`) generated from the table, with a
  drift test.
- Expression: velocity (to the sustain level, cutoff or pulse width, per
  preset), pitch bend (+-2 semitones by default, RPN 0 or NRPN 6/0 up to
  +-24), the modulation wheel and channel and polyphonic aftertouch (to
  vibrato depth, pulse width or cutoff), and a per-voice vibrato with rate,
  depth and delay.
- MIDI out over USB: the panel's parameter changes as CCs (or NRPNs), K4 as
  a program change, rate-limited, deduplicated and never echoing MIDI input.
- A note-on-to-sound latency test over emulated TRS MIDI, and decision record
  0005 (notes stay at block starts).
- ENVELOPE: INSTANT / AUTHENTIC (CC 26, NRPN 1/10). The SID core has a gate
  reset option, `sid_set_gate_reset` (off by default, so the core stays
  bit-exact with reSIDfp), that restarts the envelope's rate counter at each
  gate edge and skips the ADSR delay, for the attack at a note-on and the
  release at a note-off. The live instrument turns it on by default: INSTANT.
  AUTHENTIC is the chip's timing.
- MIDI learn: press SAVE, turn a knob (EDIT held for the effect slots), send a
  CC, and the knob's parameter follows that CC on every channel and both ports,
  besides its own; HOME or 10 s cancel, SAVE instead of the CC forgets one,
  SAVE three times clears all. A learned CC wins over the map's, panel changes
  go out on it, and a learned CC from MIDI is never echoed. In RAM until M2's
  storage; `docs/reference/midi.md` documents it from the code.
- Chip profiles and in-chip character (M1c Task 3): 6581 R2, 6581 R3, 6581 R4AR,
  8580 R5 and an artistic "worn 6581", each a set of reSIDfp settings (filter curve and
  range, combined-waveform strength, DAC ladder, 6581 non-linearity) generated by
  `sidou-tablegen`, and four controls: age/heat, DAC sloppiness, output thump and filter
  bleed, on CC 85, 86, 87, 89, 90 and NRPN 2/0..4 (MIDI only until the CHIP view). Tables
  are made in the main loop and swapped in at a block boundary; a control's change glides
  there over a third of a second, without clicking and without holding the main loop up; a
  profile change resets the chip. The 6581's three combined-waveform profiles are now
  compiled by default (the 8580's one). With thump on, the MASTER pot clicks at each volume
  step, as the chip does. Decision record 0006 has the sources, the measurements and the
  cost.
- The CHIP view (M1c Task 4, `app/chip_view.h`), the menu's first item, behind THE CHIP's
  title card: chip A's profile picker and model, its age/heat, DAC sloppiness, output thump
  and filter bleed as bars, and the three effect slots with their algorithm and four controls,
  named for what each does in that algorithm; chip B's tab greyed until M2. SELECT moves the
  focus between the chip and the slots, ALGORITHM steps the profile or the slot's algorithm,
  K1..K4 turn the focused section's four values. `panel_knob_param` follows the view that
  shows, so MIDI learn binds the knob the CHIP view turns (AGE for K1 on the chip) and MIDI out
  sends its CC. MIDI's changes show at the next frame. While MIDI learn's frame shows over it,
  SELECT and ALGORITHM still move the focus and step its choice, so K1..K4 pick on the page
  the focus gives. The page is built in a canvas and copied within 200 cells a frame (idle
  6.3k instructions a frame, nothing sent; the busiest frame 17.6 ms of SPI).

### Changed
- M2b: a profile or model step (the CHIP view's ALGORITHM, the FX button, CC 25, CC 85)
  prepares the new chip in the background, one slice of at most 40,000 instructions a 10 ms
  tick (7 slices, or 11 when the character's DACs are custom, 4 more the first time the 8580
  is asked for after power-on; one slice a main-loop pass at most), while the old chip plays
  on, and swaps it in when it is whole. No main-loop pass is over the poll allowance any more (the
  reset's pass was up to 471.5k), so a note arriving during a step keeps the normal latency.
  The screens show the model asked for at once.
- M2b: the register ring carries two chips, A and B (chip B's registers 0x40..0x58 and
  pseudo-registers 0xC0..0xC8; the effect slots' name no chip), in one ring so their writes
  stay in order; three sids (A, B and one spare they share, one preparation at a time) and
  three DAC tables (one each chip plays, one a ramp builds into, one build at a time). A sink
  is bound to a chip (`reg_ring_out.chip`); live stays chip-agnostic.
- M2b: the keys play chip A, or chip B while a song plays on chip A (the new menu item KEYS:
  AUTO, the default, or A); the keys' setup travels with them, chip B following it in the
  background (`app/keys_route.h`, `app/chip_ctl.h`), and the keys' chip goes first in the
  register ring so it behaves exactly as one chip alone. LIVE says KEYS:B while the keys play
  chip B, the status screen and the boot banner show both chips (their LCD goldens updated).
  Until M2c a song is a test hook, so the keys always play chip A.
- M2b: chip B renders on the device beside chip A while it is in use, mixed before the
  effect slots and faded in or out over one block; out of use it is not rendered and the
  output is chip A's bit for bit. It is made at boot with the keys' setup, prepared in the
  background. The LIVE view's tap can follow either chip.
- The menu's CHIP item opens the CHIP view; the 6581/8580 choice it held is the view's profile
  picker (and the FX button, as before).
- The EDIT layer (EDIT + K1..K4, SCL, ENV) is no longer a stopgap: it stays in every view as a
  shortcut to the selected effect slot, the one the CHIP view's slot focus shows.
- The fm1-emu credit is reworded so that it fits one About page.

### M1 status
M1 (design doc section 14: sound) is done on `main`, with these exceptions.
- **Done:** the SID core against reSIDfp and the five chip profiles (M1b, M1c Task 3); the
  character layer and the post-chip effects, as three stereo slots (M1c Tasks 1, 2 and 5), with
  the CHIP view on the panel (Task 4); the live instrument on the device, playable from the
  keys, knobs and MASTER (M1a); MIDI in on TRS and USB, the CC/NRPN map, expression, MIDI out
  and MIDI learn (M1d); and M1e's art direction page, font, widget and transition framework,
  boot, menus, About cracktro, LIVE's scopes, envelopes and VU, the title cards and SCENE's
  first five pieces (ahead of M2's SCENE view).
- **Deferred to M2 with chip B:** chip B was never in the design's M1 list; the CHIP view's B
  half and M1e Task 6's UI sounds on its spare voice wait for it.
- **Moved to M2:** the character's expert layer (every analogue parameter as its own control
  and NRPN); everything that needs storage: user-saved profiles, MIDI learn's mappings, the
  CARDS option, and the full boot once after an install; and About's music on the SID (design
  doc section 7), which M1e's plan left out and needs M2's song player.
- **Kept as an exception (retired in M2b, above):** a chip reset (a profile or model step:
  the CHIP view's ALGORITHM, the FX button, CC 25, CC 85) ran in one main-loop pass of up to
  471.5k instructions (worn at full character), about four times the latency gate's poll
  allowance: a note arriving then waited about 5 ms instead of 3.4.
- **On hardware only:** the whole checklist in `docs/hardware-test.md` is still to be run on a
  unit (install and recovery, the printed labels against the ids, SAVE's id among them, the
  LCD, the audio, TRS and USB MIDI, MIDI out, the latency's feel, the boot guard), and the open
  checks in `docs/hardware.md` (the real CPU clock, cycles against fm1-emu's instructions, the
  audio path from SRAM, the combined waveforms read from flash, the USB retry's stall, the
  interrupt latencies, the second core's RAM).
