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
