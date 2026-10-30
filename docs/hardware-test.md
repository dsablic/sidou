<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU hardware checklist (M1 and M2)

**Emulator first** ([TESTING.md](../TESTING.md#emulator-first-and-the-first-use-tests)). Each
check below says what checks it:

- *Emulator:* the tests named check it in fm1-emu or on the host (paths from `host/crates/`;
  `emu-gate/tests/` runs the firmware image in fm1-emu, the others the C core on the host,
  `docs/recipes/` the ergonomics journeys in fm1-emu).
- *Device-only:* only a unit can check it, and why: the speaker and the room, the feel, the real
  LCD and LEDs as seen, the printed controls and their timing, real USB hosts and operating
  systems, the real flash chip, SRAM and power, install and recovery, external gear.
- *Emulator (no test yet):* the emulator could check it and no test does yet: a test to write,
  not a question for the user.

A device session starts only when everything marked *Emulator* passes
(`SIDOU_FM1_BUILD=$PWD/../build/fm1 cargo nextest run --release -p emu-gate`,
`cargo test --workspace --release` and `tools/ergo-gate build/fm1`). The user is asked only the
*Device-only* checks, and of a mixed item only its device-only part.

## Ergonomics: effects you hear on pick

- [ ] On LIVE with a held note, SELECT to Distortion's page and turn ALGORITHM one click: the
  sound changes at once (TAPE warms it). Turn on for OVERDRIVE, FUZZ, CRUSH, LO-FI: each sounds
  different, none silent; turned back to OFF the slot is bypassed. SELECT on to Modulation and FX
  and step their algorithms: each is audible.
  *Emulator:* `emu-gate/tests/fx_live.rs::a_slot_algorithm_on_live_is_heard` (ALGORITHM once on DIST's page selects TAPE at its musical start), `sidou-tests/tests/app/fx_live.rs::every_default_is_audible` (every algorithm of every slot adds an audible signal to a saw), `sidou-tests/tests/app/live_instrument.rs::a_silent_slot_picks_up_a_musical_start`. *Emulator (no test yet):* that the algorithms render different audio from each other.
- [ ] SAW LEAD, one held note, each distortion in turn (EDIT + ALGORITHM), EDIT + K1 from the
  bottom to the top: TAPE thickens and warms, OVERDRIVE and FUZZ go from clean to clipped at
  about the same loudness (not just louder), CRUSH goes from clean to a coarse 4-bit buzz and
  the note never vanishes, LO-FI's TV tone darkens; the popup says `TAPE` / `DRIVE 127` and so
  on. LO-FI's HUM and NOISE (K2, K3) sit under the note at the top, not over it. Then a chord
  and a song through OVERDRIVE: louder input clips harder, as a pedal does.
  *Emulator:* `sidou-tests/tests/app/fx_live.rs::every_distortion_amount_is_heard_on_a_saw_lead_note` (each Amount changes the wave by -15 dB of itself or more, the level by 6 dB at most, the note never silent), `emu-gate/tests/fx_knobs.rs::every_distortion_amount_from_the_edit_layer_is_heard` (the same from the panel, the DAC's output); *device-only:* how much drive is musical.
- [ ] Turn one control with EDIT held, then step the algorithm: the control keeps your value.
  *Emulator:* `sidou-tests/tests/app/live_instrument.rs::an_edited_slot_keeps_its_values` (a slot with a turned control keeps its values when the algorithm is stepped, also through OFF), `sidou-tests/tests/app/fx_live.rs::edit_held_turns_k1_k4_into_the_selected_slots_controls`.

## Ergonomics: DICE

- [ ] On LIVE turn PRESETS clockwise past NOISE PERC: the header reads DICE 01 and the sound
  changes; play a note. Turn again: DICE 02, a different sound each time, none broken or silent.
  *Emulator:* `emu-gate/tests/screen.rs::presets_reaches_dice_and_rolls` (header DICE 01 then 02, the screen changes, golden live-dice), `sidou-tests/tests/app/live_dice.rs::every_roll_stays_in_its_ranges`, `sidou-tests/tests/app/live_dice.rs::the_rolls_differ_with_the_seed`. *Emulator (no test yet):* the rendered audio of rolled sounds is never silent (the ranges keep the envelope and waveform sane, no audio is rendered).
- [ ] Turn PRESETS counter-clockwise (NOISE PERC) and clockwise again: the same DICE 02 sound.
  *Emulator:* `sidou-tests/tests/app/live_dice.rs::a_roll_is_kept_and_recalled` (back to NOISE PERC as it is; forward again the same roll, same number), `emu-gate/tests/screen.rs::presets_reaches_dice_and_rolls`.
- [ ] Set an effect slot, roll DICE a few times: the slot stays as you set it.
  *Emulator:* `sidou-tests/tests/app/live_dice.rs::a_roll_leaves_the_effect_slots_alone`, `sidou-tests/tests/app/live_dice.rs::every_roll_stays_in_its_ranges` (no slot touched over 10,000 seeds).

## Ergonomics: SCENE

- [ ] Open SCENE (FIRE) and play nothing: a low red flame flickers along the bottom; it is
  never dark. On entry, after the card, a toast reads FIRE.
  *Emulator:* `emu-gate/tests/screen.rs::fire_keeps_a_low_flame_in_silence` (red and light red cells in rows 22-28), `emu-gate/tests/screen.rs::entering_scene_names_the_piece` (toast FIRE once the card is past). *Emulator (no test yet):* the idle flame changes from frame to frame (flicker); *device-only:* how the flame looks on the real LCD.
- [ ] Turn ALGORITHM clockwise and counter-clockwise: the piece steps like SELECT, with its
  name in a toast.
  *Emulator:* `emu-gate/tests/screen.rs::algorithm_steps_the_piece` (PLASMA then back to FIRE, name in the toast).

## Chord arpeggio (ARP on LIVE)

- [ ] On LIVE press and release ARP: the badge under the band reads `ARP 3F UP 1OCT` at 2×,
  readable at arm's length. Hold C, E and G: one voice steps through the three notes, each one
  heard (ARP + K1 back to 1F: the C64's buzzy arpeggio chord), no click between them, the other
  two lanes stay dark. Let go: one release rings out.
  *Emulator:* `sidou-tests/tests/app/live_view.rs::arp_switches_the_arpeggio_and_held_turns_its_settings` (badge text `ARP 3F UP 1OCT` at 2x), `emu-gate/tests/screen.rs::live_shows_the_arpeggio_and_stays_within_its_budgets` (golden), `emu-gate/tests/live.rs::a_held_chord_with_arp_on_is_one_voice_cycling_its_notes_at_the_rate` (one voice steps C, E, G every 60 ms, gate held, other voices silent, gate off after release). *Emulator (no test yet):* the audio has no step at the note changes; *device-only:* the badge readable at arm's length, how the chord sounds.
- [ ] Hold ARP and turn K1 two clicks: the knob row shows ARP's page and RATE reads `3F`; the
  notes are now separate. K2 steps ORDER (UP, DN, UD, AS), K3 SPAN (1, 2 octaves). Let go of
  ARP: it stays on (a knob turned). Press and release ARP again: off, chords on three voices.
  *Emulator:* `sidou-tests/tests/app/live_view.rs::arp_switches_the_arpeggio_and_held_turns_its_settings` (held: ARP's knob row RATE, ORDER, SPAN; K1..K3 turn them; release keeps it on; press again turns it off), `sidou-tests/tests/app/live_arp.rs::turning_it_on_takes_a_held_chord_into_one_voice_and_off_gives_it_back` (chords back on three voices), `sidou-tests/tests/app/live_arp.rs::the_orders_down_up_down_and_as_played`.
- [ ] With a MIDI keyboard: CC 27 at 127 turns it on, and notes held there arpeggiate too. With
  a sequencer's clock running and the rate at `/16` (K1 past `8F`), the notes land on the 16ths.
  *Emulator:* `sidou-tests/tests/app/midi_rx.rs::midi_notes_arpeggiate_and_the_clock_steps_a_clock_rate` (CC 27 on, MIDI notes held arpeggiate, a 16th steps every sixth clock), `sidou-tests/tests/app/live_arp.rs::a_clock_rate_steps_on_the_midi_clock_and_on_frames_without_one`; *device-only:* a real keyboard and sequencer clock over the MIDI adapter.

## Ergonomics: menu spin

- [ ] Open HOME's menu and spin SELECT quickly down past the first six items,
  then immediately back up. Exactly one selected bar remains at each step;
  no old selected row stays highlighted. After stopping, every row settles to
  the selected page without stale labels. Repeat with a value popup visible.
  On OCTAVE, turn ALGORITHM, then immediately jump back to CHIP: the old bar
  clears before the popup opens; after the jump the whole popup appears within
  four UI frames. Repeat while changing another knob during an opening popup.
  *Emulator:* `emu-gate/tests/screen.rs::a_fast_select_spin_leaves_one_selected_line` (one selected bar every frame, forward and reversed), `sidou-tests/tests/app/popup.rs::a_popup_open_waits_for_a_jump_between_uncovered_menu_bars` (the popup fully drawn within four frames, one bar), `sidou-tests/tests/app/popup.rs::a_delayed_menu_popup_keeps_the_item_that_algorithm_changed`, `sidou-tests/tests/app/popup.rs::a_popup_delayed_by_continuous_menu_moves_does_not_expire_unseen`.

A manual checklist for SIDOU on a real M-VAVE FM-1. Anyone with a unit can
run it. Copy this file, fill in the boxes and notes, and attach it to an
issue. The [first hardware run](hardware-results/2026-10-17-first-flash.md)
booted and played, but USB-MIDI output failed and recovery to V15 was needed.
That partial run does not mark the items below passed for the current build.
The flash timing spike has not run on a unit: program/erase timings remain
the datasheet's provisional maxima, not measured results.

> **Risk.** Installing custom firmware can leave the unit unable to boot. Do
> it only if you can recover it (section 0) and return to stock (section 9).
> SIDOU and its update path come with no warranty (GPL-3.0). SIDOU is not
> affiliated with M-VAVE.

**Unit and build**

| | |
|---|---|
| Tester, date | |
| Unit (serial, colour, where bought) | |
| Firmware before (`fm1_install.py --info`: `FM-1_015` is V15) | |
| SIDOU commit, package SHA-256 (`sidou-pack` prints it) | |
| Computer, OS, MIDI interface | |

Mark each item **[x] pass** or **[x] fail**, and write what you saw.

## 0. Before you start: recovery

On the day, [flash-day.md](flash-day.md) is the one-page order of work around this
checklist, and `tools/preflash` checks the package and the recovery route first (files only;
it never touches a device).

The update loader writes only the app area, never the flash head, so the
unit keeps M-VAVE's SPL and settings. Once step 1 of an install commits, the
unit runs the update loader at every power-on until an install completes. If
the unit still ends up unable to boot, these community recovery paths exist.
How each is used, and in which state of the unit it works, is part of what
this checklist records.

- [ ] I have M-VAVE's official `FM-1.fwsc` (V15, SHA-256
  `db1642b2b6fa5c2cccb11ffd13878068bb28601678d3644049f99dc40e7edb8a`) for
  the way back.
  *Device-only:* a record that the file is at hand (recovery tools; `tools/preflash` and `ota_step1.rs` check the way back to V15 only on files).
- [ ] I have read the instructions of FM-1-transporter
  (<https://github.com/kurogedelic/FM-1-transporter>) and of
  MvaveFM1Unbricker with jl-uboot-tool, and have them ready.
  *Device-only:* external recovery tools read and ready (FM-1-transporter, MvaveFM1Unbricker).
- [ ] I have Felucca's installer (`tools/fm1_install.py` from
  <https://github.com/hugelton/Felucca>, needs `mido` and
  `python-rtmidi`), and `fm1_install.py --info` finds the unit.
  *Device-only:* the installer finding a real unit over USB (install tool, real USB host).
- [ ] **The first recovery route, from the USB fix on: OCT- at power-on.**
  With the unit off, hold **OCT- alone**, switch on, and keep holding for
  about a second: the screen stays black (the backlight goes off), and the
  unit appears on USB as the chip's ROM boot mode, `WL82 UBOOT1.00` (USB
  4C4A:8057). There `tools/fm1-unbrick-mac` (from a Mac) or
  MvaveFM1Unbricker (Linux, Windows) restores a package
  (`tools/fm1-unbrick-mac/README.md`). It is the first thing SIDOU runs at
  power-on, before anything that could hang or crash
  ([controls](reference/controls.md#at-power-on)), so it works whatever
  state the rest of the firmware or its update path is in. It needs a
  SIDOU build that has it (not v0.2.0-m2-preview), and it is checked on the
  unit right after the first install (section 1). Switching off and on
  without the key leaves the ROM boot mode with nothing written. The
  checklist's dying builds (section 10) leave it out.
  *Emulator:* `emu-gate/tests/bootguard.rs::oct_down_alone_held_a_second_at_power_on_enters_uboot` (the firmware asks for the ROM boot mode after a second of OCT- alone, backlight line high, nothing else runs, boot guard ran first), `emu-gate/tests/bootguard.rs::oct_down_let_go_within_the_second_boots_on`; *device-only:* the unit showing as `WL82 UBOOT1.00`, the unbricker restoring a package (ROM boot mode, recovery tools, real silicon).
- [ ] The fallback, for a unit without a SIDOU build that has the gesture: I
  can put the unit into ROM boot mode (`WL82 UBOOT1.00`, USB
  4C4A:8057) without the firmware's help: a Flipper Zero with
  [flipper-fm1-recovery](https://github.com/merthsoft/flipper-fm1-recovery), or the FM-1
  Transporter. From there MvaveFM1Unbricker writes a package from Linux or Windows, or from a
  Mac with `tools/fm1-unbrick-mac` (a Linux VM's USB passthrough did not work). This route
  recovered a unit on 2026-10-17: `docs/hardware-results/2026-10-17-first-flash.md`.
  *Device-only:* Flipper Zero or Transporter into ROM boot mode, MvaveFM1Unbricker from Linux, Windows or a Mac (recovery tools, real USB hosts).
- [ ] After switching firmware on a Mac, the old USB-MIDI device is deleted in Audio MIDI
  Setup (Window > Show MIDI Studio): the Felucca family shares USB 1209:0001 and CoreMIDI
  otherwise keeps the old record.
  *Device-only:* CoreMIDI's stale device record in Audio MIDI Setup (a real OS).

Notes:

## 1. Install

The package is `build/fm1/sidou.fwsc` (BUILDING.md) or the `sidou-fwsc`
CI artifact. Its update loader is Felucca's, byte for byte, so during the
install the unit appears as Felucca's loader.

- [ ] `fm1_install.py sidou.fwsc` starts: step 1 (SIDOU or stock reads the
  package and stages the loader) reports no error.
  *Emulator:* `emu-gate/tests/usb.rs::the_running_firmware_serves_an_updates_whole_step_one` (every read of step 1, the loader staged, the success, the record; the core reset into the loader is not modelled: the core waits for it), `sidou-pack/tests/ota_step1.rs::a_package_stages_its_loader_and_commits`; *device-only:* the real `fm1_install.py` on a real computer and OS.
- [ ] The unit restarts into the update loader. It enumerates as Felucca's
  loader, whatever the OS shows (expected: "Felucca Update", USB
  1209:0002, identity `ota-FM-1_900`). Record what you see:
  *Device-only:* how the OS shows Felucca's loader (USB 1209:0002, `ota-FM-1_900`; Felucca loader, real USB host); a record. The loader's bytes are checked by `sidou-pack/tests/ota_step1.rs::the_built_loader_is_felucca_1_6_1s`.
- [ ] Step 2 (the loader writes the app area) completes; the unit restarts.
  *Device-only:* Felucca's loader writing the real flash chip (flash timing, real silicon, restart).
- [ ] `fm1_install.py --info` reports `FM-1_900`. (Felucca's development
  builds answer the same; distinct digits come later.)
  *Emulator:* `emu-gate/tests/usb.rs::a_host_gets_the_identity_and_the_knobs_over_usb` (the running app answers the identity request with `FM-1_900`); *device-only:* the real installer's reply after a completed install.
- [ ] **Right after the first install of a build with the power-on UBOOT
  gesture, before anything else:** switch off, hold OCT- alone, switch on and
  keep holding. Within about a second the screen is black, and the computer
  shows `WL82 UBOOT1.00` (USB 4C4A:8057; on a Mac, System Information > USB).
  Record what you see:
  *Emulator:* `emu-gate/tests/bootguard.rs::oct_down_alone_held_a_second_at_power_on_enters_uboot`, `emu-gate/tests/bootguard.rs::oct_down_let_go_within_the_second_boots_on` (the firmware's request and the black backlight); *device-only:* what the computer shows (`WL82 UBOOT1.00`, USB 4C4A:8057), a record (ROM boot mode, real silicon).
  - [ ] `tools/fm1-unbrick-mac` (or MvaveFM1Unbricker) finds the unit there.
    Restore `sidou.fwsc` with it, or just switch off and on.
    *Device-only:* the unbricker finding and writing a unit in ROM boot mode (recovery tools, real USB host).
  - [ ] It stays in the ROM boot mode for five minutes (no watchdog reset).
    *Device-only:* the ROM's own watchdog behaviour over five minutes (real silicon).
  - [ ] Switched off and on without the key, SIDOU starts as before.
    *Emulator:* `emu-gate/tests/bootguard.rs::oct_down_with_another_key_or_none_does_nothing` (no key held: SIDOU boots with audio); *device-only:* a real power cycle leaving the ROM boot mode (real silicon).
  - [ ] OCT- tapped briefly at power-on, OCT- held with OCT+, and OCT- held
    with a note key each boot SIDOU as usual.
  If the gesture fails, stop here: recover with the fallback in section 0
  and report it.
    *Emulator:* `emu-gate/tests/bootguard.rs::oct_down_let_go_within_the_second_boots_on` (let go at 5 ms, 300 ms, 900 ms), `emu-gate/tests/bootguard.rs::oct_down_with_another_key_or_none_does_nothing` (OCT- with OCT+, OCT- with a note key, OCT+ alone, none), `emu-gate/tests/bootguard.rs::a_second_key_pressed_during_the_hold_cancels_it`.
- [ ] Optional, only if you know what it writes: the aroum/fm1-custom-fw
  flasher with the same `.fwsc`. Record whether it writes the flash head
  (0..0x4000); SIDOU's own path never does.
  *Device-only:* an external flasher (aroum/fm1-custom-fw) and what it writes to a real flash head; a record.

Notes:

## 2. Interrupted and refused installs

Do this only with section 0 ready. Step 1 is the part SIDOU runs (it reads
the package and stages the loader); step 2 is the loader's (it writes the
app area).

- [ ] Pull the USB cable during step 1 (while the installer reports its
  first read requests, before the unit restarts). Within about 15 s (three
  unanswered reads of 5 s) SIDOU gives up: it erases what it staged, shows
  "FAILED" with a code, and reboots into SIDOU. Plug the cable back:
  `fm1_install.py sidou.fwsc` then installs normally.
  *Emulator (no test yet):* a host that stops answering the read requests of step 1: after about 15 s (three unanswered reads of 5 s) the staging area is erased, the screen shows FAILED with a code, the unit reboots into SIDOU, a new install works (`emu-gate/tests/usb.rs::the_running_firmware_serves_an_updates_whole_step_one` has the host side to build on); *device-only:* the physical cable pull.
- [ ] Power-cycle the unit during step 1: it comes back as SIDOU (its first
  boot clears a staging area left behind), and an install afterwards works.
  *Emulator (no test yet):* boot over a flash that holds a half-staged update (`0xE0000..0xE5000`): `ota_boot_cleanup` erases it and a later step 1 works (`app/store_layout.rs` only checks the area's place); *device-only:* a real power cut mid-write (real silicon, flash timing).
- [ ] A refused package: build one with `sidou-pack --test-wrong-size` (its
  flash.bin is 4 KiB short) and install it with `fm1_install.py`. SIDOU
  refuses it before erasing anything: the screen shows "UPDATE" and
  "FAILED -15" for about 1.5 s, then the unit reboots into SIDOU as normal.
  *Emulator:* `sidou-pack/tests/ota_step1.rs::a_wrong_size_package_with_the_built_loader_is_refused_before_anything_is_erased` (FAILED -15, flash untouched, the loader also refuses with -14), `sidou-tests/tests/app/views.rs::the_update_box_shows_each_step_and_the_result` (the box text FAILED -15). *Emulator (no test yet):* the 1.5 s of that screen and the reboot into SIDOU.
- [ ] Pull the USB cable during step 2 (the write). Plug it back: the unit
  comes up as the update loader again, and running `fm1_install.py
  sidou.fwsc` again finishes the install.
  *Device-only:* Felucca's loader interrupted while it writes the real flash (loader, real USB host, real silicon).
- [ ] Power-cycle the unit during step 2. It comes up as the update loader
  again, and the install finishes as above.
  *Device-only:* power cut while Felucca's loader writes the flash (loader, power cycle, real silicon).

Notes:

## 3. Boot

Check the full sequence first, then let the instrument sit quiet for at least 6 s so its
version can be saved. The title-card-only checks below apply to subsequent boots of the
same version with a writable store.

- [ ] With no saved version, or after installing a different firmware version, the full
  sequence plays without holding HOME (the sequence described below). After a quiet save,
  power-cycle: only the title card plays. Reinstalling the same version keeps the short
  boot; installing a different version gives the full boot once again.
  *Emulator:* `emu-gate/tests/settings.rs::the_full_boot_plays_once_after_an_install_then_the_short_one` (blank flash: full boot, version stored, then the short boot; an older version's record: full boot once, then short), `sidou-tests/tests/app/settings.rs::a_first_boot_plays_the_full_boot_and_saves_the_version_then_the_short_one`.
- [ ] The title card appears: Metal Chrome's scene, the spiked SIDOU letters
  with their red to white chrome and purple shadow on a dithered purple glow,
  between raster bars, with stars, a glint on the U and the tagline "FORGED IN
  SILICON FOR THE FM-1" (as `assets/logo/metal-chrome.png`; look for wrong
  colours, a shifted row, torn cells or a dither that shimmers), the version at the top and "PRESS ANY KEY" blinking at
  the bottom. Time from power-on to it:
  *Emulator:* `emu-gate/tests/screen.rs::the_short_boot_shows_the_title_card_then_live` (golden boot-title, the card is the logo source pixel for pixel, ends at 1.6 to 2.2 s); *device-only:* colours, shifted rows, torn cells and dither shimmer as seen on the real LCD, and the time measured on the unit (a record).
- [ ] About 1.2 s later LIVE wipes in from the top: C64 blue, a light blue
  header line "LIVE ... 6581 SAW LEAD ... OCT+0", three voice boxes "V1 SAW"..
  "V3 SAW" with a flat yellow line each, FILTER LP and VU boxes, and the
  DIST, MOD and FX lines at the bottom.
  *Emulator:* `emu-gate/tests/screen.rs::the_short_boot_shows_the_title_card_then_live` (the boot ends at 1.6 to 2.2 s into LIVE, golden live); *device-only:* how the wipe looks on the real LCD.
- [ ] A note key on the title card: it plays at once, LIVE wipes in, and
  voice 1's scope shows the saw.
  *Emulator:* `emu-gate/tests/screen.rs::a_note_skips_the_boot_and_plays` (LIVE on top, voice 1 names the note, peak over 1000), `emu-gate/tests/midi.rs::a_trs_note_on_during_the_title_card_plays_and_ends_the_boot`. *Emulator (no test yet):* the voice 1 scope shows a saw.
- [ ] OCT+ on the title card: LIVE wipes in, and the header stays OCT+0
  (the press only skipped). The same with FX: LIVE stays (LAB does not open).
  *Emulator:* `emu-gate/tests/screen.rs::a_button_skips_the_boot_and_does_nothing_else` (OCT+ and FX each end the boot and LIVE matches the golden with OCTAVE 0 and 6581).
- [ ] Tap HOME as soon as the title card shows: LIVE wipes in (not the full
  boot, and not the menu).
  *Emulator:* `emu-gate/tests/screen.rs::home_tapped_as_soon_as_the_card_shows_skips_it` (LIVE within 600 ms, golden live).
- [ ] Hold HOME while powering on, then let it go: the full boot plays
  ("READY.", `LOAD"SIDOU",8,1` typed, "SEARCHING FOR SIDOU", "LOADING" with
  colour stripes running in the border, "READY.", "RUN", then the title
  card), about 10 s, and a note key played during it sounds.
  *Emulator:* `emu-gate/tests/screen.rs::home_held_at_power_on_plays_the_full_boot` (loading stripes in the border, READY. and RUN, the title card, LIVE at about 11 s, goldens), `sidou-tests/tests/app/boot.rs::a_note_skips_and_plays` (a note during the full boot plays); *device-only:* the real HOME key held through power-on.
- [ ] HOME held at power-on, 5 times: the full boot every time (letting HOME
  go does not skip it).
  *Emulator:* `sidou-tests/tests/app/boot.rs::home_held_at_power_on_plays_the_full_boot_and_its_press_edge_is_part_of_the_hold`, `emu-gate/tests/screen.rs::home_held_at_power_on_plays_the_full_boot` (HOME let go after 600 ms does not skip); *device-only:* repeated real power-ons with a real key.
- [ ] No sound, click or hum before a key is played.
  *Emulator:* `emu-gate/tests/first_use.rs::every_note_key_sounds_its_note_and_lights_while_held` (a second of LIVE after the boot under the silence floor, every 10 ms); *device-only:* a click or hum heard from the speaker and headphones (analog output).
- [ ] Power on and off ten times: it boots every time.
  *Device-only:* real power cycles of the silicon (boot reliability, flash, power rails); the emulator's boot guard is `emu-gate/tests/bootguard.rs::a_stable_boot_clears_an_earlier_death`.

The first boot after any install is also the first time SIDOU's flash code
runs on the unit: at boot it reads the flash chip's id and, if an update
left its staging area (`0xE0000..0xE5000`) in use, erases it
(`ota_boot_cleanup`). Note anything unusual on that first boot (a longer
start, a dark screen for a moment).

Notes:

## 4. Note keys and their LEDs

Key 1 is the lowest. At OCTAVE 0 each key plays the note below (the
preset's saw, through the filter). Check the pitch of a few with a tuner
(A4 = 440 Hz). Its LED is lit while the key is held. The id is Felucca's
input id, for mapping notes.

*Emulator:* `emu-gate/tests/first_use.rs::every_note_key_sounds_its_note_and_lights_while_held` (each key's pitch within 0.5 % and its LED lit while it is held); *device-only:* which printed key has which id, the tuner on the speaker's sound and the LED as seen.

| Key | Note | Hz | Id | Sounds right | LED while held | Notes |
|---|---|---|---|---|---|---|
| 1 | F3 | 174.61 | 14 | [ ] | [ ] | |
| 2 | F#3 | 185.00 | 15 | [ ] | [ ] | |
| 3 | G3 | 196.00 | 16 | [ ] | [ ] | |
| 4 | G#3 | 207.65 | 17 | [ ] | [ ] | |
| 5 | A3 | 220.00 | 18 | [ ] | [ ] | |
| 6 | A#3 | 233.08 | 19 | [ ] | [ ] | |
| 7 | B3 | 246.94 | 20 | [ ] | [ ] | |
| 8 | C4 | 261.63 | 21 | [ ] | [ ] | |
| 9 | C#4 | 277.18 | 22 | [ ] | [ ] | |
| 10 | D4 | 293.66 | 23 | [ ] | [ ] | |
| 11 | D#4 | 311.13 | 24 | [ ] | [ ] | |
| 12 | E4 | 329.63 | 25 | [ ] | [ ] | |
| 13 | F4 | 349.23 | 26 | [ ] | [ ] | |
| 14 | F#4 | 369.99 | 27 | [ ] | [ ] | |
| 15 | G4 | 392.00 | 28 | [ ] | [ ] | |
| 16 | G#4 | 415.30 | 29 | [ ] | [ ] | |
| 17 | A4 | 440.00 | 30 | [ ] | [ ] | |
| 18 | A#4 | 466.16 | 31 | [ ] | [ ] | |
| 19 | B4 | 493.88 | 32 | [ ] | [ ] | |
| 20 | C5 | 523.25 | 33 | [ ] | [ ] | |
| 21 | C#5 | 554.37 | 34 | [ ] | [ ] | |
| 22 | D5 | 587.33 | 35 | [ ] | [ ] | |
| 23 | D#5 | 622.25 | 36 | [ ] | [ ] | |
| 24 | E5 | 659.26 | 37 | [ ] | [ ] | |
| 25 | F5 | 698.46 | 38 | [ ] | [ ] | |
| 26 | F#5 | 739.99 | 39 | [ ] | [ ] | |
| 27 | G5 | 783.99 | 40 | [ ] | [ ] | |

- [ ] Three keys at once sound together; a fourth steals the oldest note.
  *Emulator:* `sidou-tests/tests/app/live_instrument.rs::a_fourth_note_steals_the_oldest`, `sidou-tests/tests/app/panel.rs::many_keys_steal_and_release_cleanly` (all keys down: three gated voices); *device-only:* how three notes sound in the room.
- [ ] A held key shows its note name on the NOTES line.
  *Emulator:* `sidou-tests/tests/app/live_view.rs::the_envelope_stage_and_the_note_follow_the_tap` (A-4 in the voice lane while held, dimmed after release), `emu-gate/tests/screen.rs::live_matches_its_golden_images_and_its_scopes_follow_the_notes` (golden live-a4).

Notes:

## 5. Buttons, encoders and MASTER

The printed labels on the FM-1 may not match these roles: SIDOU's mapping is
Felucca's input ids, and the labels are guesses. For each control, write
which printed control did it.

| Role (id) | Expected | Printed control that does it | Pass | Covered |
|---|---|---|---|---|
| OCT- (button 0) | OCTAVE -1 (to -3), keys an octave lower; its LED lit while the octave is below 0 | | [ ] | *Emulator:* `sidou-tests/tests/app/panel.rs::buttons_turn_the_octave_and_fx_and_env_do_nothing_here`, `sidou-tests/tests/app/panel.rs::leds_show_held_keys_the_octave_and_the_8580` (octave -1, LED); *device-only:* the printed control |
| OCT+ (button 1) | OCTAVE +1 (to +3), keys an octave higher; its LED lit while the octave is above 0 | | [ ] | *Emulator:* `emu-gate/tests/live.rs::oct_up_doubles_the_pitch_and_lights_its_led` (pitch doubles, LED lit); *device-only:* the printed control |
| FX (button 2) | on LIVE opens LAB at its FX page; FX LED lit for the 8580 (ALGORITHM on TONE to R5) | | [ ] | *Emulator:* `emu-gate/tests/lab.rs::env_lfo_and_fx_on_live_open_lab_at_their_pages`, `emu-gate/tests/live.rs::the_model_toggle_swaps_chips_without_a_gap` (FX LED lit for the 8580); *device-only:* the printed control |
| K1 (encoder 2) | CUTOFF; turning down takes the highs out | | [ ] | *Emulator:* `emu-gate/tests/live.rs::turning_k1_down_takes_the_highs_out`, `sidou-tests/tests/app/panel.rs::k1_to_k4_are_cutoff_resonance_the_sounds_own_control_and_release`; *device-only:* the printed control |
| K2 (encoder 3) | RESO 0..15 | | [ ] | *Emulator:* `sidou-tests/tests/app/panel.rs::k1_to_k4_are_cutoff_resonance_the_sounds_own_control_and_release`, `sidou-tests/tests/app/live_view.rs::the_knob_row_shows_what_k1_to_k4_turn_and_nothing_else` (the resonance value follows K2, 0..15); *device-only:* the printed control |
| K3 (encoder 4) | the sound's own control: DECAY (SAW LEAD, NOISE PERC), PULSE (PWM BASS: 3 % to 50 %, one side), RATIO (SYNC LEAD: -12..+12 semitones); the label and value change with the preset | | [ ] | *Emulator:* `sidou-tests/tests/app/live_instrument.rs::k3_follows_the_sound` (DECAY on SAW LEAD and NOISE PERC, PULSE 3 % to 50 % on PWM BASS); *emulator (no test yet):* SYNC LEAD's RATIO -12..+12 on K3; *device-only:* the printed control |
| K4 (encoder 5) | RELEASE 0..15 (a note let go rings longer) | | [ ] | *Emulator:* `sidou-tests/tests/app/panel.rs::k4_turns_the_release` (0..15 on the register); *emulator (no test yet):* a released note rings longer as audio; *device-only:* the printed control |
| PRESETS (encoder 6) | the preset: SAW LEAD, PWM BASS, SYNC LEAD, NOISE PERC, clamped; each keeps its edits (a `*` after its name); EDIT held puts the preset back | | [ ] | *Emulator:* `sidou-tests/tests/app/panel.rs::presets_steps_the_preset_in_every_view`, `sidou-tests/tests/app/live_instrument.rs::a_preset_remembers_its_edits`, `sidou-tests/tests/app/live_instrument.rs::edit_presets_reverts` (EDIT held puts it back), `emu-gate/tests/screen.rs::a_preset_remembers_its_edits_and_shows_a_star`; *device-only:* the printed control |
| MASTER (pot) | VOLUME 0..100 %, even steps of loudness across the travel (a taper: no jump near the top), quieter turned down | | [ ] | *Emulator:* `sidou-tests/tests/app/master_gain.rs::master_has_an_even_db_taper`, `sidou-tests/tests/app/panel.rs::master_follows_the_pot_with_hysteresis`, `emu-gate/tests/live.rs::master_turns_the_volume_down`; *device-only:* the real pot's travel and how even the loudness feels, which printed control |
| MASTER (pot) untouched | the MASTER popup stays away while the pot is untouched: leave it in the middle, fully down and fully up for a minute each (keys playing and silent); no VOLUME popup comes up and the volume does not drift; a slow turn still brings the popup and it ends on the value turned to | | [ ] | *Emulator:* `sidou-tests/tests/app/panel.rs::a_noisy_pot_at_rest_never_asks_for_the_popup`, `sidou-tests/tests/app/panel.rs::a_real_turn_asks_and_its_gain_reaches_the_target` (a simulated noisy pot); *device-only:* the real pot's noise |
| SCL (button 3) | selects the next effect slot (the play mark on LIVE's DIST, MOD and FX lines moves) | | [ ] | *Emulator:* `sidou-tests/tests/app/fx_live.rs::edit_held_turns_k1_k4_into_the_selected_slots_controls` (SCL steps the slot and wraps), `sidou-tests/tests/app/live_view.rs::the_header_lanes_knobs_and_slots_show_the_instrument` (the slot line on LIVE); *device-only:* the printed control |
| ENV (button 4) | on LIVE opens LAB at its ENVELOPE page (LFO, button 5, at MOD); in the other views but LAB nothing | | [ ] | *Emulator:* `emu-gate/tests/lab.rs::env_lfo_and_fx_on_live_open_lab_at_their_pages`, `sidou-tests/tests/app/views.rs::the_panel_plays_the_instrument_as_before_in_every_view` (nothing outside LIVE and LAB); *device-only:* the printed control |
| EDIT (button 6) held + K1..K4 | the selected slot's Amount, A, B, Mix (the numbers on the status screen and the CHIP view; Mix as LIVE's bar), in every view; EDIT lit while the slot is on; on LIVE the large popup names the effect over the control and its value (`OVERDRIVE`, `DRIVE 91`) with a bar | | [ ] | *Emulator:* `sidou-tests/tests/app/fx_live.rs::edit_held_turns_k1_k4_into_the_selected_slots_controls` (Amount, A, B, Mix, EDIT LED lit while the slot is on), `sidou-tests/tests/app/chip_view.rs::the_edit_shortcut_turns_the_selected_slot_in_every_view`, `emu-gate/tests/screen.rs::edit_held_relabels_the_knob_row`, `sidou-tests/tests/app/popup.rs::edit_and_a_knob_on_live_open_the_effects_popup`, `emu-gate/tests/screen.rs::edit_and_a_knob_on_live_open_the_effects_popup` (golden `live-edit-popup`); *device-only:* the printed control, the popup readable at arm's length |
| HOME (button 8) | acts on its release: from LIVE, the menu (a wipe down from the top); held 600 ms it opens the help card of the view that shows instead (below); in the menu, back to LIVE (a wipe up from the bottom; behind THE OSCILLATOR's title card only with CARDS at ALWAYS, below) | | [ ] | *Emulator:* `emu-gate/tests/screen.rs::a_wipe_opens_the_menu_from_the_top_and_closes_it_from_the_bottom`, `sidou-tests/tests/app/views.rs::home_opens_the_menu_and_goes_back_one_view`, `emu-gate/tests/screen.rs::holding_home_shows_the_help_card_and_a_tap_still_goes_home` (600 ms), `emu-gate/tests/screen.rs::cards_always_and_off` (THE OSCILLATOR's card only with ALWAYS); *device-only:* the printed control and the real key's hold timing |
| SAVE (button 9) | clean release saves (7.1–7.2); held + knob learns (7) | | [ ] | *Emulator:* `sidou-tests/tests/app/save_key.rs::a_clean_press_and_release_saves_at_the_release`, `sidou-tests/tests/app/save_key.rs::another_button_or_a_knob_while_save_is_down_spoils_the_save`, `emu-gate/tests/midi.rs::midi_learn_over_trs_binds_cc_14_to_k1s_cutoff` (SAVE held + knob learns, SAVE alone does not), `emu-gate/tests/settings.rs::saves_press_on_live_stores_the_settings_within_a_second`; *device-only:* the printed control |
| SEQ (button 11) | from any view, STEP the first time after power-on (later the song view used last); then TRACKER, ORDER, INSTR, TABLE and back to STEP. Each page's primary values are readable at 2×; HOME returns to the previous non-song view | | [ ] | *Emulator:* `sidou-tests/tests/app/song_view.rs::seq_reopens_the_song_view_used_last` (STEP the first time), `sidou-tests/tests/app/song_view.rs::seq_opens_the_tracker_from_any_view_and_home_goes_back` (TRACKER, ORDER, INSTR, TABLE, STEP; HOME goes back), `emu-gate/tests/first_use.rs::tracker_rec_enters_a_note_that_plays_and_seq_walks_every_song_view`; *device-only:* the printed control, readability at 2x at arm's length |
| SELECT / ALGORITHM, in TRACKER | SELECT moves one row (EDIT held: an 8-row page); ALGORITHM shows V2 in cyan, V3 in light green, then V1 again; the cursor's voice and audition follow the voice shown | | [ ] | *Emulator:* `sidou-tests/tests/app/tracker_view.rs::select_moves_rows_and_edit_select_moves_a_readable_page` (a row, EDIT a page), `sidou-tests/tests/app/tracker_view.rs::algorithm_steps_the_shown_voice_and_round` (V2 in its colour, V3, V1), `sidou-tests/tests/app/tracker_view.rs::the_keys_audition_on_the_shown_voice`; *device-only:* the colours as seen on the LCD |
| STEP (SEQ from LIVE) | the 16 leftmost keys are steps: tap one, a ball appears on the lane (V1 selected, the arrow) and the key's pad turns the lane's colour; tap again, it goes. Staccato (the default) writes a key off on the next empty row; ARP switches the lane to LEGATO and back. SELECT picks V2, V3; ALGORITHM pages (the page follows the playing step while the song plays); PRESETS sets the lane's instrument, or with a step held that step's note | | [ ] | *Emulator:* `emu-gate/tests/first_use.rs::every_template_instrument_on_steps_plays_heard_and_on_time`, `emu-gate/tests/first_use.rs::every_template_instrument_auditions_on_step_heard_at_its_pitch_and_each_its_own`, `sidou-tests/tests/app/step_view.rs::a_step_pressed_and_let_go_toggles_a_note_with_a_key_off_after_it`, `sidou-tests/tests/app/step_view.rs::a_legato_lane_writes_no_key_off_and_taking_a_note_away_leaves_the_one_before`, `sidou-tests/tests/app/step_view.rs::while_the_song_plays_the_page_and_the_white_step_follow_it`, `emu-gate/tests/screen.rs::the_step_view_matches_its_goldens`; *device-only:* the 16 leftmost real keys as step keys |
| STEP, a step held | hold a step key and turn K1: the HELD STEP frame comes alive, K1 CUTOFF shows LOCK in yellow with a bar, a yellow notch stays under the step after the key is let go, and nothing toggles; played, only that step's row is darker. K4 on a held step shows SET in cyan (waveform), a cyan notch; on a row that has a command it says ROW HAS A COMMAND. LFO with the step held clears them. Save, then open the song in GoatTracker: the SETs play there as here | | [ ] | *Emulator:* `sidou-tests/tests/app/step_view.rs::a_held_step_and_a_knob_is_a_lock_that_the_step_shows_and_keeps_for_its_row_only`, `sidou-tests/tests/app/step_view.rs::a_held_step_and_k4_is_a_set_the_players_own_waveform_command_shown_apart_from_a_lock`, `sidou-tests/tests/app/step_view.rs::lfo_on_a_held_step_clears_its_locks_and_its_set`, `sidou-tests/tests/app/step_view.rs::a_step_made_song_plays_as_goattrackers_player_does_with_its_set_and_without_the_chunk_locks` (against GoatTracker's reference player); *device-only:* the saved song opened in the GoatTracker program on a computer |
| TRACKER ROWS in SETTINGS (menu, SETTINGS) | BAR: while SIDOU LOOP plays the bar moves down still rows, and the rows change cleanly at each new pattern; SCROLL: the bar stays on the middle row and the rows move up, smoothly at this tempo or a frame behind at most | | [ ] | *Emulator:* `sidou-tests/tests/app/tracker_view.rs::scroll_keeps_the_bar_on_the_windows_row_5`, `sidou-tests/tests/app/tracker_view.rs::the_bar_moves_down_the_page_and_turns_it`, `emu-gate/tests/screen.rs::the_tracker_scrolling_matches_its_golden_image`; *device-only:* how smooth the scroll looks on the real LCD |
| PLAY (button 12) | SIDOU LOOP plays (a lead, a bass and drums, in time, no clicks between rows), in any view; TRACKER's header shows a play mark and the rows move (the bar, and the strips' tall bars where the other voices play notes); PLAY again stops it and the notes ring out; EDIT + PLAY goes on from there; PLAY's LED lit while it plays | | [ ] | *Emulator:* `emu-gate/tests/song.rs::play_plays_the_first_song_bit_exact_with_the_host` (SIDOU LOOP as the host renders it), `emu-gate/tests/first_use.rs::every_template_instrument_on_steps_plays_heard_and_on_time` (on time), `sidou-tests/tests/app/song_view.rs::play_plays_from_the_cursor_stops_and_with_edit_goes_on`, `emu-gate/tests/song.rs::the_play_led_is_dim_between_beats_and_lit_on_each_beat`, `sidou-tests/tests/song_ctl.rs::stop_releases_continue_resumes_mute_silences`, `emu-gate/tests/screen.rs::the_song_views_header_and_heads_match_their_goldens`; `emu-gate/tests/first_use.rs::play_is_quiet_until_the_first_note` (no thud before the first note); *device-only:* the printed control, clicks between rows heard on the speaker |
| REC (button 13) | REC in red under TRACKER's play mark, and off again | | [ ] | *Emulator:* `sidou-tests/tests/app/song_view.rs::the_header_shows_the_transport_the_tempo_the_position_and_rec`, `emu-gate/tests/screen.rs::the_song_views_header_and_heads_match_their_goldens` (REC in the header), `sidou-tests/tests/app/panel.rs::rec_is_dim_armed_blinks_in_the_count_in_and_lit_recording`; *device-only:* the printed control |
| A note key in TRACKER, REC off and the song stopped | SIDOU LOOP's first instrument on the voice shown (after ALGORITHM, on that voice), nothing of the song plays; let go, it releases; LIVE's keys sound the same as before afterwards | | [ ] | *Emulator:* `sidou-tests/tests/app/song_view.rs::a_key_in_a_song_view_jams_the_current_instrument_on_the_cursors_voice`, `sidou-tests/tests/app/tracker_view.rs::the_keys_audition_on_the_shown_voice`, `emu-gate/tests/song_store.rs::a_new_song_plays_and_auditions_its_lead_at_pitch`; *emulator (no test yet):* LIVE's keys sound bit-exact as before after such an audition |
| TRACKER with REC on | enter four notes: each appears at the cursor and advances a row; K1 instrument, K2 command, K3/K4 data show large popups; the selected field is reverse yellow at 2×. ARP writes key off; LFO clears the note, LFO again clears the row. EDIT + LFO undoes/redoes; EDIT + PRESETS inserts/deletes without changing the pattern length | | [ ] | *Emulator:* `emu-gate/tests/first_use.rs::tracker_rec_enters_a_note_that_plays_and_seq_walks_every_song_view`, `sidou-tests/tests/app/tracker_view.rs::rec_keys_and_knobs_edit_the_shown_pattern_with_one_step_undo` (K1 instrument, K2 command, K3/K4 data, popups, EDIT + LFO undo and redo), `sidou-tests/tests/app/tracker_view.rs::a_second_lfo_on_the_same_row_clears_the_whole_row_and_arp_sets_keyoff`, `sidou-tests/tests/app/tracker_view.rs::the_edit_cursor_and_each_edited_field_are_visible_at_two_times`, `sidou-tests/tests/app/tracker_view.rs::presets_step_size_and_shift_keep_the_length_and_undo_the_lost_tail`; *device-only:* the 2x yellow field as seen on the LCD |
| ORDER / INSTR / TABLE | ORDER: SELECT entries, ALGORITHM voice (subtune on its selector), K1 value, K2 kind, EDIT + PRESETS insert/delete, ARP restart, PLAY from that subtune/entry. INSTR: PRESETS instrument, SELECT parameter/name character, K1 coarse/K2 fine; keys audition. EDIT + SEQ on a pointer opens its table. TABLE: ALGORITHM kind, SELECT row, K1/K2 bytes; meanings at the foot | | [ ] | *Emulator:* `sidou-tests/tests/app/song_pages.rs::order_insert_restart_delete_and_invalid_last_kind`, `sidou-tests/tests/app/song_pages.rs::order_subtune_selector_and_voice_navigation_are_separate`, `sidou-tests/tests/app/song_pages.rs::instrument_coarse_fine_and_names_use_acknowledged_cells`, `sidou-tests/tests/app/song_pages.rs::table_bytes_are_separate_cells_and_pointer_jump_is_one_based`, `sidou-tests/tests/app/song_pages.rs::global_edit_seq_jumps_to_the_selected_instrument_pointer_before_cycling`, `emu-gate/tests/screen.rs::the_readable_order_instrument_and_table_pages_match_their_goldens` |
| GLO (button 7) held + the leftmost note key | while SIDOU LOOP plays, its voice 1 goes silent; TRACKER's V1 turns grey and says MUT in red beside the pattern (a muted voice shown as a strip: grey and short), LIVE's top line M1 in red in the preset's place; the key sounds nothing; again: back. GLO + the fourth key: only voice 1 sounds, SOL beside V1's pattern | | [ ] | *Emulator:* `emu-gate/tests/first_use.rs::glo_mutes_a_lane_and_only_that_lane_falls_silent` (muted lane silent, only that lane), `sidou-tests/tests/app/song_view.rs::glo_and_the_leftmost_keys_mute_and_solo_and_sound_nothing`, `sidou-tests/tests/app/tracker_view.rs::a_muted_voice_is_grey_in_the_header_and_its_strip_and_says_so`, `emu-gate/tests/screen.rs::live_shows_the_song_and_the_mutes` (M1 on LIVE); *device-only:* the printed control |
| SELECT (encoder 0), in the menu | the selected line (reverse video) moves between CHIP, TRACKER, SCENE, SONG, LAB, SIDS, PRE (PRESET), OCTAVE, SETTINGS, STATUS, MIDI, CHARSET and ABOUT; six lines show at a time and the list scrolls with it, the title bar's arrows telling which way more is | | [ ] | *Emulator:* `sidou-tests/tests/app/views.rs::every_menu_item_is_reachable_and_readable_as_the_cursor_walks_down_and_up`, `emu-gate/tests/screen.rs::the_menu_matches_its_golden_images_at_the_top_and_scrolled_and_does_not_blink`, `sidou-tests/tests/app/views.rs::the_menu_draws_six_items_in_large_text_with_the_selected_one_in_reverse_video`; *device-only:* the printed control |
| SAVE (button 9), in the menu | the menu's OK: opens the item under the selection (CHIP, TRACKER, SCENE, SONG, LAB, SIDS, SETTINGS, STATUS, MIDI, CHARSET, ABOUT); on PRE or OCTAVE nothing happens and no SAVED toast shows | | [ ] | *Emulator:* `sidou-tests/tests/app/views.rs::save_opens_the_menus_item_and_does_nothing_on_a_value`, `sidou-tests/tests/app/help_view.rs::save_opens_the_highlighted_item_on_every_list_screen_and_home_goes_back`, `sidou-tests/tests/app/save_key.rs::saves_press_saves_the_settings_in_scene_status_and_chip_not_in_the_menu` (no SAVED toast) |
| ALGORITHM (encoder 1), in SETTINGS | changes the item under the selection (KEYS, SOUNDS, ROWS, CARDS: OFF, ONCE (the default), ALWAYS, one step a detent; STORE; THEME: counter-clockwise HIGH, the default, clockwise C64: every screen, SETTINGS included, changes colours at once; REC); HOME goes back to the menu | | [ ] | *Emulator:* `sidou-tests/tests/app/views.rs::settings_draws_its_seven_items_like_the_menu_and_home_goes_back`, `sidou-tests/tests/app/title_card.rs::the_cards_option_steps_off_once_always` (CARDS), `sidou-tests/tests/app/theme.rs::the_menu_item_turns_the_theme_and_the_menu_follows_it` (THEME), `emu-gate/tests/screen.rs::live_matches_its_golden_image_in_the_c64_theme_and_back`, `sidou-tests/tests/app/tracker_view.rs::scroll_keeps_the_bar_on_the_windows_row_5` (ROWS), `sidou-tests/tests/app/save_key.rs::reset_asks_first_only_save_confirms_and_cancels_write_nothing`; *emulator (no test yet):* KEYS, SOUNDS, STORE and REC stepping one value a detent in one test; *device-only:* the colours of the C64 theme on the real LCD |
| ALGORITHM (encoder 1), in the menu | changes the item under the selection (PRESET; OCTAVE -3..+3); clockwise on CHIP opens the CHIP view behind THE CHIP's title card (below), on STATUS the status screen (the numbers), on SCENE the generative pieces (below), on CHARSET the glyph sheet, on ABOUT the credits (the chrome lettering at the top, the credits pages fading over stars, raster bars behind a white sine scroller), and HOME goes back to the menu | | [ ] | *Emulator:* `sidou-tests/tests/app/views.rs::the_menu_changes_the_preset_and_octave`, `sidou-tests/tests/app/chip_view.rs::chip_opens_from_the_menu_behind_the_chip_card`, `emu-gate/tests/screen.rs::the_glyph_sheet_opens_from_the_menu_and_home_goes_back`, `emu-gate/tests/screen.rs::about_opens_from_the_menu_and_home_goes_back`, `emu-gate/tests/screen.rs::entering_scene_names_the_piece`, `sidou-tests/tests/app/views.rs::the_status_view_shows_the_instrument`; *device-only:* the About scroller and fades as seen on the LCD |
| SELECT (encoder 0), on STATUS | turns its three pages, either way round (`1/3`, `2/3`, `3/3` in the title): 1 the knobs with their bars (the 6502's rows with a tune loaded), 2 the chip, preset, octave and notes, 3 the effect slots; all at 2×; a new opening starts on page 1; the help card (hold HOME) names SELECT | | [ ] | *Emulator:* `sidou-tests/tests/app/views.rs::the_status_view_shows_the_instrument`, `sidou-tests/tests/app/sid_view.rs::status_shows_the_6502s_rows_while_a_tune_is_loaded`, `emu-gate/tests/screen.rs::the_status_screen_matches_its_golden_image`; *device-only:* the 2× text readable on the LCD |
| SELECT (encoder 0), on CHIP | the focus (the yellow tab, or the yellow title on the A/B tabs) moves tabs, CHIP, DIST, MOD, FX, EXPERT and stops at either end; onto a slot that was not the selected one, the toast says SLOT DIST (or MOD, FX); onto the slot already selected (DIST at power-on), no toast | | [ ] | *Emulator:* `sidou-tests/tests/app/chip_view.rs::select_moves_the_focus_and_selects_the_slot` (CHIP, DIST, MOD, FX, EXPERT, clamped, the slot follows), `sidou-tests/tests/app/chip_view.rs::expert_is_one_more_select_stop_without_changing_the_selected_slot`; *emulator (no test yet):* the toast SLOT DIST, MOD, FX on a slot change and none for the slot already selected |
| ALGORITHM (encoder 1), on CHIP | on CHIP the PROFILE steps 6581 R2, 6581 R3, 6581 R4AR, 8580 R5, WORN 6581, then filled USER slots (7.2; MODEL follows; held notes attack again on the new chip, as at a model change); on a slot its algorithm, OFF to the last, at 3× under the tabs | | [ ] | *Emulator:* `sidou-tests/tests/app/chip_view.rs::algorithm_steps_the_profile_and_the_model_follows` (every profile in order, MODEL follows), `sidou-tests/tests/app/chip_view.rs::algorithm_steps_the_focused_slots_algorithm`, `sidou-tests/tests/app/live_instrument.rs::a_profile_change_reattacks_the_held_notes_on_the_new_chip`, `emu-gate/tests/screen.rs::chip_matches_its_golden_images`; *emulator (no test yet):* the filled USER slots after WORN 6581 in the same walk |
| K1..K4, on CHIP | on CHIP AGE, DAC, THUMP, BLEED (values at 3× with bars, in the knob's colour; the sound darkens with AGE; DAC adds a worn chip's grit, heard on SAW LEAD, PWM BASS, NOISE PERC and soft notes alike from its first detents, rougher to the top, and a released note whispers with it); on a slot its four controls, named for the algorithm (OVERDRIVE: DRIVE, TONE, BIAS, MIX) | | [ ] | *Emulator:* `sidou-tests/tests/app/chip_view.rs::the_knobs_turn_the_character_on_the_chip`, `sidou-tests/tests/app/chip_view.rs::the_knobs_turn_the_focused_slot`, `emu-gate/tests/character.rs::the_chip_views_k1_turns_the_chips_age_and_matches_the_host_sample_exact`, `emu-gate/tests/screen.rs::chip_matches_its_golden_images`; *emulator (no test yet):* the sound darkening with AGE and DAC's grit on each preset as measured audio; *device-only:* how the worn chip sounds |

- [ ] A held note keeps its pitch when the octave changes, and stops when
  the key is released.
  *Emulator:* `sidou-tests/tests/app/panel.rs::key_up_after_an_octave_change_releases_the_note_it_played` (the held note keeps sounding, key-up releases it, the next press plays the new octave).
- [ ] OVERDRIVE with Amount and Mix up distorts audibly; REVERB or LUSH with
  Mix up adds a stereo tail (listen on headphones); switching algorithms
  while a note plays crossfades without a click.
  *Emulator:* `emu-gate/tests/fx_live.rs::the_edit_layer_turns_on_overdrive_and_the_wave_clips` (the wave clips), `emu-gate/tests/fx_live.rs::trs_ccs_turn_on_the_reverb_in_stereo_with_a_tail` (stereo tail), `sidou-tests/tests/unit/slots.rs::switching_algorithms_is_click_free_in_the_existing_blocks`; *device-only:* listening on headphones.
- [ ] In the menu, on the status screen, in SCENE and on the glyph sheet, the
  keys, K1..K4, OCT-/OCT+ and the EDIT layer play as on LIVE (ENV and FX do
  nothing there). On the CHIP view the keys, OCT-/OCT+ and the EDIT layer do too
  (EDIT + K1 turns the selected slot's Amount, whatever the focus), and K1..K4 are
  the page's.
  *Emulator:* `sidou-tests/tests/app/views.rs::the_panel_plays_the_instrument_as_before_in_every_view`, `sidou-tests/tests/app/chip_view.rs::the_panel_plays_on_the_chip_view`, `sidou-tests/tests/app/chip_view.rs::the_edit_shortcut_turns_the_selected_slot_in_every_view`.
- [ ] On the CHIP view, a character or slot CC from a controller (CC 86 for AGE,
  CC 102..116 for the slots) moves its bar or value on the page as it plays.
  *Emulator:* `emu-gate/tests/screen.rs::chip_follows_midi_and_the_knobs_there_and_back` (a CC takes the age to 0 and the page shows it), `sidou-tests/tests/app/chip_view.rs::midi_changes_show_on_the_page`; *device-only:* a real controller sending CCs (external gear).
- [ ] Outside LIVE (on STATUS, say), OCT+, SCL and a slot's algorithm show a toast on the
  bottom line ("OCTAVE +1", "SLOT MOD", "MOD CHORUS") that goes after about 1.5 s; a change
  to the 8580 (ALGORITHM on LIVE's TONE to R5) shows "CHIP 8580" and sweeps colour bands down
  the border for about half a second.
  *Emulator:* `sidou-tests/tests/app/views.rs::a_change_of_octave_slot_or_algorithm_shows_a_toast` (OCTAVE +1, SLOT MOD, MOD CHORUS, gone after the toast time), `sidou-tests/tests/app/views.rs::a_chip_change_shows_a_toast_and_runs_the_raster` (CHIP 8580), `emu-gate/tests/screen.rs::a_chip_change_sweeps_the_raster_over_the_border`, `emu-gate/tests/screen.rs::a_button_change_shows_a_toast_that_goes_away`.
- [ ] **The song views' button ids (M2d).** GLO 7, SEQ 11, PLAY 12 and REC 13 (and LFO 5,
  ARP 10: STEP's gate, LIVE's chord arpeggio) are Felucca's ids and were never confirmed on a SIDOU unit (M1 used
  only FX, SCL, ENV, EDIT, HOME and SAVE). The four rows above check them by what they do;
  write the printed control for each. If one does nothing and another printed button does
  its job, only `app/panel.h` changes: note which id is which. M2d is not done on hardware
  until this passes.
  *Device-only:* which printed control each id is on a real unit (real keys, ids never confirmed on a SIDOU unit); the firmware's own ids are fixed by `sidou-tests/tests/app/panel.rs::the_song_buttons_have_feluccas_ids`.
- [ ] The other buttons and encoders do nothing (record anything they do);
  SELECT does nothing outside the menu, CHIP, SCENE and TRACKER, ALGORITHM nothing
  outside the menu and CHIP.
  *Emulator:* `sidou-tests/tests/app/views.rs::select_and_algorithm_do_nothing_outside_the_menu` (SELECT and ALGORITHM outside their views); *device-only:* any real control that does something unexpected (real key and encoder hardware).

Notes:

## 6. LCD

The panel runs at 12 MHz SPI (Felucca's setting).

- [ ] Text is sharp, colours right, no stray pixels or shifted cells.
  *Emulator:*
  `emu-gate/tests/screen.rs::the_menu_matches_its_golden_images_at_the_top_and_scrolled_and_does_not_blink`,
  `emu-gate/tests/screen.rs::live_matches_its_golden_images_and_its_scopes_follow_the_notes`,
  `emu-gate/tests/screen.rs::chip_matches_its_golden_images` (a golden match rules out wrong
  cells, stray pixels and shifted rows in the drawn frame); *device-only:* sharpness and colours
  as seen on the real LCD (look of the physical panel).
- [ ] Only what changes is redrawn: no flicker while idle, none while
  turning K1 fast or playing fast notes.
  *Emulator:*
  `emu-gate/tests/live.rs::the_lcd_shows_the_status_screen_and_redraws_only_on_change`,
  `emu-gate/tests/screen.rs::live_stays_within_its_budgets`,
  `sidou-tests/tests/app/live_view.rs::meters_envelopes_notes_and_a_toast_redraw_their_cells_not_the_canvas`
  (only changed cells are sent, also while K1 turns and notes play); *device-only:* whether the
  panel flickers to the eye (flicker as seen).
- [ ] Nothing goes wrong on screen while the audio plays (no clicks either).
  *Emulator:*
  `emu-gate/tests/latency.rs::a_note_on_reaches_the_buffer_within_a_block_and_a_poll_while_the_screen_redraws`,
  `emu-gate/tests/screen.rs::live_stays_within_its_budgets` (screen work never starves the audio
  interrupt); *device-only:* clicks heard from the speaker (how it sounds in the room).
- [ ] The wipes and the border raster run smoothly, with no tearing or
  stray cells left behind, and no click in the sound while they run.
  *Emulator:*
  `emu-gate/tests/screen.rs::a_wipe_opens_the_menu_from_the_top_and_closes_it_from_the_bottom`,
  `emu-gate/tests/screen.rs::a_chip_change_sweeps_the_raster_over_the_border`,
  `emu-gate/tests/screen.rs::every_wipe_into_an_animated_view_sends_only_its_rows` (rows sent,
  ramp order, nothing left stale); *device-only:* smoothness and tearing as seen, and a click
  heard (panel look and speaker).

LIVE (the screen at power-on):

- [ ] At rest the three scopes are flat dark grey lines two pixels thick, the voice numbers
  1 2 3 in light red, cyan and light green, the three notes `---`, the VU meters empty, the
  knob row (CUTOFF, RESO, PULSE, PRESET) in yellow, cyan, light green and light red, and
  nothing flickers.
  *Emulator:*
  `emu-gate/tests/screen.rs::live_matches_its_golden_images_and_its_scopes_follow_the_notes`,
  `sidou-tests/tests/app/live_view.rs::the_header_lanes_knobs_and_slots_show_the_instrument`,
  `sidou-tests/tests/app/live_view.rs::a_note_draws_a_trace_and_silence_a_flat_line` (flat
  lines, numbers, colours and knob row in the framebuffer, nothing redrawn at rest).
- [ ] A held key: its voice's scope turns its colour and shows the waveform (SAW LEAD a
  sawtooth, PWM BASS a pulse whose width moves, SYNC LEAD voice 1's synced saw, NOISE PERC
  noise), standing still while the note holds, two pixels thick; the note's name at 2× on
  the right and ATK, DEC, SUS, REL under it; its VU bar moves. Higher notes show more periods.
  *Emulator:*
  `emu-gate/tests/screen.rs::live_matches_its_golden_images_and_its_scopes_follow_the_notes`,
  `sidou-tests/tests/app/live_view.rs::a_note_draws_a_trace_and_silence_a_flat_line`,
  `sidou-tests/tests/app/live_view.rs::the_envelope_stage_and_the_note_follow_the_tap` (trace,
  note, ADSR, meter); *emulator (no test yet):* each preset's own scope shape (SAW, PWM, SYNC,
  NOISE) and more periods for higher notes.
- [ ] Released, the note's name dims, and the scope goes flat and dark grey again once the
  release has faded.
  *Emulator:*
  `sidou-tests/tests/app/live_view.rs::a_note_draws_a_trace_and_silence_a_flat_line`,
  `sidou-tests/tests/app/live_view.rs::the_envelope_stage_and_the_note_follow_the_tap`,
  `sidou-tests/tests/app/live_view.rs::back_from_the_menu_the_scopes_and_envelopes_start_flat`.
- [ ] A three-note chord: all three scopes move; with noise on all three
  (CC 20 = 64) they keep moving, taking turns, with no tearing.
  *Emulator:* `emu-gate/tests/screen.rs::a_chord_on_live_matches_its_golden_image`,
  `sidou-tests/tests/app/live_view.rs::three_voices_of_noise_take_turns_within_the_frame_budget`
  (three scopes move; noise voices take turns within the frame budget).
- [ ] K1 and K2 move the CUTOFF and RESO values and their bars (the cutoff as `980`, `1.2K`
  or `12K`); MIDI CC 23 changes the filter's mode at 2× (LP, BP, HP, NOTCH...). K3 moves
  PULSE (a percent) and PRESETS changes the preset (the name in the header, a star once edited); K4 is the RELEASE and its
  name at the top.
  *Emulator:*
  `sidou-tests/tests/app/live_view.rs::the_filter_mode_and_the_knob_values_follow_the_filter`,
  `sidou-tests/tests/app/live_view.rs::every_knob_value_fits_its_column`,
  `emu-gate/tests/screen.rs::a_preset_remembers_its_edits_and_shows_a_star` (cutoff and
  resonance values and bars, the filter mode at 2x, preset name and star).
- [ ] The L and R meters follow the output; with REVERB on, the tail shows
  on them after the note.
  *Emulator:* `sidou-tests/tests/app/live_view.rs::the_meters_show_the_peaks_and_fall_back` (L
  and R meters follow the peaks); *emulator (no test yet):* the reverb tail showing on the
  meters after the note
  (`emu-gate/tests/fx_live.rs::trs_ccs_turn_on_the_reverb_in_stereo_with_a_tail` checks the tail
  in the audio, not on the meters).
- [ ] No click or crackle in the sound while LIVE shows, against the menu
  (LIVE taps the audio interrupt; the menu does not).
  *Emulator:*
  `sidou-tests/tests/app/scope.rs::the_tap_takes_a_point_every_decimate_samples_and_leaves_the_audio_alone`,
  `emu-gate/tests/live.rs::the_main_loop_is_cheap_when_idle`,
  `emu-gate/tests/screen.rs::live_stays_within_its_budgets` (the tap leaves the samples alone
  and fits the interrupt budget); *device-only:* whether any click or crackle is heard
  (speaker).

Large text, the menu and the value popup ([controls](reference/controls.md#value-popup);
`docs/art/README.md` "Large text"): the first hardware run found the 8×8 font almost
unreadable on this panel. Look at arm's length, at a desk and held in the hand:

- [ ] **The menu** (HOME): the title bar and six lines are readable without leaning in, the
  selected line's reverse video is clear at a glance, nothing sits under the bezel at the
  left or right edge (a cell of margin), and the top line of the title bar is whole. Scroll to
  the end and back: the lines come in a line or two a frame (a short sweep, no tearing, no
  stray cells), the arrows in the title bar say where more is, PRE shows NOISE PERC without
  touching its label, STORE reads `193K`, ROWS BAR or SCROLL and THEME HIGH or C64. Every one of
  the 15 items can be reached and read.
  *Emulator:*
  `emu-gate/tests/screen.rs::the_menu_matches_its_golden_images_at_the_top_and_scrolled_and_does_not_blink`,
  `emu-gate/tests/screen.rs::a_wipe_opens_the_menu_from_the_top_and_closes_it_from_the_bottom`,
  `ergo/tests/menu.rs::select_item_reaches_every_line_in_both_themes` (title bar, six lines,
  scroll, arrows, all items reachable);
  `sidou-tests/tests/app/save_key.rs::the_menus_store_shows_the_free_space` (STORE value);
  *Device-only:* readable at arm's length and nothing hidden under the real bezel (look on the
  real LCD and its frame).
- [ ] **The popup**: turn K1 on LIVE: CUTOFF and its value in Hz (`1.2K`) in large letters
  over the middle of the screen with a bar in the knob's colour (yellow for K1), opening in about a tenth of a second; keep
  turning and it stays; stop and it goes about a second later, LIVE back whole (scopes, the knob
  row and the slots as before, no stray cells). Each of these says its own name and value
  readably, in the large size where it fits: K2 RESO, K3 PULSE (a percent), K4 PRESET (a name,
  no bar), EDIT + K1..K4 (`DIST DRIVE` and the like, in the smaller size), MASTER VOLUME, and on
  CHIP K1..K4 AGE, DAC, THUMP, BLEED and ALGORITHM the PROFILE or the slot's algorithm. Over
  the menu, ALGORITHM on OCTAVE shows OCTAVE and `+1`. SELECT opens nothing.
  *Emulator:*
  `sidou-tests/tests/app/popup.rs::a_knob_turn_opens_the_popup_three_rows_a_frame_then_closes_a_second_after_the_last_change`,
  `sidou-tests/tests/app/popup.rs::a_change_restores_the_view_beneath_cell_for_cell_once_the_popup_is_gone`,
  `sidou-tests/tests/app/popup.rs::every_knob_and_algorithm_shows_its_name_and_value_as_the_views_say_them`,
  (names, values, bar colour, restore, SELECT opens nothing); *device-only:* readable in the
  large size on the real panel (look at arm's length).
- [ ] **MIDI never opens it**: sweep a controller (CC 74) over MIDI while LIVE shows: the
  CUTOFF value and bar move, no popup.
  *Emulator:* `sidou-tests/tests/app/popup.rs::midi_changes_never_open_the_popup`,
  `sidou-tests/tests/app/live_view.rs::the_filter_mode_and_the_knob_values_follow_the_filter`.
- [ ] **A popup while notes sound**: hold a chord on LIVE and turn K1: the scopes under the box
  are hidden, the rest keeps moving, the sound has no click or dropout, and the box goes
  cleanly a second after the last turn.
  *Emulator:*
  `sidou-tests/tests/app/popup.rs::the_popup_covers_its_rows_and_guards_them_from_the_views_large_text`,
  `emu-gate/tests/screen.rs::the_popup_stays_within_its_budgets_over_every_view` (box hides the
  scopes, budget kept with a chord held); *emulator (no test yet):* audio over a popup equals
  the same run without it, sample for sample (no click or dropout); a close a second after the
  last turn is in the popup test above.
- [ ] **Toasts**: OCT+ shows `OCTAVE +1` in large letters on the bottom two rows, readable, for
  about 1.5 s; SAVE shows `SAVED`.
  *Emulator:*
  `sidou-tests/tests/app/popup.rs::the_toast_is_large_text_on_two_rows_or_four_when_it_breaks_at_a_space`,
  `sidou-tests/tests/app/popup.rs::a_large_toast_ends_and_gives_its_rows_back`,
  `sidou-tests/tests/app/save_key.rs::saves_press_saves_the_settings_now_on_live_and_says_saved`.

Readability of the redesigned screens ([the theme and layouts](art/README.md#theme)): each is
drawn to be read at the panel's 23 mm. Hold the FM-1 at arm's length, at a desk and in the
hand, in the HIGH CONTRAST theme (the default) and again after THEME to C64 in SETTINGS:

- [ ] **LIVE**: from across a desk you can tell, without leaning in, which voices sound (the
  scopes in their colours against dark grey), each voice's note, and the four knobs' values
  (the cutoff, resonance, pulse width and preset) in their colours. The 2-pixel traces are
  clean lines, not a ladder of dots. The small text (the waveform under each number, the stage
  under each note, the knob names, the slot lines) is the part you read when you look at it,
  not from across the desk: note which of it you cannot read at all.
  *Emulator:*
  `emu-gate/tests/screen.rs::live_matches_its_golden_images_and_its_scopes_follow_the_notes`,
  `emu-gate/tests/screen.rs::live_matches_its_golden_image_in_the_c64_theme_and_back` (what is
  drawn, both themes); *device-only:* readable from across a desk, 2-pixel traces clean, which
  small text cannot be read (look and sharpness on the real LCD; includes a note to write down).
- [ ] **TRACKER**: one voice's twelve rows at 2× are readable at a glance (the note and the
  instrument), the current row's dark grey bar is clear against black, and the other two
  voices' strips show where they play; the row numbers and the commands (small) are legible
  in the hand. SELECT switches voices without a flicker. Mute a voice: its number and strip
  are visibly grey. Note anything you cannot read.
  *Emulator:* `emu-gate/tests/screen.rs::the_song_views_header_and_heads_match_their_goldens`,
  `sidou-tests/tests/app/tracker_view.rs::a_muted_voice_is_grey_in_the_header_and_its_strip_and_says_so`,
  `sidou-tests/tests/app/tracker_view.rs::algorithm_steps_the_shown_voice_and_round` (rows, bar,
  strips, mute colour); *device-only:* readable at a glance and in the hand (look on the real
  LCD).
- [ ] **STEP**: the lanes' balls (a note) against the dark squares (none) and the white
  playing step read from across the desk; the pads under them show the same steps in the lane's
  colour, the held one yellow. The HELD STEP frame's names (K1 CUTOFF ...), LOCK and SET are
  small: note which of it you cannot read in the hand.
  *Emulator:* `emu-gate/tests/screen.rs::the_step_view_matches_its_goldens`,
  `sidou-tests/tests/app/step_view.rs::the_layout_is_the_header_the_pages_the_lanes_the_held_frame_and_the_pads`
  (balls, squares, white step, pads, held-step frame); *device-only:* which small text can be
  read in the hand (look on the real LCD).
- [ ] **CHIP**: the four tabs, the choice at 3× and the four controls at 3× are readable from
  arm's length; the values in knob order match the knobs under your fingers; three digits at 2×
  (from 100) are still easy. The A/B title and KEYS marker, and the SAVE TO USER n line (yellow,
  small), can be found when you look for them.
  *Emulator:* `emu-gate/tests/screen.rs::chip_matches_its_golden_images`,
  `sidou-tests/tests/app/chip_view.rs::chip_labels_legends_and_tabs_use_readable_roles` (the
  drawn page and its colour roles); *device-only:* readable from arm's length, and values in
  knob order matching the knobs under the fingers (look on the real LCD; which physical knob is
  which).
- [ ] **STATUS and ABOUT**: STATUS is all 2× on three pages and readable at arm's length: SELECT
  turns them (either way round; `1/3` in the title; the first page on each opening): 1 the
  knobs with their bars, 2 the chip, preset, octave and the notes, 3 the effect slots. ABOUT's
  2× title and the key numbers are readable; its small lines are as before.
  *Emulator:* `emu-gate/tests/screen.rs::the_status_screen_matches_its_golden_image`,
  `emu-gate/tests/screen.rs::about_opens_from_the_menu_and_home_goes_back` (what is drawn);
  *Device-only:* readable on the real LCD.
- [ ] **MENU, SETTINGS and the MIDI page**: the menu's eleven items and SETTINGS' seven at 2×
  read from arm's length; the MIDI page (HOME, then MIDI) is small text in the theme's colours
  (the items white, the help lines grey): note whether you can read its ten items in the hand.
  *Emulator:*
  `emu-gate/tests/screen.rs::the_menu_matches_its_golden_images_at_the_top_and_scrolled_and_does_not_blink`,
  `emu-gate/tests/screen.rs::the_midi_page_at_2x_matches_its_golden_and_stays_within_190_cells`
  (drawn pages); *device-only:* readable in the hand on the real LCD.
- [ ] **Contrast**: the grey labels and dark grey inactive things are visible but clearly
  second; in the C64 theme the same is true on blue. Note any pair of colours that vanishes on
  the panel (the dark grey on black especially, which the panel may show darker than the
  monitor).
  *Emulator:* `sidou-tests/tests/app/theme.rs::theme_roles_pass_contrast`,
  `sidou-tests/tests/app/theme.rs::every_role_is_a_palette_colour_and_reads_on_the_background_and_the_cursor_row`
  (contrast as computed from the palette); *device-only:* which colour pairs vanish on the real
  panel, dark grey on black especially (colours as seen).

Title cards ([controls](reference/controls.md#title-cards)):

- [ ] With CARDS at ONCE (the default, set it first in SETTINGS), the first SCENE and the first
  CHIP after power-on play their card and the second opening of either does not; the way back
  to LIVE plays none. With ALWAYS every opening plays it, THE OSCILLATOR's included.
  *Emulator:* `emu-gate/tests/screen.rs::a_card_plays_once_then_not`,
  `emu-gate/tests/screen.rs::cards_always_and_off`,
  `sidou-tests/tests/app/title_card.rs::a_card_plays_once_then_not`.
- [ ] Menu, SCENE: THE SCENE's card first (as `assets/logo/title-scene.png`):
  a dark blue to purple dithered glow fading in over about a second in four
  steps, THE and SCENE in the metal lettering wiping in from the left, a
  second held, then SCENE cutting in from the top in bands. Look for wrong
  colours, torn cells, a shimmer in the dither or a step of the glow that
  flashes. Time from the turn to SCENE (about 2.6 s):
  *Emulator:* `emu-gate/tests/screen.rs::title_cards_match_their_goldens_and_their_sources`,
  `sidou-tests/tests/app/title_card.rs::the_glow_fades_in_through_its_dither_levels`,
  `sidou-tests/tests/app/title_card.rs::scene_from_the_menu_shows_its_card_then_cuts_to_scene`
  (frames, glow steps, sequence into SCENE, cell and SPI budgets in
  `emu-gate/tests/screen.rs::title_cards_stay_within_their_budgets`); *device-only:* wrong
  colours, torn cells, dither shimmer and a flashing glow step as seen on the real LCD, and the
  stopwatch time to SCENE (a record).
- [ ] Menu, CHIP: THE CHIP's card (as `assets/logo/title-chip.png`), then the
  CHIP view in bands.
  *Emulator:* `emu-gate/tests/screen.rs::title_cards_match_their_goldens_and_their_sources`,
  `sidou-tests/tests/app/title_card.rs::chip_from_the_menu_shows_its_card_then_cuts_to_chip`.
- [ ] CARDS at ALWAYS, menu, HOME: THE OSCILLATOR's card, then LIVE. Not at power-on, not
  after the boot, and not when MIDI learn's frame closes.
  *Emulator:* `emu-gate/tests/screen.rs::cards_always_and_off`,
  `sidou-tests/tests/app/title_card.rs::home_from_the_menu_to_live_shows_the_oscillator_card`,
  `sidou-tests/tests/app/title_card.rs::power_on_the_boot_and_back_from_a_view_to_the_menu_show_no_card`
  (card on HOME to LIVE, none at power-on); *emulator (no test yet):* no card when MIDI learn's
  frame closes under ALWAYS.
- [ ] OCT+, FX or SELECT on a card: the view comes in at once, and the
  octave, the chip and SCENE's piece stay as they were.
  *Emulator:* `emu-gate/tests/screen.rs::a_control_skips_a_title_card_and_does_nothing_else`,
  `sidou-tests/tests/app/title_card.rs::a_button_or_a_knob_skips_and_does_nothing_else`.
- [ ] A note key on a card, and a note over MIDI: it sounds at once (no
  delay against playing on LIVE) and the view comes in.
  *Emulator:* `emu-gate/tests/screen.rs::a_note_plays_through_a_title_card_and_ends_it`,
  `sidou-tests/tests/app/title_card.rs::a_note_plays_and_ends_the_card`.
- [ ] A chord held and played through a card: no click, crackle or late note.
  *Emulator (no test yet):* a chord held through a card: the audio equals the same chord with no
  card sample for sample, and every note-on is within the latency bound
  (`emu-gate/tests/latency.rs::a_note_on_reaches_the_buffer_within_a_block_and_a_poll_while_the_screen_redraws`
  covers redraws, not cards); *device-only:* a click or crackle heard (speaker).
- [ ] SETTINGS, CARDS turned counter-clockwise to OFF: SCENE and HOME back to LIVE
  now wipe in with no card. Clockwise: ONCE, then ALWAYS, and the cards are back. Set OFF,
  wait at least 6 s with no further changes and the keys released, then power-cycle:
  CARDS stays OFF; set ALWAYS and power-cycle: it stays ALWAYS. Restore ALWAYS or ONCE for the
  remaining title-card checks (they need every card to play).
  *Emulator:* `sidou-tests/tests/app/title_card.rs::the_cards_option_steps_off_once_always`,
  `emu-gate/tests/settings.rs::cards_the_envelope_mode_and_a_learned_cc_survive_a_power_cycle`
  (OFF after a quiet save and a power cycle),
  `sidou-tests/tests/app/save_key.rs::a_menu_cards_change_is_background_saved_without_save`;
  ALWAYS surviving a power cycle:
  `sidou-tests/tests/app/settings.rs::cards_byte_reads_old_and_new`,
  `sidou-tests/tests/app/settings.rs::every_persisted_value_round_trips_and_the_volume_does_not`.

Help card ([controls](reference/controls.md#help)):

- [ ] On LIVE, a quick tap of HOME opens the menu, and a tap in the menu goes back to LIVE; a
  tap never shows the card. Holding HOME about half a second shows HELP LIVE (title bar with the
  name on the right, eight lines: SELECT PAGE, ALG PROFILE, K1 CUTOFF, K2 RESO, K3 SOUND'S OWN,
  K4 RELEASE, PRESETS SOUND, E+K SLOT; SELECT turns to the second: SCL NEXT SLOT, ENV LFO FX
  LAB, ARP CHORD ARP, ARP+K ARP SET, OCT-/+ OCTAVE, SAVE+K LEARN, HOME MENU), drawn in a few
  frames with no flicker, and the menu does not open under it. Let HOME go: the card stays.
  *Emulator:*
  `emu-gate/tests/screen.rs::holding_home_shows_the_help_card_and_a_tap_still_goes_home`,
  `sidou-tests/tests/app/help_view.rs::a_tap_goes_home_and_a_hold_shows_the_help_card`,
  `sidou-tests/tests/app/help_view.rs::a_held_home_in_the_menu_keeps_the_menu`,
  `emu-gate/tests/screen.rs::the_help_card_stays_within_the_frame_budget` (golden help-live, no
  menu under it, frame budget).
- [ ] Any button or knob closes the card and does nothing else (no octave step, no chip
  toggle, no knob change, no save); a note key plays and the card stays. SAVE closes it and
  saves nothing.
  *Emulator:*
  `sidou-tests/tests/app/help_view.rs::any_control_closes_the_card_and_does_nothing_else_but_a_note_plays`,
  `sidou-tests/tests/app/help_view.rs::keys_under_a_help_card_play_the_view_under_it`.
- [ ] The card of the menu, CHIP, TRACKER (SEQ), SCENE and STATUS each names its view and
  reads right at 2x in the C64 and the high-contrast themes (yellow controls, white actions).
  *Emulator:* `emu-gate/tests/screen.rs::help_cards_match_their_goldens`,
  `emu-gate/tests/screen.rs::the_states_help_cards_match_their_goldens`,
  `sidou-tests/tests/app/help_view.rs::every_view_and_page_holds_home_to_its_own_card` (each
  card names its view); *device-only:* reads right at 2x in both themes on the real LCD
  (readability and colours as seen).

First-run tour ([controls](reference/controls.md#at-power-on)): after an install (or any
firmware version change), the full boot, then:

- [ ] Three large toasts one after the other, 1.6 s apart at least: TURN K1, PRESS ENV, and HOLD
  HOME FOR HELP on two lines. Turning K1 ends the first at once and PRESS ENV follows; pressing
  ENV opens LAB at its ENVELOPE page and ends the second; holding HOME there shows LAB ENV's
  help card with no toast left over it, and a tap of HOME then goes back to LIVE. Left alone each stays 8 s. Power-cycle: no tour. HOME held while powering on, or
  a normal boot: no tour.
  *Emulator:* `emu-gate/tests/screen.rs::the_tour_follows_the_first_boot_after_an_install`,
  `emu-gate/tests/screen.rs::no_tour_when_home_asked_for_the_boot_or_on_a_normal_boot`,
  `sidou-tests/tests/app/help_view.rs::an_unanswered_tour_runs_on_its_own_clock_and_ends`,
  `sidou-tests/tests/app/help_view.rs::an_action_during_the_tour_keeps_its_own_toast`.

CHIP (menu, CHIP; [controls](reference/controls.md#chip)):

- [ ] The page as `tests/golden/lcd/chip.png`, its state set over MIDI (the emulator test's
  CCs): AGE 100, DAC 40, THUMP 64, BLEED 20; DIST OVERDRIVE, MOD CHORUS, FX REVERB. CHIP A at
  2× in white, the CHIP tab yellow, `6581` and the profile at 3×, the four small control labels
  in yellow, cyan, light green and light grey (K4 white in C64), their bars in the knob's
  accent colour, and their values white at 3× (AGE's `100` at 2×, which is how three digits
  fit). The K1..K4 tags, names below the choice, KEYS marker, note and help lines use the
  readable hint colour; the three slots' algorithms are at 2× below.
  *Emulator:* `emu-gate/tests/screen.rs::chip_matches_its_golden_images` (the page against
  `tests/golden/lcd/chip.png` with the same MIDI state; labels, bars, values, 2x and 3x text,
  hint colour); *device-only:* how those colours and sizes look on the real LCD.
- [ ] SELECT through DIST, MOD and FX: each tab turns yellow in turn, the choice at 3× is the
  slot's algorithm, its controls are named for it (OVERDRIVE: DRIVE, TONE, BIAS, MIX), and the
  lines below are the chip and the other two slots. With every slot OFF (ALGORITHM
  counter-clockwise on each), a slot's control labels, values and bars are dark grey under
  their generic names (AMT, A, B, MIX), but the K1..K4 tags remain readable in the hint colour.
  *Emulator:* `emu-gate/tests/screen.rs::chip_matches_its_golden_images` (DIST, MOD, FX pages
  after SELECT), `sidou-tests/tests/app/chip_view.rs::every_algorithm_names_its_controls`,
  `sidou-tests/tests/app/chip_view.rs::select_moves_the_focus_and_selects_the_slot`; *Emulator
  (no test yet):* the page with every slot OFF (dark grey generic names AMT, A, B, MIX, tags
  still in the hint colour) is not a named golden.
- [ ] ALGORITHM on CHIP between two 6581 profiles (R2 and R3, R3 and R4AR, either way) shows
  no toast and no raster, by design: only a model change (to or from 8580 R5) says CHIP 6581
  or CHIP 8580 and sweeps the border. The profile word changes either way.
  *Emulator:* `emu-gate/tests/screen.rs::a_chip_change_sweeps_the_raster_over_the_border` (the
  raster and toast for a model change),
  `sidou-tests/tests/app/chip_view.rs::algorithm_steps_the_profile_and_the_model_follows`;
  *Emulator (no test yet):* no toast and no raster for a step between two 6581 profiles
  (R2/R3/R4AR) while the profile word changes.
- [ ] Turning K1 fast end to end on CHIP: the AGE value and bar follow without lag or torn cells,
  and the sound darkens smoothly, with no click.
  *Emulator:*
  `emu-gate/tests/character.rs::the_chip_views_k1_turns_the_chips_age_and_matches_the_host_sample_exact`,
  `emu-gate/tests/screen.rs::chip_follows_midi_and_the_knobs_there_and_back`,
  `emu-gate/tests/screen.rs::chip_stays_within_its_budgets` (value, bar, ramp and audio, within
  the frame budget); *device-only:* a click heard while the sound darkens (speaker).
- [ ] MIDI CCs for every slot and character control at once (a controller's snapshot,
  or a DAW sending them in one go): the page catches up within a few frames, no cell
  left stale.
  *Emulator:* `sidou-tests/tests/app/chip_view.rs::midi_changes_show_on_the_page`,
  `sidou-tests/tests/app/chip_view.rs::a_burst_of_changes_stays_within_the_budget`,
  `emu-gate/tests/screen.rs::chip_follows_midi_and_the_knobs_there_and_back`.
- [ ] SELECT one stop after FX opens EXPERT. ALGORITHM steps its four pages and stops
  at either end; SELECT back returns to FX. The four physical knobs turn the labelled
  controls (K1/K2 upper pair, K3/K4 lower pair); the last page's unused positions do nothing.
  *Emulator:*
  `sidou-tests/tests/app/chip_view.rs::expert_is_one_more_select_stop_without_changing_the_selected_slot`,
  `sidou-tests/tests/app/chip_view.rs::expert_pages_bind_four_direct_knobs_and_leave_unused_knobs_inert`,
  `emu-gate/tests/screen.rs::expert_pages_are_readable_and_stay_inside_the_chip_frame_and_poll_budgets`.
- [ ] From CHIP and from EXPERT, press SEQ for TRACKER, then HOME immediately and again
  after TRACKER has finished drawing: CHIP returns whole, with no tracker rows left in it.
  Repeat with MIDI LEARN open over the returned CHIP page. Open SONG, return to TRACKER,
  and check its rows/name are restored rather than stale CHIP or SONG text.
  *Emulator:*
  `sidou-tests/tests/app/chip_view.rs::shared_canvas_rebuilds_chip_after_tracker_is_popped_without_enter`,
  `sidou-tests/tests/app/chip_view.rs::shared_canvas_is_acquired_by_the_view_composed_under_midi_learn_not_by_input`,
  `sidou-tests/tests/app/chip_view.rs::shared_canvas_restores_song_menu_after_tracker_and_chip_used_it`,
  `emu-gate/tests/screen.rs::applied_song_edits_refresh_the_tracker_before_and_after_playback`
  (no stale rows after SEQ then HOME, under MIDI learn, after SONG).
- [ ] In both HIGH CONTRAST and C64 themes, the expert names at 2× and unit values at
  2×/3× are readable in the hand and at arm's length. Active large names keep the knob accents:
  K1 yellow, K2 cyan, K3 light green and K4 light red (light grey in C64). The small knob tags
  and the macro reminder use the neutral, readable hint colour, not the physical knob accent.
  Popup bars retain their theme colours (the knob accent in HIGH CONTRAST, light green in
  C64). Every popup names its own control; no value overlaps its neighbour or runs under the
  bezel. Model-specific inactive names and values dim, values say N/A, and tags remain readable.
  *Emulator:*
  `sidou-tests/tests/app/chip_view.rs::expert_uses_readable_coloured_pairs_and_refreshes_after_an_external_edit`,
  `sidou-tests/tests/app/popup.rs::every_expert_knob_popup_has_its_name_and_physical_units`,
  `sidou-tests/tests/app/theme.rs::the_popup_bar_and_name_take_the_knobs_colour_in_both_themes`
  (colour roles, names, bar colours, no overlap in the grid); *device-only:* readable in the
  hand and at arm's length on the real LCD, nothing under the bezel.
- [ ] With a fresh R4AR setup, following defaults read 100% nonlinearity, 0.75% DAC leakage,
  1.16 DC filter bias and 13154 Hz external low-pass. These are physical units, not raw
  MIDI values. Compare the four pages with `chip-expert-1.png` through `chip-expert-4.png`.
  *Emulator:*
  `sidou-tests/tests/app/popup.rs::expert_formatter_uses_the_core_ranges_and_noise_tap_bands`,
  `emu-gate/tests/screen.rs::expert_pages_are_readable_and_stay_inside_the_chip_frame_and_poll_budgets`
  (the four expert pages and their values in physical units; the images `chip-expert-1.png` to
  `-4.png` are the host-side reference).

SCENE (menu, SCENE; [controls](reference/controls.md#scene)):

- [ ] It opens on FIRE: black, with a brown ember line at the bottom that
  holds still in silence.
  *Emulator:* `emu-gate/tests/screen.rs::fire_keeps_a_low_flame_in_silence`,
  `emu-gate/tests/screen.rs::scene_fire_matches_its_golden_image`,
  `sidou-tests/tests/app/scene.rs::a_silent_fire_keeps_a_low_flame`.
- [ ] SELECT clockwise: PLASMA, RULE 30, STARS, ANT, RED MOON RITE, then
  FIRE again, each wiping in from the top with its name as a toast;
  counter-clockwise goes back. HOME returns to the menu.
  *Emulator:*
  `sidou-tests/tests/app/scene.rs::scene_opens_from_the_menu_and_select_steps_the_pieces`,
  `emu-gate/tests/screen.rs::algorithm_steps_the_piece`,
  `emu-gate/tests/screen.rs::entering_scene_names_the_piece` (order, name toast, back);
  `sidou-tests/tests/app/scene.rs::the_wipes_into_scene_send_only_their_rows` (wipe).
- [ ] FIRE: a key flares and heats one flame (the first note the left
  third); a three-note chord all three, in tongues of red and light red;
  higher notes burn higher; K1 down dulls them to red and light red, up
  burns yellow and white near the bed. Released, they die back to the
  embers.
  *Emulator:*
  `sidou-tests/tests/app/scene.rs::the_fire_burns_in_its_rows_with_each_voices_envelope`,
  `sidou-tests/tests/app/scene.rs::the_cutoff_sets_how_hot_the_fire_burns`,
  `sidou-tests/tests/app/scene.rs::the_fire_burns_up_its_ramp_in_tongues`.
- [ ] PLASMA: blue and purple dithered clouds drifting slowly in silence,
  faster with notes held and with K1 up; each note-on drops a bright ring
  that grows from a place set by the note (higher notes higher up).
  *Emulator:*
  `sidou-tests/tests/app/scene.rs::the_plasma_fills_the_screen_and_the_cutoff_speeds_it`,
  `sidou-tests/tests/app/scene.rs::a_note_on_drops_a_ripple_where_its_pitch_says`,
  `emu-gate/tests/screen.rs::scene_plasma_matches_its_golden_image`.
- [ ] RULE 30: a grey triangle at the start; each note-on writes a new
  generation in the voice's colour (SAW LEAD's yellow) on the row below the
  last, a held note keeps it going, and it stops in silence. K1 turned well
  down darkens the colour.
  *Emulator:*
  `sidou-tests/tests/app/scene.rs::rule30_starts_with_its_pattern_in_grey_and_holds_still_in_silence`,
  `sidou-tests/tests/app/scene.rs::rule30_steps_at_each_note_on_in_the_voices_colour`,
  `sidou-tests/tests/app/scene.rs::rule30_keeps_stepping_while_a_voice_sounds_and_stops_with_it`;
  *Emulator (no test yet):* K1 turned well down darkening the colour.
- [ ] STARS: stars fly out of the centre, slowly in silence, faster with
  notes; a note-on lights a third of them in its colour for a moment, and
  high notes steer the centre right, low ones left.
  *Emulator:*
  `sidou-tests/tests/app/scene.rs::the_stars_fly_out_faster_with_the_envelope_and_steer_with_the_pitch`,
  `sidou-tests/tests/app/scene.rs::a_note_on_lights_the_stars_before_its_envelope_rises`,
  `sidou-tests/tests/app/scene.rs::the_stars_are_drawn_far_to_near_and_light_with_their_voice`.
- [ ] ANT: three white balls (grey while their voice is silent) walking
  Langton's ant trails while their voices sound, one per voice.
  *Emulator:* `sidou-tests/tests/app/scene.rs::the_ants_follow_langtons_rules`,
  `emu-gate/tests/screen.rs::scene_ant_matches_its_golden_image`; *emulator (no test yet):* the
  ball turning grey while its voice is silent.
- [ ] No flicker or tearing beyond the pieces' own motion, the sound stays
  clean (SCENE taps the audio interrupt as LIVE does), and fast playing
  (a run of notes, a chord stab) keeps the sound and the keys on time.
  *Emulator:* `emu-gate/tests/screen.rs::scene_stays_within_its_budgets`,
  `emu-gate/tests/screen.rs::a_note_moves_every_piece_and_silence_calms_it`,
  `sidou-tests/tests/app/scene.rs::every_piece_stays_within_the_budget_while_the_band_plays`,
  `sidou-tests/tests/app/scope.rs::the_tap_takes_a_point_every_decimate_samples_and_leaves_the_audio_alone`
  (budgets, tap leaves the audio alone),
  `emu-gate/tests/latency.rs::a_note_on_reaches_the_buffer_within_a_block_and_a_poll_while_the_screen_redraws`
  (keys on time); *device-only:* flicker and tearing as seen and any click heard (panel look and
  speaker).

RED MOON RITE (SELECT counter-clockwise from FIRE; [art](art/README.md#red-moon-rite),
[controls](reference/controls.md#red-moon-rite)): the panel's look of the drawn piece, which
only the real LCD shows.

- [ ] Still, it looks like `tests/golden/lcd/scene-rmr-song.png` (two barbarians in guard
  under a blood moon, a ruin with two torches, a broadsword stamina blade a side and VS):
  the colours as in the PNG on a computer screen (no colour swapped or washed out), the
  wide pixels square-ish, no torn or shifted 8 x 8 cell.
  *Emulator:* `emu-gate/tests/screen.rs::scene_rmr_matches_its_golden_image`,
  `sidou-tests/tests/app/scene_art.rs::every_image_decodes_to_its_png_pixel_for_pixel`,
  `sidou-tests/tests/app/scene_rmr_drawing.rs::red_moon_rite_pngs_match_their_drawing_code` (the
  framebuffer equals the PNG: no swapped cell or torn 8 x 8 cell); *device-only:* colours washed
  out or swapped as seen on the real panel and the wide pixels looking square-ish (look on the
  real LCD).
- [ ] The dither (the moon's halo, the sky's seams between bands, the flagstones' half
  tones, the fighters' shading): does any area shimmer or crawl, or show moire, when the
  piece is still? Note where.
  *Emulator:* `sidou-tests/tests/app/scene_px.rs::an_unchanged_frame_scans_and_sends_nothing` (a
  still piece sends nothing, so the data does not crawl); *device-only:* shimmer or moire in the
  dither as seen on the real panel.
- [ ] PLAY (button 12): the song plays the duel. Kicks chop, snares parry with a white and
  yellow spark, the hats shift the guards, the torches flicker and flare on the lead's notes;
  near the end of the fourth bar the spinning attack knocks a sword away, it spins through
  the air and sticks in the floor, and a beat later its owner bends and pulls it free. The
  motion looks smooth (25 frames a second) with no cell lagging behind the rest of a
  fighter for more than a frame, and the sound stays clean.
  *Emulator:*
  `emu-gate/tests/screen.rs::red_moon_rite_plays_the_demo_song_as_its_golden_images`,
  `emu-gate/tests/screen.rs::red_moon_rite_stays_within_its_budgets_over_a_minute_of_the_song`,
  `sidou-tests/tests/app/scene_rmr.rs::the_vignette_plays_the_script_frame_for_frame`,
  `emu-gate/tests/song.rs::the_first_song_is_bit_exact_while_red_moon_rite_plays_it` (the duel
  script, frame rate and cells, audio untouched); *device-only:* the motion looking smooth to
  the eye and the sound staying clean as heard (panel look and speaker).
- [ ] K1 (the keys' cutoff, with the song stopped) or the song's filter darkens and
  brightens the sky in three palettes, wiped down a row at a time.
  *Emulator:* `sidou-tests/tests/app/scene_rmr.rs::the_sky_takes_the_cutoffs_palette`; *Emulator
  (no test yet):* the sky wiped down a row at a time with the key cutoff and with the song's
  filter.
- [ ] PLAY again (stop): once the notes have faded the duel settles and the torches hold
  still; the picture stays clean (no stale or flickering cell).
  *Emulator:* `emu-gate/tests/screen.rs::red_moon_rite_calms_when_the_song_stops` (nothing sent
  once the duel has settled).
- [ ] Hold SAVE and turn K1 (MIDI learn's frame) over it: the frame reads as over the other
  pieces (white on blue), the duel goes on around it, and HOME brings the duel back whole.
  *Emulator:*
  `emu-gate/tests/screen.rs::midi_learn_over_scene_and_live_stays_within_the_budget`,
  `sidou-tests/tests/app/scene.rs::learns_frame_over_every_piece_and_over_live_stays_within_the_budget`,
  `emu-gate/tests/screen.rs::midi_learn_prompts_match_their_golden_images` (frame over each piece,
  duel goes on); *emulator (no test yet):* white on blue frame colours over RED MOON RITE and
  the duel whole again after HOME.

### 6.3 Two chips, routing and UI sounds (M2b)

These are listening/panel checks for the user; emulator instruction counts do not mark
them passed. Use a moderate headphone level. Keep the USB-output release gate in section
8.1 open until it passes independently.

- [ ] With KEYS AUTO and the song stopped, the keys sound on A as before. PLAY starts the
  song on A; newly played keys sound independently above it on B, with the same keys'
  profile and character. LIVE says KEYS:B. Stop the song with a key held: it continues
  without a cut; after release and A's preparation, new keys return to A. B's release
  tails finish without a click or a frozen tail on its next use.
  *Emulator:*
  `sidou-tests/tests/app/keys_route.rs::a_song_moves_the_keys_to_chip_b_and_back_when_no_key_is_held`,
  `emu-gate/tests/keys.rs::a_release_tail_plays_out_on_chip_b_after_the_keys_move_back`,
  `emu-gate/tests/chip_b.rs::chip_b_fades_in_plays_and_fades_out_bit_exact_with_the_host`,
  `emu-gate/tests/screen.rs::live_shows_the_keys_on_chip_b` (routing, KEYS:B, tails, same
  profile in
  `sidou-tests/tests/app/keys_route.rs::chip_b_holds_the_keys_setup_after_every_change`);
  *Device-only:* whether a cut or click can be heard in the room (speaker).
- [ ] Hold keys across PLAY: they re-sound on B while the song starts on A. No stuck note
  remains on either chip after releasing the keys and stopping the song.
  *Emulator:*
  `sidou-tests/tests/app/keys_route.rs::a_song_moves_the_keys_to_chip_b_and_back_when_no_key_is_held`
  (a key held across the start is released on A and sounds again on B),
  `emu-gate/tests/keys.rs::a_release_tail_plays_out_on_chip_b_after_the_keys_move_back`,
  `sidou-tests/tests/app/keys_route.rs::a_song_before_chip_b_is_ready_waits_and_chip_a_is_already_the_songs`.
- [ ] On CHIP, SELECT counter-clockwise to the A/B tab focus, then ALGORITHM between A and
  B. The title and KEYS marker follow the chosen chip. With no song, B says FOLLOWS KEYS;
  editing it changes the keys' setup. While a song plays, A edits the song's setup and B
  the keys'. Effects remain one shared chain, with the B tab's shared-slot indication.
  *Emulator:*
  `sidou-tests/tests/app/chip_view.rs::the_tabs_switch_mark_the_keys_and_show_the_keys_setup`,
  `sidou-tests/tests/app/chip_view.rs::during_a_song_the_a_tab_edits_the_songs_setup`,
  `emu-gate/tests/screen.rs::a_profile_step_on_the_b_tab_leaves_chip_a_bit_exact`.
- [ ] Select KEYS A in SETTINGS and play the song: a held key temporarily takes a song
  voice; releasing it gives the voice back, with no stuck gate. The song keeps its filter
  and volume; the instrument's filter knobs do not take them over. Restore AUTO.
  *Emulator:*
  `emu-gate/tests/song.rs::keys_a_over_a_playing_song_keeps_every_midi_notes_pitch_and_gate`,
  `sidou-tests/tests/app/keys_route.rs::keys_a_takes_a_voice_from_the_song_and_gives_it_back`,
  `sidou-tests/tests/app/keys_route.rs::keys_a_leaves_the_songs_other_voices_byte_for_byte`;
  *Emulator (no test yet):* the song keeping its filter and volume while instrument filter knobs
  turn.
- [ ] While a chord holds on the keys' chip, step a built-in profile both ways (ALGORITHM on
  LIVE's TONE, 6581 R4AR to 8580 R5 and back). No silent block, dropout or crackle occurs during preparation. A profile/model
  change intentionally re-attacks held notes; an expert edit below must not.
  *Emulator:* `emu-gate/tests/live.rs::the_model_toggle_swaps_chips_without_a_gap`,
  `emu-gate/tests/screen.rs::chip_resets_at_full_character_stay_within_the_poll_allowance`,
  `emu-gate/tests/latency.rs::a_note_on_keeps_its_bounds_while_a_profile_step_is_prepared`,
  `emu-gate/tests/character.rs::expert_nrpn_changes_a_held_notes_sound_without_a_reset` (no gap
  while a spare chip is prepared; an expert edit does not reset the held note); *Emulator (no
  test yet):* a profile or model change re-attacking the held notes; *device-only:* crackle
  heard during preparation (speaker).
- [ ] SOUNDS is OFF by default: menu moves and confirmations add no sound. Turn it ON:
  menu SELECT gives a blip, confirmation a confirm sound, a toast its chime, a completed
  explicit SAVE the drive chunk, and a refused action a buzz. They do not change the
  keys' setup or effect settings. Restore OFF and check it remains OFF after a quiet save
  and power-cycle.
  *Emulator:* `emu-gate/tests/screen.rs::with_sounds_on_a_menu_move_blips_and_off_it_is_silent`,
  `sidou-tests/tests/app/ui_sound.rs::each_sound_steps_as_goattrackers_format_says`,
  `sidou-tests/tests/app/ui_sound.rs::off_writes_nothing` (the register writes of each sound and
  OFF silent); *emulator (no test yet):* SOUNDS staying OFF after a quiet save and a power cycle
  (`sidou-tests/tests/app/settings.rs::every_persisted_value_round_trips_and_the_volume_does_not`
  is the nearest test); *device-only:* how the blip, confirm, chime, drive chunk and buzz sound
  in the room (speaker).
- [ ] With AUTO and a song playing, hold three keys on B and move through the menu with
  SOUNDS ON: no cursor sound and no held note stolen or changed. With a UI sound playing
  and the other B voices busy, a new note takes priority immediately; its release leaves
  no stuck note. Turn SOUNDS OFF afterwards.
  *Emulator:*
  `emu-gate/tests/screen.rs::with_three_notes_held_on_chip_b_a_menu_move_is_silent_and_bit_exact`,
  `sidou-tests/tests/app/ui_sound.rs::three_held_notes_leave_no_voice_for_a_sound`,
  `sidou-tests/tests/app/ui_sound.rs::a_note_takes_the_voice_back_from_a_sound`,
  `sidou-tests/tests/app/ui_sound.rs::the_voice_is_given_back_with_lives_registers`.
- [ ] Song plus keys at a loud setting: listen for clipping at the shared mix and record
  the profiles/notes/MASTER level if heard. Two loud, in-phase chips are soft limited, not
  clipped: the output has a soft limiter where it used to clamp, before MASTER
  ([decision record 0004](decisions/0004-effect-algorithms.md), The output's soft limiter), and
  the build applies no automatic attenuation. Also listen during expert edits,
  screen transitions and a USB replug; record any click, dropout or late note.
  *Emulator:*
  `sidou-tests/tests/app/chip_mix.rs::two_loud_chips_through_the_slots_are_soft_limited_never_on_a_rail`,
  `sidou-tests/tests/app/chip_mix.rs::one_chips_loudest_steady_sample_is_as_recorded` (the
  shared mix as numbers); *device-only:* clipping heard at a loud setting, the
  profiles/notes/MASTER level recorded, and any click, dropout or late note heard during expert
  edits, screen transitions and a USB replug (speaker; real USB host and replug).

### 6.4 Expert edits without a note reset (M2b Task 6)

- [ ] Hold a triangle note (or a steady chord) while changing every applicable expert,
  including filter curve/range and DAC kink/leakage. Pitch/envelope/gate stay continuous:
  no new attack, silent gap or click. Table changes may glide over several ticks; check
  both the 6581 and 8580, skipping their N/A controls. The external corners change tone,
  combined strength changes combined waves, and noise feedback changes a noise combination.
  *Emulator:*
  `emu-gate/tests/character.rs::expert_nrpn_changes_a_held_notes_sound_without_a_reset`,
  `emu-gate/tests/character.rs::expert_filter_builds_are_sliced_and_never_reset_a_held_gate`,
  `sidou-tests/tests/expert_transport.rs::expert_nrpn_edits_keep_a_held_gate_and_macros_reclaim_the_path`,
  `sidou-tests/tests/app/keys_route.rs::expert_setup_follows_the_keys_without_resetting_a_held_note`
  (gate and note kept); *emulator (no test yet):* every applicable expert in turn on a held
  triangle on both models, with pitch and envelope continuous and no new attack (named tests
  cover the held gate and selected controls, not each expert).
- [ ] Edit 6581 curve, then move AGE: the curve returns to AGE's path. A filter-bias edit
  stays when AGE moves. DAC reclaims only kink/leakage, THUMP only output thump, BLEED only
  filter bleed. A different built-in profile/model clears all overrides; a clamped picker
  step or an empty USER choice leaves them intact. The page's macro reminder is readable.
  *Emulator:*
  `sidou-tests/tests/expert_transport.rs::expert_nrpn_edits_keep_a_held_gate_and_macros_reclaim_the_path`,
  `sidou-tests/tests/app/chip_view.rs::clamped_or_empty_profile_steps_do_not_discard_expert_edits`,
  `sidou-tests/tests/app/user_profile.rs::picking_a_user_goes_through_the_profile_change_and_the_character_ramp`
  (AGE, macros reclaim paths; clamped or empty USER steps keep experts); *device-only:* whether
  the macro reminder reads well on the real LCD (readability).
- [ ] Send the generated chart's expert NRPNs 2/5–18 over TRS and USB: their values and the
  sound follow, with no popup opened by MIDI and no echo back. On EXPERT, SAVE held + a
  knob learns that expert without changing it; a learned CC can reach both endpoints.
  In a USB monitor, a local expert turn sends its NRPN, or its learned CC. Mixed macro and
  expert automation reaches the final values without a stale override arriving later.
  *Emulator:*
  `emu-gate/tests/character.rs::expert_nrpn_changes_a_held_notes_sound_without_a_reset`,
  `sidou-tests/tests/expert_transport.rs::expert_tracking_suppresses_remote_echoes_and_forgets_dropped_or_resynced_edits`,
  `sidou-tests/tests/app/popup.rs::midi_changes_never_open_the_popup`,
  `sidou-tests/tests/app/midi_learn.rs::a_learned_cc_takes_the_parameters_curve_and_range` (NRPN
  values, no popup, no echo, learned CC range); *device-only:* the chart sent from a real DAW
  over USB and a real USB monitor.
- [ ] Save and recall experts with a USER profile (section 7.2), including on B and on
  the song's A tab. Release notes and wait at least 6 s after expert/settings changes,
  then power-cycle: the keys' explicit experts and learned bindings return; following
  fields still follow a subsequent related macro move.
  *Emulator:*
  `emu-gate/tests/user_profile.rs::a_user_profile_saved_from_the_chip_view_survives_a_power_cycle_and_sounds_as_saved`,
  `sidou-tests/tests/app/user_profile.rs::saving_expert_overrides_keeps_raw_sentinels_across_a_reboot`,
  `sidou-tests/tests/app/settings.rs::expert_raw_overrides_and_learn_bindings_survive_loading_after_macros`,
  `sidou-tests/tests/app/user_profile.rs::on_a_songs_chip_a_the_save_target_stores_the_songs_setup_not_the_keys`;
  *Emulator (no test yet):* the same save and recall from chip B's tab.

Notes:

## 7. MIDI over the TRS jack

Which TRS-to-DIN adapter (type A or B) the FM-1 needs is not known yet:
record the one that works.

- [ ] Adapter type that works:
  *Device-only:* record of which TRS-to-DIN adapter type works (external gear: the MIDI
  adapter).
- [ ] Note on/off on any channel (1..16) plays and releases.
  *Emulator:* `emu-gate/tests/midi.rs::a_trs_note_on_plays_c4_and_the_note_off_releases_it`,
  `sidou-tests/tests/app/midi_route.rs::omni_is_the_default_and_every_channel_plays_the_instrument`.
- [ ] CC 74 cutoff, CC 71 resonance, CC 1 pulse width, CC 7 volume move the
  sound and the bars.
  *Emulator:* `emu-gate/tests/midi.rs::cc74_moves_the_cutoff_and_takes_the_highs_out` (cutoff on
  the device), `sidou-tests/tests/app/midi_rx.rs::ccs_follow_the_default_mapping`,
  `sidou-tests/tests/app/live_instrument.rs::ccs_drive_the_parameters` (CC 74, 71, 1, 7 reach
  the parameters on the host); *emulator (no test yet):* CC 71, CC 1 and CC 7 over TRS on the
  device image, and the bars on screen.
- [ ] Program change 0..3 selects the preset; 4 and above change nothing.
  *Emulator:* `emu-gate/tests/midi.rs::program_change_switches_the_preset`.
- [ ] CC 123 (all notes off) and CC 120 release held notes.
  *Emulator:* `sidou-tests/tests/app/midi_rx.rs::ccs_follow_the_default_mapping` (CC 123
  releases held notes), `sidou-tests/tests/app/live_instrument.rs::ccs_drive_the_parameters` (CC
  123 and CC 120),
  `sidou-tests/tests/app/midi_route.rs::all_notes_off_releases_the_channels_voices_only`.
- [ ] CC 112 = 90 and CC 116 = 127 turn the FX slot's REVERB on
  ([controls.md](reference/controls.md)); CC 116 = 0 back to dry.
  *Emulator:* `emu-gate/tests/fx_live.rs::trs_ccs_turn_on_the_reverb_in_stereo_with_a_tail`.
- [ ] A dense part from a sequencer (running status, clock on) plays without
  hanging notes. Latency feels:
  *Emulator:* `emu-gate/tests/midi.rs::a_full_speed_stream_loses_nothing`,
  `emu-gate/tests/midi.rs::live_under_a_full_speed_midi_flood_stays_within_its_budget`,
  `sidou-tests/tests/app/midi_in.rs::running_status` (running status, no loss at the jack's full
  speed); *device-only:* a real sequencer through the adapter, hanging notes seen on it and how
  the latency feels (external gear, feel).

**MIDI learn (SAVE held + knob)** ([controls.md](reference/controls.md), MIDI learn), with a controller
that sends a CC the map leaves free (e.g. CC 14; [midi.md](reference/midi.md) lists the
map's) on the TRS jack. While the frame waits for a CC, any learnable CC binds, whatever
sends it: a DAW that sends CC 0, 32 or 64 when its transport starts, or a controller's
pedal, binds that instead. Stop the transport and keep pedals still while learning.

- [ ] Press and hold SAVE: no learn frame yet. Turn K1: the MIDI LEARN frame opens on
  CUTOFF &larr; SEND A CC, and the cutoff does not move. Release SAVE: no save toast,
  and the frame stays. Move the controller's
  knob: LEARNED CC 14 &rarr; CUTOFF, the frame closes, and the controller's knob now sweeps
  the cutoff (the bar and the sound) on any channel. CC 74 still moves it too.
  *Emulator:* `emu-gate/tests/midi.rs::midi_learn_over_trs_binds_cc_14_to_k1s_cutoff`,
  `sidou-tests/tests/app/learn_view.rs::save_held_and_a_knob_open_the_frame_on_its_parameter_and_a_cc_learns`,
  `emu-gate/tests/screen.rs::midi_learn_prompts_match_their_golden_images`,
  `sidou-tests/tests/app/midi_learn.rs::the_cc_that_teaches_also_sets_and_then_the_parameter_follows_it_on_every_channel`.
- [ ] The same over USB from a DAW, and the other way: learn K2 on CC 15 from USB, then
  send CC 15 from a TRS controller: both ports drive what either learned.
  *Emulator (no test yet):* learning over USB then driving from TRS (and the reverse): the TRS
  test is `emu-gate/tests/midi.rs::midi_learn_over_trs_binds_cc_14_to_k1s_cutoff`, nothing
  injects USB for learn; *device-only:* the real DAW as the sender (external gear).
- [ ] EDIT held + K4 while learning, then a CC: the selected slot's Mix follows it
  (LEARNED CC n &rarr; DIST MIX; SCL first for MOD or FX).
  *Emulator:*
  `sidou-tests/tests/app/learn_view.rs::edit_held_picks_the_selected_slots_controls`.
- [ ] On the CHIP view with the focus on CHIP, hold SAVE and turn K1: AGE &larr; SEND A CC;
  release SAVE, then send the CC: it moves AGE's bar and the sound. On a slot, hold SAVE
  and turn K4: that slot's MIX.
  *Emulator:* `sidou-tests/tests/app/chip_view.rs::midi_learn_binds_a_chip_view_knob`,
  `sidou-tests/tests/app/chip_view.rs::the_character_parameters_have_names`.
- [ ] On the CHIP view with the focus on CHIP, hold SAVE and turn K1, release SAVE, then
  SELECT one step clockwise while the
  frame shows: the focus moves to DIST under the frame, and K4 then learns DIST MIX (not
  BLEED). ALGORITHM under the frame steps DIST's algorithm. In the menu, SELECT and
  ALGORITHM do nothing while the frame shows.
  *Emulator:*
  `sidou-tests/tests/app/chip_view.rs::select_and_algorithm_work_under_the_learn_frame`,
  `sidou-tests/tests/app/learn_view.rs::select_and_algorithm_reach_nothing_over_the_menu`.
- [ ] Learn the resonance on CC 74: CC 74 now moves the resonance, not the cutoff (the
  cutoff's NRPN 0/0 still does).
  *Emulator:*
  `sidou-tests/tests/app/midi_learn.rs::a_learned_cc_that_was_another_parameters_wins_and_gives_it_back_when_forgotten`,
  `sidou-tests/tests/app/midi_learn.rs::one_cc_per_parameter_and_one_parameter_per_cc`.
- [ ] With a DAW monitoring SIDOU's USB output, turn K1 after learning CC 14 for it: the
  monitor shows CC 14, not CC 74. Moving the controller's knob sends nothing back.
  *Emulator:*
  `sidou-tests/tests/app/midi_learn.rs::a_panel_change_to_a_learned_parameter_goes_out_on_the_learned_cc`,
  `sidou-tests/tests/app/midi_learn.rs::a_learned_cc_from_midi_is_never_echoed_even_on_cc_1`,
  `emu-gate/tests/midi.rs::the_panel_goes_out_over_usb_and_midi_in_is_not_echoed`.
- [ ] HOME while the frame shows: LEARN CANCELLED, no menu, nothing learned. Leave the
  frame alone for 10 s: LEARN TIMED OUT.
  *Emulator:* `sidou-tests/tests/app/learn_view.rs::home_cancels_and_reaches_nothing_else`,
  `sidou-tests/tests/app/learn_view.rs::ten_seconds_without_a_step_time_out`,
  `sidou-tests/tests/app/midi_learn.rs::home_cancels_at_every_step_and_binds_nothing`.
- [ ] Hold SAVE and turn K1, release SAVE, then press and release SAVE: CUTOFF: CC 14
  FORGOTTEN, and CC 14 does nothing. Repeat with no binding: CUTOFF: NO CC, frame stays.
  *Emulator:* `sidou-tests/tests/app/learn_view.rs::save_after_a_knob_forgets_that_knobs_cc`,
  `sidou-tests/tests/app/midi_learn.rs::forget_instead_of_a_cc_forgets_the_picked_parameters_binding`.
- [ ] With the learn frame open, hold SAVE and press HOME: CLEAR ALL MAPPINGS?. Release
  SAVE: nothing cleared yet. Press and release SAVE again: MIDI LEARN CLEARED. Repeat with
  HOME without SAVE held instead of confirming, then with a 10 s wait: mappings are kept.
  *Emulator:*
  `sidou-tests/tests/app/learn_view.rs::save_held_and_home_ask_and_save_clears_every_mapping`,
  `sidou-tests/tests/app/learn_view.rs::home_or_ten_seconds_after_the_question_keep_the_mappings`,
  `sidou-tests/tests/app/midi_learn.rs::asked_then_confirmed_clears_every_binding`.
- [ ] Outside the frame, hold SAVE and press HOME: no menu and no frame; release SAVE:
  no save. Hold SAVE and press OCT+: octave changes, but release SAVE saves nothing.
  *Emulator:*
  `sidou-tests/tests/app/learn_view.rs::save_held_and_home_outside_the_frame_do_nothing`,
  `sidou-tests/tests/app/save_key.rs::another_button_while_save_is_down_acts_and_saves_nothing`.
- [ ] While the frame waits, turn K2 without holding SAVE: RESO &larr; SEND A CC; resonance
  does not move. On LIVE, SAVE held + K4 says K4: THE PRESET, NO CC and opens no frame.
  *Emulator:*
  `sidou-tests/tests/app/learn_view.rs::another_knob_with_save_held_or_not_picks_instead`,
  `sidou-tests/tests/app/learn_view.rs::k3_ratio_has_no_cc`,
  `sidou-tests/tests/app/midi_learn.rs::a_knob_without_a_parameter_opens_nothing_and_another_knob_picks_again`.
- [ ] The note keys play while the frame shows, and the sound has no gap when it opens or
  closes.
  *Emulator:*
  `sidou-tests/tests/app/learn_view.rs::the_keys_and_the_other_buttons_play_while_learn_waits`,
  `emu-gate/tests/screen.rs::midi_learn_over_scene_and_live_stays_within_the_budget` (keys play;
  frame fits the budget); *device-only:* whether a gap is heard when the frame opens or closes
  (speaker).
- [ ] Learn CC 14 for K1, release all notes and wait at least 6 s without changing a persisted
  value; power off and on: CC 14 still moves cutoff. Forget it, wait for another quiet save,
  power-cycle: the binding stays forgotten.
  *Emulator:*
  `emu-gate/tests/settings.rs::cards_the_envelope_mode_and_a_learned_cc_survive_a_power_cycle`,
  `sidou-tests/tests/app/save_key.rs::a_learned_binding_alone_is_background_saved_through_views_frame`,
  `sidou-tests/tests/app/settings.rs::binding_forgetting_and_clearing_are_background_saved`.

Notes:

## 7.1 Settings, SAVE and STORE (M2a Task 5)

Use a writable store for the persistence checks. Previous Felucca data in the
store's range reads as FOREIGN, not as a blank store. Read the destructive format check
below before deciding whether to format; nothing persists until then. SIDOU never formats
it without the menu's confirmation; preserve wanted data with the previous firmware first.

- [ ] Change CARDS, KEYS, SOUNDS and ROWS (TRACKER ROWS), the preset and a preset edit, the
  chip profile and a character control, an effect algorithm and Mix, and the envelope mode (CC 26). Release
  notes and wait at least 6 s after the last change, without further persisted changes:
  no background-save toast. Power-cycle: all those values and the learn binding return.
  Octave returns to 0, no notes are held, LIVE opens and volume follows MASTER.
  *Emulator:*
  `emu-gate/tests/settings.rs::cards_the_envelope_mode_and_a_learned_cc_survive_a_power_cycle`,
  `sidou-tests/tests/app/settings.rs::every_persisted_value_round_trips_and_the_volume_does_not`,
  `sidou-tests/tests/app/settings.rs::every_parameter_setter_marks_a_change_for_the_background_save`,
  `sidou-tests/tests/app/user_profile.rs::a_save_target_stores_the_chips_setup_without_changing_the_chip`
  (the stored fields and a power cycle; volume not stored).
- [ ] Make several changes less than 5 s apart, then stop: the settings save follows the
  last change, not the first. After a quiet save and power-cycle, the final values return.
  *Emulator:*
  `sidou-tests/tests/app/settings.rs::a_burst_of_changes_is_one_write_five_seconds_after_the_last`.
- [ ] Change CARDS, then press and hold SAVE: no frame and no save yet. Release without
  turning an encoder or pressing another button: SAVED appears when the write commits,
  without the 5 s debounce. Power-cycle after the toast: the new value returns.
  *Emulator:*
  `sidou-tests/tests/app/save_key.rs::a_clean_press_and_release_saves_at_the_release`,
  `sidou-tests/tests/app/settings.rs::save_now_skips_the_debounce_and_tells_when_it_commits`,
  `emu-gate/tests/settings.rs::saves_press_on_live_stores_the_settings_within_a_second`.
- [ ] Hold a chord and release SAVE to request a save. If it cannot write while playing,
  after about 2 s short held operations may interrupt the sound; completion says
  SAVED (GAPS). With SOUNDS on a completed explicit save plays the drive chunk. A background
  save alone waits for quiet and never forces gaps.
  *Emulator:*
  `sidou-tests/tests/app/settings.rs::an_explicit_save_on_a_short_pool_forces_the_held_operations_and_says_so`,
  `emu-gate/tests/store.rs::a_file_claims_before_it_writes_and_lands_while_a_chord_plays`,
  `emu-gate/tests/store.rs::no_erase_while_it_plays`,
  `sidou-tests/tests/app/user_profile.rs::a_store_that_waits_for_quiet_is_asked_again_then_forced_and_says_gaps`
  (wait for quiet, forced path, SAVED (GAPS), no forced gaps by a background save).
- [ ] In the menu, STORE is immediately after CARDS and shows nK (the kilobytes free), excluding the
  reserve pool. ALGORITHM on a writable SIDOU store does not format it.
  *Emulator:* `sidou-tests/tests/app/save_key.rs::the_menus_store_shows_the_free_space` (STORE
  shows nK); *emulator (no test yet):* ALGORITHM on a writable SIDOU store leaving it
  unformatted.
- [ ] **Only with unwanted foreign data and permission to destroy it:** STORE says FOREIGN.
  A plain SAVE says STORE FOREIGN: NOT SAVED; power-cycling without formatting leaves the
  region foreign. ALGORITHM clockwise once on STORE says FORMAT?; counter-clockwise
  cancels, as does moving SELECT away and back. Nothing is erased by those cancelled steps.
  *Emulator:*
  `sidou-tests/tests/app/save_key.rs::a_foreign_store_reads_foreign_a_clockwise_turn_arms_and_save_formats_it`,
  `sidou-tests/tests/app/settings.rs::a_foreign_store_keeps_nothing_and_an_explicit_save_says_so`,
  `sidou-tests/tests/app/user_profile.rs::a_foreign_store_keeps_nothing_and_says_so_but_the_slot_serves_until_reboot`
  (FOREIGN, NOT SAVED, FORMAT? and cancelling erase nothing).
- [ ] On FOREIGN, spin ALGORITHM clockwise fast: STORE says FORMAT? and nothing is erased.
  *Emulator:*
  `sidou-tests/tests/app/save_key.rs::a_fast_algorithm_spin_on_a_foreign_store_never_formats_it`.
- [ ] **Destructive confirmation:** on FOREIGN, turn ALGORITHM clockwise (FORMAT?, the popup
  says SAVE FORMATS), then press SAVE without leaving STORE. The store's 73 sectors are erased; STORE shows ERASE n (sound can have
  gaps), then STORE FORMATTED and nK. Let settings save, power-cycle, and check that
  STORE is no longer FOREIGN and settings persist. Never format automatically just to
  proceed with the checklist.
  *Emulator:*
  `emu-gate/tests/settings.rs::a_foreign_store_formats_on_the_menus_save_confirm_and_then_keeps_the_settings`,
  `sidou-tests/tests/app/save_key.rs::a_foreign_store_reads_foreign_a_clockwise_turn_arms_and_save_formats_it`
  (the confirmation, ERASE n, FORMATTED, settings kept); *device-only:* the real flash chip
  erasing the 73 sectors, its timing and the gaps in the sound (real silicon, flash chip timing;
  destructive).

Notes:

## 7.2 USER chip profiles (M2a Task 6)

Use a writable store (section 7.1). CHIP's chip or EXPERT focus, not its tabs or effect
slots, owns profile saves. A saved slot contains a base profile, AGE, DAC, THUMP and BLEED,
and all fourteen expert overrides (including fields following their defaults); the
settings keep the base setup, not the identity of the USER slot picked.

- [ ] On CHIP, select a built-in profile and set recognisable character and expert values. Hold EDIT
  and turn ALGORITHM to target an empty USER slot: the yellow title says SAVE TO USER n
  (with `-` for empty), and the popup names the target. Neither the current profile nor
  the sound changes. Step through USER 1..8 and OFF; leaving chip focus or closing CHIP
  clears the target.
  *Emulator:*
  `sidou-tests/tests/app/user_profile.rs::a_save_target_stores_the_chips_setup_without_changing_the_chip`,
  `sidou-tests/tests/app/user_profile.rs::the_target_steps_through_the_slots_and_off_and_clears_when_the_focus_leaves`,
  `sidou-tests/tests/app/popup.rs::the_chip_views_user_profiles_and_save_target_are_named`.
- [ ] Target an empty slot again, then press and cleanly release SAVE. Wait for
  SAVED TO USER n (or SAVED TO USER n (GAPS), after the 2 s forced path). The chip's setup
  has not changed. Power-cycle after completion: the slot remains. ALGORITHM's picker
  runs the five built-ins then filled USER slots, skipping empty ones with USER n EMPTY
  at an empty end. Pick the saved slot: its base model and four controls, and its sound,
  return, including the expert overrides; the PROFILE row and popup name USER n. Repeat
  EDIT + ALGORITHM and SAVE from EXPERT, selecting the target again after reaching it:
  passing through DIST, MOD or FX clears the previous target.
  *Emulator:*
  `emu-gate/tests/user_profile.rs::a_user_profile_saved_from_the_chip_view_survives_a_power_cycle_and_sounds_as_saved`,
  `emu-gate/tests/user_profile.rs::the_picker_steps_onto_the_saved_slot_after_a_power_cycle`,
  `sidou-tests/tests/app/user_profile.rs::the_picker_runs_the_built_in_profiles_then_the_filled_users_skipping_empty_ones`,
  `sidou-tests/tests/app/chip_view.rs::expert_save_targets_user_profiles_and_song_picker_restores_the_expert_tail`.
- [ ] Change a character control while that USER slot is applied, then SAVE with no target:
  OVERWRITE USER n? first. HOME keeps the old slot and leaves the view, clearing the
  question/target. Repeat, then a fresh SAVE press and release confirms. Reboot and pick
  the slot: only the confirmed changes survived.
  *Emulator:*
  `sidou-tests/tests/app/user_profile.rs::save_with_a_user_slot_applied_stores_back_into_it_asking_first`,
  `sidou-tests/tests/app/user_profile.rs::home_keeps_the_slot_and_forgets_the_question_and_the_target`;
  *Emulator (no test yet):* a reboot after the confirmed overwrite showing only the confirmed
  changes.
- [ ] Target a filled slot without applying it: SAVE asks before overwriting, and the
  current sound is stored only after the second SAVE. With a built-in profile and no
  target, SAVE instead saves settings (SAVED), not a USER slot.
  *Emulator:*
  `sidou-tests/tests/app/user_profile.rs::a_non_empty_slot_asks_before_it_is_overwritten`,
  `sidou-tests/tests/app/user_profile.rs::save_with_no_target_and_a_built_in_profile_saves_the_settings`.
- [ ] Repeat the save on chip B's tab showing the keys' setup. During a song, save chip A's
  setup from the A tab: the slot holds the song's setup, not the keys' or a temporary lock.
  *Emulator:*
  `sidou-tests/tests/app/user_profile.rs::on_a_songs_chip_a_the_save_target_stores_the_songs_setup_not_the_keys`,
  `sidou-tests/tests/app/chip_view.rs::during_a_song_the_a_tab_edits_the_songs_setup`; *Emulator
  (no test yet):* the save from chip B's tab storing the keys' setup.
- [ ] CC 85 / NRPN 2/0 can pick a filled USER slot (profile range 0..12); an empty one changes
  nothing. Use the current [MIDI chart](reference/midi.md), since the CC scaling changed
  when the eight slots were added.
  *Emulator:*
  `emu-gate/tests/user_profile.rs::bank_2_program_change_applies_a_saved_user_profile_like_cc_85`,
  `sidou-tests/tests/app/user_profile.rs::an_empty_slot_never_applies_from_a_cc`,
  `sidou-tests/tests/app/midi_route.rs::bank_2_and_cc_85_pick_the_same_builtin_and_saved_profiles_on_chip_a`;
  *Emulator (no test yet):* NRPN 2/0 picking a filled USER slot.
- [ ] If testing an unformatted foreign store, a profile save says STORE FOREIGN: NOT SAVED;
  it does not format or replace the foreign data. A RAM-only slot may work until reboot,
  but it is not persistent. Only the destructive confirmation in section 7.1 formats it.
  *Emulator:*
  `sidou-tests/tests/app/user_profile.rs::a_foreign_store_keeps_nothing_and_says_so_but_the_slot_serves_until_reboot`.

Notes:

## 8. MIDI over USB

- [ ] The unit enumerates as a USB-MIDI device "SIDOU FM-1" by "SIDOU
  contributors" (pid.codes test VID/PID 1209:0001). Record what the OS shows:
  *Emulator:* `emu-gate/tests/usb.rs::a_host_gets_the_identity_and_the_knobs_over_usb` (fm1-emu's host enumerates it: device descriptor VID/PID 1209:0001, the product string "SIDOU FM-1" over EP0 and the manufacturer's "SIDOU contributors" in usb.c, EP0 in 64-byte packets), `sidou-pack/tests/usb_audio.rs::the_device_is_one_composite_usb_midi_and_class_compliant_audio_input` (the descriptors).
  *Device-only:* what macOS, Windows or Linux show for the unit (real USB hosts and OSes).
- [ ] The same note, CC and program tests as section 7 pass over USB.
  *Emulator:* `sidou-tests/tests/app/midi_rx.rs::usb_packets_play_notes`, `sidou-tests/tests/app/midi_rx.rs::ccs_follow_the_default_mapping` (a USB CC 74 moves the cutoff), `sidou-tests/tests/app/midi_in.rs::a_channel_message_whole_is_its_bytes_one_at_a_time` (a USB packet gives the messages the TRS bytes give, so section 7's tests carry over).
  *Emulator (no test yet):* a note-on, CC 74 and a program change sent through fm1-emu's USB host (common/usb_host.rs) to the firmware image, checking pitch, cutoff and preset as `emu-gate/tests/midi.rs` does over TRS.
- [ ] TRS and USB at the same time both play.
  *Emulator:* `sidou-tests/tests/app/midi_rx.rs::a_usb_message_between_the_bytes_of_a_trs_message_leaves_both_whole`, `sidou-tests/tests/app/midi_rx.rs::a_gap_restarts_only_the_parser_of_the_source_it_lost`.
- [ ] **MIDI out (M1d):** in a DAW (or a MIDI monitor) on the SIDOU input, turn K1:
  CC 74 on channel 1 follows the knob, at most a message every 10 ms, and stops when
  the knob stops. K2 sends CC 71, K3 CC 1, MASTER CC 7, FX CC 25 (127 for the 8580,
  0 for the 6581), K4 CC 72 (the release), PRESETS a program change 0..3 and nothing else, and EDIT held with K1..K4
  CCs 103..106 for the Distortion slot (SCL moves to 108..111 and 113..116). On the CHIP
  view K1..K4 on CHIP send CC 86, 87, 89 and 90, ALGORITHM on CHIP CC 85, on a slot its
  algorithm's CC (102, 107, 112). Record
  what the monitor shows for one K1 turn:
  *Emulator:* `emu-gate/tests/usb.rs::a_host_gets_the_identity_and_the_knobs_over_usb` (the emulated host reads CC 74 on channel 1 from K1 and a program change from PRESETS), `emu-gate/tests/midi.rs::the_panel_goes_out_over_usb_and_midi_in_is_not_echoed` (nothing sent at rest, at most one CC a 10 ms tick ending on the latest value, FX gives CC 25 = 127 for the 8580 with CC 85), `sidou-tests/tests/app/chip_view.rs::midi_out_sends_the_chip_views_knobs` (the CHIP view's knobs and a slot's algorithm CC).
  *Emulator (no test yet):* the CC numbers the panel itself sends for K2 (71), K3 (1), MASTER (7), K4 and EDIT held with K1..K4 (103..106, SCL 108..111 and 113..116), turned through the panel into the emulated host; `sidou-tests/tests/app/midi_out.rs::the_rate_is_bounded_and_nothing_starves` checks the numbers on parameters, not through the knobs.
  *Device-only:* what the real monitor shows for one K1 turn (the record, on a real host and OS).
- [ ] Record K1 as automation in the DAW, then play it back into SIDOU over USB: the
  cutoff moves as recorded, and the monitor shows no CCs coming back from SIDOU while
  it plays (no echo).
  *Emulator:* `sidou-tests/tests/app/midi_out.rs::a_change_from_midi_is_never_echoed_on_either_port` (CC input over USB or TRS sends nothing back), `sidou-tests/tests/app/midi_rx.rs::ccs_follow_the_default_mapping` (a USB CC 74 moves the cutoff).
  *Device-only:* a real DAW recording the unit's CCs and playing them back through the OS's MIDI layer (real DAW, CoreMIDI or its equivalent).
- [ ] With the DAW's MIDI thru (input monitoring) on, send CC 74 to SIDOU over USB and
  over TRS: nothing comes back from SIDOU, so no feedback loop, no runaway messages.
  *Emulator:* `sidou-tests/tests/app/midi_out.rs::a_change_from_midi_is_never_echoed_on_either_port`, `emu-gate/tests/midi.rs::the_panel_goes_out_over_usb_and_midi_in_is_not_echoed` (CC 74 over TRS: nothing queued), `emu-gate/tests/midi.rs::a_learned_cc_flood_costs_what_a_map_cc_flood_does` (a full-speed flood is not echoed). SIDOU sends nothing it received, so a thru loop cannot start in the firmware.
- [ ] Notes from the keys are not sent (M1: parameters only).
  *Emulator (no test yet):* with a host attached, press and release panel keys and check that nothing is queued in `midi_out_q` (`midi.rs` checks knobs, FX and PRESETS only).
- [ ] **MIDI out across a host sleep:** with the monitor open, put the computer to
  sleep, turn K1 and K2 while it sleeps, then wake it: SIDOU still shows up, no burst of
  the values from the sleep arrives, and the next K1 turn shows CC 74 again within a
  second. Record what the monitor shows right after the wake:
  *Emulator:* `emu-gate/tests/midi.rs::the_panel_goes_out_over_usb_and_midi_in_is_not_echoed` (with no configured host a knob change is dropped and nothing queues, so no burst follows), `sidou-tests/tests/app/midi_out.rs::without_a_host_pending_changes_are_dropped`.
  *Device-only:* the real computer's suspend and resume on the cable, and what the monitor shows after the wake (real host and OS; the emulated host has no suspend).
- [ ] **MIDI out across a replug:** turn K1 continuously, unplug the USB cable mid-turn
  (on battery), keep turning a few seconds, plug it back in and keep turning: the sound
  never stops, the unit enumerates again, and CC 74 follows the knob again without a
  stale burst. Repeat with the cable pulled and replugged quickly (under a second).
  *Emulator:* `emu-gate/tests/latency.rs::a_usb_retry_stalls_a_note_on_by_its_attach_at_most` (the once-a-second USB retry never holds a note past its bound), `sidou-tests/tests/app/midi_out.rs::without_a_host_pending_changes_are_dropped` (nothing stale while no host).
  *Emulator (no test yet):* a bus reset and a new enumeration (`UsbHost::bus_reset`, then `enumerate`) while K1 turns: the sound goes on, the host sees CC 74 again with no stale burst.
  *Device-only:* the real cable pulled on battery and replugged within a second (cable and host timing).
- [ ] *(Optional, a build configured with `-DSIDOU_MIDI_OUT_NRPN=ON`.)* K1 sends NRPN
  0/0 as four CCs (99 0, 98 0, 6 and 38) at full 11-bit resolution, K3 NRPN 0/2; K2
  still sends CC 71. Record K1 at both ends (0 and 2047):
  *Emulator:* `sidou-tests/tests/app/midi_out.rs::high_resolution_parameters_go_as_nrpns_only_when_asked` (NRPN 0/0 as CCs 99 0, 98 0, 6 and 38, 11 bits, CC 71 unchanged), `sidou-tests/tests/app/midi_out.rs::every_parameter_reaches_a_second_instrument_through_its_stream` (the 11-bit values arrive exact).
  *Device-only:* the record of K1 at 0 and 2047 on a real monitor, with the NRPN build on the unit (real host).
- [ ] With the DAW closed (the port open by no application), turn the knobs for a
  minute: the sound and the panel stay responsive; opening the DAW afterwards gets no
  burst of old values beyond at most 64 messages.
  *Emulator:* `emu-gate/tests/midi.rs::midi_out_costs_little_and_waits_for_no_one` (a host that never reads: the ring stops at 64 packets, the main loop and a note go on).
- [ ] Unplug the USB cable while playing (on battery): the sound goes on
  without dropouts, and plugging it back in enumerates again within a few
  seconds.
  *Emulator:* `emu-gate/tests/latency.rs::a_usb_retry_stalls_a_note_on_by_its_attach_at_most` (the retry's attach holds a note back by its bound at most).
  *Device-only:* the real cable unplugged on battery and the unit enumerating again on a real host within seconds (real USB host, power).
- [ ] Put the computer to sleep and wake it while the unit plays: no
  dropouts, MIDI works again after the wake.
  *Device-only:* a real host's suspend and resume (real USB hosts and OSes; the emulated host has no suspend).
- [ ] On battery with no cable for five minutes: the keys and screen stay
  responsive and the sound clean (USB is retried once a second).
  *Emulator:* `emu-gate/tests/latency.rs::a_usb_retry_stalls_a_note_on_by_its_attach_at_most` (the USB retry), `emu-gate/tests/usb.rs::a_host_gets_the_identity_and_the_knobs_over_usb` (with no host the main loop sleeps, under 20 % busy).
  *Device-only:* five real minutes on battery: keys, screen and sound over time (real silicon, battery).
- [ ] If you can measure it: supply current with no cable:
  *Device-only:* a current measurement on the unit (real silicon).

Notes:

### 8.1 Before any release: USB-MIDI out (must pass)

v0.2.0-m2-preview received USB-MIDI (notes, the upgrade request) but almost
nothing it sent reached the host. The cause was the build flags of the file that
includes the USB stack. The fix runs Felucca's `usb.c` (with two marked hooks), compiled with
Felucca's flags, with its EP buffers 64-byte aligned
([hardware-results/usb-out-diagnosis.md](hardware-results/usb-out-diagnosis.md)).
fm1-emu checks it against a modelled host
(`host/crates/emu-gate/tests/usb.rs`), which cannot show the chip's own
behaviour. **No release goes out unless every item here passes on a real
FM-1 with that release's package**, each from a cold boot and again after a
USB replug:

- [ ] `fm1_install.py --info` answers with SIDOU's identity (`FM-1_900`
  for development builds). Record its output:
  *Emulator:* `emu-gate/tests/usb.rs::a_host_gets_the_identity_and_the_knobs_over_usb` (the handshake answers "FM-1_900").
  *Device-only:* fm1_install.py on a real host and the 10-out-of-10 handshake record (install tools, real USB host).
- [ ] A reinstall over SIDOU (`fm1_install.py sidou.fwsc`, SIDOU to SIDOU)
  completes: the read requests reach the host, the loader writes, and the
  unit boots the new package (`--info` again).
  *Emulator:* `emu-gate/tests/usb.rs::the_running_firmware_serves_an_updates_whole_step_one` (every read request, the staged loader, the hand-off record), `emu-gate/tests/usb.rs::an_upgrade_request_gets_the_first_read_request_out`, `sidou-pack/tests/ota_step1.rs::a_package_stages_its_loader_and_commits`.
  *Device-only:* Felucca's loader writing the real flash and the unit booting the new package (install tools, real silicon).
- [ ] In a MIDI monitor on the SIDOU input, K1 sends CC 74 on channel 1
  while it turns, and K4 (release in LIVE) CC 72. Record what the
  monitor shows:
  *Emulator:* `emu-gate/tests/usb.rs::a_host_gets_the_identity_and_the_knobs_over_usb` (K1 gives CC 74 on channel 1).
  *Emulator (no test yet):* K4 turned through the panel into the emulated host gives CC 72.
  *Device-only:* what a real monitor shows (the record, real host and OS).
- [ ] The same after deleting the device from the OS's MIDI setup (macOS:
  Audio MIDI Setup) and replugging.
  *Device-only:* macOS's cached device entry in Audio MIDI Setup (real hosts and OSes); the names it shows come from the descriptor strings checked in the first item of section 8.
- [ ] *(Since USB audio out.)* The unit is one USB device with an audio input
  beside its MIDI port (section 19); `--info` and the reinstall above pass with
  it, also while a DAW records from it.
  *Emulator:* `sidou-pack/tests/usb_audio.rs::the_device_is_one_composite_usb_midi_and_class_compliant_audio_input` (one composite device), `emu-gate/tests/usb.rs::a_host_records_the_dacs_output_over_usb_audio_at_44_1_and_48_khz`.
  *Emulator (no test yet):* the update's step 1 (`the_running_firmware_serves_an_updates_whole_step_one`) with the emulated host also reading the audio endpoint (`ep4_reading`).
  *Device-only:* a real DAW holding the input open while the real loader updates the unit.

If an item fails, record it, and get back to a working firmware with the
power-on gesture (section 0) rather than the installer.

Notes:

**Passed on 2026-10-19** with FULL-FIX and FULL-FIX-NC, the fix's builds on the
`usbdbg` branch ([hardware-results/usb-out-diagnosis.md](hardware-results/usb-out-diagnosis.md)):

- `--info` answered `FM-1_900`, and the handshake answered 10 times out of 10.
- Reinstalls over USB worked: FULL-FIX to FULL-FIX-NC, then FULL-FIX-NC to stock
  V15 and back.
- K1 sent CC 74 (326 messages in the monitor).

**Passed again on 2026-10-21** with `USB-FIX-dadffc9`, the merged fix (Felucca's
`usb.c` with its two hooks, the quiet retry, the packed buffers):

- Installed over FULL-FIX, then reinstalled over itself; `--info` answered
  `FM-1_900` both times, and the unit presents as "SIDOU" on USB again (renamed "SIDOU FM-1" afterwards, so installers find it).
- The handshake answered 10 times out of 10; SIDOU's inquiry and the universal
  inquiry answered.
- K1 sent CC 74 on channel 1, and K4 CC 72.
- After a replug, and again after a battery boot with the cable plugged in
  later: the unit enumerated, the handshake answered 10/10, both inquiries
  answered, and K1 sent CC 74 within seconds of the port opening.

Then with `USB-FIX-2964aa5` (the product name "SIDOU FM-1"): after deleting the
device in Audio MIDI Setup and replugging, the port is named "SIDOU FM-1" (before,
macOS kept the cached "SIDOU"), and `fm1_install.py --info` finds the unit without
`--port`. The handshake answered 10/10, both inquiries answered, and K1 sent CC 74
(201 messages).

**v0.2.0's package passed on 2026-10-22** (`sidou-0.2.0.fwsc`, SHA-256 `f1219c72…2d52`):
`fm1_install.py` found the unit without `--port` and updated it from SIDOU over USB;
`--info` answered `FM-1_900` and the port is "SIDOU FM-1"; the handshake answered 10/10 and
both inquiries answered; K1 sent CC 74 on channel 1 (185 messages), and again right after a
replug. Every later release package goes through this section before it ships.

## 8b. MIDI clock and transport (M2e Task 2)

The song is SIDOU's first song (PLAY). The MIDI page is HOME, then MIDI at the end of the
menu; CLOCK is AUTO and CLOCK OUT OFF at power-on.

- [ ] **Follow a DAW over USB:** set the DAW to send MIDI clock and transport to SIDOU at
  120 BPM and press its play: SIDOU's song starts on the DAW's downbeat and stays with it for
  a minute (record the DAW's metronome and SIDOU together: the rows on the clicks, no drift).
  Change the DAW's tempo to 90 and to 140 while it plays: the song follows within a beat or
  two. Stop the DAW: the song stops. Record what you hear at the start:
  *Emulator:* `emu-gate/tests/midi_transport.rs::rows_land_on_a_120_bpm_clock_over_trs` (the start lands on the clock, every row within 1 ms of its 16th), `emu-gate/tests/midi_transport.rs::a_song_follows_a_steady_clock_for_ten_minutes_without_drifting` (120 and 133 BPM, no drift), `sidou-tests/tests/app/midi_clock.rs::a_tempo_step_settles_within_the_window_without_overshoot` (tempo steps on a TRS and a USB jitter model). Stop over USB: `emu-gate/tests/midi_transport.rs::stop_song_position_and_continue_over_usb`.
  *Device-only:* a real DAW's clock and metronome over a real USB cable, heard against SIDOU (real DAW and OS timing, the ear).
- [ ] **The same over TRS:** a hardware sequencer or a DAW's MIDI interface into the TRS
  jack: as above.
  *Emulator:* `emu-gate/tests/midi_transport.rs::rows_land_on_a_120_bpm_clock_over_trs`, `emu-gate/tests/midi_transport.rs::stop_song_position_and_continue_over_trs`, `emu-gate/tests/midi_transport.rs::a_song_follows_a_steady_clock_for_ten_minutes_without_drifting`.
  *Device-only:* a real hardware sequencer or MIDI interface on the TRS jack (external gear).
- [ ] **Song position:** in the DAW, stop, move the play head to bar 3 and press play
  (continue): the song plays from bar 3, in step with the DAW.
  *Emulator:* `emu-gate/tests/midi_transport.rs::stop_song_position_and_continue_over_usb` and `emu-gate/tests/midi_transport.rs::stop_song_position_and_continue_over_trs` (song position 32 16ths, then continue: the first call is row 32), `sidou-tests/tests/app/midi_sync.rs::a_song_position_seeks_and_continue_plays_from_there`.
- [ ] **A clock that stops:** with CLOCK at AUTO, unplug the clock's cable while the song
  plays: the song carries on at the tempo it had. With CLOCK at FOLLOW, the same stops it.
  *Emulator:* `sidou-tests/tests/app/midi_sync.rs::a_clock_that_stops_carries_the_song_on_with_auto_and_stops_it_with_follow` (AUTO goes on at its tempo, FOLLOW stops), `sidou-tests/tests/app/midi_clock.rs::a_clock_that_stops_is_lost_after_300_ms`.
- [ ] **Lead a DAW:** set CLOCK OUT to ON on the MIDI page and set the DAW to follow MIDI clock
  from SIDOU over USB. Press PLAY on SIDOU: the DAW starts and follows the song's tempo;
  PLAY again stops both. Record the tempo the DAW shows:
  *Emulator:* `sidou-tests/tests/app/midi_sync.rs::clock_out_sends_24_a_beat_at_the_songs_tempo_and_the_transport` (start, 24 clocks a beat at the song's tempo, stop; song position and continue), `emu-gate/tests/usb.rs::clock_outs_song_position_waits_for_a_sysex_reply_to_end` (CLOCK OUT on the emulated USB host).
  *Device-only:* a real DAW following SIDOU's clock and the tempo it shows (real DAW).
- [ ] **No echo:** with CLOCK OUT on and the DAW sending clock to SIDOU, the DAW's MIDI
  monitor shows no clock coming back from SIDOU.
  *Emulator:* `sidou-tests/tests/app/midi_sync.rs::nothing_goes_out_while_a_clock_is_followed` (CLOCK OUT on, a clock followed: nothing is sent).
- [ ] **DAW mode routing over USB:** choose MULTI and base channel N on MIDI.
  Check that the panel's keys and DAW channel N+3 play the instrument on chip B;
  OMNI returns the live routing. Check WHOLE B similarly. Record channel/profile
  selection separately from sound. **Chip-A MULTI/WHOLE jam callbacks remain
  unbound in this build:** channels N..N+2 are not yet an audio acceptance pass.
  Do not mark full multitimbral playback passed until that binding is added.
  *Emulator:* `emu-gate/tests/midi_modes.rs::multitimbral_plays_the_instrument_on_chip_b_from_n_plus_3` (MULTI: channel N+3 plays the instrument on chip B, OMNI gives chip A back), `sidou-tests/tests/app/midi_route.rs::whole_chip_b_plays_channel_n_on_the_instrument`, `sidou-tests/tests/app/midi_route.rs::omni_is_the_default_and_every_channel_plays_the_instrument` (these send over TRS or the parser, not through the emulated USB host).
  *Emulator (no test yet):* audio of channels N..N+2 in MULTI and WHOLE A: the jam callbacks are not bound in this build, so there is nothing to check until they are.

Notes:

## 8c. SysEx files, instruments and songs (M2e Task 5)

These are manual acceptance checks, not recorded hardware passes. USB-MIDI output
works since v0.2.0 (section 8.1 passed on 2026-10-21 and on v0.2.0's package), so USB
replies can carry a dump or an upload. The protocol and exact payloads are
in [SysEx protocol, version 1](reference/sysex.md). `sidou-sysex` is a host-only
packet library, not a MIDI transport; its one command writes `.sid` tunes as `.syx` files
(section 20). Use a SysEx utility that supports
per-frame ACK handling for USB and an explicit frame gap for TRS.

Use a writable store (section 7.1), back up wanted songs and choose disposable
filenames. Never format a foreign store just to proceed. Do not use updater
commands for these checks.

- [ ] **Inquiry, USB:** send `F0 7D 53 44 01 00 7F F7`. The INQ reply reports
  protocol 1, the installed firmware version, store format, free bytes, clean
  pool size and write-window flag. Universal inquiry to id 0 and broadcast id
  `7F` returns the documented SIDOU identity. Record the replies; the updater's
  independent identity still works.
  *Emulator:* `sidou-tests/tests/app/sysex.rs::inquiry_is_packetised_and_reports_version` (INQ: protocol, version, store format, free bytes, pool size, window flag; the universal inquiry to 7F; other ids and TRS requests get no reply; the test does not assert id 0 over USB separately), `emu-gate/tests/usb.rs::a_host_gets_the_identity_and_the_knobs_over_usb` (the updater's identity).
  *Device-only:* the replies as a real SysEx utility records them (real host and OS, librarian).
- [ ] **Directory and download, USB:** request DIR, ACK every entry and its
  final END. GET a disposable named file; ACK PUT metadata, every DATA and END.
  Reassemble exactly the declared length and compare CRC-32 and bytes with the
  original. Reserved `MIDI INSTRUMENT` and `MIDI SONG` entries are not listed.
  *Emulator:* `sidou-tests/tests/app/sysex.rs::store_upload_download_directory_and_delete_roundtrip` (DIR, GET with an ACK each, bytes and length compared), `emu-gate/tests/usb.rs::a_silent_usb_put_and_get_move_a_256_byte_packet_quickly` (GET over the emulated USB host returns what PUT stored, CRC in the metadata), `sidou-tests/tests/app/sysex.rs::current_source_crc_prepass_restarts_and_always_releases_hooks` (the reserved MIDI INSTRUMENT and MIDI SONG are not listed and not fetchable by name).
- [ ] **Upload and delete, USB:** PUT a disposable file, waiting for the
  metadata ACK before DATA and each DATA ACK before the next frame. Send
  nonfinal payloads in multiples of 16 bytes, at most 256; allow a short final
  payload. Wait for END's ACK, GET and compare bytes/CRC, then DEL and wait for
  its ACK. DIR no longer lists it. Repeat with a 65,536-byte file.
  *Emulator:* `emu-gate/tests/usb.rs::a_silent_usb_put_and_get_move_a_256_byte_packet_quickly` (4 KiB PUT then GET identical, stop-and-wait ACKs), `sidou-tests/tests/app/sysex.rs::store_upload_download_directory_and_delete_roundtrip` (PUT, GET, DEL, DIR no longer lists it), `sidou-tests/tests/app/sysex.rs::empty_and_maximum_files_never_buffer_a_dump` (0 and 65,536 bytes).
  *Emulator (no test yet):* the 65,536-byte PUT, GET and DEL through the emulated USB host (`usb.rs` moves 4 KiB).
- [ ] **Production-default programs with a chord:** INQ's window flag is zero.
  Upload the large file while holding keys. It is accepted, not refused solely
  because windows are disabled. Listen for the explicit SAVE-style short
  mute/ramp gaps and recovery between programs; the transfer must not leave
  audio held continuously. Record what is heard and any stuck or missed note.
  On 2026-10-23 every PUT with a chord held on LIVE was refused BUSY with INQ
  reporting 22 clean pool sectors: a record the store was writing (a collection
  that quiet maintenance had started) waited for quiet, and the PUT gave up on
  it. Now the PUT (and DEL) waits for that record in its own short holds, then
  claims. Re-check: with the unit silent for a few seconds after an earlier
  64 KiB round trip (maintenance runs then), hold a three-note chord on LIVE and,
  still holding it, run `tools/sysex-roundtrip "SIDOU FM-1" 4096`, then
  `tools/sysex-roundtrip "SIDOU FM-1" 65536`. Both print PUT, GET identical and
  DEL gone, with no NAK; record the PUT seconds (about 263 ms a 256-byte packet
  in fm1-emu since 1.1's 250 ms gap, so about 4.2 s for 4 KiB and 67 s for 64 KiB).
  Repeat each with three notes held by another app (a DAW, or a keyboard routed to
  the FM-1's USB port). With the demo song playing (PLAY) instead, the upload waits
  for quiet (the next item). Over TRS, hold the chord and repeat the TRS upload
  below: RECEIVING then SAVED, never STORE BUSY: RESEND.
  *Emulator:* `emu-gate/tests/usb.rs::a_usb_put_with_keys_held_waits_out_a_record_and_is_accepted` (4 KiB, a chord held, a store record waiting: the PUT is accepted and no erase happens; the same with MIDI notes held), `emu-gate/tests/usb.rs::a_sounding_usb_put_keeps_its_gaps_with_one_hold_a_packet` (one hold a packet, at least 250 ms apart, the chord plays between packets), `emu-gate/tests/sysex.rs::a_production_full_size_trs_put_holds_every_program_but_never_erases_or_stays_muted` (64 KiB over TRS with windows off).
  *Emulator (no test yet):* a 64 KiB PUT through the emulated USB host with a chord held, and the 263 ms a packet bound at that size.
  *Device-only:* listening for the mute and ramp gaps, stuck or missed notes, the PUT seconds on the real flash chip, notes held by another app or a keyboard, and `tools/sysex-roundtrip` on a real host (ear, real flash timing, external gear).
- [ ] **An upload waits for quiet; the gap by ear (1.1 item 13).** Play the demo song
  (PLAY) and send a song from the Songs page: the page says it is waiting for the FM-1 to be
  quiet, the screen says `UPLOAD WAITS FOR QUIET` (again every two seconds), and the song
  plays on with no dip. Stop it: the upload goes on and ends SENT. The same with a DAW
  recording the FM-1's USB audio (input armed and recording) and no song: it waits until the
  recording stops. Leave a song playing for over a minute: the page fails with "waited a
  minute for quiet", nothing replaced. Then the gap: with only a three-note chord held on
  LIVE (no song, nothing recording), send a 64 KiB file
  (`tools/sysex-roundtrip "SIDOU FM-1" 65536`) on each variant package, 100, 250 and 500 ms
  ([BUILDING.md](../BUILDING.md), "The SysEx gap variants"), and write down which one's dips
  are least disturbing for how long the upload takes (about 1.7, 4.2 and 8.2 s for 4 KiB).
  The chosen value becomes the default; reflash the release package afterwards.
  *Emulator:* `emu-gate/tests/usb.rs::a_usb_put_with_a_song_playing_waits_out_a_collection_and_is_accepted` (the song plays: WAIT at once and every two seconds, no hold, past the two-second timeout; the song stops and the PUT and a DEL go on), `emu-gate/tests/usb.rs::a_sounding_usb_put_keeps_its_gaps_with_one_hold_a_packet` (the keys alone: holds 250 ms apart), `sidou-tests/tests/app/sysex.rs::a_loud_device_holds_a_datas_ack_and_says_wait_until_quiet`, `sidou-tests/tests/app/sysex.rs::a_wait_for_quiet_gives_up_with_busy_after_its_limit` (the minute's limit), `web/sysex.test.mjs` (the page keeps waiting on WAIT, and does not retry the BUSY that ends one).
  *Emulator (no test yet):* the wait while a computer records the USB audio (`midi_usb_recording`; the song's case runs the same code).
  *Device-only:* the ear call between the three gaps, the song playing on without a dip on the real DAC, a real DAW's recording (ear, external gear).
- [ ] **Refusal before streaming:** if the clean pool cannot cover a file,
  metadata receives BUSY with pool size (or STORE BUSY: RESEND on TRS), before
  accepting DATA. Stop the transfer and allow quiet refill before retrying.
  A file beyond 65,536 bytes receives FULL; it does not trigger a mid-transfer
  erase. Do not erase wanted files to manufacture this condition.
  *Emulator:* `emu-gate/tests/sysex.rs::a_pool_drained_put_is_refused_before_any_program_or_claim` (STORE BUSY: RESEND over TRS, nothing written), `sidou-tests/tests/app/sysex.rs::drained_pool_and_flash_failure_release_only_owned_claims` (BUSY before DATA, claims released), `sidou-tests/tests/app/sysex.rs::malformed_headers_status_truncation_and_names` (65,537 bytes gets FULL).
- [ ] **Abort and damaged uploads:** overwrite a disposable file, then test
  ABORT, a bad packet checksum, a wrong DATA sequence, a nonfinal 15-byte DATA,
  a wrong final CRC and a pause over two seconds between frames. Each upload
  ends with the documented NAK/toast, leaves the old file intact and allows a
  fresh transfer. Real-time MIDI clock bytes between SysEx bytes do not corrupt
  framing. While USB owns a transfer, TRS cannot continue or interrupt it.
  *Emulator:* `sidou-tests/tests/app/sysex.rs::sequence_checksum_canonical_and_length_errors_preserve_existing_file` (bad checksum, wrong DATA sequence, short nonfinal DATA, wrong CRC: the old file stays), `sidou-tests/tests/app/sysex.rs::per_port_ownership_realtime_abort_lost_and_timeout` (ABORT, the 2 s pause, real-time bytes inside SysEx, the other port cannot interrupt), `emu-gate/tests/usb.rs::a_ring_gap_aborts_only_its_ports_sysex_and_never_cuts_a_reply`.
- [ ] **Instrument:** stop the song and release all audition notes. GET an instrument as
  GTI5, then PUT it into a disposable instrument slot. Its four tables are
  relocated and it sounds as before. Invalid content and insufficient trailing
  table rows refuse without changing the song. Interior holes are not reused;
  valid GTI5 content can still receive FULL. `MIDI INSTRUMENT` is not offered as
  a song by the song browser or bank-1 recall.
  *Emulator:* `emu-gate/tests/sysex.rs::a_trs_instrument_put_relocates_tables_into_the_current_song` (tables relocated, the song intact), `sidou-tests/tests/app/gt2_ins.rs::malformed_truncated_and_full_files_preserve_every_song_byte` (invalid content refused, FULL), `sidou-tests/tests/app/song_io.rs::internal_midi_staging_songs_do_not_take_a_bank_program_number` (MIDI INSTRUMENT is not a bank-1 song).
  *Emulator (no test yet):* the imported instrument sounds as the exported one (audio compared), and the song browser not offering MIDI INSTRUMENT.
- [ ] **Current song:** GET the current song as GTS5 with SIDOU's trailer and
  open it in a compatible editor. With no unsaved edits, PUT it to target 2;
   wait for END's ACK: ACK/SAVED follows completed, stopped loading. Invalid
   song content produces a refusal and preserves the old model. Unsaved edits or a busy
   save are refused rather than silently discarded.
   Before SAVE AS, repeat a current-song PUT: BUSY / SAVE AS FIRST must leave the
   old model and cache untouched. SAVE AS to a nonreserved filename, then retry.
  *Emulator:* `emu-gate/tests/sysex.rs::a_current_song_put_finishes_the_normal_loader_before_releasing_the_transfer` (PUT to target 2 loads stopped; a second PUT with unsaved edits is refused SAVE AS FIRST and keeps the old song), `emu-gate/tests/song_bank.rs::bank_one_refuses_unsaved_row_edits_without_discarding_them`, `sidou-tests/tests/app/sysex.rs::current_source_crc_prepass_restarts_and_always_releases_hooks` (GET of the current song).
  *Device-only:* opening the downloaded GTS5 in a compatible editor (GoatTracker on a computer, external gear).
- [ ] **TRS upload:** send PUT, DATA and END with **one second after EACH
   complete frame**, including metadata and file END, using at most 256 decoded
  bytes per DATA and the alignment rule above. Check RECEIVING then SAVED or
  the refusal toast; there is no TRS reply and no USB reply to a TRS request.
  After completion, load the song or, once USB output passes, GET and compare
   the file. This conservative gap is not a measured hardware throughput claim.
   For instrument/current-song END, allow the bounded importer/loader up to
   15 seconds, or wait for its SAVED/refusal toast, before another transfer.
  *Emulator:* `emu-gate/tests/sysex.rs::a_trs_put_commits_a_song_byte_exact_and_bank_one_loads_it` (PUT, DATA, END commit the file, then it loads and PLAY plays it), `sidou-tests/tests/app/sysex.rs::trs_upload_is_silent_and_early_packet_aborts` (no reply to a TRS request; RECEIVING toast in `sysex.rs`'s hooks), `emu-gate/tests/sysex.rs::a_production_trs_put_with_keys_held_waits_out_a_record_and_commits`.
  *Device-only:* a real MIDI interface sending with the one second gap, and the toasts on the real LCD (external gear, a real SysEx sender).
- [ ] **Hold-free programs, only after section 12 validates windows:** repeat
  the large upload with a chord on a build using validated timing values.
  INQ reports windows enabled; no program hold, dropout or click is heard.
  Emulator window-override results do not satisfy this hardware item.
  *Device-only:* this needs section 12's validated flash timings and listening for a hold, dropout or click on the real unit (real silicon, flash chip timing, speaker). `emu-gate/tests/sysex.rs::a_full_size_trs_put_with_a_chord_never_erases_or_holds` runs the window override, which this item says does not count.

Record the package hash, port, payload size, frame gap, elapsed time, observed
toasts and any failure. Keep sections 8.1 and 12 open until separately passed.

Notes:

## 9. Reinstall, back to stock V15, and SIDOU again

- [ ] From SIDOU, install `sidou.fwsc` again (SIDOU to SIDOU): it installs,
  and `--info` reports `FM-1_900`.
  *Emulator:* `emu-gate/tests/usb.rs::the_running_firmware_serves_an_updates_whole_step_one`, `sidou-pack/tests/ota_step1.rs::a_package_stages_its_loader_and_commits` (the running firmware's step 1 against this package).
  *Device-only:* the real installer and Felucca's loader writing the real flash, then `--info` reporting FM-1_900 (install tools, real silicon).
- [ ] From SIDOU, `fm1_install.py FM-1.fwsc` (the unmodified official V15)
  installs: SIDOU's step 1 accepts it (its flash head is the unit's), the
  official loader writes it, and `--info` reports `FM-1_015`.
  *Emulator:* `sidou-pack/tests/ota_step1.rs::the_way_back_to_stock_v15_stays_open` (step 1 accepts V15's package when the cached V15 package is present).
  *Device-only:* the official loader writing V15 and `--info` reporting FM-1_015 (stock V15 round trip, install tools, real silicon).
- [ ] The stock firmware plays its patches as before.
  *Device-only:* the stock V15 firmware running on the unit, heard through the speaker (stock firmware, nothing of SIDOU to emulate).
- [ ] Installing `sidou.fwsc` again from stock works (section 1).
  *Emulator:* `sidou-pack/tests/ota_step1.rs::a_device_with_the_stock_head_takes_our_package` (a device with V15's head takes our package).
  *Device-only:* the real install from stock V15 on the unit (install tools, real silicon).

Notes:

## 10. Boot guard, watchdog and resets

**Run this section only after a recovery from UBOOT has been shown to work
on this unit** (with a tool from section 0), since its expected end is
UBOOT.

It needs a checklist build that dies on purpose, one second after boot,
before the boot counts as stable (10 s). Build it as BUILDING.md says, with
`-DSIDOU_TEST_DEATH=hang` (interrupts off: the watchdog resets the chip
after about 8 s) or `-DSIDOU_TEST_DEATH=reboot` (a P33 reset, as the fault
handler does), and package it as usual. It dies **only when OCT- (button 0)
is held at power-on**, and only on that boot: started without the key it is
the normal SIDOU, so the normal installer always reaches it. These builds
leave out the power-on UBOOT gesture (section 0), whose key is the same: their
OCT- at power-on dies instead. Never give these builds to anyone else, and
install the normal package again when you are done.

- [ ] Without OCT-: the checklist build boots and plays as normal, and stays
  up past 10 s.
  *Emulator:* `emu-gate/tests/bootguard.rs::a_stable_boot_clears_an_earlier_death` (a boot that runs past the stable time is stable), `emu-gate/tests/bootguard.rs::the_running_firmware_feeds_the_watchdog` (24 s, no reset); the SIDOU_TEST_DEATH build without OCT- is not run in the emulator.
  *Device-only:* a real boot of the checklist build staying up past 10 s (real silicon).
- [ ] Hang build: hold OCT- while powering on and keep it held through the
  resets. Boot 1 dies, the watchdog resets, boot 2 dies, and boot 3 enters
  UBOOT instead of SIDOU. Record what the screen shows and how the unit
  appears on USB in UBOOT. This shows the guard state in `.noinit` survives
  a watchdog reset.
  *Emulator:* `emu-gate/tests/bootguard.rs::a_hang_resets_through_the_watchdog_and_the_third_boot_enters_uboot` (the hang build, with `SIDOU_FM1_DEATH_BUILD`: two watchdog resets 9 s apart, the guard kept in RAM, the third boot asks for UBOOT), `emu-gate/tests/bootguard.rs::two_early_deaths_end_in_uboot`.
  *Device-only:* the real watchdog's timing on silicon, `.noinit` surviving a real reset, and what the screen and USB show in UBOOT (real silicon, mask ROM).
- [ ] In UBOOT, the unit stays there (no watchdog reset) for at least five
  minutes.
  *Device-only:* the mask ROM's UBOOT staying up for five minutes with no watchdog reset (real silicon).
- [ ] From UBOOT, install the normal `sidou.fwsc` with a recovery tool from
  section 0, and record which one worked and how:
  *Device-only:* a recovery tool from section 0 installing from UBOOT, and which one worked (install and recovery tools, ROM boot mode).
- [ ] Reboot build: the same, with the guard surviving a P33 reset.
  *Emulator:* `emu-gate/tests/bootguard.rs::two_early_deaths_end_in_uboot` (two crashes, each reset through P33 by the fault handler, the guard kept in RAM; the third boot asks for UBOOT and starts no audio).
  *Device-only:* the real P33 reset keeping the guard (real silicon).
- [ ] One death, then a normal boot (OCT- released) that runs past 10 s,
  then one more death: no UBOOT (the stable boot cleared the count).
  *Emulator:* `emu-gate/tests/bootguard.rs::a_stable_boot_clears_an_earlier_death` (after one death a stable boot clears the count and no UBOOT follows), `sidou-tests/tests/app/bootguard.rs::a_stable_boot_clears_the_count`.
- [ ] Normal build: hold OCT- alone while powering on, about a second: the
  ROM boot mode, as the guard's (section 1 has the full check). Record
  whether the guard's UBOOT and this one look the same on USB:
  *Emulator:* `emu-gate/tests/bootguard.rs::oct_down_alone_held_a_second_at_power_on_enters_uboot` (OCT- alone for a second asks for UBOOT, backlight dark, no audio), `emu-gate/tests/bootguard.rs::oct_down_let_go_within_the_second_boots_on`, `emu-gate/tests/bootguard.rs::a_second_key_pressed_during_the_hold_cancels_it`.
  *Device-only:* whether the real ROM boot mode looks the same on USB as the guard's UBOOT (real silicon, ROM boot mode).
- [ ] Normal build: power-cycle quickly (off for under a second) three times
  within ten seconds of each boot. Record whether it ends in UBOOT (it
  should not, if RAM does not survive a power cut, but a brownout may keep
  it):
  *Device-only:* whether RAM survives a real power cut and a brown-out leaves the guard set (real silicon, power). `sidou-tests/tests/app/bootguard.rs::power_on_noise_starts_a_fresh_guard` checks only the logic.

Notes:

## 11. Measurements for later work

Record what you can. Counter/CRC microbenchmarks need a measuring build;
the store's listening checks use normal SIDOU, and section 12's flash spike is built.

- [ ] A photo of the chip's part number (docs/hardware.md expects AC7911B8).
  *Device-only:* a photograph of the real chip (real silicon).
- [ ] The real CPU clock (360 MHz expected after the SPL; the budget assumes
  240).
  *Device-only:* the unit's real clock after the SPL (real silicon).
- [ ] Cycles per audio block against fm1-emu's instruction count, and the
  gain from running the audio path from SRAM (docs/hardware.md, open
  checks).
  *Device-only:* real cycle counts on the chip; fm1-emu counts instructions only (real silicon).
- [ ] Cycles per audio block with combined waveforms on both chips against
  single waveforms: the combined waveforms' pulldown rows are the audio
  interrupt's only flash data reads (docs/hardware.md, open checks).
  *Device-only:* real cycle counts of the flash reads on the chip (real silicon).
- [ ] USB poll cost with a host attached (three SIE round trips per poll at
  2 kHz).
  *Device-only:* the real SIE round trips at 2 kHz; the emulated host counts accesses only (real silicon, real USB host).
- [ ] Divide and IRQ-entry latencies.
  *Device-only:* real latencies on the chip (real silicon).
- [ ] The voice DAC tables the device builds at boot match the host's. The
  core computes them in float (`sid_dac_build`), and the device's float
  division may round differently from the host and from fm1-emu's soft FPU,
  which the emulator tests compare against. A measuring build computes the
  CRC-32 of `sid_dac_default(0)` (8580) and `sid_dac_default(1)` (6581), each
  over the struct's bytes as it lies in memory (`wave`, then `env`), and
  shows them on the status screen (or, later, answers them over SysEx).
  Expected, from `cargo test --release -p sidou-tests --test unit --
  the_default_tables_crc --nocapture` in `host/`: 8580 `0xfde1a3ba`, 6581
  `0x748fe138`. Record the device's values:
  *Device-only:* the real FPU's rounding; fm1-emu's soft FPU is what the emulator tests compare (real silicon). The expected CRCs come from `the_default_tables_crc` on the host.
- [ ] The 6581's filter on the device (M1b Task 6). Each 6581 sample does one
  FPU divide in the audio interrupt (the band-pass's damping, `sid_nl_tick`),
  which fm1-emu counts as one instruction. A measuring build renders the
  golden `workload_6581_1chip` (`bench/workload.c`, one chip, the 6581) on
  the device and compares its FNV-1a 64 hash with `tests/golden/hashes.txt`:
  a match shows the divide (and the rest of the float path) rounds as IEEE
  single precision does on the host. It also measures the cycles of the
  6581's render loop against the 8580's on the same workload, which shows the
  divide's real cost (in fm1-emu the workload's triad costs 400.50
  instructions a sample on the 6581 and 347.37 on the 8580). Record the hash
  and the cycles:
  *Device-only:* the real FPU's divide and the real cycle cost of the 6581 loop (real silicon); `emu-gate/tests/sid_budget.rs::one_chip_fits_budget_and_matches_host` matches the host in fm1-emu's soft FPU only.
- [ ] Core 1 is not used by SIDOU; its RAM self-test (Felucca's) is for a
  later milestone.
  *Device-only:* a note on the real second core, with no check yet (real silicon, a later milestone).
- [ ] The store's write windows (M2a Task 4, once Task 3b's numbers let
  programs go in windows; docs/hardware.md, "How the store writes"): hold a
  chord and save settings (Task 5's SAVE) several times; listen for a click,
  a dropout or a missed block. Then play a TRS MIDI controller at full rate
  (a fast arpeggio) while saving: no note is lost or stuck.
  *Emulator:* `emu-gate/tests/store.rs::programs_in_windows_start_right_after_a_block`, `emu-gate/tests/latency.rs::a_note_on_keeps_its_bounds_while_a_file_is_saved`, `emu-gate/tests/store.rs::a_file_claims_before_it_writes_and_lands_while_a_chord_plays`.
  *Emulator (no test yet):* a full-rate TRS stream (a fast arpeggio) during a SAVE: no note lost or stuck (`emu-gate/tests/midi.rs::a_full_speed_stream_loses_nothing` runs without a save).
  *Device-only:* hearing a click, dropout or missed block on the speaker, and a real MIDI controller (speaker, external gear).
- [ ] MASTER controls a song's level (M2 hardware fix): play a song (PLAY, the demo
  song or any other) with MASTER fully up, then turn MASTER down slowly to zero and
  back: the song's level follows the pot smoothly, with no zipper or click, down to
  silence at the bottom. Start a song with MASTER low and raise it. Press keys over a
  playing song (KEYS AUTO plays chip B): they follow MASTER too, and a UI sound (the
  menu's) is quiet when MASTER is. With MASTER full the song is as loud as before.
  *Emulator:* `emu-gate/tests/song.rs::master_scales_a_playing_song_and_full_master_leaves_it_alone` (the song's level follows the pot, full MASTER unchanged), `sidou-tests/tests/app/master_gain.rs::a_change_ramps_linearly_over_the_block` (no zipper), `sidou-tests/tests/app/master_gain.rs::live_play_is_the_chips_output_scaled_by_the_taper` (keys over a song follow MASTER).
  *Device-only:* the real pot's smoothness under the hand and how the fade sounds (real encoder or pot hardware, speaker).
- [ ] The tail goes silent (M2 hardware fix): play a chord on a saw or pulse
  preset with the character at 0 (BLEED 0), release it and wait. Headphones
  or a quiet amp, volume up: about 50 ms after the release ends there is no
  faint tone and no hiss left over from the voices, and no click when it
  ends or when the next note starts. Then set BLEED up (CHIP view, K4) and
  repeat: the leak stays, as it should. Play two notes less than 40 ms
  apart (a fast trill): nothing drops out or changes.
  *Emulator:* `sidou-tests/tests/app/tail_gate.rs::the_tail_after_notes_reaches_exact_silence`, `sidou-tests/tests/app/tail_gate.rs::the_leak_fades_out_without_a_click`, `sidou-tests/tests/app/tail_gate.rs::a_chip_with_bleed_keeps_its_leak` (notes less than the hold apart play untouched: `notes_within_a_phrase_play_untouched`).
  *Device-only:* faint tone, hiss or hum at the headphone and quiet-amp level with the volume up (analog output).
- [ ] A hold's gap: release a chord and wait for its tail to fade; a pool
  refill (after a few saves) may follow 500 ms after the input stops. Listen
  for each held erase (a fade out and back, up to 35 ms): note whether it is
  heard, and whether a DAW's MIDI clock into the TRS jack keeps the refill
  from running (it must not).
  *Emulator:* `emu-gate/tests/store.rs::no_erase_while_it_plays` (the erase waits for quiet, a DAW's clock and active sensing do not block it, the output fades to silence around it).
  *Device-only:* whether the held erase is heard (speaker).
- [ ] The first boot after an install on a unit whose store region is blank:
  the store formats its first 22 sectors at boot, up to 0.5 s more than a
  later boot. Note the boot's length against the next one's.
  *Emulator:* `emu-gate/tests/usb.rs::a_first_boot_over_another_firmwares_sectors_answers_usb_at_once` (the boot's flash work is bounded), `emu-gate/tests/settings.rs::the_full_boot_plays_once_after_an_install_then_the_short_one`.
  *Device-only:* the real boot's length against the next one (real flash erase timing on the chip).

Notes:

## 12. Flash timing spike (M2a Task 3b)

The store (M2a) will write the flash while SIDOU plays, and the size of its
writes depends on how long this unit's flash part takes to program and
erase. The emulator can't tell; this section measures it with a test
firmware, the flash spike. While the spike is installed the unit is not
SIDOU: it makes no sound and shows only the spike's screen. You put SIDOU
(or stock) back at the end with the normal installer.

What the spike writes, and what it never touches:
- It erases and programs only the store's last two sectors,
  `0xDE000..0xDFFFF`, and in each only bytes below `0xF00`. It never
  writes the flash head, the app area or the update staging area
  (`0xE0000..0xE4FFF`).
- At boot it only reads: both sectors, and the whole store range
  `0x97000..0xDFFFF`. If either sector holds anything that is neither blank
  nor the spike's own, or the pattern it left in `0xDE000` has changed, it
  shows **REFUSED** and writes nothing.
- It starts measuring only when you press SAVE. Until then, and once it is
  done, it answers the normal installer just as SIDOU does.
- At the end it erases `0xDF000` back to blank and leaves a test pattern in
  `0xDE000`, for the stock round trip (12.4). The pattern is harmless:
  SIDOU's store treats that sector as unused and erases it when it needs it.

Like SIDOU, the spike's boot erases a staging area that an interrupted
install left in use (section 3). That is the update protocol's own cleanup,
not the spike.

### 12.0 Before you start

- [ ] Section 0 is done: M-VAVE's `FM-1.fwsc` (V15), FM-1-transporter,
  MvaveFM1Unbricker with jl-uboot-tool, and Felucca's `fm1_install.py`
  are at hand, and `fm1_install.py --info` finds the unit.
  *Device-only:* the recovery tools and `fm1_install.py --info` finding the real unit (install and recovery tools).
- [ ] Sections 1 and 9 have passed on this unit at least once: SIDOU
  installs, and the way back to V15 works.
  *Device-only:* a record of earlier device runs on this unit (install tools, stock V15 round trip).
- [ ] For the TRS check (optional): a MIDI sender on the TRS jack that never
  pauses, wired as in section 7. One way is a computer sending a large
  SysEx file in a loop through a USB-MIDI interface, or a sequencer
  streaming dense CCs. Full rate is 3,125 bytes a second; the spike shows
  how many arrived. Without a sender, the TRS rows read 0 and that check is
  skipped.
  *Device-only:* a sender on the real TRS jack (external gear).
- [ ] A phone or camera, to photograph the result screen.
  *Device-only:* photographing the real LCD (readability on the real LCD).

Notes:

### 12.1 Build the spike's package

From the repository root (BUILDING.md, "The flash timing spike"):

```sh
tools/fm1-docker cmake --preset fm1 -B build/fm1-spike -DSIDOU_FLASH_SPIKE=ON
tools/fm1-docker cmake --build build/fm1-spike
```

Then from `host/`:

```sh
cargo run --release -p sidou-pack -- \
    --app ../build/fm1-spike/platform/fm1/sidou_app.bin \
    --loader ../build/fm1-spike/platform/fm1/loader/loader.bin \
    -o ../build/fm1-spike/sidou-spike.fwsc
```

`sidou-pack` says "NOT A RELEASE BUILD" for it, and `sidou-pack --verify` and
`tools/preflash` fail it (SIDOU_FLASH_SPIKE=ON): that is by design, the spike is never
released; the installer does not check it.

Also build the normal `build/fm1/sidou.fwsc` (section 1), for the way back.

- [ ] SIDOU commit, and the SHA-256 that `sidou-pack` prints for
  `sidou-spike.fwsc`:
  *Device-only:* a record of the build installed (install tools); `sidou-pack/tests/pack.rs::verify_refuses_a_package_of_the_flash_spike` checks that a spike package is refused as a release.

### 12.2 Install it and read the start screen

Install it as in section 1, from SIDOU or from stock:
`fm1_install.py build/fm1-spike/sidou-spike.fwsc`. The unit restarts into
the spike.

- [ ] The screen reads "SIDOU FLASH SPIKE M2A TASK 3B". It shows the part
  (JEDEC, SR, CR, the SFDP lines and CLOCK), "START DE: ... DF: ...",
  "PATTERN AT START", "STORE SECTORS USED n/73", and "SAVE: START". There is
  no sound. Record:
  *Emulator:* `emu-gate/tests/flash_spike.rs::the_spike_runs_to_its_result_screen_and_writes_only_its_two_sectors` (the title line, the ERASE 4K row and the DONE line; runs when SIDOU_FM1_SPIKE_BUILD is set; no sound is not asserted).
  *Device-only:* every value read off the real flash chip and its clock, as the records below say (real silicon).
  - JEDEC (expected `856014`), SR, CR:
    *Device-only:* the real part's JEDEC id, SR and CR (flash chip, real silicon).
  - SFDP and ERASE (the part's own table: the signature `50444653`, and the
    erase types, where `81` with size `08` would be the 256-byte page erase):
    *Device-only:* the real part's SFDP table and erase types (flash chip, real silicon).
  - CLOCK, in adds/µs (192 at 240 MHz, 288 at 360 MHz):
    *Device-only:* the real CPU clock in adds per microsecond (real silicon).
  - START: both BLANK on a first run, `DE:SPIKE'S` after one:
    *Device-only:* what the real flash holds in the spike's two sectors (flash chip contents on this unit).
  - STORE SECTORS USED (how much of the store range holds anything: Felucca
    or stock data, or nothing):
    *Device-only:* what Felucca or stock left in the real store range (flash chip contents on this unit).
- [ ] If the screen says **REFUSED**: photograph it and go to 12.5. Nothing
  was written.
  *Emulator:* `emu-gate/tests/flash_spike.rs::the_spike_refuses_a_sector_that_is_not_its_own_and_writes_nothing` (a foreign byte gives REFUSED and no write).
  *Device-only:* a photograph of the real LCD.

### 12.3 Run the measurements

Start the TRS sender, then press **SAVE**. The screen shows "RUNNING, STEP
n" for steps 0 to 6 and fills in the results; it takes under a minute.
**Do not power the unit off while it runs.** A cut there can leave the
spike's own sectors half-erased: the next boot shows REFUSED. Nothing else
is at risk, and SIDOU installs as usual.

- [ ] The screen ends with "DONE. DF000 ERASED, DE000 KEEPS THE PATTERN".
  Photograph it, and copy the numbers into
  [hardware.md](hardware.md)'s table ("The flash part's timings").
  *Emulator:* `emu-gate/tests/flash_spike.rs::the_spike_runs_to_its_result_screen_and_writes_only_its_two_sectors` (the run reaches DONE, writes only its two sectors, loses no TRS byte; the bounds are `sidou-pack/tests/flash_spike_guard.rs`).
  *Device-only:* the real program and erase timings of this flash chip, copied into hardware.md (flash chip timing on real silicon).
- [ ] If it ends with "FAILED AT STEP n RC m": photograph it and go to
  12.5.
  *Device-only:* a real flash chip failing a step (real silicon; the emulator's flash is instant and cannot fail this way).

How to read the screen:
- **TIMES US**: min, mean and max microseconds. `ERASE 4K` is a 4 KiB
  sector erase (20 runs). `PROG 16`, `PROG 64` and `PROG 256` are page
  programs of that many bytes (20, 20 and 280 runs). `PAGE ER` is the
  256-byte page erase, `SUSPEND` the erase-suspend latency. For comparison,
  the datasheet's maximums are 20 ms for an erase, 3 ms for a page program
  and 30 µs for a suspend.
- **PAGE ERASE 81**: `THE PAGE` (it erased that page and nothing else),
  `WIDER` (more than the page: the CR's DP bit makes pages 512 bytes),
  `IGNORED` (nothing changed), or `ODD` (anything else).
- **SUS1 n/5**: how many of the 5 suspended erases set the suspend bit.
  `READ` is whether `0xDE000` read correctly while suspended; `END` is
  whether each erase finished after the resume.
- **VERIFY**: every timed program read back. **PARTIAL**: 64 programs of
  4 bytes into one page (several programs per page). **REZERO**: zeros
  programmed over zeros, 3 times. **BITWALK**: one byte programmed 8
  times, clearing one more bit each time. **TWICE**: pages programmed twice
  with different data; anything but OK means the part does not just AND
  what it is given, which suggests ECC. **GUARD**: requests the spike's
  own bounds turned down (must be 0).
- **TRS**: bytes received and lost while the interrupts run (1 s), while
  they are masked for 1 ms (100 times), and for 35 ms (10 times). LOST
  must be 0 on the 35 ms row: the store's longest hold is 35 ms.

Notes:

### 12.4 The stock round trip (the store's range under V15)

This checks whether the stock firmware, or its SPL, writes into the store's
range, as the package's VM region allows (docs/hardware.md, "The flash
part's timings" and the M2a plan's flash map). Start right after a DONE,
with the pattern in `0xDE000`.

1. Install M-VAVE's V15: `fm1_install.py FM-1.fwsc` (as in section 9).
2. Use the stock firmware for a few minutes: play, and change and save a
   setting or a patch if it offers one. Power it off and on once.
3. Install the spike again: `fm1_install.py
   build/fm1-spike/sidou-spike.fwsc` (as in section 1).

- [ ] On the start screen, record "PATTERN AT START" (INTACT: the stock
  firmware left the sector alone; DAMAGED or ABSENT: something wrote or
  erased it), "START DE: ... DF: ...", and "STORE SECTORS USED", against the
  values before the round trip:
  *Emulator:* `sidou-pack/tests/flash_spike_guard.rs::the_round_trip_pattern_is_absent_intact_or_damaged` (how the spike classifies the pattern).
  *Device-only:* whether the real stock V15 left the pattern alone after the round trip (stock V15 round trip, real flash).
- [ ] If it shows REFUSED, something wrote the spike's sectors (PATTERN AT
  START DAMAGED, or a sector FOREIGN). That is the result: record it. The
  spike writes nothing then, so the evidence stays.
  *Device-only:* evidence that stock V15 or its SPL wrote the real spike sectors (stock V15 round trip, real flash). The refusal itself is covered by `emu-gate/tests/flash_spike.rs::the_spike_refuses_a_sector_that_is_not_its_own_and_writes_nothing`.
- [ ] Optional: press SAVE to measure once more and compare.
  *Device-only:* a second measurement on the real flash chip (real silicon).

Notes:

### 12.5 Back to SIDOU or to stock

The spike answers the installer whenever it is not RUNNING: on the start
screen, after DONE, REFUSED or FAILED. The screen shows SIDOU's UPDATE box
during the install.

- [ ] Back to SIDOU: `fm1_install.py build/fm1/sidou.fwsc`. `--info`
  reports `FM-1_900`, and SIDOU boots and plays (section 3).
  *Device-only:* the real installer from the spike back to SIDOU, `--info` FM-1_900, and SIDOU booting (install tools, real silicon).
- [ ] Or back to stock: `fm1_install.py FM-1.fwsc`; `--info` reports
  `FM-1_015`.
  *Device-only:* the real installer from the spike to stock V15, `--info` FM-1_015 (install tools, stock V15 round trip).
- [ ] If the spike's screen never appears, or the unit doesn't answer the
  installer: three boots in a row that die within 10 s end in UBOOT
  (section 10), from which a tool from section 0 installs `sidou.fwsc` or
  V15. Record what happened and which tool worked.
  *Device-only:* three real early deaths ending in UBOOT and a recovery tool from section 0 (recovery tools, ROM boot mode).

Notes:

## 13. Songs on the store (M2c Task 8)

These checks are for the user to run on hardware; automated testing uses fm1-emu only.

- [ ] Open SONG, choose NEW, and confirm discarding changes if asked. Check the three
  64-row patterns and the pulse lead, bass and noise drum instruments.
  *Emulator:* `emu-gate/tests/song_store.rs::new_template_save_as_and_delete_keep_the_right_identity` (three patterns, a pulse lead, a different bass, the drum kit from instrument 3), `sidou-tests/tests/app/views.rs::new_requires_confirmation_and_home_keeps_the_unsaved_song`, `emu-gate/tests/first_use.rs::every_template_instrument_auditions_on_step_heard_at_its_pitch_and_each_its_own`.
- [ ] Choose SAVE AS, enter a 16-character name with SELECT and ALGORITHM, and save.
  Wait for SONG SAVED before switching the power off.
  *Emulator:* `emu-gate/tests/first_use.rs::save_as_and_a_power_cycle_bring_back_the_song_and_a_setting` (the name editor with SELECT and ALGORITHM), `sidou-tests/tests/app/views.rs::save_routes_named_songs_to_io_and_save_as_accepts_all_sixteen_characters`, `emu-gate/tests/store.rs::a_cut_after_any_operation_leaves_the_old_value_or_the_new` (a power cut at any point leaves the old value or the new).
- [ ] Power on again: the last song loads stopped. PLAY sounds the saved song.
  *Emulator:* `emu-gate/tests/song_store.rs::saved_song_survives_reboot_stopped_and_plays_bit_exact` (loads stopped, plays bit exact), `emu-gate/tests/first_use.rs::save_as_and_a_power_cycle_bring_back_the_song_and_a_setting`.
- [ ] SAVE an unchanged song: SONG SAVED appears without a flash-write delay.
  *Emulator:* `emu-gate/tests/song_store.rs::saved_song_survives_reboot_stopped_and_plays_bit_exact` (an unchanged SAVE rewrites nothing), `sidou-tests/tests/app/song_io.rs::an_unchanged_save_writes_again_once_its_file_was_deleted_or_replaced` (SONG SAVED toast).
- [ ] During a save, edits are refused with SAVING; playback, mute, solo and jam continue.
  *Emulator:* `emu-gate/tests/song_store.rs::save_freezes_edits_and_keeps_the_snapshot_at_save` (SAVING toast, the song unchanged), `emu-gate/tests/song_store.rs::saving_during_playback_forces_progress_even_with_a_settings_save` (playback goes on).
  *Emulator (no test yet):* mute, solo and a jam note while a save is streaming.
- [ ] Near the saved-size limit, an edit that would exceed 65,536 bytes is refused
  with SONG FULL and leaves the song unchanged.
  *Emulator:* `emu-gate/tests/screen.rs::tracker_refuses_growth_at_the_real_song_size_cap` (SONG FULL at the exact 65,536-byte cap, the song unchanged), `sidou-tests/tests/song_capacity.rs::hole_growth_is_refused_before_the_song_becomes_unsaveable`.
- [ ] With a clean, stopped song, open ABOUT: its tune plays. HOME stops it and
  restores the loaded song, stopped. With unsaved changes, ABOUT stays silent and
  its scroller says SAVE YOUR SONG FOR MUSIC.
  *Emulator:* `emu-gate/tests/song_store.rs::about_music_restores_the_saved_song_and_leaves_unsaved_edits_silent`, `sidou-tests/tests/app/about.rs::clean_about_requests_its_tune_once_and_close_requests_restoration`, `sidou-tests/tests/app/about.rs::unsaved_song_keeps_about_silent_and_the_silent_scroller_stays_in_budget`.
- [ ] DELETE asks for confirmation. Cancelling keeps the file; confirming removes it.
  *Emulator:* `sidou-tests/tests/app/views.rs::load_directory_pages_and_delete_waits_for_yes` (the default NO deletes nothing, YES starts the delete), `emu-gate/tests/song_store.rs::new_template_save_as_and_delete_keep_the_right_identity` (the file is gone, the loaded song stays).

## 14. Make a song on the device (M2d)

Do this after the button-id check in section 5 (GLO, SEQ, PLAY, REC) has passed. It is a whole
song made on the unit: keys, knobs, recording, mute, save, reload. Start with SONG, NEW (a
silent three-pattern song at tempo 6, so what you hear is what you made). Controls:
[controls.md, song views](reference/controls.md#song-views).

- [ ] **STEP.** SEQ (STEP shows). Tap steps 1, 5, 9, 13 on V1: a ball on each, the key's note
  sounds as it goes down. PLAY: the white step runs across the lane, the notes sound in time.
  SELECT to V2, tap a bass line; to V3, a noise drum. ALGORITHM pages (the page follows the
  playing step). ARP switches the lane to LEGATO and back.
  *Emulator:* `emu-gate/tests/first_use.rs::every_template_instrument_on_steps_plays_heard_and_on_time` (steps tapped on V1 play heard and on time, a step key auditions), `sidou-tests/tests/app/step_view.rs::while_the_song_plays_the_page_and_the_white_step_follow_it`, `sidou-tests/tests/app/step_view.rs::a_legato_lane_writes_no_key_off_and_taking_a_note_away_leaves_the_one_before` (the legato lane).
- [ ] **A lock.** On a playing STEP, hold a step and turn K1: only that step's row has a
  different cutoff (yellow notch, `LOCK`); K4 on a held step sets a waveform (`SET`, cyan). LFO
  with the step held clears both. Save and reload (below): the notches are still there.
  *Emulator:* `emu-gate/tests/song.rs::a_pattern_programmed_on_step_is_the_expected_song_and_plays_bit_exact` (a lock on one held step and a SET on another, bit exact), `sidou-tests/tests/app/step_view.rs::lfo_on_a_held_step_clears_its_locks_and_its_set`, `sidou-tests/tests/song_locks.rs::locks_stay_sorted_and_round_trip_through_the_trailer` (locks survive a save and reload).
- [ ] **TRACKER, by hand.** SEQ to TRACKER, REC on, SELECT to row 0: four note keys write four
  notes, each advancing a row; K1 the instrument, K2 the command, K3 and K4 the data nibbles.
  ARP writes a key off, LFO clears. EDIT + LFO undoes. PLAY: the bar (or, with TRACKER ROWS
  SCROLL, the rows) follows the song.
  *Emulator:* `emu-gate/tests/song.rs::a_four_note_tracker_phrase_is_the_expected_song_and_plays_bit_exact` (four note keys), `sidou-tests/tests/app/tracker_view.rs::rec_keys_and_knobs_edit_the_shown_pattern_with_one_step_undo` (K1..K4 and EDIT + LFO undo), `sidou-tests/tests/app/tracker_view.rs::a_second_lfo_on_the_same_row_clears_the_whole_row_and_arp_sets_keyoff`.
- [ ] **REC, the three modes.** Menu, REC: set it to each and, with REC on and the song looping
  in TRACKER at tempo 6, play a short line on the keys in time with the song:
  *Emulator:* `sidou-tests/tests/app/song_rec.rs::the_menus_rec_item_chooses_the_quantising` (the menu's REC item picks the mode).
  - **NEAREST** (the default): each note lands on the row it is closest to; a note a little
    before the beat lands on that beat. Stop, and read the rows: the notes are on the beats you
    played them on, not a row late all the time.
    *Emulator:* `sidou-tests/tests/app/song_rec.rs::quantising_is_the_nearest_row_or_the_row_the_key_falls_in`, `sidou-tests/tests/app/song_rec.rs::a_key_while_recording_writes_its_note_at_the_nearest_row_and_sounds`, `emu-gate/tests/song.rs::rec_records_a_timed_key_script_on_the_expected_rows_and_plays_back_bit_exact` (a timed key script lands on the expected rows).
  - **OFF**: each note lands on the row it falls in: a note just before a beat lands on the row
    before it. Compare with NEAREST on the same line.
    *Emulator:* `sidou-tests/tests/app/song_rec.rs::off_records_the_row_the_key_falls_in`, `sidou-tests/tests/app/song_rec.rs::quantising_is_the_nearest_row_or_the_row_the_key_falls_in`.
  - **COUNT-IN**: stop the song, set COUNT-IN, REC on, PLAY: the toast says COUNT-IN, the keys
    sound for one bar but nothing is written, then recording begins; REC turned on while it
    plays counts in too.
    *Emulator:* `sidou-tests/tests/app/song_rec.rs::a_count_in_of_one_bar_runs_before_recording_begins`, `sidou-tests/tests/app/song_leds.rs::rec_dim_when_armed_blinking_in_the_count_in_lit_recording`, `sidou-tests/tests/app/song_rec.rs::rec_on_live_toasts_its_state_and_counts_nothing_in`.
  - Turn K1 and K2 while recording: the row you turned them on gets a cutoff or resonance lock
    (the popup says CUTOFF or RESONANCE) that plays back.
    *Emulator:* `sidou-tests/tests/app/song_rec.rs::k1_and_k2_record_cutoff_and_resonance_locks_on_the_row_they_were_turned_in`, `sidou-tests/tests/song_locks.rs::a_locked_row_holds_its_values_and_the_song_returns_after` (the lock plays back).
  - In STEP, REC on with the song playing: a step key records its note and (staccato) its key
    off on the lane; the pad toggles nothing.
    *Emulator:* `sidou-tests/tests/app/song_rec.rs::a_step_key_while_recording_writes_the_lanes_note_and_its_key_off`, `sidou-tests/tests/app/song_rec.rs::a_legato_lane_records_the_note_alone`.
- [ ] **Mute.** While the song plays, GLO held + key 1 mutes V1 (MUT in red on TRACKER, M1 on
  LIVE), key 4 solos it; again undoes each.
  *Emulator:* `emu-gate/tests/song.rs::glo_and_the_first_key_mute_voice_1_and_its_writes_stop`, `emu-gate/tests/first_use.rs::glo_mutes_a_lane_and_only_that_lane_falls_silent`, `emu-gate/tests/screen.rs::live_shows_the_song_and_the_mutes` (M1 on LIVE).
- [ ] **Save and reload.** SAVE (clean release): SONG SAVED, or SAVE AS for an unnamed song.
  Switch the power off and on: the song loads stopped; PLAY sounds what you made, locks and
  recorded notes included. Open the song in GoatTracker (through SysEx, section 8c): the same
  patterns, with SETs playing as here.
  *Emulator:* `emu-gate/tests/first_use.rs::save_as_and_a_power_cycle_bring_back_the_song_and_a_setting`, `emu-gate/tests/song_store.rs::saved_song_survives_reboot_stopped_and_plays_bit_exact` (plays bit exact, locks and recorded notes included), `sidou-tests/tests/app/step_view.rs::a_step_made_song_plays_as_goattrackers_player_does_with_its_set_and_without_the_chunk_locks`.
  *Device-only:* opening the song in GoatTracker on a computer after a SysEx transfer (external gear).
- [ ] **Feel.** Note how recording feels at tempo 6 and at tempo 3: does a note you play on
  the beat land on that beat? Since 1.1 a key is placed 6.0 ms before the device reads it (the
  output's double buffer and the key's debounce, computed from the code's constants and
  measured in the emulator: [controls.md](reference/controls.md), REC). Play along in OFF too, where
  a key just early lands on the row before. While recording TRACKER follows the playing
  pattern (the notes show as they land) and its cursor comes back when the song stops. Write
  down whether notes played on the beat land on it, early or late, at both tempos, and which
  of NEAREST and OFF you prefer.
  *Emulator:* `emu-gate/tests/first_use.rs::rec_places_a_key_where_it_was_heard` (a key pressed a known time before and after a row is heard, in OFF and NEAREST at tempo 6: the row it lands on, and the compensation as a number), `sidou-tests/tests/app/song_rec.rs::rec_places_a_key_where_it_was_played_the_lag_before_now` (the boundaries at tempo 6 and 3), `sidou-tests/tests/app/song_rec.rs::before_row_0_the_lag_goes_back_to_the_entry_that_played_before`.
  *Device-only:* how recording feels at tempo 6 and 3, the finger's own timing and the speaker's, and the choice between NEAREST and OFF (feel and musicality).

## 15. The output stage: SPREAD and the soft limiter (v0.3.0)

Headphones or a stereo pair on the line out. Controls: [controls.md, SPREAD](reference/controls.md#spread).

- [ ] **SPREAD.** With a song playing on chip A and the keys on chip B (KEYS AUTO), menu,
  SETTINGS, SPREAD: OFF is mono, as before. Step it to 100%: the song is on the left alone, the
  keys on the right alone, neither louder nor quieter than at OFF; between, they move apart
  smoothly, with no click as you step. Release the keys: the song glides back to the centre
  (about 0.1 s, no click) once they have died away, and moves left again when you play.
  Stop the song and play the keys (chip A): at 100% they stay in the centre. Play a song alone:
  it stays in the centre too. Power off and on: SPREAD is as you left it. CC 117 from a sequencer sets it
  too (0..127 over OFF..100%).
  *Emulator:* `emu-gate/tests/chip_b.rs::spread_sets_chip_a_left_and_chip_b_right_within_the_audio_target` (CC 117: 0 is mono, 100 puts A left and B right, a chip alone stays centred), `sidou-tests/tests/app/chip_spread.rs::each_chip_keeps_its_power_at_every_spread` (neither louder nor quieter), `sidou-tests/tests/app/chip_spread.rs::the_glide_moves_the_gains_without_a_step` (no click as it steps or glides).
  *Emulator (no test yet):* SETTINGS' SPREAD item stepped from the panel, and its value kept over a power cycle (`sidou-tests/tests/app/chip_spread.rs::the_cc_and_the_nrpn_set_spread` checks CC and NRPN and the dirty flag only).
  *Device-only:* the left and right image heard on headphones (speaker and analog output).
- [ ] **Effects on a spread pair.** At SPREAD 100% put a reverb (FX: REVERB or LUSH) at its
  default Mix: the dry song stays left and the keys right, with a tail across both sides. A
  distortion at Mix 127 makes the two one (mono); at a lower Mix the dry part stays apart.
  *Emulator (no test yet):* SPREAD 100 with REVERB or LUSH at default Mix: the dry song stays left, the keys right, the tail on both sides; a distortion at Mix 127 makes mono. No test combines the effect slots with the spread stage (`sidou-tests/tests/app/chip_spread.rs::unity_converts_as_the_slots_do` covers unity only).
- [ ] **No setting clips.** MASTER full, two loud chords on both chips at once (a song and the
  keys), FLANGER with its feedback (B) at 127, then PHASER the same, then REVERB and LUSH with
  Amount at 127: loud, but no hard crackle of clipping on the peaks (the soft limiter rounds
  them). Note anything that still sounds clipped, and at which setting.
  *Emulator:* `sidou-tests/tests/app/chip_mix.rs::two_loud_chips_through_the_slots_are_soft_limited_never_on_a_rail`, `sidou-tests/tests/unit/slots.rs::full_feedback_never_reaches_a_rail` (feedback and gain settings stay off the rails), `sidou-tests/tests/app/thump_out.rs::no_thump_clips_two_full_scale_chords_through_any_effect`.
  *Device-only:* hearing any crackle on the peaks through the real output stage (speaker and analog output).

## 16. The drum kit and the digi drums

Headphones or the line out. Controls: [controls.md, STEP](reference/controls.md#step) and
[SONG](reference/controls.md#song); [decision record 0009](decisions/0009-drum-kit-and-digi-drums.md).

- [ ] **The kit.** Menu, SONG, NEW. On STEP, PRESETS on each lane names the instrument
  (`03 KICK` ... `0A RIM`). Put KICK on V1 (steps 1, 5, 9, 13), SNARE on V2 (5, 13), CL HAT on
  V3 (every other step) and play: a kick with weight (a falling thump, not a click), a snare
  with a tonal crack and noise, short bright hats. Try OP HAT, CLAP (three quick bursts), the
  toms (low and a fifth higher, both falling) and RIM. Do it again with chip A an 8580 (CHIP
  view): the kick still has its low end. Note any drum that sounds weak, harsh or out of time.
  *Emulator:* `emu-gate/tests/first_use.rs::every_template_instrument_auditions_on_step_heard_at_its_pitch_and_each_its_own` (PRESETS names each instrument, each is heard, KICK's body at A#1, the toms at C#3 and G#3, each told apart), `sidou-tests/tests/app/step_view.rs::presets_names_the_lanes_instrument`, `emu-gate/tests/song.rs::the_drum_kit_and_a_digi_drum_play_bit_exact_with_the_host`.
  *Emulator:* `emu-gate/tests/first_use.rs::every_template_instrument_on_steps_plays_heard_and_on_time` (the kit's drums within the first 80 ms of their row); `emu-gate/tests/first_use.rs::the_kits_drums_sound_one_frame_after_their_row` (each drum one frame after its row), `sidou-tests/tests/kit.rs::every_new_instrument_starts_on_time` (the kit, the lead and the bass on time at every phase of the chip's rate counter, on reSIDfp as on SIDOU's chip), `sidou-tests/tests/kit.rs::a_staccato_drum_sounds_as_a_held_one`.
  *Emulator (no test yet):* the kick keeps its low end with chip A an 8580 (level in the kick's band on both models).
  *Device-only:* whether a drum sounds weak, harsh or out of time to the ear, the kick's weight and the snare's crack (speaker, feel and musicality).
- [ ] **Digi drums.** On the same song set a lane to `0B DG KICK`, `0C DG SNARE`, `0D DG CLAP`
  or `0E DG HAT`: sampled drums, gritty as a C64's, on the beat with the chip drums (no flam);
  the chip copy of the drum is not heard under them. On an 8580 they are as loud as on a 6581.
  GLO-mute the digi lane: it goes silent. A digi drum and a chip drum on the same lane in turn:
  the chip drum after a digi one comes a few milliseconds late at most (listen for a flam).
  *Emulator:* `sidou-tests/tests/digi.rs::a_digi_note_plays_its_sample_and_holds_its_voice_off` (the sample from the call on, the chip copy not heard, the 8580 at the 6581's level), `sidou-tests/tests/digi.rs::a_muted_voice_plays_no_digi` (GLO mute silences it), `emu-gate/tests/first_use.rs::every_template_instrument_on_steps_plays_heard_and_on_time` (digi drums on time with their row).
  *Emulator (no test yet):* a chip drum after a digi one on the same lane starts at most a few milliseconds late (the flam); `emu-gate/tests/first_use.rs::the_kits_drums_sound_one_frame_after_their_row` checks a chip drum after a chip drum or a note only.
  *Device-only:* the gritty C64 character by ear (speaker).
- [ ] **In GoatTracker.** SAVE the song, copy it off (Live Link or SysEx) and open it in
  GoatTracker 2.77: it loads and plays, the digi lanes as the chip kit's drums.
  *Emulator:* `sidou-tests/tests/digi.rs::the_chunk_round_trips_and_goattracker_keeps_only_the_gt2_part`, `sidou-tests/tests/gt2_match.rs::the_drum_kit_matches` (the kit's tables replay as the reference player plays them).
  *Device-only:* GoatTracker 2.77 loading and playing the copied song on a computer (external gear).

## 17. Code in Felucca's debug area (v0.4.0)

**Not used by v0.4.0.** A release candidate let the code that runs from SRAM (RAMTEXT) reach up
to RAM, over the 8 KB below it (0x01C0C000..0x01C0E000) that Felucca keeps for RAM-run debug
images. In its device session (2026-10-24) STEP's edits seemed not to land and this area was
suspected; that was a misread view (TRACKER, where a key only auditions without REC), and the
faint sound was NEW's silent lead (fixed separately). The area was never shown to fail, and never
proved safe either, so the release keeps code out of it: RAMTEXT ends at 0x01C0C000 again (48 KB), `app.ld`
asserts `_rt_end <= 0x01C0C000` and emu-gate `ram_audio.rs` checks it, and crt0.S saves its five
boot words there as Felucca's does ([hardware.md, Code in SRAM](hardware.md#code-in-sram)).

Check on the release build (the emulator's `ram_audio` test prints `.ram_text`'s size: at most
49,152 bytes):

- [ ] **STEP's edits land.** On a new song, in STEP, press and release a step key on V1: the
  step toggles on (a note in the pattern), and again toggles it off. Do it for a short tap and a
  held press, on a few steps and lanes; PLAY plays what the pattern shows.
  *Emulator:* `emu-gate/tests/ram_audio.rs::the_boot_copies_the_audio_path_to_sram` (RAMTEXT ends at 0x01C0C000, clear of Felucca's debug area), `emu-gate/tests/first_use.rs::chance_dice_and_mutate_on_step_do_what_they_say` (STEP's edits reach the pattern in fm1-emu), `emu-gate/tests/first_use.rs::save_as_and_a_power_cycle_bring_back_the_song_and_a_setting` (taps toggle steps that play).
  *Device-only:* confirming on the unit that a step key toggles the pattern: the failure was in real SRAM behaviour and the emulator applied the same edits from the same image (real silicon).
- [ ] **Boot.** It boots to the title card and LIVE, the first boot and a few power cycles after
  it (section 3), and a held chord plays clean on both chip models.
  *Emulator:* `emu-gate/tests/boot.rs::boots_and_plays_a_held_key_sample_exact`, `emu-gate/tests/screen.rs::the_short_boot_shows_the_title_card_then_live`.
  *Device-only:* real boots and power cycles to the title card and LIVE, and a held chord clean by ear on both chip models (real silicon, speaker).
- [ ] **A store write.** SAVE the song (section 13) and a setting (section 7.1): both survive a
  power cycle. (The flash driver runs from RAMTEXT with XIP off.)
  *Emulator:* `emu-gate/tests/first_use.rs::save_as_and_a_power_cycle_bring_back_the_song_and_a_setting` (a song and a setting survive a power cycle), `emu-gate/tests/song_store.rs::saved_song_survives_reboot_stopped_and_plays_bit_exact`.
  *Device-only:* the flash driver running from RAMTEXT with XIP off on the real chip (real silicon, flash chip).

## 18. Generative play in STEP (chance, MUTATE, DICE)

By ear; no device risk beyond the usual song editing.

1. In STEP on a song with a steady pattern on V1, hold a step with a note and turn ALGORITHM
   anticlockwise: the popup says `CHANCE 95%` and counts down by 5. Let go, press PLAY: at 50 % the
   step should play about every other pass, never in a pattern you can predict; at 0 it is silent;
   at 100 (turn back up) the notch is gone and it always plays. A skipped step must leave the
   notes around it, the key-off rows and the timing alone.
   *Emulator:* `emu-gate/tests/first_use.rs::chance_dice_and_mutate_on_step_do_what_they_say` (the CHANCE popup, the step keeps its note, the song holds the lock), `sidou-tests/tests/app/step_view.rs::algorithm_with_a_step_held_is_its_chance_a_lock_that_is_gone_at_one_hundred`, `sidou-tests/tests/song_locks.rs::a_step_with_a_chance_plays_by_its_roll_and_the_player_otherwise_plays_as_ever` (about C in 100, the rest of the song unchanged).
   *Emulator (no test yet):* the firmware image playing a 50 % step over many passes in fm1-emu (the roll is tested on the host only).
2. Stop and PLAY again: the passes differ from the first run (a new seed each PLAY).
   *Emulator:* `sidou-tests/tests/song_locks.rs::the_roll_is_seedable_and_a_chance_of_c_plays_about_c_in_100` (the roll follows its seed).
   *Emulator (no test yet):* two PLAYs of a 50 % step differ (a new seed each PLAY).
3. EDIT + SCL (DICE) on V1: a new eighth-note pattern in a minor pentatonic around the lane's note
   appears row by row (the edits are written one at a time; about a second for 64 rows). Press it
   again for another. Does it sound musical on your instrument? Note what you would change
   (density, range, the scale). Press EDIT and SCL together as a hand does, either first, and
   turn K1 at once: DENSITY moves, never the effect slot (SCL selects no slot in the chord).
   *Emulator:* `emu-gate/tests/first_use.rs::chance_dice_and_mutate_on_step_do_what_they_say` (NEW PATTERN, eighth notes all in key, key offs, PLAY heard through several pitches), `sidou-tests/tests/app/step_view.rs::dice_writes_a_new_pattern_in_the_scale_on_the_grid_as_ordinary_edits`, `emu-gate/tests/first_use.rs::scl_before_edit_is_dices_chord_too` and `sidou-tests/tests/app/step_view.rs::scl_landing_before_edit_is_the_same_chord_and_k1_goes_to_dice` (SCL 5 ms before EDIT, K1 at once: DICE's density, no slot change).
   *Device-only:* whether it sounds musical and what to change in density, range and scale (feel and musicality); the chord with the real buttons' timing.
4. EDIT + ARP (MUTATE) a few times on a pattern you like: each press should change it a little (two
   notes at most) and stay in key. Play while pressing.
   *Emulator:* `emu-gate/tests/first_use.rs::chance_dice_and_mutate_on_step_do_what_they_say` (MUTATED, one or two notes change, still in key), `sidou-tests/tests/app/step_view.rs::mutate_changes_a_little_stays_in_the_scale_and_valid_every_time`, `sidou-tests/tests/app/step_view.rs::mutate_and_dice_wait_for_a_step_to_be_let_go_and_leave_rec_alone`.
5. On a song whose SCNE chunk gives a voice the role DRUMS, DICE on that
   lane keeps its one note and puts hits on the sixteenths with a heavy downbeat.
   *Emulator:* `sidou-tests/tests/app/step_view.rs::dice_on_a_drum_lane_keeps_its_note_and_hits_on_the_sixteenths_the_downbeat_most`.
6. Save, reopen the song in GoatTracker 2: the pattern is plain; every step plays (GoatTracker
   ignores chance).
   *Emulator:* `sidou-tests/tests/song_locks.rs::a_chance_is_a_lock_that_round_trips_and_leaves_the_gt2_bytes_alone` (the GT2 bytes are plain), `sidou-tests/tests/app/step_view.rs::a_step_made_song_plays_as_goattrackers_player_does_with_its_set_and_without_the_chunk_locks`.
7. (1.1) Hold EDIT + SCL: the frame shows DENS, RANGE, SCALE, RHYTHM and ROOT at 2x. Turn K2 and
   K4 through their choices and roll each rhythm (let go without a turn): OFFBEAT, 3-3-2, FOUR and
   BROKEN should each be heard as its name says, on a melodic and on a drum lane; BASS should sit
   under the lead. Press a key while holding the chord: ROOT follows it and the roll still comes.
   *Emulator:* `emu-gate/tests/first_use.rs::dice_settings_on_step_are_kept_in_the_song_and_the_next_roll_uses_them` (K1-K4 turned at once after SCL), `emu-gate/tests/screen.rs::the_step_view_matches_its_goldens` (golden step-dice), `sidou-tests/tests/app/step_view.rs::each_rhythm_hits_as_its_weights_say_on_melodic_and_drum_lanes`, `sidou-tests/tests/app/step_view.rs::each_range_keeps_to_its_octaves`, `sidou-tests/tests/app/song_gen.rs::a_note_key_with_edit_and_scl_held_sets_the_lanes_root_and_the_roll_still_comes`.
   *Device-only:* whether each rhythm and range sounds musical (by ear).
8. (1.1) In TRACKER: EDIT + SCL, EDIT + ARP and EDIT + ENV on the shown voice, the frame over the
   lower rows while EDIT + SCL is held. On a voice whose instrument uses the filter, EDIT + ENV and
   PLAY: the cutoff should open (SWEEP UP) or close (SWEEP DOWN) across the pattern, smoothly
   enough to use.
   *Emulator:* `emu-gate/tests/first_use.rs::tracker_dices_the_shown_voice_on_its_root_and_sweeps_its_filter`, `emu-gate/tests/screen.rs::the_tracker_dice_frame_matches_its_golden`, `sidou-tests/tests/app/song_gen.rs::a_swept_song_plays_in_goattrackers_player_as_written`.
   *Device-only:* the sweep heard, and whether every second row is fine-grained enough.
   *Device-only:* GoatTracker 2 opening and playing the saved song on a computer (external gear).

## 19. USB audio out

What it is: [README, What you can do](../README.md#what-you-can-do). The unit is one USB
device, the MIDI port and a class-compliant audio input "SIDOU FM-1" (2 channels, 16-bit,
44.1 or 48 kHz), Felucca's `usb.c` audio input. The host tests
(`host/crates/sidou-pack/tests/usb_audio.rs`, and Felucca's own `uac_test.c`) and fm1-emu's own USB
controller and host in emu-gate (`tests/usb.rs`) check the firmware's side; only the unit shows
the chip's.
A note held (or a song playing) throughout unless an item says otherwise.

- [ ] **macOS:** plug in. Audio MIDI Setup lists "SIDOU FM-1" with 2 input channels and the
  formats 44,100 and 48,000 Hz, and the MIDI port "SIDOU FM-1" as before (no driver install,
  no prompt). Record what the Format menu shows:
  *Emulator:* `sidou-pack/tests/usb_audio.rs::the_device_is_one_composite_usb_midi_and_class_compliant_audio_input` (descriptors, strings), `sidou-pack/tests/usb_audio.rs::feluccas_own_usb_audio_test_passes_on_sidous_usb_c` (the descriptors as a host parses them, SET_CUR's rate), `emu-gate/tests/usb.rs::a_host_records_the_dacs_output_over_usb_audio_at_44_1_and_48_khz`.
  *Device-only:* what Audio MIDI Setup shows on a real Mac, with no driver or prompt (real USB hosts and OSes, CoreAudio).
- [ ] **Windows 10/11:** Sound settings, Input lists the unit (its name may be "Microphone
  (SIDOU FM-1)" or "Line (...)"); its Properties, Advanced lists 2 channel 16 bit at 44100
  and 48000 Hz. The MIDI port still shows in a DAW or MIDI-OX. Record the names:
  *Emulator:* the same descriptor and recording tests as for macOS (`usb_audio.rs`, `usb.rs`).
  *Device-only:* Windows's own device names, Advanced formats and the MIDI port in a DAW or MIDI-OX (real USB host and OS driver).
- [ ] **Linux:** `arecord -l` lists the card; `arecord -D hw:<card> -f S16_LE -c 2 -r 44100
  -d 10 a.wav` and the same with `-r 48000` record 10 s each that play back clean; `amidi -l`
  still lists the MIDI port.
  *Emulator:* the same descriptor and recording tests as for macOS.
  *Device-only:* `arecord` and `amidi` on a real Linux host, recordings that play back clean (real USB host and OS driver).
- [ ] **iPad (USB-C, or a Lightning camera adapter):** a recording app (GarageBand, AUM)
  offers the unit as an input and records it; the MIDI port works in the same app.
  *Device-only:* a recording app on a real iPad, the MIDI port in the same app (real USB host and OS).
- [ ] **44.1 kHz in a DAW:** project at 44.1 kHz, the unit as the input, record a minute of a
  song: no clicks, gaps or pitch drift; the recording sounds like the headphone out.
  *Emulator:* `emu-gate/tests/usb.rs::a_host_records_the_dacs_output_over_usb_audio_at_44_1_and_48_khz` (the recorded frames are the DAC's in order, no gap, no empty IN token, over 1,000 frames), `sidou-pack/tests/usb_audio.rs::a_host_records_every_frame_in_order_at_44_1_khz_bit_for_bit`.
  *Device-only:* a minute in a real DAW over a real cable: clicks, gaps or pitch drift against the DAW's clock, and that it sounds like the headphone out (real DAW and host, ear).
- [ ] **48 kHz in a DAW:** the same at 48 kHz (the unit resamples): no clicks or gaps, no
  audible aliasing on a bright patch (noise or a high pulse).
  *Emulator:* `emu-gate/tests/usb.rs::a_host_records_the_dacs_output_over_usb_audio_at_44_1_and_48_khz` (48 kHz packets of 47..49 frames, level within 3 % of the DAC's), `sidou-pack/tests/usb_audio.rs::a_host_records_every_resampled_frame_in_order_at_48_khz`.
  *Device-only:* audible aliasing on a bright patch in a real DAW (ear, real host).
- [ ] **No dropouts while the screen works:** record two minutes while you move through the
  views fast (SCENE, the CHIP view, the song editor), scroll lists, and turn K1..K4: the
  recording has no clicks. Then the same with the effect slots at their heaviest (FLANGER,
  PHASER, REVERB on, two chips playing).
  *Emulator:* `emu-gate/tests/usb.rs::recording_over_usb_audio_costs_the_audio_interrupt_little` (recording adds at most USB_AUDIO_PER_SAMPLE instructions to the audio interrupt with the SIE answering at once, and at most 3.0 with fm1-emu's estimated 1 µs SIE round trip).
  *Emulator (no test yet):* the emulated host records for minutes while the panel moves through the views, scrolls and turns K1..K4, then with the heaviest effect slots and two chips: underruns, overruns, missed blocks and empty IN tokens stay 0 (`usb.rs` checks those counters with a chord held only).
  *Device-only:* clicks in a recording made on a real host (real USB host, ear).
- [ ] **What recording costs the audio interrupt:** with a chord held, time the audio interrupt
  with TIMER4 (24 MHz) while a host records and while it does not, at 44.1 and 48 kHz, and note
  the difference a sample. It replaces fm1-emu's estimate of the SIE's round trip (1 µs), on
  which the emulator's bound of 3.0 instructions a sample rests (0.73 with the SIE at once).
  STATUS's meter times the 6502's slices only, so this needs a checklist build that times the
  interrupt.
  *Device-only:* the SIE's real round trip and the interrupt's real cost (real silicon, real USB host).
- [ ] **Level:** MASTER turns the recording's level as it turns the speaker's (USB takes the
  output after MASTER). At full MASTER, two loud chords: the recording's peaks are where the
  line out's are, no harder clipping than on the line out.
  *Emulator:* `emu-gate/tests/usb.rs::a_host_records_the_dacs_output_over_usb_audio_at_44_1_and_48_khz` (the recording is the DAC's output, so it has the same peaks and clipping as the line out), `sidou-pack/tests/usb_audio.rs::full_scale_reaches_the_host_unchanged`.
  *Emulator (no test yet):* MASTER turned down scales the recorded frames as it scales the DAC's.
- [ ] **Saving while recording:** record while you SAVE a song or a setting (a flash write):
  the sound pauses in the recording exactly as on the speaker (the store's hold), then goes
  on; note any click beyond the speaker's.
  *Emulator:* `sidou-pack/tests/usb_audio.rs::a_held_block_is_recorded_as_silence` (a store hold goes to the host as silence), `sidou-pack/tests/usb_audio.rs::a_starved_timer5_skips_a_block_and_the_stream_recovers`.
  *Emulator (no test yet):* a SAVE through the firmware image while the emulated host records: the silence in the recording matches the DAC's.
  *Device-only:* any click beyond the speaker's, heard in a real recording (ear, real host).
- [ ] **USB-MIDI beside it:** while recording, the MIDI items of section 8 pass (notes in, K1's
  CC 74 out, a SysEx file put and get, section 8c).
  *Emulator (no test yet):* with the emulated host reading the audio endpoint (`ep4_reading`), K1's CC 74 out, notes in and a 4 KiB SysEx PUT and GET all work.
  *Device-only:* a real DAW using both at once (real USB host and DAW).
- [ ] **Updates beside it:** `fm1_install.py --info` and a reinstall over SIDOU pass while the
  DAW has the input open (section 8.1); after the update the unit enumerates with its audio
  input again.
  *Emulator (no test yet):* the update's step 1 (`emu-gate/tests/usb.rs::the_running_firmware_serves_an_updates_whole_step_one`) while the emulated host reads the audio endpoint, and the audio endpoint enumerating again after the update's restart.
  *Device-only:* a real DAW holding the input open while the real loader updates the unit (install tools, real host).
- [ ] **Stop and start:** stop the recording and start it again ten times, close and reopen the
  DAW, unplug and replug mid-recording: the stream comes back every time, the MIDI port too.
  *Emulator:* `emu-gate/tests/usb.rs::a_host_records_the_dacs_output_over_usb_audio_at_44_1_and_48_khz` (the stream stopped, by the bus reset of a new enumeration as fm1-emu's host has no SET_INTERFACE to alt 0: no packets, the hand-over goes on).
  *Emulator (no test yet):* ten stop and start cycles and a bus reset mid-recording, the stream and the MIDI port coming back each time.
  *Device-only:* a real DAW closed and reopened and the cable replugged mid-recording (real host and DAW).
- [ ] **Host asleep:** record, put the computer to sleep, wake it: the sound never stops on the
  speaker; recording works again after the wake.
  *Device-only:* a real host's suspend and resume with the unit playing (real USB hosts and OSes; the emulated host has no suspend).
- [ ] *(If you can measure it.)* The unit's current while recording against idle, at 44.1 and
  48 kHz:
  *Device-only:* the unit's real current while recording at 44.1 and 48 kHz (a measurement on real silicon).

Notes:

## 20. .sid tunes over SysEx PUT (M3 Task 10)

What it is: [Put a .sid tune on the FM-1](manual/sid-tunes.md); the protocol's type 2 in
[SysEx protocol, version 1](reference/sysex.md#sid-tunes-type-2). fm1-emu checked it over TRS
(`host/crates/emu-gate/tests/psid_store.rs`); the unit has not run it. The build stores tunes
but these checks end at the store (section 21 plays them). Use
disposable tune names, a writable store (section 7.1) and tunes you may copy; keep a backup of
songs you want. Make the files first, from `host/`, with a tune of your own (`a.sid`) and a
second one:

`cargo run --release -p sidou-sysex -- sid a.sid b.sid --out ../build/syx`

- [ ] **The files:** each tune prints its store name, title, songs and chips and `N messages`;
  `../build/syx/a.syx` and `b.syx` exist. A copy of `a.sid` with its first four bytes changed to
  `RSID` (a hex editor) is refused (`refused: RSID: NOT YET`) and gets no `.syx`. Record the
  names:
- [ ] **Over USB, a pause between messages:** in the SysEx tool (manual, section 2) send `a.syx`
  to the port "SIDOU FM-1" with one second after each message. RECEIVING, then SAVED. Record the
  tool, the time taken and the toasts:
- [ ] **It is on the store:** INQ and DIR as in section 8c: DIR lists the tune's name with type
  2 and the `.sid`'s length; GET it (section 8c) and compare it with `a.sid` byte for byte.
- [ ] **Over USB, too fast:** send `b.syx` with no pause. The upload is refused (STORE BUSY:
  RESEND or MIDI: BAD PACKET) and DIR does not list `b`'s name, or lists it whole if the
  computer happened to be slow enough. Send it again with the pause: SAVED.
- [ ] **Refusals:** the command refuses an RSID, a MUS or a broken file before there is a
  `.syx`, so there is nothing of one to send from a SysEx tool; fm1-emu checks the device's own
  refusals (each toast at the first DATA, no flash program). Here, check only that a `.syx` of a
  song (any `.syx` of section 8c's PUT of a type-1 file) still goes in as before.
- [ ] **Over TRS:** from a MIDI interface's OUT to the TRS MIDI IN, send `a.syx` again with one
  second after each message (`--trs` prints the time): RECEIVING, then SAVED; DIR over USB still
  lists one file of that name (replaced, not doubled), same length.
- [ ] **A song beside it:** the SONG view's browser lists the songs only, never the tune; a
  song loads and plays as before.
- [ ] **Store full:** only if the store already holds many files (do not fill it for this):
  a tune's upload when INQ's free bytes are fewer than its length is refused with STORE FULL
  (USB: NAK 5 at its first DATA) and nothing changes.

Notes:

## 21. .sid playback and the 6502's meter (M3 Task 12: the flash day)

What it decides: whether the 6502 that plays a `.sid` fits the CPU on the unit. fm1-emu counts
instructions, not cycles; it does not model the I-cache that the 6502's flash code runs through
(docs/hardware.md, "Open checks"). **Run this on the flash day, with the user: nothing is
changed on guesses.** Tune and chip behaviour that fm1-emu already proves (bit-exact with the
host, the look-ahead, the stamped writes) is not re-tested here, only the device's speed and
sound.

**What the meter is.** The main loop's pass that runs the 6502 (a slice of at most 1,000 of its
cycles: the code, the voice shadows, the write ring) is timed with TIMER4 (24 MHz). STATUS,
while a tune is loaded, shows on its `CPU` row `S.S% T/KC`: the share of the CPU those slices
took over the last second (to a tenth of a percent), and the 24 MHz ticks a thousand 6502 cycles
took. `CYCLES/S` is the 6502 cycles a second (985,248 at the PAL clock, when it is not behind),
`LATE` the writes the audio applied after their cycle, `OVERRUNS` the play calls that ran past
their period. An audio interrupt that lands inside a slice counts in it, so the share reads a
little high: it is the pessimistic side. In fm1-emu the rows read from the same counters (the
emulator's TIMER4); the dojo tune there reads about 1.2 % and 296 ticks a thousand cycles, which
is the figure to set the unit's beside.

Set up: the build with the starter pack on the store (`sidou-sysex sid ../assets/starter --pack`,
section 20's way of sending), the unit's clock noted (docs/hardware.md: 360 MHz as M-VAVE's head
leaves it, 240 MHz if a unit or build changes it; the budgets assume 240), a tune of the
user's heaviest HVSC set: the study's `Sound_Test`, `Thomkat_with_Strange_End`, `Kill_Me` and
`Last_Ninja`, sent as in section 20. fm1-emu's figures for each tune (run on the host first:
`LATE` and the instruction counts) go in the same table.

For each tune, 60 s of play, once with LIVE showing (HOME back to the keyboard view while it
plays) and once with STATUS (or NOW PLAYING) showing; read STATUS's rows (page 1, the first on opening: SELECT turns the pages) at the start, after 30 s
and after 60 s, and listen for clicks, dropouts, a tune that stops early or hangs (`PLAY HUNG`,
`INIT TOO LONG`):

- [ ] **Starter pack, every tune:** each plays from the start to a minute without a late write
  or a glitch you can hear; record `CPU` and `T/KC` (the largest of the three reads), `LATE`,
  `OVERRUNS`:
- [ ] **Sound_Test:** `CPU` / `T/KC` / `LATE` / `OVERRUNS` with LIVE showing, then with STATUS:
- [ ] **Thomkat_with_Strange_End:** the same:
- [ ] **Kill_Me:** the same:
- [ ] **Last_Ninja:** the same:
- [ ] **A 2SID tune** (if one is at hand): the same, and the second chip is heard:
- [ ] **The song views while it plays:** open SONG, STEP, TRACKER: each says `SID PLAYING: STOP
  TO EDIT`; the tune does not glitch as the views change. STATUS's `CPU` with the song views
  not drawn, and with the LCD redrawing the most (NOW PLAYING):
- [ ] **Stop and back:** PLAY stops the tune to silence and returns the song; the next tune
  starts from silence. `CPU` reads `--` when no tune runs:
- [ ] **A sound edit is not the song's:** on the built-in loop, turn PRESETS and K1 on LIVE, then
  play a tune from SIDS: no `SONG NOT SAVED`; once it ends the sound on the keys is the edited one.
  *Emulator:* `emu-gate/tests/psid_views.rs::a_sound_edit_asks_nothing_and_survives_a_tune`,
  `sidou-tests/tests/app/song_io.rs::a_sound_edit_is_not_the_songs_and_a_tune_keeps_it`.
- [ ] **At the unit's other clock,** only if the first run is over the rule below: the same
  tune, the same reads, at the other clock. Record the clock:

**The decision rule.** Take the worst tune's `CPU` of all the reads. If it is **above 15 %** of
the CPU, or **any late write** shows in a 60 s run of a tune that has none in fm1-emu, then the
fallbacks are, in this order:

1. `cpu6502.c` at `-O2` (+2.6 KB of flash, about 7 % fewer instructions);
2. the dispatch loop in RAMTEXT (it needs the room: only 416 bytes of RAMTEXT's 48 KB are free, so
   room is made first);
3. last, OSC3 and ENV3 read from the audio chip a frame late instead of the voice shadow (a tune
   that waits on ENV3 then hangs to the play cap, so this one only the user may choose).

At or under 15 % with no late write: M3 ships as built. The controller brings the numbers to the
user; nothing is changed on guesses. The results go in `docs/hardware-results/` (one file for
the day, as the earlier flash days').

Notes:

## 22. LAB (1.0)

What it decides: whether LAB reads and feels right in the hand. What fm1-emu proves (the pages,
their goldens and budgets, the store, undo and the rolls) is named under each item; on the unit
only what it cannot see: the LCD at arm's length, the ear, the flash across a real power cut and
the feel. Set up: the build on the unit, LIVE after the boot, a note key to hold.

- [ ] **ENV, LFO and FX on LIVE open ENVELOPE, MOD and FX; SELECT steps the six pages** (OSC,
  ENVELOPE, FILTER, MOD, FX, MACRO); HOME goes back to LIVE.
  *Emulator:* `emu-gate/tests/lab.rs::env_lfo_and_fx_on_live_open_lab_at_their_pages`,
  `emu-gate/tests/lab.rs::lab_opens_from_the_menu_and_home_goes_back_to_live`.
  *Device-only:* the pages' drawings readable at arm's length on the LCD, the lit card visible in
  daylight.
- [ ] **Turn every card of every page with a key held:** the drawing follows each turn.
  *Emulator:* `sidou-tests/tests/app/lab_view.rs::the_pages_match_their_goldens` and the drawing
  tests beside it, `emu-gate/tests/lab.rs::lab_osc_and_envelope_draw_within_their_budgets`,
  `emu-gate/tests/lab.rs::lab_filter_mod_and_fx_draw_within_their_budgets`.
  *Device-only:* that what is drawn matches what is heard (the filter's peak, the envelope's
  length), by ear.
- [ ] **SAVE keeps:** the toast, `U01` on LIVE, PRESETS on LIVE reaches it after DICE; power off
  and on: the sound and the unsaved edit come back.
  *Emulator:* `emu-gate/tests/lab.rs::a_happy_accident_is_kept_with_one_press`,
  `emu-gate/tests/lab.rs::a_kept_sound_survives_a_power_cycle`,
  `emu-gate/tests/lab.rs::the_edit_comes_back_at_power_on`,
  `emu-gate/tests/lab.rs::presets_reaches_the_user_sounds_on_live_and_in_lab`.
  *Device-only:* the real flash across a real power cut (the battery pulled mid-save: the old
  sounds or the new, never neither); the keep flow's feel (one press, the toast read in time).
- [ ] **EDIT + SAVE names a slot with the keys.**
  *Emulator:* `emu-gate/tests/lab.rs::edit_save_names_and_writes_a_slot`.
  *Device-only:* typing on the keys feels right.
- [ ] **EDIT tapped compares, EDIT + LFO undoes, EDIT + ARP varies, EDIT + SCL rolls.**
  *Emulator:* `emu-gate/tests/lab.rs::edit_tapped_swaps_the_sound_heard`,
  `emu-gate/tests/lab.rs::edit_lfo_undoes_in_lab`,
  `emu-gate/tests/lab.rs::edit_arp_and_edit_scl_change_the_sound_heard_and_undo`,
  `sidou-tests/tests/app/lab_edit.rs::ten_thousand_varies_and_rolls_are_never_silent`.
  *Device-only:* the tap's timing in the hand; whether the variations sound like useful
  accidents (feel and musicality).
- [ ] **MIDI learn on a card from a real controller** (SAVE held, turn a card, move the
  controller's knob).
  *Emulator:* `emu-gate/tests/lab.rs::midi_learn_on_a_lab_card_over_trs`.
  *Device-only:* a real controller over the MIDI adapter and over USB.
- [ ] **The chip model from ALGORITHM on TONE** (FX no longer toggles it): R5 is the 8580, and
  FX's LED is lit while it plays; a slot's algorithm from ALGORITHM on LAB's FX page (down to
  OFF bypasses it, what ENV's cycle once did).
  *Emulator:* `emu-gate/tests/live.rs::the_model_toggle_swaps_chips_without_a_gap` (ALGORITHM
  on TONE both ways, FX's LED lit for the 8580), `emu-gate/tests/keys.rs::the_panels_chip_model_acts_on_the_keys_setup`.
  *Device-only:* the LED.

Notes:

## 23. Auditions before writing (1.1)

What it decides: whether the ARP helper's draft and a held step's locks sound right through the
unit's output, and whether the audio interrupt keeps its time while they play over a song. What
fm1-emu proves (the registers each audition writes, RAMTEXT, the player's worst block) is named
under each item. Set up: the build on the unit, a song with a lead on V1 (SIDOU LOOP will do).

- [ ] **The ARP helper's draft on the keys.** TABLE, the WAVE table, a blank row (or the
  instrument's own, from INSTR's EDIT + SEQ on WAVE PTR), ARP: `HEARING DRAFT` under the name.
  Hold a note key: the arpeggio plays. Turn K1/K2 on a step, K3 (speed), K4 (loop) and ALGORITHM
  (steps) with the key still held: each turn is heard at once, with no click or stuck note. HOME:
  the key plays the song's rows again and the table is unchanged. ARP again, SAVE: the same
  arpeggio as before SAVE.
  First, after power-on with SIDOU LOOP stopped: TABLE, the WAVE table, a blank row past the
  song's, ARP: the unit does not restart (the 1.1.0 candidate did: a misaligned word copy).
  *Emulator:* `emu-gate/tests/song.rs::the_arp_helpers_draft_plays_on_chip_a_before_save` (two
  steps an octave apart on chip A, a K2 turn heard on the held note, HOME writes nothing, no
  access the FM-1 faults on),
  `emu-gate/tests/song.rs::the_arp_helper_on_an_empty_row_of_the_built_in_loop_does_not_crash`
  (blank rows from the first after the song's to FE: no reset, no misaligned 16/32-bit RAM access,
  which the chip faults on and fm1-emu carries out),
  `sidou-tests/tests/song_ctl.rs::a_drafts_jam_sounds_as_the_saved_rows_do` (the draft's render
  equals the saved rows' call for call), `a_drafts_change_is_heard_on_the_held_note`,
  `sidou-tests/tests/app/song_pages.rs::the_arp_helpers_draft_is_heard_until_it_closes_or_its_rows_are_written`.
  *Device-only:* listening (the arpeggio clean on the held note through every turn).
- [ ] **A held step plays its locks.** STEP, V1: hold a step and turn K1 well down, K2 up, K3:
  each turn is heard on the held note at once; let go and hold it again: the same darker, resonant
  sound; let go: the song's own sound on the next notes. EDIT + K1 (a character lock) on the step
  is not heard in the audition, only when the song plays the row.
  *Emulator:* `emu-gate/tests/song.rs::a_held_step_sounds_its_cutoff_lock_on_chip_a`,
  `sidou-tests/tests/song_locks.rs::a_held_steps_audition_plays_its_row_as_the_sequencer_does`,
  `a_held_steps_locks_go_at_its_release_and_leave_the_next_rows_alone`,
  `sidou-tests/tests/app/step_view.rs::a_held_step_gives_the_player_its_locks_and_its_release_takes_them_back`.
  *Device-only:* listening (the filter heard as locked, no click at the release).
- [ ] **Both with a song playing.** PLAY, then hold a step in STEP (its locks over the song on
  that voice), and in TABLE's helper hold a key: the other voices play on unchanged, no dropout,
  no late note, STATUS's meter where it was.
  *Emulator:* `emu-gate/tests/ram_audio.rs` (the draft's reads and the held locks run from SRAM:
  `.ram_text` 49,072 of 49,152 bytes), `emu-gate/tests/song.rs::the_players_worst_block_fits_its_budget`
  (48.9 instructions a sample of 60), `sidou-tests/tests/song_ctl.rs::a_draft_plays_only_on_its_channel`.
  *Device-only:* the audio interrupt's real timing with the song playing (by ear, and STATUS's
  meter).

## 24. LAB 1.1: the filter LFO, ARP RATE, peek and macros

What it decides: whether 1.1's additions to LAB sound and feel right. Set up as section 22.

- [ ] **The filter LFO (FILTER's K4) on a 6581 and on the 8580:** SAW LEAD, K4 up a few detents
  from off (the rate starts at 1 Hz), a chord held; then K4 to 32 and OSC's PWM HZ to its top.
  *Emulator:* `sidou-tests/tests/app/live_macro.rs::the_filter_lfo_sweeps_k1s_walk_each_way_at_the_pwm_rate`
  (the sweep, smooth between the walk's entries), `sidou-tests/tests/app/lab_view.rs::the_filter_lfo_draws_its_band`,
  `emu-gate/tests/lab.rs::lab_filter_mod_and_fx_draw_within_their_budgets` (the page with the LFO
  sweeping), `emu-gate/tests/midi.rs::a_filter_cc_flood_fits_the_isr_overhead_at_any_age`.
  *Device-only:* by ear: no zipper or stepping on the 6581's filter at slow rates and deep sweeps
  (the cutoff moves up to 100 times a second, 1/256 of a detent at a time), and whether 32
  detents each way is a useful range.
- [ ] **ARP RATE on MOD's K4:** ARP on with ALGORITHM, a chord held, K4 through the rates; with a
  MIDI clock coming in, the last three follow it.
  *Emulator:* `sidou-tests/tests/app/lab_view.rs::mod_k4_is_the_arp_rate_dark_while_arp_is_off`,
  `sidou-tests/tests/app/live_macro.rs::the_arp_rate_is_a_parameter_on_cc_28s_row`.
  *Device-only:* none beyond section 22's (the card read at arm's length).
- [ ] **Hold-to-peek:** an edited sound, EDIT held, a few notes played: A while held, B back on
  letting go; the header's outlined `A`.
  *Emulator:* `emu-gate/tests/lab.rs::edit_held_and_a_key_peeks_at_a` (heard, its input within
  the poll allowance), `sidou-tests/tests/app/lab_view.rs::a_note_while_edit_is_down_peeks_at_a`,
  `sidou-tests/tests/app/lab_view.rs::a_control_during_a_peek_acts_on_b`.
  *Device-only:* the feel: whether the switch at the first key and back at EDIT's release is
  quick enough to compare by ear, and whether the outlined `A` reads apart from the reversed one.
- [ ] **The macro links (MACRO):** RES LNK at `+8` then `-8`, a chord held while K1 sweeps on LIVE;
  PWM LNK at `+6` on PWM BASS while K3 (the width) turns.
  *Emulator:* `sidou-tests/tests/app/live_macro.rs::reso_link_makes_the_resonance_follow_the_cutoff`,
  `sidou-tests/tests/app/live_macro.rs::pwm_link_makes_the_pwm_depth_follow_the_width`,
  `sidou-tests/tests/app/lab_view.rs::macro_turns_the_links_and_k3_k4_are_dark`,
  `sidou-tests/tests/app/lab_view.rs::a_link_lights_the_card_it_moves`,
  `emu-gate/tests/lab.rs::lab_filter_mod_and_fx_draw_within_their_budgets` (MACRO's frames).
  *Device-only:* by ear: whether the link's range is musical (the squelch, the resonance never
  whistling where it should not), and the graphs readable at arm's length.

Notes:

## 25. Knob curves (1.1)

What it decides: whether each curved knob feels even by ear (docs/reference/controls.md "Knob
curves"). Set up as section 22; a chord held while turning, one detent at a time, up then down.

- [ ] **LAB OSC PWM and PWM HZ:** PULSE at 50 %, PWM up from off through its 32 detents; then PWM
  HZ from its bottom (`.05`) to its top (`13`).
  *Emulator:* `sidou-tests/tests/app/knob_curves.rs` (the walks, the detents, the ratios, the
  first detent off the walk, the CCs), `sidou-tests/tests/app/lab_view.rs::each_card_turns_its_parameter_by_its_detent`.
  *Device-only:* by ear: PWM's first detents just audible and no detent near the top doing nothing;
  PWM HZ's detents sounding like even steps of speed, none jumping.
- [ ] **LAB MOD VIB HZ, DEPTH and DELAY:** DEPTH up from 0 through its detents, VIB HZ through its
  24, DELAY through its 24 on repeated notes.
  *Emulator:* as above.
  *Device-only:* by ear: DEPTH's first detent (+-4.7 cents) a just-audible shimmer and every detent
  after it an even step; DELAY's steps even in feel.
- [ ] **An effect slot's controls (EDIT + K1-K4, LAB FX):** for each algorithm, each control from 0
  to 127 a detent at a time, and Mix from 0.
  *Emulator:* `sidou-tests/tests/app/knob_curves.rs::each_slot_control_takes_the_most_even_shape_for_its_ear`
  (each control's measure rendered through the slots, its steps within 0.5x..2x of the median),
  `sidou-tests/tests/app/fx_live.rs`, `sidou-tests/tests/app/chip_view.rs::the_knobs_turn_the_focused_slot`.
  *Device-only:* by ear, especially the ones the measure could not settle: OVERDRIVE's and FUZZ's
  drive (linear as before: does FUZZ's clip still arrive all at once near a third of the travel?
  is OVERDRIVE's lower half too quiet a change?), CRUSH's rate (fine near the top), DELAY's
  feedback and every Mix (even steps of dB: does Mix's first detent, -28 dB, start low enough?).

Notes:

## 26. LAB's song bridge (1.5)

What it decides: whether a sound sent from LAB sounds in the song as it did in LAB, and whether
the gestures read and feel right (docs/reference/controls.md "Send a LAB sound to the song").
Set up as section 22, a song loaded (SIDOU LOOP or one of your own).

- [ ] **Send while the song plays:** PLAY, then in LAB a few cards turned on a plain sound (SAW
  LEAD), **REC**: the toast `SENT AS I06 SAW LEAD`; SEQ to INSTR and PRESETS to that instrument,
  then a note in TRACKER with it, or the keys on INSTR.
  *Emulator:* `emu-gate/tests/lab.rs::rec_in_lab_sends_to_the_playing_song_and_env_on_instr_pulls_it_back`
  (the bytes through the audio interrupt's queue, the chunk, the song playing on),
  `sidou-tests/tests/app/lab_song.rs::the_sent_instrument_plays_the_lab_sounds_registers`,
  `sidou-tests/tests/gt2_match.rs::lab_sounds_sent_to_the_song_match` (GoatTracker's player plays
  it as SIDOU's), the ergo journey `send`.
  *Device-only:* by ear: the instrument in the song against the sound in LAB, on the same chip
  model (only the hard restart's click at each note should differ); no glitch in the playing song
  while the bytes go in.
- [ ] **The approximate fields:** PWM BASS (its PWM), then a sound with the filter LFO (FILTER K4 at
  8), then the vibrato (MOD: 6 Hz, DEPTH half way), each sent and played in the song; the hint line
  says `APPROX: PWM`, `APPROX: FILTER LFO`, `APPROX: VIB`.
  *Emulator:* `sidou-tests/tests/app/lab_song.rs::the_pwm_loop_and_the_vibrato_play_within_their_tolerances`.
  *Device-only:* by ear: whether the PWM loop, restarting at each note, and the vibrato's nearest
  speed (6.9 Hz for 6.0) are close enough; whether the filter taking over the song's filter is
  what you expect (the hint line said so).
- [ ] **EDIT + REC over INSTR's instrument:** EDIT + REC asks `REPLACE I06? EDIT+REC`; again
  replaces it; the song plays the new sound where it played the old one.
  *Emulator:* `sidou-tests/tests/app/song_pages.rs::rec_in_lab_sends_and_edit_rec_asks_before_it_replaces`,
  `sidou-tests/tests/app/song_pages.rs::a_send_over_a_sent_instrument_writes_in_its_rows_and_blanks_the_rest`.
  *Device-only:* whether the question is seen in time and 4 s is long enough to answer it.
- [ ] **Pull back:** on INSTR, ENV (or LFO, FX) on the instrument sent: LAB opens at ENVELOPE with
  the sound exactly (`PULLED I06`); EDIT + LFO gives back the sound before; then a kit drum
  (instrument 01 of a NEW song): `I01 ~DRUM`.
  *Emulator:* `emu-gate/tests/lab.rs::rec_in_lab_sends_to_the_playing_song_and_env_on_instr_pulls_it_back`,
  `sidou-tests/tests/app/song_pages.rs::env_lfo_and_fx_on_instr_pull_the_instrument_into_lab`.
  *Device-only:* the feel of ENV on INSTR opening LAB; whether a pulled kit drum is a useful start.
- [ ] **GoatTracker:** the song saved, opened in GoatTracker 2 on a computer: the instrument sent
  plays there as on the FM-1 (the `LBnn` chunks are ignored).
  *Emulator:* `sidou-tests/tests/app/lab_song.rs::the_chunk_survives_a_save_and_goattracker_skips_it`.
  *Device-only:* the file in GoatTracker itself.

## 27. Lift a sound from a .sid (1.5)

What it decides: whether a lifted instrument sounds like the tune's voice by ear, on HVSC tunes
the emulator never played (docs/manual/sid-tunes.md "5. Lift a sound"). Set up: a few tunes on
the store (section 20): the starter pack's The Consultant, and three or four HVSC tunes of
different players (a GoatTracker tune, a Rob Hubbard or Martin Galway lead with vibrato, a
JCH or DMC tune with pulse sweeps, one with a filtered bass), and a song of your own, saved.

- [ ] **Lift:** play a tune, solo a voice (GLO + key 4-6), REC: the lane says ARM, then REC as a
  note starts; the toast names it (`LIFTED: ...`, or `LIFTED, VIBRATO DROPPED`).
  *Emulator:* `emu-gate/tests/psid_views.rs::a_voice_lifted_from_a_tune_is_imported_into_the_song`,
  `sidou-tests/tests/psid_lift.rs` (the capture and conversion; The Consultant's six notes played
  back register for register, the vibrato within half of the held note's difference).
  *Device-only:* none for the steps; the audio never dips while the note is taken.
- [ ] **Import and play:** stop the tune, HOME out of SIDS: the song comes back and LIFTED SOUND is
  offered; SAVE: `IMPORTED AS I0x`. In TRACKER, play the instrument at the note the tune used and
  an octave either side.
  *Emulator:* as above (the instrument's bytes and wave rows in the song).
  *Device-only:* by ear, against the tune's voice soloed: the attack, the waveform changes, the
  pulse sweep and the vibrato's speed and depth; note what differs (a filter that took over the
  song's, a vibrato dropped, a drum's noise pitch).
- [ ] **Refusals:** a digi tune (a sampled drum): `DIGI: CAN'T LIFT`. REC with no voice soloed:
  `LIFT: SOLO ONE VOICE`.
  *Emulator:* `sidou-tests/tests/psid_lift.rs::a_digi_burst_is_refused`,
  `sidou-tests/tests/app/sid_view.rs::rec_on_now_playing_lifts_the_soloed_voice`.
  *Device-only:* whether a real digi tune trips it (eight writes a call to one register).

## 28. Chords and time-shared drums from DICE (1.5)

What it decides: whether DICE's chords and its time-shared drums and bass sound like C64 music
on the chip, and whether setting a lane's role reads and feels right (docs/reference/controls.md
"Roles"). A NEW song, STEP.

- [ ] **The role:** hold EDIT, then SCL, and turn PRESETS: the frame says `PRE ROLE` and the role
  beside the root, the popup says it large; letting go rolls nothing.
  *Emulator:* `emu-gate/tests/first_use.rs::a_bass_and_drums_lane_rolls_the_kit_and_the_bass_time_shared`,
  `sidou-tests/tests/app/song_gen_roles.rs::presets_with_edit_and_scl_held_sets_the_lanes_role_into_the_scene_chunk`,
  goldens `step-dice` and `tracker-dice`, the ergo journey `roles`.
  *Device-only:* whether the small `PRE ROLE` line is found, and the PRESETS turn feels like part
  of the chord.
- [ ] **BASS+DRUMS:** V1 BASS+DRUMS, PRESETS (outside the chord) to the BASS instrument 02, range
  BASS (K2), EDIT + SCL, PLAY; then each rhythm (K4) in turn.
  *Emulator:* `emu-gate/tests/first_use.rs::a_bass_and_drums_lane_rolls_the_kit_and_the_bass_time_shared`,
  `sidou-tests/tests/app/song_gen_roles.rs::dice_on_a_bass_and_drums_lane_puts_the_kit_on_the_strong_beats_and_the_bass_between`
  (GoatTracker's player plays it as SIDOU's, call for call).
  *Device-only:* by ear, on both chip models: does one voice switching between the kick, the snare
  and the bass sound like C64 music, or does the bass's cut at each drum hit jar; the kit's hard
  restarts against the bass's.
- [ ] **Chords:** V2 ARP, its instrument 01 (PULSE), scale MAJOR (K3), EDIT + SCL (`NEW CHORDS`),
  PLAY with the BASS+DRUMS lane; SEQ to TRACKER, ALGORITHM to V2: the command line names each row's
  chord (`CHORD A MIN`). A second roll writes no more table rows (TABLE: the 17 chord rows once).
  *Emulator:* `emu-gate/tests/first_use.rs::an_arp_lane_rolls_chords_in_tracker`,
  `sidou-tests/tests/app/song_gen_roles.rs::goattrackers_player_plays_a_chord_lane_as_sidous_does`,
  `sidou-tests/tests/app/song_gen_roles.rs::dice_on_an_arp_lane_writes_chords_in_the_scale_and_the_programs_once`.
  *Device-only:* by ear: the 50 Hz chords (a frame a tone) on the 6581 and the 8580; whether the
  chords chosen fit the scale musically; the same song in GoatTracker 2 on a computer.

Notes:

## Result

| Section | Pass / fail | Notes |
|---|---|---|
| 0 Recovery ready | | |
| 1 Install | | |
| 2 Interrupted and refused installs | | |
| 3 Boot | | |
| 4 Keys and LEDs | | |
| 5 Buttons, encoders, MASTER | | |
| 6 LCD | | |
| 7 TRS MIDI | | |
| 7.1 Settings, SAVE and STORE | | |
| 8 USB MIDI | | |
| 9 Reinstall, stock and back | | |
| 10 Guard, watchdog, resets | | |
| 11 Measurements | | |
| 12 Flash timing spike | | |
| 13 Songs on the store | | |
| 14 Make a song on the device | | |
| 15 SPREAD and the soft limiter | | |
| 16 The drum kit and the digi drums | | |
| 17 Code in Felucca's debug area | | |
| 18 Generative play in STEP | | |
| 19 USB audio out | | |
| 20 .sid tunes over SysEx PUT | | |
| 21 .sid playback and the 6502's meter | | |
| 22 LAB | | |
| 23 Auditions before writing | | |
| 24 LAB 1.1 | | |
| 25 Knob curves | | |
| 26 LAB's song bridge | | |
| 27 Lift a sound from a .sid | | |
| 28 Chords and time-shared drums from DICE | | |
