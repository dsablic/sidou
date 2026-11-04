<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SIDOU roadmap

Where SIDOU is going, in the order we mean to do it. Each line says what a player gets; the
why lives in [decision records](decisions/) and the how in [plans](plans/). What shipped is
in [CHANGELOG.md](../CHANGELOG.md). Sizes: S is under a day of work, M a few days, L more.

## Released

- **[1.6.0](https://github.com/dsablic/sidou/releases/tag/v1.6.0)** (2026-11-04): layered LAB
  sounds, LAB's filter envelope, SidStation and MIDIbox SID patch import, no title cards.
- **[1.5.0](https://github.com/dsablic/sidou/releases/tag/v1.5.0)** (2026-11-02): LAB's
  song bridge, sounds lifted from a `.sid`, chords and drums from DICE, Live Link, the site's
  editor and library (the list below).
- **[1.1.0](https://github.com/dsablic/sidou/releases/tag/v1.1.0)** (2026-10-30): LAB's
  filter LFO, ARP RATE, MACRO links and peek, the auditions, generative TRACKER, DICE's range
  and rhythm, knob curves (the list below).
- **[1.0.0](https://github.com/dsablic/sidou/releases/tag/v1.0.0)** (2026-10-29): LAB, sixteen
  USER sounds, DICE's density and scale, dense `.sid` players on time (the list below).
- **[0.5.0](https://github.com/dsablic/sidou/releases/tag/v0.5.0)** (2026-10-25): `.sid`
  playback on SIDOU's own 6502, SIDS and NOW PLAYING, tunes over SysEx.
- **[0.4.0](https://github.com/dsablic/sidou/releases/tag/v0.4.0)** (2026-10-25): SID drum kit
  and digi drums, generative STEP, recording over the USB cable, an ARP helper, songs to MIDI.
- **[0.3.1](https://github.com/dsablic/sidou/releases/tag/v0.3.1)** (2026-10-23): an install
  over Felucca boots straight into SIDOU.
- **[0.3.0](https://github.com/dsablic/sidou/releases/tag/v0.3.0)** (2026-10-23): the first
  wider release: help cards, the first-run tour, the output limiter, CHIP SPREAD.
- **[0.2.0](https://github.com/dsablic/sidou/releases/tag/v0.2.0)** (2026-10-22): the first
  release to run on a real FM-1, with USB-MIDI out.

## 1.0

**LAB: shape one sound and keep it.** A view for experimenting on the go: five pages with
drawings of the oscillator, envelope and more, one press to keep a sound, 16 USER sounds,
A/B, undo and variation rolls. ENV, LFO and FX on LIVE open it. Design:
[plans/2026-10-25-lab-design.md](plans/2026-10-25-lab-design.md). L. **Released** in 1.0
(the manual page: [docs/manual/lab.md](manual/lab.md)).

**Smaller things that make 1.0 feel finished**

- **ARP helper: waveform choice.** Done: PRESETS in TABLE's helper picks the rows' waveform.
- **Readable small text.** Done for TABLE (the meaning line at 2×) and STEP (the ruler and the
  lane lines at 2×, the greyed chip B lane gone); STEP's page selector row stays 1×. S-M.
- **Fewer late writes for dense `.sid` players.** Done: 24 render parts a block (from 16), 80
  writes a block's window (from 100, to stay in the interrupt's budget), proved by a
  one-write-a-sample bench and synthetic player bursts; Sound Test stays a known limit.
- **Minimal DICE settings.** Done: EDIT + SCL held, K1 turned is the density (sparse to every
  step) and K3 the scale (six), saved per song in a `DICE` chunk, shown on STEP while held; range
  and rhythm stay in 1.x.
- **Store health on STATUS.** Done: STATUS's bottom row shows the store's retired sectors and
  flash errors, and a background settings save that failed is detected, retried at each debounce,
  and said once by a toast.
- **More test coverage.** Done: a gate that decodes every instruction of the image, and the
  ergo harness walks LOAD and the SONG pages, checking SAVE and HOME on every list.
- **A SIDS how-to video and What's new in 1.0.** S-M.
- **The GitHub look.** Avatar, social preview, the Firmware Hub card checked. S, by hand.
  Made: `docs/media/avatar.png` (576 x 576, Metal Chrome's S, `docs/art/avatar.png` scaled 12x by nearest
  neighbour) and `docs/media/social-preview.png` (1280 x 640, the logo, the tagline, five golden screens).
  To set them: Settings, General, Social preview, Edit, Upload an image (the preview); the avatar is the
  owner's account or organisation picture (profile Settings, Public profile, Profile picture). Redo
  both if the logo changes. The social preview is set (2026-10-29); the Hub card is still to be checked.

**Hygiene before the tag**

- **Its own USB ID.** A pid.codes product ID for the app, applied for by the user. 1.0 is not
  blocked by it: the ID lands in a 1.0.x. The update loader stays Felucca's, vendored byte for
  byte, on 0x1209:0x0002.
- **The ergo baseline refreshed** after LAB, with the accepted findings reviewed. Done.
- **A device session gates 1.0.** USB audio recording, the first-run tour, unplugging the
  cable during playback, REC timing, LAB's readability and keep flow, a 2SID tune, and a long 48 kHz recording, left to users (the CPU erratum in Felucca's resampler, [felucca-bsp#16](https://github.com/hugelton/felucca-bsp/issues/16); if it fails, 48 kHz goes off until a fix).

## 1.1

Every small and medium item of 1.x below (released in 1.1.0; the USB ID follows in a 1.1.x): [the 1.1 design](plans/2026-10-29-1.1-design.md)
(the user's choices, flash plan, order). The large ones (LAB's song bridge, instruments from a
`.sid`, the browser editor, Live Link, felucca-bsp) get their own designs after it.

## 1.6

Released in 1.6.0: layered LAB sounds (up to three oscillators on one key, the LAYERS
page), FILTER ENV, `sidou-sysex import` for SidStation and MIDIbox SID V2 banks (songs of sixteen
patches, tables as GoatTracker wave programs), filter metrics against real chips (the 6581 filter
is inside their spread, so the op-amp port is parked), and the title cards removed
([the design](plans/2026-11-03-patch-import-design.md)).

**After 1.6**
- **Bluetooth MIDI on an open stack: first after 1.7** (the owner, 2026-11-04, ahead of the plugin; research on branch `ble-research`). The old blocker, M-VAVE's closed BLE
  library, is gone: Optimist's stack (Henri David, `GPL-3.0-only OR MPL-2.0`) runs Bluetooth MIDI,
  pairing and the FM-1 as central on felucca-bsp's radio drivers (MPL-2.0, the radio tables
  Apache-2.0), with nothing of JieLi's linked; fm1-base (`ble_stack = "own"`) and CTL-1 ship it.
  Every part is GPL-3.0 compatible, so a published SIDOU may carry it. Open: its flash, RAM and
  RAMTEXT against SIDOU's (25 KB of flash, 16 B of RAMTEXT free; fm1-base says a radio fills most of
  the FM-1), the radio's time beside two SIDs' audio, and the felucca-bsp move first. A measurement
  spike first (BLE MIDI only, no A2DP). L.
- **The 8580 mixer's grit (a spike).** SIDOU's 8580 voice and mixer path is linear; reSIDfp's adds
  a second harmonic of -52 to -30 dB, which may be why SIDOU's 8580 is 0.4 to 1.0 dB farther from
  the CSG8580R5 recordings on some tunes ([the measurements](plans/2026-11-03-filter-model-feasibility.md#measured)).
  A short interpolated curve, a few hundred bytes, judged by the harmonic probe, the real-chip
  distance and the owner's ears; its RAMTEXT and instructions measured before anything is kept.
  S-M. The reSIDfp op-amp port stays parked for good (the owner, 2026-11-04): it would move SIDOU
  closer to reSIDfp, not to the chips, and the filter's SRAM, not flash, is what it lacks.
- **The DAW plugin's design session** (the owner, 2026-11-04), after 1.7: the spike
  ([its plan](plans/2026-11-05-daw-plugin-spike.md), code in `spikes/daw-plugin/`) runs SIDOU's
  package in fm1-emu's plugin at 17.6x real time; stable parameters a LAB card, GPL-3.0-or-later.
  A first release M, the whole edition L.
- **Chip B on the second core** ([decision 0010](decisions/0010-second-core-reopened.md)): first
  the RAMTEXT room in 1.7's performance pass, then the experiment in fm1-emu, off by default.

## 1.5

Released in 1.5.0: LAB's song bridge, instruments lifted from a `.sid`, chords and time-shared
drums from DICE, a popup for every small-print edit (with a check that enforces it), page jumps
and INSTR's pointer summaries, FM-1 Live Link with the computer keyboard, the site's library,
song editor and preview, MIDI controller maps, and the test and code cleanup (no code comments:
the decisions are in [design-notes.md](design-notes.md)).

**1.5.x**
- **SIDOU's own USB ID** (0x51D0): waits for pid.codes request #1341.
- **The move to felucca-bsp**: waits for felucca-bsp's PR #21 (the raw MIDI-in hook, our patch);
  #14 and #16 are fixed upstream. SIDOU's USB audio hand-over needs adapting to its newer
  recording path (found in the trial: 22 of 23 USB tests pass).

- **The quality pass**: performance headroom (RAMTEXT 80 bytes free, K1's input 246 of 250, the
  MIDI floods at 98 % of their budget, the SID views' UI poll 452 under its budget after the
  lift), the faster ergonomics gate, suggested automations, knob-curve follow-ups from the device
  sessions, and the two `-Oz` puzzles (`gt2_song.c`'s goldens, `fx_slots_init.c`'s timing).
- **Small fixes**: a pulled LAB name comes back after a power cycle (today the sound does, the
  name not); the Live Link smoke test's HOME hold no longer depends on wall-clock timing.

## 1.x

**Sound and play**

- ~~**The rest of DICE's settings:** range (K2) and rhythm (K4), left unbound beside the minimal
  density and scale. M.~~ Done in 1.1: the frame shows all four and the lane's root.
- ~~**The ARP helper's audition.** Hear the draft arpeggio before SAVE: the player reads the
  helper's draft rows as an overlay (the song-control jam plays only the song's own tables).
  M; the audio path and RAMTEXT (moved from 1.0, the user, 2026-10-29).~~ Done in 1.1: the keys
  play the draft (`HEARING DRAFT`), every turn heard at once; a device check follows
  (hardware-test.md section 23).
- ~~**A held step's audition plays its locks.** Hold a step and hear it with its locks. M; needs
  room in the audio interrupt, so a device check follows.~~ Done in 1.1: its cutoff, resonance,
  filter mode, route and pulse locks, pinned while held; the device check is section 23.
- ~~**LAB's song bridge:** send a sound to a song as an instrument, pull one back. M-L. Its own
  design first (the LAB design calls it the second step).~~ Done for 1.5 ([the design](plans/2026-10-30-1.5-design.md#2-labs-song-bridge)):
  REC in LAB sends, ENV / LFO / FX on INSTR pull, the `LBnn` chunk for the exact round trip; a
  device check follows (hardware-test.md section 26).
- ~~**Instruments from a `.sid`.** Lift an instrument out of a tune you play. M-L; pairs with
  the song bridge.~~ Done (1.5): REC in NOW PLAYING lifts the soloed voice's next note as a
  GoatTracker instrument, offered to the song when it is back ([manual](manual/sid-tunes.md#5-lift-a-sound));
  a device check follows (hardware-test.md section 27).
- ~~**LAB: hold-to-peek and macros.**~~ Done (1.1): EDIT held while you play peeks at A; the
  MACRO page's RESO and PWM links ([LAB](manual/lab.md)).
- ~~**FILTER's K4: a filter LFO.**~~ Done (1.1): `LIVE_PARAM_FILTER_LFO`, 0..32 of K1's detents
  each way at the PWM rate, CC 31.
- ~~**MOD's K4: ARP RATE**~~ Done (1.1): `LIVE_PARAM_ARP_RATE`, CC 28 through its map row; the
  velocity target stays on NRPN 6/1 (STATUS does not show it).
- ~~**LAB, press order:** EDIT + ARP and EDIT + LFO whichever finger lands first, in LAB and STEP.~~
  Done (1.1).

**Songs**

- **Combined drum-and-bass instruments** from DICE: a drum's first frames, then the bass's wave
  program, in one instrument (1.5 did the time-shared form). M.
- ~~**Generative play in TRACKER:** the same chance, MUTATE and DICE as STEP, C64 idioms
  (generated arpeggio chords, multiplexed drums, filter sweeps), a scale per lane. M each.~~
  Done in 1.1 (the cut): MUTATE, DICE and a filter sweep (EDIT + ENV) in TRACKER, a root and a
  scale per lane, chance shown as a mark (edited in STEP). Generated arpeggio chords and
  time-shared drums followed in 1.5.
- ~~**MIDI export on the device,** not only on the site; wavetable arpeggios and TRKB subtunes
  in the export. M.~~ Done on the site instead (1.1): its Songs to MIDI page pulls the current
  or a stored song from the connected FM-1 over SysEx (the FM-1 shows no files over USB), and
  the export follows wavetable arpeggios. TRKB: nothing writes it, so nothing to export; the
  export joins it when chip B's tracks are written. Live notes out stay later.
- **More than 48 keys in the store.** Only if players hit STORE FULL (the starter pack uses
  11; LAB adds one). M-L; changes the store format.

**Connectivity**

- **A browser editor and librarian.** Edit instruments, tables and patterns, list the
  device's songs, export plain GoatTracker files; today the site sends songs and files over
  SysEx. L; no firmware change. In progress for 1.5
  ([the design](plans/2026-10-30-1.5-design.md#5-browser-editor-and-librarian-site-only)):
  done, the wasm core (SIDOU's song reader and writer, GTI5 export and player as
  `sidou-core.wasm`, `tools/wasm-core`), the Library page (list, download, send with an overwrite
  check, rename, delete, back up and restore) and export (`.sng` with the trailer, plain
  GoatTracker with the channel-count check, instruments as GTI5), and the editor
  ([editor.html](https://dsablic.github.io/sidou/editor.html)): instruments, tables, a basic
  pattern and orderlist grid through the device's own edit primitives (LOCK and the save-size cap
  as on the FM-1), undo, a preview with the wasm player (an instrument at a note, a pattern, the
  song from a position; the chip's setup and the effect slots), saving to a named file or loading
  it now. Left for later: instrument delete and reorder, pattern renumbering, editing a Stereo
  song's voices 4-6, and the preview's output stage (thump, chip B, MASTER).
- **Screen mirror (FM-1 Live Link).** The screen, scopes and remote controls in a browser
  over the USB cable, for the other FM-1 firmwares too. L; the design text is in the
  [old roadmap](https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-18-roadmap-after-m2.md)
  and [1.5's design](plans/2026-10-30-1.5-design.md), section 6. In progress: the device's side
  (the session, the cells mirror, remote control), the [protocol](reference/live-link.md), the
  reference decoder and the browser viewer ([link.html](https://dsablic.github.io/sidou/link.html):
  the screen drawn by SIDOU's own `grid.c` and `font.c` in wasm, the buttons and encoders) are in;
  scopes as state are next.

**Platform**

- **Move to felucca-bsp (Felucca 1.7.0's board support).** Felucca 1.7.0 moved its chip and board
  code (USB, flash, the second core) into [felucca-bsp](https://github.com/hugelton/felucca-bsp),
  under MPL-2.0. SIDOU takes it as it takes Felucca's code today, byte for byte with its flags:
  - its `chip/usb.c` in place of 1.6.1's, with its USB-name build settings instead of SIDOU's
    hook, once [#15](https://github.com/hugelton/felucca-bsp/issues/15) (a raw MIDI-in hook) is
    settled ([felucca-bsp#14](https://github.com/hugelton/felucca-bsp/issues/14), the SIE timing,
    and #16, the 48 kHz resampler's multiplies, are fixed upstream in `14610bd`, 2026-10-31);
  - the update loader kept compatible: installs and updates from 0.x and from Felucca still work;
  - its notes on the CPU (64-bit multiplies beside 16-bit loads) checked against SIDOU's audio
    path;
  - what MPL-2.0 changes: the second core's code becomes usable, and the BLE variant's licence
    question (below) is worth asking again;
  - fm1-emu bumped past its watchdog model, so the tests check that a hang recovers. Done in
    1.1: fm1-emu `93cb7ee3`, real resets in the boot-guard tests, a watchdog hang test, and the
    USB tests on fm1-emu's own USB controller and host.

  L; a device session (installs, updates, USB-MIDI, USB audio).

**Quality**

- ~~**Knob curves for every parameter.** K1's cutoff, MASTER, K3 and K4 are done; the rest get
  a useful range, an even-in-perception curve and a test for dead zones and jumps. M.~~ Done in
  1.1: PWM, PWM HZ, VIB HZ, DEPTH and DELAY on walks, each effect control on one of three shapes
  chosen by rendering (controls.md "Knob curves"); OVERDRIVE's and FUZZ's drive left linear for
  the ear check.
- **Suggested automations.** A one-line toast offers "record this knob move as locks?" after
  you keep doing it. L, idea; cheap in the main loop.
- ~~**SysEx during a chord.**~~ Done in 1.1: an upload waits for quiet while a song plays or a
  computer records (WAIT, `UPLOAD WAITS FOR QUIET`); keys alone dip 250 ms apart, a build
  option until the ear check on the device. S.
- **Performance headroom.** A pass over the thin spots in the audio-interrupt gates (LIVE
  noise, SELECT into RULE, STATUS idle, MIDI floods, USB drain). M-L.
- **Faster test CI.** The 13-minute test is split and the local merge check runs everything under nextest; the GitHub CI (tags only) runs just the package boot test and does not run emu-gate. What is left: an optional sharded emu-gate workflow on GitHub's x86 runners (the gate's JIT works there), to take the emulator stage off the local machine. S-M.
- ~~**REC from the keys, compensated.**~~ Done in 1.1: a fixed 6.0 ms computed from the block,
  the double buffer and the debounce, in NEAREST and OFF; the feel at tempo 6 and 3 is a device
  check. S.
- ~~**sidou-flash retries on NAK BUSY** when a transfer meets a store write. S.~~ Done (1.1) in
  the site's sender; `sidou-sysex` only writes files.

## Later and ideas

- **SIDOU as a DAW plugin: a SIDOU edition of fm1-emu's FM-1 plugin** (the user, 2026-10-31). fm1-emu
  ships an FM-1 instrument plugin (AU, VST3, CLAP; MIT; macOS on Apple silicon) that runs any
  firmware package, SIDOU's included. A SIDOU edition adds what the panel lacks: a large mirror of
  the screen, real knobs for LAB's pages and LIVE, tracker entry from the computer keyboard, and
  side panels drawn from the firmware's state (the song's patterns, instrument and table rows,
  scopes), so songs and sounds are the same on the FM-1 and in the DAW. It runs the real
  firmware, so nothing is reimplemented. A standalone SID plugin is not the aim: Plogue's
  chipsynth C64 already is the reference SID instrument. First a spike (build the plugin, run
  SIDOU in it, measure the CPU, one side panel and LAB's knobs), then a design session. M-L.
- **A second engine on the FM-1's second core** (research): users want SID beside polyphonic
  synthesis without re-flashing. After the felucca-bsp move makes the second core's code usable;
  flash (about 25 KB left in the slot) and CPU measured first.
- **Faster firmware switching from the site** (research): the Library page installs another
  firmware package and brings SIDOU back, keeping the songs and sounds on the FM-1.
- **Layer mode.** Two sounds on one key. L; audio interrupt.
- **FM-1 Live Link extras:** a tray app, a share link, iOS.
- **RSID playback.** Not now (see below); revisit if there is demand.

## Not doing

- **RSID playback as built.** SIDOU refuses an RSID tune politely
  ([psid.md](reference/psid.md)): it needs a full C64 environment, not just the chips.
- **Combined-waveform rows in POOL.** POOL has no room for them
  ([flash-space plan](plans/2026-10-24-flash-space.md)).
- **Recording into a DAW with a plugin.** Superseded by USB audio out, which records over the
  cable.
- **2× oversampling and an idle LED glow.** Rejected: cost against a gain nobody asked for.
- ~~**Our own BLE stack.**~~ Not needed: Optimist's open stack (above) has the radio's controller.
