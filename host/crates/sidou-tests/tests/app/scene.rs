// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

use crate::views::Ui;
use crate::{
    assert_wipe_holds_still, drain, petscii, petscii_mut, wipe_frames, Sent, SPI_BUDGET_MS,
};
use crate::{c_text, cutoff_after};

const COLS: usize = GRID_COLS as usize;
const ROWS: usize = GRID_ROWS as usize;
const BLANK: (u8, u8) = (b' ', 0);

fn new_scene(piece: u32) -> Box<scene> {
    let mut s = Box::<scene>::default();
    unsafe { scene_start(&mut *s, piece, &no_voice_playing()) };
    s
}

fn step(s: &mut scene, m: &scene_music) {
    unsafe { scene_step(s, m) };
}

fn no_voice_playing() -> scene_music {
    scene_music {
        gate: [0; 3],
        trig: [0; 3],
        note: [SCENE_NO_NOTE as u8; 3],
        env: [0; 3],
        cutoff: 1200,
        resonance: 6,
    }
}

fn held(mut m: scene_music, v: usize, note: u8, env: u8) -> scene_music {
    m.gate[v] = 1;
    m.note[v] = note;
    m.env[v] = env;
    m
}

fn note_on(m: scene_music, v: usize, note: u8, env: u8) -> scene_music {
    let mut m = held(m, v, note, env);
    m.trig[v] = 1;
    m
}

fn cell(s: &scene, c: usize, r: usize) -> (u8, u8) {
    (s.cv.ch[r][c], s.cv.attr[r][c])
}

fn cells_changed(a: &scene_canvas, b: &scene_canvas) -> usize {
    (0..ROWS)
        .flat_map(|r| (0..COLS).map(move |c| (r, c)))
        .filter(|&(r, c)| a.ch[r][c] != b.ch[r][c] || a.attr[r][c] != b.attr[r][c])
        .count()
}

fn attr(fg: u32, bg: u32) -> u8 {
    ((bg << 4) | fg) as u8
}

fn fire(s: &scene) -> scene_fire {
    unsafe { s.u.fire }
}

const FIRE_TOP: usize = ROWS - SCENE_FIRE_ROWS as usize;
const FIRE_COLORS: [u32; 6] = [
    GRID_BLACK,
    GRID_BROWN,
    GRID_RED,
    GRID_LIGHT_RED,
    GRID_YELLOW,
    GRID_WHITE,
];
const FIRE_WHITE: usize = 5;
const FIRE_IDLE_HOT: u8 = 120;

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

fn flame_height_in_heat_rows(s: &scene, v: usize) -> usize {
    let lv = fire_levels(s);
    let h = lv.len();
    (0..h)
        .find(|&y| (10 * v..10 * v + 10).any(|x| lv[y][x] >= 3))
        .map_or(0, |y| h - y)
}

#[test]
fn the_fire_burns_in_its_rows_with_each_voices_envelope() {
    let mut s = new_scene(SCENE_FIRE);
    let check_above = |s: &scene| {
        for r in 0..FIRE_TOP {
            for c in 0..COLS {
                assert_eq!(cell(s, c, r), BLANK, "cell ({c}, {r})");
            }
        }
    };
    check_above(&s);
    for _ in 0..60 {
        step(&mut s, &no_voice_playing());
    }
    assert_eq!(
        (0..3).map(|v| flame_height_in_heat_rows(&s, v)).max(),
        Some(0)
    );
    let m = held(no_voice_playing(), 1, 60, 255);
    for _ in 0..60 {
        step(&mut s, &m);
    }
    check_above(&s);
    let h = [0, 1, 2].map(|v| flame_height_in_heat_rows(&s, v));
    assert!(h[1] >= 8, "voice 2's flame: {h:?}");
    assert_eq!((h[0], h[2]), (0, 0), "{h:?}");
    let mut tall = new_scene(SCENE_FIRE);
    let mut short = new_scene(SCENE_FIRE);
    let (mut sum_tall, mut sum_short) = (0, 0);
    for _ in 0..100 {
        step(&mut tall, &held(no_voice_playing(), 0, 84, 255));
        step(&mut short, &held(no_voice_playing(), 0, 36, 255));
        sum_tall += flame_height_in_heat_rows(&tall, 0);
        sum_short += flame_height_in_heat_rows(&short, 0);
    }
    assert!(
        sum_tall > sum_short * 5 / 4,
        "{sum_tall} against {sum_short}"
    );
    for _ in 0..100 {
        step(&mut s, &no_voice_playing());
    }
    assert_eq!(flame_height_in_heat_rows(&s, 1), 0);
}

