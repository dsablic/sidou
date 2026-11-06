# SIDOU: make a sound as a patch (sidou-patch/1)

How to use this: paste it into a chat with any model, or say "read
https://dsablic.github.io/sidou/llms.txt and make me a squelchy acid bass". The model answers with
one JSON patch and a link; open the link (or paste the JSON on
https://dsablic.github.io/sidou/library.html), listen, and send it to the FM-1. To change a sound
you have, open it as a patch on that page (a sound's JSON button, or Patch beside a song's
instrument; for one of your USER sounds, load it in LAB and press REC to put it in the song, then
Open the FM-1's current song), give the patch to the model and ask for the change.

You, the model: you design one sound for SIDOU, firmware for the FM-1, a synthesizer built around a
real MOS SID chip (a 6581 or an 8580). Answer with the patch in one ```json block, then the patch
link (see "The link"), then at most two sentences on how to play it. Nothing else.

## What the sound engine can and can't do

- **Three voices in all.** A sound of one oscillator plays three-note chords. Each entry in
  `layers` adds an oscillator to every note (up to two, so three oscillators); a layered sound plays
  one note at a time.
- **An oscillator** has a wave: triangle (soft, flute-like), saw (bright), pulse (hollow to nasal;
  `pulse_width` 50 is square, 10 to 25 thin and nasal) or noise (drums, wind). Waves joined with `+`
  play together and are very quiet on a 6581. `ring` (ring modulation, bells and metal) and `sync`
  (hard sync, screaming leads) take the oscillator before it as the modulator: layer 1 takes layer 3,
  layer 2 takes layer 1, layer 3 takes layer 2. Use them with three layers, or `sync_lead`.
- **The SID envelope** per oscillator, 0 to 15 each. Attack: 0 = 2 ms, 2 = 16 ms, 4 = 38 ms,
  6 = 68 ms, 8 = 100 ms, 9 = 250 ms, 10 = 500 ms, 11 = 800 ms, 12 = 1 s, 13 = 3 s, 15 = 8 s.
  Decay and release are three times longer (0 = 6 ms, 8 = 300 ms, 9 = 750 ms, 10 = 1.5 s,
  12 = 3 s, 15 = 24 s). Sustain is a level (0 silent, 15 full).
- **One analog filter for the whole chip:** `mode` low, band or high (low+high is a notch),
  `cutoff_hz`, `resonance`, and `voices`, the chip voices that pass through it (a one-oscillator
  sound plays its notes on voices 1, 2 and 3; layer n is voice n). Voices left out are unfiltered.
  The filter has its own envelope, in cutoff steps, plus `lfo_depth`. `cutoff_hz` is as an 8580
  plays it; a 6581's filter is darker below 300 Hz and brighter above.
- **Movement:** PWM (the pulse width swept by its own LFO), vibrato with a delay, two LFOs
  (each to pitch, pulse width, cutoff or volume), an arpeggio of the keys held, and a step table: each
  step a wave and a note, one step every few 50 Hz frames, for drums, chip chords and zaps.
- **Effects after the chip**, one of each: distortion (tape, overdrive, fuzz, crush, lofi),
  modulation (chorus, flanger, phaser, tremolo, autopan, wow) and an effect (delay, pingpong,
  reverb, lush, a longer reverb). Knobs 0 to 127: `amount`, `a`, `b`, `mix` (dry to wet). Tape:
  amount drive. Overdrive and fuzz: amount drive, a tone (fuzz: octave), b bias. Crush: amount bits,
  a rate. Lofi: amount tone, a hum, b noise. Chorus: amount depth, a rate, b ensemble. Flanger and
  phaser: depth, rate, feedback. Tremolo: depth, rate, shape. Autopan: width, rate, shape. Wow:
  wow, flutter. Delay and pingpong: amount feedback, a time, b damping. Reverb and lush: amount
  decay, a damping, b diffusion (lush: band). Give only `type` for the type's own settings.
- **It can't:** play samples, play more than three voices, filter voices separately, or hold more
  than one sound per patch. It is a SID: thin, raw and bright is its charm.

## Making it sound good

- **The FM-1 refuses a sound its estimate calls silent on both chips** (and warns when one chip
  would not hear it). Keep it clearly audible: sustain 8 or
  more, or with sustain 0 a decay of 9 or more; a low-pass cutoff at least two octaves above the
  notes (2000 Hz or more), or a filter envelope that opens it (depth 25 to 50); no `none` wave;
  pulse width between 10 and 90.
- Bass: saw or pulse 25 to 40, attack 0, decay 6 to 9, sustain 6 to 10, low-pass 2000 to 3000 Hz
  with resonance 10 to 13 and a filter envelope (depth 30 to 50, decay 5 to 8, sustain 0): that is
  the squelch. Voices [1] keeps the filter on the first note only, the 303 way.
- Leads: pulse 30 to 45 with PWM (depth 10 to 25, rate 0.5 to 3 Hz), or saw; vibrato rate 5 to 6 Hz,
  depth 10 to 20, delay 20 to 40 (hundredths of a second); `sync_lead` for a classic sync scream.
- Pads: attack 8 to 10, release 9 to 11, sustain 10 to 14, slow PWM or a slow LFO on the cutoff.
  Layers make a pad fatter (two saws detuned 5 to 15 cents) but one note at a time.
- Bells and metal: triangle layers with ring, the modulator at a non-octave interval (transpose 7,
  15 or 19), decay 9 to 11, sustain 0, release 8 to 10.
- Drums: a table. Kick: noise then triangle falling (`"note": -5` and lower), frames 1. Snare:
  noise, a triangle hit, noise. Hats: noise alone, decay 0 to 2.
- Chip chords: a table of 0, 4, 7 (major) or 0, 3, 7 (minor) and 12, frames 1 or 2, loop 1.
- Effects: a little reverb or delay (mix 20 to 40) flatters leads and pads; overdrive or fuzz suits
  basses and leads.

## Changing a patch

When you are given a patch (its JSON or its link) and asked for a change ("darker", "a slower
attack", "more squelch"), answer with the whole patch: keep every field you were not asked to
change exactly as it is, change only what was asked, and keep the name unless asked.

## The fields

Every field may be left out; what is left out is LAB's own starting sound (a saw, attack 0,
decay 9, sustain 10, release 6, low-pass 1200 Hz, resonance 6, all voices filtered, vibrato
6 Hz at depth 0). A layer is a copy of the top-level oscillator but what it names. A table step
plays its layer's wave unless it names one. An effect takes its type's settings but what it names.
An LFO is a triangle unless it names a shape. Ranges:

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
- `filter` {`mode` low, band and high, one or several joined with + (like "low+band"), or off; `cutoff_hz` 0 to 14298 (Hz, as an 8580 plays it); `resonance` 0 to 15; `voices` a list of chip voices 1 to 3, like [1, 2, 3] (the chip voices through the filter); `resonance_link` -8 to 8 (the resonance follows the cutoff knob); `lfo_depth` 0 to 32 (a cutoff wobble at the PWM rate, 0 off); `envelope` {`depth` -63 to 63 (cutoff steps of 32 at the peak, below 0 closes); `attack`, `decay`, `sustain`, `release` 0 to 15}}
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

## Examples

Each is a request and the patch for it. All sixteen of SIDOU's sounds as patches are listed in
https://dsablic.github.io/sidou/sounds/sidou-sounds.json (each `patch` a file beside it); the JSON
Schema is https://dsablic.github.io/sidou/patch.schema.json.

"a resonant saw bass for STEP's SLIDE and ACCENT locks":

```json
{
  "format": "sidou-patch/1",
  "name": "303 BASS",
  "wave": "saw",
  "envelope": {"attack": 0, "decay": 8, "sustain": 8, "release": 8},
  "filter": {"cutoff_hz": 2077, "resonance": 12, "voices": [1], "envelope": {"depth": 40, "decay": 6}},
  "vibrato": {"rate_hz": 0}
}
```

https://dsablic.github.io/sidou/library.html#patch=%7B%22format%22%3A%22sidou-patch%2F1%22%2C%22name%22%3A%22303%20BASS%22%2C%22wave%22%3A%22saw%22%2C%22envelope%22%3A%7B%22attack%22%3A0%2C%22decay%22%3A8%2C%22sustain%22%3A8%2C%22release%22%3A8%7D%2C%22filter%22%3A%7B%22cutoff_hz%22%3A2077%2C%22resonance%22%3A12%2C%22voices%22%3A%5B1%5D%2C%22envelope%22%3A%7B%22depth%22%3A40%2C%22decay%22%3A6%7D%7D%2C%22vibrato%22%3A%7B%22rate_hz%22%3A0%7D%7D

"a pulse bass with the cutoff wobbling just under 1 Hz":

```json
{
  "format": "sidou-patch/1",
  "name": "WOBBLE BASS",
  "wave": "pulse",
  "pulse_width": 37.5,
  "envelope": {"attack": 0, "decay": 0, "sustain": 15, "release": 9},
  "filter": {"cutoff_hz": 6966, "resonance": 10, "voices": [1]},
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
  "filter": {"cutoff_hz": 1383, "resonance": 8, "voices": [1], "envelope": {"depth": 30, "attack": 10, "decay": 10, "sustain": 6, "release": 10}},
  "vibrato": {"rate_hz": 4, "depth": 12, "delay": 40}
}
```

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

## The link

After the JSON, give the link
`https://dsablic.github.io/sidou/library.html#patch=` followed by the JSON on one line with no
spaces outside the strings, percent-encoded: write `{` as `%7B`, `}` as `%7D`, `[` as `%5B`, `]` as
`%5D`, `"` as `%22`, `:` as `%3A`, `,` as `%2C`, a space as `%20` and `+` as `%2B`; letters, digits,
`-`, `_` and `.` stay. For example, `{"format":"sidou-patch/1","name":"HI"}` is
`https://dsablic.github.io/sidou/library.html#patch=%7B%22format%22%3A%22sidou-patch%2F1%22%2C%22name%22%3A%22HI%22%7D`
(`/` is `%2F`). The page reads the patch from the link and sends nothing until its user presses
Send to FM-1.
