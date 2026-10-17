<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# USER chip profile record

A USER chip profile is a chip's setup a player saved from the CHIP view
([controls.md](controls.md#chip)): a built-in base profile and the character's four
controls. Each of the eight slots, USER 1..8, is its own value in the
[store](store-format.md), apart from the [settings record](settings-format.md): saving
one writes 7 bytes under its own key, and the settings record does not grow with them.

| Slot | Key |
|---|---|
| USER 1..8 | `0x101 + n`, n = 0..7 (`STORE_KEY_PROFILE(n)`) |

The settings keep `0x100`; keys below it are the emulator tests'. The value's store type
is `2`. A slot that was never saved has no record (a missing key is an empty slot; a
deleted key too).

Schema 1 is 7 bytes:

| Offset | Bytes | Value |
|---|---|---|
| 0 | 1 | Schema version, `1` |
| 1 | 1 | Base profile, a `SID_CHIP_*`: `0` 6581 R2, `1` 6581 R3, `2` 6581 R4AR, `3` 8580 R5, `4` worn 6581; the model follows it |
| 2 | 1 | Number of control bytes that follow, `4` |
| 3 | 4 | The controls, `0..127` each: age/heat, DAC sloppiness, output thump, filter bleed |

A later schema appends fields after the controls (M2b Task 6's expert layer), so an
older firmware reads a newer record's schema-1 part, and a newer one fills what an older
record lacks with its defaults. A reader treats a record as an empty slot, and keeps
nothing of it, when its schema is `0`, its control count is under 4, it is shorter than
`3 + count` bytes, its base profile is not a built-in one, or a control is over 127.

What a slot holds, and when it is read and written:

- At power-on, after the store mounts, each key is read once (`user_profiles_load`).
  The slots then live in the instrument (`live.user_valid`, `user_base`, `user_ctl`);
  the picker, CC 85 and the CHIP view read them there, with or without a store.
- SAVE on the CHIP view's chip focus puts the slot into the instrument at once and
  queues its write. The frame hands it to the store (one write at a time, the settings'
  included), asking again while the store is busy, and the toast **SAVED TO USER 3**
  comes when it commits. After 2 s of waiting the held flash operations are forced, as
  for an explicit settings save, and the toast says **(GAPS)**. A foreign store keeps
  nothing (**STORE FOREIGN: NOT SAVED**); the slot serves until the next boot.
- A format of the store empties every slot.
- The settings record keeps the keys' current profile and controls as before (the base
  profile, never a USER number); which USER slot a pick came from is not stored, so after
  a power cycle the page shows the base profile, and USER n again once it is picked.
