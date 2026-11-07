# SIDOU: make a sound as a patch (sidou-patch/1)

How to use this: paste it into a chat with any model, or say "read
https://dsablic.github.io/sidou/llms.txt and make me a squelchy acid bass". The model answers with a
patch and a link; open the link (or paste the answer on https://dsablic.github.io/sidou/library.html),
listen, and send it to the FM-1. To change a sound you have, open it as a patch on that page (a
sound's JSON button, or Patch beside a song's instrument; for one of your USER sounds, load it in LAB
and press REC to put it in the song, then Open the FM-1's current song), give the patch to the model
and ask for the change.

You design sounds for SIDOU, firmware for the FM-1, a synthesizer built around a real MOS SID chip
(a 6581 or an 8580). This text describes SIDOU as of this release; where it and
https://dsablic.github.io/sidou/patch.schema.json or the page disagree, the schema and the page are
right. More depth, to open when you need it:
https://dsablic.github.io/sidou/llms-fields.txt (every field and its range),
https://dsablic.github.io/sidou/llms-lab.txt (what each of LAB's controls does),
https://dsablic.github.io/sidou/llms-examples.txt (more patches, banks and STEP patterns).

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

- OSC: PULSE, PWM, PWM HZ, TUNE; ALGORITHM the wave
- ENVELOPE: ATTACK, DECAY, SUS, RELEASE; ALGORITHM the envelope mode, for every sound
- FILTER: CUTOFF, RESO, VOICES, LFO; ALGORITHM LP, BP, HP or NOTCH
- FILTER ENV: ATTACK, DECAY, SUS, RELEASE; ALGORITHM depth: up toward +63, more squelch on each note; below 0 it sweeps down
- MOD: VIB HZ, DEPTH, DELAY, RATE; ALGORITHM the arpeggio's order, or off
- LFO: EDIT, DEST, RATE, DEPTH; ALGORITHM the shape, or off
- FX: AMT, A, B, MIX; ALGORITHM the slot's type; SCL selects the slot
- MACRO: RES LNK, PWM LNK; ALGORITHM nothing
- LAYERS: EDIT, TRANS, DETUNE, FILT; ALGORITHM one, two or three oscillators
- TABLE: STEP, WAVE, NOTE, FRAMES; ALGORITHM off, play once, or loop

## Examples

"a squelchy acid bass":

```json
{
  "format": "sidou-patch/1",
  "name": "SQUELCH 2",
  "wave": "saw",
  "envelope": {"attack": 0, "decay": 6, "sustain": 0, "release": 3},
  "filter": {"cutoff_hz": 497, "resonance": 14, "envelope": {"depth": 50, "decay": 5, "release": 3}},
  "fx": {"distortion": {"type": "overdrive", "mix": 70}}
}
```

https://dsablic.github.io/sidou/library.html#patch=%7B%22format%22%3A%22sidou-patch%2F1%22%2C%22name%22%3A%22SQUELCH%202%22%2C%22wave%22%3A%22saw%22%2C%22envelope%22%3A%7B%22attack%22%3A0%2C%22decay%22%3A6%2C%22sustain%22%3A0%2C%22release%22%3A3%7D%2C%22filter%22%3A%7B%22cutoff_hz%22%3A497%2C%22resonance%22%3A14%2C%22envelope%22%3A%7B%22depth%22%3A50%2C%22decay%22%3A5%2C%22release%22%3A3%7D%7D%2C%22fx%22%3A%7B%22distortion%22%3A%7B%22type%22%3A%22overdrive%22%2C%22mix%22%3A70%7D%7D%7D

FILTER ENV, ALGORITHM (depth) up toward +63: more squelch on each note. FILTER, RESO (K2) down: less ringing, rounder. ENVELOPE, DECAY (K2) up: longer notes.

"a ring-modulated triangle that rings out like a bell":

```json
{
  "format": "sidou-patch/1",
  "name": "RING BELL",
  "wave": "triangle",
  "ring": true,
  "envelope": {"attack": 0, "decay": 10, "sustain": 0, "release": 8},
  "filter": {"cutoff_hz": 14298, "resonance": 0, "voices": []},
  "vibrato": {"rate_hz": 0},
  "layers": [
    {"ring": false},
    {"ring": false, "transpose": 19}
  ]
}
```

## Several sounds, or a pattern

Asked for several sounds at once ("a drum kit"), answer with a bank: `{"format": "sidou-bank/1",
"name": "ACID KIT", "patches": [...]}`, up to 16 patches, sent as one song; its link is `#bank=`.
Asked for a groove or a bassline, answer with a STEP pattern (`sidou-step/1`, its link `#step=`):
one lane of up to 64 steps with notes or drums, slides, accents and chances. Both formats and
examples: llms-examples.txt.

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

# SIDOU: every patch field

The ranges of `sidou-patch/1`, as the FM-1's converter checks them (the exact contract: https://dsablic.github.io/sidou/patch.schema.json). A field left out is LAB's starting sound.

- `format` "sidou-patch/1"
- `name` up to 12 characters: A to Z, 0 to 9, space and !"#$%&'()*+,-./:;<=>?@[\]^_
- `wave` triangle, saw, pulse and noise, one or several joined with + (like "triangle+saw"), or none
- `ring` true or false
- `sync` true or false
- `pulse_width` 0 to 100 (percent of the cycle, 50 is square)
- `envelope` {`attack`, `decay`, `sustain`, `release` 0 to 15}
- `transpose` -24 to 24 (semitones)
- `detune` -99 to 99 (cents)
- `fine_tune` -50 to 50 (cents, the whole sound)
- `filter` {`mode` low, band and high, one or several joined with + (like "low+band"), or off; `cutoff_hz` 0 to 14298 (Hz, as an 8580 plays it); `resonance` 0 to 15; `voices` a list of chip voices 1 to 3, like [1, 2, 3] (the chip voices through the filter: all three or none on a one-oscillator sound); `resonance_link` -8 to 8 (the resonance follows the cutoff knob); `lfo_depth` 0 to 32 (a cutoff wobble at the PWM rate, 0 off); `envelope` {`depth` -63 to 63 (cutoff steps of 32 at the peak, below 0 closes); `attack`, `decay`, `sustain`, `release` 0 to 15}}
- `pwm` {`depth` 0 to 50 (percent of the cycle swept); `rate_hz` 0 to 12.75; `link` -8 to 8 (the PWM depth follows the pulse width knob)}
- `sync_lead` {`on` true or false (oscillator 1 hard-synced to a silent one; one note at a time); `ratio` 0 to 15.996 (the synced pitch over the note's, 2.75 is SYNC LEAD's); `semitones` -12 to 12 (added to the ratio)}
- `vibrato` {`rate_hz` 0 to 12.7; `depth` 0 to 127 (127 is about a semitone each way); `delay` 0 to 127 (hundredths of a second after the note starts)}
- `arp` {`on` true or false; `rate` 1fr, 2fr, 3fr, 4fr, 6fr, 8fr, 1/32, 1/16 or 1/8 (50 Hz frames a note, or a MIDI clock division); `order` up, down, up_down or played; `octaves` 1 to 2}
- `bend_range` 0 to 24 (semitones)
- `velocity` off, sustain, cutoff or pulse_width (what the key velocity moves)
- `mod_wheel` pulse_width, vibrato or cutoff (what the wheel moves)
- `aftertouch` off, vibrato, pulse_width or cutoff (what pressure moves)
- `layers` up to 2 of {`wave`, `ring`, `sync`, `pulse_width`, `envelope`, `transpose`, `detune` as at the top}
- `table` {`on` true or false; `layer` 1 to 3 (the layer it plays on; a one-oscillator sound plays it on every note); `loop` 0 to 16 (0 plays once and holds the last step, n goes back to step n); `frames` 1 to 16 (50 Hz frames a step); `steps` up to 16 of {`wave` triangle, saw, pulse and noise, one or several joined with + (like "triangle+saw"), or none; `test` true or false (the SID's test bit: silences and resets the oscillator); `ring`, `sync` true or false; `note` -32 to 95 semitones from the key played, a fixed note "C#0" to "B7", or "hold"}}
- `lfos` up to 2 of {`shape` off, triangle, saw_up, saw_down, square or sample_hold; `to` pitch, pulse_width, cutoff or volume; `rate_hz` 0.05 to 12.75 (snapped to the nearest of 32 rates); `depth` -63 to 63 (below 0 inverts; 63 on pitch is an octave)}
- `fx` {`distortion` {`type` off, tape, overdrive, fuzz, crush or lofi; `amount`, `a`, `b`, `mix` 0 to 127}; `modulation` {`type` off, chorus, flanger, phaser, tremolo, autopan or wow; `amount`, `a`, `b`, `mix` 0 to 127}; `effect` {`type` off, delay, pingpong, reverb or lush; `amount`, `a`, `b`, `mix` 0 to 127}}

The effects' knobs, 0 to 127: tape: amount drive. Overdrive and fuzz: amount drive, a tone (fuzz: octave), b bias. Crush: amount bits, a rate. Lofi: amount tone, a hum, b noise. Chorus: amount depth, a rate, b ensemble. Flanger and phaser: depth, rate, feedback. Tremolo: depth, rate, shape. Autopan: width, rate, shape. Wow: wow, flutter. Delay and pingpong: amount feedback, a time, b damping. Reverb and lush: amount decay, a damping, b diffusion (lush: band).

# SIDOU: LAB's controls

What each of LAB's pages and controls does, turned up (clockwise), for suggesting what to turn on the FM-1. SELECT steps the pages; K1 to K4 turn the four cards; ALGORITHM is the page's choice.

| Page | Control | Turning it |
|---|---|---|
| OSC | K1 PULSE | the pulse width: toward 50 fuller, toward the ends thinner |
| OSC | K2 PWM | the PWM depth: up, a wider sweep |
| OSC | K3 PWM HZ | the PWM rate: up, faster |
| OSC | K4 TUNE | fine tune |
| OSC | ALGORITHM | the wave |
| ENVELOPE | K1 ATTACK | up: a slower start |
| ENVELOPE | K2 DECAY | up: falls more slowly |
| ENVELOPE | K3 SUS | up: a louder held part, longer notes |
| ENVELOPE | K4 RELEASE | up: a longer tail after the key |
| ENVELOPE | ALGORITHM | the envelope mode, for every sound |
| FILTER | K1 CUTOFF | up: brighter |
| FILTER | K2 RESO | up: a sharper, ringing peak |
| FILTER | K3 VOICES | the voices through the filter |
| FILTER | K4 LFO | the filter LFO's depth: up, a wider wobble at PWM HZ |
| FILTER | ALGORITHM | LP, BP, HP or NOTCH |
| FILTER ENV | K1 ATTACK | up: the sweep rises more slowly |
| FILTER ENV | K2 DECAY | up: the wow falls more slowly |
| FILTER ENV | K3 SUS | up: the filter stays open longer |
| FILTER ENV | K4 RELEASE | up: it closes more slowly after the key |
| FILTER ENV | ALGORITHM | depth: up toward +63, more squelch on each note; below 0 it sweeps down |
| MOD | K1 VIB HZ | the vibrato's rate |
| MOD | K2 DEPTH | the vibrato's depth |
| MOD | K3 DELAY | up: the vibrato comes in later |
| MOD | K4 RATE | the arpeggio's speed |
| MOD | ALGORITHM | the arpeggio's order, or off |
| LFO | K1 EDIT | LFO 1 or 2 |
| LFO | K2 DEST | what it moves: pitch, pulse width, cutoff or volume |
| LFO | K3 RATE | up: faster |
| LFO | K4 DEPTH | up: more; below 0 inverted |
| LFO | ALGORITHM | the shape, or off |
| FX | K1 AMT | the selected slot's amount (named for its type, overdrive's DRIVE) |
| FX | K2 A | its second knob (overdrive's TONE) |
| FX | K3 B | its third (overdrive's BIAS) |
| FX | K4 MIX | up: wetter |
| FX | ALGORITHM | the slot's type; SCL selects the slot |
| MACRO | K1 RES LNK | the resonance follows the cutoff knob |
| MACRO | K2 PWM LNK | the PWM depth follows the pulse width knob |
| MACRO | ALGORITHM | nothing |
| LAYERS | K1 EDIT | the layer OSC and ENVELOPE edit |
| LAYERS | K2 TRANS | its transpose |
| LAYERS | K3 DETUNE | up: wider, fatter |
| LAYERS | K4 FILT | the layer into the filter or not |
| LAYERS | ALGORITHM | one, two or three oscillators |
| TABLE | K1 STEP | the step the others edit |
| TABLE | K2 WAVE | its wave |
| TABLE | K3 NOTE | its pitch |
| TABLE | K4 FRAMES | up: slower steps |
| TABLE | ALGORITHM | off, play once, or loop |

# SIDOU: more examples

More patches (`sidou-patch/1`), a bank and a STEP pattern, as https://dsablic.github.io/sidou/llms.txt describes them.

"a pulse bass with the cutoff wobbling just under 1 Hz":

```json
{
  "format": "sidou-patch/1",
  "name": "WOBBLE BASS",
  "wave": "pulse",
  "pulse_width": 37.5,
  "envelope": {"attack": 0, "decay": 0, "sustain": 15, "release": 9},
  "filter": {"cutoff_hz": 6966, "resonance": 10},
  "vibrato": {"rate_hz": 0},
  "lfos": [
    {"to": "cutoff", "rate_hz": 0.9, "depth": 40}
  ]
}
```

"a saw pad that opens slowly, with a late vibrato":

```json
{
  "format": "sidou-patch/1",
  "name": "SWEEP PAD",
  "wave": "saw",
  "envelope": {"attack": 8, "decay": 10, "sustain": 10, "release": 9},
  "filter": {"cutoff_hz": 1383, "resonance": 8, "envelope": {"depth": 30, "attack": 10, "decay": 10, "sustain": 6, "release": 10}},
  "vibrato": {"rate_hz": 4, "depth": 12, "delay": 40}
}
```

"a noise click into a triangle that drops in pitch":

```json
{
  "format": "sidou-patch/1",
  "name": "NOISE KICK",
  "wave": "triangle",
  "envelope": {"attack": 0, "decay": 0, "sustain": 10, "release": 0},
  "filter": {"cutoff_hz": 14298, "resonance": 0, "voices": []},
  "vibrato": {"rate_hz": 0},
  "table": {
    "steps": [
      {"wave": "noise", "note": 40},
      {},
      {"note": -5}
    ]
  }
}
```

"a major arpeggio on one key, the C64 way":

```json
{
  "format": "sidou-patch/1",
  "name": "CHIP CHORD",
  "wave": "pulse",
  "pulse_width": 25,
  "envelope": {"attack": 0, "decay": 0, "sustain": 12, "release": 4},
  "filter": {"cutoff_hz": 14298, "resonance": 0, "voices": []},
  "vibrato": {"rate_hz": 0},
  "table": {
    "loop": 1,
    "frames": 2,
    "steps": [
      {},
      {"note": 4},
      {"note": 7},
      {"note": 12}
    ]
  }
}
```

## A bank (`sidou-bank/1`)

`name` is the song's name on the FM-1 (1 to 16 characters); `patches` holds 1 to 16 patches, each its `format` optional. All sixteen of SIDOU's sounds as a bank: https://dsablic.github.io/sidou/sounds/sidou-sounds-bank.json. Three of them:

```json
{
  "format": "sidou-bank/1",
  "name": "THREE",
  "patches": [
    {
      "name": "PWM STRINGS",
      "wave": "pulse",
      "envelope": {"attack": 0, "decay": 8, "sustain": 10, "release": 8},
      "filter": {"cutoff_hz": 9078, "resonance": 4},
      "vibrato": {"rate_hz": 0},
      "lfos": [
        {"to": "pulse_width", "rate_hz": 0.4, "depth": 28}
      ]
    },
    {
      "name": "303 BASS",
      "wave": "saw",
      "envelope": {"attack": 0, "decay": 8, "sustain": 8, "release": 8},
      "filter": {"cutoff_hz": 2077, "resonance": 12, "envelope": {"depth": 40, "decay": 6}},
      "vibrato": {"rate_hz": 0}
    },
    {
      "name": "WOBBLE BASS",
      "wave": "pulse",
      "pulse_width": 37.5,
      "envelope": {"attack": 0, "decay": 0, "sustain": 15, "release": 9},
      "filter": {"cutoff_hz": 6966, "resonance": 10},
      "vibrato": {"rate_hz": 0},
      "lfos": [
        {"to": "cutoff", "rate_hz": 0.9, "depth": 40}
      ]
    }
  ]
}
```

## A STEP pattern (`sidou-step/1`)

The FM-1 gets it as a song that opens on STEP (its 16 step keys) and plays. `format` "sidou-step/1"; `name` the song's name (1 to 16 characters); `voice` 1 to 3, the lane; `role` auto, drums, bass, arp, lead, none, bass+drums or acid (what DICE and the SCENE hear it as); `gate` staccato (a key off after each note, the default) or legato; `pages` 1 to 4 of 16 steps. `instrument`: a patch (its `format` optional), "pulse" or "bass" (the new song's own), or "kit" (its drums). `steps`: one entry a step that plays, `step` 1 to 64; `note` "C0" to "G#7" (default "C3"); with the kit, `drum` kick, snare, closed_hat, open_hat, clap, low_tom, high_tom, rim, digi_kick, digi_snare, digi_clap or digi_hat; `slide` true (the gate held to the next note, which glides from this one, a 303's slide); `accent` true (brighter and louder); `chance` 0 to 100 (its chance to play each pass). Steps left out are rests.

An acid line:

```json
{
  "format": "sidou-step/1",
  "name": "ACID LINE",
  "voice": 1,
  "role": "acid",
  "gate": "staccato",
  "pages": 1,
  "instrument": {
    "name": "303",
    "wave": "saw",
    "envelope": {"attack": 0, "decay": 8, "sustain": 8, "release": 8},
    "filter": {"cutoff_hz": 2077, "resonance": 12, "envelope": {"depth": 40, "decay": 6}},
    "vibrato": {"rate_hz": 0}
  },
  "steps": [
    {"step": 1, "note": "C2", "accent": true},
    {"step": 3, "note": "C2", "slide": true},
    {"step": 4, "note": "D#2"},
    {"step": 7, "accent": true},
    {"step": 9, "note": "G2", "chance": 75},
    {"step": 11, "note": "A#2", "slide": true},
    {"step": 12},
    {"step": 15, "note": "F2"}
  ]
}
```
