# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later

import argparse
import array
import hashlib
import json
import os
import re
import subprocess
import sys
import time
import wave
from functools import lru_cache
from multiprocessing import Pool
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import panel  # noqa: E402
import template as T  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
HOST = ROOT / "host"
RECIPES = ROOT / "docs/recipes"
OUT = ROOT / "build/videos"
FFMPEG = os.environ.get("FFMPEG", "/opt/homebrew/bin/ffmpeg" if Path("/opt/homebrew/bin/ffmpeg").exists() else "ffmpeg")
SOUNDTRACK = Path(__file__).resolve().parent / "soundtrack.txt"
VIDEOS = Path(__file__).resolve().parent / "videos.json"
FPS = 60
CAMERA_FPS = FPS
LCD_MS = 40
TEASER_S = 3.0
TITLE_S = 1.8
END_S = 1.5
RATE = 44118
CAMERA_MOVE_MS = 900
CAMERA_HOLD_MS = 1000
SAME_FRAMING = 0.6
POINTER_GLIDE_MS = 600
POINTER_EARLY_MS = 100
CALLOUT_GLIDE_MS = 600
CAPTION_FADE_MS = 200
HEADLINE_FADE_MS = 200
DIM_FADE_MS = 300
PANEL_DIM = 0.5
BEAT_GLOW = 0.4
XFADE_MS = 300
LUFS_TARGET = -16.0
LUFS_TOLERANCE = 1.0
TRUE_PEAK_MAX_DB = -1.0


def sha(*parts: bytes) -> str:
    h = hashlib.sha256()
    for p in parts:
        h.update(hashlib.sha256(p).digest())
    return h.hexdigest()


def fw_dir() -> Path:
    d = os.environ.get("SIDOU_FM1_BUILD")
    return Path(d) if d else ROOT / "build/fm1"


def recipe_chain(rid: str) -> list:
    out, seen = [], set()
    while rid and rid not in seen:
        seen.add(rid)
        v = json.loads((RECIPES / f"{rid}.json").read_text())
        out.append(v)
        rid = v["prerequisites"].get("after")
    return out


def capture(rid: str, source: Path, key_parts: list) -> Path:
    harness = sorted((HOST / "crates/ergo/src").glob("*.rs")) + [
        HOST / "crates/emu-gate/tests/common/mod.rs"]
    key = sha((fw_dir() / "platform/fm1/sidou_app.bin").read_bytes(),
              *[p.read_bytes() for p in harness], *key_parts)[:16]
    out = OUT / "cache" / f"{rid}-{key}"
    cap = out / "capture" / rid
    if (cap / "frames.json").exists():
        return cap
    env = dict(os.environ, SIDOU_FM1_BUILD=str(fw_dir()))
    cmd = [str(HOST / "target/release/ergo"), "--out", str(out), "--journeys", str(source),
           "--capture", "full", "--journey", rid]
    r = subprocess.run(cmd, cwd=HOST, env=env, capture_output=True, text=True)
    if r.returncode != 0 or not (cap / "frames.json").exists():
        sys.exit(f"make-videos: the capture of {rid} failed:\n{r.stdout}{r.stderr}")
    return cap


def song_source(name: str) -> Path:
    text_song = ROOT / f"assets/text-songs/{name}.json"
    return text_song if text_song.exists() else ROOT / f"assets/songs/{name}.sng"


def recipe_capture(rid: str) -> Path:
    chain = recipe_chain(rid)
    songs = sorted({s for v in chain for s in v["prerequisites"].get("songs", [])})
    parts = [json.dumps(v, sort_keys=True).encode() for v in chain]
    parts += [song_source(s).read_bytes() for s in songs]
    tunes = sorted({s for v in chain for s in v["prerequisites"].get("tunes", [])})
    parts += [(ROOT / f"assets/starter/{s}.sid").read_bytes() for s in tunes]
    art = sorted({s for v in chain for s in v["prerequisites"].get("art", [])})
    parts += [(ROOT / f"assets/art/{s}.sidart").read_bytes() for s in art]
    return capture(rid, RECIPES, parts)


def soundtrack_capture() -> Path:
    return capture("soundtrack", SOUNDTRACK,
                   [SOUNDTRACK.read_bytes(), (ROOT / "assets/songs/chrome-comet.sng").read_bytes()])


def read_wav(path: Path) -> array.array:
    with wave.open(str(path)) as w:
        a = array.array("h")
        a.frombytes(w.readframes(w.getnframes()))
    return a


def piece(src: array.array, start: int, n: int, gain: float = 1.0, fade: int = 220) -> array.array:
    out = array.array("h", bytes(4 * n))
    for i in range(n):
        j = start + i
        if 0 <= j < len(src) // 2:
            g = gain * min(1.0, (i + 1) / fade, (n - i) / fade)
            out[2 * i] = int(src[2 * j] * g)
            out[2 * i + 1] = int(src[2 * j + 1] * g)
    return out


