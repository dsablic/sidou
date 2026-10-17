// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! RED MOON RITE on the device's side (app/scene_rmr.h): its state machine frame by frame
//! against the script's (tests/golden/scene/rmr-trace.tsv, written by
//! assets/scene/red-moon-rite.js), and its layers against the script's golden frames.

use std::path::PathBuf;

use sidou_sys::*;
use sidou_tests::font::repo_root;
use sidou_tests::scene_art;

const W: usize = 240;
const POSE_NAMES: [&str; 9] = [
    "guard", "guard2", "raise", "chop", "block", "spinA", "spinB", "hit", "reach",
];

fn golden_dir() -> PathBuf {
    repo_root().join("tests/golden/scene")
}

/// A frame of the trace: the steps it took, the cutoff, and the state after it, as the
/// script prints them.
struct TraceFrame {
    frame: u32,
    steps: Vec<scene_rmr_beat>,
    cutoff: u32,
    state: String,
}

fn beat_of(step: &str) -> scene_rmr_beat {
    let f: Vec<&str> = step.split('.').collect();
    let flags = f[2];
    let mut hits = 0;
    for (c, h) in [
        ('k', SCENE_RMR_KICK),
        ('s', SCENE_RMR_SNARE),
        ('h', SCENE_RMR_HAT),
        ('a', SCENE_RMR_ARP),
    ] {
        if flags.contains(c) {
            hits |= h;
        }
    }
    // The prototype's bass: 50 (D3) and up gives the right barbarian the lead.
    let bass = match f[3] {
        "-" => SCENE_RMR_BASS_NONE,
        n if n.parse::<u32>().unwrap() >= SCENE_SONG_BASS_SPLIT => SCENE_RMR_BASS_HIGH,
        _ => SCENE_RMR_BASS_LOW,
    };
    scene_rmr_beat {
        row: 1,
        s: f[0].parse().unwrap(),
        bar: f[1].parse().unwrap(),
        rpb: 4,
        hits: hits as u8,
        bass: bass as u8,
    }
}

fn trace() -> Vec<TraceFrame> {
    let text = std::fs::read_to_string(golden_dir().join("rmr-trace.tsv")).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let w: Vec<&str> = l.split(' ').collect();
            let steps = if w[1] == "-" {
                Vec::new()
            } else {
                w[1].split(',').map(beat_of).collect()
            };
            TraceFrame {
                frame: w[0].parse().unwrap(),
                steps,
                cutoff: w[2].parse().unwrap(),
                state: w[3..].join(" "),
            }
        })
        .collect()
}

/// The device's state printed as the script prints it.
fn state_of(r: &scene_rmr) -> String {
    let pose = |k: usize| POSE_NAMES[usize::from(r.pose[k])];
    let next = if r.next == 0 {
        "-".to_string()
    } else {
        POSE_NAMES[usize::from(r.next - 1)].to_string()
    };
    let (mut gx, mut gy, mut k) = (0i32, 0i32, 0u32);
    let sword = if unsafe { scene_rmr_sword_at(r, &mut gx, &mut gy, &mut k) } != 0 {
        let t = unsafe { scene_rmr_flights[r.unarmed as usize].frames };
        format!("{}/{t}/{gx}/{gy}/{k}", r.sword_n)
    } else {
        "-".to_string()
    };
    let sky: String = r.sky.iter().map(|p| p.to_string()).collect();
    format!(
        "{} {} {},{} {},{} {},{} {} {} {} {} {} {} {next} {sword} {sky}",
        r.x,
        r.lead,
        pose(0),
        pose(1),
        r.t[0],
        r.t[1],
        r.hp[0],
        r.hp[1],
        r.unarmed,
        r.spark,
        r.spin,
        r.spin_who,
        r.shake,
        r.flare
    )
}

fn played(upto: u32) -> Box<scene_rmr> {
    let mut r = Box::<scene_rmr>::default();
    unsafe { scene_rmr_init(&mut *r) };
    for f in trace().iter().take_while(|f| f.frame <= upto) {
        for b in &f.steps {
            unsafe { scene_rmr_step(&mut *r, b) };
        }
        unsafe { scene_rmr_frame(&mut *r, f.cutoff, 1) };
    }
    r
}

