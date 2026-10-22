# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""SIDOU's how-to videos from the recipes (docs/videos.md): each recipe of docs/recipes is run in
fm1-emu by the ergonomics harness (`ergo --capture full`), and its frames, audio and step
timeline become a video: a payoff teaser, a typed title card, the steps with the control lit on
the panel and the screen pushed in where the result shows, and an end beat. Run it through
tools/make-videos.

Nothing reads a clock or a random number: the same firmware, recipe and Pillow give the same
frames (their SHA-256 are written next to each video). Captures are cached under
build/videos/cache, keyed by the firmware image, the recipe (and the ones it starts after), the
songs it loads and the harness's sources, so a change of the look re-renders without the
emulator.
"""

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
FPS = 25
TEASER_S = 3.0
TITLE_S = 1.8
END_S = 1.5
RATE = 44118


# --------------------------------------------------------------- captures ---

def sha(*parts: bytes) -> str:
    h = hashlib.sha256()
    for p in parts:
        h.update(hashlib.sha256(p).digest())
    return h.hexdigest()


def fw_dir() -> Path:
    d = os.environ.get("SIDOU_FM1_BUILD")
    return Path(d) if d else ROOT / "build/fm1"


def recipe_chain(rid: str) -> list:
    """The recipe and the ones it starts after (prerequisites.after), as JSON values."""
    out, seen = [], set()
    while rid and rid not in seen:
        seen.add(rid)
        v = json.loads((RECIPES / f"{rid}.json").read_text())
        out.append(v)
        rid = v["prerequisites"].get("after")
    return out


def capture(rid: str, source: Path, key_parts: list) -> Path:
    """Runs (or reuses) the harness's full capture of one recipe; returns its directory."""
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


def recipe_capture(rid: str) -> Path:
    chain = recipe_chain(rid)
    songs = sorted({s for v in chain for s in v["prerequisites"].get("songs", [])})
    parts = [json.dumps(v, sort_keys=True).encode() for v in chain]
    parts += [(ROOT / f"assets/songs/{s}.sng").read_bytes() for s in songs]
    return capture(rid, RECIPES, parts)


def soundtrack_capture() -> Path:
    return capture("soundtrack", SOUNDTRACK,
                   [SOUNDTRACK.read_bytes(), (ROOT / "assets/songs/chrome-comet.sng").read_bytes()])


# ------------------------------------------------------------------ audio ---

def read_wav(path: Path) -> array.array:
    with wave.open(str(path)) as w:
        a = array.array("h")
        a.frombytes(w.readframes(w.getnframes()))
    return a


def piece(src: array.array, start: int, n: int, gain: float = 1.0, fade: int = 220) -> array.array:
    """`n` stereo frames of `src` from frame `start` (silence outside it), gained and faded."""
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


# ------------------------------------------------------------- the plan ---

VERB = {"press": "PRESS", "hold": "HOLD", "turn": "TURN", "sweep": "TURN", "pot": "TURN"}


def events_of(steps: list) -> list:
    """Every input as panel events: (control, kind, t0, t1, detents), ms of the capture."""
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
            elif op == "hold_key":
                ev.append((a["button"], "hold", t0, t1, 0))
                ev.append((a["note"], "key", t0 + 30, t0 + 180, 0))
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
    if op == "hold_key":
        return f"HOLD {a['button']} + KEY {panel.note_number(a['note']) - 52}"
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
        s["next_ms"] = steps[i + 1]["ms"] if i + 1 < len(steps) else n * 40
    cfg = json.loads(VIDEOS.read_text()).get(rid, {})
    # The beat starts where PLAY was first pressed (the song's first row, near enough).
    plays = [a["end_ms"] for s in steps for a in s["inputs"] if a["op"] == "press" and a.get("button") == "PLAY"]
    anchor = plays[0] if plays else None
    # The preview: 4 s where the most happens (the busiest step's start), else the end.
    busiest = max(steps, key=lambda s: len([a for a in s["inputs"] if a["op"] != "wait"])) if steps else None
    p0 = busiest["ms"] // 40 if busiest else max(0, n - 100)
    if cfg.get("preview_step"):
        p0 = next(x for x in steps if x["id"] == cfg["preview_step"])["ms"] // 40
    return {
        "id": rid, "title": v["title"], "goal": v["goal"], "frames": n, "cap": str(cap),
        "sound": str(sound), "steps": steps, "events": events_of(steps), "palette": palette(cap),
        "version": version(), "cfg": cfg, "bpm": cfg.get("bpm"), "beat_anchor": anchor,
        "preview": [p0, min(n, p0 + 100)],
    }


