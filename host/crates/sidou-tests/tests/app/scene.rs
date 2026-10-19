// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SCENE (app/scene.c, app/scene_view.c): each piece's rules against a model of its own (rule
//! 30, Langton's ants, the starfield's flight, the fire's and the plasma's bounds), what the
//! music moves in each, and the view: reached from the menu, SELECT stepping the pieces, the
//! tap while it shows, and every frame inside the SPI budget.

use std::ffi::CStr;

use sidou_sys::*;

use crate::cutoff_after;
use crate::views::Ui;
use crate::{
    assert_wipe_holds_still, drain, petscii, petscii_mut, wipe_frames, Sent, SPI_BUDGET_MS,
};

const COLS: usize = GRID_COLS as usize;
const ROWS: usize = GRID_ROWS as usize;
const W60: usize = SCENE_RULE30_W as usize;
const BLANK: (u8, u8) = (b' ', 0);

fn new_scene(piece: u32) -> Box<scene> {
    let mut s = Box::<scene>::default();
    unsafe { scene_start(&mut *s, piece, &quiet()) };
    s
}

fn step(s: &mut scene, m: &scene_music) {
    unsafe { scene_step(s, m) };
}

/// No voice playing; the default preset's cutoff and resonance.
fn quiet() -> scene_music {
    scene_music {
        gate: [0; 3],
        trig: [0; 3],
        note: [SCENE_NO_NOTE as u8; 3],
        env: [0; 3],
        color: [GRID_YELLOW as u8; 3],
        cutoff: 1200,
        resonance: 6,
    }
}

/// Voice v held at note with envelope env (no note-on this frame).
fn held(mut m: scene_music, v: usize, note: u8, env: u8) -> scene_music {
    m.gate[v] = 1;
    m.note[v] = note;
    m.env[v] = env;
    m
}

/// Voice v's note-on this frame.
fn hit(m: scene_music, v: usize, note: u8, env: u8) -> scene_music {
    let mut m = held(m, v, note, env);
    m.trig[v] = 1;
    m
}

fn cell(s: &scene, c: usize, r: usize) -> (u8, u8) {
    (s.cv.ch[r][c], s.cv.attr[r][c])
}

/// Cells of the canvas that differ between two snapshots.
fn changed(a: &scene_canvas, b: &scene_canvas) -> usize {
    (0..ROWS)
        .flat_map(|r| (0..COLS).map(move |c| (r, c)))
        .filter(|&(r, c)| a.ch[r][c] != b.ch[r][c] || a.attr[r][c] != b.attr[r][c])
        .count()
}

fn attr(fg: u32, bg: u32) -> u8 {
    ((bg << 4) | fg) as u8
}

// ---- RULE 30 ----

/// Rule 30 on a ring of 60, as Wolfram states it: new = left XOR (centre OR right).
fn rule30_model(cur: &[bool; W60]) -> [bool; W60] {
    std::array::from_fn(|i| cur[(i + W60 - 1) % W60] ^ (cur[i] | cur[(i + 1) % W60]))
}

fn bits(x: u64) -> [bool; W60] {
    std::array::from_fn(|i| x >> i & 1 != 0)
}

#[test]
fn rule30_steps_by_the_rule_on_a_ring_of_60() {
    let mut x = 1u64 << 30;
    let mut model = bits(x);
    for g in 0..200 {
        x = unsafe { scene_rule30_next(x) };
        model = rule30_model(&model);
        assert_eq!(bits(x), model, "generation {}", g + 1);
        assert_eq!(x >> W60, 0, "bits past the ring");
    }
    // The first generations of a single cell: the familiar triangle.
    let one = unsafe { scene_rule30_next(1 << 30) };
    assert_eq!(one, 0b111 << 29);
    assert_eq!(
        unsafe { scene_rule30_next(one) },
        0b10011 << 28,
        "cells 28..32: 1 1 0 0 1"
    );
    // Round the ring's ends.
    assert_eq!(
        unsafe { scene_rule30_next(1) },
        1 << 59 | 0b11,
        "cell 0's neighbours are 59 and 1"
    );
}

fn r30(s: &scene) -> scene_rule30 {
    unsafe { s.u.rule30 }
}

/// The canvas's cell (c, r): the quadrants of generation rows 2r and 2r + 1, cells 2c and
/// 2c + 1, in the colour of the lower row where it has a pixel.
fn rule30_cell(t: &scene_rule30, c: usize, r: usize) -> (u8, u8) {
    let (a, b) = (2 * r, 2 * r + 1);
    let bit = |row: usize, x: usize| (t.row[row] >> x & 1) as u32;
    let q = bit(a, 2 * c) | bit(a, 2 * c + 1) << 1 | bit(b, 2 * c) << 2 | bit(b, 2 * c + 1) << 3;
    if q == 0 {
        return BLANK;
    }
    let fg = if q & 12 != 0 { t.color[b] } else { t.color[a] };
    ((FONT_Q0 + q) as u8, attr(u32::from(fg), GRID_BLACK))
}

fn assert_rule30_canvas(s: &scene) {
    let t = r30(s);
    for r in 0..ROWS {
        for c in 0..COLS {
            assert_eq!(cell(s, c, r), rule30_cell(&t, c, r), "cell ({c}, {r})");
        }
    }
}

#[test]
fn rule30_starts_with_its_pattern_in_grey_and_holds_still_in_silence() {
    let mut s = new_scene(SCENE_RULE30);
    let t = r30(&s);
    let mut x = 1u64 << 30;
    for g in 0..SCENE_RULE30_START as usize {
        x = unsafe { scene_rule30_next(x) };
        assert_eq!(t.row[g], x, "generation {g}");
        assert_eq!(u32::from(t.color[g]), GRID_GREY);
    }
    assert_eq!(usize::from(t.head), SCENE_RULE30_START as usize);
    assert_eq!(t.cur, x);
    assert_rule30_canvas(&s);
    let before = s.cv;
    for _ in 0..50 {
        step(&mut s, &quiet());
    }
    assert_eq!(changed(&before, &s.cv), 0, "silence moved it");
}

#[test]
fn rule30_steps_at_each_note_on_in_the_voices_colour() {
    let mut s = new_scene(SCENE_RULE30);
    let mut m = hit(quiet(), 1, 60, 0);
    m.color = [GRID_CYAN as u8, GRID_YELLOW as u8, GRID_LIGHT_GREEN as u8];
    let head = usize::from(r30(&s).head);
    let cur = r30(&s).cur;
    step(&mut s, &m);
    let t = r30(&s);
    assert_eq!(usize::from(t.head), head + 1);
    assert_eq!(t.row[head], unsafe { scene_rule30_next(cur) });
    assert_eq!(u32::from(t.color[head]), GRID_YELLOW);
    // The two rows ahead of the head are cleared.
    assert_eq!((t.row[head + 1], t.row[head + 2]), (0, 0));
    assert_rule30_canvas(&s);

    // Two voices' note-ons: two generations, each in its colour.
    let mut m2 = hit(hit(quiet(), 0, 60, 0), 2, 40, 0);
    m2.color = m.color;
    step(&mut s, &m2);
    let t = r30(&s);
    assert_eq!(usize::from(t.head), head + 3);
    assert_eq!(u32::from(t.color[head + 1]), GRID_CYAN);
    assert_eq!(u32::from(t.color[head + 2]), GRID_LIGHT_GREEN);
    // Voice 3's note-on flips the cell its pitch picks before its generation.
    let frame = 1usize; // the second step
    let flip = (40 * 7 + frame * 11) % W60;
    let before = unsafe { scene_rule30_next(t.row[head]) };
    assert_eq!(
        t.row[head + 2],
        unsafe { scene_rule30_next(t.row[head + 1] ^ 1 << flip) },
        "voice 3 flipped cell {flip}"
    );
    assert_ne!(t.row[head + 2], before);
    assert_rule30_canvas(&s);

    // A low cutoff darkens the colour.
    let mut dark = hit(quiet(), 1, 60, 0);
    dark.color = m.color;
    dark.cutoff = SCENE_DARK_CUTOFF as u16 - 1;
    step(&mut s, &dark);
    let t = r30(&s);
    assert_eq!(t.color[head + 3], unsafe { scene_dark(GRID_YELLOW as u8) });
    assert_ne!(u32::from(t.color[head + 3]), GRID_YELLOW);
}

