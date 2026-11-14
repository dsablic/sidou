# SIDOU: write a whole song as text (sidou-song/1)

How to use this: https://dsablic.github.io/sidou/llms.txt tells you how SIDOU works and how to write
a sound (`sidou-patch/1`); read it first, a song's instruments are those sounds. Here you write a
song: a few sections of notes and drums on a step grid, and the order to play them in. The Library
page checks it with the FM-1's own rules, plays it and sends it to the FM-1 as an ordinary song; to
change a song you have, give the model the song's text and ask for the change. Where this text and
https://dsablic.github.io/sidou/patch.schema.json disagree, the schema is right.

## Your answer

1. The song in one ```json block.
2. Its link (see "The link").
3. The tempo that plays, one line: the BPM you asked for and the one in the list below that plays.
4. Two to four things to try in the editor, each one concrete (see "Things to try").

## The shape

```
{"format": "sidou-song/1", "name": "...", "bpm": 125,
 "instruments": { "bass": {a sidou-patch/1 sound}, "lead": {...} },
 "sections": { "A": {"rows": 16, "voices": [voice 1, voice 2, voice 3]}, "B": {...} },
 "order": ["A", "A", "B"] }
```

- `format` is required: `"sidou-song/1"`. `name` is the song's name on the FM-1, 1 to 16 characters.
- `bpm` 60 to 250, default 125. It plays as one of the tempos in "The tempo".
- `instruments`: up to 16 sounds, each named by you (1 to 8 characters). A sound is a
  `sidou-patch/1` patch with `format` left out; leave out any field you do not mean (the sound is
  then LAB's starting sound). Three names are built in and need no entry: `"kit"` is the drum kit,
  `"pulse"` and `"bass"` are the new song's own two sounds. If you give an instrument one of those
  names, yours is used.
- `sections`: named parts of the song, up to 16. Each has `rows` (1 to 128, a row is a sixteenth
  note: 16 rows is one bar of four beats) and `voices`, up to three, one for each of the chip's
  three voices. A voice is `{"instrument": "bass", "notes": "..."}`. A voice you leave out or write
  as `null` is silent for the whole section.
- `order`: the section names in the order they play, up to 64 entries. The song loops from the
  start when it reaches the end.

## The notes string

One token for each row, tokens separated by spaces, exactly as many tokens as `rows`. Count them.

- A note: `C0` to `G#7` (sharps with `#`, no flats: `D#4`, not `Eb4`). `C4` is middle C. A note
  starts at its row, and the sound plays until something else happens.
