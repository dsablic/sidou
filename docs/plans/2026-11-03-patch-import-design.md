<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Patch import, layered sounds and the filter gap: design

**Status:** design agreed section by section, 2026-11-03. Not yet planned task by task.

## Why

A comparison video (50 SidStation factory patches against SIDOU on an FM-1) drew comments that
SIDOU sounds thin and unlike the SidStation. Reading the comments against our records:

- The video's converter loaded only each patch's main oscillator. A LAB sound is one
  oscillator (`app/lab_sound.h`, 48 bytes), so layered patches cannot be kept at all today.
- The SID core is within reSIDfp's tolerances (decision 0003). Its known misses are 2 to 4 dB
  filter differences in specific conditions. They do not explain "nothing like a SID", but they
  are measured, bounded and worth closing.
- The hardware is not the limit: the chip runs at 360 MHz against a 240 MHz budget, and
  the output is 44.1 kHz I2S.

Two tracks follow, independent of each other.

## Track A: patch import

### Decisions

| Question | Decision |
|---|---|
| Source formats | SidStation `.syx` and MidiBox SID V2 patches, through one shared patch model |
| Target of a patch | A layered LAB sound (up to three oscillator layers) |
| Step tables | Become GT2 song instruments, delivered in a generated one-pattern patch song |
| Lossy fields | Never silent: a loss report is shown and written with the import |

### Architecture

1. **Parsers, `sidou-patch` (new host crate).** One per format, bytes to the patch model, no
   decisions. The model holds three oscillators (waveform, ring, sync, pulse width and sweep,
   ADSR, transpose, detune, vibrato, portamento, table), the filter (type, cutoff, resonance,
   routing, envelope, LFO), four LFOs, three tables, the direct controllers and the voice mode.
2. **Mapper, one pure function.** Patch model to layered LAB sound, plus table instruments, plus
   the loss report.
3. **Delivery.** `sidou-sysex` gets an `import` command that writes the sounds into the USER
   slots over the existing SysEx path, and writes the patch song as a `.sng`. Nothing reaches the
   device without the owner's yes for that step (CLAUDE.md); everything is proven in fm1-emu first.

### SidStation format

Source: pysidstation's `docs/FORMAT.md` (Apache-2.0; read as a specification, no code copied),
which corrects the owner's manual against a real 90-patch bank. Points the parser must honour:

- Messages: `F0`, a 4-byte (`00 45 01 00`) or 5-byte (`00 20 3C 01 00`) prefix, a type byte
  (`02` is a patch dump), a version byte, a 14-bit size, 24 bytes of `2D`, marker `45`, the payload, `F7`.
- **The size field is unreliable** (54 of 90 factory patches are wrong). Length comes from the
  message boundary.
- The payload is a 10-byte ASCII name, then every byte split into two nibbles (packing), **and**
  every parameter byte stored nibble-swapped. Both are undone on load.
- `PATCH_MODE`: **single mode** layers all active oscillators on one key, monophonically; **poly
  mode** is three-note polyphony from one oscillator.
- Detune is 1/64 semitone a step (about plus or minus one semitone in all), not plus or minus half a semitone as the manual says.
- LFO type 5 is Flat; the table's note bytes carry an add or subtract flag in bit 6, with the base
  played note plus OSC_TRANSPOSE.

### Mapping

| SidStation | SIDOU |
|---|---|
| Poly mode, one oscillator | Today's single-oscillator LAB sound |
| Single mode, 1 to 3 oscillators | A layered LAB sound: each layer has waveform, ring, sync, ADSR, pulse, tuning |
| Waveform byte (SID control layout) | The layer's waveform, ring and sync |
| PWM start (x32 is the 12-bit pulse), PWM LFO | Pulse and PWM depth and rate |
| Transpose, detune | The layer's tuning |
| Vibrato LFO, portamento | Vibrato rate, depth and delay, glide |
| Filter type, cutoff, resonance, routing | LAB's filter; routing sets which layers feed it |
| Filter envelope, filter LFO | LAB's filter LFO (nearest control), reported where it differs |
| Table steps | GT2 wave-table rows; relative or absolute notes, transpose folded in |
| Table loop (`FE n`) and end (`FF`) | A jump row, or the table's end |
| Table speed | Delay rows; conversion measured in fm1-emu, not assumed |
| Pulse sweep, filter envelope (table patches) | The instrument's pulse and filter tables |
| Four-LFO matrix | Vibrato, PWM and filter destinations only; the rest reported |
| Held-chord arpeggio per oscillator, direct controllers | Approximated by LAB's arp or dropped, reported |

One table patch becomes up to three instruments, named after the patch (`ANPANMAN 1`), within
GT2's 63-instrument limit. The generated patch song has the three instruments on its three
channels and a pattern that triggers them together, so PLAY auditions it.

