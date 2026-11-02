<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Lift a sound into LAB (design, 2026-11-02)

The owner tried 1.5's `.sid` lift on the device: "this is complicated... everything about it
frankly". It took GLO + a note key to solo a voice, REC to arm, a wait for the voice's next note,
then a LIFTED SOUND page after the tune and a SAVE that imported a file into the song. The new
design (approved by the owner, ships in 1.5.0): **lifting a sound is taking a voice into LAB.**

## The flow

1. A tune plays on NOW PLAYING. **REC** asks, large: `LIFT WHICH` / `VOICE? 1 2 3`. Note keys 1, 2
   and 3 light up as the answers; the voice lanes and their level bars stay on screen, so the
   player sees which voice carries the lead. HOME or REC again cancels: the question goes, nothing
   else changes. Paused or stopped, REC says `LIFT: PLAY THE TUNE` and asks nothing.
2. **Note key 1, 2 or 3** picks the voice. SIDOU takes that voice's most recent note from a
   rolling history of the tune's register frames: no solo, no arming, no wait. A note still
   sounding is followed live to its end (at most a second). A voice with no note in the history
   shows `WAITING FOR V2`, large, and its next note is taken; REC or HOME cancels the wait.
3. **LAB opens** with the sound, named `LIFTED V2`, a toast saying what LAB could not keep
   (`LIFTED V2 ~SWEEP`). The keys play it; every LAB page edits it; **SAVE keeps it as a USER
   sound** as any LAB sound (named `LIFTED V2`; EDIT + SAVE names it). HOME goes back to NOW
   PLAYING with the tune as it was, playing on.
4. A digi tune still refuses with one line (`DIGI: CAN'T LIFT`); a note too short to fit says
   `LIFT: NOTE TOO SHORT`.

In a 2SID tune either chip's voice is taken, whichever played the more recent note; in the same
6502 call, chip A's. A 2SID tune's keys are off (as now), so LAB opens silent until the tune stops.

## The history

`psid_lift_past` (`core/psid/psid_lift.h`) is a ring of chip-frames: one record a chip a play call,
each the chip's 25 registers as the call left them plus two flag bytes (a voice's gate went on, a
voice's gate changed, a voice's or the shared registers written more than 8 times: a digi).

| | Bytes |
|---|---|
| A record: 25 registers, 2 flag bytes | 27 |
| The ring: `PSID_LIFT_PAST` 64 records | 1,728 |
| `psid_lift`'s write counts: [2][25] in place of [11] | +39 |
| Freed: `lift_offer` (the LIFTED SOUND page's state) | -84 |
| Freed: `sid_tune`'s lift fields (file write's length, flags, time) | -8 |
| **Net** | **about +1,680** |

A 1SID tune keeps 64 calls (1.28 s PAL, 1.07 s NTSC); a 2SID tune 32 calls of both chips
(0.64 s). A lifted note is at most `PSID_LIFT_FRAMES` (50) calls, as before; 64 keeps a whole
second-long note that ended a quarter second ago.

**Where it lives.** `psid_lift` stays in `song_space` (POOL, laid over `song_work`: 1,243 of
1,384 bytes). The ring cannot join it: POOL has 5,328 free against `REGION_FREE_MIN` 5,120. It
goes in `.bss` (the RAM region, `audio.c`), which has 9,804 free. Measured before the change (emu
gate `the_song_region_is_64_kib_at_pools_start_and_ram_keeps_its_free_minimum`): RAM region 9,804,
POOL 5,328, together 15,132 against `FREE_RAM_MIN` 8,976. After: about 8,120, 5,328 and 13,450.
No budget changes. Overlays considered and not taken: `player_isr`'s psid half leaves 432 bytes
of the song half's 8,532 idle and `song_space` 180, together not enough.