#[test]
fn rule30_keeps_stepping_while_a_voice_sounds_and_stops_with_it() {
    let mut s = new_scene(SCENE_RULE30);
    let head = |s: &scene| usize::from(r30(s).head);
    let h0 = head(&s);
    // A full envelope held: a generation every other frame.
    for _ in 0..20 {
        step(&mut s, &held(quiet(), 0, 60, 255));
    }
    let steps = (head(&s) + W60 - h0) % W60;
    assert!(
        (9..=10).contains(&steps),
        "{steps} generations in 20 frames"
    );
    // Released, silent: no more.
    let h1 = head(&s);
    for _ in 0..20 {
        step(&mut s, &quiet());
    }
    assert_eq!(head(&s), h1);
    // Many note-ons in one frame step at most SCENE_RULE30_MAX_STEPS times.
    let mut all = quiet();
    for v in 0..3 {
        all = hit(all, v, 60, 255);
    }
    step(&mut s, &all);
    assert!((head(&s) + W60 - h1) % W60 <= SCENE_RULE30_MAX_STEPS as usize);
    // It wraps round the screen.
    for _ in 0..80 {
        step(&mut s, &hit(quiet(), 0, 60, 0));
    }
    assert_rule30_canvas(&s);
}

// ---- ANT ----

#[derive(Clone, Copy, Debug, PartialEq)]
struct Ant {
    x: usize,
    y: usize,
    d: usize,
}

/// Langton's ants on a 30 x 30 torus: on an empty cell turn right and paint it, on a painted
/// one turn left and clear it, then step forward.
struct AntModel {
    cell: [[u8; COLS]; ROWS],
    a: [Ant; 3],
    acc: [u32; 3],
}

impl AntModel {
    fn new() -> Self {
        AntModel {
            cell: [[0; COLS]; ROWS],
            a: [
                Ant { x: 9, y: 12, d: 0 },
                Ant { x: 20, y: 12, d: 1 },
                Ant { x: 15, y: 20, d: 2 },
            ],
            acc: [0; 3],
        }
    }

    fn walk(&mut self, v: usize, color: u8) {
        let a = &mut self.a[v];
        let c = &mut self.cell[a.y][a.x];
        if *c == 0 {
            a.d = (a.d + 1) % 4;
            *c = color + 1;
        } else {
            a.d = (a.d + 3) % 4;
            *c = 0;
        }
        let (dx, dy) = [(0, -1), (1, 0), (0, 1), (-1, 0)][a.d];
        a.x = (a.x as i32 + dx).rem_euclid(COLS as i32) as usize;
        a.y = (a.y as i32 + dy).rem_euclid(ROWS as i32) as usize;
    }

    fn frame(&mut self, m: &scene_music) {
        for v in 0..3 {
            let mut n = if m.trig[v] != 0 { SCENE_ANT_STEPS } else { 0 };
            if m.env[v] > 0 {
                self.acc[v] += u32::from(m.env[v]);
                while self.acc[v] >= SCENE_ENV_STEP {
                    self.acc[v] -= SCENE_ENV_STEP;
                    n += 1;
                }
            } else {
                self.acc[v] = 0;
            }
            let color = if u32::from(m.cutoff) < SCENE_DARK_CUTOFF {
                unsafe { scene_dark(m.color[v]) }
            } else {
                m.color[v]
            };
            for _ in 0..n {
                self.walk(v, color);
            }
        }
    }
}

fn ant(s: &scene) -> scene_ant {
    unsafe { s.u.ant }
}

#[test]
fn the_ants_follow_langtons_rules() {
    let mut s = new_scene(SCENE_ANT);
    let mut model = AntModel::new();
    let mut rng = 12345u32;
    for f in 0..600u32 {
        let mut m = quiet();
        m.color = [GRID_CYAN as u8, GRID_YELLOW as u8, GRID_LIGHT_GREEN as u8];
        m.cutoff = if f % 200 < 100 { 1200 } else { 300 };
        for v in 0..3 {
            let r = unsafe { scene_random(&mut rng) };
            if r % 7 == 0 {
                m = hit(m, v, 60, 0);
            }
            if (f / 50 + v as u32).is_multiple_of(3) {
                m.env[v] = (r >> 8) as u8 | 1;
            }
        }
        step(&mut s, &m);
        model.frame(&m);
        let a = ant(&s);
        for v in 0..3 {
            let w = a.a[v];
            assert_eq!(
                Ant {
                    x: w.x.into(),
                    y: w.y.into(),
                    d: w.d.into()
                },
                model.a[v],
                "frame {f}, ant {v}"
            );
        }
        assert_eq!(a.cell, model.cell, "frame {f}");
        // The canvas: a pip in the trail's colour, the ants as balls on top (a silent voice's
        // grey), blank elsewhere.
        for r in 0..ROWS {
            for c in 0..COLS {
                let k = a.cell[r][c];
                let want = if k == 0 {
                    BLANK
                } else {
                    (FONT_PIP as u8, attr(u32::from(k - 1), GRID_BLACK))
                };
                let on = (0..3)
                    .rev()
                    .find(|&v| usize::from(a.a[v].x) == c && usize::from(a.a[v].y) == r);
                let want = match on {
                    Some(v) => {
                        let fg = if m.env[v] > 0 {
                            GRID_WHITE
                        } else {
                            GRID_DARK_GREY
                        };
                        (FONT_BALL as u8, attr(fg, GRID_BLACK))
                    }
                    None => want,
                };
                assert_eq!(cell(&s, c, r), want, "frame {f}, cell ({c}, {r})");
            }
        }
    }
}

#[test]
fn the_ants_start_again_after_scene_ant_reset_steps() {
    let mut s = new_scene(SCENE_ANT);
    let mut m = quiet();
    for v in 0..3 {
        m = held(m, v, 60, 255);
    }
    let mut most = 0;
    let mut restarted = false;
    for _ in 0..10_000 {
        let before = ant(&s).steps;
        step(&mut s, &m);
        most = most.max(ant(&s).steps);
        if ant(&s).steps < before {
            restarted = true;
            break;
        }
    }
    assert!(restarted, "the ants never started again");
    assert!(u32::from(most) < SCENE_ANT_RESET);
}

// ---- STARS ----

fn stars(s: &scene) -> scene_stars {
    unsafe { s.u.stars }
}