### Firmware: the layered LAB sound

- The sound format grows from 48 bytes by about 32 for two more layers. It is versioned like
  1.0 (45 bytes) to 1.5 (48 bytes) was: `LAB_SOUND_BYTES_1_0` style, so kept sounds load unchanged.
- A layered note takes all three voices of a chip, so it is monophonic per chip.
- **Budget gate.** The instruction budget (900 per sample per chip), RAMTEXT (48,736 of 49,152) and
  flash headroom are measured first. A change that does not fit is brought to the owner with the
  numbers; budgets are not raised to make it fit.
- LAB gets cards for the layers. Every parameter an encoder changes shows its popup (CLAUDE.md).

### MidiBox SID

Not yet verified. The layout is in the MIOS SVN (`sid_sysex_table.inc`,
`mbsidv2_sysex_implementation.txt`) and the JSynthLib module. The parser starts only after those
are read and a patch file is checked against them; if the layout cannot be established from
sources, that parser is dropped from the scope and the owner is told.

### Tests

- Parser: hand-built patches covering each field and each of the corrections above; truncated,
  wrong-size and wrong-prefix input. The SidStation factory bank is Elektron's, so it is not
  committed: a local, git-ignored check decodes it to valid ranges and round-trips it.
- Mapper: table-driven, one row per mapping above and one per reported loss.
- Firmware: in fm1-emu. A layered sound renders the three layers; an old 48-byte sound renders
  as before (golden hash unchanged); table instruments play the notes of the source table.
- Host C in sidou-tests, the site and demo page with node tests and `tools/web-demo --check`.
- Docs: `docs/reference/sound-format.md` and `sysex.md` for the new format, the manual's LAB page,
  README, the demo page, the cheat sheet, `fmhub.json` and CHANGELOG, all kept in step
  (`tools/readme-check`).

## Track B: the filter gap

The four known misses, all from decision 0003:

| Miss | Now | Target | Cause |
|---|---|---|---|
| 6581 loud chord, band-pass | 4.37 dB | 3 dB | The op-amps saturate in reSIDfp; `SID_NL_M_MIN` (0.35) stands in; 0.3 clipped |
| 8580 small-signal sweeps | 1.62 / 2.02 / 1.70 dB | 1.0 dB | Warping near Nyquist; no output low-pass on the high-pass |
| 8580 full-scale band-pass | 2.04 dB | record | The same warping |
| 6581 mixer compression and volume-DC step | deferred | none | They go in together or three voices clip |

Order:

1. The 6581 chord band-pass: model saturation on the filter's integrator states in place of the
   `SID_NL_M_MIN` bound, then the mixer compression and the volume-DC step together. The
   headroom test (-0.53 dB) is the safety net.
2. The 8580 warping: pre-warp the coefficients (the bilinear fix). An output pole goes in only if
   all three modes improve at once.
3. Each step is its own commit, test first against the `filter` and `reference` tests, with
   ceilings tightened to the new measured values. Cost is measured in fm1-emu against the budgets.
   A new decision record gives the before and after table.

This will not change the character of a patch. It closes measured differences of 2 to 4 dB.

## Order of work

1. Track B step 1, since it touches only the core and is independent of the format change.
2. Budget measurement for the layered sound; bring the numbers to the owner.
3. The SidStation parser and the patch model, with tests.
4. The mapper for single-oscillator patches, then the loss report.
5. The layered LAB sound and its cards, in fm1-emu.
6. Table instruments and the patch song.
7. The `import` command and docs.
8. Track B step 2, and the MidiBox SID parser if its sources check out.

Steps 1, 3 and 4 do not depend on the budget decision. Steps 5 and 6 do.

## Findings so far

- Elektron's factory bank (`SidStation_Presets_r1.syx`, 90 patches, published on Elektron's
  support site) parses with `sidou-patch`; the local check is `tests/factory_bank.rs` with
  `SIDSTATION_PRESETS` set (the bank is not committed).
- In it, 65 patches are single mode with all three oscillators, 13 with two, 12 with one; 8 are
  poly. 61 patches have a table. Layering and tables are therefore most of the bank, which is why
  the video's one-oscillator conversion sounded thin.
- A table ends with `FF`, with `FE <loop>`, or where the data ends (the last tables may be absent).
- The filter cutoff byte reaches 127 and the filter type 1 to 7 in the bank.
- The 6581 filter's chord band-pass: three cheap saturation models were tried and none kept
  (`docs/design-notes.md`, the 6581 filter section). Track B step 1 is not closed.

## Not decided

- The conversion of a SidStation table speed to GT2 frames: measured against the emulator.
- Whether the filter envelope needs its own LAB control: if the filter LFO stands in for it
  poorly, the owner chooses between a new control and a reported loss.