**Cost.** Each register write (in the main loop's 6502 slice, as now) updates the chip's shadow,
its count and the gate flags: a few instructions more than the armed-only path. Each play call
writes one record a chip (about 60 bytes copied, 50 counts scanned): within the slice budget
`PSID_SLICE_WORST` (40,000), measured in emu-gate. The conversion stays a main-loop pass of its
own within `LIFT_CONVERT_WORST` (40,000).

## Code

**Reused as is:** `psid_lift_convert` (frames to a GoatTracker instrument and its tables: base
note, wave and SR rows, the pulse and filter runs, the vibrato fit, cut to fit) and
`lab_song_pull`'s fit of an instrument to a LAB sound (ADSR, waveform, pulse width and PWM,
filter, vibrato; what LAB cannot hold is named in the toast).

**Changed:**
- `psid_lift` records the history on every call; `psid_lift_take` starts a lift from it (the
  most recent gate-on of the voice, followed to its gate change, the next gate-on or 50 calls);
  with none it arms as before. The armed and taking paths read the newest record, so the live
  capture and the history capture are the same code.
- `lab_song_pull` reads an instrument from a source (a song's instrument and tables, or a GTI5
  file's): `lab_song_pull_gti` fits a lifted file without a song. Two fixes the lift needs: a
  wave row of 00 keeps the first wave (it no longer takes the sound's old waveform and says DRUM),
  and a pulse or filter run that turns back and forth (a lifted PWM, which never loops as a table)
  becomes PWM or the filter LFO from its turns; a run one way stays a dropped SWEEP.
- NOW PLAYING: the question and the wait (`sid_view`), keys 1-3 answering and lit, its help card;
  the LIFT help card now covers the question.
- `views`: a lift ready opens LAB on top of NOW PLAYING, the sound pulled as ENV/LFO/FX pull an
  instrument; the lifted sound is no user sound, so SAVE keeps a new one named `LIFTED V2`.
- `main.c`: `tb_lift` takes from the history; `lift_pass` converts and holds the file until the
  view has it (a tune that ends waits for it, as the store write waited before).

**Removed:** the solo requirement, ARM/REC lane marks, the second REC, the LIFTED SOUND page
(`sid_view_offer`), `tb_import` and the transport's `import`, the offer's status fields, the
`LIFTED SOUND` store write and its import into the song.

**The type-3 file.** The FM-1 no longer writes `LIFTED SOUND`. Files already lifted are GTI5
files (type 3) as any: SysEx LIST/GET, the web Library and the store's readers keep them; nothing
that reads them changes.

## Screens

```
NOW PLAYING, asked            waiting                    LAB
 NIGHT DRIVE ON THE           NIGHT DRIVE ON THE         LIFTED V2
 MOTORWAY                     MOTORWAY                   (LAB's OSC page)
 LIFT WHICH                   WAITING FOR V2
 VOICE? 1 2 3                 REC/HOME: CANCEL
 KEYS 1-3: VOICE  HOME: NO
 1 ######                     1 ######
 2 ##                         2
 3 ####                       3 ####
```

## Tests

- sidou-tests `psid_lift.rs`: the history keeps the last records of both chips; a take finds the
  most recent note (an ended one, a sounding one followed live), arms with none, prefers the more
  recent chip, refuses a digi; the conversion tests stay.
- sidou-tests `lab_song.rs`: a lifted GTI file pulls into a LAB sound (waveform, ADSR, PWM from a
  back-and-forth run, filter, vibrato); a song's pull unchanged.
- sidou-tests `sid_view.rs`: REC asks (golden), keys 1-3 lit and answering, REC/HOME cancel,
  paused says why, waiting (golden), a ready lift opens LAB named `LIFTED V2`, HOME back.
- emu-gate `psid_views.rs`: a tune, REC, key 2: LAB with `LIFTED V2` and the lifted AD/SR and
  waveform; SAVE keeps a USER sound; HOME back to the tune still playing; a voice silent so far
  waits and is taken at its next note; a digi tune refuses; the budgets.
