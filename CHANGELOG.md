<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- **The emulator is fm1-emu `93cb7ee3`** (was `c1c045fc`), for the emulator gate and the browser
  demo: it models the watchdog and the reset through P33 (fm1-emu #92, our report #65). The
  boot-guard tests now die and reset as the FM-1 does, RAM kept: two crashes, each reset by the
  fault handler, and the third boot asks for UBOOT; the running firmware feeds the watchdog; and,
  with the checklist's hang build (`SIDOU_FM1_DEATH_BUILD`), a hang resets through the watchdog
  8 s after the last feed. The update test checks the reset after its record. The budgets are
  unchanged, instruction for instruction.

### Fixed
- The Songs page waits and sends again when the FM-1 answers a transfer with NAK BUSY, instead
  of giving up: it shows that the FM-1 is tidying its store, waits 1, 2, 4 and then 5 seconds
  between tries for about a minute, and sends the whole PUT again. Other refusals still stop the
  transfer. `sidou-sysex` only writes `.syx` files, so it has no sender to change.

## [1.0.0] - 2026-10-29

### Added
- **LAB, a view to shape one sound and keep it**. Five pages, OSC, ENVELOPE,
  FILTER, MOD and FX, each with four cards on K1 to K4 and a drawing of what they do (the pulse's
  width, the envelope, the filter's curve, the modulation, the effect slot); ALGORITHM is the
  page's choice (the waveform, the envelope mode, the filter mode, the arpeggio, the slot's
  algorithm). It opens from the menu (after SONG) and from LIVE with ENV, LFO or FX; HOME goes back
  to LIVE. A slot's algorithm OFF on the FX page bypasses it. SAVE held with a knob learns that
  card's parameter for MIDI; every page has a help card
  ([shape a sound in LAB](docs/manual/lab.md)).
- **Keep it, name it, undo it.** SAVE keeps the sound as one of sixteen **USER sounds** (`LAB 01`
  and on, in the lowest free slot, or in place for a USER sound); EDIT + SAVE opens a name page
  (the keys type A to Z and a space). Tapping EDIT compares the sound as kept (A) with the edit
  (B); EDIT + LFO undoes, sixteen steps; EDIT + PRESETS goes back to the sound as kept. EDIT + ARP
  varies the page and EDIT + SCL rolls the voice, and neither result is ever silent (an estimate of
  a C4 held for 500 ms) or the same as the one before.
- **USER sounds in PRESETS.** PRESETS clockwise from DICE plays the USER sounds (`U01` to `U16`,
  empty slots skipped); a USER sound loads as a preset, with its three effect slots and its name,
  and its edits come back when it is loaded again. A MIDI program change knows the built-ins only.
- **The store and MIDI:** the sixteen sounds are one private store file (`USER SOUNDS`, type 4,
  rewritten whole a keep at a time; a power cut leaves the old file or the new; SAVE AS and SysEx
  refuse its name and no list shows it) and the unsaved edit is one more key. The settings record
  is schema 8; old records load as before. The PWM depth, PWM rate and fine tune are MIDI
  parameters (NRPN, no CC).
- **DICE's settings on STEP:** with EDIT + SCL held, K1 sets the density (SPARSE, LIGHT, MEDIUM,
  BUSY, FULL: how many steps get a note) and K3 the scale (MIN PENT, MAJ PENT, MINOR, MAJOR,
  DORIAN, CHROMATIC); the frame shows both at 2x while SCL is held. Letting go of SCL with no
  knob turned rolls, as before, now at the release; MUTATE keeps to the chosen scale. The
  settings are saved with the song in SIDOU's `DICE` chunk (GoatTracker skips it), and a song
  with none rolls exactly as before (MEDIUM, MIN PENT). STEP's help card gained SCL+K1 and
  SCL+K3 where PLAY and KEYS 17+ were. EDIT + SCL is the same chord whichever finger lands
  first: SCL a few milliseconds before EDIT no longer selects the next effect slot and leaves K1
  to the EDIT layer's effect knob (SCL alone on STEP selects the slot when it is let go).
- **The ARP helper's waveform:** PRESETS in TABLE's ARP helper chooses the waveform the rows
  carry (TRI, SAW, PULSE, NOISE and the combined ones), shown on the helper's page; the rows stay
  plain GoatTracker 2 wavetable rows, and the host test plays each waveform in GoatTracker's own
  player.
- **Store health on STATUS:** the bottom row shows `STORE ERR n` (flash errors since boot) and, when
  the store retired sectors for the session, `RETIRED n ERR n`; nothing while it is healthy.

### Changed
- **ENV, LFO and FX open LAB.** On LIVE, ENV
  toggled the chip model and cycled the effect, FX stepped the next algorithm, and LFO did
  nothing. They open LAB at ENVELOPE, MOD and FX now, and do nothing outside LIVE and LAB. The chip model is ALGORITHM on
  LIVE's TONE page (or CC 25), the next algorithm is ALGORITHM on a slot's page, and an effect is
  bypassed by turning its algorithm to OFF. The first-run tour's `PRESS ENV` now opens LAB.
- **Readable small text on TABLE and STEP:** TABLE's meaning line is at 2x for every table (the
  PULSE, FILTER and SPEED words are shorter), and STEP's step ruler and each lane's note,
  instrument and gate line (`C-3 I01 STAC`) are at 2x. The greyed chip B lane (`CHIP B IN M5`)
  made room for it.

### Fixed
- **A failed background settings save is retried:** a program error after the store took the
  settings (or the unsaved LAB edit) was never noticed; both are now written again at the next
  debounce, and the second failure in a row shows `STORE ERROR: NOT SAVED` once.
- **Dense `.sid` players write on time:** a block of the audio now renders in up to 23 parts, not
  15, so a player that writes 23-26 times within 16-19 samples (Thomkat, Kill Me, The Last Ninja)
  no longer has writes land at the end of their 128-sample block (up to 2.9 ms late). LATE on
  STATUS stays at 0 for them; Sound Test, with a write about every sample for a whole frame, still
  shows some. No memory and no audible change elsewhere.