#[test]
fn the_stars_fly_out_faster_with_the_envelope_and_steer_with_the_pitch() {
    let mut s = new_scene(SCENE_STARS);
    let z = |s: &scene| -> Vec<i32> { stars(s).st.iter().map(|p| i32::from(p.z)).collect() };
    // In silence each star comes closer by the cruising speed, the cutoff's.
    let cruise = |cutoff: u32| 8 + cutoff as i32 * 24 / 2047;
    for cutoff in [0u16, 1200, 2047] {
        let before = z(&s);
        let mut m = quiet();
        m.cutoff = cutoff;
        step(&mut s, &m);
        for (i, (&b, &a)) in before.iter().zip(&z(&s)).enumerate() {
            if a <= b {
                assert_eq!(
                    b - a,
                    cruise(u32::from(cutoff)),
                    "star {i}, cutoff {cutoff}"
                );
            } else {
                assert_eq!(a, 4096, "star {i} starts again at the far end");
            }
        }
    }
    // A sounding voice speeds its third of the stars.
    let before = z(&s);
    step(&mut s, &held(quiet(), 1, 60, 255));
    for (i, (&b, &a)) in before.iter().zip(&z(&s)).enumerate() {
        if a <= b {
            let boost = if i % 3 == 1 { 255 * 180 / 256 } else { 0 };
            assert_eq!(b - a, cruise(1200) + boost, "star {i}");
        }
    }
    // Every star stays in its box, deeper than the near plane.
    for p in stars(&s).st {
        assert!(
            (-768..=768).contains(&p.x) && (-768..=768).contains(&p.y),
            "{p:?}"
        );
        assert!((205..=4096).contains(&p.z), "{p:?}");
    }
    // The centre steers toward the latest note: right of the middle for high notes.
    for _ in 0..40 {
        step(&mut s, &hit(quiet(), 2, 80, 200));
    }
    assert_eq!(stars(&s).cx, 80, "steered to (80 - 60) * 4");
    for _ in 0..40 {
        step(&mut s, &hit(quiet(), 0, 40, 200));
    }
    assert_eq!(stars(&s).cx, -80);
}

/// A note-on lights its voice's stars even before its envelope rises (the tap's ENV3 is still
/// 0 when the note-on comes before the audio has run a block): the light goes early only once
/// the voice is let go and silent.
#[test]
fn a_note_on_lights_the_stars_before_its_envelope_rises() {
    let lit = SCENE_STARS_LIT as u8;
    let mut s = new_scene(SCENE_STARS);
    let mut m = hit(quiet(), 0, 60, 0);
    m.color = [GRID_CYAN as u8, GRID_YELLOW as u8, GRID_YELLOW as u8];
    step(&mut s, &m);
    assert_eq!(
        stars(&s).lit[0],
        lit,
        "the note-on's light went with ENV3 at 0"
    );
    let cyan = |s: &scene| {
        (0..ROWS).any(|r| (0..COLS).any(|c| u32::from(s.cv.attr[r][c] & 15) == GRID_CYAN))
    };
    assert!(cyan(&s), "no star in the voice's colour");
    m.trig = [0; 3];
    step(&mut s, &m); // held, ENV3 still 0
    assert_eq!(stars(&s).lit[0], lit - 1);
    m.gate = [0; 3];
    m.env[0] = 30;
    step(&mut s, &m); // let go, still sounding
    assert_eq!(stars(&s).lit[0], lit - 2);
    m.env[0] = 0;
    step(&mut s, &m); // let go and silent
    assert_eq!(stars(&s).lit[0], 0);
    assert!(!cyan(&s));
}

/// The canvas from the stars as the piece projects them.
fn star_canvas(s: &scene, m: &scene_music) -> Vec<(usize, usize, u8, u8)> {
    let t = stars(s);
    let mut out = Vec::new();
    let near = |z: u16| match z {
        3073.. => 0,
        1844..=3072 => 1,
        902..=1843 => 2,
        _ => 3,
    };
    for layer in 0..4 {
        for (i, p) in t.st.iter().enumerate() {
            if near(p.z) != layer {
                continue;
            }
            let sx = 120 + i32::from(t.cx) + i32::from(p.x) * 640 / i32::from(p.z);
            let sy = 120 + i32::from(p.y) * 640 / i32::from(p.z);
            if !(0..240).contains(&sx) || !(0..240).contains(&sy) {
                continue;
            }
            let glyph = [FONT_TINY, FONT_DOT, FONT_PLUS, FONT_STAR][layer] as u8;
            let v = i % 3;
            let fg = if t.lit[v] > 0 {
                u32::from(m.color[v])
            } else {
                [GRID_GREY, GRID_LIGHT_GREY, GRID_WHITE, GRID_WHITE][layer]
            };
            out.push((
                sx as usize / 8,
                sy as usize / 8,
                glyph,
                attr(fg, GRID_BLACK),
            ));
        }
    }
    out
}

#[test]
fn the_stars_are_drawn_far_to_near_and_light_with_their_voice() {
    let mut s = new_scene(SCENE_STARS);
    let mut m = quiet();
    m.color = [GRID_CYAN as u8, GRID_YELLOW as u8, GRID_LIGHT_GREEN as u8];
    for f in 0..60 {
        let m = if f == 10 {
            hit(m, 1, 60, 200)
        } else if f > 10 && f < 30 {
            held(m, 1, 60, 200)
        } else {
            m
        };
        step(&mut s, &m);
        let mut want = [[BLANK; COLS]; ROWS];
        for (c, r, g, a) in star_canvas(&s, &m) {
            want[r][c] = (g, a);
        }
        for (r, row) in want.iter().enumerate() {
            for (c, &w) in row.iter().enumerate() {
                assert_eq!(cell(&s, c, r), w, "frame {f}, cell ({c}, {r})");
            }
        }
        let lit = stars(&s).lit[1];
        if (10..10 + SCENE_STARS_LIT).contains(&f) {
            assert!(lit > 0, "frame {f}: voice 2's stars are not lit");
        } else {
            assert_eq!(lit, 0, "frame {f}");
        }
    }
}

// ---- FIRE ----

fn fire(s: &scene) -> scene_fire {
    unsafe { s.u.fire }
}

const FIRE_TOP: usize = ROWS - SCENE_FIRE_ROWS as usize;
/// The fire's ramp, coolest first: brown only in the ember bed, and no orange (olive-brown on
/// this palette).
const FIRE_COLORS: [u32; 6] = [
    GRID_BLACK,
    GRID_BROWN,
    GRID_RED,
    GRID_LIGHT_RED,
    GRID_YELLOW,
    GRID_WHITE,
];
const FIRE_WHITE: usize = 5;

/// The fire's level (0..5) of a cell row's half: top or bottom.
fn fire_levels(s: &scene) -> Vec<Vec<usize>> {
    (FIRE_TOP..ROWS)
        .flat_map(|r| {
            let (fg, bg): (Vec<usize>, Vec<usize>) = (0..COLS)
                .map(|c| {
                    let (ch, a) = cell(s, c, r);
                    assert_eq!(u32::from(ch), FONT_Q3, "cell ({c}, {r})");
                    let lv = |k: u8| FIRE_COLORS.iter().position(|&x| x == u32::from(k)).unwrap();
                    (lv(a & 15), lv(a >> 4))
                })
                .unzip();
            [fg, bg]
        })
        .collect()
}

/// How many heat rows (half cells) up from the bottom the flame of voice v reaches: the
/// highest with a cell of level 3 (light red) or more.
fn flame_height(s: &scene, v: usize) -> usize {
    let lv = fire_levels(s);
    let h = lv.len();
    (0..h)
        .find(|&y| (10 * v..10 * v + 10).any(|x| lv[y][x] >= 3))
        .map_or(0, |y| h - y)
}

