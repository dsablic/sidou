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
