<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Art packs: your own RED MOON RITE

SCENE's RED MOON RITE draws its pictures from an **art pack**, a file in the FM-1's store
([the format](../reference/art-pack.md)). The site's
[art pack builder](https://dsablic.github.io/sidou/art.html) makes your own: replace the
fighters, the swords, the skies, the arena or the status bar with your PNGs, watch the duel play
with them, then download the `.sidart` or send it to the FM-1. The page's checks and its preview
are SIDOU's own code, compiled to WebAssembly: a pack it accepts, the FM-1 accepts, and the
preview is drawn as SCENE draws it. Nothing leaves your computer.

Run locally, `tools/web-demo` serves it at `http://localhost:8642/art.html`
([BUILDING.md](../../BUILDING.md#browser-demo-toolsweb-demo)).

## 1. Start from SIDOU's art

The page opens with the pack SIDOU ships and shows its twelve **slots**, each with its frames,
its size and its frame count:

| Slot | Frames | Stored size |
|---|---|---|
| The left fighter, armed | guard, guard 2, raise, chop, block, spin A, spin B, hit | 64 × 124 |
| The right fighter, armed | the same poses, drawn mirrored | 64 × 124 |
| The left fighter, unarmed | guard, guard 2, hit, reach | 64 × 124 |
| The right fighter, unarmed | the same, drawn mirrored | 64 × 124 |
| The flying sword | eight turns of 45° | 42 × 84 |
| The sword stuck in the ground | one | 16 × 44 |
| The sky | closed, half-open and open filter | 240 × 124 |
| The arena | one | 240 × 240 |
| The status bar, a stamina blade empty and full | one each | 240 × 16 |
| A blade's tip | one | 6 × 5 |

**Open a .sidart…** continues a pack you made before; **Start over from SIDOU's art** goes back.
The fighters' shadows, the torches' flames, the sparks, the sword's arc and the skies' colour
gradients are drawn by the FM-1 itself, not taken from the pack.

## 2. Paint a slot

1. **Template PNG** downloads the slot as it is now: **its frames side by side in one row**, each
   at the slot's size in **stored pixels**. The fighters and swords are stored at half width (each
   stored pixel is two on the screen), so their templates look narrow; paint them that way. The
   right fighter is painted facing as the left one; the scene mirrors him.
2. Paint over it in any editor, keeping the PNG's size. Transparent pixels (under half opacity)
   show what is behind. A sky is painted whole: where your sky equals the FM-1's gradient (or is
   transparent) the gradient shows, so the sky still turns with the filter.
3. **Replace…** takes it back. The page refuses a PNG of the wrong size and says what it wants
   (for a fighter: "takes a 512 × 124 PNG, its 8 frames of 64 × 124 side by side in a row"), and
   reduces the colours to the screen's 16, the nearest of each, showing your PNG and the reduced
   one side by side and how many pixels moved. Paint in the template's colours and nothing moves.
4. **Back to SIDOU's** undoes a slot.

The pack needs one colour that a frame does not use, to mean transparent: a frame using all 16 is
refused. A pack is at most 65,536 bytes; very busy art (a colour change every pixel or two) can go
over, and the page says so.

## 3. Watch it

The preview plays the duel with your pack, by RED MOON RITE's own code: the kick is a chop, the
snare a parry, the bar-4 fill disarms a fighter (his unarmed poses, the flying and the stuck
sword), and the sky turns through its three palettes. **Pause** holds a frame.

## 4. Name it, download it, send it

**Name** is the pack's name inside the file, 1 to 16 characters of the FM-1's font. **Download
.sidart** saves it. **Send it to the FM-1** (Chrome or Edge, Web MIDI with SysEx): Connect, check
the pair, then **Send the pack**; it replaces the art on the FM-1, after asking. On the FM-1:
menu, **SCENE**, then SELECT to RED MOON RITE. The [Library](library.md) sends a `.sidart` too.

A pack is 30 to 64 KB, too big for a link like a sound's: share the `.sidart` file.
