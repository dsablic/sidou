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
2. **global:** HOME opens the menu from LIVE and goes back one view anywhere else; SAVE
   saves on a clean release, or held with a knob opens MIDI learn over any view; SAVE held
   with HOME consumes HOME ([SAVE](#save)); SEQ, PLAY and REC are the song views' ([below](#song-views)),
   in every view;
3. **the view on top:** the menu, CHIP and TRACKER take SELECT and ALGORITHM; SCENE takes SELECT;
   LIVE, the status screen, the glyph sheet and About take nothing; MIDI learn's frame passes
   them to the CHIP view under it, and to no other;
4. **the panel:** everything else plays the instrument as below, in every view. K1..K4 turn
   the page of the view that shows: CHIP's focused section on the CHIP view (on top or under
   MIDI learn's frame), LIVE's knobs in every other view.

The note keys and MASTER always play, whichever view shows, but for two layers before the
panel: **GLO held** turns the leftmost keys into the song's mutes and solos, and in a **song
view** the keys audition the song's instrument ([Song views](#song-views)). The effect-slot shortcut (EDIT,
SCL, ENV) works in every view, the CHIP view included ([CHIP](#chip) says why it stays).

## Which chip the keys play

SIDOU has two SID chips, A and B. The panel and MIDI play **the keys' chip**:

- **no song playing:** chip A, exactly as with one chip;
- **a song playing on chip A:** chip B, on top of the song (KEYS **AUTO**, the default), or
  chip A with KEYS **A** (menu): a key then takes a voice from the song while it sounds, the
  song keeps the filter and the chip's volume register (K1 and K2 do nothing on it; MASTER
  still scales the whole output, song included) and its chip setup.

The keys' setup (the chip model, the profile, the character controls and the envelope mode)
travels with the keys: chip B always holds it, prepared in the background after every change,
so a key sounds the same on either chip. FX, the CHIP view's profile and character, and the
chip CCs change the keys' setup. When a song starts the keys move to chip B (held keys sound
again there); when it stops they move back to chip A once it has the keys' setup again and no
key is held. LIVE says **KEYS:B** in its bottom band while the keys play chip B; the status
screen shows both chips and marks the keys'. Until M2c plays songs, nothing starts one, so the
keys always play chip A. KEYS persists with the settings.

## UI sounds

**SOUNDS** in the menu (OFF by default, restored with the settings) turns
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
| K3 (encoder 4) | **the sound's own control**: RATIO on a sync sound (SYNC LEAD: the sync ratio, -12..+12 semitones from the preset's, a semitone a detent; the panel's alone, with no CC), PULSE on a sound with the pulse waveform (PWM BASS: the pulse width, 31 positions from 3 % to 50 %, 1.6 % a detent, clockwise wider), DECAY on the others (SAW LEAD, NOISE PERC: 0-15) (CHIP: the third) |
| K4 (encoder 5) | the release, 0-15 (CHIP: the fourth) |
| **PRESETS (encoder 6)** | the preset: clockwise the next, clamped at both ends; with EDIT held it puts the current preset back to its factory settings; in every view (TRACKER takes it with REC on) |
| MASTER (pot) | volume: a digital gain on the final output, for everything that plays (the keys, a song on either chip, UI sounds), on an audio taper from -60 dB to full in even steps of dB, shown in percent of the pot's travel |
| **EDIT (button 6) held** + K1 / K2 / K3 / K4 | the selected effect slot's **Amount / A / B / Mix** (32 detents end to end), in every view; K1-K4 are the knobs above again once EDIT is let go. EDIT is lit while the selected slot has an algorithm |
| **SCL (button 3)** | selects the next effect slot: Distortion, Modulation, FX |
| **ENV (button 4)** | the selected slot's next algorithm (back to OFF after the last); it starts from a musical setting, so it is heard at once ([Effect slots](#effect-slots)) |
| **HOME (button 8)** | the menu, from LIVE; back one view from any other (the menu, CHIP, the song views, the status screen, SCENE, the glyph sheet, About); from the menu back to LIVE behind THE OSCILLATOR's title card (below) |
| **SAVE (button 9)** | clean press and release: save a USER profile on CHIP's chip focus, the song in a song view, settings elsewhere; hold and turn K1..K4: MIDI learn for that knob ([SAVE](#save)) |
| **SEQ (button 11)** | opens the song views, and steps through them ([below](#song-views)) |
| **PLAY (button 12)** | plays the loaded song from the song views' position, and stops it (each voice's release rings on); with EDIT held, goes on from where it stopped; lit while it plays. While it plays the keys play chip B (KEYS AUTO), or take voices from the song on chip A (KEYS A). A song the device cannot play is refused with a toast (`SNG: MULTISPEED 8 (MAX 4)`) |
| **REC (button 13)** | record mode on and off |
| **GLO (button 7) held** + note key 1..3 / 4..6 | mutes voice 1..3 / solos voice 1..3 of the song (again: back), in every view; the keys sound nothing while GLO is held |
| **SELECT (encoder 0)**, in the menu | moves the selection (a line in reverse video) up and down the items; the list scrolls with it |
| **ALGORITHM (encoder 1)**, in the menu | changes the item under the selection, which the [popup](#value-popup) also says in full: PRESET (PRE in the menu), OCTAVE, KEYS, SOUNDS, TRACKER ROWS (ROWS), CARDS ([title cards](#title-cards)), STORE ([format confirmation](#settings-and-store)), THEME; turned clockwise on CHIP opens the CHIP view, on TRACKER the song views, on SCENE the generative pieces, on SONG the [song-file actions](#song), on STATUS the status screen, on MIDI the MIDI page, on CHARSET the glyph sheet, on ABOUT the credits |
| **SELECT (encoder 0)**, on LIVE | pages the knob row: TONE, DIST, MOD, FX; on a slot's page K1-K4 turn that slot's Amount, A, B and Mix and SCL moves the page with the slot; it stops at either end |
| **ALGORITHM (encoder 1)**, on LIVE | steps the chip's profile on TONE (R2, R3, R4AR, R5, WORN; it stops at either end; the band's profile word shows the change), or the selected slot's algorithm on a slot's page or with EDIT held |
| **SELECT (encoder 0)**, on CHIP | moves the focus: A/B tabs, CHIP (the profile and the character), DIST, MOD, FX, EXPERT; it stops at either end |
| **ALGORITHM (encoder 1)**, on CHIP | steps the focused section's choice: the profile on CHIP, the algorithm on a slot, or the four-control page on EXPERT; EDIT held on CHIP or EXPERT selects the USER save target |
| **SELECT (encoder 0)**, in TRACKER | moves the cursor one row, or one 8-row page with EDIT held; stops at the first and last body rows |
| **ALGORITHM (encoder 1)**, in TRACKER | the shown voice: voice 1, 2, 3 and round; the cursor's voice and the audition always match the voice on screen |
| **SELECT (encoder 0)**, in SCENE | the next piece clockwise, the one before counter-clockwise (FIRE, PLASMA, RULE 30, STARS, ANT, RED MOON RITE, and round): it wipes in, and a toast names it |

LFO, ARP and PRESETS (encoder 6) edit TRACKER with REC on (below); the other buttons (LFO, ARP) do
nothing else yet, and PRESETS steps the preset everywhere else. SELECT does nothing on STATUS and
ABOUT, ALGORITHM nothing on STATUS and ABOUT.

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
| MASTER | VOLUME (0-100%, the bar the pot's travel), in every view |
| ALGORITHM + EDIT on CHIP | the USER slot to save to (`SAVE TO`, `USER 2 -`) |
| ALGORITHM in the menu, on an item with a value | the item in full and its value: `PRESET`, `OCTAVE`, `CARDS`, `STORE`, `KEYS`, `SOUNDS`, `TRACKER ROWS`, `THEME` (`HI-CONTRAST` or `C64`) |

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
**TABLE**, all on the same GoatTracker 2 song (`app/song_view.h`). TRACKER is the first made
(`app/tracker_view.h`; docs/art/README.md "TRACKER" shows it): the patterns in GoatTracker's
layout, **one voice at a time at 2× the font** (12 rows) with the other two as activity strips on
the right, and the shown voice's orderlist and the bar row's command. **SELECT moves the row;
ALGORITHM steps the voice shown** (the cursor's voice: the note keys audition on it, below).
With REC off its rows follow the song while it plays, the cursor while it is stopped; with
REC on the edit cursor stays on screen. The moving bar over still rows (the
menu's **TRACKER ROWS** BAR, the default) or scrolling under a still bar (SCROLL, as
GoatTracker does). TRACKER ROWS is saved with the settings. The song
is SIDOU LOOP until songs load from the store (M2c Task 8).

| Control | Does |
|---|---|
| **SEQ** | from any view, opens the first song view over it (HOME goes back to that view); in a song view, the next one: STEP, TRACKER, ORDER, INSTR, TABLE and round (the views not made yet are skipped). The menu's TRACKER opens them too |
| **PLAY** | in every view: plays the song from the orderlist entry the song views show (the cursor's), or stops it (releases ring on). While it starts (the keys moving off chip A, [which chip the keys play](#which-chip-the-keys-play)) it shows as playing; PLAY again stops it |
| **EDIT held + PLAY** | goes on from where the song stopped (from the cursor if it never played) |
| **REC** | record mode on and off: REC in red on the song views' top line |
| **GLO held + note key 1, 2, 3** | mutes voice 1, 2, 3 of the song, or lets it sound again |
| **GLO held + note key 4, 5, 6** | solos voice 1, 2, 3, or unsolos it: while any voice is soloed the soloed ones sound and the rest are silent; with none, the muted ones are silent |
| **Note keys**, in a song view | audition the song's current instrument on the cursor's voice through the player, on chip A with the song's setup, at the panel's octave, one note at a time (the last key wins). With the song stopped the audition takes chip A for the song (the keys elsewhere move to chip B, as for PLAY; the first note sounds a few tens of milliseconds late) and gives it back two seconds after the last note is let go; no row of the song plays |
| **SAVE** | a clean release saves the song; an unnamed song opens SAVE AS. Held + K1..K4 opens MIDI learn |

GLO's layer works in every view and comes before the panel: a key pressed while GLO is held
sounds nothing until it is let go, even if GLO is let go first; a key already sounding when GLO
goes down sounds on. The mutes and solos show in TRACKER's header (the shown voice's number
grey, with MUT in red or SOL in white beside its pattern) and on the other two voices' strips
(grey, short while silent), and on LIVE's top line, in red at 2× in the preset's place: **M13 S2**
is voices 1 and 3 muted and voice 2 soloed.

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

### Editing TRACKER

**REC on** enables pattern editing. The row number is a yellow block; the selected note or
instrument is reverse yellow at 2×. When editing a command or its data, the whole command/data
field expands to 2× in reverse yellow. Other voices remain activity strips.

| Control | With REC on |
|---|---|
| Note key | writes its note and the current instrument into the cursor's row, auditions it, then advances by the edit step (wrapping within the pattern) |
| ARP | writes key off (`===`), keeping the row's other fields |
| LFO | clears the note to a rest; a second press on the same row clears all four fields |
| EDIT + LFO | undoes the last edit; the next press redoes it (also available with REC off) |
| K1 | instrument number, 00–3F (00 leaves the playing instrument unchanged) |
| K2 | command, 0–F |
| K3 / K4 | data's high / low nibble, independently clamped to 0–F |
| PRESETS | current instrument for note entry and audition, 01–3F |
| EDIT + PRESETS | clockwise inserts an empty row and drops the tail; counter-clockwise removes this row and appends a rest; pattern length stays fixed and locks follow their rows |
| EDIT + ALGORITHM | edit step, 0–16; 0 stays on the same row |

K1–K4 show the instrument, command or data being set in the large value popup. Navigation and
the step/current-instrument selectors do not change the song. With REC off keys only audition
and K1–K4 keep their LIVE functions. An edit waits for the player's acknowledgement before it
can be undone. While arena compaction runs, further edits wait: **TIDYING**. Refusals use large
toasts: **SAVING**, **SONG FULL**, **INVALID EDIT**, or **WAITING** while another edit completes.
The save-stream and stored-song-size checks are integration hooks until songs on the store land.

### ORDER, INSTR and TABLE

SEQ cycles **TRACKER → ORDER → INSTR → TABLE → TRACKER** (STEP joins when implemented).
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

ORDER shows one voice's decoded entries at 2×; the other two have compact raw-hex summaries.
Repeats use `R0` for 16 times; transpose `-10` means minus 16 semitones (hex), not minus zero.
Choosing an unheld pattern creates an empty 64-row pattern before publishing its reference;
the whole request is checked against the serialized-size cap first. Invalid repeat/transpose
sequences are refused without creating a pattern. Instrument names use GT2's 16 raw bytes;
untouched imported bytes are preserved, and name-only instruments follow GoatTracker's normal
save rules. TABLE's footer spells out the selected operation, including absolute/relative
wavetable notes and jumps (SPEED's FF is a speed value, not a jump).

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
lettering (`app/title_card.h`): **THE CHIP** before CHIP, **THE SCENE** before SCENE, and
**THE OSCILLATOR** before LIVE when HOME goes back to it from the menu (not at power-on, after
the boot, nor when MIDI learn's frame closes). The glow fades in, the title
wipes in, the card holds for a second and the view cuts in, about 2.6 s in all.

- **Any button or encoder** ends the card at once and does nothing else (FX does not toggle
  the chip, OCT+ does not step the octave, SELECT does not pick a piece or move CHIP's focus).
- **A note**, from the keys or MIDI, plays as ever and ends the card. The keys, MIDI and the
  audio are never held up by it.
- **CARDS** in the menu turns the title cards off (ALGORITHM counter-clockwise on it) and on
  again (clockwise). They are on by default; the setting persists across power-off.

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
| 27-28 | the tab's note (SONG SETUP  SLOTS SHARED, FOLLOWS KEYS  SLOTS SHARED, SLOTS SHARED WITH CHIP A; nothing on chip A with no song) and the help line in the hint colour: SELECT MOVE ALGORITHM STEP |

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
bleed. Other independent settings stay. A new base profile releases all expert overrides.
The page reminds in the hint colour: **MACRO MOVES RESET ITS OVERRIDES**. During a song the A
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
cutoff, resonance, pulse width, waveform, envelope, filter, expression settings and sync ratio
you had are back (the effect slots, the chip and the volume are not the preset's and stay as
they are). A preset that differs from its factory settings shows a `*` after its name on
LIVE's header, in the popup and in the menu's PRE (`SAW LEAD*`). **EDIT held + PRESETS** puts
the current preset back to its factory settings and forgets its edits. A MIDI program change
loads the factory's settings over any edits the preset kept, as before. The memory is not
saved: a power cycle brings back the preset that was current, with its edits as the settings
keep them (the settings record is unchanged), and the other presets' edits are gone. On MIDI
out, PRESETS sends the program change and then, as CCs, the restored parameters that differ
from the preset's factory values.

**Known limit:** voice 1's frequency register tops out (0xFFFF, 3.85 kHz) from note 90
(F#6), and from lower notes bent or with vibrato upward (note 88 at a bend of +2
semitones, lower at wider bend ranges). Voice 1 then stays there: the pitch is still
right, but the ratio falls, and the timbre thins, toward the top.

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
| DAC sloppiness | 87 | 2/2 | 0..127: leakier, more kinked voice DACs (a released voice still whispers, with BLEED above 0) |
| Output thump | 89 | 2/3 | 0..127: a volume change steps the output, as the 6581's operating point does |
| Filter bleed | 90 | 2/4 | 0..127: the unfiltered voices also reach the filter, up to half; above 0 a released voice's DAC leak is kept as well (below) |

A profile is a chip and a model: changing it resets the chip (held notes attack again on the
new chip, as at a model change), and the model follows it. The model (CC 25, the FX button) selects
the 8580 R5, or goes back to the last 6581 profile chosen. The four controls change the
playing chip without a reset, and glide there rather than jump, so they don't click: from
the next 10 ms tick, age and thump take 0.32 s from 0 to 127, the bleed 1.27 s, the DACs
1.72 s. A
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
algorithm, Amount, A, B, Mix). CC 120 and 123 release every note. The chip model (CC 25,
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
the parameter's CC (above), K4 as a program change. At most one message per parameter
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

Settings load at power-on: CARDS, KEYS, SOUNDS, TRACKER ROWS, the preset and its edits, the keys' chip
model, profile and character, the effect slots, envelope mode, MIDI-learn bindings and the
last firmware version booted. Missing or unreadable settings use the defaults. Octave,
held notes, controllers and the view on screen are not restored; volume follows MASTER.

Changes are saved in the background **5 s after the last change to a persisted value**,
without a toast. A busy store waits; background saves never force audible gaps. With the
current flash timing limits, programs as well as erases wait for quiet. Stop playing and
let release tails fade before powering off, or use SAVE and wait for its completion toast.

The menu's **STORE** item, immediately after CARDS, shows free space outside the store's
reserve pool in kilobytes (for example, **STORE 193K**) or **FOREIGN**. FOREIGN means the region
contains data SIDOU does not own: it is left untouched, and settings cannot persist.

On FOREIGN, turn **ALGORITHM clockwise once**: **FORMAT?**. Turn clockwise again
without leaving the item to confirm. Counter-clockwise or moving the menu cursor cancels
the confirmation. **Formatting destroys the other firmware's data in the store**, erases
all 73 sectors and can interrupt the sound. STORE shows **ERASE n**, then the toast
**STORE FORMATTED**. Current settings are then saved when the store can take them. Formatting
is never automatic; ALGORITHM does not format a SIDOU store.

## SONG

In STEP, TRACKER and its ORDER, INSTR and TABLE pages, a clean SAVE release saves
the loaded song, not the global settings. An unnamed song opens SAVE AS. Unchanged
songs report **SONG SAVED** without rewriting flash. From an accepted SAVE until
completion, including a wait for the store, the header shows **SAVING** and new
song edits are refused with the same toast. Already queued edits finish first;
playback, mute, solo and jam continue. Songs are not autosaved.

The menu's **SONG** item opens the song-file actions:

- **LOAD** lists the store's songs. Loading stops the transport. Unsaved changes
  require confirmation before they can be discarded. If a file is refused, its
  reason appears in a toast and the previous file is reloaded, stopped.
- **NEW** creates one subtune, three one-pattern orderlists and three 64-row
  patterns, with pulse-lead, bass and noise-drum instruments at tempo 6.
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
Closing ABOUT restores that song, stopped. With unsaved changes the music is
silent and the scroller says **SAVE YOUR SONG FOR MUSIC**.

## SAVE

A plain SAVE acts on **release**, not on the initial press. Turning any encoder or pressing
another button while SAVE is down prevents that release from saving.

| Gesture | Result |
|---|---|
| SAVE pressed and released, no encoder turned or other button pressed while down | saves the song in a song view (above); elsewhere saves settings now, skipping the 5 s debounce; **SAVED** when committed |
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
**STORE FOREIGN: NOT SAVED**. Song saves report **SONG SAVED** or **SONG SAVED (GAPS)**;
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
and the slot's algorithm at 2× in the band is in reverse video.

The **status screen** (STATUS in the menu) shows the numbers: the chip, preset, octave,
cutoff, resonance, pulse width, volume and the held notes, and under EFFECTS each slot's
algorithm and Mix (`>` marks the selected slot) with the selected slot's Amount, A and B.

The menu (HOME) is drawn at 2 times the font, six items of the sixteen at a time (the
selected one a line in reverse video; the title bar says which it is of how many, and an arrow
up or down when items are out of sight, and the list scrolls with SELECT). It lists CHIP (the
CHIP view, above), TRACKER (the song views), SCENE (the generative pieces), SONG (the song-file
actions), PRE (the preset), OCTAVE, KEYS, SOUNDS, ROWS (TRACKER ROWS), CARDS (title cards),
STORE, THEME, STATUS, MIDI, CHARSET (the glyph sheet) and ABOUT. A line is 14 characters:
PRE, ROWS and STORE's kilobytes (`193K`, FOREIGN, FORMAT?, `ERASE 20`) are short so that the
longest preset name (NOISE PERC) and value fit beside their labels; the
[popup](#value-popup) says the whole names. LIVE's scopes, TRACKER, CHIP, STATUS and ABOUT keep
the font's own size. A view opens with a wipe from the top and
closes with one from the bottom; CHIP, SCENE, and LIVE back from the menu, open behind a title
card instead, and cut in under it in bands. M2's STEP view will be the default then; for M1 LIVE is.

A change of octave, selected slot or algorithm shows a toast on the bottom line for
1.5 s ("OCTAVE +1", "SLOT MOD", "MOD CHORUS"), whatever made it: a button, the menu or
MIDI. A chip change shows "CHIP 8580" and sweeps raster colours down the border.
