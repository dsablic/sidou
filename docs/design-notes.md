<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Design notes

Why SIDOU's code is the way it is, by area. Code carries no explanatory comments; the decisions
behind it live here. Cross-cutting decisions also have records in [decisions/](decisions/).
The cleanup before 1.5's release moves the decisions now written as code comments here.

## Hardware

- The FM-1's CPU faults on any misaligned 16- or 32-bit RAM access; fm1-emu does not model it.
  Structures copied word by word, especially in the audio interrupt, are aligned to 4 bytes with
  a static assertion (1.1's ARP helper crash). Genuinely packed data is read and written with
  byte copies (`memcpy`), never through a wider pointer.
- Every emu-gate test enforces the rule: `common::step`, which `Machine::run_cycles` and the
  bench runners use, steps through a bus that panics on a misaligned 16- or 32-bit RAM access,
  naming the PC, the address and the width. The check is one mask and branch per access; the
  `lab` binary ran as fast with it as without (under load, within noise). The JIT runs
  (`run_ms_jit`: the settled first boot) and `decode_gate`'s scratch steps are not checked; the
  first boot is also run stepped in `boot.rs`. The first sweep with the check on for every test
  (1.5, 2026-10-31) found no misaligned access in SIDOU's code, Felucca's `usb.c` or
  `third_party/`; with 1.1's fix reverted it fails at once on the draft copy's word store
  (`song_draft`).

## Flash and build flags

- The app slot is 581,564 bytes; each release plan sets a ceiling, `IMAGE_CEILING` in emu-gate's
  `tests/flash_plan`, which `flash_budget.rs` and `psid_budget.rs` both check (`psid_budget.rs`
  kept a copy once, stale at 552,000 through 1.1). A compile-time assertion keeps 13,564 bytes
  of the slot free above the ceiling for felucca-bsp and later.
- The ceilings by plan: M2a 372,000, M2b 384,000, M2c 414,000, M2d 440,000, M2e 452,000, M2f
  512,000; M3 534,000 (its 506,000 start plus 28,000); 1.0 552,000 (LAB's estimate and 1.0's
  small features, the user's raise on 2026-10-28); 1.1 556,000 (after a 3-5 KB win-back, the
  user's decision on 2026-10-29); 1.5 568,000 (after a 2-4 KB win-back, 2026-10-30). A raise is
  the owner's decision, recorded in the release's plan.
- Cold code builds at `-Os`/`-Oz`; files on instruction-budget paths keep `-O2`. `gt2_song.c`
  stays at `-Os`: at `-Oz` it moved four LCD goldens, cause not yet found.
- The voice shadow (`core/sid/sid_shadow.c`, the .sid player's main-loop copy of the voices)
  runs from flash but calls the chip's own out-of-line voice helpers (`sid_osc_run_combined`,
  `sid_osc_advance_slow`, `sid_noise_clock`, `sid_noise_settle_deferred`, `sid_env_run`,
  `sid_env_advance_slow`) in SRAM through the `*_far` pointers `sid.c` exports
  (`SID_VOICE_FAR`): a direct call cannot reach SRAM from flash (docs/hardware.md, "Call range").
  It saved 7.3 KB of flash copies and leaves `sid.c`'s SRAM code byte for byte as it was. The
  shadow still reads its envelope tables from flash (`SID_ENV_TABLES_IN_FLASH`), and builds
  with `-fno-function-sections` so that none of the helpers it still instantiates is taken
  into SRAM by app.ld's name patterns.

## Controls

- EDIT + SCL, EDIT + ARP and EDIT + LFO are chords whichever finger lands first: a hand's two
  fingers rarely land together. One mechanism (`screen_lone` in `app/screen.h`) serves LAB, STEP
  and TRACKER: a button pressed without EDIT is remembered while it stays down. EDIT pressed
  then makes it the chord. Its lone action is either done at its press (`SCREEN_LONE_DONE`:
  LAB's ARP and LFO, STEP's LFO) or held back (`SCREEN_LONE_HELD_BACK`: SCL's next effect slot,
  STEP's ARP gate, TRACKER's key off and rest with REC on) until it is let go or another control
  is used, which does the held-back action first.

## Ergonomics

- The popup rule: an encoder that changes a parameter shown in small print opens the value
  popup, its name and value large. A view that already shows the turned value large (3x, or 2x
  where the text is short) lights it in place instead: the popup would cover the very value and
  meters being tuned. The views that do: LIVE (the knob row's values at 3x, the profile word,
  the slot band at 2x, the arpeggio's rate, order and span on the knob row while ARP is held),
  CHIP (its four controls, the profile or the slot's algorithm, EXPERT 1/4, all at 3x), LAB
  (the cards at 3x, the choice at 2x). EDIT + K1-K4 over LIVE turns the selected slot, whose
  controls are small on the knob row: that one opens the popup.
- The other turns that open none: TRACKER's ALGORITHM (the voice at 2x in the header), SCENE's
  ALGORITHM (the piece's name in the large toast), SAVE AS's ALGORITHM (the name at 2x), the
  list actions (SONG, SIDS) and the turns that change nothing (STATUS, ABOUT, INSTR's
  ALGORITHM, K1 and K2 on ORDER's subtune selector, K2 on its restart marker). SELECT never
  opens it: it moves a cursor or a focus, which the screen shows.
- One table holds every exception, `NO_POPUP` in sidou-tests' `app/popup.rs`; the sweep beside
  it turns every encoder, with EDIT and without, in every view and state it lists, and fails on
  a turn that opens no popup and is not in the table, and on one that opens a popup the table
  says it does not. It found two popups that named the wrong control: EDIT + K1-K4 with ARP
  held on LIVE (the arpeggio turns, the popup said the slot's control) and K1 or K2 where
  ORDER, INSTR or TABLE take the knob for nothing (the popup said LIVE's cutoff or resonance).

## LAB

- Each drawing is sampled every 2 pixel columns (0.2 mm on the panel) to halve the drawing's cost:
  ENVELOPE at every pixel costs about 70,000 instructions; every 2 pixels about 35,000. The spans
  are turned into 8×8 masks on the stack and set as the cells' pixel overlays; no bitmap is kept.
- A cell's colour: the marker's if it has a marker's pixel, else the lit colour if it has a lit
  pixel, else layer 0's, else layer 1's. Only the cells whose overlay differs from what is drawn
  are set, within the budget; what the budget cuts short is set by the next call (the grid is the
  record).
- Traces (layers 0 and 1) are joined to the column before and a pixel thicker below; the marker
  (layer 2) stands alone. A trace's samples are 2 columns wide (the kit's `0xC0` bit).
- ENVELOPE's time axis (decades of 48 pixels from 1 ms): the attack rises across its sample
  because a pixel is 5 % of the time at 8 seconds (400 ms), and a point alone could miss the peak.
  The gate is let go at the middle (about 0.25 s) or once sustain is reached and held half as long
  again, at `LAB_DRAW_GATE_MAX_X` at the latest so the release has room.
- ENVELOPE's curve uses the SID's rate periods (the datasheet's 2 ms to 8 s attacks; a decay or
  release from 255 to 0 is three times as long, 756 periods) and the envelope's exponential steps
  on the way down (reSID's exponential counter). A period's microseconds are 1.015 of its cycles at
  the PAL clock (985,248 Hz).
- OSC's carrier ratio to the modulator for ring or sync on a sound of four voices (the modulator is
  another key's): 384/256 = 1.5. A cent's tuning is 1/1731 of the cycle: `2^(c/1200)` within
  0.05 % at 50 cents.
- OSC's noise: a fixed scribble about the middle, a random level every 4 pixels joined (at full
  height the trace would ink every cell, past the grid's overlays; the hash: `k * 2654435761u`).
- The device's `memset` (USB chip's) goes a byte at a time, so `lab_draw_apply` clears a
  word-by-word loop's buffer with a word-by-word store.
- FILTER's response is calculated from the filter's transfer function with the damping the chip has
  (the resonance's 1/Q, `sid_resonance_k`) and the cutoff the chip has now (`cutoff_out` through
  its curve, `sid_cutoff_hz`). The 6581's modes lose more even open (its high-pass about 11 dB),
  and its cutoff stops at about 240 Hz.
- FX's distortion transfer curves use the overdrive's gain law (`core/fx/fx_overdrive.h`) and the
  clip's cubic; the modulation uses the LFO's rate from the A control (0.15 to 0.05 to 0.1 Hz
  minimum, 26 to 172 column-octaves); the delay's tap distance is log-scaled (10 ms to line's end).
- Undo holds `LAB_EDIT_STEPS` steps in `LAB_EDIT_ENTRIES` ring entries (the byte's place in the
  sound and its old value). A step never takes more entries than the image has bytes, so any one
  step fits. A run (a card or the choice turned again) adds to the newest step only the bytes it
  does not hold yet, so the step keeps the values from before the run's first change.
- Silent as LAB's rolls mean it: a held C-4 at the panel's velocity, 500 ms from its note-on (the
  chip's envelopes at 0 before it, as a played instrument has them), whose output on the chip
  alone, through a DC blocker (a first-order high-pass at about 35 Hz: the chip's DC level moves
  with the envelope, a thump that is not the note), has an RMS under −40 dBFS.
- `lab_audible` estimates in whole dB: the waveform's level on the model (none, noise with another
  waveform, the 6581's combinations that barely sound, the 8580's with the pulse when it falls away
  from 50 %), the pulse's narrowness (the PWM taking half its swing off, a side under 16 steps
  refused), the envelope (the attack's ramp, the decay to sustain; an arpeggio 3 dB more), SYNC
  LEAD's sync 6 dB (more below a ratio of 1, where the voice never ends a cycle), and the filter's
  loss at the cutoff's distance from the note (routed to no mode: refused; a narrow pulse under a
  low-pass near the note counts its fundamental alone; a combination or the ring 6 dB more). The
  resonance's gain, the chip's profile, the effect slots and the controllers are left out, each
  only louder or not the chip's. The estimate must reach `LAB_AUDIBLE_DB` (−38 dB). `lab_edit.rs`
  proves it against renders: no sound it passes is silent.
- Song bridge send: the waveform, ring and sync as a one-row wave program (`ctrl|gate 00`, then
  `FF 00`); AD and SR with GoatTracker's hard restart (gate timer 2, HR on; an attack of 0 is
  raised to 1, which does not click); the pulse as `$8H LL`, the PWM (its link baked) as a pulse
  loop; the filter (mode, resonance with RESO LINK baked at the cutoff, the cutoff's top 8 bits)
  routed to the song's current voice, the filter LFO as a filter loop; the vibrato as the
  instrument's delay and a calculated-mode speed row (50/(2S+4) Hz, a power-of-two depth); the
  arpeggio as the ARP helper's default chord. One-voice sync, the fine tune, the performance targets
  and the effect slots are dropped. Exact round trip: a trailer chunk per sent instrument (`LB` and
  the instrument in hex) keeps the 48-byte sound and a CRC-32 of the instrument's 25 bytes and its
  programs' rows; pull restores the sound exactly while the CRC matches.
- GoatTracker's optimisations (on by default) rest the pulse program on each row's gate-off tick
  and the vibrato on tick 0: one frame in six at tempo 6 (the default), which the pulse loop's and
  the vibrato's frames make up for (`RUNS(f)` macro: `(f) * 5 / 6`).
- LAB sound image: what a USER slot keeps on the store, what `live.base` holds, what LAB compares,
  undoes and rolls. Not in it: the chip's profile and character (they stay the instrument's, CHIP
  and USER 1-8), the model, the envelope mode (instrument-wide), the octave and the volume. A
  reader takes any sound of at least `LAB_SOUND_BYTES_1_0` bytes; the bytes it does not have are at
  their defaults (0).
- LAB composes as CHIP does: the text is built in the canvas when what it shows changed (a hash of
  it, `lab_view.look`), and the canvas's changed cells are copied onto the grid within
  `LAB_VIEW_FRAME_CELLS` a frame less what the screen's layers take; the drawing then takes what is
  left of the budget, drawn again when what it shows changed (`lab_view.art`) or until it is whole.
  When both changed (a card turned while a note sounds), a frame builds one of them by turns, so
  that no frame's `ui_poll` pays for both.
- EDIT + ARP varies: each card the page uses moves 1 to 3 detents either way (clamped) and, one
  time in four (always when it uses no card: an effect slot that is off), the choice a step (not
  ENVELOPE's, the instrument's mode). EDIT + SCL rolls the voice (`live_dice_shape`: the waveform,
  the envelope, the filter, the pulse, the vibrato's depth; the slots, the arpeggio, the PWM and
  the tune left), the sound's base and name kept. A result is never silent (`lab_audible` on the
  chip's model) and never the sound before it or the last result (`lab_view.rolled`): such a result
  is tried again (`LAB_VIEW_VARY_TRIES` times for a variation, `LAB_VIEW_ROLL_TRIES` for a roll,
  and then the smallest move that passes); when none passes the sound stays and the toast says
  `NOTHING AUDIBLE`.
- OSC's sync base at most 4095 (16 times the pitch): the ratio's product stays in 32 bits.

### LEARN

- LEARN's frame (rows 20-25) is drawn over the view underneath (which keeps showing the
  instrument). It takes the panel's events first (`screen_capture`): K1-K4 pick the knob's
  parameter instead of turning it; everything else (the note keys, SCL, OCT, FX, on CHIP SELECT and
  ALGORITHM) goes on as usual. The frame's compose calls the view underneath first, then draws the
  frame over it; it covers its cells (`screen_cover`) so the view underneath that holds to a budget
  (LIVE, SCENE) leaves them alone and counts what the frame sends this frame. It clears the
  overlays the view underneath draws on its cells; the others stay, there and back (`SCREEN_OVER`).
- The frame's input passes SELECT and ALGORITHM on to CHIP underneath only: the menu's ALGORITHM
  would open a view over the frame, SCENE's SELECT would wipe in a piece under it.

## LIVE

### Register sink protocol

The live instrument never touches a sid directly. Every register change goes through a sink
function, called once per write in the order the writes must happen. On the host the sink applies
them to a sid at once; on the device the sink pushes them onto a ring that the audio ISR drains,
so the chip changes only between two rendered blocks.

Besides SID registers 0x00..0x18 a sink receives pseudo-registers, outside the SID's 5-bit
address space so sinks can catch them: LIVE_REG_RESET (0x80, a sid_model: re-initialise the chip
with that model); LIVE_REG_GATE_RESET (0x81, 0 or 1: the envelope mode, which sid_init turns
off); LIVE_REG_CHAR(field) (0x82..0x86: the character's profile and four controls, 0..127 each;
a sink keeps them and applies them to every chip a later reset makes, and ramps the playing chip
towards them at each LIVE_REG_CHAR_TICK); LIVE_REG_CHAR_TICK (0x87, value 0, once every
live_tick).

A model or profile change with a preparer waits for the chip the reset will swap in: live_tick
calls the preparer once a tick, a slice of the work each, and sends the reset once it says the
chip is whole; without one the reset goes at the next tick.

### Pitch offsets and frequency calculation

Pitch offsets are in 1/8192 semitone (Q13): a 14-bit pitch bend times the range in semitones is
exact at every range. live_freq_at returns the frequency register for a note moved by an offset
(the table's value at offset 0, otherwise within 0.6 of the exact value, rising with the offset
and by at most 1 for each 1/8192 semitone). Notes above 106 do not fit 16 bits and are folded
down by whole octaves.

### K1's cutoff walk

K1's walk (live_cutoff_steps): per chip model (6581, 8580), 65 registers rising strictly in
register and in Hz, from the first one a semitone above the chip's lowest cutoff to the highest,
spaced evenly in pitch. live_cutoff_up and live_cutoff_down: per bucket of 32 registers, the
first entry above every register of the bucket and the last entry below it, so a cutoff the table
does not hold (MIDI, a preset, the model changed) starts from just before the first entry beyond
it in the turn's direction.

### The chord arpeggio

With the arpeggio on, the notes held (keys' and MIDI's) are one voice's, which steps through
them, a note every LIVE_ARP_RATES frames (50 Hz, a C64 player's), its gate held from the first
note to the release of the last: only the frequency moves, at a tick, so the chip changes pitch
between two rendered blocks with no new attack. The other voices stay free. A fresh preset's rate
(load_preset) is 3F, three frames a note: at 1F a held chord sounds like a wobble; at 3F each
note is heard. Rates from LIVE_ARP_RATE_CLOCK count MIDI clocks while they come (a 32nd, a 16th,
an 8th), else as many frames.

### Velocity targets

The SID has no per-voice VCA: these stand in for one. A velocity of 127 sounds as OFF does; lower
ones lower the target (the sustain level scaled, the filter 8 steps lower per velocity below 127,
the pulse width narrower).

### Envelope modes

LIVE_ENVELOPE_INSTANT (the default): the rate counter restarts at each gate edge, so the attack
at a note-on and the release at a note-off take their first step within one rate period and 3
chip cycles, whatever the rate before them. An instrument played live wants every note, and every
note's end, on time. LIVE_ENVELOPE_AUTHENTIC: the chip's ADSR delay bug, as reSIDfp has it: after
a slower rate most notes and note-offs wait up to 33 ms for the rate counter to come round.

### PWM and filter LFO

The pulse-width LFO's depth narrows near the ends of the range, so that its sweep never goes
thinner than K3's narrowest width (LIVE_PULSE_FIRST), nor as thin at the wide end: clamped to 0,
a width is no pulse but a DC level, and the voice went silent for part of every cycle (PWM BASS
at a 9 % width, a quarter of its 2 s cycle). One LFO: the PWM's and the filter's, at the PWM's
rate.

### Macro links

The macro links (RESO LINK, PWM LINK): the resonance written is the parameter plus reso_link ×
(32 − the place of the cutoff written on K1's walk) / 8, clamped to 0..15 (positive opens it as
the cutoff closes; negative, the squelch, as it opens); the PWM's depth written is the parameter
plus pwm_link × the width's distance from 2048 / 8, clamped to 0..LIVE_PWM_DEPTH_MAX. They act
wherever the cutoff or the width move (K1, K3, LAB, CC 74, the wheel, the filter LFO).

### SYNC LEAD (mono_sync)

SYNC LEAD's one-voice sync: one note at a time on voice 1, hard-synced to voice 3, which runs
silently at the note's pitch while voice 1 runs sync_ratio_q8 / 256 times higher. The sync lead's
voices 1 and 3 take the same pitch, so the ratio holds through a bend and the vibrato. A known
limit: at 2.75 times the pitch the register is full from note 90 (F#6) at rest, from lower notes
bent or vibrato'd up (88 at +2 semitones). Voice 3 still sets the pitch; the ratio, and the
timbre, shrink toward the top. Dropping voice 1 an octave instead would jump the timbre mid-bend,
so it saturates.

### Voice allocation

A voice for a new note: a free one (the longest released), the UI sound's when the others are all
held (a UI sound gives its voice back, with the instrument's envelope), else the one held longest.

### Expression scales

The cutoff moves 8 steps per controller value (0..1016), the pulse width 16 (0..2032); a vibrato
depth of 127 is 127 × 64 = 8128 / 8192 semitone.

### DICE randomisation

DICE's roll uses a xorshift32 generator (the state never 0), the seed mixed with the roll's
number. The waveform's distribution: 3 of 8 saw, 2 pulse, 2 triangle, 1 noise. Ring on a triangle
and sync on a saw or pulse are drawn whether or not the choice applies: one number a choice. The
filter LFO is off.

### Knob walks

Knob walks (live_walk.c, docs/reference/controls.md "Knob curves"): the values a knob lands on,
rising strictly, each one detent from the next, so a parameter whose ear is logarithmic moves by
even steps of what is heard. They choose the values only: a stored value keeps its meaning, and a
value off the walk (a preset, MIDI, a sound saved before) is kept until a turn, whose first
detent lands on the first entry beyond it the turn's way (none that way: it stays). PWM Rate: 32
positions, geometric 0.05..12.75 Hz, in phase steps. PWM Depth: 32, square law 0..LIVE_PWM_DEPTH_MAX.
Vibrato Rate: 24, geometric 0.5..12.7 Hz. Vibrato Depth: 32: 0, then geometric 6..127
(±4.7..±99 cents). Vibrato Delay: 24: 0, then quasi-log 40 ms..1.27 s.

The effect slots' shapes (32 over 0..127): LIN even, 4 a detent (5 mid-way, clear of the steeper
ends); LOG 0, then geometric 5..127 (+1 steps where geometric is finer); EQP equal power, 127
sin(π/2 i/31), dense at the top (+1 at the top where finer). A slot's control's shape is chosen
per algorithm and field by its rendered response (host knob_curves.rs).

A walk's CC position and a position's CC without a division (MIDI out sends a CC a parameter,
emu-gate midi.rs bounds a tick): round(cc (n − 1) / 127) = (cc POS + 2048) >> 12 and round(i 127
/ (n − 1)) = (i CC + 128) >> 8, exact over 0..127 and 0..n − 1.

### MASTER

The MASTER pot's volume (0..1023) is a digital gain on the final stereo output
(platform/fm1/audio.c): after the chips, the effect slots and chip B's mix, so it scales
everything the instrument plays (the keys, a song on either chip, UI sounds) and the chips' own
volume registers keep their musical meaning (a song's fades and its 6581 volume digi trick play
relative to MASTER). The live instrument's volume register is always 15.

The pot's 0..1023 maps through an audio taper, −60 dB to 0 dB in even steps of dB (a pot's travel
feels even), so full is exactly unity (MASTER_UNITY, 1.0) and the stage does nothing at all there
(the output is the chips' bit for bit); 0 is silence. A change ramps linearly over the block it
lands in, so the pot never zippers.

### Tail gate

With every envelope at 0 and its gate off a SID chip still puts out the waveform's leak through
the voice DACs' envelope zero: a faint tone, about −50 dBFS, which never decays. The chips are
left exactly as they are; this stage mutes the block they produce instead. Once the chip (or both,
with chip B rendering) has had every envelope at 0 with its gate off, and a filter bleed of 0,
for TAIL_HOLD_BLOCKS blocks (16, 2048 samples, about 46 ms), the next block fades to 0 linearly
(TAIL_BLOCK samples) and the output stays exactly 0 until a voice gates on or a bleed is set. A
bleed keeps the leak: it is a deliberate effect of the character. The hold is longer than the
gaps between the notes of a phrase, so they play untouched.

### Output thump

The character's thump is a step at each note's start and volume write that the chip's DC blocker
turns into a decaying bump. The chips leave it out of their render; here each chip's steps are
taken (at its output gain) and played as its blocker would have played them, a step that decays
by the chip's dc_r a sample. Played last (after the chips, chip B's mix and the effect slots,
before MASTER), the thump is fitted into the room the output leaves: a sample of the stereo
output s gets the thump x where |s + x| stays within THUMP_OUT_CEILING (one inside the rails) with
room to spare, and the part of x beyond half the room left is softly limited (x′ = h + d h / (d +
h), h half the room, d the excess), so the output nears the ceiling but never reaches it. A
signal without a thump passes bit for bit, a sample already on a rail stays as it is, and no
thump clips: not on two chips at once, not after an effect's gain, at no setting. The thump does
not pass through the slots (an echo or a reverb does not repeat it, a distortion does not shape
it): it is a low bump under the note's start, not part of the voice. Nothing runs while no thump
sounds (the stage is idle once a bump has decayed under half an LSB, which no longer changes a
sample).

### Effect slots protocol

live keeps each slot's settings (algorithm, Amount, A, B, Mix; the controls 0..127 as MIDI) and
sends every change down its sink as a pseudo-register, LIVE_REG_SLOT(slot, field), value
(0xA0..0xB4: the base 0xA0 has no bit in slot << 3 | field, and bit 7 marks a pseudo-register).
On the device that is the register ring, so a slot change reaches the audio interrupt in order
with the SID writes of the same pass and is applied at the start of a block, like them. The slots
are one chain on the mix of both chips, so their pseudo-registers name no chip: a sink bound to
chip B sends them as they are. On the host fx_live_sid_sink applies everything at once.

Each slot's defaults (Amount, A, B, Mix): what an algorithm picked from the panel starts from,
each one audible at once. NONE's row is zeros.

### LIVE view

LIVE composes every frame (no changed function), within a budget of LIVE_VIEW_FRAME_CELLS cells
(200; about 17.5 ms of SPI, inside the 20 ms budget): the scopes get what the rest of the frame
and the screen's toast, raster and cover (MIDI learn's frame over it) leave, and a trace that
does not fit waits for a later frame (three voices of noise take turns). The canvas (small text
and bars) is built whole once; after that each part (the header, a voice's lane, the knob row,
the bottom band) is drawn again only when what it shows changed, and a meter only its own cells:
a frame of moving meters costs a few cells, not the 900 of the canvas. The large text is drawn on
the grid itself (grid_text_big) for the rows that were copied. While a wipe or a raster runs it
draws what it drew last, as About does: the transitions are sized for a still view.

The scopes come from the audio interrupt's tap: the newest points, folded by the oscillator's
phase (scope_trace), `cycles` periods across the width, more for higher notes; a hard-synced
voice is folded by its sync source's phase, so the sync shape shows. The trace's height is the
waveform times the voice's envelope: flat at rest. The trace is 160 × 32 pixels, 2 pixels thick,
an overlay on each cell it crosses, over a dotted centre line, in the voice's colour (dark grey
and flat when the voice is silent).

The overlays are taken to be the drawn traces (lv->y, in lv->color) while the grid's pool has as
many free slots as LIVE left it: only LIVE uses overlays while it shows, and a view switch frees
them all. When they are not, every voice is marked to be drawn again whole. Fresh traces (pend)
take the cells the frame has left once everything else is composed and what it sends and what the
screen's layers and the cover add are set aside; voices to draw again go one a frame, top first,
which keeps ahead of a wipe (it reveals voice 1's scope in its first frame, voice 2's in its
second, voice 3's in its fourth). A trace no pixel of which moved by more than one is the last
one wobbling (which points the fold got): it stays as drawn, so a note that holds still sends
nothing. A voice's level: its waveform's peak times its envelope. The meters: up at once; down at
most LIVE_VIEW_FALL a frame, and not for a step or two (a steady note's peaks wobble), so a held
note's meter holds still.

### USER chip profiles

The USER chip profiles (eight slots, USER 1..8): each a built-in base profile, the
character's four controls and expert overrides. A profile is a chip's setup as a player left it.
The slots themselves live in `live` (user_valid, user_base, user_ctl, user_expert: what the
picker, CC 85 and the CHIP view read, no store needed); user_profiles reads them from the store
at boot and writes a slot the player saved. Each slot is its own store key, STORE_KEY_PROFILE(n),
so the settings record does not grow with the saved slots.

Schema 1 (USER_PROFILE_V1_BYTES): version, base profile, control count (4), controls. Schema 2
(USER_PROFILE_V2_BYTES) appends the expert count and raw overrides (255 follows the
profile/macros). A later schema's extra bytes are ignored.

user_profile_save puts the slot into live at once and queues the write; the views' frame ticks it
until the store takes the record, then until it commits, and says so (the result). A store that
cannot take it now (STORE_BUSY) is asked again on later ticks; after USER_PROFILE_FORCE_MS (2 s)
of waiting the platform's forced held operations are asked for, as for an explicit settings save,
and the result says GAPS. A foreign store keeps nothing: the slot stays in live until the next
boot.

### USER sounds

LAB's USER sounds (sixteen slots, USER 1..16): each a sound (app/lab_sound.h) and its name, in
one private store file (STORE_SOUNDS_NAME, type STORE_FILE_SOUNDS); and the unsaved edit, one
small record (USER_SOUND_EDIT_KEY) the settings' debounce writes before the settings record and
settings_load applies after it. The slots are not cached in RAM: live.sounds_valid says which
hold a sound (user_sounds_load, at boot), and a slot is read from the store when it is loaded.

The file: [version][sound bytes][slots], then per slot [used][name][sound]. A keep rewrites the
file whole, streamed through the store's own staging buffer: the other slots from the file as it
is, the kept one from a copy here, so a write costs one slot's bytes of RAM. The file is the old
one until the new one commits (a power cut leaves either), and a later version's file is read for
this version's bytes of each slot (an older file's slot has fewer bytes: the rest are their
defaults, 0).

user_sound_save sets the slot in live at once (its bit, and the sound as the current sound's
base: a keep is of the sound playing) and queues the write; the views' frame ticks it while it
has work, until the file commits, and says so once (the result). One write at a time: a keep
while one is written is refused (try again).

The unsaved edit (key 0x109, type 4): [version][preset][edited][name][sound bytes][sound]. The
preset byte is the settings record's: 0..LIVE_PRESET_SLOTS − 1 a built-in or DICE,
LIVE_PRESET_SLOTS + n USER n + 1.

## Boot, help, about and the scope

**Scope (LIVE's audio tap).** The ring is a history, not a queue: single producer (audio
interrupt), single consumer (main loop), one core; the producer never waits or looks at the
consumer, overwrites the oldest point, and publishes the head once a block (after the points);
the consumer takes the newest points it wants and drops what it overtook. Drawn phase-folded (an
equivalent-time display): each waveform point lands by its oscillator's phase, not its time, so
the trace stays still and shows a waveform whole at any pitch. Points are decimated (every 16
samples: 2757 points a second, 110 a frame) to keep far fewer than a time-base display would
need. The ring's size is a power of two for masking. One render a block keeps a render's
per-call setup out of the tap's cost. The tap runs only while the consumer asks for it (`on`):
nothing is added to the interrupt while LIVE is not showing. Compiler fences order memory
accesses (the points before the head, the head before the points, the copy before the head that
says whether it held); the pi32v2 compiler has no `__atomic_signal_fence`. Peak meter: 6 dB per
8 steps (8 log₂(amplitude); 64 and below empty, 32768 full).

**Help.** Drawn in units (the title bar, then each line), sized so no frame sends more than 200
cells (HELP_FRAME_CELLS) whatever it covers. A card takes at most two pages, eight lines each;
what a line does can be the state's own words (a first byte HELP_TEXT_K1..K4 the name of that
knob's control of the slot in `arg`, HELP_TEXT_QUANT the quantise in `arg`). Lines fit 14
characters, the control and what it does.

**About.** Frame-by-frame cost planning: the logo is still (a bitmap sent once, under the
wipe); the bars are five bands of 4 pixels, one moves a band on each odd frame, so each moves
at most every 4 frames; the credits fade on even frames, two text rows at most at a time; the
scroller moves 2 pixels a frame, each letter a white overlay over the bars' cells (no bar has
white in it), bobbing a whole cell row at a time on a sine; the stars are one-pixel overlays in
three layers, moving a pixel every 4, 2 or 1 frames. It composes every frame and holds still
while a wipe or a raster runs (screen_animating): the wipe sends its own rows and nothing else.
The credits pages cascade: each row appears ABOUT_ROW_DELAY frames (4) after the one above and
is shown ABOUT_ROW_FRAMES frames (86), fading through 4 levels a level every other frame (so at
most two rows change in a frame, all on even frames). The bars' budget: moving them recolours
the border's 116 cells, about 10 ms of SPI, so they change every other frame at most
(BOOT_STRIPE_MS = 80 ms). Silent mode (unsaved changes) selects different scroller text.
Credits are generated from CREDITS.md by a host test (with SIDOU_CREDITS_UPDATE=1): a page per
section, each line wrapped to CREDITS_COLS characters (26, inside the frame and its margin) in
upper case and the screen font's ASCII (accented letters lose their accents); a credit longer
than a page starts one and goes on under "<SECTION> (CONT.)", and nothing shares a page with its
tail.

**Logo (pluggable asset).** A logo is a table in flash: its footprint, cost, a draw function
for one frame of its animation into the footprint, and optionally a full-screen scene. Draw
sets every cell of the footprint (glyph, colours, overlay, pixel region), so the caller
composes it like any other part, every frame or once. Its animation runs on the caller's frame
count (25 fps); frame 0 is the still. Its background is its own: no transparent one. The grid
has one pixel region and it is the logo's: a screen shows one bitmap logo at a time, and a view
switch clears it. Pulse Train is drawn in code with font glyphs and overlays; it changes only
on odd frames (the stripes on frames 2, 6, 10, ..., the pulse on 4, 8, ...). The chrome logos
(Metal Chrome and Cracktro Chrome) are bitmaps (generated from assets/logo/*.png): their draw
hands the footprint to the renderer's pixel region, which shows the bitmap's cells pixel for
pixel; the cells are blank (white on the background, no overlay) so the bitmap shows alone, and
what the caller writes in them afterwards draws over it. Still: no overlays, and no cell
changes after the first frame.

**Title card.** Sequence: the glow fades in through ordered-dither levels (a purple and red
glow drawn a cell at a time with the font's dither glyphs in two neighbouring stops of black,
blue, purple and red), each level holding TITLE_CARD_LEVEL_HOLD frames (2, the page's preview)
once all its cells are out; the title wipes in from the left, TITLE_CARD_WIPE_COLS columns (3)
a frame, the bitmap growing over the same bitmap cells, so each frame sends the cells it adds
and no others; it holds TITLE_CARD_HOLD_FRAMES frames (25, 1 s); it ends. Budget: every frame
sends at most TITLE_CARD_FRAME_CELLS cells (200, about 17.5 ms of SPI, inside the 20 ms
budget). A step that changes more goes out in bands, top first, from where the last frame
stopped, until every cell of it is out. Ends on: a button or an encoder (swallows the event
and does nothing else), or a note from the keys or MIDI (which also plays).

**Boot.** Two sequences: the full one (after an install, or HOME held at power-on) is the
banner and READY., LOAD"SIDOU",8,1 typed, SEARCHING FOR SIDOU, LOADING with raster stripes in
the border, READY. and RUN, then the title card (the logo's scene or a slot with ramp lines);
the short one (the default) is the title card alone. HOME held at power-on: the first HOME
press edge reported by the key scan (after the debounce, about 3 ms) is part of the hold, not
a skip. The boot never holds the instrument up: audio, the tick, MIDI and the interrupts start
before the screen's first frame, and the main loop plays the keys and MIDI while it runs.
Anything ends it: a button or encoder (swallows the event), and a note from the keys or MIDI
(which also plays). The loading stripes: xorshift32 for repeatability (the same step gives the
same stripes), each cell row of the border two colours split at a random height (a lower
eighths glyph); 116 border cells, about 10 ms of SPI, change every other frame at most
(BOOT_STRIPE_MS = 80 ms). The logo holds still while the wipe into the card runs (the wipe is
sized for a view that does not move).

**Boot guard.** Firmware that crashes or hangs early in every boot ends in UBOOT after
BOOTGUARD_FAILED_MAX failed boots (2), which still accepts updates. The state lives in .noinit
RAM: a reset keeps it, power-on fills it with noise. A magic word and a check word (magic ^
pending ^ failed ^ BOOTGUARD_CHECK) seal the guard: a guard whose check does not match is taken
for power-on noise or another firmware's, so only counts SIDOU wrote send a boot to UBOOT. Each
boot that finds the previous one still pending counts it as failed (a crash, a watchdog reset
or a power cut before it was stable); a boot is stable once the main loop has run
BOOTGUARD_STABLE_MS (10 s), and the count is cleared. An intentional reset (an update's commit)
is not a failure (bootguard_leave clears `pending`).

**UI sounds (chip B's spare voice).** Each sound is data in GoatTracker's sound-effect format
(readme section 6.3): attack/decay, sustain/release, a pulse byte, then the frames: $01-$81 a
waveform kept until the next, $82-$DF a note (D-0 to B-7, which every frame has), $00 the end.
Stepped at 50 Hz (every other live_tick). A sound never steals: it plays on a voice of chip B
that is free. The keys play chip A: chip B is all free (voice 3, chip B's volume up, no
filter). The keys play chip B (a song on chip A): a free voice is one whose gate is off and
whose release has run out (live's voice state and the release time), the newest of them (so not
the one live's allocator takes next); none under mono sync (voices 1 and 3). While a sound
holds the voice, live's allocator skips it (`live.ui_voice`, the voice number plus one) and its
writes to the voice (and its $D417 filter bit) are held back by a filter on live's sink; a note
that finds the other two voices busy takes the voice back at once, and the sound stops. When
the sound and its release are done, the voice gets live's registers back (live_rewrite_voice).
A sound stops, writing nothing more, if the keys move between chips while it plays.

**SAVE gesture.** SAVE pressed and released with no knob turned and no other button pressed
while it was down is a save (save_key_buttons returns SAVE_KEY_SAVE at the release); a knob
turned or another button pressed while SAVE is down spoils that save (save_key_spoil), and the
release saves nothing. SAVE's press never reaches the views or the panel (the views read
save_key's held bit directly for cycle savings while routing).

**Sine (screen animations).** Integer only, from a quarter-wave table in flash (65 entries,
sin(2π i / 256) × 127 for i = 0..64); symmetry gives the full turn.

**Bitmap codec.** Run dictionary and checkpointed row packets (documented in
docs/art/bitmap-codec.md). Nibble cursors for decoding. Raw vs coded modes (bit 128 of byte
15): raw mode uses byte runs and cumulative row offsets; coded mode uses a dictionary and delta
offsets. The logos' scenes and the title cards are coded bitmaps: Metal Chrome 9730 bytes (vs
28800 unpacked), Cracktro Chrome 3971 bytes, title cards 19754 bytes (17729 coded data, 1440
row offsets, 360 cell bits, 225 glow levels). Generated by host tests from assets/logo/*.png.

## MIDI, SysEx and link

**The MIDI clock follower** estimates tempo and phase with a least-squares line through the last
96 clocks (four beats, `MIDI_CLOCK_WINDOW`): stamp = a + b × clock number. A fit, not a
second-order loop (PLL), because the fit's slope is a weighted average of the intervals, so it
never overshoots a tempo step or ramp where a PLL rings, it locks in known time with no state to
settle (exact for a steady clock however few clocks), and for the input's bounded white noise (TRS
poll's 0.5 ms, USB's 1 ms frames) a line's least-squares fit is the best unbiased estimate. Four
beats, not two: the bounds (0.1 % tempo, 1 ms phase) must hold from two beats after the clock
starts, when 48 clocks are fitted, and 48-clock fit is at its limit; clocks in USB's 1 ms frames
give up to 0.085 % tempo error at 200 BPM. Grown to 96, the error halves once four beats have run.
The cost: the fit lags a ramp by about two beats (the window's middle), settles a step within four,
and its ring takes 768 bytes. The x axis is the clock's number since start, so a clock left out
(a hold's) leaves a hole rather than shifting the clocks after it.

**Holds** mask flash erases (up to 35 ms): TRS bytes that arrive meanwhile are stamped late,
together, at the first poll after. The caller reports each hold (`midi_clock_hold`) before feeding
the messages it stamped late; clocks stamped from the hold's start to one clock period after its
end count for the song position but stay out of the fit. A clock more than 300 ms late is lost: the
fit is dropped, the next clock starts a new one.

**MIDI sync timing.** Every call lands `MIDI_SYNC_DELAY` samples (352, 8 ms) after its clock
reached the jack: the block being heard and the next (already rendered when the command is sent),
plus the stamp's age (the poll, the drain, up to 96 samples). A constant a sequencer can compensate
for. The stamps are fm1_ticks (24 MHz); the player counts samples. `midi_sync_anchor` ties the two
once a block: the block that starts at sample `samples` was rendered at tick `ticks`. The player's
samples stop while a flash hold masks the interrupts and the ticks do not, so the clock's fit is
kept on the ticks' time line and its times turned into the player's samples by the latest anchor.
The commands and clock out are in the player's samples.

**MIDI parsing** is a state machine, one byte at a time. Running status: channel messages may omit
their status. Real-time bytes (F8..FF) are reported where they occur, even inside another message
or a SysEx, and change no state. SysEx (F0 .. F7) is passed byte by byte to the callback:
`MIDI_SYSEX_START`, `MIDI_SYSEX_DATA` for each, then `MIDI_SYSEX_END` or `MIDI_SYSEX_ABORT` (the
status byte that cut it short; an F0 inside aborts it and starts a new one). Real-time inside it is
not part of it. System common (F1..F7) cancels running status. An incomplete message is dropped
when a new status byte arrives. Note-on with velocity 0 is reported as note-off.

**One parser per source** (TRS, USB), because sources interleave: a TRS message arrives a byte per
320 µs, so USB messages come between its bytes, and a parser shared by both would take a USB status
byte as the end of the TRS message. USB packets go through `midi_usb_bytes` first (the 32-bit word
read little-endian: CIN in bits 0..3, cable in bits 4..7, MIDI_0..MIDI_2 in bits 8..31; the CIN
gives the byte count), so both sources share the one parser implementation.

**The MIDI input ring** moves entries from the TIMER5 interrupt (polls USB and TRS UART at 2 kHz)
to the main loop. 256 entries: 256 TRS bytes are 82 ms at 31250 baud. Single producer, single
consumer, one core: the producer is an interrupt that preempts the consumer. Each index is written
by one side only; compiler fences keep an entry's stores before the index store that publishes it,
and its loads before the index store that hands its slot back. A full ring drops the new entry and
counts it in `lost`; so does the producer for input it lost before the ring
(`midi_ring_lose`). Each run of losses with no successful push between them is one gap and starts a
new `epoch`; every entry carries the epoch it was pushed in. The consumer sees exactly where input
went missing: between two entries whose epochs differ (by 1: a gap needs a push to end it), or
after the last entry if the ring's epoch is ahead of it. A gap also says whose input it lost (the
sources' bits: `in_gap` until the entry after it carries them), so a SysEx transfer on the other
port goes on. The USB side never loses: it leaves a packet in the endpoint (the host is NAKed and
retries) while the ring has less room than a packet's events.

**Lost input:** the parsers of the sources it lost restart, every note is released and the
controllers reset (bend centred, wheel and pressure 0) exactly where the gap is, so what came
before it is taken whole, what was cut by it is dropped (a message is never completed with bytes
from after the gap), and a note-off or a bend back to the centre that may be among the lost cannot
leave a note hanging or bent. The other source's parser goes on, so a SysEx frame on it is not cut
by the gap.

**MIDI out** is diff-based: `midi_out` keeps every parameter's value as it last saw it
(`live_param_get`) and compares. `midi_out_local` takes the changes since the last look as the
panel's (anything but MIDI input): they are pending. `midi_out_remote` takes them as MIDI input's:
the sender has the value already, so they are never echoed, on whichever port they came in (TRS or
USB), and a pending panel change of the same parameter is dropped (the last change was MIDI's). The
caller looks locally before taking MIDI input and remotely right after it, once for a whole pass of
input when nothing else changes a parameter meanwhile. `midi_out_tick`, once per 10 ms tick, takes
a local look, then sends what is pending with each parameter's value at that moment: at most one
message per parameter per tick, the latest value winning, at most `MIDI_OUT_TICK_PACKETS` (16)
packets a tick, round robin over the parameters so none starves; the mod target first, since it
says what CC 1 is (the pulse width or the wheel), and nothing before it while it waits for room.
What does not fit (the budget, or the room the caller has) stays pending, one flag per parameter:
nothing queues beyond that. Cold expert metadata or restoration halves the tick's packet budget.

**Encoding.** A parameter's CC, value `midi_map_to_cc`. A parameter of more than 128 values
(cutoff, pulse width, volume) goes as its NRPN instead, at full resolution, when the high-resolution
option is on. A parameter with no CC (the bend range, the expression targets) always goes as its
NRPN, and so does the pulse width while the mod wheel drives another target (CC 1 is then the
wheel's). An NRPN is four CCs: 99 the MSB and 98 the LSB of its number, 6 the value's MSB and 38
its LSB (value >> 7, value & 127), the order `midi_map_cc` applies at full resolution and, for a
small parameter, at the LSB. No null NRPN follows.

**MIDI learn** binds CCs to parameters at run time, over the published map, added to it (the
parameter's own CC still works); a learned CC wins (if it was another parameter's CC, it now drives
the learned parameter only); one CC per parameter and one parameter per CC (learning a parameter
again moves it to the new CC, learning a bound CC for another parameter moves the CC); the value
goes through the parameter's curve and range from its map row. Learnable CCs are 0..119 but bank
select (0, 32), data entry, increment, decrement, NRPN and RPN (6, 38, 96..101): `midi_map_cc`
takes those before any binding. Every lookup is O(1): a CC's binding is one byte of `cc_row` (its
map row), a parameter's one byte of `param_cc`. The bindings are omni (a learned CC means the same
on every channel and on TRS and USB). An armed learn with no step for 10 seconds is cancelled.

**NRPN data entry** (CC 6, 38, 96, 97) applies a parameter selected by CC 99/98 (or RPN 101/100).
For a parameter of more than 128 values (max >= 128) CC 6 applies the MSB at once (the LSB taken as
0) and CC 38 refines it. For a smaller one (max < 128: the value is all in the LSB) CC 6 only
stores the MSB and CC 38 applies the value, so a sender's 6 = 0, 38 = v never passes through 0 (a
chip model reset, a silent waveform, a cut envelope). CC 6 alone does not change such a parameter:
send CC 38, or the plain CC. `midi_map_cc` keeps the selection (CC 99/98, RPN 101/100) and data
entry state (CC 6/38, increment 96, decrement 97) per channel, for one input (a `midi_map_port`
each). The NRPN/RPN state is per channel and per source (TRS and USB separate).

**The published map** has one table row per parameter, with its CC (7-bit) and NRPN (14-bit),
range, curve and default. A CC sets the parameter through its curve: `LINEAR` spans the range,
value × max / 127 rounded (needs max >= 127); `STEPS` divides 0..127 evenly among the values,
value × (max + 1) / 128 (needs max < 128); `WALK` lands on the positions of the knob's walk, so the
CC moves as the knob does and 0..127 reaches every position. An NRPN carries the parameter's own
value, MSB × 128 + LSB, clamped to its range: the full 11 bits of the cutoff and 12 of the pulse
width.

**MIDI's modes** route input channels. OMNI (the default): every channel plays the instrument, on
the chip the keys play. LIVE: channel N alone plays the instrument; the others are ignored.
MULTITIMBRAL: channels N, N+1, N+2 play chip A's voices 1, 2, 3, each a jam voice of the player
with its own GoatTracker instrument (program change n is instrument n + 1), its own pitch bend (the
range per channel, RPN 0) and its own notes: one at a time, the last one held sounding (a release
goes back to the note held before it), with the LEGATO option a change of note while one is held
glides on without a new attack. Channel N+3 plays the instrument, on chip B. WHOLE-CHIP A: channel
N plays all three voices of chip A with one GoatTracker instrument (channel N's program): three-
voice polyphony, a new note taking the voice whose note started longest ago when all three sound.
Options: CHORD (one key plays the stored chord) and ARP (the held notes cycled on voice 1 in the
order they were pressed, a step every `arp_rate` player frames, or every 16th of the MIDI clock
with `arp_rate` 0, so voices 2 and 3 stay the song's). Channel N+3 plays the instrument, on chip B.
WHOLE-CHIP B: channel N plays the instrument on chip B (all three of its voices); chip A is left to
the song. In MULTITIMBRAL and both WHOLE-CHIP modes chip A belongs to the player, song or not. On
chip A's channels (the jam channels): notes 12..107 (GoatTracker's C-0..B-7) play; others are
ignored. Velocity is not used (GoatTracker's instruments have none). Pitch bend: ±the channel's
range (2 semitones, RPN 0 sets it per channel), applied at the next tick, coalesced. CC 74
(cutoff) and CC 71 (resonance) lock chip A's filter to the value, over the song's filter writes,
until CC 121 or a change of mode. Program change n (0..62) picks the channel's instrument n + 1;
others are ignored.

**Program change and bank select** (CC 0 MSB, CC 32 LSB) select a channel's bank, MSB × 128 + LSB,
held until changed (from either port; kept across lost input); a program change means what the
channel's bank says. Bank 0, the channel's sound: on the instrument's channel the instrument's
preset (0..LIVE_PRESET_COUNT − 1), on a chip A channel (MULTITIMBRAL, WHOLE-CHIP A) GoatTracker
instrument n + 1. Bank 1, songs: program n asks for the store's n-th song in name order, loaded
stopped, on the instrument's channel or channel N only. Bank 2, the chip's profiles: 0..4 the
built-in ones, on the instrument's channel live's profile (the keys' setup), on a chip A channel
chip A's (its want path); 5..12 USER 1..8 (the saved base and controls; an empty slot does
nothing). CC 0 and 32 are bank select on every channel the mode routes: MIDI learn leaves them
alone.

**The Link protocol** carries the screen to a Mac client over a SysEx stream (USB only, not
TRS) at 22 bytes per ms, burst 96. Commands are 7-bit encoded: a mask byte (high bit of the next
7 data bytes, bits 0..6) followed by the 7 data bytes with their high bits cleared, repeated; a
checksum (the two's complement of the sum, masked to 7 bits) follows. Frame: F0, magic (7D 'FM1L'),
command, payload (7-bit encoded), checksum, F7. Commands from the client: START (opens the link,
the device responds with HELLO, then sends the font, palette and screen), HEARTBEAT (the client
sends every 600 ms, the device responds with BEAT), STOP, KEYFRAME (the device resends everything),
FONT (the device sends the glyphs), PALETTE (the device sends the colours), PANEL (the client's
held keys and encoder steps). From the device: HELLO (the device's name, the font and palette
CRCs), BEAT, STOPPED, COLOURS (the 16 palette entries, 2 bytes each), GLYPHS (up to 5 glyphs, 8
bytes each), CELLS (a run of character cells: row, column, then records), PIXELS (a pixel cell: row,
column, record, then 32 bytes of 4-bit pixels). LINK_TIMEOUT_MS (3000) without a message from the
client closes the link. Panel keys held for LINK_RELEASE_MS (500) without a PANEL message are
released.

**SysEx protocol** (0x7D 'SD'): INQ (inquiry: the device replies with version, format, free space),
DIR (directory: the device sends a DIR message per file, then END), GET (the device sends the
file's metadata, then DATA messages of the file, then END), PUT (the host sends metadata, the
device acknowledges, the host sends DATA messages, then END; the device acknowledges each and
commits at END), DEL (the host names a file to delete), ABORT. Replies: ACK, NAK (with error
code), STILL (the device is busy, sent every 2 seconds during a long operation). A message times
out after 2 seconds. The frame: F0, 7D 'SD', command, sequence (0..127, wraps), payload (7-bit
encoded as Link's: mask byte + 7 data bytes, repeated), checksum (sum of command, sequence,
payload and checksum is 0 mod 128), F7. The sequence is the sender's, incremented per message; the
receiver's reply carries it back. A command after ACK or NAK that does not match the sequence fails
with SYSEX_SEQUENCE. Reserved names: "MIDI INSTRUMENT", "MIDI SONG", the sounds file; PUT or DEL of
them fails with SYSEX_MALFORMED. The device has one 256-byte packet buffer; PUT with the buffer busy
(another operation running) fails with SYSEX_BUSY. A .sid file's PUT is checked at the first DATA
packet before the file is opened (parsing the PSID header); RSID, MUS, broken and too-long files
fail there.

## The register ring

- The ring (`app/reg_ring.h`) carries every register write from the main loop to the audio
  interrupt, which drains it at the start of each block, so a chip only ever changes between two
  rendered blocks. Single producer, single consumer, one core: the interrupt preempts the main
  loop, never the other way round. Each index is written by one side only; a compiler fence keeps
  an entry's stores before the index store that publishes it. The fence is an empty `asm` with a
  memory clobber: the pi32v2 compiler lowers `__atomic_signal_fence` to a `__sync_synchronize`
  call that has no implementation there.
- Entries stay private until `reg_ring_commit`, once a pass, so the interrupt sees a pass's writes
  whole or not at all. A full ring never drops a write (a dropped one could leave a gate stuck or
  the wrong model): the sink commits and waits for the consumer, which on the device sleeps until
  the next interrupt. So the producer must never run with interrupts off once the audio runs.
- 1024 entries, 2 bytes each. The worst main-loop pass is 449 (`REG_RING_WORST_PASS`): the
  instrument's 391 on one chip (`REG_RING_LIVE_PASS`), the song control's 28 on the other and a
  move of the keys' 30. The instrument's: a model change with its reset, gate reset and registers
  (27), ALGORITHM (1), a preset's registers (25), K1-K3 (9), two catch-up ticks (30), all 27 note
  keys pressed and released in the pass (27 × 11 = 297) and two character ticks (2). 27 × 11
  bounds the keys although a note-on can write 12: the pulse width and the cutoff are stale only
  after MIDI, a preset, a model change or a note of another velocity, and the panel's notes share
  one velocity. The host tests push this pass and check it needs no wait. The rest of the ring is
  MIDI's: the device's drain takes an entry only while the pass's worst case and the entry's still
  fit, which takes up to 19 program changes between two blocks where the TRS jack delivers 9. At
  512 entries it took 2, and a flood overflowed the MIDI ring in about 150 ms.
- Both chips share one ring, so a pass's writes to A and B stay in order. Bit 6 of an entry's
  register names chip B (A 0x00..0x18 and 0x80..0x89, B 0x40..0x58 and 0xC0..0xC9); the effect
  slots' pseudo-registers 0xA0..0xBF name no chip (one chain on the mix of both). The drain's own
  pseudo-registers are `REG_RING_CHARACTER` (0x88: commit a character stage) and `REG_RING_MUTE`
  (0x89: a tune's mutes and solos act on the chip, not on its writes; a swap carries the mute to
  the new sid). The drain dispatches with an `if` chain, not a `switch`: a jump table would be
  data in flash, which the interrupt's SRAM code must not read (`platform/fm1/app.ld`, emu-gate's
  `ram_audio.rs`).
- **Model changes.** Re-initialising a chip costs about 0.5 M instructions with a profile's tables,
  far more than a block allows inside the interrupt and more than a pass's 120k poll allowance. So
  there are three sids, chips A and B and a spare: the main loop prepares the spare a slice a tick
  (`reg_ring_prepare_step`, at most `REG_RING_PREPARE_SLICE` instructions, leaving MIDI's drain 12k
  beside the busiest view's 68k pass) while the chips play on, and the drain swaps it in when it
  reaches the reset entry, which live sends once the spare is whole. A reset with no preparation
  waits for the ring to empty and prepares at once. At most one slice runs a pass, and a pass that
  ran one starts no DAC table; requests faster than a preparation (about 70 ms) restart it, so the
  chip swaps once they stop. Chip B is made by its first reset, prepared like any other in the
  main loop's first ticks: an unprepared one would move the boot's timeline.
- **Character.** The sink keeps the character's settings and applies them in the main loop, never
  waiting for the interrupt: all at once to the spare before a swap, and to a playing chip a tick at
  a time into one of two stages, which the drain commits at `REG_RING_CHARACTER`. The producer
  writes a stage only while its pending flag is clear and sets the flag before the entry that
  names it is committed; the drain clears it after the copy. With both stages queued the tick
  waits (`character_defers`). Everything the character sets therefore changes at one block
  boundary, in order with the writes around it, and the host's `live_sid_sink`, running the same
  ramp at the same ticks, makes the same chip.
- **DAC tables.** Each chip plays one table and a ramp builds into the third, one build at a time,
  so ramps on A and B take turns (a table is 4 ticks). A free table is one no chip renders from or
  will: `free_dac` reads the queued stages first, then the chips, because a commit writes the
  chip's table and then clears the stage's flag; read the other way round, a commit between the
  reads would hide a playing table from both (sidou-tests' `reg_ring.rs` plays that interrupt
  through `SIDOU_REG_RING_RACE_HOOK`). The 8580's default DACs are built on first use and the
  first 8580 preparation spends their slices whether done or not, so the slice count is the same
  on the host and the device.
- **The keys' chip goes first** (`reg_ring_set_keys`): a preparation of the other chip is dropped
  for its request, a table the other's ramp builds is taken back, and the other's work yields a
  pass to its own, so the keys' chip changes exactly as one chip alone would. Once the other
  chip's request has waited `REG_RING_YIELD_TICKS` (16) ticks, the keys' ramp starts no new table
  until it is served, so a DAC sweep on the keys cannot hold chip B's preparation off for ever.
- **Filter tables** never change on a chip that renders: age/heat is the stage's `filter_t`, and a
  profile's tables are built on the spare. Only `sid_update_filter` reads them, in the audio
  interrupt, and each entry is an aligned 32-bit store, so even `sid_set_filter_curve` on a
  playing chip gives an old or a new entry, never a torn one. That rests on every register write
  reaching a chip through the drain or from the audio interrupt itself: the song player writes
  chip A from the same interrupt between the drain and the render, and a swap of chip A gives the
  player its registers again through the swap hook before the block renders. A 6502 on the other
  core must keep to the same, writing only through the ring.
- A fresh sid's envelopes release from `SID_ENV_POWER_ON`. Chip A renders all the time and plays
  that out at once; chip B renders only while in use, so its release would play under its first
  user's registers (a 150 ms DC decay, a thud before the first note). Each sid made for chip B is
  set to where that release ends before any write, set rather than rendered: rendering the
  release (about 450 samples) took a pass past its instruction budget (`song_store.rs`).