#[test]
fn a_silent_fire_keeps_a_low_red_flame_that_never_reaches_light_red() {
    let mut s = new_scene(SCENE_FIRE);
    let (mut flickers, mut tallest) = (0usize, 0usize);
    for f in 0..200 {
        step(&mut s, &no_voice_playing());
        if f < 60 {
            continue;
        }
        let lv = fire_levels(&s);
        let red = |y: usize| lv[y].iter().filter(|&&l| l >= 2).count();
        flickers += (0..lv.len() - 1).map(red).sum::<usize>();
        let top = (0..lv.len())
            .find(|&y| red(y) > 0)
            .map_or(0, |y| lv.len() - y);
        tallest = tallest.max(top);
        assert!(lv.iter().flatten().all(|&l| l <= 2), "frame {f}: above red");
    }
    println!("idle fire: {flickers} red half cells above the bed in 140 frames, up to {tallest}");
    let mean = flickers as f64 / (140.0 * COLS as f64);
    assert!(
        (2.0..=10.0).contains(&mean),
        "{mean} red half cells a column"
    );
    assert!(
        (8..=20).contains(&tallest),
        "the idle flame is {tallest} half cells high"
    );
    assert!(
        fire(&s).hot.iter().all(|&h| h == FIRE_IDLE_HOT),
        "{:?}",
        fire(&s).hot
    );
}

#[test]
fn the_cutoff_sets_how_hot_the_fire_burns() {
    let hottest = |cutoff: u16| {
        let mut s = new_scene(SCENE_FIRE);
        let mut most = 0;
        for _ in 0..80 {
            let mut m = held(no_voice_playing(), 0, 60, 255);
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
    let heat = |cutoff: u16| {
        let mut s = new_scene(SCENE_FIRE);
        for _ in 0..40 {
            let mut m = held(no_voice_playing(), 0, 60, 255);
            m.cutoff = cutoff;
            step(&mut s, &m);
        }
        fire(&s).heat
    };
    assert_eq!(heat(0), heat(2047));
}

#[test]
fn the_fire_burns_up_its_ramp_in_tongues() {
    let mut s = new_scene(SCENE_FIRE);
    for _ in 0..30 {
        step(&mut s, &no_voice_playing());
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
        let mut m = no_voice_playing();
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
    let m = held(no_voice_playing(), 0, 60, 255);
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

const PLASMA_RAMP: [u32; 6] = [
    GRID_BLACK,
    GRID_BLUE,
    GRID_PURPLE,
    GRID_LIGHT_BLUE,
    GRID_CYAN,
    GRID_WHITE,
];

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
            let mut m = held(no_voice_playing(), 0, 60, env);
            m.cutoff = cutoff;
            step(&mut s, &m);
            plasma_levels(&s);
            total += cells_changed(&before, &s.cv);
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
        let mut m = held(no_voice_playing(), 0, 60, 255);
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
        step(&mut s, &note_on(no_voice_playing(), 2, note, 200));
        let r = unsafe { s.u.plasma }.rip[2];
        assert_eq!((r.x, r.y, r.age), (x, y, 1), "note {note}");
        let mut plain = new_scene(SCENE_PLASMA);
        step(&mut plain, &held(no_voice_playing(), 2, note, 200));
        for _ in 0..7 {
            step(&mut s, &held(no_voice_playing(), 2, note, 200));
            step(&mut plain, &held(no_voice_playing(), 2, note, 200));
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
    let mut s = new_scene(SCENE_PLASMA);
    step(&mut s, &note_on(no_voice_playing(), 0, 72, 200));
    for _ in 0..SCENE_RIPPLE_FRAMES {
        step(&mut s, &held(no_voice_playing(), 0, 72, 200));
    }
    assert_eq!(
        u32::from(unsafe { s.u.plasma }.rip[0].age),
        SCENE_RIPPLE_FRAMES
    );
}

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
        assert_eq!(cells_changed(&a.cv, &b.cv), 0, "piece {piece}");
        let mut rng = 7u32;
        for f in 0..200 {
            let r = unsafe { scene_random(&mut rng) };
            let m = if r % 5 == 0 {
                note_on(
                    no_voice_playing(),
                    (r % 3) as usize,
                    48 + (r >> 8) as u8 % 36,
                    220,
                )
            } else {
                held(no_voice_playing(), 0, 60, (r >> 16) as u8)
            };
            step(&mut a, &m);
            step(&mut b, &m);
            assert_eq!(cells_changed(&a.cv, &b.cv), 0, "piece {piece}, frame {f}");
        }
        unsafe { scene_start(&mut *a, piece, &no_voice_playing()) };
        assert_eq!(
            cells_changed(&a.cv, &new_scene(piece).cv),
            0,
            "piece {piece}"
        );
    }
    assert_eq!(u32::from(new_scene(SCENE_PETSCII + 1).piece), SCENE_PLASMA);
}

#[test]
fn a_piece_begun_and_filled_by_rows_is_the_piece_started_whole() {
    let mut m = no_voice_playing();
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
                note_on(m, f % 3, 60 + f as u8, 200)
            } else {
                m
            };
            step(&mut part, &mf);
            step(&mut lazy, &mf);
            step(&mut whole, &mf);
            assert_eq!(
                cells_changed(&part.cv, &whole.cv),
                0,
                "piece {piece}, frame {f}"
            );
            assert_eq!(
                cells_changed(&lazy.cv, &whole.cv),
                0,
                "piece {piece}, frame {f}"
            );
        }
    }
}