#[test]
fn the_fire_burns_in_its_rows_with_each_voices_envelope() {
    let mut s = new_scene(SCENE_FIRE);
    // The rows above it stay blank.
    let check_above = |s: &scene| {
        for r in 0..FIRE_TOP {
            for c in 0..COLS {
                assert_eq!(cell(s, c, r), BLANK, "cell ({c}, {r})");
            }
        }
    };
    check_above(&s);
    // In silence only the idle flame burns, low and red: no light red anywhere.
    for _ in 0..60 {
        step(&mut s, &quiet());
    }
    assert_eq!((0..3).map(|v| flame_height(&s, v)).max(), Some(0));
    // Voice 2 held at full envelope: its flame, not the others'.
    let m = held(quiet(), 1, 60, 255);
    for _ in 0..60 {
        step(&mut s, &m);
    }
    check_above(&s);
    let h = [0, 1, 2].map(|v| flame_height(&s, v));
    assert!(h[1] >= 8, "voice 2's flame: {h:?}");
    assert_eq!((h[0], h[2]), (0, 0), "{h:?}");
    // Higher notes burn higher.
    let mut tall = new_scene(SCENE_FIRE);
    let mut short = new_scene(SCENE_FIRE);
    let (mut sum_tall, mut sum_short) = (0, 0);
    for _ in 0..100 {
        step(&mut tall, &held(quiet(), 0, 84, 255));
        step(&mut short, &held(quiet(), 0, 36, 255));
        sum_tall += flame_height(&tall, 0);
        sum_short += flame_height(&short, 0);
    }
    assert!(
        sum_tall > sum_short * 5 / 4,
        "{sum_tall} against {sum_short}"
    );
    // Released, the flame dies back to the embers.
    for _ in 0..100 {
        step(&mut s, &quiet());
    }
    assert_eq!(flame_height(&s, 1), 0);
}

/// A silent fire is not dark: its flames fall to FIRE_IDLE_HOT and flicker a few cell rows
/// high in red, and never reach light red or the cell rows above them.
#[test]
fn a_silent_fire_keeps_a_low_flame() {
    let mut s = new_scene(SCENE_FIRE);
    let (mut flickers, mut tallest) = (0usize, 0usize);
    for f in 0..200 {
        step(&mut s, &quiet());
        if f < 60 {
            continue;
        }
        let lv = fire_levels(&s);
        // The half cells at level 2 (red) or more above the bed; the highest one's height.
        let red = |y: usize| lv[y].iter().filter(|&&l| l >= 2).count();
        flickers += (0..lv.len() - 1).map(red).sum::<usize>();
        let top = (0..lv.len())
            .find(|&y| red(y) > 0)
            .map_or(0, |y| lv.len() - y);
        tallest = tallest.max(top);
        assert!(lv.iter().flatten().all(|&l| l <= 2), "frame {f}: above red");
    }
    println!("idle fire: {flickers} red half cells above the bed in 140 frames, up to {tallest}");
    // On average a few half cells a column (a cell row or two), now and then a tongue to
    // about eight cell rows, nothing taller.
    let mean = flickers as f64 / (140.0 * COLS as f64);
    assert!(
        (2.0..=10.0).contains(&mean),
        "{mean} red half cells a column"
    );
    assert!(
        (8..=20).contains(&tallest),
        "the idle flame is {tallest} half cells high"
    );
    // Each voice's hot value rests at the idle level.
    assert!(fire(&s).hot.iter().all(|&h| h == 120), "{:?}", fire(&s).hot);
}

#[test]
fn the_cutoff_sets_how_hot_the_fire_burns() {
    let hottest = |cutoff: u16| {
        let mut s = new_scene(SCENE_FIRE);
        let mut most = 0;
        for _ in 0..80 {
            let mut m = held(quiet(), 0, 60, 255);
            m.cutoff = cutoff;
            step(&mut s, &m);
            most = most.max(fire_levels(&s).iter().flatten().copied().max().unwrap());
        }
        most
    };
    assert!(hottest(0) <= 3, "a closed filter burns at {}", hottest(0));
    assert_eq!(hottest(2047), FIRE_WHITE, "an open filter burns white");
    assert_eq!(
        hottest(1200),
        FIRE_WHITE,
        "the preset's cutoff burns white at the bed"
    );
    // The cutoff moves the top of the ramp, not the heat: the flames burn the same.
    let heat = |cutoff: u16| {
        let mut s = new_scene(SCENE_FIRE);
        for _ in 0..40 {
            let mut m = held(quiet(), 0, 60, 255);
            m.cutoff = cutoff;
            step(&mut s, &m);
        }
        fire(&s).heat
    };
    assert_eq!(heat(0), heat(2047));
}

/// The ramp: brown only in the ember bed (the bottom heat row), and there only where it just
/// glows; yellow a thin band between light red and white; the hottest near the bed; and the
/// flames break into tongues (columns of differing height), not one mound.
#[test]
fn the_fire_burns_up_its_ramp_in_tongues() {
    let mut s = new_scene(SCENE_FIRE);
    for _ in 0..30 {
        step(&mut s, &quiet());
    }
    let lv = fire_levels(&s);
    let bed = lv.len() - 1;
    assert!(
        lv[bed].iter().all(|&l| l == 2),
        "the bed in silence burns red (the idle flame): {:?}",
        lv[bed]
    );
    let mut count = [0usize; 6];
    let mut heights = Vec::new();
    for f in 0..200 {
        let mut m = quiet();
        m.cutoff = 2047;
        for v in 0..3 {
            m = held(m, v, 48 + 12 * v as u8, 230);
        }
        step(&mut s, &m);
        let lv = fire_levels(&s);
        for (y, row) in lv.iter().enumerate() {
            for &l in row {
                count[l] += 1;
                assert!(l != 1 || y == bed, "brown above the bed, frame {f}");
            }
        }
        // Each column's flame top (level 2, red, or more).
        heights
            .extend((0..COLS).map(|x| (0..lv.len()).find(|&y| lv[y][x] >= 2).unwrap_or(lv.len())));
    }
    println!("levels {count:?}");
    assert!(
        count[4] > 0 && count[4] < count[3] / 2,
        "yellow is a thin band: {count:?}"
    );
    assert!(count[5] > 0, "no white: {count:?}");
    let mean = heights.iter().sum::<usize>() as f64 / heights.len() as f64;
    let spread = (heights
        .iter()
        .map(|&h| (h as f64 - mean).powi(2))
        .sum::<f64>()
        / heights.len() as f64)
        .sqrt();
    println!("flame tops: mean {mean:.1} half cells down, spread {spread:.1}");
    assert!(
        spread >= 3.0,
        "the flames are one mound: spread {spread:.1}"
    );
}

#[test]
fn the_fire_steps_every_other_frame_and_draws_half_its_rows_a_frame() {
    let mut s = new_scene(SCENE_FIRE);
    let m = held(quiet(), 0, 60, 255);
    for f in 0..40u32 {
        let heat = fire(&s).heat;
        let before = s.cv;
        step(&mut s, &m);
        let stepped = fire(&s).heat != heat;
        assert_eq!(stepped, f % 2 == 0, "frame {f}");
        for r in FIRE_TOP..ROWS {
            let row_changed = (0..COLS)
                .any(|c| before.ch[r][c] != s.cv.ch[r][c] || before.attr[r][c] != s.cv.attr[r][c]);
            if row_changed {
                assert_eq!((r - FIRE_TOP) % 2, f as usize % 2, "frame {f} drew row {r}");
            }
        }
    }
}

// ---- PLASMA ----

const PLASMA_RAMP: [u32; 6] = [
    GRID_BLACK,
    GRID_BLUE,
    GRID_PURPLE,
    GRID_LIGHT_BLUE,
    GRID_CYAN,
    GRID_WHITE,
];

