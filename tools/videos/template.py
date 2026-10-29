# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The videos' look, in one place: the layouts, the colours, the fonts, the wipes and the cards.

SIDOU's own art: the C64 palette as the firmware draws it (read from the capture's frames), a
C64-style border round a dark paper, raster bars from the boot screen and the logo, the
device's 8x8 font for tags, labels and callouts, a C64 sprite pointer, and one clean sans
(Inter Tight, OFL-1.1) for the headline and the sentence under it. Every coordinate is in the
layout's units (1920 x 1080 for `wide`) and scaled by `S` (0.5 for --fast). Change the look
here; make.py only says what happens when.
"""

import math
import re
from functools import lru_cache
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

import panel

ROOT = Path(__file__).resolve().parents[2]
S = 1.0  # set by make.py: 1.0, or 0.5 for --fast
FPS = 25

# The C64 palette in SIDOU's order (app/grid.h); make.py replaces it with the frames' own.
PAL = [
    (0, 0, 0), (255, 255, 255), (107, 52, 41), (115, 166, 181), (107, 60, 132), (90, 142, 66),
    (49, 40, 123), (189, 199, 107), (107, 77, 33), (66, 56, 0), (156, 101, 90), (66, 69, 66),
    (107, 109, 107), (156, 211, 132), (107, 93, 181), (148, 150, 148),
]
BLACK, WHITE, RED, CYAN, PURPLE, GREEN, BLUE, YELLOW = range(8)
ORANGE, BROWN, LRED, DGREY, GREY, LGREEN, LBLUE, LGREY = range(8, 16)

# Raster bars: the boot screen's cool bars and the logo's warm ones.
COOL = [BLUE, LBLUE, CYAN, WHITE, CYAN, LBLUE, BLUE]
WARM = [BROWN, RED, ORANGE, LRED, YELLOW, WHITE, YELLOW, LRED, ORANGE, RED, BROWN]

# The layouts: the same video re-laid-out, not cropped. `tag` (x, y, size), `headline` (x, y,
# width, size), `lcd` the big screen's box (x, y, side; a whole multiple of 240 reads best),
# `panel` its top left and scale (at least 40 % of the width: the controls read on a phone),
# `callout` the label's glyph size.
LAYOUTS = {
    "wide": {"size": (1920, 1080), "border": 36, "logo": (72, 62, 1.0), "version": (1848, 76),
             "tag": (72, 142, 24), "headline": (72, 178, 1010, 72), "lcd": (1128, 250, 720),
             "panel": (50, 352, 0.72), "callout": 32},
    "square": {"size": (1080, 1080), "border": 24, "logo": (40, 34, 0.6), "version": (1040, 40),
               "tag": (40, 80, 16), "headline": (40, 100, 1000, 36), "lcd": (300, 172, 480),
               "panel": (239, 668, 0.47), "callout": 24},
    "vertical": {"size": (1080, 1920), "border": 36, "logo": (90, 90, 1.0), "version": (990, 104),
                 "tag": (90, 190, 24), "headline": (90, 228, 900, 64), "lcd": (180, 420, 720),
                 "panel": (28, 1190, 0.8), "callout": 32},
}
L = LAYOUTS["wide"]


def set_format(name: str) -> None:
    global L
    L = LAYOUTS[name]


def c(i: int) -> tuple:
    return PAL[i]


def mix(a: tuple, b: tuple, t: float) -> tuple:
    t = min(max(t, 0.0), 1.0)
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def paper() -> tuple:
    return mix(c(BLACK), c(BLUE), 0.22)


def border() -> tuple:
    return c(LBLUE)


def u(x: float) -> int:
    """A layout unit in pixels."""
    return int(round(x * S))


def size() -> tuple:
    w, h = L["size"]
    return (u(w), u(h))


def ease(t: float) -> float:
    t = min(max(t, 0.0), 1.0)
    return t * t * (3 - 2 * t)


# ------------------------------------------------------------------------ fonts --

def _font_c() -> list:
    """app/font.c's glyphs: 8 bytes each (codes 0..0xDC), the top row first, bit 7 the left pixel."""
    text = (ROOT / "app/font.c").read_text()
    rows = re.findall(r"\{((?:0x[0-9a-fA-F]{2},?\s*){8})\}", text)
    return [[int(b, 16) for b in re.findall(r"0x[0-9a-fA-F]{2}", r)] for r in rows]