def write_wav(path: Path, a: array.array) -> None:
    with wave.open(str(path), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(a.tobytes())


VERB = {"press": "PRESS", "hold": "HOLD", "turn": "TURN", "sweep": "TURN", "pot": "TURN"}


def events_of(steps: list) -> list:
    ev, down = [], {}
    for s in steps:
        for a in s["inputs"]:
            op, t0, t1 = a["op"], a["ms"], a["end_ms"]
            if op == "press":
                ev.append((a["button"], "press", t0, t1, 0))
            elif op == "hold":
                ev.append((a["button"], "hold", t0, t1, 0))
            elif op in ("turn", "sweep"):
                ev.append((a["encoder"], "turn", t0, max(t1, t0 + 1), a["detents"]))
            elif op == "hold_turn":
                ev.append((a["button"], "hold", t0, t1, 0))
                ev.append((a["encoder"], "turn", t0 + 30, max(t1 - 30, t0 + 31), a["detents"]))
            elif op == "hold2_turn":
                ev.append((a["button"], "hold", t0, t1, 0))
                ev.append((a["button2"], "hold", t0 + 30, t1, 0))
                ev.append((a["encoder"], "turn", t0 + 210, max(t1 - 30, t0 + 61), a["detents"]))
            elif op == "hold_key":
                ev.append((a["button"], "hold", t0, t1, 0))
                ev.append((a["note"], "key", t0 + 30, t0 + 180, 0))
            elif op == "hold_press":
                ev.append((a["button"], "hold", t0, t1, 0))
                ev.append((a["press"], "press", t0 + 30, max(t1 - 30, t0 + 31), 0))
            elif op == "note":
                for n in a["notes"]:
                    ev.append((n, "key", t0, t0 + a["ms"], 0))
            elif op == "down":
                down[a["note"]] = t0
            elif op == "up":
                if a["note"] in down:
                    ev.append((a["note"], "key", down.pop(a["note"]), t0, 0))
            elif op == "taps":
                t = t0
                for n in a["notes"]:
                    ev.append((n, "key", t, t + a["ms"], 0))
                    t += a["ms"] + 60
            elif op == "play":
                six = 15000 / a["bpm"]
                t = float(t0)
                for e in a["events"]:
                    end = t + six * e["sixteenths"]
                    off = max(end - min(six * 0.25, 40), t + 10)
                    for n in e["notes"]:
                        ev.append((n, "key", round(t), round(off), 0))
                    t = end
            elif op == "select_item":
                ev.append(("SELECT", "turn", t0, max(t1, t0 + 1), 0))
            elif op == "pot":
                ev.append(("MASTER", "turn", t0, max(t1, t0 + 1), 0))
    end = steps[-1]["end_ms"] if steps else 0
    for n, t0 in down.items():
        ev.append((n, "key", t0, end, 0))
    return ev


def default_callout(step: dict) -> str:
    acts = [a for a in step["inputs"] if a["op"] not in ("wait", "sync", "up", "boot")]
    if not acts:
        return ""
    a = acts[0]
    op = a["op"]
    if op in ("press", "hold"):
        return f"{VERB[op]} {a['button']}"
    if op in ("turn", "sweep"):
        return f"TURN {a['encoder']}"
    if op == "hold_turn":
        return f"HOLD {a['button']} + TURN {a['encoder']}"
    if op == "hold2_turn":
        return f"HOLD {a['button']} + {a['button2']} + TURN {a['encoder']}"
    if op == "hold_key":
        return f"HOLD {a['button']} + KEY {panel.note_number(a['note']) - 52}"
    if op == "hold_press":
        return f"HOLD {a['button']} + {a['press']}"
    if op == "taps":
        return "TAP THE STEP KEYS"
    if op in ("note", "down", "play"):
        return "PLAY THE KEYS"
    if op == "select_item":
        return f"SELECT {a['text']}"
    if op == "pot":
        return "TURN MASTER"
    if op in ("midi", "clock"):
        return "MIDI IN"
    return ""


def plan_of(rid: str, cap: Path, sound: Path) -> dict:
    side = json.loads((cap / "frames.json").read_text())
    v = json.loads((RECIPES / f"{rid}.json").read_text())
    n = len(side["frames"])
    steps = side["steps"]
    for i, s in enumerate(steps):
        f = s.get("focus", {})
        s["callout_text"] = f.get("callout") or default_callout(s)
        s["region"] = None
        if "screen" in f:
            r = f["screen"]
            s["region"] = [r["col"] * 8, r["row"] * 8, (r["col"] + r["cols"]) * 8, (r["row"] + r["rows"]) * 8]
        acts = [a for a in s["inputs"] if a["op"] not in ("wait", "sync")]
        s["acts_end"] = max([a["end_ms"] for a in acts], default=s["ms"])
        s["next_ms"] = steps[i + 1]["ms"] if i + 1 < len(steps) else n * LCD_MS
    cfg = json.loads(VIDEOS.read_text()).get(rid, {})
    plays = [a["end_ms"] for s in steps for a in s["inputs"] if a["op"] == "press" and a.get("button") == "PLAY"]
    anchor = plays[0] if plays else None
    busiest = max(steps, key=lambda s: len([a for a in s["inputs"] if a["op"] != "wait"])) if steps else None
    p0 = busiest["ms"] if busiest else max(0, n * LCD_MS - 4000)
    if cfg.get("preview_step"):
        p0 = next(x for x in steps if x["id"] == cfg["preview_step"])["ms"]
    events = events_of(steps)
    return {
        "id": rid, "title": v["title"], "goal": v["goal"], "frames": n, "main_ms": n * LCD_MS,
        "cap": str(cap), "sound": str(sound), "steps": steps, "events": events, "palette": palette(cap),
        "version": version(), "cfg": cfg, "bpm": cfg.get("bpm"), "beat_anchor": anchor,
        "preview": [p0, min(n * LCD_MS, p0 + 4000)],
        "camera": camera_moves(steps), "pointer": pointer_glides(steps, events),
    }


def palette(cap: Path) -> list:
    p = Image.open(cap / "frames/00001.png").getpalette()[:48]
    return [tuple(p[i:i + 3]) for i in range(0, 48, 3)]


def version() -> str:
    if os.environ.get("SIDOU_VIDEOS_VERSION"):
        return f"SIDOU {os.environ['SIDOU_VIDEOS_VERSION']} - PLAYED ON SIDOU"
    m = re.search(r"project\(sidou VERSION ([0-9.]+)", (ROOT / "CMakeLists.txt").read_text())
    return f"SIDOU {m.group(1) if m else '?'} - PLAYED ON SIDOU"


P: dict = {}


def init(plan: dict, scale: float, fmt: str) -> None:
    P.clear()
    P.update(plan)
    T.S = scale
    T.set_format(fmt)
    T.PAL = [tuple(x) for x in plan["palette"]]
    base.cache_clear()
    P["callouts"] = [callout_place(s) for s in plan["steps"]]


@lru_cache(maxsize=8)
def logo(two: bool) -> Image.Image:
    name = "metal-chrome-lettering-2x.png" if two else "metal-chrome-lettering.png"
    return Image.open(ROOT / "assets/logo" / name).convert("RGBA")


@lru_cache(maxsize=4)
def lcd(i: int) -> Image.Image:
    return Image.open(Path(P["cap"]) / f"frames/{i + 1:05}.png").convert("RGB")


def lcd_at(ms: float) -> Image.Image:
    return lcd(min(P["frames"] - 1, max(0, int(ms // LCD_MS))))


@lru_cache(maxsize=64)
def base(tag: str, headline: str) -> Image.Image:
    return T.frame_base(tag, headline, P["version"], logo(False))


def step_index(steps: list, ms: float) -> int:
    i = 0
    for k, s in enumerate(steps):
        if s["ms"] <= ms:
            i = k
    return i


def step_at(ms: float) -> int:
    return step_index(P["steps"], ms)


def is_card(s: dict) -> bool:
    return s.get("focus", {}).get("control") == "card"


def mode_of(s: dict) -> str | None:
    return s.get("focus", {}).get("control")


def shows_panel(s: dict) -> bool:
    return mode_of(s) not in ("screen", "device", "card")


FULL = (1.0, (120.0, 120.0), None, 0.0)


def framing(region: list | None) -> tuple:
    if not region:
        return FULL
    side = max(region[2] - region[0], region[3] - region[1])
    zoom = T.pixel_zoom(max(1.0, min(1.6, 0.9 * 240 / side)))
    return (zoom, ((region[0] + region[2]) / 2, (region[1] + region[3]) / 2), tuple(region), 1.0)


def overlap(a: tuple, b: tuple) -> float:
    w = min(a[2], b[2]) - max(a[0], b[0])
    h = min(a[3], b[3]) - max(a[1], b[1])
    if w <= 0 or h <= 0:
        return 0.0
    area = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - w * h
    return w * h / area


def same_framing(a: tuple, b: tuple) -> bool:
    if a[2] is None or b[2] is None:
        return a[2] is b[2]
    return a[0] == b[0] and overlap(a[2], b[2]) >= SAME_FRAMING


def camera_moves(steps: list) -> list:
    moves, cur, free_at = [], FULL, float("-inf")
    for s in steps:
        target = framing(s["region"])
        if same_framing(target, cur):
            continue
        start = max(s["ms"], free_at)
        if start + CAMERA_MOVE_MS > s["next_ms"]:
            continue
        moves.append((start, cur, target))
        cur = target
        free_at = start + CAMERA_MOVE_MS + CAMERA_HOLD_MS
    return moves


def lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


def between(a: tuple, b: tuple, t: float) -> tuple:
    zoom = lerp(a[0], b[0], t)
    centre = (lerp(a[1][0], b[1][0], t), lerp(a[1][1], b[1][1], t))
    if a[2] is None:
        return (zoom, centre, b[2], b[3] * t)
    if b[2] is None:
        return (zoom, centre, a[2], a[3] * (1 - t))
    return (zoom, centre, tuple(lerp(x, y, t) for x, y in zip(a[2], b[2])), 1.0)


def camera_state(moves: list, ms: float) -> tuple:
    state = FULL
    for start, a, b in moves:
        if start > ms:
            break
        state = between(a, b, T.glide((ms - start) / CAMERA_MOVE_MS))
    return state


def camera_at(ms: float) -> tuple:
    return camera_state(P["camera"], ms)


def angle_of(name: str, ms: float) -> float:
    deg = 0.0
    for c, kind, t0, t1, det in P["events"]:
        if c == name and kind == "turn" and t0 <= ms:
            deg += 18 * det * min(1.0, (ms - t0) / max(1, t1 - t0))
    return deg


def panel_states(ms: float, k: int) -> dict:
    s = P["steps"][k]
    out = {}
    lo, hi = s["ms"], s["next_ms"]
    f = primary(s, P["events"])
    if f:
        out[f] = {"active": 0.0}
    for c, kind, t0, t1, det in P["events"]:
        if t1 < lo or t0 >= hi:
            continue
        if c not in out and not (t0 - 400 <= ms <= max(t1, t0 + 400) + 1200):
            continue
        st = out.setdefault(c, {"active": 0.0})
        if kind == "key":
            st["kind"] = "key"
            if t0 <= ms < max(t1, t0 + 120):
                st["active"] = 1.0
            continue
        st["kind"] = kind
        if t0 <= ms < max(t1, t0 + 400):
            st["active"] = max(st["active"], 1.0 - max(0.0, ms - t1) / 400)
            st["phase"] = min(1.0, (ms - t0) / max(1, t1 - t0))
            st["detents"] = det
    for c, st in out.items():
        if c in panel.KNOBS:
            st["angle"] = angle_of(c, ms)
    return out


def pointer_glides(steps: list, events: list) -> list:
    wanted = []
    for s in steps:
        if not shows_panel(s):
            continue
        first = primary(s, events)
        if first:
            wanted.append((s["ms"], first))
        for c, kind, t0, t1, det in events:
            if kind != "key" and s["ms"] <= t0 < s["next_ms"]:
                wanted.append((max(s["ms"], t0 - POINTER_GLIDE_MS - POINTER_EARLY_MS), c))
    glides, cur, free_at = [], None, float("-inf")
    for t, c in sorted(wanted, key=lambda x: x[0]):
        if c == cur:
            continue
        start = max(t, free_at)
        glides.append((start, cur, c))
        cur, free_at = c, start + POINTER_GLIDE_MS
    return glides


def pointer_at(ms: float) -> tuple | None:
    here = None
    for start, a, b in P["pointer"]:
        if start > ms:
            break
        to = T.control_point(b)
        fro = T.control_point(a) if a else None
        if to is None:
            continue
        if fro is None:
            here = to
            continue
        e = T.glide((ms - start) / POINTER_GLIDE_MS)
        here = (lerp(fro[0], to[0], e), lerp(fro[1], to[1], e))
    return here


def beat(ms: float) -> float:
    bpm, anchor = P.get("bpm"), P.get("beat_anchor")
    if not bpm or anchor is None or ms < anchor:
        return 0.0
    frac = ((ms - anchor) * bpm / 60000) % 1.0
    return BEAT_GLOW * (1 - frac) ** 3


def primary(s: dict, events: list) -> str | None:
    f = mode_of(s)
    if f and f not in ("device", "screen", "card"):
        return f
    for c, kind, t0, t1, det in events:
        if s["ms"] <= t0 < s["next_ms"]:
            return c
    return None


def callout_place(s: dict) -> tuple | None:
    if is_card(s) or not s["callout_text"]:
        return None
    main = primary(s, P["events"]) if shows_panel(s) else None
    return T.callout_layout(s["callout_text"], T.highlight_rect(main) if main else None)


def callout_at(ms: float) -> tuple | None:
    k = step_at(ms)
    to = P["callouts"][k]
    fro = P["callouts"][k - 1] if k else None
    if to is None or fro is None:
        return to
    e = T.glide((ms - P["steps"][k]["ms"]) / CALLOUT_GLIDE_MS)
    tick = None
    if to[2]:
        tick = (lerp(fro[2][0], to[2][0], e) if fro[2] else to[2][0], to[2][1])
    return (lerp(fro[0], to[0], e), lerp(fro[1], to[1], e), tick)


def caption_at(steps: list, ms: float) -> tuple | None:
    k = step_index(steps, ms)
    s = steps[k]
    dt = ms - s["ms"]
    if k and dt < CAPTION_FADE_MS:
        prev = steps[k - 1]
        return None if is_card(prev) else (prev.get("caption") or "", 1 - dt / CAPTION_FADE_MS)
    if is_card(s):
        return None
    fade_from = CAPTION_FADE_MS if k else 0
    return (s.get("caption") or "", min(1.0, max(0.0, (dt - fade_from) / CAPTION_FADE_MS)))


def dim_target(s: dict) -> float:
    return PANEL_DIM if shows_panel(s) else 0.0


def dim_at(ms: float, k: int) -> float:
    s = P["steps"][k]
    was = dim_target(P["steps"][k - 1]) if k else 0.0
    return lerp(was, dim_target(s), T.ease((ms - s["ms"]) / DIM_FADE_MS))


def headline_base(ms: float, k: int) -> Image.Image:
    steps = P["steps"]
    s = steps[k]
    now = base(tag_of(k), s["title"])
    dt = ms - s["ms"]
    if not k or dt >= HEADLINE_FADE_MS or is_card(steps[k - 1]):
        return now.copy()
    return Image.blend(base(tag_of(k - 1), steps[k - 1]["title"]), now, T.ease(dt / HEADLINE_FADE_MS))


def tag_of(k: int) -> str:
    return P["steps"][k].get("chapter") or f"{k + 1:02} / {len(P['steps']):02}"


def compose_main(ms: float) -> Image.Image:
    k = step_at(ms)
    s = P["steps"][k]
    if is_card(s):
        diag = s.get("focus", {}).get("callout", "")
        return T.card(tag_of(k), s["title"], s.get("caption") or "", diag if "<>" in diag else "",
                      (ms - s["ms"]) / 1000, logo(False), P["version"], lcd_at(ms) if "<>" in diag else None)
    im = headline_base(ms, k)
    z, centre, box, alpha = camera_at(ms)
    T.put_lcd(im, lcd_at(ms), z, centre)
    T.beat_glow(im, beat(ms))
    from PIL import ImageDraw
    d = ImageDraw.Draw(im)
    if box:
        p0 = T.lcd_to_frame(box[0], box[1], z, centre)
        p1 = T.lcd_to_frame(box[2], box[3], z, centre)
        T.region_box(d, (*p0, *p1), alpha)
    if shows_panel(s):
        states = panel_states(ms, k)
        lit = {c: st for c, st in states.items() if not (st.get("kind") == "key" and st.get("active", 0) <= 0)}
        keep = [T.highlight_rect(c) for c in lit]
        T.dim_panel(im, dim_at(ms, k) if lit else 0.0, [r for r in keep if r])
        for c, st in lit.items():
            T.control_highlight(im, c, st)
        pt = pointer_at(ms)
        if pt:
            T.pointer(im, pt, 0)
    at = callout_at(ms)
    if at:
        T.callout(im, s["callout_text"], at)
    cap = caption_at(P["steps"], ms)
    if cap:
        T.lower_third(im, *cap)
    return im


def ms_of(j: int) -> float:
    return j * 1000 / FPS


def frames_for(ms: float) -> int:
    return round(ms * FPS / 1000)


def compose(job: tuple) -> bytes:
    kind, j = job
    ms = ms_of(j)
    if kind == "main":
        im = compose_main(ms)
    elif kind == "teaser":
        at = P["main_ms"] - P["teaser_ms"] + ms
        im = base("SIDOU HOW-TO", P["title"]).copy()
        T.put_lcd(im, lcd_at(at), 1.0, (120, 120))
        T.beat_glow(im, beat(at))
    elif kind == "title":
        im = T.title_card(ms / 1000, P["title"], logo(True), TITLE_S)
    else:
        last = P["main_ms"] - LCD_MS
        s = P["steps"][-1]
        if is_card(s):
            im = compose_main(last)
        else:
            im = base("READY.", P["title"]).copy()
            z, centre, _, _ = between(camera_at(last), FULL, T.glide(ms / CAMERA_MOVE_MS))
            T.put_lcd(im, lcd_at(last), z, centre)
    return im.tobytes()


def fmt_srt(ms: float) -> str:
    ms = int(round(ms))
    return f"{ms // 3600000:02}:{ms // 60000 % 60:02}:{ms // 1000 % 60:02},{ms % 1000:03}"


def segments(plan: dict) -> list:
    cfg = plan["cfg"]
    main = frames_for(plan["main_ms"])
    teaser = min(frames_for(TEASER_S * 1000), main) if cfg.get("teaser", True) else 0
    title = frames_for(TITLE_S * 1000) if cfg.get("title", True) else 0
    return [("teaser", teaser), ("title", title), ("main", main), ("end", frames_for(END_S * 1000))]


def loudness(media: Path) -> dict:
    r = subprocess.run([FFMPEG, "-hide_banner", "-nostats", "-i", str(media), "-af",
                        "ebur128=peak=true", "-f", "null", "-"], capture_output=True, text=True, check=True)
    tail = r.stderr[r.stderr.rindex("Summary:"):]
    return {"lufs": float(re.search(r"I:\s+(-?[0-9.]+) LUFS", tail).group(1)),
            "true_peak_dbtp": float(re.search(r"Peak:\s+(-?[0-9.]+|-inf) dBFS", tail).group(1))}


def loud_enough(m: dict) -> bool:
    return abs(m["lufs"] - LUFS_TARGET) <= LUFS_TOLERANCE and m["true_peak_dbtp"] <= TRUE_PEAK_MAX_DB


def crossfaded(parts: list, total: int, xfade: int) -> array.array:
    out = array.array("h", bytes(4 * total))
    h = xfade // 2
    for i, (at, src, src_at, n, gain) in enumerate(parts):
        pre = h if i else 0
        post = h if i + 1 < len(parts) else 0
        p = faded(src, src_at - pre, n + pre + post, gain, 2 * pre, 2 * post)
        o = at - pre
        for k in range(max(0, -o), min(len(p) // 2, total - o)):
            for ch in (0, 1):
                v = out[2 * (o + k) + ch] + p[2 * k + ch]
                out[2 * (o + k) + ch] = max(-32768, min(32767, v))
    return out


def faded(src: array.array, start: int, n: int, gain: float, fade_in: int, fade_out: int) -> array.array:
    out = array.array("h", bytes(4 * n))
    for i in range(n):
        j = start + i
        if 0 <= j < len(src) // 2:
            g = gain * min(1.0, (i + 1) / fade_in if fade_in else 1.0, (n - i) / fade_out if fade_out else 1.0)
            out[2 * i] = int(src[2 * j] * g)
            out[2 * i + 1] = int(src[2 * j + 1] * g)
    return out


def loudnorm(src: Path, dst: Path) -> dict:
    target = "I=-16:TP=-1.5:LRA=11"
    r = subprocess.run([FFMPEG, "-hide_banner", "-nostats", "-i", str(src), "-af",
                        f"loudnorm={target}:print_format=json", "-f", "null", "-"],
                       capture_output=True, text=True, check=True)
    m = json.loads(r.stderr[r.stderr.rindex("{"):r.stderr.rindex("}") + 1])
    subprocess.run([FFMPEG, "-y", "-loglevel", "error", "-i", str(src), "-af",
                    f"loudnorm={target}:measured_I={m['input_i']}:measured_TP={m['input_tp']}:"
                    f"measured_LRA={m['input_lra']}:measured_thresh={m['input_thresh']}:"
                    f"offset={m['target_offset']}:linear=true", "-ar", "48000",
                    "-fflags", "+bitexact", "-flags:a", "+bitexact", str(dst)], check=True)
    m["aac_peak_db"] = fit_aac_peak(dst)
    return m


AAC = ["-c:a", "aac", "-b:a", "192k"]
AAC_PEAK_DB = -1.5
AAC_PEAK_MARGIN_DB = 0.1


def aac_peak_db(wav: Path) -> float:
    enc = wav.with_suffix(".peak.m4a")
    subprocess.run([FFMPEG, "-y", "-loglevel", "error", "-i", str(wav), *AAC, "-fflags", "+bitexact",
                    "-flags:a", "+bitexact", str(enc)], check=True)
    r = subprocess.run([FFMPEG, "-hide_banner", "-nostats", "-i", str(enc), "-af",
                        "astats=measure_perchannel=none:measure_overall=Peak_level", "-f", "null", "-"],
                       capture_output=True, text=True, check=True)
    enc.unlink()
    return float(re.findall(r"Peak level dB: (-?[0-9.]+|-inf)", r.stderr)[-1])


def fit_aac_peak(wav: Path) -> float:
    peak = aac_peak_db(wav)
    for _ in range(3):
        if peak <= AAC_PEAK_DB:
            break
        tmp = wav.with_suffix(".fit.wav")
        gain = AAC_PEAK_DB - AAC_PEAK_MARGIN_DB - peak
        subprocess.run([FFMPEG, "-y", "-loglevel", "error", "-i", str(wav), "-af", f"volume={gain:.2f}dB",
                        "-fflags", "+bitexact", "-flags:a", "+bitexact", str(tmp)], check=True)
        tmp.replace(wav)
        peak = aac_peak_db(wav)
    return peak


def outputs(plan: dict, segs: list, out: Path, rid: str) -> tuple:
    seg_ms = [round(ms_of(n)) for _, n in segs]
    main0 = seg_ms[0] + seg_ms[1]
    total = sum(seg_ms)
    srt, chap = [], []
    if seg_ms[0]:
        chap.append((0, "Preview"))
    if seg_ms[1]:
        chap.append((seg_ms[0], "Title"))
    chaptered = any(x.get("chapter") for x in plan["steps"])
    for i, s in enumerate(plan["steps"]):
        start, end = main0 + s["ms"], main0 + s["next_ms"]
        text = T.plain(s.get("caption") or "")
        if s.get("chapter"):
            text = f"[{s['chapter']}]\n{text}"
        srt.append(f"{i + 1}\n{fmt_srt(start)} --> {fmt_srt(end)}\n{text}\n")
        if s.get("chapter") or not chaptered:
            chap.append((start, T.plain(s.get("chapter") or s["title"])))
    chap.append((main0 + seg_ms[2], "The end"))
    (out / f"{rid}.srt").write_text("\n".join(srt))
    meta = [";FFMETADATA1", f"title={plan['title']}"]
    for j, (t, name) in enumerate(chap):
        e = chap[j + 1][0] if j + 1 < len(chap) else total
        meta += ["[CHAPTER]", "TIMEBASE=1/1000", f"START={t}", f"END={e}", f"title={name}"]
    mf = out / f"{rid}.chapters.txt"
    mf.write_text("\n".join(meta) + "\n")

    side = json.loads((Path(plan["cap"]) / "frames.json").read_text())
    a = read_wav(Path(plan["cap"]) / "audio.wav")
    st_side = json.loads((Path(plan["sound"]) / "frames.json").read_text())
    st = read_wav(Path(plan["sound"]) / "audio.wav")
    music0 = next(s["ms"] for s in st_side["steps"] if s["id"] == "play") + 300
    off = round(side["audio"]["offset_ms"] * RATE / 1000)

    def at(ms):
        return round(ms * RATE / 1000)

    n_main = seg_ms[2]
    main = piece(a, -off, at(n_main), 1.0, 60)
    beds = [s for s in plan["steps"] if s["id"] in plan["cfg"].get("bed", [])]
    for s in beds:
        n = at(s["next_ms"] - s["ms"])
        bed = piece(st, at(music0 + s["ms"]), n, 0.7, at(300))
        o = at(s["ms"])
        for i in range(min(2 * n, len(main) - 2 * o)):
            main[2 * o + i] = max(-32768, min(32767, main[2 * o + i] + bed[i]))
    starts = [sum(seg_ms[:i]) for i in range(4)]
    parts = [
        (at(starts[0]), a, at(n_main - seg_ms[0]) - off, at(seg_ms[0]), 1.0),
        (at(starts[1]), st, at(music0 + 4000), at(seg_ms[1]), 0.35),
        (at(starts[2]), main, 0, at(n_main), 1.0),
        (at(starts[3]), a, at(n_main - seg_ms[3]) - off, at(seg_ms[3]), 0.8),
    ]
    audio = crossfaded([p for p in parts if p[3]], at(total), at(XFADE_MS))
    raw = out / f"{rid}.raw.wav"
    write_wav(raw, audio)
    wav = out / f"{rid}.wav"
    loud = loudnorm(raw, wav)
    raw.unlink()
    return mf, wav, chap, loud


PREVIEW_MS = 80


def preview_frame(plan: dict, j: int) -> bool:
    ms = ms_of(j)
    lo, hi = plan["preview"]
    return lo <= ms < hi and int(ms // PREVIEW_MS) != int(ms_of(j - 1) // PREVIEW_MS)


def render(rid: str, fast: bool, jobs: int, formats: list) -> dict:
    t_start = time.monotonic()
    cap = recipe_capture(rid)
    sound = soundtrack_capture()
    t_cap = time.monotonic() - t_start
    plan = plan_of(rid, cap, sound)
    segs = segments(plan)
    plan["teaser_ms"] = ms_of(segs[0][1])
    OUT.mkdir(parents=True, exist_ok=True)
    mf, wav, chap, loud = outputs(plan, segs, OUT, rid)
    scale = 0.5 if fast else 1.0
    work = [(kind, j) for kind, n in segs for j in range(n)]
    result = {"id": rid, "seconds": round(ms_of(len(work)) / 1000, 1), "capture_s": round(t_cap, 1),
              "loudness_in_lufs": loud["input_i"], "aac_peak_db": loud["aac_peak_db"],
              "chapters": len(chap)}
    for fmt in formats:
        t0 = time.monotonic()
        init(plan, scale, fmt)
        W, H = T.size()
        name = rid if fmt == "wide" else f"{rid}-{fmt}"
        mp4 = OUT / f"{name}.mp4"
        preset = ["-preset", "veryfast", "-crf", "24"] if fast else ["-preset", "slow", "-crf", "16"]
        cmd = [FFMPEG, "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}",
               "-r", str(FPS), "-i", "-", "-i", str(wav), "-f", "ffmetadata", "-i", str(mf),
               "-map", "0:v", "-map", "1:a", "-map_chapters", "2", "-map_metadata", "2",
               "-c:v", "libx264", *preset, "-pix_fmt", "yuv420p", "-x264-params", "threads=1",
               *AAC, "-fflags", "+bitexact",
               "-flags:v", "+bitexact", "-flags:a", "+bitexact", "-movflags", "+faststart", str(mp4)]
        ff = subprocess.Popen(cmd, stdin=subprocess.PIPE)
        hashes, preview = [], []
        with Pool(jobs, initializer=init, initargs=(plan, scale, fmt)) as pool:
            for idx, raw in enumerate(pool.imap(compose, work, chunksize=4)):
                ff.stdin.write(raw)
                hashes.append(hashlib.sha256(raw).hexdigest())
                kind, j = work[idx]
                if fmt == "wide" and kind == "main" and preview_frame(plan, j):
                    preview.append(Image.frombytes("RGB", (W, H), raw).resize((480, 270), Image.LANCZOS))
        ff.stdin.close()
        if ff.wait() != 0:
            sys.exit(f"make-videos: ffmpeg failed on {name}")
        (OUT / f"{name}.frames.sha256").write_text("\n".join(hashes) + "\n")
        heard = loudness(mp4)
        if not loud_enough(heard):
            sys.exit(f"make-videos: {name} is {heard['lufs']} LUFS, {heard['true_peak_dbtp']} dBTP")
        result["loudness"] = heard
        result[fmt] = {"mp4": str(mp4.relative_to(ROOT)), "bytes": mp4.stat().st_size,
                       "frames_sha256": hashlib.sha256("".join(hashes).encode()).hexdigest(),
                       "render_s": round(time.monotonic() - t0, 1), "size": f"{W}x{H}"}
        if preview:
            webp = OUT / f"{rid}.webp"
            preview[0].save(webp, save_all=True, append_images=preview[1:], duration=80, loop=0,
                            quality=50, method=6)
            result["preview_bytes"] = webp.stat().st_size
    return result


def main() -> None:
    ap = argparse.ArgumentParser(description="SIDOU's how-to videos from docs/recipes")
    ap.add_argument("--recipe", action="append", help="only this recipe (repeatable)")
    ap.add_argument("--fast", action="store_true", help="half resolution, fast x264: a quick look")
    ap.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) - 1))
    ap.add_argument("--formats", default="wide,square,vertical",
                    help="comma list of wide (1920x1080), square (1080x1080), vertical (1080x1920)")
    a = ap.parse_args()
    formats = [f for f in a.formats.split(",") if f]
    for f in formats:
        if f not in T.LAYOUTS:
            sys.exit(f"make-videos: no format {f}")
    cfg = json.loads(VIDEOS.read_text())
    ids = a.recipe or sorted(p.stem for p in RECIPES.glob("*.json")
                             if cfg.get(p.stem, {}).get("video", True))
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(max(1, (os.cpu_count() or 2) // 2)) as ex:
        list(ex.map(lambda f: f(), [soundtrack_capture] + [lambda r=r: recipe_capture(r) for r in ids]))
    report = []
    for rid in ids:
        r = render(rid, a.fast, a.jobs, formats)
        report.append(r)
        sizes = ", ".join(f"{f} {r[f]['bytes'] / 1e6:.1f} MB in {r[f]['render_s']} s" for f in formats)
        print(f"{rid}: {r['seconds']} s; {sizes}; preview {r.get('preview_bytes', 0) // 1024} KB; "
              f"capture {r['capture_s']} s", flush=True)
    (OUT / f"report{'-' + '-'.join(a.recipe) if a.recipe else ''}.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