/// The plasma level (0..19) of each cell: a dither glyph over two neighbours of the ramp.
fn plasma_levels(s: &scene) -> Vec<usize> {
    let mut out = Vec::new();
    for r in 0..ROWS {
        for c in 0..COLS {
            let (ch, a) = cell(s, c, r);
            let dn = (u32::from(ch) - FONT_D0) as usize;
            assert!(
                dn.is_multiple_of(4) && dn <= 12,
                "cell ({c}, {r}) glyph {ch:#x}"
            );
            let pr = PLASMA_RAMP
                .iter()
                .position(|&x| x == u32::from(a >> 4))
                .unwrap();
            assert_eq!(u32::from(a & 15), PLASMA_RAMP[pr + 1], "cell ({c}, {r})");
            out.push(pr * 4 + dn / 4);
        }
    }
    out
}

#[test]
fn the_plasma_fills_the_screen_and_the_cutoff_speeds_it() {
    let moved = |cutoff: u16, env: u8| {
        let mut s = new_scene(SCENE_PLASMA);
        let mut total = 0;
        for _ in 0..50 {
            let before = s.cv;
            let mut m = held(quiet(), 0, 60, env);
            m.cutoff = cutoff;
            step(&mut s, &m);
            plasma_levels(&s);
            total += changed(&before, &s.cv);
        }
        total
    };
    let slow = moved(200, 0);
    let fast = moved(2047, 0);
    let loud = moved(200, 255);
    println!(
        "plasma cells changed in 50 frames: cutoff 200 {slow}, 2047 {fast}, 200 and loud {loud}"
    );
    assert!(fast > 3 * slow, "the cutoff: {fast} against {slow}");
    assert!(loud > 2 * slow, "the envelope: {loud} against {slow}");
    // Levels from the bottom of the ramp to the top.
    let s = new_scene(SCENE_PLASMA);
    let lv = plasma_levels(&s);
    assert!(
        lv.iter().min() <= Some(&4) && lv.iter().max() >= Some(&14),
        "{lv:?}"
    );
}

#[test]
fn the_plasma_draws_every_other_row_a_frame() {
    let mut s = new_scene(SCENE_PLASMA);
    for f in 0..20usize {
        let before = s.cv;
        let mut m = held(quiet(), 0, 60, 255);
        m.cutoff = 2047;
        step(&mut s, &m);
        for r in 0..ROWS {
            let row_changed = (0..COLS)
                .any(|c| before.ch[r][c] != s.cv.ch[r][c] || before.attr[r][c] != s.cv.attr[r][c]);
            if row_changed {
                assert_eq!(r % 2, f % 2, "frame {f} drew row {r}");
            }
        }
    }
}

#[test]
fn a_note_on_drops_a_ripple_where_its_pitch_says() {
    for (note, x, y) in [(72u8, 3u8, 15u8), (60, 3, 21), (83, 25, 10), (127, 17, 0)] {
        let mut s = new_scene(SCENE_PLASMA);
        step(&mut s, &hit(quiet(), 2, note, 200));
        let r = unsafe { s.u.plasma }.rip[2];
        assert_eq!((r.x, r.y, r.age), (x, y, 1), "note {note}");
        // A ring grows from there: brighter than the plasma without it, half a cell a frame.
        let mut plain = new_scene(SCENE_PLASMA);
        step(&mut plain, &held(quiet(), 2, note, 200));
        for _ in 0..7 {
            step(&mut s, &held(quiet(), 2, note, 200));
            step(&mut plain, &held(quiet(), 2, note, 200));
        }
        let (a, b) = (plasma_levels(&s), plasma_levels(&plain));
        let ring: Vec<usize> = (0..COLS * ROWS).filter(|&i| a[i] != b[i]).collect();
        assert!(ring.len() > 15, "note {note}: no ripple");
        for &i in &ring {
            let (c, r) = ((i % COLS) as f64, (i / COLS) as f64);
            let d = (c - f64::from(x)).hypot(r - f64::from(y));
            assert!(a[i] > b[i], "note {note}: cell ({c}, {r}) darker");
            assert!(
                (2.0..=6.5).contains(&d),
                "note {note}: cell ({c}, {r}) {d:.1} cells out"
            );
        }
    }
    // It fades out.
    let mut s = new_scene(SCENE_PLASMA);
    step(&mut s, &hit(quiet(), 0, 72, 200));
    for _ in 0..SCENE_RIPPLE_FRAMES {
        step(&mut s, &held(quiet(), 0, 72, 200));
    }
    assert_eq!(
        u32::from(unsafe { s.u.plasma }.rip[0].age),
        SCENE_RIPPLE_FRAMES
    );
}

/// The plasma's distance table, generated into app/scene.c: round(4 hypot(dx, dy)).
#[test]
fn the_distance_table_is_four_times_the_distance() {
    let table = unsafe { scene_dist4 };
    for (dy, row) in table.iter().enumerate() {
        for (dx, &d) in row.iter().enumerate() {
            let want = (4.0 * (dx as f64).hypot(dy as f64)).round() as u8;
            assert_eq!(d, want, "({dx}, {dy})");
        }
    }
}

// ---- every piece ----

/// The plasma's whole-turn sine is sine_q7, byte for byte.
#[test]
fn the_plasmas_sine_table_is_sine_q7() {
    for phase in 0..256u32 {
        let want = unsafe { sine_q7(phase) };
        assert_eq!(
            i32::from(unsafe { scene_sin8[phase as usize] }),
            want,
            "phase {phase}"
        );
    }
}

#[test]
fn a_piece_starts_and_plays_the_same_every_time() {
    for piece in 0..SCENE_PETSCII {
        let mut a = new_scene(piece);
        let mut b = new_scene(piece);
        assert_eq!(changed(&a.cv, &b.cv), 0, "piece {piece}");
        let mut rng = 7u32;
        for f in 0..200 {
            let r = unsafe { scene_random(&mut rng) };
            let m = if r % 5 == 0 {
                hit(quiet(), (r % 3) as usize, 48 + (r >> 8) as u8 % 36, 220)
            } else {
                held(quiet(), 0, 60, (r >> 16) as u8)
            };
            step(&mut a, &m);
            step(&mut b, &m);
            assert_eq!(changed(&a.cv, &b.cv), 0, "piece {piece}, frame {f}");
        }
        // scene_start brings it back to its first frame.
        unsafe { scene_start(&mut *a, piece, &quiet()) };
        assert_eq!(changed(&a.cv, &new_scene(piece).cv), 0, "piece {piece}");
    }
    // Pieces wrap, among the five drawn in cells.
    assert_eq!(new_scene(SCENE_PETSCII + 2).piece, 2);
}

/// A piece begun (scene_begin) and drawn a wipe's rows at a time (scene_fill) is the piece
/// started whole (scene_start), and plays on the same; PLASMA, the costliest start, draws no
/// row before it is asked for it. A step draws what is left first.
#[test]
fn a_piece_begun_and_filled_by_rows_is_the_piece_started_whole() {
    let mut m = quiet();
    m.resonance = 11;
    m.cutoff = 1900;
    for piece in 0..SCENE_PETSCII {
        let mut whole = Box::<scene>::default();
        unsafe { scene_start(&mut *whole, piece, &m) };
        let mut part = Box::<scene>::default();
        part.cv.ch = [[b'X'; COLS]; ROWS];
        unsafe { scene_begin(&mut *part, piece) };
        if piece == SCENE_PLASMA {
            assert_eq!(part.filled, 0);
            assert!(part.cv.ch.iter().all(|r| r.iter().all(|&c| c == b'X')));
        }
        for rows in (4..ROWS).step_by(4).chain([ROWS]) {
            unsafe { scene_fill(&mut *part, &m, rows as u32) };
            for r in 0..rows {
                assert_eq!(part.cv.ch[r], whole.cv.ch[r], "piece {piece}, row {r}");
                assert_eq!(part.cv.attr[r], whole.cv.attr[r], "piece {piece}, row {r}");
            }
        }
        assert_eq!(u32::from(part.filled), GRID_ROWS);
        let mut lazy = Box::<scene>::default();
        unsafe { scene_begin(&mut *lazy, piece) };
        for f in 0..20 {
            let mf = if f % 5 == 0 {
                hit(m, f % 3, 60 + f as u8, 200)
            } else {
                m
            };
            step(&mut part, &mf);
            step(&mut lazy, &mf);
            step(&mut whole, &mf);
            assert_eq!(changed(&part.cv, &whole.cv), 0, "piece {piece}, frame {f}");
            assert_eq!(changed(&lazy.cv, &whole.cv), 0, "piece {piece}, frame {f}");
        }
    }
}