GLYPHS = _font_c()


@lru_cache(maxsize=1024)
def pixel_text(s: str, scale: int, fg: tuple, bg: tuple | None = None, advance: int = 8) -> Image.Image:
    """`s` in the device's 8x8 font at a whole `scale`, as an RGBA image."""
    s = s.upper()
    w = max(1, (len(s) - 1) * advance + 8) * scale
    im = Image.new("RGBA", (w, 8 * scale), bg + (255,) if bg else (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    for i, ch in enumerate(s):
        g = GLYPHS[ord(ch)] if ord(ch) < len(GLYPHS) else GLYPHS[ord("?")]
        for y, row in enumerate(g):
            for x in range(8):
                if row & (0x80 >> x):
                    x0 = (i * advance + x) * scale
                    d.rectangle((x0, y * scale, x0 + scale - 1, y * scale + scale - 1), fill=fg + (255,))
    return im


@lru_cache(maxsize=64)
def sans(size_px: int, weight: int) -> ImageFont.FreeTypeFont:
    f = ImageFont.truetype(str(ROOT / "assets/fonts/InterTight-Variable.ttf"), size_px)
    f.set_variation_by_axes([weight])
    return f


def pixel_scale(units: float) -> int:
    """The 8x8 font's whole scale for a glyph about `units` tall."""
    return max(1, int(units * S / 8 + 0.5))


def paste(im: Image.Image, src: Image.Image, xy: tuple) -> None:
    im.paste(src, (int(xy[0]), int(xy[1])), src if src.mode == "RGBA" else None)


# ------------------------------------------------------------------ rich text --

def runs(text: str) -> list:
    """`plain *marked* plain` (or `**marked**`) -> words, each a list of (text, marked) pieces:
    punctuation after a marked word stays on it, with no space."""
    words, word, piece, marked = [], [], "", False
    for ch in re.sub(r"\*\*", "*", text) + " ":
        if ch == "*" or ch.isspace():
            if piece:
                word.append((piece, marked))
                piece = ""
            if ch == "*":
                marked = not marked
            elif word:
                words.append(word)
                word = []
        else:
            piece += ch
    return words


def plain(text: str) -> str:
    return re.sub(r"\*\*?", "", text)


def draw_rich(d: ImageDraw.ImageDraw, xy: tuple, text: str, size_px: float, colours: tuple,
              weights: tuple, width: float, max_lines: int, leading: float = 1.18,
              centre: bool = False) -> int:
    """Wraps `text` into `width`, marked words in colours[1] and weights[1]; returns the y
    (pixels) under the last line."""
    fonts = (sans(u(size_px), weights[0]), sans(u(size_px), weights[1]))
    space = fonts[0].getlength(" ")
    x0, y = u(xy[0]), u(xy[1])
    lines, line, lw = [], [], 0.0
    for word in runs(text):
        wl = sum(fonts[m].getlength(t) for t, m in word)
        if line and lw + space + wl > u(width):
            lines.append((line, lw))
            line, lw = [], 0.0
        line.append((word, wl))
        lw += (space if len(line) > 1 else 0) + wl
    if line:
        lines.append((line, lw))
    for ln, lw in lines[:max_lines]:
        x = x0 + (u(width) - lw) / 2 if centre else x0
        for word, wl in ln:
            for t, m in word:
                d.text((x, y), t, font=fonts[m], fill=colours[m])
                x += fonts[m].getlength(t)
            x += space
        y += int(u(size_px) * leading)
    return y


# ---------------------------------------------------------------- the panel ---

def _p(x: float, y: float) -> tuple:
    """A panel point in frame pixels."""
    px, py, k = L["panel"]
    return (u(px + x * k), u(py + y * k))


def panel_rect(box: tuple) -> tuple:
    x, y, w, h = box
    a, b = _p(x, y), _p(x + w, y + h)
    return (a[0], a[1], b[0], b[1])


@lru_cache(maxsize=4)
def panel_base(key: tuple) -> Image.Image:
    """The panel without the screen's picture, drawn at 3x and filtered down (smooth edges),
    labelled in the device's font."""
    px0, py0, k = L["panel"]
    w, h = u(panel.W * k), u(panel.H * k)
    f = 3 * w / panel.W
    im = Image.new("RGBA", (w * 3, h * 3), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)

    def r(x, y, ww, hh):
        return (x * f, y * f, (x + ww) * f, (y + hh) * f)

    d.rounded_rectangle(r(14, 14, panel.W - 28, panel.H - 28), 30 * f, fill=(74, 77, 84))
    d.rounded_rectangle(r(*panel.BEZEL), 22 * f, fill=(14, 14, 16))
    d.rounded_rectangle(r(50, 388, 260, 64), 14 * f, fill=(43, 44, 47))
    d.rounded_rectangle(r(896, 222, 360, 160), 16 * f, fill=(43, 44, 47))
    d.rounded_rectangle(r(34, 562, 1212, 236), 26 * f, fill=(39, 39, 43))
    for name, (x, y, rad) in panel.KNOBS.items():
        d.ellipse(((x - rad) * f, (y - rad) * f, (x + rad) * f, (y + rad) * f), fill=(52, 54, 59))
        d.ellipse(((x - rad * 0.72) * f, (y - rad * 0.72) * f, (x + rad * 0.72) * f,
                   (y + rad * 0.72) * f), fill=(84, 88, 95))
    for name, (x, y, ww, hh) in panel.BUTTONS.items():
        d.rounded_rectangle(r(x, y, ww, hh), 9 * f, fill=(64, 66, 72))
    for n, (x, y, ww, hh, black) in panel.KEYS.items():
        d.rounded_rectangle(r(x, y, ww, hh), ww / 2 * f, fill=(79, 63, 66) if black else (94, 76, 79))
    im = im.resize((w, h), Image.LANCZOS)
    o = (u(px0), u(py0))
    sc = pixel_scale(8 * k / 0.6)
    small = k < 0.5
    for name, (x, y, rad) in panel.KNOBS.items():
        t = pixel_text(name if not small else name[:3], sc, (201, 198, 189))
        px, py = _p(x, y - rad - 20)
        im.alpha_composite(t, (max(0, px - o[0] - t.width // 2), py - o[1]))
    for name, (x, y, ww, hh) in panel.BUTTONS.items():
        t = pixel_text(name if not small else name[:3], sc, (170, 168, 160), advance=7)
        px, py = _p(x + ww / 2, y + hh / 2)
        im.alpha_composite(t, (px - o[0] - t.width // 2, py - o[1] - t.height // 2))
    return im


# The C64 sprite pointer, 12 x 21 (a hand-drawn arrow: 1 outline, 2 fill).
POINTER = [
    "1...........", "11..........", "121.........", "1221........", "12221.......",
    "122221......", "1222221.....", "12222221....", "122222221...", "1222222221..",
    "12222222221.", "122222211111", "1222122221..", "122112221...", "12.1.12221..",
    "1...1.12221.", "......12221.", ".......1221.", ".......111..", "............",
    "............",
]


@lru_cache(maxsize=8)
def pointer_sprite(scale: int) -> Image.Image:
    im = Image.new("RGBA", (12 * scale, 21 * scale), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    for y, row in enumerate(POINTER):
        for x, ch in enumerate(row):
            if ch != ".":
                col = c(BLACK) if ch == "1" else c(WHITE)
                d.rectangle((x * scale, y * scale, x * scale + scale - 1, y * scale + scale - 1), fill=col + (255,))
    return im


def control_point(name: str) -> tuple | None:
    """Where the pointer's tip goes for a control: on it, below the centre and to the right."""
    box = panel.control_box(name)
    if box is None:
        return None
    x, y, w, h = box
    return _p(x + w * 0.6, y + h * 0.62)


def pointer(im: Image.Image, at: tuple, bob: float) -> None:
    """The C64 sprite pointer, large, with a drop shadow."""
    sc = max(1, pixel_scale(8 * L["panel"][2] / 0.74 * 3.2))
    sp = pointer_sprite(sc)
    sh = Image.new("RGBA", sp.size, (0, 0, 0, 0))
    sh.paste((0, 0, 0, 140), (0, 0), sp.getchannel("A"))
    paste(im, sh, (at[0] + u(6), at[1] + bob + u(8)))
    paste(im, sp, (at[0], at[1] + bob))


# --------------------------------------------------------------- the frame ---

def frame_base(tag: str, headline: str, version: str, logo: Image.Image) -> Image.Image:
    """Everything that stays for a step: the border, the paper, the logo, the tag and the
    headline, the panel and the screen's frame. No sentences: the picture and the music carry
    it (the captions are the .srt's)."""
    im = Image.new("RGB", size(), border())
    d = ImageDraw.Draw(im)
    b = L["border"]
    W, H = L["size"]
    d.rectangle((u(b), u(b), u(W - b) - 1, u(H - b) - 1), fill=paper())
    lx, ly, lk = L["logo"]
    lg = logo.resize((u(logo.width * lk), u(logo.height * lk)), Image.NEAREST)
    paste(im, lg, (u(lx), u(ly)))
    v = pixel_text(version, pixel_scale(16 * lk), c(GREY))
    paste(im, v, (u(L["version"][0]) - v.width, u(L["version"][1])))
    tx, ty, th = L["tag"]
    paste(im, pixel_text(tag, pixel_scale(th), c(CYAN)), (u(tx), u(ty)))
    x, y, w, hs = L["headline"]
    draw_rich(d, (x, y), headline, hs, (c(WHITE), c(YELLOW)), (800, 800), w, 2, 1.04)
    base = panel_base((L["panel"], S))
    paste(im, base, (u(L["panel"][0]), u(L["panel"][1])))
    lx, ly, ls = L["lcd"]
    d.rounded_rectangle((u(lx - 18), u(ly - 18), u(lx + ls + 18), u(ly + ls + 18)), u(22), fill=(14, 14, 16))
    return im


def panel_box() -> tuple:
    """The panel's rectangle in frame pixels."""
    px, py, k = L["panel"]
    return (u(px), u(py), u(px + panel.W * k), u(py + panel.H * k))


def dim_panel(im: Image.Image, amount: float, keep: list) -> None:
    """Dims the panel by `amount` (0.5: half), all but the rectangles in `keep`."""
    if amount <= 0:
        return
    box = panel_box()
    src = im.crop(box)
    mask = panel_base((L["panel"], S)).getchannel("A").crop((0, 0, src.width, src.height))
    im.paste(Image.blend(src, Image.new("RGB", src.size, (0, 0, 0)), amount), box[:2], mask)
    for r in keep:
        g = u(10)
        rr = (max(box[0], r[0] - g), max(box[1], r[1] - g), min(box[2], r[2] + g), min(box[3], r[3] + g))
        if rr[2] > rr[0] and rr[3] > rr[1]:
            im.paste(src.crop((rr[0] - box[0], rr[1] - box[1], rr[2] - box[0], rr[3] - box[1])), rr[:2])


def beat_glow(im: Image.Image, strength: float) -> None:
    """The big screen's frame breathing with the music: a ring that brightens on the beat."""
    if strength <= 0:
        return
    d = ImageDraw.Draw(im)
    lx, ly, ls = L["lcd"]
    col = mix((14, 14, 16), c(PURPLE), strength)
    for i in range(u(5)):
        g = u(12) + i
        d.rounded_rectangle((u(lx) - g, u(ly) - g, u(lx + ls) + g, u(ly + ls) + g), u(16), outline=col)


def lcd_view(lcd: Image.Image, zoom: float, centre: tuple) -> Image.Image:
    """The 240 x 240 frame in the big screen's box, pushed in by `zoom` round `centre` (pixels
    of the 240 x 240 frame)."""
    side = u(L["lcd"][2])
    if zoom <= 1.0001:
        return lcd.resize((side, side), Image.NEAREST)
    span = 240 / zoom
    cx = min(max(centre[0], span / 2), 240 - span / 2)
    cy = min(max(centre[1], span / 2), 240 - span / 2)
    return lcd.resize((side, side), Image.NEAREST, box=(cx - span / 2, cy - span / 2, cx + span / 2, cy + span / 2))


def lcd_to_frame(px: float, py: float, zoom: float, centre: tuple) -> tuple:
    """A point of the 240 x 240 frame in the big screen's box, as lcd_view shows it."""
    span = 240 / max(zoom, 1.0)
    cx = min(max(centre[0], span / 2), 240 - span / 2) if zoom > 1.0001 else 120
    cy = min(max(centre[1], span / 2), 240 - span / 2) if zoom > 1.0001 else 120
    lx, ly, ls = L["lcd"]
    k = ls / span
    return (u(lx + (px - cx + span / 2) * k), u(ly + (py - cy + span / 2) * k))


def put_lcd(im: Image.Image, lcd: Image.Image, zoom: float, centre: tuple) -> None:
    im.paste(lcd_view(lcd, zoom, centre), (u(L["lcd"][0]), u(L["lcd"][1])))
    x, y, w, h = panel.LCD
    a, b = _p(x, y), _p(x + w, y + h)
    im.paste(lcd.resize((b[0] - a[0], b[1] - a[1]), Image.NEAREST), a)


def region_box(d: ImageDraw.ImageDraw, rect: tuple, alpha: float) -> None:
    """The highlight round the screen region that changes."""
    if alpha <= 0:
        return
    col = mix(paper(), c(YELLOW), alpha)
    x0, y0, x1, y1 = rect
    for i in range(max(1, u(4))):
        d.rectangle((x0 - i - u(3), y0 - i - u(3), x1 + i + u(3), y1 + i + u(3)), outline=col)


def ring(d: ImageDraw.ImageDraw, rect: tuple, colour: tuple, grow: float = 0.0, width: float = 7,
         round_: bool = False) -> None:
    x0, y0, x1, y1 = rect
    g = u(8 + grow)
    if round_:
        d.ellipse((x0 - g, y0 - g, x1 + g, y1 + g), outline=colour, width=max(1, u(width)))
    else:
        d.rounded_rectangle((x0 - g, y0 - g, x1 + g, y1 + g), u(12 + grow), outline=colour, width=max(1, u(width)))


def glow(im: Image.Image, rect: tuple, colour: tuple, strength: float, round_: bool) -> None:
    """A soft outer glow round a control: rings fading outwards."""
    if strength <= 0:
        return
    pad = u(40)
    x0, y0, x1, y1 = rect
    ov = Image.new("RGBA", (x1 - x0 + 2 * pad, y1 - y0 + 2 * pad), (0, 0, 0, 0))
    d = ImageDraw.Draw(ov)
    for i in range(8):
        g = u(10 + i * 3.5)
        a = int(110 * strength * (1 - i / 8) ** 2)
        box = (pad - g, pad - g, pad + x1 - x0 + g, pad + y1 - y0 + g)
        if round_:
            d.ellipse(box, outline=colour + (a,), width=max(1, u(4)))
        else:
            d.rounded_rectangle(box, u(14 + i * 3), outline=colour + (a,), width=max(1, u(4)))
    base = im.crop((x0 - pad, y0 - pad, x1 + pad, y1 + pad)).convert("RGBA")
    im.paste(Image.alpha_composite(base, ov).convert("RGB"), (x0 - pad, y0 - pad))


def highlight_rect(name: str) -> tuple | None:
    box = panel.control_box(name)
    return panel_rect(box) if box else None


def control_highlight(im: Image.Image, name: str, state: dict) -> None:
    """The control in use, unmistakable: a glow and a thick ring, and its cue: a press sinks
    and bursts, a turn has an arrow round the knob going its way and the pointer of the knob
    turning, a hold fills a ring like a timer. `state`: `active` (0..1), `kind` (press, hold,
    turn, key), `phase` (0..1 through the act), `detents`, a knob's `angle`."""
    rect = highlight_rect(name)
    if rect is None:
        return
    a = state.get("active", 0.0)
    kind = state.get("kind")
    knob = name in panel.KNOBS
    hot = c(YELLOW)
    glow(im, rect, hot, 0.55 + 0.45 * a, knob)
    d = ImageDraw.Draw(im)
    k = L["panel"][2] / 0.74
    if kind == "key":
        fill = mix((94, 76, 79), hot, 0.35 + 0.65 * a)
        off = u(3) if a > 0.5 else 0
        d.rounded_rectangle((rect[0], rect[1] + off, rect[2], rect[3] + off), (rect[2] - rect[0]) // 2, fill=fill)
        ring(d, rect, hot, width=6 * k, grow=-2)
        return
    if knob:
        x, y, rad = panel.KNOBS[name]
        cx, cy = _p(x, y)
        rr = u(rad * L["panel"][2])
        ang = math.radians(state.get("angle", 0.0) - 90)
        d.line((cx, cy, cx + math.cos(ang) * rr * 0.85, cy + math.sin(ang) * rr * 0.85),
               fill=c(WHITE), width=max(2, u(6 * k)))
        ring(d, rect, hot, width=7 * k, round_=True)
        if kind == "turn" and a > 0:
            sign = 1 if state.get("detents", 1) >= 0 else -1
            start = -90 + sign * 180 * state.get("phase", 0)
            g = u(26 * k)
            arc = (cx - rr - g, cy - rr - g, cx + rr + g, cy + rr + g)
            a0, a1 = (start - 110, start) if sign > 0 else (start, start + 110)
            col = mix(paper(), c(LGREEN), a)
            d.arc(arc, a0, a1, fill=col, width=max(2, u(8 * k)))
            tip = math.radians(a1 if sign > 0 else a0)
            r2 = (arc[2] - arc[0]) / 2
            tx, ty = cx + math.cos(tip) * r2, cy + math.sin(tip) * r2
            dx, dy = -math.sin(tip) * sign, math.cos(tip) * sign
            s_ = u(20 * k)
            d.polygon([(tx + dx * s_, ty + dy * s_), (tx - dy * s_ * 0.6, ty + dx * s_ * 0.6),
                       (tx + dy * s_ * 0.6, ty - dx * s_ * 0.6)], fill=col)
        return
    if kind == "hold" and a > 0:
        off = u(3)
        d.rounded_rectangle((rect[0], rect[1] + off, rect[2], rect[3] + off), u(8 * k), fill=mix((64, 66, 72), c(LRED), 0.6))
        cx, cy = (rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2
        rr = max(rect[2] - rect[0], rect[3] - rect[1]) / 2 + u(16 * k)
        box = (cx - rr, cy - rr, cx + rr, cy + rr)
        d.ellipse(box, outline=mix(paper(), c(DGREY), 1.0), width=max(2, u(8 * k)))
        d.arc(box, -90, -90 + 360 * state.get("phase", 0.0), fill=c(LRED), width=max(2, u(8 * k)))
        return
    if kind == "press" and a > 0:
        off = u(4) if a > 0.6 else 0
        d.rounded_rectangle((rect[0], rect[1] + off, rect[2], rect[3] + off), u(8 * k), fill=mix((40, 42, 46), hot, 0.75 * a))
        ring(d, rect, mix(paper(), c(WHITE), a), grow=26 * (1 - a) * k, width=4 * k)
    ring(d, rect, hot, width=7 * k)


def callout_near(im: Image.Image, text: str, rect: tuple | None, alpha: float = 1.0) -> None:
    """The plain-words label next to the control, big, black on yellow, with a tick pointing at
    it: above it when there is room, else below. Without a control, under the big screen."""
    if not text or alpha <= 0:
        return
    sc = pixel_scale(L["callout"])
    col = c(YELLOW)
    t = pixel_text(text, sc, c(BLACK), col)
    pad = max(2, u(L["callout"] * 0.4))
    W, H = size()
    b = u(L["border"]) + u(8)
    w, h = t.width + 2 * pad, t.height + 2 * pad
    if rect is None:
        lx, ly, ls = L["lcd"]
        x = u(lx + ls / 2) - w // 2
        y = u(ly + ls + 34)
        tick = None
    else:
        cx = (rect[0] + rect[2]) // 2
        gap = u(30)
        x = min(max(b, cx - w // 2), W - b - w)
        lx, ly, ls = L["lcd"]
        screen = (u(lx - 18), u(ly - 18), u(lx + ls + 18), u(ly + ls + 18))

        def clear(yy):
            over = x < screen[2] and x + w > screen[0] and yy < screen[3] and yy + h > screen[1]
            return yy >= b and yy + h <= H - b and not over

        # Beside the screen, never over it: slide left (or right) of it when it would cover it.
        if cx < screen[0] and x + w > screen[0] - u(16):
            x = max(b, screen[0] - u(16) - w)
        elif cx > screen[2] and x < screen[2] + u(16):
            x = min(W - b - w, screen[2] + u(16))
        y, up = rect[1] - gap - h, True
        if not clear(y) and clear(rect[3] + gap):
            y, up = rect[3] + gap, False
        tick = (cx, up)
    d = ImageDraw.Draw(im)
    d.rectangle((x + u(5), y + u(5), x + w + u(5), y + h + u(5)), fill=(0, 0, 0))
    d.rectangle((x, y, x + w, y + h), fill=col)
    if tick:
        cx, up = tick
        s_ = u(12)
        cx = min(max(cx, x + s_), x + w - s_)
        if up:
            d.polygon([(cx - s_, y + h), (cx + s_, y + h), (cx, y + h + s_)], fill=col)
        else:
            d.polygon([(cx - s_, y), (cx + s_, y), (cx, y - s_)], fill=col)
    paste(im, t, (x + pad, y + pad))


def raster_border(im: Image.Image, phase: float, colours: list, alpha: float = 1.0) -> None:
    """Raster bars across the border (the paper untouched): `phase` moves them down."""
    if alpha <= 0:
        return
    d = ImageDraw.Draw(im)
    W, H = size()
    bar = max(1, u(12))
    n = len(colours)
    off = int(phase * n * bar * 4)
    b = u(L["border"])
    for y in range(0, H, bar):
        col = mix(border(), c(colours[((y + off) // bar) % n]), alpha)
        if y < b or y + bar > H - b:
            d.rectangle((0, y, W, min(H, y + bar) - 1), fill=col)
        else:
            d.rectangle((0, y, b - 1, y + bar - 1), fill=col)
            d.rectangle((W - b, y, W, y + bar - 1), fill=col)


def wipe_alpha(dt: float) -> float:
    """The step-change raster's strength `dt` seconds after the change (0.45 s)."""
    return max(0.0, 1.0 - dt / 0.45) if dt >= 0 else 0.0


def sparkle(im: Image.Image, at: tuple, dt: float) -> None:
    """A little sprite flourish where something landed: four stars flying out (0.6 s)."""
    if not 0 <= dt < 0.6:
        return
    k = dt / 0.6
    star = pixel_text("*", pixel_scale(16), mix(c(WHITE), c(LGREEN), k))
    for i in range(4):
        ang = math.radians(45 + 90 * i)
        r = u(10 + 46 * k)
        paste(im, star, (at[0] + math.cos(ang) * r - star.width // 2, at[1] + math.sin(ang) * r - star.height // 2))


def card(tag: str, headline: str, line: str, diagram: str, t: float, logo: Image.Image,
         version: str, lcd: Image.Image | None = None) -> Image.Image:
    """A full-paper card (no device): the tag, a big headline, at most one short line, and an
    optional diagram `A <> B <> C`: boxes with a dot travelling the arrows both ways; with
    `lcd`, the screen as it is now stands over the first box."""
    im = Image.new("RGB", size(), border())
    d = ImageDraw.Draw(im)
    b = L["border"]
    W, H = L["size"]
    d.rectangle((u(b), u(b), u(W - b) - 1, u(H - b) - 1), fill=paper())
    lx, ly, lk = L["logo"]
    lg = logo.resize((u(logo.width * lk), u(logo.height * lk)), Image.NEAREST)
    paste(im, lg, (u(lx), u(ly)))
    v = pixel_text(version, pixel_scale(16 * lk), c(GREY))
    paste(im, v, (u(L["version"][0]) - v.width, u(L["version"][1])))
    m = W * 0.08
    hs = L["headline"][3] * 1.2
    tg = pixel_text(tag, pixel_scale(L["tag"][2]), c(CYAN))
    top = H * (0.16 if diagram else 0.34)
    paste(im, tg, (u(W / 2) - tg.width // 2, u(top)))
    y = draw_rich(d, (m, top + L["tag"][2] * 1.8), headline, hs, (c(WHITE), c(YELLOW)), (800, 800), W - 2 * m, 2, 1.04, True)
    if line:
        y = draw_rich(d, (m, y / S + hs * 0.25), line, hs * 0.42, (c(LGREY), c(WHITE)), (400, 700), W - 2 * m, 2, 1.3, True)
    if diagram:
        boxes = [x.strip() for x in diagram.split("<>")]
        sc = pixel_scale(L["tag"][2])
        cols = [LBLUE, LGREEN, YELLOW]
        imgs = [pixel_text(x, sc, c(BLACK), c(cols[i % 3])) for i, x in enumerate(boxes)]
        pad = u(14)
        gap = u(W * 0.06)
        total = sum(i.width + 2 * pad for i in imgs) + gap * (len(imgs) - 1)
        x = (u(W) - total) // 2
        yy = y + u(H * 0.05)
        if lcd is not None:
            side = u(min(W * 0.22, H * 0.3))
            paste(im, lcd.resize((side, side), Image.NEAREST), (x + (imgs[0].width + 2 * pad - side) // 2, yy))
            yy += side + u(24)
        spans = []
        for i, t_im in enumerate(imgs):
            d.rectangle((x, yy - pad, x + t_im.width + 2 * pad, yy + t_im.height + pad), fill=c(cols[i % 3]))
            paste(im, t_im, (x + pad, yy))
            spans.append((x, x + t_im.width + 2 * pad))
            x += t_im.width + 2 * pad + gap
        for i in range(len(spans) - 1):
            x0, x1 = spans[i][1] + u(10), spans[i + 1][0] - u(10)
            ym = yy + imgs[i].height // 2
            d.line((x0, ym, x1, ym), fill=c(GREY), width=max(1, u(4)))
            for tip, sgn in ((x1, 1), (x0, -1)):
                d.polygon([(tip, ym), (tip - sgn * u(12), ym - u(8)), (tip - sgn * u(12), ym + u(8))], fill=c(GREY))
            ph = (t * 0.7 + i * 0.5) % 2.0
            p = ph if ph < 1 else 2 - ph
            dx = x0 + (x1 - x0) * ease(p)
            dot = u(10)
            d.ellipse((dx - dot, ym - dot, dx + dot, ym + dot), fill=c(WHITE))
    return im


def title_card(t: float, title: str, logo2x: Image.Image, length: float) -> Image.Image:
    """The opening card: LOAD"TITLE",8,1 typed on the paper, then the loading stripes in the
    border and the logo (at most 2 s)."""
    im = Image.new("RGB", size(), border())
    d = ImageDraw.Draw(im)
    b = L["border"]
    W, H = L["size"]
    d.rectangle((u(b), u(b), u(W - b) - 1, u(H - b) - 1), fill=paper())
    sc = pixel_scale(32 if W > 1500 else 24)
    cmd = f'LOAD"{title.upper()}",8,1'
    cols = int((W - 2 * L["tag"][0]) * S / (8 * sc))
    typed = cmd[: int(len(cmd) * min(1.0, t / (length * 0.45)))]
    lines = ["READY."] + [typed[i:i + cols] for i in range(0, max(1, len(typed)), cols)]
    if t > length * 0.5:
        lines += ["", "LOADING"]
    y = u(H * 0.14)
    for ln in lines:
        if ln:
            paste(im, pixel_text(ln, sc, c(LBLUE)), (u(L["tag"][0]), y))
        y += 10 * sc
    if int(t * 4) % 2 == 0 and t <= length * 0.5:
        x = u(L["tag"][0]) + len(lines[-1]) * 8 * sc
        d.rectangle((x, y - 10 * sc, x + 8 * sc - 1, y - 2 * sc - 1), fill=c(LBLUE))
    if t > length * 0.5:
        raster_border(im, t * 3, WARM if int(t * 10) % 2 else COOL)
        k = L["logo"][2]
        lg = logo2x.resize((u(logo2x.width * k), u(logo2x.height * k)), Image.NEAREST)
        e = ease((t - length * 0.55) / (length * 0.3))
        if e > 0:
            paste(im, lg, (u(W / 2) - lg.width // 2, u(H * 0.62) + int((1 - e) * u(H * 0.2))))
    return im
