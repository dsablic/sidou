# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later

import array
import math
import random
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import make  # noqa: E402
import measure  # noqa: E402

TOP = [0, 0, 240, 16]
TOP_AND_MORE = [0, 0, 240, 24]
KNOBS = [0, 176, 240, 240]
NARROW = [0, 0, 96, 96]


def steps(*spec: tuple) -> list:
    out, t = [], 0
    for region, ms in spec:
        out.append({"id": f"s{len(out)}", "ms": t, "next_ms": t + ms, "acts_end": t + ms // 2,
                    "region": region, "caption": f"caption {len(out)}", "inputs": []})
        t += ms
    return out


def camera_track(st: list) -> list:
    moves = make.camera_moves(st)
    step = 1000 / make.CAMERA_FPS
    return measure.moves([measure.view_state(*make.camera_state(moves, i * step))
                          for i in range(int(st[-1]["next_ms"] / step))], step)


class Camera(unittest.TestCase):
    def test_it_moves_only_when_the_region_really_changes(self):
        st = steps((None, 3000), (TOP, 3000), (TOP_AND_MORE, 3000), (KNOBS, 3000), (KNOBS, 3000))
        self.assertEqual(len(camera_track(st)), 2)

    def test_each_move_takes_its_full_glide(self):
        st = steps((None, 3000), (NARROW, 3000), (None, 3000), (KNOBS, 3000))
        for m in camera_track(st):
            self.assertGreaterEqual(m["ms"], make.CAMERA_MOVE_MS - 1000 / make.CAMERA_FPS)

    def test_it_never_goes_back_and_forth_within_a_second(self):
        st = steps(*[(r, 1500) for r in (NARROW, None, NARROW, None, NARROW, None)])
        self.assertEqual(measure.reversals(camera_track(st)), 0)


class Overlays(unittest.TestCase):
    def test_one_caption_at_a_time_with_short_fades(self):
        st = steps((None, 3000), (None, 3000))
        fade = make.CAPTION_FADE_MS
        old, a = make.caption_at(st, 3000 + fade / 2)
        self.assertEqual((old, round(a, 2)), ("caption 0", 0.5))
        new, b = make.caption_at(st, 3000 + 1.5 * fade)
        self.assertEqual((new, round(b, 2)), ("caption 1", 0.5))
        self.assertEqual(make.caption_at(st, 3000 + 2 * fade), ("caption 1", 1.0))

    def test_the_pointer_finishes_each_glide_before_the_next(self):
        st = steps((None, 2000), (None, 2000))
        events = [("K1", "turn", 100, 400, 2), ("SELECT", "turn", 300, 500, 1), ("K4", "turn", 2100, 2300, 1)]
        glides = make.pointer_glides(st, events)
        self.assertEqual([g[2] for g in glides], ["K1", "SELECT", "K4"])
        for a, b in zip(glides, glides[1:]):
            self.assertGreaterEqual(b[0] - a[0], make.POINTER_GLIDE_MS)


class Sound(unittest.TestCase):
    def test_segments_crossfade_without_a_jump(self):
        n = 2000
        loud = array.array("h", [8000] * (6 * n))
        quiet = array.array("h", [-8000] * (6 * n))
        out = make.crossfaded([(0, loud, n, n, 1.0), (n, quiet, n, n, 1.0)], 2 * n, 400)
        left = out[::2]
        self.assertLess(max(abs(a - b) for a, b in zip(left, left[1:])), 100)

    def test_the_mix_meets_the_loudness_target(self):
        rng = random.Random(1)
        n = make.RATE * 6
        tone = array.array("h", [int(30000 * math.sin(i / 9) * rng.random()) for i in range(n) for _ in (0, 1)])
        with tempfile.TemporaryDirectory() as d:
            raw, wav = Path(d) / "raw.wav", Path(d) / "out.wav"
            make.write_wav(raw, tone)
            make.loudnorm(raw, wav)
            self.assertTrue(make.loud_enough(make.loudness(wav)), make.loudness(wav))

    def test_a_punchy_take_meets_the_loudness_target_under_the_true_peak_limit(self):
        n = make.RATE * 6
        clicks = array.array("h", [int(30000 if i % 4410 < 2 else -30000 if i % 4410 < 4 else 3000 * math.sin(i / 7))
                                   for i in range(n) for _ in (0, 1)])
        with tempfile.TemporaryDirectory() as d:
            raw, wav, enc = Path(d) / "raw.wav", Path(d) / "out.wav", Path(d) / "out.m4a"
            make.write_wav(raw, clicks)
            make.loudnorm(raw, wav)
            make.encode_aac(wav, enc)
            self.assertTrue(make.loud_enough(make.loudness(enc)), make.loudness(enc))

    def test_an_intersample_peak_is_turned_under_the_true_peak_limit(self):
        n = make.RATE * 3
        amp = 32767 * 10 ** (-2.0 / 20)
        quarter = array.array("h", [int(amp * math.sin(math.pi * i / 2 + math.pi / 4)) for i in range(n) for _ in (0, 1)])
        with tempfile.TemporaryDirectory() as d:
            wav, enc = Path(d) / "in.wav", Path(d) / "out.m4a"
            make.write_wav(wav, quarter)
            make.fit_aac_peak(wav)
            make.encode_aac(wav, enc)
            self.assertLessEqual(make.loudness(enc)["true_peak_dbtp"], make.TRUE_PEAK_MAX_DB)


if __name__ == "__main__":
    unittest.main()