// ---- the view ----

fn piece_name(k: u32) -> String {
    unsafe { CStr::from_ptr(scene_names[k as usize]) }
        .to_string_lossy()
        .into_owned()
}

/// The menu, then SCENE, settled.
pub(crate) fn open_scene(ui: &mut Ui) {
    ui.press(PANEL_BTN_HOME);
    ui.turn(PANEL_ENC_SELECT, -(VIEWS_MENU_ITEMS as i32));
    ui.turn(PANEL_ENC_SELECT, VIEWS_MENU_SCENE as i32);
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
}

/// The grid's composed cells as the piece has them: the canvas's, or RED MOON RITE's blank
/// cells over its painter region.
fn grid_is_canvas(ui: &Ui) -> bool {
    if u32::from(ui.v.scene.which) == SCENE_RMR {
        return ui.g.region_painter.is_some()
            && (0..ROWS * COLS).all(|i| ui.g.ch[i] == b' ' && ui.g.attr[i] == 0);
    }
    let cv = &petscii(&ui.v.scene).cv;
    (0..ROWS * COLS).all(|i| {
        let (r, c) = (i / COLS, i % COLS);
        ui.g.ch[i] == cv.ch[r][c] && ui.g.attr[i] == cv.attr[r][c]
    })
}

#[test]
fn scene_opens_from_the_menu_and_select_steps_the_pieces() {
    let mut ui = Ui::without_cards();
    open_scene(&mut ui);
    assert_eq!(ui.top(), "SCENE");
    assert_eq!(u32::from(ui.v.scene.which), SCENE_FIRE);
    assert_eq!(u32::from(petscii(&ui.v.scene).piece), SCENE_FIRE);
    assert_eq!(ui.toast().as_deref(), Some("FIRE"), "the name on entry");
    for _ in 0..40 {
        ui.frame(); // the toast gone
        drain(&mut ui.g);
    }
    assert!(ui.toast().is_none());
    assert!(grid_is_canvas(&ui), "the grid is not the fire's canvas");
    // SELECT: the next piece wipes in, and a toast names it; round and back.
    for k in 1..=SCENE_PIECES {
        ui.turn(PANEL_ENC_SELECT, 1);
        assert_eq!(u32::from(ui.v.scene.which), k % SCENE_PIECES);
        ui.frame();
        if k % SCENE_PIECES < SCENE_PETSCII {
            assert_eq!(u32::from(petscii(&ui.v.scene).piece), k % SCENE_PIECES);
        }
        assert_ne!(ui.v.s.wipe, 0, "piece {k}: no wipe");
        assert_eq!(
            ui.toast().as_deref(),
            Some(piece_name(k % SCENE_PIECES).as_str())
        );
        ui.settle();
        if k % SCENE_PIECES == SCENE_RMR {
            for _ in 0..40 {
                ui.frame(); // the toast gone
                drain(&mut ui.g);
            }
            assert!(grid_is_canvas(&ui), "RED MOON RITE's cells are not blank");
        }
    }
    ui.turn(PANEL_ENC_SELECT, -2);
    assert_eq!(u32::from(ui.v.scene.which), SCENE_PIECES - 2);
    // Every other control plays: K1 the cutoff, the octave.
    let cutoff = ui.live().cutoff;
    ui.turn(PANEL_ENC_K1, 3);
    assert_eq!(ui.live().cutoff, cutoff_after(ui.live().model, cutoff, 3));
    assert!(ui.live().cutoff > cutoff);
    ui.press(PANEL_BTN_OCT_UP);
    assert_eq!(ui.live().octave, 1);
    // HOME goes back; SCENE comes back with the piece it showed, from its start.
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
    ui.settle();
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert_eq!(ui.top(), "SCENE");
    assert_eq!(u32::from(petscii(&ui.v.scene).piece), SCENE_PIECES - 2);
}

/// ALGORITHM steps the piece exactly as SELECT does: the same piece, the wipe, the name.
#[test]
fn algorithm_steps_the_piece_as_select_does() {
    let step_with = |enc: u32, steps: i32| {
        let mut ui = Ui::without_cards();
        open_scene(&mut ui);
        for _ in 0..40 {
            ui.frame();
        }
        ui.turn(enc, steps);
        ui.frame();
        let text = ui.toast();
        let wiping = ui.v.s.wipe;
        (u32::from(ui.v.scene.which), text, wiping)
    };
    for steps in [1, 2, -1, -3] {
        let by_select = step_with(PANEL_ENC_SELECT, steps);
        let by_algorithm = step_with(PANEL_ENC_ALGORITHM, steps);
        assert_eq!(by_select, by_algorithm, "{steps} steps");
        assert_ne!(by_algorithm.2, 0, "{steps} steps: no wipe");
        assert_eq!(
            by_algorithm.1.as_deref(),
            Some(piece_name(by_algorithm.0).as_str())
        );
    }
}

/// The piece's name is toasted once, when SCENE first shows, and again at each step, never
/// twice for one entry.
#[test]
fn entering_scene_names_the_piece_once() {
    let mut ui = Ui::without_cards();
    open_scene(&mut ui);
    assert_eq!(ui.toast().as_deref(), Some("FIRE"));
    assert_eq!(ui.v.scene.say, 0);
    for _ in 0..60 {
        ui.frame();
    }
    assert!(ui.toast().is_none(), "the toast ends and is not said again");
}

#[test]
fn the_tap_runs_while_scene_shows() {
    let mut ui = Ui::without_cards();
    assert_eq!(ui.sc.on, 1, "LIVE");
    ui.press(PANEL_BTN_HOME);
    ui.frame();
    assert_eq!(ui.sc.on, 0, "the menu");
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    open_scene(&mut ui);
    assert_eq!(ui.sc.on, 1, "the tap is off under SCENE");
    assert_eq!(ui.v.scene.on, 1);
    assert_eq!(ui.v.live.on, 0, "LIVE's use of the tap while SCENE shows");
    // Under MIDI learn's frame too.
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *ui.v, &mut *ml);
    }
    ui.learn_on(PANEL_ENC_K1);
    ui.frame();
    assert_eq!(ui.top(), "LEARN");
    assert_eq!(ui.sc.on, 1, "the tap is off under learn's frame over SCENE");
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    assert_eq!(ui.top(), "SCENE");
    ui.press(PANEL_BTN_HOME);
    ui.frame();
    assert_eq!((ui.sc.on, ui.v.scene.on), (0, 0), "the tap stayed on");
}

/// A band for the pieces: an arpeggio every 4 frames, a bass every 8 and a high note every 16,
/// each released a few frames later, and a simple envelope a voice standing in for the chip's
/// (the tap's ENV3, which the host's views read from the Ui's scope).
struct Band {
    env: [u8; 3],
    frame: u32,
    playing: Vec<(u8, u32)>, // a note and the frame it ends
}

impl Band {
    fn new() -> Self {
        Band {
            env: [0; 3],
            frame: 0,
            playing: Vec::new(),
        }
    }

