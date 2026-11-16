// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::scene::feed_the_tap;
use crate::scene_rmr::W;

const LANE: usize = SCENE_SCOPE_LANE_H as usize;
const MID: u8 = SCENE_SCOPE_MID as u8;
const A_HELD_NOTE: u8 = 57;
const LOUD: u8 = 220;
const SETTLE_FRAMES: usize = 12;

struct Scope {
    so: Box<scene_scope>,
    px: crate::scene_rmr::Rig,
    tap: Box<scope>,
    keys: crate::Rig,
    acc: [u32; 3],
}

impl Scope {
    fn new(phase: u32) -> Scope {
        let mut so = Box::<scene_scope>::default();
        let mut tap = Box::<scope>::default();
        unsafe {
            scene_scope_init(&mut *so);
            scope_init(&mut *tap);
        }
        let mut s = Scope {
            so,
            px: crate::scene_rmr::Rig::new(),
            tap,
            keys: crate::Rig::new(SID_MODEL_6581),
            acc: [phase; 3],
        };
        s.frame();
        s.sent();
        s
    }

    fn play(&mut self, note: u8) -> usize {
        self.keys.note_on(note);
        let voice = &self.keys.live.voice;
        let v = (0..3)
            .find(|&v| voice[v].gate != 0 && voice[v].note == note)
            .expect("a voice plays the note");
        self.tap.env = [0; 3];
        self.tap.env[v] = LOUD;
        v
    }

    fn frame(&mut self) {
        for _ in 0..SCENE_SCOPE_LANES {
            let freq = std::array::from_fn(|v| self.keys.live.voice[v].freq_out);
            feed_the_tap(&mut self.tap, &mut self.acc, freq);
            unsafe {
                scene_scope_frame(
                    &mut *self.so,
                    &*self.tap,
                    &*self.keys.live,
                    &mut *self.px.sp,
                )
            };
        }
    }

    fn sent(&mut self) -> Vec<(usize, usize)> {
        unsafe { scene_px_scan(&mut *self.px.sp, &mut *self.px.g, GRID_CELLS) };
        let marked = (0..GRID_CELLS as usize)
            .filter(|&t| unsafe { grid_marked(&*self.px.g, (t % 30) as u32, (t / 30) as u32) } != 0)
            .map(|t| (t % 30, t / 30))
            .collect();
        crate::drain(&mut self.px.g);
        marked
    }

    fn settled(&mut self) {
        for _ in 0..SETTLE_FRAMES {
            self.frame();
            self.sent();
        }
    }

    fn lane(&self, v: usize) -> [u8; W] {
        self.so.y[v]
    }
}

fn swing(y: &[u8]) -> u8 {
    y.iter().max().unwrap() - y.iter().min().unwrap()
}

#[test]
fn the_trigger_is_the_first_rising_crossing_of_the_middle() {
    let w = SCOPE_W as usize;
    for shift in [0, 17, 80, 159] {
        let saw: Vec<u8> = (0..w).map(|k| ((k + shift) % w * 256 / w) as u8).collect();
        let k = unsafe { scene_scope_trigger(saw.as_ptr()) } as usize;
        assert!(
            saw[(k + w - 1) % w] < 128 && saw[k] >= 128,
            "shift {shift}: {k}"
        );
        assert_eq!((k + shift) % w, w / 2, "shift {shift}");
    }
    assert_eq!(
        unsafe { scene_scope_trigger([128u8; SCOPE_W as usize].as_ptr()) },
        0
    );
}

#[test]
fn a_held_note_stands_still_whatever_its_phase() {
    let traces: Vec<[u8; W]> = [0, 0x55_0000, 0xc3_0000]
        .into_iter()
        .map(|phase| {
            let mut s = Scope::new(phase);
            let v = s.play(A_HELD_NOTE);
            s.settled();
            let held = s.lane(v);
            for f in 0..25 {
                s.frame();
                assert_eq!(s.sent(), vec![], "frame {f}: a held note moved");
            }
            assert_eq!(s.lane(v), held);
            assert!(swing(&held) > 40, "the saw's swing {}", swing(&held));
            for other in (0..3).filter(|&o| o != v) {
                assert!(
                    s.lane(other).iter().all(|&y| y == MID),
                    "voice {other} is silent"
                );
            }
            held
        })
        .collect();
    let edges = unsafe { live_view_scope_cycles(A_HELD_NOTE) } * SCENE_SCOPE_EDGE_JITTER;
    for other in &traces[1..] {
        let apart = (0..W)
            .filter(|&x| traces[0][x].abs_diff(other[x]) > SCENE_SCOPE_STILL as u8)
            .count();
        assert!(
            apart <= edges as usize,
            "the trigger holds the trace at any phase: {apart} columns apart"
        );
    }
}

fn colours(s: &Scope) -> Vec<u8> {
    let mut seen: Vec<u8> = s.px.screen();
    seen.sort_unstable();
    seen.dedup();
    seen
}

#[test]
fn the_trace_glows_in_the_colours_of_the_chip_the_keys_play() {
    let mut s = Scope::new(0);
    s.play(A_HELD_NOTE);
    s.settled();
    let [a, b] = unsafe { scene_scope_colour };
    let shown = colours(&s);
    for c in a {
        assert!(shown.contains(&c), "chip A's colour {c} in {shown:?}");
    }
    assert!(shown.contains(&(GRID_DARK_GREY as u8)), "the graticule");
    s.keys.live.two_chips = 1;
    s.keys.live.keys_chip = 1;
    s.frame();
    assert!(!s.sent().is_empty(), "a chip change repaints the trace");
    let shown = colours(&s);
    for c in b {
        assert!(shown.contains(&c), "chip B's colour {c} in {shown:?}");
    }
    for c in a {
        assert!(!shown.contains(&c), "chip A's colour {c} under chip B");
    }
}

#[test]
fn silence_lays_the_trace_flat_and_then_sends_nothing() {
    let mut s = Scope::new(0);
    let v = s.play(A_HELD_NOTE);
    s.settled();
    s.tap.env[v] = 0;
    s.frame();
    assert!(s.lane(v).iter().all(|&y| y == MID));
    assert!(!s.sent().is_empty());
    for _ in 0..SCENE_SCOPE_CALM_FRAMES {
        s.frame();
        assert_eq!(s.sent(), vec![]);
    }
    assert_eq!(u32::from(s.so.quiet), SCENE_SCOPE_CALM_FRAMES);
}

#[test]
fn a_softer_note_redraws_only_the_tiles_its_old_and_new_traces_cross() {
    let mut s = Scope::new(0);
    let v = s.play(A_HELD_NOTE);
    s.settled();
    let was = s.lane(v);
    s.tap.env[v] = LOUD / 2;
    s.frame();
    let now = s.lane(v);
    assert_ne!(now, was);
    let marked = s.sent();
    assert!(!marked.is_empty());
    let lo = usize::from(*was.iter().chain(&now).min().unwrap()) - 1;
    let hi = usize::from(*was.iter().chain(&now).max().unwrap()) + 1;
    let top = v * LANE / 8;
    for &(c, r) in &marked {
        assert!(
            (top..top + LANE / 8).contains(&r) && (r - top) * 8 + 7 >= lo && (r - top) * 8 <= hi,
            "tile ({c}, {r}) outside the traces' rows {lo}..={hi}"
        );
    }
    assert!(
        marked.len() < (hi / 8 - lo / 8 + 1) * GRID_COLS as usize,
        "{} tiles: more than the traces' band",
        marked.len()
    );
}