- **A sound edit no longer asks about the song before a tune:** a PRESETS change or a knob on
  LIVE or in LAB made SIDS ask `SONG NOT SAVED` before playing a tune, even on the built-in loop,
  and its discard lost the sound too (the song's file put its own back when the tune ended). Only
  the song's own edits (patterns, tables, instruments, its settings) ask now, and the keys' sound,
  effect slots, MIDI and chip A's setup stay as they are across the tune.

## [0.5.0] - 2026-10-25

### Added
- **`.sid` playback:** PSID tunes (one or two SID chips, PAL or NTSC, up to 65,536 bytes) play on
  the FM-1 from the store. SIDOU's own 6502 runs a frame ahead of the audio in the main loop and
  stamps each SID write with its cycle; the audio interrupt places it at that cycle, so the timing
  is exact (and the tune never makes the audio wait). OSC3 and ENV3 reads see the chip's real
  voices (a shadow of them), as on a C64. Every GoatTracker 2 example and SIDOU song plays sample for
  sample the same as the host's reference, and the 6502 itself matches Tom Harte's and Klaus
  Dormann's tests. RSID tunes and MUS files are refused by name, never played wrongly
  ([docs/reference/psid.md](docs/reference/psid.md)).
- **SIDS and NOW PLAYING:** the menu's SIDS (after SONG) lists the tunes on the store by name,
  eight a page; ALGORITHM or SAVE plays one and opens NOW PLAYING: title, author, year, the song
  (ALGORITHM changes it), the time, PAL or NTSC, each chip's model and the three voices with envelope
  bars. PLAY stops, EDIT + PLAY pauses; GLO mutes and solos the voices on the chip; MIDI start, stop
  (pauses) and continue follow. A tune takes the song's memory: with unsaved changes SIDS asks first
  (SAVE, discard or cancel), and the song comes back when the tune ends. The keys play chip B under a
  1SID tune.
- **SETTINGS → SID MODEL:** AUTO plays each tune on the chip model its file names, KEYS on the chip
  the keys play, with its profile and character.
- **`.sid` files on the store, over SysEx:** a tune goes in as a SysEx `PUT` of type 2, checked at its
  first data message before anything is written, over USB-MIDI or the TRS jack. `sidou-sysex sid`
  makes the `.syx`, with the same parser as the FM-1 and the same refusals
  ([the manual](docs/manual/sid-tunes.md)); `--pack` makes one upload of a whole set.
- **A starter pack:** eleven openly licensed tunes (five of SIDOU's songs and six of GoatTracker's
  example songs), in `assets/starter`, each with a note on its licence and source
  ([LICENSING.md](LICENSING.md#the-starter-pack)).
- **STATUS shows the 6502:** while a tune is loaded, STATUS's rows are the 6502's state, cycles a
  second, its share of the CPU and the ticks a thousand cycles took (timed with TIMER4), overruns,
  late writes, the voice shadow and a third SID's dropped writes. The flash day's checklist and its
  decision rule are in [docs/hardware-test.md](docs/hardware-test.md), section 21.

### Changed
- **STATUS is readable:** it draws at 2× on three pages that SELECT turns (`1/3` in the title): the
  knobs (the 6502's rows while a `.sid` tune is loaded), the chip, preset, octave and notes, the
  effect slots. Its help card names SELECT.
- MENU: SIDS comes after SONG, so PRE and OCTAVE each move down one item.
- The store holds `.sid` tunes beside songs (file type 2); a tune and the song share the song's
  memory, so the song views say `SID PLAYING: STOP TO EDIT` while one plays.

## [0.4.0] - 2026-10-25

### Added
- **Songs to MIDI, on the site:** `midi.html` turns a GoatTracker 2 `.sng` (dropped or chosen, or
  one of the four showcase songs) into a Standard MIDI File in the browser: a track and a MIDI
  channel a voice (six for a GoatTracker Stereo song), one pass to the orderlist's loop point,
  the notes where GoatTracker's player puts them: speed, funktempo, `F` and `E` commands,
  repeats, transposes, key offs and gate timers, legato and tone portamento as tied notes. One
  constant tempo and a tick that is a twentieth of a player call, so every start and end lands on
  a whole tick. The notes are the rows' (wavetable arpeggios, vibrato and slides are not
  followed), which the page says. `web/songmidi.test.mjs` compares every note's start, end and pitch
  with GoatTracker 2.77's packed player on the showcase songs, SIDOU LOOP and GoatTracker's
  examples (19,853 runs of the gate; fixtures made by `host/crates/gt2-ref/tests/midi_walk.rs`).
- **Generative play in STEP:** a held step plus ALGORITHM sets the step's **chance** to play on each
  pass (100 % down to 0, a lock in the LOCK chunk, parameter 9; GoatTracker plays the step always);
  the roll is seeded and made by the main loop a row ahead, so the audio interrupt only drops the
  note of a step that lost it. **EDIT + ARP is MUTATE** (two small changes to the lane's pattern:
  a note nudged within the scale or taken away, or a ghost step added) and **EDIT + SCL is DICE**
  (a new pattern for the lane, minor pentatonic notes on the eighth grid, or on the sixteenths for
  a SCNE drum lane): ordinary cell edits, plain GoatTracker 2 data.
- **The button LEDs show more, in two levels:** the hardware's dim glow means set or available,
  full light active now. ARP is dim while on and flashes on each arpeggio step (once a cycle at
  1-2 frames a step); PLAY is lit on each beat and dim between while the song plays; REC is dim
  while armed, blinks through the count-in and is lit while recording; SAVE blinks while a
  question waits for it. With GLO held or latched, note keys 1-6 show the voices (lit sounding,
  dark muted, blinking solo), and in STEP the 16 step keys show the page (lit a note, dim an
  empty step) with the playhead's key flicking as it moves.
- **An ARP helper on TABLE's WAVE table:** press ARP on an arpeggio's first row (or a blank one)
  and set its steps as semitone offsets (SELECT, K1 octave, K2 semitone), the number of steps
  (ALGORITHM), the speed (K3) and the loop step or HOLD (K4); SAVE writes plain GoatTracker 2
  wavetable rows, and rows written by it or by hand in the same shape open as the arpeggio. No new
  engine; GoatTracker's own packed player plays the rows as the helper says (a host test).
- **SETTINGS → RESET:** one reset for everything that is not a song. ALGORITHM clockwise asks
  (RESET?), SAVE alone does it: every preset's edits and all the saved settings go back to
  factory and are stored at once (toast RESET AND SAVED). Songs, USER chip profiles and MIDI learn mappings stay;
  HOME or moving away cancels with nothing written. Before, the only whole reset was a STORE
  format, which erased the songs too.
- **A SID drum kit:** a NEW song holds eight drums on the chip's own voices after its lead and
  bass, as instruments 03-0A: KICK, SNARE, CL HAT, OP HAT, CLAP, TOM LO, TOM HI and RIM. They
  are plain GoatTracker 2 instruments and table rows (a falling triangle for the kick and the
  toms, a tonal attack and noise for the snare, short and long noise for the hats, three noise
  bursts for the clap), so a song made with them plays the same in GoatTracker; each was tuned
  by rendering it on both chip models, and the kick keeps its weight on the 8580. Each drum
  sounds one frame (20 ms) after its row, every hit alike (its first frame resets the
  oscillator), with an 8 ms attack, and fades the same held or staccato (at tempo 5 and up);
  its instrument's SR is `00` and a wavetable `F6` row (command 6, sustain/release) sets its
  release in its second or third frame, which keeps the chip's ADSR delay bug from holding the
  hit back 33 ms. STEP's and
  TRACKER's PRESETS popup names the instrument (`03 KICK`), so a lane's drum is picked by name.
  Decision record 0009.
- **Digi drums:** short 4-bit samples (KICK, SNARE, CLAP, HAT) played through chip A's output
  as a C64 tune plays them through the 6581's volume register, at about 8.8 kHz, from the
  sample of their note, one at a time. A new `DIGI` chunk in SIDOU's trailer marks the
  instruments that play them; GoatTracker skips it and plays the instruments' own GT2 content
  instead, so a song with digi drums still opens and plays there. A NEW song has them as
  instruments 0B-0E (`DG KICK`, `DG SNARE`, `DG CLAP`, `DG HAT`), copies of the chip kit's drums;
  while a digi drum plays, its voice's own sound is muted. They sound the same on either chip
  model (an 8580 plays them as one with the "digi boost"). The samples are synthesized by
  SIDOU's table generator, no recorded material.
- **Record SIDOU over the USB cable:** the FM-1 is also a class-compliant USB audio input (UAC1,
  no driver on macOS, Windows, Linux or iPad), beside its USB-MIDI port in one device. A DAW
  records what the line out plays (after the effects, the limiter and MASTER), 16-bit stereo at
  44.1 kHz, or 48 kHz resampled, the host's choice. It is Felucca's USB audio input from the
  same `usb.c`, on as Felucca's firmware has it. The TIMER5 interrupt takes each block from the
  I2S buffer and feeds it, so the audio interrupt and its SRAM code are unchanged; recording costs
  the interrupts about 42 instructions a sample at 44.1 kHz and 335 at 48 (the resampler), and
  nothing while no host records. The installer's updates and SysEx work as before, still over the
  MIDI port.

### Changed
- **The profile's word everywhere:** STATUS names the chip as its model and the profile's word
  (`6581 CLASSIC`, `8580 CLEAN`; both chips on a two-chip setup) and the PROFILE popup says the
  same at 2x, where they said `6581 R4AR`; the ergonomics readability table counts the new names
  as read while playing.
- **The emulator is fm1-emu `c1c045fc`** (was `db8d472e`), for the emulator gate and the browser
  demo: it runs the decrementing block load `{r1, r3} = [-r1]` (fm1-emu #67, our report #64) and
  `psid_load` checks its bounds (#68, our report #63), so the gate's own stand-in for that load,
  `step_block_load_dec`, is gone and with it the `emulated_block_loads` count in the gate's
  reports.
- **About 25 KB of flash and 5.4 KB of the RAM region won back, nothing heard
  differently:** the filter's cutoff codes are delta-coded (11,984 bytes), the combined
  waveforms' blocks overlap and alike rows are stored once (5,976), the noise tables are built
  at boot into POOL (2,980 of flash, 3,140 of the RAM region), the digi drums' codes are two a
  byte (2,412 of both), and the store is built at `-Oz` (1,568). `song_ctl.c` is built at `-Oz`
  (112 bytes of RAMTEXT), so the code that runs from SRAM stays in its 48 KB, below
  Felucca's 8 KB debug area, which is kept free until a device check shows it is the app's
  ([hardware-test.md section 17](docs/hardware-test.md#17-code-in-feluccas-debug-area-v040)).
  The digi drums cost the audio interrupt less than before (9.47 instructions a sample, was
  10.42). [The study](docs/plans/2026-10-24-flash-space.md) has the numbers.

### Fixed
- **A forced RESET says it reset.** When RESET's save had to be forced (the device was never
  quiet), the toast said SAVED (GAPS); it now says RESET AND SAVED (GAPS).
- **A NEW song's bass differs from its lead.** Both were the same pulse at the same pitch. The
  bass (instrument 02) now plays an octave down (WAVE right column `74`, relative note -12) with
  a thinner pulse, a quarter width (`$400`, its own PULSE row 03 after the kit's); the lead keeps
  the square. Songs saved from NEW before keep their sound.
- **A NEW song's lead and bass sound at their notes.** Their wavetable rows kept the frequency
  unchanged (`$80` on the right, GoatTracker's "keep frequency"), so no note ever set it: STEP's
  and TRACKER's auditions and PLAY gave only a faint thump on a voice stuck at 0 Hz (since NEW
  came in, 0.2.0). The rows now play the note (relative note `00`) and end with `FF 00`
  (the table stops, so the pattern's slides and vibrato still move the pitch). Songs saved
  from NEW before keep their rows: set the lead's and bass's WAVE right column to `00` in TABLE.
- **A NEW song's lead and bass start on their row.** Their attack 0 met the chip's ADSR delay
  bug one time in nine (the envelope's rate counter, compared with the decay's rate for a cycle
  as the gate goes on, ran past attack 0's and waited out its wrap): the note then started
  33 ms late, a flam against the drums, as GoatTracker plays it on reSIDfp too. Their attack is
  now 1 (8 ms to the top: AD `19` for the lead, `12` for the bass). Songs saved from NEW
  before keep `09` and `02`: set the attack to 1 in INSTR for the same fix.
- **The first tap after picking an instrument auditions it in full.** An instrument the song
  does not use yet waits a few calls for the player to take its tables in; a step key let go
  meanwhile released the note right after its first call, so the first tap after PRESETS (or any
  audition of a new instrument) gave a click of its first wave step instead of the instrument,
  and the drums all sounded alike. The note now sounds as long as the key was held.
- **The MASTER popup stays away while the pot is untouched.** The pot's reading wanders by
  more than the 4 counts (0.4 % of the travel) that moved the volume, so the VOLUME popup came
  up on its own. The reading is now the mean of four conversions, the popup needs a turn of
  16 counts (1.6 %) from where the pot rested, and the last 8 counts at either end read as
  that end (0 % and 100 % hold through the noise). The volume still follows a small turn,
  without the popup; once a turn shows it, the popup follows the turn to its last value.
- **PLAY is quiet until the song's first note.** The first PLAY after power-on gave a thud
  (a DC bump that died away over about 150 ms) before the music: chip B, which renders only
  while in use, played the power-on release of its envelopes only when the keys moved to it, at
  their volume and filter (since chip B came in, 0.2.0). Chip B's power-on now plays out where
  the chip is made, silently.

## [0.3.1] - 2026-10-23

### Fixed
- **The first boot after another firmware no longer detours through UBOOT.** Installed over
  Felucca 1.6.1, SIDOU's first boot went to the chip's USB boot (WL80UBOOT1.00) for 45 s before
  starting, and the installer said "did not come back". Felucca's reset-surviving RAM lies over
  SIDOU's boot-loop guard (its crash record is 4 bytes longer), and its words read as two failed
  boots. The guard now carries a check word, so only counts SIDOU wrote can send a boot to UBOOT.
  On the unit, Felucca → SIDOU is back in under a second.
- **A store full of another firmware's sectors no longer holds up the boot:** the boot does at
  most 100 ms of flash work and starts USB; reclaiming the rest goes on in the background.

### Changed
- The browser demo's site has a [videos page](https://dsablic.github.io/sidou/videos.html) that
  plays the how-to videos in the browser.

## [0.3.0] - 2026-10-23

### Added
- **Chord arpeggios on LIVE:** ARP switches them on; a chord held is one voice stepping through
  its notes a frame at a time (50 Hz), the C64's signature sound, its gate held so only the
  pitch moves, and the other two voices free. ARP held with K1, K2 and K3 sets the rate (1 to 8
  frames, or a 32nd, 16th or 8th of a MIDI clock), the order (up, down, up-down, as played) and
  the span (1 or 2 octaves); they are kept with the preset's edits. MIDI notes arpeggiate too,
  and CC 27..30 switch it and set the rate, order and span. Row 20 shows `ARP 1F UP 1OCT`; the
  recipe `chord-arpeggio` walks through it.

### Fixed
- **MIDI clock across flash writes:** the clocks that arrive while a flash erase or program
  masks the interrupts are stamped late; they now stay out of the clock follower's fit, and the
  fit keeps the timer's own time line across the audio blocks a hold loses, so a USB upload
  while the song follows a clock no longer throws the song off the beat (review 4.4).
- **USB-MIDI in no longer drops input in a long main-loop pass:** the device NAKs the host
  while SIDOU's MIDI-in ring is nearly full, so a long SysEx dump waits instead of being cut,
  which released every note (review 4.5). An update's first step, which runs in place of the
  main loop, empties that ring itself, so the hold never stalls the installer's answers.
- **CLOCK OUT's song position no longer cuts a SysEx reply:** it waits for the reply's end
  (review 4.2).
- **A MIDI continue (FB) with no song position, for a song never played, starts it** on the
  clock; it used to wait and hijack the next PLAY (review 4.3).
- **A song file's parameter locks are checked when it is read:** a LOCK chunk with a value out
  of range or records out of order is dropped (`SNG: LOCKS DROPPED`); a filter-mode lock of 8
  turned voice 3 off (review 3.6).
- **No `idle` while a USB host is attached, also in the register ring's wait** (review 2.4).
- **LIVE's meters never lose a block's peak** (review 2.5).

### Changed
- **The emulator is fm1-emu `db8d472e`** (0.2.0 and later; was `7ca5b96c`), for the emulator
  gate and the browser demo. Every count the gate measures is the same at both. The browser
  demo now keeps its flash in the browser, one save per build, with Save now, Export…, Import…
  and a reset, so the first-run tour and anything saved there survive a reload.

### Fixed
- **A SysEx PUT or DEL is no longer refused BUSY while the device sounds.** On the unit, with a
  chord held on LIVE (or a song playing, or MIDI notes held), every PUT was refused BUSY although
  INQ reported a full pool: a record the store was writing (a settings save, or a collection's
  copy that quiet maintenance had started) waited for quiet, and the PUT gave up on it. Now the
  transfer waits for that record in its own short hold (at most two seconds), and a collection
  under way waits for the file; nothing is erased meanwhile.

## [0.2.0] - 2026-10-22

### Added
- **M2d complete: STEP, TRACKER, the song pages and REC** (Tasks 1-7). Make a song on the FM-1:
  SEQ steps STEP, TRACKER, ORDER, INSTR and TABLE; PLAY plays and stops it; GLO held and a key
  mutes or solos a voice; SAVE saves it, and it opens in GoatTracker. The song views' controls
  are in [controls.md](docs/reference/controls.md#song-views), their costs in
  [the art README](docs/art/README.md#tracker) and the plan's
  [status](https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2d-step-tracker.md#status-m2d-complete-tasks-1-7-2026-10-22).
  The image is 493,840 bytes against M2f's 512,000; the RAM region and POOL have 4,256 and
  4,264 bytes free (8,520 together, against the gate's 7,000; each region keeps 4,096). Nothing
  of M2d has run on a unit yet: [hardware-test.md section 14](docs/hardware-test.md#14-make-a-song-on-the-device-m2d)
  is the check, with the button ids of section 5.
- **REC records the keys and the filter knobs into the playing pattern** (M2d Task 6): with REC
  on and the song playing, a note key in TRACKER writes its note and the current instrument on
  the cursor's voice at the row the voice's player is on (NEAREST rounds a key in a row's second
  half to the next row; OFF keeps the row it falls in; COUNT-IN is NEAREST after one silent
  bar), and in STEP a step key writes the lane's note and, on a staccato lane, its key off.
  K1 and K2 become cutoff and resonance locks on the row they were turned in. The menu's REC
  item chooses the mode (not saved). TRACKER's cursor stays where it is while recording, and a
  key's time is not compensated for latency: both are open for the unit
  ([TODO](https://github.com/dsablic/sidou/blob/v0.5.0/docs/TODO.md)).
- Ergonomics: **DICE**, a place after NOISE PERC on PRESETS: each turn rolls a new sound (waveform,
  envelope, cutoff, resonance, filter, pulse width, now and then a vibrato) and leaves the effect
  slots alone; turning back and forth brings the last roll back. MIDI program changes stay 0..3.
- **STEP, the song made like a drum machine** (M2d Task 5): the 16 leftmost note keys are
  steps 1-16 of the selected voice's pattern (a tap toggles a note, with a key off after it on
  a staccato lane, or none on a legato one: ARP), SELECT picks the lane, ALGORITHM pages,
  PRESETS sets the lane's instrument or a held step's note. A **held step plus a knob** is a
  parameter lock for that row only (K1 cutoff, K2 resonance, K3 pulse width, with EDIT the
  character), and K4 writes GoatTracker's own `7XY` waveform command, which holds as it does
  there: two kinds, one meaning each, shown apart (`LOCK` yellow, `SET` cyan). SEQ now steps
  STEP, TRACKER, ORDER, INSTR, TABLE and round.
- Ergonomics: title cards play once per power-on by default (CARDS: OFF, ONCE, ALWAYS); the
  way back to LIVE never plays one unless ALWAYS. Settings byte 1 holds 0, 1 or 2 (an old
  record's ON reads as ONCE); no schema change.
- Ergonomics: hold HOME for a help card of the view that shows (LIVE, the menu and MIDI, CHIP,
  the song views, SCENE, and one for the rest); HOME now acts when it is let go, so a tap goes
  home as before and a hold of 600 ms opens the card. Any control closes the card and does
  nothing else.
- Ergonomics: a first-run tour after the full boot an install plays: TURN K1, PRESS ENV, HOLD
  HOME FOR HELP, each until you do it or 8 s; not after a boot HOME asked for, and not again.
- Ergonomics: SCENE's FIRE keeps a low flame in silence, the piece's name is toasted on entry,
  and ALGORITHM steps the piece like SELECT.
- Ergonomics: an effect algorithm picked from the panel (ENV, CHIP's ALGORITHM on a slot) starts
  from a musical setting per algorithm, so it is heard at once; MIDI's algorithm CC loads nothing.
- **ORDER, INSTR and TABLE song editors**, using the readable 2x high-contrast style and
  the shared inactive canvas. Edit order entries, restarts and the subtune, the nine
  instrument parameters and the GT2 name, and all four tables with their meanings spelled
  out. EDIT + SEQ on an instrument pointer opens its table. Edits go through the player's
  save-aware queue; value popups name the field being edited.
- The song audition keeps live state: a jam on an unused instrument, an omitted voice, a note
  outside the song's range or a table of another encoding neither restarts nor refuses; it is
  prepared in bounded main-loop slices and converted at a block boundary.
- Ergonomics pass, LIVE (ergo-live): K1 walks a generated cutoff table (64 detents, even in pitch,
  per chip model) and MASTER has an audio taper (-60 dB to 0 dB in even steps of dB) and reads in
  percent; LIVE's knob row says what K1-K4 turn (values of at most 3 characters, the page's
  name, the slot's controls on EDIT or SELECT's pages) and a knob that turns on LIVE or CHIP
  highlights its label instead of opening a popup; **PRESETS** (the big encoder) steps the preset
  in every view, each preset **keeps its edits** (a `*` after its name; EDIT + PRESETS puts it
  back) and MIDI out sends them after the program change; **SELECT** pages LIVE's knob row and
  **ALGORITHM** steps the chip's profile or the slot's algorithm, with no popup; **K3** is the
  sound's own control (the pulse width in 31 positions, the decay, or the sync ratio in
  semitones) and **K4** the release (learnable); **GLO** latches with a double tap, its layer
  shows on LIVE, the keys beyond the leftmost six play under GLO, and LIVE shows the song's play
  mark, tempo and the voices' badges. See [the ergo-live report](https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-18-ergonomics-pass.md#track-ergo-live)
  for the budgets. A toast now closes an open popup.
- M2e Task 5: [version-1 SysEx](docs/reference/sysex.md) for inquiry, streamed
  directory/file GET and PUT, deletion, GTI5 instruments with table relocation,
  and current-song dumps/imports. USB uses per-frame ACK/NAK; TRS accepts paced
  uploads without replies. Nonfinal upload DATA is a multiple of 16 bytes;
  the final packet may contain any 1–256 bytes. All sectors are claimed before
  the first PUT ACK, DATA ACKs wait for programming, and incomplete uploads
  do not replace existing files. No-window PUT/DEL uses explicit program holds
  with SAVE's mute/ramp, released between programs; transfers remain not quiet
  and never authorize maintenance erases. Window-enabled programs are hold-free,
  with hardware timing acceptance still pending section 12 of the checklist.
  Current-song END ACK follows completed, stopped loading; validation failures
  return a refusal instead of an early success. The host-only
  `sidou-sysex` packet library opens no devices and has no CLI. The generated
  MIDI chart includes SysEx commands; checklist 8c covers manual acceptance.
  USB-MIDI output works on hardware since the USB fix below (section 8.1 passed on
  2026-10-21).
- M2c song engine complete (Tasks 1-9): native GoatTracker `.sng` files with SIDOU's
  trailer, the host 6502 reference and register-matched C player, frame-exact interrupt
  playback with bounded live edits, parameter locks, SONG's store actions and About's
  original music. [The M2c status](https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2c-song-engine.md#m2c-status)
  records the remeasured costs, memory, image and coverage. The narrow RAM-region margin
  and the GoatTracker licence-audit findings remain open for the end-of-M2 review.
- **M2b complete** (Tasks 1–8): sliced preparation, two SIDs, keys on B
  during a song, both CHIP tabs, expert controls and off-by-default UI sounds. The
  [M2b status](https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2b-chip-b.md#m2b-status) records the integrated
  image (497,120 bytes), RAMTEXT (46,016), free RAM + POOL (9,852), chip B's marginal
  state (35,216) and two-chip ISR/screen measurements. TESTING documents the
  new coverage; hardware checklist 6.3–6.4 adds two-chip routing, voice-safe sounds,
  expert readability, continuous notes and persistence. Hardware acceptance remains open.
- **The character's expert layer** (M2b Task 6): fourteen independent analogue controls,
  each on NRPN 2/5..18 and available to MIDI learn. CHIP's final **EXPERT** SELECT stop has
  four directly mapped knobs per page, in two readable pairs, with names and unit-bearing
  values at 2×/3× and the theme's K1-K4 colours. The value popup names every expert too.
  Held notes continue through constant changes and sliced filter/DAC rebuilds; no expert
  edit resets the chip. A macro move reclaims only its related expert values.
  USER profiles save the raw overrides (schema 2), and settings save them and their learned
  CCs (schema 5, after M2c's schema-4 last-song filename); older records load with the
  profile/macro defaults. Built-in profile/model
  choices clear overrides, while USER picks recall them. Outgoing MIDI sends explicit
  expert edits after their macros without turning inherited defaults into overrides.
- **A theme for the screens' colours** (docs/art/README.md "Theme"): the views take their colours
  from roles defined once in `app/theme.c`. **HIGH CONTRAST** is the new default (black, white
  values, grey labels, dark grey for the inactive, light red, cyan and light green for voices
  1-3 and yellow, cyan, light green and light red for K1-K4, the same everywhere, the
  popup's bar included); **C64** keeps the old blue look. The menu's **THEME** item chooses, and
  the choice persists (settings schema 3: schema 1 and 2 records load HIGH CONTRAST).
- **TRACKER shows one voice at a time at 2× the font** with the other two as activity strips,
  and **ALGORITHM steps the voice** (1, 2, 3 and round); SELECT moves rows (EDIT: an 8-row page).
  REC enables note entry, key-off/rest/whole-row clear, instrument/command/data editing with
  the value popup, fixed-length row insertion/deletion and one-step undo/redo (EDIT + LFO).
  Edits are queued cell writes or index swaps; locks follow shifted rows, including undo.
- docs/hardware-test.md: a readability check of each redesigned screen on the panel, in both
  themes.
- M2a storage complete (Tasks 1-7; the hardware timing run remains open): a
  log-structured store in `[0x97000, 0xE0000)`, with per-sector guard bands,
  CRCs, power-loss recovery and an erase-ahead pool. Files up to 64 KiB claim
  space before writing and commit whole; torn-operation and faint-bit host
  tests complement the emulator's operation-boundary power cuts.
  [`The M2a status`](https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2a-storage.md#m2a-status) records
  current measured RAM, RAMTEXT, image size and the outstanding hardware checks.
- The flash timing spike (`-DSIDOU_FLASH_SPIKE=ON`, a test firmware, never
  released), confined to the store's last two sectors at 0xDE000; its checklist
  and result fields are in `docs/hardware-test.md` section 12. It has not run on
  a unit. The P25Q80H datasheet's provisional 3 ms program and 20 ms sector-erase
  maxima currently select held programs and erases, not write windows. The
  window path is exercised explicitly in fm1-emu; background saves wait for quiet,
  while an explicit save can force gaps after 2 s.
- Eight USER chip profiles (M2a Task 6). Each is a built-in base profile and the character's
  controls and expert overrides, kept on the store under its own key (`0x101`..`0x108`, apart from
  the settings record: [`docs/reference/profile-format.md`](docs/reference/profile-format.md)).
  On the CHIP view, ALGORITHM on the chip runs the five built-in profiles, then the filled
  USER slots (an empty one is skipped: **USER 4 EMPTY**); picking one is an ordinary profile
  change (the spare chip prepared, then swapped). **EDIT held + ALGORITHM** steps a save
  target, USER 1..8 or OFF, shown yellow in the CHIP frame (**SAVE TO USER 3**) without changing
  the chip; **SAVE** on the chip focus stores the tab's profile and controls into it (or back
  into the slot picked), asking **OVERWRITE USER 3?** first over a profile, and **SAVED TO USER 3**
  when the store commits. On a song's chip A it stores the song's setup. CC 85 and NRPN 2/0
  reach the slots above the built-in profiles (range 0..12; an empty slot does nothing).
- OCT- alone held about a second at power-on enters the chip's ROM boot mode (UBOOT), the
  first thing the firmware does, so a broken firmware or update path is recovered with
  `tools/fm1-unbrick-mac` or MvaveFM1Unbricker, without a Flipper.
- **Large text**, because the 8×8 font on the 1.3-inch panel was almost unreadable at the first
  hardware run: `grid_text_big` draws a string at 2 or 3 times the font from its own glyphs
  (pixel-doubled, each cell drawing its part, no new font data), and `grid_put_top` a cell no
  overlay shows over (`app/grid.c`, docs/art/README.md "Large text").
- **A value popup** over any view when the panel changes a parameter (K1-K4, ALGORITHM,
  MASTER): the name and value at 3 times the font with a bar for a continuous value, opening 3
  rows a frame, gone a second after the last change with the view under it back as it was.
  The names and values are the views' own, from one function (`app/views.c popup_text`).
  MIDI changes do not open it: one switch, `VIEWS_POPUP_MIDI` (`views_popup_midi`).
- docs/hardware-test.md: a readability check of the large text on the panel.

### Changed
- CHIP adopts T1's readable text roles: small control labels use `knob_text`, captions,
  tags and help use `hint`, while bars and large EXPERT names keep knob accents. Inactive
  controls stay dim. Tab rendering and navigation are unchanged; T2 remains separate.
- Expert settings use **schema 5**, retaining M2c Task 8's schema-4 last-song filename
  at offsets 108–123; v1–4 migrate without turning filename bytes into expert overrides.
  CHIP and the song views share their inactive drawing canvas to retain the unchanged
  RAM floors after integration. Song snapshots preserve raw following values and wide
  cutoff/pulse parameters in the existing 160-byte I/O buffer.
- **LIVE, TRACKER, CHIP and STATUS are drawn to be read at the panel's 23 mm**, after the
  screen-readability prototype the user chose (assets/prototypes/screen-readability.html).
  LIVE: the preset's name at 2×; three voice lanes (the voice's number at 2× in its colour, its
  scope 2 pixels thick, dark grey and flat while the voice is silent, the note at 2×); the
  knob row (CUTOFF, RESO, PULSE, PRESET: what K1-K4 turn, each value at 2× with a bar in the
  knob's colour); the filter's mode at 2×, the slots and the VU meters. Dropped from LIVE: the
  envelope bars, each voice's pulse width, the filter's response curve and each slot's Mix
  bar. TRACKER: 12 rows at 2× of one voice. CHIP: one section at a time under the tabs CHIP,
  DIST, MOD, FX, its choice and its four controls at 3× in the knobs' places. STATUS and ABOUT:
  the titles and key numbers larger.
- **The menu has 16 items** (THEME after ROWS, SONG before ABOUT, MIDI last), its title bar,
  values and selected line in the theme's colours; the toast, the popup and MIDI learn's
  frame follow the theme too.
- Settings persist: CARDS, KEYS, SOUNDS, TRACKER ROWS, the preset and its edits,
  the keys' chip model, base profile and character, effect slots, envelope mode
  and MIDI-learn bindings. A background save runs 5 s after the last persisted
  change, without a toast; MASTER, octave, held notes, controllers and the current
  view are not restored. The stored firmware version gives the full boot once
  per version (or when missing), then the short title card; HOME at power-on
  still forces the full sequence. Schema-1 settings load TRACKER ROWS as BAR.
- SAVE saves on a clean **release**, not on press; any encoder turn or other
  button pressed while held cancels that release's save. With CHIP's chip focus
  and a USER target/applied slot it saves the profile; in the song views it saves
  the song; elsewhere it saves settings. SAVE held + a knob opens MIDI learn on
  that parameter without moving it. A fresh SAVE forgets
  its binding; SAVE held + HOME in the frame asks to clear all, then a fresh
  SAVE confirms. HOME or 10 s cancels; outside the frame SAVE held + HOME consumes
  HOME. Explicit saves report completion and forced gaps, with the drive chunk
  when SOUNDS is on.
- STORE after CARDS shows free space outside the reserve pool or FOREIGN.
  Previous ChoralRoot/Felucca data is FOREIGN and is never automatically
  formatted. Only two clockwise ALGORITHM steps on STORE confirm erasing it;
  counter-clockwise or leaving the item cancels the first step. Progress reads
  ERASE n, then STORE FORMATTED. Settings and USER slots cannot persist on a
  foreign store until the user confirms formatting.
- The combined-waveform tables share their 16-entry blocks (`core/sid/gen/pulldown.h`,
  generated by sidou-tablegen in place of `wave_6581.h` and `wave_8580.h`): the firmware's
  122,880 bytes of tables take 39,256, and the image falls from 493,628 to 410,260 bytes.
  Lossless: every row of every profile and model reads back as reSIDfp's table (sidou-tablegen
  and emu-gate's `character.rs` check it), and the audio is bit-exact with before. A pulldown
  lookup reads a row's index, then its block: +12 instructions a sample per chip at worst,
  with every audio gate still met.
- **The menu is drawn at 2× text**: six of the thirteen items at a time, scrolling with the
  cursor, the selected item's line in reverse video (the blinking block cursor is gone, and
  with it the menu's idle redraws), the title bar showing the position and arrows for the items
  out of sight. Labels are shorter to fit a 14-character line: **PRE** (PRESET), **ROWS**
  (TRACKER ROWS); STORE reads `193K`, `FORMAT?` and `ERASE 20` (was `193K FREE`,
  `FORMAT? TURN`, `ERASING 20`).
- **Toasts use the large text** (2×, two lines when they are long), over rows 27-28.
- The minimum Rust version is 1.88 (was 1.87): `gt2-ref`'s 6502 core, mos6502 0.10.1,
  uses let chains.

### Fixed
- TRACKER records the first note after navigation even before its shared canvas is rebuilt,
  and resumes recording after nested MIDI-learn visits. Learn capture/completion no longer
  closes a child view or leaves a stale capture installed.
- A faint tone (about -50 dBFS, the voice DACs' leak) no longer stays after notes: with the
  filter bleed at 0 the output fades to exact silence once every voice has been done for
  46 ms, and comes back at the next note (found on the first real FM-1); a nonzero bleed
  keeps it. The chips are untouched (the stage mutes their block), so every reSIDfp
  comparison holds.
- **USB-MIDI out works on the FM-1.** On v0.2.0-m2-preview almost nothing SIDOU sent reached
  the host (the update protocol's replies, K1's CCs), so a unit could not be updated over USB
  from SIDOU. SIDOU now uses Felucca's `usb.c` byte for byte, built with Felucca's compiler
  flags (at SIDOU's `-O2 -mcpu=r3` its USB register accesses run too close together and the
  controller stops sending), with its endpoint buffers 8-byte aligned and two marked hooks:
  SIDOU's own USB strings and every incoming packet to SIDOU's parser (song position, song
  select, active sensing and reset arrive over USB as before). Without a host, a short check
  replaces Felucca's retry, which stopped the main loop for 28 ms a second. Hardware test 8.1
  passed on a real FM-1 (2026-10-21): installs and updates over USB, SysEx replies, K1's CCs,
  a replug and a cable plugged in after a battery boot. The diagnosis is in
  [usb-out-diagnosis.md](docs/hardware-results/usb-out-diagnosis.md).
- **The unit is "SIDOU FM-1" on USB** (was "SIDOU"): the FM-1 installers (Felucca's and the
  Firmware Hub's) look for "FM-1" in a port's name. macOS keeps a port's old name; delete the
  device in Audio MIDI Setup to see the new one.
- SysEx transfers no longer time out on TRS one packet in: their clock could step backwards
  within a main-loop pass.
- From the end-of-M2 review ([docs/reviews/2026-10-22-m2-review.md](docs/reviews/2026-10-22-m2-review.md)):
  - With KEYS A over a playing song, the song can no longer overwrite a key's note before the
    voice is handed over, and PLAY no longer silences the notes the keys hold.
  - REC and STEP stop at G#7, GoatTracker's last note; they wrote 93-95 as REST, KEYOFF and
    KEYON. REC above it toasts NOTE OUT OF RANGE, and the key still sounds.
  - A latched GLO only mutes in the song views; it also toggled a step or wrote a note.
  - Four screens deep, holding HOME goes home instead of doing nothing, and MIDI learn says
    NO ROOM FOR LEARN instead of failing silently.
  - Every help card says what its own view does: new cards for STEP, ORDER, INSTR, TABLE, the
    MIDI page and SONG, and LIVE's card corrected.
  - A flash chip the store doesn't know refuses saves at once (SONG: SAVE FAILED) instead of
    freezing the song; a sector that never erases is retired for the session instead of
    stalling the store; SAVE writes again after SysEx deleted or replaced the loaded file;
    settings and profiles say STORE ERROR: NOT SAVED after a flash error instead of SAVED.
  - A gap in incoming MIDI aborts only that port's SysEx transfer, and a reply already being
    sent finishes with its F7.
- The licence labels of the files derived from Felucca (`ota_flash.h`), reSIDfp (`sid_osc.h`,
  `sid_env.h`) and GoatTracker (`gt2_song.h`, `gt2_sng.h`) name their authors and licences.
- MASTER scales a playing song (found on the first real FM-1: a song played at full volume
  whatever the pot said): it is now a digital gain on the final output, after both chips and
  the effect slots, instead of the chip's volume register, which the song's own $D418 writes
  overrode. It covers the keys, songs on either chip and UI sounds; full MASTER is the
  output as before, the song's fades and volume digi trick keep their meaning relative to
  it, and MASTER no longer thumps at thump above 0. Live play differs from the 15-step
  register by at most half a step (1/30 of full scale) between its steps.
- `gt2-ref` on CI: a song one byte short is refused with a fortifying C library too, the
  packer's report no longer takes the test threads' output with it, and a refused pack
  no longer leaks the relocator's buffers.

## [0.2.0-m2-preview] - 2026-10-17
A preview of M2 in progress, from `main` as it stood when the work paused: **untested on
hardware, not for installing on an FM-1**. It is published so the songs can be heard in the
browser demo; the first release proper comes after M3 and the hardware checklist. Where M2
stands: "M2 status (preview)" below and `https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2-overview.md`.

### Added
- `docs/hardware-results/2026-10-17-first-flash.md`: SIDOU's first run on a real FM-1. It
  played; its USB-MIDI output did not work (no update path); the unit was recovered to V15
  through ROM boot with a Flipper Zero and `tools/fm1-unbrick-mac` (MvaveFM1Unbricker on
  macOS through a libusb Bulk-Only-Transport back end). docs/hardware-test.md section 0 lists
  the route.
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
- MIDI learn: hold SAVE and turn a knob (also hold EDIT for the effect slots), release SAVE, send a
  CC, and the knob's parameter follows that CC on every channel and both ports,
  besides its own; HOME or 10 s cancel, SAVE instead of the CC forgets one,
  SAVE held + HOME in the frame asks to clear all, then a fresh SAVE press and release
  confirms. A learned CC wins over the map's, panel changes go out on it, and a learned CC
  from MIDI is never echoed. Mappings persist with the settings.
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
- M2e: MIDI's modes (`app/midi_route.h`), set on the new MIDI page (HOME, MIDI) and saved
  with a song in its `MIDI` chunk (its options appended): OMNI (the default, M1's), LIVE
  (one channel), MULTITIMBRAL (channels N..N+2 chip A's voices, each a mono jam voice with
  its own GoatTracker instrument, bend and range, LEGATO; N+3 the instrument on chip B),
  WHOLE-CHIP A (one channel on chip A's three voices: POLY, CHORD or ARP, by frames or the
  MIDI clock) and WHOLE-CHIP B. The player-owning modes move the keys to chip B as a song
  does. On chip A's channels CC 74 and 71 lock its filter until CC 121, and the chip-level
  CCs set chip A's setup. Chip A's jam voices sound once song control (M2c) binds the
  router's player interface; until then they are routed and tracked. The setting is in RAM
  until the settings persist.
- M2e: MIDI clock and transport for the song (`app/midi_sync.h`): the song follows a MIDI
  clock from TRS or USB, its rows on the clock's 16ths (0.03 ms worst in fm1-emu at 120 BPM,
  a fixed 8 ms after the clock), with start, stop, continue, song position (sought in the
  background) and song select; or it leads one over USB with CLOCK OUT (24 a beat, start,
  stop, continue). The MIDI page's CLOCK (AUTO, FOLLOW, OFF) and CLOCK OUT, in RAM until the
  settings persist. Song control places a call at any sample (`song_ctl_align`, held starts).
- M2e: program change with bank select (CC 0 and 32, per channel): bank 0 the channel's
  sound (presets, or a GoatTracker instrument on chip A's channels), bank 1 songs (a request
  for the store's n-th song, taken once songs are on the device), bank 2 the chip's profile
  (0-4 built in; 5-12 USER 1-8, a request until user profiles exist). Bank select is no
  longer learnable. MIDI out puts a preset's program change in bank 0 and can send a song's
  in bank 1.

- M2a Tasks 1-4 and the Task 3b spike build were included in this preview;
  see [Unreleased](#unreleased) for the completed M2a results and the provisional timing policy.
- M2c Tasks 1-5b: GoatTracker 2's `.sng` as SIDOU's song file (decision record 0007), read and
  written with SIDOU's trailer; GoatTracker's packed player in a host 6502 as the reference
  (`host/crates/gt2-ref`); the player in C (`core/player/`), matching it register for register
  on every example.
- M2e Task 1: MIDI realtime and system common messages, song position and song select, and a
  clock follower (a least-squares fit over 96 clocks).
- M2f Tasks 1-3: RED MOON RITE's art from its prototype's drawing code, with the disarm and its
  originality review; the art as run-length sprites in flash; a painter region and a tile
  compositor for pixel scenes.
- The browser demo plays songs (Start, then Space or PLAY), and its Start button waits until
  the page is ready.
- M2c Task 7: parameter locks. A pattern row may lock the cutoff, the resonance, the filter
  mode, its voice's routing or pulse width (written over the player's registers for that
  row, in the audio interrupt) and chip A's character (moved with chip_ctl's glide); the song's
  own values return at the next row. Kept sorted in the LOCK chunk, carried by the pattern
  edits that move rows, dropped by the GT2 part's CRC rule.
- M2c Task 6: the song player in the audio interrupt. PLAY (button 12) plays SIDOU's first
  song, SIDOU LOOP (`assets/songs/sidou-loop.sng`, original, GPL-3.0-or-later), on chip A
  until M2d's transport (the keys move to chip B as M2b's keys_route says, and chip A takes
  the song's setup through chip_ctl first); the player's calls fall at their exact samples (gt2reloc's CIA
  period, a block's render split at the call), so the device's audio equals the host's
  render bit for bit. The packer's analysis is the main loop's, in slices
  (`core/player/gt2_shape.c`), and widens the player's shape for each edit; `song_ctl`
  carries play, stop, continue, mute, shape swaps, hand-overs, jams, edits (at most 8 a
  block, O(1) each) and the call period to the interrupt, and row and note events back.
  The song's 64 KiB content region is first in POOL (`__song_region`); the RAM/POOL boundary
  moved 8 KiB down.
- M2f Task 4: RED MOON RITE, SCENE's sixth piece (SELECT counter-clockwise from FIRE), a
  barbarian duel the song plays: kicks chop, snares parry with a spark, hats shift the guards,
  the bass gives the lead, the arp flares the torches, the cutoff darkens the sky, and bar 4's
  fill is a spinning attack that knocks the other's sword into the flagstones until he pulls it
  free. With no song the keys play it. The prototype's logic in C (`app/scene_rmr.h`, checked
  frame for frame against the script), music hooks over the song's events (`app/scene_song.h`;
  the `SCNE` chunk's voice roles and a new instrument-to-drum table, else guesses), drawn on
  the tile compositor in 21 layers: 98.931 cells a frame on average and 170 at most over a
  minute of the demo song, 8.727 ms of SPI a frame, 908.6k instructions. The song's events are
  drained once a pass in `platform/fm1/main.c` and handed to each listener (the song views' places, SCENE's hooks); `ui_poll` runs a
  view's work between frames (`views_idle`) when it has nothing to send. Goldens scene-rmr,
  scene-rmr-song, -disarm and -stuck. On main `084fea9`, the free-RAM gate measures 15,756 bytes against
  the 9,000-byte gate (M2d, M2a and M2f); the image is 479,584 bytes against M2f's 512,000.
- M2f complete: RED MOON RITE's plan marked done with its results against the budgets
  (`https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2f-scene.md`); `TESTING.md` lists its tests.

### Changed
- M2c Task 8: songs load and save on the store in bounded slices. SONG offers LOAD,
  NEW, 16-character SAVE AS and confirmed DELETE; SAVE in a song view saves the song.
  Unchanged saves write nothing, active saves freeze song edits, and settings schema 4
  remembers the last song after schema 3's THEME byte and schema 2's
  TRACKER ROWS. ABOUT plays SIDOU
  LOOP only with a clean, stopped song and restores the loaded song on close.
  Device edits also enforce a 65,536-byte canonical saved-size cap, trailer included:
  SONG FULL refuses an edit without changing the song. Oversized saved forms are
  refused on load; incremental accounting keeps edits from making songs unsaveable.
  Integrated with the large-text menu/popups and M2e: bank 1 recalls songs in name
  order, stopped; song saves and loads preserve MIDI setup and rows per beat. CHIP
  and MIDI USER selections retain explicit control intent under parameter locks.
  SONG follows both themes' colour roles, and TRACKER refreshes same-shaped recalls
  without displaying a partially loaded model. Refused loads preserve held notes,
  RPN ownership and inherited runtime settings; validation precedes runtime changes.
- M2f Task 4: the compositor's caps came down to 4 cells a run and 8 candidates a scan slice
  (RED MOON RITE's own tiles cost up to 5,930 instructions, and a song's analysis slice may
  share the pass); MIDI learn's frame (`SCREEN_OVER`) keeps the grid's pixel region as it keeps
  the overlays, so the view under it keeps its pixels.
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
- M2b: UI sounds on chip B's spare voice (M1e Task 6), off by default (the menu's new SOUNDS):
  a blip, a confirm, a chime, a buzz and SAVE's drive chunk, in GoatTracker's sound-effect
  format at 50 Hz. They never take a voice the keys use; off, nothing is written. The menu's
  golden changes (the SOUNDS item), and scene-rule30 and scene-stars with the longer way to
  SCENE in the menu (a later moment of the same pieces).
- M2b: the CHIP view's B tab. SELECT moves up from CHIP into the tabs, ALGORITHM there
  switches A and B, KEYS marks the keys' chip. A tab shows the keys' setup and edits it
  (with no song chip B follows it: FOLLOWS KEYS), but chip A's during a song, which shows and
  edits the song's setup. The effect slots show on both tabs as one shared chain. The CHIP
  goldens change on row 2 (the tabs); chip-b, chip-b-keys and chip-a-song are new. The tab's
  note sits on row 3, and the help line keeps SELECT MOVE ALGORITHM STEP; MIDI learn over the
  song's tab says SONG SETUP: NO CC.
- M2b: the keys play chip A, or chip B while a song plays on chip A (the new menu item KEYS:
  AUTO, the default, or A); the keys' setup travels with them, chip B following it in the
  background (`app/keys_route.h`, `app/chip_ctl.h`), and the keys' chip goes first in the
  register ring so it behaves exactly as one chip alone. LIVE says KEYS:B while the keys play
  chip B, the status screen and the boot banner show both chips (their LCD goldens updated).
  Until M2c a song is a test hook, so the keys always play chip A. With the keys on chip A
  during a song, chip A is the song's: only the voices the keys hold are written. Chip B
  stays in use until its envelopes are at 0, so release tails play out on it.
- M2b: chip B renders on the device beside chip A while it is in use, mixed before the
  effect slots and faded in or out over one block; out of use it is not rendered and the
  output is chip A's bit for bit. It is made at boot with the keys' setup, prepared in the
  background. The LIVE view's tap can follow either chip.
- M2d: the song views' frame (`app/song_view.h`). SEQ (button 11) opens the song views from
  any view and steps through them (STEP, TRACKER, ORDER, INSTR, TABLE; for now a TRACKER
  placeholder with the rows' numbers, also the menu's new second item); PLAY (12) plays and
  stops the song, EDIT + PLAY goes on; REC (13) turns record mode on and off; GLO (7) held
  with note keys 1-3 mutes voices 1-3 of the song and with keys 4-6 solos them, in every view,
  the keys sounding nothing under GLO (`app/panel.h`). In a song view the note keys audition
  the song's instrument on the cursor's voice. Row 0 shows the view, the transport, the tempo
  in BPM, the position and REC; row 1 the voices' waveforms, mutes, solos and envelopes; LIVE's
  top line shows the mutes and solos in the preset's place. The transport reaches the player
  through a small binding (`app/song_transport.h`, bound over `song_ctl` in
  `platform/fm1/main.c`; PLAY's LED follows it). `song_ctl` gains a jam release
  (`SONG_JAM_RELEASE`) and the audition (`song_ctl_audition`): jams that sound with the song
  stopped, the calls running with the sequencer held (`gt2_player.frozen`), chip A taken for
  the song as PLAY takes it and given back two seconds after the last jam; continue takes chip A
  again if it went back after a stop. The note keys now pass through the views
  (`views_note_keys`). The free-RAM gate moves to M2d's floor, 16,000 bytes. The menu, SCENE's RULE 30 and STARS
  goldens are redone (the menu's new item; the pieces' moment moves with the menu's longer
  walk), and TRACKER's, its mute and REC, and LIVE's mutes are new.
- M2d: TRACKER (`app/tracker_view.h`), replacing the placeholder: each voice's pattern in
  GoatTracker's layout (C-401308, `---`, `===`, `+++`, `..`, `...`), silent voices in dark
  grey, the bar's place in its pattern, each voice's orderlist from its entry and the bar
  row's command spelled out; the rows follow the player's row events while the song plays and
  the cursor while it is stopped, as a moving bar (the default) or scrolling (the menu's new
  TRACKER ROWS). The song views read the song through the binding (`song_transport.song`) and
  copy only the rows they built. Its costs over 30 s at tempo 6 and 3 in both modes are in
  docs/art/README.md "TRACKER"; goldens tracker, tracker-play, tracker-mute-rec redone and
  tracker-scroll new; the menu, RULE 30 and STARS again.
- The menu's CHIP item opens the CHIP view; the 6581/8580 choice it held is the view's profile
  picker (and the FX button, as before).
- The EDIT layer (EDIT + K1..K4, SCL, ENV) is no longer a stopgap: it stays in every view as a
  shortcut to the selected effect slot, the one the CHIP view's slot focus shows.
- The fm1-emu credit is reworded so that it fits one About page.

### M2 status (preview)
M2 is partway through; see the progress table in `https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2-overview.md`.
In this preview: chip B, the song player with parameter locks, TRACKER, RED MOON RITE,
the store, settings and song saves from the panel, SONG's file actions and About's music.
MIDI modes, banked program changes and clock/transport are included. Not yet:
TRACKER and STEP editing, SysEx.
The store's flash timings are the datasheet's until the spike runs on a unit.

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
  doc section 7), which M1e's plan left out and M2c Task 8 now supplies.
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