    /// The next frame's notes and envelopes, then a frame and what it sent.
    fn frame(&mut self, ui: &mut Ui, playing: bool) -> Sent {
        const CHORDS: [[u8; 3]; 4] = [[57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]];
        let f = self.frame;
        self.frame += 1;
        let ch = CHORDS[(f / 64 % 4) as usize];
        let mut on = |note: u8, len: u32| {
            ui.rig.note_on(note);
            self.playing.push((note, f + len));
        };
        if playing && f.is_multiple_of(4) {
            on(ch[(f / 4 % 3) as usize] + 12, 3);
        }
        if playing && f.is_multiple_of(8) {
            on(ch[0] - 12, 6);
        }
        if playing && f % 16 == 2 {
            on(ch[2] + 24, 2);
        }
        for &(note, end) in &self.playing {
            if end == f || !playing {
                ui.rig.note_off(note);
            }
        }
        self.playing.retain(|&(_, end)| playing && end > f);
        for v in 0..3 {
            let gate = ui.live().voice[v].gate != 0;
            self.env[v] = if gate {
                self.env[v].saturating_add(96).min(230)
            } else {
                (u32::from(self.env[v]) * 3 / 4) as u8
            };
            ui.sc.env[v] = self.env[v];
        }
        ui.frame();
        drain(&mut ui.g)
    }
}

/// Opens SCENE on `piece`, settled, everything sent; the title cards off (title_card.rs).
fn scene_on(piece: u32) -> Ui {
    let mut ui = Ui::without_cards();
    open_scene(&mut ui);
    for _ in 0..piece {
        ui.turn(PANEL_ENC_SELECT, 1);
        ui.settle();
    }
    // The cutoff wide open: the busiest plasma and the hottest fire.
    ui.turn(PANEL_ENC_K1, 64);
    // The knob's popup (app/screen.h) opens over the piece and closes, which restores the cells
    // under it a few frames at a time as a piece does: sent frame by frame as the panel takes
    // them.
    for _ in 0..80 {
        ui.frame();
        drain(&mut ui.g);
    }
    ui.v.scene.held = 0;
    ui
}

#[test]
fn every_piece_stays_within_the_budget_while_the_band_plays() {
    for piece in 0..SCENE_PIECES {
        let mut ui = scene_on(piece);
        let mut band = Band::new();
        let sent: Vec<Sent> = (0..400).map(|_| band.frame(&mut ui, true)).collect();
        let n = sent.len() as f64;
        let avg = sent.iter().map(|s| s.cells).sum::<usize>() as f64 / n;
        let worst = sent.iter().copied().max_by_key(|s| s.bytes()).unwrap();
        let most = sent.iter().map(|s| s.cells).max().unwrap();
        println!(
            "{}: {avg:.1} cells a frame, at most {most}; worst frame {worst:?} {:.2} ms; {} \
             frames held by the budget",
            piece_name(piece),
            worst.spi_ms(),
            ui.v.scene.held
        );
        assert!(
            most <= SCENE_VIEW_FRAME_CELLS as usize,
            "{}: a frame sent {most} cells",
            piece_name(piece)
        );
        assert!(worst.spi_ms() <= SPI_BUDGET_MS, "{}", piece_name(piece));
        // A band playing moves every piece.
        assert!(avg >= 2.0, "{}: {avg:.1} cells a frame", piece_name(piece));
    }
}

#[test]
fn a_note_moves_the_scene_and_silence_calms_it() {
    for piece in 0..SCENE_PIECES {
        let mut ui = scene_on(piece);
        let mut band = Band::new();
        let quiet: usize = (0..50).map(|_| band.frame(&mut ui, false).cells).sum();
        let playing: usize = (0..50).map(|_| band.frame(&mut ui, true).cells).sum();
        for _ in 0..100 {
            band.frame(&mut ui, false);
        }
        let calmed: usize = (0..50).map(|_| band.frame(&mut ui, false).cells).sum();
        println!(
            "{}: cells in 50 frames quiet {quiet}, playing {playing}, after {calmed}",
            piece_name(piece)
        );
        // FIRE's idle flame flickers in silence (about a third of what the band moves).
        let times = if piece == SCENE_FIRE { 2 } else { 3 };
        assert!(
            playing > times * quiet.max(10) && playing > times * calmed.max(10),
            "{}: quiet {quiet}, playing {playing}, after {calmed}",
            piece_name(piece)
        );
    }
}

#[test]
fn the_grid_catches_up_with_the_canvas_within_a_few_frames() {
    // The ant draws only the cells it changes: a canvas changed whole (as its board clearing
    // at SCENE_ANT_RESET changes it) is far more than a frame's budget.
    let mut ui = scene_on(SCENE_ANT);
    let cv = &mut petscii_mut(&mut ui.v.scene).cv;
    for r in 0..ROWS {
        for c in 0..COLS {
            cv.ch[r][c] = FONT_PIP as u8;
            cv.attr[r][c] = attr(GRID_CYAN, GRID_BLACK);
        }
    }
    let mut frames = 0;
    loop {
        ui.frame();
        let sent = drain(&mut ui.g);
        assert!(sent.cells <= SCENE_VIEW_FRAME_CELLS as usize, "{sent:?}");
        if sent.cells == 0 {
            break;
        }
        frames += 1;
        assert!(frames < 10, "still sending after {frames} frames");
    }
    assert_eq!(frames, 5, "900 cells, 200 a frame");
    assert!(grid_is_canvas(&ui));
}

/// Whatever the grid held (another view, the update's frame: screen_redraw), the canvas comes
/// back whole within a few frames, each inside the budget.
#[test]
fn every_cell_comes_back_when_the_screen_asks() {
    for piece in 0..SCENE_PIECES {
        let mut ui = scene_on(piece);
        for i in 0..COLS * ROWS {
            unsafe { grid_put(&mut *ui.g, (i % COLS) as u32, (i / COLS) as u32, b'X', 0x52) };
        }
        drain(&mut ui.g); // on the panel
        unsafe { screen_redraw(&mut ui.v.s) };
        let mut frames = 0;
        while !grid_is_canvas(&ui) {
            ui.frame();
            let sent = drain(&mut ui.g);
            assert!(sent.cells <= SCENE_VIEW_FRAME_CELLS as usize, "{sent:?}");
            frames += 1;
            assert!(frames <= 6, "{}: the canvas is not back", piece_name(piece));
        }
    }
}

/// The wipe into SCENE, and from one piece to the next: the piece holds still under it.
#[test]
fn the_wipes_into_scene_send_only_their_rows() {
    for piece in 0..SCENE_PIECES {
        let mut ui = scene_on(piece);
        ui.press(PANEL_BTN_HOME);
        ui.settle();
        drain(&mut ui.g);
        ui.rig.note_on(60);
        ui.turn(PANEL_ENC_ALGORITHM, 1);
        let frames = wipe_frames(&mut || {
            ui.sc.env = [255; 3];
            ui.frame();
            (drain(&mut ui.g), ui.v.s.wipe != 0)
        });
        assert_wipe_holds_still(&format!("into {}", piece_name(piece)), &frames);
        ui.turn(PANEL_ENC_SELECT, 1);
        let frames = wipe_frames(&mut || {
            ui.sc.env = [255; 3];
            ui.frame();
            (drain(&mut ui.g), ui.v.s.wipe != 0)
        });
        assert_wipe_holds_still(&format!("from {}", piece_name(piece)), &frames);
    }
}

