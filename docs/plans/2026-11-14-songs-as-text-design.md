<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Whole songs as text (`sidou-song/1`): design

1.10's first item ([roadmap](../ROADMAP.md)): a text form of a whole song that a model or a coding
agent writes, which opens in the site's editor and reaches the FM-1 as an ordinary `.sng`.
Agreed with the owner on 2026-11-14.

## Decisions

- **For writing new songs, one way.** A musical form a model writes well; the converter lays it
  out as GoatTracker patterns and orderlists. An existing song is not turned back into it (a
  lossless tracker mirror was the alternative, not chosen).
- **Step grid strings.** One token per row (a 16th note), one string per voice per section: compact
  links, rows one to one, no beat arithmetic for a model to get wrong.
- **A token says** a note, a hold, a rest, accent and slide; the instrument is named per voice per
  section. No raw tracker commands, no per-step chance.
- **The converter is C**, beside `platform/web/patch_json.c`, for the reasons recorded there: the
  rules it applies are the firmware's C (`song_io_template`, `gt2_set_row`, `song_lock_put`, LAB's
  encoder and checks), the site runs that C as wasm, and Rust (`sidou-patch`) wraps it for the
  tests, the docs and `sidou-sysex`. No firmware change.

## The format

```json
{
  "format": "sidou-song/1",
  "name": "NIGHT DRIVE",
  "bpm": 120,
  "instruments": {
    "bass": {"name": "303", "wave": "saw", "envelope": {"attack": 0, "decay": 8, "sustain": 8, "release": 8}},
    "lead": {"name": "LEAD", "wave": "pulse"}
  },
  "sections": {
    "A": {"rows": 16, "voices": [
      {"instrument": "bass", "notes": "C2! . C2~ D#2 . . G2 . C2 . . . A#1 . C2 ."},
      {"instrument": "kit", "notes": "k . h . s . h . k k h . s . h h"},
      {"instrument": "lead", "notes": "C4 - - - D#4 - - - G4 - - - - - . ."}
    ]}
  },
  "order": ["A", "A"]
}
```

- `name`: the song's name on the FM-1, 1 to 16 characters.
- `bpm`: 60 to 250. The converter picks the nearest tempo or funktempo pair and reports the BPM
  that plays. At four rows a beat these come in half-speed steps (125.3, 115.7, 136.7, 107.4 ...),
  so "asked 120" plays 115.7 or 125.3; the docs list the steps.
- `instruments`: up to 16 named `sidou-patch/1` sounds (`format` optional). The names `"kit"` (the
  NEW song's drum kit), `"pulse"` and `"bass"` (its own two sounds) are built in, as in
  `sidou-step/1`.
- `sections`: named sections, each `rows` 1 to 128 and up to three voices (the first chip's).
  A voice left out or `null` rests for the section.
- `notes` tokens, separated by spaces:
  - `C0` to `G#7`: a note; `!` after it an accent, `~` a slide (STEP's meanings);
  - `-`: hold (nothing written; at a section's start it holds whatever plays);
  - `.`: rest (a key-off);
  - with `"kit"`: `k` kick, `s` snare, `h` closed hat, `o` open hat, `c` clap, `t` low tom,
    `T` high tom, `r` rim, `dk` `ds` `dc` `dh` the digi kick, snare, clap and hat; `!` after a drum
    is an accent as in STEP, `~` is refused (a slide means nothing on a drum).
- `order`: the sections in play order, looped from the start.

Not in 1.10: the second chip's voices (Stereo songs), raw tracker commands, per-step chance.

## The converter

- Starts from the NEW song's template, as `sidou-step/1` does, so the song is one the FM-1 makes.
- Instruments are placed as a bank places them (tables end to end, then each kept).
- Each section's voice becomes a pattern written with `gt2_set_row`; accent and slide use STEP's
  locks and held gate. Identical patterns are stored once (208 patterns and 64 KiB are the limits).
- The three orderlists follow `order` and loop to the start.

## Where a song goes

- **A `#song=` link** (the compact JSON, percent-encoded, as `#patch=`) opens it in the site's
  editor. Over the 8 KB link limit the site says so and offers paste.
- **The Library page** takes it pasted, like a patch or a bank, and previews it.
- **`sidou-sysex song file.json`** writes the `.sng`; `sidou-sysex send` puts it on the FM-1.
- **For models:** `llms.txt` gains a short "a whole song" section pointing to a generated
  `llms-song.txt` (the format, a groove and a three-section song as worked examples, the drum
  tokens); `llms-full.txt` includes it; the schema gains `sidou-song/1`.

## Errors

In the patches' style, `path: what is wrong. Allowed: what is.`:

- `sections.A.voices[0].notes: token 7 "C#9" is not a note. Allowed: C0 to G#7, - . and ! or ~ after a note.`
- `sections.B.voices[2].notes: 15 tokens, rows is 16.`
- `order[3]: no section "C". Sections: A, B.`
- drum tokens on a voice whose instrument is not `"kit"`; an instrument not in the list;
- a song that does not fit: the FM-1's words (`SONG FULL`), naming what to shorten.

## Tests

- **Host:** every documented example converts and the FM-1's reader takes the `.sng`; gt2-ref
  plays each token at its row (a note's frequency, a key-off on `.`, nothing on `-`); a lane
  written as `sidou-step/1` and as `sidou-song/1` gives the same pattern and locks; identical
  sections share patterns; the reported BPM is the one that plays; each refusal has its words.
- **Emulator:** a song sent with `sidou-sysex send` into fm1-emu is heard, at pitch and on time
  (the first-use tests' judges).
- **Site:** `web/patch.test.mjs` against the host build; a `#song=` link opens the editor; a
  pasted song previews in the Library.
- **Docs:** `llms-song.txt` generated from its template; a test reads every example back.

## Size

The patch wasm is about 77 KB of its 80 KiB budget (the owner raised it to 88 KiB on 2026-11-14). Measure first; raising the budget or giving
songs their own module is the owner's call when the number is known.

## Docs to keep current

README, `web/index.html`, the cheat sheet where it applies, `fmhub.json`, CHANGELOG `[Unreleased]`.