const INTRO: u32 = SCENE_PIECES;

fn piece_name(k: u32) -> String {
    if k == INTRO {
        return "RED MOON RITE's intro".to_string();
    }
    c_text(unsafe { scene_names[k as usize] })
}

pub(crate) fn open_scene(ui: &mut Ui) {
    ui.v.scene.st = crate::art_store();
    ui.press(PANEL_BTN_HOME);
    ui.choose_menu_item(VIEWS_MENU_SCENE);
    ui.settle();
}

fn grid_is_canvas(ui: &Ui) -> bool {
    if u32::from(ui.v.scene.which) >= SCENE_PETSCII {
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
    let mut ui = Ui::new();
    open_scene(&mut ui);
    assert_eq!(ui.top(), "SCENE");
    assert_eq!(u32::from(ui.v.scene.which), SCENE_FIRE);
    assert_eq!(u32::from(petscii(&ui.v.scene).piece), SCENE_FIRE);
    assert_eq!(ui.toast().as_deref(), Some("FIRE"), "the name on entry");
    ui.frames_sent(40);
    assert!(ui.toast().is_none());
    assert!(grid_is_canvas(&ui), "the grid is not the fire's canvas");
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
        if k % SCENE_PIECES >= SCENE_PETSCII {
            ui.frames_sent(40);
            assert!(
                grid_is_canvas(&ui),
                "{}: the painter's cells are not blank",
                piece_name(k % SCENE_PIECES)
            );
        }
    }
    ui.turn(PANEL_ENC_SELECT, -(SCENE_PIECES as i32) - 1);
    assert_eq!(u32::from(ui.v.scene.which), SCENE_PIECES - 1);
    let cutoff = ui.live().cutoff;
    ui.turn(PANEL_ENC_K1, 3);
    assert_eq!(ui.live().cutoff, cutoff_after(ui.live().model, cutoff, 3));
    assert!(ui.live().cutoff > cutoff);
    ui.press(PANEL_BTN_OCT_UP);
    assert_eq!(ui.live().octave, 1);
    ui.press(PANEL_BTN_HOME);
    assert_eq!(ui.top(), "MENU");
    ui.settle();
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    ui.settle();
    assert_eq!(ui.top(), "SCENE");
    assert_eq!(u32::from(ui.v.scene.which), SCENE_PIECES - 1);
    assert_eq!(
        unsafe { ui.v.scene.u.rite.which },
        ui.v.scene.which,
        "the menu reopens the piece it left"
    );
}

