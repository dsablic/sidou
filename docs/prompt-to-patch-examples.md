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
