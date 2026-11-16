# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later

import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import make  # noqa: E402
import template as T  # noqa: E402

READ_WPM = 180
READ_BEAT_MS = 1000
REVERSAL_MS = 1000
STILL = 1e-4
JUMP_MS = 400
JUMP_PX = 20


def reading_ms(caption: str) -> int:
    return round(len(T.plain(caption).split()) * 60000 / READ_WPM) + READ_BEAT_MS


def dwell(plan: dict) -> list:
    out = []
    for s in plan["steps"]:
        need = s.get("read_ms", reading_ms(s.get("caption") or ""))
        out.append({"id": s["id"], "dwell_ms": s["next_ms"] - s["ms"], "read_ms": need,
                    "after_action_ms": s["next_ms"] - s["acts_end"]})
    return out


def view_state(zoom: float, centre: tuple, box: tuple | None, alpha: float) -> tuple:
    span = 240 / max(zoom, 1.0)
    cx = min(max(centre[0], span / 2), 240 - span / 2)
    cy = min(max(centre[1], span / 2), 240 - span / 2)
    lit = tuple(v * alpha for v in box) if box else (0.0, 0.0, 0.0, 0.0)
    return (span, cx, cy, *lit, alpha)


def moves(samples: list, frame_ms: float) -> list:
    out, cur = [], None
    for i in range(1, len(samples)):
        if max(abs(x - y) for x, y in zip(samples[i - 1], samples[i])) <= STILL:
            continue
        if cur and cur["end"] == i - 1:
            cur["end"] = i
        else:
            cur = {"start": i - 1, "end": i}
            out.append(cur)
    return [{"start_ms": m["start"] * frame_ms, "ms": (m["end"] - m["start"]) * frame_ms,
             "from": samples[m["start"]], "to": samples[m["end"]]} for m in out]


def undoes(a: dict, b: dict) -> bool:
    return max(abs(x - y) for x, y in zip(a["from"], b["to"])) <= 1.0


def reversals(ms: list) -> int:
    return sum(1 for a, b in zip(ms, ms[1:])
               if undoes(a, b) and b["start_ms"] - (a["start_ms"] + a["ms"]) < REVERSAL_MS)


def camera_samples(plan: dict, fps: float) -> list:
    make.init(plan, 1.0, "wide")
    step = 1000 / fps
    return [view_state(*make.camera_at(i * step)) for i in range(int(plan["frames"] * 40 / step))]


def pointer_jumps(plan: dict, fps: float) -> int:
    make.init(plan, 1.0, "wide")
    step = 1000 / fps
    at = [make.pointer_at(i * step) for i in range(int(plan["frames"] * 40 / step))]
    track = moves([p or (0.0, 0.0) for p in at], step)
    return sum(1 for m in track if m["ms"] < JUMP_MS and math.dist(m["from"], m["to"]) > JUMP_PX)


def report(rid: str, media: Path | None) -> dict:
    cap = make.recipe_capture(rid)
    plan = make.plan_of(rid, cap, make.soundtrack_capture())
    side = json.loads((cap / "frames.json").read_text())
    fps = make.CAMERA_FPS
    cam = moves(camera_samples(plan, fps), 1000 / fps)
    steps = dwell(plan)
    seconds = plan["frames"] * 40 / 1000
    out = {
        "id": rid, "main_s": seconds,
        "steps": steps,
        "short_steps": sum(1 for s in steps if s["dwell_ms"] < s["read_ms"]),
        "camera_moves": len(cam),
        "camera_moves_per_s": round(len(cam) / seconds, 3),
        "camera_move_ms": sorted(round(m["ms"]) for m in cam),
        "camera_reversals_1s": reversals(cam),
        "pointer_jumps": pointer_jumps(plan, fps),
        "lcd": side.get("lcd", {}),
    }
    if media:
        out["loudness"] = make.loudness(media)
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description="pacing, camera, LCD and loudness figures of one video")
    ap.add_argument("recipe")
    ap.add_argument("--media", type=Path, help="the rendered MP4 or WAV, for its loudness")
    a = ap.parse_args()
    print(json.dumps(report(a.recipe, a.media), indent=2))


if __name__ == "__main__":
    main()