#[test]
fn algorithm_steps_the_piece_as_select_does() {
    let step_with = |enc: u32, steps: i32| {
        let mut ui = Ui::new();
        open_scene(&mut ui);
        ui.frames(40);
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

#[test]
fn entering_scene_names_the_piece_once() {
    let mut ui = Ui::new();
    open_scene(&mut ui);
    assert_eq!(ui.toast().as_deref(), Some("FIRE"));
    assert_eq!(ui.v.scene.say, 0);
    ui.frames(60);
    assert!(ui.toast().is_none(), "the toast ends and is not said again");
}

#[test]
fn the_tap_runs_while_scene_shows() {
    let mut ui = Ui::new();
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
    let _ml = ui.attach_learn();
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

const TAP_POINTS_A_FRAME: u32 = 60;
const SID_CYCLES_A_TAP_POINT: u32 = 328;

pub(crate) fn feed_the_tap(sc: &mut scope, acc: &mut [u32; 3], freq: [u16; 3]) {
    for _ in 0..TAP_POINTS_A_FRAME {
        let p = &mut sc.point[(sc.head % SCOPE_RING) as usize];
        for v in 0..3 {
            acc[v] = (acc[v] + u32::from(freq[v]) * SID_CYCLES_A_TAP_POINT) & 0xff_ffff;
            p.phase[v] = (acc[v] >> 16) as u8;
            p.wave[v] = p.phase[v];
        }
        sc.head = sc.head.wrapping_add(1);
    }
}

struct Band {
    env: [u8; 3],
    acc: [u32; 3],
    frame: u32,
    notes_and_their_end_frames: Vec<(u8, u32)>,
}

impl Band {
    fn new() -> Self {
        Band {
            env: [0; 3],
            acc: [0; 3],
            frame: 0,
            notes_and_their_end_frames: Vec::new(),
        }
    }

    fn frame(&mut self, ui: &mut Ui, playing: bool) -> Sent {
        const CHORDS: [[u8; 3]; 4] = [[57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]];
        let f = self.frame;
        self.frame += 1;
        let ch = CHORDS[(f / 64 % 4) as usize];
        let mut on = |note: u8, len: u32| {
            ui.rig.note_on(note);
            self.notes_and_their_end_frames.push((note, f + len));
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
        for &(note, end) in &self.notes_and_their_end_frames {
            if end == f || !playing {
                ui.rig.note_off(note);
            }
        }
        self.notes_and_their_end_frames
            .retain(|&(_, end)| playing && end > f);
        for v in 0..3 {
            let gate = ui.live().voice[v].gate != 0;
            self.env[v] = if gate {
                self.env[v].saturating_add(96).min(230)
            } else {
                (u32::from(self.env[v]) * 3 / 4) as u8
            };
            ui.sc.env[v] = self.env[v];
        }
        let freq = std::array::from_fn(|v| ui.live().voice[v].freq_out);
        feed_the_tap(&mut ui.sc, &mut self.acc, freq);
        ui.frame();
        drain(&mut ui.g)
    }
}

fn scene_on(piece: u32) -> Ui {
    let mut ui = Ui::new();
    open_scene(&mut ui);
    if piece == INTRO {
        ui.v.scene.st = std::ptr::null();
    }
    for _ in 0..piece.min(SCENE_RMR) {
        ui.turn(PANEL_ENC_SELECT, 1);
        ui.settle();
    }
    let cutoff_wide_open_for_the_busiest_plasma_and_hottest_fire = 64;
    ui.turn(
        PANEL_ENC_K1,
        cutoff_wide_open_for_the_busiest_plasma_and_hottest_fire,
    );
    ui.frames_sent(80);
    ui.v.scene.held = 0;
    ui
}

#[test]
fn every_piece_stays_within_the_budget_while_the_band_plays() {
    for piece in 0..=INTRO {
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
        assert!(avg >= 2.0, "{}: {avg:.1} cells a frame", piece_name(piece));
    }
}

#[test]
fn a_note_moves_the_scene_and_silence_calms_it() {
    for piece in 0..=INTRO {
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
        let fires_idle_flame_moves_a_third_as_much = piece == SCENE_FIRE;
        let times = if fires_idle_flame_moves_a_third_as_much {
            2
        } else {
            3
        };
        assert!(
            playing > times * quiet.max(10) && playing > times * calmed.max(10),
            "{}: quiet {quiet}, playing {playing}, after {calmed}",
            piece_name(piece)
        );
    }
}

fn pips_above_the_fire(ui: &mut Ui) -> usize {
    let cv = &mut petscii_mut(&mut ui.v.scene).cv;
    for r in 0..FIRE_TOP {
        for c in 0..COLS {
            cv.ch[r][c] = FONT_PIP as u8;
            cv.attr[r][c] = attr(GRID_CYAN, GRID_BLACK);
        }
    }
    FIRE_TOP * COLS
}

#[test]
fn the_grid_catches_up_with_the_canvas_within_a_few_frames() {
    let mut ui = scene_on(SCENE_FIRE);
    let pips = pips_above_the_fire(&mut ui);
    let mut sent = Vec::new();
    while !grid_is_canvas(&ui) || sent.is_empty() {
        ui.frame();
        let s = drain(&mut ui.g);
        assert!(s.cells <= SCENE_VIEW_FRAME_CELLS as usize, "{s:?}");
        sent.push(s.cells);
        assert!(sent.len() < 10, "still behind after {sent:?}");
    }
    assert_eq!(
        sent.len(),
        pips.div_ceil(SCENE_VIEW_FRAME_CELLS as usize),
        "{pips} cells, 200 a frame: {sent:?}"
    );
}

#[test]
fn every_cell_comes_back_when_the_screen_asks() {
    for piece in 0..SCENE_PIECES {
        let mut ui = scene_on(piece);
        for i in 0..COLS * ROWS {
            unsafe { grid_put(&mut *ui.g, (i % COLS) as u32, (i / COLS) as u32, b'X', 0x52) };
        }
        drain(&mut ui.g);
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

#[test]
fn learns_frame_over_every_piece_and_over_live_stays_within_the_budget() {
    for piece in 0..=INTRO + 1 {
        let (mut ui, name) = if piece <= INTRO {
            (scene_on(piece), piece_name(piece))
        } else {
            let mut ui = Ui::new();
            ui.turn(PANEL_ENC_K1, 64);
            ui.settle();
            drain(&mut ui.g);
            (ui, "LIVE".to_string())
        };
        let _ml = ui.attach_learn();
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

#[test]
fn the_copy_leaves_a_showing_toast_alone() {
    let mut ui = scene_on(SCENE_FIRE);
    let text = c"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let n = text.to_bytes().len();
    unsafe { screen_toast(&mut ui.v.s, text.as_ptr(), 4000) };
    ui.frame();
    drain(&mut ui.g);
    pips_above_the_fire(&mut ui);
    let toast_row = SCREEN_TOAST_ROW as usize;
    let toast_col = SCREEN_TOAST_COL as usize;
    let under_toast =
        |r: usize, c: usize| r == toast_row && (toast_col..toast_col + n).contains(&c);
    let caught_up = |ui: &Ui| {
        let cv = &petscii(&ui.v.scene).cv;
        (0..ROWS * COLS).all(|i| {
            let (r, c) = (i / COLS, i % COLS);
            under_toast(r, c) || (ui.g.ch[i] == cv.ch[r][c] && ui.g.attr[i] == cv.attr[r][c])
        })
    };
    let mut sent = Vec::new();
    while !caught_up(&ui) || sent.is_empty() {
        ui.frame();
        sent.push(drain(&mut ui.g).cells);
        assert!(sent.len() < 10, "still sending: {sent:?}");
    }
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
        cells_changed(&petscii(&ui.v.scene).cv, &before),
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
    ui.press(PANEL_BTN_HOME);
    ui.settle();
    let before = petscii(&ui.v.scene).cv;
    ui.turn(PANEL_ENC_ALGORITHM, 1);
    assert_eq!(ui.top(), "SCENE");
    assert_eq!(
        cells_changed(&petscii(&ui.v.scene).cv, &before),
        0,
        "drawn in the input"
    );
    ui.frame();
    unsafe { scene_start(&mut *fresh, SCENE_PLASMA, &ui.v.scene.music) };
    assert_eq!(petscii(&ui.v.scene).cv.ch[..4], fresh.cv.ch[..4]);
}

#[test]
fn sync_leads_silent_voices_stop_their_parts() {
    let sync = (0..4u8)
        .find(|&i| crate::preset_name(crate::preset(i)) == "SYNC LEAD")
        .expect("a SYNC LEAD preset");
    let mut ui = scene_on(SCENE_FIRE);
    ui.rig.set_preset(sync);
    let mut band = Band::new();
    let mut heard = [0u32; 3];
    for _ in 0..200 {
        band.frame(&mut ui, true);
        let m = &ui.v.scene.music;
        for (v, h) in heard.iter_mut().enumerate() {
            *h += u32::from(m.gate[v]) + u32::from(m.env[v]) + u32::from(m.trig[v]);
        }
    }
    assert!(heard[0] > 0, "voice 1 is not heard");
    assert_eq!(heard[1..], [0, 0], "voices 2 and 3 heard");
    let s = petscii(&ui.v.scene);
    assert!(
        flame_height_in_heat_rows(s, 0) > 0,
        "voice 1's flame is out"
    );
    assert_eq!(
        (
            flame_height_in_heat_rows(s, 1),
            flame_height_in_heat_rows(s, 2)
        ),
        (0, 0)
    );
}