#[test]
fn the_vignette_plays_the_script_frame_for_frame() {
    let t = trace();
    assert!(t.len() >= 700, "{} frames", t.len());
    let mut r = Box::<scene_rmr>::default();
    unsafe { scene_rmr_init(&mut *r) };
    let (mut disarms, mut stuck, mut reaches, mut parries) = (0, 0, 0, 0);
    let mut was = state_of(&r);
    for f in &t {
        for b in &f.steps {
            unsafe { scene_rmr_step(&mut *r, b) };
        }
        unsafe { scene_rmr_frame(&mut *r, f.cutoff, 1) };
        let now = state_of(&r);
        assert_eq!(
            now, f.state,
            "frame {}: the device (left) and the script (right); the frame before: {was}",
            f.frame
        );
        disarms += usize::from(r.sword != 0 && r.sword_n == 1);
        stuck += usize::from(r.sword != 0 && now.contains("/8 "));
        reaches +=
            usize::from(r.unarmed >= 0 && u32::from(r.pose[r.unarmed as usize]) == SCENE_RMR_REACH);
        parries += usize::from(r.spark == 2);
        was = now;
    }
    println!(
        "{disarms} disarms, {stuck} frames stuck, {reaches} frames reaching, {parries} parries"
    );
    // Three passes of the four bars: a disarm each, the sword stuck a while, pulled free (the
    // reach holds 8 frames, the last of them back in guard).
    assert_eq!(disarms, 3);
    assert!(stuck > 10 && reaches == 3 * 7 && parries > 10);
}

#[test]
fn the_sky_takes_the_cutoffs_palette() {
    let of = |c: u32| unsafe { scene_rmr_sky_of(c) };
    // The prototype's 0.36 and 0.68 of the cutoff's range.
    assert_eq!(
        (of(0), of(736), of(737), of(1391), of(1392), of(2047)),
        (0, 0, 1, 1, 2, 2)
    );
    let mut r = Box::<scene_rmr>::default();
    unsafe { scene_rmr_init(&mut *r) };
    for f in 0..15 {
        unsafe { scene_rmr_frame(&mut *r, 2047, 1) };
        assert_eq!(u32::from(r.sky_row), 2 + f, "a row a frame, from the top");
    }
    unsafe { scene_rmr_frame(&mut *r, 2047, 1) };
    assert_eq!(r.sky_row, 0xff, "wiped");
    assert!(r.sky.iter().all(|&p| p == 2));
}

/// A screen composited from scene_px's slots, the way the panel gets it.
struct Rig {
    /// The grid the compositor's region is on, kept alive with it.
    _g: Box<grid>,
    sp: Box<scene_px>,
}

impl Rig {
    fn new() -> Rig {
        let mut g = Box::<grid>::default();
        let mut sp = Box::<scene_px>::default();
        unsafe {
            grid_init(&mut *g, b' ', 0);
            scene_px_init(&mut *sp, &mut *g);
        }
        Rig { _g: g, sp }
    }

    fn screen(&self) -> Vec<u8> {
        let mut px = vec![0u8; W * W];
        for t in 0..900usize {
            let mut tile = [0u8; 64];
            unsafe {
                scene_px_tile(
                    &*self.sp,
                    (t % 30) as u32,
                    (t / 30) as u32,
                    tile.as_mut_ptr(),
                )
            };
            for y in 0..8 {
                let at = ((t / 30) * 8 + y) * W + (t % 30) * 8;
                px[at..at + 8].copy_from_slice(&tile[y * 8..y * 8 + 8]);
            }
        }
        px
    }
}

/// The torches' boxes (their cycling and flare are the device's own) and the status bar (the
/// script's goldens leave both out).
fn masked(x: usize, y: usize) -> bool {
    let f = unsafe { scene_rmr_flames };
    y < 16
        || f.iter().any(|b| {
            let (bx, by, bw, bh) = (
                usize::from(b[0]),
                usize::from(b[1]),
                usize::from(b[2]),
                usize::from(b[3]),
            );
            x >= bx && x < bx + bw && y + 14 >= by && y < by + bh
        })
}