/// MIDI learn's frame (SAVE) over each piece and over LIVE while the band plays: the frame
/// comes in at once, over a view that composes every frame within its budget, so the view
/// leaves the frame's cells alone and counts what the frame sends (app/screen.h
/// screen_cover); no frame sends more than the budget, and the frame shows whole.
#[test]
fn learns_frame_over_every_piece_and_over_live_stays_within_the_budget() {
    for piece in 0..=SCENE_PIECES {
        let (mut ui, name) = if piece < SCENE_PIECES {
            (scene_on(piece), piece_name(piece))
        } else {
            let mut ui = Ui::new();
            ui.turn(PANEL_ENC_K1, 64);
            ui.settle();
            drain(&mut ui.g);
            (ui, "LIVE".to_string())
        };
        let mut ml = Box::<midi_learn>::default();
        unsafe {
            midi_learn_init(&mut *ml);
            views_learn(&mut *ui.v, &mut *ml);
        }
        let mut band = Band::new();
        for _ in 0..20 {
            band.frame(&mut ui, true);
        }
        ui.learn_on(PANEL_ENC_K1);
        let sent: Vec<usize> = (0..60).map(|_| band.frame(&mut ui, true).cells).collect();
        assert_eq!(ui.top(), "LEARN", "{name}");
        let most = sent.iter().copied().max().unwrap();
        println!(
            "learn over {name}: at most {most} cells; the first frames {:?}",
            &sent[..6]
        );
        assert!(
            most <= SCENE_VIEW_FRAME_CELLS as usize && most <= LIVE_VIEW_FRAME_CELLS as usize,
            "learn over {name}: a frame sent {most} cells"
        );
        assert!(
            ui.big(LEARN_VIEW_ROW as usize + 2, 1, 2, 14).trim_end() == "SEND A CC",
            "learn over {name}: {:?}",
            ui.big(LEARN_VIEW_ROW as usize + 2, 1, 2, 14)
        );
    }
}

/// A toast's text is the toast's while it shows: the copy spends none of the budget on the
/// cells under it, so a canvas changed whole catches up as fast as without it.
#[test]
fn the_copy_leaves_a_showing_toast_alone() {
    let mut ui = scene_on(SCENE_ANT);
    let text = c"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let n = text.to_bytes().len();
    unsafe { screen_toast(&mut ui.v.s, text.as_ptr(), 4000) };
    ui.frame();
    drain(&mut ui.g);
    let cv = &mut petscii_mut(&mut ui.v.scene).cv;
    for r in 0..ROWS {
        for c in 0..COLS {
            cv.ch[r][c] = FONT_PIP as u8;
            cv.attr[r][c] = attr(GRID_CYAN, GRID_BLACK);
        }
    }
    let mut sent = Vec::new();
    loop {
        ui.frame();
        let s = drain(&mut ui.g);
        if s.cells == 0 {
            break;
        }
        sent.push(s.cells);
        assert!(sent.len() < 10, "still sending: {sent:?}");
    }
    let toast_row = SCREEN_TOAST_ROW as usize;
    let toast_col = SCREEN_TOAST_COL as usize;
    let cv = &petscii(&ui.v.scene).cv;
    let under_toast =
        |r: usize, c: usize| r == toast_row && (toast_col..toast_col + n).contains(&c);
    assert!(
        (0..ROWS * COLS).all(|i| {
            let (r, c) = (i / COLS, i % COLS);
            under_toast(r, c) || (ui.g.ch[i] == cv.ch[r][c] && ui.g.attr[i] == cv.attr[r][c])
        }),
        "the canvas did not catch up"
    );
    assert!(
        sent[..sent.len() - 1]
            .iter()
            .all(|&c| c == SCENE_VIEW_FRAME_CELLS as usize),
        "every frame but the last spends the whole budget: {sent:?}"
    );
    assert_eq!(
        ui.row(toast_row)[toast_col..toast_col + n],
        *"ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    );
}

/// SELECT, and the menu's ALGORITHM on SCENE, only pick the piece: it starts in the compose of
/// the wipe's first frame (ui_poll's), not in the input's call (ui_encoder's, in the same
/// main-loop pass as that ui_poll), and that frame reveals the start's top rows.
#[test]
fn a_piece_starts_in_the_wipes_first_frame_not_in_the_input() {
    let mut ui = scene_on(SCENE_FIRE);
    let before = petscii(&ui.v.scene).cv;
    ui.turn(PANEL_ENC_SELECT, 1);
    assert_eq!(u32::from(ui.v.scene.which), SCENE_PLASMA);
    assert_eq!(
        u32::from(petscii(&ui.v.scene).piece),
        SCENE_FIRE,
        "started in the input"
    );
    assert_eq!(
        changed(&petscii(&ui.v.scene).cv, &before),
        0,
        "drawn in the input"
    );
    ui.frame();
    assert_eq!(u32::from(petscii(&ui.v.scene).piece), SCENE_PLASMA);
    let mut fresh = Box::<scene>::default();
    unsafe { scene_start(&mut *fresh, SCENE_PLASMA, &ui.v.scene.music) };
    for r in 0..SCREEN_WIPE_ROWS as usize {
        let shown: Vec<(u8, u8)> = (0..COLS)
            .map(|c| (ui.g.ch[r * COLS + c], ui.g.attr[r * COLS + c]))
            .collect();
        let start: Vec<(u8, u8)> = (0..COLS)
            .map(|c| (fresh.cv.ch[r][c], fresh.cv.attr[r][c]))
            .collect();
        assert_eq!(shown, start, "row {r}");
    }
    ui.settle();
    // From the menu.
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    let before = petscii(&ui.v.scene).cv;
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.top(), "SCENE");
    assert_eq!(
        changed(&petscii(&ui.v.scene).cv, &before),
        0,
        "drawn in the input"
    );
    ui.frame();
    unsafe { scene_start(&mut *fresh, SCENE_PLASMA, &ui.v.scene.music) };
    assert_eq!(petscii(&ui.v.scene).cv.ch[..4], fresh.cv.ch[..4]);
}

/// SYNC LEAD makes sound on voice 1 alone (voices 2 and 3 have no waveform: they clock the
/// sync): SCENE hears them as silent, so their parts of a piece stay still while the band
/// plays, and voice 1's moves.
#[test]
fn sync_leads_silent_voices_stop_their_parts() {
    let sync = (0..4u8)
        .find(|&i| crate::preset_name(crate::preset(i)) == "SYNC LEAD")
        .expect("a SYNC LEAD preset");
    for piece in [SCENE_FIRE, SCENE_ANT] {
        let mut ui = scene_on(piece);
        ui.rig.set_preset(sync);
        let mut band = Band::new();
        let ants = unsafe { petscii(&ui.v.scene).u.ant.a };
        let mut heard = [0u32; 3];
        for _ in 0..200 {
            band.frame(&mut ui, true);
            let m = &ui.v.scene.music;
            for (v, h) in heard.iter_mut().enumerate() {
                *h += u32::from(m.gate[v]) + u32::from(m.env[v]) + u32::from(m.trig[v]);
            }
        }
        assert!(heard[0] > 0, "{}: voice 1 is not heard", piece_name(piece));
        assert_eq!(
            heard[1..],
            [0, 0],
            "{}: voices 2 and 3 heard",
            piece_name(piece)
        );
        if piece == SCENE_FIRE {
            let s = petscii(&ui.v.scene);
            assert!(flame_height(s, 0) > 0, "voice 1's flame is out");
            assert_eq!((flame_height(s, 1), flame_height(s, 2)), (0, 0));
        } else {
            let now = unsafe { petscii(&ui.v.scene).u.ant.a };
            assert_ne!(
                (now[0].x, now[0].y),
                (ants[0].x, ants[0].y),
                "voice 1's ant stood still"
            );
            for v in 1..3 {
                assert_eq!(
                    (now[v].x, now[v].y),
                    (ants[v].x, ants[v].y),
                    "ant {v} walked"
                );
            }
        }
    }
}