def palette(cap: Path) -> list:
    p = Image.open(cap / "frames/00001.png").getpalette()[:48]
    return [tuple(p[i:i + 3]) for i in range(0, 48, 3)]


def version() -> str:
    m = re.search(r"project\(sidou VERSION ([0-9.]+)", (ROOT / "CMakeLists.txt").read_text())
    return f"SIDOU {m.group(1) if m else '?'} - PLAYED ON SIDOU"


# ---------------------------------------------------------------- render ---

P: dict = {}


def init(plan: dict, scale: float, fmt: str) -> None:
    P.clear()
    P.update(plan)
    T.S = scale
    T.set_format(fmt)
    T.PAL = [tuple(x) for x in plan["palette"]]
    base.cache_clear()


@lru_cache(maxsize=8)
def logo(two: bool) -> Image.Image:
    name = "metal-chrome-lettering-2x.png" if two else "metal-chrome-lettering.png"
    return Image.open(ROOT / "assets/logo" / name).convert("RGBA")


@lru_cache(maxsize=4)
def lcd(i: int) -> Image.Image:
    return Image.open(Path(P["cap"]) / f"frames/{i + 1:05}.png").convert("RGB")


@lru_cache(maxsize=64)
def base(tag: str, headline: str) -> Image.Image:
    return T.frame_base(tag, headline, P["version"], logo(False))


def step_at(ms: float) -> int:
    i = 0
    for k, s in enumerate(P["steps"]):
        if s["ms"] <= ms:
            i = k
    return i


def is_card(s: dict) -> bool:
    return s.get("focus", {}).get("control") == "card"


def zoom_at(ms: float) -> tuple:
    """The push-in (zoom, centre, alpha, region) at a capture time."""
    k = step_at(ms)
    s = P["steps"][k]
    reg = s["region"]
    if not reg:
        return 1.0, (120, 120), 0.0, None
    side = max(reg[2] - reg[0], reg[3] - reg[1])
    target = max(1.0, min(1.6, 0.9 * 240 / side))
    cin = T.ease((ms - s["acts_end"] - 200) / 500)
    nxt = P["steps"][k + 1]["region"] if k + 1 < len(P["steps"]) else None
    cout = 0.0 if nxt == reg else T.ease((ms - (s["next_ms"] - 500)) / 500)
    a = cin * (1 - cout)
    return 1 + (target - 1) * a, ((reg[0] + reg[2]) / 2, (reg[1] + reg[3]) / 2), a, reg


def angle_of(name: str, ms: float) -> float:
    deg = 0.0
    for c, kind, t0, t1, det in P["events"]:
        if c == name and kind == "turn" and t0 <= ms:
            deg += 18 * det * min(1.0, (ms - t0) / max(1, t1 - t0))
    return deg


def panel_states(ms: float, k: int) -> dict:
    """Each lit control: its cue at this time."""
    s = P["steps"][k]
    out = {}
    lo, hi = s["ms"], s["next_ms"]
    f = primary(s)
    if f:
        out[f] = {"active": 0.0}
    for c, kind, t0, t1, det in P["events"]:
        if t1 < lo or t0 >= hi:
            continue
        # Only what is in use now (from just before to a moment after), and the step's focus.
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


def pointer_at(ms: float) -> tuple | None:
    """The sprite pointer: it flies (eased, 0.3 s) to each control just before it is used."""
    lead, fly = 300, 300
    targets = [(t0 - lead, c) for c, kind, t0, t1, det in P["events"]]
    targets.sort(key=lambda x: x[0])
    prev, cur, since = None, None, 0
    for t, c in targets:
        if t <= ms:
            if c != cur:
                prev, cur, since = cur, c, t
        else:
            break
    if cur is None:
        if not targets:
            return None
        cur, since, prev = targets[0][1], -1e9, None
    b = T.control_point(cur)
    if b is None:
        return None
    a = T.control_point(prev) if prev else None
    if a is None:
        return b
    e = T.ease((ms - since) / fly)
    return (a[0] + (b[0] - a[0]) * e, a[1] + (b[1] - a[1]) * e)


def beat(ms: float) -> float:
    """0..1: how much the frame glows, strongest on each beat of the song."""
    bpm, anchor = P.get("bpm"), P.get("beat_anchor")
    if not bpm or anchor is None or ms < anchor:
        return 0.0
    frac = ((ms - anchor) * bpm / 60000) % 1.0
    return 0.8 * (1 - frac) ** 3


