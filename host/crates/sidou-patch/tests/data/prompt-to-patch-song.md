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
- With the instrument `"kit"` the tokens are drums, and no `!` or `~`: `k` kick, `s` snare,
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

{{BPM_STEPS}}

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
{{GROOVE}}```

{{GROOVE_LINK}}

A song of three sections (A the verse twice, B the change, C a drum fill with the others silent),
with the built-in bass, the kit and one lead of its own; the order A A B A C:

```json
{{THREE_PARTS}}```

{{THREE_PARTS_LINK}}

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
