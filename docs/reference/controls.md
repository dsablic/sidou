<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Controls

How the live instrument is played from the FM-1's panel and over MIDI. Button names are
fm1-emu's and Felucca's ids (`platform/fm1/VENDORED.md`); the printed labels may differ,
see [hardware-test.md](../hardware-test.md). The CHIP view (below) edits the chip's profile,
its character and the effect slots; the EDIT layer stays as a shortcut to the slots in every
view.

## At power-on

The full boot sequence (about 10 s) plays once when the saved firmware version is missing or
differs from the installed version. Once the current version has been saved, later boots
show only the title card (about 1.2 s). Both play over LIVE while the instrument already
plays. **Any button or encoder** skips the boot and does nothing else; **a note** (from the
keys or MIDI) skips it and plays. **HOME held while powering on** always plays the full
sequence; letting HOME go does not skip it, and anything after that does. Without HOME held,
a HOME press skips it like any button. Allow the settings save to finish before powering
off; a foreign store cannot remember the version ([Settings and STORE](#settings-and-store)).

**The first-run tour.** After that full boot, when the version changed and HOME was not what asked
for it, three large toasts lead the way, 1.6 s apart: **TURN K1** (until the cutoff or the
selected effect slot's amount changes, or 8 s), **PRESS ENV** (until the selected slot's
algorithm changes, or 8 s), then **HOLD HOME FOR HELP** (until a [help card](#help) opens, or
8 s). Doing what a toast asks ends it at once; a help card opened on the way ends the tour. The
tour is not saved: the version is, after the boot, so the next power-on has none, and a boot
with HOME held or a normal boot never has one.

**OCT- alone held while powering on**, for about a second, enters the chip's ROM boot mode
(UBOOT, `WL82 UBOOT1.00` on USB): the recovery route that needs nothing of SIDOU but its first
instructions. The screen stays black (the backlight goes off), and `tools/fm1-unbrick-mac` or
MvaveFM1Unbricker can then restore a package ([hardware-test.md](../hardware-test.md), section 0).
It is the first thing the firmware does at power-on, before memory, the boot guard, the
screen, sound, USB and the stored settings, so nothing they do can stop it. The keys are read
21 times over the second, and every reading must show OCT- and no other button or note key;
let go, or press anything else, and the boot goes on as usual (a normal boot spends 0.3 ms on
the first reading). Switching off and on leaves the ROM boot mode with nothing written. The
checklist's dying builds (`SIDOU_TEST_DEATH`) leave it out: there OCT- at power-on is theirs
([hardware-test.md](../hardware-test.md), section 10).

## Routing

Each button press and encoder turn goes, in this order (`app/views.h`):

1. **a capture:** a view waiting for one control (M1d's MIDI learn, the boot animation, a
   title card) sees everything first, HOME included, so HOME can cancel it; it ends when its
   view closes;
2. **global:** HOME's press only starts it, and on its release it opens the menu from LIVE and goes back one view anywhere else (held 600 ms it opens the [help card](#help) and the release does nothing); SAVE
   saves on a clean release, or held with a knob opens MIDI learn over any view; SAVE held
   with HOME consumes HOME ([SAVE](#save)); SEQ, PLAY and REC are the song views' ([below](#song-views)),
   in every view, but for PLAY while a `.sid` tune is loaded: it is the tune's
   ([SIDS and NOW PLAYING](#sids-and-now-playing));
3. **the view on top:** the menu, the MIDI page, CHIP and the song views take SELECT and ALGORITHM (INSTR only SELECT, with K1 and K2); SCENE takes SELECT and ALGORITHM;
   LIVE takes SELECT (the knob row's page), ALGORITHM (the profile, or a slot's algorithm), ARP (the [chord arpeggio](#chord-arpeggio)), K1-K3 while ARP is held and, on a slot's page, K1-K4; the status screen takes SELECT (its page); the glyph sheet and About take nothing; MIDI learn's frame passes
   them to the CHIP view under it, and to no other;
4. **the panel:** everything else plays the instrument as below, in every view. K1..K4 turn
   the page of the view that shows: CHIP's focused section on the CHIP view (on top or under
   MIDI learn's frame), LIVE's knobs in every other view.

The note keys and MASTER always play, whichever view shows, but for two layers before the
panel: **GLO held** (or latched) turns the leftmost six keys into the song's mutes and solos (a
`.sid` tune's while one is loaded), and in a **song
view** the keys audition the song's instrument ([Song views](#song-views)). The effect-slot shortcut (EDIT,
SCL, ENV) works in every view, the CHIP view included ([CHIP](#chip) says why it stays), except where
a view takes the knobs for itself: TRACKER with REC on (K1-K4 edit the cell under the cursor, and
record the filter while the song plays), STEP with a step held (the step's locks), and K1 and K2 on
the ORDER, INSTR and TABLE pages (the entry's value). STEP's EDIT + SCL and EDIT + ARP are DICE and
MUTATE ([STEP](#step)).

## Which chip the keys play

SIDOU has two SID chips, A and B. The panel and MIDI play **the keys' chip**:

- **no song playing:** chip A, exactly as with one chip;
- **a song playing on chip A:** chip B, on top of the song (KEYS **AUTO**, the default), or
  chip A with KEYS **A** (SETTINGS): a key then takes a voice from the song while it sounds, the
  song keeps the filter and the chip's volume register (K1 and K2 do nothing on it; MASTER
  still scales the whole output, song included) and its chip setup.

The keys' setup (the chip model, the profile, the character controls and the envelope mode)
travels with the keys: chip B always holds it, prepared in the background after every change,
so a key sounds the same on either chip. FX, the CHIP view's profile and character, and the
chip CCs change the keys' setup. When a song starts the keys move to chip B (held keys sound
again there); when it stops they move back to chip A once it has the keys' setup again and no
key is held. LIVE says **KEYS:B** in its bottom band while the keys play chip B; the status
screen shows both chips and marks the keys'. With KEYS **AUTO** (the default) the keys play chip B
while a song plays and chip A otherwise; with KEYS **A** they stay on chip A. KEYS persists with
the settings.

### SPREAD

**SPREAD** in SETTINGS sets the two chips apart in stereo, as on a two-SID (HardSID, 2SID)
setup: chip A to the left, chip B to the right. **OFF** (0, the default) is mono, the sound as
it always was; ALGORITHM steps it by 10 % to **100%**, where chip A is on the left alone and
chip B on the right alone. Between, each chip moves out from the centre at equal power (it is
as loud at every setting; at 100 % its side is +3 dB, the other silent). SPREAD separates the
chips, so it acts only while both sound: a song on both chips, or a song on A with the keys on B,
sits apart, and a chip sounding alone (a song with no keys, the keys with no song, chip B out of
use) stays in the centre at any setting, as at OFF. The separation glides in over about 0.1 s
as the second chip starts and back out once one has been quiet for about 50 ms (a chip sounds
while one of its envelopes is above about -24 dB, and stops under about -36 dB), so the gaps
between a phrase's notes leave the image still. CC 117 and NRPN 0/4 set it from MIDI, 0..100 % at full
resolution ([MIDI](midi.md)); it is saved with the settings (schema 6), not with a preset.

The effect slots keep it: their dry sound stays spread, and their wet sound is made from both
chips together (as the FX slot's always was: [decision record 0004](../decisions/0004-effect-algorithms.md),
"The output stage: CHIP SPREAD"), so a reverb or a delay adds its own stereo tail around the spread pair,
and a slot at Mix 127 hears the two chips as one (a fully wet distortion is mono). The chips'
thump stays in the centre.

## UI sounds

**SOUNDS** in SETTINGS (OFF by default, restored with the settings) turns
on short sounds on chip B's spare voice: a blip as the menu's cursor moves, a confirm as
ALGORITHM changes a value or opens a view, a chime with a toast, a buzz when an action is
refused (the cursor already at the menu's end, an action turned counter-clockwise), and a
drive "chunk" when SAVE saves. They never take a voice the keys use: with the keys
on chip A they play on chip B; with the keys on chip B (during a song) only on a voice whose
note has finished its release, and a note that needs that voice takes it back at once, ending
the sound. Off, they write nothing and chip B stays out of use.

## Panel

| Control (id) | Does |
|---|---|
| Note keys (14..40) | play F3..G5 at the current octave, at velocity 127: they sound as velocity off does, whatever the preset's velocity target |
| OCT- / OCT+ (buttons 0, 1) | octave -1 / +1 (-3..+3); their LEDs show the direction |
| FX (button 2) | chip 6581 / 8580; lit for the 8580 |
| K1 (encoder 2) | cutoff: 64 detents from a semitone above the chip's lowest cutoff to its highest, even in pitch (on the CHIP view: its focused section's first value, [below](#chip)) |
| K2 (encoder 3) | resonance (CHIP: the second) |
| K3 (encoder 4) | **the sound's own control**: RATIO on a sync sound (SYNC LEAD: the sync ratio, -12..+12 semitones from the preset's, a semitone a detent; the panel's alone, with no CC), PULSE on a sound with the pulse waveform (PWM BASS: the pulse width, 31 positions from 3 % to 50 %, 1.6 % a detent, clockwise wider; its modulation sweeps around that width, shallower near 3 % so it never goes thinner than 3 %), DECAY on the others (SAW LEAD, NOISE PERC: 0-15) (CHIP: the third) |
| K4 (encoder 5) | the release, 0-15 (CHIP: the fourth) |
| **PRESETS (encoder 6)** | the preset: clockwise the next (DICE after NOISE PERC, then the USER sounds [LAB kept](#lab), the empty slots skipped), clamped at both ends; with EDIT held it puts the current preset back to its factory settings (a USER sound, to the sound as kept); in every view but where the view takes the encoder: TRACKER with REC on, STEP and INSTR always (the lane's or the song's instrument), ORDER with EDIT held (inserts or deletes an entry) |
| MASTER (pot) | volume: a digital gain on the final output, for everything that plays (the keys, a song on either chip, UI sounds), on an audio taper from -60 dB to full in even steps of dB, shown in percent of the pot's travel |
| **EDIT (button 6) held** + K1 / K2 / K3 / K4 | the selected effect slot's **Amount / A / B / Mix** (32 detents end to end), in every view but TRACKER with REC on, STEP with a step held, and K1 and K2 on ORDER, INSTR and TABLE (the view's own, [below](#song-views)); K1-K4 are the knobs above again once EDIT is let go. EDIT is lit while the selected slot has an algorithm |
| **SCL (button 3)** | selects the next effect slot: Distortion, Modulation, FX |
| **ENV (button 4)** | the selected slot's next algorithm (back to OFF after the last); it starts from a musical setting, so it is heard at once ([Effect slots](#effect-slots)) |
| **HOME (button 8)** | acts when it is let go: the menu, from LIVE; back one view from any other (the menu, SETTINGS, CHIP, the song views, the status screen, SCENE, the glyph sheet, About); from the menu back to LIVE (a wipe; behind THE OSCILLATOR's title card only with CARDS at ALWAYS). **Held 600 ms** it opens the [help card](#help) of the view that shows instead, and letting go then does nothing; a toast raised meanwhile waits and shows once the card closes |
| **SAVE (button 9)** | clean press and release: on every list its **OK**, opening or choosing the highlighted item: the menu's item (nothing on a value item; on SETTINGS it only confirms STORE's format or RESET), [SONG](#song)'s action, the file LOAD or DELETE lists, a confirmation's answer (NO goes back a page, as HOME); save a USER profile on CHIP's chip focus, the song in a song view, settings elsewhere; hold and turn K1..K4: MIDI learn for that knob ([SAVE](#save)) |
| **SEQ (button 11)** | opens the song views, and steps through them ([below](#song-views)) |
| **PLAY (button 12)** | plays the loaded song from the song views' position, and stops it (each voice's release rings on); with EDIT held, goes on from where it stopped; lit while it plays. While it plays the keys play chip B (KEYS AUTO), or take voices from the song on chip A (KEYS A). A song the device cannot play is refused with a toast (`SNG: MULTISPEED 8 (MAX 4)`) |
| **REC (button 13)** | record mode on and off (outside the song views a toast says **REC ON** or **REC OFF**) |
| **GLO (button 7) held** + note key 1..3 / 4..6 | mutes voice 1..3 / solos voice 1..3 of the song (again: back), in every view; those six keys sound nothing while GLO is held; **the other keys play** (a change from before: held and latched are one rule) |
| **GLO tapped twice** (within 400 ms) | **latches** GLO's layer: the keys are the mutes and solos without holding GLO (a toast says `GLO LATCHED`, GLO's LED is lit, and LIVE shows the layer); a tap on GLO ends it (`GLO OFF`) |
| **SELECT (encoder 0)**, in the menu or SETTINGS | moves the selection (a line in reverse video) up and down the items; the list scrolls with it |
| **SAVE**, in the menu | the menu's **OK**: opens the item under the selection: CHIP the CHIP view, TRACKER the song views, SCENE the generative pieces, SONG the [song-file actions](#song), SETTINGS the settings page, STATUS the status screen, MIDI the MIDI page, CHARSET the glyph sheet, ABOUT the credits. On PRE or OCTAVE it does nothing (and saves nothing: the settings save themselves) |
| **ALGORITHM (encoder 1)**, in the menu | changes the item under the selection, which the [popup](#value-popup) also says in full: PRESET (PRE in the menu), OCTAVE; turned clockwise on a view's item opens it, as SAVE does |
| **ALGORITHM (encoder 1)**, in SETTINGS | changes the item under the selection, which the popup also says in full: KEYS, SOUNDS, TRACKER ROWS (ROWS), CARDS ([title cards](#title-cards)), STORE ([format confirmation](#settings-and-store)), THEME, REC QUANTISE (REC), [SPREAD](#spread), [RESET](#settings-and-store) |
| **ARP (button 10)**, on LIVE | the [chord arpeggio](#chord-arpeggio) on or off, when ARP is let go; **held**, K1..K3 turn its rate, order and span instead (the knob row shows ARP's page), and the release then switches nothing |
| **SELECT (encoder 0)**, on LIVE | pages the knob row: TONE, DIST, MOD, FX; on a slot's page K1-K4 turn that slot's Amount, A, B and Mix and SCL moves the page with the slot; it stops at either end |
| **ALGORITHM (encoder 1)**, on LIVE | steps the chip's profile on TONE (R2, R3, R4AR, R5, WORN; it stops at either end; the band's profile word shows the change), or the selected slot's algorithm on a slot's page or with EDIT held |
| **SELECT (encoder 0)**, on CHIP | moves the focus: A/B tabs, CHIP (the profile and the character), DIST, MOD, FX, EXPERT; it stops at either end |
| **ALGORITHM (encoder 1)**, on CHIP | steps the focused section's choice: the profile on CHIP, the algorithm on a slot, or the four-control page on EXPERT; EDIT held on CHIP or EXPERT selects the USER save target |
| **SELECT (encoder 0)**, in TRACKER | moves the cursor one row, or one 8-row page with EDIT held; stops at the first and last body rows |
| **ALGORITHM (encoder 1)**, in TRACKER | the shown voice: voice 1, 2, 3 and round; the cursor's voice and the audition always match the voice on screen |
| **SELECT (encoder 0)**, in SCENE | the next piece clockwise, the one before counter-clockwise (FIRE, PLASMA, RULE 30, STARS, ANT, RED MOON RITE, and round): it wipes in, and a toast names it |

LFO, ARP and PRESETS (encoder 6) edit TRACKER with REC on (below), and STEP (ARP is the lane's gate,
LFO clears a held step, PRESETS the lane's instrument) and ORDER (ARP sets the restart) use them too;
on LIVE, ARP is the [chord arpeggio](#chord-arpeggio); elsewhere LFO and ARP do nothing yet, and
PRESETS steps the preset. SELECT does nothing on ABOUT (on STATUS it turns the
[page](#status-pages)), ALGORITHM nothing on STATUS and ABOUT.

## The button LEDs

Lit is active now; dim, a glow about a thirtieth as bright (the hardware's own), is set or
available. A key that is held lights, as always.

| LED | What it shows |
|---|---|
| OCT- / OCT+ | lit while the octave is below / above 0 |
| FX | lit while the 8580 plays |
| EDIT | lit while the selected effect slot has an algorithm |
| **ARP** | dim while the [chord arpeggio](#chord-arpeggio) is on; lit for the first half of each step. At 1 and 2 frames a step it flashes once a cycle instead (faster would read as flicker) |
| **PLAY** | dark stopped; while the song plays, dim, and lit for the first quarter of each beat |
| **REC** | dim while record mode is on; blinking through the count-in; lit while recording |
| GLO | lit while its layer is latched |
| **SAVE** | blinks while a question waits for SAVE to confirm (MIDI learn's clear-all, the song menu's confirmation, SETTINGS' armed STORE format) |

With GLO held or latched, the note keys 1-6 show the voices: keys 1-3 are lit while their voice
sounds and dark while it is muted, keys 4-6 blink for a soloed voice and are dim otherwise.

In [STEP](#step) the 16 leftmost note keys show the page instead of what is held: a lit key is a
step with a note, a dim one an empty step (a short last page leaves the rest dark), and the key of
the step the song plays flicks, inverted for 50 ms, as the song reaches it. The keys above 16 light
as they are held.

Blinks run at about 2.8 Hz. The flash hold of an update or a store erase puts every LED out, as
before.

## Value popup

A change of a parameter **from the panel** opens a large overlay over the view on screen, in any
view: the parameter's name at 3 times the font (24-pixel characters, 8 to a row; 2 times, 12
to a row, for a name or value longer than 8 characters), its value as large, and for a
continuous value a bar. It opens 3 rows a frame, stays while the control keeps changing, goes
**1 s after the last change** (3 rows a frame), and the view under it is back as it was. What
opens it, and what it says (the text the views show, from one place, `popup_text` in
`app/views.c`):

| Control | Popup |
|---|---|
| K1 / K2 on LIVE's page (every view but LIVE and CHIP, which show the knob instead: below) | CUTOFF (in Hz as LIVE's FILTER frame says it, `11.4K`), RESO (0-15) |
| K3 on LIVE's page (the same views) | the sound's own control: DECAY (0-15), PULSE (a percent of the full width), or on a sync sound RATIO (`+3`, in semitones) |
| K4 on LIVE's page (the same views) | RELEASE (0-15) |
| PRESETS (encoder 6) | PRESET and the preset's name (no bar), in every view but LIVE (whose header says it) |
| K1-K4 on CHIP's EXPERT section | the named expert control in its units: percent, DAC ratio `R`, voice-DAC DC units `DC`, Hz, WEAK / MED / STRONG, or noise write-back tap count |
| EDIT held + K1-K4 with the focus on CHIP's chip section | the slot and its control in the algorithm's own word: `DIST DRIVE`, `MOD DEPTH` (0-127) (the slot is not on that page) |
| EDIT held + K1-K4 on STATUS, the menu and SCENE | the same |
| MASTER | VOLUME (0-100%, the bar the pot's travel), in every view, once the pot turns by about 1.5 % of its travel (a pot at rest never shows it) |
| ALGORITHM + EDIT on CHIP | the USER slot to save to (`SAVE TO`, `USER 2 -`) |
| ALGORITHM in the menu or SETTINGS, on an item with a value | the item in full and its value: `PRESET`, `OCTAVE`; in SETTINGS `KEYS`, `SOUNDS`, `TRACKER ROWS`, `CARDS`, `STORE`, `THEME` (`HI-CONTRAST` or `C64`), `REC QUANTISE`, `SPREAD` (`OFF`, `10%` .. `100%`), `RESET ALL?` (`SAVE RESETS` once asked) |

**LIVE and CHIP show a turned knob where it is instead of opening the popup over it** (the popup
would hide the very controls and meters being tuned): the knob's label (on LIVE, in row 15; on
CHIP, its label under the choice) is shown in reverse video, in the knob's own colour, for a
second after its last detent. The profile word and the selected slot's algorithm on LIVE do the
same when ALGORITHM changes them, and on CHIP the choice at 3x is the display of ALGORITHM. A
control the slot does not use stays dim. MASTER keeps its popup everywhere. STATUS, the menu and
SCENE keep every popup.

SELECT never opens it (it moves a cursor or a focus, which the screen shows); the buttons
(OCT-, OCT+, FX, SCL, ENV) say what they changed in a toast, as before, and a control's popup
replaces its toast; **a toast closes a popup that is open** (the two would stack over the same
rows). LIVE does not toast the selected slot or a slot's algorithm: its bottom band shows both
at 2x. Nothing opens under the boot, a title card or MIDI learn's frame (a knob there
does what it did), and a view change takes an open popup away at once. **MIDI never opens
it**: a controller sweep or a song would flood the screen. That is one switch,
`VIEWS_POPUP_MIDI` in `app/views.h` (0), or `views_popup_midi` at run time: on, a change of the
cutoff, resonance, pulse width or volume that no panel control made opens it as well.

**Toasts** use the large text too: 2 times the font on the two rows above the border, up to 13
characters; longer ones break at a space into two lines (up to 26 characters), and one whose
words fit neither (`SNG: A-4 PITCH NOT PLAYED`) shows in the font's own size on one row.

## Song views

The song views make and play a song (M2d): **STEP**, **TRACKER**, **ORDER**, **INSTR** and
**TABLE**, all on the same GoatTracker 2 song (`app/song_view.h`), the one the [SONG](#song)
menu loaded (the last song at power-on). Make a song with the keys on STEP, with the cursor and
the keys on TRACKER, or by recording it ([below](#recording-while-the-song-plays)); a song made
here opens in GoatTracker. TRACKER (`app/tracker_view.h`; docs/art/README.md "TRACKER" shows it)
is the patterns in GoatTracker's layout, **one voice at a time at 2× the font** (12 rows) with the other two as activity strips on
the right, and the shown voice's orderlist and the bar row's command. **SELECT moves the row;
ALGORITHM steps the voice shown** (the cursor's voice: the note keys audition on it, below).
Its rows follow the song while it plays (with REC on too: what is recorded is seen landing),
the cursor while it is stopped; with REC on and the song stopped the edit cursor shows. The moving bar over still rows (the
**TRACKER ROWS** in SETTINGS: BAR, the default) or scrolling under a still bar (SCROLL, as
GoatTracker does). TRACKER ROWS is saved with the settings.

| Control | Does |
|---|---|
| **SEQ** | from any view, opens the song view used last over it (STEP the first time after power-on; HOME goes back to that view); in a song view, the next one: STEP, TRACKER, ORDER, INSTR, TABLE and round. The menu's TRACKER opens TRACKER. Not over [ABOUT](#song)'s tune |
| **PLAY** | in every view: plays the song from the orderlist entry the song views show (the cursor's), or stops it (releases ring on). While it starts (the keys moving off chip A, [which chip the keys play](#which-chip-the-keys-play)) it shows as playing; PLAY again stops it |
| **EDIT held + PLAY** | goes on from where the song stopped (from the cursor if it never played) |
| **PLAY**, **EDIT held + PLAY**, a `.sid` tune loaded | the tune's: see [SIDS and NOW PLAYING](#sids-and-now-playing). The song views do not open while it plays (`SID PLAYING: STOP TO EDIT`); one that showed when the tune took the song's memory says **SID PLAYING / STOP TO EDIT** and takes no edits. With the tune stopped, SEQ or the menu's TRACKER gives it up for the song (`LOADING`: press again once it is back) |
| **REC** | record mode on and off: REC in red on the song views' top line. In TRACKER with the song stopped it enables note entry ([Editing TRACKER](#editing-tracker)); with the song playing, the keys and K1 and K2 are recorded ([below](#recording-while-the-song-plays)). STEP needs no REC to edit; REC there records |
| **SELECT**, in TRACKER | moves the row; with EDIT held, 8 rows (a page) |
| **ALGORITHM**, in TRACKER | steps the voice shown, V1, V2, V3 and round; with EDIT held, the edit step (below) |
| **HOME held** | the help card of the song view shown, in its state (a step held, REC on: [Help](#help)) |
| **GLO held + note key 1, 2, 3** | mutes voice 1, 2, 3 of the song, or lets it sound again |
| **GLO held + note key 4, 5, 6** | solos voice 1, 2, 3, or unsolos it: while any voice is soloed the soloed ones sound and the rest are silent; with none, the muted ones are silent |
| **Note keys**, in a song view | audition the song's current instrument on the cursor's voice through the player, on chip A with the song's setup, at the panel's octave, one note at a time (the last key wins). With the song stopped the audition takes chip A for the song (the keys elsewhere move to chip B, as for PLAY; the first note sounds a few tens of milliseconds late) and gives it back two seconds after the last note is let go; no row of the song plays |
| **SAVE** | a clean release saves the song; an unnamed song opens SAVE AS. Held + K1..K4 opens MIDI learn |

GLO's layer works in every view and comes before the panel: one of the leftmost six keys
pressed while GLO is held (or latched) sounds nothing until it is let go, even if GLO is let go
first; the other keys play, and a key already sounding when GLO goes down sounds on. The mutes
and solos show in TRACKER's header (the shown voice's number grey, with MUT in red or SOL in white
beside its pattern) and on the other two voices' strips (grey, short while silent). **LIVE shows
them too**, and the song: its top line keeps the preset's name and, while a song plays, a **play
mark** at 2× that beats (yellow for the first 120 ms of a bar, white for a beat, grey between); row 20
says the tempo (`125 BPM`) and the voices' **badges** `1 2 3` (dark grey while a voice is silent,
reversed yellow while it is soloed), shown while a song plays or a voice is muted or soloed; and
while GLO's layer is on (held or latched) the bottom band becomes the layer: `GLO  MUTE SOLO` (or
`GLO LATCHED`) and six boxes at 2×, **M1 M2 M3 S1 S2 S3**, red when a mute is on and yellow when a
solo is.

TRACKER's header (rows 0-1): the shown voice **V1** at 2× in its colour (light red, cyan, light
green), its pattern **PAT 03** at 2× (hex), a play mark or a stop square, the tempo in BPM small (the
frame rate times the play routine's calls a frame, over the calls a row and the rows a beat, 4;
funktempo's two speeds averaged: tempo 6 on PAL is 125.3 BPM, shown 125) and REC in red. Row 2
numbers the strips. The rows: 12 pattern rows of two cell rows, each its number small (yellow on
the current row), the note at 2×, the instrument at 2× in the voice's colour and the command
small; the current row a dark grey bar. At the foot: the orderlist from the shown voice's entry,
the current row's first command spelled out, and **ORD 03 ROW 1A** (hex: the player's while the
song plays, the cursor's while it is stopped). The other two voices are the **strips** on the
right: a tall bar where a note sounds on that row of that voice's own pattern, a short one where
none does, in the voice's colour (dark grey while the song mutes it).

### STEP

STEP (`app/step_view.h`; docs/art/README.md "STEP" shows it) is the song made like a drum
machine: **the 16 leftmost note keys are steps 1-16** of the selected voice's pattern, and a
**held step plus a knob is a parameter lock**. It edits the same pattern TRACKER shows, so a
song made here opens in GoatTracker. The voice is the song views' voice (the lane marked by the
arrow); a page is 16 rows of its pattern (the pattern's length says how many pages: 64 rows,
4 pages); B is chip B's lane, greyed until M5. It needs no REC. The pads under the lanes are the
16 keys in beats of four, numbered by beat (1 2 3 4, four times; the second and fourth beats
shaded, each beat's first number brighter); a held step's number in the pattern shows in the
HELD STEP frame (`HELD STEP 22`).

| Control | Does |
|---|---|
| Step key 1-16, pressed and let go | toggles a note at that step on the selected lane: the lane's note (C-3 until PRESETS with a step held changes it) and instrument; off writes a rest. The key also auditions the step's note, as TRACKER's keys do. A key held with a knob turned on it is a lock (below) and toggles nothing |
| SELECT | selects the lane, V1, V2, V3 and round |
| ALGORITHM | pages through the pattern; while the song plays the page follows the playing step |
| PRESETS | the lane's instrument, 01-3F, its name in the popup (`03 KICK`: a NEW song's drum kit, [SONG](#song)); with a step held, that step's note (and the lane's note for the steps toggled after it) |
| ARP | the lane's gate: **STACCATO** (the default) or **LEGATO**, shown under the lane |
| Step held + K1 | cutoff **LOCK** on that step: 32 of 2047 a detent, from the middle (1024) |
| Step held + K2 | resonance LOCK, 0-15 |
| Step held + K3 | pulse width LOCK, 64 of 4095 a detent from the middle (2048) |
| Step held + K4 | waveform **SET**: OFF, TRI, SAW, PULSE, NOISE |
| Step held + EDIT + K1-K4 | the character's LOCKs: age/heat, DAC, thump, bleed (0-127) |
| Step held + ALGORITHM | the step's **chance** to play on each pass, 100 % (always) down to 0 in steps of 5; the popup says `CHANCE 75%`, the frame the number beside the note, the notch is yellow (it is a lock) |
| Step held + LFO | clears that step's locks, its chance and its SET |
| EDIT + ARP | **MUTATE**: two small changes to the lane's pattern, a note nudged to the next note of the scale or taken away, or a ghost step of the scale added; toast `MUTATED` |
| EDIT + SCL | **DICE**: a new pattern for the lane, notes of the scale on a rhythm grid; toast `NEW PATTERN` |
| EDIT + LFO | undoes the last cell edit, then redoes it (one step, as TRACKER's; the help card says E+LFO ONE UNDO) |
| Keys 17-27 | audition, as in the other song views |
| GLO + a step key | the mute or solo of the layer; no step toggles |

**Two kinds, one meaning each.** A **SET** is GoatTracker's own `7XY` command written into the
step's row (a row with a command already refuses it: `ROW HAS A COMMAND`): the player holds the
waveform from that row until the next waveform change, as GoatTracker plays it, and a song with
SETs plays in GoatTracker as it does here. It shows as `SET` in cyan, and on the step as a cyan
notch. A **LOCK** is SIDOU's: one parameter replaced for that row only (the `LOCK` chunk,
[song format](song-format.md#sidous-trailer)); the song's own value is back at the next row. It
shows as `LOCK` in yellow, and on the step as a yellow notch (both: the yellow one, and the
frame says both). A persistent cutoff change is TRACKER's `CXY` command.

**Chance, MUTATE and DICE (generative play).** A step with a chance below 100 plays that often
on each pass of the pattern: the roll is made as the row before it starts, from a seed PLAY takes
anew, and a step that loses it plays as a rest (its note is dropped; commands, timing and the
gate rows stay). The chance is a lock of the `LOCK` chunk (parameter 9), so it moves with its row
under row inserts and deletes, undo restores it, and GoatTracker plays the step always. MUTATE and
DICE write ordinary cells through the editor, one row a turn like every STEP edit, so the lane's
pattern stays plain GoatTracker data (undo steps back one cell at a time, as for any edit); both
need no step held and no REC (they do nothing while recording: REC on, the song playing and any count-in over), and they stop if STEP is left before they finish. The notes are the
**minor pentatonic** on the lane's note (the one PRESETS sets with a step held, C-3 by default) and
the octave above; a lane the SCNE chunk calls DRUMS keeps its one note and varies only where it
hits. DICE puts hits on the eighths for a melodic lane (the beat 85 %, the offbeat 55 %) and on the
sixteenths for a drum lane (90, 15, 50, 25 % through the beat), a key off after each hit on a
staccato lane; a row's command and data stay. The seed comes from the time of the press, so each
press is another pattern.

**Gate.** A staccato lane writes a key off on the next row if that row has no note, and takes
it away with the note; where the row before a removed note holds a note, the removed note's row
becomes that note's key off. A legato lane writes nothing but the note. Both are real key-off
rows, so GoatTracker plays them the same.

The edits go to the player one at a time between its blocks; the first key after a pause starts
the audition, which holds edits for a short time. Taps and knob turns in that time wait in order
(at most four besides the one being written; a fifth is refused with `WAITING`). Refusals use
the large toast (`SAVING`, `SONG FULL`, `INVALID EDIT`, `NO SONG`, `LOCKS FULL`).

### Editing TRACKER

**REC on** enables pattern editing. The row number is a yellow block; the selected note or
instrument is reverse yellow at 2×. When editing a command or its data, the whole command/data
field expands to 2× in reverse yellow. Other voices remain activity strips.

| Control | With REC on |
|---|---|
| Note key | writes its note and the current instrument into the cursor's row, auditions it, then advances by the edit step (wrapping within the pattern) |
| ARP | writes key off (`===`), keeping the row's other fields |
| LFO | clears the note to a rest; a second press on the same row clears all four fields |
| EDIT + LFO | undoes the last edit; the next press redoes it (also available with REC off). Undo keeps **one step**: the help card says E+LFO ONE UNDO |
| K1 | instrument number, 00–3F (00 leaves the playing instrument unchanged) |
| K2 | command, 0–F |
| K3 / K4 | data's high / low nibble, independently clamped to 0–F |
| PRESETS | current instrument for note entry and audition, 01–3F, its name in the popup |
| EDIT + PRESETS | clockwise inserts an empty row and drops the tail; counter-clockwise removes this row and appends a rest; pattern length stays fixed and locks follow their rows |
| EDIT + ALGORITHM | edit step, 0–16; 0 stays on the same row |

K1–K4 show the instrument, command or data being set in the large value popup. Navigation and
the step/current-instrument selectors do not change the song. With REC off keys only audition
and K1–K4 keep their LIVE functions. An edit waits for the player's acknowledgement before it
can be undone. While arena compaction runs, further edits wait: **TIDYING**. Refusals use large
toasts: **SAVING**, **SONG FULL**, **INVALID EDIT**, or **WAITING** while another edit completes.
While a save runs the header says **SAVING** and edits are refused (see [SONG](#song)); an edit
that would take the song past 65,536 bytes saved says **SONG FULL**.

### ORDER, INSTR and TABLE

SEQ cycles **STEP → TRACKER → ORDER → INSTR → TABLE → STEP**.
These pages share TRACKER's high-contrast, 2× values and bounded redraws. They edit the same
song through the player's queue; no separate instrument or table copy is edited. Values may
wait for acknowledgement. **SONG FULL**, **SAVING**, **SONG BUSY**, **TIDYING** and invalid-edit
refusals use the large toast. K1/K2 show the actual edited value in the value popup.

| Page | Control | Does |
|---|---|---|
| ORDER | SELECT | moves between the subtune selector, entries and the restart marker |
| ORDER | ALGORITHM | selects the subtune on its selector; elsewhere changes the shown voice |
| ORDER | K1 / K2 | changes the entry's value / kind (pattern, repeat, transpose) |
| ORDER | EDIT + PRESETS | clockwise inserts an entry; counter-clockwise deletes it |
| ORDER | ARP | sets the restart to the cursor's entry |
| ORDER | PLAY | plays from the selected subtune and cursor entry; again stops |
| INSTR | PRESETS | selects instrument 01–3F |
| INSTR | SELECT | selects one of the nine parameters, then a character of its 16-byte GT2 name |
| INSTR | K1 / K2 | coarse (16) / fine (1) adjustment of the selected byte or name character |
| INSTR | EDIT + SEQ | on WAVE/PULSE/FILTER/VIBRATO pointer, opens that table at its pointer; otherwise asks SELECT A PTR |
| TABLE | ALGORITHM | selects WAVE, PULSE, FILTER or SPEED |
| TABLE | SELECT | selects table row 01–FF |
| TABLE | K1 / K2 | edits the left / right byte |
| TABLE | ARP | on the WAVE table: opens the [ARP helper](#arp-helper) on the arpeggio at the cursor row (a new one on a blank row; **NOT AN ARP** when the rows are anything else, **WAVE TABLE ONLY** elsewhere) |

ORDER shows one voice's decoded entries at 2×; the other two have compact raw-hex summaries.
Repeats use `R0` for 16 times; transpose `-10` means minus 16 semitones (hex), not minus zero.
Choosing an unheld pattern creates an empty 64-row pattern before publishing its reference;
the whole request is checked against the serialized-size cap first. Invalid repeat/transpose
sequences are refused without creating a pattern. Instrument names use GT2's 16 raw bytes;
untouched imported bytes are preserved, and name-only instruments follow GoatTracker's normal
save rules. TABLE's footer spells out the selected operation, including absolute/relative
wavetable notes and jumps (SPEED's FF is a speed value, not a jump).

### ARP helper

An arpeggio in GoatTracker's wavetable is relative note steps, a delay between them and a jump
back; entered by hand they are rows of raw bytes. TABLE's ARP helper writes those rows from a
list of semitone offsets, a speed and a loop step, and reads them back. The song stays plain
GoatTracker 2: the helper adds no engine and no chunk, only wavetable rows.

On the WAVE table put the cursor on the arpeggio's first row (from INSTR, EDIT + SEQ on the
WAVE PTR lands there) and press **ARP**. A blank row opens a new arpeggio (a major chord, 2 ticks
a step); rows the helper wrote, or hand-made ones of the same shape, open as that arpeggio;
anything else says **NOT AN ARP** and nothing changes. The page shows the number of steps, the
speed and the loop step at 2x, the offsets four to a line (the step the cursor is on in
reverse, the loop step in yellow), the waveform that is kept (`WAVE 41`) and where SAVE writes.

| Control | Does |
|---|---|
| SELECT | the step under the cursor |
| ALGORITHM | the number of steps, 1-8 (a new step copies the last) |
| K1 / K2 | the step's offset over the played note: an octave / a semitone, -64 to +63 |
| K3 | the speed: ticks a step lasts, 1-16 |
| K4 | the loop step: where the arpeggio returns to after the last (1 loops all of it); **HOLD** plays the steps once and holds the last |
| SAVE | writes the rows and closes the helper (**ARP SAVED**) |
| HOME | closes it without writing |
| keys | audition the instrument as in the table |

The rows: row 1 holds the waveform and the first offset; each later step is a delay row of
speed - 1 ticks (at speed 1 the waveform again) holding its offset; then a jump (`FF`) to the loop
step, or `FF 00` to hold. Looping back to step 1 at speeds over 1 takes one more row, because a
delay holds the note before it. Offsets are GoatTracker's relative notes (00-3F up, 40-7F down),
and the waveform is the one read (41, pulse and gate, for a new arpeggio): K1 on its table row
changes it. SAVE writes one byte at a time through the player's edit queue, as the table's
knobs do, only the bytes that differ, and blanks the rows a shorter arpeggio no longer needs.
It refuses with **ROWS IN USE** when it would write over rows past the ones it read that are not
blank (another program's), and with **TABLE FULL** at the table's end; the helper then stays open.
SEQ or any page change closes it too. Holding HOME shows its card.

### Recording while the song plays

With **REC on and the song playing**, TRACKER and STEP record what you play (the keys still
sound). Every write goes through the player's edit queue (cell writes) or the lock table, as
the views' own edits, and is refused with the same large toasts (`SONG FULL`, `SAVING`,
`WAITING`, `LOCKS FULL`).

| Control | Records |
|---|---|
| Note key, in TRACKER | its note and the current instrument on the cursor's voice, in the row the voice's player is on when the key goes down, or the next row if the key is in the second half of it (the nearest row); a note replaces the row's note and keeps its command; a key past the end of the pattern goes to the pattern that plays next (the same pattern again while an orderlist repeat replays it) |
| Step key (the 16 leftmost), in STEP | the selected lane's note and instrument the same way, with the key off after it on the next empty row on a staccato lane; the pad toggles nothing |
| Other keys, in STEP | their own pitch on the selected lane |
| K1 / K2 | cutoff (32 per detent) and resonance (1 per detent), starting from LIVE's values, as a lock on the row the knob was turned in (never rounded to the next row); the value popup says `CUTOFF` (in Hz, as LIVE says it) or `RESONANCE` |

SETTINGS' **REC** item (popup `REC QUANTISE`; not saved) chooses `NEAREST` (the default), `OFF`
(the row the key falls in) or `COUNT-IN` (the nearest row, after one bar of the song's tempo,
rows per beat times four, that records nothing: a toast says `COUNT-IN`; it starts when the
song starts with REC on in a song view, or when REC is turned on there while it plays; a song
view opened while the song already plays records at once). During a count-in TRACKER's K1 and K2
do nothing; STEP's act as in LIVE. While recording TRACKER shows the recorded voice's playing
pattern, its bar on the player's row, so the notes appear where they land; its edit cursor
stays where it was and shows again when the song stops. With the song stopped REC is TRACKER's
cursor entry, as before.

**Transposes.** A note is written as it was heard: on an orderlist entry with a transpose (ORDER's
`+n` / `-n`, which holds until the next transpose) the transpose is taken off the note, so the
pattern plays it back at the pitch that sounded. A note the transpose would take below C-0 or
above G#7 is refused with `NOTE OUT OF RANGE` (the key still sounds). The transpose is the one
the orderlist gives the entry on the song's first pass (the nearest transpose before it).

The keys sound during a count-in but record nothing. Recorded notes go to the editor one cell
write at a time, with at most four waiting (a fifth key is refused with `WAITING`; a staccato
STEP key's key off counts as a second write). A key's time is read where the audio block starts,
about 3 ms, with no compensation for the keys' own delay or the sound's. With REC off, or the
song stopped, nothing is recorded (REC stays on). What was recorded is the song's pattern and lock table, which
SAVE saves like any edit.

The note keys audition the selected instrument on the cursor's voice. The edit cursor's
subtune and the actually playing subtune are tracked separately, so selecting another subtune
does not silently restart playback or make TRACKER show the wrong orderlist.

The audition keeps what already sounds. An instrument the song never uses, a voice the packed
song omits, a note outside the song's range or a table of another encoding is prepared in
slices between frames and handed to the player at a block boundary, so the song keeps its
position and every held note and table its phase. A key you press still starts its note
normally, and an edit made while a note is held changes that note the way the edit says. With
the song stopped, only the voices you jam sound; the song's own notes never play.

## Title cards

A major view entered from the menu opens behind a short title card in the logo's metal
lettering (`app/title_card.h`): **THE CHIP** before CHIP, **THE SCENE** before SCENE, and,
with CARDS at ALWAYS, **THE OSCILLATOR** before LIVE when HOME goes back to it from the menu
(not at power-on, after the boot, nor when MIDI learn's frame closes). The glow fades in, the
title wipes in, the card holds for a second and the view cuts in, about 2.6 s in all. By
default (ONCE) each of THE CHIP and THE SCENE plays the first time you open that view after
power-on and not again; the way back to LIVE never plays a card, it is a wipe.

- **Any button or encoder** ends the card at once and does nothing else (FX does not toggle
  the chip, OCT+ does not step the octave, SELECT does not pick a piece or move CHIP's focus).
- **A note**, from the keys or MIDI, plays as ever and ends the card. The keys, MIDI and the
  audio are never held up by it.
- **CARDS** in SETTINGS is **OFF**, **ONCE** or **ALWAYS** (ALGORITHM clockwise steps OFF,
  ONCE, ALWAYS, counter-clockwise back, and it stops at the ends). **ONCE** is the default:
  THE CHIP and THE SCENE the first time each since power-on, none on the way back to LIVE.
  **ALWAYS** plays all three every time; **OFF** none. The setting persists across power-off.

## Help

**Hold HOME for 600 ms** in any view and a help card opens over it: the title bar (**HELP** and
the card's name, `app/help_view.h`), then up to eight lines at 2×, the control on the left and
what it does on the right. It is the one explanation on the device (the views draw no help line
of their own), so every view and page the panel reaches has a card of its own: LIVE, the menu,
SETTINGS, the MIDI page (its setup is RAM-only), CHIP, SCENE, each of the five song views (STEP,
TRACKER, ORDER, INSTR, TABLE: SEQ goes round them and SAVE saves the song), STATUS, ABOUT, the
glyph sheet (CHARSET), the SONG menu's actions and each of its pages: the list of files LOAD
and DELETE browse (**FILES**), SAVE AS's name and a confirmation; and the SID views: the browser
(**SIDS**), **NOW PLAYING** (its card is **PLAYING**) and the browser's question (**UNSAVED**).

**The card is the one for the state the view is in when HOME has been held**, where a control
means something else there:

| View, state | Card | What changes |
|---|---|---|
| LIVE on a slot's page (SELECT) | **LIVE SLOT** | ALG the slot's algorithm; K1..K4 named for that algorithm (OVERDRIVE: DRIVE, TONE, BIAS, MIX; UNUSED where it has none) |
| CHIP, the A/B tabs | **CHIP TABS** | ALG the tab; K1..K4 the chip's AGE, DAC, THUMP, BLEED |
| CHIP, the chip | **CHIP** | ALG the profile, EDIT + ALG the save target, SAVE stores it; K3 is NOTE THUMP (it changes where a note starts) |
| CHIP, a slot | **CHIP SLOT** | ALG the algorithm; K1..K4 named for it, as on LIVE SLOT |
| CHIP, EXPERT | **EXPERT** | ALG the page; K1..K4 as the page names them; MACROS RESET (a macro releases its overrides) |
| STEP, a step held | **HELD STEP** | K1..K3 the step's locks, K4 its waveform SET, EDIT + K the character's locks, PRESETS its note, LFO clears it, the key let go toggles it |
| STEP, REC on and the song playing | **STEP REC** | the keys record, K1 and K2 record locks; QUANT says SETTINGS' REC QUANTISE |
| TRACKER, REC on and the song stopped | **TRACK EDIT** | the keys write notes, ARP a key off, LFO clears, K1..K4 the row's instrument, command and data, PRESETS the instrument, EDIT + PRESETS inserts or deletes a row |
| TRACKER, REC on and the song playing | **TRACK REC** | the keys record, K1 and K2 record locks, QUANT the quantise; EDIT + K edits the cursor's row |
| SETTINGS on STORE | **STORE** | ALG arms a foreign store's format, SAVE formats (WIPES ALL DATA) |
| SETTINGS on RESET | **RESET** | ALG arms the reset, SAVE puts every preset and setting back to factory (KEEPS SONGS, PROFILES, MIDI MAP) |
| SIDS, a tune chosen over an unsaved song | **UNSAVED** | SAVE saves the song first, ALG discards its changes, HOME cancels |

**A card of more than eight lines has two pages**: its title bar says **1/2** (or **2/2**) where
it says HELP on a card of one page, and **SELECT** turns the page, either way; the page is drawn
as the card is, within the frame's 200 cells. LIVE, LIVE SLOT, CHIP, STEP and its states,
TRACKER and its states, ORDER and INSTR have two.

**Every control a view takes is on its card.** A host test (`help_view.rs`,
`every_control_a_view_takes_is_on_its_card`) opens each view in each state above, feeds the view's
own input handler every button and encoder, alone and with EDIT held, each on a fresh power-on,
and looks for a line naming each control the handler takes (E+ or EDIT+ names EDIT held; a control
alone also covers EDIT + it where the view takes both). It adds what acts outside the handler:
HOME everywhere, SAVE where it has a meaning of its own (a USER slot on CHIP, the menu's and SONG's
pages' OK, the song in a song view, SAVE AS's name, STORE's format), the keys and SEQ in a song view (EDIT + SEQ on
INSTR), and K1..K4 where the view turns the panel's knobs to its own page (CHIP). A view that
learns a control without a line fails it. A walk opens every state, holds HOME and reads the card;
each card (and each second page) has a golden (`tests/golden/lcd/help-*.png`).

- HOME only opens the menu or goes back when it is **let go before 600 ms**: a tap acts in the
  next frame after the release, so a hold never opens the menu first.
- **Any button or encoder** but SELECT on a card of two pages closes the card and does nothing
  else (SELECT does not move, FX does not toggle the chip, SAVE saves nothing). **A note** plays as ever, as the view under the card
plays it (over a song view its instrument, and REC records it), and the card stays. The card
  stays after HOME is let go until a control closes it.
- The screen stack is four deep, LIVE included (LIVE, menu, SONG, a song view), and keeps one
  slot more for the card: a held HOME shows the card over a full stack too. MIDI learn (SAVE + a
  knob) has no such slot: on a full stack it toasts `NO ROOM FOR LEARN`.
- On SONG's pages HOME acts on its release as everywhere (back a page; a hold shows the page's
  card).
- **SAVE is the OK of every list.** Wherever ALGORITHM clockwise opens or chooses the highlighted
  item (the menu, SONG's actions, LOAD's and DELETE's files, a confirmation's YES, SIDS' tunes), a
  clean SAVE does the same, and HOME goes back. The one exception is SIDS' question (UNSAVED),
  where the two are its two answers: SAVE saves the song, ALGORITHM discards it. A host test (`help_view.rs`,
  `save_opens_the_highlighted_item_on_every_list_screen_and_home_goes_back`) walks every state
  above and checks both, so a new list is covered without being named.
- Over the boot, a title card and MIDI learn's frame HOME keeps its own meaning (it ends the
  boot or the card, or cancels the learn frame): no card there.

## CHIP

CHIP (the menu's first item) is a chip's sound a section at a time (`app/chip_view.h`; [the art
page](../art/README.md#chip) draws it): its profile and character, or one of the three effect
slots, for chip **A** or chip **B** (the title). What a tab shows and edits follows [which chip
the keys play](#which-chip-the-keys-play):

- the keys' setup (live's), on both tabs, with no song: chip B follows it, so its note says
  **FOLLOWS KEYS**;
- during a song, the A tab is the song's chip: it shows the song's setup and edits it (the
  profile and K1..K4; MIDI learn binds nothing there, since MIDI reaches chip A on its own
  only from M2e), and the B tab shows the keys' setup and edits it.

The effect slots are one chain on the mix of both chips: they show on both tabs, and the B
tab's note says **SLOTS SHARED**. **KEYS:A** or **KEYS:B** at the top right marks the tab of the
chip the keys play.

| Rows | Shows |
|---|---|
| 0-1 | the title **CHIP A** or **CHIP B** at 2× (yellow while the focus is on the tabs, below), **KEYS:A / KEYS:B** small on the right on the keys' chip |
| 2 | the section tabs **CHIP, DIST, MOD, FX**: the focused one yellow (the CHIP tab grey instead while the focus is on the A/B tabs, whose K1..K4 turn the chip's) |
| 4-6, 7 | the section's choice at 3×, and under it its names in the hint colour: the chip's model (6581, 8580) and profile (R2, R3, R4AR, R5, WORN, or **USR 3** when USER 3 was picked) over MODEL and PROFILE; or the slot's algorithm over ALGORITHM. With a save target the names read **SAVE TO USER 3** in yellow (**SAVE TO USER 3 -** while the slot is empty) |
| 9-14 | the section's four controls in the knobs' places (K1..K4 left to right, as LIVE's knob row): its label small in the knob's readable text colour (AGE, DAC, THUMP, BLEED; a slot's named for what they do in its algorithm: OVERDRIVE has DRIVE, TONE, BIAS, MIX), its value 0..127 at 3× in white (2× from 100: three digits do not fit a column), a bar in the knob's accent colour and the knob's name K1..K4 in the neutral hint colour. A control the algorithm does not use, and every control of a slot that is off, has dark grey labels, values and bars; the knob tags remain readable |
| 16-21 | the other sections at 2×, a line each: DIST, MOD and FX with their algorithms (OFF dark grey); with a slot in focus, the chip's model and profile and the other two slots |
| 27 | the tab's note (SONG SETUP  SLOTS SHARED, FOLLOWS KEYS  SLOTS SHARED, SLOTS SHARED WITH CHIP A; nothing on chip A with no song) in the hint colour; what the controls do is the [help card](#help)'s, by the focus |

The focus is on the tabs, the chip, a slot or EXPERT; the yellow tab (or title) shows it.

| Control | On the tabs | On the chip | On a slot |
|---|---|---|---|
| SELECT | moves the focus: the tabs, CHIP, DIST, MOD, FX, EXPERT (it stops at either end) | the same | the same |
| ALGORITHM | the B tab clockwise, the A tab counter-clockwise | the next profile clockwise, the one before counter-clockwise: the five built-in ones, then USER 1..8 (it stops at either end) | the slot's next algorithm clockwise, the one before counter-clockwise (it stops at OFF and at the last) |
| EDIT held + ALGORITHM | | steps the **save target**: USER 1..8, or OFF (below) | |
| K1 / K2 / K3 / K4 | as on the chip | age / DAC sloppiness / output thump / filter bleed (32 detents end to end) | the slot's Amount / A / B / Mix (32 detents end to end) |

The slot in focus is the selected slot, the one SCL selects and the EDIT shortcut turns:
SELECT onto a slot selects it (and the toast says so, SLOT MOD), and SCL on a slot moves the
focus with it. A profile changes from the next 10 ms tick, as its CC does (a chip reset, once
the new chip is prepared, 70 to 110 ms later, the first 8580 after power-on about 150 ms:
held notes attack again on the new chip); the
page shows it at once, the model follows it, and the FX button still toggles the model. The
character's controls glide, as from MIDI ([Chip character](#chip-character)). Everything else
plays as on LIVE: the note keys, MASTER, OCT-/OCT+, FX, SAVE (save settings, or held + a knob
for MIDI learn, which picks the parameter K1..K4 turn on the page: AGE for K1 on the chip),
and HOME back to the menu. A change
from MIDI shows on the page at the next frame.

**EXPERT.** One SELECT stop after FX opens four pages of independent controls; ALGORITHM
steps pages, clamped at either end. K1 and K2 are the upper pair, K3 and K4 the lower pair,
each explicitly tagged K1..K4 in the neutral, readable hint colour. Names are at 2× in the
physical knob's accent colour; values, including their units, are at 2× or 3×. Empty positions
on the last page do nothing. Each detent moves
one raw step out of 127; the popup names the parameter, shows its value in units and keeps
the theme's popup colours. EDIT + K1..K4 still turns the selected effect slot.

| Page | K1 | K2 | K3 | K4 |
|---|---|---|---|---|
| 1/4 | 6581 curve | 6581 range | 8580 curve | nonlinearity |
| 2/4 | filter bias | filter bleed | DAC kink | DAC leakage |
| 3/4 | combined waveform strength | noise write-back | output thump | external low-pass corner |
| 4/4 | external high-pass corner | voice DC offset | unused | unused |

Curve and range are normalized percentages of the model's setting coordinates, not Hz.
Nonlinearity shows 100% for a factor of one; bleed and leakage show fractions as percentages.
`R` is the DAC's 2R/R ladder ratio; `DC` is a voice-DAC unit, not volts. External corners show
Hz. Combined strength uses three bands of the raw range (WEAK, MED, STRONG); noise write-back
shows OFF or the number of connected taps (1 TAP through 8 TAPS). Model-specific controls
remain named on both models; an inapplicable control's name and value are dim and it says
**N/A** on the page, while its knob tag remains readable.
Its popup also says **N/A**.

Expert edits stay until a related macro moves: AGE releases the two curves, range and
nonlinearity; DAC releases kink and leakage; THUMP releases output thump; BLEED releases filter
bleed. Other independent settings stay. DAC's bit errors have no expert control of their own:
they follow DAC whatever kink and leakage are set to. A new base profile releases all expert overrides.
EXPERT's help card says it: **MACROS RESET**. During a song the A
tab edits the song's expert setup, not the keys'; its knobs cannot be MIDI-learned. B edits
the keys' setup.

**USER profiles.** A USER profile is a base profile, the four character controls and the
expert overrides (including values that follow the profile/macros),
stored by name USER 1..8 ([format](profile-format.md)). The picker (ALGORITHM on the chip)
runs the five built-in profiles, then the slots that hold one; an empty slot is skipped, and
a step that finds only empty ones to its end stays where it is and says which: **USER 4
EMPTY**. Picking one applies it as any profile change does (the spare chip is prepared a
slice a tick, then swapped in; its four controls glide), and the choice's profile word reads **USR 3** (the popup says **USER 3**)
until another profile or the model (FX) is chosen. CC 85 reaches them too, above the
built-in ones (values 5..12; an empty slot does nothing).

To store the sound: with the focus on the chip or EXPERT, **EDIT held + ALGORITHM** steps a **save
target** through USER 1..8 and OFF without changing the chip, shown in yellow under the choice
(**SAVE TO USER 3**); the target clears on the A/B tabs, an effect slot, or when the view
closes. **SAVE** then stores the tab's base profile, four controls and expert overrides into the target, or,
with no target, back into the USER slot the tab was picked from; with neither it saves the
settings, as elsewhere ([SAVE](#save)). A slot that holds a profile asks first
(**OVERWRITE USER 3?**): SAVE again stores, HOME keeps it. The toast **SAVED TO USER 3** comes
when the store commits (**SAVED TO USER 3 (GAPS)** when held flash operations were forced, as
for the settings). On a song's chip A the tab's setup is the song's (not the keys', and not a
parameter lock's value), and picking a USER there sets the song's setup.

**The EDIT layer stays, as a shortcut.** EDIT + K1..K4, SCL and ENV were the slots' stopgap
until this view. They stay in every view, because they are a player's: they turn an effect
while the scopes and the keys stay in sight, one hand on EDIT, without the three presses and
the title card that reach CHIP; they cost nothing and conflict with nothing (EDIT is a hold
nothing else uses, and on the CHIP view they act on the same selected slot, so the page shows
what they turn). The CHIP view is where the slots are seen and named in full. M2 may give SCL
and ENV the jobs their names suggest (a scale, an envelope); then the shortcut keeps EDIT +
K1..K4 and the view keeps the rest.

## LAB

LAB (the menu's LAB, after SONG; `app/lab_view.h`; [the art page](../art/README.md#lab) draws
it) is where one LIVE sound is shaped with every parameter in sight
([LAB's design](../plans/2026-10-25-lab-design.md)). The menu's LAB takes the menu's place, so
**HOME** goes back to LIVE, the sound kept as edited. It opens at OSC.

**SELECT** steps the five pages, clamped at both ends: **OSC**, **ENVELOPE**, **FILTER**,
**MOD**, **FX**. **ENV**, **LFO** and **FX** open ENVELOPE, MOD and FX (in LAB they are pages:
the panel's next algorithm and chip toggle do not run there). On every page:

| Rows | Shows |
|---|---|
| 0-1 | the page's name at 2× in yellow, its place (`2/5`) on the right |
| 2-3 | the sound's name at 2× in white, `*` while it differs from the sound as loaded; **A** or **B** on the right, reversed, while EDIT's tap compares |
| 4-17 | the page's drawing |
| 19-20 | ALGORITHM's choice at 2× (dark grey while it is off), ALG small on the right (ENVELOPE's also ALL SOUNDS) |
| 22-27 | the four cards in the knobs' places, as LIVE's knob row: the card's name small in the knob's colour, its value at 3× in white (2× when it takes three characters), a bar and the knob's name |
| 29 | the hint line: LAB's gestures, a phrase every 2 s (`EDIT A/B  +LFO UNDO  +ARP VARY`, `EDIT +SCL ROLL  +SAVE NAME`, `SAVE KEEP  SELECT NEXT PAGE`) |

A card or the choice that turns is lit (its name in reverse video) for a second instead of the
value popup; PRESETS opens no popup either (the header names the sound).

| Page | K1 | K2 | K3 | K4 | ALGORITHM |
|---|---|---|---|---|---|
| OSC | PULSE: the pulse width, LIVE's walk (`50%`); RATIO on a mono-sync sound without the pulse bit (SYNC LEAD), the sync ratio's semitones | PWM: the pulse-width LFO's depth, 32 steps a detent (`25%`) | PWM HZ: its rate, 0.05 Hz a detent | TUNE: the fine tune, a cent a detent (`+20`) | TRIANGLE, SAW, PULSE, NOISE, TRI+SAW, TRI+PULSE, SAW+PULSE, TRI+SAW+PUL, TRI RING, SAW SYNC, PULSE SYNC |
| ENVELOPE | ATTACK | DECAY | SUS (the sustain level) | RELEASE (0..15, a step a detent) | INSTANT, AUTHENTIC: the envelope mode, the instrument's for every sound (ALL SOUNDS) |
| FILTER | CUTOFF: LIVE's K1 walk, the cutoff the chip has now | RESO | VOICES: the voices routed to the filter (`123`, `-` none) | free | LP, BP, HP, NOTCH (from another mode MIDI set, the next one the turn's way) |
| MOD | VIB HZ: the vibrato's rate | DEPTH | DELAY | VELO: the velocity target (OFF, SUS, CUT, PW) | ARP OFF, ARP UP, ARP DOWN, ARP UP-DOWN, ARP PLAYED |
| FX | the selected slot's Amount | A | B | Mix (named for its algorithm, as on CHIP; dark grey where it has no use; a slot's detent) | the slot's algorithm, OFF included |

EDIT held turns the cards as without it. **SCL** selects the next slot, as everywhere (on FX the
page follows it). The keys play and the song plays on as everywhere: OCT, PLAY, SEQ, REC, GLO,
MASTER and PRESETS do what they do in every view.

**SAVE** keeps the sound with one press, whatever the page. A USER sound is kept in its own
slot (its name stays); any other sound goes to the lowest free slot of the sixteen as `LAB 01`..
(its number is the slot's), and from then on it is that USER sound: its `*` goes, it is the
sound as loaded. The toast, once the store has written it: `KEPT AS U03 LAB 03` (a new slot) or
`KEPT U03 LAB 03` (in place), with `(GAPS)` when held store operations were forced; the store's
refusals read `STORE FOREIGN: NOT SAVED`, `STORE ERROR: NOT SAVED` and `STORE FULL: NOT SAVED`.
A second SAVE while one is being written toasts `SAVING: WAIT` and writes nothing. With all
sixteen slots full, SAVE on a sound that is not a USER one toasts `USER FULL: EDIT+SAVE` and
writes nothing (a USER sound still keeps in place). SAVE held while a knob turns is MIDI learn
for that card's parameter, and SAVE + HOME is learn's clear, as everywhere.

**EDIT + SAVE** (EDIT held first, as the help card teaches) opens the **name page**, a mode of
LAB (`KEEP AS`, everything at 2x): the slot (`SLOT U03`, with `FREE` or `USED` and a used slot's
name under it) and the name, twelve characters with the cursor reversed. It opens on the sound's
own slot with its own name, or on the lowest free slot as `LAB nn` (the first slot when all are
full: K1 chooses). **K1** turns the slot (the automatic name follows it until the name is the
player's); the **note keys** type at the cursor (the first 26 keys A to Z, the 27th a space) and
step it on, making no sound; **SELECT** moves the cursor; **ALGORITHM** changes the character
under it (space to underscore, as SAVE AS does); the first typed key on an automatic name starts
a new one. **SAVE** writes it (the spaces around the name trimmed; none left, the automatic
name): a slot that holds a sound asks once more (`OVERWRITE U03?`, `SAVE AGAIN`; any other
control cancels the question). The toast is the keep's. **HOME** leaves the page without writing
(the next HOME is LAB's: back to LIVE).

**EDIT tapped alone** compares (A/B): on an edited sound the edit (**B**) is put aside and the
sound as kept or loaded (**A**, the `*` gone) plays, the header showing `A`; the next tap plays B
again (`B`), and so on. It acts when EDIT is let go, and only if nothing else was pressed or
turned while it was down (the note keys do not count, so a note can be played while it is held):
EDIT + a knob, LFO, ARP, SCL, SAVE or PRESETS are combinations and never compare, and EDIT keeps
its meaning everywhere else. On a sound that is its base it toasts `NOTHING TO COMPARE`. A card
or the choice turned while A plays makes A the edit: comparing ends there, and B's difference
from it is one undo step (below), so B is not lost. Comparing also ends when another sound loads
(PRESETS, a program change).

**EDIT + LFO** undoes, sixteen steps back (`UNDO (3)`: the steps left; `NOTHING TO UNDO` when
none are). A step is a card's run of turns (until another control is used: one detent or twenty,
one step), a run of ALGORITHM (ENVELOPE's, the envelope mode, too), **EDIT + PRESETS** (the revert
to the sound as kept or loaded) and A becoming the edit. The seventeenth step drops the oldest.
The steps last across HOME and back; another sound loading forgets them (undoing past a load
would mix two sounds). There is no redo. While A plays, EDIT + LFO gives B back.

## SCENE

SCENE (the menu's item after STATUS) is M1e's prototype of M2's visuals: a
generative piece on the whole screen that reacts to what the instrument plays
(`app/scene.h`), and RED MOON RITE, a drawn duel the song plays (below). SELECT and ALGORITHM step between the pieces (the same step; a toast names the piece, on entry too); everything else plays as on
LIVE, so the keys and MIDI move the piece, and K1 (cutoff) and K2 (resonance) set
its mood. The piece starts again from its beginning each time it opens.

| Piece | Note-ons | Envelopes (ENV3) | Pitch | Cutoff, resonance |
|---|---|---|---|---|
| FIRE | flare that voice's flame (voice 1 the left third, 2 the middle, 3 the right) | heat its flame; silent, it burns low: a flame a cell row or two high, red, never dark | higher notes burn higher | the cutoff how hot it looks (red, up to yellow and white near the bed); the resonance a little taller |
| PLASMA | drop a bright ring that grows from a place | speed it up (silence runs it at an eighth) and voice 3's warms it | the pitch class sets the ring's place across, the pitch its height | the cutoff its speed, the resonance its twist |
| RULE 30 | step the automaton a generation in the voice's colour; voice 3's flips a cell first | a held voice keeps it stepping | voice 3's pitch picks the cell it flips | a low cutoff darkens the colours |
| STARS | light the voice's third of the stars in its colour | speed the voice's stars | the latest note steers the centre across | the cutoff sets the cruising speed |
| ANT | walk the voice's ant three steps | a sounding voice keeps its ant walking; silent, it stops and greys | | a low cutoff darkens the trail |

A voice that makes no sound (SYNC LEAD's voices 2 and 3) counts as silent. The voices'
colours are their waveforms' (pulse cyan, saw yellow, triangle light green, noise light
grey), as on LIVE.

### RED MOON RITE

The sixth piece (M2f) is a drawn duel of two barbarians under a blood moon
([art](../art/README.md#red-moon-rite)), played by the song while one plays (PLAY, button
12), else by the keys. What moves it (`app/scene_song.h`, `app/scene_rmr.h`):

| The music | The duel |
|---|---|
| a kick | the leader raises his sword and chops overhead |
| a snare | the other parries, a spark where the blades meet, and the leader loses a notch of stamina |
| a hi-hat | both shift between their two guards |
| the bass | a high note gives the right barbarian the lead, a low one the left, and the pair drifts toward the one giving ground |
| the fill, the last beat of every fourth bar | the leader's spinning attack, which disarms the other: his sword spins away and sticks in the flagstones; on the first beat after it sticks he pulls it free. Nobody attacks while he is unarmed |
| an arp or lead note | the torches flare |
| the cutoff (K1, or the song's filter) | the sky's palette: low dark, high bright, wiped down a row a frame |
| a bar's first row | an empty stamina blade fills again |

**With a song:** the voices' roles and the drums come from the song's `SCNE` chunk
([song format](song-format.md)), else they are guessed: a voice whose instrument's wavetable
starts with noise is the drums (a kick under MIDI note 100, a hat from 106, a snare between,
by the first wavetable step's note), the lowest of the others the bass, the rest the arp.
A row is each row of voice 1; the bar has 4 beats of the `MIDI` chunk's rows a beat (4 by
default). The bass is high above the middle of its range over the current and the last bar.
A muted voice plays no part: muted drums stop the chops and parries.

**With the keys:** a note-on on a voice whose waveform has noise (NOISE PERC) is a drum by its
pitch, the lowest note held is the bass, the others the arp; a free clock makes a row every
3 frames while a note is held, so the fill comes as you play. A second after the last
release the torches hold still, and once the duel has settled nothing changes.

## Presets

SAW LEAD, PWM BASS, SYNC LEAD and NOISE PERC (PRESETS, the big encoder, or the menu's PRE; program
change 0..3). SYNC LEAD plays
one note at a time: voice 3 runs silently at the note's pitch and voice 1, hard-synced to
it, 2.75 times higher, so voice 3 sets the pitch and the ratio the timbre.

**Each preset keeps your edits** while another plays: turn PRESETS away and back and the
cutoff, resonance, pulse width, waveform, envelope, filter, expression settings, sync ratio and
[chord arpeggio](#chord-arpeggio) you had are back (the effect slots, the chip and the volume are not the preset's and stay as
they are). A preset that differs from its factory settings shows a `*` after its name on
LIVE's header, in the popup and in the menu's PRE (`SAW LEAD*`). **EDIT held + PRESETS** puts
the current preset back to its factory settings and forgets its edits. A MIDI program change
loads the factory's settings over any edits the preset kept, as before. The memory is not
saved: a power cycle brings back the preset that was current, with its edits as the settings
keep them (the settings record is unchanged), and the other presets' edits are gone. On MIDI
out, PRESETS sends the program change and then, as CCs, the restored parameters that differ
from the preset's factory values.

**The USER sounds** (the sounds [LAB keeps](#lab), U01..U16) follow DICE: PRESETS clockwise from
DICE goes to the lowest USER sound, then the next, the empty slots skipped; anticlockwise goes
back; the last one clamps (DICE alone still rolls again at its end). A USER sound loads as a
preset: its sound, its three effect slots and its name (the header, the popup and the menu's PRE
say it, `LAB 03` or what it was named, `*` once edited) and the voices released; a built-in or
DICE after it leaves the slots as they are, as before. **EDIT held + PRESETS** puts a USER sound
back to the sound as kept. The edits of the USER sound last left (one shared place, not one a
slot) come back when it is loaded again, until another USER sound is left edited; a built-in's
kept edits are never touched by it. A MIDI program change knows the built-ins only.

**Known limit:** voice 1's frequency register tops out (0xFFFF, 3.85 kHz) from note 90
(F#6), and from lower notes bent or with vibrato upward (note 88 at a bend of +2
semitones, lower at wider bend ranges). Voice 1 then stays there: the pitch is still
right, but the ratio falls, and the timbre thins, toward the top.

### DICE

One more place on PRESETS, after NOISE PERC: **DICE, a new sound at every turn**. Turning
clockwise from NOISE PERC lands on DICE and rolls a sound; clockwise again, while on DICE, rolls
another at each turn (the header says `DICE 07`, the number of the roll, two digits, counting from
power-on). Counter-clockwise goes back to NOISE PERC; clockwise again brings back the last roll,
not a new one. A roll picks a waveform (saw, pulse, triangle or noise; ring on some triangles,
sync on some saws and pulses), an envelope, a cutoff in the middle of K1's range, a resonance, a
filter mode (mostly low-pass, sometimes band-pass, high-pass or a notch) with every voice in the
filter, a pulse width on a pulse, and now and then a vibrato. It leaves the effect slots, the
chip, the octave and the volume as they are: they are the instrument's, not a preset's. The
knobs turn a roll like any sound, and DICE shows no `*`. EDIT held + PRESETS on DICE puts back
its base (SAW LEAD's settings) and forgets the roll.

DICE is the panel's: a MIDI program change selects 0..3 only (a program of 4 or more is refused),
and on MIDI out the roll goes as CCs, parameter by parameter, with no program change. The
settings keep the roll's parameters like any sound's and bring DICE back on power-up (its number
starts from 0 again).

## Chord arpeggio

The C64's signature sound, playable: **hold a chord and one voice steps through its notes at the
frame rate**, the buzzy arpeggio C64 tunes play a major chord as (0, 4, 7) because the chip has
three voices and the tune needs the other two. **ARP** on LIVE switches it on and off (on its
release); while it is on a badge at 2× under LIVE's band says so, readable at arm's length:
`ARP 3F UP 1OCT` (ARP in yellow, then the rate, the order and the span in white).

- **One voice.** The first key held starts a voice on the cycle's first note (an attack as
  usual); each key after it joins the cycle at once, keeping its time, and every note change is
  the voice's frequency alone: the gate stays on, so there is no new attack and no click, and
  the change lands between two of the chip's rendered blocks (2.9 ms) at a 100 Hz tick. Letting
  go of a key takes it out of the cycle (the note that sounds plays out its step); letting go of
  the last releases the voice with the preset's release, at the pitch it had. The other two
  voices are free: nothing else plays on them, and over a song on chip A (KEYS A) the arpeggio
  takes one voice from it, not three. Its lane on LIVE shows the note it steps to.
- **ARP held + K1: the rate**, the frames a note: `1F` (50 Hz, a C64 player's frame), `2F`
  (25 Hz), `3F` (16.7 Hz, the default: each note of a held chord is heard, where 1F sounds like a
  wobble), `4F`, `6F`, `8F` (6.25 Hz); then the clock's, `/32`,
  `/16`, `/8`: a note every 32nd, 16th or 8th of a MIDI clock while one comes in (a start, FA,
  puts the next note a whole step away), and as many frames as the clocks (3, 6, 12: a 125 BPM
  clock) while none does.
- **ARP held + K2: the order**: `UP` (lowest to highest, the default), `DN` (down), `UD` (up and
  down, the top and bottom notes once each) and `AS` (as played: the order the keys went down).
- **ARP held + K3: the span**: `1` octave (the chord) or `2` (the chord, then again an octave
  up). A change of order or span keeps the note that sounds and goes on from it in the new
  cycle; a change of rate restarts the step's count.
- **ARP held + K4** does nothing. While ARP is held the knob row is ARP's page (RATE, ORDER,
  SPAN), a turned knob's label reversed for a second as on the TONE page.
- **Switched on with keys held,** their notes become the cycle, on the voice of the oldest (its
  gate kept); the others are released. **Switched off with keys held,** each held note gets a
  voice again (a new attack), as if played again in the order played.
- **Per preset:** the arpeggio's settings, on or off with them, are edits of the preset's
  ([Presets](#presets)): PRESETS away and back keeps them, they show the preset's `*`, and EDIT
  + PRESETS, or a MIDI program change, loads the factory's (off). Like the other presets' edits
  they are not saved with the settings: the arpeggio is off at power-on.
- **MIDI:** notes held over MIDI arpeggiate as the keys do, with them; up to eight notes held
  (a ninth drops the oldest). CC 27 switches it (on from 64), CC 28 sets the rate (0..127 over
  the nine rates), CC 29 the order and CC 30 the span (2 octaves from 64): [midi.md](midi.md),
  Chord arpeggio. MIDI out does not send them.

## Effect slots

Distortion, Modulation and FX run after the chip, in that order, in stereo
(`core/fx/fx_slots.h`; what each algorithm does: [decision record 0004](../decisions/0004-effect-algorithms.md)).

| Slot | Algorithms (ENV order) | Amount | A | B |
|---|---|---|---|---|
| Distortion | OFF, TAPE, OVERDRIVE, FUZZ, CRUSH, LO-FI | drive (CRUSH: bit depth; LO-FI: TV tone) | OVERDRIVE tone, FUZZ octave, CRUSH rate, LO-FI hum | OVERDRIVE bias, FUZZ starved bias, LO-FI noise |
| Modulation | OFF, CHORUS, FLANGER, PHASER, TREMOLO, AUTO-PAN, WOW | depth (AUTO-PAN width; WOW wow) | rate (WOW: flutter) | CHORUS ensemble, FLANGER and PHASER feedback, TREMOLO and AUTO-PAN shape |
| FX | OFF, DELAY, PING-PONG, REVERB, LUSH | DELAY feedback, REVERB and LUSH decay | DELAY time, REVERB and LUSH damping | DELAY damping, REVERB diffusion, LUSH bandwidth |

Mix is dry/wet. Every control is smoothed, and changing algorithm crossfades through
the dry sound (slots take turns, so a full change takes up to ~150 ms). Everything is off
by default; saved slot settings are restored at power-on.

**An algorithm picked from the panel starts from a musical setting** (ENV, and ALGORITHM on
CHIP's slot line), so it is heard at once. The slot takes the algorithm's four values when it
is still silent (Amount and Mix 0) or still at the values of the algorithm it has (untouched
since it was picked); a slot you have edited keeps your values. Stepping OFF, TAPE,
OVERDRIVE gives each its own start until you turn a control. MIDI never loads these: an
algorithm CC sets the algorithm alone.

| Slot | Algorithm | Amount | A | B | Mix |
|---|---|---|---|---|---|
| Distortion | TAPE | 80 | 0 | 0 | 127 |
| Distortion | OVERDRIVE | 52 | 64 | 32 | 127 |
| Distortion | FUZZ | 48 | 0 | 32 | 127 |
| Distortion | CRUSH | 64 | 32 | 0 | 127 |
| Distortion | LO-FI | 64 | 16 | 16 | 127 |
| Modulation | CHORUS | 64 | 32 | 0 | 64 |
| Modulation | FLANGER | 64 | 24 | 96 | 64 |
| Modulation | PHASER | 64 | 32 | 64 | 64 |
| Modulation | TREMOLO | 100 | 64 | 0 | 127 |
| Modulation | AUTO-PAN | 96 | 32 | 0 | 127 |
| Modulation | WOW | 48 | 32 | 0 | 127 |
| FX | DELAY | 48 | 64 | 48 | 40 |
| FX | PING-PONG | 48 | 64 | 48 | 40 |
| FX | REVERB | 72 | 48 | 80 | 40 |
| FX | LUSH | 80 | 48 | 96 | 40 |

Each changes a 220 Hz saw at half scale by at least -20 dB of its own level (the RMS of the
difference from the dry sound; the test `every_default_is_audible`).

## Chip character

On the panel they are the CHIP view's ([above](#chip)): ALGORITHM steps the profile and K1..K4
turn the four controls. The chip's profile and its four character controls (`core/sid/sid_character.h`; sources, definitions
and measurements: [decision record 0006](../decisions/0006-chip-profiles.md)):

| Control | CC | NRPN | Values |
|---|---|---|---|
| Profile | 85 | 2/0 | 0 6581 R2, 1 6581 R3, 2 6581 R4AR (power-on with the 6581), 3 8580 R5, 4 worn 6581 (artistic), 5..12 USER 1..8 (an empty slot does nothing) |
| Age/heat | 86 | 2/1 | 0..127: the filter drifts darker, the 6581's non-linearity rises |
| DAC sloppiness | 87 | 2/2 | 0..127: a worn chip's grit, on every sound: the chip's output through a worn converter, a noise that rides on the signal (as rough on a quiet note as on a loud one) and a bowed curve that squeezes the peaks and makes a chord's notes beat; under it, worn voice DACs: bit errors, a more kinked ladder and more leakage (triangles get grit, a quiet voice's level steps as its envelope moves). The grit rises fastest over the first detents and keeps rising to the top; a turn reaches its new amount within a second. A released voice still whispers, with BLEED above 0 |
| Output thump | 89 | 2/3 | 0..127: a thump at each note's start, as the 6581's voice DC jumps when its envelope attacks (a slow attack hardly thumps, a retrigger high in the envelope less), and at a volume change, as its operating point steps. Only note starts change: a held note sounds the same, at the same level. The thump is added last, after the effects, into the room the sound leaves, so it never clips, on either chip or both |
| Filter bleed | 90 | 2/4 | 0..127: the voices routed into the filter leak past it, unfiltered, up to half: a closed filter lets the bright, dry sound through; on the 6581, whose filter inverts, an open one gets thinner and quieter instead. Voices not routed into the filter are untouched. Above 0 a released voice's DAC leak is kept as well (below) |

DAC, thump and bleed follow a perceptual curve: the first detents are subtle, the middle (64)
gives about 29 % of the full effect, the top is obvious. Age is linear.

A profile is a chip and a model: changing it resets the chip (held notes attack again on the
new chip, as at a model change), and the model follows it. The model (CC 25, the FX button) selects
the 8580 R5, or goes back to the last 6581 profile chosen. The four controls change the
playing chip without a reset, and glide there rather than jump, so they don't click: from
the next 10 ms tick, age takes 0.32 s from 0 to 127, the thump 0.44 s, the bleed 1.74 s, the
DACs 2.61 s (the curved controls slower where a detent changes more). A
profile applies from the next tick: the new chip is prepared in the background while the old
one plays on, and swaps in once it is whole, 70 to 110 ms later (the first 8580 after
power-on about 150 ms; the CHIP view and the toast show the choice at once). Changes faster
than that, a profile every 70 ms or less, restart the preparation each time: the chip swaps
once they stop. All of them keep the last value received and
survive preset and model changes. Every control at 0 is the profile as reSIDfp has it (the
worn 6581 aside, which goes beyond reSIDfp on purpose).

**The tail after notes:** the voice DACs leak a faint tone (about -50 dBFS) through a
released voice's envelope. With the bleed at 0 (the default) the output is faded to silence over
3 ms once every voice's envelope has ended and stayed ended for about 46 ms, and stays exactly
silent until the next note: the tail ends about 50 ms after the release's end. The sound itself
is untouched (the stage mutes the block the chips produce), and notes closer than 46 ms play as
they always did. Any bleed above 0 keeps the leak, a deliberate part of the sound.

**The MASTER pot** is a digital gain on the final stereo output, after both chips and the
effect slots, so it scales everything that plays: the keys, a song on either chip and the UI
sounds. Full is exactly the chips' output (the stage does nothing); 0 is silence; between,
the gain follows an audio taper (-60 dB just above 0, rising 0.94 dB per 16 steps of the pot to
0 dB at full: even steps of dB, where a linear gain would put its whole travel in the top
6 dB) and a change ramps over one 2.9 ms block, so turning it never zippers. The chips' own volume registers keep their musical
meaning: live leaves $D418's volume at 15, and a song's fades and 6581 volume digi trick
play relative to MASTER. (Until this fix MASTER set the chip's volume register in 15 steps,
which a song's own writes overrode, so a playing song ignored it, and with output thump
above 0 each step thumped. MASTER no longer thumps, whatever the thump.) The gain differs from
the old register's 15 steps by the taper: the register's steps were linear in gain.

## MIDI

Notes on every channel by default (MIDI's mode OMNI: [the MIDI page](#the-midi-page) can
give the instrument one channel and chip A's voices channels of their own); program change
0..3 selects the preset. Bank select (CC 0 and 32) makes program changes pick songs (bank 1)
or the chip's profile (bank 2): [midi.md](midi.md), Program change and banks. A program change loads
the preset's cutoff, resonance, pulse width and SID settings (waveform, ring, sync, the
envelope, filter routing and mode) and expression settings over any edits, even when it
selects the preset already playing; volume, octave, chip, envelope mode, effects and a bend
range set by RPN 0 are kept. Every parameter has an NRPN, and all but the bend range and the
expression targets a CC: the map, generated from the code, is [midi.md](midi.md). The
numbers M1a and M1c set are kept: 1 pulse width, 7 volume, 71 resonance, 74 cutoff, and
the effect slots on 102..116 (Distortion 102..106, Modulation 107..111, FX 112..116:
algorithm, Amount, A, B, Mix). CC 27..30 are the [chord arpeggio](#chord-arpeggio)'s (on, rate,
order, span). CC 120 and 123 release every note. The chip model (CC 25,
NRPN 1/5) changes from the next 10 ms tick, to the last value received: its chip is
prepared in the background, a slice a tick, and swaps in when it is whole, so a stream of
changes never holds the instrument up; the FX button asks for a change the same way.

**Envelope: INSTANT / AUTHENTIC** (CC 26, NRPN 1/10; INSTANT by default, restored with the
settings). The real SID
has a quirk known as the ADSR delay bug. Each voice's envelope steps when a 15-bit rate
counter reaches the current rate's period, and only that match restarts the counter: a
gate change does not. If a note starts while the counter is already past the attack's
(shorter) period, the counter has to run all the way round first: up to 32767 chip
cycles, about 33 ms, before the attack's first step. The factory presets release slower
than they attack (SAW LEAD: release 6, attack 0), so with the chip's behaviour about 97%
of the notes you play start 33 ms late, and the rest at once. A note-off meets the same
delay when the release is faster than the decay before it: about 73% of note-offs on SAW
LEAD (decay 9, release 6), 80% on SYNC LEAD and 32% on PWM BASS hold the note at its
sustain level for up to 33 ms more, so a staccato note comes out 33 ms long at random.
That is the chip, and reSIDfp behaves the same way. C64 music works around it with a
"hard restart" (setting the rates to 0 a few frames before each note), which a player at
a keyboard cannot do.

- **INSTANT** (0, the default for the live instrument): each gate edge also restarts the
  rate counter, so a note's attack, and its release at the note-off, take their first
  step within one rate period and 3 chip cycles: 12 cycles at attack 0, 270 at release 6.
  The sound is otherwise the chip's: the same envelope, rates, exponential release curve
  and DACs. This is the departure from the chip, behind a setting, because a live
  instrument that is late on most notes, and on most note-offs, is hard to play.
- **AUTHENTIC** (1): the chip's behaviour, bit for bit as reSIDfp. Use it to hear or
  test the real timing.

A change counts from the next gate edge: a sounding note keeps the attack it had, and its
release follows the new setting. The setting
is the instrument's, not the preset's: program changes and the chip model keep it. Song
and `.sid` playback (M2, M3) will default to AUTHENTIC: their music was written for the
chip and handles the delay itself, with hard restarts, so its timing is part of how it
sounds. The measured latencies are in
[decision record 0005](../decisions/0005-midi-note-timing.md).

**MIDI out (USB only):** what you change on the panel goes out over USB on channel 1, so
a DAW can record it as automation: each knob, MASTER, FX and the effect-slot controls as
the parameter's CC (above) (K4 is the release, CC 72), and PRESETS as a program change. At most one message per parameter
every 10 ms, with its latest value. A change that came in over MIDI (TRS or USB) is never
sent back, so recording with the DAW's MIDI thru on does not loop, and TRS input is not
passed on to USB. The channel and 14-bit NRPNs for cutoff, pulse width and volume are
build options for now: [midi.md](midi.md), Output.

**SysEx transfers:** [protocol version 1](sysex.md) provides inquiry, the store
directory, named files, current-song dumps and GTI5 instruments. Replies use USB
only; TRS accepts paced uploads without replies. PUT claims its space before
acknowledging. With today's no-window flash configuration, PUT uses explicit
SAVE-style program holds and the existing mute/ramp; validated windows make it
hold-free. RECEIVING is followed by SAVED or a refusal toast. A transfer never
enables refill erases, and partial uploads do not replace files. Instrument
transfers need the transport stopped; import blocks seeks/audition while it owns the
model, and requires a fresh PLAY afterward. Song import refuses unsaved changes.
Use SAVE AS to name an imported song before another current-song upload; the
private rollback cache stays pinned meanwhile, with **SAVE AS FIRST** on refusal.
USB output and hold-free flash operation still require their hardware checks.

An algorithm CC picks the algorithm in order (above) by dividing 0..127 evenly
among the slot's algorithms: for the FX slot's five, 0..25 OFF, 26..51 DELAY, 52..76
PING-PONG, 77..102 REVERB, 103..127 LUSH. A control CC (0..127) is the control's whole
range.

## Settings and STORE

Settings load at power-on: CARDS, KEYS, SOUNDS, TRACKER ROWS, THEME, SPREAD, SID MODEL, the preset and its edits, the keys' chip
model, profile and character, the effect slots, envelope mode, MIDI-learn bindings and the
last firmware version booted. Missing or unreadable settings use the defaults. Octave,
held notes, controllers and the view on screen are not restored; volume follows MASTER.

Changes are saved in the background **5 s after the last change to a persisted value**,
without a toast. A busy store waits; background saves never force audible gaps. With the
current flash timing limits, programs as well as erases wait for quiet. Stop playing and
let release tails fade before powering off, or use SAVE and wait for its completion toast.

SETTINGS' **STORE** item, immediately after CARDS, shows free space outside the store's
reserve pool in kilobytes (for example, **STORE 193K**), **FOREIGN** or **NEWER**. FOREIGN means
the region contains data SIDOU does not own; NEWER means it is a newer SIDOU version's store
(after a downgrade: install that version again to keep its songs). Either is left untouched,
and settings cannot persist.

On FOREIGN or NEWER, turn **ALGORITHM clockwise**: **FORMAT?**, and the popup says **SAVE
FORMATS**. Then press **SAVE**, without leaving the item, to confirm. Further turns never
confirm, so a fast spin of ALGORITHM cannot format. Counter-clockwise, moving SETTINGS' cursor
or leaving SETTINGS cancels the confirmation. **Formatting destroys the other firmware's data in the store**, erases
all 73 sectors and can interrupt the sound. STORE shows **ERASE n**, then the toast
**STORE FORMATTED**. Current settings are then saved when the store can take them. Formatting
is never automatic; ALGORITHM does not format a SIDOU store.

SETTINGS' **SID MODEL** decides the chips a `.sid` tune plays on ([SIDS and NOW PLAYING](#sids-and-now-playing)):
**AUTO** (the default) plays a tune whose header names a model on reSIDfp's profile of it (the
6581 R4AR or the 8580 R5, at character 0), and one that says "unknown" or "both" on the keys'
setup; **KEYS** plays every tune on the keys' setup (the chip, profile and character the keys
have). ALGORITHM clockwise is KEYS, counter-clockwise AUTO; it counts from the next tune that
loads (or restarts), and is saved with the settings (schema 7).

SETTINGS' last item, **RESET**, puts everything that is not a song back to factory: every
preset's edits (not only the playing one's: EDIT held + PRESETS does one), and every setting
that is saved (KEYS, SOUNDS, TRACKER ROWS, CARDS, THEME, SPREAD, SID MODEL, the playing preset, the chip
model, profile and character, the EXPERT overrides, the effect slots, and the envelope mode).
**Songs, USER chip profiles and MIDI learn mappings stay**, and so does the last song that boots. Turn **ALGORITHM clockwise**: the item says **RESET?** and the popup **SAVE
RESETS** under the name **RESET ALL?** (the question: reset all sounds and settings?). Then
press **SAVE**, without leaving the item, to do it: the settings are saved at once and the
toast says **RESET AND SAVED** (**RESET AND SAVED (GAPS)** when the save had to be forced). Further turns never confirm, and counter-clockwise, moving
SETTINGS' cursor or leaving SETTINGS (HOME) cancels it with nothing written. REC QUANTISE and
the MIDI page's setup are RAM-only and are not part of it.

## SONG

In STEP, TRACKER and its ORDER, INSTR and TABLE pages, a clean SAVE release saves
the loaded song, not the global settings. An unnamed song opens SAVE AS. Unchanged
songs report **SONG SAVED** without rewriting flash. From an accepted SAVE until
completion, including a wait for the store, the header shows **SAVING** and new
song edits are refused with the same toast. Already queued edits finish first;
playback, mute, solo and jam continue. Songs are not autosaved.

The menu's **SONG** item opens the song-file actions. On each of its pages **SAVE is the OK**,
as in the menu: it opens the highlighted action, chooses the highlighted file, or answers the
confirmation (NO goes back a page, as HOME); ALGORITHM clockwise does the same, and HOME goes
back a page.

- **LOAD** lists the built-in **SIDOU LOOP** first (in the firmware, marked
  built-in by a dot after its name: `SIDOU LOOP ·`), then the store's songs, at 2×
  eight a page. Loading stops the transport. Unsaved changes require confirmation
  before they can be discarded, as for NEW. If a file is refused, its reason appears in a toast
  and the previous file is reloaded, stopped. The built-in loads as a file does but
  stays unnamed: SAVE AS (which offers its name) makes a store copy, and it is not
  in DELETE's list. A fresh install's LOAD so always has a song to load.
- **NEW** asks first when the song has unsaved changes (DISCARD UNSAVED SONG?, as LOAD does), then
  creates one subtune, three one-pattern orderlists and three 64-row
  patterns at tempo 6, with a pulse lead (instrument 01), a bass (02) and the **SID drum kit**
  (03-0A): KICK, SNARE, CL HAT, OP HAT, CLAP, TOM LO, TOM HI and RIM, plain GoatTracker
  instruments on the chip's own voices ([decision record 0009](../decisions/0009-drum-kit-and-digi-drums.md)),
  so a song made with them plays the same in GoatTracker. Each drum sounds one frame (20 ms)
  after its row, whatever the row's note. On STEP, PRESETS picks a lane's drum by name.
  Instruments 0B-0E are the **digi drums**, `DG KICK`, `DG SNARE`, `DG CLAP` and `DG HAT`:
  short 4-bit samples played through chip A's output as a C64 tune plays them through the
  6581's volume register, at the sample of their note, one at a time (a hit cuts the last);
  while one plays, its voice's own sound is muted. The song's `DIGI` chunk marks them
  ([song format](song-format.md#version-1-of-each-chunk)), which GoatTracker skips: there they
  play their chip-kit copies. A digi drum sounds the same whichever model chip A is (as a 6581,
  or an 8580 with the "digi boost"), goes through the effect slots, and plays in the middle of
  CHIP SPREAD's stereo. A note of another instrument right after a digi drum on the same voice
  starts a few milliseconds late (the voice's mute is the main loop's).
- **SAVE AS** edits an upper-case filename of up to 16 characters: SELECT moves
  the cursor and ALGORITHM steps its character. The page's save action commits it.
- **DELETE** requires confirmation before removing a file.

The last song loads stopped at power-on. A device song must fit both the content
arena and the **65,536-byte saved-file limit**, including its trailer. An edit
that would cross the latter limit reports **SONG FULL** and changes nothing.

MIDI bank 1 program `n` loads the store's zero-based `n`-th song in name order,
stopped. Other file types and private MIDI staging files do not count. Lookup and
loading are asynchronous;
transport requests during a load are not queued for the new song. An out-of-range
program keeps the current song. Unsaved changes refuse MIDI recall with
**SAVE YOUR SONG FIRST**; save or discard them through SONG before recalling.
The loaded song restores its MIDI mode/options and rows per beat. A file without
a MIDI chunk retains the current MIDI setup; CLOCK and CLOCK OUT remain global.
Recall is limited to programs 0..127 and does not queue behind an active file
operation. A new program can replace a pending directory lookup before loading
starts. The current device build retains the inherited MIDI limitation: MULTI
and WHOLE A jam-channel notes, bends and filter changes are not connected to the
song player; restoring their setup does not add that connection.

ABOUT plays its original tune only when the loaded song is stopped and clean.
Closing ABOUT restores that song, stopped, and the song views as they were (their
cursors, the instrument, STEP's lanes). With unsaved changes the music is
silent and the scroller says **SAVE YOUR SONG FOR MUSIC**. While ABOUT's tune is
the song, SEQ and the menu's TRACKER do not open the song views (the toast **LEAVE
ABOUT FIRST**, or **LOADING** while its close reads the song back), so nothing edits
ABOUT's tune.

## SIDS and NOW PLAYING

The menu's **SIDS** (after SONG) is the store's PSID `.sid` tunes (M3: SIDOU's own 6502 plays them
on the chips; [the manual](../manual/sid-tunes.md) says how a tune gets there). `app/sid_view.h`.

| Control | In the browser (SIDS) | In NOW PLAYING |
|---|---|---|
| **SELECT** | moves through the tunes, eight a page at 2× by their store names (a longer one ends in an arrow); past the eighth the next page, before the first the one before | nothing |
| **ALGORITHM** clockwise, or **SAVE** | plays the highlighted tune from its start song and opens NOW PLAYING; on the tune that is loaded, opens NOW PLAYING as it is | turned either way: the song before or after (clamped at 1 and the last), which plays from its init |
| **PLAY** | (as in NOW PLAYING while a tune is loaded) | stops a playing or paused tune (its block fades); plays a stopped one's song from init |
| **EDIT held + PLAY** | (as in NOW PLAYING) | pauses (its block fades; the 6502 and chip A stand still) and resumes; on a stopped tune, plays it |
| **GLO held + note key 1-3 / 4-6** | | mutes / solos the tune's voices, as for a song: on the chip (the tune keeps every write), and on voice 1-3 of both chips for a 2SID tune |
| **HOME** | back to the menu; with the tune stopped (or failed) it ends for good there, and the song comes back from the store | back to the browser, the tune playing on |

An empty store says **NO TUNES: SEE MANUAL**. The loaded tune keeps its place in the browser:
a play mark at its left while it plays, a square while it is paused or stopped. The tune plays
on whatever view shows: HOME leaves NOW PLAYING and the browser with it playing, and LIVE,
SCENE and STATUS follow it. **PLAY in another view** stops a playing tune, and gives a stopped
one up for the song (it ends for good, so the next PLAY is the song's); with no tune loaded PLAY
is the song's, as always.

**Before a tune takes the song's memory**: the song and a tune share it, so a tune loaded over
a song with unsaved changes asks first, **SONG NOT SAVED** and the song's name (NEW SONG for one
never named): **SAVE** saves it (in place, or through SAVE AS for an unnamed song, back to the
question after), and the tune plays once it is saved; **ALGORITHM** clockwise again discards
the changes and plays the tune; **HOME** cancels. When the tune ends for good, the song comes
back from the store (the last one opened, by name), stopped.

**NOW PLAYING** shows the tune's title at 2× on two lines (broken at a space), its author and
released at 1×, **SONG 2/5**, the time since the song started and its state (PLAYING,
PAUSED, STOPPED, FAILED, LOADING), PAL or NTSC and each chip's model (as SID MODEL gives it,
in SETTINGS), **2SID** for a tune of two chips, **3SID: 3RD SILENT** for one of three (the third's
writes are dropped), the three voices with GLO's mutes and solos (as TRACKER's header: the voice's
colour, dark grey while silent, reversed while soloed, MUT and SOL) and chip A's envelopes as
bars, and the overruns (play calls that ran past their frame) and late writes when not zero.

While a tune plays: a 1SID tune takes chip A and the keys play chip B, KEYS A acting as AUTO
(no voice can be taken from a tune); a 2SID tune takes chip B too, and the keys and UI sounds
are silent (`KEYS OFF: 2SID TUNE`, once). The CHIP view's A tab changes the tune's chip A.
**MIDI** (TRS and USB): **start** restarts its song from init, **stop** holds it where it is
(paused), **continue** goes on (a stopped tune: from init); clock, song position, song select
and program change are ignored, and clock out sends nothing (a PSID has no tempo); CLOCK OFF
ignores the transport, as for a song. **STATUS** shows the 6502's rows on its first page, in the sound's
knobs' place: its state, cycles a second, `CPU` (the share of the CPU the 6502's slices took
over the last second, to a tenth of a percent, and the 24 MHz ticks a thousand 6502 cycles took:
`3.2% 412T/KC`; `--` while nothing runs), overruns, late writes, the voice shadow (ON: OSC3 and
ENV3 read as on a C64) and a third SID's dropped writes.

Refusals are toasts: `RSID: NOT YET`, `MUS: NOT SUPPORTED`, `BROKEN .SID`, `TOO LONG`, `SID NOT
FOUND`, `SID: READ ERROR` (the file went while it loaded), `INIT TOO LONG`, `PLAY HUNG`,
`6502 JAM AT $xxxx`, `MIDI MODE HAS CHIP A` (MULTITIMBRAL and WHOLE-CHIP keep chip A).

## SAVE

A plain SAVE acts on **release**, not on the initial press. Turning any encoder or pressing
another button while SAVE is down prevents that release from saving.

| Gesture | Result |
|---|---|
| SAVE pressed and released, no encoder turned or other button pressed while down | saves the song in a song view (above); in the menu opens the item under the selection (its OK; nothing on a value item), and on SETTINGS confirms an armed STORE format or RESET and does nothing else (the settings save themselves 5 s after a change); elsewhere saves settings now, skipping the 5 s debounce; **SAVED** when committed |
| the same, on the CHIP view's chip focus with a USER slot as the save target, or applied | stores the tab's profile and controls into that slot ([USER profiles](#chip)); **OVERWRITE USER 3?** first when it holds one, then **SAVED TO USER 3** |
| SAVE held + K1..K4, with or without EDIT | opens MIDI LEARN on that knob's parameter on the turn, not the press; the parameter does not move and SAVE's release saves nothing |
| Another knob turned while the learn frame waits, SAVE held or not | picks that knob's parameter instead |
| CC received while the frame waits | learns it, closes the frame and applies its value; **LEARNED CC 21 → CUTOFF** |
| SAVE pressed and released again in the learn frame | forgets the picked parameter's binding and closes: **CUTOFF: CC 21 FORGOTTEN**; with none, **CUTOFF: NO CC** and the frame stays |
| SAVE held + HOME in the learn frame | asks **CLEAR ALL MAPPINGS?**; release SAVE, then press and release it again to confirm: **MIDI LEARN CLEARED**; HOME or 10 s without a step keeps the mappings |
| HOME without SAVE held, or 10 s without a step in the learn frame | cancels and closes the frame; HOME opens nothing else |
| SAVE held + HOME outside the learn frame | consumes HOME without opening the menu; SAVE's release saves nothing |
| SAVE held + any other button | that button acts as usual; SAVE's release saves nothing |
| SAVE pressed during the boot or a title card | skips it only; its release saves nothing |

An explicit save waiting for 2 s may force short held flash operations, with gaps in the
audio; completion then says **SAVED (GAPS)**. Background saves never do this. With SOUNDS on,
a completed explicit save plays the drive chunk. A foreign store refuses it with
**STORE FOREIGN: NOT SAVED**; a flash error, or a flash part the store does not know, with
**STORE ERROR: NOT SAVED** (and a song's SAVE with **SONG: SAVE FAILED**). Song saves report **SONG SAVED** or **SONG SAVED (GAPS)**;
on CHIP, SAVE stores a USER slot as above, and settings otherwise.

## The MIDI page

From the menu (HOME, MIDI): MIDI's mode and its options, a menu like the main one (SELECT
moves the cursor, ALGORITHM changes the value under it, HOME goes back). The modes are in
[midi.md](midi.md), Modes; the clock in Clock and transport. A change of MODE or CHANNEL
releases what MIDI was playing. The page's setup stays in RAM and is not included in saved
settings; a song carries its own mode in its `MIDI` chunk.

MIDI follows STATUS in the large main menu. The options page shows six rows at 2×, like the
main menu; its title bar says which of the ten items is selected, and it scrolls to CLOCK and
CLOCK OUT. It opens in a cut, the title bar and the help first and then its lines two a frame
(four frames, 0.16 s: a frame of it stays within the page's `ui_poll` bound). Turning ALGORITHM
names the selected item
and its value in the large value popup; closing it restores the page underneath.

| Item | Values | What |
|---|---|---|
| MODE | LIVE, MULTI, WHOLE A, WHOLE B, OMNI (at power-on) | which channel plays what |
| CHANNEL | 1..16 (1) | N, the first channel: the instrument's in LIVE and WHOLE B, chip A's voice 1 in MULTI, chip A's in WHOLE A; N + 3 plays the instrument on chip B in MULTI and WHOLE A |
| OPTION | POLY (default), CHORD, ARP | WHOLE A's: three-voice polyphony, the stored chord, the arpeggio on voice 1 |
| LEGATO | OFF (default), ON | MULTI and CHORD: the next note while one is held glides on, no new attack |
| CHORD 2, CHORD 3 | +0..+24 (+4, +7) | the stored chord: semitones above the note on voices 2 and 3 |
| ARP RATE | CLOCK, 1..32 FR (3 FR) | frames an arpeggio step; CLOCK: every 16th of the MIDI clock |
| ROWS/BEAT | 1..16 (4) | the song's rows a beat, for following and sending the MIDI clock |
| CLOCK | AUTO (default), FOLLOW, OFF | AUTO: the song follows a MIDI clock and transport while one arrives, and carries on at the last tempo when it stops; FOLLOW: the same, but the song stops with the clock; OFF: the song on its own frames, MIDI's start and stop ignored |
| CLOCK OUT | OFF (default), ON | the song's clock out over USB, 24 a beat, with start, stop and continue, while no clock comes in |

In MULTI and both WHOLE modes chip A belongs to the player, song or not, so the keys play
chip B as they do [while a song plays](#which-chip-the-keys-play). The MIDI
chip-A jam callbacks remain unbound: those channels are routed but sound nothing
yet. This is an integration follow-up, separate from the panel's working song audition.

## MIDI learn

Any knob's parameter can follow a CC of your choice, from a controller on TRS or USB:

1. **Hold SAVE** (button 9) **and turn a knob.** A frame, MIDI LEARN, opens over the screen
   on the turn and names its parameter (CUTOFF
   &larr; SEND A CC) and does not move it. The knob is the parameter it turns on the panel, in
   the view under the frame: K1..K4 the cutoff, resonance, the sound's own control (the pulse width on
   PWM BASS, the decay on the saw and noise sounds) and the release; on a slot's page of LIVE, the
   slot's controls; on the CHIP view,
   K1..K4 its focused section's (AGE, DAC, THUMP, BLEED on the chip; a slot's Amount, A, B and
   Mix on a slot; SELECT moves the focus while the frame shows); with **EDIT held**, K1..K4
   the selected effect slot's Amount, A, B and Mix in any view (SCL first to pick the slot; it
   works while the frame shows). K3 on SYNC LEAD is the sync ratio, which has no CC; trying it
   shows RATIO: NO CC and opens no frame. K4 is the release, which has one (CC 72).
   Release SAVE once the frame opens: the frame keeps waiting and that release saves nothing.
2. **Send the CC** (move the controller's knob). The frame closes with LEARNED CC 21 &rarr;
   CUTOFF, the parameter jumps to the controller's value, and from then on follows it.

While the frame shows, the note keys, OCT-/OCT+, FX, SCL, ENV and EDIT work as usual, and on
the CHIP view SELECT and ALGORITHM (K1..K4 then pick on the page the focus gives). Over the menu
and SCENE, SELECT and ALGORITHM do nothing while it shows.

- **Cancel:** HOME (it closes the frame and opens nothing), or 10 s without a step (each
  step restarts the time). A cancelled learn changes nothing.
- **Forget one:** hold SAVE and turn the knob, release SAVE, then press and release SAVE
  again instead of sending a CC: the parameter's learned CC is forgotten (CUTOFF: CC 21
  FORGOTTEN), and its own CC is its own
  again. The frame shows SAVE: FORGET CC 21 when there is one. With no binding, it says
  CUTOFF: NO CC and stays open.
- **Clear all:** with the frame open, hold SAVE and press HOME (CLEAR ALL MAPPINGS?),
  release SAVE, then press and release SAVE again: MIDI LEARN CLEARED. HOME without SAVE,
  or 10 s without a step, keeps them.

What learning means for MIDI in and out ([midi.md](midi.md), MIDI learn): a learned CC
works on every channel and on both ports, like the map; the parameter's own CC keeps
working; if the learned CC was another parameter's, it now moves the learned one only (the
other keeps its NRPN); learning a parameter again moves it to the new CC, and a CC learned
for another parameter moves to it. The panel's changes to a learned parameter go out over
USB on the learned CC. Bank select (0, 32), data entry and NRPN/RPN selection (6, 38,
96..101) and the channel mode messages (120..127) cannot be learned.

The mappings persist with the settings, including forget and clear-all changes: background
save after 5 s without another persisted change, or a plain SAVE after the frame closes.

**Why SAVE.** SAVE is about keeping what you set, which a mapping is. Its plain press and
release now saves settings; MIDI learn has moved to a knob turned while SAVE is held.
Opening the frame on the turn distinguishes learning from saving without a long-press
delay, and works with EDIT's effect-slot layer. The mappings are saved with the settings.
SAVE's id (9) follows the design's button order (FX SCL ENV LFO EDIT GLO HOME SAVE ...),
which matches the ids known so far (FX 2, SCL 3, ENV 4, EDIT 6, HOME 8); the hardware
checklist confirms it.

## Screen

**LIVE** is what the screen shows from power-on (the root view, `app/live_view.h`;
[the art page](../art/README.md#live) draws it): the preset's name at 2 times the font and, when
the octave is not 0, the octave at 2× in yellow on the right (`+1`, `-3`); three voice lanes, each
the voice's number at 2× in its colour (voice 1 light red, voice 2 cyan, voice 3 light green) with
the waveform under it, its oscilloscope drawn 2 pixels thick in the voice's colour, and its note
at 2× with the envelope stage (ATK, DEC, SUS, REL) under it; **the knob row, what K1..K4 turn, in
their left-to-right order**, under the page's name in yellow (`TONE`, which SELECT pages to the
slots'): CUTOFF (K1, yellow), RESO (K2, cyan), K3's own control (K3, light green: RATIO, PULSE or
DECAY, by the sound) and RELEASE (K4, light red), each label in the knob's readable
colour, its value at 2× (three characters at most: `980`, `7K2`, `12K`, `15`, `50%`, `-3`) with a bar
in the knob's colour and its name K1..K4 in the legend colour, a knob that just turned reversed
for a second ([above](#value-popup)); and a bottom band with the filter's mode at 2× and the
chip's model and profile (`CHIP 6581`, `R4AR`), the three effect slots' names across one row (the
selected one in reverse video) with the selected slot's algorithm at 2× under them, and VU meters
for each voice and for the left and right output after the effect slots. The scopes are flat and
dark grey at rest and draw each sounding voice's waveform, still while a note holds, over two to
four periods (more for higher notes). **EDIT held** turns the selected slot's four controls
on K1..K4 and the knob row says so: the page's name is the slot's (`DIST`) and the labels are
the controls' own names (`DRIVE`, `TONE`, `BIAS`, `MIX`; one the algorithm does not use is dim),
and the slot's algorithm at 2× in the band is in reverse video. **ARP held** shows ARP's page the
same way (RATE, ORDER, SPAN: `3F`, `UP`, `1`), and while the [chord arpeggio](#chord-arpeggio) is
on, a badge at 2× on rows 27-28 names it with its settings (`ARP 3F UP 1OCT`, ARP in yellow).

<a id="status-pages"></a>The **status screen** (STATUS in the menu) shows the numbers at 2×, on
three pages that **SELECT** turns (either way, round the three); the title bar's `1/3` says which. It
opens on the first page every time, as a help card does.

| Page | Shows |
|---|---|
| 1 | the sound's knobs, each with its bar: CUTOFF, RESO, PULSE and VOLUME. While a `.sid` tune is loaded, the 6502's rows instead: `6502` and its state, `CPU` (its share), `T/KC`, `LATE`, `OVERRUNS`, then `CYC/S` (cycles a second), `SHADOW` and `3RD SID` (see [SIDS and NOW PLAYING](#sids-and-now-playing)) |
| 2 | the chip (its model and the profile's word, `6581 CLASSIC`; with two chips a line each, KEYS marking the one the keys play), the preset and octave (`OCT +0`), and the held notes |
| 3 | under the effect slots DIST, MOD and FX each slot's algorithm and Mix (`>` marks the selected slot), then the selected slot's Amount, A and B |

The version number sits after SIDOU in the title.

The menu (HOME) is drawn at 2 times the font, six items of the eleven at a time (the
selected one a line in reverse video; the title bar says which it is of how many, and an arrow
up or down when items are out of sight, and the list scrolls with SELECT). It holds the views
and the playing values: CHIP (the CHIP view, above), TRACKER (the song views), SCENE (the
generative pieces), SONG (the song-file actions), PRE (the preset), OCTAVE, SETTINGS, STATUS,
MIDI, CHARSET (the glyph sheet) and ABOUT. **SAVE opens the item under the selection** (the
menu's OK; ALGORITHM clockwise does too), and the help under the items says so (SAVE OPEN).
**SETTINGS** is a page like it of the set-and-forget settings: KEYS, SOUNDS, ROWS (TRACKER
ROWS), CARDS (title cards), STORE, THEME, REC (REC QUANTISE), SPREAD and RESET; ALGORITHM changes them (its
help says ALG CHANGE) and HOME goes back to the menu. A line is 14 characters:
PRE, ROWS and STORE's kilobytes (`193K`, FOREIGN, NEWER, FORMAT?, `ERASE 20`) are short so that the
longest preset name (NOISE PERC) and value fit beside their labels; the
[popup](#value-popup) says the whole names. LIVE's scopes, TRACKER, CHIP, STATUS and ABOUT keep
the font's own size. A view opens with a wipe from the top and
closes with one from the bottom; CHIP, SCENE, and LIVE back from the menu, open behind a title
card instead, and cut in under it in bands. M2's STEP view will be the default then; for M1 LIVE is.

A change of octave, selected slot or algorithm shows a toast on the bottom line for
1.5 s ("OCTAVE +1", "SLOT MOD", "MOD CHORUS"), whatever made it: a button, the menu or
MIDI. A chip change shows "CHIP 8580" and sweeps raster colours down the border.