- `-` holds: the sound goes on, nothing new starts. `.` is a rest: the sound is let go.
- After a note, `!` is an accent (brighter and louder) and `~` a slide (the gate stays held and the
  next note glides from this one, an acid bass's slide). One of them at most: `C2!` or `C2~`.
- With the instrument `"kit"` the tokens are drums, each with an optional `!` for an accent but no `~`: `k` kick, `s` snare,
  `h` closed hat, `o` open hat, `c` clap, `t` low tom, `T` high tom, `r` rim, and the sampled digi
  drums `dk` kick, `ds` snare, `dc` clap, `dh` hat. Drum notes are not pitched: no note names on
  the kit, no drum letters on another instrument.
- A section's first token should be a note, a drum or `.`. A `-` there holds whatever played before.
- Over the song at most 512 accents and slides together fit, and the song as a whole must fit the
  FM-1 (about 200 different patterns): a repeated section costs nothing, because equal parts are
  stored once. Repeat sections in `order` rather than writing them out again.

## The tempo

The FM-1 plays a song at one of a few tempos (four rows a beat). `bpm` is taken to the nearest of
these, and the Library page and `sidou-sysex song` say which one plays. Choose from the list, so the
song plays as asked:

60.1, 62.7, 65.4, 68.4, 71.6, 75.2, 79.1, 83.5, 88.5, 94.0, 100.2, 107.4, 115.7, 125.3, 136.7, 150.4, 167.1, 188.0, 214.8, 250.6

## Making it sound good

- Give each voice its own job: bass low (`C1` to `C3`), a kit of drums, a lead or chords high
  (`C4` to `C6`). Three voices is all there is: no more than one note at a time on each.
- Sections of 8 or 16 rows, an A and a B, then an order such as A A B A, sound like a song. A
  section of 4 rows of `s s s s` is a fill. Silence (`null`) in one voice makes a break.
- An accent and a slide on a bassline make it acid; slides need a following note to glide to.
- Put the sound's design into the instrument, as in the sound instructions: a bass is a saw or a
  pulse, a short envelope and a low-pass filter; a lead is a pulse with some PWM; one instrument
  can be used in several voices and sections.

## Examples

A groove, one section of 16 rows: an acid bass with an accent and a slide, a kit with hats, a rest
for the third voice:

```json
{"format": "sidou-song/1", "name": "GROOVE", "bpm": 125,
  "instruments": {"bass": {"name": "303", "wave": "saw",
    "envelope": {"attack": 0, "decay": 8, "sustain": 8, "release": 8},
    "filter": {"cutoff_hz": 2077, "resonance": 12, "envelope": {"depth": 40, "decay": 6}}}},
  "sections": {"A": {"rows": 16, "voices": [
    {"instrument": "bass", "notes": "C2! . C2~ D#2 . . G2 . C2 . . . A#1 . C2 ."},
    {"instrument": "kit", "notes": "k . h . s . h . k k h . s . h dh"},
    null]}},
  "order": ["A"]}
```

https://dsablic.github.io/sidou/library.html#song=%7B%22format%22%3A%22sidou-song%2F1%22%2C%22name%22%3A%22GROOVE%22%2C%22bpm%22%3A125%2C%22instruments%22%3A%7B%22bass%22%3A%7B%22name%22%3A%22303%22%2C%22wave%22%3A%22saw%22%2C%22envelope%22%3A%7B%22attack%22%3A0%2C%22decay%22%3A8%2C%22sustain%22%3A8%2C%22release%22%3A8%7D%2C%22filter%22%3A%7B%22cutoff_hz%22%3A2077%2C%22resonance%22%3A12%2C%22envelope%22%3A%7B%22depth%22%3A40%2C%22decay%22%3A6%7D%7D%7D%7D%2C%22sections%22%3A%7B%22A%22%3A%7B%22rows%22%3A16%2C%22voices%22%3A%5B%7B%22instrument%22%3A%22bass%22%2C%22notes%22%3A%22C2%21%20.%20C2%7E%20D%232%20.%20.%20G2%20.%20C2%20.%20.%20.%20A%231%20.%20C2%20.%22%7D%2C%7B%22instrument%22%3A%22kit%22%2C%22notes%22%3A%22k%20.%20h%20.%20s%20.%20h%20.%20k%20k%20h%20.%20s%20.%20h%20dh%22%7D%2Cnull%5D%7D%7D%2C%22order%22%3A%5B%22A%22%5D%7D

A song of three sections (A the verse twice, B the change, C a drum fill with the others silent),
with the built-in bass, the kit and one lead of its own; the order A A B A C:

```json
{"format": "sidou-song/1", "name": "THREE PARTS", "bpm": 116,
  "instruments": {"lead": {"name": "LEAD", "wave": "pulse"}},
  "sections": {
    "A": {"rows": 8, "voices": [
      {"instrument": "bass", "notes": "C2 - . C2 D#2 - . G1"},
      {"instrument": "kit", "notes": "k . s . k k s ."},
      {"instrument": "lead", "notes": "C4 - - - D#4 - - -"}]},
    "B": {"rows": 8, "voices": [
      {"instrument": "bass", "notes": "F2 - . F2 G#2 - . C2"},
      {"instrument": "kit", "notes": "k . s . k k s ."},
      {"instrument": "lead", "notes": "F4 - - - G#4 - G4 -"}]},
    "C": {"rows": 4, "voices": [null, {"instrument": "kit", "notes": "s s s s"}, null]}},
  "order": ["A", "A", "B", "A", "C"]}
```

https://dsablic.github.io/sidou/library.html#song=%7B%22format%22%3A%22sidou-song%2F1%22%2C%22name%22%3A%22THREE%20PARTS%22%2C%22bpm%22%3A116%2C%22instruments%22%3A%7B%22lead%22%3A%7B%22name%22%3A%22LEAD%22%2C%22wave%22%3A%22pulse%22%7D%7D%2C%22sections%22%3A%7B%22A%22%3A%7B%22rows%22%3A8%2C%22voices%22%3A%5B%7B%22instrument%22%3A%22bass%22%2C%22notes%22%3A%22C2%20-%20.%20C2%20D%232%20-%20.%20G1%22%7D%2C%7B%22instrument%22%3A%22kit%22%2C%22notes%22%3A%22k%20.%20s%20.%20k%20k%20s%20.%22%7D%2C%7B%22instrument%22%3A%22lead%22%2C%22notes%22%3A%22C4%20-%20-%20-%20D%234%20-%20-%20-%22%7D%5D%7D%2C%22B%22%3A%7B%22rows%22%3A8%2C%22voices%22%3A%5B%7B%22instrument%22%3A%22bass%22%2C%22notes%22%3A%22F2%20-%20.%20F2%20G%232%20-%20.%20C2%22%7D%2C%7B%22instrument%22%3A%22kit%22%2C%22notes%22%3A%22k%20.%20s%20.%20k%20k%20s%20.%22%7D%2C%7B%22instrument%22%3A%22lead%22%2C%22notes%22%3A%22F4%20-%20-%20-%20G%234%20-%20G4%20-%22%7D%5D%7D%2C%22C%22%3A%7B%22rows%22%3A4%2C%22voices%22%3A%5Bnull%2C%7B%22instrument%22%3A%22kit%22%2C%22notes%22%3A%22s%20s%20s%20s%22%7D%2Cnull%5D%7D%7D%2C%22order%22%3A%5B%22A%22%2C%22A%22%2C%22B%22%2C%22A%22%2C%22C%22%5D%7D

## Things to try

End your answer with two to four things to try in the editor (the Library page's Edit button opens
the song in it), each a concrete step on this song, for example:

- "Patterns, pattern 2: change a note of the lead, then Play pattern on voice 3."
- "Orderlists, voice 1: add an entry at the end to play section B twice."
- "Instruments, the lead: raise its pulse width and Preview the song again."
- "Download .sng, or Save to the FM-1, to keep it."

## The link

After the JSON, give `https://dsablic.github.io/sidou/library.html#song=` followed by the JSON on one
line with no spaces outside the strings, percent-encoded the way `https://dsablic.github.io/sidou/llms.txt`
describes (`{` is `%7B`, `"` `%22`, a space `%20` and so on). A link carries up to 8 KB: for a longer song,
give only the JSON block and tell the user to paste it on the Library page.