def primary(s: dict) -> str | None:
    """The control the step's callout points at: its focus control, else its first input's."""
    f = s.get("focus", {}).get("control")
    if f and f not in ("device", "screen", "card"):
        return f
    for c, kind, t0, t1, det in P["events"]:
        if s["ms"] <= t0 < s["next_ms"]:
            return c
    return None


def compose_main(i: int) -> Image.Image:
    ms = i * 40
    k = step_at(ms)
    s = P["steps"][k]
    tag = s.get("chapter") or f"{k + 1:02} / {len(P['steps']):02}"
    if is_card(s):
        diag = s.get("focus", {}).get("callout", "")
        im = T.card(tag, s["title"], s.get("caption") or "", diag if "<>" in diag else "",
                    (ms - s["ms"]) / 1000, logo(False), P["version"], lcd(i) if "<>" in diag else None)
        T.raster_border(im, (ms - s["ms"]) / 450, T.WARM, T.wipe_alpha((ms - s["ms"]) / 1000))
        return im
    im = base(tag, s["title"]).copy()
    z, centre, a, reg = zoom_at(ms)
    T.put_lcd(im, lcd(i), z, centre)
    T.beat_glow(im, beat(ms))
    from PIL import ImageDraw
    d = ImageDraw.Draw(im)
    if reg:
        p0 = T.lcd_to_frame(reg[0], reg[1], z, centre)
        p1 = T.lcd_to_frame(reg[2], reg[3], z, centre)
        T.region_box(d, (*p0, *p1), a)
        T.sparkle(im, ((p0[0] + p1[0]) // 2, p0[1]), (ms - s["acts_end"] - 700) / 1000)
    mode = s.get("focus", {}).get("control")
    main = primary(s) if mode not in ("screen", "device") else None
    if mode not in ("screen", "device"):
        states = panel_states(ms, k)
        lit = {c: st for c, st in states.items() if not (st.get("kind") == "key" and st.get("active", 0) <= 0)}
        keep = [T.highlight_rect(c) for c in lit]
        dim = 0.5 * T.ease((ms - s["ms"]) / 250) if lit else 0.0
        T.dim_panel(im, dim, [r for r in keep if r])
        for c, st in lit.items():
            T.control_highlight(im, c, st)
        pt = pointer_at(ms)
        if pt:
            T.pointer(im, pt, 0)
    T.callout_near(im, s["callout_text"], T.highlight_rect(main) if main else None)
    T.raster_border(im, (ms - s["ms"]) / 450, T.COOL, T.wipe_alpha((ms - s["ms"]) / 1000) if k else 0)
    return im


def compose(job: tuple) -> bytes:
    kind, j = job
    if kind == "main":
        im = compose_main(j)
    elif kind == "teaser":
        i = P["frames"] - P["teaser"] + j
        im = base("SIDOU HOW-TO", P["title"]).copy()
        T.put_lcd(im, lcd(i), 1.0, (120, 120))
        T.beat_glow(im, beat(i * 40))
        T.raster_border(im, j / 25, T.WARM, 1.0 if j < 3 else 0.0)
    elif kind == "title":
        im = T.title_card(j / FPS, P["title"], logo(True), TITLE_S)
    else:
        last = P["frames"] - 1
        s = P["steps"][-1]
        if is_card(s):
            im = compose_main(last)
        else:
            im = base("READY.", P["title"]).copy()
            z, centre, _, _ = zoom_at(last * 40)
            k = T.ease(j / (FPS * 0.6))
            T.put_lcd(im, lcd(last), z + (1 - z) * k, centre)
        T.raster_border(im, j / 12, T.WARM, max(0.0, 1 - j / 10))
    return im.tobytes()


# ----------------------------------------------------------- the outputs ---

def fmt_srt(ms: float) -> str:
    ms = int(round(ms))
    return f"{ms // 3600000:02}:{ms // 60000 % 60:02}:{ms // 1000 % 60:02},{ms % 1000:03}"


def segments(plan: dict) -> list:
    """The video's parts: (kind, frames)."""
    cfg = plan["cfg"]
    teaser = min(int(TEASER_S * FPS), plan["frames"]) if cfg.get("teaser", True) else 0
    title = int(TITLE_S * FPS) if cfg.get("title", True) else 0
    return [("teaser", teaser), ("title", title), ("main", plan["frames"]), ("end", int(END_S * FPS))]


def loudnorm(src: Path, dst: Path) -> dict:
    """Two-pass EBU R128 normalisation to -16 LUFS integrated, -1.5 dBTP, 48 kHz."""
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
    return m


def outputs(plan: dict, segs: list, out: Path, rid: str) -> tuple:
    """The subtitles, the chapters (ffmetadata) and the normalised audio."""
    main0 = (segs[0][1] + segs[1][1]) * 40
    total = sum(n for _, n in segs) * 40
    srt, chap = [], []
    if segs[0][1]:
        chap.append((0, "Preview"))
    if segs[1][1]:
        chap.append((segs[0][1] * 40, "Title"))
    chaptered = any(x.get("chapter") for x in plan["steps"])
    for i, s in enumerate(plan["steps"]):
        start, end = main0 + s["ms"], main0 + s["next_ms"]
        text = T.plain(s.get("caption") or "")
        if s.get("chapter"):
            text = f"[{s['chapter']}]\n{text}"
        srt.append(f"{i + 1}\n{fmt_srt(start)} --> {fmt_srt(end)}\n{text}\n")
        if s.get("chapter") or not chaptered:
            chap.append((start, T.plain(s.get("chapter") or s["title"])))
    chap.append((main0 + plan["frames"] * 40, "The end"))
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

    n_main = plan["frames"] * 40
    main = piece(a, -off, at(n_main), 1.0, 60)
    # The soundtrack under the steps the recipe leaves quiet (videos.json `bed`), from its start.
    beds = [s for s in plan["steps"] if s["id"] in plan["cfg"].get("bed", [])]
    for s in beds:
        n = at(s["next_ms"] - s["ms"])
        bed = piece(st, at(music0 + s["ms"]), n, 0.7, at(300))
        o = at(s["ms"])
        for i in range(min(2 * n, len(main) - 2 * o)):
            main[2 * o + i] = max(-32768, min(32767, main[2 * o + i] + bed[i]))
    parts = [
        piece(a, at(n_main - segs[0][1] * 40) - off, at(segs[0][1] * 40)),
        piece(st, at(music0 + 4000), at(segs[1][1] * 40), 0.35, at(200)),
        main,
        piece(a, at(n_main - segs[3][1] * 40) - off, at(segs[3][1] * 40), 0.8, at(segs[3][1] * 40) // 2),
    ]
    audio = array.array("h")
    for p in parts:
        audio.extend(p)
    raw = out / f"{rid}.raw.wav"
    write_wav(raw, audio)
    wav = out / f"{rid}.wav"
    loud = loudnorm(raw, wav)
    raw.unlink()
    return mf, wav, chap, loud


def render(rid: str, fast: bool, jobs: int, formats: list) -> dict:
    t_start = time.monotonic()
    cap = recipe_capture(rid)
    sound = soundtrack_capture()
    t_cap = time.monotonic() - t_start
    plan = plan_of(rid, cap, sound)
    segs = segments(plan)
    plan["teaser"] = segs[0][1]
    OUT.mkdir(parents=True, exist_ok=True)
    mf, wav, chap, loud = outputs(plan, segs, OUT, rid)
    scale = 0.5 if fast else 1.0
    work = [(kind, j) for kind, n in segs for j in range(n)]
    result = {"id": rid, "seconds": round(len(work) / FPS, 1), "capture_s": round(t_cap, 1),
              "loudness_in_lufs": loud["input_i"], "chapters": len(chap)}
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
               "-c:a", "aac", "-b:a", "192k", "-fflags", "+bitexact",
               "-flags:v", "+bitexact", "-flags:a", "+bitexact", "-movflags", "+faststart", str(mp4)]
        ff = subprocess.Popen(cmd, stdin=subprocess.PIPE)
        hashes, preview = [], []
        with Pool(jobs, initializer=init, initargs=(plan, scale, fmt)) as pool:
            for idx, raw in enumerate(pool.imap(compose, work, chunksize=4)):
                ff.stdin.write(raw)
                hashes.append(hashlib.sha256(raw).hexdigest())
                kind, j = work[idx]
                if fmt == "wide" and kind == "main" and plan["preview"][0] <= j < plan["preview"][1] and j % 2 == 0:
                    preview.append(Image.frombytes("RGB", (W, H), raw).resize((480, 270), Image.LANCZOS))
        ff.stdin.close()
        if ff.wait() != 0:
            sys.exit(f"make-videos: ffmpeg failed on {name}")
        (OUT / f"{name}.frames.sha256").write_text("\n".join(hashes) + "\n")
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
    ids = a.recipe or sorted(p.stem for p in RECIPES.glob("*.json"))
    # The captures first, several at once (each is one emulator on one core); then the renders,
    # each on every core.
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