#[test]
fn the_layers_draw_the_scripts_golden_frames() {
    let names = ["guard", "parry", "spin", "flight", "stuck", "reach", "wipe"];
    for name in names {
        let text = std::fs::read_to_string(golden_dir().join(format!("rmr-{name}.txt"))).unwrap();
        let frame: u32 = text
            .lines()
            .next()
            .and_then(|l| l.split("frame ").nth(1))
            .and_then(|l| l.split(' ').next())
            .unwrap()
            .parse()
            .unwrap();
        let png = scene_art::parse_png(
            &std::fs::read(golden_dir().join(format!("rmr-{name}.png"))).unwrap(),
        )
        .unwrap();
        let mut r = played(frame);
        let mut rig = Rig::new();
        unsafe { scene_rmr_draw(&mut *r, &mut *rig.sp) };
        let screen = rig.screen();
        let mut off = Vec::new();
        for y in 0..W {
            for x in 0..W {
                if !masked(x, y) && screen[y * W + x] != png.px[y * W + x] {
                    off.push((x, y));
                }
            }
        }
        // The spin's arc is the device's sine (app/sine.h), not the script's: within a pixel.
        let arc = if name == "spin" {
            2 * SCENE_RMR_ARC_MARKS as usize
        } else {
            0
        };
        println!("{name} (frame {frame}): {} pixels off", off.len());
        assert!(
            off.len() <= arc,
            "{name}: {} pixels differ from the script's, the first {:?}",
            off.len(),
            &off[..off.len().min(8)]
        );
    }
}

/// A tile of the view's compositor, as the grid will paint it.
fn view_tile(ui: &crate::views::Ui, col: usize, row: usize) -> [u8; 64] {
    let mut tile = [0u8; 64];
    let sp = unsafe { &ui.v.scene.u.rite.px };
    unsafe { scene_px_tile(sp, col as u32, row as u32, tile.as_mut_ptr()) };
    tile
}

/// SELECT reaches RED MOON RITE (its toast); it shows over the compositor's painter region, its
/// cells blank, and what the screen draws over it (the toast, MIDI learn's frame) keeps its own
/// background; SELECT on to FIRE gives the region back.
#[test]
fn red_moon_rite_is_scenes_sixth_piece() {
    use crate::views::Ui;
    let mut ui = Ui::without_cards();
    crate::scene::open_scene(&mut ui);
    ui.turn(PANEL_ENC_SELECT, -1);
    assert_eq!(u32::from(ui.v.scene.which), SCENE_RMR);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("RED MOON RITE"));
    ui.settle();
    crate::drain(&mut ui.g);
    let sp: *const scene_px = unsafe { &ui.v.scene.u.rite.px };
    assert_eq!(ui.g.region_ctx.cast_const(), sp.cast());
    // The toast's cells: their background under the glyphs.
    let (c, r) = (SCREEN_TOAST_COL as usize, SCREEN_TOAST_ROW as usize);
    let bg = ui.g.attr[r * 30 + c] >> 4;
    assert_ne!(ui.g.ch[r * 30 + c], b' ');
    ui.frame();
    assert!(
        view_tile(&ui, c, r).iter().all(|&p| p == bg),
        "the toast's background"
    );
    for _ in 0..40 {
        ui.frame();
        crate::drain(&mut ui.g);
    }
    assert!(ui.toast().is_none());
    assert!(
        view_tile(&ui, c, r).iter().any(|&p| p != bg),
        "the floor under the toast's row once it is gone"
    );
    // MIDI learn's frame over it: the frame's cells keep the frame's background.
    let mut ml = Box::<midi_learn>::default();
    unsafe {
        midi_learn_init(&mut *ml);
        views_learn(&mut *ui.v, &mut *ml);
    }
    ui.learn_on(PANEL_ENC_K1);
    ui.frame();
    ui.frame();
    assert_eq!(ui.top(), "LEARN");
    assert_eq!(
        ui.g.region_ctx.cast_const(),
        sp.cast(),
        "the region stays under the frame"
    );
    let (c, r) = (LEARN_VIEW_COL as usize + 1, LEARN_VIEW_ROW as usize + 1);
    let bg = ui.g.attr[r * 30 + c] >> 4;
    assert!(
        view_tile(&ui, c, r).iter().all(|&p| p == bg),
        "the frame's background"
    );
    ui.press(PANEL_BTN_HOME);
    ui.frame();
    assert_eq!(ui.top(), "SCENE");
    ui.frame();
    assert!(
        view_tile(&ui, c, r).iter().any(|&p| p != bg),
        "the scene back under it"
    );
    // FIRE: the cells' own pixels again.
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.frame();
    assert!(ui.g.region_painter.is_none());
    assert_eq!(u32::from(crate::petscii(&ui.v.scene).piece), SCENE_FIRE);
}
