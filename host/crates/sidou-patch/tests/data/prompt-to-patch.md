# SIDOU: make a sound as a patch (sidou-patch/1)

How to use this: paste it into a chat with any model, or say "read
https://dsablic.github.io/sidou/llms.txt and make me a squelchy acid bass". The model answers with a
patch and a link; open the link (or paste the answer on https://dsablic.github.io/sidou/library.html),
listen, and send it to the FM-1. To change a sound you have, open it as a patch on that page (a
sound's JSON button, or Patch beside a song's instrument; for one of your USER sounds, the USER sounds
button beside Refresh), give the patch to the model
and ask for the change.

You design sounds for SIDOU, firmware for the FM-1, a synthesizer built around a real MOS SID chip
(a 6581 or an 8580). This text describes SIDOU as of this release; where it and
https://dsablic.github.io/sidou/patch.schema.json or the page disagree, the schema and the page are
right. More depth, to open when you need it:
https://dsablic.github.io/sidou/llms-fields.txt (every field and its range),
https://dsablic.github.io/sidou/llms-lab.txt (what each of LAB's controls does),
https://dsablic.github.io/sidou/llms-examples.txt (more patches, banks and STEP patterns),
https://dsablic.github.io/sidou/llms-song.txt (a whole song: sections of notes and drums, and their order).

## Your answer

1. The patch in one ```json block.
2. Its link (see "The link").
3. Two to four things to turn on the FM-1 for this sound, each as the LAB page, then the control,
   then what it does to this sound, named as LAB shows them (the table below), for example:
   "FILTER ENV, ALGORITHM (depth) up toward +63: more squelch on each note".

## What the sound engine can and can't do

- **Three voices in all.** A sound of one oscillator plays three-note chords. Each entry in
  `layers` adds an oscillator to every note (up to two, so three oscillators); a layered sound plays
  one note at a time.
- **An oscillator** has a wave: triangle (soft, flute-like), saw (bright), pulse (hollow to nasal;
  `pulse_width` 50 is square, 10 to 25 thin and nasal) or noise (drums, wind). Waves joined with `+`
  play together and are very quiet on a 6581. `ring` (ring modulation, bells and metal) and `sync`
  (hard sync, screaming leads) take the oscillator before it as the modulator: layer 1 takes layer 3,
  layer 2 takes layer 1, layer 3 takes layer 2, so they need three layers (or `sync_lead`).
- **The SID envelope** per oscillator, 0 to 15 each. Attack: 0 = 2 ms, 4 = 38 ms, 8 = 100 ms,
  9 = 250 ms, 10 = 500 ms, 12 = 1 s, 15 = 8 s. Decay and release are three times longer (8 = 300 ms,
  9 = 750 ms, 10 = 1.5 s, 12 = 3 s). Sustain is a level (0 silent, 15 full).
- **One analog filter for the whole chip:** `mode` low, band or high (low+high is a notch),
  `cutoff_hz`, `resonance`, its own envelope, and `voices`, the chip voices through it. A
  one-oscillator sound filters all three voices or none (`[1, 2, 3]` or `[]`), because LAB plays
  each key on the next free voice: a voice left out would play some notes unfiltered. A layered
  sound may route layers apart, since layer n is voice n. `cutoff_hz` is as an 8580 plays it; a
  6581 is darker below 300 Hz and brighter above.
- **Movement:** PWM, vibrato with a delay, two LFOs (to pitch, pulse width, cutoff or volume), an
  arpeggio of the keys held, and a step table (a wave and a note a step, for drums, chip chords and
  zaps).
- **Effects after the chip**, one of each: distortion (tape, overdrive, fuzz, crush, lofi),
  modulation (chorus, flanger, phaser, tremolo, autopan, wow) and an effect (delay, pingpong,
  reverb, lush). Give only `type` for the type's own settings; `mix` 0 to 127 is dry to wet.
- **It can't** play samples, more than three voices, or a filter per voice. It is a SID: thin, raw
  and bright is its charm.

## Making it sound good

The page plays every sound on both chips before taking it, and refuses only one neither makes
heard (a wave of `none`, a closed envelope, a filter far from the notes with nothing to open it).
So write the sound you mean.

- **Acid bass:** saw, attack 0, decay 5 to 7, sustain 0, release 2 to 4; low-pass with a low cutoff
  (300 to 700 Hz) and high resonance (12 to 15), the filter envelope doing the work (depth 40 to 60,
  attack 0, decay 4 to 6, sustain 0): the squelch is the resonant peak sweeping down after each
  note. Overdrive (mix 50 to 80) adds bite.
- **Pads:** attack 8 to 10, sustain 10 to 14, release 9 to 11; pulse 30 to 45 with slow PWM (depth
  10 to 25, 0.2 to 0.8 Hz) or a slow LFO on the cutoff; a soft low-pass (2000 to 4000 Hz,
  resonance 2 to 6); chorus and lush. Layers make it fatter but one note at a time.
- **Bells and metal:** triangle layers with ring, the modulator at a non-octave interval (transpose
  7, 15 or 19), attack 0, decay 9 to 11, sustain 0, release 8 to 10; reverb.
- **Leads:** pulse 30 to 45 with PWM (depth 10 to 25, 0.5 to 3 Hz), or saw; vibrato 5 to 6 Hz,
  depth 10 to 20, delay 20 to 40; `sync_lead` for a sync scream; a delay at mix 20 to 30.
- **Drums:** a table (noise, then a triangle falling, frames 1); **chip chords:** a table of 0, 4, 7
  and 12, frames 1 or 2, loop 1. See the examples file.

## The core fields

`format` is required (`"sidou-patch/1"`); every other field may be left out and is then LAB's own
starting sound (a saw, attack 0, decay 9, sustain 10, release 6, low-pass 1200 Hz, resonance 6, all
voices filtered). A layer copies the top-level oscillator but what it names. The common fields:

- `name` up to 12 characters; `wave`; `ring`, `sync`; `pulse_width` 0 to 100;
  `envelope` {`attack`, `decay`, `sustain`, `release` 0 to 15}; `transpose` (semitones), `detune`
  (cents)
- `filter` {`mode`, `cutoff_hz` 0 to 14298, `resonance` 0 to 15, `voices`, `envelope` {`depth` -63
  to 63, `attack`, `decay`, `sustain`, `release`}}
- `pwm` {`depth` 0 to 50, `rate_hz`}; `vibrato` {`rate_hz`, `depth` 0 to 127, `delay`}; `lfos` up
  to 2 of {`shape`, `to`, `rate_hz`, `depth` -63 to 63}; `layers` up to 2 of {the oscillator
  fields}; `table` {`frames`, `loop`, `steps` of {`wave`, `note`}}
- `fx` {`distortion`, `modulation`, `effect`: {`type`, `amount`, `a`, `b`, `mix`}}

Every field, its range and what each knob of each effect is: llms-fields.txt and the schema.

## LAB's controls, for your suggestions

SELECT steps LAB's pages; K1 to K4 turn the four cards; ALGORITHM is the page's choice.

{{LAB_NAMES}}

## Examples

{{EXAMPLES}}

## Several sounds, or a pattern

Asked for several sounds at once ("a drum kit"), answer with a bank: `{"format": "sidou-bank/1",
"name": "ACID KIT", "patches": [...]}`, up to 16 patches, sent as one song; its link is `#bank=`.
Asked for a groove or a bassline, answer with a STEP pattern (`sidou-step/1`, its link `#step=`):
one lane of up to 64 steps with notes or drums, slides, accents and chances. Both formats and
examples: llms-examples.txt. Asked for a whole song (a verse, a chorus, bass, drums and a lead),
answer with a song (`sidou-song/1`, its link `#song=`): sections of step-grid strings, patches as its
instruments, and a play order; read llms-song.txt first.

## Changing a patch

Given a patch and asked for a change ("darker", "a slower attack"), answer with the whole patch:
keep every field you were not asked about as it is, change only what was asked, and keep the name
unless asked.

## The link

After the JSON, give `https://dsablic.github.io/sidou/library.html#patch=` followed by the JSON on
one line with no spaces outside the strings, percent-encoded: `{` is `%7B`, `}` `%7D`, `[` `%5B`,
`]` `%5D`, `"` `%22`, `:` `%3A`, `,` `%2C`, a space `%20`, `+` `%2B`, `/` `%2F`; letters, digits,
`-`, `_` and `.` stay. The page reads the patch from the link and sends nothing until its user
presses Send to FM-1.
