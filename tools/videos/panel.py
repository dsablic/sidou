# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The FM-1's front panel as the videos draw it: where every control is and what it is called.

The geometry is fm1-emu's (crates/fm1-live/src/panel.rs at the revision host/crates/ergo pins,
MIT, Copyright (c) 2026 Joakim Eriksson), which draws the panel after Felucca's controls
diagram: MASTER, SELECT, PRESETS and ALGORITHM top left with OCT- and OCT+ under them, the
screen in the middle, K1-K4 and two rows of six buttons on the right, and the 27 pill keys
(F3..G5) across the bottom. The names are SIDOU's (docs/reference/controls.md "Panel"). Only
coordinates are taken; the drawing is SIDOU's own.
"""

W, H = 1280, 820
LCD = (370, 42, 480, 480)  # x, y, w, h: the 240 x 240 screen at 2x
BEZEL = (352, 24, 516, 516)

# Encoders and the pot: name -> (centre x, centre y, radius).
KNOBS = {
    "MASTER": (110, 135, 44),
    "SELECT": (250, 135, 44),
    "PRESETS": (110, 295, 44),
    "ALGORITHM": (250, 295, 44),
    "K1": (935, 120, 36),
    "K2": (1027, 120, 36),
    "K3": (1119, 120, 36),
    "K4": (1211, 120, 36),
}

# Buttons: name -> (x, y, w, h).
BUTTONS = {"OCT-": (62, 398, 112, 44), "OCT+": (186, 398, 112, 44)}
for _r, _row in enumerate(
    [["FX", "SCL", "ENV", "LFO", "EDIT", "GLO"], ["HOME", "SAVE", "ARP", "SEQ", "PLAY", "REC"]]
):
    for _i, _name in enumerate(_row):
        BUTTONS[_name] = (906 + _i * 58, 232 + _r * 76, 50, 64)

NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]


def note_number(name: str) -> int:
    """`A4`, `C#3` -> MIDI note (C4 = 60)."""
    sharp = len(name) > 1 and name[1] == "#"
    base = "C D EF G A B".index(name[0])
    return (int(name[2 if sharp else 1 :]) + 1) * 12 + base + int(sharp)


def keys() -> dict:
    """Note keys, F3 (53) .. G5 (79): MIDI note -> (x, y, w, h, black)."""
    kw, gap, x0 = 62, 11, 61
    out = {}
    whites = [0, 2, 4, 6, 7, 9, 11, 12, 14, 16, 18, 19, 21, 23, 24, 26]
    for i, n in enumerate(whites):
        out[53 + n] = (x0 + i * (kw + gap), 680, kw, 106, False)
    for n, after in [(1, 0), (3, 1), (5, 2), (8, 4), (10, 5), (13, 7), (15, 8), (17, 9), (20, 11), (22, 12), (25, 14)]:
        cx = x0 + (after + 1) * (kw + gap) - gap // 2
        out[53 + n] = (cx - kw // 2, 574, kw, 94, True)
    return out


KEYS = keys()


def control_box(name: str):
    """The panel rectangle (x, y, w, h) of a control name (a button, an encoder, MASTER or a
    note), or None."""
    if name in BUTTONS:
        return BUTTONS[name]
    if name in KNOBS:
        x, y, r = KNOBS[name]
        return (x - r, y - r, 2 * r, 2 * r)
    try:
        n = note_number(name)
    except (ValueError, IndexError):
        return None
    if n in KEYS:
        return KEYS[n][:4]
    return None
